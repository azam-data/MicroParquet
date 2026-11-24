//! Model ▸ Calendar… and Model ▸ As-of date… (Phase 9 M2): the one calendar every data tab of the project reads, the date link of
//! each data tab, its snapshot dates, and the date the model reports as of.

use crate::ui::prelude::*;
use chrono::NaiveDate;
use micro_parquet::dates::{Weekday, MONTHS_LONG};
use micro_parquet::jobs::{JobKey, JobKind};
use micro_parquet::model::{self, AsOf, DateLink, ModelCalendar};
use micro_parquet::model_calendar as mc;
use micro_parquet::model_calendar::SnapshotInfo;
use micro_parquet::tab_jobs::{compute_date_info, DateInfoRequest, DateInfoResult, JobOutput};
use std::hash::{Hash, Hasher};

pub const CALENDAR_DIALOG_TITLE: &str = "Model calendar";
pub const AS_OF_DIALOG_TITLE: &str = "As-of date";

/// The snapshot dates of one date-linked tab, as the last job found them.
#[derive(Clone, Debug)]
pub struct DateInfoRuntime {
    pub column: String,
    /// The state of the tab and column the result was made for.
    pub fingerprint: Option<u64>,
    pub in_flight: bool,
    pub info: Option<Result<SnapshotInfo, String>>,
}

/// What the two dialogs drew in the last frame (the render tests read it).
#[derive(Clone, Debug, Default)]
pub struct CalRects {
    pub buttons: Vec<(String, egui::Rect)>,
    /// "Use the model calendar", "Fiscal year starts in", "Week starts on", "Date column: Accounts".
    pub combos: Vec<(String, egui::Rect)>,
    pub checkbox: Option<egui::Rect>,
    pub status: Option<egui::Rect>,
    /// The notes under a tab's snapshot dates: ("Accounts", kind, rect) with kind "summary", "single", "incomplete", "error".
    pub notes: Vec<(String, String, egui::Rect)>,
    pub error: Option<egui::Rect>,
    /// The as-of dialog: the two choices, the box, the ends note, the preview.
    pub radios: Vec<(String, egui::Rect)>,
    pub as_of_box: Option<egui::Rect>,
    pub ends: Vec<egui::Rect>,
    pub ends_note: Option<egui::Rect>,
    pub preview: Option<egui::Rect>,
}

#[derive(Default)]
pub struct CalUi {
    pub rects: CalRects,
    /// A refused date column (text), said in the dialog.
    pub error: String,
    /// The date typed in the as-of dialog and what is wrong with it.
    pub as_of_text: String,
    pub as_of_error: String,
}

fn hash_of<T: Hash>(v: &T) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

impl MicroParquetApp {
    // ---------------------------------------------------------------------------- the model's calendar

    pub(crate) fn model_calendar(&self) -> Option<&ModelCalendar> {
        self.project.model.calendar.as_ref()
    }

    /// The Calendar tab of the model, when it is open.
    pub(crate) fn model_calendar_tab(&self) -> Option<usize> {
        let key = &self.project.model.calendar.as_ref()?.tab;
        let id = self.model_tab_id(key)?;
        self.tabs.iter().position(|t| t.id == id)
    }

    /// True when the tab is the generated Calendar.
    pub(crate) fn is_calendar_source(&self, idx: usize) -> bool {
        self.tabs.get(idx).is_some_and(|t| matches!(t.source, TabSource::Calendar { .. }))
    }

