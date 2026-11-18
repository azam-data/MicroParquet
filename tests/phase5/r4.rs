//! Phase 5 R4: loan classification from the overdue period (BRPD Circular No. 15 of 27 November 2024).

use crate::common::*;
use chrono::NaiveDate;
use micro_parquet::banking::*;
use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::models::*;
use micro_parquet::overdue::*;
use micro_parquet::rules::{set_data_dir_override, SHIPPED_RULES};
use polars::prelude::*;

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("a test date")
}

fn step(since: Option<&str>, dpd: Option<&str>, over: Option<&str>, cat: Option<&str>, cutoff: &str) -> TransformStep {
    TransformStep::ClassifyOverdue {
        category_col: cat.map(str::to_string),
        fixed_category: if cat.is_some() { String::new() } else { "Continuous".to_string() },
        since_col: since.map(str::to_string),
        dpd_col: dpd.map(str::to_string),
        override_col: over.map(str::to_string),
        overdue_amount_col: None,
        cutoff_date: cutoff.to_string(),
    }
}

/// Runs the step on a table in a fresh data folder (the rule file and the maps are written there).
fn run(name: &str, df: &DataFrame, s: &TransformStep) -> anyhow::Result<DataFrame> {
    set_data_dir_override(Some(dir(&format!("{name}_data"))));
    let out = run_steps(name, df, std::slice::from_ref(s));
    set_data_dir_override(None);
    out
}

fn acct(n: usize) -> Series {
    Series::new("Account_No".into(), (1..=n).map(|i| format!("A{i}")).collect::<Vec<_>>())
}

#[test]
fn r4_the_acceptance_dates_give_the_grades_at_the_cutoff_2026_10_31() {
    let cases = [
        ("", "STD-0", 0),
        ("2026-10-15", "STD-1", 0),
        ("2026-09-30", "STD-2", 1),
        ("2026-08-31", "SMA", 2),
        ("2026-08-01", "SMA", 2),
        ("2026-07-31", "SS", 3),
        ("2026-04-30", "DF", 6),
        ("2025-10-31", "B/L", 12),
        ("2026-10-31", "STD-0", 0), // overdue since the cutoff itself: no day has passed
        ("2026-10-30", "STD-1", 0),
    ];
    let df = DataFrame::new(vec![acct(cases.len()), dates("Overdue_Since", &cases.iter().map(|c| c.0).collect::<Vec<_>>())]).expect("book");
    let out = run("r4a", &df, &step(Some("Overdue_Since"), None, None, None, "2026-10-31")).expect("classifies");
    assert_eq!(strings(&out, "CL_Objective"), cases.iter().map(|c| Some(c.1.to_string())).collect::<Vec<_>>());
    assert_eq!(strings(&out, "CL_Final"), strings(&out, "CL_Objective"), "no override: the same");
    assert_eq!(floats(&out, "Overdue_Months"), cases.iter().map(|c| Some(c.2 as f64)).collect::<Vec<_>>());
    assert!(strings(&out, "CL_Rule_File").iter().all(|f| f.as_deref() == Some("cl_overdue/2025-04-01.json")));
    // the input columns are kept in front of the four new ones
    let names: Vec<String> = out.get_column_names().iter().map(|n| n.to_string()).collect();
    assert_eq!(names, vec!["Account_No", "Overdue_Since", "Overdue_Months", "CL_Objective", "CL_Final", "CL_Rule_File"]);
}

