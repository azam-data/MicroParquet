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

/// `portable_data/` next to the executable (falls back to the working directory).
pub fn portable_root_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    exe_dir.join("portable_data")
}

impl PortablePaths {
    pub fn init() -> Self {
        let root_dir = portable_root_dir();
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

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ColDataType {
    #[default]
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

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PiiMaskMode {
    #[default]
    None,
    ClientNameHash,
    AccountNumber,
}

impl PiiMaskMode {
    pub fn label(&self) -> &'static str {
        match self {
            PiiMaskMode::None => "No Masking",
            PiiMaskMode::ClientNameHash => "Mask Name (CLIENT_XXXX)",
            PiiMaskMode::AccountNumber => "Mask Acct (ACCT_XXXX)",
        }
    }
}

/// Which side of a Dr/Cr marked amount is positive. `Unset` refuses to load a column whose
/// values carry Dr/Cr, so the sign is never guessed.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SignConvention {
    #[default]
    Unset,
    DrPositive,
    CrPositive,
}

impl SignConvention {
    pub fn all() -> &'static [SignConvention] {
        &[
            SignConvention::Unset,
            SignConvention::DrPositive,
            SignConvention::CrPositive,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            SignConvention::Unset => "Dr/Cr: not set",
            SignConvention::DrPositive => "Dr is +",
            SignConvention::CrPositive => "Cr is +",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ColumnSchemaConfig {
    pub name: String,
    pub dtype: ColDataType,
    pub pii_mask: PiiMaskMode,
    pub sample_values: Vec<String>,
    /// Sign of Dr/Cr marked amounts in a Float64 column.
    #[serde(default)]
    pub sign_convention: SignConvention,
    /// Auto-detect saw Dr/Cr markers in this column's sample (the schema modal highlights it).
    #[serde(default)]
    pub has_dr_cr: bool,
    /// Parquet sources: the column's type in the file ("String", "Float64", "Date", ...).
    /// Empty for CSV.
    #[serde(default)]
    pub source_dtype: String,
}

/// The column type a Parquet column already has, from its `source_dtype` label; `None` for
/// types that cannot be converted here (Boolean, List, ...).
pub fn parquet_native_type(source_dtype: &str) -> Option<ColDataType> {
    match source_dtype {
        "String" => Some(ColDataType::Utf8),
        "Float32" | "Float64" | "Decimal" => Some(ColDataType::Float64),
        "Int8" | "Int16" | "Int32" | "Int64" | "UInt8" | "UInt16" | "UInt32" | "UInt64" => {
            Some(ColDataType::Int64)
        }
        "Date" | "Datetime" => Some(ColDataType::DateISO),
        _ => None,
    }
}

/// True when a Parquet column must be read as text and converted: its type was changed from
/// the file's own, or it has a PII mask. A type with no counterpart here (Boolean, List...)
/// shown as Text counts as unchanged.
pub fn parquet_needs_conversion(c: &ColumnSchemaConfig) -> bool {
    if c.pii_mask != PiiMaskMode::None {
        return true;
    }
    match parquet_native_type(&c.source_dtype) {
        Some(native) => native != c.dtype,
        None => c.dtype != ColDataType::Utf8,
    }
}

/// Non-blank values of one typed column that could not be converted and became null.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParseFailure {
    pub column: String,
    pub count: usize,
    /// Up to 5 raw values, as read.
    pub examples: Vec<String>,
}

/// Text encoding of a CSV file.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CsvTextEncoding {
    #[default]
    Utf8,
    Windows1252,
}

impl CsvTextEncoding {
    pub fn label(&self) -> &'static str {
        match self {
            CsvTextEncoding::Utf8 => "UTF-8",
            CsvTextEncoding::Windows1252 => "Windows-1252 (ANSI)",
        }
    }
}

