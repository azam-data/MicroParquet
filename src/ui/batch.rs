//! Screen module `batch` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

#[derive(Default)]
pub struct BatchModalState {
    pub open: bool,
    pub input_folder: Option<PathBuf>,
    pub output_folder: Option<PathBuf>,
    pub direction: Option<BatchDirection>,
    pub schema_strategy: Option<BatchSchemaStrategy>,
    pub template_path: Option<PathBuf>,
    pub loaded_template: Option<IngestionConfig>,
    pub mdy_dates_on_csv: bool,
    pub last_report: Option<BatchReport>,
    /// Error of the last run as a whole (bad folders, missing template), or of loading the template.
    pub last_error: Option<String>,
}

impl MicroParquetApp {
    pub(crate) fn render_batch_modal(&mut self, ctx: &egui::Context) {
        if !self.batch_modal.open {
            return;
        }
        let dir = *self
            .batch_modal
            .direction
            .get_or_insert(BatchDirection::CsvToParquet);
        let strat = *self
            .batch_modal
            .schema_strategy
            .get_or_insert(BatchSchemaStrategy::SmartSafeAutoDetect);

        let mut close_clicked = false;
        egui::Window::new("🔄 Recursive Folder-Tree Mirror Batch Converter")
            .collapsible(false)
            .default_width(680.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("📂 1. Input Root Folder...").clicked() {
                        self.batch_modal.input_folder = rfd::FileDialog::new().pick_folder();
                    }
                    ui.label(format!("{:?}", self.batch_modal.input_folder));
                });
                ui.horizontal(|ui| {
                    if ui.button("📂 2. Output Root Folder...").clicked() {
                        self.batch_modal.output_folder = rfd::FileDialog::new().pick_folder();
                    }
                    ui.label(format!("{:?}", self.batch_modal.output_folder));
                });

                let mut d = dir;
                ui.horizontal(|ui| {
                    ui.radio_value(&mut d, BatchDirection::CsvToParquet, "CSV -> Parquet");
                    ui.radio_value(&mut d, BatchDirection::ParquetToCsv, "Parquet -> CSV");
                });
                self.batch_modal.direction = Some(d);

                let mut s = strat;
                if d == BatchDirection::CsvToParquet {
                    ui.radio_value(
                        &mut s,
                        BatchSchemaStrategy::SmartSafeAutoDetect,
                        "Smart Safe Auto-Detect (Locks Leading Zeros to Utf8)",
                    );
                    ui.radio_value(
                        &mut s,
                        BatchSchemaStrategy::MasterSchemaTemplate,
                        "Apply Saved .schema.json Template",
                    );
                    self.batch_modal.schema_strategy = Some(s);
                    if s == BatchSchemaStrategy::MasterSchemaTemplate {
                        ui.horizontal(|ui| {
                            if ui.button("📂 Pick .schema.json...").clicked() {
                                if let Some(p) = rfd::FileDialog::new()
                                    .set_directory(&self.paths.schemas_dir)
                                    .add_filter("Schema JSON", &["json"])
                                    .pick_file()
                                {
                                    match std::fs::read_to_string(&p)
                                        .map_err(anyhow::Error::from)
                                        .and_then(|t| {
                                            serde_json::from_str::<IngestionConfig>(&t)
                                                .map_err(anyhow::Error::from)
                                        }) {
                                        Ok(cfg) => {
                                            self.batch_modal.loaded_template = Some(cfg);
                                            self.batch_modal.template_path = Some(p);
                                            self.batch_modal.last_error = None;
                                        }
                                        Err(e) => {
                                            self.batch_modal.loaded_template = None;
                                            self.batch_modal.template_path = None;
                                            self.batch_modal.last_error = Some(format!(
                                                "Could not load the template {:?}: {}",
                                                p, e
                                            ));
                                        }
                                    }
                                }
                            }
                            match &self.batch_modal.template_path {
                                Some(p) => ui.label(format!("{:?}", p)),
                                None => ui.colored_label(
                                    theme::WARN_ORANGE,
                                    "No template chosen",
                                ),
                            };
                        });
                    }
                }

