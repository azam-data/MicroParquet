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
            .with_title(ui::tabs::window_title(None))
            .with_inner_size(theme::WINDOW_SIZE)
            .with_min_inner_size(theme::WINDOW_MIN_SIZE),
        ..Default::default()
    };

    eframe::run_native(
        "MicroParquet",
        native_options,
        Box::new(move |cc| {
            theme::load_fonts(&cc.egui_ctx);
            let mut app = MicroParquetApp::new(paths, cc.egui_ctx.clone());
            // the saved default zoom (105% when there is none)
            theme::apply_ui_theme(&cc.egui_ctx, false, app.ui_zoom);
            if let Some(n) = rule_notice {
                app.status_message = n;
            }
            Ok(Box::new(app))
        }),
    )
}

/// What Help > About drew (the render tests read it).
#[derive(Clone, Copy, Debug)]
pub struct AboutRects {
    pub version: egui::Rect,
    pub build: egui::Rect,
    pub folder: egui::Rect,
    pub open_folder: egui::Rect,
    pub key: egui::Rect,
    pub fonts: egui::Rect,
    pub close: egui::Rect,
}

impl Default for AboutRects {
    fn default() -> Self {
        let n = egui::Rect::NOTHING;
        Self { version: n, build: n, folder: n, open_folder: n, key: n, fonts: n, close: n }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveToolModal {
    None,
    FilterRows,
    SortRows,
    /// The custom sort-list manager (Phase 6 U3).
    SortLists,
    /// "Save run" of a result tab (Phase 6 P1).
    SaveRun,
    /// File ▸ "Saved tool runs" (Phase 6 P1).
    SavedRuns,
    ManageColumns,
    ConditionalCol,
    MathFormula,
    NormalizeEntity,
    /// Clean line breaks inside text values.
    CleanLineBreaks,
    GroupByPivot,
    /// The pivot builder (Phase 4 V1); `GroupByPivot` is the older dialog that edits old steps.
    PivotBuilder,
    StackFiles,
    JoinTabs,
    ReconcilePeriods,
    ClMigration,
    GapLadder,
    IrrbbLadder,
    IrrbbSix,
    BehaviourProfile,
    YieldCurves,
    MaturityBucket,
    RateBand,
    RateShockSim,
    FtpCurveMap,
    BankingAudit,
    DurationAnalysis,
    LoanProvision,
    /// Classify by overdue (Phase 5 R4).
    ClassifyOverdue,
    /// CL-1 summary (Phase 5 R5).
    Cl1Summary,
    /// LCR / NSFR (Phase 5 R6).
    Liquidity,
    /// IFRS 9 ECL (Phase 5 R7).
    IfrsEcl,
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
    show_chart_drawer: bool,
    /// The summary row (selectors and values) under the column names; shown by default.
    show_summary_row: bool,
    /// Right panel: which tab is shown, and the column the Column tab describes.
    pub right_tab: RightTab,
    pub selected_column: Option<String>,
    /// Steps panel: the selected step, a delete waiting for confirmation, and where the next
    /// added step goes (after this step instead of at the end).
    pub selected_step: Option<usize>,
    pub confirm_delete_step: Option<usize>,
    /// The Column inspector draws its bars on a log scale.
    pub inspector_log: bool,
    pub insert_after: Option<usize>,
    /// A step reopened in its modal (edit in place).
    pub editing: Option<ui::step_edit::EditingStep>,
    show_about: bool,
    /// The row-count "Details" dialog is open.
    pub row_details_open: bool,
    /// The last 50 status messages (time, text), newest last, and the "Messages" list's state.
    pub status_log: Vec<(String, String)>,
    pub status_logged: String,
    pub messages_open: bool,
    /// Closing: the "Export in progress — cancel and quit?" question is open, or was answered yes.
    pub quit_guard: bool,
    pub quit_confirmed: bool,
    /// Command search overlay (Ctrl K): open, what is typed, the highlighted row, and the commands
    /// used last (first = most recent).
    pub command_open: bool,
    pub command_query: String,
    pub command_cursor: usize,
    pub command_focus: bool,
    /// The pass the search overlay opened in, and where the search box in the bar is.
    pub command_opened_pass: u64,
    pub command_box_rect: egui::Rect,
    pub recent_commands: Vec<micro_parquet::commands::CommandId>,
    dark_mode: bool,
    ui_zoom: f32,
    pending_csv_modal: Option<PendingCsvModal>,
    /// Several saved schemas match the file that was opened: which one? (Phase 6 U1)
    pub template_pick: Option<crate::ui::schema::TemplatePick>,
    /// A schema change that would break steps, waiting for "Apply anyway" or "Cancel" (U1).
    pub schema_confirm: Option<SchemaApplied>,
    /// A line that starts the status message of a tab that is still loading (by tab id): the
    /// saved schema it opened with (Phase 6 U1).
    pub open_notes: HashMap<u64, String>,
    /// `portable_data/settings.json` (Phase 6): panel width, default zoom, chart choices.
    pub settings: micro_parquet::settings::Settings,
    /// The right panel: its width in points this frame, whether a drag is under way, and a counter
    /// that gives the panel a new identity (so it starts at the width asked for).
    pub right_panel_w: f32,
    pub right_panel_dragging: bool,
    /// What the SQL tab drew (for the render tests).
    pub sql_rects: ui::sql::SqlRects,
    /// The title the window has now ("MicroParquet v1.6.0 — loans.csv"), and what About drew.
    pub window_title: String,
    pub about_rects: AboutRects,
    /// The folder "Open folder" in About last opened (tests read it; the program starts the
    /// file manager).
    pub opened_folder: Option<PathBuf>,
    /// Tests: the file the "Open" dialog would give (`None` = cancelled), and how often it was asked for.
    #[cfg(test)]
    pub test_pick: Option<PathBuf>,
    #[cfg(test)]
    pub file_dialog_requests: u32,
    batch_modal: BatchModalState,
    active_tool: ActiveToolModal,
    status_message: String,

    // One state struct per screen (src/ui/<name>.rs)
    pub filter: FilterState,
    pub sort: SortState,
    pub sort_lists: SortListsState,
    pub runs: RunsState,
    /// The value picker of the filter or conditional-column dialog that is open, and the number
    /// of distinct values last listed for each column (for "3 of 42 selected").
    pub picker: PickerPanel,
    pub picker_totals: HashMap<String, usize>,
    /// The tool the picker was last reset for (a new dialog starts with a new picker).
    pub picker_tool: ActiveToolModal,
    pub columns: ColumnsState,
    pub cond: CondState,
    pub calc: CalcState,
    pub normaliser: NormState,
    pub clean_breaks: CleanBreaksState,
    /// The tab whose name is being edited in the strip.
    pub tab_rename: Option<crate::ui::tabs::TabRename>,
    pub pivot: PivotState,
    pub pivot_builder: PivotBuilderState,
    pub pivot_bar: PivotBarState,
    pub cl_migration: ClMigrationState,
    pub gap: GapState,
    pub cf: CfLadderState,
    pub irrbb: IrrbbState,
    pub behaviour: BehaviourState,
    pub curves: CurvesState,
    pub stack: StackState,
    pub join: JoinState,
    pub recon: ReconcileState,
    pub buckets: BucketState,
    pub rate_band: RateBandState,
    pub shock: ShockState,
    pub ftp: FtpState,
    pub audit: AuditState,
    pub duration: DurationState,
    pub provision: ProvisionState,
    pub overdue: OverdueState,
    pub cl1: Cl1State,
    pub liq: LiqState,
    pub ecl: EclState,
    /// The IFRS 9 summary tab to open once the step has been added to the active tab.
    pub pending_ecl_summary: Option<TabSource>,
    pub limits: LimitState,
    pub macros: MacroState,
    pub export: ExportState,
    pub chart: ChartState,
}

impl MicroParquetApp {
    pub fn new(paths: PortablePaths, egui_ctx: egui::Context) -> Self {
        let presets = CustomBucketDef::load_all_presets(&paths.buckets_file);
        let rate_presets = RateBandDef::load_all_presets(&paths.rate_bands_file());
        let (settings, settings_note) = micro_parquet::settings::Settings::load(&paths.root_dir);
        let (start_zoom, zoom_note) = settings.start_zoom(theme::UI_ZOOM_DEFAULT);
        let settings_note = match (settings_note, zoom_note) {
            (Some(a), Some(b)) => Some(format!("{} {}", a, b)),
            (a, b) => a.or(b),
        };
        let recent_commands = std::fs::read_to_string(paths.root_dir.join("recent_commands.txt"))
            .map(|t| micro_parquet::commands::recent_from_text(&t))
            .unwrap_or_default();
        let mut app = Self {
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
            show_chart_drawer: false,
            show_summary_row: true,
            right_tab: RightTab::Steps,
            selected_column: None,
            selected_step: None,
            confirm_delete_step: None,
            inspector_log: false,
            insert_after: None,
            editing: None,
            show_about: false,
            row_details_open: false,
            status_log: Vec::new(),
            status_logged: String::new(),
            messages_open: false,
            quit_guard: false,
            quit_confirmed: false,
            command_open: false,
            command_query: String::new(),
            command_cursor: 0,
            command_focus: false,
            command_opened_pass: 0,
            command_box_rect: egui::Rect::NOTHING,
            recent_commands,
            dark_mode: false,
            ui_zoom: start_zoom,
            pending_csv_modal: None,
            template_pick: None,
            schema_confirm: None,
            open_notes: HashMap::new(),
            settings: settings.clone(),
            right_panel_w: settings.right_panel_width.unwrap_or(ui::theme::RIGHT_PANEL_W).max(ui::theme::RIGHT_PANEL_W),
            right_panel_dragging: false,
            sql_rects: ui::sql::SqlRects::default(),
            window_title: ui::tabs::window_title(None),
            about_rects: AboutRects::default(),
            opened_folder: None,
            #[cfg(test)]
            test_pick: None,
            #[cfg(test)]
            file_dialog_requests: 0,
            batch_modal: BatchModalState::default(),
            active_tool: ActiveToolModal::None,
            status_message:
                "Ready. Open a CSV/Parquet file, load a .mpq.json macro, or run a Batch conversion."
                    .to_string(),

            filter: FilterState::default(),
            sort: SortState::default(),
            sort_lists: SortListsState::default(),
            runs: RunsState::default(),
            picker: PickerPanel::default(),
            picker_totals: HashMap::new(),
            picker_tool: ActiveToolModal::None,
            columns: ColumnsState::default(),
            cond: CondState::default(),
            calc: CalcState::default(),
            normaliser: NormState::default(),
            clean_breaks: CleanBreaksState::default(),
            tab_rename: None,
            pivot: PivotState::default(),
            pivot_builder: PivotBuilderState::default(),
            pivot_bar: PivotBarState::default(),
            cl_migration: ClMigrationState::default(),
            gap: GapState::default(),
            cf: CfLadderState::default(),
            irrbb: IrrbbState::default(),
            behaviour: BehaviourState::default(),
            curves: CurvesState::default(),
            stack: StackState::default(),
            join: JoinState::default(),
            recon: ReconcileState::default(),
            buckets: BucketState { presets, ..BucketState::default() },
            rate_band: RateBandState { presets: rate_presets, ..RateBandState::default() },
            shock: ShockState::default(),
            ftp: FtpState::default(),
            audit: AuditState::default(),
            duration: DurationState::default(),
            provision: ProvisionState::default(),
            overdue: OverdueState::default(),
            cl1: Cl1State::default(),
            liq: LiqState::default(),
            ecl: EclState::default(),
            pending_ecl_summary: None,
            limits: LimitState::default(),
            macros: MacroState::default(),
            export: ExportState::default(),
            chart: ChartState::default(),
        };
        if let Some(note) = settings_note {
            app.status_message = note;
        }
        app
    }

    /// Writes `settings.json`; a failure goes to the status line.
    pub(crate) fn save_settings(&mut self) {
        if let Err(e) = self.settings.save(&self.paths.root_dir) {
            self.status_message = format!("Settings were not saved: {:#}", e);
        }
    }
}

impl eframe::App for MicroParquetApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_ui(ctx);
    }
}

