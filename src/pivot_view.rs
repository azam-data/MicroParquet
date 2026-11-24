//! The Nested layout of a pivot (Phase 9 P2): Excel's Compact form.
//!
//! All Rows fields share one first column, "Row Labels". A parent row carries its subtotal on the same row, its children are
//! indented under it, the last Rows field's rows are the leaves, and the Grand total is at the bottom.
//!
//! The pivot step's result stays the flat table (`pivot_table::build_pivot` as before). A `NestedView` is a second, display-only
//! arrangement of the same numbers: it is built from a pivot with its subtotals switched on, and `build_pivot` computes every
//! subtotal from the group's own source rows (one group-by per level), never by adding up the children. So a Count distinct,
//! an Average or a W.Avg subtotal is the figure of the whole group.

use crate::engine::compile_tab_lazy;
use crate::models::*;
use crate::pivot_table::{build_pivot, PivotBuild, BLANK_LABEL, GRAND_TOTAL_LABEL};
use anyhow::{bail, Result};
use polars::prelude::*;
use std::collections::HashMap;
use std::path::Path;

/// The heading of the shared label column.
pub const ROW_LABELS: &str = "Row Labels";

/// What a line of the Nested view is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NestedKind {
    /// A group of a Rows field other than the last: its subtotal is on the line (bold).
    Parent,
    /// A row of the last Rows field: a row of the flat table.
    Leaf,
    /// The Grand total at the bottom (bold).
    GrandTotal,
    /// An empty line after a top-level group.
    Blank,
}

/// One line of the Nested view.
#[derive(Clone, Debug, PartialEq)]
pub struct NestedRow {
    /// 0 for the first Rows field (and the Grand total), one more for each field below it. The indent is this many levels.
    pub level: usize,
    /// "Conventional", "HC", "(blank)", "Grand total"; empty for a blank line.
    pub label: String,
    pub kind: NestedKind,
    /// The values of the Rows fields this line stands for, `None` being a blank: a parent has the first `level + 1`, a leaf all,
    /// the Grand total and a blank line none ("Show rows" reads them).
    pub keys: Vec<Option<String>>,
}

impl NestedRow {
    pub fn is_subtotal(&self) -> bool {
        self.kind == NestedKind::Parent
    }
    pub fn is_grand_total(&self) -> bool {
        self.kind == NestedKind::GrandTotal
    }
    pub fn is_blank(&self) -> bool {
        self.kind == NestedKind::Blank
    }
    /// A line that is drawn in bold: a parent and the Grand total.
    pub fn is_bold(&self) -> bool {
        matches!(self.kind, NestedKind::Parent | NestedKind::GrandTotal)
    }
}

/// A pivot as the Nested layout draws it (the screen, the Excel export and the PDF export).
#[derive(Clone, Debug)]
pub struct NestedView {
    /// "Row Labels".
    pub label_header: String,
    /// The Rows fields, in order ("Bank", "CL status").
    pub fields: Vec<String>,
    pub rows: Vec<NestedRow>,
    /// The value columns of the flat table (same names, same types, in the same order), one line per entry of `rows`; a blank
    /// line is all blank.
    pub values: DataFrame,
    /// For each value column: the values of the Columns fields it stands for (a Total column has none).
    pub col_keys: Vec<Vec<Option<String>>>,
    /// An empty line follows each top-level group.
    pub blank_line_after_group: bool,
}

impl NestedView {
    /// The value column named `name`, or `None`.
    pub fn column_keys(&self, name: &str) -> Option<&Vec<Option<String>>> {
        let at = self.values.get_column_names().iter().position(|n| n.as_str() == name)?;
        self.col_keys.get(at)
    }
}

/// The pivot of the tab's last step that is on, when it is a pivot in the Nested layout: `Some((index, settings))`. A pivot with no Rows field has nothing to nest.
pub fn nested_pivot_of(steps: &[TransformStep]) -> Option<(usize, PivotSpec)> {
    // the last step that is on: steps switched off after the pivot do not count
    let i = steps.iter().rposition(|s| !s.is_skipped())?;
    let spec = steps[i].as_pivot_spec()?;
    if matches!(spec.layout, PivotLayout::Nested { .. }) && !spec.rows.is_empty() {
        Some((i, spec))
    } else {
        None
    }
}

/// For a tab whose steps do not end in the nested pivot: the nested pivot that steps follow, when there is one (the tab then shows the
/// flat table and says so).
pub fn nested_pivot_followed(steps: &[TransformStep]) -> Option<usize> {
    let at = steps.iter().rposition(|s| !s.is_skipped() && matches!(s, TransformStep::PivotTable { .. }))?;
    let spec = steps[at].as_pivot_spec()?;
    let followed = steps[at + 1..].iter().any(|s| !s.is_skipped());
    if followed && matches!(spec.layout, PivotLayout::Nested { .. }) && !spec.rows.is_empty() {
        Some(at)
    } else {
        None
    }
}

/// The note a tab shows when its nested pivot has steps after it.
pub const NESTED_NOTE: &str = "Nested layout shows when the pivot is the tab's last step";

/// The Nested view of a tab: `None` when its last step is not a pivot in the Nested layout. The tab's steps run up to the pivot.
pub fn nested_view_of_tab(source: &TabSource, steps: &[TransformStep], temp_dir: &Path) -> Result<Option<NestedView>> {
    let Some((at, spec)) = nested_pivot_of(steps) else {
        return Ok(None);
    };
    let lf = compile_tab_lazy(source, &steps[..at], temp_dir)?;
    Ok(Some(build_nested(lf, &spec)?))
}

