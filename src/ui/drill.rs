//! Drill-down and "Show rows" of charts (Phase 8 W6). A click on a mark goes one level down, a right-click opens the menu
//! (Drill down, Show rows, Keep only this, Exclude this), a breadcrumb above the chart goes back up, and so does the Back key.
//! The studio's chart (side panel and pop-out) and a chart tab both use these; the numbers come from the same calculation
//! as every chart (`charts::calc`), whose rows are read by the drill path and the chart's own filters.

use crate::ui::chart_render::{ChartAction, DrillCtx};
use crate::ui::prelude::*;
use micro_parquet::charts::data::MarkRef;
use micro_parquet::charts::spec::ChartSpec;

/// The breadcrumb above a drilled chart: "All › FDR › Dhaka". The levels left behind are buttons that go back to them;
/// the level the chart is at is plain text. Returns the depth that was clicked. Nothing is drawn for a chart without drill.
pub(crate) fn drill_breadcrumb(ui: &mut egui::Ui, dark: bool, spec: &ChartSpec, rects: &mut Vec<(String, egui::Rect)>) -> Option<usize> {
    if !spec.has_drill() {
        return None;
    }
    let tk = *theme::tokens(dark);
    let depth = spec.depth();
    let mut names: Vec<String> = vec!["All".to_string()];
    names.extend(spec.drill_path.iter().map(|(_, v)| v.clone()));
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        for (i, n) in names.iter().enumerate() {
            if i > 0 {
                ui.label(egui::RichText::new("\u{203a}").color(tk.muted));
            }
            if i == depth {
                let l = ui.label(theme::bold(n).color(tk.ink));
                rects.push((format!("Level {i}"), l.rect));
            } else {
                let b = ui.add(egui::Button::new(egui::RichText::new(n).color(tk.accent)).frame(false)).on_hover_text("Go back to this level");
                rects.push((format!("Level {i}"), b.rect));
                if b.clicked() {
                    picked = Some(i);
                }
            }
        }
        // the level the chart shows now, so the last crumb is not mistaken for it
        if let Some(l) = spec.current_level() {
            ui.label(egui::RichText::new(format!("\u{b7} by {}", l.name)).color(tk.muted));
        }
    });
    picked
}

impl MicroParquetApp {
    /// What a click on a mark of `spec` may do. `rows_unavailable` greys Show rows with its reason; `blocked` greys the
    /// whole menu (a chart tab that cannot be calculated again).
    pub(crate) fn drill_ctx(&self, spec: &ChartSpec, rows_unavailable: Option<String>, blocked: Option<String>) -> DrillCtx {
        let can_down = spec.has_drill() && !spec.at_last_level() && blocked.is_none();
        DrillCtx {
            kind_ok: spec.kind.drill_reason().is_none(),
            has_drill: spec.has_drill(),
            can_down,
            next_level: spec.drill.get(spec.depth() + 1).map(|l| l.name.clone()).unwrap_or_default(),
            rows_unavailable,
            blocked,
        }
    }

    /// Why a chart tab cannot drill, filter or show rows (its source is closed, or it was made before it had one).
    pub(crate) fn chart_tab_blocked(&self, id: u64) -> Option<String> {
        let info = self.chart_tabs.get(&id)?;
        if let Some(c) = &info.closed {
            return Some(format!("The source tab '{}' is closed. Reopen it to drill into or filter this chart.", c.title));
        }
        match info.source_tab {
            None => Some("This chart tab was made by an older version and is never calculated again, so it cannot drill or filter. Use Edit chart.".to_string()),
            Some(s) if !self.tabs.iter().any(|t| t.id == s) => Some("The source tab is still being opened. Try again in a moment.".to_string()),
            Some(_) => None,
        }
    }

    /// The drill context of the studio's chart.
    pub(crate) fn studio_drill_ctx(&self) -> DrillCtx {
        let rows = if self.chart.tab_id.is_some_and(|id| self.tabs.iter().any(|t| t.id == id)) { None } else { Some("There is no tab to take the rows from.".to_string()) };
        self.drill_ctx(&self.chart.spec, rows, None)
    }