#[test]
fn r4_adding_months_clamps_to_the_month_end() {
    // 31 Jan + 1 month is 28 Feb: at the cutoff 2026-02-28 one complete month has passed
    assert_eq!(complete_months(d("2026-01-31"), d("2026-02-28")), Some(1));
    assert_eq!(complete_months(d("2026-01-31"), d("2026-02-27")), Some(0));
    assert_eq!(complete_months(d("2028-01-31"), d("2028-02-29")), Some(1), "29 Feb in a leap year");
    assert_eq!(complete_months(d("2028-01-31"), d("2028-02-28")), Some(0));
    assert_eq!(complete_months(d("2026-08-31"), d("2026-10-30")), Some(1));
    assert_eq!(complete_months(d("2026-08-31"), d("2026-10-31")), Some(2));
    assert_eq!(complete_months(d("2025-10-31"), d("2026-10-31")), Some(12));
    assert_eq!(complete_months(d("2025-10-31"), d("2026-10-30")), Some(11));
    assert_eq!(complete_months(d("2026-11-01"), d("2026-10-31")), None, "since after the cutoff");
    let df = DataFrame::new(vec![acct(1), dates("Since", &["2026-01-31"])]).expect("book");
    let out = run("r4b", &df, &step(Some("Since"), None, None, None, "2026-02-28")).expect("classifies");
    assert_eq!(strings(&out, "CL_Final"), vec![Some("STD-2".to_string())]);
}

#[test]
fn r4_days_past_due_become_the_overdue_since_date() {
    // DPD 92 at the cutoff 2026-10-31 is overdue since 2026-07-31: three complete months, SS
    let dpd = [Some(92.0), Some(0.0), Some(15.0), None, Some(400.0), Some(31.0)];
    let df = DataFrame::new(vec![acct(6), Series::new("DPD".into(), dpd.to_vec())]).expect("book");
    let out = run("r4c", &df, &step(None, Some("DPD"), None, None, "2026-10-31")).expect("classifies");
    assert_eq!(strings(&out, "CL_Final"), ["SS", "STD-0", "STD-1", "STD-0", "B/L", "STD-2"].iter().map(|s| Some(s.to_string())).collect::<Vec<_>>());
    assert_eq!(floats(&out, "Overdue_Months")[0], Some(3.0));
    // an integer column works too
    let df = DataFrame::new(vec![acct(1), Series::new("DPD".into(), &[92i64])]).expect("book");
    let out = run("r4c2", &df, &step(None, Some("DPD"), None, None, "2026-10-31")).expect("classifies");
    assert_eq!(strings(&out, "CL_Final"), vec![Some("SS".to_string())]);
}

#[test]
fn r4_the_worse_of_the_objective_and_the_override_grade_wins_and_both_counts_are_said() {
    // objective STD-2 with override SS -> SS; objective SS with override STD-0 -> SS (ignored)
    let df = DataFrame::new(vec![
        acct(5),
        dates("Since", &["2026-09-30", "2026-07-31", "2026-09-30", "2026-07-31", ""]),
        Series::new("Subjective".into(), &[Some("SS"), Some("STD-0"), None, Some("sub-standard"), Some("DF")]),
    ])
    .expect("book");
    let s = step(Some("Since"), None, Some("Subjective"), None, "2026-10-31");
    let out = run("r4d", &df, &s).expect("classifies");
    assert_eq!(strings(&out, "CL_Objective"), ["STD-2", "SS", "STD-2", "SS", "STD-0"].iter().map(|x| Some(x.to_string())).collect::<Vec<_>>());
    assert_eq!(strings(&out, "CL_Final"), ["SS", "SS", "STD-2", "SS", "DF"].iter().map(|x| Some(x.to_string())).collect::<Vec<_>>());
    // two overrides were worse (rows 1 and 5), one was better and ignored (row 2); row 4 equals its objective; row 3 has none
    set_data_dir_override(Some(dir("r4d_notes")));
    let lf = df.clone().lazy();
    let (_, stats) = classify_lazy(lf, &spec_of(&s).expect("spec")).expect("classifies");
    set_data_dir_override(None);
    assert_eq!((stats.override_worse, stats.override_ignored, stats.no_overdue_date), (2, 1, 1), "{stats:?}");
    let note = stats.note("Classify by overdue", true);
    assert!(note.contains("2 override(s) were worse than the overdue grade and were used") && note.contains("1 override(s) were better and were ignored"), "{note}");
    // an override code that is neither a sub-grade nor in cl_code_map.json is an error naming the code
    let bad = DataFrame::new(vec![acct(2), dates("Since", &["", ""]), Series::new("Subjective".into(), &["XYZ", "SS"])]).expect("book");
    let e = format!("{:#}", run("r4d2", &bad, &s).err().expect("unmapped override"));
    assert!(e.contains("Subjective") && e.contains("'XYZ'") && e.contains("cl_code_map.json"), "{e}");
}

