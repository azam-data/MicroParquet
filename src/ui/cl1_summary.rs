//! Banking ▸ Loans & credit ▸ "CL-1 summary" (Phase 5 R5): the loans of a tab by loan category and
//! grade group, laid out by the template `portable_data/report_templates/cl1.json`, and the
//! Excel export of the result.

use crate::ui::pick::{col_combo, field_label, guess_column, text_combo, DialogRects};
use crate::ui::prelude::*;
use micro_parquet::cl1::{load_template, Cl1Spec, Cl1Template, MEASURE_IDS};

pub const CL1_TITLE: &str = "CL-1 summary";

pub struct Cl1State {
    pub category_by_col: bool,
    pub category_col: String,
    pub fixed_category: String,
    pub grade_col: String,
    /// (measure id, column) in the order of `MEASURE_IDS`.
    pub measure_cols: Vec<(String, String)>,
    /// The template as read when the dialog opened.
    pub template: Option<Result<Cl1Template, String>>,
    pub rects: DialogRects,
}

impl Default for Cl1State {
    fn default() -> Self {
        Self {
            category_by_col: false,
            category_col: String::new(),
            fixed_category: String::new(),
            grade_col: String::new(),
            measure_cols: MEASURE_IDS.iter().map(|id| (id.to_string(), String::new())).collect(),
            template: None,
            rects: DialogRects::default(),
        }
    }
}

impl MicroParquetApp {
    pub(crate) fn open_cl1_summary(&mut self) {
        let cols = self.tabs.get(self.active_tab).map(|t| t.available_columns()).unwrap_or_default();
        let s = &mut self.cl1;
        s.category_col = guess_column(&cols, &["category", "loan_type", "facility", "loantype"]);
        s.category_by_col = !s.category_col.is_empty();
        s.grade_col = if cols.iter().any(|c| c == "CL_Final") { "CL_Final".to_string() } else { guess_column(&cols, &["cl_grade", "cl_status", "classification"]) };
        let find = |id: &str| -> String {
            match id {
                "outstanding" => guess_column(&cols, &["balance", "outstanding"]),
                "base" => guess_column(&cols, &["base_for_provision", "bfp"]),
                _ => guess_column(&cols, &["required_provision", "provision_required"]),
            }
        };
        s.measure_cols = MEASURE_IDS.iter().map(|id| (id.to_string(), find(id))).collect();
        s.template = Some(load_template().map_err(|e| format!("{:#}", e)));
        if let Some(Ok(t)) = &s.template {
            if s.fixed_category.trim().is_empty() || !t.categories.contains(&s.fixed_category) {
                s.fixed_category = t.categories.first().cloned().unwrap_or_default();
            }
        }
        self.active_tool = ActiveToolModal::Cl1Summary;
    }

    /// What the dialog would build (the title and the tab source), or the first thing that is missing.
    pub(crate) fn cl1_source(&self) -> Result<(String, TabSource), String> {
        let s = &self.cl1;
        let template = match &s.template {
            Some(Ok(t)) => t.clone(),
            Some(Err(e)) => return Err(e.clone()),
            None => return Err("The template has not been read yet.".to_string()),
        };
        let tab = self.tabs.get(self.active_tab).ok_or_else(|| "Open the tab with the loans first.".to_string())?;
        let cols = tab.available_columns();
        let has = |c: &str| !c.trim().is_empty() && cols.iter().any(|x| x == c);
        if s.category_by_col && !has(&s.category_col) {
            return Err("Choose the loan category column, or choose the same category for every row.".to_string());
        }
        if !s.category_by_col && s.fixed_category.trim().is_empty() {
            return Err("Choose the loan category of the rows.".to_string());
        }
        if !has(&s.grade_col) {
            return Err("Choose the grade column (CL_Final from Classify by overdue).".to_string());
        }
        let mut measure_cols = Vec::new();
        for m in &template.measures {
            let c = s.measure_cols.iter().find(|(id, _)| *id == m.id).map(|(_, c)| c.clone()).unwrap_or_default();
            if !has(&c) {
                return Err(format!("Choose the column of '{}'.", m.label));
            }
            measure_cols.push((m.id.clone(), c));
        }
        let spec = Cl1Spec {
            category_col: if s.category_by_col { Some(s.category_col.clone()) } else { None },
            fixed_category: if s.category_by_col { String::new() } else { s.fixed_category.clone() },
            grade_col: s.grade_col.clone(),
            measure_cols,
        };
        Ok((CL1_TITLE.to_string(), TabSource::Cl1Summary { source: Box::new(tab.source.clone()), steps: tab.steps.clone(), spec, template: Box::new(template) }))
    }

