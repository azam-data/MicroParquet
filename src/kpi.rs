//! KPI cards (Phase 10 D2): a card's saved settings, the worker's calculation of its number, and how the number is shown.
//!
//! The number is the measure at the model's as-of date and period, worked out by the pivot's own code (`build_pivot` with a
//! Calendar Month-Year row), so a card shows the number the pivot shows. A number that can't be worked out is `None` with its
//! reason, never 0. The unit changes the display, never the stored number.

use crate::dashboard::{pivot_spec_with, PageFilter};
use crate::models::*;
use crate::target::{self, Bands, Direction, Rag};
use anyhow::{bail, Context, Result};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

/// What a card shows.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum KpiSource {
    /// A named measure of the model, by name.
    Measure(String),
    /// A time value of a column of a data tab (by project key).
    Value { tab: String, value: PivotValue },
}

impl Default for KpiSource {
    fn default() -> Self {
        KpiSource::Measure(String::new())
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KpiUnit {
    /// Crore for money, Percent for a ratio (or the dashboard's own default for money).
    #[default]
    Auto,
    Crore,
    Lakh,
    Million,
    Absolute,
    Percent,
    /// Percentage points.
    Pp,
    /// The stored number as it is.
    AsIs,
}

impl KpiUnit {
    pub const ALL: [KpiUnit; 8] = [KpiUnit::Auto, KpiUnit::Crore, KpiUnit::Lakh, KpiUnit::Million, KpiUnit::Absolute, KpiUnit::Percent, KpiUnit::Pp, KpiUnit::AsIs];
    pub fn label(self) -> &'static str {
        match self {
            KpiUnit::Auto => "Automatic",
            KpiUnit::Crore => "Crore",
            KpiUnit::Lakh => "Lakh",
            KpiUnit::Million => "Million",
            KpiUnit::Absolute => "Absolute",
            KpiUnit::Percent => "Percent",
            KpiUnit::Pp => "Percentage points",
            KpiUnit::AsIs => "As is",
        }
    }
    /// The suffix shown after the number.
    fn suffix(self) -> &'static str {
        match self {
            KpiUnit::Crore => " Cr",
            KpiUnit::Lakh => " Lakh",
            KpiUnit::Million => " M",
            KpiUnit::Percent => "%",
            KpiUnit::Pp => " pp",
            KpiUnit::Auto | KpiUnit::Absolute | KpiUnit::AsIs => "",
        }
    }
    /// What the stored number is divided by.
    fn divisor(self) -> f64 {
        match self {
            KpiUnit::Crore => 10_000_000.0,
            KpiUnit::Lakh => 100_000.0,
            KpiUnit::Million => 1_000_000.0,
            _ => 1.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KpiCompare {
    #[default]
    None,
    PriorPeriod,
    Sply,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GoodDirection {
    #[default]
    Up,
    Down,
    Neutral,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ThresholdOp {
    #[default]
    Below,
    Above,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ThresholdColour {
    #[default]
    Amber,
    Red,
    Green,
}

/// "below 80.00 → amber": the owner's number, in the card's unit.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Threshold {
    pub op: ThresholdOp,
    pub limit: f64,
    pub colour: ThresholdColour,
}

/// A card's saved settings. Every field is `#[serde(default)]`, so a card saved by D0 loads in D2.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct KpiSpec {
    pub title: String,
    pub source: KpiSource,
    pub unit: KpiUnit,
    pub decimals: usize,
    pub compare: KpiCompare,
    pub good_direction: GoodDirection,
    /// At most `MAX_THRESHOLDS`; the first that matches colours the number.
    pub thresholds: Vec<Threshold>,
    pub sparkline: bool,
    /// Phase 12: the window of a measure card (Period, MTD, QTD, YTD, rolling 3 or 12 months). A card on a time value of a column
    /// has its window in the value itself.
    pub window: TimeWindow,
    /// Phase 12: a moving average of the measure over the last periods (needs no comparison).
    pub smooth: Smooth,
    /// Phase 12: a target with red/amber/green bands; the bands decide the colour of the number (they replace the limits).
    pub target: Option<KpiTarget>,
    /// Phase 12: how the card draws the number: as text, as a bullet bar or as a gauge.
    pub visual: KpiVisual,
}

impl Default for KpiSpec {
    fn default() -> Self {
        KpiSpec {
            title: String::new(),
            source: KpiSource::default(),
            unit: KpiUnit::Auto,
            decimals: 2,
            compare: KpiCompare::None,
            good_direction: GoodDirection::Up,
            thresholds: Vec::new(),
            sparkline: false,
            window: TimeWindow::Period,
            smooth: Smooth::None,
            target: None,
            visual: KpiVisual::Number,
        }
    }
}

/// How a card draws its number.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KpiVisual {
    /// The number, its change and its sparkline.
    #[default]
    Number,
    /// A horizontal bar for the number on a track of red, amber and green bands, with a mark at the target.
    Bullet,
    /// A half-circle dial of the three bands with a needle at the number and a mark at the target.
    Gauge,
}

impl KpiVisual {
    pub const ALL: [KpiVisual; 3] = [KpiVisual::Number, KpiVisual::Bullet, KpiVisual::Gauge];
    pub fn label(self) -> &'static str {
        match self {
            KpiVisual::Number => "Number",
            KpiVisual::Bullet => "Bullet bar",
            KpiVisual::Gauge => "Gauge",
        }
    }
}

/// Where a card's target comes from.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum TargetSource {
    /// A number typed in the card's unit (5,000 with Crore = 5,000 Cr).
    Fixed(f64),
    /// A measure of the same tab (a Phase 11 plan measure such as the budget), worked out for the card's period and window.
    Measure(String),
}

impl Default for TargetSource {
    fn default() -> Self {
        TargetSource::Fixed(0.0)
    }
}

/// A card's target and how far from it is still green, then amber.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct KpiTarget {
    pub source: TargetSource,
    pub bands: Bands,
}

