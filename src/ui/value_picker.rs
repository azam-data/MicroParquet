//! The value picker of the filter and the conditional column (Phase 6 U7): the column's distinct
//! values with their row counts (calculated on the worker over the whole tab as it stands before
//! the step), a search box, "Select all", "Clear", a "(blank)" entry and "3 of 42 selected".

use crate::ui::prelude::*;
use micro_parquet::value_picker::{picker_summary, PickerValues};

/// What the open picker drew (the render tests read it).
#[derive(Clone, Debug, Default)]
pub struct PickerRects {
    pub panel: Option<egui::Rect>,
    pub search: Option<egui::Rect>,
    pub select_all: Option<egui::Rect>,
    pub clear: Option<egui::Rect>,
    pub summary: Option<egui::Rect>,
    pub note: Option<egui::Rect>,
    /// (the value's text, "(blank)" for the blank entry) and the rectangle of its check box row.
    pub rows: Vec<(String, egui::Rect)>,
    /// The check box itself.
    pub boxes: Vec<(String, egui::Rect)>,
}

/// The picker of the dialog that is open (only one is open at a time).
#[derive(Default)]
pub struct PickerPanel {
    /// The condition (or rule) it is open for.
    pub owner: Option<usize>,
    pub field: String,
    pub search: String,
    /// The listing the worker gave for `loaded_search`.
    pub loaded: Option<PickerValues>,
    pub loaded_search: Option<String>,
    /// The listing without a search; kept to filter locally while it is complete.
    pub full: Option<PickerValues>,
    pub error: Option<String>,
    pub token: u64,
    pub waiting: bool,
    pub rects: PickerRects,
}

impl PickerPanel {
    /// Opens for another condition (or closes when it is open for that one).
    pub fn toggle(&mut self, owner: usize, field: &str) -> bool {
        if self.owner == Some(owner) {
            self.owner = None;
            return false;
        }
        *self = PickerPanel { owner: Some(owner), field: field.to_string(), token: self.token, ..Default::default() };
        true
    }
}

/// What a frame of the picker asks for.
#[derive(Default)]
pub struct PickerOutcome {
    /// The selection changed.
    pub changed: bool,
    /// The list for this search is wanted from the worker (`Some(None)` = no search).
    pub request: Option<Option<String>>,
}

/// The values shown now: the full list (filtered here while it is complete), or the worker's list for the search.
fn shown_values(panel: &PickerPanel, order: &SortOrder) -> Vec<(Option<String>, u64)> {
    let needle = panel.search.trim().to_lowercase();
    let mut v: Vec<(Option<String>, u64)> = match (&panel.full, &panel.loaded) {
        (Some(f), _) if needle.is_empty() => f.values.clone(),
        (Some(f), _) if !f.more => f
            .values
            .iter()
            .filter(|(k, _)| match k {
                Some(t) => t.to_lowercase().contains(&needle),
                None => "(blank)".contains(&needle),
            })
            .cloned()
            .collect(),
        (_, Some(l)) if panel.loaded_search.as_deref() == Some(needle.as_str()) => l.values.clone(),
        _ => Vec::new(),
    };
    v.sort_by(|a, b| order.compare(&a.0, &b.0));
    v
}

/// The order the list is shown in: the custom list suggested for the column, else natural.
pub fn picker_order(lists: &[micro_parquet::sort_lists::SortList], field: &str) -> SortOrder {
    micro_parquet::sort_lists::suggested_for(lists, field).map(SortOrder::from_list).unwrap_or_default()
}

