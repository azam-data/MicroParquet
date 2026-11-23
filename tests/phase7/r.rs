//! Review fixes (Phase 7, Session E): the correctness items of the owner's build check and the architect's
//! review. The UI-side checks (chart book, chart-data tab, colour map by real clicks) are in
//! `src/ui/render_tests_r.rs`.

use crate::p7_c1::lay;
use crate::p7_c3::forms;
use crate::p7_c4::{inspect, tmp};
use crate::p7_c5::{all_cases, chart_of};
use micro_parquet::chart::{ChartAgg, ChartKind, ChartSpec};
use micro_parquet::charts::calc::compute;
use micro_parquet::charts::colours::Colour;
use micro_parquet::charts::data::{waterfall_from_data, ChartData};
use micro_parquet::charts::echarts::{chart_html, chart_pair};
use micro_parquet::charts::layout::{layout_chart, subtitle_text, ApproxMeasure, LayoutOpts, Role};
use micro_parquet::charts::python::chart_to_python;
use micro_parquet::charts::samples as sm;
use micro_parquet::charts::spec::{ChartScale, Stack};
use micro_parquet::charts::templates::{prepare, Ctx, Template};
use micro_parquet::charts::tokens as t;
use micro_parquet::charts::xlsx::write_chart_xlsx;
use polars::prelude::*;

fn crore() -> LayoutOpts {
    LayoutOpts { dark: false, divisor: 1.0, decimals: 2, unit: "BDT Crore".into(), bare: false, colours: None }
}

fn ctx() -> Ctx {
    Ctx { divisor: 1.0, suffix: "Cr".into(), dq_green: Some(98.0), dq_amber: Some(95.0), ..Default::default() }
}

fn python() -> bool {
    std::process::Command::new("python3").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// The subtitle as each of the four renderers writes it.
fn subtitles(d: &ChartData, o: &LayoutOpts, what: &str) -> [String; 4] {
    let app = lay_with(d, o)
        .texts
        .iter()
        .find(|x| x.role == Role::Subtitle)
        .map(|x| x.text.clone())
        .unwrap_or_default();
    let html = chart_pair(d, o).unwrap_or_else(|e| panic!("{what}: {e:#}")).light.to_string();
    let py = chart_to_python(d, o).unwrap_or_else(|e| panic!("{what}: {e:#}"));
    let xl = if python() {
        let p = tmp(&format!("r4-{}.xlsx", what.replace([' ', '/', ':'], "_")));
        write_chart_xlsx(&p, d, o).unwrap_or_else(|e| panic!("{what}: {e:#}"));
        inspect(&p)["strings"].to_string()
    } else {
        String::new()
    };
    [app, html, py, xl]
}

fn lay_with(d: &ChartData, o: &LayoutOpts) -> micro_parquet::charts::layout::ChartLayout {
    layout_chart(d, egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(1400.0, 700.0)), o, &ApproxMeasure)
}

// ------------------------------------------------------------------------------------------ 4. subtitle

#[test]
fn r4_the_subtitle_says_what_was_done_to_the_numbers_and_never_a_money_unit_beside_a_percentage() {
    let all = forms();
    let get = |n: &str| all.iter().find(|(k, _)| *k == n).map(|(_, d)| d.clone()).unwrap_or_else(|| panic!("form {n}"));
    // a heatmap of "% of row" (the owner's sss-02) and a 100% stacked column
    for (name, word) in [("heatmap", "% of row"), ("percent", "% of column")] {
        let d = get(name);
        for (i, s) in subtitles(&d, &crore(), name).iter().enumerate() {
            if i == 3 && !python() {
                continue;
            }
            let renderer = ["app", "html", "python", "excel"][i];
            assert!(s.contains(word), "{name} ({renderer}): the subtitle names the transform '{word}': {s}");
            assert!(!s.contains("Crore"), "{name} ({renderer}): no money unit on a percentage view: {s}");
        }
    }
    // a % of total share and a plain amount chart: the first has no unit, the second keeps it
    let mut spec = get("column").spec.clone();
    spec.measures = vec![micro_parquet::charts::spec::MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, transform: micro_parquet::charts::spec::Transform::ShareOfTotal, ..Default::default() }];
    let share = compute(crate::p7_c3::frame().lazy(), &spec).expect("share chart");
    let s = subtitle_text(&share, &crore());
    assert!(s.contains("% of total") && !s.contains("Crore"), "{s}");
    let plain = get("column");
    let s = subtitle_text(&plain, &crore());
    assert!(s.ends_with("BDT Crore") && !s.contains("% of"), "an amount chart keeps its unit: {s}");
    // a typed subtitle is never changed on a chart of the user's own
    let mut typed = plain.clone();
    typed.spec.subtitle = "My own words".into();
    assert_eq!(subtitle_text(&typed, &crore()), "My own words");
}

// -------------------------------------------------------------------------------- 5. Save as SVG in the page

fn node_with_playwright() -> bool {
    std::process::Command::new("node").arg("-e").arg("require('playwright')").output().map(|o| o.status.success()).unwrap_or(false)
}

#[test]
fn r5_the_save_as_svg_button_downloads_an_svg_and_save_as_png_still_downloads_a_png() {
    if !node_with_playwright() {
        eprintln!("r5: node with playwright is not installed here: the browser click is not tested");
        return;
    }
    let all = forms();
    for name in ["column", "waterfall", "heatmap"] {
        let d = all.iter().find(|(k, _)| *k == name).map(|(_, d)| d.clone()).expect("form");
        let page = tmp(&format!("r5-{name}.html"));
        std::fs::write(&page, chart_html(&d, &crate::p7_c3::opts()).expect("page")).expect("write");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let o = std::process::Command::new("node").arg(root.join("tools/html_svg_click.js")).arg(&page).output().expect("node runs");
        assert!(o.status.success(), "{name}: {}", String::from_utf8_lossy(&o.stderr));
        let j: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("{name}: {e}: {}", String::from_utf8_lossy(&o.stdout)));
        let svg = &j["svg"];
        assert!(svg["error"].is_null(), "{name}: Save as SVG did nothing: {svg}");
        assert!(svg["name"].as_str().unwrap_or("").ends_with(".svg"), "{name}: {svg}");
        assert!(svg["text"].as_str().unwrap_or("").starts_with("<svg"), "{name}: the download is an SVG");
        assert!(svg["errors"].as_array().map(|a| a.is_empty()).unwrap_or(false), "{name}: no script error: {svg}");
        let png = &j["png"];
        assert!(png["error"].is_null(), "{name}: Save as PNG: {png}");
        assert!(png["name"].as_str().unwrap_or("").ends_with(".png") && png["start"].as_str().unwrap_or("").contains("PNG"), "{name}: {png}");
    }
}

// ----------------------------------------------------------------------------------------- 6. NII new / closed

/// prev {TD 100 @ 8, SB 50 @ 4}, curr {TD 120 @ 8.5, SB 50 @ 4, FX 30 @ 5}: interest 10.00 to 13.70.
fn mom_new_key() -> DataFrame {
    let f = |p: &[&str], b: &[f64], r: &[f64]| DataFrame::new(vec![Series::new("Product".into(), p), Series::new("Balance".into(), b), Series::new("Rate".into(), r)]).unwrap();
    let prev = f(&["TD", "SB"], &[100.0, 50.0], &[8.0, 4.0]);
    let curr = f(&["TD", "SB", "FX"], &[120.0, 50.0, 30.0], &[8.5, 4.0, 5.0]);
    let spec = micro_parquet::mom::MomSpec { key_cols: vec!["Product".into()], bal_col: "Balance".into(), rate_col: Some("Rate".into()), ..Default::default() };
    micro_parquet::mom::build_mom(prev.lazy(), curr.lazy(), &spec).unwrap_or_else(|e| panic!("{e:#}"))
}

