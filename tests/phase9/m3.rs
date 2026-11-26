//! M3: balances and flows. A balance is a position on a date, a flow an amount in a period. The aggregations (closing, opening and
//! average balance, total of a flow, period amount of a YTD balance, counts and a weighted average at closing) are worked out from
//! the daily series of a cell and the tab's snapshot dates; a total is the measure over its own rows and period, never a sum of the
//! cells. Every number here is worked by hand in the test.

use crate::common::{dir, floats, parquet, strings};
use chrono::NaiveDate;
use micro_parquet::dates::{CalendarSettings, Weekday};
use micro_parquet::measures::{suggest, ColumnKind, ColumnTag};
use micro_parquet::model::*;
use micro_parquet::model_calendar::date_link;
use micro_parquet::models::*;
use micro_parquet::pivot_table::{build_pivot, PivotBuild};
use polars::prelude::*;
use std::sync::Arc;

pub fn d(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("a date")
}

pub fn dates(name: &str, v: &[&str]) -> Series {
    let days: Vec<i32> = v.iter().map(|t| micro_parquet::dates::days_from_date(d(t))).collect();
    Series::new(name.into(), days).cast(&DataType::Date).expect("dates")
}

pub fn nums(name: &str, v: &[f64]) -> Series {
    Series::new(name.into(), v.to_vec())
}

pub fn strs(name: &str, v: &[&str]) -> Series {
    Series::new(name.into(), v.to_vec())
}

pub fn settings(fy_start_month: u32) -> CalendarSettings {
    CalendarSettings { fy_start_month, week_start: Weekday::Sat }
}

/// The model the worker reads: the data tab t1 "Accounts" (date column "Date") linked to the Calendar t2, as of `as_of`.
pub fn scope(dd: &std::path::Path, cal: CalendarSettings, as_of: &str, kinds: Vec<ColumnKind>) -> Option<Arc<ModelContext>> {
    let calendar = ModelTab { key: "t2".into(), title: "Calendar".into(), source: TabSource::Calendar { settings: cal, first_year: 2025, last_year: 2027 }, steps: vec![] };
    Some(Arc::new(ModelContext {
        links: vec![ScopeLink { link: date_link("t1", "Date", "t2"), lookup: Some(calendar), refusal: None }],
        plans: vec![],
        temp_dir: dd.to_path_buf(),
        calendar: Some(CalendarScope {
            settings: cal,
            tab: "t2".into(),
            date_links: vec![DateLink { data_tab: "t1".into(), column: "Date".into() }],
            as_of: Some(d(as_of)),
            titles: vec![("t1".into(), "Accounts".into())],
        }),
        kinds,
        measures: vec![],
        ..Default::default()
    }))
}

pub fn cal_field(column: &str) -> ModelFieldRef {
    ModelFieldRef::new(&date_link("t1", "Date", "t2"), "Accounts \u{2192} Calendar", "Calendar", column)
}

pub fn time(field: &str, agg: TimeAgg) -> PivotValue {
    PivotValue { field: field.into(), agg: PivotAgg::Sum, weight_col: None, time: Some(TimeValue::of(agg)) }
}

pub fn plain(field: &str, agg: PivotAgg) -> PivotValue {
    PivotValue { field: field.into(), agg, weight_col: None, time: None }
}

pub fn spec(rows: &[&str], columns: &[&str], values: Vec<PivotValue>, calendar: &[&str]) -> PivotSpec {
    PivotSpec {
        rows: rows.iter().map(|r| r.to_string()).collect(),
        columns: columns.iter().map(|r| r.to_string()).collect(),
        values,
        row_totals: true,
        column_totals: true,
        model_fields: calendar.iter().map(|c| cal_field(c)).collect(),
        // as the pivot builder adds a Calendar field: in the order of its date part (M2-9)
        field_orders: calendar
            .iter()
            .filter_map(|c| micro_parquet::dates::DatePart::from_label(c).and_then(|p| p.sort_order(&settings(1))).map(|order| PivotFieldOrder { field: cal_field(c).name, order }))
            .collect(),
        data_tab: "t1".into(),
        ..Default::default()
    }
}

pub fn month_year() -> String {
    cal_field("Month-Year").name
}

pub fn run(dd: &std::path::Path, df: &DataFrame, sc: Option<Arc<ModelContext>>, s: &PivotSpec) -> PivotBuild {
    let src = parquet(dd, "accounts", df);
    with_scope(sc, || build_pivot(micro_parquet::engine::compile_tab_lazy(&src, &[], dd).expect("accounts"), s)).unwrap_or_else(|e| panic!("the pivot: {e:#}"))
}

pub fn run_err(dd: &std::path::Path, df: &DataFrame, sc: Option<Arc<ModelContext>>, s: &PivotSpec) -> String {
    let src = parquet(dd, "accounts", df);
    with_scope(sc, || build_pivot(micro_parquet::engine::compile_tab_lazy(&src, &[], dd).expect("accounts"), s)).err().map(|e| format!("{e:#}")).expect("a refusal")
}

pub fn names(b: &PivotBuild) -> Vec<String> {
    b.df.get_column_names().iter().map(|n| n.to_string()).collect()
}

pub fn labels(b: &PivotBuild) -> Vec<String> {
    strings(&b.df, &names(b)[0]).into_iter().map(|l| l.unwrap_or_default()).collect()
}

/// The number of a row (by its label) in a column (by its heading).
pub fn at(b: &PivotBuild, row: &str, column: &str) -> Option<f64> {
    let r = labels(b).iter().position(|l| l == row).unwrap_or_else(|| panic!("no row '{row}' in {:?}", labels(b)));
    assert!(names(b).iter().any(|n| n == column), "no column '{column}' in {:?}", names(b));
    floats(&b.df, column)[r]
}

