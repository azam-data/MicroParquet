//! R2: the HTML page. A headless browser (Playwright, run from here with node; not a dependency) opens the written file, picks a
//! filter, clicks a bar, drills, goes Back and expands a pivot row, and every number it shows is compared with `compute_item`.

use crate::common::{dir, parquet};
use crate::p9_m3::{nums, strs};
use micro_parquet::charts::spec::{MeasureSpec, WaterfallMode};
use micro_parquet::kpi::*;
use polars::prelude::*;
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec, DrillLevel};
use micro_parquet::dashboard::*;
use micro_parquet::models::*;
use micro_parquet::report_html::*;
use serde_json::Value;
use std::process::Command;

fn scale() -> (f64, usize, String) {
    (1.0, 2, "BDT".to_string())
}

fn filter(id: &str, label: &str, field: &str) -> PageFilter {
    PageFilter { id: id.into(), label: label.into(), tab: "t1".into(), field: field.into(), ..Default::default() }
}

fn bar(category: &str) -> ChartSpec {
    ChartSpec { category: category.into(), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 500, ..Default::default() }
}

fn item(id: &str, title: &str, src: &TabSource, source: fn(TabSource) -> ItemSource) -> ReportItem {
    ReportItem { id: id.into(), title: title.into(), source: source(src.clone()), data_tab: Some("t1".into()), scale: scale() }
}

fn wing_drilled(base: TabSource) -> ItemSource {
    let mut s = bar("Wing");
    s.set_drill(vec![DrillLevel::column("Wing"), DrillLevel::column("Product code")]);
    ItemSource::Chart { base, steps: vec![], spec: s }
}

fn by_product(base: TabSource) -> ItemSource {
    ItemSource::Chart { base, steps: vec![], spec: bar("Product code") }
}

fn nested(base: TabSource) -> ItemSource {
    ItemSource::Pivot { base, steps: vec![], spec: crate::p10_p9_baseline::nested_pivot() }
}

