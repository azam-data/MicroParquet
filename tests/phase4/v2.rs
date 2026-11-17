//! V2: the CL migration matrix.

use crate::common::*;
use micro_parquet::banking::*;
use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::models::*;
use polars::prelude::*;

fn map() -> Vec<(String, String)> {
    ProvisionConfig::defaults().cl_code_map
}

fn frame(keys: &[&str], bal: &[f64], cl: &[&str], seg: Option<&[&str]>) -> DataFrame {
    let mut cols = vec![
        Series::new("Actnum".into(), keys),
        Series::new("Balance".into(), bal),
        Series::new("CL_Status".into(), cl),
    ];
    if let Some(s) = seg {
        cols.push(Series::new("Sector".into(), s));
    }
    DataFrame::new(cols).expect("frame")
}

/// The acceptance item: previous A1 STD 100, A2 SS 50, A3 STD 30; current A1 SMA 100, A2 DF 50, A4 STD 20.
fn months() -> (DataFrame, DataFrame) {
    (frame(&["A1", "A2", "A3"], &[100.0, 50.0, 30.0], &["STD", "SS", "STD"], None), frame(&["A1", "A2", "A4"], &[100.0, 50.0, 20.0], &["SMA", "DF", "STD"], None))
}

fn spec(measure: MigrationMeasure, pct: bool, seg: bool) -> ClMigrationSpec {
    ClMigrationSpec { key_col: "Actnum".into(), bal_col: "Balance".into(), cl_col: "CL_Status".into(), segment_col: if seg { Some("Sector".into()) } else { None }, measure, pct_of_row: pct }
}

fn build(prev: &DataFrame, curr: &DataFrame, s: &ClMigrationSpec) -> DataFrame {
    build_cl_migration_with(prev.clone().lazy(), curr.clone().lazy(), s, &map()).unwrap_or_else(|e| panic!("{e:#}"))
}

/// The value of the row labelled `row` (in the first block) under column `col`.
fn at(df: &DataFrame, row: &str, col: &str) -> f64 {
    let labels = strings(df, MIGRATION_ROW_HEADING);
    let i = labels.iter().position(|l| l.as_deref() == Some(row)).unwrap_or_else(|| panic!("no row {row}: {labels:?}"));
    floats(df, col)[i].unwrap_or(f64::NAN)
}

#[test]
fn v2_the_acceptance_example_gives_the_cells_and_the_row_totals() {
    let (prev, curr) = months();
    let m = build(&prev, &curr, &spec(MigrationMeasure::Balance, false, false));
    assert_eq!(
        m.get_column_names().iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        vec!["Previous grade", "STD", "SMA", "SS", "DF", "BL", "Closed/Repaid", "Total"],
        "the grades in cl_code_map.json order, then Closed/Repaid and Total"
    );
    assert_eq!(
        strings(&m, "Previous grade").into_iter().flatten().collect::<Vec<_>>(),
        vec!["STD", "SMA", "SS", "DF", "BL", "Previous month total", "New", "Total"]
    );
    assert_eq!(at(&m, "STD", "SMA"), 100.0, "STD to SMA");
    assert_eq!(at(&m, "SS", "DF"), 50.0, "SS to DF");
    assert_eq!(at(&m, "STD", "Closed/Repaid"), 30.0, "STD to Closed/Repaid");
    assert_eq!(at(&m, "New", "STD"), 20.0, "New to STD");
    // row totals
    assert_eq!(at(&m, "STD", "Total"), 130.0);
    assert_eq!(at(&m, "SS", "Total"), 50.0);
    assert_eq!(at(&m, "New", "Total"), 20.0);
    assert_eq!(at(&m, "SMA", "Total"), 0.0);
    // the grand total of the previous month's balances is 180, as in the previous month's own total
    assert_eq!(at(&m, "Previous month total", "Total"), 180.0);
    let prev_total: f64 = floats(&prev, "Balance").iter().flatten().sum();
    assert_eq!(at(&m, "Previous month total", "Total"), prev_total);
    assert_eq!(at(&m, "Total", "Total"), 200.0, "with the new account");
    // the current month's balances: the five grade columns of every row add up to its own total
    let curr_total: f64 = floats(&curr, "Balance").iter().flatten().sum();
    let current: f64 = ["STD", "SMA", "SS", "DF", "BL"].iter().map(|g| at(&m, "Total", g)).sum();
    assert_eq!(current, curr_total, "everything in the current month is somewhere in the five grade columns");
}

