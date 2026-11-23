//! `ChartSpec`: a chart described completely (Phase 7). It grew out of the Phase 6 chart settings:
//! every Phase 6 field keeps its name and meaning, so a chart saved in `settings.json` by Phase 6
//! loads unchanged; everything new has a default (`#[serde(default)]` on the struct covers each field).

use super::colours::ColourMap;
use crate::models::{MetricFormatType, MoneyScale};
use crate::sort_lists::SortOrder;
use serde::{Deserialize, Serialize};

/// The default number of categories before the rest are "Other".
pub const DEFAULT_TOP_N: usize = 30;

fn default_top_n() -> usize {
    DEFAULT_TOP_N
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChartAgg {
    #[default]
    Sum,
    Count,
    Average,
    WeightedAvg,
    Min,
    Max,
}

impl ChartAgg {
    pub fn all() -> &'static [ChartAgg] {
        &[ChartAgg::Sum, ChartAgg::Count, ChartAgg::Average, ChartAgg::WeightedAvg, ChartAgg::Min, ChartAgg::Max]
    }
    pub fn label(&self) -> &'static str {
        match self {
            ChartAgg::Sum => "Sum",
            ChartAgg::Count => "Count",
            ChartAgg::Average => "Average",
            ChartAgg::WeightedAvg => "W.Avg",
            ChartAgg::Min => "Min",
            ChartAgg::Max => "Max",
        }
    }
    /// The word in the title: "Sum of Balance", "Weighted average of Rate".
    pub fn title_word(&self) -> &'static str {
        match self {
            ChartAgg::Sum => "Sum",
            ChartAgg::Count => "Count",
            ChartAgg::Average => "Average",
            ChartAgg::WeightedAvg => "Weighted average",
            ChartAgg::Min => "Min",
            ChartAgg::Max => "Max",
        }
    }
    pub fn needs_value(&self) -> bool {
        *self != ChartAgg::Count
    }
}

/// The form of the chart. `Bar` is the Phase 6 name for the vertical column chart (it is drawn as
/// columns, or as horizontal bars when `horizontal` is set); old settings say "Bar" or "Line".
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChartKind {
    #[default]
    Bar,
    Line,
    Area,
    Scatter,
    Bubble,
    Histogram,
    BoxPlot,
    Waterfall,
    Heatmap,
    Donut,
    /// HTML only.
    Sankey,
    /// HTML only.
    Treemap,
}

impl ChartKind {
    pub fn all() -> &'static [ChartKind] {
        &[
            ChartKind::Bar,
            ChartKind::Line,
            ChartKind::Area,
            ChartKind::Scatter,
            ChartKind::Bubble,
            ChartKind::Histogram,
            ChartKind::BoxPlot,
            ChartKind::Waterfall,
            ChartKind::Heatmap,
            ChartKind::Donut,
            ChartKind::Sankey,
            ChartKind::Treemap,
        ]
    }
    pub fn label(&self) -> &'static str {
        match self {
            ChartKind::Bar => "Column / bar",
            ChartKind::Line => "Line",
            ChartKind::Area => "Area",
            ChartKind::Scatter => "Scatter",
            ChartKind::Bubble => "Bubble",
            ChartKind::Histogram => "Histogram",
            ChartKind::BoxPlot => "Box plot",
            ChartKind::Waterfall => "Waterfall",
            ChartKind::Heatmap => "Heatmap",
            ChartKind::Donut => "Donut",
            ChartKind::Sankey => "Sankey",
            ChartKind::Treemap => "Treemap",
        }
    }
    /// Drawn only in the HTML export (the in-app and Excel renderers refuse it with a message).
    pub fn html_only(&self) -> bool {
        matches!(self, ChartKind::Sankey | ChartKind::Treemap)
    }
}

/// How several series share a category (bars and areas).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Stack {
    /// Side by side (one series: a single bar).
    #[default]
    Grouped,
    Stacked,
    /// Stacked, each category as a share of its own total (100%).
    Percent,
}

/// What is done to a measure after it is added up.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Transform {
    #[default]
    None,
    /// Each value as a percent of the sum of all values.
    ShareOfTotal,
    /// Each value as a percent of its category's sum over the series (100% stacked columns).
    ShareOfCategory,
    /// Each value as a percent of its series' sum over the categories ("% of row" in a heatmap).
    ShareOfSeries,
    /// A running sum along the categories, per series (cumulative lines).
    RunningSum,
}

