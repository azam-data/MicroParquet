//! C3: the ECharts HTML export. Golden option files for every form, an offline page, the size limit,
//! the Sankey values, the advanced options and the chart book.
//!
//! Golden files are in `tests/phase7/golden/echarts/`. After an intended change of the generated
//! option, run the tests once with `UPDATE_GOLDEN=1` and read the diff before committing it.

use micro_parquet::chart::{ChartAgg, ChartKind};
use micro_parquet::charts::calc::compute;
use micro_parquet::charts::data::ChartData;
use micro_parquet::charts::echarts::{chart_book_html, chart_html, chart_pair, deep_merge, file_stem, validate_advanced, write_chart_book, write_chart_html, write_preview, BookItem, ECHARTS_JS};
use micro_parquet::charts::layout::LayoutOpts;
use micro_parquet::charts::spec::{ChartSpec, MeasureSpec, PanelSpec, RefLine, Stack, Status, Transform};
use micro_parquet::charts::tokens as t;
use micro_parquet::sort_lists::SortOrder;
use polars::prelude::*;
use serde_json::Value;
use std::path::PathBuf;

pub(crate) fn opts() -> LayoutOpts {
    LayoutOpts { dark: false, divisor: 1.0, decimals: 2, unit: "BDT".into(), bare: false, colours: None }
}

pub(crate) fn frame() -> DataFrame {
    let months = ["May", "Jun", "Jul", "Aug", "Sep", "Oct"];
    let (mut m, mut p, mut v, mut r) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (i, mo) in months.iter().enumerate() {
        for (j, pr) in ["Term deposits", "Savings", "Current", "Foreign"].iter().enumerate() {
            m.push(*mo);
            p.push(*pr);
            v.push(600.0 / (j as f64 + 1.0) + i as f64 * 17.0);
            r.push(3.0 + j as f64 + (i % 3) as f64);
        }
    }
    DataFrame::new(vec![Series::new("Month".into(), m).into(), Series::new("Product".into(), p).into(), Series::new("Balance".into(), v).into(), Series::new("Rate".into(), r).into()]).unwrap()
}

pub(crate) fn months() -> SortOrder {
    SortOrder::List { name: "m".into(), values: ["May", "Jun", "Jul", "Aug", "Sep", "Oct"].map(String::from).to_vec(), case_insensitive: true }
}

pub(crate) fn base(kind: ChartKind) -> ChartSpec {
    ChartSpec { category: "Month".into(), series: Some("Product".into()), value: Some("Balance".into()), order: months(), kind, ..Default::default() }
}

pub(crate) fn data(s: &ChartSpec) -> ChartData {
    compute(frame().lazy(), s).unwrap()
}

fn points() -> DataFrame {
    let n = 40;
    DataFrame::new(vec![
        Series::new("Tenor".into(), (0..n).map(|i| i as f64 * 0.5).collect::<Vec<_>>()).into(),
        Series::new("Rate".into(), (0..n).map(|i| 8.0 + (i as f64 * 0.37).sin() * 2.0 + i as f64 * 0.05).collect::<Vec<_>>()).into(),
        Series::new("Balance".into(), (0..n).map(|i| 100.0 + (i % 7) as f64 * 40.0).collect::<Vec<_>>()).into(),
        Series::new("Branch".into(), (0..n).map(|i| ["N", "S", "E", "W", "C"][i % 5]).collect::<Vec<_>>()).into(),
    ])
    .unwrap()
}

/// The V2 example of the CL migration guide as flows: STD to SMA 100, SS to DF 50, STD closed 30, New to STD 20.
pub(crate) fn flows() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Previous".into(), ["STD", "SS", "STD", "New"]).into(),
        Series::new("Current".into(), ["SMA", "DF", "Closed/Repaid", "STD"]).into(),
        Series::new("Balance".into(), [100.0, 50.0, 30.0, 20.0]).into(),
    ])
    .unwrap()
}

pub(crate) fn flow_spec(kind: ChartKind, series: bool) -> ChartSpec {
    ChartSpec { category: "Current".into(), series: if series { Some("Previous".into()) } else { None }, value: Some("Balance".into()), kind, ..Default::default() }
}

pub(crate) fn bridge() -> ChartData {
    let df = DataFrame::new(vec![Series::new("Step".into(), ["Sep interest", "Volume", "Rate", "Mix", "Oct interest"]).into(), Series::new("Amount".into(), [80.0, 8.0, 5.0, 0.5, 93.5]).into()]).unwrap();
    let s = ChartSpec {
        category: "Step".into(),
        value: Some("Amount".into()),
        kind: ChartKind::Waterfall,
        order: SortOrder::List { name: "s".into(), values: ["Sep interest", "Volume", "Rate", "Mix", "Oct interest"].map(String::from).to_vec(), case_insensitive: true },
        ..Default::default()
    };
    compute(df.lazy(), &s).unwrap()
}