#[test]
fn v2_account_counts_and_percent_of_the_previous_grades_row_total() {
    let (prev, curr) = months();
    let c = build(&prev, &curr, &spec(MigrationMeasure::Count, false, false));
    assert_eq!(c.column("SMA").expect("c").dtype(), &DataType::Int64, "counts are whole numbers");
    assert_eq!((at(&c, "STD", "SMA"), at(&c, "SS", "DF"), at(&c, "STD", "Closed/Repaid"), at(&c, "New", "STD")), (1.0, 1.0, 1.0, 1.0));
    assert_eq!((at(&c, "STD", "Total"), at(&c, "Previous month total", "Total"), at(&c, "Total", "Total")), (2.0, 3.0, 4.0));
    // % of the row total: STD is 100 of 130 in SMA and 30 of 130 closed
    let p = build(&prev, &curr, &spec(MigrationMeasure::Balance, true, false));
    close(Some(at(&p, "STD", "SMA")), 100.0 / 130.0 * 100.0, 1e-9, "STD to SMA share");
    close(Some(at(&p, "STD", "Closed/Repaid")), 30.0 / 130.0 * 100.0, 1e-9, "STD to Closed share");
    for row in ["STD", "SS", "New"] {
        close(Some(at(&p, row, "Total")), 100.0, 1e-9, "a row of shares adds up to 100");
    }
    assert_eq!(at(&p, "SMA", "Total"), 0.0, "an empty row has no shares");
    // the same for counts
    let pc = build(&prev, &curr, &spec(MigrationMeasure::Count, true, false));
    close(Some(at(&pc, "STD", "SMA")), 50.0, 1e-9, "one of two accounts");
}

#[test]
fn v2_codes_are_normalised_a_key_with_several_rows_uses_its_worst_grade_and_segments_get_blocks() {
    // codes in any case with spaces; A1 has two rows (UC and SS), the worst grade counts
    let prev = frame(&["A1", "A1", "A2", "A3"], &[10.0, 90.0, 40.0, 60.0], &[" uc ", "ss", "STD-1", "B/L"], Some(&["CMS", "CMS", "Medium", "Medium"]));
    let curr = frame(&["A1", "A2", "A3", "A5"], &[100.0, 40.0, 60.0, 5.0], &["SS", "std", "BL", "SMA"], Some(&["CMS", "Medium", "Medium", "CMS"]));
    let m = build(&prev, &curr, &spec(MigrationMeasure::Balance, false, true));
    assert_eq!(m.get_column_names()[0].to_string(), "Segment");
    let seg = strings(&m, "Segment");
    let prow = strings(&m, MIGRATION_ROW_HEADING);
    let find = |s: &str, r: &str| (0..m.height()).find(|i| seg[*i].as_deref() == Some(s) && prow[*i].as_deref() == Some(r)).unwrap_or_else(|| panic!("{s} {r}"));
    let v = |s: &str, r: &str, c: &str| floats(&m, c)[find(s, r)].unwrap_or(f64::NAN);
    // CMS: A1 was SS (worst of its two rows, balance 100) and still SS; A5 is new
    assert_eq!(v("CMS", "SS", "SS"), 100.0);
    assert_eq!(v("CMS", "New", "SMA"), 5.0);
    // Medium: A2 STD to STD, A3 BL to BL
    assert_eq!((v("Medium", "STD", "STD"), v("Medium", "BL", "BL")), (40.0, 60.0));
    // All segments adds the blocks up
    assert_eq!(v(MIGRATION_ALL_SEGMENTS, "Previous month total", "Total"), 200.0, "10 + 90 + 40 + 60");
    assert_eq!(v("CMS", "Previous month total", "Total") + v("Medium", "Previous month total", "Total"), 200.0);
    // blocks are in natural order, the last is all segments
    let order: Vec<String> = seg.iter().flatten().fold(Vec::new(), |mut a, s| {
        if !a.contains(s) {
            a.push(s.clone());
        }
        a
    });
    assert_eq!(order, vec!["CMS".to_string(), "Medium".to_string(), MIGRATION_ALL_SEGMENTS.to_string()]);
}

