//! Renderer B of the chart studio (Phase 7 C3): an Apache ECharts option, as JSON, and the one
//! self-contained HTML file that shows it. The option is built from the same `ChartData` and the same
//! design tokens as the in-app painter, so the look is the same; it is plain `serde_json`, so the output
//! is deterministic and the tests compare it with golden files.
//!
//! Strings that start with "@" in the option ("@fmt:amount:2", "@tip", "@axistip", "@saveSvg") stand for
//! functions: JSON cannot hold a function, so the page's script swaps each for its implementation when it
//! loads. The advanced options a user types (merged over the option) may use the same names.
//!
//! The HTML never loads anything: ECharts itself is compiled in (`assets/vendor/echarts/`), and the page
//! has no `http`/`https` address of its own.

use super::axis::{padded_floor, diverging_text, nice_ticks, value_text, ValueKind};
use super::labels::LabelPlan;
use super::spec::LabelMode;
use super::colours::ColourMap;
use super::data::{collapse_series, heatmap, waterfall_from_data, waterfall_stack, ChartData, HeatFill, StepKind};
use super::layout::{base_word, chart_title, scaled, value_kind, LayoutOpts};
use super::spec::{ChartKind, ChartSpec, Stack, Transform};
use super::tokens as t;
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// The ECharts library, compiled in (Apache-2.0; see `THIRD_PARTY_NOTICES.md`).
pub const ECHARTS_JS: &str = include_str!("../../assets/vendor/echarts/echarts.min.js");
pub const ECHARTS_VERSION: &str = "6.1.0";

/// The reference of every option the advanced box can set (shown as a text, never opened by the program).
pub const ECHARTS_OPTION_REFERENCE: &str = "echarts.apache.org/en/option.html";

const MARKERS_MAX_POINTS: usize = 40;
const LABEL_MAX_BARS: usize = 12;
const PAD: f64 = 20.0;
const HEAD_H: f64 = 58.0;
const LEGEND_H: f64 = 26.0;
const SINGLE_H: f64 = 330.0;
const LINKED_H: f64 = 170.0;
const LINKED_GAP: f64 = 40.0;
const SMALL_H: f64 = 150.0;
const SMALL_GAP: f64 = 46.0;
const AXIS_H: f64 = 30.0;
const FOOT_LINE_H: f64 = 14.0;
const FOOT_CHARS: usize = 120;

/// What `chart_pair` makes: the option in both themes, the height the page gives it, and its table.
#[derive(Clone, Debug)]
pub struct ChartOut {
    pub title: String,
    pub light: Value,
    pub dark: Value,
    pub height: u32,
    /// The chart's numbers as an HTML table (the page's "Table" toggle).
    pub table: String,
    /// The table is shown open under the chart (the Data table option, Phase 8 W11), and the toggle leaves it alone.
    pub table_open: bool,
}

fn r6(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if r == 0.0 {
        0.0
    } else {
        r
    }
}

fn num(v: f64) -> Value {
    json!(r6(v))
}

fn hx(c: u32) -> String {
    t::hex_text(c)
}

/// Text for a literal ECharts label: the template syntax `{...}` is not meant here.
fn lit(s: &str) -> String {
    t::tidy(s).replace('{', "(").replace('}', ")")
}

fn fmt_tok(vk: ValueKind, opts: &LayoutOpts) -> String {
    match vk {
        ValueKind::Amount => format!("@fmt:amount:{}", opts.decimals),
        ValueKind::Percent => "@fmt:percent:1".to_string(),
        ValueKind::Count => "@fmt:count".to_string(),
        ValueKind::Plain => format!("@fmt:plain:{}", opts.decimals),
    }
}

// ----------------------------------------------------------------------------------------- the plan

struct Env<'a> {
    data: &'a ChartData,
    opts: &'a LayoutOpts,
    dark: bool,
    ink: t::ChartInk,
    cm: ColourMap,
    /// Which data labels, in which unit and decimals (the Data labels setting).
    lp: LabelPlan,
    /// Labels of "All" left out because they would overlap (the app's layout decides, at the page's size).
    dropped: std::collections::HashSet<(usize, usize, usize)>,
    /// The labels inside stacked segments (Phase 8 W11), as the app's layout decided them at a nominal size: a segment with no entry
    /// has no label. Empty with "Segment labels" off.
    segs: std::collections::HashMap<(usize, usize, usize), super::layout::SegmentLabel>,
}

struct SerD {
    name: String,
    vals: Vec<Option<f64>>,
    rows: Vec<u64>,
    colour: u32,
}

struct Plan<'a> {
    kind: ChartKind,
    title: String,
    measure: usize,
    vk: ValueKind,
    series: Vec<SerD>,
    cats: &'a [String],
    idx: usize,
}

impl<'a> Env<'a> {
    fn colour_of(&self, name: &str, i: usize, n: usize) -> u32 {
        let base = if name.is_empty() && n == 1 {
            super::colours::single_colour(self.data, &self.cm).hex(self.dark)
        } else if name.is_empty() {
            t::series_colour(i.min(t::SLOTS - 1), self.dark).unwrap_or(0)
        } else {
            match self.cm.get(name) {
                Some(c) => c.hex(self.dark),
                None => t::series_colour(i.min(t::SLOTS - 1), self.dark).unwrap_or(0),
            }
        };
        match &self.data.spec.highlight {
            Some(h) if !h.is_empty() && n > 1 && name != h => self.cm.dimmed(name, self.dark),
            _ => base,
        }
    }

    /// A value as a data label says it (the Data labels setting's unit and decimals).
    fn txt(&self, vk: ValueKind, v: f64, signed: bool) -> String {
        self.lp.text(v, vk, signed)
    }

    fn left_out(&self, panel: usize, series: usize, cat: usize) -> bool {
        self.dropped.contains(&(panel, series, cat))
    }

    fn tip(&self, p: &Plan, s: usize, c: usize, v: Option<f64>) -> String {
        let spec = &self.data.spec;
        let mut out = format!("{}: {}", if spec.category.is_empty() { "Category" } else { &spec.category }, p.cats.get(c).cloned().unwrap_or_default());
        if let Some(sr) = p.series.get(s) {
            if !sr.name.is_empty() {
                out.push_str(&format!("\n{}: {}", spec.series.clone().unwrap_or_else(|| "Series".to_string()), sr.name));
            }
        }
        let name = self.data.measure_names.get(p.measure).cloned().unwrap_or_default();
        let vt = match v {
            Some(x) => value_text(scaled(x, p.vk, self.opts), p.vk, self.opts.decimals + usize::from(p.vk == ValueKind::Percent), false),
            None => "no value".to_string(),
        };
        out.push_str(&format!("\n{}: {}", if name.is_empty() { "Value" } else { &name }, vt));
        if let Some(sr) = p.series.get(s) {
            out.push_str(&format!("\n{} rows", crate::models::format_with_commas(sr.rows.get(c).copied().unwrap_or(0) as f64, 0)));
        }
        if p.vk == ValueKind::Percent {
            let ml = spec.measure_list();
            let word = base_word(ml.get(p.measure).map(|m| m.transform).unwrap_or(Transform::None));
            if !word.is_empty() {
                out.push_str(&format!("\nPercent {}", word));
            }
        }
        lit_tip(&out)
    }

    /// A short line for the shared tooltip of a line or area ("Series: value").
    fn axis_tip(&self, p: &Plan, s: usize, c: usize, v: Option<f64>) -> String {
        let vt = match v {
            Some(x) => value_text(scaled(x, p.vk, self.opts), p.vk, self.opts.decimals + usize::from(p.vk == ValueKind::Percent), false),
            None => "no value".to_string(),
        };
        let rows = p.series.get(s).and_then(|x| x.rows.get(c)).copied().unwrap_or(0);
        lit_tip(&format!("{}: {} ({} rows)", p.series.get(s).map(|x| x.name.clone()).filter(|n| !n.is_empty()).or_else(|| self.data.measure_names.get(p.measure).cloned()).unwrap_or_else(|| "Value".to_string()), vt, crate::models::format_with_commas(rows as f64, 0)))
    }
}

fn lit_tip(s: &str) -> String {
    t::tidy(s)
}

fn plans<'a>(env: &Env<'a>) -> Vec<Plan<'a>> {
    let data = env.data;
    let spec = &data.spec;
    let kind = spec.kind;
    let n_series = data.series.len();
    let mk = |measure: usize| -> Vec<SerD> {
        let Some(md) = data.measure(measure) else { return Vec::new() };
        (0..n_series)
            .map(|i| {
                let name = data.series.get(i).cloned().unwrap_or_default();
                SerD { colour: env.colour_of(&name, i, n_series), name, vals: md.values.get(i).cloned().unwrap_or_default(), rows: md.rows.get(i).cloned().unwrap_or_default() }
            })
            .collect()
    };
    let mut out = Vec::new();
    if !data.facets.is_empty() {
        for (fi, f) in data.facets.iter().enumerate() {
            let fd = &f.data;
            let ns = fd.series.len();
            let series = (0..ns)
                .map(|i| SerD {
                    name: fd.series[i].clone(),
                    vals: fd.values.get(i).cloned().unwrap_or_default(),
                    rows: fd.rows.get(i).cloned().unwrap_or_default(),
                    colour: env.colour_of(&fd.series[i], data.series.iter().position(|x| *x == fd.series[i]).unwrap_or(i), n_series.max(ns)),
                })
                .collect();
            out.push(Plan { kind, title: f.name.clone(), measure: 0, vk: value_kind(spec, 0), series, cats: &fd.categories, idx: fi });
        }
    } else if !spec.panels.is_empty() {
        for (pi, p) in spec.panels.iter().take(t::MAX_PANELS).enumerate() {
            let mi = p.measures.first().copied().unwrap_or(0).min(data.measure_count().saturating_sub(1));
            let title = if p.title.is_empty() { data.measure_names.get(mi).cloned().unwrap_or_default() } else { p.title.clone() };
            let series = if p.single {
                // one series named after the measure, in the first categorical colour
                let md = data.measure(mi);
                let (vals, rows) = md.as_ref().map(|m| collapse_series(&m.values, &m.rows, data.categories.len())).unwrap_or_default();
                vec![SerD { name: data.measure_names.get(mi).cloned().unwrap_or_default(), vals, rows, colour: t::series_colour(0, env.dark).unwrap_or(0) }]
            } else {
                mk(mi)
            };
            out.push(Plan { kind: p.kind, title, measure: mi, vk: value_kind(spec, mi), series, cats: &data.categories, idx: pi });
        }
    } else if data.measure_count() > 1 && !matches!(kind, ChartKind::Waterfall | ChartKind::Histogram | ChartKind::BoxPlot | ChartKind::Scatter | ChartKind::Bubble) {
        for mi in 0..data.measure_count().min(t::MAX_PANELS) {
            out.push(Plan { kind, title: data.measure_names.get(mi).cloned().unwrap_or_default(), measure: mi, vk: value_kind(spec, mi), series: mk(mi), cats: &data.categories, idx: mi });
        }
    } else {
        out.push(Plan { kind, title: String::new(), measure: 0, vk: value_kind(spec, 0), series: mk(0), cats: &data.categories, idx: 0 });
    }
    out
}

// ----------------------------------------------------------------------------------- shared pieces

fn font_style(size: f64, weight: &str, colour: u32) -> Value {
    json!({"fontFamily": t::FONT_HTML, "fontSize": size, "fontWeight": weight, "color": hx(colour)})
}

fn subtitle_of(data: &ChartData, opts: &LayoutOpts) -> String {
    super::layout::subtitle_text(data, opts)
}

/// The width and height the labels' collisions are worked out at (a page chart is about this big).
const PAGE_CHART_W: f32 = 1060.0;
const PAGE_CHART_H: f32 = 480.0;

fn foot_lines(data: &ChartData, notes: &[String]) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    if !data.spec.footnote.trim().is_empty() {
        parts.push(data.spec.footnote.trim().to_string());
    }
    if !data.spec.limit_note.trim().is_empty() {
        parts.push(format!("\u{2014} {}", data.spec.limit_note.trim()));
    }
    parts.extend(data.notes.iter().cloned());
    parts.extend(super::colours::colour_notes(data, &data.spec.colour_map));
    parts.extend(notes.iter().cloned());
    if parts.is_empty() {
        return Vec::new();
    }
    let text = t::tidy(&parts.join(" \u{b7} "));
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for w in text.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + w.chars().count() > FOOT_CHARS {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(w);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines.truncate(4);
    lines
}

