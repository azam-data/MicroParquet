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

// ---------------------------------------------------------------------------------- fix 3

fn filter(value: &str) -> TransformStep {
    TransformStep::FilterRows { column: "Branch".into(), operator: FilterOp::Eq, value: value.into(), keep_matching: true }
}

#[test]
fn fix3_pipeline_hash_sees_settings_on_off_and_source_not_just_the_count() {
    use micro_parquet::steps_edit::{pipeline_hash, toggle_skip};
    let d = dir("f3");
    let src = parquet(&d, "a", dr_frame());
    let base = pipeline_hash(&src, &[filter("DHK")]);
    assert_eq!(base, pipeline_hash(&src, &[filter("DHK")]), "same pipeline, same hash");
    assert_ne!(base, pipeline_hash(&src, &[filter("CTG")]), "same count, other setting");
    let mut off = vec![filter("DHK")];
    toggle_skip(&mut off, 0);
    assert_ne!(base, pipeline_hash(&src, &off), "turned off");
    // another column type in the ingestion settings (the source schema)
    let typed = match &src {
        TabSource::SingleFile { path, is_parquet, config } => {
            let mut config = config.clone();
            config.columns.push(ColumnSchemaConfig { name: "Amount".into(), ..Default::default() });
            TabSource::SingleFile { path: path.clone(), is_parquet: *is_parquet, config }
        }
        _ => unreachable!(),
    };
    assert_ne!(base, pipeline_hash(&typed, &[filter("DHK")]), "schema changed");
    // the file replaced with other content
    let mut df = df_with_three_rows();
    let TabSource::SingleFile { path, .. } = &src else { unreachable!() };
    ParquetWriter::new(std::fs::File::create(path).unwrap()).finish(&mut df).unwrap();
    assert_ne!(base, pipeline_hash(&src, &[filter("DHK")]), "file replaced");
}

fn df_with_three_rows() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Branch".into(), &["DHK", "CTG", "SYL"]).into(),
        Series::new("Amount".into(), &["1", "2", "3"]).into(),
    ])
    .unwrap()
}

// ---------------------------------------------------------------------------------- fix 4

#[test]
fn fix4_a_step_check_that_cannot_run_is_reported() {
    let d = dir("f4");
    let src = parquet(&d, "gone", dr_frame());
    let TabSource::SingleFile { path, .. } = &src else { unreachable!() };
    std::fs::remove_file(path).unwrap();
    let req = RefreshRequest {
        tab_id: 1,
        source: src.clone(),
        steps: vec![filter("DHK")],
        offset: 0,
        limit: 100,
        control_col: Some("Amount".into()),
        recalculate_baseline: false,
        check_join: false,
        added_status: None,
        temp_dir: d.clone(),
        validate: true,
        exec_steps: 1,
    };
    let r = refresh_tab(req, &JobCtx::inert()).unwrap();
    let e = r.error.expect("the failed check is an error, not 'no issues'");
    assert!(e.starts_with(STEP_CHECK_FAILED), "{e}");
    assert!(r.slice.is_none() && r.page_error.is_some());
    assert_eq!(r.totals.rows, None, "rows show — instead of a number");

    let load = LoadRequest {
        tab_id: 2,
        title: "Gone".into(),
        source: src,
        steps: vec![filter("DHK")],
        col_formats: Default::default(),
        temp_dir: d.clone(),
        reconcile_rate_used: None,
    };
    let err = match load_tab(load, &JobCtx::inert()) {
        Ok(_) => panic!("a tab whose steps cannot be checked must not open"),
        Err(e) => format!("{:#}", e),
    };
    assert!(err.starts_with(STEP_CHECK_FAILED), "{err}");
}
