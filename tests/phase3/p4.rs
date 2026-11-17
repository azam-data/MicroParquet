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

// ------------------------------------------------------------------------ item 6: rate profile

const EN: &str = "\u{2013}";

fn band(def: &RateBandDef, rate: Option<f64>) -> String {
    def.label_for(rate).unwrap()
}

#[test]
fn p4_6_default_bands_are_upper_inclusive_and_labelled_so_they_sort() {
    let d = RateBandDef::default_profile();
    for (rate, want) in [
        (Some(9.00), format!("09. 8%{EN}9%")),
        (Some(8.01), format!("09. 8%{EN}9%")),
        (Some(0.0), "00. 0%".to_string()),
        (Some(0.50), format!("01. 0%{EN}1%")),
        (Some(15.00), format!("15. 14%{EN}15%")),
        (Some(15.50), "> 15%".to_string()),
        (Some(-0.25), "Below 0%".to_string()),
        (None, "No rate".to_string()),
        (Some(f64::NAN), "No rate".to_string()),
        (Some(1.0), format!("01. 0%{EN}1%")),
        (Some(1.0000001), format!("02. 1%{EN}2%")),
    ] {
        assert_eq!(band(&d, rate), want, "rate {rate:?}");
    }
    // the numbered labels sort in band order
    let l = d.labels().unwrap();
    let mut all = vec![l.zero.clone()];
    all.extend(l.bands.clone());
    let mut sorted = all.clone();
    sorted.sort();
    assert_eq!(all, sorted);
    assert_eq!(l.bands.len(), 15);
}

#[test]
fn p4_6_custom_edges_and_the_lower_inclusive_rule() {
    let custom = RateBandDef::custom("c", vec![0.0, 5.0, 7.0, 9.0, 12.0]);
    assert_eq!(band(&custom, Some(8.25)), format!("03. 7%{EN}9%"));
    assert_eq!(band(&custom, Some(9.00)), format!("03. 7%{EN}9%"));
    assert_eq!(band(&custom, Some(12.5)), "> 12%");
    // unsorted, duplicated and spaced input gives the same bands
    let messy = RateBandDef::custom("m", vec![12.0, 5.0, 0.0, 9.0, 7.0, 7.0]);
    assert_eq!(messy.labels().unwrap(), custom.labels().unwrap());
    // edges that start above 0 get a first band from 0
    let from5 = RateBandDef::custom("f", vec![5.0, 7.0]);
    assert_eq!(band(&from5, Some(3.0)), format!("01. 0%{EN}5%"));
    assert_eq!(band(&from5, Some(6.0)), format!("02. 5%{EN}7%"));
    // [a, b)
    let mut lo = RateBandDef::default_profile();
    lo.lower_inclusive = true;
    assert_eq!(band(&lo, Some(9.00)), format!("10. 9%{EN}10%"));
    assert_eq!(band(&lo, Some(15.00)), "\u{2265} 15%");
    assert_eq!(band(&lo, Some(14.99)), format!("15. 14%{EN}15%"));
    assert_eq!(band(&lo, Some(0.0)), "00. 0%", "exactly 0 is its own band under either rule");
    assert_eq!(band(&lo, Some(0.5)), format!("01. 0%{EN}1%"));
    assert_eq!(band(&lo, Some(-0.25)), "Below 0%");
    // fractional steps: 0.1 * 3 is 0.3, not 0.30000000000000004
    let frac = RateBandDef::fixed("f", 0.0, 1.0, 0.1);
    assert_eq!(band(&frac, Some(0.3)), format!("03. 0.2%{EN}0.3%"));
}

#[test]
fn p4_6_bad_settings_are_errors_a_banker_can_act_on() {
    let msg = |d: RateBandDef| format!("{:#}", d.labels().unwrap_err());
    assert!(msg(RateBandDef::fixed("x", 0.0, 15.0, 0.0)).contains("step"));
    assert!(msg(RateBandDef::fixed("x", 5.0, 5.0, 1.0)).contains("end"));
    assert!(msg(RateBandDef::fixed("x", 0.0, 10.0, 3.0)).contains("whole number of steps"));
    assert!(msg(RateBandDef::fixed("x", 0.0, 1000.0, 0.01)).contains("too many"));
    assert!(msg(RateBandDef::custom("x", vec![])).contains("at least one"));
    assert!(msg(RateBandDef::custom("x", vec![0.0])).contains("at least one"));
    assert!(msg(RateBandDef::custom("x", vec![-1.0, 5.0])).contains("below 0"));
    let e = parse_rate_edges("0, 5, abc").unwrap_err();
    assert!(format!("{e:#}").contains("'abc'"));
    assert_eq!(parse_rate_edges("0, 5; 7 9%, 12").unwrap(), vec![0.0, 5.0, 7.0, 9.0, 12.0]);
}

fn rates(d: &Path, values: &[Option<f64>]) -> TabSource {
    parquet(d, "rates", DataFrame::new(vec![Series::new("Rate".into(), values).into(), Series::new("Name".into(), vec!["x"; values.len()]).into()]).unwrap())
}