#[test]
fn r4_the_loan_category_goes_through_a_code_map_and_an_unmapped_code_stops_the_run() {
    let df = DataFrame::new(vec![acct(3), Series::new("Loan_Type".into(), &["CONTINUOUS", " fixed term ", "Demand"]), dates("Since", &["2026-07-31"; 3])]).expect("book");
    let s = step(Some("Since"), None, None, Some("Loan_Type"), "2026-10-31");
    let out = run("r4e", &df, &s).expect("the shipped map knows the four names");
    assert_eq!(strings(&out, "CL_Final"), vec![Some("SS".to_string()); 3]);
    // unmapped codes: the count, the codes and the file to add them to
    let bad = DataFrame::new(vec![acct(4), Series::new("Loan_Type".into(), &[Some("TL"), Some("CONTINUOUS"), None, Some("TL")]), dates("Since", &["2026-07-31"; 4])]).expect("book");
    let e = format!("{:#}", run("r4e2", &bad, &s).err().expect("unmapped category"));
    assert!(e.contains("Loan_Type") && e.contains("'TL'") && e.contains("(blank)") && e.contains("loan_category_map.json") && e.contains("3 row(s)"), "{e}");
    // the map is a file the owner edits: a code added to it is understood
    let data = dir("r4e3_data");
    set_data_dir_override(Some(data.clone()));
    let tmp = dir("r4e3");
    let map = data.join("loan_category_map.json");
    std::fs::write(&map, "{\n  \"defaults_version\": 1,\n  \"TL\": \"Fixed term\",\n  \"CONTINUOUS\": \"Continuous\"\n}\n").expect("write the map");
    let src = parquet(&tmp, "in", &DataFrame::new(vec![acct(2), Series::new("Loan_Type".into(), &["TL", "CONTINUOUS"]), dates("Since", &["2026-07-31"; 2])]).expect("book"));
    let out = compile_tab_lazy(&src, &[s.clone()], &tmp).and_then(|lf| Ok(lf.collect()?)).expect("TL is mapped now");
    assert_eq!(strings(&out, "CL_Final"), vec![Some("SS".to_string()); 2]);
    let kept = std::fs::read_to_string(&map).expect("read");
    assert!(kept.contains("\"TL\""), "an existing map is never overwritten");
    // a map that points to a category the rule file does not have is refused
    std::fs::write(&map, "{\n  \"defaults_version\": 1,\n  \"TL\": \"Overnight\"\n}\n").expect("write the map");
    let e = format!("{:#}", compile_tab_lazy(&src, &[s], &tmp).and_then(|lf| Ok(lf.collect()?)).err().expect("unknown category"));
    assert!(e.contains("'TL'") && e.contains("Continuous, Demand, Fixed term, Short-term agri & micro"), "{e}");
    set_data_dir_override(None);
}

