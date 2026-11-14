#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod banking;
mod batch;
mod engine;
mod exporters;
mod macros;
mod models;

use batch::{run_folder_mirror_batch, BatchReport};
use clap::Parser;
use eframe::egui;
use egui_extras::{Column, TableBuilder};
use egui_plot::{Bar, BarChart, Plot};
use engine::{
    auto_detect_smart_schema, compile_tab_lazy, compute_control_totals, export_to_csv_stream,
    export_to_parquet_stream, fetch_viewport_slice, preview_csv_raw,
};
use exporters::{
    export_tabs_to_pdf, export_tabs_to_xlsx, format_df_for_clipboard, split_and_export_by_column,
};
use macros::{generate_python_polars_code, load_macro_from_file, save_macro_to_file};
use models::{
    default_bd_ftp_curve, format_with_commas, AggFunc, AggSpec, BatchDirection,
    BatchSchemaStrategy, BucketPreset, ColDataType, CondRule, FilterOp, FtpPoint, IngestionConfig,
    MacroRecipe, MathOp, NumberDisplayFormat, ParquetDialect, PiiMaskMode, PortablePaths,
    SplitExportFormat, SqlJoinKind, TabSource, TabState, TransformStep,
};
use polars::datatypes::AnyValue;
use polars::prelude::DataFrame;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "MicroParquet",
    about = "Portable CSV/Parquet & Banking Analytics Engine"
)]
struct CliArgs {
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
}

fn main() -> eframe::Result<()> {
    let args = CliArgs::parse();
    let paths = PortablePaths::init();

    if args.headless {
        if let Err(e) = run_headless_cli(&args, &paths) {
            eprintln!("[MicroParquet Headless Error]: {}", e);
            std::process::exit(1);
        }
        return Ok(());
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(
                "MicroParquet v1.1 — Portable CSV/Parquet, ALM & Loans Analytics Workstation",
            )
            .with_inner_size([1440.0, 880.0])
            .with_min_inner_size([1024.0, 640.0]),
        ..Default::default()
    };

    eframe::run_native(
        "MicroParquet",
        native_options,
        Box::new(|cc| {
            MicroParquetApp::apply_ui_theme(&cc.egui_ctx, false, 1.10);
            Ok(Box::new(MicroParquetApp::new(paths)))
        }),
    )
}

fn run_headless_cli(args: &CliArgs, paths: &PortablePaths) -> anyhow::Result<()> {
    let input_path = args
        .input
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--input <FILE> is required in --headless mode"))?;

    let is_parquet = input_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .eq_ignore_ascii_case("parquet");

    let (cfg, steps) = if let Some(m_path) = &args.macro_file {
        let recipe = load_macro_from_file(m_path)?;
        let ing = if is_parquet {
            IngestionConfig::default()
        } else if let Some(saved_ing) = recipe.optional_ingestion_schema {
            saved_ing
        } else {
            auto_detect_smart_schema(input_path)?
        };
        (ing, recipe.steps)
    } else {
        let ing = if is_parquet {
            IngestionConfig::default()
        } else {
            auto_detect_smart_schema(input_path)?
        };
        (ing, Vec::new())
    };

    let source = TabSource::SingleFile {
        path: input_path.clone(),
        is_parquet,
        config: cfg,
    };

    let lf = compile_tab_lazy(&source, &steps, &paths.temp_dir)?;

    if let Some(out_parq) = &args.export_parquet {
        export_to_parquet_stream(lf.clone(), out_parq, ParquetDialect::ArchiveZstd)?;
        println!("Exported Parquet: {:?}", out_parq);
    }
    if let Some(out_csv) = &args.export_csv {
        export_to_csv_stream(lf.clone(), out_csv, false)?;
        println!("Exported CSV: {:?}", out_csv);
    }
    if let Some(out_xlsx) = &args.export_xlsx {
        let df = lf.clone().collect()?;
        export_tabs_to_xlsx(vec![("Headless_Output".to_string(), df)], out_xlsx)?;
        println!("Exported Excel: {:?}", out_xlsx);
    }
    if let Some(out_pdf) = &args.export_pdf {
        let df = lf.collect()?;
        export_tabs_to_pdf(
            vec![("Headless_Report".to_string(), df)],
            out_pdf,
            NumberDisplayFormat::BdtCrore,
        )?;
        println!("Exported PDF: {:?}", out_pdf);
    }

    Ok(())
}

struct PendingCsvModal {
    csv_path: PathBuf,
    raw_preview_df: DataFrame,
    config: IngestionConfig,
    direct_convert_only: bool,
}

#[derive(Default)]
struct BatchModalState {
    open: bool,
    input_folder: Option<PathBuf>,
    output_folder: Option<PathBuf>,
    direction: Option<BatchDirection>,
    schema_strategy: Option<BatchSchemaStrategy>,
    template_path: Option<PathBuf>,
    loaded_template: Option<IngestionConfig>,
    mdy_dates_on_csv: bool,
    last_report: Option<BatchReport>,
}

#[derive(PartialEq, Eq)]
enum ActiveToolModal {
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
    parquet_dialect: ParquetDialect,
    export_csv_mdy: bool,
    show_sql_bar: bool,
    show_chart_drawer: bool,
    dark_mode: bool,
    ui_zoom: f32,
    pending_csv_modal: Option<PendingCsvModal>,
    batch_modal: BatchModalState,
    active_tool: ActiveToolModal,
    status_message: String,

    filter_col: String,
    filter_op: FilterOp,
    filter_val: String,
    filter_keep: bool,

    col_mgr_list: Vec<(String, bool)>,
    rename_old: String,
    rename_new: String,

    cond_new_col: String,
    cond_rules: Vec<CondRule>,
    cond_fallback: String,
    cond_as_float: bool,

    math_new_col: String,
    math_left_col: String,
    math_op: MathOp,
    math_right_is_col: bool,
    math_right_val: String,

    norm_source_col: String,
    norm_new_col: String,

    pivot_group_cols: Vec<(String, bool)>,
    pivot_aggs: Vec<AggSpec>,
    pivot_new_target: String,
    pivot_new_func: AggFunc,
    pivot_new_weight: String,
    pivot_new_alias: String,
    pivot_top_n: usize,
    pivot_open_in_new_tab: bool,

    stack_files_list: Vec<PathBuf>,
    stack_include_source: bool,

    join_left_tab: usize,
    join_right_tab: usize,
    join_kind: SqlJoinKind,
    join_keys_input: String,

    rec_prev_tab: usize,
    rec_curr_tab: usize,
    rec_key_col: String,
    rec_bal_col: String,
    rec_rate_col: String,
    rec_cl_col: String,

    alm_mat_col: String,
    alm_cutoff: String,
    alm_preset: BucketPreset,
    alm_null_sight: bool,
    alm_new_col: String,

    shock_bal_col: String,
    shock_rate_col: String,
    shock_mat_col: String,
    shock_bps: f64,

    ftp_opn_col: String,
    ftp_mat_col: String,
    ftp_bal_col: String,
    ftp_rate_col: String,
    ftp_is_liability: bool,
    ftp_curve: Vec<FtpPoint>,

    audit_opn_col: String,
    audit_mat_col: String,
    audit_cls_col: String,
    audit_stat_col: String,
    audit_bal_col: String,
    audit_rate_col: String,
    audit_key_col: String,
    audit_only_exceptions: bool,

    dur_bal_col: String,
    dur_rate_col: String,
    dur_mat_col: String,
    dur_freq: usize,
    dur_shock_bps: f64,
    dur_sight_yrs: f64,

    loan_bal_col: String,
    loan_cl_col: String,
    loan_seg_col: String,
    loan_susp_col: String,
    loan_sec_col: String,

    lim_col: String,
    lim_bal_col: String,
    lim_ccf_pct: f64,

    macro_name: String,
    macro_desc: String,
    macro_enforce_ctrl: bool,
    macro_include_schema: bool,
    generated_python_code: String,

    multi_export_checks: Vec<bool>,
    split_col_name: String,
    split_format: SplitExportFormat,
    chart_x_col: String,
    chart_y_col: String,
}

