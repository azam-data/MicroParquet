//! Phase 9 M3: balances and flows.
//!
//! A balance (a stock) is a position on a date: adding the balances of 23 dates gives a number that means nothing. A flow (interest,
//! income) is an amount in a period: adding is right. This module holds
//! - what a column is tagged as (`ColumnTag`) and the suggestions from its name,
//! - the time aggregations of a value (`TimeAgg`) worked out from one **daily series** and the **snapshot dates** of the tab
//!   (`evaluate`), as plain numbers and dates so the hand-worked acceptance cases can be tested without a table,
//! - the stock warning.
//!
//! The pivot (`pivot_time.rs`) builds the daily series and the period of each cell and calls `evaluate`.

use crate::dates::CalendarSettings;
use crate::model_calendar::long_date;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ------------------------------------------------------------------------------------------------ column tags

/// What a numeric column of a date-linked data tab is (Model > Measures > Columns).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ColumnTag {
    /// A position on a date: a balance, an outstanding amount. Closing and average balance are right; a plain sum over dates is not.
    #[default]
    Balance,
    /// An amount in a period: interest, income, disbursements. Summing is right.
    Flow,
    /// A trial balance's income and expense GLs, which add up from the start of the fiscal year.
    YtdBalance,
}

impl ColumnTag {
    pub fn all() -> &'static [ColumnTag] {
        &[ColumnTag::Balance, ColumnTag::Flow, ColumnTag::YtdBalance]
    }

    /// How the Columns list and the pickers call it.
    pub fn label(self) -> &'static str {
        match self {
            ColumnTag::Balance => "Balance (stock)",
            ColumnTag::Flow => "Flow",
            ColumnTag::YtdBalance => "YTD balance",
        }
    }

    /// The aggregation a column of this kind gets when it is added to Values.
    pub fn default_agg(self) -> crate::models::TimeAgg {
        match self {
            ColumnTag::Balance => crate::models::TimeAgg::ClosingBalance,
            ColumnTag::Flow => crate::models::TimeAgg::TotalFlow,
            ColumnTag::YtdBalance => crate::models::TimeAgg::PeriodAmount,
        }
    }
}

/// A column of a data tab with its tag. A column that is not here is untagged.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct ColumnKind {
    /// The data tab's project key.
    #[serde(default)]
    pub tab: String,
    #[serde(default)]
    pub column: String,
    #[serde(default)]
    pub kind: ColumnTag,
}

/// What the name of a column suggests: Balance, Outstanding, Bal, OS mean a balance; Interest, Income, Expense, Disbursed, Recovery
/// mean a flow. A suggestion only (the owner applies it by a click). A name that suggests both (an "Interest Balance") suggests
/// nothing, and so does one that says neither.
pub fn suggest(column: &str) -> Option<ColumnTag> {
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let chars: Vec<char> = column.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word).to_lowercase());
            }
            continue;
        }
        // a new word starts at a capital after a small letter ("OutstandingBalance")
        if c.is_uppercase() && i > 0 && chars[i - 1].is_lowercase() && !word.is_empty() {
            words.push(std::mem::take(&mut word).to_lowercase());
        }
        word.push(*c);
    }
    if !word.is_empty() {
        words.push(word.to_lowercase());
    }
    let is_balance = words.iter().any(|w| matches!(w.as_str(), "balance" | "balances" | "outstanding" | "bal" | "os"));
    let is_flow = words.iter().any(|w| matches!(w.as_str(), "interest" | "income" | "expense" | "expenses" | "disbursed" | "disbursement" | "disbursements" | "recovery" | "recoveries"));
    match (is_balance, is_flow) {
        (true, false) => Some(ColumnTag::Balance),
        (false, true) => Some(ColumnTag::Flow),
        _ => None,
    }
}

// ------------------------------------------------------------------------------------------------ the aggregations

pub use crate::models::{TimeAgg, TimeValue};

