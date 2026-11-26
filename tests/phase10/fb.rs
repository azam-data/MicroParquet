//! The feedback round: the wide 4 × 3 grid, text boxes and pictures in the three exports, and old projects still loading.

use crate::common::dir;
use micro_parquet::dash_export::*;
use micro_parquet::dashboard::*;
use micro_parquet::kpi::KpiUnit;

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    ::image::DynamicImage::new_rgb8(w, h).write_to(&mut std::io::Cursor::new(&mut out), ::image::ImageFormat::Png).expect("png");
    out
}

fn static_src(d: &std::path::Path) -> ExportSrc {
    let text = TextSpec { text: "Liquidity at a glance\nALCO, November".into(), size: TextSize::Large, bold: true, align: TextAlign::Centre };
    let pic = ImageSpec::from_bytes("logo.png", &png(80, 40)).expect("picture");
    let item = |id: &str, source: ItemSource, rect: DashRect| ExportItemSrc { id: id.into(), title: String::new(), source, data_tab: None, scale: (1.0, 2, String::new()), kpi: None, rect, picks: vec![], drill: vec![] };
    ExportSrc {
        title: "Pack".into(),
        kpi_unit: KpiUnit::Crore,
        temp_dir: d.to_path_buf(),
        pages: vec![ExportPageSrc {
            id: "p1".into(),
            title: "Cover".into(),
            page_h: PAGE_H_WIDE,
            items: vec![item("i1", ItemSource::Text(text), DashRect { x: 0.0, y: 0.0, w: 1000.0, h: 200.0 }), item("i2", ItemSource::Image(pic), DashRect { x: 300.0, y: 250.0, w: 400.0, h: 200.0 })],
            filters: vec![],
            values: vec![],
            filter_text: vec![],
            background: None,
        }],
    }
}

fn entry(a: &mut ::zip::read::ZipArchive<std::io::Cursor<Vec<u8>>>, name: &str) -> String {
    let mut s = String::new();
    std::io::Read::read_to_string(&mut a.by_name(name).unwrap_or_else(|_| panic!("{name} in the package")), &mut s).expect("text");
    s
}

#[test]
fn fb_a_text_box_and_a_picture_reach_the_html_the_powerpoint_and_the_pdf() {
    let dd = dir("p10fb_static");
    let src = static_src(&dd);
    // HTML: the text and the picture are in the page, and they are counted as items that read no numbers
    let html = prepare_html(&src).expect("html");
    assert!(html.html.contains("Liquidity at a glance") && html.html.contains("data:image/png;base64,"), "text and picture are inlined");
    // PowerPoint: the text is a text box, the picture is a picture part of its own
    let pptx = dd.join("a.pptx");
    export_pptx(&src, false, &pptx).expect("pptx");
    let mut z = ::zip::read::ZipArchive::new(std::io::Cursor::new(std::fs::read(&pptx).expect("file"))).expect("zip");
    let slide = entry(&mut z, "ppt/slides/slide1.xml");
    assert!(slide.contains("Liquidity at a glance") && slide.contains("ALCO, November") && slide.contains("algn=\"ctr\""), "{slide}");
    // (each item is a slide: the picture is the second)
    let second = entry(&mut z, "ppt/slides/slide2.xml");
    assert!(second.contains("<p:pic>") && second.contains("r:embed=\"rId4\""), "{second}");
    assert!(z.by_name("ppt/media/slide2.png").is_ok(), "the picture is in the package");
    // PDF: written, one page
    let pdf = dd.join("a.pdf");
    let (msg, report) = export_pdf(&src, &pdf).expect("pdf");
    assert!(msg.contains("1 page") && report.cut.is_empty(), "{msg} {:?}", report.cut);
    // a text box and a picture come out of the item pipeline as they are: no data is read and no drill is asked
    let req = |source| ItemRequest { item_id: "i1".into(), source, picks: vec![], drill: vec![], temp_dir: dd.clone() };
    assert!(matches!(compute_item(&req(ItemSource::Text(TextSpec { text: "x".into(), ..Default::default() }))), Ok(ItemData::Text(_))));
    assert!(matches!(compute_item(&req(ItemSource::Image(ImageSpec::from_bytes("l.png", &png(8, 8)).expect("pic")))), Ok(ItemData::Image(_))));
}

