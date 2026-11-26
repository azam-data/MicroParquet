//! X1: the PowerPoint package, read back with the zip crate's own reader.

use crate::common::{dir, parquet};
use crate::p9_m3::{nums, strs};
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec, WaterfallMode};
use micro_parquet::dashboard::*;
use micro_parquet::kpi::*;
use micro_parquet::models::*;
use micro_parquet::pptx::*;
use micro_parquet::report_html::unit_scale;
use polars::prelude::*;
use std::io::{Cursor, Read};

fn book(d: &std::path::Path) -> TabSource {
    let df = DataFrame::new(vec![strs("Step", &["1 Opening", "2 Inflow", "3 Outflow"]), nums("Balance", &[150e6, 50e6, -30e6]), nums("Accounts", &[2.0, 2.0, 2.0])]).expect("data");
    parquet(d, "x1_book", &df)
}

fn chart(kind: ChartKind) -> ChartSpec {
    ChartSpec { category: "Step".into(), value: Some("Balance".into()), agg: ChartAgg::Sum, kind, top_n: 500, waterfall: WaterfallMode::StartSteps, ..Default::default() }
}

fn data_of(source: ItemSource, d: &std::path::Path) -> ItemData {
    compute_item(&ItemRequest { item_id: "x".into(), source, picks: vec![], drill: vec![], temp_dir: d.to_path_buf() }).expect("data")
}

fn slide(title: &str, data: ItemData, scale: (f64, usize, String)) -> PptxSlide {
    PptxSlide { title: title.into(), data, scale, kpi: None, footnote: footnote("31 Oct 2026", &["Wing = Islami".into()], &[]) }
}

fn plan(d: &std::path::Path) -> PptxPlan {
    let src = book(d);
    let crore = (10_000_000.0, 2, "Cr".to_string());
    let src2 = src.clone();
    let mut mixed = chart(ChartKind::Bar);
    mixed.measures = vec![
        micro_parquet::charts::spec::MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, ..Default::default() },
        micro_parquet::charts::spec::MeasureSpec { column: Some("Accounts".into()), agg: ChartAgg::Count, ..Default::default() },
    ];
    let chart_item = |k: ChartKind| data_of(ItemSource::Chart { base: src.clone(), steps: vec![], spec: chart(k) }, d);
    let pivot = PivotSpec {
        rows: vec!["Step".into()],
        values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None }, PivotValue { field: "Accounts".into(), agg: PivotAgg::Count, weight_col: None, time: None }],
        ..Default::default()
    };
    let kspec = KpiSpec { title: "Net".into(), unit: KpiUnit::Crore, compare: KpiCompare::PriorPeriod, thresholds: vec![Threshold { op: ThresholdOp::Below, limit: 500.0, colour: ThresholdColour::Red }], sparkline: true, ..Default::default() };
    let kvalue = KpiValue { current: Some(4_000_000_000.0), earlier: Some(4_200_000_000.0), period: "Oct 2026".into(), spark: vec![Some(4.1e9), Some(4.2e9), None, Some(4.0e9)], spark_labels: vec!["Jul 2026".into(), "Aug 2026".into(), "Sep 2026".into(), "Oct 2026".into()], ..Default::default() };
    let mut k = slide("Net position", ItemData::Kpi(Box::new(kvalue)), crore.clone());
    k.kpi = Some((kspec, KpiUnit::Crore));
    PptxPlan {
        title: "ALCO pack".into(),
        slides: vec![
            slide("By step", chart_item(ChartKind::Bar), crore.clone()),
            slide("Trend", chart_item(ChartKind::Line), crore.clone()),
            slide("Mix", chart_item(ChartKind::Donut), crore.clone()),
            slide("Bridge", chart_item(ChartKind::Waterfall), crore.clone()),
            slide("Steps", data_of(ItemSource::Pivot { base: src, steps: vec![], spec: pivot }, d), unit_scale(ItemUnit::Lakh)),
            k,
            slide("Balance and accounts", data_of(ItemSource::Chart { base: src2, steps: vec![], spec: mixed }, d), crore.clone()),
        ],
        background: None,
    }
}

