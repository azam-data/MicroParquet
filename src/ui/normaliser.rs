//! Screen module `normaliser` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct NormState {
    pub source_col: String,
    pub new_col: String,
}

impl Default for NormState {
    fn default() -> Self {
        Self {
            source_col: "Acttit".to_string(),
            new_col: "Normalized_Entity".to_string(),
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_normaliser_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        theme::modal("\u{E102} Counterparty Entity Name Normalizer")
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(
                    "Standardizes LIMITED -> LTD, strips punctuation, and unifies corporate names.",
                );
                ui.horizontal(|ui| {
                    ui.label("Source Name Column (e.g. Acttit):");
                    egui::ComboBox::from_id_salt("norm_src")
                        .selected_text(&self.normaliser.source_col)
                        .show_ui(ui, |ui| {
                            for c in &avail_cols {
                                ui.selectable_value(
                                    &mut self.normaliser.source_col,
                                    c.clone(),
                                    c,
                                );
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("New Normalized Column:");
                    ui.text_edit_singleline(&mut self.normaliser.new_col);
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "\u{E182} Normalize Counterparty Names").clicked() {
                        step_to_apply = self.tool_step(&ActiveToolModal::NormalizeEntity).map(|s| (s, false));
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
