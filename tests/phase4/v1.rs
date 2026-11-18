//! V1: the pivot builder's engine (Rows, Columns, Values, Filters, totals, show-as, order).

use crate::common::*;
use micro_parquet::models::*;
use micro_parquet::pivot_table::*;
use polars::prelude::*;

const EN: &str = "\u{2013}";

fn bucket(n: u32) -> String {
    match n {
        2 => format!("02. 2{EN}7 Days"),
        4 => format!("04. 1{EN}3 Months"),
        _ => format!("10. 3{EN}5 Years"),
    }
}

/// The frame of the acceptance item: Bucket, Bank, Balance, Rate.
fn book() -> DataFrame {
    let b = |n: &[u32]| n.iter().map(|x| bucket(*x)).collect::<Vec<_>>();
    DataFrame::new(vec![
        Series::new("Bucket".into(), b(&[2, 10, 4, 2, 4, 10, 2])),
        Series::new("Bank".into(), &["Conventional", "Islami", "Conventional", "Islami", "Islami", "Conventional", "Conventional"]),
        Series::new("Balance".into(), &[100.0, 300.0, 200.0, 50.0, 150.0, 400.0, 100.0]),
        Series::new("Rate".into(), &[5.0, 9.0, 6.0, 8.0, 7.0, 10.0, 7.0]),
    ])
    .expect("the book")
}

fn sum_balance() -> PivotValue {
    PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None }
}

fn spec(rows: &[&str], cols: &[&str], values: Vec<PivotValue>) -> PivotSpec {
    PivotSpec {
        rows: rows.iter().map(|s| s.to_string()).collect(),
        columns: cols.iter().map(|s| s.to_string()).collect(),
        values,
        row_totals: true,
        column_totals: true,
        ..Default::default()
    }
}

fn build(df: &DataFrame, s: &PivotSpec) -> DataFrame {
    build_pivot(df.clone().lazy(), s).unwrap_or_else(|e| panic!("{e:#}")).df
}

fn names(df: &DataFrame) -> Vec<String> {
    df.get_column_names().iter().map(|s| s.to_string()).collect()
}

fn cell(df: &DataFrame, col: &str, row: usize) -> Option<f64> {
    floats(df, col)[row]
}

#[test]
fn v1_rows_bucket_columns_bank_sum_balance_has_natural_row_order_and_true_totals() {
    let out = build(&book(), &spec(&["Bucket"], &["Bank"], vec![sum_balance()]));
    assert_eq!(names(&out), vec!["Bucket", "Conventional", "Islami", "Total"]);
    let labels: Vec<String> = strings(&out, "Bucket").into_iter().map(|s| s.unwrap_or_default()).collect();
    assert_eq!(labels, vec![bucket(2), bucket(4), bucket(10), GRAND_TOTAL_LABEL.to_string()], "02, 04, 10 (never by size), then the grand total");
    // 10 is the biggest bucket by balance and it is still last
    assert_eq!(floats(&out, "Conventional"), vec![Some(200.0), Some(200.0), Some(400.0), Some(800.0)]);
    assert_eq!(floats(&out, "Islami"), vec![Some(50.0), Some(150.0), Some(300.0), Some(500.0)]);
    assert_eq!(floats(&out, "Total"), vec![Some(250.0), Some(350.0), Some(700.0), Some(1300.0)]);
    // the grand totals equal the column sums and the overall sum
    let total_balance: f64 = floats(&book(), "Balance").iter().flatten().sum();
    for c in ["Conventional", "Islami", "Total"] {
        let v = floats(&out, c);
        let body: f64 = v[..3].iter().flatten().sum();
        assert_eq!(v[3], Some(body), "{c}: the last row is the sum of the rows above");
    }
    assert_eq!(cell(&out, "Total", 3), Some(total_balance), "the grand total is the overall sum");
}