pub fn close(a: Option<f64>, b: f64) -> bool {
    a.is_some_and(|a| (a - b).abs() < 1e-9)
}

pub fn balance_kinds() -> Vec<ColumnKind> {
    vec![ColumnKind { tab: "t1".into(), column: "Balance".into(), kind: ColumnTag::Balance }]
}

// ------------------------------------------------------------------------------------------------ closing and average

#[test]
fn m3_a_daily_tab_closes_at_the_last_balance_and_the_average_carries_the_balance_over_the_missing_day() {
    let dd = dir("p9m3_daily");
    // 1 Nov = 100 and 3 Nov = 130 (2 Nov is missing); as of 3 Nov
    let df = DataFrame::new(vec![dates("Date", &["2026-11-01", "2026-11-03"]).into(), nums("Balance", &[100.0, 130.0])]).expect("data");
    let s = spec(&[&month_year()], &[], vec![time("Balance", TimeAgg::ClosingBalance), time("Balance", TimeAgg::AverageBalance), time("Balance", TimeAgg::OpeningBalance)], &["Month-Year"]);
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-11-03", vec![]), &s);
    assert_eq!(names(&b), [month_year().as_str(), "Closing balance of Balance", "Average balance of Balance", "Opening balance of Balance"].map(String::from));
    assert!(close(at(&b, "Nov 2026", "Closing balance of Balance"), 130.0), "closing is the value at 3 Nov");
    assert!(close(at(&b, "Nov 2026", "Average balance of Balance"), 110.0), "(100 + 100 + 130) / 3: 2 Nov carries 1 Nov's balance forward");
    assert!(close(at(&b, "Grand total", "Closing balance of Balance"), 130.0) && close(at(&b, "Grand total", "Average balance of Balance"), 110.0), "the Grand total is the measure of its own period");
    assert_eq!(at(&b, "Nov 2026", "Opening balance of Balance"), None, "nothing before 1 Nov");
    let why: Vec<String> = b.notes.iter().filter(|n| n.column == "Opening balance of Balance").map(|n| n.text.clone()).collect();
    assert_eq!(why.first().map(String::as_str), Some("No balance before 1 Nov 2026 for 'Accounts'"));
}

#[test]
fn m3_with_no_snapshot_on_or_before_the_start_the_average_is_blank_with_the_message_and_never_zero() {
    let dd = dir("p9m3_noprior");
    let df = DataFrame::new(vec![dates("Date", &["2026-11-03", "2026-11-04"]).into(), nums("Balance", &[130.0, 140.0])]).expect("data");
    let s = spec(&[&month_year()], &[], vec![time("Balance", TimeAgg::ClosingBalance), time("Balance", TimeAgg::AverageBalance)], &["Month-Year"]);
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-11-04", vec![]), &s);
    assert!(close(at(&b, "Nov 2026", "Closing balance of Balance"), 140.0));
    assert_eq!(at(&b, "Nov 2026", "Average balance of Balance"), None, "blank, not 0");
    let note = b.notes.iter().find(|n| n.column == "Average balance of Balance" && n.row == 0).expect("a reason");
    assert_eq!(note.text, "No balance on or before 1 Nov 2026. Add that date's data or start later.");
}

#[test]
fn m3_a_period_with_no_snapshot_before_the_as_of_date_is_blank_and_says_when_the_tab_ended() {
    let dd = dir("p9m3_blank_period");
    // the tab ends 31 Oct; a row dated 25 Nov exists but the as-of date is 20 Nov, so November has no balance yet
    let df = DataFrame::new(vec![dates("Date", &["2026-10-30", "2026-10-31", "2026-11-25"]).into(), nums("Balance", &[100.0, 110.0, 150.0])]).expect("data");
    let s = spec(&[&month_year()], &[], vec![time("Balance", TimeAgg::ClosingBalance)], &["Month-Year"]);
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-11-20", vec![]), &s);
    assert!(close(at(&b, "Oct 2026", "Closing balance of Balance"), 110.0));
    assert_eq!(at(&b, "Nov 2026", "Closing balance of Balance"), None);
    let note = b.notes.iter().find(|n| labels(&b)[n.row] == "Nov 2026").expect("a reason");
    assert_eq!(note.text, "No balance in Nov 2026 for 'Accounts' (last date 31 Oct 2026)");
}

#[test]
fn m3_a_month_end_tab_averages_its_month_ends_and_says_so_in_the_heading() {
    let dd = dir("p9m3_monthend");
    // 31 Jan = 100, 28 Feb = 120, 31 Mar = 140 (and 30 Apr = 150, so that there is a second quarter to open from)
    let df = DataFrame::new(vec![dates("Date", &["2026-01-31", "2026-02-28", "2026-03-31", "2026-04-30"]).into(), nums("Balance", &[100.0, 120.0, 140.0, 150.0])]).expect("data");
    let s = spec(&[&cal_field("Year-Quarter").name], &[], vec![time("Balance", TimeAgg::AverageBalance), time("Balance", TimeAgg::ClosingBalance), time("Balance", TimeAgg::OpeningBalance)], &["Year-Quarter"]);
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-04-30", vec![]), &s);
    let avg = "Average of month-ends of Balance";
    assert_eq!(names(&b)[1], avg, "headed as the average of the month-ends");
    assert!(close(at(&b, "2026 Q1", avg), 120.0), "(100 + 120 + 140) / 3");
    assert!(close(at(&b, "2026 Q1", "Closing balance of Balance"), 140.0));
    assert_eq!(at(&b, "2026 Q1", "Opening balance of Balance"), None, "nothing before 1 Jan: blank");
    assert!(close(at(&b, "2026 Q2", "Opening balance of Balance"), 140.0), "Q2 opens at the 31 Mar balance");
    assert!(close(at(&b, "2026 Q2", avg), 150.0));
    // the whole span: the mean of the four month-ends, not the sum and not the mean of the quarters
    assert!(close(at(&b, "Grand total", avg), (100.0 + 120.0 + 140.0 + 150.0) / 4.0));
}

