//! The calendar core (Phase 8 W0): the only place that knows about calendars. The Split date step,
//! the pivot's date fields, the chart's date pickers, the filter "By" choice, the Calendar sort order
//! and the Python export all ask this module.
//!
//! - `CalendarSettings`: the first month of the fiscal year and the first day of the week (defaults
//!   January and Saturday). A step or spec that uses a date part keeps its own copy, so a macro replays
//!   the same result after the settings change.
//! - `DatePart`: Year, Quarter, Month-Year, Day name … each with its label, its type and its order.
//!
//! Week rules (quoted the same way in the guide, the tooltip and the Python export):
//! - a week starts on `week_start`; "Week starting" is the date of the week's first day, on or before the date;
//! - "Week of year" counts the weeks of the calendar year of the date: W01 is the week that contains
//!   1 January (so it can start in the previous December), W02 the next, and so on.
//!
//! Blank dates give blank parts: never "Unknown", never 0, never a default date.

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use polars::prelude::*;
use serde::{Deserialize, Serialize};

/// A day of the week, in the order the Calendar dialog lists them (Saturday first).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub enum Weekday {
    #[default]
    Sat,
    Sun,
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
}

impl Weekday {
    /// Saturday to Friday, as the dialog lists them.
    pub const ALL: [Weekday; 7] = [Weekday::Sat, Weekday::Sun, Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri];

    /// Days since Monday (Mon = 0 … Sun = 6), as chrono counts them.
    pub fn from_monday(self) -> u32 {
        match self {
            Weekday::Mon => 0,
            Weekday::Tue => 1,
            Weekday::Wed => 2,
            Weekday::Thu => 3,
            Weekday::Fri => 4,
            Weekday::Sat => 5,
            Weekday::Sun => 6,
        }
    }

    pub fn from_chrono(d: chrono::Weekday) -> Weekday {
        match d {
            chrono::Weekday::Mon => Weekday::Mon,
            chrono::Weekday::Tue => Weekday::Tue,
            chrono::Weekday::Wed => Weekday::Wed,
            chrono::Weekday::Thu => Weekday::Thu,
            chrono::Weekday::Fri => Weekday::Fri,
            chrono::Weekday::Sat => Weekday::Sat,
            chrono::Weekday::Sun => Weekday::Sun,
        }
    }

    /// "Sat".
    pub fn short(self) -> &'static str {
        match self {
            Weekday::Sat => "Sat",
            Weekday::Sun => "Sun",
            Weekday::Mon => "Mon",
            Weekday::Tue => "Tue",
            Weekday::Wed => "Wed",
            Weekday::Thu => "Thu",
            Weekday::Fri => "Fri",
        }
    }

    /// "Saturday".
    pub fn long(self) -> &'static str {
        match self {
            Weekday::Sat => "Saturday",
            Weekday::Sun => "Sunday",
            Weekday::Mon => "Monday",
            Weekday::Tue => "Tuesday",
            Weekday::Wed => "Wednesday",
            Weekday::Thu => "Thursday",
            Weekday::Fri => "Friday",
        }
    }

    /// The day named by a label ("sat", "Saturday", any case).
    pub fn parse(text: &str) -> Option<Weekday> {
        let t = text.trim().to_lowercase();
        Weekday::ALL.into_iter().find(|d| t == d.short().to_lowercase() || t == d.long().to_lowercase())
    }
}

pub const MONTHS_SHORT: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
pub const MONTHS_LONG: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// The month (1–12) named by a label ("aug", "August", "Sept", any case).
pub fn parse_month(text: &str) -> Option<u32> {
    let t = text.trim().to_lowercase();
    if t == "sept" {
        return Some(9);
    }
    (0..12).find(|&i| t == MONTHS_SHORT[i].to_lowercase() || t == MONTHS_LONG[i].to_lowercase()).map(|i| i as u32 + 1)
}

fn default_fy_start() -> u32 {
    1
}

/// The calendar a date part is worked out with. Stored in `settings.json` (View ▸ Calendar…) and
/// copied into every step and spec that uses a date part.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CalendarSettings {
    /// The first month of the fiscal year, 1 (January) to 12.
    #[serde(default = "default_fy_start")]
    pub fy_start_month: u32,
    /// The first day of the week.
    #[serde(default)]
    pub week_start: Weekday,
}

impl Default for CalendarSettings {
    fn default() -> Self {
        CalendarSettings { fy_start_month: 1, week_start: Weekday::Sat }
    }
}

