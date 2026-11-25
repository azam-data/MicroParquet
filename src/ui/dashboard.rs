//! The dashboard view (Phase 10 D1): pages on the left, the page in the centre, the properties of the page or of the selected item on
//! the right. An item points at a chart tab or a pivot tab and draws what that tab's own code gives (`dashboard::compute_item`, on the
//! worker); a page filter is a dropdown of a lookup, Calendar or data field of one data tab. The window never waits: a number that is
//! being calculated shows "Calculating…".

use crate::ui::prelude::*;
use micro_parquet::dashboard::*;
use micro_parquet::kpi::KpiSpec;
use micro_parquet::model::ModelFieldRef;
use micro_parquet::models::*;
use micro_parquet::tab_jobs::{dash_item_job, dash_values_job, source_signature};
use std::collections::HashMap;

/// What "Dashboard" says outside a project.
pub const NO_PROJECT: &str = "Dashboards belong to a project. Save or open a project first.";
pub const DASH_BUTTON: &str = "Dashboard";

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
    /// A refusal or a note in plain words, shown under the header.
    pub notice: String,
    pub next_id: u64,
    /// The controls drawn in the last frame, by name (the render tests click these rects).
    pub rects: Vec<(String, egui::Rect)>,
    /// The Dashboard entry of the tab strip, as drawn in the last frame.
    pub strip_rect: Option<egui::Rect>,
    /// The id of each worker job's scope, by item or filter id.
    pub scopes: HashMap<u64, String>,
}

fn scope_of(id: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h);
    h.finish()
}

