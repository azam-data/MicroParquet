use polars::prelude::DataFrame;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Automatically initializes the zero-registry portable directory next to the executable
#[derive(Clone, Debug)]
pub struct PortablePaths {
    pub root_dir: PathBuf,
    pub macros_dir: PathBuf,
    pub schemas_dir: PathBuf,
    pub curves_dir: PathBuf,
    pub temp_dir: PathBuf,
}

impl PortablePaths {
    pub fn init() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let root_dir = exe_dir.join("portable_data");
        let macro_folder = root_dir.join("macros");
        let schema_folder = root_dir.join("schemas");
        let curve_folder = root_dir.join("curves");
        let temp_folder = root_dir.join("temp_scratch");

        let _ = std::fs::create_dir_all(¯o_folder);
        let _ = std::fs::create_dir_all(&schema_folder);
        let _ = std::fs::create_dir_all(&curve_folder);
        let _ = std::fs::create_dir_all(&temp_folder);

        std::env::set_var("POLARS_TEMP_DIR", &temp_folder);

        Self {
            root_dir,
            macros_dir: macro_folder,
            schemas_dir: schema_folder,
            curves_dir: curve_folder,
            temp_dir: temp_folder,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColDataType {
    Utf8,
    Float64,
    Int64,
    DateMDY,
    DateDMY,
    DateISO,
}

impl ColDataType {
    pub fn all() -> &'static [ColDataType] {
        &[
            ColDataType::Utf8,
            ColDataType::Float64,
            ColDataType::Int64,
            ColDataType::DateMDY,
            ColDataType::DateDMY,
            ColDataType::DateISO,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            ColDataType::Utf8 => "Text / ID (Utf8 - Keeps Zeros)",
            ColDataType::Float64 => "Decimal / Balance (Float64)",
            ColDataType::Int64 => "Whole Integer (Int64)",
            ColDataType::DateMDY => "Date (M/D/YYYY)",
            ColDataType::DateDMY => "Date (D/M/YYYY)",
            ColDataType::DateISO => "Date (YYYY-MM-DD)",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PiiMaskMode {
    None,
    ClientNameHash,
    AccountNumber,
}

impl PiiMaskMode {
    pub fn label(&self) -> &'static str {
        match self {
            PiiMaskMode::None => "No Masking",
            PiiMaskMode::ClientNameHash => "Mask Name (CLIENT_XXXX)",
            PiiMaskMode::AccountNumber => "Mask Acct (629***042)",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ColumnSchemaConfig {
    pub name: String,
    pub dtype: ColDataType,
    pub pii_mask: PiiMaskMode,
    pub sample_values: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IngestionConfig {
    pub columns: Vec<ColumnSchemaConfig>,
    pub sanitize_linebreaks: bool,
    pub parse_accounting_negatives: bool,
    pub heal_broken_csv_lines: bool,
    pub pii_salt: String,
}

impl Default for IngestionConfig {
    fn default() -> Self {
        Self {
            columns: Vec::new(),
            sanitize_linebreaks: true,
            parse_accounting_negatives: true,
            heal_broken_csv_lines: false,
            pii_salt: "BANK_ASIA_ALM_2026".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParquetDialect {
    ArchiveZstd,
    PowerBiSnappy,
}

impl ParquetDialect {
    pub fn label(&self) -> &'static str {
        match self {
            ParquetDialect::ArchiveZstd => "Archive / Zstd (Smallest Size - Python/DuckDB)",
            ParquetDialect::PowerBiSnappy => "Power BI Safe / Snappy (100% PBI Compatible)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberDisplayFormat {
    Raw,
    StandardCommas,
    BdtLakh,
    BdtCrore,
}

impl NumberDisplayFormat {
    pub fn label(&self) -> &'static str {
        match self {
            NumberDisplayFormat::Raw => "Raw Numbers",
            NumberDisplayFormat::StandardCommas => "Commas (1,234.56)",
            NumberDisplayFormat::BdtLakh => "BDT Lakh (÷ 100,000)",
            NumberDisplayFormat::BdtCrore => "BDT Crore (÷ 10,000,000)",
        }
    }

    pub fn format_f64(&self, val: f64) -> String {
        match self {
            NumberDisplayFormat::Raw => format!("{}", val),
            NumberDisplayFormat::StandardCommas => format_with_commas(val, 2),
            NumberDisplayFormat::BdtLakh => format!("{} L", format_with_commas(val / 100_000.0, 2)),
            NumberDisplayFormat::BdtCrore => {
                format!("{} Cr", format_with_commas(val / 10_000_000.0, 2))
            }
        }
    }
}

pub fn format_with_commas(val: f64, decimals: usize) -> String {
    let sign = if val < 0.0 { "-" } else { "" };
    let abs_val = val.abs();
    let formatted = format!("{:.*}", decimals, abs_val);
    let parts: Vec<&str> = formatted.split('.').collect();
    let int_part = parts[0];
    let dec_part = parts.get(1).copied().unwrap_or("");

    let mut result = String::new();
    let chars: Vec<char> = int_part.chars().collect();
    let len = chars.len();
    for (i, c) in chars.iter().enumerate() {
        result.push(*c);
        let pos_from_right = len - 1 - i;
        if pos_from_right > 0 && pos_from_right % 3 == 0 {
            result.push(',');
        }
    }
    if decimals > 0 && !dec_part.is_empty() {
        format!("{}{}.{}", sign, result, dec_part)
    } else {
        format!("{}{}", sign, result)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchDirection {
    CsvToParquet,
    ParquetToCsv,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchSchemaStrategy {
    SmartSafeAutoDetect,
    MasterSchemaTemplate,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterOp {
    Eq,
    NotEq,
    Gt,
    Gte,
    Lt,
    Lte,
    Contains,
    StartsWith,
    IsNull,
    IsNotNull,
    InList,
}

impl FilterOp {
    pub fn all() -> &'static [FilterOp] {
        &[
            FilterOp::Eq,
            FilterOp::NotEq,
            FilterOp::Gt,
            FilterOp::Gte,
            FilterOp::Lt,
            FilterOp::Lte,
            FilterOp::Contains,
            FilterOp::StartsWith,
            FilterOp::IsNull,
            FilterOp::IsNotNull,
            FilterOp::InList,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            FilterOp::Eq => "== (Equals)",
            FilterOp::NotEq => "!= (Not Equals)",
            FilterOp::Gt => "> (Greater Than)",
            FilterOp::Gte => ">= (Greater or Equal)",
            FilterOp::Lt => "< (Less Than)",
            FilterOp::Lte => "<= (Less or Equal)",
            FilterOp::Contains => "Contains Text",
            FilterOp::StartsWith => "Starts With",
            FilterOp::IsNull => "Is Null / Blank",
            FilterOp::IsNotNull => "Is Not Null",
            FilterOp::InList => "In List (a, b, c)",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CondRule {
    pub col_name: String,
    pub op: FilterOp,
    pub compare_val: String,
    pub output_val: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl MathOp {
    pub fn symbol(&self) -> &'static str {
        match self {
            MathOp::Add => "+",
            MathOp::Sub => "-",
            MathOp::Mul => "*",
            MathOp::Div => "/",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BucketPreset {
    BangladeshBankSlp,
    BaselLcrNsfr,
}

impl BucketPreset {
    pub fn label(&self) -> &'static str {
        match self {
            BucketPreset::BangladeshBankSlp => "Bangladesh Bank SLP (9 Liquidity Buckets)",
            BucketPreset::BaselLcrNsfr => "Basel III LCR / NSFR (5 Buckets)",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggFunc {
    Sum,
    Count,
    Mean,
    Min,
    Max,
    WeightedAvg,
    PctOfTotal,
}

impl AggFunc {
    pub fn all() -> &'static [AggFunc] {
        &[
            AggFunc::Sum,
            AggFunc::Count,
            AggFunc::Mean,
            AggFunc::Min,
            AggFunc::Max,
            AggFunc::WeightedAvg,
            AggFunc::PctOfTotal,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            AggFunc::Sum => "Sum",
            AggFunc::Count => "Count",
            AggFunc::Mean => "Simple Average",
            AggFunc::Min => "Minimum",
            AggFunc::Max => "Maximum",
            AggFunc::WeightedAvg => "Weighted Average (WAVG)",
            AggFunc::PctOfTotal => "% of Grand Total",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AggSpec {
    pub target_col: String,
    pub func: AggFunc,
    pub weight_col: Option<String>,
    pub alias: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FtpPoint {
    pub tenor_days: i64,
    pub ftp_rate: f64,
}

impl Default for FtpPoint {
    fn default() -> Self {
        Self {
            tenor_days: 30,
            ftp_rate: 8.5,
        }
    }
}

pub fn default_bd_ftp_curve() -> Vec<FtpPoint> {
    vec![
        FtpPoint { tenor_days: 1, ftp_rate: 8.00 },
        FtpPoint { tenor_days: 7, ftp_rate: 8.25 },
        FtpPoint { tenor_days: 30, ftp_rate: 8.75 },
        FtpPoint { tenor_days: 91, ftp_rate: 9.50 },
        FtpPoint { tenor_days: 182, ftp_rate: 10.10 },
        FtpPoint { tenor_days: 364, ftp_rate: 10.65 },
        FtpPoint { tenor_days: 730, ftp_rate: 11.20 },
        FtpPoint { tenor_days: 1825, ftp_rate: 11.75 },
    ]
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlJoinKind {
    Left,
    Inner,
    Full,
    Anti,
    Semi,
}

impl SqlJoinKind {
    pub fn label(&self) -> &'static str {
        match self {
            SqlJoinKind::Left => "LEFT JOIN (All Left + Matching Right)",
            SqlJoinKind::Inner => "INNER JOIN (Matching Rows Only)",
            SqlJoinKind::Full => "FULL OUTER JOIN (All Rows Both Sides)",
            SqlJoinKind::Anti => "ANTI JOIN (Rows in Left MISSING in Right)",
            SqlJoinKind::Semi => "SEMI JOIN (Rows in Left PRESENT in Right)",
        }
    }
}

/// Serializable transformation step stored in a Tab's Applied Steps & .mpq.json Macros
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum TransformStep {
    SelectAndReorderColumns {
        cols: Vec<String>,
    },
    RenameColumn {
        old_name: String,
        new_name: String,
    },
    FilterRows {
        column: String,
        operator: FilterOp,
        value: String,
        keep_matching: bool,
    },
    ConditionalColumn {
        new_col: String,
        rules: Vec<CondRule>,
        fallback: String,
        output_as_float: bool,
    },
    MathFormula {
        new_col: String,
        left_col: String,
        op: MathOp,
        right_is_col: bool,
        right_val_or_col: String,
    },
    NormalizeEntity {
        source_col: String,
        new_col: String,
    },
    MaturityBucket {
        mat_col: String,
        cutoff_date: String,
        preset: BucketPreset,
        null_as_sight: bool,
        new_col: String,
    },
    RateShockSim {
        bal_col: String,
        rate_col: String,
        mat_col: String,
        cutoff_date: String,
        shock_bps: f64,
    },
    FtpCurveMap {
        opndat_col: String,
        matdat_col: String,
        bal_col: String,
        rate_col: String,
        curve: Vec<FtpPoint>,
        is_liability: bool,
    },
    BankingHygieneAudit {
        opndat_col: String,
        matdat_col: String,
        clsdat_col: String,
        acstat_col: String,
        bal_col: String,
        rate_col: String,
        actnum_col: String,
        cutoff_date: String,
        only_exceptions: bool,
    },
    DurationAnalysis {
        bal_col: String,
        rate_col: String,
        mat_col: String,
        cutoff_date: String,
        compounding_freq: usize,
        shock_bps: f64,
        default_sight_years: f64,
    },
    LoanClassificationProvision {
        bal_col: String,
        cl_status_col: String,
        segment_col: Option<String>,
        suspense_col: Option<String>,
        security_col: Option<String>,
    },
    LoanLimitUtilization {
        limit_col: String,
        bal_col: String,
        ccf_pct: f64,
    },
    GroupByPivot {
        group_cols: Vec<String>,
        pivot_col: Option<String>,
        aggs: Vec<AggSpec>,
        sort_by_col: Option<String>,
        top_n: Option<usize>,
    },
    SqlQuery {
        query: String,
    },
}

impl TransformStep {
    pub fn short_label(&self) -> String {
        match self {
            TransformStep::SelectAndReorderColumns { cols } => {
                format!("Select/Order {} Cols", cols.len())
            }
            TransformStep::RenameColumn { old_name, new_name } => {
                format!("Rename {} -> {}", old_name, new_name)
            }
            TransformStep::FilterRows {
                column,
                operator,
                value,
                keep_matching,
            } => {
                let mode = if *keep_matching { "Keep" } else { "Delete" };
                format!("{}: {} {:?} '{}'", mode, column, operator, value)
            }
            TransformStep::ConditionalColumn { new_col, .. } => {
                format!("+Cond Col: {}", new_col)
            }
            TransformStep::MathFormula {
                new_col,
                left_col,
                op,
                right_val_or_col,
                ..
            } => format!(
                "+Math: {} = {} {} {}",
                new_col,
                left_col,
                op.symbol(),
                right_val_or_col
            ),
            TransformStep::NormalizeEntity { new_col, .. } => {
                format!("+Normalize Entity: {}", new_col)
            }
            TransformStep::MaturityBucket { new_col, .. } => {
                format!("+ALM Bucket: {}", new_col)
            }
            TransformStep::RateShockSim { shock_bps, .. } => {
                format!("+ΔNII Shock ({:+.0} bps)", shock_bps)
            }
            TransformStep::FtpCurveMap { .. } => "+FTP Yield Curve Spread".to_string(),
            TransformStep::BankingHygieneAudit {
                only_exceptions, ..
            } => {
                if *only_exceptions {
                    "Audit Exceptions Only".to_string()
                } else {
                    "+Banking Hygiene Flags".to_string()
                }
            }
            TransformStep::DurationAnalysis { shock_bps, .. } => {
                format!("+Duration & ΔEVE ({:+.0} bps)", shock_bps)
            }
            TransformStep::LoanClassificationProvision { .. } => {
                "+BB CL Provision & BFP".to_string()
            }
            TransformStep::LoanLimitUtilization { .. } => {
                "+Limit Util & Undrawn CCF".to_string()
            }
            TransformStep::GroupByPivot { group_cols, .. } => {
                format!("Group/Pivot by {:?}", group_cols)
            }
            TransformStep::SqlQuery { .. } => "SQL Query Transform".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum TabSource {
    SingleFile {
        path: PathBuf,
        is_parquet: bool,
        config: IngestionConfig,
    },
    VerticalStack {
        files: Vec<(PathBuf, bool, IngestionConfig)>,
        include_source_col: bool,
    },
    JoinedTabs {
        left_source: Box<TabSource>,
        left_steps: Vec<TransformStep>,
        right_source: Box<TabSource>,
        right_steps: Vec<TransformStep>,
        join_kind: SqlJoinKind,
        join_keys: Vec<String>,
        right_cols_to_keep: Vec<String>,
    },
    PeriodReconcile {
        prev_source: Box<TabSource>,
        prev_steps: Vec<TransformStep>,
        curr_source: Box<TabSource>,
        curr_steps: Vec<TransformStep>,
        key_col: String,
        bal_col: String,
        rate_col: Option<String>,
        cl_status_col: Option<String>,
    },
}

#[derive(Clone)]
pub struct TabState {
    pub title: String,
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    pub total_rows: usize,
    pub control_col: Option<String>,
    pub control_sum: Option<f64>,
    pub baseline_control_sum: Option<f64>,
    pub viewport_offset: usize,
    pub viewport_limit: usize,
    pub cached_slice: DataFrame,
    pub sql_scratchpad: String,
}

impl TabState {
    pub fn is_likely_id_column(col_name: &str) -> bool {
        let lower = col_name.to_lowercase();
        let keywords = [
            "brancd",
            "branch",
            "actnum",
            "account",
            "glcode",
            "gl_code",
            "sector code",
            "id",
            "code",
            "cif",
            "tin",
            "nid",
            "phone",
            "mobile",
            "routing",
        ];
        keywords.iter().any(|k| lower.contains(k))
    }

    pub fn available_columns(&self) -> Vec<String> {
        self.cached_slice
            .get_columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MacroRecipe {
    pub name: String,
    pub description: String,
    pub created_at: String,
    pub enforce_balance_control: bool,
    pub control_column: Option<String>,
    pub optional_ingestion_schema: Option<IngestionConfig>,
    pub steps: Vec<TransformStep>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitExportFormat {
    Parquet,
    Csv,
    Xlsx,
}

impl SplitExportFormat {
    pub fn label(&self) -> &'static str {
        match self {
            SplitExportFormat::Parquet => "Parquet Files (.parquet)",
            SplitExportFormat::Csv => "CSV Files (.csv)",
            SplitExportFormat::Xlsx => "Excel Workbooks (.xlsx)",
        }
    }
}
