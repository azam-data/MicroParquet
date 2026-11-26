//! Dashboards (Phase 10 D0): the model a project saves, and the layout maths of its grid.
//!
//! A dashboard belongs to the open project and is written in the `.mpproj` (`project_version` 3, only when a dashboard exists).
//! An item **points at** a chart tab or a pivot tab by the tab's project key (`t1`, `t2`, …) and never holds a copy of its
//! spec, so a dashboard follows its tabs. A tab that was deleted shows "Source tab removed" on its item and in every export.
//! Links use ids, never names, so renaming a page, an item or a filter breaks nothing.

use crate::kpi::{KpiSpec, KpiUnit};
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
/// The height of a 16:9 page (the width is always `PAGE_W`).
pub const PAGE_H_WIDE: f64 = 562.5;
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

/// What a slot of a design may hold.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SlotKind {
    #[default]
    Any,
    Chart,
    Pivot,
    Kpi,
}

impl SlotKind {
    pub const ALL: [SlotKind; 4] = [SlotKind::Any, SlotKind::Chart, SlotKind::Pivot, SlotKind::Kpi];

    pub fn label(self) -> &'static str {
        match self {
            SlotKind::Any => "Any",
            SlotKind::Chart => "Chart",
            SlotKind::Pivot => "Pivot",
            SlotKind::Kpi => "KPI",
        }
    }

    /// True when an item of this kind may fill a slot of this kind.
    pub fn accepts(self, k: &DashItemKind) -> bool {
        matches!((self, k), (SlotKind::Any, _) | (SlotKind::Chart, DashItemKind::Chart(_)) | (SlotKind::Pivot, DashItemKind::Pivot(_)) | (SlotKind::Kpi, DashItemKind::Kpi(_)))
    }
}

/// A place the owner drew over a design, in page units.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Slot {
    pub name: String,
    pub kind: SlotKind,
    pub rect: DashRect,
}

/// A design kept inside the project: the picture (base64 of the PNG), its blake3 and its slots.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct TemplateData {
    pub name: String,
    pub blake3: String,
    pub w: u32,
    pub h: u32,
    pub png_b64: String,
    pub slots: Vec<Slot>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Dashboard {
    pub id: String,
    pub title: String,
    pub pages: Vec<DashPage>,
    pub template_library: Vec<TemplateRef>,
    /// The copies of the designs, kept in the project so it shows them on another computer (D5).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub templates: Vec<TemplateData>,
    /// The unit KPI cards show money in when the card says Automatic (Crore when it is left alone).
    pub default_kpi_unit: crate::kpi::KpiUnit,
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
    /// The page's height in units when its design has another shape than 16:10 (the width is always 1000); `None` is 625.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_h: Option<f64>,
    /// The empty places of the design the page uses; an item that fills one has `slot` set (D5).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<Slot>,
    /// Free layout only: items may overlap (off by default).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub allow_overlap: bool,
    /// The items per row the page had before it was made Free, so switching back gives the same cells.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid_per_row: Option<usize>,
    /// Grid only: the page keeps at least this many rows of cells, so 4 per row and 3 rows fill a page with 12 places whatever the item count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid_rows: Option<usize>,
    /// What the reader had picked: saved with the project so reopening shows the same page.
    pub state: PageState,
    /// Phase 13: named filter states of this page, one click each ("Conventional wing, this quarter").
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<SavedView>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum DashItemKind {
    /// A chart tab, by project key.
    Chart(String),
    /// A pivot tab, by project key.
    Pivot(String),
    Kpi(KpiSpec),
    /// A text box (UI revamp): a heading or a note, shown as it is typed. It reads no data.
    Text(TextSpec),
    /// A picture (a logo, a floor plan): kept inside the project, so it shows on another computer. It reads no data.
    Image(ImageSpec),
}

/// How big the words of a text box are, in page units (a page is 1000 wide).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextSize {
    Small,
    #[default]
    Medium,
    Large,
    Title,
}

