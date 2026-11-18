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

/// The data folder (`portable_data`): next to the executable, or on macOS next to the `.app`
/// bundle (with `~/Library/Application Support/MicroParquet` as the fallback). Falls back to
/// the working directory when the executable path is unknown. See `paths::choose_portable_root`.
pub fn portable_root_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join("MicroParquet")
    });
    let home = std::env::var_os("HOME").map(PathBuf::from);
    crate::paths::choose_portable_root(
        &exe,
        cfg!(target_os = "macos"),
        home.as_deref(),
        &crate::paths::dir_is_writable,
    )
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
    /// Set when the Schema & Ingestion box made the recorded "Clean line breaks" step: masked text
    /// columns are still trimmed before they are masked, so a mask is the same as it was before
    /// Phase 5. Old files do not have it.
    #[serde(default)]
    pub trim_before_mask: bool,
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
            trim_before_mask: false,
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
    BdtThousand,
    BdtMillion,
}

/// A money scale: the value is divided by `divisor` and shown with `suffix`
/// ("2.50 Cr"); reports name it in the header with `unit` ("Balance (BDT Cr)").
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoneyScale {
    pub divisor: f64,
    pub suffix: &'static str,
    pub unit: &'static str,
}

pub const SCALE_THOUSAND: MoneyScale = MoneyScale { divisor: 1_000.0, suffix: "K", unit: "BDT K" };
pub const SCALE_MILLION: MoneyScale = MoneyScale { divisor: 1_000_000.0, suffix: "M", unit: "BDT M" };
pub const SCALE_LAKH: MoneyScale = MoneyScale { divisor: 100_000.0, suffix: "L", unit: "BDT L" };
pub const SCALE_CRORE: MoneyScale = MoneyScale { divisor: 10_000_000.0, suffix: "Cr", unit: "BDT Cr" };

impl MoneyScale {
    /// `value` divided by the scale, with thousands separators and the suffix: "25.00 M".
    pub fn format(&self, value: f64, decimals: usize) -> String {
        format!("{} {}", format_with_commas(value / self.divisor, decimals), self.suffix)
    }
}

impl NumberDisplayFormat {
    pub fn label(&self) -> &'static str {
        match self {
            NumberDisplayFormat::Raw => "Raw Numbers",
            NumberDisplayFormat::StandardCommas => "Commas (1,234.56)",
            NumberDisplayFormat::BdtThousand => "BDT Thousand (÷ 1,000)",
            NumberDisplayFormat::BdtMillion => "BDT Million (÷ 1,000,000)",
            NumberDisplayFormat::BdtLakh => "BDT Lakh (÷ 100,000)",
            NumberDisplayFormat::BdtCrore => "BDT Crore (÷ 10,000,000)",
        }
    }

    /// The scale this format divides by, or `None` for plain numbers.
    pub fn scale(&self) -> Option<MoneyScale> {
        match self {
            NumberDisplayFormat::BdtThousand => Some(SCALE_THOUSAND),
            NumberDisplayFormat::BdtMillion => Some(SCALE_MILLION),
            NumberDisplayFormat::BdtLakh => Some(SCALE_LAKH),
            NumberDisplayFormat::BdtCrore => Some(SCALE_CRORE),
            NumberDisplayFormat::Raw | NumberDisplayFormat::StandardCommas => None,
        }
    }

    /// Every scale in menu order: Crore, Lakh, Million, Thousand, then plain.
    pub fn all() -> &'static [NumberDisplayFormat] {
        &[
            NumberDisplayFormat::BdtCrore,
            NumberDisplayFormat::BdtLakh,
            NumberDisplayFormat::BdtMillion,
            NumberDisplayFormat::BdtThousand,
            NumberDisplayFormat::StandardCommas,
            NumberDisplayFormat::Raw,
        ]
    }
}

/// Per-Column & Per-Metric Display Format Type (v1.2)
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricFormatType {
    Auto,            // Smart-detects Monetary vs Rate (%) vs Duration/Count
    BdtCrore,        // Divide by 10,000,000 + " Cr"
    BdtLakh,         // Divide by 100,000 + " L"
    BdtThousand,     // Divide by 1,000 + " K"
    BdtMillion,      // Divide by 1,000,000 + " M"
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
            MetricFormatType::BdtMillion,
            MetricFormatType::BdtThousand,
            MetricFormatType::StandardCommas,
            MetricFormatType::PercentAsIs,
            MetricFormatType::PercentDiv100,
            MetricFormatType::Raw,
        ]
    }

    /// The money scale of this format, or `None` (percent, plain numbers, Auto).
    pub fn scale(&self) -> Option<MoneyScale> {
        match self {
            MetricFormatType::BdtCrore => Some(SCALE_CRORE),
            MetricFormatType::BdtLakh => Some(SCALE_LAKH),
            MetricFormatType::BdtThousand => Some(SCALE_THOUSAND),
            MetricFormatType::BdtMillion => Some(SCALE_MILLION),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            MetricFormatType::Auto => "Auto (Smart Detect)",
            MetricFormatType::BdtCrore => "BDT Crore (Cr)",
            MetricFormatType::BdtLakh => "BDT Lakh (L)",
            MetricFormatType::BdtThousand => "BDT Thousand (K)",
            MetricFormatType::BdtMillion => "BDT Million (M)",
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
        MetricFormatType::BdtCrore
        | MetricFormatType::BdtLakh
        | MetricFormatType::BdtThousand
        | MetricFormatType::BdtMillion => match resolved_type.scale() {
            Some(sc) => sc.format(val, dec),
            None => format_with_commas(val, dec),
        },
        MetricFormatType::StandardCommas => format_with_commas(val, dec),
        MetricFormatType::PercentAsIs => format!("{}%", format_with_commas(val, dec)),
        MetricFormatType::PercentDiv100 => format!("{}%", format_with_commas(val * 100.0, dec)),
        MetricFormatType::Raw => format!("{}", val),
        MetricFormatType::Auto => match global_monetary_fmt.scale() {
            Some(sc) => sc.format(val, dec),
            None => match global_monetary_fmt {
                NumberDisplayFormat::StandardCommas => format_with_commas(val, dec),
                _ => format!("{}", val),
            },
        },
    }
}

/// The money scale a column is shown in: its own format, or for Auto money columns the global
/// scale. `None` for percentages, durations, counts and plain-number settings.
pub fn resolved_scale(
    col_name: &str,
    col_spec: Option<&ColumnFormatSpec>,
    global_monetary_fmt: NumberDisplayFormat,
) -> Option<MoneyScale> {
    let fmt_type = col_spec.map(|s| s.fmt_type).unwrap_or(MetricFormatType::Auto);
    let resolved = if fmt_type == MetricFormatType::Auto {
        classify_column_auto_format(col_name)
    } else {
        fmt_type
    };
    match resolved {
        MetricFormatType::Auto => global_monetary_fmt.scale(),
        other => other.scale(),
    }
}