// ------------------------------------------------------------------------------------------------ flows and YTD balances

#[test]
fn m3_a_ytd_balance_gives_the_movement_of_each_period_and_a_new_fiscal_year_starts_from_nothing() {
    let dd = dir("p9m3_ytd");
    // income, adding up from the start of the fiscal year: 31 Jan = 10, 28 Feb = 25, 31 Mar = 45
    let df = DataFrame::new(vec![dates("Date", &["2026-01-31", "2026-02-28", "2026-03-31"]).into(), nums("Income", &[10.0, 25.0, 45.0])]).expect("data");
    let s = spec(&[&month_year()], &[], vec![time("Income", TimeAgg::PeriodAmount)], &["Month-Year"]);
    let h = "Period amount of Income (YTD)";
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-03-31", vec![]), &s);
    assert_eq!(names(&b)[1], h);
    assert_eq!((at(&b, "Jan 2026", h), at(&b, "Feb 2026", h), at(&b, "Mar 2026", h)), (Some(10.0), Some(15.0), Some(20.0)), "Jan 10, Feb 25 - 10, Mar 45 - 25");
    let q = run(&dd, &df, scope(&dd, settings(1), "2026-03-31", vec![]), &spec(&[&cal_field("Year-Quarter").name], &[], vec![time("Income", TimeAgg::PeriodAmount)], &["Year-Quarter"]));
    assert!(close(at(&q, "2026 Q1", h), 45.0), "the quarter is the closing, nothing before it");
    // a fiscal year from February: 31 Jan was in the year before, so February starts again from nothing
    let b = run(&dd, &df, scope(&dd, settings(2), "2026-03-31", vec![]), &s);
    assert_eq!((at(&b, "Jan 2026", h), at(&b, "Feb 2026", h), at(&b, "Mar 2026", h)), (Some(10.0), Some(25.0), Some(20.0)), "Feb = 25: 31 Jan is in the previous fiscal year");
}

#[test]
fn m3_a_flow_total_is_the_sum_of_the_rows_dated_in_the_period() {
    let dd = dir("p9m3_flow");
    let df = DataFrame::new(vec![
        dates("Date", &["2026-10-05", "2026-10-05", "2026-10-20", "2026-11-02", "2026-11-02", "2026-11-18", "2026-11-30"]).into(),
        nums("Interest", &[1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0]),
    ])
    .expect("data");
    let s = spec(&[&month_year()], &[], vec![time("Interest", TimeAgg::TotalFlow), plain("Interest", PivotAgg::Sum)], &["Month-Year"]);
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-11-30", vec![]), &s);
    assert!(close(at(&b, "Oct 2026", "Total of Interest"), 7.0) && close(at(&b, "Nov 2026", "Total of Interest"), 120.0), "the sum of each month's rows");
    assert_eq!(at(&b, "Oct 2026", "Total of Interest"), at(&b, "Oct 2026", "Sum of Interest"), "the same as a plain Sum on a flow");
    assert!(close(at(&b, "Grand total", "Total of Interest"), 127.0), "the Grand total is the sum of its rows");
    // the as-of date cuts the period: as of 20 Nov the November total leaves 30 Nov out
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-11-20", vec![]), &s);
    assert!(close(at(&b, "Nov 2026", "Total of Interest"), 56.0), "8 + 16 + 32");
}

// ------------------------------------------------------------------------------------------------ counts at closing

#[test]
fn m3_counts_at_closing_count_each_account_once_whatever_the_number_of_snapshot_dates() {
    let dd = dir("p9m3_counts");
    // three accounts on each of three dates; a fourth only on the first date (closed since)
    let mut d_: Vec<&str> = Vec::new();
    let (mut ac, mut bal, mut rate): (Vec<&str>, Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new(), Vec::new());
    for date in ["2026-11-01", "2026-11-02", "2026-11-03"] {
        for (a, b, r) in [("A1", 100.0, 10.0), ("A2", 200.0, 20.0), ("A3", 300.0, 30.0)] {
            d_.push(date);
            ac.push(a);
            bal.push(b);
            rate.push(r);
        }
    }
    d_.push("2026-11-01");
    ac.push("A4");
    bal.push(50.0);
    rate.push(40.0);
    let df = DataFrame::new(vec![dates("Date", &d_).into(), strs("AC Number", &ac).into(), nums("Balance", &bal), nums("Rate", &rate)]).expect("data");
    let w = PivotValue { field: "Rate".into(), agg: PivotAgg::Sum, weight_col: Some("Balance".into()), time: Some(TimeValue::of(TimeAgg::WAvgAtClosing)) };
    let s = spec(&[&month_year()], &[], vec![time("AC Number", TimeAgg::CountAtClosing), time("AC Number", TimeAgg::DistinctCountAtClosing), plain("AC Number", PivotAgg::Count), w], &["Month-Year"]);
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-11-03", vec![]), &s);
    assert_eq!(at(&b, "Nov 2026", "Count at closing of AC Number"), Some(3.0), "the three accounts on 3 Nov, once each");
    assert_eq!(at(&b, "Nov 2026", "Distinct count at closing of AC Number"), Some(3.0));
    assert_eq!(at(&b, "Nov 2026", "Count of AC Number"), Some(10.0), "a plain Count counts every row of every date");
    // (100 x 10 + 200 x 20 + 300 x 30) / 600
    assert!(close(at(&b, "Nov 2026", "W.Avg at closing of Rate by Balance"), 14_000.0 / 600.0));
    // an account on the first date only is counted when the period closes on the first date
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-11-01", vec![]), &s);
    assert_eq!(at(&b, "Nov 2026", "Count at closing of AC Number"), Some(4.0));
}

