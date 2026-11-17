//! Screen module `buckets` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct BucketState {
    pub mat_col: String,
    pub cutoff: String,
    pub presets: Vec<CustomBucketDef>,
    pub selected_preset_idx: usize,
    pub null_sight: bool,
    pub new_col: String,
    pub show_custom_creator: bool,
    pub draft: CustomBucketDef,
}

impl Default for BucketState {
    fn default() -> Self {
        Self {
            mat_col: "Matdat".to_string(),
            cutoff: "2026-10-31".to_string(),
            presets: Vec::new(),
            selected_preset_idx: 0,
            null_sight: true,
            new_col: "Maturity_Bucket".to_string(),
            show_custom_creator: false,
            draft: CustomBucketDef {
                        name: "c-m-b-1".to_string(),
                        rules: vec![
                            BucketRuleItem { max_days: 7, label: "01. 1-7 Days".to_string() },
                            BucketRuleItem { max_days: 30, label: "02. 8-30 Days".to_string() },
                            BucketRuleItem { max_days: 90, label: "03. 31-90 Days".to_string() },
                            BucketRuleItem { max_days: 365, label: "04. 91-365 Days".to_string() },
                        ],
                        overflow_label: "05. Over 1 Year".to_string(),
                        overdue_label: "06. Overdue / Matured".to_string(),
                        null_sight_label: "01. Sight / Open".to_string(),
                        null_long_label: "05. Non-Maturing Core".to_string(),
                    },
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_buckets_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        theme::modal("\u{E108} Maturity Bucketer & Custom Preset Creator")
            .collapsible(false)
            .default_width(680.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Maturity Date Column:");
                    egui::ComboBox::from_id_salt("alm_mat")
                        .selected_text(&self.buckets.mat_col)
                        .show_ui(ui, |ui| {
                            for c in &avail_cols {
                                ui.selectable_value(&mut self.buckets.mat_col, c.clone(), c);
                            }
                        });
                    ui.label("Cutoff Date (YYYY-MM-DD):");
                    ui.text_edit_singleline(&mut self.buckets.cutoff);
                });

                let selected_name = self
                    .buckets.presets
                    .get(self.buckets.selected_preset_idx)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "Select Preset".to_string());

                ui.horizontal(|ui| {
                    ui.label("Bucketing Preset:");
                    egui::ComboBox::from_id_salt("alm_preset_combo")
                        .selected_text(&selected_name)
                        .show_ui(ui, |ui| {
                            for (idx, preset) in self.buckets.presets.iter().enumerate() {
                                ui.selectable_value(
                                    &mut self.buckets.selected_preset_idx,
                                    idx,
                                    &preset.name,
                                );
                            }
                        });

                    if ui
                        .button(if self.buckets.show_custom_creator {
                            "\u{E13C} Hide Custom Creator"
                        } else {
                            "\u{E3D4} Create Custom Preset (e.g. c-m-b-1)"
                        })
                        .clicked()
                    {
                        self.buckets.show_custom_creator = !self.buckets.show_custom_creator;
                    }
                });

                // Custom Maturity Bucket Preset Creator Drawer
                if self.buckets.show_custom_creator {
                    ui.separator();
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.strong("\u{E5D4} Custom Maturity Bucket Preset Creator");
                        ui.horizontal(|ui| {
                            ui.label("Custom Preset Name:");
                            ui.text_edit_singleline(&mut self.buckets.draft.name);
                        });

                        let mut remove_rule = None;
                        for (r_idx, rule) in
                            self.buckets.draft.rules.iter_mut().enumerate()
                        {
                            ui.horizontal(|ui| {
                                ui.label("Up to Days:");
                                ui.add(
                                    egui::DragValue::new(&mut rule.max_days)
                                        .range(1..=36500),
                                );
                                ui.label("Bucket Label:");
                                ui.text_edit_singleline(&mut rule.label);
                                if ui.small_button("\u{E4F6}").clicked() {
                                    remove_rule = Some(r_idx);
                                }
                            });
                        }
                        if let Some(ri) = remove_rule {
                            if self.buckets.draft.rules.len() > 1 {
                                self.buckets.draft.rules.remove(ri);
                            }
                        }
                        if ui.small_button("\u{E3D4} Add Day Threshold Row").clicked() {
                            let next_days = self
                                .buckets.draft
                                .rules
                                .last()
                                .map(|r| r.max_days + 30)
                                .unwrap_or(30);
                            self.buckets.draft.rules.push(BucketRuleItem {
                                max_days: next_days,
                                label: format!("Up to {} Days", next_days),
                            });
                        }

                        ui.horizontal(|ui| {
                            ui.label("Above Max Days Label:");
                            ui.text_edit_singleline(
                                &mut self.buckets.draft.overflow_label,
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("Overdue (<0 Days) Label:");
                            ui.text_edit_singleline(
                                &mut self.buckets.draft.overdue_label,
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("Null Matdat (Sight) Label:");
                            ui.text_edit_singleline(
                                &mut self.buckets.draft.null_sight_label,
                            );
                        });

                        if ui.button("\u{E248} Save Custom Preset to Dropdown").clicked()
                            && !self.buckets.draft.name.trim().is_empty()
                        {
                            if let Ok(_) = CustomBucketDef::save_custom_preset(
                                &self.paths.buckets_file,
                                self.buckets.draft.clone(),
                            ) {
                                self.buckets.presets = CustomBucketDef::load_all_presets(
                                    &self.paths.buckets_file,
                                );
                                self.buckets.selected_preset_idx =
                                    self.buckets.presets.len().saturating_sub(1);
                                self.buckets.show_custom_creator = false;
                                self.status_message = format!(
                                    "Saved custom bucket preset '{}'.",
                                    self.buckets.draft.name
                                );
                            }
                        }
                    });
                    ui.separator();
                }

                ui.checkbox(
                    &mut self.buckets.null_sight,
                    "Treat Null Maturity Dates (e.g. Current/Savings) as Sight/Overnight",
                );
                ui.horizontal(|ui| {
                    ui.label("Output Bucket Column Name:");
                    ui.text_edit_singleline(&mut self.buckets.new_col);
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "\u{E182} Generate Maturity Buckets").clicked() {
                        step_to_apply = self.tool_step(&ActiveToolModal::MaturityBucket).map(|s| (s, false));
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
