//! Screen module `rate_shock` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct ShockState {
    pub side_col: String,
    pub side_cols_seen: Vec<String>,
    pub bal_col: String,
    pub rate_col: String,
    pub mat_col: String,
    pub bps: f64,
}

impl Default for ShockState {
    fn default() -> Self {
        Self {
            side_col: String::new(),
            side_cols_seen: Vec::new(),
            bal_col: "Balance".to_string(),
            rate_col: "Int Rate".to_string(),
            mat_col: "Matdat".to_string(),
            bps: 100.0,
        }
    }
}

/// Default for the asset/liability side picker: the first column with a "side" or "direction"
/// name token. "type" is not used: Account_Type or Collateral_Type are not sides.
pub(crate) fn default_side_column(cols: &[String]) -> String {
    cols.iter()
        .find(|c| {
            column_name_tokens(c)
                .iter()
                .any(|t| t == "side" || t == "direction")
        })
        .cloned()
        .unwrap_or_default()
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_rate_shock_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        theme::modal("\u{E154} Interest Rate Shock / ΔNII Sensitivity Simulator")
            .collapsible(false)
            .show(ctx, |d| {
                d.body(|ui| {
                    ui.label("Calculates 1-Year repricing-weighted Net Interest Income impact.");
                    ui.label("NII_Impact_BDT is signed (+ asset, − liability). Shocked_Annual_Int is gross income or expense after the shock, unsigned.");
                    ui.horizontal(|ui| {
                        ui.label("Balance Col:");
                        egui::ComboBox::from_id_salt("shk_bal")
                            .selected_text(&self.shock.bal_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.shock.bal_col, c.clone(), c);
                                }
                            });
                        ui.label("Int Rate Col:");
                        egui::ComboBox::from_id_salt("shk_rt")
                            .selected_text(&self.shock.rate_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.shock.rate_col, c.clone(), c);
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Maturity Date Col:");
                        egui::ComboBox::from_id_salt("shk_mat")
                            .selected_text(&self.shock.mat_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.shock.mat_col, c.clone(), c);
                                }
                            });
                        ui.label("Cutoff Date (YYYY-MM-DD):");
                        ui.text_edit_singleline(&mut self.buckets.cutoff);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Parallel Rate Shock (bps, e.g. +100, +200, -150):");
                        ui.add(egui::DragValue::new(&mut self.shock.bps).speed(5.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Asset/Liability Side Col:");
                        let shown = if self.shock.side_col.is_empty() {
                            "(none: treat all rows as assets)".to_string()
                        } else {
                            self.shock.side_col.clone()
                        };
                        egui::ComboBox::from_id_salt("shk_side")
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
                });
                d.footer(|ui| {
                    ui.horizontal(|ui| {
                        if theme::primary_button(ui, "\u{E182} Run Rate Shock Simulation").clicked() {
                            step_to_apply = self.tool_step(&ActiveToolModal::RateShockSim).map(|s| (s, true));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_picker_default_ignores_type() {
        let cols = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(default_side_column(&cols(&["Account_Type", "Balance", "AL_Side"])), "AL_Side");
        assert_eq!(default_side_column(&cols(&["Collateral_Type", "Balance"])), "");
        assert_eq!(default_side_column(&cols(&["Direction", "Side"])), "Direction");
        assert_eq!(default_side_column(&cols(&["BalanceSide"])), "BalanceSide");
    }
}
