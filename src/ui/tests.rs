//! Tests of the app state the screens draw (Phase 3 fixes). The app is built without a window:
//! its job queue runs inline, so a submitted job's result is ready for the next `poll_jobs`.

use crate::ui::export::PendingExport;
use crate::ui::prelude::*;

/// A fresh data folder under the system temp folder (one per test).
pub(crate) fn temp_root(name: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mp_ui_{}_{}_{}", name, std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("temp_scratch")).expect("test folder");
    dir
}

/// An app on a temporary data folder with an inline job queue.
pub(crate) fn test_app(name: &str) -> MicroParquetApp {
    let root = temp_root(name);
    let paths = PortablePaths {
        macros_dir: root.join("macros"),
        schemas_dir: root.join("schemas"),
        curves_dir: root.join("curves"),
        buckets_file: root.join("buckets.json"),
        temp_dir: root.join("temp_scratch"),
        root_dir: root,
    };
    let mut app = MicroParquetApp::new(paths, egui::Context::default());
    app.jobs = JobQueue::inline();
    app
}

fn failing(msg: &'static str) -> impl FnOnce(&micro_parquet::jobs::JobCtx) -> anyhow::Result<JobOutput> + Send {
    move |_| Err(anyhow::anyhow!(msg))
}

#[test]
fn inspector_failure_does_not_stop_a_pending_export() {
    let ctx = egui::Context::default();
    let mut app = test_app("fix1");
    app.export.pending = Some(PendingExport { token: 1, kind: GuardedExport::Csv, sidecar: None });
    app.jobs.submit(JobKey::tab(JobKind::Stats, 1), "Column statistics", false, failing("column 'Balance' not found"));
    app.poll_jobs(&ctx);
    assert!(app.export.pending.is_some(), "an inspector failure must not drop the export");
    assert!(!app.status_message.contains("Export stopped"), "{}", app.status_message);
    assert_eq!(app.status_message, "Column statistics failed: column 'Balance' not found");

    app.jobs.submit(JobKey::unique(JobKind::TextCheck), "Checking Text columns", true, failing("file is gone"));
    app.poll_jobs(&ctx);
    assert!(app.export.pending.is_none(), "a failed Text check stops the export");
    assert_eq!(app.status_message, "Export stopped: could not check the Text columns: file is gone");
}

#[test]
fn cancelled_text_check_drops_the_export_but_cancelled_stats_does_not() {
    let ctx = egui::Context::default();
    let mut app = test_app("fix1b");
    app.export.pending = Some(PendingExport { token: 1, kind: GuardedExport::Csv, sidecar: None });
    app.jobs.submit(JobKey::tab(JobKind::Stats, 1), "Column statistics", false, |_| {
        Err(micro_parquet::jobs::Cancelled.into())
    });
    app.poll_jobs(&ctx);
    assert!(app.export.pending.is_some());
    app.jobs.submit(JobKey::unique(JobKind::TextCheck), "Checking Text columns", true, |_| {
        Err(micro_parquet::jobs::Cancelled.into())
    });
    app.poll_jobs(&ctx);
    assert!(app.export.pending.is_none());
    assert_eq!(app.status_message, "Checking Text columns cancelled.");
}

// ------------------------------------------------------------------ helpers for tab tests

use polars::prelude::{NamedFrom, ParquetWriter, Series};

/// Writes `df` as a Parquet file in `dir` and returns it as a tab source.
pub(crate) fn parquet_source(dir: &Path, name: &str, mut df: DataFrame) -> TabSource {
    let p = dir.join(format!("{}.parquet", name));
    let f = std::fs::File::create(&p).expect("create parquet");
    ParquetWriter::new(f).finish(&mut df).expect("write parquet");
    TabSource::SingleFile { path: p, is_parquet: true, config: IngestionConfig::default() }
}

/// Branch / Amount (text, one value with "Dr") / Balance.
pub(crate) fn loans_frame() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Branch".into(), &["DHK", "CTG", "SYL"]).into(),
        Series::new("Amount".into(), &["100", "200 Dr", "300"]).into(),
        Series::new("Balance".into(), &[10_000_000.0f64, 20_000_000.0, 30_000_000.0]).into(),
    ])
    .expect("frame")
}

/// Applies job results until none are left (each result may submit more inline jobs).
pub(crate) fn settle(app: &mut MicroParquetApp) {
    let ctx = egui::Context::default();
    for _ in 0..20 {
        if !app.jobs.any_busy() {
            return;
        }
        app.poll_jobs(&ctx);
    }
}

