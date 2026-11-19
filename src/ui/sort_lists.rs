//! The custom sort-list manager (Phase 6 U3): View ▸ "Custom sort lists". Lists live in
//! `portable_data/sort_lists/<name>.json`; the dialog creates, renames, edits and deletes them,
//! and "Fill from column" loads the active tab's distinct values in natural order.

use crate::ui::prelude::*;
use micro_parquet::sort_lists::{delete_list, load_lists, rename_list, save_list, sort_lists_dir, validate_list, SortList};

/// What the dialog drew, for the render tests.
#[derive(Clone, Debug)]
pub struct SortListRects {
    pub list: egui::Rect,
    pub list_rows: Vec<(String, egui::Rect)>,
    pub editor: egui::Rect,
    pub value_rows: Vec<egui::Rect>,
    /// (name, rect) of the fields and the row buttons of the first value row.
    pub fields: Vec<(String, egui::Rect)>,
    pub buttons: Vec<(String, egui::Rect)>,
    pub message: egui::Rect,
}

impl Default for SortListRects {
    fn default() -> Self {
        let n = egui::Rect::NOTHING;
        Self { list: n, list_rows: Vec::new(), editor: n, value_rows: Vec::new(), fields: Vec::new(), buttons: Vec::new(), message: n }
    }
}

/// The list being edited, as typed.
#[derive(Clone, Debug, Default)]
pub struct ListEditor {
    pub name: String,
    pub values: Vec<String>,
    pub case_insensitive: bool,
    /// Column names, separated by commas or semicolons.
    pub columns: String,
    /// The name of the saved list this was opened from (`None` for a new list).
    pub opened_from: Option<String>,
}

#[derive(Default)]
pub struct SortListsState {
    /// The lists of the folder, read when the dialog opens or after a change.
    pub lists: Vec<SortList>,
    pub problems: Vec<(String, String)>,
    pub editor: ListEditor,
    /// Lines typed in the "Add several" box.
    pub add_text: String,
    /// The column "Fill from column" reads.
    pub fill_column: String,
    /// A fill waiting for the worker: (tab id, column).
    pub fill_wait: Option<(u64, String)>,
    /// A count of the values that are not tenors, waiting for the worker: (tab id, column).
    pub tenor_wait: Option<(u64, String)>,
    pub message: Option<(bool, String)>,
    pub confirm_delete: bool,
    pub confirm_overwrite: bool,
    pub rects: SortListRects,
}

/// The column names typed in the editor.
pub fn split_columns(text: &str) -> Vec<String> {
    text.split([',', ';']).map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect()
}

impl SortListsState {
    pub fn reload(&mut self) {
        let (lists, problems) = load_lists(&sort_lists_dir());
        self.lists = lists;
        self.problems = problems;
    }

    pub fn new_list(&mut self) {
        self.editor = ListEditor { case_insensitive: true, ..Default::default() };
        self.add_text.clear();
        self.message = None;
        self.confirm_delete = false;
        self.confirm_overwrite = false;
    }

    pub fn open(&mut self, name: &str) {
        if let Some(l) = self.lists.iter().find(|l| l.name == name) {
            self.editor = ListEditor { name: l.name.clone(), values: l.values.clone(), case_insensitive: l.case_insensitive, columns: l.apply_to_columns.join(", "), opened_from: Some(l.name.clone()) };
            self.message = None;
            self.confirm_delete = false;
            self.confirm_overwrite = false;
        }
    }

    /// The list as typed.
    pub fn typed(&self) -> SortList {
        SortList {
            name: self.editor.name.trim().to_string(),
            values: self.editor.values.clone(),
            case_insensitive: self.editor.case_insensitive,
            apply_to_columns: split_columns(&self.editor.columns),
        }
    }

    /// Why the list cannot be saved yet; `None` when it can.
    pub fn problem(&self) -> Option<String> {
        let typed = self.typed();
        if let Some(p) = validate_list(&typed) {
            return Some(p);
        }
        let taken = self.lists.iter().any(|l| l.name.to_lowercase() == typed.name.to_lowercase() && Some(&l.name) != self.editor.opened_from.as_ref());
        if taken {
            return Some(format!("A list called '{}' already exists. Select it in the list to change it, or choose another name.", typed.name));
        }
        None
    }

    /// Adds lines (one value per line) to the end of the values.
    pub fn add_lines(&mut self, text: &str) -> usize {
        let mut n = 0;
        for line in text.lines() {
            let t = line.trim();
            if !t.is_empty() {
                self.editor.values.push(t.to_string());
                n += 1;
            }
        }
        n
    }