                ui.separator();
                let needs_template = d == BatchDirection::CsvToParquet
                    && s == BatchSchemaStrategy::MasterSchemaTemplate
                    && self.batch_modal.loaded_template.is_none();
                let have_folders = self.batch_modal.input_folder.is_some()
                    && self.batch_modal.output_folder.is_some();
                let batch_running = self.jobs.active_long().is_some_and(|a| a.label == "Batch conversion");
                ui.horizontal(|ui| {
                    let run = ui
                        .add_enabled(
                            !needs_template && have_folders && !batch_running,
                            egui::Button::new("🚀 Start Mirror Batch"),
                        )
                        .on_disabled_hover_text(if needs_template {
                            "Pick the .schema.json template first"
                        } else {
                            "Choose the input and output folders first"
                        });
                    if run.clicked() {
                        if let (Some(in_dir), Some(out_dir)) = (
                            self.batch_modal.input_folder.clone(),
                            self.batch_modal.output_folder.clone(),
                        ) {
                            let (dialect, mdy) = (self.parquet_dialect, self.batch_modal.mdy_dates_on_csv);
                            let template = self.batch_modal.loaded_template.clone();
                            let temp = self.paths.temp_dir.clone();
                            self.jobs.submit(
                                JobKey::unique(JobKind::Batch),
                                "Batch conversion",
                                true,
                                move |ctx| {
                                    micro_parquet::batch::run_folder_mirror_batch_ctx(
                                        &in_dir, &out_dir, d, s, template.as_ref(), dialect, mdy, &temp, ctx,
                                    )
                                    .map(|r| JobOutput::Batch(Box::new(r)))
                                },
                            );
                            self.batch_modal.last_report = None;
                            self.batch_modal.last_error = None;
                            self.status_message = "Batch started…".to_string();
                        }
                    }
                    if ui.button("Close").clicked() {
                        close_clicked = true;
                    }
                });

                if needs_template {
                    ui.colored_label(
                        theme::WARN_ORANGE,
                        "Pick a .schema.json template, or choose Smart Safe Auto-Detect.",
                    );
                }
                if batch_running {
                    ui.label("Batch running… progress and Cancel are in the status bar.");
                }
                if let Some(err) = &self.batch_modal.last_error {
                    ui.colored_label(theme::ERR_RED, format!("❌ {}", err));
                }
                if let Some(rep) = &self.batch_modal.last_report {
                    ui.separator();
                    ui.label(format!(
                        "Converted: {} | Companion Copied: {} | Errors: {} | Size: {:.1} MB -> {:.1} MB",
                        rep.files_converted,
                        rep.companion_files_copied,
                        rep.errors.len(),
                        rep.bytes_before as f64 / 1_048_576.0,
                        rep.bytes_after as f64 / 1_048_576.0
                    ));
                    if let Some(p) = &rep.log_path {
                        ui.label(format!("Log: {:?}", p));
                    }
                    let fails: usize = rep.log.iter().filter_map(|r| r.parse_failures).sum();
                    if fails > 0 {
                        ui.colored_label(
                            theme::WARN_ORANGE,
                            format!("⚠ {} value(s) could not be converted and are null (see parse_failures in the log)", fails),
                        );
                    }
                    for n in &rep.notices {
                        ui.colored_label(theme::WARN_ORANGE, format!("⚠ {}", n));
                    }
                    if !rep.warnings.is_empty() {
                        ui.colored_label(
                            theme::WARN_ORANGE,
                            format!("⚠ {} file(s) with fewer rows than data lines (status WARNING in the log):", rep.warnings.len()),
                        );
                        egui::ScrollArea::vertical()
                            .id_salt("batch_warnings")
                            .max_height(120.0)
                            .show(ui, |ui| {
                                for w in &rep.warnings {
                                    ui.label(format!("• {}", w));
                                }
                            });
                    }
                    if !rep.errors.is_empty() {
                        ui.colored_label(theme::ERR_RED, "Errors:");
                        egui::ScrollArea::vertical()
                            .max_height(180.0)
                            .show(ui, |ui| {
                                for e in &rep.errors {
                                    ui.label(format!("• {}", e));
                                }
                            });
                    }
                }
            });

        if close_clicked {
            self.batch_modal.open = false;
        }
    }
}
