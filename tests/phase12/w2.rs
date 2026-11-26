//! Phase 12 W2: rolling windows (3 and 12 months), moving averages, and the running total that resets each fiscal year (the YTD
//! window on a Month-Year row). Every number is worked by hand from the synthetic daily tab of the Phase 9 M4 tests.

use crate::common::dir;
use crate::p9_m3::*;
use crate::p9_m4::{daily, tv};
use micro_parquet::models::*;

fn ord(text: &str) -> f64 {
    (d(text) - d("2024-12-31")).num_days() as f64
}

fn values(window: TimeWindow, smooth: Smooth, agg: TimeAgg, field: &str) -> PivotValue {
    PivotValue { field: field.into(), agg: PivotAgg::Sum, weight_col: None, time: Some(TimeValue { agg, window, smooth, ..Default::default() }) }
}

fn note_for(b: &micro_parquet::pivot_table::PivotBuild, row: &str) -> String {
    b.notes.iter().find(|n| labels(b)[n.row] == row).map(|n| n.text.clone()).unwrap_or_default()
}

#[test]
fn w2_a_rolling_3_and_12_months_sum_the_calendar_months_ending_with_the_row() {
    let dd = dir("p12w2_roll");
    let df = daily("2025-01-01", "2026-11-24");
    let s = spec(
        &[&month_year()],
        &[],
        vec![values(TimeWindow::Rolling3, Smooth::None, TimeAgg::TotalFlow, "Interest"), values(TimeWindow::Rolling12, Smooth::None, TimeAgg::TotalFlow, "Interest")],
        &["Month-Year"],
    );
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-11-24", vec![]), &s);
    let h = names(&b);
    assert_eq!(h[1..], ["Total of Interest \u{b7} Rolling 3M", "Total of Interest \u{b7} Rolling 12M"].map(String::from));
    // Jan-Mar 2025 = 90 days at 2 a day; Jan-Mar 2026 = 90 days at 1 a day
    assert!(close(at(&b, "Mar 2025", &h[1]), 180.0));
    assert!(close(at(&b, "Mar 2026", &h[1]), 90.0));
    // Nov 2025 + Dec 2025 + Jan 2026 = 30*2 + 31*2 + 31
    assert!(close(at(&b, "Jan 2026", &h[1]), 153.0), "the 3 months cross the year end");
    // Jan-Dec 2025 = 365 days at 2; Apr 2025-Mar 2026 = 275 days at 2 plus 90 at 1
    assert!(close(at(&b, "Dec 2025", &h[2]), 730.0));
    assert!(close(at(&b, "Mar 2026", &h[2]), 640.0));
    // the as-of month is cut at 24 Nov: Dec 2025-Nov 2026 reads 31*2 + 334-31 days... = Dec 2025 (62) + Jan-Oct 2026 (304) + 24
    assert!(close(at(&b, "Nov 2026", &h[2]), 62.0 + 304.0 + 24.0), "{:?}", at(&b, "Nov 2026", &h[2]));
}

#[test]
fn w2_b_a_rolling_window_that_starts_before_the_first_data_is_blank_with_the_reason_not_a_short_sum() {
    let dd = dir("p12w2_early");
    let df = daily("2025-01-01", "2026-11-24");
    let s = spec(&[&month_year()], &[], vec![values(TimeWindow::Rolling12, Smooth::None, TimeAgg::TotalFlow, "Interest")], &["Month-Year"]);
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-11-24", vec![]), &s);
    let h = names(&b);
    assert!(at(&b, "Nov 2025", &h[1]).is_none(), "Dec 2024-Nov 2025 starts before Jan 2025");
    let n = note_for(&b, "Nov 2025");
    assert!(n.contains("Rolling 12 months") && n.contains("before the first data (Jan 2025)"), "{n}");
    assert!(close(at(&b, "Dec 2025", &h[1]), 730.0), "Jan-Dec 2025 is complete");
}

#[test]
fn w2_c_the_ytd_window_is_the_running_total_and_starts_again_with_each_fiscal_year() {
    let dd = dir("p12w2_run");
    let df = daily("2025-01-01", "2026-11-24");
    let s = spec(&[&month_year()], &[], vec![tv("Interest", TimeAgg::TotalFlow, TimeWindow::Ytd, TimeShift::None, TimeShow::Value, TimePartial::SameDays)], &["Month-Year"]);
    // January fiscal year
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-11-24", vec![]), &s);
    let h = names(&b);
    assert!(close(at(&b, "Dec 2025", &h[1]), 730.0));
    assert!(close(at(&b, "Jan 2026", &h[1]), 31.0), "resets in January");
    assert!(close(at(&b, "Mar 2026", &h[1]), 90.0));
    // July fiscal year: Jul 2025-Jun 2026 = 184 days at 2 plus 181 at 1; July starts again
    let b = run(&dd, &df, scope(&dd, settings(7), "2026-11-24", vec![]), &s);
    assert!(close(at(&b, "Jun 2026", &h[1]), 368.0 + 181.0), "{:?}", at(&b, "Jun 2026", &h[1]));
    assert!(close(at(&b, "Jul 2026", &h[1]), 31.0), "resets in July");
}