    /// Saves the list as typed. A change to a stored list of the same name is asked for first.
    pub fn save(&mut self, confirmed: bool) -> Result<(), String> {
        if let Some(p) = self.problem() {
            return Err(p);
        }
        let typed = self.typed();
        let changes_stored = self.editor.opened_from.as_deref() == Some(typed.name.as_str());
        if changes_stored && !confirmed {
            if self.lists.iter().any(|l| *l == micro_parquet::sort_lists::cleaned(&typed)) {
                self.message = Some((false, "Nothing changed, so nothing was saved.".to_string()));
                return Ok(());
            }
            self.confirm_overwrite = true;
            return Ok(());
        }
        // a new name for an opened list saves a copy; "Rename" moves it
        save_list(&sort_lists_dir(), &typed).map_err(|e| format!("{:#}", e))?;
        self.confirm_overwrite = false;
        self.reload();
        self.editor.opened_from = Some(typed.name.clone());
        self.message = Some((false, format!("Saved the list '{}' ({} values).", typed.name, typed.values.iter().filter(|v| !v.trim().is_empty()).count())));
        Ok(())
    }

    /// Renames the opened list to the typed name (its values are kept as saved).
    pub fn rename(&mut self) -> Result<(), String> {
        let Some(old) = self.editor.opened_from.clone() else {
            return Err("Select a saved list to rename.".to_string());
        };
        let new = self.editor.name.trim().to_string();
        if let Some(p) = validate_list(&SortList { name: new.clone(), values: vec!["x".into()], case_insensitive: true, apply_to_columns: vec![] }) {
            return Err(p);
        }
        rename_list(&sort_lists_dir(), &old, &new).map_err(|e| format!("{:#}", e))?;
        self.reload();
        self.editor.opened_from = Some(new.clone());
        self.message = Some((false, format!("Renamed '{}' to '{}'.", old, new)));
        Ok(())
    }

    pub fn delete_opened(&mut self) -> Result<(), String> {
        let Some(name) = self.editor.opened_from.clone() else {
            return Err("Select a saved list to delete.".to_string());
        };
        delete_list(&sort_lists_dir(), &name).map_err(|e| format!("{:#}", e))?;
        self.reload();
        self.new_list();
        self.message = Some((false, format!("Deleted the list '{}'.", name)));
        Ok(())
    }

    /// The values of a column from the worker, in natural order, become the list's values.
    pub fn fill_with(&mut self, values: Vec<Option<String>>, more: bool) {
        let texts: Vec<String> = values.into_iter().flatten().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect();
        let mut seen = std::collections::HashSet::new();
        let texts: Vec<String> = texts.into_iter().filter(|v| seen.insert(v.to_lowercase())).collect();
        let n = texts.len();
        self.editor.values = texts;
        self.message = Some((
            false,
            if more {
                format!("Loaded the first {} values of '{}' in natural order (the column has more). Reorder them, then save.", n, self.fill_column)
            } else {
                format!("Loaded {} values of '{}' in natural order. Reorder them, then save.", n, self.fill_column)
            },
        ));
    }
}

impl MicroParquetApp {
    pub(crate) fn open_sort_lists(&mut self) {
        self.sort_lists.reload();
        self.sort_lists.new_list();
        if let Some(c) = self.tabs.get(self.active_tab).and_then(|t| t.available_columns().into_iter().next()) {
            if self.sort_lists.fill_column.is_empty() {
                self.sort_lists.fill_column = c;
            }
        }
        match self.sort_lists.lists.first().map(|l| l.name.clone()) {
            Some(n) => self.sort_lists.open(&n),
            None => {}
        }
        self.active_tool = ActiveToolModal::SortLists;
    }

    /// A finished job that lists the values of a field: true when "Fill from column" asked for it.
    pub(crate) fn apply_sort_list_fill(&mut self, r: &micro_parquet::tab_jobs::PivotValuesResult) -> bool {
        if let Some((tab_id, column)) = self.sort_lists.tenor_wait.clone() {
            if tab_id == r.tab_id && column == r.field {
                self.sort_lists.tenor_wait = None;
                if let Ok((values, more)) = &r.values {
                    let n = micro_parquet::sort_lists::count_unparsed_tenors(values.iter().flatten().map(|v| v.as_str()));
                    let suffix = if *more { " (counted in the first values of the column)" } else { "" };
                    self.status_message = if n == 0 {
                        format!("Sorted '{}' by tenor: every value is a tenor.", column)
                    } else {
                        format!("Sorted '{}' by tenor: {} value(s) are not tenors and follow the others in natural order{}.", column, n, suffix)
                    };
                }
                return true;
            }
        }
        let Some((tab_id, column)) = self.sort_lists.fill_wait.clone() else {
            return false;
        };
        if tab_id != r.tab_id || column != r.field {
            return false;
        }
        self.sort_lists.fill_wait = None;
        match &r.values {
            Ok((values, more)) => self.sort_lists.fill_with(values.clone(), *more),
            Err(e) => self.sort_lists.message = Some((true, format!("Could not read the values of '{}': {}", column, e))),
        }
        true
    }

