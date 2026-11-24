//! Screen module `sort`: the multi-level sort editor (Phase 3 T2).

use crate::ui::prelude::*;

/// State of the sort editor.
#[derive(Default)]
pub struct SortState {
    /// The sort step being edited; `None` while a new sort is being built.
    pub editing: Option<usize>,
    pub levels: Vec<SortLevel>,
}

/// Why the sort cannot be applied yet; `None` when it can.
fn sort_problem(levels: &[SortLevel]) -> Option<String> {
    if levels.is_empty() {
        return Some("Add at least one level.".to_string());
    }
    let mut seen = std::collections::HashSet::new();
    levels
        .iter()
        .find(|l| !seen.insert(l.column.clone()))
        .map(|l| format!("{} is used twice: a column can be a level once.", l.column))
}

impl MicroParquetApp {
    fn last_sort_step(&self) -> Option<usize> {
        self.tabs
            .get(self.active_tab)?
            .steps
            .iter()
            .rposition(|s| s.sort_levels().is_some())
    }

    /// Opens the sort editor on the tab's sort (the last sort step), or on a new one. `column`
    /// adds that column as a new level.
    pub(crate) fn open_sort_editor(&mut self, column: Option<&str>) {
        let idx = self.last_sort_step();
        self.open_sort_editor_at(idx, column);
    }

    /// Opens the sort editor on the sort step `idx` (or a new sort when `None`).
    pub(crate) fn open_sort_editor_at(&mut self, idx: Option<usize>, column: Option<&str>) {
        self.init_tool_defaults_from_active_tab();
        self.sort_lists.reload();
        let existing = idx.and_then(|i| {
            self.tabs
                .get(self.active_tab)
                .and_then(|t| t.steps.get(i))
                .and_then(|s| s.sort_levels().map(|l| l.to_vec()))
        });
        self.sort.editing = idx.filter(|_| existing.is_some());
        self.sort.levels = existing.unwrap_or_default();
        let wanted = column.map(str::to_string).or_else(|| {
            if self.sort.levels.is_empty() {
                self.tabs.get(self.active_tab).and_then(|t| t.available_columns().into_iter().next())
            } else {
                None
            }
        });
        if let Some(c) = wanted {
            if !self.sort.levels.iter().any(|l| l.column == c) {
                self.sort.levels.push(SortLevel { column: c, descending: false, ..Default::default() });
            }
        }
        self.active_tool = ActiveToolModal::SortRows;
    }

    /// Sort straight from a column header: replaces the sort with this column, or (with `add`)
    /// adds it as the next level; asking for the same direction again removes it.
    pub(crate) fn sort_from_header(&mut self, column: &str, descending: bool, add: bool) {
        let idx = self.last_sort_step();
        let mut levels: Vec<SortLevel> = idx
            .and_then(|i| self.tabs.get(self.active_tab).and_then(|t| t.steps.get(i)).and_then(|s| s.sort_levels().map(|l| l.to_vec())))
            .unwrap_or_default();
        if add {
            match levels.iter_mut().find(|l| l.column == column) {
                Some(l) => l.descending = descending,
                None => levels.push(SortLevel { column: column.to_string(), descending, ..Default::default() }),
            }
        } else {
            levels = vec![SortLevel { column: column.to_string(), descending, ..Default::default() }];
        }
        self.apply_sort_levels(idx, levels);
    }

