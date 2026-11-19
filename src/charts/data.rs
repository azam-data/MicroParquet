//! `ChartData`: the calculated chart, the only thing any renderer receives, and the transforms that
//! are done on it (shares, running sums, waterfall steps, heatmap matrices). Everything here is
//! plain arithmetic on the table, so it is the same for the in-app, HTML and Excel renderers.

use super::spec::{ChartAgg, ChartSpec, Transform, WaterfallMode};
use anyhow::{bail, Result};

/// One measure's numbers, in the shape of the chart's categories and series.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MeasureData {
    pub name: String,
    /// `[series][category]`; `None` where the group has no value.
    pub values: Vec<Vec<Option<f64>>>,
    /// Rows behind each value, same shape.
    pub rows: Vec<Vec<u64>>,
}

/// One small multiple: the facet value and its own chart table.
#[derive(Clone, Debug, PartialEq)]
pub struct FacetData {
    pub name: String,
    pub data: Box<ChartData>,
}

/// One point of a scatter or bubble chart.
#[derive(Clone, Debug, PartialEq)]
pub struct ScatterPoint {
    pub x: f64,
    pub y: f64,
    pub size: Option<f64>,
    /// Position in `ChartData::series`.
    pub series: usize,
}

/// A histogram: equal bins from the 1st to the 99th percentile (as in the Phase 5 column inspector).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Histogram {
    /// `bins + 1` edges.
    pub edges: Vec<f64>,
    pub counts: Vec<u64>,
    /// The values below the first edge and above the last.
    pub below: u64,
    pub above: u64,
}

/// The five numbers of a box plot (whiskers reach the furthest value within 1.5 × the interquartile range).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BoxStat {
    pub category: String,
    pub n: u64,
    pub min: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub max: f64,
    pub low_whisker: f64,
    pub high_whisker: f64,
    pub outliers: Vec<f64>,
}

/// The calculated chart. The primary measure is `values` / `rows`; further measures are in `extra`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ChartData {
    pub spec: ChartSpec,
    /// In chart order; "Other" (when there is one) is last.
    pub categories: Vec<String>,
    /// One name per series ("" when there is no series), most important first, "Other" last.
    pub series: Vec<String>,
    /// `[series][category]`; `None` where the group has no value (no rows, or a rate with weights of both signs).
    pub values: Vec<Vec<Option<f64>>>,
    /// Rows behind each value, same shape.
    pub rows: Vec<Vec<u64>>,
    /// Every row of the tab (after its steps).
    pub total_rows: u64,
    /// Distinct categories in the data, and how many of them are inside "Other".
    pub distinct_categories: usize,
    pub folded_categories: usize,
    pub notes: Vec<String>,
    /// The categories and series that stayed (before the rest were folded into "Other").
    pub kept_categories: Vec<String>,
    pub kept_series: Vec<String>,
    /// The name of every measure (the primary one first): "Sum of Balance".
    pub measure_names: Vec<String>,
    /// Measures 2, 3, …: the same categories and series as the primary one.
    pub extra: Vec<MeasureData>,
    /// The bubble size, when the chart has one.
    pub size: Option<MeasureData>,
    /// Small multiples, when the chart has a facet.
    pub facets: Vec<FacetData>,
    /// Scatter and bubble: the points (at most 5,000 per series) and how many rows they stand for.
    pub points: Vec<ScatterPoint>,
    pub points_total: u64,
    /// Histogram: the bins (the categories are their labels, the values their counts).
    pub hist: Option<Histogram>,
    /// Box plot: one box per category.
    pub boxes: Vec<BoxStat>,
}

impl ChartData {
    /// "Sum of Balance by Term Period: all 244,250 rows (Crore)".
    pub fn title(&self, unit: Option<&str>) -> String {
        let s = &self.spec;
        let what = match (&s.value, s.agg) {
            (_, ChartAgg::Count) | (None, _) => "Count of rows".to_string(),
            (Some(v), a) => format!("{} of {}", a.title_word(), v),
        };
        let by = match &s.series {
            Some(x) => format!("{} and {}", s.category, x),
            None => s.category.clone(),
        };
        let mut t = format!("{} by {}: all {} rows", what, by, format_count(self.total_rows));
        if self.folded_categories > 0 {
            t.push_str(&format!(", top {} of {} categories", self.categories.len().saturating_sub(1), self.distinct_categories));
        }
        if let Some(u) = unit.filter(|u| !u.is_empty() && s.agg != ChartAgg::Count) {
            t.push_str(&format!(" ({})", u));
        }
        t
    }

