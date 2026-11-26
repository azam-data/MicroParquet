//! M4: time intelligence. A time value has a window (Period, MTD, QTD, YTD), a shift (Prior period, same period last year), what it
//! shows (Value, Difference, % change) and how a partial period is shifted (the same days, or the full period). Every number here is
//! worked by hand from a synthetic daily tab.

use crate::common::{dir, floats};
use crate::p9_m3::*;
use chrono::NaiveDate;
use micro_parquet::models::*;
use micro_parquet::pivot_table::PivotBuild;
use polars::prelude::*;

pub fn tv(field: &str, agg: TimeAgg, window: TimeWindow, shift: TimeShift, show: TimeShow, partial: TimePartial) -> PivotValue {
    PivotValue { field: field.into(), agg: PivotAgg::Sum, weight_col: None, time: Some(TimeValue { agg, window, shift, show, partial, ..Default::default() }) }
}

fn day(text: &str) -> NaiveDate {
    d(text)
}

/// One row a day from `from` to `to`: Balance = days since 31 Dec 2024 (1 Jan 2025 = 1, 24 Nov 2025 = 328, 24 Nov 2026 = 693),
/// Interest = 2 a day in 2025 and 1 a day after, Branch "B1".
pub fn daily(from: &str, to: &str) -> DataFrame {
    let (a, b) = (day(from), day(to));
    let epoch = day("2024-12-31");
    let mut dts: Vec<String> = Vec::new();
    let (mut bal, mut int) = (Vec::new(), Vec::new());
    let mut x = a;
    while x <= b {
        dts.push(x.format("%Y-%m-%d").to_string());
        bal.push((x - epoch).num_days() as f64);
        int.push(if x < day("2026-01-01") { 2.0 } else { 1.0 });
        x = x.succ_opt().expect("next day");
    }
    let refs: Vec<&str> = dts.iter().map(String::as_str).collect();
    DataFrame::new(vec![dates("Date", &refs).into(), strs("Branch", &vec!["B1"; refs.len()]).into(), nums("Balance", &bal), nums("Interest", &int)]).expect("daily data")
}

fn ord(text: &str) -> f64 {
    (day(text) - day("2024-12-31")).num_days() as f64
}

pub fn one(b: &PivotBuild, column: &str) -> f64 {
    let v = floats(&b.df, column);
    assert_eq!(v.len(), 2, "B1 and the Grand total: {:?}", names(b));
    v[0].unwrap_or_else(|| panic!("a number in '{column}': {:?}", b.notes.iter().map(|n| n.text.clone()).collect::<Vec<_>>()))
}

#[test]
fn m4_mtd_and_sply_use_the_same_days_by_default_and_the_whole_period_when_asked() {
    let dd = dir("p9m4_sply");
    let df = daily("2025-01-01", "2026-11-24");
    let sc = scope(&dd, settings(1), "2026-11-24", vec![]);
    let (p, w, n, v) = (TimeWindow::Period, TimeWindow::Mtd, TimeShift::None, TimeShow::Value);
    let _ = p;
    let same = TimePartial::SameDays;
    let full = TimePartial::FullPeriod;
    let s = spec(
        &["Branch"],
        &[],
        vec![
            tv("Interest", TimeAgg::TotalFlow, w, n, v, same),
            tv("Interest", TimeAgg::TotalFlow, w, TimeShift::Sply, v, same),
            tv("Interest", TimeAgg::TotalFlow, w, TimeShift::Sply, v, full),
            tv("Balance", TimeAgg::ClosingBalance, w, TimeShift::Sply, v, same),
            tv("Balance", TimeAgg::ClosingBalance, w, TimeShift::Sply, v, full),
            tv("Balance", TimeAgg::ClosingBalance, w, TimeShift::Sply, TimeShow::Difference, same),
            tv("Balance", TimeAgg::ClosingBalance, w, TimeShift::Sply, TimeShow::PercentChange, same),
        ],
        &[],
    );
    let b = run(&dd, &df, sc, &s);
    assert_eq!(
        names(&b)[1..],
        [
            "Total of Interest \u{b7} MTD",
            "Total of Interest \u{b7} MTD \u{b7} SPLY",
            "Total of Interest \u{b7} MTD \u{b7} SPLY \u{b7} full period",
            "Closing balance of Balance \u{b7} MTD \u{b7} SPLY",
            "Closing balance of Balance \u{b7} MTD \u{b7} SPLY \u{b7} full period",
            "Closing balance of Balance \u{b7} MTD \u{b7} vs SPLY",
            "Closing balance of Balance \u{b7} MTD \u{b7} vs SPLY %",
        ]
        .map(String::from),
        "the headings name the choices"
    );
    let h = names(&b);
    assert_eq!(one(&b, &h[1]), 24.0, "MTD as of 24 Nov 2026: 1-24 Nov at 1 a day");
    assert_eq!(one(&b, &h[2]), 48.0, "SPLY with the same days: 1-24 Nov 2025 at 2 a day");
    assert_eq!(one(&b, &h[3]), 60.0, "SPLY with the full period: all 30 days of Nov 2025 at 2 a day");
    assert_eq!(one(&b, &h[4]), ord("2025-11-24"), "the balance of 24 Nov 2025 = 328");
    assert_eq!(one(&b, &h[5]), ord("2025-11-30"), "the balance of 30 Nov 2025 = 334");
    assert_eq!(one(&b, &h[6]), ord("2026-11-24") - ord("2025-11-24"), "693 - 328 = 365");
    assert!((one(&b, &h[7]) - 365.0 / 328.0 * 100.0).abs() < 1e-9, "(693 - 328) / 328 in percent");
}

