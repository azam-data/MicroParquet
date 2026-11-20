//! "Suggest a chart" (Phase 7 C2): the user picks columns and the studio proposes the best form with a
//! one-line reason. Fixed rules, no guessing from the data's look beyond whether a text column is
//! ordered (buckets, tenors, grades, months).

use super::spec::{ChartKind, Stack};
use crate::sort_lists::SortOrder;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColKind {
    Date,
    Number,
    Text,
}

/// One column the user picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestCol {
    pub name: String,
    pub kind: ColKind,
    /// A text column whose values have a natural order (buckets, tenors, grades, months).
    pub ordered: bool,
}

/// What the user says about the columns beyond their types.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SuggestFlags {
    /// A signed change between two totals (a bridge: start, steps, end).
    pub change_between_totals: bool,
}

/// The proposal: the form, how it is laid out, which column plays which part, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub kind: ChartKind,
    pub stack: Stack,
    pub horizontal: bool,
    /// Categories largest first (a sorted bar).
    pub by_value: bool,
    /// Keep the categories in their natural order (buckets, tenors, grades, months, dates).
    pub keep_order: bool,
    pub category: String,
    pub series: Option<String>,
    pub value: Option<String>,
    pub reason: String,
}

/// True when a text column looks ordered: its name says so, or its values are tenors / months / grades.
pub fn looks_ordered(name: &str, sample: &[String]) -> bool {
    let n = name.to_lowercase();
    if ["bucket", "tenor", "grade", "term", "band", "month", "maturity", "ageing", "aging", "stage", "class"].iter().any(|w| n.contains(w)) {
        return true;
    }
    let vals: Vec<&String> = sample.iter().filter(|s| !s.trim().is_empty()).take(50).collect();
    if vals.len() < 2 {
        return false;
    }
    let tenors = vals.iter().filter(|v| SortOrder::Tenor.rank(v).is_some()).count();
    if tenors * 10 >= vals.len() * 8 {
        return true;
    }
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let months = vals.iter().filter(|v| MONTHS.iter().any(|m| v.to_lowercase().starts_with(m))).count();
    months * 10 >= vals.len() * 8
}

/// The proposal for the picked columns, or the reason there is none.
pub fn suggest(cols: &[SuggestCol], flags: SuggestFlags) -> Result<Suggestion, String> {
    let dates: Vec<&SuggestCol> = cols.iter().filter(|c| c.kind == ColKind::Date).collect();
    let nums: Vec<&SuggestCol> = cols.iter().filter(|c| c.kind == ColKind::Number).collect();
    let texts: Vec<&SuggestCol> = cols.iter().filter(|c| c.kind == ColKind::Text).collect();
    let base = |kind: ChartKind, category: &str, value: Option<&str>, reason: &str| Suggestion {
        kind,
        stack: Stack::Grouped,
        horizontal: false,
        by_value: false,
        keep_order: true,
        category: category.to_string(),
        series: None,
        value: value.map(str::to_string),
        reason: reason.to_string(),
    };
    if cols.is_empty() {
        return Err("Pick at least one column.".to_string());
    }
    // a bridge: a signed change between two totals
    if flags.change_between_totals {
        return match (texts.first(), nums.first()) {
            (Some(t), Some(n)) => Ok(base(ChartKind::Waterfall, &t.name, Some(&n.name), "A waterfall shows how a start total reaches an end total through the steps between them.")),
            _ => Err("A waterfall needs a text column (the steps, in order) and a number column (the start, the changes and the end).".to_string()),
        };
    }
    // two numbers: scatter
    if nums.len() >= 2 && texts.is_empty() && dates.is_empty() {
        return Ok(base(ChartKind::Scatter, &nums[0].name, Some(&nums[1].name), "Two numbers: a scatter shows whether they move together."));
    }
    // a date: over time
    if let (Some(d), Some(n)) = (dates.first(), nums.first()) {
        if let Some(t) = texts.first() {
            let mut s = base(ChartKind::Bar, &d.name, Some(&n.name), "Parts of a total over time: stacked columns show the total and what it is made of.");
            s.stack = Stack::Stacked;
            s.series = Some(t.name.clone());
            return Ok(s);
        }
        return Ok(base(ChartKind::Line, &d.name, Some(&n.name), "A date and a number: a line shows the trend over time."));
    }
    if let Some(n) = nums.first() {
        // two text columns and a number: heatmap
        if texts.len() >= 2 {
            let mut s = base(ChartKind::Heatmap, &texts[0].name, Some(&n.name), "Two categories and a number: a heatmap shows where the amount sits in the grid.");
            s.series = Some(texts[1].name.clone());
            return Ok(s);
        }
        if let Some(t) = texts.first() {
            if t.ordered {
                return Ok(base(ChartKind::Bar, &t.name, Some(&n.name), "An ordered category (buckets, tenors, grades, months): columns keep that order."));
            }
            let mut s = base(ChartKind::Bar, &t.name, Some(&n.name), "A category and a number: bars sorted by size make the ranking easy to read.");
            s.horizontal = true;
            s.by_value = true;
            s.keep_order = false;
            return Ok(s);
        }
        return Err("Pick a category (a text or date column) to go with the number.".to_string());
    }
    Err("Pick a number column to chart, with a category (text or date) or a second number.".to_string())
}
