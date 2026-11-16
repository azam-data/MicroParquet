#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use micro_parquet::batch::{run_folder_mirror_batch, BatchReport};
use clap::Parser;
use eframe::egui;
use egui_extras::{Column, TableBuilder};
use egui_plot::{Bar, BarChart, Plot};
use micro_parquet::engine::{
    auto_detect_smart_schema, compile_tab_lazy, compute_control_totals, export_to_csv_stream,
    adapt_schema_to_parquet, auto_detect_parquet_schema, auto_detect_smart_schema_with, export_to_parquet_stream,
    blank_balance_notes, collect_masked_columns, control_totals_state, source_masks_pii,
    source_row_check, detect_file_schema, ensure_pii_key_for, export_pii_key, import_pii_key,
    pii_key_fingerprint, pii_key_path, read_pii_key, fetch_viewport_slice, join_status, preview_parquet_raw, stack_file_configs,
    parse_failure_summary, preview_csv_raw, preview_csv_with,
    source_parse_failures, write_parse_failures_csv,
};
use micro_parquet::exporters::{
    export_tabs_to_pdf, export_tabs_to_xlsx, format_df_for_clipboard, split_and_export_by_column,
    PDF_MAX_ROWS_PER_TAB,
};
use micro_parquet::macros::{
    generate_python_polars_code, load_macro_from_file, python_pii_refusal, python_untranslated_steps,
    save_macro_to_file,
};
use micro_parquet::models::{
    column_name_tokens, default_active_statuses, default_bd_ftp_curve, default_rate_max_pct, parquet_native_type, format_metric_value, format_with_commas, AggFunc, AggSpec,
    BatchDirection, BatchSchemaStrategy, BucketRuleItem, ColDataType, ColumnFormatSpec, CondRule,
    CsvTextEncoding,
    CustomBucketDef, FilterOp, FtpInterpolation, FtpPoint, IngestionConfig, MacroRecipe, MathOp, MetricFormatType,
    NumberDisplayFormat, ParquetDialect, ParseFailure, PiiMaskMode, PortablePaths, SignConvention,
    SplitExportFormat, SqlJoinKind, TabSource, TabState, TransformStep,
};
use polars::datatypes::AnyValue;
use polars::prelude::DataFrame;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
        if let Err(e) = run_headless_cli(&args, &paths) {
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
            MicroParquetApp::apply_ui_theme(&cc.egui_ctx, false, 1.05);
            let mut app = MicroParquetApp::new(paths);
            if let Some(n) = rule_notice {
                app.status_message = n;
            }
            Ok(Box::new(app))
        }),
    )
}

/// Where a stored tab index points after the tab at `closed` is removed, leaving `new_len` tabs.
/// Indices after the closed tab shift down so they keep pointing at the same tab. An index at
/// the closed tab moves to the tab that took its place (or the new last tab). Any result is
/// clamped to 0..new_len, and is 0 when no tabs are left.
fn tab_index_after_close(sel: usize, closed: usize, new_len: usize) -> usize {
    let shifted = if sel > closed { sel - 1 } else { sel };
    shifted.min(new_len.saturating_sub(1))
}

/// Default for the asset/liability side picker: the first column with a "side" or "direction"
/// name token. "type" is not used: Account_Type or Collateral_Type are not sides.
fn default_side_column(cols: &[String]) -> String {
    cols.iter()
        .find(|c| {
            column_name_tokens(c)
                .iter()
                .any(|t| t == "side" || t == "direction")
        })
        .cloned()
        .unwrap_or_default()
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

    let (cfg, steps, recipe_control) = if let Some(m_path) = &args.macro_file {
        let recipe = load_macro_from_file(m_path)?;
        let control = recipe.control_column.clone();
        let ing = if is_parquet {
            match recipe.optional_ingestion_schema {
                // Applies to columns that exist and are String in the file; the others keep
                // their Parquet type.
                Some(saved_ing) => {
                    let (fitted, missing) = adapt_schema_to_parquet(input_path, &saved_ing)?;
                    if !missing.is_empty() {
                        eprintln!(
                            "[MicroParquet] Warning: macro schema columns not in the Parquet file, skipped: {}",
                            missing.join(", ")
                        );
                    }
                    fitted
                }
                None => IngestionConfig::default(),
            }
        } else if let Some(saved_ing) = recipe.optional_ingestion_schema {
            saved_ing
        } else {
            auto_detect_smart_schema(input_path)?
        };
        (ing, recipe.steps, control)
    } else {
        let ing = if is_parquet {
            IngestionConfig::default()
        } else {
            auto_detect_smart_schema(input_path)?
        };
        (ing, Vec::new(), None)
    };

    let source = TabSource::SingleFile {
        path: input_path.clone(),
        is_parquet,
        config: cfg,
    };

    if let Some(note) = ensure_pii_key_for(&source)? {
        eprintln!("[MicroParquet] {}", note);
    }
    let lf = compile_tab_lazy(&source, &steps, &paths.temp_dir)?;
    if let Some(w) = source_row_check(&source, &paths.temp_dir)?.and_then(|c| c.warning()) {
        eprintln!("[MicroParquet] WARNING: {}", w);
    }
    for note in blank_balance_notes(&source, &steps, &paths.temp_dir)? {
        eprintln!("[MicroParquet] {}", note);
    }

    // Values that could not be converted: always reported, and they stop the run when the
    // control column has any (unless --allow-parse-failures).
    let failures = source_parse_failures(&source, &paths.temp_dir)?;
    for f in &failures {
        eprintln!(
            "[MicroParquet] Parse failures: {} {} (examples: {})",
            f.column,
            f.count,
            f.examples.join(" | ")
        );
    }
    let control = match recipe_control {
        Some(c) => Some(c),
        None => MicroParquetApp::guess_balance_column(&lf.clone().limit(0).collect()?),
    };
    if let Some(ctrl) = &control {
        if let Some(f) = failures.iter().find(|f| &f.column == ctrl) {
            if !args.allow_parse_failures {
                anyhow::bail!(
                    "Control column '{}' has {} value(s) that could not be converted (e.g. {}). \
                     Fix the file or its schema, or pass --allow-parse-failures to export anyway.",
                    ctrl,
                    f.count,
                    f.examples.join(" | ")
                );
            }
        }
    }

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
        let rep = export_tabs_to_xlsx(vec![("Headless_Output".to_string(), df)], Vec::new(), out_xlsx)?;
        println!("Exported Excel: {:?} ({})", out_xlsx, rep.summary());
    }
    if let Some(out_pdf) = &args.export_pdf {
        let df = lf.collect()?;
        let rep = export_tabs_to_pdf(
            vec![("Headless_Report".to_string(), df, HashMap::new())],
            out_pdf,
            NumberDisplayFormat::BdtCrore,
            2,
        )?;
        println!("Exported PDF: {:?}", out_pdf);
        let s = rep.summary();
        if !s.is_empty() {
            eprintln!("WARNING (PDF): {}", s);
        }
    }

    Ok(())
}

struct PendingCsvModal {
    csv_path: PathBuf,
    raw_preview_df: DataFrame,
    config: IngestionConfig,
    direct_convert_only: bool,
    /// Set when the modal edits the schema of an open tab instead of opening a new one.
    target_tab: Option<usize>,
    /// Parse failures of that tab's last load (empty for a new file).
    parse_failures: Vec<ParseFailure>,
    /// The file is Parquet: CSV reading options are hidden and file types are shown.
    is_parquet: bool,
}