#[test]
fn v1_weighted_average_per_cell_follows_the_phase_2_rule_and_mixed_signs_are_null() {
    let w = PivotValue { field: "Rate".into(), agg: PivotAgg::WeightedAvg, weight_col: Some("Balance".into()) };
    let out = build(&book(), &spec(&["Bucket"], &["Bank"], vec![w.clone()]));
    // 02 / Conventional: (100 x 5 + 100 x 7) / 200 = 6, 02 / Islami: 8
    close(cell(&out, "Conventional", 0), 6.0, 1e-9, "02 Conventional");
    close(cell(&out, "Islami", 0), 8.0, 1e-9, "02 Islami");
    // the grand total is the weighted average of everything, not the mean of the cells
    let all: f64 = [(100.0, 5.0), (300.0, 9.0), (200.0, 6.0), (50.0, 8.0), (150.0, 7.0), (400.0, 10.0), (100.0, 7.0)].iter().map(|(b, r)| b * r).sum::<f64>() / 1300.0;
    close(cell(&out, "Total", 3), all, 1e-9, "grand total");

    // weights of both signs in a cell: no rate for that cell (and for the totals that contain it)
    let df = DataFrame::new(vec![
        Series::new("Bucket".into(), &["A", "A", "B", "B"]),
        Series::new("Bank".into(), &["X", "X", "X", "X"]),
        Series::new("Balance".into(), &[1000.0, -999.0, 100.0, 300.0]),
        Series::new("Rate".into(), &[5.0, 6.0, 8.0, 12.0]),
    ])
    .expect("frame");
    let out = build(&df, &spec(&["Bucket"], &["Bank"], vec![w.clone()]));
    assert_eq!(cell(&out, "X", 0), None, "A mixes +1000 and -999");
    close(cell(&out, "X", 1), 11.0, 1e-9, "B is one-signed: (800 + 3600) / 400");
    assert_eq!(cell(&out, "Total", 2), None, "the grand total has weights of both signs too");
    let notes = pivot_mixed_sign_cells(df.lazy(), &spec(&["Bucket"], &["Bank"], vec![w])).expect("the check runs");
    assert_eq!(notes, vec![("W.Avg of Rate by Balance".to_string(), 1)]);
}

