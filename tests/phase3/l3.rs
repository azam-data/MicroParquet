//! Phase 3 L3: Thousands, Millions and Lakh scales (synthetic numbers only).

use micro_parquet::exporters::*;
use micro_parquet::models::*;
use polars::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mp_l3_{}_{}", name, std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn spec(t: MetricFormatType) -> ColumnFormatSpec {
    ColumnFormatSpec { fmt_type: t, decimals: 2 }
}

#[test]
fn l3_25_million_taka_in_every_scale() {
    let v = 25_000_000.0;
    let f = |t: MetricFormatType| format_metric_value(v, "Balance", Some(&spec(t)), NumberDisplayFormat::Raw, 2);
    assert_eq!(f(MetricFormatType::BdtThousand), "25,000.00 K");
    assert_eq!(f(MetricFormatType::BdtMillion), "25.00 M");
    assert_eq!(f(MetricFormatType::BdtLakh), "250.00 L");
    assert_eq!(f(MetricFormatType::BdtCrore), "2.50 Cr");
    // the same through the global scale (View -> Default monetary scale) on an Auto money column
    let g = |n: NumberDisplayFormat| format_metric_value(v, "Balance", None, n, 2);
    assert_eq!(g(NumberDisplayFormat::BdtThousand), "25,000.00 K");
    assert_eq!(g(NumberDisplayFormat::BdtMillion), "25.00 M");
    assert_eq!(g(NumberDisplayFormat::BdtLakh), "250.00 L");
    assert_eq!(g(NumberDisplayFormat::BdtCrore), "2.50 Cr");
    assert_eq!(g(NumberDisplayFormat::StandardCommas), "25,000,000.00");
}

#[test]
fn l3_negatives_keep_their_sign_and_decimals_apply() {
    let f = |v: f64, t: MetricFormatType, d: usize| {
        format_metric_value(v, "Balance", Some(&ColumnFormatSpec { fmt_type: t, decimals: d }), NumberDisplayFormat::Raw, 2)
    };
    assert_eq!(f(-25_000_000.0, MetricFormatType::BdtMillion, 2), "-25.00 M");
    assert_eq!(f(-25_000_000.0, MetricFormatType::BdtThousand, 0), "-25,000 K");
    assert_eq!(f(-25_000_000.0, MetricFormatType::BdtLakh, 1), "-250.0 L");
    assert_eq!(f(-25_000_000.0, MetricFormatType::BdtCrore, 2), "-2.50 Cr");
    assert_eq!(f(0.0, MetricFormatType::BdtMillion, 2), "0.00 M");
}

#[test]
fn l3_crore_and_lakh_are_unchanged_and_rates_are_never_scaled() {
    // the Phase 1 results
    assert_eq!(format_metric_value(123_456_789.0, "Balance", None, NumberDisplayFormat::BdtCrore, 2), "12.35 Cr");
    assert_eq!(format_metric_value(123_456_789.0, "Balance", None, NumberDisplayFormat::BdtLakh, 2), "1,234.57 L");
    // rate columns stay percent whatever the global scale is
    assert_eq!(format_metric_value(9.5, "Int Rate", None, NumberDisplayFormat::BdtMillion, 2), "9.50%");
    assert!(resolved_scale("Int Rate", None, NumberDisplayFormat::BdtMillion).is_none());
    assert!(resolved_scale("Tenor_Days", None, NumberDisplayFormat::BdtMillion).is_none());
    assert_eq!(resolved_scale("Balance", None, NumberDisplayFormat::BdtMillion).unwrap().unit, "BDT M");
    assert_eq!(
        resolved_scale("Balance", Some(&spec(MetricFormatType::BdtLakh)), NumberDisplayFormat::BdtMillion).unwrap().unit,
        "BDT L",
        "a column's own format wins over the global scale"
    );
    // the menus list the new scales next to Crore and plain numbers
    let labels: Vec<&str> = NumberDisplayFormat::all().iter().map(|f| f.label()).collect();
    assert!(labels.iter().any(|l| l.contains("Thousand")) && labels.iter().any(|l| l.contains("Million")));
    assert!(MetricFormatType::all().contains(&MetricFormatType::BdtMillion));
    assert!(MetricFormatType::all().contains(&MetricFormatType::BdtThousand));
}

#[test]
fn l3_old_macros_with_crore_and_lakh_formats_still_load() {
    let spec_json = r#"{"fmt_type":"BdtLakh","decimals":3}"#;
    let s: ColumnFormatSpec = serde_json::from_str(spec_json).unwrap();
    assert_eq!(s.fmt_type, MetricFormatType::BdtLakh);
    let n: ColumnFormatSpec = serde_json::from_str(r#"{"fmt_type":"BdtMillion","decimals":1}"#).unwrap();
    assert_eq!(n.fmt_type, MetricFormatType::BdtMillion);
}

fn unzip_entry(zip: &std::path::Path, entry: &str) -> String {
    let out = std::process::Command::new("unzip").arg("-p").arg(zip).arg(entry).output().expect("unzip must be installed");
    assert!(out.status.success(), "unzip {entry}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn frame() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Balance".into(), &[25_000_000.0_f64, -5_000_000.0]).into(),
        Series::new("Int Rate".into(), &[9.5_f64, 8.0]).into(),
        Series::new("Actnum".into(), &["A1", "A2"]).into(),
    ])
    .unwrap()
}

