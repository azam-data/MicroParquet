//! Model ▸ Measures… (Phase 9 M3): Columns. Which numeric columns of a date-linked data tab are balances (a position on a date),
//! flows (an amount in a period) or YTD balances (a trial balance's income and expense GLs). The tag sets the aggregation a column
//! gets when it is added to Values, and warns when a balance is summed over dates. A name that looks like a balance or a flow
//! gets a suggestion, applied by a click and never by itself.

use crate::ui::prelude::*;
use micro_parquet::measures::{suggest, ColumnKind, ColumnTag};

pub const MEASURES_DIALOG_TITLE: &str = "Measures";
/// The first choice of a column's list.
pub const NOT_TAGGED: &str = "Not tagged";

/// What the dialog drew in the last frame (the render tests read it).
#[derive(Clone, Debug, Default)]
pub struct MeasureRects {
    pub buttons: Vec<(String, egui::Rect)>,
    /// The tag box of each column: ("Accounts \u{b7} Balance", rect).
    pub tags: Vec<(String, egui::Rect)>,
    /// The suggestion button of each column that has one.
    pub suggestions: Vec<(String, egui::Rect)>,
    /// The column names.
    pub names: Vec<(String, egui::Rect)>,
    pub message: Option<egui::Rect>,
}

#[derive(Default)]
pub struct MeasuresUi {
    pub rects: MeasureRects,
}

impl MicroParquetApp {
    /// The tag of a column of a data tab, if it has one.
    pub(crate) fn column_tag(&self, tab_key: &str, column: &str) -> Option<ColumnTag> {
        self.project.model.column_kinds.iter().find(|k| k.tab == tab_key && k.column == column).map(|k| k.kind)
    }

    /// Sets or clears (`None`) the tag of a column.
    pub(crate) fn set_column_tag(&mut self, tab_key: &str, column: &str, tag: Option<ColumnTag>) {
        let kinds = &mut self.project.model.column_kinds;
        kinds.retain(|k| !(k.tab == tab_key && k.column == column));
        if let Some(kind) = tag {
            kinds.push(ColumnKind { tab: tab_key.to_string(), column: column.to_string(), kind });
        }
    }

    /// The date-linked data tabs and their numeric columns: (tab id, project key, title, columns).
    fn measure_tabs(&self) -> Vec<(u64, String, String, Vec<String>)> {
        let Some(cal) = &self.project.model.calendar else {
            return Vec::new();
        };
        self.tabs
            .iter()
            .filter_map(|t| {
                let key = self.project.keys.get(&t.id)?;
                cal.date_links.iter().find(|l| &l.data_tab == key)?;
                let numbers: Vec<String> = t.cached_slice.get_columns().iter().filter(|c| c.dtype().is_numeric()).map(|c| c.name().to_string()).collect();
                Some((t.id, key.clone(), t.title.clone(), numbers))
            })
            .collect()
    }

    pub(crate) fn render_model_measures_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = MeasureRects::default();
        let tabs = self.measure_tabs();
        let mut set: Option<(String, String, Option<ColumnTag>)> = None;
        let mut close = false;
        theme::modal(&format!("{}  {}", theme::ph::CALENDAR, MEASURES_DIALOG_TITLE)).default_width(theme::DIALOG_W_MEASURES).show(ctx, |d| {
            d.body(|ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("A balance is a position on a date: adding the balances of several dates means nothing. A flow is an amount in a period: adding is right. Tag the numeric columns of each date-linked tab; the tag sets the aggregation a column gets in Values.")
                            .size(theme::TEXT_LABEL)
                            .color(tk.muted),
                    )
                    .wrap(),
                );
                ui.add_space(theme::SPACE_S);
                ui.label(egui::RichText::new("COLUMNS").size(theme::TEXT_LABEL).color(tk.muted).strong());
                if tabs.is_empty() {
                    let m = ui.add(egui::Label::new(egui::RichText::new("No data tab has a date link yet. Turn on the calendar and link a tab's date column in Model \u{25b8} Calendar\u{2026}.").color(tk.ink)).wrap());
                    rects.message = Some(m.rect);
                    return;
                }
                for (_, key, title, columns) in &tabs {
                    ui.add_space(theme::SPACE_XS);
                    ui.label(egui::RichText::new(title).strong().color(tk.ink));
                    if columns.is_empty() {
                        ui.label(egui::RichText::new("No number column.").color(tk.muted));
                    }
                    for c in columns {
                        let current = self.column_tag(key, c);
                        let id = format!("{} \u{b7} {}", title, c);
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(egui::vec2(theme::MEASURE_NAME_W, ui.spacing().interact_size.y), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.set_min_width(theme::MEASURE_NAME_W);
                                ui.set_max_width(theme::MEASURE_NAME_W);
                                let r = ui.add(egui::Label::new(egui::RichText::new(c).color(tk.ink)).truncate()).on_hover_text(c);
                                rects.names.push((id.clone(), r.rect));
                            });
                            let shown = current.map(|t| t.label()).unwrap_or(NOT_TAGGED);
                            let combo = egui::ComboBox::from_id_salt(("measure_tag", key.as_str(), c.as_str())).width(theme::MEASURE_TAG_W).selected_text(shown).show_ui(ui, |ui| {
                                if ui.selectable_label(current.is_none(), NOT_TAGGED).clicked() {
                                    set = Some((key.clone(), c.clone(), None));
                                }
                                for t in ColumnTag::all() {
                                    if ui.selectable_label(current == Some(*t), t.label()).clicked() {
                                        set = Some((key.clone(), c.clone(), Some(*t)));
                                    }
                                }
                            });
                            rects.tags.push((id.clone(), combo.response.rect));
                            // a name that looks like a balance or a flow: a suggestion, applied by a click
                            if let Some(s) = suggest(c).filter(|s| current != Some(*s)) {
                                let b = ui.add(egui::Button::new(egui::RichText::new(format!("Suggested: {}", s.label())).size(theme::TEXT_LABEL).color(tk.accent_dark)).fill(tk.accent_soft)).on_hover_text("Tag this column so. Nothing is tagged until you click.");
                                rects.suggestions.push((id.clone(), b.rect));
                                if b.clicked() {
                                    set = Some((key.clone(), c.clone(), Some(s)));
                                }
                            }
                        });
                    }
                }
            });
            d.footer(|ui| {
                let c = ui.button("Close");
                rects.buttons.push(("Close".to_string(), c.rect));
                close = c.clicked();
            });
        });
        self.project.model_ui.measures.rects = rects;
        if let Some((key, column, tag)) = set {
            self.set_column_tag(&key, &column, tag);
        }
        if close {
            self.active_tool = ActiveToolModal::None;
        }
        ModalOutcome { close: false, step: None }
    }
}
