//! Linked, editable chart tabs (Phase 8 W5).
//!
//! A chart tab draws a chart of a *source* tab. It remembers the source (`ChartTabInfo::source_tab`), what the source
//! looked like when the chart was drawn (`source_sig`) and the chart's own `ChartSpec` (`data.spec`). When the source's
//! steps, filters or file change, the chart is calculated again on the worker from the source, never from the chart's
//! own table, so "Sum of Sum of Balance" cannot happen. "Edit chart" opens the studio on the source with the chart tab's
//! spec; "Update chart tab" puts the studio's spec back into the chart tab, which is then calculated again.

use crate::ui::chart::{scale_word, ChartTabInfo, ClosedSource};
use crate::ui::prelude::*;
use micro_parquet::tab_jobs::{chart_tab_job, source_signature, ChartTabCalc, ChartTabRequest};

/// What a chart tab made by Phase 7 (it knows no source tab) says.
pub const OLDER_VERSION_NOTE: &str = "Made by an older version; Edit chart opens it on its own table";

impl MicroParquetApp {
    /// Called every frame: a chart tab whose source changed is calculated again on the worker.
    pub(crate) fn chart_tab_links_tick(&mut self, now: f64) {
        if self.chart_tabs.is_empty() || now - self.chart_tab_checked_at < theme::CHART_TAB_LINK_CHECK_S {
            return;
        }
        self.chart_tab_checked_at = now;
        let ids: Vec<u64> = self.chart_tabs.keys().copied().collect();
        for id in ids {
            let Some(info) = self.chart_tabs.get(&id) else { continue };
            let Some(sid) = info.source_tab else { continue };
            // a source that is still being opened is waited for
            let Some(src) = self.tabs.iter().find(|t| t.id == sid) else { continue };
            let exec = src.exec_steps.min(src.steps.len());
            let sig = source_signature(&src.source, &src.steps[..exec]);
            if sig == info.source_sig || info.pending_sig.as_deref() == Some(sig.as_str()) {
                continue;
            }
            let req = ChartTabRequest { chart_tab: id, sig: sig.clone(), base: src.source.clone(), steps: src.steps[..exec].to_vec(), spec: info.data.spec.clone(), temp_dir: self.paths.temp_dir.clone() };
            if let Some(info) = self.chart_tabs.get_mut(&id) {
                info.pending_sig = Some(sig);
            }
            self.jobs.submit(JobKey::tab(JobKind::ChartTab, id), "Calculating the chart tab", false, move |ctx| chart_tab_job(req, ctx));
        }
    }