    /// Removes the tab's sort.
    pub(crate) fn clear_sort(&mut self) {
        if let Some(i) = self.last_sort_step() {
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                tab.record_undo();
                tab.steps.remove(i);
                tab.viewport_offset = 0;
            }
            self.status_message = "Sort removed.".to_string();
            self.refresh_active_tab(false);
        }
    }

    fn apply_sort_levels(&mut self, existing: Option<usize>, levels: Vec<SortLevel>) {
        let tenor_column = levels.iter().find(|l| l.order == SortOrder::Tenor).map(|l| l.column.clone());
        let step = TransformStep::SortRows { levels };
        match existing {
            Some(i) => self.replace_step_in_active_tab(i, step, "Sort updated."),
            None => self.push_step_to_active_tab(step, false),
        }
        if let Some(column) = tenor_column {
            self.count_unparsed_tenors(&column);
        }
    }

    /// After a Tenor sort: the worker lists the column's values, and the status message says how
    /// many are not tenors (they follow the others in natural order).
    fn count_unparsed_tenors(&mut self, column: &str) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let req = PivotValuesRequest { tab_id: tab.id, source: tab.source.clone(), steps: tab.steps.clone(), field: column.to_string(), temp_dir: self.paths.temp_dir.clone() };
        self.sort_lists.tenor_wait = Some((tab.id, column.to_string()));
        let key = JobKey::tab(JobKind::PivotValues, tab.id);
        self.jobs.submit(key, "Values of a column", false, move |ctx| compute_pivot_values(req, ctx).map(|r| JobOutput::PivotValues(Box::new(r))));
    }

    #[allow(unused_variables)]
    pub(crate) fn render_sort_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut apply = false;
        let mut remove_sort = false;
        let editing = self.sort.editing;
        let saved_lists = self.sort_lists.lists.clone();
        let builtin = self.builtin_orders();
        let before: Vec<String> = self.sort.levels.iter().map(|l| l.column.clone()).collect();
        theme::modal(if editing.is_some() { "\u{E444}  Edit sort" } else { "\u{E444}  Sort rows" })
            .default_width(theme::DIALOG_W_SORT)
            .show(ctx, |d| {
                d.body(|ui| {
                    ui.label("Rows are ordered by level 1, then level 2 for ties, and so on. Blanks are always last.");
                    ui.add_space(theme::SPACE_S);
                    let mut remove: Option<usize> = None;
                    let mut swap: Option<(usize, usize)> = None;
                    let n = self.sort.levels.len();
                    for (i, l) in self.sort.levels.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(theme::mono_semibold(format!("{}", i + 1)));
                            egui::ComboBox::from_id_salt(("sort_col", i))
                                .width(theme::FIELD_W_SORT_COLUMN)
                                .selected_text(&l.column)
                                .show_ui(ui, |ui| {
                                    for c in &avail_cols {
                                        ui.selectable_value(&mut l.column, c.clone(), c);
                                    }
                                });
                            let order_text = l.order.label();
                            egui::ComboBox::from_id_salt(("sort_order", i)).width(theme::FIELD_W_SORT_ORDER).selected_text(order_text).show_ui(ui, |ui| {
                                ui.selectable_value(&mut l.order, SortOrder::Natural, "Natural");
                                for list in &saved_lists {
                                    let o = SortOrder::from_list(list);
                                    ui.selectable_value(&mut l.order, o, &list.name);
                                }
                                ui.selectable_value(&mut l.order, SortOrder::Tenor, "Tenor");
                                for o in &builtin {
                                    ui.selectable_value(&mut l.order, o.clone(), o.label());
                                }
                            });
                            ui.radio_value(&mut l.descending, false, "Lowest first");
                            ui.radio_value(&mut l.descending, true, "Highest first");
                            if ui.add_enabled(i > 0, egui::Button::new(theme::ph::ARROW_UP)).on_hover_text("Move up").clicked() {
                                swap = Some((i, i - 1));
                            }
                            if ui.add_enabled(i + 1 < n, egui::Button::new(theme::ph::ARROW_DOWN)).on_hover_text("Move down").clicked() {
                                swap = Some((i, i + 1));
                            }
                            if theme::icon_button(ui, theme::ph::X, "Remove this level").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some((a, b)) = swap {
                        self.sort.levels.swap(a, b);
                    }
                    if let Some(i) = remove {
                        self.sort.levels.remove(i);
                    }
                    if ui.button(format!("{}  Add level", theme::ph::PLUS)).clicked() {
                        let used: Vec<&str> = self.sort.levels.iter().map(|l| l.column.as_str()).collect();
                        let next = avail_cols.iter().find(|c| !used.contains(&c.as_str())).or(avail_cols.first()).cloned();
                        if let Some(column) = next {
                            self.sort.levels.push(SortLevel { column, descending: false, ..Default::default() });
                        }
                    }
                    let problem = sort_problem(&self.sort.levels);
                    if let Some(p) = &problem {
                        theme::warning_label(ui, p);
                    }
                });
                d.footer(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        let problem = sort_problem(&self.sort.levels);
                        let ok = ui
                            .add_enabled_ui(problem.is_none(), |ui| theme::primary_button(ui, "\u{E182}  Apply sort"))
                            .inner;
                        if ok.clicked() {
                            apply = true;
                        }
                        if editing.is_some() && theme::danger_button(ui, "Remove sort").clicked() {
                            remove_sort = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_modal = true;
                        }
                    });
                });
            });
        // W0 auto order: a level whose column was just picked (or added) and has no order chosen
        let changed: Vec<(usize, String)> = self
            .sort
            .levels
            .iter()
            .enumerate()
            .filter(|(i, l)| before.get(*i) != Some(&l.column) && !before.contains(&l.column) && l.order.is_natural())
            .map(|(i, l)| (i, l.column.clone()))
            .collect();
        for (i, column) in changed {
            if let Some(order) = self.auto_order(&column, crate::ui::auto_order::AutoTarget::SortLevel(i)) {
                if let Some(l) = self.sort.levels.get_mut(i) {
                    l.order = order;
                }
            }
        }
        if apply {
            let levels = self.sort.levels.clone();
            self.apply_sort_levels(editing, levels);
            close_modal = true;
        }
        if remove_sort {
            self.clear_sort();
            close_modal = true;
        }
        ModalOutcome { close: close_modal, step: None }
    }
}