#[test]
fn m4_prior_period_of_mtd_as_of_31_mar_is_1_to_28_feb_and_a_leap_day_goes_to_28_feb_the_next_year() {
    let dd = dir("p9m4_clamp");
    let df = daily("2025-01-01", "2026-03-31");
    let (w, v) = (TimeWindow::Mtd, TimeShow::Value);
    let mk = |partial| spec(&["Branch"], &[], vec![tv("Interest", TimeAgg::TotalFlow, w, TimeShift::None, v, partial), tv("Interest", TimeAgg::TotalFlow, w, TimeShift::PriorPeriod, v, partial)], &[]);
    for partial in [TimePartial::SameDays, TimePartial::FullPeriod] {
        let b = run(&dd, &df, scope(&dd, settings(1), "2026-03-31", vec![]), &mk(partial));
        let h = names(&b);
        let (now, before) = (one(&b, &h[1]), one(&b, &h[2]));
        assert_eq!((now, before), (31.0, 28.0), "{partial:?}: 1-31 Mar 2026 against 1-28 Feb 2026 (a window that is not cut gives the same either way)");
    }
    // a leap day: 1-29 Feb 2028 against 1-28 Feb 2027 (1 a day from 2026 on)
    let df = daily("2026-01-01", "2028-02-29");
    let s = spec(&["Branch"], &[], vec![tv("Interest", TimeAgg::TotalFlow, w, TimeShift::None, v, TimePartial::SameDays), tv("Interest", TimeAgg::TotalFlow, w, TimeShift::Sply, v, TimePartial::SameDays)], &[]);
    let b = run(&dd, &df, scope(&dd, settings(1), "2028-02-29", vec![]), &s);
    let h = names(&b);
    assert_eq!((one(&b, &h[1]), one(&b, &h[2])), (29.0, 28.0), "29 Feb 2028 goes back to 28 Feb 2027");
    // a cut window: as of 30 Mar the prior month's same days are 1-28 Feb (clamped), not 1-30
    let df = daily("2025-01-01", "2026-03-30");
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-03-30", vec![]), &mk(TimePartial::SameDays));
    let h = names(&b);
    assert_eq!((one(&b, &h[1]), one(&b, &h[2])), (30.0, 28.0), "1-30 Mar against 1-28 Feb: the day number is clamped to the month's last day");
}