/// Opens a tab on `source` with `steps` and returns its index (it becomes active).
pub(crate) fn open_tab(app: &mut MicroParquetApp, source: TabSource, steps: Vec<TransformStep>) -> usize {
    app.create_tab_from_source("Loans".into(), source, steps, HashMap::new());
    settle(app);
    assert!(!app.tabs.is_empty(), "tab did not open: {}", app.status_message);
    app.active_tab
}

#[test]
fn unreadable_page_is_said_dimmed_and_marked_out_of_date() {
    let mut app = test_app("fix2");
    let src = parquet_source(&app.paths.root_dir.clone(), "loans", loans_frame());
    let idx = open_tab(&mut app, src, vec![]);
    assert_eq!(app.tabs[idx].cached_slice.height(), 3);
    assert!(app.tabs[idx].page_error.is_none());
    assert!(!app.tabs[idx].summary_cells.is_empty());
    assert!(app.tabs[idx].summary_error.is_none());

    // "200 Dr" cannot be read without a sign convention: the step builds, reading the rows fails.
    app.push_step_to_active_tab(TransformStep::ConvertToNumber { columns: vec!["Amount".into()] }, false);
    settle(&mut app);
    let tab = &app.tabs[idx];
    let err = tab.page_error.clone().expect("page error kept on the tab");
    assert!(err.contains("Amount"), "{}", err);
    assert_eq!(tab.cached_slice.height(), 3, "the last page stays on screen");
    // The summary job ran after the refresh, so the last message is its failure; both name the cause.
    assert!(app.status_message.contains("Dr/Cr"), "{}", app.status_message);

    // The summary job failed too: no old values, "—" with the reason on hover.
    assert!(tab.summary_cells.is_empty());
    let reason = tab.summary_error.clone().expect("summary error");
    assert!(reason.contains("Amount"), "{}", reason);
    let view = crate::ui::table::SummaryView { cells: &tab.summary_cells, overrides: &tab.summary, error: tab.summary_error.as_deref() };
    let (text, hover) = crate::ui::table::summary_band_text(&view, "Balance", true, None, NumberDisplayFormat::BdtCrore, 2);
    assert_eq!(text, "\u{2014}");
    assert!(hover.unwrap_or_default().starts_with("The summary row could not be computed:"));
    let (text, _) = crate::ui::table::summary_band_text(&view, "Balance", false, None, NumberDisplayFormat::BdtCrore, 2);
    assert_eq!(text, "", "a column without a summary stays empty");

    // Undo: the page reads again and the marks go.
    app.undo_steps();
    settle(&mut app);
    let tab = &app.tabs[idx];
    assert!(tab.page_error.is_none());
    assert!(tab.summary_error.is_none());
    assert!(!tab.summary_cells.is_empty());
}

#[test]
fn summary_result_for_another_pipeline_with_the_same_step_count_is_dropped() {
    use micro_parquet::summary::{SummaryCell, SummaryValue};
    use micro_parquet::tab_jobs::SummaryResult;
    let ctx = egui::Context::default();
    let mut app = test_app("fix3");
    let src = parquet_source(&app.paths.root_dir.clone(), "loans", loans_frame());
    let dhk = TransformStep::FilterRows { column: "Branch".into(), operator: FilterOp::Eq, value: "DHK".into(), keep_matching: true };
    let ctg = TransformStep::FilterRows { column: "Branch".into(), operator: FilterOp::Eq, value: "CTG".into(), keep_matching: true };
    let idx = open_tab(&mut app, src.clone(), vec![dhk.clone()]);
    let good = app.tabs[idx].summary_cells.clone();
    assert!(!good.is_empty());
    let tab_id = app.tabs[idx].id;
    let fake = vec![SummaryCell { column: "Balance".into(), agg: SummaryAgg::Sum, weight_col: None, value: Some(SummaryValue::Number(1.0)), note: None }];

    // one step either way, but a different filter value: dropped
    let stale = micro_parquet::steps_edit::pipeline_hash(&src, &[ctg]);
    let cells = fake.clone();
    app.jobs.submit(JobKey::tab(JobKind::Summary, tab_id), "Summary row", false, move |_| {
        Ok(JobOutput::Summary(Box::new(SummaryResult { tab_id, pipeline: stale, cells })))
    });
    app.poll_jobs(&ctx);
    assert_eq!(app.tabs[idx].summary_cells, good);

    // the same filter turned off: dropped too
    let mut off = vec![dhk.clone()];
    micro_parquet::steps_edit::toggle_skip(&mut off, 0);
    let stale = micro_parquet::steps_edit::pipeline_hash(&src, &off);
    let cells = fake.clone();
    app.jobs.submit(JobKey::tab(JobKind::Summary, tab_id), "Summary row", false, move |_| {
        Ok(JobOutput::Summary(Box::new(SummaryResult { tab_id, pipeline: stale, cells })))
    });
    app.poll_jobs(&ctx);
    assert_eq!(app.tabs[idx].summary_cells, good);

    // the current pipeline: applied
    let current = micro_parquet::steps_edit::pipeline_hash(&src, &[dhk]);
    let cells = fake.clone();
    app.jobs.submit(JobKey::tab(JobKind::Summary, tab_id), "Summary row", false, move |_| {
        Ok(JobOutput::Summary(Box::new(SummaryResult { tab_id, pipeline: current, cells })))
    });
    app.poll_jobs(&ctx);
    assert_eq!(app.tabs[idx].summary_cells, fake);
}

