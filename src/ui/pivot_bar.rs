//! The bar above a pivot tab (PR #6 items 19 and 20): an "Edit pivot" button that reopens the
//! pivot dialog on the same step (the tab is updated in place), and a value picker for every field
//! in the pivot's Filters, with "All" by default.

use crate::ui::prelude::*;
use micro_parquet::pivot_table::BLANK_LABEL;
use std::collections::HashSet;

/// The most characters of a value shown in a picker button.
const BAR_VALUE_MAX_CHARS: usize = 28;

/// The text of a filter's picker: "Segment: All", "Segment: CMSME", "Segment: CMSME (+2)".
pub fn filter_label(f: &PivotFilter) -> String {
    let cut = |s: String| -> String {
        if s.chars().count() > BAR_VALUE_MAX_CHARS {
            let mut t: String = s.chars().take(BAR_VALUE_MAX_CHARS - 1).collect();
            t.push('\u{2026}');
            t
        } else {
            s
        }
    };
    match f.values.split_first() {
        None => format!("{}: All", f.field),
        Some((first, rest)) => {
            let first = cut(first.clone().unwrap_or_else(|| BLANK_LABEL.to_string()));
            if rest.is_empty() {
                format!("{}: {}", f.field, first)
            } else {
                format!("{}: {} (+{})", f.field, first, rest.len())
            }
        }
    }
}

/// The open checklist of one filter.
#[derive(Clone, Debug)]
pub struct BarPopup {
    pub tab_id: u64,
    pub step_index: usize,
    pub field: String,
    pub search: String,
    /// `None` while loading.
    pub values: Option<Result<(Vec<Option<String>>, bool), String>>,
    pub checked: HashSet<Option<String>>,
}

/// What the bar drew, for the render tests.
#[derive(Clone, Debug)]
pub struct PivotBarRects {
    pub bar: egui::Rect,
    pub edit: egui::Rect,
    pub chips: Vec<(String, egui::Rect)>,
    pub popup: egui::Rect,
}

impl Default for PivotBarRects {
    fn default() -> Self {
        let n = egui::Rect::NOTHING;
        Self { bar: n, edit: n, chips: Vec::new(), popup: n }
    }
}

#[derive(Default)]
pub struct PivotBarState {
    pub popup: Option<BarPopup>,
    pub rects: PivotBarRects,
}

impl MicroParquetApp {
    /// The pivot step of the active tab (its last step, when it is a pivot and is on) and its index.
    pub(crate) fn active_pivot(&self) -> Option<(usize, PivotSpec)> {
        let tab = self.tabs.get(self.active_tab)?;
        let i = tab.steps.len().checked_sub(1)?;
        let spec = tab.steps.get(i)?.as_pivot_spec()?;
        if tab.steps[i].is_skipped() {
            return None;
        }
        Some((i, spec))
    }

    /// A finished job that lists the values of a field, for the bar's checklist. True when it was theirs.
    pub(crate) fn apply_pivot_bar_values(&mut self, r: &micro_parquet::tab_jobs::PivotValuesResult) -> bool {
        let current: Vec<Option<String>> = self.active_pivot().and_then(|(_, s)| s.filters.iter().find(|f| f.field == r.field).map(|f| f.values.clone())).unwrap_or_default();
        let Some(p) = self.pivot_bar.popup.as_mut() else {
            return false;
        };
        if p.tab_id != r.tab_id || p.field != r.field || self.pivot_builder.open {
            return false;
        }
        if let Ok((values, _)) = &r.values {
            // no restriction: every value ticked
            p.checked = if current.is_empty() { values.iter().cloned().collect() } else { current.into_iter().collect() };
        }
        p.values = Some(r.values.clone());
        true
    }

    fn open_bar_popup(&mut self, field: &str) {
        let Some((i, _)) = self.active_pivot() else {
            return;
        };
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let req = micro_parquet::tab_jobs::PivotValuesRequest {
            tab_id: tab.id,
            source: tab.source.clone(),
            steps: tab.steps[..i].to_vec(),
            field: field.to_string(),
            temp_dir: self.paths.temp_dir.clone(),
        };
        self.pivot_bar.popup = Some(BarPopup { tab_id: tab.id, step_index: i, field: field.to_string(), search: String::new(), values: None, checked: HashSet::new() });
        let key = JobKey::tab(JobKind::PivotValues, tab.id);
        self.jobs.submit(key, "Values of a field", false, move |ctx| micro_parquet::tab_jobs::compute_pivot_values(req, ctx).map(|r| JobOutput::PivotValues(Box::new(r))));
    }

