//! The window's shell (UI revamp): the rail that switches between the Report, Data and Model views, which tabs each view lists,
//! the Analytics tab of the Report view, the empty-screen hints, and "Send to report". The rules are in `micro_parquet::workspace`.

use crate::ui::prelude::*;
use micro_parquet::commands::CommandId;
use micro_parquet::workspace::{self as ws, Hint, HintAction, HintState, SettingsSection, View};

/// What the shell keeps between frames.
#[derive(Default)]
pub struct WorkspaceUi {
    pub view: View,
    /// The id of the active tab last frame: the view follows a tab that becomes active, never a quiet frame.
    pub last_active: Option<u64>,
    pub last_dash_open: bool,
    /// The tab each view had active when it was left (by id).
    pub last_in_view: HashMap<View, u64>,
    /// Where the rail's buttons were drawn (tests click them).
    pub rail_rects: Vec<(View, egui::Rect)>,
    /// Where an empty screen's buttons were drawn, by their text.
    pub hint_rects: Vec<(String, egui::Rect)>,
    /// The Settings dialog: open, and its section.
    pub settings_open: bool,
    pub settings_section: SettingsSection,
    pub settings_rects: Vec<(String, egui::Rect)>,
    /// The model's set-up: the step asked for, and where the strip and Back / Next were drawn.
    pub flow_goto: Option<micro_parquet::workspace::ModelStep>,
    pub flow_rects: Vec<(String, egui::Rect)>,
    /// Every item the top bar drew last frame.
    pub bar_rects: Vec<(String, egui::Rect)>,
    /// What the Model view drew (tests read it).
    pub model_rects: Vec<(String, egui::Rect)>,
    /// The "+" of the Report view's tab strip.
    pub plus_rect: Option<egui::Rect>,
    /// The side panel's hide button (when the panel is shown) or its show arrow (when it is hidden).
    pub panel_toggle_rect: Option<egui::Rect>,
}

impl MicroParquetApp {
    pub(crate) fn view(&self) -> View {
        self.ws.view
    }

    pub(crate) fn tab_is_pivot(&self, idx: usize) -> bool {
        self.tabs.get(idx).is_some_and(|t| t.steps.iter().any(|s| s.is_pivot()))
    }

    /// Whether the tab at `idx` is listed in `view`'s tab strip.
    pub(crate) fn tab_visible_in(&self, view: View, idx: usize) -> bool {
        let Some(t) = self.tabs.get(idx) else { return false };
        ws::tab_in_view(view, self.chart_tabs.contains_key(&t.id), self.tab_is_pivot(idx), self.project.report_ids.contains(&t.id))
    }

    /// The indexes of the tabs `view` lists, in tab order.
    pub(crate) fn visible_tab_indexes(&self, view: View) -> Vec<usize> {
        (0..self.tabs.len()).filter(|i| self.tab_visible_in(view, *i)).collect()
    }

    /// The active tab is one the current view lists.
    pub(crate) fn active_tab_in_view(&self) -> bool {
        self.tab_visible_in(self.ws.view, self.active_tab)
    }

    /// Switches the view. A view that does not list the active tab shows the tab it had when it was left (or its last tab).
    pub(crate) fn switch_view(&mut self, view: View) {
        if view == self.ws.view {
            return;
        }
        if view == View::Dashboard && !self.dashboard_command() {
            return;
        }
        if let Some(id) = self.tabs.get(self.active_tab).map(|t| t.id) {
            self.ws.last_in_view.insert(self.ws.view, id);
        }
        self.ws.view = view;
        self.settings.last_view = Some(view.label().to_string());
        self.dashboard_ui.open = view == View::Dashboard;
        if view != View::Model && view != View::Dashboard && !self.active_tab_in_view() {
            let remembered = self.ws.last_in_view.get(&view).and_then(|id| self.tabs.iter().position(|t| t.id == *id)).filter(|i| self.tab_visible_in(view, *i));
            if let Some(i) = remembered.or_else(|| self.visible_tab_indexes(view).last().copied()) {
                self.active_tab = i;
                self.selected_column = None;
                self.init_tool_defaults_from_active_tab();
            }
        }
    }