impl TimeAgg {
    pub fn all() -> &'static [TimeAgg] {
        &[
            TimeAgg::ClosingBalance,
            TimeAgg::OpeningBalance,
            TimeAgg::AverageBalance,
            TimeAgg::TotalFlow,
            TimeAgg::PeriodAmount,
            TimeAgg::CountAtClosing,
            TimeAgg::DistinctCountAtClosing,
            TimeAgg::WAvgAtClosing,
        ]
    }

    /// How the aggregation list names it.
    pub fn label(self) -> &'static str {
        match self {
            TimeAgg::ClosingBalance => "Closing balance",
            TimeAgg::OpeningBalance => "Opening balance",
            TimeAgg::AverageBalance => "Average balance",
            TimeAgg::TotalFlow => "Total (flow)",
            TimeAgg::PeriodAmount => "Period amount (YTD balance)",
            TimeAgg::CountAtClosing => "Count at closing",
            TimeAgg::DistinctCountAtClosing => "Distinct count at closing",
            TimeAgg::WAvgAtClosing => "W.Avg at closing",
            TimeAgg::Measure => "Named measure",
        }
    }

    /// The heading of a column: "Closing balance of Balance", "Total of Interest", "Period amount of Balance (YTD)". `month_end`
    /// is true on a Month-end tab, where the average is of the month-ends.
    pub fn heading(self, field: &str, weight: Option<&str>, month_end: bool) -> String {
        match self {
            TimeAgg::ClosingBalance => format!("Closing balance of {}", field),
            TimeAgg::OpeningBalance => format!("Opening balance of {}", field),
            TimeAgg::AverageBalance if month_end => format!("Average of month-ends of {}", field),
            TimeAgg::AverageBalance => format!("Average balance of {}", field),
            TimeAgg::TotalFlow => format!("Total of {}", field),
            TimeAgg::PeriodAmount => format!("Period amount of {} (YTD)", field),
            TimeAgg::CountAtClosing => format!("Count at closing of {}", field),
            TimeAgg::DistinctCountAtClosing => format!("Distinct count at closing of {}", field),
            TimeAgg::Measure => field.to_string(),
            TimeAgg::WAvgAtClosing => match weight {
                Some(w) => format!("W.Avg at closing of {} by {}", field, w),
                None => format!("W.Avg at closing of {}", field),
            },
        }
    }

    /// True when the aggregation reads a column that is a number (the others count rows).
    pub fn needs_number(self) -> bool {
        !matches!(self, TimeAgg::CountAtClosing | TimeAgg::DistinctCountAtClosing | TimeAgg::Measure)
    }

    /// True when the cells of a row or column add up to its total. Only a flow does; a balance is never added across dates.
    pub fn is_additive(self) -> bool {
        matches!(self, TimeAgg::TotalFlow)
    }

    /// Counts are whole numbers.
    pub fn is_count(self) -> bool {
        matches!(self, TimeAgg::CountAtClosing | TimeAgg::DistinctCountAtClosing)
    }
}

/// What one date holds for one cell: its rows on that date.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DayStats {
    /// The sum of the value over the cell's rows on that date.
    pub sum: f64,
    /// The rows that have a value in the counted column.
    pub count: f64,
    /// The distinct values of the counted column.
    pub distinct: f64,
    /// The sum of value x weight, and of the weight (W.Avg at closing).
    pub weighted: f64,
    pub weight: f64,
}

/// What the aggregations need to know about the tab, whatever the cell.
#[derive(Clone, Debug)]
pub struct Timeline {
    /// The tab's snapshot dates, sorted: the distinct dates with rows, after its steps and before the cell's filters.
    pub snapshots: Vec<NaiveDate>,
    /// The fiscal year of the model's calendar (Period amount).
    pub fiscal: CalendarSettings,
    /// The tab's title, for the messages.
    pub tab: String,
    /// The as-of date: the end of every period is cut here, and "last date" in a message is the tab's last snapshot date up to it.
    pub as_of: NaiveDate,
}

/// A cell's number, or why it is blank.
#[derive(Clone, Debug, PartialEq)]
pub enum Measured {
    Value(f64),
    Blank(String),
}

impl Measured {
    pub fn value(&self) -> Option<f64> {
        match self {
            Measured::Value(v) => Some(*v),
            Measured::Blank(_) => None,
        }
    }
}

/// The value of a date for a cell: its rows' sum on that date, and 0 when it has none (a branch with no row on a date has 0).
fn value_on(series: &BTreeMap<NaiveDate, DayStats>, d: NaiveDate) -> f64 {
    series.get(&d).map(|s| s.sum).unwrap_or(0.0)
}

