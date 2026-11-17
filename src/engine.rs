use crate::banking::{
    apply_banking_hygiene_audit, apply_duration_analysis, apply_entity_normalizer,
    apply_ftp_curve_map, apply_loan_classification_provision, apply_loan_limit_utilization,
    apply_maturity_bucket, apply_rate_shock_sim, build_period_reconcile_lazy,
    ensure_date, weighted_rate_exprs, FTP_MISSING_DATE,
};
use crate::models::{
    column_name_tokens, parquet_native_type, parquet_needs_conversion, portable_root_dir, AggFunc, AggSpec, CondOp,
    FilterCondition, GroupJoin,
    ColDataType, ColumnSchemaConfig, ControlTotals, CsvTextEncoding, LineDetail, LineIssue, RowCountCheck, ROW_DETAILS_LIMIT, FilterOp, IngestionConfig, JoinStatus, MathOp,
    ParquetDialect, ParseFailure, PiiMaskMode, SignConvention, SqlJoinKind, TabSource, TabState,
    TransformStep,
};
use anyhow::{Context, Result};
use chrono::NaiveDate;
use polars::prelude::*;
use polars::sql::SQLContext;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

/// First rows of a CSV as text, read with the separator, title-line skip and encoding that
/// auto-detection picks for the file.
pub fn preview_csv_raw(path: &Path, n_rows: usize) -> Result<DataFrame> {
    let settings = detect_csv_read_settings(path)?;
    preview_csv_with(path, n_rows, &settings)
}

/// First `n_rows` data rows as text, read with `config`'s separator, skip_rows and encoding.
pub fn preview_csv_with(path: &Path, n_rows: usize, config: &IngestionConfig) -> Result<DataFrame> {
    let sep = separator_byte(config.separator)?;
    // Only the head of the file is read: title lines, header and n_rows lines (a quoted field
    // spanning lines can make the preview a little shorter).
    let lines = read_head_lines(path, config.skip_rows + 1 + n_rows)?;
    let mut text = String::new();
    for raw in lines.iter().skip(config.skip_rows) {
        text.push_str(&decode_line(raw, config.encoding));
        text.push('\n');
    }
    if text.is_empty() {
        anyhow::bail!(
            "The file has no header row after skipping {} title line(s).",
            config.skip_rows
        );
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text).to_string();

    let parse_opts = CsvParseOptions::default()
        .with_separator(sep)
        .with_encoding(CsvEncoding::LossyUtf8)
        .with_truncate_ragged_lines(true);

    let mut df = CsvReadOptions::default()
        .with_parse_options(parse_opts)
        .with_infer_schema_length(Some(0))
        .with_ignore_errors(true)
        .with_n_rows(Some(n_rows))
        .into_reader_with_file_handle(std::io::Cursor::new(text.into_bytes()))
        .finish()
        .context("Failed to read CSV preview rows")?;
    strip_bom_from_first_name(&mut df);
    Ok(df)
}

/// Separator, title lines (0) and encoding for a file: Windows-1252 when the head of the file
/// is not valid UTF-8, and the most frequent of , | ; tab outside quotes in the header line.
pub fn detect_csv_read_settings(path: &Path) -> Result<IngestionConfig> {
    let mut head = Vec::new();
    File::open(path)
        .with_context(|| format!("Failed to open {:?}", path))?
        .take(64 * 1024)
        .read_to_end(&mut head)
        .with_context(|| format!("Failed to read {:?}", path))?;
    let encoding = match std::str::from_utf8(&head) {
        Ok(_) => CsvTextEncoding::Utf8,
        // Cut off in the middle of a character at the 64 KB mark: still UTF-8.
        Err(e) if e.error_len().is_none() => CsvTextEncoding::Utf8,
        Err(_) => CsvTextEncoding::Windows1252,
    };
    let header = read_head_lines(path, 1)?
        .first()
        .map(|l| decode_line(l, encoding))
        .unwrap_or_default();
    Ok(IngestionConfig {
        separator: detect_separator(&header),
        encoding,
        ..Default::default()
    })
}

/// The candidate (, | ; tab) that occurs most often outside quotes; ',' when none does.
pub fn detect_separator(header_line: &str) -> char {
    let mut best = (',', 0usize);
    for cand in [',', '|', ';', '\t'] {
        let n = count_unquoted(header_line, cand);
        if n > best.1 {
            best = (cand, n);
        }
    }
    best.0
}

/// `auto_detect_smart_schema` with the separator, skip_rows and encoding detected for the file.
pub fn auto_detect_smart_schema(path: &Path) -> Result<IngestionConfig> {
    let settings = detect_csv_read_settings(path)?;
    auto_detect_smart_schema_with(path, &settings)
}

/// Infers column types from the first 200 rows, read with `base`'s separator, skip_rows and
/// encoding. The other settings of `base` are kept; its columns are replaced.
pub fn auto_detect_smart_schema_with(path: &Path, base: &IngestionConfig) -> Result<IngestionConfig> {
    let sample_df = preview_csv_with(path, 200, base)?;
    let preview_5 = sample_df.head(Some(5));
    let mut columns = Vec::with_capacity(sample_df.width());

    for series in sample_df.get_columns() {
        let col_name = series.name().to_string();
        let mut samples = Vec::new();
        if let Ok(ca) = preview_5.column(&col_name).and_then(|c| c.str()) {
            for val in ca.into_iter().take(5) {
                samples.push(val.unwrap_or("null").to_string());
            }
        }

        let dtype = infer_safe_column_type(&col_name, series);
        let has_dr_cr = dtype == ColDataType::Float64
            && series
                .str()
                .map(|ca| ca.into_iter().flatten().any(amount_has_dr_cr))
                .unwrap_or(false);
        columns.push(ColumnSchemaConfig {
            name: col_name,
            dtype,
            pii_mask: PiiMaskMode::None,
            sample_values: samples,
            sign_convention: SignConvention::Unset,
            has_dr_cr,
            source_dtype: String::new(),
        });
    }

    Ok(IngestionConfig {
        columns,
        ..base.clone()
    })
}

fn infer_safe_column_type(col_name: &str, series: &Series) -> ColDataType {
    if TabState::is_likely_id_column(col_name) {
        return ColDataType::Utf8;
    }

    let Ok(ca) = series.str() else {
        return ColDataType::Utf8;
    };

    let mut non_empty = 0;
    let mut has_leading_zero = false;
    let mut all_float = true;
    let mut all_mdy_date = true;
    let mut all_dmy_date = true;
    let mut all_iso_date = true;

    for opt_val in ca.into_iter().flatten() {
        let trimmed = opt_val.trim();
        if trimmed.is_empty()
            || trimmed == "-"
            || trimmed.eq_ignore_ascii_case("null")
            || trimmed.eq_ignore_ascii_case("none")
            || trimmed.eq_ignore_ascii_case("na")
            || trimmed.eq_ignore_ascii_case("n/a")
        {
            continue;
        }
        non_empty += 1;

        if trimmed.len() > 1
            && trimmed.starts_with('0')
            && trimmed.bytes().all(|b| b.is_ascii_digit())
        {
            has_leading_zero = true;
        }

        // Any sign convention will do here: only "is this a number" matters.
        if !matches!(
            parse_banking_amount(trimmed, SignConvention::CrPositive, true),
            AmountCell::Value(_)
        ) {
            all_float = false;
        }

        let fit = classify_date_value(trimmed);
        all_iso_date &= fit.iso;
        all_dmy_date &= fit.dmy;
        all_mdy_date &= fit.mdy;
    }

    // Order: empty -> ISO date -> D/M/Y vs M/D/Y -> float -> text
    if non_empty == 0 {
        // Blank in every sampled row: a date by name (Closure_Date, Expiry_Dt, Open...),
        // otherwise text.
        const DATE_TOKENS: &[&str] = &[
            "date", "dt", "dat", "maturity", "expiry", "closure", "close", "open",
        ];
        let tokens = column_name_tokens(col_name);
        if tokens.iter().any(|t| DATE_TOKENS.contains(&t.as_str())) {
            return ColDataType::DateDMY;
        }
        return ColDataType::Utf8;
    }
    if all_iso_date {
        return ColDataType::DateISO;
    }
    // Bangladesh CBS exports are D/M/Y: when both readings fit every value, choose D/M/Y.
    // M/D/Y only wins when some value can't be D/M/Y (its second part is a day > 12).
    if all_dmy_date {
        return ColDataType::DateDMY;
    }
    if all_mdy_date {
        return ColDataType::DateMDY;
    }
    if has_leading_zero {
        return ColDataType::Utf8;
    }
    if all_float {
        return ColDataType::Float64;
    }

    ColDataType::Utf8
}

/// One amount cell after parsing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AmountCell {
    Value(f64),
    /// Blank, NA, N/A or NULL: a genuine null, not a parse failure.
    Blank,
    /// Not a number under the parsing rules; it becomes null and counts as a parse failure.
    Invalid,
    /// Carries Dr/Cr but the column's sign convention is not set.
    NeedsSignConvention,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DrCr {
    Dr,
    Cr,
}

/// Blank, NA, N/A or NULL (any case) after trimming.
pub fn is_blank_amount_token(s: &str) -> bool {
    let s = strip_leading_apostrophe(s);
    s.is_empty()
        || s.eq_ignore_ascii_case("na")
        || s.eq_ignore_ascii_case("n/a")
        || s.eq_ignore_ascii_case("null")
}

/// CBS exports put one apostrophe in front of a value to protect it from Excel ("'0012",
/// "'1,234.50"). Number and date parsing drops that one apostrophe; text columns are never
/// touched. Returns the trimmed value without it.
fn strip_leading_apostrophe(s: &str) -> &str {
    let s = s.trim();
    s.strip_prefix('\'').unwrap_or(s).trim()
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        s.get(prefix.len()..)
    } else {
        None
    }
}

fn strip_suffix_ci<'a>(s: &'a str, suffix: &str) -> Option<&'a str> {
    let cut = s.len().checked_sub(suffix.len())?;
    let tail = s.get(cut..)?;
    if tail.eq_ignore_ascii_case(suffix) {
        s.get(..cut)
    } else {
        None
    }
}

/// Removes one currency prefix: BDT, TK, Tk. or ৳ (any case, optional space after it).
fn strip_currency_prefix(s: &str) -> &str {
    for p in ["৳", "bdt", "tk.", "tk"] {
        if let Some(rest) = strip_prefix_ci(s, p) {
            return rest.trim_start();
        }
    }
    s
}

/// Splits off a Dr/Cr marker (Dr, Dr., CR, any case, optional space) at either end.
fn split_dr_cr(s: &str) -> (Option<DrCr>, &str) {
    const MARKERS: [(&str, DrCr); 4] = [
        ("dr.", DrCr::Dr),
        ("cr.", DrCr::Cr),
        ("dr", DrCr::Dr),
        ("cr", DrCr::Cr),
    ];
    for (m, kind) in MARKERS {
        if let Some(rest) = strip_suffix_ci(s, m) {
            return (Some(kind), rest.trim());
        }
    }
    for (m, kind) in MARKERS {
        if let Some(rest) = strip_prefix_ci(s, m) {
            return (Some(kind), rest.trim());
        }
    }
    (None, s)
}

/// True when the value carries a Dr/Cr marker (after any currency prefix).
pub fn amount_has_dr_cr(raw: &str) -> bool {
    split_dr_cr(strip_currency_prefix(strip_leading_apostrophe(raw))).0.is_some()
}

/// Cleans an amount down to an unsigned digit string and its sign.
/// Order: currency prefix, Dr/Cr marker, thousands commas, brackets, trailing minus, lone "-".
fn clean_amount(
    raw: &str,
    sign: SignConvention,
    accounting_negatives: bool,
) -> std::result::Result<(String, bool), AmountCell> {
    let s = strip_leading_apostrophe(raw);
    if is_blank_amount_token(s) {
        return Err(AmountCell::Blank);
    }
    let s = strip_currency_prefix(s);
    let (marker, s) = split_dr_cr(s);
    if marker.is_some() && sign == SignConvention::Unset {
        return Err(AmountCell::NeedsSignConvention);
    }
    let s = strip_currency_prefix(s.trim());
    if s == "-" {
        return Ok(("0".to_string(), false));
    }
    let s = s.replace(',', "");
    let mut body = s.trim();
    let mut negative = false;
    if accounting_negatives {
        if let Some(inner) = body.strip_prefix('(').and_then(|b| b.strip_suffix(')')) {
            body = strip_currency_prefix(inner.trim());
            negative = true;
        } else if body.len() > 1 {
            if let Some(inner) = body.strip_suffix('-') {
                body = inner.trim();
                negative = true;
            }
        }
    }
    if let Some(rest) = body.strip_prefix('-') {
        if negative {
            return Err(AmountCell::Invalid);
        }
        negative = true;
        body = rest;
    } else if let Some(rest) = body.strip_prefix('+') {
        body = rest;
    }
    let digits = body.bytes().filter(|b| b.is_ascii_digit()).count();
    let dots = body.bytes().filter(|b| *b == b'.').count();
    if digits == 0 || dots > 1 || digits + dots != body.len() {
        return Err(AmountCell::Invalid);
    }
    match marker {
        None => Ok((body.to_string(), negative)),
        // A Dr/Cr marker on a value that already has a minus or brackets is ambiguous.
        Some(_) if negative => Err(AmountCell::Invalid),
        Some(m) => {
            let positive_side = match sign {
                SignConvention::DrPositive => DrCr::Dr,
                _ => DrCr::Cr,
            };
            Ok((body.to_string(), m != positive_side))
        }
    }
}

/// Parses one amount: currency prefix (BDT, TK, Tk., ৳), thousands commas, brackets and a
/// trailing minus (when `accounting_negatives`), Dr/Cr per `sign`, "-" = 0, and blank / NA /
/// N/A / NULL = blank. Anything else is `Invalid`.
pub fn parse_banking_amount(
    raw: &str,
    sign: SignConvention,
    accounting_negatives: bool,
) -> AmountCell {
    match clean_amount(raw, sign, accounting_negatives) {
        Ok((body, negative)) => match body.parse::<f64>() {
            Ok(v) if v.is_finite() => AmountCell::Value(if negative { -v } else { v }),
            _ => AmountCell::Invalid,
        },
        Err(cell) => cell,
    }
}

/// Whole-number version of `parse_banking_amount` (a decimal point makes it `Invalid`).
fn parse_banking_int(raw: &str, sign: SignConvention, accounting_negatives: bool) -> Option<i64> {
    let (body, negative) = clean_amount(raw, sign, accounting_negatives).ok()?;
    let v: i64 = body.parse().ok()?;
    Some(if negative { -v } else { v })
}

fn dr_cr_unset_message(col_name: &str) -> String {
    format!(
        "Column '{}' has Dr/Cr amounts. Choose 'Dr is +' or 'Cr is +' for it in Schema & Ingestion.",
        col_name
    )
}

/// What one pass over a source's typed columns found.
#[derive(Clone, Debug, Default)]
pub struct TypedScan {
    /// Float64 columns with no sign convention whose values carry Dr/Cr (in schema order).
    pub dr_cr_columns: Vec<String>,
    /// Values that could not be converted, per typed column (columns with none are left out).
    pub failures: Vec<ParseFailure>,
}

