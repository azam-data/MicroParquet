//! The PowerPoint export (Phase 10 X1): one item per slide, written with the `zip` crate (Deflate).
//!
//! A chart is a real DrawingML chart part with an embedded workbook, so it can be edited in PowerPoint; a pivot is a native table;
//! a KPI card is shapes and text. Every number comes from `ItemData` (the one compute path) and is shown the way the app shows it:
//! money in the item's unit, counts whole, averages and shares as they are. A chart this file cannot draw as a chart is a table with a
//! line saying so, never a different chart.

use crate::charts::data::{waterfall_from_data, ChartData, StepKind};
use crate::charts::spec::{ChartKind, Stack};
use crate::dashboard::*;
use crate::kpi::{KpiSpec, KpiUnit, ThresholdColour, Tone};
use crate::models::format_with_commas;
use crate::report_html::{measure_scale, measure_unit};
use anyhow::{bail, Context, Result};
use std::io::{Cursor, Write};

/// The slide size in EMU (13.333 × 7.5 in).
pub const SLIDE_W: i64 = 12_192_000;
pub const SLIDE_H: i64 = 6_858_000;
/// A package over this many bytes is refused (the zip format's own limit without zip64).
pub const MAX_PACKAGE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// Rows of a pivot per slide; a longer pivot continues on the next slide.
pub const TABLE_ROWS_PER_SLIDE: usize = 16;

/// One slide: an item with its numbers.
#[derive(Clone, Debug)]
pub struct PptxSlide {
    pub title: String,
    pub data: ItemData,
    /// The money unit of the item: divisor, decimals, short name ("Cr").
    pub scale: (f64, usize, String),
    /// A KPI card's settings and the dashboard's default money unit.
    pub kpi: Option<(KpiSpec, KpiUnit)>,
    /// "As of 31 Oct 2026 · Filters: Wing = Islami" (see `footnote`).
    pub footnote: String,
}

#[derive(Clone, Debug)]
pub struct PptxPlan {
    pub title: String,
    pub slides: Vec<PptxSlide>,
    /// The design picture (PNG) as the slides' background, when the owner picked "Use design as slide background".
    pub background: Option<Vec<u8>>,
}

/// The footnote of a slide: the as-of date and the filters in force.
pub fn footnote(as_of: &str, filters: &[String], notes: &[String]) -> String {
    let f = if filters.is_empty() { "none".to_string() } else { filters.join("; ") };
    let mut s = format!("As of {} \u{b7} Filters: {}", as_of, f);
    for n in notes {
        s.push_str(" \u{b7} ");
        s.push_str(n);
    }
    s
}

// ------------------------------------------------------------------------------------------------ what a slide holds

pub(crate) enum Body {
    Chart(ChartPart),
    Table { head: Vec<String>, rows: Vec<Vec<String>>, indent: Vec<usize>, bold: Vec<bool>, note: Option<String> },
    Kpi { value: String, value_colour: Option<u32>, change: String, change_colour: Option<u32>, words: String, period: String, reason: Option<String>, spark: Option<ChartPart> },
    Note(String),
    /// A text box of the dashboard: one slide with the words as typed.
    Text { text: String, size_hundredths_pt: u32, bold: bool, algn: &'static str },
    /// A picture of the dashboard: one slide with the picture, as wide as it can be.
    Image { png: Vec<u8>, w: u32, h: u32, alt: String },
}

#[derive(Clone)]
pub(crate) struct Series {
    pub(crate) name: String,
    pub(crate) values: Vec<Option<f64>>,
    pub(crate) colour: u32,
    /// Waterfall's invisible base series.
    pub(crate) hidden: bool,
}

#[derive(Clone)]
pub(crate) struct ChartPart {
    pub(crate) kind: &'static str,
    pub(crate) grouping: &'static str,
    pub(crate) horizontal: bool,
    pub(crate) categories: Vec<String>,
    pub(crate) series: Vec<Series>,
    pub(crate) unit: String,
    pub(crate) numfmt: &'static str,
    /// The series (by index) drawn against a second value axis on the right, in `unit2`: measures in another unit than the first.
    pub(crate) secondary: Vec<usize>,
    pub(crate) unit2: String,
    /// A card's sparkline: no axes, gridlines or legend.
    pub(crate) mini: bool,
}

fn plain(s: &str) -> String {
    s.to_string()
}

fn colour_of(i: usize) -> u32 {
    crate::charts::tokens::series_colour(i.min(crate::charts::tokens::SLOTS - 1), false).unwrap_or(0x2a78d6)
}

fn money_fmt(unit: &str) -> &'static str {
    if unit.is_empty() {
        "#,##0"
    } else {
        "#,##0.00"
    }
}

fn scaled(v: Option<f64>, div: f64) -> Option<f64> {
    v.filter(|x| x.is_finite()).map(|x| x / div)
}

fn table_of_chart(c: &ChartData, scale: &(f64, usize, String), why: &str) -> Body {
    let (div, dec) = measure_scale(&c.spec, 0, scale);
    let head: Vec<String> = std::iter::once(String::new()).chain(c.series.iter().map(|s| if s.is_empty() { c.measure_names.first().cloned().unwrap_or_default() } else { s.clone() })).collect();
    let rows: Vec<Vec<String>> = c
        .categories
        .iter()
        .enumerate()
        .map(|(i, cat)| std::iter::once(cat.clone()).chain(c.values.iter().map(|r| r.get(i).copied().flatten().map(|v| format_with_commas(v / div, dec)).unwrap_or_default())).collect())
        .collect();
    let n = rows.len();
    Body::Table { head, rows, indent: vec![0; n], bold: vec![false; n], note: Some(why.to_string()) }
}