/// Tests that write output files or delete `.part` files take this lock (the list of unfinished
/// outputs is per process).
pub(crate) static FILE_OUTPUT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn closing_during_an_export_asks_then_cancels_and_deletes_the_part_file() {
    use crate::ui::jobs::CloseDecision;
    let _guard = FILE_OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = test_app("fix5");
    app.jobs = JobQueue::new(|| {});
    let out = app.paths.root_dir.join("Loans.xlsx");
    let (started_tx, started_rx) = std::sync::mpsc::channel::<PathBuf>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    // An export stuck in a write that cannot be interrupted (like an Excel workbook build).
    app.jobs.submit(JobKey::unique(JobKind::Export), "Export Loans.xlsx", true, move |_ctx| {
        let tmp = micro_parquet::jobs::TempOutput::new(&out);
        std::fs::write(tmp.path(), b"partial").map_err(anyhow::Error::from)?;
        let _ = started_tx.send(tmp.path().to_path_buf());
        let _ = release_rx.recv_timeout(std::time::Duration::from_secs(30));
        anyhow::bail!("stopped")
    });
    let part = started_rx.recv_timeout(std::time::Duration::from_secs(10)).expect("export started");
    assert!(part.exists() && part.to_string_lossy().ends_with(".part"));

    // First close: the question, nothing cancelled.
    assert_eq!(app.close_requested(), CloseDecision::Ask);
    assert!(app.quit_guard);
    assert!(app.export_running());
    assert!(part.exists());

    // "Cancel export and quit": jobs cancelled, the part file deleted, then exit.
    app.quit_guard = false;
    app.quit_confirmed = true;
    assert_eq!(app.close_requested(), CloseDecision::Exit);
    assert!(!part.exists(), "the .part file is deleted before the program ends");
    assert!(app.status_message.contains("1 unfinished output file(s) deleted"), "{}", app.status_message);
    assert!(!micro_parquet::jobs::part_files().contains(&part));
    let _ = release_tx.send(());
}

#[test]
fn closing_without_an_export_does_not_ask() {
    use crate::ui::jobs::CloseDecision;
    let _guard = FILE_OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = test_app("fix5b");
    app.jobs = JobQueue::new(|| {});
    let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
    // A refresh that stops at its next safe point.
    app.jobs.submit(JobKey::tab(JobKind::Refresh, 1), "Refreshing view", false, move |ctx| {
        let _ = started_tx.send(());
        loop {
            ctx.check()?;
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    });
    started_rx.recv_timeout(std::time::Duration::from_secs(10)).expect("job started");
    assert!(!app.export_running());
    assert_eq!(app.close_requested(), CloseDecision::Exit);
    assert!(!app.quit_guard);
    assert!(app.jobs.wait_idle(std::time::Duration::from_millis(10)), "the job stopped");
}

#[test]
fn row_counts_are_recounted_when_the_file_changes_under_the_same_steps() {
    let mut app = test_app("fix6");
    let root = app.paths.root_dir.clone();
    let src = parquet_source(&root, "loans", loans_frame());
    let idx = open_tab(&mut app, src.clone(), vec![]);
    app.push_step_to_active_tab(TransformStep::FilterRows { column: "Branch".into(), operator: FilterOp::NotEq, value: "SYL".into(), keep_matching: true }, false);
    settle(&mut app);
    assert_eq!(app.tabs[idx].step_counts, vec![Some(3), Some(2)]);
    // The file is replaced by one with five rows; the steps are the same.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let five = DataFrame::new(vec![
        Series::new("Branch".into(), &["DHK", "CTG", "SYL", "DHK", "KHL"]).into(),
        Series::new("Amount".into(), &["1", "2", "3", "4", "5"]).into(),
        Series::new("Balance".into(), &[1.0f64, 2.0, 3.0, 4.0, 5.0]).into(),
    ])
    .expect("frame");
    parquet_source(&root, "loans", five);
    app.refresh_tab_at(idx, false, None);
    settle(&mut app);
    assert_eq!(app.tabs[idx].step_counts, vec![Some(5), Some(4)], "no count of the old file is kept");
}

