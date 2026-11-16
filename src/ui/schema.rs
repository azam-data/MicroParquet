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
            "🧬 Schema & Ingestion (Parquet): Types & PII Masking"
        } else {
            "🧬 Schema & Ingestion: CSV 5-Row Raw Preview, Linebreak Sanitizer & Strict Schema Lock"
        };
        egui::Window::new(title)
            .collapsible(false)
            .resizable(true)
            .default_width(980.0)
            .show(ctx, |ui| {
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
                            .button("🔄 Re-read with these settings")
                            .on_hover_text("Reloads the preview and re-detects the columns and their types")
                            .clicked()
                        {
                            action_reread = true;
                        }
                    });
                }

                ui.horizontal(|ui| {
                    if ui.button("💾 Save Schema Template (.schema.json)").clicked() {
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

                    if ui.button("📂 Load Saved Schema Template").clicked() {
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
                    ui.label("🔑 Mask Name / Mask Acct use this install's secret key (not a salt in the schema).");
                    match read_pii_key(&key_path) {
                        Ok(Some(k)) => {
                            ui.label(format!("Key fingerprint: {}", pii_key_fingerprint(&k)));
                        }
                        Ok(None) => {
                            ui.label("No key yet: it is created the first time a column is masked.");
                        }
                        Err(e) => {
                            ui.colored_label(theme::ERR_RED, format!("{:#}", e));
                        }
                    }
                    if ui
                        .button("📤 Export key...")
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
                        .button("📥 Import key...")
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
                    theme::WARN_ORANGE,
                    "⚠ Masks made with different keys don't match. To get the same masks on another PC, export the key here and import it there.",
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
                                            theme::WARN_AMBER,
                                            format!("⚠ {} (has Dr/Cr)", col_cfg.name),
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
                                                theme::WARN_ORANGE,
                                                format!("⚠ {}", f.count),
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

                ui.separator();
                ui.horizontal(|ui| {
                    if modal.target_tab.is_some() {
                        if ui.button("✅ Apply Schema to This Tab").clicked() {
                            action_apply_to_tab = true;
                            keep_open = false;
                        }
                    } else if !modal.direct_convert_only
                        && ui.button("✅ Lock Schema & Open in Viewer Tab").clicked()
                    {
                        action_open_tab = true;
                        keep_open = false;
                    }
                    if !modal.is_parquet
                        && ui
                            .button("⚡ Convert Directly to Parquet File Now...")
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
                let src = TabSource::SingleFile {
                    path: modal.csv_path.clone(),
                    is_parquet: false,
                    config: modal.config,
                };
                match compile_tab_lazy(&src, &[], &self.paths.temp_dir)
                    .and_then(|lf| export_to_parquet_stream(lf, &out_path, self.parquet_dialect))
                {
                    Ok(_) => {
                        let orig_mb = modal
                            .csv_path
                            .metadata()
                            .map(|m| m.len() as f64 / 1_048_576.0)
                            .unwrap_or(0.0);
                        let new_mb = out_path
                            .metadata()
                            .map(|m| m.len() as f64 / 1_048_576.0)
                            .unwrap_or(0.0);
                        self.status_message = format!(
                            "✅ Converted to {:?}! ({:.1} MB -> {:.1} MB)",
                            out_path, orig_mb, new_mb
                        );
                    }
                    Err(e) => {
                        self.status_message = format!("❌ Direct conversion error: {:#}", e);
                    }
                }
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

    /// Replaces the ingestion settings of an open tab, keeping its steps, and reloads it.
    pub(crate) fn apply_schema_to_tab(&mut self, idx: usize, config: IngestionConfig) {
        let Some(tab) = self.tabs.get_mut(idx) else {
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
        let key_note = match ensure_pii_key_for(&new_source) {
            Ok(n) => n,
            Err(e) => {
                self.status_message = format!("Schema not applied: {:#}", e);
                return;
            }
        };
        if let Err(e) = compile_tab_lazy(&new_source, &tab.steps, &self.paths.temp_dir) {
            self.status_message = format!("Schema not applied: {:#}", e);
            return;
        }
        match source_parse_failures(&new_source, &self.paths.temp_dir) {
            Ok(f) => tab.parse_failures = f,
            Err(e) => {
                tab.parse_failures.clear();
                self.status_message = format!("⚠ Could not count parse failures: {:#}", e);
            }
        }
        let row_check = source_row_check(&new_source, &self.paths.temp_dir);
        tab.source = new_source;
        let mut row_note = String::new();
        match row_check {
            Ok(c) => {
                if let Some(w) = c.as_ref().and_then(|c| c.warning()) {
                    row_note = format!(" ⚠ {}", w);
                }
                tab.row_check = c;
            }
            Err(e) => {
                tab.row_check = None;
                row_note = format!(" ⚠ Could not compare rows with the file's lines: {:#}", e);
            }
        }
        self.active_tab = idx;
        self.refresh_active_tab(true);
        self.status_message = format!("Schema applied and tab reloaded.{}", row_note);
        if let Some(n) = key_note {
            self.status_message.push_str(&format!(" 🔑 {}", n));
        }
    }
}
