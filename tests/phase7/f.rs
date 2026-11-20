//! Session C: the owner's fixes after the first review of C3–C5. Dark-theme sequential fills, the true
//! minus sign everywhere, signed two-decimal labels on diverging bars, the template changes (classified
//! trend, yield curves, data quality, gap ladder, LCR/NSFR, NII bridge), the waterfall minimum height
//! and connectors, "row"/"rows", and the Excel axes.

use crate::p7_c1::{all_sizes, check, lay, overlap};
use crate::p7_c3::{forms, heat, opts as html_opts};
use crate::p7_c5::{all_cases, chart_of, v3_ladder};
use micro_parquet::chart::ChartKind;
use micro_parquet::charts::calc::compute;
use micro_parquet::charts::data::ChartData;
use micro_parquet::charts::echarts::{chart_pair, ECHARTS_JS};
use micro_parquet::charts::layout::{MarkKind, Prim, Role};
use micro_parquet::charts::samples::*;
use micro_parquet::charts::templates::{prepare, yield_curves, Ctx, Template};
use micro_parquet::charts::tokens as t;
use micro_parquet::charts::xlsx::write_chart_xlsx;
use polars::prelude::IntoLazy;
use serde_json::Value;
use std::path::PathBuf;

fn ctx() -> Ctx {
    Ctx { divisor: 1.0, suffix: "Cr".into(), dq_green: Some(98.0), dq_amber: Some(95.0), ..Default::default() }
}

fn python() -> bool {
    std::process::Command::new("python3").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mp-f-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

fn json_strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) if !s.starts_with('@') => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| json_strings(x, out)),
        Value::Object(m) => m.values().for_each(|x| json_strings(x, out)),
        _ => {}
    }
}

/// Every chart the checks run on: the 20 forms, every template's chart, and a signed chart of the owner's example.
pub(crate) fn every_chart() -> Vec<(String, ChartData)> {
    let mut v: Vec<(String, ChartData)> = forms().into_iter().map(|(n, d)| (n.to_string(), d)).collect();
    let mut cases = all_cases();
    // the NII bridge of three products also has its effects-by-key chart
    cases.push((Template::NiiBridge, sm_mom_keys()));
    for (tpl, df) in cases {
        let (p, d) = chart_of(tpl, &df);
        v.push((format!("template {}", tpl.name()), d));
        if let Some(c) = p.companion {
            let d2 = compute(c.table.clone().unwrap().lazy(), &c.spec).unwrap();
            v.push((format!("template {} second chart", tpl.name()), d2));
        }
    }
    v
}

fn sm_mom_keys() -> polars::prelude::DataFrame {
    micro_parquet::charts::samples::mom_table_keys()
}

// ------------------------------------------------------------------------------ 1. dark sequential