pub fn classify_column_auto_format(col_name: &str) -> MetricFormatType {
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

/// Operators of the multi-condition filter (T1).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CondOp {
    Equals,
    NotEquals,
    Gt,
    Gte,
    Lt,
    Lte,
    Between,
    Contains,
    StartsWith,
    EndsWith,
    InList,
    IsBlank,
    IsNotBlank,
}

impl CondOp {
    pub fn all() -> &'static [CondOp] {
        &[
            CondOp::Equals,
            CondOp::NotEquals,
            CondOp::Gt,
            CondOp::Gte,
            CondOp::Lt,
            CondOp::Lte,
            CondOp::Between,
            CondOp::Contains,
            CondOp::StartsWith,
            CondOp::EndsWith,
            CondOp::InList,
            CondOp::IsBlank,
            CondOp::IsNotBlank,
        ]
    }

    /// Short text for chips and the editor.
    pub fn label(&self) -> &'static str {
        match self {
            CondOp::Equals => "equals",
            CondOp::NotEquals => "does not equal",
            CondOp::Gt => ">",
            CondOp::Gte => "\u{2265}",
            CondOp::Lt => "<",
            CondOp::Lte => "\u{2264}",
            CondOp::Between => "between",
            CondOp::Contains => "contains",
            CondOp::StartsWith => "starts with",
            CondOp::EndsWith => "ends with",
            CondOp::InList => "is in list",
            CondOp::IsBlank => "is blank",
            CondOp::IsNotBlank => "is not blank",
        }
    }

    /// Needs no value (is blank / is not blank).
    pub fn is_unary(&self) -> bool {
        matches!(self, CondOp::IsBlank | CondOp::IsNotBlank)
    }

    /// The equivalent of an old single-condition operator.
    pub fn from_filter_op(op: FilterOp) -> CondOp {
        match op {
            FilterOp::Eq => CondOp::Equals,
            FilterOp::NotEq => CondOp::NotEquals,
            FilterOp::Gt => CondOp::Gt,
            FilterOp::Gte => CondOp::Gte,
            FilterOp::Lt => CondOp::Lt,
            FilterOp::Lte => CondOp::Lte,
            FilterOp::Contains => CondOp::Contains,
            FilterOp::StartsWith => CondOp::StartsWith,
            FilterOp::IsNull => CondOp::IsBlank,
            FilterOp::IsNotNull => CondOp::IsNotBlank,
            FilterOp::InList => CondOp::InList,
        }
    }
}

/// One condition of a filter: "Balance > 1000", "Opndat between 2026-09-01 and 2026-09-30".
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FilterCondition {
    pub column: String,
    pub op: CondOp,
    #[serde(default)]
    pub value: String,
    /// Upper bound of `Between` (inclusive).
    #[serde(default)]
    pub value2: String,
}

impl FilterCondition {
    /// The chip text: "Brancd equals DHK", "Balance between 1000 and 5000".
    pub fn describe(&self) -> String {
        match self.op {
            CondOp::IsBlank | CondOp::IsNotBlank => format!("{} {}", self.column, self.op.label()),
            CondOp::Between => format!("{} between {} and {}", self.column, self.value.trim(), self.value2.trim()),
            _ => format!("{} {} {}", self.column, self.op.label(), self.value.trim()),
        }
    }
}

/// How the conditions of a filter combine.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GroupJoin {
    /// A row must meet every condition.
    #[default]
    And,
    /// A row must meet at least one condition.
    Or,
}

impl GroupJoin {
    pub fn label(&self) -> &'static str {
        match self {
            GroupJoin::And => "and",
            GroupJoin::Or => "or",
        }
    }
}

fn default_true() -> bool {
    true
}

/// The currency an FTP step reads curves in unless it says otherwise.
pub fn default_ftp_currency() -> String {
    "BDT".to_string()
}

/// Header mark for each sorted column: its level (1-based) and whether it is highest first. The
/// last sort step decides the order, so its levels are the ones shown.
pub fn sort_marks(steps: &[TransformStep]) -> HashMap<String, (usize, bool)> {
    let mut marks = HashMap::new();
    if let Some(levels) = steps.iter().rev().find_map(|s| s.sort_levels()) {
        for (i, l) in levels.iter().enumerate() {
            marks.entry(l.column.clone()).or_insert((i + 1, l.descending));
        }
    }
    marks
}

/// The text of a sort mark: "1 \u{2191}" (lowest first) or "2 \u{2193}" (highest first).
pub fn sort_mark_label(level: usize, descending: bool) -> String {
    format!("{} {}", level, if descending { "\u{2193}" } else { "\u{2191}" })
}

/// What a column's summary cell computes (T3).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SummaryAgg {
    #[default]
    None,
    Count,
    CountDistinct,
    Sum,
    Average,
    Min,
    Max,
    WeightedAvg,
}

impl SummaryAgg {
    pub fn all() -> &'static [SummaryAgg] {
        &[
            SummaryAgg::None,
            SummaryAgg::Count,
            SummaryAgg::CountDistinct,
            SummaryAgg::Sum,
            SummaryAgg::Average,
            SummaryAgg::Min,
            SummaryAgg::Max,
            SummaryAgg::WeightedAvg,
        ]
    }

    /// The label in the selector row and on exported totals rows.
    pub fn label(&self) -> &'static str {
        match self {
            SummaryAgg::None => "None",
            SummaryAgg::Count => "Count",
            SummaryAgg::CountDistinct => "Count distinct",
            SummaryAgg::Sum => "Sum",
            SummaryAgg::Average => "Average",
            SummaryAgg::Min => "Min",
            SummaryAgg::Max => "Max",
            SummaryAgg::WeightedAvg => "W.Avg",
        }
    }
}

/// A column's summary choice. `weight_col` is the weight of a weighted average.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct SummarySpec {
    #[serde(default)]
    pub agg: SummaryAgg,
    #[serde(default)]
    pub weight_col: Option<String>,
}

/// One level of a multi-level sort.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SortLevel {
    pub column: String,
    /// false = lowest first (A to Z, smallest, oldest); true = highest first.
    #[serde(default)]
    pub descending: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CondRule {
    pub col_name: String,
    pub op: FilterOp,
    /// The value to compare with, or (when `compare_is_col`) the name of the column to compare with.
    pub compare_val: String,
    pub output_val: String,
    /// The right-hand side is another column, not a value.
    #[serde(default)]
    pub compare_is_col: bool,
}

