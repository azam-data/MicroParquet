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
            TimeAgg::WAvgAtClosing => match weight {
                Some(w) => format!("W.Avg at closing of {} by {}", field, w),
                None => format!("W.Avg at closing of {}", field),
            },
        }
    }

    /// True when the aggregation reads a column that is a number (the others count rows).
    pub fn needs_number(self) -> bool {
        !matches!(self, TimeAgg::CountAtClosing | TimeAgg::DistinctCountAtClosing)
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

// ------------------------------------------------------------------------------------------------ the stock warning

/// "Sum of Balance adds 23 dates of balances. Use Closing balance or Average balance."
pub fn stock_warning(agg_label: &str, column: &str, dates: usize) -> String {
    format!("{} of {} adds {} dates of balances. Use Closing balance or Average balance.", agg_label, column, dates)
}
