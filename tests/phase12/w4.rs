//! Phase 12 W4: what-if scenarios. A rate shift and a growth % change the measures marked for them, from the as-of month on, in pivots,
//! KPI cards and forecasts, and every changed number is labelled. Worked by hand.

use crate::common::{dir, parquet};
use crate::p9_m3::*;
use crate::p9_m5::{measure_value, scope5};
use micro_parquet::kpi::*;
use micro_parquet::measures::{NamedMeasure, RatioFormat, Sign, Term};
use micro_parquet::model::*;
use micro_parquet::models::*;
use micro_parquet::whatif::*;
use polars::prelude::*;
use std::sync::Arc;

fn s_of(name: &str, v: &[&str]) -> Series {
    Series::new(name.into(), v.to_vec())
}

fn accounts() -> DataFrame {
    DataFrame::new(vec![
        dates("Date", &["2026-01-31", "2026-01-31", "2026-02-28", "2026-02-28"]).into(),
        s_of("Product", &["A", "B", "A", "B"]).into(),
        nums("Balance", &[100.0, 50.0, 120.0, 40.0]),
        nums("Income", &[10.0, 5.0, 12.0, 4.0]),
    ])
    .expect("accounts")
}

fn base(name: &str, column: &str, agg: TimeAgg) -> NamedMeasure {
    NamedMeasure::Base { name: name.into(), tab: "t1".into(), column: column.into(), aggregation: agg, filters: vec![], sign: Sign::Plus, weight: None }
}

fn measures() -> Vec<NamedMeasure> {
    vec![
        base("Deposits", "Balance", TimeAgg::ClosingBalance),
        base("Income", "Income", TimeAgg::TotalFlow),
        NamedMeasure::Ratio {
            name: "Yield".into(),
            tab: "t1".into(),
            numerator: vec![Term { measure: "Income".into(), sign: Sign::Plus }],
            denominator: vec![Term { measure: "Deposits".into(), sign: Sign::Plus }],
            annualise: None,
            format: RatioFormat::Percent,
        },
    ]
}

fn stressed() -> Scenario {
    Scenario { name: "Stressed".into(), rate_shift_bp: 200.0, growth_pct: -10.0 }
}

fn what_if(marks: &[(&str, Role)]) -> WhatIf {
    WhatIf { scenarios: vec![stressed()], active: "Stressed".into(), marked: marks.iter().map(|(m, r)| Marked { measure: (*m).into(), role: *r }).collect() }
}

fn scope_with(dd: &std::path::Path, w: &WhatIf) -> Option<Arc<ModelContext>> {
    scope5(dd, "2026-02-28", None, measures()).map(|b| {
        let mut c = (*b).clone();
        c.what_if = w.scope();
        Arc::new(c)
    })
}

fn pivot(dd: &std::path::Path, sc: Option<Arc<ModelContext>>) -> micro_parquet::pivot_table::PivotBuild {
    let s = spec(&[&month_year()], &[], vec![measure_value("Deposits", TimeValue::default()), measure_value("Income", TimeValue::default()), measure_value("Yield", TimeValue::default())], &["Month-Year"]);
    run(dd, &accounts(), sc, &s)
}

#[test]
fn w4_a_growth_scales_a_volume_and_the_rate_shift_adds_to_a_rate_from_the_as_of_month_on() {
    let dd = dir("p12w4_a");
    let w = what_if(&[("Deposits", Role::Volume), ("Yield", Role::Rate)]);
    let b = pivot(&dd, scope_with(&dd, &w));
    let h = names(&b);
    // January is closed history: the actual number. February is the as-of month: 160 less 10%, and a yield of 16 / 160 = 10% plus 2 points
    assert!(close(at(&b, "Jan 2026", &h[1]), 150.0), "{:?}", at(&b, "Jan 2026", &h[1]));
    assert!(close(at(&b, "Feb 2026", &h[1]), 144.0));
    assert!(close(at(&b, "Jan 2026", &h[3]), 10.0));
    assert!(close(at(&b, "Feb 2026", &h[3]), 12.0));
    // an unmarked measure is never touched
    assert!(close(at(&b, "Feb 2026", &h[2]), 16.0) && close(at(&b, "Jan 2026", &h[2]), 15.0));
    // the output says so
    assert!(b.warnings.iter().any(|n| n.starts_with("Scenario Stressed: rate +200 bp, growth \u{2212}10% on Deposits, Yield") && n.ends_with("These are not actual figures.")), "{:?}", b.warnings);
}