type ScanCache = std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<TypedScan>>>;

thread_local! {
    static TYPED_SCAN_RUNS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many full typed-column scans this thread has run (cache hits do not count). For tests.
pub fn typed_scan_runs_on_this_thread() -> usize {
    TYPED_SCAN_RUNS.with(|c| c.get())
}

fn scan_cache() -> &'static ScanCache {
    static CACHE: std::sync::OnceLock<ScanCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Size and modified time of a file, as text, for cache keys. `None` when it cannot be read.
fn file_stamp(path: &Path) -> Option<String> {
    let m = std::fs::metadata(path).ok()?;
    let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(format!("{}|{}|{}", path.display(), m.len(), t.as_nanos()))
}

/// The Dr/Cr check and the parse-failure counts need the same full scan of the typed columns.
/// One aggregation pass answers both, and its answer is kept per (file, size, modified time,
/// schema), so building a pipeline and opening the tab scan the file once between them.
pub fn typed_scan(
    path: &Path,
    is_parquet: bool,
    config: &IngestionConfig,
    temp_dir: &Path,
) -> Result<std::sync::Arc<TypedScan>> {
    let key = file_stamp(path).and_then(|st| {
        serde_json::to_string(config).ok().map(|cfg| format!("{}|{}|{}", st, is_parquet, cfg))
    });
    if let Some(k) = &key {
        if let Some(hit) = scan_cache().lock().ok().and_then(|c| c.get(k).cloned()) {
            return Ok(hit);
        }
    }
    TYPED_SCAN_RUNS.with(|c| c.set(c.get() + 1));
    let result = std::sync::Arc::new(run_typed_scan(path, is_parquet, config, temp_dir)?);
    if let Some(k) = key {
        if let Ok(mut c) = scan_cache().lock() {
            if c.len() >= 64 {
                c.clear();
            }
            c.insert(k, result.clone());
        }
    }
    Ok(result)
}

fn run_typed_scan(
    path: &Path,
    is_parquet: bool,
    config: &IngestionConfig,
    temp_dir: &Path,
) -> Result<TypedScan> {
    // Per column: the schema entry, the raw text (for the Dr/Cr check) and the text the typed
    // value is parsed from (for the failure count).
    let (lf, entries): (LazyFrame, Vec<(&ColumnSchemaConfig, Expr, Expr)>) = if is_parquet {
        let (lf, plan) = parquet_conversion_plan(path, config)?;
        (lf, plan.into_iter().map(|(c, e)| (c, e.clone(), e)).collect())
    } else {
        let lf = scan_csv_text(path, config, temp_dir)?;
        let entries = config
            .columns
            .iter()
            .map(|c| (c, col(&c.name), raw_text_expr(c, config)))
            .collect();
        (lf, entries)
    };

    let accounting_negatives = config.parse_accounting_negatives;
    let mut exprs: Vec<Expr> = Vec::new();
    for (i, (c, raw, text)) in entries.iter().enumerate() {
        if c.dtype == ColDataType::Float64 && c.sign_convention == SignConvention::Unset {
            exprs.push(
                raw.clone()
                    .map(
                        |s: Series| {
                            let ca = s.str()?;
                            let flags: BooleanChunked = ca
                                .into_iter()
                                .map(|v| Some(v.is_some_and(amount_has_dr_cr)))
                                .collect();
                            Ok(Some(flags.into_series()))
                        },
                        GetOutput::from_type(DataType::Boolean),
                    )
                    .any(true)
                    .alias(format!("__dc_{i}")),
            );
        }
        if c.dtype != ColDataType::Utf8 {
            let (dtype, sign) = (c.dtype, c.sign_convention);
            let mask = text.clone().map(
                move |s: Series| {
                    let ca = s.str()?;
                    let flags: BooleanChunked = ca
                        .into_iter()
                        .map(|v| Some(v.is_some_and(|v| is_parse_failure(v, dtype, sign, accounting_negatives))))
                        .collect();
                    Ok(Some(flags.into_series()))
                },
                GetOutput::from_type(DataType::Boolean),
            );
            exprs.push(mask.clone().sum().cast(DataType::UInt64).alias(format!("__pf_n_{i}")));
            exprs.push(text.clone().filter(mask).head(Some(5)).implode().alias(format!("__pf_x_{i}")));
        }
    }
    if exprs.is_empty() {
        return Ok(TypedScan::default());
    }
    let df = lf
        .select(exprs)
        .collect()
        .context("Failed to scan the file's amount and date columns")?;

    let mut out = TypedScan::default();
    for (i, (c, _, _)) in entries.iter().enumerate() {
        let found_dr_cr = df
            .column(&format!("__dc_{i}"))
            .ok()
            .and_then(|s| s.bool().ok().and_then(|b| b.get(0)))
            .unwrap_or(false);
        if found_dr_cr {
            out.dr_cr_columns.push(c.name.clone());
        }
        let count = df
            .column(&format!("__pf_n_{i}"))
            .ok()
            .and_then(|s| s.u64().ok().and_then(|ca| ca.get(0)))
            .unwrap_or(0) as usize;
        if count == 0 {
            continue;
        }
        let mut examples = Vec::new();
        if let Some(list) = df
            .column(&format!("__pf_x_{i}"))
            .ok()
            .and_then(|s| s.list().ok().and_then(|l| l.get_as_series(0)))
        {
            if let Ok(ca) = list.str() {
                examples = ca.into_iter().flatten().map(|v| v.to_string()).collect();
            }
        }
        out.failures.push(ParseFailure { column: c.name.clone(), count, examples });
    }
    Ok(out)
}

/// Fails when a Float64 column with no sign convention carries Dr/Cr values.
fn check_dr_cr_conventions(path: &Path, is_parquet: bool, config: &IngestionConfig, temp_dir: &Path) -> Result<()> {
    let needs_check = config
        .columns
        .iter()
        .any(|c| c.dtype == ColDataType::Float64 && c.sign_convention == SignConvention::Unset);
    if !needs_check {
        return Ok(());
    }
    let scan = typed_scan(path, is_parquet, config, temp_dir)?;
    if let Some(c) = scan.dr_cr_columns.first() {
        anyhow::bail!(dr_cr_unset_message(c));
    }
    Ok(())
}

// ------------------------------------------------------------------- step validation (T4)

/// A step that cannot run against the columns it would see.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepIssue {
    /// Position in the steps list (0-based).
    pub index: usize,
    /// What is wrong, in words that name the column.
    pub reason: String,
}

/// Polars says `unable to find column "X"; valid columns: [...]`: keep the part that matters.
fn friendly_step_error(e: &anyhow::Error) -> String {
    let full = format!("{:#}", e);
    if let Some(i) = full.find("unable to find column \"") {
        let rest = &full[i + "unable to find column \"".len()..];
        if let Some(j) = rest.find('"') {
            return format!("Column '{}' does not exist at this step.", &rest[..j]);
        }
    }
    full.lines().next().unwrap_or("").to_string()
}

/// Checks every step against the columns it would see, in order. A broken step does not change
/// the columns for the steps after it, so each later step is judged on its own (a step that needs
/// a column the broken one would have made is reported too). Turned-off steps are not checked.
/// Fails only when the source itself cannot be read.
pub fn validate_steps(source: &TabSource, steps: &[TransformStep], temp_dir: &Path) -> Result<Vec<StepIssue>> {
    let mut lf = compile_tab_lazy(source, &[], temp_dir)?;
    // A source that cannot be read is an error, not a broken first step.
    lf.collect_schema().map_err(anyhow::Error::from)?;
    let mut issues = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        if step.is_skipped() {
            continue;
        }
        let attempt = apply_single_step(lf.clone(), step).and_then(|next| {
            let mut probe = next.clone();
            probe.collect_schema().map(|_| next).map_err(anyhow::Error::from)
        });
        match attempt {
            Ok(next) => lf = next,
            Err(e) => issues.push(StepIssue { index: i, reason: friendly_step_error(&e) }),
        }
    }
    Ok(issues)
}

// ------------------------------------------------------------ Text columns of numbers (L4)

/// A share of non-blank values at or above this makes a Text column "look like numbers".
pub const TEXT_NUMBER_SHARE: f64 = 0.95;

/// A Text column in an export whose values mostly parse as numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct NumericLookingColumn {
    pub column: String,
    /// Non-blank values.
    pub non_blank: usize,
    /// Of those, values that parse as numbers (the amount rules, incl. a leading apostrophe).
    pub numeric: usize,
}

impl NumericLookingColumn {
    /// Whole percent, rounded.
    pub fn percent(&self) -> usize {
        if self.non_blank == 0 {
            0
        } else {
            ((self.numeric as f64 * 100.0) / self.non_blank as f64).round() as usize
        }
    }

    /// The question shown before an export.
    pub fn message(&self) -> String {
        format!(
            "{} is Text, but {}% of its values look like numbers. Set it to Number before exporting?",
            self.column,
            self.percent()
        )
    }
}

/// Text columns of the result where at least 95% of the non-blank values parse as numbers. ID
/// columns (Phase 1 name tokens: account numbers, branch codes, CIF ...) are left out: their
/// leading zeros are meaningful. One aggregation pass over the Text columns.
pub fn text_columns_that_look_numeric(lf: LazyFrame) -> Result<Vec<NumericLookingColumn>> {
    let mut lf = lf;
    let schema = lf.collect_schema().context("Could not read the column types for the Text check")?;
    let text_cols: Vec<String> = schema
        .iter()
        .filter(|(n, dt)| **dt == DataType::String && !TabState::is_likely_id_column(n.as_str()))
        .map(|(n, _)| n.to_string())
        .collect();
    if text_cols.is_empty() {
        return Ok(Vec::new());
    }
    let mut exprs: Vec<Expr> = Vec::with_capacity(text_cols.len() * 2);
    for (i, name) in text_cols.iter().enumerate() {
        let counted = |numeric: bool| {
            col(name.as_str()).map(
                move |s: Series| {
                    let ca = s.str()?;
                    let flags: UInt32Chunked = ca
                        .into_iter()
                        .map(|v| {
                            Some(match v {
                                None => 0u32,
                                Some(v) if is_blank_amount_token(v) => 0,
                                Some(v) => {
                                    if numeric {
                                        matches!(
                                            parse_banking_amount(v, SignConvention::Unset, true),
                                            AmountCell::Value(_)
                                        ) as u32
                                    } else {
                                        1
                                    }
                                }
                            })
                        })
                        .collect();
                    Ok(Some(flags.into_series()))
                },
                GetOutput::from_type(DataType::UInt32),
            )
        };
        exprs.push(counted(false).sum().cast(DataType::UInt64).alias(format!("__tn_b_{i}")));
        exprs.push(counted(true).sum().cast(DataType::UInt64).alias(format!("__tn_n_{i}")));
    }
    let df = lf.select(exprs).collect().context("Failed to check the Text columns for numbers")?;
    let mut out = Vec::new();
    for (i, name) in text_cols.iter().enumerate() {
        let get = |p: &str| {
            df.column(&format!("{p}{i}"))
                .ok()
                .and_then(|s| s.u64().ok().and_then(|ca| ca.get(0)))
                .unwrap_or(0) as usize
        };
        let (non_blank, numeric) = (get("__tn_b_"), get("__tn_n_"));
        if non_blank > 0 && (numeric as f64) >= TEXT_NUMBER_SHARE * non_blank as f64 {
            out.push(NumericLookingColumn { column: name.clone(), non_blank, numeric });
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------------ baseline control sum

/// Where `source_baseline_sum` got its answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaselineOrigin {
    Cached,
    Computed,
}

type BaselineCache = std::sync::Mutex<std::collections::HashMap<String, Option<f64>>>;

fn baseline_cache() -> &'static BaselineCache {
    static CACHE: std::sync::OnceLock<BaselineCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Identity of a source's raw data for the baseline cache: every file's path, size and modified
/// time, plus the schema and the control column (a different schema or column is a different
/// number). Joined and reconciled sources are not cached.
fn baseline_key(source: &TabSource, control_col: &str) -> Option<String> {
    let one = |path: &Path, is_parquet: bool, config: &IngestionConfig| -> Option<String> {
        let cfg = serde_json::to_string(config).ok()?;
        Some(format!("{}|{}|{}", file_stamp(path)?, is_parquet, cfg))
    };
    let body = match source {
        TabSource::SingleFile { path, is_parquet, config } => one(path, *is_parquet, config)?,
        TabSource::VerticalStack { files, include_source_col } => {
            let mut parts = vec![format!("stack|{}", include_source_col)];
            for (p, pq, cfg) in files {
                parts.push(one(p, *pq, cfg)?);
            }
            parts.join("||")
        }
        _ => return None,
    };
    Some(format!("{}##{}", body, control_col))
}

/// Control sum of the raw source (no steps). It is computed once per unchanged file: the answer
/// is kept per (path, size, modified time, schema, column), so a second tab on the same file does
/// not read it again. No control column means no sum, and no scan.
pub fn source_baseline_sum(
    source: &TabSource,
    control_col: Option<&str>,
    temp_dir: &Path,
) -> (Option<f64>, BaselineOrigin) {
    let Some(ctrl) = control_col else {
        return (None, BaselineOrigin::Computed);
    };
    let key = baseline_key(source, ctrl);
    if let Some(k) = &key {
        if let Some(hit) = baseline_cache().lock().ok().and_then(|c| c.get(k).copied()) {
            return (hit, BaselineOrigin::Cached);
        }
    }
    let computed = compile_tab_lazy(source, &[], temp_dir)
        .ok()
        .and_then(|lf| compute_control_totals(lf, Some(ctrl)).ok())
        .map(|(_, s)| s);
    if let (Some(k), Some(sum)) = (key, computed) {
        remember(k, sum);
    }
    (computed.flatten(), BaselineOrigin::Computed)
}

/// Stores a baseline the caller already has (a tab with no steps has just summed the raw source).
pub fn remember_baseline(source: &TabSource, control_col: Option<&str>, sum: Option<f64>) {
    if let Some(k) = control_col.and_then(|c| baseline_key(source, c)) {
        remember(k, sum);
    }
}

fn remember(key: String, sum: Option<f64>) {
    if let Ok(mut c) = baseline_cache().lock() {
        if c.len() >= 256 {
            c.clear();
        }
        c.insert(key, sum);
    }
}

/// Ultra-fast native Rust date parser that handles single-digit or double-digit months/days
/// (e.g., "2/8/2024", "02/08/2024", "11/8/2026", "11/30/2023", "31-Oct-2026", "2024-02-08",
/// 2-digit years, and trailing timestamps) and returns days since 1970-01-01.
pub fn parse_flexible_banking_date(raw: &str, mode: ColDataType) -> Option<i32> {
    if is_blank_date_token(raw) {
        return None;
    }
    let s = raw.trim().trim_matches('"').trim_matches('\'').trim();

    // Strip trailing timestamp if present (e.g. "02/08/2024 00:00:00" or "2024-02-08T12:30:00")
    let date_part = strip_time_part(s);

    let mut parts = date_part.split(['/', '-', '.']);
    let p1_str = parts.next()?.trim();
    let p2_str = parts.next()?.trim();
    let p3_str = parts.next()?.trim();
    if !all_ascii_digits(p1_str) || !all_ascii_digits(p3_str) {
        return None;
    }

    let p1: i32 = p1_str.parse().ok()?;
    let p2_month_name = parse_month_name(p2_str);
    let p2: u32 = match p2_month_name {
        Some(m) => m,
        None if all_ascii_digits(p2_str) => p2_str.parse().ok()?,
        None => return None,
    };
    let p3: u32 = p3_str.parse().ok()?;

    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1)?;

    // Case 1: First token is a 4-digit year (YYYY-MM-DD or YYYY/MM/DD)
    if p1_str.len() == 4 {
        if !(1900..=2200).contains(&p1) {
            return None;
        }
        let (m, d) = if p2 <= 12 { (p2, p3) } else { (p3, p2) };
        let dt = NaiveDate::from_ymd_opt(p1, m, d)?;
        return Some((dt - epoch).num_days() as i32);
    }

    // Case 2: Third token is the year: two digits (00-69 -> 20xx, 70-99 -> 19xx) or four.
    let year = year_from_token(p3_str)?;

    // If middle token was a text month (e.g. "08-Feb-2024"), p1 is always day and p2 is month
    if p2_month_name.is_some() {
        let dt = NaiveDate::from_ymd_opt(year, p2, p1 as u32)?;
        return Some((dt - epoch).num_days() as i32);
    }

    let u1 = p1 as u32;
    let u2 = p2;

    let dt = match mode {
        ColDataType::DateMDY => {
            // Try M/D/Y first; if u1 > 12 (e.g. 30/11/2023), automatically fall back to D/M/Y
            NaiveDate::from_ymd_opt(year, u1, u2)
                .or_else(|| NaiveDate::from_ymd_opt(year, u2, u1))?
        }
        ColDataType::DateDMY => {
            // Try D/M/Y first; if u2 > 12 (e.g. 11/30/2023), automatically fall back to M/D/Y
            NaiveDate::from_ymd_opt(year, u2, u1)
                .or_else(|| NaiveDate::from_ymd_opt(year, u1, u2))?
        }
        _ => NaiveDate::from_ymd_opt(year, u1, u2)
            .or_else(|| NaiveDate::from_ymd_opt(year, u2, u1))?,
    };

    Some((dt - epoch).num_days() as i32)
}

fn all_ascii_digits(t: &str) -> bool {
    !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit())
}

