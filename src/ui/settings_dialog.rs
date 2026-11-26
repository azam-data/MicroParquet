//! One Settings dialog (UI revamp) with a list on the left: Display, Units & numbers, Calendar & fiscal year, As-of date, Colours,
//! Sort lists, Toolbar & panels. Display, units, calendar and toolbar are set here; the as-of date, the house colours and the sort lists
//! keep their own editors (they hold a lot) and open from their section. The values are the ones the View and Model menus held.

use crate::ui::prelude::*;
use micro_parquet::dates::{CalendarSettings, Weekday, MONTHS_LONG};
use micro_parquet::workspace::{ModelStep, SettingsSection};

pub const SETTINGS_TITLE: &str = "Settings";

impl MicroParquetApp {
    /// Opens the dialog at a section (none: the one it had).
    pub(crate) fn open_settings(&mut self, section: Option<SettingsSection>) {
        if let Some(s) = section {
            self.ws.settings_section = s;
        }
        if self.ws.settings_section == SettingsSection::Calendar {
            self.calendar_dialog.draft = self.settings.calendar;
        }
        self.ws.settings_open = true;
    }

    fn settings_select(&mut self, s: SettingsSection) {
        self.ws.settings_section = s;
        if s == SettingsSection::Calendar {
            self.calendar_dialog.draft = self.settings.calendar;
        }
    }

