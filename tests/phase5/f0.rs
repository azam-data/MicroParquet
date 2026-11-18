//! Phase 5 F0: Phase 4 follow-ups.

use crate::common::*;
use micro_parquet::engine::{compile_tab_lazy, hoist_clean_line_breaks};
use micro_parquet::models::*;
use polars::prelude::*;

const MULTILINE: &str = "Actnum,Name,Remarks,Balance\n\
    A1,\"RAHIM\nUDDIN\",\"Paid in full\nby cheque\", 100\n\
    A2,KARIM,  plain  ,200\n\
    A3,SALMA,\"Line one\r\nLine two\",300\n\
    A4,JAMAL,\"  leading and trailing\n\n\n  \",400\n\
    A5,NASIR,fine,500\n";

/// The settings file of Phase 1–4 (written by the code of main): the box ticked, no "trim" field.
const OLD_SCHEMA_JSON: &str = r#"{"columns":[
    {"name":"Actnum","dtype":"Utf8","pii_mask":"None","sample_values":[]},
    {"name":"Name","dtype":"Utf8","pii_mask":"ClientNameHash","sample_values":[]},
    {"name":"Remarks","dtype":"Utf8","pii_mask":"None","sample_values":[]},
    {"name":"Balance","dtype":"Float64","pii_mask":"None","sample_values":[]}],
  "sanitize_linebreaks":true,"parse_accounting_negatives":true,"heal_broken_csv_lines":false,"pii_salt":"x"}"#;

fn old_config(sanitize: bool) -> IngestionConfig {
    let mut c: IngestionConfig = serde_json::from_str(OLD_SCHEMA_JSON).expect("the old schema file loads");
    c.sanitize_linebreaks = sanitize;
    c
}

fn csv_source(name: &str, config: IngestionConfig) -> (std::path::PathBuf, TabSource) {
    let d = dir(name);
    let p = d.join("multiline.csv");
    std::fs::write(&p, MULTILINE).expect("write the CSV");
    micro_parquet::engine::set_pii_key_path_override(Some(d.join("pii_key.bin")));
    (d, TabSource::SingleFile { path: p, is_parquet: false, config })
}

fn same(a: &[TransformStep], b: &[TransformStep]) -> bool {
    format!("{a:?}") == format!("{b:?}")
}

/// The step the Schema & Ingestion box makes.
fn clean_step() -> TransformStep {
    TransformStep::CleanLineBreaks { columns: Vec::new(), trim_all: true }
}

#[test]
fn f0_ticking_the_box_makes_the_clean_line_breaks_step_the_first_step() {
    // ticked, no steps yet: the step is added (with "Trim spaces on all text values") and the setting is cleared
    let mut config = IngestionConfig { sanitize_linebreaks: true, ..IngestionConfig::default() };
    let mut steps: Vec<TransformStep> = Vec::new();
    hoist_clean_line_breaks(&mut config, false, &mut steps);
    assert!(same(&steps, &vec![clean_step()]), "{steps:?}");
    assert!(!config.sanitize_linebreaks, "the step is the only thing that cleans now");

    // ticked with other steps: it goes in front of them, once
    let other = TransformStep::CleanLineBreaks { columns: vec!["Remarks".into()], trim_all: false };
    let mut config = IngestionConfig { sanitize_linebreaks: true, ..IngestionConfig::default() };
    let mut steps = vec![other.clone()];
    hoist_clean_line_breaks(&mut config, false, &mut steps);
    assert!(same(&steps, &vec![clean_step(), other.clone()]), "{steps:?}");
    config.sanitize_linebreaks = true;
    hoist_clean_line_breaks(&mut config, false, &mut steps);
    assert!(same(&steps, &vec![clean_step(), other.clone()]), "not added twice");

    // unticked: the box's first step is taken out, a step for chosen columns stays
    config.sanitize_linebreaks = false;
    hoist_clean_line_breaks(&mut config, false, &mut steps);
    assert!(same(&steps, &vec![other.clone()]), "{steps:?}");
    hoist_clean_line_breaks(&mut config, false, &mut steps);
    assert!(same(&steps, &vec![other]), "a step for chosen columns is not the box's");

    // a Transform-menu step for all columns (trim off) is not the box's: it stays when the box is unticked,
    // and the ticked box adds its own step in front
    let menu = TransformStep::CleanLineBreaks { columns: Vec::new(), trim_all: false };
    let mut config = IngestionConfig { sanitize_linebreaks: false, ..IngestionConfig::default() };
    let mut steps = vec![menu.clone()];
    hoist_clean_line_breaks(&mut config, false, &mut steps);
    assert!(same(&steps, &vec![menu.clone()]), "a menu step is left alone: {steps:?}");
    config.sanitize_linebreaks = true;
    hoist_clean_line_breaks(&mut config, false, &mut steps);
    assert!(same(&steps, &vec![clean_step(), menu]), "{steps:?}");

    // Parquet has no such box: nothing changes
    let mut config = IngestionConfig { sanitize_linebreaks: true, ..IngestionConfig::default() };
    let mut steps: Vec<TransformStep> = Vec::new();
    hoist_clean_line_breaks(&mut config, true, &mut steps);
    assert!(steps.is_empty() && config.sanitize_linebreaks);
}