/// A trailing year token: two digits pivot at 70 (00-69 -> 2000-2069, 70-99 -> 1970-1999);
/// four digits must be 1900-2200. Any other length is not a year.
fn year_from_token(t: &str) -> Option<i32> {
    if !all_ascii_digits(t) {
        return None;
    }
    let y: i32 = t.parse().ok()?;
    match t.len() {
        2 if y < 70 => Some(2000 + y),
        2 => Some(1900 + y),
        4 if (1900..=2200).contains(&y) => Some(y),
        _ => None,
    }
}

/// A date cell that means "no date": blank, "-", "0", NULL, NONE, NA or N/A (any case).
pub fn is_blank_date_token(raw: &str) -> bool {
    let s = raw.trim().trim_matches('"').trim_matches('\'').trim();
    s.is_empty()
        || s == "-"
        || s == "0"
        || s.eq_ignore_ascii_case("null")
        || s.eq_ignore_ascii_case("none")
        || s.eq_ignore_ascii_case("na")
        || s.eq_ignore_ascii_case("n/a")
}

/// Drops a trailing time: everything after a space, or after a 'T' between two digits
/// (ISO 8601). A 'T' inside a month name such as "31-OCT-2026" is kept.
fn strip_time_part(s: &str) -> &str {
    let s = s.split(' ').next().unwrap_or(s);
    let b = s.as_bytes();
    for i in 1..b.len().saturating_sub(1) {
        if (b[i] == b'T' || b[i] == b't') && b[i - 1].is_ascii_digit() && b[i + 1].is_ascii_digit() {
            return &s[..i];
        }
    }
    s
}

/// Which strict date layouts a single sampled value fits (no day/month swapping).
#[derive(Default, Clone, Copy)]
struct DateFit {
    iso: bool,
    dmy: bool,
    mdy: bool,
}

fn classify_date_value(raw: &str) -> DateFit {
    let mut fit = DateFit::default();
    let s = strip_time_part(raw.trim().trim_matches('"').trim_matches('\'').trim());
    let parts: Vec<&str> = s.split(['/', '-', '.']).map(str::trim).collect();
    let [a, b, c] = parts.as_slice() else {
        return fit;
    };
    let all_digits = |t: &str| !t.is_empty() && t.bytes().all(|x| x.is_ascii_digit());

    // ISO: YYYY-MM-DD with a four-digit year first
    if a.len() == 4 && all_digits(a) {
        if let (Ok(y), Ok(m), Ok(d)) = (a.parse::<i32>(), b.parse::<u32>(), c.parse::<u32>()) {
            fit.iso = (1900..=2200).contains(&y) && NaiveDate::from_ymd_opt(y, m, d).is_some();
        }
        return fit;
    }

    // Year last: two or four digits
    let Some(year) = year_from_token(c) else {
        return fit;
    };
    if !all_digits(a) {
        return fit;
    }
    let Ok(p1) = a.parse::<u32>() else {
        return fit;
    };
    if let Some(m) = parse_month_name(b) {
        fit.dmy = NaiveDate::from_ymd_opt(year, m, p1).is_some();
        return fit;
    }
    if !all_digits(b) {
        return fit;
    }
    let Ok(p2) = b.parse::<u32>() else {
        return fit;
    };
    fit.dmy = NaiveDate::from_ymd_opt(year, p2, p1).is_some();
    fit.mdy = NaiveDate::from_ymd_opt(year, p1, p2).is_some();
    fit
}

fn parse_month_name(s: &str) -> Option<u32> {
    match s.to_lowercase().as_str() {
        "jan" | "january" => Some(1),
        "feb" | "february" => Some(2),
        "mar" | "march" => Some(3),
        "apr" | "april" => Some(4),
        "may" => Some(5),
        "jun" | "june" => Some(6),
        "jul" | "july" => Some(7),
        "aug" | "august" => Some(8),
        "sep" | "sept" | "september" => Some(9),
        "oct" | "october" => Some(10),
        "nov" | "november" => Some(11),
        "dec" | "december" => Some(12),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Progress of the loops that read a file's bytes themselves (the line healer, the Windows-1252
// transcoder and the line counter). Polars reads files on its own and reports nothing, so those
// steps have no percentage.

/// Receives `(what, bytes read, file size)`; a file size of 0 means that loop is finished.
pub type ByteSink = Box<dyn Fn(&str, u64, u64)>;

thread_local! {
    static BYTE_SINK: std::cell::RefCell<Option<ByteSink>> = const { std::cell::RefCell::new(None) };
}

/// Runs `f` with `sink` receiving the byte progress of every such loop that runs on this thread.
/// The sink is removed afterwards, also when `f` panics.
pub fn with_byte_progress<R>(sink: impl Fn(&str, u64, u64) + 'static, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<ByteSink>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let prev = self.0.take();
            BYTE_SINK.with(|s| *s.borrow_mut() = prev);
        }
    }
    let prev = BYTE_SINK.with(|s| s.borrow_mut().replace(Box::new(sink)));
    let _restore = Restore(prev);
    f()
}

fn report_bytes(what: &str, read: u64, total: u64) {
    BYTE_SINK.with(|s| {
        if let Some(sink) = s.borrow().as_ref() {
            sink(what, read, total);
        }
    });
}

/// One loop over a file: reports the bytes read (at most every 256 KB and at the end) and, when
/// dropped, that the loop is finished (so no percentage stays on screen).
struct ByteLoop {
    what: &'static str,
    total: u64,
    read: u64,
    last_reported: u64,
}

impl ByteLoop {
    const STEP: u64 = 256 * 1024;

    fn new(what: &'static str, path: &Path) -> Self {
        let total = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        report_bytes(what, 0, total);
        Self { what, total, read: 0, last_reported: 0 }
    }

    fn add(&mut self, n: usize) {
        self.read += n as u64;
        if self.read - self.last_reported >= Self::STEP || self.read >= self.total {
            self.last_reported = self.read;
            report_bytes(self.what, self.read.min(self.total), self.total);
        }
    }
}

impl Drop for ByteLoop {
    fn drop(&mut self) {
        report_bytes(self.what, self.total, 0);
    }
}

/// A reader that counts the bytes taken from the file it wraps.
struct CountingReader<R> {
    inner: R,
    counter: std::rc::Rc<std::cell::Cell<u64>>,
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.counter.set(self.counter.get() + n as u64);
        Ok(n)
    }
}

/// Joins physical lines until each record has as many unquoted separators as the header.
/// The first `skip_rows` lines (titles above the header) are dropped.
pub fn heal_broken_csv_file(
    input_path: &Path,
    temp_dir: &Path,
    separator: u8,
    skip_rows: usize,
) -> Result<PathBuf> {
    let healed_path = temp_dir.join(format!(
        "healed_{}",
        input_path.file_name().unwrap_or_default().to_string_lossy()
    ));
    let counter = std::rc::Rc::new(std::cell::Cell::new(0u64));
    let reader = BufReader::new(CountingReader { inner: File::open(input_path)?, counter: counter.clone() });
    let mut writer = BufWriter::new(File::create(&healed_path)?);
    let sep = separator as char;
    let mut progress = ByteLoop::new("Healing broken lines", input_path);

    let mut lines = reader.lines().skip(skip_rows);
    let Some(Ok(header)) = lines.next() else {
        anyhow::bail!("CSV file is empty");
    };
    let expected = count_unquoted(&header, sep);
    writeln!(writer, "{}", header)?;

    let mut pending_line = String::new();
    for line_res in lines {
        let line = line_res?;
        // bytes the reader has taken from the file so far
        let seen = counter.get();
        progress.add(seen.saturating_sub(progress.read) as usize);
        if pending_line.is_empty() {
            pending_line = line;
        } else {
            pending_line.push(' ');
            pending_line.push_str(line.trim());
        }

        if count_unquoted(&pending_line, sep) >= expected {
            writeln!(writer, "{}", pending_line)?;
            pending_line.clear();
        }
    }
    if !pending_line.is_empty() {
        writeln!(writer, "{}", pending_line)?;
    }
    progress.add(counter.get().saturating_sub(progress.read) as usize);
    writer.flush()?;
    Ok(healed_path)
}

fn count_unquoted(s: &str, sep: char) -> usize {
    let mut in_quotes = false;
    let mut count = 0;
    for c in s.chars() {
        if c == '"' {
            in_quotes = !in_quotes;
        } else if c == sep && !in_quotes {
            count += 1;
        }
    }
    count
}

/// The separator as a byte; only , | ; and tab are accepted.
fn separator_byte(sep: char) -> Result<u8> {
    match sep {
        ',' | '|' | ';' | '\t' => Ok(sep as u8),
        other => anyhow::bail!(
            "Separator {:?} is not supported. Choose comma, pipe, semicolon or tab in Schema & Ingestion.",
            other
        ),
    }
}

/// Up to `n` raw lines from the start of a file (line endings removed).
fn read_head_lines(path: &Path, n: usize) -> Result<Vec<Vec<u8>>> {
    let mut reader =
        BufReader::new(File::open(path).with_context(|| format!("Failed to open {:?}", path))?);
    let mut out = Vec::new();
    while out.len() < n {
        let mut buf = Vec::new();
        if reader.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        while matches!(buf.last(), Some(b'\n' | b'\r')) {
            buf.pop();
        }
        out.push(buf);
    }
    Ok(out)
}

fn decode_line(raw: &[u8], encoding: CsvTextEncoding) -> String {
    match encoding {
        CsvTextEncoding::Utf8 => String::from_utf8_lossy(raw).into_owned(),
        CsvTextEncoding::Windows1252 => decode_windows_1252(raw),
    }
}

/// Windows-1252 to text: 0x00-0x7F and 0xA0-0xFF are the same code points; 0x80-0x9F use the
/// Windows-1252 table (the five unassigned bytes map to the C1 code point of the same value).
pub fn decode_windows_1252(bytes: &[u8]) -> String {
    const HIGH: [u32; 32] = [
        0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160,
        0x2039, 0x0152, 0x8D, 0x017D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022,
        0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
    ];
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => char::from_u32(HIGH[(b - 0x80) as usize]).unwrap_or('\u{FFFD}'),
            _ => b as char,
        })
        .collect()
}

/// Writes a Windows-1252 file as UTF-8 into the temp folder and returns the copy's path.
fn transcode_windows_1252(input_path: &Path, temp_dir: &Path) -> Result<PathBuf> {
    let out_path = temp_dir.join(format!(
        "utf8_{}",
        input_path.file_name().unwrap_or_default().to_string_lossy()
    ));
    let mut reader = BufReader::new(
        File::open(input_path).with_context(|| format!("Failed to open {:?}", input_path))?,
    );
    let mut writer = BufWriter::new(
        File::create(&out_path).with_context(|| format!("Failed to create {:?}", out_path))?,
    );
    let mut buf = vec![0u8; 64 * 1024];
    let mut progress = ByteLoop::new("Converting the file's encoding", input_path);
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(decode_windows_1252(&buf[..n]).as_bytes())?;
        progress.add(n);
    }
    writer.flush()?;
    Ok(out_path)
}

/// Removes a UTF-8 byte-order mark left on the first column name.
fn strip_bom_from_first_name(df: &mut DataFrame) {
    let first = df.get_column_names().first().map(|n| n.to_string());
    if let Some(name) = first {
        if let Some(clean) = name.strip_prefix('\u{feff}') {
            let clean = clean.to_string();
            let _ = df.rename(&name, clean.into());
        }
    }
}

pub fn build_lazy_pipeline(
    path: &Path,
    is_parquet: bool,
    config: &IngestionConfig,
    temp_dir: &Path,
) -> Result<LazyFrame> {
    if is_parquet {
        let (lf, plan) = parquet_conversion_plan(path, config)?;
        if plan.is_empty() {
            return Ok(lf);
        }
        check_dr_cr_conventions(path, true, config, temp_dir)?;
        let exprs: Vec<Expr> = plan
            .into_iter()
            .map(|(c, text)| typed_expr(text, c, config).alias(&c.name))
            .collect();
        return Ok(lf.with_columns(exprs));
    }

    let mut lf = scan_csv_text(path, config, temp_dir)?;
    check_dr_cr_conventions(path, false, config, temp_dir)?;

    let exprs: Vec<Expr> = config
        .columns
        .iter()
        .map(|c| typed_expr(raw_text_expr(c, config), c, config).alias(&c.name))
        .collect();
    if !exprs.is_empty() {
        lf = lf.with_columns(exprs);
    }

    Ok(lf)
}

