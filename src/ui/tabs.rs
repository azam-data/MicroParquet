//! Screen module `tabs` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// Where a stored tab index points after the tab at `closed` is removed, leaving `new_len` tabs.
/// Indices after the closed tab shift down so they keep pointing at the same tab. An index at
/// the closed tab moves to the tab that took its place (or the new last tab). Any result is
/// clamped to 0..new_len, and is 0 when no tabs are left.
pub(crate) fn tab_index_after_close(sel: usize, closed: usize, new_len: usize) -> usize {
    let shifted = if sel > closed { sel - 1 } else { sel };
    shifted.min(new_len.saturating_sub(1))
}

impl MicroParquetApp {
    pub(crate) fn initiate_file_open(&mut self, direct_convert_only: bool) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Data Files (CSV, Parquet)", &["csv", "parquet"])
            .pick_file()
        else {
            return;
        };

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

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
                    });
                }
                (Err(e), _) | (_, Err(e)) => {
                    self.status_message = format!("Error reading CSV preview: {}", e);
                }
            }
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
            self.chart.x_col = first.clone();
        }
        if let Some(second) = cols.get(1) {
            self.chart.y_col = second.clone();
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
            let title = format!("{} - {}", curr.title, step.short_label());
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

    pub(crate) fn render_empty_state(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(theme::EMPTY_STATE_TOP);
            ui.heading(format!(
                "MicroParquet {}: Portable CSV/Parquet, ALM & Loans Analytics Workstation",
                env!("CARGO_PKG_VERSION")
            ));
            ui.label("Out-of-Core Polars Engine • Multi-Tab Views • Custom Maturity Buckets • Per-Metric Formats");
            ui.add_space(theme::EMPTY_STATE_GAP);
            if theme::primary_button(ui, "\u{E256}  Open CSV or Parquet File...").clicked() {
                self.initiate_file_open(false);
            }
            ui.add_space(theme::SPACE_S);
            if ui.button("\u{E466} Stack / Merge Multiple Files...").clicked() {
                self.active_tool = ActiveToolModal::StackFiles;
            }
            ui.add_space(theme::SPACE_S);
            if ui
                .button("\u{E094} Launch Recursive Folder Batch Converter...")
                .clicked()
            {
                self.batch_modal.open = true;
            }
        });
    }

    /// Tab strip (40 px): the active tab is white with a 2 px accent line on top.
    pub(crate) fn render_tab_strip_panel(&mut self, ctx: &egui::Context) {
        if self.tabs.is_empty() {
            return;
        }
        let tk = *theme::tokens(self.dark_mode);
        let mut tab_to_close: Option<usize> = None;
        let mut duplicate_tab: Option<usize> = None;
        let mut select: Option<usize> = None;
        let mut new_tab = false;
        egui::TopBottomPanel::top("tab_strip")
            .exact_height(theme::TAB_STRIP_H)
            .frame(
                egui::Frame::none()
                    .fill(tk.bg)
                    .inner_margin(theme::MARGIN_TAB_STRIP),
            )
            .show(ctx, |ui| {
                theme::bottom_border(ui, tk.border);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::BOTTOM), |ui| {
                    ui.spacing_mut().item_spacing.x = theme::BAR_ITEM_GAP;
                    for idx in 0..self.tabs.len() {
                        let active = idx == self.active_tab;
                        let title = self.tabs.get(idx).map(|t| t.title.clone()).unwrap_or_default();
                        let frame = egui::Frame::none()
                            .fill(if active { tk.surface } else { egui::Color32::TRANSPARENT })
                            .stroke(if active { theme::line(tk.border) } else { egui::Stroke::NONE })
                            .rounding(theme::ROUNDING_TAB_TOP)
                            .inner_margin(theme::margin_tab(active));
                        let out = frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = theme::SPACE_S;
                                let text = if active {
                                    theme::semibold(title.clone()).color(tk.ink)
                                } else {
                                    egui::RichText::new(title.clone()).color(tk.muted)
                                };
                                let label = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
                                if label.clicked() {
                                    select = Some(idx);
                                }
                                if theme::icon_button(ui, theme::ph::X, "Close tab").clicked() {
                                    tab_to_close = Some(idx);
                                }
                            });
                        });
                        if active {
                            let r = out.response.rect;
                            ui.painter().hline(r.x_range(), r.top() + 1.0, theme::line_thick(tk.accent));
                        }
                        let resp = out.response.interact(egui::Sense::click());
                        if resp.clicked() {
                            select = Some(idx);
                        }
                        resp.context_menu(|ui| {
                            if ui.button("Duplicate view").clicked() {
                                duplicate_tab = Some(idx);
                                ui.close_menu();
                            }
                            if ui.button("Close tab").clicked() {
                                tab_to_close = Some(idx);
                                ui.close_menu();
                            }
                        });
                    }
                    if theme::icon_button(ui, theme::ph::PLUS, "Open a file in a new tab").clicked() {
                        new_tab = true;
                    }
                });
            });

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
        if let Some(idx) = duplicate_tab {
            if let Some(orig) = self.tabs.get(idx).cloned() {
                let mut cloned = orig;
                cloned.title = format!("View of {}", cloned.title);
                cloned.id = self.next_tab_id();
                self.tabs.push(cloned);
                self.active_tab = self.tabs.len() - 1;
            }
        }
        if let Some(idx) = tab_to_close {
            if idx < self.tabs.len() {
                self.tabs.remove(idx);
                // Every stored tab index must stay valid (see tab_index_after_close)
                let new_len = self.tabs.len();
                for sel in [
                    &mut self.active_tab,
                    &mut self.join.left_tab,
                    &mut self.join.right_tab,
                    &mut self.recon.prev_tab,
                    &mut self.recon.curr_tab,
                ] {
                    *sel = tab_index_after_close(*sel, idx, new_len);
                }
                self.selected_column = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
