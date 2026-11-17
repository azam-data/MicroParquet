//! The pivot builder (Phase 4 V1), after `docs/design/Pivot.dc.html`: a searchable list of fields,
//! four areas (Filters, Columns, Rows, Values) that fields are dragged into or added to from a menu,
//! the totals and "show values as" settings, and a live preview of the first 20 rows that the
//! worker thread computes.

use crate::ui::prelude::*;
use micro_parquet::pivot_table::{pivot_column_formats, BLANK_LABEL};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

/// The four areas of the builder, in the order of the design.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Area {
    Filters,
    Columns,
    Rows,
    Values,
}

impl Area {
    pub const ALL: [Area; 4] = [Area::Filters, Area::Columns, Area::Rows, Area::Values];

    pub fn index(self) -> usize {
        match self {
            Area::Filters => 0,
            Area::Columns => 1,
            Area::Rows => 2,
            Area::Values => 3,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Area::Filters => "Filters",
            Area::Columns => "Columns",
            Area::Rows => "Rows",
            Area::Values => "Values",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Area::Filters => "limit the whole pivot",
            Area::Columns => "across the top",
            Area::Rows => "down the side",
            Area::Values => "one or more",
        }
    }

    /// The word in the field list for a field that is in this area.
    fn used_word(self) -> &'static str {
        match self {
            Area::Filters => "Filter",
            other => other.title(),
        }
    }
}

/// What the dialog drew, for the render tests.
#[derive(Clone, Debug)]
pub struct PivotRects {
    pub body: egui::Rect,
    pub fields: egui::Rect,
    pub fields_list: egui::Rect,
    pub right: egui::Rect,
    pub areas: [egui::Rect; 4],
    pub chips: Vec<(Area, String, egui::Rect)>,
    pub field_rows: Vec<(String, egui::Rect)>,
    /// The "Add to" menu button of each listed field.
    pub field_menus: Vec<(String, egui::Rect)>,
    pub toggles: egui::Rect,
    pub preview: egui::Rect,
    pub preview_scroll: egui::Rect,
    pub popup: egui::Rect,
    pub footer_text: egui::Rect,
    pub buttons: Vec<(String, egui::Rect)>,
}

impl Default for PivotRects {
    fn default() -> Self {
        let n = egui::Rect::NOTHING;
        Self {
            body: n,
            fields: n,
            fields_list: n,
            right: n,
            areas: [n; 4],
            chips: Vec::new(),
            field_rows: Vec::new(),
            field_menus: Vec::new(),
            toggles: n,
            preview: n,
            preview_scroll: n,
            popup: n,
            footer_text: n,
            buttons: Vec::new(),
        }
    }
}

/// The values of a field in the Filters checklist.
#[derive(Clone, Debug, Default)]
pub struct FilterPopup {
    pub field: String,
    pub search: String,
    /// `None` while loading.
    pub values: Option<Result<(Vec<Option<String>>, bool), String>>,
    pub checked: HashSet<Option<String>>,
}

/// The state of the builder.
#[derive(Default)]
pub struct PivotBuilderState {
    pub spec: PivotSpec,
    pub fields: Vec<(String, FieldKind)>,
    pub search: String,
    /// The settings the last preview job was submitted for.
    pub submitted: Option<u64>,
    /// The preview of those settings: the first rows, or why there is none.
    pub preview: Option<Result<DataFrame, String>>,
    pub preview_total_rows: usize,
    /// All columns of the last good preview (the label columns first), for the column formats.
    pub value_columns: Vec<String>,
    pub popup: Option<FilterPopup>,
    /// Make a new tab (false: the pivot is added to this tab, or an edited step is replaced).
    pub new_tab: bool,
    pub rects: PivotRects,
    pub open: bool,
}

fn fingerprint<T: std::fmt::Debug>(x: &T) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}", x).hash(&mut h);
    h.finish()
}

/// What the pivot's input is: which tab, the steps it sees, and a title for the dialog.
struct PivotInput {
    tab_id: u64,
    source: TabSource,
    steps: Vec<TransformStep>,
    tab_title: String,
    after_step: usize,
}

impl MicroParquetApp {
    /// Opens the builder for a new pivot (`from`: the settings of a step to edit, in place).
    pub(crate) fn open_pivot_builder(&mut self, from: Option<PivotSpec>, editing: bool) {
        let kinds: Vec<(String, FieldKind)> = self
            .tabs
            .get(self.active_tab)
            .map(|t| t.cached_slice.get_columns().iter().map(|c| (c.name().to_string(), FieldKind::of(c.dtype()))).collect())
            .unwrap_or_default();
        let b = &mut self.pivot_builder;
        b.spec = from.unwrap_or_else(|| PivotSpec { row_totals: true, column_totals: true, ..Default::default() });
        b.fields = kinds;
        b.search.clear();
        b.submitted = None;
        b.preview = None;
        b.preview_total_rows = 0;
        b.value_columns.clear();
        b.popup = None;
        b.new_tab = !editing;
        b.open = true;
    }

    /// The steps the pivot sees: while editing, those before the edited step; after "Insert after",
    /// those up to that step; else all the steps that run.
    fn pivot_input(&self) -> Option<PivotInput> {
        if let Some(ed) = &self.editing {
            let tab = self.tabs.iter().find(|t| t.id == ed.tab_id)?;
            let n = ed.index.min(tab.steps.len());
            return Some(PivotInput { tab_id: tab.id, source: tab.source.clone(), steps: tab.steps[..n].to_vec(), tab_title: tab.title.clone(), after_step: n });
        }
        let tab = self.tabs.get(self.active_tab)?;
        let n = match self.insert_after {
            Some(i) => (i + 1).min(tab.steps.len()),
            None => tab.exec_steps.min(tab.steps.len()),
        };
        Some(PivotInput { tab_id: tab.id, source: tab.source.clone(), steps: tab.steps[..n].to_vec(), tab_title: tab.title.clone(), after_step: n })
    }

    /// Sends the settings to the worker when they changed since the last preview.
    pub(crate) fn pivot_builder_tick(&mut self) {
        let Some(input) = self.pivot_input() else {
            return;
        };
        let token = fingerprint(&(&self.pivot_builder.spec, micro_parquet::steps_edit::pipeline_hash(&input.source, &input.steps)));
        if self.pivot_builder.submitted == Some(token) {
            return;
        }
        self.pivot_builder.submitted = Some(token);
        let req = PivotPreviewRequest {
            tab_id: input.tab_id,
            token,
            source: input.source,
            steps: input.steps,
            spec: self.pivot_builder.spec.clone(),
            temp_dir: self.paths.temp_dir.clone(),
        };
        let key = JobKey::tab(JobKind::PivotPreview, input.tab_id);
        self.jobs.submit(key, "Pivot preview", false, move |ctx| compute_pivot_preview(req, ctx).map(|r| JobOutput::PivotPreview(Box::new(r))));
    }