    pub(crate) fn render_settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.ws.settings_open {
            return;
        }
        let tk = *theme::tokens(self.dark_mode);
        let mut open = true;
        let mut rects: Vec<(String, egui::Rect)> = Vec::new();
        let mut select: Option<SettingsSection> = None;
        let mut leave: Option<SettingsSection> = None;
        let mut close = false;
        let section = self.ws.settings_section;
        theme::modal(SETTINGS_TITLE).open(&mut open).default_width(theme::DIALOG_W_SETTINGS).show(ctx, |d| {
            d.body(|ui| {
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(egui::vec2(theme::SETTINGS_NAV_W, theme::SETTINGS_BODY_MIN_H), egui::Layout::top_down(egui::Align::Min), |ui| {
                        for s in SettingsSection::ALL {
                            let r = ui.selectable_label(s == section, s.label());
                            rects.push((s.label().to_string(), r.rect));
                            if r.clicked() {
                                select = Some(s);
                            }
                        }
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.set_min_height(theme::SETTINGS_BODY_MIN_H);
                        ui.label(theme::semibold(section.label()).size(theme::TEXT_HEADING));
                        ui.add_space(theme::SPACE_S);
                        match section {
                            SettingsSection::Display => self.settings_display(ui, ctx, &mut rects),
                            SettingsSection::Units => self.settings_units(ui, &mut rects),
                            SettingsSection::Calendar => self.settings_calendar(ui, &mut rects),
                            SettingsSection::AsOf => {
                                let text = self.model_as_of_text().unwrap_or_else(|| "Not set: the model has no calendar yet.".to_string());
                                ui.label(theme::semibold(text).color(tk.ink));
                                ui.add(egui::Label::new(egui::RichText::new("The date the model reports as of: balances, windows and the status bar use it. It is saved in the project.").color(tk.muted)).wrap());
                                ui.add_space(theme::SPACE_S);
                                let b = ui.button("Change the as-of date\u{2026}");
                                rects.push(("Change the as-of date".to_string(), b.rect));
                                if b.clicked() {
                                    leave = Some(section);
                                }
                            }
                            SettingsSection::Colours => {
                                ui.add(egui::Label::new(egui::RichText::new("House colours name the colours of your bank and are offered in every chart's colour picker.").color(tk.muted)).wrap());
                                ui.add_space(theme::SPACE_S);
                                let b = ui.button("Edit house colours\u{2026}");
                                rects.push(("Edit house colours".to_string(), b.rect));
                                if b.clicked() {
                                    leave = Some(section);
                                }
                            }
                            SettingsSection::SortLists => {
                                ui.add(egui::Label::new(egui::RichText::new(format!("{} saved. A sort list puts values in your own order (tenors, risk grades) wherever a table or chart sorts.", plural_lists(self.sort_lists.lists.len()))).color(tk.muted)).wrap());
                                ui.add_space(theme::SPACE_S);
                                let b = ui.button("Edit sort lists\u{2026}");
                                rects.push(("Edit sort lists".to_string(), b.rect));
                                if b.clicked() {
                                    leave = Some(section);
                                }
                            }
                            SettingsSection::Toolbar => self.settings_toolbar(ui, &mut rects),
                        }
                    });
                });
            });
            d.footer(|ui| {
                let c = ui.button("Close");
                rects.push(("Close".to_string(), c.rect));
                close = c.clicked();
            });
        });
        self.ws.settings_rects = rects;
        if let Some(s) = select {
            self.settings_select(s);
        }
        if close || !open {
            self.ws.settings_open = false;
        }
        if let Some(s) = leave {
            self.ws.settings_open = false;
            match s {
                SettingsSection::AsOf => self.open_model_flow(Some(ModelStep::AsOf)),
                SettingsSection::Colours => self.open_house_colours(),
                SettingsSection::SortLists => self.open_sort_lists(),
                _ => {}
            }
        }
    }

    fn settings_display(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, rects: &mut Vec<(String, egui::Rect)>) {
        ui.horizontal(|ui| {
            ui.label("Theme");
            for (name, dark) in [("Light", false), ("Dark", true)] {
                let r = ui.selectable_label(self.dark_mode == dark, name);
                rects.push((format!("Theme {name}"), r.rect));
                if r.clicked() && self.dark_mode != dark {
                    self.dark_mode = dark;
                    theme::apply_ui_theme(ctx, self.dark_mode, self.ui_zoom);
                }
            }
        });
        ui.add_space(theme::SPACE_S);
        ui.horizontal(|ui| {
            ui.label(format!("Zoom {}", crate::ui::zoom::percent_text(self.ui_zoom)));
            if ui.button("\u{2212}").on_hover_text("Zoom out (Ctrl+-)").clicked() {
                self.zoom_out(ctx);
            }
            if ui.button("+").on_hover_text("Zoom in (Ctrl+=)").clicked() {
                self.zoom_in(ctx);
            }
            if ui.button("Reset").on_hover_text("Back to the default zoom (Ctrl+0)").clicked() {
                self.zoom_reset(ctx);
            }
            if ui.button("Make this the default").clicked() {
                let z = self.ui_zoom;
                self.zoom_set_default(z);
            }
        });
        ui.add_space(theme::SPACE_S);
        let r = ui.checkbox(&mut self.show_summary_row, "Summary row under the table");
        rects.push(("Summary row".to_string(), r.rect));
    }

    fn settings_units(&mut self, ui: &mut egui::Ui, rects: &mut Vec<(String, egui::Rect)>) {
        let tk = *theme::tokens(self.dark_mode);
        ui.horizontal(|ui| {
            ui.add_sized([theme::SETTINGS_LABEL_W, 0.0], egui::Label::new("Money shown in"));
            let r = egui::ComboBox::from_id_salt("settings_units").selected_text(self.num_format.label()).show_ui(ui, |ui| {
                for fmt in NumberDisplayFormat::all() {
                    ui.selectable_value(&mut self.num_format, *fmt, fmt.label());
                }
            });
            rects.push(("Money shown in".to_string(), r.response.rect));
        });
        ui.horizontal(|ui| {
            ui.add_sized([theme::SETTINGS_LABEL_W, 0.0], egui::Label::new("Decimal places"));
            if ui.small_button(" - ").clicked() && self.global_decimals > 0 {
                self.global_decimals -= 1;
            }
            ui.label(theme::semibold(format!("{}", self.global_decimals)));
            if ui.small_button(" + ").clicked() && self.global_decimals < 6 {
                self.global_decimals += 1;
            }
        });
        ui.horizontal(|ui| {
            ui.add_sized([theme::SETTINGS_LABEL_W, 0.0], egui::Label::new("Rates"));
            ui.label("Percent (9.5 means 9.5%); shocks are in basis points");
        });
        ui.add_space(theme::SPACE_S);
        ui.add(egui::Label::new(egui::RichText::new("Tables and exports use these units. A dashboard item has its own unit (Crore unless you change it), so a chart and a table side by side agree.").color(tk.muted)).wrap());
    }

    fn settings_calendar(&mut self, ui: &mut egui::Ui, rects: &mut Vec<(String, egui::Rect)>) {
        let tk = *theme::tokens(self.dark_mode);
        ui.add(egui::Label::new(egui::RichText::new("Used by Split date, the date parts of pivots, charts and filters, and the Calendar sort order. A step keeps the calendar it was made with, so a change here does not change steps you already have.").color(tk.muted)).wrap());
        ui.add_space(theme::SPACE_S);
        ui.label(theme::label_text("FISCAL YEAR STARTS IN").color(tk.muted));
        egui::Grid::new("settings_months").num_columns(4).spacing(egui::vec2(theme::SPACE_S, theme::SPACE_XS)).show(ui, |ui| {
            for (i, m) in MONTHS_LONG.iter().enumerate() {
                let r = ui.selectable_value(&mut self.calendar_dialog.draft.fy_start_month, i as u32 + 1, *m);
                rects.push((m.to_string(), r.rect));
                if i % 4 == 3 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(theme::SPACE_S);
        ui.label(theme::label_text("WEEK STARTS ON").color(tk.muted));
        ui.horizontal_wrapped(|ui| {
            for day in Weekday::ALL {
                let r = ui.selectable_value(&mut self.calendar_dialog.draft.week_start, day, day.long());
                rects.push((day.long().to_string(), r.rect));
            }
        });
        ui.add_space(theme::SPACE_S);
        ui.label(theme::semibold(self.calendar_dialog.draft.status_line()).color(tk.ink));
        ui.add_space(theme::SPACE_S);
        let changed = self.calendar_dialog.draft != self.settings.calendar;
        ui.horizontal(|ui| {
            let save = ui.add_enabled(changed, egui::Button::new("Save calendar"));
            rects.push(("Save calendar".to_string(), save.rect));
            if save.clicked() {
                self.settings.calendar = self.calendar_dialog.draft;
                self.save_settings();
                self.status_message = format!("Calendar saved: {}. Steps you already have keep their own calendar.", self.settings.calendar.status_line());
            }
            let d = ui.button("Defaults").on_hover_text("January and Saturday");
            rects.push(("Defaults".to_string(), d.rect));
            if d.clicked() {
                self.calendar_dialog.draft = CalendarSettings::default();
            }
        });
    }

    fn settings_toolbar(&mut self, ui: &mut egui::Ui, rects: &mut Vec<(String, egui::Rect)>) {
        let tk = *theme::tokens(self.dark_mode);
        // two columns, so the whole section is in view without scrolling: the side panel's tabs, then the Data view's buttons
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            ui.label(theme::label_text("TABS OF THE SIDE PANEL").color(tk.muted));
            for t in crate::ui::steps::RightTab::ALL {
                let mut on = !self.settings.side_panel_hidden.iter().any(|n| n == t.name());
                let r = ui.checkbox(&mut on, t.name());
                rects.push((format!("Tab {}", t.name()), r.rect));
                if r.changed() {
                    self.set_side_tab_visible(t, on);
                }
            }
            let ui = &mut cols[1];
            ui.label(theme::label_text("TOOLBAR BUTTONS (DATA VIEW)").color(tk.muted));
            for id in micro_parquet::commands::toolbar_buttons() {
                let name = crate::ui::commands::toolbar_label(id);
                let mut on = !self.settings.toolbar_hidden.iter().any(|n| n == name);
                let r = ui.checkbox(&mut on, name);
                rects.push((format!("Button {name}"), r.rect));
                if r.changed() {
                    self.settings.toolbar_hidden.retain(|n| n != name);
                    if !on {
                        self.settings.toolbar_hidden.push(name.to_string());
                    }
                    self.save_settings();
                }
            }
        });
    }
}

fn plural_lists(n: usize) -> String {
    format!("{} {}", n, if n == 1 { "list" } else { "lists" })
}
