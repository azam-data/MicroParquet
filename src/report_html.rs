//! The HTML report engine (Phase 10 R1): for each page of a dashboard it computes the table of answers the file will hold (one per
//! page-filter state and drill path of each item, all from `dashboard::compute_item`), counts the numbers, estimates the size of the
//! file, and applies the email rule ("Will you email this file?": over 5 MB the export is refused and says what to cut). The page itself
//! is R2; here is only a stub shell that holds the library and the answers, so the estimate can be checked against a real file.
//!
//! No account rows go into the file: an answer is a group's total, never a row.

use crate::charts::data::ChartData;
use crate::charts::echarts::ECHARTS_JS;
use crate::dashboard::*;
use crate::models::*;
use anyhow::{bail, Result};

/// The most numbers one item may hold across its states and drill paths.
pub const MAX_NUMBERS_PER_ITEM: usize = 20_000;
/// A file that will be emailed may not be bigger than this.
pub const EMAIL_LIMIT_BYTES: usize = 5_000_000;
/// What a number costs in the answers' JSON, to predict the size before the answers are computed.
pub const BYTES_PER_NUMBER: usize = 8;

/// An item to put in the file: what `compute_item` needs, and how its numbers are shown.
#[derive(Clone, Debug)]
pub struct ReportItem {
    pub id: String,
    pub title: String,
    pub source: ItemSource,
    /// The project key of the data tab (a page filter reaches the item when it is the filter's tab).
    pub data_tab: Option<String>,
    /// A chart's divisor, decimals and unit (Crore: 10,000,000, 2, "Crore").
    pub scale: (f64, usize, String),
}

#[derive(Clone, Debug)]
pub struct ReportPage {
    pub id: String,
    pub title: String,
    pub filters: Vec<PageFilter>,
    /// The values of each filter, in the order the dropdown lists them.
    pub values: Vec<Vec<String>>,
    pub items: Vec<ReportItem>,
}

#[derive(Clone, Debug)]
pub struct ReportPlan {
    pub title: String,
    pub pages: Vec<ReportPage>,
    /// The size of the design picture as a data URI in the file (0 without one).
    pub template_bytes: usize,
    pub temp_dir: std::path::PathBuf,
}

/// One answer: the item as the reader sees it for these filter picks and this drill path.
#[derive(Clone, Debug)]
pub struct Answer {
    /// One entry per filter of the page: `None` is "All" (and every filter that does not reach the item).
    pub picks: Vec<Option<String>>,
    /// The values clicked on the way down (empty at the top).
    pub path: Vec<String>,
    pub data: ItemData,
}

#[derive(Clone, Debug)]
pub struct ItemAnswers {
    pub item_id: String,
    /// The filters (by index in the page) that reach this item.
    pub filters: Vec<usize>,
    pub answers: Vec<Answer>,
    pub numbers: usize,
    /// `Ok` when a click on another item can filter this one in the file; otherwise why not ("Not linked to clicks").
    pub linked: std::result::Result<(), String>,
    /// The item as it looks while another item's bar is clicked (R2): one answer per clicked value and filter state.
    pub cross: Vec<CrossAnswer>,
}

/// An answer to a click: the item with the clicked value of `src` (an item's id) added to the page's filters.
#[derive(Clone, Debug)]
pub struct CrossAnswer {
    pub src: String,
    pub value: String,
    pub answer: Answer,
}

#[derive(Clone, Debug)]
pub struct PageAnswers {
    pub page_id: String,
    pub items: Vec<ItemAnswers>,
}

/// States of filters with these numbers of values: each filter has "All" and its values, so the product of (values + 1).
pub fn state_count(filter_sizes: &[usize]) -> usize {
    filter_sizes.iter().map(|n| n + 1).product()
}

/// Every state of the filters, "All" (`None`) first in each: the last filter changes fastest.
pub fn states(values: &[&Vec<String>]) -> Vec<Vec<Option<String>>> {
    let mut out: Vec<Vec<Option<String>>> = vec![Vec::new()];
    for vs in values {
        let mut next = Vec::with_capacity(out.len() * (vs.len() + 1));
        for s in &out {
            for pick in std::iter::once(None).chain(vs.iter().map(|v| Some(v.clone()))) {
                let mut t = s.clone();
                t.push(pick);
                next.push(t);
            }
        }
        out = next;
    }
    out
}

/// How many numbers an answer holds: a chart's values, a table's value cells, a card's number and its sparkline.
pub fn numbers_of(d: &ItemData) -> usize {
    match d {
        ItemData::Chart(c) => chart_numbers(c),
        ItemData::Table(t) => match &t.nested {
            Some(n) => n.values.width() * n.values.height(),
            None => t.df.width().saturating_sub(t.label_columns) * t.df.height(),
        },
        ItemData::Kpi(k) => 1 + k.spark.len(),
        ItemData::Text(_) | ItemData::Image(_) | ItemData::Problem(_) => 0,
    }
}