    /// A finished preview job.
    pub(crate) fn apply_pivot_preview(&mut self, r: micro_parquet::tab_jobs::PivotPreviewResult) {
        let b = &mut self.pivot_builder;
        if !b.open {
            return;
        }
        if !r.fields.is_empty() {
            b.fields = r.fields;
        }
        if b.submitted == Some(r.token) {
            if let Ok(df) = &r.table {
                b.value_columns = df.get_column_names().iter().map(|n| n.to_string()).collect();
            }
            b.preview = Some(r.table);
            b.preview_total_rows = r.total_rows;
        }
    }

    pub(crate) fn pivot_preview_failed(&mut self, msg: String) {
        if self.pivot_builder.open {
            self.pivot_builder.preview = Some(Err(msg));
        }
    }

    /// A finished job that lists the values of a field.
    pub(crate) fn apply_pivot_values(&mut self, r: micro_parquet::tab_jobs::PivotValuesResult) {
        let Some(p) = self.pivot_builder.popup.as_mut() else {
            return;
        };
        if p.field != r.field {
            return;
        }
        if let Ok((values, _)) = &r.values {
            // an existing filter starts with its values ticked, a new one with all of them
            let current: Vec<Option<String>> = self.pivot_builder.spec.filters.iter().find(|f| f.field == r.field).map(|f| f.values.clone()).unwrap_or_default();
            p.checked = if current.is_empty() { values.iter().cloned().collect() } else { current.into_iter().collect() };
        }
        p.values = Some(r.values);
    }

    fn open_filter_popup(&mut self, field: &str) {
        let Some(input) = self.pivot_input() else {
            return;
        };
        self.pivot_builder.popup = Some(FilterPopup { field: field.to_string(), ..Default::default() });
        let req = PivotValuesRequest { tab_id: input.tab_id, source: input.source, steps: input.steps, field: field.to_string(), temp_dir: self.paths.temp_dir.clone() };
        let key = JobKey::tab(JobKind::PivotValues, input.tab_id);
        self.jobs.submit(key, "Values of a field", false, move |ctx| compute_pivot_values(req, ctx).map(|r| JobOutput::PivotValues(Box::new(r))));
    }

    fn kind_of(&self, field: &str) -> FieldKind {
        self.pivot_builder.fields.iter().find(|(n, _)| n == field).map(|(_, k)| *k).unwrap_or(FieldKind::Other)
    }

    /// The value a field gets when it is added to Values: a sum of a number, a count of anything
    /// else; a rate is averaged by the pinned control column (a sum of rates means nothing).
    fn default_value_for(&self, field: &str) -> PivotValue {
        let kind = self.kind_of(field);
        let numbers: Vec<&String> = self.pivot_builder.fields.iter().filter(|(_, k)| *k == FieldKind::Number).map(|(n, _)| n).collect();
        let control = self.tabs.get(self.active_tab).and_then(|t| t.control_col.clone()).filter(|c| numbers.contains(&c) && c != field);
        let (mut agg, mut weight) = match kind {
            FieldKind::Number if micro_parquet::summary::is_rate_like_name(field) => match control {
                Some(c) => (PivotAgg::WeightedAvg, Some(c)),
                None => (PivotAgg::Average, None),
            },
            FieldKind::Number => (PivotAgg::Sum, None),
            _ => (PivotAgg::Count, None),
        };
        // the same field twice gets another aggregation, so the two columns differ
        let taken = |a: PivotAgg, spec: &PivotSpec| spec.values.iter().any(|v| v.field == field && v.agg == a);
        if taken(agg, &self.pivot_builder.spec) {
            let options: &[PivotAgg] = if kind == FieldKind::Number {
                &[PivotAgg::Sum, PivotAgg::Count, PivotAgg::Average, PivotAgg::Min, PivotAgg::Max, PivotAgg::CountDistinct]
            } else {
                &[PivotAgg::Count, PivotAgg::CountDistinct]
            };
            if let Some(next) = options.iter().find(|a| !taken(**a, &self.pivot_builder.spec)) {
                agg = *next;
                weight = None;
            }
        }
        PivotValue { field: field.to_string(), agg, weight_col: weight }
    }

    /// Puts a field into an area (a field is in Rows or in Columns, not both).
    pub(crate) fn add_pivot_field(&mut self, field: &str, area: Area) {
        match area {
            Area::Rows | Area::Columns => {
                let spec = &mut self.pivot_builder.spec;
                spec.rows.retain(|f| f != field);
                spec.columns.retain(|f| f != field);
                if area == Area::Rows {
                    spec.rows.push(field.to_string());
                } else {
                    spec.columns.push(field.to_string());
                }
            }
            Area::Values => {
                let v = self.default_value_for(field);
                self.pivot_builder.spec.values.push(v);
            }
            Area::Filters => {
                if !self.pivot_builder.spec.filters.iter().any(|f| f.field == field) {
                    self.pivot_builder.spec.filters.push(PivotFilter { field: field.to_string(), values: Vec::new() });
                }
                self.open_filter_popup(field);
            }
        }
    }

    fn remove_pivot_chip(&mut self, area: Area, index: usize) {
        let spec = &mut self.pivot_builder.spec;
        match area {
            Area::Rows => {
                if index < spec.rows.len() {
                    spec.rows.remove(index);
                }
            }
            Area::Columns => {
                if index < spec.columns.len() {
                    spec.columns.remove(index);
                }
            }
            Area::Filters => {
                if index < spec.filters.len() {
                    spec.filters.remove(index);
                }
                self.pivot_builder.popup = None;
            }
            Area::Values => {
                if index < spec.values.len() {
                    spec.values.remove(index);
                    spec.sort_by_value = match spec.sort_by_value {
                        Some(s) if s.value_index == index => None,
                        Some(s) if s.value_index > index => Some(PivotSortByValue { value_index: s.value_index - 1, ..s }),
                        other => other,
                    };
                }
            }
        }
    }

    /// The areas a field is in, for the word next to it in the list.
    fn areas_of(&self, field: &str) -> Vec<Area> {
        let s = &self.pivot_builder.spec;
        let mut v = Vec::new();
        if s.filters.iter().any(|f| f.field == field) {
            v.push(Area::Filters);
        }
        if s.columns.iter().any(|f| f == field) {
            v.push(Area::Columns);
        }
        if s.rows.iter().any(|f| f == field) {
            v.push(Area::Rows);
        }
        if s.values.iter().any(|f| f.field == field) {
            v.push(Area::Values);
        }
        v
    }

