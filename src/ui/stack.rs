//! Screen module `stack` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct StackState {
    pub files_list: Vec<PathBuf>,
    pub include_source: bool,
    /// 0 = each file's own schema, 1 = first file's schema for all, 2 = a saved .schema.json for all.
    pub schema_mode: u8,
    pub schema_path: Option<PathBuf>,
}

impl Default for StackState {
    fn default() -> Self {
        Self {
            files_list: Vec::new(),
            include_source: true,
            schema_mode: 0,
            schema_path: None,
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_stack_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let step_to_apply: Option<(TransformStep, bool)> = None;
        egui::Window::new("📚 Vertical Multi-File Merger / Stacker")
            .collapsible(false)
            .show(ctx, |ui| {
                if ui.button("➕ Add Files to Stack...").clicked() {
                    if let Some(files) = rfd::FileDialog::new()
                        .add_filter("Data Files", &["csv", "parquet"])
                        .pick_files()
                    {
                        self.stack.files_list.extend(files);
                    }
                }
                for f in &self.stack.files_list {
                    ui.label(format!("  • {:?}", f.file_name().unwrap_or_default()));
                }
                ui.checkbox(
                    &mut self.stack.include_source,
                    "Append 'Source_File' Column to identify origin month/file",
                );
                ui.label("Column types (a column missing from a file is null for its rows):");
                ui.radio_value(&mut self.stack.schema_mode, 0, "Each file's own detected types (types must agree)");
                ui.radio_value(&mut self.stack.schema_mode, 1, "Apply one schema to every file: the first file's");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.stack.schema_mode, 2, "Apply one schema to every file: saved .schema.json");
                    if ui.button("📂 Pick...").clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .set_directory(&self.paths.schemas_dir)
                            .add_filter("Schema JSON", &["json"])
                            .pick_file()
                        {
                            self.stack.schema_path = Some(p);
                            self.stack.schema_mode = 2;
                        }
                    }
                    if let Some(p) = &self.stack.schema_path {
                        ui.label(p.file_name().unwrap_or_default().to_string_lossy().to_string());
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    let needs_template =
                        self.stack.schema_mode == 2 && self.stack.schema_path.is_none();
                    let run = ui
                        .add_enabled(
                            !needs_template,
                            egui::Button::new("🚀 Stack & Open as Unified Tab"),
                        )
                        .on_disabled_hover_text("Pick the .schema.json to apply first");
                    if run.clicked() && !self.stack.files_list.is_empty() {
                        let shared: anyhow::Result<Option<IngestionConfig>> =
                            match (self.stack.schema_mode, &self.stack.schema_path) {
                                (1, _) => self
                                    .stack.files_list
                                    .first()
                                    .map(|p| detect_file_schema(p))
                                    .transpose(),
                                (2, Some(p)) => std::fs::read_to_string(p)
                                    .map_err(anyhow::Error::from)
                                    .and_then(|t| {
                                        serde_json::from_str::<IngestionConfig>(&t)
                                            .map_err(anyhow::Error::from)
                                    })
                                    .map(Some)
                                    .map_err(|e| {
                                        anyhow::anyhow!("Could not load the schema file {:?}: {}", p, e)
                                    }),
                                _ => Ok(None),
                            };
                        match shared.and_then(|s| {
                            stack_file_configs(&self.stack.files_list, s.as_ref())
                        }) {
                            Err(e) => {
                                self.status_message = format!("Stack failed: {:#}", e);
                            }
                            Ok((specs, notes)) => {
                                self.create_tab_from_source(
                                    format!("Stacked ({} files)", specs.len()),
                                    TabSource::VerticalStack {
                                        files: specs,
                                        include_source_col: self.stack.include_source,
                                    },
                                    Vec::new(),
                                    HashMap::new(),
                                );
                                if !notes.is_empty() {
                                    self.status_message
                                        .push_str(&format!(" ⚠ {}", notes.join("; ")));
                                }
                                close_modal = true;
                            }
                        }
                    }
                    if ui.button("Clear List").clicked() {
                        self.stack.files_list.clear();
                    }
                    if ui.button("Cancel").clicked() {
                        close_modal = true;
                    }
                });
            });
        ModalOutcome { close: close_modal, step: step_to_apply }
    }
}