fn chart_numbers(c: &ChartData) -> usize {
    c.values.iter().map(|r| r.len()).sum::<usize>() + c.extra.iter().map(|m| m.values.iter().map(|r| r.len()).sum::<usize>()).sum::<usize>()
}

/// Why a click on another item cannot filter this one in the file (it cannot be added up from parts), or `Ok` when it can.
pub fn additive(source: &ItemSource) -> std::result::Result<(), String> {
    let not_sum = |what: &str| Err(format!("{} cannot be rebuilt from clicked parts", what));
    match source {
        ItemSource::Chart { spec, .. } => {
            for m in spec.measure_list() {
                if m.time.is_some() {
                    return not_sum("a time aggregation");
                }
                if !matches!(m.agg, crate::charts::spec::ChartAgg::Sum | crate::charts::spec::ChartAgg::Count) {
                    return not_sum(&format!("'{}'", m.agg.label()));
                }
                if m.transform != crate::charts::spec::Transform::None {
                    return not_sum("a share or running total");
                }
            }
            Ok(())
        }
        ItemSource::Pivot { spec, .. } => {
            if spec.show_as != PivotShowAs::NoCalculation {
                return not_sum("a percent of a total");
            }
            for v in &spec.values {
                if v.time.is_some() {
                    return not_sum("a time aggregation");
                }
                if !matches!(v.agg, PivotAgg::Sum | PivotAgg::Count) {
                    return not_sum(&format!("'{}'", v.agg.label()));
                }
            }
            Ok(())
        }
        ItemSource::Kpi { .. } => Err("a card is one number".to_string()),
        ItemSource::Text(_) | ItemSource::Image(_) => Err("a text box or a picture shows no data".to_string()),
    }
}

fn drill_levels(source: &ItemSource) -> usize {
    match source {
        ItemSource::Chart { spec, .. } if spec.has_drill() => spec.drill.len(),
        _ => 0,
    }
}

fn too_many(title: &str, numbers: usize) -> anyhow::Error {
    anyhow::anyhow!("The item '{}' would hold {} numbers in the file; the limit is {}. Cut the filter values, put Top N on the chart, or use fewer drill levels.", title, numbers, MAX_NUMBERS_PER_ITEM)
}

/// The numbers an item will hold, worked out from its shape before any answer is computed: states × the numbers of one answer.
pub fn predict_numbers(filter_sizes: &[usize], numbers_per_answer: usize) -> usize {
    state_count(filter_sizes) * numbers_per_answer
}

/// The picks of a state, as `compute_item` takes them: the filters that have a value.
fn picks_of(page: &ReportPage, applicable: &[usize], state: &[Option<String>]) -> Vec<(PageFilter, Vec<String>)> {
    applicable.iter().zip(state).filter_map(|(i, v)| v.clone().map(|v| (page.filters[*i].clone(), vec![v]))).collect()
}

/// Computes every answer of every item of every page. Runs inside the model scope, as `compute_item` does. An item over the limit stops
/// the export with what to cut.
pub fn compute_answers(plan: &ReportPlan) -> Result<Vec<PageAnswers>> {
    let mut out = Vec::new();
    for page in &plan.pages {
        let mut items = Vec::new();
        for item in &page.items {
            let applicable: Vec<usize> = (0..page.filters.len()).filter(|i| filter_applies(&page.filters[*i], item.data_tab.as_deref())).collect();
            let vals: Vec<&Vec<String>> = applicable.iter().map(|i| &page.values[*i]).collect();
            let sizes: Vec<usize> = vals.iter().map(|v| v.len()).collect();
            // a first look at the size before any work: one number per state at least
            if state_count(&sizes) > MAX_NUMBERS_PER_ITEM {
                return Err(too_many(&item.title, state_count(&sizes)));
            }
            let levels = drill_levels(&item.source);
            let mut answers = Vec::new();
            let mut numbers = 0usize;
            for state in states(&vals) {
                let picks = picks_of(page, &applicable, &state);
                // the item at the top, then each category one level down, as deep as the item's levels go
                let mut queue: Vec<Vec<String>> = vec![Vec::new()];
                while let Some(path) = queue.pop() {
                    let req = ItemRequest { item_id: item.id.clone(), source: item.source.clone(), picks: picks.clone(), drill: path.clone(), temp_dir: plan.temp_dir.clone() };
                    let data = compute_item(&req)?;
                    numbers += numbers_of(&data);
                    if numbers > MAX_NUMBERS_PER_ITEM {
                        return Err(too_many(&item.title, numbers));
                    }
                    if let (ItemData::Chart(c), true) = (&data, path.len() + 1 < levels) {
                        for cat in c.categories.iter().filter(|x| x.as_str() != crate::chart::OTHER_LABEL) {
                            let mut p = path.clone();
                            p.push(cat.clone());
                            queue.push(p);
                        }
                    }
                    let mut full: Vec<Option<String>> = vec![None; page.filters.len()];
                    for (k, i) in applicable.iter().enumerate() {
                        full[*i] = state[k].clone();
                    }
                    answers.push(Answer { picks: full, path, data });
                }
            }
            items.push(ItemAnswers { item_id: item.id.clone(), filters: applicable, answers, numbers, linked: additive(&item.source), cross: Vec::new() });
        }
        let mut pa = PageAnswers { page_id: page.id.clone(), items };
        add_cross_answers(plan, page, &mut pa)?;
        out.push(pa);
    }
    Ok(out)
}

