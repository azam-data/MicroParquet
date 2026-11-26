//! Model ▸ Measures… (Phase 9 M3): Columns. Which numeric columns of a date-linked data tab are balances (a position on a date),
//! flows (an amount in a period) or YTD balances (a trial balance's income and expense GLs). The tag sets the aggregation a column
//! gets when it is added to Values, and warns when a balance is summed over dates. A name that looks like a balance or a flow
//! gets a suggestion, applied by a click and never by itself.

use crate::ui::prelude::*;
use micro_parquet::measures::{check_measure, removal_refusal, suggest, ColumnKind, ColumnTag, DayCount, NamedMeasure, PlanAgg, PlanShow, RatioFormat, Sign, Term};
use micro_parquet::models::{CondOp, FilterCondition, TimeAgg};

pub const MEASURES_DIALOG_TITLE: &str = "Measures";
/// The first choice of a column's list.
pub const NOT_TAGGED: &str = "Not tagged";

/// What the dialog drew in the last frame (the render tests read it).
#[derive(Clone, Debug, Default)]
pub struct MeasureRects {
    pub buttons: Vec<(String, egui::Rect)>,
    /// The tag box of each column: ("Accounts \u{b7} Balance", rect).
    pub tags: Vec<(String, egui::Rect)>,
    /// The suggestion button of each column that has one.
    pub suggestions: Vec<(String, egui::Rect)>,
    /// The column names.
    pub names: Vec<(String, egui::Rect)>,
    pub message: Option<egui::Rect>,
    /// The two sections of the dialog, the measures' own buttons ("Add base measure", "Edit NIM", "Remove NIM", "Save"...) and the
    /// boxes of the editor ("Name", "Day count", "Sign"...), by name.
    pub sections: Vec<(String, egui::Rect)>,
    pub fields: Vec<(String, egui::Rect)>,
    pub list_rows: Vec<(String, egui::Rect)>,
    pub error: Option<egui::Rect>,
}

/// One filter of a base measure while it is edited.
#[derive(Clone, Debug)]
pub struct FilterDraft {
    /// The field's name as the condition holds it ("Product", "GL map \u{b7} Head").
    pub field: String,
    pub lookup: Option<micro_parquet::model::ModelFieldRef>,
    pub op: CondOp,
    pub value: String,
}

/// A measure being added or edited.
#[derive(Clone, Debug)]
pub struct MeasureEdit {
    /// The place of the measure in the model when it is edited (its name may change; every reference follows).
    pub index: Option<usize>,
    pub ratio: bool,
    /// A budget or target measure (Phase 11).
    pub plan: bool,
    pub plan_tab: String,
    pub plan_agg: PlanAgg,
    pub plan_show: PlanShow,
    pub actual: String,
    pub name: String,
    pub tab_key: String,
    pub column: String,
    pub agg: TimeAgg,
    pub weight: String,
    pub sign: Sign,
    pub filters: Vec<FilterDraft>,
    pub num: Vec<Term>,
    pub den: Vec<Term>,
    pub annualise: Option<DayCount>,
    pub format: RatioFormat,
    pub error: String,
}

impl MeasureEdit {
    fn new(ratio: bool, tab_key: &str) -> Self {
        MeasureEdit { index: None, ratio, plan: false, plan_tab: String::new(), plan_agg: PlanAgg::Sum, plan_show: PlanShow::PercentAchieved, actual: String::new(), name: String::new(), tab_key: tab_key.to_string(), column: String::new(), agg: TimeAgg::ClosingBalance, weight: String::new(), sign: Sign::Plus, filters: Vec::new(), num: vec![Term::default()], den: vec![Term::default()], annualise: None, format: RatioFormat::Percent, error: String::new() }
    }

    fn of(index: usize, m: &NamedMeasure) -> Self {
        let mut e = MeasureEdit::new(m.is_ratio(), m.tab());
        e.index = Some(index);
        e.name = m.name().to_string();
        match m {
            NamedMeasure::Base { column, aggregation, filters, sign, weight, .. } => {
                e.column = column.clone();
                e.agg = *aggregation;
                e.sign = *sign;
                e.weight = weight.clone().unwrap_or_default();
                e.filters = filters.iter().map(|c| FilterDraft { field: c.column.clone(), lookup: c.lookup.clone(), op: c.op, value: c.value.clone() }).collect();
            }
            NamedMeasure::Ratio { numerator, denominator, annualise, format, .. } => {
                e.num = numerator.clone();
                e.den = denominator.clone();
                e.annualise = *annualise;
                e.format = *format;
            }
            NamedMeasure::Plan { plan_tab, column, agg, show, actual, .. } => {
                e.plan = true;
                e.plan_tab = plan_tab.clone();
                e.column = column.clone();
                e.plan_agg = *agg;
                e.plan_show = *show;
                e.actual = actual.clone();
            }
        }
        e
    }