#[test]
fn f0_an_old_schema_file_with_the_box_ticked_gives_the_pre_phase_5_values() {
    // Written by hand from what main did: line breaks to one space, then the spaces at both ends trimmed
    // on every text value (also values with no line break), before the mask and the typing.
    let remarks: Vec<Option<String>> = ["Paid in full by cheque", "plain", "Line one Line two", "leading and trailing", "fine"]
        .iter()
        .map(|s| Some(s.to_string()))
        .collect();

    // the old source: the box is ticked inside the saved settings and no step is recorded
    let (d, old) = csv_source("f0a", old_config(true));
    let old_out = compile_tab_lazy(&old, &[], &d).expect("compiles").collect().expect("collects");
    assert_eq!(strings(&old_out, "Remarks"), remarks);
    assert_eq!(strings(&old_out, "Actnum")[0].as_deref(), Some("A1"));
    assert_eq!(floats(&old_out, "Balance")[0], Some(100.0), "the leading space of ' 100' is typed away as before");
    let masked_trimmed = strings(&old_out, "Name");

    // the tab made from the dialog with the box ticked: the step is first and the setting is cleared
    let TabSource::SingleFile { path, is_parquet, mut config } = old.clone() else { panic!("single file") };
    let mut steps = Vec::new();
    hoist_clean_line_breaks(&mut config, is_parquet, &mut steps);
    let tab = TabSource::SingleFile { path, is_parquet, config };
    let new_out = compile_tab_lazy(&tab, &steps, &d).expect("compiles").collect().expect("collects");
    assert!(same(&steps, &vec![clean_step()]), "{steps:?}");
    assert_eq!(strings(&new_out, "Remarks"), remarks);
    assert!(new_out.equals_missing(&old_out), "same table, value for value, including the masked column");

    // "RAHIM<line break>UDDIN" is masked after the break became a space, so its token is the token of
    // "RAHIM UDDIN" (before Phase 5 the box cleaned before the mask)
    let (d2, plain) = csv_source("f0c", old_config(false));
    micro_parquet::engine::set_pii_key_path_override(Some(d.join("pii_key.bin"))); // the same install key
    let plain_out = compile_tab_lazy(&plain, &[], &d2).expect("compiles").collect().expect("collects");
    let spaced = std::fs::read_to_string(d2.join("multiline.csv")).expect("read").replace("RAHIM\nUDDIN", "RAHIM UDDIN");
    std::fs::write(d2.join("spaced.csv"), spaced).expect("write");
    let TabSource::SingleFile { config, .. } = &plain else { panic!("single file") };
    let spaced_src = TabSource::SingleFile { path: d2.join("spaced.csv"), is_parquet: false, config: config.clone() };
    let spaced_out = compile_tab_lazy(&spaced_src, &[], &d2).expect("compiles").collect().expect("collects");
    assert_eq!(masked_trimmed[0], strings(&spaced_out, "Name")[0], "the token of the cleaned name");
    assert_ne!(strings(&plain_out, "Name")[0], masked_trimmed[0], "unticked, the line break stays in what is masked");

    // unticked: nothing is cleaned and no step is recorded
    assert_eq!(strings(&plain_out, "Remarks")[1].as_deref(), Some("  plain  "));
    assert!(strings(&plain_out, "Remarks")[0].as_deref().unwrap_or("").contains('\n'));
}