#[test]
fn r6_a_new_key_is_its_own_step_in_the_nii_bridge_and_its_own_series_by_key_in_all_four_renderers() {
    let df = mom_new_key();
    let p = prepare(Template::NiiBridge, &df, &ctx()).unwrap_or_else(|e| panic!("{e:#}"));
    let table = p.table.clone().expect("the bridge table");
    let steps: Vec<String> = table.column("Step").unwrap().str().unwrap().into_iter().map(|s| s.unwrap().to_string()).collect();
    assert_eq!(steps, ["Prior interest", "Volume", "Rate", "Mix", "New / closed", "Current interest"], "New / closed sits between Mix and Current interest");
    let amounts: Vec<f64> = table.column("Interest (BDT)").unwrap().f64().unwrap().into_iter().map(|v| v.unwrap()).collect();
    for (a, w) in amounts.iter().zip([10.0, 1.6, 0.5, 0.1, 1.5, 13.7]) {
        assert!((a - w).abs() < 1e-9, "{amounts:?}");
    }
    let d = compute(table.lazy(), &p.spec).unwrap_or_else(|e| panic!("{e:#}"));
    let steps = waterfall_from_data(&d).expect("waterfall steps");
    assert_eq!(steps.len(), 6);
    assert!((steps[0].value - 10.0).abs() < 1e-9 && (steps[5].value - 13.7).abs() < 1e-9);
    // drawn: the start and the end are labelled in the app, the page, the workbook and the script
    let o = LayoutOpts { unit: String::new(), ..crore() };
    let l = lay_with(&d, &o);
    for want in ["10.00", "13.70", "+1.50"] {
        assert!(l.texts.iter().any(|x| x.text == want), "app: label {want}: {:?}", l.texts.iter().map(|x| &x.text).collect::<Vec<_>>());
    }
    let pair = chart_pair(&d, &o).expect("html");
    assert!(pair.table.contains("13.70") && pair.table.contains("New / closed"), "html table");
    assert!(pair.light.to_string().contains("New / closed"), "html chart");
    let py = chart_to_python(&d, &o).expect("python");
    assert!(py.contains("New / closed") && py.contains("13.7"), "python");
    if python() {
        let path = tmp("r6.xlsx");
        write_chart_xlsx(&path, &d, &o).expect("xlsx");
        let j = inspect(&path);
        let strings = j["strings"].to_string();
        assert!(strings.contains("New / closed"), "excel: {strings}");
        let nums: Vec<f64> = j["cells"].as_object().unwrap().values().filter_map(|v| v.as_f64()).collect();
        assert!(nums.iter().any(|v| (v - 13.7).abs() < 1e-9) && nums.iter().any(|v| (v - 1.5).abs() < 1e-9), "excel cells hold 13.7 and 1.5");
    }
    // by key: FX has no effects, so its change is the fourth series, never three zeros
    let c = p.companion.expect("effects by key");
    let tb = c.table.clone().expect("table");
    let key: Vec<String> = tb.column("Product").unwrap().str().unwrap().into_iter().map(|s| s.unwrap().to_string()).collect();
    let eff: Vec<String> = tb.column("Effect").unwrap().str().unwrap().into_iter().map(|s| s.unwrap().to_string()).collect();
    let val: Vec<f64> = tb.column("Change in interest (BDT)").unwrap().f64().unwrap().into_iter().map(|v| v.unwrap()).collect();
    let fx: Vec<(&String, f64)> = key.iter().zip(&eff).zip(&val).filter(|((k, _), _)| *k == "FX").map(|((_, e), v)| (e, *v)).collect();
    assert_eq!(fx.len(), 1, "one row for FX: {fx:?}");
    assert_eq!((fx[0].0.as_str(), (fx[0].1 - 1.5).abs() < 1e-9), ("New / closed", true));
    assert!(c.spec.colour_map.get("New / closed").is_some(), "the series has a colour");
    let d2 = compute(tb.lazy(), &c.spec).expect("by key");
    assert!(d2.series.iter().any(|s| s == "New / closed"));
    // a key with only some of its effects blank is not guessed at
    let mut bad = df.clone();
    let td = bad.column("Product").unwrap().str().unwrap().into_iter().position(|k| k == Some("TD")).expect("TD row");
    let vol = bad.column("Volume effect").unwrap().f64().unwrap().into_iter().enumerate().map(|(i, v)| if i == td { None } else { v }).collect::<Vec<Option<f64>>>();
    bad.with_column(Series::new("Volume effect".into(), vol)).unwrap();
    let e = prepare(Template::NiiBridge, &bad, &ctx()).err().unwrap_or_else(|| panic!("refused")).to_string();
    assert!(e.contains("some of its volume, rate and mix effects blank"), "{e}");
}

// ---------------------------------------------------------------------------------- 7. months are dates

fn classified_text(months: [&str; 3], npl: [f64; 3]) -> DataFrame {
    let (mut m, mut g, mut b) = (Vec::new(), Vec::new(), Vec::new());
    for (i, month) in months.iter().enumerate() {
        // STD 90 and SS, so that NPL / total is npl[i] percent of 100
        for (gr, v) in [("STD", 100.0 - 10.0 - npl[i]), ("SMA", 10.0), ("SS", npl[i])] {
            m.push(*month);
            g.push(gr);
            b.push(v);
        }
    }
    DataFrame::new(vec![Series::new("Month".into(), m), Series::new("Grade".into(), g), Series::new("Balance".into(), b)]).unwrap()
}

#[test]
fn r7_months_are_sorted_as_dates_and_a_month_without_std_or_a_date_is_refused() {
    // 30/04 sorts before 31/01 as text: by date the last month is April with NPL 15
    let df = classified_text(["31/01/2026", "31/03/2026", "30/04/2026"], [5.0, 10.0, 15.0]);
    let p = prepare(Template::ClassifiedTrend, &df, &ctx()).unwrap_or_else(|e| panic!("{e:#}"));
    assert_eq!(p.finding, "NPL ratio rose to 15.0%");
    let months: Vec<String> = p.table.unwrap().column("Month").unwrap().str().unwrap().into_iter().flatten().map(str::to_string).collect();
    let mut seen: Vec<String> = Vec::new();
    for m in months {
        if !seen.contains(&m) {
            seen.push(m);
        }
    }
    assert_eq!(seen, ["Jan 2026", "Mar 2026", "Apr 2026"]);
    // Y-M-D text works too
    let df = classified_text(["2026-01-31", "2026-03-31", "2026-04-30"], [15.0, 10.0, 5.0]);
    assert_eq!(prepare(Template::ClassifiedTrend, &df, &ctx()).unwrap().finding, "NPL ratio fell to 5.0%");
    // anything that is not a date is refused, naming the value
    let bad = classified_text(["Jan-26", "Mar-26", "Apr-26"], [5.0, 10.0, 15.0]);
    let e = prepare(Template::ClassifiedTrend, &bad, &ctx()).err().expect("refused").to_string();
    assert!(e.contains("'Jan-26'") && e.contains("day/month/year"), "{e}");
    // a month with no STD row would overstate the ratio
    let no_std = classified_text(["31/01/2026", "31/03/2026", "30/04/2026"], [5.0, 10.0, 15.0]);
    let keep: Vec<bool> = no_std.column("Month").unwrap().str().unwrap().into_iter().zip(no_std.column("Grade").unwrap().str().unwrap().into_iter()).map(|(m, g)| !(m == Some("31/03/2026") && g == Some("STD"))).collect();
    let no_std = no_std.filter(&BooleanChunked::from_slice("k".into(), &keep)).unwrap();
    let e = prepare(Template::ClassifiedTrend, &no_std, &ctx()).err().expect("refused").to_string();
    assert!(e.contains("31 Mar 2026") && e.contains("no STD row"), "{e}");
    // the deposit mix reads the months the same way: by text April would come first and the finding would flip
    let (mut m, mut pr, mut b) = (Vec::new(), Vec::new(), Vec::new());
    for (month, td, sb) in [("31/01/2026", 100.0, 100.0), ("31/03/2026", 120.0, 90.0), ("30/04/2026", 110.0, 150.0)] {
        for (p, v) in [("Term deposit", td), ("Savings", sb)] {
            m.push(month);
            pr.push(p);
            b.push(v);
        }
    }
    let mix = DataFrame::new(vec![Series::new("Month".into(), m), Series::new("Product".into(), pr), Series::new("Balance".into(), b)]).unwrap();
    let p = prepare(Template::DepositMix, &mix, &ctx()).unwrap();
    assert_eq!(p.finding, "Savings drove growth");
    let d = compute(p.table.clone().expect("the mix has its own table").lazy(), &p.spec).unwrap();
    assert_eq!(d.categories, ["Jan 2026", "Mar 2026", "Apr 2026"], "the columns follow the dates and read Jan 2026");
    let bad_mix = mix.clone().lazy().with_column(lit("soon").alias("Month")).collect().unwrap();
    assert!(prepare(Template::DepositMix, &bad_mix, &ctx()).is_err());
}

// ---------------------------------------------------------------------------------- 8. non-finite numbers

#[test]
fn r8_a_value_that_is_not_a_number_stops_the_chart_naming_the_column_and_the_category() {
    let df = DataFrame::new(vec![
        Series::new("Cat".into(), ["A", "A", "B", "B"]),
        Series::new("Val".into(), [Some(f64::NAN), None, Some(5.0), Some(f64::INFINITY)]),
    ])
    .unwrap();
    let spec = ChartSpec { category: "Cat".into(), value: Some("Val".into()), agg: ChartAgg::Sum, ..Default::default() };
    let e = compute(df.clone().lazy(), &spec).err().expect("stopped").to_string();
    assert!(e.contains("'Val'") && e.contains("'A'") && e.contains("NaN"), "{e}");
    let only_inf = df.clone().lazy().filter(col("Cat").eq(lit("B"))).collect().unwrap();
    let e = compute(only_inf.lazy(), &spec).err().expect("stopped").to_string();
    assert!(e.contains("'Val'") && e.contains("'B'") && e.contains("infinity"), "{e}");
    // a scatter's points
    let pts = DataFrame::new(vec![Series::new("X".into(), [1.0, f64::NAN, 3.0]), Series::new("Y".into(), [1.0, 2.0, 3.0])]).unwrap();
    let sc = ChartSpec { category: "X".into(), value: Some("Y".into()), kind: ChartKind::Scatter, ..Default::default() };
    let e = compute(pts.lazy(), &sc).err().expect("stopped").to_string();
    assert!(e.contains("'X'") && e.contains("row 2"), "{e}");
    // a chart table that was made some other way is refused by every renderer, and no workbook holds inf
    let mut d = compute(df.lazy().filter(col("Cat").eq(lit("B"))).with_column(lit(1.0).alias("Val")).collect().unwrap().lazy(), &spec).unwrap();
    d.values[0][0] = Some(f64::INFINITY);
    assert!(d.non_finite().is_some());
    assert!(chart_pair(&d, &crore()).is_err() && chart_to_python(&d, &crore()).is_err());
    let msg = lay_with(&d, &crore()).message.unwrap_or_default();
    assert!(msg.contains("not a finite number"), "the app says so: {msg}");
    let p = tmp("r8.xlsx");
    std::fs::remove_file(&p).ok();
    assert!(write_chart_xlsx(&p, &d, &crore()).is_err());
    if let Ok(bytes) = std::fs::read(&p) {
        assert!(!String::from_utf8_lossy(&bytes).contains("<v>inf</v>"));
    }
}

