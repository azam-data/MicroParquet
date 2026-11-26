//! Phase 14: the help assistant, a panel at the right edge of every view (Report, Dashboard, Data, Model). Opened from the rail's
//! "Ask" button, Help ▸ Assistant or F1. It answers from the guide book (`micro_parquet::assistant`), shows the sections it used,
//! never calculates, and writes its answers with a local model only when one has been copied into `portable_data/assistant/`.

use crate::ui::prelude::*;
use micro_parquet::assistant::{self as asst, Answer, AnswerKind, Guide};
use micro_parquet::llm::{self, ModelServer, Setup};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;

/// The questions offered on an empty panel.
pub const EXAMPLES: [&str; 3] = ["How do I run a liquidity gap ladder?", "What does the CL-1 summary do?", "How is the NIM calculated?"];

pub struct Message {
    pub question: String,
    pub answer: Answer,
    /// A model is writing a better answer; the guide's own text is shown meanwhile.
    pub waiting: bool,
}

/// The section being read.
#[derive(Clone, Copy, PartialEq)]
pub struct Reading {
    pub message: usize,
    pub section: usize,
    pub whole_note: bool,
}

pub enum ModelState {
    Off,
    Starting(Receiver<Result<ModelServer, String>>),
    Ready(Arc<ModelServer>),
    Failed(String),
}

impl Default for ModelState {
    fn default() -> Self {
        ModelState::Off
    }
}

#[derive(Default)]
pub struct AssistantUi {
    pub open: bool,
    pub input: String,
    pub focus_input: bool,
    pub messages: Vec<Message>,
    pub reading: Option<Reading>,
    pub model: ModelState,
    pub setup: Option<Setup>,
    pub setup_open: bool,
    pub pending: Option<(usize, Receiver<Answer>)>,
    /// Where the panel's controls were drawn, by name (tests click them).
    pub rects: Vec<(String, egui::Rect)>,
    /// The rail's "Ask" button.
    pub rail_rect: Option<egui::Rect>,
    /// The question to send on the next frame (an example chip or a test).
    pub send_now: Option<String>,
}

fn setup_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.join(llm::ASSISTANT_DIR)
}

impl MicroParquetApp {
    pub(crate) fn toggle_assistant(&mut self) {
        let open = !self.assistant.open;
        self.set_assistant_open(open);
    }

    pub(crate) fn set_assistant_open(&mut self, open: bool) {
        self.assistant.open = open;
        self.assistant.focus_input = open;
        if self.settings.assistant_open != open {
            self.settings.assistant_open = open;
            self.save_settings();
        }
        if open {
            self.assistant_check_setup();
            // a model that was started before is started again
            if self.settings.assistant_autostart && matches!(self.assistant.model, ModelState::Off) {
                self.assistant_start_model();
            }
        }
    }

