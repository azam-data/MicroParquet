//! K2: a pivot's subtotal and Grand total rows are total rows: the summary row, the control total,
//! the chart, the column statistics and the export totals row leave them out, so nothing is counted twice.

use crate::common::*;
use micro_parquet::chart::*;
use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::jobs::JobCtx;
use micro_parquet::models::*;
use micro_parquet::summary::SummaryValue;
use micro_parquet::tab_jobs::*;
use polars::prelude::*;
use std::collections::HashMap;

const N: usize = 13;

/// 13 Term Periods, two Risk grades, a Balance each.
fn book() -> DataFrame {
    let terms: Vec<String> = (0..N * 2).map(|i| format!("{}m", i % N + 1)).collect();
    let risk: Vec<&str> = (0..N * 2).map(|i| if i < N { "Low" } else { "High" }).collect();
    let bal: Vec<f64> = (0..N * 2).map(|i| 1_000_000.25 + i as f64 * 12_345.5).collect();
    DataFrame::new(vec![
        Series::new("Term Period".into(), terms).into(),
        Series::new("Risk".into(), risk).into(),
        Series::new("Balance".into(), bal).into(),
    ])
    .expect("book")
}

fn pivot_step(rows: &[&str], subtotals: bool, column_totals: bool) -> TransformStep {
    TransformStep::from_pivot_spec(PivotSpec {
        rows: rows.iter().map(|s| s.to_string()).collect(),
        values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None }],
        column_totals,
        subtotals,
        ..Default::default()
    })
}

fn value_col(df: &DataFrame) -> String {
    df.get_column_names().last().expect("a column").to_string()
}

fn compiled(name: &str, steps: &[TransformStep]) -> (TabSource, std::path::PathBuf, DataFrame, String) {
    let d = dir(name);
    let src = parquet(&d, "in", &book());
    let df = compile_tab_lazy(&src, steps, &d).expect("compiles").collect().expect("collects");
    let v = value_col(&df);
    (src, d, df, v)
}

fn grand(df: &DataFrame, v: &str) -> f64 {
    *floats(df, v).last().expect("a row").as_ref().expect("a value")
}

#[test]
fn k2_term_period_pivot_summary_counts_13_and_sums_to_the_grand_total_and_the_control_total_equals_it() {
    let steps = vec![pivot_step(&["Term Period"], false, true)];
    let (src, d, df, v) = compiled("k2a", &steps);
    assert_eq!(df.height(), N + 1, "13 rows and the Grand total");
    assert_eq!(strings(&df, "Term Period").last().cloned().flatten().as_deref(), Some("Grand total"));
    let g = grand(&df, &v);
    let book_sum: f64 = floats(&book(), "Balance").into_iter().flatten().sum();
    assert!((g - book_sum).abs() < 0.01, "{g} vs {book_sum}");

    // the summary row: Count 13, Sum = the Grand total (not twice it)
    let mut over: HashMap<String, SummarySpec> = HashMap::new();
    over.insert("Term Period".into(), SummarySpec { agg: SummaryAgg::Count, weight_col: None });
    over.insert(v.clone(), SummarySpec { agg: SummaryAgg::Sum, weight_col: None });
    let r = compute_tab_summary(SummaryRequest { tab_id: 1, source: src.clone(), steps: steps.clone(), overrides: over, control_col: None, temp_dir: d.clone() }, &JobCtx::inert()).expect("summary");
    let cell = |c: &str| r.cells.iter().find(|x| x.column == c).unwrap_or_else(|| panic!("cell {c}")).value.clone();
    assert_eq!(cell("Term Period"), Some(SummaryValue::Count(N as u64)));
    match cell(&v) {
        Some(SummaryValue::Number(x)) => assert!((x - g).abs() < 0.01, "Sum {x} equals the Grand total {g}"),
        other => panic!("{other:?}"),
    }

    // the status-bar control total: the rows are all 14, the sum equals the Grand total
    let lf = compile_tab_lazy(&src, &steps, &d).expect("lf");
    let t = micro_parquet::engine::control_totals_state_for(Ok(lf), Some(&v), &steps);
    assert_eq!(t.rows, Some(N + 1));
    assert!((t.sum.expect("a sum") - g).abs() < 0.01, "control {:?} vs {g}", t.sum);

    // the chart: 13 categories, none of them "Grand total"
    let spec = ChartSpec { category: "Term Period".into(), value: Some(v.clone()), agg: ChartAgg::Sum, ..Default::default() };
    let c = compute_chart_job(ChartRequest { tab_id: 1, token: 1, source: src.clone(), steps: steps.clone(), spec, temp_dir: d.clone() }, &JobCtx::inert()).expect("job").data.expect("chart");
    assert_eq!(c.categories.len(), N, "{:?}", c.categories);
    assert!(!c.categories.iter().any(|x| x == "Grand total"));
    let chart_sum: f64 = c.values[0].iter().flatten().sum();
    assert!((chart_sum - g).abs() < 0.01, "the bars add up to the Grand total: {chart_sum}");

    // the column inspector's statistics
    let s = compute_tab_column_stats(ColumnStatsRequest { tab_id: 1, source: src.clone(), steps: steps.clone(), column: v.clone(), temp_dir: d.clone() }, &JobCtx::inert()).expect("stats").stats;
    assert_eq!(s.rows, N as u64);
    let n = s.number.expect("number stats");
    assert!((n.sum - g).abs() < 0.01, "stats sum {} vs {g}", n.sum);
    assert!(n.max < g, "the Grand total is not the maximum");
}