pub const MAX_THRESHOLDS: usize = 2;
/// The sparkline shows this many periods of the model calendar.
pub const SPARK_PERIODS: usize = 12;

// -------------------------------------------------------------------------------------- the number

/// What the worker found for a card. The numbers are the stored ones (BDT, or percent for a ratio).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KpiValue {
    pub current: Option<f64>,
    pub earlier: Option<f64>,
    /// Why `current` is blank ("No balance in Nov 2026 for 'Accounts' (last date 31 Oct 2026)").
    pub reason: Option<String>,
    /// The measure for the last 12 periods, oldest first; a period that can't be worked out is `None` (a gap, never 0).
    pub spark: Vec<Option<f64>>,
    pub spark_labels: Vec<String>,
    /// The period the number is for ("Oct 2026").
    pub period: String,
    /// The pivot's notes (the month-end note among them).
    pub notes: Vec<String>,
    pub is_ratio: bool,
    /// The target worked out from a measure (stored units); a typed target is not here (it is in the card's own unit).
    pub target: Option<f64>,
    /// Why the target measure has no number for the period.
    pub target_reason: Option<String>,
}

pub struct KpiRequest {
    pub spec: KpiSpec,
    /// The project key of the data tab.
    pub tab_key: String,
    pub base: TabSource,
    pub steps: Vec<TransformStep>,
    pub picks: Vec<(PageFilter, Vec<String>)>,
    pub temp_dir: std::path::PathBuf,
}

/// The tab a card reads: a measure's tab, or the value's.
pub fn source_tab(source: &KpiSource) -> Result<String> {
    match source {
        KpiSource::Measure(name) => crate::measures::find_measure(name).map(|m| m.tab().to_string()).with_context(|| format!("The measure '{}' is not in the model. Pick another in the card's settings.", name)),
        KpiSource::Value { tab, .. } => Ok(tab.clone()),
    }
}

fn month_start(d: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap_or(d)
}

fn months_back(first: NaiveDate, n: i32) -> NaiveDate {
    let total = first.year() * 12 + first.month0() as i32 - n;
    NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1).unwrap_or(first)
}

fn value_of(spec: &KpiSpec, source: &KpiSource, shift: TimeShift) -> PivotValue {
    match source {
        KpiSource::Measure(name) => PivotValue { field: name.clone(), agg: PivotAgg::Sum, weight_col: None, time: Some(TimeValue { agg: TimeAgg::Measure, shift, show: TimeShow::Value, window: spec.window, smooth: spec.smooth, ..Default::default() }) },
        KpiSource::Value { value, .. } => {
            let mut v = value.clone();
            let t = v.time.unwrap_or_default();
            // a card on a column keeps the window and the moving average chosen in the value itself
            v.time = Some(TimeValue { shift, show: TimeShow::Value, ..t });
            v
        }
    }
}

