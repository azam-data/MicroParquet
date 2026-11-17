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
        // the choices the body edits and the footer's Start button reads
        let mut d = dir;
        let mut s = strat;
        theme::modal("\u{E094} Recursive Folder-Tree Mirror Batch Converter")
            .collapsible(false)
            .default_width(theme::DIALOG_W_TOOL)
            .show(ctx, |dlg| {
                dlg.body(|ui| {
                    ui.horizontal(|ui| {
                        if ui.button("\u{E256} 1. Input Root Folder...").clicked() {
                            self.batch_modal.input_folder = rfd::FileDialog::new().pick_folder();
                        }
                        ui.label(format!("{:?}", self.batch_modal.input_folder));
                    });
                    ui.horizontal(|ui| {
                        if ui.button("\u{E256} 2. Output Root Folder...").clicked() {
                            self.batch_modal.output_folder = rfd::FileDialog::new().pick_folder();
                        }
                        ui.label(format!("{:?}", self.batch_modal.output_folder));
                    });

                    ui.horizontal(|ui| {
                        ui.radio_value(&mut d, BatchDirection::CsvToParquet, "CSV -> Parquet");
                        ui.radio_value(&mut d, BatchDirection::ParquetToCsv, "Parquet -> CSV");
                    });
                    self.batch_modal.direction = Some(d);

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
                                if ui.button("\u{E256} Pick .schema.json...").clicked() {
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
                                        theme::warning(ui),
                                        "No template chosen",
                                    ),
                                };
                            });
                        }
                    }


                    let needs_template = d == BatchDirection::CsvToParquet
                        && s == BatchSchemaStrategy::MasterSchemaTemplate
                        && self.batch_modal.loaded_template.is_none();
                    let batch_running = self.jobs.active_long().is_some_and(|a| a.label == "Batch conversion");

                    if needs_template {
                        ui.colored_label(
                            theme::warning(ui),
                            "Pick a .schema.json template, or choose Smart Safe Auto-Detect.",
                        );
                    }
                    if batch_running {
                        ui.label("Batch running… progress and Cancel are in the status bar.");
                    }
                    if let Some(err) = &self.batch_modal.last_error {
                        ui.colored_label(theme::danger(ui), format!("\u{E4F8} {}", err));
                    }
                    if let Some(rep) = &self.batch_modal.last_report {
                        ui.separator();
                        ui.label(format!(
                            "Converted: {} | Companion Copied: {} | Errors: {} | Size: {:.1} MB -> {:.1} MB",
                            micro_parquet::table_layout::format_count(rep.files_converted),
                            micro_parquet::table_layout::format_count(rep.companion_files_copied),
                            micro_parquet::table_layout::format_count(rep.errors.len()),
                            rep.bytes_before as f64 / 1_048_576.0, // not a size: bytes per MB
                            rep.bytes_after as f64 / 1_048_576.0 // not a size: bytes per MB
                        ));
                        if let Some(p) = &rep.log_path {
                            ui.label(format!("Log: {:?}", p));
                        }
                        let fails: usize = rep.log.iter().filter_map(|r| r.parse_failures).sum();
                        if fails > 0 {
                            ui.colored_label(
                                theme::warning(ui),
                                format!("\u{E4E0} {} value(s) could not be converted and are null (see parse_failures in the log)", micro_parquet::table_layout::format_count(fails)),
                            );
                        }
                        for n in &rep.notices {
                            ui.colored_label(theme::warning(ui), format!("\u{E4E0} {}", n));
                        }
                        if !rep.warnings.is_empty() {
                            ui.colored_label(
                                theme::warning(ui),
                                format!("\u{E4E0} {} file(s) with fewer rows than data lines (status WARNING in the log):", micro_parquet::table_layout::format_count(rep.warnings.len())),
                            );
                            egui::ScrollArea::vertical()
                                .id_salt("batch_warnings")
                                .max_height(theme::LIST_MAX_H_S)
                                .show(ui, |ui| {
                                    for w in &rep.warnings {
                                        ui.label(format!("• {}", w));
                                    }
                                });
                        }
                        if !rep.errors.is_empty() {
                            ui.colored_label(theme::danger(ui), "Errors:");
                            egui::ScrollArea::vertical()
                                .max_height(theme::LIST_MAX_H_M)
                                .show(ui, |ui| {
                                    for e in &rep.errors {
                                        ui.label(format!("• {}", e));
                                    }
                                });
                        }
                    }

                });
                dlg.footer(|ui| {
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
                                egui::Button::new("\u{E3FE} Start Mirror Batch"),
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


                });
            });

        if close_clicked {
            self.batch_modal.open = false;
        }
    }
}
