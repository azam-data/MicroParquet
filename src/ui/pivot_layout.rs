//! The Nested layout of a pivot tab (Phase 9 P2): the view kept with the tab, built on the worker when the pivot runs, and the
//! switch between Side by side and Nested on a pivot tab. The pivot step's result is the flat table in both layouts; switching only
//! rebuilds this view.

use crate::ui::prelude::*;
use micro_parquet::pivot_view::{nested_pivot_followed, nested_pivot_of, NestedView, NESTED_NOTE};
use micro_parquet::tab_jobs::{compute_nested_view, NestedViewRequest, NestedViewResult};

/// What the app holds for a tab in the Nested layout.
pub struct NestedCache {
    /// `steps_edit::pipeline_hash` of the steps the view was built for. A view whose steps have changed is kept, dimmed, until the
    /// new one arrives.
    pub pipeline: u64,
    pub state: NestedState,
}

/// What the app holds for a pivot tab's notes and warnings (Phase 9 M3).
pub struct ReportCache {
    /// The steps and the model the report was made for: `steps_edit::pipeline_hash` and the model scope's fingerprint.
    pub key: (u64, u64),
    pub report: Option<micro_parquet::tab_jobs::PivotReport>,
    pub error: Option<String>,
}

pub enum NestedState {
    Ready(NestedView),
    /// The view could not be built: the reason (the tab shows the flat table and says so).
    Failed(String),
}

impl MicroParquetApp {
    /// Whether the tab's pivot is the last step it runs, in the Nested layout, and so has a view.
    fn wants_nested(tab: &TabState) -> bool {
        tab.exec_steps == tab.steps.len() && nested_pivot_of(&tab.steps).is_some()
    }

    /// The key a tab's report is made for: its steps and the model the workers read.
    fn report_key(&self, tab: &TabState) -> (u64, u64) {
        (micro_parquet::steps_edit::pipeline_hash(&tab.source, &tab.steps[..tab.exec_steps.min(tab.steps.len())]), self.project.model_ui.scope_fp)
    }

    /// Called every frame for the active tab: its report is made again when its steps or the model changed (a pivot made on a data
    /// tab has notes and warnings; any other tab has none).
    pub(crate) fn pivot_report_tick(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let id = tab.id;
        let steps = &tab.steps[..tab.exec_steps.min(tab.steps.len())];
        let wants = steps.iter().rposition(|s| !s.is_skipped()).and_then(|i| steps[i].as_pivot_spec()).is_some_and(|s| !s.data_tab.is_empty());
        if !wants {
            self.pivot_reports.remove(&id);
            return;
        }
        let key = self.report_key(tab);
        if self.pivot_reports.get(&id).is_some_and(|c| c.key == key) || self.jobs.is_busy(JobKey::tab(JobKind::PivotReport, id)) {
            return;
        }
        let req = micro_parquet::tab_jobs::PivotReportRequest { tab_id: id, source: tab.source.clone(), steps: steps.to_vec(), temp_dir: self.paths.temp_dir.clone() };
        self.pivot_report_key.insert(id, key);
        self.jobs.submit(JobKey::tab(JobKind::PivotReport, id), "Pivot notes", false, move |ctx| micro_parquet::tab_jobs::compute_pivot_report(req, ctx).map(|r| JobOutput::PivotReport(Box::new(r))));
    }

    /// The worker made a report: it is kept when the tab still has the steps (and the model) it was made for.
    pub(crate) fn apply_pivot_report(&mut self, r: micro_parquet::tab_jobs::PivotReportResult) {
        let Some(tab) = self.tabs.iter().find(|t| t.id == r.tab_id) else {
            return;
        };
        if micro_parquet::steps_edit::pipeline_hash(&tab.source, &tab.steps[..tab.exec_steps.min(tab.steps.len())]) != r.pipeline {
            return;
        }
        let key = (r.pipeline, self.pivot_report_key.get(&r.tab_id).map(|k| k.1).unwrap_or_default());
        let cache = match r.report {
            Ok(report) => ReportCache { key, report, error: None },
            Err(e) => ReportCache { key, report: None, error: Some(e) },
        };
        self.pivot_reports.insert(r.tab_id, cache);
    }

