//! Screen module `steps` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct MacroState {
    pub name: String,
    pub desc: String,
    pub enforce_ctrl: bool,
    pub include_schema: bool,
    pub python_code: String,
    pub python_warning: String,
}

impl Default for MacroState {
    fn default() -> Self {
        Self {
            name: "Monthly_WB_ALCO_Pipeline".to_string(),
            desc: "Sanitizes dump, buckets maturities, and computes WAVG/Duration."
                        .to_string(),
            enforce_ctrl: true,
            include_schema: true,
            python_code: String::new(),
            python_warning: String::new(),
        }
    }
}

/// What the head of one row of the steps list shows.
pub(crate) struct StepRowSpec<'a> {
    pub title: &'a str,
    pub struck: bool,
    pub title_color: egui::Color32,
    /// A status word ("Off", "Error", "Not run") and its colour.
    pub tag: Option<(&'a str, egui::Color32)>,
    /// The row count ("1,234,567", or "…" while it is being counted).
    pub count: &'a str,
    pub detail: &'a str,
    pub detail_color: egui::Color32,
}

/// Where the parts of a step row were drawn.
#[derive(Clone, Copy, Debug)]
pub struct StepRowRects {
    pub title: egui::Rect,
    /// The tag, where it was drawn.
    pub tag: Option<egui::Rect>,
    /// The tag is on the title line (it fits there); otherwise it starts the detail line.
    pub tag_on_title: bool,
    pub count: egui::Rect,
    pub detail: egui::Rect,
    /// The title and the detail line were cut with "…".
    pub title_cut: bool,
    pub detail_cut: bool,
}

/// The steps panel as drawn in the last frame.
#[derive(Clone, Debug)]
pub struct StepsReport {
    /// What the panel leaves inside its margins: nothing may reach past its right edge.
    pub inner: egui::Rect,
    pub rows: Vec<StepRowRects>,
    pub cards: Vec<egui::Rect>,
    /// The button rows of the selected step.
    pub buttons: Vec<egui::Rect>,
}

pub fn steps_report(ctx: &egui::Context) -> Option<StepsReport> {
    ctx.data(|d| d.get_temp::<StepsReport>(egui::Id::new("steps_report")))
}

