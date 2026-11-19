//! U1: "Rename to" in Schema & Ingestion, saved schemas that match a file by its columns.

use crate::common::*;
use micro_parquet::engine::*;
use micro_parquet::jobs::JobCtx;
use micro_parquet::models::*;
use micro_parquet::schema_templates::*;
use micro_parquet::tab_jobs::*;
use polars::prelude::*;
use std::path::{Path, PathBuf};

fn csv(d: &Path, name: &str, body: &str) -> PathBuf {
    let p = d.join(name);
    std::fs::write(&p, body).expect("write the CSV");
    p
}

const FILE_A: &str = "ACNO,CRIRTE,BAL\n1001,9.5,100.00\n1002,12.0,250.50\n1003,7.25,80.00\n";
const FILE_B: &str = "BAL,ACNO,CRIRTE\n300.00,2001,11.0\n40.00,2002,6.5\n";

/// The schema of FILE_A with the renames of the acceptance item.
fn renamed_schema(p: &Path) -> IngestionConfig {
    let mut cfg = auto_detect_smart_schema(p).expect("detected");
    for c in cfg.columns.iter_mut() {
        c.rename_to = match c.name.as_str() {
            "ACNO" => {
                // an account number keeps its leading zeros: text
                c.dtype = ColDataType::Utf8;
                Some("Account_No".into())
            }
            "CRIRTE" => Some("Int_Rate".into()),
            "BAL" => Some("Balance".into()),
            _ => None,
        };
    }
    cfg
}

fn open(src: &TabSource, steps: &[TransformStep], d: &Path) -> DataFrame {
    compile_tab_lazy(src, steps, d).expect("compiles").collect().expect("runs")
}

fn single(p: &Path, config: IngestionConfig) -> TabSource {
    TabSource::SingleFile { path: p.to_path_buf(), is_parquet: false, config }
}

fn filter_rate_above_8() -> TransformStep {
    TransformStep::FilterRows { column: "Int_Rate".into(), operator: FilterOp::Gt, value: "8".into(), keep_matching: true }
}

#[test]
fn u1_renames_apply_after_typing_and_steps_use_the_new_names() {
    let d = dir("u1_rename");
    let a = csv(&d, "a.csv", FILE_A);
    let cfg = renamed_schema(&a);
    let df = open(&single(&a, cfg), &[], &d);
    let names: Vec<String> = df.get_column_names().iter().map(|s| s.to_string()).collect();
    assert_eq!(names, vec!["Account_No", "Int_Rate", "Balance"], "the column order is the file's");
    assert_eq!(df.column("Balance").expect("c").dtype(), &DataType::Float64, "typing happened before the rename");
    // a filter on the new name works
    let out = open(&single(&a, renamed_schema(&a)), &[filter_rate_above_8()], &d);
    assert_eq!(strings(&out, "Account_No"), vec![Some("1001".to_string()), Some("1002".to_string())]);
    // a blank "Rename to" keeps the file's name, and whitespace only is blank too
    let mut cfg = auto_detect_smart_schema(&a).expect("detected");
    cfg.columns[0].rename_to = Some("  ".into());
    cfg.columns[1].rename_to = Some(" Int_Rate ".into());
    let df = open(&single(&a, cfg), &[], &d);
    let names: Vec<String> = df.get_column_names().iter().map(|s| s.to_string()).collect();
    assert_eq!(names, vec!["ACNO", "Int_Rate", "BAL"], "the new name is trimmed");
}