    /// The report the tab shows now: its warnings and why its blank cells are blank.
    pub(crate) fn pivot_report_for(&self, tab: &TabState) -> Option<&micro_parquet::tab_jobs::PivotReport> {
        let c = self.pivot_reports.get(&tab.id)?;
        if c.key != self.report_key(tab) {
            return None;
        }
        c.report.as_ref()
    }

    /// Builds the Nested view of tab `idx` on the worker, or drops the one it has when the tab no longer has one.
    pub(crate) fn submit_nested(&mut self, idx: usize) {
        let Some(tab) = self.tabs.get(idx) else {
            return;
        };
        let id = tab.id;
        if !Self::wants_nested(tab) {
            self.nested_views.remove(&id);
            return;
        }
        let req = NestedViewRequest { tab_id: id, source: tab.source.clone(), steps: tab.steps.clone(), temp_dir: self.paths.temp_dir.clone() };
        self.jobs.submit(JobKey::tab(JobKind::NestedView, id), "Nested pivot", false, move |ctx| compute_nested_view(req, ctx).map(|r| JobOutput::NestedView(Box::new(r))));
    }

    /// The worker built a view: it is kept when the tab still has the steps it was built for.
    pub(crate) fn apply_nested_view(&mut self, r: NestedViewResult) {
        let Some(tab) = self.tabs.iter().find(|t| t.id == r.tab_id) else {
            return;
        };
        if !Self::wants_nested(tab) || micro_parquet::steps_edit::pipeline_hash(&tab.source, &tab.steps) != r.pipeline {
            return;
        }
        let state = match r.view {
            Ok(Some(v)) => NestedState::Ready(v),
            Ok(None) => {
                self.nested_views.remove(&r.tab_id);
                return;
            }
            Err(e) => NestedState::Failed(e),
        };
        self.nested_views.insert(r.tab_id, NestedCache { pipeline: r.pipeline, state });
    }

    /// The Nested view the tab shows now, with whether it is the one for its current steps (a stale one is drawn dimmed).
    pub(crate) fn nested_for_display(&self, tab: &TabState) -> Option<(&NestedView, bool)> {
        if !Self::wants_nested(tab) {
            return None;
        }
        match &self.nested_views.get(&tab.id)?.state {
            NestedState::Ready(v) => {
                let fresh = self.nested_views.get(&tab.id).is_some_and(|c| c.pipeline == micro_parquet::steps_edit::pipeline_hash(&tab.source, &tab.steps));
                Some((v, fresh))
            }
            NestedState::Failed(_) => None,
        }
    }

    /// The note above a pivot tab's table about its layout: steps follow a Nested pivot, or its view could not be built.
    pub(crate) fn nested_note(&self, tab: &TabState) -> Option<String> {
        if nested_pivot_followed(&tab.steps).is_some() {
            return Some(NESTED_NOTE.to_string());
        }
        match self.nested_views.get(&tab.id).map(|c| &c.state) {
            Some(NestedState::Failed(e)) if Self::wants_nested(tab) => Some(format!("The Nested layout could not be built: {e}. The flat table is shown.")),
            _ => None,
        }
    }

    /// Switches the active pivot tab between Side by side and Nested, in place and with undo, without rebuilding the flat table.
    pub(crate) fn set_pivot_layout(&mut self, layout: PivotLayout) {
        let Some((i, mut spec)) = self.active_pivot() else {
            return;
        };
        if spec.layout == layout {
            return;
        }
        spec.layout = layout;
        let step = TransformStep::from_pivot_spec(spec);
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        tab.record_undo();
        tab.steps[i] = step;
        let id = tab.id;
        self.status_message = format!("Pivot layout: {}.", layout.label());
        self.selected_step = Some(i);
        // the table is the same: only the view is built again
        self.nested_views.remove(&id);
        self.submit_nested(self.active_tab);
    }
}