impl MicroParquetApp {
    /// One frame of the whole window: panels, central table, dialogs. (Tests draw it in a
    /// headless egui context at a given window size and scale.)
    pub(crate) fn frame_ui(&mut self, ctx: &egui::Context) {
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
        self.handle_zoom_keys(ctx);
        self.update_window_title(ctx);
        self.handle_step_shortcuts(ctx);
        self.handle_tab_shortcuts(ctx);
        self.render_top_bar(ctx);
        self.render_tab_strip_panel(ctx);
        self.render_status_bar(ctx);
        // Modal dialogs may cover the right panel: their area is the window less the top bar, the
        // tab strip and the status bar. (The central panel takes the rest, so egui cannot be
        // asked for it later.) Only the non-modal panels keep clear of the right panel.
        theme::set_dialog_area(ctx, ctx.available_rect());
        self.render_right_panel(ctx);
        theme::set_central_area(ctx, ctx.available_rect());

        // Central workspace: filter chips and paging, then the table.
        let tk = *theme::tokens(self.dark_mode);
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(tk.bg).inner_margin(theme::MARGIN_CENTRAL))
            .show(ctx, |ui| {
                if self.tabs.is_empty() {
                    self.render_empty_state(ui);
                    return;
                }
                self.render_pivot_bar(ui);
                self.render_filter_bar(ui);
                self.render_chart_drawer(ui);
                self.render_table_area(ui);
            });

