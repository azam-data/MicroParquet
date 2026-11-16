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

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_save_macro_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let step_to_apply: Option<(TransformStep, bool)> = None;
        egui::Window::new("💾 Save Portable Macro Recipe (.mpq.json)")
            .collapsible(false)
            .show(ctx, |ui| {
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
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("💾 Save .mpq.json File...").clicked() {
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
                            };
                            if let Some(save_path) = rfd::FileDialog::new()
                                .set_directory(&self.paths.macros_dir)
                                .set_file_name(format!("{}.mpq.json", self.macros.name))
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
        ModalOutcome { close: close_modal, step: step_to_apply }
    }

    #[allow(unused_variables)]
    pub(crate) fn render_python_preview_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let step_to_apply: Option<(TransformStep, bool)> = None;
        egui::Window::new("🐍 Export GUI Pipeline as Python Polars Jupyter Code")
            .collapsible(false)
            .default_width(760.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.macros.python_code)
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .desired_rows(18),
                        );
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("📋 Copy Python Code to Clipboard").clicked() {
                        ctx.copy_text(self.macros.python_code.clone());
                        self.status_message = format!(
                            "Copied Python Polars code to clipboard!{}",
                            self.macros.python_warning
                        );
                    }
                    if ui.button("💾 Save as .py Script...").clicked() {
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
                    self.create_tab_from_source(
                        format!("Macro: {}", recipe.name),
                        curr.source,
                        recipe.steps,
                        curr.col_formats,
                    );
                } else {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
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
                " ⚠ WARNING: {} has no Python translation; the script raises NotImplementedError there.",
                missing.join(", ")
            )
        };
        if !self.macros.python_warning.is_empty() {
            self.status_message = format!("Python preview ready.{}", self.macros.python_warning);
        }
        self.active_tool = ActiveToolModal::PythonCodePreview;
    }

    pub(crate) fn render_steps_panel(&mut self, ctx: &egui::Context) {
        if !self.tabs.is_empty() && self.show_steps_sidebar {
            egui::SidePanel::right("applied_steps_sidebar")
                .resizable(true)
                .default_width(255.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading("📋 Applied Steps");
                        if ui.small_button("✖ Hide").clicked() {
                            self.show_steps_sidebar = false;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.small_button("💾 Save .mpq").clicked() {
                            self.active_tool = ActiveToolModal::SaveMacro;
                        }
                        if ui.small_button("▶ Load .mpq").clicked() {
                            self.load_and_apply_macro_interactive(false);
                        }
                        if ui.small_button("🐍 To .py").clicked() {
                            self.prepare_python_preview_for_active_tab();
                        }
                    });
                    ui.separator();

                    let mut step_to_remove: Option<usize> = None;
                    let mut clear_all = false;
                    let mut open_schema = false;

                    if let Some(tab) = self.tabs.get(self.active_tab) {
                        ui.horizontal(|ui| {
                            ui.label("0. [Source Locked & Sanitized]");
                            if matches!(tab.source, TabSource::SingleFile { .. })
                                && ui
                                    .small_button("🧬 Schema & Ingestion")
                                    .on_hover_text("Column types, Dr/Cr sign and parse failures")
                                    .clicked()
                            {
                                open_schema = true;
                            }
                        });
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for (idx, step) in tab.steps.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(format!("{}. {}", idx + 1, step.short_label()));
                                    if ui.small_button("✖").clicked() {
                                        step_to_remove = Some(idx);
                                    }
                                });
                            }
                        });
                        if !tab.steps.is_empty() {
                            ui.separator();
                            if ui.button("🗑 Reset All Steps").clicked() {
                                clear_all = true;
                            }
                        }
                    }

                    if open_schema {
                        self.open_schema_modal_for_active_tab();
                    }
                    if let Some(idx) = step_to_remove {
                        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                            tab.steps.remove(idx);
                        }
                        self.refresh_active_tab(false);
                    } else if clear_all {
                        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                            tab.steps.clear();
                        }
                        self.refresh_active_tab(false);
                    }
                });
        }
    }
}
