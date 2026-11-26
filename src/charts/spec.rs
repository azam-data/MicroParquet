//! `ChartSpec`: a chart described completely (Phase 7). It grew out of the Phase 6 chart settings:
//! every Phase 6 field keeps its name and meaning, so a chart saved in `settings.json` by Phase 6
//! loads unchanged; everything new has a default (`#[serde(default)]` on the struct covers each field).

use super::colours::ColourMap;
use crate::models::{CondOp, FilterCondition, MetricFormatType, MoneyScale};
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

/// Where a chart's data is printed with it (Phase 8 W11): on a PDF page and in the HTML page.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DataTable {
    #[default]
    Off,
    /// Under the chart, on the same page.
    Below,
    /// Beside the chart, on a landscape page.
    Beside,
}

impl DataTable {
    pub fn all() -> [DataTable; 3] {
        [DataTable::Off, DataTable::Below, DataTable::Beside]
    }

    pub fn label(&self) -> &'static str {
        match self {
            DataTable::Off => "Off",
            DataTable::Below => "Below the chart",
            DataTable::Beside => "Beside the chart",
        }
    }
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
    /// A time aggregation with its window, shift and show (Phase 9 M4): the value type the pivot uses, so a chart shows the pivot's
    /// number. When set, `agg` is not used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<crate::models::TimeValue>,
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

/// One level of a chart's drill (Phase 8 W6): the field the category shows at that depth, a column or a date part.
/// `name` is what the chart's `category` is while that level is shown ("Branch", "Open Date \u{b7} Quarter").
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DrillLevel {
    #[serde(default)]
    pub name: String,
    /// The date part, when the level is one (its calendar is the one the level was made with).
    #[serde(default)]
    pub part: Option<crate::dates::DateFieldRef>,
    /// The lookup field, when the level is one (Phase 9 M1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<crate::model::ModelFieldRef>,
}

impl DrillLevel {
    pub fn column(name: &str) -> Self {
        Self { name: name.to_string(), part: None, model: None }
    }

    pub fn of_part(f: &crate::dates::DateFieldRef) -> Self {
        Self { name: f.name.clone(), part: Some(f.clone()), model: None }
    }

    pub fn of_model(f: &crate::model::ModelFieldRef) -> Self {
        Self { name: f.name.clone(), part: None, model: Some(f.clone()) }
    }