    fn to_measure(&self) -> NamedMeasure {
        let name = self.name.trim().to_string();
        if self.plan {
            NamedMeasure::Plan { name, tab: self.tab_key.clone(), plan_tab: self.plan_tab.clone(), column: self.column.clone(), agg: self.plan_agg, show: self.plan_show, actual: if self.plan_show == PlanShow::Plan { String::new() } else { self.actual.clone() } }
        } else if self.ratio {
            NamedMeasure::Ratio { name, tab: self.tab_key.clone(), numerator: self.num.clone(), denominator: self.den.clone(), annualise: self.annualise, format: self.format }
        } else {
            NamedMeasure::Base {
                name,
                tab: self.tab_key.clone(),
                column: self.column.clone(),
                aggregation: self.agg,
                filters: self.filters.iter().map(|f| FilterCondition { column: f.field.clone(), op: f.op, value: f.value.trim().to_string(), lookup: f.lookup.clone(), ..Default::default() }).collect(),
                sign: self.sign,
                weight: Some(self.weight.trim().to_string()).filter(|w| !w.is_empty()),
            }
        }
    }
}

/// The two sections of the dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MeasuresSection {
    #[default]
    Columns,
    Measures,
}

#[derive(Default)]
pub struct MeasuresUi {
    pub rects: MeasureRects,
    pub section: MeasuresSection,
    pub edit: Option<MeasureEdit>,
    /// Why a measure was not removed (or what was done).
    pub message: String,
}


/// A date-linked data tab as the Measures editor sees it.
pub struct MeasureTabInfo {
    pub key: String,
    pub title: String,
    pub numbers: Vec<String>,
    pub columns: Vec<String>,
    /// Lookup and Calendar fields: the field and its picker text ("GL map \u{203a} Head").
    pub lookups: Vec<(micro_parquet::model::ModelFieldRef, String)>,
    /// The budget and target tables linked to the tab: (project key, title, numeric columns).
    pub plans: Vec<(String, String, Vec<String>)>,
}

enum MeasureAct {
    AddBase,
    AddRatio,
    AddPlan,
    Edit(usize),
    Remove(usize),
    Save,
    Cancel,
}

/// "Closing balance of Balance where GL map \u{203a} Head = Interest income (sign \u{2212})" for the list.
fn measure_summary(m: &NamedMeasure) -> String {
    match m {
        NamedMeasure::Base { column, aggregation, filters, sign, .. } => {
            let mut t = format!("{} of {}", aggregation.label(), column);
            if !filters.is_empty() {
                let f: Vec<String> = filters.iter().map(|c| format!("{} {} {}", c.column.replacen(" \u{b7} ", " \u{203a} ", 1), if c.op == CondOp::NotEquals { "\u{2260}" } else if c.op == CondOp::Contains { "contains" } else { "=" }, c.value)).collect();
                t.push_str(&format!(" where {}", f.join(" and ")));
            }
            if *sign == Sign::Minus {
                t.push_str(" (sign \u{2212})");
            }
            t
        }
        NamedMeasure::Plan { plan_tab, column, agg, show, actual, .. } => {
            let _ = plan_tab;
            let how = if *agg == PlanAgg::Closing { "closing" } else { "sum" };
            match show {
                PlanShow::Plan => format!("Plan: {} ({} of months)", column, how),
                PlanShow::Variance => format!("{} less plan {} ({} of months)", actual, column, how),
                PlanShow::PercentAchieved => format!("{} as % of plan {} ({} of months)", actual, column, how),
            }
        }
        NamedMeasure::Ratio { numerator, denominator, annualise, .. } => {
            let side = |terms: &[Term]| terms.iter().enumerate().map(|(i, t)| format!("{}{}", if t.sign == Sign::Minus { "\u{2212} " } else if i > 0 { "+ " } else { "" }, t.measure)).collect::<Vec<_>>().join(" ");
            let mut t = format!("({}) / ({})", side(numerator), side(denominator));
            if let Some(d) = annualise {
                t.push_str(&format!(", annualised {}", d.label()));
            }
            t
        }
    }
}

fn combo_field<T: PartialEq + Copy>(ui: &mut egui::Ui, id: (&str, &str), width: f32, shown: &str, current: &mut T, options: &[(T, String)], rects: &mut MeasureRects, name: &str) {
    let r = egui::ComboBox::from_id_salt(id).width(width).selected_text(shown).show_ui(ui, |ui| {
        for (v, label) in options {
            if ui.selectable_label(*current == *v, label).clicked() {
                *current = *v;
            }
        }
    });
    rects.fields.push((name.to_string(), r.response.rect));
}