/// Draws the picker for one condition. `values` and `or_blank` are its selection.
pub fn picker_body(ui: &mut egui::Ui, tk: &theme::Tokens, panel: &mut PickerPanel, values: &mut Vec<String>, or_blank: &mut bool, allow_blank: bool, order: &SortOrder) -> PickerOutcome {
    let mut out = PickerOutcome::default();
    let mut rects = PickerRects::default();
    let frame = egui::Frame::none().stroke(theme::line(tk.border)).rounding(theme::RADIUS_CONTROL).inner_margin(theme::SPACE_S).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let shown = shown_values(panel, order);
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut panel.search).desired_width(theme::PICKER_SEARCH_W).hint_text("Search values"));
            rects.search = Some(r.rect);
            let all = ui.button("Select all");
            rects.select_all = Some(all.rect);
            if all.clicked() {
                for (k, _) in &shown {
                    match k {
                        Some(t) if !values.contains(t) => values.push(t.clone()),
                        None if allow_blank => *or_blank = true,
                        _ => {}
                    }
                }
                out.changed = true;
            }
            let clear = ui.button("Clear");
            rects.clear = Some(clear.rect);
            if clear.clicked() {
                values.clear();
                *or_blank = false;
                out.changed = true;
            }
        });
        // a changed search while the full list is cut: the worker looks through all values
        let needle = panel.search.trim().to_string();
        let cut = panel.full.as_ref().is_some_and(|f| f.more);
        if cut && !needle.is_empty() && panel.loaded_search.as_deref() != Some(needle.to_lowercase().as_str()) && !panel.waiting {
            out.request = Some(Some(needle.clone()));
        }
        let total = panel.full.as_ref().map(|f| f.distinct);
        let summary = picker_summary(values.len(), *or_blank, total);
        rects.summary = Some(ui.label(egui::RichText::new(summary).color(tk.muted)).rect);
        if let Some(e) = &panel.error {
            ui.add(egui::Label::new(egui::RichText::new(e).color(tk.danger)).wrap());
        } else if panel.full.is_none() {
            ui.label(egui::RichText::new("Listing the values\u{2026}").color(tk.muted));
        }
        egui::ScrollArea::vertical().id_salt("picker_list").max_height(theme::PICKER_LIST_H).auto_shrink([false, true]).show(ui, |ui| {
            for (k, n) in &shown {
                let label = k.clone().unwrap_or_else(|| "(blank)".to_string());
                if k.is_none() && !allow_blank {
                    continue;
                }
                let mut on = match k {
                    Some(t) => values.contains(t),
                    None => *or_blank,
                };
                let row = ui.horizontal(|ui| {
                    let cb = ui.checkbox(&mut on, "");
                    ui.add(egui::Label::new(egui::RichText::new(&label)).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(theme::mono(format_with_commas(*n as f64, 0)).color(tk.muted));
                    });
                    cb
                });
                rects.rows.push((label.clone(), row.response.rect));
                rects.boxes.push((label.clone(), row.inner.rect));
                if row.inner.changed() {
                    match k {
                        Some(t) => {
                            if on {
                                if !values.contains(t) {
                                    values.push(t.clone());
                                }
                            } else {
                                values.retain(|v| v != t);
                            }
                        }
                        None => *or_blank = on,
                    }
                    out.changed = true;
                }
            }
        });
        if let Some(f) = panel.full.as_ref().filter(|f| f.more) {
            rects.note = Some(ui.add(egui::Label::new(egui::RichText::new(format!("Showing {} of {}; search to find others", format_with_commas(f.values.len() as f64, 0), format_with_commas(f.distinct as f64, 0))).color(tk.muted).size(theme::TEXT_LABEL)).wrap()).rect);
        }
    });
    rects.panel = Some(frame.response.rect);
    panel.rects = rects;
    out
}

impl MicroParquetApp {
    /// How many steps come before the step the dialog builds or edits: the picker lists the
    /// column as it stands there.
    pub(crate) fn picker_prefix_len(&self) -> usize {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return 0;
        };
        let n = tab.steps.len();
        if self.active_tool == ActiveToolModal::FilterRows {
            if let Some(i) = self.filter.editing {
                return i.min(n);
            }
        } else if let Some(ed) = self.editing.as_ref().filter(|e| e.tab_id == tab.id) {
            return ed.index.min(n);
        }
        match self.insert_after {
            Some(i) => (i + 1).min(n),
            None => tab.exec_steps.min(n),
        }
    }

    /// Asks the worker for the values of the picker's column (`search`: only those that contain it).
    pub(crate) fn picker_request(&mut self, search: Option<String>) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let steps = tab.steps[..self.picker_prefix_len().min(tab.steps.len())].to_vec();
        self.picker.token += 1;
        self.picker.waiting = true;
        self.picker.error = None;
        let req = PickerRequest { tab_id: tab.id, token: self.picker.token, source: tab.source.clone(), steps, field: self.picker.field.clone(), search, temp_dir: self.paths.temp_dir.clone() };
        let key = JobKey::tab(JobKind::PickerValues, tab.id);
        self.jobs.submit(key, "Listing the values", false, move |ctx| micro_parquet::tab_jobs::compute_picker_values(req, ctx).map(|r| JobOutput::Picker(Box::new(r))));
    }

    pub(crate) fn apply_picker_result(&mut self, r: micro_parquet::tab_jobs::PickerResult) {
        if r.token != self.picker.token || r.field != self.picker.field {
            return;
        }
        self.picker.waiting = false;
        match r.values {
            Ok(v) => {
                if r.search.is_none() {
                    self.picker_totals.insert(r.field.clone(), v.distinct);
                }
                match r.search {
                    None => self.picker.full = Some(v),
                    Some(s) => {
                        self.picker.loaded_search = Some(s.trim().to_lowercase());
                        self.picker.loaded = Some(v);
                    }
                }
                self.picker.error = None;
            }
            Err(e) => self.picker.error = Some(e),
        }
    }

    pub(crate) fn picker_failed(&mut self, msg: String) {
        self.picker.waiting = false;
        self.picker.error = Some(msg);
    }
}