impl CalendarSettings {
    /// "Week starts Saturday · Fiscal year from January": the line every dialog that uses date parts shows.
    pub fn status_line(&self) -> String {
        format!("Week starts {} \u{b7} Fiscal year from {}", self.week_start.long(), self.fy_month_name())
    }

    fn fy_month_name(&self) -> &'static str {
        MONTHS_LONG.get((self.fy_start_month.clamp(1, 12) - 1) as usize).copied().unwrap_or("January")
    }

    /// A fiscal start outside 1–12 (a hand-edited file) is refused, never clamped.
    pub fn validate(&self) -> Result<()> {
        if !(1..=12).contains(&self.fy_start_month) {
            anyhow::bail!("The fiscal year start month is {}. It must be 1 (January) to 12 (December). Set it again in View \u{25b8} Calendar\u{2026}.", self.fy_start_month);
        }
        Ok(())
    }

    /// The fiscal year a date is in, by the year it starts in.
    pub fn fiscal_start_year(&self, d: NaiveDate) -> i32 {
        if d.month() >= self.fy_start_month {
            d.year()
        } else {
            d.year() - 1
        }
    }

    /// The month of the fiscal year (1 = the first).
    pub fn fiscal_month(&self, d: NaiveDate) -> u32 {
        (d.month() + 12 - self.fy_start_month) % 12 + 1
    }

    /// "FY2026" with a January start; "FY2026-27" with any other.
    pub fn fiscal_year_label(&self, d: NaiveDate) -> String {
        let y = self.fiscal_start_year(d);
        if self.fy_start_month == 1 {
            format!("FY{}", y)
        } else {
            format!("FY{}-{:02}", y, (y + 1).rem_euclid(100))
        }
    }

    /// The first day of the week the date is in (on or before it).
    pub fn week_starting(&self, d: NaiveDate) -> NaiveDate {
        let back = (d.weekday().num_days_from_monday() + 7 - self.week_start.from_monday()) % 7;
        d - chrono::Duration::days(back as i64)
    }

    /// The week of the calendar year: W01 is the week that contains 1 January.
    pub fn week_of_year(&self, d: NaiveDate) -> u32 {
        let jan1 = NaiveDate::from_ymd_opt(d.year(), 1, 1).unwrap_or(d);
        let first = self.week_starting(jan1);
        ((self.week_starting(d) - first).num_days() / 7) as u32 + 1
    }

    /// Where a day name stands in the week (0 = the first day).
    pub fn day_position(&self, day: Weekday) -> u32 {
        (day.from_monday() + 7 - self.week_start.from_monday()) % 7
    }
}

/// The type of the column a date part writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartType {
    Int,
    Text,
    Date,
}

/// How the values of a part are ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartOrder {
    /// By value (a number).
    Numeric,
    /// By the Calendar order (month names, quarters, "Aug 2026" …).
    Calendar,
    /// By date.
    Date,
}

/// One part of a date.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum DatePart {
    #[default]
    Year,
    Quarter,
    YearQuarter,
    Month,
    MonthNumber,
    MonthYear,
    WeekOfYear,
    WeekStarting,
    DayName,
    DayOfMonth,
    Date,
    FiscalYear,
    FiscalQuarter,
    FiscalMonth,
}

/// The value of a part for one date.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PartValue {
    Int(i32),
    Text(String),
    Date(NaiveDate),
}

impl std::fmt::Display for PartValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PartValue::Int(i) => write!(f, "{}", i),
            PartValue::Text(t) => write!(f, "{}", t),
            PartValue::Date(d) => write!(f, "{}", d.format("%Y-%m-%d")),
        }
    }
}

impl DatePart {
    /// Every part, in the order the dialogs list them.
    pub const ALL: [DatePart; 14] = [
        DatePart::Year,
        DatePart::Quarter,
        DatePart::YearQuarter,
        DatePart::Month,
        DatePart::MonthNumber,
        DatePart::MonthYear,
        DatePart::WeekOfYear,
        DatePart::WeekStarting,
        DatePart::DayName,
        DatePart::DayOfMonth,
        DatePart::Date,
        DatePart::FiscalYear,
        DatePart::FiscalQuarter,
        DatePart::FiscalMonth,
    ];

    /// The parts the Split date dialog ticks first.
    pub const DEFAULT_SPLIT: [DatePart; 5] = [DatePart::Year, DatePart::Quarter, DatePart::Month, DatePart::MonthYear, DatePart::DayName];

