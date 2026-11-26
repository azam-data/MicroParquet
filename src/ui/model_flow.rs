//! The model's set-up as one step-by-step window (UI revamp): Tables, Links, Key check, Calendar, As-of date, Measures. Each step is the
//! dialog it always was; they share one title (so one window position) and a strip of steps on top that says what is done and what is
//! still to do, with Back and Next at the bottom. Nothing about a link, a key check or a measure changed.

use crate::ui::prelude::*;
use micro_parquet::model::LinkStatus;
use micro_parquet::workspace::{self as ws, ModelProgress, ModelStep, StepInfo, StepState};

/// The title every step of the set-up has, so the window stays where it is when the step changes.
pub const MODEL_SETUP_TITLE: &str = "Model set-up";

impl MicroParquetApp {
    /// How far the set-up has got (from the open tabs and the project's model).
    pub(crate) fn model_progress(&self) -> ModelProgress {
        let m = &self.project.model;
        let links = &m.links;
        let mut p = ModelProgress {
            tables: self.tabs.iter().filter(|t| !self.chart_tabs.contains_key(&t.id) && !matches!(t.source, TabSource::Unavailable { .. })).count(),
            links: links.len(),
            calendar: m.calendar.is_some(),
            as_of_chosen: m.as_of != micro_parquet::model::AsOf::Latest,
            measures: m.measures.len(),
            ..Default::default()
        };
        for l in links {
            let runtime = self.project.model_ui.runtime.get(&(l.data_tab.clone(), l.lookup_tab.clone()));
            if runtime.is_some_and(|r| !matches!(r.status, LinkStatus::Checking)) {
                p.checked += 1;
            }
            match self.model_status_of(l) {
                s if s.is_broken() => p.broken += 1,
                LinkStatus::OkUnmatched(_) => p.unmatched += 1,
                _ => {}
            }
        }
        p
    }

    pub(crate) fn model_step_infos(&self) -> Vec<StepInfo> {
        ws::model_steps(&self.model_progress())
    }

    fn model_step_tool(step: ModelStep) -> ActiveToolModal {
        match step {
            ModelStep::Tables => ActiveToolModal::ModelTables,
            ModelStep::Links => ActiveToolModal::ModelLinks,
            ModelStep::KeyCheck => ActiveToolModal::ModelKeyCheck,
            ModelStep::Calendar => ActiveToolModal::ModelCalendar,
            ModelStep::AsOf => ActiveToolModal::ModelAsOf,
            ModelStep::Measures => ActiveToolModal::ModelMeasures,
        }
    }

    /// Opens the set-up at a step (none: the first that still needs something). Without a project it says so, as the dialogs always did.
    pub(crate) fn open_model_flow(&mut self, step: Option<ModelStep>) {
        if !self.project.active {
            self.active_tool = ActiveToolModal::ModelNoProject;
            return;
        }
        let step = step.unwrap_or_else(|| ws::first_open_step(&self.model_step_infos()));
        self.project.model_ui.draft = None;
        self.active_tool = Self::model_step_tool(step);
    }

