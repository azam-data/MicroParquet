//! L2: PDF export options: the heading is what was typed (none when blank), the paper is A4 or Legal,
//! upright or on its side, and columns that do not fit the width continue on further pages with the
//! row labels repeated.

use crate::common::*;
use micro_parquet::exporters::*;
use micro_parquet::models::*;
use micro_parquet::pdf_layout::PdfPage;
use polars::prelude::*;

/// The media boxes (width, height in points) of the pages of a PDF made by the exporter.
pub fn pdf_media_boxes(bytes: &[u8]) -> Vec<(f32, f32)> {
    let s = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    for part in s.split("/MediaBox[").skip(1) {
        let nums: Vec<f32> = part.split(']').next().unwrap_or("").split_whitespace().filter_map(|n| n.parse().ok()).collect();
        if nums.len() == 4 {
            out.push((nums[2] - nums[0], nums[3] - nums[1]));
        }
    }
    out
}

/// The texts drawn on the pages, in order (the exporter writes them as hex strings before `Tj`).
pub fn pdf_texts(bytes: &[u8]) -> Vec<String> {
    let s = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    for line in s.lines() {
        let l = line.trim();
        if let Some(hex) = l.strip_prefix('<').and_then(|r| r.strip_suffix("> Tj")) {
            let bytes: Vec<u8> = (0..hex.len() / 2).filter_map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()).collect();
            out.push(bytes.iter().map(|b| *b as char).collect());
        }
    }
    out
}

fn small() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Branch".into(), &["Dhaka", "Chattogram"]).into(),
        Series::new("Balance".into(), &[1.5f64, 2.5]).into(),
    ])
    .expect("frame")
}

fn wide(cols: usize) -> DataFrame {
    let mut v: Vec<Series> = vec![Series::new("Branch".into(), &["Dhaka", "Chattogram"]).into()];
    for i in 0..cols {
        v.push(Series::new(format!("Measure {:02}", i).into(), &[1_000_000.5f64 + i as f64, 2_000_000.5]).into());
    }
    DataFrame::new(v).expect("wide")
}

fn write(name: &str, df: DataFrame, opts: &PdfOptions) -> Vec<u8> {
    let d = dir(name);
    let p = d.join("x.pdf");
    export_pdf_tabs_with(vec![PdfTab { title: "Loans".into(), df, col_formats: Default::default(), summary: vec![], chart: None }], &p, NumberDisplayFormat::Raw, 2, opts).expect("pdf");
    std::fs::read(&p).expect("read")
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1.0
}

#[test]
fn l2_the_page_size_follows_orientation_and_paper() {
    for (landscape, legal, w, h) in [(false, false, 595.3, 841.9), (true, false, 841.9, 595.3), (false, true, 612.0, 1008.0), (true, true, 1008.0, 612.0)] {
        let b = write("l2a", small(), &PdfOptions { title: String::new(), page: PdfPage { landscape, legal }, chart_tables: false });
        assert!(b.starts_with(b"%PDF"));
        let boxes = pdf_media_boxes(&b);
        assert!(!boxes.is_empty());
        for (bw, bh) in boxes {
            assert!(near(bw, w) && near(bh, h), "landscape {landscape} legal {legal}: {bw} x {bh} (wanted {w} x {h})");
        }
    }
}

#[test]
fn l2_a_blank_title_writes_no_heading_and_a_title_writes_exactly_that_text() {
    let blank = write("l2b", small(), &PdfOptions::default());
    let t = pdf_texts(&blank);
    assert!(!String::from_utf8_lossy(&blank).contains("Helvetica-Bold 12 Tf"), "no heading line (the 12 pt bold)");
    assert!(!t.iter().any(|x| x.contains("MicroParquet Report")), "the fixed heading is gone: {t:?}");
    assert_eq!(t.first().map(String::as_str), Some("Branch"), "the page starts with the column headers: {t:?}");
    let spaces = write("l2b2", small(), &PdfOptions { title: "   ".into(), page: PdfPage::default() , chart_tables: false });
    assert!(!String::from_utf8_lossy(&spaces).contains("Helvetica-Bold 12 Tf"), "a title of spaces is no title");

    let titled = write("l2c", small(), &PdfOptions { title: "ALCO pack, November 2026".into(), page: PdfPage { landscape: false, legal: false }, chart_tables: false });
    let t = pdf_texts(&titled);
    assert_eq!(t.first().map(String::as_str), Some("ALCO pack, November 2026"), "{t:?}");
    assert!(!t.iter().any(|x| x.contains("MicroParquet Report")));
    // a title wider than the page is wrapped, not cut: the words all appear
    let long_title = "Asset and liability committee monthly pack for the board of directors with a very long heading that cannot fit on one line of a portrait page";
    let t = pdf_texts(&write("l2d", small(), &PdfOptions { title: long_title.into(), page: PdfPage { landscape: false, legal: false }, chart_tables: false }));
    let joined = t.iter().take_while(|x| *x != "Branch").cloned().collect::<Vec<_>>().join(" ");
    assert!(t.len() > 3 && joined == long_title, "{joined:?}");
}

#[test]
fn l2_columns_that_do_not_fit_continue_on_following_pages_with_the_row_labels_repeated() {
    let df = wide(30);
    let portrait = pdf_texts(&write("l2e", df.clone(), &PdfOptions { title: String::new(), page: PdfPage { landscape: false, legal: false }, chart_tables: false }));
    let landscape = pdf_texts(&write("l2f", df, &PdfOptions { title: String::new(), page: PdfPage { landscape: true, legal: false }, chart_tables: false }));
    let parts = |t: &[String]| t.iter().filter(|x| x.as_str() == "Branch").count();
    let (p, l) = (parts(&portrait), parts(&landscape));
    assert!(p > l && l >= 2, "portrait needs more parts than landscape: {p} vs {l}");
    // every part repeats the row labels
    assert_eq!(portrait.iter().filter(|x| x.as_str() == "Dhaka").count(), p, "the label column is on every part");
    assert_eq!(portrait.iter().filter(|x| x.as_str() == "Chattogram").count(), p);
    // and every measure is printed exactly once
    for i in 0..30 {
        let name = format!("Measure {:02}", i);
        let n = portrait.iter().filter(|x| **x == name).count();
        assert_eq!(n, 1, "{name} is printed once");
    }
}