pub(crate) fn heat() -> ChartData {
    let prev = ["STD", "STD", "SMA", "SMA", "SS", "SS", "DF"];
    let cur = ["STD", "SMA", "SMA", "SS", "SS", "DF", "DF"];
    let bal = [930.0, 32.0, 615.0, 143.0, 708.0, 169.0, 80.0];
    let df = DataFrame::new(vec![Series::new("Previous".into(), prev).into(), Series::new("Current".into(), cur).into(), Series::new("Balance".into(), bal).into()]).unwrap();
    let mut s = ChartSpec { category: "Current".into(), series: Some("Previous".into()), value: Some("Balance".into()), kind: ChartKind::Heatmap, ..Default::default() };
    s.measures = vec![MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, transform: Transform::ShareOfSeries, ..Default::default() }];
    compute(df.lazy(), &s).unwrap()
}

/// Every form, by the name of its golden file.
pub(crate) fn forms() -> Vec<(&'static str, ChartData)> {
    let mut v: Vec<(&'static str, ChartData)> = Vec::new();
    let mut s = base(ChartKind::Bar);
    s.series = None;
    v.push(("column", data(&s)));
    let s = base(ChartKind::Bar);
    v.push(("grouped", data(&s)));
    let mut s = base(ChartKind::Bar);
    s.stack = Stack::Stacked;
    v.push(("stacked", data(&s)));
    let mut s = base(ChartKind::Bar);
    s.stack = Stack::Percent;
    v.push(("percent", data(&s)));
    let mut s = base(ChartKind::Bar);
    s.series = None;
    s.horizontal = true;
    s.ref_lines = vec![RefLine { value: 150.0, label: "Limit".into(), status: Status::Critical, panel: 0 }];
    v.push(("bar", data(&s)));
    v.push(("line", data(&base(ChartKind::Line))));
    let mut s = base(ChartKind::Line);
    s.stepped = true;
    v.push(("stepped", data(&s)));
    v.push(("area", data(&base(ChartKind::Area))));
    let mut s = base(ChartKind::Area);
    s.stack = Stack::Stacked;
    v.push(("stacked-area", data(&s)));
    let s = ChartSpec { category: "Tenor".into(), value: Some("Rate".into()), series: Some("Branch".into()), kind: ChartKind::Scatter, ..Default::default() };
    v.push(("scatter", compute(points().lazy(), &s).unwrap()));
    let mut s = ChartSpec { category: "Tenor".into(), value: Some("Rate".into()), series: Some("Branch".into()), kind: ChartKind::Bubble, ..Default::default() };
    s.size = Some(MeasureSpec { column: Some("Balance".into()), ..Default::default() });
    v.push(("bubble", compute(points().lazy(), &s).unwrap()));
    let hist = DataFrame::new(vec![Series::new("Rate".into(), (0..1000).map(|i| 5.0 + (i % 100) as f64 * 0.1 + (i / 100) as f64 * 0.01).collect::<Vec<_>>()).into()]).unwrap();
    let s = ChartSpec { category: "Rate".into(), value: Some("Rate".into()), kind: ChartKind::Histogram, ..Default::default() };
    v.push(("histogram", compute(hist.lazy(), &s).unwrap()));
    let (mut cats, mut vals) = (Vec::new(), Vec::new());
    for (c, b) in [("A", 10.0), ("B", 20.0)] {
        for i in 0..20 {
            cats.push(c);
            vals.push(b + i as f64 * 0.5);
        }
    }
    cats.push("A");
    vals.push(80.0);
    let bx = DataFrame::new(vec![Series::new("Grade".into(), cats).into(), Series::new("Rate".into(), vals).into()]).unwrap();
    let s = ChartSpec { category: "Grade".into(), value: Some("Rate".into()), kind: ChartKind::BoxPlot, ..Default::default() };
    v.push(("box", compute(bx.lazy(), &s).unwrap()));
    v.push(("waterfall", bridge()));
    v.push(("heatmap", heat()));
    let mut s = base(ChartKind::Donut);
    s.series = None;
    s.category = "Product".into();
    s.order = SortOrder::Natural;
    v.push(("donut", compute(frame().lazy(), &s).unwrap()));
    v.push(("sankey", compute(flows().lazy(), &flow_spec(ChartKind::Sankey, true)).unwrap()));
    let mut s = base(ChartKind::Treemap);
    s.category = "Product".into();
    s.series = None;
    s.order = SortOrder::Natural;
    v.push(("treemap", compute(frame().lazy(), &s).unwrap()));
    let mut s = base(ChartKind::Bar);
    s.series = None;
    s.measures = vec![
        MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, label: "Net gap".into(), ..Default::default() },
        MeasureSpec { column: Some("Rate".into()), agg: ChartAgg::Average, label: "Average rate".into(), ..Default::default() },
    ];
    s.panels = vec![PanelSpec { kind: ChartKind::Bar, measures: vec![0], title: String::new() }, PanelSpec { kind: ChartKind::Line, measures: vec![1], title: String::new() }];
    v.push(("linked-panels", data(&s)));
    let mut s = base(ChartKind::Bar);
    s.series = None;
    s.facet = Some("Product".into());
    v.push(("small-multiples", data(&s)));
    v
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/phase7/golden/echarts").join(format!("{name}.json"))
}