impl TextSize {
    pub const ALL: [TextSize; 4] = [TextSize::Small, TextSize::Medium, TextSize::Large, TextSize::Title];
    pub fn label(self) -> &'static str {
        match self {
            TextSize::Small => "Small",
            TextSize::Medium => "Medium",
            TextSize::Large => "Large",
            TextSize::Title => "Title",
        }
    }
    /// The height of a line, in page units.
    pub fn units(self) -> f64 {
        match self {
            TextSize::Small => 14.0,
            TextSize::Medium => 20.0,
            TextSize::Large => 32.0,
            TextSize::Title => 48.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Left,
    Centre,
    Right,
}

impl TextAlign {
    pub const ALL: [TextAlign; 3] = [TextAlign::Left, TextAlign::Centre, TextAlign::Right];
    pub fn label(self) -> &'static str {
        match self {
            TextAlign::Left => "Left",
            TextAlign::Centre => "Centre",
            TextAlign::Right => "Right",
        }
    }
}

/// The settings of a text box.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct TextSpec {
    pub text: String,
    pub size: TextSize,
    pub bold: bool,
    pub align: TextAlign,
}

/// A picture of an image item: a PNG, as base64 text (the picture is made smaller than `MAX_IMAGE_SIDE` before it is kept).
#[derive(Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(default)]
pub struct ImageSpec {
    pub name: String,
    pub png_b64: String,
    pub w: u32,
    pub h: u32,
}

// the picture's bytes are left out of the debug text (it is hashed and compared every frame): its name, shape and size stand for it
impl std::fmt::Debug for ImageSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ImageSpec({}, {}x{}, {} bytes)", self.name, self.w, self.h, self.png_b64.len())
    }
}

/// The longest side a kept picture has, in pixels: a logo needs no more, and the project stays small.
pub const MAX_IMAGE_SIDE: u32 = 1600;
/// The biggest picture file that is read, in bytes.
pub const MAX_IMAGE_FILE_BYTES: u64 = 20_000_000;

impl ImageSpec {
    /// Reads a picture file (PNG or JPEG), makes it fit `MAX_IMAGE_SIDE` and keeps it as a PNG. A file that is not a picture says so.
    pub fn from_bytes(name: &str, bytes: &[u8]) -> Result<ImageSpec> {
        if bytes.len() as u64 > MAX_IMAGE_FILE_BYTES {
            bail!("The picture '{}' is {:.0} MB; the biggest picture that is read is {:.0} MB. Pick a smaller one.", name, bytes.len() as f64 / 1e6, MAX_IMAGE_FILE_BYTES as f64 / 1e6);
        }
        let img = ::image::load_from_memory(bytes).with_context(|| format!("'{}' is not a PNG or JPEG picture, so it can't be put on the dashboard.", name))?;
        let img = if img.width().max(img.height()) > MAX_IMAGE_SIDE { img.resize(MAX_IMAGE_SIDE, MAX_IMAGE_SIDE, ::image::imageops::FilterType::Lanczos3) } else { img };
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), ::image::ImageFormat::Png).context("The picture could not be prepared. Pick another file.")?;
        Ok(ImageSpec { name: name.to_string(), png_b64: crate::templates::base64_encode(&png), w: img.width(), h: img.height() })
    }

    /// The PNG bytes, or why they can't be read.
    pub fn png(&self) -> Result<Vec<u8>> {
        crate::templates::base64_decode(&self.png_b64).map_err(|_| anyhow::anyhow!("The picture '{}' in this project is damaged. Remove the item and add the picture again.", self.name))
    }
}

impl Default for DashItemKind {
    fn default() -> Self {
        DashItemKind::Kpi(KpiSpec::default())
    }
}

/// The height of a page for a design `w` × `h` px: the page keeps the picture's shape (1000 units wide), so nothing is stretched.
pub fn page_height_for(w: u32, h: u32) -> f64 {
    if w == 0 || h == 0 {
        return PAGE_H;
    }
    PAGE_W * h as f64 / w as f64
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
    /// The slot of the page's design this item fills.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    /// The unit a pivot item shows money in; `None` is Crore. Old projects have none and look the same as before the setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<ItemUnit>,
}

/// The unit a pivot item's money columns are shown in (Phase 10): Crore unless the item says otherwise. Counts and shares are not money
/// and follow the app's unit rule: counts whole, shares and rates as they are.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ItemUnit {
    #[default]
    Crore,
    Lakh,
    Million,
    Thousand,
    Absolute,
}

