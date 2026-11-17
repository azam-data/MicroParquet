//! Phase 3 fixes, part 2 (layout round): library-level checks.

use micro_parquet::models::*;
use polars::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mp_fix2_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn unzip_entry(zip: &Path, entry: &str) -> String {
    let out = std::process::Command::new("unzip").arg("-p").arg(zip).arg(entry).output().expect("unzip must be installed");
    assert!(out.status.success(), "unzip {entry}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn parquet(d: &Path, name: &str, mut df: DataFrame) -> TabSource {
    let p = d.join(format!("{name}.parquet"));
    ParquetWriter::new(std::fs::File::create(&p).unwrap()).finish(&mut df).unwrap();
    TabSource::SingleFile { path: p, is_parquet: true, config: IngestionConfig::default() }
}

// ------------------------------------------------------------------------------------ A

#[test]
fn a_totals_row_that_cannot_be_computed_keeps_the_sheet_without_the_totals_row() {
    use micro_parquet::exporters::*;
    let d = dir("a");
    let mk = |name: &str, rows: &[f64]| {
        let n = rows.len();
        let labels: Vec<String> = (0..n).map(|i| format!("B{i}")).collect();
        parquet(
            &d,
            name,
            DataFrame::new(vec![
                Series::new("Branch".into(), labels).into(),
                Series::new("Balance".into(), rows).into(),
            ])
            .unwrap(),
        )
    };
    let good = mk("good", &[10.0, 20.0]);
    let bad = mk("bad", &[1.0, 2.0, 3.0]);
    let formats = HashMap::new();
    let spec: HashMap<String, SummarySpec> = HashMap::new();
    let tabs = vec![
        ScaledTabToExport { title: "Good".into(), source: &good, steps: &[], col_formats: &formats, summary: Some(&spec), control_col: None },
        ScaledTabToExport { title: "NoTotals".into(), source: &bad, steps: &[], col_formats: &formats, summary: Some(&spec), control_col: None },
    ];
    let out = d.join("out.xlsx");
    let scale = XlsxScale { num_format: NumberDisplayFormat::Raw, decimals: 2 };
    // the totals computation fails for the tab with 3 rows only
    let summarize = |df: &DataFrame, over: Option<&HashMap<String, SummarySpec>>, ctl: Option<&str>| {
        if df.height() == 3 {
            anyhow::bail!("The totals row could not be computed: simulated failure");
        }
        let cols: Vec<(String, DataType)> = df.get_columns().iter().map(|s| (s.name().to_string(), s.dtype().clone())).collect();
        let specs = micro_parquet::summary::effective_specs(&cols, over.unwrap(), ctl);
        micro_parquet::summary::compute_summary(df.clone().lazy(), &specs)
    };
    let rep = export_scaled_tabs_with(&tabs, &out, &d, scale, &summarize).unwrap();

    // both sheets are in the workbook; nothing was dropped
    let wb = unzip_entry(&out, "xl/workbook.xml");
    assert!(wb.contains("name=\"1_Good\"") && wb.contains("name=\"2_NoTotals\""), "{wb}");
    assert_eq!(rep.sheets.len(), 2, "{rep:?}");
    assert!(rep.errors.is_empty(), "no tab failed: {rep:?}");
    assert_eq!(rep.totals_errors.len(), 1);
    assert_eq!(rep.totals_errors[0].0, "NoTotals");

    // the good sheet has its totals rows (blank row, labels, values: 2 data rows + header → rows 5, 6),
    // the other has its 3 data rows and nothing after them
    let s1 = unzip_entry(&out, "xl/worksheets/sheet1.xml");
    let s2 = unzip_entry(&out, "xl/worksheets/sheet2.xml");
    assert!(s1.contains("r=\"A5\"") || s1.contains("r=\"B5\""), "good sheet carries totals: {s1}");
    assert!(s2.contains("r=\"B4\""), "all 3 data rows are there: {s2}");
    assert!(!s2.contains("r=\"B6\"") && !s2.contains("r=\"A6\"") && !s2.contains("r=\"B7\""), "no totals rows: {s2}");

    // the reason is on Export_Errors and in the status text
    assert!(wb.contains("name=\"Export_Errors\""), "{wb}");
    let strings = unzip_entry(&out, "xl/sharedStrings.xml");
    assert!(strings.contains("WITHOUT its totals row") && strings.contains("simulated failure") && strings.contains("NoTotals"), "{strings}");
    let status = rep.summary();
    assert!(status.contains("NoTotals") && status.contains("WITHOUT its totals row") && status.contains("simulated failure"), "{status}");
    assert!(!status.contains("failed and are listed"), "a kept sheet is not a failed tab: {status}");
}

#[test]
fn a_failed_tab_and_a_missing_totals_row_are_listed_separately() {
    use micro_parquet::exporters::*;
    let d = dir("a2");
    let df = DataFrame::new(vec![Series::new("Balance".into(), &[1.0f64]).into()]).unwrap();
    let tab = XlsxTab { title: "Loans".into(), df, col_formats: HashMap::new(), summary: Vec::new() };
    let rep = write_xlsx_tabs_noted(
        vec![tab],
        vec![("Broken".into(), "cannot read".into())],
        vec![("Loans".into(), "no totals".into())],
        &d.join("o.xlsx"),
        1000,
        None,
    )
    .unwrap();
    let s = rep.summary();
    assert!(s.contains("1 tab(s) failed") && s.contains("Broken") && s.contains("'Loans' was exported WITHOUT"), "{s}");
}

// ------------------------------------------------------------------------------------ B

#[test]
fn b_startup_sweep_deletes_recorded_part_files_and_keeps_the_locked_ones() {
    use micro_parquet::jobs::*;
    let d = dir("b");
    let root = d.join("portable_data");
    std::fs::create_dir_all(&root).unwrap();
    // one that can be deleted, one that cannot (a directory stands in for a file Windows holds),
    // one that is already gone, and one that is not ours and must never be deleted
    let ok = d.join(".Loans.xlsx.1-1.part");
    std::fs::write(&ok, b"partial").unwrap();
    let locked = d.join(".Loans.xlsx.1-2.part");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::write(locked.join("inner"), b"x").unwrap();
    let gone = d.join(".Loans.xlsx.1-3.part");
    let precious = d.join("Balance_Sheet.xlsx");
    std::fs::write(&precious, b"keep me").unwrap();

    record_pending_parts(&root, &[ok.clone(), locked.clone(), gone.clone()]).unwrap();
    // recording twice does not duplicate; a hand-edited list may contain any path
    record_pending_parts(&root, &[ok.clone()]).unwrap();
    let list = std::fs::read_to_string(root.join(PENDING_PARTS_FILE)).unwrap();
    assert_eq!(list.lines().count(), 3, "{list}");
    std::fs::write(root.join(PENDING_PARTS_FILE), format!("{list}{}\n", precious.display())).unwrap();

    let rep = sweep_pending_parts(&root);
    assert_eq!(rep.deleted, 1, "{rep:?}");
    assert_eq!(rep.still_locked.len(), 1, "{rep:?}");
    assert!(rep.still_locked[0].contains(".Loans.xlsx.1-2.part"));
    assert!(!ok.exists(), "the deletable file is deleted");
    assert!(locked.exists(), "the locked one stays");
    assert!(precious.exists(), "a file that is not a .part file is never deleted");
    let msg = rep.message().unwrap();
    assert!(msg.contains("1 unfinished output file(s)") && msg.contains("tried again at the next start"), "{msg}");
    // the list keeps only the locked file
    let left = std::fs::read_to_string(root.join(PENDING_PARTS_FILE)).unwrap();
    assert_eq!(left.trim(), locked.display().to_string());

    // once it can be deleted (the lock is gone), the next start deletes it and the list goes away
    std::fs::remove_dir_all(&locked).unwrap();
    std::fs::write(&locked, b"now a plain file").unwrap();
    let rep2 = sweep_pending_parts(&root);
    assert_eq!(rep2.deleted, 1, "{rep2:?}");
    assert!(rep2.still_locked.is_empty());
    assert!(!locked.exists());
    assert!(!root.join(PENDING_PARTS_FILE).exists(), "an empty list is removed");
    // and with no list there is nothing to say
    assert_eq!(sweep_pending_parts(&root).message(), None);
}

// ------------------------------------------------------------------------------ layout 5

#[test]
fn l5_text_is_mono_by_what_it_holds_not_by_the_column_name() {
    use micro_parquet::table_layout::text_column_is_mono;
    let col = |v: &[&str]| text_column_is_mono(v.iter().map(|s| Some(*s)));
    // codes, dates and references: digits, '-', '/', '.' and spaces
    assert!(col(&["20400-01", "20400-02", "10100-11"]));
    assert!(col(&["16/11/2026", "01/12/2026"]));
    assert!(col(&["2026.11 07", "12 3-4/5.6"]));
    // words, letters, commas and other signs are ordinary text
    assert!(!col(&["FIXED TERM DEPOSIT", "SAVINGS"]));
    assert!(!col(&["20400-01", "20400-0A"]), "one letter makes it text");
    assert!(!col(&["12,345.67", "1,000"]), "a comma is not on the list");
    assert!(!col(&["(1,234)", "+5"]));
    assert!(!col(&["Dr 100"]));
    // blanks and nulls are ignored, but an all-blank column is not mono
    assert!(text_column_is_mono(vec![None, Some("20400-01"), Some("  "), Some("")]));
    assert!(!text_column_is_mono(vec![None, Some(""), Some(" ")]));
    assert!(!text_column_is_mono(Vec::<Option<&str>>::new()));
    // only the first 100 rows are looked at
    let mut many: Vec<Option<&str>> = vec![Some("20400-01"); 100];
    many.push(Some("WORDS AFTER THE SAMPLE"));
    assert!(text_column_is_mono(many));
}

// ------------------------------------------------------------------------------ layout 6

type Reports = std::rc::Rc<std::cell::RefCell<Vec<(String, u64, u64)>>>;

fn collecting() -> (Reports, impl Fn(&str, u64, u64) + 'static) {
    let seen: Reports = Default::default();
    let s2 = seen.clone();
    (seen, move |what: &str, read, total| s2.borrow_mut().push((what.to_string(), read, total)))
}

/// A CSV with Windows line ends (a count of "length + 1" per line would fall short of the file).
fn crlf_csv(d: &Path, rows: usize) -> (PathBuf, u64) {
    let p = d.join("crlf.csv");
    let mut body = String::from("Actnum,Balance,Brancd\r\n");
    for i in 0..rows {
        body.push_str(&format!("A{i},{}.25,DHK\r\n", 1000 + i));
    }
    std::fs::write(&p, &body).unwrap();
    let len = std::fs::metadata(&p).unwrap().len();
    (p, len)
}

/// The reports of one loop are: a start at 0, steadily more bytes of the same file size, the
/// whole file at the end, then the "finished" marker (file size 0). A call may run the loop more
/// than once (the file is read again for another check); each run is checked on its own.
fn assert_exact_byte_progress(seen: &Reports, file_len: u64, what: &str) {
    let r = seen.borrow();
    let mut loops = 0;
    let mut current: Vec<(String, u64, u64)> = Vec::new();
    for rep in r.iter() {
        if rep.2 == 0 {
            // the finished marker
            assert_eq!(rep.1, file_len, "{what}: the loop says it is finished: {rep:?}");
            assert!(current.len() >= 2, "{what}: a start and progress came before the end: {current:?}");
            assert_eq!(current[0].1, 0, "{what}: starts at 0 bytes");
            let mut last = 0;
            for (_, read, total) in &current {
                assert_eq!(*total, file_len, "{what}: every report names the real file size");
                assert!(*read >= last && *read <= file_len, "{what}: bytes read only grow and never pass the file: {read} after {last}");
                last = *read;
            }
            assert_eq!(last, file_len, "{what}: the last report is the whole file ({last} of {file_len})");
            loops += 1;
            current.clear();
        } else {
            current.push(rep.clone());
        }
    }
    assert!(current.is_empty(), "{what}: every loop ends with the finished marker");
    assert!(loops >= 1, "{what}: reported at least once: {r:?}");
}

#[test]
fn l6_line_counting_reports_the_real_bytes_read() {
    use micro_parquet::engine::{count_nonempty_lines, with_byte_progress};
    let d = dir("l6count");
    let (p, len) = crlf_csv(&d, 60_000);
    assert!(len > 1_000_000, "several 256 KB steps");
    let (seen, sink) = collecting();
    let n = with_byte_progress(sink, || count_nonempty_lines(&p)).unwrap();
    assert_eq!(n, 60_001);
    assert_exact_byte_progress(&seen, len, "line counting");
    assert!(seen.borrow().iter().any(|(_, r, t)| *t > 0 && *r > 0 && *r < *t), "a percentage strictly between 0 and 100 was reported");
}

#[test]
fn l6_healing_reports_the_real_bytes_read_even_with_windows_line_ends() {
    use micro_parquet::engine::{heal_broken_csv_file, with_byte_progress};
    let d = dir("l6heal");
    let (p, len) = crlf_csv(&d, 60_000);
    let (seen, sink) = collecting();
    let healed = with_byte_progress(sink, || heal_broken_csv_file(&p, &d, b',', 0)).unwrap();
    assert!(healed.exists());
    assert_exact_byte_progress(&seen, len, "healing");
}

#[test]
fn l6_transcoding_reports_the_real_bytes_read() {
    use micro_parquet::engine::{auto_detect_smart_schema_with, build_lazy_pipeline, with_byte_progress};
    let d = dir("l6enc");
    let (p, len) = crlf_csv(&d, 60_000);
    let mut cfg = IngestionConfig::default();
    cfg.encoding = CsvTextEncoding::Windows1252;
    let cfg = auto_detect_smart_schema_with(&p, &cfg).unwrap();
    let (seen, sink) = collecting();
    let _lf = with_byte_progress(sink, || build_lazy_pipeline(&p, false, &cfg, &d)).unwrap();
    // the whole file is read once for the conversion
    assert_exact_byte_progress(&seen, len, "transcoding");
    assert!(seen.borrow().iter().any(|(w, _, _)| w.contains("encoding")));
}

#[test]
fn l6_without_a_sink_nothing_is_reported_and_nothing_breaks() {
    use micro_parquet::engine::{count_nonempty_lines, with_byte_progress};
    let d = dir("l6none");
    let (p, _) = crlf_csv(&d, 100);
    assert_eq!(count_nonempty_lines(&p).unwrap(), 101);
    // a sink only lives inside its call
    let (seen, sink) = collecting();
    with_byte_progress(sink, || ());
    count_nonempty_lines(&p).unwrap();
    assert!(seen.borrow().is_empty(), "the sink was removed after its call");
}

#[test]
fn l6_a_percentage_exists_only_for_bytes_read() {
    use micro_parquet::jobs::JobCtx;
    let ctx = JobCtx::inert();
    ctx.set_stages(&["Reading", "Parsing", "Checking"]);
    let p = ctx.progress();
    assert_eq!((p.stage, p.fraction, p.bytes), (0, None, None), "a stage alone has no percentage");
    assert_eq!(p.stages, ["Reading", "Parsing", "Checking"]);
    ctx.set_bytes("Counting the file's lines", 1, 4);
    let p = ctx.progress();
    assert_eq!(p.fraction, Some(0.25));
    assert_eq!(p.bytes, Some((1, 4)));
    assert_eq!(p.text, "Counting the file's lines");
    // the loop finishes: the percentage goes and the stage name is back
    ctx.set_bytes("Counting the file's lines", 4, 0);
    let p = ctx.progress();
    assert_eq!((p.fraction, p.bytes, p.text.as_str()), (None, None, "Reading"));
    // a new stage clears it as well
    ctx.set_bytes("x", 2, 4);
    ctx.set_stage(1);
    let p = ctx.progress();
    assert_eq!((p.stage, p.fraction, p.bytes, p.text.as_str()), (1, None, None, "Parsing"));
    ctx.set_stage(9);
    assert_eq!(ctx.progress().stage, 2, "past the last stage stays on the last");
}

#[test]
fn l6_a_load_goes_through_three_stages_and_never_shows_an_unfounded_percentage() {
    use micro_parquet::jobs::JobCtx;
    use micro_parquet::tab_jobs::{load_tab, LoadRequest};
    let d = dir("l6load");
    let (p, _) = crlf_csv(&d, 150_000);
    let cfg = micro_parquet::engine::auto_detect_smart_schema(&p).unwrap();
    let req = LoadRequest {
        tab_id: 1,
        title: "crlf.csv".into(),
        source: TabSource::SingleFile { path: p.clone(), is_parquet: false, config: cfg },
        steps: vec![],
        col_formats: Default::default(),
        temp_dir: d.clone(),
        reconcile_rate_used: None,
    };
    let ctx = JobCtx::inert();
    let worker_ctx = ctx.clone();
    let worker = std::thread::spawn(move || load_tab(req, &worker_ctx).map(|_| ()));
    // watch what the load reports while it runs
    let mut seen = Vec::new();
    while !worker.is_finished() {
        seen.push(ctx.progress());
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    worker.join().expect("no panic").expect("the load works");
    seen.push(ctx.progress());
    let mut last_stage = 0;
    for pr in &seen {
        if pr.stages.is_empty() {
            continue; // before the load named its stages
        }
        assert_eq!(pr.stages, ["Reading", "Parsing", "Checking"]);
        assert!(pr.stage >= last_stage, "stages only move forward");
        last_stage = pr.stage;
        if pr.fraction.is_some() {
            let (read, total) = pr.bytes.expect("a percentage comes with the bytes it is made of");
            assert!(read <= total && total > 0);
            assert_eq!(pr.fraction, Some((read as f64 / total as f64) as f32), "the fraction is exactly bytes read / file size");
        }
    }
    let end = ctx.progress();
    assert_eq!((end.stage, end.fraction, end.bytes), (2, None, None), "ends on Checking with no percentage left over");
}

#[test]
fn l6_elapsed_time_reads_plainly() {
    use micro_parquet::jobs::format_elapsed;
    use std::time::Duration;
    assert_eq!(format_elapsed(Duration::from_millis(900)), "0 s");
    assert_eq!(format_elapsed(Duration::from_secs(7)), "7 s");
    assert_eq!(format_elapsed(Duration::from_secs(65)), "1 min 05 s");
    assert_eq!(format_elapsed(Duration::from_secs(3720)), "1 h 02 min");
}

#[test]
fn l6_the_active_job_carries_stage_labels_and_the_time_it_has_run() {
    use micro_parquet::jobs::{JobKey, JobKind, JobQueue};
    let q: JobQueue<u8> = JobQueue::new(|| {});
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    let (up_tx, up_rx) = std::sync::mpsc::channel::<()>();
    q.submit(JobKey::unique(JobKind::Load), "Opening loans.csv", true, move |ctx| {
        ctx.set_stages(&["Reading", "Parsing", "Checking"]);
        ctx.set_stage(1);
        let _ = up_tx.send(());
        let _ = go_rx.recv_timeout(std::time::Duration::from_secs(10));
        Ok(0)
    });
    up_rx.recv_timeout(std::time::Duration::from_secs(10)).expect("the job is running");
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let job = q.active_long().expect("a long job is running");
    assert_eq!(job.stage_labels(), ["Reading 1/3", "Parsing 2/3", "Checking 3/3"]);
    assert_eq!(job.progress.stage, 1);
    assert_eq!(job.percent(), None, "no percentage without bytes");
    assert!(job.elapsed() >= std::time::Duration::from_secs(1), "{:?}", job.elapsed());
    let _ = go_tx.send(());
}