    pub(crate) fn render_cl1_modal(&mut self, ctx: &egui::Context, cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let mut build = false;
        let tk = *theme::tokens(self.dark_mode);
        let title = format!("{}  {}", theme::ph::TABLE, CL1_TITLE);
        let mut rects = DialogRects::default();
        let problem = self.cl1_source().err();
        let (sample, template_line, categories, measures): (bool, String, Vec<String>, Vec<(String, String)>) = match &self.cl1.template {
            Some(Ok(t)) => (t.sample, format!("Layout: report_templates/cl1.json. {}", t.source), t.categories.clone(), t.measures.iter().map(|m| (m.id.clone(), m.label.clone())).collect()),
            _ => (false, String::new(), Vec::new(), Vec::new()),
        };
        theme::modal(&title).default_width(theme::DIALOG_W_TOOL).show(ctx, |d| {
            d.header(|ui| {
                if sample {
                    let r = ui.scope(|ui| theme::warning_label(ui, "Sample parameters: the CL-1 template in use is marked sample. Align it with the current CL-1 form before the figures go into a return."));
                    rects.banner = r.response.rect;
                }
                if let Some(p) = &problem {
                    theme::error_label(ui, p);
                }
            });
            d.body_scroll(|ui| {
                let s = &mut self.cl1;
                ui.horizontal_wrapped(|ui| {
                    field_label(ui, &tk, "Loan category");
                    let a = ui.radio_value(&mut s.category_by_col, false, "Same for every row:");
                    rects.controls.push(("Same category".to_string(), a.rect));
                    ui.add_enabled_ui(!s.category_by_col, |ui| {
                        let r = text_combo(ui, "cl1_fixed", &mut s.fixed_category, &categories, theme::CF_COMBO_W);
                        rects.controls.push(("Fixed category".to_string(), r));
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(theme::SPACE_L);
                    let b = ui.radio_value(&mut s.category_by_col, true, "From the column:");
                    rects.controls.push(("Category from column".to_string(), b.rect));
                    ui.add_enabled_ui(s.category_by_col, |ui| {
                        let r = col_combo(ui, "cl1_cat", &mut s.category_col, &cols, false);
                        rects.controls.push(("Category column".to_string(), r));
                    });
                });
                ui.add_space(theme::SPACE_S);
                ui.horizontal_wrapped(|ui| {
                    field_label(ui, &tk, "Grade column");
                    let r = col_combo(ui, "cl1_grade", &mut s.grade_col, &cols, false);
                    rects.controls.push(("Grade column".to_string(), r));
                });
                ui.add_space(theme::SPACE_S);
                egui::Grid::new("cl1_measures").num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_S)).show(ui, |ui| {
                    for h in ["Measure", "Column"] {
                        ui.label(egui::RichText::new(h).color(tk.muted).size(theme::TEXT_LABEL));
                    }
                    ui.end_row();
                    for (id, label) in &measures {
                        ui.label(label.as_str());
                        if let Some(slot) = s.measure_cols.iter_mut().find(|(i, _)| i == id) {
                            let r = col_combo(ui, &format!("cl1_m_{id}"), &mut slot.1, &cols, false);
                            rects.controls.push((label.clone(), r));
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(theme::SPACE_S);
                let notes = ui.scope(|ui| {
                    ui.set_max_width(ui.available_width());
                    let muted = |t: &str| egui::Label::new(egui::RichText::new(t).color(tk.muted).size(theme::TEXT_LABEL)).wrap();
                    if !template_line.is_empty() {
                        ui.add(muted(&template_line));
                    }
                    ui.add(muted("Rows are the loan categories of the template, in its order; columns are its grade groups (STD is STD-0, STD-1 and STD-2) with the measures under each, then the total across grades; the last row is the column total. The grade column must hold CL_Final from Classify by overdue (a Phase 1 grade such as STD counts as its first sub-grade). The measures come from the columns you choose: the balance, and Base_For_Provision_BDT and Required_Provision_BDT from the Provision step. Then use Export \u{25b8} Export the CL-1 summary to Excel\u{2026}."));
                });
                rects.notes = notes.response.rect;
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let b = ui.add_enabled_ui(problem.is_none(), |ui| theme::primary_button(ui, "Build CL-1 summary")).inner;
                    rects.buttons.push(("Build CL-1 summary".to_string(), b.rect));
                    if b.clicked() {
                        build = true;
                    }
                    let c = ui.button("Cancel");
                    rects.buttons.push(("Cancel".to_string(), c.rect));
                    if c.clicked() {
                        close = true;
                    }
                });
            });
        });
        self.cl1.rects = rects;
        if build {
            if let Ok((title, source)) = self.cl1_source() {
                self.create_tab_from_source_with(title, source, Vec::new(), HashMap::new(), None);
                close = true;
            }
        }
        ModalOutcome { close, step: None }
    }

    /// Export ▸ "Export the CL-1 summary to Excel…": asks for the file, then writes the active tab.
    pub(crate) fn export_cl1_excel(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            self.status_message = "Open a CL-1 summary tab first.".to_string();
            return;
        };
        if !matches!(tab.source, TabSource::Cl1Summary { .. }) {
            self.status_message = "The active tab is not a CL-1 summary. Build one first (Banking \u{25b8} Loans & credit \u{25b8} CL-1 summary), then export it.".to_string();
            return;
        }
        let Some(out_path) = crate::ui::save_dialog::choose_save_path(&crate::ui::save_dialog::SaveSpec::new("Excel workbook (*.xlsx)", "xlsx", micro_parquet::paths::default_export_file_name(&tab.title, "xlsx"))) else {
            return;
        };
        self.export_cl1_excel_to(out_path);
    }

    /// Writes the active CL-1 tab to `out_path` in a job (the file dialog is not part of this, so tests can call it).
    pub(crate) fn export_cl1_excel_to(&mut self, out_path: std::path::PathBuf) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let source = tab.source.clone();
        let (temp, fmt, dec) = (self.paths.temp_dir.clone(), self.num_format, self.global_decimals);
        let label = format!("CL-1 Excel export {}", out_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
        self.jobs.submit(JobKey::unique(JobKind::Export), label, true, move |ctx| {
            micro_parquet::tab_jobs::export_cl1_xlsx(&source, &temp, &out_path, fmt, dec, ctx).map(JobOutput::Exported)
        });
        self.status_message = "Export started\u{2026}".to_string();
    }
}