        // Modals
        self.render_csv_schema_modal(ctx);
        self.render_template_picker(ctx);
        self.render_schema_confirm(ctx);
        self.render_export_guard(ctx);
        self.render_text_number_guard(ctx);
        self.render_batch_modal(ctx);
        self.render_active_tool_modal(ctx);
        self.render_about(ctx);
        self.render_row_details(ctx);
        self.render_messages(ctx);
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
            let zoom_title = format!("Zoom  {}", ui::zoom::percent_text(self.ui_zoom));
            ui.menu_button(zoom_title, |ui| self.zoom_menu_contents(ui, ctx));
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
            self.command_menu_item_titled(ui, ctx, micro_parquet::commands::CommandId::ToggleSql, "SQL");
            self.command_menu_item(ui, ctx, micro_parquet::commands::CommandId::SortLists);
            ui.checkbox(&mut self.show_chart_drawer, "Chart");
            ui.checkbox(&mut self.show_summary_row, "Summary row");
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



    /// A flat toolbar button for a command; greyed with the reason when it cannot run. Icon-only
    /// buttons say what they are in a tooltip.
    fn toolbar_button(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, id: micro_parquet::commands::CommandId, icon_only: bool) -> egui::Response {
        let reason = self.command_unavailable(id);
        let label = if icon_only { "" } else { ui::commands::toolbar_label(id) };
        let btn = ui
            .add_enabled_ui(reason.is_none(), |ui| theme::flat_button(ui, ui::commands::command_icon(id), label))
            .inner;
        let btn = match reason {
            Some(r) if icon_only => btn.on_disabled_hover_text(format!("{}: {}", ui::commands::toolbar_label(id), r)),
            Some(r) => btn.on_disabled_hover_text(r),
            None if icon_only => btn.on_hover_text(ui::commands::toolbar_label(id)),
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
            .stroke(theme::line(tk.border))
            .rounding(egui::Rounding::same(theme::RADIUS_PANEL))
            .inner_margin(theme::MARGIN_SEARCH_BOX)
            .show(ui, |ui| {
                ui.set_width((width - theme::SEARCH_FRAME_PAD).max(0.0));
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(theme::ph::MAGNIFYING_GLASS).color(tk.muted));
                    let hint = if width >= theme::SEARCH_LONG_HINT_W { "Search actions and columns" } else { "Search\u{2026}" };
                    ui.add(egui::Label::new(egui::RichText::new(hint).color(tk.muted)).truncate());
                    // a very narrow box keeps the icon and "Search…"; the Ctrl K badge needs room
                    if width >= theme::SEARCH_BADGE_MIN_W {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            egui::Frame::none()
                                .fill(tk.surface)
                                .stroke(theme::line(tk.border))
                                .rounding(egui::Rounding::same(theme::RADIUS_SMALL))
                                .inner_margin(theme::MARGIN_KEYCAP)
                                .show(ui, |ui| {
                                    ui.label(theme::mono("Ctrl K").size(theme::TEXT_KEYCAP).color(tk.muted));
                                });
                        });
                    }
                });
            })
            .response
            .interact(egui::Sense::click());
        self.command_box_rect = resp.rect;
        if resp.clicked() {
            // the box opens the search and closes it again
            if self.command_open {
                self.command_open = false;
            } else {
                self.open_command_palette();
            }
        }
        resp
    }

    pub(crate) fn render_about(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let tk = *theme::tokens(self.dark_mode);
        let mut open = true;
        let mut close = false;
        let mut open_folder = false;
        let mut rects = AboutRects::default();
        let key_text = match micro_parquet::engine::read_pii_key(&micro_parquet::engine::pii_key_path()) {
            Ok(Some(k)) => micro_parquet::engine::pii_key_fingerprint(&k),
            Ok(None) => "not created yet (it is made the first time a column is masked)".to_string(),
            Err(e) => format!("could not be read: {:#}", e),
        };
        let folder = self.paths.root_dir.display().to_string();
        theme::modal("About MicroParquet").open(&mut open).default_width(theme::DIALOG_W_ABOUT).show(ctx, |d| {
            d.body(|ui| {
                ui.set_max_width(ui.available_width());
                let wrap = |ui: &mut egui::Ui, text: String| {
                    ui.add(egui::Label::new(text).wrap());
                };
                rects.version = ui.label(theme::semibold(format!("MicroParquet v{}", env!("CARGO_PKG_VERSION"))).size(theme::TEXT_HEADING)).rect;
                rects.build = ui.label(egui::RichText::new(format!("Built {}", env!("MP_BUILD_DATE"))).color(tk.muted)).rect;
                ui.add_space(theme::SPACE_S);
                wrap(ui, "Portable CSV/Parquet viewer, converter and editor for treasury, ALM and loans analytics.".to_string());
                ui.add_space(theme::SPACE_S);
                ui.label(theme::semibold("What it does"));
                for line in [
                    "Out-of-core Polars engine: big files stay on disk",
                    "Multi-tab views, each with its own steps",
                    "Custom maturity buckets and rate bands",
                    "Per-metric number formats (Crore, Lakh, Million, Thousand)",
                ] {
                    wrap(ui, format!("\u{2022}  {}", line));
                }
                ui.add_space(theme::SPACE_S);
                ui.label(theme::semibold("Data folder (portable_data)"));
                ui.horizontal(|ui| {
                    rects.folder = ui.add(egui::Label::new(theme::mono(folder.clone())).truncate()).on_hover_text(folder.clone()).rect;
                });
                let b = ui.button("Open folder");
                rects.open_folder = b.rect;
                if b.clicked() {
                    open_folder = true;
                }
                ui.add_space(theme::SPACE_S);
                ui.label(theme::semibold("Masking key"));
                rects.key = ui.add(egui::Label::new(theme::mono(key_text.clone())).wrap()).rect;
                ui.add_space(theme::SPACE_S);
                ui.label(theme::semibold("Fonts"));
                rects.fonts = ui
                    .add(egui::Label::new("Source Sans 3 and JetBrains Mono, both under the SIL Open Font License 1.1 (OFL). The licence texts are in assets/fonts and THIRD_PARTY_NOTICES.md.").wrap())
                    .rect;
            });
            d.footer(|ui| {
                let b = ui.button("Close");
                rects.close = b.rect;
                if b.clicked() {
                    close = true;
                }
            });
        });
        self.about_rects = rects;
        if open_folder {
            self.open_data_folder();
        }
        if close {
            open = false;
        }
        if !open {
            self.show_about = false;
        }
    }

    /// Shows the data folder in the system's file manager.
    fn open_data_folder(&mut self) {
        let dir = self.paths.root_dir.clone();
        self.opened_folder = Some(dir.clone());
        #[cfg(not(test))]
        {
            let result = if cfg!(target_os = "windows") {
                std::process::Command::new("explorer").arg(&dir).spawn()
            } else if cfg!(target_os = "macos") {
                std::process::Command::new("open").arg(&dir).spawn()
            } else {
                std::process::Command::new("xdg-open").arg(&dir).spawn()
            };
            if let Err(e) = result {
                self.status_message = format!("Could not open the folder {}: {}", dir.display(), e);
            }
        }
    }
}
