//! Screen module `filter` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct FilterState {
    pub col: String,
    pub op: FilterOp,
    pub val: String,
    pub keep: bool,
}

impl Default for FilterState {
    fn default() -> Self {
        Self {
            col: String::new(),
            op: FilterOp::Eq,
            val: String::new(),
            keep: true,
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_filter_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        theme::modal("\u{E30C} Filter Rows / Delete Filtered Rows")
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Column:");
                    egui::ComboBox::from_id_salt("flt_col")
                        .selected_text(&self.filter.col)
                        .show_ui(ui, |ui| {
                            for c in &avail_cols {
                                ui.selectable_value(&mut self.filter.col, c.clone(), c);
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Condition:");
                    egui::ComboBox::from_id_salt("flt_op")
                        .selected_text(self.filter.op.label())
                        .show_ui(ui, |ui| {
                            for op in FilterOp::all() {
                                ui.selectable_value(&mut self.filter.op, *op, op.label());
                            }
                        });
                });
                if self.filter.op != FilterOp::IsNull
                    && self.filter.op != FilterOp::IsNotNull
                {
                    ui.horizontal(|ui| {
                        ui.label("Value:");
                        ui.text_edit_singleline(&mut self.filter.val);
                    });
                }
                ui.horizontal(|ui| {
                    ui.radio_value(
                        &mut self.filter.keep,
                        true,
                        "Keep Matching Rows (Standard Filter)",
                    );
                    ui.radio_value(
                        &mut self.filter.keep,
                        false,
                        "Delete Matching Rows (Invert Filter)",
                    );
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "\u{E182} Apply to Current Tab").clicked() {
                        step_to_apply = Some((
                            TransformStep::FilterRows {
                                column: self.filter.col.clone(),
                                operator: self.filter.op,
                                value: self.filter.val.clone(),
                                keep_matching: self.filter.keep,
                            },
                            false,
                        ));
                        close_modal = true;
                    }
                    if ui.button("\u{E3D4} Open Filtered View in New Tab").clicked() {
                        step_to_apply = Some((
                            TransformStep::FilterRows {
                                column: self.filter.col.clone(),
                                operator: self.filter.op,
                                value: self.filter.val.clone(),
                                keep_matching: self.filter.keep,
                            },
                            true,
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