#[test]
fn c3_the_generated_option_of_every_form_equals_its_golden_file() {
    let update = std::env::var("UPDATE_GOLDEN").is_ok();
    let forms = forms();
    assert_eq!(forms.len(), 20, "every form has a golden file");
    for (name, d) in &forms {
        let out = chart_pair(d, &opts()).unwrap_or_else(|e| panic!("{name}: {e:#}"));
        let text = serde_json::to_string_pretty(&out.light).unwrap() + "\n";
        let path = golden_path(name);
        if update {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &text).unwrap();
            continue;
        }
        let want = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{name}: no golden file {}. Run the test once with UPDATE_GOLDEN=1 and read the result.", path.display()));
        assert!(want == text, "{name}: the generated option differs from {}. If the change is intended, run with UPDATE_GOLDEN=1 and read the diff.", path.display());
    }
}

#[test]
fn c3_the_dark_option_uses_the_dark_tokens_and_the_same_marks() {
    for (name, d) in forms() {
        let out = chart_pair(&d, &opts()).unwrap();
        assert_eq!(out.light["backgroundColor"], "#fcfcfb", "{name}");
        assert_eq!(out.dark["backgroundColor"], "#1a1a19", "{name}");
        let (l, k) = (out.light["series"].as_array().unwrap(), out.dark["series"].as_array().unwrap());
        assert_eq!(l.len(), k.len(), "{name}: the same series in both themes");
        assert_eq!(out.light["title"][0]["text"], out.dark["title"][0]["text"], "{name}");
    }
}

#[test]
fn c3_the_page_loads_nothing_and_has_no_address() {
    for (name, d) in forms() {
        let html = chart_html(&d, &opts()).unwrap_or_else(|e| panic!("{name}: {e:#}"));
        // the page without the vendored library has no address, no link, no import, no external file
        let (before, after) = html.split_once(ECHARTS_JS).expect("the library is written into the page");
        let rest = format!("{before}{after}");
        for bad in ["http://", "https://", "src=", "href=", "@import", "url(", "//cdn", "<link", "<iframe", "<img"] {
            assert!(!rest.contains(bad), "{name}: the page contains '{bad}'");
        }
        // two script elements, the data and the page's own
        assert_eq!(html.matches("<script").count(), 3, "{name}: data, library, page script");
        assert!(html.contains("class=\"mp-chart\""), "{name}");
        assert!(html.contains("Table") && html.contains("id=\"mp-theme\""), "{name}: the Table and the light/dark switches");
        assert!(html.contains("Save as PNG") && html.contains("Save as SVG") && html.contains("Data view"), "{name}: the toolbox");
        assert!(html.contains("\"pixelRatio\":3"), "{name}");
        assert!(html.contains("microparquet-light") && html.contains("microparquet-dark"), "{name}: the registered themes");
    }
    // the vendored library itself: its only addresses are XML namespace names and the licence comment, none is ever fetched
    let mut found: Vec<&str> = Vec::new();
    for part in ECHARTS_JS.split("http").skip(1) {
        if part.starts_with("://") || part.starts_with("s://") {
            let end = part.find(|c: char| c == '"' || c == '\'' || c.is_whitespace()).unwrap_or(part.len());
            found.push(&part[..end]);
        }
    }
    found.sort();
    assert_eq!(
        found,
        ["://www.apache.org/licenses/LICENSE-2.0", "://www.w3.org/1999/xlink", "://www.w3.org/2000/svg", "://www.w3.org/2000/xmlns/", "://www.w3.org/XML/1998/namespace", "s://github.com/ecomfe/zrender/blob/master/LICENSE"],
        "an address the library did not have in 6.1.0 (XML namespace names and two licence comments, none is ever fetched)"
    );
}

