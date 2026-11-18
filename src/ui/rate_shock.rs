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
    /// Empty: no repricing date (the maturity is used).
    pub reprice_col: String,
    /// The beta file chosen (a name in `portable_data/betas`); empty: no betas.
    pub beta_file: String,
    pub product_col: String,
    pub use_floor: bool,
    /// The floor of the shocked rate, in percent.
    pub floor_pct: f64,
    /// The beta files that exist, read when the dialog opens.
    pub beta_files: Vec<String>,
}

impl ShockState {
    fn given(s: &str) -> Option<String> {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    }
    pub fn reprice_choice(&self) -> Option<String> {
        Self::given(&self.reprice_col)
    }
    pub fn beta_choice(&self) -> Option<String> {
        Self::given(&self.beta_file)
    }
    pub fn product_choice(&self) -> Option<String> {
        if self.beta_choice().is_some() {
            Self::given(&self.product_col)
        } else {
            None
        }
    }
    pub fn floor_choice(&self) -> Option<f64> {
        self.use_floor.then_some(self.floor_pct)
    }
    pub fn reload_beta_files(&mut self) {
        self.beta_files = micro_parquet::betas::list_beta_files(&micro_parquet::betas::betas_dir());
    }
}

impl Default for ShockState {
    fn default() -> Self {
        Self {
            side_col: String::new(),
            side_cols_seen: Vec::new(),
            bal_col: "Balance".to_string(),
            rate_col: "Int Rate".to_string(),
            mat_col: "Matdat".to_string(),
            bps: 100.0, // not a size: basis points or a drag speed
            reprice_col: String::new(),
            beta_file: String::new(),
            product_col: String::new(),
            use_floor: false,
            floor_pct: 0.0,
            beta_files: Vec::new(),
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
        let mut open_gap_summary = false;
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
                        ui.add(egui::DragValue::new(&mut self.shock.bps).speed(5.0)); // not a size: basis points or a drag speed
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
                    ui.separator();
                    ui.strong("Repricing, pass-through and floor (all optional):");
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Repricing Date Col:");
                        let shown = if self.shock.reprice_col.is_empty() { "(none: use the maturity)".to_string() } else { self.shock.reprice_col.clone() };
                        egui::ComboBox::from_id_salt("shk_rep").width(theme::GAP_COMBO_W).selected_text(shown).show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.shock.reprice_col, String::new(), "(none: use the maturity)");
                            for c in &avail_cols {
                                ui.selectable_value(&mut self.shock.reprice_col, c.clone(), c);
                            }
                        });
                        ui.label("A deal reprices on the earlier of this date and its maturity.");
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Beta file:");
                        let shown = if self.shock.beta_file.is_empty() { "(none: every product 100%)".to_string() } else { self.shock.beta_file.clone() };
                        egui::ComboBox::from_id_salt("shk_beta").width(theme::GAP_COMBO_W).selected_text(shown).show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.shock.beta_file, String::new(), "(none: every product 100%)");
                            for f in self.shock.beta_files.clone() {
                                ui.selectable_value(&mut self.shock.beta_file, f.clone(), f);
                            }
                        });
                        ui.add_enabled_ui(!self.shock.beta_file.is_empty(), |ui| {
                            ui.label("Product Col:");
                            let shown = if self.shock.product_col.is_empty() { "(choose)".to_string() } else { self.shock.product_col.clone() };
                            egui::ComboBox::from_id_salt("shk_prod").width(theme::GAP_COMBO_W).selected_text(shown).show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.shock.product_col, c.clone(), c);
                                }
                            });
                        });
                        ui.label("Files are in portable_data/betas; a product that is not in the file passes 100%.");
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut self.shock.use_floor, "Floor the shocked rate at");
                        ui.add_enabled(self.shock.use_floor, egui::DragValue::new(&mut self.shock.floor_pct).speed(0.05).suffix("%")); // not a size: drag speed
                        ui.label("A fall in the rate stops at the floor; the change is cut to fit.");
                    });
                });
                d.footer(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        if theme::primary_button(ui, "\u{E182} Run Rate Shock Simulation").clicked() {
                            step_to_apply = self.tool_step(&ActiveToolModal::RateShockSim).map(|s| (s, true));
                            close_modal = true;
                        }
                        if self.editing.is_none() && ui.button("Repricing gap summary").on_hover_text("Opens the buckets 0\u{2013}1m to >12m with assets, liabilities, gap, cumulative gap and the total \u{394}NII in a new tab").clicked() {
                            open_gap_summary = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_modal = true;
                        }
                    });
                });
            });
        if open_gap_summary {
            if self.open_repricing_gap_tab() {
                close_modal = true;
            }
        }
        ModalOutcome { close: close_modal, step: step_to_apply }
    }

    /// The step the dialog would make, as the settings of the repricing-gap tab.
    pub(crate) fn rate_shock_spec(&self) -> Option<micro_parquet::banking::RateShockSpec> {
        match self.tool_step(&ActiveToolModal::RateShockSim)? {
            TransformStep::RateShockSim { bal_col, rate_col, mat_col, cutoff_date, shock_bps, side_col, reprice_col, beta_file, product_col, floor_pct } => {
                Some(micro_parquet::banking::RateShockSpec { bal_col, rate_col, mat_col, cutoff_date, shock_bps, side_col, reprice_col, beta_file, product_col, floor_pct })
            }
            _ => None,
        }
    }

    /// Opens the repricing-gap summary of the active tab (its steps that run) for these settings.
    pub(crate) fn open_repricing_gap_tab(&mut self) -> bool {
        let Some(spec) = self.rate_shock_spec() else {
            return false;
        };
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return false;
        };
        let exec = tab.exec_steps.min(tab.steps.len());
        let title = format!("Repricing gap ({})", tab.title);
        let source = TabSource::RepricingGap { source: Box::new(tab.source.clone()), steps: tab.steps[..exec].to_vec(), spec };
        self.create_tab_from_source_with(title, source, Vec::new(), HashMap::new(), None);
        true
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
