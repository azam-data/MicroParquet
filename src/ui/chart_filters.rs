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
use crate::ui::value_picker::{picker_body_single, picker_body_with, picker_order, PickerPanel};
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
    /// The field is a lookup field (Phase 9 M1): `name` and `column` are its name, "Branch master · Region".
    pub lookup: Option<micro_parquet::model::ModelFieldRef>,
}

impl FieldKey {
    pub fn column(name: &str) -> Self {
        FieldKey { name: name.to_string(), column: name.to_string(), part: None, lookup: None }
    }

    pub fn of(name: &str, part: &Option<DateFieldRef>) -> Self {
        match part {
            Some(p) => FieldKey { name: p.name.clone(), column: p.column.clone(), part: Some(p.clone()), lookup: None },
            None => Self::column(name),
        }
    }

    /// A lookup field.
    pub fn of_lookup(f: &micro_parquet::model::ModelFieldRef) -> Self {
        FieldKey { name: f.name.clone(), column: f.name.clone(), part: None, lookup: Some(f.clone()) }
    }

    /// The chart's category, series or panels: a column, a date part or a lookup field.
    pub fn of_field(name: &str, part: &Option<DateFieldRef>, model: &Option<micro_parquet::model::ModelFieldRef>) -> Self {
        match model.as_ref().filter(|m| m.name == name) {
            Some(m) => Self::of_lookup(m),
            None => Self::of(name, part),
        }
    }

    fn of_condition(c: &FilterCondition) -> Self {
        match &c.lookup {
            Some(l) => Self::of_lookup(l),
            None => Self::of(&c.column, &c.part),
        }
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
    /// The operator of the filter being added on a date column (W9): `None` is the tick list, the others compare with a limit.
    pub add_op: Option<CondOp>,
    /// Its limit and, for "between", the upper limit; and which of the two the open list is for.
    pub add_value: String,
    pub add_value2: String,
    pub add_list: Option<u8>,
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
        let req = micro_parquet::tab_jobs::PickerRequest { tab_id: tab.id, token: panel.token, source: tab.source.clone(), steps, field: key.name.clone(), search, temp_dir: self.paths.temp_dir.clone(), lookup: key.lookup.clone() };
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
        ui.panel = PickerPanel { owner: Some(0), field: key.name.clone(), token, part: key.part.clone(), lookup: key.lookup.clone(), ..Default::default() };
        ui.open = Some(key.clone());
        self.chart_picker_request(key, None);
    }