/// The title line (title, status tag, row count) and the detail line of a step row. The count
/// is at the right edge. The tag is on the title line only if title, tag and count all fit; if
/// not, it moves to the front of the detail line. A title or detail that is still too long ends
/// in "…" and shows its whole text on hover. Nothing passes the width `ui` has.
pub(crate) fn step_row_head(ui: &mut egui::Ui, s: &StepRowSpec) -> StepRowRects {
    let tk = *theme::tokens(ui.visuals().dark_mode);
    let w = ui.available_width();
    let gap = ui.spacing().item_spacing.x;
    let width_of = |ui: &egui::Ui, rt: egui::RichText| {
        egui::WidgetText::from(rt).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body).size().x
    };
    let mut title_rt = theme::bold(s.title).color(s.title_color);
    if s.struck {
        title_rt = title_rt.strikethrough();
    }
    let count_rt = theme::mono(s.count).size(theme::TEXT_COUNT).color(tk.muted);
    let tag_rt = s.tag.map(|(t, c)| theme::semibold(t).size(theme::TEXT_SMALL).color(c));
    let (title_w, count_w) = (width_of(ui, title_rt.clone()), width_of(ui, count_rt.clone()));
    let tag_w = tag_rt.as_ref().map_or(0.0, |t| width_of(ui, t.clone()));
    let tag_on_title = tag_rt.is_some() && title_w + gap + tag_w + gap + count_w <= w;
    let title_max = (w - count_w - gap - if tag_on_title { tag_w + gap } else { 0.0 }).max(0.0);
    let mut rects = StepRowRects {
        title: egui::Rect::NOTHING,
        tag: None,
        tag_on_title,
        count: egui::Rect::NOTHING,
        detail: egui::Rect::NOTHING,
        title_cut: title_w > title_max + 0.5,
        detail_cut: false,
    };
    ui.horizontal(|ui| {
        let r = ui
            .scope(|ui| {
                ui.set_max_width(title_max);
                ui.add(egui::Label::new(title_rt).truncate())
            })
            .inner;
        rects.title = if rects.title_cut { r.clone().on_hover_text(s.title).rect } else { r.rect };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            rects.count = ui.label(count_rt).rect;
            if let (true, Some(t)) = (tag_on_title, tag_rt.clone()) {
                rects.tag = Some(ui.label(t).rect);
            }
        });
    });
    if !s.detail.is_empty() || (tag_rt.is_some() && !tag_on_title) {
        let detail_rt = egui::RichText::new(s.detail).size(theme::TEXT_SMALL).color(s.detail_color);
        ui.horizontal(|ui| {
            if let (false, Some(t)) = (tag_on_title, tag_rt) {
                rects.tag = Some(ui.label(t).rect);
            }
            let (avail, dw) = (ui.available_width(), width_of(ui, detail_rt.clone()));
            let r = ui.add(egui::Label::new(detail_rt).truncate());
            rects.detail_cut = dw > avail + 0.5;
            rects.detail = if rects.detail_cut { r.on_hover_text(s.detail).rect } else { r.rect };
        });
    }
    rects
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_save_macro_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let step_to_apply: Option<(TransformStep, bool)> = None;
        theme::modal("\u{E248} Save Portable Macro Recipe (.mpq.json)")
            .collapsible(false)
            .show(ctx, |d| {
                d.body(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("Macro Name:");
                        ui.text_edit_singleline(&mut self.macros.name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Description:");
                        ui.text_edit_singleline(&mut self.macros.desc);
                    });
                    ui.checkbox(
                        &mut self.macros.enforce_ctrl,
                        "Enforce Control Column Reconciliation Check during replay",
                    );
                    ui.checkbox(
                        &mut self.macros.include_schema,
                        "Embed CSV Ingestion Schema Rules inside Macro",
                    );
                });
                d.footer(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("\u{E248} Save .mpq.json File...").clicked() {
                            if let Some(tab) = self.tabs.get(self.active_tab) {
                                let opt_schema = if self.macros.include_schema {
                                    match &tab.source {
                                        TabSource::SingleFile { config, .. } => {
                                            Some(config.clone())
                                        }
                                        _ => None,
                                    }
                                } else {
                                    None
                                };
                                let recipe = MacroRecipe {
                                    name: self.macros.name.clone(),
                                    description: self.macros.desc.clone(),
                                    created_at: chrono::Local::now()
                                        .format("%Y-%m-%d %H:%M")
                                        .to_string(),
                                    enforce_balance_control: self.macros.enforce_ctrl,
                                    control_column: tab.control_col.clone(),
                                    optional_ingestion_schema: opt_schema,
                                    steps: tab.steps.clone(),
                                    summary: tab.summary.clone(),
                                };
                                if let Some(save_path) = rfd::FileDialog::new()
                                    .set_directory(&self.paths.macros_dir)
                                    .set_file_name(micro_parquet::paths::default_export_file_name(&self.macros.name, "mpq.json"))
                                    .save_file()
                                {
                                    match save_macro_to_file(&recipe, &save_path) {
                                        Ok(_) => {
                                            self.status_message =
                                                format!("Saved macro to {:?}", save_path);
                                        }
                                        Err(e) => {
                                            self.status_message =
                                                format!("Failed to save macro: {}", e);
                                        }
                                    }
                                }
                            }
                            close_modal = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_modal = true;
                        }
                    });
                });
            });
        ModalOutcome { close: close_modal, step: step_to_apply }
    }

    #[allow(unused_variables)]
    pub(crate) fn render_python_preview_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let step_to_apply: Option<(TransformStep, bool)> = None;
        theme::modal("\u{EB2C} Export GUI Pipeline as Python Polars Jupyter Code")
            .collapsible(false)
            .default_width(theme::DIALOG_W_PYTHON)
            .show(ctx, |d| {
                d.body(|ui| {
                    egui::ScrollArea::vertical()
                        .max_height(theme::PYTHON_PREVIEW_MAX_H)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.macros.python_code)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(18),
                            );
                        });
                });
                d.footer(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("\u{E198} Copy Python Code to Clipboard").clicked() {
                            ctx.copy_text(self.macros.python_code.clone());
                            self.status_message = format!(
                                "Copied Python Polars code to clipboard!{}",
                                self.macros.python_warning
                            );
                        }
                        if ui.button("\u{E248} Save as .py Script...").clicked() {
                            if let Some(p) = rfd::FileDialog::new()
                                .set_file_name("micro_parquet_pipeline.py")
                                .save_file()
                            {
                                self.status_message =
                                    match std::fs::write(&p, &self.macros.python_code) {
                                        Ok(()) => format!(
                                            "Saved Python script to {:?}{}",
                                            p, self.macros.python_warning
                                        ),
                                        Err(e) => format!(
                                            "Could not save Python script to {:?}: {}",
                                            p, e
                                        ),
                                    };
                            }
                        }
                        if ui.button("Close").clicked() {
                            close_modal = true;
                        }
                    });
                });
            });
        ModalOutcome { close: close_modal, step: step_to_apply }
    }

    pub(crate) fn load_and_apply_macro_interactive(&mut self, into_new_tab: bool) {
        let Some(path) = rfd::FileDialog::new()
            .set_directory(&self.paths.macros_dir)
            .add_filter("MicroParquet Macro (.mpq.json)", &["json"])
            .pick_file()
        else {
            return;
        };

        match load_macro_from_file(&path) {
            Ok(recipe) => {
                let Some(curr) = self.tabs.get(self.active_tab).cloned() else {
                    self.status_message =
                        "Open a source file first before replaying a macro.".to_string();
                    return;
                };

                if into_new_tab {
                    let id = self.create_tab_from_source_with(
                        format!("Macro: {}", recipe.name),
                        curr.source,
                        recipe.steps,
                        curr.col_formats,
                        None,
                    );
                    if !recipe.summary.is_empty() {
                        self.pending_summary.insert(id, recipe.summary.clone());
                    }
                } else {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.record_undo();
                        tab.summary.extend(recipe.summary.clone());
                        tab.steps.extend(recipe.steps);
                        if recipe.enforce_balance_control && recipe.control_column.is_some() {
                            tab.control_col = recipe.control_column;
                        }
                    }
                    self.refresh_active_tab(true);
                }
                self.status_message = format!("Loaded and executed macro '{}'.", recipe.name);
            }
            Err(e) => {
                self.status_message = format!("Failed to load macro: {}", e);
            }
        }
    }

    pub(crate) fn prepare_python_preview_for_active_tab(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let (path, is_parq, cfg) = match &tab.source {
            TabSource::SingleFile {
                path,
                is_parquet,
                config,
            } => (path.clone(), *is_parquet, config.clone()),
            _ => (
                PathBuf::from("combined_or_joined_source.parquet"),
                true,
                IngestionConfig::default(),
            ),
        };
        // Masked columns anywhere in the source (also inside stacks and joins) refuse the export.
        if source_masks_pii(&tab.source) {
            let mut masked: Vec<String> = Vec::new();
            collect_masked_columns(&tab.source, &mut masked);
            self.status_message = python_pii_refusal(&masked);
            return;
        }
        self.macros.python_code = match generate_python_polars_code(
            &path,
            is_parq,
            &cfg,
            &tab.steps,
            tab.control_col.as_deref(),
        ) {
            Ok(py) => py,
            Err(e) => {
                self.status_message = format!("{:#}", e);
                return;
            }
        };
        let missing = python_untranslated_steps(&tab.steps);
        self.macros.python_warning = if missing.is_empty() {
            String::new()
        } else {
            format!(
                " \u{E4E0} WARNING: {} has no Python translation; the script raises NotImplementedError there.",
                missing.join(", ")
            )
        };
        if !self.macros.python_warning.is_empty() {
            self.status_message = format!("Python preview ready.{}", self.macros.python_warning);
        }
        self.active_tool = ActiveToolModal::PythonCodePreview;
    }

    /// Right panel (316 px) with two tabs: "Steps . N" and "Column".
    pub(crate) fn render_right_panel(&mut self, ctx: &egui::Context) {
        if self.tabs.is_empty() || !self.show_steps_sidebar {
            return;
        }
        let tk = *theme::tokens(self.dark_mode);
        let n_steps = self.tabs.get(self.active_tab).map(|t| t.steps.len()).unwrap_or(0);
        egui::SidePanel::right("right_panel")
            .exact_width(theme::RIGHT_PANEL_W)
            .resizable(false)
            .frame(egui::Frame::none().fill(tk.surface))
            .show(ctx, |ui| {
                theme::left_border(ui, tk.border);
                egui::Frame::none()
                    .fill(tk.grid)
                    .rounding(egui::Rounding::same(theme::RADIUS_PANEL))
                    .inner_margin(theme::MARGIN_SEGMENTED)
                    .outer_margin(theme::MARGIN_SEGMENTED_OUTER)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = theme::SPACE_XS;
                            let w = (ui.available_width() - theme::SPACE_XS) / 2.0;
                            for (tab, label) in [
                                (RightTab::Steps, format!("Steps \u{b7} {}", n_steps)),
                                (RightTab::Column, "Column".to_string()),
                            ] {
                                let on = self.right_tab == tab;
                                let text = theme::semibold(label).color(if on { tk.ink } else { tk.muted });
                                let btn = egui::Button::new(text)
                                    .fill(if on { tk.surface } else { egui::Color32::TRANSPARENT })
                                    .stroke(egui::Stroke::NONE)
                                    .rounding(egui::Rounding::same(theme::RADIUS_CONTROL));
                                if ui.add_sized([w, theme::SEGMENT_BTN_H], btn).clicked() {
                                    self.right_tab = tab;
                                }
                            }
                        });
                    });
                match self.right_tab {
                    RightTab::Steps => self.render_steps_tab(ui),
                    RightTab::Column => self.render_column_tab(ui),
                }
            });
    }

    /// Applies a change to the active tab's steps (recorded for undo) and refreshes the view.
    /// `f` returns `None` when it changed nothing, or the step to select afterwards.
    fn edit_steps(&mut self, status: &str, f: impl FnOnce(&mut Vec<TransformStep>) -> Option<Option<usize>>) {
        let idx = self.active_tab;
        let Some(tab) = self.tabs.get_mut(idx) else {
            return;
        };
        let before = tab.snapshot();
        let Some(sel) = f(&mut tab.steps) else {
            return;
        };
        tab.history.record(&before);
        tab.viewport_offset = 0;
        self.selected_step = sel;
        self.confirm_delete_step = None;
        self.status_message = status.to_string();
        self.refresh_tab_at(idx, false, Some(status.to_string()));
    }

    pub(crate) fn undo_steps(&mut self) {
        let idx = self.active_tab;
        let Some(tab) = self.tabs.get_mut(idx) else {
            return;
        };
        let current = tab.snapshot();
        match tab.history.undo(&current) {
            Some(state) => {
                // Steps, control column, summary choices and formats go back together.
                let ctrl_changed = tab.restore(state);
                tab.summary_cells.clear();
                tab.viewport_offset = 0;
                self.selected_step = None;
                self.status_message = "Undone.".to_string();
                self.refresh_tab_at(idx, ctrl_changed, Some("Undone.".to_string()));
                self.init_tool_defaults_from_active_tab();
            }
            None => self.status_message = "Nothing to undo.".to_string(),
        }
    }

    pub(crate) fn redo_steps(&mut self) {
        let idx = self.active_tab;
        let Some(tab) = self.tabs.get_mut(idx) else {
            return;
        };
        let current = tab.snapshot();
        match tab.history.redo(&current) {
            Some(state) => {
                // Steps, control column, summary choices and formats go back together.
                let ctrl_changed = tab.restore(state);
                tab.summary_cells.clear();
                tab.viewport_offset = 0;
                self.selected_step = None;
                self.status_message = "Redone.".to_string();
                self.refresh_tab_at(idx, ctrl_changed, Some("Redone.".to_string()));
                self.init_tool_defaults_from_active_tab();
            }
            None => self.status_message = "Nothing to redo.".to_string(),
        }
    }

    /// Ctrl+Z undoes and Ctrl+Y (or Ctrl+Shift+Z) redoes a change to the steps, unless the
    /// keyboard is busy in a text box.
    pub(crate) fn handle_step_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() || self.tabs.is_empty() {
            return;
        }
        let (redo, undo) = ctx.input_mut(|i| {
            let redo = i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z)
                || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y);
            let undo = !redo && i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z);
            (redo, undo)
        });
        if redo {
            self.redo_steps();
        } else if undo {
            self.undo_steps();
        }
    }

    /// Opens the editor for step `i`, prefilled (its tool modal, the filter or the sort editor).
    fn edit_step_dialog(&mut self, i: usize) {
        if !self.open_step_editor(i) {
            self.status_message = "This step has no editor (Set to Number, SQL): delete it and add it again.".to_string();
        }
    }

    fn render_steps_tab(&mut self, ui: &mut egui::Ui) {
        let tk = *theme::tokens(self.dark_mode);
        let mut open_schema = false;
        let mut select: Option<usize> = None;
        let mut do_edit: Option<usize> = None;
        let mut do_up: Option<usize> = None;
        let mut do_down: Option<usize> = None;
        let mut do_skip: Option<usize> = None;
        let mut do_dup: Option<usize> = None;
        let mut ask_delete: Option<usize> = None;
        let mut do_delete: Option<usize> = None;
        let mut cancel_delete = false;
        let mut insert_tool: Option<ActiveToolModal> = None;
        let mut insert_filter = false;
        let selected = self.selected_step;
        let confirm = self.confirm_delete_step;
        let mut row_rects: Vec<StepRowRects> = Vec::new();
        let mut card_rects: Vec<egui::Rect> = Vec::new();
        let mut button_rects: Vec<egui::Rect> = Vec::new();
        let mut inner_rect = egui::Rect::NOTHING;
        egui::Frame::none().inner_margin(theme::MARGIN_PANEL_STEPS).show(ui, |ui| {
            inner_rect = ui.available_rect_before_wrap();
            let Some(tab) = self.tabs.get(self.active_tab) else {
                return;
            };
            let count_text = |c: Option<Option<usize>>| match c.flatten() {
                Some(n) => format_with_commas(n as f64, 0),
                None => "\u{2026}".to_string(),
            };
            // Source
            ui.horizontal(|ui| {
                ui.label(theme::bold("Source").color(tk.ink));
                if matches!(tab.source, TabSource::SingleFile { .. })
                    && ui
                        .small_button("\u{E434} Schema")
                        .on_hover_text("Schema & Ingestion: column types, Dr/Cr sign and parse failures")
                        .clicked()
                {
                    open_schema = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(theme::mono(count_text(tab.step_counts.first().copied())).size(theme::TEXT_COUNT).color(tk.muted));
                });
            });
            let list_height = (ui.available_height() - theme::STEPS_FOOTER_RESERVE).max(theme::STEPS_LIST_MIN_H);
            egui::ScrollArea::vertical().max_height(list_height).show(ui, |ui| {
                for (i, step) in tab.steps.iter().enumerate() {
                    let is_sel = selected == Some(i);
                    let issue = tab.issues.iter().find(|x| x.index == i);
                    let skipped = step.is_skipped();
                    let beyond = tab.issues.first().is_some_and(|f| i > f.index);
                    let title_color = if issue.is_some() {
                        tk.danger
                    } else if skipped || beyond {
                        tk.muted
                    } else {
                        tk.ink
                    };
                    let out = ui.horizontal_top(|ui| {
                        ui.add_space(theme::STEP_LIST_INDENT);
                        let card = egui::Frame::none()
                            .fill(if is_sel { tk.accent_soft } else { egui::Color32::TRANSPARENT })
                            .rounding(egui::Rounding::same(theme::RADIUS_PANEL))
                            .inner_margin(theme::MARGIN_STEP_ROW)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                // (the card sits in a left-to-right row: its lines stack in their own column)
                                ui.vertical(|ui| {
                                let tag = if issue.is_some() {
                                    Some(("Error", tk.danger))
                                } else if skipped {
                                    Some(("Off", tk.muted))
                                } else if beyond {
                                    Some(("Not run", tk.muted))
                                } else {
                                    None
                                };
                                let c = if beyond { None } else { tab.step_counts.get(i + 1).copied() };
                                let detail = step.short_label();
                                let spec = StepRowSpec {
                                    title: step.title(),
                                    struck: skipped,
                                    title_color,
                                    tag,
                                    count: &count_text(c),
                                    detail: &detail,
                                    detail_color: if issue.is_some() { tk.danger } else { tk.muted },
                                };
                                row_rects.push(step_row_head(ui, &spec));
                                if let Some(x) = issue {
                                    ui.label(egui::RichText::new(&x.reason).size(theme::TEXT_SMALL).color(tk.danger));
                                } else if beyond {
                                    ui.label(
                                        egui::RichText::new("Not run: an earlier step is broken.")
                                            .size(theme::TEXT_SMALL)
                                            .color(tk.muted),
                                    );
                                }
                                if is_sel {
                                    ui.add_space(theme::SPACE_XS);
                                    // wraps to a second line instead of passing the panel's edge
                                    let buttons = ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing.x = theme::SPACE_XS;
                                        let editable = !skipped && crate::ui::step_edit::step_is_editable(step);
                                        let edit = ui.add_enabled(editable, egui::Button::new(theme::ph::PENCIL_SIMPLE));
                                        if edit
                                            .on_hover_text("Edit")
                                            .on_disabled_hover_text(if skipped { "Turn the step on to edit it." } else { "Set to Number and SQL steps have no editor: delete the step and add it again." })
                                            .clicked()
                                        {
                                            do_edit = Some(i);
                                        }
                                        if ui.add_enabled(i > 0, egui::Button::new(theme::ph::ARROW_UP)).on_hover_text("Move up").clicked() {
                                            do_up = Some(i);
                                        }
                                        if ui.add_enabled(i + 1 < tab.steps.len(), egui::Button::new(theme::ph::ARROW_DOWN)).on_hover_text("Move down").clicked() {
                                            do_down = Some(i);
                                        }
                                        let (icon, tip) = if skipped { (theme::ph::EYE, "Turn on") } else { (theme::ph::EYE_SLASH, "Turn off (skip)") };
                                        if ui.add(egui::Button::new(icon)).on_hover_text(tip).clicked() {
                                            do_skip = Some(i);
                                        }
                                        if ui.add(egui::Button::new(theme::ph::COPY)).on_hover_text("Duplicate").clicked() {
                                            do_dup = Some(i);
                                        }
                                        if confirm == Some(i) {
                                            if theme::danger_button(ui, "Delete?").clicked() {
                                                do_delete = Some(i);
                                            }
                                            if ui.button("No").clicked() {
                                                cancel_delete = true;
                                            }
                                        } else if ui
                                            .add(egui::Button::new(egui::RichText::new(theme::ph::TRASH).color(tk.danger)))
                                            .on_hover_text("Delete (asks first)")
                                            .clicked()
                                        {
                                            ask_delete = Some(i);
                                        }
                                    });
                                    button_rects.push(buttons.response.rect);
                                }
                                });
                            });
                        card_rects.push(card.response.rect);
                        card.response
                    });
                    let rect = out.inner.rect;
                    if out.inner.interact(egui::Sense::click()).clicked() {
                        select = Some(i);
                    }
                    // timeline: dot, and the line down to the next step
                    let x = rect.left() - theme::STEP_RAIL_X;
                    let dot_y = rect.top() + theme::STEP_DOT_Y;
                    let dot_color = if issue.is_some() {
                        tk.danger
                    } else if skipped || beyond {
                        tk.muted
                    } else {
                        tk.accent
                    };
                    let painter = ui.painter();
                    painter.line_segment(
                        [egui::pos2(x, dot_y), egui::pos2(x, rect.bottom() + theme::SPACE_XS)],
                        theme::line_thick(tk.border),
                    );
                    painter.circle(egui::pos2(x, dot_y), theme::STEP_DOT_R, tk.surface, theme::line_thick(dot_color));
                }
                // The line above the first step starts at the source.
            });

            // + Insert step after <selected>
            let label = match selected.and_then(|i| tab.steps.get(i).map(|s| (i, s))) {
                Some((i, s)) => format!("+ Insert step after \"{}\" ({})", s.title(), i + 1),
                None => "+ Add step at the end".to_string(),
            };
            ui.add_space(theme::SPACE_S);
            let insert = egui::menu::menu_custom_button(
                ui,
                egui::Button::new(egui::RichText::new(&label).size(theme::TEXT_SMALL).color(tk.ink2)).truncate(),
                |ui| {
                ui.label(theme::label_text("The next step you apply goes here").color(tk.muted));
                if ui.button(format!("{}  Filter", theme::ph::FUNNEL)).clicked() {
                    insert_filter = true;
                    ui.close_menu();
                }
                for (icon, name, tool) in [
                    (theme::ph::CALCULATOR, "Calc column", ActiveToolModal::MathFormula),
                    (theme::ph::PLUS, "Conditional column", ActiveToolModal::ConditionalCol),
                    (theme::ph::TABLE, "Pivot", ActiveToolModal::GroupByPivot),
                    (theme::ph::BUILDINGS, "Normalise names", ActiveToolModal::NormalizeEntity),
                    (theme::ph::CALENDAR, "Maturity buckets", ActiveToolModal::MaturityBucket),
                    (theme::ph::CHART_LINE, "Rate shock", ActiveToolModal::RateShockSim),
                    (theme::ph::TIMER, "Duration / \u{394}EVE", ActiveToolModal::DurationAnalysis),
                    (theme::ph::CALCULATOR, "FTP", ActiveToolModal::FtpCurveMap),
                    (theme::ph::BANK, "Provision", ActiveToolModal::LoanProvision),
                    (theme::ph::CREDIT_CARD, "Limits", ActiveToolModal::LoanLimitUtil),
                    (theme::ph::SHIELD_CHECK, "Hygiene audit", ActiveToolModal::BankingAudit),
                ] {
                    if ui.button(format!("{}  {}", icon, name)).clicked() {
                        insert_tool = Some(tool);
                        ui.close_menu();
                    }
                }
            },
            );
            insert.response.on_hover_text(&label);
            if tab.history.can_undo() || tab.history.can_redo() {
                ui.horizontal(|ui| {
                    if ui.add_enabled(tab.history.can_undo(), egui::Button::new("Undo")).on_hover_text("Ctrl+Z").clicked() {
                        select = Some(usize::MAX - 1);
                    }
                    if ui.add_enabled(tab.history.can_redo(), egui::Button::new("Redo")).on_hover_text("Ctrl+Y").clicked() {
                        select = Some(usize::MAX);
                    }
                });
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - theme::SPACE_M) / 3.0; // not a size: three buttons
                    if ui.add_sized([w, theme::PANEL_BTN_H], egui::Button::new(theme::semibold("Save macro").size(theme::TEXT_SMALL))).clicked() {
                        self.active_tool = ActiveToolModal::SaveMacro;
                    }
                    if ui.add_sized([w, theme::PANEL_BTN_H], egui::Button::new(theme::semibold("Load").size(theme::TEXT_SMALL))).clicked() {
                        self.load_and_apply_macro_interactive(false);
                    }
                    if ui.add_sized([w, theme::PANEL_BTN_H], egui::Button::new(theme::semibold("To .py").size(theme::TEXT_SMALL))).clicked() {
                        self.prepare_python_preview_for_active_tab();
                    }
                });
            });
        });

        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("steps_report"), StepsReport { inner: inner_rect, rows: row_rects, cards: card_rects, buttons: button_rects }));

        if open_schema {
            self.open_schema_modal_for_active_tab();
        }
        match select {
            Some(x) if x == usize::MAX - 1 => self.undo_steps(),
            Some(usize::MAX) => self.redo_steps(),
            Some(i) => {
                self.selected_step = Some(i);
                self.confirm_delete_step = None;
            }
            None => {}
        }
        if let Some(i) = do_edit {
            self.edit_step_dialog(i);
        }
        if let Some(i) = do_up {
            self.edit_steps("Step moved up.", |s| micro_parquet::steps_edit::move_up(s, i).map(Some));
        }
        if let Some(i) = do_down {
            self.edit_steps("Step moved down.", |s| micro_parquet::steps_edit::move_down(s, i).map(Some));
        }
        if let Some(i) = do_skip {
            self.edit_steps("Step turned on or off.", |s| micro_parquet::steps_edit::toggle_skip(s, i).map(|_| Some(i)));
        }
        if let Some(i) = do_dup {
            self.edit_steps("Step duplicated.", |s| micro_parquet::steps_edit::duplicate_step(s, i).map(Some));
        }
        if let Some(i) = ask_delete {
            self.confirm_delete_step = Some(i);
        }
        if cancel_delete {
            self.confirm_delete_step = None;
        }
        if let Some(i) = do_delete {
            self.edit_steps("Step deleted.", |s| micro_parquet::steps_edit::delete_step(s, i).map(|_| None));
        }
        if insert_filter {
            self.insert_after = self.selected_step;
            self.open_filter_editor(None, false);
        }
        if let Some(tool) = insert_tool {
            self.insert_after = self.selected_step;
            self.init_tool_defaults_from_active_tab();
            self.active_tool = tool;
        }
    }

    /// The Column tab (inspector): name, type and format, counts, statistics, a histogram or the
    /// most frequent values, and actions for the column.
    fn render_column_tab(&mut self, ui: &mut egui::Ui) {
        use micro_parquet::column_stats::ColumnKind;
        let tk = *theme::tokens(self.dark_mode);
        let mut do_filter: Option<String> = None;
        let mut do_sort: Option<String> = None;
        let mut do_summary: Option<(String, SummarySpec)> = None;
        let mut do_format = false;
        let mut do_type = false;
        let (num_format, decimals) = (self.num_format, self.global_decimals);
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Frame::none().inner_margin(theme::MARGIN_PANEL_COLUMN).show(ui, |ui| {
                let Some(tab) = self.tabs.get(self.active_tab) else {
                    return;
                };
                let cols = tab.available_columns();
                let Some(name) = self.selected_column.clone().filter(|c| cols.contains(c)) else {
                    ui.label(egui::RichText::new("Click a column header to see its statistics.").color(tk.muted));
                    return;
                };
                let dtype = tab.cached_slice.column(&name).map(|c| c.dtype().clone()).ok();
                let spec = tab.col_formats.get(&name);
                let key = stats_key(&tab.source, &tab.steps[..tab.exec_steps.min(tab.steps.len())]);
                let stats = tab.column_stats.as_ref().filter(|s| s.column == name);
                let stale = stats.is_none_or(|s| s.key != key);

                ui.label(theme::bold(name.clone()).size(theme::TEXT_INSPECTOR_TITLE));
                let kind_label = match stats.map(|s| &s.kind) {
                    Some(ColumnKind::Number) => "Number",
                    Some(ColumnKind::Date) => "Date",
                    Some(ColumnKind::Text) => "Text",
                    Some(ColumnKind::Other) => "Other",
                    None => match &dtype {
                        Some(d) if d.is_numeric() => "Number",
                        Some(d) if d.is_temporal() => "Date",
                        Some(_) => "Text",
                        None => "",
                    },
                };
                let is_number = kind_label == "Number";
                let format_text = if is_number {
                    let scale = resolved_scale(&name, spec, num_format);
                    let dec = spec.map(|s| s.decimals).unwrap_or(decimals);
                    match scale {
                        Some(s) => format!("{} \u{b7} {} \u{b7} {} dp", kind_label, s.suffix, dec),
                        None => format!("{} \u{b7} {} dp", kind_label, dec),
                    }
                } else {
                    kind_label.to_string()
                };
                ui.label(egui::RichText::new(format_text).color(tk.muted));
                ui.add_space(theme::SPACE_M);

                if stale {
                    ui.label(theme::semibold("Updating\u{2026}").size(theme::TEXT_SMALL).color(tk.accent));
                }
                if let Some(st) = stats {
                    ui.scope(|ui| {
                        if stale {
                            ui.set_opacity(theme::DISABLED_OPACITY);
                        }
                        let count = |n: u64| format_with_commas(n as f64, 0);
                        let fails = tab.parse_failures.iter().find(|f| f.column == name).map(|f| f.count as u64).unwrap_or(0);
                        let num = |v: f64| format_metric_value(v, &name, spec, num_format, decimals);
                        let mut tiles: Vec<(&str, String, Option<egui::Color32>)> = vec![
                            ("Rows", count(st.rows), None),
                            ("Blanks", count(st.blanks), None),
                            ("Parse failures", count(fails), Some(if fails == 0 { tk.success } else { tk.warning })),
                        ];
                        if let Some(n) = &st.number {
                            tiles.push(("Sum", num(n.sum), None));
                            tiles.push(("Min", num(n.min), None));
                            tiles.push(("Max", num(n.max), None));
                            tiles.push(("Mean", num(n.mean), None));
                            tiles.push(("Distinct", count(n.distinct), None));
                        }
                        if let Some(d) = &st.date {
                            tiles.push(("Min", micro_parquet::summary::date_text(d.min), None));
                            tiles.push(("Max", micro_parquet::summary::date_text(d.max), None));
                        }
                        if let Some(t) = &st.text {
                            tiles.push(("Distinct", count(t.distinct), None));
                        }
                        egui::Grid::new("col_tiles").num_columns(2).spacing([theme::SPACE_S, theme::SPACE_S]).show(ui, |ui| {
                            for (i, (k, v, color)) in tiles.iter().enumerate() {
                                egui::Frame::none()
                                    .fill(tk.bg)
                                    .rounding(egui::Rounding::same(theme::RADIUS_PANEL))
                                    .inner_margin(theme::MARGIN_TILE)
                                    .show(ui, |ui| {
                                        ui.set_width(theme::TILE_W);
                                        ui.label(theme::semibold(*k).size(theme::TEXT_LABEL).color(tk.muted));
                                        ui.add(egui::Label::new(theme::mono_semibold(v.clone()).color(color.unwrap_or(tk.ink))).truncate());
                                    });
                                if i % 2 == 1 {
                                    ui.end_row();
                                }
                            }
                        });
                        // distribution
                        if let Some(n) = &st.number {
                            ui.add_space(theme::SPACE_M);
                            ui.label(theme::semibold("Distribution").size(theme::TEXT_LABEL).color(tk.muted));
                            let max = n.histogram.iter().copied().max().unwrap_or(1).max(1) as f32;
                            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), theme::HISTOGRAM_H), egui::Sense::hover());
                            let w = rect.width() / n.histogram.len() as f32;
                            for (i, c) in n.histogram.iter().enumerate() {
                                let h = (*c as f32 / max) * rect.height();
                                let bar = egui::Rect::from_min_max(
                                    egui::pos2(rect.left() + i as f32 * w + theme::HISTOGRAM_BAR_GAP, rect.bottom() - h),
                                    egui::pos2(rect.left() + (i as f32 + 1.0) * w - theme::HISTOGRAM_BAR_GAP, rect.bottom()),
                                );
                                ui.painter().rect_filled(bar, theme::ROUNDING_BAR_TOP, tk.accent.gamma_multiply(theme::HISTOGRAM_BAR_ALPHA));
                            }
                            resp.on_hover_text(format!(
                                "{} bars from {} to {}; the bars add up to {} values",
                                n.histogram.len(),
                                num(n.min),
                                num(n.max),
                                count(n.histogram.iter().sum())
                            ));
                        }
                        if let Some(t) = &st.text {
                            ui.add_space(theme::SPACE_M);
                            ui.label(theme::semibold("Most frequent").size(theme::TEXT_LABEL).color(tk.muted));
                            for (v, c) in &t.top {
                                ui.horizontal(|ui| {
                                    ui.add(egui::Label::new(egui::RichText::new(v.clone()).color(tk.ink)).truncate());
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(theme::mono(count(*c)).size(theme::TEXT_COUNT).color(tk.muted));
                                    });
                                });
                            }
                        }
                    });
                }

                // actions
                ui.add_space(theme::SPACE_M);
                let wide = |ui: &mut egui::Ui, text: String| ui.add_sized([ui.available_width(), theme::WIDE_BTN_H], egui::Button::new(theme::semibold(text)));
                if wide(ui, format!("{}  Filter on this column\u{2026}", theme::ph::FUNNEL)).clicked() {
                    do_filter = Some(name.clone());
                }
                if wide(ui, format!("{}  Add as sort level", theme::ph::SORT_ASCENDING)).clicked() {
                    do_sort = Some(name.clone());
                }
                // Summary: the choice for this column, and the weight of a weighted average
                let current = tab.summary.get(&name).cloned().unwrap_or_else(|| match dtype.as_ref() {
                    Some(d) => micro_parquet::summary::default_spec_for(d, &name, tab.control_col.as_deref()),
                    None => SummarySpec::default(),
                });
                ui.horizontal(|ui| {
                    ui.label(theme::semibold("Summary").size(theme::TEXT_SMALL).color(tk.muted));
                    egui::ComboBox::from_id_salt("insp_summary")
                        .selected_text(current.agg.label())
                        .show_ui(ui, |ui| {
                            for agg in SummaryAgg::all() {
                                let needs_numbers = matches!(agg, SummaryAgg::Sum | SummaryAgg::Average | SummaryAgg::WeightedAvg);
                                if ui.add_enabled(!needs_numbers || is_number, egui::SelectableLabel::new(current.agg == *agg, agg.label())).clicked() {
                                    do_summary = Some((name.clone(), SummarySpec { agg: *agg, weight_col: current.weight_col.clone() }));
                                }
                            }
                        });
                });
                if current.agg == SummaryAgg::WeightedAvg {
                    ui.horizontal(|ui| {
                        ui.label(theme::semibold("Weight").size(theme::TEXT_SMALL).color(tk.muted));
                        egui::ComboBox::from_id_salt("insp_weight")
                            .selected_text(current.weight_col.clone().unwrap_or_else(|| "pick a column".to_string()))
                            .show_ui(ui, |ui| {
                                for c in &cols {
                                    if c != &name && ui.selectable_label(current.weight_col.as_deref() == Some(c.as_str()), c).clicked() {
                                        do_summary = Some((
                                            name.clone(),
                                            SummarySpec { agg: SummaryAgg::WeightedAvg, weight_col: Some(c.clone()) },
                                        ));
                                    }
                                }
                            });
                    });
                }
                let single = matches!(tab.source, TabSource::SingleFile { .. });
                ui.menu_button(
                    theme::semibold(format!("{}  Change type or format\u{2026}", theme::ph::SLIDERS_HORIZONTAL)),
                    |ui| {
                        if ui.button("Number format and decimals (Columns)\u{2026}").clicked() {
                            do_format = true;
                            ui.close_menu();
                        }
                        let t = ui.add_enabled(single, egui::Button::new("Column type (Schema & Ingestion)\u{2026}"));
                        if t.on_disabled_hover_text("Types are set on the file; this tab is not a single file.").clicked() {
                            do_type = true;
                            ui.close_menu();
                        }
                    },
                );
            });
        });

        if let Some(c) = do_filter {
            self.open_filter_editor_for_column(&c);
        }
        if let Some(c) = do_sort {
            self.sort_from_header(&c, false, true);
        }
        if let Some((c, spec)) = do_summary {
            self.set_summary(&c, spec);
        }
        if do_format {
            self.init_tool_defaults_from_active_tab();
            self.active_tool = ActiveToolModal::ManageColumns;
        }
        if do_type {
            self.open_schema_modal_for_active_tab();
        }
    }
}

/// The two tabs of the right panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RightTab {
    Steps,
    Column,
}
