//! Screen module `filter`: the multi-condition filter editor (Phase 3 T1).

use crate::ui::prelude::*;

/// State of the filter editor.
pub struct FilterState {
    /// A default column for new conditions (kept in step with the active tab's columns).
    pub col: String,
    /// The filter step being edited; `None` while a new filter is being built.
    pub editing: Option<usize>,
    pub conditions: Vec<FilterCondition>,
    pub join: GroupJoin,
    /// true = keep matching rows, false = delete them.
    pub keep: bool,
    /// Where the value pickers' buttons were drawn (tests).
    pub pick_buttons: Vec<(usize, egui::Rect)>,
    /// Where the "By" boxes of date columns were drawn (W2, tests).
    pub by_boxes: Vec<(usize, egui::Rect)>,
}

impl Default for FilterState {
    fn default() -> Self {
        Self {
            col: String::new(),
            editing: None,
            conditions: Vec::new(),
            join: GroupJoin::And,
            keep: true,
            pick_buttons: Vec::new(),
            by_boxes: Vec::new(),
        }
    }
}

/// Why the filter cannot be applied yet; `None` when it can.
fn filter_problem(conditions: &[FilterCondition]) -> Option<String> {
    if conditions.is_empty() {
        Some("Add at least one condition.".to_string())
    } else {
        conditions.iter().find_map(condition_problem)
    }
}

/// Why a condition cannot be applied yet, in words for the user; `None` when it is complete.
pub fn condition_problem(c: &FilterCondition) -> Option<String> {
    if c.column.trim().is_empty() {
        return Some("Pick a column.".to_string());
    }
    if c.op.is_unary() {
        return None;
    }
    if is_pick_op(c.op) {
        return if c.values.is_empty() && !c.or_blank && c.value.trim().is_empty() { Some(format!("{}: pick at least one value.", c.column)) } else { None };
    }
    if c.value.trim().is_empty() {
        return Some(format!("{}: enter a value.", c.column));
    }
    if c.op == CondOp::Between && c.value2.trim().is_empty() {
        return Some(format!("{}: enter the upper limit of the range.", c.column));
    }
    None
}

/// The operators whose value is picked from the column's list.
pub fn is_pick_op(op: CondOp) -> bool {
    matches!(op, CondOp::Equals | CondOp::NotEquals | CondOp::InList | CondOp::NotInList)
}

/// What the condition has picked: its values, or (an old filter) the text typed in the value box.
pub fn picker_selection(c: &FilterCondition) -> Vec<String> {
    if !c.values.is_empty() {
        return c.values.clone();
    }
    let t = c.value.trim();
    if t.is_empty() {
        Vec::new()
    } else if matches!(c.op, CondOp::InList | CondOp::NotInList) {
        t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    } else {
        vec![t.to_string()]
    }
}

/// After a change in the picker: one value is "equals", several are "is in list" (and "does not
/// equal" / "is not in list" for the opposite); `value` keeps a short summary.
pub fn normalize_pick(c: &mut FilterCondition) {
    let negative = matches!(c.op, CondOp::NotEquals | CondOp::NotInList);
    c.op = match (negative, c.values.len()) {
        (false, 0 | 1) => CondOp::Equals,
        (false, _) => CondOp::InList,
        (true, 0 | 1) => CondOp::NotEquals,
        (true, _) => CondOp::NotInList,
    };
    c.value = c.values.join(", ");
}

impl MicroParquetApp {
    fn blank_condition(&self) -> FilterCondition {
        FilterCondition {
            column: self.filter.col.clone(),
            op: CondOp::Equals,
            value: String::new(),
            value2: String::new(),
            ..Default::default()
        }
    }

    /// Index of the active tab's first filter step (a group, or an old single filter).
    pub(crate) fn first_filter_step(&self) -> Option<usize> {
        self.tabs
            .get(self.active_tab)?
            .steps
            .iter()
            .position(|s| s.as_filter_group().is_some())
    }

    /// Opens the editor on step `idx` (an old single filter shows as a one-condition group), or
    /// on a new filter. `add_blank` appends an empty condition ("+ Add condition").
    pub(crate) fn open_filter_editor(&mut self, idx: Option<usize>, add_blank: bool) {
        self.init_tool_defaults_from_active_tab();
        self.picker = PickerPanel::default();
        self.sort_lists.reload();
        let loaded = idx.and_then(|i| {
            self.tabs
                .get(self.active_tab)
                .and_then(|t| t.steps.get(i))
                .and_then(|s| s.as_filter_group())
        });
        match loaded {
            Some((conditions, join, keep)) => {
                self.filter.editing = idx;
                self.filter.conditions = conditions;
                self.filter.join = join;
                self.filter.keep = keep;
            }
            None => {
                self.filter.editing = None;
                self.filter.conditions = Vec::new();
                self.filter.join = GroupJoin::And;
                self.filter.keep = true;
            }
        }
        if add_blank || self.filter.conditions.is_empty() {
            let c = self.blank_condition();
            self.filter.conditions.push(c);
        }
        self.active_tool = ActiveToolModal::FilterRows;
    }

