//! Transform ▸ Split date… (Phase 8 W1): one new column per date part ("Open Date Year", "Open Date Month" …)
//! from a Date or Datetime column. Also a button of the Column panel when the inspected column is a date,
//! and in Ctrl+K. The step copies the calendar in force when it is made.

use crate::ui::prelude::*;
use micro_parquet::dates::{is_date_type, CalendarSettings, DatePart};
use polars::prelude::{col, DataType, IntoLazy};

/// The title of the dialog.
pub const SPLIT_DATE_TITLE: &str = "Split date";

/// What the dialog drew, for the render tests.
#[derive(Clone, Debug, Default)]
pub struct SplitDateRects {
    pub column: Option<egui::Rect>,
    pub prefix: Option<egui::Rect>,
    /// The tick boxes, by the part's name.
    pub parts: Vec<(String, egui::Rect)>,
    pub preview: Option<egui::Rect>,
    pub calendar: Option<egui::Rect>,
    pub message: Option<egui::Rect>,
    pub buttons: Vec<(String, egui::Rect)>,
}

#[derive(Clone, Debug, Default)]
pub struct SplitDateState {
    pub column: String,
    pub parts: Vec<DatePart>,
    pub prefix: String,
    /// The calendar of the step: the settings when the dialog opened, or the edited step's own.
    pub calendar: CalendarSettings,
    pub rects: SplitDateRects,
}

impl MicroParquetApp {
    /// Opens the dialog for `column` (the inspected column, or the first date column of the tab).
    pub(crate) fn open_split_date(&mut self, column: Option<String>) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let df = &tab.cached_slice;
        let first_date = df.get_columns().iter().find(|c| is_date_type(c.dtype())).map(|c| c.name().to_string());
        let column = column.or(first_date).unwrap_or_default();
        self.split_date = SplitDateState { prefix: column.clone(), column, parts: DatePart::DEFAULT_SPLIT.to_vec(), calendar: self.calendar(), rects: Default::default() };
        self.active_tool = ActiveToolModal::SplitDate;
    }

    /// The step the dialog makes, in the order of the parts list.
    pub(crate) fn split_date_step(&self) -> TransformStep {
        let s = &self.split_date;
        let parts: Vec<DatePart> = DatePart::ALL.iter().copied().filter(|p| s.parts.contains(p)).collect();
        let prefix = if s.prefix.trim() == s.column.trim() { String::new() } else { s.prefix.trim().to_string() };
        TransformStep::SplitDate { column: s.column.clone(), parts, calendar: s.calendar, prefix }
    }

    /// Why the step cannot be made, from what the dialog's tab shows; `None` when it can.
    fn split_date_problem(&self, avail_cols: &[String], dtypes: &HashMap<String, DataType>) -> Option<String> {
        let step = self.split_date_step();
        let TransformStep::SplitDate { column, parts, calendar, .. } = &step else {
            return None;
        };
        let names = step.split_date_names();
        micro_parquet::engine::split_date_check(column, parts, calendar, &names, |c| {
            if avail_cols.iter().any(|a| a == c) {
                Some(dtypes.get(c).cloned().unwrap_or(DataType::Null))
            } else {
                None
            }
        })
        .err()
        .map(|e| e.to_string().trim_start_matches("Split date: ").to_string())
    }

    pub(crate) fn render_split_date_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let mut step: Option<(TransformStep, bool)> = None;
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = SplitDateRects::default();
        let slice = self.tabs.get(self.active_tab).map(|t| t.cached_slice.clone()).unwrap_or_default();
        let dtypes: HashMap<String, DataType> = slice.get_columns().iter().map(|c| (c.name().to_string(), c.dtype().clone())).collect();
        let (date_cols, other_cols): (Vec<String>, Vec<String>) = avail_cols.iter().cloned().partition(|c| dtypes.get(c).is_some_and(is_date_type));
        let problem = self.split_date_problem(&avail_cols, &dtypes);
        let editing = self.editing.is_some();
        theme::modal(&format!("{}  {}", theme::ph::CALENDAR, SPLIT_DATE_TITLE)).default_width(theme::DIALOG_W_TOOL).show(ctx, |d| {
            d.body(|ui| {
                ui.add(egui::Label::new("Adds one column per part of a date. A blank date gives blank parts. The step keeps the calendar below, so a macro gives the same parts later.").wrap());
                ui.add_space(theme::SPACE_S);
                egui::Grid::new("split_date_fields").num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_S)).show(ui, |ui| {
                    ui.label("Date column:");
                    let before = self.split_date.column.clone();
                    let r = egui::ComboBox::from_id_salt("split_date_col").width(theme::FIELD_W_SORT_COLUMN).selected_text(&self.split_date.column).show_ui(ui, |ui| {
                        for c in &date_cols {
                            ui.selectable_value(&mut self.split_date.column, c.clone(), c);
                        }
                        if !other_cols.is_empty() {
                            ui.separator();
                            ui.label(egui::RichText::new("Not dates").size(theme::TEXT_LABEL).color(tk.muted));
                            for c in &other_cols {
                                ui.selectable_value(&mut self.split_date.column, c.clone(), c);
                            }
                        }
                    });
                    rects.column = Some(r.response.rect);
                    // a new column takes its own name as the prefix, unless one was typed
                    if self.split_date.column != before && (self.split_date.prefix.trim().is_empty() || self.split_date.prefix.trim() == before.trim()) {
                        self.split_date.prefix = self.split_date.column.clone();
                    }
                    ui.end_row();
                    ui.label("Names start with:");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.split_date.prefix).desired_width(theme::FIELD_W_SORT_COLUMN).hint_text("the column's name"));
                    rects.prefix = Some(r.rect);
                    ui.end_row();
                });
                ui.add_space(theme::SPACE_S);
                ui.label(egui::RichText::new("PARTS").size(theme::TEXT_LABEL).color(tk.muted).strong());
                egui::Grid::new("split_date_parts").num_columns(3).spacing(egui::vec2(theme::SPACE_L, theme::SPACE_XS)).show(ui, |ui| {
                    for (i, p) in DatePart::ALL.iter().enumerate() {
                        let mut on = self.split_date.parts.contains(p);
                        let r = ui.checkbox(&mut on, format!("{}  ({})", p.label(), p.example()));
                        if r.changed() {
                            if on {
                                self.split_date.parts.push(*p);
                            } else {
                                self.split_date.parts.retain(|x| x != p);
                            }
                        }
                        rects.parts.push((p.label().to_string(), r.rect));
                        if i % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
                ui.add_space(theme::SPACE_S);
                let r = ui.label(egui::RichText::new(self.split_date.calendar.status_line()).color(tk.muted));
                rects.calendar = Some(r.rect);
                if editing {
                    ui.label(egui::RichText::new("This step keeps the calendar it was made with.").size(theme::TEXT_LABEL).color(tk.muted));
                }
                ui.add_space(theme::SPACE_S);
                match &problem {
                    Some(p) => {
                        let r = ui.scope(|ui| theme::error_label(ui, p));
                        rects.message = Some(r.response.rect);
                    }
                    None => {
                        let preview = split_date_preview(&slice, &self.split_date_step());
                        let r = egui::Frame::none().show(ui, |ui| match preview {
                            Ok(df) => preview_grid(ui, &tk, &df),
                            Err(e) => {
                                theme::error_label(ui, &e);
                            }
                        });
                        rects.preview = Some(r.response.rect);
                    }
                }
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let n = self.split_date.parts.len();
                    let label = if editing { "Save step".to_string() } else { format!("Add {} column{}", n, if n == 1 { "" } else { "s" }) };
                    let go = ui.add_enabled_ui(problem.is_none(), |ui| theme::primary_button(ui, label.clone())).inner;
                    rects.buttons.push((label, go.rect));
                    if go.clicked() {
                        step = Some((self.split_date_step(), false));
                        close = true;
                    }
                    let c = ui.button("Cancel");
                    rects.buttons.push(("Cancel".to_string(), c.rect));
                    if c.clicked() {
                        close = true;
                    }
                });
            });
        });
        self.split_date.rects = rects;
        ModalOutcome { close, step }
    }
}