/// Reads a CSV with every configured column as text (no typing yet).
fn scan_csv_text(path: &Path, config: &IngestionConfig, temp_dir: &Path) -> Result<LazyFrame> {
    let sep = separator_byte(config.separator)?;
    let mut actual_csv_path = match config.encoding {
        CsvTextEncoding::Utf8 => path.to_path_buf(),
        CsvTextEncoding::Windows1252 => transcode_windows_1252(path, temp_dir)?,
    };
    let mut skip_rows = config.skip_rows;
    if config.heal_broken_csv_lines {
        // The healer drops the title lines itself.
        actual_csv_path = heal_broken_csv_file(&actual_csv_path, temp_dir, sep, skip_rows)?;
        skip_rows = 0;
    }

    let mut schema = Schema::with_capacity(config.columns.len());
    for col_cfg in &config.columns {
        schema.with_column(col_cfg.name.clone().into(), DataType::String);
    }

    let mut lf = LazyCsvReader::new(&actual_csv_path)
        .with_separator(sep)
        .with_skip_rows(skip_rows)
        .with_dtype_overwrite(Some(Arc::new(schema)))
        .with_encoding(CsvEncoding::LossyUtf8)
        .with_ignore_errors(true)
        .with_truncate_ragged_lines(true)
        .with_try_parse_dates(false)
        .with_infer_schema_length(Some(0))
        .finish()
        .context("Failed to build LazyCsvReader")?;

    // A UTF-8 byte-order mark must not stay on the first column name.
    let first = lf
        .collect_schema()
        .context("Failed to read the CSV header")?
        .iter_names()
        .next()
        .map(|n| n.to_string());
    if let Some(name) = first {
        if let Some(clean) = name.strip_prefix('\u{feff}') {
            lf = lf.rename([name.as_str()], [clean]);
        }
    }
    Ok(lf)
}

/// The text a column holds just before typing: linebreaks sanitised, then the PII mask.
fn raw_text_expr(col_cfg: &ColumnSchemaConfig, config: &IngestionConfig) -> Expr {
    text_expr_from(col(&col_cfg.name), col_cfg, config)
}

fn text_expr_from(mut e: Expr, col_cfg: &ColumnSchemaConfig, config: &IngestionConfig) -> Expr {
    if config.sanitize_linebreaks {
        e = e
            .str()
            .replace_all(lit(r"[\r\n]+"), lit(" "), false)
            .str()
            .strip_chars(lit(" "));
    }

    if col_cfg.pii_mask != PiiMaskMode::None {
        // The old `config.pii_salt` is no longer used: masks use the per-install key.
        let mode = col_cfg.pii_mask;
        e = match pii_key_for_pipeline() {
            Ok(key) => e.map(
                move |s: Series| {
                    let ca = s.str()?;
                    let out: StringChunked = ca
                        .into_iter()
                        .map(|opt_v| opt_v.map(|v| mask_pii_value(v, mode, &key)))
                        .collect();
                    Ok(Some(out.into_series()))
                },
                GetOutput::from_type(DataType::String),
            ),
            Err(err) => {
                let msg = format!("{:#}", err);
                e.map(
                    move |_s: Series| Err(polars_err!(ComputeError: "{}", msg)),
                    GetOutput::from_type(DataType::String),
                )
            }
        };
    }
    e
}

/// Name of the per-install PII key file inside `portable_data/`.
pub const PII_KEY_FILE: &str = "pii_key.bin";

thread_local! {
    /// Lets a test point the key at a temporary folder (the key is read while the pipeline is
    /// built, on the calling thread).
    static PII_KEY_PATH_OVERRIDE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Uses `path` as the PII key file on this thread (`None` restores the default). For tests.
pub fn set_pii_key_path_override(path: Option<PathBuf>) {
    PII_KEY_PATH_OVERRIDE.with(|p| *p.borrow_mut() = path);
}

/// `portable_data/pii_key.bin` next to the executable.
pub fn pii_key_path() -> PathBuf {
    PII_KEY_PATH_OVERRIDE
        .with(|p| p.borrow().clone())
        .unwrap_or_else(|| portable_root_dir().join(PII_KEY_FILE))
}

// Set when a pipeline created the key without going through `ensure_pii_key_for`; the app
// shows it in the status bar so a new key is never created unannounced.
// (Per thread: pipelines are built on the thread that shows the status bar.)
thread_local! {
    static PII_KEY_PENDING_NOTE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

fn pii_key_created_note(path: &Path, key: &[u8; 32]) -> String {
    format!(
        "Created a new PII key (fingerprint {}) at {:?}. Export it (Schema & Ingestion) to keep masks reproducible on another PC or after reinstalling.",
        pii_key_fingerprint(key),
        path
    )
}

/// The one place that reads (or creates) the key for masking. Returns the key and, when it was
/// just created, the notice to show.
fn pii_key_and_note() -> Result<([u8; 32], Option<String>)> {
    let path = pii_key_path();
    let (key, created) = load_or_create_pii_key(&path)?;
    Ok((key, created.then(|| pii_key_created_note(&path, &key))))
}

/// Key for building a masking expression. A key created here (no caller announced it first) is
/// remembered for `take_pii_key_created_note`.
fn pii_key_for_pipeline() -> Result<[u8; 32]> {
    let (key, note) = pii_key_and_note()?;
    if let Some(n) = note {
        PII_KEY_PENDING_NOTE.with(|slot| *slot.borrow_mut() = Some(n));
    }
    Ok(key)
}

/// A notice for a key that a pipeline created on its own, once.
pub fn take_pii_key_created_note() -> Option<String> {
    PII_KEY_PENDING_NOTE.with(|slot| slot.borrow_mut().take())
}

fn pii_key_from_bytes(path: &Path, bytes: &[u8]) -> Result<[u8; 32]> {
    <[u8; 32]>::try_from(bytes).map_err(|_| {
        anyhow::anyhow!(
            "The PII key file {:?} is damaged ({} bytes, expected 32). Import your exported key in Schema & Ingestion, or delete the file to create a new key (masks made with a new key will not match earlier ones).",
            path,
            bytes.len()
        )
    })
}

/// Writes a new file that only the current user can read (Unix); fails if it exists.
fn write_new_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

/// Writes (or replaces) a file that only the current user can read on Unix. On Windows this is
/// an ordinary write.
fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    // `mode` only applies to a new file; tighten one that already existed.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    f.write_all(bytes)?;
    f.sync_all()
}

fn pii_key_temp_path(path: &Path) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    path.with_file_name(format!("{}.tmp-{}-{}-{}", PII_KEY_FILE, std::process::id(), nanos, n))
}

/// Reads the PII key, creating it with 32 random bytes when the file does not exist.
/// Returns the key and whether it was just created. A damaged file is an error, never replaced.
pub fn load_or_create_pii_key(path: &Path) -> Result<([u8; 32], bool)> {
    match std::fs::read(path) {
        Ok(bytes) => return Ok((pii_key_from_bytes(path, &bytes)?, false)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => anyhow::bail!(
            "Could not read the PII key file {:?}: {}. Check the file's permissions, or import your exported key in Schema & Ingestion.",
            path,
            e
        ),
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("Could not create the folder {:?} for the PII key", dir))?;
    }
    let mut key = [0u8; 32];
    getrandom::getrandom(&mut key)
        .map_err(|e| anyhow::anyhow!("Could not get random bytes for a new PII key: {}", e))?;
    // Write a temporary file, then hard-link it into place: a link never replaces an existing
    // file, so two processes creating the key at once end up with the same key.
    let tmp = pii_key_temp_path(path);
    write_new_private_file(&tmp, &key)
        .with_context(|| format!("Could not write a new PII key next to {:?}", path))?;
    let linked = std::fs::hard_link(&tmp, path);
    let _ = std::fs::remove_file(&tmp);
    match linked {
        Ok(()) => Ok((key, true)),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let bytes = std::fs::read(path)
                .with_context(|| format!("Could not read the PII key file {:?}", path))?;
            Ok((pii_key_from_bytes(path, &bytes)?, false))
        }
        // File systems without hard links (e.g. FAT on a USB stick): exclusive create instead.
        Err(_) => match write_new_private_file(path, &key) {
            Ok(()) => Ok((key, true)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let bytes = std::fs::read(path)
                    .with_context(|| format!("Could not read the PII key file {:?}", path))?;
                Ok((pii_key_from_bytes(path, &bytes)?, false))
            }
            Err(e) => anyhow::bail!("Could not create the PII key file {:?}: {}", path, e),
        },
    }
}

/// First 8 hex characters of blake3 of the key: shows whether two installs share a key.
pub fn pii_key_fingerprint(key: &[u8; 32]) -> String {
    blake3::hash(key).to_hex()[..8].to_uppercase()
}

/// Copies the install key (created first if missing) to `dest`. Returns whether it was created.
pub fn export_pii_key(key_path: &Path, dest: &Path) -> Result<bool> {
    let (key, created) = load_or_create_pii_key(key_path)?;
    write_private_file(dest, &key).with_context(|| format!("Could not write the key to {:?}", dest))?;
    Ok(created)
}

/// Replaces the install key with the 32-byte key in `src`. The current key is first copied to
/// `pii_key.bin.bak-<unix seconds>`; that path is returned.
pub fn import_pii_key(key_path: &Path, src: &Path) -> Result<Option<PathBuf>> {
    let bytes = std::fs::read(src).with_context(|| format!("Could not read the key file {:?}", src))?;
    let key = <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| {
        anyhow::anyhow!(
            "{:?} is not a MicroParquet PII key ({} bytes, expected 32). Nothing was changed.",
            src,
            bytes.len()
        )
    })?;
    if let Some(dir) = key_path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("Could not create the folder {:?}", dir))?;
    }
    let mut backup = None;
    if key_path.exists() {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let current = std::fs::read(key_path)
            .with_context(|| format!("Could not read the current key {:?}; nothing was changed", key_path))?;
        // Never overwrite an earlier backup: a second import in the same second gets `_2`, `_3` ...
        let mut n = 1u32;
        let bak = loop {
            let name = if n == 1 {
                format!("{}.bak-{}", PII_KEY_FILE, secs)
            } else {
                format!("{}.bak-{}_{}", PII_KEY_FILE, secs, n)
            };
            let candidate = key_path.with_file_name(name);
            match write_new_private_file(&candidate, &current) {
                Ok(()) => break candidate,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
                Err(e) => anyhow::bail!(
                    "Could not back up the current key to {:?}: {}. Nothing was changed.",
                    candidate,
                    e
                ),
            }
        };
        backup = Some(bak);
    }
    let tmp = pii_key_temp_path(key_path);
    write_new_private_file(&tmp, &key)
        .with_context(|| format!("Could not write the new key next to {:?}", key_path))?;
    if let Err(e) = std::fs::rename(&tmp, key_path) {
        let _ = std::fs::remove_file(&tmp);
        anyhow::bail!("Could not replace the PII key {:?}: {}", key_path, e);
    }
    Ok(backup)
}

/// The key in `path`: `Ok(None)` when there is no key file yet. Never creates one.
pub fn read_pii_key(path: &Path) -> Result<Option<[u8; 32]>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(pii_key_from_bytes(path, &bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => anyhow::bail!("Could not read the PII key file {:?}: {}", path, e),
    }
}

/// Creates the key when `source` masks a column and there is no key file yet. Returns a note
/// for the user when a key was created (here or by an earlier pipeline build that nobody
/// announced), or the error (missing permissions, damaged file).
pub fn ensure_pii_key_for(source: &TabSource) -> Result<Option<String>> {
    if !source_masks_pii(source) {
        return Ok(take_pii_key_created_note());
    }
    let (_, note) = pii_key_and_note()?;
    Ok(note.or_else(take_pii_key_created_note))
}

/// Names of the PII-masked columns of every file in `source` (no duplicates).
pub fn collect_masked_columns(source: &TabSource, out: &mut Vec<String>) {
    let mut add = |c: &IngestionConfig| {
        for col in c.columns.iter().filter(|c| c.pii_mask != PiiMaskMode::None) {
            if !out.contains(&col.name) {
                out.push(col.name.clone());
            }
        }
    };
    match source {
        TabSource::SingleFile { config, .. } => add(config),
        TabSource::VerticalStack { files, .. } => files.iter().for_each(|(_, _, c)| add(c)),
        TabSource::JoinedTabs { left_source, right_source, .. } => {
            collect_masked_columns(left_source, out);
            collect_masked_columns(right_source, out);
        }
        TabSource::PeriodReconcile { prev_source, curr_source, .. } => {
            collect_masked_columns(prev_source, out);
            collect_masked_columns(curr_source, out);
        }
    }
}

/// Whether building this source masks any column (and so needs the PII key).
pub fn source_masks_pii(source: &TabSource) -> bool {
    let cfg_masks = |c: &IngestionConfig| c.columns.iter().any(|col| col.pii_mask != PiiMaskMode::None);
    match source {
        TabSource::SingleFile { config, .. } => cfg_masks(config),
        TabSource::VerticalStack { files, .. } => files.iter().any(|(_, _, c)| cfg_masks(c)),
        TabSource::JoinedTabs { left_source, right_source, .. } => {
            source_masks_pii(left_source) || source_masks_pii(right_source)
        }
        TabSource::PeriodReconcile { prev_source, curr_source, .. } => {
            source_masks_pii(prev_source) || source_masks_pii(curr_source)
        }
    }
}

/// Converts a text expression to the column's configured type (P1 amount and date rules).
fn typed_expr(e: Expr, col_cfg: &ColumnSchemaConfig, config: &IngestionConfig) -> Expr {
    match col_cfg.dtype {
        ColDataType::Utf8 => e,
        ColDataType::Float64 => {
            let sign = col_cfg.sign_convention;
            let acc = config.parse_accounting_negatives;
            let name = col_cfg.name.clone();
            e.map(
                move |s: Series| {
                    let ca = s.str()?;
                    let mut out: Vec<Option<f64>> = Vec::with_capacity(ca.len());
                    for v in ca.into_iter() {
                        out.push(match v.map(|v| parse_banking_amount(v, sign, acc)) {
                            Some(AmountCell::Value(x)) => Some(x),
                            Some(AmountCell::NeedsSignConvention) => {
                                return Err(PolarsError::ComputeError(
                                    dr_cr_unset_message(&name).into(),
                                ))
                            }
                            _ => None,
                        });
                    }
                    Ok(Some(Float64Chunked::from_iter(out).into_series()))
                },
                GetOutput::from_type(DataType::Float64),
            )
        }
        ColDataType::Int64 => {
            let sign = col_cfg.sign_convention;
            let acc = config.parse_accounting_negatives;
            e.map(
                move |s: Series| {
                    let ca = s.str()?;
                    let out: Int64Chunked = ca
                        .into_iter()
                        .map(|v| v.and_then(|v| parse_banking_int(v, sign, acc)))
                        .collect();
                    Ok(Some(out.into_series()))
                },
                GetOutput::from_type(DataType::Int64),
            )
        }
        ColDataType::DateMDY | ColDataType::DateDMY | ColDataType::DateISO => {
            let date_mode = col_cfg.dtype;
            e.map(
                move |s: Series| {
                    let ca = s.str()?;
                    let days_ca: Int32Chunked = ca
                        .into_iter()
                        .map(|opt_v| opt_v.and_then(|v| parse_flexible_banking_date(v, date_mode)))
                        .collect();
                    let date_series = days_ca.into_series().cast(&DataType::Date)?;
                    Ok(Some(date_series))
                },
                GetOutput::from_type(DataType::Date),
            )
        }
    }
}