    /// "+ Add condition": into the existing filter, or a new one when there is none.
    pub(crate) fn open_filter_editor_to_add(&mut self) {
        let idx = self.first_filter_step();
        self.open_filter_editor(idx, true);
    }

    /// The filter button: edit the tab's filter, or start one.
    pub(crate) fn open_filter_editor_default(&mut self) {
        let idx = self.first_filter_step();
        self.open_filter_editor(idx, false);
    }

    /// Opens a new condition on `column` ("Filter on this column...", used by the inspector).
    pub(crate) fn open_filter_editor_for_column(&mut self, column: &str) {
        self.filter.col = column.to_string();
        let idx = self.first_filter_step();
        self.open_filter_editor(idx, true);
        if let Some(c) = self.filter.conditions.last_mut() {
            c.column = column.to_string();
        }
    }

    /// Removes one condition (and the whole step when it was the last one).
    pub(crate) fn remove_filter_condition(&mut self, step_idx: usize, cond_idx: usize) {
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        let Some((mut conds, join, keep)) = tab.steps.get(step_idx).and_then(|s| s.as_filter_group()) else {
            return;
        };
        if cond_idx >= conds.len() {
            return;
        }
        tab.record_undo();
        conds.remove(cond_idx);
        if conds.is_empty() {
            tab.steps.remove(step_idx);
        } else if let Some(slot) = tab.steps.get_mut(step_idx) {
            *slot = TransformStep::FilterGroup { conditions: conds, join, keep_matching: keep };
        }
        tab.viewport_offset = 0;
        self.status_message = "Filter condition removed.".to_string();
        self.refresh_active_tab(false);
    }