#[test]
fn m4_ytd_with_a_july_fiscal_year_as_of_15_jan_covers_1_jul_to_15_jan() {
    let dd = dir("p9m4_ytd");
    let df = daily("2025-01-01", "2026-01-15");
    let (w, n, v, same) = (TimeWindow::Ytd, TimeShift::None, TimeShow::Value, TimePartial::SameDays);
    let s = spec(&["Branch"], &[], vec![tv("Interest", TimeAgg::TotalFlow, w, n, v, same), tv("Interest", TimeAgg::TotalFlow, TimeWindow::Qtd, n, v, same)], &[]);
    let b = run(&dd, &df, scope(&dd, settings(7), "2026-01-15", vec![]), &s);
    let h = names(&b);
    // 1 Jul 2025 - 31 Dec 2025: 184 days at 2 a day, then 15 days of January at 1 a day
    assert_eq!(one(&b, &h[1]), 184.0 * 2.0 + 15.0, "1 Jul 2025 to 15 Jan 2026");
    // the fiscal quarter of 15 Jan 2026 with a July year is Jan-Mar: 1-15 Jan
    assert_eq!(one(&b, &h[2]), 15.0, "QTD: 1-15 Jan 2026");
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-01-15", vec![]), &s);
    assert_eq!(one(&b, &names(&b)[1]), 15.0, "with a January year YTD is 1-15 Jan");
}

#[test]
fn m4_a_pivot_by_month_year_shows_each_months_change_against_the_month_before_and_blank_where_that_is_zero() {
    let dd = dir("p9m4_pct");
    let df = daily("2025-01-01", "2026-11-24");
    let sc = scope(&dd, settings(1), "2026-11-24", vec![]);
    let (pp, v) = (TimeShift::PriorPeriod, TimeShow::PercentChange);
    let s = spec(&[&month_year()], &[], vec![tv("Balance", TimeAgg::ClosingBalance, TimeWindow::Period, pp, v, TimePartial::SameDays)], &["Month-Year"]);
    let b = run(&dd, &df, sc.clone(), &s);
    let h = "Closing balance of Balance \u{b7} vs Prior period %";
    assert_eq!(names(&b)[1], h);
    // Dec 2025 against Nov 2025, and Jan 2026 against Dec of the previous year
    let pct = |this: &str, last: &str| (ord(this) - ord(last)) / ord(last).abs() * 100.0;
    assert!(close(at(&b, "Dec 2025", h), pct("2025-12-31", "2025-11-30")));
    assert!(close(at(&b, "Jan 2026", h), pct("2026-01-31", "2025-12-31")), "January against December of the previous year");
    // November 2026 is cut at the as-of date: 1-24 Nov against the same days of October (1-24 Oct)
    assert!(close(at(&b, "Nov 2026", h), pct("2026-11-24", "2026-10-24")), "the partial month against the same days of the month before");
    // January 2025 has no month before it in the data: blank with the reason
    assert_eq!(at(&b, "Jan 2025", h), None);
    assert!(b.notes.iter().any(|n| labels(&b)[n.row] == "Jan 2025" && n.text.starts_with("Earlier window (")), "{:?}", b.notes);
    // 0 -> blank: month-ends 31 Jan = 0, 28 Feb = 10, 31 Mar = 20
    let df = DataFrame::new(vec![dates("Date", &["2026-01-31", "2026-02-28", "2026-03-31"]).into(), nums("Balance", &[0.0, 10.0, 20.0])]).expect("data");
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-03-31", vec![]), &s);
    assert_eq!(at(&b, "Feb 2026", h), None, "the earlier value is 0: no % change, never 0 or infinity");
    assert!(b.notes.iter().any(|n| labels(&b)[n.row] == "Feb 2026" && n.text.contains("is 0")), "{:?}", b.notes);
    assert!(close(at(&b, "Mar 2026", h), 100.0), "(20 - 10) / 10");
    // the Difference of the same pivot
    let s = spec(&[&month_year()], &[], vec![tv("Balance", TimeAgg::ClosingBalance, TimeWindow::Period, pp, TimeShow::Difference, TimePartial::SameDays)], &["Month-Year"]);
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-03-31", vec![]), &s);
    assert!(close(at(&b, "Mar 2026", "Closing balance of Balance \u{b7} vs Prior period"), 10.0));
}

