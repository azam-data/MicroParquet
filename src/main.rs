#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod headless;
mod ui;

use clap::Parser;
use ui::prelude::*;

#[derive(Parser, Debug)]
#[command(
    name = "MicroParquet",
    about = "Portable CSV/Parquet & Banking Analytics Engine"
)]
pub(crate) struct CliArgs {
    #[arg(long)]
    headless: bool,
    #[arg(long)]
    macro_file: Option<PathBuf>,
    #[arg(long)]
    input: Option<PathBuf>,
    #[arg(long)]
    export_parquet: Option<PathBuf>,
    #[arg(long)]
    export_csv: Option<PathBuf>,
    #[arg(long)]
    export_xlsx: Option<PathBuf>,
    #[arg(long)]
    export_pdf: Option<PathBuf>,
    /// Export even when the control column has values that could not be converted.
    #[arg(long)]
    allow_parse_failures: bool,
}

fn main() -> eframe::Result<()> {
    let args = CliArgs::parse();
    let paths = PortablePaths::init();
    // Provisioning rule files: write defaults on first run, upgrade untouched old defaults,
    // and warn about edited files from an older version (shown in the status bar).
    let rule_notice: Option<String> =
        match micro_parquet::banking::ProvisionConfig::load_or_create(&paths.root_dir) {
            Ok(cfg) if cfg.warnings.is_empty() => None,
            Ok(cfg) => Some(format!("⚠ {}", cfg.warnings.join(" | "))),
            Err(e) => Some(format!("⚠ Provisioning rule files: {:#}", e)),
        };
    if let Some(n) = &rule_notice {
        eprintln!("[MicroParquet] {}", n);
    }

    if args.headless {
        if let Err(e) = headless::run_headless_cli(&args, &paths) {
            eprintln!("[MicroParquet Headless Error]: {}", e);
            std::process::exit(1);
        }
        return Ok(());
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(
                "MicroParquet v1.2 — Portable CSV/Parquet, ALM & Loans Analytics Workstation",
            )
            .with_inner_size([1320.0, 740.0])
            .with_min_inner_size([800.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "MicroParquet",
        native_options,
        Box::new(move |cc| {
            theme::apply_ui_theme(&cc.egui_ctx, false, 1.05);
            let mut app = MicroParquetApp::new(paths);
            if let Some(n) = rule_notice {
                app.status_message = n;
            }
            Ok(Box::new(app))
        }),
    )
}

#[derive(PartialEq, Eq)]
pub enum ActiveToolModal {
    None,
    FilterRows,
    ManageColumns,
    ConditionalCol,
    MathFormula,
    NormalizeEntity,
    GroupByPivot,
    StackFiles,
    JoinTabs,
    ReconcilePeriods,
    MaturityBucket,
    RateShockSim,
    FtpCurveMap,
    BankingAudit,
    DurationAnalysis,
    LoanProvision,
    LoanLimitUtil,
    SaveMacro,
    PythonCodePreview,
    MultiTabExport,
    SplitByColumn,
}

pub struct MicroParquetApp {
    paths: PortablePaths,
    tabs: Vec<TabState>,
    active_tab: usize,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
    parquet_dialect: ParquetDialect,
    export_csv_mdy: bool,
    show_steps_sidebar: bool,
    show_sql_bar: bool,
    show_chart_drawer: bool,
    dark_mode: bool,
    ui_zoom: f32,
    pending_csv_modal: Option<PendingCsvModal>,
    batch_modal: BatchModalState,
    active_tool: ActiveToolModal,
    status_message: String,

    // One state struct per screen (src/ui/<name>.rs)
    pub filter: FilterState,
    pub columns: ColumnsState,
    pub cond: CondState,
    pub calc: CalcState,
    pub normaliser: NormState,
    pub pivot: PivotState,
    pub stack: StackState,
    pub join: JoinState,
    pub recon: ReconcileState,
    pub buckets: BucketState,
    pub shock: ShockState,
    pub ftp: FtpState,
    pub audit: AuditState,
    pub duration: DurationState,
    pub provision: ProvisionState,
    pub limits: LimitState,
    pub macros: MacroState,
    pub export: ExportState,
    pub chart: ChartState,
}

impl MicroParquetApp {
    pub fn new(paths: PortablePaths) -> Self {
        let presets = CustomBucketDef::load_all_presets(&paths.buckets_file);
        Self {
            paths,
            tabs: Vec::new(),
            active_tab: 0,
            num_format: NumberDisplayFormat::BdtCrore,
            global_decimals: 2,
            parquet_dialect: ParquetDialect::ArchiveZstd,
            export_csv_mdy: false,
            show_steps_sidebar: false, // Hidden by default for maximum 720p table width; toggle in View menu
            show_sql_bar: false,
            show_chart_drawer: false,
            dark_mode: false,
            ui_zoom: 1.05,
            pending_csv_modal: None,
            batch_modal: BatchModalState::default(),
            active_tool: ActiveToolModal::None,
            status_message:
                "Ready. Open a CSV/Parquet file, load a .mpq.json macro, or run a Batch conversion."
                    .to_string(),

            filter: FilterState::default(),
            columns: ColumnsState::default(),
            cond: CondState::default(),
            calc: CalcState::default(),
            normaliser: NormState::default(),
            pivot: PivotState::default(),
            stack: StackState::default(),
            join: JoinState::default(),
            recon: ReconcileState::default(),
            buckets: BucketState { presets, ..BucketState::default() },
            shock: ShockState::default(),
            ftp: FtpState::default(),
            audit: AuditState::default(),
            duration: DurationState::default(),
            provision: ProvisionState::default(),
            limits: LimitState::default(),
            macros: MacroState::default(),
            export: ExportState::default(),
            chart: ChartState::default(),
        }
    }
}

impl eframe::App for MicroParquetApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // A PII key created by a pipeline nobody announced it for is never silent.
        if let Some(note) = take_pii_key_created_note() {
            self.status_message = note;
        }
        self.render_menu_bar(ctx);
        self.render_status_bar(ctx);
        self.render_steps_panel(ctx);

        // Central workspace
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.tabs.is_empty() {
                self.render_empty_state(ui);
                return;
            }
            if self.render_tab_strip(ui) {
                return;
            }
            ui.separator();
            self.render_toolbar(ui, ctx);
            self.render_sql_bar(ui);
            self.render_chart_drawer(ui);
            ui.separator();
            self.render_table_area(ui);
        });

        // Modals
        self.render_csv_schema_modal(ctx);
        self.render_export_guard(ctx);
        self.render_batch_modal(ctx);
        self.render_active_tool_modal(ctx);
    }
}