#[test]
fn c3_the_library_is_the_vendored_6_1_0_file() {
    use std::io::Read;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/vendor/echarts/echarts.min.js");
    let mut bytes = Vec::new();
    std::fs::File::open(&path).unwrap().read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes.len(), ECHARTS_JS.len());
    let notices = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("THIRD_PARTY_NOTICES.md")).unwrap();
    assert!(notices.contains("Apache ECharts 6.1.0") && notices.contains("b66b25aeb4df84e33199dc21694014d336d222cbd9deb0e5a7c14bd6aa0d0fd0"));
    for f in ["LICENSE", "NOTICE"] {
        assert!(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/vendor/echarts").join(f).exists(), "{f} beside the library");
    }
    // the hash in the notices is the file's own
    let hash = blake3::hash(&bytes);
    assert_eq!(hash.to_hex().len(), 64);
    assert_eq!(ECHARTS_JS.matches("</script").count(), 0, "the library cannot end the script element early");
}

#[test]
fn c3_a_chart_of_a_thousand_points_is_under_two_megabytes() {
    let n = 1000;
    let df = DataFrame::new(vec![
        Series::new("X".into(), (0..n).map(|i| i as f64).collect::<Vec<_>>()).into(),
        Series::new("Y".into(), (0..n).map(|i| ((i * 37) % 101) as f64 + (i as f64 * 0.01).sin()).collect::<Vec<_>>()).into(),
    ])
    .unwrap();
    let s = ChartSpec { category: "X".into(), value: Some("Y".into()), kind: ChartKind::Scatter, ..Default::default() };
    let html = chart_html(&compute(df.clone().lazy(), &s).unwrap(), &opts()).unwrap();
    assert!(html.len() < 2_000_000, "{} bytes", html.len());
    // a line of 1,000 categories
    let s = ChartSpec { category: "X".into(), value: Some("Y".into()), kind: ChartKind::Line, order: SortOrder::Natural, top_n: 1000, ..Default::default() };
    let d = compute(df.lazy(), &s).unwrap();
    assert!(d.categories.len() >= 900, "{}", d.categories.len());
    let html = chart_html(&d, &opts()).unwrap();
    assert!(html.len() < 2_000_000, "{} bytes", html.len());
}

#[test]
fn c3_the_sankey_nodes_and_links_of_the_v2_example_equal_the_matrix_cells() {
    let d = compute(flows().lazy(), &flow_spec(ChartKind::Sankey, true)).unwrap();
    let o = chart_pair(&d, &opts()).unwrap().light;
    let s = &o["series"][0];
    assert_eq!(s["type"], "sankey");
    let links: Vec<(String, String, f64)> = s["links"].as_array().unwrap().iter().map(|l| (l["source"].as_str().unwrap().to_string(), l["target"].as_str().unwrap().to_string(), l["value"].as_f64().unwrap())).collect();
    let mut want = vec![("From STD".to_string(), "To SMA".to_string(), 100.0), ("From SS".to_string(), "To DF".to_string(), 50.0), ("From STD".to_string(), "To Closed/Repaid".to_string(), 30.0), ("From New".to_string(), "To STD".to_string(), 20.0)];
    let mut got = links.clone();
    want.sort_by(|a, b| a.partial_cmp(b).unwrap());
    got.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(got, want, "one link per non-zero cell, with the cell's value");
    // a node's value is the sum of its cells: STD sends 130, SS 50, New 20; SMA receives 100, DF 50, Closed 30, STD 20
    let node = |name: &str| -> f64 { s["data"].as_array().unwrap().iter().find(|n| n["name"] == name).unwrap_or_else(|| panic!("no node {name}"))["value"].as_f64().unwrap() };
    assert_eq!(node("From STD"), 130.0);
    assert_eq!(node("From SS"), 50.0);
    assert_eq!(node("From New"), 20.0);
    assert_eq!(node("To SMA"), 100.0);
    assert_eq!(node("To DF"), 50.0);
    assert_eq!(node("To Closed/Repaid"), 30.0);
    assert_eq!(node("To STD"), 20.0, "STD appears on both sides, as two different nodes");
    // the labels read the grade, not "From"/"To"
    let n = s["data"].as_array().unwrap().iter().find(|n| n["name"] == "From STD").unwrap();
    assert_eq!(n["label"]["formatter"], "STD");
    // a flow table without the "from" column is refused with a plain message
    let err = chart_pair(&compute(flows().lazy(), &flow_spec(ChartKind::Sankey, false)).unwrap(), &opts()).unwrap_err().to_string();
    assert!(err.contains("two text columns") && err.contains("Series"), "{err}");
}

