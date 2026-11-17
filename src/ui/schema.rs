//! Screen module `schema` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

pub const SCHEMA_TITLE_PARQUET: &str = "\u{E434} Schema & Ingestion (Parquet): Types & PII Masking";
pub const SCHEMA_TITLE_CSV: &str = "\u{E434} Schema & Ingestion: CSV 5-Row Raw Preview, Linebreak Sanitizer & Strict Schema Lock";

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
    /// The dialog is shown in its own native window (an egui immediate viewport) instead of
    /// inside the app window.
    pub popped_out: bool,
}

/// What the user asked for in one frame of the schema dialog (header buttons, Enter, Esc).
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct SchemaOutcome {
    pub reread: bool,
    /// "Lock Schema & Open" (or "Apply Schema to This Tab" when the dialog edits an open tab).
    pub lock: bool,
    pub convert: bool,
    pub cancel: bool,
    pub toggle_pop: bool,
}

impl SchemaOutcome {
    pub fn any(&self) -> bool {
        *self != Self::default()
    }
}

/// The keys of this frame, read before the dialog is drawn: a text field or an open list that
/// takes Enter or Esc itself must not also close the dialog.
#[derive(Clone, Copy, Debug)]
pub struct SchemaKeys {
    pub enter: bool,
    pub esc: bool,
}

impl SchemaKeys {
    pub fn read(ctx: &egui::Context) -> Self {
        let busy = ctx.wants_keyboard_input() || ctx.memory(|m| m.any_popup_open());
        let focused = ctx.memory(|m| m.focused().is_some());
        let (enter, esc) = ctx.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
        // a focused button takes Enter itself
        Self { enter: enter && !busy && !focused, esc: esc && !busy }
    }

    /// No keys while another overlay (the Ctrl+K search) has them.
    pub fn unless(self, overlay_open: bool) -> Self {
        if overlay_open {
            Self { enter: false, esc: false }
        } else {
            self
        }
    }

    /// Enter is "Lock Schema & Open" (the conversion when that is all the dialog offers), Esc is
    /// Cancel. A click on a button in the same frame wins.
    pub fn apply(&self, modal: &PendingCsvModal, out: &mut SchemaOutcome) {
        if out.any() {
            return;
        }
        if self.esc {
            out.cancel = true;
        } else if self.enter {
            if modal.direct_convert_only && !modal.is_parquet {
                out.convert = true;
            } else {
                out.lock = true;
            }
        }
    }
}

/// The buttons at the right of the dialog's header, in the order they read left to right:
/// [Pop out] [Lock Schema & Open] [Convert to Parquet…] [Cancel]. Returns the row's rectangle.
pub fn schema_header(ui: &mut egui::Ui, modal: &PendingCsvModal, can_pop: bool, out: &mut SchemaOutcome) -> egui::Rect {
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // added right to left
            if ui.button("Cancel").on_hover_text("Close without changes (Esc)").clicked() {
                out.cancel = true;
            }
            if !modal.is_parquet
                && ui
                    .button("\u{E2DE} Convert to Parquet\u{2026}")
                    .on_hover_text("Convert the file to Parquet now, without opening a tab")
                    .clicked()
            {
                out.convert = true;
            }
            if modal.target_tab.is_some() {
                if theme::primary_button(ui, "\u{E182} Apply Schema to This Tab").on_hover_text("Apply and reload the tab (Enter)").clicked() {
                    out.lock = true;
                }
            } else if !modal.direct_convert_only
                && theme::primary_button(ui, "\u{E182} Lock Schema & Open").on_hover_text("Lock this schema and open the file in a viewer tab (Enter)").clicked()
            {
                out.lock = true;
            }
            if can_pop || modal.popped_out {
                let (label, tip) = if modal.popped_out {
                    (format!("{} Dock", egui_phosphor::regular::ARROW_SQUARE_IN), "Put the dialog back inside the app window")
                } else {
                    (format!("{} Pop out", egui_phosphor::regular::ARROW_SQUARE_OUT), "Show this dialog in its own window, which you can move to another monitor or maximise")
                };
                if ui.button(label).on_hover_text(tip).clicked() {
                    out.toggle_pop = true;
                }
            }
        });
    })
    .response
    .rect
}