// ----------------------------------------------------------------------------- 9. LCR / NSFR from the rows

fn lcr_with(f: impl Fn(&mut Vec<String>, &mut Vec<Option<f64>>, &mut Vec<String>)) -> DataFrame {
    let base = sm::lcr_summary();
    let mut item: Vec<String> = base.column("Item").unwrap().str().unwrap().into_iter().map(|s| s.unwrap().to_string()).collect();
    let mut val: Vec<Option<f64>> = base.column("Value").unwrap().f64().unwrap().into_iter().collect();
    let mut note: Vec<String> = base.column("Note").unwrap().str().unwrap().into_iter().map(|s| s.unwrap().to_string()).collect();
    f(&mut item, &mut val, &mut note);
    let unit = vec![""; item.len()];
    DataFrame::new(vec![Series::new("Item".into(), item), Series::new("Value".into(), val), Series::new("Unit".into(), unit), Series::new("Note".into(), note)]).unwrap()
}

#[test]
fn r9_the_lcr_minimum_and_the_breach_come_from_the_results_rows_never_from_a_default() {
    let tick = |p: &micro_parquet::charts::templates::Prepared, cat: &str| p.spec.limit_ticks.iter().find(|x| x.category == cat).cloned().expect("tick");
    // the minimum is the row's: 110, not 100
    let df = lcr_with(|item, val, _| {
        let i = item.iter().position(|s| s == "LCR minimum").unwrap();
        val[i] = Some(110.0);
    });
    let p = prepare(Template::LcrNsfr, &df, &ctx()).unwrap();
    assert_eq!(tick(&p, "LCR").value, 110.0);
    assert!(p.finding.contains("110% minimum"), "{}", p.finding);
    // a ratio under a minimum, with the tool saying "Below the minimum": a breach
    let df = lcr_with(|item, val, note| {
        let i = item.iter().position(|s| s == "LCR").unwrap();
        val[i] = Some(90.0);
        let s = item.iter().position(|s| s == "LCR status").unwrap();
        note[s] = "Below the minimum".into();
    });
    let p = prepare(Template::LcrNsfr, &df, &ctx()).unwrap();
    assert!(tick(&p, "LCR").breach.starts_with("Breach"), "{:?}", tick(&p, "LCR"));
    assert!(tick(&p, "NSFR").breach.is_empty());
    // the tool's answer decides (a ratio equal to a "greater than" minimum is below it): no arithmetic of ours
    let df = lcr_with(|item, val, note| {
        let i = item.iter().position(|s| s == "NSFR").unwrap();
        val[i] = Some(100.0);
        let s = item.iter().position(|s| s == "NSFR status").unwrap();
        note[s] = "Below the minimum".into();
    });
    assert!(tick(&prepare(Template::LcrNsfr, &df, &ctx()).unwrap(), "NSFR").breach.starts_with("Breach"));
    // and a ratio above its minimum that the tool calls a pass has no breach, whatever we would compute
    let df = lcr_with(|item, val, note| {
        let i = item.iter().position(|s| s == "LCR").unwrap();
        val[i] = Some(99.0);
        let s = item.iter().position(|s| s == "LCR status").unwrap();
        note[s] = "Meets the minimum".into();
    });
    assert!(tick(&prepare(Template::LcrNsfr, &df, &ctx()).unwrap(), "LCR").breach.is_empty());
    // a missing minimum or status row stops the chart with a plain message
    let no_min = lcr_with(|item, val, note| {
        let i = item.iter().position(|s| s == "LCR minimum").unwrap();
        item.remove(i);
        val.remove(i);
        note.remove(i);
    });
    let e = prepare(Template::LcrNsfr, &no_min, &ctx()).err().expect("refused").to_string();
    assert!(e.contains("'LCR minimum'") && e.contains("Run the LCR / NSFR tool again"), "{e}");
    let no_status = lcr_with(|item, val, note| {
        let i = item.iter().position(|s| s == "NSFR status").unwrap();
        item.remove(i);
        val.remove(i);
        note.remove(i);
    });
    let e = prepare(Template::LcrNsfr, &no_status, &ctx()).err().expect("refused").to_string();
    assert!(e.contains("NSFR status"), "{e}");
    // the guide no longer promises a default
    let guide = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/guide/chart-lcr-nsfr.md")).unwrap();
    assert!(!guide.contains("taken as 100"), "the guide must not say a missing minimum is taken as 100");
}

// ----------------------------------------------------------------------- 10. template subtitles carry the unit

#[test]
fn r10_a_template_subtitle_ends_with_the_unit_for_amounts_and_not_for_percentages_in_all_four_renderers() {
    let mut cases: Vec<(String, ChartData, bool)> = Vec::new();
    for (tpl, df) in all_cases() {
        let (p, d) = chart_of(tpl, &df);
        let amount = matches!(tpl, Template::GapLadder | Template::RepricingGap | Template::NiiBridge | Template::ClassifiedTrend | Template::EclByStage | Template::DepositMix | Template::RateProfile | Template::BehaviourProfile);
        cases.push((tpl.name().to_string(), d, amount));
        if let Some(c) = p.companion {
            let d2 = compute(c.table.clone().unwrap().lazy(), &c.spec).unwrap();
            let amt = matches!(c.template, Template::NiiBridge) || c.spec.template.contains("hqla") || c.spec.subtitle.starts_with("HQLA");
            cases.push((format!("{} second", tpl.name()), d2, amt));
        }
    }
    let o = crore();
    for (name, d, amount) in &cases {
        let want = format!("\u{b7} {}", o.unit);
        let s = subtitle_text(d, &o);
        assert_eq!(s.contains(&want), *amount, "{name}: the unit belongs on amount charts only: {s}");
        let all = subtitles(d, &o, name);
        for (i, text) in all.iter().enumerate() {
            if i == 3 && !python() {
                continue;
            }
            let renderer = ["app", "html", "python", "excel"][i];
            let has = text.contains("BDT Crore");
            assert_eq!(has, *amount, "{name} ({renderer}): unit in the subtitle: {text}");
        }
    }
    assert!(cases.iter().filter(|c| c.2).count() >= 8);
}

// ---------------------------------------------------------------------------- 11. sample parameters footnote

fn with_row(df: &DataFrame, cells: Vec<Series>) -> DataFrame {
    df.vstack(&DataFrame::new(cells).unwrap()).unwrap()
}

#[test]
fn r11_irrbb_lcr_and_ecl_charts_copy_the_sample_parameters_text_into_the_footnote() {
    let warn = "The shock file is marked sample: confirm the numbers before they are used in a return.";
    let irrbb = with_row(
        &sm::irrbb_results(),
        vec![
            Series::new("Scenario".into(), ["Sample parameters"]),
            Series::new("EVE change".into(), [None::<f64>]),
            Series::new("Loss".into(), [None::<f64>]),
            Series::new("Loss % of Tier 1".into(), [None::<f64>]),
            Series::new("\u{394}NII".into(), [None::<f64>]),
            Series::new("Note".into(), [warn]),
        ],
    );
    let p = prepare(Template::IrrbbScenarios, &irrbb, &ctx()).unwrap();
    assert!(p.spec.footnote.contains(warn) && p.spec.footnote.contains("BRPD IRRBB guideline"), "the sample text and the rule source: {}", p.spec.footnote);
    let lcr_warn = "The factors in this map are a SAMPLE (Basel), not Bangladesh Bank's.";
    let lcr = lcr_with(|item, val, note| {
        item.push("Sample parameters".into());
        val.push(None);
        note.push(lcr_warn.into());
    });
    let p = prepare(Template::LcrNsfr, &lcr, &ctx()).unwrap();
    assert!(p.spec.footnote.contains(lcr_warn), "{}", p.spec.footnote);
    assert!(p.companion.expect("hqla").spec.footnote.contains(lcr_warn), "the HQLA chart too");
    let ecl_warn = "The parameter file is marked sample.";
    let ecl = with_row(
        &sm::ecl_summary(),
        vec![
            Series::new("Section".into(), ["Sample parameters"]),
            Series::new("Group".into(), [ecl_warn]),
            Series::new("Loans".into(), [None::<f64>]),
            Series::new("EAD".into(), [None::<f64>]),
            Series::new("ECL".into(), [None::<f64>]),
        ],
    );
    let p = prepare(Template::EclByStage, &ecl, &ctx()).unwrap();
    assert!(p.spec.footnote.contains(ecl_warn), "{}", p.spec.footnote);
    // and it reaches the page
    let (_, d) = chart_of(Template::IrrbbScenarios, &irrbb);
    assert!(chart_html(&d, &crore()).unwrap().contains("marked sample"), "the page shows it");
    // without such a row there is no warning to copy
    assert!(!prepare(Template::LcrNsfr, &sm::lcr_summary(), &ctx()).unwrap().spec.footnote.contains("SAMPLE"));
}

