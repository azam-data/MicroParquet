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
    // .part files the last session could not delete (for example Windows still held one).
    let sweep_notice = micro_parquet::jobs::sweep_pending_parts(&paths.root_dir).message();
    if let Some(n) = &sweep_notice {
        eprintln!("[MicroParquet] {}", n);
    }
    let rule_notice = match (rule_notice, sweep_notice) {
        (Some(a), Some(b)) => Some(format!("{} | {}", a, b)),
        (a, b) => a.or(b),
    };

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveToolModal {
    None,
    FilterRows,
    SortRows,
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
    /// Summary choices of a macro replayed into a tab that is still loading (by tab id).
    pending_summary: HashMap<u64, HashMap<String, SummarySpec>>,
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
    /// Steps panel: the selected step, a delete waiting for confirmation, and where the next
    /// added step goes (after this step instead of at the end).
    pub selected_step: Option<usize>,
    pub confirm_delete_step: Option<usize>,
    pub insert_after: Option<usize>,
    /// A step reopened in its modal (edit in place).
    pub editing: Option<ui::step_edit::EditingStep>,
    show_about: bool,
    /// Closing: the "Export in progress — cancel and quit?" question is open, or was answered yes.
    pub quit_guard: bool,
    pub quit_confirmed: bool,
    /// Command search overlay (Ctrl K): open, what is typed, the highlighted row, and the commands
    /// used last (first = most recent).
    pub command_open: bool,
    pub command_query: String,
    pub command_cursor: usize,
    pub command_focus: bool,
    pub recent_commands: Vec<micro_parquet::commands::CommandId>,
    dark_mode: bool,
    ui_zoom: f32,
    pending_csv_modal: Option<PendingCsvModal>,
    batch_modal: BatchModalState,
    active_tool: ActiveToolModal,
    status_message: String,

    // One state struct per screen (src/ui/<name>.rs)
    pub filter: FilterState,
    pub sort: SortState,
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
        let recent_commands = std::fs::read_to_string(paths.root_dir.join("recent_commands.txt"))
            .map(|t| micro_parquet::commands::recent_from_text(&t))
            .unwrap_or_default();
        Self {
            jobs: JobQueue::new(move || egui_ctx.request_repaint()),
            tab_id_counter: 0,
            pending_refresh: HashMap::new(),
            pending_summary: HashMap::new(),
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
            selected_step: None,
            confirm_delete_step: None,
            insert_after: None,
            editing: None,
            show_about: false,
            quit_guard: false,
            quit_confirmed: false,
            command_open: false,
            command_query: String::new(),
            command_cursor: 0,
            command_focus: false,
            recent_commands,
            dark_mode: false,
            ui_zoom: 1.05,
            pending_csv_modal: None,
            batch_modal: BatchModalState::default(),
            active_tool: ActiveToolModal::None,
            status_message:
                "Ready. Open a CSV/Parquet file, load a .mpq.json macro, or run a Batch conversion."
                    .to_string(),

            filter: FilterState::default(),
            sort: SortState::default(),
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
        if ctx.input(|i| i.viewport().close_requested()) {
            match self.close_requested() {
                ui::jobs::CloseDecision::Ask => ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose),
                ui::jobs::CloseDecision::Exit => {}
            }
        }
        self.handle_palette_shortcut(ctx);
        self.handle_step_shortcuts(ctx);
        self.render_top_bar(ctx);
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
        self.render_command_palette(ctx);
        self.render_quit_guard(ctx);
    }
}

impl MicroParquetApp {
    fn file_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> egui::Response {
        use micro_parquet::commands::{CommandGroup, CommandId};
        ui.menu_button("File", |ui| {
            for c in micro_parquet::commands::in_group(CommandGroup::File) {
                if matches!(c.id, CommandId::SaveMacro | CommandId::Quit) {
                    ui.separator();
                }
                self.command_menu_item(ui, ctx, c.id);
            }
        })
        .response
    }

    fn view_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> egui::Response {
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
        })
        .response
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



    /// A flat toolbar button for a command; greyed with the reason when it cannot run.
    fn toolbar_button(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, id: micro_parquet::commands::CommandId) -> egui::Response {
        let reason = self.command_unavailable(id);
        let btn = ui
            .add_enabled_ui(reason.is_none(), |ui| {
                theme::flat_button(ui, ui::commands::command_icon(id), ui::commands::toolbar_label(id))
            })
            .inner;
        let btn = match reason {
            Some(r) => btn.on_disabled_hover_text(r),
            None => btn,
        };
        if btn.clicked() {
            self.run_command(ctx, id);
        }
        btn
    }

    /// The search box on the right of the toolbar ("Search actions and columns", Ctrl K).
    fn command_box(&mut self, ui: &mut egui::Ui, width: f32) -> egui::Response {
        let tk = *theme::tokens(self.dark_mode);
        let resp = egui::Frame::none()
            .fill(tk.bg)
            .stroke(egui::Stroke::new(1.0_f32, tk.border))
            .rounding(egui::Rounding::same(theme::RADIUS_PANEL))
            .inner_margin(egui::Margin::symmetric(10.0, 6.0))
            .show(ui, |ui| {
                ui.set_width((width - theme::SEARCH_FRAME_PAD).max(0.0));
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(theme::ph::MAGNIFYING_GLASS).color(tk.muted));
                    let hint = if width >= theme::SEARCH_LONG_HINT_W { "Search actions and columns" } else { "Search\u{2026}" };
                    ui.add(egui::Label::new(egui::RichText::new(hint).color(tk.muted)).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::Frame::none()
                            .fill(tk.surface)
                            .stroke(egui::Stroke::new(1.0_f32, tk.border))
                            .rounding(egui::Rounding::same(4.0))
                            .inner_margin(egui::Margin::symmetric(5.0, 1.0))
                            .show(ui, |ui| {
                                ui.label(theme::mono("Ctrl K").size(theme::TEXT_KEYCAP).color(tk.muted));
                            });
                    });
                });
            })
            .response
            .interact(egui::Sense::click());
        if resp.clicked() {
            self.open_command_palette();
        }
        resp
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