/// The answers to clicks (R2-1): for each chart that can be clicked (a sum or count over one dimension), each of its top-level
/// categories, and each other item that reads the same data tab and adds up: the item again with that value added to the page filters,
/// for every filter state of the item. All through `compute_item`, so a click shows exactly what the app shows.
fn add_cross_answers(plan: &ReportPlan, page: &ReportPage, pa: &mut PageAnswers) -> Result<()> {
    // the clicks: (source item index, category) from the source's top-level answer with no filter picked
    let mut clicks: Vec<(usize, CrossPick)> = Vec::new();
    for (si, (item, ia)) in page.items.iter().zip(&pa.items).enumerate() {
        let ItemSource::Chart { spec, .. } = &item.source else { continue };
        if ia.linked.is_err() {
            continue;
        }
        let Some(top) = ia.answers.iter().find(|a| a.path.is_empty() && a.picks.iter().all(|p| p.is_none())) else { continue };
        let ItemData::Chart(c) = &top.data else { continue };
        for cat in &c.categories {
            if let Ok(pick) = CrossPick::from_chart(spec, cat, &item.id, item.data_tab.as_deref()) {
                clicks.push((si, pick));
            }
        }
    }
    for (si, pick) in &clicks {
        for (ti, item) in page.items.iter().enumerate() {
            if ti == *si || pa.items[ti].linked.is_err() || item.data_tab.as_deref() != Some(pick.tab.as_str()) {
                continue;
            }
            let applicable = pa.items[ti].filters.clone();
            let vals: Vec<&Vec<String>> = applicable.iter().map(|i| &page.values[*i]).collect();
            for state in states(&vals) {
                let mut picks = picks_of(page, &applicable, &state);
                picks.push((pick.as_filter(), vec![pick.value.clone()]));
                let req = ItemRequest { item_id: item.id.clone(), source: item.source.clone(), picks, drill: Vec::new(), temp_dir: plan.temp_dir.clone() };
                let data = compute_item(&req)?;
                let ia = &mut pa.items[ti];
                ia.numbers += numbers_of(&data);
                if ia.numbers > MAX_NUMBERS_PER_ITEM {
                    return Err(too_many(&item.title, ia.numbers));
                }
                let mut full: Vec<Option<String>> = vec![None; page.filters.len()];
                for (k, i) in applicable.iter().enumerate() {
                    full[*i] = state[k].clone();
                }
                ia.cross.push(CrossAnswer { src: page.items[*si].id.clone(), value: pick.value.clone(), answer: Answer { picks: full, path: Vec::new(), data } });
            }
        }
    }
    Ok(())
}

/// The items that do not react to a click in the file, by name, with why: the export dialog lists them.
pub fn not_linked(plan: &ReportPlan, answers: &[PageAnswers]) -> Vec<String> {
    let mut v = Vec::new();
    for (page, pa) in plan.pages.iter().zip(answers) {
        for (item, ia) in page.items.iter().zip(&pa.items) {
            if let Err(why) = &ia.linked {
                v.push(format!("{} \u{b7} {}: not linked to clicks ({})", page.title, item.title, why));
            }
        }
    }
    v
}

// ------------------------------------------------------------------------------------------------ the answers as JSON

/// A number as the file holds it: in the unit it is shown in (Crore), with the decimals it is shown with, `null` when it has no value.
pub fn number_json(v: Option<f64>, divisor: f64, decimals: usize) -> String {
    match v {
        Some(x) if x.is_finite() => format!("{:.*}", decimals, x / divisor),
        _ => "null".to_string(),
    }
}

fn strings_json(v: &[String]) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string())
}

/// The divisor and decimals a chart's measure is held with in the file, and the unit it is shown in (what the app's `value_kind` says).
pub(crate) fn measure_scale(spec: &crate::charts::spec::ChartSpec, m: usize, scale: &(f64, usize, String)) -> (f64, usize) {
    use crate::charts::axis::ValueKind;
    match crate::charts::layout::value_kind(spec, m) {
        ValueKind::Amount => (scale.0, scale.1),
        ValueKind::Percent => (1.0, 1),
        ValueKind::Count => (1.0, 0),
        ValueKind::Plain => (1.0, 2),
    }
}