impl ItemUnit {
    pub const ALL: [ItemUnit; 5] = [ItemUnit::Crore, ItemUnit::Lakh, ItemUnit::Million, ItemUnit::Thousand, ItemUnit::Absolute];
    pub fn label(self) -> &'static str {
        match self {
            ItemUnit::Crore => "Crore",
            ItemUnit::Lakh => "Lakh",
            ItemUnit::Million => "Million",
            ItemUnit::Thousand => "Thousand",
            ItemUnit::Absolute => "Absolute (BDT)",
        }
    }
    pub fn divisor(self) -> f64 {
        match self {
            ItemUnit::Crore => 10_000_000.0,
            ItemUnit::Lakh => 100_000.0,
            ItemUnit::Million => 1_000_000.0,
            ItemUnit::Thousand => 1_000.0,
            ItemUnit::Absolute => 1.0,
        }
    }
    /// What a heading says after a money column's name.
    pub fn short(self) -> &'static str {
        match self {
            ItemUnit::Crore => "Cr",
            ItemUnit::Lakh => "Lakh",
            ItemUnit::Million => "M",
            ItemUnit::Thousand => "K",
            ItemUnit::Absolute => "BDT",
        }
    }
}

/// What a pivot value column holds, which decides how it is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellKind {
    /// Money: divided by the item's unit.
    Money,
    /// A count of rows or of distinct values: whole numbers.
    Count,
    /// An average or weighted average: as it is.
    Plain,
    /// A share or a rate (percent): as it is, with its percent sign in the heading.
    Share,
}

/// The kind of a pivot value, by the app's rule for charts (`value_kind`): counts whole, averages as they are, shares and ratio measures
/// as percent, everything else money.
pub fn cell_kind(v: &PivotValue) -> CellKind {
    if let Some(t) = &v.time {
        if t.show == TimeShow::PercentChange {
            return CellKind::Share;
        }
        if crate::measures::find_measure(&v.field).is_some_and(|m| m.is_ratio()) {
            return CellKind::Share;
        }
        return CellKind::Money;
    }
    match v.agg {
        PivotAgg::Count | PivotAgg::CountDistinct => CellKind::Count,
        PivotAgg::Average | PivotAgg::WeightedAvg => CellKind::Plain,
        PivotAgg::Sum | PivotAgg::Min | PivotAgg::Max => CellKind::Money,
    }
}

/// The divisor and decimals a value column is shown with.
pub fn cell_scale(kind: CellKind, unit: ItemUnit) -> (f64, usize) {
    match kind {
        CellKind::Money => (unit.divisor(), 2),
        CellKind::Count => (1.0, 0),
        CellKind::Plain | CellKind::Share => (1.0, 2),
    }
}

/// A heading with its unit: "Sum of Balance (Cr)", "Average of Rate", "Share (%)".
pub fn heading_with_unit(name: &str, kind: CellKind, unit: ItemUnit) -> String {
    match kind {
        CellKind::Money => format!("{} ({})", name, unit.short()),
        CellKind::Share => format!("{} (%)", name),
        CellKind::Count | CellKind::Plain => name.to_string(),
    }
}

impl TableData {
    /// The kind of value column `k` (counted among the value columns, not the labels); money when the table does not know.
    pub fn kind_of(&self, k: usize) -> CellKind {
        self.kinds.get(k).copied().unwrap_or(CellKind::Money)
    }
}