impl DashboardUi {
    fn fresh_id(&mut self, prefix: &str, taken: &[String]) -> String {
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
            DashItemKind::Kpi(_) => Err("KPI cards come with the next step of this phase.".to_string()),
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
    fn dash_picks_for(&self, page: &DashPage, data_tab: Option<&str>) -> (Vec<(PageFilter, Vec<String>)>, Vec<String>) {
        let (mut yes, mut no) = (Vec::new(), Vec::new());
        for (_, f, v) in page.active_picks() {
            if filter_applies(f, data_tab) {
                yes.push((f.clone(), v.clone()));
            } else {
                no.push(f.label.clone());
            }
        }
        (yes, no)
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
            let (picks, _) = self.dash_picks_for(&page, res.data_tab.as_deref());
            let sig = match &res.source {
                ItemSource::Chart { base, steps, spec } => format!("{}|{:?}|{:?}", source_signature(base, steps), spec, picks),
                ItemSource::Pivot { base, steps, spec } => format!("{}|{:?}|{:?}", source_signature(base, steps), spec, picks),
            };
            if view.sig == sig || view.pending.as_deref() == Some(sig.as_str()) {
                continue;
            }
            let req = ItemRequest { item_id: item.id.clone(), source: res.source, picks, temp_dir: self.paths.temp_dir.clone() };
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
    fn dash_source_choices(&mut self) -> Vec<(String, String, bool)> {
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
        let before = dash.clone();
        if dash.pages.is_empty() {
            dash.pages.push(DashPage { id: "p1".into(), title: "Page 1".into(), ..Default::default() });
        }
        let pi = self.dashboard_ui.page.min(dash.pages.len() - 1);
        self.dashboard_ui.page = pi;
        let mut close = false;
        let mut open_tab: Option<String> = None;
        let mut add_item: Option<DashItemKind> = None;
        let choices = self.dash_source_choices();

        // header
        ui.horizontal(|ui| {
            ui.label(theme::semibold(format!("{}  {}", theme::ph::CHART_BAR, dash.title)).size(theme::TEXT_HEADING));
            ui.add_space(theme::SPACE_M);
            let r = ui.menu_button("Add item", |ui| {
                if choices.is_empty() {
                    ui.label("No chart tab or pivot tab is open. Make one first.");
                }
                for (key, title, chart) in &choices {
                    let text = format!("{}  {}", if *chart { theme::ph::CHART_BAR } else { theme::ph::TABLE }, title);
                    let b = ui.button(text);
                    if b.clicked() {
                        add_item = Some(if *chart { DashItemKind::Chart(key.clone()) } else { DashItemKind::Pivot(key.clone()) });
                        ui.close_menu();
                    }
                }
            });
            rects.push(("Add item".into(), r.response.rect));
            if Self::dash_button(ui, "Add filter", "Add filter", &mut rects) {
                if dash.pages[pi].filters.len() >= MAX_PAGE_FILTERS {
                    self.dashboard_ui.notice = FIFTH_FILTER.to_string();
                } else {
                    self.dashboard_ui.notice.clear();
                    self.dashboard_ui.add_filter = Some(AddFilter::default());
                }
            }
            if Self::dash_button(ui, "Clear", "Clear", &mut rects) {
                dash.pages[pi].state.clear();
                self.dashboard_ui.notice.clear();
            }
            if Self::dash_button(ui, "Close dashboard", "Close dashboard", &mut rects) {
                close = true;
            }
        });
        if !self.dashboard_ui.notice.is_empty() {
            ui.colored_label(tk.danger, self.dashboard_ui.notice.clone());
        }

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
        ui.add_space(theme::SPACE_XS);

        // pages (left), properties (right), the page (centre)
        let mut select_page: Option<usize> = None;
        let mut new_page = false;
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
        });
        let mut page_op: Option<(&'static str, usize)> = None;
        let mut item_op: Option<(&'static str, usize)> = None;
        egui::SidePanel::right("dash_props").exact_width(theme::DASH_RIGHT_W).resizable(false).frame(egui::Frame::none()).show_inside(ui, |ui| {
            ui.label(theme::semibold("Page"));
            let mut title = dash.pages[pi].title.clone();
            if Self::dash_text_edit(ui, "Page title", &mut title, &mut rects, theme::DASH_RIGHT_W - theme::DASH_PROPS_PAD) {
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
            ui.separator();
            let sel = self.dashboard_ui.selected.clone().and_then(|id| dash.pages[pi].items.iter().position(|i| i.id == id));
            match sel {
                None => {
                    ui.label(egui::RichText::new("Click an item to change it.").color(tk.muted));
                }
                Some(ii) => {
                    ui.label(theme::semibold("Item"));
                    let source = match &dash.pages[pi].items[ii].kind {
                        DashItemKind::Chart(k) | DashItemKind::Pivot(k) => self.dash_index_of_key(k).map(|i| self.tabs[i].title.clone()).unwrap_or_else(|| SOURCE_REMOVED.to_string()),
                        DashItemKind::Kpi(k) => k.title.clone(),
                    };
                    ui.label(egui::RichText::new(source).color(tk.muted));
                    let mut t = dash.pages[pi].items[ii].title_override.clone().unwrap_or_default();
                    ui.label("Title");
                    if Self::dash_text_edit(ui, "Item title", &mut t, &mut rects, theme::DASH_RIGHT_W - theme::DASH_PROPS_PAD) {
                        dash.pages[pi].items[ii].title_override = if t.is_empty() { None } else { Some(t) };
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
                }
            }
        });
        egui::CentralPanel::default().frame(egui::Frame::none()).show_inside(ui, |ui| {
            self.render_dash_page(ui, &dash.pages[pi], &mut rects, &mut open_tab);
        });

        // what was asked for
        if let Some(kind) = add_item {
            let taken: Vec<String> = dash.pages[pi].items.iter().map(|i| i.id.clone()).collect();
            let id = self.dashboard_ui.fresh_id("i", &taken);
            self.dashboard_ui.selected = Some(id.clone());
            dash.pages[pi].items.push(DashItem { id, kind, ..Default::default() });
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
        self.dashboard_ui.rects = rects;
    }

    /// The page: its items in the grid, scaled to the space there is.
    fn render_dash_page(&mut self, ui: &mut egui::Ui, page: &DashPage, rects: &mut Vec<(String, egui::Rect)>, open_tab: &mut Option<String>) {
        let tk = *theme::tokens(self.dark_mode);
        let avail = ui.available_rect_before_wrap().shrink2(egui::vec2(theme::SPACE_S, 0.0));
        let scale = (avail.width() / PAGE_W as f32).min(avail.height() / PAGE_H as f32).max(theme::DASH_MIN_SCALE);
        let size = egui::vec2(PAGE_W as f32 * scale, PAGE_H as f32 * scale);
        let (area, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let area = area.translate(egui::vec2(theme::SPACE_S, 0.0));
        ui.painter().rect(area, theme::RADIUS_PANEL, tk.surface, theme::line(tk.border));
        rects.push(("Page area".into(), area));
        let places = match item_rects(page) {
            Ok(p) => p,
            Err(e) => {
                ui.painter().text(area.center(), egui::Align2::CENTER_CENTER, e.to_string(), egui::FontId::proportional(theme::TEXT_BASE), tk.danger);
                return;
            }
        };
        if page.items.is_empty() {
            ui.painter().text(area.center(), egui::Align2::CENTER_CENTER, "This page is empty. Use Add item to put a chart or a pivot on it.", egui::FontId::proportional(theme::TEXT_BASE), tk.muted);
        }
        for (item, r) in page.items.iter().zip(places) {
            let rect = egui::Rect::from_min_size(area.min + egui::vec2(r.x as f32 * scale, r.y as f32 * scale), egui::vec2(r.w as f32 * scale, r.h as f32 * scale));
            let resolved = self.dash_resolve(item);
            let selected = self.dashboard_ui.selected.as_deref() == Some(item.id.as_str());
            let click = ui.interact(rect, ui.id().with(("dash_item", item.id.clone())), egui::Sense::click());
            ui.painter().rect(rect, theme::RADIUS_CONTROL, tk.surface, if selected { theme::line_thick(tk.accent) } else { theme::line(tk.border) });
            rects.push((format!("Item {}", item.id), rect));
            if click.clicked() {
                self.dashboard_ui.selected = Some(item.id.clone());
            }
            let inner = rect.shrink(theme::SPACE_XS);
            let title = item.title_override.clone().or_else(|| resolved.as_ref().ok().map(|r| r.title.clone())).unwrap_or_else(|| "Item".to_string());
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
            child.set_clip_rect(inner.intersect(ui.clip_rect()));
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
                Ok(res) => (self.dash_picks_for(page, res.data_tab.as_deref()).1, true),
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
                (_, Some(ItemData::Chart(data)), _) => {
                    let (divisor, decimals, unit) = view.scale.clone().unwrap_or((1.0, 2, String::new()));
                    let opts = micro_parquet::charts::layout::LayoutOpts { dark: self.dark_mode, divisor, decimals, unit, bare: false, colours: None };
                    let h = (child.available_height() - theme::SPACE_XS).max(theme::SPACE_L);
                    crate::ui::chart_render::show_chart(&mut child, egui::Id::new(("dash_chart", item.id.clone())), data, &opts, Some(h));
                }
                (_, Some(ItemData::Table(t)), _) => {
                    Self::draw_dash_table(&mut child, &item.id, t);
                }
            }
        }
    }

    /// A pivot as a table (Nested, with its indents, when the pivot is Nested).
    fn draw_dash_table(ui: &mut egui::Ui, id: &str, t: &TableData) {
        use polars::prelude::AnyValue;
        let text = |s: &polars::prelude::Series, i: usize| -> String {
            match s.get(i) {
                Ok(AnyValue::Null) | Err(_) => String::new(),
                Ok(AnyValue::Float64(v)) => format_with_commas(v, 2),
                Ok(AnyValue::Float32(v)) => format_with_commas(v as f64, 2),
                Ok(AnyValue::String(v)) => v.to_string(),
                Ok(AnyValue::StringOwned(v)) => v.to_string(),
                Ok(v) => format!("{}", v),
            }
        };
        let cols: Vec<_> = t.df.get_columns().iter().collect();
        egui::ScrollArea::both().id_salt(("dash_table", id.to_string())).auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new(("dash_grid", id.to_string())).striped(true).spacing(egui::vec2(theme::SPACE_M, 2.0)).show(ui, |ui| {
                match &t.nested {
                    Some(n) => {
                        ui.label(theme::semibold(n.label_header.clone()));
                        for c in n.values.get_columns() {
                            ui.label(theme::semibold(c.name().to_string()));
                        }
                        ui.end_row();
                        for (i, r) in n.rows.iter().enumerate().take(300) {
                            ui.label(format!("{}{}", "\u{a0}\u{a0}".repeat(r.level), r.label));
                            for c in n.values.get_columns() {
                                ui.label(text(c, i));
                            }
                            ui.end_row();
                        }
                    }
                    None => {
                        for c in &cols {
                            ui.label(theme::semibold(c.name().to_string()));
                        }
                        ui.end_row();
                        for i in 0..t.df.height().min(300) {
                            for c in &cols {
                                ui.label(text(c, i));
                            }
                            ui.end_row();
                        }
                    }
                }
            });
        });
    }
}

#[allow(dead_code)]
fn _kpi_is_next(_: &KpiSpec) {}