/// True when a non-blank raw value of a typed column does not convert (it would become null).
pub fn is_parse_failure(
    raw: &str,
    dtype: ColDataType,
    sign: SignConvention,
    accounting_negatives: bool,
) -> bool {
    match dtype {
        ColDataType::Utf8 => false,
        ColDataType::Float64 => !matches!(
            parse_banking_amount(raw, sign, accounting_negatives),
            AmountCell::Value(_) | AmountCell::Blank
        ),
        ColDataType::Int64 => {
            !is_blank_amount_token(raw) && parse_banking_int(raw, sign, accounting_negatives).is_none()
        }
        ColDataType::DateMDY | ColDataType::DateDMY | ColDataType::DateISO => {
            !is_blank_date_token(raw) && parse_flexible_banking_date(raw, dtype).is_none()
        }
    }
}

/// Counts, per typed column, the non-blank raw values that become null, with up to 5 examples.
/// Columns with no failures are left out.
pub fn compute_parse_failures(
    path: &Path,
    is_parquet: bool,
    config: &IngestionConfig,
    temp_dir: &Path,
) -> Result<Vec<ParseFailure>> {
    Ok(typed_scan(path, is_parquet, config, temp_dir)?.failures.clone())
}

/// Parse failures of everything a tab reads (stacked, joined and reconciled sources included),
/// added up per column name.
pub fn source_parse_failures(source: &TabSource, temp_dir: &Path) -> Result<Vec<ParseFailure>> {
    let mut parts = Vec::new();
    match source {
        TabSource::SingleFile {
            path,
            is_parquet,
            config,
        } => parts.extend(compute_parse_failures(path, *is_parquet, config, temp_dir)?),
        TabSource::VerticalStack { files, .. } => {
            for (path, is_parquet, cfg) in files {
                parts.extend(compute_parse_failures(path, *is_parquet, cfg, temp_dir)?);
            }
        }
        TabSource::JoinedTabs {
            left_source,
            right_source,
            ..
        } => {
            parts.extend(source_parse_failures(left_source, temp_dir)?);
            parts.extend(source_parse_failures(right_source, temp_dir)?);
        }
        TabSource::PeriodReconcile {
            prev_source,
            curr_source,
            ..
        } => {
            parts.extend(source_parse_failures(prev_source, temp_dir)?);
            parts.extend(source_parse_failures(curr_source, temp_dir)?);
        }
    }
    let mut merged: Vec<ParseFailure> = Vec::new();
    for f in parts {
        match merged.iter_mut().find(|m| m.column == f.column) {
            Some(m) => {
                m.count += f.count;
                for ex in f.examples {
                    if m.examples.len() < 5 {
                        m.examples.push(ex);
                    }
                }
            }
            None => merged.push(f),
        }
    }
    Ok(merged)
}

/// "Parse failures: Balance 3, Matdat 1", or "" when there are none.
pub fn parse_failure_summary(failures: &[ParseFailure]) -> String {
    if failures.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = failures
        .iter()
        .map(|f| format!("{} {}", f.column, crate::models::format_with_commas(f.count as f64, 0)))
        .collect();
    format!("Parse failures: {}", parts.join(", "))
}

/// Writes `<output>.parse_failures.csv` next to an export (columns: column, count, examples)
/// and returns its path.
pub fn write_parse_failures_csv(output: &Path, failures: &[ParseFailure]) -> Result<PathBuf> {
    let mut name = output.file_name().unwrap_or_default().to_os_string();
    name.push(".parse_failures.csv");
    let side = output.with_file_name(name);
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut body = String::from("column,count,examples\n");
    for f in failures {
        body.push_str(&format!(
            "{},{},{}\n",
            quote(&f.column),
            f.count,
            quote(&f.examples.join(" | "))
        ));
    }
    std::fs::write(&side, body)
        .with_context(|| format!("Failed to write the parse-failure list {:?}", side))?;
    Ok(side)
}

/// Label of a Parquet column type, as stored in `ColumnSchemaConfig::source_dtype`.
pub fn parquet_type_label(dt: &DataType) -> String {
    match dt {
        DataType::String => "String".into(),
        DataType::Float32 => "Float32".into(),
        DataType::Float64 => "Float64".into(),
        DataType::Decimal(..) => "Decimal".into(),
        DataType::Int8 => "Int8".into(),
        DataType::Int16 => "Int16".into(),
        DataType::Int32 => "Int32".into(),
        DataType::Int64 => "Int64".into(),
        DataType::UInt8 => "UInt8".into(),
        DataType::UInt16 => "UInt16".into(),
        DataType::UInt32 => "UInt32".into(),
        DataType::UInt64 => "UInt64".into(),
        DataType::Date => "Date".into(),
        DataType::Datetime(..) => "Datetime".into(),
        other => format!("{:?}", other),
    }
}

/// First rows of a Parquet file as stored (for the schema modal preview).
pub fn preview_parquet_raw(path: &Path, n_rows: usize) -> Result<DataFrame> {
    LazyFrame::scan_parquet(path, ScanArgsParquet::default())
        .context("Failed to scan Parquet file")?
        .limit(n_rows as IdxSize)
        .collect()
        .context("Failed to read the first Parquet rows")
}

