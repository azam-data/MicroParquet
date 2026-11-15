use polars::prelude::DataFrame;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Automatically initializes the zero-registry portable directory next to the executable
#[derive(Clone, Debug)]
pub struct PortablePaths {
    pub root_dir: PathBuf,
    pub macros_dir: PathBuf,
    pub schemas_dir: PathBuf,
    pub curves_dir: PathBuf,
    pub buckets_file: PathBuf,
    pub temp_dir: PathBuf,
}

impl PortablePaths {
    pub fn init() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let root_dir = exe_dir.join("portable_data");
        let recipes_folder = root_dir.join("macros");
        let schema_folder = root_dir.join("schemas");
        let curve_folder = root_dir.join("curves");
        let temp_folder = root_dir.join("temp_scratch");
        let buckets_file = root_dir.join("buckets.json");

        let _ = std::fs::create_dir_all(&recipes_folder);
        let _ = std::fs::create_dir_all(&schema_folder);
        let _ = std::fs::create_dir_all(&curve_folder);
        let _ = std::fs::create_dir_all(&temp_folder);

        std::env::set_var("POLARS_TEMP_DIR", &temp_folder);

        Self {
            root_dir,
            macros_dir: recipes_folder,
            schemas_dir: schema_folder,
            curves_dir: curve_folder,
            buckets_file,
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
            ColDataType::DateMDY => "Date (M/D/YYYY or M-D-YYYY)",
            ColDataType::DateDMY => "Date (D/M/YYYY or D-M-YYYY)",
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
            ParquetDialect::ArchiveZstd => "Archive / Zstd (Smallest Size)",
            ParquetDialect::PowerBiSnappy => "Power BI Safe / Snappy (100% PBI)",
        }
    }
}

/// Global Monetary Scale for Balance/Amount columns
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
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
}

/// Per-Column & Per-Metric Display Format Type (v1.2)
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricFormatType {
    Auto,            // Smart-detects Monetary vs Rate (%) vs Duration/Count
    BdtCrore,        // Divide by 10,000,000 + " Cr"
    BdtLakh,         // Divide by 100,000 + " L"
    StandardCommas,  // 1,234.56
    Raw,             // Unformatted float
    PercentAsIs,     // e.g. 8.5 -> "8.50%"
    PercentDiv100,   // e.g. 0.085 -> "8.50%"
}

