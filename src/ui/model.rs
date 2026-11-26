//! The Model menu (Phase 9 M0): Links…, Key check…, the checks that run when a link is made, a project opens or a tab changes, the
//! open line and the status bar's "Model: 4 links · 1 broken".

use crate::ui::prelude::*;
use micro_parquet::jobs::{JobKey, JobKind};
use polars::prelude::DataType;
use micro_parquet::model::{self, KeyCheck, KeyPair, Link, LinkStatus, ModelTab, TabInfo, TabKind};
use micro_parquet::tab_jobs::{compute_model_check, compute_model_rows, JobOutput, ModelCheckRequest, ModelCheckResult, ModelList, ModelRowsRequest, ModelRowsResult};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

pub const MODEL_TITLE: &str = "Model";
pub const LINKS_TITLE: &str = "Links";
pub const KEY_CHECK_TITLE: &str = "Key check";

/// What the app knows about one link: what the last key check found, and for which state of the two tabs.
#[derive(Clone, Debug)]
pub struct LinkRuntime {
    pub status: LinkStatus,
    pub check: Option<KeyCheck>,
    /// The state of both tabs (steps, source) and of the link that the last result was made for.
    pub fingerprint: Option<u64>,
    pub in_flight: bool,
}

/// A link being added or edited in the Links dialog.
#[derive(Clone, Debug, Default)]
pub struct LinkDraft {
    /// The place of the link being edited (`None`: a new link).
    pub editing: Option<usize>,
    pub data_tab: Option<u64>,
    pub lookup_tab: Option<u64>,
    /// The key column pairs: (data column, lookup column), empty until picked.
    pub pairs: Vec<(String, String)>,
    pub error: String,
    /// A budget or target table link (Phase 11): the lookup tab is the plan table and the month column is its date.
    pub plan: bool,
    pub month_column: String,
}

/// The lookup columns a data tab can use as fields, under one link (Phase 9 M1): "Branch master ›" and its columns.
#[derive(Clone, Debug)]
pub struct LookupGroup {
    pub link: Link,
    /// "Accounts → Branch master".
    pub link_name: String,
    pub lookup_title: String,
    /// Why the link can't be used, when it can't: the fields are listed greyed with this.
    pub refusal: Option<String>,
    pub columns: Vec<(micro_parquet::model::ModelFieldRef, DataType)>,
}

/// What the model's dialogs drew in the last frame (the render tests read it).
#[derive(Clone, Debug, Default)]
pub struct ModelRects {
    pub buttons: Vec<(String, egui::Rect)>,
    /// The boxes of the Add/Edit form: "Data tab", "Lookup tab", "Data key 1", "Lookup key 1", ...
    pub combos: Vec<(String, egui::Rect)>,
    /// One per link of the Links list: the row's first cell.
    pub rows: Vec<(usize, egui::Rect)>,
    pub statuses: Vec<(usize, egui::Rect)>,
    /// The boxes of the Key check: (link, name), e.g. (0, "Show duplicates").
    pub section_buttons: Vec<(usize, String, egui::Rect)>,
    pub error: Option<egui::Rect>,
    pub message: Option<egui::Rect>,
}

#[derive(Default)]
pub struct ModelState {
    pub runtime: HashMap<(String, String), LinkRuntime>,
    pub draft: Option<LinkDraft>,
    pub selected: Option<usize>,
    /// The selected budget or target table of the Links dialog (Phase 11).
    pub plan_selected: Option<usize>,
    pub rects: ModelRects,
    /// The open line waiting for the key checks: "9 tabs opened."
    pub open_base: Option<String>,
    pub file_stamp: u64,
    /// What Copy list put on the clipboard last (tests read it).
    pub last_copied: String,
    /// The model the workers read (Phase 9 M1), and what it was made from.
    pub scope: Option<std::sync::Arc<model::ModelContext>>,
    pub scope_fp: u64,
    /// The title each linked tab had when it was last open, by project key: a tab that is closed is still named in the messages.
    pub titles: HashMap<String, String>,
    /// The snapshot dates of each date-linked tab, by project key (Phase 9 M2).
    pub date_info: HashMap<String, crate::ui::model_calendar::DateInfoRuntime>,
    /// The Calendar and As-of date dialogs (Phase 9 M2).
    pub cal: crate::ui::model_calendar::CalUi,
    /// The Measures dialog (Phase 9 M3).
    pub measures: crate::ui::model_measures::MeasuresUi,
}

fn fingerprint<T: Hash>(v: &T) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

impl MicroParquetApp {
    // ---------------------------------------------------------------------------- the tabs of the model

    /// The tab (by its id) that has this project key.
    pub(crate) fn model_tab_id(&self, key: &str) -> Option<u64> {
        self.project.keys.iter().find(|(_, k)| k.as_str() == key).map(|(id, _)| *id).filter(|id| self.tabs.iter().any(|t| t.id == *id))
    }

    /// The project key of a tab; a tab that has none yet gets the next one (the same numbering a save gives).
    pub(crate) fn model_ensure_key(&mut self, id: u64) -> String {
        if let Some(k) = self.project.keys.get(&id) {
            return k.clone();
        }
        let mut next = self.project.keys.values().filter_map(|k| k.strip_prefix('t').and_then(|n| n.parse::<u32>().ok())).max().unwrap_or(0);
        let mut k;
        loop {
            next += 1;
            k = format!("t{}", next);
            if !self.project.keys.values().any(|x| *x == k) && !self.project.model.links.iter().any(|l| l.data_tab == k || l.lookup_tab == k) {
                break;
            }
        }
        self.project.keys.insert(id, k.clone());
        k
    }

    pub(crate) fn model_tab_info(&self, id: u64) -> Option<TabInfo> {
        let t = self.tabs.iter().find(|t| t.id == id)?;
        let kind = if self.chart_tabs.contains_key(&id) {
            TabKind::Chart
        } else if matches!(t.source, TabSource::Unavailable { .. }) {
            TabKind::Placeholder
        } else {
            TabKind::Table
        };
        let key = self.project.keys.get(&id).cloned().unwrap_or_else(|| format!("#{}", id));
        Some(TabInfo { key, title: t.title.clone(), kind, columns: t.cached_slice.get_columns().iter().map(|c| (c.name().to_string(), c.dtype().clone())).collect() })
    }