/// Schema of a Parquet file for the schema modal: every column keeps its own type (String
/// columns are Text until the user changes them); sample values are the first 5 rows as text.
pub fn auto_detect_parquet_schema(path: &Path) -> Result<IngestionConfig> {
    let mut lf = LazyFrame::scan_parquet(path, ScanArgsParquet::default())
        .context("Failed to scan Parquet file")?;
    let schema = lf.collect_schema().context("Failed to read the Parquet schema")?;
    let head = lf
        .limit(5)
        .collect()
        .context("Failed to read the first Parquet rows")?;
    let mut columns = Vec::with_capacity(schema.len());
    for (name, dt) in schema.iter() {
        let label = parquet_type_label(dt);
        let samples = head
            .column(name)
            .ok()
            .and_then(|s| s.cast(&DataType::String).ok())
            .map(|s| {
                s.str()
                    .map(|ca| {
                        ca.into_iter()
                            .map(|v| v.unwrap_or("null").to_string())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        columns.push(ColumnSchemaConfig {
            name: name.to_string(),
            dtype: parquet_native_type(&label).unwrap_or(ColDataType::Utf8),
            sample_values: samples,
            source_dtype: label,
            ..Default::default()
        });
    }
    Ok(IngestionConfig {
        columns,
        ..Default::default()
    })
}

/// Fits a saved (macro) schema to a Parquet file: entries for columns that exist and are
/// String in the file are kept (with that file type); columns of other types keep their own
/// type. Returns the fitted schema and the names of schema columns absent from the file.
pub fn adapt_schema_to_parquet(
    path: &Path,
    saved: &IngestionConfig,
) -> Result<(IngestionConfig, Vec<String>)> {
    let mut lf = LazyFrame::scan_parquet(path, ScanArgsParquet::default())
        .context("Failed to scan Parquet file")?;
    let schema = lf.collect_schema().context("Failed to read the Parquet schema")?;
    let mut missing = Vec::new();
    let mut columns = Vec::new();
    for c in &saved.columns {
        match schema.get(&c.name) {
            None => missing.push(c.name.clone()),
            Some(DataType::String) => {
                let mut c = c.clone();
                c.source_dtype = "String".into();
                columns.push(c);
            }
            Some(_) => {}
        }
    }
    Ok((
        IngestionConfig {
            columns,
            ..saved.clone()
        },
        missing,
    ))
}

/// Scans a Parquet file and lists the configured columns that must be converted, each with an
/// expression giving its text (String columns as they are; other types cast to text). Columns
/// whose type is unchanged and unmasked are left as they are in the file.
fn parquet_conversion_plan<'a>(
    path: &Path,
    config: &'a IngestionConfig,
) -> Result<(LazyFrame, Vec<(&'a ColumnSchemaConfig, Expr)>)> {
    let mut lf = LazyFrame::scan_parquet(path, ScanArgsParquet::default())
        .context("Failed to scan Parquet file")?;
    if config.columns.is_empty() {
        return Ok((lf, Vec::new()));
    }
    let schema = lf.collect_schema().context("Failed to read the Parquet schema")?;
    let mut plan = Vec::new();
    for c in &config.columns {
        let Some(dt) = schema.get(&c.name) else {
            anyhow::bail!(
                "Column '{}' is in the schema but not in the Parquet file {:?}.",
                c.name,
                path
            );
        };
        // Judge by the file's actual type, not a label saved earlier.
        let label = parquet_type_label(dt);
        let mut actual = c.clone();
        actual.source_dtype = label.clone();
        if !parquet_needs_conversion(&actual) {
            continue;
        }
        if parquet_native_type(&label).is_none() {
            anyhow::bail!(
                "Column '{}' is {} in the Parquet file and cannot be converted or masked. \
                 Set it back to Text with no mask in Schema & Ingestion.",
                c.name,
                label
            );
        }
        let base = if *dt == DataType::String {
            col(&c.name)
        } else {
            col(&c.name).cast(DataType::String)
        };
        plan.push((c, text_expr_from(base, c, config)));
    }
    Ok((lf, plan))
}

/// Stacks files on top of each other. A column missing from a file is null for that file's rows
/// (diagonal concatenation). A column whose type differs between files is an error.
fn stack_lazy(
    files: &[(PathBuf, bool, IngestionConfig)],
    include_source_col: bool,
    temp_dir: &Path,
) -> Result<LazyFrame> {
    if files.is_empty() {
        anyhow::bail!("Stack: no files to stack. Add at least one file.");
    }
    let mut parts = Vec::with_capacity(files.len());
    for (path, is_parq, cfg) in files {
        let fname = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let mut sub_lf = build_lazy_pipeline(path, *is_parq, cfg, temp_dir)
            .with_context(|| format!("Stack: could not read {}", fname))?;
        let schema = sub_lf
            .collect_schema()
            .with_context(|| format!("Stack: could not read the columns of {}", fname))?;
        parts.push((fname, sub_lf, schema));
    }

    // Union of columns in order of first appearance, with the type each column has.
    let mut order: Vec<(PlSmallStr, DataType, String)> = Vec::new();
    for (fname, _, schema) in &parts {
        for (name, dt) in schema.iter() {
            match order.iter_mut().find(|(n, _, _)| n == name) {
                None => order.push((name.clone(), dt.clone(), fname.clone())),
                Some((_, seen, seen_file)) => {
                    if *seen == DataType::Null {
                        *seen = dt.clone();
                        *seen_file = fname.clone();
                    } else if dt != seen && *dt != DataType::Null {
                        anyhow::bail!(
                            "Stack: column '{}' is {:?} in {} but {:?} in {}. Use 'Apply one schema to every file' in the Stack dialog (first file's schema or a saved .schema.json), or fix the files.",
                            name,
                            seen,
                            seen_file,
                            dt,
                            fname
                        );
                    }
                }
            }
        }
    }
    let source_col = PlSmallStr::from("Source_File");
    let mut lfs = Vec::with_capacity(parts.len());
    for (fname, sub_lf, schema) in parts {
        let mut exprs: Vec<Expr> = Vec::with_capacity(order.len() + 1);
        for (name, dt, _) in &order {
            if include_source_col && *name == source_col {
                continue;
            }
            match schema.get(name) {
                Some(own) if own == dt => exprs.push(col(name.clone())),
                Some(_) => exprs.push(col(name.clone()).cast(dt.clone())),
                None => exprs.push(lit(NULL).cast(dt.clone()).alias(name.clone())),
            }
        }
        if include_source_col {
            exprs.push(lit(fname).alias(source_col.clone()));
        }
        lfs.push(sub_lf.select(exprs));
    }
    concat(&lfs, UnionArgs::default()).context("Failed to stack files vertically")
}

/// Applies one schema to a file of a stack. CSV: the file's own separator, title lines and
/// encoding are detected; a column the schema names takes the schema's settings, other columns
/// keep their detected type. Parquet: as `adapt_schema_to_parquet`.
/// Returns the file's config and the schema columns the file does not have.
pub fn apply_shared_schema(
    path: &Path,
    is_parquet: bool,
    shared: &IngestionConfig,
) -> Result<(IngestionConfig, Vec<String>)> {
    if is_parquet {
        return adapt_schema_to_parquet(path, shared);
    }
    let own = auto_detect_smart_schema(path)?;
    let columns: Vec<ColumnSchemaConfig> = own
        .columns
        .iter()
        .map(|c| {
            shared
                .columns
                .iter()
                .find(|s| s.name == c.name)
                .cloned()
                .unwrap_or_else(|| c.clone())
        })
        .collect();
    let missing: Vec<String> = shared
        .columns
        .iter()
        .filter(|s| !own.columns.iter().any(|c| c.name == s.name))
        .map(|s| s.name.clone())
        .collect();
    Ok((
        IngestionConfig {
            columns,
            separator: own.separator,
            skip_rows: own.skip_rows,
            encoding: own.encoding,
            ..shared.clone()
        },
        missing,
    ))
}

fn path_is_parquet(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .eq_ignore_ascii_case("parquet")
}

/// The schema a file opens with: detected column types for a CSV, native types for Parquet.
pub fn detect_file_schema(path: &Path) -> Result<IngestionConfig> {
    if path_is_parquet(path) {
        auto_detect_parquet_schema(path)
    } else {
        auto_detect_smart_schema(path)
    }
}

/// Configs for the files of a stack. Without a shared schema each CSV gets its own detected
/// schema and each Parquet file its native types; with one, `apply_shared_schema` is used.
/// Also returns one note per file that lacks columns of the shared schema.
pub fn stack_file_configs(
    files: &[PathBuf],
    shared: Option<&IngestionConfig>,
) -> Result<(Vec<(PathBuf, bool, IngestionConfig)>, Vec<String>)> {
    let mut specs = Vec::with_capacity(files.len());
    let mut notes = Vec::new();
    for p in files {
        let is_parq = path_is_parquet(p);
        let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
        let (cfg, missing) = match shared {
            Some(s) => apply_shared_schema(p, is_parq, s)
                .with_context(|| format!("Stack: could not apply the schema to {}", name))?,
            None if is_parq => (IngestionConfig::default(), Vec::new()),
            None => (
                auto_detect_smart_schema(p)
                    .with_context(|| format!("Stack: could not read {}", name))?,
                Vec::new(),
            ),
        };
        if !missing.is_empty() {
            notes.push(format!("{} has no column {} (null for its rows)", name, missing.join(", ")));
        }
        specs.push((p.clone(), is_parq, cfg));
    }
    Ok((specs, notes))
}

/// Key combinations (all key columns non-null) that occur more than once in `lf`:
/// (number of such combinations, extra rows beyond the first per key, up to 10 examples).
pub fn duplicate_key_report(lf: LazyFrame, keys: &[String]) -> Result<(usize, usize, Vec<String>)> {
    if keys.is_empty() {
        anyhow::bail!("Join: no key column given. Enter at least one key column (e.g. Actnum).");
    }
    let not_null = keys
        .iter()
        .fold(lit(true), |acc, k| acc.and(col(k).is_not_null()));
    let key_exprs: Vec<Expr> = keys.iter().map(|k| col(k)).collect();
    let dup = lf
        .filter(not_null)
        .group_by_stable(key_exprs)
        .agg([len().alias("__mp_key_rows")])
        .filter(col("__mp_key_rows").gt(lit(1)))
        .collect()
        .context("Join: could not check the right table for duplicated keys")?;
    let n = dup.height();
    let mut extra = 0usize;
    if let Ok(c) = dup.column("__mp_key_rows").and_then(|c| c.cast(&DataType::UInt64)) {
        if let Ok(ca) = c.u64() {
            extra = ca.into_iter().flatten().map(|v| v.saturating_sub(1) as usize).sum();
        }
    }
    let mut key_text = Vec::with_capacity(keys.len());
    for k in keys {
        key_text.push(dup.column(k)?.cast(&DataType::String)?);
    }
    let mut examples = Vec::new();
    for i in 0..n.min(10) {
        let parts: Vec<String> = key_text
            .iter()
            .map(|s| {
                s.str()
                    .ok()
                    .and_then(|ca| ca.get(i))
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        examples.push(parts.join(" / "));
    }
    Ok((n, extra, examples))
}

/// Keeps the first right-side row per non-null key; rows with a null key are kept as they are
/// (they never match, and a full join must still show them).
fn keep_first_per_key(lf: LazyFrame, keys: &[String]) -> Result<LazyFrame> {
    let any_null = keys
        .iter()
        .fold(lit(false), |acc, k| acc.or(col(k).is_null()));
    let subset: Vec<PlSmallStr> = keys.iter().map(|k| PlSmallStr::from(k.as_str())).collect();
    let deduped = lf
        .clone()
        .filter(any_null.clone().not())
        .unique_stable(Some(subset), UniqueKeepStrategy::First);
    let null_keys = lf.filter(any_null);
    concat(&[deduped, null_keys], UnionArgs::default())
        .context("Join: could not keep the first row per key")
}

/// Rows and control sum of a joined tab next to its left table's. `None` for other sources.
pub fn join_status(
    source: &TabSource,
    steps: &[TransformStep],
    control_col: Option<&str>,
    temp_dir: &Path,
) -> Result<Option<JoinStatus>> {
    let TabSource::JoinedTabs {
        left_source,
        left_steps,
        right_source,
        right_steps,
        join_kind,
        join_keys,
        keep_first_duplicate_right,
        ..
    } = source
    else {
        return Ok(None);
    };
    let left_lf = compile_tab_lazy(left_source, left_steps, temp_dir)?;
    let left_has_ctrl = control_col
        .map(|c| {
            left_lf
                .clone()
                .collect_schema()
                .map(|s| s.contains(c))
                .unwrap_or(false)
        })
        .unwrap_or(false);
    let (left_rows, left_sum) =
        compute_control_totals(left_lf, if left_has_ctrl { control_col } else { None })?;
    let joined = compile_tab_lazy(source, steps, temp_dir)?;
    let (rows, sum) = compute_control_totals(joined, control_col)?;
    let (mut duplicate_keys, mut dropped_rows) = (0, 0);
    if *keep_first_duplicate_right && join_inflates_left(*join_kind) {
        let right_lf = compile_tab_lazy(right_source, right_steps, temp_dir)?;
        let (n, extra, _) = duplicate_key_report(right_lf, join_keys)?;
        duplicate_keys = n;
        dropped_rows = extra;
    }
    Ok(Some(JoinStatus {
        left_rows,
        left_sum,
        rows,
        sum,
        duplicate_keys,
        dropped_rows,
    }))
}

/// Semi and anti joins never repeat a left row, so right-side duplicates do not matter there.
fn join_inflates_left(kind: SqlJoinKind) -> bool {
    !matches!(kind, SqlJoinKind::Semi | SqlJoinKind::Anti)
}

pub fn compile_tab_lazy(
    source: &TabSource,
    steps: &[TransformStep],
    temp_dir: &Path,
) -> Result<LazyFrame> {
    let mut lf = match source {
        TabSource::SingleFile {
            path,
            is_parquet,
            config,
        } => build_lazy_pipeline(path, *is_parquet, config, temp_dir)?,

        TabSource::VerticalStack {
            files,
            include_source_col,
        } => stack_lazy(files, *include_source_col, temp_dir)?,

        TabSource::JoinedTabs {
            left_source,
            left_steps,
            right_source,
            right_steps,
            join_kind,
            join_keys,
            right_cols_to_keep,
            keep_first_duplicate_right,
        } => {
            let left_lf = compile_tab_lazy(left_source, left_steps, temp_dir)?;
            let mut right_lf = compile_tab_lazy(right_source, right_steps, temp_dir)?;

            if join_inflates_left(*join_kind) {
                if *keep_first_duplicate_right {
                    right_lf = keep_first_per_key(right_lf, join_keys)?;
                } else {
                    let (n, _, examples) = duplicate_key_report(right_lf.clone(), join_keys)?;
                    if n > 0 {
                        anyhow::bail!(
                            "Right table has {} duplicated keys on {}, e.g. {}. Fix the mapping table, or switch on 'Keep first row per key' for this join.",
                            n,
                            join_keys.join(", "),
                            examples.join(", ")
                        );
                    }
                }
            }

            if !right_cols_to_keep.is_empty() {
                let mut select_cols: Vec<Expr> = join_keys.iter().map(|k| col(k)).collect();
                for rc in right_cols_to_keep {
                    if !join_keys.contains(rc) {
                        select_cols.push(col(rc));
                    }
                }
                right_lf = right_lf.select(select_cols);
            }

            let key_exprs: Vec<Expr> = join_keys.iter().map(|k| col(k)).collect();
            let j_type = match join_kind {
                SqlJoinKind::Left => JoinType::Left,
                SqlJoinKind::Inner => JoinType::Inner,
                SqlJoinKind::Full => JoinType::Full,
                SqlJoinKind::Anti => JoinType::Anti,
                SqlJoinKind::Semi => JoinType::Semi,
            };

            left_lf.join(
                right_lf,
                &key_exprs,
                &key_exprs,
                JoinArgs::new(j_type).with_suffix(Some("_right".into())),
            )
        }

        TabSource::PeriodReconcile {
            prev_source,
            prev_steps,
            curr_source,
            curr_steps,
            key_col,
            bal_col,
            rate_col,
            cl_status_col,
        } => {
            let prev_lf = compile_tab_lazy(prev_source, prev_steps, temp_dir)?;
            let curr_lf = compile_tab_lazy(curr_source, curr_steps, temp_dir)?;
            build_period_reconcile_lazy(
                prev_lf,
                curr_lf,
                key_col,
                bal_col,
                rate_col.as_deref(),
                cl_status_col.as_deref(),
            )?
        }
    };

    for step in steps {
        lf = apply_single_step(lf, step)?;
    }

    Ok(lf)
}

fn apply_single_step(lf: LazyFrame, step: &TransformStep) -> Result<LazyFrame> {
    match step {
        TransformStep::Skipped { .. } => Ok(lf),
        TransformStep::SelectAndReorderColumns { cols } => {
            let exprs: Vec<Expr> = cols.iter().map(|c| col(c)).collect();
            Ok(lf.select(exprs))
        }
        TransformStep::RenameColumn { old_name, new_name } => {
            Ok(lf.rename([old_name.as_str()], [new_name.as_str()]))
        }
        TransformStep::ConvertToNumber { columns } => {
            let mut lf = lf;
            let schema = lf
                .collect_schema()
                .context("Could not read the column types for Set to Number")?;
            let mut exprs: Vec<Expr> = Vec::with_capacity(columns.len());
            for c in columns {
                if schema.get(c.as_str()).is_none() {
                    anyhow::bail!("Set to Number: column '{}' does not exist at this step.", c);
                }
                let name = c.clone();
                exprs.push(
                    col(c.as_str())
                        .cast(DataType::String)
                        .map(
                            move |s: Series| {
                                let ca = s.str()?;
                                let mut out: Vec<Option<f64>> = Vec::with_capacity(ca.len());
                                for v in ca.into_iter() {
                                    out.push(match v.map(|v| parse_banking_amount(v, SignConvention::Unset, true)) {
                                        Some(AmountCell::Value(x)) => Some(x),
                                        Some(AmountCell::NeedsSignConvention) => {
                                            return Err(PolarsError::ComputeError(
                                                dr_cr_unset_message(&name).into(),
                                            ))
                                        }
                                        _ => None,
                                    });
                                }
                                Ok(Some(Float64Chunked::from_iter(out).into_series()))
                            },
                            GetOutput::from_type(DataType::Float64),
                        )
                        .alias(c.as_str()),
                );
            }
            Ok(lf.with_columns(exprs))
        }
        TransformStep::SortRows { levels } => {
            if levels.is_empty() {
                return Ok(lf);
            }
            let mut lf = lf;
            let schema = lf
                .collect_schema()
                .context("Could not read the column types for this sort")?;
            let mut keys: Vec<Expr> = Vec::with_capacity(levels.len());
            for l in levels {
                let dtype = schema.get(l.column.as_str()).ok_or_else(|| {
                    anyhow::anyhow!("Sort: column '{}' was not found. Pick it again in the sort editor.", l.column)
                })?;
                // Blank text sorts with the nulls: last, in either direction. Other text sorts by
                // its natural key: case-insensitive, numbers inside the text by value.
                keys.push(if dtype.is_string() {
                    when(col(l.column.as_str()).str().strip_chars(lit(NULL)).eq(lit("")))
                        .then(lit(NULL).cast(DataType::String))
                        .otherwise(col(l.column.as_str()))
                        .map(
                            |s: Series| {
                                let ca = s.str()?;
                                let out: StringChunked =
                                    ca.into_iter().map(|v| v.map(natural_sort_key)).collect();
                                Ok(Some(out.with_name(s.name().clone()).into_series()))
                            },
                            GetOutput::from_type(DataType::String),
                        )
                } else {
                    col(l.column.as_str())
                });
            }
            let n = levels.len();
            let options = SortMultipleOptions::default()
                .with_order_descending_multi(levels.iter().map(|l| l.descending))
                .with_nulls_last_multi(vec![true; n])
                .with_maintain_order(true);
            Ok(lf.sort_by_exprs(keys, options))
        }
        TransformStep::FilterGroup { conditions, join, keep_matching } => {
            if conditions.is_empty() {
                return Ok(lf);
            }
            let mut lf = lf;
            let schema = lf
                .collect_schema()
                .context("Could not read the column types for this filter")?;
            let mut exprs = Vec::with_capacity(conditions.len());
            for c in conditions {
                exprs.push(build_condition_expr(c, &schema)?);
            }
            let mut it = exprs.into_iter();
            let mut combined = it.next().unwrap_or_else(|| lit(true));
            for e in it {
                combined = match join {
                    GroupJoin::And => combined.and(e),
                    GroupJoin::Or => combined.or(e),
                };
            }
            // An unknown test (a null against a value) is "does not match".
            let matches = combined.fill_null(lit(false));
            Ok(lf.filter(if *keep_matching { matches } else { matches.not() }))
        }
        TransformStep::FilterRows {
            column,
            operator,
            value,
            keep_matching,
        } => {
            let mut lf = lf;
            let schema = lf
                .collect_schema()
                .context("Could not read the column types for this filter")?;
            let cond = build_filter_expr(column, *operator, value, &schema)?;
            let final_cond = if *keep_matching { cond } else { cond.not() };
            Ok(lf.filter(final_cond))
        }
        TransformStep::ConditionalColumn {
            new_col,
            rules,
            fallback,
            output_as_float,
        } => {
            if rules.is_empty() {
                return Ok(lf);
            }
            let mut lf = lf;
            let schema = lf
                .collect_schema()
                .context("Could not read the column types for this conditional column")?;
            let mut expr: Expr = parse_val_expr(fallback, *output_as_float);
            for r in rules.iter().rev() {
                let c = build_filter_expr(&r.col_name, r.op, &r.compare_val, &schema)?;
                let o = parse_val_expr(&r.output_val, *output_as_float);
                expr = when(c).then(o).otherwise(expr);
            }
            Ok(lf.with_column(expr.alias(new_col)))
        }
        TransformStep::MathFormula {
            new_col,
            left_col,
            op,
            right_is_col,
            right_val_or_col,
        } => {
            let l = col(left_col).cast(DataType::Float64);
            let r = if *right_is_col {
                col(right_val_or_col).cast(DataType::Float64)
            } else {
                lit(parse_math_scalar(new_col, right_val_or_col)?)
            };
            let res = match op {
                MathOp::Add => l + r,
                MathOp::Sub => l - r,
                MathOp::Mul => l * r,
                MathOp::Div => l / r,
            };
            Ok(lf.with_column(res.alias(new_col)))
        }
        TransformStep::NormalizeEntity {
            source_col,
            new_col,
        } => Ok(apply_entity_normalizer(lf, source_col, new_col)),
        TransformStep::MaturityBucket {
            mat_col,
            cutoff_date,
            bucket_def,
            null_as_sight,
            new_col,
        } => apply_maturity_bucket(lf, mat_col, cutoff_date, bucket_def, *null_as_sight, new_col),
        TransformStep::RateShockSim {
            bal_col,
            rate_col,
            mat_col,
            cutoff_date,
            shock_bps,
            side_col,
        } => apply_rate_shock_sim(
            lf,
            bal_col,
            rate_col,
            mat_col,
            cutoff_date,
            *shock_bps,
            side_col.as_deref(),
        ),
        TransformStep::FtpCurveMap {
            opndat_col,
            matdat_col,
            bal_col,
            rate_col,
            curve,
            is_liability,
            interpolation,
        } => apply_ftp_curve_map(
            lf,
            opndat_col,
            matdat_col,
            bal_col,
            rate_col,
            curve,
            *is_liability,
            *interpolation,
        ),
        TransformStep::BankingHygieneAudit {
            opndat_col,
            matdat_col,
            clsdat_col,
            acstat_col,
            bal_col,
            rate_col,
            cutoff_date,
            only_exceptions,
            active_statuses,
            rate_min_pct,
            rate_max_pct,
            ..
        } => apply_banking_hygiene_audit(
            lf,
            opndat_col,
            matdat_col,
            clsdat_col,
            acstat_col,
            bal_col,
            rate_col,
            cutoff_date,
            *only_exceptions,
            active_statuses,
            *rate_min_pct,
            *rate_max_pct,
        ),
        TransformStep::DurationAnalysis {
            bal_col,
            rate_col,
            mat_col,
            cutoff_date,
            compounding_freq,
            shock_bps,
            default_sight_years,
            side_col,
        } => apply_duration_analysis(
            lf,
            bal_col,
            rate_col,
            mat_col,
            cutoff_date,
            *compounding_freq,
            *shock_bps,
            *default_sight_years,
            side_col.as_deref(),
        ),
        TransformStep::LoanClassificationProvision {
            bal_col,
            cl_status_col,
            segment_col,
            suspense_col,
            security_col,
            cutoff_date,
            collateral_type_col,
        } => apply_loan_classification_provision(
            lf,
            bal_col,
            cl_status_col,
            segment_col.as_deref(),
            suspense_col.as_deref(),
            security_col.as_deref(),
            collateral_type_col.as_deref(),
            cutoff_date,
        ),
        TransformStep::LoanLimitUtilization {
            limit_col,
            bal_col,
            ccf_pct,
        } => apply_loan_limit_utilization(lf, limit_col, bal_col, *ccf_pct),
        TransformStep::GroupByPivot {
            group_cols,
            aggs,
            sort_by_col,
            top_n,
            ..
        } => apply_groupby_pivot(lf, group_cols, aggs, sort_by_col.as_deref(), *top_n),
        TransformStep::SqlQuery { query } => {
            let mut ctx = SQLContext::new();
            ctx.register("self", lf);
            let res_lf = ctx
                .execute(query)
                .context("SQL execution failed (use table name 'self')")?;
            Ok(res_lf)
        }
    }
}

/// Builds a filter condition that compares by the column's real type: text columns compare
/// as text (so 17-digit account numbers stay exact), numeric columns as numbers and date
/// columns as dates. A value that doesn't fit the column type is an error, not a silent miss.
fn build_filter_expr(col_name: &str, op: FilterOp, raw_val: &str, schema: &Schema) -> Result<Expr> {
    let c = col(col_name);
    let trimmed = raw_val.trim();

    Ok(match op {
        FilterOp::IsNull => c.is_null(),
        FilterOp::IsNotNull => c.is_not_null(),
        FilterOp::Contains => c
            .cast(DataType::String)
            .str()
            .contains(lit(trimmed), false),
        FilterOp::StartsWith => c.cast(DataType::String).str().starts_with(lit(trimmed)),
        FilterOp::InList => {
            let items: Vec<String> = trimmed
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let mut expr = lit(false);
            for item in items {
                expr = expr.or(c.clone().cast(DataType::String).eq(lit(item)));
            }
            expr
        }
        FilterOp::Eq | FilterOp::NotEq | FilterOp::Gt | FilterOp::Gte | FilterOp::Lt | FilterOp::Lte => {
            let (lhs, rhs) = typed_operand(col_name, trimmed, schema)?;
            match op {
                FilterOp::Eq => lhs.eq(rhs),
                FilterOp::NotEq => lhs.neq(rhs),
                FilterOp::Gt => lhs.gt(rhs),
                FilterOp::Gte => lhs.gt_eq(rhs),
                FilterOp::Lt => lhs.lt(rhs),
                _ => lhs.lt_eq(rhs),
            }
        }
    })
}

/// The column and the typed value to compare it with: text as text, numbers as numbers (the
/// value must be one), dates as dates (the value must be YYYY-MM-DD).
fn typed_operand(col_name: &str, trimmed: &str, schema: &Schema) -> Result<(Expr, Expr)> {
    let c = col(col_name);
    let dtype = schema.get(col_name).ok_or_else(|| {
        anyhow::anyhow!(
            "Column '{}' was not found. Check the column chosen in this filter.",
            col_name
        )
    })?;
    if dtype.is_string() || dtype.is_categorical() {
        Ok((c.cast(DataType::String), lit(trimmed.to_string())))
    } else if dtype.is_numeric() {
        let num = trimmed.parse::<f64>().map_err(|_| {
            anyhow::anyhow!(
                "'{}' is not a number for numeric column '{}'",
                trimmed,
                col_name
            )
        })?;
        Ok((c.cast(DataType::Float64), lit(num)))
    } else if matches!(dtype, DataType::Date | DataType::Datetime(_, _)) {
        let d = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").map_err(|_| {
            anyhow::anyhow!(
                "'{}' is not a date for date column '{}'. Enter it as YYYY-MM-DD (e.g. 2026-10-31).",
                trimmed,
                col_name
            )
        })?;
        let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).context("Internal date error (epoch)")?;
        let days = (d - epoch).num_days() as i32;
        Ok((c.cast(DataType::Date), lit(days).cast(DataType::Date)))
    } else {
        anyhow::bail!(
            "Column '{}' is {}; comparison filters work on text, number and date columns only",
            col_name,
            dtype
        );
    }
}

/// One condition of a filter group, with the same type-aware comparison as a single filter.
/// Blank means null, or text that is empty after trimming.
fn build_condition_expr(cond: &FilterCondition, schema: &Schema) -> Result<Expr> {
    let name = cond.column.as_str();
    let dtype = schema.get(name).ok_or_else(|| {
        anyhow::anyhow!("Column '{}' was not found. Check the column chosen in this filter.", name)
    })?;
    let value = cond.value.trim();
    Ok(match cond.op {
        CondOp::Equals => build_filter_expr(name, FilterOp::Eq, value, schema)?,
        CondOp::NotEquals => build_filter_expr(name, FilterOp::NotEq, value, schema)?,
        CondOp::Gt => build_filter_expr(name, FilterOp::Gt, value, schema)?,
        CondOp::Gte => build_filter_expr(name, FilterOp::Gte, value, schema)?,
        CondOp::Lt => build_filter_expr(name, FilterOp::Lt, value, schema)?,
        CondOp::Lte => build_filter_expr(name, FilterOp::Lte, value, schema)?,
        CondOp::Contains => build_filter_expr(name, FilterOp::Contains, value, schema)?,
        CondOp::StartsWith => build_filter_expr(name, FilterOp::StartsWith, value, schema)?,
        CondOp::InList => build_filter_expr(name, FilterOp::InList, value, schema)?,
        CondOp::EndsWith => col(name).cast(DataType::String).str().ends_with(lit(value.to_string())),
        CondOp::Between => {
            let (lhs, lo) = typed_operand(name, value, schema)?;
            let (_, hi) = typed_operand(name, cond.value2.trim(), schema)?;
            lhs.clone().gt_eq(lo).and(lhs.lt_eq(hi))
        }
        CondOp::IsBlank | CondOp::IsNotBlank => {
            let blank = if dtype.is_string() {
                col(name).is_null().or(col(name).str().strip_chars(lit(NULL)).eq(lit("")))
            } else {
                col(name).is_null()
            };
            if cond.op == CondOp::IsBlank {
                blank
            } else {
                blank.not()
            }
        }
    })
}

fn parse_val_expr(val: &str, as_float: bool) -> Expr {
    if as_float {
        let num = val.trim().parse::<f64>().unwrap_or(0.0);
        lit(num)
    } else {
        lit(val.to_string())
    }
}

fn apply_groupby_pivot(
    lf: LazyFrame,
    group_cols: &[String],
    aggs: &[AggSpec],
    sort_by_col: Option<&str>,
    top_n: Option<usize>,
) -> Result<LazyFrame> {
    if group_cols.is_empty() || aggs.is_empty() {
        return Ok(lf);
    }

    let g_exprs: Vec<Expr> = group_cols.iter().map(|c| col(c)).collect();
    let mut agg_exprs: Vec<Expr> = Vec::with_capacity(aggs.len());
    let mut pct_cols_to_post_compute: Vec<(String, String)> = Vec::new();

    for spec in aggs {
        let tc = col(&spec.target_col);
        let expr = match spec.func {
            AggFunc::Sum => tc.cast(DataType::Float64).sum().alias(&spec.alias),
            AggFunc::Count => tc.count().alias(&spec.alias),
            AggFunc::Mean => tc.cast(DataType::Float64).mean().alias(&spec.alias),
            AggFunc::Min => tc.min().alias(&spec.alias),
            AggFunc::Max => tc.max().alias(&spec.alias),
            AggFunc::WeightedAvg => {
                let w_name = spec
                    .weight_col
                    .as_deref()
                    .map(str::trim)
                    .filter(|w| !w.is_empty())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Weighted average needs a weight column (metric '{}')",
                            spec.alias
                        )
                    })?;
                let w = col(w_name).cast(DataType::Float64);
                let v = tc.cast(DataType::Float64);
                // Only rows where both value and weight are known count; weights of mixed sign
                // give a null rate (counted by `weighted_mixed_sign_notes`)
                weighted_rate_exprs(v, w).0.alias(&spec.alias)
            }
            AggFunc::PctOfTotal => {
                let temp_sum_name = format!("__sum_for_{}", spec.alias);
                pct_cols_to_post_compute.push((temp_sum_name.clone(), spec.alias.clone()));
                tc.cast(DataType::Float64).sum().alias(&temp_sum_name)
            }
        };
        agg_exprs.push(expr);
    }

    let mut out_lf = lf.group_by(g_exprs).agg(agg_exprs);

    for (temp_sum, final_alias) in &pct_cols_to_post_compute {
        out_lf = out_lf
            .with_column(
                (col(temp_sum) / col(temp_sum).sum() * lit(100.0))
                    .round(2)
                    .alias(final_alias),
            )
            .drop([temp_sum.as_str()]);
    }

    if let Some(sc) = sort_by_col {
        out_lf = out_lf.sort(
            [sc],
            SortMultipleOptions::default().with_order_descending(true),
        );
    }

    if let Some(n) = top_n {
        if n > 0 {
            out_lf = out_lf.limit(n as IdxSize);
        }
    }

    Ok(out_lf)
}