/// Exports that stop for confirmation when the control column has parse failures.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GuardedExport {
    Parquet,
    Csv,
    MultiXlsx,
    MultiPdf,
    Split,
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
    /// Error of the last run as a whole (bad folders, missing template), or of loading the template.
    last_error: Option<String>,
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
    global_decimals: usize,
    parquet_dialect: ParquetDialect,
    export_csv_mdy: bool,
    show_steps_sidebar: bool,
    show_sql_bar: bool,
    show_chart_drawer: bool,
    dark_mode: bool,
    ui_zoom: f32,
    pending_csv_modal: Option<PendingCsvModal>,
    /// Export waiting for "Export anyway" / "Cancel" because of parse failures.
    export_guard: Option<GuardedExport>,
    /// The user chose "Export anyway" for this export.
    export_override: Option<GuardedExport>,
    batch_modal: BatchModalState,
    active_tool: ActiveToolModal,
    status_message: String,

    filter_col: String,
    filter_op: FilterOp,
    filter_val: String,
    filter_keep: bool,

    col_mgr_list: Vec<(String, bool, ColumnFormatSpec)>,
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
    pivot_new_fmt: ColumnFormatSpec,
    pivot_top_n: usize,
    pivot_open_in_new_tab: bool,

    stack_files_list: Vec<PathBuf>,
    stack_include_source: bool,
    /// 0 = each file's own schema, 1 = first file's schema for all, 2 = a saved .schema.json for all.
    stack_schema_mode: u8,
    stack_schema_path: Option<PathBuf>,

    join_left_tab: usize,
    join_right_tab: usize,
    join_kind: SqlJoinKind,
    join_keys_input: String,
    join_keep_first: bool,

    rec_prev_tab: usize,
    rec_curr_tab: usize,
    rec_key_col: String,
    rec_bal_col: String,
    rec_rate_col: String,
    rec_cl_col: String,

    // Maturity Bucketer & Custom Preset Creator (v1.2)
    alm_mat_col: String,
    alm_cutoff: String,
    alm_presets: Vec<CustomBucketDef>,
    alm_selected_preset_idx: usize,
    alm_null_sight: bool,
    alm_new_col: String,
    alm_show_custom_creator: bool,
    custom_bucket_draft: CustomBucketDef,

    // Asset/liability side column shared by the ΔNII and ΔEVE modals ("" = none)
    alm_side_col: String,
    alm_side_cols_seen: Vec<String>,

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
    ftp_interpolation: FtpInterpolation,

    audit_opn_col: String,
    audit_mat_col: String,
    audit_cls_col: String,
    audit_stat_col: String,
    audit_bal_col: String,
    audit_rate_col: String,
    audit_key_col: String,
    audit_only_exceptions: bool,
    /// Comma-separated status codes that mean active (hygiene audit).
    audit_active_statuses: String,
    audit_rate_min: f64,
    audit_rate_max: f64,

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
    loan_coll_type_col: String,
    loan_cutoff: String,

    lim_col: String,
    lim_bal_col: String,
    lim_ccf_pct: f64,

    macro_name: String,
    macro_desc: String,
    macro_enforce_ctrl: bool,
    macro_include_schema: bool,
    generated_python_code: String,
    python_export_warning: String,

    multi_export_checks: Vec<bool>,
    split_col_name: String,
    split_format: SplitExportFormat,
    chart_x_col: String,
    chart_y_col: String,
}

impl MicroParquetApp {
    pub fn apply_ui_theme(ctx: &egui::Context, dark_mode: bool, zoom: f32) {
        ctx.set_zoom_factor(zoom);

        let mut style = (*ctx.style()).clone();
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(13.5, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(13.5, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::new(13.0, egui::FontFamily::Monospace),
        );
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(17.5, egui::FontFamily::Proportional),
        );
        style.spacing.item_spacing = egui::vec2(7.0, 5.0);
        style.spacing.button_padding = egui::vec2(7.0, 3.5);
        ctx.set_style(style);

        let mut visuals = if dark_mode {
            let mut v = egui::Visuals::dark();
            v.override_text_color = Some(egui::Color32::from_rgb(240, 244, 248));
            v.panel_fill = egui::Color32::from_rgb(22, 27, 34);
            v.faint_bg_color = egui::Color32::from_rgb(33, 38, 45);
            v
        } else {
            let mut v = egui::Visuals::light();
            v.override_text_color = Some(egui::Color32::from_rgb(15, 23, 42));
            v.panel_fill = egui::Color32::from_rgb(248, 250, 252);
            v.window_fill = egui::Color32::from_rgb(255, 255, 255);
            v.faint_bg_color = egui::Color32::from_rgb(236, 242, 249);
            v.extreme_bg_color = egui::Color32::from_rgb(255, 255, 255);
            v.selection.bg_fill = egui::Color32::from_rgb(2, 132, 199);
            v.selection.stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
            v.widgets.noninteractive.bg_stroke =
                egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(203, 213, 225));
            v
        };