impl Transform {
    pub fn label(&self) -> &'static str {
        match self {
            Transform::None => "As is",
            Transform::ShareOfTotal => "% of total",
            Transform::ShareOfCategory => "% of category",
            Transform::ShareOfSeries => "% of row",
            Transform::RunningSum => "Running sum",
        }
    }
}

/// How a waterfall reads its categories (in chart order).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WaterfallMode {
    /// The first category is the start total, the last is the end total; the ones between are the
    /// steps. The end must equal the start plus the steps, or the chart is refused.
    #[default]
    StartStepsEnd,
    /// The first category is the start total and the rest are steps; the end is calculated.
    StartSteps,
}

/// One thing to measure: a column, how it is added up, and what is done with the result.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct MeasureSpec {
    pub column: Option<String>,
    pub agg: ChartAgg,
    pub weight: Option<String>,
    /// The name in legends and titles; empty = "Sum of Balance" and the like.
    pub label: String,
    pub transform: Transform,
    /// The numbers are already percentages (a cumulative gap in % of assets, a rate): written with "%".
    pub pct: bool,
}

/// One panel of linked panels: a form and the measures (by position in the measure list) it draws.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct PanelSpec {
    pub kind: ChartKind,
    pub measures: Vec<usize>,
    pub title: String,
    /// The panel's measure is the same in every series of the chart (a ratio of each category, such as the NPL %):
    /// it is drawn once, as one series named after the measure in the first categorical colour.
    pub single: bool,
}

/// The status colour of a reference line (always shown with its text label).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Status {
    Good,
    Warning,
    Serious,
    #[default]
    Critical,
}

impl Status {
    pub fn all() -> &'static [Status] {
        &[Status::Good, Status::Warning, Status::Serious, Status::Critical]
    }
    pub fn label(&self) -> &'static str {
        match self {
            Status::Good => "Good",
            Status::Warning => "Warning",
            Status::Serious => "Serious",
            Status::Critical => "Critical",
        }
    }
    pub fn hex(&self) -> u32 {
        use super::tokens as t;
        match self {
            Status::Good => t::STATUS_GOOD,
            Status::Warning => t::STATUS_WARNING,
            Status::Serious => t::STATUS_SERIOUS,
            Status::Critical => t::STATUS_CRITICAL,
        }
    }
}

/// A limit, minimum or outlier line across the value axis.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct RefLine {
    pub value: f64,
    pub label: String,
    pub status: Status,
    /// Which panel of linked panels it belongs to (0 = the first or only one).
    pub panel: usize,
}

/// A shaded band between two values (for example the core level of a balance history).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct RefBand {
    pub from: f64,
    pub to: f64,
    pub label: String,
    pub panel: usize,
}

/// A limit that belongs to one category (the minimum cumulative gap of one bucket, the minimum ratio of
/// one bar): a short tick in the status colour across the category's band at `value`. When `breach` is
/// not empty the mark of that category is ringed and the text is written beside the ring.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct LimitTick {
    /// Which panel of linked panels it belongs to (0 = the first or only one).
    pub panel: usize,
    pub category: String,
    pub value: f64,
    pub status: Status,
    /// "Breach: -7.1% vs −5% limit"; empty = the category is within its limit.
    pub breach: String,
}

/// One bar drawn in a status colour with an icon and a text ("Below Amber"): the colour is never alone.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct BarStatus {
    pub category: String,
    pub status: Status,
    pub label: String,
}

/// How the amounts are shown: the value column's own format, or one scale for the chart.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChartScale {
    /// The value column's display format (Phase 3).
    #[default]
    Follow,
    Plain,
    Thousand,
    Lakh,
    Million,
    Crore,
}

