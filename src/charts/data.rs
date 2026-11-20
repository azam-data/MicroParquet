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

/// A measure that is the same in every series (the panel is `single`): one value per category, the first the
/// series has, and the rows behind it taken from the series that gave it.
pub fn collapse_series(values: &[Vec<Option<f64>>], rows: &[Vec<u64>], n_cat: usize) -> (Vec<Option<f64>>, Vec<u64>) {
    let mut vals = vec![None; n_cat];
    let mut rws = vec![0u64; n_cat];
    for c in 0..n_cat {
        for (s, r) in values.iter().enumerate() {
            if let Some(Some(v)) = r.get(c) {
                vals[c] = Some(*v);
                rws[c] = rows.get(s).and_then(|x| x.get(c)).copied().unwrap_or(0);
                break;
            }
        }
    }
    (vals, rws)
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

    /// The first number in the chart that is not finite (NaN or infinity), as a plain sentence; every renderer
    /// refuses such a chart, so nothing that is not a number is ever drawn or written to a file.
    pub fn non_finite(&self) -> Option<String> {
        let grid = |m: &[Vec<Option<f64>>], what: &str| -> Option<String> {
            for (s, row) in m.iter().enumerate() {
                for (c, v) in row.iter().enumerate() {
                    if v.is_some_and(|x| !x.is_finite()) {
                        let cat = self.categories.get(c).map(String::as_str).unwrap_or("?");
                        let ser = self.series.get(s).filter(|n| !n.is_empty()).map(|n| format!(" and series '{}'", n)).unwrap_or_default();
                        return Some(format!("'{}' is not a finite number for the category '{}'{}", what, cat, ser));
                    }
                }
            }
            None
        };
        let name0 = self.measure_names.first().cloned().unwrap_or_default();
        if let Some(m) = grid(&self.values, &name0) {
            return Some(m);
        }
        for e in &self.extra {
            if let Some(m) = grid(&e.values, &e.name) {
                return Some(m);
            }
        }
        if let Some(z) = &self.size {
            if let Some(m) = grid(&z.values, &z.name) {
                return Some(m);
            }
        }
        if let Some(p) = self.points.iter().find(|p| !p.x.is_finite() || !p.y.is_finite() || p.size.is_some_and(|z| !z.is_finite())) {
            return Some(format!("a point of '{}' is not a finite number (x {}, y {})", self.spec.category, p.x, p.y));
        }
        if let Some(b) = self.boxes.iter().find(|b| ![b.min, b.q1, b.median, b.q3, b.max, b.low_whisker, b.high_whisker].iter().all(|x| x.is_finite())) {
            return Some(format!("the box of '{}' is not a finite number", b.category));
        }
        if let Some(h) = &self.hist {
            if !h.edges.iter().all(|x| x.is_finite()) {
                return Some("a bin of the histogram is not a finite number".to_string());
            }
        }
        self.facets.iter().find_map(|f| f.data.non_finite().map(|m| format!("panel '{}': {}", f.name, m)))
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

/// A waterfall as three stacked series (the form Excel and ECharts draw): an invisible base, and the
/// visible bar split at zero into its positive and its negative part. `base + pos` is the top of a bar
/// above zero; `base + neg` the bottom of one below it; a bar across zero has base 0.
#[derive(Clone, Debug, PartialEq)]
pub struct WaterfallStack {
    pub base: Vec<f64>,
    pub pos: Vec<f64>,
    pub neg: Vec<f64>,
}

pub fn waterfall_stack(steps: &[WaterfallStep]) -> WaterfallStack {
    let mut out = WaterfallStack { base: Vec::new(), pos: Vec::new(), neg: Vec::new() };
    for s in steps {
        if s.base >= 0.0 {
            out.base.push(s.base);
            out.pos.push(s.top - s.base);
            out.neg.push(0.0);
        } else if s.top <= 0.0 {
            out.base.push(s.top);
            out.pos.push(0.0);
            out.neg.push(s.base - s.top);
        } else {
            out.base.push(0.0);
            out.pos.push(s.top);
            out.neg.push(s.base);
        }
    }
    out
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
    /// The smallest value above zero: the ramp runs from here to `max` (zero and missing cells are neutral).
    pub pos_min: Option<f64>,
    pub notes: Vec<String>,
}

/// How a heatmap cell is filled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeatFill {
    /// Zero, missing or a pair with no rows: the neutral near-surface fill with muted text.
    Neutral,
    /// A value above zero: this step of the sequential ramp (0 to 12, before the dark-theme reversal).
    Step(usize),
}

impl HeatMatrix {
    /// The fill of one cell: the sequential ramp only for values above zero.
    pub fn fill(&self, v: f64, rows: u64) -> HeatFill {
        if rows == 0 || !(v > 0.0) {
            return HeatFill::Neutral;
        }
        HeatFill::Step(sequential_step(v, self.pos_min.unwrap_or(v), self.max.unwrap_or(v)))
    }
}

/// The order of a heatmap's rows or columns: the custom order list when the chart has one; with none, CL
/// grade codes follow the CL order (STD, SMA, SS, DF, BL) and other text sorts naturally. "Other" stays last.
fn heat_order(names: &[String], spec: &ChartSpec, other: &str) -> Vec<usize> {
    use crate::sort_lists::SortOrder;
    let is_other = |s: &str| s == other || s == "Other" || s == "Other categories" || s == "Other series";
    let cl_rank = |s: &str| crate::banking::CL_GRADES.iter().position(|g| g.eq_ignore_ascii_case(s.trim()));
    let named: Vec<&String> = names.iter().filter(|n| !is_other(n)).collect();
    let all_cl = !named.is_empty() && named.iter().all(|n| cl_rank(n).is_some());
    let mut idx: Vec<usize> = (0..names.len()).collect();
    idx.sort_by(|&a, &b| {
        let (x, y) = (&names[a], &names[b]);
        match (is_other(x), is_other(y)) {
            (true, true) => return std::cmp::Ordering::Equal,
            (true, false) => return std::cmp::Ordering::Greater,
            (false, true) => return std::cmp::Ordering::Less,
            _ => {}
        }
        if spec.order.is_natural() && all_cl {
            cl_rank(x).cmp(&cl_rank(y))
        } else {
            let key = |s: &String| if s == BLANK { None } else { Some(s.clone()) };
            let order = if spec.order.is_natural() { &SortOrder::Natural } else { &spec.order };
            order.compare(&key(x), &key(y))
        }
    });
    idx
}

const BLANK: &str = "(blank)";

/// The heatmap matrix of a chart table, with the transform applied ("% of row" is `Transform::ShareOfSeries`).
/// Rows and columns follow the order list (see `heat_order`); every row × column pair is a cell, and a pair
/// with no data is 0.0 (its `counts` entry is 0).
pub fn heatmap(data: &ChartData, t: Transform) -> HeatMatrix {
    let (cells0, notes) = apply_transform(&data.values, t);
    let row_totals0: Vec<Option<f64>> = data.values.iter().map(|r| sum_of(r.iter())).collect();
    let ro = heat_order(&data.series, &data.spec, "");
    let co = heat_order(&data.categories, &data.spec, "");
    let mut cells: Matrix = Vec::new();
    let mut counts: Vec<Vec<u64>> = Vec::new();
    for &r in &ro {
        let mut crow = Vec::new();
        let mut nrow = Vec::new();
        for &c in &co {
            let n = data.rows.get(r).and_then(|x| x.get(c)).copied().unwrap_or(0);
            let v = cells0.get(r).and_then(|x| x.get(c)).copied().flatten();
            // a pair with no rows is a real zero; a pair whose percentage could not be worked out stays blank
            crow.push(if n == 0 { Some(0.0) } else { v });
            nrow.push(n);
        }
        cells.push(crow);
        counts.push(nrow);
    }
    let row_totals = ro.iter().map(|&r| row_totals0.get(r).copied().flatten()).collect();
    let flat: Vec<f64> = cells.iter().flatten().flatten().copied().collect();
    let min = flat.iter().copied().fold(None, |a: Option<f64>, v| Some(a.map_or(v, |x| x.min(v))));
    let max = flat.iter().copied().fold(None, |a: Option<f64>, v| Some(a.map_or(v, |x| x.max(v))));
    let pos_min = flat.iter().copied().filter(|v| *v > 0.0).fold(None, |a: Option<f64>, v| Some(a.map_or(v, |x| x.min(v))));
    HeatMatrix {
        rows: ro.iter().filter_map(|&r| data.series.get(r).cloned()).collect(),
        cols: co.iter().filter_map(|&c| data.categories.get(c).cloned()).collect(),
        cells,
        counts,
        row_totals,
        min,
        max,
        pos_min,
        notes,
    }
}

/// The step (0 to 12) of the sequential ramp for a value between `lo` and `hi`.
pub fn sequential_step(v: f64, lo: f64, hi: f64) -> usize {
    let top = super::tokens::SEQUENTIAL.len() - 1;
    if !(hi > lo) {
        return top / 2;
    }
    (((v - lo) / (hi - lo)).clamp(0.0, 1.0) * top as f64).round() as usize
}