fn base_option(env: &Env, title: &str, legend: Option<Vec<(String, u32)>>, foot: &[String], height: f64) -> Map<String, Value> {
    let ink = env.ink;
    let mut o = Map::new();
    o.insert("animation".into(), json!(false));
    o.insert("backgroundColor".into(), json!(hx(ink.surface)));
    o.insert("textStyle".into(), json!({"fontFamily": t::FONT_HTML, "color": hx(ink.ink2)}));
    let mut colors: Vec<String> = (0..t::SLOTS).filter_map(|i| t::series_colour(i, env.dark)).map(hx).collect();
    colors.truncate(t::SLOTS);
    o.insert("color".into(), json!(colors));
    let sub = subtitle_of(env.data, env.opts);
    o.insert(
        "title".into(),
        json!([{
            "text": lit(title), "subtext": lit(&sub), "left": PAD, "top": 10, "itemGap": 4,
            "textStyle": font_style(t::TITLE_PT as f64, "600", ink.ink), "subtextStyle": font_style(t::SUBTITLE_PT as f64, "400", ink.ink2)
        }]),
    );
    if let Some(items) = legend {
        o.insert(
            "legend".into(),
            json!({
                "data": items.iter().map(|(n, c)| json!({"name": lit(n), "itemStyle": {"color": hx(*c)}})).collect::<Vec<_>>(),
                "left": PAD, "top": HEAD_H, "icon": "roundRect", "itemWidth": 10, "itemHeight": 10, "itemGap": 18,
                "textStyle": font_style(t::label_pt() as f64, "400", ink.ink2)
            }),
        );
    }
    if !foot.is_empty() {
        let n = foot.len() as f64;
        let elems: Vec<Value> = foot
            .iter()
            .enumerate()
            .map(|(i, l)| {
                json!({"type": "text", "left": PAD, "top": height - 10.0 - FOOT_LINE_H * (n - i as f64), "silent": true,
                       "style": {"text": l, "fill": hx(ink.muted), "fontFamily": t::FONT_HTML, "fontSize": t::FOOTNOTE_PT, "fontWeight": "400"}})
            })
            .collect();
        o.insert("graphic".into(), json!(elems));
    }
    o.insert(
        "tooltip".into(),
        json!({
            "trigger": "item", "formatter": "@tip", "confine": true, "padding": [6, 8],
            "backgroundColor": hx(ink.surface), "borderColor": hx(ink.baseline), "borderWidth": 1,
            "textStyle": font_style(t::label_pt() as f64, "400", ink.ink), "extraCssText": "box-shadow:none;border-radius:4px;"
        }),
    );
    o.insert(
        "toolbox".into(),
        json!({
            "right": 14, "top": 8, "itemSize": 14,
            "iconStyle": {"borderColor": hx(ink.muted)},
            "emphasis": {"iconStyle": {"borderColor": hx(ink.ink)}},
            "feature": {
                "saveAsImage": {"title": "Save as PNG", "type": "png", "pixelRatio": 3, "backgroundColor": hx(ink.surface), "name": "chart"},
                "mySvg": {"title": "Save as SVG", "icon": "path://M4 3h12l4 4v14H4z M8 3v5h8", "onclick": "@saveSvg"},
                "dataView": {"title": "Data view", "readOnly": true, "lang": ["Data view", "Close", "Refresh"], "backgroundColor": hx(ink.surface), "textColor": hx(ink.ink), "textareaColor": hx(ink.surface), "textareaBorderColor": hx(ink.baseline)}
            }
        }),
    );
    o
}

fn category_axis(env: &Env, grid: usize, cats: &[String], horizontal: bool, show_labels: bool, gap: bool) -> Value {
    let ink = env.ink;
    let mut label = json!({"show": show_labels, "color": hx(ink.ink2), "fontFamily": t::FONT_HTML, "fontSize": t::tick_pt(), "margin": 8,
                           // every category keeps its label: none is skipped (interval 0), none is overlapped away
                           "interval": 0, "hideOverlap": false});
    if show_labels && !cats.is_empty() {
        if horizontal {
            label["overflow"] = json!("break");
            label["width"] = json!((PAGE_CHART_W * 0.34).round());
        } else {
            // the same decision as the app, at the width of a page chart: full, else two lines, else angled 35 degrees
            let measure = super::layout::ApproxMeasure;
            let wf = |s: &str| -> f32 { <super::layout::ApproxMeasure as super::layout::Measure>::size(&measure, s, t::tick_pt(), false).x };
            let band = (PAGE_CHART_W - 120.0) / cats.len() as f32;
            let pl = super::catlabels::plan(cats, band, &wf, t::tick_pt() * 1.3);
            match pl.fit {
                super::catlabels::CatFit::Full => {}
                super::catlabels::CatFit::Wrapped => {
                    label["overflow"] = json!("break");
                    label["width"] = json!((band - 4.0).max(10.0).round());
                    label["lineHeight"] = json!((t::tick_pt() * 1.3).round());
                }
                super::catlabels::CatFit::Angled => {
                    label["rotate"] = json!(super::catlabels::ANGLE_DEG);
                }
            }
        }
    }
    json!({
        "type": "category", "gridIndex": grid, "data": cats.iter().map(|c| lit(c)).collect::<Vec<_>>(),
        "inverse": horizontal, "position": if horizontal { "left" } else { "bottom" },
        "boundaryGap": gap,
        "axisLine": {"show": true, "lineStyle": {"color": hx(ink.baseline), "width": 1}},
        "axisTick": {"show": false}, "splitLine": {"show": false},
        "axisLabel": label
    })
}

fn value_axis(env: &Env, grid: usize, lo: f64, hi: f64, vk: ValueKind, horizontal: bool, show_labels: bool, include_zero: bool, grid_lines: bool) -> Value {
    let ink = env.ink;
    let (lo, hi) = (scaled(lo, vk, env.opts), scaled(hi, vk, env.opts));
    let (mut a, mut b) = (lo, hi);
    if include_zero {
        a = a.min(0.0);
        b = b.max(0.0);
    }
    if env.data.spec.pad_axis_below && !include_zero {
        a = padded_floor(a, b, 6);
    }
    let (ticks, step) = nice_ticks(a, b, 6);
    json!({
        "type": "value", "gridIndex": grid, "min": num(*ticks.first().unwrap_or(&0.0)), "max": num(*ticks.last().unwrap_or(&1.0)), "interval": num(step),
        "position": if horizontal { "bottom" } else { "left" },
        "axisLine": {"show": false}, "axisTick": {"show": false},
        "splitLine": {"show": grid_lines, "lineStyle": {"color": hx(ink.grid), "width": 1, "type": "solid"}},
        "axisLabel": {"show": show_labels, "color": hx(ink.muted), "fontFamily": t::FONT_HTML, "fontSize": t::tick_pt(), "margin": 8,
                      "formatter": if vk == ValueKind::Percent { "@fmt:axis:percent" } else { "@fmt:axis:plain" }}
    })
}

fn extent(p: &Plan, stack: Stack) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let stacked = matches!(p.kind, ChartKind::Bar | ChartKind::Area) && matches!(stack, Stack::Stacked | Stack::Percent);
    if stacked {
        if stack == Stack::Percent {
            return (0.0, 100.0);
        }
        for c in 0..p.cats.len() {
            let (mut pos, mut neg) = (0.0, 0.0);
            for s in &p.series {
                if let Some(Some(v)) = s.vals.get(c) {
                    if *v >= 0.0 {
                        pos += v;
                    } else {
                        neg += v;
                    }
                }
            }
            hi = hi.max(pos);
            lo = lo.min(neg);
        }
    } else {
        for s in &p.series {
            for v in s.vals.iter().flatten() {
                lo = lo.min(*v);
                hi = hi.max(*v);
            }
        }
    }
    if lo.is_finite() && hi.is_finite() {
        (lo, hi)
    } else {
        (0.0, 1.0)
    }
}

fn mark_lines(env: &Env, p: &Plan, horizontal: bool) -> (Option<Value>, Option<Value>) {
    let spec = &env.data.spec;
    let key = if horizontal { "xAxis" } else { "yAxis" };
    let lines: Vec<Value> = spec
        .ref_lines
        .iter()
        .filter(|r| r.panel == p.idx.min(if env.data.facets.is_empty() { usize::MAX } else { 0 }))
        .map(|r| {
            json!({key: num(scaled(r.value, p.vk, env.opts)), "label": {"show": true, "formatter": lit(&r.label), "position": "insideEndTop", "color": hx(env.ink.ink), "fontFamily": t::FONT_HTML, "fontSize": t::label_pt()},
                   "lineStyle": {"color": hx(r.status.hex()), "width": t::REFERENCE_LINE_PX, "type": "solid"}})
        })
        .collect();
    let bands: Vec<Value> = spec
        .ref_bands
        .iter()
        .filter(|r| r.panel == p.idx.min(if env.data.facets.is_empty() { usize::MAX } else { 0 }))
        .map(|r| {
            json!([{key: num(scaled(r.from, p.vk, env.opts)), "name": lit(&r.label), "label": {"show": !r.label.is_empty(), "formatter": lit(&r.label), "color": hx(env.ink.ink2)}, "itemStyle": {"color": hx(r.status.map(|st| st.hex()).unwrap_or(env.ink.deemph)), "opacity": if r.status.is_some() { 0.16 } else { 0.35 }}}, {key: num(scaled(r.to, p.vk, env.opts))}])
        })
        .collect();
    let ml = (!lines.is_empty()).then(|| json!({"silent": true, "symbol": "none", "data": lines}));
    let ma = (!bands.is_empty()).then(|| json!({"silent": true, "data": bands}));
    (ml, ma)
}

// ------------------------------------------------------------------------------- cartesian forms

/// The rounded data-end of a bar: [top-left, top-right, bottom-right, bottom-left] in ECharts order.
fn radius(positive: bool, horizontal: bool) -> Value {
    let r = t::BAR_RADIUS_PX as f64;
    match (horizontal, positive) {
        (false, true) => json!([r, r, 0, 0]),
        (false, false) => json!([0, 0, r, r]),
        (true, true) => json!([0, r, r, 0]),
        (true, false) => json!([r, 0, 0, r]),
    }
}