#[test]
fn w4_b_no_scenario_or_the_base_changes_nothing_and_says_nothing() {
    let dd = dir("p12w4_b");
    let plain = pivot(&dd, scope_with(&dd, &WhatIf::default()));
    let h = names(&plain);
    assert!(close(at(&plain, "Feb 2026", &h[1]), 160.0) && close(at(&plain, "Feb 2026", &h[3]), 10.0));
    assert!(!plain.warnings.iter().any(|n| n.contains("Scenario")));
    let mut base_only = what_if(&[("Deposits", Role::Volume)]);
    base_only.scenarios = vec![Scenario { name: "Base".into(), rate_shift_bp: 0.0, growth_pct: 0.0 }];
    base_only.active = "Base".into();
    assert!(base_only.scope().is_none(), "the base has nothing to apply");
}

#[test]
fn w4_c_a_mark_that_does_not_fit_the_measure_is_refused_in_plain_words() {
    let dd = dir("p12w4_c");
    let s = spec(&[&month_year()], &[], vec![measure_value("Yield", TimeValue::default())], &["Month-Year"]);
    let e = run_err(&dd, &accounts(), scope_with(&dd, &what_if(&[("Yield", Role::Volume)])), &s);
    assert!(e.contains("scenario 'Stressed'") && e.contains("ratio of two amounts"), "{e}");
    let s = spec(&[&month_year()], &[], vec![measure_value("Deposits", TimeValue::default())], &["Month-Year"]);
    let e = run_err(&dd, &accounts(), scope_with(&dd, &what_if(&[("Deposits", Role::Rate)])), &s);
    assert!(e.contains("is an amount, not a rate"), "{e}");
    let plan = NamedMeasure::Plan { name: "Budget".into(), tab: "t1".into(), plan_tab: "t4".into(), column: "Budget".into(), agg: micro_parquet::measures::PlanAgg::Closing, show: micro_parquet::measures::PlanShow::Plan, actual: String::new() };
    assert!(check_role(&plan, Role::Volume).expect_err("plan").contains("A plan is the plan"));
    let pct = NamedMeasure::Ratio { name: "Times".into(), tab: "t1".into(), numerator: vec![], denominator: vec![], annualise: None, format: RatioFormat::Number };
    assert!(check_role(&pct, Role::Rate).expect_err("not percent").contains("not shown in percent"));
}

#[test]
fn w4_d_a_kpi_card_shows_the_scenario_number_and_says_the_scenario_beside_its_period() {
    let dd = dir("p12w4_d");
    let w = what_if(&[("Deposits", Role::Volume)]);
    let sc = scope_with(&dd, &w);
    let spec = KpiSpec { source: KpiSource::Measure("Deposits".into()), sparkline: true, ..Default::default() };
    let req = KpiRequest { spec, tab_key: "t1".into(), base: parquet(&dd, "accounts", &accounts()), steps: vec![], picks: vec![], temp_dir: dd.clone() };
    let v = with_scope(sc, || compute_kpi(&req)).expect("card");
    assert_eq!(v.current, Some(144.0));
    assert_eq!(v.period, "Feb 2026 \u{b7} Scenario Stressed (rate +200 bp, growth \u{2212}10%)");
    assert_eq!(v.spark[10], Some(150.0), "January stays actual in the sparkline");
    assert_eq!(v.spark[11], Some(144.0));
    // without the scenario: 160 and a plain period
    let v = with_scope(scope_with(&dd, &WhatIf::default()), || compute_kpi(&req)).expect("card");
    assert_eq!((v.current, v.period.as_str()), (Some(160.0), "Feb 2026"));
}