pub(crate) fn measure_unit(spec: &crate::charts::spec::ChartSpec, m: usize, scale: &(f64, usize, String)) -> String {
    use crate::charts::axis::ValueKind;
    match crate::charts::layout::value_kind(spec, m) {
        ValueKind::Amount => scale.2.clone(),
        ValueKind::Percent => "%".to_string(),
        ValueKind::Count | ValueKind::Plain => String::new(),
    }
}

/// The numbers of one answer as JSON text, with the commas that separate them: what `BYTES_PER_NUMBER` predicts.
pub fn values_json(a: &Answer, scale: &(f64, usize, String)) -> String {
    match &a.data {
        ItemData::Chart(c) => {
            // each measure in the unit the app shows it in: money in the item's unit, a share with one decimal, a count whole, any other number as it is
            let mut rows: Vec<String> = Vec::new();
            for (m, vals) in std::iter::once(&c.values).chain(c.extra.iter().map(|e| &e.values)).enumerate() {
                let (div, dec) = measure_scale(&c.spec, m, scale);
                rows.extend(vals.iter().map(|r| format!("[{}]", r.iter().map(|v| number_json(*v, div, dec)).collect::<Vec<_>>().join(","))));
            }
            format!("[{}]", rows.join(","))
        }
        ItemData::Table(t) => {
            // money in the item's unit, counts whole, averages and shares as they are (`cell_scale`)
            let cols: Vec<_> = match &t.nested {
                Some(n) => n.values.get_columns().iter().collect(),
                None => t.df.get_columns().iter().skip(t.label_columns).collect(),
            };
            let rows: Vec<String> = cols
                .iter()
                .enumerate()
                .map(|(k, s)| {
                    let (div, dec) = match t.kind_of(k) {
                        CellKind::Money => (scale.0, 2),
                        CellKind::Count => (1.0, 0),
                        CellKind::Plain | CellKind::Share => (1.0, 2),
                    };
                    format!("[{}]", (0..s.len()).map(|i| number_json(s.get(i).ok().and_then(|v| v.extract::<f64>()), div, dec)).collect::<Vec<_>>().join(","))
                })
                .collect();
            format!("[{}]", rows.join(","))
        }
        ItemData::Kpi(k) => format!("[[{}]]", std::iter::once(k.current).chain(k.spark.iter().copied()).map(|v| number_json(v, 1.0, 4)).collect::<Vec<_>>().join(",")),
        ItemData::Text(_) | ItemData::Image(_) | ItemData::Problem(_) => "[]".to_string(),
    }
}

fn answer_json(a: &Answer, item: &ReportItem, kpi_unit: crate::kpi::KpiUnit) -> String {
    let mut out = answer_core(a, &item.scale, &item.source, kpi_unit);
    let notes = a.data.notes();
    if !notes.is_empty() && out.ends_with('}') {
        out.pop();
        out.push_str(&format!(",\"n\":{}}}", strings_json(notes)));
    }
    out
}

