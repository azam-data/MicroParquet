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
        ItemData::Problem(_) => 0,
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
            items.push(ItemAnswers { item_id: item.id.clone(), filters: applicable, answers, numbers, linked: additive(&item.source) });
        }
        out.push(PageAnswers { page_id: page.id.clone(), items });
    }
    Ok(out)
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

/// The numbers of one answer as JSON text, with the commas that separate them: what `BYTES_PER_NUMBER` predicts.
pub fn values_json(a: &Answer, scale: &(f64, usize, String)) -> String {
    match &a.data {
        ItemData::Chart(c) => {
            let rows: Vec<String> = c.values.iter().map(|r| format!("[{}]", r.iter().map(|v| number_json(*v, scale.0, scale.1)).collect::<Vec<_>>().join(","))).collect();
            format!("[{}]", rows.join(","))
        }
        ItemData::Table(t) => {
            let cols: Vec<_> = match &t.nested {
                Some(n) => n.values.get_columns().iter().collect(),
                None => t.df.get_columns().iter().skip(t.label_columns).collect(),
            };
            let rows: Vec<String> = cols.iter().map(|s| format!("[{}]", (0..s.len()).map(|i| number_json(s.get(i).ok().and_then(|v| v.extract::<f64>()), 1.0, 2)).collect::<Vec<_>>().join(","))).collect();
            format!("[{}]", rows.join(","))
        }
        ItemData::Kpi(k) => format!("[[{}]]", std::iter::once(k.current).chain(k.spark.iter().copied()).map(|v| number_json(v, 1.0, 4)).collect::<Vec<_>>().join(",")),
        ItemData::Problem(_) => "[]".to_string(),
    }
}

fn answer_json(a: &Answer, scale: &(f64, usize, String)) -> String {
    let picks: Vec<String> = a.picks.iter().map(|p| p.as_ref().map(|s| serde_json::to_string(s).unwrap_or_default()).unwrap_or_else(|| "null".to_string())).collect();
    match &a.data {
        ItemData::Chart(c) => format!("{{\"p\":[{}],\"d\":{},\"c\":{},\"s\":{},\"v\":{}}}", picks.join(","), strings_json(&a.path), strings_json(&c.categories), strings_json(&c.series), values_json(a, scale)),
        ItemData::Table(t) => {
            let (labels, head): (Vec<String>, Vec<String>) = match &t.nested {
                Some(n) => (n.rows.iter().map(|r| format!("{}\u{1f}{}", r.level, r.label)).collect(), n.values.get_columns().iter().map(|c| c.name().to_string()).collect()),
                None => {
                    let first = t.df.get_columns().first().map(|c| (0..c.len()).map(|i| c.get(i).map(|v| v.to_string().trim_matches('"').to_string()).unwrap_or_default()).collect()).unwrap_or_default();
                    (first, t.df.get_columns().iter().skip(t.label_columns).map(|c| c.name().to_string()).collect())
                }
            };
            format!("{{\"p\":[{}],\"d\":{},\"r\":{},\"h\":{},\"v\":{}}}", picks.join(","), strings_json(&a.path), strings_json(&labels), strings_json(&head), values_json(a, scale))
        }
        ItemData::Kpi(_) => format!("{{\"p\":[{}],\"d\":[],\"v\":{}}}", picks.join(","), values_json(a, scale)),
        ItemData::Problem(m) => format!("{{\"p\":[{}],\"d\":[],\"x\":{}}}", picks.join(","), serde_json::to_string(m).unwrap_or_default()),
    }
}

/// The whole table of answers as one JSON text.
pub fn answers_json(plan: &ReportPlan, answers: &[PageAnswers]) -> String {
    let mut pages = Vec::new();
    for (page, pa) in plan.pages.iter().zip(answers) {
        let mut items = Vec::new();
        for (item, ia) in page.items.iter().zip(&pa.items) {
            let kind = match &item.source {
                ItemSource::Chart { .. } => "chart",
                ItemSource::Pivot { .. } => "pivot",
                ItemSource::Kpi { .. } => "kpi",
            };
            let list: Vec<String> = ia.answers.iter().map(|a| answer_json(a, &item.scale)).collect();
            items.push(format!(
                "{{\"id\":{},\"title\":{},\"kind\":\"{}\",\"filters\":{:?},\"linked\":{},\"unit\":{},\"answers\":[{}]}}",
                serde_json::to_string(&item.id).unwrap_or_default(),
                serde_json::to_string(&item.title).unwrap_or_default(),
                kind,
                ia.filters,
                ia.linked.is_ok(),
                serde_json::to_string(&item.scale.2).unwrap_or_default(),
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
