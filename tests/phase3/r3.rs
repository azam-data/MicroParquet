//! Phase 3 R3: streaming exports, one scan of the typed columns, cached baseline (synthetic data).

use micro_parquet::engine::*;
use micro_parquet::jobs::JobCtx;
use micro_parquet::models::*;
use micro_parquet::tab_jobs::*;
use polars::prelude::*;
use std::path::{Path, PathBuf};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mp_r3_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn big_csv(d: &Path, name: &str, rows: usize) -> PathBuf {
    let p = d.join(name);
    let mut body = String::from("Actnum,Balance,Brancd,Opndat\n");
    for i in 0..rows {
        let br = ["DHK", "CTG", "SYL"][i % 3];
        body.push_str(&format!("A{i},{}.25,{br},{:02}/09/2026\n", 1000 + i, 1 + i % 28));
    }
    std::fs::write(&p, body).unwrap();
    p
}

fn source_of(p: &Path) -> TabSource {
    TabSource::SingleFile {
        path: p.to_path_buf(),
        is_parquet: false,
        config: auto_detect_smart_schema(p).unwrap(),
    }
}

fn parquet_df(p: &Path) -> DataFrame {
    LazyFrame::scan_parquet(p, ScanArgsParquet::default()).unwrap().collect().unwrap()
}

fn csv_df(p: &Path) -> DataFrame {
    LazyCsvReader::new(p).with_has_header(true).finish().unwrap().collect().unwrap()
}

fn sum_of(df: &DataFrame, c: &str) -> f64 {
    df.column(c).unwrap().cast(&DataType::Float64).unwrap().f64().unwrap().sum().unwrap()
}