    /// The condition "this field is `value`" (the text a mark's category shows; "(blank)" is the blank rows).
    pub fn condition(&self, value: &str, keep: bool) -> FilterCondition {
        let blank = value == crate::pivot_table::BLANK_LABEL;
        // a lookup field's "(no match)" is the flag, never a value (M1-4)
        let no_match = self.model.is_some() && value == crate::model::NO_MATCH_LABEL;
        // "(blank key)" too: the data rows whose key is blank, a flag of its own (M1-12)
        let blank_key = self.model.is_some() && value == crate::model::BLANK_KEY_LABEL;
        FilterCondition {
            column: self.part.as_ref().map(|p| p.column.clone()).unwrap_or_else(|| self.name.clone()),
            op: if keep { CondOp::InList } else { CondOp::NotInList },
            values: if blank || no_match || blank_key { Vec::new() } else { vec![value.to_string()] },
            or_blank: blank,
            no_match,
            blank_key,
            part: self.part.clone(),
            lookup: self.model.clone(),
            ..Default::default()
        }
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
    /// The one measure's time aggregation (Phase 9 M4), as `MeasureSpec.time`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<crate::models::TimeValue>,
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
    /// The category, series or facet is a lookup field (Phase 9 M1): its name is `category`, `series` or `facet`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_model: Option<crate::model::ModelFieldRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_model: Option<crate::model::ModelFieldRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet_model: Option<crate::model::ModelFieldRef>,
    /// The order of the series (Q1): `None` = by size, as before Phase 8. Filled when a series is added.
    pub series_order: Option<SortOrder>,
    /// The order of the facet panels (Q1): `None` = largest first, as before Phase 8.
    pub facet_order: Option<SortOrder>,

    // ---- Phase 8 W6 ----
    /// The drill levels, top first (empty = no drill, as before Phase 8). While it is not empty the `category` is the
    /// level the chart is at (`drill[drill_path.len()]`): `sync_level` keeps them together.
    pub drill: Vec<DrillLevel>,
    /// The values clicked on the way down: (the level's name, the value). One per level left behind.
    pub drill_path: Vec<(String, String)>,
    /// The chart's own filters ("Keep only this", "Exclude this"): conditions on the rows the chart reads, all of them.
    pub filters: Vec<FilterCondition>,
    /// The data tab the chart is drawn from, by project key, when it is date-linked (Phase 9 M3): the stock warning reads its column
    /// tags. Filled by the app when it asks for the chart, never saved.
    #[serde(skip)]
    pub data_tab: String,

    // ---- Phase 8 W11 ----
    /// Stacked and 100% stacked column and bar: each segment says its value inside it (percent for 100% stacked), straight
    /// where it fits, turned where it does not and a column has room, left out where nothing fits. Off = as before Phase 8.
    pub inside_labels: bool,
    /// With `inside_labels`: a segment that is less than this percent of its bar has no label (5).
    pub inside_min_pct: f64,
    /// The chart's data printed with it: Off, below it or beside it.
    pub data_table: DataTable,

    // ---- Phase 11 ----
    /// Extend the line with a forecast and its bands (Analytics pane). `None` = no forecast, as before Phase 11.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forecast: Option<crate::forecast_chart::ForecastSpec>,
}

/// The share of a bar below which a segment has no label (Phase 8 W11).
pub const INSIDE_MIN_PCT_DEFAULT: f64 = 5.0;

impl Default for ChartSpec {
    fn default() -> Self {
        Self {
            category: String::new(),
            series: None,
            value: None,
            weight: None,
            agg: ChartAgg::Sum,
            time: None,
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
            category_model: None,
            series_model: None,
            facet_model: None,
            series_order: None,
            facet_order: None,
            drill: Vec::new(),
            drill_path: Vec::new(),
            filters: Vec::new(),
            data_tab: String::new(),
            inside_labels: false,
            inside_min_pct: INSIDE_MIN_PCT_DEFAULT,
            data_table: DataTable::Off,
            forecast: None,
        }
    }
}

impl ChartSpec {
    /// The colour map is tied to the category and series columns: when either differs from `before`'s the map is
    /// emptied, so the new names take slots 1, 2, 3 …. With the same columns (a filter, Top N) it stays.
    pub fn reset_colours_if_columns_changed(&mut self, before: &ChartSpec) {
        // a click down (or up) the same drill levels changes the category, not the columns the colours belong to (W6)
        let along_the_levels = self.has_drill() && self.drill == before.drill;
        if (self.category != before.category && !along_the_levels) || self.series != before.series {
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

    /// The lookup fields the chart uses as its category, series or facet (Phase 9 M1): only those whose name is still the field's.
    pub fn model_fields(&self) -> Vec<crate::model::ModelFieldRef> {
        let mut out: Vec<crate::model::ModelFieldRef> = Vec::new();
        for (f, field) in [(&self.category_model, Some(self.category.as_str())), (&self.series_model, self.series.as_deref()), (&self.facet_model, self.facet.as_deref())] {
            if let Some(f) = f.as_ref().filter(|f| Some(f.name.as_str()) == field) {
                if !out.contains(f) {
                    out.push(f.clone());
                }
            }
        }
        out
    }

    /// Drops a date part or lookup field whose field is no longer the category, series or facet (another column was picked).
    pub fn drop_stale_date_parts(&mut self) {
        if self.category_model.as_ref().is_some_and(|p| p.name != self.category) {
            self.category_model = None;
        }
        if self.series_model.as_ref().is_some_and(|p| Some(p.name.as_str()) != self.series.as_deref()) {
            self.series_model = None;
        }
        if self.facet_model.as_ref().is_some_and(|p| Some(p.name.as_str()) != self.facet.as_deref()) {
            self.facet_model = None;
        }
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

    /// A named measure was renamed (Phase 9 M5 follow-up): the values that use it follow. True when something changed.
    pub fn rename_measure(&mut self, old: &str, new: &str) -> bool {
        use crate::models::TimeAgg;
        let is = |column: &Option<String>, time: &Option<crate::models::TimeValue>| column.as_deref() == Some(old) && time.is_some_and(|t| t.agg == TimeAgg::Measure);
        let mut changed = false;
        if is(&self.value, &self.time) {
            self.value = Some(new.to_string());
            changed = true;
        }
        for m in self.measures.iter_mut().chain(self.size.iter_mut()) {
            if is(&m.column, &m.time) {
                m.column = Some(new.to_string());
                changed = true;
            }
        }
        changed
    }

    /// The measures to calculate: `measures`, or the Phase 6 value/agg/weight as the only one.
    pub fn measure_list(&self) -> Vec<MeasureSpec> {
        if self.measures.is_empty() {
            vec![MeasureSpec { column: self.value.clone(), agg: self.agg, weight: self.weight.clone(), label: String::new(), transform: Transform::None, pct: false, time: self.time }]
        } else {
            self.measures.clone()
        }
    }

    /// The Phase 6 shape of measure `i` (its column, aggregation and weight in the old fields), which is
    /// what the single-measure engine takes.
    pub fn for_measure(&self, m: &MeasureSpec) -> ChartSpec {
        ChartSpec { value: m.column.clone(), agg: m.agg, weight: m.weight.clone(), time: m.time, measures: Vec::new(), size: None, facet: None, ..self.clone() }
    }

    /// The settings that change the numbers (not the look): when they change the worker recalculates.
    pub fn calc_key(&self) -> String {
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{}|{}|{:?}|{:?}|{:?}|{}|{:?}|{:?}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            self.category,
            self.series,
            self.value,
            self.weight,
            (self.agg, self.time),
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
            self.date_fields(),
            self.model_fields(),
            self.series_order,
            self.facet_order,
            // W6: the level, the path and the chart's own filters decide which rows are read
            (&self.drill, &self.drill_path, &self.filters),
            // Phase 11: a forecast, and the form it is drawn on (it needs a line)
            self.forecast.as_ref().map(|f| (f, self.kind as u8))
        )
    }
}

// ------------------------------------------------------------------------------------------ drill (Phase 8 W6)

impl ChartKind {
    /// Why this kind of chart has no drill; `None` when it has (column, bar, stacked, grouped, line, area, donut, heatmap).
    pub fn drill_reason(self) -> Option<&'static str> {
        match self {
            ChartKind::Bar | ChartKind::Line | ChartKind::Area | ChartKind::Donut | ChartKind::Heatmap => None,
            ChartKind::Scatter | ChartKind::Bubble => Some("A scatter or bubble chart draws single rows, not groups, so there is nothing to drill into."),
            ChartKind::Histogram => Some("A histogram draws ranges of one number, not groups, so there is nothing to drill into."),
            ChartKind::BoxPlot => Some("A box plot draws the spread of the rows of each group, so there is nothing to drill into."),
            ChartKind::Waterfall => Some("A waterfall draws steps that add up to a total, so a step is not a group to drill into."),
            ChartKind::Sankey | ChartKind::Treemap => Some("This chart is drawn only in the HTML page, which has no drill."),
        }
    }
}

impl ChartSpec {
    /// True when the chart has drill levels.
    pub fn has_drill(&self) -> bool {
        !self.drill.is_empty()
    }