pub fn default_csv_separator() -> char {
    ','
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IngestionConfig {
    pub columns: Vec<ColumnSchemaConfig>,
    pub sanitize_linebreaks: bool,
    pub parse_accounting_negatives: bool,
    pub heal_broken_csv_lines: bool,
    pub pii_salt: String,
    /// Column separator: ',', '|', ';' or tab.
    #[serde(default = "default_csv_separator")]
    pub separator: char,
    /// Title lines above the header row.
    #[serde(default)]
    pub skip_rows: usize,
    #[serde(default)]
    pub encoding: CsvTextEncoding,
}

impl Default for IngestionConfig {
    fn default() -> Self {
        Self {
            columns: Vec::new(),
            sanitize_linebreaks: true,
            parse_accounting_negatives: true,
            heal_broken_csv_lines: false,
            pii_salt: "BANK_ASIA_ALM_2026".to_string(),
            separator: default_csv_separator(),
            skip_rows: 0,
            encoding: CsvTextEncoding::Utf8,
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
    // Whole-token matches only: "Utility_Bill_Amount" is money, not utilisation
    const BPS_TOKENS: &[&str] = &["bps", "bp"];
    const PLAIN_NUMBER_TOKENS: &[&str] = &[
        "duration", "years", "yrs", "tenor", "days", "count", "cnt", "num", "nos", "freq",
    ];
    const PERCENT_TOKENS: &[&str] = &[
        "rate", "pct", "percent", "yield", "spread", "util", "utilization", "utilisation", "ratio",
        "wavg",
    ];
    let tokens = column_name_tokens(col_name);
    let has_any = |set: &[&str]| tokens.iter().any(|t| set.contains(&t.as_str()));

    // 1. Basis points are plain numbers, even when the name also says "rate"
    if has_any(BPS_TOKENS) {
        return MetricFormatType::StandardCommas;
    }
    // 2. Duration / tenor / day / count columns display as plain numbers
    if has_any(PLAIN_NUMBER_TOKENS)
        || col_name.to_lowercase().starts_with("no_of")
        || tokens.starts_with(&["no".to_string(), "of".to_string()])
    {
        return MetricFormatType::StandardCommas;
    }
    // 3. Rates are stored in percent and must never be divided into Crore
    if has_any(PERCENT_TOKENS) {
        return MetricFormatType::PercentAsIs;
    }
    // 4. Everything else (Balance, Amount, BFP, Provision, NII, EVE, DV01, Limit) is money
    MetricFormatType::Auto
}

/// Splits a column name into lowercase tokens on `_`, space, `-`, `.` and camelCase
/// boundaries: "BranchCode" -> ["branch", "code"], "CIFNumber" -> ["cif", "number"].
pub fn column_name_tokens(col_name: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for part in col_name.split(['_', ' ', '-', '.']) {
        let chars: Vec<char> = part.chars().collect();
        let mut cur = String::new();
        for (i, &c) in chars.iter().enumerate() {
            if c.is_uppercase() && !cur.is_empty() {
                let prev = chars.get(i.wrapping_sub(1)).copied().unwrap_or(' ');
                let next_is_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
                if prev.is_lowercase() || prev.is_ascii_digit() || (prev.is_uppercase() && next_is_lower) {
                    tokens.push(cur.to_lowercase());
                    cur.clear();
                }
            }
            cur.push(c);
        }
        if !cur.is_empty() {
            tokens.push(cur.to_lowercase());
        }
    }
    tokens
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

/// How a tenor between two curve points gets its FTP rate.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FtpInterpolation {
    /// Straight line on days between the two surrounding points (flat outside the curve).
    #[default]
    Linear,
    /// The rate of the next tenor point at or above the deal's tenor (Phase 1 behaviour).
    NextPointUp,
}

impl FtpInterpolation {
    pub fn label(&self) -> &'static str {
        match self {
            FtpInterpolation::Linear => "Linear between tenor points",
            FtpInterpolation::NextPointUp => "Next tenor point up (step)",
        }
    }
}

/// Hygiene audit: status codes that mean "active" by default.
pub fn default_active_statuses() -> Vec<String> {
    ["ACT", "ACTIVE", "A", "OPEN"].iter().map(|s| s.to_string()).collect()
}

/// Hygiene audit: rates above this (percent) are outliers by default.
pub fn default_rate_max_pct() -> f64 {
    25.0
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
        /// Asset/liability side column; None = old macro, treated as asset (flagged in output)
        #[serde(default)]
        side_col: Option<String>,
    },
    FtpCurveMap {
        opndat_col: String,
        matdat_col: String,
        bal_col: String,
        rate_col: String,
        curve: Vec<FtpPoint>,
        is_liability: bool,
        /// Old macros load as Linear (the Phase 2 default).
        #[serde(default)]
        interpolation: FtpInterpolation,
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
        /// Status codes that mean "active", matched trimmed and case-insensitive.
        #[serde(default = "default_active_statuses")]
        active_statuses: Vec<String>,
        /// Rates outside [rate_min_pct, rate_max_pct] are flagged as outliers.
        #[serde(default)]
        rate_min_pct: f64,
        #[serde(default = "default_rate_max_pct")]
        rate_max_pct: f64,
    },
    DurationAnalysis {
        bal_col: String,
        rate_col: String,
        mat_col: String,
        cutoff_date: String,
        compounding_freq: usize,
        shock_bps: f64,
        default_sight_years: f64,
        /// Asset/liability side column; None = old macro, treated as asset (flagged in output)
        #[serde(default)]
        side_col: Option<String>,
    },
    LoanClassificationProvision {
        bal_col: String,
        cl_status_col: String,
        segment_col: Option<String>,
        suspense_col: Option<String>,
        security_col: Option<String>,
        /// YYYY-MM-DD; empty = today (flagged in a Cutoff_Assumed column)
        #[serde(default)]
        cutoff_date: String,
        /// Collateral type column; DEPOSIT_LIEN etc. skip the 15% BFP floor
        #[serde(default)]
        collateral_type_col: Option<String>,
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
        /// Keep the first right-side row per key instead of failing on duplicated keys.
        /// (TabSource is not serialised, so there is no serde attribute to add.)
        keep_first_duplicate_right: bool,
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
    /// Values of the tab's source that could not be converted to their column type.
    pub parse_failures: Vec<ParseFailure>,
    /// For a joined tab: rows and control sum against the left table, and dropped duplicates.
    pub join_status: Option<JoinStatus>,
    /// What the status bar shows for rows and control sum (None = "—", with the reason).
    pub totals: ControlTotals,
    /// CSV sources: rows read against the file's data lines (status-bar warning when short).
    pub row_check: Option<RowCountCheck>,
}

/// Rows read from a CSV against its non-empty lines (minus the header and title lines).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowCountCheck {
    pub nonempty_lines: usize,
    pub skip_rows: usize,
    pub parsed_rows: usize,
}

impl RowCountCheck {
    /// Non-empty lines minus the header line and the title lines above it.
    pub fn data_lines(&self) -> usize {
        self.nonempty_lines.saturating_sub(1 + self.skip_rows)
    }

    /// A warning when fewer rows were read than the file has data lines, or none at all from a
    /// non-empty file. Not an error: quoted values may legitimately span lines.
    pub fn warning(&self) -> Option<String> {
        if self.nonempty_lines > 0 && self.parsed_rows == 0 {
            return Some(format!(
                "0 rows read from a file with {} non-empty lines ({} data lines). Check the separator, title lines and encoding.",
                self.nonempty_lines,
                self.data_lines()
            ));
        }
        if self.parsed_rows < self.data_lines() {
            return Some(format!(
                "Read {} rows but the file has {} data lines ({} non-empty lines − 1 header − {} title lines). Check for unclosed quotes or broken lines.",
                self.parsed_rows,
                self.data_lines(),
                self.nonempty_lines,
                self.skip_rows
            ));
        }
        None
    }
}

/// Rows and control sum for the status bar. `None` is shown as "—"; `note` says why.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ControlTotals {
    pub rows: Option<usize>,
    pub sum: Option<f64>,
    pub note: Option<String>,
}

