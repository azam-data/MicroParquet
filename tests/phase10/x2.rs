//! X2: the PDF pages of a dashboard, read back with poppler (pdfinfo, pdftotext, pdfimages) when it is installed.

use crate::common::{dir, parquet};
use crate::p9_m3::{nums, strs};
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec};
use micro_parquet::dashboard::*;
use micro_parquet::dashboard_pdf::*;
use micro_parquet::kpi::*;
use micro_parquet::models::*;
use micro_parquet::report_html::unit_scale;
use polars::prelude::*;
use std::process::Command;

fn poppler() -> bool {
    Command::new("pdftotext").arg("-v").output().is_ok() && Command::new("pdfinfo").arg("-v").output().is_ok() && Command::new("pdfimages").arg("-v").output().is_ok()
}

fn data_of(source: ItemSource, d: &std::path::Path) -> ItemData {
    compute_item(&ItemRequest { item_id: "x".into(), source, picks: vec![], drill: vec![], temp_dir: d.to_path_buf() }).expect("data")
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> DashRect {
    DashRect { x, y, w, h }
}

fn plan(d: &std::path::Path, with_picture: bool) -> DashPdfPlan {
    let df = DataFrame::new(vec![strs("Step", &["1 Opening", "2 Inflow", "3 Outflow"]), nums("Balance", &[150e6, 50e6, -30e6]), nums("Accounts", &[2.0, 2.0, 2.0])]).expect("data");
    let src = parquet(d, "x2_book", &df);
    let chart = ItemSource::Chart { base: src.clone(), steps: vec![], spec: ChartSpec { category: "Step".into(), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 500, ..Default::default() } };
    let pivot = ItemSource::Pivot {
        base: src,
        steps: vec![],
        spec: PivotSpec {
            rows: vec!["Step".into()],
            values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None }, PivotValue { field: "Accounts".into(), agg: PivotAgg::Count, weight_col: None, time: None }],
            ..Default::default()
        },
    };
    let kspec = KpiSpec { title: "Net".into(), unit: KpiUnit::Crore, compare: KpiCompare::PriorPeriod, thresholds: vec![Threshold { op: ThresholdOp::Below, limit: 500.0, colour: ThresholdColour::Red }], sparkline: true, ..Default::default() };
    let kvalue = KpiValue { current: Some(4_000_000_000.0), earlier: Some(4_200_000_000.0), period: "Oct 2026".into(), spark: vec![Some(4.1e9), Some(4.2e9), None, Some(4.0e9)], spark_labels: vec!["Jul".into(), "Aug".into(), "Sep".into(), "Oct".into()], ..Default::default() };
    let crore = (10_000_000.0, 2, "Crore".to_string());
    let item = |id: &str, title: &str, r: DashRect, data: ItemData, scale: (f64, usize, String), kpi: Option<(KpiSpec, KpiUnit)>| PdfDashItem { id: id.into(), title: title.into(), rect: r, data, scale, kpi };
    let mut png = Vec::new();
    ::image::DynamicImage::new_rgb8(160, 90).write_to(&mut std::io::Cursor::new(&mut png), ::image::ImageFormat::Png).expect("png");
    DashPdfPlan {
        title: "ALCO pack".into(),
        pages: vec![
            PdfDashPage {
                title: "Overview".into(),
                page_h: 562.5,
                items: vec![
                    item("k", "Net position", rect(20.0, 20.0, 300.0, 200.0), ItemData::Kpi(Box::new(kvalue)), crore.clone(), Some((kspec, KpiUnit::Crore))),
                    item("c", "By step", rect(340.0, 20.0, 640.0, 260.0), data_of(chart.clone(), d), crore.clone(), None),
                    item("t", "Steps table", rect(20.0, 300.0, 600.0, 80.0), data_of(pivot, d), unit_scale(ItemUnit::Lakh), None),
                ],
                background: with_picture.then_some(png),
                footer: "As of 31 Oct 2026 \u{b7} Filters: Wing = Islami".into(),
            },
            PdfDashPage { title: "Second".into(), page_h: 625.0, items: vec![item("c2", "Again", rect(0.0, 0.0, 1000.0, 625.0), data_of(chart, d), crore, None)], background: None, footer: "As of 31 Oct 2026 \u{b7} Filters: none".into() },
        ],
    }
}

fn text_of(pdf: &std::path::Path, page: usize) -> String {
    let o = Command::new("pdftotext").args(["-f", &page.to_string(), "-l", &page.to_string(), "-layout"]).arg(pdf).arg("-").output().expect("pdftotext");
    String::from_utf8_lossy(&o.stdout).to_string()
}

/// Every word of a page with its box in millimetres from the top left.
fn words(pdf: &std::path::Path, page: usize) -> Vec<(String, f32, f32, f32, f32)> {
    let o = Command::new("pdftotext").args(["-f", &page.to_string(), "-l", &page.to_string(), "-bbox"]).arg(pdf).arg("-").output().expect("pdftotext");
    let s = String::from_utf8_lossy(&o.stdout).to_string();
    let k = 25.4 / 72.0;
    s.lines()
        .filter(|l| l.trim_start().starts_with("<word "))
        .filter_map(|l| {
            let attr = |n: &str| l.split(&format!("{n}=\"")).nth(1).and_then(|r| r.split('"').next()).and_then(|v| v.parse::<f32>().ok());
            let t = l.split('>').nth(1)?.split('<').next()?.to_string();
            Some((t, attr("xMin")? * k, attr("yMin")? * k, attr("xMax")? * k, attr("yMax")? * k))
        })
        .collect()
}