fn chart_body(c: &ChartData, scale: &(f64, usize, String)) -> Body {
    let spec = &c.spec;
    match spec.kind {
        ChartKind::Bar | ChartKind::Line | ChartKind::Area => {
            let m = c.measure_names.len().max(1);
            let units: Vec<String> = (0..m).map(|k| measure_unit(spec, k, scale)).collect();
            let mut series = Vec::new();
            let mut secondary: Vec<usize> = Vec::new();
            for (k, vals) in std::iter::once(&c.values).chain(c.extra.iter().map(|e| &e.values)).enumerate() {
                let (div, _) = measure_scale(spec, k, scale);
                for (j, row) in vals.iter().enumerate() {
                    let s = c.series.get(j).cloned().unwrap_or_default();
                    let mname = c.measure_names.get(k).cloned().unwrap_or_default();
                    let name = if m > 1 { if s.is_empty() { mname } else { format!("{} \u{b7} {}", s, mname) } } else if s.is_empty() { mname } else { s };
                    if units[k] != units[0] {
                        secondary.push(series.len());
                    }
                    series.push(Series { name, values: row.iter().map(|v| scaled(*v, div)).collect(), colour: colour_of(series.len()), hidden: false });
                }
            }
            let unit2 = units.iter().find(|u| **u != units[0]).cloned().unwrap_or_default();
            let kind = match spec.kind {
                ChartKind::Bar => "bar",
                ChartKind::Line => "line",
                _ => "area",
            };
            let grouping = match (kind, spec.stack) {
                ("line", _) => "standard",
                (_, Stack::Grouped) => "clustered",
                (_, Stack::Stacked) => "stacked",
                (_, Stack::Percent) => "percentStacked",
            };
            let grouping = if kind == "area" && grouping == "clustered" { "standard" } else { grouping };
            Body::Chart(ChartPart { kind, grouping, horizontal: spec.horizontal && kind == "bar", categories: c.categories.clone(), series, unit: units[0].clone(), numfmt: if units[0] == "%" { "#,##0.0" } else { money_fmt(&units[0]) }, secondary, unit2, mini: false })
        }
        ChartKind::Donut => {
            let (div, _) = measure_scale(spec, 0, scale);
            let row = c.values.first().cloned().unwrap_or_default();
            Body::Chart(ChartPart {
                kind: "donut",
                grouping: "standard",
                horizontal: false,
                categories: c.categories.clone(),
                series: vec![Series { name: c.measure_names.first().cloned().unwrap_or_default(), values: row.iter().map(|v| scaled(*v, div)).collect(), colour: colour_of(0), hidden: false }],
                unit: measure_unit(spec, 0, scale),
                numfmt: money_fmt(&measure_unit(spec, 0, scale)),
                secondary: Vec::new(),
                unit2: String::new(),
                mini: false,
            })
        }
        ChartKind::Histogram => match &c.hist {
            Some(h) => {
                let cats: Vec<String> = h.counts.iter().enumerate().map(|(i, _)| format!("{} \u{2013} {}", format_with_commas(h.edges[i], 2), format_with_commas(h.edges.get(i + 1).copied().unwrap_or(h.edges[i]), 2))).collect();
                Body::Chart(ChartPart { kind: "bar", grouping: "clustered", horizontal: false, categories: cats, series: vec![Series { name: "Count".into(), values: h.counts.iter().map(|n| Some(*n as f64)).collect(), colour: colour_of(0), hidden: false }], unit: plain("Count"), numfmt: "#,##0", secondary: Vec::new(), unit2: String::new(), mini: false })
            }
            None => Body::Note("The histogram has no bins.".into()),
        },
        ChartKind::Waterfall => match waterfall_from_data(c) {
            Err(e) => Body::Note(e.to_string()),
            Ok(steps) => {
                let (div, _) = measure_scale(spec, 0, scale);
                if steps.iter().any(|w| w.base < 0.0 || w.top < 0.0) {
                    return table_of_chart(c, scale, "Shown as a table: the bridge crosses zero, which a stacked PowerPoint chart cannot draw.");
                }
                let n = steps.len();
                let (mut base, mut total, mut up, mut down) = (vec![None; n], vec![None; n], vec![None; n], vec![None; n]);
                for (i, w) in steps.iter().enumerate() {
                    base[i] = Some(w.base / div);
                    let height = Some((w.top - w.base) / div);
                    match (w.kind, w.value >= 0.0) {
                        (StepKind::Start | StepKind::End, _) => total[i] = height,
                        (StepKind::Delta, true) => up[i] = height,
                        (StepKind::Delta, false) => down[i] = height,
                    }
                }
                let s = |name: &str, values: Vec<Option<f64>>, colour: u32, hidden: bool| Series { name: name.into(), values, colour, hidden };
                Body::Chart(ChartPart {
                    kind: "bar",
                    grouping: "stacked",
                    horizontal: false,
                    categories: steps.iter().map(|w| w.label.clone()).collect(),
                    series: vec![s("Base", base, 0xffffff, true), s("Total", total, colour_of(0), false), s("Increase", up, crate::charts::tokens::STATUS_GOOD, false), s("Decrease", down, crate::charts::tokens::STATUS_CRITICAL, false)],
                    unit: measure_unit(spec, 0, scale),
                    numfmt: money_fmt(&measure_unit(spec, 0, scale)),
                    secondary: Vec::new(),
                    unit2: String::new(),
                    mini: false,
                })
            }
        },
        ChartKind::Heatmap => {
            let tf = spec.measure_list().first().map(|m| m.transform).unwrap_or(crate::charts::spec::Transform::None);
            let hm = crate::charts::data::heatmap(c, tf);
            let (div, dec) = measure_scale(spec, 0, scale);
            let head: Vec<String> = std::iter::once(String::new()).chain(hm.cols.iter().cloned()).collect();
            let rows: Vec<Vec<String>> = hm.rows.iter().enumerate().map(|(i, r)| std::iter::once(r.clone()).chain(hm.cells[i].iter().map(|v| v.map(|x| format_with_commas(x / div, dec)).unwrap_or_default())).collect()).collect();
            let n = rows.len();
            Body::Table { head, rows, indent: vec![0; n], bold: vec![false; n], note: Some("A heatmap is a table of its cells in PowerPoint.".into()) }
        }
        _ => table_of_chart(c, scale, "Shown as a table: PowerPoint has no chart of this type here."),
    }
}

