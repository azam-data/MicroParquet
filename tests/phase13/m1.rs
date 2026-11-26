//! Phase 13 M1: top movers. Worked by hand from a small long file of month-end snapshots (with one mid-month date that must not count).

use crate::common::{dir, parquet};
use crate::p9_m3::{dates, nums, strs};
use micro_parquet::movers::*;
use polars::prelude::*;

fn data() -> DataFrame {
    // Jan 31, Feb 15 (ignored: not the last date of Feb), Feb 28, Mar 31
    let rows: Vec<(&str, &str, f64)> = vec![
        ("2026-01-31", "A", 100.0),
        ("2026-01-31", "B", 50.0),
        ("2026-01-31", "C", 10.0),
        ("2026-02-15", "A", 999.0),
        ("2026-02-28", "A", 130.0),
        ("2026-02-28", "B", 20.0),
        ("2026-02-28", "C", 10.0),
        ("2026-02-28", "D", 40.0),
        ("2026-03-31", "A", 130.0),
        ("2026-03-31", "C", 10.0),
        ("2026-03-31", "D", 10.0),
        ("2026-03-31", "E", 5.0),
    ];
    DataFrame::new(vec![
        dates("Date", &rows.iter().map(|r| r.0).collect::<Vec<_>>()).into(),
        strs("Account", &rows.iter().map(|r| r.1).collect::<Vec<_>>()).into(),
        nums("Balance", &rows.iter().map(|r| r.2).collect::<Vec<_>>()),
    ])
    .expect("data")
}

fn spec(top: usize, months: usize) -> MoversSpec {
    MoversSpec { date_col: "Date".into(), bal_col: "Balance".into(), key_cols: vec!["Account".into()], top, months, basis: Basis::PreviousMonth }
}

fn run(name: &str, df: &DataFrame, s: &MoversSpec) -> Result<(DataFrame, Vec<String>), String> {
    let dd = dir(name);
    let lf = micro_parquet::engine::compile_tab_lazy(&parquet(&dd, "t", df), &[], &dd).map_err(|e| format!("{e:#}"))?;
    build_movers(lf, s).map_err(|e| format!("{e:#}"))
}

fn col_str(df: &DataFrame, c: &str) -> Vec<String> {
    df.column(c).expect("col").str().expect("str").into_iter().map(|v| v.unwrap_or("").to_string()).collect()
}

fn col_f(df: &DataFrame, c: &str) -> Vec<Option<f64>> {
    df.column(c).expect("col").f64().expect("f64").into_iter().collect()
}

#[test]
fn m1_a_each_month_lists_its_biggest_changes_against_the_month_before_largest_first() {
    let (df, _) = run("p13m1_a", &data(), &spec(10, 12)).expect("movers");
    // Mar (latest first): D 40 -> 10 = -30, B 20 -> gone = -20 (Closed), E new +5; A and C did not move
    // Feb: D new +40, A +30, B -30 (the tie of A and B goes to A by name), C did not move
    assert_eq!(col_str(&df, COL_MONTH), ["Mar 2026", "Mar 2026", "Mar 2026", "Feb 2026", "Feb 2026", "Feb 2026"]);
    assert_eq!(col_str(&df, "Account"), ["D", "B", "E", "D", "A", "B"]);
    assert_eq!(col_f(&df, COL_CHANGE), [-30.0, -20.0, 5.0, 40.0, 30.0, -30.0].map(Some));
    assert_eq!(col_str(&df, COL_STATUS), [STATUS_DOWN, STATUS_CLOSED, STATUS_NEW, STATUS_NEW, STATUS_UP, STATUS_DOWN]);
    assert_eq!(col_f(&df, COL_PREV), [40.0, 20.0, 0.0, 0.0, 100.0, 50.0].map(Some));
    assert_eq!(col_f(&df, COL_CURR), [10.0, 0.0, 5.0, 40.0, 130.0, 20.0].map(Some));
    // the percent is blank for a New line (no earlier balance to divide by)
    assert_eq!(col_f(&df, COL_PCT), [Some(-75.0), Some(-100.0), None, None, Some(30.0), Some(-60.0)]);
    let ranks: Vec<u32> = df.column(COL_RANK).expect("rank").u32().expect("u32").into_iter().flatten().collect();
    assert_eq!(ranks, [1, 2, 3, 1, 2, 3]);
}

#[test]
fn m1_b_the_mid_month_date_is_not_a_snapshot_and_the_comparison_dates_are_shown() {
    let (df, _) = run("p13m1_b", &data(), &spec(10, 12)).expect("movers");
    // Feb 2026 is read on 28 Feb (A = 130), not on 15 Feb (A = 999)
    let at = col_f(&df, COL_CURR);
    assert!(at.contains(&Some(130.0)) && !at.contains(&Some(999.0)));
    let as_at = df.column(COL_AS_AT).expect("as at").cast(&DataType::String).expect("str");
    assert_eq!(as_at.str().expect("str").get(0), Some("2026-03-31"));
    let with = df.column(COL_PREV_AS_AT).expect("with").cast(&DataType::String).expect("str");
    assert_eq!(with.str().expect("str").get(0), Some("2026-02-28"));
}

#[test]
fn m1_c_top_cuts_each_month_and_months_keeps_the_latest() {
    let (df, _) = run("p13m1_c", &data(), &spec(2, 12)).expect("movers");
    assert_eq!(col_str(&df, "Account"), ["D", "B", "D", "A"], "two a month; the Feb tie goes to A");
    let (df, _) = run("p13m1_d", &data(), &spec(10, 1)).expect("movers");
    assert_eq!(df.height(), 3, "one month only: March");
    assert!(col_str(&df, COL_MONTH).iter().all(|m| m == "Mar 2026"));
}

