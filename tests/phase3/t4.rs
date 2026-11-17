//! Phase 3 T4: editing the steps list (pure functions, validation, counts, undo).

use micro_parquet::engine::{compile_tab_lazy, validate_steps};
use micro_parquet::jobs::JobCtx;
use micro_parquet::macros::*;
use micro_parquet::models::*;
use micro_parquet::steps_edit::*;
use micro_parquet::tab_jobs::*;
use polars::prelude::*;
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mp_t4_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn source(d: &std::path::Path) -> TabSource {
    let mut df = DataFrame::new(vec![
        Series::new("Actnum".into(), &["A1", "A2", "A3", "A4", "A5"]).into(),
        Series::new("Brancd".into(), &["DHK", "DHK", "CTG", "CTG", "SYL"]).into(),
        Series::new("Balance".into(), &[10_000_000.0f64, 50_000_000.0, 70_000_000.0, 20_000_000.0, 90_000_000.0]).into(),
    ])
    .unwrap();
    let p = d.join("loans.parquet");
    ParquetWriter::new(std::fs::File::create(&p).unwrap()).finish(&mut df).unwrap();
    TabSource::SingleFile { path: p, is_parquet: true, config: IngestionConfig::default() }
}

fn calc() -> TransformStep {
    TransformStep::MathFormula {
        new_col: "Balance_Cr".into(),
        left_col: "Balance".into(),
        op: MathOp::Div,
        right_is_col: false,
        right_val_or_col: "10000000".into(),
    }
}

fn filter_cr() -> TransformStep {
    TransformStep::FilterGroup {
        conditions: vec![FilterCondition { column: "Balance_Cr".into(), op: CondOp::Gt, value: "3".into(), value2: String::new() }],
        join: GroupJoin::And,
        keep_matching: true,
    }
}

fn filter_br(v: &str) -> TransformStep {
    TransformStep::FilterRows { column: "Brancd".into(), operator: FilterOp::Eq, value: v.into(), keep_matching: true }
}

fn dbg(s: &TransformStep) -> String {
    format!("{:?}", s)
}

fn labels(steps: &[TransformStep]) -> Vec<String> {
    steps.iter().map(|s| s.title().to_string()).collect()
}

#[test]
fn t4_move_delete_duplicate_insert_on_the_list() {
    let mut s = vec![calc(), filter_cr(), filter_br("DHK")];
    assert_eq!(move_down(&mut s, 0), Some(1));
    assert_eq!(labels(&s), vec!["Filter", "Calc column", "Filter"]);
    assert_eq!(move_up(&mut s, 1), Some(0));
    assert_eq!(labels(&s), vec!["Calc column", "Filter", "Filter"]);
    assert_eq!(move_up(&mut s, 0), None, "already first");
    assert_eq!(move_down(&mut s, 2), None, "already last");
    assert!(!move_step(&mut s, 0, 9) && !move_step(&mut s, 9, 0) && !move_step(&mut s, 1, 1));
    assert!(move_step(&mut s, 0, 2));
    assert_eq!(labels(&s), vec!["Filter", "Filter", "Calc column"]);
    // duplicate
    let mut s = vec![calc(), filter_br("DHK")];
    assert_eq!(duplicate_step(&mut s, 1), Some(2));
    assert_eq!(s.len(), 3);
    assert_eq!(dbg(&s[1]), dbg(&s[2]), "the copy equals the original");
    assert_eq!(duplicate_step(&mut s, 7), None);
    // delete
    let removed = delete_step(&mut s, 0).unwrap();
    assert_eq!(removed.title(), "Calc column");
    assert_eq!(s.len(), 2);
    assert!(delete_step(&mut s, 5).is_none());
    // insert
    let mut s = vec![calc(), filter_br("DHK")];
    assert_eq!(insert_after(&mut s, Some(0), filter_cr()), 1);
    assert_eq!(labels(&s), vec!["Calc column", "Filter", "Filter"]);
    assert_eq!(insert_after(&mut s, None, filter_br("CTG")), 3, "no selection: at the end");
    assert_eq!(insert_after(&mut s, Some(99), filter_br("SYL")), 4, "a stale index: at the end");
}