#[test]
fn x2_pages_text_picture_footer_and_positions_of_a_free_layout_page() {
    if !poppler() {
        eprintln!("X2 PDF read-back skipped: poppler (pdftotext, pdfinfo, pdfimages) is not installed here");
        return;
    }
    let dd = dir("p10x2_pdf");
    let out = dd.join("pack.pdf");
    let report = export_dashboard_pdf(&plan(&dd, true), &out).expect("pdf");
    // page count = dashboard pages, A4 landscape
    let info = String::from_utf8_lossy(&Command::new("pdfinfo").arg(&out).output().expect("pdfinfo").stdout).to_string();
    assert!(info.contains("Pages:           2"), "{info}");
    assert!(info.contains("595.276 x 841.89") || info.contains("595.28 x 841.89") || info.contains("841.89 x 595.2"), "A4 landscape: {info}");
    // the picture is embedded once on page 1 and not on page 2
    let imgs = String::from_utf8_lossy(&Command::new("pdfimages").arg("-list").arg(&out).output().expect("pdfimages").stdout).to_string();
    let per_page = |n: usize| imgs.lines().skip(2).filter(|l| l.split_whitespace().next() == Some(&n.to_string())).count();
    assert_eq!((per_page(1), per_page(2)), (1, 0), "{imgs}");
    // the text: the card's number as the card shows it, the table in its unit, the footer
    let p1 = text_of(&out, 1);
    assert!(p1.contains("400.00 Cr"), "{p1}");
    assert!(p1.contains("Sum of Balance (Lakh)") && p1.contains("1,500.00"), "{p1}");
    assert!(p1.contains("As of 31 Oct 2026 \u{b7} Filters: Wing = Islami") && p1.contains("Page 1 of 2"), "{p1}");
    assert!(text_of(&out, 2).contains("Filters: none") && text_of(&out, 2).contains("Page 2 of 2"));
    // positions: the free layout keeps each item where its rect says, within 1 mm
    let (left, top, s) = page_fit(562.5);
    for p in report.placed.iter().filter(|p| p.page == 0) {
        let r = match p.item_id.as_str() {
            "k" => rect(20.0, 20.0, 300.0, 200.0),
            "c" => rect(340.0, 20.0, 640.0, 260.0),
            _ => rect(20.0, 300.0, 600.0, 80.0),
        };
        assert!((p.left_mm - (left + r.x as f32 * s)).abs() <= 1.0 && (p.top_mm - (top + r.y as f32 * s)).abs() <= 1.0, "{p:?}");
        assert!((p.width_mm - r.w as f32 * s).abs() <= 1.0 && (p.height_mm - r.h as f32 * s).abs() <= 1.0, "{p:?}");
    }
    // and the paper agrees: the title of the card and of the table start at their item's left edge (+1.5 mm), inside its top
    let w = words(&out, 1);
    for (title, id) in [("Net", "k"), ("Steps", "t")] {
        let p = report.placed.iter().find(|p| p.item_id == id).expect("placed");
        let word = w.iter().find(|x| x.0 == title).unwrap_or_else(|| panic!("the word {title} on the page: {w:?}"));
        assert!((word.1 - (p.left_mm + 1.5)).abs() <= 1.0, "{title}: x {} vs {}", word.1, p.left_mm + 1.5);
        assert!(word.2 >= p.top_mm - 0.5 && word.2 <= p.top_mm + 4.0, "{title}: y {} vs {}", word.2, p.top_mm);
    }
    // no overflow: every word is on the paper inside the margins
    for (t, x0, y0, x1, y1) in &w {
        assert!(*x0 >= 7.0 && *x1 <= 290.0 && *y0 >= 7.0 && *y1 <= 203.0, "{t} at {x0},{y0},{x1},{y1}");
    }
    // the table's item is short: its rows are cut and the report says so
    assert!(report.cut.iter().any(|c| c.contains("Steps table") && c.contains("more rows")), "{:?}", report.cut);
    assert!(p1.contains("more rows are in the HTML and PowerPoint files"));
}

#[test]
fn x2_an_empty_dashboard_is_refused_and_a_picture_that_is_not_one_says_so() {
    let dd = dir("p10x2_refuse");
    let e = export_dashboard_pdf(&DashPdfPlan { title: "t".into(), pages: vec![] }, &dd.join("a.pdf")).expect_err("no pages");
    assert!(e.to_string().contains("no pages"), "{e}");
    let mut p = plan(&dd, false);
    p.pages[0].background = Some(b"not a picture".to_vec());
    let e = export_dashboard_pdf(&p, &dd.join("b.pdf")).expect_err("bad picture");
    assert!(e.to_string().contains("design picture could not be read"), "{e}");
    assert!(!dd.join("b.pdf").exists() && !dd.join("b.pdf.part").exists(), "no half-written file");
}