fn read_all(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = ::zip::read::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("a zip");
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).expect("entry");
            let name = f.name().to_string();
            let mut v = Vec::new();
            // reading to the end checks the entry's CRC: a damaged entry is an error here
            f.read_to_end(&mut v).unwrap_or_else(|e| panic!("{name}: {e}"));
            (name, v)
        })
        .collect()
}

fn part<'a>(parts: &'a [(String, Vec<u8>)], name: &str) -> String {
    String::from_utf8_lossy(&parts.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("no part {name}")).1).to_string()
}

#[test]
fn x1_the_package_reads_back_with_every_crc_content_types_first_and_deflate_smaller_than_stored() {
    let dd = dir("p10x1_pkg");
    let p = plan(&dd);
    let bytes = build_pptx(&p).expect("package");
    let parts = read_all(&bytes);
    assert_eq!(parts[0].0, "[Content_Types].xml", "the content types come first");
    let stored = build_pptx_with(&p, true).expect("stored");
    assert!(bytes.len() < stored.len(), "{} deflated vs {} stored", bytes.len(), stored.len());
    assert!(read_all(&stored).len() == parts.len());
    assert_eq!(MAX_PACKAGE_BYTES, 4 * 1024 * 1024 * 1024);
    let e = build_pptx(&PptxPlan { title: "x".into(), slides: vec![], background: None }).expect_err("nothing to put");
    assert!(e.to_string().contains("no items"), "{e}");
}

#[test]
fn x1_one_slide_per_item_each_chart_part_has_its_chart_type_palette_and_workbook_ranges() {
    let dd = dir("p10x1_charts");
    let parts = read_all(&build_pptx(&plan(&dd)).expect("package"));
    let slides = parts.iter().filter(|(n, _)| n.starts_with("ppt/slides/slide")).count();
    assert_eq!(slides, 7, "one slide per item");
    let charts: Vec<String> = (1..=4).map(|i| part(&parts, &format!("ppt/charts/chart{i}.xml"))).collect();
    assert!(parts.iter().all(|(n, _)| n != "ppt/charts/chart7.xml"), "the pivot is not a chart; the card has a sparkline chart (5) and the two-unit chart is 6");
    // the card's sparkline is a small line chart without axes, with a gap for the missing period
    let spark = part(&parts, "ppt/charts/chart5.xml");
    assert!(spark.contains("<c:lineChart>") && spark.contains("<c:delete val=\"1\"/>") && spark.contains("<c:ptCount val=\"4\"/>") && !spark.contains("<c:pt idx=\"2\"><c:v>4"), "{spark}");
    assert!(part(&parts, "ppt/slides/slide6.xml").contains("r:id=\"rId2\""), "the card's slide holds the sparkline");
    // money and a count in one chart: bars on the left axis, the count on a second axis on the right
    let mixed = part(&parts, "ppt/charts/chart6.xml");
    assert_eq!(mixed.matches("<c:barChart>").count(), 2, "one group per axis");
    assert!(mixed.contains("<c:axId val=\"114\"/>") && mixed.contains("<c:axPos val=\"r\"/>") && mixed.contains("<c:crosses val=\"max\"/>"), "{mixed}");
    assert!(charts[0].contains("<c:barChart>") && charts[0].contains("<c:barDir val=\"col\"/>"));
    assert!(charts[1].contains("<c:lineChart>"));
    assert!(charts[2].contains("<c:doughnutChart>"));
    assert!(charts[3].contains("<c:barChart>") && charts[3].contains("<c:grouping val=\"stacked\"/>"), "the bridge is a stacked bar with a hidden base");
    // the series point at the workbook's ranges: 3 categories in A2:A4 and the values in B2:B4
    assert!(charts[0].contains("<c:f>Sheet1!$A$2:$A$4</c:f>") && charts[0].contains("<c:f>Sheet1!$B$2:$B$4</c:f>") && charts[0].contains("<c:f>Sheet1!$B$1</c:f>"));
    // the palette is the chart's: the first series is house slot 0
    let slot0 = format!("{:06X}", micro_parquet::charts::tokens::series_colour(0, false).expect("slot"));
    assert!(charts[0].contains(&format!("<a:srgbClr val=\"{slot0}\"/>")), "{slot0}");
    // the values are in the unit of the chart (÷ 10,000,000): 150 million = 15 Cr
    assert!(charts[0].contains("<c:v>15</c:v>") && charts[0].contains("<c:v>5</c:v>") && charts[0].contains("<c:v>-3</c:v>"), "{}", &charts[0]);
    // the embedded workbook holds the same numbers and the category names
    let wb = parts.iter().find(|(n, _)| n == "ppt/embeddings/Microsoft_Excel_Sheet1.xlsx").expect("workbook").1.clone();
    let inner = read_all(&wb);
    let sheet = part(&inner, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("<v>15</v>") && sheet.contains("<v>5</v>") && sheet.contains("<v>-3</v>"), "{sheet}");
    assert!(part(&inner, "xl/sharedStrings.xml").contains("2 Inflow"));
    assert!(part(&parts, "ppt/charts/_rels/chart1.xml.rels").contains("../embeddings/Microsoft_Excel_Sheet1.xlsx"));
}

