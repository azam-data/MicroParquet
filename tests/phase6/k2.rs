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
        values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None }],
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
    let tab = ExportTab { title: "Pivot".into(), source: src, steps, col_formats: HashMap::new(), summary, control_col: None, chart: None };
    export_tabs_xlsx(&[tab], &d, &out, NumberDisplayFormat::StandardCommas, 2, true, None, &JobCtx::inert()).expect("export");
    let sheet = unzip_entry(&out, "xl/worksheets/sheet1.xml");
    // the header and 14 rows (the last is the Grand total): row 16 would be an extra totals row
    assert!(sheet.contains("<row r=\"15\""), "the Grand total row is row 15");
    assert!(!sheet.contains("<row r=\"16\""), "no extra totals row");
    let _ = df;
}

// ---------------------------------------------------------------------------------------- K5

use micro_parquet::pivot_table::{pivot_totals_lost, total_rows_expr};

fn calc(new_col: &str, left: &str) -> TransformStep {
    TransformStep::MathFormula { new_col: new_col.into(), left_col: left.into(), op: MathOp::Mul, right_is_col: false, right_val_or_col: "0.5".into() }
}

#[test]
fn k5_pivot_then_calc_column_still_counts_13_and_sums_to_the_grand_total() {
    let pv = pivot_step(&["Term Period"], false, true);
    let (_, _, df0, v) = compiled("k5a0", &[pv.clone()]);
    let g = grand(&df0, &v);
    let steps = vec![pv, calc("Half", &v)];
    let (src, d, df, _) = compiled("k5a", &steps);
    assert_eq!(df.height(), N + 1);
    assert!(!pivot_totals_lost(&steps));
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
    let t = micro_parquet::engine::control_totals_state_for(Ok(compile_tab_lazy(&src, &steps, &d).expect("lf")), Some(&v), &steps);
    assert!((t.sum.expect("a sum") - g).abs() < 0.01);
}

#[test]
fn k5_steps_that_keep_the_label_column_and_the_rows_keep_the_total_rows_recognised() {
    let pv = pivot_step(&["Risk", "Term Period"], true, true);
    let (_, _, df, v) = compiled("k5b", &[pv.clone()]);
    let labels = df.get_column_names().iter().take(2).map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(labels, vec!["Risk", "Term Period"]);
    let sort = TransformStep::SortRows { levels: vec![SortLevel { column: v.clone(), descending: true, ..Default::default() }] };
    let filter = TransformStep::FilterRows { column: "Risk".into(), operator: FilterOp::Eq, value: "Low".into(), keep_matching: true };
    let cond = TransformStep::ConditionalColumn {
        new_col: "Big".into(),
        rules: vec![],
        fallback: "x".into(),
        output_as_float: false,
        blank_result: BlankRule::FallThrough,
    };
    let rename_other = TransformStep::RenameColumn { old_name: v.clone(), new_name: "Total balance".into() };
    let convert_other = TransformStep::ConvertToNumber { columns: vec![v.clone()] };
    let select_keep = TransformStep::SelectAndReorderColumns { cols: vec!["Term Period".into(), "Risk".into(), v.clone()] };
    let clean = TransformStep::CleanLineBreaks { columns: vec![], trim_all: false };
    for (what, step) in [("sort", sort), ("filter", filter), ("conditional column", cond), ("rename of another column", rename_other), ("type change of another column", convert_other), ("column selection that keeps the labels", select_keep), ("clean line breaks", clean), ("calc column", calc("Half", &v))] {
        let steps = vec![pv.clone(), step];
        assert!(!pivot_totals_lost(&steps), "{what}");
        assert!(total_rows_expr(&steps).is_some(), "{what}: the total rows are still recognised");
    }
    // a rename of the label column is followed
    let steps = vec![pv.clone(), TransformStep::RenameColumn { old_name: "Term Period".into(), new_name: "Tenor".into() }];
    assert!(total_rows_expr(&steps).is_some() && !pivot_totals_lost(&steps));
}

#[test]
fn k5_steps_that_reshape_the_rows_stop_the_recognition_and_say_so() {
    let pv = pivot_step(&["Term Period"], false, true);
    let sql = TransformStep::SqlQuery { query: "SELECT * FROM self".into() };
    let group = TransformStep::GroupByPivot { group_cols: vec!["Term Period".into()], pivot_col: None, aggs: vec![], sort_by_col: None, top_n: None };
    let to_number_label = TransformStep::ConvertToNumber { columns: vec!["Term Period".into()] };
    let drop_label = TransformStep::SelectAndReorderColumns { cols: vec!["Sum of Balance".into()] };
    let overwrite_label = TransformStep::MathFormula { new_col: "Term Period".into(), left_col: "Sum of Balance".into(), op: MathOp::Mul, right_is_col: false, right_val_or_col: "1".into() };
    for (what, step) in [("SQL", sql), ("Group", group), ("number conversion of the label column", to_number_label), ("selection without the label column", drop_label), ("a column that overwrites the label column", overwrite_label)] {
        let steps = vec![pv.clone(), step.clone()];
        assert!(pivot_totals_lost(&steps), "{what}: the warning is raised");
        assert!(total_rows_expr(&steps).is_none(), "{what}: no guess");
    }
    // a skipped step does nothing; a later pivot starts afresh
    let skipped = TransformStep::Skipped { step: Box::new(TransformStep::SqlQuery { query: "SELECT * FROM self".into() }) };
    assert!(!pivot_totals_lost(&[pv.clone(), skipped]));
    let again = vec![pv.clone(), TransformStep::SqlQuery { query: "SELECT * FROM self".into() }, pivot_step(&["Term Period"], false, true)];
    assert!(!pivot_totals_lost(&again) && total_rows_expr(&again).is_some());
    // a table that never had a pivot has nothing to warn about
    assert!(!pivot_totals_lost(&[TransformStep::SqlQuery { query: "SELECT * FROM self".into() }]));
}
