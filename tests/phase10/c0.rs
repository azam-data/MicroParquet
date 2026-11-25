//! C0: the Phase 9 carry-over. A month whose last calendar day has no rows gets a note on the pivot's notes (which the chart's footnote
//! reads too: `charts::calc` copies `PivotBuild::warnings`); the numbers are unchanged.

use crate::common::dir;
use crate::p9_m3::*;
use micro_parquet::models::*;
use polars::prelude::*;

const H: &str = "Period amount of Income (YTD)";

fn ytd(dates_: &[&str], income: &[f64], as_of: &str, name: &str) -> micro_parquet::pivot_table::PivotBuild {
    let dd = dir(name);
    let df = DataFrame::new(vec![dates("Date", dates_).into(), nums("Income", income)]).expect("data");
    let s = spec(&[&month_year()], &[], vec![time("Income", TimeAgg::PeriodAmount)], &["Month-Year"]);
    run(&dd, &df, scope(&dd, settings(1), as_of, vec![]), &s)
}

#[test]
fn c0_a_ytd_income_month_whose_last_day_has_no_rows_carries_the_note_and_the_same_number() {
    // rows through 29 Oct 2026 (the 30th and 31st have none), then 30 Nov: YTD income 100, 130, 160
    let b = ytd(&["2026-09-30", "2026-10-29", "2026-11-30"], &[100.0, 130.0, 160.0], "2026-11-30", "p10c0_a");
    // hand-worked: Oct = 130 - 100 = 30 and Nov = 160 - 130 = 30, as before the note existed
    assert_eq!((at(&b, "Oct 2026", H), at(&b, "Nov 2026", H)), (Some(30.0), Some(30.0)));
    let notes: Vec<&String> = b.warnings.iter().filter(|w| w.contains("last date with rows")).collect();
    assert_eq!(notes.len(), 1, "{:?}", b.warnings);
    assert_eq!(notes[0], "The last date with rows in Oct 2026 is 29 Oct; income posted after it counts in Nov 2026.");
}

#[test]
fn c0_a_month_whose_last_day_has_rows_has_no_note() {
    let b = ytd(&["2026-09-30", "2026-10-31", "2026-11-30"], &[100.0, 130.0, 160.0], "2026-11-30", "p10c0_b");
    assert_eq!(at(&b, "Oct 2026", H), Some(30.0));
    assert!(!b.warnings.iter().any(|w| w.contains("last date with rows")), "{:?}", b.warnings);
    // a month that is not over at the as-of date has no note either
    let b = ytd(&["2026-09-30", "2026-10-29"], &[100.0, 130.0], "2026-10-29", "p10c0_c");
    assert!(!b.warnings.iter().any(|w| w.contains("last date with rows")), "{:?}", b.warnings);
}

#[test]
fn c0_a_flow_gets_no_note_even_when_the_last_day_has_no_rows() {
    let dd = dir("p10c0_d");
    let df = DataFrame::new(vec![dates("Date", &["2026-10-28", "2026-10-29", "2026-11-30"]).into(), nums("Income", &[5.0, 6.0, 7.0])]).expect("data");
    let s = spec(&[&month_year()], &[], vec![time("Income", TimeAgg::TotalFlow)], &["Month-Year"]);
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-11-30", vec![]), &s);
    assert_eq!(at(&b, "Oct 2026", "Total of Income"), Some(11.0));
    assert!(!b.warnings.iter().any(|w| w.contains("last date with rows")), "{:?}", b.warnings);
}