#[test]
fn u1_a_saved_schema_matches_a_second_file_by_its_header_names_in_any_order() {
    let d = dir("u1_match");
    let schemas = d.join("schemas");
    std::fs::create_dir_all(&schemas).expect("folder");
    let a = csv(&d, "a.csv", FILE_A);
    let mut template = renamed_schema(&a);
    template.auto_apply = false;
    std::fs::write(schemas.join("loans.schema.json"), serde_json::to_string_pretty(&template).expect("json")).expect("save");
    let templates = load_templates(&schemas);
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].name, "loans");

    // the second file: columns in another order
    let b = csv(&d, "b.csv", FILE_B);
    let detected = auto_detect_smart_schema(&b).expect("detected");
    let headers: Vec<String> = detected.columns.iter().map(|c| c.name.clone()).collect();
    let hits = matching_templates(&templates, &headers);
    assert_eq!(hits.len(), 1, "exactly one match");
    let applied = apply_template(&detected, &hits[0].config);
    assert_eq!(applied.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["BAL", "ACNO", "CRIRTE"], "the file's own order");
    let df = open(&single(&b, applied), &[filter_rate_above_8()], &d);
    assert_eq!(strings(&df, "Account_No"), vec![Some("2001".to_string())], "a filter on Int_Rate works");
    assert_eq!(df.get_column_names().iter().map(|s| s.to_string()).collect::<Vec<_>>(), vec!["Balance", "Account_No", "Int_Rate"]);

    // a file with one extra column does not match; nor does one with a column missing
    let extra = csv(&d, "extra.csv", "ACNO,CRIRTE,BAL,NOTE\n1,2,3,x\n");
    let h: Vec<String> = auto_detect_smart_schema(&extra).expect("d").columns.iter().map(|c| c.name.clone()).collect();
    assert!(matching_templates(&templates, &h).is_empty(), "one extra column: no match");
    let short = csv(&d, "short.csv", "ACNO,BAL\n1,3\n");
    let h: Vec<String> = auto_detect_smart_schema(&short).expect("d").columns.iter().map(|c| c.name.clone()).collect();
    assert!(matching_templates(&templates, &h).is_empty(), "one column missing: no match");
    // header names are compared trimmed and without case
    let loose = csv(&d, "loose.csv", " acno ,Crirte,bal\n1,2,3\n");
    let h: Vec<String> = auto_detect_smart_schema(&loose).expect("d").columns.iter().map(|c| c.name.clone()).collect();
    assert_eq!(matching_templates(&templates, &h).len(), 1, "trimmed, case-insensitive");
    // a second template with the same columns makes two matches
    std::fs::write(schemas.join("loans2.schema.json"), serde_json::to_string(&template).expect("json")).expect("save");
    assert_eq!(matching_templates(&load_templates(&schemas), &headers).len(), 2);
}

#[test]
fn u1_two_columns_with_the_same_final_name_are_refused_in_words() {
    let d = dir("u1_dup");
    let a = csv(&d, "a.csv", FILE_A);
    let mut cfg = renamed_schema(&a);
    cfg.columns[0].rename_to = Some("Balance".into());
    let problems = cfg.rename_problems();
    assert_eq!(problems.len(), 2, "both columns are marked: {problems:?}");
    assert!(problems[0].1.contains("\"Balance\" is also the name of \"BAL\""), "{}", problems[0].1);
    assert!(problems.iter().map(|(i, _)| *i).collect::<Vec<_>>() == vec![0, 2]);
    let err = compile_tab_lazy(&single(&a, cfg), &[], &d).err().expect("the load stops");
    assert!(format!("{err:#}").contains("also the name of"), "{err:#}");
    // a name that is empty after trimming (a file column with no name and no rename)
    let mut cfg = renamed_schema(&a);
    cfg.columns[1].name = "  ".into();
    cfg.columns[1].rename_to = None;
    assert!(cfg.rename_problems()[0].1.contains("has no name"));
    // a rename onto a name another column keeps is a clash too, even for an unlisted column
    let mut cfg = auto_detect_smart_schema(&a).expect("d");
    cfg.columns[0].rename_to = Some("CRIRTE".into());
    assert_eq!(cfg.rename_problems().len(), 2);
    // G9: the check is not case-sensitive: "balance" and "Balance" are one name for Excel and SQL
    let mut cfg = renamed_schema(&a);
    cfg.columns[0].rename_to = Some("  BALANCE ".into());
    assert_eq!(cfg.rename_problems().len(), 2, "{:?}", cfg.rename_problems());
}

#[test]
fn g9_a_damaged_schema_file_is_named_in_a_warning_and_the_good_ones_still_match() {
    let d = dir("g9_schemas");
    let a = csv(&d, "a.csv", FILE_A);
    let cfg = renamed_schema(&a);
    std::fs::write(d.join("good.schema.json"), serde_json::to_string_pretty(&cfg).expect("json")).expect("good");
    std::fs::write(d.join("broken.schema.json"), "{ not json").expect("broken");
    std::fs::write(d.join("notes.json"), "{ not a schema either").expect("other json");
    let (templates, damaged) = micro_parquet::schema_templates::load_templates_checked(&d);
    assert_eq!(templates.len(), 1);
    assert_eq!(damaged.len(), 1, "only a *.schema.json is reported: {damaged:?}");
    assert!(damaged[0].contains("broken.schema.json") && damaged[0].contains("damaged"), "{damaged:?}");
}

