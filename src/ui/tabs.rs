//! Screen module `tabs` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// Where a stored tab index points after the tab at `closed` is removed, leaving `new_len` tabs.
/// Indices after the closed tab shift down so they keep pointing at the same tab. An index at
/// the closed tab moves to the tab that took its place (or the new last tab). Any result is
/// clamped to 0..new_len, and is 0 when no tabs are left.
/// `title`, or "title (2)", "title (3)" … when a tab of that name is open: the title names the tab.
pub(crate) fn unique_tab_title(tabs: &[TabState], title: &str) -> String {
    if !tabs.iter().any(|t| t.title == title) {
        return title.to_string();
    }
    (2..).map(|n| format!("{} ({})", title, n)).find(|t| !tabs.iter().any(|x| &x.title == t)).unwrap_or_else(|| title.to_string())
}

pub(crate) fn tab_index_after_close(sel: usize, closed: usize, new_len: usize) -> usize {
    let shifted = if sel > closed { sel - 1 } else { sel };
    shifted.min(new_len.saturating_sub(1))
}

/// The window title: "MicroParquet v1.6.0", and " — <file name>" when a tab is open.
pub fn window_title(file: Option<&str>) -> String {
    match file.map(str::trim).filter(|f| !f.is_empty()) {
        Some(f) => format!("MicroParquet v{} \u{2014} {}", env!("CARGO_PKG_VERSION"), f),
        None => format!("MicroParquet v{}", env!("CARGO_PKG_VERSION")),
    }
}