fn answer_core(a: &Answer, scale: &(f64, usize, String), source: &ItemSource, kpi_unit: crate::kpi::KpiUnit) -> String {
    let picks: Vec<String> = a.picks.iter().map(|p| p.as_ref().map(|s| serde_json::to_string(s).unwrap_or_default()).unwrap_or_else(|| "null".to_string())).collect();
    match &a.data {
        ItemData::Chart(c) => {
            let units: Vec<String> = (0..c.measure_names.len().max(1)).map(|m| measure_unit(&c.spec, m, scale)).collect();
            let mut extra = String::new();
            if c.spec.kind == crate::charts::spec::ChartKind::Waterfall {
                match crate::charts::data::waterfall_from_data(c) {
                    Ok(steps) => {
                        let (div, dec) = measure_scale(&c.spec, 0, scale);
                        let rows: Vec<String> = steps
                            .iter()
                            .map(|w| {
                                let k = match w.kind {
                                    crate::charts::data::StepKind::Start => "s",
                                    crate::charts::data::StepKind::Delta => "d",
                                    crate::charts::data::StepKind::End => "e",
                                };
                                format!("[{},\"{}\",{},{},{},{}]", serde_json::to_string(&w.label).unwrap_or_default(), k, number_json(Some(w.value), div, dec), number_json(Some(w.base), div, dec), number_json(Some(w.top), div, dec), number_json(Some(w.level), div, dec))
                            })
                            .collect();
                        extra = format!(",\"wf\":[{}]", rows.join(","));
                    }
                    Err(e) => extra = format!(",\"x\":{}", serde_json::to_string(&e.to_string()).unwrap_or_default()),
                }
            }
            if c.spec.kind == crate::charts::spec::ChartKind::Heatmap {
                // the matrix as the app orders and fills it: rows are the series, columns the categories
                let tf = c.spec.measure_list().first().map(|m| m.transform).unwrap_or(crate::charts::spec::Transform::None);
                let hm = crate::charts::data::heatmap(c, tf);
                let (div, dec) = measure_scale(&c.spec, 0, scale);
                let cells: Vec<String> = hm.cells.iter().map(|r| format!("[{}]", r.iter().map(|v| number_json(*v, div, dec)).collect::<Vec<_>>().join(","))).collect();
                extra = format!(",\"hm\":{{\"rows\":{},\"cols\":{},\"cells\":[{}]}}", strings_json(&hm.rows), strings_json(&hm.cols), cells.join(","));
            }
            if let Some(h) = &c.hist {
                let edges: Vec<String> = h.edges.iter().map(|e| number_json(Some(*e), 1.0, 2)).collect();
                let counts: Vec<String> = h.counts.iter().map(|n| n.to_string()).collect();
                extra = format!(",\"hg\":{{\"edges\":[{}],\"counts\":[{}],\"below\":{},\"above\":{}}}", edges.join(","), counts.join(","), h.below, h.above);
            }
            format!("{{\"p\":[{}],\"d\":{},\"m\":{},\"mu\":{},\"c\":{},\"s\":{},\"v\":{}{}}}", picks.join(","), strings_json(&a.path), strings_json(&c.measure_names), strings_json(&units), strings_json(&c.categories), strings_json(&c.series), values_json(a, scale), extra)
        }
        ItemData::Table(t) => {
            let (labels, head): (Vec<String>, Vec<String>) = match &t.nested {
                Some(n) => (n.rows.iter().map(|r| format!("{}\u{1f}{}", r.level, r.label)).collect(), n.values.get_columns().iter().map(|c| c.name().to_string()).collect()),
                None => {
                    let n = t.label_columns.max(1);
                    let cols: Vec<_> = t.df.get_columns().iter().take(n).collect();
                    let first = (0..t.df.height()).map(|i| cols.iter().map(|c| c.get(i).map(|v| v.to_string().trim_matches('"').to_string()).unwrap_or_default()).collect::<Vec<_>>().join(" \u{b7} ")).collect();
                    (first, t.df.get_columns().iter().skip(t.label_columns).map(|c| c.name().to_string()).collect())
                }
            };
            {
                let head: Vec<String> = head
                    .iter()
                    .enumerate()
                    .map(|(k, h)| match t.kind_of(k) {
                        CellKind::Money => format!("{} ({})", h, scale.2),
                        CellKind::Share => format!("{} (%)", h),
                        CellKind::Count | CellKind::Plain => h.clone(),
                    })
                    .collect();
                format!("{{\"p\":[{}],\"d\":{},\"r\":{},\"h\":{},\"v\":{}}}", picks.join(","), strings_json(&a.path), strings_json(&labels), strings_json(&head), values_json(a, scale))
            }
        }
        ItemData::Kpi(k) => {
            let ItemSource::Kpi { spec, .. } = source else { return format!("{{\"p\":[{}],\"d\":[],\"x\":\"This card has no settings.\"}}", picks.join(",")) };
            let v = crate::kpi::view(spec, k, kpi_unit);
            let tone = match v.tone {
                crate::kpi::Tone::Good => "good",
                crate::kpi::Tone::Bad => "bad",
                crate::kpi::Tone::Neutral => "neutral",
            };
            let thr = match v.threshold {
                Some(crate::kpi::ThresholdColour::Amber) => "amber",
                Some(crate::kpi::ThresholdColour::Red) => "red",
                Some(crate::kpi::ThresholdColour::Green) => "green",
                None => "",
            };
            let spark: Vec<String> = v.spark.iter().map(|x| number_json(*x, 1.0, 4)).collect();
            format!(
                "{{\"p\":[{}],\"d\":[],\"k\":{{\"val\":{},\"chg\":{},\"arrow\":\"{}\",\"words\":\"{}\",\"tone\":\"{}\",\"thr\":\"{}\",\"why\":{},\"alt\":{},\"per\":{},\"spark\":[{}]}},\"v\":{}}}",
                picks.join(","),
                serde_json::to_string(&v.value_text).unwrap_or_default(),
                serde_json::to_string(&v.change_text).unwrap_or_default(),
                v.arrow,
                v.words,
                tone,
                thr,
                serde_json::to_string(&v.reason).unwrap_or_default(),
                serde_json::to_string(&v.alt).unwrap_or_default(),
                serde_json::to_string(&k.period).unwrap_or_default(),
                spark.join(","),
                values_json(a, scale)
            )
        }
        ItemData::Text(t) => {
            let size = match t.size {
                TextSize::Small => "s",
                TextSize::Medium => "m",
                TextSize::Large => "l",
                TextSize::Title => "t",
            };
            let align = match t.align {
                TextAlign::Left => "left",
                TextAlign::Centre => "center",
                TextAlign::Right => "right",
            };
            format!("{{\"p\":[{}],\"d\":[],\"tx\":{{\"t\":{},\"z\":\"{}\",\"b\":{},\"a\":\"{}\"}}}}", picks.join(","), serde_json::to_string(&t.text).unwrap_or_default(), size, t.bold, align)
        }
        ItemData::Image(i) => format!("{{\"p\":[{}],\"d\":[],\"im\":{{\"src\":\"data:image/png;base64,{}\",\"alt\":{}}}}}", picks.join(","), i.png_b64, serde_json::to_string(&i.name).unwrap_or_default()),
        ItemData::Problem(m) => format!("{{\"p\":[{}],\"d\":[],\"x\":{}}}", picks.join(","), serde_json::to_string(m).unwrap_or_default()),
    }
}