    /// The drill context of a chart tab.
    pub(crate) fn chart_tab_drill_ctx(&self, id: u64) -> DrillCtx {
        let spec = self.chart_tabs.get(&id).map(|i| i.next_spec.clone().unwrap_or_else(|| i.data.spec.clone())).unwrap_or_default();
        let blocked = self.chart_tab_blocked(id);
        self.drill_ctx(&spec, blocked.clone(), blocked)
    }

    /// A click or a menu item on the studio's chart.
    pub(crate) fn studio_chart_action(&mut self, action: ChartAction) {
        let mut spec = self.chart.spec.clone();
        match action {
            ChartAction::Colour(m) => self.open_colour_picker_of_mark(crate::ui::colour_picker::PickTarget::Studio, &m),
            ChartAction::DrillDown(m) => {
                if let Err(e) = ChartSpec::refuse_other(&m.category) {
                    self.status_message = format!("{:#}", e);
                } else if spec.drill_down(&m.category) {
                    self.chart_set_spec(spec);
                }
            }
            ChartAction::Keep(ref m) | ChartAction::Exclude(ref m) => {
                let keep = matches!(action, ChartAction::Keep(_));
                match spec.add_value_filter(&m.category, keep) {
                    Ok(true) => self.chart_set_spec(spec),
                    Ok(false) => self.status_message = "That filter is already on the chart.".to_string(),
                    Err(e) => self.status_message = format!("{:#}", e),
                }
            }
            ChartAction::ShowRows(m) => {
                let Some(source) = self.chart.tab_id else {
                    self.status_message = "There is no tab to take the rows from.".to_string();
                    return;
                };
                self.show_rows_of_mark(source, &spec, &m);
            }
        }
    }

    /// A click or a menu item on a chart tab: the chart tab is calculated again from its source with the new level or filter.
    pub(crate) fn chart_tab_chart_action(&mut self, id: u64, action: ChartAction) {
        // a colour is not a calculation: it works on a chart that cannot be calculated again too
        if let ChartAction::Colour(m) = &action {
            self.open_colour_picker_of_mark(crate::ui::colour_picker::PickTarget::ChartTab(id), m);
            return;
        }
        if let Some(why) = self.chart_tab_blocked(id) {
            self.status_message = why;
            return;
        }
        let Some(info) = self.chart_tabs.get(&id) else { return };
        let mut spec = info.next_spec.clone().unwrap_or_else(|| info.data.spec.clone());
        let source = info.source_tab;
        match action {
            ChartAction::DrillDown(m) => {
                if let Err(e) = ChartSpec::refuse_other(&m.category) {
                    self.status_message = format!("{:#}", e);
                } else if spec.drill_down(&m.category) {
                    self.chart_tab_set_spec(id, spec);
                }
            }
            ChartAction::Keep(ref m) | ChartAction::Exclude(ref m) => {
                let keep = matches!(action, ChartAction::Keep(_));
                match spec.add_value_filter(&m.category, keep) {
                    Ok(true) => self.chart_tab_set_spec(id, spec),
                    Ok(false) => self.status_message = "That filter is already on the chart.".to_string(),
                    Err(e) => self.status_message = format!("{:#}", e),
                }
            }
            ChartAction::ShowRows(m) => {
                if let Some(source) = source {
                    self.show_rows_of_mark(source, &spec, &m);
                }
            }
            ChartAction::Colour(_) => {}
        }
    }

    /// The chart tab draws `spec` from now on: the worker calculates it from the source, and the numbers shown stay as they were
    /// (with the spec they were calculated with) until the new ones arrive.
    pub(crate) fn chart_tab_set_spec(&mut self, id: u64, spec: ChartSpec) {
        if let Some(info) = self.chart_tabs.get_mut(&id) {
            info.next_spec = Some(spec);
            // a different signature than the source's: the next look at the source calculates it
            info.source_sig.clear();
            info.pending_sig = None;
            info.error = None;
        }
    }