#[test]
fn u1_a_stack_of_both_files_with_the_schema_has_the_renamed_columns() {
    let d = dir("u1_stack");
    let a = csv(&d, "a.csv", FILE_A);
    let b = csv(&d, "b.csv", FILE_B);
    let shared = renamed_schema(&a);
    let (specs, notes) = stack_file_configs(&[a.clone(), b.clone()], Some(&shared)).expect("configs");
    assert!(notes.is_empty(), "{notes:?}");
    let src = TabSource::VerticalStack { files: specs, include_source_col: false };
    let df = open(&src, &[], &d);
    assert_eq!(df.get_column_names().iter().map(|s| s.to_string()).collect::<Vec<_>>(), vec!["Account_No", "Int_Rate", "Balance"]);
    assert_eq!(df.height(), 5);
    let acc: Vec<String> = strings(&df, "Account_No").into_iter().flatten().collect();
    assert_eq!(acc, vec!["1001", "1002", "1003", "2001", "2002"]);
    assert_eq!(src.file_name_of("Balance").as_deref(), Some("BAL"));
    assert_eq!(src.file_name_of("Account_No").as_deref(), Some("ACNO"));
}

#[test]
fn u1_the_python_export_has_the_rename_mapping_after_the_typing() {
    let d = dir("u1_py");
    let a = csv(&d, "a.csv", FILE_A);
    let cfg = renamed_schema(&a);
    let py = micro_parquet::macros::generate_python_polars_code(&a, false, &cfg, &[filter_rate_above_8()], None).expect("python");
    assert!(py.contains("lf = lf.rename({"), "{py}");
    for pair in ["\"ACNO\": \"Account_No\"", "\"CRIRTE\": \"Int_Rate\"", "\"BAL\": \"Balance\""] {
        assert!(py.contains(pair), "{pair}");
    }
    let (rename_at, steps_at, cast_at) = (py.find("2c. Rename columns").expect("rename"), py.find("# 3. Applied Transformation").expect("steps"), py.find("2b. Apply Ingestion Column Types").expect("types"));
    assert!(cast_at < rename_at && rename_at < steps_at, "typing, then the rename, then the steps");
    // no renames: no block
    let plain = micro_parquet::macros::generate_python_polars_code(&a, false, &auto_detect_smart_schema(&a).expect("d"), &[], None).expect("python");
    assert!(!plain.contains("Rename columns"));
}

#[test]
fn u1_an_old_schema_file_without_rename_to_loads_unchanged() {
    let old = r#"{"columns":[{"name":"ACNO","dtype":"Utf8","pii_mask":"None","sample_values":["1001"]},{"name":"BAL","dtype":"Float64","pii_mask":"None","sample_values":["100.00"]}],
        "sanitize_linebreaks":true,"parse_accounting_negatives":true,"heal_broken_csv_lines":false,"pii_salt":"X"}"#;
    let cfg: IngestionConfig = serde_json::from_str(old).expect("an old schema loads");
    assert!(cfg.columns.iter().all(|c| c.rename_to.is_none()) && !cfg.auto_apply);
    assert!(cfg.rename_pairs().is_empty() && cfg.rename_problems().is_empty());
    assert_eq!(cfg.final_names(), vec!["ACNO", "BAL"]);
    // and it opens a file as before
    let d = dir("u1_old");
    let a = csv(&d, "a.csv", FILE_A);
    let mut cfg = cfg;
    cfg.columns.push(ColumnSchemaConfig { name: "CRIRTE".into(), dtype: ColDataType::Float64, ..Default::default() });
    let df = open(&single(&a, cfg), &[], &d);
    assert_eq!(df.get_column_names().iter().map(|s| s.to_string()).collect::<Vec<_>>(), vec!["ACNO", "CRIRTE", "BAL"], "the file's order");
}