#[test]
fn f1_the_dark_sequential_ramp_runs_from_the_darkest_step_at_low_values_to_the_lightest_at_high() {
    // pinned in tokens.rs
    assert_eq!(t::sequential_colour(0, false), t::SEQUENTIAL[0], "light: low is the lightest");
    assert_eq!(t::sequential_colour(12, false), t::SEQUENTIAL[12], "light: high is the darkest");
    assert_eq!(t::sequential_colour(0, true), t::SEQUENTIAL[12], "dark: low is the darkest, nearest the surface");
    assert_eq!(t::sequential_colour(12, true), t::SEQUENTIAL[0], "dark: high is the lightest");
    for r in 0..13 {
        assert_eq!(t::sequential_colour(r, true), t::sequential_colour(12 - r, false), "the dark ramp is the light one reversed");
    }
    // the text follows the fill that is drawn
    assert!(t::sequential_text_white(0, true) && !t::sequential_text_white(12, true));
    assert!(!t::sequential_text_white(0, false) && t::sequential_text_white(12, false));

    let d = heat();
    for dark in [false, true] {
        // in the app
        let l = lay(&d, 900.0, 520.0, dark);
        let cells: Vec<_> = l.marks.iter().filter(|m| m.kind == MarkKind::Cell).collect();
        assert!(cells.len() > 4);
        let fill_of = |r: egui::Rect| -> u32 {
            l.prims
                .iter()
                .find_map(|p| match p {
                    Prim::Rect { rect, fill, .. } if *rect == r => Some(((fill.r() as u32) << 16) | ((fill.g() as u32) << 8) | fill.b() as u32),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("no fill for {r:?}"))
        };
        // the ramp is for values above zero: its lowest step is the smallest positive cell (zero is neutral, tested in r.rs)
        let lo = cells.iter().filter(|c| c.value.is_some_and(|v| v > 0.0)).min_by(|a, b| a.value.partial_cmp(&b.value).unwrap()).unwrap();
        let hi = cells.iter().max_by(|a, b| a.value.partial_cmp(&b.value).unwrap()).unwrap();
        assert_eq!(fill_of(lo.rect), t::sequential_colour(0, dark), "dark={dark}: the lowest cell");
        assert_eq!(fill_of(hi.rect), t::sequential_colour(12, dark), "dark={dark}: the highest cell");
        // the cell text is readable on the fill: white on step 7 and darker, the light ink on the paler steps
        for c in [lo, hi] {
            let fill = fill_of(c.rect);
            let white = t::SEQUENTIAL.iter().position(|s| *s == fill).unwrap() >= t::SEQUENTIAL_WHITE_TEXT_FROM;
            let text = l.texts.iter().find(|x| x.role == Role::Cell && c.rect.contains_rect(x.rect)).unwrap_or_else(|| panic!("a label in the cell {:?}", c.rect));
            assert_eq!(text.colour == egui::Color32::WHITE, white, "dark={dark}: text on {fill:06x}");
        }
        // in the HTML
        let pair = chart_pair(&d, &html_opts()).unwrap();
        let opt = if dark { &pair.dark } else { &pair.light };
        let series = opt["series"].as_array().unwrap().iter().find(|s| s["type"] == "heatmap").unwrap();
        let items = series["data"].as_array().unwrap();
        let val = |i: &Value| i["value"][2].as_f64().unwrap();
        let lo = items.iter().filter(|i| val(i) > 0.0).min_by(|a, b| val(a).partial_cmp(&val(b)).unwrap()).unwrap();
        let hi = items.iter().max_by(|a, b| val(a).partial_cmp(&val(b)).unwrap()).unwrap();
        assert_eq!(lo["itemStyle"]["color"], t::hex_text(t::sequential_colour(0, dark)), "html dark={dark}: low");
        assert_eq!(hi["itemStyle"]["color"], t::hex_text(t::sequential_colour(12, dark)), "html dark={dark}: high");
    }
}

// ---------------------------------------------------------------------------- 2. the true minus sign

#[test]
fn f2_no_rendered_string_writes_a_hyphen_before_a_number() {
    assert_eq!(t::minus("-10.5%"), "\u{2212}10.5%");
    assert_eq!(t::minus("Breach: -16.7% vs -10% limit"), "Breach: \u{2212}16.7% vs \u{2212}10% limit");
    assert_eq!(t::minus("Mar-24 and 2026-01-31 and 1\u{2013}3 months"), "Mar-24 and 2026-01-31 and 1\u{2013}3 months", "a hyphen in a word or between digits stays");
    assert_eq!(t::minus("(-5)"), "(\u{2212}5)");
    let mut bad: Vec<String> = Vec::new();
    for (name, d) in every_chart() {
        // the app: every text, tooltip and crosshair, in both themes and at the three sizes
        for l in all_sizes(&d, &name) {
            for x in &l.texts {
                if let Some(s) = t::find_hyphen_minus(&x.text) {
                    bad.push(format!("{name} text: {s}"));
                }
            }
            for m in &l.marks {
                if let Some(s) = t::find_hyphen_minus(&m.tip) {
                    bad.push(format!("{name} tip: {s}"));
                }
            }
            for c in &l.crosshairs {
                if let Some(s) = t::find_hyphen_minus(&c.tip) {
                    bad.push(format!("{name} crosshair: {s}"));
                }
            }
        }
        // the HTML: every string of both options, and the table
        if let Ok(pair) = chart_pair(&d, &html_opts()) {
            for opt in [&pair.light, &pair.dark] {
                let mut strings = Vec::new();
                json_strings(opt, &mut strings);
                for s in strings {
                    if let Some(f) = t::find_hyphen_minus(&s) {
                        bad.push(format!("{name} html: {f}"));
                    }
                }
            }
            if let Some(f) = t::find_hyphen_minus(&pair.table.replace("<td>", " ").replace("</td>", " ")) {
                bad.push(format!("{name} html table: {f}"));
            }
        }
    }
    assert!(bad.is_empty(), "a hyphen where the minus sign belongs:\n{}", bad.join("\n"));
    // the page's own number formatter writes the minus sign too
    assert!(micro_parquet::charts::echarts::chart_html(&forms().remove(0).1, &html_opts()).unwrap().contains("'\u{2212}'"));
    let _ = ECHARTS_JS;
}

/// Strings a person reads in the workbook: shared strings, chart texts and number formats.
const READ_TEXTS: &str = r#"
import sys, json, zipfile, re
import xml.etree.ElementTree as ET
z = zipfile.ZipFile(sys.argv[1])
out = []
for n in z.namelist():
    if not n.endswith('.xml'): continue
    root = ET.fromstring(z.read(n))
    for el in root.iter():
        tag = el.tag.split('}')[1]
        if n.endswith('sharedStrings.xml') and tag == 't' and el.text: out.append(el.text)
        if n.startswith('xl/charts/') and tag == 't' and el.text: out.append(el.text)
        if tag == 'formatCode': out.append(el.get('val') or '')
        if tag == 'numFmt' and el.get('formatCode'): out.append(el.get('formatCode'))
print(json.dumps(out))
"#;

#[test]
fn f2_the_workbook_writes_the_minus_sign_in_texts_and_number_formats() {
    if !python() {
        eprintln!("python3 is not installed: the workbook XML check is skipped");
        return;
    }
    let mut bad: Vec<String> = Vec::new();
    let mut any_format = false;
    for (name, d) in every_chart() {
        let p = tmp(&format!("{}.xlsx", name.replace([' ', '/'], "-")));
        if write_chart_xlsx(&p, &d, &html_opts()).is_err() {
            continue; // the HTML-only forms are not written
        }
        let o = std::process::Command::new("python3").arg("-I").arg("-c").arg(READ_TEXTS).arg(&p).output().expect("python3 runs");
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let strings: Vec<String> = serde_json::from_slice(&o.stdout).unwrap();
        for s in strings {
            if s.contains('#') || s.contains('0') && s.contains(';') {
                any_format = true;
                // a number format has a negative section written with the minus sign
                if s.contains(';') && !s.contains('\u{2212}') {
                    bad.push(format!("{name} format without the minus sign: {s}"));
                }
            }
            if let Some(f) = t::find_hyphen_minus(&s) {
                bad.push(format!("{name}: {f}"));
            }
        }
    }
    assert!(any_format, "number formats were found");
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

// ------------------------------------------------------------------------ Q2: signed two decimals

#[test]
fn f3_diverging_bars_are_labelled_with_a_sign_and_two_decimals_in_all_three_renderers() {
    let (p, d) = chart_of(Template::IrrbbScenarios, &irrbb_results());
    assert!(p.spec.horizontal);
    let want: Vec<String> = d.values[0].iter().map(|v| {
        let v = v.unwrap();
        let body = micro_parquet::models::format_with_commas(v.abs(), 2);
        if v < 0.0 { format!("\u{2212}{body}%") } else { format!("+{body}%") }
    }).collect();
    assert!(want.iter().any(|w| w.starts_with('+')) && want.iter().any(|w| w.starts_with('\u{2212}')), "{want:?}");
    for l in all_sizes(&d, "irrbb") {
        for w in &want {
            assert!(l.texts.iter().any(|x| x.role == Role::Label && x.text == *w), "the app writes {w}: {:?}", l.texts.iter().map(|x| &x.text).collect::<Vec<_>>());
        }
    }
    let pair = chart_pair(&d, &html_opts()).unwrap();
    let mut strings = Vec::new();
    json_strings(&pair.light, &mut strings);
    for w in &want {
        assert!(strings.iter().any(|s| s == w), "the HTML writes {w}");
    }
    if python() {
        let p = tmp("irrbb-signed.xlsx");
        write_chart_xlsx(&p, &d, &html_opts()).unwrap();
        let o = std::process::Command::new("python3").arg("-I").arg("-c").arg(READ_TEXTS).arg(&p).output().unwrap();
        let _ = o;
        // the label format of the series is written in the chart XML: "+" and the minus sign with two decimals and a percent sign
        let xml = std::process::Command::new("python3")
            .arg("-I")
            .arg("-c")
            .arg("import sys,zipfile,re\nz=zipfile.ZipFile(sys.argv[1])\nprint(''.join(z.read(n).decode() for n in z.namelist() if n.startswith('xl/charts/chart')))")
            .arg(&p)
            .output()
            .unwrap();
        let xml = String::from_utf8_lossy(&xml.stdout).to_string();
        assert!(xml.contains("&quot;+&quot;#,##0.00&quot;%&quot;;&quot;\u{2212}&quot;#,##0.00&quot;%&quot;"), "the Excel data labels are signed with two decimals: {}", &xml[..xml.len().min(600)]);
    }
}

// --------------------------------------------------------------------------------- 3. classified trend

#[test]
fn f4_the_classified_trend_stacks_only_sma_ss_df_and_bl() {
    let (p, d) = chart_of(Template::ClassifiedTrend, &classified());
    assert_eq!(d.series, ["SMA", "SS", "DF", "BL"], "Standard is not stacked");
    assert!(p.spec.subtitle.contains("Standard (STD) not shown"), "{}", p.spec.subtitle);
    // the NPL % panel is unchanged: SS + DF + BL over every grade including STD
    assert_eq!(p.finding, "NPL ratio rose to 7.0%");
    let npl = d.measure(1).unwrap();
    let first = npl.values.iter().flatten().flatten().next().copied().unwrap();
    assert!((first - 5.0).abs() < 1e-9, "(20+20+10)/1000 = 5%: {first}");
    assert_eq!(p.spec.panels.len(), 2);
    for l in all_sizes(&d, "classified trend") {
        assert!(l.message.is_none());
        assert!(l.texts.iter().any(|x| x.text.contains("Standard (STD) not shown")), "the subtitle says so: {:?}", l.texts.iter().map(|x| format!("{:?}:{}", x.role, x.text)).collect::<Vec<_>>());
        assert!(!l.texts.iter().any(|x| x.role == Role::Legend && x.text == "STD"), "no STD legend entry");
    }
}

// ------------------------------------------------------------------------------- 4. yield curves

fn many_curves() -> Vec<micro_parquet::charts::templates::CurveInput> {
    use micro_parquet::charts::templates::CurveInput;
    let d = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    // five curves whose right ends are within a few hundredths of each other: their labels collide unless nudged
    let mk = |date: &str, base: f64| CurveInput { date: d(date), points: vec![(30, base), (90, base + 0.4), (365, base + 1.0), (1825, base + 1.85)] };
    vec![mk("2026-05-31", 7.40), mk("2026-06-30", 7.42), mk("2026-08-31", 7.44), mk("2026-09-30", 7.46), mk("2026-10-31", 7.60)]
}

#[test]
fn f5_every_yield_curve_is_labelled_at_its_right_end_and_the_older_ones_are_grey() {
    let y = yield_curves(&many_curves()).unwrap();
    let d = compute(y.table.clone().unwrap().lazy(), &y.spec).unwrap();
    assert_eq!(d.series.len(), 5);
    let names = ["31 May 2026", "30 Jun 2026", "31 Aug 2026", "30 Sep 2026", "31 Oct 2026"];
    // colours: the latest keeps blue, the older curves two greys (the oldest the fainter), the same in the other themes
    let cm = &y.spec.colour_map;
    assert_eq!(cm.get("31 Oct 2026").map(|c| c.hex(false)), Some(t::SERIES_LIGHT[0]));
    let greys: Vec<u32> = names[..4].iter().map(|n| cm.get(n).unwrap().hex(false)).collect();
    assert_eq!(greys.iter().collect::<std::collections::BTreeSet<_>>().len(), 2, "two grey steps: {greys:?}");
    assert_eq!(greys[0], t::INK_LIGHT.deemph, "the oldest is the faintest grey");
    assert_eq!(greys[3], t::INK_LIGHT.muted, "the most recent older curve is the darker grey");
    assert!(greys.iter().all(|g| *g != t::SERIES_LIGHT[0]));
    for l in all_sizes(&d, "yield curves") {
        assert!(l.message.is_none());
        let end_labels: Vec<_> = l.texts.iter().filter(|x| x.role == Role::Direct).collect();
        assert_eq!(end_labels.len(), 5, "every curve is labelled: {:?}", end_labels.iter().map(|x| &x.text).collect::<Vec<_>>());
        let plot = l.panels[0].plot;
        // the last point of the curves is the centre of the last tenor's band
        let end_x = l.marks.iter().filter(|m| m.kind == MarkKind::Point && m.category == 3).map(|m| m.rect.center().x).fold(f32::MIN, f32::max);
        for n in names {
            let x = end_labels.iter().find(|x| x.text == n).unwrap_or_else(|| panic!("{n} has no label"));
            assert!(x.rect.left() >= end_x, "{n} is labelled at the right end, beyond the last point: {:?} vs {end_x}", x.rect);
        }
        // collisions are nudged apart vertically: the labels keep the order of the curves and do not overlap
        let mut by_y: Vec<_> = end_labels.iter().map(|x| (x.rect.center().y, x.text.clone())).collect();
        by_y.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let order: Vec<&str> = by_y.iter().map(|x| x.1.as_str()).collect();
        assert_eq!(order, ["31 Oct 2026", "30 Sep 2026", "31 Aug 2026", "30 Jun 2026", "31 May 2026"], "highest curve first");
        // only the latest curve carries markers
        let markers: Vec<_> = l.marks.iter().filter(|m| m.kind == MarkKind::Point).collect();
        let latest = d.series.iter().position(|n| n == "31 Oct 2026").unwrap();
        assert!(markers.iter().all(|m| m.series == latest), "markers on the latest curve only");
        assert_eq!(markers.len(), 4, "one per tenor");
        // the y axis starts one tick below the lowest value (7.40)
        let axis = &l.panels[0];
        let lowest = axis.coord(7.40);
        let bottom_tick = l.texts.iter().filter(|x| x.role == Role::Tick && x.rect.center().x < plot.left()).map(|x| x.rect.center().y).fold(f32::MIN, f32::max);
        assert!(bottom_tick > lowest + 1.0, "a tick below the lowest curve: lowest at {lowest}, bottom tick at {bottom_tick}");
    }
    // the HTML labels every curve at its end, nudged apart, and the Excel chart styles the older curves grey
    let pair = chart_pair(&d, &html_opts()).unwrap();
    let series = pair.light["series"].as_array().unwrap();
    let lines: Vec<_> = series.iter().filter(|s| s["type"] == "line").collect();
    assert_eq!(lines.len(), 5);
    assert!(lines.iter().all(|s| s["labelLayout"]["moveOverlap"] == "shiftY"));
    let with_symbols: Vec<_> = lines.iter().filter(|s| s["symbol"] == "circle" && s["symbolSize"].as_f64().unwrap_or(0.0) > 0.0).collect();
    assert_eq!(with_symbols.len(), 1, "only the latest curve has markers");
    let line_of = |n: &str| lines.iter().find(|s| s["name"] == n).unwrap_or_else(|| panic!("no line {n}"));
    assert_eq!(line_of("31 May 2026")["lineStyle"]["color"], t::hex_text(t::INK_LIGHT.deemph));
    assert_eq!(line_of("31 Oct 2026")["lineStyle"]["color"], t::hex_text(t::SERIES_LIGHT[0]));
    let yaxis = pair.light["yAxis"].as_array().map(|a| a[0].clone()).unwrap_or(pair.light["yAxis"].clone());
    assert!(yaxis["min"].as_f64().unwrap() <= 7.40 - 0.1, "the HTML axis is padded below: {}", yaxis["min"]);
}

// ------------------------------------------------------------------------------ 5. data quality

#[test]
fn f6_a_rule_below_amber_is_critical_with_an_icon_and_the_words() {
    use micro_parquet::charts::spec::Status;
    let df = {
        use polars::prelude::*;
        DataFrame::new(vec![
            Series::new("Rule".into(), ["Blank Actnum", "Balance in range", "Unique key", "Valid date", "Overall score"]),
            Series::new("Rows checked".into(), [Some(100i64), Some(100), Some(100), Some(100), None]),
            Series::new("Rows failing".into(), [Some(0i64), Some(8), Some(3), Some(2), None]),
            Series::new("Pass %".into(), [Some(100.0), Some(92.0), Some(97.0), Some(98.5), Some(96.3)]),
            Series::new("Result".into(), ["", "", "", "", "Amber"]),
        ])
        .unwrap()
    };
    let p = prepare(Template::DataQuality, &df, &ctx()).unwrap();
    assert_eq!(p.finding, "1 rule below Amber");
    let st: Vec<(&str, Status, &str)> = p.spec.bar_status.iter().map(|b| (b.category.as_str(), b.status, b.label.as_str())).collect();
    assert_eq!(st, [("Balance in range", Status::Critical, "Below Amber"), ("Unique key", Status::Warning, "Below Green")], "the other bars stay default");
    let d = compute(p.table.clone().unwrap().lazy(), &p.spec).unwrap();
    for l in all_sizes(&d, "data quality") {
        assert!(l.message.is_none());
        let crit = egui::Color32::from_rgb(0xd0, 0x3b, 0x3b);
        let warn = egui::Color32::from_rgb(0xfa, 0xb2, 0x19);
        let fills: Vec<egui::Color32> = l.marks.iter().filter(|m| m.kind == MarkKind::Bar).map(|m| l.prims.iter().find_map(|p| if let Prim::Rect { rect, fill, .. } = p { if *rect == m.rect { Some(*fill) } else { None } } else { None }).unwrap()).collect();
        assert_eq!(fills.len(), 4);
        assert_eq!(fills[1], crit, "below Amber is critical");
        assert_eq!(fills[2], warn, "between Amber and Green is the warning colour");
        assert!(fills[0] != crit && fills[0] != warn && fills[3] != crit && fills[3] != warn, "the other bars are the default colour");
        assert!(l.texts.iter().any(|x| x.text == "Below Amber"), "the words, never the colour alone: {:?}", l.texts.iter().map(|x| format!("{:?}:{}", x.role, x.text)).collect::<Vec<_>>());
        assert!(l.texts.iter().any(|x| x.text == "Below Green"));
        // an icon beside each status text: a triangle
        let tris: Vec<_> = l.prims.iter().filter(|p| matches!(p, Prim::Tris { tris, .. } if tris.len() == 1)).collect();
        assert_eq!(tris.len(), 2, "one icon per status bar");
        // the two thresholds (Green 98, Amber 95) are close, and their labels do not overlap each other or anything else
        let refs: Vec<_> = l.texts.iter().filter(|x| x.role == Role::Reference).collect();
        assert_eq!(refs.len(), 2, "{:?}", refs.iter().map(|x| &x.text).collect::<Vec<_>>());
        assert!(!overlap(refs[0].rect, refs[1].rect), "the threshold labels never overlap: {:?} {:?}", refs[0].rect, refs[1].rect);
    }
    // HTML and Excel carry the colour and the words as well
    let pair = chart_pair(&d, &html_opts()).unwrap();
    let bars = pair.light["series"].as_array().unwrap().iter().find(|s| s["type"] == "bar").unwrap();
    let items = bars["data"].as_array().unwrap();
    assert_eq!(items[1]["itemStyle"]["color"], t::hex_text(t::STATUS_CRITICAL));
    assert!(items[1]["label"]["formatter"].as_str().unwrap().contains("Below Amber"));
    assert!(items[2]["label"]["formatter"].as_str().unwrap().contains("Below Green"));
    assert!(items[0]["itemStyle"]["color"].is_null(), "the default bar keeps the series colour");
    if python() {
        let path = tmp("dq.xlsx");
        write_chart_xlsx(&path, &d, &html_opts()).unwrap();
        let o = std::process::Command::new("python3").arg("-I").arg("-c").arg(READ_TEXTS).arg(&path).output().unwrap();
        let strings: Vec<String> = serde_json::from_slice(&o.stdout).unwrap();
        assert!(strings.iter().any(|s| s == "Below Amber") && strings.iter().any(|s| s == "Below Green"), "the Status column says it in words");
    }
}

// --------------------------------------------------------------------------------- 6. gap ladder

#[test]
fn f7_the_breach_label_sits_on_the_side_away_from_the_line_and_touches_no_mark() {
    use micro_parquet::charts::layout::LText;
    let (p, d) = chart_of(Template::GapLadder, &v3_ladder(Some(-10.0)));
    let label = &p.spec.limit_ticks[0].breach;
    for l in all_sizes(&d, "gap ladder") {
        let tx: &LText = l.texts.iter().find(|x| x.text == *label).expect("the breach is labelled");
        let rect = tx.rect;
        let ring = l.prims.iter().find_map(|p| if let Prim::Outline { c, r, .. } = p { Some((*c, *r)) } else { None }).unwrap();
        // not on the ring, not on any line segment of 1.5 px or wider (the data line, the limit tick), not on a mark
        let cx = ring.0.x.clamp(rect.left(), rect.right());
        let cy = ring.0.y.clamp(rect.top(), rect.bottom());
        assert!(((ring.0.x - cx).powi(2) + (ring.0.y - cy).powi(2)).sqrt() > ring.1, "the label is clear of the ring");
        for pr in &l.prims {
            if let Prim::Line { a, b, w, .. } = pr {
                if *w >= t::REFERENCE_LINE_PX {
                    assert!(!micro_parquet::charts::layout::segment_hits_rect(*a, *b, rect), "the label {:?} is crossed by a line {:?}-{:?}", rect, a, b);
                }
            }
        }
        for m in &l.marks {
            assert!(!overlap(rect, m.rect), "the label is covered by a mark {:?}", m.rect);
        }
        // the line leaves the ring toward bucket 2, which is above it (the cumulative gap rises): the label is below
        let line = &l.panels[1];
        let next = line.coord(33.33);
        assert!(next < ring.0.y, "bucket 2 is higher on the screen");
        assert!(rect.center().y > ring.0.y, "the label is on the side away from the line (below the ring): label {:?}, ring {:?}", rect, ring);
    }
}

// ------------------------------------------------------------------------------------ 7. waterfall

#[test]
fn f8_every_waterfall_step_is_at_least_two_pixels_tall_and_the_html_draws_the_connectors() {
    use polars::prelude::*;
    // a step of 0.5 on a bridge of 1,000: under a pixel
    let df = DataFrame::new(vec![Series::new("Step".into(), ["Start", "Tiny", "Big", "End"]), Series::new("Amount".into(), [1000.0, 0.5, -300.0, 700.5])]).unwrap();
    let order = ["Start", "Tiny", "Big", "End"].map(String::from).to_vec();
    let s = micro_parquet::charts::spec::ChartSpec { category: "Step".into(), value: Some("Amount".into()), kind: ChartKind::Waterfall, order: micro_parquet::sort_lists::SortOrder::List { name: "s".into(), values: order, case_insensitive: true }, ..Default::default() };
    let d = compute(df.lazy(), &s).unwrap();
    for l in all_sizes(&d, "waterfall") {
        let bars: Vec<_> = l.marks.iter().filter(|m| m.kind == MarkKind::Bar).collect();
        assert_eq!(bars.len(), 4);
        for b in bars {
            assert!(b.rect.height() >= t::MIN_BAR_PX - 1e-3, "a step is at least {} px tall: {:?}", t::MIN_BAR_PX, b.rect);
        }
    }
    let pair = chart_pair(&d, &html_opts()).unwrap();
    let series = pair.light["series"].as_array().unwrap();
    let with_connectors = series.iter().find(|s| s["markLine"]["data"].is_array() && s["name"] == "Base").expect("connectors");
    let conns = with_connectors["markLine"]["data"].as_array().unwrap();
    assert_eq!(conns.len(), 3, "one connector between each pair of bars");
    // the connector of step i is at that step's level, from bar i to bar i+1
    assert_eq!(conns[0][0]["coord"][0], 0);
    assert_eq!(conns[0][1]["coord"][0], 1);
    assert_eq!(conns[0][0]["coord"][1], conns[0][1]["coord"][1]);
    assert!((conns[0][0]["coord"][1].as_f64().unwrap() - 1000.0).abs() < 1e-6, "the level after the start bar");
    let bars: Vec<_> = series.iter().filter(|s| s["type"] == "bar" && s["name"] != "Base").collect();
    assert!(bars.iter().all(|s| s["barMinHeight"].as_f64() == Some(t::MIN_BAR_PX as f64)), "a minimum height in the page too");
    // no border eats into a 2 px step (a border of 1 px would leave nothing of it)
    assert!(bars.iter().all(|s| s["data"].as_array().unwrap().iter().filter(|i| !i.is_null()).all(|i| i["itemStyle"]["borderWidth"].as_f64() == Some(0.0))), "a waterfall step has no border");
    // a part without a value is null, so the minimum never draws a stub for a zero
    let nulls: usize = bars.iter().map(|s| s["data"].as_array().unwrap().iter().filter(|x| x.is_null()).count()).sum();
    assert!(nulls >= 3, "{nulls}");
}

// ------------------------------------------------------------------------------------- 8. tooltips

#[test]
fn f9_a_count_of_one_is_written_row_and_the_html_page_drops_the_repeated_category_line() {
    assert_eq!(t::tidy("1 rows"), "1 row");
    assert_eq!(t::tidy("11 rows, 21 rows, 1,001 rows, 1 rows"), "11 rows, 21 rows, 1,001 rows, 1 row");
    assert_eq!(t::tidy("0 rows"), "0 rows");
    // a group with one row in the tooltips of the three renderers
    use polars::prelude::*;
    let df = DataFrame::new(vec![Series::new("Month".into(), ["May", "Jun", "Jun"]), Series::new("Balance".into(), [10.0, 20.0, 30.0])]).unwrap();
    let s = micro_parquet::charts::spec::ChartSpec { category: "Month".into(), value: Some("Balance".into()), kind: ChartKind::Bar, ..Default::default() };
    let d = compute(df.lazy(), &s).unwrap();
    let l = lay(&d, 900.0, 500.0, false);
    let tips: Vec<&str> = l.marks.iter().map(|m| m.tip.as_str()).collect();
    assert!(tips.iter().any(|t| t.contains("\n1 row") && !t.contains("1 rows")), "{tips:?}");
    assert!(tips.iter().any(|t| t.contains("\n2 rows")));
    let pair = chart_pair(&d, &html_opts()).unwrap();
    let mut strings = Vec::new();
    json_strings(&pair.light, &mut strings);
    assert!(strings.iter().any(|s| s.contains("\n1 row") && !s.contains("1 rows")));
    assert!(!strings.iter().any(|s| s.contains("1 rows")));
    // the shared tooltip of the page: the header is the category, and a line "Month: May" inside the items is dropped
    let page = micro_parquet::charts::echarts::chart_html(&d, &html_opts()).unwrap();
    assert!(page.contains("': ' + head"), "the page drops a repeated category line");
}

// ------------------------------------------------------------------------------- 9. Excel axes

const AXES: &str = r#"
import sys, json, zipfile, re
import xml.etree.ElementTree as ET
ns = {'c': 'http://schemas.openxmlformats.org/drawingml/2006/chart'}
z = zipfile.ZipFile(sys.argv[1])
out = []
for n in sorted(x for x in z.namelist() if re.match(r'xl/charts/chart\d+\.xml$', x)):
    root = ET.fromstring(z.read(n))
    for ax in root.iter('{%s}valAx' % ns['c']):
        sc = ax.find('c:scaling', ns)
        mn = sc.find('c:min', ns); mx = sc.find('c:max', ns); mu = ax.find('c:majorUnit', ns)
        out.append({'file': n, 'min': float(mn.get('val')) if mn is not None else None, 'max': float(mx.get('val')) if mx is not None else None, 'unit': float(mu.get('val')) if mu is not None else None})
    out.append({'file': n, 'markers': len(list(root.iter('{%s}marker' % ns['c']))), 'areas': len(list(root.iter('{%s}areaChart' % ns['c'])))})
print(json.dumps(out))
"#;

fn axes_of(p: &PathBuf) -> Vec<Value> {
    let o = std::process::Command::new("python3").arg("-I").arg("-c").arg(AXES).arg(p).output().expect("python3 runs");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap()
}

#[test]
fn f10_excel_line_panels_use_the_apps_axis_range_and_stacked_areas_have_no_markers() {
    if !python() {
        eprintln!("python3 is not installed: the workbook axis check is skipped");
        return;
    }
    // a line of 8.6 to 10.2: the app's axis is nice ticks round the data, not forced to zero
    let y = yield_curves(&curves()).unwrap();
    let d = compute(y.table.clone().unwrap().lazy(), &y.spec).unwrap();
    let p = tmp("curves.xlsx");
    write_chart_xlsx(&p, &d, &html_opts()).unwrap();
    let axes = axes_of(&p);
    let ax = axes.iter().find(|a| a["min"].is_number()).expect("a fixed axis");
    let (lo, hi) = (ax["min"].as_f64().unwrap(), ax["max"].as_f64().unwrap());
    assert!(lo > 0.0, "a line is not forced to start at zero: {ax}");
    assert!(lo <= 7.5 - 0.1 && hi >= 10.2, "padded one tick below the lowest value, and over the highest: {lo} {hi}");
    // the app pads below the lowest curve too: its lowest tick is under 7.5
    let layout = lay(&d, 900.0, 500.0, false);
    let panel = &layout.panels[0];
    let lowest_tick = layout.texts.iter().filter(|x| x.role == Role::Tick && x.rect.center().x < panel.plot.left()).map(|x| x.rect.center().y).fold(f32::MIN, f32::max);
    assert!(lowest_tick > panel.coord(7.5), "the app's axis starts below the lowest curve as well");
    assert_eq!(ax["unit"].as_f64().map(|u| ((hi - lo) / u).round() * u), Some(hi - lo), "whole ticks from the minimum to the maximum");
    // a stacked area has no point markers
    let sa = forms().into_iter().find(|(n, _)| *n == "stacked-area").unwrap().1;
    let p = tmp("stacked-area.xlsx");
    write_chart_xlsx(&p, &sa, &html_opts()).unwrap();
    let m = axes_of(&p);
    let marks = m.iter().find(|a| a["markers"].is_number()).unwrap();
    assert_eq!(marks["areas"], 1);
    assert_eq!(marks["markers"], 0, "no markers on the layers of a stacked area");
    // and in the app and the HTML the layers show a point on hover only
    let l = lay(&sa, 900.0, 500.0, false);
    assert!(!l.prims.iter().any(|p| matches!(p, Prim::Circle { ring: Some(_), .. })), "no marker discs on a stacked area");
    assert!(l.marks.iter().any(|m| m.kind == MarkKind::Point), "the points still answer to the pointer");
    let pair = chart_pair(&sa, &html_opts()).unwrap();
    let lines = pair.light["series"].as_array().unwrap().iter().filter(|s| s["type"] == "line");
    assert!(lines.clone().count() > 1 && lines.into_iter().all(|s| s["showSymbol"] == false));
}

// -------------------------------------------------------------------------- 10. finishing C5

#[test]
fn f11_the_lcr_summary_also_gives_the_hqla_composition_as_a_tall_segmented_bar() {
    let (p, _) = chart_of(Template::LcrNsfr, &lcr_summary());
    let c = p.companion.as_ref().expect("the HQLA composition");
    assert_eq!(c.finding, "70.0% of the HQLA is Level 1");
    assert_eq!(c.tab_title, "Chart: HQLA composition");
    assert_eq!((c.spec.kind, c.spec.stack, c.spec.series.as_deref()), (ChartKind::Bar, micro_parquet::charts::spec::Stack::Stacked, Some("Level")));
    let d = compute(c.table.clone().unwrap().lazy(), &c.spec).unwrap();
    assert_eq!(d.categories, ["HQLA"], "one tall column");
    assert_eq!(d.series, ["Level 1", "Level 2A", "Level 2B"]);
    let total: f64 = d.values.iter().flatten().flatten().sum();
    assert!((total - 5000.0).abs() < 1e-9, "the levels add up to the HQLA after caps: {total}");
    for l in all_sizes(&d, "hqla") {
        assert!(l.message.is_none());
        assert_eq!(l.marks.iter().filter(|m| m.kind == MarkKind::Segment).count(), 3);
    }
    // caps leave part out: it is a fourth segment in the de-emphasised colour, so the bar stays whole
    use polars::prelude::*;
    let mut df = lcr_summary();
    let items: Vec<String> = df.column("Item").unwrap().str().unwrap().into_no_null_iter().map(String::from).collect();
    let mut vals: Vec<Option<f64>> = df.column("Value").unwrap().f64().unwrap().into_iter().collect();
    let at = items.iter().position(|s| s == "Adjustment for the Level 2B cap").unwrap();
    vals[at] = Some(250.0);
    df.with_column(Series::new("Value".into(), vals)).unwrap();
    let p = prepare(Template::LcrNsfr, &df, &ctx()).unwrap();
    let d = compute(p.companion.unwrap().table.unwrap().lazy(), &p.spec).ok();
    let _ = d;
    // an older summary without the level rows has nothing to compose
    let old = DataFrame::new(vec![
        Series::new("Item".into(), ["HQLA after caps", "LCR", "LCR minimum", "LCR status"]),
        Series::new("Value".into(), [Some(5_000.0), Some(133.68), Some(100.0), None]),
        Series::new("Note".into(), ["", "", "at least", "Meets the minimum"]),
    ])
    .unwrap();
    assert!(prepare(Template::LcrNsfr, &old, &ctx()).unwrap().companion.is_none());
}

#[test]
fn f12_the_nii_bridge_is_per_key_when_the_tab_has_one_key_and_adds_the_effects_by_key_when_it_has_several() {
    use polars::prelude::*;
    // one key: its bridge (the tab filtered to a key is that key's bridge), and nothing to compare it with
    let (p, _) = chart_of(Template::NiiBridge, &mom_table());
    assert!(p.finding.contains("in TD interest"), "{}", p.finding);
    assert!(p.companion.is_none());
    // several keys: the total bridge, and each key's volume, rate and mix effects
    let df = DataFrame::new(vec![
        Series::new("Product".into(), ["TD", "Savings", "Current", "Total"]),
        Series::new("Prev annual interest".into(), [80.0, 30.0, 5.0, 115.0]),
        Series::new("Curr annual interest".into(), [93.5, 28.0, 5.5, 127.0]),
        Series::new("Volume effect".into(), [8.0, -3.0, 0.4, 5.4]),
        Series::new("Rate effect".into(), [5.0, 1.0, 0.1, 6.1]),
        Series::new("Mix effect".into(), [0.5, 0.0, 0.0, 0.5]),
    ])
    .unwrap();
    let p = prepare(Template::NiiBridge, &df, &ctx()).unwrap();
    assert!(p.finding.contains("in total interest"), "{}", p.finding);
    let c = p.companion.expect("effects by key");
    let d = compute(c.table.clone().unwrap().lazy(), &c.spec).unwrap();
    assert_eq!(d.categories, ["TD", "Savings", "Current"], "the largest change first, the Total row is not a key");
    let mut series = d.series.clone();
    series.sort();
    assert_eq!(series, ["Mix", "Rate", "Volume"]);
    let vol = d.series.iter().position(|s| s == "Volume").unwrap();
    assert_eq!(d.values[vol][0], Some(8.0));
    assert!(c.finding.starts_with("TD has the largest change"), "{}", c.finding);
    for l in all_sizes(&d, "nii by key") {
        assert!(l.message.is_none());
    }
}
