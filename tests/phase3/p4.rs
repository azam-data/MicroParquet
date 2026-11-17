//! Phase 3 fixes, part 4: library-level checks (conditional column, rate profile).

use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::models::*;
use polars::prelude::*;
use std::path::{Path, PathBuf};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mp_p4_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn parquet(d: &Path, name: &str, mut df: DataFrame) -> TabSource {
    let p = d.join(format!("{name}.parquet"));
    ParquetWriter::new(std::fs::File::create(&p).unwrap()).finish(&mut df).unwrap();
    TabSource::SingleFile { path: p, is_parquet: true, config: IngestionConfig::default() }
}

fn run(src: &TabSource, steps: &[TransformStep], d: &Path) -> anyhow::Result<DataFrame> {
    Ok(compile_tab_lazy(src, steps, d)?.collect()?)
}

fn strings(df: &DataFrame, name: &str) -> Vec<Option<String>> {
    df.column(name).unwrap().str().unwrap().into_iter().map(|v| v.map(|s| s.to_string())).collect()
}

// -------------------------------------------------------------------- item 3: conditional column

fn rule(col: &str, op: FilterOp, rhs: &str, is_col: bool, out: &str) -> CondRule {
    CondRule { col_name: col.into(), op, compare_val: rhs.into(), output_val: out.into(), compare_is_col: is_col }
}

fn cond(rules: Vec<CondRule>, blank: BlankRule) -> TransformStep {
    TransformStep::ConditionalColumn { new_col: "Flag".into(), rules, fallback: "normal".into(), output_as_float: false, blank_result: blank }
}

fn loans(d: &Path) -> TabSource {
    parquet(
        d,
        "loans",
        DataFrame::new(vec![
            Series::new("Matamt".into(), &[Some(100.0f64), Some(500.0), Some(50.0), Some(70.0), None]).into(),
            Series::new("Balance".into(), &[Some(100.0f64), Some(400.0), Some(60.0), None, Some(10.0)]).into(),
            Series::new("Note".into(), &["a", "b", "c", "d", "e"]).into(),
        ])
        .unwrap(),
    )
}

#[test]
fn p4_3_a_rule_can_compare_with_another_column_and_a_blank_gives_the_blank_result() {
    let d = dir("cond");
    let src = loans(&d);
    // Matamt <= Balance -> abnormal, else normal; a blank on either side gives a blank cell
    let step = cond(vec![rule("Matamt", FilterOp::Lte, "Balance", true, "abnormal")], BlankRule::Result(String::new()));
    let df = run(&src, &[step], &d).unwrap();
    assert_eq!(
        strings(&df, "Flag"),
        vec![Some("abnormal".into()), Some("normal".into()), Some("abnormal".into()), None, None],
        "100<=100, 500<=400, 50<=60, blank Balance, blank Matamt"
    );
    // a value as the blank result
    let step = cond(vec![rule("Matamt", FilterOp::Lte, "Balance", true, "abnormal")], BlankRule::Result("check".into()));
    let df = run(&src, &[step], &d).unwrap();
    assert_eq!(strings(&df, "Flag")[3..], [Some("check".to_string()), Some("check".to_string())]);
    // steps saved before the choice existed: a blank never matches, the next rule or ELSE decides
    let step = cond(vec![rule("Matamt", FilterOp::Lte, "Balance", true, "abnormal")], BlankRule::FallThrough);
    let df = run(&src, &[step], &d).unwrap();
    assert_eq!(strings(&df, "Flag")[3..], [Some("normal".to_string()), Some("normal".to_string())]);
}

#[test]
fn p4_3_the_blank_result_is_looked_at_before_the_rule_and_also_works_against_a_value() {
    let d = dir("cond2");
    let src = loans(&d);
    // the first rule looks at Balance (blank in row 3), the second at Matamt (blank in row 4)
    let step = cond(
        vec![rule("Balance", FilterOp::Gt, "50", false, "big"), rule("Matamt", FilterOp::Lt, "60", false, "small")],
        BlankRule::Result("n/a".into()),
    );
    let df = run(&src, &[step], &d).unwrap();
    // row 0: Balance 100 > 50 big; row 1 big; row 2 big; row 3: Balance blank -> n/a; row 4: Balance 10 not > 50, Matamt blank -> n/a
    assert_eq!(
        strings(&df, "Flag"),
        vec![Some("big".into()), Some("big".into()), Some("big".into()), Some("n/a".into()), Some("n/a".into())]
    );
    // "is blank" asks the question itself: it is not turned into the blank result
    let step = cond(vec![rule("Balance", FilterOp::IsNull, "", false, "empty")], BlankRule::Result("n/a".into()));
    let df = run(&src, &[step], &d).unwrap();
    assert_eq!(strings(&df, "Flag")[3], Some("empty".to_string()));
    assert_eq!(strings(&df, "Flag")[0], Some("normal".to_string()));
}

