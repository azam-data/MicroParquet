//! The dashboard view (Phase 10 D1): pages on the left, the page in the centre, the properties of the page or of the selected item on
//! the right. An item points at a chart tab or a pivot tab and draws what that tab's own code gives (`dashboard::compute_item`, on the
//! worker); a page filter is a dropdown of a lookup, Calendar or data field of one data tab. The window never waits: a number that is
//! being calculated shows "Calculating…".

use crate::ui::prelude::*;
use micro_parquet::charts::spec::ChartSpec;
use micro_parquet::dashboard::*;
use micro_parquet::kpi::*;
use micro_parquet::model::ModelFieldRef;
use micro_parquet::models::*;
use micro_parquet::tab_jobs::{dash_item_job, dash_values_job, source_signature};
use std::collections::HashMap;

/// What "Dashboard" says outside a project.
pub const NO_PROJECT: &str = "Dashboards belong to a project. Save or open a project first.";
pub const DASH_BUTTON: &str = "Dashboard";

/// A design being made: the picture, the slots drawn so far.
pub struct TplEdit {
    pub name: String,
    pub design: micro_parquet::templates::Design,
    pub texture: egui::TextureHandle,
    pub slots: Vec<Slot>,
    /// The rectangle being drawn, in screen points: where the press was and where the pointer is.
    pub drawing: Option<(egui::Pos2, egui::Pos2)>,
    pub error: String,
}

impl std::fmt::Debug for TplEdit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TplEdit({})", self.name)
    }
}

/// A drag of an item (or one of its handles) on a Free page.
#[derive(Clone, Debug)]
pub struct DragState {
    pub item: String,
    pub handle: Handle,
    /// The item's rect when the drag started.
    pub start: DashRect,
    /// How far the pointer has gone since, in screen points.
    pub total: egui::Vec2,
    /// The last place that was allowed.
    pub last_ok: DashRect,
}

/// What a click on the page asked for; the page state is changed after the page is drawn.
pub enum PageAct {
    Cross(CrossPick),
    DrillDown(String, String),
    DrillBack(String, usize),
    Notice(String),
    /// A drag, a resize or an arrow key gave the item this place on a Free page.
    SetRect(String, DashRect),
    /// A click on an empty slot: its fill choices open.
    OpenSlot(String),
    /// An item chosen on the empty page's cards.
    Add(DashItemKind),
}

/// One item's latest numbers.
#[derive(Default, Clone)]
pub struct ItemView {
    pub sig: String,
    /// The signature a calculation on the worker is for.
    pub pending: Option<String>,
    pub data: Option<ItemData>,
    pub error: Option<String>,
    /// The chart tab's scale (divisor, decimals, unit).
    pub scale: Option<(f64, usize, String)>,
}

#[derive(Default, Clone)]
pub struct FilterValues {
    pub sig: String,
    pub pending: Option<String>,
    pub values: Vec<String>,
    pub truncated: bool,
    pub error: Option<String>,
}

/// The "Add filter" row.
#[derive(Default, Clone)]
pub struct AddFilter {
    pub tab: Option<u64>,
    pub field: Option<(String, Option<ModelFieldRef>)>,
    pub label: String,
    pub multi: bool,
}

#[derive(Default)]
pub struct DashboardUi {
    pub open: bool,
    pub page: usize,
    pub selected: Option<String>,
    pub items: HashMap<String, ItemView>,
    pub values: HashMap<String, FilterValues>,
    pub add_filter: Option<AddFilter>,
    pub confirm_delete: bool,
    /// The layout the page was asked to switch to, waiting for the dialog's answer: `true` is Free.
    pub pending_layout: Option<bool>,
    /// The drag in progress on a Free page.
    pub drag: Option<DragState>,
    /// The design editor (D5): a picture just imported, its slots being drawn.
    pub tpl: Option<TplEdit>,
    /// The slot being filled: its name; the choices show under the header.
    pub fill_slot: Option<String>,
    /// The background pictures of the pages, by blake3, and what went wrong with a design, by name.
    pub textures: HashMap<String, egui::TextureHandle>,
    pub design_notes: HashMap<String, String>,
    /// A file the owner picked to import (the Import button sets it; the next frame reads it).
    pub import_path: Option<std::path::PathBuf>,
    /// A refusal or a note in plain words, shown under the header.
    pub notice: String,
    pub next_id: u64,
    /// The controls drawn in the last frame, by name (the render tests click these rects).
    pub rects: Vec<(String, egui::Rect)>,
    /// The Dashboard entry of the tab strip, as drawn in the last frame.
    pub strip_rect: Option<egui::Rect>,
    /// The id of each worker job's scope, by item or filter id.
    pub scopes: HashMap<u64, String>,
    /// The Export dialog, when open.
    pub export: Option<crate::ui::dashboard_export::ExportDlg>,
}

fn scope_of(id: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h);
    h.finish()
}

impl DashboardUi {
    pub(crate) fn fresh_id(&mut self, prefix: &str, taken: &[String]) -> String {
        loop {
            self.next_id += 1;
            let id = format!("{}{}", prefix, self.next_id);
            if !taken.contains(&id) {
                return id;
            }
        }
    }
}

/// What an item is built from, found in the open tabs.
pub(crate) struct Resolved {
    pub(crate) source: ItemSource,
    /// The project key of the data tab the item reads (what page filters are matched against).
    pub(crate) data_tab: Option<String>,
    pub(crate) scale: Option<(f64, usize, String)>,
    pub(crate) title: String,
}

impl MicroParquetApp {
    /// The Dashboard command: opens the dashboard of the project, making it the first time.
    pub(crate) fn dashboard_command(&mut self) {
        if !self.project.active {
            self.status_message = NO_PROJECT.to_string();
            return;
        }
        if self.project.dashboard.is_none() {
            self.project.dashboard = Some(Dashboard { id: "d1".into(), title: "Dashboard".into(), pages: vec![DashPage { id: "p1".into(), title: "Page 1".into(), ..Default::default() }], ..Default::default() });
        }
        self.dashboard_ui.open = true;
    }

    fn dash_index_of_key(&self, key: &str) -> Option<usize> {
        let id = self.model_tab_id(key)?;
        self.tabs.iter().position(|t| t.id == id)
    }

    /// What the item reads, or why it can't be drawn.
    pub(crate) fn dash_resolve(&self, item: &DashItem) -> Result<Resolved, String> {
        match &item.kind {
            DashItemKind::Kpi(k) => {
                let key = match &k.source {
                    KpiSource::Measure(name) => self.project.model.measures.iter().find(|m| m.name() == name).map(|m| m.tab().to_string()).ok_or_else(|| if name.is_empty() { "Pick a measure in the card's settings.".to_string() } else { format!("The measure '{}' is not in the model. Pick another in the card's settings.", name) })?,
                    KpiSource::Value { tab, .. } => {
                        if tab.is_empty() {
                            return Err("Pick a data tab and a column in the card's settings.".to_string());
                        }
                        tab.clone()
                    }
                };
                let idx = self.dash_index_of_key(&key).ok_or_else(|| SOURCE_REMOVED.to_string())?;
                let t = &self.tabs[idx];
                let n = t.exec_steps.min(t.steps.len());
                Ok(Resolved { source: ItemSource::Kpi { spec: k.clone(), tab_key: key.clone(), base: t.source.clone(), steps: t.steps[..n].to_vec() }, data_tab: Some(key), scale: None, title: if k.title.is_empty() { "KPI".to_string() } else { k.title.clone() } })
            }
            DashItemKind::Chart(key) => {
                let idx = self.dash_index_of_key(key).ok_or_else(|| SOURCE_REMOVED.to_string())?;
                let tab = &self.tabs[idx];
                let info = self.chart_tabs.get(&tab.id).ok_or_else(|| format!("'{}' is not a chart tab.", tab.title))?;
                let sid = info.source_tab.ok_or_else(|| format!("The source of '{}' is closed. Open it again from the chart tab.", tab.title))?;
                let src = self.tabs.iter().find(|t| t.id == sid).ok_or_else(|| format!("The source of '{}' is closed. Open it again from the chart tab.", tab.title))?;
                let n = src.exec_steps.min(src.steps.len());
                let spec = info.next_spec.clone().unwrap_or_else(|| info.data.spec.clone());
                Ok(Resolved {
                    source: ItemSource::Chart { base: src.source.clone(), steps: src.steps[..n].to_vec(), spec },
                    data_tab: self.project.keys.get(&sid).cloned(),
                    scale: Some((info.divisor, info.decimals, info.unit.clone())),
                    title: tab.title.clone(),
                })
            }
            DashItemKind::Pivot(key) => {
                let idx = self.dash_index_of_key(key).ok_or_else(|| SOURCE_REMOVED.to_string())?;
                let tab = &self.tabs[idx];
                let n = tab.exec_steps.min(tab.steps.len());
                let at = tab.steps[..n].iter().rposition(|s| s.as_pivot_spec().is_some()).ok_or_else(|| format!("'{}' is not a pivot tab.", tab.title))?;
                let spec = tab.steps[at].as_pivot_spec().ok_or_else(|| format!("'{}' is not a pivot tab.", tab.title))?;
                let before = tab.steps[..at].to_vec();
                // the data tab: the pivot's own (a model pivot), else the tab whose table the pivot reads
                let data_tab = if !spec.data_tab.is_empty() {
                    Some(spec.data_tab.clone())
                } else {
                    let want = (format!("{:?}", tab.source), format!("{:?}", before));
                    self.tabs.iter().find(|t| t.id != tab.id && !self.chart_tabs.contains_key(&t.id) && (format!("{:?}", t.source), format!("{:?}", t.steps[..t.exec_steps.min(t.steps.len())].to_vec())) == want).and_then(|t| self.project.keys.get(&t.id).cloned())
                };
                Ok(Resolved { source: ItemSource::Pivot { base: tab.source.clone(), steps: before, spec }, data_tab, scale: None, title: tab.title.clone() })
            }
        }
    }

