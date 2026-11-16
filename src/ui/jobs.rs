//! Background jobs in the UI (Phase 3 R2): submitting work to the worker thread and applying the
//! results each frame. All Polars work runs in `micro_parquet::tab_jobs`.

use crate::ui::prelude::*;

/// Carried from a refresh that a newer one replaced, so the newer one still does that work.
#[derive(Default, Clone)]
pub struct PendingRefresh {
    pub recalculate_baseline: bool,
    pub added_status: Option<String>,
}

impl MicroParquetApp {
    /// Applies every finished job. Called at the start of each frame.
    pub(crate) fn poll_jobs(&mut self, ctx: &egui::Context) {
        let results = self.jobs.poll();
        if results.is_empty() {
            return;
        }
        for r in results {
            let line = r.status_line();
            let kind = r.key.kind;
            match r.outcome {
                Outcome::Done(out) => self.apply_job_output(out),
                Outcome::Failed(msg) => match kind {
                    JobKind::Stats if self.export.pending.is_some() => {
                        self.drop_pending_export();
                        self.status_message = format!("Export stopped: could not check the Text columns: {}", msg);
                    }
                    JobKind::Load => self.status_message = format!("Pipeline error: {}", msg),
                    JobKind::Schema => self.status_message = format!("Schema not applied: {}", msg),
                    JobKind::Batch => {
                        self.batch_modal.last_report = None;
                        self.batch_modal.last_error = Some(msg);
                        if let Some(l) = line {
                            self.status_message = l;
                        }
                    }
                    _ => {
                        if let Some(l) = line {
                            self.status_message = l;
                        }
                    }
                },
                Outcome::Cancelled => {
                    if kind == JobKind::Stats {
                        self.drop_pending_export();
                    }
                    if let Some(l) = line {
                        self.status_message = l;
                    }
                }
            }
        }
        ctx.request_repaint();
    }

    fn tab_position(&self, tab_id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == tab_id)
    }

    fn apply_job_output(&mut self, out: JobOutput) {
        match out {
            JobOutput::Loaded(l) => {
                let LoadedTab { mut tab, status } = *l;
                // A macro replayed into a new tab brings its summary choices.
                if let Some(sum) = self.pending_summary.remove(&tab.id) {
                    tab.summary = sum;
                }
                self.tabs.push(tab);
                self.active_tab = self.tabs.len() - 1;
                self.init_tool_defaults_from_active_tab();
                self.status_message = status;
                if let Some(msg) = self.tabs.get(self.active_tab).and_then(broken_step_message) {
                    self.status_message = msg;
                }
                self.submit_summary(self.active_tab);
                self.submit_step_counts(self.active_tab);
            }
            JobOutput::Refreshed(r) => {
                let Some(idx) = self.tab_position(r.tab_id) else {
                    return;
                };
                self.pending_refresh.remove(&r.tab_id);
                if let Some(tab) = self.tabs.get_mut(idx) {
                    tab.issues = r.issues.clone();
                    tab.exec_steps = r.exec_steps;
                    if let Some(slice) = r.slice {
                        tab.cached_slice = slice;
                        tab.slice_offset = r.offset;
                    }
                    tab.total_rows = r.totals.rows.unwrap_or(0);
                    tab.control_sum = r.totals.sum;
                    tab.totals = r.totals;
                    if let Some(b) = r.baseline {
                        tab.baseline_control_sum = b;
                    }
                    if let Some(j) = r.join_status {
                        tab.join_status = j;
                    }
                }
                match (r.error, r.status) {
                    (Some(e), _) => self.status_message = format!("Step execution failed: {}", e),
                    (None, Some(s)) => self.status_message = s,
                    (None, None) => {}
                }
                // A broken step is always said, whatever else the status was.
                if let Some(msg) = self.tabs.get(idx).and_then(broken_step_message) {
                    self.status_message = msg;
                }
                if idx == self.active_tab {
                    self.init_tool_defaults_from_active_tab();
                }
                // The summary row is of the whole result, so it follows every refresh.
                self.submit_summary(idx);
                self.submit_step_counts(idx);
                self.submit_column_stats(idx);
            }
            JobOutput::ColumnStats(r) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == r.tab_id) {
                    tab.column_stats = Some(r.stats);
                }
            }
            JobOutput::StepCounts(r) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == r.tab_id) {
                    // Counts for steps that have changed since are dropped.
                    if tab.step_hashes == r.hashes {
                        tab.step_counts = r.counts;
                    }
                }
            }
            JobOutput::Summary(r) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == r.tab_id) {
                    // A result for an older pipeline is dropped; the newer job follows.
                    if tab.exec_steps == r.steps_len {
                        tab.summary_cells = r.cells;
                    }
                }
            }
            JobOutput::Schema(s) => {
                let SchemaApplied { tab_id, new_source, parse_failures, parse_note, row_check, mut status } = *s;
                let Some(idx) = self.tab_position(tab_id) else {
                    return;
                };
                if let Some(tab) = self.tabs.get_mut(idx) {
                    tab.source = new_source;
                    tab.parse_failures = parse_failures;
                    tab.row_check = row_check;
                }
                if let Some(n) = parse_note {
                    status.push_str(&format!(" {}", n));
                }
                self.active_tab = idx;
                self.refresh_tab_at(idx, true, None);
                self.status_message = status;
            }
            JobOutput::TextCheck(r) => self.apply_text_check(*r),
            JobOutput::Exported(o) => {
                self.status_message = o.message;
            }
            JobOutput::Batch(rep) => {
                if rep.cancelled {
                    self.status_message =
                        "Batch cancelled: files after the last one in batch_log.csv were not converted.".to_string();
                }
                self.batch_modal.last_report = Some(*rep);
                self.batch_modal.last_error = None;
            }
        }
    }
}

/// The status-bar text for a tab whose steps cannot all run: which step, why, and what is shown.
pub fn broken_step_message(tab: &TabState) -> Option<String> {
    let first = tab.issues.first()?;
    let step = tab.steps.get(first.index)?;
    Some(format!(
        "Step {} '{}' is broken: {} The table shows the result up to step {}.",
        first.index + 1,
        step.short_label(),
        first.reason,
        first.index
    ))
}