#[test]
fn r4_bad_values_stop_the_run_with_the_count_and_the_first_five_accounts() {
    let n = 7;
    // a date after the cutoff on 7 rows
    let df = DataFrame::new(vec![acct(n), dates("Since", &vec!["2026-11-15"; n])]).expect("book");
    let e = format!("{:#}", run("r4f", &df, &step(Some("Since"), None, None, None, "2026-10-31")).err().expect("future date"));
    assert!(e.contains("the overdue-since date is after the cutoff 2026-10-31 on 7 row(s)"), "{e}");
    assert!(e.contains("the first 5 values of Account_No: A1, A2, A3, A4, A5") && !e.contains("A6"), "{e}");
    // negative and fractional days past due
    let df = DataFrame::new(vec![acct(3), Series::new("DPD".into(), &[-5.0, 2.5, 10.0])]).expect("book");
    let e = format!("{:#}", run("r4f2", &df, &step(None, Some("DPD"), None, None, "2026-10-31")).err().expect("bad dpd"));
    assert!(e.contains("negative on 1 row(s)") && e.contains("not a whole number of days on 1 row(s)"), "{e}");
    // a text column for the date, a text column for the days
    let df = DataFrame::new(vec![acct(1), Series::new("Since".into(), &["31/07/2026"]), Series::new("DPD".into(), &["92"])]).expect("book");
    let e = format!("{:#}", run("r4f3", &df, &step(Some("Since"), None, None, None, "2026-10-31")).err().expect("text date"));
    assert!(e.contains("is String, not a date") || e.contains("not a date"), "{e}");
    let e = format!("{:#}", run("r4f4", &df, &step(None, Some("DPD"), None, None, "2026-10-31")).err().expect("text days"));
    assert!(e.contains("not a number"), "{e}");
    // a missing column, no cutoff, neither period column
    let e = format!("{:#}", run("r4f5", &df, &step(Some("Nope"), None, None, None, "2026-10-31")).err().expect("missing"));
    assert!(e.contains("Nope") && e.contains("not found"), "{e}");
    let e = format!("{:#}", run("r4f6", &df, &step(Some("Since"), None, None, None, "")).err().expect("no cutoff"));
    assert!(e.contains("cutoff date"), "{e}");
    let e = format!("{:#}", run("r4f7", &df, &step(None, None, None, None, "2026-10-31")).err().expect("no period column"));
    assert!(e.contains("either the overdue-since date column or the days-past-due column"), "{e}");
    // the new columns may not already be there
    let df2 = DataFrame::new(vec![acct(1), dates("Since", &["2026-07-31"]), Series::new("CL_Final".into(), &["SS"])]).expect("book");
    let e = format!("{:#}", run("r4f8", &df2, &step(Some("Since"), None, None, None, "2026-10-31")).err().expect("existing column"));
    assert!(e.contains("already has a column 'CL_Final'"), "{e}");
}