/// Rows and control sum of a join next to the left table's, for the status bar.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JoinStatus {
    pub left_rows: usize,
    pub left_sum: Option<f64>,
    pub rows: usize,
    pub sum: Option<f64>,
    /// Key combinations that appeared more than once on the right side ("Keep first row per key").
    pub duplicate_keys: usize,
    /// Right-side rows dropped by "Keep first row per key".
    pub dropped_rows: usize,
}

impl JoinStatus {
    pub fn summary(&self) -> String {
        let fmt_sum = |s: Option<f64>| match s {
            Some(v) => format!("{:.2} Cr", v / 10_000_000.0),
            None => "—".to_string(),
        };
        let mut out = format!(
            "Join: {} rows (left {}), control {} (left {})",
            self.rows,
            self.left_rows,
            fmt_sum(self.sum),
            fmt_sum(self.left_sum)
        );
        if self.duplicate_keys > 0 {
            out.push_str(&format!(
                " ⚠ kept first row for {} duplicated right key(s), {} right row(s) dropped",
                self.duplicate_keys, self.dropped_rows
            ));
        }
        out
    }
}

impl TabState {
    /// A column is an ID (kept as text) when one whole token is an ID word and no token is a
    /// money, rate or date word. "Account_Balance" is a balance, "Customer_ID" is an ID.
    pub fn is_likely_id_column(col_name: &str) -> bool {
        const ID_TOKENS: &[&str] = &[
            "brancd", "branch", "actnum", "account", "acct", "glcode", "gl", "code", "cif", "id",
            "tin", "nid", "phone", "mobile", "routing",
        ];
        const VALUE_TOKENS: &[&str] = &[
            "bal", "balance", "amt", "amount", "outstanding", "limit", "sum", "total", "rate",
            "int", "interest", "prov", "provision", "value", "val", "date", "dt", "maturity",
            "profit",
        ];
        let tokens = column_name_tokens(col_name);
        tokens.iter().any(|t| ID_TOKENS.contains(&t.as_str()))
            && !tokens.iter().any(|t| VALUE_TOKENS.contains(&t.as_str()))
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