impl MicroParquetApp {
    /// The name of the active tab's file (a tab made by a tool has its own title).
    pub(crate) fn active_file_name(&self) -> Option<String> {
        let tab = self.tabs.get(self.active_tab)?;
        let first = micro_parquet::steps_edit::source_files(&tab.source).first().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string());
        Some(first.unwrap_or_else(|| tab.title.clone()))
    }

    /// Keeps the window's title bar in step with the active tab.
    pub(crate) fn update_window_title(&mut self, ctx: &egui::Context) {
        let title = self.current_window_title();
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
    }

    pub(crate) fn initiate_file_open(&mut self, direct_convert_only: bool) {
        #[cfg(test)]
        let picked = {
            self.file_dialog_requests += 1;
            self.test_pick.take()
        };
        #[cfg(not(test))]
        let picked = rfd::FileDialog::new().add_filter("Data Files (CSV, Parquet)", &["csv", "parquet"]).pick_file();
        let Some(path) = picked else {
            return;
        };
        self.open_data_file(path, direct_convert_only);
    }

    /// Opens a CSV or Parquet file: with the saved schema that matches its columns (Phase 6 U1),
    /// else as before (a CSV through the Schema dialog, a Parquet file straight into a tab).
    pub(crate) fn open_data_file(&mut self, path: PathBuf, direct_convert_only: bool) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // the saved schemas whose original column names are this file's header names
        let (templates, damaged) = micro_parquet::schema_templates::load_templates_checked(&self.paths.schemas_dir);
        if !damaged.is_empty() {
            self.status_message = format!("\u{26A0} Saved schema(s) skipped when matching this file: {}. Fix or delete the file in portable_data/schemas.", damaged.join("; "));
        }
        if !templates.is_empty() {
            if let Ok(detected) = detect_file_schema(&path) {
                let headers: Vec<String> = detected.columns.iter().map(|c| c.name.clone()).collect();
                let matched: Vec<micro_parquet::schema_templates::SchemaTemplate> =
                    micro_parquet::schema_templates::matching_templates(&templates, &headers).into_iter().cloned().collect();
                match matched.len() {
                    0 => {}
                    1 => {
                        self.open_with_template(path, direct_convert_only, detected, &matched[0], false);
                        return;
                    }
                    _ => {
                        self.status_message = format!("{} saved schemas match this file. Choose one.", matched.len());
                        self.template_pick = Some(crate::ui::schema::TemplatePick { path, direct_convert_only, detected, candidates: matched, rects: Vec::new() });
                        return;
                    }
                }
            }
        }

        if ext == "parquet" {
            self.create_tab_from_source(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                TabSource::SingleFile {
                    path,
                    is_parquet: true,
                    config: IngestionConfig::default(),
                },
                Vec::new(),
                HashMap::new(),
            );
        } else {
            match (preview_csv_raw(&path, 5), auto_detect_smart_schema(&path)) {
                (Ok(preview_df), Ok(config)) => {
                    self.pending_csv_modal = Some(PendingCsvModal {
                        csv_path: path,
                        raw_preview_df: preview_df,
                        config,
                        direct_convert_only,
                        target_tab: None,
                        parse_failures: Vec::new(),
                        is_parquet: false,
                        popped_out: false,
                        applied: None,
                    });
                }
                (Err(e), _) | (_, Err(e)) => {
                    self.status_message = format!("Error reading CSV preview: {}", e);
                }
            }
        }
    }

    /// Opens a file with a saved schema whose columns match it. A CSV shows the Schema dialog
    /// with the schema applied and an Undo button, unless the template is ticked "Apply without
    /// asking" (and the choice was not made from the list of several): then the tab opens at
    /// once. A Parquet file opens at once (its dialog is the tab's Schema button).
    pub(crate) fn open_with_template(
        &mut self,
        path: PathBuf,
        direct_convert_only: bool,
        detected: IngestionConfig,
        template: &micro_parquet::schema_templates::SchemaTemplate,
        chosen_from_list: bool,
    ) {
        let applied_config = micro_parquet::schema_templates::apply_template(&detected, &template.config);
        let n = applied_config.columns.len();
        let title = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let is_parquet = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("parquet"));
        let note = format!("Applied saved schema '{}' (all {} columns matched)", template.name, n);
        let fine = applied_config.rename_problems().is_empty();
        if is_parquet && direct_convert_only {
            // converting a Parquet file is not offered here: open it as it is
            self.create_tab_from_source(title, TabSource::SingleFile { path, is_parquet: true, config: IngestionConfig::default() }, Vec::new(), HashMap::new());
            return;
        }
        // a template ticked "Apply without asking" opens the file at once, CSV and Parquet alike
        if template.config.auto_apply && !direct_convert_only && !chosen_from_list && fine {
            let mut config = applied_config;
            let mut steps = Vec::new();
            hoist_clean_line_breaks(&mut config, is_parquet, &mut steps);
            let id = self.create_tab_from_source_with(title, TabSource::SingleFile { path, is_parquet, config }, steps, HashMap::new(), None);
            self.open_notes.insert(id, format!("{}, opened without asking.", note));
            return;
        }
        // otherwise the Schema dialog opens with the template applied (and an Undo button)
        let preview = if is_parquet { preview_parquet_raw(&path, 5) } else { preview_csv_raw(&path, 5) };
        match preview {
            Ok(preview_df) => {
                self.pending_csv_modal = Some(PendingCsvModal {
                    csv_path: path,
                    raw_preview_df: preview_df,
                    config: applied_config,
                    direct_convert_only,
                    target_tab: None,
                    parse_failures: Vec::new(),
                    is_parquet,
                    popped_out: false,
                    applied: Some(crate::ui::schema::AppliedTemplate { name: template.name.clone(), columns: n, before: detected }),
                });
            }
            Err(e) => self.status_message = format!("Error reading CSV preview: {}", e),
        }
    }

    pub(crate) fn create_tab_from_source(
        &mut self,
        title: String,
        source: TabSource,
        steps: Vec<TransformStep>,
        col_formats: HashMap<String, ColumnFormatSpec>,
    ) {
        self.create_tab_from_source_with(title, source, steps, col_formats, None);
    }

    /// Builds the tab on the worker thread; the tab appears (and becomes active) when it is ready.
    /// `reconcile_rate_used`: a period reconcile, whose status gets the reconcile summary.
    pub(crate) fn create_tab_from_source_with(
        &mut self,
        title: String,
        source: TabSource,
        steps: Vec<TransformStep>,
        col_formats: HashMap<String, ColumnFormatSpec>,
        reconcile_rate_used: Option<bool>,
    ) -> u64 {
        let tab_id = self.next_tab_id();
        let label = format!("Opening '{}'", title);
        let req = LoadRequest {
            tab_id,
            title,
            source,
            steps,
            col_formats,
            temp_dir: self.paths.temp_dir.clone(),
            reconcile_rate_used,
        };
        self.jobs.submit(JobKey::tab(JobKind::Load, tab_id), label, true, move |ctx| {
            load_tab(req, ctx).map(|t| JobOutput::Loaded(Box::new(t)))
        });
        self.status_message = "Loading…".to_string();
        tab_id
    }

    pub(crate) fn init_tool_defaults_from_active_tab(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let cols = tab.available_columns();
        if let Some(first) = cols.first() {
            if self.filter.col.is_empty() || !cols.contains(&self.filter.col) {
                self.filter.col = first.clone();
            }
            self.columns.rename_old = first.clone();
        }
        self.columns.list = cols
            .iter()
            .map(|c| {
                let spec = tab
                    .col_formats
                    .get(c)
                    .copied()
                    .unwrap_or(ColumnFormatSpec {
                        fmt_type: MetricFormatType::Auto,
                        decimals: self.global_decimals,
                    });
                (c.clone(), true, spec)
            })
            .collect();
        self.pivot.group_cols = cols.iter().map(|c| (c.clone(), false)).collect();
        self.export.multi_checks = vec![true; self.tabs.len()];
    }

    /// Replaces step `idx` of the active tab in place and refreshes the view.
    pub(crate) fn replace_step_in_active_tab(&mut self, idx: usize, step: TransformStep, status: &str) {
        let pivot_formats = self.pivot_formats_for(&step);
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        if idx >= tab.steps.len() {
            return;
        }
        tab.record_undo();
        // as when a step is added: a pivot's measure formats and the Columns dialog's formats
        if let TransformStep::GroupByPivot { aggs, .. } = &step {
            for a in aggs {
                tab.col_formats.insert(a.alias.clone(), a.format_spec);
            }
        }
        for (c, spec) in self.columns.apply_formats.take().unwrap_or_default() {
            tab.col_formats.insert(c, spec);
        }
        for (c, spec) in pivot_formats {
            tab.col_formats.insert(c, spec);
        }
        tab.steps[idx] = step;
        tab.viewport_offset = 0;
        self.status_message = status.to_string();
        self.refresh_tab_at(self.active_tab, false, Some(status.to_string()));
    }

    pub(crate) fn push_step_to_active_tab(&mut self, step: TransformStep, open_new_tab: bool) {
        let Some(curr) = self.tabs.get(self.active_tab).cloned() else {
            return;
        };
        // the model's Calendar is made by the model: it takes no steps
        if !open_new_tab && self.is_calendar_source(self.active_tab) {
            self.status_message = micro_parquet::model_calendar::CALENDAR_NOTE.to_string();
            return;
        }
        // a chart tab has no steps of its own (they are its source tab's): nothing is added to it in place
        if !open_new_tab && self.active_is_chart_tab() {
            self.status_message = self.chart_tab_steps_note().unwrap_or_else(|| "This tab draws a chart. Change the data on its source tab.".to_string());
            return;
        }

        // Inherit per-metric format specifications if this is a Pivot step; formats chosen in the
        // Columns dialog arrive with its step (so one undo takes back both).
        let mut updated_formats = curr.col_formats.clone();
        for (c, spec) in self.columns.apply_formats.take().unwrap_or_default() {
            updated_formats.insert(c, spec);
        }
        if let TransformStep::GroupByPivot { ref aggs, .. } = step {
            for a in aggs {
                updated_formats.insert(a.alias.clone(), a.format_spec);
            }
        }
        for (c, spec) in self.pivot_formats_for(&step) {
            updated_formats.insert(c, spec);
        }

        let step_label = step.short_label();
        if open_new_tab {
            let mut new_steps = curr.steps.clone();
            // a pivot tab is named after its first row field ("Pivot: Segment"); other steps after the tab and the step
            let title = match step.as_pivot_spec() {
                Some(spec) => pivot_tab_title(&spec),
                None => format!("{} - {}", curr.title, step.short_label()),
            };
            new_steps.push(step);
            self.create_tab_from_source(title, curr.source, new_steps, updated_formats);
        } else {
            let after = self.insert_after.take();
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                tab.record_undo();
                let at = micro_parquet::steps_edit::insert_after(&mut tab.steps, after, step);
                tab.col_formats = updated_formats;
                tab.viewport_offset = 0;
                self.selected_step = Some(at);
            }
            // The job's result replaces this message: with the notes, or with the error.
            self.status_message = format!("Step added: {}.", step_label);
            self.refresh_tab_at(self.active_tab, false, Some(format!("Step added: {}.", step_label)));
        }
    }

    pub(crate) fn refresh_active_tab(&mut self, recalculate_baseline: bool) {
        self.refresh_tab_at(self.active_tab, recalculate_baseline, None);
    }

    /// Rebuilds a tab's page, totals and notes on the worker thread. A newer refresh of the same
    /// tab replaces an older one that has not finished.
    pub(crate) fn refresh_tab_at(
        &mut self,
        idx: usize,
        recalculate_baseline: bool,
        added_status: Option<String>,
    ) {
        let Some(tab) = self.tabs.get(idx) else {
            return;
        };
        // A refresh that this one replaces may have had a baseline recount or a "Step added" note
        // to deliver; the newer job does that work too.
        let pend = self.pending_refresh.entry(tab.id).or_default();
        pend.recalculate_baseline |= recalculate_baseline;
        if added_status.is_some() {
            pend.added_status = added_status;
        }
        let req = RefreshRequest {
            validate: true,
            exec_steps: tab.exec_steps,
            tab_id: tab.id,
            source: tab.source.clone(),
            steps: tab.steps.clone(),
            offset: tab.viewport_offset,
            limit: tab.viewport_limit,
            control_col: tab.control_col.clone(),
            recalculate_baseline: pend.recalculate_baseline,
            check_join: tab.join_status.is_some(),
            added_status: pend.added_status.clone(),
            temp_dir: self.paths.temp_dir.clone(),
        };
        let key = JobKey::tab(JobKind::Refresh, tab.id);
        self.jobs.submit(key, "Refreshing view", false, move |ctx| {
            refresh_tab(req, ctx).map(|r| JobOutput::Refreshed(Box::new(r)))
        });
    }

    /// Recomputes the summary row of tab `idx` on the worker thread.
    pub(crate) fn submit_summary(&mut self, idx: usize) {
        let Some(tab) = self.tabs.get(idx) else {
            return;
        };
        let exec = tab.exec_steps.min(tab.steps.len());
        let req = SummaryRequest {
            tab_id: tab.id,
            source: tab.source.clone(),
            steps: tab.steps[..exec].to_vec(),
            overrides: tab.summary.clone(),
            control_col: tab.control_col.clone(),
            temp_dir: self.paths.temp_dir.clone(),
        };
        let key = JobKey::tab(JobKind::Summary, tab.id);
        self.jobs.submit(key, "Summary row", false, move |ctx| {
            compute_tab_summary(req, ctx).map(|r| JobOutput::Summary(Box::new(r)))
        });
    }

    /// Counts the rows after each step of tab `idx` on the worker thread. Counts of steps whose
    /// earlier steps did not change are kept.
    pub(crate) fn submit_step_counts(&mut self, idx: usize) {
        let Some(tab) = self.tabs.get_mut(idx) else {
            return;
        };
        let exec = tab.exec_steps.min(tab.steps.len());
        // Keep a count while the source (path, size, modified time, ingestion settings) and the
        // steps up to it are the same as when it was counted.
        let hashes = micro_parquet::steps_edit::count_keys(&tab.source, &tab.steps[..exec]);
        let known = micro_parquet::steps_edit::reusable_counts(&tab.step_hashes, &tab.step_counts, &hashes);
        tab.step_hashes = hashes.clone();
        tab.step_counts = known.clone();
        let req = StepCountsRequest {
            tab_id: tab.id,
            source: tab.source.clone(),
            steps: tab.steps[..exec].to_vec(),
            known,
            hashes,
            temp_dir: self.paths.temp_dir.clone(),
        };
        let key = JobKey::tab(JobKind::StepCounts, tab.id);
        self.jobs.submit(key, "Row counts per step", false, move |ctx| {
            compute_step_counts(req, ctx).map(|r| JobOutput::StepCounts(Box::new(r)))
        });
    }

    /// Computes the inspector statistics of the selected column of tab `idx` on the worker thread.
    pub(crate) fn submit_column_stats(&mut self, idx: usize) {
        let Some(column) = self.selected_column.clone() else {
            return;
        };
        let Some(tab) = self.tabs.get(idx) else {
            return;
        };
        if !tab.available_columns().contains(&column) {
            return;
        }
        let exec = tab.exec_steps.min(tab.steps.len());
        let req = ColumnStatsRequest {
            tab_id: tab.id,
            source: tab.source.clone(),
            steps: tab.steps[..exec].to_vec(),
            column,
            temp_dir: self.paths.temp_dir.clone(),
        };
        let key = JobKey::tab(JobKind::Stats, tab.id);
        self.jobs.submit(key, "Column statistics", false, move |ctx| {
            compute_tab_column_stats(req, ctx).map(|r| JobOutput::ColumnStats(Box::new(r)))
        });
    }

    /// Selects a column for the inspector and shows its tab.
    pub(crate) fn select_column(&mut self, column: &str) {
        self.selected_column = Some(column.to_string());
        self.right_tab = RightTab::Column;
        self.show_steps_sidebar = true;
        self.submit_column_stats(self.active_tab);
    }

    /// The user picked a summary (or a weight column) for a column of the active tab.
    pub(crate) fn set_summary(&mut self, column: &str, spec: SummarySpec) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.record_undo();
            tab.summary.insert(column.to_string(), spec);
            // The old values no longer match the choice; the job replaces them.
            tab.summary_cells.retain(|c| c.column != column);
        }
        self.submit_summary(self.active_tab);
    }

    /// Picks the active tab's control column (undoable). True when it changed; the caller
    /// refreshes with a new baseline.
    pub(crate) fn set_control_column(&mut self, column: Option<String>) -> bool {
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return false;
        };
        if tab.control_col == column {
            return false;
        }
        tab.record_undo();
        tab.control_col = column;
        true
    }

    /// A tab id nobody has used (ids are never reused, unlike positions).
    pub(crate) fn next_tab_id(&mut self) -> u64 {
        self.tab_id_counter += 1;
        self.tab_id_counter
    }

    /// The start screen: three buttons, in the middle of the window (nothing else).
    pub(crate) fn render_empty_state(&mut self, ui: &mut egui::Ui) {
        // the height of the three buttons is measured the frame before (the first frame guesses)
        let id = egui::Id::new("empty_state_h");
        let guess = 3.0 * ui.spacing().interact_size.y + 2.0 * theme::SPACE_S; // not a size: three buttons, two gaps
        let content_h = ui.ctx().data(|d| d.get_temp::<f32>(id)).unwrap_or(guess);
        let top = ((ui.available_height() - content_h) / 2.0).max(0.0);
        let mut rects: Vec<(String, egui::Rect)> = Vec::new();
        ui.vertical_centered(|ui| {
            ui.add_space(top);
            let r = theme::primary_button(ui, "\u{E256}  Open CSV or Parquet File...");
            rects.push(("open".to_string(), r.rect));
            if r.clicked() {
                self.initiate_file_open(false);
            }
            ui.add_space(theme::SPACE_S);
            let r = ui.button("\u{E466} Stack / Merge Multiple Files...");
            rects.push(("stack".to_string(), r.rect));
            if r.clicked() {
                self.active_tool = ActiveToolModal::StackFiles;
            }
            ui.add_space(theme::SPACE_S);
            let r = ui.button("\u{E094} Launch Recursive Folder Batch Converter...");
            rects.push(("batch".to_string(), r.rect));
            if r.clicked() {
                self.batch_modal.open = true;
            }
        });
        // (the buttons' own rectangles: the panel's rectangle is always the whole area)
        let used = match (rects.first(), rects.last()) {
            (Some((_, f)), Some((_, l))) => l.bottom() - f.top(),
            _ => content_h,
        };
        ui.ctx().data_mut(|d| {
            d.insert_temp(id, used.max(0.0));
            d.insert_temp(egui::Id::new("empty_state_rects"), rects);
        });
    }

    /// Closes the tab at `idx`. Every stored tab index stays valid (see `tab_index_after_close`):
    /// closing the active tab selects the tab that took its place (the new last tab when it was
    /// the last), and closing the only tab leaves the start screen.
    pub(crate) fn close_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        let closed_id = self.tabs.get(idx).map(|t| t.id);
        self.chart_tabs_source_closing(idx);
        if let Some(id) = closed_id {
            self.chart_tabs.remove(&id);
            self.nested_views.remove(&id);
            self.pivot_reports.remove(&id);
        }
        self.tabs.remove(idx);
        let new_len = self.tabs.len();
        for sel in [&mut self.active_tab, &mut self.join.left_tab, &mut self.join.right_tab, &mut self.recon.prev_tab, &mut self.recon.curr_tab] {
            *sel = tab_index_after_close(*sel, idx, new_len);
        }
        if self.tab_rename.as_ref().is_some_and(|r| Some(r.tab_id) == closed_id) {
            self.tab_rename = None;
        }
        self.selected_column = None;
        self.selected_step = None;
        if new_len > 0 {
            self.init_tool_defaults_from_active_tab();
        }
    }

    /// Ctrl+W closes the active tab, unless a dialog or the search is open or a text box has the keyboard.
    pub(crate) fn handle_tab_shortcuts(&mut self, ctx: &egui::Context) {
        if self.tabs.is_empty() || self.active_tool != ActiveToolModal::None || self.command_open || ctx.wants_keyboard_input() {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)) {
            let at = self.active_tab;
            self.close_tab(at);
        }
    }

    /// Tab strip: the active tab is white with a 2 px accent line on top. A tab is one rectangle
    /// that selects on click, closes on middle click, renames on double click; its cross sits on top
    /// of it (registered after it, so a click on the cross is the cross's, not the tab's).
    pub(crate) fn render_tab_strip_panel(&mut self, ctx: &egui::Context) {
        if self.tabs.is_empty() {
            return;
        }
        let tk = *theme::tokens(self.dark_mode);
        let mut tab_to_close: Option<usize> = None;
        let mut duplicate_tab: Option<usize> = None;
        let mut rerun_tab: Option<usize> = None;
        let mut save_run_tab: Option<usize> = None;
        let mut select: Option<usize> = None;
        let mut new_tab = false;
        let mut start_rename: Option<usize> = None;
        let mut commit_rename: Option<(u64, Option<String>)> = None;
        let mut report = TabStripReport::default();
        let active_before = self.active_tab;
        let last_active: Option<usize> = ctx.data(|d| d.get_temp(egui::Id::new("tab_strip_last_active")));
        // the strip scrolls to the active tab for a few frames after it changes (the first frames do not know the widths yet)
        let scroll_id = egui::Id::new("tab_strip_scroll_frames");
        let mut scroll_frames: u8 = ctx.data(|d| d.get_temp(scroll_id)).unwrap_or(0);
        if last_active != Some(active_before) {
            scroll_frames = theme::TAB_SCROLL_FRAMES;
        }
        let active_changed = scroll_frames > 0;
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("tab_strip_last_active"), active_before);
            d.insert_temp(scroll_id, scroll_frames.saturating_sub(1));
        });
        egui::TopBottomPanel::top("tab_strip")
            .exact_height(theme::TAB_STRIP_H)
            .frame(egui::Frame::none().fill(tk.bg).inner_margin(theme::MARGIN_TAB_STRIP))
            .show(ctx, |ui| {
                theme::bottom_border(ui, tk.border);
                report.strip = ui.available_rect_before_wrap();
                egui::ScrollArea::horizontal()
                    .id_salt("tab_strip_scroll")
                    .animated(false)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::BOTTOM), |ui| {
                            ui.spacing_mut().item_spacing.x = theme::BAR_ITEM_GAP;
                            for idx in 0..self.tabs.len() {
                                let active = idx == self.active_tab;
                                let (tab_id, title) = match self.tabs.get(idx) {
                                    Some(t) => (t.id, t.title.clone()),
                                    None => continue,
                                };
                                let renaming = self.tab_rename.as_ref().is_some_and(|r| r.tab_id == tab_id);
                                let shown = shorten_tab_title(&title);
                                let rich = if active { theme::semibold(shown.clone()).color(tk.ink) } else { egui::RichText::new(shown.clone()).color(tk.muted) };
                                let galley = egui::WidgetText::from(rich).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body);
                                let pad = theme::margin_tab(active);
                                let label_w = if renaming { theme::TAB_RENAME_W } else { galley.size().x };
                                let w = pad.left + label_w + theme::SPACE_S + theme::TAB_CLOSE_SIZE + pad.right * theme::HALF;
                                let h = (galley.size().y + pad.top + pad.bottom).max(theme::TAB_CLOSE_SIZE + theme::SPACE_S);
                                let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());
                                // the tab
                                let painter = ui.painter();
                                painter.rect(rect, theme::ROUNDING_TAB_TOP, if active { tk.surface } else { egui::Color32::TRANSPARENT }, if active { theme::line(tk.border) } else { egui::Stroke::NONE });
                                if active {
                                    painter.hline(rect.x_range(), rect.top() + 1.0, theme::line_thick(tk.accent));
                                }
                                let label_rect = egui::Rect::from_min_size(egui::pos2(rect.left() + pad.left, rect.center().y - galley.size().y * theme::HALF), egui::vec2(label_w, galley.size().y));
                                let close_rect = egui::Rect::from_center_size(egui::pos2(rect.right() - pad.right * theme::HALF - theme::TAB_CLOSE_SIZE * theme::HALF, rect.center().y), egui::vec2(theme::TAB_CLOSE_SIZE, theme::TAB_CLOSE_SIZE));
                                // the cross: registered after the tab, so it gets the click on it
                                let close = ui.interact(close_rect, ui.id().with(("tab_close", tab_id)), egui::Sense::click());
                                if close.hovered() {
                                    ui.painter().rect_filled(close_rect, theme::RADIUS_CONTROL, tk.zebra);
                                }
                                ui.painter().text(close_rect.center(), egui::Align2::CENTER_CENTER, theme::ph::X, egui::FontId::proportional(theme::TEXT_BASE), if close.hovered() { tk.ink } else { tk.ink2 });
                                let close = close.on_hover_text("Close tab (Ctrl+W)");
                                if renaming {
                                    let mut buf = self.tab_rename.as_ref().map(|r| r.text.clone()).unwrap_or_default();
                                    let edit = ui.put(label_rect, egui::TextEdit::singleline(&mut buf).desired_width(label_w).frame(false));
                                    if let Some(r) = self.tab_rename.as_mut() {
                                        r.text = buf.clone();
                                        if !r.focused {
                                            edit.request_focus();
                                            r.focused = true;
                                        }
                                    }
                                    let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                                    if esc {
                                        commit_rename = Some((tab_id, None));
                                    } else if enter || (edit.lost_focus() && !edit.has_focus()) {
                                        commit_rename = Some((tab_id, Some(buf)));
                                    }
                                } else {
                                    ui.painter().galley(label_rect.min, galley, tk.ink);
                                }
                                // events
                                if close.clicked() {
                                    tab_to_close = Some(idx);
                                } else if !renaming {
                                    if resp.double_clicked() {
                                        start_rename = Some(idx);
                                    } else if resp.clicked() {
                                        select = Some(idx);
                                    }
                                    if resp.middle_clicked() {
                                        tab_to_close = Some(idx);
                                    }
                                }
                                if active && active_changed {
                                    resp.scroll_to_me(None);
                                }
                                let resp = if shown != title { resp.on_hover_text(title.clone()) } else { resp };
                                resp.context_menu(|ui| {
                                    if ui.button("Rename").clicked() {
                                        start_rename = Some(idx);
                                        ui.close_menu();
                                    }
                                    if ui.button("Duplicate view").clicked() {
                                        duplicate_tab = Some(idx);
                                        ui.close_menu();
                                    }
                                    if self.tabs.get(idx).is_some_and(crate::ui::runs::tab_can_be_saved) {
                                        ui.separator();
                                        if ui.button("Re-run with changes").on_hover_text("Open the tool's dialog with the inputs of this result").clicked() {
                                            rerun_tab = Some(idx);
                                            ui.close_menu();
                                        }
                                        if ui.button("Save run\u{2026}").on_hover_text("Save the inputs of this result in portable_data/runs").clicked() {
                                            save_run_tab = Some(idx);
                                            ui.close_menu();
                                        }
                                    }
                                    if ui.button("Close tab").clicked() {
                                        tab_to_close = Some(idx);
                                        ui.close_menu();
                                    }
                                });
                                report.cells.push(TabCell { index: idx, tab: rect, label: label_rect, close: close_rect, shown, full: title });
                            }
                            if theme::icon_button(ui, theme::ph::PLUS, "Open a file in a new tab").clicked() {
                                new_tab = true;
                            }
                        });
                    });
            });
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("tab_strip_report"), report));

        if let Some((tab_id, text)) = commit_rename {
            self.tab_rename = None;
            if let Some(t) = text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                    tab.title = t;
                }
            }
        }
        if let Some(idx) = start_rename {
            if let Some(t) = self.tabs.get(idx) {
                self.tab_rename = Some(TabRename { tab_id: t.id, text: t.title.clone(), focused: false });
            }
        }
        if let Some(idx) = select {
            if idx != self.active_tab {
                self.active_tab = idx;
                self.selected_column = None;
                self.init_tool_defaults_from_active_tab();
            }
        }
        if new_tab {
            self.initiate_file_open(false);
        }
        if let Some(idx) = rerun_tab {
            self.rerun_tab_with_changes(idx);
        }
        if let Some(idx) = save_run_tab {
            self.open_save_run(idx);
        }
        if let Some(idx) = duplicate_tab {
            if let Some(orig) = self.tabs.get(idx).cloned() {
                let mut cloned = orig;
                cloned.title = unique_tab_title(&self.tabs, &format!("View of {}", cloned.title));
                cloned.id = self.next_tab_id();
                self.tabs.push(cloned);
                self.active_tab = self.tabs.len() - 1;
            }
        }
        if let Some(idx) = tab_to_close {
            self.close_tab(idx);
        }
    }
}