/// What a conditional column gives a row when a column in one of its rules is blank (null, or
/// text that is empty after trimming).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub enum BlankRule {
    /// Steps saved before this choice existed: a blank never matches, so the next rule (or
    /// ELSE) decides.
    #[default]
    FallThrough,
    /// The row gets this value at once, without trying the other rules (empty: a blank cell).
    Result(String),
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

// ---------------------------------------------------------------------------- rate profile

/// How the rate bands are laid out.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum RateBandMode {
    /// Equal bands from `start_pct` to `end_pct`, `step_pct` wide (all in percent).
    Fixed { start_pct: f64, end_pct: f64, step_pct: f64 },
    /// Bands between the listed edges (percent).
    Custom { edges: Vec<f64> },
}

/// A rate profile: the bands of an interest-rate column (saved as a preset in
/// `portable_data/rate_bands.json`, like the maturity bucket presets).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RateBandDef {
    pub name: String,
    pub mode: RateBandMode,
    /// `false`: a band is (a, b], so 9.00 falls in 8%–9%. `true`: a band is [a, b), so 9.00
    /// falls in 9%–10%.
    #[serde(default)]
    pub lower_inclusive: bool,
}

/// The most bands a profile may have.
pub const RATE_BANDS_MAX: usize = 500;

/// The labels of a profile.
#[derive(Clone, Debug, PartialEq)]
pub struct RateBandLabels {
    /// The band edges in percent, starting at 0.
    pub edges: Vec<f64>,
    /// "00. 0%": exactly zero.
    pub zero: String,
    /// "Below 0%".
    pub below: String,
    /// One label per band: `bands[k]` is the band between `edges[k]` and `edges[k + 1]`.
    pub bands: Vec<String>,
    /// "> 15%" (or "≥ 15%" for lower-inclusive bands): beyond the last edge.
    pub above: String,
    /// "No rate": blank.
    pub blank: String,
}

/// A percentage as a banker writes it: 8, 8.25, 0.5 (no trailing zeros).
pub fn fmt_pct(x: f64) -> String {
    let t = format!("{:.4}", x);
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// "0, 5, 7.5, 9" -> edges. Commas, semicolons and spaces separate; anything that is not a
/// number is an error that names it.
pub fn parse_rate_edges(text: &str) -> anyhow::Result<Vec<f64>> {
    let mut out = Vec::new();
    for tok in text.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).map(str::trim).filter(|t| !t.is_empty()) {
        let t = tok.trim_end_matches('%');
        let v: f64 = t.parse().map_err(|_| anyhow::anyhow!("'{}' is not a number. Write the band edges as numbers in percent, like 0, 5, 7, 9, 12.", tok))?;
        out.push(v);
    }
    Ok(out)
}

impl RateBandDef {
    pub fn fixed(name: &str, start_pct: f64, end_pct: f64, step_pct: f64) -> Self {
        Self { name: name.to_string(), mode: RateBandMode::Fixed { start_pct, end_pct, step_pct }, lower_inclusive: false }
    }

    pub fn custom(name: &str, edges: Vec<f64>) -> Self {
        Self { name: name.to_string(), mode: RateBandMode::Custom { edges }, lower_inclusive: false }
    }

    /// The default profile: 0% to 15% in steps of 1%.
    pub fn default_profile() -> Self {
        Self::fixed("0% to 15% by 1%", 0.0, 15.0, 1.0)
    }

    /// The edges, checked, in ascending order and starting at 0 (a profile that starts above 0
    /// gets a first band from 0).
    pub fn edges(&self) -> anyhow::Result<Vec<f64>> {
        let round = |x: f64| (x * 1_000_000.0).round() / 1_000_000.0;
        let mut e: Vec<f64> = match &self.mode {
            RateBandMode::Fixed { start_pct, end_pct, step_pct } => {
                let (a, b, st) = (*start_pct, *end_pct, *step_pct);
                if !(a.is_finite() && b.is_finite() && st.is_finite()) {
                    anyhow::bail!("The start, end and step of the rate bands must be numbers.");
                }
                if st <= 0.0 {
                    anyhow::bail!("The step of the rate bands must be more than 0.");
                }
                if b <= a {
                    anyhow::bail!("The end of the rate bands ({}%) must be above the start ({}%).", fmt_pct(b), fmt_pct(a));
                }
                let n = (b - a) / st;
                if (n - n.round()).abs() > 1e-6 {
                    anyhow::bail!(
                        "From {}% to {}% is not a whole number of steps of {}%. Change the end or the step.",
                        fmt_pct(a),
                        fmt_pct(b),
                        fmt_pct(st)
                    );
                }
                let n = n.round() as usize;
                if n > RATE_BANDS_MAX {
                    anyhow::bail!("{} bands is too many (the most is {}). Use a wider step.", n, RATE_BANDS_MAX);
                }
                (0..=n).map(|i| round(a + i as f64 * st)).collect()
            }
            RateBandMode::Custom { edges } => {
                if edges.iter().any(|x| !x.is_finite()) {
                    anyhow::bail!("The band edges must be numbers.");
                }
                let mut v: Vec<f64> = edges.iter().map(|x| round(*x)).collect();
                v.sort_by(|a, b| a.total_cmp(b));
                v.dedup();
                v
            }
        };
        if let Some(first) = e.first() {
            if *first < 0.0 {
                anyhow::bail!("A band edge ({}%) is below 0. Negative rates are put in 'Below 0%'; start the edges at 0 or above.", fmt_pct(*first));
            }
        }
        if e.first().is_some_and(|f| *f > 0.0) {
            e.insert(0, 0.0);
        }
        if e.len() < 2 {
            anyhow::bail!("Give at least one band edge above 0, like 0, 5, 7, 9, 12.");
        }
        if e.len() - 1 > RATE_BANDS_MAX {
            anyhow::bail!("{} bands is too many (the most is {}).", e.len() - 1, RATE_BANDS_MAX);
        }
        Ok(e)
    }

    /// The labels of every band. They are numbered so that they sort in band order.
    pub fn labels(&self) -> anyhow::Result<RateBandLabels> {
        let edges = self.edges()?;
        let m = edges.len() - 1;
        let w = m.to_string().len().max(2);
        let bands = (1..=m).map(|k| format!("{:0w$}. {}%\u{2013}{}%", k, fmt_pct(edges[k - 1]), fmt_pct(edges[k]), w = w)).collect();
        let last = fmt_pct(edges[m]);
        Ok(RateBandLabels {
            zero: format!("{:0w$}. 0%", 0, w = w),
            below: "Below 0%".to_string(),
            bands,
            above: if self.lower_inclusive { format!("\u{2265} {}%", last) } else { format!("> {}%", last) },
            blank: "No rate".to_string(),
            edges,
        })
    }

