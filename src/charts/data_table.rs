//! The chart's data as a table (Phase 8 W11): categories down the side, one column per series, a total column and a total row,
//! in the chart's scale and the chart's order. The PDF page prints it with the chart (below it or beside it) and the HTML page
//! shows it open under the chart; both read this one table so they say the same numbers.

use super::axis::{value_text, ValueKind};
use super::data::ChartData;
use super::layout::{scaled, value_kind, LayoutOpts};
use super::spec::{ChartAgg, ChartKind, ChartSpec, Transform};
use super::tokens as t;

/// What a cell shows for a group that has no value.
pub const NO_VALUE: &str = "\u{2013}";

/// The table, as text: the first column is the categories, then one column per series (one column for a chart with no series), then
/// the total column when the numbers add up.
#[derive(Clone, Debug, PartialEq)]
pub struct DataTableText {
    /// "Sum of Balance \u{b7} BDT Crore": what the numbers are and in what unit.
    pub caption: String,
    /// The category column's name, the series names, and "Total" when there is a total column.
    pub header: Vec<String>,
    /// One row per category: its name and the numbers.
    pub rows: Vec<Vec<String>>,
    /// The total row ("Total" and the totals), when the numbers add up.
    pub total: Option<Vec<String>>,
    /// Why there is no total, when there is not one.
    pub no_total_note: Option<String>,
}

impl ChartSpec {
    /// Why this chart has no data table to print; `None` when it has one (a chart of categories and series: column, bar, line, area,
    /// heatmap, donut and histogram, with one measure and no small multiples).
    pub fn data_table_reason(&self) -> Option<String> {
        match self.kind {
            ChartKind::Bar | ChartKind::Line | ChartKind::Area | ChartKind::Heatmap | ChartKind::Donut | ChartKind::Histogram => {}
            ChartKind::Scatter | ChartKind::Bubble => return Some("A scatter or bubble chart draws single rows, so it has no table of categories and series.".to_string()),
            ChartKind::BoxPlot => return Some("A box plot draws the spread of each group, so it has no table of categories and series.".to_string()),
            ChartKind::Waterfall => return Some("A waterfall draws steps that add up to a total; its table is the steps, not categories and series.".to_string()),
            ChartKind::Sankey | ChartKind::Treemap => return Some("A flow or treemap page has its own table.".to_string()),
        }
        if self.facet.is_some() || !self.panels.is_empty() || self.measure_list().len() > 1 {
            return Some("A chart in small multiples, or with several measures, has one table for each panel; choose one panel's chart to print its table.".to_string());
        }
        None
    }
}

/// The data table of a chart, or `None` when the chart has none (`ChartSpec::data_table_reason`) or no data.
pub fn data_table(data: &ChartData, opts: &LayoutOpts) -> Option<DataTableText> {
    let spec = &data.spec;
    if spec.data_table_reason().is_some() || data.categories.is_empty() || data.series.is_empty() {
        return None;
    }
    let vk = value_kind(spec, 0);
    let decimals = opts.decimals + if vk == ValueKind::Percent { 1 } else { 0 };
    let text = |v: Option<f64>| v.map(|x| value_text(scaled(x, vk, opts), vk, decimals, false)).unwrap_or_else(|| NO_VALUE.to_string());
    let measure = data.measure_names.first().cloned().unwrap_or_default();
    let ml = spec.measure_list();
    let m0 = ml.first().cloned().unwrap_or_default();
    // the numbers add up when they are amounts or counts that were summed or counted, and nothing was done to them after
    let additive = matches!(vk, ValueKind::Amount | ValueKind::Count) && matches!(m0.agg, ChartAgg::Sum | ChartAgg::Count) && m0.transform == Transform::None && spec.kind != ChartKind::Histogram;
    let n_series = data.series.len();
    let mut header = vec![t::tidy(if spec.category.is_empty() { "Category" } else { &spec.category })];
    for s in &data.series {
        header.push(t::tidy(&if s.is_empty() { measure.clone() } else { s.clone() }));
    }
    let with_total_col = additive && n_series > 1;
    if with_total_col {
        header.push("Total".to_string());
    }
    let cell = |si: usize, c: usize| data.values.get(si).and_then(|r| r.get(c)).copied().flatten();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (c, name) in data.categories.iter().enumerate() {
        let mut row = vec![t::tidy(name)];
        for si in 0..n_series {
            row.push(text(cell(si, c)));
        }
        if with_total_col {
            let present: Vec<f64> = (0..n_series).filter_map(|si| cell(si, c)).collect();
            row.push(if present.is_empty() { NO_VALUE.to_string() } else { text(Some(present.iter().sum())) });
        }
        rows.push(row);
    }
    let total = additive.then(|| {
        let mut row = vec!["Total".to_string()];
        let mut grand = 0.0;
        let mut any = false;
        for si in 0..n_series {
            let present: Vec<f64> = (0..data.categories.len()).filter_map(|c| cell(si, c)).collect();
            any |= !present.is_empty();
            grand += present.iter().sum::<f64>();
            row.push(if present.is_empty() { NO_VALUE.to_string() } else { text(Some(present.iter().sum())) });
        }
        if with_total_col {
            row.push(if any { text(Some(grand)) } else { NO_VALUE.to_string() });
        }
        row
    });
    let no_total_note = (!additive).then(|| "No totals: these numbers are rates, shares, averages or extremes, which do not add up.".to_string());
    let mut caption = t::tidy(&measure);
    if super::layout::shows_unit(data, opts) && !opts.unit.is_empty() {
        caption = format!("{} \u{b7} {}", caption, opts.unit);
    }
    Some(DataTableText { caption, header, rows, total, no_total_note })
}

/// The table as the page shows it: an HTML table under the chart, every cell escaped.
pub fn data_table_html(table: &DataTableText) -> String {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let mut s = format!("<table class=\"mp-table\"><caption>{}</caption><thead><tr>", esc(&table.caption));
    for h in &table.header {
        s.push_str(&format!("<th>{}</th>", esc(h)));
    }
    s.push_str("</tr></thead><tbody>");
    for r in &table.rows {
        s.push_str("<tr>");
        for c in r {
            s.push_str(&format!("<td>{}</td>", esc(c)));
        }
        s.push_str("</tr>");
    }
    if let Some(tr) = &table.total {
        s.push_str("<tr class=\"mp-total\">");
        for c in tr {
            s.push_str(&format!("<th>{}</th>", esc(c)));
        }
        s.push_str("</tr>");
    }
    s.push_str("</tbody></table>");
    if let Some(n) = &table.no_total_note {
        s.push_str(&format!("<p class=\"mp-note\">{}</p>", esc(n)));
    }
    s
}