/// White on a dark fill, the ink on a pale one.
fn text_on(fill: u32) -> String {
    let lin = |c: u32| {
        let v = (c & 0xff) as f64 / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let l = 0.2126 * lin(fill >> 16) + 0.7152 * lin(fill >> 8) + 0.0722 * lin(fill);
    if l > 0.30 { hx(t::INK_LIGHT.ink) } else { "#ffffff".to_string() }
}

/// The size the page's segment labels are decided at (the book's half-width card; the one-chart page is larger).
const SEGMENT_NOMINAL_W: f32 = 640.0;
const SEGMENT_NOMINAL_H: f32 = 420.0;

fn label_style(env: &Env, text: &str, position: &str) -> Value {
    json!({"show": true, "formatter": lit(text), "position": position, "color": hx(env.ink.ink), "fontFamily": t::FONT_HTML, "fontSize": t::data_label_pt()})
}

fn bar_series(env: &Env, p: &Plan, grid: usize, horizontal: bool) -> Vec<Value> {
    let spec = &env.data.spec;
    let stacked = matches!(spec.stack, Stack::Stacked | Stack::Percent) && p.series.len() > 1;
    let percent = spec.stack == Stack::Percent && p.series.len() > 1;
    let n_cat = p.cats.len();
    let surface = hx(env.ink.surface);
    // the topmost positive and the lowest negative segment of each category carry the rounded end
    let mut top_pos: Vec<Option<usize>> = vec![None; n_cat];
    let mut low_neg: Vec<Option<usize>> = vec![None; n_cat];
    for c in 0..n_cat {
        for (si, s) in p.series.iter().enumerate() {
            match s.vals.get(c).copied().flatten() {
                Some(v) if v > 0.0 => top_pos[c] = Some(si),
                Some(v) if v < 0.0 => low_neg[c] = Some(si),
                _ => {}
            }
        }
    }
    let totals: Vec<f64> = (0..n_cat).map(|c| p.series.iter().filter_map(|s| s.vals.get(c).copied().flatten()).sum()).collect();
    let mode = env.lp.mode;
    let labels_on = env.lp.kind_on(p.vk);
    // Auto: one series of 12 bars or fewer; All and Ends only: every series; None: no value labels at all
    let label_all = labels_on
        && match mode {
            LabelMode::Auto => p.series.len() == 1 && n_cat <= LABEL_MAX_BARS,
            LabelMode::None => false,
            _ => true,
        };
    // one series with negative values is a diverging bar: its labels always carry a sign
    let diverging = p.series.len() == 1 && p.series[0].vals.iter().flatten().any(|v| *v < 0.0);
    let mut out = Vec::new();
    for (si, s) in p.series.iter().enumerate() {
        let data: Vec<Value> = (0..n_cat)
            .map(|c| {
                let v = s.vals.get(c).copied().flatten();
                let Some(raw) = v else { return Value::Null };
                let shown = if percent { if totals[c] == 0.0 { 0.0 } else { raw / totals[c] * 100.0 } } else { scaled(raw, p.vk, env.opts) };
                let rounded = if !stacked { true } else { top_pos[c] == Some(si) || low_neg[c] == Some(si) };
                let mut item = json!({"value": num(shown), "tip": env.tip(p, si, c, v)});
                if rounded {
                    item["itemStyle"] = json!({"borderRadius": radius(raw >= 0.0, horizontal)});
                } else {
                    item["itemStyle"] = json!({"borderRadius": 0});
                }
                let status = spec.bar_status.iter().find(|x| p.series.len() == 1 && x.category == p.cats.get(c).cloned().unwrap_or_default());
                if let Some(st) = status {
                    item["itemStyle"]["color"] = json!(hx(st.status.hex()));
                } else if diverging {
                    // a signed bar: blue above zero, red below, grey at exactly zero (as in the app)
                    item["itemStyle"]["color"] = json!(hx(t::diverging_colour(raw)));
                }
                if spec.segment_labels && stacked && !horizontal && !percent && totals[c] != 0.0 && labels_on {
                    // each segment beside its stack with its value and its share of the stack
                    let x = scaled(raw, p.vk, env.opts);
                    let body = if (x - x.round()).abs() < 1e-9 { value_text(x, p.vk, 0, false) } else { value_text(x, p.vk, env.opts.decimals, false) };
                    item["label"] = label_style(env, &format!("{} \u{b7} {}%", body, crate::models::format_with_commas(raw.abs() / totals[c].abs() * 100.0, 1)), "right");
                }
                // Phase 8 W11: a label inside the segment, straight or turned, in white or ink; none where it has no room
                if spec.inside_labels && stacked {
                    match env.segs.get(&(p.idx, si, c)) {
                        Some(sl) => {
                            item["label"] = label_style(env, &sl.text, "inside");
                            item["label"]["color"] = json!(hx(((sl.colour.r() as u32) << 16) | ((sl.colour.g() as u32) << 8) | sl.colour.b() as u32));
                            if sl.turned {
                                item["label"]["rotate"] = json!(90);
                            }
                        }
                        None => item["label"] = json!({"show": false}),
                    }
                }
                if label_all && !(spec.inside_labels && stacked) && env.lp.shows(c, n_cat, true) && !env.left_out(p.idx, si, c) {
                    let pos = match (horizontal, raw >= 0.0) {
                        (false, true) => "top",
                        (false, false) => "bottom",
                        (true, true) => "right",
                        (true, false) => "left",
                    };
                    let mut text = if diverging { env.lp.diverging(raw, p.vk) } else { env.txt(p.vk, raw, false) };
                    if let Some(st) = status {
                        // the status in words with an icon, after the value: the colour is never alone
                        text = format!("{}  \u{25b2} {}", text, st.label);
                    }
                    if stacked {
                        // "All" on a stack: each segment says its value inside it, in the colour its fill asks for
                        // (with segment labels on, those say it)
                        if !spec.segment_labels && !(spec.inside_labels && stacked) {
                            item["label"] = label_style(env, &text, "inside");
                            item["label"]["color"] = json!(text_on(s.colour));
                        }
                    } else {
                        item["label"] = label_style(env, &text, pos);
                    }
                }
                item
            })
            .collect();
        let mut ser = json!({
            "name": lit(&s.name), "type": "bar", "xAxisIndex": grid, "yAxisIndex": grid, "data": data, "barMaxWidth": if spec.inside_labels && stacked { t::BAR_MAX_PX_LABELLED } else { t::BAR_MAX_PX },
            "itemStyle": {"color": hx(s.colour), "borderColor": surface, "borderWidth": if stacked || p.series.len() > 1 { 1 } else { 0 }},
            "emphasis": {"focus": "none", "itemStyle": {"borderColor": hx(env.ink.ink), "borderWidth": 1}}
        });
        if horizontal {
            ser["xAxisIndex"] = json!(grid);
        }
        if stacked {
            ser["stack"] = json!(format!("total{}", grid));
            if spec.segment_labels {
                ser["labelLayout"] = json!({"moveOverlap": "shiftY"});
            }
        }
        if env.lp.mode == LabelMode::All || (spec.inside_labels && stacked) {
            // "All" never prints a label that collides: ECharts hides the ones that would overlap. The same goes for the
            // labels inside segments (W11): they are decided at a nominal size, and on a smaller page the ones that collide drop out
            ser["labelLayout"]["hideOverlap"] = json!(true);
        }
        if si == 0 {
            let (ml, ma) = mark_lines(env, p, horizontal);
            if let Some(ml) = ml {
                ser["markLine"] = ml;
            }
            if let Some(ma) = ma {
                ser["markArea"] = ma;
            }
        }
        out.push(ser);
    }
    // the net of each stack: a short dark tick across it and its signed value beside it; the stack edges carry no label
    if stacked && spec.net_marker && !horizontal && !percent {
        let data: Vec<Value> = (0..n_cat)
            .map(|c| {
                let raw: f64 = totals[c];
                let mut item = json!({"value": num(scaled(raw, p.vk, env.opts)), "tip": lit_tip(&format!("Net: {}", diverging_text(scaled(raw, p.vk, env.opts), p.vk, env.opts.decimals)))});
                if labels_on && env.lp.shows(c, n_cat, true) {
                    item["label"] = label_style(env, &env.lp.diverging(raw, p.vk), "right");
                }
                item
            })
            .collect();
        out.push(json!({"name": "Net", "type": "line", "xAxisIndex": grid, "yAxisIndex": grid, "data": data, "symbol": "rect", "symbolSize": [t::BAR_MAX_PX + 8.0, t::LINE_PX],
                        "itemStyle": {"color": hx(env.ink.ink), "borderWidth": 0}, "lineStyle": {"opacity": 0}, "tooltip": {"show": false}, "silent": true, "z": 6}));
        return out;
    }
    // the total on top of a stacked column, and only there
    if stacked {
        let data: Vec<Value> = (0..n_cat)
            .map(|c| {
                let raw: f64 = totals[c];
                let shown = if percent { 100.0 } else { scaled(raw, p.vk, env.opts) };
                let mut item = json!({"value": num(shown)});
                if labels_on && env.lp.shows(c, n_cat, true) && mode != LabelMode::All || labels_on && mode == LabelMode::All && !env.left_out(p.idx, 0, c) {
                    item["label"] = label_style(env, &env.txt(p.vk, raw, false), if horizontal { "right" } else { "top" });
                }
                item
            })
            .collect();
        out.push(json!({"name": "Total", "type": "line", "xAxisIndex": grid, "yAxisIndex": grid, "data": data, "symbol": "circle", "symbolSize": 0, "lineStyle": {"opacity": 0}, "silent": true, "tooltip": {"show": false}, "z": 5}));
    }
    out
}

fn line_series(env: &Env, p: &Plan, grid: usize) -> Vec<Value> {
    let spec = &env.data.spec;
    let area = p.kind == ChartKind::Area;
    let stacked = area && matches!(spec.stack, Stack::Stacked | Stack::Percent) && p.series.len() > 1;
    let percent = spec.stack == Stack::Percent && p.series.len() > 1;
    let n_cat = p.cats.len();
    let markers = n_cat <= MARKERS_MAX_POINTS;
    let highlighted = spec.highlight.as_ref().filter(|h| !h.is_empty()).cloned();
    let direct = (p.series.len() <= 4 || (highlighted.is_some() && p.series.len() > 1)) && !stacked;
    let surface = hx(env.ink.surface);
    let totals: Vec<f64> = (0..n_cat).map(|c| p.series.iter().filter_map(|s| s.vals.get(c).copied().flatten()).sum()).collect();
    let mut out = Vec::new();
    for (si, s) in p.series.iter().enumerate() {
        let last = (0..n_cat).rev().find(|c| s.vals.get(*c).copied().flatten().is_some());
        let first = (0..n_cat).find(|c| s.vals.get(*c).copied().flatten().is_some());
        // with a highlighted series only that one carries markers (older curves are plain lines)
        let markers = markers && highlighted.as_ref().map_or(true, |h| *h == s.name);
        let data: Vec<Value> = (0..n_cat)
            .map(|c| {
                let v = s.vals.get(c).copied().flatten();
                let Some(raw) = v else { return Value::Null };
                let shown = if percent { if totals[c] == 0.0 { 0.0 } else { raw / totals[c] * 100.0 } } else { scaled(raw, p.vk, env.opts) };
                let mut item = json!({"value": num(shown), "tip": env.axis_tip(p, si, c, v)});
                let value_on = env.lp.kind_on(p.vk);
                // the Data labels setting: All = every point, Ends only = the first and the last point, None = no values
                let point_value = !stacked
                    && value_on
                    && match env.lp.mode {
                        LabelMode::All => !env.left_out(p.idx, si, c),
                        LabelMode::EndsOnly => Some(c) == first || Some(c) == last,
                        _ => false,
                    };
                if Some(c) == last && !stacked {
                    // as in the app: one series says its value (with its name when it has one); two to four say their names
                    // (every curve when one is highlighted); more than that have the legend only
                    let single_value = value_on && env.lp.shows(c, n_cat, true);
                    let text = if p.series.len() == 1 {
                        match (s.name.is_empty(), single_value) {
                            (true, true) => Some(env.txt(p.vk, raw, false)),
                            (false, true) => Some(format!("{}  {}", s.name, env.txt(p.vk, raw, false))),
                            (false, false) => Some(s.name.clone()),
                            (true, false) => None,
                        }
                    } else if direct {
                        Some(s.name.clone())
                    } else {
                        None
                    };
                    if let Some(text) = text {
                        let emphasised = highlighted.as_ref().map_or(true, |h| *h == s.name);
                        item["label"] = label_style(env, &text, "right");
                        if p.series.len() > 1 && !emphasised {
                            item["label"]["color"] = json!(hx(env.ink.ink2));
                        }
                    }
                    if !markers {
                        // a line without markers still needs a (tiny, invisible) symbol to carry its end label
                        item["symbol"] = json!("circle");
                        item["symbolSize"] = json!(0);
                    }
                } else if point_value {
                    item["label"] = label_style(env, &env.txt(p.vk, raw, false), "top");
                    if !markers {
                        item["symbol"] = json!("circle");
                        item["symbolSize"] = json!(0);
                    }
                }
                item
            })
            .collect();
        let mut ser = json!({
            "name": lit(&s.name), "type": "line", "xAxisIndex": grid, "yAxisIndex": grid, "data": data, "smooth": false,
            "symbol": if markers { "circle" } else { "none" }, "symbolSize": t::MARKER_PX,
            "lineStyle": {"width": t::LINE_PX, "color": hx(s.colour), "cap": "round", "join": "round"},
            "itemStyle": {"color": hx(s.colour), "borderColor": surface, "borderWidth": t::MARKER_RING_PX},
            "emphasis": {"focus": "none"}, "connectNulls": false
        });
        if spec.stepped {
            ser["step"] = json!("end");
        }
        if direct && p.series.len() > 1 {
            // end labels of nearby curves are nudged apart vertically
            ser["labelLayout"] = json!({"moveOverlap": "shiftY"});
        }
        if env.lp.mode == LabelMode::All {
            ser["labelLayout"]["hideOverlap"] = json!(true);
        }
        if area {
            ser["areaStyle"] = json!({"color": hx(s.colour), "opacity": num(if stacked { t::STACKED_AREA_OPACITY } else { t::AREA_OPACITY } as f64)});
        }
        if stacked {
            ser["stack"] = json!(format!("total{}", grid));
            // no point markers on the layers of a stacked area: a point shows on hover only
            ser["showSymbol"] = json!(false);
        }
        if si == 0 {
            let (ml, ma) = mark_lines(env, p, false);
            if let Some(ml) = ml {
                ser["markLine"] = ml;
            }
            if let Some(ma) = ma {
                ser["markArea"] = ma;
            }
        }
        out.push(ser);
    }
    out
}

/// Per-category limits: a short tick in the status colour at each limit, and a ring with the text on a breach.
fn tick_series(env: &Env, p: &Plan, grid: usize, horizontal: bool) -> Vec<Value> {
    let spec = &env.data.spec;
    let panel = if env.data.facets.is_empty() { p.idx } else { 0 };
    let ticks: Vec<_> = spec.limit_ticks.iter().filter(|l| l.panel == panel).collect();
    if ticks.is_empty() {
        return Vec::new();
    }
    let at = |cat: &str, v: f64| -> Value {
        if horizontal {
            json!([num(v), lit(cat)])
        } else {
            json!([lit(cat), num(v)])
        }
    };
    let mut out = Vec::new();
    let tick_items: Vec<Value> = ticks
        .iter()
        .map(|l| json!({"value": at(&l.category, scaled(l.value, p.vk, env.opts)), "itemStyle": {"color": hx(l.status.hex())}}))
        .collect();
    let size = if horizontal { json!([t::REFERENCE_LINE_PX, 34]) } else { json!([44, t::REFERENCE_LINE_PX]) };
    out.push(json!({"name": "Limit", "type": "scatter", "xAxisIndex": grid, "yAxisIndex": grid, "data": tick_items, "symbol": "rect", "symbolSize": size, "silent": true, "tooltip": {"show": false}, "z": 6}));
    let rings: Vec<Value> = ticks
        .iter()
        .filter(|l| !l.breach.is_empty())
        .filter_map(|l| {
            let ci = p.cats.iter().position(|c| *c == l.category)?;
            let v = p.series.first()?.vals.get(ci).copied().flatten()?;
            Some(json!({"value": at(&l.category, scaled(v, p.vk, env.opts)), "label": label_style(env, &l.breach, if horizontal { "right" } else { "top" }),
                        "itemStyle": {"color": "rgba(0,0,0,0)", "borderColor": hx(l.status.hex()), "borderWidth": t::REFERENCE_LINE_PX}}))
        })
        .collect();
    if !rings.is_empty() {
        out.push(json!({"name": "Breach", "type": "scatter", "xAxisIndex": grid, "yAxisIndex": grid, "data": rings, "symbol": "circle", "symbolSize": t::MARKER_PX + 10.0, "silent": true, "tooltip": {"show": false}, "z": 7}));
    }
    out
}

/// Phase 13: a numbered marker on each noted category of the first panel, at the height of the first series' mark there; its text is
/// the chart's footnote.
fn note_series(env: &Env, p: &Plan, grid: usize, horizontal: bool) -> Vec<Value> {
    let spec = &env.data.spec;
    if p.idx != 0 || spec.annotations.is_empty() {
        return Vec::new();
    }
    let at = |cat: &str, v: f64| -> Value {
        if horizontal {
            json!([num(v), lit(cat)])
        } else {
            json!([lit(cat), num(v)])
        }
    };
    let items: Vec<Value> = spec
        .annotations
        .iter()
        .enumerate()
        .filter(|(_, a)| !a.text.trim().is_empty())
        .filter_map(|(n, a)| {
            let ci = p.cats.iter().position(|c| *c == a.category)?;
            let v = p.series.iter().filter_map(|s| s.vals.get(ci).copied().flatten()).fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))))?;
            Some(json!({"value": at(&a.category, scaled(v, p.vk, env.opts)), "label": {"show": true, "formatter": format!("{}", n + 1), "color": "#ffffff", "fontSize": 11, "fontWeight": "600"},
                        "tip": lit_tip(&format!("Note {}: {}", n + 1, a.text.trim()))}))
        })
        .collect();
    if items.is_empty() {
        return Vec::new();
    }
    vec![json!({"name": "Notes", "type": "scatter", "xAxisIndex": grid, "yAxisIndex": grid, "data": items, "symbol": "circle", "symbolSize": 18, "itemStyle": {"color": hx(env.ink.ink)}, "tooltip": {"show": true}, "z": 8})]
}