#[test]
fn fb_a_picture_is_made_smaller_and_a_file_that_is_not_a_picture_says_so() {
    let big = ImageSpec::from_bytes("big.png", &png(3200, 1600)).expect("big");
    assert_eq!((big.w, big.h), (MAX_IMAGE_SIDE, MAX_IMAGE_SIDE / 2), "the longest side is cut to the limit and the shape is kept");
    assert_eq!(::image::load_from_memory(&big.png().expect("png")).expect("decodes").width(), MAX_IMAGE_SIDE);
    let e = ImageSpec::from_bytes("notes.txt", b"not a picture").expect_err("a text file");
    assert!(e.to_string().contains("not a PNG or JPEG"), "{e}");
    let broken = ImageSpec { name: "x".into(), png_b64: "!!".into(), w: 1, h: 1 };
    assert!(broken.png().expect_err("damaged").to_string().contains("damaged"));
}

#[test]
fn fb_the_wide_grid_has_four_across_three_down_on_a_16_9_page_and_old_pages_are_unchanged() {
    let mut page = DashPage { id: "p1".into(), title: "P".into(), ..Default::default() };
    // an old page: 16:10 and two per row, exactly as before (and its JSON has none of the new fields)
    assert_eq!((page.height(), page.grid_rows), (PAGE_H, None));
    let json = serde_json::to_string(&page).expect("json");
    assert!(!json.contains("grid_rows"), "{json}");
    let back: DashPage = serde_json::from_str(&json).expect("an old page loads");
    assert_eq!(back.grid_rows, None);
    // the preset: 16:9, four across, at least three rows
    page.set_wide_grid();
    assert_eq!((page.height(), page.layout, page.grid_rows), (PAGE_H_WIDE, PageLayout::Grid { per_row: 4 }, Some(3)));
    assert!((PAGE_W / page.height() - 16.0 / 9.0).abs() < 1e-9);
    // with one item the cell is still a twelfth of the page, not the whole page
    page.items.push(DashItem { id: "i1".into(), kind: DashItemKind::Text(TextSpec::default()), ..Default::default() });
    let one = item_rects(&page).expect("rects");
    assert_eq!(one.len(), 1);
    assert!((one[0].w - (PAGE_W - 3.0 * GRID_GAP) / 4.0).abs() < 1e-9 && (one[0].h - (PAGE_H_WIDE - 2.0 * GRID_GAP) / 3.0).abs() < 1e-9, "{:?}", one[0]);
    // twelve items fill the 4 × 3 cells: four columns, three rows, none outside the page
    for i in 2..=12 {
        page.items.push(DashItem { id: format!("i{i}"), kind: DashItemKind::Text(TextSpec::default()), ..Default::default() });
    }
    let r = item_rects(&page).expect("rects");
    let xs: std::collections::BTreeSet<i64> = r.iter().map(|c| (c.x * 100.0).round() as i64).collect();
    let ys: std::collections::BTreeSet<i64> = r.iter().map(|c| (c.y * 100.0).round() as i64).collect();
    assert_eq!((xs.len(), ys.len()), (4, 3));
    assert!(r.iter().all(|c| c.x + c.w <= PAGE_W + 1e-6 && c.y + c.h <= PAGE_H_WIDE + 1e-6));
    // a thirteenth item adds a fourth row rather than being lost
    page.items.push(DashItem { id: "i13".into(), kind: DashItemKind::Text(TextSpec::default()), ..Default::default() });
    let r = item_rects(&page).expect("rects");
    assert_eq!(r.iter().map(|c| (c.y * 100.0).round() as i64).collect::<std::collections::BTreeSet<_>>().len(), 4);
    // the saved page carries the new fields and loads again
    let back: DashPage = serde_json::from_str(&serde_json::to_string(&page).expect("json")).expect("loads");
    assert_eq!((back.grid_rows, back.height()), (Some(3), PAGE_H_WIDE));
}

#[test]
fn fb_text_and_picture_items_round_trip_and_old_item_kinds_still_load() {
    let t = DashItem { id: "i1".into(), kind: DashItemKind::Text(TextSpec { text: "Hello".into(), size: TextSize::Title, bold: true, align: TextAlign::Right }), ..Default::default() };
    let back: DashItem = serde_json::from_str(&serde_json::to_string(&t).expect("json")).expect("loads");
    assert_eq!(back, t);
    let old = r#"{"id":"i9","kind":{"Pivot":"t3"}}"#;
    let item: DashItem = serde_json::from_str(old).expect("an item saved before the feedback round");
    assert_eq!(item.kind, DashItemKind::Pivot("t3".into()));
    assert!(t.source_key().is_none(), "a text box has no tab to open");
}
