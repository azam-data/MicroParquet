//! Phase 13 D1: drill-through. The rows behind a KPI card's number, and behind a pivot cell that holds a base measure. Every
//! number is worked by hand; the rows found are added up and must equal the number shown.

use crate::common::{dir, parquet};
use crate::p9_m3::*;
use crate::p9_m5::scope5;
use micro_parquet::kpi::*;
use micro_parquet::measures::{NamedMeasure, Sign, Term};
use micro_parquet::models::*;
use polars::prelude::*;

fn accounts() -> DataFrame {
    DataFrame::new(vec![
        dates("Date", &["2026-01-31", "2026-01-31", "2026-02-28", "2026-02-28", "2026-02-28"]).into(),
        strs("Product", &["A", "B", "A", "B", "A"]).into(),
        nums("Balance", &[100.0, 50.0, 120.0, 40.0, 30.0]),
        nums("Income", &[10.0, 5.0, 12.0, 4.0, 3.0]),
    ])
    .expect("accounts")
}

fn base(name: &str, column: &str, agg: TimeAgg, product: Option<&str>) -> NamedMeasure {
    let filters = product.map(|p| vec![FilterCondition { column: "Product".into(), op: CondOp::Equals, value: p.into(), ..Default::default() }]).unwrap_or_default();
    NamedMeasure::Base { name: name.into(), tab: "t1".into(), column: column.into(), aggregation: agg, filters, sign: Sign::Plus, weight: None }
}

fn measures() -> Vec<NamedMeasure> {
    vec![
        base("Deposits", "Balance", TimeAgg::ClosingBalance, None),
        base("A deposits", "Balance", TimeAgg::ClosingBalance, Some("A")),
        base("Income", "Income", TimeAgg::TotalFlow, None),
        NamedMeasure::Ratio { name: "Yield".into(), tab: "t1".into(), numerator: vec![Term { measure: "Income".into(), sign: Sign::Plus }], denominator: vec![Term { measure: "Deposits".into(), sign: Sign::Plus }], annualise: None, format: micro_parquet::measures::RatioFormat::Percent },
    ]
}

/// The card's number and the rows behind it: the rows' column added up by the card's own aggregation.
fn rows_of_card(name: &str, spec: KpiSpec) -> (Option<f64>, Result<(DataFrame, String), String>) {
    let dd = dir(name);
    let sc = scope5(&dd, "2026-02-28", None, measures());
    let base_src = parquet(&dd, "accounts", &accounts());
    let req = KpiRequest { spec, tab_key: "t1".into(), base: base_src.clone(), steps: vec![], picks: vec![], temp_dir: dd.clone() };
    let current = micro_parquet::model::with_scope(sc.clone(), || compute_kpi(&req)).expect("number").current;
    let rows = micro_parquet::model::with_scope(sc, || {
        let cr = kpi_rows(&req)?;
        let lf = micro_parquet::engine::compile_tab_lazy(&base_src, &[], &dd)?;
        let mut lf = micro_parquet::engine::filter_by_conditions(lf, &cr.conditions)?;
        lf = micro_parquet::engine::apply_steps_to(lf, &cr.extra_steps)?;
        Ok::<_, anyhow::Error>((lf.collect()?, cr.title))
    });
    (current, rows.map_err(|e| format!("{e:#}")))
}

fn sum(df: &DataFrame, col: &str) -> f64 {
    df.column(col).expect("column").cast(&DataType::Float64).expect("cast").f64().expect("f64").sum().unwrap_or(0.0)
}

#[test]
fn d1_a_a_closing_balance_card_shows_the_rows_of_its_closing_date_and_they_add_up_to_the_number() {
    let spec = KpiSpec { title: "Deposits".into(), source: KpiSource::Measure("Deposits".into()), ..Default::default() };
    let (current, rows) = rows_of_card("p13d1_a", spec);
    let (df, title) = rows.expect("rows");
    // 28 Feb: 120 + 40 + 30
    assert_eq!(current, Some(190.0));
    assert_eq!(df.height(), 3);
    assert_eq!(sum(&df, "Balance"), 190.0);
    assert_eq!(title, "Rows behind Deposits \u{b7} Feb 2026");
}