/// The panels of a cartesian chart (bars, lines, areas, histogram): grids, axes and series.
struct Grids {
    grid: Vec<Value>,
    x: Vec<Value>,
    y: Vec<Value>,
    series: Vec<Value>,
    titles: Vec<Value>,
    height: f64,
}

fn cartesian(env: &Env, plans: &[Plan], legend_h: f64, foot_h: f64) -> Grids {
    let spec = &env.data.spec;
    let multi = !env.data.facets.is_empty();
    let n = plans.len().max(1);
    let horizontal = spec.horizontal && plans.first().map(|p| p.kind == ChartKind::Bar).unwrap_or(false);
    let top0 = HEAD_H + legend_h + 6.0;
    let mut g = Grids { grid: Vec::new(), x: Vec::new(), y: Vec::new(), series: Vec::new(), titles: Vec::new(), height: 0.0 };
    // a shared value scale across small multiples
    let shared = multi.then(|| {
        plans.iter().map(|p| extent(p, spec.stack)).fold((f64::INFINITY, f64::NEG_INFINITY), |a, b| (a.0.min(b.0), a.1.max(b.1)))
    });
    let cols = if multi { n.clamp(1, 4).min(if n <= 4 { n } else { 4 }) } else { 1 };
    let rows = if multi { n.div_ceil(cols) } else { n };
    let (ph, gap) = if multi { (SMALL_H, SMALL_GAP) } else if n > 1 { (LINKED_H, LINKED_GAP) } else { (SINGLE_H, 0.0) };
    let end_reserve = if plans.iter().any(|p| p.kind == ChartKind::Line && p.series.len() <= 4 && !p.series.is_empty()) {
        let longest = plans.iter().flat_map(|p| p.series.iter().map(|s| s.name.chars().count())).max().unwrap_or(0) + 10;
        (longest as f64 * 6.2).clamp(60.0, 190.0)
    } else if horizontal {
        50.0
    } else {
        24.0
    };
    let any_cat_height = plans.iter().map(|p| p.cats.iter().map(|c| c.chars().count()).max().unwrap_or(0)).max().unwrap_or(0);
    let _ = any_cat_height;
    for (i, p) in plans.iter().enumerate() {
        let (r, c) = (i / cols, i % cols);
        let top = top0 + r as f64 * (ph + gap) + if n > 1 || multi { 22.0 } else { 0.0 };
        let last_row = if multi { true } else { i == n - 1 };
        let hz = horizontal && p.kind == ChartKind::Bar;
        let mut gr = json!({"top": top, "height": ph, "containLabel": true});
        if multi {
            let w = (100.0 - 4.0) / cols as f64;
            gr["left"] = json!(format!("{:.2}%", 2.0 + c as f64 * w));
            gr["width"] = json!(format!("{:.2}%", w - 3.0));
        } else {
            gr["left"] = json!(PAD - 8.0);
            gr["right"] = json!(end_reserve);
        }
        g.grid.push(gr);
        let (lo, hi) = shared.unwrap_or_else(|| extent(p, spec.stack));
        let include_zero = matches!(p.kind, ChartKind::Bar | ChartKind::Area | ChartKind::Histogram);
        let (mut lo2, mut hi2) = (lo, hi);
        for rl in spec.ref_lines.iter().filter(|r| r.panel == p.idx.min(if multi { 0 } else { usize::MAX })) {
            lo2 = lo2.min(rl.value);
            hi2 = hi2.max(rl.value);
        }
        for lt in spec.limit_ticks.iter().filter(|l| l.panel == p.idx.min(if multi { 0 } else { usize::MAX })) {
            lo2 = lo2.min(lt.value);
            hi2 = hi2.max(lt.value);
        }
        let grid_lines = true;
        let x_cat = category_axis(env, i, p.cats, hz, last_row || multi, p.kind == ChartKind::Histogram || matches!(p.kind, ChartKind::Bar));
        let y_val = value_axis(env, i, lo2, hi2, p.vk, hz, !multi || c == 0, include_zero, grid_lines);
        if hz {
            // horizontal bars: categories down the side, values along the bottom
            g.x.push(y_val);
            g.y.push(x_cat);
        } else {
            g.x.push(x_cat);
            g.y.push(y_val);
        }
        if n > 1 || multi {
            g.titles.push(json!({"text": lit(&p.title), "left": if multi { json!(format!("{:.2}%", 2.0 + c as f64 * ((100.0 - 4.0) / cols as f64))) } else { json!(PAD) }, "top": top - 22.0,
                                "textStyle": font_style(t::label_pt() as f64 + 1.0, "600", env.ink.ink2)}));
        }
        let series = match p.kind {
            ChartKind::Line | ChartKind::Area => line_series(env, p, i),
            _ => bar_series(env, p, i, hz),
        };
        g.series.extend(series);
        g.series.extend(tick_series(env, p, i, hz));
        g.series.extend(note_series(env, p, i, hz));
    }
    g.height = top0 + rows as f64 * ph + (rows as f64 - 1.0) * gap + if n > 1 || multi { 22.0 * rows as f64 } else { 0.0 } + AXIS_H + foot_h + 6.0;
    g
}

// ----------------------------------------------------------------------------------- other forms

fn scatter_parts(env: &Env, bubble: bool) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>)> {
    let data = env.data;
    let spec = &data.spec;
    let ml = spec.measure_list();
    let ycol = ml.first().and_then(|m| m.column.clone()).unwrap_or_default();
    let n_series = data.series.len().max(1);
    let max_size = data.points.iter().filter_map(|p| p.size).fold(0.0f64, f64::max);
    let mut series: Vec<Value> = Vec::new();
    let (mut xlo, mut xhi, mut ylo, mut yhi) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for (si, name) in data.series.iter().enumerate() {
        let colour = env.colour_of(name, si, n_series);
        let items: Vec<Value> = data
            .points
            .iter()
            .filter(|p| p.series == si)
            .map(|p| {
                xlo = xlo.min(p.x);
                xhi = xhi.max(p.x);
                ylo = ylo.min(p.y);
                yhi = yhi.max(p.y);
                let mut tip = format!("{}: {}\n{}: {}", spec.category, value_text(p.x, ValueKind::Plain, env.opts.decimals, false), ycol, value_text(p.y, ValueKind::Plain, env.opts.decimals, false));
                let mut item = json!({"value": [num(p.x), num(p.y)]});
                if let (true, Some(sz)) = (bubble, p.size) {
                    let share = if max_size > 0.0 { (sz / max_size).max(0.0).sqrt() } else { 0.0 };
                    item["value"] = json!([num(p.x), num(p.y), num(sz)]);
                    item["symbolSize"] = json!(num(12.0 + share * 34.0));
                    tip.push_str(&format!("\nSize: {}", value_text(sz, ValueKind::Plain, env.opts.decimals, false)));
                }
                if !name.is_empty() {
                    tip.push_str(&format!("\n{}: {}", spec.series.clone().unwrap_or_else(|| "Series".to_string()), name));
                }
                item["tip"] = json!(lit_tip(&tip));
                item
            })
            .collect();
        series.push(json!({
            "name": lit(name), "type": "scatter", "data": items, "symbolSize": t::MARKER_PX,
            "itemStyle": {"color": hx(colour), "opacity": if bubble { 0.7 } else { 0.85 }, "borderColor": hx(env.ink.surface), "borderWidth": 1}
        }));
    }
    if !xlo.is_finite() {
        bail!("No data to chart: no row has both an x and a y number.");
    }
    let ink = env.ink;
    let axis = |lo: f64, hi: f64, vertical: bool| {
        let (ticks, step) = nice_ticks(lo, hi, 6);
        json!({"type": "value", "gridIndex": 0, "min": num(*ticks.first().unwrap_or(&0.0)), "max": num(*ticks.last().unwrap_or(&1.0)), "interval": num(step), "scale": true,
               "axisLine": {"show": !vertical, "lineStyle": {"color": hx(ink.baseline)}}, "axisTick": {"show": false},
               "splitLine": {"show": vertical, "lineStyle": {"color": hx(ink.grid), "width": 1, "type": "solid"}},
               "axisLabel": {"color": hx(ink.muted), "fontFamily": t::FONT_HTML, "fontSize": t::tick_pt(), "formatter": "@fmt:axis:plain"},
               "name": if vertical { ycol.clone() } else { spec.category.clone() }, "nameLocation": "middle", "nameGap": if vertical { 44 } else { 28 },
               "nameTextStyle": font_style(t::label_pt() as f64, "400", ink.ink2)})
    };
    Ok((vec![axis(xlo, xhi, false)], vec![axis(ylo, yhi, true)], series))
}