    /// The label of one rate (percent; `None` = blank).
    pub fn label_for(&self, rate: Option<f64>) -> anyhow::Result<String> {
        let l = self.labels()?;
        Ok(match rate {
            None => l.blank,
            Some(x) if x.is_nan() => l.blank,
            Some(x) if x == 0.0 => l.zero,
            Some(x) if x < 0.0 => l.below,
            Some(x) => {
                let hit = (1..l.edges.len()).find(|k| if self.lower_inclusive { x < l.edges[*k] } else { x <= l.edges[*k] });
                match hit {
                    Some(k) => l.bands[k - 1].clone(),
                    None => l.above,
                }
            }
        })
    }

    pub fn default_presets() -> Vec<RateBandDef> {
        vec![
            Self::default_profile(),
            Self::fixed("0% to 20% by 2%", 0.0, 20.0, 2.0),
            Self::custom("Edges 0, 5, 7, 9, 12", vec![0.0, 5.0, 7.0, 9.0, 12.0]),
        ]
    }

    /// The built-in presets and those saved in `rate_bands.json`.
    pub fn load_all_presets(file: &Path) -> Vec<RateBandDef> {
        let mut presets = Self::default_presets();
        if let Ok(txt) = std::fs::read_to_string(file) {
            if let Ok(customs) = serde_json::from_str::<Vec<RateBandDef>>(&txt) {
                for c in customs {
                    presets.retain(|p| !p.name.eq_ignore_ascii_case(&c.name));
                    presets.push(c);
                }
            }
        }
        presets
    }

    pub fn save_custom_preset(file: &Path, new_preset: RateBandDef) -> anyhow::Result<()> {
        let mut customs: Vec<RateBandDef> = std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        customs.retain(|p| !p.name.eq_ignore_ascii_case(&new_preset.name));
        customs.push(new_preset);
        std::fs::write(file, serde_json::to_string_pretty(&customs)?)?;
        Ok(())
    }
}

impl PortablePaths {
    /// The folder of the gap limit files (`gap_limits/<preset>.json`).
    pub fn gap_limits_dir(&self) -> PathBuf {
        self.root_dir.join(crate::banking::GAP_LIMITS_DIR)
    }

    /// Where the rate profile presets are saved.
    pub fn rate_bands_file(&self) -> PathBuf {
        self.root_dir.join("rate_bands.json")
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

// ---------------------------------------------------------------------------- pivot table (V1)

/// How one value field of a pivot table is aggregated.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PivotAgg {
    #[default]
    Sum,
    Count,
    CountDistinct,
    Average,
    Min,
    Max,
    WeightedAvg,
}

impl PivotAgg {
    pub fn all() -> &'static [PivotAgg] {
        &[
            PivotAgg::Sum,
            PivotAgg::Count,
            PivotAgg::CountDistinct,
            PivotAgg::Average,
            PivotAgg::Min,
            PivotAgg::Max,
            PivotAgg::WeightedAvg,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            PivotAgg::Sum => "Sum",
            PivotAgg::Count => "Count",
            PivotAgg::CountDistinct => "Count distinct",
            PivotAgg::Average => "Average",
            PivotAgg::Min => "Min",
            PivotAgg::Max => "Max",
            PivotAgg::WeightedAvg => "W.Avg",
        }
    }

    /// True when the cells of a row or column add up to its total (so a share of the total means something).
    pub fn is_additive(&self) -> bool {
        matches!(self, PivotAgg::Sum | PivotAgg::Count)
    }
}

/// One value field of a pivot table.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct PivotValue {
    pub field: String,
    #[serde(default)]
    pub agg: PivotAgg,
    /// The weight column of a weighted average.
    #[serde(default)]
    pub weight_col: Option<String>,
}

impl PivotValue {
    /// "Sum of Balance", "W.Avg of Rate by Balance".
    pub fn label(&self) -> String {
        match (&self.agg, self.weight_col.as_deref().map(str::trim).filter(|w| !w.is_empty())) {
            (PivotAgg::WeightedAvg, Some(w)) => format!("W.Avg of {} by {}", self.field, w),
            (a, _) => format!("{} of {}", a.label(), self.field),
        }
    }
}

/// A filter of a pivot table: only rows whose field is one of `values` are used; `None` is a blank value.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct PivotFilter {
    pub field: String,
    #[serde(default)]
    pub values: Vec<Option<String>>,
}

/// "Show values as" of a pivot table.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PivotShowAs {
    #[default]
    NoCalculation,
    PctOfRowTotal,
    PctOfColumnTotal,
    PctOfGrandTotal,
}

impl PivotShowAs {
    pub fn all() -> &'static [PivotShowAs] {
        &[PivotShowAs::NoCalculation, PivotShowAs::PctOfRowTotal, PivotShowAs::PctOfColumnTotal, PivotShowAs::PctOfGrandTotal]
    }

    pub fn label(&self) -> &'static str {
        match self {
            PivotShowAs::NoCalculation => "No calculation",
            PivotShowAs::PctOfRowTotal => "% of row total",
            PivotShowAs::PctOfColumnTotal => "% of column total",
            PivotShowAs::PctOfGrandTotal => "% of grand total",
        }
    }
}

/// The scale a pivot's value columns are shown in. The numbers stay in BDT; this sets each value
/// column's display format (so the global scale is not applied a second time).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PivotScale {
    /// As set in the View menu.
    #[default]
    Global,
    Plain,
    Thousand,
    Lakh,
    Million,
    Crore,
}

impl PivotScale {
    pub fn all() -> &'static [PivotScale] {
        &[PivotScale::Global, PivotScale::Plain, PivotScale::Thousand, PivotScale::Lakh, PivotScale::Million, PivotScale::Crore]
    }

    pub fn label(&self) -> &'static str {
        match self {
            PivotScale::Global => "As in View",
            PivotScale::Plain => "Plain",
            PivotScale::Thousand => "Thousand",
            PivotScale::Lakh => "Lakh",
            PivotScale::Million => "Million",
            PivotScale::Crore => "Crore",
        }
    }

    /// The column format that shows this scale; `None` follows the global setting.
    pub fn format_type(&self) -> Option<MetricFormatType> {
        match self {
            PivotScale::Global => None,
            PivotScale::Plain => Some(MetricFormatType::StandardCommas),
            PivotScale::Thousand => Some(MetricFormatType::BdtThousand),
            PivotScale::Lakh => Some(MetricFormatType::BdtLakh),
            PivotScale::Million => Some(MetricFormatType::BdtMillion),
            PivotScale::Crore => Some(MetricFormatType::BdtCrore),
        }
    }
}