/// The last snapshot date on or before `d`.
fn last_on_or_before(snapshots: &[NaiveDate], d: NaiveDate) -> Option<NaiveDate> {
    let n = snapshots.partition_point(|s| *s <= d);
    n.checked_sub(1).map(|i| snapshots[i])
}

/// The last snapshot date before `d`.
fn last_before(snapshots: &[NaiveDate], d: NaiveDate) -> Option<NaiveDate> {
    let n = snapshots.partition_point(|s| *s < d);
    n.checked_sub(1).map(|i| snapshots[i])
}

/// The snapshot dates that are days of the period (`period` is sorted).
fn snapshots_in(snapshots: &[NaiveDate], period: &[NaiveDate]) -> Vec<NaiveDate> {
    snapshots.iter().copied().filter(|s| period.binary_search(s).is_ok()).collect()
}

/// "No balance in Nov 2026 for 'Accounts' (last date 31 Oct 2026)".
fn no_balance(tl: &Timeline, period_text: &str) -> Measured {
    let last = last_on_or_before(&tl.snapshots, tl.as_of).map(|d| format!(" (last date {})", long_date(d))).unwrap_or_default();
    Measured::Blank(format!("No balance in {} for '{}'{}", period_text, tl.tab, last))
}

/// One cell of an aggregation.
///
/// - `series`: the cell's rows, by date (only the dates that have rows; the others are 0).
/// - `period`: the days of the cell's period P, sorted, already cut at the as-of date.
/// - `period_text`: P in words, for the messages ("Nov 2026").
/// - `month_end`: the tab is a Month-end tab (its average is of the month-ends).
pub fn evaluate(agg: TimeAgg, series: &BTreeMap<NaiveDate, DayStats>, period: &[NaiveDate], period_text: &str, tl: &Timeline, month_end: bool) -> Measured {
    let Some(start) = period.first().copied() else {
        return no_balance(tl, period_text);
    };
    let in_p = snapshots_in(&tl.snapshots, period);
    let closing = in_p.last().copied();
    match agg {
        TimeAgg::ClosingBalance => match closing {
            Some(d) => Measured::Value(value_on(series, d)),
            None => no_balance(tl, period_text),
        },
        TimeAgg::OpeningBalance => match last_before(&tl.snapshots, start) {
            Some(d) => Measured::Value(value_on(series, d)),
            None => Measured::Blank(format!("No balance before {} for '{}'", long_date(start), tl.tab)),
        },
        TimeAgg::AverageBalance if month_end => {
            if in_p.is_empty() {
                return no_balance(tl, period_text);
            }
            let total: f64 = in_p.iter().map(|d| value_on(series, *d)).sum();
            Measured::Value(total / in_p.len() as f64)
        }
        TimeAgg::AverageBalance => {
            // every calendar day of P takes the value of the last snapshot date on or before it (Fridays, Saturdays and holidays
            // carry the last balance forward)
            if last_on_or_before(&tl.snapshots, start).is_none() {
                return Measured::Blank(format!("No balance on or before {}. Add that date's data or start later.", long_date(start)));
            }
            let mut sum = 0.0;
            let mut at = 0usize;
            let mut current = 0.0;
            for d in period {
                while at < tl.snapshots.len() && tl.snapshots[at] <= *d {
                    current = value_on(series, tl.snapshots[at]);
                    at += 1;
                }
                sum += current;
            }
            Measured::Value(sum / period.len() as f64)
        }
        TimeAgg::TotalFlow => {
            if in_p.is_empty() {
                return Measured::Blank(format!("No data in {} for '{}'{}", period_text, tl.tab, last_on_or_before(&tl.snapshots, tl.as_of).map(|d| format!(" (last date {})", long_date(d))).unwrap_or_default()));
            }
            let last = period.last().copied().unwrap_or(start);
            let total: f64 = series.range(start..=last).filter(|(d, _)| period.binary_search(d).is_ok()).map(|(_, s)| s.sum).sum();
            Measured::Value(total)
        }
        TimeAgg::PeriodAmount => {
            let Some(c) = closing else {
                return no_balance(tl, period_text);
            };
            // the balance since the start of the fiscal year, less what it was before P when that was in the same fiscal year
            let before = match last_before(&tl.snapshots, start) {
                Some(p) if tl.fiscal.fiscal_start_year(p) == tl.fiscal.fiscal_start_year(c) => value_on(series, p),
                _ => 0.0,
            };
            Measured::Value(value_on(series, c) - before)
        }
        TimeAgg::Measure => Measured::Blank("A named measure is worked out from its own definition.".to_string()),
        TimeAgg::CountAtClosing | TimeAgg::DistinctCountAtClosing | TimeAgg::WAvgAtClosing => {
            let Some(c) = closing else {
                return no_balance(tl, period_text);
            };
            let s = series.get(&c).copied().unwrap_or_default();
            match agg {
                TimeAgg::CountAtClosing => Measured::Value(s.count),
                TimeAgg::DistinctCountAtClosing => Measured::Value(s.distinct),
                _ => {
                    if s.weight == 0.0 {
                        Measured::Blank(format!("No weights on {} for '{}': the weighted average is blank.", long_date(c), tl.tab))
                    } else {
                        Measured::Value(s.weighted / s.weight)
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------ the rows behind a number

/// The rows behind a time cell (M3 follow-up): the dates whose rows make up its number, and whether the rows get a "Part" column.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimeRows {
    /// The dates, each with its part ("End" / "Start") when the cell subtracts two dates.
    pub dates: Vec<(NaiveDate, Option<&'static str>)>,
    /// Said under the tab's title: "These rows make up an average; their sum is not the cell."
    pub note: Option<&'static str>,
}

/// The note of the rows of an average.
pub const AVERAGE_ROWS_NOTE: &str = "These rows make up an average; their sum is not the cell.";

/// The dates of the rows behind the number `evaluate` gives for the same arguments (`None` when the cell is blank).
pub fn rows_of(agg: TimeAgg, period: &[NaiveDate], tl: &Timeline, month_end: bool) -> Option<TimeRows> {
    let start = period.first().copied()?;
    let in_p = snapshots_in(&tl.snapshots, period);
    let closing = in_p.last().copied();
    let plain = |dates: Vec<NaiveDate>| Some(TimeRows { dates: dates.into_iter().map(|d| (d, None)).collect(), note: None });
    match agg {
        TimeAgg::ClosingBalance | TimeAgg::CountAtClosing | TimeAgg::DistinctCountAtClosing | TimeAgg::WAvgAtClosing => plain(vec![closing?]),
        TimeAgg::OpeningBalance => plain(vec![last_before(&tl.snapshots, start)?]),
        TimeAgg::TotalFlow => {
            if in_p.is_empty() {
                None
            } else {
                plain(in_p)
            }
        }
        TimeAgg::Measure => None,
        TimeAgg::PeriodAmount => {
            let c = closing?;
            let mut dates = vec![(c, Some("End"))];
            if let Some(p) = last_before(&tl.snapshots, start) {
                if tl.fiscal.fiscal_start_year(p) == tl.fiscal.fiscal_start_year(c) {
                    dates.push((p, Some("Start")));
                }
            }
            Some(TimeRows { dates, note: None })
        }
        TimeAgg::AverageBalance => {
            let mut dates = in_p;
            if !month_end {
                // the date carried in: the last snapshot date on or before the first day, when it is not a day of the period
                let carried = last_on_or_before(&tl.snapshots, start)?;
                if !dates.contains(&carried) {
                    dates.insert(0, carried);
                }
            } else if dates.is_empty() {
                return None;
            }
            Some(TimeRows { dates: dates.into_iter().map(|d| (d, None)).collect(), note: Some(AVERAGE_ROWS_NOTE) })
        }
    }
}

// ------------------------------------------------------------------------------------------------ time intelligence (M4)

/// The period a cell stands for before the as-of date cuts it: its first and last day, and what it is a period of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Base {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub unit: crate::dates::PeriodUnit,
}

/// The days a time value reads, and the days of its earlier window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The window's first day and its last day cut at the as-of date; `None` when the window starts after the as-of date.
    pub current: Option<(NaiveDate, NaiveDate)>,
    /// The earlier window (a shift), when there is one.
    pub earlier: Option<(NaiveDate, NaiveDate)>,
}

enum Back {
    Months(i32),
    Days(i64),
}

/// The window of a time value (`t`) for a cell whose period is `base` (`None` for a cell with no period: its anchor is the as-of
/// date), and the earlier window the shift gives. The anchor is the period's end, never later than the as-of date.
pub fn plan(t: &crate::models::TimeValue, base: Option<Base>, as_of: NaiveDate, fiscal: &CalendarSettings) -> std::result::Result<Plan, String> {
    use crate::dates::{add_months, add_months_end, month_end, month_start, PeriodUnit};
    use crate::models::{TimePartial, TimeShift, TimeWindow};
    let (ws, ne, unit) = match t.window {
        TimeWindow::Period => match base {
            Some(b) => (b.start, b.end, b.unit),
            None => return Err("Add a Calendar field that names a period (Calendar \u{203a} Month-Year, for example) to Rows or Columns, or choose MTD, QTD or YTD.".to_string()),
        },
        w => {
            let a = base.map(|b| b.end.min(as_of)).unwrap_or(as_of);
            let (ws, ne, unit) = match w {
                TimeWindow::Mtd => (month_start(a), month_end(a), PeriodUnit::Month),
                TimeWindow::Qtd => (fiscal.fiscal_quarter_start(a), fiscal.fiscal_quarter_end(a), PeriodUnit::Quarter),
                _ => (fiscal.fiscal_year_start(a), fiscal.fiscal_year_end(a), PeriodUnit::Year),
            };
            // a cell with a period reads up to its own anchor (its period's end), not to the end of the month, quarter or year
            (ws, base.map(|b| ne.min(b.end)).unwrap_or(ne), unit)
        }
    };
    let starts_late = base.is_some_and(|b| b.start > as_of) || ws > as_of;
    let we = ne.min(as_of);
    let current = if starts_late { None } else { Some((ws, we)) };
    let back = match (t.shift, unit) {
        (TimeShift::None, _) => None,
        (TimeShift::Sply, PeriodUnit::Week) => Some(Back::Days(364)),
        (TimeShift::Sply, _) => Some(Back::Months(12)),
        (TimeShift::PriorPeriod, PeriodUnit::Day) => Some(Back::Days(1)),
        (TimeShift::PriorPeriod, PeriodUnit::Week) => Some(Back::Days(7)),
        (TimeShift::PriorPeriod, PeriodUnit::Month) => Some(Back::Months(1)),
        (TimeShift::PriorPeriod, PeriodUnit::Quarter) => Some(Back::Months(3)),
        (TimeShift::PriorPeriod, PeriodUnit::Year) => Some(Back::Months(12)),
    };
    let earlier = back.map(|b| {
        let cut = we < ne;
        let same_days = cut && t.partial == TimePartial::SameDays;
        match b {
            Back::Months(n) => (add_months(ws, -n), if same_days { add_months(we, -n) } else { add_months_end(ne, -n) }),
            Back::Days(n) => {
                let d = chrono::Duration::days(n);
                (ws - d, if same_days { we - d } else { ne - d })
            }
        }
    });
    Ok(Plan { current, earlier })
}

/// The days from `a` to `b`, both included.
pub fn days_between(a: NaiveDate, b: NaiveDate) -> Vec<NaiveDate> {
    let mut out = Vec::new();
    let mut d = a;
    while d <= b {
        out.push(d);
        match d.succ_opt() {
            Some(n) => d = n,
            None => break,
        }
    }
    out
}

/// What a time value shows from its window's number and its earlier window's: the value of the shifted window, the difference
/// (this - earlier) or the % change ((this - earlier) / |earlier|, blank when the earlier value is 0).
pub fn combine(show: crate::models::TimeShow, shifted: bool, current: Measured, earlier: Option<(Measured, String)>) -> Measured {
    use crate::models::TimeShow;
    if !shifted {
        return current;
    }
    let Some((earlier, text)) = earlier else {
        return Measured::Blank("The earlier window could not be worked out.".to_string());
    };
    let earlier_value = match earlier {
        Measured::Value(v) => v,
        Measured::Blank(why) => return Measured::Blank(format!("Earlier window ({}): {}", text, why)),
    };
    match show {
        TimeShow::Value => Measured::Value(earlier_value),
        TimeShow::Difference | TimeShow::PercentChange => {
            let now = match current {
                Measured::Value(v) => v,
                blank => return blank,
            };
            if show == TimeShow::Difference {
                Measured::Value(now - earlier_value)
            } else if earlier_value == 0.0 {
                Measured::Blank(format!("The earlier window ({}) is 0, so a % change can't be worked out.", text))
            } else {
                Measured::Value((now - earlier_value) / earlier_value.abs() * 100.0)
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------ the stock warning

/// "Sum of Balance adds 23 dates of balances. Use Closing balance or Average balance."
pub fn stock_warning(agg_label: &str, column: &str, dates: usize) -> String {
    format!("{} of {} adds {} dates of balances. Use Closing balance or Average balance.", agg_label, column, dates)
}

/// The note of a plain time value with no Calendar field: the whole data it covers, so the number can't be misread.
pub fn span_note(heading: &str, first: NaiveDate, last: NaiveDate, as_of: NaiveDate) -> String {
    use crate::model_calendar::long_date;
    format!("{}: whole data, {} \u{2013} {} (as of {})", heading, long_date(first), long_date(last), long_date(as_of))
}

// ------------------------------------------------------------------------------------------------ named measures (M5)

use crate::models::FilterCondition;

/// The sign of a measure or of a term of a ratio: Minus turns a credit-negative trial balance around.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Sign {
    #[default]
    Plus,
    Minus,
}

impl Sign {
    pub fn factor(self) -> f64 {
        match self {
            Sign::Plus => 1.0,
            Sign::Minus => -1.0,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Sign::Plus => "Plus (+)",
            Sign::Minus => "Minus (\u{2212})",
        }
    }
}

/// How a ratio is annualised: the days of the year it is scaled to.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DayCount {
    #[default]
    #[serde(rename = "Actual/365")]
    Actual365,
    #[serde(rename = "Actual/360")]
    Actual360,
    #[serde(rename = "Actual/Actual")]
    ActualActual,
}

impl DayCount {
    pub fn all() -> &'static [DayCount] {
        &[DayCount::Actual365, DayCount::Actual360, DayCount::ActualActual]
    }
    pub fn label(self) -> &'static str {
        match self {
            DayCount::Actual365 => "Actual/365",
            DayCount::Actual360 => "Actual/360",
            DayCount::ActualActual => "Actual/Actual",
        }
    }
    /// The days of a year: 365, 360, or the days of the fiscal year holding `end` (365 or 366).
    pub fn year_days(self, end: NaiveDate, fiscal: &CalendarSettings) -> f64 {
        match self {
            DayCount::Actual365 => 365.0,
            DayCount::Actual360 => 360.0,
            DayCount::ActualActual => ((fiscal.fiscal_year_end(end) - fiscal.fiscal_year_start(end)).num_days() + 1) as f64,
        }
    }
}

/// How a ratio is shown: a percent (9.5 = 9.5%, as rates are kept) or a plain number.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RatioFormat {
    #[default]
    Percent,
    Number,
}

impl RatioFormat {
    pub fn label(self) -> &'static str {
        match self {
            RatioFormat::Percent => "Percent",
            RatioFormat::Number => "Number",
        }
    }
}

/// A base measure of a ratio's numerator or denominator, with its sign.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Term {
    pub measure: String,
    #[serde(default)]
    pub sign: Sign,
}

/// A measure of one data tab with a name of its own (Phase 9 M5).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind")]
pub enum NamedMeasure {
    /// A column of the tab, aggregated over time (M3), for the rows that meet the filters (lookup and Calendar fields allowed).
    Base {
        name: String,
        tab: String,
        column: String,
        #[serde(default)]
        aggregation: TimeAgg,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        filters: Vec<FilterCondition>,
        #[serde(default)]
        sign: Sign,
        /// The weight column of W.Avg at closing.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        weight: Option<String>,
    },
    /// Numerator over denominator, each a signed sum of base measures; annualised when a day count is set.
    Ratio {
        name: String,
        tab: String,
        #[serde(default)]
        numerator: Vec<Term>,
        #[serde(default)]
        denominator: Vec<Term>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        annualise: Option<DayCount>,
        #[serde(default)]
        format: RatioFormat,
    },
    /// A budget or target (Phase 11): the amount column of a plan table linked to the tab (Model \u{25b8} Links), by month and the link's keys,
    /// shown as the plan itself, the actual less the plan, or the actual as a percent of the plan. The actual is a base measure or a ratio.
    Plan {
        name: String,
        /// The data tab (key) the actual and the pivot belong to.
        tab: String,
        /// The plan table's tab (key).
        plan_tab: String,
        /// The plan table's amount column (Budget, Target ...).
        column: String,
        #[serde(default)]
        agg: PlanAgg,
        #[serde(default)]
        show: PlanShow,
        /// The measure the plan is set against; unused (blank) when only the plan is shown.
        #[serde(default)]
        actual: String,
    },
}

/// How a plan's months add up over a period.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum PlanAgg {
    /// The months of the period add up (income, cost, a flow budget). A month that has started counts in full.
    #[default]
    Sum,
    /// The last month of the period (a closing balance target).
    Closing,
}

