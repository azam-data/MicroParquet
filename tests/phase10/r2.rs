//! R2: the HTML page. A headless browser (Playwright, run from here with node; not a dependency) opens the written file, picks a
//! filter, clicks a bar, drills, goes Back and expands a pivot row, and every number it shows is compared with `compute_item`.

use crate::common::dir;
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