#[test]
fn m1_d_a_sum_of_the_changes_is_the_change_of_the_total_when_every_line_is_listed() {
    let (df, _) = run("p13m1_e", &data(), &spec(100, 12)).expect("movers");
    // Feb: total 200 (A130+B20+C10+D40) against Jan 160 = +40; the listed changes add to the same
    let feb: f64 = col_str(&df, COL_MONTH).iter().zip(col_f(&df, COL_CHANGE)).filter(|(m, _)| *m == "Feb 2026").map(|(_, c)| c.unwrap_or(0.0)).sum();
    assert_eq!(feb, 40.0);
    let mar: f64 = col_str(&df, COL_MONTH).iter().zip(col_f(&df, COL_CHANGE)).filter(|(m, _)| *m == "Mar 2026").map(|(_, c)| c.unwrap_or(0.0)).sum();
    assert_eq!(mar, 155.0 - 200.0);
}

#[test]
fn m1_e_bad_input_is_refused_in_plain_words_not_defaulted() {
    let mut blank = data();
    let bal = Series::new("Balance".into(), [Some(100.0), None, Some(10.0), Some(999.0), Some(130.0), Some(20.0), Some(10.0), Some(40.0), Some(130.0), Some(10.0), Some(10.0), Some(5.0)]);
    blank.with_column(bal).expect("col");
    let e = run("p13m1_f", &blank, &spec(10, 12)).expect_err("blank balance");
    assert!(e.contains("blank or non-number balance"), "{e}");
    let one_month = data().head(Some(3));
    let e = run("p13m1_g", &one_month, &spec(10, 12)).expect_err("one month");
    assert!(e.contains("Two months with data are needed"), "{e}");
    let mut s = spec(10, 12);
    s.date_col = "Account".into();
    s.key_cols = vec!["Date".into()];
    let e = run("p13m1_h", &data(), &s).expect_err("not a date");
    assert!(e.contains("not a Date"), "{e}");
    let mut s = spec(0, 12);
    s.top = 0;
    assert!(s.check().expect_err("top").to_string().contains("between 1 and"));
    let mut s = spec(10, 12);
    s.key_cols = vec![];
    assert!(s.check().expect_err("key").to_string().contains("one or two columns"));
}

#[test]
fn m1_f_the_setup_is_guessed_from_the_columns() {
    let cols = vec![("Branch".to_string(), DataType::String), ("Account no".to_string(), DataType::String), ("Value date".to_string(), DataType::Date), ("Rate".to_string(), DataType::Float64), ("Outstanding balance".to_string(), DataType::Float64)];
    let g = guess(&cols);
    assert_eq!((g.date_col.as_str(), g.bal_col.as_str(), g.key_cols.as_slice()), ("Value date", "Outstanding balance", &["Account no".to_string()][..]));
    // nothing to guess from: left empty, never made up
    let g = guess(&[("Notes".to_string(), DataType::String)]);
    assert!(g.date_col.is_empty() && g.bal_col.is_empty());
}

fn two_years() -> DataFrame {
    let rows: Vec<(&str, &str, f64)> = vec![
        ("2025-02-28", "A", 80.0),
        ("2025-02-28", "B", 60.0),
        ("2025-12-31", "A", 100.0),
        ("2025-12-31", "B", 50.0),
        ("2026-01-31", "A", 110.0),
        ("2026-01-31", "B", 50.0),
        ("2026-02-28", "A", 130.0),
        ("2026-02-28", "B", 20.0),
        ("2026-02-28", "C", 5.0),
    ];
    DataFrame::new(vec![
        dates("Date", &rows.iter().map(|r| r.0).collect::<Vec<_>>()).into(),
        strs("Account", &rows.iter().map(|r| r.1).collect::<Vec<_>>()).into(),
        nums("Balance", &rows.iter().map(|r| r.2).collect::<Vec<_>>()),
    ])
    .expect("data")
}

#[test]
fn m1_g_start_of_year_is_the_default_and_compares_with_the_year_end_before() {
    assert_eq!(MoversSpec::default().basis, Basis::StartOfYear);
    let mut s = spec(10, 0);
    s.basis = Basis::StartOfYear;
    let (df, notes) = run("p13m1_g2", &two_years(), &s).expect("movers");
    // Feb 2026 and Jan 2026 against 31 Dec 2025; Dec 2025 has no earlier year, so it uses the first month of its own year (Feb 2025)
    assert_eq!(col_str(&df, COL_MONTH), ["Feb 2026", "Feb 2026", "Feb 2026", "Jan 2026", "Dec 2025", "Dec 2025"]);
    assert_eq!(col_str(&df, "Account"), ["A", "B", "C", "A", "A", "B"]);
    assert_eq!(col_f(&df, COL_CHANGE), [30.0, -30.0, 5.0, 10.0, 20.0, -10.0].map(Some));
    assert!(notes.iter().any(|n| n.contains("Feb 2025") && n.contains("Not listed")), "{notes:?}");
}

#[test]
fn m1_h_same_month_last_year_skips_a_month_with_no_partner_and_says_so() {
    let mut s = spec(10, 0);
    s.basis = Basis::SameMonthLastYear;
    let (df, notes) = run("p13m1_h2", &two_years(), &s).expect("movers");
    assert_eq!(col_str(&df, COL_MONTH), ["Feb 2026", "Feb 2026", "Feb 2026"]);
    assert_eq!(col_f(&df, COL_CHANGE), [50.0, -40.0, 5.0].map(Some));
    assert!(notes.iter().any(|n| n.contains("Not listed") && n.contains("Jan 2026") && n.contains("same month")), "{notes:?}");
}