    /// Once a frame: the view follows a tab that has just become active (a chart made from the Data view opens in Report) and a
    /// dashboard that has just been opened. A view the owner chose with the rail is left alone while nothing changes.
    pub(crate) fn sync_view(&mut self) {
        let active = self.tabs.get(self.active_tab).map(|t| t.id);
        if self.ws.last_active != active {
            self.ws.last_active = active;
            if active.is_some() && self.ws.view != View::Model && self.ws.view != View::Dashboard && !self.active_tab_in_view() {
                let home = [View::Data, View::Report].into_iter().find(|v| self.tab_visible_in(*v, self.active_tab));
                if let Some(v) = home {
                    self.ws.view = v;
                    self.settings.last_view = Some(v.label().to_string());
                }
            }
        }
        // `dashboard_ui.open` turned on or off from elsewhere moves the window to the Dashboard view or back to the Report view
        let has = self.project.dashboard.is_some();
        if self.dashboard_ui.open && !self.ws.last_dash_open && self.ws.view != View::Dashboard && has {
            self.ws.view = View::Dashboard;
        } else if !self.dashboard_ui.open && self.ws.last_dash_open && self.ws.view == View::Dashboard {
            self.ws.view = View::Report;
        }
        // the Dashboard view needs the project's dashboard; a project that was closed or has none sends the window back to the Report view
        if self.ws.view == View::Dashboard && !has && !self.dashboard_command() {
            self.ws.view = View::Report;
        }
        let dash = self.ws.view == View::Dashboard && self.project.dashboard.is_some();
        self.dashboard_ui.open = dash;
        self.ws.last_dash_open = dash;
    }

    /// The commands that belong to a view take the window there first.
    pub(crate) fn enter_home_view(&mut self, id: CommandId) {
        if let Some(v) = ws::home_view(id) {
            self.switch_view(v);
        }
    }

    /// "Send to report": the active table shows in the Report view too.
    pub(crate) fn send_to_report(&mut self) {
        let Some((id, title)) = self.tabs.get(self.active_tab).map(|t| (t.id, t.title.clone())) else { return };
        if self.chart_tabs.contains_key(&id) || self.tab_is_pivot(self.active_tab) {
            self.status_message = format!("'{}' is already in the Report view.", title);
        } else if self.project.report_ids.insert(id) {
            self.status_message = format!("Sent '{}' to the Report view.", title);
        } else {
            self.status_message = format!("'{}' is already in the Report view.", title);
        }
        self.switch_view(View::Report);
    }

    /// Takes a table out of the Report view again (a pivot or a chart tab stays: it is there by what it is).
    pub(crate) fn remove_from_report(&mut self, idx: usize) {
        if let Some(id) = self.tabs.get(idx).map(|t| t.id) {
            self.project.report_ids.remove(&id);
        }
    }

    /// Shows the Analytics tab of the right panel, or hides it when it is the one in view.
    pub(crate) fn toggle_analytics(&mut self) {
        use crate::ui::steps::RightTab;
        if self.ws.view == View::Report && self.side_panel_shown() && self.right_tab == RightTab::Analytics {
            self.set_side_tab_visible(RightTab::Analytics, false);
        } else {
            self.switch_view(View::Report);
            self.set_side_tab_visible(RightTab::Analytics, true);
        }
    }

    // ------------------------------------------------------------------------------------------ the rail