#[test]
fn k2_subtotals_and_the_grand_total_are_both_left_out_and_a_blank_label_stays_data() {
    // two Rows fields with subtotals: Low and High subtotals plus the Grand total
    let steps = vec![pivot_step(&["Risk", "Term Period"], true, true)];
    let (src, d, df, v) = compiled("k2b", &steps);
    assert_eq!(df.height(), 2 * N + 2 + 1, "26 detail rows, 2 subtotals, the Grand total");
    let g = grand(&df, &v);
    let t = micro_parquet::engine::control_totals_state_for(Ok(compile_tab_lazy(&src, &steps, &d).expect("lf")), Some(&v), &steps);
    assert!((t.sum.expect("a sum") - g).abs() < 0.01, "{:?} vs {g}", t.sum);
    let mut over: HashMap<String, SummarySpec> = HashMap::new();
    over.insert("Term Period".into(), SummarySpec { agg: SummaryAgg::Count, weight_col: None });
    let r = compute_tab_summary(SummaryRequest { tab_id: 1, source: src.clone(), steps: steps.clone(), overrides: over, control_col: None, temp_dir: d.clone() }, &JobCtx::inert()).expect("summary");
    // (the Count of a label column counts the non-blank labels: the 26 detail rows have one)
    let c = r.cells.iter().find(|x| x.column == "Term Period").expect("cell").value.clone();
    assert_eq!(c, Some(SummaryValue::Count(2 * N as u64)));

    // a blank key is "(blank)", a detail row, so it stays
    let mut b = book();
    b.replace("Term Period", Series::new("Term Period".into(), (0..N * 2).map(|i| if i == 0 { None } else { Some(format!("{}m", i % N + 1)) }).collect::<Vec<Option<String>>>())).expect("blank");
    let d2 = dir("k2c");
    let src2 = parquet(&d2, "in", &b);
    let steps2 = vec![pivot_step(&["Term Period"], false, true)];
    let t2 = micro_parquet::engine::control_totals_state_for(Ok(compile_tab_lazy(&src2, &steps2, &d2).expect("lf")), Some("Sum of Balance"), &steps2);
    let all: f64 = floats(&b, "Balance").into_iter().flatten().sum();
    assert!((t2.sum.expect("a sum") - all).abs() < 0.01, "{:?} vs {all}", t2.sum);
}

#[test]
fn k2_a_pivot_sheet_keeps_its_grand_total_and_gets_no_extra_totals_row_in_excel() {
    let steps = vec![pivot_step(&["Term Period"], false, true)];
    let (src, d, df, v) = compiled("k2d", &steps);
    let out = d.join("pivot.xlsx");
    let mut summary: HashMap<String, SummarySpec> = HashMap::new();
    summary.insert(v.clone(), SummarySpec { agg: SummaryAgg::Sum, weight_col: None });
    let tab = ExportTab { title: "Pivot".into(), source: src, steps, col_formats: HashMap::new(), summary, control_col: None };
    export_tabs_xlsx(&[tab], &d, &out, NumberDisplayFormat::StandardCommas, 2, true, None, &JobCtx::inert()).expect("export");
    let sheet = unzip_entry(&out, "xl/worksheets/sheet1.xml");
    // the header and 14 rows (the last is the Grand total): row 16 would be an extra totals row
    assert!(sheet.contains("<row r=\"15\""), "the Grand total row is row 15");
    assert!(!sheet.contains("<row r=\"16\""), "no extra totals row");
    let _ = df;
}