#[test]
fn x1_the_footnote_the_pivot_table_and_the_kpi_card_say_what_the_app_says() {
    let dd = dir("p10x1_text");
    let parts = read_all(&build_pptx(&plan(&dd)).expect("package"));
    let s1 = part(&parts, "ppt/slides/slide1.xml");
    assert!(s1.contains("As of 31 Oct 2026 \u{b7} Filters: Wing = Islami"), "the as-of date and the filters in force");
    // the pivot: money in Lakh with the unit in the heading, the count whole
    let s5 = part(&parts, "ppt/slides/slide5.xml");
    assert!(s5.contains("<a:tbl>") && s5.contains("Sum of Balance (Lakh)") && s5.contains("Count of Accounts"), "{s5}");
    assert!(s5.contains("1,500.00") && s5.contains("<a:t>1</a:t>"), "150 million = 1,500 Lakh; one row per step counted whole, not 0.00 Lakh");
    // the card: the app's own text, the threshold colour on the number, the tone colour and words on the change
    let s6 = part(&parts, "ppt/slides/slide6.xml");
    assert!(s6.contains("400.00 Cr"), "{s6}");
    assert_eq!(s6.matches("<a:srgbClr val=\"D03B3B\"/>").count(), 2, "the number is red (below 500) and the change is red (a fall is bad)");
    assert!(s6.contains("\u{25bc}") && s6.contains("down against the earlier period"), "the arrow and the words");
}

#[test]
fn x1_libreoffice_opens_the_file_when_it_is_installed() {
    let Some(office) = ["soffice", "libreoffice"].iter().find(|c| std::process::Command::new(c).arg("--version").output().map(|o| o.status.success()).unwrap_or(false)) else {
        eprintln!("X1 LibreOffice check skipped: LibreOffice is not installed here");
        return;
    };
    let dd = dir("p10x1_office");
    let file = dd.join("pack.pptx");
    write_pptx(&file, &plan(&dd)).expect("written");
    let o = std::process::Command::new(office).args(["--headless", "--convert-to", "pdf", "--outdir"]).arg(&dd).arg(&file).output().expect("runs");
    assert!(o.status.success() && dd.join("pack.pdf").exists(), "{}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn x1_a_design_picture_goes_behind_every_slide_only_when_asked() {
    let dd = dir("p10x1_bg");
    let mut p = plan(&dd);
    let without = read_all(&build_pptx(&p).expect("package"));
    assert!(!without.iter().any(|(n, _)| n == "ppt/media/image1.png") && !part(&without, "ppt/slides/slide1.xml").contains("<p:bg>"));
    let mut png = Vec::new();
    image::DynamicImage::new_rgb8(4, 3).write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).expect("png");
    p.background = Some(png.clone());
    let with = read_all(&build_pptx(&p).expect("package"));
    assert_eq!(with.iter().find(|(n, _)| n == "ppt/media/image1.png").map(|(_, d)| d.clone()), Some(png));
    for i in 1..=7 {
        assert!(part(&with, &format!("ppt/slides/slide{i}.xml")).contains("<p:bg>"), "slide {i}");
        assert!(part(&with, &format!("ppt/slides/_rels/slide{i}.xml.rels")).contains("../media/image1.png"));
    }
}