    /// Measure `i` as a table (0 = the primary one).
    pub fn measure(&self, i: usize) -> Option<MeasureData> {
        if i == 0 {
            Some(MeasureData { name: self.measure_names.first().cloned().unwrap_or_default(), values: self.values.clone(), rows: self.rows.clone() })
        } else {
            self.extra.get(i - 1).cloned()
        }
    }

    pub fn measure_count(&self) -> usize {
        1 + self.extra.len()
    }

    /// True when there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.empty_reason().is_some()
    }

    /// The reason there is nothing to chart, for "No data to chart: <reason>".
    pub fn empty_reason(&self) -> Option<String> {
        use super::spec::ChartKind;
        match self.spec.kind {
            ChartKind::Scatter | ChartKind::Bubble => {
                if self.points.is_empty() {
                    return Some("no row has both an x and a y number".to_string());
                }
                return None;
            }
            ChartKind::BoxPlot => {
                if self.boxes.is_empty() {
                    return Some("no category has a number to summarise".to_string());
                }
                return None;
            }
            _ => {}
        }
        if self.categories.is_empty() {
            Some("the tab has no rows after its steps".to_string())
        } else if self.values.iter().all(|r| r.iter().all(|v| v.is_none())) {
            Some("every group is blank (no values to add up)".to_string())
        } else {
            None
        }
    }
}

fn format_count(n: u64) -> String {
    crate::models::format_with_commas(n as f64, 0)
}

/// A table of numbers: `[series][category]`.
pub type Matrix = Vec<Vec<Option<f64>>>;

fn sum_of<'a>(it: impl Iterator<Item = &'a Option<f64>>) -> Option<f64> {
    let mut any = false;
    let mut t = 0.0;
    for v in it.flatten() {
        any = true;
        t += *v;
    }
    any.then_some(t)
}

/// The values as percents (0 to 100) of a total, with the notes about groups that could not be done.
/// A group whose total is zero has no percentage (blank, with a note) instead of a value that is made up.
pub fn shares(values: &Matrix, t: Transform) -> (Matrix, Vec<String>) {
    let mut notes = Vec::new();
    let n_series = values.len();
    let n_cat = values.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut out: Matrix = vec![vec![None; n_cat]; n_series];
    let total_all = sum_of(values.iter().flat_map(|r| r.iter()));
    let any_negative = values.iter().flatten().flatten().any(|v| *v < 0.0);
    let mut zero_groups = 0usize;
    for (si, row) in values.iter().enumerate() {
        let row_total = sum_of(row.iter());
        for (ci, v) in row.iter().enumerate() {
            let Some(v) = v else { continue };
            let base = match t {
                Transform::ShareOfTotal => total_all,
                Transform::ShareOfSeries => row_total,
                Transform::ShareOfCategory => sum_of(values.iter().map(|r| r.get(ci).unwrap_or(&None))),
                _ => None,
            };
            match base {
                Some(b) if b != 0.0 => out[si][ci] = Some(v / b * 100.0),
                _ => zero_groups += 1,
            }
        }
    }
    if zero_groups > 0 {
        notes.push(format!("{} value(s) are blank as a percentage because the total they are a share of is zero.", zero_groups));
    }
    if any_negative && matches!(t, Transform::ShareOfTotal | Transform::ShareOfSeries | Transform::ShareOfCategory) {
        notes.push("The values include negatives, so a share can be below 0% or above 100%.".to_string());
    }
    (out, notes)
}

/// A running sum along the categories, per series. A blank value stays blank and does not stop the sum.
pub fn running_sum(values: &Matrix) -> Matrix {
    values
        .iter()
        .map(|row| {
            let mut acc = 0.0;
            row.iter()
                .map(|v| {
                    v.map(|x| {
                        acc += x;
                        acc
                    })
                })
                .collect()
        })
        .collect()
}

/// Applies a transform to a matrix (`None` leaves it as is).
pub fn apply_transform(values: &Matrix, t: Transform) -> (Matrix, Vec<String>) {
    match t {
        Transform::None => (values.clone(), Vec::new()),
        Transform::RunningSum => (running_sum(values), Vec::new()),
        _ => shares(values, t),
    }
}

// ------------------------------------------------------------------------------------------ waterfall

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepKind {
    Start,
    Delta,
    End,
}

/// One bar of a waterfall. A bar spans `base` to `top` (`base <= top`); a delta bar floats.
#[derive(Clone, Debug, PartialEq)]
pub struct WaterfallStep {
    pub label: String,
    pub kind: StepKind,
    /// The step as given: the total for a start or end, the signed change for a delta.
    pub value: f64,
    /// The lower and upper edge of the bar.
    pub base: f64,
    pub top: f64,
    /// The running level after this step.
    pub level: f64,
}

