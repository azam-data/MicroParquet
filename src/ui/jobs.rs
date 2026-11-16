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
                let LoadedTab { tab, status } = *l;
                self.tabs.push(tab);
                self.active_tab = self.tabs.len() - 1;
                self.init_tool_defaults_from_active_tab();
                self.status_message = status;
            }
            JobOutput::Refreshed(r) => {
                let Some(idx) = self.tab_position(r.tab_id) else {
                    return;
                };
                self.pending_refresh.remove(&r.tab_id);
                if let Some(tab) = self.tabs.get_mut(idx) {
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
                if idx == self.active_tab {
                    self.init_tool_defaults_from_active_tab();
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