#[test]
fn v2_unmapped_codes_in_either_month_stop_with_the_phase_1_message_listing_them() {
    let prev = frame(&["A1", "A2"], &[1.0, 2.0], &["STD", "XYZ"], None);
    let curr = frame(&["A1", "A2"], &[1.0, 2.0], &["BLW", "STD"], None);
    let err = format!("{:#}", build_cl_migration_with(prev.lazy(), curr.lazy(), &spec(MigrationMeasure::Balance, false, false), &map()).err().expect("unmapped codes"));
    assert!(err.contains("CL status column 'CL_Status' has codes not in cl_code_map.json: 'XYZ', 'BLW'. Add each code with its grade (STD, SMA, SS, DF or BL) to that file."), "{err}");
    assert!(!err.contains("'STD'"), "{err}");
    // a blank code is listed as (blank)
    let prev = frame(&["A1"], &[1.0], &[" "], None);
    let curr = frame(&["A1"], &[1.0], &["STD"], None);
    let err = format!("{:#}", build_cl_migration_with(prev.lazy(), curr.lazy(), &spec(MigrationMeasure::Balance, false, false), &map()).err().expect("blank code"));
    assert!(err.contains("(blank)"), "{err}");
}

#[test]
fn v2_bad_settings_are_messages_a_banker_can_act_on() {
    let (prev, curr) = months();
    let run = |p: DataFrame, c: DataFrame, s: ClMigrationSpec| format!("{:#}", build_cl_migration_with(p.lazy(), c.lazy(), &s, &map()).err().expect("an error"));
    let mut s = spec(MigrationMeasure::Balance, false, false);
    s.key_col = "Nope".into();
    let e = run(prev.clone(), curr.clone(), s);
    assert!(e.contains("previous month has no column 'Nope'"), "{e}");
    // a text balance
    let text_bal = DataFrame::new(vec![Series::new("Actnum".into(), &["A1"]), Series::new("Balance".into(), &["1,000"]), Series::new("CL_Status".into(), &["STD"])]).expect("frame");
    let e = run(text_bal, curr.clone(), spec(MigrationMeasure::Balance, false, false));
    assert!(e.contains("balance column 'Balance'") && e.contains("not a number"), "{e}");
    // a blank key
    let blank_key = DataFrame::new(vec![Series::new("Actnum".into(), &[Some("A1"), None]), Series::new("Balance".into(), &[1.0, 2.0]), Series::new("CL_Status".into(), &["STD", "STD"])]).expect("frame");
    let e = run(prev, blank_key, spec(MigrationMeasure::Balance, false, false));
    assert!(e.contains("1 blank value(s) in the current month"), "{e}");
}

#[test]
fn v2_shading_marks_upgrades_and_downgrades_only() {
    assert_eq!(migration_shade("SS", "STD"), Shade::Upgrade);
    assert_eq!(migration_shade("BL", "DF"), Shade::Upgrade);
    assert_eq!(migration_shade("STD", "SMA"), Shade::Downgrade);
    assert_eq!(migration_shade("SS", "BL"), Shade::Downgrade);
    for (a, b) in [("STD", "STD"), ("DF", "DF"), ("New", "STD"), ("STD", "Closed/Repaid"), ("STD", "Total"), ("Total", "BL"), ("Previous month total", "SS")] {
        assert_eq!(migration_shade(a, b), Shade::None, "{a} to {b}");
    }
    assert_eq!(cl_grades(), ["STD", "SMA", "SS", "DF", "BL"], "the order of cl_code_map.json");
}