#[test]
fn r4_the_rule_file_is_dated_sourced_per_category_and_a_later_file_takes_over() {
    let (kind, (date, text)) = SHIPPED_RULES.iter().find(|(k, _)| *k == "cl_overdue").expect("shipped");
    assert_eq!((*kind, *date), ("cl_overdue", "2025-04-01"));
    let f: serde_json::Value = serde_json::from_str(text).expect("json");
    assert_eq!(f["sample"], serde_json::json!(false));
    assert!(f["source"].as_str().unwrap_or("").contains("Circular No. 15") && f["source"].as_str().unwrap_or("").contains("27 November 2024"));
    let names: Vec<&str> = f["categories"].as_array().expect("categories").iter().map(|c| c["name"].as_str().unwrap_or("")).collect();
    assert_eq!(names, vec!["Continuous", "Demand", "Fixed term", "Short-term agri & micro"]);
    // before the circular's effective date there is no file
    set_data_dir_override(Some(dir("r4g")));
    let e = format!("{:#}", load_overdue_rules(d("2025-03-31")).err().expect("too early"));
    assert!(e.contains("dated on or before the cutoff 2025-03-31"), "{e}");
    let ok = load_overdue_rules(d("2026-10-31")).expect("loads");
    assert_eq!(ok.meta.file, "cl_overdue/2025-04-01.json");
    assert!(!ok.meta.sample);
    set_data_dir_override(None);

    // a later dated file with another threshold for Demand loans only: SS from 4 months
    let data = dir("r4g2_data");
    set_data_dir_override(Some(data.clone()));
    load_overdue_rules(d("2026-10-31")).expect("writes the shipped file");
    let mut v: serde_json::Value = serde_json::from_str(text).expect("json");
    v["as_of"] = serde_json::json!("2027-01-01");
    v["source"] = serde_json::json!("a later BRPD circular");
    for c in v["categories"].as_array_mut().expect("categories") {
        if c["name"] == "Demand" {
            c["bands"][4]["from_months"] = serde_json::json!(4);
            c["bands"][5]["from_months"] = serde_json::json!(7);
        }
    }
    std::fs::write(data.join("rules").join("cl_overdue").join("2027-01-01.json"), serde_json::to_string_pretty(&v).expect("json")).expect("write");
    let df = DataFrame::new(vec![acct(2), Series::new("Cat".into(), &["Demand", "Continuous"]), dates("Since", &["2027-01-31"; 2])]).expect("book");
    let s = step(Some("Since"), None, None, Some("Cat"), "2027-05-31");
    let out = run_steps_in("r4g3", &df, &s, &data).expect("classifies");
    // four complete months: Demand is still SMA..SS from 4 -> SS; Continuous is SS from 3 -> SS; at 3 months they differ
    assert_eq!(strings(&out, "CL_Rule_File")[0].as_deref(), Some("cl_overdue/2027-01-01.json"));
    let df3 = DataFrame::new(vec![acct(2), Series::new("Cat".into(), &["Demand", "Continuous"]), dates("Since", &["2027-02-28"; 2])]).expect("book");
    let out = run_steps_in("r4g4", &df3, &s, &data).expect("classifies");
    assert_eq!(strings(&out, "CL_Final"), vec![Some("SMA".to_string()), Some("SS".to_string())], "Demand: 3 months is still SMA under the later file; Continuous: SS");
    // and an earlier cutoff still uses the 2025 file
    let early = step(Some("Since"), None, None, Some("Cat"), "2026-10-31");
    let df4 = DataFrame::new(vec![acct(1), Series::new("Cat".into(), &["Demand"]), dates("Since", &["2026-07-31"])]).expect("book");
    let out = run_steps_in("r4g5", &df4, &early, &data).expect("classifies");
    assert_eq!(strings(&out, "CL_Rule_File")[0].as_deref(), Some("cl_overdue/2025-04-01.json"));
    set_data_dir_override(None);
}

/// Runs a step with the data folder `data` (already holding its files).
fn run_steps_in(name: &str, df: &DataFrame, s: &TransformStep, data: &std::path::Path) -> anyhow::Result<DataFrame> {
    set_data_dir_override(Some(data.to_path_buf()));
    let out = run_steps(name, df, std::slice::from_ref(s));
    set_data_dir_override(None);
    out
}

#[test]
fn r4_a_bad_rule_file_is_refused_with_its_name() {
    let (_, (_, text)) = SHIPPED_RULES.iter().find(|(k, _)| *k == "cl_overdue").expect("shipped");
    let data = dir("r4h_data");
    let folder = data.join("rules").join("cl_overdue");
    std::fs::create_dir_all(&folder).expect("folder");
    let mut v: serde_json::Value = serde_json::from_str(text).expect("json");
    v["categories"][0]["bands"][3]["grade"] = serde_json::json!("SMA-X");
    std::fs::write(folder.join("2025-04-01.json"), serde_json::to_string(&v).expect("json")).expect("write");
    set_data_dir_override(Some(data.clone()));
    let e = format!("{:#}", load_overdue_rules(d("2026-10-31")).err().expect("unknown grade"));
    assert!(e.contains("cl_overdue/2025-04-01.json") && e.contains("'SMA-X'"), "{e}");
    let mut v: serde_json::Value = serde_json::from_str(text).expect("json");
    v["categories"][0]["bands"][4]["from_months"] = serde_json::json!(1); // not growing
    std::fs::write(folder.join("2025-04-01.json"), serde_json::to_string(&v).expect("json")).expect("write");
    let e = format!("{:#}", load_overdue_rules(d("2026-10-31")).err().expect("bounds do not grow"));
    assert!(e.contains("must grow"), "{e}");
    set_data_dir_override(None);
}

