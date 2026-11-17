//! Screen module `calc_column` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct CalcState {
    pub new_col: String,
    pub left_col: String,
    pub op: MathOp,
    pub right_is_col: bool,
    pub right_val: String,
}

impl Default for CalcState {
    fn default() -> Self {
        Self {
            new_col: "Balance_Crore".to_string(),
            left_col: "Balance".to_string(),
            op: MathOp::Div,
            right_is_col: false,
            right_val: "10000000".to_string(),
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_calc_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        theme::modal("\u{E538} Add Mathematical Calculated Column")
            .collapsible(false)
            .show(ctx, |d| {
                d.body(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("New Column Name:");
                        ui.text_edit_singleline(&mut self.calc.new_col);
                    });
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("math_l")
                            .selected_text(&self.calc.left_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.calc.left_col, c.clone(), c);
                                }
                            });
                        for op in [MathOp::Add, MathOp::Sub, MathOp::Mul, MathOp::Div] {
                            ui.selectable_value(&mut self.calc.op, op, op.symbol());
                        }
                        ui.checkbox(&mut self.calc.right_is_col, "Use Another Column");
                    });
                    ui.horizontal(|ui| {
                        if self.calc.right_is_col {
                            egui::ComboBox::from_id_salt("math_r_col")
                                .selected_text(&self.calc.right_val)
                                .show_ui(ui, |ui| {
                                    for c in &avail_cols {
                                        ui.selectable_value(
                                            &mut self.calc.right_val,
                                            c.clone(),
                                            c,
                                        );
                                    }
                                });
                        } else {
                            ui.label("Scalar Value (e.g. 10000000):");
                            ui.text_edit_singleline(&mut self.calc.right_val);
                        }
                    });
                });
                d.footer(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        if theme::primary_button(ui, "\u{E182} Add Calculated Column").clicked() {
                            step_to_apply = self.tool_step(&ActiveToolModal::MathFormula).map(|s| (s, false));
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
