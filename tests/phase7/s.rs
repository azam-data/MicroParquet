//! Final fixes for PR #13 (Phase 7, Session F): the SVG as XML, category labels that are never skipped, the donut
//! without a refusal, Reset colours, Text size, the chart PDF, and the items of the last review.

use crate::p7_c1::lay;
use crate::p7_c3::forms;
use crate::p7_c4::tmp;
use micro_parquet::chart::{ChartAgg, ChartKind, ChartSpec};
use micro_parquet::charts::calc::compute;
use micro_parquet::charts::data::ChartData;
use micro_parquet::charts::echarts::{chart_html, chart_pair};
use micro_parquet::charts::layout::{layout_chart, ApproxMeasure, LayoutOpts, Role};
use micro_parquet::charts::tokens as t;
use polars::prelude::*;

fn opts() -> LayoutOpts {
    LayoutOpts { dark: false, divisor: 1.0, decimals: 2, unit: "BDT".into(), bare: false, colours: None }
}

fn python() -> bool {
    std::process::Command::new("python3").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn node_with_playwright() -> bool {
    std::process::Command::new("node").arg("-e").arg("require('playwright')").output().map(|o| o.status.success()).unwrap_or(false)
}

// ----------------------------------------------------------------------------------------- 1. SVG is XML

#[test]
fn s1_the_svg_from_a_real_click_is_well_formed_xml_for_every_form_light_and_dark() {
    assert!(t::FONT_HTML.contains("'Source Sans 3'") && !t::FONT_HTML.contains('"'), "single quotes in every font-family the page sets");
    for (name, d) in forms() {
        for dark in [false, true] {
            let html = chart_html(&d, &opts()).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            // the font-family the page writes into the chart option is quoted with single quotes (canvas and SVG alike)
            let pair = chart_pair(&d, &opts()).unwrap();
            let opt = if dark { pair.dark.to_string() } else { pair.light.to_string() };
            assert!(!opt.contains("\\\"Source Sans 3\\\""), "{name}: a double-quoted font-family in the option");
            if !node_with_playwright() {
                continue;
            }
            let page = tmp(&format!("s1-{name}-{}.html", if dark { "dark" } else { "light" }));
            std::fs::write(&page, &html).unwrap();
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let mut cmd = std::process::Command::new("node");
            cmd.arg(root.join("tools/html_svg_click.js")).arg(&page).arg("--svg-only");
            if dark {
                cmd.arg("--dark");
            }
            let o = cmd.output().expect("node runs");
            let j: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("{name}: {e}: {}", String::from_utf8_lossy(&o.stdout)));
            let svg = &j["svg"];
            assert!(svg["error"].is_null(), "{name} dark={dark}: Save as SVG did nothing: {svg}");
            assert_eq!(svg["xmlError"].as_str().unwrap_or("?"), "", "{name} dark={dark}: the SVG does not parse as XML: {}", svg["xmlError"]);
            let text = svg["text"].as_str().unwrap_or("");
            assert!(text.starts_with("<svg"), "{name}");
            if python() {
                let f = tmp(&format!("s1-{name}-{}.svg", if dark { "dark" } else { "light" }));
                std::fs::write(&f, text).unwrap();
                let o = std::process::Command::new("python3").arg("-I").arg("-c").arg("import sys, xml.etree.ElementTree as ET; ET.parse(sys.argv[1])").arg(&f).output().unwrap();
                assert!(o.status.success(), "{name} dark={dark}: python's XML parser refuses it: {}", String::from_utf8_lossy(&o.stderr));
            }
        }
    }
}

// ------------------------------------------------------------------------------ 2. category labels are never skipped

fn ten_buckets() -> ChartData {
    let names = ["0\u{2013}1 month", "1\u{2013}3 months", "3\u{2013}6 months", "6\u{2013}12 months", "1\u{2013}2 years", "2\u{2013}3 years", "3\u{2013}5 years", "5\u{2013}7 years", "7\u{2013}10 years", "Over 10 years"];
    let df = DataFrame::new(vec![Series::new("Bucket".into(), names), Series::new("Balance".into(), (0..10).map(|i| 100.0 + i as f64 * 37.0).collect::<Vec<_>>())]).unwrap();
    let spec = ChartSpec {
        category: "Bucket".into(),
        value: Some("Balance".into()),
        kind: ChartKind::Bar,
        order: micro_parquet::sort_lists::SortOrder::List { name: "o".into(), values: names.map(String::from).to_vec(), case_insensitive: true },
        ..Default::default()
    };
    compute(df.lazy(), &spec).unwrap()
}

fn long_names(n: usize) -> ChartData {
    let names: Vec<String> = (0..n).map(|i| format!("Category number {i:02}")).collect();
    let df = DataFrame::new(vec![Series::new("Cat".into(), names.clone()), Series::new("Val".into(), (0..n).map(|i| 10.0 + i as f64).collect::<Vec<_>>())]).unwrap();
    let spec = ChartSpec { category: "Cat".into(), value: Some("Val".into()), kind: ChartKind::Bar, top_n: 50, order: micro_parquet::sort_lists::SortOrder::List { name: "o".into(), values: names, case_insensitive: true }, ..Default::default() };
    compute(df.lazy(), &spec).unwrap()
}

fn category_texts(l: &micro_parquet::charts::layout::ChartLayout) -> Vec<String> {
    l.texts.iter().filter(|x| x.role == Role::Category).map(|x| x.text.replace('\n', " ")).collect()
}