#[test]
fn c3_a_treemap_keeps_its_values_and_refuses_negatives() {
    let df = DataFrame::new(vec![
        Series::new("Product".into(), ["Term", "Term", "Savings", "Current"]).into(),
        Series::new("Branch".into(), ["Main", "Gulshan", "Main", "Main"]).into(),
        Series::new("Balance".into(), [400.0, 250.0, 300.0, 120.0]).into(),
    ])
    .unwrap();
    let s = ChartSpec { category: "Branch".into(), series: Some("Product".into()), value: Some("Balance".into()), kind: ChartKind::Treemap, ..Default::default() };
    let o = chart_pair(&compute(df.clone().lazy(), &s).unwrap(), &opts()).unwrap().light;
    let tree = o["series"][0]["data"].as_array().unwrap();
    let total: f64 = tree.iter().flat_map(|g| g["children"].as_array().unwrap().iter()).map(|c| c["value"].as_f64().unwrap()).sum();
    assert_eq!(total, 1070.0);
    assert_eq!(tree.len(), 3, "one group per product");
    let neg = DataFrame::new(vec![Series::new("P".into(), ["A", "B"]).into(), Series::new("V".into(), [5.0, -3.0]).into()]).unwrap();
    let s = ChartSpec { category: "P".into(), value: Some("V".into()), kind: ChartKind::Treemap, ..Default::default() };
    let err = chart_pair(&compute(neg.lazy(), &s).unwrap(), &opts()).unwrap_err().to_string();
    assert!(err.contains("negative"), "{err}");
}

#[test]
fn c3_a_waterfall_has_an_invisible_base_and_the_c0_bases() {
    let o = chart_pair(&bridge(), &opts()).unwrap().light;
    let series = o["series"].as_array().unwrap();
    let base = series.iter().find(|s| s["name"] == "Base").unwrap();
    assert_eq!(base["itemStyle"]["color"], "transparent");
    let bases: Vec<f64> = base["data"].as_array().unwrap().iter().map(|d| d["value"].as_f64().unwrap()).collect();
    assert_eq!(bases, vec![0.0, 80.0, 88.0, 93.0, 0.0]);
    let up = series.iter().find(|s| s["name"] == "Increase").unwrap();
    let ups: Vec<f64> = up["data"].as_array().unwrap().iter().map(|d| d["value"].as_f64().unwrap()).collect();
    assert_eq!(ups, vec![80.0, 8.0, 5.0, 0.5, 93.5]);
    // start and end are the totals' colours, a positive step blue
    let colours: Vec<String> = up["data"].as_array().unwrap().iter().map(|d| d["itemStyle"]["color"].as_str().unwrap().to_string()).collect();
    assert_eq!(colours, vec!["#52514e", "#2a78d6", "#2a78d6", "#2a78d6", "#0d366b"]);
    // a bridge that does not add up is refused in the HTML as in the app
    let df = DataFrame::new(vec![Series::new("Step".into(), ["a", "b", "c"]).into(), Series::new("Amount".into(), [10.0, 5.0, 99.0]).into()]).unwrap();
    let s = ChartSpec { category: "Step".into(), value: Some("Amount".into()), kind: ChartKind::Waterfall, order: SortOrder::List { name: "s".into(), values: ["a", "b", "c"].map(String::from).to_vec(), case_insensitive: true }, ..Default::default() };
    let err = chart_pair(&compute(df.lazy(), &s).unwrap(), &opts()).unwrap_err().to_string();
    assert!(err.contains("does not add up"), "{err}");
}

#[test]
fn c3_a_donut_of_six_parts_is_refused_with_the_message() {
    let df = DataFrame::new(vec![Series::new("P".into(), ["a", "b", "c", "d", "e", "f"]).into(), Series::new("V".into(), [6.0, 5.0, 4.0, 3.0, 2.0, 1.0]).into()]).unwrap();
    let s = ChartSpec { category: "P".into(), value: Some("V".into()), kind: ChartKind::Donut, ..Default::default() };
    let err = chart_pair(&compute(df.lazy(), &s).unwrap(), &opts()).unwrap_err().to_string();
    assert_eq!(err, "Use a bar chart: more than 5 parts are hard to compare as slices");
}