    /// Asks the worker for the distinct values of a column of the active tab.
    fn fill_sort_list_from_column(&mut self) {
        let column = self.sort_lists.fill_column.clone();
        let Some(tab) = self.tabs.get(self.active_tab) else {
            self.sort_lists.message = Some((true, "Open a file first: \"Fill from column\" reads the active tab.".to_string()));
            return;
        };
        if !tab.available_columns().contains(&column) {
            self.sort_lists.message = Some((true, "Choose a column of the active tab.".to_string()));
            return;
        }
        let req = PivotValuesRequest {
            tab_id: tab.id,
            source: tab.source.clone(),
            steps: tab.steps[..tab.exec_steps.min(tab.steps.len())].to_vec(),
            field: column.clone(),
            temp_dir: self.paths.temp_dir.clone(),
        };
        self.sort_lists.fill_wait = Some((tab.id, column));
        self.sort_lists.message = Some((false, "Reading the values on the worker\u{2026}".to_string()));
        let key = JobKey::tab(JobKind::PivotValues, req.tab_id);
        self.jobs.submit(key, "Values of a column", false, move |ctx| compute_pivot_values(req, ctx).map(|r| JobOutput::PivotValues(Box::new(r))));
    }

    pub(crate) fn render_sort_lists_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let tk = *theme::tokens(self.dark_mode);
        let title = format!("{}  Custom sort lists", theme::ph::SORT_ASCENDING);
        let mut rects = SortListRects::default();
        let problem = self.sort_lists.problem();
        let columns = self.tabs.get(self.active_tab).map(|t| t.available_columns()).unwrap_or_default();
        let mut action: Option<&'static str> = None;
        let mut open_name: Option<String> = None;
        theme::modal(&title).default_width(theme::CURVES_DIALOG_W).show(ctx, |d| {
            d.body(|ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = theme::PIVOT_GAP;
                    let list = ui.allocate_ui_with_layout(egui::vec2(theme::CURVES_LIST_W, theme::CURVES_BODY_MIN_H), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.label(egui::RichText::new("LISTS").size(theme::TEXT_LABEL).color(tk.muted).strong());
                        let selected = self.sort_lists.editor.opened_from.clone();
                        if self.sort_lists.lists.is_empty() {
                            ui.label(egui::RichText::new("No list yet. Make one on the right.").color(tk.muted));
                        }
                        for l in self.sort_lists.lists.clone() {
                            let label = format!("{}  \u{b7}  {} values", l.name, l.values.len());
                            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), theme::PIVOT_FIELD_ROW_H), egui::Sense::click());
                            let is_sel = selected.as_deref() == Some(l.name.as_str());
                            if is_sel || resp.hovered() {
                                ui.painter().rect_filled(rect, theme::RADIUS_SMALL, if is_sel { tk.accent_soft } else { tk.zebra });
                            }
                            ui.put(rect.shrink2(egui::vec2(theme::SPACE_S, 0.0)), egui::Label::new(egui::RichText::new(&label).color(tk.ink)).truncate().selectable(false));
                            rects.list_rows.push((l.name.clone(), rect));
                            if resp.clicked() {
                                open_name = Some(l.name.clone());
                            }
                        }
                        if !self.sort_lists.problems.is_empty() {
                            ui.add_space(theme::SPACE_S);
                            ui.label(egui::RichText::new("Files that cannot be used").size(theme::TEXT_LABEL).color(tk.danger).strong());
                            for (file, reason) in &self.sort_lists.problems.clone() {
                                ui.add(egui::Label::new(egui::RichText::new(format!("{}: {}", file, reason)).size(theme::TEXT_LABEL).color(tk.danger)).wrap());
                            }
                        }
                    });
                    rects.list = list.response.rect;
                    let w = ui.available_width();
                    let ed = ui.allocate_ui_with_layout(egui::vec2(w, theme::CURVES_BODY_MIN_H), egui::Layout::top_down(egui::Align::Min), |ui| {
                        egui::Grid::new("sort_list_fields").num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_S)).show(ui, |ui| {
                            ui.label("Name:");
                            let r = ui.add(egui::TextEdit::singleline(&mut self.sort_lists.editor.name).desired_width(theme::FIELD_W_SORT_LIST));
                            rects.fields.push(("Name".to_string(), r.rect));
                            ui.end_row();
                            ui.label("Suggested for columns:");
                            let r = ui.add(egui::TextEdit::singleline(&mut self.sort_lists.editor.columns).desired_width(theme::FIELD_W_SORT_LIST).hint_text("e.g. Term Period; Risk Grade"));
                            rects.fields.push(("Suggested for columns".to_string(), r.rect));
                            ui.end_row();
                            ui.label("");
                            let r = ui.checkbox(&mut self.sort_lists.editor.case_insensitive, "Ignore upper and lower case");
                            rects.fields.push(("Ignore case".to_string(), r.rect));
                            ui.end_row();
                        });
                        ui.add_space(theme::SPACE_S);
                        ui.label(egui::RichText::new("Values, first to last").size(theme::TEXT_LABEL).color(tk.muted));
                        let mut remove: Option<usize> = None;
                        let mut swap: Option<(usize, usize)> = None;
                        let n = self.sort_lists.editor.values.len();
                        egui::ScrollArea::vertical().id_salt("sort_list_values").max_height(theme::LIST_MAX_H_L).auto_shrink([false, true]).show(ui, |ui| {
                            for (i, v) in self.sort_lists.editor.values.iter_mut().enumerate() {
                                let row = ui.horizontal(|ui| {
                                    ui.label(theme::mono_semibold(format!("{}", i + 1)));
                                    let r = ui.add(egui::TextEdit::singleline(v).desired_width(theme::GAP_COMBO_W));
                                    if i == 0 {
                                        rects.fields.push(("Value 1".to_string(), r.rect));
                                    }
                                    let up = ui.add_enabled(i > 0, egui::Button::new(theme::ph::ARROW_UP)).on_hover_text("Move up");
                                    let down = ui.add_enabled(i + 1 < n, egui::Button::new(theme::ph::ARROW_DOWN)).on_hover_text("Move down");
                                    let x = theme::icon_button(ui, theme::ph::X, "Remove this value");
                                    if i == 1 || (i == 0 && n == 1) {
                                        rects.fields.push(("Move up".to_string(), up.rect));
                                    }
                                    if i == 0 {
                                        rects.fields.push(("Move down".to_string(), down.rect));
                                        rects.fields.push(("Remove value".to_string(), x.rect));
                                    }
                                    if up.clicked() {
                                        swap = Some((i, i - 1));
                                    }
                                    if down.clicked() {
                                        swap = Some((i, i + 1));
                                    }
                                    if x.clicked() {
                                        remove = Some(i);
                                    }
                                });
                                rects.value_rows.push(row.response.rect);
                            }
                        });
                        if let Some((a, b)) = swap {
                            self.sort_lists.editor.values.swap(a, b);
                        }
                        if let Some(i) = remove {
                            self.sort_lists.editor.values.remove(i);
                        }
                        ui.horizontal(|ui| {
                            if ui.button(format!("{}  Add value", theme::ph::PLUS)).clicked() {
                                self.sort_lists.editor.values.push(String::new());
                            }
                            ui.label("Fill from column:");
                            let current = if self.sort_lists.fill_column.is_empty() { "(no column)".to_string() } else { self.sort_lists.fill_column.clone() };
                            let c = egui::ComboBox::from_id_salt("sort_list_fill_col").width(theme::GAP_COMBO_W).selected_text(current).show_ui(ui, |ui| {
                                for c in &columns {
                                    ui.selectable_value(&mut self.sort_lists.fill_column, c.clone(), c);
                                }
                            });
                            rects.fields.push(("Fill column".to_string(), c.response.rect));
                            let b = ui.add_enabled(!columns.is_empty(), egui::Button::new("Fill from column"));
                            rects.buttons.push(("Fill from column".to_string(), b.rect));
                            if b.clicked() {
                                action = Some("fill");
                            }
                        });
                        ui.add_space(theme::SPACE_S);
                        ui.label(egui::RichText::new("Add several values, one per line").size(theme::TEXT_LABEL).color(tk.muted));
                        let r = ui.add(egui::TextEdit::multiline(&mut self.sort_lists.add_text).desired_rows(3).desired_width(theme::FIELD_W_SORT_LIST));
                        rects.fields.push(("Add lines".to_string(), r.rect));
                        let b = ui.add_enabled(!self.sort_lists.add_text.trim().is_empty(), egui::Button::new("Add these lines"));
                        rects.buttons.push(("Add these lines".to_string(), b.rect));
                        if b.clicked() {
                            action = Some("add_lines");
                        }
                        ui.add_space(theme::SPACE_S);
                        let msg = ui.scope(|ui| {
                            ui.set_max_width(ui.available_width());
                            match (&problem, &self.sort_lists.message) {
                                (_, Some((true, m))) => {
                                    ui.add(egui::Label::new(egui::RichText::new(m).color(tk.danger)).wrap());
                                }
                                (Some(p), _) => {
                                    ui.add(egui::Label::new(egui::RichText::new(p).color(tk.danger)).wrap());
                                }
                                (None, Some((false, m))) => {
                                    ui.add(egui::Label::new(egui::RichText::new(m).color(tk.muted)).wrap());
                                }
                                (None, None) if self.sort_lists.confirm_overwrite => {
                                    ui.add(egui::Label::new(egui::RichText::new("Replace the saved list with these values? Sorts and pivots that already use it keep their own copy.").color(tk.warning)).wrap());
                                }
                                (None, None) => {
                                    ui.add(egui::Label::new(egui::RichText::new("Ready to save.").color(tk.muted)).wrap());
                                }
                            }
                        });
                        rects.message = msg.response.rect;
                    });
                    rects.editor = ed.response.rect;
                });
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let mut add = |ui: &mut egui::Ui, name: &str, enabled: bool, primary: bool, id: &'static str| {
                        let r = ui.add_enabled_ui(enabled, |ui| if primary { theme::primary_button(ui, name) } else { ui.button(name) }).inner;
                        rects.buttons.push((name.to_string(), r.rect));
                        if r.clicked() {
                            action = Some(id);
                        }
                    };
                    if self.sort_lists.confirm_overwrite {
                        add(ui, "Yes, replace it", true, true, "overwrite_yes");
                        add(ui, "No", true, false, "overwrite_no");
                    } else {
                        add(ui, "Save list", problem.is_none(), true, "save");
                    }
                    add(ui, "New list", true, false, "new");
                    let can_rename = self.sort_lists.editor.opened_from.as_deref().is_some_and(|o| o != self.sort_lists.editor.name.trim()) && !self.sort_lists.editor.name.trim().is_empty();
                    add(ui, "Rename", can_rename, false, "rename");
                    if self.sort_lists.confirm_delete {
                        add(ui, "Yes, delete it", true, false, "delete_yes");
                        add(ui, "No", true, false, "delete_no");
                    } else {
                        add(ui, "Delete", self.sort_lists.editor.opened_from.is_some(), false, "delete");
                    }
                    add(ui, "Close", true, false, "close");
                });
            });
        });
        self.sort_lists.rects = rects;
        if let Some(n) = open_name {
            self.sort_lists.open(&n);
        }
        match action {
            Some("save") => {
                if let Err(e) = self.sort_lists.save(false) {
                    self.sort_lists.message = Some((true, e));
                } else if !self.sort_lists.confirm_overwrite {
                    self.status_message = self.sort_lists.message.as_ref().map(|m| m.1.clone()).unwrap_or_default();
                }
            }
            Some("overwrite_yes") => {
                if let Err(e) = self.sort_lists.save(true) {
                    self.sort_lists.message = Some((true, e));
                    self.sort_lists.confirm_overwrite = false;
                }
            }
            Some("overwrite_no") => self.sort_lists.confirm_overwrite = false,
            Some("new") => self.sort_lists.new_list(),
            Some("rename") => {
                if let Err(e) = self.sort_lists.rename() {
                    self.sort_lists.message = Some((true, e));
                }
            }
            Some("delete") => self.sort_lists.confirm_delete = true,
            Some("delete_no") => self.sort_lists.confirm_delete = false,
            Some("delete_yes") => {
                self.sort_lists.confirm_delete = false;
                if let Err(e) = self.sort_lists.delete_opened() {
                    self.sort_lists.message = Some((true, e));
                }
            }
            Some("fill") => self.fill_sort_list_from_column(),
            Some("add_lines") => {
                let text = std::mem::take(&mut self.sort_lists.add_text);
                let n = self.sort_lists.add_lines(&text);
                self.sort_lists.message = Some((false, format!("Added {} values at the end.", n)));
            }
            Some("close") => close = true,
            _ => {}
        }
        ModalOutcome { close, step: None }
    }
}