fn table_body(t: &TableData, scale: &(f64, usize, String)) -> Body {
    let unit = ItemUnit::ALL.iter().copied().find(|u| u.short() == scale.2).unwrap_or_default();
    let cell = |s: &polars::prelude::Series, i: usize, k: usize| -> String {
        use polars::prelude::AnyValue;
        let (div, dec) = match t.kind_of(k) {
            CellKind::Money => (scale.0, 2),
            other => cell_scale(other, unit),
        };
        match s.get(i) {
            Ok(AnyValue::Float64(v)) => format_with_commas(v / div, dec),
            Ok(AnyValue::Float32(v)) => format_with_commas(v as f64 / div, dec),
            Ok(AnyValue::Null) | Err(_) => String::new(),
            Ok(AnyValue::String(v)) => v.to_string(),
            Ok(AnyValue::StringOwned(v)) => v.to_string(),
            Ok(v) => format!("{}", v),
        }
    };
    let heading = |name: &str, k: usize| match t.kind_of(k) {
        CellKind::Money => format!("{} ({})", name, scale.2),
        CellKind::Share => format!("{} (%)", name),
        _ => name.to_string(),
    };
    match &t.nested {
        Some(n) => {
            let head: Vec<String> = std::iter::once(n.label_header.clone()).chain(n.values.get_columns().iter().enumerate().map(|(k, c)| heading(c.name(), k))).collect();
            let mut rows = Vec::new();
            let (mut indent, mut bold) = (Vec::new(), Vec::new());
            for (i, r) in n.rows.iter().enumerate() {
                if r.label.is_empty() {
                    continue;
                }
                rows.push(std::iter::once(r.label.clone()).chain(n.values.get_columns().iter().enumerate().map(|(k, c)| cell(c, i, k))).collect());
                indent.push(r.level);
                bold.push(r.is_subtotal() || r.label == crate::pivot_table::GRAND_TOTAL_LABEL);
            }
            Body::Table { head, rows, indent, bold, note: None }
        }
        None => {
            let cols = t.df.get_columns();
            let head: Vec<String> = cols.iter().enumerate().map(|(ci, c)| if ci < t.label_columns { c.name().to_string() } else { heading(c.name(), ci - t.label_columns) }).collect();
            let rows: Vec<Vec<String>> = (0..t.df.height()).map(|i| cols.iter().enumerate().map(|(ci, c)| cell(c, i, ci.saturating_sub(t.label_columns))).collect()).collect();
            let n = rows.len();
            Body::Table { head, rows, indent: vec![0; n], bold: vec![false; n], note: None }
        }
    }
}

fn kpi_body(k: &crate::kpi::KpiValue, spec: &KpiSpec, default_unit: KpiUnit) -> Body {
    let v = crate::kpi::view(spec, k, default_unit);
    let status = |c: ThresholdColour| match c {
        ThresholdColour::Amber => crate::charts::tokens::STATUS_WARNING,
        ThresholdColour::Red => crate::charts::tokens::STATUS_CRITICAL,
        ThresholdColour::Green => crate::charts::tokens::STATUS_GOOD,
    };
    let change = if v.change_text.is_empty() { String::new() } else if v.arrow.is_empty() { v.change_text.clone() } else { format!("{} {}", v.arrow, v.change_text) };
    Body::Kpi {
        value: v.value_text,
        value_colour: v.threshold.map(status),
        change,
        change_colour: match v.tone {
            Tone::Good => Some(crate::charts::tokens::STATUS_GOOD),
            Tone::Bad => Some(crate::charts::tokens::STATUS_CRITICAL),
            Tone::Neutral => None,
        },
        words: if v.words.is_empty() { String::new() } else { format!("{} against the earlier period", v.words) },
        period: k.period.clone(),
        reason: v.reason,
        spark: if v.spark.iter().flatten().count() > 1 {
            let labels: Vec<String> = if k.spark_labels.len() == v.spark.len() { k.spark_labels.clone() } else { (1..=v.spark.len()).map(|i| i.to_string()).collect() };
            Some(ChartPart { kind: "line", grouping: "standard", horizontal: false, categories: labels, series: vec![Series { name: spec.title.clone(), values: v.spark.clone(), colour: colour_of(0), hidden: false }], unit: String::new(), numfmt: "#,##0.00", secondary: Vec::new(), unit2: String::new(), mini: true })
        } else {
            None
        },
    }
}