#[test]
fn undo_and_redo_restore_steps_control_column_summary_and_formats_together() {
    let mut app = test_app("fix7");
    let df = DataFrame::new(vec![
        Series::new("Branch".into(), &["DHK", "CTG", "DHK"]).into(),
        Series::new("Balance".into(), &[10.0f64, 20.0, 30.0]).into(),
        Series::new("Limit".into(), &[100.0f64, 200.0, 300.0]).into(),
    ])
    .expect("frame");
    let src = parquet_source(&app.paths.root_dir.clone(), "loans", df);
    let idx = open_tab(&mut app, src, vec![]);
    assert_eq!(app.tabs[idx].control_col.as_deref(), Some("Balance"));

    // 1: control column; 2: a summary choice; 3: a pivot step that brings its formats
    assert!(app.set_control_column(Some("Limit".into())));
    settle(&mut app);
    app.set_summary("Balance", SummarySpec { agg: SummaryAgg::Average, weight_col: None });
    settle(&mut app);
    let fmt = ColumnFormatSpec { fmt_type: MetricFormatType::BdtLakh, decimals: 1 };
    let pivot = TransformStep::GroupByPivot {
        group_cols: vec!["Branch".into()],
        pivot_col: None,
        aggs: vec![AggSpec { target_col: "Balance".into(), func: AggFunc::Sum, weight_col: None, alias: "Bal_Sum".into(), format_spec: fmt }],
        sort_by_col: None,
        top_n: None,
    };
    app.push_step_to_active_tab(pivot, false);
    settle(&mut app);
    let after = app.tabs[idx].snapshot();
    assert_eq!(after.steps.len(), 1);
    assert!(after.col_formats.contains_key("Bal_Sum"));

    // undo the pivot: its step and its format go together; the earlier changes stay
    app.undo_steps();
    settle(&mut app);
    let t = &app.tabs[idx];
    assert!(t.steps.is_empty());
    assert!(!t.col_formats.contains_key("Bal_Sum"));
    assert_eq!(t.summary.get("Balance").map(|s| s.agg), Some(SummaryAgg::Average));
    assert_eq!(t.control_col.as_deref(), Some("Limit"));
    // undo the summary choice, then the control column (its baseline is recomputed)
    app.undo_steps();
    settle(&mut app);
    assert!(app.tabs[idx].summary.is_empty());
    app.undo_steps();
    settle(&mut app);
    let t = &app.tabs[idx];
    assert_eq!(t.control_col.as_deref(), Some("Balance"));
    assert_eq!(t.control_sum, Some(60.0));
    assert_eq!(t.baseline_control_sum, Some(60.0));
    assert!(!t.history.can_undo());

    // redo all three: back to the same state
    for _ in 0..3 {
        app.redo_steps();
        settle(&mut app);
    }
    let t = &app.tabs[idx];
    assert_eq!(format!("{:?}", t.snapshot().steps), format!("{:?}", after.steps));
    assert_eq!(t.control_col, after.control_col);
    assert_eq!(t.summary.get("Balance").map(|s| s.agg), Some(SummaryAgg::Average));
    assert_eq!(t.col_formats.get("Bal_Sum").map(|f| f.fmt_type), Some(MetricFormatType::BdtLakh));
}

#[test]
fn columns_dialog_formats_are_undone_with_their_step() {
    let mut app = test_app("fix7b");
    let src = parquet_source(&app.paths.root_dir.clone(), "loans", loans_frame());
    let idx = open_tab(&mut app, src, vec![]);
    let spec = ColumnFormatSpec { fmt_type: MetricFormatType::BdtCrore, decimals: 3 };
    app.columns.apply_formats = Some(vec![("Balance".into(), spec)]);
    app.push_step_to_active_tab(TransformStep::SelectAndReorderColumns { cols: vec!["Balance".into(), "Branch".into()] }, false);
    settle(&mut app);
    assert_eq!(app.tabs[idx].col_formats.get("Balance").map(|f| f.decimals), Some(3));
    app.undo_steps();
    settle(&mut app);
    assert!(app.tabs[idx].steps.is_empty());
    assert!(app.tabs[idx].col_formats.get("Balance").is_none(), "one undo takes back the step and its formats");
}
