//! Screen module `join` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct JoinState {
    pub left_tab: usize,
    pub right_tab: usize,
    pub kind: SqlJoinKind,
    pub keys_input: String,
    pub keep_first: bool,
}

impl Default for JoinState {
    fn default() -> Self {
        Self {
            left_tab: 0,
            right_tab: 0,
            kind: SqlJoinKind::Left,
            keys_input: "Actnum".to_string(),
            keep_first: false,
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_join_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let step_to_apply: Option<(TransformStep, bool)> = None;
        theme::modal("\u{E2E2} SQL-Style Multi-Tab Joiner")
            .collapsible(false)
            .show(ctx, |dlg| {
                dlg.body(|ui| {
                    if self.tabs.len() < 2 {
                        ui.label("Please open at least 2 tabs to perform a join.");
                    } else {
                        let left_title = self
                            .tabs
                            .get(self.join.left_tab)
                            .map(|t| t.title.clone())
                            .unwrap_or_default();
                        let right_title = self
                            .tabs
                            .get(self.join.right_tab)
                            .map(|t| t.title.clone())
                            .unwrap_or_default();
                        let tab_titles: Vec<(usize, String)> = self
                            .tabs
                            .iter()
                            .enumerate()
                            .map(|(i, t)| (i, t.title.clone()))
                            .collect();

                        ui.horizontal(|ui| {
                            ui.label("Left Tab:");
                            egui::ComboBox::from_id_salt("jn_l")
                                .selected_text(&left_title)
                                .show_ui(ui, |ui| {
                                    for (i, title) in &tab_titles {
                                        ui.selectable_value(&mut self.join.left_tab, *i, title);
                                    }
                                });
                            ui.label("Right Tab:");
                            egui::ComboBox::from_id_salt("jn_r")
                                .selected_text(&right_title)
                                .show_ui(ui, |ui| {
                                    for (i, title) in &tab_titles {
                                        ui.selectable_value(
                                            &mut self.join.right_tab,
                                            *i,
                                            title,
                                        );
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label("Join Type:");
                            egui::ComboBox::from_id_salt("jn_kind")
                                .selected_text(self.join.kind.label())
                                .show_ui(ui, |ui| {
                                    for k in [
                                        SqlJoinKind::Left,
                                        SqlJoinKind::Inner,
                                        SqlJoinKind::Full,
                                        SqlJoinKind::Anti,
                                        SqlJoinKind::Semi,
                                    ] {
                                        ui.selectable_value(&mut self.join.kind, k, k.label());
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label("Join Key Column(s) (e.g. Actnum):");
                            ui.text_edit_singleline(&mut self.join.keys_input);
                        });
                        ui.checkbox(&mut self.join.keep_first, "Keep first row per key")
                            .on_hover_text("Off: a key repeated in the right table is an error. On: the first right row per key is used and the status bar shows how many were dropped.");
                    }
                });
                dlg.footer(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        if self.tabs.len() >= 2 {
                            if theme::primary_button(ui, "\u{E3FE} Execute Join into New Tab").clicked() {
                                let (Some(l_tab), Some(r_tab)) = (
                                    self.tabs.get(self.join.left_tab).cloned(),
                                    self.tabs.get(self.join.right_tab).cloned(),
                                ) else {
                                    self.status_message =
                                        "Join: the selected left or right tab no longer exists. Pick both tabs again.".to_string();
                                    return;
                                };
                                let keys: Vec<String> = self
                                    .join.keys_input
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect();
                                self.create_tab_from_source(
                                    format!("Join({}+{})", l_tab.title, r_tab.title),
                                    TabSource::JoinedTabs {
                                        left_source: Box::new(l_tab.source),
                                        left_steps: l_tab.steps,
                                        right_source: Box::new(r_tab.source),
                                        right_steps: r_tab.steps,
                                        join_kind: self.join.kind,
                                        join_keys: keys,
                                        right_cols_to_keep: Vec::new(),
                                        keep_first_duplicate_right: self.join.keep_first,
                                    },
                                    Vec::new(),
                                    l_tab.col_formats,
                                );
                                close_modal = true;
                            }
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
