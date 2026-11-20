//! The chart gallery (Phase 7 C2): every chart form grouped by the question it answers, each with one
//! line of "use it when…" and one of "avoid it when…" (the same lines the guide of C8 teaches from).

use super::spec::{ChartAgg, ChartKind, ChartSpec, Stack};
use super::suggest::{ColKind, SuggestCol};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Question {
    Compare,
    Change,
    Composition,
    Distribution,
    Relationship,
    FlowAndShare,
}

impl Question {
    pub fn all() -> &'static [Question] {
        &[Question::Compare, Question::Change, Question::Composition, Question::Distribution, Question::Relationship, Question::FlowAndShare]
    }
    pub fn title(&self) -> &'static str {
        match self {
            Question::Compare => "Compare",
            Question::Change => "Change over time",
            Question::Composition => "Composition",
            Question::Distribution => "Distribution",
            Question::Relationship => "Relationship",
            Question::FlowAndShare => "Flow and share (HTML only)",
        }
    }
    pub fn asks(&self) -> &'static str {
        match self {
            Question::Compare => "Which is bigger?",
            Question::Change => "How did it move?",
            Question::Composition => "What is it made of?",
            Question::Distribution => "How are the values spread?",
            Question::Relationship => "Do two things move together?",
            Question::FlowAndShare => "Where does it go, and who holds what share?",
        }
    }
}

/// One card of the gallery.
#[derive(Clone, Copy, Debug)]
pub struct Card {
    pub id: &'static str,
    pub name: &'static str,
    pub question: Question,
    pub kind: ChartKind,
    pub stack: Stack,
    pub horizontal: bool,
    pub stepped: bool,
    /// Sorted largest first.
    pub by_value: bool,
    pub use_when: &'static str,
    pub avoid_when: &'static str,
}

const fn card(id: &'static str, name: &'static str, question: Question, kind: ChartKind, use_when: &'static str, avoid_when: &'static str) -> Card {
    Card { id, name, question, kind, stack: Stack::Grouped, horizontal: false, stepped: false, by_value: false, use_when, avoid_when }
}

use ChartKind as K;
use Question as Q;

/// Every card, in gallery order.
pub const CARDS: &[Card] = &[
    Card { horizontal: true, ..card("bar", "Bar", Q::Compare, K::Bar, "you rank a few to a few dozen named things by size.", "the categories have an order (months, buckets): use columns.") },
    card("grouped", "Grouped bars", Q::Compare, K::Bar, "you compare two to four series side by side for each category.", "there are more than four series, or the totals matter more than the parts.") ,
    Card { horizontal: true, by_value: true, ..card("sorted-bar", "Sorted bar", Q::Compare, K::Bar, "the ranking is the message: largest first, one bar each.", "you need to show change over time.") },
    card("line", "Line", Q::Change, K::Line, "values follow a date or an ordered scale and the shape matters.", "there are only two points, or the categories have no order.") ,
    card("area", "Area", Q::Change, K::Area, "the total over time matters as much as the trend.", "series overlap and hide each other, or the values go negative.") ,
    card("column", "Column by month", Q::Change, K::Bar, "you compare amounts month by month and want each month read on its own.", "there are more than about 24 periods: use a line.") ,
    Card { stack: Stack::Stacked, ..card("stacked", "Stacked column", Q::Composition, K::Bar, "you show a total and what it is made of, over months or buckets.", "readers must compare the middle segments with each other.") },
    Card { stack: Stack::Percent, ..card("percent", "100% stacked", Q::Composition, K::Bar, "the mix matters, not the size of the total.", "the totals differ a lot and that is the story.") },
    card("donut", "Donut", Q::Composition, K::Donut, "two to five parts of one whole and one value to read.", "there are more than five parts or you compare several wholes: use a bar.") ,
    card("waterfall", "Waterfall", Q::Composition, K::Waterfall, "a start total reaches an end total through signed steps (a bridge).", "the steps do not add up to the end total.") ,
    card("histogram", "Histogram", Q::Distribution, K::Histogram, "you want to see how many rows fall in each range of a number.", "you need exact values: use a table.") ,
    card("box", "Box plot", Q::Distribution, K::BoxPlot, "you compare the spread of a number across a few groups.", "readers do not know quartiles: add a note, or use a histogram.") ,
    card("scatter", "Scatter", Q::Relationship, K::Scatter, "you test whether two numbers move together, one point per row.", "there are only a handful of rows: a table reads faster.") ,
    card("bubble", "Bubble", Q::Relationship, K::Bubble, "a third number (size) matters as well as the two on the axes.", "the sizes are close: readers cannot tell the areas apart.") ,
    card("heatmap", "Heatmap", Q::Relationship, K::Heatmap, "two categories and a number meet and you want to see where in the grid the amount sits.", "there are few cells: a table with the numbers is clearer.") ,
    card("sankey", "Sankey", Q::FlowAndShare, K::Sankey, "amounts flow from one set of states to another (grade migration).", "there is no flow, only two totals to compare.") ,
    card("treemap", "Treemap", Q::FlowAndShare, K::Treemap, "you show shares of a whole by two levels (product, then branch).", "readers need exact comparisons: use sorted bars.") ,
];