/// The Nested view of a pivot's input: the pivot is built with its subtotals on (each from its group's own rows), then arranged.
pub fn build_nested(lf: LazyFrame, spec: &PivotSpec) -> Result<NestedView> {
    let PivotLayout::Nested { blank_line_after_group } = spec.layout else {
        bail!("This pivot is in the Side by side layout. Choose Nested in the pivot's layout first.");
    };
    if spec.rows.is_empty() {
        bail!("The Nested layout needs at least one field in Rows. Add a field to Rows, or use Side by side.");
    }
    let mut with_subtotals = spec.clone();
    with_subtotals.subtotals = true;
    with_subtotals.layout = PivotLayout::SideBySide;
    let build = build_pivot(lf, &with_subtotals)?;
    arrange(&build, spec, blank_line_after_group)
}

/// Arranges a pivot built with subtotals on: the parent rows come from its subtotal rows, the leaves from its detail rows.
fn arrange(build: &PivotBuild, spec: &PivotSpec, blank_line: bool) -> Result<NestedView> {
    let nr = spec.rows.len();
    let shown = |k: &Option<String>| k.clone().unwrap_or_else(|| BLANK_LABEL.to_string());
    // the rows of the flat table by what they are: a detail row has all the Rows fields, a subtotal the first few, the Grand total none
    let mut subtotal_at: HashMap<&[Option<String>], usize> = HashMap::new();
    let mut details: Vec<usize> = Vec::new();
    let mut grand: Option<usize> = None;
    for (i, key) in build.row_keys.iter().enumerate() {
        if key.len() == nr {
            details.push(i);
        } else if key.is_empty() {
            grand = Some(i);
        } else {
            subtotal_at.insert(key.as_slice(), i);
        }
    }

    let mut rows: Vec<NestedRow> = Vec::new();
    // the row of the pivot each line takes its numbers from (None: a blank line)
    let mut source: Vec<Option<usize>> = Vec::new();
    for (n, &i) in details.iter().enumerate() {
        let key = &build.row_keys[i];
        // the first field at which this row leaves the group of the one before it
        let start = match n.checked_sub(1).map(|p| &build.row_keys[details[p]]) {
            None => 0,
            Some(prev) => (0..nr).find(|l| prev[*l] != key[*l]).unwrap_or(nr),
        };
        for level in start..nr.saturating_sub(1) {
            let group = &key[..=level];
            let Some(&at) = subtotal_at.get(group) else {
                bail!("Nested pivot: the subtotal of '{}' is missing from the pivot. Open the pivot again.", group.iter().map(shown).collect::<Vec<_>>().join(" / "));
            };
            rows.push(NestedRow { level, label: shown(&key[level]), kind: NestedKind::Parent, keys: group.to_vec() });
            source.push(Some(at));
        }
        rows.push(NestedRow { level: nr - 1, label: shown(&key[nr - 1]), kind: NestedKind::Leaf, keys: key.clone() });
        source.push(Some(i));
        if blank_line {
            let ends = match details.get(n + 1) {
                Some(&next) => build.row_keys[next][0] != key[0],
                None => true,
            };
            if ends {
                rows.push(NestedRow { level: 0, label: String::new(), kind: NestedKind::Blank, keys: Vec::new() });
                source.push(None);
            }
        }
    }
    if let Some(g) = grand {
        rows.push(NestedRow { level: 0, label: GRAND_TOTAL_LABEL.to_string(), kind: NestedKind::GrandTotal, keys: Vec::new() });
        source.push(Some(g));
    }

    // the numbers: the flat table's value columns, one line each
    let mut series: Vec<Series> = Vec::new();
    for c in build.df.get_columns().iter().skip(build.label_columns) {
        let name = c.name().clone();
        let s = match c.dtype() {
            DataType::Float64 => {
                let ca = c.f64()?;
                let v: Vec<Option<f64>> = source.iter().map(|s| s.and_then(|r| ca.get(r))).collect();
                Series::new(name, v)
            }
            DataType::Int64 => {
                let ca = c.i64()?;
                let v: Vec<Option<i64>> = source.iter().map(|s| s.and_then(|r| ca.get(r))).collect();
                Series::new(name, v)
            }
            other => bail!("Nested pivot: the column '{}' has an unexpected type ({:?}). Use Side by side.", c.name(), other),
        };
        series.push(s);
    }
    let values = DataFrame::new(series)?;
    Ok(NestedView { label_header: ROW_LABELS.to_string(), fields: spec.rows.clone(), rows, values, col_keys: build.col_keys.clone(), blank_line_after_group: blank_line })
}

/// The Nested view as the table the PDF export lays out: a "Row Labels" column of the lines' labels, then the value columns, with each
/// line's style (10 pt of indent per level, bold for a parent line). At most `max_lines` lines are taken.
pub fn pdf_table(view: &NestedView, max_lines: usize) -> Result<(DataFrame, Vec<crate::pdf_layout::RowStyle>)> {
    let n = view.rows.len().min(max_lines);
    let mut series: Vec<Series> = vec![Series::new(view.label_header.as_str().into(), view.rows.iter().take(n).map(|r| r.label.clone()).collect::<Vec<_>>())];
    for c in view.values.get_columns() {
        series.push(c.slice(0, n));
    }
    let styles = view.rows.iter().take(n).map(|r| crate::pdf_layout::RowStyle { indent_mm: r.level as f32 * crate::pdf_layout::NESTED_INDENT_MM, bold: r.is_bold() }).collect();
    Ok((DataFrame::new(series)?, styles))
}
