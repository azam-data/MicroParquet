//! Data labels (Phase 7 review, item 20): the one place that decides which value labels a chart prints and how
//! they are written, for the app, the HTML page, the workbook and the Python script.
//!
//! * which: `Auto` (the design system's selective rule), `All`, `EndsOnly` (the first and last category of each
//!   series) or `None`;
//! * how: the decimals (0 to 4, default the view's), the unit (As in view, Absolute, Thousand, Lakh, Million,
//!   Crore) and whether labels on share views (percentages) are printed;
//! * `All` never prints a label that collides: the app's layout leaves such a label out (`ChartLayout::dropped`),
//!   and the other three renderers leave out the same ones (`dropped_labels`); a note says how many.

use super::axis::{signed_body, ValueKind};
use super::data::ChartData;
use super::layout::{layout_chart, scaled, ApproxMeasure, LayoutOpts};
use super::spec::{ChartSpec, LabelMode};

/// One label that was left out, by panel, series and category.
pub type LabelKey = (usize, usize, usize);

/// The label rules of one chart.
#[derive(Clone, Debug)]
pub struct LabelPlan {
    pub mode: LabelMode,
    pub percent: bool,
    /// Decimals for every label; `None` = the view's (and 1 for a percentage, as always).
    pub decimals: Option<usize>,
    /// How the numbers of labels are scaled and rounded: the view's, with the label unit and decimals on top.
    pub opts: LayoutOpts,
    /// The label unit differs from the view's.
    pub unit_changed: bool,
}

impl LabelPlan {
    pub fn new(spec: &ChartSpec, opts: &LayoutOpts) -> LabelPlan {
        let l = &spec.labels;
        let mut lo = opts.clone();
        let unit_changed = l.unit.divisor().is_some_and(|d| d != opts.divisor);
        if let Some(d) = l.unit.divisor() {
            lo.divisor = d;
        }
        if let Some(d) = l.decimals {
            lo.decimals = d.min(4);
        }
        LabelPlan { mode: l.mode, percent: l.percent, decimals: l.decimals.map(|d| d.min(4)), opts: lo, unit_changed }
    }

    /// True when the labels are drawn exactly as a chart saved before this setting was drawn them.
    pub fn is_default(&self) -> bool {
        self.mode == LabelMode::Auto && self.percent && self.decimals.is_none() && !self.unit_changed
    }

    /// The label at position `i` of `n` (a category, a point, a step): `auto` is what the design system does
    /// without this setting.
    pub fn shows(&self, i: usize, n: usize, auto: bool) -> bool {
        match self.mode {
            LabelMode::Auto => auto,
            LabelMode::All => true,
            LabelMode::EndsOnly => i == 0 || i + 1 == n,
            LabelMode::None => false,
        }
    }

    /// Labels of this kind of number are printed at all (share views can be switched off).
    pub fn kind_on(&self, vk: ValueKind) -> bool {
        self.mode != LabelMode::None && (self.percent || vk != ValueKind::Percent)
    }

    /// A value as a label says it.
    pub fn text(&self, raw: f64, vk: ValueKind, signed: bool) -> String {
        let v = scaled(raw, vk, &self.opts);
        let d = match vk {
            ValueKind::Amount | ValueKind::Plain => self.opts.decimals,
            ValueKind::Percent => self.decimals.unwrap_or(1),
            ValueKind::Count => 0,
        };
        signed_body(v, vk, d, signed)
    }

    /// A value as a signed (diverging) label says it: always a sign, two decimals for a percentage unless set.
    pub fn diverging(&self, raw: f64, vk: ValueKind) -> String {
        let v = scaled(raw, vk, &self.opts);
        let d = match vk {
            ValueKind::Percent => self.decimals.unwrap_or(2),
            ValueKind::Count => 0,
            _ => self.opts.decimals,
        };
        signed_body(v, vk, d, true)
    }

    /// "Data labels are in Lakh" when the label unit is not the view's.
    pub fn unit_note(&self, spec: &ChartSpec) -> Option<String> {
        self.unit_changed.then(|| format!("Data labels are in {}", spec.labels.unit.label()))
    }
}

/// The note under a chart that left labels out: "3 data labels left out where they would overlap".
pub fn dropped_note(n: usize) -> Option<String> {
    (n > 0).then(|| format!("{} data label{} left out where {} would overlap", n, if n == 1 { "" } else { "s" }, if n == 1 { "it" } else { "they" }))
}

/// The labels the app's layout leaves out for this chart at the size of an exported chart (a page of about 1,060 px,
/// a workbook chart of 720 px): the HTML, Excel and Python renderers leave out the same ones.
pub fn dropped_labels(data: &ChartData, opts: &LayoutOpts, width: f32, height: f32) -> Vec<LabelKey> {
    if data.spec.labels.mode != LabelMode::All {
        return Vec::new();
    }
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(width, height));
    layout_chart(data, rect, opts, &ApproxMeasure).dropped
}

/// The extra footnote lines the label setting asks for: the unit, and the labels left out.
pub fn footnote_notes(data: &ChartData, opts: &LayoutOpts, dropped: usize) -> Vec<String> {
    let plan = LabelPlan::new(&data.spec, opts);
    let mut v = Vec::new();
    if plan.kind_on(ValueKind::Amount) {
        v.extend(plan.unit_note(&data.spec));
    }
    v.extend(dropped_note(dropped));
    v
}
