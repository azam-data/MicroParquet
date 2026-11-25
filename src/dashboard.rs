//! Dashboards (Phase 10 D0): the model a project saves, and the layout maths of its grid.
//!
//! A dashboard belongs to the open project and is written in the `.mpproj` (`project_version` 3, only when a dashboard exists).
//! An item **points at** a chart tab or a pivot tab by the tab's project key (`t1`, `t2`, …) and never holds a copy of its
//! spec, so a dashboard follows its tabs. A tab that was deleted shows "Source tab removed" on its item and in every export.
//! Links use ids, never names, so renaming a page, an item or a filter breaks nothing.

use crate::kpi::KpiSpec;
use crate::project::Project;
use anyhow::{bail, Result};
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