impl MicroParquetApp {
    /// Configures a crisp, high-contrast Daylight theme (or high-contrast Dark theme)
    /// and bumps up default font sizes so financial tables are effortless to read.
    pub fn apply_ui_theme(ctx: &egui::Context, dark_mode: bool, zoom: f32) {
        ctx.set_zoom_factor(zoom);

        let mut style = (*ctx.style()).clone();
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(14.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(14.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::new(13.5, egui::FontFamily::Monospace),
        );
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(18.5, egui::FontFamily::Proportional),
        );
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        ctx.set_style(style);

        let mut visuals = if dark_mode {
            let mut v = egui::Visuals::dark();
            // Sharpen dark mode contrast so text is bright white instead of muddy gray
            v.override_text_color = Some(egui::Color32::from_rgb(240, 244, 248));
            v.panel_fill = egui::Color32::from_rgb(22, 27, 34);
            v.faint_bg_color = egui::Color32::from_rgb(33, 38, 45); // Alternating table stripe
            v
        } else {
            let mut v = egui::Visuals::light();
            // Crisp Daylight Paper / Spreadsheet palette
            v.override_text_color = Some(egui::Color32::from_rgb(15, 23, 42)); // Deep Slate Black
            v.panel_fill = egui::Color32::from_rgb(248, 250, 252);              // Soft Alabaster (no harsh glare)
            v.window_fill = egui::Color32::from_rgb(255, 255, 255);             // Pure White modals
            v.faint_bg_color = egui::Color32::from_rgb(236, 242, 249);          // Crisp Excel-blue alternating row stripe
            v.extreme_bg_color = egui::Color32::from_rgb(255, 255, 255);        // Text input boxes
            v.selection.bg_fill = egui::Color32::from_rgb(2, 132, 199);         // Banking Blue selection
            v.selection.stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
            v.widgets.noninteractive.bg_stroke =
                egui::Stroke::new(1.0, egui::Color32::from_rgb(203, 213, 225)); // Clean grid borders
            v
        };

        visuals.striped = true;
        ctx.set_visuals(visuals);
    }
    pub fn new(paths: PortablePaths) -> Self {
        Self {
            paths,
            tabs: Vec::new(),
            active_tab: 0,
            num_format: NumberDisplayFormat::BdtCrore,
            parquet_dialect: ParquetDialect::ArchiveZstd,
            export_csv_mdy: false,
            show_sql_bar: false,
            show_chart_drawer: false,
            dark_mode: false, // Default to crisp Light Mode
            ui_zoom: 1.10,    // Default to 110% high-legibility scale
            pending_csv_modal: None,
            batch_modal: BatchModalState::default(),
            active_tool: ActiveToolModal::None,
            status_message:
                "Ready. Open a CSV/Parquet file, load a .mpq.json macro, or run a Batch conversion."
                    .to_string(),

            filter_col: String::new(),
            filter_op: FilterOp::Eq,
            filter_val: String::new(),
            filter_keep: true,

            col_mgr_list: Vec::new(),
            rename_old: String::new(),
            rename_new: String::new(),

            cond_new_col: "Custom_Flag".to_string(),
            cond_rules: vec![CondRule {
                col_name: "Acstat".to_string(),
                op: FilterOp::Eq,
                compare_val: "DMT".to_string(),
                output_val: "Dormant".to_string(),
            }],
            cond_fallback: "Active/Other".to_string(),
            cond_as_float: false,

            math_new_col: "Balance_Crore".to_string(),
            math_left_col: "Balance".to_string(),
            math_op: MathOp::Div,
            math_right_is_col: false,
            math_right_val: "10000000".to_string(),

            norm_source_col: "Acttit".to_string(),
            norm_new_col: "Normalized_Entity".to_string(),

            pivot_group_cols: Vec::new(),
            pivot_aggs: Vec::new(),
            pivot_new_target: "Balance".to_string(),
            pivot_new_func: AggFunc::Sum,
            pivot_new_weight: "Balance".to_string(),
            pivot_new_alias: "Total_Balance".to_string(),
            pivot_top_n: 0,
            pivot_open_in_new_tab: true,

            stack_files_list: Vec::new(),
            stack_include_source: true,

            join_left_tab: 0,
            join_right_tab: 0,
            join_kind: SqlJoinKind::Left,
            join_keys_input: "Actnum".to_string(),

            rec_prev_tab: 0,
            rec_curr_tab: 0,
            rec_key_col: "Actnum".to_string(),
            rec_bal_col: "Balance".to_string(),
            rec_rate_col: "Int Rate".to_string(),
            rec_cl_col: String::new(),

            alm_mat_col: "Matdat".to_string(),
            alm_cutoff: "2026-10-31".to_string(),
            alm_preset: BucketPreset::BangladeshBankSlp,
            alm_null_sight: true,
            alm_new_col: "SLP_Maturity_Bucket".to_string(),

            shock_bal_col: "Balance".to_string(),
            shock_rate_col: "Int Rate".to_string(),
            shock_mat_col: "Matdat".to_string(),
            shock_bps: 100.0,

            ftp_opn_col: "Opndat".to_string(),
            ftp_mat_col: "Matdat".to_string(),
            ftp_bal_col: "Balance".to_string(),
            ftp_rate_col: "Int Rate".to_string(),
            ftp_is_liability: true,
            ftp_curve: default_bd_ftp_curve(),

            audit_opn_col: "Opndat".to_string(),
            audit_mat_col: "Matdat".to_string(),
            audit_cls_col: "Clsdat".to_string(),
            audit_stat_col: "Acstat".to_string(),
            audit_bal_col: "Balance".to_string(),
            audit_rate_col: "Int Rate".to_string(),
            audit_key_col: "Actnum".to_string(),
            audit_only_exceptions: true,

            dur_bal_col: "Balance".to_string(),
            dur_rate_col: "Int Rate".to_string(),
            dur_mat_col: "Matdat".to_string(),
            dur_freq: 4,
            dur_shock_bps: 200.0,
            dur_sight_yrs: 0.0833,

            loan_bal_col: "Balance".to_string(),
            loan_cl_col: "CL_Status".to_string(),
            loan_seg_col: "Sector".to_string(),
            loan_susp_col: String::new(),
            loan_sec_col: String::new(),

            lim_col: "Sanctioned_Limit".to_string(),
            lim_bal_col: "Balance".to_string(),
            lim_ccf_pct: 20.0,

            macro_name: "Monthly_WB_ALCO_Pipeline".to_string(),
            macro_desc: "Sanitizes dump, buckets SLP maturities, and computes WAVG/Duration."
                .to_string(),
            macro_enforce_ctrl: true,
            macro_include_schema: true,
            generated_python_code: String::new(),

            multi_export_checks: Vec::new(),
            split_col_name: "Brancd".to_string(),
            split_format: SplitExportFormat::Parquet,
            chart_x_col: String::new(),
            chart_y_col: String::new(),
        }
    }

    fn initiate_file_open(&mut self, direct_convert_only: bool) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Data Files (CSV, Parquet)", &["csv", "parquet"])
            .pick_file()
        else {
            return;
        };

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if ext == "parquet" {
            self.create_tab_from_source(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                TabSource::SingleFile {
                    path,
                    is_parquet: true,
                    config: IngestionConfig::default(),
                },
                Vec::new(),
            );
        } else {
            match (preview_csv_raw(&path, 5), auto_detect_smart_schema(&path)) {
                (Ok(preview_df), Ok(config)) => {
                    self.pending_csv_modal = Some(PendingCsvModal {
                        csv_path: path,
                        raw_preview_df: preview_df,
                        config,
                        direct_convert_only,
                    });
                }
                (Err(e), _) | (_, Err(e)) => {
                    self.status_message = format!("Error reading CSV preview: {}", e);
                }
            }
        }
    }

    fn create_tab_from_source(
        &mut self,
        title: String,
        source: TabSource,
        steps: Vec<TransformStep>,
    ) {
        match compile_tab_lazy(&source, &steps, &self.paths.temp_dir) {
            Ok(lf) => {
                let slice = fetch_viewport_slice(lf.clone(), 0, 100).unwrap_or_default();
                let default_ctrl = Self::guess_balance_column(&slice);
                let (total_rows, control_sum) =
                    compute_control_totals(lf, default_ctrl.as_deref()).unwrap_or((0, None));

                self.tabs.push(TabState {
                    title,
                    source,
                    steps,
                    total_rows,
                    control_col: default_ctrl,
                    control_sum,
                    baseline_control_sum: control_sum,
                    viewport_offset: 0,
                    viewport_limit: 100,
                    cached_slice: slice,
                    sql_scratchpad: "SELECT * FROM self LIMIT 100".to_string(),
                });
                self.active_tab = self.tabs.len() - 1;
                self.init_tool_defaults_from_active_tab();
                self.status_message = "Tab created and evaluated out-of-core.".to_string();
            }
            Err(e) => {
                self.status_message = format!("Pipeline error: {}", e);
            }
        }
    }

    fn init_tool_defaults_from_active_tab(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let cols = tab.available_columns();
        if let Some(first) = cols.first() {
            if self.filter_col.is_empty() || !cols.contains(&self.filter_col) {
                self.filter_col = first.clone();
            }
            self.rename_old = first.clone();
            self.chart_x_col = first.clone();
        }
        if let Some(second) = cols.get(1) {
            self.chart_y_col = second.clone();
        }
        self.col_mgr_list = cols.iter().map(|c| (c.clone(), true)).collect();
        self.pivot_group_cols = cols.iter().map(|c| (c.clone(), false)).collect();
        self.multi_export_checks = vec![true; self.tabs.len()];
    }

    fn push_step_to_active_tab(&mut self, step: TransformStep, open_new_tab: bool) {
        let Some(curr) = self.tabs.get(self.active_tab).cloned() else {
            return;
        };

        if open_new_tab {
            let mut new_steps = curr.steps.clone();
            let title = format!("{} - {}", curr.title, step.short_label());
            new_steps.push(step);
            self.create_tab_from_source(title, curr.source, new_steps);
        } else {
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                tab.steps.push(step);
                tab.viewport_offset = 0;
            }
            self.refresh_active_tab();
        }
    }

    fn guess_balance_column(df: &DataFrame) -> Option<String> {
        for col in df.get_columns() {
            let lower = col.name().to_lowercase();
            if lower.contains("balance") || lower.contains("amount") || lower.contains("delta") {
                return Some(col.name().to_string());
            }
        }
        None
    }

    fn refresh_active_tab(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        match compile_tab_lazy(&tab.source, &tab.steps, &self.paths.temp_dir) {
            Ok(lf) => {
                if let Ok(slice) =
                    fetch_viewport_slice(lf.clone(), tab.viewport_offset, tab.viewport_limit)
                {
                    tab.cached_slice = slice;
                }
                if let Ok((rows, sum)) = compute_control_totals(lf, tab.control_col.as_deref()) {
                    tab.total_rows = rows;
                    tab.control_sum = sum;
                }
                self.init_tool_defaults_from_active_tab();
            }
            Err(e) => {
                self.status_message = format!("Step execution failed: {}", e);
            }
        }
    }

    fn load_and_apply_macro_interactive(&mut self, into_new_tab: bool) {
        let Some(path) = rfd::FileDialog::new()
            .set_directory(&self.paths.macros_dir)
            .add_filter("MicroParquet Macro (.mpq.json)", &["json"])
            .pick_file()
        else {
            return;
        };

        match load_macro_from_file(&path) {
            Ok(recipe) => {
                let Some(curr) = self.tabs.get(self.active_tab).cloned() else {
                    self.status_message =
                        "Open a source file first before replaying a macro.".to_string();
                    return;
                };

                if into_new_tab {
                    self.create_tab_from_source(
                        format!("Macro: {}", recipe.name),
                        curr.source,
                        recipe.steps,
                    );
                } else {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.steps.extend(recipe.steps);
                        if recipe.enforce_balance_control && recipe.control_column.is_some() {
                            tab.control_col = recipe.control_column;
                        }
                    }
                    self.refresh_active_tab();
                }
                self.status_message = format!("Loaded and executed macro '{}'.", recipe.name);
            }
            Err(e) => {
                self.status_message = format!("Failed to load macro: {}", e);
            }
        }
    }

    fn prepare_python_preview_for_active_tab(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let (path, is_parq, cfg) = match &tab.source {
            TabSource::SingleFile {
                path,
                is_parquet,
                config,
            } => (path.clone(), *is_parquet, config.clone()),
            _ => (
                PathBuf::from("combined_or_joined_source.parquet"),
                true,
                IngestionConfig::default(),
            ),
        };
        self.generated_python_code = generate_python_polars_code(
            &path,
            is_parq,
            &cfg,
            &tab.steps,
            tab.control_col.as_deref(),
        );
        self.active_tool = ActiveToolModal::PythonCodePreview;
    }
}