/// The steps of a waterfall from a start total, the signed changes and (optionally) the end total the
/// changes must reach. With an end given that does not equal the start plus the changes, the chart is
/// refused: a bridge that does not add up is a wrong number on a slide.
pub fn waterfall_steps(start: (&str, f64), deltas: &[(String, f64)], end: Option<(&str, f64)>) -> Result<Vec<WaterfallStep>> {
    let mut out = vec![WaterfallStep { label: start.0.to_string(), kind: StepKind::Start, value: start.1, base: start.1.min(0.0), top: start.1.max(0.0), level: start.1 }];
    let mut level = start.1;
    for (label, d) in deltas {
        let after = level + d;
        out.push(WaterfallStep { label: label.clone(), kind: StepKind::Delta, value: *d, base: level.min(after), top: level.max(after), level: after });
        level = after;
    }
    let (end_label, end_value) = match end {
        Some((l, v)) => {
            let tol = 1e-6 * v.abs().max(level.abs()).max(1.0);
            if (v - level).abs() > tol {
                bail!(
                    "The waterfall does not add up: {} {} plus the steps {} is {}, but {} is {}. Check the steps (a missing or doubled one) or the totals.",
                    start.0,
                    fmt2(start.1),
                    fmt2(level - start.1),
                    fmt2(level),
                    l,
                    fmt2(v)
                );
            }
            (l.to_string(), v)
        }
        None => ("Total".to_string(), level),
    };
    out.push(WaterfallStep { label: end_label, kind: StepKind::End, value: end_value, base: end_value.min(0.0), top: end_value.max(0.0), level: end_value });
    Ok(out)
}

fn fmt2(v: f64) -> String {
    crate::models::format_with_commas(v, 2)
}

/// The waterfall of a one-series chart table: the categories in chart order are the steps (see `WaterfallMode`).
pub fn waterfall_from_data(data: &ChartData) -> Result<Vec<WaterfallStep>> {
    let Some(row) = data.values.first() else { bail!("No data to chart: the waterfall has no values.") };
    let mode = data.spec.waterfall;
    let min = match mode {
        WaterfallMode::StartStepsEnd => 3,
        WaterfallMode::StartSteps => 2,
    };
    if data.categories.len() < min || row.len() < min {
        let what = if min == 3 { "a start, a step and an end" } else { "a start and a step" };
        bail!("A waterfall needs at least {} categories ({}), but the chart has {}.", min, what, data.categories.len());
    }
    let mut vals = Vec::new();
    for (c, v) in data.categories.iter().zip(row.iter()) {
        match v {
            Some(x) => vals.push((c.clone(), *x)),
            None => bail!("The waterfall step '{}' has no value. Remove it or fill the blank.", c),
        }
    }
    let start = (vals[0].0.as_str(), vals[0].1);
    match mode {
        WaterfallMode::StartStepsEnd => {
            let last = vals.len() - 1;
            waterfall_steps(start, &vals[1..last], Some((vals[last].0.as_str(), vals[last].1)))
        }
        WaterfallMode::StartSteps => waterfall_steps(start, &vals[1..], None),
    }
}

// ------------------------------------------------------------------------------------------ heatmap

/// A heatmap's matrix: rows are the series, columns the categories.
#[derive(Clone, Debug, PartialEq)]
pub struct HeatMatrix {
    pub rows: Vec<String>,
    pub cols: Vec<String>,
    pub cells: Matrix,
    /// The rows behind each cell (0 = no data, drawn blank).
    pub counts: Vec<Vec<u64>>,
    pub row_totals: Vec<Option<f64>>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub notes: Vec<String>,
}

/// The heatmap matrix of a chart table, with the transform applied ("% of row" is `Transform::ShareOfSeries`).
pub fn heatmap(data: &ChartData, t: Transform) -> HeatMatrix {
    let (cells, notes) = apply_transform(&data.values, t);
    let row_totals = data.values.iter().map(|r| sum_of(r.iter())).collect();
    let flat: Vec<f64> = cells.iter().flatten().flatten().copied().collect();
    let min = flat.iter().copied().fold(None, |a: Option<f64>, v| Some(a.map_or(v, |x| x.min(v))));
    let max = flat.iter().copied().fold(None, |a: Option<f64>, v| Some(a.map_or(v, |x| x.max(v))));
    HeatMatrix { rows: data.series.clone(), cols: data.categories.clone(), cells, counts: data.rows.clone(), row_totals, min, max, notes }
}

/// The step (0 to 12) of the sequential ramp for a value between `lo` and `hi`.
pub fn sequential_step(v: f64, lo: f64, hi: f64) -> usize {
    let top = super::tokens::SEQUENTIAL.len() - 1;
    if !(hi > lo) {
        return top / 2;
    }
    (((v - lo) / (hi - lo)).clamp(0.0, 1.0) * top as f64).round() as usize
}