#[test]
fn m4_a_period_window_needs_a_period_field_and_a_field_that_mixes_years_is_refused_in_words() {
    let dd = dir("p9m4_refuse");
    let df = daily("2026-01-01", "2026-03-31");
    let sc = scope(&dd, settings(1), "2026-03-31", vec![]);
    let (pp, v, same) = (TimeShift::PriorPeriod, TimeShow::Value, TimePartial::SameDays);
    let e = run_err(&dd, &df, sc.clone(), &spec(&["Branch"], &[], vec![tv("Balance", TimeAgg::ClosingBalance, TimeWindow::Period, pp, v, same)], &[]));
    assert!(e.contains("Add a Calendar field that names a period (Calendar \u{203a} Month-Year, for example) to Rows or Columns, or choose MTD, QTD or YTD."), "{e}");
    let month = cal_field("Month").name;
    let e = run_err(&dd, &df, sc.clone(), &spec(&[&month], &[], vec![tv("Balance", TimeAgg::ClosingBalance, TimeWindow::Period, pp, v, same)], &["Month"]));
    assert!(e.contains("'Calendar \u{203a} Month' mixes years. Use Month-Year"), "{e}");
    // MTD needs no period field (its anchor is the as-of date), a mixing field is still refused
    let b = run(&dd, &df, sc.clone(), &spec(&["Branch"], &[], vec![tv("Balance", TimeAgg::ClosingBalance, TimeWindow::Mtd, pp, v, same)], &[]));
    assert!(close(floats(&b.df, "Closing balance of Balance \u{b7} MTD \u{b7} Prior period")[0], ord("2026-02-28")), "MTD of March, one month back: February's closing");
    let e = run_err(&dd, &df, sc.clone(), &spec(&[&month], &[], vec![tv("Balance", TimeAgg::ClosingBalance, TimeWindow::Mtd, TimeShift::None, v, same)], &["Month"]));
    assert!(e.contains("mixes years"), "{e}");
    // a difference needs a shift
    let e = run_err(&dd, &df, sc, &spec(&[&month_year()], &[], vec![tv("Balance", TimeAgg::ClosingBalance, TimeWindow::Period, TimeShift::None, TimeShow::PercentChange, same)], &["Month-Year"]));
    assert!(e.contains("needs a shift"), "{e}");
    // the plain M3 pivot (no choice made) is not refused without a period field
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-03-31", vec![]), &spec(&["Branch"], &[], vec![time("Balance", TimeAgg::ClosingBalance)], &[]));
    assert!(close(floats(&b.df, "Closing balance of Balance")[0], ord("2026-03-31")));
}

#[test]
fn m4_a_period_after_the_as_of_date_is_blank_and_the_as_of_date_moves_the_mtd_window() {
    let dd = dir("p9m4_asof");
    let df = daily("2026-01-01", "2026-11-24");
    let (v, n, same) = (TimeShow::Value, TimeShift::None, TimePartial::SameDays);
    let s = spec(&["Branch"], &[], vec![tv("Interest", TimeAgg::TotalFlow, TimeWindow::Mtd, n, v, same)], &[]);
    let h = "Total of Interest \u{b7} MTD";
    for (as_of, expected) in [("2026-11-24", 24.0), ("2026-11-10", 10.0), ("2026-10-31", 31.0)] {
        let b = run(&dd, &df, scope(&dd, settings(1), as_of, vec![]), &s);
        assert_eq!(one(&b, h), expected, "MTD as of {as_of}");
    }
    // Nov 2026 starts after an as-of date of 31 Oct: blank, with the reason, also with a shift
    let s = spec(&[&month_year()], &[], vec![tv("Balance", TimeAgg::ClosingBalance, TimeWindow::Period, TimeShift::PriorPeriod, TimeShow::PercentChange, same)], &["Month-Year"]);
    let b = run(&dd, &df, scope(&dd, settings(1), "2026-10-31", vec![]), &s);
    let h = "Closing balance of Balance \u{b7} vs Prior period %";
    assert_eq!(at(&b, "Nov 2026", h), None);
    assert!(b.notes.iter().any(|n| labels(&b)[n.row] == "Nov 2026" && n.text.starts_with("No balance in Nov 2026")), "{:?}", b.notes.iter().map(|n| n.text.clone()).collect::<Vec<_>>());
}

