//! View ▸ Calendar… (Phase 8 W0): the first month of the fiscal year and the first day of the week.
//! Saved in `settings.json`; every step or spec that uses a date part copies the settings it was made
//! with, so a change here never alters a step that already exists.

use crate::ui::prelude::*;
use micro_parquet::dates::{CalendarSettings, Weekday, MONTHS_LONG};

/// The title of the dialog.
pub const CALENDAR_TITLE: &str = "Calendar";

/// What the dialog drew, for the render tests.
#[derive(Clone, Debug, Default)]
pub struct CalendarRects {
    /// The month buttons (January …) and the day buttons (Saturday …), by their names.
    pub months: Vec<(String, egui::Rect)>,
    pub days: Vec<(String, egui::Rect)>,
    pub status: Option<egui::Rect>,
    pub buttons: Vec<(String, egui::Rect)>,
}

#[derive(Clone, Debug, Default)]
pub struct CalendarState {
    /// The settings being edited (copied from `settings.json` when the dialog opens).
    pub draft: CalendarSettings,
    pub rects: CalendarRects,
}

impl MicroParquetApp {
    /// The calendar in force: the one in `settings.json`.
    pub(crate) fn calendar(&self) -> CalendarSettings {
        self.settings.calendar
    }

    pub(crate) fn open_calendar_settings(&mut self) {
        self.calendar_dialog.draft = self.settings.calendar;
        self.active_tool = ActiveToolModal::Calendar;
    }

    pub(crate) fn render_calendar_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = CalendarRects::default();
        let mut save = false;
        theme::modal(&format!("{}  {}", theme::ph::CALENDAR, CALENDAR_TITLE)).default_width(theme::DIALOG_W_TOOL).show(ctx, |d| {
            d.body(|ui| {
                ui.add(egui::Label::new("Used by Split date, the date parts of pivots, charts and filters, and the Calendar sort order. A step keeps the calendar it was made with, so changing it here does not change steps you already have.").wrap());
                ui.add_space(theme::SPACE_S);
                ui.label(egui::RichText::new("FISCAL YEAR STARTS IN").size(theme::TEXT_LABEL).color(tk.muted).strong());
                egui::Grid::new("calendar_months").num_columns(4).spacing(egui::vec2(theme::SPACE_S, theme::SPACE_XS)).show(ui, |ui| {
                    for (i, m) in MONTHS_LONG.iter().enumerate() {
                        let r = ui.selectable_value(&mut self.calendar_dialog.draft.fy_start_month, i as u32 + 1, *m);
                        rects.months.push((m.to_string(), r.rect));
                        if i % 4 == 3 {
                            ui.end_row();
                        }
                    }
                });
                ui.add_space(theme::SPACE_S);
                ui.label(egui::RichText::new("WEEK STARTS ON").size(theme::TEXT_LABEL).color(tk.muted).strong());
                ui.horizontal_wrapped(|ui| {
                    for day in Weekday::ALL {
                        let r = ui.selectable_value(&mut self.calendar_dialog.draft.week_start, day, day.long());
                        rects.days.push((day.long().to_string(), r.rect));
                    }
                });
                ui.add_space(theme::SPACE_S);
                let r = ui.add(egui::Label::new(egui::RichText::new(self.calendar_dialog.draft.status_line()).color(tk.ink)).wrap());
                rects.status = Some(r.rect);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("Week of year: W01 is the week that contains 1 January, so it can start in December. Fiscal year: FY2026 with a January start, FY2026-27 with any other.")
                            .size(theme::TEXT_LABEL)
                            .color(tk.muted),
                    )
                    .wrap(),
                );
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let r = theme::primary_button(ui, "Save");
                    rects.buttons.push(("Save".to_string(), r.rect));
                    if r.clicked() {
                        save = true;
                        close = true;
                    }
                    let r = ui.button("Defaults");
                    rects.buttons.push(("Defaults".to_string(), r.rect));
                    if r.on_hover_text("January and Saturday").clicked() {
                        self.calendar_dialog.draft = CalendarSettings::default();
                    }
                    let r = ui.button("Cancel");
                    rects.buttons.push(("Cancel".to_string(), r.rect));
                    if r.clicked() {
                        close = true;
                    }
                });
            });
        });
        self.calendar_dialog.rects = rects;
        if save {
            self.settings.calendar = self.calendar_dialog.draft;
            self.save_settings();
            self.status_message = format!("Calendar saved: {}. Steps you already have keep their own calendar.", self.settings.calendar.status_line());
        }
        ModalOutcome { close, step: None }
    }
}