/// The default name of a pivot tab: "Pivot: <first row field>" (the first column field when there
/// are no rows, "Pivot" when there is neither).
pub fn pivot_tab_title(spec: &PivotSpec) -> String {
    match spec.rows.first().or(spec.columns.first()) {
        Some(f) => format!("Pivot: {}", f),
        None => "Pivot".to_string(),
    }
}

/// A tab's name shown in the strip: at most 24 characters, the last one "…" when it is longer
/// (the full name is on hover and in the rename box).
pub const TAB_TITLE_MAX_CHARS: usize = 24;

pub fn shorten_tab_title(title: &str) -> String {
    let n = title.chars().count();
    if n <= TAB_TITLE_MAX_CHARS {
        title.to_string()
    } else {
        let mut s: String = title.chars().take(TAB_TITLE_MAX_CHARS - 1).collect();
        s = s.trim_end().to_string();
        s.push('\u{2026}');
        s
    }
}

/// A tab being renamed: its text so far.
#[derive(Clone, Debug)]
pub struct TabRename {
    pub tab_id: u64,
    pub text: String,
    /// The text box was given the keyboard.
    pub focused: bool,
}

/// What the strip drew for one tab, for the render tests.
#[derive(Clone, Debug)]
pub struct TabCell {
    pub index: usize,
    pub tab: egui::Rect,
    pub label: egui::Rect,
    pub close: egui::Rect,
    pub shown: String,
    pub full: String,
}