    pub(crate) fn model_worker_tab(&self, key: &str) -> Option<ModelTab> {
        let id = self.model_tab_id(key)?;
        let t = self.tabs.iter().find(|t| t.id == id)?;
        if matches!(t.source, TabSource::Unavailable { .. }) || self.chart_tabs.contains_key(&id) {
            return None;
        }
        let n = t.exec_steps.min(t.steps.len());
        Some(ModelTab { key: key.to_string(), title: t.title.clone(), source: t.source.clone(), steps: t.steps[..n].to_vec() })
    }

    /// "Accounts" for a tab key, or the key and "(not open)".
    pub(crate) fn model_tab_title(&self, key: &str) -> String {
        match self.model_tab_id(key).and_then(|id| self.tabs.iter().find(|t| t.id == id)) {
            Some(t) => t.title.clone(),
            None => format!("{} (not open)", self.project.model_ui.titles.get(key).map(String::as_str).unwrap_or(key)),
        }
    }

    pub(crate) fn model_link_name(&self, l: &Link) -> String {
        model::link_name(&self.model_tab_title(&l.data_tab), &self.model_tab_title(&l.lookup_tab))
    }

    pub(crate) fn model_status_of(&self, l: &Link) -> LinkStatus {
        self.project.model_ui.runtime.get(&(l.data_tab.clone(), l.lookup_tab.clone())).map(|r| r.status.clone()).unwrap_or(LinkStatus::Checking)
    }

    /// How many links the model has and how many of them cannot be used.
    pub(crate) fn model_counts(&self) -> (usize, usize) {
        let links = &self.project.model.links;
        (links.len(), links.iter().filter(|l| self.model_status_of(l).is_broken()).count())
    }

    // ---------------------------------------------------------------------------- lookup fields (M1)

