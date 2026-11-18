//! Phase 5 F0: Phase 4 follow-ups.

use crate::common::*;
use micro_parquet::engine::{compile_tab_lazy, hoist_clean_line_breaks};
use micro_parquet::models::*;
use polars::prelude::*;

const MULTILINE: &str = "Actnum,Remarks,Balance\n\
    A1,\"Paid in full\nby cheque\",100\n\
    A2,plain,200\n\
    A3,\"Line one\r\nLine two\",300\n\
    A4,\"  leading and trailing\n\n\n  \",400\n\
    A5,fine,500\n";

fn csv_source(name: &str, sanitize: bool) -> (std::path::PathBuf, TabSource) {
    let d = dir(name);
    let p = d.join("multiline.csv");
    std::fs::write(&p, MULTILINE).expect("write the CSV");
    let config = IngestionConfig { sanitize_linebreaks: sanitize, ..IngestionConfig::default() };
    (d, TabSource::SingleFile { path: p, is_parquet: false, config })
}

fn same(a: &[TransformStep], b: &[TransformStep]) -> bool {
    format!("{a:?}") == format!("{b:?}")
}

fn clean_step() -> TransformStep {
    TransformStep::CleanLineBreaks { columns: Vec::new() }
}

#[test]
fn f0_ticking_the_box_makes_the_clean_line_breaks_step_the_first_step() {
    // ticked, no steps yet: the step is added and the setting is cleared
    let mut config = IngestionConfig { sanitize_linebreaks: true, ..IngestionConfig::default() };
    let mut steps: Vec<TransformStep> = Vec::new();
    hoist_clean_line_breaks(&mut config, false, &mut steps);
    assert!(same(&steps, &vec![clean_step()]), "{steps:?}");
    assert!(!config.sanitize_linebreaks, "the step is the only thing that cleans now");

    // ticked with other steps: it goes in front of them, once
    let other = TransformStep::CleanLineBreaks { columns: vec!["Remarks".into()] };
    let mut config = IngestionConfig { sanitize_linebreaks: true, ..IngestionConfig::default() };
    let mut steps = vec![other.clone()];
    hoist_clean_line_breaks(&mut config, false, &mut steps);
    assert!(same(&steps, &vec![clean_step(), other.clone()]), "{steps:?}");
    config.sanitize_linebreaks = true;
    hoist_clean_line_breaks(&mut config, false, &mut steps);
    assert!(same(&steps, &vec![clean_step(), other.clone()]), "not added twice");

    // unticked: a first all-columns step is taken out, a step for chosen columns stays
    config.sanitize_linebreaks = false;
    hoist_clean_line_breaks(&mut config, false, &mut steps);
    assert!(same(&steps, &vec![other.clone()]), "{steps:?}");
    hoist_clean_line_breaks(&mut config, false, &mut steps);
    assert!(same(&steps, &vec![other]), "a step for chosen columns is not the box's");

    // Parquet has no such box: nothing changes
    let mut config = IngestionConfig { sanitize_linebreaks: true, ..IngestionConfig::default() };
    let mut steps: Vec<TransformStep> = Vec::new();
    hoist_clean_line_breaks(&mut config, true, &mut steps);
    assert!(steps.is_empty() && config.sanitize_linebreaks);
}

#[test]
fn f0_the_box_and_the_step_give_the_same_values_as_the_old_setting() {
    let want: Vec<Option<String>> = ["Paid in full by cheque", "plain", "Line one Line two", "leading and trailing", "fine"]
        .iter()
        .map(|s| Some(s.to_string()))
        .collect();

    // an old schema file or macro: the box is ticked inside the saved settings, no step recorded
    let (d, old) = csv_source("f0a", true);
    let old_out = compile_tab_lazy(&old, &[], &d).expect("compiles").collect().expect("collects");
    assert_eq!(strings(&old_out, "Remarks"), want, "an old source with the box ticked still cleans");

    // the tab made from the dialog: the step is recorded first and the setting is cleared
    let TabSource::SingleFile { path, is_parquet, mut config } = old.clone() else { panic!("single file") };
    let mut steps = Vec::new();
    hoist_clean_line_breaks(&mut config, is_parquet, &mut steps);
    let tab = TabSource::SingleFile { path, is_parquet, config };
    let new_out = compile_tab_lazy(&tab, &steps, &d).expect("compiles").collect().expect("collects");
    assert!(same(&steps, &vec![clean_step()]), "{steps:?}");
    assert_eq!(strings(&new_out, "Remarks"), want);
    assert!(new_out.equals_missing(&old_out), "same table, value for value");

    // unticked: nothing is cleaned and no step is recorded
    let (d2, off) = csv_source("f0b", false);
    let off_out = compile_tab_lazy(&off, &[], &d2).expect("compiles").collect().expect("collects");
    assert!(strings(&off_out, "Remarks")[0].as_deref().unwrap_or("").contains('\n'));
}

#[test]
fn f0_an_old_schema_file_with_the_box_ticked_still_loads() {
    // the saved settings of Phase 1–4, written by the code of main
    let json = r#"{"columns":[],"sanitize_linebreaks":true,"parse_accounting_negatives":true,"heal_broken_csv_lines":false,"pii_salt":"x"}"#;
    let cfg: IngestionConfig = serde_json::from_str(json).expect("loads");
    assert!(cfg.sanitize_linebreaks);
    // and a macro's step list with the new first step round-trips
    let steps = vec![clean_step()];
    let back: Vec<TransformStep> = serde_json::from_str(&serde_json::to_string(&steps).expect("json")).expect("loads");
    assert!(same(&back, &steps));
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
