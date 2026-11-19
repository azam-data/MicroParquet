//! Phase 3 R2: the background job queue (synthetic jobs only).

use micro_parquet::jobs::*;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn wait_for<T: Send + 'static>(q: &JobQueue<T>, n: usize) -> Vec<JobResult<T>> {
    let mut out = Vec::new();
    let end = Instant::now() + Duration::from_secs(20);
    while out.len() < n && Instant::now() < end {
        out.extend(q.poll());
        std::thread::sleep(Duration::from_millis(5));
    }
    out
}

#[test]
fn r2_newer_job_for_same_tab_drops_the_older_result() {
    let q: JobQueue<&'static str> = JobQueue::new(|| {});
    let (gate_tx, gate_rx) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let saw_cancel = Arc::new(Mutex::new(false));
    let saw = saw_cancel.clone();
    // Job A blocks until released, then notices it was superseded.
    q.submit(JobKey::tab(JobKind::Refresh, 7), "Refresh A", false, move |ctx| {
        started_tx.send(()).ok();
        gate_rx.recv().ok();
        *saw.lock().unwrap() = ctx.is_cancelled();
        ctx.check()?;
        Ok("old")
    });
    started_rx.recv_timeout(Duration::from_secs(10)).expect("A started");
    q.submit(JobKey::tab(JobKind::Refresh, 7), "Refresh B", false, |_| Ok("new"));
    gate_tx.send(()).unwrap();
    let got = wait_for(&q, 1);
    std::thread::sleep(Duration::from_millis(100));
    let late = q.poll();
    assert_eq!(got.len(), 1, "only the newest job's result is delivered");
    assert!(matches!(got[0].outcome, Outcome::Done("new")));
    assert!(late.is_empty(), "the superseded job's result never shows up");
    assert!(*saw_cancel.lock().unwrap(), "the older job was told to stop");
}

#[test]
fn r2_queued_job_replaced_before_it_starts_never_runs() {
    let q: JobQueue<u32> = JobQueue::new(|| {});
    let (gate_tx, gate_rx) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let ran = Arc::new(Mutex::new(Vec::new()));
    // The worker is busy with a blocker, so the next two Refresh jobs for tab 1 queue up.
    q.submit(JobKey::unique(JobKind::Export), "blocker", false, move |_| {
        started_tx.send(()).ok();
        gate_rx.recv().ok();
        Ok(0)
    });
    started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    for n in 1..=3u32 {
        let ran = ran.clone();
        q.submit(JobKey::tab(JobKind::Refresh, 1), format!("r{n}"), false, move |_| {
            ran.lock().unwrap().push(n);
            Ok(n)
        });
    }
    gate_tx.send(()).unwrap();
    let got = wait_for(&q, 2);
    assert_eq!(got.len(), 2);
    let vals: Vec<u32> = got.iter().filter_map(|r| if let Outcome::Done(v) = r.outcome { Some(v) } else { None }).collect();
    assert_eq!(vals, vec![0, 3], "blocker, then only the newest refresh");
    assert_eq!(*ran.lock().unwrap(), vec![3], "the two replaced jobs did not run");
}

#[test]
fn r2_other_tabs_and_unique_jobs_do_not_supersede_each_other() {
    let q: JobQueue<u32> = JobQueue::new(|| {});
    q.submit(JobKey::tab(JobKind::Refresh, 1), "t1", false, |_| Ok(1));
    q.submit(JobKey::tab(JobKind::Refresh, 2), "t2", false, |_| Ok(2));
    q.submit(JobKey::unique(JobKind::Export), "e1", true, |_| Ok(3));
    q.submit(JobKey::unique(JobKind::Export), "e2", true, |_| Ok(4));
    let mut vals: Vec<u32> = wait_for(&q, 4)
        .into_iter()
        .filter_map(|r| if let Outcome::Done(v) = r.outcome { Some(v) } else { None })
        .collect();
    vals.sort();
    assert_eq!(vals, vec![1, 2, 3, 4]);
}