// ------------------------------------------------------------------------------------ 3. colours (library)

#[test]
fn r3_the_library_resets_the_colour_map_only_when_the_category_or_series_column_changes() {
    let all = forms();
    let mut spec = all.iter().find(|(k, _)| *k == "donut").map(|(_, d)| d.spec.clone()).expect("donut");
    let d = lay(&all.iter().find(|(k, _)| *k == "donut").unwrap().1, 900.0, 560.0, false);
    spec.colour_map = d.colour_map.clone();
    assert!(!spec.colour_map.slots.is_empty());
    let before = spec.clone();
    let mut same = spec.clone();
    same.top_n = 3;
    same.reset_colours_if_columns_changed(&before);
    assert_eq!(same.colour_map, before.colour_map, "Top N does not repaint");
    let mut other = spec.clone();
    other.category = "Month".into();
    other.reset_colours_if_columns_changed(&before);
    assert!(other.colour_map.slots.is_empty());
    let mut other = spec.clone();
    other.series = Some("Product".into());
    other.reset_colours_if_columns_changed(&before);
    assert!(other.colour_map.slots.is_empty());
    // the new names then take slots 0, 1, 2 in order
    let mut cm = other.colour_map.clone();
    cm.assign_all(&["HC", "LC", "NC", "(blank)"]);
    assert_eq!([cm.get("HC"), cm.get("LC"), cm.get("NC"), cm.get("(blank)")], [Some(Colour::Slot(0)), Some(Colour::Slot(1)), Some(Colour::Slot(2)), Some(Colour::Slot(3))]);
    let _ = (t::SLOTS, ChartScale::Follow, Stack::Grouped);
}

// ------------------------------------------------------------------- Excel 100% charts have a 0-1 axis

#[test]
fn r12_the_excel_100_percent_chart_has_its_axis_from_0_to_1_written_as_percent() {
    if !python() {
        return;
    }
    let all = forms();
    let d = all.iter().find(|(k, _)| *k == "percent").map(|(_, d)| d.clone()).expect("percent form");
    let p = tmp("r12.xlsx");
    write_chart_xlsx(&p, &d, &crore()).expect("xlsx");
    let o = std::process::Command::new("python3")
        .arg("-I")
        .arg("-c")
        .arg("import sys,zipfile,re; z=zipfile.ZipFile(sys.argv[1]); x=[z.read(n).decode() for n in z.namelist() if re.match(r'xl/charts/chart\\d+\\.xml$', n)][0]; print(re.findall(r'<c:max val=\"([^\"]+)\"', x), re.findall(r'formatCode=\"([^\"]+)\"', x)[:3])")
        .arg(&p)
        .output()
        .expect("python");
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    assert!(out.contains("['1']") || out.contains("['1.0']") || out.contains("\"1\""), "the axis maximum is 1 (100%), not 100: {out}");
    assert!(!out.contains("'100'"), "{out}");
}


// =====================================================================================================================
// PART B
// =====================================================================================================================

fn has_matplotlib() -> bool {
    python() && std::process::Command::new("python3").arg("-c").arg("import matplotlib, pandas").output().map(|o| o.status.success()).unwrap_or(false)
}