#[test]
fn r4_cl_final_runs_into_the_provisioning_step_and_the_migration_matrix_directly() {
    let df = DataFrame::new(vec![
        Series::new("Actnum".into(), &["A1", "A2", "A3", "A4"]),
        Series::new("Balance".into(), &[1_000_000.0, 2_000_000.0, 3_000_000.0, 4_000_000.0]),
        dates("Since", &["", "2026-09-30", "2026-08-31", "2026-04-30"]),
    ])
    .expect("book");
    let data = dir("r4i_data");
    set_data_dir_override(Some(data.clone()));
    let out = run_steps("r4i", &df, &[step(Some("Since"), None, None, None, "2026-10-31")]).expect("classifies");
    assert_eq!(strings(&out, "CL_Final"), ["STD-0", "STD-2", "SMA", "DF"].iter().map(|x| Some(x.to_string())).collect::<Vec<_>>());
    // Phase 1: the provisioning step reads CL_Final (STD-0 and STD-2 are STD, 1%; SMA 5%; DF 50%)
    let cfg = ProvisionConfig::load_or_create(&data).expect("the maps");
    let prov = apply_loan_classification_provision_with(out.clone().lazy(), "Balance", "CL_Final", None, None, None, None, "2026-10-31", &cfg).expect("provisions").collect().expect("collects");
    assert_eq!(strings(&prov, "CL_Grade"), ["STD", "STD", "SMA", "DF"].iter().map(|x| Some(x.to_string())).collect::<Vec<_>>());
    // Phase 4: the migration matrix reads it too (previous month all standard)
    let prev = df.clone().lazy().with_column(lit("STD").alias("CL_Final"));
    let spec = ClMigrationSpec { key_col: "Actnum".into(), bal_col: "Balance".into(), cl_col: "CL_Final".into(), segment_col: None, measure: MigrationMeasure::Count, pct_of_row: false, balance_basis: Default::default() };
    let m = build_cl_migration_with(prev, out.lazy(), &spec, &cfg.cl_code_map).expect("the matrix");
    assert!(m.height() >= 1);
    set_data_dir_override(None);
}