#[test]
fn t4_turning_a_step_off_and_on_keeps_it_and_changes_nothing() {
    let mut s = vec![calc(), filter_cr()];
    assert_eq!(toggle_skip(&mut s, 1), Some(true));
    assert!(s[1].is_skipped());
    assert_eq!(s[1].title(), "Filter", "the title is the wrapped step's");
    assert!(s[1].short_label().starts_with("Off: "));
    assert_eq!(dbg(s[1].inner()), dbg(&filter_cr()));
    assert_eq!(toggle_skip(&mut s, 1), Some(false));
    assert_eq!(dbg(&s[1]), dbg(&filter_cr()), "turning it back on restores the step exactly");
    assert_eq!(toggle_skip(&mut s, 9), None);
    // JSON round trip of an off step
    s[1] = TransformStep::Skipped { step: Box::new(filter_cr()) };
    let back: TransformStep = serde_json::from_str(&serde_json::to_string(&s[1]).unwrap()).unwrap();
    assert!(back.is_skipped());
}

#[test]
fn t4_a_turned_off_step_does_not_change_the_rows_or_the_script() {
    let d = dir("skip");
    let src = source(&d);
    let all = |steps: &[TransformStep]| compile_tab_lazy(&src, steps, &d).unwrap().collect().unwrap();
    let on = all(&[calc(), filter_cr()]);
    assert_eq!(on.height(), 3, "above 3 crore: 5, 7 and 9");
    let off = all(&[calc(), TransformStep::Skipped { step: Box::new(filter_cr()) }]);
    assert_eq!(off.height(), 5, "the filter is off: all rows, calc column still there");
    assert!(off.column("Balance_Cr").is_ok());
    let py = generate_python_polars_code(
        std::path::Path::new("x.parquet"),
        true,
        &IngestionConfig::default(),
        &[calc(), TransformStep::Skipped { step: Box::new(filter_cr()) }],
        None,
    )
    .unwrap();
    assert!(py.contains("turned off in MicroParquet"), "{py}");
    assert!(!py.contains("lf.filter"), "the off filter is not in the script: {py}");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn t4_deleting_a_calc_column_marks_a_later_filter_invalid_naming_the_column() {
    let d = dir("validate");
    let src = source(&d);
    let mut steps = vec![calc(), filter_cr()];
    assert!(validate_steps(&src, &steps, &d).unwrap().is_empty(), "valid as built");
    // delete the Calc column step
    delete_step(&mut steps, 0).unwrap();
    let issues = validate_steps(&src, &steps, &d).unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].index, 0, "the filter is now step 1 of the list");
    assert!(issues[0].reason.contains("Balance_Cr"), "reason names the column: {}", issues[0].reason);
    // an old single filter on the same column is named too
    let old = vec![TransformStep::FilterRows { column: "Balance_Cr".into(), operator: FilterOp::Gt, value: "3".into(), keep_matching: true }];
    let issues = validate_steps(&src, &old, &d).unwrap();
    assert!(issues[0].reason.contains("Balance_Cr"), "{}", issues[0].reason);
    // contains / starts-with filters do not check the column themselves; Polars' complaint is cleaned up
    let c = vec![TransformStep::FilterRows { column: "Nope".into(), operator: FilterOp::Contains, value: "x".into(), keep_matching: true }];
    let issues = validate_steps(&src, &c, &d).unwrap();
    assert!(issues[0].reason.contains("Nope") && !issues[0].reason.contains("valid columns"), "{}", issues[0].reason);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn t4_moving_turning_off_or_reordering_revalidates_the_later_steps() {
    let d = dir("revalidate");
    let src = source(&d);
    // move the filter before the calc column it needs
    let mut steps = vec![calc(), filter_cr()];
    move_up(&mut steps, 1).unwrap();
    let issues = validate_steps(&src, &steps, &d).unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].index, 0);
    // turn the calc column off: the filter after it breaks
    let mut steps = vec![calc(), filter_cr()];
    toggle_skip(&mut steps, 0).unwrap();
    let issues = validate_steps(&src, &steps, &d).unwrap();
    assert_eq!(issues.iter().map(|i| i.index).collect::<Vec<_>>(), vec![1]);
    // an off step is not judged itself: an off filter on a missing column is fine
    let steps = vec![TransformStep::Skipped { step: Box::new(filter_cr()) }];
    assert!(validate_steps(&src, &steps, &d).unwrap().is_empty());
    // a break does not hide a second, independent problem
    let steps = vec![filter_cr(), filter_br("DHK"), TransformStep::FilterRows { column: "Nope".into(), operator: FilterOp::Eq, value: "1".into(), keep_matching: true }];
    let issues = validate_steps(&src, &steps, &d).unwrap();
    assert_eq!(issues.iter().map(|i| i.index).collect::<Vec<_>>(), vec![0, 2]);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn t4_the_table_runs_up_to_the_last_valid_step() {
    let d = dir("refresh");
    let src = source(&d);
    let steps = vec![filter_br("DHK"), filter_cr(), filter_br("CTG")]; // step 2 needs Balance_Cr
    let r = refresh_tab(
        RefreshRequest {
            tab_id: 1,
            source: src.clone(),
            steps: steps.clone(),
            offset: 0,
            limit: 100,
            control_col: Some("Balance".into()),
            recalculate_baseline: false,
            check_join: false,
            added_status: None,
            temp_dir: d.clone(),
            validate: true,
            exec_steps: 0,
        },
        &JobCtx::inert(),
    )
    .unwrap();
    assert_eq!(r.issues.len(), 1);
    assert_eq!(r.issues[0].index, 1);
    assert_eq!(r.exec_steps, 1, "only the first filter runs");
    assert!(r.error.is_none());
    assert_eq!(r.totals.rows, Some(2), "the two DHK rows");
    // paging reuses the earlier answer without checking again
    let r2 = refresh_tab(
        RefreshRequest {
            tab_id: 1,
            source: src,
            steps,
            offset: 0,
            limit: 100,
            control_col: Some("Balance".into()),
            recalculate_baseline: false,
            check_join: false,
            added_status: None,
            temp_dir: d.clone(),
            validate: false,
            exec_steps: 1,
        },
        &JobCtx::inert(),
    )
    .unwrap();
    assert_eq!(r2.exec_steps, 1);
    assert_eq!(r2.totals.rows, Some(2));
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn t4_row_counts_per_step_and_reuse_of_known_counts() {
    let d = dir("counts");
    let src = source(&d);
    let steps = vec![calc(), filter_cr(), TransformStep::Skipped { step: Box::new(filter_br("DHK")) }, filter_br("CTG")];
    let hashes = prefix_hashes(&steps);
    let r = compute_step_counts(
        StepCountsRequest { tab_id: 1, source: src.clone(), steps: steps.clone(), known: vec![None; 5], hashes: hashes.clone(), temp_dir: d.clone() },
        &JobCtx::inert(),
    )
    .unwrap();
    // source 5; calc 5; filter > 3 Cr 3 rows (A2: 5, A3: 7, A5: 9); off filter 3; CTG: A3 only
    assert_eq!(r.counts, vec![Some(5), Some(5), Some(3), Some(3), Some(1)]);
    assert_eq!(r.hashes, hashes);
    // known counts are kept and only the missing ones computed (a wrong known value is returned
    // as is, which shows it was not recomputed)
    let r = compute_step_counts(
        StepCountsRequest {
            tab_id: 1,
            source: src,
            steps: steps.clone(),
            known: vec![Some(5), Some(5), Some(999), None, None],
            hashes: hashes.clone(),
            temp_dir: d.clone(),
        },
        &JobCtx::inert(),
    )
    .unwrap();
    assert_eq!(r.counts, vec![Some(5), Some(5), Some(999), Some(999), Some(1)]);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn t4_counts_stay_valid_until_a_step_before_them_changes() {
    let a = vec![calc(), filter_cr(), filter_br("CTG")];
    let mut b = a.clone();
    b[2] = filter_br("SYL"); // only the last step changes
    let (ha, hb) = (prefix_hashes(&a), prefix_hashes(&b));
    assert_eq!(&ha[..2], &hb[..2], "counts of the first two steps stay");
    assert_ne!(ha[2], hb[2]);
    let mut c = a.clone();
    c[0] = TransformStep::RenameColumn { old_name: "a".into(), new_name: "b".into() };
    let hc = prefix_hashes(&c);
    assert!(ha.iter().zip(&hc).all(|(x, y)| x != y), "a change at the start changes every count after it");
    // turning a step off changes its own and later hashes
    let mut d = a.clone();
    toggle_skip(&mut d, 1);
    let hd = prefix_hashes(&d);
    assert_eq!(ha[0], hd[0]);
    assert_ne!(ha[1], hd[1]);
}

#[test]
fn t4_undo_and_redo_at_least_fifty_levels() {
    let mut steps: Vec<TransformStep> = vec![calc()];
    let mut h = StepHistory::default();
    assert!(!h.can_undo() && !h.can_redo());
    for i in 0..60 {
        h.record(&steps);
        steps.push(filter_br(&format!("B{i}")));
    }
    assert_eq!(steps.len(), 61);
    assert!(h.undo_levels() >= 50);
    // walk back 55 changes
    for n in 0..55 {
        let prev = h.undo(&steps).expect("undo");
        steps = prev;
        assert_eq!(steps.len(), 60 - n);
    }
    assert_eq!(steps.len(), 6);
    // and forward again
    let next = h.redo(&steps).unwrap();
    assert_eq!(next.len(), 7);
    steps = next;
    // a new change ends the redo chain
    h.record(&steps);
    steps.push(filter_br("new"));
    assert!(!h.can_redo());
    assert!(h.redo(&steps).is_none());
    // the changes that were left (5 never undone, the redone one, the new one) can all be undone
    let mut n = 0;
    while let Some(prev) = h.undo(&steps) {
        steps = prev;
        n += 1;
    }
    assert_eq!(n, 7);
    assert!(!h.can_undo());
    assert_eq!(steps.len(), 1, "back at the first state");
}

#[test]
fn t4_history_is_capped_so_memory_stays_bounded() {
    let mut h = StepHistory::default();
    let mut steps = vec![calc()];
    for i in 0..(HISTORY_LEVELS + 40) {
        h.record(&steps);
        steps.push(filter_br(&format!("{i}")));
    }
    assert_eq!(h.undo_levels(), HISTORY_LEVELS);
    assert!(HISTORY_LEVELS >= 50);
}

#[test]
fn t4_edit_then_undo_round_trip_on_a_real_list() {
    let d = dir("roundtrip");
    let src = source(&d);
    let mut steps = vec![calc(), filter_cr()];
    let mut h = StepHistory::default();
    let original = steps.clone();
    // delete the calc column: filter breaks
    h.record(&steps);
    delete_step(&mut steps, 0);
    assert_eq!(validate_steps(&src, &steps, &d).unwrap().len(), 1);
    // undo: valid again, same list
    steps = h.undo(&steps).unwrap();
    assert_eq!(format!("{:?}", steps), format!("{:?}", original));
    assert!(validate_steps(&src, &steps, &d).unwrap().is_empty());
    steps = h.redo(&steps).unwrap();
    assert_eq!(steps.len(), 1);
    std::fs::remove_dir_all(&d).ok();
}