    /// How many levels the chart has gone down (the clicked values on its path).
    pub fn depth(&self) -> usize {
        self.drill_path.len()
    }

    /// True when a click cannot go further down: the last level, or no drill at all.
    pub fn at_last_level(&self) -> bool {
        self.depth() + 1 >= self.drill.len()
    }

    /// The level the chart shows, when it has drill levels.
    pub fn current_level(&self) -> Option<&DrillLevel> {
        self.drill.get(self.depth())
    }

    /// Makes the category the level the chart is at (a chart with no drill is left alone).
    pub fn sync_level(&mut self) {
        if let Some(l) = self.drill.get(self.drill_path.len()).cloned() {
            self.category = l.name;
            self.category_part = l.part;
            self.category_model = l.model;
        }
    }

    /// The chart as drawn at its level.
    pub fn at_level(&self) -> ChartSpec {
        let mut s = self.clone();
        s.sync_level();
        s
    }

    /// One level down on the clicked category `value`. `false` (and nothing changes) at the last level or with no drill.
    pub fn drill_down(&mut self, value: &str) -> bool {
        if self.drill.is_empty() || self.at_last_level() {
            return false;
        }
        let name = self.drill[self.depth()].name.clone();
        self.drill_path.push((name, value.to_string()));
        self.sync_level();
        true
    }