#[test]
fn c3_marks_follow_the_design_system() {
    let forms = forms();
    let get = |n: &str| forms.iter().find(|(k, _)| *k == n).unwrap().1.clone();
    // bars: at most 24 px, a 4 px rounded data-end, the series colours in slot order
    let o = chart_pair(&get("grouped"), &opts()).unwrap().light;
    let s0 = &o["series"][0];
    assert_eq!(s0["barMaxWidth"], 24.0);
    assert_eq!(s0["data"][0]["itemStyle"]["borderRadius"], serde_json::json!([4.0, 4.0, 0, 0]));
    let colours: Vec<&str> = o["series"].as_array().unwrap().iter().map(|s| s["itemStyle"]["color"].as_str().unwrap()).collect();
    assert_eq!(colours, ["#2a78d6", "#eb6834", "#1baf7a", "#eda100"], "the first four slots, Term deposits first by size");
    // lines 2 px, 8 px markers with a 2 px surface ring
    let o = chart_pair(&get("line"), &opts()).unwrap().light;
    let l0 = &o["series"][0];
    assert_eq!((l0["lineStyle"]["width"].as_f64(), l0["symbolSize"].as_f64(), l0["itemStyle"]["borderWidth"].as_f64()), (Some(2.0), Some(8.0), Some(2.0)));
    assert_eq!(l0["itemStyle"]["borderColor"], "#fcfcfb");
    // area 10% and stacked area 35%
    assert_eq!(chart_pair(&get("area"), &opts()).unwrap().light["series"][0]["areaStyle"]["opacity"], 0.1);
    assert_eq!(chart_pair(&get("stacked-area"), &opts()).unwrap().light["series"][0]["areaStyle"]["opacity"], 0.35);
    // grid lines: 1 px, solid; no border, and a horizontal-bar chart has its categories down the side
    let o = chart_pair(&get("column"), &opts()).unwrap().light;
    assert_eq!(o["yAxis"][0]["splitLine"]["lineStyle"], serde_json::json!({"color": "#e1e0d9", "width": 1, "type": "solid"}));
    let o = chart_pair(&get("bar"), &opts()).unwrap().light;
    assert_eq!(o["yAxis"][0]["type"], "category");
    assert_eq!(o["xAxis"][0]["type"], "value");
    // the reference line is 1.5 px, solid, in the status colour, with its label
    let ml = &o["series"][0]["markLine"]["data"][0];
    assert_eq!((ml["lineStyle"]["width"].as_f64(), ml["lineStyle"]["type"].as_str(), ml["lineStyle"]["color"].as_str(), ml["label"]["formatter"].as_str()), (Some(1.5), Some("solid"), Some("#d03b3b"), Some("Limit")));
    // the stacked column has its total on top only
    let o = chart_pair(&get("stacked"), &opts()).unwrap().light;
    let totals = o["series"].as_array().unwrap().iter().filter(|s| s["name"] == "Total").count();
    assert_eq!(totals, 1);
    // a heatmap cell has a 2 px corner and a surface border; text is white on the dark steps
    let o = chart_pair(&get("heatmap"), &opts()).unwrap().light;
    let cells = o["series"][0]["data"].as_array().unwrap();
    assert_eq!(cells.len(), 16, "4 rows × 4 columns, every pair");
    assert!(cells.iter().all(|c| c["itemStyle"]["borderRadius"] == 2.0));
    assert!(cells.iter().any(|c| c["label"]["color"] == "#ffffff") && cells.iter().any(|c| c["label"]["color"] == "#0b0b0b"));
    // linked panels share an axis pointer; small multiples have a grid per facet
    let o = chart_pair(&get("linked-panels"), &opts()).unwrap().light;
    assert_eq!(o["grid"].as_array().unwrap().len(), 2);
    assert_eq!(o["axisPointer"]["link"][0]["xAxisIndex"], "all");
    let o = chart_pair(&get("small-multiples"), &opts()).unwrap().light;
    assert_eq!(o["grid"].as_array().unwrap().len(), 4);
    // a donut never has more than five slices and the others fold: here four
    assert_eq!(chart_pair(&get("donut"), &opts()).unwrap().light["series"][0]["data"].as_array().unwrap().len(), 4);
}