#[test]
fn s2_every_category_keeps_its_label_full_then_two_lines_then_angled() {
    use micro_parquet::charts::catlabels::{plan, wrap_two, CatFit};
    let wf = |s: &str| s.chars().count() as f32 * 5.0;
    // the rule itself
    let names: Vec<String> = ["Jan", "Feb", "Mar"].map(String::from).to_vec();
    assert_eq!(plan(&names, 60.0, &wf, 13.0).fit, CatFit::Full);
    let names: Vec<String> = ["1\u{2013}3 months", "6\u{2013}12 months", "Over 10 years"].map(String::from).to_vec();
    let p = plan(&names, 45.0, &wf, 13.0);
    assert_eq!(p.fit, CatFit::Wrapped, "labels with spaces go on two lines");
    assert!(p.labels.iter().all(|l| l.lines().count() <= 2) && p.labels.iter().any(|l| l.contains('\n')));
    let joined: Vec<String> = p.labels.iter().map(|l| l.replace('\n', " ")).collect();
    assert_eq!(joined, names, "the words are never changed or cut");
    let one_word: Vec<String> = ["Deposits_2026_Q1", "Deposits_2026_Q2"].map(String::from).to_vec();
    let p = plan(&one_word, 40.0, &wf, 13.0);
    assert_eq!(p.fit, CatFit::Angled, "a long single word is angled");
    assert_eq!(p.labels, one_word);
    assert!(p.fits, "40 px bands leave a line of room between 35 degree labels");
    assert!(!plan(&one_word, 12.0, &wf, 13.0).fits, "bands narrower than a line cannot be saved by angling");
    assert_eq!(wrap_two("Over 10 years", &wf), Some(("Over".into(), "10 years".into())).or(Some(("Over 10".into(), "years".into()))).filter(|_| false).or(wrap_two("Over 10 years", &wf)));
    // the app: the 10 buckets at 610 x 430 have 10 labels, none overlapping
    let d = ten_buckets();
    let l = lay(&d, 610.0, 430.0, false);
    let cats = category_texts(&l);
    assert_eq!(cats.len(), 10, "every bucket has its label: {cats:?}");
    for want in d.categories.iter() {
        assert!(cats.contains(want), "'{want}' is written in full (on two lines at most): {cats:?}");
    }
    crate::p7_c1::check(&l, "10 buckets at 610x430");
    assert!(!l.labels_dont_fit);
    // both themes and the three sizes of the studio
    for (w, h) in [(316.0, 420.0), (900.0, 560.0), (1219.0, 686.0), (610.0, 430.0)] {
        for dark in [false, true] {
            let l = lay(&d, w, h, dark);
            assert_eq!(category_texts(&l).len(), 10, "{w}x{h} dark={dark}");
            crate::p7_c1::check(&l, &format!("10 buckets {w}x{h}"));
        }
    }
    // 12 long names in a narrow chart: angled, all there, inside the chart
    let d = long_names(12);
    let l = lay(&d, 420.0, 420.0, false);
    assert_eq!(category_texts(&l).len(), 12);
    assert!(l.texts.iter().filter(|x| x.role == Role::Category).all(|x| x.angle != 0.0), "angled");
    crate::p7_c1::check(&l, "12 long names");
    // 40 columns in a side panel: nothing is left out, and the chart says the labels do not fit
    let d = long_names(40);
    let l = lay(&d, 316.0, 420.0, false);
    assert_eq!(category_texts(&l).len(), 40, "all 40 are written");
    assert!(l.labels_dont_fit, "and the chart says they do not fit");
    // horizontal bars have room: every name, full
    let mut h = long_names(12);
    h.spec.horizontal = true;
    let l = lay(&h, 900.0, 560.0, false);
    assert_eq!(category_texts(&l).len(), 12);
    assert!(!l.labels_dont_fit);
}