pub(crate) fn body_for(data: &ItemData, scale: &(f64, usize, String), kpi: &Option<(KpiSpec, KpiUnit)>) -> Body {
    match data {
        ItemData::Chart(c) => chart_body(c, scale),
        ItemData::Table(t) => table_body(t, scale),
        ItemData::Kpi(k) => match kpi {
            Some((spec, unit)) => kpi_body(k, spec, *unit),
            None => Body::Note("This card has no settings.".into()),
        },
        ItemData::Text(t) => Body::Text {
            text: t.text.clone(),
            size_hundredths_pt: match t.size {
                crate::dashboard::TextSize::Small => 1400,
                crate::dashboard::TextSize::Medium => 2000,
                crate::dashboard::TextSize::Large => 3200,
                crate::dashboard::TextSize::Title => 4800,
            },
            bold: t.bold,
            algn: match t.align {
                crate::dashboard::TextAlign::Left => "l",
                crate::dashboard::TextAlign::Centre => "ctr",
                crate::dashboard::TextAlign::Right => "r",
            },
        },
        ItemData::Image(i) => match i.png() {
            Ok(png) => Body::Image { png, w: i.w, h: i.h, alt: i.name.clone() },
            Err(e) => Body::Note(format!("{:#}", e)),
        },
        ItemData::Problem(m) => Body::Note(m.clone()),
    }
}

// ------------------------------------------------------------------------------------------------ XML

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn hex6(c: u32) -> String {
    format!("{:06X}", c & 0xffffff)
}

const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const NS_C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

fn col_letter(i: usize) -> char {
    (b'A' + i as u8) as char
}

fn str_cache(vals: &[String]) -> String {
    format!("<c:ptCount val=\"{}\"/>{}", vals.len(), vals.iter().enumerate().map(|(i, v)| format!("<c:pt idx=\"{}\"><c:v>{}</c:v></c:pt>", i, esc(v))).collect::<String>())
}

fn chart_xml(c: &ChartPart) -> String {
    let n = c.categories.len();
    let cat_ref = format!("Sheet1!$A$2:$A${}", n + 1);
    let ser_xml = |i: usize, s: &Series| -> String {
        let col = col_letter(i + 1);
        let fill = if s.hidden { "<a:noFill/>".to_string() } else { format!("<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>", hex6(s.colour)) };
        let sp = match c.kind {
            "line" => format!("<c:spPr><a:ln w=\"28575\" cap=\"rnd\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill><a:round/></a:ln></c:spPr><c:marker><c:symbol val=\"none\"/></c:marker>", hex6(s.colour)),
            "donut" => String::new(),
            _ => format!("<c:spPr>{}</c:spPr>{}", fill, if c.kind == "bar" { "<c:invertIfNegative val=\"0\"/>" } else { "" }),
        };
        let dpts = if c.kind == "donut" { (0..n).map(|k| format!("<c:dPt><c:idx val=\"{}\"/><c:bubble3D val=\"0\"/><c:spPr><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></c:spPr></c:dPt>", k, hex6(colour_of(k)))).collect::<String>() } else { String::new() };
        let pts: String = s.values.iter().enumerate().filter_map(|(k, v)| v.map(|x| format!("<c:pt idx=\"{}\"><c:v>{}</c:v></c:pt>", k, x))).collect();
        format!(
            "<c:ser><c:idx val=\"{i}\"/><c:order val=\"{i}\"/><c:tx><c:strRef><c:f>Sheet1!${col}$1</c:f><c:strCache>{name}</c:strCache></c:strRef></c:tx>{sp}{dpts}<c:cat><c:strRef><c:f>{cat_ref}</c:f><c:strCache>{cats}</c:strCache></c:strRef></c:cat><c:val><c:numRef><c:f>Sheet1!${col}$2:${col}${last}</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"{n}\"/>{pts}</c:numCache></c:numRef></c:val>{smooth}</c:ser>",
            i = i,
            col = col,
            name = str_cache(&[s.name.clone()]),
            sp = sp,
            dpts = dpts,
            cat_ref = cat_ref,
            cats = str_cache(&c.categories),
            last = n + 1,
            n = n,
            pts = pts,
            smooth = if c.kind == "line" { "<c:smooth val=\"0\"/>" } else { "" }
        )
    };
    let overlap = if c.grouping == "stacked" || c.grouping == "percentStacked" { "<c:overlap val=\"100\"/>" } else { "" };
    // one chart group per value axis: the first measure's unit on the left, any other unit on the right
    let group = |sers: &str, ax: (u32, u32)| match c.kind {
        "bar" => format!("<c:barChart><c:barDir val=\"{}\"/><c:grouping val=\"{}\"/><c:varyColors val=\"0\"/>{}<c:gapWidth val=\"80\"/>{}<c:axId val=\"{}\"/><c:axId val=\"{}\"/></c:barChart>", if c.horizontal { "bar" } else { "col" }, c.grouping, sers, overlap, ax.0, ax.1),
        "line" => format!("<c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{}<c:marker val=\"1\"/><c:axId val=\"{}\"/><c:axId val=\"{}\"/></c:lineChart>", sers, ax.0, ax.1),
        "area" => format!("<c:areaChart><c:grouping val=\"{}\"/><c:varyColors val=\"0\"/>{}<c:axId val=\"{}\"/><c:axId val=\"{}\"/></c:areaChart>", c.grouping, sers, ax.0, ax.1),
        _ => format!("<c:doughnutChart><c:varyColors val=\"1\"/>{}<c:firstSliceAng val=\"0\"/><c:holeSize val=\"55\"/></c:doughnutChart>", sers),
    };
    let first: String = c.series.iter().enumerate().filter(|(i, _)| !c.secondary.contains(i)).map(|(i, s)| ser_xml(i, s)).collect();
    let second: String = c.series.iter().enumerate().filter(|(i, _)| c.secondary.contains(i)).map(|(i, s)| ser_xml(i, s)).collect();
    let mut groups = group(&first, (111, 112));
    if !second.is_empty() {
        groups.push_str(&group(&second, (113, 114)));
    }
    let axes = if c.kind == "donut" {
        String::new()
    } else {
        let (cat_pos, val_pos) = if c.horizontal { ("l", "b") } else { ("b", "l") };
        let title = |u: &str| if u.is_empty() || c.mini { String::new() } else { format!("<c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:rPr lang=\"en-US\" sz=\"1100\" b=\"0\"/><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title>", esc(u)) };
        let del = if c.mini { 1 } else { 0 };
        let grid = if c.mini { String::new() } else { "<c:majorGridlines><c:spPr><a:ln w=\"6350\"><a:solidFill><a:srgbClr val=\"E1E0D9\"/></a:solidFill></a:ln></c:spPr></c:majorGridlines>".to_string() };
        let mut x = format!(
            "<c:catAx><c:axId val=\"111\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"{del}\"/><c:axPos val=\"{cp}\"/><c:numFmt formatCode=\"General\" sourceLinked=\"0\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"low\"/><c:crossAx val=\"112\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx><c:valAx><c:axId val=\"112\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"{del}\"/><c:axPos val=\"{vp}\"/>{grid}{title}<c:numFmt formatCode=\"{fmt}\" sourceLinked=\"0\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"111\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>",
            del = del,
            cp = cat_pos,
            vp = val_pos,
            grid = grid,
            title = title(&c.unit),
            fmt = c.numfmt
        );
        if !second.is_empty() {
            x.push_str(&format!(
                "<c:catAx><c:axId val=\"113\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"1\"/><c:axPos val=\"b\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"114\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx><c:valAx><c:axId val=\"114\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"r\"/>{}<c:numFmt formatCode=\"{}\" sourceLinked=\"0\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"113\"/><c:crosses val=\"max\"/><c:crossBetween val=\"between\"/></c:valAx>",
                title(&c.unit2),
                if c.unit2 == "%" { "#,##0.0" } else if c.unit2.is_empty() { "#,##0" } else { "#,##0.00" }
            ));
        }
        x
    };
    let hide_legend = c.mini || (c.series.len() == 1 && c.kind != "donut") || c.series.iter().any(|s| s.hidden);
    let legend = if hide_legend { String::new() } else { "<c:legend><c:legendPos val=\"b\"/><c:overlay val=\"0\"/></c:legend>".to_string() };
    format!(
        "{XML}<c:chartSpace xmlns:c=\"{NS_C}\" xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\"><c:roundedCorners val=\"0\"/><c:chart><c:autoTitleDeleted val=\"1\"/><c:plotArea><c:layout/>{groups}{axes}</c:plotArea>{legend}<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart><c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz=\"1200\"/></a:pPr><a:endParaRPr lang=\"en-US\"/></a:p></c:txPr><c:externalData r:id=\"rId1\"><c:autoUpdate val=\"0\"/></c:externalData></c:chartSpace>"
    )
}

