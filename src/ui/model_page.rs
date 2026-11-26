//! The Model view (UI revamp): the project's relationships as a diagram, how far the set-up has got, and the as-of date, calendar and
//! measures at a glance. Each part opens the step of the set-up that changes it. Nothing is calculated here.

use crate::ui::prelude::*;
use micro_parquet::model::LinkStatus;
use micro_parquet::workspace::{self as ws, DiagramLink, ModelStep, StepState};

impl MicroParquetApp {
    /// The links as the diagram needs them: titles of the two tabs and what the key check says.
    fn diagram_links(&self) -> Vec<DiagramLink> {
        self.project
            .model
            .links
            .iter()
            .map(|l| {
                let st = self.model_status_of(l);
                let bad = st.is_broken() || matches!(st, LinkStatus::OkUnmatched(_) | LinkStatus::Failed(_));
                let status = match &st {
                    LinkStatus::Ok => "Matched".to_string(),
                    LinkStatus::Checking => "Not checked".to_string(),
                    other => other.text(),
                };
                DiagramLink { data_key: l.data_tab.clone(), data_title: self.model_tab_title(&l.data_tab), lookup_key: l.lookup_tab.clone(), lookup_title: self.model_tab_title(&l.lookup_tab), status, bad }
            })
            .collect()
    }