    /// The step the builder makes.
    pub(crate) fn pivot_builder_step(&self) -> Option<TransformStep> {
        let s = &self.pivot_builder.spec;
        if s.values.is_empty() || (s.rows.is_empty() && s.columns.is_empty()) {
            return None;
        }
        if !matches!(self.pivot_builder.preview, Some(Ok(_))) {
            return None;
        }
        Some(TransformStep::from_pivot_spec(s.clone()))
    }

    /// The text under the title and in the footer.
    fn pivot_hint(&self) -> String {
        match &self.pivot_builder.preview {
            Some(Err(e)) => e.clone(),
            _ => "Row labels keep their natural order (01 \u{2192} 09), never sorted by size unless you choose Sort rows by a value.".to_string(),
        }
    }

    pub(crate) fn render_pivot_builder(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let mut step: Option<(TransformStep, bool)> = None;
        if !self.pivot_builder.open {
            self.open_pivot_builder(None, self.editing.is_some());
        }
        self.pivot_builder_tick();
        let input = self.pivot_input();
        let subtitle = match (&input, &self.editing) {
            (Some(i), Some(ed)) => format!("{} \u{b7} editing step {}", i.tab_title, ed.index + 1),
            (Some(i), None) if i.after_step > 0 => format!("{} \u{b7} after step {}", i.tab_title, i.after_step),
            (Some(i), None) => i.tab_title.clone(),
            _ => String::new(),
        };
        let editing = self.editing.is_some();
        let can_create = self.pivot_builder_step().is_some();
        let tk = *theme::tokens(self.dark_mode);
        let title = format!("{}  Pivot table", theme::ph::TABLE);
        let mut rects = PivotRects::default();
        theme::modal(&title)
            .default_width(theme::DIALOG_W_PIVOT_BUILDER)
            .min_width(theme::DIALOG_MIN_W_PIVOT_BUILDER)
            .fill_height()
            .show(ctx, |d| {
                d.header(|ui| {
                    ui.add(egui::Label::new(egui::RichText::new(&subtitle).color(tk.muted)).truncate());
                });
                let body_h = d.body_max_h();
                d.body_scroll(|ui| {
                    self.pivot_builder_body(ui, body_h, &mut rects);
                });
                d.footer(|ui| {
                    ui.horizontal(|ui| {
                        let hint = self.pivot_hint();
                        let room = (ui.available_width() - theme::PIVOT_FOOTER_BUTTONS_W - theme::PIVOT_GAP).max(0.0);
                        let r = ui
                            .allocate_ui_with_layout(egui::vec2(room, ui.spacing().interact_size.y), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                let color = if matches!(self.pivot_builder.preview, Some(Err(_))) { tk.danger } else { tk.muted };
                                ui.add(egui::Label::new(egui::RichText::new(hint).color(color)).truncate());
                            })
                            .response;
                        rects.footer_text = r.rect;
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let label = if editing { "Save changes" } else { "Create pivot tab" };
                            let create = ui.add_enabled_ui(can_create, |ui| theme::primary_button(ui, label)).inner;
                            rects.buttons.push((label.to_string(), create.rect));
                            if create.clicked() {
                                if let Some(s) = self.pivot_builder_step() {
                                    step = Some((s, self.pivot_builder.new_tab));
                                    close = true;
                                }
                            }
                            let cancel = ui.button("Cancel");
                            rects.buttons.push(("Cancel".to_string(), cancel.rect));
                            if cancel.clicked() {
                                close = true;
                            }
                        });
                    });
                });
            });
        self.pivot_builder.rects = rects;
        if close {
            self.pivot_builder.open = false;
            self.pivot_builder.popup = None;
        }
        ModalOutcome { close, step }
    }

    fn pivot_builder_body(&mut self, ui: &mut egui::Ui, body_h: f32, rects: &mut PivotRects) {
        let tk = *theme::tokens(self.dark_mode);
        let total_w = ui.available_width();
        let fields_w = theme::PIVOT_FIELDS_W.min(total_w * theme::PIVOT_FIELDS_MAX_SHARE);
        let right_w = (total_w - fields_w - theme::PIVOT_GAP - theme::SPACE_M).max(0.0);
        rects.body = ui.max_rect();
        let h = body_h.max(theme::PIVOT_PREVIEW_MIN_H);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme::PIVOT_GAP;
            let left = ui.allocate_ui_with_layout(egui::vec2(fields_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                self.pivot_fields_panel(ui, h, rects);
            });
            rects.fields = left.response.rect;
            let right = ui.allocate_ui_with_layout(egui::vec2(right_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                self.pivot_right_panel(ui, h, &tk, rects);
            });
            rects.right = right.response.rect;
        });
        self.pivot_filter_popup(ui.ctx(), rects);
    }

    fn pivot_fields_panel(&mut self, ui: &mut egui::Ui, h: f32, rects: &mut PivotRects) {
        let tk = *theme::tokens(self.dark_mode);
        ui.label(egui::RichText::new("FIELDS").size(theme::TEXT_LABEL).color(tk.muted).strong());
        let n = self.pivot_builder.fields.len();
        ui.add(
            egui::TextEdit::singleline(&mut self.pivot_builder.search)
                .hint_text(format!("{}  Search {} fields\u{2026}", theme::ph::MAGNIFYING_GLASS, n))
                .desired_width(ui.available_width()),
        );
        let q = self.pivot_builder.search.trim().to_lowercase();
        let list: Vec<(String, FieldKind)> = self.pivot_builder.fields.iter().filter(|(name, _)| q.is_empty() || name.to_lowercase().contains(&q)).cloned().collect();
        let used_h = ui.min_rect().height();
        let list_h = (h - used_h - theme::SPACE_M).max(theme::PIVOT_FIELD_ROW_H);
        let w = ui.available_width();
        let out = egui::ScrollArea::vertical().id_salt("pivot_fields").max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
            ui.set_width(w - theme::SPACE_M);
            for (name, kind) in &list {
                self.pivot_field_row(ui, name, *kind, rects);
            }
            if list.is_empty() {
                ui.label(egui::RichText::new("No field matches.").color(tk.muted));
            }
        });
        rects.fields_list = out.inner_rect;
    }

    fn pivot_field_row(&mut self, ui: &mut egui::Ui, name: &str, kind: FieldKind, rects: &mut PivotRects) {
        let tk = *theme::tokens(self.dark_mode);
        let areas = self.areas_of(name);
        let used = areas.iter().map(|a| a.used_word()).collect::<Vec<_>>().join(", ");
        let row_w = ui.available_width();
        // the row is laid out by hand, so nothing can be wider than it: badge, name, where it is
        // used, and the menu button
        let (row, _) = ui.allocate_exact_size(egui::vec2(row_w, theme::PIVOT_FIELD_ROW_H), egui::Sense::hover());
        let menu_rect = egui::Rect::from_min_size(egui::pos2(row.right() - theme::PIVOT_CHIP_MENU_W, row.top()), egui::vec2(theme::PIVOT_CHIP_MENU_W, theme::PIVOT_FIELD_ROW_H));
        let drag_rect = egui::Rect::from_min_max(row.min, egui::pos2(menu_rect.left() - theme::SPACE_XS, row.bottom()));
        // dragging the row (not the menu button) carries the field's name
        let id = egui::Id::new(("pivot_field", name));
        let drag = ui.interact(drag_rect, id, egui::Sense::click_and_drag());
        if drag.dragged() {
            egui::DragAndDrop::set_payload(ui.ctx(), name.to_string());
            ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Grabbing);
            if let Some(p) = ui.ctx().pointer_latest_pos() {
                egui::Area::new(egui::Id::new("pivot_drag_ghost")).order(egui::Order::Tooltip).interactable(false).fixed_pos(p + egui::vec2(theme::SPACE_M, theme::SPACE_M)).show(ui.ctx(), |ui| {
                    egui::Frame::none().fill(tk.accent_soft).stroke(theme::line(tk.accent_soft_border)).rounding(theme::RADIUS_CONTROL).inner_margin(egui::Margin::symmetric(theme::SPACE_S, theme::SPACE_XS)).show(ui, |ui| {
                        ui.label(egui::RichText::new(name).color(tk.accent_dark).strong());
                    });
                });
            }
        } else if drag.hovered() {
            ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Grab);
        }
        if !areas.is_empty() || drag.hovered() {
            let fill = if areas.is_empty() { tk.zebra } else { tk.accent_soft };
            ui.painter().rect_filled(drag_rect, theme::RADIUS_SMALL, fill);
        }
        let badge = egui::Rect::from_min_size(egui::pos2(drag_rect.left() + theme::SPACE_S, drag_rect.center().y - theme::PIVOT_BADGE_H / 2.0), egui::vec2(theme::PIVOT_BADGE_W, theme::PIVOT_BADGE_H));
        ui.painter().rect_filled(badge, theme::RADIUS_SMALL, tk.header_bg);
        ui.painter().text(badge.center(), egui::Align2::CENTER_CENTER, kind.badge(), egui::FontId::new(theme::TEXT_MARK, theme::family_semibold()), tk.ink2);
        let used_w = if used.is_empty() { 0.0 } else { ui.fonts(|f| f.layout_no_wrap(used.clone(), egui::FontId::new(theme::TEXT_MARK, theme::family_semibold()), tk.accent).size().x) };
        let used_rect = egui::Rect::from_min_max(egui::pos2((drag_rect.right() - theme::SPACE_S - used_w).max(badge.right()), drag_rect.top()), egui::pos2(drag_rect.right() - theme::SPACE_S, drag_rect.bottom()));
        if !used.is_empty() {
            ui.painter().text(egui::pos2(used_rect.right(), used_rect.center().y), egui::Align2::RIGHT_CENTER, &used, egui::FontId::new(theme::TEXT_MARK, theme::family_semibold()), tk.accent);
        }
        let gap = if used.is_empty() { 0.0 } else { theme::SPACE_S };
        let name_rect = egui::Rect::from_min_max(egui::pos2(badge.right() + theme::SPACE_S, drag_rect.top()), egui::pos2((used_rect.left() - gap).max(badge.right() + theme::SPACE_S), drag_rect.bottom()));
        ui.put(name_rect, egui::Label::new(egui::RichText::new(name).color(tk.ink)).truncate());
        // the menu: Add to Rows / Columns / Values / Filters
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(menu_rect), |ui| {
            // a button as small as its cell: the default minimum size would push the row wider
            ui.spacing_mut().interact_size = egui::vec2(theme::PIVOT_CHIP_MENU_W, theme::PIVOT_FIELD_ROW_H);
            ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
            ui.menu_button(theme::ph::DOTS_THREE, |ui| {
                for area in [Area::Rows, Area::Columns, Area::Values, Area::Filters] {
                    if ui.button(format!("Add to {}", area.title())).clicked() {
                        self.add_pivot_field(name, area);
                        ui.close_menu();
                    }
                }
            })
            .response
            .on_hover_text("Add to Rows, Columns, Values or Filters");
        });
        rects.field_rows.push((name.to_string(), row));
        rects.field_menus.push((name.to_string(), menu_rect));
    }

    fn pivot_right_panel(&mut self, ui: &mut egui::Ui, h: f32, tk: &theme::Tokens, rects: &mut PivotRects) {
        ui.spacing_mut().item_spacing.y = theme::PIVOT_GAP;
        let start = ui.cursor().top();
        // the four areas, two by two
        let gap = theme::PIVOT_GAP;
        let w = ui.available_width();
        let cell_w = ((w - gap) / 2.0).max(0.0);
        for pair in [[Area::Filters, Area::Columns], [Area::Rows, Area::Values]] {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for area in pair {
                    let r = ui.allocate_ui_with_layout(egui::vec2(cell_w, theme::PIVOT_AREA_MIN_H), egui::Layout::top_down(egui::Align::Min), |ui| self.pivot_area(ui, area, cell_w, tk, rects));
                    rects.areas[area.index()] = r.response.rect;
                }
            });
        }
        // totals, show values as, scale, sort: groups of controls packed into rows by their width
        // (a combo box does not wrap by itself)
        let start_toggles = ui.cursor().top();
        let avail = ui.available_width();
        let widths = [self.toggle_group_width(ui, 0), self.toggle_group_width(ui, 1), self.toggle_group_width(ui, 2), self.toggle_group_width(ui, 3)];
        let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
        let mut used = 0.0;
        for (g, gw) in widths.iter().enumerate() {
            let need = if rows.last().map(|r| r.is_empty()).unwrap_or(true) { *gw } else { used + theme::SPACE_L + *gw };
            if need > avail && !rows.last().map(|r| r.is_empty()).unwrap_or(true) {
                rows.push(Vec::new());
                used = *gw;
            } else {
                used = need;
            }
            if let Some(r) = rows.last_mut() {
                r.push(g);
            }
        }
        for row in rows {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_L;
                for g in row {
                    self.toggle_group(ui, g);
                }
            });
        }
        rects.toggles = egui::Rect::from_min_max(egui::pos2(ui.min_rect().left(), start_toggles), egui::pos2(ui.min_rect().left() + avail.min(ui.min_rect().width()), ui.cursor().top() - ui.spacing().item_spacing.y));
        // the preview takes what is left
        let used = ui.cursor().top() - start;
        let preview_h = (h - used - gap).max(theme::PIVOT_PREVIEW_MIN_H);
        self.pivot_preview(ui, preview_h, tk, rects);
    }

    fn pivot_area(&mut self, ui: &mut egui::Ui, area: Area, w: f32, tk: &theme::Tokens, rects: &mut PivotRects) {
        let spec_snapshot = self.pivot_builder.spec.clone();
        let hovered = egui::DragAndDrop::has_payload_of_type::<String>(ui.ctx());
        let stroke = if hovered { theme::line(tk.accent) } else { theme::line(tk.border) };
        let frame = egui::Frame::none().fill(tk.zebra).stroke(stroke).rounding(theme::RADIUS_PANEL).inner_margin(theme::SPACE_M);
        let mut remove: Option<usize> = None;
        let mut open_popup: Option<String> = None;
        let (zone, dropped) = ui.dnd_drop_zone::<String, ()>(frame, |ui| {
            ui.set_min_size(egui::vec2((w - 2.0 * theme::SPACE_M - 2.0).max(0.0), theme::PIVOT_AREA_MIN_H - 2.0 * theme::SPACE_M));
            ui.set_max_width((w - 2.0 * theme::SPACE_M - 2.0).max(0.0));
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(area.title()).color(tk.ink).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(area.hint()).size(theme::TEXT_LABEL).color(tk.muted)).truncate());
                });
            });
            // the chips flow into rows by their width (a row never needs more than the area has)
            let avail = ui.available_width();
            enum Chip {
                Plain(String, String),
                Value(usize),
            }
            let chips: Vec<Chip> = match area {
                Area::Rows => spec_snapshot.rows.iter().map(|f| Chip::Plain(f.clone(), String::new())).collect(),
                Area::Columns => spec_snapshot.columns.iter().map(|f| Chip::Plain(f.clone(), String::new())).collect(),
                Area::Filters => spec_snapshot
                    .filters
                    .iter()
                    .map(|f| {
                        let extra = match f.values.len() {
                            0 => "all values".to_string(),
                            1 => format!("= {}", f.values[0].clone().unwrap_or_else(|| BLANK_LABEL.to_string())),
                            n => format!("{} values", n),
                        };
                        Chip::Plain(f.field.clone(), extra)
                    })
                    .collect(),
                Area::Values => (0..spec_snapshot.values.len()).map(Chip::Value).collect(),
            };
            let widths: Vec<f32> = chips
                .iter()
                .map(|c| match c {
                    Chip::Plain(n, e) => simple_chip_width(ui, n, e).min(avail),
                    Chip::Value(i) => value_chip_width(ui, &spec_snapshot.values[*i], 0.0).min(avail),
                })
                .collect();
            let mut rows: Vec<Vec<usize>> = Vec::new();
            let mut used = 0.0;
            for (i, cw) in widths.iter().enumerate() {
                if rows.is_empty() || used + theme::SPACE_S + cw > avail {
                    rows.push(Vec::new());
                    used = *cw;
                } else {
                    used += theme::SPACE_S + cw;
                }
                if let Some(r) = rows.last_mut() {
                    r.push(i);
                }
            }
            for row in rows {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_S;
                    for i in row {
                        match &chips[i] {
                            Chip::Plain(name, extra) => {
                                let (clicked, rm, rect) = simple_chip(ui, name, extra, widths[i], tk);
                                rects.chips.push((area, name.clone(), rect));
                                if clicked && area == Area::Filters {
                                    open_popup = Some(name.clone());
                                }
                                if rm {
                                    remove = Some(i);
                                }
                            }
                            Chip::Value(j) => {
                                let v = spec_snapshot.values[*j].clone();
                                let (rect, rm) = self.value_chip(ui, *j, &v, widths[i], tk);
                                rects.chips.push((area, v.field.clone(), rect));
                                if rm {
                                    remove = Some(*j);
                                }
                            }
                        }
                    }
                });
            }
        });
        let _ = zone;
        if let Some(field) = dropped {
            self.add_pivot_field(&field, area);
        }
        if let Some(i) = remove {
            self.remove_pivot_chip(area, i);
        }
        if let Some(f) = open_popup {
            if self.pivot_builder.popup.as_ref().map(|p| p.field != f).unwrap_or(true) {
                self.open_filter_popup(&f);
            } else {
                self.pivot_builder.popup = None;
            }
        }
    }

    /// A chip of Values of exactly width `w`: the field (cut when the room is short), its
    /// aggregation (and weight), and a remove button.
    fn value_chip(&mut self, ui: &mut egui::Ui, i: usize, v: &PivotValue, w: f32, tk: &theme::Tokens) -> (egui::Rect, bool) {
        let numeric = self.kind_of(&v.field) == FieldKind::Number;
        let numbers: Vec<String> = self.pivot_builder.fields.iter().filter(|(_, k)| *k == FieldKind::Number).map(|(n, _)| n.clone()).collect();
        let mut agg = v.agg;
        let mut weight = v.weight_col.clone();
        let mut removed = false;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, theme::PIVOT_CHIP_H), egui::Sense::hover());
        ui.painter().rect(rect, theme::RADIUS_CONTROL, tk.surface, theme::line(tk.accent_soft_border));
        let inner = egui::Rect::from_min_max(egui::pos2(rect.left() + theme::SPACE_S, rect.top()), egui::pos2(rect.right() - theme::SPACE_XS, rect.bottom()));
        let fixed = value_chip_fixed_width(agg);
        let label_w = (inner.width() - fixed).max(0.0);
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_S;
            ui.spacing_mut().interact_size.y = theme::PIVOT_CHIP_H - 2.0;
            ui.allocate_ui_with_layout(egui::vec2(label_w, theme::PIVOT_CHIP_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(egui::RichText::new(&v.field).font(egui::FontId::new(theme::TEXT_BASE, theme::family_semibold())).color(tk.accent_dark)).truncate());
            });
            egui::ComboBox::from_id_salt(("pivot_agg", i)).width(theme::PIVOT_COMBO_AGG_W - ui.spacing().interact_size.y).selected_text(agg.label()).show_ui(ui, |ui| {
                for a in PivotAgg::all() {
                    if numeric || matches!(a, PivotAgg::Count | PivotAgg::CountDistinct) {
                        ui.selectable_value(&mut agg, *a, a.label());
                    }
                }
            });
            if agg == PivotAgg::WeightedAvg {
                ui.label(egui::RichText::new("by").color(tk.muted));
                let shown = weight.clone().unwrap_or_else(|| "choose".to_string());
                egui::ComboBox::from_id_salt(("pivot_weight", i)).width(theme::PIVOT_COMBO_WEIGHT_W - ui.spacing().interact_size.y).selected_text(shown).show_ui(ui, |ui| {
                    for n in &numbers {
                        ui.selectable_value(&mut weight, Some(n.clone()), n.as_str());
                    }
                });
            }
            if ui
                .add_sized(egui::vec2(theme::PIVOT_CHIP_MENU_W, theme::PIVOT_CHIP_H), egui::Button::new(egui::RichText::new(theme::ph::X).size(theme::TEXT_MARK).color(tk.muted)).frame(false))
                .on_hover_text("Remove field")
                .clicked()
            {
                removed = true;
            }
        });
        if agg != v.agg || weight != v.weight_col {
            if let Some(slot) = self.pivot_builder.spec.values.get_mut(i) {
                slot.agg = agg;
                if agg == PivotAgg::WeightedAvg && weight.is_none() {
                    weight = numbers.iter().find(|n| **n != slot.field).cloned();
                }
                slot.weight_col = if agg == PivotAgg::WeightedAvg { weight } else { None };
            }
        }
        (rect, removed)
    }

    fn pivot_preview(&mut self, ui: &mut egui::Ui, h: f32, tk: &theme::Tokens, rects: &mut PivotRects) {
        let frame = egui::Frame::none().stroke(theme::line(tk.border)).rounding(theme::RADIUS_PANEL).inner_margin(0.0);
        let w = ui.available_width();
        let out = ui.allocate_ui_with_layout(egui::vec2(w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
            frame.show(ui, |ui| {
                ui.set_width((w - 2.0).max(0.0));
                ui.spacing_mut().item_spacing.y = 0.0;
                // the header of the preview
                let caption = match &self.pivot_builder.preview {
                    Some(Ok(_)) => {
                        let s = &self.pivot_builder.spec;
                        let what = s.values.iter().map(|v| v.label()).collect::<Vec<_>>().join(", ");
                        let shown = self.pivot_builder.preview_total_rows.min(micro_parquet::tab_jobs::PIVOT_PREVIEW_ROWS);
                        format!("{} \u{b7} {} \u{b7} first {} of {} rows", what, s.scale.label(), shown, self.pivot_builder.preview_total_rows)
                    }
                    Some(Err(_)) => "no table yet".to_string(),
                    None => "computing\u{2026}".to_string(),
                };
                egui::Frame::none().fill(tk.zebra).inner_margin(egui::Margin::symmetric(theme::SPACE_M, theme::SPACE_S)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Preview").color(tk.ink).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(caption).size(theme::TEXT_LABEL).color(tk.muted)).truncate());
                        });
                    });
                });
                ui.separator();
                let left = ui.available_height().min(h);
                let area_h = (left - theme::SPACE_XS).max(theme::DATA_ROW_H);
                let out = egui::ScrollArea::both().id_salt("pivot_preview").max_height(area_h).auto_shrink([false, false]).show(ui, |ui| match &self.pivot_builder.preview {
                    Some(Ok(df)) => self.pivot_preview_grid(ui, df, tk),
                    Some(Err(e)) => {
                        ui.add_space(theme::SPACE_S);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new(e).color(tk.muted));
                        });
                    }
                    None => {
                        ui.add_space(theme::SPACE_S);
                        ui.label(egui::RichText::new("Computing the preview\u{2026}").color(tk.muted));
                    }
                });
                rects.preview_scroll = out.inner_rect;
            });
        });
        rects.preview = out.response.rect;
    }

    fn pivot_preview_grid(&self, ui: &mut egui::Ui, df: &DataFrame, tk: &theme::Tokens) {
        let spec = &self.pivot_builder.spec;
        let names: Vec<String> = df.get_column_names().iter().map(|n| n.to_string()).collect();
        let label_n = spec.rows.len().max(1);
        let formats: HashMap<String, ColumnFormatSpec> = pivot_column_formats(spec, &names, self.global_decimals).into_iter().collect();
        // the cells as shown, and the width of each column: its heading or its widest cell
        let cells: Vec<Vec<String>> = (0..df.height())
            .map(|r| names.iter().enumerate().map(|(c, n)| df.get_columns().get(c).and_then(|s| s.get(r).ok()).map(|v| self.pivot_cell_text(n, v, formats.get(n))).unwrap_or_default()).collect())
            .collect();
        let mono = egui::FontId::new(theme::TEXT_MONO, egui::FontFamily::Monospace);
        let head_font = egui::FontId::new(theme::TEXT_BASE, theme::family_semibold());
        let widths: Vec<f32> = names
            .iter()
            .enumerate()
            .map(|(c, n)| {
                let head = ui.fonts(|f| f.layout_no_wrap(n.clone(), head_font.clone(), tk.ink).size().x);
                let body = cells.iter().map(|row| ui.fonts(|f| f.layout_no_wrap(row[c].clone(), mono.clone(), tk.ink).size().x)).fold(0.0, f32::max);
                let least = if c < label_n { theme::PIVOT_PREVIEW_LABEL_W } else { theme::PIVOT_PREVIEW_COL_W };
                (head.max(body) + 2.0 * theme::SPACE_M).max(least)
            })
            .collect();
        let total_w: f32 = widths.iter().sum();
        let last = cells.len().saturating_sub(1);
        let has_total_row = spec.column_totals && spec.rows.len() > 0 && self.pivot_builder.preview_total_rows <= micro_parquet::tab_jobs::PIVOT_PREVIEW_ROWS;
        let mut draw_row = |ui: &mut egui::Ui, texts: &[String], head: bool, fill: Option<egui::Color32>, strong: bool| {
            let (row, _) = ui.allocate_exact_size(egui::vec2(total_w, theme::PIVOT_PREVIEW_ROW_H), egui::Sense::hover());
            if let Some(f) = fill {
                ui.painter().rect_filled(row, 0.0, f);
            }
            let mut x = row.left();
            for (c, t) in texts.iter().enumerate() {
                let cell = egui::Rect::from_min_size(egui::pos2(x, row.top()), egui::vec2(widths[c], row.height()));
                x += widths[c];
                let inner = cell.shrink2(egui::vec2(theme::SPACE_M, 0.0));
                let value = c >= label_n;
                let rich = if head {
                    egui::RichText::new(t).font(head_font.clone()).color(tk.ink)
                } else if value {
                    egui::RichText::new(t).font(mono.clone()).color(if strong { tk.accent_dark } else { tk.ink })
                } else {
                    let r = egui::RichText::new(t).color(if strong { tk.accent_dark } else { tk.ink });
                    if strong { r.strong() } else { r }
                };
                let layout = if value { egui::Layout::right_to_left(egui::Align::Center) } else { egui::Layout::left_to_right(egui::Align::Center) };
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(inner).layout(layout), |ui| {
                    ui.add(egui::Label::new(rich).truncate());
                });
            }
        };
        draw_row(ui, &names, true, Some(tk.zebra), true);
        for (r, row) in cells.iter().enumerate() {
            let total = has_total_row && r == last;
            let fill = if total { Some(tk.accent_soft) } else if r % 2 == 1 { Some(tk.zebra) } else { None };
            draw_row(ui, row, false, fill, total);
        }
    }

    fn pivot_cell_text(&self, name: &str, v: AnyValue, fmt: Option<&ColumnFormatSpec>) -> String {
        match v {
            AnyValue::Null => "\u{2014}".to_string(),
            AnyValue::String(s) => s.to_string(),
            AnyValue::StringOwned(s) => s.to_string(),
            AnyValue::Float64(x) => format_metric_value(x, name, fmt, self.num_format, self.global_decimals),
            AnyValue::Int64(x) => format_with_commas(x as f64, 0),
            other => format!("{}", other),
        }
    }

    /// The Filters checklist of one field.
    fn pivot_filter_popup(&mut self, ctx: &egui::Context, rects: &mut PivotRects) {
        let Some(mut popup) = self.pivot_builder.popup.clone() else {
            return;
        };
        let tk = *theme::tokens(self.dark_mode);
        // under the chip of this field
        let anchor = rects.chips.iter().find(|(a, f, _)| *a == Area::Filters && *f == popup.field).map(|(_, _, r)| *r);
        let Some(anchor) = anchor else {
            self.pivot_builder.popup = None;
            return;
        };
        let pos = egui::pos2(anchor.left(), anchor.bottom() + theme::SPACE_XS);
        let mut changed = false;
        let mut close = false;
        let area = egui::Area::new(egui::Id::new("pivot_filter_popup")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
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
                        ui.horizontal(|ui| {
                            if ui.small_button("Select all").clicked() && !more {
                                popup.checked = values.iter().cloned().collect();
                                changed = true;
                            }
                            if ui.small_button("Clear").clicked() {
                                popup.checked.clear();
                                changed = true;
                            }
                            ui.label(egui::RichText::new(format!("{} of {}{}", popup.checked.len(), values.len(), if more { "+" } else { "" })).size(theme::TEXT_LABEL).color(tk.muted));
                        });
                        let q = popup.search.trim().to_lowercase();
                        egui::ScrollArea::vertical().id_salt("pivot_filter_values").max_height(theme::PIVOT_POPUP_LIST_H).auto_shrink([false, true]).show(ui, |ui| {
                            for v in values.iter().filter(|v| q.is_empty() || v.as_deref().unwrap_or(BLANK_LABEL).to_lowercase().contains(&q)) {
                                let mut on = popup.checked.contains(v);
                                if ui.checkbox(&mut on, v.clone().unwrap_or_else(|| BLANK_LABEL.to_string())).changed() {
                                    if on {
                                        popup.checked.insert(v.clone());
                                    } else {
                                        popup.checked.remove(v);
                                    }
                                    changed = true;
                                }
                            }
                        });
                        if more {
                            ui.label(egui::RichText::new("More values than are listed: search, or tick the ones you want.").size(theme::TEXT_LABEL).color(tk.muted));
                        }
                        if popup.checked.is_empty() {
                            ui.label(egui::RichText::new("Nothing ticked: the pivot would be empty. Tick a value, or remove the filter.").size(theme::TEXT_LABEL).color(tk.danger));
                        }
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("Done").clicked() {
                        close = true;
                    }
                });
            });
        });
        rects.popup = area.response.rect;
        // a click outside it, or Esc, closes it
        let pressed_outside = ctx.input(|i| i.pointer.any_pressed()) && ctx.input(|i| i.pointer.interact_pos()).map(|p| !area.response.rect.contains(p) && !anchor.contains(p)).unwrap_or(false);
        if pressed_outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if changed {
            if let Some(Ok((values, more))) = &popup.values {
                // all values ticked is no filter; the list, when it is the whole list
                let all: HashSet<Option<String>> = values.iter().cloned().collect();
                let chosen: Vec<Option<String>> = {
                    let mut v: Vec<Option<String>> = values.iter().filter(|x| popup.checked.contains(*x)).cloned().collect();
                    // values outside a truncated list that are ticked stay (they were set earlier)
                    v.extend(popup.checked.iter().filter(|x| !all.contains(*x)).cloned());
                    v
                };
                if let Some(f) = self.pivot_builder.spec.filters.iter_mut().find(|f| f.field == popup.field) {
                    f.values = if !*more && popup.checked == all { Vec::new() } else { chosen };
                }
            }
        }
        if close {
            self.pivot_builder.popup = None;
        } else {
            self.pivot_builder.popup = Some(popup);
        }
    }

    /// After a refresh: a tab whose last step is a pivot gets the formats of its value columns, so
    /// a rate is never divided into Crore and a count stays a whole number. Run again whenever
    /// the columns may have changed (new data, an edit, a macro).
    pub(crate) fn apply_pivot_formats(&mut self, idx: usize) {
        let decimals = self.global_decimals;
        let Some(tab) = self.tabs.get_mut(idx) else {
            return;
        };
        let exec = tab.exec_steps.min(tab.steps.len());
        let Some(spec) = exec.checked_sub(1).and_then(|i| tab.steps.get(i)).and_then(|s| s.as_pivot_spec()) else {
            return;
        };
        let columns = tab.available_columns();
        for (name, f) in pivot_column_formats(&spec, &columns, decimals) {
            tab.col_formats.insert(name, f);
        }
    }

    /// Makes the formats of the pivot step being added or edited.
    pub(crate) fn pivot_formats_for(&self, step: &TransformStep) -> Vec<(String, ColumnFormatSpec)> {
        match step.as_pivot_spec() {
            Some(spec) => pivot_column_formats(&spec, &self.pivot_builder.value_columns, self.global_decimals),
            None => Vec::new(),
        }
    }
}