fn box_parts(env: &Env) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>)> {
    let data = env.data;
    let ink = env.ink;
    let colour = env.colour_of("", 0, 1);
    if data.boxes.is_empty() {
        bail!("No data to chart: no category has a number to summarise.");
    }
    let cats: Vec<String> = data.boxes.iter().map(|b| b.category.clone()).collect();
    let items: Vec<Value> = data
        .boxes
        .iter()
        .map(|b| {
            json!({"value": [num(b.low_whisker), num(b.q1), num(b.median), num(b.q3), num(b.high_whisker)],
                   "tip": lit_tip(&format!("{}: {}\nMedian: {}\nQ1 to Q3: {} to {}\nWhiskers: {} to {}\n{} rows", data.spec.category, b.category, value_text(b.median, ValueKind::Plain, env.opts.decimals, false), value_text(b.q1, ValueKind::Plain, env.opts.decimals, false), value_text(b.q3, ValueKind::Plain, env.opts.decimals, false), value_text(b.low_whisker, ValueKind::Plain, env.opts.decimals, false), value_text(b.high_whisker, ValueKind::Plain, env.opts.decimals, false), crate::models::format_with_commas(b.n as f64, 0)))})
        })
        .collect();
    let mut out_pts: Vec<Value> = Vec::new();
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (i, b) in data.boxes.iter().enumerate() {
        lo = lo.min(b.low_whisker);
        hi = hi.max(b.high_whisker);
        for o in &b.outliers {
            lo = lo.min(*o);
            hi = hi.max(*o);
            out_pts.push(json!({"value": [i, num(*o)], "tip": lit_tip(&format!("{}: {}\nOutlier: {}", data.spec.category, b.category, value_text(*o, ValueKind::Plain, env.opts.decimals, false)))}));
        }
    }
    let (ticks, step) = nice_ticks(lo, hi, 6);
    let x = category_axis(env, 0, &cats, false, true, true);
    let y = json!({"type": "value", "gridIndex": 0, "min": num(*ticks.first().unwrap_or(&0.0)), "max": num(*ticks.last().unwrap_or(&1.0)), "interval": num(step), "axisLine": {"show": false}, "axisTick": {"show": false},
                   "splitLine": {"show": true, "lineStyle": {"color": hx(ink.grid), "width": 1, "type": "solid"}},
                   "axisLabel": {"color": hx(ink.muted), "fontFamily": t::FONT_HTML, "fontSize": t::tick_pt(), "formatter": "@fmt:axis:plain"}});
    let mut series = vec![json!({"name": "Box", "type": "boxplot", "data": items, "boxWidth": [8, t::BAR_MAX_PX],
                                 "itemStyle": {"color": format!("{}33", hx(colour)), "borderColor": hx(colour), "borderWidth": 2}})];
    if !out_pts.is_empty() {
        series.push(json!({"name": "Outliers", "type": "scatter", "data": out_pts, "symbolSize": 6, "itemStyle": {"color": hx(colour)}}));
    }
    Ok((vec![x], vec![y], series))
}

fn waterfall_parts(env: &Env) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>)> {
    let data = env.data;
    let steps = waterfall_from_data(data)?;
    let vk = value_kind(&data.spec, 0);
    let stack = waterfall_stack(&steps);
    let surface = hx(env.ink.surface);
    let colour_of = |k: StepKind, v: f64| -> u32 {
        match k {
            StepKind::Start => t::WATERFALL_START,
            StepKind::End => t::WATERFALL_END,
            StepKind::Delta => t::diverging_colour(v),
        }
    };
    let cats: Vec<String> = steps.iter().map(|s| s.label.clone()).collect();
    let sc = |v: f64| num(scaled(v, vk, env.opts));
    let tips: Vec<String> = steps
        .iter()
        .map(|s| {
            let kind = match s.kind {
                StepKind::Start => "Start",
                StepKind::Delta => "Change",
                StepKind::End => "End",
            };
            lit_tip(&format!("{}: {}\n{}: {}\nLevel after: {}", data.spec.category, s.label, kind, value_text(scaled(s.value, vk, env.opts), vk, env.opts.decimals, s.kind == StepKind::Delta), value_text(scaled(s.level, vk, env.opts), vk, env.opts.decimals, false)))
        })
        .collect();
    let part = |vals: &[f64], name: &str, tip: bool| -> Value {
        let is_pos = name == "Increase";
        let items: Vec<Value> = steps
            .iter()
            .enumerate()
            .map(|(i, s)| {
                // a part with nothing in it is left out, so the 2 px minimum height never draws a stub for a zero
                let empty = vals[i] == 0.0 && !(is_pos && stack.neg[i] == 0.0);
                if empty {
                    return Value::Null;
                }
                json!({"value": sc(vals[i]), "tip": tips[i], "itemStyle": {"color": hx(colour_of(s.kind, s.value)), "borderRadius": 0, "borderColor": surface, "borderWidth": 0}})
            })
            .collect();
        let mut o = json!({"name": name, "type": "bar", "stack": "wf", "data": items, "barMaxWidth": t::BAR_MAX_PX, "barMinHeight": t::MIN_BAR_PX, "z": 2});
        if !tip {
            o["tooltip"] = json!({"show": false});
        }
        o
    };
    let base_items: Vec<Value> = stack.base.iter().map(|v| json!({"value": sc(*v)})).collect();
    let mut base = json!({"name": "Base", "type": "bar", "stack": "wf", "data": base_items, "barMaxWidth": t::BAR_MAX_PX, "silent": true, "z": 1,
                      "itemStyle": {"color": "transparent", "borderColor": "transparent", "borderWidth": 0}, "tooltip": {"show": false}});
    // connectors, as in the app: a 1 px line at each step's level to the next bar (drawn under the bars)
    let connectors: Vec<Value> = steps
        .windows(2)
        .enumerate()
        .map(|(i, w)| json!([{"coord": [i, sc(w[0].level)]}, {"coord": [i + 1, sc(w[0].level)]}]))
        .collect();
    if !connectors.is_empty() {
        base["markLine"] = json!({"silent": true, "symbol": "none", "animation": false, "z": 1, "label": {"show": false},
                                  "lineStyle": {"color": hx(env.ink.baseline), "width": t::GRID_PX, "type": "solid"}, "data": connectors});
    }
    // every step is labelled: a hidden line at the bar's tip carries the text
    let n_steps = steps.len();
    let label_items: Vec<Value> = steps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let (y, pos) = if s.kind == StepKind::Delta && s.value < 0.0 { (s.base, "bottom") } else { (s.top.max(if s.top == 0.0 { s.base } else { s.top }), "top") };
            let y = if s.kind != StepKind::Delta && s.value < 0.0 { s.base } else { y };
            let pos = if s.kind != StepKind::Delta && s.value < 0.0 { "bottom" } else { pos };
            if env.lp.kind_on(vk) && env.lp.shows(i, n_steps, true) && !env.left_out(0, 0, i) {
                json!({"value": sc(y), "label": label_style(env, &env.txt(vk, s.value, s.kind == StepKind::Delta), pos)})
            } else {
                json!({"value": sc(y)})
            }
        })
        .collect();
    let mut labels = json!({"name": "Labels", "type": "line", "data": label_items, "symbol": "circle", "symbolSize": 0, "lineStyle": {"opacity": 0}, "silent": true, "tooltip": {"show": false}, "z": 5});
    if env.lp.mode == LabelMode::All {
        labels["labelLayout"] = json!({"hideOverlap": true});
    }
    let lo = steps.iter().map(|s| s.base).fold(0.0, f64::min);
    let hi = steps.iter().map(|s| s.top).fold(0.0, f64::max);
    let mut series = vec![base, part(&stack.pos, "Increase", true), part(&stack.neg, "Decrease", true), labels];
    let (ml, _) = mark_lines(env, &Plan { kind: ChartKind::Waterfall, title: String::new(), measure: 0, vk, series: Vec::new(), cats: &[], idx: 0 }, false);
    if let Some(ml) = ml {
        series[1]["markLine"] = ml;
    }
    let x = category_axis(env, 0, &cats, false, true, true);
    let y = value_axis(env, 0, lo, hi, vk, false, true, true, true);
    Ok((vec![x], vec![y], series))
}

fn heat_parts(env: &Env) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>)> {
    let data = env.data;
    let h = heatmap(data, Transform::None);
    let vk = value_kind(&data.spec, 0);
    let transform = data.spec.measure_list().first().map(|m| m.transform).unwrap_or(Transform::None);
    let (lo, hi) = (h.min.unwrap_or(0.0), h.max.unwrap_or(1.0));
    let ink = env.ink;
    let mut items: Vec<Value> = Vec::new();
    for (ri, r) in h.rows.iter().enumerate() {
        for (ci, c) in h.cols.iter().enumerate() {
            let Some(v) = h.cells.get(ri).and_then(|x| x.get(ci)).copied().flatten() else { continue };
            let n = h.counts.get(ri).and_then(|x| x.get(ci)).copied().unwrap_or(0);
            let fill_kind = h.fill(v, n);
            let text = if vk == ValueKind::Percent { crate::models::format_with_commas(v, env.lp.decimals.unwrap_or(1)) } else { env.txt(vk, v, false) };
            let (fill_hex, text_hex) = match fill_kind {
                HeatFill::Neutral => (hx(t::heat_neutral(env.dark)), hx(t::heat_neutral_text(env.dark))),
                HeatFill::Step(step) => (hx(t::sequential_colour(step, env.dark)), if t::sequential_text_white(step, env.dark) { "#ffffff".to_string() } else { hx(t::INK_LIGHT.ink) }),
            };
            let mut tip = if data.spec.series.is_some() { format!("{}: {}\n{}: {}", data.spec.series.clone().unwrap_or_default(), r, data.spec.category, c) } else { format!("{}: {}", data.spec.category, c) };
            tip.push_str(&format!("\n{}: {}\n{} rows", data.measure_names.first().cloned().unwrap_or_else(|| "Value".to_string()), if vk == ValueKind::Percent { format!("{}%", crate::models::format_with_commas(v, 2)) } else { value_text(scaled(v, vk, env.opts), vk, env.opts.decimals, false) }, crate::models::format_with_commas(n as f64, 0)));
            if vk == ValueKind::Percent {
                tip.push_str(&format!("\nPercent {}", base_word(transform)));
            }
            items.push(json!({
                "value": [ci, ri, num(v)], "tip": lit_tip(&tip),
                "itemStyle": {"color": fill_hex, "borderColor": hx(ink.surface), "borderWidth": 1, "borderRadius": t::HEAT_RADIUS_PX},
                "label": {"show": env.lp.kind_on(vk), "formatter": lit(&text), "color": text_hex, "fontFamily": t::FONT_HTML, "fontSize": t::data_label_pt()}
            }));
        }
    }
    let mut x = category_axis(env, 0, &h.cols, false, true, true);
    x["position"] = json!("top");
    x["axisLine"] = json!({"show": false});
    let y = category_axis(env, 0, &h.rows, true, true, true);
    let mut y = y;
    y["axisLine"] = json!({"show": false});
    let series = vec![json!({"name": data.measure_names.first().cloned().unwrap_or_default(), "type": "heatmap", "data": items})];
    Ok((vec![x], vec![y], series))
}

fn donut_series(env: &Env) -> Result<Vec<Value>> {
    let data = env.data;
    let vk = value_kind(&data.spec, 0);
    let parts: Vec<(String, f64, u64)> = data
        .categories
        .iter()
        .enumerate()
        .filter_map(|(i, c)| data.values.first().and_then(|r| r.get(i)).copied().flatten().map(|v| (c.clone(), v, data.rows.first().and_then(|r| r.get(i)).copied().unwrap_or(0))))
        .collect();
    if parts.len() < 2 {
        bail!("A donut needs at least 2 parts, but this chart has {}. Choose another form.", parts.len());
    }
    if parts.iter().any(|(_, v, _)| *v < 0.0) {
        bail!("A donut cannot show negative values. Use a bar chart.");
    }
    let total: f64 = parts.iter().map(|p| p.1).sum();
    // colours by name in the order of the parts; past the eighth the de-emphasised grey (and every part keeps its own label)
    let mut cm = env.cm.clone();
    cm.assign_all(&parts.iter().map(|p| p.0.clone()).collect::<Vec<_>>());
    let items: Vec<Value> = parts
        .iter()
        .enumerate()
        .map(|(i, (name, v, n))| {
            let colour = cm.get(name).map(|c| c.hex(env.dark)).unwrap_or_else(|| env.colour_of(name, i, parts.len()));
            let share = if total > 0.0 { v / total * 100.0 } else { 0.0 };
            let tip = format!("{}: {}\n{}: {}\nShare: {}%\n{} rows", data.spec.category, name, data.measure_names.first().cloned().unwrap_or_else(|| "Value".to_string()), value_text(scaled(*v, vk, env.opts), vk, env.opts.decimals, false), crate::models::format_with_commas(share, 1), crate::models::format_with_commas(*n as f64, 0));
            // a part under 1% has no label on the ring (it stays in the legend and the tooltip)
            let text = if env.lp.mode != LabelMode::None && env.lp.percent { format!("{}  {}%", name, crate::models::format_with_commas(share, 0)) } else { name.to_string() };
            json!({"name": lit(name), "value": num(scaled(*v, vk, env.opts)), "tip": lit_tip(&tip), "itemStyle": {"color": hx(colour)},
                   "label": {"show": share >= 1.0, "formatter": lit(&text)}, "labelLine": {"show": share >= 1.0}})
        })
        .collect();
    Ok(vec![json!({
        "type": "pie", "radius": ["46%", "64%"], "center": ["50%", "55%"], "data": items, "avoidLabelOverlap": true, "minShowLabelAngle": 3.6,
        "itemStyle": {"borderColor": hx(env.ink.surface), "borderWidth": 2},
        // outside the ring with leader lines; labels that would overlap are hidden (they are in the tooltip)
        "label": {"show": true, "position": "outside", "color": hx(env.ink.ink), "fontFamily": t::FONT_HTML, "fontSize": t::label_pt()},
        "labelLine": {"show": true, "length": 14, "length2": 10, "lineStyle": {"color": hx(env.ink.muted), "width": 1}},
        "labelLayout": {"hideOverlap": true}
    })])
}