#[test]
fn w2_d_a_moving_average_is_the_mean_of_this_period_and_the_ones_before_and_says_when_it_cannot_be_worked_out() {
    let dd = dir("p12w2_ma");
    let df = daily("2025-01-01", "2026-11-24");
    let s = spec(
        &[&month_year()],
        &[],
        vec![
            values(TimeWindow::Period, Smooth::Moving3, TimeAgg::ClosingBalance, "Balance"),
            values(TimeWindow::Period, Smooth::Moving3, TimeAgg::TotalFlow, "Interest"),
            values(TimeWindow::Period, Smooth::Moving12, TimeAgg::TotalFlow, "Interest"),
        ],
        &["Month-Year"],
    );
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-11-24", vec![]), &s);
    let h = names(&b);
    assert_eq!(h[1], "Closing balance of Balance \u{b7} 3-period moving average");
    let want = (ord("2026-01-31") + ord("2026-02-28") + ord("2026-03-31")) / 3.0;
    assert!(close(at(&b, "Mar 2026", &h[1]), want));
    // the as-of month is only a closing balance of the 24th, which is still a fair last point
    let want_nov = (ord("2026-11-24") + ord("2026-10-31") + ord("2026-09-30")) / 3.0;
    assert!(close(at(&b, "Nov 2026", &h[1]), want_nov));
    // a flow: (31 + 28 + 31) / 3 = 30 a month in early 2026; Jan 2026 mixes 2 a day and 1 a day: (31*2 + 31*2... )
    assert!(close(at(&b, "Mar 2026", &h[2]), 30.0));
    assert!(close(at(&b, "Jan 2026", &h[2]), (31.0 + 62.0 + 60.0) / 3.0), "Nov 2025 (60) + Dec 2025 (62) + Jan 2026 (31)");
    // not finished: November is cut at the 24th, so a flow average that includes it is refused
    assert!(at(&b, "Nov 2026", &h[2]).is_none());
    assert!(note_for(&b, "Nov 2026").contains("is not finished (data to 24 Nov 2026)"), "{}", note_for(&b, "Nov 2026"));
    // reaching back before the first data
    assert!(at(&b, "Feb 2025", &h[1]).is_none());
    assert!(note_for(&b, "Feb 2025").contains("before the first data (Jan 2025)"), "{}", note_for(&b, "Feb 2025"));
    assert!(close(at(&b, "Dec 2025", &h[3]), 730.0 / 12.0), "12 months of 2025 at 2 a day");
    assert!(at(&b, "Nov 2025", &h[3]).is_none(), "needs Dec 2024");
}

#[test]
fn w2_e_a_moving_average_with_a_shift_or_a_window_is_refused_in_plain_words() {
    let dd = dir("p12w2_refuse");
    let df = daily("2025-01-01", "2026-11-24");
    let bad = PivotValue { field: "Interest".into(), agg: PivotAgg::Sum, weight_col: None, time: Some(TimeValue { agg: TimeAgg::TotalFlow, smooth: Smooth::Moving3, shift: TimeShift::Sply, show: TimeShow::Value, ..Default::default() }) };
    let e = run_err(&dd, &df, scope(&dd, settings(1), "2026-11-24", vec![]), &spec(&[&month_year()], &[], vec![bad], &["Month-Year"]));
    assert!(e.contains("a moving average works on the cell's own period"), "{e}");
}

#[test]
fn w2_f_old_files_load_and_the_new_choices_are_not_written_while_they_are_the_defaults() {
    let old: TimeValue = serde_json::from_str("{\"agg\":\"TotalFlow\",\"window\":\"Ytd\"}").expect("an M4 value loads");
    assert_eq!((old.smooth, old.window), (Smooth::None, TimeWindow::Ytd));
    assert!(!serde_json::to_string(&old).expect("json").contains("smooth"));
    let new = TimeValue { smooth: Smooth::Moving6, window: TimeWindow::Rolling3, ..Default::default() };
    let back: TimeValue = serde_json::from_str(&serde_json::to_string(&new).expect("json")).expect("round trip");
    assert_eq!(back, new);
    assert!(new.is_m4());
}