/// The workbook behind a chart: the categories in column A, one series per column from B, the header in row 1 (what the chart's
/// `Sheet1!$B$2:$B$n` references point at).
fn workbook(c: &ChartPart) -> Result<Vec<u8>> {
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("Sheet1")?;
    for (j, s) in c.series.iter().enumerate() {
        ws.write_string(0, (j + 1) as u16, s.name.as_str())?;
    }
    for (i, cat) in c.categories.iter().enumerate() {
        ws.write_string((i + 1) as u32, 0, cat.as_str())?;
        for (j, s) in c.series.iter().enumerate() {
            if let Some(Some(v)) = s.values.get(i) {
                ws.write_number((i + 1) as u32, (j + 1) as u16, *v)?;
            }
        }
    }
    Ok(wb.save_to_buffer()?)
}

fn text_box(id: usize, name: &str, x: i64, y: i64, w: i64, h: i64, paras: &str) -> String {
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap=\"square\" rtlCol=\"0\"><a:normAutofit/></a:bodyPr><a:lstStyle/>{paras}</p:txBody></p:sp>",
        id = id,
        name = esc(name),
        x = x,
        y = y,
        w = w,
        h = h,
        paras = paras
    )
}

fn para(text: &str, size: u32, bold: bool, colour: Option<u32>, algn: &str) -> String {
    let fill = colour.map(|c| format!("<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>", hex6(c))).unwrap_or_default();
    format!("<a:p><a:pPr algn=\"{}\"/><a:r><a:rPr lang=\"en-US\" sz=\"{}\" b=\"{}\" dirty=\"0\">{}</a:rPr><a:t>{}</a:t></a:r></a:p>", algn, size, bold as u8, fill, esc(text))
}

const MARGIN: i64 = 457_200;
const TITLE_H: i64 = 640_080;
const FOOT_H: i64 = 420_000;

