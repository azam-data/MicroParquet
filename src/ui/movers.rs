//! Top movers (Phase 13): Banking ▸ Reports ▸ "Top movers". One click on a tab with a date column and a balance gives, for each
//! month, the lines whose balance moved most against the month before. The date, balance and line columns are guessed from the tab;
//! the dialog only asks to confirm them.

use crate::ui::prelude::*;
use micro_parquet::movers::{guess, movers_formats, MoversSpec, DEFAULT_MONTHS, DEFAULT_TOP};

/// What the dialog drew, for the render tests.
#[derive(Clone, Debug, Default)]
pub struct MoversRects {
    pub combos: Vec<(String, egui::Rect)>,
    pub fields: Vec<(String, egui::Rect)>,
    pub buttons: Vec<(String, egui::Rect)>,
}

pub struct MoversState {
    pub date: String,
    pub bal: String,
    pub key1: String,
    /// Empty: one key only.
    pub key2: String,
    pub top_text: String,
    pub months_text: String,
    /// The tab the columns were guessed for.
    pub guessed_for: Option<u64>,
    pub rects: MoversRects,
}

impl Default for MoversState {
    fn default() -> Self {
        MoversState { date: String::new(), bal: String::new(), key1: String::new(), key2: String::new(), top_text: DEFAULT_TOP.to_string(), months_text: DEFAULT_MONTHS.to_string(), guessed_for: None, rects: MoversRects::default() }
    }
}

fn whole(t: &str) -> Option<usize> {
    t.trim().parse::<usize>().ok()
}

impl MicroParquetApp {
    pub(crate) fn open_movers(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            self.status_message = "Open the tab with the balances first.".to_string();
            return;
        };
        // a new tab gets a new guess; the same tab keeps what the banker chose
        if self.movers.guessed_for != Some(tab.id) {
            let cols: Vec<(String, polars::prelude::DataType)> = tab.cached_slice.get_columns().iter().map(|c| (c.name().to_string(), c.dtype().clone())).collect();
            let g = guess(&cols);
            self.movers.date = g.date_col;
            self.movers.bal = g.bal_col;
            self.movers.key1 = g.key_cols.first().cloned().unwrap_or_default();
            self.movers.key2 = String::new();
            self.movers.guessed_for = Some(tab.id);
        }
        self.active_tool = ActiveToolModal::TopMovers;
    }

    /// The settings the dialog holds, or the first thing that is missing.
    pub(crate) fn movers_spec(&self) -> Result<MoversSpec, String> {
        let m = &self.movers;
        let top = whole(&m.top_text).ok_or_else(|| "Type how many movers to list for each month as a whole number (for example 10).".to_string())?;
        let months = whole(&m.months_text).ok_or_else(|| "Type how many months to list as a whole number (0 lists every month).".to_string())?;
        let mut keys = vec![m.key1.clone()];
        if !m.key2.trim().is_empty() {
            keys.push(m.key2.clone());
        }
        let spec = MoversSpec { date_col: m.date.clone(), bal_col: m.bal.clone(), key_cols: keys, top, months };
        spec.check().map_err(|e| format!("{:#}", e))?;
        Ok(spec)
    }

    /// The tab the dialog would make, or why it cannot yet.
    pub(crate) fn movers_source(&self) -> Result<TabSource, String> {
        let Some(t) = self.tabs.get(self.active_tab) else {
            return Err("Open the tab with the balances first.".to_string());
        };
        let spec = self.movers_spec()?;
        let exec = t.exec_steps.min(t.steps.len());
        Ok(TabSource::TopMovers { source: Box::new(t.source.clone()), steps: t.steps[..exec].to_vec(), spec })
    }

    pub(crate) fn render_movers_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let (mut close, mut build) = (false, false);
        let mut rects = MoversRects::default();
        let columns: Vec<String> = self.tabs.get(self.active_tab).map(|t| t.available_columns()).unwrap_or_default();
        let problem = self.movers_source().err();
        let title = format!("{}  Top movers", theme::ph::TREND_UP);
        theme::modal(&title).default_width(theme::MIGRATION_DIALOG_W).show(ctx, |d| {
            d.body(|ui| {
                ui.label(egui::RichText::new(format!("Tab: {}", self.tabs.get(self.active_tab).map(|t| t.title.clone()).unwrap_or_default())).color(tk.muted));
                ui.add_space(theme::SPACE_S);
                egui::Grid::new("movers_grid").num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_S)).show(ui, |ui| {
                    let mut pick = |ui: &mut egui::Ui, label: &str, id: &str, cur: &mut String, optional: bool| {
                        ui.label(label);
                        let shown = if cur.is_empty() { if optional { "(none)".to_string() } else { "Choose\u{2026}".to_string() } } else { cur.clone() };
                        let r = egui::ComboBox::from_id_salt(id).width(theme::MIGRATION_COMBO_W).selected_text(shown).show_ui(ui, |ui| {
                            if optional {
                                ui.selectable_value(cur, String::new(), "(none)");
                            }
                            for c in &columns {
                                ui.selectable_value(cur, c.clone(), c);
                            }
                        });
                        rects.combos.push((label.to_string(), r.response.rect));
                        ui.end_row();
                    };
                    pick(ui, "Date:", "movers_date", &mut self.movers.date, false);
                    pick(ui, "Balance:", "movers_bal", &mut self.movers.bal, false);
                    pick(ui, "Line:", "movers_key1", &mut self.movers.key1, false);
                    pick(ui, "Second line column (optional):", "movers_key2", &mut self.movers.key2, true);
                    ui.label("Movers per month:");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.movers.top_text).desired_width(theme::CF_DEFAULT_W));
                    rects.fields.push(("Top".to_string(), r.rect));
                    ui.end_row();
                    ui.label("Latest months (0 = all):");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.movers.months_text).desired_width(theme::CF_DEFAULT_W));
                    rects.fields.push(("Months".to_string(), r.rect));
                    ui.end_row();
                });
                if let Some(p) = &problem {
                    ui.add_space(theme::SPACE_S);
                    theme::error_label(ui, p);
                }
                ui.add_space(theme::SPACE_S);
                ui.add(egui::Label::new(egui::RichText::new("For each month, the balance of every line on the month's last date with data is set against the month before, and the lines that moved most (up or down) are listed with their change. A line that was not there before is marked New, one that is gone Closed. The columns above were guessed from the tab; change them if they are wrong.").color(tk.muted).size(theme::TEXT_LABEL)).wrap());
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let b = ui.add_enabled_ui(problem.is_none(), |ui| theme::primary_button(ui, "Show top movers")).inner;
                    rects.buttons.push(("Show top movers".to_string(), b.rect));
                    if b.clicked() {
                        build = true;
                    }
                    let c = ui.button("Cancel");
                    rects.buttons.push(("Cancel".to_string(), c.rect));
                    if c.clicked() {
                        close = true;
                    }
                });
            });
        });
        self.movers.rects = rects;
        if build {
            if let Ok(source) = self.movers_source() {
                self.create_tab_from_source_with("Top movers".to_string(), source, Vec::new(), movers_formats(), None);
                close = true;
            }
        }
        ModalOutcome { close, step: None }
    }
}