    /// What the steps panel says on the Calendar tab.
    pub(crate) fn calendar_tab_note(&self) -> Option<&'static str> {
        if self.is_calendar_source(self.active_tab) {
            Some(mc::CALENDAR_NOTE)
        } else {
            None
        }
    }

    /// The years the Calendar tab covers now.
    fn calendar_years(&self) -> Option<(i32, i32)> {
        match &self.tabs.get(self.model_calendar_tab()?)?.source {
            TabSource::Calendar { first_year, last_year, .. } => Some((*first_year, *last_year)),
            _ => None,
        }
    }

    /// Turns the model calendar on: the settings are copied from `settings.json`, and the Calendar tab is made.
    pub(crate) fn model_calendar_on(&mut self) {
        if self.project.model.calendar.is_some() {
            return;
        }
        self.project.model.calendar = Some(ModelCalendar { settings: self.settings.calendar, date_links: Vec::new(), tab: String::new() });
        self.model_calendar_ensure_tab();
        self.status_message = format!("Model calendar on: {}.", self.settings.calendar.status_line());
    }

    /// Turns it off: the date links go, and so does the Calendar tab. A saved pivot that reads a Calendar field says so.
    pub(crate) fn model_calendar_off(&mut self) {
        if let Some(i) = self.model_calendar_tab() {
            self.close_tab(i);
        }
        let key = self.project.model.calendar.as_ref().map(|c| c.tab.clone()).unwrap_or_default();
        self.project.model.calendar = None;
        self.project.model.as_of = AsOf::Latest;
        self.project.model_ui.date_info.clear();
        self.project.model_ui.runtime.retain(|k, _| k.1 != key);
        self.status_message = "Model calendar off. The date links are gone.".to_string();
    }

    /// Makes the Calendar tab when it is not open (it is generated, so it is made again from the model).
    pub(crate) fn model_calendar_ensure_tab(&mut self) {
        let Some(cal) = self.project.model.calendar.clone() else { return };
        if self.model_calendar_tab().is_some() {
            return;
        }
        let year = chrono::Datelike::year(&chrono::Local::now());
        let (first, last) = self.model_calendar_target_years().unwrap_or((year, year));
        let source = TabSource::Calendar { settings: cal.settings, first_year: first, last_year: last };
        let id = self.create_tab_from_source_with(mc::CALENDAR_TITLE.to_string(), source, Vec::new(), HashMap::new(), None);
        // the tab keeps its project key when it is made again
        if cal.tab.is_empty() {
            let key = self.model_ensure_key(id);
            if let Some(c) = self.project.model.calendar.as_mut() {
                c.tab = key;
            }
        } else {
            self.project.keys.retain(|_, k| *k != cal.tab);
            self.project.keys.insert(id, cal.tab.clone());
        }
    }

    /// Changes the calendar's settings: every Calendar field of the project changes with them.
    pub(crate) fn model_calendar_set_settings(&mut self, settings: micro_parquet::dates::CalendarSettings) {
        let Some(c) = self.project.model.calendar.as_mut() else { return };
        if c.settings == settings {
            return;
        }
        c.settings = settings;
        self.model_calendar_rebuild();
        self.status_message = format!("Model calendar: {}. Every Calendar field changes with it.", settings.status_line());
    }

    /// Gives the date column of a data tab as its date link (`None`: no link). A column that is not a date is refused in words.
    pub(crate) fn model_set_date_link(&mut self, tab_id: u64, column: Option<String>) -> Result<(), String> {
        if self.project.model.calendar.is_none() {
            return Err("Turn on the model calendar first.".to_string());
        }
        let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) else { return Err("That tab is closed.".to_string()) };
        let title = tab.title.clone();
        let dtype = column.as_ref().map(|c| tab.cached_slice.get_columns().iter().find(|x| x.name() == c.as_str()).map(|x| x.dtype().clone()));
        if let (Some(c), Some(dtype)) = (&column, dtype) {
            match dtype {
                Some(dt) => micro_parquet::dates::check_date_column(c, &dt).map_err(|e| format!("{:#}", e))?,
                None => return Err(format!("There is no column '{}' in '{}'.", c, title)),
            }
        }
        let key = self.model_ensure_key(tab_id);
        if let Some(c) = self.project.model.calendar.as_mut() {
            c.date_links.retain(|l| l.data_tab != key);
            if let Some(column) = column {
                c.date_links.push(DateLink { data_tab: key.clone(), column });
            }
        }
        self.project.model_ui.date_info.remove(&key);
        self.project.model_ui.runtime.retain(|k, _| k.0 != key || !self.project.model.calendar.as_ref().is_some_and(|c| c.tab == k.1));
        Ok(())
    }

    /// The years the Calendar must cover: from the earliest year to the latest of the linked tabs' dates. `None` until each has been read.
    fn model_calendar_target_years(&self) -> Option<(i32, i32)> {
        let cal = self.project.model.calendar.as_ref()?;
        let mut dates: Vec<NaiveDate> = Vec::new();
        for l in &cal.date_links {
            let rt = self.project.model_ui.date_info.get(&l.data_tab)?;
            match rt.info.as_ref()? {
                Ok(i) => dates.extend(i.first.into_iter().chain(i.last)),
                Err(_) => return None,
            }
        }
        mc::year_range(&dates)
    }

    /// The Calendar tab is made again from the model: its settings, and the years the linked dates need.
    fn model_calendar_rebuild(&mut self) {
        let Some(cal) = self.project.model.calendar.clone() else { return };
        let Some(idx) = self.model_calendar_tab() else {
            self.model_calendar_ensure_tab();
            return;
        };
        let (first, last) = self.model_calendar_target_years().or_else(|| self.calendar_years()).unwrap_or((2026, 2026));
        let source = TabSource::Calendar { settings: cal.settings, first_year: first, last_year: last };
        if let Some(t) = self.tabs.get_mut(idx) {
            t.source = source;
        }
        self.refresh_tab_at(idx, true, None);
    }

    // ---------------------------------------------------------------------------- the snapshot dates

    /// Called every frame: reads the snapshot dates of each date-linked tab when it is new or has changed, and makes the Calendar
    /// tab cover the years they need.
    pub(crate) fn model_calendar_tick(&mut self) {
        let Some(cal) = self.project.model.calendar.clone() else { return };
        if !self.project.active || self.project.opening.is_some() {
            return;
        }
        self.project.model_ui.date_info.retain(|k, _| cal.date_links.iter().any(|l| &l.data_tab == k));
        for l in &cal.date_links {
            let Some(tab) = self.model_worker_tab(&l.data_tab) else { continue };
            let fp = hash_of(&(micro_parquet::steps_edit::pipeline_hash(&tab.source, &tab.steps), l.column.clone()));
            let rt = self.project.model_ui.date_info.entry(l.data_tab.clone()).or_insert(DateInfoRuntime { column: l.column.clone(), fingerprint: None, in_flight: false, info: None });
            if rt.fingerprint == Some(fp) || rt.in_flight {
                continue;
            }
            rt.in_flight = true;
            rt.column = l.column.clone();
            let req = DateInfoRequest { tab_key: l.data_tab.clone(), column: l.column.clone(), source: tab.source, steps: tab.steps, fingerprint: fp, temp_dir: self.paths.temp_dir.clone() };
            self.jobs.submit(JobKey::unique(JobKind::DateInfo), "Reading the snapshot dates", false, move |ctx| compute_date_info(req, ctx).map(|r| JobOutput::DateInfo(Box::new(r))));
        }
        // the Calendar covers whole years from the earliest to the latest date found; it is made again when they change
        if let (Some(target), Some(now)) = (self.model_calendar_target_years(), self.calendar_years()) {
            let settings_now = match self.model_calendar_tab().and_then(|i| self.tabs.get(i)).map(|t| &t.source) {
                Some(TabSource::Calendar { settings, .. }) => Some(*settings),
                _ => None,
            };
            if target != now || settings_now.is_some_and(|s| s != cal.settings) {
                self.model_calendar_rebuild();
            }
        }
    }

    /// A date job came back.
    pub(crate) fn apply_date_info(&mut self, r: DateInfoResult) {
        let Some(rt) = self.project.model_ui.date_info.get_mut(&r.tab_key) else { return };
        if rt.column != r.column {
            return;
        }
        rt.in_flight = false;
        rt.fingerprint = Some(r.fingerprint);
        rt.info = Some(r.counts.map(|c| SnapshotInfo::from_counts(&c)));
    }

    pub(crate) fn date_info_failed(&mut self, msg: &str) {
        for rt in self.project.model_ui.date_info.values_mut() {
            if rt.in_flight {
                rt.in_flight = false;
                rt.fingerprint = None;
                rt.info = Some(Err(msg.to_string()));
            }
        }
    }

    /// Each date-linked tab's last snapshot date, by the tab's title (the tabs whose dates have been read).
    pub(crate) fn model_ends(&self) -> Vec<(String, NaiveDate)> {
        let Some(cal) = self.project.model.calendar.as_ref() else { return Vec::new() };
        cal.date_links
            .iter()
            .filter_map(|l| {
                let last = match self.project.model_ui.date_info.get(&l.data_tab)?.info.as_ref()? {
                    Ok(i) => i.last?,
                    Err(_) => return None,
                };
                Some((self.model_tab_title(&l.data_tab).trim_end_matches(" (not open)").to_string(), last))
            })
            .collect()
    }

    /// The as-of date in force, and whether it is "latest data".
    pub(crate) fn model_as_of(&self) -> Option<(NaiveDate, bool)> {
        self.project.model.calendar.as_ref()?;
        mc::resolve(&self.project.model.as_of, &self.model_ends())
    }

    /// "As of 24 Nov 2026 (latest data)" for the status bar; `None` with no calendar or no date yet.
    pub(crate) fn model_as_of_text(&self) -> Option<String> {
        self.model_as_of().map(|(d, latest)| mc::status_text(d, latest))
    }

    // ---------------------------------------------------------------------------- Model ▸ Calendar…

    /// The tabs a date link can be made on: open tables that are not lookups, not charts and not the Calendar.
    fn model_date_link_candidates(&self) -> Vec<(u64, String, Vec<(String, polars::prelude::DataType)>)> {
        let lookups: Vec<&String> = self.project.model.links.iter().map(|l| &l.lookup_tab).collect();
        self.tabs
            .iter()
            .filter(|t| !self.chart_tabs.contains_key(&t.id) && !matches!(t.source, TabSource::Unavailable { .. } | TabSource::Calendar { .. }))
            .filter(|t| !self.project.keys.get(&t.id).is_some_and(|k| lookups.contains(&k)))
            .map(|t| (t.id, t.title.clone(), t.cached_slice.get_columns().iter().map(|c| (c.name().to_string(), c.dtype().clone())).collect()))
            .collect()
    }

    pub(crate) fn render_model_calendar_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = CalRects::default();
        let mut close = false;
        let on = self.project.model.calendar.is_some();
        let mut turn: Option<bool> = None;
        let mut settings = self.project.model.calendar.as_ref().map(|c| c.settings).unwrap_or(self.settings.calendar);
        let mut link_change: Option<(u64, Option<String>)> = None;
        let candidates = self.model_date_link_candidates();
        let links: Vec<DateLink> = self.project.model.calendar.as_ref().map(|c| c.date_links.clone()).unwrap_or_default();
        let error = self.project.model_ui.cal.error.clone();
        theme::modal(&format!("{}  {}", theme::ph::CALENDAR, CALENDAR_DIALOG_TITLE)).default_width(theme::DIALOG_W_PIVOT_CONVERT).show(ctx, |d| {
            d.body(|ui| {
                let mut checked = on;
                let r = ui.checkbox(&mut checked, "Use one calendar for the whole project");
                rects.checkbox = Some(r.rect);
                if checked != on {
                    turn = Some(checked);
                }
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("Every data tab reads the same fiscal year and week, through its date link. Changing the calendar here changes every Calendar field of the project at once.")
                            .size(theme::TEXT_LABEL)
                            .color(tk.muted),
                    )
                    .wrap(),
                );
                if !on {
                    return;
                }
                ui.add_space(theme::SPACE_S);
                ui.label(egui::RichText::new("FISCAL YEAR STARTS IN").size(theme::TEXT_LABEL).color(tk.muted).strong());
                let c = egui::ComboBox::from_id_salt("model_cal_fy").width(theme::MODEL_COMBO_W).height(theme::CHART_PICKER_POPUP_H).selected_text(MONTHS_LONG[(settings.fy_start_month.clamp(1, 12) - 1) as usize]).show_ui(ui, |ui| {
                    for (i, m) in MONTHS_LONG.iter().enumerate() {
                        ui.selectable_value(&mut settings.fy_start_month, i as u32 + 1, *m);
                    }
                });
                rects.combos.push(("Fiscal year starts in".to_string(), c.response.rect));
                ui.add_space(theme::SPACE_XS);
                ui.label(egui::RichText::new("WEEK STARTS ON").size(theme::TEXT_LABEL).color(tk.muted).strong());
                let c = egui::ComboBox::from_id_salt("model_cal_week").width(theme::MODEL_COMBO_W).height(theme::CHART_PICKER_POPUP_H).selected_text(settings.week_start.long()).show_ui(ui, |ui| {
                    for day in Weekday::ALL {
                        ui.selectable_value(&mut settings.week_start, day, day.long());
                    }
                });
                rects.combos.push(("Week starts on".to_string(), c.response.rect));
                ui.add_space(theme::SPACE_XS);
                let r = ui.add(egui::Label::new(egui::RichText::new(settings.status_line()).color(tk.ink)).wrap());
                rects.status = Some(r.rect);

                ui.add_space(theme::SPACE_S);
                ui.label(egui::RichText::new("DATE LINKS").size(theme::TEXT_LABEL).color(tk.muted).strong());
                ui.add(egui::Label::new(egui::RichText::new("Pick the date column of each data tab (a Date or Date and time column). A tab has one date link.").size(theme::TEXT_LABEL).color(tk.muted)).wrap());
                let key_of = |id: u64, app: &MicroParquetApp| app.project.keys.get(&id).cloned();
                if candidates.is_empty() {
                    ui.label(egui::RichText::new("No data tab is open.").color(tk.muted));
                }
                for (id, title, columns) in &candidates {
                    let key = key_of(*id, self);
                    let current = key.as_ref().and_then(|k| links.iter().find(|l| &l.data_tab == k)).map(|l| l.column.clone());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(title).color(tk.ink));
                        let name = format!("Date column: {}", title);
                        let c = egui::ComboBox::from_id_salt(("model_cal_link", *id)).width(theme::MODEL_COMBO_W).height(theme::CHART_PICKER_POPUP_H).selected_text(current.clone().unwrap_or_else(|| "No date link".to_string())).show_ui(ui, |ui| {
                            if ui.selectable_label(current.is_none(), "No date link").clicked() {
                                link_change = Some((*id, None));
                            }
                            for (n, _) in columns {
                                if ui.selectable_label(current.as_deref() == Some(n.as_str()), n).clicked() {
                                    link_change = Some((*id, Some(n.clone())));
                                }
                            }
                        });
                        rects.combos.push((name, c.response.rect));
                    });
                }
                if !error.is_empty() {
                    let r = ui.add(egui::Label::new(egui::RichText::new(&error).color(tk.danger)).wrap());
                    rects.error = Some(r.rect);
                }

                // the snapshot dates of each linked tab
                if !links.is_empty() {
                    ui.add_space(theme::SPACE_S);
                    ui.label(egui::RichText::new("SNAPSHOT DATES").size(theme::TEXT_LABEL).color(tk.muted).strong());
                }
                for l in &links {
                    let title = self.model_tab_title(&l.data_tab).trim_end_matches(" (not open)").to_string();
                    let info = self.project.model_ui.date_info.get(&l.data_tab).and_then(|r| r.info.clone());
                    let mut note = |ui: &mut egui::Ui, kind: &str, text: String, colour: egui::Color32| {
                        let r = ui.add(egui::Label::new(egui::RichText::new(text).color(colour)).wrap());
                        rects.notes.push((title.clone(), kind.to_string(), r.rect));
                    };
                    match info {
                        None => note(ui, "summary", format!("{}: reading the dates\u{2026}", title), tk.muted),
                        Some(Err(e)) => note(ui, "error", format!("{}: {}", title, e), tk.danger),
                        Some(Ok(i)) => {
                            note(ui, "summary", format!("{}: {}", title, i.summary()), tk.ink);
                            if let Some(t) = i.single_months_text() {
                                note(ui, "single", t, tk.muted);
                            }
                            if let Some(t) = i.incomplete_text() {
                                note(ui, "incomplete", format!("{} {}", theme::ph::WARNING, t), tk.warning);
                            }
                        }
                    }
                }
            });
            d.footer(|ui| {
                let c = ui.button("Close");
                rects.buttons.push(("Close".to_string(), c.rect));
                close = c.clicked();
            });
        });
        self.project.model_ui.cal.rects = rects;
        match turn {
            Some(true) => self.model_calendar_on(),
            Some(false) => self.model_calendar_off(),
            None => {}
        }
        if on && turn.is_none() {
            self.model_calendar_ensure_tab();
            self.model_calendar_set_settings(settings);
        }
        if let Some((id, column)) = link_change {
            match self.model_set_date_link(id, column) {
                Ok(()) => self.project.model_ui.cal.error.clear(),
                Err(e) => self.project.model_ui.cal.error = e,
            }
        }
        if close {
            self.active_tool = ActiveToolModal::None;
            self.project.model_ui.cal.error.clear();
        }
        ModalOutcome { close: false, step: None }
    }

    // ---------------------------------------------------------------------------- Model ▸ As-of date…

    pub(crate) fn render_model_as_of_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = CalRects::default();
        let mut close = false;
        let has_calendar = self.project.model.calendar.is_some();
        let ends = self.model_ends();
        let latest_choice = matches!(self.project.model.as_of, AsOf::Latest);
        let mut pick_latest = false;
        let mut set_date: Option<String> = None;
        let mut text = std::mem::take(&mut self.project.model_ui.cal.as_of_text);
        let err = self.project.model_ui.cal.as_of_error.clone();
        let current = self.model_as_of_text();
        let (note, picked_note) = (
            mc::ends_note(&ends),
            match &self.project.model.as_of {
                AsOf::Date(t) => mc::parse_date(t).ok().and_then(|d| mc::picked_note(d, &ends)),
                AsOf::Latest => None,
            },
        );
        theme::modal(&format!("{}  {}", theme::ph::CALENDAR, AS_OF_DIALOG_TITLE)).default_width(theme::DIALOG_W_PIVOT_CONVERT).show(ctx, |d| {
            d.body(|ui| {
                if !has_calendar {
                    ui.add(egui::Label::new(egui::RichText::new("The as-of date is read from the date-linked tabs. Turn on the model calendar and link a date column first (Model \u{25b8} Calendar\u{2026}).").color(tk.ink)).wrap());
                    return;
                }
                ui.add(egui::Label::new(egui::RichText::new("The date the model reports as of. Balances, windows and the status bar use it.").size(theme::TEXT_LABEL).color(tk.muted)).wrap());
                ui.add_space(theme::SPACE_S);
                let r = ui.radio(latest_choice, "Latest data");
                rects.radios.push(("Latest data".to_string(), r.rect));
                if r.clicked() {
                    pick_latest = true;
                }
                ui.horizontal_wrapped(|ui| {
                    let r = ui.radio(!latest_choice, "A date");
                    rects.radios.push(("A date".to_string(), r.rect));
                    let e = ui.add(egui::TextEdit::singleline(&mut text).desired_width(theme::FIELD_W_VALUE).hint_text("24/11/2026"));
                    rects.as_of_box = Some(e.rect);
                    let b = ui.button("Set date");
                    rects.buttons.push(("Set date".to_string(), b.rect));
                    if b.clicked() || r.clicked() && !text.trim().is_empty() {
                        set_date = Some(text.clone());
                    }
                });
                if !err.is_empty() {
                    ui.add(egui::Label::new(egui::RichText::new(&err).color(tk.danger)).wrap());
                }
                if let Some(n) = &picked_note {
                    ui.add(egui::Label::new(egui::RichText::new(n).color(tk.warning)).wrap());
                }
                ui.add_space(theme::SPACE_S);
                ui.label(egui::RichText::new("LAST DATE OF EACH TAB").size(theme::TEXT_LABEL).color(tk.muted).strong());
                if ends.is_empty() {
                    ui.label(egui::RichText::new("No date-linked tab has been read yet.").color(tk.muted));
                }
                for (t, dte) in &ends {
                    let r = ui.label(format!("{}: {}", t, mc::long_date(*dte)));
                    rects.ends.push(r.rect);
                }
                if let Some(n) = &note {
                    ui.add_space(theme::SPACE_XS);
                    let r = ui.add(egui::Label::new(egui::RichText::new(format!("{} {}", theme::ph::WARNING, n)).color(tk.warning)).wrap());
                    rects.ends_note = Some(r.rect);
                }
                ui.add_space(theme::SPACE_S);
                if let Some(c) = &current {
                    let r = ui.label(theme::semibold(c).color(tk.ink));
                    rects.preview = Some(r.rect);
                }
            });
            d.footer(|ui| {
                let c = ui.button("Close");
                rects.buttons.push(("Close".to_string(), c.rect));
                close = c.clicked();
            });
        });
        self.project.model_ui.cal.rects = rects;
        if text.is_empty() {
            if let AsOf::Date(t) = &self.project.model.as_of {
                text = mc::parse_date(t).map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default();
            }
        }
        self.project.model_ui.cal.as_of_text = text;
        if pick_latest {
            self.project.model.as_of = AsOf::Latest;
            self.project.model_ui.cal.as_of_error.clear();
        }
        if let Some(t) = set_date {
            match mc::parse_date(&t) {
                Ok(d) => {
                    self.project.model.as_of = AsOf::Date(d.format("%Y-%m-%d").to_string());
                    self.project.model_ui.cal.as_of_error.clear();
                    self.status_message = format!("{}.", mc::status_text(d, false));
                }
                Err(e) => self.project.model_ui.cal.as_of_error = format!("{:#}", e),
            }
        }
        if close {
            self.project.model_ui.cal.as_of_error.clear();
            self.active_tool = ActiveToolModal::None;
        }
        ModalOutcome { close: false, step: None }
    }
}

/// The model's `all_links` as the app reads them (the user's links, then the date links).
pub fn effective_links(m: &model::Model) -> Vec<model::Link> {
    m.all_links()
}