    /// The parts the filter's "By" choice offers (besides the date itself).
    pub const FILTER_BY: [DatePart; 5] = [DatePart::Year, DatePart::MonthYear, DatePart::Month, DatePart::Quarter, DatePart::DayName];

    /// The words of the dialogs and of the new column names ("Open Date Month-Year").
    pub fn label(self) -> &'static str {
        match self {
            DatePart::Year => "Year",
            DatePart::Quarter => "Quarter",
            DatePart::YearQuarter => "Year-Quarter",
            DatePart::Month => "Month",
            DatePart::MonthNumber => "Month number",
            DatePart::MonthYear => "Month-Year",
            DatePart::WeekOfYear => "Week of year",
            DatePart::WeekStarting => "Week starting",
            DatePart::DayName => "Day name",
            DatePart::DayOfMonth => "Day of month",
            DatePart::Date => "Date",
            DatePart::FiscalYear => "Fiscal year",
            DatePart::FiscalQuarter => "Fiscal quarter",
            DatePart::FiscalMonth => "Fiscal month",
        }
    }

    /// An example label, for the dialogs.
    pub fn example(self) -> &'static str {
        match self {
            DatePart::Year => "2026",
            DatePart::Quarter => "Q1",
            DatePart::YearQuarter => "2026 Q1",
            DatePart::Month => "Jan",
            DatePart::MonthNumber => "1",
            DatePart::MonthYear => "Aug 2026",
            DatePart::WeekOfYear => "W01",
            DatePart::WeekStarting => "01 Aug 2026",
            DatePart::DayName => "Sat",
            DatePart::DayOfMonth => "1",
            DatePart::Date => "01 Aug 2026",
            DatePart::FiscalYear => "FY2026",
            DatePart::FiscalQuarter => "FQ1",
            DatePart::FiscalMonth => "1",
        }
    }

    pub fn from_label(text: &str) -> Option<DatePart> {
        let t = text.trim().to_lowercase();
        DatePart::ALL.into_iter().find(|p| p.label().to_lowercase() == t)
    }

    pub fn part_type(self) -> PartType {
        match self {
            DatePart::Year | DatePart::MonthNumber | DatePart::DayOfMonth | DatePart::FiscalMonth => PartType::Int,
            DatePart::WeekStarting | DatePart::Date => PartType::Date,
            _ => PartType::Text,
        }
    }

    pub fn dtype(self) -> DataType {
        match self.part_type() {
            PartType::Int => DataType::Int32,
            PartType::Text => DataType::String,
            PartType::Date => DataType::Date,
        }
    }

    pub fn part_order(self) -> PartOrder {
        match self.part_type() {
            PartType::Int => PartOrder::Numeric,
            PartType::Text => PartOrder::Calendar,
            PartType::Date => PartOrder::Date,
        }
    }

    /// The text order of the part's labels: Calendar (from the copied week start) for the text parts,
    /// none for numbers and dates (they sort as they are).
    pub fn sort_order(self, cal: &CalendarSettings) -> Option<crate::sort_lists::SortOrder> {
        match self.part_order() {
            PartOrder::Calendar => Some(crate::sort_lists::SortOrder::Calendar { week_start: cal.week_start }),
            _ => None,
        }
    }

    /// The part of one date.
    pub fn value(self, d: NaiveDate, cal: &CalendarSettings) -> PartValue {
        let q = (d.month() - 1) / 3 + 1;
        let month = MONTHS_SHORT.get((d.month() - 1) as usize).copied().unwrap_or("");
        match self {
            DatePart::Year => PartValue::Int(d.year()),
            DatePart::Quarter => PartValue::Text(format!("Q{}", q)),
            DatePart::YearQuarter => PartValue::Text(format!("{} Q{}", d.year(), q)),
            DatePart::Month => PartValue::Text(month.to_string()),
            DatePart::MonthNumber => PartValue::Int(d.month() as i32),
            DatePart::MonthYear => PartValue::Text(format!("{} {}", month, d.year())),
            DatePart::WeekOfYear => PartValue::Text(format!("W{:02}", cal.week_of_year(d))),
            DatePart::WeekStarting => PartValue::Date(cal.week_starting(d)),
            DatePart::DayName => PartValue::Text(Weekday::from_chrono(d.weekday()).short().to_string()),
            DatePart::DayOfMonth => PartValue::Int(d.day() as i32),
            DatePart::Date => PartValue::Date(d),
            DatePart::FiscalYear => PartValue::Text(cal.fiscal_year_label(d)),
            DatePart::FiscalQuarter => PartValue::Text(format!("FQ{}", (cal.fiscal_month(d) - 1) / 3 + 1)),
            DatePart::FiscalMonth => PartValue::Int(cal.fiscal_month(d) as i32),
        }
    }

    /// The part of a column as one expression. The column must be a Date or Datetime (a Datetime
    /// drops its time first); `check_date_column` says why another type is refused. A blank date
    /// gives a blank part.
    pub fn expr(self, column: &str, cal: CalendarSettings) -> Expr {
        let part = self;
        col(column).cast(DataType::Date).map(move |s: Series| part.apply(&s, &cal).map(Some), GetOutput::from_type(self.dtype()))
    }

    /// The part of every value of a Date series (or one that casts to Date).
    pub fn apply(self, s: &Series, cal: &CalendarSettings) -> PolarsResult<Series> {
        let name = s.name().clone();
        let days = s.cast(&DataType::Date)?.cast(&DataType::Int32)?;
        let days = days.i32()?;
        let dates = days.into_iter().map(|v| v.and_then(date_from_days));
        let out = match self.part_type() {
            PartType::Int => {
                let ca: Int32Chunked = dates
                    .map(|d| {
                        d.and_then(|d| match self.value(d, cal) {
                            PartValue::Int(i) => Some(i),
                            _ => None,
                        })
                    })
                    .collect();
                ca.into_series()
            }
            PartType::Text => {
                let ca: StringChunked = dates
                    .map(|d| {
                        d.and_then(|d| match self.value(d, cal) {
                            PartValue::Text(t) => Some(t),
                            _ => None,
                        })
                    })
                    .collect();
                ca.into_series()
            }
            PartType::Date => {
                let ca: Int32Chunked = dates
                    .map(|d| {
                        d.and_then(|d| match self.value(d, cal) {
                            PartValue::Date(x) => Some(days_from_date(x)),
                            _ => None,
                        })
                    })
                    .collect();
                ca.into_series().cast(&DataType::Date)?
            }
        };
        Ok(out.with_name(name))
    }
}