impl eframe::App for MicroParquetApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Top Menu Bar
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
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                if ui.button("🔄 Batch Folder Mirror").clicked() {
                    self.batch_modal.open = true;
                }

                ui.menu_button("🔗 Merge & SQL Join", |ui| {
                    if ui
                        .button("📚 Vertical Stack / Merge Multiple Files (Req #11)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::StackFiles;
                    }
                    if ui
                        .button("🔗 SQL Join Two Open Tabs (Req #12)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::JoinTabs;
                    }
                    if ui
                        .button("⚖ Two-Period Variance & CL Migration Reconciler (A6/C2)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::ReconcilePeriods;
                    }
                });

                ui.menu_button("🏦 Banking, ALM & Loans Suite", |ui| {
                    ui.strong("Treasury & ALM (Liquidity / IRRBB):");
                    if ui
                        .button("📅 Regulatory Maturity Bucketer (BB SLP / LCR) (A5)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::MaturityBucket;
                    }
                    if ui
                        .button("📈 Interest Rate Shock / ΔNII Simulator (B1)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::RateShockSim;
                    }
                    if ui
                        .button("⏱ Duration, DV01 & ΔEVE Sensitivity Engine (IRRBB)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::DurationAnalysis;
                    }
                    if ui
                        .button("🧮 Automated FTP Yield Curve Mapper (B2)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::FtpCurveMap;
                    }
                    ui.separator();
                    ui.strong("Loans & Advances (Credit / CL Suite):");
                    if ui
                        .button("🏦 Bangladesh Bank CL Provision & BFP Calculator (C1)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::LoanProvision;
                    }
                    if ui
                        .button("💳 Loan Limit Utilization, Over-Limit & Undrawn CCF (C5)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::LoanLimitUtil;
                    }
                    ui.separator();
                    ui.strong("Hygiene & Counterparty Grouping:");
                    if ui
                        .button("🛡 Core Banking Dirty Data & Ghost Auditor (B3)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::BankingAudit;
                    }
                    if ui
                        .button("🏢 Counterparty Entity Name Normalizer (B8)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::NormalizeEntity;
                    }
                });

                ui.menu_button("⚙ Macros & Python", |ui| {
                    if ui
                        .button("💾 Save Active Tab Steps as Macro (.mpq.json)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.active_tool = ActiveToolModal::SaveMacro;
                    }
                    if ui
                        .button("▶ Load & Replay Macro on Current Tab...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.load_and_apply_macro_interactive(false);
                    }
                    if ui
                        .button("➕ Load & Replay Macro into New View Tab...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.load_and_apply_macro_interactive(true);
                    }
                    ui.separator();
                    if ui
                        .button("🐍 View / Export Pipeline as Python Polars Code (.py)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.prepare_python_preview_for_active_tab();
                    }
                });

                ui.menu_button("📤 Export & Reports", |ui| {
                    if ui.button("Export Active Tab to Parquet (.parquet)...").clicked() {
                        ui.close_menu();
                        self.export_active_tab_parquet();
                    }
                    if ui.button("Export Active Tab to CSV (.csv)...").clicked() {
                        ui.close_menu();
                        self.export_active_tab_csv();
                    }
                    ui.separator();
                    if ui
                        .button("📊 Export Multiple Tabs to Single Excel Workbook (.xlsx)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.multi_export_checks = vec![true; self.tabs.len()];
                        self.active_tool = ActiveToolModal::MultiTabExport;
                    }
                    if ui
                        .button("📄 Export Multiple Tabs to Single Landscape PDF Report (.pdf)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.multi_export_checks = vec![true; self.tabs.len()];
                        self.active_tool = ActiveToolModal::MultiTabExport;
                    }
                    ui.separator();
                    if ui
                        .button("🗂 Smart Partitioned Split Export by Column (Brancd/Glcode)...")
                        .clicked()
                    {
                        ui.close_menu();
                        self.init_tool_defaults_from_active_tab();
                        self.active_tool = ActiveToolModal::SplitByColumn;
                    }
                });

                ui.separator();
                egui::ComboBox::from_id_salt("parquet_dialect_combo")
                    .selected_text(self.parquet_dialect.label())
                    .show_ui(ui, |ui| {
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

                egui::ComboBox::from_id_salt("num_fmt_combo")
                    .selected_text(self.num_format.label())
                    .show_ui(ui, |ui| {
                        for fmt in [
                            NumberDisplayFormat::BdtCrore,
                            NumberDisplayFormat::BdtLakh,
                            NumberDisplayFormat::StandardCommas,
                            NumberDisplayFormat::Raw,
                        ] {
                            ui.selectable_value(&mut self.num_format, fmt, fmt.label());
                        }
                    });
                ui.separator();
                let theme_label = if self.dark_mode { "☀ Light" } else { "🌙 Dark" };
                if ui
                    .button(theme_label)
                    .on_hover_text("Switch between High-Contrast Daylight and Dark Theme")
                    .clicked()
                {
                    self.dark_mode = !self.dark_mode;
                    Self::apply_ui_theme(ctx, self.dark_mode, self.ui_zoom);
                }

                if ui.small_button("🔍-").on_hover_text("Zoom Out UI").clicked() {
                    self.ui_zoom = (self.ui_zoom - 0.10).clamp(0.85, 1.60);
                    Self::apply_ui_theme(ctx, self.dark_mode, self.ui_zoom);
                }
                ui.label(format!("{:.0}%", self.ui_zoom * 100.0));
                if ui.small_button("🔍+").on_hover_text("Zoom In UI").clicked() {
                    self.ui_zoom = (self.ui_zoom + 0.10).clamp(0.85, 1.60);
                    Self::apply_ui_theme(ctx, self.dark_mode, self.ui_zoom);
                }
            });
        });

        // 2. Bottom Pinned Control Total & Reconciliation Bar
        egui::TopBottomPanel::bottom("status_reconciliation_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    ui.strong(format!(
                        "Rows: {}",
                        format_with_commas(tab.total_rows as f64, 0)
                    ));
                    ui.separator();

                    if let (Some(col_name), Some(sum_val)) = (&tab.control_col, tab.control_sum) {
                        ui.colored_label(
                            egui::Color32::from_rgb(0, 110, 200),
                            format!(
                                "Control [{}]: {:.2} Cr (Raw: {})",
                                col_name,
                                sum_val / 10_000_000.0,
                                format_with_commas(sum_val, 2)
                            ),
                        );
                        if let Some(base) = tab.baseline_control_sum {
                            let diff = sum_val - base;
                            if diff.abs() > 0.01 {
                                ui.colored_label(
                                    egui::Color32::from_rgb(210, 70, 0),
                                    format!("⚠ Δ vs Source: {:+.2} Cr", diff / 10_000_000.0),
                                );
                            }
                        }
                        ui.separator();
                    }
                }
                ui.label(&self.status_message);
            });
        });

        // 3. Right Side Panel: Applied Pipeline Steps & Macro Controls
        if !self.tabs.is_empty() {
            egui::SidePanel::right("applied_steps_sidebar")
                .resizable(true)
                .default_width(265.0)
                .show(ctx, |ui| {
                    ui.heading("📋 Applied Steps (Macro)");
                    ui.horizontal(|ui| {
                        if ui.small_button("💾 Save .mpq").clicked() {
                            self.active_tool = ActiveToolModal::SaveMacro;
                        }
                        if ui.small_button("▶ Load .mpq").clicked() {
                            self.load_and_apply_macro_interactive(false);
                        }
                        if ui.small_button("🐍 To .py").clicked() {
                            self.prepare_python_preview_for_active_tab();
                        }
                    });
                    ui.separator();

                    let mut step_to_remove: Option<usize> = None;
                    let mut clear_all = false;

                    if let Some(tab) = self.tabs.get(self.active_tab) {
                        ui.label("0. [Source Locked & Sanitized]");
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for (idx, step) in tab.steps.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(format!("{}. {}", idx + 1, step.short_label()));
                                    if ui.small_button("✖").clicked() {
                                        step_to_remove = Some(idx);
                                    }
                                });
                            }
                        });
                        if !tab.steps.is_empty() {
                            ui.separator();
                            if ui.button("🗑 Reset All Steps").clicked() {
                                clear_all = true;
                            }
                        }
                    }

                    if let Some(idx) = step_to_remove {
                        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                            tab.steps.remove(idx);
                        }
                        self.refresh_active_tab();
                    } else if clear_all {
                        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                            tab.steps.clear();
                        }
                        self.refresh_active_tab();
                    }
                });
        }

        // 4. Central Workspace
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.tabs.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.heading(
                        "MicroParquet v1.1: Portable CSV/Parquet, ALM & Loans Analytics Workstation",
                    );
                    ui.label("Out-of-Core Polars Engine • Multi-Tab Views • Duration & ΔEVE • BB CL Provisioning • Macros");
                    ui.add_space(20.0);
                    if ui.button("📂 Open CSV or Parquet File...").clicked() {
                        self.initiate_file_open(false);
                    }
                    ui.add_space(8.0);
                    if ui.button("📚 Stack / Merge Multiple Files...").clicked() {
                        self.active_tool = ActiveToolModal::StackFiles;
                    }
                    ui.add_space(8.0);
                    if ui
                        .button("🔄 Launch Recursive Folder Batch Converter...")
                        .clicked()
                    {
                        self.batch_modal.open = true;
                    }
                });
                return;
            }

            // Tab Strip
            let mut tab_to_close: Option<usize> = None;
            let mut duplicate_tab: Option<usize> = None;
            ui.horizontal_wrapped(|ui| {
                for idx in 0..self.tabs.len() {
                    let is_selected = idx == self.active_tab;
                    let tab_label = format!("{} ({})", self.tabs[idx].title, idx + 1);
                    if ui.selectable_label(is_selected, tab_label).clicked() {
                        self.active_tab = idx;
                        self.init_tool_defaults_from_active_tab();
                    }
                    if ui.small_button("➕").on_hover_text("Clone View Tab").clicked() {
                        duplicate_tab = Some(idx);
                    }
                    if ui.small_button("✖").on_hover_text("Close Tab").clicked() {
                        tab_to_close = Some(idx);
                    }
                    ui.separator();
                }
            });

            if let Some(idx) = duplicate_tab {
                if let Some(orig) = self.tabs.get(idx).cloned() {
                    let mut cloned = orig;
                    cloned.title = format!("View of {}", cloned.title);
                    self.tabs.push(cloned);
                    self.active_tab = self.tabs.len() - 1;
                }
            }

            if let Some(idx) = tab_to_close {
                self.tabs.remove(idx);
                if self.active_tab >= self.tabs.len() && !self.tabs.is_empty() {
                    self.active_tab = self.tabs.len() - 1;
                }
                return;
            }

            ui.separator();

            // Quick Transformation & Clipboard Action Bar
            ui.horizontal_wrapped(|ui| {
                ui.strong("Transform:");
                if ui.button("🔍 Filter/Delete").clicked() {
                    self.init_tool_defaults_from_active_tab();
                    self.active_tool = ActiveToolModal::FilterRows;
                }
                if ui.button("📐 Columns").clicked() {
                    self.init_tool_defaults_from_active_tab();
                    self.active_tool = ActiveToolModal::ManageColumns;
                }
                if ui.button("➕ Cond Col").clicked() {
                    self.active_tool = ActiveToolModal::ConditionalCol;
                }
                if ui.button("🧮 Math Col").clicked() {
                    self.active_tool = ActiveToolModal::MathFormula;
                }
                if ui.button("📊 Pivot / WAVG").clicked() {
                    self.init_tool_defaults_from_active_tab();
                    self.active_tool = ActiveToolModal::GroupByPivot;
                }
                if ui.button("⏱ Duration").clicked() {
                    self.active_tool = ActiveToolModal::DurationAnalysis;
                }
                if ui.button("🏦 CL Prov").clicked() {
                    self.active_tool = ActiveToolModal::LoanProvision;
                }
                if ui.selectable_label(self.show_sql_bar, "💻 SQL").clicked() {
                    self.show_sql_bar = !self.show_sql_bar;
                }
                if ui
                    .selectable_label(self.show_chart_drawer, "📈 ALCO Chart")
                    .clicked()
                {
                    self.init_tool_defaults_from_active_tab();
                    self.show_chart_drawer = !self.show_chart_drawer;
                }
                ui.separator();
                if ui
                    .button("📋 Copy for Slide/Excel")
                    .on_hover_text(
                        "Copies visible slice/pivot as Tab-Delimited table in current Crore/Lakh format",
                    )
                    .clicked()
                {
                    if let Some(tab) = self.tabs.get(self.active_tab) {
                        let txt =
                            format_df_for_clipboard(&tab.cached_slice, self.num_format, false);
                        ctx.copy_text(txt);
                        self.status_message =
                            "Copied formatted table to clipboard (ready to paste into PowerPoint/Excel)."
                                .to_string();
                    }
                }
                if ui
                    .button("📋 Copy Markdown")
                    .on_hover_text("Copies visible slice/pivot as a Markdown table")
                    .clicked()
                {
                    if let Some(tab) = self.tabs.get(self.active_tab) {
                        let txt = format_df_for_clipboard(&tab.cached_slice, self.num_format, true);
                        ctx.copy_text(txt);
                        self.status_message = "Copied Markdown table to clipboard.".to_string();
                    }
                }
            });

            if self.show_sql_bar {
                ui.separator();
                let mut sql_to_run: Option<(String, bool)> = None;
                if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                    ui.horizontal(|ui| {
                        ui.strong("SQL (`self`):");
                        ui.add(
                            egui::TextEdit::singleline(&mut tab.sql_scratchpad)
                                .desired_width(600.0),
                        );
                        if ui.button("▶ Apply Here").clicked() {
                            sql_to_run = Some((tab.sql_scratchpad.clone(), false));
                        }
                        if ui.button("➕ New Tab").clicked() {
                            sql_to_run = Some((tab.sql_scratchpad.clone(), true));
                        }
                    });
                }
                if let Some((q, new_tab)) = sql_to_run {
                    self.push_step_to_active_tab(TransformStep::SqlQuery { query: q }, new_tab);
                }
            }

            if self.show_chart_drawer {
                ui.separator();
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    let avail = tab.available_columns();
                    ui.horizontal(|ui| {
                        ui.strong("ALCO Visual Bar Chart:");
                        ui.label("Category (X):");
                        egui::ComboBox::from_id_salt("chart_x")
                            .selected_text(&self.chart_x_col)
                            .show_ui(ui, |ui| {
                                for c in &avail {
                                    ui.selectable_value(&mut self.chart_x_col, c.clone(), c);
                                }
                            });
                        ui.label("Value (Y - in BDT Crore):");
                        egui::ComboBox::from_id_salt("chart_y")
                            .selected_text(&self.chart_y_col)
                            .show_ui(ui, |ui| {
                                for c in &avail {
                                    ui.selectable_value(&mut self.chart_y_col, c.clone(), c);
                                }
                            });
                    });

                    Self::render_bar_chart(
                        ui,
                        &tab.cached_slice,
                        &self.chart_x_col,
                        &self.chart_y_col,
                    );
                }
            }

            ui.separator();

            let mut needs_refresh = false;
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Showing rows {} – {} of {}",
                        tab.viewport_offset + 1,
                        (tab.viewport_offset + tab.cached_slice.height()).min(tab.total_rows),
                        tab.total_rows
                    ));

                    if ui.button("⏮ First").clicked() && tab.viewport_offset > 0 {
                        tab.viewport_offset = 0;
                        needs_refresh = true;
                    }
                    if ui.button("◀ Prev 100").clicked() && tab.viewport_offset > 0 {
                        tab.viewport_offset =
                            tab.viewport_offset.saturating_sub(tab.viewport_limit);
                        needs_refresh = true;
                    }
                    if ui.button("Next 100 ▶").clicked()
                        && tab.viewport_offset + tab.viewport_limit < tab.total_rows
                    {
                        tab.viewport_offset += tab.viewport_limit;
                        needs_refresh = true;
                    }

                    ui.separator();
                    ui.label("Pinned Control Col:");
                    let current_ctrl = tab
                        .control_col
                        .clone()
                        .unwrap_or_else(|| "None".to_string());
                    egui::ComboBox::from_id_salt("ctrl_col_selector")
                        .selected_text(current_ctrl)
                        .show_ui(ui, |ui| {
                            for col_series in tab.cached_slice.get_columns() {
                                let cname = col_series.name().to_string();
                                if ui
                                    .selectable_value(
                                        &mut tab.control_col,
                                        Some(cname.clone()),
                                        &cname,
                                    )
                                    .clicked()
                                {
                                    needs_refresh = true;
                                }
                            }
                        });
                });

                ui.separator();
                Self::render_data_grid(ui, &tab.cached_slice, tab.viewport_offset, self.num_format);
            }

            if needs_refresh {
                self.refresh_active_tab();
            }
        });

        // Modals
        self.render_csv_schema_modal(ctx);
        self.render_batch_modal(ctx);
        self.render_active_tool_modal(ctx);
    }
}