#[test]
fn m4_the_new_choices_are_written_only_when_used_and_an_m3_file_loads_unchanged() {
    let m3 = TimeValue::of(TimeAgg::AverageBalance);
    assert_eq!(serde_json::to_string(&m3).expect("json"), r#"{"agg":"AverageBalance"}"#, "an M3 value keeps its bytes");
    let back: TimeValue = serde_json::from_str(r#"{"agg":"AverageBalance"}"#).expect("an M3 file");
    assert_eq!(back, m3);
    let full = TimeValue { agg: TimeAgg::ClosingBalance, window: TimeWindow::Ytd, shift: TimeShift::Sply, show: TimeShow::PercentChange, partial: TimePartial::FullPeriod, ..Default::default() };
    let text = serde_json::to_string(&full).expect("json");
    assert_eq!(serde_json::from_str::<TimeValue>(&text).expect("round trip"), full, "{text}");
    assert_eq!(full.heading("Balance", None, false), "Closing balance of Balance \u{b7} YTD \u{b7} vs SPLY % \u{b7} full period");
}

// ------------------------------------------------------------------------------------------------ charts share the pivot's value type

fn chart_of(dd: &std::path::Path, df: &DataFrame, sc: Option<std::sync::Arc<micro_parquet::model::ModelContext>>, time: TimeValue, series: Option<&str>) -> micro_parquet::charts::data::ChartData {
    use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec};
    let f = cal_field("Month-Year");
    let src = crate::common::parquet(dd, "accounts", df);
    let mut spec = ChartSpec { category: f.name.clone(), category_model: Some(f), value: Some("Balance".into()), agg: ChartAgg::Sum, time: Some(time), kind: ChartKind::Bar, data_tab: "t1".into(), ..Default::default() };
    if let Some(s) = series {
        spec.series = Some(s.to_string());
    }
    micro_parquet::model::with_scope(sc, || micro_parquet::charts::calc::compute(micro_parquet::engine::compile_tab_lazy(&src, &[], dd).expect("source"), &spec)).unwrap_or_else(|e| panic!("the chart: {e:#}"))
}

#[test]
fn m4_a_chart_of_closing_average_and_closing_vs_sply_percent_by_month_year_equals_the_pivot() {
    let dd = dir("p9m4_chart");
    let df = daily("2025-01-01", "2026-11-24");
    let sc = scope(&dd, settings(1), "2026-11-24", vec![]);
    for (time, heading) in [
        (TimeValue::of(TimeAgg::ClosingBalance), "Closing balance of Balance"),
        (TimeValue::of(TimeAgg::AverageBalance), "Average balance of Balance"),
        (TimeValue { agg: TimeAgg::ClosingBalance, shift: TimeShift::Sply, show: TimeShow::PercentChange, ..Default::default() }, "Closing balance of Balance \u{b7} vs SPLY %"),
        (TimeValue { agg: TimeAgg::AverageBalance, window: TimeWindow::Ytd, shift: TimeShift::PriorPeriod, show: TimeShow::Difference, partial: TimePartial::FullPeriod, ..Default::default() }, "Average balance of Balance \u{b7} YTD \u{b7} vs Prior period \u{b7} full period"),
    ] {
        let s = spec(&[&month_year()], &[], vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: Some(time) }], &["Month-Year"]);
        let b = run(&dd, &df, sc.clone(), &s);
        let chart = chart_of(&dd, &df, sc.clone(), time, None);
        assert_eq!(names(&b)[1], heading);
        assert_eq!(chart.measure_names, [heading.to_string()], "the chart names the value as the pivot does");
        let pivot_labels: Vec<String> = labels(&b).into_iter().filter(|l| l != "Grand total").collect();
        assert_eq!(chart.categories, pivot_labels, "the same months in the same order");
        let pivot = floats(&b.df, heading);
        let shown = &chart.values[0];
        assert_eq!(shown.len(), pivot_labels.len());
        for (i, label) in pivot_labels.iter().enumerate() {
            match (shown[i], pivot[i]) {
                (Some(c), Some(p)) => assert_eq!(c, p, "{heading}: {label}: the chart's mark is the pivot's cell"),
                (None, None) => {}
                other => panic!("{heading}: {label}: {other:?}"),
            }
        }
        assert!(shown.iter().flatten().count() >= 10, "{heading}: the months with data have a number");
    }
}