/// The days since 1970-01-01 of a date (Polars' Date).
pub fn days_from_date(d: NaiveDate) -> i32 {
    d.num_days_from_ce() - 719_163
}

/// The date of a Polars Date value.
pub fn date_from_days(days: i32) -> Option<NaiveDate> {
    NaiveDate::from_num_days_from_ce_opt(days.checked_add(719_163)?)
}

/// Refuses a column that is not a Date or Datetime, with the message of the spec. Nothing is guessed
/// from text.
pub fn check_date_column(name: &str, dtype: &DataType) -> Result<()> {
    match dtype {
        DataType::Date | DataType::Datetime(_, _) => Ok(()),
        DataType::String => anyhow::bail!("'{}' is text. Set it to Date in Schema & Ingestion (D/M/Y) first.", name),
        other if other.is_numeric() => anyhow::bail!("'{}' is a number, not a date. Set it to Date in Schema & Ingestion (D/M/Y) first.", name),
        other => anyhow::bail!("'{}' is {}, not a date. Set it to Date in Schema & Ingestion (D/M/Y) first.", name, other),
    }
}

/// True for a Date or Datetime column.
pub fn is_date_type(dtype: &DataType) -> bool {
    matches!(dtype, DataType::Date | DataType::Datetime(_, _))
}

// ------------------------------------------------------------------------------------ Calendar order

