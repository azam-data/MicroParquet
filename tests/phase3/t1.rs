//! Phase 3 T1: multi-condition filter (small synthetic frame).

use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::macros::*;
use micro_parquet::models::*;
use polars::prelude::*;
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mp_t1_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Five loans: text IDs that look numeric, a branch, a balance with a null, a Date column and a
/// remark with blanks.
fn source(d: &std::path::Path) -> TabSource {
    let dates = Series::new("Opndat".into(), &[Some(20_332i32), Some(20_340), Some(20_361), Some(20_362), None]); // 2025-09-01.. as days
    let mut df = DataFrame::new(vec![
        Series::new("Actnum".into(), &["0012", "0034", "12", "0099", "0100"]).into(),
        Series::new("Brancd".into(), &["DHK", "DHK", "CTG", "CTG", "DHK"]).into(),
        Series::new("Balance".into(), &[Some(1000.0f64), Some(2500.0), Some(5000.0), None, Some(8000.0)]).into(),
        dates.cast(&DataType::Date).unwrap().into(),
        Series::new("Remarks".into(), &[Some("Overdue loan"), Some("  "), None, Some("OK"), Some("overdue (rescheduled)")]).into(),
    ])
    .unwrap();
    let p = d.join("loans.parquet");
    ParquetWriter::new(std::fs::File::create(&p).unwrap()).finish(&mut df).unwrap();
    TabSource::SingleFile { path: p, is_parquet: true, config: IngestionConfig::default() }
}

fn cond(column: &str, op: CondOp, value: &str) -> FilterCondition {
    FilterCondition { column: column.into(), op, value: value.into(), value2: String::new() }
}

fn rows(src: &TabSource, d: &std::path::Path, step: TransformStep) -> Vec<String> {
    let df = compile_tab_lazy(src, &[step], d).unwrap().collect().unwrap();
    df.column("Actnum").unwrap().str().unwrap().into_iter().map(|v| v.unwrap_or("?").to_string()).collect()
}

fn group(conds: Vec<FilterCondition>, join: GroupJoin, keep: bool) -> TransformStep {
    TransformStep::FilterGroup { conditions: conds, join, keep_matching: keep }
}