/// The number of a card, by the pivot's own code. Runs on the worker, inside the model scope.
pub fn compute_kpi(req: &KpiRequest) -> Result<KpiValue> {
    let Some(ctx) = crate::model::scope() else {
        bail!("{}", crate::model::outside_project_message("a KPI card"));
    };
    let Some(cal) = ctx.calendar.clone() else {
        bail!("A KPI card needs the model calendar. Turn it on and link the tab's date column in Model \u{25b8} Calendar\u{2026}.");
    };
    let Some(as_of) = cal.as_of else {
        bail!("The as-of date is not known yet (the dates of the tabs are still being read). Try again in a moment, or set the date in Model \u{25b8} As-of date\u{2026}.");
    };
    let Some(link) = cal.date_links.iter().find(|l| l.data_tab == req.tab_key) else {
        bail!("The tab '{}' is not linked to the calendar. Link its date column in Model \u{25b8} Calendar\u{2026}.", cal.title_of(&req.tab_key));
    };
    let field = crate::model::ModelFieldRef::new(&crate::model_calendar::date_link(&req.tab_key, &link.column, &cal.tab), &format!("{} \u{2192} Calendar", cal.title_of(&req.tab_key)), "Calendar", "Month-Year");
    let this = month_start(as_of);
    let labels: Vec<String> = (0..SPARK_PERIODS as i32).rev().map(|k| months_back(this, k).format("%b %Y").to_string()).collect();
    let period = labels.last().cloned().unwrap_or_default();
    let shown: Vec<String> = if req.spec.sparkline { labels.clone() } else { vec![period.clone()] };
    let shift = match req.spec.compare {
        KpiCompare::None => None,
        KpiCompare::PriorPeriod => Some(TimeShift::PriorPeriod),
        KpiCompare::Sply => Some(TimeShift::Sply),
    };
    let smoothed = match &req.spec.source {
        KpiSource::Measure(_) => req.spec.smooth != Smooth::None,
        KpiSource::Value { value, .. } => value.time.is_some_and(|t| t.smooth != Smooth::None),
    };
    if smoothed && shift.is_some() {
        bail!("A moving average can't be compared with an earlier period. Set \"Compare with\" to Nothing, or take the moving average off.");
    }
    let mut values = vec![value_of(&req.spec, &req.spec.source, TimeShift::None)];
    if let Some(s) = shift {
        values.push(value_of(&req.spec, &req.spec.source, s));
    }
    // Phase 12: a target taken from a measure (a budget or target of the plan tables) for the same period and window
    let target_j = match req.spec.target.as_ref().map(|t| &t.source) {
        Some(TargetSource::Measure(name)) => {
            let Some(m) = crate::measures::find_measure(name) else {
                bail!("The target measure '{}' is not in the model. Pick another in the card's settings.", name);
            };
            if m.tab() != req.tab_key {
                bail!("The target measure '{}' belongs to another tab than the card's measure. Pick a measure of the same tab (a plan measure of it, for example).", name);
            }
            values.push(value_of(&req.spec, &KpiSource::Measure(name.clone()), TimeShift::None));
            Some(values.len() - 1)
        }
        _ => None,
    };
    let spec = PivotSpec {
        rows: vec![field.name.clone()],
        values,
        filters: vec![PivotFilter { field: field.name.clone(), values: shown.iter().map(|l| Some(l.clone())).collect() }],
        model_fields: vec![field.clone()],
        data_tab: req.tab_key.clone(),
        ..Default::default()
    };
    let spec = pivot_spec_with(&spec, &req.picks);
    let lf = crate::engine::compile_tab_lazy(&req.base, &req.steps, &req.temp_dir)?;
    let build = crate::pivot_table::build_pivot(lf, &spec)?;
    let names: Vec<String> = build.df.get_column_names().iter().map(|n| n.to_string()).collect();
    let label_col = build.df.column(names.first().map(String::as_str).unwrap_or(""))?.cast(&polars::prelude::DataType::String)?;
    let labels_found: Vec<Option<String>> = label_col.str()?.into_iter().map(|s| s.map(|x| x.to_string())).collect();
    let col_of = |j: usize| -> Option<String> { build.col_values.iter().position(|v| *v == j).and_then(|k| names.get(build.label_columns + k).cloned()) };
    let cell = |label: &str, j: usize| -> (Option<f64>, Option<String>) {
        let Some(row) = labels_found.iter().position(|l| l.as_deref() == Some(label)) else {
            return (None, Some(format!("No value for {}", label)));
        };
        let Some(col) = col_of(j) else { return (None, Some("The value is not in the table".to_string())) };
        let v = build.df.column(col.as_str()).ok().and_then(|c| c.cast(&polars::prelude::DataType::Float64).ok()).and_then(|c| c.f64().ok().and_then(|f| f.get(row)));
        let why = build.notes.iter().find(|n| n.row == row && n.column == col).map(|n| n.text.clone());
        (v, if v.is_none() { why.or_else(|| Some(format!("No value for {}", label))) } else { None })
    };
    let (current, mut reason) = cell(&period, 0);
    // the month has no rows at all, so the pivot has no row to explain it: say it as the pivot does for a blank cell
    if current.is_none() && !labels_found.iter().any(|l| l.as_deref() == Some(period.as_str())) {
        let lf = crate::engine::compile_tab_lazy(&req.base, &req.steps, &req.temp_dir)?;
        let last = crate::model_calendar::snapshot_counts(lf, &link.column).ok().and_then(|c| c.into_iter().map(|(d, _)| d).filter(|d| *d <= as_of).max());
        let balance = match &req.spec.source {
            KpiSource::Measure(n) => matches!(crate::measures::find_measure(n), Some(crate::measures::NamedMeasure::Base { aggregation, .. }) if !crate::measures::is_flow(aggregation)),
            KpiSource::Value { value, .. } => value.time.is_some_and(|t| !crate::measures::is_flow(t.agg)),
        };
        reason = Some(format!("No {} in {} for '{}'{}", if balance { "balance" } else { "data" }, period, cal.title_of(&req.tab_key), last.map(|d| format!(" (last date {})", crate::model_calendar::long_date(d))).unwrap_or_default()));
    }
    let earlier = if shift.is_some() { cell(&period, 1).0 } else { None };
    let (target, target_reason) = match target_j {
        Some(j) => cell(&period, j),
        None => (None, None),
    };
    let spark: Vec<Option<f64>> = if req.spec.sparkline { labels.iter().map(|l| cell(l, 0).0).collect() } else { Vec::new() };
    let is_ratio = match &req.spec.source {
        KpiSource::Measure(n) => crate::measures::find_measure(n).is_some_and(|m| m.is_ratio()),
        KpiSource::Value { .. } => false,
    };
    // Phase 12: a card whose measure is changed by the active scenario says so beside its period, in every output
    let scenario = match (&ctx.what_if, &req.spec.source) {
        (Some(w), KpiSource::Measure(n)) if current.is_some() && w.role_of(n).is_some() => format!(" \u{b7} Scenario {} ({})", w.name, crate::whatif::Scenario { name: String::new(), rate_shift_bp: w.rate_shift_bp, growth_pct: w.growth_pct }.words()),
        _ => String::new(),
    };
    Ok(KpiValue { current, earlier, reason, spark, spark_labels: if req.spec.sparkline { labels } else { Vec::new() }, period: format!("{}{}", period, scenario), notes: build.warnings, is_ratio, target, target_reason })
}

