//! Screen module `audit` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct AuditState {
    pub opn_col: String,
    pub mat_col: String,
    pub cls_col: String,
    pub stat_col: String,
    pub bal_col: String,
    pub rate_col: String,
    pub key_col: String,
    pub only_exceptions: bool,
    /// Comma-separated status codes that mean active (hygiene audit).
    pub active_statuses: String,
    pub rate_min: f64,
    pub rate_max: f64,
}

impl Default for AuditState {
    fn default() -> Self {
        Self {
            opn_col: "Opndat".to_string(),
            mat_col: "Matdat".to_string(),
            cls_col: "Clsdat".to_string(),
            stat_col: "Acstat".to_string(),
            bal_col: "Balance".to_string(),
            rate_col: "Int Rate".to_string(),
            key_col: "Actnum".to_string(),
            only_exceptions: true,
            active_statuses: default_active_statuses().join(", "),
            rate_min: 0.0,
            rate_max: default_rate_max_pct(),
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_audit_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        theme::modal("\u{E40C} Core Banking Dirty Data & Ghost Account Auditor")
            .collapsible(false)
            .show(ctx, |d| {
                d.body(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("Open Date:");
                        ui.text_edit_singleline(&mut self.audit.opn_col);
                        ui.label("Maturity Date:");
                        ui.text_edit_singleline(&mut self.audit.mat_col);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Close Date:");
                        ui.text_edit_singleline(&mut self.audit.cls_col);
                        ui.label("Status (Acstat):");
                        ui.text_edit_singleline(&mut self.audit.stat_col);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Balance Col:");
                        ui.text_edit_singleline(&mut self.audit.bal_col);
                        ui.label("Int Rate Col:");
                        ui.text_edit_singleline(&mut self.audit.rate_col);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Reporting Cutoff Date (YYYY-MM-DD):");
                        ui.text_edit_singleline(&mut self.buckets.cutoff);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Active status codes (comma-separated, any case):");
                        ui.text_edit_singleline(&mut self.audit.active_statuses);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Rate outlier below %:");
                        ui.add(egui::DragValue::new(&mut self.audit.rate_min).speed(0.1));
                        ui.label("or above %:");
                        ui.add(egui::DragValue::new(&mut self.audit.rate_max).speed(0.1));
                    });
                    ui.label("Audit_Exception_Flag shows the first flag; Audit_All_Flags lists every flag that applies.");
                    ui.checkbox(
                        &mut self.audit.only_exceptions,
                        "Show Exception Rows Only (Filter out 'OK' accounts)",
                    );
                });
                d.footer(|ui| {
                    ui.horizontal(|ui| {
                        if theme::primary_button(ui, "\u{E3FE} Run Audit into New Exception Tab").clicked() {
                            step_to_apply = self.tool_step(&ActiveToolModal::BankingAudit).map(|s| (s, true));
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