impl ChartScale {
    pub fn all() -> &'static [ChartScale] {
        &[ChartScale::Follow, ChartScale::Plain, ChartScale::Thousand, ChartScale::Lakh, ChartScale::Million, ChartScale::Crore]
    }
    pub fn label(&self) -> &'static str {
        match self {
            ChartScale::Follow => "As in View",
            ChartScale::Plain => "Plain (BDT)",
            ChartScale::Thousand => "Thousand",
            ChartScale::Lakh => "Lakh",
            ChartScale::Million => "Million",
            ChartScale::Crore => "Crore",
        }
    }
    /// The scale to divide by; `None` = no division. `Follow` is resolved by the caller.
    pub fn fixed(&self) -> Option<Option<MoneyScale>> {
        match self {
            ChartScale::Follow => None,
            ChartScale::Plain => Some(None),
            ChartScale::Thousand => Some(MetricFormatType::BdtThousand.scale()),
            ChartScale::Lakh => Some(MetricFormatType::BdtLakh.scale()),
            ChartScale::Million => Some(MetricFormatType::BdtMillion.scale()),
            ChartScale::Crore => Some(MetricFormatType::BdtCrore.scale()),
        }
    }
}

/// The size of the chart's text: value labels, axis ticks, axis names and the legend (titles keep their size).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextSize {
    Small,
    #[default]
    Normal,
    Large,
    Larger,
}

impl TextSize {
    pub fn all() -> &'static [TextSize] {
        &[TextSize::Small, TextSize::Normal, TextSize::Large, TextSize::Larger]
    }
    pub fn label(&self) -> &'static str {
        match self {
            TextSize::Small => "Small",
            TextSize::Normal => "Normal",
            TextSize::Large => "Large",
            TextSize::Larger => "Larger",
        }
    }
    /// 0.9, 1.0, 1.15 and 1.3 times the design system's sizes.
    pub fn factor(&self) -> f32 {
        match self {
            TextSize::Small => 0.9,
            TextSize::Normal => 1.0,
            TextSize::Large => 1.15,
            TextSize::Larger => 1.3,
        }
    }
}

/// Which data labels a chart prints.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LabelMode {
    /// The selective rule of the design system (bars at the tip, lines at the end, every step of a waterfall).
    #[default]
    Auto,
    /// Every bar, segment and point; a label that would collide is left out and the chart says how many.
    All,
    /// Only the first and the last category of each series.
    EndsOnly,
    /// No value labels (the tooltip and the table still have every number).
    None,
}

impl LabelMode {
    pub fn all() -> &'static [LabelMode] {
        &[LabelMode::Auto, LabelMode::All, LabelMode::EndsOnly, LabelMode::None]
    }
    pub fn label(&self) -> &'static str {
        match self {
            LabelMode::Auto => "Auto",
            LabelMode::All => "All",
            LabelMode::EndsOnly => "Ends only",
            LabelMode::None => "None",
        }
    }
}

/// The unit the data labels are written in (the axis keeps the view's).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LabelUnit {
    #[default]
    AsView,
    Absolute,
    Thousand,
    Lakh,
    Million,
    Crore,
}

impl LabelUnit {
    pub fn all() -> &'static [LabelUnit] {
        &[LabelUnit::AsView, LabelUnit::Absolute, LabelUnit::Thousand, LabelUnit::Lakh, LabelUnit::Million, LabelUnit::Crore]
    }
    pub fn label(&self) -> &'static str {
        match self {
            LabelUnit::AsView => "As in view",
            LabelUnit::Absolute => "Absolute",
            LabelUnit::Thousand => "Thousand",
            LabelUnit::Lakh => "Lakh",
            LabelUnit::Million => "Million",
            LabelUnit::Crore => "Crore",
        }
    }
    /// BDT per unit; `None` = as in the view.
    pub fn divisor(&self) -> Option<f64> {
        match self {
            LabelUnit::AsView => None,
            LabelUnit::Absolute => Some(1.0),
            LabelUnit::Thousand => Some(1_000.0),
            LabelUnit::Lakh => Some(100_000.0),
            LabelUnit::Million => Some(1_000_000.0),
            LabelUnit::Crore => Some(10_000_000.0),
        }
    }
}

fn yes() -> bool {
    true
}

/// "Data labels" in "Titles and look": which labels, how many decimals, which unit, and whether share views print their %.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct LabelSpec {
    pub mode: LabelMode,
    /// 0 to 4; `None` = the view's decimals.
    pub decimals: Option<usize>,
    pub unit: LabelUnit,
    /// Labels on share views (percentages) are printed; off = none of them.
    #[serde(default = "yes")]
    pub percent: bool,
}

