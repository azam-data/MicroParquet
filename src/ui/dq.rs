//! The data-quality scorecard (Phase 6 P4): rules over the open tab, a pass percentage for each,
//! the overall score with its Green, Amber or Red, the first failing rows of each failing rule, and
//! the scorecard (or a rule's failing rows) as a tab.

use crate::ui::prelude::*;
use micro_parquet::dq::*;
use micro_parquet::models::DqView;
use micro_parquet::tab_jobs::{DqJobResult, DqRequest, DqShown};

#[derive(Clone, Debug, Default)]
pub struct DqRects {
    pub fields: Vec<(String, egui::Rect)>,
    pub buttons: Vec<(String, egui::Rect)>,
    pub rows: Vec<(String, egui::Rect)>,
    pub banner: Option<egui::Rect>,
    pub result: Option<egui::Rect>,
}

/// The rule being typed before "Add rule".
#[derive(Clone, Debug)]
pub struct DqDraft {
    pub kind: usize,
    pub name: String,
    pub col: String,
    pub col2: String,
    pub op: DqCompare,
    pub min: String,
    pub max: String,
    pub values: String,
    pub use_map: bool,
    pub gl: String,
    pub tol: String,
}

impl Default for DqDraft {
    fn default() -> Self {
        Self { kind: 0, name: String::new(), col: String::new(), col2: String::new(), op: DqCompare::Ge, min: String::new(), max: String::new(), values: String::new(), use_map: false, gl: String::new(), tol: "0.1".to_string() }
    }
}

/// The kinds in the order of the "Kind" list.
pub const DQ_KINDS: [&str; 7] = ["Completeness", "Uniqueness", "Type validity", "Range", "Consistency", "Allowed values", "Control total"];

pub struct DqState {
    pub set: DqRuleSet,
    pub opened_from: String,
    pub saved: Vec<String>,
    pub draft: DqDraft,
    pub green_text: String,
    pub amber_text: String,
    /// The last checks: the tab they ran on and what they found.
    pub shown: Option<(u64, DqShown)>,
    pub token: u64,
    pub waiting: bool,
    pub message: Option<(bool, String)>,
    pub rects: DqRects,
}

impl Default for DqState {
    fn default() -> Self {
        let set = DqRuleSet::default();
        Self { green_text: format!("{}", set.green_min), amber_text: format!("{}", set.amber_min), set, opened_from: String::new(), saved: Vec::new(), draft: DqDraft::default(), shown: None, token: 0, waiting: false, message: None, rects: DqRects::default() }
    }
}

fn number(t: &str) -> Option<f64> {
    t.trim().replace(',', "").parse::<f64>().ok().filter(|v| v.is_finite())
}

impl MicroParquetApp {
    pub(crate) fn open_dq(&mut self) {
        self.dq.saved = list_rulesets(&dq_dir());
        if self.dq.set.rules.is_empty() && self.dq.opened_from.is_empty() {
            // start from the sample set, so the first run has something to show
            if let Ok(s) = load_ruleset(&dq_dir(), "sample") {
                self.dq_take_set(s);
            }
        }
        self.dq.message = None;
        self.active_tool = ActiveToolModal::DqScorecard;
    }

    fn dq_take_set(&mut self, s: DqRuleSet) {
        self.dq.green_text = format!("{}", s.green_min);
        self.dq.amber_text = format!("{}", s.amber_min);
        self.dq.opened_from = s.name.clone();
        self.dq.set = s;
        self.dq.shown = None;
    }

    fn dq_msg(&mut self, error: bool, text: impl Into<String>) {
        self.dq.message = Some((error, text.into()));
    }

    /// The set as the dialog holds it (the thresholds as typed), checked.
    fn dq_current(&self) -> Result<DqRuleSet, String> {
        let mut s = self.dq.set.clone();
        s.green_min = number(&self.dq.green_text).ok_or_else(|| "Type the Green threshold as a number (for example 98).".to_string())?;
        s.amber_min = number(&self.dq.amber_text).ok_or_else(|| "Type the Amber threshold as a number (for example 95).".to_string())?;
        s.check().map_err(|e| format!("{:#}", e))?;
        Ok(s)
    }