fn text_combo(ui: &mut egui::Ui, id: (&str, usize), width: f32, current: &mut String, options: &[String], rects: &mut MeasureRects, name: &str) {
    let shown = if current.is_empty() { "(choose)".to_string() } else { current.clone() };
    let r = egui::ComboBox::from_id_salt(id).width(width).selected_text(shown).show_ui(ui, |ui| {
        for o in options {
            if ui.selectable_label(current == o, o).clicked() {
                *current = o.clone();
            }
        }
    });
    rects.fields.push((name.to_string(), r.response.rect));
}

/// The terms of one side of a ratio.
fn terms_editor(ui: &mut egui::Ui, side: &str, terms: &mut Vec<Term>, bases: &[String], rects: &mut MeasureRects, tk: &theme::Tokens) {
    ui.label(egui::RichText::new(side).strong().color(tk.ink));
    let mut remove: Option<usize> = None;
    for (i, t) in terms.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let mut m = t.measure.clone();
            text_combo(ui, (if side == "Numerator" { "m_num" } else { "m_den" }, i), theme::MEASURE_BOX_W, &mut m, bases, rects, &format!("{} {} measure", side, i + 1));
            t.measure = m;
            let signs = [(Sign::Plus, Sign::Plus.label().to_string()), (Sign::Minus, Sign::Minus.label().to_string())];
            combo_field(ui, (if side == "Numerator" { "m_nsign" } else { "m_dsign" }, &i.to_string()), theme::MEASURE_SIGN_W, t.sign.label(), &mut t.sign, &signs, rects, &format!("{} {} sign", side, i + 1));
            let b = ui.button("Remove term");
            rects.buttons.push((format!("Remove {} term {}", side.to_lowercase(), i + 1), b.rect));
            if b.clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        if terms.len() > 1 {
            terms.remove(i);
        }
    }
    let b = ui.button(format!("Add {} term", side.to_lowercase()));
    rects.buttons.push((format!("Add {} term", side.to_lowercase()), b.rect));
    if b.clicked() {
        terms.push(Term::default());
    }
}