impl MicroParquetApp {
    /// Top menu bar (File, View, Transform, ...). L1 replaces most of it with the toolbar groups.
    fn render_menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("📂 File", |ui| {
                    if ui.button("Open CSV or Parquet...").clicked() {
                        ui.close_menu();
                        self.initiate_file_open(false);
                    }
                    if ui.button("Direct CSV -> Parquet Converter...").clicked() {
                        ui.close_menu();
                        self.initiate_file_open(true);
                    }
                    ui.separator();
                    if ui.button("🔄 Recursive Batch Folder Mirror...").clicked() {
                        ui.close_menu();
                        self.batch_modal.open = true;
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                // NEW: View & Display Menu (Accommodates 720p screens cleanly)
                ui.menu_button("👁 View", |ui| {
                    ui.checkbox(
                        &mut self.show_steps_sidebar,
                        "📋 Show Applied Steps (Macro) Sidebar",
                    );
                    ui.checkbox(&mut self.show_sql_bar, "💻 Show SQL Scratchpad Bar");
                    ui.checkbox(&mut self.show_chart_drawer, "📈 Show ALCO Visual Bar Chart");
                    ui.separator();

                    ui.menu_button("🔢 Default Monetary Scale", |ui| {
                        for fmt in [
                            NumberDisplayFormat::BdtCrore,
                            NumberDisplayFormat::BdtLakh,
                            NumberDisplayFormat::StandardCommas,
                            NumberDisplayFormat::Raw,
                        ] {
                            ui.selectable_value(&mut self.num_format, fmt, fmt.label());
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Decimal Places:");
                        if ui.small_button(" - ").clicked() && self.global_decimals > 0 {
                            self.global_decimals -= 1;
                        }
                        ui.strong(format!("{}", self.global_decimals));
                        if ui.small_button(" + ").clicked() && self.global_decimals < 6 {
                            self.global_decimals += 1;
                        }
                    });

                    ui.separator();
                    let theme_label = if self.dark_mode {
                        "☀ Switch to Daylight Light Theme"
                    } else {
                        "🌙 Switch to High-Contrast Dark Theme"
                    };
                    if ui.button(theme_label).clicked() {
                        self.dark_mode = !self.dark_mode;
                        theme::apply_ui_theme(ctx, self.dark_mode, self.ui_zoom);
                    }

                    ui.horizontal(|ui| {
                        ui.label("UI Scale:");
                        if ui.small_button("🔍-").clicked() {
                            self.ui_zoom = (self.ui_zoom - 0.10).clamp(0.80, 1.50);
                            theme::apply_ui_theme(ctx, self.dark_mode, self.ui_zoom);
                        }
                        ui.strong(format!("{:.0}%", self.ui_zoom * 100.0));
                        if ui.small_button("🔍+").clicked() {
                            self.ui_zoom = (self.ui_zoom + 0.10).clamp(0.80, 1.50);
                            theme::apply_ui_theme(ctx, self.dark_mode, self.ui_zoom);
                        }
                    });
                });

                ui.menu_button("🛠 Transform", |ui| {
                    if ui.button("🔍 Filter / Delete Rows...").clicked() {
                        ui.close_menu();
                        self.init_tool_defaults_from_active_tab();
                        self.active_tool = ActiveToolModal::FilterRows;
                    }
                    if ui.button("📐 Columns (Drop / Order / Rename / Format)...").clicked() {
                        ui.close_menu();
                        self.init_tool_defaults_from_active_tab();
                        self.active_tool = ActiveToolModal::ManageColumns;
                    }
                    if ui.button("➕ Add Conditional Column (IF/ELSE)...").clicked() {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::ConditionalCol;
                    }
                    if ui.button("🧮 Add Calculated Math Column...").clicked() {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::MathFormula;
                    }
                    ui.separator();
                    if ui.button("📊 Pivot & WAVG Aggregation Builder...").clicked() {
                        ui.close_menu();
                        self.init_tool_defaults_from_active_tab();
                        self.active_tool = ActiveToolModal::GroupByPivot;
                    }
                });

                ui.menu_button("🔗 Merge & Join", |ui| {
                    if ui.button("📚 Vertical Stack / Merge Multiple Files...").clicked() {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::StackFiles;
                    }
                    if ui.button("🔗 SQL Join Two Open Tabs...").clicked() {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::JoinTabs;
                    }
                    if ui
                        .button("⚖ Two-Period Variance & CL Migration Reconciler...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::ReconcilePeriods;
                    }
                });

                ui.menu_button("🏦 Banking & Loans", |ui| {
                    ui.strong("Treasury & ALM:");
                    if ui
                        .button("📅 Maturity Bucketer (SLP / LCR / Custom)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.buckets.presets = CustomBucketDef::load_all_presets(&self.paths.buckets_file);
                        self.active_tool = ActiveToolModal::MaturityBucket;
                    }
                    if ui.button("📈 Interest Rate Shock / ΔNII Simulator...").clicked() {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::RateShockSim;
                    }
                    if ui.button("⏱ Duration, DV01 & ΔEVE Engine...").clicked() {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::DurationAnalysis;
                    }
                    if ui.button("🧮 FTP Yield Curve Mapper...").clicked() {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::FtpCurveMap;
                    }
                    ui.separator();
                    ui.strong("Loans & Credit Risk:");
                    if ui.button("🏦 BB CL Provision & BFP Calculator...").clicked() {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::LoanProvision;
                    }
                    if ui
                        .button("💳 Limit Utilization, Over-Limit & Undrawn CCF...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::LoanLimitUtil;
                    }
                    ui.separator();
                    if ui.button("🛡 Core Banking Dirty Data & Ghost Auditor...").clicked() {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::BankingAudit;
                    }
                    if ui.button("🏢 Counterparty Entity Name Normalizer...").clicked() {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::NormalizeEntity;
                    }
                });

                ui.menu_button("⚙ Macros", |ui| {
                    if ui.button("💾 Save Active Tab as Macro (.mpq.json)...").clicked() {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::SaveMacro;
                    }
                    if ui.button("▶ Load & Replay Macro on Current Tab...").clicked() {
                        ui.close_menu();
                        self.load_and_apply_macro_interactive(false);
                    }
                    if ui.button("➕ Load & Replay Macro into New Tab...").clicked() {
                        ui.close_menu();
                        self.load_and_apply_macro_interactive(true);
                    }
                    ui.separator();
                    if ui.button("🐍 Export Pipeline as Python Polars (.py)...").clicked() {
                        ui.close_menu();
                        self.prepare_python_preview_for_active_tab();
                    }
                });

                ui.menu_button("📤 Export", |ui| {
                    ui.menu_button("⚙ Parquet Compression Mode", |ui| {
                        ui.selectable_value(
                            &mut self.parquet_dialect,
                            ParquetDialect::ArchiveZstd,
                            ParquetDialect::ArchiveZstd.label(),
                        );
                        ui.selectable_value(
                            &mut self.parquet_dialect,
                            ParquetDialect::PowerBiSnappy,
                            ParquetDialect::PowerBiSnappy.label(),
                        );
                    });
                    ui.separator();
                    if ui.button("Export Active Tab to Parquet (.parquet)...").clicked() {
                        ui.close_menu();
                        self.run_guarded_export(GuardedExport::Parquet);
                    }
                    if ui.button("Export Active Tab to CSV (.csv)...").clicked() {
                        ui.close_menu();
                        self.run_guarded_export(GuardedExport::Csv);
                    }
                    ui.separator();
                    if ui.button("📊 Multi-Tab Consolidated Excel (.xlsx)...").clicked() {
                        ui.close_menu();
                        self.export.multi_checks = vec![true; self.tabs.len()];
                        self.active_tool = ActiveToolModal::MultiTabExport;
                    }
                    if ui.button("📄 Multi-Tab Consolidated PDF (.pdf)...").clicked() {
                        ui.close_menu();
                        self.export.multi_checks = vec![true; self.tabs.len()];
                        self.active_tool = ActiveToolModal::MultiTabExport;
                    }
                    ui.separator();
                    if ui.button("🗂 Partitioned Split Export by Column...").clicked() {
                        ui.close_menu();
                        self.init_tool_defaults_from_active_tab();
                        self.active_tool = ActiveToolModal::SplitByColumn;
                    }
                });
            });
        });
    }

    /// Quick transformation and clipboard action bar.
    fn render_toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("🔍 Filter").clicked() {
                self.init_tool_defaults_from_active_tab();
                self.active_tool = ActiveToolModal::FilterRows;
            }
            if ui.button("📐 Columns/Fmt").clicked() {
                self.init_tool_defaults_from_active_tab();
                self.active_tool = ActiveToolModal::ManageColumns;
            }
            if ui.button("➕ Cond Col").clicked() {
                self.active_tool = ActiveToolModal::ConditionalCol;
            }
            if ui.button("📊 Pivot/WAVG").clicked() {
                self.init_tool_defaults_from_active_tab();
                self.active_tool = ActiveToolModal::GroupByPivot;
            }
            if ui.button("📅 Buckets").clicked() {
                self.buckets.presets = CustomBucketDef::load_all_presets(&self.paths.buckets_file);
                self.active_tool = ActiveToolModal::MaturityBucket;
            }
            if ui.button("⏱ Duration").clicked() {
                self.active_tool = ActiveToolModal::DurationAnalysis;
            }
            if ui.button("🏦 CL Prov").clicked() {
                self.active_tool = ActiveToolModal::LoanProvision;
            }
            if ui
                .selectable_label(self.show_steps_sidebar, "📋 Steps")
                .on_hover_text("Toggle Applied Steps (Macro) Sidebar")
                .clicked()
            {
                self.show_steps_sidebar = !self.show_steps_sidebar;
            }
            if ui.selectable_label(self.show_sql_bar, "💻 SQL").clicked() {
                self.show_sql_bar = !self.show_sql_bar;
            }
            if ui
                .button("📋 Copy Table")
                .on_hover_text("Copies visible table formatted for Excel / PowerPoint")
                .clicked()
            {
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    let txt = format_df_for_clipboard(
                        &tab.cached_slice,
                        &tab.col_formats,
                        self.num_format,
                        self.global_decimals,
                        false,
                    );
                    ctx.copy_text(txt);
                    self.status_message = "Copied formatted table to clipboard.".to_string();
                }
            }
        });
    }
}