#[test]
fn v2_the_matrix_is_a_tab_source_that_steps_can_follow_and_a_changed_month_changes_it() {
    let d = dir("v2_source");
    let (prev, curr) = months();
    let src = TabSource::ClMigration {
        prev_source: Box::new(parquet(&d, "prev", &prev)),
        prev_steps: vec![],
        curr_source: Box::new(parquet(&d, "curr", &curr)),
        curr_steps: vec![],
        spec: spec(MigrationMeasure::Balance, false, false),
    };
    let steps = vec![TransformStep::FilterRows { column: "Previous grade".into(), operator: FilterOp::NotEq, value: "Total".into(), keep_matching: true }];
    let out = compile_tab_lazy(&src, &steps, &d).expect("compiles").collect().expect("collects");
    assert_eq!(out.height(), 7, "the Total row is filtered out by a step after the matrix");
    assert!(micro_parquet::engine::validate_steps(&src, &steps, &d).expect("validates").is_empty());
    // the previous tab's own steps apply: drop the closed account first
    let src2 = TabSource::ClMigration {
        prev_source: Box::new(parquet(&d, "prev", &prev)),
        prev_steps: vec![TransformStep::FilterRows { column: "Actnum".into(), operator: FilterOp::NotEq, value: "A3".into(), keep_matching: true }],
        curr_source: Box::new(parquet(&d, "curr", &curr)),
        curr_steps: vec![],
        spec: spec(MigrationMeasure::Balance, false, false),
    };
    let m = compile_tab_lazy(&src2, &[], &d).expect("compiles").collect().expect("collects");
    assert_eq!(at(&m, "STD", "Closed/Repaid"), 0.0, "A3 is not in the previous month any more");
    assert_eq!(at(&m, "Previous month total", "Total"), 150.0);
    // the fingerprint of the pipeline follows the settings
    let h1 = micro_parquet::steps_edit::pipeline_hash(&src, &[]);
    let h2 = micro_parquet::steps_edit::pipeline_hash(&src2, &[]);
    assert_ne!(h1, h2);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn v2_the_tool_is_a_two_tab_banking_command_under_loans_and_credit_and_the_search_finds_it() {
    use micro_parquet::commands::*;
    let c = command(CommandId::ClMigration).expect("the command");
    assert_eq!((c.group, c.needs), (CommandGroup::Banking, Needs::TwoTabs));
    assert_eq!(unavailable_reason(c, CommandState { tabs: 1, ..Default::default() }), Some("Needs two open tabs."));
    assert_eq!(unavailable_reason(c, CommandState { tabs: 2, ..Default::default() }), None);
    assert_eq!(banking_section(CommandId::ClMigration), Some("Loans & credit"));
    assert_eq!(banking_section(CommandId::Ftp), Some("Treasury & ALM"));
    assert_eq!(banking_section(CommandId::Filter), None, "only Banking commands have a heading");
    // every Banking command is under one of the two headings, and the commands of a heading are together
    let sections: Vec<&str> = in_group(CommandGroup::Banking).map(|c| banking_section(c.id).expect("a heading")).collect();
    let mut changes = 0;
    for w in sections.windows(2) {
        if w[0] != w[1] {
            changes += 1;
        }
    }
    assert_eq!(changes, 1, "Treasury & ALM first, then Loans & credit: {sections:?}");
    // searching for what bankers call it finds it first
    let entries: Vec<Entry> = COMMANDS.iter().map(|c| Entry { label: c.title, words: c.words, recent: None }).collect();
    for q in ["cl migration", "grade upgrade downgrade", "npl roll"] {
        let ranked = rank(q, &entries);
        assert_eq!(COMMANDS[ranked[0]].id, CommandId::ClMigration, "{q}");
    }
}