fn measure_editor(ui: &mut egui::Ui, e: &mut MeasureEdit, view_tabs: &[MeasureTabInfo], bases: &[String], rects: &mut MeasureRects, act: &mut Option<MeasureAct>, tk: &theme::Tokens) {
    ui.add_space(theme::SPACE_S);
    ui.separator();
    ui.label(egui::RichText::new(if e.plan { "BUDGET / TARGET MEASURE" } else if e.ratio { "RATIO" } else { "BASE MEASURE" }).size(theme::TEXT_LABEL).color(tk.muted).strong());
    egui::Grid::new("measure_editor").num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_XS)).show(ui, |ui| {
        ui.label("Name");
        let name = ui.add(egui::TextEdit::singleline(&mut e.name).desired_width(theme::MEASURE_NAME_BOX_W).hint_text("for example NIM"));
        rects.fields.push(("Name".to_string(), name.rect));
        ui.end_row();
        ui.label("Tab");
        let titles: Vec<String> = view_tabs.iter().map(|t| t.title.clone()).collect();
        let mut shown = view_tabs.iter().find(|t| t.key == e.tab_key).map(|t| t.title.clone()).unwrap_or_default();
        let before = shown.clone();
        ui.add_enabled_ui(e.index.is_none(), |ui| text_combo(ui, ("m_tab", 0), theme::MEASURE_BOX_W, &mut shown, &titles, rects, "Tab"));
        if shown != before {
            if let Some(t) = view_tabs.iter().find(|t| t.title == shown) {
                e.tab_key = t.key.clone();
                e.column.clear();
                e.filters.clear();
            }
        }
        ui.end_row();
        let tab = view_tabs.iter().find(|t| t.key == e.tab_key);
        if e.plan {
            let plans = tab.map(|t| t.plans.clone()).unwrap_or_default();
            ui.label("Plan table");
            let titles: Vec<String> = plans.iter().map(|p| p.1.clone()).collect();
            let mut shown = plans.iter().find(|p| p.0 == e.plan_tab).map(|p| p.1.clone()).unwrap_or_default();
            let before = shown.clone();
            text_combo(ui, ("m_plan", 0), theme::MEASURE_BOX_W, &mut shown, &titles, rects, "Plan table");
            if shown != before {
                if let Some(p) = plans.iter().find(|p| p.1 == shown) {
                    e.plan_tab = p.0.clone();
                    e.column.clear();
                }
            }
            ui.end_row();
            ui.label("Amount column");
            let numbers = plans.iter().find(|p| p.0 == e.plan_tab).map(|p| p.2.clone()).unwrap_or_default();
            text_combo(ui, ("m_pcol", 0), theme::MEASURE_BOX_W, &mut e.column, &numbers, rects, "Amount column");
            ui.end_row();
            ui.label("Months add up as");
            let aggs = [(PlanAgg::Sum, "Sum of the months (a flow budget)".to_string()), (PlanAgg::Closing, "Last month (a closing target)".to_string())];
            let shown = aggs.iter().find(|(a, _)| *a == e.plan_agg).map(|(_, l)| l.clone()).unwrap_or_default();
            combo_field(ui, ("m_pagg", "0"), theme::MEASURE_BOX_W, &shown, &mut e.plan_agg, &aggs, rects, "Months add up as");
            ui.end_row();
            ui.label("Shows");
            let shows = [(PlanShow::Plan, PlanShow::Plan.label().to_string()), (PlanShow::Variance, PlanShow::Variance.label().to_string()), (PlanShow::PercentAchieved, PlanShow::PercentAchieved.label().to_string())];
            combo_field(ui, ("m_pshow", "0"), theme::MEASURE_BOX_W, e.plan_show.label(), &mut e.plan_show, &shows, rects, "Shows");
            ui.end_row();
            if e.plan_show != PlanShow::Plan {
                ui.label("Actual");
                text_combo(ui, ("m_actual", 0), theme::MEASURE_BOX_W, &mut e.actual, bases, rects, "Actual");
                ui.end_row();
            }
        } else if !e.ratio {
            ui.label("Aggregation");
            let aggs: Vec<(TimeAgg, String)> = TimeAgg::all().iter().map(|a| (*a, a.label().to_string())).collect();
            combo_field(ui, ("m_agg", "0"), theme::MEASURE_BOX_W, e.agg.label(), &mut e.agg, &aggs, rects, "Aggregation");
            ui.end_row();
            ui.label("Column");
            let numbers = tab.map(|t| t.numbers.clone()).unwrap_or_default();
            let all = tab.map(|t| t.columns.clone()).unwrap_or_default();
            text_combo(ui, ("m_col", 0), theme::MEASURE_BOX_W, &mut e.column, if e.agg.needs_number() { &numbers } else { &all }, rects, "Column");
            ui.end_row();
            if e.agg == TimeAgg::WAvgAtClosing {
                ui.label("Weight");
                text_combo(ui, ("m_weight", 0), theme::MEASURE_BOX_W, &mut e.weight, &numbers, rects, "Weight");
                ui.end_row();
            }
            ui.label("Sign");
            let signs = [(Sign::Plus, Sign::Plus.label().to_string()), (Sign::Minus, Sign::Minus.label().to_string())];
            combo_field(ui, ("m_sign", "0"), theme::MEASURE_BOX_W, e.sign.label(), &mut e.sign, &signs, rects, "Sign");
            ui.end_row();
        } else {
            ui.label("Day count");
            let counts: Vec<(Option<DayCount>, String)> = std::iter::once((None, "Not annualised (a plain share)".to_string())).chain(DayCount::all().iter().map(|d| (Some(*d), d.label().to_string()))).collect();
            let shown = e.annualise.map(|d| d.label().to_string()).unwrap_or_else(|| "Not annualised (a plain share)".to_string());
            combo_field(ui, ("m_dc", "0"), theme::MEASURE_DAYCOUNT_W, &shown, &mut e.annualise, &counts, rects, "Day count");
            ui.end_row();
            ui.label("Shown as");
            let fmts = [(RatioFormat::Percent, "Percent".to_string()), (RatioFormat::Number, "Number".to_string())];
            combo_field(ui, ("m_fmt", "0"), theme::MEASURE_BOX_W, e.format.label(), &mut e.format, &fmts, rects, "Shown as");
            ui.end_row();
        }
    });
    if e.plan {
        ui.label(egui::RichText::new("The plan is read by month and the keys of its link. A month that has started counts in full; a month the plan has no row for makes the cell blank.").size(theme::TEXT_LABEL).color(tk.muted));
    } else if e.ratio {
        terms_editor(ui, "Numerator", &mut e.num, bases, rects, tk);
        terms_editor(ui, "Denominator", &mut e.den, bases, rects, tk);
    } else {
        ui.label(egui::RichText::new("Only rows that meet these filters are added up (lookup and Calendar fields are allowed):").size(theme::TEXT_LABEL).color(tk.muted));
        let tab = view_tabs.iter().find(|t| t.key == e.tab_key);
        let mut fields: Vec<String> = tab.map(|t| t.columns.clone()).unwrap_or_default();
        fields.extend(tab.map(|t| t.lookups.iter().map(|(_, l)| l.clone()).collect::<Vec<_>>()).unwrap_or_default());
        let mut remove: Option<usize> = None;
        for (i, f) in e.filters.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let mut shown = f.field.replacen(" \u{b7} ", " \u{203a} ", 1);
                let before = shown.clone();
                text_combo(ui, ("m_ff", i), theme::MEASURE_FIELD_W, &mut shown, &fields, rects, &format!("Filter {} field", i + 1));
                if shown != before {
                    match tab.and_then(|t| t.lookups.iter().find(|(_, l)| *l == shown)) {
                        Some((r, _)) => {
                            f.field = r.name.clone();
                            f.lookup = Some(r.clone());
                        }
                        None => {
                            f.field = shown;
                            f.lookup = None;
                        }
                    }
                }
                let ops = [(CondOp::Equals, "equals".to_string()), (CondOp::NotEquals, "is not".to_string()), (CondOp::Contains, "contains".to_string())];
                let opt = ops.iter().find(|(o, _)| *o == f.op).map(|(_, l)| l.clone()).unwrap_or_else(|| "equals".to_string());
                combo_field(ui, ("m_fo", &i.to_string()), theme::MEASURE_TEST_W, &opt, &mut f.op, &ops, rects, &format!("Filter {} test", i + 1));
                let v = ui.add(egui::TextEdit::singleline(&mut f.value).desired_width(theme::MEASURE_VALUE_W));
                rects.fields.push((format!("Filter {} value", i + 1), v.rect));
                let b = ui.button("Remove");
                rects.buttons.push((format!("Remove filter {}", i + 1), b.rect));
                if b.clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            e.filters.remove(i);
        }
        let b = ui.button("Add filter");
        rects.buttons.push(("Add filter".to_string(), b.rect));
        if b.clicked() {
            e.filters.push(FilterDraft { field: String::new(), lookup: None, op: CondOp::Equals, value: String::new() });
        }
    }
    if !e.error.is_empty() {
        let m = ui.add(egui::Label::new(egui::RichText::new(&e.error).color(tk.danger)).wrap());
        rects.error = Some(m.rect);
    }
    ui.horizontal(|ui| {
        let save = ui.button("Save measure");
        rects.buttons.push(("Save".to_string(), save.rect));
        if save.clicked() {
            *act = Some(MeasureAct::Save);
        }
        let cancel = ui.button("Cancel");
        rects.buttons.push(("Cancel edit".to_string(), cancel.rect));
        if cancel.clicked() {
            *act = Some(MeasureAct::Cancel);
        }
    });
}