/// Where a Calendar label stands: month names, day names (from the week start), Q1–Q4, FQ1–FQ4,
/// W01–W54, "Aug 2026", "2026 Q1", "FY2026" and "FY2026-27". `None` for anything else. Labels of
/// different kinds keep apart (months before days before quarters …), so a mixed column still has
/// one order.
pub fn calendar_rank(label: &str, week_start: Weekday) -> Option<f64> {
    let t = label.trim();
    if t.is_empty() {
        return None;
    }
    let lower = t.to_lowercase();
    if let Some(m) = parse_month(&lower) {
        return Some(m as f64);
    }
    if let Some(d) = Weekday::parse(&lower) {
        let cal = CalendarSettings { fy_start_month: 1, week_start };
        return Some(100.0 + cal.day_position(d) as f64);
    }
    if let Some(n) = lower.strip_prefix("fq").and_then(small_number) {
        if (1..=4).contains(&n) {
            return Some(300.0 + n as f64);
        }
        return None;
    }
    if let Some(n) = lower.strip_prefix('q').and_then(small_number) {
        if (1..=4).contains(&n) {
            return Some(200.0 + n as f64);
        }
        return None;
    }
    if let Some(n) = lower.strip_prefix('w').and_then(small_number) {
        if (1..=54).contains(&n) {
            return Some(400.0 + n as f64);
        }
        return None;
    }
    if let Some(rest) = lower.strip_prefix("fy") {
        let rest = rest.trim();
        let (y, tail) = rest.split_at(rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len()));
        let year = year4(y)?;
        let tail = tail.trim();
        if tail.is_empty() {
            return Some(3_000_000.0 + year as f64);
        }
        let end = tail.strip_prefix('-').or_else(|| tail.strip_prefix('\u{2013}')).or_else(|| tail.strip_prefix('/'))?;
        let end = end.trim();
        let ok = (end.len() == 2 && end.parse::<i32>().ok() == Some((year + 1).rem_euclid(100))) || (end.len() == 4 && end.parse::<i32>().ok() == Some(year + 1));
        return ok.then_some(3_000_000.0 + year as f64);
    }
    let words: Vec<&str> = lower.split_whitespace().collect();
    if let [a, b] = words.as_slice() {
        // "Aug 2026"
        if let (Some(m), Some(y)) = (parse_month(a), year4(b)) {
            return Some(1_000_000.0 + (y as f64) * 100.0 + m as f64);
        }
        // "2026 Q1"
        if let (Some(y), Some(q)) = (year4(a), b.strip_prefix('q').and_then(small_number)) {
            if (1..=4).contains(&q) {
                return Some(2_000_000.0 + (y as f64) * 10.0 + q as f64);
            }
        }
    }
    None
}

fn small_number(t: &str) -> Option<u32> {
    let t = t.trim();
    if t.is_empty() || t.len() > 2 || !t.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    t.parse().ok()
}

fn year4(t: &str) -> Option<i32> {
    let t = t.trim();
    if t.len() != 4 || !t.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    t.parse().ok()
}

// ------------------------------------------------------------------------------------ Ranges order

/// What a range label says about its first number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RangeKey {
    /// The first number, scaled by its unit word (Crore × 1e7, months in days …).
    pub value: f64,
    /// 0 "Below x", 1 an exact "x", 2 a range that starts at x, 3 "Above x".
    pub kind: u8,
    /// A bare number with no unit, keyword or second number ("5"): such labels alone don't make a
    /// column a Ranges column.
    pub plain: bool,
}

/// The scale of a unit word: % none; Thousand/K 1e3; Lakh 1e5; Million/M 1e6; Crore/Cr 1e7; days,
/// weeks, months and years by their length in days.
fn unit_scale(word: &str) -> Option<f64> {
    Some(match word {
        "%" => 1.0,
        "k" | "thousand" | "thousands" => 1e3,
        "l" | "lakh" | "lakhs" | "lac" | "lacs" => 1e5,
        "m" | "mn" | "million" | "millions" => 1e6,
        "cr" | "crore" | "crores" => 1e7,
        "bn" | "billion" | "billions" => 1e9,
        "d" | "day" | "days" => 1.0,
        "w" | "wk" | "wks" | "week" | "weeks" => 7.0,
        "mo" | "mos" | "mon" | "month" | "months" => 365.0 / 12.0,
        "y" | "yr" | "yrs" | "year" | "years" => 365.0,
        _ => return None,
    })
}

/// A number at the start of `t` ("-1,250.5", "−3", "8"): its value and the rest of the text.
fn take_number(t: &str) -> Option<(f64, &str)> {
    let t = t.trim_start();
    let (neg, body) = if let Some(r) = t.strip_prefix('-') {
        (true, r)
    } else if let Some(r) = t.strip_prefix('\u{2212}') {
        (true, r)
    } else if let Some(r) = t.strip_prefix('+') {
        (false, r)
    } else {
        (false, t)
    };
    let end = body.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',')).unwrap_or(body.len());
    let digits = &body[..end];
    if !digits.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: f64 = digits.replace(',', "").parse().ok()?;
    if !n.is_finite() {
        return None;
    }
    Some((if neg { -n } else { n }, &body[end..]))
}

/// A unit right after a number ("%", " Crore", "Cr"): its scale and the rest.
fn take_unit(t: &str) -> (Option<f64>, &str) {
    if let Some(r) = t.strip_prefix('%') {
        return (Some(1.0), r);
    }
    let s = t.trim_start();
    let end = s.find(|c: char| !c.is_alphabetic()).unwrap_or(s.len());
    if end == 0 {
        return (None, t);
    }
    match unit_scale(&s[..end]) {
        Some(x) => (Some(x), &s[end..]),
        None => (None, t),
    }
}