impl DashItem {
    /// The project key of the tab this item reads, or `None` for a KPI card.
    pub fn source_key(&self) -> Option<&str> {
        match &self.kind {
            DashItemKind::Chart(k) | DashItemKind::Pivot(k) => Some(k),
            DashItemKind::Kpi(_) | DashItemKind::Text(_) | DashItemKind::Image(_) => None,
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
    /// The item the click was on: it keeps all its marks, every other item that reads the same data tab is filtered.
    pub item: String,
    /// The project key of the data tab the field belongs to.
    pub tab: String,
    /// The lookup field behind `dimension`; `None` for a column of the data tab.
    pub model: Option<crate::model::ModelFieldRef>,
}

impl CrossPick {
    /// The click as a page filter with one value, so it reaches an item the way a page filter does.
    pub fn as_filter(&self) -> PageFilter {
        PageFilter { id: "cross".into(), label: self.dimension.clone(), tab: self.tab.clone(), field: self.dimension.clone(), model: self.model.clone(), multi: false }
    }

    /// "Region = Dhaka": the chip's text.
    pub fn chip(&self) -> String {
        format!("{} = {}", self.dimension, self.value)
    }

    fn data_tab_of(tab: Option<&str>) -> Result<String> {
        match tab {
            Some(t) if !t.is_empty() => Ok(t.to_string()),
            _ => bail!("This item has no data tab, so a click on it cannot filter the page."),
        }
    }

    /// A click on a bar, slice or point of a chart: the chart's category at the level it is drawn at. A date part, the
    /// "(Other)" mark and a blank are refused with what to do instead.
    pub fn from_chart(spec: &ChartSpec, category: &str, item: &str, data_tab: Option<&str>) -> Result<CrossPick> {
        ChartSpec::refuse_other(category)?;
        if spec.category_part.is_some() {
            bail!("'{}' is a part of a date, which a click cannot filter by. Use a page filter on the calendar field instead.", spec.category);
        }
        if category == crate::pivot_table::BLANK_LABEL || category.is_empty() {
            bail!("A blank group cannot filter the page, because no value stands for it.");
        }
        Ok(CrossPick { dimension: spec.category.clone(), value: category.to_string(), item: item.to_string(), tab: Self::data_tab_of(data_tab)?, model: spec.category_model.clone() })
    }

    /// A click on a row label of a pivot: `level` is the Rows field the label belongs to (0 for the first).
    pub fn from_pivot(spec: &PivotSpec, level: usize, value: &str, item: &str, data_tab: Option<&str>) -> Result<CrossPick> {
        let Some(field) = spec.rows.get(level) else { bail!("This label does not belong to a Rows field, so it cannot filter the page.") };
        if spec.date_fields.iter().any(|d| &d.name == field) {
            bail!("'{}' is a part of a date, which a click cannot filter by. Use a page filter on the calendar field instead.", field);
        }
        if value == crate::pivot_table::BLANK_LABEL || value.is_empty() {
            bail!("A blank group cannot filter the page, because no value stands for it.");
        }
        if value == crate::pivot_table::GRAND_TOTAL_LABEL {
            bail!("The grand total is every row, so it cannot filter the page.");
        }
        Ok(CrossPick { dimension: field.clone(), value: value.to_string(), item: item.to_string(), tab: Self::data_tab_of(data_tab)?, model: spec.model_fields.iter().find(|m| &m.name == field).cloned() })
    }
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

/// The most views a page keeps: a list longer than this is not a one-click list any more.
pub const MAX_VIEWS: usize = 20;

/// A named filter state of one page (Phase 13): what each page filter had picked, and the click on a mark or a row. The picks are
/// kept by the filter's id, so a view still fits when the filters are reordered. The drill levels of the items are not part of a view.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SavedView {
    pub name: String,
    /// (filter id, picked values); a filter that had nothing picked is not listed (it is "All").
    pub picks: Vec<(String, Vec<String>)>,
    pub cross: Option<CrossPick>,
}

impl DashPage {
    /// Saves the page's present filter state as the view `name`; a view of the same name (capitals ignored) is replaced.
    pub fn save_view(&mut self, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            bail!("Type a name for the view (for example \"Conventional wing, Q4\").");
        }
        let view = SavedView {
            name: name.to_string(),
            picks: self.active_picks().into_iter().map(|(_, f, v)| (f.id.clone(), v.clone())).collect(),
            cross: self.state.cross.clone(),
        };
        match self.views.iter().position(|v| v.name.to_lowercase() == name.to_lowercase()) {
            Some(i) => self.views[i] = view,
            None => {
                if self.views.len() >= MAX_VIEWS {
                    bail!("A page keeps {} views. Delete one first.", MAX_VIEWS);
                }
                self.views.push(view);
            }
        }
        Ok(())
    }

    /// Puts the page's filters back as the view `name` had them. A filter the view names that is no longer on the page is said, not
    /// skipped silently. The items' drill levels start again from the top.
    pub fn apply_view(&mut self, name: &str) -> Result<Vec<String>> {
        let Some(view) = self.views.iter().find(|v| v.name == name).cloned() else {
            bail!("The view '{}' is not on this page.", name);
        };
        let mut notes = Vec::new();
        let mut state = PageState::default();
        for (id, values) in &view.picks {
            match self.filters.iter().position(|f| &f.id == id) {
                Some(j) => state.filter_picks[j] = Some(values.clone()),
                None => notes.push(format!("The view '{}' used a filter that is no longer on this page, so that part is not applied.", view.name)),
            }
        }
        state.cross = view.cross.clone();
        notes.dedup();
        self.state = state;
        Ok(notes)
    }

    pub fn delete_view(&mut self, name: &str) -> bool {
        let before = self.views.len();
        self.views.retain(|v| v.name != name);
        self.views.len() != before
    }

    /// Whether the page is in the state the view saved (to mark the view that is on).
    pub fn view_is_on(&self, view: &SavedView) -> bool {
        let now: Vec<(String, Vec<String>)> = self.active_picks().into_iter().map(|(_, f, v)| (f.id.clone(), v.clone())).collect();
        now == view.picks && self.state.cross == view.cross
    }
}

impl PageState {
    /// The "Clear" button: all picks, the cross-filter and every drill go back to the start.
    pub fn clear(&mut self) {
        *self = PageState::default();
    }

