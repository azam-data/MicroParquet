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
            Ok(cfg) => Some(format!("\u{E4E0} {}", cfg.warnings.join(" | "))),
            Err(e) => Some(format!("\u{E4E0} Provisioning rule files: {:#}", e)),
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
            .with_title(format!(
                "MicroParquet v{} — Portable CSV/Parquet, ALM & Loans Analytics Workstation",
                env!("CARGO_PKG_VERSION")
            ))
            .with_inner_size([1320.0, 740.0])
            .with_min_inner_size([800.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "MicroParquet",
        native_options,
        Box::new(move |cc| {
            theme::load_fonts(&cc.egui_ctx);
            theme::apply_ui_theme(&cc.egui_ctx, false, 1.05);
            let mut app = MicroParquetApp::new(paths, cc.egui_ctx.clone());
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
    /// The worker thread: all Polars work runs there (R2).
    pub jobs: JobQueue<JobOutput>,
    tab_id_counter: u64,
    pending_refresh: HashMap<u64, PendingRefresh>,
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
    /// Right panel: which tab is shown, and the column the Column tab describes.
    pub right_tab: RightTab,
    pub selected_column: Option<String>,
    show_about: bool,
    /// Command search overlay (Ctrl K).
    pub command_open: bool,
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
    pub fn new(paths: PortablePaths, egui_ctx: egui::Context) -> Self {
        let presets = CustomBucketDef::load_all_presets(&paths.buckets_file);
        Self {
            jobs: JobQueue::new(move || egui_ctx.request_repaint()),
            tab_id_counter: 0,
            pending_refresh: HashMap::new(),
            paths,
            tabs: Vec::new(),
            active_tab: 0,
            num_format: NumberDisplayFormat::BdtCrore,
            global_decimals: 2,
            parquet_dialect: ParquetDialect::ArchiveZstd,
            export_csv_mdy: false,
            show_steps_sidebar: true,
            show_sql_bar: false,
            show_chart_drawer: false,
            right_tab: RightTab::Steps,
            selected_column: None,
            show_about: false,
            command_open: false,
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
        self.poll_jobs(ctx);
        self.render_title_bar(ctx);
        self.render_toolbar_panel(ctx);
        self.render_tab_strip_panel(ctx);
        self.render_status_bar(ctx);
        self.render_right_panel(ctx);

        // Central workspace: filter chips and paging, then the table.
        let tk = *theme::tokens(self.dark_mode);
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(tk.bg).inner_margin(egui::Margin { left: 16.0, right: 16.0, top: 14.0, bottom: 8.0 }))
            .show(ctx, |ui| {
                if self.tabs.is_empty() {
                    self.render_empty_state(ui);
                    return;
                }
                self.render_filter_bar(ui);
                self.render_sql_bar(ui);
                self.render_chart_drawer(ui);
                self.render_table_area(ui);
            });

        // Modals
        self.render_csv_schema_modal(ctx);
        self.render_export_guard(ctx);
        self.render_text_number_guard(ctx);
        self.render_batch_modal(ctx);
        self.render_active_tool_modal(ctx);
        self.render_about(ctx);
    }
}

impl MicroParquetApp {
    /// Title strip: logo, name, the File / View / Help menus and the active tab's name.
    fn render_title_bar(&mut self, ctx: &egui::Context) {
        let tk = *theme::tokens(self.dark_mode);
        egui::TopBottomPanel::top("title_bar")
            .exact_height(theme::TITLE_BAR_H)
            .frame(egui::Frame::none().fill(tk.surface).inner_margin(egui::Margin::symmetric(14.0, 0.0)))
            .show(ctx, |ui| {
                theme::bottom_border(ui, tk.border);
                egui::menu::bar(ui, |ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 4.0, tk.accent);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "M",
                        egui::FontId::new(11.0, theme::family_bold()),
                        theme::on_accent(self.dark_mode),
                    );
                    ui.label(theme::semibold("MicroParquet"));
                    ui.add_space(theme::SPACE_S);
                    self.file_menu(ui, ctx);
                    self.view_menu(ui, ctx);
                    ui.menu_button("Help", |ui| {
                        if ui.button("About MicroParquet").clicked() {
                            ui.close_menu();
                            self.show_about = true;
                        }
                    });
                    if let Some(t) = self.tabs.get(self.active_tab) {
                        ui.add_space(theme::SPACE_M);
                        ui.label(egui::RichText::new(&t.title).color(tk.muted));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(theme::label_text(format!("v{}", env!("CARGO_PKG_VERSION"))).color(tk.muted));
                    });
                });
            });
    }

    fn file_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.menu_button("File", |ui| {
            if ui.button("Open CSV or Parquet...").clicked() {
                ui.close_menu();
                self.initiate_file_open(false);
            }
            if ui.button("Direct CSV -> Parquet Converter...").clicked() {
                ui.close_menu();
                self.initiate_file_open(true);
            }
            ui.separator();
            if ui.button("Recursive Batch Folder Mirror...").clicked() {
                ui.close_menu();
                self.batch_modal.open = true;
            }
            ui.separator();
            let has_tab = !self.tabs.is_empty();
            if ui.add_enabled(has_tab, egui::Button::new("Save Active Tab as Macro (.mpq.json)...")).clicked() {
                ui.close_menu();
                self.active_tool = ActiveToolModal::SaveMacro;
            }
            if ui.add_enabled(has_tab, egui::Button::new("Load & Replay Macro on Current Tab...")).clicked() {
                ui.close_menu();
                self.load_and_apply_macro_interactive(false);
            }
            if ui.add_enabled(has_tab, egui::Button::new("Load & Replay Macro into New Tab...")).clicked() {
                ui.close_menu();
                self.load_and_apply_macro_interactive(true);
            }
            if ui.add_enabled(has_tab, egui::Button::new("Export Pipeline as Python Polars (.py)...")).clicked() {
                ui.close_menu();
                self.prepare_python_preview_for_active_tab();
            }
            ui.separator();
            if ui.button("Quit").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn view_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.menu_button("View", |ui| {
            let theme_label = if self.dark_mode { "Switch to Light Theme" } else { "Switch to Dark Theme" };
            if ui.button(format!("{}  {}", if self.dark_mode { theme::ph::SUN } else { theme::ph::MOON }, theme_label)).clicked() {
                self.dark_mode = !self.dark_mode;
                theme::apply_ui_theme(ctx, self.dark_mode, self.ui_zoom);
            }
            ui.horizontal(|ui| {
                ui.label("UI Scale:");
                if ui.small_button("\u{E30C}-").clicked() {
                    self.ui_zoom = (self.ui_zoom - 0.10).clamp(0.80, 1.50);
                    theme::apply_ui_theme(ctx, self.dark_mode, self.ui_zoom);
                }
                ui.strong(format!("{:.0}%", self.ui_zoom * 100.0));
                if ui.small_button("\u{E30C}+").clicked() {
                    self.ui_zoom = (self.ui_zoom + 0.10).clamp(0.80, 1.50);
                    theme::apply_ui_theme(ctx, self.dark_mode, self.ui_zoom);
                }
            });
            ui.separator();
            ui.menu_button("Default Monetary Scale", |ui| {
                for fmt in NumberDisplayFormat::all() {
                    ui.selectable_value(&mut self.num_format, *fmt, fmt.label());
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
            ui.checkbox(&mut self.show_steps_sidebar, "Right panel (Steps / Column)");
            ui.checkbox(&mut self.show_sql_bar, "SQL scratchpad bar");
            ui.checkbox(&mut self.show_chart_drawer, "ALCO bar chart");
        });
    }

    /// Toolbar (52 px): Open / Stack / Batch | Filter / Columns / Calc column / Pivot | Banking |
    /// Export, and the command search box on the right.
    fn render_toolbar_panel(&mut self, ctx: &egui::Context) {
        let tk = *theme::tokens(self.dark_mode);
        egui::TopBottomPanel::top("toolbar")
            .exact_height(theme::TOOLBAR_H)
            .frame(egui::Frame::none().fill(tk.surface).inner_margin(egui::Margin::symmetric(12.0, 0.0)))
            .show(ctx, |ui| {
                theme::bottom_border(ui, tk.border);
                ui.visuals_mut().widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let ph = theme::ph::FOLDER_OPEN;
                    if theme::flat_button(ui, ph, "Open").clicked() {
                        self.initiate_file_open(false);
                    }
                    if theme::flat_button(ui, theme::ph::STACK, "Stack").clicked() {
                        self.active_tool = ActiveToolModal::StackFiles;
                    }
                    if theme::flat_button(ui, theme::ph::ARROWS_CLOCKWISE, "Batch").clicked() {
                        self.batch_modal.open = true;
                    }
                    ui.separator();
                    let has_tab = !self.tabs.is_empty();
                    ui.add_enabled_ui(has_tab, |ui| {
                        self.transform_buttons(ui);
                    });
                    ui.separator();
                    ui.add_enabled_ui(has_tab, |ui| {
                        self.banking_menu(ui);
                    });
                    ui.separator();
                    ui.add_enabled_ui(has_tab, |ui| {
                        self.export_menu(ui);
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.command_box(ui);
                    });
                });
            });
    }

    fn transform_buttons(&mut self, ui: &mut egui::Ui) {
        if theme::flat_button(ui, theme::ph::FUNNEL, "Filter").clicked() {
            self.open_filter_editor_default();
        }
        if theme::flat_button(ui, theme::ph::COLUMNS, "Columns").clicked() {
            self.init_tool_defaults_from_active_tab();
            self.active_tool = ActiveToolModal::ManageColumns;
        }
        if theme::flat_button(ui, theme::ph::CALCULATOR, "Calc column").clicked() {
            self.active_tool = ActiveToolModal::MathFormula;
        }
        if theme::flat_button(ui, theme::ph::TABLE, "Pivot").clicked() {
            self.init_tool_defaults_from_active_tab();
            self.active_tool = ActiveToolModal::GroupByPivot;
        }
        ui.menu_button(format!("{}  More", theme::ph::DOTS_THREE), |ui| {
            if ui.button(format!("{}  Conditional column (IF / ELSE)...", theme::ph::PLUS)).clicked() {
                ui.close_menu();
                self.active_tool = ActiveToolModal::ConditionalCol;
            }
            if ui.button(format!("{}  Counterparty name normaliser...", theme::ph::BUILDINGS)).clicked() {
                ui.close_menu();
                self.active_tool = ActiveToolModal::NormalizeEntity;
            }
            if ui.button(format!("{}  SQL join of two open tabs...", theme::ph::LINK)).clicked() {
                ui.close_menu();
                self.active_tool = ActiveToolModal::JoinTabs;
            }
            ui.separator();
            ui.checkbox(&mut self.show_sql_bar, "SQL scratchpad bar");
            ui.checkbox(&mut self.show_chart_drawer, "ALCO bar chart");
            if ui.button(format!("{}  Copy table (for Excel / PowerPoint)", theme::ph::CLIPBOARD_TEXT)).clicked() {
                ui.close_menu();
                self.copy_table_to_clipboard(ui.ctx());
            }
        });
    }

    fn copy_table_to_clipboard(&mut self, ctx: &egui::Context) {
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

    fn banking_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(format!("{}  Banking  {}", theme::ph::BANK, theme::ph::CARET_DOWN), |ui| {
            ui.strong("Treasury & ALM");
            if ui.button(format!("{}  Maturity bucketer (SLP / LCR / Custom)...", theme::ph::CALENDAR)).clicked() {
                ui.close_menu();
                self.buckets.presets = CustomBucketDef::load_all_presets(&self.paths.buckets_file);
                self.active_tool = ActiveToolModal::MaturityBucket;
            }
            if ui.button(format!("{}  Interest rate shock / \u{394}NII...", theme::ph::CHART_LINE)).clicked() {
                ui.close_menu();
                self.active_tool = ActiveToolModal::RateShockSim;
            }
            if ui.button(format!("{}  Duration, DV01 & \u{394}EVE...", theme::ph::TIMER)).clicked() {
                ui.close_menu();
                self.active_tool = ActiveToolModal::DurationAnalysis;
            }
            if ui.button(format!("{}  FTP yield curve mapper...", theme::ph::CALCULATOR)).clicked() {
                ui.close_menu();
                self.active_tool = ActiveToolModal::FtpCurveMap;
            }
            ui.separator();
            ui.strong("Loans & credit risk");
            if ui.button(format!("{}  BB classified-loan provision & BFP...", theme::ph::BANK)).clicked() {
                ui.close_menu();
                self.active_tool = ActiveToolModal::LoanProvision;
            }
            if ui.button(format!("{}  Limit utilisation, over-limit & undrawn CCF...", theme::ph::CREDIT_CARD)).clicked() {
                ui.close_menu();
                self.active_tool = ActiveToolModal::LoanLimitUtil;
            }
            ui.separator();
            if ui.button(format!("{}  Two-period variance & CL migration...", theme::ph::SCALES)).clicked() {
                ui.close_menu();
                self.active_tool = ActiveToolModal::ReconcilePeriods;
            }
            if ui.button(format!("{}  Core banking dirty-data auditor...", theme::ph::SHIELD_CHECK)).clicked() {
                ui.close_menu();
                self.active_tool = ActiveToolModal::BankingAudit;
            }
        });
    }

    fn export_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(format!("{}  Export  {}", theme::ph::EXPORT, theme::ph::CARET_DOWN), |ui| {
            ui.menu_button("Parquet compression mode", |ui| {
                ui.selectable_value(&mut self.parquet_dialect, ParquetDialect::ArchiveZstd, ParquetDialect::ArchiveZstd.label());
                ui.selectable_value(&mut self.parquet_dialect, ParquetDialect::PowerBiSnappy, ParquetDialect::PowerBiSnappy.label());
            });
            ui.separator();
            if ui.button("Active tab to Parquet (.parquet)...").clicked() {
                ui.close_menu();
                self.run_guarded_export(GuardedExport::Parquet);
            }
            if ui.button("Active tab to CSV (.csv)...").clicked() {
                ui.close_menu();
                self.run_guarded_export(GuardedExport::Csv);
            }
            ui.separator();
            if ui.button(format!("{}  Multi-tab Excel workbook (.xlsx)...", theme::ph::FILE_XLS)).clicked() {
                ui.close_menu();
                self.export.multi_checks = vec![true; self.tabs.len()];
                self.active_tool = ActiveToolModal::MultiTabExport;
            }
            if ui.button(format!("{}  Multi-tab PDF report (.pdf)...", theme::ph::FILE_PDF)).clicked() {
                ui.close_menu();
                self.export.multi_checks = vec![true; self.tabs.len()];
                self.active_tool = ActiveToolModal::MultiTabExport;
            }
            ui.separator();
            if ui.button(format!("{}  Split export by column...", theme::ph::FILES)).clicked() {
                ui.close_menu();
                self.init_tool_defaults_from_active_tab();
                self.active_tool = ActiveToolModal::SplitByColumn;
            }
        });
    }

    /// The search box on the right of the toolbar ("Search actions and columns", Ctrl K).
    fn command_box(&mut self, ui: &mut egui::Ui) {
        let tk = *theme::tokens(self.dark_mode);
        let resp = egui::Frame::none()
            .fill(tk.bg)
            .stroke(egui::Stroke::new(1.0_f32, tk.border))
            .rounding(egui::Rounding::same(theme::RADIUS_PANEL))
            .inner_margin(egui::Margin::symmetric(10.0, 6.0))
            .show(ui, |ui| {
                ui.set_width(310.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(theme::ph::MAGNIFYING_GLASS).color(tk.muted));
                    ui.label(egui::RichText::new("Search actions and columns").color(tk.muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::Frame::none()
                            .fill(tk.surface)
                            .stroke(egui::Stroke::new(1.0_f32, tk.border))
                            .rounding(egui::Rounding::same(4.0))
                            .inner_margin(egui::Margin::symmetric(5.0, 1.0))
                            .show(ui, |ui| {
                                ui.label(theme::mono("Ctrl K").size(11.0).color(tk.muted));
                            });
                    });
                });
            })
            .response
            .interact(egui::Sense::click());
        if resp.clicked() {
            self.command_open = true;
        }
    }

    fn render_about(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let mut open = true;
        theme::modal("About MicroParquet").open(&mut open).show(ctx, |ui| {
            ui.label(theme::semibold(format!("MicroParquet {}", env!("CARGO_PKG_VERSION"))));
            ui.label("Portable Parquet / CSV viewer, converter and editor for treasury, ALM and finance work.");
            ui.add_space(theme::SPACE_S);
            ui.label(format!("Data folder: {}", self.paths.root_dir.display()));
        });
        if !open {
            self.show_about = false;
        }
    }
}
