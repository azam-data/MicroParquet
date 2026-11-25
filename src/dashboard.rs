//! Dashboards (Phase 10 D0): the model a project saves, and the layout maths of its grid.
//!
//! A dashboard belongs to the open project and is written in the `.mpproj` (`project_version` 3, only when a dashboard exists).
//! An item **points at** a chart tab or a pivot tab by the tab's project key (`t1`, `t2`, …) and never holds a copy of its
//! spec, so a dashboard follows its tabs. A tab that was deleted shows "Source tab removed" on its item and in every export.
//! Links use ids, never names, so renaming a page, an item or a filter breaks nothing.

use crate::kpi::KpiSpec;
use crate::project::Project;
use crate::chart::{ChartData, ChartSpec};
use crate::models::*;
use anyhow::{bail, Context, Result};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A page is 1000 × 625 units (16:10) unless its template says otherwise.
pub const PAGE_W: f64 = 1000.0;
pub const PAGE_H: f64 = 625.0;
/// The gap between two items of a grid, in page units.
pub const GRID_GAP: f64 = 10.0;
/// Free layout snaps to this many page units.
pub const SNAP: f64 = 10.0;
/// A page has at most this many filters.
pub const MAX_PAGE_FILTERS: usize = 4;
/// A grid has 1 to this many items per row.
pub const MAX_PER_ROW: usize = 4;
/// What an item shows when the tab it points at is gone.
pub const SOURCE_REMOVED: &str = "Source tab removed";