fn table_frame(id: usize, head: &[String], rows: &[Vec<String>], indent: &[usize], bold: &[bool]) -> String {
    let body_w = SLIDE_W - 2 * MARGIN;
    let cols = head.len().max(1);
    let first = if cols == 1 { body_w } else { (body_w * 3 / 10).max(body_w / cols as i64) };
    let rest = if cols > 1 { (body_w - first) / (cols as i64 - 1) } else { 0 };
    let grid: String = (0..cols).map(|i| format!("<a:gridCol w=\"{}\"/>", if i == 0 { first } else { rest })).collect();
    let row_h = 330_000i64;
    let cell = |t: &str, b: bool, right: bool, ind: usize, fill: bool| {
        format!(
            "<a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:pPr algn=\"{}\" marL=\"{}\"/><a:r><a:rPr lang=\"en-US\" sz=\"1200\" b=\"{}\" dirty=\"0\"/><a:t>{}</a:t></a:r></a:p></a:txBody><a:tcPr marL=\"91440\" marR=\"91440\" marT=\"36000\" marB=\"36000\">{}</a:tcPr></a:tc>",
            if right { "r" } else { "l" },
            ind as i64 * 182_880,
            b as u8,
            esc(t),
            if fill { "<a:solidFill><a:srgbClr val=\"E1E0D9\"/></a:solidFill>" } else { "<a:noFill/>" }
        )
    };
    let mut trs = format!("<a:tr h=\"{}\">{}</a:tr>", row_h, head.iter().enumerate().map(|(i, h)| cell(h, true, i > 0, 0, true)).collect::<String>());
    for (r, row) in rows.iter().enumerate() {
        trs.push_str(&format!("<a:tr h=\"{}\">{}</a:tr>", row_h, row.iter().enumerate().map(|(i, v)| cell(v, bold.get(r).copied().unwrap_or(false), i > 0, if i == 0 { indent.get(r).copied().unwrap_or(0) } else { 0 }, false)).collect::<String>()));
    }
    let h = row_h * (rows.len() as i64 + 1);
    format!(
        "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"{id}\" name=\"Table\"/><p:cNvGraphicFramePr><a:graphicFrameLocks noGrp=\"1\"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></p:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\"><a:tbl><a:tblPr firstRow=\"1\"/><a:tblGrid>{grid}</a:tblGrid>{trs}</a:tbl></a:graphicData></a:graphic></p:graphicFrame>",
        id = id,
        x = MARGIN,
        y = MARGIN + TITLE_H,
        w = body_w,
        h = h,
        grid = grid,
        trs = trs
    )
}

fn chart_frame(id: usize, alt: &str, rect: (i64, i64, i64, i64)) -> String {
    format!(
        "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"{id}\" name=\"Chart\" descr=\"{alt}\"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></p:xfrm><a:graphic><a:graphicData uri=\"{NS_C}\"><c:chart xmlns:c=\"{NS_C}\" r:id=\"rId2\"/></a:graphicData></a:graphic></p:graphicFrame>",
        id = id,
        alt = esc(alt),
        x = rect.0,
        y = rect.1,
        w = rect.2,
        h = rect.3,
        NS_C = NS_C
    )
}

struct SlideOut {
    xml: String,
    chart: Option<ChartPart>,
    /// The PNG of a picture slide.
    image: Option<Vec<u8>>,
}

fn slide_xml(title: &str, foot: &str, body: &str, background: bool) -> String {
    let bg = if background { "<p:bg><p:bgPr><a:blipFill><a:blip r:embed=\"rId3\"/><a:stretch><a:fillRect/></a:stretch></a:blipFill><a:effectLst/></p:bgPr></p:bg>" } else { "" };
    let title_box = text_box(2, "Title", MARGIN, MARGIN / 2, SLIDE_W - 2 * MARGIN, TITLE_H, &para(title, 2400, true, None, "l"));
    let foot_box = text_box(3, "Footnote", MARGIN, SLIDE_H - MARGIN / 2 - FOOT_H, SLIDE_W - 2 * MARGIN, FOOT_H, &para(foot, 1100, false, Some(0x52514E), "l"));
    format!(
        "{XML}<p:sld xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p=\"{NS_P}\"><p:cSld>{bg}<p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{title_box}{body}{foot_box}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"
    )
}

