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
    let d = compute(mix.clone().lazy(), &p.spec).unwrap();
    assert_eq!(d.categories, ["31/01/2026", "31/03/2026", "30/04/2026"], "the columns follow the dates");
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