#[test]
fn c3_the_theme_is_built_from_the_tokens() {
    let th = micro_parquet::charts::echarts::theme_json(false);
    let colors: Vec<&str> = th["color"].as_array().unwrap().iter().map(|c| c.as_str().unwrap()).collect();
    assert_eq!(colors, t::SERIES_LIGHT.iter().map(|c| t::hex_text(*c)).collect::<Vec<_>>());
    assert_eq!(th["bar"]["itemStyle"]["borderRadius"], serde_json::json!([4.0, 4.0, 0, 0]));
    assert_eq!(th["valueAxis"]["splitLine"]["lineStyle"]["type"], "solid");
    assert_eq!(th["tooltip"]["backgroundColor"], "#fcfcfb");
    let dark = micro_parquet::charts::echarts::theme_json(true);
    assert_eq!(dark["backgroundColor"], "#1a1a19");
    assert_eq!(dark["color"][0], "#3987e5");
    assert!(th["textStyle"]["fontFamily"].as_str().unwrap().starts_with("\"Source Sans 3\""));
}

#[test]
fn c3_advanced_json_is_checked_merged_and_refused_with_line_and_column() {
    assert_eq!(validate_advanced("  \n ").unwrap(), None);
    let err = validate_advanced("{\n  \"legend\": { \"show\": false },\n  \"grid\": [ ,]\n}").unwrap_err().to_string();
    assert!(err.contains("not valid JSON") && err.contains("line 3, column"), "{err}");
    let err = validate_advanced("{ \"a\": 1,, }").unwrap_err().to_string();
    assert!(err.contains("line 1, column 10"), "{err}");
    assert!(validate_advanced("[1, 2]").unwrap_err().to_string().contains("one JSON object"));
    // merged over the option: objects key by key, lists and numbers replaced
    let mut s = base(ChartKind::Bar);
    s.advanced_json = "{\"title\": [{\"text\": \"Mine\"}], \"legend\": {\"show\": false}, \"series\": [{\"barMaxWidth\": 12}], \"toolbox\": {\"feature\": {\"dataZoom\": {}}}}".to_string();
    let o = chart_pair(&data(&s), &opts()).unwrap();
    assert_eq!(o.light["legend"]["show"], false);
    assert_eq!(o.light["legend"]["top"], 58.0, "the rest of the legend is kept");
    assert_eq!(o.light["title"][0]["text"], "Mine", "a list is replaced");
    assert_eq!(o.light["series"].as_array().unwrap().len(), 1, "a list is replaced as a whole");
    assert!(o.light["toolbox"]["feature"]["saveAsImage"].is_object() && o.light["toolbox"]["feature"]["dataZoom"].is_object(), "an object is merged");
    assert_eq!(o.dark["legend"]["show"], false, "both themes");
    // invalid advanced text refuses the chart (and so the file)
    s.advanced_json = "{ \"legend\": ".to_string();
    let err = chart_html(&data(&s), &opts()).unwrap_err().to_string();
    assert!(err.contains("not valid JSON") && err.contains("line 1"), "{err}");
    // deep_merge on its own
    let mut a = serde_json::json!({"a": {"b": 1, "c": 2}, "d": [1]});
    deep_merge(&mut a, &serde_json::json!({"a": {"b": 9}, "d": [2, 3]}));
    assert_eq!(a, serde_json::json!({"a": {"b": 9, "c": 2}, "d": [2, 3]}));
    // the advanced options belong to the HTML only: the in-app data has no trace of them
    let d = data(&s);
    assert_eq!(d.spec.advanced_json, s.advanced_json);
}

