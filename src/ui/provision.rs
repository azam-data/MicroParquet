//! Screen module `provision` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct ProvisionState {
    pub bal_col: String,
    pub cl_col: String,
    pub seg_col: String,
    pub susp_col: String,
    pub sec_col: String,
    pub coll_type_col: String,
    pub cutoff: String,
}

impl Default for ProvisionState {
    fn default() -> Self {
        Self {
            bal_col: "Balance".to_string(),
            cl_col: "CL_Status".to_string(),
            seg_col: "Sector".to_string(),
            susp_col: String::new(),
            sec_col: String::new(),
            coll_type_col: String::new(),
            cutoff: String::new(),
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_provision_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        theme::modal("\u{E0B4} Bangladesh Bank CL Provision & BFP Calculator")
            .collapsible(false)
            .show(ctx, |d| {
                d.body(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("Outstanding Balance Col:");
                        egui::ComboBox::from_id_salt("ln_bal")
                            .selected_text(&self.provision.bal_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.provision.bal_col, c.clone(), c);
                                }
                            });
                        ui.label("CL Status Col (UC/SMA/SS/DF/BL):");
                        egui::ComboBox::from_id_salt("ln_cl")
                            .selected_text(&self.provision.cl_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.provision.cl_col, c.clone(), c);
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Optional Segment/Sector Col:");
                        ui.text_edit_singleline(&mut self.provision.seg_col);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Optional Interest Suspense Col:");
                        ui.text_edit_singleline(&mut self.provision.susp_col);
                        ui.label("Optional Eligible Security Col:");
                        ui.text_edit_singleline(&mut self.provision.sec_col);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Optional Collateral Type Col:");
                        ui.text_edit_singleline(&mut self.provision.coll_type_col);
                        ui.label("Cutoff Date (YYYY-MM-DD, blank = today):");
                        ui.text_edit_singleline(&mut self.provision.cutoff);
                    });
                    ui.label("Rates, CL codes and segments come from cl_code_map.json, segment_map.json and provision_rules.json in portable_data.");
                });
                d.footer(|ui| {
                    ui.horizontal(|ui| {
                        if theme::primary_button(ui, "\u{E182} Calculate CL Provision & BFP").clicked() {
                            step_to_apply = self.tool_step(&ActiveToolModal::LoanProvision).map(|s| (s, false));
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