    /// The section in the studio. `spec` is the chart as it is being edited; a change of a filter is a change of the spec.
    pub(crate) fn studio_filters_section(&mut self, ui: &mut egui::Ui, spec: &mut ChartSpec, avail: &[String], rects: &mut ChartRects) {
        let tk = *theme::tokens(self.dark_mode);
        let calendar = self.calendar();
        let sort_lists = self.sort_lists.lists.clone();
        let date_cols: Vec<String> = self.tabs.get(self.active_tab).map(|t| t.cached_slice.get_columns().iter().filter(|c| micro_parquet::dates::is_date_type(c.dtype())).map(|c| c.name().to_string()).collect()).unwrap_or_default();
        let groups = self.chart_model_groups();
        let lookup_named = |n: &str| groups.iter().find_map(|g| g.columns.iter().find(|(r, _)| r.name == n).map(|(r, _)| r.clone()));
        let mut panel = std::mem::take(&mut self.chart.filters_ui.panel);
        let mut open = self.chart.filters_ui.open.clone();
        let mut add = self.chart.filters_ui.add.clone();
        let mut add_op = self.chart.filters_ui.add_op;
        let mut add_value = std::mem::take(&mut self.chart.filters_ui.add_value);
        let mut add_value2 = std::mem::take(&mut self.chart.filters_ui.add_value2);
        let mut add_list = self.chart.filters_ui.add_list;
        let mut add_done = false;
        let mut toggle: Option<FieldKey> = None;
        let mut open_now: Option<FieldKey> = None;
        let mut ask: Option<(FieldKey, Option<String>)> = None;
        let mut remove: Option<usize> = None;
        let mut new_filter: Option<FilterCondition> = None;
        let mut add_problem_text: Option<String> = None;
        let fields: Vec<(&str, FieldKey)> = [
            ("Category", Some(FieldKey::of_field(&spec.category, &spec.category_part, &spec.category_model)).filter(|k| !k.name.is_empty())),
            ("Series", spec.series.as_ref().map(|s| FieldKey::of_field(s, &spec.series_part, &spec.series_model))),
            ("Panels", spec.facet.as_ref().map(|s| FieldKey::of_field(s, &spec.facet_part, &spec.facet_model))),
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
                Some(i) => (spec.filters[i].picked_with_no_match(picker_selection(&spec.filters[i])), spec.filters[i].or_blank),
                // no filter: every value is kept, so every one is ticked (when they are all listed)
                None if complete => (listed.clone(), has_blank),
                None => (Vec::new(), false),
            };
            panel.part = key.part.clone();
            panel.lookup = key.lookup.clone();
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
                    let mut c = FilterCondition { column: key.column.clone(), op: CondOp::InList, or_blank: blank, part: key.part.clone(), lookup: key.lookup.clone(), ..Default::default() };
                    c.set_picked(values.clone());
                    c.value = c.values.join(", ");
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
                    Some(c) => format!("{} kept", c.values.len() + usize::from(c.or_blank) + usize::from(c.no_match) + usize::from(c.blank_key)),
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
                            add_op = None;
                            add_value.clear();
                            add_value2.clear();
                            add_list = None;
                        }
                    }
                    // Phase 9 M1: the lookup columns of each link
                    if let Some(f) = crate::ui::chart::model_items(ui, &groups, add.as_ref().map(|(a, _)| a.as_str()), tk.warning) {
                        add = Some((f.name.clone(), None));
                        open_now = Some(FieldKey::of_lookup(&f));
                        add_op = None;
                        add_value.clear();
                        add_value2.clear();
                        add_list = None;
                    }
                });
                rects.fields.push(("Add a filter".to_string(), r.response.rect));
                // a date column: its values are dates, or a part of them
                if let Some((col, part)) = add.clone().filter(|(c, _)| date_cols.contains(c)) {
                    let shown = part.map(|p| p.label()).unwrap_or("Date");
                    let r = egui::ComboBox::from_id_salt("chart_filter_by").width(theme::FIELD_W_OP).height(theme::CHART_PICKER_POPUP_H).selected_text(format!("By {}", shown)).show_ui(ui, |ui| {
                        if ui.selectable_label(part.is_none(), "Date").clicked() && part.is_some() {
                            add = Some((col.clone(), None));
                            open_now = Some(FieldKey::column(&col));
                            // values of one kind are not values of another
                            add_value.clear();
                            add_value2.clear();
                            add_list = None;
                        }
                        for p in DatePart::FILTER_BY {
                            if ui.selectable_label(part == Some(p), p.label()).clicked() && part != Some(p) {
                                add = Some((col.clone(), Some(p)));
                                open_now = Some(FieldKey::of(&col, &Some(DateFieldRef::new(&col, p, calendar))));
                                add_value.clear();
                                add_value2.clear();
                                add_list = None;
                            }
                        }
                    });
                    rects.fields.push(("Add by".to_string(), r.response.rect));
                }
            });
            // the operator (W9): the ticked values, or a comparison with a limit. By stays whichever is chosen. (Its own line: the
            // studio's panel is narrow, and a pop-up list does not wrap with the row.)
            if let Some((col, part)) = add.clone().filter(|(c, _)| date_cols.contains(c)) {
                ui.horizontal(|ui| {
                    let op_text = add_op.map(|o| o.label()).unwrap_or("is in list");
                    let r = egui::ComboBox::from_id_salt("chart_filter_op").width(theme::FIELD_W_OP).height(theme::CHART_PICKER_POPUP_H).selected_text(op_text).show_ui(ui, |ui| {
                        if ui.selectable_label(add_op.is_none(), "is in list").clicked() {
                            add_op = None;
                            add_list = None;
                        }
                        for op in [CondOp::Equals, CondOp::NotEquals, CondOp::Gt, CondOp::Gte, CondOp::Lt, CondOp::Lte, CondOp::Between] {
                            if ui.selectable_label(add_op == Some(op), op.label()).clicked() && add_op != Some(op) {
                                if op != CondOp::Between {
                                    add_value2.clear();
                                }
                                add_op = Some(op);
                                add_list = None;
                            }
                        }
                    });
                    rects.fields.push(("Add op".to_string(), r.response.rect));
                });
                if let Some(op) = add_op {
                    ui.horizontal_wrapped(|ui| {
                        let hint = part.map(|p| p.example()).unwrap_or("YYYY-MM-DD");
                        for slot in 0..(if op == CondOp::Between { 2u8 } else { 1u8 }) {
                            if slot == 1 {
                                ui.label("and");
                            }
                            let text = if slot == 0 { &mut add_value } else { &mut add_value2 };
                            let e = ui.add(egui::TextEdit::singleline(text).desired_width(theme::FIELD_W_VALUE_NARROW).hint_text(hint));
                            rects.fields.push((format!("Add value {}", slot + 1), e.rect));
                            if part.is_some() {
                                let on = add_list == Some(slot);
                                let b = ui.add(egui::SelectableLabel::new(on, "\u{25be}")).on_hover_text("Choose from the values in the table");
                                rects.fields.push((format!("Add list {}", slot + 1), b.rect));
                                if b.clicked() {
                                    add_list = if on { None } else { Some(slot) };
                                }
                            }
                        }
                        // the filter, built from what is typed: a value that is not that part is said in words and Add stays off
                        let c = FilterCondition { column: col.clone(), op, value: add_value.trim().to_string(), value2: add_value2.trim().to_string(), part: part.map(|p| DateFieldRef::new(&col, p, calendar)), ..Default::default() };
                        let problem = add_problem(&c);
                        let b = ui.add_enabled(problem.is_none(), egui::Button::new("Add filter"));
                        rects.fields.push(("Add filter".to_string(), b.rect));
                        if b.clicked() {
                            new_filter = Some(c);
                        } else if let Some(p) = problem.filter(|_| !add_value.trim().is_empty()) {
                            add_problem_text = Some(p);
                        }
                    });
                }
            }
            if let Some(p) = &add_problem_text {
                ui.add(egui::Label::new(egui::RichText::new(p).size(theme::TEXT_LABEL).color(tk.danger)).wrap());
            }
            // the list of the value being chosen, the part's values in the part's order
            if let (Some(slot), Some((col, Some(p)))) = (add_list.filter(|_| add_op.is_some()), add.clone()) {
                let key = FieldKey::of(&col, &Some(DateFieldRef::new(&col, p, calendar)));
                if open.as_ref() == Some(&key) {
                    let current = if slot == 0 { add_value.clone() } else { add_value2.clone() };
                    let order = p.sort_order(&calendar).unwrap_or_default();
                    panel.part = key.part.clone();
                    panel.calendar = calendar;
                    let (picked, out) = picker_body_single(ui, &tk, &mut panel, &current, &order);
                    if let Some(v) = picked {
                        if slot == 0 {
                            add_value = v;
                        } else {
                            add_value2 = v;
                        }
                    }
                    if let Some(req) = out.request {
                        ask = Some((key.clone(), req));
                    }
                }
            }
            if let Some(c) = new_filter.take() {
                spec.filters.push(c);
                add_done = true;
            }
            // the tick list of the filter being added (until it has a value, it is not a filter)
            if let Some((col, part)) = add.clone().filter(|_| add_op.is_none()) {
                let key = match lookup_named(&col) {
                    Some(f) => FieldKey::of_lookup(&f),
                    None => FieldKey::of(&col, &part.map(|p| DateFieldRef::new(&col, p, calendar))),
                };
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
        self.chart.filters_ui.add_op = add_op;
        self.chart.filters_ui.add_value = add_value;
        self.chart.filters_ui.add_value2 = add_value2;
        self.chart.filters_ui.add_list = add_list;
        if add_done {
            // the filter is in the list above now: the adding row starts again
            let ui = &mut self.chart.filters_ui;
            ui.add = None;
            ui.add_op = None;
            ui.add_value.clear();
            ui.add_value2.clear();
            ui.add_list = None;
            ui.open = None;
        }
        // the adding row is done once its filter exists: the list row takes over
        if let Some((col, part)) = self.chart.filters_ui.add.clone() {
            let key = match lookup_named(&col) {
                Some(f) => FieldKey::of_lookup(&f),
                None => FieldKey::of(&col, &part.map(|p| DateFieldRef::new(&col, p, calendar))),
            };
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

/// Why the filter being added cannot be added yet, in words; `None` when it can. A value by a part is checked as the part (W9);
/// by Date it is a day written YYYY-MM-DD.
fn add_problem(c: &FilterCondition) -> Option<String> {
    if c.value.trim().is_empty() {
        return Some("Enter a value.".to_string());
    }
    if c.op == CondOp::Between && c.value2.trim().is_empty() {
        return Some("Enter the upper limit too.".to_string());
    }
    if c.part.is_some() {
        return crate::ui::filter::part_problem(c);
    }
    let day = |v: &str| chrono::NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d").is_ok();
    for v in [&c.value, &c.value2].into_iter().take(if c.op == CondOp::Between { 2 } else { 1 }) {
        if !day(v) {
            return Some(format!("'{}' is not a date. Write it as YYYY-MM-DD (for example 2026-10-31).", v.trim()));
        }
    }
    None
}
