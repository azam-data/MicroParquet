//! The zoom of the whole window (Phase 6 U6): View ▸ Zoom, Ctrl+= / Ctrl+- / Ctrl+0, the label at
//! the right end of the status bar, and the default zoom kept in `settings.json`.

use crate::ui::prelude::*;

/// The zoom as the menu and the status bar show it: "105%".
pub fn percent_text(z: f32) -> String {
    format!("{:.0}%", z * 100.0) // not a size: percent
}

impl MicroParquetApp {
    /// The zoom the app starts at and goes back to: the saved default (held to 50%–200%), else 105%.
    pub(crate) fn default_zoom(&self) -> f32 {
        self.settings.start_zoom(theme::UI_ZOOM_DEFAULT).0
    }

    pub(crate) fn set_zoom(&mut self, ctx: &egui::Context, z: f32) {
        self.ui_zoom = z.clamp(theme::UI_ZOOM_MIN, theme::UI_ZOOM_MAX);
        theme::apply_ui_theme(ctx, self.dark_mode, self.ui_zoom);
    }

    pub(crate) fn zoom_in(&mut self, ctx: &egui::Context) {
        let z = theme::zoom_in(self.ui_zoom);
        self.set_zoom(ctx, z);
    }

    pub(crate) fn zoom_out(&mut self, ctx: &egui::Context) {
        let z = theme::zoom_out(self.ui_zoom);
        self.set_zoom(ctx, z);
    }

    /// Back to the default zoom (Ctrl+0).
    pub(crate) fn zoom_reset(&mut self, ctx: &egui::Context) {
        let z = self.default_zoom();
        self.set_zoom(ctx, z);
        self.status_message = format!("Zoom {} (the default).", percent_text(z));
    }

    /// Keeps `z` (50%–200%, steps of 5%) as the default zoom in `settings.json`.
    pub(crate) fn zoom_set_default(&mut self, z: f32) {
        let z = theme::snap_zoom(z);
        self.settings.default_zoom = Some((z * 100.0).round() / 100.0); // not a size: percent to a fraction
        self.save_settings();
        self.status_message = format!("The default zoom is {}: the app starts at it, and Ctrl+0 goes back to it.", percent_text(z));
    }

    /// Ctrl+= zooms in, Ctrl+- zooms out, Ctrl+0 resets to the default.
    pub(crate) fn handle_zoom_keys(&mut self, ctx: &egui::Context) {
        let m = egui::Modifiers::COMMAND;
        let (plus, minus, zero) = ctx.input_mut(|i| {
            (
                i.consume_key(m, egui::Key::Equals) || i.consume_key(m, egui::Key::Plus),
                i.consume_key(m, egui::Key::Minus),
                i.consume_key(m, egui::Key::Num0),
            )
        });
        if plus {
            self.zoom_in(ctx);
        } else if minus {
            self.zoom_out(ctx);
        } else if zero {
            self.zoom_reset(ctx);
        }
    }

    /// The contents of the Zoom menu (View ▸ Zoom, and a click on the zoom label in the status bar).
    pub(crate) fn zoom_menu_contents(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label(theme::semibold(format!("Zoom: {}", percent_text(self.ui_zoom))));
        if ui.button("Zoom in  (Ctrl+=)").clicked() {
            self.zoom_in(ctx);
        }
        if ui.button("Zoom out  (Ctrl+-)").clicked() {
            self.zoom_out(ctx);
        }
        if ui.button(format!("Reset to default  ({})  (Ctrl+0)", percent_text(self.default_zoom()))).clicked() {
            self.zoom_reset(ctx);
            ui.close_menu();
        }
        if ui.button("Set current zoom as default").clicked() {
            let z = self.ui_zoom;
            self.zoom_set_default(z);
            ui.close_menu();
        }
        ui.menu_button(format!("Default zoom: {}", percent_text(self.default_zoom())), |ui| {
            egui::ScrollArea::vertical().max_height(theme::ZOOM_LIST_H).show(ui, |ui| {
                for z in theme::zoom_choices() {
                    let on = (z - self.default_zoom()).abs() < 0.001; // not a size: two zooms that are the same
                    if ui.selectable_label(on, percent_text(z)).clicked() {
                        self.zoom_set_default(z);
                        ui.close_menu();
                    }
                }
            });
        });
    }
}
