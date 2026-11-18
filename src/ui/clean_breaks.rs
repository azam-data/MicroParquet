//! "Clean line breaks": replaces the line breaks inside quoted text values with spaces. Opened from
//! the Transform menu, Ctrl+K, and the "Replace with spaces" button of the row-count warning.

use crate::ui::prelude::*;

/// The settings of the dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct CleanBreaksState {
    /// All text columns (the default) or only `picked`.
    pub all: bool,
    pub picked: Vec<String>,
    /// "Trim spaces on all text values" (off from the Transform menu).
    pub trim_all: bool,
}

impl Default for CleanBreaksState {
    fn default() -> Self {
        Self { all: true, picked: Vec::new(), trim_all: false }
    }
}

impl MicroParquetApp {
    /// The step the dialog would make; `None` when "Only these columns" has none ticked.
    pub(crate) fn clean_breaks_step(&self) -> Option<TransformStep> {
        let s = &self.clean_breaks;
        if s.all {
            Some(TransformStep::CleanLineBreaks { columns: Vec::new(), trim_all: s.trim_all })
        } else if s.picked.is_empty() {
            None
        } else {
            Some(TransformStep::CleanLineBreaks { columns: s.picked.clone(), trim_all: s.trim_all })
        }
    }

    /// True when the active tab already cleans line breaks (a step that is on).
    pub(crate) fn tab_cleans_line_breaks(tab: &TabState) -> bool {
        tab.steps.iter().any(|s| matches!(s, TransformStep::CleanLineBreaks { .. }))
    }

    /// The "Replace with spaces" button of the row-count warning: adds the step to the active tab,
    /// for all text columns. Nothing is cleaned until it is pressed.
    pub(crate) fn replace_line_breaks_with_spaces(&mut self) {
        self.push_step_to_active_tab(TransformStep::CleanLineBreaks { columns: Vec::new(), trim_all: false }, false);
    }

    pub(crate) fn render_clean_breaks_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let mut step: Option<(TransformStep, bool)> = None;
        let tk = *theme::tokens(self.dark_mode);
        // the text columns: by the type of the columns the tab shows (a column that is not shown is listed too)
        let text_cols: Vec<String> = {
            let slice = self.tabs.get(self.active_tab).map(|t| &t.cached_slice);
            avail_cols
                .iter()
                .filter(|c| match slice.and_then(|df| df.column(c.as_str()).ok()) {
                    Some(s) => matches!(s.dtype(), polars::prelude::DataType::String),
                    None => true,
                })
                .cloned()
                .collect()
        };
        let ready = self.clean_breaks_step().is_some();
        theme::modal(&format!("{}  Clean line breaks", theme::ph::BROOM)).default_width(theme::DIALOG_W_TOOL).show(ctx, |d| {
            d.body(|ui| {
                ui.add(egui::Label::new("Replaces every run of line breaks (\\r\\n, \\n or \\r) inside a text value with one space, and trims the ends of the values that had one. Values without a line break are not touched, unless you tick the box below. Nothing is cleaned until you click the button; the step is recorded, so a macro repeats it.").wrap());
                ui.add_space(theme::SPACE_S);
                ui.radio_value(&mut self.clean_breaks.all, true, "All text columns");
                ui.radio_value(&mut self.clean_breaks.all, false, "Only these columns:");
                ui.add_enabled_ui(!self.clean_breaks.all, |ui| {
                    egui::ScrollArea::vertical().id_salt("clean_cols").max_height(theme::LIST_MAX_H_L).show(ui, |ui| {
                        if text_cols.is_empty() {
                            ui.label(egui::RichText::new("This table has no text column.").color(tk.muted));
                        }
                        for c in &text_cols {
                            let mut on = self.clean_breaks.picked.contains(c);
                            if ui.checkbox(&mut on, c.as_str()).changed() {
                                if on {
                                    self.clean_breaks.picked.push(c.clone());
                                } else {
                                    self.clean_breaks.picked.retain(|x| x != c);
                                }
                            }
                        }
                    });
                });
                ui.add_space(theme::SPACE_S);
                ui.checkbox(&mut self.clean_breaks.trim_all, "Trim spaces on all text values")
                    .on_hover_text("Also trims the spaces at both ends of values that have no line break. The Schema & Ingestion box does this; it is off here unless you tick it.");
                if !ready {
                    ui.add_space(theme::SPACE_S);
                    theme::error_label(ui, "Tick at least one column, or choose All text columns.");
                }
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let go = ui.add_enabled_ui(ready, |ui| theme::primary_button(ui, "Clean line breaks")).inner;
                    if go.clicked() {
                        step = self.clean_breaks_step().map(|s| (s, false));
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        });
        ModalOutcome { close, step }
    }
}