// -------------------------------------------------------------------------------------- how it is shown

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Good,
    Bad,
    Neutral,
}

/// What a card draws and says. The colour is never the only signal: the arrow and the words carry it too.
#[derive(Clone, Debug, PartialEq)]
pub struct KpiView {
    /// "4,825.00 Cr", or "\u{2014}" when the number can't be worked out.
    pub value_text: String,
    /// "+125.00 Cr \u{b7} +2.66%", "+0.50 pp", or "" without a comparison.
    pub change_text: String,
    /// "\u{25b2}", "\u{25bc}", "\u{25ba}" or "" without a comparison.
    pub arrow: &'static str,
    /// "up", "down" or "unchanged" (or "" without a comparison).
    pub words: &'static str,
    pub tone: Tone,
    pub threshold: Option<ThresholdColour>,
    /// Why the number is blank.
    pub reason: Option<String>,
    /// The sparkline in the card's unit, with gaps.
    pub spark: Vec<Option<f64>>,
    /// The words for a tooltip and the alt text of the HTML and the PPTX.
    pub alt: String,
    /// Phase 12: "Target 5,000.00 Cr \u{b7} 97.2% of target \u{b7} amber", or why there is no target number; empty without a target.
    pub target_text: String,
    /// Phase 12: the bullet bar or gauge, when the card has a target and draws one.
    pub graphic: Option<Graphic>,
}