/// Sankey: the series (rows of the table) are the "from" nodes, the categories the "to" nodes, and each
/// non-zero cell is a link with the cell's value.
fn sankey_series(env: &Env) -> Result<Vec<Value>> {
    let data = env.data;
    if data.spec.series.as_deref().map(str::trim).unwrap_or("").is_empty() {
        bail!("A Sankey needs two text columns: the Series column is where the flow comes from and the Category column is where it goes to. Choose the Series column.");
    }
    let vk = value_kind(&data.spec, 0);
    let mut names: Vec<String> = Vec::new();
    for n in data.series.iter().chain(data.categories.iter()) {
        if !names.contains(n) {
            names.push(n.clone());
        }
    }
    let mut cm = env.cm.clone();
    cm.assign_all(&names);
    let col = |n: &str, i: usize| -> u32 {
        match cm.get(n) {
            Some(c) => c.hex(env.dark),
            None => t::series_colour(i.min(t::SLOTS - 1), env.dark).unwrap_or(0),
        }
    };
    let mut links: Vec<Value> = Vec::new();
    let mut out_sum = vec![0.0f64; data.series.len()];
    let mut in_sum = vec![0.0f64; data.categories.len()];
    for (si, s) in data.series.iter().enumerate() {
        for (ci, c) in data.categories.iter().enumerate() {
            let Some(v) = data.values.get(si).and_then(|r| r.get(ci)).copied().flatten() else { continue };
            if v < 0.0 {
                bail!("A Sankey cannot show a negative flow ({} to {}: {}). Filter the rows or use a heatmap.", s, c, value_text(v, vk, env.opts.decimals, false));
            }
            if v == 0.0 {
                continue;
            }
            out_sum[si] += v;
            in_sum[ci] += v;
            let rows = data.rows.get(si).and_then(|r| r.get(ci)).copied().unwrap_or(0);
            let tip = format!("{} \u{2192} {}\n{}: {}\n{} rows", s, c, data.measure_names.first().cloned().unwrap_or_else(|| "Value".to_string()), value_text(scaled(v, vk, env.opts), vk, env.opts.decimals, false), crate::models::format_with_commas(rows as f64, 0));
            links.push(json!({"source": format!("From {}", s), "target": format!("To {}", c), "value": num(scaled(v, vk, env.opts)), "tip": lit_tip(&tip)}));
        }
    }
    let mut nodes: Vec<Value> = Vec::new();
    for (si, s) in data.series.iter().enumerate() {
        if out_sum[si] > 0.0 {
            nodes.push(json!({"name": format!("From {}", s), "depth": 0, "value": num(scaled(out_sum[si], vk, env.opts)), "itemStyle": {"color": hx(col(s, si))}, "label": {"formatter": lit(s), "position": "left"}}));
        }
    }
    for (ci, c) in data.categories.iter().enumerate() {
        if in_sum[ci] > 0.0 {
            nodes.push(json!({"name": format!("To {}", c), "depth": 1, "value": num(scaled(in_sum[ci], vk, env.opts)), "itemStyle": {"color": hx(col(c, ci))}, "label": {"formatter": lit(c), "position": "right"}}));
        }
    }
    if links.is_empty() {
        bail!("No data to chart: every flow is zero or blank.");
    }
    Ok(vec![json!({
        "type": "sankey", "data": nodes, "links": links, "left": 90, "right": 130, "top": HEAD_H + 10.0, "bottom": 30,
        "nodeWidth": 14, "nodeGap": 12, "nodeAlign": "justify", "draggable": false, "layoutIterations": 0,
        "emphasis": {"focus": "adjacency"},
        "lineStyle": {"color": "source", "opacity": 0.35, "curveness": 0.5},
        "label": {"color": hx(env.ink.ink), "fontFamily": t::FONT_HTML, "fontSize": t::label_pt()},
        "itemStyle": {"borderColor": hx(env.ink.surface), "borderWidth": 1}
    })])
}

fn treemap_series(env: &Env) -> Result<Vec<Value>> {
    let data = env.data;
    let vk = value_kind(&data.spec, 0);
    let n_series = data.series.len();
    let mut tree: Vec<Value> = Vec::new();
    let leaf = |c: &str, v: f64, rows: u64, group: &str, colour: u32| -> Result<Value> {
        if v < 0.0 {
            bail!("A treemap cannot show a negative value ({}: {}). Filter the rows or use a bar chart.", c, value_text(v, vk, env.opts.decimals, false));
        }
        let tip = format!("{}{}\n{}: {}\n{} rows", if group.is_empty() { String::new() } else { format!("{} \u{203a} ", group) }, c, data.measure_names.first().cloned().unwrap_or_else(|| "Value".to_string()), value_text(scaled(v, vk, env.opts), vk, env.opts.decimals, false), crate::models::format_with_commas(rows as f64, 0));
        Ok(json!({"name": lit(c), "value": num(scaled(v, vk, env.opts)), "tip": lit_tip(&tip), "itemStyle": {"color": hx(colour)}}))
    };
    let has_series = data.spec.series.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false) && n_series > 0;
    if has_series {
        for (si, s) in data.series.iter().enumerate() {
            let colour = env.colour_of(s, si, n_series);
            let mut kids = Vec::new();
            for (ci, c) in data.categories.iter().enumerate() {
                if let Some(v) = data.values.get(si).and_then(|r| r.get(ci)).copied().flatten() {
                    if v != 0.0 {
                        kids.push(leaf(c, v, data.rows.get(si).and_then(|r| r.get(ci)).copied().unwrap_or(0), s, colour)?);
                    }
                }
            }
            if !kids.is_empty() {
                tree.push(json!({"name": lit(s), "children": kids, "itemStyle": {"color": hx(colour)}}));
            }
        }
    } else {
        let n = data.categories.len();
        for (ci, c) in data.categories.iter().enumerate() {
            if let Some(v) = data.values.first().and_then(|r| r.get(ci)).copied().flatten() {
                if v != 0.0 {
                    let colour = env.colour_of(c, ci, n);
                    tree.push(leaf(c, v, data.rows.first().and_then(|r| r.get(ci)).copied().unwrap_or(0), "", colour)?);
                }
            }
        }
    }
    if tree.is_empty() {
        bail!("No data to chart: every value is zero or blank.");
    }
    Ok(vec![json!({
        "type": "treemap", "data": tree, "left": PAD - 8.0, "right": PAD - 8.0, "top": HEAD_H + 10.0, "bottom": 30, "roam": false, "nodeClick": false,
        "breadcrumb": {"show": false}, "squareRatio": 1.2,
        "label": {"show": true, "color": "#ffffff", "fontFamily": t::FONT_HTML, "fontSize": t::label_pt()},
        "upperLabel": {"show": has_series, "height": 22, "color": "#ffffff", "fontFamily": t::FONT_HTML, "fontSize": t::label_pt() + 1.0},
        "itemStyle": {"borderColor": hx(env.ink.surface), "borderWidth": 2, "gapWidth": 2}
    })])
}

// ---------------------------------------------------------------------------------------- the build

fn legend_items(env: &Env, plans: &[Plan]) -> Option<Vec<(String, u32)>> {
    let data = env.data;
    let spec = &data.spec;
    let n = data.series.len();
    if matches!(spec.kind, ChartKind::Waterfall | ChartKind::Histogram | ChartKind::BoxPlot | ChartKind::Heatmap | ChartKind::Donut | ChartKind::Sankey | ChartKind::Treemap) || n < 2 {
        return None;
    }
    let _ = plans;
    Some(data.series.iter().enumerate().map(|(i, s)| (s.clone(), env.colour_of(s, i, n))).collect())
}

fn build_one(data: &ChartData, opts: &LayoutOpts, dark: bool) -> Result<(Value, f64)> {
    let spec = &data.spec;
    let ink = t::ink(dark);
    let mut cm = opts.colours.clone().unwrap_or_else(|| spec.colour_map.clone());
    let named: Vec<String> = data.series.iter().filter(|s| !s.is_empty()).cloned().collect();
    cm.assign_all(&named);
    let lp = LabelPlan::new(spec, opts);
    // "All" on the page: every label is in the option and ECharts hides the ones that really overlap at the size the
    // page is shown in (`labelLayout.hideOverlap`); the nominal rule of the app, the workbook and the script is not used
    let dropped: std::collections::HashSet<(usize, usize, usize)> = std::collections::HashSet::new();
    // "Segment labels" (W11): which segments say their value, and whether it is turned, is the app's layout at a size a page chart
    // is about as big as (a segment that has no room there has none here either)
    let segs = if spec.inside_labels && spec.stack != Stack::Grouped {
        let nominal = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(SEGMENT_NOMINAL_W, SEGMENT_NOMINAL_H));
        let l = super::layout::layout_chart(data, nominal, &LayoutOpts { dark, ..opts.clone() }, &super::layout::ApproxMeasure);
        l.segment_labels.into_iter().map(|x| ((x.panel, x.series, x.category), x)).collect()
    } else {
        std::collections::HashMap::new()
    };
    let env = Env { data, opts, dark, ink, cm, lp, dropped, segs };
    if let Some(r) = data.empty_reason() {
        bail!("No data to chart: {}.", r);
    }
    let title = chart_title(data);
    let mut notes = super::labels::footnote_notes(data, opts, 0);
    if env.lp.mode == LabelMode::All {
        // the page cannot count what the browser hides: the note says so, and the tooltip holds every value
        notes.push("Data labels that would overlap are hidden; hover a mark for its value".to_string());
    }
    let foot = foot_lines(data, &notes);
    let foot_h = if foot.is_empty() { 0.0 } else { foot.len() as f64 * FOOT_LINE_H + 8.0 };
    let kind = spec.kind;
    let ps = plans(&env);
    let legend = legend_items(&env, &ps);
    let legend_h = if legend.is_some() { LEGEND_H } else { 0.0 };

    let mut o;
    match kind {
        ChartKind::Donut => {
            let series = donut_series(&env)?;
            let height = HEAD_H + 340.0 + foot_h;
            o = base_option(&env, &title, None, &foot, height);
            o.insert("series".into(), json!(series));
            return finish(o, spec, height);
        }
        ChartKind::Sankey => {
            let series = sankey_series(&env)?;
            let height = HEAD_H + 380.0 + foot_h;
            o = base_option(&env, &title, None, &foot, height);
            o.insert("series".into(), json!(series));
            return finish(o, spec, height);
        }
        ChartKind::Treemap => {
            let series = treemap_series(&env)?;
            let height = HEAD_H + 380.0 + foot_h;
            o = base_option(&env, &title, None, &foot, height);
            o.insert("series".into(), json!(series));
            return finish(o, spec, height);
        }
        ChartKind::Scatter | ChartKind::Bubble | ChartKind::BoxPlot | ChartKind::Waterfall | ChartKind::Heatmap => {
            let (x, y, series) = match kind {
                ChartKind::Scatter => scatter_parts(&env, false)?,
                ChartKind::Bubble => scatter_parts(&env, true)?,
                ChartKind::BoxPlot => box_parts(&env)?,
                ChartKind::Waterfall => waterfall_parts(&env)?,
                _ => heat_parts(&env)?,
            };
            let rows = if kind == ChartKind::Heatmap { data.series.len().max(1) } else { 0 };
            let plot_h = if kind == ChartKind::Heatmap { (rows as f64 * 40.0).clamp(160.0, 520.0) } else { SINGLE_H };
            let h = HEAD_H + legend_h + 6.0 + plot_h + AXIS_H + foot_h + 6.0;
            o = base_option(&env, &title, legend.clone(), &foot, h);
            let top = HEAD_H + legend_h + 6.0 + if kind == ChartKind::Heatmap { 22.0 } else { 0.0 };
            let (pad_left, pad_right) = (PAD - 8.0, 24.0);
            o.insert("grid".into(), json!([{"left": pad_left, "right": pad_right, "top": top, "height": plot_h, "containLabel": true}]));
            o.insert("xAxis".into(), json!(x));
            o.insert("yAxis".into(), json!(y));
            o.insert("series".into(), json!(series));
            return finish(o, spec, h);
        }
        _ => {}
    }
    let g = cartesian(&env, &ps, legend_h, foot_h);
    let height = g.height;
    o = base_option(&env, &title, legend, &foot, height);
    if let Some(Value::Array(titles)) = o.get_mut("title") {
        titles.extend(g.titles.clone());
    }
    o.insert("grid".into(), json!(g.grid));
    o.insert("xAxis".into(), json!(g.x));
    o.insert("yAxis".into(), json!(g.y));
    o.insert("series".into(), json!(g.series));
    if matches!(kind, ChartKind::Line | ChartKind::Area) || spec.panels.iter().any(|p| matches!(p.kind, ChartKind::Line | ChartKind::Area)) {
        let mut tt = o.get("tooltip").cloned().unwrap_or(json!({}));
        tt["trigger"] = json!("axis");
        tt["formatter"] = json!("@axistip");
        tt["axisPointer"] = json!({"type": "line", "lineStyle": {"color": hx(ink.muted), "width": 1, "type": "solid"}, "label": {"show": false}});
        o.insert("tooltip".into(), tt);
    }
    if g.grid.len() > 1 {
        o.insert("axisPointer".into(), json!({"link": [{"xAxisIndex": "all"}]}));
    }
    finish(o, spec, height)
}