#[test]
fn f0_trim_spaces_on_all_text_values_is_the_difference_between_the_menu_and_the_box() {
    let df = DataFrame::new(vec![Series::new("T".into(), &["  a  ", "b\n c ", "\td\t"])]).expect("frame");
    // from the Transform menu (option off): only the value with a line break is cleaned, and all of its ends trimmed
    let menu = run_steps("f0_menu", &df, &[TransformStep::CleanLineBreaks { columns: vec![], trim_all: false }]).expect("runs");
    assert_eq!(strings(&menu, "T"), vec![Some("  a  ".into()), Some("b  c".into()), Some("\td\t".into())]);
    // option on: every value loses the spaces at its ends (spaces only, as the old box did)
    let all = run_steps("f0_all", &df, &[TransformStep::CleanLineBreaks { columns: vec![], trim_all: true }]).expect("runs");
    assert_eq!(strings(&all, "T"), vec![Some("a".into()), Some("b  c".into()), Some("\td\t".into())]);
    // a macro saved before the option existed loads with it off
    let old: TransformStep = serde_json::from_str(r#"{"CleanLineBreaks":{"columns":["T"]}}"#).expect("loads");
    assert!(matches!(old, TransformStep::CleanLineBreaks { trim_all: false, .. }));
    let json = serde_json::to_string(&clean_step()).expect("json");
    let back: TransformStep = serde_json::from_str(&json).expect("loads");
    assert!(same(&[back], &[clean_step()]));
    // the macro's Python repeats the trim
    let py = micro_parquet::macros::generate_python_polars_code(std::path::Path::new("x.parquet"), true, &IngestionConfig::default(), &[clean_step()], None).expect("python");
    assert!(py.contains("strip_chars(\" \")"), "{py}");
}

#[test]
fn f0_delta_nii_blank_rate_error_names_the_column_of_the_account_numbers() {
    let n = 7;
    let mut accts: Vec<String> = (1..=n).map(|i| format!("A{i}")).collect();
    let mut rates: Vec<Option<f64>> = vec![None; n];
    accts.push("OK1".into());
    rates.push(Some(2.0));
    let rows = accts.len();
    let df = DataFrame::new(vec![
        Series::new("Account_No".into(), accts),
        Series::new("Balance".into(), vec![1_000_000.0; rows]),
        Series::new("Rate".into(), rates),
        Series::new("Side".into(), vec!["Liability"; rows]),
        dates("Matdat", &vec![""; rows]),
    ])
    .expect("book");
    let spec = micro_parquet::banking::RateShockSpec {
        bal_col: "Balance".into(),
        rate_col: "Rate".into(),
        mat_col: "Matdat".into(),
        cutoff_date: "2026-10-31".into(),
        shock_bps: -100.0,
        side_col: Some("Side".into()),
        reprice_col: None,
        beta_file: None,
        product_col: None,
        floor_pct: Some(0.0),
    };
    let e = format!("{:#}", micro_parquet::banking::apply_rate_shock_with(df.lazy(), &spec, &[]).err().expect("blank rates with a floor"));
    assert!(e.contains("7 row(s) have no current rate in 'Rate'"), "{e}");
    assert!(e.contains("first 5 values of Account_No: A1, A2, A3, A4, A5"), "{e}");
    assert!(!e.contains("A6") && !e.contains("OK1"), "{e}");
}
