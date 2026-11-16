//! Screen module `sql` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

impl MicroParquetApp {
    pub(crate) fn render_sql_bar(&mut self, ui: &mut egui::Ui) {
        if self.show_sql_bar {
            ui.separator();
            let mut sql_to_run: Option<(String, bool)> = None;
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                ui.horizontal(|ui| {
                    ui.strong("SQL (`self`):");
                    ui.add(
                        egui::TextEdit::singleline(&mut tab.sql_scratchpad)
                            .desired_width(520.0),
                    );
                    if ui.button("\u{E3D0} Apply Here").clicked() {
                        sql_to_run = Some((tab.sql_scratchpad.clone(), false));
                    }
                    if ui.button("\u{E3D4} New Tab").clicked() {
                        sql_to_run = Some((tab.sql_scratchpad.clone(), true));
                    }
                });
            }
            if let Some((q, new_tab)) = sql_to_run {
                self.push_step_to_active_tab(TransformStep::SqlQuery { query: q }, new_tab);
            }
        }
    }
}