impl MicroParquetApp {
    /// The width of a group of the toggles row: 0 the three totals boxes, 1 show values as,
    /// 2 scale, 3 sort rows.
    fn toggle_group_width(&self, ui: &egui::Ui, g: usize) -> f32 {
        let text_w = |t: &str| ui.fonts(|f| f.layout_no_wrap(t.to_string(), egui::FontId::new(theme::TEXT_BASE, egui::FontFamily::Proportional), egui::Color32::WHITE).size().x);
        let check = |t: &str| text_w(t) + ui.spacing().icon_width + ui.spacing().icon_spacing + theme::SPACE_L;
        let combo = |w: f32| w + ui.spacing().interact_size.y + theme::SPACE_S;
        match g {
            0 => check("Row totals") + check("Column totals") + check("Subtotals"),
            1 => text_w("Show values as") + theme::SPACE_S + combo(theme::PIVOT_COMBO_W),
            2 => text_w("Scale") + theme::SPACE_S + combo(theme::PIVOT_COMBO_W),
            _ => text_w("Sort rows") + theme::SPACE_S + combo(theme::PIVOT_COMBO_SORT_W),
        }
    }

    fn toggle_group(&mut self, ui: &mut egui::Ui, g: usize) {
        let s = &mut self.pivot_builder.spec;
        ui.spacing_mut().item_spacing.x = theme::SPACE_S;
        match g {
            0 => {
                ui.checkbox(&mut s.row_totals, "Row totals");
                ui.checkbox(&mut s.column_totals, "Column totals");
                ui.checkbox(&mut s.subtotals, "Subtotals");
            }
            1 => {
                ui.label("Show values as");
                egui::ComboBox::from_id_salt("pivot_show_as").width(theme::PIVOT_COMBO_W).selected_text(s.show_as.label()).show_ui(ui, |ui| {
                    for a in PivotShowAs::all() {
                        ui.selectable_value(&mut s.show_as, *a, a.label());
                    }
                });
            }
            2 => {
                ui.label("Scale");
                egui::ComboBox::from_id_salt("pivot_scale").width(theme::PIVOT_COMBO_W).selected_text(s.scale.label()).show_ui(ui, |ui| {
                    for a in PivotScale::all() {
                        ui.selectable_value(&mut s.scale, *a, a.label());
                    }
                });
            }
            _ => {
                ui.label("Sort rows");
                let current = match s.sort_by_value {
                    None => "Natural order".to_string(),
                    Some(b) => format!("{} {}", s.values.get(b.value_index).map(|v| v.label()).unwrap_or_default(), if b.descending { "(high to low)" } else { "(low to high)" }),
                };
                egui::ComboBox::from_id_salt("pivot_sort").width(theme::PIVOT_COMBO_SORT_W).selected_text(current).show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.sort_by_value, None, "Natural order");
                    for (i, v) in s.values.clone().iter().enumerate() {
                        ui.selectable_value(&mut s.sort_by_value, Some(PivotSortByValue { value_index: i, descending: true }), format!("{} (high to low)", v.label()));
                        ui.selectable_value(&mut s.sort_by_value, Some(PivotSortByValue { value_index: i, descending: false }), format!("{} (low to high)", v.label()));
                    }
                });
            }
        }
    }
}