    /// The picks of a page that reach an item, and the labels of the filters that have a pick but don't reach it.
    pub(crate) fn dash_picks_for(&self, page: &DashPage, data_tab: Option<&str>, item_id: &str) -> (Vec<(PageFilter, Vec<String>)>, Vec<String>) {
        let (mut yes, mut no) = (Vec::new(), Vec::new());
        // the page filters, then the click (the item that was clicked is not filtered by its own click)
        let all = page.active_picks().into_iter().map(|(_, f, v)| (f.clone(), v.clone())).chain(page.cross_for(item_id));
        for (f, v) in all {
            if filter_applies(&f, data_tab) {
                yes.push((f, v));
            } else {
                no.push(f.label.clone());
            }
        }
        (yes, no)
    }

    /// What an item's numbers depend on: change it and they are calculated again. A card's unit, decimals, limits, direction and
    /// title are not in it (they change the display, not the stored number).
    pub(crate) fn dash_sig(&self, res: &Resolved, picks: &[(PageFilter, Vec<String>)]) -> String {
        match &res.source {
            ItemSource::Chart { base, steps, spec } => format!("{}|{:?}|{:?}", source_signature(base, steps), spec, picks),
            ItemSource::Pivot { base, steps, spec } => format!("{}|{:?}|{:?}", source_signature(base, steps), spec, picks),
            // a card also depends on the as-of date and the measures (the model)
            ItemSource::Kpi { spec, base, steps, .. } => format!("{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}", source_signature(base, steps), spec.source, spec.compare, spec.sparkline, picks, self.project.model.measures, self.project.model.as_of),
        }
    }

    /// `dash_sig` with the item's own drill path: what the worker's answer is checked against.
    pub(crate) fn dash_item_sig(&self, res: &Resolved, picks: &[(PageFilter, Vec<String>)], drill: &[String]) -> String {
        format!("{}|{:?}", self.dash_sig(res, picks), drill)
    }

    /// Called every frame: the items of the open page and the values of its filters are calculated again when what they read changed.
    pub(crate) fn dashboard_tick(&mut self) {
        if !self.dashboard_ui.open {
            return;
        }
        let Some(dash) = self.project.dashboard.clone() else { return };
        let Some(page) = dash.pages.get(self.dashboard_ui.page.min(dash.pages.len().saturating_sub(1))).cloned() else { return };
        for item in &page.items {
            let view = self.dashboard_ui.items.entry(item.id.clone()).or_default().clone();
            let Ok(res) = self.dash_resolve(item) else { continue };
            let (picks, _) = self.dash_picks_for(&page, res.data_tab.as_deref(), &item.id);
            let drill: Vec<String> = page.state.drill_of(&item.id).to_vec();
            let sig = self.dash_item_sig(&res, &picks, &drill);
            if view.sig == sig || view.pending.as_deref() == Some(sig.as_str()) {
                continue;
            }
            let req = ItemRequest { item_id: item.id.clone(), source: res.source, picks, drill, temp_dir: self.paths.temp_dir.clone() };
            if let Some(v) = self.dashboard_ui.items.get_mut(&item.id) {
                v.pending = Some(sig.clone());
                v.scale = res.scale;
            }
            let scope = scope_of(&item.id);
            self.dashboard_ui.scopes.insert(scope, item.id.clone());
            self.jobs.submit(JobKey::tab(JobKind::DashItem, scope), "Calculating the dashboard item", false, move |ctx| dash_item_job(req, sig, ctx));
        }
        for f in &page.filters {
            let Some(idx) = self.dash_index_of_key(&f.tab) else { continue };
            let t = &self.tabs[idx];
            let n = t.exec_steps.min(t.steps.len());
            let sig = format!("{}|{}|{:?}", source_signature(&t.source, &t.steps[..n]), f.field, f.model);
            let cur = self.dashboard_ui.values.entry(f.id.clone()).or_default().clone();
            if cur.sig == sig || cur.pending.as_deref() == Some(sig.as_str()) {
                continue;
            }
            if let Some(v) = self.dashboard_ui.values.get_mut(&f.id) {
                v.pending = Some(sig.clone());
            }
            let (base, steps, filter, temp) = (t.source.clone(), t.steps[..n].to_vec(), f.clone(), self.paths.temp_dir.clone());
            let scope = scope_of(&f.id);
            self.dashboard_ui.scopes.insert(scope, f.id.clone());
            self.jobs.submit(JobKey::tab(JobKind::DashValues, scope), "Reading the values of the filter", false, move |ctx| dash_values_job(base, steps, filter, sig, temp, ctx));
        }
    }

    pub(crate) fn apply_dash_item(&mut self, r: DashItemResult) {
        if let Some(v) = self.dashboard_ui.items.get_mut(&r.item_id) {
            if v.pending.as_deref() == Some(r.sig.as_str()) {
                v.pending = None;
                v.sig = r.sig;
                v.data = Some(r.data);
                v.error = None;
            }
        }
    }

    pub(crate) fn dash_item_failed(&mut self, scope: u64, msg: &str) {
        let Some(id) = self.dashboard_ui.scopes.get(&scope).cloned() else { return };
        if let Some(v) = self.dashboard_ui.items.get_mut(&id) {
            // not tried again until something it reads changes
            v.sig = v.pending.take().unwrap_or_default();
            v.error = Some(msg.to_string());
            v.data = None;
        }
    }

    pub(crate) fn apply_dash_values(&mut self, r: DashValuesResult) {
        if let Some(v) = self.dashboard_ui.values.get_mut(&r.filter_id) {
            if v.pending.as_deref() == Some(r.sig.as_str()) {
                v.pending = None;
                v.sig = r.sig;
                v.values = r.values;
                v.truncated = r.truncated;
                v.error = None;
            }
        }
    }

    pub(crate) fn dash_values_failed(&mut self, scope: u64, msg: &str) {
        let Some(id) = self.dashboard_ui.scopes.get(&scope).cloned() else { return };
        if let Some(v) = self.dashboard_ui.values.get_mut(&id) {
            v.sig = v.pending.take().unwrap_or_default();
            v.error = Some(msg.to_string());
        }
    }

    /// The tabs an item can point at: (project key, title, is a chart tab), in tab order.
    pub(crate) fn dash_source_choices(&mut self) -> Vec<(String, String, bool)> {
        let ids: Vec<(u64, String, bool, bool)> = self
            .tabs
            .iter()
            .filter(|t| !matches!(t.source, TabSource::Unavailable { .. }))
            .map(|t| {
                let n = t.exec_steps.min(t.steps.len());
                (t.id, t.title.clone(), self.chart_tabs.contains_key(&t.id), t.steps[..n].iter().any(|s| s.as_pivot_spec().is_some()))
            })
            .filter(|(_, _, chart, pivot)| *chart || *pivot)
            .collect();
        ids.into_iter().map(|(id, title, chart, _)| (self.model_ensure_key(id), title, chart)).collect()
    }

    /// The fields a page filter can use on a data tab: its columns, then each linked lookup's (and the Calendar's) fields.
    fn dash_field_choices(&self, tab_id: u64) -> Vec<(String, Option<ModelFieldRef>)> {
        let mut out: Vec<(String, Option<ModelFieldRef>)> = self.model_tab_info(tab_id).map(|i| i.columns.into_iter().map(|(c, _)| (c, None)).collect()).unwrap_or_default();
        for g in self.model_groups_for(tab_id) {
            if g.refusal.is_some() {
                continue;
            }
            out.extend(g.columns.into_iter().map(|(f, _)| (f.name.clone(), Some(f))));
        }
        out
    }

    fn dash_text_edit(ui: &mut egui::Ui, name: &str, text: &mut String, rects: &mut Vec<(String, egui::Rect)>, width: f32) -> bool {
        let r = ui.add(egui::TextEdit::singleline(text).desired_width(width));
        rects.push((name.to_string(), r.rect));
        r.changed()
    }

    fn dash_button(ui: &mut egui::Ui, name: &str, text: &str, rects: &mut Vec<(String, egui::Rect)>) -> bool {
        let r = ui.button(text);
        rects.push((name.to_string(), r.rect));
        r.clicked()
    }

