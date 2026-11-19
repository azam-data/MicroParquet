//! The SQL tab of the right panel (Phase 6 U2): a multi-line editor for a query on the tab
//! (`self` is the table), "Apply here" and "New tab", the last error and this tab's last queries.

use crate::ui::prelude::*;

/// How many queries a tab remembers.
pub const SQL_HISTORY_MAX: usize = 10;

/// What the SQL tab drew, for the render tests.
#[derive(Clone, Copy, Debug)]
pub struct SqlRects {
    pub hint: egui::Rect,
    pub editor: egui::Rect,
    pub apply: egui::Rect,
    pub new_tab: egui::Rect,
    pub history: egui::Rect,
    pub error: egui::Rect,
    /// Lines the editor shows at this height.
    pub editor_rows: usize,
}

impl Default for SqlRects {
    fn default() -> Self {
        let n = egui::Rect::NOTHING;
        Self { hint: n, editor: n, apply: n, new_tab: n, history: n, error: n, editor_rows: 0 }
    }
}

/// Puts `query` at the end of a tab's history (no repeat of the last one; the oldest of more
/// than 10 is dropped).
pub fn remember_query(history: &mut Vec<String>, query: &str) {
    let q = query.trim();
    if q.is_empty() {
        return;
    }
    history.retain(|h| h != q);
    history.push(q.to_string());
    while history.len() > SQL_HISTORY_MAX {
        history.remove(0);
    }
}