impl MicroParquetApp {
    /// The tag of a column of a data tab, if it has one.
    pub(crate) fn column_tag(&self, tab_key: &str, column: &str) -> Option<ColumnTag> {
        self.project.model.column_kinds.iter().find(|k| k.tab == tab_key && k.column == column).map(|k| k.kind)
    }

    /// Sets or clears (`None`) the tag of a column.
    pub(crate) fn set_column_tag(&mut self, tab_key: &str, column: &str, tag: Option<ColumnTag>) {
        let kinds = &mut self.project.model.column_kinds;
        kinds.retain(|k| !(k.tab == tab_key && k.column == column));
        if let Some(kind) = tag {
            kinds.push(ColumnKind { tab: tab_key.to_string(), column: column.to_string(), kind });
        }
    }

    /// The date-linked data tabs and their numeric columns: (tab id, project key, title, columns).
    fn measure_tabs(&self) -> Vec<(u64, String, String, Vec<String>)> {
        let Some(cal) = &self.project.model.calendar else {
            return Vec::new();
        };
        self.tabs
            .iter()
            .filter_map(|t| {
                let key = self.project.keys.get(&t.id)?;
                cal.date_links.iter().find(|l| &l.data_tab == key)?;
                let numbers: Vec<String> = t.cached_slice.get_columns().iter().filter(|c| c.dtype().is_numeric()).map(|c| c.name().to_string()).collect();
                Some((t.id, key.clone(), t.title.clone(), numbers))
            })
            .collect()
    }

    /// The date-linked data tabs with their columns and lookup fields, for the editor of a measure.
    pub(crate) fn measure_tab_infos(&self) -> Vec<MeasureTabInfo> {
        let Some(cal) = &self.project.model.calendar else {
            return Vec::new();
        };
        self.tabs
            .iter()
            .filter_map(|t| {
                let key = self.project.keys.get(&t.id)?;
                cal.date_links.iter().find(|l| &l.data_tab == key)?;
                let columns: Vec<String> = t.cached_slice.get_columns().iter().map(|c| c.name().to_string()).collect();
                let numbers: Vec<String> = t.cached_slice.get_columns().iter().filter(|c| c.dtype().is_numeric()).map(|c| c.name().to_string()).collect();
                let lookups = self.model_groups_for(t.id).into_iter().flat_map(|g| g.columns.into_iter().map(|(r, _)| { let l = r.picker(); (r, l) })).collect();
                let plans: Vec<(String, String, Vec<String>)> = self
                    .project
                    .model
                    .plans
                    .iter()
                    .filter(|p| &p.data_tab == key)
                    .filter_map(|p| {
                        let pt = self.tabs.iter().find(|x| self.project.keys.get(&x.id) == Some(&p.plan_tab))?;
                        let nums: Vec<String> = pt.cached_slice.get_columns().iter().filter(|c| c.dtype().is_numeric()).map(|c| c.name().to_string()).collect();
                        Some((p.plan_tab.clone(), pt.title.clone(), nums))
                    })
                    .collect();
                Some(MeasureTabInfo { key: key.clone(), title: t.title.clone(), numbers, columns, lookups, plans })
            })
            .collect()
    }