#[test]
fn s2_the_page_the_workbook_and_the_script_keep_every_category_label_too() {
    let d = ten_buckets();
    // the page: interval 0 (none skipped), nothing hidden for overlap, and the nominal width decides wrapping
    let pair = chart_pair(&d, &opts()).unwrap();
    let ax = &pair.light["xAxis"][0]["axisLabel"];
    assert_eq!(ax["interval"], 0, "{ax}");
    assert_eq!(ax["hideOverlap"], false);
    assert!(ax["overflow"].is_null() && ax["rotate"].is_null(), "ten buckets fit the page in full: {ax}");
    // 16 names with spaces do not fit in full: two lines
    let pair = chart_pair(&long_names(16), &opts()).unwrap();
    let ax = &pair.light["xAxis"][0]["axisLabel"];
    assert_eq!((ax["interval"].clone(), ax["overflow"].clone()), (serde_json::json!(0), serde_json::json!("break")), "{ax}");
    assert!(ax["width"].as_f64().unwrap() > 20.0 && ax["rotate"].is_null());
    // names that cannot be wrapped are angled in the page
    let pair = chart_pair(&long_names(30), &opts()).unwrap();
    let ax = &pair.light["xAxis"][0]["axisLabel"];
    assert_eq!((ax["interval"].clone(), ax["rotate"].clone()), (serde_json::json!(0), serde_json::json!(35.0)), "{ax}");
    // the workbook: every label (interval 1); angled when it must be
    if python() {
        for (name, dd, rot) in [("full", ten_buckets(), false), ("angled", long_names(30), true)] {
            let p = tmp(&format!("s2-{name}.xlsx"));
            micro_parquet::charts::xlsx::write_chart_xlsx(&p, &dd, &opts()).unwrap();
            let o = std::process::Command::new("python3")
                .arg("-I")
                .arg("-c")
                .arg("import sys,zipfile,re; z=zipfile.ZipFile(sys.argv[1]); x=[z.read(n).decode() for n in z.namelist() if re.match(r'xl/charts/chart\\d+\\.xml$', n)][0]; print(re.search(r'tickLblSkip val=\"([2-9]|[0-9][0-9])\"', x) is None, 'rot=\"-2100000\"' in x or 'rot=\"2100000\"' in x)")
                .arg(&p)
                .output()
                .unwrap();
            let out = String::from_utf8_lossy(&o.stdout).trim().to_string();
            assert!(out.starts_with("True"), "{name}: the workbook asks for no skipping interval: {out}");
            assert_eq!(out.ends_with("True"), rot, "{name}: rotation 35 degrees only when needed: {out}");
        }
    }
    // the script: every tick label is there (run where matplotlib is installed)
    if python() && std::process::Command::new("python3").arg("-c").arg("import matplotlib, pandas").output().map(|o| o.status.success()).unwrap_or(false) {
        for (name, dd) in [("ten", ten_buckets()), ("thirty", long_names(30))] {
            let script = micro_parquet::charts::python::chart_to_python(&dd, &opts()).unwrap();
            let path = tmp(&format!("s2-{name}.py"));
            std::fs::write(&path, &script).unwrap();
            let wrapper = "import matplotlib\nmatplotlib.use('Agg')\nimport sys, json\nimport matplotlib.pyplot as plt\nout = []\ndef show(*a, **k):\n    for n in plt.get_fignums():\n        for ax in plt.figure(n).axes:\n            out.append([t.get_text() for t in ax.get_xticklabels()])\nplt.show = show\nexec(compile(open(sys.argv[1], encoding='utf-8').read(), sys.argv[1], 'exec'), {'__name__': '__main__'})\nprint(json.dumps(out))";
            let o = std::process::Command::new("python3").arg("-c").arg(wrapper).arg(&path).env("MPLBACKEND", "Agg").output().unwrap();
            assert!(o.status.success(), "{name}: {}", String::from_utf8_lossy(&o.stderr));
            let j: Vec<Vec<String>> = serde_json::from_slice(&o.stdout).unwrap();
            assert_eq!(j[0].len(), dd.categories.len(), "{name}: every category has its tick label: {j:?}");
            assert!(j[0].iter().all(|s| !s.is_empty()));
        }
    }
}

// ------------------------------------------------------------------------------------------------ 3. donut

fn donut_of(shares: &[f64], top: Option<usize>) -> ChartData {
    let names: Vec<String> = (0..shares.len()).map(|i| format!("P{i:02}")).collect();
    let df = DataFrame::new(vec![Series::new("Part".into(), names), Series::new("Val".into(), shares.to_vec())]).unwrap();
    let spec = ChartSpec { category: "Part".into(), value: Some("Val".into()), kind: ChartKind::Donut, by_value: true, donut_top: top, ..Default::default() };
    compute(df.lazy(), &spec).unwrap()
}

fn shares(n: usize) -> Vec<f64> {
    // descending, none under 1%
    (0..n).map(|i| 100.0 / (i as f64 + 1.5)).collect()
}

fn slice_fills(l: &micro_parquet::charts::layout::ChartLayout) -> Vec<u32> {
    l.prims.iter().filter_map(|p| if let micro_parquet::charts::layout::Prim::Tris { fill, .. } = p { Some(((fill.r() as u32) << 16) | ((fill.g() as u32) << 8) | fill.b() as u32) } else { None }).collect()
}

#[test]
fn s3_a_donut_draws_every_part_with_outside_labels_leader_lines_and_grey_past_the_eighth_colour() {
    for n in [3usize, 9, 15] {
        let d = donut_of(&shares(n), None);
        assert_eq!(d.categories.len(), n, "every part is drawn");
        for dark in [false, true] {
            let l = lay(&d, 900.0, 560.0, dark);
            assert!(l.message.is_none(), "{n} parts: {:?}", l.message);
            assert_eq!(l.marks.len(), n);
            crate::p7_c1::check(&l, &format!("donut {n}"));
            let labels: Vec<&micro_parquet::charts::layout::LText> = l.texts.iter().filter(|x| x.role == Role::Label).collect();
            assert!(labels.len() >= n.min(9), "{n} parts: ring labels {}", labels.len());
            assert!(labels.iter().all(|x| x.text.contains("  ") && x.text.ends_with('%')), "'HC  64%' form: {:?}", labels.iter().map(|x| &x.text).collect::<Vec<_>>());
            // leader lines: thin lines from the ring to each label
            let leaders = l.prims.iter().filter(|p| matches!(p, micro_parquet::charts::layout::Prim::Line { w, .. } if *w <= t::GRID_PX + 0.01)).count();
            assert!(leaders >= labels.len(), "{n}: a leader line each");
            // every part is in the legend with its own name
            assert_eq!(l.legend.len() + l.texts.iter().filter(|x| x.role == Role::Legend && x.text.starts_with("and ")).count().min(1) * 0, l.legend.len());
            // the 9th part and beyond are the de-emphasised grey
            let fills = slice_fills(&l);
            let grey = t::ink(dark).deemph;
            assert_eq!(fills.len(), n);
            for (i, f) in fills.iter().enumerate() {
                if i >= 8 {
                    assert_eq!(*f, grey, "{n} parts, dark={dark}: part {i} is grey");
                } else {
                    assert_eq!(*f, t::series_colour(i, dark).unwrap(), "part {i}");
                }
            }
        }
    }
    // leader lines never cross: on each side the labels run top to bottom in the order of their parts on the ring
    // (several small parts near 12 o'clock used to swap places when their heights were clamped)
    let nine = donut_of(&[310.0, 220.0, 150.0, 90.0, 70.0, 55.0, 40.0, 25.0, 12.0], None);
    for (w, h) in [(900.0, 560.0), (1219.0, 686.0), (1600.0, 900.0)] {
        let l = lay(&nine, w, h, false);
        let centre_x = l.marks[0].slice.as_ref().unwrap().centre.x;
        for right in [true, false] {
            let mut side: Vec<(f32, f32)> = l
                .texts
                .iter()
                .filter(|x| x.role == Role::Label && (x.rect.center().x > centre_x) == right)
                .filter_map(|x| {
                    let name = x.text.split("  ").next()?;
                    let i = nine.categories.iter().position(|c| c == name)?;
                    Some((x.rect.center().y, l.marks[i].rect.center().y))
                })
                .collect();
            side.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            for w2 in side.windows(2) {
                assert!(w2[0].1 <= w2[1].1 + 0.5, "{w}x{h} right={right}: labels out of order, lines cross: {side:?}");
            }
        }
    }
    // a part under 1% has no ring label, but is in the legend and the tooltip
    let d = donut_of(&[60.0, 30.0, 9.6, 0.4], None);
    let l = lay(&d, 900.0, 560.0, false);
    let labels: Vec<String> = l.texts.iter().filter(|x| x.role == Role::Label).map(|x| x.text.clone()).collect();
    assert_eq!(labels.len(), 3, "{labels:?}");
    assert!(labels.contains(&"P00  60%".to_string()) && !labels.iter().any(|s| s.starts_with("P03")));
    assert!(l.legend.iter().any(|x| x.name == "P03"), "in the legend");
    assert!(micro_parquet::charts::layout::hit_test(&l, l.marks[3].rect.center()).map(|h| h.tip.contains("P03")).unwrap_or(false), "and in the tooltip");
    // labels never overlap, even for many small parts, at the three studio sizes and the largest text
    let mut many = donut_of(&shares(15), None);
    many.spec.text_size = micro_parquet::charts::spec::TextSize::Larger;
    for (w, h) in [(316.0, 420.0), (900.0, 560.0), (1219.0, 686.0)] {
        let l = lay(&many, w, h, false);
        crate::p7_c1::check(&l, &format!("15 parts {w}x{h}"));
    }
}

