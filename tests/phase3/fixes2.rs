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