    fn dq_draft_rule(&self) -> Result<DqRule, String> {
        let d = &self.dq.draft;
        let need = |c: &String, what: &str| if c.trim().is_empty() { Err(format!("Choose the {} column.", what)) } else { Ok(c.clone()) };
        let opt = |t: &str, what: &str| -> Result<Option<f64>, String> { if t.trim().is_empty() { Ok(None) } else { number(t).map(Some).ok_or_else(|| format!("The {} must be a number.", what)) } };
        let kind = match d.kind {
            0 => DqKind::Completeness { column: need(&d.col, "")? },
            1 => {
                let mut cols = vec![need(&d.col, "key")?];
                if !d.col2.trim().is_empty() {
                    cols.push(d.col2.clone());
                }
                DqKind::Uniqueness { columns: cols }
            }
            2 => DqKind::TypeValidity { column: need(&d.col, "")? },
            3 => DqKind::Range { column: need(&d.col, "")?, min: opt(&d.min, "minimum")?, max: opt(&d.max, "maximum")? },
            4 => DqKind::Consistency { left: need(&d.col, "first")?, op: d.op, right: need(&d.col2, "second")? },
            5 => DqKind::AllowedValues { column: need(&d.col, "")?, values: d.values.split(['\n', ';', ',']).map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect(), code_map: if d.use_map { Some("cl_code_map".to_string()) } else { None }, case_insensitive: true },
            _ => DqKind::ControlTotal { column: need(&d.col, "")?, gl_total: number(&d.gl).ok_or_else(|| "Type the GL figure as a number.".to_string())?, tolerance_pct: number(&d.tol).filter(|v| *v >= 0.0).ok_or_else(|| "Type the tolerance in percent (0 or more).".to_string())? },
        };
        let rule = DqRule { name: d.name.trim().to_string(), kind };
        let mut one = DqRuleSet { rules: vec![rule.clone()], ..Default::default() };
        one.name = "x".to_string();
        one.check().map_err(|e| format!("{:#}", e))?;
        Ok(rule)
    }

