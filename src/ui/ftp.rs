//! Screen module `ftp` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct FtpState {
    pub opn_col: String,
    pub mat_col: String,
    pub bal_col: String,
    pub rate_col: String,
    pub is_liability: bool,
    pub curve: Vec<FtpPoint>,
    pub interpolation: FtpInterpolation,
}

impl Default for FtpState {
    fn default() -> Self {
        Self {
            opn_col: "Opndat".to_string(),
            mat_col: "Matdat".to_string(),
            bal_col: "Balance".to_string(),
            rate_col: "Int Rate".to_string(),
            is_liability: true,
            curve: default_bd_ftp_curve(),
            interpolation: FtpInterpolation::Linear,
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_ftp_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        theme::modal("\u{E538} Automated FTP Yield Curve Tenor Mapper")
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Open Date:");
                    egui::ComboBox::from_id_salt("ftp_opn")
                        .selected_text(&self.ftp.opn_col)
                        .show_ui(ui, |ui| {
                            for c in &avail_cols {
                                ui.selectable_value(&mut self.ftp.opn_col, c.clone(), c);
                            }
                        });
                    ui.label("Maturity Date:");
                    egui::ComboBox::from_id_salt("ftp_mat")
                        .selected_text(&self.ftp.mat_col)
                        .show_ui(ui, |ui| {
                            for c in &avail_cols {
                                ui.selectable_value(&mut self.ftp.mat_col, c.clone(), c);
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Balance Col:");
                    egui::ComboBox::from_id_salt("ftp_bal")
                        .selected_text(&self.ftp.bal_col)
                        .show_ui(ui, |ui| {
                            for c in &avail_cols {
                                ui.selectable_value(&mut self.ftp.bal_col, c.clone(), c);
                            }
                        });
                    ui.label("Int Rate Col:");
                    egui::ComboBox::from_id_salt("ftp_rt")
                        .selected_text(&self.ftp.rate_col)
                        .show_ui(ui, |ui| {
                            for c in &avail_cols {
                                ui.selectable_value(&mut self.ftp.rate_col, c.clone(), c);
                            }
                        });
                });
                ui.checkbox(
                    &mut self.ftp.is_liability,
                    "Liability Book (Deposits/Borrowings: Spread = FTP Rate - Account Rate)",
                );
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Rate between tenor points:");
                    for m in [FtpInterpolation::Linear, FtpInterpolation::NextPointUp] {
                        ui.radio_value(&mut self.ftp.interpolation, m, m.label());
                    }
                });
                ui.label("Below the first point the first rate applies; beyond the last point the last rate. The FTP_Method column shows which rule each row used.");
                ui.strong("Benchmark Yield Curve (Tenor Days -> FTP Rate %):");
                for pt in &mut self.ftp.curve {
                    ui.horizontal(|ui| {
                        ui.label("Tenor Days:");
                        ui.add(egui::DragValue::new(&mut pt.tenor_days));
                        ui.label("FTP %:");
                        ui.add(egui::DragValue::new(&mut pt.ftp_rate).speed(0.05));
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "\u{E182} Map FTP Rates & Compute Margin").clicked() {
                        step_to_apply = Some((
                            TransformStep::FtpCurveMap {
                                opndat_col: self.ftp.opn_col.clone(),
                                matdat_col: self.ftp.mat_col.clone(),
                                bal_col: self.ftp.bal_col.clone(),
                                rate_col: self.ftp.rate_col.clone(),
                                curve: self.ftp.curve.clone(),
                                is_liability: self.ftp.is_liability,
                                interpolation: self.ftp.interpolation,
                            },
                            false,
                        ));
                        close_modal = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close_modal = true;
                    }
                });
            });
        ModalOutcome { close: close_modal, step: step_to_apply }
    }
}