#[test]
fn t1_and_and_or_give_the_expected_rows() {
    let d = dir("andor");
    let src = source(&d);
    let both = vec![cond("Brancd", CondOp::Equals, "DHK"), cond("Balance", CondOp::Gt, "1500")];
    assert_eq!(rows(&src, &d, group(both.clone(), GroupJoin::And, true)), vec!["0034", "0100"]);
    assert_eq!(rows(&src, &d, group(both, GroupJoin::Or, true)), vec!["0012", "0034", "12", "0100"], "DHK, or balance above 1,500 (the null balance never matches)");
    // a single condition behaves as before
    assert_eq!(rows(&src, &d, group(vec![cond("Brancd", CondOp::NotEquals, "DHK")], GroupJoin::And, true)), vec!["12", "0099"]);
    // no conditions: nothing is filtered
    assert_eq!(rows(&src, &d, group(vec![], GroupJoin::And, true)).len(), 5);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn t1_delete_matching_keeps_rows_with_an_unknown_test() {
    let d = dir("delete");
    let src = source(&d);
    // Balance > 1500 is true for 3 rows, false for 1, unknown (null) for 1: the null row is kept.
    let step = group(vec![cond("Balance", CondOp::Gt, "1500")], GroupJoin::And, false);
    assert_eq!(rows(&src, &d, step), vec!["0012", "0099"]);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn t1_between_works_on_dates_numbers_and_text() {
    let d = dir("between");
    let src = source(&d);
    // dates: inclusive, entered as YYYY-MM-DD (2025-09-09 .. 2025-09-30)
    let from = (chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap() + chrono::Duration::days(20_340)).format("%Y-%m-%d").to_string();
    let to = (chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap() + chrono::Duration::days(20_361)).format("%Y-%m-%d").to_string();
    let dates = FilterCondition { column: "Opndat".into(), op: CondOp::Between, value: from.clone(), value2: to.clone() };
    assert_eq!(rows(&src, &d, group(vec![dates], GroupJoin::And, true)), vec!["0034", "12"], "both ends are included");
    // numbers
    let nums = FilterCondition { column: "Balance".into(), op: CondOp::Between, value: "2500".into(), value2: "5000".into() };
    assert_eq!(rows(&src, &d, group(vec![nums], GroupJoin::And, true)), vec!["0034", "12"]);
    // text compares as text
    let text = FilterCondition { column: "Brancd".into(), op: CondOp::Between, value: "CTG".into(), value2: "DHK".into() };
    assert_eq!(rows(&src, &d, group(vec![text], GroupJoin::And, true)).len(), 5);
    // a bad bound names the column
    let bad = FilterCondition { column: "Opndat".into(), op: CondOp::Between, value: "01/09/2025".into(), value2: to };
    let err = compile_tab_lazy(&src, &[group(vec![bad], GroupJoin::And, true)], &d).and_then(|lf| Ok(lf.collect()?)).err().unwrap();
    assert!(format!("{err:#}").contains("Opndat") && format!("{err:#}").contains("YYYY-MM-DD"), "{err:#}");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn t1_in_list_matches_text_ids_exactly_even_when_they_look_numeric() {
    let d = dir("inlist");
    let src = source(&d);
    // "12" is not "0012": IDs are text
    assert_eq!(rows(&src, &d, group(vec![cond("Actnum", CondOp::InList, "0012, 12")], GroupJoin::And, true)), vec!["0012", "12"]);
    assert_eq!(rows(&src, &d, group(vec![cond("Actnum", CondOp::InList, "12")], GroupJoin::And, true)), vec!["12"]);
    assert_eq!(rows(&src, &d, group(vec![cond("Actnum", CondOp::InList, "0034,0100")], GroupJoin::And, true)), vec!["0034", "0100"]);
    // equality on a text column is text equality too
    assert_eq!(rows(&src, &d, group(vec![cond("Actnum", CondOp::Equals, "12")], GroupJoin::And, true)), vec!["12"]);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn t1_text_operators_and_blanks() {
    let d = dir("text");
    let src = source(&d);
    let r = |c: FilterCondition| rows(&src, &d, group(vec![c], GroupJoin::And, true));
    assert_eq!(r(cond("Remarks", CondOp::StartsWith, "Overdue")), vec!["0012"], "case sensitive, as the old filter");
    assert_eq!(r(cond("Remarks", CondOp::EndsWith, "loan")), vec!["0012"]);
    assert_eq!(r(cond("Remarks", CondOp::EndsWith, "(rescheduled)")), vec!["0100"]);
    assert_eq!(r(cond("Remarks", CondOp::Contains, "overdue")), vec!["0100"]);
    // blank = null or only spaces
    assert_eq!(r(cond("Remarks", CondOp::IsBlank, "")), vec!["0034", "12"]);
    assert_eq!(r(cond("Remarks", CondOp::IsNotBlank, "")), vec!["0012", "0099", "0100"]);
    assert_eq!(r(cond("Balance", CondOp::IsBlank, "")), vec!["0099"]);
    assert_eq!(r(cond("Opndat", CondOp::IsBlank, "")), vec!["0100"]);
    // a column that is not there
    let err = compile_tab_lazy(&src, &[group(vec![cond("Nope", CondOp::Equals, "1")], GroupJoin::And, true)], &d)
        .and_then(|lf| Ok(lf.collect()?))
        .err()
        .unwrap();
    assert!(format!("{err:#}").contains("'Nope'"), "{err:#}");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn t1_a_v12_macro_with_filter_rows_loads_and_gives_the_same_rows() {
    let d = dir("macro");
    let src = source(&d);
    // As written by v1.2: a FilterRows step, no FilterGroup anywhere.
    let json = r#"{
        "name": "old", "description": "", "created_at": "2026-01-01 10:00",
        "enforce_balance_control": false, "control_column": null, "optional_ingestion_schema": null,
        "steps": [ { "FilterRows": { "column": "Brancd", "operator": "Eq", "value": "DHK", "keep_matching": true } } ]
    }"#;
    let p = d.join("old.mpq.json");
    std::fs::write(&p, json).unwrap();
    let recipe = load_macro_from_file(&p).unwrap();
    let old_rows = rows(&src, &d, recipe.steps[0].clone());
    assert_eq!(old_rows, vec!["0012", "0034", "0100"]);
    // shown as a one-condition group
    let (conds, join, keep) = recipe.steps[0].as_filter_group().unwrap();
    assert_eq!(conds, vec![cond("Brancd", CondOp::Equals, "DHK")]);
    assert_eq!((join, keep), (GroupJoin::And, true));
    // and the equivalent group gives the same rows
    assert_eq!(rows(&src, &d, group(conds, join, keep)), old_rows);
    // other steps are not filters
    assert!(TransformStep::RenameColumn { old_name: "a".into(), new_name: "b".into() }.as_filter_group().is_none());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn t1_filter_group_serialises_with_defaults_and_describes_itself() {
    // new fields have defaults: a bare step loads as "all of, keep"
    let s: TransformStep = serde_json::from_str(r#"{"FilterGroup":{"conditions":[{"column":"A","op":"Equals"}]}}"#).unwrap();
    match &s {
        TransformStep::FilterGroup { conditions, join, keep_matching } => {
            assert_eq!(conditions[0].value, "");
            assert_eq!((*join, *keep_matching), (GroupJoin::And, true));
        }
        other => panic!("{other:?}"),
    }
    let round: TransformStep = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(round.short_label(), s.short_label());
    // chip text
    assert_eq!(cond("Brancd", CondOp::Equals, " DHK ").describe(), "Brancd equals DHK");
    assert_eq!(cond("Remarks", CondOp::IsBlank, "").describe(), "Remarks is blank");
    let b = FilterCondition { column: "Balance".into(), op: CondOp::Between, value: "1000".into(), value2: "5000".into() };
    assert_eq!(b.describe(), "Balance between 1000 and 5000");
    assert_eq!(
        group(vec![cond("A", CondOp::Gte, "1"), cond("B", CondOp::Lt, "2")], GroupJoin::Or, false).short_label(),
        "Delete: A \u{2265} 1 or B < 2"
    );
}

#[test]
fn t1_python_export_translates_a_filter_group() {
    let steps = [group(
        vec![
            cond("Brancd", CondOp::Equals, "DHK"),
            FilterCondition { column: "Balance".into(), op: CondOp::Between, value: "1000".into(), value2: "5000".into() },
            cond("Remarks", CondOp::IsBlank, ""),
        ],
        GroupJoin::Or,
        true,
    )];
    let py = generate_python_polars_code(std::path::Path::new("x.parquet"), true, &IngestionConfig::default(), &steps, None).unwrap();
    assert!(py.contains("pl.col(\"Balance\").is_between(1000, 5000)"), "{py}");
    assert!(py.contains(") | ("), "{py}");
    assert!(py.contains(".fill_null(False)"), "{py}");
    assert!(python_untranslated_steps(&steps).is_empty());
}