// ------------------------------------------------------------------------------------------------ totals are measured, never added

/// Two branches of one region and a third of another, on 1 and 15 Oct and 1 and 3 Nov.
fn branches() -> DataFrame {
    let rows: [(&str, &str, &str, f64); 12] = [
        ("2026-10-01", "B1", "East", 100.0),
        ("2026-10-01", "B2", "East", 50.0),
        ("2026-10-01", "B3", "West", 10.0),
        ("2026-10-15", "B1", "East", 200.0),
        ("2026-10-15", "B2", "East", 50.0),
        ("2026-10-15", "B3", "West", 10.0),
        ("2026-11-01", "B1", "East", 300.0),
        ("2026-11-01", "B2", "East", 70.0),
        ("2026-11-01", "B3", "West", 20.0),
        ("2026-11-03", "B1", "East", 330.0),
        ("2026-11-03", "B2", "East", 70.0),
        ("2026-11-03", "B3", "West", 20.0),
    ];
    DataFrame::new(vec![
        dates("Date", &rows.iter().map(|r| r.0).collect::<Vec<_>>()).into(),
        strs("Branch", &rows.iter().map(|r| r.1).collect::<Vec<_>>()).into(),
        strs("Region", &rows.iter().map(|r| r.2).collect::<Vec<_>>()).into(),
        nums("Balance", &rows.iter().map(|r| r.3).collect::<Vec<_>>()),
    ])
    .expect("branches")
}

#[test]
fn m3_the_total_column_is_the_average_over_the_whole_span_and_not_the_sum_of_the_months() {
    let dd = dir("p9m3_total");
    let s = spec(&["Branch"], &[&month_year()], vec![time("Balance", TimeAgg::AverageBalance)], &["Month-Year"]);
    let b = run(&dd, &branches(), scope(&dd, settings(1), "2026-11-03", vec![]), &s);
    assert_eq!(names(&b), ["Branch", "Oct 2026", "Nov 2026", "Total"].map(String::from));
    // B1: October 100 for 1 to 14 (14 days) and 200 for 15 to 31 (17 days); November 300, 300, 330
    let (oct, nov) = ((14.0 * 100.0 + 17.0 * 200.0) / 31.0, (300.0 + 300.0 + 330.0) / 3.0);
    assert!(close(at(&b, "B1", "Oct 2026"), oct) && close(at(&b, "B1", "Nov 2026"), nov));
    let total = (14.0 * 100.0 + 17.0 * 200.0 + 300.0 + 300.0 + 330.0) / 34.0;
    assert!(close(at(&b, "B1", "Total"), total), "34 days from 1 Oct to 3 Nov: {:?} vs {total}", at(&b, "B1", "Total"));
    assert!((at(&b, "B1", "Total").unwrap_or(0.0) - (oct + nov)).abs() > 1.0, "not the sum of the months");
    assert!((at(&b, "B1", "Total").unwrap_or(0.0) - (oct + nov) / 2.0).abs() > 1.0, "not the mean of the months' means");
    // the Grand total row is the measure of all rows: the branches' balances added on each date
    let grand_total = (14.0 * 160.0 + 17.0 * 260.0 + 390.0 + 390.0 + 420.0) / 34.0;
    assert!(close(at(&b, "Grand total", "Total"), grand_total), "{:?} vs {grand_total}", at(&b, "Grand total", "Total"));
    assert!(close(at(&b, "Grand total", "Nov 2026"), (390.0 + 390.0 + 420.0) / 3.0));
}

#[test]
fn m3_a_subtotal_is_the_measure_of_its_own_rows_and_equals_a_pivot_by_the_parent_field_alone() {
    let dd = dir("p9m3_subtotal");
    let sc = scope(&dd, settings(1), "2026-11-03", vec![]);
    let mut by_both = spec(&["Region", "Branch"], &[&month_year()], vec![time("Balance", TimeAgg::AverageBalance), time("Balance", TimeAgg::ClosingBalance)], &["Month-Year"]);
    by_both.subtotals = true;
    let both = run(&dd, &branches(), sc.clone(), &by_both);
    let alone = run(&dd, &branches(), sc.clone(), &spec(&["Region"], &[&month_year()], vec![time("Balance", TimeAgg::AverageBalance), time("Balance", TimeAgg::ClosingBalance)], &["Month-Year"]));
    let column_names = names(&both);
    for region in ["East", "West"] {
        let r = strings(&both.df, "Region").iter().position(|l| l.as_deref() == Some(&format!("{region} Total"))).unwrap_or_else(|| panic!("a subtotal of {region}"));
        let a = labels(&alone).iter().position(|l| l == region).expect("the parent alone");
        for c in &column_names[2..] {
            assert_eq!(floats(&both.df, c)[r], floats(&alone.df, c)[a], "{region} subtotal in '{c}' equals the pivot by Region alone");
        }
    }
    // the East subtotal is not the sum of B1 and B2 by accident: its closing is the sum, its average is over the same days
    let east = strings(&both.df, "Region").iter().position(|l| l.as_deref() == Some("East Total")).expect("East");
    assert!(close(floats(&both.df, "Total | Closing balance of Balance")[east], 400.0), "East closes at B1 330 + B2 70 on 3 Nov: {:?}", names(&both));
}