    /// A click on a mark: the same mark again clears the cross-filter, any other mark replaces it. Returns true when it is set.
    pub fn toggle_cross(&mut self, pick: CrossPick) -> bool {
        if self.cross.as_ref() == Some(&pick) {
            self.cross = None;
            false
        } else {
            self.cross = Some(pick);
            true
        }
    }

    /// One level down on an item: the clicked value joins the item's own path.
    pub fn drill_down(&mut self, item: &str, value: &str) {
        self.drill.entry(item.to_string()).or_default().push(value.to_string());
    }

    /// Back up so that `depth` clicked values are left on the item's path (0 = the top of the item).
    pub fn drill_back_to(&mut self, item: &str, depth: usize) {
        if let Some(p) = self.drill.get_mut(item) {
            p.truncate(depth);
            if p.is_empty() {
                self.drill.remove(item);
            }
        }
    }

    /// The item's own drill path.
    pub fn drill_of(&self, item: &str) -> &[String] {
        self.drill.get(item).map(|v| v.as_slice()).unwrap_or(&[])
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
    grid_rects_min_rows(per_row, count, 0, page_w, page_h)
}

/// The most rows a page can be asked to keep.
pub const MAX_GRID_ROWS: usize = 6;

/// `grid_rects` with at least `min_rows` rows, so a page with few items still has cells of the size of a full page.
pub fn grid_rects_min_rows(per_row: usize, count: usize, min_rows: usize, page_w: f64, page_h: f64) -> Result<Vec<DashRect>> {
    if !(1..=MAX_PER_ROW).contains(&per_row) {
        bail!("A grid has 1 to {} items per row, not {}.", MAX_PER_ROW, per_row);
    }
    if !(page_w > 0.0 && page_h > 0.0) {
        bail!("The page has no size.");
    }
    let rows = count.div_ceil(per_row).max(min_rows.min(MAX_GRID_ROWS)).max(1);
    let w = (page_w - GRID_GAP * (per_row - 1) as f64) / per_row as f64;
    let h = (page_h - GRID_GAP * (rows - 1) as f64) / rows as f64;
    Ok((0..count).map(|i| DashRect { x: (i % per_row) as f64 * (w + GRID_GAP), y: (i / per_row) as f64 * (h + GRID_GAP), w, h }).collect())
}

/// Where each item of the page sits, in page units: the grid's cells in order, or each item's own rect on a Free page
/// (an item with no rect on a Free page is an error to show, never a guessed place).
pub fn item_rects(page: &DashPage) -> Result<Vec<DashRect>> {
    match page.layout {
        PageLayout::Grid { per_row } => grid_rects_min_rows(per_row, page.items.len(), page.grid_rows.unwrap_or(0), PAGE_W, page.height()),
        PageLayout::Free => page.items.iter().map(|i| i.rect.ok_or_else(|| anyhow::anyhow!("The item '{}' on the page '{}' has no place yet. Drag it onto the page.", i.id, page.title))).collect(),
    }
}

// -------------------------------------------------------------------------------------- free layout (Phase 10 D4)

/// The smallest an item may be made, in page units.
pub const MIN_ITEM_W: f64 = 60.0;
pub const MIN_ITEM_H: f64 = 40.0;
/// The size of an item added to a Free page.
pub const NEW_ITEM_W: f64 = 300.0;
pub const NEW_ITEM_H: f64 = 200.0;

/// What a drag holds: the whole item, a side or a corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Handle {
    Move,
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

impl Handle {
    pub const ALL: [Handle; 8] = [Handle::NW, Handle::N, Handle::NE, Handle::E, Handle::SE, Handle::S, Handle::SW, Handle::W];