#[test]
fn u1_changing_a_rename_back_warns_naming_the_step_that_would_break() {
    let d = dir("u1_warn");
    let a = csv(&d, "a.csv", FILE_A);
    let with = single(&a, renamed_schema(&a));
    let steps = vec![filter_rate_above_8()];
    // the filter on Int_Rate works today
    assert!(validate_steps(&with, &steps, &d).expect("validated").is_empty());
    // the owner clears the rename of CRIRTE
    let mut back = renamed_schema(&a);
    back.columns[1].rename_to = None;
    let r = check_schema_for_tab(
        SchemaRequest { tab_id: 1, new_source: single(&a, back), steps: steps.clone(), temp_dir: d.clone(), already_broken: vec![] },
        &JobCtx::inert(),
    )
    .expect("the check runs");
    assert_eq!(r.new_breaks.len(), 1, "{:?}", r.new_breaks);
    assert_eq!(r.new_breaks[0].0, 0, "the filter is step 1");
    assert!(r.new_breaks[0].1.contains("Int_Rate"), "the reason names the column: {}", r.new_breaks[0].1);
    // a step that was already broken is not reported again
    let r = check_schema_for_tab(
        SchemaRequest { tab_id: 1, new_source: single(&a, auto_detect_smart_schema(&a).expect("d")), steps: steps.clone(), temp_dir: d.clone(), already_broken: vec![0] },
        &JobCtx::inert(),
    )
    .expect("the check runs");
    assert!(r.new_breaks.is_empty());
    // a rename that breaks nothing reports nothing
    let r = check_schema_for_tab(
        SchemaRequest { tab_id: 1, new_source: with, steps, temp_dir: d.clone(), already_broken: vec![] },
        &JobCtx::inert(),
    )
    .expect("the check runs");
    assert!(r.new_breaks.is_empty());
}

#[test]
fn u1_parse_failures_masks_and_parquet_use_the_final_names() {
    let d = dir("u1_names");
    let a = csv(&d, "bad.csv", "ACNO,BAL\n1001,12x\n1002,5\n");
    let mut cfg = auto_detect_smart_schema(&a).expect("d");
    cfg.columns[1].dtype = ColDataType::Float64;
    cfg.columns[1].rename_to = Some("Balance".into());
    cfg.columns[0].dtype = ColDataType::Utf8;
    cfg.columns[0].pii_mask = PiiMaskMode::AccountNumber;
    cfg.columns[0].rename_to = Some("Account".into());
    let src = single(&a, cfg);
    let failures = source_parse_failures(&src, &d).expect("failures");
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].column, "Balance", "the failure is listed under the name the owner sees");
    let mut masked = Vec::new();
    collect_masked_columns(&src, &mut masked);
    assert_eq!(masked, vec!["Account".to_string()]);
    // Parquet: the same rename
    let df = DataFrame::new(vec![Series::new("ACNO".into(), &["a", "b"]), Series::new("BAL".into(), &[1.0, 2.0])]).expect("frame");
    let p = parquet(&d, "p", &df);
    let TabSource::SingleFile { path, .. } = &p else { panic!("single") };
    let mut pc = auto_detect_parquet_schema(path).expect("parquet schema");
    pc.columns[1].rename_to = Some("Balance".into());
    let out = open(&TabSource::SingleFile { path: path.clone(), is_parquet: true, config: pc }, &[], &d);
    assert_eq!(out.get_column_names().iter().map(|s| s.to_string()).collect::<Vec<_>>(), vec!["ACNO", "Balance"]);
}

#[test]
fn u1_a_swap_of_two_names_is_a_valid_rename() {
    let d = dir("u1_swap");
    let a = csv(&d, "a.csv", "A,B\n1,x\n");
    let mut cfg = auto_detect_smart_schema(&a).expect("d");
    cfg.columns[0].rename_to = Some("B".into());
    cfg.columns[1].rename_to = Some("A".into());
    let df = open(&single(&a, cfg), &[], &d);
    assert_eq!(df.get_column_names().iter().map(|s| s.to_string()).collect::<Vec<_>>(), vec!["B", "A"]);
    assert_eq!(strings(&df, "A"), vec![Some("x".to_string())]);
}

#[test]
fn u1_a_batch_with_one_saved_schema_writes_the_renamed_columns() {
    let d = dir("u1_batch");
    let (input, output) = (d.join("in"), d.join("out"));
    std::fs::create_dir_all(&input).expect("in");
    let a = csv(&input, "a.csv", FILE_A);
    csv(&input, "b.csv", FILE_B);
    let template = renamed_schema(&a);
    let report = micro_parquet::batch::run_folder_mirror_batch_ctx(
        &input,
        &output,
        BatchDirection::CsvToParquet,
        BatchSchemaStrategy::MasterSchemaTemplate,
        Some(&template),
        ParquetDialect::ArchiveZstd,
        false,
        &d,
        &JobCtx::inert(),
    )
    .expect("the batch runs");
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    for f in ["a.parquet", "b.parquet"] {
        let lf = LazyFrame::scan_parquet(output.join(f), ScanArgsParquet::default()).expect("scan");
        let names: Vec<String> = lf.clone().collect().expect("read").get_column_names().iter().map(|s| s.to_string()).collect();
        assert!(names.contains(&"Account_No".to_string()) && names.contains(&"Int_Rate".to_string()) && names.contains(&"Balance".to_string()), "{f}: {names:?}");
    }
}