/// "Sort by value": the rows are ordered by the row total of one value field instead of by their labels.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PivotSortByValue {
    /// Index into the pivot's value fields.
    pub value_index: usize,
    /// Highest first.
    #[serde(default)]
    pub descending: bool,
}

/// Everything a pivot table is made of (the step `PivotTable`, the builder dialog and the engine share it).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PivotSpec {
    #[serde(default)]
    pub rows: Vec<String>,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub values: Vec<PivotValue>,
    #[serde(default)]
    pub filters: Vec<PivotFilter>,
    #[serde(default = "default_true")]
    pub row_totals: bool,
    #[serde(default = "default_true")]
    pub column_totals: bool,
    #[serde(default)]
    pub subtotals: bool,
    #[serde(default)]
    pub show_as: PivotShowAs,
    #[serde(default)]
    pub scale: PivotScale,
    #[serde(default)]
    pub sort_by_value: Option<PivotSortByValue>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
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

/// Which yield curve an FTP step uses (Phase 4 V4).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub enum FtpCurveMode {
    /// The points typed in the step (its `curve`): what every step saved before Phase 4 does.
    #[default]
    Inline,
    /// The curve of the store dated on or before `date` (YYYY-MM-DD), for every deal.
    FixedDate { date: String },
    /// For each deal, the latest curve of the store dated on or before its open date. A deal opened
    /// before the earliest curve is an error, unless `use_earliest_for_older`: then the earliest
    /// curve is used and `FTP_Note` says so.
    OriginationDate {
        #[serde(default)]
        use_earliest_for_older: bool,
    },
}