fn slides_of(s: &PptxSlide, background: bool) -> Vec<SlideOut> {
    let body = body_for(&s.data, &s.scale, &s.kpi);
    match body {
        Body::Chart(c) => {
            let alt = format!("{}: chart of {} categories", s.title, c.categories.len());
            vec![SlideOut { xml: slide_xml(&s.title, &s.footnote, &chart_frame(4, &alt, (MARGIN, MARGIN + TITLE_H, SLIDE_W - 2 * MARGIN, SLIDE_H - 2 * MARGIN - TITLE_H - FOOT_H)), background), chart: Some(c), image: None }]
        }
        Body::Table { head, rows, indent, bold, note } => {
            let pages = rows.len().div_ceil(TABLE_ROWS_PER_SLIDE).max(1);
            (0..pages)
                .map(|p| {
                    let lo = p * TABLE_ROWS_PER_SLIDE;
                    let hi = (lo + TABLE_ROWS_PER_SLIDE).min(rows.len());
                    let title = if pages > 1 { format!("{} ({} of {})", s.title, p + 1, pages) } else { s.title.clone() };
                    let mut foot = s.footnote.clone();
                    if let Some(n) = &note {
                        foot = format!("{} \u{b7} {}", foot, n);
                    }
                    let frame = table_frame(4, &head, &rows[lo..hi], &indent[lo..hi], &bold[lo..hi]);
                    SlideOut { xml: slide_xml(&title, &foot, &frame, background), chart: None, image: None }
                })
                .collect()
        }
        Body::Kpi { value, value_colour, change, change_colour, words, period, reason, spark } => {
            let mut paras = para(&value, 6000, true, value_colour, "l");
            if !change.is_empty() {
                paras.push_str(&para(&change, 2800, false, change_colour, "l"));
            }
            if !words.is_empty() {
                paras.push_str(&para(&words, 1800, false, Some(0x52514E), "l"));
            }
            if let Some(r) = &reason {
                paras.push_str(&para(r, 1800, false, Some(0x52514E), "l"));
            }
            if !period.is_empty() {
                paras.push_str(&para(&period, 1800, false, Some(0x898781), "l"));
            }
            let card = format!(
                "<p:sp><p:nvSpPr><p:cNvPr id=\"4\" name=\"KPI card\" descr=\"{}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"roundRect\"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val=\"FCFCFB\"/></a:solidFill><a:ln w=\"12700\"><a:solidFill><a:srgbClr val=\"C3C2B7\"/></a:solidFill></a:ln></p:spPr><p:txBody><a:bodyPr wrap=\"square\" lIns=\"365760\" anchor=\"ctr\"><a:normAutofit/></a:bodyPr><a:lstStyle/>{}</p:txBody></p:sp>",
                esc(&format!("{} {} {}", value, change, words)),
                MARGIN,
                MARGIN + TITLE_H,
                SLIDE_W - 2 * MARGIN,
                SLIDE_H - 2 * MARGIN - TITLE_H - FOOT_H,
                paras
            );
            // the sparkline is a small native line chart on the card, bottom right
            let (body, chart) = match spark {
                Some(c) => {
                    let frame = chart_frame(5, &format!("{}: the last {} periods", s.title, c.categories.len()), (SLIDE_W - MARGIN - 4_200_000, SLIDE_H - MARGIN - FOOT_H - 1_900_000, 3_800_000, 1_500_000));
                    (format!("{}{}", card, frame), Some(c))
                }
                None => (card, None),
            };
            vec![SlideOut { xml: slide_xml(&s.title, &s.footnote, &body, background), chart, image: None }]
        }
        Body::Text { text, size_hundredths_pt, bold, algn } => {
            let paras: String = text.split('\n').map(|line| para(line, size_hundredths_pt, bold, None, algn)).collect();
            let b = text_box(4, "Text", MARGIN, MARGIN + TITLE_H, SLIDE_W - 2 * MARGIN, SLIDE_H - 2 * MARGIN - TITLE_H - FOOT_H, &paras);
            vec![SlideOut { xml: slide_xml(&s.title, &s.footnote, &b, background), chart: None, image: None }]
        }
        Body::Image { png, w, h, alt } => {
            let (aw, ah) = ((SLIDE_W - 2 * MARGIN) as f64, (SLIDE_H - 2 * MARGIN - TITLE_H - FOOT_H) as f64);
            let k = (aw / w.max(1) as f64).min(ah / h.max(1) as f64);
            let (pw, ph) = ((w as f64 * k) as i64, (h as f64 * k) as i64);
            let (x, y) = (MARGIN + ((aw as i64 - pw) / 2), MARGIN + TITLE_H + ((ah as i64 - ph) / 2));
            let pic = format!(
                "<p:pic><p:nvPicPr><p:cNvPr id=\"4\" name=\"Picture\" descr=\"{}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"rId4\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>",
                esc(&alt), x, y, pw, ph
            );
            vec![SlideOut { xml: slide_xml(&s.title, &s.footnote, &pic, background), chart: None, image: Some(png) }]
        }
        Body::Note(m) => {
            let b = text_box(4, "Message", MARGIN, MARGIN + TITLE_H, SLIDE_W - 2 * MARGIN, 1_000_000, &para(&m, 2000, false, Some(0x52514E), "l"));
            vec![SlideOut { xml: slide_xml(&s.title, &s.footnote, &b, background), chart: None, image: None }]
        }
    }
}

const THEME: &str = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="MicroParquet"><a:themeElements><a:clrScheme name="MicroParquet"><a:dk1><a:srgbClr val="0B0B0B"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="52514E"/></a:dk2><a:lt2><a:srgbClr val="FCFCFB"/></a:lt2><a:accent1><a:srgbClr val="2A78D6"/></a:accent1><a:accent2><a:srgbClr val="E8703A"/></a:accent2><a:accent3><a:srgbClr val="0CA30C"/></a:accent3><a:accent4><a:srgbClr val="FAB219"/></a:accent4><a:accent5><a:srgbClr val="D8537F"/></a:accent5><a:accent6><a:srgbClr val="3A2196"/></a:accent6><a:hlink><a:srgbClr val="2A78D6"/></a:hlink><a:folHlink><a:srgbClr val="3A2196"/></a:folHlink></a:clrScheme><a:fontScheme name="MicroParquet"><a:majorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="MicroParquet"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#;

fn rels(items: &[(&str, &str, &str)]) -> String {
    format!("{XML}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{}</Relationships>", items.iter().map(|(id, ty, target)| format!("<Relationship Id=\"{}\" Type=\"{}/{}\" Target=\"{}\"/>", id, REL, ty, target)).collect::<String>())
}

