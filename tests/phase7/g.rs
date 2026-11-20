//! Session D: the owner's fixes after the second review. The NPL % panel is one series, the NII effects by key
//! mark each key's net, the HQLA segments are labelled beside the bar, and the NII bridge names its product.

use crate::p7_c1::all_sizes;
use crate::p7_c3::opts as html_opts;
use crate::p7_c5::chart_of;
use micro_parquet::charts::layout::{MarkKind, Prim, Role};
use micro_parquet::charts::echarts::chart_pair;
use micro_parquet::charts::samples as sm;
use micro_parquet::charts::templates::Template;
use micro_parquet::charts::tokens as t;
use micro_parquet::charts::xlsx::write_chart_xlsx;
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mp-g-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

fn python() -> bool {
    std::process::Command::new("python3").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// The text of every file of a workbook's charts, joined.
fn chart_xml(path: &PathBuf, n: usize) -> String {
    crate::common::unzip_entry(path, &format!("xl/charts/chart{n}.xml"))
}

#[test]
fn g1_the_npl_panel_of_the_classified_trend_is_one_series_in_the_first_colour_named_at_its_end() {
    let (p, d) = chart_of(Template::ClassifiedTrend, &sm::classified());
    assert!(p.spec.panels[1].single && !p.spec.panels[0].single);
    for l in all_sizes(&d, "classified trend") {
        assert_eq!(l.panels.len(), 2);
        // the points of the NPL panel (panel 1) belong to one series
        let npl: Vec<_> = l.marks.iter().filter(|m| m.panel == 1 && m.kind == MarkKind::Point).collect();
        assert_eq!(npl.len(), 3, "one point a month");
        assert!(npl.iter().all(|m| m.series == 0), "exactly one series in the NPL panel");
        // the line is drawn in the first categorical colour
        let blue = if l.dark { t::SERIES_DARK[0] } else { t::SERIES_LIGHT[0] };
        let col = egui::Color32::from_rgb((blue >> 16) as u8, (blue >> 8) as u8, blue as u8);
        let panel = &l.panels[1];
        assert!(l.prims.iter().any(|pr| matches!(pr, Prim::Line { a, w, col: c, .. } if *w == t::LINE_PX && *c == col && a.y > panel.plot.top() - 1.0 && a.y < panel.plot.bottom() + 1.0)), "the NPL line is in the first colour");
        // named at its end, with its value
        assert!(l.texts.iter().any(|x| x.role == Role::Label && x.text == "NPL %  7.0%"), "{:?}", l.texts.iter().map(|x| x.text.clone()).collect::<Vec<_>>());
        // the grades are the legend; NPL % is not a legend entry
        let legend: Vec<&str> = l.texts.iter().filter(|x| x.role == Role::Legend).map(|x| x.text.as_str()).collect();
        assert_eq!(legend, ["SMA", "SS", "DF", "BL"], "{legend:?}");
    }
    // the HTML has one line series in the NPL panel (grid 1), in the first colour
    let pair = chart_pair(&d, &html_opts()).unwrap();
    let lines: Vec<_> = pair.light["series"].as_array().unwrap().iter().filter(|s| s["type"] == "line" && s["xAxisIndex"] == 1 && s["name"] != "Total").collect();
    assert_eq!(lines.len(), 1, "one series in the NPL panel");
    assert_eq!(lines[0]["lineStyle"]["color"], t::hex_text(t::SERIES_LIGHT[0]));
    assert_eq!(lines[0]["name"], "NPL %");
    // and one series in the Excel chart of the panel
    if python() {
        let path = tmp("classified.xlsx");
        write_chart_xlsx(&path, &d, &html_opts()).unwrap();
        assert_eq!(chart_xml(&path, 1).matches("<c:ser>").count(), 4, "the grades");
        assert_eq!(chart_xml(&path, 2).matches("<c:ser>").count(), 1, "the NPL % panel is one series");
    }
}

#[test]
fn g2_the_nii_effects_by_key_mark_each_keys_net_change_with_a_tick_and_a_signed_label() {
    let p = micro_parquet::charts::templates::prepare(Template::NiiBridge, &sm::mom_table_keys(), &micro_parquet::charts::templates::Ctx { divisor: 1.0, suffix: "Cr".into(), ..Default::default() }).unwrap();
    let c = p.companion.expect("effects by key");
    assert!(c.spec.net_marker);
    let d = micro_parquet::charts::calc::compute(polars::prelude::IntoLazy::lazy(c.table.clone().unwrap()), &c.spec).unwrap();
    assert_eq!(d.categories, ["TD", "Savings", "Current"]);
    // the nets on the guide example: TD 8 + 5 + 0.5, Savings −3 + 1 + 0, Current 0.4 + 0.1 + 0
    let want = ["+13.50", "\u{2212}2.00", "+0.50"];
    for l in all_sizes(&d, "nii effects") {
        assert!(l.message.is_none());
        let labels: Vec<&str> = l.texts.iter().filter(|x| x.role == Role::Label).map(|x| x.text.as_str()).collect();
        assert_eq!(labels.len(), 3, "only the nets are labelled, never the stack edges: {labels:?}");
        for w in want {
            assert!(labels.contains(&w), "{w} in {labels:?}");
        }
        // a dark tick across each stack at its net
        let panel = &l.panels[0];
        let ink = t::ink(l.dark).ink;
        let ink_c = egui::Color32::from_rgb((ink >> 16) as u8, (ink >> 8) as u8, ink as u8);
        for net in [13.5, -2.0, 0.5] {
            let y = panel.coord(net);
            assert!(l.prims.iter().any(|pr| matches!(pr, Prim::Line { a, b, w, col } if *w == t::LINE_PX && *col == ink_c && (a.y - y).abs() < 0.6 && (b.y - y).abs() < 0.6 && (b.x - a.x) > 20.0)), "a tick at {net}");
        }
    }
    // the HTML: a "Net" series of ticks with the values, and no "Total" label series
    let pair = chart_pair(&d, &html_opts()).unwrap();
    let series = pair.light["series"].as_array().unwrap();
    let net = series.iter().find(|s| s["name"] == "Net").expect("the net series");
    let vals: Vec<f64> = net["data"].as_array().unwrap().iter().map(|i| i["value"].as_f64().unwrap()).collect();
    assert_eq!(vals, [13.5, -2.0, 0.5]);
    let labels: Vec<&str> = net["data"].as_array().unwrap().iter().map(|i| i["label"]["formatter"].as_str().unwrap()).collect();
    assert_eq!(labels, want);
    assert!(!series.iter().any(|s| s["name"] == "Total"), "no total label on the stack's edge");
    // the Excel chart: a Net column and a series of dash markers
    if python() {
        let path = tmp("effects.xlsx");
        write_chart_xlsx(&path, &d, &html_opts()).unwrap();
        let xml = chart_xml(&path, 1);
        assert!(xml.contains(">Net<") && xml.contains("<c:lineChart>"), "a Net series");
        assert!(xml.contains("&quot;+&quot;#,##0.00") || xml.contains("\"+\"#,##0.00"), "signed labels");
    }
}

#[test]
fn g3_each_hqla_segment_is_labelled_beside_the_bar_with_its_value_and_share_and_the_labels_never_overlap() {
    let (p, _) = chart_of(Template::LcrNsfr, &sm::lcr_summary());
    let c = p.companion.expect("the HQLA composition");
    assert!(c.spec.segment_labels);
    let d = micro_parquet::charts::calc::compute(polars::prelude::IntoLazy::lazy(c.table.clone().unwrap()), &c.spec).unwrap();
    let want = ["3,500 \u{b7} 70.0%", "1,000 \u{b7} 20.0%", "500 \u{b7} 10.0%"];
    for l in all_sizes(&d, "hqla") {
        let bar = l.marks.iter().filter(|m| m.kind == MarkKind::Segment).map(|m| m.rect).fold(egui::Rect::NOTHING, |a, r| a.union(r));
        for w in want {
            let x = l.texts.iter().find(|x| x.text == w).unwrap_or_else(|| panic!("{w}: {:?}", l.texts.iter().map(|x| x.text.clone()).collect::<Vec<_>>()));
            assert!(x.rect.left() >= bar.right(), "{w} stands beside the bar: {:?} vs {:?}", x.rect, bar);
        }
        // check() (inside all_sizes) already pins that no two texts overlap and none leaves the chart
    }
    let pair = chart_pair(&d, &html_opts()).unwrap();
    let mut strings = Vec::new();
    fn walk(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::String(s) => out.push(s.clone()),
            serde_json::Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            serde_json::Value::Object(m) => m.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    walk(&pair.light, &mut strings);
    for w in want {
        assert!(strings.iter().any(|s| s == w), "the HTML says {w}");
    }
    if python() {
        let path = tmp("hqla.xlsx");
        write_chart_xlsx(&path, &d, &html_opts()).unwrap();
        let xml = chart_xml(&path, 1);
        for w in want {
            assert!(xml.contains(w), "the Excel label {w}");
        }
    }
}
