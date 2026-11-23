//! W11: stacked segment labels and the data table. The window (the studio's options) is tested in `src/ui/render_tests_p8.rs`
//! (`w11_…`); here are the renderers: a label is straight where it fits, turned where it does not and the column has room, left out
//! where nothing fits, never leaves its segment and never touches another label; the Excel chart carries data labels on every series
//! when the option is on and none when it is off; the PDF page holds the data table (below or beside the chart) with every value and
//! total; and with both options off every output equals Phase 7's.

use micro_parquet::charts::calc::compute;
use micro_parquet::charts::data::ChartData;
use micro_parquet::charts::data_table::data_table;
use micro_parquet::charts::echarts::chart_pair;
use micro_parquet::charts::layout::{layout_chart, ApproxMeasure, ChartLayout, LayoutOpts, MarkKind, Measure, Role};
use micro_parquet::charts::pdf::PdfMeasure;
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec, DataTable, Stack};
use micro_parquet::charts::xlsx::write_chart_xlsx;
use polars::prelude::*;

fn opts(dark: bool, decimals: usize) -> LayoutOpts {
    LayoutOpts { dark, divisor: 1.0, decimals, unit: String::new(), bare: false, colours: None }
}

/// 40 columns of three products, so the bars are narrow: a short value (9.5), a longer one (120.0 and more) and a tiny one (0.04).
fn narrow_columns(stack: Stack, inside: bool) -> ChartData {
    narrow_n(40, stack, inside)
}

fn narrow_n(n: usize, stack: Stack, inside: bool) -> ChartData {
    let (mut m, mut p, mut v) = (Vec::new(), Vec::new(), Vec::new());
    for c in 0..n {
        for (name, value) in [("Term deposit", 120.0 + c as f64), ("Savings", 9.5), ("Current", 0.04)] {
            m.push(format!("M{:02}", c + 1));
            p.push(name.to_string());
            v.push(value);
        }
    }
    let df = DataFrame::new(vec![Series::new("Month".into(), m).into(), Series::new("Product".into(), p).into(), Series::new("Balance".into(), v).into()]).expect("df");
    let s = ChartSpec { category: "Month".into(), series: Some("Product".into()), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, stack, top_n: 500, inside_labels: inside, ..Default::default() };
    compute(df.lazy(), &s).expect("the chart")
}

fn rect_of(w: f32, h: f32) -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(w, h))
}

/// What the rule says a label does, worked out from the segment's rectangle and the text's size alone: straight, turned or none.
fn rule(seg: egui::Rect, text: egui::Vec2, column: bool) -> &'static str {
    if seg.width() >= text.x + 6.0 && seg.height() >= text.y + 4.0 {
        "straight"
    } else if column && seg.height() >= text.x + 6.0 && seg.width() >= text.y + 4.0 {
        "turned"
    } else {
        "none"
    }
}