/// What a plan measure shows.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum PlanShow {
    /// The budget or target itself.
    #[default]
    Plan,
    /// Actual less plan (in the actual's unit; a percent ratio's difference is in percentage points).
    Variance,
    /// Actual over plan, in percent.
    PercentAchieved,
}

impl PlanShow {
    pub fn label(self) -> &'static str {
        match self {
            PlanShow::Plan => "Plan",
            PlanShow::Variance => "Actual less plan",
            PlanShow::PercentAchieved => "% achieved",
        }
    }
}

impl NamedMeasure {
    pub fn name(&self) -> &str {
        match self {
            NamedMeasure::Base { name, .. } | NamedMeasure::Ratio { name, .. } | NamedMeasure::Plan { name, .. } => name,
        }
    }
    pub fn tab(&self) -> &str {
        match self {
            NamedMeasure::Base { tab, .. } | NamedMeasure::Ratio { tab, .. } | NamedMeasure::Plan { tab, .. } => tab,
        }
    }
    pub fn is_plan(&self) -> bool {
        matches!(self, NamedMeasure::Plan { .. })
    }
    pub fn is_ratio(&self) -> bool {
        matches!(self, NamedMeasure::Ratio { .. })
    }
    /// Percent ratios show a change in percentage points.
    pub fn is_percent_ratio(&self) -> bool {
        matches!(self, NamedMeasure::Ratio { format: RatioFormat::Percent, .. })
    }
}

