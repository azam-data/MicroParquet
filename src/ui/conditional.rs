//! Screen module `conditional` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct CondState {
    pub new_col: String,
    pub rules: Vec<CondRule>,
    pub fallback: String,
    pub as_float: bool,
}

impl Default for CondState {
    fn default() -> Self {
        Self {
            new_col: "Custom_Flag".to_string(),
            rules: vec![CondRule {
                        col_name: "Acstat".to_string(),
                        op: FilterOp::Eq,
                        compare_val: "DMT".to_string(),
                        output_val: "Dormant".to_string(),
                    }],
            fallback: "Active/Other".to_string(),
            as_float: false,
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_conditional_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        theme::modal("\u{E3D4} Add Conditional Column (IF / ELSE IF / ELSE)")
            .collapsible(false)
            .show(ctx, |d| {
                d.body(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("New Column Name:");
                        ui.text_edit_singleline(&mut self.cond.new_col);
                        ui.checkbox(&mut self.cond.as_float, "Output as Numeric (Float64)");
                    });
                    ui.separator();

                    let mut remove_idx = None;
                    for (idx, rule) in self.cond.rules.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(if idx == 0 { "IF" } else { "ELSE IF" });
                            egui::ComboBox::from_id_salt(format!("cc_col_{}", idx))
                                .selected_text(&rule.col_name)
                                .show_ui(ui, |ui| {
                                    for c in &avail_cols {
                                        ui.selectable_value(&mut rule.col_name, c.clone(), c);
                                    }
                                });
                            egui::ComboBox::from_id_salt(format!("cc_op_{}", idx))
                                .selected_text(rule.op.label())
                                .show_ui(ui, |ui| {
                                    for op in FilterOp::all() {
                                        ui.selectable_value(&mut rule.op, *op, op.label());
                                    }
                                });
                            ui.add(
                                egui::TextEdit::singleline(&mut rule.compare_val)
                                    .desired_width(theme::FIELD_W_S),
                            );
                            ui.label("THEN");
                            ui.add(
                                egui::TextEdit::singleline(&mut rule.output_val)
                                    .desired_width(theme::FIELD_W_S),
                            );
                            if ui.small_button("\u{E4F6}").clicked() {
                                remove_idx = Some(idx);
                            }
                        });
                    }
                    if let Some(i) = remove_idx {
                        if self.cond.rules.len() > 1 {
                            self.cond.rules.remove(i);
                        }
                    }
                    if ui.button("\u{E3D4} Add ELSE IF Branch").clicked() {
                        self.cond.rules.push(CondRule {
                            col_name: avail_cols.first().cloned().unwrap_or_default(),
                            op: FilterOp::Eq,
                            compare_val: String::new(),
                            output_val: String::new(),
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label("ELSE (Fallback Value):");
                        ui.text_edit_singleline(&mut self.cond.fallback);
                    });
                });
                d.footer(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        if theme::primary_button(ui, "\u{E182} Add Column").clicked() {
                            step_to_apply = self.tool_step(&ActiveToolModal::ConditionalCol).map(|s| (s, false));
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
