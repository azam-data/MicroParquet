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
    /// An export waiting for the Text-column check job (token of that job).
    pub pending: Option<PendingExport>,
    /// Question shown when a Text column of the export looks like numbers.
    pub text_guard: Option<TextGuard>,
    pub check_token: u64,
}

/// An export that starts when its Text-column check finds nothing (or the user decides).
#[derive(Clone)]
pub struct PendingExport {
    pub token: u64,
    pub kind: GuardedExport,
    pub sidecar: Option<Vec<ParseFailure>>,
}

/// The pre-export question about Text columns that look like numbers.
pub struct TextGuard {
    pub kind: GuardedExport,
    pub sidecar: Option<Vec<ParseFailure>>,
    pub findings: Vec<TabTextFindings>,
}

impl Default for ExportState {
    fn default() -> Self {
        Self {
            multi_checks: Vec::new(),
            split_col: "Brancd".to_string(),
            split_format: SplitExportFormat::Parquet,
            guard: None,
            override_: None,
            pending: None,
            text_guard: None,
            check_token: 0,
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
        theme::modal("\u{E150} Consolidated Multi-Tab Exporter (.xlsx / .pdf)")
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
                        .button("\u{E150} Export Selected Tabs to Single .xlsx...")
                        .clicked()
                    {
                        self.run_guarded_export(GuardedExport::MultiXlsx);
                        close_modal = true;
                    }

                    if ui
                        .button("\u{E702} Export Selected Tabs to Single Landscape .pdf...")
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
        theme::modal("\u{E710} Smart Partitioned Split Exporter")
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
                    if theme::primary_button(ui, "\u{E3FE} Select Output Folder & Split Now...").clicked() {
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
        // "Export anyway": the parse failures are listed next to the file by the export job.
        let sidecar = if forced { Some(self.failure_sidecar_rows(&tabs)) } else { None };
        self.start_text_check(kind, &tabs, sidecar);
    }

    /// Looks for Text columns that are mostly numbers before the export starts (a background
    /// job); `apply_text_check` continues with the export or asks.
    fn start_text_check(&mut self, kind: GuardedExport, tabs: &[usize], sidecar: Option<Vec<ParseFailure>>) {
        let list: Vec<(u64, String, TabSource, Vec<TransformStep>)> = tabs
            .iter()
            .filter_map(|i| self.tabs.get(*i))
            .map(|t| (t.id, t.title.clone(), t.source.clone(), t.steps.clone()))
            .collect();
        if list.is_empty() {
            self.perform_export(kind, sidecar);
            return;
        }
        self.export.check_token += 1;
        let token = self.export.check_token;
        self.export.pending = Some(PendingExport { token, kind, sidecar });
        let req = TextCheckRequest { token, tabs: list, temp_dir: self.paths.temp_dir.clone() };
        self.jobs.submit(JobKey::unique(JobKind::Stats), "Checking Text columns", true, move |ctx| {
            check_text_columns(req, ctx).map(|r| JobOutput::TextCheck(Box::new(r)))
        });
        self.status_message = "Checking Text columns before the export…".to_string();
    }

    /// The check finished: export, or ask about the columns that look like numbers.
    pub(crate) fn apply_text_check(&mut self, result: TextCheckResult) {
        let Some(p) = self.export.pending.take() else {
            return;
        };
        if p.token != result.token {
            self.export.pending = Some(p);
            return;
        }
        if result.findings.is_empty() {
            self.perform_export(p.kind, p.sidecar);
        } else {
            self.status_message = "Export paused: a Text column looks like numbers.".to_string();
            self.export.text_guard = Some(TextGuard { kind: p.kind, sidecar: p.sidecar, findings: result.findings });
        }
    }

    /// A failed or cancelled check stops the export (the reason is in the status bar).
    pub(crate) fn drop_pending_export(&mut self) {
        self.export.pending = None;
    }

    fn perform_export(&mut self, kind: GuardedExport, sidecar: Option<Vec<ParseFailure>>) {
        match kind {
            GuardedExport::Parquet => self.export_active_tab_parquet(sidecar),
            GuardedExport::Csv => self.export_active_tab_csv(sidecar),
            GuardedExport::MultiXlsx => self.export_multi_tab_xlsx(sidecar),
            GuardedExport::MultiPdf => self.export_multi_tab_pdf(sidecar),
            GuardedExport::Split => self.export_split_by_column(sidecar),
        }
    }

    /// The question: "<col> is Text, but 98% of its values look like numbers. Set it to Number
    /// before exporting?" with Set to Number / Export as is.
    pub(crate) fn render_text_number_guard(&mut self, ctx: &egui::Context) {
        let Some(g) = &self.export.text_guard else {
            return;
        };
        let mut lines: Vec<(String, String)> = Vec::new();
        for f in &g.findings {
            for c in &f.columns {
                lines.push((f.title.clone(), c.message()));
            }
        }
        let multi = g.findings.len() > 1;
        let mut set_number = false;
        let mut as_is = false;
        let mut cancel = false;
        theme::modal("\u{E4E0}  Text column that looks like numbers").show(ctx, |ui| {
            for (title, msg) in &lines {
                if multi {
                    ui.label(format!("{}: {}", title, msg));
                } else {
                    ui.label(msg.as_str());
                }
            }
            ui.add_space(theme::SPACE_S);
            ui.label("Numbers stored as Text sort wrongly, cannot be summed in Excel and show as text in Power BI.");
            ui.separator();
            ui.horizontal(|ui| {
                if theme::primary_button(ui, "Set to Number").clicked() {
                    set_number = true;
                }
                if ui.button("Export as is").clicked() {
                    as_is = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if set_number || as_is {
            if let Some(g) = self.export.text_guard.take() {
                if set_number {
                    let keep = self.active_tab;
                    for f in &g.findings {
                        let Some(idx) = self.tabs.iter().position(|t| t.id == f.tab_id) else {
                            continue;
                        };
                        self.active_tab = idx;
                        let columns = f.columns.iter().map(|c| c.column.clone()).collect();
                        self.push_step_to_active_tab(TransformStep::ConvertToNumber { columns }, false);
                    }
                    self.active_tab = keep.min(self.tabs.len().saturating_sub(1));
                }
                self.perform_export(g.kind, g.sidecar);
            }
        } else if cancel {
            self.export.text_guard = None;
            self.status_message = "Export cancelled: a Text column looks like numbers.".into();
        }
    }

    /// The parse failures of the exported tabs (a "tab / column" prefix when several tabs).
    pub(crate) fn failure_sidecar_rows(&self, tabs: &[usize]) -> Vec<ParseFailure> {
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
        rows
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
                        if ctrl { "\u{E18A}" } else { " " },
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
        theme::modal("\u{E4E0} Values could not be converted")
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
                ui.small("\u{E18A} = control column");
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

    fn export_tab_of(t: &TabState) -> ExportTab {
        ExportTab {
            title: t.title.clone(),
            source: t.source.clone(),
            steps: t.steps.clone(),
            col_formats: t.col_formats.clone(),
        }
    }

    fn submit_export(
        &mut self,
        label: String,
        work: impl FnOnce(&micro_parquet::jobs::JobCtx) -> anyhow::Result<micro_parquet::tab_jobs::ExportOutcome>
            + Send
            + 'static,
    ) {
        self.jobs.submit(JobKey::unique(JobKind::Export), label, true, move |ctx| {
            work(ctx).map(JobOutput::Exported)
        });
        self.status_message = "Export started…".to_string();
    }

    pub(crate) fn export_active_tab_parquet(&mut self, sidecar: Option<Vec<ParseFailure>>) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let Some(out_path) = rfd::FileDialog::new()
            .set_file_name(format!("{}.parquet", tab.title))
            .save_file()
        else {
            return;
        };
        let (source, steps) = (tab.source.clone(), tab.steps.clone());
        let (temp, dialect) = (self.paths.temp_dir.clone(), self.parquet_dialect);
        let label = format!("Parquet export {}", file_label(&out_path));
        self.submit_export(label, move |ctx| {
            export_tab_parquet(&source, &steps, &temp, &out_path, dialect, sidecar.as_deref(), ctx)
        });
    }

    pub(crate) fn export_active_tab_csv(&mut self, sidecar: Option<Vec<ParseFailure>>) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let Some(out_path) = rfd::FileDialog::new()
            .set_file_name(format!("{}.csv", tab.title))
            .save_file()
        else {
            return;
        };
        let (source, steps) = (tab.source.clone(), tab.steps.clone());
        let (temp, mdy) = (self.paths.temp_dir.clone(), self.export_csv_mdy);
        let label = format!("CSV export {}", file_label(&out_path));
        self.submit_export(label, move |ctx| {
            export_tab_csv(&source, &steps, &temp, &out_path, mdy, sidecar.as_deref(), ctx)
        });
    }

    fn ticked_export_tabs(&self) -> Vec<ExportTab> {
        self.tabs
            .iter()
            .enumerate()
            .filter(|(i, _)| self.export.multi_checks.get(*i).copied().unwrap_or(false))
            .map(|(_, t)| Self::export_tab_of(t))
            .collect()
    }

    pub(crate) fn export_multi_tab_xlsx(&mut self, sidecar: Option<Vec<ParseFailure>>) {
        let Some(out_path) = rfd::FileDialog::new()
            .set_file_name("Consolidated_ALCO_Views.xlsx")
            .save_file()
        else {
            return;
        };
        let tabs = self.ticked_export_tabs();
        let temp = self.paths.temp_dir.clone();
        let (fmt, dec) = (self.num_format, self.global_decimals);
        let label = format!("Excel export {}", file_label(&out_path));
        self.submit_export(label, move |ctx| {
            export_tabs_xlsx(&tabs, &temp, &out_path, fmt, dec, sidecar.as_deref(), ctx)
        });
    }

    pub(crate) fn export_multi_tab_pdf(&mut self, sidecar: Option<Vec<ParseFailure>>) {
        let Some(out_path) = rfd::FileDialog::new()
            .set_file_name("Consolidated_ALCO_Report.pdf")
            .save_file()
        else {
            return;
        };
        let tabs = self.ticked_export_tabs();
        let temp = self.paths.temp_dir.clone();
        let (fmt, dec) = (self.num_format, self.global_decimals);
        let label = format!("PDF export {}", file_label(&out_path));
        self.submit_export(label, move |ctx| {
            export_tabs_pdf(&tabs, &temp, &out_path, fmt, dec, sidecar.as_deref(), ctx)
        });
    }

    pub(crate) fn export_split_by_column(&mut self, sidecar: Option<Vec<ParseFailure>>) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let Some(out_dir) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let (source, steps) = (tab.source.clone(), tab.steps.clone());
        let control_col = tab.control_col.clone();
        let temp = self.paths.temp_dir.clone();
        let (split_col, format) = (self.export.split_col.clone(), self.export.split_format);
        let (dialect, mdy) = (self.parquet_dialect, self.export_csv_mdy);
        let label = format!("Split export to {}", file_label(&out_dir));
        self.submit_export(label, move |ctx| {
            export_split(
                &source, &steps, &temp, &out_dir, &split_col, format, dialect, mdy,
                control_col.as_deref(), sidecar.as_deref(), ctx,
            )
        });
    }
}

/// The file or folder name of a path, for job labels.
pub(crate) fn file_label(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}