    pub(crate) fn handle_assistant_shortcut(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F1)) {
            self.toggle_assistant();
        }
    }

    /// Looks in `portable_data/assistant` again.
    pub(crate) fn assistant_check_setup(&mut self) {
        self.assistant.setup = Some(llm::detect(&setup_dir(&self.paths.root_dir)));
    }

    pub(crate) fn assistant_start_model(&mut self) {
        let dir = setup_dir(&self.paths.root_dir);
        let _ = std::fs::create_dir_all(&dir);
        let setup = llm::detect(&dir);
        if !setup.complete() {
            self.assistant.model = ModelState::Failed(setup.status_line());
            self.assistant.setup = Some(setup);
            return;
        }
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(ModelServer::start(&setup).map_err(|e| format!("{:#}", e)));
        });
        self.assistant.model = ModelState::Starting(rx);
        if !self.settings.assistant_autostart {
            self.settings.assistant_autostart = true;
            self.save_settings();
        }
    }

    pub(crate) fn assistant_stop_model(&mut self) {
        self.assistant.model = ModelState::Off;
        if self.settings.assistant_autostart {
            self.settings.assistant_autostart = false;
            self.save_settings();
        }
    }

    /// Asks a question: the guide answers at once; a running model then writes the answer from the sections found.
    pub(crate) fn assistant_ask(&mut self, question: &str) {
        let q = question.trim().to_string();
        if q.is_empty() || self.assistant.pending.is_some() {
            return;
        }
        let guide: &'static Guide = asst::embedded();
        let answer = guide.answer(&q);
        let model = match &self.assistant.model {
            ModelState::Ready(m) => Some(m.clone()),
            _ => None,
        };
        let wants_model = model.is_some() && answer.kind != AnswerKind::NotCalculated;
        let idx = self.assistant.messages.len();
        let sections: Vec<usize> = answer.sources.iter().map(|s| s.section).collect();
        self.assistant.messages.push(Message { question: q.clone(), answer, waiting: wants_model });
        self.assistant.reading = None;
        if let (true, Some(m)) = (wants_model, model) {
            let (tx, rx) = channel();
            let question = q.clone();
            std::thread::spawn(move || {
                let reply = if sections.is_empty() {
                    m.chat(asst::SYSTEM_GENERAL, &question, 300).map(|r| asst::vet_model_reply(guide, &question, &r, &[]))
                } else {
                    let user = asst::grounded_user_message(&guide.model_sources(&sections), &question);
                    m.chat(asst::SYSTEM_GROUNDED, &user, 400).map(|r| asst::vet_model_reply(guide, &question, &r, &sections))
                };
                let a = reply.unwrap_or_else(|e| {
                    let mut a = guide.answer(&question);
                    a.note = Some(format!("The model could not answer ({:#}). This is the guide book's own text.", e));
                    a
                });
                let _ = tx.send(a);
            });
            self.assistant.pending = Some((idx, rx));
        }
    }

    /// Takes what the model thread has finished: the model starting, and an answer written.
    fn assistant_poll(&mut self, ctx: &egui::Context) {
        let mut state = std::mem::take(&mut self.assistant.model);
        if let ModelState::Starting(rx) = &state {
            match rx.try_recv() {
                Ok(Ok(server)) => state = ModelState::Ready(Arc::new(server)),
                Ok(Err(e)) => state = ModelState::Failed(e),
                Err(TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(300)),
                Err(TryRecvError::Disconnected) => state = ModelState::Failed("The model start was interrupted.".to_string()),
            }
        }
        self.assistant.model = state;
        if let Some((idx, rx)) = &self.assistant.pending {
            let idx = *idx;
            match rx.try_recv() {
                Ok(a) => {
                    if let Some(m) = self.assistant.messages.get_mut(idx) {
                        m.answer = a;
                        m.waiting = false;
                    }
                    self.assistant.pending = None;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(300)),
                Err(TryRecvError::Disconnected) => {
                    if let Some(m) = self.assistant.messages.get_mut(idx) {
                        m.waiting = false;
                    }
                    self.assistant.pending = None;
                }
            }
        }
    }

    /// The panel at the right edge. Drawn before the Steps panel, so it is the outermost one.
    pub(crate) fn render_assistant_panel(&mut self, ctx: &egui::Context) {
        self.assistant_poll(ctx);
        self.assistant.rects.clear();
        if !self.assistant.open {
            return;
        }
        if let Some(q) = self.assistant.send_now.take() {
            self.assistant_ask(&q);
        }
        let tk = *theme::tokens(self.dark_mode);
        let mut close = false;
        let mut rects: Vec<(String, egui::Rect)> = Vec::new();
        let mut ask: Option<String> = None;
        egui::SidePanel::right("assistant_panel")
            .default_width(theme::RIGHT_PANEL_W + theme::ASSISTANT_EXTRA_W)
            .min_width(theme::RIGHT_PANEL_W)
            .max_width((ctx.screen_rect().width() * theme::RIGHT_PANEL_MAX_SHARE).max(theme::RIGHT_PANEL_W))
            .resizable(true)
            .frame(egui::Frame::none().fill(tk.surface).inner_margin(egui::Margin::symmetric(theme::SPACE_M, theme::SPACE_S)))
            .show(ctx, |ui| {
                theme::left_border(ui, tk.border);
                // head: title, model state, close
                ui.horizontal(|ui| {
                    ui.label(theme::semibold("Assistant").size(theme::TEXT_HEADING).color(tk.ink));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let r = theme::icon_button(ui, theme::ph::X, "Close the assistant (F1)");
                        rects.push(("Close".to_string(), r.rect));
                        if r.clicked() {
                            close = true;
                        }
                    });
                });
                let (state_text, state_ink) = match &self.assistant.model {
                    ModelState::Ready(_) => ("Local model running on this PC", tk.success),
                    ModelState::Starting(_) => ("Loading the local model\u{2026}", tk.warning),
                    ModelState::Failed(_) => ("Local model not running. The guide book is searched.", tk.warning),
                    ModelState::Off => ("Searching the guide book. Works offline; nothing leaves this PC.", tk.muted),
                };
                ui.label(egui::RichText::new(state_text).size(theme::TEXT_LABEL).color(state_ink));
                ui.add_space(theme::SPACE_XS);
                // the question box sits at the bottom; the conversation takes the room above it
                egui::TopBottomPanel::bottom("assistant_input").frame(egui::Frame::none()).show_separator_line(false).show_inside(ui, |ui| {
                    ui.add_space(theme::SPACE_XS);
                    let busy = self.assistant.pending.is_some();
                    ui.horizontal(|ui| {
                        let w = ui.available_width() - theme::ASSISTANT_ASK_W;
                        let edit = egui::TextEdit::singleline(&mut self.assistant.input).hint_text("Ask about a tool or a task\u{2026}").desired_width(w);
                        let resp = ui.add_enabled(!busy, edit);
                        rects.push(("Input".to_string(), resp.rect));
                        if self.assistant.focus_input {
                            resp.request_focus();
                            self.assistant.focus_input = false;
                        }
                        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let send = ui.add_enabled(!busy && !self.assistant.input.trim().is_empty(), egui::Button::new("Ask"));
                        rects.push(("Ask".to_string(), send.rect));
                        if (enter || send.clicked()) && !self.assistant.input.trim().is_empty() && !busy {
                            ask = Some(std::mem::take(&mut self.assistant.input));
                            self.assistant.focus_input = true;
                        }
                    });
                    self.assistant_setup_section(ui, &tk, &mut rects);
                });
                egui::ScrollArea::vertical().id_salt("assistant_scroll").auto_shrink([false, false]).stick_to_bottom(self.assistant.reading.is_none()).show(ui, |ui| {
                    self.assistant_conversation(ui, &tk, &mut rects, &mut ask);
                });
            });
        self.assistant.rects = rects;
        if let Some(q) = ask {
            self.assistant_ask(&q);
        }
        if close {
            self.set_assistant_open(false);
        }
    }

    fn assistant_conversation(&mut self, ui: &mut egui::Ui, tk: &theme::Tokens, rects: &mut Vec<(String, egui::Rect)>, ask: &mut Option<String>) {
        if self.assistant.messages.is_empty() {
            ui.add_space(theme::SPACE_S);
            ui.label(egui::RichText::new("Ask how a tool works, which columns it needs, or what a message means. Answers come from the guide book and name the section they used.").color(tk.ink2));
            ui.add_space(theme::SPACE_XS);
            ui.label(egui::RichText::new("I do not calculate numbers: the app's own functions do that, from your data.").color(tk.muted).size(theme::TEXT_SMALL));
            ui.add_space(theme::SPACE_M);
            for (i, ex) in EXAMPLES.iter().enumerate() {
                let r = ui.add(egui::Button::new(egui::RichText::new(*ex).color(tk.accent)).fill(tk.accent_soft).stroke(egui::Stroke::new(1.0, tk.accent_soft_border)).wrap());
                rects.push((format!("Example {}", i + 1), r.rect));
                if r.clicked() {
                    *ask = Some((*ex).to_string());
                }
            }
            return;
        }
        let guide = asst::embedded();
        let mut reading = self.assistant.reading;
        let mut find_tool: Option<String> = None;
        for (mi, m) in self.assistant.messages.iter().enumerate() {
            ui.add_space(theme::SPACE_S);
            egui::Frame::none().fill(tk.accent_soft).rounding(egui::Rounding::same(theme::RADIUS_PANEL)).inner_margin(egui::Margin::symmetric(theme::SPACE_S, theme::SPACE_XS)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new(&m.question).color(tk.ink));
            });
            ui.add_space(theme::SPACE_XS);
            let (tag, tag_ink) = match m.answer.kind {
                AnswerKind::Guide => ("From the guide book", tk.muted),
                AnswerKind::Model => ("Written by the local model from the guide book", tk.muted),
                AnswerKind::ModelGeneral => ("General explanation by the local model. Not from the guide book: check it against your own policy.", tk.warning),
                AnswerKind::NotCalculated => ("Not calculated", tk.warning),
                AnswerKind::NotCovered => ("Not covered", tk.warning),
            };
            ui.label(egui::RichText::new(tag).size(theme::TEXT_LABEL).color(tag_ink));
            ui.add(egui::Label::new(egui::RichText::new(asst::display_text(&m.answer.text)).color(tk.ink)).selectable(true));
            if m.waiting {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(egui::RichText::new("The local model is writing a clearer answer\u{2026}").size(theme::TEXT_LABEL).color(tk.muted));
                });
            }
            if let Some(n) = &m.answer.note {
                ui.label(egui::RichText::new(n).size(theme::TEXT_SMALL).color(tk.warning));
            }
            if !m.answer.sources.is_empty() {
                ui.add_space(theme::SPACE_XS);
                ui.label(egui::RichText::new("Sections used").size(theme::TEXT_LABEL).color(tk.muted));
                for (si, s) in m.answer.sources.iter().enumerate() {
                    let on = reading.is_some_and(|r| r.message == mi && r.section == s.section);
                    let r = ui.add(egui::Button::new(egui::RichText::new(format!("{} {}", if on { "\u{25be}" } else { "\u{25b8}" }, s.label)).color(tk.accent)).frame(false).wrap());
                    if mi + 1 == self.assistant.messages.len() {
                        rects.push((format!("Source {}", si + 1), r.rect));
                    }
                    if r.on_hover_text(format!("{} in the guide book", s.file)).clicked() {
                        reading = if on { None } else { Some(Reading { message: mi, section: s.section, whole_note: false }) };
                    }
                    if on {
                        if let Some(sec) = guide.section(s.section) {
                            egui::Frame::none().fill(tk.bg).rounding(egui::Rounding::same(theme::RADIUS_PANEL)).inner_margin(egui::Margin::same(theme::SPACE_S)).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                let whole = reading.is_some_and(|r| r.whole_note);
                                ui.horizontal(|ui| {
                                    if ui.small_button(if whole { "Only this section" } else { "Whole note" }).clicked() {
                                        reading = Some(Reading { message: mi, section: s.section, whole_note: !whole });
                                    }
                                    if ui.small_button("Find the tool (Ctrl+K)").clicked() {
                                        find_tool = Some(sec.doc_title.clone());
                                    }
                                });
                                let ids: Vec<usize> = if whole { guide.note_sections(&sec.file) } else { vec![s.section] };
                                for id in ids {
                                    if let Some(part) = guide.section(id) {
                                        if !part.heading.is_empty() {
                                            ui.label(theme::semibold(&part.heading).color(tk.ink));
                                        }
                                        render_section_text(ui, &part.body, tk);
                                        ui.add_space(theme::SPACE_XS);
                                    }
                                }
                            });
                        }
                    }
                }
            }
            ui.separator();
        }
        self.assistant.reading = reading;
        if let Some(t) = find_tool {
            self.open_command_palette();
            self.command_query = t;
        }
    }

    fn assistant_setup_section(&mut self, ui: &mut egui::Ui, tk: &theme::Tokens, rects: &mut Vec<(String, egui::Rect)>) {
        let dir = setup_dir(&self.paths.root_dir);
        let head = egui::CollapsingHeader::new(egui::RichText::new("Local model (optional)").size(theme::TEXT_SMALL).color(tk.muted)).id_salt("assistant_setup").open(Some(self.assistant.setup_open)).show(ui, |ui| {
            if self.assistant.setup.is_none() {
                self.assistant.setup = Some(llm::detect(&dir));
            }
            let setup = self.assistant.setup.clone().unwrap_or_default();
            ui.label(egui::RichText::new(setup.status_line()).size(theme::TEXT_SMALL).color(tk.ink2));
            for n in &setup.notes {
                ui.label(egui::RichText::new(n).size(theme::TEXT_SMALL).color(tk.warning));
            }
            if let ModelState::Failed(e) = &self.assistant.model {
                ui.label(egui::RichText::new(e).size(theme::TEXT_SMALL).color(tk.danger));
            }
            ui.label(egui::RichText::new(format!("Folder: {}", dir.display())).size(theme::TEXT_SMALL).color(tk.muted));
            ui.label(egui::RichText::new("Once, from a USB stick: copy a model file (.gguf, 1 to 3 GB) and the unzipped llama.cpp program folder (it holds llama-server.exe) into this folder. See Help ▸ Assistant set-up in the guide book. The model runs on this PC only.").size(theme::TEXT_SMALL).color(tk.muted));
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("Check again").clicked() {
                    let _ = std::fs::create_dir_all(&dir);
                    self.assistant_check_setup();
                }
                if ui.small_button("Copy folder path").clicked() {
                    ui.ctx().copy_text(dir.display().to_string());
                }
                if ui.small_button("Open folder").clicked() {
                    let _ = std::fs::create_dir_all(&dir);
                    open_folder(&dir);
                }
                match &self.assistant.model {
                    ModelState::Off | ModelState::Failed(_) => {
                        let r = ui.add_enabled(setup.complete(), egui::Button::new("Start the model"));
                        rects.push(("Start model".to_string(), r.rect));
                        if r.clicked() {
                            self.assistant_start_model();
                        }
                    }
                    _ => {
                        let r = ui.small_button("Stop the model");
                        rects.push(("Stop model".to_string(), r.rect));
                        if r.clicked() {
                            self.assistant_stop_model();
                        }
                    }
                }
            });
        });
        rects.push(("Setup header".to_string(), head.header_response.rect));
        if head.header_response.clicked() {
            self.assistant.setup_open = !self.assistant.setup_open;
        }
    }

    /// The rail's "Ask" button, at the foot of the rail.
    pub(crate) fn render_rail_assistant_button(&mut self, ui: &mut egui::Ui) {
        let tk = *theme::tokens(self.dark_mode);
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            let on = self.assistant.open;
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(theme::RAIL_BTN_W, theme::RAIL_BTN_H), egui::Sense::click());
            let fill = if on { tk.accent_soft } else if resp.hovered() { tk.zebra } else { egui::Color32::TRANSPARENT };
            ui.painter().rect_filled(rect, theme::RADIUS_PANEL, fill);
            let ink = if on { tk.accent } else { tk.muted };
            ui.painter().text(egui::pos2(rect.center().x, rect.top() + theme::SPACE_L), egui::Align2::CENTER_CENTER, theme::ph::CHAT_CIRCLE_TEXT, egui::FontId::proportional(theme::TEXT_RAIL_ICON), ink);
            let label = if on { theme::semibold("Ask") } else { egui::RichText::new("Ask") };
            let galley = egui::WidgetText::from(label.size(theme::TEXT_LABEL).color(ink)).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body);
            ui.painter().galley(egui::pos2(rect.center().x - galley.size().x * theme::HALF, rect.bottom() - galley.size().y - theme::SPACE_XS), galley, ink);
            self.assistant.rail_rect = Some(rect);
            if resp.on_hover_text("Ask the assistant (F1): answers from the guide book, offline").clicked() {
                self.toggle_assistant();
            }
        });
    }
}