/// The checks every layout with segment labels must pass: a label is where the rule puts it, inside its own segment, touching no
/// other label, white or ink by contrast (never the series colour), and the segments below the share asked for have none.
fn check_labels(l: &ChartLayout, d: &ChartData, m: &dyn Measure, min_pct: f64, what: &str) -> (usize, usize, usize) {
    let column = !d.spec.horizontal;
    let pt = micro_parquet::charts::tokens::data_label_pt();
    let segments: Vec<&micro_parquet::charts::layout::Mark> = l.marks.iter().filter(|x| x.kind == MarkKind::Segment).collect();
    let labels: Vec<&micro_parquet::charts::layout::LText> = l.texts.iter().filter(|t| t.role == Role::Cell).collect();
    let mut by_state = (0usize, 0usize, 0usize);
    for seg in &segments {
        let cat_total: f64 = segments.iter().filter(|o| o.category == seg.category && o.panel == seg.panel).filter_map(|o| o.value).map(f64::abs).sum();
        let share = seg.value.map_or(0.0, |v| v.abs()) / cat_total * 100.0;
        // the labels that stand inside this segment
        let inside: Vec<&&micro_parquet::charts::layout::LText> = labels.iter().filter(|t| seg.rect.contains_rect(t.rect)).collect();
        let sl = l.segment_labels.iter().find(|x| x.panel == seg.panel && x.series == seg.series && x.category == seg.category);
        if share < min_pct {
            assert!(inside.is_empty() && sl.is_none(), "{what}: a segment of {share:.2}% of its bar has no label");
            by_state.2 += 1;
            continue;
        }
        // the text this segment would carry
        let text = match d.spec.stack {
            Stack::Percent => format!("{}%", micro_parquet::models::format_with_commas(share, 1)),
            _ => micro_parquet::charts::axis::value_text(seg.value.unwrap_or(0.0), micro_parquet::charts::axis::ValueKind::Amount, 1, false),
        };
        let sz = m.size(&text, pt, false);
        match (rule(seg.rect, sz, column), sl) {
            ("none", None) => {
                assert!(inside.is_empty(), "{what}: nothing fits, so nothing is written");
                by_state.2 += 1;
            }
            ("straight", Some(x)) => {
                assert!(!x.turned && inside.len() == 1 && inside[0].angle == 0.0, "{what}: straight");
                by_state.0 += 1;
            }
            ("turned", Some(x)) => {
                assert!(x.turned && inside.len() == 1 && (inside[0].angle + std::f32::consts::FRAC_PI_2).abs() < 1e-6, "{what}: turned");
                by_state.1 += 1;
            }
            (want, got) => panic!("{what}: the rule says {want}, the layout has {got:?} for a segment {:?} and a text of {sz:?}", seg.rect),
        }
    }
    // inside its segment, and no two labels touch
    for (i, a) in labels.iter().enumerate() {
        assert!(segments.iter().any(|s| s.rect.contains_rect(a.rect)), "{what}: the label '{}' {:?} is inside a segment", a.text, a.rect);
        for b in labels.iter().skip(i + 1) {
            assert!(!a.rect.intersects(b.rect), "{what}: the labels '{}' and '{}' overlap", a.text, b.text);
        }
        // never the series colour: white, or the ink of the light theme
        let ink = micro_parquet::charts::tokens::INK_LIGHT.ink;
        let c = a.colour;
        assert!(c == egui::Color32::WHITE || (c.r() as u32) << 16 | (c.g() as u32) << 8 | c.b() as u32 == ink, "{what}: '{}' is white or ink, not {c:?}", a.text);
    }
    by_state
}

#[test]
fn w11_a_segment_label_is_straight_where_it_fits_turned_where_a_column_has_room_and_left_out_where_nothing_fits() {
    let d = narrow_columns(Stack::Stacked, true);
    let pdf_m = PdfMeasure::new();
    let mut seen = (0, 0, 0);
    for (w, h) in [(1200.0, 700.0), (1600.0, 900.0), (420.0, 300.0)] {
        for dark in [false, true] {
            let what = format!("{w}x{h} {}", if dark { "dark" } else { "light" });
            // both measures: the rough one the tests use, and the fonts of the PDF
            for (name, m) in [("approx", &ApproxMeasure as &dyn Measure), ("fonts", &pdf_m as &dyn Measure)] {
                let l = layout_chart(&d, rect_of(w, h), &opts(dark, 1), m);
                let n = check_labels(&l, &d, m, d.spec.inside_min_pct, &format!("{what} {name}"));
                if (w, h, dark, name) == (1200.0, 700.0, false, "approx") {
                    seen = n;
                }
            }
        }
    }
    // the chart of the owner's three kinds of segment: the short value straight, the longer one turned, the tiny one left out
    assert!(seen.0 >= 30 && seen.1 >= 30 && seen.2 >= 30, "straight, turned and left out: {seen:?}");
    // the total on top stays: the column's total is still written above it (a chart of 12 columns or fewer writes them)
    let twelve = twelve_by_three(true, DataTable::Off);
    let l = layout_chart(&twelve, rect_of(1200.0, 700.0), &opts(false, 2), &ApproxMeasure);
    let jan: f64 = (0..3).map(|si| twelve.values[si][0].expect("a value")).sum();
    let total = micro_parquet::models::format_with_commas(jan, 2);
    assert!(l.texts.iter().any(|t| t.role == Role::Label && t.text == total), "the total {total} is on top: {:?}", l.texts.iter().filter(|t| t.role == Role::Label).map(|t| t.text.clone()).take(6).collect::<Vec<_>>());
    check_labels_loose(&l, &twelve);
}

