//! Small controls shared by the Phase 5 risk and regulatory dialogs, and the rectangles they
//! report to the render tests.

use crate::ui::prelude::*;

/// What a dialog drew, for the render tests: its controls by name, its notes, its buttons and
/// the banner under the header (if any).
#[derive(Clone, Debug)]
pub struct DialogRects {
    pub controls: Vec<(String, egui::Rect)>,
    pub notes: egui::Rect,
    pub buttons: Vec<(String, egui::Rect)>,
    pub banner: egui::Rect,
}

impl Default for DialogRects {
    fn default() -> Self {
        Self { controls: Vec::new(), notes: egui::Rect::NOTHING, buttons: Vec::new(), banner: egui::Rect::NOTHING }
    }
}

impl DialogRects {
    pub fn control(&self, name: &str) -> Option<egui::Rect> {
        self.controls.iter().find(|(n, _)| n == name).map(|(_, r)| *r)
    }
    pub fn button(&self, name: &str) -> Option<egui::Rect> {
        self.buttons.iter().find(|(n, _)| n == name).map(|(_, r)| *r)
    }
}

/// A muted label of a control.
pub fn field_label(ui: &mut egui::Ui, tk: &theme::Tokens, text: &str) {
    ui.label(egui::RichText::new(text).color(tk.muted).size(theme::TEXT_LABEL));
}

/// A combo box over the columns of a table. `optional` adds "(none)" (the empty text). Returns its rect.
pub fn col_combo(ui: &mut egui::Ui, id: &str, value: &mut String, cols: &[String], optional: bool) -> egui::Rect {
    let shown = if value.trim().is_empty() {
        if optional { "(none)".to_string() } else { "Choose\u{2026}".to_string() }
    } else {
        value.clone()
    };
    let r = egui::ComboBox::from_id_salt(id).width(theme::CF_COMBO_W).selected_text(shown).show_ui(ui, |ui| {
        if optional {
            ui.selectable_value(value, String::new(), "(none)");
        }
        for c in cols {
            ui.selectable_value(value, c.clone(), c.as_str());
        }
    });
    r.response.rect
}

/// A combo box over fixed texts (the chosen one is the value).
pub fn text_combo(ui: &mut egui::Ui, id: &str, value: &mut String, items: &[String], width: f32) -> egui::Rect {
    let shown = if value.trim().is_empty() { "Choose\u{2026}".to_string() } else { value.clone() };
    let r = egui::ComboBox::from_id_salt(id).width(width).selected_text(shown).show_ui(ui, |ui| {
        for c in items {
            ui.selectable_value(value, c.clone(), c.as_str());
        }
    });
    r.response.rect
}

/// The first column whose lower-case name contains one of the words, or the empty text.
pub fn guess_column(columns: &[String], words: &[&str]) -> String {
    columns.iter().find(|c| words.iter().any(|w| c.to_lowercase().contains(w))).cloned().unwrap_or_default()
}
