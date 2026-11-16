//! Screen module `duration` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct DurationState {
    pub bal_col: String,
    pub rate_col: String,
    pub mat_col: String,
    pub freq: usize,
    pub shock_bps: f64,
    pub sight_yrs: f64,
}

impl Default for DurationState {
    fn default() -> Self {
        Self {
            bal_col: "Balance".to_string(),
            rate_col: "Int Rate".to_string(),
            mat_col: "Matdat".to_string(),
            freq: 4,
            shock_bps: 200.0,
            sight_yrs: 0.0833,
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_duration_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        egui::Window::new("⏱ IRRBB Duration, DV01 & ΔEVE Sensitivity Engine")
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label("Computes Residual Years (T), Macaulay Duration, Modified Duration, DV01, and ΔEVE.");
                ui.horizontal(|ui| {
                    ui.label("Balance Col:");
                    egui::ComboBox::from_id_salt("dur_bal")
                        .selected_text(&self.duration.bal_col)
                        .show_ui(ui, |ui| {
                            for c in &avail_cols {
                                ui.selectable_value(&mut self.duration.bal_col, c.clone(), c);
                            }
                        });
                    ui.label("Int Rate / Yield Col (%):");
                    egui::ComboBox::from_id_salt("dur_rt")
                        .selected_text(&self.duration.rate_col)
                        .show_ui(ui, |ui| {
                            for c in &avail_cols {
                                ui.selectable_value(&mut self.duration.rate_col, c.clone(), c);
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Maturity Date Col:");
                    egui::ComboBox::from_id_salt("dur_mat")
                        .selected_text(&self.duration.mat_col)
                        .show_ui(ui, |ui| {
                            for c in &avail_cols {
                                ui.selectable_value(&mut self.duration.mat_col, c.clone(), c);
                            }
                        });
                    ui.label("Cutoff Date (YYYY-MM-DD):");
                    ui.text_edit_singleline(&mut self.buckets.cutoff);
                });
                ui.horizontal(|ui| {
                    ui.label("Compounding Frequency per Year (1=Ann, 2=Semi, 4=Qtr, 12=Mo):");
                    ui.add(egui::DragValue::new(&mut self.duration.freq).range(1..=12));
                });
                ui.horizontal(|ui| {
                    ui.label("Parallel Rate Shock for ΔEVE (bps):");
                    ui.add(egui::DragValue::new(&mut self.duration.shock_bps).speed(10.0));
                    ui.label("Null Matdat Tenor (Yrs):");
                    ui.add(egui::DragValue::new(&mut self.duration.sight_yrs).speed(0.05));
                });
                ui.horizontal(|ui| {
                    ui.label("Asset/Liability Side Col:");
                    let shown = if self.shock.side_col.is_empty() {
                        "(none: treat all rows as assets)".to_string()
                    } else {
                        self.shock.side_col.clone()
                    };
                    egui::ComboBox::from_id_salt("dur_side")
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.shock.side_col,
                                String::new(),
                                "(none: treat all rows as assets)",
                            );
                            for c in &avail_cols {
                                ui.selectable_value(&mut self.shock.side_col, c.clone(), c);
                            }
                        });
                });
                ui.label("Side values: A/ASSET = asset; L/LIABILITY, DEPOSIT, BORROWING = liability.");
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("✅ Compute Duration & ΔEVE Columns").clicked() {
                        step_to_apply = Some((
                            TransformStep::DurationAnalysis {
                                bal_col: self.duration.bal_col.clone(),
                                rate_col: self.duration.rate_col.clone(),
                                mat_col: self.duration.mat_col.clone(),
                                cutoff_date: self.buckets.cutoff.clone(),
                                compounding_freq: self.duration.freq,
                                shock_bps: self.duration.shock_bps,
                                default_sight_years: self.duration.sight_yrs,
                                side_col: if self.shock.side_col.trim().is_empty() {
                                    None
                                } else {
                                    Some(self.shock.side_col.clone())
                                },
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