#[test]
fn r4_the_step_is_saved_in_a_macro_and_an_old_step_list_still_loads() {
    let s = step(Some("Since"), None, Some("Subjective"), Some("Loan_Type"), "2026-10-31");
    let json = serde_json::to_string(&s).expect("json");
    let back: TransformStep = serde_json::from_str(&json).expect("loads");
    assert_eq!(format!("{back:?}"), format!("{s:?}"));
    // only the cutoff given: the other fields take their defaults
    let bare: TransformStep = serde_json::from_str(r#"{"ClassifyOverdue":{"cutoff_date":"2026-10-31"}}"#).expect("defaults");
    assert!(matches!(bare, TransformStep::ClassifyOverdue { category_col: None, since_col: None, dpd_col: None, override_col: None, .. }));
    assert_eq!(s.title(), "Classify by overdue");
    assert!(micro_parquet::macros::python_untranslated_steps(std::slice::from_ref(&s)).iter().any(|m| m.contains("Classify by overdue")));
}

// ---------------------------------------------------------------- B3: blank dates and the overdue amount

fn step_with_amount(since: Option<&str>, dpd: Option<&str>, amount: &str) -> TransformStep {
    match step(since, dpd, None, None, "2026-10-31") {
        TransformStep::ClassifyOverdue { category_col, fixed_category, since_col, dpd_col, override_col, cutoff_date, .. } => TransformStep::ClassifyOverdue { category_col, fixed_category, since_col, dpd_col, override_col, overdue_amount_col: Some(amount.to_string()), cutoff_date },
        other => other,
    }
}

#[test]
fn b3_the_status_note_always_counts_the_rows_treated_as_not_overdue_because_the_date_was_blank() {
    let data = dir("b3a_data");
    set_data_dir_override(Some(data));
    let tmp = dir("b3a");
    let s = step(Some("Since"), None, None, None, "2026-10-31");
    // three blank dates
    let df = DataFrame::new(vec![acct(4), dates("Since", &["", "2026-07-31", "", ""])]).expect("book");
    let src = parquet(&tmp, "in", &df);
    let notes = micro_parquet::engine::blank_balance_notes(&src, std::slice::from_ref(&s), &tmp).expect("notes");
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("3 row(s) treated as not overdue (STD-0) because the overdue date was blank"), "{notes:?}");
    // none: the note is still there, with 0
    let df = DataFrame::new(vec![acct(2), dates("Since", &["2026-07-31", "2026-08-31"])]).expect("book");
    let src = parquet(&tmp, "in2", &df);
    let notes = micro_parquet::engine::blank_balance_notes(&src, std::slice::from_ref(&s), &tmp).expect("notes");
    assert!(notes[0].contains("0 row(s) treated as not overdue (STD-0) because the overdue date was blank"), "{notes:?}");
    // a blank days-past-due counts the same way
    let df = DataFrame::new(vec![acct(2), Series::new("DPD".into(), &[Some(5.0), None])]).expect("book");
    let src = parquet(&tmp, "in3", &df);
    let notes = micro_parquet::engine::blank_balance_notes(&src, &[step(None, Some("DPD"), None, None, "2026-10-31")], &tmp).expect("notes");
    assert!(notes[0].contains("1 row(s) treated as not overdue"), "{notes:?}");
    set_data_dir_override(None);
}