/// Labels of a chart whose texts are not the ones of `narrow_columns`: inside their segments and clear of each other.
fn check_labels_loose(l: &ChartLayout, d: &ChartData) {
    let segments: Vec<_> = l.marks.iter().filter(|x| x.kind == MarkKind::Segment).collect();
    let labels: Vec<_> = l.texts.iter().filter(|t| t.role == Role::Cell).collect();
    assert_eq!(labels.len(), l.segment_labels.len());
    for (i, a) in labels.iter().enumerate() {
        assert!(segments.iter().any(|s| s.rect.contains_rect(a.rect)), "the label '{}' is inside a segment", a.text);
        for b in labels.iter().skip(i + 1) {
            assert!(!a.rect.intersects(b.rect));
        }
    }
    let _ = d;
}

#[test]
fn w11_hide_below_x_percent_of_the_bar_and_the_100_percent_stack_says_percent() {
    // the share asked for decides: 5% by default, and a higher one leaves out more
    let d = narrow_columns(Stack::Stacked, true);
    assert_eq!(d.spec.inside_min_pct, 5.0);
    let count = |min: f64| {
        let mut s = d.spec.clone();
        s.inside_min_pct = min;
        let d2 = ChartData { spec: s, ..d.clone() };
        layout_chart(&d2, rect_of(1200.0, 700.0), &opts(false, 1), &ApproxMeasure).segment_labels.len()
    };
    let (a, b, c) = (count(0.0), count(5.0), count(50.0));
    assert!(a >= b && b > c, "0%: {a}, 5%: {b}, 50%: {c}");
    // a 100% stacked chart says each share, with a percent sign
    let p = narrow_columns(Stack::Percent, true);
    let l = layout_chart(&p, rect_of(1200.0, 700.0), &opts(false, 1), &ApproxMeasure);
    assert!(!l.segment_labels.is_empty() && l.segment_labels.iter().all(|x| x.text.ends_with('%')), "{:?}", l.segment_labels.iter().map(|x| x.text.clone()).take(4).collect::<Vec<_>>());
    check_labels(&l, &p, &ApproxMeasure, 5.0, "100% stacked");
    // horizontal bars are never turned
    let mut s = d.spec.clone();
    s.horizontal = true;
    let hz = ChartData { spec: s, ..d.clone() };
    let l = layout_chart(&hz, rect_of(1200.0, 700.0), &opts(false, 1), &ApproxMeasure);
    assert!(l.segment_labels.iter().all(|x| !x.turned));
    // the option off: no label inside any segment
    let off = narrow_columns(Stack::Stacked, false);
    assert!(layout_chart(&off, rect_of(1200.0, 700.0), &opts(false, 1), &ApproxMeasure).segment_labels.is_empty());
}

/// The text of the chart XML parts of a workbook (python3 reads the zip).
fn chart_xml(path: &std::path::Path) -> String {
    let script = "import sys, zipfile\nz = zipfile.ZipFile(sys.argv[1])\nprint(''.join(z.read(n).decode('utf-8') for n in sorted(z.namelist()) if n.startswith('xl/charts/chart')))\n";
    let o = std::process::Command::new("python3").arg("-I").arg("-c").arg(script).arg(path).output().expect("python3 runs");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).to_string()
}

