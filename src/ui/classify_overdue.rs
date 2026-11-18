//! Banking ▸ Loans & credit ▸ "Classify by overdue" (Phase 5 R4): the loan category, the
//! overdue-since date or days-past-due column, an optional override grade column and the cutoff
//! date make the step that adds `Overdue_Months`, `CL_Objective`, `CL_Final` and `CL_Rule_File`.

use crate::ui::pick::{col_combo, field_label, guess_column, text_combo, DialogRects};
use crate::ui::prelude::*;
use micro_parquet::overdue::load_overdue_rules;

pub const OVERDUE_TITLE: &str = "Classify by overdue (BRPD 15/2024)";

pub struct OverdueState {
    /// The category comes from a column (through loan_category_map.json) instead of one category for all rows.
    pub category_by_col: bool,
    pub category_col: String,
    pub fixed_category: String,
    /// The overdue period is a days-past-due column instead of an overdue-since date column.
    pub use_dpd: bool,
    pub since_col: String,
    pub dpd_col: String,
    pub override_on: bool,
    pub override_col: String,
    pub amount_on: bool,
    pub amount_col: String,
    pub cutoff: String,
    /// The category names of the rule file in force at the cutoff date (the date they were read for).
    pub categories: Option<(String, Result<Vec<String>, String>)>,
    pub rects: DialogRects,
}

impl Default for OverdueState {
    fn default() -> Self {
        Self {
            category_by_col: false,
            category_col: String::new(),
            fixed_category: "Continuous".to_string(),
            use_dpd: false,
            since_col: String::new(),
            dpd_col: String::new(),
            override_on: false,
            override_col: String::new(),
            amount_on: false,
            amount_col: String::new(),
            cutoff: String::new(),
            categories: None,
            rects: DialogRects::default(),
        }
    }
}

impl OverdueState {
    /// The column choices guessed from the names of a table's columns.
    pub fn guess(&mut self, cols: &[String]) {
        self.category_col = guess_column(cols, &["category", "loan_type", "facility", "loantype", "product_type"]);
        self.category_by_col = !self.category_col.is_empty();
        self.since_col = guess_column(cols, &["overdue_since", "od_since", "overdue_date", "since"]);
        self.dpd_col = guess_column(cols, &["dpd", "days_past", "overdue_days", "days_overdue"]);
        self.use_dpd = self.since_col.is_empty() && !self.dpd_col.is_empty();
        self.override_col = guess_column(cols, &["override", "subjective", "judgemental", "judgmental"]);
        self.override_on = !self.override_col.is_empty();
        self.amount_col = guess_column(cols, &["overdue_amt", "overdue_amount", "arrear", "od_amount"]);
        self.amount_on = !self.amount_col.is_empty();
    }
}

impl MicroParquetApp {
    pub(crate) fn open_classify_overdue(&mut self) {
        let cols = self.tabs.get(self.active_tab).map(|t| t.available_columns()).unwrap_or_default();
        if self.overdue.cutoff.trim().is_empty() {
            self.overdue.cutoff = crate::ui::cashflow_ladder::last_month_end(chrono::Local::now().date_naive());
        }
        self.overdue.guess(&cols);
        self.overdue.categories = None;
        self.active_tool = ActiveToolModal::ClassifyOverdue;
    }

    /// The category names of the rule file for the typed cutoff date, read again when the date changed.
    fn refresh_overdue_categories(&mut self) {
        let key = self.overdue.cutoff.trim().to_string();
        if matches!(&self.overdue.categories, Some((k, _)) if *k == key) {
            return;
        }
        let r = match chrono::NaiveDate::parse_from_str(&key, "%Y-%m-%d") {
            Ok(d) => load_overdue_rules(d).map(|f| f.data.categories.iter().map(|c| c.name.clone()).collect()).map_err(|e| format!("{:#}", e)),
            Err(_) => Err("Type the cutoff date as YYYY-MM-DD (for example 2026-10-31).".to_string()),
        };
        self.overdue.categories = Some((key, r));
    }

    /// The step the dialog would make, or the first thing that is missing.
    pub(crate) fn classify_overdue_step(&self) -> Result<TransformStep, String> {
        let s = &self.overdue;
        let cutoff = s.cutoff.trim().to_string();
        if chrono::NaiveDate::parse_from_str(&cutoff, "%Y-%m-%d").is_err() {
            return Err("Type the cutoff date as YYYY-MM-DD (for example 2026-10-31).".to_string());
        }
        if let Some((k, Err(e))) = &s.categories {
            if *k == cutoff {
                return Err(e.clone());
            }
        }
        let some = |on: bool, v: &str| if on && !v.trim().is_empty() { Some(v.trim().to_string()) } else { None };
        if s.category_by_col && s.category_col.trim().is_empty() {
            return Err("Choose the loan category column, or choose the same category for every row.".to_string());
        }
        if !s.category_by_col && s.fixed_category.trim().is_empty() {
            return Err("Choose the loan category of the rows.".to_string());
        }
        if s.use_dpd && s.dpd_col.trim().is_empty() {
            return Err("Choose the days-past-due column.".to_string());
        }
        if !s.use_dpd && s.since_col.trim().is_empty() {
            return Err("Choose the overdue-since date column.".to_string());
        }
        if s.override_on && s.override_col.trim().is_empty() {
            return Err("Choose the override grade column, or untick it.".to_string());
        }
        if s.amount_on && s.amount_col.trim().is_empty() {
            return Err("Choose the overdue amount column, or untick it.".to_string());
        }
        Ok(TransformStep::ClassifyOverdue {
            category_col: some(s.category_by_col, &s.category_col),
            fixed_category: if s.category_by_col { String::new() } else { s.fixed_category.trim().to_string() },
            since_col: some(!s.use_dpd, &s.since_col),
            dpd_col: some(s.use_dpd, &s.dpd_col),
            override_col: some(s.override_on, &s.override_col),
            overdue_amount_col: some(s.amount_on, &s.amount_col),
            cutoff_date: cutoff,
        })
    }

