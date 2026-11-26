//! P1: budget and target tables. A plan table (month, keys, amount columns) is linked to a data tab; a plan measure shows the plan, the
//! actual less the plan, or the actual as a percent of the plan in any pivot of the tab (so also in charts, KPI cards and dashboards, which
//! read the same pivot). Every number is worked by hand. A plan that is wrong (a repeated month and key, a missing month, a split it
//! can't make) is a refusal or a blank with its reason, never a quiet number.

use crate::common::{dir, parquet};
use crate::p9_m3::*;
use crate::p9_m5::{plain_measure, scope5};
use micro_parquet::measures::{check_measure, removal_refusal, NamedMeasure, PlanAgg, PlanShow, Sign};
use micro_parquet::model::*;
use micro_parquet::models::*;
use polars::prelude::*;
use std::sync::Arc;

fn strs_of(name: &str, v: &[&str]) -> Series {
    Series::new(name.into(), v.to_vec())
}

/// Month-end rows of two products: closing balance and the income of the month.
fn accounts() -> DataFrame {
    DataFrame::new(vec![
        dates("Date", &["2026-01-31", "2026-01-31", "2026-02-28", "2026-02-28"]).into(),
        strs_of("Product", &["A", "B", "A", "B"]).into(),
        nums("Balance", &[100.0, 50.0, 120.0, 40.0]),
        nums("Income", &[10.0, 5.0, 12.0, 4.0]),
    ])
    .expect("accounts")
}

/// The plan: any day of the month will do.
fn plan_df(rows: &[(&str, &str, f64, f64)]) -> DataFrame {
    DataFrame::new(vec![
        dates("Month", &rows.iter().map(|r| r.0).collect::<Vec<_>>()).into(),
        strs_of("Product", &rows.iter().map(|r| r.1).collect::<Vec<_>>()).into(),
        nums("Budget", &rows.iter().map(|r| r.2).collect::<Vec<_>>()),
        nums("Income budget", &rows.iter().map(|r| r.3).collect::<Vec<_>>()),
    ])
    .expect("plan")
}

fn good_plan() -> DataFrame {
    plan_df(&[("2026-01-15", "A", 90.0, 8.0), ("2026-01-15", "B", 60.0, 6.0), ("2026-02-01", "A", 100.0, 9.0), ("2026-02-01", "B", 50.0, 5.0)])
}

fn link() -> PlanLink {
    PlanLink { data_tab: "t1".into(), plan_tab: "t4".into(), month_column: "Month".into(), keys: vec![KeyPair { data: "Product".into(), lookup: "Product".into() }] }
}

fn plan_scope(dd: &std::path::Path, plan: &DataFrame, measures: Vec<NamedMeasure>) -> Option<Arc<ModelContext>> {
    let base = scope5(dd, "2026-02-28", None, measures)?;
    let mut c = (*base).clone();
    let table = ModelTab { key: "t4".into(), title: "Budget".into(), source: parquet(dd, "plan", plan), steps: vec![] };
    let refusal = check_plan(&link(), &table, dd).expect("plan check").refusal("Budget");
    c.plans = vec![ScopePlan { link: link(), table: Some(table), refusal }];
    Some(Arc::new(c))
}

fn actual(name: &str, column: &str, agg: TimeAgg) -> NamedMeasure {
    NamedMeasure::Base { name: name.into(), tab: "t1".into(), column: column.into(), aggregation: agg, filters: vec![], sign: Sign::Plus, weight: None }
}

fn plan(name: &str, column: &str, agg: PlanAgg, show: PlanShow, actual: &str) -> NamedMeasure {
    NamedMeasure::Plan { name: name.into(), tab: "t1".into(), plan_tab: "t4".into(), column: column.into(), agg, show, actual: actual.into() }
}

fn measures() -> Vec<NamedMeasure> {
    vec![
        actual("Deposits", "Balance", TimeAgg::ClosingBalance),
        actual("Income", "Income", TimeAgg::TotalFlow),
        plan("Budget", "Budget", PlanAgg::Closing, PlanShow::Plan, ""),
        plan("Vs budget", "Budget", PlanAgg::Closing, PlanShow::Variance, "Deposits"),
        plan("Achieved", "Budget", PlanAgg::Closing, PlanShow::PercentAchieved, "Deposits"),
        plan("Income vs budget", "Income budget", PlanAgg::Sum, PlanShow::Variance, "Income"),
        plan("Income achieved", "Income budget", PlanAgg::Sum, PlanShow::PercentAchieved, "Income"),
    ]
}

fn round2(v: Option<f64>) -> f64 {
    (v.expect("a number") * 100.0).round() / 100.0
}