#[test]
fn m4_a_chart_with_a_series_gives_one_line_per_key_and_a_chart_that_mixes_time_and_plain_values_is_refused() {
    let dd = dir("p9m4_chart_series");
    let mut df = daily("2025-01-01", "2025-03-31");
    let n = df.height();
    df.with_column(Series::new("Branch".into(), (0..n).map(|i| if i % 2 == 0 { "B1" } else { "B2" }).collect::<Vec<_>>())).expect("branches");
    let sc = scope(&dd, settings(1), "2025-03-31", vec![]);
    let time = TimeValue::of(TimeAgg::ClosingBalance);
    let chart = chart_of(&dd, &df, sc.clone(), time, Some("Branch"));
    assert_eq!(chart.series, ["B1", "B2"]);
    assert_eq!(chart.categories, ["Jan 2025", "Feb 2025", "Mar 2025"]);
    // 31 Mar 2025 (ordinal 90) is a B2 row, 30 Mar a B1 row: the closing is the value of the last date, 0 when the branch has no row on it
    assert_eq!((chart.values[0][2], chart.values[1][2]), (Some(0.0), Some(90.0)));
    // a plain measure beside a time one is refused in words
    use micro_parquet::charts::spec::{ChartAgg, ChartSpec, MeasureSpec};
    let f = cal_field("Month-Year");
    let src = crate::common::parquet(&dd, "accounts", &df);
    let spec = ChartSpec {
        category: f.name.clone(),
        category_model: Some(f),
        data_tab: "t1".into(),
        measures: vec![MeasureSpec { column: Some("Balance".into()), time: Some(time), ..Default::default() }, MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, ..Default::default() }],
        ..Default::default()
    };
    let e = micro_parquet::model::with_scope(sc, || micro_parquet::charts::calc::compute(micro_parquet::engine::compile_tab_lazy(&src, &[], &dd).expect("source"), &spec)).err().map(|e| format!("{e:#}")).expect("a refusal");
    assert!(e.contains("can't share a chart with an ordinary aggregation"), "{e}");
}

#[test]
fn m4_a_plain_value_with_no_calendar_field_says_which_span_it_covers_and_one_with_a_period_field_does_not() {
    let dd = dir("p9m4_span_note");
    let df = daily("2025-01-01", "2026-11-24");
    let sc = scope(&dd, settings(1), "2026-11-24", vec![]);
    let plain = PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: Some(TimeValue::of(TimeAgg::ClosingBalance)) };
    let b = run(&dd, &df, sc.clone(), &spec(&["Branch"], &[], vec![plain.clone()], &[]));
    assert_eq!(b.warnings, ["Closing balance of Balance: whole data, 1 Jan 2025 \u{2013} 24 Nov 2026 (as of 24 Nov 2026)"]);
    // another as-of date moves the end of the span
    let sc2 = scope(&dd, settings(1), "2026-10-31", vec![]);
    let b = run(&dd, &df, sc2, &spec(&["Branch"], &[], vec![plain.clone()], &[]));
    assert_eq!(b.warnings, ["Closing balance of Balance: whole data, 1 Jan 2025 \u{2013} 31 Oct 2026 (as of 31 Oct 2026)"]);
    // a value with an M4 choice (MTD) covers its own window, and a pivot with a Calendar period field covers each period: no note
    let mtd = tv("Balance", TimeAgg::ClosingBalance, TimeWindow::Mtd, TimeShift::None, TimeShow::Value, TimePartial::SameDays);
    assert!(run(&dd, &df, sc.clone(), &spec(&["Branch"], &[], vec![mtd], &[])).warnings.is_empty());
    let b = run(&dd, &df, sc.clone(), &spec(&[&month_year()], &[], vec![plain.clone()], &["Month-Year"]));
    assert!(b.warnings.is_empty(), "{:?}", b.warnings);
    // the chart's notes carry it too
    use micro_parquet::charts::spec::{ChartSpec, MeasureSpec};
    let src = crate::common::parquet(&dd, "accounts", &df);
    let cs = ChartSpec { category: "Branch".into(), data_tab: "t1".into(), measures: vec![MeasureSpec { column: Some("Balance".into()), time: Some(TimeValue::of(TimeAgg::ClosingBalance)), ..Default::default() }], ..Default::default() };
    let data = micro_parquet::model::with_scope(sc, || micro_parquet::charts::calc::compute(micro_parquet::engine::compile_tab_lazy(&src, &[], &dd).expect("source"), &cs)).unwrap_or_else(|e| panic!("the chart: {e:#}"));
    assert!(data.notes.iter().any(|n| n == "Closing balance of Balance: whole data, 1 Jan 2025 \u{2013} 24 Nov 2026 (as of 24 Nov 2026)"), "{:?}", data.notes);
}
