//! PR #6 review items 1-20: one test group per item (named `i<N>_...`).

use crate::common::*;
use micro_parquet::models::*;
use micro_parquet::pivot_table::*;
use polars::prelude::*;

fn agg(target: &str, func: AggFunc, alias: &str, weight: Option<&str>) -> AggSpec {
    AggSpec {
        target_col: target.into(),
        func,
        weight_col: weight.map(str::to_string),
        alias: alias.into(),
        format_spec: ColumnFormatSpec { fmt_type: MetricFormatType::Auto, decimals: 2 },
    }
}

fn old_pivot(aggs: Vec<AggSpec>, sort: Option<&str>, top_n: Option<usize>) -> TransformStep {
    TransformStep::GroupByPivot { group_cols: vec!["Segment".into()], pivot_col: None, aggs, sort_by_col: sort.map(str::to_string), top_n }
}

fn seg_book() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Segment".into(), &["CMSME", "Retail", "CMSME", "Corporate", "Retail"]),
        Series::new("Balance".into(), &[100.0, 200.0, 300.0, 400.0, 50.0]),
        Series::new("Rate".into(), &[10.0, 8.0, 12.0, 9.0, 6.0]),
    ])
    .expect("book")
}

// ------------------------------------------------------------------------------------------ 1

#[test]
fn i1_convert_lists_the_changed_names_and_the_new_pivot_gives_the_same_numbers() {
    let old = old_pivot(
        vec![
            agg("Balance", AggFunc::Sum, "Total_Balance", None),
            agg("Rate", AggFunc::WeightedAvg, "Avg_Rate", Some("Balance")),
            agg("Balance", AggFunc::Mean, "Average of Balance", None),
        ],
        None,
        None,
    );
    let conv = convert_group_by_pivot(&old).expect("convertible");
    assert_eq!(
        conv.renames,
        vec![
            ("Total_Balance".to_string(), "Sum of Balance".to_string()),
            ("Avg_Rate".to_string(), "W.Avg of Rate by Balance".to_string()),
        ]
    );
    assert!(!conv.renames.iter().any(|(o, _)| o == "Average of Balance"), "an unchanged name is not listed");
    let before = run_steps("i1a", &seg_book(), &[old.clone()]).expect("old");
    let after = run_steps("i1b", &seg_book(), &[TransformStep::from_pivot_spec(conv.spec.clone())]).expect("new");
    assert_eq!(before.height(), after.height());
    // same numbers per segment, under the new names
    let seg_old = strings(&before, "Segment");
    let seg_new = strings(&after, "Segment");
    for (old_name, new_name) in [("Total_Balance", "Sum of Balance"), ("Avg_Rate", "W.Avg of Rate by Balance"), ("Average of Balance", "Average of Balance")] {
        let o = floats(&before, old_name);
        let n = floats(&after, new_name);
        for (i, s) in seg_old.iter().enumerate() {
            let j = seg_new.iter().position(|x| x == s).expect("segment");
            close(n[j], o[i].expect("old value"), 1e-9, &format!("{old_name} {s:?}"));
        }
    }
    // no totals row or column was added
    assert_eq!(after.get_column_names().len(), before.get_column_names().len());
    assert_eq!(after.height(), 3);
}

#[test]
fn i1_convert_reports_what_cannot_be_kept_and_refuses_what_it_cannot_express() {
    let with_top = old_pivot(vec![agg("Balance", AggFunc::Sum, "Bal", None)], Some("Bal"), Some(2));
    let conv = convert_group_by_pivot(&with_top).expect("top-n is listed, not silently dropped");
    assert!(conv.notes.iter().any(|n| n.contains("Top 2")), "{:?}", conv.notes);
    assert_eq!(conv.spec.sort_by_value, Some(PivotSortByValue { value_index: 0, descending: true }));
    // all '% of total' metrics become show-as; mixed ones are refused
    let pct = old_pivot(vec![agg("Balance", AggFunc::PctOfTotal, "Share", None)], None, None);
    let c = convert_group_by_pivot(&pct).expect("pct");
    assert_eq!(c.spec.show_as, PivotShowAs::PctOfColumnTotal);
    let mixed = old_pivot(vec![agg("Balance", AggFunc::PctOfTotal, "Share", None), agg("Balance", AggFunc::Sum, "Bal", None)], None, None);
    assert!(convert_group_by_pivot(&mixed).unwrap_err().to_string().contains("mixes"));
    // a weighted average without its weight, two metrics that are the same calculation, nothing to convert
    assert!(convert_group_by_pivot(&old_pivot(vec![agg("Rate", AggFunc::WeightedAvg, "R", None)], None, None)).is_err());
    let twice = old_pivot(vec![agg("Balance", AggFunc::Sum, "A", None), agg("Balance", AggFunc::Sum, "B", None)], None, None);
    assert!(convert_group_by_pivot(&twice).unwrap_err().to_string().contains("Sum of Balance"));
    assert!(convert_group_by_pivot(&old_pivot(vec![], None, None)).is_err());
    assert!(convert_group_by_pivot(&TransformStep::from_pivot_spec(PivotSpec::default())).is_err());
}