#[test]
fn p1_a_the_plan_by_month_is_set_against_the_actual_by_month() {
    let dd = dir("p11p1_month");
    let sc = plan_scope(&dd, &good_plan(), measures());
    let s = spec(&[&month_year()], &[], vec![plain_measure("Deposits"), plain_measure("Budget"), plain_measure("Vs budget"), plain_measure("Achieved")], &["Month-Year"]);
    let b = run(&dd, &accounts(), sc, &s);
    // Jan: 150 against 90 + 60; Feb: 160 against 100 + 50
    assert_eq!(at(&b, "Jan 2026", "Deposits"), Some(150.0));
    assert_eq!(at(&b, "Jan 2026", "Budget"), Some(150.0));
    assert_eq!(at(&b, "Jan 2026", "Vs budget"), Some(0.0));
    assert_eq!(round2(at(&b, "Jan 2026", "Achieved")), 100.0);
    assert_eq!(at(&b, "Feb 2026", "Budget"), Some(150.0));
    assert_eq!(at(&b, "Feb 2026", "Vs budget"), Some(10.0));
    assert_eq!(round2(at(&b, "Feb 2026", "Achieved")), 106.67);
}

#[test]
fn p1_b_a_pivot_split_by_the_plans_key_gets_the_plan_of_each_line() {
    let dd = dir("p11p1_key");
    let sc = plan_scope(&dd, &good_plan(), measures());
    let s = spec(&["Product"], &[&month_year()], vec![plain_measure("Vs budget")], &["Month-Year"]);
    let b = run(&dd, &accounts(), sc, &s);
    let heads = names(&b);
    let jan = heads.iter().find(|n| n.contains("Jan 2026")).cloned().unwrap_or_else(|| panic!("no Jan column in {heads:?}"));
    let feb = heads.iter().find(|n| n.contains("Feb 2026")).cloned().unwrap_or_else(|| panic!("no Feb column in {heads:?}"));
    // A: 100 against 90, 120 against 100; B: 50 against 60, 40 against 50
    assert_eq!(at(&b, "A", &jan), Some(10.0));
    assert_eq!(at(&b, "A", &feb), Some(20.0));
    assert_eq!(at(&b, "B", &jan), Some(-10.0));
    assert_eq!(at(&b, "B", &feb), Some(-10.0));
}

#[test]
fn p1_c_a_flow_budget_adds_the_months_of_the_year_so_far() {
    let dd = dir("p11p1_year");
    let sc = plan_scope(&dd, &good_plan(), measures());
    let year = cal_field("Year").name;
    let s = spec(&["Product"], &[&year], vec![plain_measure("Income vs budget")], &["Year"]);
    let b = run(&dd, &accounts(), sc, &s);
    let heads = names(&b);
    let y = heads.iter().find(|n| n.contains("2026")).cloned().unwrap_or_else(|| panic!("no 2026 column in {heads:?}"));
    // A: income 10 + 12 = 22 against 8 + 9 = 17; B: 5 + 4 = 9 against 6 + 5 = 11
    assert_eq!(at(&b, "A", &y), Some(5.0));
    assert_eq!(at(&b, "B", &y), Some(-2.0));
}

#[test]
fn p1_d_a_month_the_plan_has_no_row_for_is_blank_with_its_reason_never_zero() {
    let dd = dir("p11p1_missing");
    // B has no February row
    let plan = plan_df(&[("2026-01-15", "A", 90.0, 8.0), ("2026-01-15", "B", 60.0, 6.0), ("2026-02-01", "A", 100.0, 9.0)]);
    let sc = plan_scope(&dd, &plan, measures());
    let s = spec(&["Product"], &[&month_year()], vec![plain_measure("Vs budget")], &["Month-Year"]);
    let b = run(&dd, &accounts(), sc, &s);
    let heads = names(&b);
    let feb = heads.iter().find(|n| n.contains("Feb 2026")).cloned().expect("a Feb column");
    assert_eq!(at(&b, "B", &feb), None, "no plan, no number");
    assert_eq!(at(&b, "A", &feb), Some(20.0));
    assert!(b.notes.iter().any(|n| n.text.contains("The plan has no row for B in Feb 2026")), "the reason is said: {:?}", b.notes.iter().map(|n| n.text.clone()).collect::<Vec<_>>());
}

#[test]
fn p1_e_a_repeated_month_and_key_is_refused_not_added_up() {
    let dd = dir("p11p1_dup");
    let plan = plan_df(&[("2026-01-15", "A", 90.0, 8.0), ("2026-01-20", "A", 91.0, 8.0), ("2026-01-15", "B", 60.0, 6.0)]);
    let table = ModelTab { key: "t4".into(), title: "Budget".into(), source: parquet(&dd, "plan_dup", &plan), steps: vec![] };
    let c = check_plan(&link(), &table, &dd).expect("check");
    assert_eq!(c.duplicate_keys, 1);
    let why = c.refusal("Budget").expect("a refusal");
    assert!(why.contains("more than once") && why.contains("2026-01 A"), "{why}");
    // the pivot says the same instead of using the plan
    let sc = plan_scope(&dd, &plan, measures());
    let s = spec(&[&month_year()], &[], vec![plain_measure("Budget")], &["Month-Year"]);
    let e = run_err(&dd, &accounts(), sc, &s);
    assert!(e.contains("more than once"), "{e}");
}