    /// "Show rows": a new tab with the source tab's file and steps and a filter for the mark. Its rows are the mark's rows,
    /// read by the same filter the chart read them with (`engine::filter_by_conditions`).
    pub(crate) fn show_rows_of_mark(&mut self, source_tab: u64, spec: &ChartSpec, mark: &MarkRef) {
        let Some(src) = self.tabs.iter().find(|t| t.id == source_tab) else {
            self.status_message = "The tab the chart was made from is not open, so there are no rows to show.".to_string();
            return;
        };
        let conditions = match spec.mark_conditions(&mark.category, mark.series.as_deref(), mark.facet.as_deref()) {
            Ok(c) => c,
            Err(e) => {
                self.status_message = format!("{:#}", e);
                return;
            }
        };
        let exec = src.exec_steps.min(src.steps.len());
        let mut steps: Vec<TransformStep> = src.steps[..exec].to_vec();
        steps.push(TransformStep::FilterGroup { conditions, join: GroupJoin::And, keep_matching: true });
        let title = spec.mark_title(&mark.category, mark.series.as_deref(), mark.facet.as_deref());
        let (source, formats) = (src.source.clone(), src.col_formats.clone());
        self.create_tab_from_source(title.clone(), source, steps, formats);
        self.status_message = format!("Opening '{}'.", title);
    }

    /// Where the Back key (Backspace, or Alt+Left) goes up a level: the active chart tab, or the studio's chart when the
    /// pointer is over it. Never while a text box, a dialog or the search has the keyboard.
    pub(crate) fn handle_drill_back_key(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() || self.tabs.is_empty() || self.command_open || self.active_tool != ActiveToolModal::None {
            return;
        }
        let tab_id = self.tabs.get(self.active_tab).map(|t| t.id);
        let on_chart_tab = tab_id.filter(|id| self.chart_tabs.get(id).is_some_and(|i| i.closed.is_none() && i.source_tab.is_some() && i.next_spec.as_ref().unwrap_or(&i.data.spec).depth() > 0));
        let over_studio = crate::ui::chart_render::last_shown(ctx, egui::Id::new("chart_plot")).zip(ctx.input(|i| i.pointer.hover_pos())).is_some_and(|(s, p)| s.rect.contains(p)) && self.chart.spec.depth() > 0 && self.tab_is_not_chart_tab();
        if on_chart_tab.is_none() && !over_studio {
            return;
        }
        let back = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace) || i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft));
        if !back {
            return;
        }
        if let Some(id) = on_chart_tab {
            self.chart_tab_drill_to(id, None);
        } else {
            let mut spec = self.chart.spec.clone();
            spec.drill_to(spec.depth().saturating_sub(1));
            self.chart_set_spec(spec);
        }
    }

    fn tab_is_not_chart_tab(&self) -> bool {
        !self.active_is_chart_tab()
    }

    /// The chart tab goes back to `depth` levels down (one level up when `None`).
    pub(crate) fn chart_tab_drill_to(&mut self, id: u64, depth: Option<usize>) {
        let Some(info) = self.chart_tabs.get(&id) else { return };
        let mut spec = info.next_spec.clone().unwrap_or_else(|| info.data.spec.clone());
        let to = depth.unwrap_or_else(|| spec.depth().saturating_sub(1));
        if to >= spec.depth() {
            return;
        }
        spec.drill_to(to);
        self.chart_tab_set_spec(id, spec);
    }
}

/// What the grid knows of a pivot tab's cells (stored for the grid for one frame): the row fields, the columns that later steps
/// made, and why "Show rows" is off when it is.
#[derive(Clone, Debug, Default)]
pub(crate) struct PivotRowsUi {
    /// The names of the pivot's row-field columns (a click on those shows no rows).
    pub labels: Vec<String>,
    /// Columns that steps after the pivot added, renamed or rewrote.
    pub later_made: Vec<String>,
    /// Why Show rows is off for the whole tab.
    pub off: Option<String>,
    /// The pivot's settings, to tell a named measure's column (its cell has no rows to show).
    pub spec: Option<micro_parquet::models::PivotSpec>,
}

impl PivotRowsUi {
    /// Why a click on the column `name` cannot show rows (`None` when it can).
    pub fn reason_for(&self, name: &str) -> Option<String> {
        if let Some(r) = &self.off {
            return Some(r.clone());
        }
        if self.later_made.iter().any(|c| c == name) {
            return Some(format!("Show rows is off for '{}': a step after the pivot added, renamed or changed it. Use one of the pivot's own columns.", name));
        }
        if self.spec.as_ref().is_some_and(|s| micro_parquet::pivot_table::is_measure_column(s, name)) {
            return Some(micro_parquet::pivot_table::MEASURE_NO_ROWS.to_string());
        }
        None
    }
}