#[test]
fn w4_e_a_forecast_is_fitted_to_actuals_then_the_as_of_month_and_the_forecast_months_take_the_scenario() {
    use micro_parquet::charts::data::ChartData;
    use micro_parquet::charts::spec::{ChartKind, ChartSpec};
    use micro_parquet::forecast::Monthly;
    use micro_parquet::forecast_chart::*;
    let y: Vec<f64> = (0..30).map(|t| 1000.0 + 10.0 * t as f64 + ((t * 7 % 5) as f64)).collect();
    let m = Monthly::new(2024, 1, y.clone());
    let cats: Vec<String> = (0..30).map(|i| m.label(i)).collect();
    let data = ChartData { spec: ChartSpec { kind: ChartKind::Line, ..Default::default() }, categories: cats.clone(), series: vec!["Deposits".into()], values: vec![y.iter().map(|v| Some(*v)).collect()], rows: vec![vec![1; 30]], kept_categories: cats, kept_series: vec!["Deposits".into()], ..Default::default() };
    let with = apply_forecast(data, &ForecastSpec { horizon: 3, model: ModelPick::Growth, ..Default::default() }).expect("forecast");
    let raw = with.clone();
    // as of Jun 2026 (the last actual month, index 29): that month and the 3 ahead change by -10%; May 2026 does not
    let w = what_if(&[("Deposits", Role::Volume)]).scope().expect("scope");
    let out = apply_scenario(with, &w, Role::Volume, "Deposits", d("2026-06-30")).expect("scenario");
    assert_eq!(out.values[0][28], raw.values[0][28], "May 2026 is actual");
    assert!((out.values[0][29].unwrap() - raw.values[0][29].unwrap() * 0.9).abs() < 1e-9, "the as-of month");
    for si in 1..out.values.len() {
        for ci in 30..33 {
            let (a, b) = (out.values[si][ci].expect("forecast"), raw.values[si][ci].expect("forecast"));
            assert!((a - b * 0.9).abs() < 1e-9, "series {si} month {ci}");
        }
    }
    assert!(out.notes.iter().any(|n| n.starts_with("Scenario Stressed:")), "{:?}", out.notes);
}

#[test]
fn w4_f_scenarios_are_checked_old_projects_load_and_the_ireb_style_shock_comes_from_the_rule_file() {
    let mut w = what_if(&[]);
    assert!(w.check().is_ok());
    w.scenarios.push(Scenario { name: "stressed".into(), ..Default::default() });
    assert!(w.check().expect_err("repeat").contains("Two scenarios are called"));
    w.scenarios.pop();
    w.scenarios.push(Scenario { name: "Wipe".into(), growth_pct: -100.0, ..Default::default() });
    assert!(w.check().expect_err("wipe").contains("wipe the balances out"));
    w.scenarios.pop();
    w.active = "Gone".into();
    assert!(w.check().expect_err("missing").contains("is not in the list"));
    // an old project has no what_if and a model with none writes none
    let m: Model = serde_json::from_str("{}").expect("old model");
    assert!(m.what_if.is_empty() && !serde_json::to_string(&m).expect("json").contains("what_if"));
    let mut m2 = Model::default();
    m2.what_if = what_if(&[("Deposits", Role::Volume)]);
    let back: Model = serde_json::from_str(&serde_json::to_string(&m2).expect("json")).expect("round trip");
    assert_eq!(back, m2);
    // the IRRBB parallel shock is the size in the rule file in force at the date: positive and finite (never typed in here)
    let bp = parallel_shock_bp(d("2026-02-28"), "BDT").expect("the shipped IRRBB shock file lists BDT");
    assert!(bp.is_finite() && bp > 0.0, "{bp}");
    let e = parallel_shock_bp(d("2026-02-28"), "XXX").expect_err("unknown currency").to_string();
    assert!(e.contains("XXX"), "{e}");
    // renames and removals follow the measure
    let mut x = what_if(&[("Deposits", Role::Volume)]);
    x.rename_measure("Deposits", "Total deposits");
    assert_eq!(x.role_of("Total deposits"), Some(Role::Volume));
    x.forget_measure("Total deposits");
    assert!(x.marked.is_empty());
    assert_eq!(stressed().words(), "rate +200 bp, growth \u{2212}10%");
}
