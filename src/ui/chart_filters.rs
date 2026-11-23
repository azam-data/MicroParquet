//! "Filters on this chart" (Phase 8 W10): which values of the chart's fields it draws, without touching the tab it reads.
//!
//! The category, the series and the panels each have a tick list of their values (search, All and None, how many rows each has).
//! A column of the tab can be added too, by the same picker as the tab's filter, with the date "By" choice. The filters are the
//! chart's `filters` (the ones "Keep only this" and "Exclude this" write too), so they are all listed here and can be removed
//! here. They are applied before the numbers are added up; the tab is as it was.
//!
//! A tick list keeps the ticked values. With every value ticked there is no filter (and none is stored); with some unticked the
//! chart keeps an "is in list" filter of the ticked ones; with none ticked it keeps no rows, which the chart says in words.
//! When the list of values is too long to list whole, nothing is ticked at first and the ticks are the values to keep.

use crate::ui::chart::ChartRects;
use crate::ui::filter::picker_selection;
use crate::ui::prelude::*;
use crate::ui::value_picker::{picker_body_with, picker_order, PickerPanel};
use micro_parquet::charts::spec::ChartSpec;
use micro_parquet::dates::{DateFieldRef, DatePart};
use micro_parquet::models::{CondOp, FilterCondition};
use micro_parquet::tab_jobs::PickerResult;

/// The name of the section.
pub const FILTERS_TITLE: &str = "Filters on this chart";

/// A field a tick list is of: a column, or a part of a date column.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FieldKey {
    /// The name the values are listed under ("Bank", or "Open Date · Month").
    pub name: String,
    pub column: String,
    pub part: Option<DateFieldRef>,
}

impl FieldKey {
    pub fn column(name: &str) -> Self {
        FieldKey { name: name.to_string(), column: name.to_string(), part: None }
    }

    pub fn of(name: &str, part: &Option<DateFieldRef>) -> Self {
        match part {
            Some(p) => FieldKey { name: p.name.clone(), column: p.column.clone(), part: Some(p.clone()) },
            None => Self::column(name),
        }
    }

    fn of_condition(c: &FilterCondition) -> Self {
        Self::of(&c.column, &c.part)
    }
}

/// The state of the section: the tick list that is open and the filter being added.
#[derive(Default)]
pub struct ChartFilterState {
    pub panel: PickerPanel,
    /// The field whose tick list is open.
    pub open: Option<FieldKey>,
    /// "Add a filter on …": the column chosen and, for a date column, the part its values are of.
    pub add: Option<(String, Option<DatePart>)>,
}

impl MicroParquetApp {
    /// Asks the worker for the values of the open tick list's field (`search`: only those that contain it).
    pub(crate) fn chart_picker_request(&mut self, key: &FieldKey, search: Option<String>) {
        let Some(tab) = self.chart.tab_id.and_then(|id| self.tabs.iter().find(|t| t.id == id)).or_else(|| self.tabs.get(self.active_tab)) else {
            return;
        };
        let mut steps = tab.steps[..tab.exec_steps.min(tab.steps.len())].to_vec();
        if let Some(p) = &key.part {
            steps.push(TransformStep::date_field_step(p));
        }
        let panel = &mut self.chart.filters_ui.panel;
        panel.token += 1;
        panel.waiting = true;
        panel.error = None;
        let req = micro_parquet::tab_jobs::PickerRequest { tab_id: tab.id, token: panel.token, source: tab.source.clone(), steps, field: key.name.clone(), search, temp_dir: self.paths.temp_dir.clone() };
        let tab_id = tab.id;
        self.jobs.submit(JobKey::tab(JobKind::ChartPickerValues, tab_id), "Listing the values", false, move |ctx| micro_parquet::tab_jobs::compute_picker_values(req, ctx).map(|r| JobOutput::ChartPicker(Box::new(r))));
    }

