//! Phase 3 fixes, part 1: library-level checks (UI-state checks are in `src/ui/tests.rs`).

use micro_parquet::jobs::JobCtx;
use micro_parquet::models::*;
use micro_parquet::tab_jobs::*;
use polars::prelude::*;
use std::path::{Path, PathBuf};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mp_fix_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn parquet(d: &Path, name: &str, mut df: DataFrame) -> TabSource {
    let p = d.join(format!("{name}.parquet"));
    ParquetWriter::new(std::fs::File::create(&p).unwrap()).finish(&mut df).unwrap();
    TabSource::SingleFile { path: p, is_parquet: true, config: IngestionConfig::default() }
}

fn dr_frame() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Branch".into(), &["DHK", "CTG"]).into(),
        Series::new("Amount".into(), &["100", "200 Dr"]).into(),
    ])
    .unwrap()
}

fn to_number() -> TransformStep {
    TransformStep::ConvertToNumber { columns: vec!["Amount".into()] }
}

// ---------------------------------------------------------------------------------- fix 2

#[test]
fn fix2_refresh_reports_a_page_that_cannot_be_read() {
    let d = dir("f2r");
    let req = RefreshRequest {
        tab_id: 1,
        source: parquet(&d, "dr", dr_frame()),
        steps: vec![to_number()],
        offset: 0,
        limit: 100,
        control_col: None,
        recalculate_baseline: false,
        check_join: false,
        added_status: None,
        temp_dir: d.clone(),
        validate: true,
        exec_steps: 1,
    };
    let r = refresh_tab(req, &JobCtx::inert()).unwrap();
    assert!(r.slice.is_none());
    let e = r.page_error.expect("page error");
    assert!(e.contains("Amount"), "{e}");
    assert!(r.error.is_none(), "the pipeline built; only reading failed");
}

#[test]
fn fix2_load_reports_a_first_page_that_cannot_be_read() {
    let d = dir("f2l");
    let req = LoadRequest {
        tab_id: 1,
        title: "Dr".into(),
        source: parquet(&d, "dr", dr_frame()),
        steps: vec![to_number()],
        col_formats: Default::default(),
        temp_dir: d.clone(),
        reconcile_rate_used: None,
    };
    let l = load_tab(req, &JobCtx::inert()).unwrap();
    assert!(l.tab.page_error.as_deref().is_some_and(|e| e.contains("Amount")));
    assert!(l.status.starts_with("Tab created, but its first page could not be read:"), "{}", l.status);
}