    /// The whole dashboard view. It replaces the table area while `dashboard_ui.open`.
    pub(crate) fn render_dashboard_main(&mut self, ui: &mut egui::Ui) {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects: Vec<(String, egui::Rect)> = Vec::new();
        let Some(mut dash) = self.project.dashboard.clone() else {
            ui.label(NO_PROJECT);
            return;
        };
        if self.dashboard_ui.tpl.is_some() {
            self.render_design_editor(ui, &mut rects);
            self.dashboard_ui.rects = rects;
            return;
        }
        self.dash_load_import(ui.ctx());
        self.dash_load_backgrounds(ui.ctx(), &dash);
        let before = dash.clone();
        if dash.pages.is_empty() {
            dash.pages.push(DashPage { id: "p1".into(), title: "Page 1".into(), ..Default::default() });
        }
        let pi = self.dashboard_ui.page.min(dash.pages.len() - 1);
        self.dashboard_ui.page = pi;
        let mut close = false;
        let mut open_tab: Option<String> = None;
        let mut acts: Vec<PageAct> = Vec::new();
        let mut add_item: Option<DashItemKind> = None;
        let choices = self.dash_source_choices();

        // the toolbar: add a chart, a table, a KPI card or a filter on the left; layout, export and close on the right
        ui.horizontal(|ui| {
            ui.label(theme::semibold(format!("{}  {}", theme::ph::CHART_BAR, dash.title)).size(theme::TEXT_HEADING));
            ui.add_space(theme::SPACE_M);
            for (name, text, charts) in [("Add chart", format!("{}  Chart", theme::ph::PLUS), true), ("Add table", format!("{}  Table", theme::ph::PLUS), false)] {
                let r = ui.menu_button(text, |ui| {
                    let mut any = false;
                    for (key, title, chart) in choices.iter().filter(|(_, _, c)| *c == charts) {
                        any = true;
                        if ui.button(format!("{}  {}", if *chart { theme::ph::CHART_BAR } else { theme::ph::TABLE }, title)).clicked() {
                            add_item = Some(if *chart { DashItemKind::Chart(key.clone()) } else { DashItemKind::Pivot(key.clone()) });
                            ui.close_menu();
                        }
                    }
                    if !any {
                        ui.label(if charts { "No chart tab is open. Make one in the Report view first." } else { "No pivot tab is open. Make one in the Report view first." });
                    }
                });
                rects.push((name.into(), r.response.rect));
            }
            if Self::dash_button(ui, "Add KPI card", &format!("{}  KPI", theme::ph::PLUS), &mut rects) {
                add_item = Some(DashItemKind::Kpi(KpiSpec { title: "New KPI".into(), ..Default::default() }));
            }
            if Self::dash_button(ui, "Add filter", &format!("{}  Filter", theme::ph::PLUS), &mut rects) {
                if dash.pages[pi].filters.len() >= MAX_PAGE_FILTERS {
                    self.dashboard_ui.notice = FIFTH_FILTER.to_string();
                } else {
                    self.dashboard_ui.notice.clear();
                    self.dashboard_ui.add_filter = Some(AddFilter::default());
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if Self::dash_button(ui, "Close dashboard", "Close", &mut rects) {
                    close = true;
                }
                // one Export button: the three files are inside
                let r = ui.menu_button(format!("{}  Export", theme::ph::EXPORT), |ui| {
                    for (name, f) in [("Export HTML", crate::ui::dashboard_export::ExportFormat::Html), ("Export PowerPoint", crate::ui::dashboard_export::ExportFormat::Pptx), ("Export PDF", crate::ui::dashboard_export::ExportFormat::Pdf)] {
                        let text = match f {
                            crate::ui::dashboard_export::ExportFormat::Html => "HTML report\u{2026}",
                            crate::ui::dashboard_export::ExportFormat::Pptx => "PowerPoint\u{2026}",
                            crate::ui::dashboard_export::ExportFormat::Pdf => "PDF\u{2026}",
                        };
                        let b = ui.button(text);
                        rects.push((name.into(), b.rect));
                        if b.clicked() {
                            self.dashboard_ui.export = Some(crate::ui::dashboard_export::ExportDlg { format: f, ..Default::default() });
                            ui.close_menu();
                        }
                    }
                });
                rects.push(("Export".into(), r.response.rect));
                // Clear is under a menu and asks first
                let r = ui.menu_button(theme::ph::DOTS_THREE, |ui| {
                    let b = ui.button("Clear the filters and clicks on this page");
                    rects.push(("Clear".into(), b.rect));
                    if b.clicked() {
                        dash.pages[pi].state.clear();
                        self.dashboard_ui.notice.clear();
                        ui.close_menu();
                    }
                });
                rects.push(("More".into(), r.response.rect));
            });
        });
        if !self.dashboard_ui.notice.is_empty() {
            ui.colored_label(tk.danger, self.dashboard_ui.notice.clone());
        }

        self.fill_row(ui, &mut dash, pi, &choices, &mut rects);

        // the "Add filter" row
        if let Some(mut af) = self.dashboard_ui.add_filter.clone() {
            let data_tabs: Vec<(u64, String)> = self.tabs.iter().filter(|t| !self.chart_tabs.contains_key(&t.id) && !matches!(t.source, TabSource::Unavailable { .. })).map(|t| (t.id, t.title.clone())).collect();
            let fields = af.tab.map(|t| self.dash_field_choices(t)).unwrap_or_default();
            let (mut add, mut cancel) = (false, false);
            ui.horizontal_wrapped(|ui| {
                ui.label("Data tab");
                let cur = af.tab.and_then(|id| data_tabs.iter().find(|(i, _)| *i == id)).map(|(_, t)| t.clone()).unwrap_or_else(|| "Pick a tab".into());
                let r = egui::ComboBox::from_id_salt("dash_add_filter_tab").selected_text(cur).show_ui(ui, |ui| {
                    for (id, title) in &data_tabs {
                        if ui.selectable_label(af.tab == Some(*id), title).clicked() {
                            af.tab = Some(*id);
                            af.field = None;
                        }
                    }
                });
                rects.push(("Filter data tab".into(), r.response.rect));
                ui.label("Field");
                let cur = af.field.as_ref().map(|(n, _)| n.clone()).unwrap_or_else(|| "Pick a field".into());
                let r = egui::ComboBox::from_id_salt("dash_add_filter_field").selected_text(cur).show_ui(ui, |ui| {
                    for (name, m) in &fields {
                        if ui.selectable_label(af.field.as_ref().is_some_and(|(n, _)| n == name), name).clicked() {
                            af.field = Some((name.clone(), m.clone()));
                            if af.label.is_empty() {
                                af.label = name.rsplit(" \u{b7} ").next().unwrap_or(name).to_string();
                            }
                        }
                    }
                });
                rects.push(("Filter field".into(), r.response.rect));
                ui.label("Label");
                Self::dash_text_edit(ui, "Filter label", &mut af.label, &mut rects, theme::DASH_LABEL_EDIT_W);
                let r = ui.checkbox(&mut af.multi, "Several values");
                rects.push(("Filter several".into(), r.rect));
                let ready = af.tab.is_some() && af.field.is_some() && !af.label.trim().is_empty();
                let r = ui.add_enabled(ready, egui::Button::new("Add"));
                rects.push(("Filter Add".into(), r.rect));
                add = r.clicked();
                cancel = Self::dash_button(ui, "Filter Cancel", "Cancel", &mut rects);
            });
            if add {
                let (tab_id, (field, model)) = (af.tab.unwrap_or(0), af.field.clone().unwrap_or_default());
                let key = self.model_ensure_key(tab_id);
                let taken: Vec<String> = dash.pages.iter().flat_map(|p| p.filters.iter().map(|f| f.id.clone())).collect();
                let id = self.dashboard_ui.fresh_id("f", &taken);
                let f = PageFilter { id, label: af.label.trim().to_string(), tab: key, field, model, multi: af.multi };
                match dash.pages[pi].add_filter(f) {
                    Ok(()) => {
                        self.dashboard_ui.notice.clear();
                        self.dashboard_ui.add_filter = None;
                    }
                    Err(e) => {
                        self.dashboard_ui.notice = format!("{}", e);
                        self.dashboard_ui.add_filter = None;
                    }
                }
            } else if cancel {
                self.dashboard_ui.add_filter = None;
            } else {
                self.dashboard_ui.add_filter = Some(af);
            }
        }

        // the page's filters
        if !dash.pages[pi].filters.is_empty() {
            let mut remove: Option<usize> = None;
            ui.horizontal_wrapped(|ui| {
                for i in 0..dash.pages[pi].filters.len() {
                    let f = dash.pages[pi].filters[i].clone();
                    let vals = self.dashboard_ui.values.get(&f.id).cloned().unwrap_or_default();
                    ui.label(format!("{}:", f.label));
                    let mut pick = dash.pages[pi].state.filter_picks[i].clone();
                    let shown = match &pick {
                        None => "All".to_string(),
                        Some(v) if v.is_empty() => "All".to_string(),
                        Some(v) => v.join(", "),
                    };
                    let r = egui::ComboBox::from_id_salt(("dash_filter", f.id.clone())).selected_text(shown).show_ui(ui, |ui| {
                        if ui.selectable_label(pick.as_ref().map_or(true, |v| v.is_empty()), "All").clicked() {
                            pick = None;
                        }
                        if let Some(e) = &vals.error {
                            ui.colored_label(tk.danger, e.clone());
                        }
                        // a saved pick that is no longer in the data stays, said so
                        let missing: Vec<String> = pick.iter().flatten().filter(|p| !vals.values.contains(p) && vals.pending.is_none() && !vals.sig.is_empty()).cloned().collect();
                        for v in vals.values.iter().chain(missing.iter()) {
                            let on = pick.as_ref().is_some_and(|p| p.contains(v));
                            let text = if missing.contains(v) { format!("{} (not in data)", v) } else { v.clone() };
                            if ui.selectable_label(on, text).clicked() {
                                if f.multi {
                                    let mut p = pick.clone().unwrap_or_default();
                                    if on {
                                        p.retain(|x| x != v);
                                    } else {
                                        p.push(v.clone());
                                    }
                                    pick = if p.is_empty() { None } else { Some(p) };
                                } else {
                                    pick = Some(vec![v.clone()]);
                                    ui.close_menu();
                                }
                            }
                        }
                        if vals.truncated {
                            ui.label(format!("Only the first {} values are listed.", MAX_FILTER_VALUES));
                        }
                    });
                    rects.push((format!("Filter {}", f.label), r.response.rect));
                    dash.pages[pi].state.filter_picks[i] = pick;
                    let x = ui.small_button(theme::ph::X).on_hover_text("Remove this filter");
                    rects.push((format!("Remove filter {}", f.label), x.rect));
                    if x.clicked() {
                        remove = Some(i);
                    }
                    ui.add_space(theme::SPACE_S);
                }
            });
            if let Some(i) = remove {
                dash.pages[pi].remove_filter(i);
            }
        }
        // the click on a mark or a pivot row, as a chip that clears it
        if let Some(c) = dash.pages[pi].state.cross.clone() {
            ui.horizontal(|ui| {
                ui.label("Clicked:");
                let b = ui.button(format!("{} \u{d7}", c.chip())).on_hover_text("Click to clear this filter");
                rects.push(("Cross chip".into(), b.rect));
                if b.clicked() {
                    dash.pages[pi].state.cross = None;
                }
            });
        }
        ui.add_space(theme::SPACE_XS);

        // pages (left), properties (right), the page (centre)
        let mut select_page: Option<usize> = None;
        let mut new_page = false;
        let mut page_op: Option<(&'static str, usize)> = None;
        let mut item_op: Option<(&'static str, usize)> = None;
        egui::SidePanel::left("dash_pages").exact_width(theme::DASH_LEFT_W).resizable(false).frame(egui::Frame::none()).show_inside(ui, |ui| {
            ui.label(theme::semibold("Pages"));
            for (i, p) in dash.pages.iter().enumerate() {
                let r = ui.selectable_label(i == pi, if p.title.is_empty() { "(untitled)" } else { &p.title });
                rects.push((format!("Page {}", i + 1), r.rect));
                if r.clicked() {
                    select_page = Some(i);
                }
            }
            if Self::dash_button(ui, "Add page", &format!("{}  Add page", theme::ph::PLUS), &mut rects) {
                new_page = true;
            }
            ui.separator();
            // the page's own settings stay here, so the canvas keeps the whole width
            egui::ScrollArea::vertical().id_salt("dash_page_scroll").auto_shrink([false, false]).show(ui, |ui| {
                let sel = self.dashboard_ui.selected.clone().and_then(|id| dash.pages[pi].items.iter().position(|i| i.id == id));
                let _ = sel;
            ui.label(theme::semibold("Page"));
            let mut title = dash.pages[pi].title.clone();
            if Self::dash_text_edit(ui, "Page title", &mut title, &mut rects, theme::DASH_LEFT_W - theme::DASH_PROPS_PAD) {
                dash.pages[pi].title = title;
            }
            if let PageLayout::Grid { per_row } = dash.pages[pi].layout {
                ui.horizontal(|ui| {
                    ui.label("Items per row");
                    let mut n = per_row;
                    let r = egui::ComboBox::from_id_salt("dash_per_row").selected_text(n.to_string()).width(theme::DASH_PER_ROW_W).show_ui(ui, |ui| {
                        for k in 1..=MAX_PER_ROW {
                            ui.selectable_value(&mut n, k, k.to_string());
                        }
                    });
                    rects.push(("Items per row".into(), r.response.rect));
                    dash.pages[pi].layout = PageLayout::Grid { per_row: n };
                });
            }
            // the layout: Grid or Free, with a dialog that says what changes before it does
            ui.horizontal(|ui| {
                ui.label("Layout");
                let free = dash.pages[pi].layout == PageLayout::Free;
                for (name, is_free) in [("Grid", false), ("Free", true)] {
                    let r = ui.selectable_label(free == is_free, name);
                    rects.push((format!("Layout {name}"), r.rect));
                    if r.clicked() && free != is_free {
                        self.dashboard_ui.pending_layout = Some(is_free);
                    }
                }
            });
            if let Some(to_free) = self.dashboard_ui.pending_layout {
                if (dash.pages[pi].layout == PageLayout::Free) == to_free {
                    self.dashboard_ui.pending_layout = None;
                } else {
                    ui.label(egui::RichText::new(format!("Switch to {}? {}", if to_free { "Free" } else { "Grid" }, dash.pages[pi].switch_message())).color(tk.warning));
                    ui.horizontal(|ui| {
                        if Self::dash_button(ui, "Confirm layout", "Switch layout", &mut rects) {
                            if to_free {
                                if let Err(e) = dash.pages[pi].to_free() {
                                    self.dashboard_ui.notice = format!("{:#}", e);
                                }
                            } else {
                                dash.pages[pi].to_grid();
                            }
                            self.dashboard_ui.pending_layout = None;
                        }
                        if Self::dash_button(ui, "Cancel layout", "Keep layout", &mut rects) {
                            self.dashboard_ui.pending_layout = None;
                        }
                    });
                }
            }
            if dash.pages[pi].layout == PageLayout::Free {
                let r = ui.checkbox(&mut dash.pages[pi].allow_overlap, "Allow overlap");
                rects.push(("Allow overlap".into(), r.rect));
            }
            self.design_settings(ui, &mut dash, pi, &mut rects);
            ui.horizontal_wrapped(|ui| {
                if Self::dash_button(ui, "Page earlier", "Move up", &mut rects) {
                    page_op = Some(("up", pi));
                }
                if Self::dash_button(ui, "Page later", "Move down", &mut rects) {
                    page_op = Some(("down", pi));
                }
            });
            if self.dashboard_ui.confirm_delete {
                ui.colored_label(tk.danger, format!("Delete the page '{}' and its {} items?", dash.pages[pi].title, dash.pages[pi].items.len()));
                ui.horizontal(|ui| {
                    if Self::dash_button(ui, "Confirm delete page", "Delete", &mut rects) {
                        page_op = Some(("delete", pi));
                    }
                    if Self::dash_button(ui, "Keep page", "Keep", &mut rects) {
                        self.dashboard_ui.confirm_delete = false;
                    }
                });
            } else if Self::dash_button(ui, "Delete page", "Delete page", &mut rects) {
                self.dashboard_ui.confirm_delete = true;
            }
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::none()).show_inside(ui, |ui| {
            self.render_dash_page(ui, &dash.pages[pi], dash.default_kpi_unit, &mut rects, &mut open_tab, &mut acts);
        });
        // the selected tile's settings float beside it and go when you click somewhere else
        if let Some(ii) = self.dashboard_ui.selected.clone().and_then(|id| dash.pages[pi].items.iter().position(|i| i.id == id)) {
            let tile = rects.iter().find(|(n, _)| *n == format!("Item {}", dash.pages[pi].items[ii].id)).map(|(_, r)| *r);
            let page_area = rects.iter().find(|(n, _)| n == "Page area").map(|(_, r)| *r).unwrap_or_else(|| ui.max_rect());
            if let Some(tile) = tile {
                let right_room = page_area.right() - tile.right();
                let x = if right_room >= theme::TILE_PANEL_W + theme::TILE_PANEL_GAP { tile.right() + theme::TILE_PANEL_GAP } else { (tile.left() - theme::TILE_PANEL_W - theme::TILE_PANEL_GAP).max(page_area.left()) };
                let pos = egui::pos2(x, tile.top().max(page_area.top()));
                let ctx = ui.ctx().clone();
                let area = egui::Area::new(egui::Id::new("dash_tile_panel")).order(egui::Order::Foreground).fixed_pos(pos).constrain_to(page_area.union(ui.max_rect())).show(&ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_width(theme::TILE_PANEL_W);
                        egui::ScrollArea::vertical().id_salt("dash_props_scroll").max_height(theme::TILE_PANEL_MAX_H).auto_shrink([true, true]).show(ui, |ui| {
                            let ii = ii;
                    ui.label(theme::semibold("Item"));
                    let source = match &dash.pages[pi].items[ii].kind {
                        DashItemKind::Chart(k) | DashItemKind::Pivot(k) => self.dash_index_of_key(k).map(|i| self.tabs[i].title.clone()).unwrap_or_else(|| SOURCE_REMOVED.to_string()),
                        DashItemKind::Kpi(k) => k.title.clone(),
                    };
                    ui.label(egui::RichText::new(source).color(tk.muted));
                    let mut t = dash.pages[pi].items[ii].title_override.clone().unwrap_or_default();
                    ui.label("Title");
                    if Self::dash_text_edit(ui, "Item title", &mut t, &mut rects, theme::TILE_PANEL_W - theme::DASH_PROPS_PAD) {
                        dash.pages[pi].items[ii].title_override = if t.is_empty() { None } else { Some(t) };
                    }
                    if let DashItemKind::Kpi(k) = &mut dash.pages[pi].items[ii].kind {
                        self.kpi_editor(ui, k, &mut rects);
                    }
                    if matches!(dash.pages[pi].items[ii].kind, DashItemKind::Pivot(_)) {
                        ui.label("Unit for money columns");
                        let mut u = dash.pages[pi].items[ii].unit.unwrap_or_default();
                        let before = u;
                        egui::ComboBox::from_id_salt("dash_item_unit").selected_text(u.label()).show_ui(ui, |ui| {
                            for x in ItemUnit::ALL {
                                ui.selectable_value(&mut u, x, x.label());
                            }
                        });
                        if u != before {
                            dash.pages[pi].items[ii].unit = if u == ItemUnit::Crore { None } else { Some(u) };
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        if Self::dash_button(ui, "Item earlier", "Move earlier", &mut rects) {
                            item_op = Some(("up", ii));
                        }
                        if Self::dash_button(ui, "Item later", "Move later", &mut rects) {
                            item_op = Some(("down", ii));
                        }
                        if Self::dash_button(ui, "Remove item", "Remove item", &mut rects) {
                            item_op = Some(("remove", ii));
                        }
                    });
                        });
                    });
                });
                rects.push(("Tile panel".into(), area.response.rect));
            }
        }

        // what was asked for
        if let Some(kind) = add_item {
            self.dash_add_item(&mut dash, pi, kind);
        }
        for act in acts {
            match act {
                PageAct::Cross(p) => {
                    dash.pages[pi].state.toggle_cross(p);
                    self.dashboard_ui.notice.clear();
                }
                PageAct::DrillDown(item, v) => dash.pages[pi].state.drill_down(&item, &v),
                PageAct::DrillBack(item, depth) => dash.pages[pi].state.drill_back_to(&item, depth),
                PageAct::Notice(m) => self.dashboard_ui.notice = m,
                PageAct::OpenSlot(n) => self.dashboard_ui.fill_slot = Some(n),
                PageAct::Add(kind) => self.dash_add_item(&mut dash, pi, kind),
                PageAct::SetRect(item, r) => {
                    if let Some(i) = dash.pages[pi].items.iter_mut().find(|i| i.id == item) {
                        i.rect = Some(r);
                    }
                }
            }
        }
        if let Some((op, ii)) = item_op {
            let items = &mut dash.pages[pi].items;
            match op {
                "up" if ii > 0 => items.swap(ii, ii - 1),
                "down" if ii + 1 < items.len() => items.swap(ii, ii + 1),
                "remove" => {
                    let gone = items.remove(ii);
                    self.dashboard_ui.items.remove(&gone.id);
                    dash.pages[pi].state.drill.remove(&gone.id);
                    self.dashboard_ui.selected = None;
                }
                _ => {}
            }
        }
        if let Some(i) = select_page {
            self.dashboard_ui.page = i;
            self.dashboard_ui.selected = None;
            self.dashboard_ui.confirm_delete = false;
        }
        if new_page {
            let taken: Vec<String> = dash.pages.iter().map(|p| p.id.clone()).collect();
            let id = self.dashboard_ui.fresh_id("p", &taken);
            let n = dash.pages.len() + 1;
            dash.pages.push(DashPage { id, title: format!("Page {}", n), ..Default::default() });
            self.dashboard_ui.page = dash.pages.len() - 1;
            self.dashboard_ui.selected = None;
        }
        if let Some((op, i)) = page_op {
            match op {
                "up" if i > 0 => {
                    dash.pages.swap(i, i - 1);
                    self.dashboard_ui.page = i - 1;
                }
                "down" if i + 1 < dash.pages.len() => {
                    dash.pages.swap(i, i + 1);
                    self.dashboard_ui.page = i + 1;
                }
                "delete" => {
                    dash.pages.remove(i);
                    if dash.pages.is_empty() {
                        dash.pages.push(DashPage { id: "p1".into(), title: "Page 1".into(), ..Default::default() });
                    }
                    self.dashboard_ui.page = i.min(dash.pages.len() - 1);
                    self.dashboard_ui.confirm_delete = false;
                    self.dashboard_ui.selected = None;
                }
                _ => {}
            }
        }
        if dash != before {
            self.project.dashboard = Some(dash);
        }
        if let Some(key) = open_tab {
            if let Some(i) = self.dash_index_of_key(&key) {
                self.active_tab = i;
                self.selected_column = None;
                self.dashboard_ui.open = false;
            }
        }
        if close {
            self.dashboard_ui.open = false;
        }
        let ctx = ui.ctx().clone();
        self.render_export_dialog(&ctx, &mut rects);
        self.dashboard_ui.rects = rects;
    }

    /// Puts a new item on page `pi` and selects it (on a Free page, in the first free spot; a full page says so).
    fn dash_add_item(&mut self, dash: &mut Dashboard, pi: usize, kind: DashItemKind) {
            let taken: Vec<String> = dash.pages[pi].items.iter().map(|i| i.id.clone()).collect();
        let id = self.dashboard_ui.fresh_id("i", &taken);
        self.dashboard_ui.selected = Some(id.clone());
        let mut rect = None;
        let mut ok = true;
        if dash.pages[pi].layout == PageLayout::Free {
            match dash.pages[pi].first_free_spot(NEW_ITEM_W, NEW_ITEM_H) {
                Ok(r) => rect = Some(r),
                Err(e) => {
                    self.dashboard_ui.notice = format!("{:#}", e);
                    ok = false;
                }
            }
        }
        if ok {
            dash.pages[pi].items.push(DashItem { id, kind, rect, ..Default::default() });
        } else {
            self.dashboard_ui.selected = None;
        }
    }

    /// An empty page says what to add first: a chart or a table that is already open, a KPI card, or a design to import.
    fn render_empty_page_hint(&mut self, ui: &mut egui::Ui, area: egui::Rect, rects: &mut Vec<(String, egui::Rect)>, acts: &mut Vec<PageAct>) {
        let tk = *theme::tokens(self.dark_mode);
        let choices = self.dash_source_choices();
        let (charts, tables): (Vec<_>, Vec<_>) = choices.iter().partition(|(_, _, c)| *c);
        let hint = micro_parquet::workspace::empty_dashboard_hint(charts.len(), tables.len());
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area.shrink(theme::SPACE_M)).layout(egui::Layout::top_down(egui::Align::Center)));
        child.set_clip_rect(area);
        child.add_space(area.height() * theme::HINT_TOP_SHARE);
        child.label(theme::semibold(hint.title).size(theme::TEXT_HINT_TITLE).color(tk.ink));
        child.add(egui::Label::new(egui::RichText::new(hint.line).color(tk.muted)).wrap());
        child.add_space(theme::SPACE_S);
        child.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(theme::SPACE_S, theme::SPACE_S);
            for (i, c) in hint.cards.iter().enumerate() {
                egui::Frame::none().fill(tk.bg).stroke(theme::line(tk.border)).rounding(egui::Rounding::same(theme::RADIUS_PANEL)).inner_margin(egui::Margin::same(theme::SPACE_S)).show(ui, |ui| {
                    ui.set_width(theme::HINT_CARD_W);
                    ui.vertical(|ui| {
                        ui.label(theme::semibold(c.title).color(tk.ink));
                        ui.add(egui::Label::new(egui::RichText::new(c.body).color(tk.muted)).wrap());
                        match i {
                            0 | 1 => {
                                let list = if i == 0 { &charts } else { &tables };
                                let r = ui.menu_button(if i == 0 { "Add chart" } else { "Add table" }, |ui| {
                                    if list.is_empty() {
                                        ui.label("None is open. Make one in the Report view first.");
                                    }
                                    for (key, title, chart) in list.iter().copied() {
                                        if ui.button(title).clicked() {
                                            acts.push(PageAct::Add(if *chart { DashItemKind::Chart(key.clone()) } else { DashItemKind::Pivot(key.clone()) }));
                                            ui.close_menu();
                                        }
                                    }
                                });
                                rects.push((format!("Hint {}", if i == 0 { "Add chart" } else { "Add table" }), r.response.rect));
                            }
                            _ => {
                                let b = ui.button("Add KPI card");
                                rects.push(("Hint Add KPI".into(), b.rect));
                                if b.clicked() {
                                    acts.push(PageAct::Add(DashItemKind::Kpi(KpiSpec { title: "New KPI".into(), ..Default::default() })));
                                }
                            }
                        }
                    });
                });
            }
            // the design card is last: a picture from Claude Design or Canva becomes the page's layout
            egui::Frame::none().fill(tk.bg).stroke(theme::line(tk.border)).rounding(egui::Rounding::same(theme::RADIUS_PANEL)).inner_margin(egui::Margin::same(theme::SPACE_S)).show(ui, |ui| {
                ui.set_width(theme::HINT_CARD_W);
                ui.vertical(|ui| {
                    ui.label(theme::semibold("Import a design").color(tk.ink));
                    ui.add(egui::Label::new(egui::RichText::new("A page layout made in Claude Design or Canva").color(tk.muted)).wrap());
                    let b = ui.button("Import design\u{2026}");
                    rects.push(("Hint Import design".into(), b.rect));
                    if b.clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("Picture", &["png", "jpg", "jpeg"]).pick_file() {
                            self.dashboard_ui.import_path = Some(p);
                        }
                    }
                });
            });
        });
    }

    /// The page: its items in the grid, scaled to the space there is.
    fn render_dash_page(&mut self, ui: &mut egui::Ui, page: &DashPage, default_unit: KpiUnit, rects: &mut Vec<(String, egui::Rect)>, open_tab: &mut Option<String>, acts: &mut Vec<PageAct>) {
        let tk = *theme::tokens(self.dark_mode);
        let avail = ui.available_rect_before_wrap().shrink2(egui::vec2(theme::SPACE_S, 0.0));
        let scale = (avail.width() / PAGE_W as f32).min(avail.height() / page.height() as f32).max(theme::DASH_MIN_SCALE);
        let size = egui::vec2(PAGE_W as f32 * scale, page.height() as f32 * scale);
        let (area, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let area = area.translate(egui::vec2(theme::SPACE_S, 0.0));
        ui.painter().rect(area, theme::RADIUS_PANEL, tk.surface, theme::line(tk.border));
        rects.push(("Page area".into(), area));
        // a click on the page, not on an item, closes the tile's settings (the items are added after this, so they take their own clicks)
        if ui.interact(area, ui.id().with("dash_page_bg"), egui::Sense::click()).clicked() {
            self.dashboard_ui.selected = None;
        }
        if let Some(tex) = page.background.as_ref().and_then(|b| self.dashboard_ui.textures.get(&b.blake3)) {
            ui.painter().image(tex.id(), area, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
            ui.painter().rect_stroke(area, theme::RADIUS_PANEL, theme::line(tk.border));
        }
        // the empty slots of the page's design: a click opens what can fill them
        for s in page.empty_slots() {
            let sr = egui::Rect::from_min_size(area.min + egui::vec2(s.rect.x as f32 * scale, s.rect.y as f32 * scale), egui::vec2(s.rect.w as f32 * scale, s.rect.h as f32 * scale));
            let resp = ui.interact(sr, ui.id().with(("dash_slot", s.name.clone())), egui::Sense::click());
            ui.painter().rect(sr, 0.0, tk.accent.gamma_multiply(theme::DASH_SLOT_FILL), theme::line_thick(tk.accent));
            ui.painter().text(sr.center(), egui::Align2::CENTER_CENTER, format!("{} \u{b7} {}", s.name, s.kind.label()), egui::FontId::proportional(theme::TEXT_BASE), tk.ink);
            rects.push((format!("Slot {}", s.name), sr));
            if resp.clicked() {
                acts.push(PageAct::OpenSlot(s.name.clone()));
            }
        }
        let places = match item_rects(page) {
            Ok(p) => p,
            Err(e) => {
                ui.painter().text(area.center(), egui::Align2::CENTER_CENTER, e.to_string(), egui::FontId::proportional(theme::TEXT_BASE), tk.danger);
                return;
            }
        };
        if page.items.is_empty() && page.slots.is_empty() {
            self.render_empty_page_hint(ui, area, rects, acts);
        }
        // arrow keys: 10 units; with Shift they resize (the bottom right corner), on the selected item of a Free page
        if page.layout == PageLayout::Free && ui.memory(|m| m.focused().is_none()) {
            if let Some(sel) = self.dashboard_ui.selected.clone() {
                if let Some(ii) = page.items.iter().position(|i| i.id == sel) {
                    let (dx, dy, shift) = ui.input(|i| {
                        let k = |key| i.key_pressed(key) as i32 as f64;
                        (k(egui::Key::ArrowRight) - k(egui::Key::ArrowLeft), k(egui::Key::ArrowDown) - k(egui::Key::ArrowUp), i.modifiers.shift)
                    });
                    if let (true, Some(cur)) = (dx != 0.0 || dy != 0.0, page.items[ii].rect) {
                        let want = drag_rect(cur, if shift { Handle::SE } else { Handle::Move }, dx * SNAP, dy * SNAP, page.height());
                        let new = page.place(ii, want, cur);
                        if new != cur {
                            acts.push(PageAct::SetRect(sel, new));
                        }
                    }
                }
            }
        }
        for (ii, (item, r)) in page.items.iter().zip(places).enumerate() {
            let rect = egui::Rect::from_min_size(area.min + egui::vec2(r.x as f32 * scale, r.y as f32 * scale), egui::vec2(r.w as f32 * scale, r.h as f32 * scale));
            let resolved = self.dash_resolve(item);
            let selected = self.dashboard_ui.selected.as_deref() == Some(item.id.as_str());
            let free = page.layout == PageLayout::Free;
            let click = ui.interact(rect, ui.id().with(("dash_item", item.id.clone())), if free { egui::Sense::click_and_drag() } else { egui::Sense::click() });
            if free {
                if let Some(a) = self.free_drag(&click, page, ii, Handle::Move, scale) {
                    acts.push(a);
                }
            }
            ui.painter().rect(rect, theme::RADIUS_CONTROL, tk.surface, if selected { theme::line_thick(tk.accent) } else { theme::line(tk.border) });
            rects.push((format!("Item {}", item.id), rect));
            if free && selected {
                for h in Handle::ALL {
                    let c = match h {
                        Handle::NW => rect.left_top(),
                        Handle::N => rect.center_top(),
                        Handle::NE => rect.right_top(),
                        Handle::E => rect.right_center(),
                        Handle::SE => rect.right_bottom(),
                        Handle::S => rect.center_bottom(),
                        Handle::SW => rect.left_bottom(),
                        _ => rect.left_center(),
                    };
                    let hr = egui::Rect::from_center_size(c, egui::Vec2::splat(theme::DASH_HANDLE));
                    let resp = ui.interact(hr, ui.id().with(("dash_handle", item.id.clone(), h)), egui::Sense::drag());
                    ui.painter().rect(hr, 0.0, tk.accent, theme::line(tk.surface));
                    rects.push((format!("Handle {} {}", item.id, h.name()), hr));
                    resp.clone().on_hover_cursor(match h {
                        Handle::N | Handle::S => egui::CursorIcon::ResizeVertical,
                        Handle::E | Handle::W => egui::CursorIcon::ResizeHorizontal,
                        Handle::NW | Handle::SE => egui::CursorIcon::ResizeNwSe,
                        _ => egui::CursorIcon::ResizeNeSw,
                    });
                    if let Some(a) = self.free_drag(&resp, page, ii, h, scale) {
                        acts.push(a);
                    }
                }
            }
            if click.clicked() {
                self.dashboard_ui.selected = Some(item.id.clone());
            }
            let inner = rect.shrink(theme::SPACE_XS);
            let title = item.title_override.clone().or_else(|| resolved.as_ref().ok().map(|r| r.title.clone())).unwrap_or_else(|| "Item".to_string());
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
            child.set_clip_rect(inner.intersect(ui.clip_rect()));
            // on a Free page a bar above the title moves the item (a table or a chart under the pointer would take a drag of its own)
            if free {
                let (bar, resp) = child.allocate_exact_size(egui::vec2(inner.width(), theme::DASH_HANDLE), egui::Sense::drag());
                for k in -1..=1 {
                    child.painter().circle_filled(bar.center() + egui::vec2(k as f32 * theme::SPACE_S, 0.0), 1.0, tk.muted);
                }
                rects.push((format!("Move {}", item.id), bar));
                resp.clone().on_hover_cursor(egui::CursorIcon::Grab);
                if let Some(a) = self.free_drag(&resp, page, ii, Handle::Move, scale) {
                    acts.push(a);
                }
            }
            // the title opens the source tab
            let key = item.source_key().map(|k| k.to_string());
            let t = child.add(egui::Label::new(theme::semibold(title.clone()).color(if key.is_some() { tk.accent } else { tk.ink })).truncate().sense(egui::Sense::click()));
            rects.push((format!("Title {}", item.id), t.rect));
            if t.clicked() {
                if let (Some(k), Ok(_)) = (&key, &resolved) {
                    *open_tab = Some(k.clone());
                }
            }
            let view = self.dashboard_ui.items.get(&item.id).cloned().unwrap_or_default();
            let (not_applied, tabs_ok) = match &resolved {
                Ok(res) => (self.dash_picks_for(page, res.data_tab.as_deref(), &item.id).1, true),
                Err(_) => (Vec::new(), false),
            };
            for label in &not_applied {
                child.label(egui::RichText::new(format!("Filter '{}' doesn't apply to this item", label)).size(theme::TEXT_LABEL).color(tk.warning));
            }
            match (&resolved, &view.data, &view.error) {
                (Err(e), _, _) => {
                    child.label(egui::RichText::new(e.clone()).color(tk.danger));
                }
                (_, _, Some(e)) => {
                    child.label(egui::RichText::new(format!("Could not be calculated: {}", e)).color(tk.danger));
                }
                (_, None, None) => {
                    child.label(egui::RichText::new(if tabs_ok { "Calculating\u{2026}" } else { "" }).color(tk.muted));
                }
                (_, Some(ItemData::Problem(p)), _) => {
                    child.label(egui::RichText::new(p.clone()).color(tk.danger));
                }
                (Ok(res), Some(ItemData::Chart(data)), _) => {
                    let (divisor, decimals, unit) = view.scale.clone().unwrap_or((1.0, 2, String::new()));
                    let opts = micro_parquet::charts::layout::LayoutOpts { dark: self.dark_mode, divisor, decimals, unit, bare: false, colours: None };
                    let own = page.state.drill_of(&item.id).len();
                    // the item's own drill: "All › FDR › Dhaka" with Back, per item
                    if data.spec.has_drill() {
                        let mut crumbs = Vec::new();
                        let base = data.spec.depth().saturating_sub(own);
                        if let Some(d) = crate::ui::drill::drill_breadcrumb(&mut child, self.dark_mode, &data.spec, &mut crumbs) {
                            acts.push(PageAct::DrillBack(item.id.clone(), d.saturating_sub(base)));
                        }
                        rects.extend(crumbs.into_iter().map(|(n, r)| (format!("{} {}", n, item.id), r)));
                        if own > 0 {
                            let b = child.small_button("Back").on_hover_text("Back up one level on this item");
                            rects.push((format!("Back {}", item.id), b.rect));
                            if b.clicked() {
                                acts.push(PageAct::DrillBack(item.id.clone(), own - 1));
                            }
                        }
                    }
                    if let Some(c) = page.state.cross.as_ref().filter(|c| c.item == item.id) {
                        child.label(egui::RichText::new(format!("\u{25cf} Selected: {}", c.chip())).size(theme::TEXT_LABEL).color(tk.accent));
                    }
                    let h = (child.available_height() - theme::SPACE_XS).max(theme::SPACE_L);
                    let ctx = crate::ui::chart_render::DrillCtx { kind_ok: data.spec.kind.drill_reason().is_none(), has_drill: data.spec.has_drill(), can_down: data.spec.has_drill() && !data.spec.at_last_level(), next_level: data.spec.drill.get(data.spec.depth() + 1).map(|l| l.name.clone()).unwrap_or_default(), rows_unavailable: None, blocked: None, page: true };
                    let shown = crate::ui::chart_render::show_chart_with(&mut child, egui::Id::new(("dash_chart", item.id.clone())), data, &opts, Some(h), Some(&ctx));
                    match shown.action {
                        Some(crate::ui::chart_render::ChartAction::CrossFilter(m)) => match CrossPick::from_chart(&data.spec, &m.category, &item.id, res.data_tab.as_deref()) {
                            Ok(p) => acts.push(PageAct::Cross(p)),
                            Err(e) => acts.push(PageAct::Notice(format!("{:#}", e))),
                        },
                        Some(crate::ui::chart_render::ChartAction::DrillDown(m)) => match ChartSpec::refuse_other(&m.category) {
                            Ok(()) => acts.push(PageAct::DrillDown(item.id.clone(), m.category)),
                            Err(e) => acts.push(PageAct::Notice(format!("{:#}", e))),
                        },
                        _ => {}
                    }
                }
                (Err(_), Some(ItemData::Chart(_)), _) => {}
                (_, Some(ItemData::Kpi(k)), _) => {
                    let DashItemKind::Kpi(spec) = &item.kind else { continue };
                    Self::draw_kpi(&mut child, &tk, spec, k, default_unit, &item.id, rects);
                }
                (_, Some(ItemData::Table(t)), _) => {
                    if let Some(c) = page.state.cross.as_ref().filter(|c| c.item == item.id) {
                        child.label(egui::RichText::new(format!("\u{25cf} Selected: {}", c.chip())).size(theme::TEXT_LABEL).color(tk.accent));
                    }
                    if let Some((level, value)) = Self::draw_dash_table(&mut child, &item.id, t, item.unit.unwrap_or_default(), rects) {
                        if let (Ok(res), DashItemKind::Pivot(_)) = (&resolved, &item.kind) {
                            if let ItemSource::Pivot { spec, .. } = &res.source {
                                match CrossPick::from_pivot(spec, level, &value, &item.id, res.data_tab.as_deref()) {
                                    Ok(p) => acts.push(PageAct::Cross(p)),
                                    Err(e) => acts.push(PageAct::Notice(format!("{:#}", e))),
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// One drag step of an item or a handle on a Free page: the pointer's travel since the press, in page units, gives the place
    /// (snapped, inside the page, not over another item unless allowed). Returns the new place when it changed.
    fn free_drag(&mut self, resp: &egui::Response, page: &DashPage, ii: usize, handle: Handle, scale: f32) -> Option<PageAct> {
        let item = page.items.get(ii)?;
        let cur = item.rect?;
        if resp.drag_started_by(egui::PointerButton::Primary) {
            self.dashboard_ui.drag = Some(DragState { item: item.id.clone(), handle, start: cur, total: egui::Vec2::ZERO, last_ok: cur });
            self.dashboard_ui.selected = Some(item.id.clone());
        }
        let mut out = None;
        if resp.dragged() {
            if let Some(d) = self.dashboard_ui.drag.as_mut().filter(|d| d.item == item.id && d.handle == handle) {
                d.total += resp.drag_delta();
                let want = drag_rect(d.start, handle, (d.total.x / scale) as f64, (d.total.y / scale) as f64, page.height());
                let new = page.place(ii, want, d.last_ok);
                d.last_ok = new;
                if new != cur {
                    out = Some(PageAct::SetRect(item.id.clone(), new));
                }
            }
        }
        if resp.drag_stopped() && self.dashboard_ui.drag.as_ref().is_some_and(|d| d.item == item.id && d.handle == handle) {
            self.dashboard_ui.drag = None;
        }
        out
    }

    /// The card's settings: what it shows, its unit, the comparison, which direction is good, up to two limits and the sparkline.
    fn kpi_editor(&self, ui: &mut egui::Ui, k: &mut KpiSpec, rects: &mut Vec<(String, egui::Rect)>) {
        let w = theme::DASH_RIGHT_W - theme::DASH_PROPS_PAD;
        let combo = |ui: &mut egui::Ui, label: &str, name: &str, shown: String, rects: &mut Vec<(String, egui::Rect)>, body: &mut dyn FnMut(&mut egui::Ui)| {
            ui.horizontal(|ui| {
                ui.label(label);
                let r = egui::ComboBox::from_id_salt(("kpi", name.to_string())).selected_text(shown).width(w - theme::KPI_LABEL_W).show_ui(ui, |ui| body(ui));
                rects.push((name.to_string(), r.response.rect));
            });
        };
        let measures: Vec<String> = self.project.model.measures.iter().map(|m| m.name().to_string()).collect();
        let shown = match &k.source {
            KpiSource::Measure(n) if n.is_empty() => "Pick a measure".to_string(),
            KpiSource::Measure(n) => n.clone(),
            KpiSource::Value { tab, value } => format!("{} \u{b7} {}", self.dash_index_of_key(tab).map(|i| self.tabs[i].title.clone()).unwrap_or_else(|| tab.clone()), value.field),
        };
        combo(ui, "Shows", "KPI measure", shown, rects, &mut |ui| {
            if measures.is_empty() {
                ui.label("The model has no measures. Add one in Model \u{25b8} Measures\u{2026}.");
            }
            for m in &measures {
                if ui.selectable_label(k.source == KpiSource::Measure(m.clone()), m).clicked() {
                    k.source = KpiSource::Measure(m.clone());
                    if k.title.is_empty() || k.title == "New KPI" {
                        k.title = m.clone();
                    }
                }
            }
        });
        combo(ui, "Unit", "KPI unit", k.unit.label().to_string(), rects, &mut |ui| {
            for u in KpiUnit::ALL {
                ui.selectable_value(&mut k.unit, u, u.label());
            }
        });
        ui.horizontal(|ui| {
            ui.label("Decimals");
            let r = ui.add(egui::DragValue::new(&mut k.decimals).range(0..=6));
            rects.push(("KPI decimals".into(), r.rect));
        });
        let cmp = match k.compare {
            KpiCompare::None => "Nothing",
            KpiCompare::PriorPeriod => "Prior period",
            KpiCompare::Sply => "Same period last year",
        };
        combo(ui, "Compare with", "KPI compare", cmp.to_string(), rects, &mut |ui| {
            ui.selectable_value(&mut k.compare, KpiCompare::None, "Nothing");
            ui.selectable_value(&mut k.compare, KpiCompare::PriorPeriod, "Prior period");
            ui.selectable_value(&mut k.compare, KpiCompare::Sply, "Same period last year");
        });
        let gd = match k.good_direction {
            GoodDirection::Up => "Up is good",
            GoodDirection::Down => "Down is good",
            GoodDirection::Neutral => "No judgement",
        };
        combo(ui, "Good direction", "KPI direction", gd.to_string(), rects, &mut |ui| {
            ui.selectable_value(&mut k.good_direction, GoodDirection::Up, "Up is good");
            ui.selectable_value(&mut k.good_direction, GoodDirection::Down, "Down is good");
            ui.selectable_value(&mut k.good_direction, GoodDirection::Neutral, "No judgement");
        });
        ui.label("Limits");
        let mut drop: Option<usize> = None;
        for (i, t) in k.thresholds.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let op = if t.op == ThresholdOp::Below { "below" } else { "above" };
                if ui.small_button(op).clicked() {
                    t.op = if t.op == ThresholdOp::Below { ThresholdOp::Above } else { ThresholdOp::Below };
                }
                let r = ui.add(egui::DragValue::new(&mut t.limit).speed(0.5));
                rects.push((format!("KPI limit {}", i + 1), r.rect));
                let c = match t.colour {
                    ThresholdColour::Amber => "amber",
                    ThresholdColour::Red => "red",
                    ThresholdColour::Green => "green",
                };
                if ui.small_button(c).clicked() {
                    t.colour = match t.colour {
                        ThresholdColour::Amber => ThresholdColour::Red,
                        ThresholdColour::Red => ThresholdColour::Green,
                        ThresholdColour::Green => ThresholdColour::Amber,
                    };
                }
                if ui.small_button(theme::ph::X).clicked() {
                    drop = Some(i);
                }
            });
        }
        if let Some(i) = drop {
            k.thresholds.remove(i);
        }
        if k.thresholds.len() < MAX_THRESHOLDS {
            let b = ui.button("Add limit");
            rects.push(("KPI add limit".into(), b.rect));
            if b.clicked() {
                k.thresholds.push(Threshold::default());
            }
        }
        let r = ui.checkbox(&mut k.sparkline, "Sparkline (12 months)");
        rects.push(("KPI sparkline".into(), r.rect));
    }

    /// A KPI card: the number, the change with its arrow and words, and the sparkline. A tooltip carries the same words.
    fn draw_kpi(ui: &mut egui::Ui, tk: &theme::Tokens, spec: &KpiSpec, v: &KpiValue, default_unit: KpiUnit, id: &str, rects: &mut Vec<(String, egui::Rect)>) {
        let kv = view(spec, v, default_unit);
        let tone_colour = |t: Tone| match t {
            Tone::Good => tk.success,
            Tone::Bad => tk.danger,
            Tone::Neutral => tk.muted,
        };
        let number = match kv.threshold {
            Some(ThresholdColour::Amber) => tk.warning,
            Some(ThresholdColour::Red) => tk.danger,
            Some(ThresholdColour::Green) => tk.success,
            None => tk.ink,
        };
        let big = ui.add(egui::Label::new(theme::semibold(kv.value_text.clone()).size(theme::TEXT_KPI).color(number)).truncate());
        rects.push((format!("KPI value {}", id), big.rect));
        if !kv.change_text.is_empty() {
            let text = if kv.arrow.is_empty() { kv.change_text.clone() } else { format!("{} {}", kv.arrow, kv.change_text) };
            let r = ui.add(egui::Label::new(egui::RichText::new(text).color(tone_colour(kv.tone))).truncate());
            rects.push((format!("KPI change {}", id), r.rect));
            if !kv.words.is_empty() {
                r.on_hover_text(format!("{} against the earlier period", kv.words));
            }
        }
        if let Some(why) = &kv.reason {
            ui.label(egui::RichText::new(why.clone()).size(theme::TEXT_LABEL).color(tk.muted));
        }
        for n in v.notes.iter().take(2) {
            ui.label(egui::RichText::new(n.clone()).size(theme::TEXT_LABEL).color(tk.muted));
        }
        ui.label(egui::RichText::new(v.period.clone()).size(theme::TEXT_LABEL).color(tk.muted));
        if !kv.spark.is_empty() {
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(theme::KPI_SPARK_W), theme::KPI_SPARK_H), egui::Sense::hover());
            rects.push((format!("KPI spark {}", id), rect));
            let vals: Vec<f64> = kv.spark.iter().flatten().copied().collect();
            let (lo, hi) = vals.iter().fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(*x), b.max(*x)));
            let span = if hi > lo { hi - lo } else { 1.0 };
            let n = kv.spark.len().max(2) - 1;
            let pt = |i: usize, y: f64| egui::pos2(rect.left() + rect.width() * i as f32 / n as f32, rect.bottom() - rect.height() * ((y - lo) / span) as f32);
            // a gap breaks the line: each run of numbers is its own line
            let mut run: Vec<egui::Pos2> = Vec::new();
            let flush = |run: &mut Vec<egui::Pos2>, painter: &egui::Painter| {
                match run.len() {
                    0 => {}
                    1 => {
                        painter.circle_filled(run[0], theme::KPI_SPARK_DOT, tk.accent);
                    }
                    _ => {
                        painter.add(egui::Shape::line(run.clone(), theme::line_thick(tk.accent)));
                    }
                }
                run.clear();
            };
            for (i, y) in kv.spark.iter().enumerate() {
                match y {
                    Some(y) => run.push(pt(i, *y)),
                    None => flush(&mut run, ui.painter()),
                }
            }
            flush(&mut run, ui.painter());
            resp.on_hover_text(format!("{} periods; a gap is a period with no number", kv.spark.len()));
        }
        ui.interact(ui.min_rect(), ui.id().with(("kpi_alt", id.to_string())), egui::Sense::hover()).on_hover_text(kv.alt.clone());
    }

    /// A pivot as a table (Nested, with its indents, when the pivot is Nested). A click on a row label comes back as
    /// (the Rows field it belongs to, its text); the labels are buttons without a frame, named "Row <item> <text>".
    fn draw_dash_table(ui: &mut egui::Ui, id: &str, t: &TableData, unit: ItemUnit, rects: &mut Vec<(String, egui::Rect)>) -> Option<(usize, String)> {
        use polars::prelude::AnyValue;
        // `k` is the value column (None for a label column): money is shown in the item's unit, counts whole, the rest as they are
        let text = |s: &polars::prelude::Series, i: usize, k: Option<usize>| -> String {
            let (div, dec) = k.map(|k| cell_scale(t.kind_of(k), unit)).unwrap_or((1.0, 2));
            match s.get(i) {
                Ok(AnyValue::Null) | Err(_) => String::new(),
                Ok(AnyValue::Float64(v)) => format_with_commas(v / div, dec),
                Ok(AnyValue::Float32(v)) => format_with_commas(v as f64 / div, dec),
                Ok(AnyValue::String(v)) => v.to_string(),
                Ok(AnyValue::StringOwned(v)) => v.to_string(),
                Ok(v) => format!("{}", v),
            }
        };
        let mut clicked: Option<(usize, String)> = None;
        let cols: Vec<_> = t.df.get_columns().iter().collect();
        let mut label = |ui: &mut egui::Ui, shown: String, plain: &str, level: usize, clicked: &mut Option<(usize, String)>| {
            if plain.is_empty() {
                ui.label(shown);
                return;
            }
            let r = ui.add(egui::Label::new(shown).sense(egui::Sense::click())).on_hover_text("Click to filter the page by this row");
            rects.push((format!("Row {} {}", id, plain), r.rect));
            if r.clicked() {
                *clicked = Some((level, plain.to_string()));
            }
        };
        egui::ScrollArea::both().id_salt(("dash_table", id.to_string())).auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new(("dash_grid", id.to_string())).striped(true).spacing(egui::vec2(theme::SPACE_M, 2.0)).show(ui, |ui| {
                match &t.nested {
                    Some(n) => {
                        ui.label(theme::semibold(n.label_header.clone()));
                        for (k, c) in n.values.get_columns().iter().enumerate() {
                            ui.label(theme::semibold(heading_with_unit(c.name(), t.kind_of(k), unit)));
                        }
                        ui.end_row();
                        for (i, r) in n.rows.iter().enumerate().take(300) {
                            let plain = if r.is_subtotal() || r.label == micro_parquet::pivot_table::GRAND_TOTAL_LABEL { "" } else { r.label.as_str() };
                            label(ui, format!("{}{}", "\u{a0}\u{a0}".repeat(r.level), r.label), plain, r.level, &mut clicked);
                            for (k, c) in n.values.get_columns().iter().enumerate() {
                                ui.label(text(c, i, Some(k)));
                            }
                            ui.end_row();
                        }
                    }
                    None => {
                        for (ci, c) in cols.iter().enumerate() {
                            if ci < t.label_columns {
                                ui.label(theme::semibold(c.name().to_string()));
                            } else {
                                ui.label(theme::semibold(heading_with_unit(c.name(), t.kind_of(ci - t.label_columns), unit)));
                            }
                        }
                        ui.end_row();
                        for i in 0..t.df.height().min(300) {
                            for (ci, c) in cols.iter().enumerate() {
                                let s = text(c, i, ci.checked_sub(t.label_columns));
                                if ci < t.label_columns {
                                    let plain = if s == micro_parquet::pivot_table::GRAND_TOTAL_LABEL { "" } else { s.as_str() };
                                    label(ui, s.clone(), plain, ci, &mut clicked);
                                } else {
                                    ui.label(s);
                                }
                            }
                            ui.end_row();
                        }
                    }
                }
            });
        });
        clicked
    }
}

#[allow(dead_code)]
fn _kpi_is_next(_: &KpiSpec) {}