    /// The pivots and charts (by title) that use the named measure, for the refusal to remove it.
    pub(crate) fn measure_users(&self, name: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for t in &self.tabs {
            let uses = t.steps.iter().filter_map(|s| s.as_pivot_spec()).any(|p| p.values.iter().any(|v| v.field == name && v.time.is_some_and(|x| x.agg == TimeAgg::Measure)));
            if uses {
                out.push(format!("the pivot on '{}'", t.title));
            }
        }
        for (id, info) in &self.chart_tabs {
            if info.data.spec.measure_list().iter().any(|m| m.column.as_deref() == Some(name) && m.time.is_some_and(|x| x.agg == TimeAgg::Measure)) {
                let title = self.tabs.iter().find(|t| t.id == *id).map(|t| t.title.clone()).unwrap_or_else(|| "a chart".to_string());
                out.push(format!("the chart '{}'", title));
            }
        }
        let c = &self.chart.spec;
        if c.measure_list().iter().any(|m| m.column.as_deref() == Some(name) && m.time.is_some_and(|x| x.agg == TimeAgg::Measure)) {
            out.push("the chart being drawn".to_string());
        }
        if self.pivot_builder.spec.values.iter().any(|v| v.field == name && v.time.is_some_and(|x| x.agg == TimeAgg::Measure)) {
            out.push("the pivot being built".to_string());
        }
        out
    }