#[test]
fn l3_excel_headers_name_the_scale_and_values_are_in_that_scale() {
    let d = tmpdir("l3_excel_headers_name_the_scale_and_values_are_in_that_scale");
    let p = d.join("scaled.xlsx");
    let tab = XlsxTab { title: "Loans".into(), df: frame(), col_formats: HashMap::new(), summary: Vec::new() };
    write_xlsx_tabs(vec![tab], vec![], &p, 1000, Some(XlsxScale { num_format: NumberDisplayFormat::BdtMillion, decimals: 2 })).unwrap();
    let strings = unzip_entry(&p, "xl/sharedStrings.xml");
    let sheet = unzip_entry(&p, "xl/worksheets/sheet1.xml");
    assert!(strings.contains("Balance (BDT M)"), "{strings}");
    assert!(strings.contains(">Int Rate<") && !strings.contains("Int Rate (BDT"), "the rate column is not scaled");
    assert!(strings.contains(">Actnum<"));
    // A2 = 25.00 M, A3 = -5.00 M; the rate stays 9.5
    assert!(sheet.contains("<v>25</v>"), "{sheet}");
    assert!(sheet.contains("<v>-5</v>"), "{sheet}");
    assert!(sheet.contains("<v>9.5</v>"));
    // A column's own format beats the global scale
    let mut formats = HashMap::new();
    formats.insert("Balance".to_string(), spec(MetricFormatType::BdtCrore));
    let p2 = d.join("per_column.xlsx");
    write_xlsx_tabs(
        vec![XlsxTab { title: "Loans".into(), df: frame(), col_formats: formats, summary: Vec::new() }],
        vec![],
        &p2,
        1000,
        Some(XlsxScale { num_format: NumberDisplayFormat::BdtMillion, decimals: 2 }),
    )
    .unwrap();
    assert!(unzip_entry(&p2, "xl/sharedStrings.xml").contains("Balance (BDT Cr)"));
    assert!(unzip_entry(&p2, "xl/worksheets/sheet1.xml").contains("<v>2.5</v>"));
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn l3_plain_scale_and_the_old_export_keep_raw_values_and_names() {
    let d = tmpdir("l3_plain_scale_and_the_old_export_keep_raw_values_and_names");
    let p = d.join("raw.xlsx");
    export_tabs_to_xlsx(vec![("Loans".into(), frame())], vec![], &p).unwrap();
    let strings = unzip_entry(&p, "xl/sharedStrings.xml");
    let sheet = unzip_entry(&p, "xl/worksheets/sheet1.xml");
    assert!(strings.contains(">Balance<") && !strings.contains("(BDT"), "{strings}");
    assert!(sheet.contains("<v>25000000</v>"), "{sheet}");
    // plain commas in the scaled export: nothing is divided
    let p2 = d.join("plain.xlsx");
    write_xlsx_tabs(
        vec![XlsxTab { title: "Loans".into(), df: frame(), col_formats: HashMap::new(), summary: Vec::new() }],
        vec![],
        &p2,
        1000,
        Some(XlsxScale { num_format: NumberDisplayFormat::StandardCommas, decimals: 2 }),
    )
    .unwrap();
    assert!(unzip_entry(&p2, "xl/worksheets/sheet1.xml").contains("<v>25000000</v>"));
    assert!(!unzip_entry(&p2, "xl/sharedStrings.xml").contains("(BDT"));
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn l3_pdf_shows_the_scale_in_the_header() {
    let d = tmpdir("l3_pdf_shows_the_scale_in_the_header");
    let p = d.join("scaled.pdf");
    let rep = export_tabs_to_pdf(vec![("Loans".into(), frame(), HashMap::new())], &p, NumberDisplayFormat::BdtMillion, 2).unwrap();
    assert!(rep.summary().is_empty());
    assert!(std::fs::metadata(&p).unwrap().len() > 500);
    // headers (the PDF text itself is compressed, so the header rule is tested directly)
    let m = NumberDisplayFormat::BdtMillion;
    assert_eq!(pdf_column_header("Balance", true, None, m), "Balance (BDT M)");
    assert_eq!(pdf_column_header("Int Rate", true, None, m), "Int Rate");
    assert_eq!(pdf_column_header("Actnum", false, None, m), "Actnum");
    assert_eq!(pdf_column_header("Balance", true, Some(&spec(MetricFormatType::BdtCrore)), m), "Balance (BDT Cr)");
    assert_eq!(pdf_column_header("Balance", true, None, NumberDisplayFormat::Raw), "Balance");
    // a long name is cut, the unit is not
    let long = pdf_column_header("Outstanding_Balance_Amount_Total", true, None, m);
    assert!(long.ends_with("(BDT M)"), "{long}");
    assert!(long.chars().count() <= 18, "{long}");
    std::fs::remove_dir_all(&d).ok();
}