#[test]
fn w11_the_excel_chart_has_data_labels_on_every_series_when_on_and_none_when_off() {
    let dir = crate::common::dir("w11_xlsx");
    for stack in [Stack::Stacked, Stack::Percent] {
        let on = narrow_n(20, stack, true);
        let p = dir.join(format!("on_{stack:?}.xlsx"));
        write_chart_xlsx(&p, &on, &opts(false, 1)).expect("xlsx");
        let xml = chart_xml(&p);
        assert_eq!(xml.matches("<c:ser>").count(), 3, "three series");
        assert_eq!(xml.matches("<c:dLbls>").count(), 3, "{stack:?}: data labels on every series when On");
        // the ones with no room have their label deleted, the turned ones are rotated, the text is white or ink
        assert!(xml.contains("<c:delete val=\"1\"/>"), "{stack:?}: segments with no room have no label");
        assert!(xml.contains("rot=\"-5400000\""), "{stack:?}: a turned label is rotated");
        assert!(xml.contains("FFFFFF") || xml.contains("ffffff"), "{stack:?}: white text on a dark fill");
        let off = narrow_n(20, stack, false);
        let q = dir.join(format!("off_{stack:?}.xlsx"));
        write_chart_xlsx(&q, &off, &opts(false, 1)).expect("xlsx");
        let xml = chart_xml(&q);
        assert_eq!(xml.matches("<c:ser>").count(), 3);
        assert_eq!(xml.matches("<c:dLbls>").count(), 0, "{stack:?}: no data labels when Off");
    }
}

