//! Screen module `reconcile` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct ReconcileState {
    pub prev_tab: usize,
    pub curr_tab: usize,
    pub key_col: String,
    pub bal_col: String,
    pub rate_col: String,
    pub cl_col: String,
}

impl Default for ReconcileState {
    fn default() -> Self {
        Self {
            prev_tab: 0,
            curr_tab: 0,
            key_col: "Actnum".to_string(),
            bal_col: "Balance".to_string(),
            rate_col: "Int Rate".to_string(),
            cl_col: String::new(),
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_reconcile_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let step_to_apply: Option<(TransformStep, bool)> = None;
        egui::Window::new("⚖ Two-Period Variance & Loan CL Migration Reconciler")
            .collapsible(false)
            .show(ctx, |ui| {
                if self.tabs.len() < 2 {
                    ui.label(
                        "Open Previous Period (e.g. Sep) and Current Period (e.g. Oct) in 2 tabs first.",
                    );
                } else {
                    let prev_title = self
                        .tabs
                        .get(self.recon.prev_tab)
                        .map(|t| t.title.clone())
                        .unwrap_or_default();
                    let curr_title = self
                        .tabs
                        .get(self.recon.curr_tab)
                        .map(|t| t.title.clone())
                        .unwrap_or_default();
                    let tab_titles: Vec<(usize, String)> = self
                        .tabs
                        .iter()
                        .enumerate()
                        .map(|(i, t)| (i, t.title.clone()))
                        .collect();

                    ui.horizontal(|ui| {
                        ui.label("Previous Period Tab:");
                        egui::ComboBox::from_id_salt("rec_p")
                            .selected_text(&prev_title)
                            .show_ui(ui, |ui| {
                                for (i, title) in &tab_titles {
                                    ui.selectable_value(&mut self.recon.prev_tab, *i, title);
                                }
                            });
                        ui.label("Current Period Tab:");
                        egui::ComboBox::from_id_salt("rec_c")
                            .selected_text(&curr_title)
                            .show_ui(ui, |ui| {
                                for (i, title) in &tab_titles {
                                    ui.selectable_value(&mut self.recon.curr_tab, *i, title);
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Primary Key (e.g. Actnum):");
                        ui.text_edit_singleline(&mut self.recon.key_col);
                        ui.label("Balance Col:");
                        ui.text_edit_singleline(&mut self.recon.bal_col);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Optional Rate Col (Int Rate):");
                        ui.text_edit_singleline(&mut self.recon.rate_col);
                        ui.label("Optional Loan CL Col (CL_Status):");
                        ui.text_edit_singleline(&mut self.recon.cl_col);
                    });
                    ui.separator();
                    if ui.button("🚀 Reconcile Periods into New Tab").clicked() {
                        let (Some(p_tab), Some(c_tab)) = (
                            self.tabs.get(self.recon.prev_tab).cloned(),
                            self.tabs.get(self.recon.curr_tab).cloned(),
                        ) else {
                            self.status_message =
                                "Reconcile: the selected previous or current tab no longer exists. Pick both tabs again.".to_string();
                            return;
                        };
                        let r_col = if self.recon.rate_col.trim().is_empty() {
                            None
                        } else {
                            Some(self.recon.rate_col.trim().to_string())
                        };
                        let cl_col = if self.recon.cl_col.trim().is_empty() {
                            None
                        } else {
                            Some(self.recon.cl_col.trim().to_string())
                        };
                        let tabs_before = self.tabs.len();
                        let r_col_used = r_col.is_some();
                        self.create_tab_from_source(
                            format!("Reconcile({} vs {})", p_tab.title, c_tab.title),
                            TabSource::PeriodReconcile {
                                prev_source: Box::new(p_tab.source),
                                prev_steps: p_tab.steps,
                                curr_source: Box::new(c_tab.source),
                                curr_steps: c_tab.steps,
                                key_col: self.recon.key_col.trim().to_string(),
                                bal_col: self.recon.bal_col.trim().to_string(),
                                rate_col: r_col,
                                cl_status_col: cl_col,
                            },
                            Vec::new(),
                            HashMap::new(),
                        );
                        if self.tabs.len() > tabs_before {
                            let note = self.tabs.last().map(|t| {
                                compile_tab_lazy(&t.source, &t.steps, &self.paths.temp_dir)
                                    .and_then(|lf| {
                                        micro_parquet::banking::reconcile_status_note(lf, r_col_used)
                                    })
                            });
                            match note {
                                Some(Ok(n)) => self.status_message.push_str(&n),
                                Some(Err(e)) => self.status_message.push_str(&format!(
                                    " ⚠ Could not summarise the reconcile (keys with several rows, mixed-sign balances): {:#}",
                                    e
                                )),
                                None => {}
                            }
                        }
                        close_modal = true;
                    }
                }
                if ui.button("Cancel").clicked() {
                    close_modal = true;
                }
            });
        ModalOutcome { close: close_modal, step: step_to_apply }
    }
}
