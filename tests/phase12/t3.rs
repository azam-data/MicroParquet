//! Phase 12 T3: a card with a target in the HTML page data, the PowerPoint package and the PDF. The same description (`Graphic`) is
//! drawn everywhere, so the numbers are checked once in t1 and here only that each output carries it.

use crate::common::{dir, parquet};
use crate::p9_m3::{nums, strs};
use micro_parquet::dashboard::*;
use micro_parquet::dashboard_pdf::*;
use micro_parquet::kpi::*;
use micro_parquet::models::*;
use micro_parquet::pptx::*;
use micro_parquet::report_html::*;
use micro_parquet::target::Bands;
use polars::prelude::*;
use serde_json::Value;
use std::io::{Cursor, Read};

fn card(visual: KpiVisual) -> (KpiSpec, KpiValue) {
    let spec = KpiSpec {
        title: "Deposits".into(),
        unit: KpiUnit::Crore,
        visual,
        target: Some(KpiTarget { source: TargetSource::Fixed(5000.0), bands: Bands::default() }),
        sparkline: true,
        ..Default::default()
    };
    let value = KpiValue { current: Some(4_800.0 * 1e7), period: "Oct 2026".into(), spark: vec![Some(4.2e10), Some(4.5e10), Some(4.8e10)], spark_labels: vec!["Aug".into(), "Sep".into(), "Oct".into()], ..Default::default() };
    (spec, value)
}

fn book(d: &std::path::Path) -> TabSource {
    let df = DataFrame::new(vec![strs("Step", &["1 Opening"]), nums("Balance", &[100.0])]).expect("data");
    parquet(d, "p12t3_book", &df)
}

fn pptx_xml(visual: KpiVisual) -> String {
    let (spec, value) = card(visual);
    let slide = PptxSlide { title: "Deposits".into(), data: ItemData::Kpi(Box::new(value)), scale: (1e7, 2, "Cr".into()), kpi: Some((spec, KpiUnit::Crore)), footnote: footnote("31 Oct 2026", &[], &[]) };
    let bytes = build_pptx(&PptxPlan { title: "Pack".into(), slides: vec![slide], background: None }).expect("package");
    let mut z = ::zip::read::ZipArchive::new(Cursor::new(bytes)).expect("zip");
    let mut f = z.by_name("ppt/slides/slide1.xml").expect("slide 1");
    let mut s = String::new();
    f.read_to_string(&mut s).expect("xml");
    s
}

#[test]
fn t3_a_html_page_data_carries_the_target_words_and_the_bands() {
    let dd = dir("p12t3_html");
    let (spec, value) = card(KpiVisual::Gauge);
    let item = ReportItem { id: "k".into(), title: "Deposits".into(), source: ItemSource::Kpi { spec, tab_key: "t1".into(), base: book(&dd), steps: vec![] }, data_tab: Some("t1".into()), scale: (1.0, 2, "BDT".into()) };
    let plan = ReportPlan { title: "P".into(), pages: vec![ReportPage { id: "p1".into(), title: "P".into(), filters: vec![], values: vec![], items: vec![item] }], template_bytes: 0, temp_dir: dd.clone() };
    let answers = vec![PageAnswers { page_id: "p1".into(), items: vec![ItemAnswers { item_id: "k".into(), filters: vec![], answers: vec![Answer { picks: vec![], path: vec![], data: ItemData::Kpi(Box::new(value)) }], numbers: 4, linked: Err("a card is one number".into()), cross: vec![] }] }];
    let all: Value = serde_json::from_str(&answers_json(&plan, &answers)).expect("json");
    let k = &all["pages"][0]["items"][0]["answers"][0]["k"];
    assert_eq!(k["tgt"], "Target 5,000.00 Cr \u{b7} 96.0% of target \u{b7} amber");
    assert_eq!(k["thr"], "amber", "the number is coloured by the band");
    assert_eq!(k["gr"]["kind"], "gauge");
    assert_eq!(k["gr"]["v"].as_f64(), Some(4800.0));
    assert_eq!(k["gr"]["bands"].as_array().map(|b| b.len()), Some(3));
    assert_eq!(k["gr"]["bands"][0][2], "red");
    assert!(k["gr"]["d"].as_str().unwrap_or("").contains("against a target of 5,000.00 Cr"), "{}", k["gr"]["d"]);
    // the page itself is written (a browser look is taken by hand: MP_SHOT_DIR names where the file is kept)
    let looks = vec![PageLook { page_h: 625.0, rects: vec![[0.0, 0.0, 400.0, 300.0]], background: None }];
    let html = render_page(&plan, &answers_json(&plan, &answers), &looks).expect("page");
    assert!(html.contains("data-kpi") && html.contains("blockArc") == false);
    if let Ok(keep) = std::env::var("MP_SHOT_DIR") {
        std::fs::write(std::path::Path::new(&keep).join("gauge.html"), &html).expect("write");
    }
}