#[test]
fn v1_the_status_bar_note_counts_pivot_cells_whose_weights_mix_signs() {
    let df = DataFrame::new(vec![
        Series::new("Bucket".into(), &["A", "A", "B"]),
        Series::new("Balance".into(), &[1000.0, -999.0, 100.0]),
        Series::new("Rate".into(), &[5.0, 6.0, 8.0]),
    ])
    .expect("frame");
    let d = dir("v1_note");
    let src = parquet(&d, "in", &df);
    let w = PivotValue { field: "Rate".into(), agg: PivotAgg::WeightedAvg, weight_col: Some("Balance".into()) };
    let step = TransformStep::from_pivot_spec(spec(&["Bucket"], &[], vec![w]));
    let notes = micro_parquet::engine::weighted_mixed_sign_notes(&src, &[step], &d).expect("notes");
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("W.Avg of Rate by Balance is blank for 1 cell(s)"), "{notes:?}");
    // a pivot without a weighted average says nothing
    let plain = TransformStep::from_pivot_spec(spec(&["Bucket"], &[], vec![sum_balance()]));
    assert!(micro_parquet::engine::weighted_mixed_sign_notes(&src, &[plain], &d).expect("notes").is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn v1_percent_of_row_total_gives_rows_of_100() {
    let mut s = spec(&["Bucket"], &["Bank"], vec![sum_balance()]);
    s.show_as = PivotShowAs::PctOfRowTotal;
    let out = build(&book(), &s);
    for r in 0..3 {
        let sum = cell(&out, "Conventional", r).expect("c") + cell(&out, "Islami", r).expect("i");
        assert!((sum - 100.0).abs() <= 0.01, "row {r} adds up to {sum}");
        close(cell(&out, "Total", r), 100.0, 1e-9, "the Total column of a row share is 100");
    }
    // the last row is each bank's share of the whole
    close(cell(&out, "Conventional", 3), 800.0 / 1300.0 * 100.0, 1e-9, "Conventional share");
    // columns and grand total
    s.show_as = PivotShowAs::PctOfColumnTotal;
    let out = build(&book(), &s);
    close(cell(&out, "Conventional", 0), 25.0, 1e-9, "200 / 800");
    s.show_as = PivotShowAs::PctOfGrandTotal;
    let out = build(&book(), &s);
    close(cell(&out, "Total", 3), 100.0, 1e-9, "grand");
    close(cell(&out, "Islami", 2), 300.0 / 1300.0 * 100.0, 1e-9, "300 / 1300");
}

#[test]
fn v1_a_filter_keeps_only_that_column_and_the_right_totals() {
    let mut s = spec(&["Bucket"], &["Bank"], vec![sum_balance()]);
    s.filters = vec![PivotFilter { field: "Bank".into(), values: vec![Some("Islami".into())] }];
    let out = build(&book(), &s);
    assert_eq!(names(&out), vec!["Bucket", "Islami", "Total"], "no Conventional column");
    assert_eq!(floats(&out, "Islami"), vec![Some(50.0), Some(150.0), Some(300.0), Some(500.0)]);
    assert_eq!(floats(&out, "Total"), vec![Some(50.0), Some(150.0), Some(300.0), Some(500.0)]);
    // a filter on a field that is not shown, and a blank
    let df = DataFrame::new(vec![
        Series::new("Zone".into(), &[Some("N"), Some("S"), None, Some("  ")]),
        Series::new("Balance".into(), &[1.0, 2.0, 4.0, 8.0]),
    ])
    .expect("frame");
    let mut s = spec(&["Zone"], &[], vec![sum_balance()]);
    s.filters = vec![PivotFilter { field: "Zone".into(), values: vec![Some("S".into()), None] }];
    let out = build(&df, &s);
    assert_eq!(strings(&out, "Zone"), vec![Some("S".into()), Some(BLANK_LABEL.into()), Some(GRAND_TOTAL_LABEL.into())], "blank last, null and spaces are one blank");
    assert_eq!(floats(&out, "Sum of Balance"), vec![Some(2.0), Some(12.0), Some(14.0)]);
}

#[test]
fn v1_totals_toggles_subtotals_and_several_values() {
    let mut s = spec(&["Bucket"], &["Bank"], vec![sum_balance()]);
    s.row_totals = false;
    s.column_totals = false;
    let out = build(&book(), &s);
    assert_eq!(names(&out), vec!["Bucket", "Conventional", "Islami"]);
    assert_eq!(out.height(), 3, "no totals row");

    // two values: columns are named by label | value; Count is a whole number column
    let count = PivotValue { field: "Bucket".into(), agg: PivotAgg::Count, weight_col: None };
    let s = spec(&["Bucket"], &["Bank"], vec![sum_balance(), count]);
    let out = build(&book(), &s);
    assert_eq!(
        names(&out),
        vec!["Bucket", "Conventional | Sum of Balance", "Conventional | Count of Bucket", "Islami | Sum of Balance", "Islami | Count of Bucket", "Total | Sum of Balance", "Total | Count of Bucket"]
    );
    assert_eq!(out.column("Total | Count of Bucket").expect("c").dtype(), &DataType::Int64);
    assert_eq!(out.column("Total | Count of Bucket").expect("c").i64().expect("i64").into_iter().collect::<Vec<_>>(), vec![Some(3), Some(2), Some(2), Some(7)]);

    // subtotals: two row fields
    let df = DataFrame::new(vec![
        Series::new("Bank".into(), &["B", "B", "A", "A", "A"]),
        Series::new("Zone".into(), &["Zone 10", "Zone 2", "Zone 2", "Zone 2", "Zone 10"]),
        Series::new("Balance".into(), &[1.0, 2.0, 4.0, 8.0, 16.0]),
    ])
    .expect("frame");
    let mut s = spec(&["Bank", "Zone"], &[], vec![sum_balance()]);
    s.subtotals = true;
    let out = build(&df, &s);
    assert_eq!(
        strings(&out, "Bank"),
        vec!["A", "A", "A Total", "B", "B", "B Total", "Grand total"].into_iter().map(|x| Some(x.to_string())).collect::<Vec<_>>()
    );
    assert_eq!(
        strings(&out, "Zone"),
        vec!["Zone 2", "Zone 10", "", "Zone 2", "Zone 10", "", ""].into_iter().map(|x| Some(x.to_string())).collect::<Vec<_>>(),
        "natural order inside a group"
    );
    assert_eq!(floats(&out, "Sum of Balance"), vec![Some(12.0), Some(16.0), Some(28.0), Some(2.0), Some(1.0), Some(3.0), Some(31.0)]);
    // without subtotals the same rows, no subtotal lines
    s.subtotals = false;
    assert_eq!(build(&df, &s).height(), 5);
}

#[test]
fn v1_sort_by_value_is_the_only_way_to_order_by_size_and_dates_numbers_and_blanks_sort_naturally() {
    let mut s = spec(&["Bucket"], &["Bank"], vec![sum_balance()]);
    s.sort_by_value = Some(PivotSortByValue { value_index: 0, descending: true });
    let out = build(&book(), &s);
    let labels: Vec<String> = strings(&out, "Bucket").into_iter().map(|x| x.unwrap_or_default()).collect();
    assert_eq!(labels[..3].to_vec(), vec![bucket(10), bucket(4), bucket(2)], "largest first when asked");
    s.sort_by_value = Some(PivotSortByValue { value_index: 0, descending: false });
    let out = build(&book(), &s);
    assert_eq!(strings(&out, "Bucket")[0].as_deref(), Some(bucket(2).as_str()));

    // numbers by value (9 before 10), dates by date, blank last
    let df = DataFrame::new(vec![
        Series::new("Term".into(), &[Some(10.0f64), Some(9.0), None, Some(100.0)]),
        dates("Mat", &["2027-01-05", "2026-12-31", "", "2026-02-01"]),
        Series::new("Balance".into(), &[1.0, 1.0, 1.0, 1.0]),
    ])
    .expect("frame");
    let out = build(&df, &spec(&["Term"], &[], vec![sum_balance()]));
    assert_eq!(strings(&out, "Term")[..4].to_vec(), vec![Some("9".to_string()), Some("10".to_string()), Some("100".to_string()), Some(BLANK_LABEL.to_string())]);
    let out = build(&df, &spec(&["Mat"], &[], vec![sum_balance()]));
    assert_eq!(strings(&out, "Mat")[..4].to_vec(), vec![Some("2026-02-01".to_string()), Some("2026-12-31".to_string()), Some("2027-01-05".to_string()), Some(BLANK_LABEL.to_string())]);
}

#[test]
fn v1_every_aggregation_and_the_messages_when_a_choice_does_not_fit() {
    let df = DataFrame::new(vec![
        Series::new("Bank".into(), &["A", "A", "A", "B"]),
        Series::new("Branch".into(), &[Some("x"), Some("x"), Some("y"), None]),
        Series::new("Balance".into(), &[Some(10.0), None, Some(30.0), Some(5.0)]),
    ])
    .expect("frame");
    let v = |agg| PivotValue { field: "Balance".into(), agg, weight_col: None };
    let one = |agg: PivotAgg| build(&df, &spec(&["Bank"], &[], vec![v(agg)]));
    assert_eq!(floats(&one(PivotAgg::Sum), "Sum of Balance"), vec![Some(40.0), Some(5.0), Some(45.0)]);
    assert_eq!(floats(&one(PivotAgg::Average), "Average of Balance"), vec![Some(20.0), Some(5.0), Some(15.0)], "the blank is skipped, the total is a true average");
    assert_eq!(floats(&one(PivotAgg::Min), "Min of Balance"), vec![Some(10.0), Some(5.0), Some(5.0)]);
    assert_eq!(floats(&one(PivotAgg::Max), "Max of Balance"), vec![Some(30.0), Some(5.0), Some(30.0)]);
    assert_eq!(floats(&one(PivotAgg::Count), "Count of Balance"), vec![Some(2.0), Some(1.0), Some(3.0)], "the blank balance is not counted");
    let cd = PivotValue { field: "Branch".into(), agg: PivotAgg::CountDistinct, weight_col: None };
    let out = build(&df, &spec(&["Bank"], &[], vec![cd]));
    assert_eq!(floats(&out, "Count distinct of Branch"), vec![Some(2.0), Some(0.0), Some(2.0)], "x and y; a blank is not a value; the total counts distinct values of all rows");

    let err = |s: PivotSpec| format!("{:#}", build_pivot(df.clone().lazy(), &s).err().expect("an error"));
    assert!(err(spec(&["Bank"], &[], vec![])).contains("add at least one field to Values"));
    assert!(err(spec(&[], &[], vec![v(PivotAgg::Sum)])).contains("add a field to Rows or to Columns"));
    assert!(err(spec(&["Nope"], &[], vec![v(PivotAgg::Sum)])).contains("'Nope'"));
    assert!(err(spec(&["Bank"], &["Bank"], vec![v(PivotAgg::Sum)])).contains("used twice"));
    let text_sum = PivotValue { field: "Branch".into(), agg: PivotAgg::Sum, weight_col: None };
    let m = err(spec(&["Bank"], &[], vec![text_sum]));
    assert!(m.contains("needs a number column") && m.contains("'Branch' is text"), "{m}");
    let no_weight = PivotValue { field: "Balance".into(), agg: PivotAgg::WeightedAvg, weight_col: None };
    assert!(err(spec(&["Bank"], &[], vec![no_weight])).contains("needs a weight column"));
    let mut s = spec(&["Bank"], &[], vec![v(PivotAgg::Average)]);
    s.show_as = PivotShowAs::PctOfGrandTotal;
    let m = err(s);
    assert!(m.contains("cannot be shown as a share") && m.contains("Average of Balance"), "{m}");
    let mut s = spec(&["Bank"], &[], vec![v(PivotAgg::Sum)]);
    s.sort_by_value = Some(PivotSortByValue { value_index: 3, descending: false });
    assert!(err(s).contains("Sort by value"));
    assert!(err(spec(&["Bank"], &[], vec![v(PivotAgg::Sum), v(PivotAgg::Sum)])).contains("twice"));
}

#[test]
fn v1_columns_only_rows_only_and_the_cell_limit() {
    // columns, no rows: one row
    let out = build(&book(), &spec(&[], &["Bank"], vec![sum_balance()]));
    assert_eq!(names(&out), vec![NO_ROWS_COLUMN, "Conventional", "Islami", "Total"]);
    assert_eq!(out.height(), 1);
    assert_eq!(floats(&out, "Total"), vec![Some(1300.0)]);
    // rows, no columns: a column per value, a last row of totals
    let out = build(&book(), &spec(&["Bank"], &[], vec![sum_balance()]));
    assert_eq!(names(&out), vec!["Bank", "Sum of Balance"]);
    assert_eq!(floats(&out, "Sum of Balance"), vec![Some(800.0), Some(500.0), Some(1300.0)]);
    // too many columns
    let n = 600;
    let df = DataFrame::new(vec![
        Series::new("K".into(), (0..n).map(|i| format!("k{i}")).collect::<Vec<_>>()),
        Series::new("Balance".into(), vec![1.0; n]),
    ])
    .expect("frame");
    let m = format!("{:#}", build_pivot(df.lazy(), &spec(&[], &["K"], vec![sum_balance()])).err().expect("too many"));
    assert!(m.contains("600 columns is too many"), "{m}");
}

#[test]
fn v1_the_step_is_in_a_pipeline_a_macro_and_old_json_loads_with_defaults() {
    let step = TransformStep::from_pivot_spec(spec(&["Bucket"], &["Bank"], vec![sum_balance()]));
    let out = run_steps("v1_step", &book(), &[step.clone()]).expect("the step runs");
    assert_eq!(out.height(), 4);
    // a macro file keeps it
    let d = dir("v1_macro");
    let recipe = MacroRecipe {
        name: "p".into(),
        description: String::new(),
        created_at: String::new(),
        enforce_balance_control: true,
        control_column: Some("Balance".into()),
        optional_ingestion_schema: None,
        steps: vec![step.clone()],
        summary: Default::default(),
    };
    let p = d.join("pivot.mpq.json");
    micro_parquet::macros::save_macro_to_file(&recipe, &p).expect("save");
    let back = micro_parquet::macros::load_macro_from_file(&p).expect("load");
    assert_eq!(back.steps[0].as_pivot_spec(), step.as_pivot_spec());
    let again = {
        let src = parquet(&d, "in", &book());
        micro_parquet::engine::compile_tab_lazy(&src, &back.steps, &d).expect("compile").collect().expect("collect")
    };
    assert_eq!(again.get_column_names(), out.get_column_names());
    // the smallest JSON a step can have: the other fields take their defaults
    let minimal: TransformStep = serde_json::from_str(r#"{"PivotTable":{"rows":["Bucket"],"values":[{"field":"Balance"}]}}"#).expect("loads");
    let s = minimal.as_pivot_spec().expect("a pivot");
    assert!(s.row_totals && s.column_totals && !s.subtotals);
    assert_eq!((s.show_as, s.scale, s.sort_by_value), (PivotShowAs::NoCalculation, PivotScale::Global, None));
    assert_eq!(s.values[0].agg, PivotAgg::Sum);
    // a turned-off step changes nothing, the Python export says it cannot translate it
    let off = TransformStep::Skipped { step: Box::new(step.clone()) };
    assert_eq!(run_steps("v1_off", &book(), &[off]).expect("runs").height(), 7);
    let py = micro_parquet::macros::python_untranslated_steps(&[step]);
    assert_eq!(py.len(), 1);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn v1_an_old_group_by_pivot_macro_replays_to_what_main_gave() {
    // `phase4_pivot_main.*` were made by the code of main (c01b78b) before the pivot builder
    assert_fixture_replays("phase4_pivot_main", &["FilterRows", "GroupByPivot"]);
}

#[test]
fn v1_distinct_values_for_the_filter_checklist() {
    let (v, more) = distinct_field_values(book().lazy(), "Bucket", 10).expect("values");
    assert_eq!(v, vec![Some(bucket(2)), Some(bucket(4)), Some(bucket(10))]);
    assert!(!more);
    let (v, more) = distinct_field_values(book().lazy(), "Balance", 3).expect("values");
    assert_eq!(v.len(), 3);
    assert!(more, "there are more values than the limit");
    assert_eq!(v, vec![Some("50".to_string()), Some("100".to_string()), Some("150".to_string())].into_iter().collect::<Vec<_>>(), "numbers by value");
}