    /// Back up to `depth` levels down (0 = "All"): the path is cut there.
    pub fn drill_to(&mut self, depth: usize) {
        self.drill_path.truncate(depth);
        self.sync_level();
    }

    /// Sets the drill levels (top first) and goes back to the top.
    pub fn set_drill(&mut self, levels: Vec<DrillLevel>) {
        self.drill = levels;
        self.drill_path.clear();
        self.sync_level();
    }

    /// The path and the levels must agree: one clicked value per level left behind, each at the level it was clicked in.
    pub fn check_drill(&self) -> anyhow::Result<()> {
        if self.drill.is_empty() {
            if self.drill_path.is_empty() {
                return Ok(());
            }
            anyhow::bail!("Chart: the drill path has values but the chart has no drill levels. Go back to All, or add the levels again.");
        }
        if self.drill_path.len() >= self.drill.len() {
            anyhow::bail!("Chart: the drill path is as long as the drill levels, so there is no level left to show. Go back up one level.");
        }
        for (i, (name, _)) in self.drill_path.iter().enumerate() {
            if *name != self.drill[i].name {
                anyhow::bail!("Chart: the drill path says '{}' at level {}, but that level is '{}'. Go back to All.", name, i + 1, self.drill[i].name);
            }
        }
        let mut seen: Vec<&str> = Vec::new();
        for l in &self.drill {
            if seen.contains(&l.name.as_str()) {
                anyhow::bail!("Chart: '{}' is a drill level twice. Remove one of them.", l.name);
            }
            seen.push(l.name.as_str());
            if self.series.as_deref() == Some(l.name.as_str()) || self.facet.as_deref() == Some(l.name.as_str()) {
                anyhow::bail!("Chart: '{}' is a drill level and also the series or the panels. Choose another one of them.", l.name);
            }
        }
        Ok(())
    }

    /// The conditions the rows must meet: the chart's own filters, then one "is this value" per level on the path.
    /// The chart's numbers at its level, and the rows behind a mark ("Show rows"), are both read with these.
    pub fn row_conditions(&self) -> Vec<FilterCondition> {
        let mut out = self.filters.clone();
        for (i, (_, value)) in self.drill_path.iter().enumerate() {
            if let Some(l) = self.drill.get(i) {
                out.push(l.condition(value, true));
            }
        }
        out
    }

    /// One of the chart's own filters in words: "Bank = Conventional", "Bank = Conventional or Islamic", "Bank \u{2260} Islamic".
    pub fn filter_text(c: &FilterCondition) -> String {
        let field = c.part.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| c.column.clone());
        let field = if c.lookup.is_some() { field.replacen(" \u{b7} ", " \u{203a} ", 1) } else { field };
        // a comparison with a limit (Phase 8 W9): "Open Date \u{b7} Month-Year \u{2265} Aug 2025"
        match c.op {
            CondOp::Gt | CondOp::Gte | CondOp::Lt | CondOp::Lte => return format!("{} {} {}", field, c.op.label(), c.value.trim()),
            CondOp::Between => return format!("{} between {} and {}", field, c.value.trim(), c.value2.trim()),
            CondOp::Equals | CondOp::NotEquals if c.values.is_empty() && !c.or_blank && !c.no_match && !c.blank_key && !c.value.trim().is_empty() => {
                return format!("{} {} {}", field, if c.op == CondOp::NotEquals { "\u{2260}" } else { "=" }, c.value.trim());
            }
            _ => {}
        }
        let mut vals: Vec<String> = c.values.clone();
        if c.or_blank {
            vals.push(crate::pivot_table::BLANK_LABEL.to_string());
        }
        if c.blank_key {
            vals.push(crate::model::BLANK_KEY_LABEL.to_string());
        }
        if c.no_match {
            vals.push(crate::model::NO_MATCH_LABEL.to_string());
        }
        let op = if c.op == CondOp::NotInList { "\u{2260}" } else { "=" };
        if vals.is_empty() {
            return format!("{} {} nothing ticked", field, op);
        }
        format!("{} {} {}", field, op, vals.join(" or "))
    }