/// The numbers an item shows, row by row as the page lays them out (a chart's series, a table's columns), as the file holds them.
fn numbers(d: &ItemData) -> Vec<Option<f64>> {
    let round = |v: Option<f64>, div: f64, dec: usize| v.filter(|x| x.is_finite()).map(|x| format!("{:.*}", dec, x / div).parse::<f64>().expect("a number"));
    match d {
        ItemData::Chart(c) => c.values.iter().flatten().map(|v| round(*v, 1.0, 2)).collect(),
        ItemData::Table(t) => match &t.nested {
            Some(n) => n.values.get_columns().iter().flat_map(|s| (0..s.len()).map(|i| round(s.get(i).ok().and_then(|v| v.extract::<f64>()), 1.0, 2))).collect(),
            None => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn expect(plan: &ReportPlan, ii: usize, picks: &[(usize, &str)], drill: &[&str], cross: Option<(&str, &str)>) -> ItemData {
    let page = &plan.pages[0];
    let mut p: Vec<(PageFilter, Vec<String>)> = picks.iter().map(|(i, v)| (page.filters[*i].clone(), vec![v.to_string()])).collect();
    if let Some((src, cat)) = cross {
        let s = page.items.iter().find(|i| i.id == src).expect("source");
        let ItemSource::Chart { spec, .. } = &s.source else { panic!("a chart") };
        p.push((CrossPick::from_chart(spec, cat, src, Some("t1")).expect("pick").as_filter(), vec![cat.to_string()]));
    }
    let req = ItemRequest { item_id: "x".into(), source: page.items[ii].source.clone(), picks: p, drill: drill.iter().map(|s| s.to_string()).collect(), temp_dir: plan.temp_dir.clone() };
    compute_item(&req).expect("expected")
}

fn seen(v: &Value) -> Vec<Option<f64>> {
    v["v"].as_array().expect("values").iter().flat_map(|r| r.as_array().expect("row").iter().map(|x| x.as_f64()).collect::<Vec<_>>()).collect()
}

const SCRIPT: &str = r#"
const {chromium}=require('playwright');
(async()=>{
const b=await chromium.launch();const pg=await b.newPage({viewport:{width:1300,height:900}});
const bad=[],reqs=[];
pg.on('console',m=>{if(m.type()==='error')bad.push(m.text())});pg.on('pageerror',e=>bad.push(String(e)));
pg.on('request',r=>{const u=r.url();if(!u.startsWith('file:')&&!u.startsWith('data:')&&!u.startsWith('blob:'))reqs.push(u)});
await pg.goto('file://'+process.argv[2]);await pg.waitForFunction(()=>window.__mp&&__mp.view('i1'));
const PROD=process.argv[3];
const view=async()=>pg.evaluate(()=>{const o={};for(const id of ['i1','i2','i3']){const v=__mp.view(id);o[id]=v&&{path:v.path,crossed:v.crossed,c:v.a.c,s:v.a.s,v:v.a.v,r:v.a.r}}o.sub=[...document.querySelectorAll('[data-item] .mp-sub')].map(e=>e.textContent);return o});
const out={};
out.top=await view();
await pg.selectOption('select[data-filter="1"]',PROD);out.filtered=await view();
await pg.selectOption('select[data-filter="1"]','');
let pt=await pg.evaluate(()=>__mp.point('i1',0));await pg.mouse.click(pt.x,pt.y);
out.crossed=await view();out.chip=await pg.textContent('[data-chip=cross]');
await pg.click('[data-chip=cross]');out.unclicked=await view();
pt=await pg.evaluate(()=>__mp.point('i1',0));await pg.mouse.click(pt.x,pt.y,{button:'right'});
await pg.click('.mp-menu button:has-text("Drill down")');out.drilled=await view();
await pg.click('[data-item=i1] .mp-sub button:has-text("Back")');out.back=await view();
out.rows0=await pg.locator('[data-item=i3] tr[data-row]').count();
await pg.locator('[data-item=i3] .mp-tog').first().click();out.rows1=await pg.locator('[data-item=i3] tr[data-row]').count();
out.errors=bad.concat(await pg.evaluate(()=>__mp.errors));out.requests=reqs;
console.log(JSON.stringify(out));await b.close();
})().catch(e=>{console.error(String(e));process.exit(1)});
"#;

#[test]
fn r2_the_page_in_a_headless_browser_shows_the_numbers_of_compute_item_through_filter_click_drill_back_and_expand() {
    let have = Command::new("node").args(["-e", "require.resolve('playwright')"]).output().map(|o| o.status.success()).unwrap_or(false);
    if !have {
        eprintln!("R2 browser test skipped: node with playwright is not installed here");
        return;
    }
    let dd = dir("p10r2_browser");
    let p = crate::common::fixtures_dir().join("phase9_replay/data/Accounts.csv");
    let src = TabSource::SingleFile { path: p.clone(), is_parquet: false, config: micro_parquet::engine::auto_detect_smart_schema(&p).expect("a schema") };
    let f_wing = filter("f1", "Wing", "Wing");
    let f_prod = filter("f2", "Product", "Product code");
    let values: Vec<Vec<String>> = [&f_wing, &f_prod].iter().map(|f| field_values(&src, &[], f, &dd).expect("values").0).collect();
    let page = ReportPage {
        id: "p1".into(),
        title: "Overview".into(),
        filters: vec![f_wing, f_prod],
        values: values.clone(),
        items: vec![item("i1", "By wing", &src, wing_drilled), item("i2", "By product", &src, by_product), item("i3", "Nested", &src, nested)],
    };
    let plan = ReportPlan { title: "ALCO pack".into(), pages: vec![page], template_bytes: 0, temp_dir: dd.clone() };
    let answers = compute_answers(&plan).expect("answers");
    let json = answers_json(&plan, &answers);
    let looks = vec![PageLook { page_h: 625.0, rects: vec![[0.0, 0.0, 490.0, 300.0], [510.0, 0.0, 490.0, 300.0], [0.0, 320.0, 1000.0, 300.0]], background: None }];
    let html = render_page(&plan, &json, &looks).expect("page");
    // the estimate is within 10% of the file, and the file calls nothing outside itself
    let est = estimate_page(&plan, json.len(), &looks).total() as f64;
    assert!((est - html.len() as f64).abs() / html.len() as f64 <= 0.1, "{est} vs {}", html.len());
    let file = dd.join("report.html");
    std::fs::write(&file, &html).expect("write");
    let script = dd.join("run.js");
    std::fs::write(&script, SCRIPT).expect("script");
    let node_path = Command::new("node").args(["-e", "console.log(require.resolve('playwright').split('/node_modules/')[0]+'/node_modules')"]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    let prod = values[1][0].clone();
    let o = Command::new("node").env("NODE_PATH", node_path).arg(&script).arg(&file).arg(&prod).output().expect("node runs");
    assert!(o.status.success(), "browser run failed: {}", String::from_utf8_lossy(&o.stderr));
    let r: Value = serde_json::from_slice(&o.stdout).expect("the run's json");
    assert_eq!(r["errors"].as_array().map(|a| a.len()), Some(0), "console errors: {}", r["errors"]);
    assert_eq!(r["requests"].as_array().map(|a| a.len()), Some(0), "requests left the file: {}", r["requests"]);
    // every number on screen is the number of compute_item
    let check = |stage: &str, ii: usize, picks: &[(usize, &str)], drill: &[&str], cross: Option<(&str, &str)>| {
        let want = numbers(&expect(&plan, ii, picks, drill, cross));
        let id = format!("i{}", ii + 1);
        let got = seen(&r[stage][&id]);
        assert!(!want.is_empty(), "{stage} {id}: nothing expected");
        assert_eq!(got, want, "{stage} {id}");
    };
    for ii in 0..3 {
        check("top", ii, &[], &[], None);
        check("filtered", ii, &[(1, &prod)], &[], None);
        check("unclicked", ii, &[], &[], None);
        check("back", ii, &[], &[], None);
    }
    // the clicked bar: the first wing of the top answer
    let wing = r["top"]["i1"]["c"][0].as_str().expect("a wing").to_string();
    check("crossed", 0, &[], &[], None); // the clicked chart keeps all its marks
    check("crossed", 1, &[], &[], Some(("i1", &wing)));
    check("crossed", 2, &[], &[], Some(("i1", &wing)));
    assert_eq!(r["chip"].as_str().map(|s| s.trim().to_string()), Some(format!("Wing = {wing} \u{d7}")));
    assert!(r["crossed"]["sub"][0].as_str().unwrap_or("").contains(&format!("Selected: {wing}")), "{}", r["crossed"]["sub"]);
    // the drilled chart: the clicked wing's products, then Back
    check("drilled", 0, &[], &[&wing], None);
    assert_eq!(r["drilled"]["i1"]["path"], serde_json::json!([wing]));
    assert!(r["back"]["i1"]["path"].as_array().map(|a| a.is_empty()).unwrap_or(false));
    // the Nested pivot: closed at the top, one row more shown after a row is expanded
    let (a, b) = (r["rows0"].as_i64().unwrap_or(0), r["rows1"].as_i64().unwrap_or(0));
    assert!(a >= 1 && b > a, "rows {a} then {b}");
}

#[test]
fn r2_a_page_layout_that_does_not_match_the_dashboard_is_refused_in_plain_words() {
    let dd = dir("p10r2_refuse");
    let plan = ReportPlan { title: "t".into(), pages: vec![ReportPage { id: "p".into(), title: "P".into(), filters: vec![], values: vec![], items: vec![] }], template_bytes: 0, temp_dir: dd };
    let e = render_page(&plan, "{}", &[]).expect_err("no layout");
    assert!(e.to_string().contains("export again"), "{e}");
}

/// Opening 100, inflow 50, outflow -30 (BDT) and, per row, 2 accounts: the waterfall, a count and a two-measure chart read from it.
fn flow_book(d: &std::path::Path) -> TabSource {
    let df = DataFrame::new(vec![strs("Step", &["1 Opening", "2 Inflow", "3 Outflow"]), nums("Balance", &[100.0, 50.0, -30.0]), nums("Accounts", &[2.0, 2.0, 2.0])]).expect("data");
    parquet(d, "r2_flow", &df)
}

fn the_chart(id: &str, title: &str, src: &TabSource, spec: ChartSpec) -> ReportItem {
    ReportItem { id: id.into(), title: title.into(), source: ItemSource::Chart { base: src.clone(), steps: vec![], spec }, data_tab: Some("t1".into()), scale: (10.0, 2, "Ten".into()) }
}

fn one_page(items: Vec<ReportItem>, d: &std::path::Path) -> ReportPlan {
    ReportPlan { title: "Flows".into(), pages: vec![ReportPage { id: "p1".into(), title: "P".into(), filters: vec![], values: vec![], items }], template_bytes: 0, temp_dir: d.to_path_buf() }
}

fn grid(v: &Value) -> Vec<Vec<f64>> {
    v.as_array().expect("rows").iter().map(|r| r.as_array().expect("row").iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect()).collect()
}

fn answer_of(plan: &ReportPlan, item: usize) -> Value {
    let answers = compute_answers(plan).expect("answers");
    let all: Value = serde_json::from_str(&answers_json(plan, &answers)).expect("json");
    all["pages"][0]["items"][item]["answers"][0].clone()
}

#[test]
fn r2_a_count_stays_a_count_a_two_measure_chart_keeps_both_measures_and_a_waterfall_carries_its_steps() {
    let dd = dir("p10r2_kinds");
    let src = flow_book(&dd);
    let mut count = bar("Step");
    count.agg = ChartAgg::Count;
    count.value = None;
    let mut two = bar("Step");
    two.measures = vec![
        MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, ..Default::default() },
        MeasureSpec { column: Some("Accounts".into()), agg: ChartAgg::Count, ..Default::default() },
    ];
    let mut wf = bar("Step");
    wf.kind = ChartKind::Waterfall;
    wf.waterfall = WaterfallMode::StartSteps;
    let plan = one_page(vec![the_chart("c", "Count", &src, count), the_chart("m", "Two", &src, two), the_chart("w", "Bridge", &src, wf)], &dd);
    // a count is a whole number, never divided by the money unit (3 rows of 1 = 1 each, not 0.10)
    let c = answer_of(&plan, 0);
    assert_eq!(grid(&c["v"]), vec![vec![1.0, 1.0, 1.0]], "{c}");
    assert_eq!(c["mu"], serde_json::json!([""]));
    // two measures: the money one in the item's unit (100 / 10), the other as it is; one row per measure, and a unit each
    let m = answer_of(&plan, 1);
    assert_eq!(m["m"].as_array().map(|a| a.len()), Some(2), "{m}");
    assert_eq!(grid(&m["v"]), vec![vec![10.0, 5.0, -3.0], vec![1.0, 1.0, 1.0]], "{m}");
    assert_eq!(m["mu"], serde_json::json!(["Ten", ""]));
    // the waterfall: start 100, +50, -30, in the unit (÷ 10), with the running level and the bar's edges
    let w = answer_of(&plan, 2);
    let steps = w["wf"].as_array().expect("steps");
    assert_eq!(steps.len(), 4, "start, two steps and the calculated end: {w}");
    let row = |i: usize| (steps[i][0].as_str().unwrap_or("").to_string(), steps[i][1].as_str().unwrap_or("").to_string(), (2..6).map(|k| steps[i][k].as_f64().unwrap_or(f64::NAN)).collect::<Vec<_>>());
    assert_eq!(row(0), ("1 Opening".into(), "s".into(), vec![10.0, 0.0, 10.0, 10.0]));
    assert_eq!(row(1), ("2 Inflow".into(), "d".into(), vec![5.0, 10.0, 15.0, 15.0]));
    assert_eq!(row(2), ("3 Outflow".into(), "d".into(), vec![-3.0, 12.0, 15.0, 12.0]));
    let end = row(3);
    assert_eq!((end.1.as_str(), end.2[0], end.2[3]), ("e", 12.0, 12.0), "the end total is 100 + 50 - 30 = 120, ÷ 10");
    assert_eq!(w["x"], Value::Null);
}

#[test]
fn r2_a_kpi_card_carries_its_threshold_colour_arrow_words_and_unit_exactly_as_the_app_says_them() {
    let dd = dir("p10r2_kpi");
    let src = flow_book(&dd);
    let spec = KpiSpec {
        title: "Net".into(),
        unit: KpiUnit::Crore,
        compare: KpiCompare::PriorPeriod,
        good_direction: GoodDirection::Up,
        thresholds: vec![Threshold { op: ThresholdOp::Below, limit: 500.0, colour: ThresholdColour::Red }],
        sparkline: true,
        ..Default::default()
    };
    let value = KpiValue { current: Some(4_000_000_000.0), earlier: Some(4_200_000_000.0), period: "Oct 2026".into(), spark: vec![Some(4.1e9), None, Some(4.0e9)], spark_labels: vec![], ..Default::default() };
    let want = view(&spec, &value, KpiUnit::Crore);
    assert_eq!(want.threshold, Some(ThresholdColour::Red), "400 Cr is below 500");
    let item = ReportItem { id: "k".into(), title: "Net".into(), source: ItemSource::Kpi { spec, tab_key: "t1".into(), base: src, steps: vec![] }, data_tab: Some("t1".into()), scale: scale() };
    let plan = one_page(vec![item], &dd);
    let answers = vec![PageAnswers {
        page_id: "p1".into(),
        items: vec![ItemAnswers { item_id: "k".into(), filters: vec![], answers: vec![Answer { picks: vec![], path: vec![], data: ItemData::Kpi(Box::new(value)) }], numbers: 4, linked: Err("a card is one number".into()), cross: vec![] }],
    }];
    let all: Value = serde_json::from_str(&answers_json(&plan, &answers)).expect("json");
    let k = &all["pages"][0]["items"][0]["answers"][0]["k"];
    assert_eq!(k["val"], "400.00 Cr");
    assert_eq!(k["thr"], "red");
    assert_eq!(k["arrow"], "\u{25bc}");
    assert_eq!(k["words"], "down");
    assert_eq!(k["tone"], "bad");
    assert_eq!(k["chg"].as_str(), Some(want.change_text.as_str()));
    assert_eq!(k["spark"].as_array().map(|a| a.len()), Some(3));
    assert_eq!(k["spark"][1], Value::Null, "a gap stays a gap");
    // and the page shows it: red number, the arrow and the words
    let looks = vec![PageLook { page_h: 625.0, rects: vec![[0.0, 0.0, 400.0, 300.0]], background: None }];
    let html = render_page(&plan, &answers_json(&plan, &answers), &looks).expect("page");
    let file = dd.join("kpi.html");
    std::fs::write(&file, &html).expect("write");
    let script = dd.join("kpi.js");
    std::fs::write(&script, KPI_SCRIPT).expect("script");
    let Some(node_path) = playwright_path() else {
        eprintln!("R2 KPI browser check skipped: node with playwright is not installed here");
        return;
    };
    let o = Command::new("node").env("NODE_PATH", node_path).arg(&script).arg(&file).output().expect("node");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r: Value = serde_json::from_slice(&o.stdout).expect("json");
    assert_eq!(r["errors"].as_array().map(|a| a.len()), Some(0), "{}", r["errors"]);
    assert_eq!(r["value"], "400.00 Cr");
    assert_eq!(r["valueColour"], "rgb(208, 59, 59)", "the critical colour of the house palette");
    assert!(r["change"].as_str().unwrap_or("").starts_with('\u{25bc}'), "{}", r["change"]);
    assert_eq!(r["changeColour"], "rgb(208, 59, 59)", "down is bad when up is good");
}

fn playwright_path() -> Option<String> {
    let o = Command::new("node").args(["-e", "console.log(require.resolve('playwright').split('/node_modules/')[0]+'/node_modules')"]).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

const KPI_SCRIPT: &str = r#"
const {chromium}=require('playwright');
(async()=>{const b=await chromium.launch();const pg=await b.newPage();const bad=[];
pg.on('console',m=>{if(m.type()==='error')bad.push(m.text())});pg.on('pageerror',e=>bad.push(String(e)));
await pg.goto('file://'+process.argv[2]);await pg.waitForSelector('[data-kpi=value]');
const o=await pg.evaluate(()=>({value:document.querySelector('[data-kpi=value]').textContent,valueColour:getComputedStyle(document.querySelector('[data-kpi=value]')).color,change:document.querySelector('[data-kpi=change]').textContent,changeColour:getComputedStyle(document.querySelector('[data-kpi=change]')).color}));
o.errors=bad;console.log(JSON.stringify(o));await b.close()})().catch(e=>{console.error(String(e));process.exit(1)});
"#;

#[test]
fn r2_the_page_draws_a_waterfall_and_a_two_axis_chart_without_errors() {
    let Some(node_path) = playwright_path() else {
        eprintln!("R2 chart browser check skipped: node with playwright is not installed here");
        return;
    };
    let dd = dir("p10r2_draw");
    let src = flow_book(&dd);
    let mut two = bar("Step");
    two.measures = vec![
        MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, ..Default::default() },
        MeasureSpec { column: Some("Accounts".into()), agg: ChartAgg::Count, ..Default::default() },
    ];
    let mut wf = bar("Step");
    wf.kind = ChartKind::Waterfall;
    wf.waterfall = WaterfallMode::StartSteps;
    let plan = one_page(vec![the_chart("m", "Two", &src, two), the_chart("w", "Bridge", &src, wf)], &dd);
    let answers = compute_answers(&plan).expect("answers");
    let looks = vec![PageLook { page_h: 625.0, rects: vec![[0.0, 0.0, 490.0, 300.0], [510.0, 0.0, 490.0, 300.0]], background: None }];
    let html = render_page(&plan, &answers_json(&plan, &answers), &looks).expect("page");
    let file = dd.join("draw.html");
    std::fs::write(&file, &html).expect("write");
    let script = dd.join("draw.js");
    std::fs::write(&script, DRAW_SCRIPT).expect("script");
    let o = Command::new("node").env("NODE_PATH", node_path).arg(&script).arg(&file).output().expect("node");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r: Value = serde_json::from_slice(&o.stdout).expect("json");
    assert_eq!(r["errors"].as_array().map(|a| a.len()), Some(0), "{}", r["errors"]);
    // two measures: a bar and a second axis; the bridge: one custom series of 4 bars
    assert_eq!(r["two"]["series"], 2, "{r}");
    assert_eq!(r["two"]["axes"], 2, "{r}");
    assert_eq!(r["bridge"]["series"], 1, "{r}");
    assert_eq!(r["bridge"]["points"], 4, "{r}");
}

const DRAW_SCRIPT: &str = r#"
const {chromium}=require('playwright');
(async()=>{const b=await chromium.launch();const pg=await b.newPage({viewport:{width:1300,height:900}});const bad=[];
pg.on('console',m=>{if(m.type()==='error')bad.push(m.text())});pg.on('pageerror',e=>bad.push(String(e)));
await pg.goto('file://'+process.argv[2]);await pg.waitForSelector('[data-item=w] canvas');
const o=await pg.evaluate(()=>{const g=id=>{const d=document.querySelector('[data-item='+id+'] .mp-chart');const op=echarts.getInstanceByDom(d).getOption();return {series:op.series.length,axes:op.yAxis.length,points:op.series[0].data.length}};return {two:g('m'),bridge:g('w')}});
o.errors=bad;console.log(JSON.stringify(o));await b.close()})().catch(e=>{console.error(String(e));process.exit(1)});
"#;
