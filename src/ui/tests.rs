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