impl MicroParquetApp {
    pub(crate) fn render_csv_schema_modal(&mut self, ctx: &egui::Context) {
        let Some(mut modal) = self.pending_csv_modal.take() else {
            return;
        };

        let mut out = SchemaOutcome::default();
        let title = if modal.is_parquet { SCHEMA_TITLE_PARQUET } else { SCHEMA_TITLE_CSV };
        if modal.popped_out {
            self.show_schema_viewport(ctx, &mut modal, title, &mut out);
        } else {
            let keys = SchemaKeys::read(ctx).unless(self.command_open);
            // a backend that cannot open native windows keeps the dialog in the app
            let can_pop = !ctx.embed_viewports();
            theme::modal(title)
                .resizable(true)
                .default_width(theme::SCHEMA_DIALOG_W)
                .min_width(theme::SCHEMA_DIALOG_MIN_W)
                .fill_height()
                .show(ctx, |d| {
                    d.header(|ui| schema_header(ui, &modal, can_pop, &mut out));
                    let view_h = d.body_max_h();
                    d.body_scroll(|ui| {
                        self.schema_cards(ui, &mut modal, &mut out.reread, view_h);
                    });
                });
            keys.apply(&modal, &mut out);
        }

        let action_reread = out.reread;
        let action_apply_to_tab = out.lock && modal.target_tab.is_some();
        let action_open_tab = out.lock && modal.target_tab.is_none();
        let action_direct_convert = out.convert;
        let keep_open = !(out.lock || out.convert || out.cancel);
        if out.toggle_pop && keep_open {
            modal.popped_out = !modal.popped_out;
        }

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

    /// The dialog in its own native window (an immediate viewport): the same header, cards and
    /// keys as the in-app dialog. Closing the window docks the dialog back into the app.
    fn show_schema_viewport(&mut self, ctx: &egui::Context, modal: &mut PendingCsvModal, title: &str, out: &mut SchemaOutcome) {
        let builder = egui::ViewportBuilder::default()
            .with_title(title.to_string())
            .with_inner_size(theme::SCHEMA_POPOUT_SIZE)
            .with_min_inner_size(theme::SCHEMA_POPOUT_MIN_SIZE)
            .with_resizable(true)
            .with_maximize_button(true)
            .with_minimize_button(true);
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("schema_popout"), builder, |ctx, class| {
            if class == egui::ViewportClass::Embedded || ctx.input(|i| i.viewport().close_requested()) {
                // no native windows on this backend, or the window was closed: back into the app
                out.toggle_pop = true;
                return;
            }
            self.schema_viewport_ui(ctx, modal, out);
        });
    }

    /// What fills the pop-out window: the header buttons, then the cards in a middle that
    /// scrolls. (Tests draw it in a headless window the size of the pop-out.)
    pub(crate) fn schema_viewport_ui(&mut self, ctx: &egui::Context, modal: &mut PendingCsvModal, out: &mut SchemaOutcome) -> SchemaCardRects {
        let keys = SchemaKeys::read(ctx).unless(self.command_open);
        let mut rects = SchemaCardRects { reading: egui::Rect::NOTHING, preview: egui::Rect::NOTHING, columns: egui::Rect::NOTHING };
        egui::CentralPanel::default().show(ctx, |ui| {
            schema_header(ui, modal, true, out);
            ui.add_space(theme::SPACE_S);
            let view_h = ui.available_height();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
                    rects = self.schema_cards(ui, modal, &mut out.reread, view_h);
                });
        });
        keys.apply(modal, out);
        rects
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
                    popped_out: false,
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

/// Where the three cards of the schema dialog were drawn (tests check how they share the height).
#[derive(Clone, Copy, Debug)]
pub struct SchemaCardRects {
    pub reading: egui::Rect,
    pub preview: egui::Rect,
    pub columns: egui::Rect,
}