/// Runs a generated script headless and returns the texts it put on the axes (data labels, annotations).
fn script_texts(script: &str, name: &str) -> Vec<String> {
    let path = tmp(&format!("r-{}.py", name.replace([' ', '/', ':'], "_")));
    std::fs::write(&path, script).unwrap();
    let wrapper = "import matplotlib\\nmatplotlib.use('Agg')\\nimport sys, json\\nimport matplotlib.pyplot as plt\\nout = []\\ndef show(*a, **k):\\n    for n in plt.get_fignums():\\n        for ax in plt.figure(n).axes:\\n            out.extend(t.get_text() for t in ax.texts if t.get_text())\\nplt.show = show\\nsrc = open(sys.argv[1], encoding='utf-8').read()\\nexec(compile(src, sys.argv[1], 'exec'), {'__name__': '__main__'})\\nprint(json.dumps(out))".replace("\\n", "\n");
    let o = std::process::Command::new("python3").arg("-c").arg(wrapper).arg(&path).env("MPLBACKEND", "Agg").output().expect("python3 runs");
    assert!(o.status.success(), "{name}: the script failed:\n{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("{name}: {e}: {}", String::from_utf8_lossy(&o.stdout)))
}

fn with_labels(d: &ChartData, f: impl Fn(&mut micro_parquet::charts::spec::LabelSpec)) -> ChartData {
    let mut d = d.clone();
    f(&mut d.spec.labels);
    d
}

fn form(name: &str) -> ChartData {
    crate::p7_c3::forms().into_iter().find(|(k, _)| *k == name).map(|(_, d)| d).unwrap_or_else(|| panic!("form {name}"))
}

// ------------------------------------------------------------------------------------ 12. blanks are refused

#[test]
fn r13_a_blank_is_refused_not_read_as_zero_and_a_percentage_of_a_zero_total_is_blank_with_a_note() {
    use micro_parquet::charts::samples as sm;
    // gap ladder: the Total and Overdue rows have no numbers and are left out; a bucket with a blank is refused by name
    let ladder = sm::ladder_table();
    assert!(prepare(Template::GapLadder, &ladder, &ctx()).is_ok());
    let cum: Vec<Option<f64>> = ladder.column("Cumulative gap % of total assets").unwrap().f64().unwrap().into_iter().enumerate().map(|(i, v)| if i == 1 { None } else { v }).collect();
    let mut bad = ladder.clone();
    bad.with_column(Series::new("Cumulative gap % of total assets".into(), cum)).unwrap();
    let e = prepare(Template::GapLadder, &bad, &ctx()).err().expect("refused").to_string();
    assert!(e.contains("'1\u{2013}3 months'") && e.contains("Cumulative gap % of total assets") && e.contains("not read as zero"), "{e}");
    // ECL: a stage with no EAD has a blank coverage, with a note, not 0%
    let mut ecl = sm::ecl_summary();
    let ead: Vec<f64> = vec![900.0, 1000.0, 700.0, 0.0, 100.0, 1000.0];
    ecl.with_column(Series::new("EAD".into(), ead)).unwrap();
    let p = prepare(Template::EclByStage, &ecl, &ctx()).unwrap();
    let tb = p.table.clone().unwrap();
    let cov: Vec<Option<f64>> = tb.column("Coverage %").unwrap().f64().unwrap().into_iter().collect();
    assert_eq!(cov[1], None, "Stage 2 has no EAD: its coverage is blank");
    assert!(cov[0].is_some() && cov[2].is_some());
    assert!(p.spec.footnote.contains("Coverage is blank where the EAD is 0: Stage 2"), "{}", p.spec.footnote);
    let d = compute(tb.lazy(), &p.spec).unwrap();
    let ci = d.measure(2).unwrap();
    assert_eq!(ci.values[0][1], None, "and it stays blank in the chart");
    // an ECL stage with a blank EAD is refused
    let blank_ead = ecl.clone().lazy().with_column(when(col("Group").eq(lit("Stage 3"))).then(lit(NULL).cast(DataType::Float64)).otherwise(col("EAD")).alias("EAD")).collect().unwrap();
    let e = prepare(Template::EclByStage, &blank_ead, &ctx()).err().expect("refused").to_string();
    assert!(e.contains("'Stage 3'") && e.contains("'EAD'"), "{e}");
    // classified trend: a month with no loans has a blank NPL %, with a note; a blank balance is refused
    let (mut m, mut g, mut b) = (Vec::new(), Vec::new(), Vec::new());
    for (month, vals) in [("2026-08-31", [100.0, 10.0, 5.0, 0.0, 0.0]), ("2026-09-30", [0.0, 0.0, 0.0, 0.0, 0.0])] {
        for (i, gr) in ["STD", "SMA", "SS", "DF", "BL"].iter().enumerate() {
            m.push(month);
            g.push(*gr);
            b.push(vals[i]);
        }
    }
    let zero_month = DataFrame::new(vec![sm::dates("Month", &m), Series::new("Grade".into(), g.clone()), Series::new("Balance".into(), b.clone())]).unwrap();
    let p = prepare(Template::ClassifiedTrend, &zero_month, &ctx()).unwrap();
    let npl: Vec<Option<f64>> = p.table.as_ref().unwrap().column("NPL %").unwrap().f64().unwrap().into_iter().collect();
    assert!(npl.iter().any(|v| v.is_none()) && npl.iter().any(|v| v.is_some()), "{npl:?}");
    assert!(p.spec.footnote.contains("NPL % is blank where the month has no loans: Sep 2026"), "{}", p.spec.footnote);
    let blank_bal: Vec<Option<f64>> = b.iter().enumerate().map(|(i, v)| if i == 2 { None } else { Some(*v) }).collect();
    let bad = DataFrame::new(vec![sm::dates("Month", &m), Series::new("Grade".into(), g), Series::new("Balance".into(), blank_bal)]).unwrap();
    let e = prepare(Template::ClassifiedTrend, &bad, &ctx()).err().expect("refused").to_string();
    assert!(e.contains("row 3") && e.contains("blank"), "{e}");
    // CL migration: a blank matrix cell, deposit mix and rate profile: refused
    let mut mig = sm::cl_matrix_large();
    let std_col: Vec<Option<f64>> = mig.column("STD").unwrap().f64().unwrap().into_iter().enumerate().map(|(i, v)| if i == 0 { None } else { v }).collect();
    mig.with_column(Series::new("STD".into(), std_col)).unwrap();
    let e = prepare(Template::ClMigration, &mig, &ctx()).err().expect("refused").to_string();
    assert!(e.contains("'STD'") && e.contains("not read as zero"), "{e}");
    let mut mix = sm::mix();
    let bal: Vec<Option<f64>> = mix.column("Balance").unwrap().f64().unwrap().into_iter().enumerate().map(|(i, v)| if i == 4 { None } else { v }).collect();
    mix.with_column(Series::new("Balance".into(), bal)).unwrap();
    assert!(prepare(Template::DepositMix, &mix, &ctx()).err().expect("refused").to_string().contains("row 5"));
    let mut bands = sm::rate_bands();
    let bal: Vec<Option<f64>> = bands.column("Balance").unwrap().f64().unwrap().into_iter().enumerate().map(|(i, v)| if i == 1 { None } else { v }).collect();
    bands.with_column(Series::new("Balance".into(), bal)).unwrap();
    assert!(prepare(Template::RateProfile, &bands, &ctx()).err().expect("refused").to_string().contains("row 2"));
    // IRRBB: a scenario row with no EVE change is refused; the footer rows (no loss figure) are fine
    let irrbb = sm::irrbb_results();
    assert!(prepare(Template::IrrbbScenarios, &irrbb, &ctx()).is_ok());
    let eve: Vec<Option<f64>> = irrbb.column("EVE change").unwrap().f64().unwrap().into_iter().enumerate().map(|(i, v)| if i == 2 { None } else { v }).collect();
    let mut bad = irrbb.clone();
    bad.with_column(Series::new("EVE change".into(), eve)).unwrap();
    let e = prepare(Template::IrrbbScenarios, &bad, &ctx()).err().expect("refused").to_string();
    assert!(e.contains("'Steepener'") && e.contains("'EVE change'"), "{e}");
}

// ------------------------------------------------------------------------------ 13. Python matches the app

#[test]
fn r14_python_uses_the_views_decimals_signed_two_decimal_labels_and_the_apps_heatmap_text() {
    if !has_matplotlib() {
        eprintln!("r14: python3 with matplotlib and pandas is not installed here: the scripts are not run");
        return;
    }
    // the view's decimals reach the labels: 3 decimals
    let o3 = LayoutOpts { decimals: 3, ..crore() };
    let col = form("column");
    let texts = script_texts(&chart_to_python(&col, &o3).unwrap(), "dec3");
    assert!(texts.iter().any(|t| t.contains('.') && t.rsplit('.').next().map(|f| f.len() == 3).unwrap_or(false)), "labels have 3 decimals: {texts:?}");
    let texts2 = script_texts(&chart_to_python(&col, &crore()).unwrap(), "dec2");
    assert!(texts2.iter().any(|t| t.rsplit('.').next().map(|f| f.len() == 2).unwrap_or(false)), "{texts2:?}");
    // a signed bar: always a sign, two decimals for a percentage
    let (_, irrbb) = chart_of(Template::IrrbbScenarios, &sm::irrbb_results());
    let texts = script_texts(&chart_to_python(&irrbb, &crore()).unwrap(), "irrbb");
    assert!(texts.iter().any(|t| t == "\u{2212}10.50%") && texts.iter().any(|t| t.starts_with('+') && t.ends_with('%') && t.rsplit('.').next().map(|f| f.len() == 3).unwrap_or(false)), "{texts:?}");
    // the heatmap text is the app's: no % sign, one decimal, and the same strings
    let heat = crate::p7_c3::heat();
    let texts = script_texts(&chart_to_python(&heat, &crore()).unwrap(), "heat");
    let app = lay_with(&heat, &crore());
    let cells: Vec<String> = app.texts.iter().filter(|x| x.role == Role::Cell).map(|x| x.text.clone()).collect();
    assert!(!cells.is_empty());
    for c in &cells {
        assert!(texts.contains(c), "the app writes '{c}' in a cell: the script writes {texts:?}");
    }
    // the NPL end label reads "NPL %  7.0%", as in the app
    let (_, trend) = chart_of(Template::ClassifiedTrend, &sm::classified());
    let texts = script_texts(&chart_to_python(&trend, &crore()).unwrap(), "trend");
    assert!(texts.iter().any(|t| t == "NPL %  7.0%"), "{texts:?}");
    let app = lay_with(&trend, &crore());
    assert!(app.texts.iter().any(|x| x.text == "NPL %  7.0%"));
    // the same refusals: a donut of one part
    let mut one = form("donut");
    one.categories.truncate(1);
    one.values[0].truncate(1);
    one.rows[0].truncate(1);
    assert!(chart_to_python(&one, &crore()).is_err() && chart_pair(&one, &crore()).is_err());
}

// ------------------------------------------------------------------------------ 14. zero is grey in every renderer

#[test]
fn r15_an_exact_zero_diverging_bar_is_the_grey_zero_colour_in_every_renderer() {
    let df = DataFrame::new(vec![Series::new("Scenario".into(), ["Up", "Flat", "Down"]), Series::new("Change".into(), [4.0, 0.0, -6.0])]).unwrap();
    let spec = ChartSpec { category: "Scenario".into(), value: Some("Change".into()), kind: ChartKind::Bar, horizontal: true, order: micro_parquet::sort_lists::SortOrder::List { name: "o".into(), values: vec!["Up".into(), "Flat".into(), "Down".into()], case_insensitive: true }, ..Default::default() };
    let d = compute(df.lazy(), &spec).unwrap();
    assert_eq!([t::diverging_colour(4.0), t::diverging_colour(0.0), t::diverging_colour(-6.0)], [t::DIVERGING_POSITIVE, t::DIVERGING_ZERO, t::DIVERGING_NEGATIVE]);
    // the app
    let l = lay_with(&d, &crore());
    let fills: Vec<u32> = d.categories.iter().enumerate().map(|(c, _)| {
        let m = l.marks.iter().find(|m| m.category == c && m.kind == micro_parquet::charts::layout::MarkKind::Bar).expect("bar");
        l.prims.iter().find_map(|p| if let micro_parquet::charts::layout::Prim::Rect { rect, fill, .. } = p { (*rect == m.rect).then_some(((fill.r() as u32) << 16) | ((fill.g() as u32) << 8) | fill.b() as u32) } else { None }).expect("fill")
    }).collect();
    assert_eq!(fills, [t::DIVERGING_POSITIVE, t::DIVERGING_ZERO, t::DIVERGING_NEGATIVE], "app");
    // the page
    let pair = chart_pair(&d, &crore()).unwrap();
    let items = pair.light["series"][0]["data"].as_array().unwrap().clone();
    let colours: Vec<String> = items.iter().map(|i| i["itemStyle"]["color"].as_str().unwrap_or("").to_string()).collect();
    assert_eq!(colours, [t::hex_text(t::DIVERGING_POSITIVE), t::hex_text(t::DIVERGING_ZERO), t::hex_text(t::DIVERGING_NEGATIVE)], "html");
    // the script
    let py = chart_to_python(&d, &crore()).unwrap();
    assert!(py.contains("#c3c2b7") && py.contains("v > 0 else"), "python");
    // the workbook: one point colour each
    if python() {
        let p = tmp("r15.xlsx");
        write_chart_xlsx(&p, &d, &crore()).unwrap();
        let j = inspect(&p);
        let pts = j["charts"][0]["plots"][0]["series"][0]["points"].as_array().unwrap().clone();
        let hexes: Vec<String> = pts.iter().map(|v| v.as_str().unwrap_or("").to_uppercase()).collect();
        assert_eq!(hexes, [format!("{:06X}", t::DIVERGING_POSITIVE), format!("{:06X}", t::DIVERGING_ZERO), format!("{:06X}", t::DIVERGING_NEGATIVE)], "excel");
    }
    // a waterfall step of zero is grey too
    let steps = ["Start", "Up", "None", "Down", "End"];
    let bridge = DataFrame::new(vec![Series::new("Step".into(), steps), Series::new("Amount".into(), [10.0, 5.0, 0.0, -3.0, 12.0])]).unwrap();
    let w = compute(bridge.lazy(), &ChartSpec { category: "Step".into(), value: Some("Amount".into()), kind: ChartKind::Waterfall, order: micro_parquet::sort_lists::SortOrder::List { name: "o".into(), values: steps.map(String::from).to_vec(), case_insensitive: true }, ..Default::default() }).unwrap();
    let pair = chart_pair(&w, &crore()).unwrap().light.to_string();
    assert!(pair.contains(&t::hex_text(t::DIVERGING_ZERO)), "html waterfall");
}

// ---------------------------------------------------------------------------------- 15. too many bars

#[test]
fn r16_more_than_60_bars_or_40_heatmap_columns_are_refused_with_use_top_n_or_a_filter() {
    let n = 61;
    let cats: Vec<String> = (0..n).map(|i| format!("C{i:02}")).collect();
    let df = DataFrame::new(vec![Series::new("Cat".into(), cats.clone()), Series::new("Val".into(), (0..n).map(|i| i as f64 + 1.0).collect::<Vec<_>>())]).unwrap();
    let spec = ChartSpec { category: "Cat".into(), value: Some("Val".into()), kind: ChartKind::Bar, top_n: 200, ..Default::default() };
    let e = compute(df.clone().lazy(), &spec).err().expect("refused").to_string();
    assert!(e.contains("61") && e.contains("Use Top N or a filter"), "{e}");
    // 60 are fine; Top N makes 61 fine (30 and "Other")
    assert!(compute(df.head(Some(60)).lazy(), &spec).is_ok());
    assert!(compute(df.clone().lazy(), &ChartSpec { top_n: 30, ..spec.clone() }).is_ok());
    // a heatmap of 41 columns
    let (mut r, mut c, mut v) = (Vec::new(), Vec::new(), Vec::new());
    for i in 0..41 {
        r.push("A".to_string());
        c.push(format!("K{i:02}"));
        v.push(1.0 + i as f64);
    }
    let hm = DataFrame::new(vec![Series::new("Row".into(), r), Series::new("Col".into(), c), Series::new("V".into(), v)]).unwrap();
    let hs = ChartSpec { category: "Col".into(), series: Some("Row".into()), value: Some("V".into()), kind: ChartKind::Heatmap, top_n: 200, ..Default::default() };
    let e = compute(hm.clone().lazy(), &hs).err().expect("refused").to_string();
    assert!(e.contains("41 heatmap columns") && e.contains("Use Top N or a filter"), "{e}");
    assert!(compute(hm.head(Some(40)).lazy(), &hs).is_ok());
    // the 2 MB page limit for a 1,000-point chart still holds
    let n = 1000;
    let pts = DataFrame::new(vec![Series::new("X".into(), (0..n).map(|i| i as f64).collect::<Vec<_>>()), Series::new("Y".into(), (0..n).map(|i| (i as f64 * 0.01).sin()).collect::<Vec<_>>())]).unwrap();
    let d = compute(pts.lazy(), &ChartSpec { category: "X".into(), value: Some("Y".into()), kind: ChartKind::Scatter, ..Default::default() }).unwrap();
    assert!(chart_html(&d, &crore()).unwrap().len() < 2 * 1024 * 1024);
}

// -------------------------------------------------------------------------------- 16. previews and temp files

#[test]
fn r17_the_preview_goes_under_the_apps_temp_folder_replacing_the_last_one_and_the_chart_files_are_cleaned() {
    use micro_parquet::charts::echarts::{clean_chart_temp, preview_dir, write_preview};
    let dir = std::env::temp_dir().join(format!("mp-r17-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let a = write_preview(&form("column"), &crore(), &dir).unwrap();
    assert!(a.starts_with(preview_dir(&dir)) && a.exists());
    assert!(!a.starts_with(std::env::temp_dir().join("microparquet-preview")), "never the system's temp folder directly");
    let b = write_preview(&form("line"), &crore(), &dir).unwrap();
    assert_eq!(std::fs::read_dir(preview_dir(&dir)).unwrap().count(), 1, "one preview: the last replaces the one before");
    assert!(std::fs::read_to_string(&b).unwrap().contains("mp-chart"));
    // the chart studio's other temporary files go; other files stay
    for n in ["chart_data_1_2.parquet", "chart_tab_1_2.parquet", "chart_template_1_2.parquet", "keep_me.parquet", "chart_data_note.txt"] {
        std::fs::write(dir.join(n), b"x").unwrap();
    }
    clean_chart_temp(&dir);
    let left: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
    assert!(!preview_dir(&dir).exists());
    assert!(left.contains(&"keep_me.parquet".to_string()) && left.contains(&"chart_data_note.txt".to_string()), "{left:?}");
    assert!(!left.iter().any(|n| n.starts_with("chart_tab_") || n.starts_with("chart_template_") || n == "chart_data_1_2.parquet"), "{left:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------------------------------------------------ 17. "<!--" in a page

#[test]
fn r18_a_category_with_an_html_comment_start_is_escaped_and_the_page_still_loads() {
    let df = DataFrame::new(vec![Series::new("Cat".into(), ["<!-- note", "plain", "</script> x"]), Series::new("Val".into(), [3.0, 2.0, 1.0])]).unwrap();
    let d = compute(df.lazy(), &ChartSpec { category: "Cat".into(), value: Some("Val".into()), kind: ChartKind::Bar, ..Default::default() }).unwrap();
    let html = chart_html(&d, &crore()).unwrap();
    let at = html.find("id=\"mp-data\"").unwrap();
    let from = at + html[at..].find('>').unwrap() + 1;
    let to = from + html[from..].find("</script>").unwrap();
    let json = &html[from..to];
    assert!(!json.contains("<!--") && json.contains("\\u003c!--"), "the comment start is escaped inside the data");
    let v: serde_json::Value = serde_json::from_str(json).expect("the data is valid JSON");
    assert!(v["charts"][0]["light"].to_string().contains("<!-- note"), "the text itself survives");
    // and the real page works: loaded in headless Chromium with no script error (where it is installed)
    if node_with_playwright() {
        let page = tmp("r18.html");
        std::fs::write(&page, &html).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let o = std::process::Command::new("node").arg(root.join("tools/html_svg_click.js")).arg(&page).output().unwrap();
        let j: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        assert!(j["svg"]["errors"].as_array().map(|a| a.is_empty()).unwrap_or(false), "{j}");
        assert!(j["svg"]["text"].as_str().unwrap_or("").starts_with("<svg"), "the chart drew and saved: {j}");
    }
}

// ---------------------------------------------------------------------------- 18. neutral heatmap cells

#[test]
fn r19_zero_and_missing_heatmap_cells_are_neutral_in_both_themes_and_the_ramp_is_for_values_above_zero() {
    assert_eq!((t::HEAT_NEUTRAL_LIGHT, t::HEAT_NEUTRAL_DARK), (0xe1e0d9, 0x2c2c2a));
    assert_eq!((t::heat_neutral(false), t::heat_neutral(true)), (t::ink(false).grid, t::ink(true).grid), "light: the grid colour; dark: #2c2c2a");
    assert_eq!((t::heat_neutral_text(false), t::heat_neutral_text(true)), (t::ink(false).muted, t::ink(true).muted));
    let d = crate::p7_c3::heat();
    for dark in [false, true] {
        let o = LayoutOpts { dark, ..crore() };
        let l = lay_with(&d, &o);
        let fill_of = |r: egui::Rect| -> u32 {
            l.prims.iter().find_map(|p| if let micro_parquet::charts::layout::Prim::Rect { rect, fill, .. } = p { (*rect == r).then_some(((fill.r() as u32) << 16) | ((fill.g() as u32) << 8) | fill.b() as u32) } else { None }).expect("fill")
        };
        let mut neutral = 0;
        let mut ramp = Vec::new();
        for m in l.marks.iter().filter(|m| m.kind == micro_parquet::charts::layout::MarkKind::Cell) {
            let f = fill_of(m.rect);
            let text = l.texts.iter().find(|x| x.role == Role::Cell && m.rect.contains_rect(x.rect));
            if m.value.unwrap_or(0.0) <= 0.0 {
                neutral += 1;
                assert_eq!(f, t::heat_neutral(dark), "dark={dark}: a zero cell is neutral");
                let tc = text.expect("its value is written").colour;
                let want = t::heat_neutral_text(dark);
                assert_eq!(((tc.r() as u32) << 16) | ((tc.g() as u32) << 8) | tc.b() as u32, want, "dark={dark}: muted text");
            } else {
                assert!(t::SEQUENTIAL.contains(&f), "dark={dark}: a value above zero is on the ramp");
                ramp.push((m.value.unwrap(), f));
            }
        }
        assert!(neutral > 0 && !ramp.is_empty());
        // the smallest value above zero is the lowest step of the ramp, the largest the highest
        let lo = ramp.iter().min_by(|a, b| a.0.partial_cmp(&b.0).unwrap()).unwrap().1;
        let hi = ramp.iter().max_by(|a, b| a.0.partial_cmp(&b.0).unwrap()).unwrap().1;
        assert_eq!((lo, hi), (t::sequential_colour(0, dark), t::sequential_colour(12, dark)));
        // the page
        let pair = chart_pair(&d, &o).unwrap();
        let opt = if dark { &pair.dark } else { &pair.light };
        let items = opt["series"].as_array().unwrap().iter().find(|s| s["type"] == "heatmap").unwrap()["data"].as_array().unwrap().clone();
        for i in &items {
            let v = i["value"][2].as_f64().unwrap();
            let c = i["itemStyle"]["color"].as_str().unwrap();
            assert_eq!(c == t::hex_text(t::heat_neutral(dark)), v <= 0.0, "html dark={dark}: {v} {c}");
            if v <= 0.0 {
                assert_eq!(i["label"]["color"], t::hex_text(t::heat_neutral_text(dark)));
            }
        }
    }
    // the script and the workbook
    let py = chart_to_python(&d, &crore()).unwrap();
    assert!(py.contains("NEUTRAL, NEUTRAL_TEXT = GRID, MUTED") && py.contains("pd.isna(v) or v <= 0"));
    if python() {
        let p = tmp("r19.xlsx");
        write_chart_xlsx(&p, &d, &crore()).unwrap();
        let o = std::process::Command::new("python3")
            .arg("-I")
            .arg("-c")
            .arg("import sys,zipfile; z=zipfile.ZipFile(sys.argv[1]); s=z.read('xl/worksheets/sheet1.xml').decode(); st=z.read('xl/styles.xml').decode(); print('lessThanOrEqual' in s, 'stopIfTrue=\"1\"' in s, 'E1E0D9' in st.upper(), s.index('cellIs') < s.index('colorScale'))")
            .arg(&p)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "True True True True", "the neutral rule comes first, stops, and has the neutral fill");
    }
}

// ----------------------------------------------------------------------------------- 19. N2 - N7

#[test]
fn r20_html_line_end_labels_are_the_apps_names_for_several_series_and_the_value_for_one() {
    let line = form("line");
    let pair = chart_pair(&line, &crore()).unwrap();
    let mut labels: Vec<String> = Vec::new();
    for s in pair.light["series"].as_array().unwrap() {
        for it in s["data"].as_array().unwrap() {
            if let Some(f) = it["label"]["formatter"].as_str() {
                labels.push(f.to_string());
            }
        }
    }
    let names: Vec<&String> = line.series.iter().collect();
    assert_eq!(labels.len(), names.len(), "one end label per series: {labels:?}");
    for n in names {
        assert!(labels.contains(n), "the label is the name only: {labels:?}");
    }
    // as in the app
    let app: Vec<String> = lay_with(&line, &crore()).texts.iter().filter(|x| x.role == Role::Direct).map(|x| x.text.clone()).collect();
    let mut a = app.clone();
    a.sort();
    let mut b = labels.clone();
    b.sort();
    assert_eq!(a, b);
    // one series: its value
    let one = chart_pair(&{
        let mut spec = line.spec.clone();
        spec.series = None;
        compute(crate::p7_c3::frame().lazy(), &spec).unwrap()
    }, &crore()).unwrap();
    let f: Vec<String> = one.light["series"][0]["data"].as_array().unwrap().iter().filter_map(|i| i["label"]["formatter"].as_str().map(String::from)).collect();
    assert_eq!(f.len(), 1);
    assert!(f[0].chars().any(|c| c.is_ascii_digit()) && !f[0].contains("Sum"), "{f:?}");
}

#[test]
fn r21_deposit_mix_colours_follow_the_product_names_the_single_ratio_must_agree_and_zero_changes_say_unchanged() {
    use micro_parquet::charts::samples as sm;
    let p = prepare(Template::DepositMix, &sm::mix(), &ctx()).unwrap();
    // the names in order: Current, Savings, Term deposit -> slots 0, 1, 2, whatever order the data has
    assert_eq!([p.spec.colour_map.get("Current"), p.spec.colour_map.get("Savings"), p.spec.colour_map.get("Term deposit")], [Some(Colour::Slot(0)), Some(Colour::Slot(1)), Some(Colour::Slot(2))]);
    let reversed = sm::mix().reverse();
    let q = prepare(Template::DepositMix, &reversed, &ctx()).unwrap();
    assert_eq!(p.spec.colour_map, q.spec.colour_map, "the same colours for the same names");
    // a ratio drawn once must be the same in every series of its category
    let (_, d) = chart_of(Template::ClassifiedTrend, &sm::classified());
    assert!(d.spec.panels.iter().any(|p| p.single));
    let p = prepare(Template::ClassifiedTrend, &sm::classified(), &ctx()).unwrap();
    let mut tb = p.table.clone().unwrap();
    let npl: Vec<Option<f64>> = tb.column("NPL %").unwrap().f64().unwrap().into_iter().enumerate().map(|(i, v)| if i == 1 { v.map(|x| x + 3.0) } else { v }).collect();
    tb.with_column(Series::new("NPL %".into(), npl)).unwrap();
    let e = compute(tb.lazy(), &p.spec).err().expect("stopped").to_string();
    assert!(e.contains("'NPL %'") && e.contains("same in every series"), "{e}");
    // a change that shows as zero is "unchanged", never "+0.0 rise" or "-0.0 fall"
    let f = |prev: f64, curr: f64| -> String {
        let df = DataFrame::new(vec![
            Series::new("Product".into(), ["TD"]),
            Series::new("Prev annual interest".into(), [prev]),
            Series::new("Curr annual interest".into(), [curr]),
            Series::new("Volume effect".into(), [curr - prev]),
            Series::new("Rate effect".into(), [0.0]),
            Series::new("Mix effect".into(), [0.0]),
        ])
        .unwrap();
        prepare(Template::NiiBridge, &df, &ctx()).unwrap().finding
    };
    assert_eq!(f(100.0, 100.04), "Interest is unchanged between the two months");
    assert_eq!(f(100.0, 99.97), "Interest is unchanged between the two months");
    assert!(f(100.0, 100.5).contains("+0.5") && f(100.0, 99.0).contains("\u{2212}1.0"));
    for s in [f(100.0, 100.04), f(100.0, 99.97)] {
        assert!(!s.contains("+0.0") && !s.contains("\u{2212}0.0"), "{s}");
    }
    // the guide's numbers say how the title rounds
    let guide = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/guide/chart-nii-bridge.md")).unwrap();
    assert!(guide.contains("two decimals; the title rounds the change (+13.50) to one decimal, +13.5 Cr"));
}

// --------------------------------------------------------------------------------------- 20. Data labels

#[test]
fn r22_the_data_labels_setting_loads_from_old_charts_and_defaults_to_the_old_behaviour() {
    use micro_parquet::charts::spec::{LabelMode, LabelUnit};
    // a chart saved by Session C has no "labels" at all
    let old = r#"{"category":"Month","value":"Balance","agg":"Sum","kind":"Bar","title":"x"}"#;
    let s: ChartSpec = serde_json::from_str(old).expect("loads");
    assert!(s.labels.is_default() && s.labels.mode == LabelMode::Auto && s.labels.percent && s.labels.decimals.is_none() && s.labels.unit == LabelUnit::AsView);
    // a labels object that lacks fields takes the defaults for them (percent labels stay on)
    let s: ChartSpec = serde_json::from_str(r#"{"category":"M","labels":{"mode":"All"}}"#).unwrap();
    assert_eq!((s.labels.mode, s.labels.percent), (LabelMode::All, true));
    // it round-trips through a saved chart file
    let dir = std::env::temp_dir().join(format!("mp-r22-{}", std::process::id()));
    let mut s = s;
    s.labels.decimals = Some(3);
    s.labels.unit = LabelUnit::Lakh;
    s.labels.percent = false;
    micro_parquet::charts::library::save_chart(&dir, "labels", &s).unwrap();
    let (_, back) = micro_parquet::charts::library::load_chart(&micro_parquet::charts::library::chart_path(&dir, "labels")).unwrap();
    assert_eq!(back.labels, s.labels);
    let _ = std::fs::remove_dir_all(&dir);
    // and a chart with the default setting draws exactly as before (the goldens in c3 pin the page; here the app)
    let col = form("column");
    let a = lay_with(&col, &crore());
    let b = lay_with(&with_labels(&col, |l| *l = Default::default()), &crore());
    assert_eq!(a.texts, b.texts);
}

#[test]
fn r23_each_data_label_option_works_in_the_app_the_page_the_workbook_and_the_script() {
    use micro_parquet::charts::spec::{LabelMode, LabelUnit};
    let col = form("column");
    let n = col.categories.len();
    assert_eq!(n, 6);
    let app_values = |d: &ChartData| lay_with(d, &crore()).texts.iter().filter(|x| x.role == Role::Label).count();
    let html_labels = |d: &ChartData| -> Vec<String> {
        chart_pair(d, &crore()).unwrap().light["series"][0]["data"].as_array().unwrap().iter().filter_map(|i| i["label"]["formatter"].as_str().map(String::from)).collect()
    };
    let xlsx_json = |d: &ChartData, name: &str| -> Option<serde_json::Value> {
        if !python() {
            return None;
        }
        let p = tmp(&format!("r23-{name}.xlsx"));
        write_chart_xlsx(&p, d, &crore()).unwrap();
        let o = std::process::Command::new("python3")
            .arg("-I")
            .arg("-c")
            .arg("import sys,zipfile,re,json; z=zipfile.ZipFile(sys.argv[1]); x=[z.read(n).decode() for n in z.namelist() if re.match(r'xl/charts/chart\\d+\\.xml$', n)][0]; print(json.dumps({'hidden': x.count('<c:delete val=\"1\"/>'), 'texts': re.findall(r'<c:dLbl>.*?<a:t>([^<]*)</a:t>', x), 'showval': x.count('<c:showVal val=\"1\"/>')}))")
            .arg(&p)
            .output()
            .unwrap();
        Some(serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&o.stdout))))
    };
    // Auto: a single series of 6 bars has a label on each (the design system's rule)
    assert_eq!(app_values(&col), 6);
    assert_eq!(html_labels(&col).len(), 6);
    // All: every bar (nothing collides here)
    let all = with_labels(&col, |l| l.mode = LabelMode::All);
    assert_eq!(app_values(&all), 6);
    assert_eq!(html_labels(&all).len(), 6);
    // Ends only: the first and the last
    let ends = with_labels(&col, |l| l.mode = LabelMode::EndsOnly);
    assert_eq!(app_values(&ends), 2);
    assert_eq!(html_labels(&ends).len(), 2);
    // None: no value label anywhere
    let none = with_labels(&col, |l| l.mode = LabelMode::None);
    assert_eq!(app_values(&none), 0);
    assert_eq!(html_labels(&none).len(), 0);
    // Decimals: 0 and 3
    let d0 = with_labels(&col, |l| l.decimals = Some(0));
    let t0: Vec<String> = lay_with(&d0, &crore()).texts.iter().filter(|x| x.role == Role::Label).map(|x| x.text.clone()).collect();
    assert!(t0.iter().all(|s| !s.contains('.')) && t0.len() == 6, "{t0:?}");
    assert!(html_labels(&d0).iter().all(|s| !s.contains('.')));
    let d3 = with_labels(&col, |l| l.decimals = Some(3));
    assert!(lay_with(&d3, &crore()).texts.iter().filter(|x| x.role == Role::Label).all(|x| x.text.rsplit('.').next().map(|f| f.len() == 3).unwrap_or(false)));
    assert!(html_labels(&d3).iter().all(|s| s.rsplit('.').next().map(|f| f.len() == 3).unwrap_or(false)));
    // Unit: labels in Crore while the view is plain (divisor 1): 1,250 BDT is 0.0001 Cr -> "0.00"; in Thousand it is "1.25"
    let thousand = with_labels(&col, |l| l.unit = LabelUnit::Thousand);
    let tk: Vec<String> = lay_with(&thousand, &crore()).texts.iter().filter(|x| x.role == Role::Label).map(|x| x.text.clone()).collect();
    assert!(tk.contains(&"1.25".to_string()), "1,250 in Thousand is 1.25: {tk:?}");
    assert!(html_labels(&thousand).contains(&"1.25".to_string()));
    let note = "Data labels are in Thousand";
    assert!(lay_with(&thousand, &crore()).texts.iter().any(|x| x.role == Role::Footnote && x.text.contains(note)), "the chart says so");
    assert!(chart_to_python(&thousand, &crore()).unwrap().contains(note) && chart_pair(&thousand, &crore()).unwrap().light.to_string().contains(note));
    // Percent labels on a share view: off hides them (the heatmap's cell text and a 100% bar's)
    let heat = crate::p7_c3::heat();
    let off = with_labels(&heat, |l| l.percent = false);
    assert!(lay_with(&heat, &crore()).texts.iter().any(|x| x.role == Role::Cell));
    assert!(!lay_with(&off, &crore()).texts.iter().any(|x| x.role == Role::Cell), "no percent text in the cells");
    let opt = chart_pair(&off, &crore()).unwrap().light;
    let cells = opt["series"].as_array().unwrap().iter().find(|s| s["type"] == "heatmap").unwrap()["data"].as_array().unwrap().clone();
    assert!(!cells.is_empty() && cells.iter().all(|i| i["label"]["show"] == false), "html cells carry no text");
    let on = chart_pair(&heat, &crore()).unwrap().light;
    assert!(on["series"].as_array().unwrap().iter().find(|s| s["type"] == "heatmap").unwrap()["data"].as_array().unwrap().iter().all(|i| i["label"]["show"] == true));
    // the workbook
    if let Some(j) = xlsx_json(&none, "none") {
        assert_eq!(j["texts"].as_array().unwrap().len(), 0);
    }
    if let (Some(a), Some(e)) = (xlsx_json(&all, "all"), xlsx_json(&ends, "ends")) {
        assert_eq!(a["texts"].as_array().unwrap().len(), 6, "{a}");
        assert_eq!(e["texts"].as_array().unwrap().len(), 2, "{e}");
        assert_eq!(e["hidden"].as_u64().unwrap(), 4, "{e}");
    }
    if let Some(j) = xlsx_json(&thousand, "thousand") {
        assert!(j["texts"].as_array().unwrap().iter().any(|v| v == "1.25"), "{j}");
    }
    // the script (where matplotlib is installed)
    if has_matplotlib() {
        let count = |d: &ChartData, name: &str| script_texts(&chart_to_python(d, &crore()).unwrap(), name).iter().filter(|s| s.chars().any(|c| c.is_ascii_digit())).count();
        assert_eq!(count(&col, "auto"), 6);
        assert_eq!(count(&all, "all"), 6);
        assert_eq!(count(&ends, "ends"), 2);
        assert_eq!(count(&none, "none"), 0);
        let t = script_texts(&chart_to_python(&thousand, &crore()).unwrap(), "thousand");
        assert!(t.contains(&"1.25".to_string()), "{t:?}");
    }
}

#[test]
fn r24_all_never_prints_a_label_that_collides_and_says_how_many_were_left_out() {
    use micro_parquet::charts::spec::LabelMode;
    // 40 columns at the width of a side panel: the labels cannot all fit
    let n = 40;
    let df = DataFrame::new(vec![Series::new("Cat".into(), (0..n).map(|i| format!("Category {i:02}")).collect::<Vec<_>>()), Series::new("Val".into(), (0..n).map(|i| 1_000_000.0 + i as f64 * 37_000.0).collect::<Vec<_>>())]).unwrap();
    let spec = ChartSpec { category: "Cat".into(), value: Some("Val".into()), kind: ChartKind::Bar, top_n: 50, labels: micro_parquet::charts::spec::LabelSpec { mode: LabelMode::All, ..Default::default() }, ..Default::default() };
    let d = compute(df.lazy(), &spec).unwrap();
    let o = LayoutOpts { divisor: 1.0, decimals: 2, ..crore() };
    let l = layout_chart(&d, egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(420.0, 420.0)), &o, &ApproxMeasure);
    let printed = l.texts.iter().filter(|x| x.role == Role::Label).count();
    assert!(!l.dropped.is_empty() && printed + l.dropped.len() == n, "printed {printed}, left out {}", l.dropped.len());
    // none overlaps another label or leaves the chart
    for (i, a) in l.texts.iter().enumerate() {
        for b in l.texts.iter().skip(i + 1) {
            if a.angle != 0.0 && b.angle != 0.0 {
                continue; // angled category labels: the plan keeps them a line apart (s2)
            }
            assert!(!(a.rect.intersects(b.rect) && a.rect.intersect(b.rect).width() > 0.5 && a.rect.intersect(b.rect).height() > 0.5), "'{}' overlaps '{}'", a.text, b.text);
        }
    }
    let note = format!("{} data label", l.dropped.len());
    let foot = l.texts.iter().find(|x| x.role == Role::Footnote).map(|x| x.text.clone()).unwrap_or_default();
    assert!(foot.contains(&note) && foot.contains("left out where"), "the note says how many: {foot}");
    // the page keeps every label in the option and lets ECharts hide the ones that really overlap; its note says so
    let pair = chart_pair(&d, &o).unwrap();
    assert!(pair.light.to_string().contains("would overlap are hidden"), "the page's note");
    assert!(pair.light["series"].as_array().unwrap().iter().all(|s| s["labelLayout"]["hideOverlap"] == true), "hideOverlap on every series");
    let shown = pair.light["series"][0]["data"].as_array().unwrap().iter().filter(|i| i["label"]["formatter"].is_string()).count();
    assert_eq!(shown, n, "the page puts every label in the option");
    // the script and the workbook keep the nominal rule: they leave out the labels the app leaves out, and say it
    let py = chart_to_python(&d, &o).unwrap();
    assert!(py.contains("DROPPED = {(0, "), "the script lists the labels it leaves out");
    assert!(py.contains("left out where"));
    // Auto and the other modes never drop anything
    assert!(layout_chart(&with_labels(&d, |l| l.mode = LabelMode::Auto), egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(420.0, 420.0)), &o, &ApproxMeasure).dropped.is_empty());
}