impl Default for LabelSpec {
    fn default() -> Self {
        Self { mode: LabelMode::Auto, decimals: None, unit: LabelUnit::AsView, percent: true }
    }
}

impl LabelSpec {
    /// True when nothing differs from how a chart saved before this setting existed was drawn.
    pub fn is_default(&self) -> bool {
        *self == LabelSpec::default()
    }
}

/// What to draw.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ChartSpec {
    // ---- Phase 6 fields (names and meaning unchanged) ----
    pub category: String,
    pub series: Option<String>,
    /// With `measures` empty these three are the one measure.
    pub value: Option<String>,
    pub weight: Option<String>,
    pub agg: ChartAgg,
    pub kind: ChartKind,
    /// Natural, a custom list or Tenor (U3).
    pub order: SortOrder,
    /// "By value": the categories largest first (instead of `order`).
    pub by_value: bool,
    pub top_n: usize,
    pub scale: ChartScale,

    // ---- Phase 7 ----
    /// The measures, when there is more than one (or the studio has set them); the first is the primary one.
    pub measures: Vec<MeasureSpec>,
    /// Bubble size: a measure of its own.
    pub size: Option<MeasureSpec>,
    /// Small multiples: one panel per value of this column (at most 12).
    pub facet: Option<String>,
    /// Linked panels (two or three, stacked on one shared x-axis); empty = one panel.
    pub panels: Vec<PanelSpec>,
    /// Horizontal bars (categories down the side) instead of columns.
    pub horizontal: bool,
    pub stack: Stack,
    /// A stepped line.
    pub stepped: bool,
    pub waterfall: WaterfallMode,
    /// Histogram bins; none = chosen from the data.
    pub bins: Option<usize>,
    pub title: String,
    pub subtitle: String,
    pub footnote: String,
    /// The user's takeaway: when typed it replaces the auto-title.
    pub takeaway: String,
    pub ref_lines: Vec<RefLine>,
    pub ref_bands: Vec<RefBand>,
    /// Per-category limits (ticks, and rings for breaches).
    pub limit_ticks: Vec<LimitTick>,
    /// A line chart whose value axis starts one tick below the lowest value (yield curves).
    pub pad_axis_below: bool,
    /// Stacked columns with positive and negative segments: the net of each stack is a short dark tick across it with a
    /// signed two-decimal label; the stack edges carry no labels (NII effects by key).
    pub net_marker: bool,
    /// A stacked column: each segment is labelled beside the stack with its value and its share of the stack
    /// ("3,500 \u{b7} 70.0%"), the labels kept apart vertically (HQLA composition).
    pub segment_labels: bool,
    /// Bars drawn in a status colour with an icon and a text (data quality: "Below Amber").
    pub bar_status: Vec<BarStatus>,
    /// What the ticks mean, written once under the plot ("red ticks: minimum cumulative gap").
    pub limit_note: String,
    /// One category or series that is emphasised; the rest are de-emphasised.
    pub highlight: Option<String>,
    pub colour_map: ColourMap,
    /// HTML output only: JSON merged over the generated ECharts option (C3).
    pub advanced_json: String,
    /// The banking template this chart came from (C5), if any.
    pub template: String,
    /// Data labels: which, how many decimals, which unit, and the % of share views.
    pub labels: LabelSpec,
    /// The size of labels, ticks, axis names and legend.
    pub text_size: TextSize,
    /// Donut: "Group small parts": `None` = every part is drawn; `Some(n)` = the n largest and the rest as "Other" (2 to 12).
    pub donut_top: Option<usize>,

    // ---- Phase 8 W2 ----
    /// The category is a date part ("Open Date › Quarter"): its name is `category`.
    pub category_part: Option<crate::dates::DateFieldRef>,
    /// The series is a date part: its name is `series`.
    pub series_part: Option<crate::dates::DateFieldRef>,
    /// The facet is a date part: its name is `facet`.
    pub facet_part: Option<crate::dates::DateFieldRef>,
}