/// Keyed blake3 mask of one value: `CLIENT_` or `ACCT_` + 16 upper-case hex characters
/// (64 bits). Nothing of the original value is kept. Blank stays blank.
pub fn mask_pii_value(val: &str, mode: PiiMaskMode, key: &[u8; 32]) -> String {
    let trimmed = val.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let token = |round: u32| -> String {
        let hash = if round == 0 {
            blake3::keyed_hash(key, trimmed.as_bytes())
        } else {
            let mut h = blake3::Hasher::new_keyed(key);
            h.update(trimmed.as_bytes());
            h.update(&[0]);
            h.update(&round.to_le_bytes());
            h.finalize()
        };
        hash.to_hex()[..16].to_uppercase()
    };
    match mode {
        PiiMaskMode::None => trimmed.to_string(),
        PiiMaskMode::ClientNameHash => format!("CLIENT_{}", token(0)),
        PiiMaskMode::AccountNumber => {
            // Hex digits can repeat 3 digits of the account number by chance; the next round of
            // the keyed hash is taken until they don't (deterministic for a value and key).
            let digits: Vec<char> = trimmed.chars().collect();
            let leaks = |t: &str| {
                digits.windows(3).any(|w| {
                    w.iter().all(|c| c.is_ascii_digit()) && t.contains(&w.iter().collect::<String>())
                })
            };
            for round in 0..256 {
                let t = token(round);
                if !leaks(&t) {
                    return format!("ACCT_{}", t);
                }
            }
            // Not reachable in practice; never return digits of the original.
            let safe: String = token(0)
                .chars()
                .map(|c| match c.to_digit(10) {
                    Some(d) => char::from(b'G' + d as u8),
                    None => c,
                })
                .collect();
            format!("ACCT_{}", safe)
        }
    }
}

/// The key text is sorted by (T2, owner answer Q4): case-insensitive, and every run of digits
/// compared by its value ("Branch 2" before "Branch 10"). A digit run is written as its length
/// (four digits) and its digits without leading zeros, so byte order of the keys is the natural
/// order; equal keys ("CTG" and "ctg", "02" and "2") are ties and keep their earlier order.
/// The Python export mirrors this (`_mp_natural_key`).
pub fn natural_sort_key(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut out = String::with_capacity(lower.len() + 8);
    let mut chars = lower.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            let mut digits = String::new();
            while let Some(&d) = chars.peek() {
                if !d.is_ascii_digit() {
                    break;
                }
                digits.push(d);
                chars.next();
            }
            let t = digits.trim_start_matches('0');
            let t = if t.is_empty() { "0" } else { t };
            out.push_str(&format!("{:04}", t.len().min(9999)));
            out.push_str(t);
        } else {
            out.push(c);
            chars.next();
        }
    }
    out
}

pub fn fetch_viewport_slice(lf: LazyFrame, offset: usize, limit: usize) -> Result<DataFrame> {
    let df = lf
        .slice(offset as i64, limit as IdxSize)
        .collect()
        .context("Failed to collect viewport slice")?;
    Ok(df)
}

/// The number a math column uses as its right-hand side. Thousands commas are allowed;
/// anything else that is not a finite number is an error naming the value.
pub fn parse_math_scalar(new_col: &str, value: &str) -> Result<f64> {
    let cleaned = value.trim().replace(',', "");
    match cleaned.parse::<f64>() {
        Ok(v) if v.is_finite() => Ok(v),
        _ => anyhow::bail!(
            "Math column '{}': the value '{}' is not a number. Enter a number such as 10000000 or 0.5.",
            new_col,
            value
        ),
    }
}

/// Lines of a file that hold anything other than whitespace (CR/LF or LF line ends).
pub fn count_nonempty_lines(path: &Path) -> Result<usize> {
    let mut reader = BufReader::new(
        File::open(path).with_context(|| format!("Failed to open {:?}", path))?,
    );
    let mut buf = Vec::new();
    let mut n = 0usize;
    let mut progress = ByteLoop::new("Counting the file's lines", path);
    loop {
        buf.clear();
        let read = reader
            .read_until(b'\n', &mut buf)
            .with_context(|| format!("Failed to read {:?}", path))?;
        if read == 0 {
            break;
        }
        progress.add(read);
        // A UTF-8 byte-order mark alone does not make a line non-empty.
        let body = buf.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&buf);
        if body.iter().any(|b| !b.is_ascii_whitespace()) {
            n += 1;
        }
    }
    Ok(n)
}

/// For a CSV source (single file, no steps): rows read against the file's data lines.
/// `None` for other sources.
pub fn source_row_check(source: &TabSource, temp_dir: &Path) -> Result<Option<RowCountCheck>> {
    let TabSource::SingleFile {
        path,
        is_parquet: false,
        config,
    } = source
    else {
        return Ok(None);
    };
    let lf = build_lazy_pipeline(path, false, config, temp_dir)?;
    let (parsed_rows, _) = compute_control_totals(lf, None)?;
    let mut check = RowCountCheck {
        nonempty_lines: count_nonempty_lines(path)?,
        skip_rows: config.skip_rows,
        parsed_rows,
        ..Default::default()
    };
    // Fewer rows than lines: find out why (one more pass over the file, only then).
    if check.parsed_rows < check.data_lines() {
        let a = analyze_records(path, config)?;
        check.multiline_records = a.multiline_records;
        check.explained_lines = a.explained_lines;
        check.malformed_records = a.malformed_records;
        check.details = a.details;
    }
    Ok(Some(check))
}

/// What a pass over a CSV file found about its records.
#[derive(Debug, Default)]
pub struct RecordAnalysis {
    pub multiline_records: usize,
    pub explained_lines: usize,
    pub malformed_records: usize,
    pub details: Vec<LineDetail>,
}