    #[allow(unused_variables)]
    pub(crate) fn render_filter_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        let mut replace: Option<(usize, TransformStep)> = None;
        let editing = self.filter.editing;
        let tk = *theme::tokens(self.dark_mode);
        let mut picker = std::mem::take(&mut self.picker);
        let totals = self.picker_totals.clone();
        let sort_lists = self.sort_lists.lists.clone();
        let calendar = self.calendar();
        // the date columns of the tab: their value picker has a "By" choice (W2)
        let date_cols: Vec<String> = self.tabs.get(self.active_tab).map(|t| t.cached_slice.get_columns().iter().filter(|c| micro_parquet::dates::is_date_type(c.dtype())).map(|c| c.name().to_string()).collect()).unwrap_or_default();
        let mut by_rects: Vec<(usize, egui::Rect)> = Vec::new();
        let mut ask_values: Option<(String, Option<String>)> = None;
        let mut picker_buttons_seen: Vec<(usize, egui::Rect)> = Vec::new();
        let title = if editing.is_some() {
            "\u{E30C}  Edit filter"
        } else {
            "\u{E30C}  Filter rows"
        };
        theme::modal(title).default_width(theme::DIALOG_W_FILTER).show(ctx, |d| {
            d.body(|ui| {
                let mut remove: Option<usize> = None;
                for (i, c) in self.filter.conditions.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if i > 0 {
                            ui.label(egui::RichText::new(self.filter.join.label()).color(theme::muted(ui)));
                        }
                        egui::ComboBox::from_id_salt(("flt_col", i))
                            .width(theme::FIELD_W_COLUMN)
                            .selected_text(&c.column)
                            .show_ui(ui, |ui| {
                                for name in &avail_cols {
                                    ui.selectable_value(&mut c.column, name.clone(), name);
                                }
                            });
                        egui::ComboBox::from_id_salt(("flt_op", i))
                            .width(theme::FIELD_W_OP)
                            .selected_text(c.op.label())
                            .show_ui(ui, |ui| {
                                for op in CondOp::all() {
                                    ui.selectable_value(&mut c.op, *op, op.label());
                                }
                            });
                        // a part of a column that is no longer the condition's column is dropped (W2)
                        if c.part.as_ref().is_some_and(|p| p.column != c.column) {
                            c.part = None;
                        }
                        // "By": Date (the list of dates) or a part of it, for a date column (W2)
                        if is_pick_op(c.op) && date_cols.contains(&c.column) {
                            let shown = c.part.as_ref().map(|p| p.part.label()).unwrap_or("Date");
                            let before = c.part.clone();
                            let r = egui::ComboBox::from_id_salt(("flt_by", i)).width(theme::FIELD_W_OP).selected_text(format!("By {}", shown)).show_ui(ui, |ui| {
                                if ui.selectable_label(c.part.is_none(), "Date").clicked() {
                                    c.part = None;
                                }
                                for part in micro_parquet::dates::DatePart::FILTER_BY {
                                    if ui.selectable_label(c.part.as_ref().is_some_and(|p| p.part == part), part.label()).clicked() {
                                        c.part = Some(micro_parquet::dates::DateFieldRef::new(&c.column, part, calendar));
                                    }
                                }
                            });
                            by_rects.push((i, r.response.rect));
                            if c.part.as_ref().map(|p| p.part) != before.as_ref().map(|p| p.part) {
                                // values of one kind are not values of another
                                c.values.clear();
                                c.or_blank = false;
                                c.value.clear();
                                if picker.owner == Some(i) {
                                    picker.owner = None;
                                }
                            }
                        } else if c.part.is_some() && !is_pick_op(c.op) {
                            c.part = None;
                        }
                        let pick_field = c.part.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| c.column.clone());
                        if picker.owner == Some(i) && picker.field != pick_field {
                            picker.owner = None;
                        }
                        if is_pick_op(c.op) {
                            let n = picker_selection(c).len();
                            let label = format!("{}  \u{25be}", micro_parquet::value_picker::picker_summary(n, c.or_blank, totals.get(&pick_field).copied()));
                            let b = ui.add(egui::Button::new(label).min_size(egui::vec2(theme::FIELD_W_PICK, 0.0)));
                            picker_buttons_seen.push((i, b.rect));
                            if b.clicked() && picker.toggle(i, &pick_field) {
                                picker.part = c.part.clone();
                                ask_values = Some((pick_field.clone(), None));
                            }
                        } else if !c.op.is_unary() {
                            ui.add(egui::TextEdit::singleline(&mut c.value).desired_width(theme::FIELD_W_VALUE).hint_text("value"));
                            if c.op == CondOp::Between {
                                ui.label("and");
                                ui.add(egui::TextEdit::singleline(&mut c.value2).desired_width(theme::FIELD_W_VALUE).hint_text("upper limit"));
                            }
                        }
                        if theme::icon_button(ui, theme::ph::X, "Remove this condition").clicked() {
                            remove = Some(i);
                        }
                    });
                    // the picker of this condition, open under its row
                    if picker.owner == Some(i) && is_pick_op(c.op) {
                        let mut values = picker_selection(c);
                        let mut blank = c.or_blank;
                        let order = match &c.part {
                            Some(p) => p.sort_order().unwrap_or_default(),
                            None => crate::ui::value_picker::picker_order(&sort_lists, &c.column),
                        };
                        picker.calendar = calendar;
                        let out = crate::ui::value_picker::picker_body(ui, &tk, &mut picker, &mut values, &mut blank, true, &order);
                        if out.changed {
                            c.values = values;
                            c.or_blank = blank;
                            normalize_pick(c);
                        }
                        if let Some(req) = out.request {
                            ask_values = Some((picker.field.clone(), req));
                        }
                    }
                }
                if let Some(i) = remove {
                    self.filter.conditions.remove(i);
                }
                if ui.button(format!("{}  Add condition", theme::ph::PLUS)).clicked() {
                    let c = FilterCondition {
                        column: self.filter.col.clone(),
                        op: CondOp::Equals,
                        value: String::new(),
                        value2: String::new(),
                        ..Default::default()
                    };
                    self.filter.conditions.push(c);
                }
                ui.add_space(theme::SPACE_S);
                ui.horizontal(|ui| {
                    ui.label("Rows must meet");
                    ui.radio_value(&mut self.filter.join, GroupJoin::And, "all conditions (and)");
                    ui.radio_value(&mut self.filter.join, GroupJoin::Or, "any condition (or)");
                });
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.filter.keep, true, "Keep matching rows");
                    ui.radio_value(&mut self.filter.keep, false, "Delete matching rows");
                });
                let problem = filter_problem(&self.filter.conditions);
                if let Some(p) = &problem {
                    theme::warning_label(ui, p);
                }
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let problem = filter_problem(&self.filter.conditions);
                    let step = TransformStep::FilterGroup {
                        conditions: self.filter.conditions.clone(),
                        join: self.filter.join,
                        keep_matching: self.filter.keep,
                    };
                    let apply_label = if editing.is_some() { "\u{E182}  Save filter" } else { "\u{E182}  Apply to current tab" };
                    let apply = ui.add_enabled_ui(problem.is_none(), |ui| theme::primary_button(ui, apply_label)).inner;
                    if apply.clicked() {
                        match editing {
                            Some(i) => replace = Some((i, step.clone())),
                            None => step_to_apply = Some((step.clone(), false)),
                        }
                        close_modal = true;
                    }
                    if editing.is_none() {
                        let newtab = ui.add_enabled(problem.is_none(), egui::Button::new("\u{E3D4}  Open filtered view in new tab"));
                        if newtab.clicked() {
                            step_to_apply = Some((step, true));
                            close_modal = true;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close_modal = true;
                    }
                });
            });
        });
        self.picker = picker;
        self.filter.pick_buttons = picker_buttons_seen;
        self.filter.by_boxes = by_rects;
        if let Some((field, search)) = ask_values {
            self.picker.field = field;
            self.picker_request(search);
        }
        if let Some((i, step)) = replace {
            self.replace_step_in_active_tab(i, step, "Filter updated.");
        }
        ModalOutcome { close: close_modal, step: step_to_apply }
    }
}