    pub fn name(self) -> &'static str {
        match self {
            Handle::Move => "move",
            Handle::N => "N",
            Handle::S => "S",
            Handle::E => "E",
            Handle::W => "W",
            Handle::NE => "NE",
            Handle::NW => "NW",
            Handle::SE => "SE",
            Handle::SW => "SW",
        }
    }
}

impl DashRect {
    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    /// True when the two share area; touching edges do not overlap.
    pub fn overlaps(&self, o: &DashRect) -> bool {
        self.x < o.right() - 1e-9 && o.x < self.right() - 1e-9 && self.y < o.bottom() - 1e-9 && o.y < self.bottom() - 1e-9
    }
}

pub fn snap(v: f64) -> f64 {
    (v / SNAP).round() * SNAP
}

/// The rect brought inside the page: never smaller than the minimum, never off an edge. An item dragged away is put back, not lost.
pub fn clamp_to_page(r: DashRect, page_h: f64) -> DashRect {
    let w = r.w.clamp(MIN_ITEM_W, PAGE_W);
    let h = r.h.clamp(MIN_ITEM_H, page_h);
    DashRect { x: r.x.clamp(0.0, PAGE_W - w), y: r.y.clamp(0.0, page_h - h), w, h }
}

/// The rect after dragging `handle` by (`dx`, `dy`) page units from `start`: the edges that move snap to the 10-unit grid, the
/// others stay where they were; the result is inside the page and not below the minimum size.
pub fn drag_rect(start: DashRect, handle: Handle, dx: f64, dy: f64, page_h: f64) -> DashRect {
    if handle == Handle::Move {
        return clamp_to_page(DashRect { x: snap(start.x + dx), y: snap(start.y + dy), w: start.w, h: start.h }, page_h);
    }
    let (mut l, mut t, mut r, mut b) = (start.x, start.y, start.right(), start.bottom());
    if matches!(handle, Handle::W | Handle::NW | Handle::SW) {
        l = snap(l + dx).clamp(0.0, r - MIN_ITEM_W);
    }
    if matches!(handle, Handle::E | Handle::NE | Handle::SE) {
        r = snap(r + dx).clamp(l + MIN_ITEM_W, PAGE_W);
    }
    if matches!(handle, Handle::N | Handle::NE | Handle::NW) {
        t = snap(t + dy).clamp(0.0, b - MIN_ITEM_H);
    }
    if matches!(handle, Handle::S | Handle::SE | Handle::SW) {
        b = snap(b + dy).clamp(t + MIN_ITEM_H, page_h);
    }
    DashRect { x: l, y: t, w: r - l, h: b - t }
}

impl DashPage {
    /// Where item `idx` ends up when it is asked to take `want`: inside the page, and not over another item unless the page allows
    /// it. A place that is not allowed gives `prev` (the last good place), so a drag never jumps onto another item.
    pub fn place(&self, idx: usize, want: DashRect, prev: DashRect) -> DashRect {
        let r = clamp_to_page(want, self.height());
        if self.allow_overlap {
            return r;
        }
        let blocked = self.items.iter().enumerate().any(|(i, it)| i != idx && it.rect.is_some_and(|o| o.overlaps(&r)));
        if blocked {
            prev
        } else {
            r
        }
    }

    /// Uses a design: its picture is the page's background and its slots become empty places. The page becomes Free, because an item
    /// that fills a slot sits where the slot is.
    pub fn apply_template(&mut self, t: TemplateRef, slots: Vec<Slot>, page_h: f64) -> Result<()> {
        self.to_free()?;
        self.background = Some(t);
        self.slots = slots;
        self.set_height(page_h);
        Ok(())
    }

    /// The page's height in units: 625 (16:10) unless a design of another shape gives it. Items that no longer fit are pulled inside.
    pub fn height(&self) -> f64 {
        self.page_h.unwrap_or(PAGE_H)
    }

    pub fn set_height(&mut self, h: f64) {
        self.page_h = if (h - PAGE_H).abs() < 0.01 { None } else { Some(h) };
        let h = self.height();
        for i in &mut self.items {
            if let Some(r) = i.rect {
                i.rect = Some(clamp_to_page(r, h));
            }
        }
    }

    /// The slots no item fills yet.
    pub fn empty_slots(&self) -> Vec<&Slot> {
        self.slots.iter().filter(|s| !self.items.iter().any(|i| i.slot.as_deref() == Some(s.name.as_str()))).collect()
    }