        visuals.striped = true;
        ctx.set_visuals(visuals);
    }

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
            export_guard: None,
            export_override: None,
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
            pivot_new_fmt: ColumnFormatSpec {
                fmt_type: MetricFormatType::Auto,
                decimals: 2,
            },
            pivot_top_n: 0,
            pivot_open_in_new_tab: true,

            stack_files_list: Vec::new(),
            stack_include_source: true,
            stack_schema_mode: 0,
            stack_schema_path: None,

            join_left_tab: 0,
            join_right_tab: 0,
            join_kind: SqlJoinKind::Left,
            join_keys_input: "Actnum".to_string(),
            join_keep_first: false,

            rec_prev_tab: 0,
            rec_curr_tab: 0,
            rec_key_col: "Actnum".to_string(),
            rec_bal_col: "Balance".to_string(),
            rec_rate_col: "Int Rate".to_string(),
            rec_cl_col: String::new(),

            alm_mat_col: "Matdat".to_string(),
            alm_cutoff: "2026-10-31".to_string(),
            alm_presets: presets,
            alm_selected_preset_idx: 0,
            alm_null_sight: true,
            alm_new_col: "Maturity_Bucket".to_string(),
            alm_show_custom_creator: false,
            custom_bucket_draft: CustomBucketDef {
                name: "c-m-b-1".to_string(),
                rules: vec![
                    BucketRuleItem { max_days: 7, label: "01. 1-7 Days".to_string() },
                    BucketRuleItem { max_days: 30, label: "02. 8-30 Days".to_string() },
                    BucketRuleItem { max_days: 90, label: "03. 31-90 Days".to_string() },
                    BucketRuleItem { max_days: 365, label: "04. 91-365 Days".to_string() },
                ],
                overflow_label: "05. Over 1 Year".to_string(),
                overdue_label: "06. Overdue / Matured".to_string(),
                null_sight_label: "01. Sight / Open".to_string(),
                null_long_label: "05. Non-Maturing Core".to_string(),
            },

            alm_side_col: String::new(),
            alm_side_cols_seen: Vec::new(),
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
            ftp_interpolation: FtpInterpolation::Linear,

            audit_opn_col: "Opndat".to_string(),
            audit_mat_col: "Matdat".to_string(),
            audit_cls_col: "Clsdat".to_string(),
            audit_stat_col: "Acstat".to_string(),
            audit_bal_col: "Balance".to_string(),
            audit_rate_col: "Int Rate".to_string(),
            audit_key_col: "Actnum".to_string(),
            audit_only_exceptions: true,
            audit_active_statuses: default_active_statuses().join(", "),
            audit_rate_min: 0.0,
            audit_rate_max: default_rate_max_pct(),

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
            loan_coll_type_col: String::new(),
            loan_cutoff: String::new(),

            lim_col: "Sanctioned_Limit".to_string(),
            lim_bal_col: "Balance".to_string(),
            lim_ccf_pct: 20.0,

            macro_name: "Monthly_WB_ALCO_Pipeline".to_string(),
            macro_desc: "Sanitizes dump, buckets maturities, and computes WAVG/Duration."
                .to_string(),
            macro_enforce_ctrl: true,
            macro_include_schema: true,
            generated_python_code: String::new(),
            python_export_warning: String::new(),

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
                HashMap::new(),
            );
        } else {
            match (preview_csv_raw(&path, 5), auto_detect_smart_schema(&path)) {
                (Ok(preview_df), Ok(config)) => {
                    self.pending_csv_modal = Some(PendingCsvModal {
                        csv_path: path,
                        raw_preview_df: preview_df,
                        config,
                        direct_convert_only,
                        target_tab: None,
                        parse_failures: Vec::new(),
                        is_parquet: false,
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
        col_formats: HashMap<String, ColumnFormatSpec>,
    ) {
        let key_note = match ensure_pii_key_for(&source) {
            Ok(n) => n,
            Err(e) => {
                self.status_message = format!("Pipeline error: {:#}", e);
                return;
            }
        };
        match compile_tab_lazy(&source, &steps, &self.paths.temp_dir) {
            Ok(lf) => {
                let slice = fetch_viewport_slice(lf.clone(), 0, 100).unwrap_or_default();
                let default_ctrl = Self::guess_balance_column(&slice);
                let totals = control_totals_state(Ok(lf), default_ctrl.as_deref());
                let (total_rows, control_sum) = (totals.rows.unwrap_or(0), totals.sum);

                // Baseline control sum is always computed from the raw source (0 steps)
                let baseline_sum = if steps.is_empty() {
                    control_sum
                } else {
                    compile_tab_lazy(&source, &[], &self.paths.temp_dir)
                        .ok()
                        .and_then(|base_lf| {
                            compute_control_totals(base_lf, default_ctrl.as_deref()).ok()
                        })
                        .and_then(|(_, s)| s)
                };

                let (parse_failures, mut count_note) =
                    match source_parse_failures(&source, &self.paths.temp_dir) {
                        Ok(f) => (f, String::new()),
                        Err(e) => (
                            Vec::new(),
                            format!(" ⚠ Could not count parse failures: {:#}", e),
                        ),
                    };
                let row_check = match source_row_check(&source, &self.paths.temp_dir) {
                    Ok(c) => c,
                    Err(e) => {
                        count_note.push_str(&format!(" ⚠ Could not compare rows with the file's lines: {:#}", e));
                        None
                    }
                };
                if let Some(w) = row_check.as_ref().and_then(|c| c.warning()) {
                    count_note.push_str(&format!(" ⚠ {}", w));
                }
                let join_stat = match join_status(
                    &source,
                    &steps,
                    default_ctrl.as_deref(),
                    &self.paths.temp_dir,
                ) {
                    Ok(j) => j,
                    Err(e) => {
                        count_note.push_str(&format!(" ⚠ Could not compare the join with its left table: {:#}", e));
                        None
                    }
                };
                self.tabs.push(TabState {
                    title,
                    source,
                    steps,
                    col_formats,
                    total_rows,
                    control_col: default_ctrl,
                    control_sum,
                    baseline_control_sum: baseline_sum,
                    viewport_offset: 0,
                    viewport_limit: 100,
                    cached_slice: slice,
                    sql_scratchpad: "SELECT * FROM self LIMIT 100".to_string(),
                    parse_failures,
                    join_status: join_stat,
                    totals,
                    row_check,
                });
                self.active_tab = self.tabs.len() - 1;
                self.init_tool_defaults_from_active_tab();
                self.status_message =
                    format!("Tab created and evaluated out-of-core.{}", count_note);
                if let Some(n) = key_note {
                    self.status_message.push_str(&format!(" 🔑 {}", n));
                }
                self.append_blank_balance_notes();
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
        self.col_mgr_list = cols
            .iter()
            .map(|c| {
                let spec = tab
                    .col_formats
                    .get(c)
                    .copied()
                    .unwrap_or(ColumnFormatSpec {
                        fmt_type: MetricFormatType::Auto,
                        decimals: self.global_decimals,
                    });
                (c.clone(), true, spec)
            })
            .collect();
        self.pivot_group_cols = cols.iter().map(|c| (c.clone(), false)).collect();
        self.multi_export_checks = vec![true; self.tabs.len()];
    }

    fn push_step_to_active_tab(&mut self, step: TransformStep, open_new_tab: bool) {
        let Some(curr) = self.tabs.get(self.active_tab).cloned() else {
            return;
        };

        // Inherit per-metric format specifications if this is a Pivot step
        let mut updated_formats = curr.col_formats.clone();
        if let TransformStep::GroupByPivot { ref aggs, .. } = step {
            for a in aggs {
                updated_formats.insert(a.alias.clone(), a.format_spec);
            }
        }

        let step_label = step.short_label();
        if open_new_tab {
            let mut new_steps = curr.steps.clone();
            let title = format!("{} - {}", curr.title, step.short_label());
            new_steps.push(step);
            self.create_tab_from_source(title, curr.source, new_steps, updated_formats);
        } else {
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                tab.steps.push(step);
                tab.col_formats = updated_formats;
                tab.viewport_offset = 0;
            }
            // A failed refresh replaces this message with the error.
            let added = format!("Step added: {}.", step_label);
            self.status_message = added.clone();
            self.refresh_active_tab(false);
            if self.status_message == added {
                self.append_blank_balance_notes();
            }
        }
    }

    fn guess_balance_column(df: &DataFrame) -> Option<String> {
        // Prefer exact "balance" column before partial matches
        for col in df.get_columns() {
            if col.name().eq_ignore_ascii_case("balance") {
                return Some(col.name().to_string());
            }
        }
        for col in df.get_columns() {
            let lower = col.name().to_lowercase();
            if lower.contains("balance") || lower.contains("amount") || lower.contains("delta") {
                return Some(col.name().to_string());
            }
        }
        None
    }

    /// Adds "Balance blank on N rows; treated as 0" for the active tab's banking steps.
    fn append_blank_balance_notes(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        match blank_balance_notes(&tab.source, &tab.steps, &self.paths.temp_dir) {
            Ok(notes) if !notes.is_empty() => {
                self.status_message = format!("{} ⚠ {}", self.status_message, notes.join("; "));
            }
            Ok(_) => {}
            Err(e) => {
                self.status_message = format!(
                    "{} ⚠ Could not count blank balances: {:#}",
                    self.status_message, e
                );
            }
        }
    }

    fn refresh_active_tab(&mut self, recalculate_baseline: bool) {
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
                // A failure shows "—" and the reason instead of keeping the previous totals.
                tab.totals = control_totals_state(Ok(lf), tab.control_col.as_deref());
                tab.total_rows = tab.totals.rows.unwrap_or(0);
                tab.control_sum = tab.totals.sum;
                if recalculate_baseline {
                    // Recompute baseline from the base source (0 steps) for the newly selected control column
                    let base_sum = compile_tab_lazy(&tab.source, &[], &self.paths.temp_dir)
                        .ok()
                        .and_then(|base_lf| {
                            compute_control_totals(base_lf, tab.control_col.as_deref()).ok()
                        })
                        .and_then(|(_, s)| s);
                    tab.baseline_control_sum = base_sum;
                }
                if tab.join_status.is_some() {
                    tab.join_status = join_status(
                        &tab.source,
                        &tab.steps,
                        tab.control_col.as_deref(),
                        &self.paths.temp_dir,
                    )
                    .ok()
                    .flatten();
                }
                self.init_tool_defaults_from_active_tab();
            }
            Err(e) => {
                let msg = format!("{:#}", e);
                tab.totals = control_totals_state(Err(e), tab.control_col.as_deref());
                tab.control_sum = None;
                self.status_message = format!("Step execution failed: {}", msg);
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
                        curr.col_formats,
                    );
                } else {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.steps.extend(recipe.steps);
                        if recipe.enforce_balance_control && recipe.control_column.is_some() {
                            tab.control_col = recipe.control_column;
                        }
                    }
                    self.refresh_active_tab(true);
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
        // Masked columns anywhere in the source (also inside stacks and joins) refuse the export.
        if source_masks_pii(&tab.source) {
            let mut masked: Vec<String> = Vec::new();
            collect_masked_columns(&tab.source, &mut masked);
            self.status_message = python_pii_refusal(&masked);
            return;
        }
        self.generated_python_code = match generate_python_polars_code(
            &path,
            is_parq,
            &cfg,
            &tab.steps,
            tab.control_col.as_deref(),
        ) {
            Ok(py) => py,
            Err(e) => {
                self.status_message = format!("{:#}", e);
                return;
            }
        };
        let missing = python_untranslated_steps(&tab.steps);
        self.python_export_warning = if missing.is_empty() {
            String::new()
        } else {
            format!(
                " ⚠ WARNING: {} has no Python translation; the script raises NotImplementedError there.",
                missing.join(", ")
            )
        };
        if !self.python_export_warning.is_empty() {
            self.status_message = format!("Python preview ready.{}", self.python_export_warning);
        }
        self.active_tool = ActiveToolModal::PythonCodePreview;
    }
}

impl eframe::App for MicroParquetApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Compact 720p-Friendly Top Menu Bar with Sub-Menus
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
                        Self::apply_ui_theme(ctx, self.dark_mode, self.ui_zoom);
                    }

                    ui.horizontal(|ui| {
                        ui.label("UI Scale:");
                        if ui.small_button("🔍-").clicked() {
                            self.ui_zoom = (self.ui_zoom - 0.10).clamp(0.80, 1.50);
                            Self::apply_ui_theme(ctx, self.dark_mode, self.ui_zoom);
                        }
                        ui.strong(format!("{:.0}%", self.ui_zoom * 100.0));
                        if ui.small_button("🔍+").clicked() {
                            self.ui_zoom = (self.ui_zoom + 0.10).clamp(0.80, 1.50);
                            Self::apply_ui_theme(ctx, self.dark_mode, self.ui_zoom);
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
                        self.alm_presets = CustomBucketDef::load_all_presets(&self.paths.buckets_file);
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
                        self.multi_export_checks = vec![true; self.tabs.len()];
                        self.active_tool = ActiveToolModal::MultiTabExport;
                    }
                    if ui.button("📄 Multi-Tab Consolidated PDF (.pdf)...").clicked() {
                        ui.close_menu();
                        self.multi_export_checks = vec![true; self.tabs.len()];
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

        // 2. Bottom Pinned Control Total & Reconciliation Bar
        egui::TopBottomPanel::bottom("status_reconciliation_bar").show(ctx, |ui| {
            let mut reset_baseline_clicked = false;
            ui.horizontal(|ui| {
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    ui.strong(format!(
                        "Rows: {}",
                        tab.totals
                            .rows
                            .map(|r| format_with_commas(r as f64, 0))
                            .unwrap_or_else(|| "—".to_string())
                    ));
                    ui.separator();
                    if let (Some(col_name), None) = (&tab.control_col, tab.control_sum) {
                        ui.colored_label(
                            egui::Color32::from_rgb(210, 70, 0),
                            format!("Control [{}]: —", col_name),
                        );
                        ui.separator();
                    }
                    if let Some(note) = &tab.totals.note {
                        ui.colored_label(egui::Color32::from_rgb(210, 70, 0), format!("⚠ {}", note));
                        ui.separator();
                    }

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
                                    format!("Δ vs Source: {:+.2} Cr", diff / 10_000_000.0),
                                );
                                if ui
                                    .small_button("🔄 Reset Baseline")
                                    .on_hover_text("Pin current total as the new 0.00 Cr baseline")
                                    .clicked()
                                {
                                    reset_baseline_clicked = true;
                                }
                            }
                        }
                        ui.separator();
                    }
                }
                if let Some(rc) = self.tabs.get(self.active_tab).and_then(|t| t.row_check.as_ref()) {
                    if rc.warning().is_some() {
                        ui.colored_label(
                            egui::Color32::from_rgb(210, 70, 0),
                            format!(
                                "⚠ Rows read {} vs file data lines {}",
                                format_with_commas(rc.parsed_rows as f64, 0),
                                format_with_commas(rc.data_lines() as f64, 0)
                            ),
                        )
                        .on_hover_text(rc.warning().unwrap_or_default());
                        ui.separator();
                    }
                }
                if let Some(js) = self.tabs.get(self.active_tab).and_then(|t| t.join_status.as_ref()) {
                    let warn = js.duplicate_keys > 0 || js.rows != js.left_rows;
                    let color = if warn {
                        egui::Color32::from_rgb(210, 70, 0)
                    } else {
                        egui::Color32::from_rgb(0, 110, 200)
                    };
                    ui.colored_label(color, js.summary());
                    ui.separator();
                }
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    if !tab.parse_failures.is_empty() {
                        let detail: Vec<String> = tab
                            .parse_failures
                            .iter()
                            .map(|f| format!("{}: {}", f.column, f.examples.join(" | ")))
                            .collect();
                        ui.colored_label(
                            egui::Color32::from_rgb(210, 70, 0),
                            format!("⚠ {}", parse_failure_summary(&tab.parse_failures)),
                        )
                        .on_hover_text(format!(
                            "Non-blank values that could not be converted and are null. Examples:\n{}",
                            detail.join("\n")
                        ));
                        ui.separator();
                    }
                }
                ui.label(&self.status_message);
            });

            if reset_baseline_clicked {
                if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                    tab.baseline_control_sum = tab.control_sum;
                }
            }
        });

        // 3. Toggleable Right Side Panel: Applied Pipeline Steps (Hidden by default on 720p)
        if !self.tabs.is_empty() && self.show_steps_sidebar {
            egui::SidePanel::right("applied_steps_sidebar")
                .resizable(true)
                .default_width(255.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading("📋 Applied Steps");
                        if ui.small_button("✖ Hide").clicked() {
                            self.show_steps_sidebar = false;
                        }
                    });
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
                    let mut open_schema = false;

                    if let Some(tab) = self.tabs.get(self.active_tab) {
                        ui.horizontal(|ui| {
                            ui.label("0. [Source Locked & Sanitized]");
                            if matches!(tab.source, TabSource::SingleFile { .. })
                                && ui
                                    .small_button("🧬 Schema & Ingestion")
                                    .on_hover_text("Column types, Dr/Cr sign and parse failures")
                                    .clicked()
                            {
                                open_schema = true;
                            }
                        });
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

                    if open_schema {
                        self.open_schema_modal_for_active_tab();
                    }
                    if let Some(idx) = step_to_remove {
                        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                            tab.steps.remove(idx);
                        }
                        self.refresh_active_tab(false);
                    } else if clear_all {
                        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                            tab.steps.clear();
                        }
                        self.refresh_active_tab(false);
                    }
                });
        }

        // 4. Central Workspace
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.tabs.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(90.0);
                    ui.heading(
                        "MicroParquet v1.2: Portable CSV/Parquet, ALM & Loans Analytics Workstation",
                    );
                    ui.label("Out-of-Core Polars Engine • Multi-Tab Views • Custom Maturity Buckets • Per-Metric Formats");
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
                    let title = self.tabs.get(idx).map(|t| t.title.clone()).unwrap_or_default();
                    let tab_label = format!("{} ({})", title, idx + 1);
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
                if idx < self.tabs.len() {
                    self.tabs.remove(idx);
                    // Every stored tab index must stay valid (see tab_index_after_close)
                    let new_len = self.tabs.len();
                    for sel in [
                        &mut self.active_tab,
                        &mut self.join_left_tab,
                        &mut self.join_right_tab,
                        &mut self.rec_prev_tab,
                        &mut self.rec_curr_tab,
                    ] {
                        *sel = tab_index_after_close(*sel, idx, new_len);
                    }
                }
                return;
            }

            ui.separator();

            // Quick Transformation & Clipboard Action Bar
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
                    self.alm_presets = CustomBucketDef::load_all_presets(&self.paths.buckets_file);
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

            if self.show_sql_bar {
                ui.separator();
                let mut sql_to_run: Option<(String, bool)> = None;
                if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                    ui.horizontal(|ui| {
                        ui.strong("SQL (`self`):");
                        ui.add(
                            egui::TextEdit::singleline(&mut tab.sql_scratchpad)
                                .desired_width(520.0),
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
            let mut ctrl_col_changed = false;
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Rows {} – {} of {}",
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
                                    ctrl_col_changed = true;
                                }
                            }
                        });
                });

                ui.separator();
                Self::render_data_grid(
                    ui,
                    &tab.cached_slice,
                    &tab.col_formats,
                    tab.viewport_offset,
                    self.num_format,
                    self.global_decimals,
                );
            }

            if needs_refresh {
                self.refresh_active_tab(ctrl_col_changed);
            }
        });

        // Modals
        self.render_csv_schema_modal(ctx);
        self.render_export_guard(ctx);
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
        col_formats: &HashMap<String, ColumnFormatSpec>,
        row_offset: usize,
        num_format: NumberDisplayFormat,
        global_decimals: usize,
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
                                let cname = c.name().as_str();
                                let spec = col_formats.get(cname);
                                let cell_text = match c.get(r_idx) {
                                    Ok(AnyValue::Null) => "null".to_string(),
                                    Ok(AnyValue::Float64(v)) => format_metric_value(
                                        v,
                                        cname,
                                        spec,
                                        num_format,
                                        global_decimals,
                                    ),
                                    Ok(AnyValue::Float32(v)) => format_metric_value(
                                        v as f64,
                                        cname,
                                        spec,
                                        num_format,
                                        global_decimals,
                                    ),
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

        // Default the side picker whenever the column set changes (not every frame, so a
        // deliberate "none" choice sticks): first column named like side/direction.
        if matches!(
            self.active_tool,
            ActiveToolModal::RateShockSim | ActiveToolModal::DurationAnalysis
        ) && self.alm_side_cols_seen != avail_cols
        {
            self.alm_side_cols_seen = avail_cols.clone();
            self.alm_side_col = default_side_column(&avail_cols);
        }

        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;

        match self.active_tool {
            ActiveToolModal::FilterRows => {
                egui::Window::new("🔍 Filter Rows / Delete Filtered Rows")
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
                egui::Window::new("📐 Columns: Drop, Order, Rename & Number Format (v1.2)")
                    .collapsible(false)
                    .default_width(680.0)
                    .show(ctx, |ui| {
                        ui.strong("1. Keep/Drop, Reorder & Set Per-Column Display Format:");
                        egui::ScrollArea::vertical()
                            .max_height(240.0)
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
                                        let (ref col_name, ref mut is_checked, ref mut spec) =
                                            self.col_mgr_list[i];
                                        ui.checkbox(is_checked, col_name.as_str());

                                        egui::ComboBox::from_id_salt(format!("col_fmt_{}", i))
                                            .selected_text(spec.fmt_type.label())
                                            .show_ui(ui, |ui| {
                                                for ft in MetricFormatType::all() {
                                                    ui.selectable_value(
                                                        &mut spec.fmt_type,
                                                        *ft,
                                                        ft.label(),
                                                    );
                                                }
                                            });
                                        ui.label("Dec:");
                                        ui.add(
                                            egui::DragValue::new(&mut spec.decimals).range(0..=6),
                                        );
                                    });
                                }
                                if let Some((a, b)) = swap_pair {
                                    self.col_mgr_list.swap(a, b);
                                }
                            });

                        if ui.button("✅ Apply Column Selection, Order & Formats").clicked() {
                            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                                for (cname, _, spec) in &self.col_mgr_list {
                                    tab.col_formats.insert(cname.clone(), *spec);
                                }
                            }
                            let kept: Vec<String> = self
                                .col_mgr_list
                                .iter()
                                .filter(|(_, k, _)| *k)
                                .map(|(n, _, _)| n.clone())
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
                egui::Window::new("🏢 Counterparty Entity Name Normalizer")
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
                    "📊 Pivot & WAVG Builder (with Per-Metric Format & Decimal Precision)",
                )
                .collapsible(false)
                .default_width(720.0)
                .show(ctx, |ui| {
                    ui.strong("1. Select Row Grouping Columns:");
                    ui.horizontal_wrapped(|ui| {
                        for (cname, checked) in &mut self.pivot_group_cols {
                            ui.checkbox(checked, cname.as_str());
                        }
                    });

                    ui.separator();
                    ui.strong("2. Configure Value Metric & Display Format (Crore vs % vs Decimals):");
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("piv_func")
                            .selected_text(self.pivot_new_func.label())
                            .show_ui(ui, |ui| {
                                for f in AggFunc::all() {
                                    ui.selectable_value(&mut self.pivot_new_func, *f, f.label());
                                }
                            });
                        ui.label("Target:");
                        egui::ComboBox::from_id_salt("piv_tgt")
                            .selected_text(&self.pivot_new_target)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.pivot_new_target, c.clone(), c);
                                }
                            });
                        if self.pivot_new_func == AggFunc::WeightedAvg {
                            ui.label("Weight:");
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
                        ui.label("Alias:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.pivot_new_alias)
                                .desired_width(130.0),
                        );
                        ui.label("Format:");
                        egui::ComboBox::from_id_salt("piv_fmt")
                            .selected_text(self.pivot_new_fmt.fmt_type.label())
                            .show_ui(ui, |ui| {
                                for ft in MetricFormatType::all() {
                                    ui.selectable_value(
                                        &mut self.pivot_new_fmt.fmt_type,
                                        *ft,
                                        ft.label(),
                                    );
                                }
                            });
                        ui.label("Decimals:");
                        ui.add(
                            egui::DragValue::new(&mut self.pivot_new_fmt.decimals).range(0..=6),
                        );
                        if ui.button("➕ Add Metric").clicked() {
                            self.pivot_aggs.push(AggSpec {
                                target_col: self.pivot_new_target.clone(),
                                func: self.pivot_new_func,
                                weight_col: if self.pivot_new_func == AggFunc::WeightedAvg {
                                    Some(self.pivot_new_weight.clone())
                                } else {
                                    None
                                },
                                alias: self.pivot_new_alias.clone(),
                                format_spec: self.pivot_new_fmt,
                            });
                        }
                    });

                    for a in &self.pivot_aggs {
                        ui.label(format!(
                            "  • {}: {:?}({}) — Format: {} ({} dec)",
                            a.alias,
                            a.func,
                            a.target_col,
                            a.format_spec.fmt_type.label(),
                            a.format_spec.decimals
                        ));
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
                egui::Window::new("📅 Maturity Bucketer & Custom Preset Creator (v1.2)")
                    .collapsible(false)
                    .default_width(680.0)
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
                            ui.label("Cutoff Date (YYYY-MM-DD):");
                            ui.text_edit_singleline(&mut self.alm_cutoff);
                        });

                        let selected_name = self
                            .alm_presets
                            .get(self.alm_selected_preset_idx)
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| "Select Preset".to_string());

                        ui.horizontal(|ui| {
                            ui.label("Bucketing Preset:");
                            egui::ComboBox::from_id_salt("alm_preset_combo")
                                .selected_text(&selected_name)
                                .show_ui(ui, |ui| {
                                    for (idx, preset) in self.alm_presets.iter().enumerate() {
                                        ui.selectable_value(
                                            &mut self.alm_selected_preset_idx,
                                            idx,
                                            &preset.name,
                                        );
                                    }
                                });

                            if ui
                                .button(if self.alm_show_custom_creator {
                                    "▲ Hide Custom Creator"
                                } else {
                                    "➕ Create Custom Preset (e.g. c-m-b-1)"
                                })
                                .clicked()
                            {
                                self.alm_show_custom_creator = !self.alm_show_custom_creator;
                            }
                        });

                        // Custom Maturity Bucket Preset Creator Drawer (v1.2)
                        if self.alm_show_custom_creator {
                            ui.separator();
                            egui::Frame::group(ui.style()).show(ui, |ui| {
                                ui.strong("🛠 Custom Maturity Bucket Preset Creator");
                                ui.horizontal(|ui| {
                                    ui.label("Custom Preset Name:");
                                    ui.text_edit_singleline(&mut self.custom_bucket_draft.name);
                                });

                                let mut remove_rule = None;
                                for (r_idx, rule) in
                                    self.custom_bucket_draft.rules.iter_mut().enumerate()
                                {
                                    ui.horizontal(|ui| {
                                        ui.label("Up to Days:");
                                        ui.add(
                                            egui::DragValue::new(&mut rule.max_days)
                                                .range(1..=36500),
                                        );
                                        ui.label("Bucket Label:");
                                        ui.text_edit_singleline(&mut rule.label);
                                        if ui.small_button("✖").clicked() {
                                            remove_rule = Some(r_idx);
                                        }
                                    });
                                }
                                if let Some(ri) = remove_rule {
                                    if self.custom_bucket_draft.rules.len() > 1 {
                                        self.custom_bucket_draft.rules.remove(ri);
                                    }
                                }
                                if ui.small_button("➕ Add Day Threshold Row").clicked() {
                                    let next_days = self
                                        .custom_bucket_draft
                                        .rules
                                        .last()
                                        .map(|r| r.max_days + 30)
                                        .unwrap_or(30);
                                    self.custom_bucket_draft.rules.push(BucketRuleItem {
                                        max_days: next_days,
                                        label: format!("Up to {} Days", next_days),
                                    });
                                }

                                ui.horizontal(|ui| {
                                    ui.label("Above Max Days Label:");
                                    ui.text_edit_singleline(
                                        &mut self.custom_bucket_draft.overflow_label,
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Overdue (<0 Days) Label:");
                                    ui.text_edit_singleline(
                                        &mut self.custom_bucket_draft.overdue_label,
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Null Matdat (Sight) Label:");
                                    ui.text_edit_singleline(
                                        &mut self.custom_bucket_draft.null_sight_label,
                                    );
                                });

                                if ui.button("💾 Save Custom Preset to Dropdown").clicked()
                                    && !self.custom_bucket_draft.name.trim().is_empty()
                                {
                                    if let Ok(_) = CustomBucketDef::save_custom_preset(
                                        &self.paths.buckets_file,
                                        self.custom_bucket_draft.clone(),
                                    ) {
                                        self.alm_presets = CustomBucketDef::load_all_presets(
                                            &self.paths.buckets_file,
                                        );
                                        self.alm_selected_preset_idx =
                                            self.alm_presets.len().saturating_sub(1);
                                        self.alm_show_custom_creator = false;
                                        self.status_message = format!(
                                            "Saved custom bucket preset '{}'.",
                                            self.custom_bucket_draft.name
                                        );
                                    }
                                }
                            });
                            ui.separator();
                        }

                        ui.checkbox(
                            &mut self.alm_null_sight,
                            "Treat Null Maturity Dates (e.g. Current/Savings) as Sight/Overnight",
                        );
                        ui.horizontal(|ui| {
                            ui.label("Output Bucket Column Name:");
                            ui.text_edit_singleline(&mut self.alm_new_col);
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("✅ Generate Maturity Buckets").clicked() {
                                if let Some(chosen_def) =
                                    self.alm_presets.get(self.alm_selected_preset_idx).cloned()
                                {
                                    step_to_apply = Some((
                                        TransformStep::MaturityBucket {
                                            mat_col: self.alm_mat_col.clone(),
                                            cutoff_date: self.alm_cutoff.clone(),
                                            bucket_def: chosen_def,
                                            null_as_sight: self.alm_null_sight,
                                            new_col: self.alm_new_col.clone(),
                                        },
                                        false,
                                    ));
                                }
                                close_modal = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close_modal = true;
                            }
                        });
                    });
            }

            ActiveToolModal::RateShockSim => {
                egui::Window::new("📈 Interest Rate Shock / ΔNII Sensitivity Simulator")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.label("Calculates 1-Year repricing-weighted Net Interest Income impact.");
                        ui.label("NII_Impact_BDT is signed (+ asset, − liability). Shocked_Annual_Int is gross income or expense after the shock, unsigned.");
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
                        ui.horizontal(|ui| {
                            ui.label("Asset/Liability Side Col:");
                            let shown = if self.alm_side_col.is_empty() {
                                "(none: treat all rows as assets)".to_string()
                            } else {
                                self.alm_side_col.clone()
                            };
                            egui::ComboBox::from_id_salt("shk_side")
                                .selected_text(shown)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.alm_side_col,
                                        String::new(),
                                        "(none: treat all rows as assets)",
                                    );
                                    for c in &avail_cols {
                                        ui.selectable_value(&mut self.alm_side_col, c.clone(), c);
                                    }
                                });
                        });
                        ui.label("Side values: A/ASSET = asset; L/LIABILITY, DEPOSIT, BORROWING = liability.");
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
                                        side_col: if self.alm_side_col.trim().is_empty() {
                                            None
                                        } else {
                                            Some(self.alm_side_col.clone())
                                        },
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
                egui::Window::new("⏱ IRRBB Duration, DV01 & ΔEVE Sensitivity Engine")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.label("Computes Residual Years (T), Macaulay Duration, Modified Duration, DV01, and ΔEVE.");
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
                            ui.label("Parallel Rate Shock for ΔEVE (bps):");
                            ui.add(egui::DragValue::new(&mut self.dur_shock_bps).speed(10.0));
                            ui.label("Null Matdat Tenor (Yrs):");
                            ui.add(egui::DragValue::new(&mut self.dur_sight_yrs).speed(0.05));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Asset/Liability Side Col:");
                            let shown = if self.alm_side_col.is_empty() {
                                "(none: treat all rows as assets)".to_string()
                            } else {
                                self.alm_side_col.clone()
                            };
                            egui::ComboBox::from_id_salt("dur_side")
                                .selected_text(shown)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.alm_side_col,
                                        String::new(),
                                        "(none: treat all rows as assets)",
                                    );
                                    for c in &avail_cols {
                                        ui.selectable_value(&mut self.alm_side_col, c.clone(), c);
                                    }
                                });
                        });
                        ui.label("Side values: A/ASSET = asset; L/LIABILITY, DEPOSIT, BORROWING = liability.");
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
                                        side_col: if self.alm_side_col.trim().is_empty() {
                                            None
                                        } else {
                                            Some(self.alm_side_col.clone())
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

            ActiveToolModal::LoanProvision => {
                egui::Window::new("🏦 Bangladesh Bank CL Provision & BFP Calculator")
                    .collapsible(false)
                    .show(ctx, |ui| {
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
                            ui.label("Optional Segment/Sector Col:");
                            ui.text_edit_singleline(&mut self.loan_seg_col);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Optional Interest Suspense Col:");
                            ui.text_edit_singleline(&mut self.loan_susp_col);
                            ui.label("Optional Eligible Security Col:");
                            ui.text_edit_singleline(&mut self.loan_sec_col);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Optional Collateral Type Col:");
                            ui.text_edit_singleline(&mut self.loan_coll_type_col);
                            ui.label("Cutoff Date (YYYY-MM-DD, blank = today):");
                            ui.text_edit_singleline(&mut self.loan_cutoff);
                        });
                        ui.label("Rates, CL codes and segments come from cl_code_map.json, segment_map.json and provision_rules.json in portable_data.");
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
                                        cutoff_date: self.loan_cutoff.trim().to_string(),
                                        collateral_type_col: if self.loan_coll_type_col.trim().is_empty() {
                                            None
                                        } else {
                                            Some(self.loan_coll_type_col.trim().to_string())
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
                egui::Window::new("💳 Loan Limit Utilization, Over-Limit Audit & Undrawn CCF")
                    .collapsible(false)
                    .show(ctx, |ui| {
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
                            ui.label("Credit Conversion Factor (CCF % for Undrawn Portion):");
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
                egui::Window::new("🧮 Automated FTP Yield Curve Tenor Mapper")
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
                        ui.horizontal(|ui| {
                            ui.label("Rate between tenor points:");
                            for m in [FtpInterpolation::Linear, FtpInterpolation::NextPointUp] {
                                ui.radio_value(&mut self.ftp_interpolation, m, m.label());
                            }
                        });
                        ui.label("Below the first point the first rate applies; beyond the last point the last rate. The FTP_Method column shows which rule each row used.");
                        ui.strong("Benchmark Yield Curve (Tenor Days -> FTP Rate %):");
                        for pt in &mut self.ftp_curve {
                            ui.horizontal(|ui| {
                                ui.label("Tenor Days:");
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
                                        interpolation: self.ftp_interpolation,
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
                egui::Window::new("🛡 Core Banking Dirty Data & Ghost Account Auditor")
                    .collapsible(false)
                    .show(ctx, |ui| {
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
                        ui.horizontal(|ui| {
                            ui.label("Active status codes (comma-separated, any case):");
                            ui.text_edit_singleline(&mut self.audit_active_statuses);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Rate outlier below %:");
                            ui.add(egui::DragValue::new(&mut self.audit_rate_min).speed(0.1));
                            ui.label("or above %:");
                            ui.add(egui::DragValue::new(&mut self.audit_rate_max).speed(0.1));
                        });
                        ui.label("Audit_Exception_Flag shows the first flag; Audit_All_Flags lists every flag that applies.");
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
                                        active_statuses: self
                                            .audit_active_statuses
                                            .split(',')
                                            .map(|s| s.trim().to_string())
                                            .filter(|s| !s.is_empty())
                                            .collect(),
                                        rate_min_pct: self.audit_rate_min,
                                        rate_max_pct: self.audit_rate_max,
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
                egui::Window::new("📚 Vertical Multi-File Merger / Stacker")
                    .collapsible(false)
                    .show(ctx, |ui| {
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
                        ui.label("Column types (a column missing from a file is null for its rows):");
                        ui.radio_value(&mut self.stack_schema_mode, 0, "Each file's own detected types (types must agree)");
                        ui.radio_value(&mut self.stack_schema_mode, 1, "Apply one schema to every file: the first file's");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut self.stack_schema_mode, 2, "Apply one schema to every file: saved .schema.json");
                            if ui.button("📂 Pick...").clicked() {
                                if let Some(p) = rfd::FileDialog::new()
                                    .set_directory(&self.paths.schemas_dir)
                                    .add_filter("Schema JSON", &["json"])
                                    .pick_file()
                                {
                                    self.stack_schema_path = Some(p);
                                    self.stack_schema_mode = 2;
                                }
                            }
                            if let Some(p) = &self.stack_schema_path {
                                ui.label(p.file_name().unwrap_or_default().to_string_lossy().to_string());
                            }
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            let needs_template =
                                self.stack_schema_mode == 2 && self.stack_schema_path.is_none();
                            let run = ui
                                .add_enabled(
                                    !needs_template,
                                    egui::Button::new("🚀 Stack & Open as Unified Tab"),
                                )
                                .on_disabled_hover_text("Pick the .schema.json to apply first");
                            if run.clicked() && !self.stack_files_list.is_empty() {
                                let shared: anyhow::Result<Option<IngestionConfig>> =
                                    match (self.stack_schema_mode, &self.stack_schema_path) {
                                        (1, _) => self
                                            .stack_files_list
                                            .first()
                                            .map(|p| detect_file_schema(p))
                                            .transpose(),
                                        (2, Some(p)) => std::fs::read_to_string(p)
                                            .map_err(anyhow::Error::from)
                                            .and_then(|t| {
                                                serde_json::from_str::<IngestionConfig>(&t)
                                                    .map_err(anyhow::Error::from)
                                            })
                                            .map(Some)
                                            .map_err(|e| {
                                                anyhow::anyhow!("Could not load the schema file {:?}: {}", p, e)
                                            }),
                                        _ => Ok(None),
                                    };
                                match shared.and_then(|s| {
                                    stack_file_configs(&self.stack_files_list, s.as_ref())
                                }) {
                                    Err(e) => {
                                        self.status_message = format!("Stack failed: {:#}", e);
                                    }
                                    Ok((specs, notes)) => {
                                        self.create_tab_from_source(
                                            format!("Stacked ({} files)", specs.len()),
                                            TabSource::VerticalStack {
                                                files: specs,
                                                include_source_col: self.stack_include_source,
                                            },
                                            Vec::new(),
                                            HashMap::new(),
                                        );
                                        if !notes.is_empty() {
                                            self.status_message
                                                .push_str(&format!(" ⚠ {}", notes.join("; ")));
                                        }
                                        close_modal = true;
                                    }
                                }
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
                egui::Window::new("🔗 SQL-Style Multi-Tab Joiner")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        if self.tabs.len() < 2 {
                            ui.label("Please open at least 2 tabs to perform a join.");
                        } else {
                            let left_title = self
                                .tabs
                                .get(self.join_left_tab)
                                .map(|t| t.title.clone())
                                .unwrap_or_default();
                            let right_title = self
                                .tabs
                                .get(self.join_right_tab)
                                .map(|t| t.title.clone())
                                .unwrap_or_default();
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
                                ui.label("Join Key Column(s) (e.g. Actnum):");
                                ui.text_edit_singleline(&mut self.join_keys_input);
                            });
                            ui.checkbox(&mut self.join_keep_first, "Keep first row per key")
                                .on_hover_text("Off: a key repeated in the right table is an error. On: the first right row per key is used and the status bar shows how many were dropped.");
                            ui.separator();
                            if ui.button("🚀 Execute Join into New Tab").clicked() {
                                let (Some(l_tab), Some(r_tab)) = (
                                    self.tabs.get(self.join_left_tab).cloned(),
                                    self.tabs.get(self.join_right_tab).cloned(),
                                ) else {
                                    self.status_message =
                                        "Join: the selected left or right tab no longer exists. Pick both tabs again.".to_string();
                                    return;
                                };
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
                                        keep_first_duplicate_right: self.join_keep_first,
                                    },
                                    Vec::new(),
                                    l_tab.col_formats,
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
                egui::Window::new("⚖ Two-Period Variance & Loan CL Migration Reconciler")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        if self.tabs.len() < 2 {
                            ui.label(
                                "Open Previous Period (e.g. Sep) and Current Period (e.g. Oct) in 2 tabs first.",
                            );
                        } else {
                            let prev_title = self
                                .tabs
                                .get(self.rec_prev_tab)
                                .map(|t| t.title.clone())
                                .unwrap_or_default();
                            let curr_title = self
                                .tabs
                                .get(self.rec_curr_tab)
                                .map(|t| t.title.clone())
                                .unwrap_or_default();
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
                                let (Some(p_tab), Some(c_tab)) = (
                                    self.tabs.get(self.rec_prev_tab).cloned(),
                                    self.tabs.get(self.rec_curr_tab).cloned(),
                                ) else {
                                    self.status_message =
                                        "Reconcile: the selected previous or current tab no longer exists. Pick both tabs again.".to_string();
                                    return;
                                };
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
                                let tabs_before = self.tabs.len();
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
                                    HashMap::new(),
                                );
                                if self.tabs.len() > tabs_before {
                                    let multi = self.tabs.last().map(|t| {
                                        compile_tab_lazy(&t.source, &t.steps, &self.paths.temp_dir)
                                            .and_then(micro_parquet::banking::reconcile_multi_row_keys)
                                    });
                                    match multi {
                                        Some(Ok((p, c))) => self.status_message.push_str(&format!(
                                            " Keys with more than one row: {} in the previous period, {} in the current (rows are combined per key; see Prev_Row_Count / Curr_Row_Count).",
                                            p, c
                                        )),
                                        Some(Err(e)) => self.status_message.push_str(&format!(
                                            " ⚠ Could not count keys with more than one row: {:#}",
                                            e
                                        )),
                                        None => {}
                                    }
                                }
                                close_modal = true;
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            close_modal = true;
                        }
                    });
            }

            ActiveToolModal::SaveMacro => {
                egui::Window::new("💾 Save Portable Macro Recipe (.mpq.json)")
                    .collapsible(false)
                    .show(ctx, |ui| {
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
                            "Enforce Control Column Reconciliation Check during replay",
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
                egui::Window::new("🐍 Export GUI Pipeline as Python Polars Jupyter Code")
                    .collapsible(false)
                    .default_width(760.0)
                    .show(ctx, |ui| {
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
                                self.status_message = format!(
                                    "Copied Python Polars code to clipboard!{}",
                                    self.python_export_warning
                                );
                            }
                            if ui.button("💾 Save as .py Script...").clicked() {
                                if let Some(p) = rfd::FileDialog::new()
                                    .set_file_name("micro_parquet_pipeline.py")
                                    .save_file()
                                {
                                    self.status_message =
                                        match std::fs::write(&p, &self.generated_python_code) {
                                            Ok(()) => format!(
                                                "Saved Python script to {:?}{}",
                                                p, self.python_export_warning
                                            ),
                                            Err(e) => format!(
                                                "Could not save Python script to {:?}: {}",
                                                p, e
                                            ),
                                        };
                                }
                            }
                            if ui.button("Close").clicked() {
                                close_modal = true;
                            }
                        });
                    });
            }

            ActiveToolModal::MultiTabExport => {
                egui::Window::new("📊 Consolidated Multi-Tab Exporter (.xlsx / .pdf)")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.strong("Select Open Tabs / Views to Include:");
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
            }

            ActiveToolModal::SplitByColumn => {
                egui::Window::new("🗂 Smart Partitioned Split Exporter")
                    .collapsible(false)
                    .show(ctx, |ui| {
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
                                self.run_guarded_export(GuardedExport::Split);
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
                            ui.colored_label(egui::Color32::from_rgb(200, 30, 30), format!("{:#}", e));
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
                    egui::Color32::from_rgb(210, 70, 0),
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
                                            egui::Color32::from_rgb(220, 120, 0),
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
                                                egui::Color32::from_rgb(210, 70, 0),
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
            .default_width(680.0)
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
                    if s == BatchSchemaStrategy::MasterSchemaTemplate {
                        ui.horizontal(|ui| {
                            if ui.button("📂 Pick .schema.json...").clicked() {
                                if let Some(p) = rfd::FileDialog::new()
                                    .set_directory(&self.paths.schemas_dir)
                                    .add_filter("Schema JSON", &["json"])
                                    .pick_file()
                                {
                                    match std::fs::read_to_string(&p)
                                        .map_err(anyhow::Error::from)
                                        .and_then(|t| {
                                            serde_json::from_str::<IngestionConfig>(&t)
                                                .map_err(anyhow::Error::from)
                                        }) {
                                        Ok(cfg) => {
                                            self.batch_modal.loaded_template = Some(cfg);
                                            self.batch_modal.template_path = Some(p);
                                            self.batch_modal.last_error = None;
                                        }
                                        Err(e) => {
                                            self.batch_modal.loaded_template = None;
                                            self.batch_modal.template_path = None;
                                            self.batch_modal.last_error = Some(format!(
                                                "Could not load the template {:?}: {}",
                                                p, e
                                            ));
                                        }
                                    }
                                }
                            }
                            match &self.batch_modal.template_path {
                                Some(p) => ui.label(format!("{:?}", p)),
                                None => ui.colored_label(
                                    egui::Color32::from_rgb(210, 70, 0),
                                    "No template chosen",
                                ),
                            };
                        });
                    }
                }

                ui.separator();
                let needs_template = d == BatchDirection::CsvToParquet
                    && s == BatchSchemaStrategy::MasterSchemaTemplate
                    && self.batch_modal.loaded_template.is_none();
                let have_folders = self.batch_modal.input_folder.is_some()
                    && self.batch_modal.output_folder.is_some();
                ui.horizontal(|ui| {
                    let run = ui
                        .add_enabled(
                            !needs_template && have_folders,
                            egui::Button::new("🚀 Start Mirror Batch"),
                        )
                        .on_disabled_hover_text(if needs_template {
                            "Pick the .schema.json template first"
                        } else {
                            "Choose the input and output folders first"
                        });
                    if run.clicked() {
                        if let (Some(in_dir), Some(out_dir)) = (
                            self.batch_modal.input_folder.clone(),
                            self.batch_modal.output_folder.clone(),
                        ) {
                            match run_folder_mirror_batch(
                                &in_dir,
                                &out_dir,
                                d,
                                s,
                                self.batch_modal.loaded_template.as_ref(),
                                self.parquet_dialect,
                                self.batch_modal.mdy_dates_on_csv,
                                &self.paths.temp_dir,
                            ) {
                                Ok(rep) => {
                                    self.batch_modal.last_report = Some(rep);
                                    self.batch_modal.last_error = None;
                                }
                                Err(e) => {
                                    self.batch_modal.last_report = None;
                                    self.batch_modal.last_error = Some(format!("{:#}", e));
                                }
                            }
                        }
                    }
                    if ui.button("Close").clicked() {
                        close_clicked = true;
                    }
                });

                if needs_template {
                    ui.colored_label(
                        egui::Color32::from_rgb(210, 70, 0),
                        "Pick a .schema.json template, or choose Smart Safe Auto-Detect.",
                    );
                }
                if let Some(err) = &self.batch_modal.last_error {
                    ui.colored_label(egui::Color32::from_rgb(200, 30, 30), format!("❌ {}", err));
                }
                if let Some(rep) = &self.batch_modal.last_report {
                    ui.separator();
                    ui.label(format!(
                        "Converted: {} | Companion Copied: {} | Errors: {} | Size: {:.1} MB -> {:.1} MB",
                        rep.files_converted,
                        rep.companion_files_copied,
                        rep.errors.len(),
                        rep.bytes_before as f64 / 1_048_576.0,
                        rep.bytes_after as f64 / 1_048_576.0
                    ));
                    if let Some(p) = &rep.log_path {
                        ui.label(format!("Log: {:?}", p));
                    }
                    let fails: usize = rep.log.iter().filter_map(|r| r.parse_failures).sum();
                    if fails > 0 {
                        ui.colored_label(
                            egui::Color32::from_rgb(210, 70, 0),
                            format!("⚠ {} value(s) could not be converted and are null (see parse_failures in the log)", fails),
                        );
                    }
                    if !rep.warnings.is_empty() {
                        ui.colored_label(
                            egui::Color32::from_rgb(210, 70, 0),
                            format!("⚠ {} file(s) with fewer rows than data lines (status WARNING in the log):", rep.warnings.len()),
                        );
                        egui::ScrollArea::vertical()
                            .id_salt("batch_warnings")
                            .max_height(120.0)
                            .show(ui, |ui| {
                                for w in &rep.warnings {
                                    ui.label(format!("• {}", w));
                                }
                            });
                    }
                    if !rep.errors.is_empty() {
                        ui.colored_label(egui::Color32::from_rgb(200, 30, 30), "Errors:");
                        egui::ScrollArea::vertical()
                            .max_height(180.0)
                            .show(ui, |ui| {
                                for e in &rep.errors {
                                    ui.label(format!("• {}", e));
                                }
                            });
                    }
                }
            });

        if close_clicked {
            self.batch_modal.open = false;
        }
    }

    /// Tabs an export reads: the ticked tabs for multi-tab exports, else the active tab.
    fn guarded_export_tabs(&self, kind: GuardedExport) -> Vec<usize> {
        match kind {
            GuardedExport::MultiXlsx | GuardedExport::MultiPdf => (0..self.tabs.len())
                .filter(|i| self.multi_export_checks.get(*i).copied().unwrap_or(false))
                .collect(),
            _ => vec![self.active_tab],
        }
    }

    /// True when the tab's control column has values that could not be converted.
    fn control_column_has_failures(tab: &TabState) -> bool {
        tab.control_col
            .as_ref()
            .is_some_and(|c| tab.parse_failures.iter().any(|f| &f.column == c))
    }

    /// Runs an export, or stops it for confirmation when a control column has parse failures.
    /// After "Export anyway", `<output>.parse_failures.csv` is written next to the export.
    fn run_guarded_export(&mut self, kind: GuardedExport) {
        let forced = self.export_override.take() == Some(kind);
        let tabs = self.guarded_export_tabs(kind);
        let blocked = tabs
            .iter()
            .filter_map(|i| self.tabs.get(*i))
            .any(Self::control_column_has_failures);
        if blocked && !forced {
            self.export_guard = Some(kind);
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

    fn write_failure_sidecar(&mut self, output: &Path, tabs: &[usize]) {
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

    fn render_export_guard(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.export_guard else {
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
            self.export_guard = None;
            self.export_override = Some(kind);
            self.run_guarded_export(kind);
        } else if cancel {
            self.export_guard = None;
            self.status_message = "Export cancelled: the control column has parse failures.".into();
        }
    }

    fn export_active_tab_parquet(&mut self) -> Option<PathBuf> {
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

    fn export_active_tab_csv(&mut self) -> Option<PathBuf> {
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

    fn export_multi_tab_xlsx(&mut self) -> Option<PathBuf> {
        let out_path = rfd::FileDialog::new()
            .set_file_name("Consolidated_ALCO_Views.xlsx")
            .save_file()?;
        let mut collected_tabs = Vec::new();
        let mut errors = Vec::new();
        for (i, tab) in self.tabs.iter().enumerate() {
            if self.multi_export_checks.get(i).copied().unwrap_or(false) {
                match compile_tab_lazy(&tab.source, &tab.steps, &self.paths.temp_dir)
                    .and_then(|lf| Ok(lf.collect()?))
                {
                    Ok(df) => collected_tabs.push((tab.title.clone(), df)),
                    Err(e) => errors.push((tab.title.clone(), format!("{:#}", e))),
                }
            }
        }
        match export_tabs_to_xlsx(collected_tabs, errors, &out_path) {
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

    fn export_multi_tab_pdf(&mut self) -> Option<PathBuf> {
        let out_path = rfd::FileDialog::new()
            .set_file_name("Consolidated_ALCO_Report.pdf")
            .save_file()?;
        let mut collected_tabs = Vec::new();
        let mut failed = Vec::new();
        for (i, tab) in self.tabs.iter().enumerate() {
            if self.multi_export_checks.get(i).copied().unwrap_or(false) {
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
    fn export_split_by_column(&mut self) -> Option<PathBuf> {
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
            &self.split_col_name,
            &out_dir,
            self.split_format,
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

    /// Reopens the schema modal for the active single-file tab, with its parse failures.
    fn open_schema_modal_for_active_tab(&mut self) {
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
    fn apply_schema_to_tab(&mut self, idx: usize, config: IngestionConfig) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_picker_default_ignores_type() {
        let cols = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(default_side_column(&cols(&["Account_Type", "Balance", "AL_Side"])), "AL_Side");
        assert_eq!(default_side_column(&cols(&["Collateral_Type", "Balance"])), "");
        assert_eq!(default_side_column(&cols(&["Direction", "Side"])), "Direction");
        assert_eq!(default_side_column(&cols(&["BalanceSide"])), "BalanceSide");
    }

    #[test]
    fn tab_indices_stay_valid_after_close() {
        // 4 tabs [0,1,2,3]; close tab 1 -> 3 tabs left
        assert_eq!(tab_index_after_close(0, 1, 3), 0, "before the closed tab: unchanged");
        assert_eq!(tab_index_after_close(2, 1, 3), 1, "after: same tab, shifted down");
        assert_eq!(tab_index_after_close(3, 1, 3), 2, "last tab stays last");
        assert_eq!(tab_index_after_close(1, 1, 3), 1, "closed tab: the tab that took its place");
        // Closing the selected last tab moves to the new last tab
        assert_eq!(tab_index_after_close(3, 3, 3), 2);
        // Stale index beyond the list is clamped
        assert_eq!(tab_index_after_close(9, 0, 3), 2);
        // Closing the only tab
        assert_eq!(tab_index_after_close(0, 0, 0), 0);
        // Exhaustive: every result is a valid index (or 0 when empty)
        for len in 1..6usize {
            for closed in 0..len {
                for sel in 0..len + 2 {
                    let r = tab_index_after_close(sel, closed, len - 1);
                    assert!(r < (len - 1).max(1), "sel {sel} closed {closed} len {len} -> {r}");
                }
            }
        }
    }
}