/// The first 5 rows of the page in view with the new columns (the date and the parts).
pub(crate) fn split_date_preview(slice: &DataFrame, step: &TransformStep) -> Result<DataFrame, String> {
    let TransformStep::SplitDate { column, calendar, .. } = step else {
        return Err("Not a Split date step.".to_string());
    };
    let mut exprs = vec![col(column.as_str())];
    for (part, name) in step.split_date_names() {
        exprs.push(part.expr(column, *calendar).alias(name.as_str()));
    }
    slice.head(Some(5)).lazy().select(exprs).collect().map_err(|e| format!("The preview could not be made: {}", e))
}

fn preview_grid(ui: &mut egui::Ui, tk: &theme::Tokens, df: &DataFrame) {
    egui::ScrollArea::horizontal().id_salt("split_date_preview").show(ui, |ui| {
        egui::Grid::new("split_date_preview_grid").striped(true).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_XS)).show(ui, |ui| {
            for c in df.get_columns() {
                ui.label(egui::RichText::new(c.name().as_str()).size(theme::TEXT_LABEL).color(tk.muted).strong());
            }
            ui.end_row();
            for i in 0..df.height() {
                for c in df.get_columns() {
                    let v = match c.get(i) {
                        Ok(AnyValue::Null) | Err(_) => "(blank)".to_string(),
                        Ok(AnyValue::String(s)) => s.to_string(),
                        Ok(AnyValue::Date(d)) => micro_parquet::dates::date_from_days(d).map(|d| d.format("%d %b %Y").to_string()).unwrap_or_default(),
                        Ok(v) => v.to_string(),
                    };
                    ui.label(egui::RichText::new(v).size(theme::TEXT_LABEL));
                }
                ui.end_row();
            }
        });
    });
}