#[test]
fn p4_6_the_step_gives_the_same_labels_as_the_pure_function_for_every_value() {
    let d = dir("rb");
    // every value a band edge or just beside one, for both boundary rules and a custom list
    let mut values: Vec<Option<f64>> = vec![None, Some(0.0), Some(-0.25), Some(-1e-9), Some(20.0), Some(99.99)];
    for i in 0..=62 {
        let x = i as f64 * 0.25;
        values.extend([Some(x), Some(x + 0.001), Some(x - 0.001)]);
    }
    let src = rates(&d, &values);
    let mut lo = RateBandDef::default_profile();
    lo.lower_inclusive = true;
    for def in [RateBandDef::default_profile(), lo, RateBandDef::custom("c", vec![0.0, 5.0, 7.0, 9.0, 12.0]), RateBandDef::fixed("half", 0.0, 10.0, 0.5)] {
        let step = TransformStep::RateBand { rate_col: "Rate".into(), band_def: def.clone(), new_col: "Rate_Band".into() };
        let df = run(&src, &[step], &d).unwrap();
        let got = strings(&df, "Rate_Band");
        assert_eq!(got.len(), values.len());
        for (v, g) in values.iter().zip(&got) {
            assert_eq!(g.as_deref(), Some(band(&def, *v).as_str()), "{} rate {v:?}", def.name);
        }
    }
}

#[test]
fn p4_6_a_rate_column_that_is_text_is_an_error_not_a_column_of_no_rate() {
    let d = dir("rb2");
    let src = parquet(&d, "txt", DataFrame::new(vec![Series::new("Rate".into(), &["9.5", "x"]).into()]).unwrap());
    let step = TransformStep::RateBand { rate_col: "Rate".into(), band_def: RateBandDef::default_profile(), new_col: "B".into() };
    let msg = format!("{:#}", run(&src, &[step], &d).unwrap_err());
    assert!(msg.contains("'Rate'") && msg.contains("not a number"), "{msg}");
    let step = TransformStep::RateBand { rate_col: "Nope".into(), band_def: RateBandDef::default_profile(), new_col: "B".into() };
    assert!(format!("{:#}", run(&src, &[step], &d).unwrap_err()).contains("'Nope'"));
    // a bad profile stops the step too
    let step = TransformStep::RateBand { rate_col: "Rate".into(), band_def: RateBandDef::fixed("x", 0.0, 10.0, 3.0), new_col: "B".into() };
    assert!(format!("{:#}", run(&rates(&d, &[Some(1.0)]), &[step], &d).unwrap_err()).contains("whole number"));
}

#[test]
fn p4_6_presets_are_saved_in_portable_data_and_come_back_and_the_step_survives_a_macro_file() {
    let d = dir("rb3");
    let file = d.join("rate_bands.json");
    let before = RateBandDef::load_all_presets(&file);
    assert!(before.len() >= 3 && before.iter().any(|p| p.name == "0% to 15% by 1%"));
    let mut mine = RateBandDef::custom("Deposit grid", vec![0.0, 3.0, 5.0, 6.5]);
    mine.lower_inclusive = true;
    RateBandDef::save_custom_preset(&file, mine.clone()).unwrap();
    let after = RateBandDef::load_all_presets(&file);
    assert_eq!(after.len(), before.len() + 1);
    assert_eq!(after.last(), Some(&mine));
    // saving again under the same name replaces it
    let mut changed = mine.clone();
    changed.mode = RateBandMode::Custom { edges: vec![0.0, 4.0] };
    RateBandDef::save_custom_preset(&file, changed.clone()).unwrap();
    let again = RateBandDef::load_all_presets(&file);
    assert_eq!(again.len(), before.len() + 1);
    assert_eq!(again.last(), Some(&changed));
    // an old preset file without the boundary rule loads as upper-inclusive
    std::fs::write(&file, r#"[{"name":"old","mode":{"Fixed":{"start_pct":0.0,"end_pct":4.0,"step_pct":2.0}}}]"#).unwrap();
    let old = RateBandDef::load_all_presets(&file);
    assert!(!old.last().unwrap().lower_inclusive);
    // the step in a macro file
    let step = TransformStep::RateBand { rate_col: "Rate".into(), band_def: mine, new_col: "Rate_Band".into() };
    let back: TransformStep = serde_json::from_str(&serde_json::to_string(&step).unwrap()).unwrap();
    assert_eq!(format!("{step:?}"), format!("{back:?}"));
    assert_eq!(step.title(), "Rate profile");
}

#[test]
fn p4_6_the_rate_profile_is_in_the_banking_menu_and_found_by_the_search() {
    use micro_parquet::commands::*;
    assert!(in_group(CommandGroup::Banking).any(|c| c.id == CommandId::RateProfile));
    assert!(toolbar_commands().contains(&CommandId::RateProfile));
    let entries: Vec<Entry> = COMMANDS.iter().map(|c| Entry { label: c.title, words: c.words, recent: None }).collect();
    for q in ["rate profile", "rate band", "rate bands"] {
        let r = rank(q, &entries);
        assert_eq!(r.first().map(|i| COMMANDS[*i].id), Some(CommandId::RateProfile), "query '{q}': {:?}", r.iter().take(3).map(|i| COMMANDS[*i].title).collect::<Vec<_>>());
    }
}

#[test]
fn p4_6_the_python_export_has_the_same_bands() {
    let step = TransformStep::RateBand { rate_col: "Rate".into(), band_def: RateBandDef::fixed("x", 0.0, 3.0, 1.0), new_col: "Rate_Band".into() };
    let py = micro_parquet::macros::generate_python_polars_code(Path::new("f.parquet"), true, &IngestionConfig::default(), &[step], None).unwrap();
    for want in ["pl.lit(\"No rate\")", "pl.lit(\"00. 0%\")", "pl.lit(\"Below 0%\")", "_r <= 1.0", "pl.lit(\"01. 0%\\u2013 1%\")".replace("\\u2013 ", "\\u2013").as_str(), "pl.lit(\"> 3%\")", ".alias(\"Rate_Band\")"] {
        assert!(py.contains(want), "{want} not in:\n{py}");
    }
}