/// A flow (a total over the window or a YTD movement) can be annualised; a balance or a count cannot.
pub fn is_flow(agg: TimeAgg) -> bool {
    matches!(agg, TimeAgg::TotalFlow | TimeAgg::PeriodAmount)
}

/// The named measure of the open project's model with this name.
pub fn find_measure(name: &str) -> Option<NamedMeasure> {
    crate::model::scope().and_then(|c| c.measures.iter().find(|m| m.name() == name).cloned())
}

/// Why `m` can't be saved among `all` (the measures of the project, without `m` itself when it is being edited), or `None`.
pub fn check_measure(all: &[NamedMeasure], m: &NamedMeasure) -> Option<String> {
    let name = m.name().trim();
    if name.is_empty() {
        return Some("Give the measure a name.".to_string());
    }
    if all.iter().any(|o| o.name().trim().eq_ignore_ascii_case(name)) {
        return Some(format!("A measure called '{}' already exists. Names must be different.", name));
    }
    match m {
        NamedMeasure::Base { column, aggregation, weight, .. } => {
            if *aggregation == TimeAgg::Measure {
                return Some("Pick an aggregation for the measure.".to_string());
            }
            if aggregation.needs_number() && column.trim().is_empty() {
                return Some(format!("'{}': pick the column the measure adds up.", name));
            }
            if *aggregation == TimeAgg::WAvgAtClosing && weight.as_deref().map(str::trim).unwrap_or("").is_empty() {
                return Some(format!("'{}': a W.Avg at closing needs a weight column.", name));
            }
            None
        }
        NamedMeasure::Ratio { tab, numerator, denominator, annualise, .. } => {
            if numerator.is_empty() {
                return Some(format!("'{}': add at least one numerator term.", name));
            }
            if denominator.is_empty() {
                return Some(format!("'{}': add at least one denominator term.", name));
            }
            for t in numerator.iter().chain(denominator.iter()) {
                match all.iter().find(|o| o.name() == t.measure) {
                    None => return Some(format!("'{}': the measure '{}' does not exist. Pick a base measure of this tab.", name, t.measure)),
                    Some(o) if o.is_ratio() || o.is_plan() => return Some(format!("'{}': '{}' is a ratio or a plan measure. A ratio is made of base measures.", name, t.measure)),
                    Some(o) if o.tab() != tab => return Some(format!("'{}': '{}' belongs to another tab. A ratio uses the measures of one tab.", name, t.measure)),
                    _ => {}
                }
            }
            if annualise.is_some() {
                for t in numerator {
                    if let Some(NamedMeasure::Base { aggregation, .. }) = all.iter().find(|o| o.name() == t.measure) {
                        if !is_flow(*aggregation) {
                            return Some(annualise_refusal(&t.measure));
                        }
                    }
                }
            }
            None
        }
        NamedMeasure::Plan { tab, plan_tab, column, show, actual, .. } => {
            if plan_tab.trim().is_empty() {
                return Some(format!("'{}': pick the plan table (Model \u{25b8} Links \u{25b8} Budget and target tables first).", name));
            }
            if column.trim().is_empty() {
                return Some(format!("'{}': pick the plan's amount column.", name));
            }
            if *show != PlanShow::Plan {
                match all.iter().find(|o| o.name() == actual.as_str()) {
                    None => return Some(format!("'{}': pick the measure the plan is set against (the actual).", name)),
                    Some(o) if o.is_plan() => return Some(format!("'{}': '{}' is a plan measure. Pick a base measure or a ratio as the actual.", name, actual)),
                    Some(o) if o.tab() != tab => return Some(format!("'{}': '{}' belongs to another tab. The actual must be a measure of the same tab.", name, actual)),
                    _ => {}
                }
            }
            None
        }
    }
}