#[test]
fn w11_the_page_labels_the_segments_inside_them_straight_or_turned_and_none_where_there_is_no_room() {
    // a narrow chart (turned and none) and a 12-column one (straight): between them every kind, in both themes
    let narrow = chart_pair(&narrow_n(20, Stack::Stacked, true), &opts(false, 1)).expect("html");
    let wide = chart_pair(&twelve_by_three(true, DataTable::Off), &opts(false, 2)).expect("html");
    let (mut straight, mut turned, mut hidden) = (0, 0, 0);
    for (name, pair) in [("narrow", &narrow), ("twelve", &wide)] {
        for (theme, v) in [("light", &pair.light), ("dark", &pair.dark)] {
            let series = v["series"].as_array().expect("series");
            let bars: Vec<&serde_json::Value> = series.iter().filter(|s| s["type"] == "bar").collect();
            assert_eq!(bars.len(), 3);
            for s in &bars {
                assert!(s["barMaxWidth"].as_f64().unwrap_or(0.0) > 24.0, "wider bars for labelled segments");
                // a page shown smaller than the nominal size drops the labels that collide instead of printing them over each other
                assert_eq!(s["labelLayout"]["hideOverlap"], true, "{name} {theme}: hideOverlap on the labelled series");
                for item in s["data"].as_array().expect("data") {
                    let label = &item["label"];
                    if label["show"] == false {
                        hidden += 1;
                    } else {
                        assert_eq!(label["position"], "inside", "{name} {theme}");
                        let col = label["color"].as_str().unwrap_or("");
                        assert!(col.eq_ignore_ascii_case("#ffffff") || col.eq_ignore_ascii_case(&micro_parquet::charts::tokens::hex_text(micro_parquet::charts::tokens::INK_LIGHT.ink)), "{name} {theme}: {col}");
                        if label["rotate"] == 90 {
                            turned += 1;
                        } else {
                            straight += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(straight > 0 && turned > 0 && hidden > 0, "straight {straight}, turned {turned}, none {hidden}");
    // off: no label on a segment at all
    let off = chart_pair(&narrow_n(20, Stack::Stacked, false), &opts(false, 1)).expect("html");
    assert!(off.light["series"].as_array().expect("series").iter().filter(|s| s["type"] == "bar").all(|s| s["data"].as_array().expect("data").iter().all(|i| i.get("label").is_none())));
    assert!(off.light["series"].as_array().expect("series").iter().filter(|s| s["type"] == "bar").all(|s| s.get("labelLayout").is_none()), "Off: no label layout, as in Phase 7");
}

// ---- the data table

/// 12 months of three products.
fn twelve_by_three(inside: bool, table: DataTable) -> ChartData {
    let (mut m, mut p, mut v) = (Vec::new(), Vec::new(), Vec::new());
    for (c, month) in ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"].iter().enumerate() {
        for (s, product) in ["Term deposit", "Savings", "Current"].iter().enumerate() {
            m.push(*month);
            p.push(*product);
            v.push([900.0, 420.0, 135.0][s] + 17.25 * c as f64 * (s as f64 + 1.0) + 0.37 * s as f64);
        }
    }
    let df = DataFrame::new(vec![Series::new("Month".into(), m).into(), Series::new("Product".into(), p).into(), Series::new("Balance".into(), v).into()]).expect("df");
    let order = micro_parquet::sort_lists::SortOrder::Calendar { week_start: micro_parquet::dates::Weekday::Sat };
    let s = ChartSpec { category: "Month".into(), series: Some("Product".into()), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, stack: Stack::Stacked, top_n: 500, order, inside_labels: inside, data_table: table, ..Default::default() };
    compute(df.lazy(), &s).expect("the chart")
}

fn pdf_of(d: &ChartData, name: &str, page: micro_parquet::pdf_layout::PdfPage) -> String {
    pdf_file(d, name, page).0
}

/// The PDF of a chart tab: its text and where it was written.
fn pdf_file(d: &ChartData, name: &str, page: micro_parquet::pdf_layout::PdfPage) -> (String, std::path::PathBuf) {
    use micro_parquet::exporters::{export_pdf_tabs_with, PdfOptions, PdfTab};
    let dir = crate::common::dir(&format!("w11_pdf_{name}"));
    let path = dir.join(format!("{name}.pdf"));
    let tab = PdfTab { title: "Chart".into(), df: micro_parquet::chart::chart_table(d).expect("table"), col_formats: Default::default(), summary: vec![], chart: Some((d.clone(), opts(false, 2))) };
    let o = PdfOptions { title: "Deposits".into(), page, chart_tables: false };
    export_pdf_tabs_with(vec![tab], &path, micro_parquet::models::NumberDisplayFormat::BdtCrore, 2, &o).expect("pdf");
    (crate::p8_w11_baseline::pdf_text(&path), path)
}

/// Every value and every total of the 12 x 3 chart, as the table writes them, worked out here from the numbers.
fn expected_cells(d: &ChartData) -> Vec<String> {
    let f = |v: f64| micro_parquet::models::format_with_commas(v, 2);
    let mut out = Vec::new();
    for si in 0..3 {
        for c in 0..12 {
            out.push(f(d.values[si][c].expect("a value")));
        }
        out.push(f(d.values[si].iter().flatten().sum()));
    }
    for c in 0..12 {
        out.push(f((0..3).map(|si| d.values[si][c].expect("a value")).sum()));
    }
    out.push(f(d.values.iter().flatten().flatten().sum()));
    out
}

#[test]
fn w11_a_12_by_3_chart_pdf_holds_its_table_below_and_beside_with_every_value_and_total_on_one_page() {
    if !crate::p8_w11_baseline::have_pdftotext() {
        return;
    }
    for (table, landscape) in [(DataTable::Below, true), (DataTable::Beside, false)] {
        let d = twelve_by_three(false, table);
        let page = micro_parquet::pdf_layout::PdfPage { landscape, legal: false };
        let text = pdf_of(&d, &format!("t_{table:?}"), page);
        assert_eq!(text.matches('\x0c').count(), 1, "{table:?}: one page: {} form feeds", text.matches('\x0c').count());
        for cell in expected_cells(&d) {
            assert!(text.contains(&cell), "{table:?}: the page text has {cell}");
        }
        for word in ["Jan", "Dec", "Term deposit", "Savings", "Current", "Total"] {
            assert!(text.contains(word), "{table:?}: {word}");
        }
        assert!(text.contains("Sum of Balance"), "the caption names what the numbers are");
        // the order of the chart: the months run Jan to Dec down the table
        let at = |w: &str| text.find(&format!("\n{w}")).or_else(|| text.find(w)).unwrap_or(usize::MAX);
        assert!(at("Jan") < at("Feb") && at("Nov") < at("Dec"), "{table:?}: the chart's order");
    }
    // a beside table on an upright page is written on a landscape page
    let d = twelve_by_three(false, DataTable::Beside);
    use micro_parquet::exporters::{export_pdf_tabs_with, PdfOptions, PdfTab};
    let dir = crate::common::dir("w11_pdf_beside");
    let path = dir.join("b.pdf");
    let tab = PdfTab { title: "Chart".into(), df: micro_parquet::chart::chart_table(&d).expect("table"), col_formats: Default::default(), summary: vec![], chart: Some((d.clone(), opts(false, 2))) };
    export_pdf_tabs_with(vec![tab], &path, micro_parquet::models::NumberDisplayFormat::BdtCrore, 2, &PdfOptions { title: String::new(), page: micro_parquet::pdf_layout::PdfPage { landscape: false, legal: false }, chart_tables: false }).expect("pdf");
    if std::process::Command::new("python3").arg("-I").arg("-c").arg("import pypdf").output().map_or(true, |o| !o.status.success()) {
        eprintln!("w11: pypdf is not installed here: the page size is not read");
        return;
    }
    let sizes = std::process::Command::new("python3").arg("-I").arg("-c").arg("import sys, pypdf\nfor p in pypdf.PdfReader(sys.argv[1]).pages:\n    print(round(float(p.mediabox.width)), round(float(p.mediabox.height)))").arg(&path).output().expect("python3");
    assert_eq!(String::from_utf8_lossy(&sizes.stdout).trim(), "842 595", "landscape A4: {}", String::from_utf8_lossy(&sizes.stderr));
    // off: the chart page is written as before (no table words: the chart alone has no 'Total' column)
    let plain = pdf_of(&twelve_by_three(false, DataTable::Off), "t_off", micro_parquet::pdf_layout::PdfPage::default());
    assert_eq!(plain.matches('\x0c').count(), 1);
    assert!(!plain.contains("Total ") || !plain.contains("Sum of Balance \u{b7}"), "no data table when Off");
}

#[test]
fn w11_a_table_that_does_not_fit_continues_on_the_next_page_with_its_header_repeated_and_never_below_8_pt() {
    if !crate::p8_w11_baseline::have_pdftotext() {
        return;
    }
    // 58 categories: more rows than a page holds
    let (mut m, mut p, mut v) = (Vec::new(), Vec::new(), Vec::new());
    for c in 0..58 {
        for (s, product) in ["Term deposit", "Savings", "Current"].iter().enumerate() {
            m.push(format!("Branch {:02}", c + 1));
            p.push(*product);
            v.push(1000.0 + 13.5 * c as f64 + 7.0 * s as f64);
        }
    }
    let df = DataFrame::new(vec![Series::new("Branch".into(), m).into(), Series::new("Product".into(), p).into(), Series::new("Balance".into(), v).into()]).expect("df");
    let s = ChartSpec { category: "Branch".into(), series: Some("Product".into()), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, stack: Stack::Stacked, top_n: 500, data_table: DataTable::Below, ..Default::default() };
    let d = compute(df.lazy(), &s).expect("chart");
    let (text, long_pdf) = pdf_file(&d, "long", micro_parquet::pdf_layout::PdfPage::default());
    let pages: Vec<&str> = text.split('\x0c').filter(|p| !p.trim().is_empty()).collect();
    assert!(pages.len() >= 2, "the table continues: {} pages", pages.len());
    for (i, pg) in pages.iter().enumerate() {
        assert!(pg.contains("Term deposit") && pg.contains("Total"), "page {}: the header is repeated", i + 1);
    }
    assert!(pages[1..].iter().any(|p| p.contains("(continued)")), "the next page says it continues");
    // every row once, and the total row once, on the last page
    for c in 1..=58 {
        assert_eq!(text.matches(&format!("Branch {:02}", c)).count(), 1, "Branch {c} is written once");
    }
    let t = data_table(&d, &opts(false, 2)).expect("table");
    let grand = t.total.as_ref().expect("a total row").last().expect("grand total").clone();
    assert_eq!(text.matches(&grand).count(), 1, "the grand total {grand} is written once");
    assert!(pages.last().expect("last page").contains(&grand), "and on the last page");
    // text in the table is 8 pt or more: python reads the sizes of the runs of the PDF
    let script = "import sys, pypdf\nr = pypdf.PdfReader(sys.argv[1])\nsizes = []\ndef visit(text, cm, tm, fd, fs):\n    if text.strip() and ('Branch' in text or '.' in text and text.replace(',', '').replace('.', '').strip().isdigit()):\n        sizes.append(round(fs * (tm[3] if tm[3] else tm[0]), 2))\nfor p in r.pages:\n    p.extract_text(visitor_text=visit)\nprint(min(sizes) if sizes else 0)\n";
    if std::process::Command::new("python3").arg("-I").arg("-c").arg("import pypdf").output().map_or(true, |o| !o.status.success()) {
        eprintln!("w11: pypdf is not installed here: the text sizes are not read");
        return;
    }
    let o = std::process::Command::new("python3").arg("-I").arg("-c").arg(script).arg(&long_pdf).output().expect("python3");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let min: f32 = String::from_utf8_lossy(&o.stdout).trim().parse().expect("a size");
    assert!(min >= 7.99, "the smallest table text is {min} pt");
}

#[test]
fn w11_the_html_page_shows_the_table_open_under_the_chart_and_a_pack_pdf_prints_the_charts_that_ask() {
    use micro_parquet::charts::echarts::{chart_book_html, chart_html, BookItem};
    let d = twelve_by_three(false, DataTable::Below);
    let html = chart_html(&d, &opts(false, 2)).expect("page");
    assert!(html.contains("class=\"mp-tablewrap open\"") && !html.contains("id=\"mp-c0-table\" hidden"), "the table is open under the chart");
    for cell in expected_cells(&d) {
        assert!(html.contains(&cell), "the page's table has {cell}");
    }
    assert!(html.contains("<tr class=\"mp-total\">"), "a total row");
    let book = chart_book_html("Book", "2026-11-23", &[BookItem { data: &d, opts: opts(false, 2) }, BookItem { data: &twelve_by_three(false, DataTable::Off), opts: opts(false, 2) }]).expect("book");
    assert_eq!(book.matches("mp-tablewrap open").count(), 1, "only the chart that asked has its table open");
    assert!(book.contains("id=\"mp-c1-table\" hidden"), "the other keeps the toggle's hidden table");
    // a page with no open table does not carry the extras
    let plain = chart_html(&twelve_by_three(false, DataTable::Off), &opts(false, 2)).expect("page");
    assert!(!plain.contains("mp-total") && !plain.contains("OPEN") && !plain.contains("c.open"));
    // a chart that has no table of categories and series says why, and a scatter is not printed with one
    let mut s = d.spec.clone();
    s.kind = ChartKind::Scatter;
    assert!(s.data_table_reason().expect("a reason").contains("scatter"));
    let mut s = d.spec.clone();
    s.facet = Some("Product".into());
    assert!(s.data_table_reason().expect("a reason").contains("small multiples"));
}

#[test]
fn w11_with_both_options_off_the_output_equals_phase_7s() {
    let want: std::collections::BTreeMap<String, String> = serde_json::from_str(&std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase8_w11_off.hashes.json")).expect("the fixture")).expect("its json");
    let got = crate::p8_w11_baseline::outputs();
    // (without pdftotext the PDF pages are not read, here and in the baseline)
    let have_pdf = crate::p8_w11_baseline::have_pdftotext();
    let want: std::collections::BTreeMap<String, String> = want.into_iter().filter(|(k, _)| have_pdf || !k.contains("/pdf/")).collect();
    let mut wrong: Vec<String> = Vec::new();
    for (k, v) in &want {
        match got.get(k) {
            Some(g) if g == v => {}
            Some(g) => wrong.push(format!("{k}: Phase 7 {v}, now {g}")),
            None => wrong.push(format!("{k}: no longer written")),
        }
    }
    assert_eq!(want.len(), got.len(), "the same outputs");
    assert!(wrong.is_empty(), "the output with both options off differs from Phase 7's:\n{}", wrong.join("\n"));
}


#[test]
fn w11_the_report_pack_pdf_prints_the_chart_with_its_table_when_the_chart_asks_and_is_as_before_when_it_does_not() {
    if !crate::p8_w11_baseline::have_pdftotext() {
        return;
    }
    use micro_parquet::charts::pack::ItemChart;
    use micro_parquet::packs::{write_pack, PackOutputCopy, PackResult, PackSheet};
    let dir = crate::common::dir("w11_pack");
    let build = |table: DataTable, name: &str| -> (String, usize) {
        let d = twelve_by_three(false, table);
        let sheet = PackSheet { sheet: "Deposits".into(), title: "Deposits by month".into(), df: micro_parquet::chart::chart_table(&d).expect("table"), col_formats: Default::default(), charts: vec![ItemChart { data: d.clone(), opts: opts(false, 2) }] };
        let cover = DataFrame::new(vec![Series::new("Item".into(), ["Deposits"]).into(), Series::new("Detail".into(), ["12 months"]).into()]).expect("cover");
        let result = PackResult { pack: "Pack".into(), cutoff: "2026-11-23".into(), cover, sheets: vec![sheet], inputs: vec![], used: vec![], output: PackOutputCopy { html_charts: false, pdf: true, scale: micro_parquet::models::NumberDisplayFormat::BdtCrore, decimals: 2 } };
        let (x, p) = (dir.join(format!("{name}.xlsx")), dir.join(format!("{name}.pdf")));
        write_pack(&result, &x, Some(&p), &micro_parquet::jobs::JobCtx::inert()).expect("the pack is written");
        let text = crate::p8_w11_baseline::pdf_text(&p);
        let n = text.matches('\x0c').count();
        (text, n)
    };
    let (on, pages_on) = build(DataTable::Below, "on");
    let (off, pages_off) = build(DataTable::Off, "off");
    // the chart's own page holds its data table and its totals
    let d = twelve_by_three(false, DataTable::Below);
    assert!(pages_on > pages_off, "a page for the chart: {pages_on} pages against {pages_off}");
    for cell in expected_cells(&d) {
        assert!(on.contains(&cell), "the pack PDF has {cell}");
    }
    assert!(on.contains("Sum of Balance") && on.contains("Total"), "the table's caption and its total row");
    // the chart with the option off is not in the PDF at all, as in Phase 7
    assert!(!off.contains("Sum of Balance by Month and Product"), "no chart page when the data table is off");
}

#[test]
fn w11_the_test_workflow_installs_poppler_before_cargo_test_so_the_pdf_checks_run_in_ci() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/test.yml")).expect("test.yml");
    let at = text.find("sudo apt-get install -y poppler-utils").expect("the install step");
    let run = text.find("run: cargo test").expect("cargo test");
    assert!(at < run, "poppler is installed before cargo test");
}
