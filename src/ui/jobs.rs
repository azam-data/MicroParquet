//! Background jobs in the UI (Phase 3 R2): submitting work to the worker thread and applying the
//! results each frame. All Polars work runs in `micro_parquet::tab_jobs`.

use crate::ui::prelude::*;

/// Carried from a refresh that a newer one replaced, so the newer one still does that work.
#[derive(Default, Clone)]
pub struct PendingRefresh {
    pub recalculate_baseline: bool,
    pub added_status: Option<String>,
}

/// What happens when the window is asked to close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseDecision {
    /// An export is running: ask "Export in progress — cancel and quit?" first.
    Ask,
    /// Jobs were cancelled and unfinished output files deleted: let the window close.
    Exit,
}

/// How long closing waits for a cancelled job to reach a safe point.
pub const CLOSE_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

impl MicroParquetApp {
    /// True while a job that writes files (an export, a conversion or a batch) is queued or running.
    pub(crate) fn export_running(&self) -> bool {
        self.jobs.kind_busy(JobKind::Export) || self.jobs.kind_busy(JobKind::Batch)
    }

    /// The window was asked to close. With an export running (and not yet confirmed), the question
    /// opens and the close is held; otherwise every job is cancelled and `.part` files are deleted.
    pub(crate) fn close_requested(&mut self) -> CloseDecision {
        if self.export_running() && !self.quit_confirmed {
            self.quit_guard = true;
            return CloseDecision::Ask;
        }
        self.shutdown_jobs();
        CloseDecision::Exit
    }

    /// Cancels every job, waits a moment for the running one to stop, then deletes the `.part`
    /// files of unfinished outputs. Returns the text said about it (also printed to stderr).
    pub(crate) fn shutdown_jobs(&mut self) -> String {
        self.jobs.cancel_all();
        let idle = self.jobs.wait_idle(CLOSE_WAIT);
        let cleanup = micro_parquet::jobs::remove_part_files_recording(&self.paths.root_dir);
        let mut msg = format!("Closing: jobs cancelled, {} unfinished output file(s) deleted.", cleanup.removed);
        if !idle {
            msg.push_str(" A job was still running and is stopped with the program.");
        }
        if !cleanup.failed.is_empty() {
            msg.push_str(&format!(" Could not delete now: {}.", cleanup.failed.join("; ")));
            match &cleanup.record_error {
                None => msg.push_str(" They are listed in portable_data and will be deleted at the next start."),
                Some(e) => msg.push_str(&format!(" They could not be listed for the next start ({}); delete them by hand.", e)),
            }
        }
        eprintln!("[MicroParquet] {}", msg);
        self.status_message = msg.clone();
        msg
    }