/// Every text in the option gets the true minus sign (a hyphen before a number is a minus); the
/// "@..." names of the page's formatter functions are left as they are.
fn tidy_strings(v: &mut Value) {
    match v {
        Value::String(s) => {
            if !s.starts_with('@') {
                *s = t::minus(s);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(tidy_strings),
        Value::Object(m) => m.values_mut().for_each(tidy_strings),
        _ => {}
    }
}

fn finish(o: Map<String, Value>, spec: &ChartSpec, height: f64) -> Result<(Value, f64)> {
    let mut option = Value::Object(o);
    tidy_strings(&mut option);
    if let Some(adv) = validate_advanced(&spec.advanced_json)? {
        deep_merge(&mut option, &adv);
    }
    Ok((option, height))
}

/// The chart's option in both themes, with its height and table. Refusals (a donut with 6 parts, a
/// waterfall that does not add up, bad advanced JSON) come back as errors with plain messages.
pub fn chart_pair(data: &ChartData, opts: &LayoutOpts) -> Result<ChartOut> {
    t::with_text_scale(data.spec.text_size.factor(), || chart_pair_scaled(data, opts))
}

fn chart_pair_scaled(data: &ChartData, opts: &LayoutOpts) -> Result<ChartOut> {
    if let Some(m) = data.non_finite() {
        bail!("Chart: {}. Fix those rows or filter them out, then chart again.", m);
    }
    let (light, h) = build_one(data, opts, false)?;
    let (dark, _) = build_one(data, opts, true)?;
    // the Data table option (W11): the chart's categories x series with totals, open under the chart; off = the page's own table behind its toggle
    let printed = if data.spec.data_table == super::spec::DataTable::Off { None } else { super::data_table::data_table(data, opts) };
    let (table, table_open) = match &printed {
        Some(tbl) => (super::data_table::data_table_html(tbl), true),
        None => (table_html(data, opts), false),
    };
    Ok(ChartOut { title: t::tidy(&chart_title(data)), light, dark, height: h.ceil() as u32, table, table_open })
}

// -------------------------------------------------------------------------------- advanced options

/// The advanced options: `None` when blank, the JSON object otherwise. Invalid JSON is refused with the
/// line and column of the first mistake.
pub fn validate_advanced(text: &str) -> Result<Option<Value>> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    match serde_json::from_str::<Value>(text) {
        Ok(v @ Value::Object(_)) => Ok(Some(v)),
        Ok(_) => bail!("The advanced options must be one JSON object, like {{ \"legend\": {{ \"show\": false }} }}."),
        Err(e) => bail!("The advanced options are not valid JSON: {} (line {}, column {}). Fix that spot, or press Reset to clear the box.", plain_json_error(&e), e.line(), e.column()),
    }
}

fn plain_json_error(e: &serde_json::Error) -> String {
    let s = e.to_string();
    match s.rfind(" at line ") {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

/// Objects merge key by key; anything else (a number, a list) is replaced.
pub fn deep_merge(base: &mut Value, over: &Value) {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                match b.get_mut(k) {
                    Some(bv) => deep_merge(bv, v),
                    None => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, o) => *b = o.clone(),
    }
}

// ------------------------------------------------------------------------------------ the table

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The Table toggle's content: the chart's numbers, one row per category.
pub fn table_html(data: &ChartData, opts: &LayoutOpts) -> String {
    let spec = &data.spec;
    let mut s = String::from("<table class=\"mp-table\">");
    let vk0 = value_kind(spec, 0);
    let fmt = |v: Option<f64>, vk: ValueKind| v.map(|x| value_text(scaled(x, vk, opts), vk, opts.decimals, false)).unwrap_or_default();
    match spec.kind {
        ChartKind::Scatter | ChartKind::Bubble => {
            s.push_str(&format!("<thead><tr><th>{}</th><th>{}</th>{}{}</tr></thead><tbody>", esc(&t::tidy(&spec.category)), esc(&t::tidy(&data.measure_names.first().cloned().unwrap_or_default())), if spec.kind == ChartKind::Bubble { "<th>Size</th>" } else { "" }, if spec.series.is_some() { "<th>Series</th>" } else { "" }));
            for p in data.points.iter().take(2000) {
                s.push_str(&format!("<tr><td>{}</td><td>{}</td>", value_text(p.x, ValueKind::Plain, opts.decimals, false), value_text(p.y, ValueKind::Plain, opts.decimals, false)));
                if spec.kind == ChartKind::Bubble {
                    s.push_str(&format!("<td>{}</td>", p.size.map(|z| value_text(z, ValueKind::Plain, opts.decimals, false)).unwrap_or_default()));
                }
                if spec.series.is_some() {
                    s.push_str(&format!("<td>{}</td>", esc(data.series.get(p.series).map(String::as_str).unwrap_or(""))));
                }
                s.push_str("</tr>");
            }
            s.push_str("</tbody>");
        }
        ChartKind::BoxPlot => {
            s.push_str(&format!("<thead><tr><th>{}</th><th>Rows</th><th>Min</th><th>Q1</th><th>Median</th><th>Q3</th><th>Max</th></tr></thead><tbody>", esc(&t::tidy(&spec.category))));
            for b in &data.boxes {
                s.push_str(&format!("<tr><td>{}</td><td>{}</td>", esc(&b.category), crate::models::format_with_commas(b.n as f64, 0)));
                for v in [b.min, b.q1, b.median, b.q3, b.max] {
                    s.push_str(&format!("<td>{}</td>", value_text(v, ValueKind::Plain, opts.decimals, false)));
                }
                s.push_str("</tr>");
            }
            s.push_str("</tbody>");
        }
        _ => {
            let mut heads: Vec<(usize, usize)> = Vec::new();
            for m in 0..data.measure_count() {
                for si in 0..data.series.len().max(1) {
                    heads.push((m, si));
                }
            }
            s.push_str(&format!("<thead><tr><th>{}</th>", esc(&t::tidy(if spec.category.is_empty() { "Category" } else { &spec.category }))));
            for (m, si) in &heads {
                let name = data.measure_names.get(*m).cloned().unwrap_or_default();
                let ser = data.series.get(*si).cloned().unwrap_or_default();
                s.push_str(&format!("<th>{}</th>", esc(&t::tidy(&if ser.is_empty() { name } else { format!("{} \u{b7} {}", ser, name) }))));
            }
            s.push_str("<th>Rows</th></tr></thead><tbody>");
            for (i, c) in data.categories.iter().enumerate() {
                s.push_str(&format!("<tr><td>{}</td>", esc(c)));
                for (m, si) in &heads {
                    let v = data.measure(*m).and_then(|md| md.values.get(*si).and_then(|r| r.get(i)).copied().flatten());
                    let vk = if *m == 0 { vk0 } else { value_kind(spec, *m) };
                    s.push_str(&format!("<td>{}</td>", fmt(v, vk)));
                }
                let rows: u64 = data.rows.iter().map(|r| r.get(i).copied().unwrap_or(0)).sum();
                s.push_str(&format!("<td>{}</td></tr>", crate::models::format_with_commas(rows as f64, 0)));
            }
            s.push_str("</tbody>");
        }
    }
    s.push_str("</table>");
    s
}

// ------------------------------------------------------------------------------------ the theme

/// The ECharts theme registered as `microparquet-light` / `microparquet-dark`.
pub fn theme_json(dark: bool) -> Value {
    let ink = t::ink(dark);
    let colors: Vec<String> = (0..t::SLOTS).filter_map(|i| t::series_colour(i, dark)).map(hx).collect();
    let axis = |show_line: bool, grid: bool| {
        json!({
            "axisLine": {"show": show_line, "lineStyle": {"color": hx(ink.baseline), "width": 1}},
            "axisTick": {"show": false},
            "axisLabel": {"show": true, "color": hx(ink.muted), "fontFamily": t::FONT_HTML, "fontSize": t::tick_pt()},
            "splitLine": {"show": grid, "lineStyle": {"color": hx(ink.grid), "width": 1, "type": "solid"}},
            "splitArea": {"show": false}
        })
    };
    json!({
        "color": colors,
        "backgroundColor": hx(ink.surface),
        "textStyle": {"fontFamily": t::FONT_HTML, "color": hx(ink.ink2)},
        "title": {"textStyle": {"fontFamily": t::FONT_HTML, "fontSize": t::TITLE_PT, "fontWeight": "600", "color": hx(ink.ink)}, "subtextStyle": {"fontFamily": t::FONT_HTML, "fontSize": t::SUBTITLE_PT, "color": hx(ink.ink2)}},
        "legend": {"textStyle": {"fontFamily": t::FONT_HTML, "fontSize": t::label_pt(), "color": hx(ink.ink2)}},
        "categoryAxis": axis(true, false),
        "valueAxis": axis(false, true),
        "logAxis": axis(false, true),
        "timeAxis": axis(true, false),
        "bar": {"itemStyle": {"borderRadius": [t::BAR_RADIUS_PX, t::BAR_RADIUS_PX, 0, 0], "borderWidth": 0}},
        "line": {"lineStyle": {"width": t::LINE_PX}, "symbolSize": t::MARKER_PX, "symbol": "circle", "smooth": false},
        "tooltip": {"backgroundColor": hx(ink.surface), "borderColor": hx(ink.baseline), "borderWidth": 1, "textStyle": {"fontFamily": t::FONT_HTML, "fontSize": t::label_pt(), "color": hx(ink.ink)}},
        "toolbox": {"iconStyle": {"borderColor": hx(ink.muted)}}
    })
}

// ------------------------------------------------------------------------------------- the page

const PAGE_CSS: &str = r#"
:root{--surface:#fcfcfb;--ink:#0b0b0b;--ink2:#52514e;--muted:#898781;--grid:#e1e0d9;--base:#c3c2b7}
:root[data-theme="dark"]{--surface:#1a1a19;--ink:#f0efec;--ink2:#c3c2b7;--muted:#898781;--grid:#2c2c2a;--base:#383835}
*{box-sizing:border-box}
html,body{margin:0;background:var(--surface);color:var(--ink);font-family:"Source Sans 3","Segoe UI",system-ui,sans-serif;font-size:14px}
.mp-page{max-width:1280px;margin:0 auto;padding:12px 20px 28px}
.mp-bar{display:flex;gap:8px;justify-content:flex-end;margin:4px 0 8px}
.mp-btn{font:inherit;font-size:13px;color:var(--ink2);background:var(--surface);border:1px solid var(--base);border-radius:4px;padding:3px 12px;cursor:pointer}
.mp-btn:hover{color:var(--ink);border-color:var(--muted)}
.mp-head{margin:8px 0 14px}.mp-head h1{font-size:20px;font-weight:600;margin:0 0 2px}.mp-head p{margin:0;color:var(--ink2)}
.mp-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(520px,1fr));gap:20px}
.mp-card{min-width:0}
.mp-chart{width:100%}
.mp-tablewrap{overflow:auto;max-height:640px;padding:8px 0}
.mp-table{border-collapse:collapse;font-size:13px;min-width:50%}
.mp-table th,.mp-table td{padding:4px 12px;border-bottom:1px solid var(--grid);text-align:right;white-space:nowrap}
.mp-table th:first-child,.mp-table td:first-child{text-align:left}
.mp-table th{font-weight:600;color:var(--ink2);border-bottom:1px solid var(--base)}
.mp-sr{position:absolute;left:-9999px}
.mp-err{padding:24px;color:var(--ink2)}
@media print{.mp-bar{display:none}}
"#;

const PAGE_JS: &str = r#"
(function(){
  var D = JSON.parse(document.getElementById('mp-data').textContent);
  var root = document.documentElement;
  var dark = false;
  try { dark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches && D.followSystem; } catch(e){}
  function group(s, d){ var p = s.split('.'); var n = p[0]; var neg = n.charAt(0) === '-'; if (neg) n = n.slice(1);
    n = n.replace(/\B(?=(\d{3})+(?!\d))/g, ','); return (neg ? '−' : '') + n + (p.length > 1 ? '.' + p[1] : ''); }
  function nf(v, d){ if (v === null || v === undefined || isNaN(v)) return ''; return group(Number(v).toFixed(d)); }
  function trimmed(v, max){ var s = Number(v).toFixed(max); if (s.indexOf('.') >= 0) s = s.replace(/0+$/, '').replace(/\.$/, ''); return group(s); }
  function esc(s){ return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;'); }
  var FN = {
    '@fmt:axis:plain': function(v){ return trimmed(v, 4); },
    '@fmt:axis:percent': function(v){ return trimmed(v, 4) + '%'; },
    '@tip': function(p){ var d = p.data && p.data.tip; return d ? esc(d).replace(/\n/g,'<br>') : esc(p.name || ''); },
    '@axistip': function(ps){ if (!ps || !ps.length) return ''; var out = '<b>' + esc(ps[0].axisValueLabel || ps[0].name || '') + '</b>';
      var head = String(ps[0].axisValueLabel || ps[0].name || '');
      ps.forEach(function(p){ if (p.data && p.data.tip) { var lines = String(p.data.tip).split('\n');
        if (lines.length > 1 && lines[0].slice(-(head.length + 2)) === ': ' + head) lines.shift();
        out += '<br>' + p.marker + esc(lines.join('\n')).replace(/\n/g,'<br>'); } }); return out; }
  };
  function fn(tok){
    if (FN[tok]) return FN[tok];
    var m = /^@fmt:(amount|plain):(\d+)$/.exec(tok); if (m) { var d = +m[2]; return function(v){ return nf(v, d); }; }
    m = /^@fmt:percent:(\d+)$/.exec(tok); if (m) { var e = +m[1]; return function(v){ return nf(v, e) + '%'; }; }
    if (tok === '@fmt:count') return function(v){ return nf(v, 0); };
    return null;
  }
  function revive(o){
    if (Array.isArray(o)) return o.map(revive);
    if (o && typeof o === 'object') { var r = {}; Object.keys(o).forEach(function(k){ r[k] = revive(o[k]); }); return r; }
    if (typeof o === 'string' && o.charAt(0) === '@') { var f = fn(o); if (f) return f; }
    return o;
  }
  function download(name, mime, text){
    var a = document.createElement('a'); a.download = name; a.href = URL.createObjectURL(new Blob([text], {type: mime}));
    document.body.appendChild(a); a.click(); document.body.removeChild(a); setTimeout(function(){ URL.revokeObjectURL(a.href); }, 1000);
  }
  var charts = [];
  function themeName(){ return dark ? 'microparquet-dark' : 'microparquet-light'; }
  function optionFor(c){
    var o = revive(dark ? c.dark : c.light);
    if (o.toolbox && o.toolbox.feature && o.toolbox.feature.mySvg) {
      o.toolbox.feature.mySvg.onclick = function(){
        var el = document.getElementById(c.id);
        var s = echarts.init(null, themeName(), {renderer: 'svg', ssr: true, width: el.clientWidth, height: c.height});
        var so = revive(dark ? c.dark : c.light); delete so.toolbox; delete so.animation;
        s.setOption(so); var svg = s.renderToSVGString(); s.dispose();
        download((c.name || 'chart') + '.svg', 'image/svg+xml', svg);
      };
    }
    return o;
  }
  function draw(){
    root.setAttribute('data-theme', dark ? 'dark' : 'light');
    charts.forEach(function(x){ if (x.inst) x.inst.dispose(); });
    charts = D.charts.map(function(c){
      var el = document.getElementById(c.id);
      if (!c.light) { return {c: c}; }
      el.style.height = c.height + 'px';
      var inst = echarts.init(el, themeName(), {renderer: 'canvas'});
      inst.setOption(optionFor(c));
      return {c: c, inst: inst};
    });
    var b = document.getElementById('mp-theme'); if (b) b.textContent = dark ? 'Light' : 'Dark';
  }
  echarts.registerTheme('microparquet-light', D.themes.light);
  echarts.registerTheme('microparquet-dark', D.themes.dark);
  draw();
  window.addEventListener('resize', function(){ charts.forEach(function(x){ if (x.inst) x.inst.resize(); }); });
  var tb = document.getElementById('mp-theme'); if (tb) tb.addEventListener('click', function(){ dark = !dark; draw(); });
  var tt = document.getElementById('mp-table'); if (tt) tt.addEventListener('click', function(){
    var show = tt.getAttribute('data-on') !== '1'; tt.setAttribute('data-on', show ? '1' : '0'); tt.textContent = show ? 'Chart' : 'Table';
    D.charts.forEach(function(c){ var w = document.getElementById(c.id + '-table'); var e = document.getElementById(c.id);
      if (w) w.hidden = !show; if (e) e.hidden = show; });
    if (!show) charts.forEach(function(x){ if (x.inst) x.inst.resize(); });
  });
})();
"#;

/// What a page needs when a chart has its data table open under it (Phase 8 W11): the table's own look, and the "Table" toggle leaving
/// that table (and its chart) shown. A page with no open table does not carry them.
const OPEN_TABLE_CSS: &str = ".mp-table caption{caption-side:top;text-align:left;font-weight:600;color:var(--ink2);padding:4px 12px}\n.mp-table tr.mp-total th{border-top:1px solid var(--base);border-bottom:0;color:var(--ink)}\n.mp-note{margin:4px 12px;color:var(--ink2);font-size:12px}\n.mp-tablewrap.open{max-height:none;padding-top:0}\n";
const OPEN_TABLE_JS: &str = "(function(){var tt=document.getElementById('mp-table');if(!tt)return;var D=JSON.parse(document.getElementById('mp-data').textContent);tt.addEventListener('click',function(){D.charts.forEach(function(c){if(!c.open)return;var w=document.getElementById(c.id+'-table'),e=document.getElementById(c.id);if(w)w.hidden=false;if(e)e.hidden=false;});window.dispatchEvent(new Event('resize'));});})();\n";

/// One chart of a page: its option pair, a name for saved files, the table.
#[derive(Clone, Debug)]
pub struct PageChart {
    pub out: ChartOut,
    pub name: String,
}

/// JSON for a `<script type="application/json">`: no "</" can end the element early.
fn script_json(v: &Value) -> String {
    v.to_string().replace("</", "<\\/").replace("<!--", "\\u003c!--")
}

fn page(title: &str, intro: Option<(&str, &str)>, charts: &[PageChart], grid: bool) -> String {
    let mut s = String::with_capacity(ECHARTS_JS.len() + 64 * 1024);
    s.push_str("<!doctype html>\n<html lang=\"en\" data-theme=\"light\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">");
    let any_open = charts.iter().any(|c| c.out.table_open);
    s.push_str(&format!("<title>{}</title><style>{}{}</style></head><body><div class=\"mp-page\">", esc(title), PAGE_CSS, if any_open { OPEN_TABLE_CSS } else { "" }));
    s.push_str("<div class=\"mp-bar\"><button class=\"mp-btn\" id=\"mp-table\" type=\"button\">Table</button><button class=\"mp-btn\" id=\"mp-theme\" type=\"button\">Dark</button></div>");
    if let Some((h, sub)) = intro {
        s.push_str(&format!("<div class=\"mp-head\"><h1>{}</h1><p>{}</p></div>", esc(h), esc(sub)));
    } else {
        s.push_str(&format!("<h1 class=\"mp-sr\">{}</h1>", esc(title)));
    }
    s.push_str(if grid { "<div class=\"mp-grid\">" } else { "<div>" });
    let mut list: Vec<Value> = Vec::new();
    for (i, c) in charts.iter().enumerate() {
        let id = format!("mp-c{}", i);
        s.push_str(&format!("<div class=\"mp-card\"><div class=\"mp-chart\" id=\"{id}\" role=\"img\" aria-label=\"{}\"></div><div class=\"mp-tablewrap{}\" id=\"{id}-table\"{}>{}</div></div>", esc(&c.out.title), if c.out.table_open { " open" } else { "" }, if c.out.table_open { "" } else { " hidden" }, c.out.table));
        let mut entry = json!({"id": id, "name": c.name, "height": c.out.height, "light": c.out.light, "dark": c.out.dark});
        if c.out.table_open {
            entry["open"] = json!(true);
        }
        list.push(entry);
    }
    s.push_str("</div></div>");
    let data = json!({"charts": list, "themes": {"light": theme_json(false), "dark": theme_json(true)}, "followSystem": false});
    s.push_str(&format!("<script id=\"mp-data\" type=\"application/json\">{}</script>", script_json(&data)));
    s.push_str("<script>");
    s.push_str(ECHARTS_JS);
    s.push_str("</script><script>");
    s.push_str(PAGE_JS);
    if any_open {
        s.push_str(OPEN_TABLE_JS);
    }
    s.push_str("</script></body></html>\n");
    s
}

/// The one self-contained HTML file of a chart.
pub fn chart_html(data: &ChartData, opts: &LayoutOpts) -> Result<String> {
    let out = chart_pair(data, opts)?;
    let name = file_stem(&out.title);
    let title = out.title.clone();
    Ok(page(&title, None, &[PageChart { out, name }], false))
}

/// A chart of a book: the data, how its numbers are written, and (when set) the name it has in the book.
pub struct BookItem<'a> {
    pub data: &'a ChartData,
    pub opts: LayoutOpts,
}

/// Several charts on one page: a title block, the date, then a two-column grid (one column on a narrow window).
pub fn chart_book_html(title: &str, date: &str, items: &[BookItem]) -> Result<String> {
    if items.is_empty() {
        bail!("The chart book has no charts. Choose at least one saved chart.");
    }
    let mut charts = Vec::new();
    for (i, it) in items.iter().enumerate() {
        let out = chart_pair(it.data, &it.opts).map_err(|e| anyhow::anyhow!("Chart {} of the book ({}): {}", i + 1, t::tidy(&chart_title(it.data)), e))?;
        let name = file_stem(&out.title);
        charts.push(PageChart { out, name });
    }
    Ok(page(title, Some((title, date)), &charts, true))
}

/// A file-name-safe version of a title.
pub fn file_stem(title: &str) -> String {
    let s: String = title.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { ' ' }).collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let s: String = s.chars().take(60).collect();
    if s.is_empty() {
        "chart".to_string()
    } else {
        s
    }
}

/// Writes a chart's HTML file. Nothing is written when the chart is refused.
pub fn write_chart_html(path: &Path, data: &ChartData, opts: &LayoutOpts) -> Result<()> {
    let html = chart_html(data, opts)?;
    std::fs::write(path, html).map_err(|e| anyhow::anyhow!("Could not write {}: {}. Choose another folder, or close the file if it is open.", path.display(), e))
}

/// Writes a chart book's HTML file. Nothing is written when any chart is refused.
pub fn write_chart_book(path: &Path, title: &str, date: &str, items: &[BookItem]) -> Result<()> {
    let html = chart_book_html(title, date, items)?;
    std::fs::write(path, html).map_err(|e| anyhow::anyhow!("Could not write {}: {}. Choose another folder, or close the file if it is open.", path.display(), e))
}

/// The folder of the previews: `<temp_dir>/preview`, inside the app's own temporary folder, never the system's.
pub fn preview_dir(temp_dir: &Path) -> PathBuf {
    temp_dir.join("preview")
}

/// A file for "Preview in browser": the chart's HTML in `<temp_dir>/preview/chart-preview.html`. The previous preview
/// is removed first, so there is only ever one.
pub fn write_preview(data: &ChartData, opts: &LayoutOpts, temp_dir: &Path) -> Result<PathBuf> {
    let html = chart_html(data, opts)?;
    let dir = preview_dir(temp_dir);
    if dir.exists() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    std::fs::create_dir_all(&dir).map_err(|e| anyhow::anyhow!("Could not make the preview folder {}: {}.", dir.display(), e))?;
    let path = dir.join("chart-preview.html");
    std::fs::write(&path, html).map_err(|e| anyhow::anyhow!("Could not write the preview file {}: {}.", path.display(), e))?;
    Ok(path)
}

/// Removes what the chart studio leaves in the temporary folder: the previews and the tables of "Open chart data
/// as tab", "Open as chart tab" and the templates. Run at start-up (what an earlier run left) and at exit.
pub fn clean_chart_temp(temp_dir: &Path) {
    let _ = std::fs::remove_dir_all(preview_dir(temp_dir));
    let Ok(rd) = std::fs::read_dir(temp_dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if ["chart_data_", "chart_tab_", "chart_template_"].iter().any(|p| name.starts_with(p)) && name.ends_with(".parquet") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Opens a file with the system's default program (the default browser for an HTML file). Nothing goes
/// over the network: the file is local and the page loads nothing.
pub fn open_in_browser(path: &Path) -> Result<()> {
    use std::process::Command;
    #[cfg(target_os = "windows")]
    let status = Command::new("cmd").args(["/C", "start", ""]).arg(path).spawn();
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(path).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let status = Command::new("xdg-open").arg(path).spawn();
    match status {
        Ok(_) => Ok(()),
        Err(e) => bail!("Could not open the browser ({}). The preview file is at {}; open it by double-clicking it.", e, path.display()),
    }
}
