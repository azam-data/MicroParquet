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
        let key_note = match ensure_pii_key_for(&source) {
            Ok(n) => n,
            Err(e) => {
                self.status_message = format!("Pipeline error: {:#}", e);
                return;
            }
        };
        match compile_tab_lazy(&source, &steps, &self.paths.temp_dir) {
            Ok(lf) => {
                let slice = fetch_viewport_slice(lf.clone(), 0, 100).unwrap_or_default();
                let default_ctrl = Self::guess_balance_column(&slice);
                let totals = control_totals_state(Ok(lf), default_ctrl.as_deref());
                let (total_rows, control_sum) = (totals.rows.unwrap_or(0), totals.sum);

                // Baseline control sum is always computed from the raw source (0 steps)
                let baseline_sum = if steps.is_empty() {
                    control_sum
                } else {
                    compile_tab_lazy(&source, &[], &self.paths.temp_dir)
                        .ok()
                        .and_then(|base_lf| {
                            compute_control_totals(base_lf, default_ctrl.as_deref()).ok()
                        })
                        .and_then(|(_, s)| s)
                };

                let (parse_failures, mut count_note) =
                    match source_parse_failures(&source, &self.paths.temp_dir) {
                        Ok(f) => (f, String::new()),
                        Err(e) => (
                            Vec::new(),
                            format!(" ⚠ Could not count parse failures: {:#}", e),
                        ),
                    };
                let row_check = match source_row_check(&source, &self.paths.temp_dir) {
                    Ok(c) => c,
                    Err(e) => {
                        count_note.push_str(&format!(" ⚠ Could not compare rows with the file's lines: {:#}", e));
                        None
                    }
                };
                if let Some(w) = row_check.as_ref().and_then(|c| c.warning()) {
                    count_note.push_str(&format!(" ⚠ {}", w));
                }
                let join_stat = match join_status(
                    &source,
                    &steps,
                    default_ctrl.as_deref(),
                    &self.paths.temp_dir,
                ) {
                    Ok(j) => j,
                    Err(e) => {
                        count_note.push_str(&format!(" ⚠ Could not compare the join with its left table: {:#}", e));
                        None
                    }
                };
                self.tabs.push(TabState {
                    title,
                    source,
                    steps,
                    col_formats,
                    total_rows,
                    control_col: default_ctrl,
                    control_sum,
                    baseline_control_sum: baseline_sum,
                    viewport_offset: 0,
                    viewport_limit: 100,
                    cached_slice: slice,
                    sql_scratchpad: "SELECT * FROM self LIMIT 100".to_string(),
                    parse_failures,
                    join_status: join_stat,
                    totals,
                    row_check,
                });
                self.active_tab = self.tabs.len() - 1;
                self.init_tool_defaults_from_active_tab();
                self.status_message =
                    format!("Tab created and evaluated out-of-core.{}", count_note);
                if let Some(n) = key_note {
                    self.status_message.push_str(&format!(" 🔑 {}", n));
                }
                self.append_blank_balance_notes();
            }
            Err(e) => {
                self.status_message = format!("Pipeline error: {}", e);
            }
        }
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
            // A failed refresh replaces this message with the error.
            let added = format!("Step added: {}.", step_label);
            self.status_message = added.clone();
            self.refresh_active_tab(false);
            if self.status_message == added {
                self.append_blank_balance_notes();
            }
        }
    }

    pub(crate) fn guess_balance_column(df: &DataFrame) -> Option<String> {
        // Prefer exact "balance" column before partial matches
        for col in df.get_columns() {
            if col.name().eq_ignore_ascii_case("balance") {
                return Some(col.name().to_string());
            }
        }
        for col in df.get_columns() {
            let lower = col.name().to_lowercase();
            if lower.contains("balance") || lower.contains("amount") || lower.contains("delta") {
                return Some(col.name().to_string());
            }
        }
        None
    }

    /// Adds "Balance blank on N rows; treated as 0" for the active tab's banking steps.
    pub(crate) fn append_blank_balance_notes(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let notes = blank_balance_notes(&tab.source, &tab.steps, &self.paths.temp_dir).and_then(|mut n| {
            n.extend(weighted_mixed_sign_notes(&tab.source, &tab.steps, &self.paths.temp_dir)?);
            Ok(n)
        });
        match notes {
            Ok(notes) if !notes.is_empty() => {
                self.status_message = format!("{} ⚠ {}", self.status_message, notes.join("; "));
            }
            Ok(_) => {}
            Err(e) => {
                self.status_message = format!(
                    "{} ⚠ Could not count blank balances: {:#}",
                    self.status_message, e
                );
            }
        }
    }

    pub(crate) fn refresh_active_tab(&mut self, recalculate_baseline: bool) {
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        match compile_tab_lazy(&tab.source, &tab.steps, &self.paths.temp_dir) {
            Ok(lf) => {
                if let Ok(slice) =
                    fetch_viewport_slice(lf.clone(), tab.viewport_offset, tab.viewport_limit)
                {
                    tab.cached_slice = slice;
                }
                // A failure shows "—" and the reason instead of keeping the previous totals.
                tab.totals = control_totals_state(Ok(lf), tab.control_col.as_deref());
                tab.total_rows = tab.totals.rows.unwrap_or(0);
                tab.control_sum = tab.totals.sum;
                if recalculate_baseline {
                    // Recompute baseline from the base source (0 steps) for the newly selected control column
                    let base_sum = compile_tab_lazy(&tab.source, &[], &self.paths.temp_dir)
                        .ok()
                        .and_then(|base_lf| {
                            compute_control_totals(base_lf, tab.control_col.as_deref()).ok()
                        })
                        .and_then(|(_, s)| s);
                    tab.baseline_control_sum = base_sum;
                }
                if tab.join_status.is_some() {
                    tab.join_status = join_status(
                        &tab.source,
                        &tab.steps,
                        tab.control_col.as_deref(),
                        &self.paths.temp_dir,
                    )
                    .ok()
                    .flatten();
                }
                self.init_tool_defaults_from_active_tab();
            }
            Err(e) => {
                let msg = format!("{:#}", e);
                tab.totals = control_totals_state(Err(e), tab.control_col.as_deref());
                tab.control_sum = None;
                self.status_message = format!("Step execution failed: {}", msg);
            }
        }
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