    pub(crate) fn render_rail(&mut self, ctx: &egui::Context) {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects: Vec<(View, egui::Rect)> = Vec::new();
        let mut go: Option<View> = None;
        egui::SidePanel::left("view_rail").exact_width(theme::RAIL_W).resizable(false).frame(egui::Frame::none().fill(tk.bg).inner_margin(theme::MARGIN_RAIL)).show(ctx, |ui| {
            theme::right_border(ui, tk.border);
            ui.spacing_mut().item_spacing.y = theme::SPACE_XS;
            for v in View::ALL {
                let on = self.ws.view == v;
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(theme::RAIL_BTN_W, theme::RAIL_BTN_H), egui::Sense::click());
                let fill = if on { tk.accent_soft } else if resp.hovered() { tk.zebra } else { egui::Color32::TRANSPARENT };
                ui.painter().rect_filled(rect, theme::RADIUS_PANEL, fill);
                let ink = if on { tk.accent } else { tk.muted };
                let icon = match v {
                    View::Report => theme::ph::SQUARES_FOUR,
                    View::Dashboard => theme::ph::PRESENTATION_CHART,
                    View::Data => theme::ph::TABLE,
                    View::Model => theme::ph::TREE_STRUCTURE,
                };
                ui.painter().text(egui::pos2(rect.center().x, rect.top() + theme::SPACE_L), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(theme::TEXT_RAIL_ICON), ink);
                let label = if on { theme::semibold(v.label()) } else { egui::RichText::new(v.label()) };
                let galley = egui::WidgetText::from(label.size(theme::TEXT_LABEL).color(ink)).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body);
                ui.painter().galley(egui::pos2(rect.center().x - galley.size().x * theme::HALF, rect.bottom() - galley.size().y - theme::SPACE_XS), galley, ink);
                rects.push((v, rect));
                if resp.on_hover_text(v.purpose()).clicked() {
                    go = Some(v);
                }
            }
        });
        self.ws.rail_rects = rects;
        if let Some(v) = go {
            self.switch_view(v);
        }
    }

    // ------------------------------------------------------------------------------------------ hints

    /// What an empty view says, or `None` when the view has something to show.
    pub(crate) fn current_hint(&self) -> Option<Hint> {
        let s = HintState {
            data_tabs: self.visible_tab_indexes(View::Data).len(),
            report_tabs: self.visible_tab_indexes(View::Report).len(),
            project: self.project.active,
            dashboard: self.project.dashboard.is_some(),
            links: self.project.model.links.len(),
        };
        ws::empty_hint(self.ws.view, s)
    }

    /// Draws an empty screen: a line about what to do, and one card per likely next step, each with its button.
    pub(crate) fn render_hint(&mut self, ui: &mut egui::Ui, hint: &Hint) {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects: Vec<(String, egui::Rect)> = Vec::new();
        let mut act: Option<HintAction> = None;
        let top = (ui.available_height() * theme::HINT_TOP_SHARE).max(0.0);
        ui.vertical_centered(|ui| {
            ui.add_space(top);
            ui.set_max_width(theme::HINT_MAX_W.min(ui.available_width()));
            ui.label(theme::semibold(hint.title).size(theme::TEXT_HINT_TITLE).color(tk.ink));
            ui.add_space(theme::SPACE_XS);
            ui.add(egui::Label::new(egui::RichText::new(hint.line).color(tk.muted)).wrap());
            ui.add_space(theme::SPACE_M);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(theme::SPACE_S, theme::SPACE_S);
                for (i, c) in hint.cards.iter().enumerate() {
                    egui::Frame::none()
                        .fill(tk.surface)
                        .stroke(theme::line(tk.border))
                        .rounding(egui::Rounding::same(theme::RADIUS_PANEL))
                        .inner_margin(egui::Margin::same(theme::SPACE_M))
                        .show(ui, |ui| {
                            ui.set_width(theme::HINT_CARD_W);
                            ui.vertical(|ui| {
                                ui.label(theme::semibold(c.title).color(tk.ink));
                                ui.add(egui::Label::new(egui::RichText::new(c.body).color(tk.muted)).wrap());
                                ui.add_space(theme::SPACE_XS);
                                let b = if i == 0 { theme::primary_button(ui, c.button) } else { ui.button(c.button) };
                                rects.push((c.button.to_string(), b.rect));
                                if b.clicked() {
                                    act = Some(c.action);
                                }
                            });
                        });
                }
            });
        });
        // (the Data view's start screen keeps the names the first screen has always had)
        if self.ws.view == View::Data && self.tabs.is_empty() {
            let named: Vec<(String, egui::Rect)> = rects.iter().zip(["open", "stack", "batch"]).map(|((_, r), n)| (n.to_string(), *r)).collect();
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("empty_state_rects"), named));
        }
        self.ws.hint_rects = rects;
        if let Some(a) = act {
            self.run_hint_action(ui.ctx(), a);
        }
    }

    pub(crate) fn run_hint_action(&mut self, ctx: &egui::Context, a: HintAction) {
        match a {
            HintAction::Run(id) => self.run_command(ctx, id),
            HintAction::Go(v) => self.switch_view(v),
            HintAction::NewDashboard => self.switch_view(View::Dashboard),
        }
    }

    // ------------------------------------------------------------------------------------------ Analytics

    /// The Analytics tab of the Report view (Phase 11): the forecast of the chart being edited, and pointers to what comes later.
    pub(crate) fn render_analytics_tab(&mut self, ui: &mut egui::Ui) {
        let tk = *theme::tokens(self.dark_mode);
        ui.add_space(theme::SPACE_S);
        ui.label(theme::semibold("Analytics").size(theme::TEXT_HEADING));
        for (name, what) in ANALYTICS_PLACEHOLDERS {
            egui::Frame::none().fill(tk.surface).stroke(theme::line(tk.border)).rounding(egui::Rounding::same(theme::RADIUS_PANEL)).inner_margin(egui::Margin::same(theme::SPACE_S)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(theme::semibold(*name).color(tk.muted));
                ui.add(egui::Label::new(egui::RichText::new(*what).color(tk.muted)).wrap());
            });
            ui.add_space(theme::SPACE_XS);
        }
        self.render_forecast_card(ui);
        ui.add_space(theme::SPACE_S);
        self.render_target_card(ui);
        ui.add_space(theme::SPACE_S);
        self.render_whatif_card(ui);
        ui.add_space(theme::SPACE_S);
    }
}

/// The placeholders of the Analytics pane.
pub const ANALYTICS_PLACEHOLDERS: &[(&str, &str)] = &[
    ("Plan vs actual", "Link a budget table in Model, then add a vs-budget measure (Model > Measures); it works in pivots, charts and KPI cards."),
    ("Top movers", "Later: the items that moved most between two dates."),
];
