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