#[test]
fn m3_nested_subtotals_of_a_measure_equal_a_pivot_by_the_parent_field_alone() {
    let dd = dir("p9m3_nested");
    let sc = scope(&dd, settings(1), "2026-11-03", vec![]);
    let src = parquet(&dd, "accounts", &branches());
    let mut nested = spec(&["Region", "Branch"], &[], vec![time("Balance", TimeAgg::AverageBalance), time("Balance", TimeAgg::ClosingBalance)], &[]);
    nested.layout = PivotLayout::Nested { blank_line_after_group: false };
    let lf = || micro_parquet::engine::compile_tab_lazy(&src, &[], &dd).expect("accounts");
    let view = with_scope(sc.clone(), || micro_parquet::pivot_view::build_nested(lf(), &nested)).expect("the nested view");
    let alone = with_scope(sc, || build_pivot(lf(), &spec(&["Region"], &[], vec![time("Balance", TimeAgg::AverageBalance), time("Balance", TimeAgg::ClosingBalance)], &[]))).expect("by Region");
    for region in ["East", "West"] {
        let line = view.rows.iter().position(|r| r.is_subtotal() && r.label == region).unwrap_or_else(|| panic!("a parent line {region}"));
        let a = labels(&alone).iter().position(|l| l == region).expect("alone");
        for c in ["Average balance of Balance", "Closing balance of Balance"] {
            assert_eq!(floats(&view.values, c)[line], floats(&alone.df, c)[a], "{region}: the parent line in '{c}'");
        }
    }
    // a pivot with no Calendar field takes the whole span, from the first snapshot date to the as-of date: 1 Oct to 3 Nov
    let east = labels(&alone).iter().position(|l| l == "East").expect("East");
    let want = (14.0 * 150.0 + 17.0 * 250.0 + 370.0 + 370.0 + 400.0) / 34.0;
    assert!(close(floats(&alone.df, "Average balance of Balance")[east], want), "{:?} vs {want}", floats(&alone.df, "Average balance of Balance")[east]);
}

// ------------------------------------------------------------------------------------------------ the stock warning

#[test]
fn m3_a_plain_sum_of_a_tagged_balance_over_many_dates_warns_and_an_untagged_column_or_an_old_pivot_does_not() {
    let dd = dir("p9m3_warning");
    // 23 daily snapshots of one branch
    let days: Vec<String> = (1..=23).map(|i| format!("2026-11-{:02}", i)).collect();
    let df = DataFrame::new(vec![dates("Date", &days.iter().map(String::as_str).collect::<Vec<_>>()).into(), strs("Branch", &vec!["B1"; 23]).into(), nums("Balance", &vec![10.0; 23]), nums("Fee", &vec![1.0; 23])]).expect("data");
    let sc = scope(&dd, settings(1), "2026-11-23", balance_kinds());
    let sum_of = |col: &str| spec(&["Branch"], &[], vec![plain(col, PivotAgg::Sum)], &[]);
    let b = run(&dd, &df, sc.clone(), &sum_of("Balance"));
    assert_eq!(b.warnings, ["Sum of Balance adds 23 dates of balances. Use Closing balance or Average balance."]);
    assert_eq!(at(&b, "B1", "Sum of Balance"), Some(230.0), "a warning, not a refusal: the number is the sum");
    let c = run(&dd, &df, sc.clone(), &spec(&["Branch"], &[], vec![plain("Balance", PivotAgg::Count)], &[]));
    assert_eq!(c.warnings, ["Count of Balance adds 23 dates of balances. Use Closing balance or Average balance."]);
    assert!(run(&dd, &df, sc.clone(), &sum_of("Fee")).warnings.is_empty(), "an untagged column has no warning");
    // the closing balance is the right aggregation: no stock warning, only the note of the span it covers (M4 follow-up)
    assert_eq!(run(&dd, &df, sc.clone(), &spec(&["Branch"], &[], vec![time("Balance", TimeAgg::ClosingBalance)], &[])).warnings, ["Closing balance of Balance: whole data, 1 Nov 2026 \u{2013} 23 Nov 2026 (as of 23 Nov 2026)"]);
    // an old pivot (no data tab), and the same pivot with the tag removed, give the same numbers and no warning
    let mut old = sum_of("Balance");
    old.data_tab = String::new();
    let o = run(&dd, &df, sc.clone(), &old);
    assert!(o.warnings.is_empty(), "an old pivot has no banner");
    assert_eq!(at(&o, "B1", "Sum of Balance"), Some(230.0), "and the same numbers");
    assert!(run(&dd, &df, scope(&dd, settings(1), "2026-11-23", vec![]), &sum_of("Balance")).warnings.is_empty(), "no tag, no warning");
    // rows that span one date only add one balance: no warning
    let one = DataFrame::new(vec![dates("Date", &["2026-11-23", "2026-11-23"]).into(), strs("Branch", &["B1", "B2"]).into(), nums("Balance", &[10.0, 20.0])]).expect("one date");
    assert!(run(&dd, &one, sc, &sum_of("Balance")).warnings.is_empty());
}

// ------------------------------------------------------------------------------------------------ what is refused