    /// The question shown when the window is closed during an export.
    pub(crate) fn render_quit_guard(&mut self, ctx: &egui::Context) {
        if !self.quit_guard {
            return;
        }
        let mut quit = false;
        let mut stay = false;
        theme::modal("Export in progress \u{2014} cancel and quit?").show(ctx, |d| {
            d.body(|ui| {
                ui.label("The export is stopped and its unfinished file is deleted; no partial file is left.");
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    if theme::primary_button(ui, "Cancel export and quit").clicked() {
                        quit = true;
                    }
                    if ui.button("Keep working").clicked() {
                        stay = true;
                    }
                });
            });
        });
        if quit {
            self.quit_guard = false;
            self.quit_confirmed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if stay {
            self.quit_guard = false;
        }
    }

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
                    JobKind::TextCheck => {
                        self.drop_pending_export();
                        self.status_message = format!("Export stopped: could not check the Text columns: {}", msg);
                    }
                    JobKind::Load => self.status_message = format!("Pipeline error: {}", msg),
                    JobKind::PivotPreview => self.pivot_preview_failed(msg),
                    JobKind::PivotValues => {
                        if self.sort_lists.fill_wait.take().is_some() {
                            self.sort_lists.message = Some((true, format!("Could not read the values: {}", msg)));
                        } else if let Some(p) = self.pivot_bar.popup.as_mut().filter(|_| !self.pivot_builder.open) {
                            p.values = Some(Err(msg));
                        } else if let Some(p) = self.pivot_builder.popup.as_mut() {
                            p.values = Some(Err(msg));
                        }
                    }
                    JobKind::Schema => self.status_message = format!("Schema not applied: {}", msg),
                    JobKind::SqlCheck => {
                        if let Some(tab) = r.key.scope.and_then(|id| self.tabs.iter_mut().find(|t| t.id == id)) {
                            tab.sql_error = Some(msg.clone());
                        }
                        self.status_message = format!("The query was not checked: {}", msg);
                    }
                    // The band must not keep values of an older result: "—" with the reason.
                    JobKind::Summary => {
                        if let Some(tab) = r.key.scope.and_then(|id| self.tabs.iter_mut().find(|t| t.id == id)) {
                            tab.summary_cells.clear();
                            tab.summary_error = Some(msg.clone());
                        }
                        if let Some(l) = line {
                            self.status_message = l;
                        }
                    }
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
                    if kind == JobKind::TextCheck {
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

    pub(crate) fn tab_position(&self, tab_id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == tab_id)
    }

    fn apply_job_output(&mut self, out: JobOutput) {
        match out {
            JobOutput::Loaded(l) => {
                let LoadedTab { mut tab, status } = *l;
                let status = match self.open_notes.remove(&tab.id) {
                    Some(note) => format!("{} {}", note, status),
                    None => status,
                };
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
                        tab.page_error = None;
                    } else if let Some(e) = &r.page_error {
                        // The last page stays on screen, dimmed and marked "Out of date".
                        tab.page_error = Some(e.clone());
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
                let pipeline_failed = r.error.is_some();
                match (r.error, r.status) {
                    (Some(e), _) => self.status_message = format!("Step execution failed: {}", e),
                    (None, Some(s)) => self.status_message = s,
                    (None, None) => {}
                }
                // A broken step is always said, whatever else the status was.
                if let Some(msg) = self.tabs.get(idx).and_then(broken_step_message) {
                    self.status_message = msg;
                }
                // A page that could not be read is said last, so it is never hidden.
                if !pipeline_failed {
                    if let Some(e) = &r.page_error {
                        self.status_message = page_error_message(e);
                    }
                }
                if idx == self.active_tab {
                    self.init_tool_defaults_from_active_tab();
                }
                self.apply_pivot_formats(idx);
                // The summary row is of the whole result, so it follows every refresh.
                self.submit_summary(idx);
                self.submit_step_counts(idx);
                self.submit_column_stats(idx);
            }
            JobOutput::PivotPreview(r) => self.apply_pivot_preview(*r),
            JobOutput::PivotValues(r) => {
                if !self.apply_sort_list_fill(&r) && !self.apply_pivot_bar_values(&r) {
                    self.apply_pivot_values(*r)
                }
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
                    // A result for another pipeline (steps, settings, on/off, source) is
                    // dropped; the newer job follows.
                    let exec = tab.exec_steps.min(tab.steps.len());
                    if micro_parquet::steps_edit::pipeline_hash(&tab.source, &tab.steps[..exec]) == r.pipeline {
                        tab.summary_cells = r.cells;
                        tab.summary_error = None;
                    }
                }
            }
            JobOutput::Schema(s) => {
                if !s.new_breaks.is_empty() {
                    // steps that worked would stop working: the owner is asked first (U1)
                    self.schema_confirm = Some(*s);
                    return;
                }
                self.finish_schema_applied(*s);
            }
            JobOutput::SqlChecked(r) => self.apply_sql_checked(*r),
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

    /// The tab takes the new schema (and the steps that go with it) and reloads.
    pub(crate) fn finish_schema_applied(&mut self, s: SchemaApplied) {
        let SchemaApplied { tab_id, new_source, parse_failures, parse_note, row_check, mut status, steps, new_breaks: _ } = s;
        let Some(idx) = self.tab_position(tab_id) else {
            return;
        };
        if let Some(tab) = self.tabs.get_mut(idx) {
            tab.source = new_source;
            tab.steps = steps;
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

/// The status-bar text when a page could not be read.
pub fn page_error_message(reason: &str) -> String {
    format!("The page could not be read: {} The table shows an out-of-date page.", reason)
}