#[test]
fn t3_b_powerpoint_draws_a_bullet_from_rectangles_and_a_gauge_from_arcs_and_says_the_target_in_words() {
    let bullet = pptx_xml(KpiVisual::Bullet);
    assert!(bullet.contains("Target 5,000.00 Cr"), "the words are on the slide");
    for name in ["Bullet red", "Bullet amber", "Bullet green", "Bullet value", "Bullet target"] {
        assert!(bullet.contains(&format!("name=\"{name}\"")), "{name}");
    }
    assert!(bullet.contains("against a target of 5,000.00 Cr"), "alt text on the shapes");
    let gauge = pptx_xml(KpiVisual::Gauge);
    assert_eq!(gauge.matches("prst=\"blockArc\"").count(), 3, "one arc a zone");
    assert!(gauge.contains("name=\"Gauge needle\"") && gauge.contains("name=\"Gauge target\""));
    // the red zone runs from the left end (180 degrees) and the three zones follow one another
    assert!(gauge.contains("fmla=\"val 10800000\""));
    // a plain card has none of it
    let plain = {
        let (mut spec, value) = card(KpiVisual::Number);
        spec.target = None;
        let slide = PptxSlide { title: "D".into(), data: ItemData::Kpi(Box::new(value)), scale: (1e7, 2, "Cr".into()), kpi: Some((spec, KpiUnit::Crore)), footnote: footnote("31 Oct 2026", &[], &[]) };
        let bytes = build_pptx(&PptxPlan { title: "Pack".into(), slides: vec![slide], background: None }).expect("package");
        let mut z = ::zip::read::ZipArchive::new(Cursor::new(bytes)).expect("zip");
        let mut s = String::new();
        z.by_name("ppt/slides/slide1.xml").expect("slide").read_to_string(&mut s).expect("xml");
        s
    };
    assert!(!plain.contains("blockArc") && !plain.contains("Bullet"));
}

#[test]
fn t3_c_the_pdf_page_writes_without_error_for_a_bullet_and_a_gauge_and_carries_the_target_words() {
    let dd = dir("p12t3_pdf");
    for (n, visual) in [("bullet", KpiVisual::Bullet), ("gauge", KpiVisual::Gauge)] {
        let (spec, value) = card(visual);
        let item = PdfDashItem { id: "k".into(), title: "Deposits".into(), rect: DashRect { x: 20.0, y: 20.0, w: 360.0, h: 220.0 }, data: ItemData::Kpi(Box::new(value)), scale: (1e7, 2, "Crore".into()), kpi: Some((spec, KpiUnit::Crore)) };
        let plan = DashPdfPlan { title: "Pack".into(), pages: vec![PdfDashPage { title: "Overview".into(), page_h: 562.5, items: vec![item], background: None, footer: "As of 31 Oct 2026".into() }] };
        let out = dd.join(format!("{n}.pdf"));
        micro_parquet::dashboard_pdf::export_dashboard_pdf(&plan, &out).expect("pdf");
        assert!(std::fs::metadata(&out).expect("file").len() > 1000);
        if let Ok(o) = std::process::Command::new("pdftotext").arg("-layout").arg(&out).arg("-").output() {
            let t = String::from_utf8_lossy(&o.stdout).to_string();
            assert!(t.contains("Target 5,000.00 Cr"), "{n}: {t}");
        }
    }
}