    pub(crate) fn render_model_measures_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = MeasureRects::default();
        let tabs = self.measure_tabs();
        let infos = self.measure_tab_infos();
        let measures = self.project.model.measures.clone();
        let mut section = self.project.model_ui.measures.section;
        let mut edit = self.project.model_ui.measures.edit.take();
        let message = self.project.model_ui.measures.message.clone();
        let mut set: Option<(String, String, Option<ColumnTag>)> = None;
        let mut act: Option<MeasureAct> = None;
        let mut close = false;
        theme::modal(crate::ui::model_flow::MODEL_SETUP_TITLE).default_width(theme::DIALOG_W_MODEL_FLOW).show(ctx, |d| {
            d.body(|ui| {
                self.model_flow_header(ui, micro_parquet::workspace::ModelStep::Measures);
                ui.horizontal(|ui| {
                    for (s, label) in [(MeasuresSection::Columns, "Columns"), (MeasuresSection::Measures, "Measures")] {
                        let r = ui.selectable_label(section == s, label);
                        rects.sections.push((label.to_string(), r.rect));
                        if r.clicked() {
                            section = s;
                        }
                    }
                });
                ui.add_space(theme::SPACE_XS);
                if section == MeasuresSection::Measures {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new("A base measure is a column of a tab, added up over time, for the rows you choose. A ratio is one signed sum of base measures over another; a rate is annualised with a day count. They are listed under Measures in Values and in the chart's values.")
                                .size(theme::TEXT_LABEL)
                                .color(tk.muted),
                        )
                        .wrap(),
                    );
                    ui.add_space(theme::SPACE_S);
                    if infos.is_empty() {
                        let m = ui.add(egui::Label::new(egui::RichText::new("No data tab has a date link yet. Turn on the calendar and link a tab's date column in Model \u{25b8} Calendar\u{2026}.").color(tk.ink)).wrap());
                        rects.message = Some(m.rect);
                        return;
                    }
                    // the editor first, so the list never pushes it out of view
                    if let Some(e) = edit.as_mut() {
                        let bases: Vec<String> = measures.iter().filter(|m| m.tab() == e.tab_key && if e.plan { !m.is_plan() } else { !m.is_ratio() && !m.is_plan() }).map(|m| m.name().to_string()).collect();
                        measure_editor(ui, e, &infos, &bases, &mut rects, &mut act, &tk);
                        ui.add_space(theme::SPACE_S);
                        ui.separator();
                    }
                    for (i, m) in measures.iter().enumerate() {
                        let title = infos.iter().find(|t| t.key == m.tab()).map(|t| t.title.clone()).unwrap_or_else(|| m.tab().to_string());
                        let row = ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(egui::vec2(theme::MEASURE_NAME_W, ui.spacing().interact_size.y), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.set_min_width(theme::MEASURE_NAME_W);
                                ui.set_max_width(theme::MEASURE_NAME_W);
                                ui.add(egui::Label::new(egui::RichText::new(m.name()).strong().color(tk.ink)).truncate()).on_hover_text(format!("{} \u{b7} {}", if m.is_plan() { "Budget / target" } else if m.is_ratio() { "Ratio" } else { "Base measure" }, title));
                            });
                            let e = ui.button("Edit");
                            rects.buttons.push((format!("Edit {}", m.name()), e.rect));
                            if e.clicked() {
                                act = Some(MeasureAct::Edit(i));
                            }
                            let r = ui.button("Remove");
                            rects.buttons.push((format!("Remove {}", m.name()), r.rect));
                            if r.clicked() {
                                act = Some(MeasureAct::Remove(i));
                            }
                        });
                        rects.list_rows.push((m.name().to_string(), row.response.rect));
                        ui.add(egui::Label::new(egui::RichText::new(format!("{} \u{b7} {}", title, measure_summary(m))).size(theme::TEXT_LABEL).color(tk.muted)).wrap());
                    }
                    if measures.is_empty() {
                        ui.label(egui::RichText::new("No measure yet.").color(tk.muted));
                    }
                    ui.add_space(theme::SPACE_S);
                    ui.horizontal(|ui| {
                        let b = ui.button("Add base measure");
                        rects.buttons.push(("Add base measure".to_string(), b.rect));
                        if b.clicked() {
                            act = Some(MeasureAct::AddBase);
                        }
                        let r = ui.button("Add ratio");
                        rects.buttons.push(("Add ratio".to_string(), r.rect));
                        if r.clicked() {
                            act = Some(MeasureAct::AddRatio);
                        }
                        let pb = ui.button("Add budget / target");
                        rects.buttons.push(("Add budget / target".to_string(), pb.rect));
                        if pb.clicked() {
                            act = Some(MeasureAct::AddPlan);
                        }
                    });
                    if !message.is_empty() {
                        let m = ui.add(egui::Label::new(egui::RichText::new(&message).color(tk.danger)).wrap());
                        rects.message = Some(m.rect);
                    }
                    return;
                }
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("A balance is a position on a date: adding the balances of several dates means nothing. A flow is an amount in a period: adding is right. Tag the numeric columns of each date-linked tab; the tag sets the aggregation a column gets in Values.")
                            .size(theme::TEXT_LABEL)
                            .color(tk.muted),
                    )
                    .wrap(),
                );
                ui.add_space(theme::SPACE_S);
                ui.label(egui::RichText::new("COLUMNS").size(theme::TEXT_LABEL).color(tk.muted).strong());
                if tabs.is_empty() {
                    let m = ui.add(egui::Label::new(egui::RichText::new("No data tab has a date link yet. Turn on the calendar and link a tab's date column in Model \u{25b8} Calendar\u{2026}.").color(tk.ink)).wrap());
                    rects.message = Some(m.rect);
                    return;
                }
                for (_, key, title, columns) in &tabs {
                    ui.add_space(theme::SPACE_XS);
                    ui.label(egui::RichText::new(title).strong().color(tk.ink));
                    if columns.is_empty() {
                        ui.label(egui::RichText::new("No number column.").color(tk.muted));
                    }
                    for c in columns {
                        let current = self.column_tag(key, c);
                        let id = format!("{} \u{b7} {}", title, c);
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(egui::vec2(theme::MEASURE_NAME_W, ui.spacing().interact_size.y), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.set_min_width(theme::MEASURE_NAME_W);
                                ui.set_max_width(theme::MEASURE_NAME_W);
                                let r = ui.add(egui::Label::new(egui::RichText::new(c).color(tk.ink)).truncate()).on_hover_text(c);
                                rects.names.push((id.clone(), r.rect));
                            });
                            let shown = current.map(|t| t.label()).unwrap_or(NOT_TAGGED);
                            let combo = egui::ComboBox::from_id_salt(("measure_tag", key.as_str(), c.as_str())).width(theme::MEASURE_TAG_W).selected_text(shown).show_ui(ui, |ui| {
                                if ui.selectable_label(current.is_none(), NOT_TAGGED).clicked() {
                                    set = Some((key.clone(), c.clone(), None));
                                }
                                for t in ColumnTag::all() {
                                    if ui.selectable_label(current == Some(*t), t.label()).clicked() {
                                        set = Some((key.clone(), c.clone(), Some(*t)));
                                    }
                                }
                            });
                            rects.tags.push((id.clone(), combo.response.rect));
                            // a name that looks like a balance or a flow: a suggestion, applied by a click
                            if let Some(s) = suggest(c).filter(|s| current != Some(*s)) {
                                let b = ui.add(egui::Button::new(egui::RichText::new(format!("Suggested: {}", s.label())).size(theme::TEXT_LABEL).color(tk.accent_dark)).fill(tk.accent_soft)).on_hover_text("Tag this column so. Nothing is tagged until you click.");
                                rects.suggestions.push((id.clone(), b.rect));
                                if b.clicked() {
                                    set = Some((key.clone(), c.clone(), Some(s)));
                                }
                            }
                        });
                    }
                }
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let c = ui.button("Close");
                    rects.buttons.push(("Close".to_string(), c.rect));
                    close = c.clicked();
                    self.model_flow_footer(ui, micro_parquet::workspace::ModelStep::Measures);
                });
            });
        });
        self.project.model_ui.measures.rects = rects;
        self.project.model_ui.measures.section = section;
        self.project.model_ui.measures.edit = edit;
        if let Some((key, column, tag)) = set {
            self.set_column_tag(&key, &column, tag);
        }
        if let Some(a) = act {
            self.measure_act(a, &infos);
        }
        if close {
            self.active_tool = ActiveToolModal::None;
        }
        ModalOutcome { close: false, step: None }
    }
}