#[test]
fn m3_a_time_aggregation_outside_a_project_or_without_a_date_link_or_on_a_date_part_is_refused_in_words() {
    let dd = dir("p9m3_refusals");
    let df = DataFrame::new(vec![dates("Date", &["2026-11-01", "2026-11-03"]).into(), nums("Balance", &[100.0, 130.0])]).expect("data");
    let s = spec(&[&month_year()], &[], vec![time("Balance", TimeAgg::ClosingBalance)], &["Month-Year"]);
    let e = run_err(&dd, &df, None, &s);
    assert!(e.contains("Open the project to run it"), "{e}");
    let mut no_tab = s.clone();
    no_tab.data_tab = String::new();
    let e = run_err(&dd, &df, scope(&dd, settings(1), "2026-11-03", vec![]), &no_tab);
    assert!(e.contains("needs a date-linked data tab"), "{e}");
    // a scope whose calendar has no date link for this tab
    let mut sc = scope(&dd, settings(1), "2026-11-03", vec![]).map(|c| (*c).clone()).expect("scope");
    if let Some(c) = sc.calendar.as_mut() {
        c.date_links.clear();
    }
    let e = run_err(&dd, &df, Some(Arc::new(sc)), &s);
    assert!(e.contains("needs a date link, and 'Accounts' has none"), "{e}");
    // the as-of date not read yet
    let mut sc = scope(&dd, settings(1), "2026-11-03", vec![]).map(|c| (*c).clone()).expect("scope");
    if let Some(c) = sc.calendar.as_mut() {
        c.as_of = None;
    }
    let e = run_err(&dd, &df, Some(Arc::new(sc)), &s);
    assert!(e.contains("as-of date is not known yet"), "{e}");
    // a text column cannot be closed
    let text = DataFrame::new(vec![dates("Date", &["2026-11-01"]).into(), strs("Branch", &["B1"]).into()]).expect("data");
    let e = run_err(&dd, &text, scope(&dd, settings(1), "2026-11-03", vec![]), &spec(&[&month_year()], &[], vec![time("Branch", TimeAgg::ClosingBalance)], &["Month-Year"]));
    assert!(e.contains("needs a number column"), "{e}");
    // W.Avg at closing needs its weight
    let e = run_err(&dd, &df, scope(&dd, settings(1), "2026-11-03", vec![]), &spec(&[&month_year()], &[], vec![time("Balance", TimeAgg::WAvgAtClosing)], &["Month-Year"]));
    assert!(e.contains("needs a weight column"), "{e}");
    // a share of a balance means nothing
    let mut share = s.clone();
    share.show_as = PivotShowAs::PctOfGrandTotal;
    let e = run_err(&dd, &df, scope(&dd, settings(1), "2026-11-03", vec![]), &share);
    assert!(e.contains("cannot be shown as a share"), "{e}");
}

// ------------------------------------------------------------------------------------------------ tags, suggestions, files

#[test]
fn m3_the_name_of_a_column_suggests_a_tag_and_a_name_that_says_both_or_neither_suggests_nothing() {
    for (name, want) in [
        ("Balance", Some(ColumnTag::Balance)),
        ("Outstanding", Some(ColumnTag::Balance)),
        ("Bal", Some(ColumnTag::Balance)),
        ("OS", Some(ColumnTag::Balance)),
        ("Loan Outstanding", Some(ColumnTag::Balance)),
        ("OutstandingBalance", Some(ColumnTag::Balance)),
        ("os_amount", Some(ColumnTag::Balance)),
        ("Interest", Some(ColumnTag::Flow)),
        ("Interest Income", Some(ColumnTag::Flow)),
        ("Expense", Some(ColumnTag::Flow)),
        ("Disbursed", Some(ColumnTag::Flow)),
        ("Recovery", Some(ColumnTag::Flow)),
        ("Interest Balance", None),
        ("Rate", None),
        ("Gross", None),
        ("Posted", None),
        ("Limit", None),
    ] {
        assert_eq!(suggest(name), want, "{name}");
    }
}