impl MicroParquetApp {
    /// The tab's content: hint, editor, buttons, history, and the last error.
    pub(crate) fn render_sql_tab(&mut self, ui: &mut egui::Ui, panel_h: f32) {
        let tk = *theme::tokens(self.dark_mode);
        let mut run: Option<bool> = None;
        let mut rects = SqlRects::default();
        let editor_id = egui::Id::new("sql_editor");
        // Ctrl+Shift+Enter runs in a new tab, Ctrl+Enter here (the editor would take them as a new line)
        let ctx = ui.ctx().clone();
        if ctx.memory(|m| m.has_focus(editor_id)) {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Enter)) {
                run = Some(true);
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)) {
                run = Some(false);
            }
        }
        let margin = theme::MARGIN_PANEL_STEPS;
        let mut pick: Option<String> = None;
        egui::Frame::none().inner_margin(margin).show(ui, |ui| {
            let Some(tab) = self.tabs.get_mut(self.active_tab) else {
                return;
            };
            let row_h = ui.text_style_height(&egui::TextStyle::Monospace);
            let hint = ui.add(egui::Label::new(egui::RichText::new("Table name: self").color(tk.muted).size(theme::TEXT_LABEL)).truncate());
            rects.hint = hint.rect;
            ui.add_space(theme::SPACE_XS);
            // the editor takes the height the other parts leave (never less than 8 lines)
            let below = theme::PANEL_BTN_H + theme::SPACE_S + theme::SQL_HISTORY_H + theme::SPACE_S + theme::SQL_ERROR_H + 3.0 * ui.spacing().item_spacing.y; // not a size: three gaps
            let wanted = (panel_h - margin.sum().y - hint.rect.height() - theme::SPACE_XS - below).max(0.0);
            let min_h = theme::SQL_EDITOR_MIN_ROWS as f32 * row_h + 2.0 * ui.spacing().item_spacing.y + theme::SPACE_S;
            let editor_h = wanted.max(min_h);
            let rows = ((editor_h / row_h).floor() as usize).max(theme::SQL_EDITOR_MIN_ROWS);
            let area = egui::ScrollArea::vertical()
                .id_salt("sql_editor_scroll")
                .max_height(editor_h)
                .min_scrolled_height(editor_h)
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut tab.sql_scratchpad)
                            .id(editor_id)
                            .font(egui::TextStyle::Monospace)
                            .desired_rows(rows)
                            .desired_width(f32::INFINITY)
                            .lock_focus(true)
                            .hint_text("SELECT * FROM self"),
                    );
                });
            rects.editor = area.inner_rect;
            rects.editor_rows = rows;
            ui.add_space(theme::SPACE_S);
            ui.horizontal(|ui| {
                let w = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                let empty = tab.sql_scratchpad.trim().is_empty();
                let a = ui.add_enabled(!empty, egui::Button::new(theme::semibold("Apply here")).min_size(egui::vec2(w, theme::PANEL_BTN_H)));
                let a = a.on_hover_text("Add the query as the next step of this tab (Ctrl+Enter)");
                let b = ui.add_enabled(!empty, egui::Button::new(theme::semibold("New tab")).min_size(egui::vec2(w, theme::PANEL_BTN_H)));
                let b = b.on_hover_text("Open the result in a new tab (Ctrl+Shift+Enter)");
                rects.apply = a.rect;
                rects.new_tab = b.rect;
                if a.clicked() {
                    run = Some(false);
                }
                if b.clicked() {
                    run = Some(true);
                }
            });
            ui.add_space(theme::SPACE_S);
            let hist = tab.sql_history.clone();
            let r = ui
                .add_enabled_ui(!hist.is_empty(), |ui| {
                    egui::ComboBox::from_id_salt("sql_history")
                        .width(ui.available_width() - ui.spacing().item_spacing.x)
                        .height(theme::SQL_HISTORY_POPUP_H)
                        .selected_text(if hist.is_empty() { "History (no queries yet)".to_string() } else { format!("History ({})", hist.len()) })
                        .show_ui(ui, |ui| {
                            for q in hist.iter().rev() {
                                let one_line: String = q.split_whitespace().collect::<Vec<_>>().join(" ");
                                let shown: String = if one_line.chars().count() > theme::SQL_HISTORY_CHARS { format!("{}\u{2026}", one_line.chars().take(theme::SQL_HISTORY_CHARS).collect::<String>()) } else { one_line };
                                if ui.selectable_label(false, shown).on_hover_text(q).clicked() {
                                    pick = Some(q.clone());
                                }
                            }
                        })
                        .response
                })
                .inner;
            rects.history = r.rect;
            ui.add_space(theme::SPACE_S);
            // the last error, in a box that scrolls
            let err_frame = egui::Frame::none().stroke(theme::line(if tab.sql_error.is_some() { tk.danger } else { tk.border })).rounding(theme::RADIUS_CONTROL).inner_margin(theme::SPACE_S);
            let boxed = err_frame.show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical().id_salt("sql_error_scroll").max_height(theme::SQL_ERROR_H).auto_shrink([false, true]).show(ui, |ui| match &tab.sql_error {
                    Some(e) => {
                        ui.add(egui::Label::new(egui::RichText::new(e).color(tk.danger)).wrap());
                    }
                    None => {
                        ui.label(egui::RichText::new("No error.").color(tk.muted));
                    }
                });
            });
            rects.error = boxed.response.rect;
            if let Some(q) = pick.take() {
                tab.sql_scratchpad = q;
            }
        });
        self.sql_rects = rects;
        if let Some(new_tab) = run {
            self.run_sql_query(new_tab);
        }
    }

    /// Checks the query on the worker; the step is added when the check passes.
    pub(crate) fn run_sql_query(&mut self, new_tab: bool) {
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        let query = tab.sql_scratchpad.trim().to_string();
        if query.is_empty() {
            tab.sql_error = Some("Type a query first. The table is called self, for example SELECT * FROM self LIMIT 100.".to_string());
            return;
        }
        remember_query(&mut tab.sql_history, &query);
        tab.sql_error = None;
        let req = SqlCheckRequest { tab_id: tab.id, source: tab.source.clone(), steps: tab.steps.clone(), query, new_tab, temp_dir: self.paths.temp_dir.clone() };
        let key = JobKey::tab(JobKind::SqlCheck, tab.id);
        self.jobs.submit(key, "Checking the query", false, move |ctx| micro_parquet::tab_jobs::check_sql_query(req, ctx).map(|r| JobOutput::SqlChecked(Box::new(r))));
        self.status_message = "Checking the query\u{2026}".to_string();
    }

    /// The worker's answer: an error stays under the editor; a good query becomes a step.
    pub(crate) fn apply_sql_checked(&mut self, r: micro_parquet::tab_jobs::SqlChecked) {
        let Some(idx) = self.tab_position(r.tab_id) else {
            return;
        };
        match r.error {
            Some(e) => {
                if let Some(tab) = self.tabs.get_mut(idx) {
                    tab.sql_error = Some(e.clone());
                }
                self.status_message = format!("The query was not added: {}", e);
            }
            None => {
                if let Some(tab) = self.tabs.get_mut(idx) {
                    tab.sql_error = None;
                }
                self.active_tab = idx;
                self.push_step_to_active_tab(TransformStep::SqlQuery { query: r.query }, r.new_tab);
            }
        }
    }
}