    pub(crate) fn render_classify_overdue_modal(&mut self, ctx: &egui::Context, cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let mut step: Option<(TransformStep, bool)> = None;
        let tk = *theme::tokens(self.dark_mode);
        let title = format!("{}  {}", theme::ph::BANK, OVERDUE_TITLE);
        let mut rects = DialogRects::default();
        self.refresh_overdue_categories();
        let problem = self.classify_overdue_step().err();
        let names: Vec<String> = match &self.overdue.categories {
            Some((_, Ok(n))) => n.clone(),
            _ => Vec::new(),
        };
        theme::modal(&title).default_width(theme::DIALOG_W_TOOL).show(ctx, |d| {
            d.header(|ui| {
                ui.horizontal_wrapped(|ui| {
                    field_label(ui, &tk, "Cutoff date (YYYY-MM-DD)");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.overdue.cutoff).desired_width(theme::CF_DEFAULT_W));
                    rects.controls.push(("Cutoff date".to_string(), r.rect));
                });
                if let Some(p) = &problem {
                    theme::error_label(ui, p);
                }
            });
            d.body_scroll(|ui| {
                let s = &mut self.overdue;
                ui.horizontal_wrapped(|ui| {
                    field_label(ui, &tk, "Loan category");
                    let a = ui.radio_value(&mut s.category_by_col, false, "Same for every row:");
                    rects.controls.push(("Same category".to_string(), a.rect));
                    ui.add_enabled_ui(!s.category_by_col, |ui| {
                        let r = text_combo(ui, "ov_fixed", &mut s.fixed_category, &names, theme::CF_COMBO_W);
                        rects.controls.push(("Fixed category".to_string(), r));
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(theme::SPACE_L);
                    let b = ui.radio_value(&mut s.category_by_col, true, "From the column:");
                    rects.controls.push(("Category from column".to_string(), b.rect));
                    ui.add_enabled_ui(s.category_by_col, |ui| {
                        let r = col_combo(ui, "ov_cat", &mut s.category_col, &cols, false);
                        rects.controls.push(("Category column".to_string(), r));
                    });
                });
                ui.add_space(theme::SPACE_S);
                ui.horizontal_wrapped(|ui| {
                    field_label(ui, &tk, "Overdue period");
                    let a = ui.radio_value(&mut s.use_dpd, false, "Overdue-since date column:");
                    rects.controls.push(("Use overdue-since".to_string(), a.rect));
                    ui.add_enabled_ui(!s.use_dpd, |ui| {
                        let r = col_combo(ui, "ov_since", &mut s.since_col, &cols, false);
                        rects.controls.push(("Overdue-since column".to_string(), r));
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(theme::SPACE_L);
                    let b = ui.radio_value(&mut s.use_dpd, true, "Days-past-due column:");
                    rects.controls.push(("Use days past due".to_string(), b.rect));
                    ui.add_enabled_ui(s.use_dpd, |ui| {
                        let r = col_combo(ui, "ov_dpd", &mut s.dpd_col, &cols, false);
                        rects.controls.push(("Days-past-due column".to_string(), r));
                    });
                });
                ui.add_space(theme::SPACE_S);
                ui.horizontal_wrapped(|ui| {
                    let c = ui.checkbox(&mut s.override_on, "Override grade column (the worse grade wins):");
                    rects.controls.push(("Override".to_string(), c.rect));
                    ui.add_enabled_ui(s.override_on, |ui| {
                        let r = col_combo(ui, "ov_over", &mut s.override_col, &cols, false);
                        rects.controls.push(("Override column".to_string(), r));
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    let c = ui.checkbox(&mut s.amount_on, "Overdue amount column (an amount above 0 needs a date):");
                    rects.controls.push(("Overdue amount".to_string(), c.rect));
                    ui.add_enabled_ui(s.amount_on, |ui| {
                        let r = col_combo(ui, "ov_amt", &mut s.amount_col, &cols, false);
                        rects.controls.push(("Overdue amount column".to_string(), r));
                    });
                });
                ui.add_space(theme::SPACE_S);
                let notes = ui.scope(|ui| {
                    ui.set_max_width(ui.available_width());
                    let muted = |t: &str| egui::Label::new(egui::RichText::new(t).color(tk.muted).size(theme::TEXT_LABEL)).wrap();
                    ui.add(muted("Overdue months are complete calendar months from the overdue-since date to the cutoff (31 Jan + 1 month is the end of February). With days past due, the overdue-since date is the cutoff less those days. A blank date or blank days past due counts as not overdue (STD-0)."));
                    ui.add(muted("The grade is the worse of the overdue grade and the override grade. The thresholds come from the dated rule file portable_data/rules/cl_overdue; the loan category codes from portable_data/loan_category_map.json; the grade codes are those of cl_code_map.json, so the Provision step and the CL migration matrix run on CL_Final."));
                });
                rects.notes = notes.response.rect;
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let b = ui.add_enabled_ui(problem.is_none(), |ui| theme::primary_button(ui, "Classify by overdue")).inner;
                    rects.buttons.push(("Classify by overdue".to_string(), b.rect));
                    if b.clicked() {
                        step = self.classify_overdue_step().ok().map(|s| (s, false));
                        close = true;
                    }
                    let c = ui.button("Cancel");
                    rects.buttons.push(("Cancel".to_string(), c.rect));
                    if c.clicked() {
                        close = true;
                    }
                });
            });
        });
        self.overdue.rects = rects;
        ModalOutcome { close, step }
    }
}