    pub(crate) fn apply_chart_picker_result(&mut self, r: PickerResult) {
        let panel = &mut self.chart.filters_ui.panel;
        if r.token != panel.token || r.field != panel.field {
            return;
        }
        panel.waiting = false;
        match r.values {
            Ok(v) => {
                match r.search {
                    None => panel.full = Some(v),
                    Some(s) => {
                        panel.loaded_search = Some(s.trim().to_lowercase());
                        panel.loaded = Some(v);
                    }
                }
                panel.error = None;
            }
            Err(e) => panel.error = Some(e),
        }
    }

    pub(crate) fn chart_picker_failed(&mut self, msg: String) {
        let panel = &mut self.chart.filters_ui.panel;
        panel.waiting = false;
        panel.error = Some(msg);
    }

    /// Opens (or closes) the tick list of a field.
    fn chart_picker_toggle(&mut self, key: &FieldKey) {
        let ui = &mut self.chart.filters_ui;
        if ui.open.as_ref() == Some(key) {
            ui.open = None;
            return;
        }
        let token = ui.panel.token;
        ui.panel = PickerPanel { owner: Some(0), field: key.name.clone(), token, part: key.part.clone(), ..Default::default() };
        ui.open = Some(key.clone());
        self.chart_picker_request(key, None);
    }