/// The package. `stored` writes every entry uncompressed (to show what Deflate saves).
pub fn build_pptx_with(plan: &PptxPlan, stored: bool) -> Result<Vec<u8>> {
    if plan.slides.is_empty() {
        bail!("The dashboard has no items to put on slides. Add a chart, a pivot or a card first.");
    }
    let has_bg = plan.background.is_some();
    let mut slides: Vec<SlideOut> = Vec::new();
    for s in &plan.slides {
        slides.extend(slides_of(s, has_bg));
    }
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    let mut put = |name: &str, data: Vec<u8>| parts.push((name.to_string(), data));
    let n = slides.len();
    let mut ct = format!("{XML}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"xlsx\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/><Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/><Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/><Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>");
    let mut charts = 0usize;
    for (i, s) in slides.iter().enumerate() {
        ct.push_str(&format!("<Override PartName=\"/ppt/slides/slide{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>", i + 1));
        if s.chart.is_some() {
            charts += 1;
            ct.push_str(&format!("<Override PartName=\"/ppt/charts/chart{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawingml.chart+xml\"/>", charts));
        }
    }
    ct.push_str("</Types>");
    put("[Content_Types].xml", ct.into_bytes());
    put("_rels/.rels", rels(&[("rId1", "officeDocument", "ppt/presentation.xml"), ("rId2", "metadata/core-properties", "docProps/core.xml")]).replace(&format!("{}/metadata/core-properties", REL), "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties").into_bytes());
    put("docProps/core.xml", format!("{XML}<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title><dc:creator>MicroParquet</dc:creator></cp:coreProperties>", esc(&plan.title)).into_bytes());
    let sld_ids: String = (0..n).map(|i| format!("<p:sldId id=\"{}\" r:id=\"rId{}\"/>", 256 + i, i + 2)).collect();
    put("ppt/presentation.xml", format!("{XML}<p:presentation xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p=\"{NS_P}\"><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst><p:sldIdLst>{sld_ids}</p:sldIdLst><p:sldSz cx=\"{SLIDE_W}\" cy=\"{SLIDE_H}\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>").into_bytes());
    let mut prels: Vec<(String, &str, String)> = vec![("rId1".into(), "slideMaster", "slideMasters/slideMaster1.xml".into())];
    for i in 0..n {
        prels.push((format!("rId{}", i + 2), "slide", format!("slides/slide{}.xml", i + 1)));
    }
    prels.push((format!("rId{}", n + 2), "theme", "theme/theme1.xml".into()));
    let prels_ref: Vec<(&str, &str, &str)> = prels.iter().map(|(a, b, c)| (a.as_str(), *b, c.as_str())).collect();
    put("ppt/_rels/presentation.xml.rels", rels(&prels_ref).into_bytes());
    put("ppt/theme/theme1.xml", format!("{XML}{THEME}").into_bytes());
    put("ppt/slideMasters/slideMaster1.xml", format!("{XML}<p:sldMaster xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p=\"{NS_P}\"><p:cSld><p:bg><p:bgRef idx=\"1001\"><a:schemeClr val=\"bg1\"/></p:bgRef></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld><p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst></p:sldMaster>").into_bytes());
    put("ppt/slideMasters/_rels/slideMaster1.xml.rels", rels(&[("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml"), ("rId2", "theme", "../theme/theme1.xml")]).into_bytes());
    put("ppt/slideLayouts/slideLayout1.xml", format!("{XML}<p:sldLayout xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p=\"{NS_P}\" type=\"blank\" preserve=\"1\"><p:cSld name=\"Blank\"><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>").into_bytes());
    put("ppt/slideLayouts/_rels/slideLayout1.xml.rels", rels(&[("rId1", "slideMaster", "../slideMasters/slideMaster1.xml")]).into_bytes());
    if let Some(png) = &plan.background {
        put("ppt/media/image1.png", png.clone());
    }
    let mut chart_no = 0usize;
    for (i, s) in slides.iter().enumerate() {
        put(&format!("ppt/slides/slide{}.xml", i + 1), s.xml.clone().into_bytes());
        let mut r: Vec<(String, &str, String)> = vec![("rId1".into(), "slideLayout", "../slideLayouts/slideLayout1.xml".into())];
        if let Some(c) = &s.chart {
            chart_no += 1;
            r.push(("rId2".into(), "chart", format!("../charts/chart{}.xml", chart_no)));
            put(&format!("ppt/charts/chart{}.xml", chart_no), chart_xml(c).into_bytes());
            put(&format!("ppt/charts/_rels/chart{}.xml.rels", chart_no), rels(&[("rId1", "package", &format!("../embeddings/Microsoft_Excel_Sheet{}.xlsx", chart_no))]).into_bytes());
            put(&format!("ppt/embeddings/Microsoft_Excel_Sheet{}.xlsx", chart_no), workbook(c).with_context(|| format!("The workbook behind the chart on slide {} could not be written", i + 1))?);
        }
        if has_bg {
            r.push(("rId3".into(), "image", "../media/image1.png".into()));
        }
        if let Some(png) = &s.image {
            put(&format!("ppt/media/slide{}.png", i + 1), png.clone());
            r.push(("rId4".into(), "image", format!("../media/slide{}.png", i + 1)));
        }
        let rr: Vec<(&str, &str, &str)> = r.iter().map(|(a, b, c)| (a.as_str(), *b, c.as_str())).collect();
        put(&format!("ppt/slides/_rels/slide{}.xml.rels", i + 1), rels(&rr).into_bytes());
    }
    let total: u64 = parts.iter().map(|(_, d)| d.len() as u64).sum();
    if total > MAX_PACKAGE_BYTES {
        bail!("The presentation would be {:.1} GB, over the 4 GB a PowerPoint file can hold. Put fewer items or fewer pivot rows in it.", total as f64 / 1_073_741_824.0);
    }
    let method = if stored { zip::CompressionMethod::Stored } else { zip::CompressionMethod::Deflated };
    let opts = zip::write::SimpleFileOptions::default().compression_method(method);
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in &parts {
        z.start_file(name.as_str(), opts).with_context(|| format!("The part {} could not be added to the presentation", name))?;
        z.write_all(data)?;
    }
    Ok(z.finish()?.into_inner())
}

pub fn build_pptx(plan: &PptxPlan) -> Result<Vec<u8>> {
    build_pptx_with(plan, false)
}

/// Writes the presentation to `path` through a temporary file, so a failure leaves no half-written file.
pub fn write_pptx(path: &std::path::Path, plan: &PptxPlan) -> Result<()> {
    let bytes = build_pptx(plan)?;
    let tmp = path.with_extension("pptx.part");
    std::fs::write(&tmp, &bytes).with_context(|| format!("The file {} could not be written. Close it in PowerPoint and try again.", path.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("The file {} could not be replaced. Close it in PowerPoint and try again.", path.display()))?;
    Ok(())
}