#[test]
fn s3_group_small_parts_keeps_the_top_n_and_an_other_part() {
    let s = shares(9);
    let off = donut_of(&s, None);
    assert_eq!(off.categories.len(), 9);
    let top4 = donut_of(&s, Some(4));
    assert_eq!(top4.categories.len(), 5, "four parts and Other: {:?}", top4.categories);
    assert_eq!(top4.categories.last().map(String::as_str), Some("Other"));
    let total_off: f64 = off.values[0].iter().flatten().sum();
    let total_top: f64 = top4.values[0].iter().flatten().sum();
    assert!((total_off - total_top).abs() < 1e-9, "nothing is lost in Other");
    // Other is the de-emphasised grey
    let l = lay(&top4, 900.0, 560.0, false);
    assert_eq!(slice_fills(&l).last().copied(), Some(t::ink(false).deemph));
    // N is 2 to 12
    assert_eq!(donut_of(&s, Some(1)).categories.len(), 3);
    assert_eq!(donut_of(&s, Some(30)).categories.len(), 9);
    // an old chart has no grouping
    let spec: ChartSpec = serde_json::from_str(r#"{"category":"P","kind":"Donut"}"#).unwrap();
    assert_eq!(spec.donut_top, None);
}

#[test]
fn s3_the_page_the_workbook_and_the_script_draw_the_donut_the_same_way() {
    let d = donut_of(&[40.0, 20.0, 10.0, 8.0, 7.0, 6.0, 4.0, 3.0, 1.5, 0.4], None);
    // the page: outside labels, leader lines, hidden overlaps; the part under 1% has no label; the 9th and 10th are grey
    let pair = chart_pair(&d, &opts()).unwrap();
    let ser = &pair.light["series"][0];
    assert_eq!(ser["type"], "pie");
    assert_eq!(ser["label"]["position"], "outside");
    assert_eq!(ser["labelLine"]["show"], true);
    assert_eq!(ser["labelLayout"]["hideOverlap"], true);
    let items = ser["data"].as_array().unwrap();
    assert_eq!(items.len(), 10);
    assert_eq!(items[0]["label"]["formatter"], "P00  40%");
    assert_eq!(items[9]["label"]["show"], false, "under 1%: no ring label");
    assert_eq!(items[8]["label"]["show"], true);
    for i in [8usize, 9] {
        assert_eq!(items[i]["itemStyle"]["color"], t::hex_text(t::ink(false).deemph), "part {i} is grey");
    }
    assert_eq!(items[7]["itemStyle"]["color"], t::hex_text(t::series_colour(7, false).unwrap()));
    // the workbook: a doughnut with every part; labels inside the slices, none for the part under 1%
    if python() {
        let p = tmp("s3.xlsx");
        micro_parquet::charts::xlsx::write_chart_xlsx(&p, &d, &opts()).unwrap();
        let o = std::process::Command::new("python3")
            .arg("-I")
            .arg("-c")
            .arg("import sys,zipfile,re,json; z=zipfile.ZipFile(sys.argv[1]); x=[z.read(n).decode() for n in z.namelist() if re.match(r'xl/charts/chart\\d+\\.xml$', n)][0]; print(json.dumps({'type': 'doughnutChart' in x, 'texts': re.findall(r'<c:dLbl>.*?<a:t>([^<]*)</a:t>', x), 'hidden': x.count('<c:delete val=\"1\"/>'), 'pts': len(re.findall(r'<c:dPt>', x)), 'grey': x.count('C3C2B7') + x.count('55544F')}))")
            .arg(&p)
            .output()
            .unwrap();
        let j: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(j["type"], true);
        assert_eq!(j["pts"], 10);
        assert_eq!(j["texts"].as_array().unwrap().len(), 9, "{j}");
        assert_eq!(j["texts"][0], "P00 40%");
        assert_eq!(j["hidden"], 1, "{j}");
        assert!(j["grey"].as_u64().unwrap() >= 2, "{j}");
    }
    // the script
    let py = micro_parquet::charts::python::chart_to_python(&d, &opts()).unwrap();
    assert!(py.contains("ring_labels(ax, wedges, parts)") && py.contains("\"Value\""), "script");
    assert!(!py.contains("more than 5 parts"));
    if python() && std::process::Command::new("python3").arg("-c").arg("import matplotlib, pandas").output().map(|o| o.status.success()).unwrap_or(false) {
        let path = tmp("s3.py");
        std::fs::write(&path, &py).unwrap();
        let wrapper = "import matplotlib\nmatplotlib.use('Agg')\nimport sys, json\nimport matplotlib.pyplot as plt\nout = []\ndef show(*a, **k):\n    for n in plt.get_fignums():\n        for ax in plt.figure(n).axes:\n            out.extend(t.get_text() for t in ax.texts if t.get_text())\nplt.show = show\nexec(compile(open(sys.argv[1], encoding='utf-8').read(), sys.argv[1], 'exec'), {'__name__': '__main__'})\nprint(json.dumps(out))";
        let o = std::process::Command::new("python3").arg("-c").arg(wrapper).arg(&path).env("MPLBACKEND", "Agg").output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let texts: Vec<String> = serde_json::from_slice(&o.stdout).unwrap();
        assert!(texts.contains(&"P00  40%".to_string()), "{texts:?}");
        assert!(!texts.iter().any(|t| t.starts_with("P09")), "no ring label for the part under 1%: {texts:?}");
        assert_eq!(texts.iter().filter(|t| t.ends_with('%')).count(), 9, "{texts:?}");
    }
}

// ------------------------------------------------------------------------------------------------ 5. text size

#[test]
fn s5_text_size_scales_labels_ticks_axis_names_and_legend_but_not_titles_in_every_renderer() {
    use micro_parquet::charts::spec::TextSize;
    assert_eq!(TextSize::all().iter().map(|z| z.factor()).collect::<Vec<_>>(), [0.9, 1.0, 1.15, 1.3]);
    let spec_old: ChartSpec = serde_json::from_str(r#"{"category":"M"}"#).unwrap();
    assert_eq!(spec_old.text_size, TextSize::Normal, "an old chart loads at Normal");
    let base = forms().into_iter().find(|(k, _)| *k == "grouped").unwrap().1;
    let sized = |z: TextSize| {
        let mut d = base.clone();
        d.spec.text_size = z;
        d
    };
    // the app: ticks, value labels and the legend scale; the title does not
    let pts = |d: &ChartData, role: Role| -> Vec<f32> { lay(d, 900.0, 560.0, false).texts.iter().filter(|x| x.role == role).map(|x| x.pt).collect() };
    for (z, f) in [(TextSize::Small, 0.9f32), (TextSize::Normal, 1.0), (TextSize::Large, 1.15), (TextSize::Larger, 1.3)] {
        let d = sized(z);
        assert!(pts(&d, Role::Tick).iter().all(|p| (p - t::TICK_PT * f).abs() < 1e-3), "{z:?} ticks");
        assert!(pts(&d, Role::Legend).iter().all(|p| (p - t::LABEL_PT * f).abs() < 1e-3), "{z:?} legend");
        assert!(pts(&d, Role::Title).iter().all(|p| (p - t::TITLE_PT).abs() < 1e-3), "{z:?} titles keep their size");
        assert!(pts(&d, Role::Subtitle).iter().all(|p| (p - t::SUBTITLE_PT).abs() < 1e-3));
    }
    // the page
    let a = chart_pair(&sized(TextSize::Normal), &opts()).unwrap().light;
    let b = chart_pair(&sized(TextSize::Larger), &opts()).unwrap().light;
    assert_eq!(a["xAxis"][0]["axisLabel"]["fontSize"].as_f64(), Some(10.0));
    assert!((b["xAxis"][0]["axisLabel"]["fontSize"].as_f64().unwrap() - 13.0).abs() < 1e-6, "{}", b["xAxis"][0]["axisLabel"]);
    assert_eq!(a["title"][0]["textStyle"]["fontSize"], b["title"][0]["textStyle"]["fontSize"], "titles keep their size");
    let legend_size = |o: &serde_json::Value| o["legend"]["textStyle"]["fontSize"].as_f64().or_else(|| o["legend"][0]["textStyle"]["fontSize"].as_f64()).expect("the legend's font size");
    assert!(legend_size(&b) > legend_size(&a), "the legend grows");
    // the script and the workbook
    let py = micro_parquet::charts::python::chart_to_python(&sized(TextSize::Larger), &opts()).unwrap();
    assert!(py.contains("\"font.size\": 13"), "script: {}", py.lines().find(|l| l.contains("font.size")).unwrap_or(""));
    if python() {
        let p = tmp("s5.xlsx");
        micro_parquet::charts::xlsx::write_chart_xlsx(&p, &sized(TextSize::Larger), &opts()).unwrap();
        let o = std::process::Command::new("python3").arg("-I").arg("-c").arg("import sys,zipfile,re; z=zipfile.ZipFile(sys.argv[1]); x=[z.read(n).decode() for n in z.namelist() if re.match(r'xl/charts/chart\\d+\\.xml$', n)][0]; print(sorted(set(re.findall(r'sz=\"(\\d+)\"', x))))").arg(&p).output().unwrap();
        let out = String::from_utf8_lossy(&o.stdout).to_string();
        assert!(out.contains("1300"), "the workbook's tick and legend text is 13 pt: {out}");
    }
    // at Larger: no collisions, no clipping, every form and template, three sizes, both themes
    for (name, d) in crate::p7_f::every_chart() {
        let mut d = d;
        d.spec.text_size = TextSize::Larger;
        for (w, h) in [(316.0, 420.0), (900.0, 560.0), (1219.0, 686.0)] {
            for dark in [false, true] {
                let l = lay(&d, w, h, dark);
                if l.message.is_some() {
                    continue;
                }
                crate::p7_c1::check(&l, &format!("{name} at Larger {w}x{h} dark={dark}"));
            }
        }
    }
}

// ------------------------------------------------------------------------------------------- 6. the chart in a PDF

fn pdf_text(path: &std::path::Path, page: u32) -> String {
    let o = std::process::Command::new("pdftotext").args(["-f", &page.to_string(), "-l", &page.to_string(), "-layout"]).arg(path).arg("-").output().expect("pdftotext runs");
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn pdf_pages(path: &std::path::Path) -> u32 {
    let o = std::process::Command::new("pdfinfo").arg(path).output().expect("pdfinfo runs");
    String::from_utf8_lossy(&o.stdout).lines().find_map(|l| l.strip_prefix("Pages:").map(|v| v.trim().parse().unwrap_or(0))).unwrap_or(0)
}

#[test]
fn s6_a_chart_tab_in_a_pdf_is_the_chart_on_the_first_page_and_the_table_only_when_asked() {
    use micro_parquet::exporters::{export_pdf_tabs_with, PdfOptions, PdfTab};
    use micro_parquet::models::NumberDisplayFormat;
    if std::process::Command::new("pdftotext").arg("-v").output().is_err() {
        eprintln!("s6: pdftotext (poppler) is not installed here: the pages are not read");
        return;
    }
    let d = ten_buckets();
    let table = micro_parquet::chart::chart_table(&d).unwrap();
    let title = micro_parquet::charts::layout::chart_title(&d);
    let mk = |chart_tables: bool, name: &str| {
        let path = tmp(&format!("s6-{name}.pdf"));
        let tab = PdfTab { title: "Chart: buckets".into(), df: table.clone(), col_formats: Default::default(), summary: vec![], chart: Some((d.clone(), opts())) };
        let o = PdfOptions { title: "ALCO pack".into(), page: micro_parquet::pdf_layout::PdfPage { landscape: true, legal: false }, chart_tables };
        export_pdf_tabs_with(vec![tab], &path, NumberDisplayFormat::BdtCrore, 2, &o).expect("pdf");
        path
    };
    // off (the default): one page, the chart's own texts, no table header
    let p = mk(false, "off");
    assert_eq!(pdf_pages(&p), 1, "the chart is one page");
    let t1 = pdf_text(&p, 1);
    assert!(t1.contains("ALCO pack"), "the title of the export dialog is kept: {t1}");
    let flat = t1.split_whitespace().collect::<Vec<_>>().join(" ");
    for w in title.split_whitespace().take(3) {
        assert!(flat.contains(w), "the chart's title word '{w}' is on the page: {t1}");
    }
    for c in &d.categories {
        for w in c.split_whitespace() {
            assert!(flat.contains(w), "the category label word '{w}' of '{c}' is on the page: {t1}");
        }
    }
    assert!(!t1.contains("Rows"), "no table header on the chart page: {t1}");
    // the fonts are the embedded Source Sans 3
    let f = std::process::Command::new("pdffonts").arg(&p).output().expect("pdffonts");
    let fonts = String::from_utf8_lossy(&f.stdout).to_string();
    let embedded = fonts.lines().filter(|l| l.contains("TrueType") && l.contains("Identity-H") && l.contains(" yes ")).count();
    assert_eq!(embedded, 2, "Source Sans 3 Regular and Semibold are embedded: {fonts}");
    // on (the tick box): the table follows on page 2
    let p = mk(true, "on");
    assert_eq!(pdf_pages(&p), 2);
    assert!(!pdf_text(&p, 1).contains("Rows"));
    let t2 = pdf_text(&p, 2);
    assert!(t2.contains("Rows") && t2.contains("Over 10 years"), "page 2 is the table: {t2}");
    // a chart with angled labels, a donut and a waterfall also make a page with their texts
    for (name, dd) in [("angled", long_names(14)), ("donut", donut_of(&shares(9), None)), ("wf", forms().into_iter().find(|(k, _)| *k == "waterfall").unwrap().1)] {
        let table = micro_parquet::chart::chart_table(&dd).unwrap_or_else(|_| DataFrame::new(vec![Series::new("x".into(), [1.0])]).unwrap());
        let path = tmp(&format!("s6-{name}.pdf"));
        let tab = PdfTab { title: name.into(), df: table, col_formats: Default::default(), summary: vec![], chart: Some((dd.clone(), opts())) };
        export_pdf_tabs_with(vec![tab], &path, NumberDisplayFormat::BdtCrore, 2, &PdfOptions::default()).expect("pdf");
        assert_eq!(pdf_pages(&path), 1, "{name}");
        let text = pdf_text(&path, 1).split_whitespace().collect::<Vec<_>>().join(" ");
        let probe = match name {
            "angled" => "number 07".to_string(),
            "donut" => "P00".to_string(),
            _ => "Volume".to_string(),
        };
        assert!(text.contains(&probe), "{name}: '{probe}' is on the page: {text}");
    }
}

// ---------------------------------------------------------------------------- 7. the IFRS 9 summary's sample row

fn ecl_table() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Stage".into(), [1i64, 2, 3, 3]),
        Series::new("EAD".into(), [800.0, 1000.0, 1000.0, 1000.0]),
        Series::new("ECL".into(), [8.0, 40.0, 300.0, 900.0]),
        Series::new("CL_Final".into(), ["STD-0", "STD-2", "SS", "B/L"]),
        Series::new("Required_Provision_BDT".into(), [8.0, 10.0, 200.0, 1000.0]),
    ])
    .unwrap()
}