/// Interpolation of an FTP step saved without the field (before Phase 2): the old step lookup.
pub fn legacy_ftp_interpolation() -> FtpInterpolation {
    FtpInterpolation::NextPointUp
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
    /// Text columns read as numbers (Float64) with the rules of a Float64 schema column: the
    /// pre-export warning offers it ("Set to Number").
    ConvertToNumber {
        columns: Vec<String>,
    },
    FilterRows {
        column: String,
        operator: FilterOp,
        value: String,
        keep_matching: bool,
    },
    /// A step that is turned off: it stays in the list and in macros but changes nothing (T4).
    Skipped {
        step: Box<TransformStep>,
    },
    /// Sort by several columns, blanks always last, ties keep their earlier order (T2).
    SortRows {
        #[serde(default)]
        levels: Vec<SortLevel>,
    },
    /// Several conditions joined by And / Or (T1). Rows whose test is unknown (a null compared
    /// with a value) do not match: "Keep matching" drops them, "Delete matching" keeps them.
    FilterGroup {
        #[serde(default)]
        conditions: Vec<FilterCondition>,
        #[serde(default)]
        join: GroupJoin,
        #[serde(default = "default_true")]
        keep_matching: bool,
    },
    ConditionalColumn {
        new_col: String,
        rules: Vec<CondRule>,
        fallback: String,
        output_as_float: bool,
        /// The result when a column a rule looks at is blank on either side.
        #[serde(default)]
        blank_result: BlankRule,
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
    /// Replaces every run of line breaks (\r\n, \n or \r) inside the text values of the chosen
    /// columns (all text columns when none are listed) with one space and trims the ends of the
    /// values that had one. With `trim_all` the ends of every text value are trimmed of spaces,
    /// as the old Schema & Ingestion check box did. Never applied automatically.
    CleanLineBreaks {
        #[serde(default)]
        columns: Vec<String>,
        /// "Trim spaces on all text values": on for the step the Schema & Ingestion box makes,
        /// off from the Transform menu.
        #[serde(default)]
        trim_all: bool,
    },
    MaturityBucket {
        mat_col: String,
        cutoff_date: String,
        bucket_def: CustomBucketDef,
        null_as_sight: bool,
        new_col: String,
    },
    /// Puts a rate column into labelled bands ("09. 8%–9%"), like the maturity bucketer.
    RateBand {
        rate_col: String,
        band_def: RateBandDef,
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
        /// The date a floating rate reprices (V5): the effective repricing date is the earlier of
        /// this and the maturity. `None`: the maturity (every step saved before Phase 4).
        #[serde(default)]
        reprice_col: Option<String>,
        /// A file in `portable_data/betas` (without `.json`) with the pass-through % of each
        /// product; products that are not in it pass through 100%.
        #[serde(default)]
        beta_file: Option<String>,
        /// The column with the product codes the beta file is read against.
        #[serde(default)]
        product_col: Option<String>,
        /// The shocked rate never goes below this (percent); the rate change is cut to fit.
        #[serde(default)]
        floor_pct: Option<f64>,
    },
    FtpCurveMap {
        opndat_col: String,
        matdat_col: String,
        bal_col: String,
        rate_col: String,
        curve: Vec<FtpPoint>,
        is_liability: bool,
        /// Steps saved before Phase 2 (no field) load as NextPointUp, so old macros reproduce
        /// their past results; new steps from the FTP modal default to Linear.
        #[serde(default = "legacy_ftp_interpolation")]
        interpolation: FtpInterpolation,
        /// Which curve: the points typed in the step (`curve`; every step saved before Phase 4), the
        /// curve of the store on a fixed date, or each deal's own curve by its open date (V4).
        #[serde(default)]
        curve_mode: FtpCurveMode,
        /// The currency of the curves a step that reads the store uses (default BDT); curves of
        /// other currencies are skipped with a note.
        #[serde(default = "default_ftp_currency")]
        currency: String,
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
    /// IFRS 9 staging and expected credit loss (Phase 5 R7, BRPD Circular 06/2026): adds `Stage`,
    /// `Stage_Reason`, `EAD`, `ECL`, `ECL_Rule_File` and `ECL_Params_File`.
    IfrsEcl {
        #[serde(default)]
        segment_col: Option<String>,
        #[serde(default)]
        fixed_segment: String,
        #[serde(default)]
        drawn_col: String,
        #[serde(default)]
        undrawn_col: Option<String>,
        /// The effective interest rate, in percent.
        #[serde(default)]
        eir_col: String,
        #[serde(default)]
        maturity_col: String,
        #[serde(default)]
        dpd_col: Option<String>,
        #[serde(default)]
        grade_col: Option<String>,
        /// Classified grades (SS, DF, B/L) are Stage 3.
        #[serde(default)]
        classified_stage3: bool,
        #[serde(default)]
        forborne_col: Option<String>,
        #[serde(default)]
        sicr_unassessed_col: Option<String>,
        #[serde(default)]
        rating_orig_col: Option<String>,
        #[serde(default)]
        rating_now_col: Option<String>,
        #[serde(default)]
        prev_stage_col: Option<String>,
        #[serde(default)]
        days_performing_col: Option<String>,
        /// YYYY-MM-DD.
        #[serde(default)]
        cutoff_date: String,
    },
    /// Loan classification from the overdue period (Phase 5 R4, BRPD Circular 15/2024): adds
    /// `Overdue_Months`, `CL_Objective`, `CL_Final` and `CL_Rule_File`.
    ClassifyOverdue {
        /// The loan category column (codes go through `loan_category_map.json`); empty = `fixed_category` for every row.
        #[serde(default)]
        category_col: Option<String>,
        #[serde(default)]
        fixed_category: String,
        /// The overdue-since date column, or the days-past-due column (one of the two).
        #[serde(default)]
        since_col: Option<String>,
        #[serde(default)]
        dpd_col: Option<String>,
        /// The optional subjective/override grade column.
        #[serde(default)]
        override_col: Option<String>,
        /// YYYY-MM-DD.
        #[serde(default)]
        cutoff_date: String,
    },
    GroupByPivot {
        group_cols: Vec<String>,
        pivot_col: Option<String>,
        aggs: Vec<AggSpec>,
        sort_by_col: Option<String>,
        top_n: Option<usize>,
    },
    /// The pivot builder's table (Phase 4 V1): Rows, Columns, Values and Filters, with totals.
    /// `GroupByPivot` steps from older macros stay as they were.
    PivotTable {
        #[serde(default)]
        rows: Vec<String>,
        #[serde(default)]
        columns: Vec<String>,
        #[serde(default)]
        values: Vec<PivotValue>,
        #[serde(default)]
        filters: Vec<PivotFilter>,
        #[serde(default = "default_true")]
        row_totals: bool,
        #[serde(default = "default_true")]
        column_totals: bool,
        #[serde(default)]
        subtotals: bool,
        #[serde(default)]
        show_as: PivotShowAs,
        #[serde(default)]
        scale: PivotScale,
        #[serde(default)]
        sort_by_value: Option<PivotSortByValue>,
    },
    SqlQuery {
        query: String,
    },
}

impl TransformStep {
    /// A filter step as conditions, for the chip bar and the editor: a `FilterGroup`, or an old
    /// single `FilterRows` shown as a one-condition group. `None` for every other step.
    /// The step with a "turned off" wrapper removed.
    pub fn inner(&self) -> &TransformStep {
        match self {
            TransformStep::Skipped { step } => step.inner(),
            other => other,
        }
    }

    /// True for a step that is turned off.
    pub fn is_skipped(&self) -> bool {
        matches!(self, TransformStep::Skipped { .. })
    }

    /// True for a filter that is turned on (rows are removed by it, so the control total is
    /// expected to differ from the source's).
    pub fn is_active_filter(&self) -> bool {
        matches!(self, TransformStep::FilterRows { .. } | TransformStep::FilterGroup { .. })
    }

    /// The name of the kind of step ("Filter", "Sort", "Calc column"), for the timeline.
    pub fn title(&self) -> &'static str {
        match self.inner() {
            TransformStep::SelectAndReorderColumns { .. } => "Columns",
            TransformStep::RenameColumn { .. } => "Rename",
            TransformStep::ConvertToNumber { .. } => "Set to Number",
            TransformStep::FilterRows { .. } | TransformStep::FilterGroup { .. } => "Filter",
            TransformStep::SortRows { .. } => "Sort",
            TransformStep::ConditionalColumn { .. } => "Conditional column",
            TransformStep::MathFormula { .. } => "Calc column",
            TransformStep::NormalizeEntity { .. } => "Normalise names",
            TransformStep::CleanLineBreaks { .. } => "Clean line breaks",
            TransformStep::MaturityBucket { .. } => "Maturity buckets",
            TransformStep::RateBand { .. } => "Rate profile",
            TransformStep::RateShockSim { .. } => "Rate shock",
            TransformStep::FtpCurveMap { .. } => "FTP",
            TransformStep::BankingHygieneAudit { .. } => "Hygiene audit",
            TransformStep::DurationAnalysis { .. } => "Duration / \u{394}EVE",
            TransformStep::LoanClassificationProvision { .. } => "Provision",
            TransformStep::LoanLimitUtilization { .. } => "Limits",
            TransformStep::ClassifyOverdue { .. } => "Classify by overdue",
            TransformStep::IfrsEcl { .. } => "IFRS 9 ECL",
            TransformStep::GroupByPivot { .. } | TransformStep::PivotTable { .. } => "Pivot",
            TransformStep::SqlQuery { .. } => "SQL",
            TransformStep::Skipped { .. } => "Off",
        }
    }

    /// The sort levels of a `SortRows` step.
    pub fn sort_levels(&self) -> Option<&[SortLevel]> {
        match self {
            TransformStep::SortRows { levels } => Some(levels),
            _ => None,
        }
    }

    pub fn as_filter_group(&self) -> Option<(Vec<FilterCondition>, GroupJoin, bool)> {
        match self {
            TransformStep::FilterGroup { conditions, join, keep_matching } => {
                Some((conditions.clone(), *join, *keep_matching))
            }
            TransformStep::FilterRows { column, operator, value, keep_matching } => Some((
                vec![FilterCondition {
                    column: column.clone(),
                    op: CondOp::from_filter_op(*operator),
                    value: value.clone(),
                    value2: String::new(),
                }],
                GroupJoin::And,
                *keep_matching,
            )),
            _ => None,
        }
    }

    pub fn short_label(&self) -> String {
        match self {
            TransformStep::SelectAndReorderColumns { cols } => {
                format!("Select/Order {} Cols", cols.len())
            }
            TransformStep::RenameColumn { old_name, new_name } => {
                format!("Rename {} -> {}", old_name, new_name)
            }
            TransformStep::ConvertToNumber { columns } => {
                format!("Set to Number: {}", columns.join(", "))
            }
            TransformStep::Skipped { step } => format!("Off: {}", step.short_label()),
            TransformStep::SortRows { levels } => {
                let parts: Vec<String> = levels
                    .iter()
                    .map(|l| format!("{} {}", l.column, if l.descending { "\u{2193}" } else { "\u{2191}" }))
                    .collect();
                format!("Sort: {}", parts.join(", "))
            }
            TransformStep::FilterGroup { conditions, join, keep_matching } => {
                let mode = if *keep_matching { "Keep" } else { "Delete" };
                let parts: Vec<String> = conditions.iter().map(|c| c.describe()).collect();
                format!("{}: {}", mode, parts.join(&format!(" {} ", join.label())))
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
            TransformStep::CleanLineBreaks { columns, trim_all } => {
                let trim = if *trim_all { ", trim spaces on all values" } else { "" };
                if columns.is_empty() {
                    format!("Clean line breaks (all text columns{})", trim)
                } else {
                    format!("Clean line breaks ({}{})", columns.join(", "), trim)
                }
            }
            TransformStep::MaturityBucket {
                new_col,
                bucket_def,
                ..
            } => {
                format!("+Bucket ({}): {}", bucket_def.name, new_col)
            }
            TransformStep::RateBand { new_col, band_def, .. } => {
                format!("+Rate profile ({}): {}", band_def.name, new_col)
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
            TransformStep::IfrsEcl { cutoff_date, .. } => {
                format!("+IFRS 9 ECL (cutoff {})", cutoff_date.trim())
            }
            TransformStep::ClassifyOverdue { cutoff_date, .. } => {
                format!("+Classify by overdue (cutoff {})", cutoff_date.trim())
            }
            TransformStep::GroupByPivot { group_cols, .. } => {
                format!("Group/Pivot by {:?}", group_cols)
            }
            TransformStep::PivotTable { rows, columns, values, .. } => {
                let mut t = format!("Pivot: {}", values.iter().map(|v| v.label()).collect::<Vec<_>>().join(", "));
                if !rows.is_empty() {
                    t.push_str(&format!(" by {}", rows.join(" / ")));
                }
                if !columns.is_empty() {
                    t.push_str(&format!(" across {}", columns.join(" / ")));
                }
                t
            }
            TransformStep::SqlQuery { .. } => "SQL Query Transform".to_string(),
        }
    }
}

impl TransformStep {
    /// The pivot table settings of a `PivotTable` step.
    pub fn as_pivot_spec(&self) -> Option<PivotSpec> {
        match self.inner() {
            TransformStep::PivotTable { rows, columns, values, filters, row_totals, column_totals, subtotals, show_as, scale, sort_by_value } => Some(PivotSpec {
                rows: rows.clone(),
                columns: columns.clone(),
                values: values.clone(),
                filters: filters.clone(),
                row_totals: *row_totals,
                column_totals: *column_totals,
                subtotals: *subtotals,
                show_as: *show_as,
                scale: *scale,
                sort_by_value: *sort_by_value,
            }),
            _ => None,
        }
    }

    /// A `PivotTable` step from settings.
    pub fn from_pivot_spec(spec: PivotSpec) -> TransformStep {
        TransformStep::PivotTable {
            rows: spec.rows,
            columns: spec.columns,
            values: spec.values,
            filters: spec.filters,
            row_totals: spec.row_totals,
            column_totals: spec.column_totals,
            subtotals: spec.subtotals,
            show_as: spec.show_as,
            scale: spec.scale,
            sort_by_value: spec.sort_by_value,
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
    /// The repricing-gap summary of a rate shock (Phase 4 V5): the tab's rows (its source and
    /// steps) put into repricing buckets, with the ΔNII of the shock in `spec`.
    RepricingGap {
        source: Box<TabSource>,
        steps: Vec<TransformStep>,
        spec: crate::banking::RateShockSpec,
    },
    /// The liquidity gap ladder (Phase 4 V3): inflows and outflows of the chosen tabs by maturity bucket.
    GapLadder {
        inputs: Vec<GapTabInput>,
        preset: CustomBucketDef,
        cutoff_date: String,
        null_as_sight: bool,
        /// The limits read from `portable_data/gap_limits` when the tab was built.
        limits: Option<crate::banking::GapLimitsFile>,
        /// The behaviour profile applied (Phase 5 R2); `None` gives the Phase 4 ladder.
        behaviour: Option<crate::banking::GapBehaviour>,
    },
    /// The IRRBB cash-flow ladder (Phase 5 R1): the dated cash flows of the chosen tabs in the 19
    /// buckets of the Bangladesh Bank guideline, by currency.
    IrrbbLadder {
        inputs: Vec<CfTabInput>,
        cutoff_date: String,
        mode: crate::cashflow::CashFlowMode,
        scenario: crate::cashflow::Scenario,
        /// The behaviour profile used (its file name and content when the tab was built).
        behaviour: Option<(String, crate::behaviour::BehaviourFile)>,
        /// "Treat blank maturity as O/N".
        blank_maturity_on: bool,
    },
    /// The IFRS 9 summary (Phase 5 R7): ECL, regulatory provision, higher of and shortfall by grade
    /// group and by stage, from a tab that has the "IFRS 9 ECL" step's columns.
    EclSummary {
        source: Box<TabSource>,
        steps: Vec<TransformStep>,
        spec: crate::ecl::EclSummarySpec,
    },
    /// The CL-1 summary (Phase 5 R5): the loans of a tab by loan category and grade group.
    Cl1Summary {
        source: Box<TabSource>,
        steps: Vec<TransformStep>,
        spec: crate::cl1::Cl1Spec,
        /// The template when the tab was built (the tab and its Excel layout follow it).
        template: Box<crate::cl1::Cl1Template>,
    },
    /// LCR and NSFR from the GL balances of a tab (Phase 5 R6).
    Liquidity {
        source: Box<TabSource>,
        steps: Vec<TransformStep>,
        spec: crate::liquidity::LiqSpec,
        /// The map as read when the tab was built, and its file name.
        map: Box<crate::liquidity::LiqMap>,
        map_file: String,
        view: LiquidityView,
    },
    /// The six-scenario IRRBB run (Phase 5 R3): `view` says which of its two tables this tab shows.
    Irrbb {
        inputs: Vec<CfTabInput>,
        spec: Box<crate::irrbb::IrrbbSpec>,
        view: IrrbbView,
    },
    /// The CL migration matrix of two months (Phase 4 V2).
    ClMigration {
        prev_source: Box<TabSource>,
        prev_steps: Vec<TransformStep>,
        curr_source: Box<TabSource>,
        curr_steps: Vec<TransformStep>,
        spec: crate::banking::ClMigrationSpec,
    },
}

/// Which table of the LCR / NSFR run a tab shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiquidityView {
    /// "LCR/NSFR lines": the line detail (code, label, amount, factor, weighted).
    Lines,
    /// "LCR/NSFR summary": HQLA, net cash outflows, ASF, RSF, the two ratios and their flags.
    Summary,
}

/// Which table of the six-scenario run a tab shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrrbbView {
    /// "IRRBB results": the scenarios, their EVE change, loss and ΔNII.
    Results,
    /// "IRRBB cash flows": the cash-flow ladder with the base present values.
    CashFlows,
}

/// A tab that feeds the IRRBB cash-flow tools: where its rows come from and what its columns mean.
#[derive(Clone, Debug)]
pub struct CfTabInput {
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    pub spec: crate::cashflow::CfInputSpec,
}

/// A tab that feeds the liquidity gap ladder: where its rows come from and how they count.
#[derive(Clone, Debug)]
pub struct GapTabInput {
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    pub spec: crate::banking::GapInputSpec,
}

#[derive(Clone)]
pub struct TabState {
    /// Stable identity of the tab (survives closing other tabs); background results use it.
    pub id: u64,
    pub title: String,
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    pub col_formats: HashMap<String, ColumnFormatSpec>,
    pub total_rows: usize,
    pub control_col: Option<String>,
    pub control_sum: Option<f64>,
    pub baseline_control_sum: Option<f64>,
    pub viewport_offset: usize,
    /// Row offset of `cached_slice` (differs from `viewport_offset` while a new page is loading).
    pub slice_offset: usize,
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
    /// The summary choices the user made per column (saved with the tab and in macros).
    pub summary: HashMap<String, SummarySpec>,
    /// The computed summary of the whole result (empty until the background job finishes).
    pub summary_cells: Vec<crate::summary::SummaryCell>,
    /// Steps that cannot run against the columns they would see (T4).
    pub issues: Vec<crate::engine::StepIssue>,
    /// How many steps the table runs: all of them, or those before the first broken one.
    pub exec_steps: usize,
    /// Undo / redo of changes to the steps, control column, summary choices and column formats.
    pub history: crate::steps_edit::StepHistory<crate::steps_edit::TabSnapshot>,
    /// Rows after each step: index 0 is the source, index i is after step i. `None` = not counted yet.
    pub step_counts: Vec<Option<usize>>,
    /// Key of each entry of `step_counts`: the source identity plus the steps up to it
    /// (see `steps_edit::count_keys`).
    pub step_hashes: Vec<u64>,
    /// Statistics of the column the inspector shows (computed on the worker).
    pub column_stats: Option<crate::column_stats::ColumnStats>,
    /// The page could not be read: the reason. The table keeps the last page, dimmed and marked
    /// "Out of date", until a page is read again.
    pub page_error: Option<String>,
    /// The summary row could not be computed: the reason (the band shows "—" with it on hover).
    pub summary_error: Option<String>,
}

/// What kind of line the row-count details list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineIssue {
    /// A record whose quoted value holds a line break, so it takes several physical lines.
    QuotedLineBreak,
    /// A record with another number of fields than the header has.
    WrongFieldCount,
}

/// One entry of the row-count details: where it starts in the file and what it looks like.
#[derive(Clone, Debug, PartialEq)]
pub struct LineDetail {
    /// 1-based number of the physical line the record starts on.
    pub line: usize,
    pub issue: LineIssue,
    /// Physical lines the record takes.
    pub span: usize,
    pub fields: usize,
    pub expected_fields: usize,
    /// The first 80 characters of that line.
    pub text: String,
}

/// How many entries the details list.
pub const ROW_DETAILS_LIMIT: usize = 20;

/// Rows read from a CSV against its non-empty lines (minus the header and title lines), and, when
/// fewer rows were read than there are lines, why.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowCountCheck {
    pub nonempty_lines: usize,
    pub skip_rows: usize,
    pub parsed_rows: usize,
    /// Records with a line break inside quotes.
    pub multiline_records: usize,
    /// The non-empty lines those records take beyond their first.
    pub explained_lines: usize,
    /// Records with the wrong number of fields.
    pub malformed_records: usize,
    /// The first `ROW_DETAILS_LIMIT` of both kinds, in file order.
    pub details: Vec<LineDetail>,
}

impl RowCountCheck {
    /// True when every line missing from the row count is the continuation of a record with a line
    /// break inside quotes, and no record has the wrong number of fields: the file is fine.
    pub fn is_explained(&self) -> bool {
        let missing = self.data_lines().saturating_sub(self.parsed_rows);
        self.parsed_rows > 0 && missing > 0 && self.malformed_records == 0 && self.explained_lines == missing
    }

    /// The text for the status bar: the quiet explanation (grey) or the short warning; `None` when
    /// the counts agree.
    pub fn status_text(&self) -> Option<String> {
        if self.is_explained() {
            return Some(format!(
                "{} rows from {} lines: {} records contain line breaks inside quotes",
                format_with_commas(self.parsed_rows as f64, 0),
                format_with_commas(self.data_lines() as f64, 0),
                format_with_commas(self.multiline_records as f64, 0)
            ));
        }
        self.warning().map(|_| {
            format!(
                "\u{26A0} Rows read {} vs file data lines {}",
                format_with_commas(self.parsed_rows as f64, 0),
                format_with_commas(self.data_lines() as f64, 0)
            )
        })
    }

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
                format_with_commas(self.nonempty_lines as f64, 0),
                format_with_commas(self.data_lines() as f64, 0)
            ));
        }
        if self.parsed_rows < self.data_lines() {
            return Some(format!(
                "Read {} rows but the file has {} data lines ({} non-empty lines − 1 header − {} title lines). Check for unclosed quotes or broken lines.",
                format_with_commas(self.parsed_rows as f64, 0),
                format_with_commas(self.data_lines() as f64, 0),
                format_with_commas(self.nonempty_lines as f64, 0),
                format_with_commas(self.skip_rows as f64, 0)
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
            format_with_commas(self.rows as f64, 0),
            format_with_commas(self.left_rows as f64, 0),
            fmt_sum(self.sum),
            fmt_sum(self.left_sum)
        );
        if self.duplicate_keys > 0 {
            out.push_str(&format!(
                " ⚠ kept first row for {} duplicated right key(s), {} right row(s) dropped",
                format_with_commas(self.duplicate_keys as f64, 0),
                format_with_commas(self.dropped_rows as f64, 0)
            ));
        }
        out
    }
}

impl TabState {
    /// True when the pipeline has a filter step that is turned on.
    pub fn has_active_filter(&self) -> bool {
        self.steps.iter().any(|s| s.is_active_filter())
    }

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
    /// The tab's summary choices per column (columns left out use their default).
    #[serde(default)]
    pub summary: HashMap<String, SummarySpec>,
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