    /// Sets the chosen values of one filter of the pivot step, in place: the step is replaced, the
    /// tab refreshes, an undo takes it back. An empty list is "All".
    pub(crate) fn set_pivot_filter_values(&mut self, field: &str, values: Vec<Option<String>>) {
        let Some((i, mut spec)) = self.active_pivot() else {
            return;
        };
        let Some(f) = spec.filters.iter_mut().find(|f| f.field == field) else {
            return;
        };
        if f.values == values {
            return;
        }
        let label = filter_label(&PivotFilter { field: field.to_string(), values: values.clone() });
        f.values = values;
        let step = TransformStep::from_pivot_spec(spec);
        self.replace_step_in_active_tab(i, step, &format!("Pivot filter {}.", label));
        self.selected_step = Some(i);
    }

    /// The bar above a pivot tab; nothing for other tabs.
    pub(crate) fn render_pivot_bar(&mut self, ui: &mut egui::Ui) {
        let Some((idx, spec)) = self.active_pivot() else {
            self.pivot_bar.popup = None;
            self.pivot_bar.rects = PivotBarRects::default();
            return;
        };
        // a popup of another tab or step is not this one's
        let tab_id = self.tabs.get(self.active_tab).map(|t| t.id).unwrap_or_default();
        if self.pivot_bar.popup.as_ref().is_some_and(|p| p.tab_id != tab_id || p.step_index != idx || !spec.filters.iter().any(|f| f.field == p.field)) {
            self.pivot_bar.popup = None;
        }
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = PivotBarRects::default();
        let mut edit = false;
        let mut open_field: Option<String> = None;
        let bar = egui::Frame::none().fill(tk.surface).stroke(theme::line(tk.border)).rounding(theme::RADIUS_PANEL).inner_margin(theme::SPACE_S).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let b = ui.button(format!("{}  Edit pivot", theme::ph::PENCIL_SIMPLE)).on_hover_text("Reopens the pivot dialog with these settings; Save changes updates this tab");
                rects.edit = b.rect;
                if b.clicked() {
                    edit = true;
                }
                if spec.filters.is_empty() {
                    ui.label(egui::RichText::new("No filters: drop a field in Filters (Edit pivot) to limit the pivot to chosen values.").size(theme::TEXT_LABEL).color(tk.muted));
                }
                for f in &spec.filters {
                    let open = self.pivot_bar.popup.as_ref().is_some_and(|p| p.field == f.field);
                    let text = filter_label(f);
                    let mut btn = egui::Button::new(egui::RichText::new(&text).color(if f.values.is_empty() { tk.ink } else { tk.accent }));
                    if open || !f.values.is_empty() {
                        btn = btn.fill(tk.accent_soft);
                    }
                    let r = ui.add(btn).on_hover_text(if f.values.is_empty() { format!("{}: all values. Click to choose some.", f.field) } else { f.values.iter().take(20).map(|v| v.clone().unwrap_or_else(|| BLANK_LABEL.to_string())).collect::<Vec<_>>().join(", ") });
                    rects.chips.push((f.field.clone(), r.rect));
                    if r.clicked() {
                        open_field = Some(f.field.clone());
                    }
                }
            });
        });
        rects.bar = bar.response.rect;
        ui.add_space(theme::SPACE_S);
        self.pivot_bar.rects = rects;
        if edit {
            self.open_step_editor(idx);
        }
        if let Some(f) = open_field {
            if self.pivot_bar.popup.as_ref().is_some_and(|p| p.field == f) {
                self.pivot_bar.popup = None;
            } else {
                self.open_bar_popup(&f);
            }
        }
        self.pivot_bar_popup_ui(ui.ctx());
    }

    fn pivot_bar_popup_ui(&mut self, ctx: &egui::Context) {
        let Some(mut popup) = self.pivot_bar.popup.clone() else {
            return;
        };
        let tk = *theme::tokens(self.dark_mode);
        let Some(anchor) = self.pivot_bar.rects.chips.iter().find(|(f, _)| *f == popup.field).map(|(_, r)| *r) else {
            self.pivot_bar.popup = None;
            return;
        };
        let pos = egui::pos2(anchor.left(), anchor.bottom() + theme::SPACE_XS);
        let mut apply: Option<Vec<Option<String>>> = None;
        let mut close = false;
        let area = egui::Area::new(egui::Id::new("pivot_bar_popup")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            egui::Frame::none().fill(tk.surface).stroke(theme::line(tk.border)).rounding(theme::RADIUS_PANEL).inner_margin(theme::SPACE_M).show(ui, |ui| {
                ui.set_width(theme::PIVOT_POPUP_W);
                ui.label(egui::RichText::new(format!("Values of {}", popup.field)).color(tk.ink).strong());
                ui.add(egui::TextEdit::singleline(&mut popup.search).hint_text("Search values\u{2026}").desired_width(ui.available_width()));
                match popup.values.clone() {
                    None => {
                        ui.label(egui::RichText::new("Loading the values\u{2026}").color(tk.muted));
                    }
                    Some(Err(e)) => {
                        ui.label(egui::RichText::new(e).color(tk.danger));
                    }
                    Some(Ok((values, more))) => {
                        let all: HashSet<Option<String>> = values.iter().cloned().collect();
                        let is_all = !more && popup.checked == all;
                        ui.horizontal(|ui| {
                            let mut on = is_all;
                            if ui.checkbox(&mut on, "All").changed() && on && !more {
                                popup.checked = all.clone();
                                apply = Some(Vec::new());
                            }
                            ui.label(egui::RichText::new(format!("{} of {}{}", popup.checked.len(), values.len(), if more { "+" } else { "" })).size(theme::TEXT_LABEL).color(tk.muted));
                        });
                        let q = popup.search.trim().to_lowercase();
                        egui::ScrollArea::vertical().id_salt("pivot_bar_values").max_height(theme::PIVOT_POPUP_LIST_H).auto_shrink([false, true]).show(ui, |ui| {
                            for v in values.iter().filter(|v| q.is_empty() || v.as_deref().unwrap_or(BLANK_LABEL).to_lowercase().contains(&q)) {
                                let mut on = popup.checked.contains(v);
                                if ui.checkbox(&mut on, v.clone().unwrap_or_else(|| BLANK_LABEL.to_string())).changed() {
                                    if on {
                                        popup.checked.insert(v.clone());
                                    } else {
                                        popup.checked.remove(v);
                                    }
                                    if !popup.checked.is_empty() {
                                        // the chosen values, in the list's order (all of them is "All")
                                        let chosen: Vec<Option<String>> = {
                                            let mut c: Vec<Option<String>> = values.iter().filter(|x| popup.checked.contains(*x)).cloned().collect();
                                            c.extend(popup.checked.iter().filter(|x| !all.contains(*x)).cloned());
                                            c
                                        };
                                        apply = Some(if !more && popup.checked == all { Vec::new() } else { chosen });
                                    }
                                }
                            }
                        });
                        if more {
                            ui.label(egui::RichText::new("More values than are listed: search to find others.").size(theme::TEXT_LABEL).color(tk.muted));
                        }
                        if popup.checked.is_empty() {
                            ui.label(egui::RichText::new("Nothing ticked. Tick at least one value, or tick All.").size(theme::TEXT_LABEL).color(tk.danger));
                        }
                    }
                }
                if ui.button("Done").clicked() {
                    close = true;
                }
            });
        });
        self.pivot_bar.rects.popup = area.response.rect;
        let pressed_outside = ctx.input(|i| i.pointer.any_pressed()) && ctx.input(|i| i.pointer.interact_pos()).map(|p| !area.response.rect.contains(p) && !anchor.contains(p)).unwrap_or(false);
        if pressed_outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        let field = popup.field.clone();
        self.pivot_bar.popup = if close { None } else { Some(popup) };
        if let Some(values) = apply {
            self.set_pivot_filter_values(&field, values);
        }
    }
}
