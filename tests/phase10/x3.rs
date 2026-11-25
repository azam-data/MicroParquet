//! The export module the dialog uses: pages as they are on the screen go to the HTML, PowerPoint and PDF writers.

use crate::common::{dir, parquet};
use crate::p9_m3::{nums, strs};
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec};
use micro_parquet::dash_export::*;
use micro_parquet::dashboard::*;
use micro_parquet::kpi::KpiUnit;
use micro_parquet::models::*;
use polars::prelude::*;
use std::process::Command;

fn src_of(d: &std::path::Path, picture: bool) -> ExportSrc {
    let df = DataFrame::new(vec![strs("Wing", &["Islami", "Islami", "Conv", "Conv"]), strs("Step", &["a", "b", "a", "b"]), nums("Balance", &[100e6, 200e6, 300e6, 400e6])]).expect("data");
    let base = parquet(d, "x3_book", &df);
    let chart = ItemSource::Chart { base, steps: vec![], spec: ChartSpec { category: "Step".into(), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 500, ..Default::default() } };
    let wing = PageFilter { id: "f1".into(), label: "Wing".into(), tab: "t1".into(), field: "Wing".into(), ..Default::default() };
    let mut png = Vec::new();
    ::image::DynamicImage::new_rgb8(32, 18).write_to(&mut std::io::Cursor::new(&mut png), ::image::ImageFormat::Png).expect("png");
    ExportSrc {
        title: "ALCO pack".into(),
        kpi_unit: KpiUnit::Crore,
        temp_dir: d.to_path_buf(),
        pages: vec![ExportPageSrc {
            id: "p1".into(),
            title: "Overview".into(),
            page_h: 562.5,
            items: vec![ExportItemSrc {
                id: "i1".into(),
                title: "By step".into(),
                source: chart,
                data_tab: Some("t1".into()),
                scale: (10_000_000.0, 2, "Crore".into()),
                kpi: None,
                rect: DashRect { x: 0.0, y: 0.0, w: 1000.0, h: 562.5 },
                picks: vec![(wing.clone(), vec!["Islami".into()])],
                drill: vec![],
            }],
            filters: vec![wing],
            values: vec![vec!["Conv".into(), "Islami".into()]],
            filter_text: vec!["Wing = Islami".into()],
            background: picture.then_some(png),
        }],
    }
}

#[test]
fn x3_the_html_file_counts_its_picture_and_lists_nothing_unlinked_for_a_plain_chart() {
    let dd = dir("p10x3_html");
    let plain = prepare_html(&src_of(&dd, false)).expect("html");
    let with = prepare_html(&src_of(&dd, true)).expect("html");
    assert_eq!(plain.parts.template, 0);
    assert!(with.parts.template > 0 && with.html.contains("data:image/png;base64,"), "the design is inlined and counted");
    assert!(plain.not_linked.is_empty() && !plain.has_drill && plain.pages == 1);
    // the file is written through a temporary file and says its size
    let path = dd.join("pack.html");
    assert!(write_html(&path, &plain).expect("written").contains("MB"));
    assert!(path.exists() && !dd.join("pack.html.part").exists());
}

#[test]
fn x3_powerpoint_and_pdf_show_the_screens_filters_and_the_as_of_date_is_said_when_unknown() {
    let dd = dir("p10x3_files");
    let src = src_of(&dd, true);
    let pptx = dd.join("a.pptx");
    export_pptx(&src, true, &pptx).expect("pptx");
    let z = std::fs::read(&pptx).expect("file");
    let mut a = ::zip::read::ZipArchive::new(std::io::Cursor::new(z)).expect("zip");
    let mut slide = String::new();
    std::io::Read::read_to_string(&mut a.by_name("ppt/slides/slide1.xml").expect("slide"), &mut slide).expect("xml");
    assert!(slide.contains("Filters: Wing = Islami") && slide.contains("As of date not set"), "{slide}");
    assert!(slide.contains("<p:bg>"), "the design is the background when asked");
    // the numbers are the filtered ones: Islami has 100 and 200 million = 10 and 20 Crore, not the 40 and 60 of Conv
    let mut chart = String::new();
    std::io::Read::read_to_string(&mut a.by_name("ppt/charts/chart1.xml").expect("chart"), &mut chart).expect("xml");
    assert!(chart.contains("<c:v>10</c:v>") && chart.contains("<c:v>20</c:v>") && !chart.contains("<c:v>30</c:v>"), "{chart}");
    let pdf = dd.join("a.pdf");
    let (msg, _) = export_pdf(&src, &pdf).expect("pdf");
    assert!(msg.contains("1 page"), "{msg}");
    if let Ok(o) = Command::new("pdftotext").arg(&pdf).arg("-").output() {
        let t = String::from_utf8_lossy(&o.stdout).to_string();
        assert!(t.contains("Filters: Wing = Islami"), "{t}");
    }
    let e = export_pptx(&ExportSrc { pages: vec![], ..src }, false, &dd.join("b.pptx")).expect_err("nothing to export");
    assert!(e.to_string().contains("no items"), "{e}");
}