    /// The chart's own filters must say which values they keep: a list with none ticked keeps no rows, and a chart of no rows
    /// is refused in words rather than drawn empty. (Phase 8 W10)
    pub fn check_filters(&self) -> anyhow::Result<()> {
        for c in &self.filters {
            let field = c.part.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| c.column.clone());
            if matches!(c.op, CondOp::InList | CondOp::NotInList) && c.values.is_empty() && !c.or_blank && !c.no_match && !c.blank_key {
                anyhow::bail!("Chart: the filter on '{}' has no value ticked, so it keeps no rows. Tick a value, or press All to keep every value.", field);
            }
            if c.op.is_comparison() && (c.value.trim().is_empty() || (c.op == CondOp::Between && c.value2.trim().is_empty())) {
                anyhow::bail!("Chart: the filter on '{}' {} has no value. Enter it, or remove the filter.", field, c.op.label());
            }
        }
        Ok(())
    }

    /// "Filtered: Deposit Type = FDR, Branch = Dhaka" (the path, then the chart's own filters); empty when nothing is.
    pub fn filtered_text(&self) -> String {
        let mut parts: Vec<String> = self.drill_path.iter().map(|(n, v)| format!("{} = {}", n, v)).collect();
        for c in &self.filters {
            parts.push(Self::filter_text(c));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("Filtered: {}", parts.join(", "))
        }
    }
}

impl ChartSpec {
    /// A mark called "Other" is the values Top N put together, not one value: a drill, a filter or the rows behind it would
    /// guess. Refused in words.
    pub fn refuse_other(value: &str) -> anyhow::Result<()> {
        if value == crate::chart::OTHER_LABEL {
            anyhow::bail!("'{}' is the values that Top N puts together, not one value. Raise Top N (or Group small parts) so it is shown as its own mark, or filter the table first.", value);
        }
        Ok(())
    }

    /// The rows behind one mark: the chart's own filters and the path, then the mark's category, and its series and panel
    /// when it has them. "Show rows" opens a tab with exactly these conditions, "Keep only" and "Exclude" take the category's.
    /// A mark called "Other" is the values Top N put together, so it is not one value and is refused.
    pub fn mark_conditions(&self, category: &str, series: Option<&str>, facet: Option<&str>) -> anyhow::Result<Vec<FilterCondition>> {
        let other = crate::chart::OTHER_LABEL;
        let mut out = self.row_conditions();
        Self::refuse_other(category)?;
        let level = self.current_level().cloned().unwrap_or_else(|| DrillLevel { name: self.category.clone(), part: self.category_part.clone(), model: self.category_model.clone() });
        out.push(level.condition(category, true));
        for (value, name, part, model) in [(series, self.series.as_deref(), &self.series_part, &self.series_model), (facet, self.facet.as_deref(), &self.facet_part, &self.facet_model)] {
            if let (Some(v), Some(n)) = (value.filter(|v| !v.is_empty()), name) {
                Self::refuse_other(v)?;
                out.push(DrillLevel { name: n.to_string(), part: part.clone(), model: model.clone() }.condition(v, true));
            }
        }
        Ok(out)
    }

    /// "Rows: Deposit Type = FDR, Branch = Dhaka": the title of the tab behind a mark.
    pub fn mark_title(&self, category: &str, series: Option<&str>, facet: Option<&str>) -> String {
        let mut parts: Vec<String> = self.drill_path.iter().map(|(n, v)| format!("{} = {}", n, v)).collect();
        parts.push(format!("{} = {}", self.category, category));
        if let (Some(v), Some(n)) = (series.filter(|v| !v.is_empty()), self.series.as_deref()) {
            parts.push(format!("{} = {}", n, v));
        }
        if let (Some(v), Some(n)) = (facet.filter(|v| !v.is_empty()), self.facet.as_deref()) {
            parts.push(format!("{} = {}", n, v));
        }
        format!("Rows: {}", parts.join(", "))
    }

    /// "Keep only this" or "Exclude this": the category `value` of the level the chart is at, added to the chart's own filters
    /// (a filter that is already there is not added twice). `false` when it was there already.
    pub fn add_value_filter(&mut self, value: &str, keep: bool) -> anyhow::Result<bool> {
        Self::refuse_other(value)?;
        let level = self.current_level().cloned().unwrap_or_else(|| DrillLevel { name: self.category.clone(), part: self.category_part.clone(), model: self.category_model.clone() });
        let cond = level.condition(value, keep);
        if self.filters.contains(&cond) {
            return Ok(false);
        }
        self.filters.push(cond);
        Ok(true)
    }
}