    /// The links of a tab as a data tab, each with the lookup's columns as fields. Empty outside a project and for a tab with no links.
    pub(crate) fn model_groups_for(&self, tab_id: u64) -> Vec<LookupGroup> {
        if !self.project.active {
            return Vec::new();
        }
        let Some(key) = self.project.keys.get(&tab_id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for l in self.project.model.all_links().iter().filter(|l| &l.data_tab == key) {
            let title = self.model_tab_title(&l.lookup_tab).trim_end_matches(" (not open)").to_string();
            let name = self.model_link_name(l);
            let columns = self
                .model_tab_id(&l.lookup_tab)
                .and_then(|id| self.model_tab_info(id))
                .map(|i| i.columns.into_iter().map(|(c, dt)| (model::ModelFieldRef::new(l, &name, &title, &c), dt)).collect())
                .unwrap_or_default();
            out.push(LookupGroup { link: l.clone(), link_name: name, lookup_title: title.clone(), refusal: self.model_status_of(l).refusal(&title), columns });
        }
        out
    }

    /// The lookup field of this tab with this name ("Branch master · Region"), and the reason it can't be used when its link is broken.
    pub(crate) fn model_field_named(&self, tab_id: u64, name: &str) -> Option<(model::ModelFieldRef, DataType, Option<String>)> {
        self.model_groups_for(tab_id).into_iter().find_map(|g| g.columns.iter().find(|(r, _)| r.name == name).map(|(r, dt)| (r.clone(), dt.clone(), g.refusal.clone())))
    }

    /// The order the lookup's own steps declare for a column (Split date parts, unnumbered rate bands), when a lookup field is added.
    pub(crate) fn model_declared_order(&self, f: &model::ModelFieldRef) -> Option<micro_parquet::sort_lists::SortOrder> {
        // a Calendar field has the order of its date part (Phase 9 M2): months in time, days from the week start
        if let Some(c) = self.project.model.calendar.as_ref().filter(|c| c.tab == f.lookup_tab) {
            return micro_parquet::dates::DatePart::from_label(&f.column).and_then(|p| p.sort_order(&c.settings));
        }
        let t = self.model_worker_tab(&f.lookup_tab)?;
        micro_parquet::engine::column_orders(&t.steps).remove(&f.column)
    }

    // ---------------------------------------------------------------------------- the model the workers read (M1)

    /// Why a tab can't be part of a check, in the words of the status: "the tab 'Branch master' is closed".
    fn model_tab_problem(&self, key: &str) -> String {
        match self.model_tab_id(key).and_then(|id| self.tabs.iter().find(|t| t.id == id)) {
            None => format!("the tab '{}' is closed", self.model_tab_title(key).trim_end_matches(" (not open)")),
            Some(t) if matches!(t.source, TabSource::Unavailable { .. }) => format!("the tab '{}' did not open", t.title),
            Some(t) => format!("'{}' is a chart tab, not a table", t.title),
        }
    }

    /// Makes the model of the open project the one this thread's jobs read: the links, the lookups read after their own steps, and
    /// the reason a link can't be used. Called every frame; it rebuilds only when something it is made from changed.
    pub(crate) fn model_scope_refresh(&mut self) {
        if !self.project.active || (self.project.model.all_links().is_empty() && self.project.model.plans.is_empty()) {
            self.project.model_ui.scope = None;
            self.project.model_ui.scope_fp = 0;
            model::set_scope(None);
            return;
        }
        let links = self.project.model.all_links();
        let mut parts: Vec<(Link, Option<ModelTab>, Option<String>)> = Vec::new();
        for l in &links {
            let lookup = self.model_worker_tab(&l.lookup_tab);
            let title = self.model_tab_title(&l.lookup_tab).trim_end_matches(" (not open)").to_string();
            let refusal = self.model_status_of(l).refusal(&title);
            parts.push((l.clone(), lookup, refusal));
        }
        // the model calendar, the as-of date and the column tags: what the time aggregations and the stock warning read (M3)
        let calendar = self.project.model.calendar.as_ref().filter(|c| !c.tab.is_empty()).map(|c| model::CalendarScope {
            settings: c.settings,
            tab: c.tab.clone(),
            date_links: c.date_links.clone(),
            as_of: self.model_as_of().map(|(d, _)| d),
            titles: c.date_links.iter().map(|l| (l.data_tab.clone(), self.model_tab_title(&l.data_tab).trim_end_matches(" (not open)").to_string())).collect(),
        });
        let kinds = self.project.model.column_kinds.clone();
        let measures = self.project.model.measures.clone();
        let what_if = self.project.model.what_if.scope();
        // the budget and target tables (Phase 11): each with the plan table as the worker reads it
        let plan_parts: Vec<(model::PlanLink, Option<ModelTab>)> = self.project.model.plans.iter().map(|p| (p.clone(), self.model_worker_tab(&p.plan_tab))).collect();
        let fp = fingerprint(&(
            plan_parts.iter().map(|(l, k)| (format!("{:?}", l), k.as_ref().map(|k| micro_parquet::steps_edit::pipeline_hash(&k.source, &k.steps)))).collect::<Vec<_>>(),
            parts.iter().map(|(l, k, r)| (format!("{:?}", l), k.as_ref().map(|k| micro_parquet::steps_edit::pipeline_hash(&k.source, &k.steps)), r.clone())).collect::<Vec<_>>(),
            self.paths.temp_dir.clone(),
            format!("{:?}", (&calendar, &kinds, &measures, &what_if)),
        ));
        if self.project.model_ui.scope.is_none() || self.project.model_ui.scope_fp != fp {
            let temp_dir = self.paths.temp_dir.clone();
            let plans: Vec<model::ScopePlan> = plan_parts
                .into_iter()
                .map(|(link, table)| {
                    let refusal = match &table {
                        None => Some(format!("The plan table of '{}' is not open. Open it to use its budget or target.", link.data_tab)),
                        Some(t) => match model::check_plan(&link, t, &temp_dir) {
                            Ok(c) => c.refusal(&t.title),
                            Err(e) => Some(e.to_string()),
                        },
                    };
                    model::ScopePlan { link, table, refusal }
                })
                .collect();
            let ctx = model::ModelContext {
                plans,
                links: parts.into_iter().map(|(link, lookup, refusal)| model::ScopeLink { link, lookup, refusal }).collect(),
                temp_dir: self.paths.temp_dir.clone(),
                calendar,
                kinds,
                measures,
                what_if,
            };
            self.project.model_ui.scope = Some(std::sync::Arc::new(ctx));
            self.project.model_ui.scope_fp = fp;
            // the model changed (the as-of date, a tag, the calendar): a pivot made on a data tab is worked out again (M3)
            let stale: Vec<usize> = (0..self.tabs.len()).filter(|i| self.tabs[*i].steps.iter().any(|s| s.as_pivot_spec().is_some_and(|p| p.has_time_values()))).collect();
            if !stale.is_empty() {
                model::set_scope(self.project.model_ui.scope.clone());
                for i in stale {
                    self.refresh_tab_at(i, false, None);
                }
            }
        }
        model::set_scope(self.project.model_ui.scope.clone());
    }

    // ---------------------------------------------------------------------------- the checks

    fn model_link_fingerprint(&self, l: &Link, d: &ModelTab, k: &ModelTab) -> u64 {
        fingerprint(&(micro_parquet::steps_edit::pipeline_hash(&d.source, &d.steps), micro_parquet::steps_edit::pipeline_hash(&k.source, &k.steps), format!("{:?}", l)))
    }

    /// Called every frame: sends a link to the worker when it was never checked or when either tab or the link changed since, and
    /// writes the open line once every check is back.
    pub(crate) fn model_tick(&mut self) {
        // a join that found repeated keys marks its link Broken at once; the link is then checked again for the full picture (M1-6)
        for (d, k, status) in model::take_broken_at_join() {
            if self.project.model.all_links().iter().any(|l| l.data_tab == d && l.lookup_tab == k) {
                self.project.model_ui.runtime.insert((d, k), LinkRuntime { status, check: None, fingerprint: None, in_flight: false });
            }
        }
        self.model_calendar_tick();
        if !self.project.active || self.project.opening.is_some() {
            return;
        }
        let links = self.project.model.all_links();
        for l in &links {
            for key in [&l.data_tab, &l.lookup_tab] {
                if let Some(t) = self.model_tab_id(key).and_then(|id| self.tabs.iter().find(|t| t.id == id)).map(|t| t.title.clone()) {
                    self.project.model_ui.titles.insert(key.clone(), t);
                }
            }
        }
        self.project.model_ui.runtime.retain(|k, _| links.iter().any(|l| (&l.data_tab, &l.lookup_tab) == (&k.0, &k.1)));
        for l in &links {
            let ident = (l.data_tab.clone(), l.lookup_tab.clone());
            let (d, k) = (self.model_worker_tab(&l.data_tab), self.model_worker_tab(&l.lookup_tab));
            let (Some(d), Some(k)) = (d.clone(), k) else {
                let which = if d.is_none() { &l.data_tab } else { &l.lookup_tab };
                let why = self.model_tab_problem(which);
                self.project.model_ui.runtime.insert(ident, LinkRuntime { status: LinkStatus::Failed(why), check: None, fingerprint: None, in_flight: false });
                continue;
            };
            let fp = self.model_link_fingerprint(l, &d, &k);
            let rt = self.project.model_ui.runtime.entry(ident).or_insert(LinkRuntime { status: LinkStatus::Checking, check: None, fingerprint: None, in_flight: false });
            if rt.fingerprint == Some(fp) || rt.in_flight {
                continue;
            }
            rt.in_flight = true;
            let req = ModelCheckRequest { link: l.clone(), data: d, lookup: k, fingerprint: fp, temp_dir: self.paths.temp_dir.clone() };
            self.jobs.submit(JobKey::unique(JobKind::ModelCheck), "Checking the keys", false, move |ctx| compute_model_check(req, ctx).map(|r| JobOutput::ModelCheck(Box::new(r))));
        }
        // the open line: the broken links, once none is still being checked
        if self.project.model_ui.open_base.is_some() {
            let pending = links.iter().any(|l| self.project.model_ui.runtime.get(&(l.data_tab.clone(), l.lookup_tab.clone())).map_or(true, |r| r.in_flight || r.fingerprint.is_none() && !matches!(r.status, LinkStatus::Failed(_))));
            if !pending {
                let base = self.project.model_ui.open_base.take().unwrap_or_default();
                let broken: Vec<(String, String)> = links.iter().filter(|l| self.model_status_of(l).is_broken()).map(|l| (self.model_link_name(l), self.model_status_of(l).reason())).collect();
                let line = model::open_line(&base, &broken);
                self.project.last_summary = line.clone();
                self.status_message = line;
            }
        }
    }

    /// A key check came back.
    pub(crate) fn apply_model_check(&mut self, r: ModelCheckResult) {
        let ident = (r.link.data_tab.clone(), r.link.lookup_tab.clone());
        if !self.project.model.all_links().iter().any(|l| (&l.data_tab, &l.lookup_tab) == (&ident.0, &ident.1)) {
            return;
        }
        let (status, check) = match r.check {
            Ok(c) => (c.status(), Some(c)),
            Err(m) => (LinkStatus::Failed(m), None),
        };
        self.project.model_ui.runtime.insert(ident, LinkRuntime { status, check, fingerprint: Some(r.fingerprint), in_flight: false });
    }

    pub(crate) fn model_check_failed(&mut self, msg: &str) {
        // the job was cancelled or lost: every link in flight is sent again
        for rt in self.project.model_ui.runtime.values_mut() {
            if rt.in_flight {
                rt.in_flight = false;
                rt.fingerprint = None;
                rt.status = LinkStatus::Failed(msg.to_string());
            }
        }
    }

    /// A list of rows (duplicates, unmatched) opens as a tab.
    pub(crate) fn apply_model_rows(&mut self, r: ModelRowsResult) {
        let rows = r.rows;
        self.create_tab_from_source(r.title.clone(), r.source, Vec::new(), HashMap::new());
        self.status_message = format!("Opening '{}' ({} rows).", r.title, format_with_commas(rows as f64, 0));
    }

    fn model_show_rows(&mut self, li: usize, list: ModelList) {
        let Some(l) = self.project.model.links.get(li).cloned() else { return };
        let (Some(d), Some(k)) = (self.model_worker_tab(&l.data_tab), self.model_worker_tab(&l.lookup_tab)) else {
            self.status_message = "Both tabs of the link have to be open to list the rows.".to_string();
            return;
        };
        self.project.model_ui.file_stamp += 1;
        let req = ModelRowsRequest { list, link: l, data: d, lookup: k, temp_dir: self.paths.temp_dir.clone(), file_stamp: self.project.model_ui.file_stamp };
        self.jobs.submit(JobKey::unique(JobKind::ModelRows), "Listing the rows", false, move |ctx| compute_model_rows(req, ctx).map(|r| JobOutput::ModelRows(Box::new(r))));
    }

    // ---------------------------------------------------------------------------- the menu

    #[allow(dead_code)]
    fn open_model_dialog(&mut self, which: ActiveToolModal) {
        if !self.project.active {
            self.active_tool = ActiveToolModal::ModelNoProject;
            return;
        }
        self.project.model_ui.draft = None;
        self.active_tool = which;
    }

    /// The dialog that says the model needs a project.
    pub(crate) fn render_model_no_project_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = ModelRects::default();
        let (mut save, mut close) = (false, false);
        theme::modal(MODEL_TITLE).default_width(theme::DIALOG_W_PIVOT_CONVERT).show(ctx, |d| {
            d.body(|ui| {
                let m = ui.add(egui::Label::new(egui::RichText::new(model::NO_PROJECT_MESSAGE).color(tk.ink)).wrap());
                rects.message = Some(m.rect);
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let b = theme::primary_button(ui, "Save project as\u{2026}");
                    rects.buttons.push(("Save project as\u{2026}".to_string(), b.rect));
                    save = b.clicked();
                    let c = ui.button("Close");
                    rects.buttons.push(("Close".to_string(), c.rect));
                    close = c.clicked();
                });
            });
        });
        self.project.model_ui.rects = rects;
        if save {
            self.active_tool = ActiveToolModal::None;
            self.project_save_as(None);
        } else if close {
            self.active_tool = ActiveToolModal::None;
        }
        ModalOutcome { close: false, step: None }
    }

    // ---------------------------------------------------------------------------- Links…

    fn model_draft_link(&self, d: &LinkDraft) -> Option<(Link, TabInfo, TabInfo)> {
        let (di, li) = (self.model_tab_info(d.data_tab?)?, self.model_tab_info(d.lookup_tab?)?);
        let keys = d.pairs.iter().map(|(a, b)| KeyPair { data: a.clone(), lookup: b.clone() }).collect();
        let unmatched_value = d.editing.and_then(|i| self.project.model.links.get(i)).and_then(|l| l.unmatched_value.clone());
        Some((Link { data_tab: di.key.clone(), lookup_tab: li.key.clone(), keys, unmatched_value }, di, li))
    }

    /// OK of the form: checks the link and, when it is fine, makes or replaces it.
    fn model_commit_draft(&mut self, mut d: LinkDraft) -> LinkDraft {
        if d.plan {
            return self.model_commit_plan(d);
        }
        let Some((mut link, di, li)) = self.model_draft_link(&d) else {
            d.error = "Pick the data tab and the lookup tab.".to_string();
            return d;
        };
        if let Err(e) = model::validate_link(&self.project.model, &link, d.editing, &di, &li) {
            d.error = format!("{:#}", e);
            return d;
        }
        link.data_tab = self.model_ensure_key(d.data_tab.unwrap_or_default());
        link.lookup_tab = self.model_ensure_key(d.lookup_tab.unwrap_or_default());
        match d.editing {
            Some(i) if i < self.project.model.links.len() => {
                let old = self.project.model.links[i].clone();
                self.project.model_ui.runtime.remove(&(old.data_tab, old.lookup_tab));
                self.project.model.links[i] = link;
            }
            _ => {
                self.project.model.links.push(link);
                self.project.model_ui.selected = Some(self.project.model.links.len() - 1);
            }
        }
        d.error.clear();
        d.editing = None;
        d.data_tab = None;
        d
    }

    /// OK of the budget and target form: the plan table is read now, so a repeated month and key is refused here.
    fn model_commit_plan(&mut self, mut d: LinkDraft) -> LinkDraft {
        let (Some(di), Some(pi)) = (d.data_tab.and_then(|i| self.model_tab_info(i)), d.lookup_tab.and_then(|i| self.model_tab_info(i))) else {
            d.error = "Pick the data tab and the plan table.".to_string();
            return d;
        };
        if di.key == pi.key || d.data_tab == d.lookup_tab {
            d.error = "The plan table must be a different tab from the data tab.".to_string();
            return d;
        }
        if d.month_column.is_empty() {
            d.error = "Pick the plan table's month column.".to_string();
            return d;
        }
        if d.pairs.iter().any(|(a, b)| a.is_empty() || b.is_empty()) {
            d.error = "Pick both columns of every key pair (for example Product on both tabs).".to_string();
            return d;
        }
        let (data_key, plan_key) = (self.model_ensure_key(d.data_tab.unwrap_or_default()), self.model_ensure_key(d.lookup_tab.unwrap_or_default()));
        let _ = (&di, &pi);
        let link = model::PlanLink { data_tab: data_key, plan_tab: plan_key, month_column: d.month_column.clone(), keys: d.pairs.iter().map(|(a, b)| KeyPair { data: a.clone(), lookup: b.clone() }).collect() };
        if self.project.model.plans.iter().enumerate().any(|(i, p)| Some(i) != d.editing && p.data_tab == link.data_tab && p.plan_tab == link.plan_tab) {
            d.error = "This plan table is already linked to that data tab. Edit the existing link instead.".to_string();
            return d;
        }
        let Some(table) = self.model_worker_tab(&link.plan_tab) else {
            d.error = "The plan table is not open.".to_string();
            return d;
        };
        match model::check_plan(&link, &table, &self.paths.temp_dir) {
            Err(e) => {
                d.error = format!("{:#}", e);
                return d;
            }
            Ok(c) => {
                if let Some(why) = c.refusal(&table.title) {
                    d.error = why;
                    return d;
                }
            }
        }
        match d.editing {
            Some(i) if i < self.project.model.plans.len() => self.project.model.plans[i] = link,
            _ => {
                self.project.model.plans.push(link);
                self.project.model_ui.plan_selected = Some(self.project.model.plans.len() - 1);
            }
        }
        d.error.clear();
        d.editing = None;
        d.data_tab = None;
        d
    }

    fn model_start_plan_draft(&mut self, editing: Option<usize>) {
        let mut d = LinkDraft { plan: true, ..Default::default() };
        if let Some(p) = editing.and_then(|i| self.project.model.plans.get(i)).cloned() {
            d.editing = editing;
            d.data_tab = self.model_tab_id(&p.data_tab);
            d.lookup_tab = self.model_tab_id(&p.plan_tab);
            d.month_column = p.month_column.clone();
            d.pairs = p.keys.iter().map(|k| (k.data.clone(), k.lookup.clone())).collect();
            if d.data_tab.is_none() || d.lookup_tab.is_none() {
                d.error = "Both tabs of the plan link have to be open to edit it.".to_string();
            }
        } else {
            d.pairs = vec![(String::new(), String::new())];
        }
        self.project.model_ui.draft = Some(d);
    }

    fn model_start_draft(&mut self, editing: Option<usize>) {
        let mut d = LinkDraft::default();
        if let Some(l) = editing.and_then(|i| self.project.model.links.get(i)).cloned() {
            d.editing = editing;
            d.data_tab = self.model_tab_id(&l.data_tab);
            d.lookup_tab = self.model_tab_id(&l.lookup_tab);
            d.pairs = l.keys.iter().map(|k| (k.data.clone(), k.lookup.clone())).collect();
            if d.data_tab.is_none() || d.lookup_tab.is_none() {
                d.error = "Both tabs of the link have to be open to edit it.".to_string();
            }
        } else {
            d.pairs = vec![(String::new(), String::new())];
        }
        self.project.model_ui.draft = Some(d);
    }

    pub(crate) fn render_model_links_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = ModelRects::default();
        let mut close = false;
        let mut start: Option<Option<usize>> = None;
        let mut remove = false;
        let mut select: Option<usize> = None;
        let mut draft = self.project.model_ui.draft.take();
        let mut commit = false;
        let mut cancel_draft = false;
        let mut plan_select: Option<usize> = None;
        let mut start_plan: Option<Option<usize>> = None;
        let mut remove_plan = false;
        let links = self.project.model.links.clone();
        let selected = self.project.model_ui.selected.filter(|i| *i < links.len());
        let plan_sel = self.project.model_ui.plan_selected.filter(|i| *i < self.project.model.plans.len());
        let tabs: Vec<(u64, TabInfo)> = self.tabs.iter().filter_map(|t| self.model_tab_info(t.id).map(|i| (t.id, i))).collect();
        let title = format!("{}  {}", theme::ph::LINK, LINKS_TITLE);
        let _ = &title;
        theme::modal(crate::ui::model_flow::MODEL_SETUP_TITLE).default_width(theme::DIALOG_W_MODEL_FLOW).show(ctx, |d| {
            d.body(|ui| {
                self.model_flow_header(ui, micro_parquet::workspace::ModelStep::Links);
                if let Some(dr) = draft.as_mut() {
                    self.model_draft_form(ui, dr, &tabs, &mut rects, &tk);
                    return;
                }
                ui.add(egui::Label::new(egui::RichText::new("A link joins a data tab to a lookup tab on 1 to 3 key columns. Keys must be unique in the lookup; the Key check step lists any that are not.").color(tk.muted)).wrap());
                ui.add_space(theme::SPACE_S);
                if links.is_empty() {
                    ui.label(egui::RichText::new("No links yet. Add one to use a lookup tab's columns in a pivot or chart.").color(tk.muted));
                }
                let plans = self.project.model.plans.clone();
                if !links.is_empty() {
                    egui::ScrollArea::both().id_salt("model_links").max_height(theme::LIST_MAX_H_L).auto_shrink([false, true]).show(ui, |ui| {
                        egui::Grid::new("model_links_grid").striped(true).num_columns(5).spacing([theme::SPACE_M, theme::SPACE_XS]).show(ui, |ui| {
                            for h in ["Data tab", "Key columns", "Lookup tab", "Key columns", "Status"] {
                                ui.label(theme::bold(h).color(tk.ink));
                            }
                            ui.end_row();
                            for (i, l) in links.iter().enumerate() {
                                let r = ui.selectable_label(selected == Some(i), self.model_tab_title(&l.data_tab));
                                rects.rows.push((i, r.rect));
                                if r.clicked() {
                                    select = Some(i);
                                }
                                ui.label(model::key_columns_text(l, true));
                                ui.label(self.model_tab_title(&l.lookup_tab));
                                ui.label(model::key_columns_text(l, false));
                                let st = self.model_status_of(l);
                                let colour = if st.is_broken() { tk.danger } else if matches!(st, LinkStatus::OkUnmatched(_)) { tk.warning } else if st == LinkStatus::Ok { tk.success } else { tk.muted };
                                let s = ui.add(egui::Label::new(theme::semibold(st.text()).color(colour)).wrap());
                                rects.statuses.push((i, s.rect));
                                ui.end_row();
                            }
                        });
                    });
                }
                // Phase 11: the budget and target tables
                ui.add_space(theme::SPACE_M);
                ui.separator();
                ui.label(egui::RichText::new("BUDGET AND TARGET TABLES").size(theme::TEXT_LABEL).color(tk.muted).strong());
                ui.add(egui::Label::new(egui::RichText::new("A plan table has one row per month and key (for example Product), with an amount column such as Budget. Link it to a data tab, then add a budget / target measure in Model \u{25b8} Measures to compare actuals with it in pivots, charts and KPI cards.").size(theme::TEXT_LABEL).color(tk.muted)).wrap());
                if plans.is_empty() {
                    ui.label(egui::RichText::new("No budget or target table yet.").color(tk.muted));
                }
                for (i, p) in plans.iter().enumerate() {
                    let keys: Vec<String> = p.keys.iter().map(|k| if k.data == k.lookup { k.data.clone() } else { format!("{} = {}", k.data, k.lookup) }).collect();
                    let text = format!("{} \u{2190} {}  (month {}; keys {})", self.model_tab_title(&p.data_tab), self.model_tab_title(&p.plan_tab), p.month_column, keys.join(", "));
                    let r = ui.selectable_label(plan_sel == Some(i), text);
                    rects.buttons.push((format!("Plan {}", i + 1), r.rect));
                    if r.clicked() {
                        plan_select = Some(i);
                    }
                    let why = self.project.model_ui.scope.as_ref().and_then(|c| c.plans.iter().find(|x| x.link == *p)).and_then(|x| x.refusal.clone());
                    if let Some(w) = why {
                        ui.add(egui::Label::new(egui::RichText::new(w).size(theme::TEXT_LABEL).color(tk.danger)).wrap());
                    }
                }
                ui.horizontal_wrapped(|ui| {
                let ap = ui.button("Add budget table");
                rects.buttons.push(("Add budget table".to_string(), ap.rect));
                if ap.clicked() {
                    start_plan = Some(None);
                }
                let ep = ui.add_enabled(plan_sel.is_some(), egui::Button::new("Edit budget table"));
                rects.buttons.push(("Edit budget table".to_string(), ep.rect));
                if ep.clicked() {
                    start_plan = Some(plan_sel);
                }
                let rp = ui.add_enabled(plan_sel.is_some(), egui::Button::new("Remove budget table"));
                rects.buttons.push(("Remove budget table".to_string(), rp.rect));
                remove_plan = rp.clicked();
                });
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    if draft.is_some() {
                        let ok = theme::primary_button(ui, "OK");
                        rects.buttons.push(("OK".to_string(), ok.rect));
                        commit = ok.clicked();
                        let c = ui.button("Cancel");
                        rects.buttons.push(("Cancel".to_string(), c.rect));
                        cancel_draft = c.clicked();
                    } else {
                        let a = theme::primary_button(ui, "Add");
                        rects.buttons.push(("Add".to_string(), a.rect));
                        if a.clicked() {
                            start = Some(None);
                        }
                        let e = ui.add_enabled(selected.is_some(), egui::Button::new("Edit"));
                        rects.buttons.push(("Edit".to_string(), e.rect));
                        if e.clicked() {
                            start = Some(selected);
                        }
                        let r = ui.add_enabled(selected.is_some(), egui::Button::new("Remove"));
                        rects.buttons.push(("Remove".to_string(), r.rect));
                        remove = r.clicked();
                        let c = ui.button("Close");
                        rects.buttons.push(("Close".to_string(), c.rect));
                        close = c.clicked();
                        self.model_flow_footer(ui, micro_parquet::workspace::ModelStep::Links);
                    }
                });
            });
        });
        self.project.model_ui.rects = rects;
        if commit {
            if let Some(d) = draft.take() {
                let d = self.model_commit_draft(d);
                draft = if d.error.is_empty() { None } else { Some(d) };
            }
        }
        if cancel_draft {
            draft = None;
        }
        self.project.model_ui.draft = draft;
        if let Some(i) = select {
            self.project.model_ui.selected = Some(i);
        }
        if let Some(s) = start {
            self.model_start_draft(s);
        }
        if let Some(i) = plan_select {
            self.project.model_ui.plan_selected = Some(i);
        }
        if let Some(s) = start_plan {
            self.model_start_plan_draft(s);
        }
        if remove_plan {
            if let Some(i) = plan_sel {
                let p = self.project.model.plans[i].clone();
                // a measure that reads the plan can't work without it: say so instead of leaving it to fail in a pivot
                let users: Vec<String> = self.project.model.measures.iter().filter(|m| matches!(m, micro_parquet::measures::NamedMeasure::Plan { tab, plan_tab, .. } if *tab == p.data_tab && *plan_tab == p.plan_tab)).map(|m| m.name().to_string()).collect();
                if users.is_empty() {
                    self.project.model.plans.remove(i);
                    self.project.model_ui.plan_selected = None;
                } else {
                    self.status_message = format!("The budget table is used by {}. Remove those measures first.", users.join(", "));
                }
            }
        }
        if remove {
            if let Some(i) = selected {
                let l = self.project.model.links.remove(i);
                self.project.model_ui.runtime.remove(&(l.data_tab, l.lookup_tab));
                self.project.model_ui.selected = None;
            }
        }
        if close {
            self.project.model_ui.draft = None;
            self.active_tool = ActiveToolModal::None;
        }
        ModalOutcome { close: false, step: None }
    }

    /// The Add / Edit form: the two tabs and 1 to 3 pairs of key columns.
    fn model_draft_form(&self, ui: &mut egui::Ui, dr: &mut LinkDraft, tabs: &[(u64, TabInfo)], rects: &mut ModelRects, tk: &theme::Tokens) {
        ui.label(theme::bold(match (dr.plan, dr.editing.is_some()) { (true, true) => "Edit budget / target table", (true, false) => "Add budget / target table", (false, true) => "Edit link", (false, false) => "Add link" }).color(tk.ink));
        ui.add_space(theme::SPACE_S);
        let title_of = |id: Option<u64>| id.and_then(|i| tabs.iter().find(|t| t.0 == i)).map(|t| t.1.title.clone()).unwrap_or_else(|| "Pick a tab".to_string());
        let mut tab_combo = |ui: &mut egui::Ui, name: &str, current: &mut Option<u64>, other_is_data: bool| -> bool {
            let mut changed = false;
            let combo = egui::ComboBox::from_id_salt(name).width(theme::MODEL_COMBO_W).selected_text(title_of(*current)).show_ui(ui, |ui| {
                for (id, info) in tabs {
                    // a chart tab or one that did not open cannot be picked; a tab that is the other role is refused with its message at OK
                    let why = model::tab_refusal(info);
                    let item = ui.add_enabled(why.is_none(), egui::SelectableLabel::new(*current == Some(*id), info.title.clone()));
                    let item = match &why {
                        Some(w) => item.on_disabled_hover_text(w.clone()),
                        None => item,
                    };
                    if item.clicked() {
                        *current = Some(*id);
                        changed = true;
                    }
                }
            });
            let _ = other_is_data;
            rects.combos.push((name.to_string(), combo.response.rect));
            changed
        };
        let mut data_tab = dr.data_tab;
        let mut lookup_tab = dr.lookup_tab;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Data tab").color(tk.muted));
            if tab_combo(ui, "Data tab", &mut data_tab, true) {
                dr.pairs.iter_mut().for_each(|p| p.0.clear());
            }
            ui.label(egui::RichText::new(if dr.plan { "\u{2192} Plan table" } else { "\u{2192} Lookup tab" }).color(tk.muted));
            if tab_combo(ui, "Lookup tab", &mut lookup_tab, false) {
                dr.pairs.iter_mut().for_each(|p| p.1.clear());
            }
        });
        dr.data_tab = data_tab;
        dr.lookup_tab = lookup_tab;
        let cols_of = |id: Option<u64>| -> Vec<String> { id.and_then(|i| tabs.iter().find(|t| t.0 == i)).map(|t| t.1.columns.iter().map(|c| c.0.clone()).collect()).unwrap_or_default() };
        let (dc, lc) = (cols_of(dr.data_tab), cols_of(dr.lookup_tab));
        if dr.plan {
            ui.add_space(theme::SPACE_S);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Month column of the plan table").color(tk.muted));
                let combo = egui::ComboBox::from_id_salt("Plan month column").width(theme::MODEL_COMBO_W).selected_text(if dr.month_column.is_empty() { "Pick a column".to_string() } else { dr.month_column.clone() }).show_ui(ui, |ui| {
                    for c in &lc {
                        if ui.selectable_label(&dr.month_column == c, c).clicked() {
                            dr.month_column = c.clone();
                        }
                    }
                });
                rects.combos.push(("Plan month column".to_string(), combo.response.rect));
            });
        }
        ui.add_space(theme::SPACE_S);
        ui.label(egui::RichText::new(if dr.plan { "Key columns that name a line of the plan (the data column on the left, the plan table's on the right), for example Product. The plan can then be split by these only." } else { "Key columns (the data column on the left, the lookup's on the right). Keys match as the Join tool matches them." }).size(theme::TEXT_LABEL).color(tk.muted));
        let mut remove: Option<usize> = None;
        for i in 0..dr.pairs.len() {
            ui.horizontal(|ui| {
                let pick = |ui: &mut egui::Ui, name: String, cols: &[String], value: &mut String, rects: &mut ModelRects| {
                    let combo = egui::ComboBox::from_id_salt(name.clone()).width(theme::MODEL_COMBO_W).selected_text(if value.is_empty() { "Pick a column".to_string() } else { value.clone() }).show_ui(ui, |ui| {
                        for c in cols {
                            if ui.selectable_label(value == c, c).clicked() {
                                *value = c.clone();
                            }
                        }
                    });
                    rects.combos.push((name, combo.response.rect));
                };
                pick(ui, format!("Data key {}", i + 1), &dc, &mut dr.pairs[i].0, rects);
                ui.label("\u{2194}");
                pick(ui, format!("Lookup key {}", i + 1), &lc, &mut dr.pairs[i].1, rects);
                if dr.pairs.len() > 1 {
                    let x = ui.small_button(theme::ph::X).on_hover_text("Remove this key pair");
                    rects.buttons.push((format!("Remove key {}", i + 1), x.rect));
                    if x.clicked() {
                        remove = Some(i);
                    }
                }
            });
        }
        if let Some(i) = remove {
            dr.pairs.remove(i);
        }
        if dr.pairs.len() < model::MAX_KEY_PAIRS {
            let b = ui.button("Add key column");
            rects.buttons.push(("Add key column".to_string(), b.rect));
            if b.clicked() {
                dr.pairs.push((String::new(), String::new()));
            }
        }
        if !dr.error.is_empty() {
            ui.add_space(theme::SPACE_S);
            let e = ui.add(egui::Label::new(egui::RichText::new(&dr.error).color(tk.danger)).wrap());
            rects.error = Some(e.rect);
        }
    }

    // ---------------------------------------------------------------------------- Key check…

    pub(crate) fn render_model_key_check_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = ModelRects::default();
        let mut close = false;
        let mut show: Option<(usize, ModelList)> = None;
        let mut copy: Option<usize> = None;
        let mut set_value: Option<(usize, Option<String>)> = None;
        let links = self.project.model.links.clone();
        let title = format!("{}  {}", theme::ph::LINK, KEY_CHECK_TITLE);
        let values: Vec<Vec<String>> = links
            .iter()
            .map(|l| {
                self.model_tab_id(&l.data_tab)
                    .and_then(|id| self.tabs.iter().find(|t| t.id == id))
                    .map(|t| t.cached_slice.get_columns().iter().filter(|c| c.dtype().is_numeric()).map(|c| c.name().to_string()).collect())
                    .unwrap_or_default()
            })
            .collect();
        let _ = &title;
        theme::modal(crate::ui::model_flow::MODEL_SETUP_TITLE).default_width(theme::DIALOG_W_MODEL_FLOW).show(ctx, |d| {
            d.body(|ui| {
                self.model_flow_header(ui, micro_parquet::workspace::ModelStep::KeyCheck);
                if links.is_empty() {
                    ui.label(egui::RichText::new("The model has no links yet. Go back to the Links step to make one.").color(tk.muted));
                }
                for (i, l) in links.iter().enumerate() {
                    let rt = self.project.model_ui.runtime.get(&(l.data_tab.clone(), l.lookup_tab.clone())).cloned();
                    let status = rt.as_ref().map(|r| r.status.clone()).unwrap_or(LinkStatus::Checking);
                    let colour = if status.is_broken() { tk.danger } else if matches!(status, LinkStatus::OkUnmatched(_)) { tk.warning } else if status == LinkStatus::Ok { tk.success } else { tk.muted };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(theme::bold(self.model_link_name(l)).color(tk.ink));
                        let s = ui.add(egui::Label::new(theme::semibold(status.text()).color(colour)).wrap());
                        rects.statuses.push((i, s.rect));
                    });
                    ui.add(egui::Label::new(egui::RichText::new(format!("Keys: {}", model::pairs_text(l))).size(theme::TEXT_LABEL).color(tk.muted)).wrap());
                    if let Some(c) = rt.as_ref().and_then(|r| r.check.as_ref()) {
                        let n = |v: usize| format_with_commas(v as f64, 0);
                        ui.add(egui::Label::new(egui::RichText::new(format!("Lookup: {} rows \u{b7} {} duplicate keys ({} rows) \u{b7} {} blank keys", n(c.lookup_rows), n(c.duplicate_keys), n(c.duplicate_rows), n(c.lookup_blank_keys))).color(tk.ink)).wrap());
                        ui.add(egui::Label::new(egui::RichText::new(format!("Data: {} rows \u{b7} {} matched \u{b7} {} unmatched keys ({} rows) \u{b7} {} blank keys", n(c.data_rows), n(c.matched_rows), n(c.unmatched.len()), n(c.unmatched_rows), n(c.data_blank_keys))).color(tk.ink)).wrap());
                        ui.add(egui::Label::new(egui::RichText::new(format!("{} lookup rows are used by no data row (for information).", n(c.unused_lookup_rows))).size(theme::TEXT_LABEL).color(tk.muted)).wrap());
                        for (key, rows) in c.duplicate_examples.iter().take(theme::MODEL_LIST_LINES) {
                            ui.label(egui::RichText::new(format!("Duplicate: {} \u{2014} {} rows", key.join(" / "), rows)).size(theme::TEXT_LABEL).color(tk.danger));
                        }
                        if let Some(note) = &c.value_note {
                            ui.add(egui::Label::new(egui::RichText::new(note).size(theme::TEXT_LABEL).color(tk.warning)).wrap());
                        }
                        for u in c.unmatched.iter().take(theme::MODEL_LIST_LINES) {
                            let sum = match (&c.value_column, u.sum) {
                                (Some(_), Some(s)) => format!(" \u{b7} sum {}", format_with_commas(s, 2)),
                                _ => String::new(),
                            };
                            ui.label(egui::RichText::new(format!("No match: {} \u{2014} {}{}", u.key.join(" / "), format_with_commas(u.rows as f64, 0) + if u.rows == 1 { " row" } else { " rows" }, sum)).size(theme::TEXT_LABEL).color(tk.warning));
                        }
                        if c.unmatched.len() > theme::MODEL_LIST_LINES {
                            ui.label(egui::RichText::new(format!("and {} more unmatched keys (Copy list has them all)", c.unmatched.len() - theme::MODEL_LIST_LINES)).size(theme::TEXT_LABEL).color(tk.muted));
                        }
                    } else if let LinkStatus::Failed(m) = &status {
                        ui.add(egui::Label::new(egui::RichText::new(format!("{}.", LinkStatus::cant_check(m))).color(tk.danger)).wrap());
                    }
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Sum of").color(tk.muted));
                        let current = l.unmatched_value.clone().unwrap_or_default();
                        let combo = egui::ComboBox::from_id_salt(("model_value", i)).width(theme::MODEL_COMBO_W).selected_text(if current.is_empty() { "No sums".to_string() } else { current.clone() }).show_ui(ui, |ui| {
                            if ui.selectable_label(current.is_empty(), "No sums").clicked() {
                                set_value = Some((i, None));
                            }
                            for v in &values[i] {
                                if ui.selectable_label(&current == v, v).clicked() {
                                    set_value = Some((i, Some(v.clone())));
                                }
                            }
                        });
                        rects.section_buttons.push((i, "Sum of".to_string(), combo.response.rect));
                        ui.label(egui::RichText::new("for each unmatched key").color(tk.muted));
                    });
                    let c = rt.as_ref().and_then(|r| r.check.as_ref());
                    ui.horizontal_wrapped(|ui| {
                        let dup = c.is_some_and(|c| c.duplicate_keys > 0);
                        let b = ui.add_enabled(dup, egui::Button::new("Show duplicates"));
                        let b = if dup { b.on_hover_text("A new tab of the lookup rows whose key repeats, sorted by key, with the number of times") } else { b.on_disabled_hover_text("No duplicate keys") };
                        rects.section_buttons.push((i, "Show duplicates".to_string(), b.rect));
                        if b.clicked() {
                            show = Some((i, ModelList::Duplicates));
                        }
                        let un = c.is_some_and(|c| c.unmatched_rows > 0);
                        let b = ui.add_enabled(un, egui::Button::new("Show unmatched rows"));
                        let b = if un { b.on_hover_text("A new tab of the data rows whose key has no row in the lookup") } else { b.on_disabled_hover_text("Every data key has a lookup row") };
                        rects.section_buttons.push((i, "Show unmatched rows".to_string(), b.rect));
                        if b.clicked() {
                            show = Some((i, ModelList::Unmatched));
                        }
                        let b = ui.add_enabled(un, egui::Button::new("Copy list"));
                        let b = if un { b.on_hover_text("The unmatched keys with their rows and sums, as text to paste") } else { b.on_disabled_hover_text("No unmatched keys") };
                        rects.section_buttons.push((i, "Copy list".to_string(), b.rect));
                        if b.clicked() {
                            copy = Some(i);
                        }
                    });
                    ui.add_space(theme::SPACE_XS);
                    ui.separator();
                }
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let c = ui.button("Close");
                    rects.buttons.push(("Close".to_string(), c.rect));
                    close = c.clicked();
                    self.model_flow_footer(ui, micro_parquet::workspace::ModelStep::KeyCheck);
                });
            });
        });
        self.project.model_ui.rects = rects;
        if let Some((i, v)) = set_value {
            if let Some(l) = self.project.model.links.get_mut(i) {
                l.unmatched_value = v;
            }
        }
        if let Some((i, list)) = show {
            self.model_show_rows(i, list);
        }
        if let Some(i) = copy {
            if let Some(l) = self.project.model.links.get(i).cloned() {
                if let Some(c) = self.project.model_ui.runtime.get(&(l.data_tab.clone(), l.lookup_tab.clone())).and_then(|r| r.check.clone()) {
                    let text = model::unmatched_list_text(&l, &c);
                    ctx.copy_text(text.clone());
                    self.project.model_ui.last_copied = text;
                    self.status_message = format!("Copied {} unmatched keys.", format_with_commas(c.unmatched.len() as f64, 0));
                }
            }
        }
        if close {
            self.active_tool = ActiveToolModal::None;
        }
        ModalOutcome { close: false, step: None }
    }
}