impl Default for ChartSpec {
    fn default() -> Self {
        Self {
            category: String::new(),
            series: None,
            value: None,
            weight: None,
            agg: ChartAgg::Sum,
            kind: ChartKind::Bar,
            order: SortOrder::Natural,
            by_value: false,
            top_n: default_top_n(),
            scale: ChartScale::Follow,
            measures: Vec::new(),
            size: None,
            facet: None,
            panels: Vec::new(),
            horizontal: false,
            stack: Stack::Grouped,
            stepped: false,
            waterfall: WaterfallMode::StartStepsEnd,
            bins: None,
            title: String::new(),
            subtitle: String::new(),
            footnote: String::new(),
            takeaway: String::new(),
            ref_lines: Vec::new(),
            ref_bands: Vec::new(),
            limit_ticks: Vec::new(),
            bar_status: Vec::new(),
            pad_axis_below: false,
            net_marker: false,
            segment_labels: false,
            limit_note: String::new(),
            highlight: None,
            colour_map: ColourMap::default(),
            advanced_json: String::new(),
            template: String::new(),
            labels: LabelSpec::default(),
            text_size: TextSize::Normal,
            donut_top: None,
            category_part: None,
            series_part: None,
            facet_part: None,
        }
    }
}

impl ChartSpec {
    /// The colour map is tied to the category and series columns: when either differs from `before`'s the map is
    /// emptied, so the new names take slots 1, 2, 3 …. With the same columns (a filter, Top N) it stays.
    pub fn reset_colours_if_columns_changed(&mut self, before: &ChartSpec) {
        if self.category != before.category || self.series != before.series {
            self.colour_map = ColourMap::default();
        }
    }

    /// The date parts the chart uses as its category, series or facet (W2): only those whose name is still the
    /// field's (a part left behind after another column was picked is ignored).
    pub fn date_fields(&self) -> Vec<crate::dates::DateFieldRef> {
        let mut out: Vec<crate::dates::DateFieldRef> = Vec::new();
        for (part, field) in [(&self.category_part, Some(self.category.as_str())), (&self.series_part, self.series.as_deref()), (&self.facet_part, self.facet.as_deref())] {
            if let Some(p) = part.as_ref().filter(|p| Some(p.name.as_str()) == field) {
                if !out.contains(p) {
                    out.push(p.clone());
                }
            }
        }
        out
    }

    /// Drops a date part whose field is no longer the category, series or facet (another column was picked).
    pub fn drop_stale_date_parts(&mut self) {
        if self.category_part.as_ref().is_some_and(|p| p.name != self.category) {
            self.category_part = None;
        }
        if self.series_part.as_ref().is_some_and(|p| Some(p.name.as_str()) != self.series.as_deref()) {
            self.series_part = None;
        }
        if self.facet_part.as_ref().is_some_and(|p| Some(p.name.as_str()) != self.facet.as_deref()) {
            self.facet_part = None;
        }
    }

    /// The measures to calculate: `measures`, or the Phase 6 value/agg/weight as the only one.
    pub fn measure_list(&self) -> Vec<MeasureSpec> {
        if self.measures.is_empty() {
            vec![MeasureSpec { column: self.value.clone(), agg: self.agg, weight: self.weight.clone(), label: String::new(), transform: Transform::None, pct: false }]
        } else {
            self.measures.clone()
        }
    }

    /// The Phase 6 shape of measure `i` (its column, aggregation and weight in the old fields), which is
    /// what the single-measure engine takes.
    pub fn for_measure(&self, m: &MeasureSpec) -> ChartSpec {
        ChartSpec { value: m.column.clone(), agg: m.agg, weight: m.weight.clone(), measures: Vec::new(), size: None, facet: None, ..self.clone() }
    }

    /// The settings that change the numbers (not the look): when they change the worker recalculates.
    pub fn calc_key(&self) -> String {
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{}|{}|{:?}|{:?}|{:?}|{}|{:?}|{:?}|{}|{:?}",
            self.category,
            self.series,
            self.value,
            self.weight,
            self.agg,
            self.order,
            self.by_value,
            self.top_n,
            self.measures,
            self.size,
            self.facet,
            // the point, histogram and box forms calculate their own tables; the other forms share one
            matches!(self.kind, ChartKind::Scatter | ChartKind::Bubble | ChartKind::Histogram | ChartKind::BoxPlot) as u8 * (1 + self.kind as u8),
            self.bins,
            // a donut groups its small parts (and draws every part otherwise): the numbers change with it
            self.donut_top,
            (self.kind == ChartKind::Donut) as u8,
            // the date parts (W2): their calendar changes the numbers
            self.date_fields()
        )
    }
}