#[test]
fn p1_f_a_pivot_split_by_a_column_the_plan_has_no_key_for_is_refused() {
    let dd = dir("p11p1_split");
    let df = DataFrame::new(vec![
        dates("Date", &["2026-01-31", "2026-01-31"]).into(),
        strs_of("Product", &["A", "A"]).into(),
        strs_of("Branch", &["X", "Y"]).into(),
        nums("Balance", &[60.0, 40.0]),
        nums("Income", &[1.0, 1.0]),
    ])
    .expect("accounts");
    let sc = plan_scope(&dd, &good_plan(), measures());
    let s = spec(&["Branch"], &[&month_year()], vec![plain_measure("Budget")], &["Month-Year"]);
    let e = run_err(&dd, &df, sc.clone(), &s);
    assert!(e.contains("set by Product only") && e.contains("Branch"), "{e}");
    // and a filter the plan can't take is refused too
    let mut s2 = spec(&["Product"], &[&month_year()], vec![plain_measure("Budget")], &["Month-Year"]);
    s2.filters = vec![PivotFilter { field: "Branch".into(), values: vec![Some("X".into())] }];
    let e2 = run_err(&dd, &df, sc, &s2);
    assert!(e2.contains("can't be filtered by 'Branch'"), "{e2}");
}

#[test]
fn p1_g_a_filter_on_the_plans_key_applies_to_the_plan_too() {
    let dd = dir("p11p1_filter");
    let sc = plan_scope(&dd, &good_plan(), measures());
    let mut s = spec(&[&month_year()], &[], vec![plain_measure("Deposits"), plain_measure("Budget")], &["Month-Year"]);
    s.filters = vec![PivotFilter { field: "Product".into(), values: vec![Some("A".into())] }];
    let b = run(&dd, &accounts(), sc, &s);
    assert_eq!(at(&b, "Jan 2026", "Deposits"), Some(100.0));
    assert_eq!(at(&b, "Jan 2026", "Budget"), Some(90.0), "only A's plan");
    assert_eq!(at(&b, "Feb 2026", "Budget"), Some(100.0));
}

#[test]
fn p1_h_a_plan_of_zero_gives_no_percent_and_the_measures_are_checked_when_saved() {
    let dd = dir("p11p1_zero");
    let zero = plan_df(&[("2026-01-15", "A", 0.0, 0.0), ("2026-01-15", "B", 0.0, 0.0), ("2026-02-01", "A", 100.0, 9.0), ("2026-02-01", "B", 50.0, 5.0)]);
    let sc = plan_scope(&dd, &zero, measures());
    let s = spec(&[&month_year()], &[], vec![plain_measure("Achieved")], &["Month-Year"]);
    let b = run(&dd, &accounts(), sc, &s);
    assert_eq!(at(&b, "Jan 2026", "Achieved"), None);
    assert!(b.notes.iter().any(|n| n.text.contains("is 0") && n.text.contains("% achieved")));
    // saving: a plan set against something needs an actual; a ratio can't be made of a plan measure; a used actual can't be removed
    let all = measures();
    let bad = plan("X", "Budget", PlanAgg::Sum, PlanShow::Variance, "");
    assert!(check_measure(&all, &bad).expect("refused").contains("the actual"));
    let on_plan = plan("Y", "Budget", PlanAgg::Sum, PlanShow::Variance, "Budget");
    assert!(check_measure(&all, &on_plan).expect("refused").contains("plan measure"));
    let why = removal_refusal(&all, "Deposits", &[]).expect("used");
    assert!(why.contains("plan measure 'Vs budget'"), "{why}");
}

#[test]
fn p1_i_a_plan_link_is_part_of_the_model_file_and_old_files_load() {
    let m = Model { plans: vec![link()], ..Default::default() };
    let text = serde_json::to_string(&m).expect("json");
    let back: Model = serde_json::from_str(&text).expect("model");
    assert_eq!(back.plans, vec![link()]);
    let old: Model = serde_json::from_str("{}").expect("an old model");
    assert!(old.plans.is_empty());
}

#[test]
fn p1_j_a_flow_budget_with_a_month_missing_is_blank_not_a_smaller_sum() {
    let dd = dir("p11p1_missing_sum");
    // B has no February row: its year-to-date budget would be January's alone
    let gap = plan_df(&[("2026-01-15", "A", 90.0, 8.0), ("2026-01-15", "B", 60.0, 6.0), ("2026-02-01", "A", 100.0, 9.0)]);
    let sc = plan_scope(&dd, &gap, measures());
    let year = cal_field("Year").name;
    let s = spec(&["Product"], &[&year], vec![plain_measure("Income vs budget")], &["Year"]);
    let b = run(&dd, &accounts(), sc, &s);
    let heads = names(&b);
    let y = heads.iter().find(|n| n.contains("2026")).cloned().expect("a 2026 column");
    assert_eq!(at(&b, "A", &y), Some(5.0));
    assert_eq!(at(&b, "B", &y), None);
    assert!(b.notes.iter().any(|n| n.text.contains("The plan has no row for B in Feb 2026")));
}