fn chip_text_w(ui: &egui::Ui, text: &str, size: f32, family: egui::FontFamily) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(text.to_string(), egui::FontId::new(size, family), egui::Color32::WHITE).size().x)
}

/// The width of what a chip of Values needs besides the field's name.
fn value_chip_fixed_width(agg: PivotAgg) -> f32 {
    let mut w = theme::SPACE_S + theme::PIVOT_COMBO_AGG_W + theme::SPACE_S + theme::PIVOT_CHIP_MENU_W;
    if agg == PivotAgg::WeightedAvg {
        w += theme::SPACE_S + theme::PIVOT_BADGE_W + theme::SPACE_S + theme::PIVOT_COMBO_WEIGHT_W;
    }
    w
}

/// The natural width of a chip of Values.
fn value_chip_width(ui: &egui::Ui, v: &PivotValue, _extra: f32) -> f32 {
    2.0 * theme::SPACE_S + chip_text_w(ui, &v.field, theme::TEXT_BASE, theme::family_semibold()) + value_chip_fixed_width(v.agg)
}

/// The natural width of a chip of Rows, Columns or Filters.
fn simple_chip_width(ui: &egui::Ui, name: &str, extra: &str) -> f32 {
    let mut w = 2.0 * theme::SPACE_S + chip_text_w(ui, name, theme::TEXT_BASE, theme::family_semibold()) + theme::SPACE_S + theme::PIVOT_CHIP_MENU_W;
    if !extra.is_empty() {
        w += theme::SPACE_S + chip_text_w(ui, extra, theme::TEXT_LABEL, egui::FontFamily::Proportional);
    }
    w
}

