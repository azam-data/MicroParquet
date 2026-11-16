//! Screen module `export` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct ExportState {
    pub multi_checks: Vec<bool>,
    pub split_col: String,
    pub split_format: SplitExportFormat,
    /// Export waiting for "Export anyway" / "Cancel" because of parse failures.
    pub guard: Option<GuardedExport>,
    /// The user chose "Export anyway" for this export.
    pub override_: Option<GuardedExport>,
}

impl Default for ExportState {
    fn default() -> Self {
        Self {
            multi_checks: Vec::new(),
            split_col: "Brancd".to_string(),
            split_format: SplitExportFormat::Parquet,
            guard: None,
            override_: None,
        }
    }
}

/// Exports that stop for confirmation when the control column has parse failures.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GuardedExport {
    Parquet,
    Csv,
    MultiXlsx,
    MultiPdf,
    Split,
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_multi_export_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let step_to_apply: Option<(TransformStep, bool)> = None;
        egui::Window::new("📊 Consolidated Multi-Tab Exporter (.xlsx / .pdf)")
            .collapsible(false)
            .show(ctx, |ui| {
                ui.strong("Select Open Tabs / Views to Include:");
                if self.export.multi_checks.len() != self.tabs.len() {
                    self.export.multi_checks = vec![true; self.tabs.len()];
                }
                for (i, tab) in self.tabs.iter().enumerate() {
                    let ref mut is_checked = self.export.multi_checks[i];
                    ui.checkbox(
                        is_checked,
                        format!("Tab {}: {} ({} rows)", i + 1, tab.title, tab.total_rows),
                    );
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button("📊 Export Selected Tabs to Single .xlsx...")
                        .clicked()
                    {
                        self.run_guarded_export(GuardedExport::MultiXlsx);
                        close_modal = true;
                    }

                    if ui
                        .button("📄 Export Selected Tabs to Single Landscape .pdf...")
                        .clicked()
                    {
                        self.run_guarded_export(GuardedExport::MultiPdf);
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
    pub(crate) fn render_split_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let step_to_apply: Option<(TransformStep, bool)> = None;
        egui::Window::new("🗂 Smart Partitioned Split Exporter")
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Partition / Split Column:");
                    egui::ComboBox::from_id_salt("splt_col")
                        .selected_text(&self.export.split_col)
                        .show_ui(ui, |ui| {
                            for c in &avail_cols {
                                ui.selectable_value(&mut self.export.split_col, c.clone(), c);
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Output File Format:");
                    for fmt in [
                        SplitExportFormat::Parquet,
                        SplitExportFormat::Csv,
                        SplitExportFormat::Xlsx,
                    ] {
                        ui.selectable_value(&mut self.export.split_format, fmt, fmt.label());
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("🚀 Select Output Folder & Split Now...").clicked() {
                        self.run_guarded_export(GuardedExport::Split);
                        close_modal = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close_modal = true;
                    }
                });
            });
        ModalOutcome { close: close_modal, step: step_to_apply }
    }

    /// Tabs an export reads: the ticked tabs for multi-tab exports, else the active tab.
    pub(crate) fn guarded_export_tabs(&self, kind: GuardedExport) -> Vec<usize> {
        match kind {
            GuardedExport::MultiXlsx | GuardedExport::MultiPdf => (0..self.tabs.len())
                .filter(|i| self.export.multi_checks.get(*i).copied().unwrap_or(false))
                .collect(),
            _ => vec![self.active_tab],
        }
    }

    /// True when the tab's control column has values that could not be converted.
    pub(crate) fn control_column_has_failures(tab: &TabState) -> bool {
        tab.control_col
            .as_ref()
            .is_some_and(|c| tab.parse_failures.iter().any(|f| &f.column == c))
    }

    /// Runs an export, or stops it for confirmation when a control column has parse failures.
    /// After "Export anyway", `<output>.parse_failures.csv` is written next to the export.
    pub(crate) fn run_guarded_export(&mut self, kind: GuardedExport) {
        let forced = self.export.override_.take() == Some(kind);
        let tabs = self.guarded_export_tabs(kind);
        let blocked = tabs
            .iter()
            .filter_map(|i| self.tabs.get(*i))
            .any(Self::control_column_has_failures);
        if blocked && !forced {
            self.export.guard = Some(kind);
            return;
        }
        let written = match kind {
            GuardedExport::Parquet => self.export_active_tab_parquet(),
            GuardedExport::Csv => self.export_active_tab_csv(),
            GuardedExport::MultiXlsx => self.export_multi_tab_xlsx(),
            GuardedExport::MultiPdf => self.export_multi_tab_pdf(),
            GuardedExport::Split => self.export_split_by_column(),
        };
        if let (true, Some(out)) = (forced, written) {
            self.write_failure_sidecar(&out, &tabs);
        }
    }

    pub(crate) fn write_failure_sidecar(&mut self, output: &Path, tabs: &[usize]) {
        let with_failures: Vec<&TabState> = tabs
            .iter()
            .filter_map(|i| self.tabs.get(*i))
            .filter(|t| !t.parse_failures.is_empty())
            .collect();
        let prefix = with_failures.len() > 1;
        let mut rows: Vec<ParseFailure> = Vec::new();
        for t in with_failures {
            for f in &t.parse_failures {
                let mut f = f.clone();
                if prefix {
                    f.column = format!("{} / {}", t.title, f.column);
                }
                rows.push(f);
            }
        }
        match write_parse_failures_csv(output, &rows) {
            Ok(p) => self
                .status_message
                .push_str(&format!(" Parse failures listed in {:?}.", p)),
            Err(e) => self.status_message.push_str(&format!(" ⚠ {:#}", e)),
        }
    }

    pub(crate) fn render_export_guard(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.export.guard else {
            return;
        };
        let mut lines = Vec::new();
        for i in self.guarded_export_tabs(kind) {
            if let Some(t) = self.tabs.get(i) {
                for f in &t.parse_failures {
                    let ctrl = t.control_col.as_deref() == Some(f.column.as_str());
                    lines.push(format!(
                        "{} {} — {}: {} value(s), e.g. {}",
                        if ctrl { "●" } else { " " },
                        t.title,
                        f.column,
                        f.count,
                        f.examples.join(" | ")
                    ));
                }
            }
        }
        let mut go = false;
        let mut cancel = false;
        egui::Window::new("⚠ Values could not be converted")
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(
                    "The control column has values that could not be converted to their type. \
                     They are null in the export, so its totals will not match the source file.",
                );
                ui.separator();
                for l in &lines {
                    ui.monospace(l);
                }
                ui.small("● = control column");
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button("Export anyway")
                        .on_hover_text("Also writes <output>.parse_failures.csv next to the export")
                        .clicked()
                    {
                        go = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if go {
            self.export.guard = None;
            self.export.override_ = Some(kind);
            self.run_guarded_export(kind);
        } else if cancel {
            self.export.guard = None;
            self.status_message = "Export cancelled: the control column has parse failures.".into();
        }
    }

    pub(crate) fn export_active_tab_parquet(&mut self) -> Option<PathBuf> {
        let tab = self.tabs.get(self.active_tab)?;
        let out_path = rfd::FileDialog::new()
            .set_file_name(format!("{}.parquet", tab.title))
            .save_file()?;

        match compile_tab_lazy(&tab.source, &tab.steps, &self.paths.temp_dir)
            .and_then(|lf| export_to_parquet_stream(lf, &out_path, self.parquet_dialect))
        {
            Ok(_) => {
                self.status_message = format!("✅ Exported view to {:?}", out_path);
                Some(out_path)
            }
            Err(e) => {
                self.status_message = format!("❌ Parquet export failed: {:#}", e);
                None
            }
        }
    }

    pub(crate) fn export_active_tab_csv(&mut self) -> Option<PathBuf> {
        let tab = self.tabs.get(self.active_tab)?;
        let out_path = rfd::FileDialog::new()
            .set_file_name(format!("{}.csv", tab.title))
            .save_file()?;

        match compile_tab_lazy(&tab.source, &tab.steps, &self.paths.temp_dir)
            .and_then(|lf| export_to_csv_stream(lf, &out_path, self.export_csv_mdy))
        {
            Ok(_) => {
                self.status_message = format!("✅ Exported view to {:?}", out_path);
                Some(out_path)
            }
            Err(e) => {
                self.status_message = format!("❌ CSV export failed: {:#}", e);
                None
            }
        }
    }

    pub(crate) fn export_multi_tab_xlsx(&mut self) -> Option<PathBuf> {
        let out_path = rfd::FileDialog::new()
            .set_file_name("Consolidated_ALCO_Views.xlsx")
            .save_file()?;
        let selected: Vec<micro_parquet::exporters::TabToExport> = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(i, _)| self.export.multi_checks.get(*i).copied().unwrap_or(false))
            .map(|(_, t)| (t.title.clone(), &t.source, t.steps.as_slice()))
            .collect();
        match export_selected_tabs_to_xlsx(&selected, &out_path, &self.paths.temp_dir) {
            Ok(rep) => {
                self.status_message = format!(
                    "Exported multi-tab Excel workbook to {:?}: {}",
                    out_path,
                    rep.summary()
                );
                Some(out_path)
            }
            Err(e) => {
                self.status_message = format!("Excel export error: {:#}", e);
                None
            }
        }
    }

    pub(crate) fn export_multi_tab_pdf(&mut self) -> Option<PathBuf> {
        let out_path = rfd::FileDialog::new()
            .set_file_name("Consolidated_ALCO_Report.pdf")
            .save_file()?;
        let mut collected_tabs = Vec::new();
        let mut failed = Vec::new();
        for (i, tab) in self.tabs.iter().enumerate() {
            if self.export.multi_checks.get(i).copied().unwrap_or(false) {
                // One row past the limit, so the export can say the tab was cut.
                match compile_tab_lazy(&tab.source, &tab.steps, &self.paths.temp_dir).and_then(
                    |lf| Ok(lf.limit((PDF_MAX_ROWS_PER_TAB + 1) as u32).collect()?),
                ) {
                    Ok(df) => collected_tabs.push((tab.title.clone(), df, tab.col_formats.clone())),
                    Err(e) => failed.push(format!("'{}' ({:#})", tab.title, e)),
                }
            }
        }
        match export_tabs_to_pdf(
            collected_tabs,
            &out_path,
            self.num_format,
            self.global_decimals,
        ) {
            Ok(rep) => {
                let mut msg = format!("Exported multi-tab PDF report to {:?}", out_path);
                let s = rep.summary();
                if !s.is_empty() {
                    msg.push_str(&format!(" ⚠ {}", s));
                }
                if !failed.is_empty() {
                    msg.push_str(&format!(" ⚠ Not exported (failed): {}", failed.join("; ")));
                }
                self.status_message = msg;
                Some(out_path)
            }
            Err(e) => {
                self.status_message = format!("PDF export error: {:#}", e);
                None
            }
        }
    }

    /// Returns `<folder>/split_export` as the base name for the parse-failure list.
    pub(crate) fn export_split_by_column(&mut self) -> Option<PathBuf> {
        let tab = self.tabs.get(self.active_tab)?;
        let out_dir = rfd::FileDialog::new().pick_folder()?;
        let control_col = tab.control_col.clone();
        let lf = match compile_tab_lazy(&tab.source, &tab.steps, &self.paths.temp_dir) {
            Ok(lf) => lf,
            Err(e) => {
                self.status_message = format!("Split export failed: {:#}", e);
                return None;
            }
        };
        match split_and_export_by_column(
            lf,
            &self.export.split_col,
            &out_dir,
            self.export.split_format,
            self.parquet_dialect,
            self.export_csv_mdy,
            control_col.as_deref(),
        ) {
            Ok(cnt) => {
                self.status_message = format!(
                    "Split & exported {} individual files into {:?} (rows and control sums in split_manifest.csv)",
                    cnt, out_dir
                );
                Some(out_dir.join("split_export"))
            }
            Err(e) => {
                self.status_message = format!("Split export failed: {:#}", e);
                None
            }
        }
    }
}