impl MicroParquetApp {
    fn render_bar_chart(ui: &mut egui::Ui, df: &DataFrame, x_col: &str, y_col: &str) {
        let Ok(x_series) = df.column(x_col) else {
            return;
        };
        let Ok(y_series) = df.column(y_col) else {
            return;
        };

        let n = df.height().min(25);
        let mut bars = Vec::with_capacity(n);
        for i in 0..n {
            let label = match x_series.get(i) {
                Ok(AnyValue::String(s)) => s.to_string(),
                Ok(other) => format!("{}", other),
                Err(_) => format!("{}", i),
            };
            let val_cr = match y_series.get(i) {
                Ok(AnyValue::Float64(v)) => v / 10_000_000.0,
                Ok(AnyValue::Float32(v)) => (v as f64) / 10_000_000.0,
                Ok(AnyValue::Int64(v)) => (v as f64) / 10_000_000.0,
                Ok(AnyValue::UInt32(v)) => (v as f64) / 10_000_000.0,
                _ => 0.0,
            };
            bars.push(Bar::new(i as f64, val_cr).name(label));
        }

        Plot::new("alco_bar_plot")
            .height(175.0)
            .show_axes([true, true])
            .show(ui, |plot_ui| {
                plot_ui.bar_chart(BarChart::new(bars).width(0.65));
            });
    }