/// A section's text in the reading pane: bullets, small monospace for tables and code, plain words otherwise.
fn render_section_text(ui: &mut egui::Ui, body: &str, tk: &theme::Tokens) {
    let text = asst::display_text(body);
    let mut fenced = false;
    for raw in body.lines() {
        if raw.trim_start().starts_with("```") {
            fenced = !fenced;
        }
    }
    let _ = fenced;
    for line in text.lines() {
        let t = line.trim_end();
        if t.is_empty() {
            ui.add_space(theme::SPACE_XS);
        } else if t.starts_with('|') || t.starts_with("    ") {
            ui.label(egui::RichText::new(t).monospace().size(theme::TEXT_LABEL).color(tk.ink2));
        } else if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            ui.label(egui::RichText::new(format!("\u{2022} {}", rest)).size(theme::TEXT_SMALL).color(tk.ink));
        } else if t.starts_with("### ") {
            ui.label(theme::semibold(t.trim_start_matches('#').trim()).size(theme::TEXT_SMALL).color(tk.ink));
        } else {
            ui.label(egui::RichText::new(t).size(theme::TEXT_SMALL).color(tk.ink));
        }
    }
}

/// Opens a folder in the PC's file manager.
fn open_folder(dir: &std::path::Path) {
    #[cfg(windows)]
    let prog = "explorer";
    #[cfg(target_os = "macos")]
    let prog = "open";
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let prog = "xdg-open";
    let _ = std::process::Command::new(prog).arg(dir).spawn();
}