/// Reads a range label: "8%–9%", "Below 0%", "> 15%", "1 Crore to 5 Crore", "Above 10 Crore",
/// "0%", "Up to 1 Lakh", "10 Crore and above". `None` for anything else ("No rate", "Other").
pub fn parse_range(label: &str) -> Option<RangeKey> {
    let lower = label.trim().to_lowercase();
    if lower.is_empty() {
        return None;
    }
    let mut t: &str = &lower;
    let mut kind: Option<u8> = None;
    for (prefix, k) in [
        ("below", 0u8),
        ("under", 0),
        ("less than", 0),
        ("up to", 0),
        ("upto", 0),
        ("<=", 0),
        ("\u{2264}", 0),
        ("<", 0),
        ("above", 3),
        ("over", 3),
        ("more than", 3),
        (">=", 3),
        ("\u{2265}", 3),
        (">", 3),
    ] {
        if let Some(r) = t.strip_prefix(prefix) {
            // a word prefix must end at a word boundary ("overdraft" is not "over")
            let word = prefix.chars().all(|c| c.is_alphabetic() || c == ' ');
            if word && r.chars().next().is_some_and(|c| c.is_alphabetic()) {
                continue;
            }
            t = r;
            kind = Some(k);
            break;
        }
    }
    let (first, rest) = take_number(t)?;
    let (unit1, rest) = take_unit(rest);
    let mut rest = rest.trim_start();
    let mut unit = unit1;
    let mut second = false;
    if kind.is_none() {
        let after_sep = ["\u{2013}", "\u{2014}", "-", "to "]
            .iter()
            .find_map(|sep| rest.strip_prefix(sep))
            .or_else(|| rest.strip_prefix("to").filter(|r| r.is_empty() || r.starts_with(' ')));
        if let Some(r) = after_sep {
            if let Some((_, r2)) = take_number(r) {
                let (unit2, r3) = take_unit(r2);
                if unit.is_none() {
                    unit = unit2;
                }
                rest = r3.trim_start();
                second = true;
            }
        }
    }
    // "10 Crore and above", "15%+", "5 years or more", "1 Lakh and below"
    let mut tail_kind = None;
    if !second && kind.is_none() {
        for (suffix, k) in [("and above", 3u8), ("& above", 3), ("or more", 3), ("and over", 3), ("+", 3), ("and below", 0), ("& below", 0), ("or less", 0)] {
            if rest == suffix {
                tail_kind = Some(k);
                rest = "";
                break;
            }
        }
    }
    if !rest.trim().is_empty() {
        return None;
    }
    let kind_v = kind.or(tail_kind).unwrap_or(if second { 2 } else { 1 });
    let plain = kind.is_none() && tail_kind.is_none() && !second && unit.is_none();
    Some(RangeKey { value: first * unit.unwrap_or(1.0), kind: kind_v, plain })
}

/// True for a "No …" label ("No rate", "No maturity"): it is a known label of a Ranges column and
/// goes after every ranked one.
pub fn is_no_label(label: &str) -> bool {
    let l = label.trim().to_lowercase();
    l == "no" || l.starts_with("no ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).expect("date")
    }

    #[test]
    fn the_week_rules_on_known_dates() {
        let sat = CalendarSettings::default();
        assert_eq!(sat.week_starting(d(2026, 1, 15)), d(2026, 1, 10));
        assert_eq!(sat.week_of_year(d(2026, 1, 15)), 3);
        let mon = CalendarSettings { week_start: Weekday::Mon, ..sat };
        assert_eq!(mon.week_starting(d(2026, 1, 15)), d(2026, 1, 12));
    }

    #[test]
    fn ranges_parse() {
        assert_eq!(parse_range("Below 0%").map(|k| (k.value, k.kind)), Some((0.0, 0)));
        assert_eq!(parse_range("8%\u{2013}9%").map(|k| (k.value, k.kind)), Some((8.0, 2)));
        assert_eq!(parse_range("> 15%").map(|k| (k.value, k.kind)), Some((15.0, 3)));
        assert_eq!(parse_range("1 Crore to 5 Crore").map(|k| (k.value, k.kind)), Some((1e7, 2)));
        assert_eq!(parse_range("Overdraft"), None);
        assert_eq!(parse_range("No rate"), None);
    }
}