impl MicroParquetApp {
    /// A measure was renamed (M5 follow-up): the terms of ratios, the Values of pivots (also the saved pivot steps of every tab),
    /// the values of chart tabs and of the open chart studio follow, and the pivots and charts that used it are calculated again with the model that has the new name.
    fn rename_measure_everywhere(&mut self, old: &str, new: &str) {
        micro_parquet::measures::rename_in_ratios(&mut self.project.model.measures, old, new);
        // (the pivots are worked out again by `model_scope_refresh`, once the model the workers read has the new name)
        for t in self.tabs.iter_mut() {
            for s in t.steps.iter_mut() {
                s.rename_measure(old, new);
            }
        }
        for info in self.chart_tabs.values_mut() {
            if info.data.spec.rename_measure(old, new) {
                info.source_sig.clear();
            }
        }
        self.chart.spec.rename_measure(old, new);
        for v in self.pivot_builder.spec.values.iter_mut().filter(|v| v.field == old && v.time.is_some_and(|t| t.agg == micro_parquet::models::TimeAgg::Measure)) {
            v.field = new.to_string();
        }
    }

    fn measure_act(&mut self, act: MeasureAct, infos: &[MeasureTabInfo]) {
        let first_tab = infos.first().map(|t| t.key.clone()).unwrap_or_default();
        let ui = &mut self.project.model_ui.measures;
        ui.message.clear();
        match act {
            MeasureAct::AddBase => ui.edit = Some(MeasureEdit::new(false, &first_tab)),
            MeasureAct::AddRatio => ui.edit = Some(MeasureEdit::new(true, &first_tab)),
            MeasureAct::AddPlan => {
                let mut ed = MeasureEdit::new(false, &first_tab);
                ed.plan = true;
                // the first plan table linked to the first tab, when there is one
                if let Some(p) = infos.first().and_then(|t| t.plans.first()) {
                    ed.plan_tab = p.0.clone();
                }
                ui.edit = Some(ed);
            }
            MeasureAct::Cancel => ui.edit = None,
            MeasureAct::Edit(i) => {
                if let Some(m) = self.project.model.measures.get(i) {
                    self.project.model_ui.measures.edit = Some(MeasureEdit::of(i, m));
                }
            }
            MeasureAct::Remove(i) => {
                let Some(name) = self.project.model.measures.get(i).map(|m| m.name().to_string()) else { return };
                let users = self.measure_users(&name);
                match removal_refusal(&self.project.model.measures, &name, &users) {
                    Some(why) => self.project.model_ui.measures.message = why,
                    None => {
                        self.project.model.measures.remove(i);
                        self.project.model_ui.measures.edit = None;
                    }
                }
            }
            MeasureAct::Save => {
                let Some(e) = self.project.model_ui.measures.edit.clone() else { return };
                let m = e.to_measure();
                let others: Vec<NamedMeasure> = self.project.model.measures.iter().enumerate().filter(|(i, _)| Some(*i) != e.index).map(|(_, m)| m.clone()).collect();
                // a name is not a column of the tab either: a pivot would not know which one is meant
                let clash = infos.iter().find(|t| t.key == e.tab_key).filter(|t| t.columns.iter().any(|c| c.eq_ignore_ascii_case(m.name()))).map(|t| format!("'{}' is already a column of '{}'. Choose another name.", m.name(), t.title));
                let why = clash.or_else(|| check_measure(&others, &m)).or_else(|| {
                    if let NamedMeasure::Base { filters, .. } = &m {
                        filters.iter().position(|f| f.column.trim().is_empty()).map(|i| format!("Filter {}: pick the field it tests.", i + 1))
                    } else {
                        None
                    }
                });
                match why {
                    Some(w) => {
                        if let Some(ed) = self.project.model_ui.measures.edit.as_mut() {
                            ed.error = w;
                        }
                    }
                    None => {
                        let mut renamed: Option<(String, String)> = None;
                        match e.index {
                            Some(i) if i < self.project.model.measures.len() => {
                                let old = self.project.model.measures[i].name().to_string();
                                if old != m.name() {
                                    renamed = Some((old, m.name().to_string()));
                                }
                                self.project.model.measures[i] = m;
                            }
                            _ => self.project.model.measures.push(m),
                        }
                        if let Some((old, new)) = renamed {
                            self.rename_measure_everywhere(&old, &new);
                        }
                        self.project.model_ui.measures.edit = None;
                    }
                }
            }
        }
    }
}
