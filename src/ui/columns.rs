//! Screen module `columns` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct ColumnsState {
    pub list: Vec<(String, bool, ColumnFormatSpec)>,
    pub rename_old: String,
    pub rename_new: String,
}

impl Default for ColumnsState {
    fn default() -> Self {
        Self {
            list: Vec::new(),
            rename_old: String::new(),
            rename_new: String::new(),
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_columns_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        theme::modal("\u{E546} Columns: Drop, Order, Rename & Number Format")
            .collapsible(false)
            .default_width(680.0)
            .show(ctx, |ui| {
                ui.strong("1. Keep/Drop, Reorder & Set Per-Column Display Format:");
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        let len = self.columns.list.len();
                        let mut swap_pair: Option<(usize, usize)> = None;
                        for i in 0..len {
                            ui.horizontal(|ui| {
                                if ui.small_button("\u{E08E}").clicked() && i > 0 {
                                    swap_pair = Some((i, i - 1));
                                }
                                if ui.small_button("\u{E03E}").clicked() && i + 1 < len {
                                    swap_pair = Some((i, i + 1));
                                }
                                let (ref col_name, ref mut is_checked, ref mut spec) =
                                    self.columns.list[i];
                                ui.checkbox(is_checked, col_name.as_str());

                                egui::ComboBox::from_id_salt(format!("col_fmt_{}", i))
                                    .selected_text(spec.fmt_type.label())
                                    .show_ui(ui, |ui| {
                                        for ft in MetricFormatType::all() {
                                            ui.selectable_value(
                                                &mut spec.fmt_type,
                                                *ft,
                                                ft.label(),
                                            );
                                        }
                                    });
                                ui.label("Dec:");
                                ui.add(
                                    egui::DragValue::new(&mut spec.decimals).range(0..=6),
                                );
                            });
                        }
                        if let Some((a, b)) = swap_pair {
                            self.columns.list.swap(a, b);
                        }
                    });

                if theme::primary_button(ui, "\u{E182} Apply Column Selection, Order & Formats").clicked() {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        for (cname, _, spec) in &self.columns.list {
                            tab.col_formats.insert(cname.clone(), *spec);
                        }
                    }
                    let kept: Vec<String> = self
                        .columns.list
                        .iter()
                        .filter(|(_, k, _)| *k)
                        .map(|(n, _, _)| n.clone())
                        .collect();
                    step_to_apply = Some((
                        TransformStep::SelectAndReorderColumns { cols: kept },
                        false,
                    ));
                    close_modal = true;
                }

                ui.separator();
                ui.strong("2. Rename a Column:");
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("ren_old")
                        .selected_text(&self.columns.rename_old)
                        .show_ui(ui, |ui| {
                            for c in &avail_cols {
                                ui.selectable_value(&mut self.columns.rename_old, c.clone(), c);
                            }
                        });
                    ui.label("->");
                    ui.text_edit_singleline(&mut self.columns.rename_new);
                    if ui.button("Rename").clicked() && !self.columns.rename_new.trim().is_empty() {
                        step_to_apply = Some((
                            TransformStep::RenameColumn {
                                old_name: self.columns.rename_old.clone(),
                                new_name: self.columns.rename_new.trim().to_string(),
                            },
                            false,
                        ));
                        close_modal = true;
                    }
                });
                if ui.button("Close").clicked() {
                    close_modal = true;
                }
            });
        ModalOutcome { close: close_modal, step: step_to_apply }
    }
}