#[test]
fn b3_an_overdue_amount_above_zero_with_a_blank_date_stops_the_run_with_the_count_and_the_first_five_accounts() {
    let n = 7;
    let df = DataFrame::new(vec![acct(n), dates("Since", &vec![""; n]), Series::new("Overdue_Amt".into(), vec![100.0; n])]).expect("book");
    let e = format!("{:#}", run("b3b", &df, &step_with_amount(Some("Since"), None, "Overdue_Amt")).err().expect("amount without date"));
    assert!(e.contains("the overdue amount in 'Overdue_Amt' is above 0 but the overdue date (or days past due) is blank on 7 row(s)"), "{e}");
    assert!(e.contains("the first 5 values of Account_No: A1, A2, A3, A4, A5") && !e.contains("A6"), "{e}");
    // the same with days past due
    let df = DataFrame::new(vec![acct(2), Series::new("DPD".into(), &[Some(40.0), None]), Series::new("Overdue_Amt".into(), &[50.0, 50.0])]).expect("book");
    let e = format!("{:#}", run("b3c", &df, &step_with_amount(None, Some("DPD"), "Overdue_Amt")).err().expect("amount without dpd"));
    assert!(e.contains("on 1 row(s)") && e.contains("A2") && !e.contains("A1"), "{e}");
    // a zero or blank amount with a blank date is fine (and counted); an amount with a date is fine
    let df = DataFrame::new(vec![acct(4), dates("Since", &["", "", "2026-07-31", "2026-10-15"]), Series::new("Overdue_Amt".into(), &[Some(0.0), None, Some(900.0), Some(10.0)])]).expect("book");
    let out = run("b3d", &df, &step_with_amount(Some("Since"), None, "Overdue_Amt")).expect("runs");
    assert_eq!(strings(&out, "CL_Final"), ["STD-0", "STD-0", "SS", "STD-1"].iter().map(|x| Some(x.to_string())).collect::<Vec<_>>());
    // a text amount column, and a missing one
    let df = DataFrame::new(vec![acct(1), dates("Since", &[""]), Series::new("Overdue_Amt".into(), &["100"])]).expect("book");
    let e = format!("{:#}", run("b3e", &df, &step_with_amount(Some("Since"), None, "Overdue_Amt")).err().expect("text amount"));
    assert!(e.contains("not a number"), "{e}");
    let e = format!("{:#}", run("b3f", &df, &step_with_amount(Some("Since"), None, "Nope")).err().expect("missing"));
    assert!(e.contains("overdue amount column 'Nope' was not found"), "{e}");
    // old step lists without the field load
    let bare: TransformStep = serde_json::from_str(r#"{"ClassifyOverdue":{"cutoff_date":"2026-10-31"}}"#).expect("loads");
    assert!(matches!(bare, TransformStep::ClassifyOverdue { overdue_amount_col: None, .. }));
}

// ---------------------------------------------------------------- D6: days past due out of range, date overflow

#[test]
fn d6_days_past_due_outside_0_to_36500_and_dates_outside_the_calendar_stop_the_step_without_a_panic() {
    let n = 7;
    // 1e15 days past due on 7 rows: the count and the first 5 account numbers
    let df = DataFrame::new(vec![acct(n), Series::new("DPD".into(), vec![1e15f64; n])]).expect("book");
    let e = format!("{:#}", run("d6a", &df, &step(None, Some("DPD"), None, None, "2026-10-31")).err().expect("too many days"));
    assert!(e.contains("the days past due in 'DPD' is above 36,500 (about 100 years) on 7 row(s)"), "{e}");
    assert!(e.contains("the first 5 values of Account_No: A1, A2, A3, A4, A5") && !e.contains("A6"), "{e}");
    // 36,500 is accepted (100 years: B/L), 36,501 is not
    let df = DataFrame::new(vec![acct(2), Series::new("DPD".into(), &[36_500.0, 36_501.0])]).expect("book");
    let e = format!("{:#}", run("d6b", &df, &step(None, Some("DPD"), None, None, "2026-10-31")).err().expect("36,501"));
    assert!(e.contains("above 36,500") && e.contains("on 1 row(s)") && e.contains("A2") && !e.contains("A1"), "{e}");
    let df = DataFrame::new(vec![acct(1), Series::new("DPD".into(), &[36_500.0])]).expect("book");
    let out = run("d6c", &df, &step(None, Some("DPD"), None, None, "2026-10-31")).expect("36,500 runs");
    assert_eq!(strings(&out, "CL_Final"), vec![Some("B/L".to_string())]);
    // an integer column far out, a negative infinity and a NaN
    let df = DataFrame::new(vec![acct(3), Series::new("DPD".into(), &[f64::INFINITY, f64::NAN, -1.0])]).expect("book");
    let e = format!("{:#}", run("d6d", &df, &step(None, Some("DPD"), None, None, "2026-10-31")).err().expect("not finite"));
    assert!(e.contains("not a whole number of days") && e.contains("negative"), "{e}");
    // a date column holding a day number the calendar cannot show
    let far = Series::new("Since".into(), &[2_000_000_000i32, -2_000_000_000, 20_000]).cast(&DataType::Date).expect("Date");
    let df = DataFrame::new(vec![acct(3), far]).expect("book");
    let e = format!("{:#}", run("d6e", &df, &step(Some("Since"), None, None, None, "2026-10-31")).err().expect("outside the calendar"));
    assert!(e.contains("the overdue-since date in 'Since' is outside the calendar on 2 row(s)") && e.contains("A1, A2"), "{e}");
    // a cutoff far in the past with a large DPD gives a date outside the calendar: also a stop
    let df = DataFrame::new(vec![acct(1), Series::new("DPD".into(), &[36_500.0])]).expect("book");
    let e = format!("{:#}", run("d6f", &df, &step(None, Some("DPD"), None, None, "0001-01-05")).err().expect("calendar start"));
    assert!(e.contains("outside the calendar") || e.contains("no cl_overdue file"), "{e}");
}