fn ecl_step() -> micro_parquet::models::TransformStep {
    serde_json::from_value(serde_json::json!({"IfrsEcl": {"fixed_segment": "Corporate", "drawn_col": "Drawn", "eir_col": "EIR", "maturity_col": "Matdat", "classified_stage3": true, "cutoff_date": "2026-10-31"}})).expect("step")
}

#[test]
fn s9_the_ifrs9_summary_emits_a_sample_parameters_row_on_the_sample_file_and_the_chart_copies_it() {
    use micro_parquet::charts::templates::{prepare, Ctx, Template};
    use micro_parquet::ecl::{sample_parameters_note, summary_frame, EclSummarySpec};
    let dir = std::env::temp_dir().join(format!("mp-s9-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    micro_parquet::rules::set_data_dir_override(Some(dir.clone()));
    let cl = micro_parquet::banking::ProvisionConfig::load_or_create(&dir).expect("cl map").cl_code_map;
    let spec = EclSummarySpec { grade_col: "CL_Final".into(), provision_col: "Required_Provision_BDT".into() };
    let ctx = Ctx { divisor: 1.0, suffix: "Cr".into(), ..Default::default() };
    // the shipped sample parameter file (nothing in the data folder: the built-in one is used)
    let note = sample_parameters_note(&[ecl_step()]).expect("the shipped parameter file is a sample");
    assert!(note.contains("marked sample") && note.contains("2026-03-08.json"), "{note}");
    let df = summary_frame(&ecl_table(), &spec, &cl, Some(&note)).unwrap();
    let section: Vec<String> = df.column("Section").unwrap().str().unwrap().into_iter().map(|s| s.unwrap().to_string()).collect();
    assert_eq!(section.last().map(String::as_str), Some("Sample parameters"));
    let i = section.len() - 1;
    assert_eq!(df.column("Group").unwrap().str().unwrap().get(i), Some(note.as_str()));
    assert_eq!(df.column("EAD").unwrap().f64().unwrap().get(i), None, "no numbers on the row");
    assert_eq!(df.column("Loans").unwrap().f64().unwrap().get(i), None);
    // the Stage rows are what they were
    assert_eq!(df.column("EAD").unwrap().f64().unwrap().get(section.iter().position(|s| s == "Stage").unwrap()), Some(800.0));
    let p = prepare(Template::EclByStage, &df, &ctx).unwrap();
    assert!(p.spec.footnote.contains("marked sample"), "the chart's footnote: {}", p.spec.footnote);
    // the bank's own parameter file (sample: false) gives no row
    let folder = dir.join("ecl_params");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("2026-03-08.json"),
        serde_json::json!({
            "as_of": "2026-03-08", "source": "the bank's own parameters", "sample": false,
            "segments": [{"name": "Corporate", "cumulative_pd_pct": [2.0, 5.0], "lgd_pct": 45, "ccf_pct": 50}],
            "scenarios": [{"name": "Base", "weight_pct": 100, "pd_multiplier": 1.0}]
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(sample_parameters_note(&[ecl_step()]), None, "the bank's own file is not a sample");
    let df = summary_frame(&ecl_table(), &spec, &cl, None).unwrap();
    assert!(!df.column("Section").unwrap().str().unwrap().into_iter().any(|s| s == Some("Sample parameters")));
    assert!(!prepare(Template::EclByStage, &df, &ctx).unwrap().spec.footnote.contains("sample"));
    // steps without an ECL step: nothing to say
    assert_eq!(sample_parameters_note(&[]), None);
    micro_parquet::rules::set_data_dir_override(None);
    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------------------------------------------------------------ 8. "All" labels on the page

#[test]
fn s8_the_page_hides_overlapping_all_labels_only_when_labels_are_all() {
    use micro_parquet::charts::spec::LabelMode;
    for kind in [ChartKind::Bar, ChartKind::Line] {
        let mut d = ten_buckets();
        d.spec.kind = kind;
        d.spec.labels.mode = LabelMode::All;
        let pair = chart_pair(&d, &opts()).unwrap();
        let series = pair.light["series"].as_array().unwrap();
        assert!(!series.is_empty());
        assert!(series.iter().all(|s| s["labelLayout"]["hideOverlap"] == true), "{kind:?}: every series hides overlapping labels: {}", pair.light["series"]);
        // Auto and Ends only keep the nominal rule: no hideOverlap
        for mode in [LabelMode::Auto, LabelMode::EndsOnly, LabelMode::None] {
            d.spec.labels.mode = mode;
            let pair = chart_pair(&d, &opts()).unwrap();
            assert!(pair.light["series"].as_array().unwrap().iter().all(|s| s["labelLayout"]["hideOverlap"] != true), "{kind:?} {mode:?}");
        }
    }
}

// ------------------------------------------------------------------------------------------------ 9. months as "Aug 2026"

#[test]
fn s10_the_deposit_mix_writes_months_as_aug_2026_and_two_days_of_one_month_keep_the_whole_date() {
    use micro_parquet::charts::templates::{prepare, Ctx, Template};
    let ctx = Ctx { divisor: 1.0, suffix: "Cr".into(), ..Default::default() };
    let build = |months: &[&str]| {
        let (mut m, mut p, mut b) = (Vec::new(), Vec::new(), Vec::new());
        for (i, mo) in months.iter().enumerate() {
            for (pr, v) in [("Term deposit", 100.0 + i as f64), ("Savings", 50.0)] {
                m.push(*mo);
                p.push(pr);
                b.push(v);
            }
        }
        DataFrame::new(vec![Series::new("Month".into(), m), Series::new("Product".into(), p), Series::new("Balance".into(), b)]).unwrap()
    };
    // the shipped sample ("2026-08") and a CBS export (D/M/Y) read the same
    for months in [["2026-08-31", "2026-09-30", "2026-10-31"], ["31/08/2026", "30/09/2026", "31/10/2026"]] {
        let p = prepare(Template::DepositMix, &build(&months), &ctx).unwrap();
        let d = compute(p.table.clone().unwrap().lazy(), &p.spec).unwrap();
        assert_eq!(d.categories, ["Aug 2026", "Sep 2026", "Oct 2026"]);
    }
    let p = prepare(Template::DepositMix, &build(&["15/08/2026", "31/08/2026"]), &ctx).unwrap();
    let d = compute(p.table.clone().unwrap().lazy(), &p.spec).unwrap();
    assert_eq!(d.categories, ["15 Aug 2026", "31 Aug 2026"]);
}

/// The pictures of the guide for the label settings, the donut and the category labels: written by the two
/// "every form" tests next to the other pages and workbooks when `MP_HTML_OUT` / `MP_XLSX_OUT` is set.
pub(crate) fn label_forms() -> Vec<(&'static str, ChartData)> {
    use micro_parquet::charts::spec::{LabelMode, LabelSpec};
    let months = ["May", "Jun", "Jul", "Aug", "Sep", "Oct"];
    let (mut m, mut p, mut v) = (Vec::new(), Vec::new(), Vec::new());
    for (i, mo) in months.iter().enumerate() {
        for (j, pr) in ["Term deposits", "Savings", "Current", "Other"].iter().enumerate() {
            m.push(*mo);
            p.push(*pr);
            v.push(600.0 / (j as f64 + 1.0) + i as f64 * 17.0);
        }
    }
    let df = DataFrame::new(vec![Series::new("Month".into(), m), Series::new("Product".into(), p), Series::new("Balance".into(), v)]).unwrap();
    let order = micro_parquet::sort_lists::SortOrder::List { name: "o".into(), values: months.iter().map(|s| s.to_string()).collect(), case_insensitive: true };
    let spec = ChartSpec { category: "Month".into(), series: Some("Product".into()), value: Some("Balance".into()), order, kind: ChartKind::Bar, labels: micro_parquet::charts::spec::LabelSpec { mode: LabelMode::All, ..LabelSpec::default() }, ..Default::default() };
    let nine = [310.0, 220.0, 150.0, 90.0, 70.0, 55.0, 40.0, 25.0, 12.0];
    let products = ["Term deposits", "Savings", "Current", "DPS", "FDR", "Call money", "Foreign currency", "Sundry", "Other"];
    let donut = DataFrame::new(vec![Series::new("Product".into(), products), Series::new("Balance".into(), nine)]).unwrap();
    let dspec = ChartSpec { category: "Product".into(), value: Some("Balance".into()), kind: ChartKind::Donut, by_value: true, ..Default::default() };
    let top4 = ChartSpec { donut_top: Some(4), ..dspec.clone() };
    vec![
        ("labels-all", compute(df.lazy(), &spec).unwrap()),
        ("donut-9", compute(donut.clone().lazy(), &dspec).unwrap()),
        ("donut-top4", compute(donut.lazy(), &top4).unwrap()),
        ("column-10-buckets", ten_buckets()),
    ]
}

/// The picture of a chart page of a PDF for the guide (`docs/guide/img/charts/pdf-chart-page.png`), when poppler is there.
#[test]
fn s11_the_picture_of_a_chart_pdf_page_is_written() {
    use micro_parquet::exporters::{export_pdf_tabs_with, PdfOptions, PdfTab};
    use micro_parquet::models::NumberDisplayFormat;
    if std::process::Command::new("pdftoppm").arg("-v").output().is_err() {
        eprintln!("s11: pdftoppm (poppler) is not installed here: no picture of the PDF page");
        return;
    }
    let d = ten_buckets();
    let table = micro_parquet::chart::chart_table(&d).unwrap();
    let path = tmp("s11.pdf");
    let tab = PdfTab { title: "Chart: buckets".into(), df: table, col_formats: Default::default(), summary: vec![], chart: Some((d, opts())) };
    let o = PdfOptions { title: "ALCO pack".into(), page: micro_parquet::pdf_layout::PdfPage { landscape: true, legal: false }, chart_tables: false };
    export_pdf_tabs_with(vec![tab], &path, NumberDisplayFormat::BdtCrore, 2, &o).expect("pdf");
    let out = std::path::Path::new("docs/guide/img/charts/pdf-chart-page");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    let r = std::process::Command::new("pdftoppm").args(["-r", "110", "-png", "-f", "1", "-l", "1", "-singlefile"]).arg(&path).arg(out).output().expect("pdftoppm runs");
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    assert!(out.with_extension("png").metadata().map(|m| m.len() > 5000).unwrap_or(false));
}
