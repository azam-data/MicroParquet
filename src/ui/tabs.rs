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
    ) {
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

    pub(crate) fn push_step_to_active_tab(&mut self, step: TransformStep, open_new_tab: bool) {
        let Some(curr) = self.tabs.get(self.active_tab).cloned() else {
            return;
        };

        // Inherit per-metric format specifications if this is a Pivot step
        let mut updated_formats = curr.col_formats.clone();
        if let TransformStep::GroupByPivot { ref aggs, .. } = step {
            for a in aggs {
                updated_formats.insert(a.alias.clone(), a.format_spec);
            }
        }

        let step_label = step.short_label();
        if open_new_tab {
            let mut new_steps = curr.steps.clone();
            let title = format!("{} - {}", curr.title, step.short_label());
            new_steps.push(step);
            self.create_tab_from_source(title, curr.source, new_steps, updated_formats);
        } else {
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                tab.steps.push(step);
                tab.col_formats = updated_formats;
                tab.viewport_offset = 0;
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

    /// A tab id nobody has used (ids are never reused, unlike positions).
    pub(crate) fn next_tab_id(&mut self) -> u64 {
        self.tab_id_counter += 1;
        self.tab_id_counter
    }

    pub(crate) fn render_empty_state(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(90.0);
            ui.heading(
                "MicroParquet v1.2: Portable CSV/Parquet, ALM & Loans Analytics Workstation",
            );
            ui.label("Out-of-Core Polars Engine • Multi-Tab Views • Custom Maturity Buckets • Per-Metric Formats");
            ui.add_space(20.0);
            if ui.button("📂 Open CSV or Parquet File...").clicked() {
                self.initiate_file_open(false);
            }
            ui.add_space(8.0);
            if ui.button("📚 Stack / Merge Multiple Files...").clicked() {
                self.active_tool = ActiveToolModal::StackFiles;
            }
            ui.add_space(8.0);
            if ui
                .button("🔄 Launch Recursive Folder Batch Converter...")
                .clicked()
            {
                self.batch_modal.open = true;
            }
        });
    }

    /// Draws the tab strip. Returns true when a tab was closed this frame (the frame stops there).
    pub(crate) fn render_tab_strip(&mut self, ui: &mut egui::Ui) -> bool {
        let mut tab_to_close: Option<usize> = None;
        let mut duplicate_tab: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            for idx in 0..self.tabs.len() {
                let is_selected = idx == self.active_tab;
                let title = self.tabs.get(idx).map(|t| t.title.clone()).unwrap_or_default();
                let tab_label = format!("{} ({})", title, idx + 1);
                if ui.selectable_label(is_selected, tab_label).clicked() {
                    self.active_tab = idx;
                    self.init_tool_defaults_from_active_tab();
                }
                if ui.small_button("➕").on_hover_text("Clone View Tab").clicked() {
                    duplicate_tab = Some(idx);
                }
                if ui.small_button("✖").on_hover_text("Close Tab").clicked() {
                    tab_to_close = Some(idx);
                }
                ui.separator();
            }
        });

        if let Some(idx) = duplicate_tab {
            if let Some(orig) = self.tabs.get(idx).cloned() {
                let mut cloned = orig;
                cloned.title = format!("View of {}", cloned.title);
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
            }
            return true;
        }
        false
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
