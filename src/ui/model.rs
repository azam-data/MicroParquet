//! The Model menu (Phase 9 M0): Links…, Key check…, the checks that run when a link is made, a project opens or a tab changes, the
//! open line and the status bar's "Model: 4 links · 1 broken".

use crate::ui::prelude::*;
use micro_parquet::jobs::{JobKey, JobKind};
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
    pub rects: ModelRects,
    /// The open line waiting for the key checks: "9 tabs opened."
    pub open_base: Option<String>,
    pub file_stamp: u64,
    /// What Copy list put on the clipboard last (tests read it).
    pub last_copied: String,
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
    fn model_ensure_key(&mut self, id: u64) -> String {
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

    fn model_tab_info(&self, id: u64) -> Option<TabInfo> {
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

    fn model_worker_tab(&self, key: &str) -> Option<ModelTab> {
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
            None => format!("{} (not open)", key),
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

    // ---------------------------------------------------------------------------- the checks

    fn model_link_fingerprint(&self, l: &Link, d: &ModelTab, k: &ModelTab) -> u64 {
        fingerprint(&(micro_parquet::steps_edit::pipeline_hash(&d.source, &d.steps), micro_parquet::steps_edit::pipeline_hash(&k.source, &k.steps), format!("{:?}", l)))
    }

    /// Called every frame: sends a link to the worker when it was never checked or when either tab or the link changed since, and
    /// writes the open line once every check is back.
    pub(crate) fn model_tick(&mut self) {
        if !self.project.active || self.project.opening.is_some() {
            return;
        }
        let links = self.project.model.links.clone();
        self.project.model_ui.runtime.retain(|k, _| links.iter().any(|l| (&l.data_tab, &l.lookup_tab) == (&k.0, &k.1)));
        for l in &links {
            let ident = (l.data_tab.clone(), l.lookup_tab.clone());
            let (d, k) = (self.model_worker_tab(&l.data_tab), self.model_worker_tab(&l.lookup_tab));
            let (Some(d), Some(k)) = (d.clone(), k) else {
                let which = if d.is_none() { &l.data_tab } else { &l.lookup_tab };
                let why = format!("'{}' is not open as a table", self.model_tab_title(which).trim_end_matches(" (not open)"));
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
        if !self.project.model.links.iter().any(|l| (&l.data_tab, &l.lookup_tab) == (&ident.0, &ident.1)) {
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

    pub(crate) fn model_menu(&mut self, ui: &mut egui::Ui) -> egui::Response {
        ui.menu_button(MODEL_TITLE, |ui| {
            if ui.button("Links\u{2026}").clicked() {
                ui.close_menu();
                self.open_model_dialog(ActiveToolModal::ModelLinks);
            }
            if ui.button("Key check\u{2026}").clicked() {
                ui.close_menu();
                self.open_model_dialog(ActiveToolModal::ModelKeyCheck);
            }
        })
        .response
    }

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
        let links = self.project.model.links.clone();
        let selected = self.project.model_ui.selected.filter(|i| *i < links.len());
        let tabs: Vec<(u64, TabInfo)> = self.tabs.iter().filter_map(|t| self.model_tab_info(t.id).map(|i| (t.id, i))).collect();
        let title = format!("{}  {}", theme::ph::LINK, LINKS_TITLE);
        theme::modal(&title).default_width(theme::DIALOG_W_PIVOT_BUILDER * theme::MODEL_DIALOG_SHARE).show(ctx, |d| {
            d.body(|ui| {
                if let Some(dr) = draft.as_mut() {
                    self.model_draft_form(ui, dr, &tabs, &mut rects, &tk);
                    return;
                }
                ui.add(egui::Label::new(egui::RichText::new("A link joins a data tab to a lookup tab on 1 to 3 key columns. Keys must be unique in the lookup; Model \u{25b8} Key check lists any that are not.").color(tk.muted)).wrap());
                ui.add_space(theme::SPACE_S);
                if links.is_empty() {
                    ui.label(egui::RichText::new("No links yet. Add one to use a lookup tab's columns in a pivot or chart.").color(tk.muted));
                }
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
        ui.label(theme::bold(if dr.editing.is_some() { "Edit link" } else { "Add link" }).color(tk.ink));
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
            ui.label(egui::RichText::new("\u{2192} Lookup tab").color(tk.muted));
            if tab_combo(ui, "Lookup tab", &mut lookup_tab, false) {
                dr.pairs.iter_mut().for_each(|p| p.1.clear());
            }
        });
        dr.data_tab = data_tab;
        dr.lookup_tab = lookup_tab;
        let cols_of = |id: Option<u64>| -> Vec<String> { id.and_then(|i| tabs.iter().find(|t| t.0 == i)).map(|t| t.1.columns.iter().map(|c| c.0.clone()).collect()).unwrap_or_default() };
        let (dc, lc) = (cols_of(dr.data_tab), cols_of(dr.lookup_tab));
        ui.add_space(theme::SPACE_S);
        ui.label(egui::RichText::new("Key columns (the data column on the left, the lookup's on the right). Keys match as the Join tool matches them.").size(theme::TEXT_LABEL).color(tk.muted));
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
        theme::modal(&title).default_width(theme::DIALOG_W_PIVOT_BUILDER * theme::MODEL_DIALOG_SHARE).show(ctx, |d| {
            d.body(|ui| {
                if links.is_empty() {
                    ui.label(egui::RichText::new("The model has no links yet. Model \u{25b8} Links\u{2026} makes one.").color(tk.muted));
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
                        ui.add(egui::Label::new(egui::RichText::new(format!("The keys could not be checked: {}.", m.trim_end_matches('.'))).color(tk.danger)).wrap());
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