    fn dq_run(&mut self) {
        let set = match self.dq_current() {
            Ok(s) => s,
            Err(e) => return self.dq_msg(true, e),
        };
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return self.dq_msg(true, "Open the tab to check first.");
        };
        self.dq.token += 1;
        self.dq.waiting = true;
        let req = DqRequest { tab_id: tab.id, token: self.dq.token, source: tab.source.clone(), steps: Vec::new(), rules: set, temp_dir: self.paths.temp_dir.clone() };
        self.jobs.submit(JobKey::tab(JobKind::Dq, tab.id), "Checking the data quality", false, move |ctx| micro_parquet::tab_jobs::compute_dq_job(req, ctx).map(|r| JobOutput::Dq(Box::new(r))));
        self.dq_msg(false, "Checking\u{2026}");
    }

    pub(crate) fn apply_dq_result(&mut self, r: DqJobResult) {
        if r.token != self.dq.token {
            return;
        }
        self.dq.waiting = false;
        match r.shown {
            Ok(s) => {
                let tail = match (&s.result.overall, s.result.rating.as_str()) {
                    (Some(o), g) => format!("Overall score {}% ({}).", format_with_commas(*o, 2), g),
                    _ => "No rule had rows to check.".to_string(),
                };
                self.dq.shown = Some((r.tab_id, s));
                self.dq_msg(false, tail);
            }
            Err(e) => {
                self.dq.shown = None;
                self.dq_msg(true, e);
            }
        }
    }

    pub(crate) fn dq_failed(&mut self, msg: String) {
        self.dq.waiting = false;
        self.dq_msg(true, msg);
    }

    /// A tab for the scorecard or the failing rows of one rule, from the rules that were run.
    fn dq_open_tab(&mut self, view: DqView) {
        let Some((tab_id, _)) = &self.dq.shown else {
            return;
        };
        let Some(tab) = self.tabs.iter().find(|t| t.id == *tab_id) else {
            return self.dq_msg(true, "The tab that was checked is not open any more. Run the checks again.");
        };
        let set = match self.dq_current() {
            Ok(s) => s,
            Err(e) => return self.dq_msg(true, e),
        };
        let source = TabSource::DqScorecard { source: Box::new(tab.source.clone()), steps: Vec::new(), rules: Box::new(set.clone()), view };
        let (title, formats) = match view {
            DqView::Scorecard => ("Data-quality scorecard".to_string(), [("Pass %".to_string(), ColumnFormatSpec { fmt_type: MetricFormatType::PercentAsIs, decimals: 2 })].into_iter().collect()),
            DqView::Failing(i) => (format!("Failing: {}", set.rules.get(i).map(|r| r.title()).unwrap_or_default()), HashMap::new()),
        };
        self.create_tab_from_source_with(title, source, Vec::new(), formats, None);
    }

    fn dq_save(&mut self) {
        match self.dq_current() {
            Ok(mut s) => {
                s.name = self.dq.set.name.clone();
                match save_ruleset(&dq_dir(), &s) {
                    Ok(p) => {
                        self.dq.opened_from = s.name.clone();
                        self.dq.set = s;
                        self.dq.saved = list_rulesets(&dq_dir());
                        self.dq_msg(false, format!("Saved the rule set as {}.", p.display()));
                    }
                    Err(e) => self.dq_msg(true, format!("{:#}", e)),
                }
            }
            Err(e) => self.dq_msg(true, e),
        }
    }

    pub(crate) fn render_dq_modal(&mut self, ctx: &egui::Context, cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = DqRects::default();
        let mut close = false;
        #[derive(Clone)]
        enum Act {
            Add,
            Remove(usize),
            Save,
            Open(String),
            Run,
            OpenScorecard,
            OpenFailing(usize),
        }
        let mut act: Option<Act> = None;
        let title = format!("{}  Data-quality scorecard", theme::ph::SCALES);
        theme::modal(&title).default_width(theme::CURVES_DIALOG_W).fill_height().show(ctx, |d| {
            d.header(|ui| {
                if self.dq.set.sample {
                    let r = ui.scope(|ui| theme::warning_label(ui, "Sample parameters: this rule set is marked sample. Confirm its rules and thresholds before the score goes into a paper."));
                    rects.banner = Some(r.response.rect);
                }
                ui.add(egui::Label::new(egui::RichText::new("Every rule, the type rule too, is measured on the tab's data as loaded from its file, before the tab's steps.").color(tk.muted).size(theme::TEXT_LABEL)).wrap());
                ui.horizontal_wrapped(|ui| {
                    ui.label("Rule set:");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.dq.set.name).desired_width(theme::FIELD_W_SORT_LIST * theme::HALF * 2.0));
                    rects.fields.push(("Rule set name".to_string(), r.rect));
                    let b = ui.button("Save rule set");
                    rects.buttons.push(("Save rule set".to_string(), b.rect));
                    if b.clicked() {
                        act = Some(Act::Save);
                    }
                    let saved = self.dq.saved.clone();
                    let r = egui::ComboBox::from_id_salt("dq_open").selected_text("Open a rule set").show_ui(ui, |ui| {
                        for n in &saved {
                            if ui.selectable_label(*n == self.dq.opened_from, n).clicked() {
                                act = Some(Act::Open(n.clone()));
                            }
                        }
                    });
                    rects.buttons.push(("Open a rule set".to_string(), r.response.rect));
                    ui.label("Green from");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.dq.green_text).desired_width(theme::CF_DEFAULT_W * theme::HALF));
                    rects.fields.push(("Green".to_string(), r.rect));
                    ui.label("Amber from");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.dq.amber_text).desired_width(theme::CF_DEFAULT_W * theme::HALF));
                    rects.fields.push(("Amber".to_string(), r.rect));
                });
            });
            d.body_scroll(|ui| {
                theme::card(ui, "Rules", |ui| {
                    if self.dq.set.rules.is_empty() {
                        ui.label(egui::RichText::new("No rule yet. Add one below.").color(tk.muted));
                    }
                    for i in 0..self.dq.set.rules.len() {
                        let row = ui.horizontal(|ui| {
                            let rule = &mut self.dq.set.rules[i];
                            let r = ui.add(egui::TextEdit::singleline(&mut rule.name).desired_width(theme::FIELD_W_SORT_LIST).hint_text(rule.kind.describe()));
                            rects.fields.push((format!("Rule name {}", i + 1), r.rect));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let b = ui.button("Remove");
                                rects.buttons.push((format!("Remove rule {}", i + 1), b.rect));
                                if b.clicked() {
                                    act = Some(Act::Remove(i));
                                }
                                ui.add(egui::Label::new(egui::RichText::new(format!("{}: {}", rule.kind.label(), rule.kind.describe())).color(tk.muted)).truncate());
                            });
                        });
                        rects.rows.push((format!("Rule {}", i + 1), row.response.rect));
                    }
                });
                theme::card(ui, "Add a rule", |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Kind:");
                        let r = egui::ComboBox::from_id_salt("dq_kind").selected_text(DQ_KINDS[self.dq.draft.kind.min(DQ_KINDS.len() - 1)]).show_ui(ui, |ui| {
                            for (i, k) in DQ_KINDS.iter().enumerate() {
                                ui.selectable_value(&mut self.dq.draft.kind, i, *k);
                            }
                        });
                        rects.fields.push(("Kind".to_string(), r.response.rect));
                        let kind = self.dq.draft.kind;
                        let mut col_combo = |ui: &mut egui::Ui, id: &str, label: &str, cur: &mut String, optional: bool, rects: &mut DqRects| {
                            ui.label(label);
                            let shown = if cur.is_empty() { if optional { "(none)".to_string() } else { "Choose\u{2026}".to_string() } } else { cur.clone() };
                            let r = egui::ComboBox::from_id_salt(id).selected_text(shown).show_ui(ui, |ui| {
                                if optional {
                                    ui.selectable_value(cur, String::new(), "(none)");
                                }
                                for c in &cols {
                                    ui.selectable_value(cur, c.clone(), c);
                                }
                            });
                            rects.fields.push((label.trim_end_matches(':').to_string(), r.response.rect));
                        };
                        let dr = &mut self.dq.draft;
                        match kind {
                            0 | 2 | 6 => col_combo(ui, "dq_c1", "Column:", &mut dr.col, false, &mut rects),
                            1 => {
                                col_combo(ui, "dq_c1", "Key:", &mut dr.col, false, &mut rects);
                                col_combo(ui, "dq_c2", "Second key:", &mut dr.col2, true, &mut rects);
                            }
                            3 => {
                                col_combo(ui, "dq_c1", "Column:", &mut dr.col, false, &mut rects);
                                ui.label("Minimum");
                                let r = ui.add(egui::TextEdit::singleline(&mut dr.min).desired_width(theme::CF_DEFAULT_W * theme::HALF));
                                rects.fields.push(("Minimum".to_string(), r.rect));
                                ui.label("Maximum");
                                let r = ui.add(egui::TextEdit::singleline(&mut dr.max).desired_width(theme::CF_DEFAULT_W * theme::HALF));
                                rects.fields.push(("Maximum".to_string(), r.rect));
                            }
                            4 => {
                                col_combo(ui, "dq_c1", "Column:", &mut dr.col, false, &mut rects);
                                let r = egui::ComboBox::from_id_salt("dq_op").selected_text(dr.op.label()).show_ui(ui, |ui| {
                                    for o in [DqCompare::Ge, DqCompare::Le, DqCompare::Eq] {
                                        ui.selectable_value(&mut dr.op, o, o.label());
                                    }
                                });
                                rects.fields.push(("Comparison".to_string(), r.response.rect));
                                col_combo(ui, "dq_c2", "Than column:", &mut dr.col2, false, &mut rects);
                            }
                            5 => {
                                col_combo(ui, "dq_c1", "Column:", &mut dr.col, false, &mut rects);
                                let r = ui.checkbox(&mut dr.use_map, "Codes of the CL code map");
                                rects.fields.push(("CL code map".to_string(), r.rect));
                                let r = ui.add(egui::TextEdit::singleline(&mut dr.values).desired_width(theme::FIELD_W_SORT_LIST).hint_text("or the allowed values, separated by commas"));
                                rects.fields.push(("Allowed values".to_string(), r.rect));
                            }
                            _ => {
                                col_combo(ui, "dq_c1", "Column:", &mut dr.col, false, &mut rects);
                                ui.label("GL figure");
                                let r = ui.add(egui::TextEdit::singleline(&mut dr.gl).desired_width(theme::CF_DEFAULT_W));
                                rects.fields.push(("GL figure".to_string(), r.rect));
                                ui.label("Tolerance %");
                                let r = ui.add(egui::TextEdit::singleline(&mut dr.tol).desired_width(theme::CF_DEFAULT_W * theme::HALF));
                                rects.fields.push(("Tolerance".to_string(), r.rect));
                            }
                        }
                        let r = ui.add(egui::TextEdit::singleline(&mut self.dq.draft.name).desired_width(theme::FIELD_W_SORT_LIST * theme::HALF * 2.0).hint_text("Name (optional)"));
                        rects.fields.push(("Rule name".to_string(), r.rect));
                        let b = ui.button("Add rule");
                        rects.buttons.push(("Add rule".to_string(), b.rect));
                        if b.clicked() {
                            act = Some(Act::Add);
                        }
                    });
                });
                // ---- what the last run found
                if let Some((_, shown)) = &self.dq.shown {
                    let r = theme::card(ui, "Result", |ui| {
                        let res = &shown.result;
                        egui::Grid::new("dq_result").striped(true).num_columns(5).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_S)).show(ui, |ui| {
                            for h in ["Rule", "Rows checked", "Rows failing", "Pass %", "Result"] {
                                ui.label(egui::RichText::new(h).color(tk.muted).size(theme::TEXT_LABEL));
                            }
                            ui.end_row();
                            for r in &res.rules {
                                ui.add(egui::Label::new(&r.name).truncate());
                                ui.label(theme::mono(r.checked.map(|c| format_with_commas(c as f64, 0)).unwrap_or_else(|| "\u{2014}".to_string())));
                                ui.label(theme::mono(if r.control.is_some() { "\u{2014}".to_string() } else { format_with_commas(r.failing as f64, 0) }));
                                ui.label(theme::mono(r.pass_pct.map(|p| format!("{}%", format_with_commas(p, 2))).unwrap_or_else(|| "\u{2014}".to_string())));
                                let (txt, colour) = match &r.control {
                                    Some(c) if c.pass => ("Pass".to_string(), tk.accent_dark),
                                    Some(_) => (format!("Fail ({})", format_with_commas(r.control.as_ref().map(|c| c.difference).unwrap_or(0.0), 2)), tk.danger),
                                    None => (String::new(), tk.ink),
                                };
                                ui.label(egui::RichText::new(txt).color(colour));
                                ui.end_row();
                            }
                            ui.label(theme::semibold(OVERALL_LABEL));
                            ui.label("");
                            ui.label("");
                            ui.label(theme::mono_semibold(res.overall.map(|p| format!("{}%", format_with_commas(p, 2))).unwrap_or_else(|| "\u{2014}".to_string())));
                            let colour = match res.rating.as_str() {
                                "Green" => tk.accent_dark,
                                "Red" => tk.danger,
                                _ => tk.ink,
                            };
                            ui.label(egui::RichText::new(&res.rating).color(colour).strong());
                            ui.end_row();
                        });
                        ui.add_space(theme::SPACE_S);
                        let b = ui.button("Open scorecard as tab");
                        rects.buttons.push(("Open scorecard as tab".to_string(), b.rect));
                        if b.clicked() {
                            act = Some(Act::OpenScorecard);
                        }
                        for (i, preview) in &shown.previews {
                            let rr = &res.rules[*i];
                            ui.add_space(theme::SPACE_S);
                            ui.label(theme::semibold(format!("{}: first {} of {} failing row(s)", rr.name, preview.height(), rr.failing_rows.len())));
                            let lines: Vec<String> = (0..preview.height())
                                .map(|r| preview.get_columns().iter().map(|c| format!("{}={}", c.name(), c.get(r).map(|v| v.to_string().trim_matches('"').to_string()).unwrap_or_default())).collect::<Vec<_>>().join("  \u{b7}  "))
                                .collect();
                            egui::ScrollArea::vertical().id_salt(("dq_fail", *i)).max_height(theme::LIST_MAX_H_L).auto_shrink([false, true]).show(ui, |ui| {
                                for l in &lines {
                                    ui.add(egui::Label::new(theme::mono(l.clone())).truncate());
                                }
                            });
                            let b = ui.button("Open failing rows as tab");
                            rects.buttons.push((format!("Open failing rows {}", i + 1), b.rect));
                            if b.clicked() {
                                act = Some(Act::OpenFailing(*i));
                            }
                        }
                        for rr in res.rules.iter().filter(|r| r.failing_rows.is_empty() && !r.examples.is_empty()) {
                            ui.add_space(theme::SPACE_S);
                            ui.add(egui::Label::new(egui::RichText::new(format!("{}: values that could not be read: {}", rr.name, rr.examples.join(" | "))).color(tk.muted)).wrap());
                        }
                    });
                    rects.result = Some(r);
                }
                if let Some((is_err, text)) = &self.dq.message {
                    let colour = if *is_err { tk.danger } else { tk.muted };
                    ui.add(egui::Label::new(egui::RichText::new(text).color(colour)).wrap());
                }
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let b = ui.add_enabled_ui(!self.tabs.is_empty(), |ui| theme::primary_button(ui, "Run checks")).inner;
                    rects.buttons.push(("Run checks".to_string(), b.rect));
                    if b.clicked() {
                        act = Some(Act::Run);
                    }
                    let c = ui.button("Close");
                    rects.buttons.push(("Close".to_string(), c.rect));
                    if c.clicked() {
                        close = true;
                    }
                });
            });
        });
        match act {
            Some(Act::Add) => match self.dq_draft_rule() {
                Ok(r) => {
                    self.dq.set.rules.push(r);
                    self.dq.shown = None;
                    self.dq.draft.name.clear();
                    self.dq_msg(false, "Added the rule.");
                }
                Err(e) => self.dq_msg(true, e),
            },
            Some(Act::Remove(i)) => {
                if i < self.dq.set.rules.len() {
                    self.dq.set.rules.remove(i);
                    self.dq.shown = None;
                }
            }
            Some(Act::Save) => self.dq_save(),
            Some(Act::Open(n)) => match load_ruleset(&dq_dir(), &n) {
                Ok(s) => {
                    self.dq_take_set(s);
                    self.dq_msg(false, format!("Opened the rule set '{}'.", n));
                }
                Err(e) => self.dq_msg(true, format!("{:#}", e)),
            },
            Some(Act::Run) => self.dq_run(),
            Some(Act::OpenScorecard) => self.dq_open_tab(DqView::Scorecard),
            Some(Act::OpenFailing(i)) => self.dq_open_tab(DqView::Failing(i)),
            None => {}
        }
        self.dq.rects = rects;
        ModalOutcome { close, step: None }
    }
}