impl MicroParquetApp {
    /// The schema dialog's middle: "Reading options", "Preview" (a fixed 5 rows) and "Columns",
    /// which takes the height that is left, with its heading row fixed while the rows scroll.
    pub(crate) fn schema_cards(&mut self, ui: &mut egui::Ui, modal: &mut PendingCsvModal, reread: &mut bool, view_h: f32) -> SchemaCardRects {
        let tk = *theme::tokens(ui.visuals().dark_mode);
        let none = egui::Rect::NOTHING;
        let mut rects = SchemaCardRects { reading: none, preview: none, columns: none };

        // ---- Reading options: two compact rows
        rects.reading = theme::card(ui, "Reading options", |ui| {
            if modal.is_parquet {
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut modal.config.parse_accounting_negatives, "Accounting negatives")
                        .on_hover_text("Parse Accounting Negatives (1,234.50 -> -1234.50)"); // not a size: example text
                    theme::info_icon(ui, "Columns keep their Parquet type unless you change it. Text columns can be set to a number or date type; masks apply to any column.");
                });
            } else {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = theme::SCHEMA_ROW_GAP;
                    ui.checkbox(&mut modal.config.sanitize_linebreaks, "Clean line breaks")
                        .on_hover_text("Sanitize Linebreaks (\\r, \\n) & Trim Whitespace");
                    ui.checkbox(&mut modal.config.parse_accounting_negatives, "Accounting negatives")
                        .on_hover_text("Parse Accounting Negatives (1,234.50 -> -1234.50)"); // not a size: example text
                    ui.checkbox(&mut modal.config.heal_broken_csv_lines, "Heal broken lines")
                        .on_hover_text("Pre-Heal Broken Unquoted CSV Lines");
                    theme::dot(ui);
                    theme::row_label(ui, egui::RichText::new("Separator"));
                    let sep_label = |c: char| match c {
                        ',' => "Comma ,",
                        '|' => "Pipe |",
                        ';' => "Semicolon ;",
                        '\t' => "Tab",
                        _ => "Other",
                    };
                    theme::sized_combo(ui, theme::SCHEMA_COMBO_W, |ui| {
                        egui::ComboBox::from_id_salt("csv_separator")
                            .width(theme::SCHEMA_COMBO_W)
                            .selected_text(sep_label(modal.config.separator))
                            .show_ui(ui, |ui| {
                                for c in [',', '|', ';', '\t'] {
                                    ui.selectable_value(&mut modal.config.separator, c, sep_label(c));
                                }
                            });
                    });
                    theme::dot(ui);
                    theme::row_label(ui, egui::RichText::new("Title lines"));
                    ui.add(egui::DragValue::new(&mut modal.config.skip_rows).range(0..=100))
                        .on_hover_text("Lines above the header row that are skipped");
                    theme::dot(ui);
                    theme::row_label(ui, egui::RichText::new("Encoding"));
                    theme::sized_combo(ui, theme::SCHEMA_COMBO_W, |ui| {
                        egui::ComboBox::from_id_salt("csv_encoding")
                            .width(theme::SCHEMA_COMBO_W)
                            .selected_text(modal.config.encoding.label())
                            .show_ui(ui, |ui| {
                                for enc in [CsvTextEncoding::Utf8, CsvTextEncoding::Windows1252] {
                                    ui.selectable_value(&mut modal.config.encoding, enc, enc.label());
                                }
                            });
                    });
                    if ui
                        .button("\u{E094} Re-read")
                        .on_hover_text("Reloads the preview and re-detects the columns and their types with these settings")
                        .clicked()
                    {
                        *reread = true;
                    }
                });
            }

            // row 2: template and masking key
            let key_path = pii_key_path();
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SCHEMA_ROW_GAP;
                theme::row_label(ui, egui::RichText::new("Template:").color(tk.muted));
                if ui.button("\u{E248} Save").on_hover_text("Save Schema Template (.schema.json)").clicked() {
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
                if ui.button("\u{E256} Load").on_hover_text("Load Saved Schema Template").clicked() {
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
                theme::dot(ui);
                // PII masks use the per-install key (the schema's old salt is not used).
                theme::row_label(ui, egui::RichText::new("Masking key:").color(tk.muted));
                match read_pii_key(&key_path) {
                    Ok(Some(k)) => {
                        theme::row_label(ui, theme::mono(pii_key_fingerprint(&k)));
                    }
                    Ok(None) => {
                        theme::row_label(ui, egui::RichText::new("not created")).on_hover_text("It is created the first time a column is masked.");
                    }
                    Err(e) => {
                        ui.colored_label(theme::danger(ui), format!("{:#}", e));
                    }
                }
                if ui
                    .button("\u{E4C0} Export")
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
                    .button("\u{E20C} Import")
                    .on_hover_text("Replaces this install's key (the current one is backed up next to it).")
                    .clicked()
                {
                    if let Some(src) = rfd::FileDialog::new().pick_file() {
                        self.status_message = match import_pii_key(&key_path, &src) {
                            Ok(bak) => format!(
                                "Imported the PII key from {:?}.{} Open tabs use it on their next refresh.",
                                src,
                                bak.map(|b| format!(" The previous key was saved as {:?}.", b)).unwrap_or_default()
                            ),
                            Err(e) => format!("Key import failed: {:#}", e),
                        };
                    }
                }
                theme::info_icon(
                    ui,
                    "Mask Name / Mask Acct use this install's secret key (it is not a salt in the schema).\n\nMasks made with different keys don't match. To get the same masks on another PC, export the key here and import it there.",
                );
            });
        });

        ui.add_space(theme::SPACE_S);

        // ---- Columns: the rest of the height (the Preview card under it keeps its own)
        // the rest of the visible height, but never less than a list worth looking at
        let used = ui.cursor().top() - ui.min_rect().top();
        let preview_id = egui::Id::new("schema_preview_card_h");
        let preview_h = ui.ctx().data(|d| d.get_temp::<f32>(preview_id)).unwrap_or_else(|| {
            // until the card has been drawn once: its padding, heading row and grid
            2.0 * theme::SPACE_M + 2.0 + ui.spacing().interact_size.y + ui.spacing().item_spacing.y + theme::PREVIEW_GRID_H
        });
        let gap = theme::SPACE_S + ui.spacing().item_spacing.y; // between two cards
        let columns_h = (view_h - used - gap - preview_h).max(theme::SCHEMA_COLUMNS_MIN_H);
        rects.columns = theme::card_height(ui, "Columns", columns_h, |ui| {
            let n = modal.config.columns.len();
            let is_parquet = modal.is_parquet;
            let failures = &modal.parse_failures;
            let columns = &mut modal.config.columns;
            ui.push_id("schema_columns", |ui| {
              // a list wider than the card scrolls sideways inside it; the heading row stays above
              // the rows that scroll up and down
              egui::ScrollArea::horizontal().id_salt("schema_columns_x").auto_shrink([false, false]).show(ui, |ui| {
                let limit = ui.clip_rect();
                TableBuilder::new(ui)
                    .min_scrolled_height(0.0)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::initial(theme::SCHEMA_COL_NAME_W).at_least(theme::SCHEMA_COL_MIN_W).clip(true))
                    .column(Column::initial(theme::SCHEMA_COL_TYPE_W).at_least(theme::SCHEMA_COL_MIN_W))
                    .column(Column::initial(theme::SCHEMA_COL_PII_W).at_least(theme::SCHEMA_COL_MIN_W))
                    .column(Column::initial(theme::SCHEMA_COL_SIGN_W).at_least(theme::SCHEMA_COL_MIN_W))
                    .column(Column::initial(theme::SCHEMA_COL_FAIL_W).at_least(theme::SCHEMA_COL_MIN_W))
                    .column(Column::remainder().at_least(theme::SCHEMA_COL_MIN_W).clip(true))
                    .header(theme::HEADER_ROW_H, |mut header| {
                        for title in ["Column Name", "Target Data Type", "PII Anonymizer", "Dr/Cr Sign", "Parse Failures", "Sample Values"] {
                            header.col(|ui| theme::header_cell(ui, title, limit));
                        }
                    })
                    .body(|body| {
                        body.rows(theme::DATA_ROW_H, n, |mut row| {
                            let i = row.index();
                            let Some(col_cfg) = columns.get_mut(i) else {
                                return;
                            };
                            row.col(|ui| {
                                if col_cfg.has_dr_cr && col_cfg.sign_convention == SignConvention::Unset {
                                    ui.colored_label(theme::warning(ui), format!("\u{E4E0} {} (has Dr/Cr)", col_cfg.name))
                                        .on_hover_text("Values carry Dr/Cr. Choose 'Dr is +' or 'Cr is +' or the load stops.");
                                } else if is_parquet {
                                    ui.add(egui::Label::new(format!("{}  [{}]", col_cfg.name, col_cfg.source_dtype)).truncate());
                                } else {
                                    ui.add(egui::Label::new(&col_cfg.name).truncate());
                                }
                            });
                            // Parquet types with no counterpart here (Boolean, List...) stay as they are.
                            let convertible = !is_parquet || parquet_native_type(&col_cfg.source_dtype).is_some();
                            row.col(|ui| {
                                ui.add_enabled_ui(convertible, |ui| {
                                    egui::ComboBox::from_id_salt(format!("dtype_{}", i))
                                        .width(theme::SCHEMA_COMBO_W)
                                        .selected_text(col_cfg.dtype.label())
                                        .show_ui(ui, |ui| {
                                            for dt in ColDataType::all() {
                                                ui.selectable_value(&mut col_cfg.dtype, *dt, dt.label());
                                            }
                                        });
                                });
                            });
                            row.col(|ui| {
                                ui.add_enabled_ui(convertible, |ui| {
                                    egui::ComboBox::from_id_salt(format!("pii_{}", i))
                                        .width(theme::SCHEMA_COMBO_W)
                                        .selected_text(col_cfg.pii_mask.label())
                                        .show_ui(ui, |ui| {
                                            for mode in [PiiMaskMode::None, PiiMaskMode::ClientNameHash, PiiMaskMode::AccountNumber] {
                                                ui.selectable_value(&mut col_cfg.pii_mask, mode, mode.label());
                                            }
                                        });
                                });
                            });
                            row.col(|ui| {
                                if col_cfg.dtype == ColDataType::Float64 {
                                    egui::ComboBox::from_id_salt(format!("sign_{}", i))
                                        .width(theme::SCHEMA_COMBO_W)
                                        .selected_text(col_cfg.sign_convention.label())
                                        .show_ui(ui, |ui| {
                                            for sc in SignConvention::all() {
                                                ui.selectable_value(&mut col_cfg.sign_convention, *sc, sc.label());
                                            }
                                        });
                                } else {
                                    ui.label("\u{2014}");
                                }
                            });
                            row.col(|ui| {
                                if let Some(f) = failures.iter().find(|f| f.column == col_cfg.name) {
                                    ui.colored_label(theme::warning(ui), format!("\u{E4E0} {}", f.count)).on_hover_text(format!(
                                        "Could not be converted (now null), e.g.:\n{}",
                                        f.examples.join("\n")
                                    ));
                                }
                            });
                            row.col(|ui| {
                                let text = col_cfg.sample_values.iter().take(3).cloned().collect::<Vec<_>>().join(" | ");
                                let r = ui.add(egui::Label::new(theme::mono(text.clone())).truncate());
                                if !text.is_empty() {
                                    r.on_hover_text(text);
                                }
                            });
                        });
                    });
              });
            });
        });

        // ---- Preview: always 5 rows high, last
        ui.add_space(theme::SPACE_S);
        rects.preview = theme::card_with(
            ui,
            "Preview",
            |ui| {
                ui.add(egui::Label::new(egui::RichText::new(modal.csv_path.display().to_string()).color(tk.muted)).truncate())
                    .on_hover_text(modal.csv_path.display().to_string());
            },
            |ui| {
                // exactly this high, whatever the file has
                let (area, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), theme::PREVIEW_GRID_H),
                    egui::Sense::hover(),
                );
                ui.allocate_new_ui(
                    egui::UiBuilder::new().max_rect(area).layout(egui::Layout::top_down(egui::Align::Min)),
                    |ui| {
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
                    },
                );
            },
        );
        ui.ctx().data_mut(|d| d.insert_temp(preview_id, rects.preview.height()));
        rects
    }
}
