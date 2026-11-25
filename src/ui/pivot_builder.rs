//! The pivot builder (Phase 4 V1), after `docs/design/Pivot.dc.html`: a searchable list of fields,
//! four areas (Filters, Columns, Rows, Values) that fields are dragged into or added to from a menu,
//! the totals and "show values as" settings, and a live preview of the first 20 rows that the
//! worker thread computes.

use crate::ui::prelude::*;
use micro_parquet::models::{TimeAgg, TimeValue};
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

/// The line in the empty Filters box.
/// The title of the note the app window shows while the builder is in its own window.
pub const PIVOT_POPOUT_NOTE_TITLE: &str = "Pivot builder";

pub const FILTERS_EMPTY_HINT: &str = "Drop a field here to limit the pivot to chosen values";

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
    /// The expander of each date column ("Date parts"), by the column's name (W2).
    pub expanders: Vec<(String, egui::Rect)>,
    /// The reason a broken link's fields are greyed, under its expander (Phase 9 M1).
    pub link_notes: Vec<(String, egui::Rect)>,
    pub toggles: egui::Rect,
    pub preview: egui::Rect,
    pub preview_scroll: egui::Rect,
    pub popup: egui::Rect,
    pub footer_text: egui::Rect,
    pub buttons: Vec<(String, egui::Rect)>,
    /// The "Dock" button the app window shows while the builder is in its own window.
    pub note_dock: egui::Rect,
    /// The Layout box, and the "Blank line after each group" box of the Nested layout (Phase 9 P2).
    pub layout_combo: egui::Rect,
    pub blank_line: egui::Rect,
    /// The aggregation box of each chip of Values, by its position (Phase 9 M3).
    pub agg_combos: Vec<(usize, egui::Rect)>,
    /// The warnings above the preview (Phase 9 M3).
    pub warnings: Vec<egui::Rect>,
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
            expanders: Vec::new(),
            link_notes: Vec::new(),
            toggles: n,
            preview: n,
            preview_scroll: n,
            popup: n,
            footer_text: n,
            buttons: Vec::new(),
            note_dock: n,
            layout_combo: n,
            blank_line: n,
            agg_combos: Vec::new(),
            warnings: Vec::new(),
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
    /// The Nested view of the preview's settings, when they choose Nested (the dialog draws it instead of the flat table).
    pub preview_nested: Option<micro_parquet::pivot_view::NestedView>,
    /// Why a cell of the preview is blank, and what to warn about (Phase 9 M3).
    pub preview_notes: Vec<micro_parquet::pivot_table::CellNote>,
    pub preview_warnings: Vec<String>,
    /// The blank cells with a reason drawn in the last frame: (reason, rect). The render tests read it.
    pub notes_drawn: std::cell::RefCell<Vec<(String, egui::Rect)>>,
    /// All columns of the last good preview (the label columns first), for the column formats.
    pub value_columns: Vec<String>,
    pub popup: Option<FilterPopup>,
    /// Make a new tab (false: the pivot is added to this tab, or an edited step is replaced).
    pub new_tab: bool,
    pub rects: PivotRects,
    pub open: bool,
    /// The date columns whose parts are listed under them in the field list (W2).
    pub expanded: HashSet<String>,
    /// The builder is in its own window (P1). It is this same state, drawn there instead of docked, never both.
    pub popped: bool,
    /// How many times the window was asked for (the render tests read it: a backend without windows never opens one).
    pub pop_requests: u32,
    /// Where the window was last seen: [x, y, width, height] in points.
    pub popout_live: Option<[f32; 4]>,
    /// Where and how big the window opens: worked out when it first draws and kept until it is docked.
    pub popout_start: Option<PopoutPlacement>,
    /// How many rows the last preview was cut to (0 = not known yet: the docked count).
    pub preview_limit: usize,
    /// At the end of the last frame a text box had the keyboard, or a list or pop-up was open. egui takes the keyboard from a text
    /// box when Esc arrives, before the builder looks, so this is what tells that Esc was meant for the text box.
    pub esc_busy: bool,
    /// A field's menu was open in this frame's drawing (egui does not list its menus among the open pop-ups).
    pub menu_open: bool,
}

/// What a frame of the builder asks for, drawn docked or in its own window.
#[derive(Default)]
pub(crate) struct PivotActs {
    /// Apply (with a step) or Cancel (without): the builder closes.
    pub close: bool,
    pub step: Option<(TransformStep, bool)>,
    /// Pop out, Dock, or the window's close button: the builder changes place and stays open.
    pub toggle_pop: bool,
}

/// Where the builder's own window opens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopoutPlacement {
    pub size: egui::Vec2,
    /// `None`: the system places it (the position of this app's window is not known).
    pub pos: Option<egui::Pos2>,
}