pub fn card_by_id(id: &str) -> Option<&'static Card> {
    CARDS.iter().find(|c| c.id == id)
}

pub fn cards_for(q: Question) -> impl Iterator<Item = &'static Card> {
    CARDS.iter().filter(move |c| c.question == q)
}

/// The chart settings a card stands for, on top of the columns already chosen.
pub fn apply_card(spec: &mut ChartSpec, c: &Card) {
    spec.kind = c.kind;
    spec.stack = c.stack;
    spec.horizontal = c.horizontal;
    spec.stepped = c.stepped;
    spec.by_value = c.by_value;
}

/// Fills the columns a form needs and the spec does not have yet, from the tab's columns; a column the
/// user already chose is kept where it still makes sense (a scatter needs numbers on both axes, a bar a
/// category on one).
pub fn fit_columns(spec: &mut ChartSpec, cols: &[SuggestCol]) {
    let nums: Vec<&SuggestCol> = cols.iter().filter(|c| c.kind == ColKind::Number).collect();
    let cats: Vec<&SuggestCol> = cols.iter().filter(|c| c.kind != ColKind::Number).collect();
    let kind_of = |name: &str| cols.iter().find(|c| c.name == name).map(|c| c.kind);
    let is_num = |n: &str| kind_of(n) == Some(ColKind::Number);
    let first_num_not = |not: &[&str]| nums.iter().find(|c| !not.contains(&c.name.as_str())).map(|c| c.name.clone());
    let value_ok = spec.value.as_deref().is_some_and(is_num) || spec.agg == ChartAgg::Count;
    match spec.kind {
        ChartKind::Scatter | ChartKind::Bubble => {
            if !is_num(&spec.category) {
                spec.category = nums.first().map(|c| c.name.clone()).unwrap_or_default();
            }
            let cat = spec.category.clone();
            if spec.value.as_deref().is_none_or(|v| !is_num(v) || v == cat) {
                spec.value = first_num_not(&[cat.as_str()]).or_else(|| nums.first().map(|c| c.name.clone()));
            }
            spec.agg = ChartAgg::Sum;
            if spec.kind == ChartKind::Bubble && spec.size.as_ref().and_then(|m| m.column.as_deref()).is_none_or(|c| !is_num(c)) {
                let used = [spec.category.as_str(), spec.value.as_deref().unwrap_or("")].map(str::to_string);
                spec.size = first_num_not(&[used[0].as_str(), used[1].as_str()]).map(|c| super::spec::MeasureSpec { column: Some(c), ..Default::default() });
            }
        }
        ChartKind::Histogram => {
            let v = spec.value.clone().filter(|v| is_num(v)).or_else(|| nums.first().map(|c| c.name.clone()));
            spec.category = v.clone().unwrap_or_default();
            spec.value = v;
            spec.agg = ChartAgg::Sum;
        }
        _ => {
            if is_num(&spec.category) || spec.category.is_empty() {
                if let Some(c) = cats.first() {
                    spec.category = c.name.clone();
                }
            }
            if !value_ok {
                spec.value = nums.first().map(|c| c.name.clone());
                spec.agg = if spec.value.is_some() { ChartAgg::Sum } else { ChartAgg::Count };
            }
            if spec.kind == ChartKind::Heatmap && spec.series.is_none() {
                spec.series = cats.iter().find(|c| c.name != spec.category).map(|c| c.name.clone());
            }
            if spec.kind == ChartKind::Heatmap && spec.measures.is_empty() {
                spec.measures = vec![super::spec::MeasureSpec { column: spec.value.clone(), agg: spec.agg, weight: spec.weight.clone(), transform: super::spec::Transform::ShareOfSeries, ..Default::default() }];
            }
        }
    }
}