    /// The section in the studio. `spec` is the chart as it is being edited; a change of a filter is a change of the spec.
    pub(crate) fn studio_filters_section(&mut self, ui: &mut egui::Ui, spec: &mut ChartSpec, avail: &[String], rects: &mut ChartRects) {
        let tk = *theme::tokens(self.dark_mode);
        let calendar = self.calendar();
        let sort_lists = self.sort_lists.lists.clone();
        let date_cols: Vec<String> = self.tabs.get(self.active_tab).map(|t| t.cached_slice.get_columns().iter().filter(|c| micro_parquet::dates::is_date_type(c.dtype())).map(|c| c.name().to_string()).collect()).unwrap_or_default();
        let mut panel = std::mem::take(&mut self.chart.filters_ui.panel);
        let mut open = self.chart.filters_ui.open.clone();
        let mut add = self.chart.filters_ui.add.clone();
        let mut toggle: Option<FieldKey> = None;
        let mut open_now: Option<FieldKey> = None;
        let mut ask: Option<(FieldKey, Option<String>)> = None;
        let mut remove: Option<usize> = None;
        let fields: Vec<(&str, FieldKey)> = [
            ("Category", Some(FieldKey::of(&spec.category, &spec.category_part)).filter(|k| !k.name.is_empty())),
            ("Series", spec.series.as_ref().map(|s| FieldKey::of(s, &spec.series_part))),
            ("Panels", spec.facet.as_ref().map(|s| FieldKey::of(s, &spec.facet_part))),
        ]
        .into_iter()
        .filter_map(|(l, k)| k.map(|k| (l, k)))
        .collect();
        let hosts_by_field = |k: &FieldKey| fields.iter().any(|(_, f)| f == k);

        // draws the tick list of `key` where it is open, and applies a change of ticks to the spec
        let mut tick_list = |ui: &mut egui::Ui, spec: &mut ChartSpec, panel: &mut PickerPanel, key: &FieldKey, ask: &mut Option<(FieldKey, Option<String>)>| {
            let existing = spec.filters.iter().position(|c| c.op == CondOp::InList && FieldKey::of_condition(c) == *key);
            let full = panel.full.clone();
            let complete = full.as_ref().is_some_and(|f| !f.more);
            let listed: Vec<String> = full.as_ref().map(|f| f.values.iter().filter_map(|(k, _)| k.clone()).collect()).unwrap_or_default();
            let has_blank = full.as_ref().is_some_and(|f| f.values.iter().any(|(k, _)| k.is_none()));
            let (mut values, mut blank) = match existing {
                Some(i) => (picker_selection(&spec.filters[i]), spec.filters[i].or_blank),
                // no filter: every value is kept, so every one is ticked (when they are all listed)
                None if complete => (listed.clone(), has_blank),
                None => (Vec::new(), false),
            };
            panel.part = key.part.clone();
            panel.calendar = calendar;
            let order = match &key.part {
                Some(p) => p.sort_order().unwrap_or_default(),
                None => picker_order(&sort_lists, &key.column),
            };
            let out = picker_body_with(ui, &tk, panel, &mut values, &mut blank, true, &order, ("All", "None"));
            if out.changed {
                let all = complete && values.len() == listed.len() && listed.iter().all(|v| values.contains(v)) && blank == has_blank;
                let none_and_cut = !complete && values.is_empty() && !blank;
                if all || none_and_cut {
                    if let Some(i) = existing {
                        spec.filters.remove(i);
                    }
                } else {
                    let c = FilterCondition { column: key.column.clone(), op: CondOp::InList, values: values.clone(), or_blank: blank, part: key.part.clone(), value: values.join(", "), ..Default::default() };
                    match existing {
                        Some(i) => spec.filters[i] = c,
                        None => spec.filters.push(c),
                    }
                }
            }
            if let Some(req) = out.request {
                *ask = Some((key.clone(), req));
            }
        };

        let n = spec.filters.len();
        let h = egui::CollapsingHeader::new(format!("{} ({})", FILTERS_TITLE, n)).id_salt("chart_filters").show(ui, |ui| {
            ui.add(egui::Label::new(egui::RichText::new("Tick the values the chart keeps. The tab is not changed.").size(theme::TEXT_LABEL).color(tk.muted)).wrap());
            // ---- the category, the series and the panels
            for (label, key) in &fields {
                let existing = spec.filters.iter().find(|c| c.op == CondOp::InList && FieldKey::of_condition(c) == *key);
                let summary = match existing {
                    Some(c) => format!("{} kept", c.values.len() + usize::from(c.or_blank)),
                    None => "all values".to_string(),
                };
                ui.horizontal(|ui| {
                    ui.label(format!("{}:", label));
                    ui.label(egui::RichText::new(&key.name).color(tk.muted));
                    let on = open.as_ref() == Some(key);
                    let b = ui.add(egui::SelectableLabel::new(on, format!("{}  \u{25be}", summary)));
                    rects.fields.push((format!("Values: {}", label), b.rect));
                    if b.clicked() {
                        toggle = Some(key.clone());
                    }
                });
                if open.as_ref() == Some(key) {
                    tick_list(ui, spec, &mut panel, key, &mut ask);
                }
            }
            if fields.is_empty() {
                ui.label(egui::RichText::new("Choose a Category first.").color(tk.muted));
            }
            // ---- every filter of the chart, the ones Keep only this and Exclude this made too
            ui.add_space(theme::SPACE_XS);
            for i in 0..spec.filters.len() {
                let c = spec.filters[i].clone();
                let key = FieldKey::of_condition(&c);
                // (the buttons come first and the text last: a long filter is cut at the panel's edge, never the buttons)
                ui.horizontal(|ui| {
                    let b = theme::icon_button(ui, theme::ph::X, "Remove this filter from the chart");
                    rects.fields.push((format!("Filter {} remove", i + 1), b.rect));
                    if b.clicked() {
                        remove = Some(i);
                    }
                    // a list of values of a column the chart does not use as a field has its own tick list here
                    if c.op == CondOp::InList && !hosts_by_field(&key) {
                        let on = open.as_ref() == Some(&key);
                        let b = ui.add(egui::SelectableLabel::new(on, "Values  \u{25be}"));
                        rects.fields.push((format!("Filter {} values", i + 1), b.rect));
                        if b.clicked() {
                            toggle = Some(key.clone());
                        }
                    }
                    ui.add(egui::Label::new(egui::RichText::new(ChartSpec::filter_text(&c)).color(tk.ink)).truncate());
                });
                if c.op == CondOp::InList && !hosts_by_field(&key) && open.as_ref() == Some(&key) {
                    tick_list(ui, spec, &mut panel, &key, &mut ask);
                }
            }
            // ---- add a filter on any column
            ui.horizontal(|ui| {
                let shown = add.as_ref().map(|(c, _)| c.clone()).unwrap_or_else(|| "Add a filter on\u{2026}".to_string());
                let r = egui::ComboBox::from_id_salt("chart_filter_add").width(theme::CHART_COMBO_W).height(theme::CHART_PICKER_POPUP_H).selected_text(shown).show_ui(ui, |ui| {
                    for c in avail {
                        if ui.selectable_label(add.as_ref().is_some_and(|(a, _)| a == c), c).clicked() {
                            add = Some((c.clone(), None));
                            open_now = Some(FieldKey::column(c));
                        }
                    }
                });
                rects.fields.push(("Add a filter".to_string(), r.response.rect));
                // a date column: its values are dates, or a part of them
                if let Some((col, part)) = add.clone().filter(|(c, _)| date_cols.contains(c)) {
                    let shown = part.map(|p| p.label()).unwrap_or("Date");
                    let r = egui::ComboBox::from_id_salt("chart_filter_by").width(theme::FIELD_W_OP).selected_text(format!("By {}", shown)).show_ui(ui, |ui| {
                        if ui.selectable_label(part.is_none(), "Date").clicked() {
                            add = Some((col.clone(), None));
                            open_now = Some(FieldKey::column(&col));
                        }
                        for p in DatePart::FILTER_BY {
                            if ui.selectable_label(part == Some(p), p.label()).clicked() {
                                add = Some((col.clone(), Some(p)));
                                open_now = Some(FieldKey::of(&col, &Some(DateFieldRef::new(&col, p, calendar))));
                            }
                        }
                    });
                    rects.fields.push(("Add by".to_string(), r.response.rect));
                }
            });
            // the tick list of the filter being added (until it has a value, it is not a filter)
            if let Some((col, part)) = add.clone() {
                let key = FieldKey::of(&col, &part.map(|p| DateFieldRef::new(&col, p, calendar)));
                if open.as_ref() == Some(&key) && !hosts_by_field(&key) && !spec.filters.iter().any(|c| c.op == CondOp::InList && FieldKey::of_condition(c) == key) {
                    tick_list(ui, spec, &mut panel, &key, &mut ask);
                }
            }
        });
        rects.fields.push((FILTERS_TITLE.to_string(), h.header_response.rect));
        if let Some(i) = remove {
            let gone = FieldKey::of_condition(&spec.filters[i]);
            spec.filters.remove(i);
            if open.as_ref() == Some(&gone) && !fields.iter().any(|(_, f)| *f == gone) {
                open = None;
            }
        }
        // a tick list of a date part is of the part: when the adding row's choice changed, its list is a new one
        self.chart.filters_ui.panel = panel;
        self.chart.filters_ui.open = open;
        self.chart.filters_ui.add = add;
        // the adding row is done once its filter exists: the list row takes over
        if let Some((col, part)) = self.chart.filters_ui.add.clone() {
            let key = FieldKey::of(&col, &part.map(|p| DateFieldRef::new(&col, p, calendar)));
            if spec.filters.iter().any(|c| c.op == CondOp::InList && FieldKey::of_condition(c) == key) {
                self.chart.filters_ui.add = None;
            }
        }
        if let Some(k) = open_now {
            self.chart.filters_ui.open = None;
            self.chart_picker_toggle(&k);
        } else if let Some(k) = toggle {
            // a click on the button of the open list closes it; on another's, opens that
            self.chart_picker_toggle(&k);
        }
        if let Some((k, search)) = ask {
            self.chart_picker_request(&k, search);
        }
    }
}