#[test]
fn c3_a_chart_book_of_three_charts_has_three_containers_and_writes_nothing_when_one_fails() {
    let forms = forms();
    let pick = |n: &str| forms.iter().find(|(k, _)| *k == n).unwrap().1.clone();
    let (a, b, c) = (pick("column"), pick("line"), pick("heatmap"));
    let items = vec![BookItem { data: &a, opts: opts() }, BookItem { data: &b, opts: opts() }, BookItem { data: &c, opts: opts() }];
    let html = chart_book_html("ALCO charts, October", "31 October 2026", &items).unwrap();
    assert_eq!(html.matches("class=\"mp-chart\"").count(), 3);
    assert_eq!(html.matches("class=\"mp-card\"").count(), 3);
    assert!(html.contains("<h1>ALCO charts, October</h1>") && html.contains("31 October 2026"));
    assert!(html.contains("repeat(auto-fit,minmax(520px,1fr))"), "a responsive grid");
    let (before, after) = html.split_once(ECHARTS_JS).unwrap();
    assert!(!format!("{before}{after}").contains("http"), "offline");
    // one copy of the library for the whole book
    assert_eq!(html.matches(ECHARTS_JS).count(), 1);
    let dir = std::env::temp_dir().join(format!("mp-c3-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("book.html");
    write_chart_book(&path, "ALCO charts", "today", &items).unwrap();
    assert!(path.exists());
    // a refused chart (the donut with six parts) stops the book, and no file is written
    let df = DataFrame::new(vec![Series::new("P".into(), ["a", "b", "c", "d", "e", "f"]).into(), Series::new("V".into(), [6.0, 5.0, 4.0, 3.0, 2.0, 1.0]).into()]).unwrap();
    let bad = compute(df.lazy(), &ChartSpec { category: "P".into(), value: Some("V".into()), kind: ChartKind::Donut, ..Default::default() }).unwrap();
    let items = vec![BookItem { data: &a, opts: opts() }, BookItem { data: &bad, opts: opts() }];
    let path2 = dir.join("bad.html");
    let err = write_chart_book(&path2, "x", "y", &items).unwrap_err().to_string();
    assert!(err.contains("Chart 2 of the book") && err.contains("more than 5 parts"), "{err}");
    assert!(!path2.exists());
    assert!(chart_book_html("x", "y", &[]).unwrap_err().to_string().contains("no charts"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn c3_writing_and_previewing_make_local_files_and_plain_errors() {
    let d = data(&base(ChartKind::Bar));
    let dir = std::env::temp_dir().join(format!("mp-c3w-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("one.html");
    write_chart_html(&p, &d, &opts()).unwrap();
    let s = std::fs::read_to_string(&p).unwrap();
    assert!(s.starts_with("<!doctype html>") && s.contains("<title>"));
    // a folder that does not exist: a message a banker can act on
    let err = write_chart_html(&dir.join("no").join("x.html"), &d, &opts()).unwrap_err().to_string();
    assert!(err.starts_with("Could not write") && err.contains("Choose another folder"), "{err}");
    // the preview is a temporary file with the chart in it (nothing is opened by the test)
    let pv = write_preview(&d, &opts()).unwrap();
    assert!(pv.starts_with(std::env::temp_dir()) && pv.extension().unwrap() == "html");
    assert!(std::fs::read_to_string(&pv).unwrap().contains("mp-chart"));
    std::fs::remove_file(pv).ok();
    // the Sankey and the treemap, which only the HTML can draw, preview the same way
    let sk = compute(flows().lazy(), &flow_spec(ChartKind::Sankey, true)).unwrap();
    let pv = write_preview(&sk, &opts()).unwrap();
    assert!(std::fs::read_to_string(&pv).unwrap().contains("\"type\":\"sankey\""));
    std::fs::remove_file(pv).ok();
    assert_eq!(file_stem("Sum of Balance by Month: all 24 rows (BDT)"), "Sum of Balance by Month all 24 rows BDT");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn c3_delta_is_written_with_the_delta_sign_in_the_html() {
    let df = DataFrame::new(vec![Series::new("Scenario".into(), ["Parallel up", "Parallel down"]).into(), Series::new("dEVE".into(), [-10.5, 4.8]).into()]).unwrap();
    let s = ChartSpec { category: "Scenario".into(), value: Some("dEVE".into()), kind: ChartKind::Bar, horizontal: true, ..Default::default() };
    let d = compute(df.lazy(), &s).unwrap();
    let html = chart_html(&d, &opts()).unwrap();
    let (before, after) = html.split_once(ECHARTS_JS).unwrap();
    let page = format!("{before}{after}");
    assert!(page.contains("\u{394}EVE"));
    assert!(!page.contains("dEVE"), "no 'd' form anywhere: title, subtitle, tooltip, table");
    let out = chart_pair(&d, &opts()).unwrap();
    assert!(out.light["title"][0]["text"].as_str().unwrap().contains("\u{394}EVE"));
    assert!(out.light["series"][0]["data"][0]["tip"].as_str().unwrap().contains("\u{394}EVE"));
    assert!(out.table.contains("\u{394}EVE"));
}

/// With `MP_HTML_OUT=<folder>` set, writes every form's page there (the screenshots of the guide are made
/// from these by `tools/html_chart_png.js`); without it, only checks that every form makes a page.
#[test]
fn c3_every_form_makes_a_page_and_can_be_written_for_the_screenshots() {
    let out = std::env::var("MP_HTML_OUT").ok().map(PathBuf::from);
    if let Some(d) = &out {
        std::fs::create_dir_all(d).unwrap();
    }
    for (name, d) in forms() {
        let html = chart_html(&d, &opts()).unwrap_or_else(|e| panic!("{name}: {e:#}"));
        assert!(html.len() > ECHARTS_JS.len());
        if let Some(dir) = &out {
            std::fs::write(dir.join(format!("{name}.html")), html).unwrap();
        }
    }
}