/// A design the owner imported: its name in `portable_data/dashboard_templates/` and the blake3 of the copy kept in the project (D5).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct TemplateRef {
    pub name: String,
    pub blake3: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Dashboard {
    pub id: String,
    pub title: String,
    pub pages: Vec<DashPage>,
    pub template_library: Vec<TemplateRef>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum PageLayout {
    /// 1 to 4 items per row, equal widths; an item's place is its order on the page.
    Grid { per_row: usize },
    /// Each item holds its own rect.
    Free,
}

impl Default for PageLayout {
    fn default() -> Self {
        PageLayout::Grid { per_row: 2 }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct DashPage {
    pub id: String,
    pub title: String,
    pub layout: PageLayout,
    pub items: Vec<DashItem>,
    /// At most `MAX_PAGE_FILTERS`.
    pub filters: Vec<PageFilter>,
    pub background: Option<TemplateRef>,
    /// What the reader had picked: saved with the project so reopening shows the same page.
    pub state: PageState,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum DashItemKind {
    /// A chart tab, by project key.
    Chart(String),
    /// A pivot tab, by project key.
    Pivot(String),
    Kpi(KpiSpec),
}

impl Default for DashItemKind {
    fn default() -> Self {
        DashItemKind::Kpi(KpiSpec::default())
    }
}

/// A rectangle in page units.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(default)]
pub struct DashRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct DashItem {
    pub id: String,
    pub kind: DashItemKind,
    /// The item's own rect on a Free page; `None` on a Grid page, where the item's order gives its cell.
    pub rect: Option<DashRect>,
    pub title_override: Option<String>,
}

impl DashItem {
    /// The project key of the tab this item reads, or `None` for a KPI card.
    pub fn source_key(&self) -> Option<&str> {
        match &self.kind {
            DashItemKind::Chart(k) | DashItemKind::Pivot(k) => Some(k),
            DashItemKind::Kpi(_) => None,
        }
    }

    /// "Source tab removed" when the item's tab is not in the project; `None` when it is there (or the item is a KPI card).
    pub fn source_problem(&self, project: &Project) -> Option<&'static str> {
        self.source_key().filter(|k| project.tab(k).is_none()).map(|_| SOURCE_REMOVED)
    }
}

/// A lookup field, Calendar field or data column of one data tab, offered to the reader as a dropdown with "All".
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct PageFilter {
    pub id: String,
    pub label: String,
    /// The project key of the data tab.
    pub tab: String,
    /// The field's name as the pivot and chart code know it ("Branch master · Region", "Open Date · Month-Year", "Branch").
    pub field: String,
    /// The lookup or Calendar field behind `field`; `None` for a column of the data tab.
    pub model: Option<crate::model::ModelFieldRef>,
    /// The reader may pick several values (off by default: one value or "All").
    pub multi: bool,
}

/// A bar, slice or pivot row the reader clicked.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct CrossPick {
    pub dimension: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct PageState {
    /// One entry per page filter: `None` is "All". A value that no longer exists is kept and shown as "(not in data)".
    pub filter_picks: [Option<Vec<String>>; MAX_PAGE_FILTERS],
    pub cross: Option<CrossPick>,
    /// Each item's drill path, by item id.
    pub drill: BTreeMap<String, Vec<String>>,
}

impl PageState {
    /// The "Clear" button: all picks, the cross-filter and every drill go back to the start.
    pub fn clear(&mut self) {
        *self = PageState::default();
    }
}

impl Dashboard {
    /// Checks what the file format cannot: at most 4 filters per page, 1 to 4 items per row, ids that are unique on their page.
    pub fn validate(&self) -> Result<()> {
        for p in &self.pages {
            if p.filters.len() > MAX_PAGE_FILTERS {
                bail!("The page '{}' has {} filters; a page holds at most {}. Remove {}.", p.title, p.filters.len(), MAX_PAGE_FILTERS, p.filters.len() - MAX_PAGE_FILTERS);
            }
            if let PageLayout::Grid { per_row } = p.layout {
                if !(1..=MAX_PER_ROW).contains(&per_row) {
                    bail!("The page '{}' has {} items per row; choose 1 to {}.", p.title, per_row, MAX_PER_ROW);
                }
            }
            let mut seen = std::collections::HashSet::new();
            for i in &p.items {
                if !seen.insert(i.id.as_str()) {
                    bail!("The page '{}' has two items with the id '{}'.", p.title, i.id);
                }
            }
        }
        Ok(())
    }

    pub fn page(&self, id: &str) -> Option<&DashPage> {
        self.pages.iter().find(|p| p.id == id)
    }
}

/// The rects of `count` items in a grid of `per_row` equal-width cells on a `page_w` × `page_h` page: `GRID_GAP` between cells,
/// no margin at the edge, and the rows share the page's height equally. 3 per row on 1000 units gives (1000 − 2 × 10) ÷ 3 = 326.67 wide.
pub fn grid_rects(per_row: usize, count: usize, page_w: f64, page_h: f64) -> Result<Vec<DashRect>> {
    if !(1..=MAX_PER_ROW).contains(&per_row) {
        bail!("A grid has 1 to {} items per row, not {}.", MAX_PER_ROW, per_row);
    }
    if !(page_w > 0.0 && page_h > 0.0) {
        bail!("The page has no size.");
    }
    let rows = count.div_ceil(per_row).max(1);
    let w = (page_w - GRID_GAP * (per_row - 1) as f64) / per_row as f64;
    let h = (page_h - GRID_GAP * (rows - 1) as f64) / rows as f64;
    Ok((0..count).map(|i| DashRect { x: (i % per_row) as f64 * (w + GRID_GAP), y: (i / per_row) as f64 * (h + GRID_GAP), w, h }).collect())
}

/// Where each item of the page sits, in page units: the grid's cells in order, or each item's own rect on a Free page
/// (an item with no rect on a Free page is an error to show, never a guessed place).
pub fn item_rects(page: &DashPage) -> Result<Vec<DashRect>> {
    match page.layout {
        PageLayout::Grid { per_row } => grid_rects(per_row, page.items.len(), PAGE_W, PAGE_H),
        PageLayout::Free => page.items.iter().map(|i| i.rect.ok_or_else(|| anyhow::anyhow!("The item '{}' on the page '{}' has no place yet. Drag it onto the page.", i.id, page.title))).collect(),
    }
}

// -------------------------------------------------------------------------------------- items' numbers

/// What one item needs from the worker: the source's own table (as the chart tab or pivot tab computes it), and the picks that apply.
#[derive(Clone, Debug)]
pub enum ItemSource {
    /// A chart tab's source tab and the chart tab's own spec.
    Chart { base: TabSource, steps: Vec<TransformStep>, spec: ChartSpec },
    /// A pivot tab: its table before the pivot step, and the pivot.
    Pivot { base: TabSource, steps: Vec<TransformStep>, spec: PivotSpec },
}

#[derive(Clone, Debug)]
pub struct ItemRequest {
    pub item_id: String,
    pub source: ItemSource,
    /// The page filters that apply to this item, each with the values picked (never an empty list: "All" is left out).
    pub picks: Vec<(PageFilter, Vec<String>)>,
    pub temp_dir: std::path::PathBuf,
}

/// A pivot item's table.
#[derive(Clone, Debug)]
pub struct TableData {
    pub df: DataFrame,
    pub label_columns: usize,
    /// Set when the pivot's layout is Nested.
    pub nested: Option<crate::pivot_view::NestedView>,
    pub notes: Vec<String>,
}

/// An item's numbers: what the screen, the HTML file, the PPTX and the PDF all draw.
#[derive(Clone, Debug)]
pub enum ItemData {
    Chart(Box<ChartData>),
    Table(Box<TableData>),
    /// The item can't be drawn, and why (a plain message, never an empty chart).
    Problem(String),
}

impl ItemData {
    /// The notes that go with the numbers (the month-end note among them): a chart's footnote, a pivot's notes.
    pub fn notes(&self) -> &[String] {
        match self {
            ItemData::Chart(c) => &c.notes,
            ItemData::Table(t) => &t.notes,
            ItemData::Problem(_) => &[],
        }
    }
}

pub struct DashItemResult {
    pub item_id: String,
    pub sig: String,
    pub data: ItemData,
}

pub struct DashValuesResult {
    pub filter_id: String,
    pub sig: String,
    pub values: Vec<String>,
    pub truncated: bool,
}

/// How many values a page filter's dropdown lists at most.
pub const MAX_FILTER_VALUES: usize = 2000;

fn pick_condition(f: &PageFilter, picks: &[String]) -> FilterCondition {
    FilterCondition { column: f.field.clone(), op: CondOp::InList, values: picks.to_vec(), lookup: f.model.clone(), ..Default::default() }
}

/// The chart spec with the page's picks added to its own filters (the chart's own filters stay).
pub fn chart_spec_with(spec: &ChartSpec, picks: &[(PageFilter, Vec<String>)]) -> ChartSpec {
    let mut s = spec.clone();
    s.filters.extend(picks.iter().map(|(f, v)| pick_condition(f, v)));
    s
}

/// The pivot spec with the page's picks: a pick on a field the pivot already filters replaces that filter (the page is the reader's
/// latest word); a lookup or Calendar field is added to the pivot's model fields.
pub fn pivot_spec_with(spec: &PivotSpec, picks: &[(PageFilter, Vec<String>)]) -> PivotSpec {
    let mut s = spec.clone();
    for (f, v) in picks {
        s.filters.retain(|x| x.field != f.field);
        s.filters.push(PivotFilter { field: f.field.clone(), values: v.iter().map(|x| Some(x.clone())).collect() });
        if let Some(m) = &f.model {
            if !s.model_fields.contains(m) {
                s.model_fields.push(m.clone());
            }
            if s.data_tab.is_empty() {
                s.data_tab = f.tab.clone();
            }
        }
    }
    s
}

/// The one compute path of an item (Phase 10 goal 4): the chart tab's own calculation (`chart_tab_source`'s) or the pivot's own
/// (`build_pivot`, `build_nested`), over the item's source with the page's picks added. Runs on the worker, inside the model scope.
pub fn compute_item(req: &ItemRequest) -> Result<ItemData> {
    match &req.source {
        ItemSource::Chart { base, steps, spec } => {
            let lf = crate::engine::compile_tab_lazy(base, steps, &req.temp_dir)?;
            let data = crate::charts::calc::compute(crate::pivot_table::without_total_rows(lf, steps), &chart_spec_with(spec, &req.picks))?;
            Ok(ItemData::Chart(Box::new(data)))
        }
        ItemSource::Pivot { base, steps, spec } => {
            let lf = crate::engine::compile_tab_lazy(base, steps, &req.temp_dir)?;
            let spec = pivot_spec_with(spec, &req.picks);
            let build = crate::pivot_table::build_pivot(lf.clone(), &spec)?;
            let nested = if spec.layout.is_nested() { Some(crate::pivot_view::build_nested(lf, &spec)?) } else { None };
            Ok(ItemData::Table(Box::new(TableData { df: build.df, label_columns: build.label_columns, nested, notes: build.warnings })))
        }
    }
}

/// The values a page filter's dropdown lists: the distinct values of the field over its data tab's table, as text, in order.
pub fn field_values(base: &TabSource, steps: &[TransformStep], f: &PageFilter, temp_dir: &std::path::Path) -> Result<(Vec<String>, bool)> {
    let lf = crate::engine::compile_tab_lazy(base, steps, temp_dir)?;
    let lf = match &f.model {
        Some(m) => crate::model::with_lookup_columns(lf, &[crate::model::LookupWant { out: f.field.clone(), field: m.clone(), as_: crate::model::LookupAs::Text, unmatched_out: None, blank_key_out: None }], "Page filter")?,
        None => lf,
    };
    let df = lf
        .select([col(f.field.as_str()).cast(DataType::String).alias("__v")])
        .filter(col("__v").is_not_null())
        .unique(None, UniqueKeepStrategy::First)
        .sort(["__v"], Default::default())
        .limit(MAX_FILTER_VALUES as u32 + 1)
        .collect()
        .with_context(|| format!("The values of '{}' could not be read", f.field))?;
    let mut values: Vec<String> = df.column("__v")?.str()?.into_iter().flatten().map(|s| s.to_string()).collect();
    let truncated = values.len() > MAX_FILTER_VALUES;
    values.truncate(MAX_FILTER_VALUES);
    Ok((values, truncated))
}

/// Whether a page filter reaches an item: the item's data tab is the filter's.
pub fn filter_applies(f: &PageFilter, item_data_tab: Option<&str>) -> bool {
    item_data_tab.is_some_and(|t| !t.is_empty() && t == f.tab)
}

/// "A page can have 4 filters. Remove one first." for a 5th.
pub const FIFTH_FILTER: &str = "A page can have 4 filters. Remove one first.";

impl DashPage {
    /// Adds a filter, or refuses the fifth with a plain message.
    pub fn add_filter(&mut self, f: PageFilter) -> Result<()> {
        if self.filters.len() >= MAX_PAGE_FILTERS {
            bail!("{}", FIFTH_FILTER);
        }
        self.filters.push(f);
        Ok(())
    }

    /// Removes a filter and its pick; the picks of the filters after it move up with them.
    pub fn remove_filter(&mut self, i: usize) {
        if i < self.filters.len() {
            self.filters.remove(i);
            for j in i..MAX_PAGE_FILTERS - 1 {
                self.state.filter_picks[j] = self.state.filter_picks[j + 1].take();
            }
            self.state.filter_picks[MAX_PAGE_FILTERS - 1] = None;
        }
    }

    /// The picks that count: one entry per filter that has a value picked ("All" has none).
    pub fn active_picks(&self) -> Vec<(usize, &PageFilter, &Vec<String>)> {
        self.filters.iter().enumerate().filter_map(|(i, f)| self.state.filter_picks.get(i).and_then(|p| p.as_ref()).filter(|v| !v.is_empty()).map(|v| (i, f, v))).collect()
    }
}