#[test]
fn p4_3_comparing_a_number_with_text_is_an_error_naming_both_columns_and_types() {
    let d = dir("cond3");
    let src = loans(&d);
    for op in [FilterOp::Lte, FilterOp::Eq] {
        let err = run(&src, &[cond(vec![rule("Matamt", op, "Note", true, "x")], BlankRule::default())], &d).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("'Matamt'") && msg.contains("'Note'") && msg.contains("number") && msg.contains("text"), "{msg}");
    }
    // a column that is not there is named too
    let err = run(&src, &[cond(vec![rule("Matamt", FilterOp::Lte, "Nope", true, "x")], BlankRule::default())], &d).unwrap_err();
    assert!(format!("{err:#}").contains("'Nope'"), "{err:#}");
    // text with text and date with date are fine
    let df = DataFrame::new(vec![
        Series::new("A".into(), &["x", "b"]).into(),
        Series::new("B".into(), &["x", "c"]).into(),
        Series::new("D1".into(), &[Some(19000i32), Some(19100)]).cast(&DataType::Date).unwrap().into(),
        Series::new("D2".into(), &[Some(19000i32), Some(19000)]).cast(&DataType::Date).unwrap().into(),
    ])
    .unwrap();
    let src = parquet(&d, "types", df);
    let out = run(&src, &[cond(vec![rule("A", FilterOp::Eq, "B", true, "same")], BlankRule::default())], &d).unwrap();
    assert_eq!(strings(&out, "Flag"), vec![Some("same".into()), Some("normal".into())]);
    let out = run(&src, &[cond(vec![rule("D1", FilterOp::Gt, "D2", true, "later")], BlankRule::default())], &d).unwrap();
    assert_eq!(strings(&out, "Flag"), vec![Some("normal".into()), Some("later".into())]);
    // a date against text is an error
    let err = run(&src, &[cond(vec![rule("D1", FilterOp::Gt, "A", true, "x")], BlankRule::default())], &d).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("'D1'") && msg.contains("'A'") && msg.contains("date") && msg.contains("text"), "{msg}");
}

#[test]
fn p4_3_old_macros_load_with_the_old_meaning() {
    // a step saved before compare_is_col and blank_result existed
    let json = r#"{"ConditionalColumn":{"new_col":"F","rules":[{"col_name":"Balance","op":"Gt","compare_val":"50","output_val":"big"}],"fallback":"small","output_as_float":false}}"#;
    let step: TransformStep = serde_json::from_str(json).expect("old step loads");
    match &step {
        TransformStep::ConditionalColumn { rules, blank_result, .. } => {
            assert!(!rules[0].compare_is_col);
            assert_eq!(*blank_result, BlankRule::FallThrough);
        }
        other => panic!("{other:?}"),
    }
    // and keeps its result: the blank Balance (row 3) is "small", as before
    let d = dir("cond4");
    let df = run(&loans(&d), &[step], &d).unwrap();
    assert_eq!(strings(&df, "F")[3], Some("small".to_string()));
    // a new step survives a save and a load
    let new = cond(vec![rule("Matamt", FilterOp::Lte, "Balance", true, "abnormal")], BlankRule::Result(String::new()));
    let back: TransformStep = serde_json::from_str(&serde_json::to_string(&new).unwrap()).unwrap();
    assert_eq!(format!("{new:?}"), format!("{back:?}"));
}

#[test]
fn p4_3_the_python_export_has_the_same_column_comparison_and_blank_result() {
    let step = cond(vec![rule("Matamt", FilterOp::Lte, "Balance", true, "abnormal")], BlankRule::Result(String::new()));
    let py = micro_parquet::macros::generate_python_polars_code(Path::new("file.parquet"), true, &IngestionConfig::default(), &[step], None).expect("code");
    assert!(py.contains("pl.col(\"Matamt\") <= pl.col(\"Balance\")"), "{py}");
    assert!(py.contains("pl.lit(None)") && py.contains("is_null()"), "{py}");
}