/// How the page draws a chart: its type, how series share a category and whether the bars lie down. A chart the page cannot draw
/// (scatter, waterfall, facets, several measures) says `"table"`, and the page shows its numbers as a table with a note.
fn chart_look(source: &ItemSource, ia: &ItemAnswers) -> String {
    use crate::charts::spec::{ChartKind, Stack};
    let ItemSource::Chart { spec, .. } = source else { return "null".to_string() };
    let many = ia.answers.iter().any(|a| matches!(&a.data, ItemData::Chart(c) if !c.facets.is_empty() || c.size.is_some()));
    let kind = match spec.kind {
        ChartKind::Bar => "bar",
        ChartKind::Line => "line",
        ChartKind::Area => "area",
        ChartKind::Donut => "donut",
        ChartKind::Waterfall => "waterfall",
        ChartKind::Heatmap => "heatmap",
        ChartKind::Histogram => "hist",
        _ => "table",
    };
    let kind = if many { "table" } else { kind };
    let stack = match spec.stack {
        Stack::Grouped => "g",
        Stack::Stacked => "s",
        Stack::Percent => "p",
    };
    format!("{{\"ct\":\"{}\",\"st\":\"{}\",\"hz\":{}}}", kind, stack, spec.horizontal)
}

/// The whole table of answers as one JSON text.
pub fn answers_json(plan: &ReportPlan, answers: &[PageAnswers]) -> String {
    answers_json_with(plan, answers, crate::kpi::KpiUnit::Crore)
}

/// The same, with the dashboard's own unit for money on the KPI cards (`Dashboard.default_kpi_unit`).
pub fn answers_json_with(plan: &ReportPlan, answers: &[PageAnswers], kpi_unit: crate::kpi::KpiUnit) -> String {
    let mut pages = Vec::new();
    for (page, pa) in plan.pages.iter().zip(answers) {
        let mut items = Vec::new();
        for (item, ia) in page.items.iter().zip(&pa.items) {
            let kind = match &item.source {
                ItemSource::Chart { .. } => "chart",
                ItemSource::Pivot { .. } => "pivot",
                ItemSource::Kpi { .. } => "kpi",
                ItemSource::Text(_) => "text",
                ItemSource::Image(_) => "image",
            };
            let list: Vec<String> = ia.answers.iter().map(|a| answer_json(a, item, kpi_unit)).collect();
            let cross: Vec<String> = ia.cross.iter().map(|x| format!("{{\"k\":[{},{}],\"a\":{}}}", serde_json::to_string(&x.src).unwrap_or_default(), serde_json::to_string(&x.value).unwrap_or_default(), answer_json(&x.answer, item, kpi_unit))).collect();
            items.push(format!(
                "{{\"id\":{},\"title\":{},\"kind\":\"{}\",\"filters\":{:?},\"linked\":{},\"unit\":{},\"look\":{},\"dim\":{},\"why\":{},\"cross\":[{}],\"answers\":[{}]}}",
                serde_json::to_string(&item.id).unwrap_or_default(),
                serde_json::to_string(&item.title).unwrap_or_default(),
                kind,
                ia.filters,
                ia.linked.is_ok(),
                serde_json::to_string(&item.scale.2).unwrap_or_default(),
                chart_look(&item.source, ia),
                serde_json::to_string(&match &item.source { ItemSource::Chart { spec, .. } => spec.category.clone(), _ => String::new() }).unwrap_or_default(),
                serde_json::to_string(&ia.linked.as_ref().err()).unwrap_or_default(),
                cross.join(","),
                list.join(",")
            ));
        }
        let filters: Vec<String> = page.filters.iter().zip(&page.values).map(|(f, v)| format!("{{\"label\":{},\"values\":{}}}", serde_json::to_string(&f.label).unwrap_or_default(), strings_json(v))).collect();
        pages.push(format!("{{\"id\":{},\"title\":{},\"filters\":[{}],\"items\":[{}]}}", serde_json::to_string(&page.id).unwrap_or_default(), serde_json::to_string(&page.title).unwrap_or_default(), filters.join(","), items.join(",")));
    }
    format!("{{\"title\":{},\"pages\":[{}]}}", serde_json::to_string(&plan.title).unwrap_or_default(), pages.join(","))
}