/// What a bullet bar or a gauge draws, in the card's unit. Every renderer (screen, HTML, PDF, PPTX) draws from this one description.
#[derive(Clone, Debug, PartialEq)]
pub struct Graphic {
    pub kind: KpiVisual,
    pub lo: f64,
    pub hi: f64,
    pub value: f64,
    pub target: f64,
    /// (from, to, colour) over `lo..hi`.
    pub bands: Vec<(f64, f64, Rag)>,
    pub lo_text: String,
    pub hi_text: String,
    pub value_text: String,
    pub target_text: String,
}

impl Graphic {
    /// Where `x` stands on the axis, 0 at `lo` and 1 at `hi`.
    pub fn frac(&self, x: f64) -> f64 {
        if self.hi > self.lo {
            ((x - self.lo) / (self.hi - self.lo)).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// The outline of a ring segment of a gauge from `f0` to `f1` (0 = the left end of the half circle, 1 = the right end), as points
/// of a polygon: the outer arc one way and the inner arc back. Centre (cx, cy), radii `r_out` and `r_in`; y grows downwards.
pub fn ring_segment(cx: f64, cy: f64, r_out: f64, r_in: f64, f0: f64, f1: f64, steps: usize) -> Vec<(f64, f64)> {
    let at = |f: f64, r: f64| {
        let a = std::f64::consts::PI * (1.0 - f);
        (cx + r * a.cos(), cy - r * a.sin())
    };
    let n = steps.max(2);
    let mut pts = Vec::with_capacity(2 * n + 2);
    for i in 0..=n {
        pts.push(at(f0 + (f1 - f0) * i as f64 / n as f64, r_out));
    }
    for i in (0..=n).rev() {
        pts.push(at(f0 + (f1 - f0) * i as f64 / n as f64, r_in));
    }
    pts
}

pub fn rag_colour(r: Rag) -> ThresholdColour {
    match r {
        Rag::Green => ThresholdColour::Green,
        Rag::Amber => ThresholdColour::Amber,
        Rag::Red => ThresholdColour::Red,
    }
}

/// The unit a card shows in: its own, else Crore for money (or the dashboard's default) and Percent for a ratio.
pub fn resolved_unit(spec: &KpiSpec, is_ratio: bool, money_default: KpiUnit) -> KpiUnit {
    match spec.unit {
        KpiUnit::Auto if is_ratio => KpiUnit::Percent,
        KpiUnit::Auto => match money_default {
            KpiUnit::Auto | KpiUnit::Percent | KpiUnit::Pp => KpiUnit::Crore,
            u => u,
        },
        u => u,
    }
}

fn signed(v: f64, decimals: usize) -> String {
    let t = format_with_commas(v.abs(), decimals);
    if v < 0.0 && t.chars().any(|c| c.is_ascii_digit() && c != '0') {
        format!("\u{2212}{}", t)
    } else {
        format!("+{}", t)
    }
}

pub fn view(spec: &KpiSpec, v: &KpiValue, money_default: KpiUnit) -> KpiView {
    let unit = resolved_unit(spec, v.is_ratio, money_default);
    let dec = spec.decimals;
    let div = unit.divisor();
    let show = |x: f64| format!("{}{}", format_with_commas(x / div, dec), unit.suffix());
    let ratio_like = matches!(unit, KpiUnit::Percent | KpiUnit::Pp) || v.is_ratio;
    let value_text = v.current.map(show).unwrap_or_else(|| "\u{2014}".to_string());
    // the change: money in the unit and a percent; a ratio in percentage points
    let mut change_text = String::new();
    let (mut arrow, mut words, mut tone) = ("", "", Tone::Neutral);
    if spec.compare != KpiCompare::None {
        if let (Some(c), Some(e)) = (v.current, v.earlier) {
            let diff = c - e;
            let shown_diff = diff / if ratio_like { 1.0 } else { div };
            let rounded_zero = format_with_commas(shown_diff.abs(), dec).chars().all(|ch| !ch.is_ascii_digit() || ch == '0');
            (arrow, words) = if rounded_zero { ("\u{25ba}", "unchanged") } else if diff > 0.0 { ("\u{25b2}", "up") } else { ("\u{25bc}", "down") };
            tone = match (spec.good_direction, words) {
                (GoodDirection::Neutral, _) | (_, "unchanged") => Tone::Neutral,
                (GoodDirection::Up, "up") | (GoodDirection::Down, "down") => Tone::Good,
                _ => Tone::Bad,
            };
            change_text = if ratio_like {
                format!("{} pp", signed(shown_diff, dec))
            } else {
                let pct = if e == 0.0 { "\u{2014}".to_string() } else { format!("{}%", signed(diff / e.abs() * 100.0, 2)) };
                format!("{}{} \u{b7} {}", signed(shown_diff, dec), unit.suffix(), pct)
            };
        } else {
            change_text = "\u{2014}".to_string();
        }
    }
    let threshold = v.current.and_then(|c| {
        let in_unit = c / div;
        spec.thresholds.iter().take(MAX_THRESHOLDS).find(|t| match t.op {
            ThresholdOp::Below => in_unit < t.limit,
            ThresholdOp::Above => in_unit > t.limit,
        })
    });
    let threshold = threshold.map(|t| t.colour);
    // Phase 12: a target replaces the limits: its bands decide the colour, and the card can draw them
    let (threshold, target_text, graphic) = match &spec.target {
        None => (threshold, String::new(), None),
        Some(t) => {
            let stored = match &t.source {
                TargetSource::Fixed(x) => Some(*x * div),
                TargetSource::Measure(_) => v.target,
            };
            let dir = match spec.good_direction {
                GoodDirection::Up => Some(Direction::Up),
                GoodDirection::Down => Some(Direction::Down),
                GoodDirection::Neutral => None,
            };
            match (stored, v.current, dir) {
                (_, _, None) => (threshold, "Target bands need \"Up is good\" or \"Down is good\"; \"No judgement\" has no good side.".to_string(), None),
                (_, _, Some(_)) if t.bands.check().is_err() => (threshold, t.bands.check().err().unwrap_or_default(), None),
                (None, _, _) => (threshold, format!("No target: {}", v.target_reason.clone().unwrap_or_else(|| "pick a target number or measure".to_string())), None),
                (Some(tg), None, Some(_)) => (threshold, format!("Target {}", show(tg)), None),
                (Some(tg), Some(cur), Some(dir)) => {
                    let rag = target::status(cur, tg, dir, &t.bands);
                    let pct = target::percent_of_target(cur, tg).map(|p| format!(" \u{b7} {}% of target", format_with_commas(p, 1))).unwrap_or_default();
                    let text = format!("Target {}{} \u{b7} {}", show(tg), pct, rag.word());
                    let graphic = match spec.visual {
                        KpiVisual::Number => None,
                        kind => Some(graphic_of(kind, cur, tg, dir, &t.bands, div, dec, unit)),
                    };
                    (Some(rag_colour(rag)), text, graphic)
                }
            }
        }
    };
    let alt = {
        let mut a = format!("{}: {}", if spec.title.is_empty() { "KPI" } else { &spec.title }, value_text);
        if !words.is_empty() {
            a.push_str(&format!(", {} {}", words, change_text));
        }
        if let Some(t) = threshold {
            a.push_str(&format!(", {} by your limit", match t { ThresholdColour::Amber => "amber", ThresholdColour::Red => "red", ThresholdColour::Green => "green" }));
        }
        if !target_text.is_empty() {
            a.push_str(&format!(", {}", target_text.to_lowercase()));
        }
        if let Some(r) = &v.reason {
            a.push_str(&format!(" ({})", r));
        }
        a
    };
    KpiView { value_text, change_text, arrow, words, tone, threshold, reason: if v.current.is_none() { v.reason.clone() } else { None }, spark: v.spark.iter().map(|x| x.map(|y| y / div)).collect(), alt, target_text, graphic }
}

/// The axis, bands and marks of a bullet bar or gauge, in the card's unit.
#[allow(clippy::too_many_arguments)]
fn graphic_of(kind: KpiVisual, cur: f64, tg: f64, dir: Direction, b: &Bands, div: f64, dec: usize, unit: KpiUnit) -> Graphic {
    let (value, target) = (cur / div, tg / div);
    let size = if target != 0.0 { target.abs() } else if value != 0.0 { value.abs() } else { 1.0 };
    let lo = (0.0f64).min(value).min(target - size);
    let hi = (value + 0.05 * size).max(target + size * (0.3f64).max(b.amber_within_pct / 100.0 + 0.15)).max(lo + size);
    // the bands are of the target in the card's unit, so they scale with it
    let bands = target::bands_over(lo, hi, target, dir, b);
    let text = |x: f64| format!("{}{}", format_with_commas(x, dec), unit.suffix());
    Graphic { kind, lo, hi, value, target, bands, lo_text: text(lo), hi_text: text(hi), value_text: text(value), target_text: text(target) }
}