    /// What the Model view draws.
    pub(crate) fn render_model_page(&mut self, ui: &mut egui::Ui) {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects: Vec<(String, egui::Rect)> = Vec::new();
        let mut open_step: Option<Option<ModelStep>> = None;
        let m = &self.project.model;
        let empty = m.links.is_empty() && m.calendar.is_none() && m.measures.is_empty();
        if !self.project.active || empty {
            // nothing set up: say what to do next (a model belongs to a project)
            let hint = if self.project.active {
                ws::empty_hint(micro_parquet::workspace::View::Model, ws::HintState { links: 0, ..Default::default() })
            } else {
                None
            };
            match hint {
                Some(h) => self.render_hint(ui, &h),
                None => {
                    ui.add_space(ui.available_height() * theme::HINT_TOP_SHARE);
                    ui.vertical_centered(|ui| {
                        ui.label(theme::semibold("The model needs a project").size(theme::TEXT_HINT_TITLE));
                        ui.add(egui::Label::new(egui::RichText::new(micro_parquet::model::NO_PROJECT_MESSAGE).color(tk.muted)).wrap());
                        ui.add_space(theme::SPACE_S);
                        let b = theme::primary_button(ui, "Save project as\u{2026}");
                        rects.push(("Save project as".to_string(), b.rect));
                        if b.clicked() {
                            self.project_save_as(None);
                        }
                    });
                }
            }
            self.ws.model_rects = rects;
            return;
        }
        // header
        ui.horizontal(|ui| {
            ui.label(theme::semibold(format!("{}  Model", theme::ph::TREE_STRUCTURE)).size(theme::TEXT_HEADING));
            ui.label(egui::RichText::new(if self.project.name.is_empty() { "this project".to_string() } else { self.project.name.clone() }).color(tk.muted));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let b = theme::primary_button(ui, "Set up the model\u{2026}");
                rects.push(("Set up".to_string(), b.rect));
                if b.clicked() {
                    open_step = Some(None);
                }
            });
        });
        ui.add_space(theme::SPACE_S);
        // the steps: what is done, what is wrong
        let infos = self.model_step_infos();
        ui.horizontal_wrapped(|ui| {
            for info in &infos {
                let (mark, colour) = match info.state {
                    StepState::Done => (theme::ph::CHECK_CIRCLE, tk.success),
                    StepState::Problem => (theme::ph::WARNING, tk.warning),
                    StepState::Todo => (theme::ph::CIRCLE, tk.muted),
                    StepState::Optional => (theme::ph::MINUS_CIRCLE, tk.muted),
                };
                let b = ui.add(egui::Button::new(egui::RichText::new(format!("{} {}", mark, info.step.label())).color(colour)).fill(tk.surface).stroke(theme::line(tk.border))).on_hover_text(info.note.clone());
                rects.push((format!("Step {}", info.step.label()), b.rect));
                if b.clicked() {
                    open_step = Some(Some(info.step));
                }
            }
        });
        ui.add_space(theme::SPACE_S);
        // the diagram
        let calendar = self.project.model.calendar.as_ref().filter(|c| !c.tab.is_empty()).map(|c| (c.tab.clone(), c.date_links.iter().filter(|l| !l.data_tab.is_empty()).map(|l| (l.data_tab.clone(), self.model_tab_title(&l.data_tab))).collect::<Vec<_>>()));
        let links = self.diagram_links();
        let diagram = ws::layout_diagram(&links, calendar.as_ref().map(|(k, _)| (k.as_str(), "Calendar")), &calendar.as_ref().map(|(_, d)| d.clone()).unwrap_or_default());
        if diagram.nodes.is_empty() {
            ui.label(egui::RichText::new("No tables are linked yet. The Links step joins a data table to a lookup.").color(tk.muted));
        } else {
            let w = diagram.columns as f32 * theme::DIAGRAM_NODE_W + diagram.columns.saturating_sub(1) as f32 * theme::DIAGRAM_COL_GAP + 2.0 * theme::DIAGRAM_PAD;
            let h = diagram.rows as f32 * theme::DIAGRAM_NODE_H + diagram.rows.saturating_sub(1) as f32 * theme::DIAGRAM_ROW_GAP + 2.0 * theme::DIAGRAM_PAD;
            egui::Frame::none().fill(tk.surface).stroke(theme::line(tk.border)).rounding(egui::Rounding::same(theme::RADIUS_PANEL)).show(ui, |ui| {
                egui::ScrollArea::both().id_salt("model_diagram").max_height(ui.available_height() * theme::MODEL_DIAGRAM_SHARE).auto_shrink([false, true]).show(ui, |ui| {
                    let (area, _) = ui.allocate_exact_size(egui::vec2(w.max(ui.available_width()), h), egui::Sense::hover());
                    let pos = |col: usize, row: usize| {
                        egui::pos2(area.left() + theme::DIAGRAM_PAD + col as f32 * (theme::DIAGRAM_NODE_W + theme::DIAGRAM_COL_GAP), area.top() + theme::DIAGRAM_PAD + row as f32 * (theme::DIAGRAM_NODE_H + theme::DIAGRAM_ROW_GAP))
                    };
                    let node_rect = |key: &str| diagram.nodes.iter().find(|n| n.key == key).map(|n| egui::Rect::from_min_size(pos(n.column, n.row), egui::vec2(theme::DIAGRAM_NODE_W, theme::DIAGRAM_NODE_H)));
                    let painter = ui.painter();
                    // the links first, so the boxes are on top of them
                    let mut pills: Vec<(egui::Pos2, String, bool)> = Vec::new();
                    for e in &diagram.edges {
                        if let (Some(a), Some(b)) = (node_rect(&e.from), node_rect(&e.to)) {
                            let (p, q) = (a.right_center(), b.left_center());
                            let colour = if e.bad { tk.warning } else { tk.border };
                            painter.line_segment([p, q], egui::Stroke::new(theme::DIAGRAM_EDGE_W, colour));
                            pills.push((egui::pos2((p.x + q.x) * theme::HALF, (p.y + q.y) * theme::HALF), e.status.clone(), e.bad));
                        }
                    }
                    for n in &diagram.nodes {
                        let r = egui::Rect::from_min_size(pos(n.column, n.row), egui::vec2(theme::DIAGRAM_NODE_W, theme::DIAGRAM_NODE_H));
                        painter.rect(r, theme::RADIUS_CONTROL, tk.bg, theme::line(tk.border));
                        let galley = painter.layout(n.title.clone(), egui::FontId::proportional(theme::TEXT_BASE), tk.ink, theme::DIAGRAM_NODE_W - 2.0 * theme::SPACE_S);
                        painter.galley(egui::pos2(r.left() + theme::SPACE_S, r.center().y - galley.size().y * theme::HALF), galley, tk.ink);
                        rects.push((format!("Node {}", n.title), r));
                    }
                    for (c, text, bad) in pills {
                        let galley = painter.layout_no_wrap(text.clone(), egui::FontId::proportional(theme::TEXT_LABEL), if bad { tk.warning } else { tk.muted });
                        let r = egui::Rect::from_center_size(c, galley.size() + egui::vec2(2.0 * theme::SPACE_S, theme::SPACE_XS));
                        painter.rect(r, theme::RADIUS_PILL, tk.surface, theme::line(if bad { tk.warning } else { tk.border }));
                        painter.galley(r.center() - galley.size() * theme::HALF, galley, tk.ink);
                        rects.push((format!("Link {}", text), r));
                    }
                });
            });
        }
        ui.add_space(theme::SPACE_S);
        // the as-of date, the calendar and the measures at a glance
        let as_of = self.model_as_of_text().unwrap_or_else(|| "Needs the calendar".to_string());
        let cal = match &self.project.model.calendar {
            Some(c) => format!("One calendar for {} {}. {}", c.date_links.len(), if c.date_links.len() == 1 { "table" } else { "tables" }, c.settings.status_line()),
            None => "Not set up".to_string(),
        };
        let measures = {
            let names: Vec<String> = self.project.model.measures.iter().map(|x| x.name().to_string()).collect();
            if names.is_empty() {
                "None yet".to_string()
            } else {
                names.join(", ")
            }
        };
        ui.horizontal_top(|ui| {
            for (title, text, step) in [("As-of date", as_of, ModelStep::AsOf), ("Calendar", cal, ModelStep::Calendar), ("Measures", measures, ModelStep::Measures)] {
                egui::Frame::none().fill(tk.surface).stroke(theme::line(tk.border)).rounding(egui::Rounding::same(theme::RADIUS_PANEL)).inner_margin(egui::Margin::same(theme::SPACE_M)).show(ui, |ui| {
                    ui.set_width(theme::HINT_CARD_W);
                    ui.vertical(|ui| {
                        ui.label(theme::semibold(title).color(tk.ink));
                        ui.add(egui::Label::new(egui::RichText::new(text).color(tk.muted)).wrap());
                        let b = ui.button("Change\u{2026}");
                        rects.push((format!("Change {title}"), b.rect));
                        if b.clicked() {
                            open_step = Some(Some(step));
                        }
                    });
                });
            }
        });
        self.ws.model_rects = rects;
        if let Some(step) = open_step {
            self.open_model_flow(step);
        }
    }
}
