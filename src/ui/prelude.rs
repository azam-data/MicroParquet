//! Names shared by every UI module, so each screen file starts with one `use` line.

pub use eframe::egui;
pub use egui_extras::{Column, TableBuilder};
pub use egui_plot::{Bar, BarChart, Plot};
pub use micro_parquet::batch::BatchReport;
pub use micro_parquet::engine::{
    adapt_schema_to_parquet, auto_detect_parquet_schema, auto_detect_smart_schema,
    auto_detect_smart_schema_with, blank_balance_notes, collect_masked_columns, compile_tab_lazy,
    detect_file_schema, ensure_pii_key_for, export_pii_key,
    import_pii_key, parse_failure_summary, pii_key_fingerprint, pii_key_path,
    preview_csv_raw, preview_csv_with, preview_parquet_raw, read_pii_key, source_masks_pii,
    source_parse_failures, source_row_check, stack_file_configs, take_pii_key_created_note,
    weighted_mixed_sign_notes,
};
pub use micro_parquet::exporters::{
    export_tabs_to_pdf, export_tabs_to_xlsx, format_df_for_clipboard,
};
pub use micro_parquet::macros::{
    generate_python_polars_code, load_macro_from_file, python_pii_refusal,
    python_untranslated_steps, save_macro_to_file,
};
pub use micro_parquet::models::{
    PivotAgg, PivotFilter, PivotScale, PivotShowAs, PivotSortByValue, PivotSpec, PivotValue,
    column_name_tokens, default_active_statuses, default_bd_ftp_curve, default_rate_max_pct,
    format_metric_value, format_with_commas, resolved_scale, sort_marks, parquet_native_type, AggFunc, AggSpec, BatchDirection, BlankRule, RateBandDef, RateBandMode, fmt_pct, parse_rate_edges,
    BatchSchemaStrategy, BucketRuleItem, ColDataType, ColumnFormatSpec, CondRule, CsvTextEncoding,
    CondOp, CustomBucketDef, FilterCondition, FilterOp, FtpInterpolation, GroupJoin, SortLevel, SummaryAgg, SummarySpec, FtpPoint, IngestionConfig, MacroRecipe, MathOp,
    MetricFormatType, NumberDisplayFormat, ParquetDialect, ParseFailure, PiiMaskMode, PortablePaths,
    SignConvention, SplitExportFormat, SqlJoinKind, TabSource, TabState, TransformStep,
};
pub use micro_parquet::jobs::{JobKey, JobKind, JobQueue, Outcome};
pub use micro_parquet::tab_jobs::{
    check_schema_for_tab, check_text_columns, compute_step_counts, compute_tab_column_stats,
    compute_tab_summary, ColumnStatsRequest, stats_key, convert_csv_to_parquet, export_split, export_tab_csv, export_tab_parquet,
    export_tabs_pdf, export_tabs_xlsx, load_tab, refresh_tab, ExportTab, JobOutput, LoadRequest,
    LoadedTab, RefreshRequest, SchemaApplied, SchemaRequest, TabTextFindings, TextCheckRequest,
    TextCheckResult, StepCountsRequest, SummaryRequest, compute_pivot_preview, compute_pivot_values, FieldKind,
    PivotPreviewRequest, PivotValuesRequest,
};
pub use polars::datatypes::AnyValue;
pub use polars::prelude::DataFrame;
pub use std::collections::HashMap;
pub use std::path::{Path, PathBuf};

pub use crate::ui::audit::AuditState;
pub use crate::ui::batch::BatchModalState;
pub use crate::ui::buckets::BucketState;
pub use crate::ui::rate_band::RateBandState;
pub use crate::ui::calc_column::CalcState;
pub use crate::ui::chart::ChartState;
pub use crate::ui::cl_migration::ClMigrationState;
pub use crate::ui::columns::ColumnsState;
pub use crate::ui::conditional::CondState;
pub use crate::ui::duration::DurationState;
pub use crate::ui::export::{ExportState, GuardedExport};
pub use crate::ui::filter::FilterState;
pub use crate::ui::ftp::FtpState;
pub use crate::ui::jobs::PendingRefresh;
pub use crate::ui::join::JoinState;
pub use crate::ui::limits::LimitState;
pub use crate::ui::normaliser::NormState;
pub use crate::ui::pivot::PivotState;
pub use crate::ui::pivot_builder::PivotBuilderState;
pub use crate::ui::provision::ProvisionState;
pub use crate::ui::rate_shock::ShockState;
pub use crate::ui::reconcile::ReconcileState;
pub use crate::ui::schema::PendingCsvModal;
pub use crate::ui::sort::SortState;
pub use crate::ui::stack::StackState;
pub use crate::ui::steps::{MacroState, RightTab};
pub use crate::ui::theme;
pub use crate::ui::ModalOutcome;
pub use crate::{ActiveToolModal, MicroParquetApp};