    /// Puts `item` in the slot `name`: it takes the slot's place. A slot that is full, or of another kind, is refused in words.
    pub fn fill_slot(&mut self, name: &str, mut item: DashItem) -> Result<()> {
        let Some(slot) = self.slots.iter().find(|s| s.name == name).cloned() else { bail!("The page has no slot named '{}'.", name) };
        if self.items.iter().any(|i| i.slot.as_deref() == Some(name)) {
            bail!("The slot '{}' already has an item. Remove it first.", name);
        }
        if !slot.kind.accepts(&item.kind) {
            bail!("The slot '{}' is for a {} item. Fill it with one, or change the slot's type in the design.", name, slot.kind.label().to_lowercase());
        }
        item.rect = Some(slot.rect);
        item.slot = Some(name.to_string());
        self.items.push(item);
        Ok(())
    }

    /// The first free place for a new item of `w` × `h`, scanning rows from the top left in snapped steps. A page with no room is an
    /// error in words (the page may allow overlap, or lose an item).
    pub fn first_free_spot(&self, w: f64, h: f64) -> Result<DashRect> {
        let mut y = 0.0;
        while y + h <= self.height() + 1e-9 {
            let mut x = 0.0;
            while x + w <= PAGE_W + 1e-9 {
                let r = DashRect { x, y, w, h };
                if !self.items.iter().any(|i| i.rect.is_some_and(|o| o.overlaps(&r))) {
                    return Ok(r);
                }
                x += SNAP;
            }
            y += SNAP;
        }
        if self.allow_overlap {
            return Ok(DashRect { x: 0.0, y: 0.0, w, h });
        }
        bail!("The page '{}' has no room for another item. Remove or shrink one, or turn on Allow overlap.", self.title)
    }

    /// Grid to Free: every item keeps the place its cell gave it.
    pub fn to_free(&mut self) -> Result<()> {
        if let PageLayout::Grid { per_row } = self.layout {
            let rects = grid_rects_min_rows(per_row, self.items.len(), self.grid_rows.unwrap_or(0), PAGE_W, self.height())?;
            for (i, r) in self.items.iter_mut().zip(rects) {
                i.rect = Some(r);
            }
            self.grid_per_row = Some(per_row);
            self.layout = PageLayout::Free;
        }
        Ok(())
    }

    /// The wide preset: 16:9, a grid of 4 per row and at least 3 rows, so twelve visuals show on one landscape page.
    pub fn set_wide_grid(&mut self) {
        self.set_height(PAGE_H_WIDE);
        self.layout = PageLayout::Grid { per_row: 4 };
        self.grid_per_row = None;
        self.grid_rows = Some(3);
        for i in &mut self.items {
            i.rect = None;
        }
    }

    /// Free to Grid: the items go to the cells of their order, with the items per row the page had before it was Free.
    pub fn to_grid(&mut self) {
        if self.layout == PageLayout::Free {
            for i in &mut self.items {
                i.rect = None;
            }
            self.layout = PageLayout::Grid { per_row: self.grid_per_row.unwrap_or(2).clamp(1, MAX_PER_ROW) };
        }
    }

    /// The ids of the items of a Free page that do not sit on the cell their order gives them.
    pub fn off_grid_items(&self) -> Vec<String> {
        let per_row = self.grid_per_row.unwrap_or(2).clamp(1, MAX_PER_ROW);
        let cells = grid_rects_min_rows(per_row, self.items.len(), self.grid_rows.unwrap_or(0), PAGE_W, self.height()).unwrap_or_default();
        let near = |a: f64, b: f64| (a - b).abs() < 0.01;
        self.items.iter().enumerate().filter(|(i, it)| !it.rect.zip(cells.get(*i)).is_some_and(|(r, c)| near(r.x, c.x) && near(r.y, c.y) && near(r.w, c.w) && near(r.h, c.h))).map(|(_, it)| it.id.clone()).collect()
    }