/// Where the builder's own window opens: exactly where it was last (`saved`: x, y, width, height), on whatever monitor that is, with
/// no check of the monitor (owner decision on P1-2). If that is off every screen, the Dock button in the app window brings the builder
/// back. Centred on the main window, at the default size, only when no rectangle is saved.
pub fn popout_placement(saved: Option<[f32; 4]>, main: Option<egui::Rect>, monitor: Option<egui::Vec2>) -> PopoutPlacement {
    let min = egui::vec2(theme::PIVOT_POPOUT_MIN_SIZE[0], theme::PIVOT_POPOUT_MIN_SIZE[1]);
    let default = egui::vec2(theme::PIVOT_POPOUT_SIZE[0], theme::PIVOT_POPOUT_SIZE[1]);
    // a damaged number is not trusted: it is as if nothing was saved
    if let Some(r) = saved.filter(|r| r.iter().all(|v| v.is_finite())) {
        return PopoutPlacement { size: egui::vec2(r[2], r[3]).max(min), pos: Some(egui::pos2(r[0], r[1])) };
    }
    let mut size = default;
    // never bigger than the monitor
    if let Some(m) = monitor.filter(|m| m.x > 0.0 && m.y > 0.0) {
        size = size.min(m * theme::PIVOT_POPOUT_MONITOR_SHARE).max(min.min(m));
    }
    let Some(main) = main.filter(|m| m.min.x.is_finite() && m.min.y.is_finite() && m.max.x.is_finite() && m.max.y.is_finite()) else {
        return PopoutPlacement { size, pos: None };
    };
    let centred = main.center() - size / 2.0;
    let Some(monitor) = monitor.filter(|m| m.x > 0.0 && m.y > 0.0) else {
        return PopoutPlacement { size, pos: Some(centred) };
    };
    // centred on the main window, and on its monitor (monitors sit on a grid of their own size, with one corner at 0, 0)
    let origin = egui::pos2((main.center().x / monitor.x).floor() * monitor.x, (main.center().y / monitor.y).floor() * monitor.y);
    let screen = egui::Rect::from_min_size(origin, monitor);
    let x = centred.x.clamp(screen.left(), (screen.right() - size.x).max(screen.left()));
    let y = centred.y.clamp(screen.top(), (screen.bottom() - size.y).max(screen.top()));
    PopoutPlacement { size, pos: Some(egui::pos2(x, y)) }
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
        self.sort_lists.reload();
        let b = &mut self.pivot_builder;
        b.spec = from.unwrap_or_else(|| PivotSpec { row_totals: true, column_totals: true, ..Default::default() });
        b.fields = kinds;
        b.search.clear();
        b.submitted = None;
        b.preview = None;
        b.preview_total_rows = 0;
        b.preview_nested = None;
        b.value_columns.clear();
        b.popup = None;
        b.new_tab = !editing;
        b.popped = false;
        b.popout_start = None;
        b.preview_limit = 0;
        b.esc_busy = false;
        b.open = true;
    }

    /// How many rows the preview is cut to: the docked count, or, in the builder's own window, as many as fit its height
    /// (in steps of that count, so a window being dragged does not ask for a new table at every pixel).
    pub(crate) fn pivot_preview_rows_wanted(&self) -> usize {
        use micro_parquet::tab_jobs::{PIVOT_PREVIEW_ROWS, PIVOT_PREVIEW_ROWS_MAX};
        let b = &self.pivot_builder;
        let h = b.rects.preview_scroll.height();
        if !b.popped || !h.is_finite() || h <= 0.0 {
            return PIVOT_PREVIEW_ROWS;
        }
        // the heading row comes first
        let fit = ((h - theme::PIVOT_PREVIEW_ROW_H) / theme::PIVOT_PREVIEW_ROW_H).ceil().max(0.0) as usize;
        (fit.div_ceil(PIVOT_PREVIEW_ROWS) * PIVOT_PREVIEW_ROWS).clamp(PIVOT_PREVIEW_ROWS, PIVOT_PREVIEW_ROWS_MAX)
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
        let rows = self.pivot_preview_rows_wanted();
        self.pivot_builder.spec.data_tab = self.pivot_data_tab(input.tab_id);
        let token = fingerprint(&(&self.pivot_builder.spec, micro_parquet::steps_edit::pipeline_hash(&input.source, &input.steps), rows));
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
            rows,
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
            b.preview_nested = r.nested;
            b.preview_notes = r.notes;
            b.preview_warnings = r.warnings;
            b.preview_limit = r.limit;
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
        let mut steps = input.steps;
        // a date field's values: its part as a column, made by a one-part Split date (W2)
        if let Some(d) = self.pivot_builder.spec.date_fields.iter().find(|d| d.name == field) {
            steps.push(TransformStep::date_field_step(d));
        }
        // a lookup field's values: the lookup column joined to the table, with "(no match)" and "(blank)" (M1)
        let lookup = self.pivot_builder.spec.model_fields.iter().find(|m| m.name == field).cloned();
        let req = PivotValuesRequest { tab_id: input.tab_id, source: input.source, steps, field: field.to_string(), temp_dir: self.paths.temp_dir.clone(), lookup };
        let key = JobKey::tab(JobKind::PivotValues, input.tab_id);
        self.jobs.submit(key, "Values of a field", false, move |ctx| compute_pivot_values(req, ctx).map(|r| JobOutput::PivotValues(Box::new(r))));
    }

    fn kind_of(&self, field: &str) -> FieldKind {
        if let Some((_, k)) = self.pivot_builder.fields.iter().find(|(n, _)| n == field) {
            return *k;
        }
        // a lookup field (Phase 9 M1) is the kind its lookup column is
        let tab = self.pivot_input().map(|i| i.tab_id);
        tab.and_then(|t| self.model_field_named(t, field)).map(|(_, dt, _)| FieldKind::of(&dt)).unwrap_or(FieldKind::Other)
    }

    /// The value a field gets when it is added to Values: a sum of a number, a count of anything
    /// else; a rate is averaged by the pinned control column (a sum of rates means nothing).
    /// The project key of the tab the builder is for when it is a date-linked data tab (what its time aggregations and the stock
    /// warning read); empty otherwise (Phase 9 M3).
    pub(crate) fn pivot_data_tab(&self, tab_id: u64) -> String {
        let Some(key) = self.project.keys.get(&tab_id) else {
            return String::new();
        };
        if self.project.active && self.project.model.calendar.as_ref().is_some_and(|c| c.date_links.iter().any(|l| &l.data_tab == key)) {
            key.clone()
        } else {
            String::new()
        }
    }

    /// True when the builder's tab is a date-linked data tab, so a value can have a time aggregation.
    fn pivot_has_time_choices(&self) -> bool {
        self.pivot_input().is_some_and(|i| !self.pivot_data_tab(i.tab_id).is_empty())
    }

    /// The named measures of the builder's tab (Phase 9 M5), in the order the model has them.
    pub(crate) fn pivot_measures(&self) -> Vec<String> {
        let Some(input) = self.pivot_input() else { return Vec::new() };
        let key = self.pivot_data_tab(input.tab_id);
        if key.is_empty() {
            return Vec::new();
        }
        self.project.model.measures.iter().filter(|m| m.tab() == key && !self.pivot_builder.fields.iter().any(|(n, _)| n == m.name())).map(|m| m.name().to_string()).collect()
    }

    fn default_value_for(&self, field: &str) -> PivotValue {
        if self.pivot_measures().iter().any(|m| m == field) {
            return PivotValue { field: field.to_string(), agg: PivotAgg::Sum, weight_col: None, time: Some(TimeValue::of(TimeAgg::Measure)) };
        }
        let kind = self.kind_of(field);
        // a column tagged as a balance, a flow or a YTD balance gets the aggregation its tag sets (Phase 9 M3)
        if kind == FieldKind::Number {
            if let Some(input) = self.pivot_input() {
                let key = self.pivot_data_tab(input.tab_id);
                if let Some(tag) = (!key.is_empty()).then(|| self.column_tag(&key, field)).flatten() {
                    let agg = tag.default_agg();
                    if !self.pivot_builder.spec.values.iter().any(|v| v.field == field && v.time.is_some_and(|t| t.agg == agg)) {
                        return PivotValue { field: field.to_string(), agg: PivotAgg::Sum, weight_col: None, time: Some(TimeValue::of(agg)) };
                    }
                }
            }
        }
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
        PivotValue { field: field.to_string(), agg, weight_col: weight, time: None }
    }

    /// Puts a date part into Rows, Columns or Filters (W2): the field "Open Date · Month", with its calendar.
    /// A real column of that name is refused (the pivot says so).
    pub(crate) fn add_pivot_date_field(&mut self, f: micro_parquet::dates::DateFieldRef, area: Area) {
        if self.pivot_builder.fields.iter().any(|(n, _)| *n == f.name) {
            self.pivot_builder.preview = Some(Err(format!("Pivot: the table already has a column called '{}'. Rename that column, or use it instead of the date field.", f.name)));
            return;
        }
        let name = f.name.clone();
        if !self.pivot_builder.spec.date_fields.iter().any(|d| d.name == name) {
            self.pivot_builder.spec.date_fields.push(f);
        }
        self.add_pivot_field(&name, area);
    }

    /// Puts a field into an area (a field is in Rows or in Columns, not both).
    pub(crate) fn add_pivot_field(&mut self, field: &str, area: Area) {
        // a named measure (Phase 9 M5) is a value, never a label
        if area != Area::Values && self.pivot_measures().iter().any(|m| m == field) {
            self.pivot_builder.preview = Some(Err(format!("'{}' is a measure. Add it to Values.", field)));
            return;
        }
        // a lookup field (Phase 9 M1) is registered in the spec when it is first put in an area; a broken link's field is refused
        if !self.pivot_builder.fields.iter().any(|(n, _)| n == field) && !self.pivot_builder.spec.model_fields.iter().any(|m| m.name == field) {
            if let Some((r, _, refusal)) = self.pivot_input().and_then(|i| self.model_field_named(i.tab_id, field)) {
                if let Some(why) = refusal {
                    self.pivot_builder.preview = Some(Err(why));
                    return;
                }
                self.pivot_builder.spec.model_fields.push(r);
            }
        }
        match area {
            Area::Rows | Area::Columns => {
                // a list that names this field is pre-selected (U3)
                if let Some(list) = micro_parquet::sort_lists::suggested_for(&self.sort_lists.lists, field).cloned() {
                    let spec = &mut self.pivot_builder.spec;
                    if !spec.field_orders.iter().any(|o| o.field == field) {
                        spec.field_orders.push(PivotFieldOrder { field: field.to_string(), order: SortOrder::from_list(&list) });
                    }
                }
                let spec = &mut self.pivot_builder.spec;
                let added = !spec.rows.iter().chain(spec.columns.iter()).any(|f| f == field);
                spec.rows.retain(|f| f != field);
                spec.columns.retain(|f| f != field);
                if area == Area::Rows {
                    spec.rows.push(field.to_string());
                } else {
                    spec.columns.push(field.to_string());
                }
                // W0 auto order: a date field's own order, the steps' order for the column, else Calendar or
                // Ranges from its labels
                let date_field = self.pivot_builder.spec.date_fields.iter().find(|d| d.name == field).cloned();
                if added && !self.pivot_builder.spec.field_orders.iter().any(|o| o.field == field) {
                    let model_field = self.pivot_builder.spec.model_fields.iter().find(|m| m.name == field).cloned();
                    if let Some(d) = date_field {
                        if let Some(order) = d.sort_order() {
                            self.pivot_builder.spec.field_orders.push(PivotFieldOrder { field: field.to_string(), order });
                        }
                    } else if let Some(m) = model_field {
                        // a lookup field's order is what the lookup's own steps declare (W0 on the lookup); its labels are not listed here
                        if let Some(order) = self.model_declared_order(&m) {
                            self.pivot_builder.spec.field_orders.push(PivotFieldOrder { field: field.to_string(), order });
                        }
                    } else if let Some(order) = self.auto_order(field, crate::ui::auto_order::AutoTarget::Pivot) {
                        self.pivot_builder.spec.field_orders.push(PivotFieldOrder { field: field.to_string(), order });
                    }
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
        self.remove_pivot_chip_inner(area, index);
        let spec = &mut self.pivot_builder.spec;
        let (rows, cols) = (spec.rows.clone(), spec.columns.clone());
        spec.field_orders.retain(|o| rows.contains(&o.field) || cols.contains(&o.field));
        // a date field no area uses any more is dropped (W2)
        let filters: Vec<String> = spec.filters.iter().map(|f| f.field.clone()).collect();
        spec.date_fields.retain(|d| rows.contains(&d.name) || cols.contains(&d.name) || filters.contains(&d.name));
        // and so is a lookup field no area uses any more (M1)
        let values: Vec<String> = spec.values.iter().map(|v| v.field.clone()).collect();
        spec.model_fields.retain(|m| rows.contains(&m.name) || cols.contains(&m.name) || filters.contains(&m.name) || values.contains(&m.name));
    }

    fn remove_pivot_chip_inner(&mut self, area: Area, index: usize) {
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

    /// How many rows the preview on screen was cut to.
    fn pivot_preview_limit(&self) -> usize {
        match self.pivot_builder.preview_limit {
            0 => micro_parquet::tab_jobs::PIVOT_PREVIEW_ROWS,
            n => n,
        }
    }

    /// The line under the title: the tab, and the step being edited or the step the pivot goes after.
    fn pivot_subtitle(&self) -> String {
        match (self.pivot_input(), &self.editing) {
            (Some(i), Some(ed)) => format!("{} \u{b7} editing step {}", i.tab_title, ed.index + 1),
            (Some(i), None) if i.after_step > 0 => format!("{} \u{b7} after step {}", i.tab_title, i.after_step),
            (Some(i), None) => i.tab_title,
            _ => String::new(),
        }
    }

    /// The row under the title bar: the tab on the left, Pop out (docked) or Dock (in its own window) on the right.
    fn pivot_header_row(&mut self, ui: &mut egui::Ui, subtitle: &str, can_pop: bool, rects: &mut PivotRects, acts: &mut PivotActs) {
        let tk = *theme::tokens(self.dark_mode);
        let popped = self.pivot_builder.popped;
        ui.horizontal(|ui| {
            // the layout of the pivot (Phase 9 P2), at the left of the row: it is how the tab shows the pivot, not a setting of its numbers
            self.pivot_layout_controls(ui, rects);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if can_pop || popped {
                    let (name, label, tip) = if popped {
                        ("Dock", format!("{} Dock", egui_phosphor::regular::ARROW_SQUARE_IN), "Put the builder back inside the app window, as it is")
                    } else {
                        ("Pop out", format!("{} Pop out", egui_phosphor::regular::ARROW_SQUARE_OUT), "Show the builder in its own window, which you can move to another monitor or make bigger")
                    };
                    let b = ui.button(label).on_hover_text(tip);
                    rects.buttons.push((name.to_string(), b.rect));
                    if b.clicked() {
                        acts.toggle_pop = true;
                    }
                }
                ui.add(egui::Label::new(egui::RichText::new(subtitle).color(tk.muted)).truncate());
            });
        });
    }

    /// "Layout" with its box (Side by side, Nested (Excel style)) and, for Nested, "Blank line after each group" (Phase 9 P2).
    fn pivot_layout_controls(&mut self, ui: &mut egui::Ui, rects: &mut PivotRects) {
        let s = &mut self.pivot_builder.spec;
        ui.label("Layout");
        let nested_now = s.layout.is_nested();
        let combo = egui::ComboBox::from_id_salt("pivot_layout").width(theme::PIVOT_COMBO_LAYOUT_W).selected_text(s.layout.label()).show_ui(ui, |ui| {
            if ui.selectable_label(!nested_now, PivotLayout::SideBySide.label()).clicked() {
                s.layout = PivotLayout::SideBySide;
            }
            if ui.selectable_label(nested_now, PivotLayout::Nested { blank_line_after_group: false }.label()).clicked() && !nested_now {
                s.layout = PivotLayout::Nested { blank_line_after_group: false };
            }
        });
        rects.layout_combo = combo.response.rect;
        combo.response.on_hover_text("Nested puts all Rows fields in one column with each group's subtotal on its own line, as Excel's Compact form. The preview below and the tab show it the same way; exports of the data stay flat. Side by side previews the flat table.");
        if s.layout.is_nested() {
            let mut on = s.layout.blank_line();
            let b = ui.checkbox(&mut on, "Blank line after each group");
            rects.blank_line = b.rect;
            if b.changed() {
                s.layout = PivotLayout::Nested { blank_line_after_group: on };
            }
        }
    }

    /// The footer: the hint, Cancel and Create pivot tab (Save changes). The same docked and in its own window.
    fn pivot_footer_row(&mut self, ui: &mut egui::Ui, rects: &mut PivotRects, acts: &mut PivotActs) {
        let tk = *theme::tokens(self.dark_mode);
        let editing = self.editing.is_some();
        let can_create = self.pivot_builder_step().is_some();
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
                        acts.step = Some((s, self.pivot_builder.new_tab));
                        acts.close = true;
                    }
                }
                let cancel = ui.button("Cancel");
                rects.buttons.push(("Cancel".to_string(), cancel.rect));
                if cancel.clicked() {
                    acts.close = true;
                }
            });
        });
    }

    pub(crate) fn render_pivot_builder(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        if !self.pivot_builder.open {
            self.open_pivot_builder(None, self.editing.is_some());
        }
        // a backend without windows keeps the builder in the app
        if self.pivot_builder.popped && ctx.embed_viewports() {
            self.pivot_builder.popped = false;
            self.pivot_builder.popout_start = None;
        }
        self.pivot_builder_tick();
        let mut acts = PivotActs::default();
        if self.pivot_builder.popped {
            self.show_pivot_viewport(ctx, &mut acts);
            self.pivot_popout_note(ctx, &mut acts);
        } else {
            self.show_pivot_docked(ctx, &mut acts);
        }
        self.pivot_finish(acts)
    }

    /// The builder as a dialog in the app window.
    fn show_pivot_docked(&mut self, ctx: &egui::Context, acts: &mut PivotActs) {
        if !self.command_open && self.pivot_esc_cancels(ctx) {
            acts.close = true;
        }
        self.pivot_builder.menu_open = false;
        let subtitle = self.pivot_subtitle();
        let can_pop = !ctx.embed_viewports();
        let tk = *theme::tokens(self.dark_mode);
        let _ = tk;
        let title = format!("{}  Pivot table", theme::ph::TABLE);
        let mut rects = PivotRects::default();
        theme::modal(&title)
            .default_width(theme::DIALOG_W_PIVOT_BUILDER)
            .min_width(theme::DIALOG_MIN_W_PIVOT_BUILDER)
            .fill_height()
            .show(ctx, |d| {
                d.header(|ui| self.pivot_header_row(ui, &subtitle, can_pop, &mut rects, acts));
                let body_h = d.body_max_h();
                d.body_scroll(|ui| {
                    self.pivot_builder_body(ui, body_h, &mut rects);
                });
                d.footer(|ui| self.pivot_footer_row(ui, &mut rects, acts));
            });
        self.pivot_builder.rects = rects;
        self.pivot_builder.esc_busy = self.pivot_esc_holders(ctx) || self.pivot_builder.menu_open;
    }

    /// The builder in its own native window (an immediate viewport): the same state, header, fields, preview and footer.
    /// Closing the window docks it back.
    fn show_pivot_viewport(&mut self, ctx: &egui::Context, acts: &mut PivotActs) {
        if self.pivot_builder.popout_start.is_none() {
            let (main, monitor) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().monitor_size));
            self.pivot_builder.popout_start = Some(popout_placement(self.settings.pivot_popout_rect, main, monitor));
            // what an earlier window showed is not where this one is
            self.pivot_builder.popout_live = None;
        }
        let start = self.pivot_builder.popout_start.unwrap_or(PopoutPlacement { size: egui::vec2(theme::PIVOT_POPOUT_SIZE[0], theme::PIVOT_POPOUT_SIZE[1]), pos: None });
        let tab = self.pivot_input().map(|i| i.tab_title).unwrap_or_default();
        let mut builder = egui::ViewportBuilder::default()
            .with_title(format!("Pivot builder \u{2014} {tab}"))
            .with_inner_size(start.size)
            .with_min_inner_size(theme::PIVOT_POPOUT_MIN_SIZE)
            .with_resizable(true)
            .with_maximize_button(true)
            .with_minimize_button(true);
        if let Some(p) = start.pos {
            builder = builder.with_position(p);
        }
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("pivot_popout"), builder, |ctx, class| {
            if class == egui::ViewportClass::Embedded {
                return;
            }
            self.pivot_popout_ui(ctx, acts);
        });
    }

    /// Esc is Cancel, docked and in the window alike (owner decision on P1-3). It does nothing to the builder while a text box has the
    /// keyboard, the Filters list is open, or a drop-down list or pop-up is open: Esc closes that first, and the next Esc is Cancel.
    /// Read before the builder is drawn, so what takes Esc itself does not also cancel.
    fn pivot_esc_cancels(&self, ctx: &egui::Context) -> bool {
        // egui lets go of a text box's keyboard when Esc arrives, so the last frame's state counts too
        let busy = self.pivot_esc_holders(ctx) || self.pivot_builder.esc_busy;
        !busy && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    }

    /// Something that takes Esc itself is open now: a text box with the keyboard, the Filters list, a drop-down list or a pop-up.
    fn pivot_esc_holders(&self, ctx: &egui::Context) -> bool {
        ctx.wants_keyboard_input() || ctx.memory(|m| m.any_popup_open()) || self.pivot_builder.popup.is_some()
    }

    /// What fills the builder's own window: the header, the fields and areas with a preview that takes the height that is left,
    /// and the footer. (Tests draw it in a headless window of its size.) Esc is Cancel (`pivot_esc_cancels`).
    pub(crate) fn pivot_popout_ui(&mut self, ctx: &egui::Context, acts: &mut PivotActs) {
        // the window's close button docks the builder back, as its Dock button does
        if ctx.input(|i| i.viewport().close_requested()) {
            acts.toggle_pop = true;
            return;
        }
        ctx.input(|i| {
            let v = i.viewport();
            if let (Some(o), Some(inner)) = (v.outer_rect, v.inner_rect) {
                self.pivot_builder.popout_live = Some([o.min.x, o.min.y, inner.width(), inner.height()]);
            }
        });
        if self.pivot_esc_cancels(ctx) {
            acts.close = true;
        }
        self.pivot_builder.menu_open = false;
        let subtitle = self.pivot_subtitle();
        let mut rects = PivotRects::default();
        egui::TopBottomPanel::top("pivot_popout_header").show(ctx, |ui| self.pivot_header_row(ui, &subtitle, true, &mut rects, acts));
        egui::TopBottomPanel::bottom("pivot_popout_footer").show(ctx, |ui| self.pivot_footer_row(ui, &mut rects, acts));
        egui::CentralPanel::default().show(ctx, |ui| {
            let body_h = ui.available_height();
            egui::ScrollArea::vertical().auto_shrink([false, false]).scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible).show(ui, |ui| {
                self.pivot_builder_body(ui, body_h, &mut rects);
            });
        });
        self.pivot_builder.rects = rects;
        self.pivot_builder.esc_busy = self.pivot_esc_holders(ctx) || self.pivot_builder.menu_open;
    }

    /// While the builder is in its own window, the app window says so and can take it back (the window may be on another monitor,
    /// or lost behind another program).
    fn pivot_popout_note(&mut self, ctx: &egui::Context, acts: &mut PivotActs) {
        let tk = *theme::tokens(self.dark_mode);
        let mut dock = egui::Rect::NOTHING;
        theme::modal(PIVOT_POPOUT_NOTE_TITLE).default_width(theme::DIALOG_W_PIVOT_CONVERT).show(ctx, |d| {
            d.body(|ui| {
                ui.add(egui::Label::new(egui::RichText::new("The pivot builder is in its own window.").color(tk.muted)).wrap());
            });
            d.footer(|ui| {
                let b = ui.button("Dock");
                dock = b.rect;
                if b.clicked() {
                    acts.toggle_pop = true;
                }
            });
        });
        self.pivot_builder.rects.note_dock = dock;
    }

    /// What a frame asked for: move the builder, close it, or give back the step.
    pub(crate) fn pivot_finish(&mut self, acts: PivotActs) -> ModalOutcome {
        let was = self.pivot_builder.popped;
        if acts.toggle_pop {
            // the other place has its own keyboard and lists
            self.pivot_builder.esc_busy = false;
        }
        if acts.toggle_pop && !acts.close {
            self.pivot_builder.popped = !was;
            if !was {
                self.pivot_builder.pop_requests += 1;
            }
        }
        if was && (acts.toggle_pop || acts.close) {
            self.pivot_save_popout_rect();
            self.pivot_builder.popout_start = None;
        }
        if acts.close {
            self.pivot_builder.open = false;
            self.pivot_builder.popup = None;
            self.pivot_builder.popped = false;
        }
        ModalOutcome { close: acts.close, step: acts.step }
    }

    /// Keeps where the builder's window is in `settings.json`.
    pub(crate) fn pivot_save_popout_rect(&mut self) {
        if let Some(r) = self.pivot_builder.popout_live {
            if self.settings.pivot_popout_rect != Some(r) {
                self.settings.pivot_popout_rect = Some(r);
                self.save_settings();
            }
        }
    }

    /// One frame of the builder's own window in a window of its own, with what the app does after it (tests: they have no second window).
    #[cfg(test)]
    pub(crate) fn pivot_popout_frame(&mut self, ctx: &egui::Context) {
        self.pivot_builder_tick();
        let mut acts = PivotActs::default();
        self.pivot_popout_ui(ctx, &mut acts);
        let outcome = self.pivot_finish(acts);
        self.apply_modal_outcome(None, outcome);
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
                self.pivot_field_row(ui, name, *kind, None, None, rects);
                if *kind == FieldKind::Date {
                    // W2: the parts of a date column, under an expander, added like columns
                    let open = self.pivot_builder.expanded.contains(name);
                    let text = format!("{}  Date parts", if open { theme::ph::CARET_DOWN } else { theme::ph::CARET_RIGHT });
                    let r = ui.add(egui::Button::new(egui::RichText::new(text).size(theme::TEXT_LABEL).color(tk.muted)).frame(false));
                    rects.expanders.push((name.clone(), r.rect));
                    if r.clicked() {
                        if open {
                            self.pivot_builder.expanded.remove(name);
                        } else {
                            self.pivot_builder.expanded.insert(name.clone());
                        }
                    }
                    if open {
                        let cal = self.calendar();
                        for part in micro_parquet::dates::DatePart::ALL {
                            let f = self.pivot_builder.spec.date_fields.iter().find(|d| d.column == *name && d.part == part).cloned().unwrap_or_else(|| micro_parquet::dates::DateFieldRef::new(name, part, cal));
                            self.pivot_field_row(ui, &f.name.clone(), *kind, Some(&f), None, rects);
                        }
                    }
                }
            }
            // Phase 9 M1: the links of the tab, under its own columns: "Branch master ›" expands to the lookup's columns
            let tab_id = self.pivot_input().map(|i| i.tab_id);
            let groups = tab_id.map(|t| self.model_groups_for(t)).unwrap_or_default();
            let mut shown_any = !list.is_empty();
            for g in &groups {
                let key = format!("{} \u{203a}", g.lookup_title);
                let matching: Vec<(micro_parquet::model::ModelFieldRef, polars::prelude::DataType)> = g.columns.iter().filter(|(r, _)| q.is_empty() || r.name.to_lowercase().contains(&q) || g.lookup_title.to_lowercase().contains(&q)).cloned().collect();
                if !q.is_empty() && matching.is_empty() {
                    continue;
                }
                shown_any = true;
                let open = !q.is_empty() || self.pivot_builder.expanded.contains(&key);
                let text = format!("{}  {}", if open { theme::ph::CARET_DOWN } else { theme::ph::CARET_RIGHT }, key);
                let colour = if g.refusal.is_some() { tk.muted } else { tk.muted };
                let r = ui.add(egui::Button::new(egui::RichText::new(text).size(theme::TEXT_LABEL).color(colour)).frame(false));
                let r = match &g.refusal {
                    Some(why) => r.on_hover_text(why),
                    None => r.on_hover_text(format!("The columns of '{}', joined by the link {}", g.lookup_title, g.link_name)),
                };
                rects.expanders.push((key.clone(), r.rect));
                if r.clicked() {
                    if self.pivot_builder.expanded.contains(&key) {
                        self.pivot_builder.expanded.remove(&key);
                    } else {
                        self.pivot_builder.expanded.insert(key.clone());
                    }
                }
                if open {
                    if let Some(why) = &g.refusal {
                        let note = ui.add(egui::Label::new(egui::RichText::new(why).size(theme::TEXT_LABEL).color(tk.warning)).wrap());
                        rects.link_notes.push((key.clone(), note.rect));
                    }
                    for (r, dt) in &matching {
                        self.pivot_field_row(ui, &r.name.clone(), FieldKind::of(dt), None, Some((r, g.refusal.as_deref())), rects);
                    }
                }
            }
            // Phase 9 M5: the named measures of the tab, under "Measures \u{203a}"; each is added to Values
            let measures: Vec<String> = self.pivot_measures().into_iter().filter(|m| q.is_empty() || m.to_lowercase().contains(&q) || "measures".contains(&q)).collect();
            if !measures.is_empty() {
                shown_any = true;
                let key = "Measures \u{203a}".to_string();
                let open = !q.is_empty() || self.pivot_builder.expanded.contains(&key);
                let text = format!("{}  {}", if open { theme::ph::CARET_DOWN } else { theme::ph::CARET_RIGHT }, key);
                let r = ui.add(egui::Button::new(egui::RichText::new(text).size(theme::TEXT_LABEL).color(tk.muted)).frame(false)).on_hover_text("The named measures of this tab (Model \u{25b8} Measures\u{2026}). They go to Values.");
                rects.expanders.push((key.clone(), r.rect));
                if r.clicked() {
                    if self.pivot_builder.expanded.contains(&key) {
                        self.pivot_builder.expanded.remove(&key);
                    } else {
                        self.pivot_builder.expanded.insert(key.clone());
                    }
                }
                if open {
                    for m in &measures {
                        self.pivot_field_row(ui, m, FieldKind::Number, None, None, rects);
                    }
                }
            }
            if !shown_any {
                ui.label(egui::RichText::new("No field matches.").color(tk.muted));
            }
        });
        rects.fields_list = out.inner_rect;
    }

    fn pivot_field_row(&mut self, ui: &mut egui::Ui, name: &str, kind: FieldKind, part: Option<&micro_parquet::dates::DateFieldRef>, model: Option<(&micro_parquet::model::ModelFieldRef, Option<&str>)>, rects: &mut PivotRects) {
        let tk = *theme::tokens(self.dark_mode);
        let is_measure = part.is_none() && model.is_none() && self.pivot_measures().iter().any(|m| m == name);
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
        // a date part is listed under its column as "› Month"
        let shown = match (part, model) {
            (Some(p), _) => format!("\u{203a} {}", p.part.label()),
            // a lookup column is listed under its link as "› Region" (M1)
            (None, Some((m, _))) => format!("\u{203a} {}", m.column),
            _ => name.to_string(),
        };
        // a field of a broken link is greyed, and says why on hover
        let refusal = model.and_then(|(_, why)| why);
        let ink = if refusal.is_some() { tk.muted } else { tk.ink };
        let label = ui.put(name_rect, egui::Label::new(egui::RichText::new(shown).color(ink)).truncate().selectable(false));
        if let Some(why) = refusal {
            label.on_hover_text(why);
        }
        // the menu: Add to Rows / Columns / Values / Filters
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(menu_rect), |ui| {
            // a button as small as its cell: the default minimum size would push the row wider
            ui.spacing_mut().interact_size = egui::vec2(theme::PIVOT_CHIP_MENU_W, theme::PIVOT_FIELD_ROW_H);
            ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
            let bar_id = ui.id();
            let mut draw_menu = |ui: &mut egui::Ui| {
            let menu = ui.menu_button(theme::ph::DOTS_THREE, |ui| {
                // a date part is a label: Rows, Columns or Filters (W2)
                let areas_offered: &[Area] = if part.is_some() {
                    &[Area::Rows, Area::Columns, Area::Filters]
                } else if is_measure {
                    &[Area::Values]
                } else {
                    &[Area::Rows, Area::Columns, Area::Values, Area::Filters]
                };
                for area in areas_offered {
                    if ui.button(format!("Add to {}", area.title())).clicked() {
                        if let Some(p) = part {
                            self.add_pivot_date_field(p.clone(), *area);
                        } else {
                            self.add_pivot_field(name, *area);
                        }
                        ui.close_menu();
                    }
                }
                let in_rows = areas.contains(&Area::Rows);
                if in_rows || areas.contains(&Area::Columns) {
                    ui.separator();
                    self.pivot_order_menu(ui, name, in_rows);
                }
            });
            // an open menu takes Esc itself (egui's menus are not in its list of pop-ups)
            self.pivot_builder.menu_open = self.pivot_builder.menu_open || egui::menu::BarState::load(ui.ctx(), bar_id).is_some();
            menu.response.on_hover_text("Add to Rows, Columns, Values or Filters");
            };
            // a field of a broken link has its menu disabled (M1); the others are drawn as before
            if refusal.is_some() {
                ui.add_enabled_ui(false, |ui| draw_menu(ui));
            } else {
                draw_menu(ui);
            }
        });
        rects.field_rows.push((name.to_string(), row));
        rects.field_menus.push((name.to_string(), menu_rect));
    }

    /// "Order: Natural / <list> / Tenor / By value" for a field of Rows or Columns (U3).
    fn pivot_order_menu(&mut self, ui: &mut egui::Ui, field: &str, is_row: bool) {
        let current = self.pivot_builder.spec.field_orders.iter().find(|o| o.field == field).map(|o| o.order.clone()).unwrap_or_default();
        let by_value = is_row && self.pivot_builder.spec.sort_by_value.is_some();
        let shown = if by_value { "By value".to_string() } else { current.label() };
        let mut choice: Option<SortOrder> = None;
        let mut value_sort = false;
        ui.menu_button(format!("Order: {}", shown), |ui| {
            if ui.selectable_label(!by_value && current.is_natural(), "Natural").clicked() {
                choice = Some(SortOrder::Natural);
                ui.close_menu();
            }
            for list in self.sort_lists.lists.clone() {
                let o = SortOrder::from_list(&list);
                if ui.selectable_label(!by_value && current == o, &list.name).clicked() {
                    choice = Some(o);
                    ui.close_menu();
                }
            }
            if ui.selectable_label(!by_value && current == SortOrder::Tenor, "Tenor").clicked() {
                choice = Some(SortOrder::Tenor);
                ui.close_menu();
            }
            for o in self.builtin_orders() {
                if ui.selectable_label(!by_value && current == o, o.label()).clicked() {
                    choice = Some(o);
                    ui.close_menu();
                }
            }
            if is_row && !self.pivot_builder.spec.values.is_empty() && ui.selectable_label(by_value, "By value").clicked() {
                value_sort = true;
                ui.close_menu();
            }
        });
        let spec = &mut self.pivot_builder.spec;
        if let Some(o) = choice {
            spec.field_orders.retain(|x| x.field != field);
            if !o.is_natural() {
                spec.field_orders.push(PivotFieldOrder { field: field.to_string(), order: o });
            }
            if is_row {
                spec.sort_by_value = None;
            }
        }
        if value_sort {
            spec.sort_by_value = Some(PivotSortByValue { value_index: 0, descending: true });
        }
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
            if area == Area::Filters && spec_snapshot.filters.is_empty() {
                ui.add(egui::Label::new(egui::RichText::new(FILTERS_EMPTY_HINT).size(theme::TEXT_LABEL).color(tk.muted)).truncate());
            }
            enum Chip {
                Plain(String, String),
                Value(usize),
            }
            let chips: Vec<Chip> = match area {
                Area::Rows | Area::Columns => {
                    let fields = if area == Area::Rows { &spec_snapshot.rows } else { &spec_snapshot.columns };
                    fields
                        .iter()
                        .map(|f| {
                            let extra = match spec_snapshot.field_orders.iter().find(|o| o.field == *f) {
                                Some(o) if !o.order.is_natural() => format!("order: {}", o.order.label()),
                                _ => String::new(),
                            };
                            Chip::Plain(f.clone(), extra)
                        })
                        .collect()
                }
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
                                let (rect, rm, agg_rect) = self.value_chip(ui, *j, &v, widths[i], tk);
                                rects.agg_combos.push((*j, agg_rect));
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
    fn value_chip(&mut self, ui: &mut egui::Ui, i: usize, v: &PivotValue, w: f32, tk: &theme::Tokens) -> (egui::Rect, bool, egui::Rect) {
        let numeric = self.kind_of(&v.field) == FieldKind::Number;
        let numbers: Vec<String> = self.pivot_builder.fields.iter().filter(|(_, k)| *k == FieldKind::Number).map(|(n, _)| n.clone()).collect();
        let mut agg = v.agg;
        let mut time = v.time;
        let time_choices = self.pivot_has_time_choices();
        let mut agg_rect = egui::Rect::NOTHING;
        let mut weight = v.weight_col.clone();
        let mut removed = false;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, theme::PIVOT_CHIP_H), egui::Sense::hover());
        ui.painter().rect(rect, theme::RADIUS_CONTROL, tk.surface, theme::line(tk.accent_soft_border));
        let inner = egui::Rect::from_min_max(egui::pos2(rect.left() + theme::SPACE_S, rect.top()), egui::pos2(rect.right() - theme::SPACE_XS, rect.bottom()));
        let fixed = value_chip_fixed_width(agg, time);
        let label_w = (inner.width() - fixed).max(0.0);
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_S;
            ui.spacing_mut().interact_size.y = theme::PIVOT_CHIP_H - 2.0;
            ui.allocate_ui_with_layout(egui::vec2(label_w, theme::PIVOT_CHIP_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(egui::RichText::new(&v.field).font(egui::FontId::new(theme::TEXT_BASE, theme::family_semibold())).color(tk.accent_dark)).truncate());
            });
            // the list opens under the box and must end inside the window: it scrolls when the room is short
            let list_h = (ui.ctx().screen_rect().bottom() - ui.cursor().top() - theme::PIVOT_AGG_LIST_MARGIN).clamp(theme::PIVOT_AGG_LIST_MIN_H, theme::PIVOT_AGG_LIST_H);
            let box_w = if time.is_some() { theme::PIVOT_COMBO_TIME_W } else { theme::PIVOT_COMBO_AGG_W };
            let is_measure = time.is_some_and(|t| t.agg == TimeAgg::Measure);
            let shown = match time {
                Some(t) if is_measure => "Measure",
                Some(t) => t.agg.label(),
                None => agg.label(),
            };
            let combo = egui::ComboBox::from_id_salt(("pivot_agg", i)).width(box_w - ui.spacing().interact_size.y).height(list_h).selected_text(shown).show_ui(ui, |ui| {
                if is_measure {
                    ui.label(egui::RichText::new("A named measure: edit it in Model \u{25b8} Measures\u{2026}").color(tk.muted));
                    return;
                }
                for a in PivotAgg::all() {
                    if numeric || matches!(a, PivotAgg::Count | PivotAgg::CountDistinct) {
                        if ui.selectable_label(time.is_none() && agg == *a, a.label()).clicked() {
                            agg = *a;
                            time = None;
                        }
                    }
                }
                // on a date-linked data tab: the aggregations over time (Phase 9 M3)
                if time_choices {
                    ui.separator();
                    for t in TimeAgg::all() {
                        if (numeric || !t.needs_number()) && ui.selectable_label(time.map(|x| x.agg) == Some(*t), t.label()).clicked() {
                            time = Some(TimeValue { agg: *t, ..time.unwrap_or_default() });
                        }
                    }
                }
            });
            agg_rect = combo.response.rect;
            if let Some(mut t) = time {
                let label = if t.is_m4() { "Time \u{2022}" } else { "Time" };
                let btn = ui.add_sized(egui::vec2(theme::PIVOT_TIME_BTN_W, theme::PIVOT_TIME_BTN_H), egui::Button::new(egui::RichText::new(label).size(theme::TEXT_LABEL)));
                btn.clone().on_hover_text("Window, shift and what the value shows (MTD, QTD, YTD, prior period, same period last year)");
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("pivot_time_btn", i)), btn.rect));
                let pid = egui::Id::new(("pivot_time_popup", i));
                if btn.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(pid));
                }
                let mut choices: Vec<(String, egui::Rect)> = Vec::new();
                egui::popup::popup_below_widget(ui, pid, &btn, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                    use micro_parquet::models::{TimePartial, TimeShift, TimeShow, TimeWindow};
                    ui.set_min_width(theme::PIVOT_TIME_POPUP_W);
                    let mut pick = |ui: &mut egui::Ui, group: &str, label: &str, on: bool, enabled: bool, choices: &mut Vec<(String, egui::Rect)>| -> bool {
                        let r = ui.add_enabled(enabled, egui::SelectableLabel::new(on, label));
                        choices.push((format!("{group}:{label}"), r.rect));
                        r.clicked()
                    };
                    egui::Grid::new(("pivot_time_grid", i)).num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_XS)).show(ui, |ui| {
                        ui.label(egui::RichText::new("Window").color(tk.muted));
                        ui.horizontal(|ui| {
                            for w in TimeWindow::all() {
                                if pick(ui, "window", w.label(), t.window == *w, true, &mut choices) {
                                    t.window = *w;
                                }
                            }
                        });
                        ui.end_row();
                        ui.label(egui::RichText::new("Shift").color(tk.muted));
                        ui.horizontal(|ui| {
                            for x in TimeShift::all() {
                                if pick(ui, "shift", x.label(), t.shift == *x, true, &mut choices) {
                                    t.shift = *x;
                                    if *x == TimeShift::None {
                                        t.show = TimeShow::Value;
                                    }
                                }
                            }
                        });
                        ui.end_row();
                        ui.label(egui::RichText::new("Show").color(tk.muted));
                        ui.horizontal(|ui| {
                            for x in TimeShow::all() {
                                if pick(ui, "show", x.label(), t.show == *x, *x == TimeShow::Value || t.shift != TimeShift::None, &mut choices) {
                                    t.show = *x;
                                }
                            }
                        });
                        ui.end_row();
                        ui.label(egui::RichText::new("Partial period").color(tk.muted));
                        ui.horizontal(|ui| {
                            for x in TimePartial::all() {
                                if pick(ui, "partial", x.label(), t.partial == *x, t.shift != TimeShift::None, &mut choices) {
                                    t.partial = *x;
                                }
                            }
                        });
                        ui.end_row();
                    });
                    ui.label(egui::RichText::new(if t.shift == TimeShift::None { "A difference or % change needs a shift." } else { "Same days: the shifted window has the same day numbers; Full period: all of it." }).size(theme::TEXT_LABEL).color(tk.muted));
                });
                let open = ui.memory(|m| m.is_popup_open(pid));
                ui.ctx().data_mut(|d| {
                    if open {
                        d.insert_temp(egui::Id::new("pivot_time_choices"), choices);
                    } else {
                        d.remove_temp::<Vec<(String, egui::Rect)>>(egui::Id::new("pivot_time_choices"));
                    }
                });
                time = Some(t);
            }
            let wavg = time.map(|t| t.agg == TimeAgg::WAvgAtClosing).unwrap_or(agg == PivotAgg::WeightedAvg);
            if wavg {
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
        if agg != v.agg || weight != v.weight_col || time != v.time {
            if let Some(slot) = self.pivot_builder.spec.values.get_mut(i) {
                slot.agg = agg;
                slot.time = time;
                let wavg = time.map(|t| t.agg == TimeAgg::WAvgAtClosing).unwrap_or(agg == PivotAgg::WeightedAvg);
                if wavg && weight.is_none() {
                    weight = numbers.iter().find(|n| **n != slot.field).cloned();
                }
                slot.weight_col = if wavg { weight } else { None };
            }
        }
        (rect, removed, agg_rect)
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
                        let shown = self.pivot_builder.preview_total_rows.min(self.pivot_preview_limit());
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
                // what the numbers need to be read with: the stock warning (Phase 9 M3)
                for w in self.pivot_builder.preview_warnings.clone() {
                    let r = ui.add(egui::Label::new(egui::RichText::new(format!("{}  {}", theme::ph::WARNING, w)).size(theme::TEXT_LABEL).color(tk.warning)).wrap());
                    rects.warnings.push(r.rect);
                }
                let left = ui.available_height().min(h);
                let area_h = (left - theme::SPACE_XS).max(theme::DATA_ROW_H);
                let out = egui::ScrollArea::both().id_salt("pivot_preview").max_height(area_h).auto_shrink([false, false]).show(ui, |ui| match &self.pivot_builder.preview {
                    Some(Ok(df)) => match (&self.pivot_builder.preview_nested, matches!(self.pivot_builder.spec.layout, PivotLayout::Nested { .. })) {
                        (Some(view), true) => self.pivot_preview_nested_grid(ui, view, tk),
                        _ => self.pivot_preview_grid(ui, df, tk),
                    },
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
        let has_total_row = spec.column_totals && spec.rows.len() > 0 && self.pivot_builder.preview_total_rows <= self.pivot_preview_limit();
        self.pivot_builder.notes_drawn.borrow_mut().clear();
        let draw_row = |ui: &mut egui::Ui, row_at: Option<usize>, texts: &[String], head: bool, fill: Option<egui::Color32>, strong: bool| {
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
                // a blank cell of a time aggregation says why on hover (Phase 9 M3)
                if let (Some(r), true, true) = (row_at, value, t == "\u{2014}") {
                    if let Some(n) = self.pivot_builder.preview_notes.iter().find(|n| n.row == r && n.column == names[c]) {
                        ui.interact(cell, ui.id().with(("pivot_note", r, c)), egui::Sense::hover()).on_hover_text(&n.text);
                        self.pivot_builder.notes_drawn.borrow_mut().push((n.text.clone(), cell));
                    }
                }
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
        draw_row(ui, None, &names, true, Some(tk.zebra), true);
        for (r, row) in cells.iter().enumerate() {
            let total = has_total_row && r == last;
            let fill = if total { Some(tk.accent_soft) } else if r % 2 == 1 { Some(tk.zebra) } else { None };
            draw_row(ui, Some(r), row, false, fill, total);
        }
    }

    /// The preview of the Nested layout: one "Row Labels" column, a parent line and the Grand total in bold, the indent of each level,
    /// the blank lines when chosen. It is the view the worker built for the tab's Nested layout, drawn the same way.
    fn pivot_preview_nested_grid(&self, ui: &mut egui::Ui, view: &micro_parquet::pivot_view::NestedView, tk: &theme::Tokens) {
        let spec = &self.pivot_builder.spec;
        let names: Vec<String> = view.values.get_column_names().iter().map(|n| n.to_string()).collect();
        let formats: HashMap<String, ColumnFormatSpec> = pivot_column_formats(spec, &names, self.global_decimals).into_iter().collect();
        let cells: Vec<Vec<String>> = (0..view.rows.len())
            .map(|r| {
                if view.rows[r].is_blank() {
                    return vec![String::new(); names.len()];
                }
                names.iter().enumerate().map(|(c, n)| view.values.get_columns().get(c).and_then(|s| s.get(r).ok()).map(|v| self.pivot_cell_text(n, v, formats.get(n))).unwrap_or_default()).collect()
            })
            .collect();
        let mono = egui::FontId::new(theme::TEXT_MONO, egui::FontFamily::Monospace);
        let head_font = egui::FontId::new(theme::TEXT_BASE, theme::family_semibold());
        let deepest = view.rows.iter().map(|r| r.level).max().unwrap_or(0) as f32 * theme::NESTED_INDENT_PX;
        let text_w = |t: &str, f: &egui::FontId| ui.fonts(|fo| fo.layout_no_wrap(t.to_string(), f.clone(), tk.ink).size().x);
        let label_body = view.rows.iter().map(|r| text_w(&r.label, &head_font)).fold(0.0, f32::max);
        let label_w = (text_w(&view.label_header, &head_font).max(label_body) + deepest + 2.0 * theme::SPACE_M).max(theme::PIVOT_PREVIEW_LABEL_W);
        let mut widths = vec![label_w];
        for (c, n) in names.iter().enumerate() {
            let body = cells.iter().map(|row| text_w(&row[c], &mono)).fold(0.0, f32::max);
            widths.push((text_w(n, &head_font).max(body) + 2.0 * theme::SPACE_M).max(theme::PIVOT_PREVIEW_COL_W));
        }
        let total_w: f32 = widths.iter().sum();
        let draw_row = |ui: &mut egui::Ui, level: usize, label: &str, texts: &[String], head: bool, fill: Option<egui::Color32>, bold: bool| {
            let (row, _) = ui.allocate_exact_size(egui::vec2(total_w, theme::PIVOT_PREVIEW_ROW_H), egui::Sense::hover());
            if let Some(f) = fill {
                ui.painter().rect_filled(row, 0.0, f);
            }
            let mut x = row.left();
            for c in 0..widths.len() {
                let cell = egui::Rect::from_min_size(egui::pos2(x, row.top()), egui::vec2(widths[c], row.height()));
                x += widths[c];
                let mut inner = cell.shrink2(egui::vec2(theme::SPACE_M, 0.0));
                let (t, layout) = if c == 0 {
                    inner.min.x += level as f32 * theme::NESTED_INDENT_PX;
                    (label, egui::Layout::left_to_right(egui::Align::Center))
                } else {
                    (texts[c - 1].as_str(), egui::Layout::right_to_left(egui::Align::Center))
                };
                if t.is_empty() {
                    continue;
                }
                let rich = if head {
                    egui::RichText::new(t).font(head_font.clone()).color(tk.ink)
                } else if c > 0 {
                    egui::RichText::new(t).font(if bold { egui::FontId::new(theme::TEXT_MONO, theme::family_mono_semibold()) } else { mono.clone() }).color(tk.ink)
                } else if bold {
                    egui::RichText::new(t).family(theme::family_bold()).size(theme::TEXT_BASE).color(tk.ink)
                } else {
                    egui::RichText::new(t).color(tk.ink)
                };
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(inner).layout(layout), |ui| {
                    ui.add(egui::Label::new(rich).truncate());
                });
            }
        };
        draw_row(ui, 0, &view.label_header, &names, true, Some(tk.zebra), true);
        for (r, line) in view.rows.iter().enumerate() {
            let fill = if line.is_grand_total() { Some(tk.accent_soft) } else { None };
            draw_row(ui, line.level, &line.label, &cells[r], false, fill, line.is_bold());
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
fn value_chip_fixed_width(agg: PivotAgg, time: Option<TimeValue>) -> f32 {
    let box_w = if time.is_some() { theme::PIVOT_COMBO_TIME_W } else { theme::PIVOT_COMBO_AGG_W };
    let mut w = theme::SPACE_S + box_w + theme::SPACE_S + theme::PIVOT_CHIP_MENU_W;
    if time.is_some() {
        w += theme::SPACE_S + theme::PIVOT_TIME_BTN_W;
    }
    if time.map(|t| t.agg == TimeAgg::WAvgAtClosing).unwrap_or(agg == PivotAgg::WeightedAvg) {
        w += theme::SPACE_S + theme::PIVOT_BADGE_W + theme::SPACE_S + theme::PIVOT_COMBO_WEIGHT_W;
    }
    w
}

/// The natural width of a chip of Values.
fn value_chip_width(ui: &egui::Ui, v: &PivotValue, _extra: f32) -> f32 {
    2.0 * theme::SPACE_S + chip_text_w(ui, &v.field, theme::TEXT_BASE, theme::family_semibold()) + value_chip_fixed_width(v.agg, v.time)
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
    ui.put(name_rect, egui::Label::new(egui::RichText::new(name).font(egui::FontId::new(theme::TEXT_BASE, theme::family_semibold())).color(tk.accent_dark)).truncate().selectable(false));
    let removed = ui
        .put(cross, egui::Button::new(egui::RichText::new(theme::ph::X).size(theme::TEXT_MARK).color(tk.muted)).frame(false))
        .on_hover_text("Remove field")
        .clicked();
    (resp.clicked() && !removed, removed, rect)
}