/// The bytes of every answer's numbers (their text and commas): the part `BYTES_PER_NUMBER` is about.
pub fn numbers_json_bytes(plan: &ReportPlan, answers: &[PageAnswers]) -> usize {
    plan.pages.iter().zip(answers).flat_map(|(p, pa)| p.items.iter().zip(&pa.items)).map(|(item, ia)| ia.answers.iter().map(|a| values_json(a, &item.scale).len()).sum::<usize>()).sum()
}

// ------------------------------------------------------------------------------------------------ the file, its size, the email rule

/// The shell of the file around the library and the answers (the page of R2 replaces the stub body, and keeps the same parts).
const SHELL_HEAD: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>";
const SHELL_MID: &str = "</title></head><body><div id=\"app\"></div><script>";
const SHELL_ANSWERS: &str = "</script><script id=\"answers\" type=\"application/json\">";
const SHELL_TAIL: &str = "</script></body></html>";

fn escape_script(s: &str) -> String {
    s.replace("</", "<\\/")
}

/// The stub file: the library and the answers in one HTML file. R2 puts the page's own code in it.
pub fn render_stub(plan: &ReportPlan, answers_json: &str) -> String {
    let mut s = String::with_capacity(ECHARTS_JS.len() + answers_json.len() + 1024);
    s.push_str(SHELL_HEAD);
    s.push_str(&escape_script(&plan.title));
    s.push_str(SHELL_MID);
    s.push_str(ECHARTS_JS);
    s.push_str(SHELL_ANSWERS);
    s.push_str(&escape_script(answers_json));
    s.push_str(SHELL_TAIL);
    s
}

// ------------------------------------------------------------------------------------------------ the page (R2)

const PAGE_CSS: &str = include_str!("../assets/report/page.css");
const PAGE_JS: &str = include_str!("../assets/report/page.js");

/// How a page sits in the file: its height in units (the width is always 1000), where each item goes (the same order as the page's
/// items, in page units) and the design picture as a data URI.
#[derive(Clone, Debug)]
pub struct PageLook {
    pub page_h: f64,
    pub rects: Vec<[f64; 4]>,
    pub background: Option<String>,
}

fn looks_json(looks: &[PageLook]) -> String {
    let pages: Vec<String> = looks.iter().map(|l| format!("{{\"h\":{},\"rects\":[{}]}}", l.page_h, l.rects.iter().map(|r| format!("[{:.2},{:.2},{:.2},{:.2}]", r[0], r[1], r[2], r[3])).collect::<Vec<_>>().join(","))).collect();
    format!("{{\"pages\":[{}]}}", pages.join(","))
}

fn designs_json(plan: &ReportPlan, looks: &[PageLook]) -> String {
    let parts: Vec<String> = plan.pages.iter().zip(looks).filter_map(|(p, l)| l.background.as_ref().map(|b| format!("{}:{}", serde_json::to_string(&p.id).unwrap_or_default(), serde_json::to_string(b).unwrap_or_default()))).collect();
    format!("{{{}}}", parts.join(","))
}

fn themes_json() -> String {
    {
    use crate::charts::tokens::{STATUS_CRITICAL, STATUS_GOOD, STATUS_SERIOUS, STATUS_WARNING};
    let hex = |c: u32| format!("\"#{:06x}\"", c);
    format!(
        "{{\"light\":{},\"dark\":{},\"status\":{{\"good\":{},\"warning\":{},\"serious\":{},\"critical\":{}}},\"seq\":{{\"light\":[{}],\"dark\":[{}]}}}}",
        crate::charts::echarts::theme_json(false),
        crate::charts::echarts::theme_json(true),
        hex(STATUS_GOOD),
        hex(STATUS_WARNING),
        hex(STATUS_SERIOUS),
        hex(STATUS_CRITICAL),
        (0..13).map(|r| hex(crate::charts::tokens::sequential_colour(r, false))).collect::<Vec<_>>().join(","),
        (0..13).map(|r| hex(crate::charts::tokens::sequential_colour(r, true))).collect::<Vec<_>>().join(",")
    )
}
}

const PAGE_HEAD: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>";