    /// What the switch dialog says before the layout changes.
    pub fn switch_message(&self) -> String {
        match self.layout {
            PageLayout::Grid { .. } => "Every item keeps the place its cell gives it now, and you can then drag and resize them. Switching back puts each item in the cell of its order; an item you have moved goes back to its cell.".to_string(),
            PageLayout::Free => {
                let off = self.off_grid_items();
                if off.is_empty() {
                    "Every item sits on the grid, so nothing moves.".to_string()
                } else {
                    format!("{} of {} items are not on the grid. They will go to the cells of their order and lose the place you gave them: {}.", off.len(), self.items.len(), off.join(", "))
                }
            }
        }
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
    /// A KPI card: its settings and the data tab it reads (Phase 10 D2).
    Kpi { spec: KpiSpec, tab_key: String, base: TabSource, steps: Vec<TransformStep> },
    /// A text box: nothing to calculate.
    Text(TextSpec),
    /// A picture: nothing to calculate.
    Image(ImageSpec),
}

#[derive(Clone, Debug)]
pub struct ItemRequest {
    pub item_id: String,
    pub source: ItemSource,
    /// The page filters that apply to this item, each with the values picked (never an empty list: "All" is left out).
    pub picks: Vec<(PageFilter, Vec<String>)>,
    /// The values clicked on the way down a chart's drill levels, on this page (empty = the chart as its tab has it).
    pub drill: Vec<String>,
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
    /// The kind of each value column of `df` (after the label columns), from the pivot's Values.
    pub kinds: Vec<CellKind>,
}

/// An item's numbers: what the screen, the HTML file, the PPTX and the PDF all draw.
#[derive(Clone, Debug)]
pub enum ItemData {
    Chart(Box<ChartData>),
    Table(Box<TableData>),
    Kpi(Box<crate::kpi::KpiValue>),
    Text(Box<TextSpec>),
    Image(Box<ImageSpec>),
    /// The item can't be drawn, and why (a plain message, never an empty chart).
    Problem(String),
}

impl ItemData {
    /// The notes that go with the numbers (the month-end note among them): a chart's footnote, a pivot's notes.
    pub fn notes(&self) -> &[String] {
        match self {
            ItemData::Chart(c) => &c.notes,
            ItemData::Table(t) => &t.notes,
            ItemData::Kpi(k) => &k.notes,
            ItemData::Text(_) | ItemData::Image(_) | ItemData::Problem(_) => &[],
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

/// The chart spec `path` levels further down: each clicked value goes on the chart's drill path (a value past the last level is left out).
pub fn chart_spec_drilled(spec: &ChartSpec, path: &[String]) -> ChartSpec {
    let mut s = spec.clone();
    for v in path {
        if !s.drill_down(v) {
            break;
        }
    }
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
            let data = crate::charts::calc::compute(crate::pivot_table::without_total_rows(lf, steps), &chart_spec_with(&chart_spec_drilled(spec, &req.drill), &req.picks))?;
            Ok(ItemData::Chart(Box::new(data)))
        }
        ItemSource::Kpi { spec, tab_key, base, steps } => {
            let r = crate::kpi::KpiRequest { spec: spec.clone(), tab_key: tab_key.clone(), base: base.clone(), steps: steps.clone(), picks: req.picks.clone(), temp_dir: req.temp_dir.clone() };
            Ok(ItemData::Kpi(Box::new(crate::kpi::compute_kpi(&r)?)))
        }
        ItemSource::Text(t) => Ok(ItemData::Text(Box::new(t.clone()))),
        ItemSource::Image(i) => Ok(ItemData::Image(Box::new(i.clone()))),
        ItemSource::Pivot { base, steps, spec } => {
            let lf = crate::engine::compile_tab_lazy(base, steps, &req.temp_dir)?;
            let spec = pivot_spec_with(spec, &req.picks);
            let build = crate::pivot_table::build_pivot(lf.clone(), &spec)?;
            let nested = if spec.layout.is_nested() { Some(crate::pivot_view::build_nested(lf, &spec)?) } else { None };
            let kinds = build.col_values.iter().map(|j| spec.values.get(*j).map(cell_kind).unwrap_or(CellKind::Money)).collect();
            Ok(ItemData::Table(Box::new(TableData { df: build.df, label_columns: build.label_columns, nested, notes: build.warnings, kinds })))
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

    /// The page's cross-filter as it reaches `item_id`: every item but the one that was clicked (it keeps all its marks).
    pub fn cross_for(&self, item_id: &str) -> Option<(PageFilter, Vec<String>)> {
        self.state.cross.as_ref().filter(|c| c.item != item_id).map(|c| (c.as_filter(), vec![c.value.clone()]))
    }

    /// The picks that count: one entry per filter that has a value picked ("All" has none).
    pub fn active_picks(&self) -> Vec<(usize, &PageFilter, &Vec<String>)> {
        self.filters.iter().enumerate().filter_map(|(i, f)| self.state.filter_picks.get(i).and_then(|p| p.as_ref()).filter(|v| !v.is_empty()).map(|v| (i, f, v))).collect()
    }
}