impl MicroParquetApp {
    /// For a pivot tab: its row-field columns and whether Show rows works, with steps after the pivot read as
    /// `pivot_table::pivot_rows_of` says. `None` when the tab is not a pivot tab.
    pub(crate) fn pivot_rows_ui(&self, tab: &TabState) -> Option<PivotRowsUi> {
        use micro_parquet::pivot_table::{pivot_rows_of, PivotRows, NO_ROWS_COLUMN};
        match pivot_rows_of(&tab.steps, tab.exec_steps) {
            PivotRows::NotAPivot => None,
            PivotRows::Unavailable(reason) => {
                let labels = tab.steps.iter().rev().find_map(|s| if s.is_skipped() { None } else { s.as_pivot_spec() }).map(|p| if p.rows.is_empty() { vec![NO_ROWS_COLUMN.to_string()] } else { p.rows }).unwrap_or_default();
                Some(PivotRowsUi { labels, later_made: Vec::new(), off: Some(reason), spec: None })
            }
            PivotRows::Readable { spec, later_made, .. } => {
                let labels = if spec.rows.is_empty() { vec![NO_ROWS_COLUMN.to_string()] } else { spec.rows.clone() };
                Some(PivotRowsUi { labels, later_made, off: None, spec: Some(spec) })
            }
        }
    }

    /// Double-click or "Show rows" on a value cell of the active pivot tab: a worker builds the pivot again, finds the values
    /// the cell stands for, and the rows open in a new tab.
    pub(crate) fn pivot_show_rows(&mut self, row: usize, column: &str) {
        use micro_parquet::pivot_table::{pivot_rows_of, PivotRows};
        let Some(tab) = self.tabs.get(self.active_tab) else { return };
        let (pivot_at, spec, later_made) = match pivot_rows_of(&tab.steps, tab.exec_steps) {
            PivotRows::Readable { at, spec, later_made } => (at, spec, later_made),
            PivotRows::Unavailable(reason) => {
                self.status_message = reason;
                return;
            }
            PivotRows::NotAPivot => {
                self.status_message = "Show rows works on a pivot tab.".to_string();
                return;
            }
        };
        if later_made.iter().any(|c| c == column) {
            self.status_message = format!("Show rows is off for '{}': a step after the pivot added, renamed or changed it. Use one of the pivot's own columns.", column);
            return;
        }
        if micro_parquet::pivot_table::is_measure_column(&spec, column) {
            self.status_message = micro_parquet::pivot_table::MEASURE_NO_ROWS.to_string();
            return;
        }
        // P2: on a Nested pivot the clicked line is a line of its view: a parent line gives every source row of its group
        let nested_key = match self.nested_for_display(tab) {
            Some((view, _)) => match view.rows.get(row) {
                Some(line) if !line.is_blank() => Some(line.keys.clone()),
                _ => return,
            },
            None => None,
        };
        let req = micro_parquet::tab_jobs::ShowRowsRequest {
            base: tab.source.clone(),
            steps: tab.steps.clone(),
            pivot_at,
            spec,
            later_made,
            row,
            column: column.to_string(),
            expected_rows: tab.total_rows,
            temp_dir: self.paths.temp_dir.clone(),
        };
        let tab_id = tab.id;
        match nested_key {
            Some(row_key) => {
                let req = micro_parquet::tab_jobs::NestedShowRowsRequest { request: req, row_key };
                self.jobs.submit(JobKey::tab(JobKind::ShowRows, tab_id), "Finding the rows of that cell", false, move |ctx| micro_parquet::tab_jobs::show_rows_nested_job(req, ctx));
            }
            None => {
                self.jobs.submit(JobKey::tab(JobKind::ShowRows, tab_id), "Finding the rows of that cell", false, move |ctx| micro_parquet::tab_jobs::show_rows_job(req, ctx));
            }
        }
    }

    /// The worker found the rows of a pivot cell: they open in a new tab.
    pub(crate) fn apply_show_rows(&mut self, r: micro_parquet::tab_jobs::ShowRowsResult) {
        let formats = self.tabs.get(self.active_tab).map(|t| t.col_formats.clone()).unwrap_or_default();
        let id = self.create_tab_from_source_with(r.title.clone(), r.base, r.steps, formats, None);
        if let Some(n) = r.note {
            self.tab_notes.insert(id, n);
        }
        self.status_message = format!("Opening '{}'.", r.title);
    }
}