    /// The worker calculated a chart tab again: its numbers, its table and (when it still has the rule's title) its title.
    pub(crate) fn apply_chart_tab_calc(&mut self, c: ChartTabCalc) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == c.chart_tab) else { return };
        let Some(info) = self.chart_tabs.get(&c.chart_tab) else { return };
        if info.pending_sig.as_deref() != Some(c.sig.as_str()) {
            return;
        }
        let old_rule = format!("Chart: {}", micro_parquet::charts::layout::chart_title(&info.data));
        let src = info.source_tab.and_then(|s| self.tabs.iter().find(|t| t.id == s));
        let scale = self.chart_scale_of(&c.data.spec, src);
        let (divisor, unit) = (scale.map(|s| s.divisor).unwrap_or(1.0), scale.as_ref().map(scale_word).unwrap_or("").to_string());
        let formats = self.chart_data_formats(&c.data);
        let new_title = format!("Chart: {}", micro_parquet::charts::layout::chart_title(&c.data));
        let old_title = self.tabs[idx].title.clone();
        if let Some(info) = self.chart_tabs.get_mut(&c.chart_tab) {
            info.pending_sig = None;
            info.source_sig = c.sig;
            info.error = None;
            info.divisor = divisor;
            info.unit = unit;
            info.data = c.data;
        }
        let tab = &mut self.tabs[idx];
        tab.source = c.source;
        tab.col_formats.extend(formats);
        let renamed = old_title == old_rule && old_title != new_title;
        if renamed {
            tab.title = new_title.clone();
        }
        if renamed {
            let spec = self.chart_tabs.get(&c.chart_tab).and_then(|i| serde_json::to_value(&i.data.spec).ok());
            for e in self.settings.chart_tabs.iter_mut() {
                if e.get("title").and_then(|t| t.as_str()) == Some(old_title.as_str()) {
                    e["title"] = serde_json::Value::String(new_title.clone());
                    if let Some(s) = &spec {
                        e["spec"] = s.clone();
                    }
                }
            }
            self.save_settings();
        }
        self.refresh_tab_at(idx, true, None);
    }

    /// The chart could not be calculated again: the tab keeps its last numbers and says why.
    pub(crate) fn chart_tab_calc_failed(&mut self, chart_tab: u64, msg: &str) {
        if let Some(info) = self.chart_tabs.get_mut(&chart_tab) {
            // the attempt is not made again until the source changes once more
            info.source_sig = info.pending_sig.take().unwrap_or_default();
            info.error = Some(msg.to_string());
        }
    }

    /// A tab is about to close: the chart tabs drawn from it keep what is needed to reopen it.
    pub(crate) fn chart_tabs_source_closing(&mut self, idx: usize) {
        let Some(t) = self.tabs.get(idx) else { return };
        let closed = ClosedSource { title: t.title.clone(), key: self.project.keys.get(&t.id).cloned(), source: t.source.clone(), steps: t.steps.clone(), col_formats: t.col_formats.clone() };
        let id = t.id;
        for info in self.chart_tabs.values_mut().filter(|i| i.source_tab == Some(id)) {
            info.closed = Some(closed.clone());
            info.source_tab = None;
            info.pending_sig = None;
        }
    }

    /// The closed source of a chart tab can be opened again: it is a file tab of the open project.
    pub(crate) fn chart_tab_can_reopen(&self, c: &ClosedSource) -> bool {
        self.project.active && c.key.is_some() && matches!(&c.source, TabSource::SingleFile { path, .. } if !path.starts_with(&self.paths.temp_dir))
    }

    /// "Reopen": the source tab comes back and the chart tab follows it again.
    pub(crate) fn chart_tab_reopen(&mut self, chart_tab: u64) {
        let Some(c) = self.chart_tabs.get(&chart_tab).and_then(|i| i.closed.clone()) else { return };
        if !self.chart_tab_can_reopen(&c) {
            return;
        }
        let id = self.create_tab_from_source_with(c.title.clone(), c.source.clone(), c.steps.clone(), c.col_formats.clone(), None);
        if let Some(k) = c.key {
            self.project.keys.insert(id, k);
        }
        if let Some(info) = self.chart_tabs.get_mut(&chart_tab) {
            info.source_tab = Some(id);
            info.closed = None;
        }
        self.status_message = format!("Reopening '{}'.", c.title);
    }

    /// "Edit chart": the studio opens on the source tab with the chart tab's spec (a chart tab of an older version has
    /// no source: the studio opens on its own table).
    pub(crate) fn edit_chart_tab(&mut self, chart_tab: u64) {
        let Some(info) = self.chart_tabs.get(&chart_tab) else { return };
        let spec = info.data.spec.clone();
        let linked = info.source_tab.is_some();
        if let Some(c) = &info.closed {
            self.status_message = format!("The source tab '{}' is closed. Reopen it to edit this chart.", c.title);
            return;
        }
        let target = info.source_tab.unwrap_or(chart_tab);
        let Some(idx) = self.tabs.iter().position(|t| t.id == target) else {
            self.status_message = "The source tab is still being opened. Try again in a moment.".to_string();
            return;
        };
        self.active_tab = idx;
        self.chart_attach_tab_pub();
        if linked {
            self.chart_set_spec(spec);
        }
        self.chart.editing = Some(chart_tab);
        self.set_side_tab_visible(crate::ui::steps::RightTab::Chart, true);
    }

    /// "Update chart tab": the studio's chart replaces the chart tab's own; the tab is calculated again from the source.
    pub(crate) fn update_chart_tab(&mut self) {
        let Some(id) = self.chart.editing.take() else { return };
        let spec = self.chart.spec.clone();
        let Some(idx) = self.tabs.iter().position(|t| t.id == id) else { return };
        let Some(info) = self.chart_tabs.get_mut(&id) else { return };
        info.data.spec = spec;
        info.source_sig.clear();
        info.pending_sig = None;
        info.error = None;
        self.active_tab = idx;
        self.status_message = "Updating the chart tab\u{2026}".to_string();
    }

    /// "Open as new chart tab": the studio's chart in a tab of its own; the chart tab being edited stays as it was.
    pub(crate) fn editing_open_as_new_chart_tab(&mut self) {
        self.chart.editing = None;
        self.chart_open_as_chart_tab();
    }

    /// The line over the studio while a chart tab is being edited.
    pub(crate) fn chart_editing_banner(&mut self, ui: &mut egui::Ui, rects: &mut crate::ui::chart::ChartRects) {
        let Some(id) = self.chart.editing else { return };
        let Some(tab) = self.tabs.iter().find(|t| t.id == id) else {
            self.chart.editing = None;
            return;
        };
        let title = tab.title.clone();
        let linked = self.chart_tabs.get(&id).is_some_and(|i| i.source_tab.is_some());
        let tk = *theme::tokens(self.dark_mode);
        let mut update = false;
        let mut new_tab = false;
        egui::Frame::none().fill(tk.accent_soft).rounding(egui::Rounding::same(theme::RADIUS_PANEL)).inner_margin(theme::MARGIN_STEP_ROW).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(egui::RichText::new(format!("Editing chart tab '{}'", title)).color(tk.ink)).wrap());
            if !linked {
                ui.add(egui::Label::new(egui::RichText::new(OLDER_VERSION_NOTE).color(tk.muted)).wrap());
            }
            ui.horizontal_wrapped(|ui| {
                if linked {
                    let b = ui.button("Update chart tab");
                    rects.fields.push(("Update chart tab".to_string(), b.rect));
                    update = b.clicked();
                }
                let b = ui.button("Open as new chart tab");
                rects.fields.push(("Open as new chart tab".to_string(), b.rect));
                new_tab = b.clicked();
            });
        });
        ui.add_space(theme::SPACE_S);
        if update {
            self.update_chart_tab();
        } else if new_tab {
            self.editing_open_as_new_chart_tab();
        }
    }

    /// What the steps panel says on a chart tab (its steps are the source tab's, so they are not listed).
    pub(crate) fn chart_tab_steps_note(&self) -> Option<String> {
        let id = self.tabs.get(self.active_tab)?.id;
        let info = self.chart_tabs.get(&id)?;
        let name = match (&info.closed, info.source_tab) {
            (Some(c), _) => Some(c.title.clone()),
            (None, Some(s)) => self.tabs.iter().find(|t| t.id == s).map(|t| t.title.clone()).or_else(|| Some("its source tab".to_string())),
            _ => None,
        };
        Some(match name {
            Some(n) => format!("This tab draws a chart of '{}'. Change the data on that tab, or use Edit chart.", n),
            None => OLDER_VERSION_NOTE.to_string(),
        })
    }
}

impl ChartTabInfo {
    /// A chart tab drawn from `source_tab`, which looked like `source_sig` when the numbers were calculated.
    pub fn linked(data: micro_parquet::chart::ChartData, divisor: f64, decimals: usize, unit: String, source_tab: Option<u64>, source_sig: String) -> Self {
        Self { data, show_table: false, divisor, decimals, unit, source_tab, source_sig, pending_sig: None, closed: None, error: None }
    }
}
