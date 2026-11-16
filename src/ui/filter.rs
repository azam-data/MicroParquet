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
}

impl Default for FilterState {
    fn default() -> Self {
        Self {
            col: String::new(),
            editing: None,
            conditions: Vec::new(),
            join: GroupJoin::And,
            keep: true,
        }
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
    if c.value.trim().is_empty() {
        return Some(format!("{}: enter a value.", c.column));
    }
    if c.op == CondOp::Between && c.value2.trim().is_empty() {
        return Some(format!("{}: enter the upper limit of the range.", c.column));
    }
    None
}

impl MicroParquetApp {
    fn blank_condition(&self) -> FilterCondition {
        FilterCondition {
            column: self.filter.col.clone(),
            op: CondOp::Equals,
            value: String::new(),
            value2: String::new(),
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
        tab.history.record(&tab.steps);
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
        let title = if editing.is_some() {
            "\u{E30C}  Edit filter"
        } else {
            "\u{E30C}  Filter rows"
        };
        theme::modal(title).default_width(640.0).show(ctx, |ui| {
            let mut remove: Option<usize> = None;
            for (i, c) in self.filter.conditions.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if i > 0 {
                        ui.label(egui::RichText::new(self.filter.join.label()).color(theme::muted(ui)));
                    }
                    egui::ComboBox::from_id_salt(("flt_col", i))
                        .width(160.0)
                        .selected_text(&c.column)
                        .show_ui(ui, |ui| {
                            for name in &avail_cols {
                                ui.selectable_value(&mut c.column, name.clone(), name);
                            }
                        });
                    egui::ComboBox::from_id_salt(("flt_op", i))
                        .width(120.0)
                        .selected_text(c.op.label())
                        .show_ui(ui, |ui| {
                            for op in CondOp::all() {
                                ui.selectable_value(&mut c.op, *op, op.label());
                            }
                        });
                    if !c.op.is_unary() {
                        ui.add(egui::TextEdit::singleline(&mut c.value).desired_width(130.0).hint_text(
                            match c.op {
                                CondOp::InList => "a, b, c",
                                _ => "value",
                            },
                        ));
                        if c.op == CondOp::Between {
                            ui.label("and");
                            ui.add(egui::TextEdit::singleline(&mut c.value2).desired_width(130.0).hint_text("upper limit"));
                        }
                    }
                    if theme::icon_button(ui, theme::ph::X, "Remove this condition").clicked() {
                        remove = Some(i);
                    }
                });
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
            let problem = if self.filter.conditions.is_empty() {
                Some("Add at least one condition.".to_string())
            } else {
                self.filter.conditions.iter().find_map(condition_problem)
            };
            if let Some(p) = &problem {
                theme::warning_label(ui, p);
            }
            ui.separator();
            ui.horizontal(|ui| {
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
        if let Some((i, step)) = replace {
            self.replace_step_in_active_tab(i, step, "Filter updated.");
        }
        ModalOutcome { close: close_modal, step: step_to_apply }
    }
}