    fn render_data_grid(
        ui: &mut egui::Ui,
        df: &DataFrame,
        row_offset: usize,
        num_format: NumberDisplayFormat,
    ) {
        let cols = df.get_columns();
        let num_rows = df.height();

        egui::ScrollArea::horizontal().show(ui, |ui| {
            let mut builder = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(55.0).at_least(40.0));

            for _ in cols {
                builder = builder.column(Column::initial(145.0).at_least(80.0).clip(true));
            }

            builder
                .header(32.0, |mut header| {
                    header.col(|ui| {
                        ui.strong("#");
                    });
                    for c in cols {
                        header.col(|ui| {
                            ui.strong(format!("{}\n({:?})", c.name(), c.dtype()));
                        });
                    }
                })
                .body(|body| {
                    body.rows(25.0, num_rows, |mut row| {
                        let r_idx = row.index();
                        row.col(|ui| {
                            ui.label((row_offset + r_idx + 1).to_string());
                        });
                        for c in cols {
                            row.col(|ui| {
                                let cell_text = match c.get(r_idx) {
                                    Ok(AnyValue::Null) => "null".to_string(),
                                    Ok(AnyValue::Float64(v)) => num_format.format_f64(v),
                                    Ok(AnyValue::Float32(v)) => num_format.format_f64(v as f64),
                                    Ok(AnyValue::String(s)) => s.to_string(),
                                    Ok(other) => format!("{}", other),
                                    Err(_) => "".to_string(),
                                };
                                ui.label(cell_text);
                            });
                        }
                    });
                });
        });
    }

    fn render_active_tool_modal(&mut self, ctx: &egui::Context) {
        if self.active_tool == ActiveToolModal::None {
            return;
        }

        let avail_cols = self
            .tabs
            .get(self.active_tab)
            .map(|t| t.available_columns())
            .unwrap_or_default();

        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;

        match self.active_tool {
            ActiveToolModal::FilterRows => {
                egui::Window::new("🔍 Filter Rows / Delete Filtered Rows (Req #6)")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Column:");
                            egui::ComboBox::from_id_salt("flt_col")
                                .selected_text(&self.filter_col)
                                .show_ui(ui, |ui| {
                                    for c in &avail_cols {
                                        ui.selectable_value(&mut self.filter_col, c.clone(), c);
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label("Condition:");
                            egui::ComboBox::from_id_salt("flt_op")
                                .selected_text(self.filter_op.label())
                                .show_ui(ui, |ui| {
                                    for op in FilterOp::all() {
                                        ui.selectable_value(&mut self.filter_op, *op, op.label());
                                    }
                                });
                        });
                        if self.filter_op != FilterOp::IsNull
                            && self.filter_op != FilterOp::IsNotNull
                        {
                            ui.horizontal(|ui| {
                                ui.label("Value:");
                                ui.text_edit_singleline(&mut self.filter_val);
                            });
                        }
                        ui.horizontal(|ui| {
                            ui.radio_value(
                                &mut self.filter_keep,
                                true,
                                "Keep Matching Rows (Standard Filter)",
                            );
                            ui.radio_value(
                                &mut self.filter_keep,
                                false,
                                "Delete Matching Rows (Invert Filter)",
                            );
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("✅ Apply to Current Tab").clicked() {
                                step_to_apply = Some((
                                    TransformStep::FilterRows {
                                        column: self.filter_col.clone(),
                                        operator: self.filter_op,
                                        value: self.filter_val.clone(),
                                        keep_matching: self.filter_keep,
                                    },
                                    false,
                                ));
                                close_modal = true;
                            }
                            if ui.button("➕ Open Filtered View in New Tab").clicked() {
                                step_to_apply = Some((
                                    TransformStep::FilterRows {
                                        column: self.filter_col.clone(),
                                        operator: self.filter_op,
                                        value: self.filter_val.clone(),
                                        keep_matching: self.filter_keep,
                                    },
                                    true,
                                ));
                                close_modal = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close_modal = true;
                            }
                        });
                    });
            }

            ActiveToolModal::ManageColumns => {
                egui::Window::new("📐 Manage Columns: Drop, Rearrange & Rename (Req #6)")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.strong("1. Check Columns to Keep & Use ⬆/⬇ to Rearrange Order:");
                        egui::ScrollArea::vertical()
                            .max_height(220.0)
                            .show(ui, |ui| {
                                let len = self.col_mgr_list.len();
                                let mut swap_pair: Option<(usize, usize)> = None;
                                for i in 0..len {
                                    ui.horizontal(|ui| {
                                        if ui.small_button("⬆").clicked() && i > 0 {
                                            swap_pair = Some((i, i - 1));
                                        }
                                        if ui.small_button("⬇").clicked() && i + 1 < len {
                                            swap_pair = Some((i, i + 1));
                                        }
                                        // Fixed E0502 borrow checker error by destructuring tuple
                                        let (ref col_name, ref mut is_checked) =
                                            self.col_mgr_list[i];
                                        ui.checkbox(is_checked, col_name.as_str());
                                    });
                                }
                                if let Some((a, b)) = swap_pair {
                                    self.col_mgr_list.swap(a, b);
                                }
                            });

                        if ui.button("✅ Apply Column Selection & Order").clicked() {
                            let kept: Vec<String> = self
                                .col_mgr_list
                                .iter()
                                .filter(|(_, k)| *k)
                                .map(|(n, _)| n.clone())
                                .collect();
                            step_to_apply = Some((
                                TransformStep::SelectAndReorderColumns { cols: kept },
                                false,
                            ));
                            close_modal = true;
                        }

                        ui.separator();
                        ui.strong("2. Rename a Column:");
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("ren_old")
                                .selected_text(&self.rename_old)
                                .show_ui(ui, |ui| {
                                    for c in &avail_cols {
                                        ui.selectable_value(&mut self.rename_old, c.clone(), c);
                                    }
                                });
                            ui.label("->");
                            ui.text_edit_singleline(&mut self.rename_new);
                            if ui.button("Rename").clicked() && !self.rename_new.trim().is_empty() {
                                step_to_apply = Some((
                                    TransformStep::RenameColumn {
                                        old_name: self.rename_old.clone(),
                                        new_name: self.rename_new.trim().to_string(),
                                    },
                                    false,
                                ));
                                close_modal = true;
                            }
                        });
                        if ui.button("Close").clicked() {
                            close_modal = true;
                        }
                    });
            }

            ActiveToolModal::ConditionalCol => {
                egui::Window::new("➕ Add Conditional Column (IF / ELSE IF / ELSE)")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("New Column Name:");
                            ui.text_edit_singleline(&mut self.cond_new_col);
                            ui.checkbox(&mut self.cond_as_float, "Output as Numeric (Float64)");
                        });
                        ui.separator();

                        let mut remove_idx = None;
                        for (idx, rule) in self.cond_rules.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(if idx == 0 { "IF" } else { "ELSE IF" });
                                egui::ComboBox::from_id_salt(format!("cc_col_{}", idx))
                                    .selected_text(&rule.col_name)
                                    .show_ui(ui, |ui| {
                                        for c in &avail_cols {
                                            ui.selectable_value(&mut rule.col_name, c.clone(), c);
                                        }
                                    });
                                egui::ComboBox::from_id_salt(format!("cc_op_{}", idx))
                                    .selected_text(rule.op.label())
                                    .show_ui(ui, |ui| {
                                        for op in FilterOp::all() {
                                            ui.selectable_value(&mut rule.op, *op, op.label());
                                        }
                                    });
                                ui.add(
                                    egui::TextEdit::singleline(&mut rule.compare_val)
                                        .desired_width(90.0),
                                );
                                ui.label("THEN");
                                ui.add(
                                    egui::TextEdit::singleline(&mut rule.output_val)
                                        .desired_width(90.0),
                                );
                                if ui.small_button("✖").clicked() {
                                    remove_idx = Some(idx);
                                }
                            });
                        }
                        if let Some(i) = remove_idx {
                            if self.cond_rules.len() > 1 {
                                self.cond_rules.remove(i);
                            }
                        }
                        if ui.button("➕ Add ELSE IF Branch").clicked() {
                            self.cond_rules.push(CondRule {
                                col_name: avail_cols.first().cloned().unwrap_or_default(),
                                op: FilterOp::Eq,
                                compare_val: String::new(),
                                output_val: String::new(),
                            });
                        }
                        ui.horizontal(|ui| {
                            ui.label("ELSE (Fallback Value):");
                            ui.text_edit_singleline(&mut self.cond_fallback);
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("✅ Add Column").clicked() {
                                step_to_apply = Some((
                                    TransformStep::ConditionalColumn {
                                        new_col: self.cond_new_col.clone(),
                                        rules: self.cond_rules.clone(),
                                        fallback: self.cond_fallback.clone(),
                                        output_as_float: self.cond_as_float,
                                    },
                                    false,
                                ));
                                close_modal = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close_modal = true;
                            }
                        });
                    });
            }

            ActiveToolModal::MathFormula => {
                egui::Window::new("🧮 Add Mathematical Calculated Column")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("New Column Name:");
                            ui.text_edit_singleline(&mut self.math_new_col);
                        });
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("math_l")
                                .selected_text(&self.math_left_col)
                                .show_ui(ui, |ui| {
                                    for c in &avail_cols {
                                        ui.selectable_value(&mut self.math_left_col, c.clone(), c);
                                    }
                                });
                            for op in [MathOp::Add, MathOp::Sub, MathOp::Mul, MathOp::Div] {
                                ui.selectable_value(&mut self.math_op, op, op.symbol());
                            }
                            ui.checkbox(&mut self.math_right_is_col, "Use Another Column");
                        });
                        ui.horizontal(|ui| {
                            if self.math_right_is_col {
                                egui::ComboBox::from_id_salt("math_r_col")
                                    .selected_text(&self.math_right_val)
                                    .show_ui(ui, |ui| {
                                        for c in &avail_cols {
                                            ui.selectable_value(
                                                &mut self.math_right_val,
                                                c.clone(),
                                                c,
                                            );
                                        }
                                    });
                            } else {
                                ui.label("Scalar Value (e.g. 10000000):");
                                ui.text_edit_singleline(&mut self.math_right_val);
                            }
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("✅ Add Calculated Column").clicked() {
                                step_to_apply = Some((
                                    TransformStep::MathFormula {
                                        new_col: self.math_new_col.clone(),
                                        left_col: self.math_left_col.clone(),
                                        op: self.math_op,
                                        right_is_col: self.math_right_is_col,
                                        right_val_or_col: self.math_right_val.clone(),
                                    },
                                    false,
                                ));
                                close_modal = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close_modal = true;
                            }
                        });
                    });
            }

            ActiveToolModal::NormalizeEntity => {
                egui::Window::new("🏢 Counterparty Entity Name Normalizer (Feature B8)")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.label(
                            "Standardizes LIMITED -> LTD, strips punctuation, and unifies corporate names.",
                        );
                        ui.horizontal(|ui| {
                            ui.label("Source Name Column (e.g. Acttit):");
                            egui::ComboBox::from_id_salt("norm_src")
                                .selected_text(&self.norm_source_col)
                                .show_ui(ui, |ui| {
                                    for c in &avail_cols {
                                        ui.selectable_value(
                                            &mut self.norm_source_col,
                                            c.clone(),
                                            c,
                                        );
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label("New Normalized Column:");
                            ui.text_edit_singleline(&mut self.norm_new_col);
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("✅ Normalize Counterparty Names").clicked() {
                                step_to_apply = Some((
                                    TransformStep::NormalizeEntity {
                                        source_col: self.norm_source_col.clone(),
                                        new_col: self.norm_new_col.clone(),
                                    },
                                    false,
                                ));
                                close_modal = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close_modal = true;
                            }
                        });
                    });
            }

            ActiveToolModal::GroupByPivot => {
                egui::Window::new(
                    "📊 Pivot & Aggregation Builder (with Weighted Average WAVG & % of Total)",
                )
                .collapsible(false)
                .default_width(680.0)
                .show(ctx, |ui| {
                    ui.strong(
                        "1. Select Row Grouping Columns (e.g. Glcode, Sector, SLP_Maturity_Bucket):",
                    );
                    ui.horizontal_wrapped(|ui| {
                        for (cname, checked) in &mut self.pivot_group_cols {
                            ui.checkbox(checked, cname.as_str());
                        }
                    });

                    ui.separator();
                    ui.strong(
                        "2. Add Value Aggregations (Sum, Count, Weighted Average, % of Total):",
                    );
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("piv_func")
                            .selected_text(self.pivot_new_func.label())
                            .show_ui(ui, |ui| {
                                for f in AggFunc::all() {
                                    ui.selectable_value(&mut self.pivot_new_func, *f, f.label());
                                }
                            });
                        ui.label("Target Col:");
                        egui::ComboBox::from_id_salt("piv_tgt")
                            .selected_text(&self.pivot_new_target)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.pivot_new_target, c.clone(), c);
                                }
                            });
                        if self.pivot_new_func == AggFunc::WeightedAvg {
                            ui.label("Weight Col:");
                            egui::ComboBox::from_id_salt("piv_wgt")
                                .selected_text(&self.pivot_new_weight)
                                .show_ui(ui, |ui| {
                                    for c in &avail_cols {
                                        ui.selectable_value(
                                            &mut self.pivot_new_weight,
                                            c.clone(),
                                            c,
                                        );
                                    }
                                });
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Output Alias:");
                        ui.text_edit_singleline(&mut self.pivot_new_alias);
                        if ui.button("➕ Add Metric to Pivot").clicked() {
                            self.pivot_aggs.push(AggSpec {
                                target_col: self.pivot_new_target.clone(),
                                func: self.pivot_new_func,
                                weight_col: if self.pivot_new_func == AggFunc::WeightedAvg {
                                    Some(self.pivot_new_weight.clone())
                                } else {
                                    None
                                },
                                alias: self.pivot_new_alias.clone(),
                            });
                        }
                    });

                    for a in &self.pivot_aggs {
                        ui.label(format!("  • {}: {:?}({})", a.alias, a.func, a.target_col));
                    }
                    if !self.pivot_aggs.is_empty() && ui.small_button("Clear Metrics").clicked() {
                        self.pivot_aggs.clear();
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Top-N Limit (0 = All Rows):");
                        ui.add(egui::DragValue::new(&mut self.pivot_top_n).range(0..=10000));
                        ui.checkbox(&mut self.pivot_open_in_new_tab, "Open Pivot in New Tab");
                    });

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("🚀 Generate Summary / Pivot View").clicked() {
                            let groups: Vec<String> = self
                                .pivot_group_cols
                                .iter()
                                .filter(|(_, c)| *c)
                                .map(|(n, _)| n.clone())
                                .collect();
                            let sort_c = self.pivot_aggs.first().map(|a| a.alias.clone());
                            step_to_apply = Some((
                                TransformStep::GroupByPivot {
                                    group_cols: groups,
                                    pivot_col: None,
                                    aggs: self.pivot_aggs.clone(),
                                    sort_by_col: sort_c,
                                    top_n: if self.pivot_top_n > 0 {
                                        Some(self.pivot_top_n)
                                    } else {
                                        None
                                    },
                                },
                                self.pivot_open_in_new_tab,
                            ));
                            close_modal = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_modal = true;
                        }
                    });
                });
            }

            ActiveToolModal::MaturityBucket => {
                egui::Window::new("📅 Regulatory Maturity Bucketer (Feature A5)")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Maturity Date Column:");
                            egui::ComboBox::from_id_salt("alm_mat")
                                .selected_text(&self.alm_mat_col)
                                .show_ui(ui, |ui| {
                                    for c in &avail_cols {
                                        ui.selectable_value(&mut self.alm_mat_col, c.clone(), c);
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label("Reporting / Cutoff Date (YYYY-MM-DD):");
                            ui.text_edit_singleline(&mut self.alm_cutoff);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Regulatory Preset:");
                            egui::ComboBox::from_id_salt("alm_preset")
                                .selected_text(self.alm_preset.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.alm_preset,
                                        BucketPreset::BangladeshBankSlp,
                                        BucketPreset::BangladeshBankSlp.label(),
                                    );
                                    ui.selectable_value(
                                        &mut self.alm_preset,
                                        BucketPreset::BaselLcrNsfr,
                                        BucketPreset::BaselLcrNsfr.label(),
                                    );
                                });
                        });
                        ui.checkbox(
                            &mut self.alm_null_sight,
                            "Treat Null Maturity Dates (e.g. Current/Savings C01) as Sight/Overnight",
                        );
                        ui.horizontal(|ui| {
                            ui.label("Output Bucket Column Name:");
                            ui.text_edit_singleline(&mut self.alm_new_col);
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("✅ Generate Maturity Buckets").clicked() {
                                step_to_apply = Some((
                                    TransformStep::MaturityBucket {
                                        mat_col: self.alm_mat_col.clone(),
                                        cutoff_date: self.alm_cutoff.clone(),
                                        preset: self.alm_preset,
                                        null_as_sight: self.alm_null_sight,
                                        new_col: self.alm_new_col.clone(),
                                    },
                                    false,
                                ));
                                close_modal = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close_modal = true;
                            }
                        });
                    });
            }

            ActiveToolModal::RateShockSim => {
                egui::Window::new(
                    "📈 Interest Rate Shock / ΔNII Sensitivity Simulator (Feature B1)",
                )
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("Calculates 1-Year repricing-weighted Net Interest Income impact.");
                    ui.horizontal(|ui| {
                        ui.label("Balance Col:");
                        egui::ComboBox::from_id_salt("shk_bal")
                            .selected_text(&self.shock_bal_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.shock_bal_col, c.clone(), c);
                                }
                            });
                        ui.label("Int Rate Col:");
                        egui::ComboBox::from_id_salt("shk_rt")
                            .selected_text(&self.shock_rate_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.shock_rate_col, c.clone(), c);
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Maturity Date Col:");
                        egui::ComboBox::from_id_salt("shk_mat")
                            .selected_text(&self.shock_mat_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.shock_mat_col, c.clone(), c);
                                }
                            });
                        ui.label("Cutoff Date (YYYY-MM-DD):");
                        ui.text_edit_singleline(&mut self.alm_cutoff);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Parallel Rate Shock (bps, e.g. +100, +200, -150):");
                        ui.add(egui::DragValue::new(&mut self.shock_bps).speed(5.0));
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("✅ Run Rate Shock Simulation").clicked() {
                            step_to_apply = Some((
                                TransformStep::RateShockSim {
                                    bal_col: self.shock_bal_col.clone(),
                                    rate_col: self.shock_rate_col.clone(),
                                    mat_col: self.shock_mat_col.clone(),
                                    cutoff_date: self.alm_cutoff.clone(),
                                    shock_bps: self.shock_bps,
                                },
                                true,
                            ));
                            close_modal = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_modal = true;
                        }
                    });
                });
            }

            ActiveToolModal::DurationAnalysis => {
                egui::Window::new(
                    "⏱ IRRBB Duration, DV01 & ΔEVE Sensitivity Engine (Macaulay & Modified)",
                )
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("Computes Residual Years (T), Macaulay Duration, Modified Duration, DV01, and ΔEVE under rate shocks.");
                    ui.horizontal(|ui| {
                        ui.label("Balance Col:");
                        egui::ComboBox::from_id_salt("dur_bal")
                            .selected_text(&self.dur_bal_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.dur_bal_col, c.clone(), c);
                                }
                            });
                        ui.label("Int Rate / Yield Col (%):");
                        egui::ComboBox::from_id_salt("dur_rt")
                            .selected_text(&self.dur_rate_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.dur_rate_col, c.clone(), c);
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Maturity Date Col:");
                        egui::ComboBox::from_id_salt("dur_mat")
                            .selected_text(&self.dur_mat_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.dur_mat_col, c.clone(), c);
                                }
                            });
                        ui.label("Cutoff Date (YYYY-MM-DD):");
                        ui.text_edit_singleline(&mut self.alm_cutoff);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Compounding Frequency per Year (1=Ann, 2=Semi, 4=Qtr, 12=Mo):");
                        ui.add(egui::DragValue::new(&mut self.dur_freq).range(1..=12));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Parallel Rate Shock for ΔEVE (bps, e.g. +200):");
                        ui.add(egui::DragValue::new(&mut self.dur_shock_bps).speed(10.0));
                        ui.label("Behavioral Tenor for Null Matdat (Yrs):");
                        ui.add(egui::DragValue::new(&mut self.dur_sight_yrs).speed(0.05));
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("✅ Compute Duration & ΔEVE Columns").clicked() {
                            step_to_apply = Some((
                                TransformStep::DurationAnalysis {
                                    bal_col: self.dur_bal_col.clone(),
                                    rate_col: self.dur_rate_col.clone(),
                                    mat_col: self.dur_mat_col.clone(),
                                    cutoff_date: self.alm_cutoff.clone(),
                                    compounding_freq: self.dur_freq,
                                    shock_bps: self.dur_shock_bps,
                                    default_sight_years: self.dur_sight_yrs,
                                },
                                false,
                            ));
                            close_modal = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_modal = true;
                        }
                    });
                });
            }

            ActiveToolModal::LoanProvision => {
                egui::Window::new(
                    "🏦 Loans & Advances: Bangladesh Bank CL Provision & BFP Calculator (C1)",
                )
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("Computes NPL Category, Base For Provision (BFP), Statutory Rate %, and Required Provision BDT.");
                    ui.horizontal(|ui| {
                        ui.label("Outstanding Balance Col:");
                        egui::ComboBox::from_id_salt("ln_bal")
                            .selected_text(&self.loan_bal_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.loan_bal_col, c.clone(), c);
                                }
                            });
                        ui.label("CL Status Col (UC/SMA/SS/DF/BL):");
                        egui::ComboBox::from_id_salt("ln_cl")
                            .selected_text(&self.loan_cl_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.loan_cl_col, c.clone(), c);
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Optional Segment/Sector Col (detects CMSME/Agri):");
                        ui.text_edit_singleline(&mut self.loan_seg_col);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Optional Interest Suspense Col:");
                        ui.text_edit_singleline(&mut self.loan_susp_col);
                        ui.label("Optional Eligible Security Col:");
                        ui.text_edit_singleline(&mut self.loan_sec_col);
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("✅ Calculate CL Provision & BFP").clicked() {
                            step_to_apply = Some((
                                TransformStep::LoanClassificationProvision {
                                    bal_col: self.loan_bal_col.clone(),
                                    cl_status_col: self.loan_cl_col.clone(),
                                    segment_col: if self.loan_seg_col.trim().is_empty() {
                                        None
                                    } else {
                                        Some(self.loan_seg_col.trim().to_string())
                                    },
                                    suspense_col: if self.loan_susp_col.trim().is_empty() {
                                        None
                                    } else {
                                        Some(self.loan_susp_col.trim().to_string())
                                    },
                                    security_col: if self.loan_sec_col.trim().is_empty() {
                                        None
                                    } else {
                                        Some(self.loan_sec_col.trim().to_string())
                                    },
                                },
                                false,
                            ));
                            close_modal = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_modal = true;
                        }
                    });
                });
            }

            ActiveToolModal::LoanLimitUtil => {
                egui::Window::new(
                    "💳 Loans & Advances: Limit Utilization, Over-Limit Audit & Undrawn CCF (C5)",
                )
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("Computes Utilization %, Undrawn Commitment, Over-Limit Breach, and CRAR/LCR Credit Equivalent.");
                    ui.horizontal(|ui| {
                        ui.label("Sanctioned Limit Col:");
                        egui::ComboBox::from_id_salt("lm_lim")
                            .selected_text(&self.lim_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.lim_col, c.clone(), c);
                                }
                            });
                        ui.label("Outstanding Balance Col:");
                        egui::ComboBox::from_id_salt("lm_bal")
                            .selected_text(&self.lim_bal_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.lim_bal_col, c.clone(), c);
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Credit Conversion Factor (CCF % for Undrawn Portion, e.g. 20%):");
                        ui.add(egui::DragValue::new(&mut self.lim_ccf_pct).speed(1.0));
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("✅ Calculate Limit Utilization & Undrawn CCF").clicked() {
                            step_to_apply = Some((
                                TransformStep::LoanLimitUtilization {
                                    limit_col: self.lim_col.clone(),
                                    bal_col: self.lim_bal_col.clone(),
                                    ccf_pct: self.lim_ccf_pct,
                                },
                                false,
                            ));
                            close_modal = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_modal = true;
                        }
                    });
                });
            }

            ActiveToolModal::FtpCurveMap => {
                egui::Window::new("🧮 Automated FTP Yield Curve Tenor Mapper (Feature B2)")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Open Date:");
                            egui::ComboBox::from_id_salt("ftp_opn")
                                .selected_text(&self.ftp_opn_col)
                                .show_ui(ui, |ui| {
                                    for c in &avail_cols {
                                        ui.selectable_value(&mut self.ftp_opn_col, c.clone(), c);
                                    }
                                });
                            ui.label("Maturity Date:");
                            egui::ComboBox::from_id_salt("ftp_mat")
                                .selected_text(&self.ftp_mat_col)
                                .show_ui(ui, |ui| {
                                    for c in &avail_cols {
                                        ui.selectable_value(&mut self.ftp_mat_col, c.clone(), c);
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label("Balance Col:");
                            egui::ComboBox::from_id_salt("ftp_bal")
                                .selected_text(&self.ftp_bal_col)
                                .show_ui(ui, |ui| {
                                    for c in &avail_cols {
                                        ui.selectable_value(&mut self.ftp_bal_col, c.clone(), c);
                                    }
                                });
                            ui.label("Int Rate Col:");
                            egui::ComboBox::from_id_salt("ftp_rt")
                                .selected_text(&self.ftp_rate_col)
                                .show_ui(ui, |ui| {
                                    for c in &avail_cols {
                                        ui.selectable_value(&mut self.ftp_rate_col, c.clone(), c);
                                    }
                                });
                        });
                        ui.checkbox(
                            &mut self.ftp_is_liability,
                            "Liability Book (Deposits/Borrowings: Spread = FTP Rate - Account Rate)",
                        );
                        ui.separator();
                        ui.strong("Benchmark Yield Curve (Tenor Days -> FTP Rate %):");
                        for pt in &mut self.ftp_curve {
                            ui.horizontal(|ui| {
                                ui.label("Up to Days:");
                                ui.add(egui::DragValue::new(&mut pt.tenor_days));
                                ui.label("FTP %:");
                                ui.add(egui::DragValue::new(&mut pt.ftp_rate).speed(0.05));
                            });
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("✅ Map FTP Rates & Compute Margin").clicked() {
                                step_to_apply = Some((
                                    TransformStep::FtpCurveMap {
                                        opndat_col: self.ftp_opn_col.clone(),
                                        matdat_col: self.ftp_mat_col.clone(),
                                        bal_col: self.ftp_bal_col.clone(),
                                        rate_col: self.ftp_rate_col.clone(),
                                        curve: self.ftp_curve.clone(),
                                        is_liability: self.ftp_is_liability,
                                    },
                                    false,
                                ));
                                close_modal = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close_modal = true;
                            }
                        });
                    });
            }

            ActiveToolModal::BankingAudit => {
                egui::Window::new("🛡 Core Banking Dirty Data & Ghost Account Auditor (Feature B3)")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.label("Scans for Matured-but-Active Ghosts, Backwards Dates, Rate Outliers, and Negative Balances.");
                        ui.horizontal(|ui| {
                            ui.label("Open Date:");
                            ui.text_edit_singleline(&mut self.audit_opn_col);
                            ui.label("Maturity Date:");
                            ui.text_edit_singleline(&mut self.audit_mat_col);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Close Date:");
                            ui.text_edit_singleline(&mut self.audit_cls_col);
                            ui.label("Status (Acstat):");
                            ui.text_edit_singleline(&mut self.audit_stat_col);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Balance Col:");
                            ui.text_edit_singleline(&mut self.audit_bal_col);
                            ui.label("Int Rate Col:");
                            ui.text_edit_singleline(&mut self.audit_rate_col);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Reporting Cutoff Date (YYYY-MM-DD):");
                            ui.text_edit_singleline(&mut self.alm_cutoff);
                        });
                        ui.checkbox(
                            &mut self.audit_only_exceptions,
                            "Show Exception Rows Only (Filter out 'OK' accounts)",
                        );
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("🚀 Run Audit into New Exception Tab").clicked() {
                                step_to_apply = Some((
                                    TransformStep::BankingHygieneAudit {
                                        opndat_col: self.audit_opn_col.clone(),
                                        matdat_col: self.audit_mat_col.clone(),
                                        clsdat_col: self.audit_cls_col.clone(),
                                        acstat_col: self.audit_stat_col.clone(),
                                        bal_col: self.audit_bal_col.clone(),
                                        rate_col: self.audit_rate_col.clone(),
                                        actnum_col: self.audit_key_col.clone(),
                                        cutoff_date: self.alm_cutoff.clone(),
                                        only_exceptions: self.audit_only_exceptions,
                                    },
                                    true,
                                ));
                                close_modal = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close_modal = true;
                            }
                        });
                    });
            }

            ActiveToolModal::StackFiles => {
                egui::Window::new("📚 Vertical Multi-File Merger / Stacker (Req #11)")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.label("Combines multiple CSV/Parquet files with automatic linebreak sanitization & schema alignment.");
                        if ui.button("➕ Add Files to Stack...").clicked() {
                            if let Some(files) = rfd::FileDialog::new()
                                .add_filter("Data Files", &["csv", "parquet"])
                                .pick_files()
                            {
                                self.stack_files_list.extend(files);
                            }
                        }
                        for f in &self.stack_files_list {
                            ui.label(format!("  • {:?}", f.file_name().unwrap_or_default()));
                        }
                        ui.checkbox(
                            &mut self.stack_include_source,
                            "Append 'Source_File' Column to identify origin month/file",
                        );
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("🚀 Stack & Open as Unified Tab").clicked()
                                && !self.stack_files_list.is_empty()
                            {
                                let mut specs = Vec::new();
                                for p in &self.stack_files_list {
                                    let is_parq = p
                                        .extension()
                                        .and_then(|e| e.to_str())
                                        .unwrap_or("")
                                        .eq_ignore_ascii_case("parquet");
                                    let cfg = if is_parq {
                                        IngestionConfig::default()
                                    } else {
                                        auto_detect_smart_schema(p).unwrap_or_default()
                                    };
                                    specs.push((p.clone(), is_parq, cfg));
                                }
                                self.create_tab_from_source(
                                    format!("Stacked ({} files)", specs.len()),
                                    TabSource::VerticalStack {
                                        files: specs,
                                        include_source_col: self.stack_include_source,
                                    },
                                    Vec::new(),
                                );
                                close_modal = true;
                            }
                            if ui.button("Clear List").clicked() {
                                self.stack_files_list.clear();
                            }
                            if ui.button("Cancel").clicked() {
                                close_modal = true;
                            }
                        });
                    });
            }

            ActiveToolModal::JoinTabs => {
                egui::Window::new("🔗 SQL-Style Multi-Tab Joiner (Req #12)")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        if self.tabs.len() < 2 {
                            ui.label("Please open at least 2 tabs to perform a join.");
                        } else {
                            let left_title = self.tabs[self.join_left_tab.min(self.tabs.len() - 1)]
                                .title
                                .clone();
                            let right_title = self.tabs
                                [self.join_right_tab.min(self.tabs.len() - 1)]
                                .title
                                .clone();
                            let tab_titles: Vec<(usize, String)> = self
                                .tabs
                                .iter()
                                .enumerate()
                                .map(|(i, t)| (i, t.title.clone()))
                                .collect();

                            ui.horizontal(|ui| {
                                ui.label("Left Tab:");
                                egui::ComboBox::from_id_salt("jn_l")
                                    .selected_text(&left_title)
                                    .show_ui(ui, |ui| {
                                        for (i, title) in &tab_titles {
                                            ui.selectable_value(&mut self.join_left_tab, *i, title);
                                        }
                                    });
                                ui.label("Right Tab:");
                                egui::ComboBox::from_id_salt("jn_r")
                                    .selected_text(&right_title)
                                    .show_ui(ui, |ui| {
                                        for (i, title) in &tab_titles {
                                            ui.selectable_value(
                                                &mut self.join_right_tab,
                                                *i,
                                                title,
                                            );
                                        }
                                    });
                            });
                            ui.horizontal(|ui| {
                                ui.label("Join Type:");
                                egui::ComboBox::from_id_salt("jn_kind")
                                    .selected_text(self.join_kind.label())
                                    .show_ui(ui, |ui| {
                                        for k in [
                                            SqlJoinKind::Left,
                                            SqlJoinKind::Inner,
                                            SqlJoinKind::Full,
                                            SqlJoinKind::Anti,
                                            SqlJoinKind::Semi,
                                        ] {
                                            ui.selectable_value(&mut self.join_kind, k, k.label());
                                        }
                                    });
                            });
                            ui.horizontal(|ui| {
                                ui.label("Join Key Column(s) (comma-separated, e.g. Actnum):");
                                ui.text_edit_singleline(&mut self.join_keys_input);
                            });
                            ui.separator();
                            if ui.button("🚀 Execute Join into New Tab").clicked() {
                                let l_tab = self.tabs[self.join_left_tab].clone();
                                let r_tab = self.tabs[self.join_right_tab].clone();
                                let keys: Vec<String> = self
                                    .join_keys_input
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect();
                                self.create_tab_from_source(
                                    format!("Join({}+{})", l_tab.title, r_tab.title),
                                    TabSource::JoinedTabs {
                                        left_source: Box::new(l_tab.source),
                                        left_steps: l_tab.steps,
                                        right_source: Box::new(r_tab.source),
                                        right_steps: r_tab.steps,
                                        join_kind: self.join_kind,
                                        join_keys: keys,
                                        right_cols_to_keep: Vec::new(),
                                    },
                                    Vec::new(),
                                );
                                close_modal = true;
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            close_modal = true;
                        }
                    });
            }

            ActiveToolModal::ReconcilePeriods => {
                egui::Window::new(
                    "⚖ Two-Period Variance & Loan CL Migration Reconciler (Feature A6 / C2)",
                )
                .collapsible(false)
                .show(ctx, |ui| {
                    if self.tabs.len() < 2 {
                        ui.label(
                            "Open Previous Period (e.g. Sep) and Current Period (e.g. Oct) in 2 tabs first.",
                        );
                    } else {
                        let prev_title = self.tabs[self.rec_prev_tab.min(self.tabs.len() - 1)]
                            .title
                            .clone();
                        let curr_title = self.tabs[self.rec_curr_tab.min(self.tabs.len() - 1)]
                            .title
                            .clone();
                        let tab_titles: Vec<(usize, String)> = self
                            .tabs
                            .iter()
                            .enumerate()
                            .map(|(i, t)| (i, t.title.clone()))
                            .collect();

                        ui.horizontal(|ui| {
                            ui.label("Previous Period Tab:");
                            egui::ComboBox::from_id_salt("rec_p")
                                .selected_text(&prev_title)
                                .show_ui(ui, |ui| {
                                    for (i, title) in &tab_titles {
                                        ui.selectable_value(&mut self.rec_prev_tab, *i, title);
                                    }
                                });
                            ui.label("Current Period Tab:");
                            egui::ComboBox::from_id_salt("rec_c")
                                .selected_text(&curr_title)
                                .show_ui(ui, |ui| {
                                    for (i, title) in &tab_titles {
                                        ui.selectable_value(&mut self.rec_curr_tab, *i, title);
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label("Primary Key (e.g. Actnum):");
                            ui.text_edit_singleline(&mut self.rec_key_col);
                            ui.label("Balance Col:");
                            ui.text_edit_singleline(&mut self.rec_bal_col);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Optional Rate Col (Int Rate):");
                            ui.text_edit_singleline(&mut self.rec_rate_col);
                            ui.label("Optional Loan CL Col (CL_Status):");
                            ui.text_edit_singleline(&mut self.rec_cl_col);
                        });
                        ui.separator();
                        if ui.button("🚀 Reconcile Periods into New Tab").clicked() {
                            let p_tab = self.tabs[self.rec_prev_tab].clone();
                            let c_tab = self.tabs[self.rec_curr_tab].clone();
                            let r_col = if self.rec_rate_col.trim().is_empty() {
                                None
                            } else {
                                Some(self.rec_rate_col.trim().to_string())
                            };
                            let cl_col = if self.rec_cl_col.trim().is_empty() {
                                None
                            } else {
                                Some(self.rec_cl_col.trim().to_string())
                            };
                            self.create_tab_from_source(
                                format!("Reconcile({} vs {})", p_tab.title, c_tab.title),
                                TabSource::PeriodReconcile {
                                    prev_source: Box::new(p_tab.source),
                                    prev_steps: p_tab.steps,
                                    curr_source: Box::new(c_tab.source),
                                    curr_steps: c_tab.steps,
                                    key_col: self.rec_key_col.trim().to_string(),
                                    bal_col: self.rec_bal_col.trim().to_string(),
                                    rate_col: r_col,
                                    cl_status_col: cl_col,
                                },
                                Vec::new(),
                            );
                            close_modal = true;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close_modal = true;
                    }
                });
            }

            ActiveToolModal::SaveMacro => {
                egui::Window::new("💾 Save Portable Macro Recipe (.mpq.json) (Req #8, #9)")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.label("Saves all Applied Steps into ./portable_data/macros/ so you can replay them on any PC.");
                        ui.horizontal(|ui| {
                            ui.label("Macro Name:");
                            ui.text_edit_singleline(&mut self.macro_name);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Description:");
                            ui.text_edit_singleline(&mut self.macro_desc);
                        });
                        ui.checkbox(
                            &mut self.macro_enforce_ctrl,
                            "Enforce Control Column Reconciliation Check during replay (Feature A3)",
                        );
                        ui.checkbox(
                            &mut self.macro_include_schema,
                            "Embed CSV Ingestion Schema Rules inside Macro",
                        );
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("💾 Save .mpq.json File...").clicked() {
                                if let Some(tab) = self.tabs.get(self.active_tab) {
                                    let opt_schema = if self.macro_include_schema {
                                        match &tab.source {
                                            TabSource::SingleFile { config, .. } => {
                                                Some(config.clone())
                                            }
                                            _ => None,
                                        }
                                    } else {
                                        None
                                    };
                                    let recipe = MacroRecipe {
                                        name: self.macro_name.clone(),
                                        description: self.macro_desc.clone(),
                                        created_at: chrono::Local::now()
                                            .format("%Y-%m-%d %H:%M")
                                            .to_string(),
                                        enforce_balance_control: self.macro_enforce_ctrl,
                                        control_column: tab.control_col.clone(),
                                        optional_ingestion_schema: opt_schema,
                                        steps: tab.steps.clone(),
                                    };
                                    if let Some(save_path) = rfd::FileDialog::new()
                                        .set_directory(&self.paths.macros_dir)
                                        .set_file_name(format!("{}.mpq.json", self.macro_name))
                                        .save_file()
                                    {
                                        match save_macro_to_file(&recipe, &save_path) {
                                            Ok(_) => {
                                                self.status_message =
                                                    format!("Saved macro to {:?}", save_path);
                                            }
                                            Err(e) => {
                                                self.status_message =
                                                    format!("Failed to save macro: {}", e);
                                            }
                                        }
                                    }
                                }
                                close_modal = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close_modal = true;
                            }
                        });
                    });
            }

            ActiveToolModal::PythonCodePreview => {
                egui::Window::new(
                    "🐍 Export GUI Pipeline as Python Polars Jupyter Code (Feature A8)",
                )
                .collapsible(false)
                .default_width(760.0)
                .show(ctx, |ui| {
                    ui.label("Copy and paste this script directly into your Jupyter Notebook:");
                    egui::ScrollArea::vertical()
                        .max_height(360.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.generated_python_code)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(18),
                            );
                        });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("📋 Copy Python Code to Clipboard").clicked() {
                            ctx.copy_text(self.generated_python_code.clone());
                            self.status_message =
                                "Copied Python Polars code to clipboard!".to_string();
                        }
                        if ui.button("💾 Save as .py Script...").clicked() {
                            if let Some(p) = rfd::FileDialog::new()
                                .set_file_name("micro_parquet_pipeline.py")
                                .save_file()
                            {
                                let _ = std::fs::write(&p, &self.generated_python_code);
                                self.status_message = format!("Saved Python script to {:?}", p);
                            }
                        }
                        if ui.button("Close").clicked() {
                            close_modal = true;
                        }
                    });
                });
            }

            ActiveToolModal::MultiTabExport => {
                egui::Window::new(
                    "📊 Consolidated Multi-Tab Exporter (Excel Workbook .xlsx / Landscape PDF .pdf)",
                )
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.strong("Select Open Tabs / Views to Include in Consolidated File:");
                    if self.multi_export_checks.len() != self.tabs.len() {
                        self.multi_export_checks = vec![true; self.tabs.len()];
                    }
                    for (i, tab) in self.tabs.iter().enumerate() {
                        let ref mut is_checked = self.multi_export_checks[i];
                        ui.checkbox(
                            is_checked,
                            format!("Tab {}: {} ({} rows)", i + 1, tab.title, tab.total_rows),
                        );
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .button("📊 Export Selected Tabs to Single .xlsx Workbook...")
                            .clicked()
                        {
                            if let Some(out_path) = rfd::FileDialog::new()
                                .set_file_name("Consolidated_ALCO_Views.xlsx")
                                .save_file()
                            {
                                let mut collected_tabs = Vec::new();
                                for (i, tab) in self.tabs.iter().enumerate() {
                                    if self.multi_export_checks[i] {
                                        if let Ok(lf) = compile_tab_lazy(
                                            &tab.source,
                                            &tab.steps,
                                            &self.paths.temp_dir,
                                        ) {
                                            if let Ok(df) = lf.limit(1_048_500).collect() {
                                                collected_tabs.push((tab.title.clone(), df));
                                            }
                                        }
                                    }
                                }
                                match export_tabs_to_xlsx(collected_tabs, &out_path) {
                                    Ok(_) => {
                                        self.status_message = format!(
                                            "Exported multi-tab Excel workbook to {:?}",
                                            out_path
                                        );
                                    }
                                    Err(e) => {
                                        self.status_message = format!("Excel export error: {}", e);
                                    }
                                }
                            }
                            close_modal = true;
                        }

                        if ui
                            .button("📄 Export Selected Tabs to Single Landscape .pdf...")
                            .clicked()
                        {
                            if let Some(out_path) = rfd::FileDialog::new()
                                .set_file_name("Consolidated_ALCO_Report.pdf")
                                .save_file()
                            {
                                let mut collected_tabs = Vec::new();
                                for (i, tab) in self.tabs.iter().enumerate() {
                                    if self.multi_export_checks[i] {
                                        if let Ok(lf) = compile_tab_lazy(
                                            &tab.source,
                                            &tab.steps,
                                            &self.paths.temp_dir,
                                        ) {
                                            if let Ok(df) = lf.limit(5_000).collect() {
                                                collected_tabs.push((tab.title.clone(), df));
                                            }
                                        }
                                    }
                                }
                                match export_tabs_to_pdf(collected_tabs, &out_path, self.num_format)
                                {
                                    Ok(_) => {
                                        self.status_message = format!(
                                            "Exported multi-tab PDF report to {:?}",
                                            out_path
                                        );
                                    }
                                    Err(e) => {
                                        self.status_message = format!("PDF export error: {}", e);
                                    }
                                }
                            }
                            close_modal = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_modal = true;
                        }
                    });
                });
            }

            ActiveToolModal::SplitByColumn => {
                egui::Window::new("🗂 Smart Partitioned Split Exporter (Feature B6)")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.label("Splits the active tab into separate files for each unique value in a column (e.g. Brancd, Glcode, Bank).");
                        ui.horizontal(|ui| {
                            ui.label("Partition / Split Column:");
                            egui::ComboBox::from_id_salt("splt_col")
                                .selected_text(&self.split_col_name)
                                .show_ui(ui, |ui| {
                                    for c in &avail_cols {
                                        ui.selectable_value(&mut self.split_col_name, c.clone(), c);
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
                                ui.selectable_value(&mut self.split_format, fmt, fmt.label());
                            }
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("🚀 Select Output Folder & Split Now...").clicked() {
                                if let (Some(tab), Some(out_dir)) = (
                                    self.tabs.get(self.active_tab),
                                    rfd::FileDialog::new().pick_folder(),
                                ) {
                                    if let Ok(lf) = compile_tab_lazy(
                                        &tab.source,
                                        &tab.steps,
                                        &self.paths.temp_dir,
                                    ) {
                                        match split_and_export_by_column(
                                            lf,
                                            &self.split_col_name,
                                            &out_dir,
                                            self.split_format,
                                            self.parquet_dialect,
                                            self.export_csv_mdy,
                                        ) {
                                            Ok(cnt) => {
                                                self.status_message = format!(
                                                    "Split & exported {} individual files into {:?}",
                                                    cnt, out_dir
                                                );
                                            }
                                            Err(e) => {
                                                self.status_message =
                                                    format!("Split export failed: {}", e);
                                            }
                                        }
                                    }
                                }
                                close_modal = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close_modal = true;
                            }
                        });
                    });
            }

            ActiveToolModal::None => {}
        }

        if let Some((step, new_tab)) = step_to_apply {
            self.push_step_to_active_tab(step, new_tab);
        }
        if close_modal {
            self.active_tool = ActiveToolModal::None;
        }
    }

    fn render_csv_schema_modal(&mut self, ctx: &egui::Context) {
        let Some(mut modal) = self.pending_csv_modal.take() else {
            return;
        };

        let mut keep_open = true;
        let mut action_open_tab = false;
        let mut action_direct_convert = false;

        egui::Window::new("🔍 CSV 5-Row Raw Preview, Linebreak Sanitizer & Strict Schema Lock")
            .collapsible(false)
            .resizable(true)
            .default_width(1050.0)
            .show(ctx, |ui| {
                ui.label(format!("File: {:?}", modal.csv_path));
                ui.separator();

                ui.horizontal(|ui| {
                    ui.checkbox(
                        &mut modal.config.sanitize_linebreaks,
                        "Sanitize Linebreaks (\\r, \\n) & Trim Whitespace (Default ON)",
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

                ui.separator();
                ui.strong("1. Raw 5-Row Text Preview:");
                egui::ScrollArea::vertical()
                    .id_salt("raw_preview_scroll")
                    .max_height(140.0)
                    .show(ui, |ui| {
                        Self::render_data_grid(
                            ui,
                            &modal.raw_preview_df,
                            0,
                            NumberDisplayFormat::Raw,
                        );
                    });

                ui.separator();
                ui.strong("2. Set Specific Data Type & Optional PII Masking per Column:");

                egui::ScrollArea::vertical()
                    .id_salt("schema_override_scroll")
                    .max_height(250.0)
                    .show(ui, |ui| {
                        egui::Grid::new("schema_grid")
                            .striped(true)
                            .min_col_width(150.0)
                            .show(ui, |ui| {
                                ui.strong("Column Name");
                                ui.strong("Target Data Type");
                                ui.strong("PII Anonymizer");
                                ui.strong("Sample Values");
                                ui.end_row();

                                for (i, col_cfg) in modal.config.columns.iter_mut().enumerate() {
                                    ui.label(&col_cfg.name);
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
                    if !modal.direct_convert_only
                        && ui.button("✅ Lock Schema & Open in Viewer Tab").clicked()
                    {
                        action_open_tab = true;
                        keep_open = false;
                    }
                    if ui
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

        if action_open_tab {
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
                if let Ok(lf) = compile_tab_lazy(&src, &[], &self.paths.temp_dir) {
                    let _ = export_to_parquet_stream(lf, &out_path, self.parquet_dialect);
                    self.status_message = format!("Converted directly to {:?}", out_path);
                }
            }
        } else if keep_open {
            self.pending_csv_modal = Some(modal);
        }
    }

    fn render_batch_modal(&mut self, ctx: &egui::Context) {
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
        egui::Window::new("🔄 Recursive Folder-Tree Mirror Batch Converter")
            .collapsible(false)
            .default_width(700.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("📂 1. Input Root Folder...").clicked() {
                        self.batch_modal.input_folder = rfd::FileDialog::new().pick_folder();
                    }
                    ui.label(format!("{:?}", self.batch_modal.input_folder));
                });
                ui.horizontal(|ui| {
                    if ui.button("📂 2. Output Root Folder...").clicked() {
                        self.batch_modal.output_folder = rfd::FileDialog::new().pick_folder();
                    }
                    ui.label(format!("{:?}", self.batch_modal.output_folder));
                });

                let mut d = dir;
                ui.horizontal(|ui| {
                    ui.radio_value(&mut d, BatchDirection::CsvToParquet, "CSV -> Parquet");
                    ui.radio_value(&mut d, BatchDirection::ParquetToCsv, "Parquet -> CSV");
                });
                self.batch_modal.direction = Some(d);

                let mut s = strat;
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
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("🚀 Start Mirror Batch").clicked() {
                        if let (Some(in_dir), Some(out_dir)) = (
                            self.batch_modal.input_folder.clone(),
                            self.batch_modal.output_folder.clone(),
                        ) {
                            if let Ok(rep) = run_folder_mirror_batch(
                                &in_dir,
                                &out_dir,
                                d,
                                s,
                                self.batch_modal.loaded_template.as_ref(),
                                self.parquet_dialect,
                                self.batch_modal.mdy_dates_on_csv,
                                &self.paths.temp_dir,
                            ) {
                                self.batch_modal.last_report = Some(rep);
                            }
                        }
                    }
                    if ui.button("Close").clicked() {
                        close_clicked = true;
                    }
                });

                if let Some(rep) = &self.batch_modal.last_report {
                    ui.separator();
                    ui.label(format!(
                        "Converted: {} | Companion Copied: {} | Size: {:.1} MB -> {:.1} MB",
                        rep.files_converted,
                        rep.companion_files_copied,
                        rep.bytes_before as f64 / 1_048_576.0,
                        rep.bytes_after as f64 / 1_048_576.0
                    ));
                }
            });

        if close_clicked {
            self.batch_modal.open = false;
        }
    }

    fn export_active_tab_parquet(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let Some(out_path) = rfd::FileDialog::new()
            .set_file_name(format!("{}.parquet", tab.title))
            .save_file()
        else {
            return;
        };

        match compile_tab_lazy(&tab.source, &tab.steps, &self.paths.temp_dir)
            .and_then(|lf| export_to_parquet_stream(lf, &out_path, self.parquet_dialect))
        {
            Ok(_) => self.status_message = format!("Exported view to {:?}", out_path),
            Err(e) => self.status_message = format!("Parquet export failed: {}", e),
        }
    }

    fn export_active_tab_csv(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let Some(out_path) = rfd::FileDialog::new()
            .set_file_name(format!("{}.csv", tab.title))
            .save_file()
        else {
            return;
        };

        match compile_tab_lazy(&tab.source, &tab.steps, &self.paths.temp_dir)
            .and_then(|lf| export_to_csv_stream(lf, &out_path, self.export_csv_mdy))
        {
            Ok(_) => self.status_message = format!("Exported view to {:?}", out_path),
            Err(e) => self.status_message = format!("CSV export failed: {}", e),
        }
    }
}