    /// The strip of steps above a step's dialog: done steps with a tick, a step with a problem with a warning, the step you are on in bold.
    pub(crate) fn model_flow_header(&mut self, ui: &mut egui::Ui, current: ModelStep) {
        let tk = *theme::tokens(self.dark_mode);
        let infos = self.model_step_infos();
        let mut rects: Vec<(String, egui::Rect)> = Vec::new();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = theme::STEPPER_GAP;
            for info in &infos {
                let on = info.step == current;
                let (mark, colour) = match info.state {
                    StepState::Done => (theme::ph::CHECK, tk.success),
                    StepState::Problem => (theme::ph::WARNING, tk.warning),
                    StepState::Todo => (theme::ph::CIRCLE, tk.muted),
                    StepState::Optional => (theme::ph::MINUS, tk.muted),
                };
                let text = format!("{} {}", mark, info.step.label());
                let rich = if on { theme::semibold(text).color(tk.accent) } else { egui::RichText::new(text).color(colour) };
                let b = ui.add(egui::Button::new(rich).fill(if on { tk.accent_soft } else { egui::Color32::TRANSPARENT }).stroke(egui::Stroke::NONE)).on_hover_text(info.note.clone());
                rects.push((info.step.label().to_string(), b.rect));
                if b.clicked() && !on {
                    self.ws.flow_goto = Some(info.step);
                }
            }
        });
        ui.add_space(theme::SPACE_XS);
        self.ws.flow_rects = rects;
    }

    /// Back and Next at the end of a step's buttons.
    pub(crate) fn model_flow_footer(&mut self, ui: &mut egui::Ui, current: ModelStep) {
        let mut rects: Vec<(String, egui::Rect)> = Vec::new();
        if let Some(prev) = current.previous() {
            let b = ui.button(format!("\u{2039} Back: {}", prev.label()));
            rects.push(("Back".to_string(), b.rect));
            if b.clicked() {
                self.ws.flow_goto = Some(prev);
            }
        }
        if let Some(next) = current.next() {
            let b = theme::primary_button(ui, format!("Next: {} \u{203a}", next.label()));
            rects.push(("Next".to_string(), b.rect));
            if b.clicked() {
                self.ws.flow_goto = Some(next);
            }
        }
        self.ws.flow_rects.extend(rects);
    }

    /// The step the owner asked for with the strip or Back / Next (applied once the dialog is drawn).
    pub(crate) fn apply_flow_goto(&mut self) {
        if let Some(step) = self.ws.flow_goto.take() {
            self.open_model_flow(Some(step));
        }
    }

    /// Step 1: the tables the model works on.
    pub(crate) fn render_model_tables_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut close = false;
        let mut open_file = false;
        let rows: Vec<(String, usize, bool)> = self
            .tabs
            .iter()
            .filter(|t| !self.chart_tabs.contains_key(&t.id) && !matches!(t.source, TabSource::Unavailable { .. }))
            .map(|t| {
                let key = self.project.keys.get(&t.id).cloned();
                let linked = key.as_ref().is_some_and(|k| self.project.model.links.iter().any(|l| &l.data_tab == k || &l.lookup_tab == k));
                (t.title.clone(), t.total_rows, linked)
            })
            .collect();
        theme::modal(MODEL_SETUP_TITLE).default_width(theme::DIALOG_W_MODEL_FLOW).show(ctx, |d| {
            d.body(|ui| {
                self.model_flow_header(ui, ModelStep::Tables);
                ui.add(egui::Label::new(egui::RichText::new("The model links tables that are open as tabs: the data (trial balance, account snapshots) and the lookups (branch master, product master, GL map, customer master, rates).").color(tk.muted)).wrap());
                ui.add_space(theme::SPACE_S);
                if rows.is_empty() {
                    ui.label(egui::RichText::new("No table is open yet. Open the data and the lookup files first.").color(tk.muted));
                } else {
                    egui::Grid::new("model_flow_tables").striped(true).num_columns(3).spacing([theme::SPACE_M, theme::SPACE_XS]).show(ui, |ui| {
                        for h in ["Table", "Rows", "In a link"] {
                            ui.label(theme::bold(h).color(tk.ink));
                        }
                        ui.end_row();
                        for (title, n, linked) in &rows {
                            ui.label(title);
                            ui.label(format_with_commas(*n as f64, 0));
                            ui.label(if *linked { "Yes" } else { "Not yet" });
                            ui.end_row();
                        }
                    });
                }
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let o = ui.button("Open a file\u{2026}");
                    open_file = o.clicked();
                    let c = ui.button("Close");
                    close = c.clicked();
                    self.model_flow_footer(ui, ModelStep::Tables);
                });
            });
        });
        if open_file {
            self.active_tool = ActiveToolModal::None;
            self.switch_view(micro_parquet::workspace::View::Data);
            self.initiate_file_open(false);
        }
        if close {
            self.active_tool = ActiveToolModal::None;
        }
        ModalOutcome { close: false, step: None }
    }
}