impl MetricFormatType {
    pub fn all() -> &'static [MetricFormatType] {
        &[
            MetricFormatType::Auto,
            MetricFormatType::BdtCrore,
            MetricFormatType::BdtLakh,
            MetricFormatType::StandardCommas,
            MetricFormatType::PercentAsIs,
            MetricFormatType::PercentDiv100,
            MetricFormatType::Raw,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            MetricFormatType::Auto => "Auto (Smart Detect)",
            MetricFormatType::BdtCrore => "BDT Crore (Cr)",
            MetricFormatType::BdtLakh => "BDT Lakh (L)",
            MetricFormatType::StandardCommas => "Decimal / Commas",
            MetricFormatType::PercentAsIs => "Percent (% as-is, e.g. 8.5%)",
            MetricFormatType::PercentDiv100 => "Percent (÷100 & %, e.g. 0.085 -> 8.5%)",
            MetricFormatType::Raw => "Raw Number",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnFormatSpec {
    pub fmt_type: MetricFormatType,
    pub decimals: usize,
}

impl Default for ColumnFormatSpec {
    fn default() -> Self {
        Self {
            fmt_type: MetricFormatType::Auto,
            decimals: 2,
        }
    }
}

/// Formats any f64 value respecting per-column format overrides, smart column name detection,
/// global monetary scale, and adjustable decimal precision (0..=6).
pub fn format_metric_value(
    val: f64,
    col_name: &str,
    col_spec: Option<&ColumnFormatSpec>,
    global_monetary_fmt: NumberDisplayFormat,
    global_decimals: usize,
) -> String {
    let (fmt_type, dec) = match col_spec {
        Some(spec) => (spec.fmt_type, spec.decimals),
        None => (MetricFormatType::Auto, global_decimals),
    };

    let resolved_type = if fmt_type == MetricFormatType::Auto {
        classify_column_auto_format(col_name)
    } else {
        fmt_type
    };

    match resolved_type {
        MetricFormatType::BdtCrore => {
            format!("{} Cr", format_with_commas(val / 10_000_000.0, dec))
        }
        MetricFormatType::BdtLakh => {
            format!("{} L", format_with_commas(val / 100_000.0, dec))
        }
        MetricFormatType::StandardCommas => format_with_commas(val, dec),
        MetricFormatType::PercentAsIs => format!("{}%", format_with_commas(val, dec)),
        MetricFormatType::PercentDiv100 => format!("{}%", format_with_commas(val * 100.0, dec)),
        MetricFormatType::Raw => format!("{}", val),
        MetricFormatType::Auto => match global_monetary_fmt {
            NumberDisplayFormat::Raw => format!("{}", val),
            NumberDisplayFormat::StandardCommas => format_with_commas(val, dec),
            NumberDisplayFormat::BdtLakh => {
                format!("{} L", format_with_commas(val / 100_000.0, dec))
            }
            NumberDisplayFormat::BdtCrore => {
                format!("{} Cr", format_with_commas(val / 10_000_000.0, dec))
            }
        },
    }
}

fn classify_column_auto_format(col_name: &str) -> MetricFormatType {
    let lower = col_name.to_lowercase();
    // Rate / Percentage columns should NEVER be divided by 10,000,000 (Crore)
    if lower.contains("rate")
        || lower.contains("pct")
        || lower.contains("percent")
        || lower.contains("yield")
        || lower.contains("spread")
        || lower.contains("util")
        || lower.contains("ratio")
        || lower.contains("wavg")
    {
        return MetricFormatType::PercentAsIs;
    }
    // Duration / Tenor / Days / Count columns should display as clean decimals/numbers
    if lower.contains("duration")
        || lower.contains("years")
        || lower.contains("tenor")
        || lower.contains("days")
        || lower.contains("count")
        || lower.contains("freq")
        || lower.contains("bps")
    {
        return MetricFormatType::StandardCommas;
    }
    // Everything else (Balance, Amount, BFP, Provision, NII, EVE, DV01, Limit) uses global monetary format
    MetricFormatType::Auto
}

pub fn format_with_commas(val: f64, decimals: usize) -> String {
    if val.is_nan() || val.is_infinite() {
        return "0.00".to_string();
    }
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

/// Custom Maturity Bucket Rule & Preset Definition (v1.2 — Saved to ./portable_data/buckets.json)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BucketRuleItem {
    pub max_days: i64,
    pub label: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CustomBucketDef {
    pub name: String,
    pub rules: Vec<BucketRuleItem>,
    pub overflow_label: String,
    pub overdue_label: String,
    pub null_sight_label: String,
    pub null_long_label: String,
}

impl CustomBucketDef {
    pub fn default_presets() -> Vec<CustomBucketDef> {
        vec![
            CustomBucketDef {
                name: "Bangladesh Bank SLP (9 Liquidity Buckets)".to_string(),
                rules: vec![
                    BucketRuleItem { max_days: 1, label: "01. Overnight / 1 Day".to_string() },
                    BucketRuleItem { max_days: 7, label: "02. 2 to 7 Days".to_string() },
                    BucketRuleItem { max_days: 30, label: "03. 8 to 30 Days".to_string() },
                    BucketRuleItem { max_days: 90, label: "04. 1 to 3 Months (31-90d)".to_string() },
                    BucketRuleItem { max_days: 180, label: "05. 3 to 6 Months (91-180d)".to_string() },
                    BucketRuleItem { max_days: 365, label: "06. 6 to 12 Months (181-365d)".to_string() },
                    BucketRuleItem { max_days: 1095, label: "07. 1 to 3 Years".to_string() },
                    BucketRuleItem { max_days: 1825, label: "08. 3 to 5 Years".to_string() },
                ],
                overflow_label: "09. Over 5 Years".to_string(),
                overdue_label: "10. Overdue / Matured Unencashed".to_string(),
                null_sight_label: "01. Overnight / Sight (Non-Maturing)".to_string(),
                null_long_label: "09. Over 5 Years (Core)".to_string(),
            },
            CustomBucketDef {
                name: "Basel III LCR / NSFR (5 Buckets)".to_string(),
                rules: vec![
                    BucketRuleItem { max_days: 30, label: "01. Within 30 Days (LCR Window)".to_string() },
                    BucketRuleItem { max_days: 90, label: "02. 31 to 90 Days".to_string() },
                    BucketRuleItem { max_days: 180, label: "03. 91 to 180 Days".to_string() },
                    BucketRuleItem { max_days: 365, label: "04. 181 to 365 Days".to_string() },
                ],
                overflow_label: "05. Over 1 Year (NSFR Stable)".to_string(),
                overdue_label: "06. Overdue / Past Maturity".to_string(),
                null_sight_label: "01. Sight / Open Maturity".to_string(),
                null_long_label: "05. Over 1 Year (Non-Maturing)".to_string(),
            },
        ]
    }

    pub fn load_all_presets(buckets_file: &Path) -> Vec<CustomBucketDef> {
        let mut presets = Self::default_presets();
        if let Ok(txt) = std::fs::read_to_string(buckets_file) {
            if let Ok(customs) = serde_json::from_str::<Vec<CustomBucketDef>>(&txt) {
                presets.extend(customs);
            }
        }
        presets
    }

    pub fn save_custom_preset(buckets_file: &Path, new_preset: CustomBucketDef) -> anyhow::Result<()> {
        let mut customs: Vec<CustomBucketDef> = std::fs::read_to_string(buckets_file)
            .ok()
            .and_then(|txt| serde_json::from_str(&txt).ok())
            .unwrap_or_default();

        customs.retain(|p| !p.name.eq_ignore_ascii_case(&new_preset.name));
        customs.push(new_preset);

        let json = serde_json::to_string_pretty(&customs)?;
        std::fs::write(buckets_file, json)?;
        Ok(())
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
    #[serde(default)]
    pub format_spec: ColumnFormatSpec,
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
        bucket_def: CustomBucketDef,
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
            TransformStep::MaturityBucket {
                new_col,
                bucket_def,
                ..
            } => {
                format!("+Bucket ({}): {}", bucket_def.name, new_col)
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
    pub col_formats: HashMap<String, ColumnFormatSpec>,
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