/// Walks the file record by record (a record is a row of fields; a quoted value may hold line
/// breaks, so a record can take several physical lines) and reports the records that take more
/// than one line and those whose number of fields differs from the header's. Line numbers are
/// physical, 1-based, counting title lines and blank lines.
pub fn analyze_records(path: &Path, config: &IngestionConfig) -> Result<RecordAnalysis> {
    let sep = separator_byte(config.separator)?;
    let mut reader = BufReader::new(File::open(path).with_context(|| format!("Failed to open {:?}", path))?);
    let mut progress = ByteLoop::new("Checking line breaks", path);
    let mut out = RecordAnalysis::default();
    let mut buf = Vec::new();
    let mut line_no = 0usize;
    // the record being read
    let mut in_quoted = false;
    let mut quote_seen = false; // a quote inside a quoted value (an escape or the closing one)
    let mut at_start = true; // at the start of a field
    let mut fields = 1usize;
    let mut start_line = 0usize;
    let mut first_text = String::new();
    let mut span = 0usize;
    let mut nonblank = 0usize;
    let mut expected: Option<usize> = None;
    let mut open = false;
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf).with_context(|| format!("Failed to read {:?}", path))?;
        if n == 0 {
            break;
        }
        progress.add(n);
        line_no += 1;
        if line_no <= config.skip_rows {
            continue;
        }
        let body = buf.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&buf);
        let line: &[u8] = {
            let mut e = body.len();
            while e > 0 && matches!(body[e - 1], b'\n' | b'\r') {
                e -= 1;
            }
            &body[..e]
        };
        let blank = !line.iter().any(|b| !b.is_ascii_whitespace());
        if !open {
            if blank {
                continue; // a blank line between records is not a record
            }
            open = true;
            start_line = line_no;
            first_text = decode_line(line, config.encoding).chars().take(80).collect();
            span = 0;
            nonblank = 0;
            fields = 1;
            at_start = true;
        }
        span += 1;
        if !blank {
            nonblank += 1;
        }
        for &b in line {
            if in_quoted {
                if quote_seen {
                    if b == b'"' {
                        quote_seen = false; // "" is a quote inside the value
                        continue;
                    }
                    in_quoted = false; // the quote before this byte closed the value
                    quote_seen = false;
                } else {
                    if b == b'"' {
                        quote_seen = true;
                    }
                    continue;
                }
            }
            if b == sep {
                fields += 1;
                at_start = true;
            } else if at_start && b == b'"' {
                in_quoted = true;
                at_start = false;
            } else {
                at_start = false;
            }
        }
        if in_quoted && quote_seen {
            in_quoted = false; // the closing quote was the last byte of the line
            quote_seen = false;
        }
        if in_quoted {
            continue; // the record goes on in the next line
        }
        // the record is complete
        open = false;
        let header_fields = match expected {
            None => {
                expected = Some(fields);
                continue; // the header line
            }
            Some(h) => h,
        };
        if span > 1 {
            out.multiline_records += 1;
            out.explained_lines += nonblank.saturating_sub(1);
        }
        let wrong = fields != header_fields;
        if wrong {
            out.malformed_records += 1;
        }
        if (span > 1 || wrong) && out.details.len() < ROW_DETAILS_LIMIT {
            out.details.push(LineDetail {
                line: start_line,
                issue: if wrong { LineIssue::WrongFieldCount } else { LineIssue::QuotedLineBreak },
                span,
                fields,
                expected_fields: header_fields,
                text: first_text.clone(),
            });
        }
    }
    Ok(out)
}

/// Rows where the open or the maturity date is missing (after conversion to a date).
pub fn count_missing_ftp_dates(lf: LazyFrame, opndat_col: &str, matdat_col: &str) -> Result<usize> {
    let lf = ensure_date(lf, opndat_col)?;
    let lf = ensure_date(lf, matdat_col)?;
    let df = lf
        .select([col(opndat_col)
            .is_null()
            .or(col(matdat_col).is_null())
            .cast(DataType::Int64)
            .sum()
            .alias("__mp_missing")])
        .collect()
        .context("Could not count rows with a missing open or maturity date")?;
    Ok(df
        .column("__mp_missing")
        .ok()
        .and_then(|c| c.i64().ok())
        .and_then(|ca| ca.get(0))
        .unwrap_or(0) as usize)
}

/// Rows where `column` is blank (null, or text that is not a number).
pub fn count_blank_values(lf: LazyFrame, column: &str) -> Result<usize> {
    let df = lf
        .select([col(column)
            .cast(DataType::Float64)
            .is_null()
            .sum()
            .cast(DataType::Int64)
            .alias("__mp_blank")])
        .collect()
        .with_context(|| format!("Could not count blank values in '{}'", column))?;
    Ok(df
        .column("__mp_blank")
        .ok()
        .and_then(|c| c.i64().ok())
        .and_then(|ca| ca.get(0))
        .unwrap_or(0) as usize)
}

/// Provisioning, limit utilisation, ΔNII and duration treat a blank balance as 0. One note per
/// such step whose input has blank balances: "<step>: Balance blank on N rows; treated as 0".
/// An FTP step also gets a note when rows lack an open or maturity date (no FTP rate for them).
pub fn blank_balance_notes(
    source: &TabSource,
    steps: &[TransformStep],
    temp_dir: &Path,
) -> Result<Vec<String>> {
    let mut notes = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        if let TransformStep::FtpCurveMap { opndat_col, matdat_col, .. } = step {
            let input = compile_tab_lazy(source, &steps[..i], temp_dir)?;
            let n = count_missing_ftp_dates(input, opndat_col, matdat_col)?;
            if n > 0 {
                notes.push(format!(
                    "{}: open or maturity date missing on {} rows; FTP rate and spread left blank (FTP_Method {})",
                    step.short_label(),
                    n,
                    FTP_MISSING_DATE
                ));
            }
        }
        let bal = match step {
            TransformStep::LoanClassificationProvision { bal_col, .. }
            | TransformStep::LoanLimitUtilization { bal_col, .. }
            | TransformStep::RateShockSim { bal_col, .. }
            | TransformStep::DurationAnalysis { bal_col, .. } => bal_col,
            _ => continue,
        };
        let input = compile_tab_lazy(source, &steps[..i], temp_dir)?;
        let n = count_blank_values(input, bal)?;
        if n > 0 {
            notes.push(format!(
                "{}: Balance blank on {} rows; treated as 0",
                step.short_label(),
                n
            ));
        }
    }
    Ok(notes)
}

/// Group-by steps with a weighted average: one note per step with groups whose weights mix
/// positive and negative, e.g. "Group/Pivot by [\"Grp\"]: WAVG_Rate is null for 2 groups (balances
/// of mixed sign)".
pub fn weighted_mixed_sign_notes(
    source: &TabSource,
    steps: &[TransformStep],
    temp_dir: &Path,
) -> Result<Vec<String>> {
    let mut notes = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        let TransformStep::GroupByPivot { group_cols, aggs, .. } = step else {
            continue;
        };
        let weighted: Vec<(&AggSpec, &str)> = aggs
            .iter()
            .filter(|a| a.func == AggFunc::WeightedAvg)
            .filter_map(|a| a.weight_col.as_deref().map(str::trim).map(|w| (a, w)))
            .filter(|(_, w)| !w.is_empty())
            .collect();
        if group_cols.is_empty() || weighted.is_empty() {
            continue;
        }
        let input = compile_tab_lazy(source, &steps[..i], temp_dir)?;
        for (a, w) in weighted {
            let (_, mixed) = weighted_rate_exprs(
                col(&a.target_col).cast(DataType::Float64),
                col(w).cast(DataType::Float64),
            );
            let df = input
                .clone()
                .group_by(group_cols.iter().map(|c| col(c)).collect::<Vec<_>>())
                .agg([mixed.alias("__mp_mixed")])
                .select([col("__mp_mixed").cast(DataType::Int64).sum().alias("n")])
                .collect()
                .with_context(|| format!("Could not check the weights of '{}' for mixed signs", a.alias))?;
            let n = df
                .column("n")
                .ok()
                .and_then(|c| c.i64().ok())
                .and_then(|ca| ca.get(0))
                .unwrap_or(0);
            if n > 0 {
                notes.push(format!(
                    "{}: {} is null for {} group(s) whose '{}' values mix positive and negative",
                    step.short_label(),
                    a.alias,
                    n,
                    w
                ));
            }
        }
    }
    Ok(notes)
}

/// Rows and control sum for the status bar, or "—" with the reason. The pipeline result is
/// passed in so a failed step also shows "—" instead of the previous step's totals.
pub fn control_totals_state(lf: Result<LazyFrame>, control_col: Option<&str>) -> ControlTotals {
    let unavailable = |why: String| ControlTotals {
        rows: None,
        sum: None,
        note: Some(why),
    };
    let mut lf = match lf {
        Ok(lf) => lf,
        Err(e) => return unavailable(format!("Totals unavailable: {:#}", e)),
    };
    let schema = match lf.collect_schema() {
        Ok(s) => s,
        Err(e) => return unavailable(format!("Totals unavailable: {}", e)),
    };
    let (sum_col, note) = match control_col {
        None => (None, None),
        Some(c) => match schema.get(c) {
            None => (
                None,
                Some(format!(
                    "Control column '{}' no longer exists (for example after a pivot or column selection). Choose another control column.",
                    c
                )),
            ),
            Some(dt) if !dt.is_numeric() => (
                None,
                Some(format!(
                    "Control column '{}' is {:?}, not a number, so it has no sum. Choose a numeric control column.",
                    c, dt
                )),
            ),
            Some(_) => (Some(c), None),
        },
    };
    match compute_control_totals(lf, sum_col) {
        Ok((rows, sum)) => ControlTotals {
            rows: Some(rows),
            sum,
            note,
        },
        Err(e) => unavailable(format!("Totals unavailable: {}", e)),
    }
}

pub fn compute_control_totals(
    lf: LazyFrame,
    control_col: Option<&str>,
) -> Result<(usize, Option<f64>)> {
    let mut aggs = vec![len().alias("__row_count")];
    if let Some(c) = control_col {
        aggs.push(col(c).cast(DataType::Float64).sum().alias("__control_sum"));
    }

    let res = lf.with_streaming(true).select(aggs).collect()?;
    let row_count = res
        .column("__row_count")
        .ok()
        .and_then(|s| s.u32().ok())
        .and_then(|ca| ca.get(0))
        .unwrap_or(0) as usize;

    let control_sum = if control_col.is_some() {
        res.column("__control_sum")
            .ok()
            .and_then(|s| s.f64().ok())
            .and_then(|ca| ca.get(0))
    } else {
        None
    };

    Ok((row_count, control_sum))
}

/// How an export wrote its file (shown in the status message).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportPath {
    /// Written batch by batch; the view never has to fit in memory.
    Streamed,
    /// The whole view was collected in memory, then written (the plan cannot be streamed).
    Collected,
}

impl ExportPath {
    pub fn note(self) -> &'static str {
        match self {
            ExportPath::Streamed => "streamed",
            ExportPath::Collected => "collected in memory first; this view cannot be streamed",
        }
    }
}

/// `Auto` streams when the plan allows it. `CollectOnly` always collects first (tests, and the
/// reference for "both ways give the same file").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportMode {
    Auto,
    CollectOnly,
}

/// Polars says one of these, and has written nothing, when a plan has a step that cannot be
/// streamed (it depends on the step: the whole-plan check, or the in-memory engine refusing a sink).
fn is_not_streamable(e: &PolarsError) -> bool {
    let msg = e.to_string();
    msg.contains("cannot run the whole query in a streaming order")
        || msg.contains("not yet supported in standard engine")
}

pub fn export_to_parquet_stream(
    lf: LazyFrame,
    out_path: &Path,
    dialect: ParquetDialect,
) -> Result<()> {
    export_to_parquet(lf, out_path, dialect, ExportMode::Auto).map(|_| ())
}

fn parquet_compression(dialect: ParquetDialect) -> ParquetCompression {
    match dialect {
        ParquetDialect::ArchiveZstd => ParquetCompression::Zstd(None),
        ParquetDialect::PowerBiSnappy => ParquetCompression::Snappy,
    }
}

const PARQUET_ROW_GROUP: usize = 122_880;

pub fn export_to_parquet(
    lf: LazyFrame,
    out_path: &Path,
    dialect: ParquetDialect,
    mode: ExportMode,
) -> Result<ExportPath> {
    let compression = parquet_compression(dialect);
    if mode == ExportMode::Auto {
        let options = ParquetWriteOptions {
            compression,
            statistics: StatisticsOptions::default(),
            row_group_size: Some(PARQUET_ROW_GROUP),
            data_page_size: None,
            maintain_order: true,
        };
        match lf.clone().sink_parquet(out_path, options) {
            Ok(()) => return Ok(ExportPath::Streamed),
            Err(e) if is_not_streamable(&e) => {} // nothing was written: collect instead
            Err(e) => {
                let _ = std::fs::remove_file(out_path);
                return Err(anyhow::Error::from(e).context("Failed to stream the Parquet export"));
            }
        }
    }

    let mut df = lf
        .with_streaming(true)
        .collect()
        .context("Failed to evaluate pipeline for Parquet export")?;

    let file = File::create(out_path).context("Failed to create output Parquet file on disk")?;
    ParquetWriter::new(BufWriter::with_capacity(8 * 1024 * 1024, file))
        .with_compression(compression)
        .with_row_group_size(Some(PARQUET_ROW_GROUP))
        .finish(&mut df)
        .context("Failed to write Parquet data stream")?;
    Ok(ExportPath::Collected)
}

pub fn export_to_csv_stream(lf: LazyFrame, out_path: &Path, mdy_dates: bool) -> Result<()> {
    export_to_csv(lf, out_path, mdy_dates, ExportMode::Auto).map(|_| ())
}

pub fn export_to_csv(
    lf: LazyFrame,
    out_path: &Path,
    mdy_dates: bool,
    mode: ExportMode,
) -> Result<ExportPath> {
    let date_fmt = if mdy_dates { "%m/%d/%Y" } else { "%Y-%m-%d" };

    if mode == ExportMode::Auto {
        let options = CsvWriterOptions {
            include_header: true,
            maintain_order: true,
            serialize_options: SerializeOptions {
                date_format: Some(date_fmt.to_string()),
                float_scientific: Some(false),
                ..SerializeOptions::default()
            },
            ..CsvWriterOptions::default()
        };
        match lf.clone().sink_csv(out_path, options) {
            Ok(()) => return Ok(ExportPath::Streamed),
            Err(e) if is_not_streamable(&e) => {} // nothing was written: collect instead
            Err(e) => {
                let _ = std::fs::remove_file(out_path);
                return Err(anyhow::Error::from(e).context("Failed to stream the CSV export"));
            }
        }
    }

    let mut df = lf
        .with_streaming(true)
        .collect()
        .context("Failed to evaluate pipeline for CSV export")?;

    let file = File::create(out_path).context("Failed to create output CSV file on disk")?;
    CsvWriter::new(BufWriter::with_capacity(8 * 1024 * 1024, file))
        .include_header(true)
        .with_date_format(Some(date_fmt.to_string()))
        .with_float_scientific(Some(false))
        .finish(&mut df)
        .context("Failed to write CSV data stream")?;
    Ok(ExportPath::Collected)
}