/// What the strip drew in its last frame.
#[derive(Clone, Debug)]
pub struct TabStripReport {
    pub strip: egui::Rect,
    pub cells: Vec<TabCell>,
}

impl Default for TabStripReport {
    fn default() -> Self {
        Self { strip: egui::Rect::NOTHING, cells: Vec::new() }
    }
}

pub fn tab_strip_report(ctx: &egui::Context) -> Option<TabStripReport> {
    ctx.data(|d| d.get_temp::<TabStripReport>(egui::Id::new("tab_strip_report")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_tab_names_are_cut_at_24_characters_with_an_ellipsis() {
        assert_eq!(shorten_tab_title("Loans"), "Loans");
        let exactly = "123456789012345678901234";
        assert_eq!(shorten_tab_title(exactly), exactly, "24 characters fit");
        let cut = shorten_tab_title("CL migration (Sep 2026 to Oct 2026)");
        assert!(cut.chars().count() <= 24, "{cut}");
        assert_eq!(cut, "CL migration (Sep 2026\u{2026}", "the space before the cut is dropped");
        // a trailing space before the cut does not leave a gap before the ellipsis
        assert_eq!(shorten_tab_title("1234567890123456789012 abcdef"), "1234567890123456789012\u{2026}");
        // characters, not bytes
        let multi = shorten_tab_title(&"\u{e9}".repeat(30));
        assert_eq!(multi.chars().count(), 24);
        assert_eq!(pivot_tab_title(&PivotSpec { rows: vec!["Segment".into()], columns: vec!["Bank".into()], ..Default::default() }), "Pivot: Segment");
        assert_eq!(pivot_tab_title(&PivotSpec { columns: vec!["Bank".into()], ..Default::default() }), "Pivot: Bank");
        assert_eq!(pivot_tab_title(&PivotSpec::default()), "Pivot");
    }

    #[test]
    fn tab_indices_stay_valid_after_close() {
        // 4 tabs [0,1,2,3]; close tab 1 -> 3 tabs left
        assert_eq!(tab_index_after_close(0, 1, 3), 0, "before the closed tab: unchanged");
        assert_eq!(tab_index_after_close(2, 1, 3), 1, "after: same tab, shifted down");
        assert_eq!(tab_index_after_close(3, 1, 3), 2, "last tab stays last");
        assert_eq!(tab_index_after_close(1, 1, 3), 1, "closed tab: the tab that took its place");
        // Closing the selected last tab moves to the new last tab
        assert_eq!(tab_index_after_close(3, 3, 3), 2);
        // Stale index beyond the list is clamped
        assert_eq!(tab_index_after_close(9, 0, 3), 2);
        // Closing the only tab
        assert_eq!(tab_index_after_close(0, 0, 0), 0);
        // Exhaustive: every result is a valid index (or 0 when empty)
        for len in 1..6usize {
            for closed in 0..len {
                for sel in 0..len + 2 {
                    let r = tab_index_after_close(sel, closed, len - 1);
                    assert!(r < (len - 1).max(1), "sel {sel} closed {closed} len {len} -> {r}");
                }
            }
        }
    }
}