/// The finished file: the page's code, the library, the answers and the pictures in one HTML file. It makes no request: every part is
/// inside it.
pub fn render_page(plan: &ReportPlan, answers_json: &str, looks: &[PageLook]) -> Result<String> {
    if looks.len() != plan.pages.len() || plan.pages.iter().zip(looks).any(|(p, l)| p.items.len() != l.rects.len()) {
        bail!("The page layout does not match the dashboard. Open the dashboard again and export again.");
    }
    let mut s = String::with_capacity(ECHARTS_JS.len() + answers_json.len() + PAGE_JS.len() + PAGE_CSS.len() + 16_384);
    s.push_str(PAGE_HEAD);
    s.push_str(&escape_script(&plan.title));
    s.push_str("</title><style>");
    s.push_str(PAGE_CSS);
    s.push_str("</style></head><body><div id=\"app\"></div><script>");
    s.push_str(ECHARTS_JS);
    for (id, body) in [("answers", answers_json.to_string()), ("looks", looks_json(looks)), ("designs", designs_json(plan, looks)), ("themes", themes_json())] {
        s.push_str(&format!("</script><script id=\"{}\" type=\"application/json\">", id));
        s.push_str(&escape_script(&body));
    }
    s.push_str("</script><script>");
    s.push_str(PAGE_JS);
    s.push_str("</script></body></html>");
    Ok(s)
}

/// The size of `render_page`'s file before it is written (the picture is counted as the data URI the file holds).
pub fn estimate_page(plan: &ReportPlan, answers_json_len: usize, looks: &[PageLook]) -> SizeParts {
    let template: usize = looks.iter().filter_map(|l| l.background.as_ref()).map(|b| b.len() + 2).sum();
    let other = PAGE_HEAD.len() + escape_script(&plan.title).len() + PAGE_CSS.len() + PAGE_JS.len() + looks_json(looks).len() + themes_json().len() + designs_json(plan, looks).len().saturating_sub(template) + 330;
    SizeParts { echarts: ECHARTS_JS.len(), shell: other, template, answers: answers_json_len }
}

/// The parts of the file's size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SizeParts {
    pub echarts: usize,
    pub shell: usize,
    pub template: usize,
    pub answers: usize,
}

impl SizeParts {
    pub fn total(&self) -> usize {
        self.echarts + self.shell + self.template + self.answers
    }
}

/// The size before the file is written: the library, the shell (with the title), the design picture and the answers' JSON.
pub fn estimate(plan: &ReportPlan, answers_json_len: usize) -> SizeParts {
    SizeParts { echarts: ECHARTS_JS.len(), shell: SHELL_HEAD.len() + escape_script(&plan.title).len() + SHELL_MID.len() + SHELL_ANSWERS.len() + SHELL_TAIL.len(), template: plan.template_bytes, answers: answers_json_len }
}

/// The size from the shape alone, before any answer is computed: every state of every item's filters times its numbers, at 8 bytes a number.
pub fn predict_answer_bytes(shapes: &[(Vec<usize>, usize)]) -> usize {
    shapes.iter().map(|(sizes, per_answer)| predict_numbers(sizes, *per_answer) * BYTES_PER_NUMBER).sum()
}

/// What the dialog says to someone who will email the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gate {
    pub allowed: bool,
    pub bytes: usize,
    pub message: String,
    /// What to cut, in the order to try.
    pub suggestions: Vec<&'static str>,
}

/// The email question: "Will you email this file?". No: anything goes. Yes: over 5 MB (5,000,000 bytes) the Export button stays off
/// and the dialog says what to cut, in this order (what does not apply is left out).
pub fn gate(email: bool, parts: &SizeParts, has_drill: bool, pages: usize) -> Gate {
    let bytes = parts.total();
    if !email || bytes <= EMAIL_LIMIT_BYTES {
        return Gate { allowed: true, bytes, message: format!("The file will be about {:.1} MB.", bytes as f64 / 1_000_000.0), suggestions: Vec::new() };
    }
    let mut suggestions = vec!["Use fewer filter values."];
    if has_drill {
        suggestions.push("Use fewer drill levels.");
    }
    suggestions.push("Put Top N on the biggest chart.");
    if pages > 1 {
        suggestions.push("Use fewer pages.");
    }
    if parts.template > 0 {
        suggestions.push("Leave out the design picture.");
    }
    Gate { allowed: false, bytes, message: format!("The file would be about {:.1} MB, over the 5 MB an email takes. Cut it down:", bytes as f64 / 1_000_000.0), suggestions }
}

/// A page without items or filters has nothing to export.
pub fn check_plan(plan: &ReportPlan) -> Result<()> {
    if plan.pages.iter().all(|p| p.items.is_empty()) {
        bail!("The dashboard has no items to put in a file. Add a chart, a pivot or a card first.");
    }
    Ok(())
}

/// The scale a pivot item is exported with: its unit's divisor, 2 decimals and the short name its headings carry.
pub fn unit_scale(u: ItemUnit) -> (f64, usize, String) {
    (u.divisor(), 2, u.short().to_string())
}