/// A chip of Rows, Columns or Filters of exactly width `w`: name (cut when it does not fit), a
/// small second text, and a remove button. Returns (clicked, removed, the chip's rectangle).
fn simple_chip(ui: &mut egui::Ui, name: &str, extra: &str, w: f32, tk: &theme::Tokens) -> (bool, bool, egui::Rect) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, theme::PIVOT_CHIP_H), egui::Sense::click());
    ui.painter().rect(rect, theme::RADIUS_CONTROL, tk.surface, theme::line(tk.accent_soft_border));
    let cross = egui::Rect::from_min_size(egui::pos2(rect.right() - theme::PIVOT_CHIP_MENU_W, rect.top()), egui::vec2(theme::PIVOT_CHIP_MENU_W, rect.height()));
    let mut right = cross.left();
    if !extra.is_empty() {
        let ew = chip_text_w(ui, extra, theme::TEXT_LABEL, egui::FontFamily::Proportional);
        ui.painter().text(egui::pos2(right, rect.center().y), egui::Align2::RIGHT_CENTER, extra, egui::FontId::new(theme::TEXT_LABEL, egui::FontFamily::Proportional), tk.muted);
        right -= ew + theme::SPACE_S;
    }
    let name_rect = egui::Rect::from_min_max(egui::pos2(rect.left() + theme::SPACE_S, rect.top()), egui::pos2((right - theme::SPACE_XS).max(rect.left() + theme::SPACE_S), rect.bottom()));
    ui.put(name_rect, egui::Label::new(egui::RichText::new(name).font(egui::FontId::new(theme::TEXT_BASE, theme::family_semibold())).color(tk.accent_dark)).truncate());
    let removed = ui
        .put(cross, egui::Button::new(egui::RichText::new(theme::ph::X).size(theme::TEXT_MARK).color(tk.muted)).frame(false))
        .on_hover_text("Remove field")
        .clicked();
    (resp.clicked() && !removed, removed, rect)
}