#[test]
fn d1_b_a_card_on_a_filtered_measure_keeps_the_measures_own_filter() {
    let spec = KpiSpec { title: "A deposits".into(), source: KpiSource::Measure("A deposits".into()), ..Default::default() };
    let (current, rows) = rows_of_card("p13d1_b", spec);
    let (df, _) = rows.expect("rows");
    // Product A on 28 Feb: 120 + 30
    assert_eq!(current, Some(150.0));
    assert_eq!(df.height(), 2);
    assert_eq!(sum(&df, "Balance"), 150.0);
}

#[test]
fn d1_c_a_flow_card_shows_every_day_of_the_month_and_a_ratio_card_says_why_it_has_no_single_set() {
    let spec = KpiSpec { source: KpiSource::Measure("Income".into()), ..Default::default() };
    let (current, rows) = rows_of_card("p13d1_c", spec);
    let (df, _) = rows.expect("rows");
    // Feb income: 12 + 4 + 3
    assert_eq!(current, Some(19.0));
    assert_eq!(sum(&df, "Income"), 19.0);
    let ratio = KpiSpec { source: KpiSource::Measure("Yield".into()), ..Default::default() };
    let dd = dir("p13d1_d");
    let sc = scope5(&dd, "2026-02-28", None, measures());
    let req = KpiRequest { spec: ratio, tab_key: "t1".into(), base: parquet(&dd, "accounts", &accounts()), steps: vec![], picks: vec![], temp_dir: dd };
    let e = micro_parquet::model::with_scope(sc, || kpi_rows(&req)).expect_err("ratio");
    assert!(format!("{e:#}").contains("ratio of several series"), "{e:#}");
}

#[test]
fn d1_d_a_pivot_cell_of_a_base_measure_gives_its_rows_and_a_ratio_cell_is_refused_with_the_reason() {
    let dd = dir("p13d1_e");
    let df = accounts();
    let mv = |name: &str| PivotValue { field: name.into(), agg: PivotAgg::Sum, weight_col: None, time: Some(TimeValue { agg: TimeAgg::Measure, ..Default::default() }) };
    let s = spec(&["Product"], &[], vec![mv("Deposits"), mv("Yield")], &["Month-Year"]);
    let sc = scope5(&dd, "2026-02-28", None, measures());
    let (cells, ratio_reason) = micro_parquet::model::with_scope(sc.clone(), || {
        let b = micro_parquet::pivot_table::build_pivot(micro_parquet::engine::compile_tab_lazy(&parquet(&dd, "accounts", &df), &[], &dd).expect("lf"), &s).expect("pivot");
        let names: Vec<String> = b.df.get_column_names().iter().map(|n| n.to_string()).collect();
        let at_a = b.row_keys.iter().position(|k| k == &vec![Some("A".to_string())]);
        let dep = names.iter().find(|n| n.contains("Deposits")).cloned().unwrap_or_default();
        let yld = names.iter().find(|n| n.contains("Yield")).cloned().unwrap_or_default();
        let rows = at_a.map(|r| micro_parquet::pivot_table::cell_rows(&s, &b, r, &dep).map(|c| c.conditions.len()).map_err(|e| format!("{e:#}")));
        (rows, micro_parquet::pivot_table::measure_rows_reason(&s, &yld))
    });
    // the Deposits cell of product A has conditions (the product and the dates), not a refusal
    assert!(matches!(cells, Some(Ok(n)) if n >= 2), "{cells:?}");
    assert!(ratio_reason.unwrap_or_default().contains("ratio of several series"));
    let greyed = micro_parquet::model::with_scope(sc, || micro_parquet::pivot_table::measure_rows_reason(&s, "Deposits"));
    assert!(greyed.is_none(), "a base measure's column is not greyed");
}