#[test]
fn r2_cancel_removes_the_temp_file_and_writes_no_output() {
    let dir = std::env::temp_dir().join(format!("mp_r2_cancel_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let final_path = dir.join("out.parquet");
    let q: JobQueue<()> = JobQueue::new(|| {});
    let (wrote_tx, wrote_rx) = mpsc::channel::<std::path::PathBuf>();
    let fp = final_path.clone();
    let id = q.submit(JobKey::unique(JobKind::Export), "Export out.parquet", true, move |ctx| {
        write_atomic(&fp, ctx, |tmp| {
            std::fs::write(tmp, b"partial")?;
            wrote_tx.send(tmp.to_path_buf()).ok();
            // keeps "writing" until the user cancels
            for _ in 0..2000 {
                ctx.check()?;
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(())
        })
    });
    let tmp_path = wrote_rx.recv_timeout(Duration::from_secs(10)).expect("temp file written");
    assert!(tmp_path.exists(), "the partial file exists while the job runs");
    assert!(!final_path.exists(), "the final name is not used before the job succeeds");
    let active = q.active_long().expect("long job is visible to the status bar");
    assert_eq!(active.id, id);
    q.cancel(id);
    let got = wait_for(&q, 1);
    assert!(matches!(got[0].outcome, Outcome::Cancelled));
    assert_eq!(got[0].status_line().unwrap(), "Export out.parquet cancelled.");
    assert!(!tmp_path.exists(), "the temp file is removed on cancel");
    assert!(!final_path.exists(), "no output file after a cancel");
    let left: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
    assert!(left.is_empty(), "nothing is left in the folder: {left:?}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn r2_success_renames_the_temp_file_and_replaces_an_old_one() {
    let dir = std::env::temp_dir().join(format!("mp_r2_ok_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let final_path = dir.join("out.csv");
    std::fs::write(&final_path, b"old").unwrap();
    let q: JobQueue<usize> = JobQueue::inline();
    let fp = final_path.clone();
    q.submit(JobKey::unique(JobKind::Export), "Export", true, move |ctx| {
        write_atomic(&fp, ctx, |tmp| {
            std::fs::write(tmp, b"new data")?;
            Ok(8usize)
        })
    });
    let got = q.poll();
    assert!(matches!(got[0].outcome, Outcome::Done(8)));
    assert_eq!(std::fs::read(&final_path).unwrap(), b"new data");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no temp file left");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn r2_failed_write_leaves_no_temp_file() {
    let dir = std::env::temp_dir().join(format!("mp_r2_fail_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let final_path = dir.join("out.xlsx");
    let ctx = JobCtx::inert();
    let r: anyhow::Result<()> = write_atomic(&final_path, &ctx, |tmp| {
        std::fs::write(tmp, b"half")?;
        anyhow::bail!("disk full")
    });
    assert!(r.is_err());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn r2_job_error_and_panic_reach_the_status_bar_as_text() {
    let q: JobQueue<u32> = JobQueue::new(|| {});
    q.submit(JobKey::unique(JobKind::Load), "Open Loans.csv", true, |_| {
        Err(anyhow::anyhow!("Balance: 3 values are not numbers").context("Could not read the file"))
    });
    q.submit(JobKey::unique(JobKind::Refresh), "Refresh", false, |_| -> anyhow::Result<u32> {
        let v: Vec<u32> = Vec::new();
        Ok(v[3])
    });
    let mut got = wait_for(&q, 2);
    got.sort_by_key(|r| r.id);
    let l0 = got[0].status_line().unwrap();
    assert!(l0.starts_with("Open Loans.csv failed: Could not read the file"), "{l0}");
    assert!(l0.contains("Balance: 3 values are not numbers"), "{l0}");
    let l1 = got[1].status_line().unwrap();
    assert!(l1.starts_with("Refresh failed: internal error"), "{l1}");
    // the worker survived the panic
    q.submit(JobKey::unique(JobKind::Refresh), "again", false, |_| Ok(5));
    assert!(matches!(wait_for(&q, 1)[0].outcome, Outcome::Done(5)));
}

#[test]
fn r2_inline_queue_runs_on_the_calling_thread() {
    let q: JobQueue<std::thread::ThreadId> = JobQueue::inline();
    q.submit(JobKey::tab(JobKind::Load, 1), "x", true, |_| Ok(std::thread::current().id()));
    let got = q.poll();
    assert_eq!(got.len(), 1);
    match got[0].outcome {
        Outcome::Done(id) => assert_eq!(id, std::thread::current().id()),
        _ => panic!("inline job should succeed"),
    }
    assert!(!q.any_busy());
}

#[test]
fn r2_wake_callback_runs_when_a_result_is_ready() {
    let (tx, rx) = mpsc::channel::<()>();
    let tx = Mutex::new(tx);
    let q: JobQueue<u32> = JobQueue::new(move || {
        tx.lock().unwrap().send(()).ok();
    });
    q.submit(JobKey::unique(JobKind::Stats), "s", false, |_| Ok(1));
    rx.recv_timeout(Duration::from_secs(10)).expect("wake called");
    assert_eq!(q.poll().len(), 1);
}

// ---------------------------------------------------------------- the job functions (R2)

mod functions {
    use micro_parquet::batch::run_folder_mirror_batch_ctx;
    use micro_parquet::engine::{auto_detect_smart_schema, compile_tab_lazy};
    use micro_parquet::exporters::split_and_export_by_column_ctx;
    use micro_parquet::jobs::*;
    use micro_parquet::models::*;
    use micro_parquet::tab_jobs::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mp_r2_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn csv(d: &std::path::Path, name: &str) -> PathBuf {
        let p = d.join(name);
        std::fs::write(
            &p,
            "Actnum,Balance,Brancd\nA1,1000.50,DHK\nA2,2000.25,DHK\nA3,3000.00,CTG\nA4,4000.00,CTG\n",
        )
        .unwrap();
        p
    }

    fn source(p: &std::path::Path) -> TabSource {
        TabSource::SingleFile {
            path: p.to_path_buf(),
            is_parquet: false,
            config: auto_detect_smart_schema(p).unwrap(),
        }
    }

    #[test]
    fn r2_load_and_refresh_jobs_give_the_same_numbers_the_old_inline_code_did() {
        let d = dir("load");
        let src = source(&csv(&d, "loans.csv"));
        let loaded = load_tab(
            LoadRequest {
                tab_id: 5,
                title: "loans".into(),
                source: src.clone(),
                steps: vec![],
                col_formats: HashMap::new(),
                temp_dir: d.clone(),
                reconcile_rate_used: None,
            },
            &JobCtx::inert(),
        )
        .unwrap();
        let tab = loaded.tab;
        assert_eq!(tab.id, 5);
        assert_eq!(tab.total_rows, 4);
        assert_eq!(tab.control_col.as_deref(), Some("Balance"));
        assert_eq!(tab.control_sum, Some(10000.75));
        assert_eq!(tab.baseline_control_sum, Some(10000.75));
        assert_eq!(tab.cached_slice.height(), 4);
        assert!(loaded.status.starts_with("Tab created and evaluated out-of-core."));

        let step = TransformStep::FilterRows {
            column: "Brancd".into(),
            operator: FilterOp::Eq,
            value: "DHK".into(),
            keep_matching: true,
        };
        let r = refresh_tab(
            RefreshRequest {
                tab_id: 5,
                source: src,
                steps: vec![step],
                offset: 0,
                limit: 100,
                control_col: Some("Balance".into()),
                recalculate_baseline: true,
                check_join: false,
                added_status: Some("Step added: Filter.".into()),
                temp_dir: d.clone(),
                validate: true,
                exec_steps: 0,
            },
            &JobCtx::inert(),
        )
        .unwrap();
        assert!(r.error.is_none());
        assert_eq!(r.totals.rows, Some(2));
        assert_eq!(r.totals.sum, Some(3000.75));
        assert_eq!(r.baseline, Some(Some(10000.75)), "baseline recounted from the raw source");
        assert_eq!(r.slice.unwrap().height(), 2);
        assert!(r.status.unwrap().starts_with("Step added: Filter."));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn r2_refresh_of_an_unreadable_source_is_a_result_with_the_reason() {
        let d = dir("bad");
        let missing = d.join("gone.csv");
        let src = TabSource::SingleFile {
            path: missing,
            is_parquet: true,
            config: IngestionConfig::default(),
        };
        let r = refresh_tab(
            RefreshRequest {
                tab_id: 1,
                source: src,
                steps: vec![],
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
        // The totals show "—" with the reason instead of keeping an older number.
        assert!(r.totals.rows.is_none() && r.totals.sum.is_none(), "{:?}", r.totals);
        assert!(r.totals.note.as_deref().unwrap_or("").starts_with("Totals unavailable"), "{:?}", r.totals);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn r2_cancelled_exports_write_no_file() {
        let d = dir("cancel_export");
        let src = source(&csv(&d, "loans.csv"));
        let out = d.join("out.parquet");
        let ctx = JobCtx::inert();
        ctx.request_cancel();
        let err = export_tab_parquet(&src, &[], &d, &out, ParquetDialect::ArchiveZstd, None, &ctx)
            .err()
            .expect("cancelled");
        assert!(err.downcast_ref::<Cancelled>().is_some());
        assert!(!out.exists());
        // The same export without a cancel works and leaves no temp file behind.
        let ok = export_tab_parquet(&src, &[], &d, &out, ParquetDialect::ArchiveZstd, None, &JobCtx::inert())
            .unwrap();
        assert!(ok.message.starts_with("✅ Exported view to"));
        assert!(out.exists());
        let names: Vec<String> = std::fs::read_dir(&d)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|n| !n.ends_with(".part")), "{names:?}");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn r2_cancelled_split_export_removes_what_it_wrote_and_writes_no_manifest() {
        let d = dir("cancel_split");
        let src = source(&csv(&d, "loans.csv"));
        let lf = compile_tab_lazy(&src, &[], &d).unwrap();
        let out = d.join("split");
        let ctx = JobCtx::inert();
        ctx.request_cancel();
        let r = split_and_export_by_column_ctx(
            lf, "Brancd", &out, SplitExportFormat::Csv, ParquetDialect::ArchiveZstd, false,
            Some("Balance"), &ctx,
        );
        assert!(r.is_err());
        assert!(r.err().unwrap().downcast_ref::<Cancelled>().is_some());
        let files = std::fs::read_dir(&out).map(|r| r.count()).unwrap_or(0);
        assert_eq!(files, 0, "no partition file and no manifest");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn r2_cancelled_batch_logs_it_and_converts_nothing_more() {
        let d = dir("cancel_batch");
        let input = d.join("in");
        let output = d.join("out");
        std::fs::create_dir_all(&input).unwrap();
        csv(&input, "a.csv");
        csv(&input, "b.csv");
        let ctx = JobCtx::inert();
        ctx.request_cancel();
        let rep = run_folder_mirror_batch_ctx(
            &input, &output, BatchDirection::CsvToParquet, BatchSchemaStrategy::SmartSafeAutoDetect, None,
            ParquetDialect::ArchiveZstd, false, &d, &ctx,
        )
        .unwrap();
        assert!(rep.cancelled);
        assert_eq!(rep.files_converted, 0);
        assert!(rep.log.iter().any(|r| r.status == "CANCELLED"));
        assert!(output.join("batch_log.csv").exists(), "the log says the run was cancelled");
        assert!(!output.join("a.parquet").exists() && !output.join("b.parquet").exists());
        // And an uncancelled run converts both and leaves no temp files.
        let rep = run_folder_mirror_batch_ctx(
            &input, &output, BatchDirection::CsvToParquet, BatchSchemaStrategy::SmartSafeAutoDetect, None,
            ParquetDialect::ArchiveZstd, false, &d, &JobCtx::inert(),
        )
        .unwrap();
        assert!(!rep.cancelled);
        assert_eq!(rep.files_converted, 2);
        let parts = std::fs::read_dir(&output)
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().ends_with(".part"))
            .count();
        assert_eq!(parts, 0);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn r2_schema_check_leaves_the_tab_alone_when_it_fails() {
        let d = dir("schema");
        let src = source(&csv(&d, "loans.csv"));
        // The check builds the pipeline with the new settings; a missing file is an error.
        let bad = TabSource::SingleFile {
            path: d.join("missing.csv"),
            is_parquet: false,
            config: auto_detect_smart_schema(&csv(&d, "loans.csv")).unwrap(),
        };
        let r = check_schema_for_tab(
            SchemaRequest { tab_id: 1, new_source: bad, steps: vec![], temp_dir: d.clone(), already_broken: vec![] },
            &JobCtx::inert(),
        );
        let e = r.err().expect("a missing file must fail the check, not claim success");
        assert!(format!("{e:#}").contains("missing.csv"), "{e:#}");
        let ok = check_schema_for_tab(
            SchemaRequest { tab_id: 1, new_source: src, steps: vec![], temp_dir: d.clone(), already_broken: vec![] },
            &JobCtx::inert(),
        )
        .unwrap();
        assert!(ok.status.starts_with("Schema applied and tab reloaded."));
        std::fs::remove_dir_all(&d).ok();
    }
}
