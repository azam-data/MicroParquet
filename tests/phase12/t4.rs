//! Phase 12 T4: a KPI card worked out against the model: a target taken from a Phase 11 plan measure, and a rolling window on a card.

use crate::common::{dir, parquet};
use crate::p9_m3::*;
use crate::p9_m5::scope5;
use micro_parquet::kpi::*;
use micro_parquet::measures::{NamedMeasure, PlanAgg, PlanShow, Sign};
use micro_parquet::model::*;
use micro_parquet::models::*;
use micro_parquet::target::Bands;
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

fn plan_df() -> DataFrame {
    let rows = [("2026-01-15", "A", 90.0, 8.0), ("2026-01-15", "B", 60.0, 6.0), ("2026-02-01", "A", 100.0, 9.0), ("2026-02-01", "B", 50.0, 5.0)];
    DataFrame::new(vec![
        dates("Month", &rows.iter().map(|r| r.0).collect::<Vec<_>>()).into(),
        s_of("Product", &rows.iter().map(|r| r.1).collect::<Vec<_>>()).into(),
        nums("Budget", &rows.iter().map(|r| r.2).collect::<Vec<_>>()),
        nums("Income budget", &rows.iter().map(|r| r.3).collect::<Vec<_>>()),
    ])
    .expect("plan")
}

fn link() -> PlanLink {
    PlanLink { data_tab: "t1".into(), plan_tab: "t4".into(), month_column: "Month".into(), keys: vec![KeyPair { data: "Product".into(), lookup: "Product".into() }] }
}

fn base(name: &str, column: &str, agg: TimeAgg, tab: &str) -> NamedMeasure {
    NamedMeasure::Base { name: name.into(), tab: tab.into(), column: column.into(), aggregation: agg, filters: vec![], sign: Sign::Plus, weight: None }
}

fn plan_measure(name: &str, column: &str, agg: PlanAgg) -> NamedMeasure {
    NamedMeasure::Plan { name: name.into(), tab: "t1".into(), plan_tab: "t4".into(), column: column.into(), agg, show: PlanShow::Plan, actual: String::new() }
}

fn run(spec: KpiSpec, name: &str) -> Result<KpiValue, String> {
    let dd = dir(name);
    let measures = vec![
        base("Deposits", "Balance", TimeAgg::ClosingBalance, "t1"),
        base("Income", "Income", TimeAgg::TotalFlow, "t1"),
        plan_measure("Budget", "Budget", PlanAgg::Closing),
        plan_measure("Income budget", "Income budget", PlanAgg::Sum),
        base("Other tab", "Balance", TimeAgg::ClosingBalance, "t9"),
    ];
    let sc = scope5(&dd, "2026-02-28", None, measures).map(|b| {
        let mut c = (*b).clone();
        let table = ModelTab { key: "t4".into(), title: "Budget".into(), source: parquet(&dd, "plan", &plan_df()), steps: vec![] };
        let refusal = check_plan(&link(), &table, &dd).expect("plan check").refusal("Budget");
        c.plans = vec![ScopePlan { link: link(), table: Some(table), refusal }];
        Arc::new(c)
    });
    let req = KpiRequest { spec, tab_key: "t1".into(), base: parquet(&dd, "accounts", &accounts()), steps: vec![], picks: vec![], temp_dir: dd };
    with_scope(sc, || compute_kpi(&req)).map_err(|e| format!("{e:#}"))
}

fn card(measure: &str, target: &str, dir: GoodDirection) -> KpiSpec {
    KpiSpec { source: KpiSource::Measure(measure.into()), good_direction: dir, unit: KpiUnit::AsIs, target: Some(KpiTarget { source: TargetSource::Measure(target.into()), bands: Bands::default() }), ..Default::default() }
}

#[test]
fn t4_a_the_target_is_a_plan_measure_for_the_cards_own_month() {
    // closing deposits in Feb are 160; the budget for Feb is 100 + 50 = 150
    let spec = card("Deposits", "Budget", GoodDirection::Up);
    let v = run(spec.clone(), "p12t4_a").expect("card");
    assert_eq!((v.current, v.target), (Some(160.0), Some(150.0)));
    let k = view(&spec, &v, KpiUnit::Crore);
    assert_eq!(k.threshold, Some(ThresholdColour::Green));
    assert_eq!(k.target_text, "Target 150.00 \u{b7} 106.7% of target \u{b7} green");
    // income of Feb is 16 against 9 + 5 = 14 of budget
    let v = run(card("Income", "Income budget", GoodDirection::Up), "p12t4_b").expect("card");
    assert_eq!((v.current, v.target), (Some(16.0), Some(14.0)));
    // for a cost, down is good: 160 against 150 is within 10%, so amber
    let down = card("Deposits", "Budget", GoodDirection::Down);
    let v = run(down.clone(), "p12t4_c").expect("card");
    assert_eq!(view(&down, &v, KpiUnit::Crore).threshold, Some(ThresholdColour::Amber));
}

#[test]
fn t4_b_a_target_measure_of_another_tab_or_not_in_the_model_is_refused_in_plain_words() {
    let e = run(card("Deposits", "Other tab", GoodDirection::Up), "p12t4_d").expect_err("another tab");
    assert!(e.contains("belongs to another tab"), "{e}");
    let e = run(card("Deposits", "No such", GoodDirection::Up), "p12t4_e").expect_err("missing");
    assert!(e.contains("is not in the model"), "{e}");
}

#[test]
fn t4_c_a_rolling_window_on_a_card_and_its_blank_reason() {
    // Jan + Feb income = 15 + 16 = 31 in two months of data; a 3-month window to Feb starts in Dec 2025, before the data
    let mut spec = KpiSpec { source: KpiSource::Measure("Income".into()), window: TimeWindow::Rolling3, ..Default::default() };
    let v = run(spec.clone(), "p12t4_f").expect("card");
    assert_eq!(v.current, None);
    assert!(v.reason.as_deref().unwrap_or("").contains("before the first data"), "{:?}", v.reason);
    // YTD is the running total from January: 31
    spec.window = TimeWindow::Ytd;
    assert_eq!(run(spec.clone(), "p12t4_g").expect("card").current, Some(31.0));
    // a moving average cannot be compared with an earlier period
    spec.window = TimeWindow::Period;
    spec.smooth = Smooth::Moving3;
    spec.compare = KpiCompare::PriorPeriod;
    assert!(run(spec, "p12t4_h").expect_err("compare").contains("can't be compared with an earlier period"));
}