#[test]
fn r3_parquet_and_csv_exports_give_the_same_rows_and_sum_streamed_or_collected() {
    let d = dir("both");
    let src = source_of(&big_csv(&d, "loans.csv", 5000));
    let steps = vec![TransformStep::FilterRows {
        column: "Brancd".into(),
        operator: FilterOp::Eq,
        value: "DHK".into(),
        keep_matching: true,
    }];
    let lf = compile_tab_lazy(&src, &steps, &d).unwrap();

    let (ps, pc) = (d.join("streamed.parquet"), d.join("collected.parquet"));
    assert_eq!(export_to_parquet(lf.clone(), &ps, ParquetDialect::ArchiveZstd, ExportMode::Auto).unwrap(), ExportPath::Streamed);
    assert_eq!(export_to_parquet(lf.clone(), &pc, ParquetDialect::ArchiveZstd, ExportMode::CollectOnly).unwrap(), ExportPath::Collected);
    let (a, b) = (parquet_df(&ps), parquet_df(&pc));
    assert_eq!(a.height(), b.height());
    assert_eq!(a.height(), 1667);
    assert_eq!(sum_of(&a, "Balance"), sum_of(&b, "Balance"));
    assert!(a.equals_missing(&b), "streamed and collected Parquet hold the same data in the same order");

    let (cs, cc) = (d.join("streamed.csv"), d.join("collected.csv"));
    assert_eq!(export_to_csv(lf.clone(), &cs, false, ExportMode::Auto).unwrap(), ExportPath::Streamed);
    assert_eq!(export_to_csv(lf, &cc, false, ExportMode::CollectOnly).unwrap(), ExportPath::Collected);
    assert_eq!(
        std::fs::read(&cs).unwrap(),
        std::fs::read(&cc).unwrap(),
        "the CSV bytes are identical either way (header, date format, no scientific notation)"
    );
    let a = csv_df(&cs);
    assert_eq!(a.height(), 1667);
    assert_eq!(sum_of(&a, "Balance"), sum_of(&parquet_df(&ps), "Balance"));
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn r3_a_view_that_cannot_stream_falls_back_and_the_status_says_so() {
    let d = dir("fallback");
    let src = source_of(&big_csv(&d, "loans.csv", 300));
    // A window function per branch is not streamable.
    let lf = compile_tab_lazy(&src, &[], &d)
        .unwrap()
        .with_column(col("Balance").sum().over([col("Brancd")]).alias("Branch_Total"));
    let out = d.join("windowed.parquet");
    let how = export_to_parquet(lf.clone(), &out, ParquetDialect::PowerBiSnappy, ExportMode::Auto).unwrap();
    assert_eq!(how, ExportPath::Collected, "the window view cannot be streamed");
    let reference = d.join("reference.parquet");
    export_to_parquet(lf.clone(), &reference, ParquetDialect::PowerBiSnappy, ExportMode::CollectOnly).unwrap();
    assert!(parquet_df(&out).equals_missing(&parquet_df(&reference)));
    assert_eq!(parquet_df(&out).height(), 300);
    // CSV too
    let csv_out = d.join("windowed.csv");
    assert_eq!(export_to_csv(lf, &csv_out, true, ExportMode::Auto).unwrap(), ExportPath::Collected);
    assert_eq!(csv_df(&csv_out).height(), 300);

    // The export job reports which path it used.
    let o = export_tab_parquet(&src, &[], &d, &d.join("job.parquet"), ParquetDialect::ArchiveZstd, None, &JobCtx::inert()).unwrap();
    assert_eq!(o.path_used, Some(ExportPath::Streamed));
    assert!(o.message.contains("(streamed)"), "{}", o.message);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn r3_dr_cr_check_and_parse_failure_counts_share_one_scan() {
    let d = dir("scan");
    let p = d.join("gl.csv");
    // Balance carries Dr/Cr in a column with a sign convention set, plus two values that are not numbers.
    std::fs::write(&p, "Actnum,Balance,Opndat\nA1,100,01/09/2026\nA2,abc,02/09/2026\nA3,300,xx\nA4,400,04/09/2026\n").unwrap();
    let typed = |p: &Path| {
        let mut cfg = auto_detect_smart_schema(p).unwrap();
        for c in cfg.columns.iter_mut() {
            match c.name.as_str() {
                "Balance" => {
                    c.dtype = ColDataType::Float64;
                    c.sign_convention = SignConvention::Unset; // the Dr/Cr check applies
                }
                "Opndat" => c.dtype = ColDataType::DateDMY,
                _ => {}
            }
        }
        TabSource::SingleFile { path: p.to_path_buf(), is_parquet: false, config: cfg }
    };
    let src = typed(&p);
    let before = typed_scan_runs_on_this_thread();
    let lf = compile_tab_lazy(&src, &[], &d).unwrap(); // runs the Dr/Cr check
    let failures = source_parse_failures(&src, &d).unwrap(); // needs the same scan
    let again = compile_tab_lazy(&src, &[], &d).unwrap();
    let _ = (lf, again);
    assert_eq!(typed_scan_runs_on_this_thread() - before, 1, "one pass answers both questions, once");
    let names: Vec<&str> = failures.iter().map(|f| f.column.as_str()).collect();
    assert!(names.contains(&"Balance") && names.contains(&"Opndat"), "{failures:?}");
    assert_eq!(failures.iter().find(|f| f.column == "Balance").unwrap().count, 1);

    // A changed file is scanned again.
    std::fs::write(&p, "Actnum,Balance,Opndat\nA1,100,01/09/2026\nA2,200,02/09/2026\nA3,300,03/09/2026\nA4,400,04/09/2026\nA5,500,05/09/2026\n").unwrap();
    let src2 = typed(&p);
    let f2 = source_parse_failures(&src2, &d).unwrap();
    assert!(f2.is_empty());
    assert_eq!(typed_scan_runs_on_this_thread() - before, 2);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn r3_dr_cr_values_still_stop_the_load_with_the_same_message() {
    let d = dir("drcr");
    let p = d.join("gl.csv");
    std::fs::write(&p, "Actnum,Balance\nA1,100 Dr\nA2,50 Cr\n").unwrap();
    let mut cfg = auto_detect_smart_schema(&p).unwrap();
    for c in cfg.columns.iter_mut() {
        if c.name == "Balance" {
            c.dtype = ColDataType::Float64;
            c.sign_convention = SignConvention::Unset;
        }
    }
    let src = TabSource::SingleFile { path: p, is_parquet: false, config: cfg };
    let err = compile_tab_lazy(&src, &[], &d).err().expect("must refuse").to_string();
    assert!(err.contains("Column 'Balance' has Dr/Cr amounts"), "{err}");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn r3_baseline_is_computed_once_for_two_tabs_on_the_same_unchanged_file() {
    let d = dir("baseline");
    let p = big_csv(&d, "loans.csv", 100);
    let src = source_of(&p);
    // Tab 1 (no steps) already summed the raw file for its own totals and hands the sum over.
    let (first, o1) = source_baseline_sum(&src, Some("Balance"), &d);
    assert_eq!(o1, BaselineOrigin::Computed);
    let expect = (0..100).map(|i| 1000.25 + i as f64).sum::<f64>();
    assert!((first.unwrap() - expect).abs() < 1e-6, "{first:?} vs {expect}");
    // Tab 2 on the same file, with a step: no second read.
    let (second, o2) = source_baseline_sum(&src, Some("Balance"), &d);
    assert_eq!(o2, BaselineOrigin::Cached);
    assert_eq!(first, second);
    // A different control column is a different number.
    let (_, o3) = source_baseline_sum(&src, Some("Actnum"), &d);
    assert_eq!(o3, BaselineOrigin::Computed);
    // The file changes (size and time): computed again, and the new number is right.
    std::fs::write(&p, "Actnum,Balance,Brancd,Opndat\nZ1,5.50,DHK,01/09/2026\n").unwrap();
    let src_new = source_of(&p);
    let (third, o4) = source_baseline_sum(&src_new, Some("Balance"), &d);
    assert_eq!(o4, BaselineOrigin::Computed);
    assert_eq!(third, Some(5.5));
    // No control column: no sum and no scan.
    assert_eq!(source_baseline_sum(&src_new, None, &d).0, None);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn r3_loading_a_tab_stores_its_baseline_for_the_next_tab() {
    let d = dir("load_baseline");
    let p = big_csv(&d, "loans.csv", 50);
    let src = source_of(&p);
    let req = |tab_id: u64, steps: Vec<TransformStep>| LoadRequest {
        tab_id,
        title: format!("t{tab_id}"),
        source: src.clone(),
        steps,
        col_formats: Default::default(),
        temp_dir: d.clone(),
        reconcile_rate_used: None,
    };
    let t1 = load_tab(req(1, vec![]), &JobCtx::inert()).unwrap().tab;
    let step = TransformStep::FilterRows {
        column: "Brancd".into(),
        operator: FilterOp::Eq,
        value: "DHK".into(),
        keep_matching: true,
    };
    let t2 = load_tab(req(2, vec![step]), &JobCtx::inert()).unwrap().tab;
    assert_eq!(t1.baseline_control_sum, t2.baseline_control_sum, "both tabs compare against the raw file");
    assert!(t2.control_sum.unwrap() < t2.baseline_control_sum.unwrap());
    assert_eq!(source_baseline_sum(&src, Some("Balance"), &d).1, BaselineOrigin::Cached);
    std::fs::remove_dir_all(&d).ok();
}