pub fn annualise_refusal(measure: &str) -> String {
    format!("Only flows and period amounts can be annualised; '{}' is a balance.", measure)
}

/// A measure was renamed from `old` to `new`: the terms of every ratio follow.
pub fn rename_in_ratios(measures: &mut [NamedMeasure], old: &str, new: &str) {
    for m in measures.iter_mut() {
        if let NamedMeasure::Ratio { numerator, denominator, .. } = m {
            for t in numerator.iter_mut().chain(denominator.iter_mut()).filter(|t| t.measure == old) {
                t.measure = new.to_string();
            }
        }
        if let NamedMeasure::Plan { actual, .. } = m {
            if actual == old {
                *actual = new.to_string();
            }
        }
    }
}

/// Why `name` can't be removed: the ratios (`measures`) and the pivots and charts (`others`, by title) that use it.
pub fn removal_refusal(measures: &[NamedMeasure], name: &str, others: &[String]) -> Option<String> {
    let mut users: Vec<String> = measures
        .iter()
        .filter(|m| match m {
            NamedMeasure::Ratio { numerator, denominator, .. } => numerator.iter().chain(denominator.iter()).any(|t| t.measure == name),
            NamedMeasure::Plan { actual, show, .. } => *show != PlanShow::Plan && actual == name,
            _ => false,
        })
        .map(|m| format!("{} '{}'", if m.is_plan() { "plan measure" } else { "ratio" }, m.name()))
        .collect();
    users.extend(others.iter().cloned());
    if users.is_empty() {
        None
    } else {
        Some(format!("'{}' is used by {}. Remove it from there first.", name, users.join(", ")))
    }
}

/// The ratio of one window: the signed numerator over the signed denominator, the numerator scaled to a year when annualised.
/// A blank term, a blank denominator or a denominator of 0 gives a blank with its reason, never 0.
pub fn ratio_value(name: &str, num: &[(Measured, f64)], den: &[(Measured, f64)], annual: Option<f64>, percent: bool, text: &str) -> Measured {
    let mut n = 0.0;
    for (m, s) in num {
        match m {
            Measured::Value(v) => n += v * s,
            Measured::Blank(why) => return Measured::Blank(why.clone()),
        }
    }
    let mut d = 0.0;
    for (m, s) in den {
        match m {
            Measured::Value(v) => d += v * s,
            Measured::Blank(why) => return Measured::Blank(why.clone()),
        }
    }
    if let Some(f) = annual {
        n *= f;
    }
    if d == 0.0 {
        return Measured::Blank(format!("The denominator of '{}' is 0 in {}, so the ratio can't be worked out.", name, text));
    }
    let r = n / d;
    Measured::Value(if percent { r * 100.0 } else { r })
}
