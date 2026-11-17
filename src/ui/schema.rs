//! Screen module `schema` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

pub struct PendingCsvModal {
    pub csv_path: PathBuf,
    pub raw_preview_df: DataFrame,
    pub config: IngestionConfig,
    pub direct_convert_only: bool,
    /// Set when the modal edits the schema of an open tab instead of opening a new one.
    pub target_tab: Option<usize>,
    /// Parse failures of that tab's last load (empty for a new file).
    pub parse_failures: Vec<ParseFailure>,
    /// The file is Parquet: CSV reading options are hidden and file types are shown.
    pub is_parquet: bool,
}

impl MicroParquetApp {
    pub(crate) fn render_csv_schema_modal(&mut self, ctx: &egui::Context) {
        let Some(mut modal) = self.pending_csv_modal.take() else {
            return;
        };

        let mut keep_open = true;
        let mut action_open_tab = false;
        let mut action_apply_to_tab = false;
        let mut action_direct_convert = false;
        let mut action_reread = false;

        let title = if modal.is_parquet {
            "\u{E434} Schema & Ingestion (Parquet): Types & PII Masking"
        } else {
            "\u{E434} Schema & Ingestion: CSV 5-Row Raw Preview, Linebreak Sanitizer & Strict Schema Lock"
        };
        theme::modal(title)
            .resizable(true)
            .default_width(980.0)
            .show(ctx, |d| {
                d.body(|ui| {
                    ui.label(format!("File: {:?}", modal.csv_path));
                    ui.separator();

                    if modal.is_parquet {
                        ui.checkbox(
                            &mut modal.config.parse_accounting_negatives,
                            "Parse Accounting Negatives (1,234.50 -> -1234.50)",
                        );
                        ui.label(
                            "Columns keep their Parquet type unless you change it. Text columns can be \
                             set to a number or date type; masks apply to any column.",
                        );
                    }
                    if !modal.is_parquet {
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(
                                &mut modal.config.sanitize_linebreaks,
                                "Sanitize Linebreaks (\\r, \\n) & Trim Whitespace",
                            );
                            ui.checkbox(
                                &mut modal.config.parse_accounting_negatives,
                                "Parse Accounting Negatives (1,234.50 -> -1234.50)",
                            );
                            ui.checkbox(
                                &mut modal.config.heal_broken_csv_lines,
                                "Pre-Heal Broken Unquoted CSV Lines",
                            );
                        });

                        ui.horizontal_wrapped(|ui| {
                            ui.label("Separator:");
                            let sep_label = |c: char| match c {
                                ',' => "Comma ,",
                                '|' => "Pipe |",
                                ';' => "Semicolon ;",
                                '\t' => "Tab",
                                _ => "Other",
                            };
                            egui::ComboBox::from_id_salt("csv_separator")
                                .selected_text(sep_label(modal.config.separator))
                                .show_ui(ui, |ui| {
                                    for c in [',', '|', ';', '\t'] {
                                        ui.selectable_value(&mut modal.config.separator, c, sep_label(c));
                                    }
                                });
                            ui.label("Title lines above header:");
                            ui.add(egui::DragValue::new(&mut modal.config.skip_rows).range(0..=100));
                            ui.label("Encoding:");
                            egui::ComboBox::from_id_salt("csv_encoding")
                                .selected_text(modal.config.encoding.label())
                                .show_ui(ui, |ui| {
                                    for enc in [CsvTextEncoding::Utf8, CsvTextEncoding::Windows1252] {
                                        ui.selectable_value(&mut modal.config.encoding, enc, enc.label());
                                    }
                                });
                            if ui
                                .button("\u{E094} Re-read with these settings")
                                .on_hover_text("Reloads the preview and re-detects the columns and their types")
                                .clicked()
                            {
                                action_reread = true;
                            }
                        });
                    }

                    ui.horizontal(|ui| {
                        if ui.button("\u{E248} Save Schema Template (.schema.json)").clicked() {
                            if let Some(save_path) = rfd::FileDialog::new()
                                .set_directory(&self.paths.schemas_dir)
                                .set_file_name("wb_schema.schema.json")
                                .save_file()
                            {
                                if let Ok(json) = serde_json::to_string_pretty(&modal.config) {
                                    let _ = std::fs::write(save_path, json);
                                    self.status_message = "Saved schema template.".to_string();
                                }
                            }
                        }

                        if ui.button("\u{E256} Load Saved Schema Template").clicked() {
                            if let Some(load_path) = rfd::FileDialog::new()
                                .set_directory(&self.paths.schemas_dir)
                                .add_filter("Schema JSON", &["json"])
                                .pick_file()
                            {
                                if let Ok(txt) = std::fs::read_to_string(load_path) {
                                    if let Ok(loaded) = serde_json::from_str::<IngestionConfig>(&txt) {
                                        modal.config = loaded;
                                    }
                                }
                            }
                        }
                    });

                    // PII masks use the per-install key (the schema's old salt is not used).
                    let key_path = pii_key_path();
                    ui.horizontal(|ui| {
                        ui.label("\u{E2D6} Mask Name / Mask Acct use this install's secret key (not a salt in the schema).");
                        match read_pii_key(&key_path) {
                            Ok(Some(k)) => {
                                ui.label(format!("Key fingerprint: {}", pii_key_fingerprint(&k)));
                            }
                            Ok(None) => {
                                ui.label("No key yet: it is created the first time a column is masked.");
                            }
                            Err(e) => {
                                ui.colored_label(theme::danger(ui), format!("{:#}", e));
                            }
                        }
                        if ui
                            .button("\u{E4C0} Export key...")
                            .on_hover_text("Saves the 32-byte key file. Keep it as safe as the data: anyone with it can check guesses against the masks.")
                            .clicked()
                        {
                            if let Some(dest) = rfd::FileDialog::new().set_file_name("pii_key.bin").save_file() {
                                self.status_message = match export_pii_key(&key_path, &dest) {
                                    Ok(_) => format!("Exported the PII key to {:?}. Keep it safe.", dest),
                                    Err(e) => format!("Key export failed: {:#}", e),
                                };
                            }
                        }
                        if ui
                            .button("\u{E20C} Import key...")
                            .on_hover_text("Replaces this install's key (the current one is backed up next to it).")
                            .clicked()
                        {
                            if let Some(src) = rfd::FileDialog::new().pick_file() {
                                self.status_message = match import_pii_key(&key_path, &src) {
                                    Ok(bak) => format!(
                                        "Imported the PII key from {:?}.{} Open tabs use it on their next refresh.",
                                        src,
                                        bak.map(|b| format!(" The previous key was saved as {:?}.", b))
                                            .unwrap_or_default()
                                    ),
                                    Err(e) => format!("Key import failed: {:#}", e),
                                };
                            }
                        }
                    });
                    ui.colored_label(
                        theme::warning(ui),
                        "\u{E4E0} Masks made with different keys don't match. To get the same masks on another PC, export the key here and import it there.",
                    );

                    ui.separator();
                    ui.strong(if modal.is_parquet {
                        "1. First 5 Rows:"
                    } else {
                        "1. Raw 5-Row Text Preview:"
                    });
                    egui::ScrollArea::vertical()
                        .id_salt("raw_preview_scroll")
                        .max_height(130.0)
                        .show(ui, |ui| {
                            Self::render_data_grid(
                                ui,
                                &modal.raw_preview_df,
                                &HashMap::new(),
                                0,
                                NumberDisplayFormat::Raw,
                                2,
                                0,
                                None,
                                &HashMap::new(),
                                None,
                            );
                        });

                    ui.separator();
                    ui.strong("2. Set Specific Data Type & Optional PII Masking per Column:");

                    egui::ScrollArea::vertical()
                        .id_salt("schema_override_scroll")
                        .max_height(240.0)
                        .show(ui, |ui| {
                            egui::Grid::new("schema_grid")
                                .striped(true)
                                .min_col_width(150.0)
                                .show(ui, |ui| {
                                    ui.strong("Column Name");
                                    ui.strong("Target Data Type");
                                    ui.strong("PII Anonymizer");
                                    ui.strong("Dr/Cr Sign");
                                    ui.strong("Parse Failures");
                                    ui.strong("Sample Values");
                                    ui.end_row();

                                    for (i, col_cfg) in modal.config.columns.iter_mut().enumerate() {
                                        if col_cfg.has_dr_cr
                                            && col_cfg.sign_convention == SignConvention::Unset
                                        {
                                            ui.colored_label(
                                                theme::warning(ui),
                                                format!("\u{E4E0} {} (has Dr/Cr)", col_cfg.name),
                                            )
                                            .on_hover_text(
                                                "Values carry Dr/Cr. Choose 'Dr is +' or 'Cr is +' or the load stops.",
                                            );
                                        } else if modal.is_parquet {
                                            ui.label(format!("{}  [{}]", col_cfg.name, col_cfg.source_dtype));
                                        } else {
                                            ui.label(&col_cfg.name);
                                        }
                                        // Parquet types with no counterpart here (Boolean, List...)
                                        // stay as they are.
                                        let convertible = !modal.is_parquet
                                            || parquet_native_type(&col_cfg.source_dtype).is_some();
                                        ui.add_enabled_ui(convertible, |ui| {
                                            egui::ComboBox::from_id_salt(format!("dtype_{}", i))
                                                .selected_text(col_cfg.dtype.label())
                                                .show_ui(ui, |ui| {
                                                    for dt in ColDataType::all() {
                                                        ui.selectable_value(
                                                            &mut col_cfg.dtype,
                                                            *dt,
                                                            dt.label(),
                                                        );
                                                    }
                                                });
                                        });
                                        ui.add_enabled_ui(convertible, |ui| {
                                            egui::ComboBox::from_id_salt(format!("pii_{}", i))
                                                .selected_text(col_cfg.pii_mask.label())
                                                .show_ui(ui, |ui| {
                                                    for mode in [
                                                        PiiMaskMode::None,
                                                        PiiMaskMode::ClientNameHash,
                                                        PiiMaskMode::AccountNumber,
                                                    ] {
                                                        ui.selectable_value(
                                                            &mut col_cfg.pii_mask,
                                                            mode,
                                                            mode.label(),
                                                        );
                                                    }
                                                });
                                        });
                                        if col_cfg.dtype == ColDataType::Float64 {
                                            egui::ComboBox::from_id_salt(format!("sign_{}", i))
                                                .selected_text(col_cfg.sign_convention.label())
                                                .show_ui(ui, |ui| {
                                                    for sc in SignConvention::all() {
                                                        ui.selectable_value(
                                                            &mut col_cfg.sign_convention,
                                                            *sc,
                                                            sc.label(),
                                                        );
                                                    }
                                                });
                                        } else {
                                            ui.label("—");
                                        }
                                        match modal
                                            .parse_failures
                                            .iter()
                                            .find(|f| f.column == col_cfg.name)
                                        {
                                            Some(f) => {
                                                ui.colored_label(
                                                    theme::warning(ui),
                                                    format!("\u{E4E0} {}", f.count),
                                                )
                                                .on_hover_text(format!(
                                                    "Could not be converted (now null), e.g.:\n{}",
                                                    f.examples.join("\n")
                                                ));
                                            }
                                            None => {
                                                ui.label("");
                                            }
                                        }
                                        ui.label(
                                            col_cfg
                                                .sample_values
                                                .iter()
                                                .take(3)
                                                .cloned()
                                                .collect::<Vec<_>>()
                                                .join(" | "),
                                        );
                                        ui.end_row();
                                    }
                                });
                        });

                });
                d.footer(|ui| {
                    ui.horizontal(|ui| {
                        if modal.target_tab.is_some() {
                            if theme::primary_button(ui, "\u{E182} Apply Schema to This Tab").clicked() {
                                action_apply_to_tab = true;
                                keep_open = false;
                            }
                        } else if !modal.direct_convert_only
                            && theme::primary_button(ui, "\u{E182} Lock Schema & Open in Viewer Tab").clicked()
                        {
                            action_open_tab = true;
                            keep_open = false;
                        }
                        if !modal.is_parquet
                            && ui
                                .button("\u{E2DE} Convert Directly to Parquet File Now...")
                                .clicked()
                        {
                            action_direct_convert = true;
                            keep_open = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep_open = false;
                        }
                    });
                });
            });

        if action_reread {
            match (
                preview_csv_with(&modal.csv_path, 5, &modal.config),
                auto_detect_smart_schema_with(&modal.csv_path, &modal.config),
            ) {
                (Ok(preview), Ok(cfg)) => {
                    modal.raw_preview_df = preview;
                    modal.config = cfg;
                    modal.parse_failures.clear();
                    self.status_message = "Re-read the file with the new settings.".into();
                }
                (Err(e), _) | (_, Err(e)) => {
                    self.status_message = format!("Could not re-read the file: {:#}", e);
                }
            }
            self.pending_csv_modal = Some(modal);
        } else if action_apply_to_tab {
            if let Some(idx) = modal.target_tab {
                self.apply_schema_to_tab(idx, modal.config);
            }
        } else if action_open_tab {
            let title = modal
                .csv_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            self.create_tab_from_source(
                title,
                TabSource::SingleFile {
                    path: modal.csv_path,
                    is_parquet: false,
                    config: modal.config,
                },
                Vec::new(),
                HashMap::new(),
            );
        } else if action_direct_convert {
            if let Some(out_dir) = rfd::FileDialog::new().pick_folder() {
                let stem = modal
                    .csv_path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy();
                let out_path = out_dir.join(format!("{}.parquet", stem));
                let (csv_path, config) = (modal.csv_path.clone(), modal.config);
                let (temp, dialect) = (self.paths.temp_dir.clone(), self.parquet_dialect);
                let label = format!("Converting {}", crate::ui::export::file_label(&csv_path));
                self.jobs.submit(JobKey::unique(JobKind::Export), label, true, move |ctx| {
                    convert_csv_to_parquet(&csv_path, config, &temp, &out_path, dialect, ctx)
                        .map(JobOutput::Exported)
                });
                self.status_message = "Conversion started…".to_string();
            }
        } else if keep_open {
            self.pending_csv_modal = Some(modal);
        }
    }

    /// Reopens the schema modal for the active single-file tab, with its parse failures.
    pub(crate) fn open_schema_modal_for_active_tab(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let TabSource::SingleFile {
            path,
            is_parquet,
            config,
        } = &tab.source
        else {
            return;
        };
        let loaded = if *is_parquet {
            // Parquet: list every column with the file's own type. A saved schema keeps the
            // user's choices; the file types and samples are always read fresh.
            preview_parquet_raw(path, 5).and_then(|preview| {
                let detected = auto_detect_parquet_schema(path)?;
                let cfg = if config.columns.is_empty() {
                    detected
                } else {
                    let mut cfg = config.clone();
                    for c in cfg.columns.iter_mut() {
                        if let Some(d) = detected.columns.iter().find(|d| d.name == c.name) {
                            c.source_dtype = d.source_dtype.clone();
                            c.sample_values = d.sample_values.clone();
                        }
                    }
                    cfg
                };
                Ok((preview, cfg))
            })
        } else {
            preview_csv_with(path, 5, config).map(|p| (p, config.clone()))
        };
        match loaded {
            Ok((preview, cfg)) => {
                self.pending_csv_modal = Some(PendingCsvModal {
                    csv_path: path.clone(),
                    raw_preview_df: preview,
                    config: cfg,
                    direct_convert_only: false,
                    target_tab: Some(self.active_tab),
                    parse_failures: tab.parse_failures.clone(),
                    is_parquet: *is_parquet,
                });
            }
            Err(e) => self.status_message = format!("Error reading the file preview: {:#}", e),
        }
    }

    /// Replaces the ingestion settings of an open tab, keeping its steps, and reloads it. The
    /// check runs on the worker thread; the tab is only changed when it passes.
    pub(crate) fn apply_schema_to_tab(&mut self, idx: usize, config: IngestionConfig) {
        let Some(tab) = self.tabs.get(idx) else {
            return;
        };
        let TabSource::SingleFile {
            path, is_parquet, ..
        } = &tab.source
        else {
            return;
        };
        let new_source = TabSource::SingleFile {
            path: path.clone(),
            is_parquet: *is_parquet,
            config,
        };
        let req = SchemaRequest {
            tab_id: tab.id,
            new_source,
            steps: tab.steps.clone(),
            temp_dir: self.paths.temp_dir.clone(),
        };
        let key = JobKey::tab(JobKind::Schema, tab.id);
        self.jobs.submit(key, "Applying schema", true, move |ctx| {
            check_schema_for_tab(req, ctx).map(|s| JobOutput::Schema(Box::new(s)))
        });
        self.status_message = "Applying the schema…".to_string();
    }
}
