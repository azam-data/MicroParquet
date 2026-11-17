//! Screen module `limits` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct LimitState {
    pub col: String,
    pub bal_col: String,
    pub ccf_pct: f64,
}

impl Default for LimitState {
    fn default() -> Self {
        Self {
            col: "Sanctioned_Limit".to_string(),
            bal_col: "Balance".to_string(),
            ccf_pct: 20.0, // not a size: a percentage
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_limits_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        theme::modal("\u{E1D2} Loan Limit Utilization, Over-Limit Audit & Undrawn CCF")
            .collapsible(false)
            .show(ctx, |d| {
                d.body(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("Sanctioned Limit Col:");
                        egui::ComboBox::from_id_salt("lm_lim")
                            .selected_text(&self.limits.col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.limits.col, c.clone(), c);
                                }
                            });
                        ui.label("Outstanding Balance Col:");
                        egui::ComboBox::from_id_salt("lm_bal")
                            .selected_text(&self.limits.bal_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.limits.bal_col, c.clone(), c);
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Credit Conversion Factor (CCF % for Undrawn Portion):");
                        ui.add(egui::DragValue::new(&mut self.limits.ccf_pct).speed(1.0));
                    });
                });
                d.footer(|ui| {
                    ui.horizontal(|ui| {
                        if theme::primary_button(ui, "\u{E182} Calculate Limit Utilization & Undrawn CCF").clicked() {
                            step_to_apply = self.tool_step(&ActiveToolModal::LoanLimitUtil).map(|s| (s, false));
                            close_modal = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_modal = true;
                        }
                    });
                });
            });
        ModalOutcome { close: close_modal, step: step_to_apply }
    }
}
