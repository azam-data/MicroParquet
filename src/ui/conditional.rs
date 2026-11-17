//! Screen module `conditional` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct CondState {
    pub new_col: String,
    pub rules: Vec<CondRule>,
    pub fallback: String,
    pub as_float: bool,
    /// What a blank on either side of a rule gives (default: a blank cell).
    pub blank_choice: BlankChoice,
    /// The text for `BlankChoice::Value`.
    pub blank_text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlankChoice {
    /// The result is a blank cell.
    Blank,
    /// The result is the text (or number) typed next to it.
    Value,
    /// The rule does not match: the next rule decides (what steps saved before this choice do).
    Skip,
}

impl CondState {
    /// The choice as the step stores it.
    pub fn blank_rule(&self) -> BlankRule {
        match self.blank_choice {
            BlankChoice::Blank => BlankRule::Result(String::new()),
            BlankChoice::Value => BlankRule::Result(self.blank_text.clone()),
            BlankChoice::Skip => BlankRule::FallThrough,
        }
    }

    /// Loads a step's choice into the dialog.
    pub fn set_blank_rule(&mut self, rule: &BlankRule) {
        match rule {
            BlankRule::FallThrough => self.blank_choice = BlankChoice::Skip,
            BlankRule::Result(t) if t.trim().is_empty() => self.blank_choice = BlankChoice::Blank,
            BlankRule::Result(t) => {
                self.blank_choice = BlankChoice::Value;
                self.blank_text = t.clone();
            }
        }
    }
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
                        compare_is_col: false,
                    }],
            fallback: "Active/Other".to_string(),
            as_float: false,
            blank_choice: BlankChoice::Blank,
            blank_text: String::new(),
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
            .default_width(theme::COND_BODY_W + 2.0 * theme::SPACE_M + theme::DIALOG_SCROLL_BAR_ALLOWANCE)
            .show(ctx, |d| {
                d.body(|ui| {
                    // the rows wrap inside this width instead of running out of the dialog
                    ui.set_max_width(theme::COND_BODY_W);
                    ui.horizontal_wrapped(|ui| {
                        ui.label("New Column Name:");
                        ui.text_edit_singleline(&mut self.cond.new_col);
                        ui.checkbox(&mut self.cond.as_float, "Output as Numeric (Float64)");
                    });
                    ui.separator();

                    let mut remove_idx = None;
                    for (idx, rule) in self.cond.rules.iter_mut().enumerate() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(if idx == 0 { "IF" } else { "ELSE IF" });
                            theme::sized_combo(ui, theme::COND_COMBO_W, |ui| {
                                egui::ComboBox::from_id_salt(format!("cc_col_{}", idx))
                                    .width(theme::COND_COMBO_W)
                                    .selected_text(&rule.col_name)
                                    .show_ui(ui, |ui| {
                                        for c in &avail_cols {
                                            ui.selectable_value(&mut rule.col_name, c.clone(), c);
                                        }
                                    });
                            });
                            theme::sized_combo(ui, theme::COND_OP_W, |ui| {
                                egui::ComboBox::from_id_salt(format!("cc_op_{}", idx))
                                    .width(theme::COND_OP_W)
                                    .selected_text(rule.op.label())
                                    .show_ui(ui, |ui| {
                                        for op in FilterOp::all() {
                                            ui.selectable_value(&mut rule.op, *op, op.label());
                                        }
                                    });
                            });
                            if !matches!(rule.op, FilterOp::IsNull | FilterOp::IsNotNull) {
                                // the right-hand side: a value, or another column
                                theme::sized_combo(ui, theme::COND_KIND_W, |ui| {
                                    egui::ComboBox::from_id_salt(format!("cc_rhs_{}", idx))
                                        .width(theme::COND_KIND_W)
                                        .selected_text(if rule.compare_is_col { "column" } else { "value" })
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut rule.compare_is_col, false, "value");
                                            ui.selectable_value(&mut rule.compare_is_col, true, "column");
                                        });
                                });
                                if rule.compare_is_col {
                                    theme::sized_combo(ui, theme::COND_COMBO_W, |ui| {
                                        egui::ComboBox::from_id_salt(format!("cc_cmp_{}", idx))
                                            .width(theme::COND_COMBO_W)
                                            .selected_text(if rule.compare_val.is_empty() { "choose a column" } else { rule.compare_val.as_str() })
                                            .show_ui(ui, |ui| {
                                                for c in &avail_cols {
                                                    ui.selectable_value(&mut rule.compare_val, c.clone(), c);
                                                }
                                            });
                                    });
                                } else {
                                    ui.add(egui::TextEdit::singleline(&mut rule.compare_val).desired_width(theme::FIELD_W_S));
                                }
                            }
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
                            compare_is_col: false,
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label("ELSE (Fallback Value):");
                        ui.text_edit_singleline(&mut self.cond.fallback);
                    });
                    // a blank on either side of a rule (a column that is empty) is not "no":
                    // it gets its own result
                    ui.horizontal_wrapped(|ui| {
                        ui.label("If either side of a rule is blank, the result is:");
                        ui.selectable_value(&mut self.cond.blank_choice, BlankChoice::Blank, "blank");
                        ui.selectable_value(&mut self.cond.blank_choice, BlankChoice::Value, "this value");
                        if self.cond.blank_choice == BlankChoice::Value {
                            ui.add(egui::TextEdit::singleline(&mut self.cond.blank_text).desired_width(theme::FIELD_W_S));
                        }
                        ui.selectable_value(&mut self.cond.blank_choice, BlankChoice::Skip, "no match (try the next rule)")
                            .on_hover_text("What steps saved before this choice existed do: a blank never matches, so the next rule or ELSE decides.");
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