#[test]
fn m3_old_files_load_unchanged_and_the_new_fields_are_written_only_when_used() {
    // a pivot value without a time part is written as it always was
    let v = plain("Balance", PivotAgg::Sum);
    let json = serde_json::to_string(&v).expect("json");
    assert!(!json.contains("time"), "{json}");
    let old: PivotValue = serde_json::from_str(r#"{"field":"Balance","agg":"Sum","weight_col":null}"#).expect("old");
    assert_eq!(old, v);
    let t = time("Balance", TimeAgg::ClosingBalance);
    let back: PivotValue = serde_json::from_str(&serde_json::to_string(&t).expect("json")).expect("back");
    assert_eq!(back, t);
    // the pivot's data tab is written only when set
    let mut s = spec(&["Branch"], &[], vec![v], &[]);
    s.data_tab = String::new();
    assert!(!serde_json::to_string(&s).expect("json").contains("data_tab"));
    let old: PivotSpec = serde_json::from_str(r#"{"rows":["Branch"],"values":[{"field":"Balance"}]}"#).expect("old spec");
    assert!(old.data_tab.is_empty() && !old.has_time_values());
    // the model's tags round-trip and an empty model is still empty
    let mut m = Model::default();
    assert!(m.is_empty());
    m.column_kinds = balance_kinds();
    assert!(!m.is_empty());
    let back: Model = serde_json::from_str(&serde_json::to_string(&m).expect("json")).expect("back");
    assert_eq!(back, m);
    let old: Model = serde_json::from_str(r#"{"links":[]}"#).expect("an old model");
    assert!(old.column_kinds.is_empty());
}

#[test]
fn m3_a_chart_of_a_sum_of_a_tagged_balance_over_many_dates_says_so_in_its_notes_and_an_untagged_column_or_closing_does_not() {
    use micro_parquet::charts::spec::{ChartAgg, ChartSpec};
    let dd = dir("p9m3_chart");
    let days: Vec<String> = (1..=23).map(|i| format!("2026-11-{:02}", i)).collect();
    let df = DataFrame::new(vec![dates("Date", &days.iter().map(String::as_str).collect::<Vec<_>>()).into(), strs("Branch", &vec!["B1"; 23]).into(), nums("Balance", &vec![10.0; 23]), nums("Fee", &vec![1.0; 23])]).expect("data");
    let src = parquet(&dd, "accounts", &df);
    let chart = |value: &str, data_tab: &str| ChartSpec { category: "Branch".into(), value: Some(value.into()), agg: ChartAgg::Sum, data_tab: data_tab.into(), ..Default::default() };
    let notes_of = |spec: &ChartSpec, kinds: Vec<ColumnKind>| -> Vec<String> {
        let sc = scope(&dd, settings(1), "2026-11-23", kinds);
        with_scope(sc, || micro_parquet::charts::calc::compute(micro_parquet::engine::compile_tab_lazy(&src, &[], &dd).expect("accounts"), spec)).expect("the chart").notes
    };
    let w = "Sum of Balance adds 23 dates of balances. Use Closing balance or Average balance.";
    assert!(notes_of(&chart("Balance", "t1"), balance_kinds()).iter().any(|n| n == w), "the chart note");
    assert!(!notes_of(&chart("Fee", "t1"), balance_kinds()).iter().any(|n| n.contains("adds")), "an untagged column has none");
    assert!(!notes_of(&chart("Balance", ""), balance_kinds()).iter().any(|n| n.contains("adds")), "a chart with no data tab (an old chart) has none");
    assert!(!notes_of(&chart("Balance", "t1"), vec![]).iter().any(|n| n.contains("adds")), "no tag, no note");
    // the numbers are the same with and without the warning
    let value_of = |spec: &ChartSpec, kinds: Vec<ColumnKind>| {
        let sc = scope(&dd, settings(1), "2026-11-23", kinds);
        with_scope(sc, || micro_parquet::charts::calc::compute(micro_parquet::engine::compile_tab_lazy(&src, &[], &dd).expect("accounts"), spec)).expect("the chart").values[0][0]
    };
    assert_eq!(value_of(&chart("Balance", "t1"), balance_kinds()), value_of(&chart("Balance", ""), vec![]));
}

// ------------------------------------------------------------------------------------------------ Show rows on a time cell (S7)

/// The rows tab Show rows opens for the cell (row label, column heading): the tab's source, the filter and the steps that follow.
fn rows_tab(dd: &std::path::Path, df: &DataFrame, sc: Option<Arc<ModelContext>>, s: &PivotSpec, b: &PivotBuild, row: &str, column: &str) -> (DataFrame, Option<String>) {
    let src = parquet(dd, "accounts", df);
    let r = labels(b).iter().position(|l| l == row).unwrap_or_else(|| panic!("no row '{row}'"));
    with_scope(sc, || {
        let cell = micro_parquet::pivot_table::cell_rows(s, b, r, column).unwrap_or_else(|e| panic!("show rows of {row} / {column}: {e:#}"));
        let mut steps = vec![TransformStep::FilterGroup { conditions: cell.conditions.clone(), join: GroupJoin::And, keep_matching: true }];
        steps.extend(cell.extra_steps.clone());
        let lf = micro_parquet::engine::compile_tab_lazy(&src, &[], dd).expect("source");
        let out = micro_parquet::engine::apply_steps_to(lf, &steps).expect("steps").collect().expect("rows");
        (out, cell.note)
    })
}

fn date_texts(df: &DataFrame, column: &str) -> Vec<Option<String>> {
    let c = df.column(column).expect("date column").cast(&DataType::String).expect("as text");
    c.str().expect("text").into_iter().map(|v| v.map(str::to_string)).collect()
}

fn sum_of(df: &DataFrame, column: &str) -> f64 {
    floats(df, column).into_iter().flatten().sum()
}

fn accounts_by_day() -> DataFrame {
    // two branches; 31 Oct, 1 Nov and 3 Nov (2 Nov missing); each row an account and an interest amount
    DataFrame::new(vec![
        dates("Date", &["2026-10-31", "2026-10-31", "2026-11-01", "2026-11-01", "2026-11-03", "2026-11-03", "2026-11-03"]).into(),
        strs("Branch", &["B1", "B2", "B1", "B2", "B1", "B1", "B2"]).into(),
        nums("Balance", &[100.0, 50.0, 110.0, 60.0, 130.0, 20.0, 70.0]),
        nums("Interest", &[1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0]),
        strs("AC Number", &["A1", "A2", "A1", "A2", "A1", "A3", "A2"]).into(),
    ])
    .expect("data")
}

#[test]
fn m3_show_rows_of_a_closing_opening_total_or_count_cell_gives_the_rows_whose_sum_or_count_is_the_cell() {
    let dd = dir("p9m3_rows_basic");
    let df = accounts_by_day();
    let sc = scope(&dd, settings(1), "2026-11-03", balance_kinds());
    let s = spec(&["Branch"], &[&month_year()], vec![time("Balance", TimeAgg::ClosingBalance), time("Balance", TimeAgg::OpeningBalance), time("Interest", TimeAgg::TotalFlow), time("AC Number", TimeAgg::CountAtClosing)], &["Month-Year"]);
    let b = run(&dd, &df, sc.clone(), &s);
    let h = |agg: &str, what: &str| format!("Nov 2026 | {agg} of {what}");
    let (closing, opening, total, count) = (h("Closing balance", "Balance"), h("Opening balance", "Balance"), h("Total", "Interest"), h("Count at closing", "AC Number"));
    for branch in ["B1", "B2"] {
        // Closing: the rows of the closing date (3 Nov), whose sum is the cell
        let (rows, note) = rows_tab(&dd, &df, sc.clone(), &s, &b, branch, &closing);
        assert!(close(at(&b, branch, &closing), sum_of(&rows, "Balance")), "{branch}: the closing rows add up to the cell");
        assert!(date_texts(&rows, "Date").iter().all(|d| d.as_deref() == Some("2026-11-03")), "{branch}: only the closing date");
        assert_eq!(note, None);
        // Opening: the rows of the opening date (31 Oct)
        let (rows, _) = rows_tab(&dd, &df, sc.clone(), &s, &b, branch, &opening);
        assert!(close(at(&b, branch, &opening), sum_of(&rows, "Balance")), "{branch}: the opening rows add up to the cell");
        assert!(date_texts(&rows, "Date").iter().all(|d| d.as_deref() == Some("2026-10-31")), "{branch}: only the opening date");
        // Total (flow): the rows dated in the period (not 31 Oct)
        let (rows, _) = rows_tab(&dd, &df, sc.clone(), &s, &b, branch, &total);
        assert!(close(at(&b, branch, &total), sum_of(&rows, "Interest")), "{branch}: the flow rows add up to the cell");
        assert!(date_texts(&rows, "Date").iter().all(|d| d.as_deref().is_some_and(|d| d.starts_with("2026-11"))), "{branch}: only rows dated in November");
        // Count at closing: the number of rows of the closing date
        let (rows, _) = rows_tab(&dd, &df, sc.clone(), &s, &b, branch, &count);
        assert!(close(at(&b, branch, &count), rows.height() as f64), "{branch}: the count of the closing rows is the cell");
    }
    // B1 closes at 130 + 20 on 3 Nov: two rows, 150; a Total column cell reads its own rows
    let (rows, _) = rows_tab(&dd, &df, sc.clone(), &s, &b, "B1", &closing);
    assert_eq!((rows.height(), sum_of(&rows, "Balance")), (2, 150.0));
}

#[test]
fn m3_show_rows_of_a_period_amount_gives_the_rows_of_its_two_dates_with_a_part_column_and_end_minus_start_is_the_cell() {
    let dd = dir("p9m3_rows_ytd");
    let df = DataFrame::new(vec![dates("Date", &["2026-01-31", "2026-02-28", "2026-03-31"]).into(), nums("Income", &[10.0, 25.0, 45.0])]).expect("data");
    let sc = scope(&dd, settings(1), "2026-03-31", vec![]);
    let s = spec(&[&month_year()], &[], vec![time("Income", TimeAgg::PeriodAmount)], &["Month-Year"]);
    let h = "Period amount of Income (YTD)";
    let b = run(&dd, &df, sc.clone(), &s);
    for (month, cell) in [("Feb 2026", 15.0), ("Mar 2026", 20.0)] {
        let (rows, _) = rows_tab(&dd, &df, sc.clone(), &s, &b, month, h);
        let parts = strings(&rows, "Part");
        let inc = floats(&rows, "Income");
        let part_sum = |p: &str| -> f64 { parts.iter().zip(inc.iter()).filter(|(x, _)| x.as_deref() == Some(p)).map(|(_, v)| v.unwrap_or(0.0)).sum() };
        assert_eq!(rows.height(), 2, "{month}: one row for each of the two dates");
        assert!(close(at(&b, month, h), cell) && close(Some(part_sum("End") - part_sum("Start")), cell), "{month}: End minus Start is the cell");
    }
    // January has no earlier date in the fiscal year: only the End row, and End is the cell
    let (rows, _) = rows_tab(&dd, &df, sc.clone(), &s, &b, "Jan 2026", h);
    assert_eq!((rows.height(), strings(&rows, "Part")[0].clone()), (1, Some("End".to_string())));
}

#[test]
fn m3_show_rows_of_an_average_gives_the_rows_dated_in_the_period_and_the_date_carried_in_with_a_note_and_a_blank_cell_has_none() {
    let dd = dir("p9m3_rows_avg");
    // the daily example: 1 Nov = 100, 3 Nov = 130, as of 3 Nov
    let df = DataFrame::new(vec![dates("Date", &["2026-11-01", "2026-11-03"]).into(), nums("Balance", &[100.0, 130.0])]).expect("data");
    let sc = scope(&dd, settings(1), "2026-11-03", vec![]);
    let s = spec(&[&month_year()], &[], vec![time("Balance", TimeAgg::AverageBalance), time("Balance", TimeAgg::OpeningBalance)], &["Month-Year"]);
    let b = run(&dd, &df, sc.clone(), &s);
    let (rows, note) = rows_tab(&dd, &df, sc.clone(), &s, &b, "Nov 2026", "Average balance of Balance");
    let got: Vec<String> = date_texts(&rows, "Date").into_iter().flatten().collect();
    assert_eq!(got, ["2026-11-01", "2026-11-03"], "1 Nov and 3 Nov");
    assert_eq!(note.as_deref(), Some("These rows make up an average; their sum is not the cell."));
    // a period that starts after the first snapshot carries the earlier date in
    let df = DataFrame::new(vec![dates("Date", &["2026-10-29", "2026-10-30", "2026-11-03"]).into(), nums("Balance", &[90.0, 100.0, 130.0])]).expect("data");
    let b = run(&dd, &df, sc.clone(), &s);
    let (rows, _) = rows_tab(&dd, &df, sc.clone(), &s, &b, "Nov 2026", "Average balance of Balance");
    let got: Vec<String> = date_texts(&rows, "Date").into_iter().flatten().collect();
    assert_eq!(got, ["2026-10-30", "2026-11-03"], "the date carried in (30 Oct, the last one before 1 Nov), then the rows of the period");
    // a blank cell (Opening before the first date) has no rows
    let df = DataFrame::new(vec![dates("Date", &["2026-11-01", "2026-11-03"]).into(), nums("Balance", &[100.0, 130.0])]).expect("data");
    let b = run(&dd, &df, sc.clone(), &s);
    assert_eq!(at(&b, "Nov 2026", "Opening balance of Balance"), None);
    let r = labels(&b).iter().position(|l| l == "Nov 2026").expect("row");
    let e = with_scope(sc, || micro_parquet::pivot_table::cell_rows(&s, &b, r, "Opening balance of Balance")).err().map(|e| format!("{e:#}")).expect("a refusal");
    assert!(e.contains("blank"), "{e}");
}
