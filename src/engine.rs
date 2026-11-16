use crate::banking::{
    apply_banking_hygiene_audit, apply_duration_analysis, apply_entity_normalizer,
    apply_ftp_curve_map, apply_loan_classification_provision, apply_loan_limit_utilization,
    apply_maturity_bucket, apply_rate_shock_sim, build_period_reconcile_lazy,
};
use crate::models::{
    column_name_tokens, parquet_native_type, parquet_needs_conversion, AggFunc, AggSpec,
    ColDataType, ColumnSchemaConfig, CsvTextEncoding, FilterOp, IngestionConfig, MathOp,
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
    let s = s.trim();
    s.is_empty()
        || s.eq_ignore_ascii_case("na")
        || s.eq_ignore_ascii_case("n/a")
        || s.eq_ignore_ascii_case("null")
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
    split_dr_cr(strip_currency_prefix(raw.trim())).0.is_some()
}

/// Cleans an amount down to an unsigned digit string and its sign.
/// Order: currency prefix, Dr/Cr marker, thousands commas, brackets, trailing minus, lone "-".
fn clean_amount(
    raw: &str,
    sign: SignConvention,
    accounting_negatives: bool,
) -> std::result::Result<(String, bool), AmountCell> {
    let s = raw.trim();
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

/// Scans every value of each Float64 column whose sign convention is unset and fails when one
/// carries Dr/Cr. `text_cols` pairs each column with an expression giving its raw text.
fn check_dr_cr_conventions(
    raw_lf: &LazyFrame,
    text_cols: &[(&ColumnSchemaConfig, Expr)],
) -> Result<()> {
    let checked: Vec<(&str, Expr)> = text_cols
        .iter()
        .filter(|(c, _)| {
            c.dtype == ColDataType::Float64 && c.sign_convention == SignConvention::Unset
        })
        .map(|(c, e)| (c.name.as_str(), e.clone()))
        .collect();
    if checked.is_empty() {
        return Ok(());
    }
    let cols: Vec<&str> = checked.iter().map(|(n, _)| *n).collect();
    let exprs: Vec<Expr> = checked
        .into_iter()
        .map(|(c, text)| {
            text.map(
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
                .alias(c)
        })
        .collect();
    let flags = raw_lf
        .clone()
        .select(exprs)
        .collect()
        .context("Failed to scan amount columns for Dr/Cr markers")?;
    for c in cols {
        let found = flags
            .column(c)
            .ok()
            .and_then(|s| s.bool().ok().and_then(|b| b.get(0)))
            .unwrap_or(false);
        if found {
            anyhow::bail!(dr_cr_unset_message(c));
        }
    }
    Ok(())
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
    let reader = BufReader::new(File::open(input_path)?);
    let mut writer = BufWriter::new(File::create(&healed_path)?);
    let sep = separator as char;

    let mut lines = reader.lines().skip(skip_rows);
    let Some(Ok(header)) = lines.next() else {
        anyhow::bail!("CSV file is empty");
    };
    let expected = count_unquoted(&header, sep);
    writeln!(writer, "{}", header)?;

    let mut pending_line = String::new();
    for line_res in lines {
        let line = line_res?;
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
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(decode_windows_1252(&buf[..n]).as_bytes())?;
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
        check_dr_cr_conventions(&lf, &plan)?;
        let exprs: Vec<Expr> = plan
            .into_iter()
            .map(|(c, text)| typed_expr(text, c, config).alias(&c.name))
            .collect();
        return Ok(lf.with_columns(exprs));
    }

    let mut lf = scan_csv_text(path, config, temp_dir)?;
    let text_cols: Vec<(&ColumnSchemaConfig, Expr)> =
        config.columns.iter().map(|c| (c, col(&c.name))).collect();
    check_dr_cr_conventions(&lf, &text_cols)?;

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
        let salt = config.pii_salt.clone();
        let mode = col_cfg.pii_mask;
        e = e.map(
            move |s: Series| {
                let ca = s.str()?;
                let out: StringChunked = ca
                    .into_iter()
                    .map(|opt_v| opt_v.map(|v| mask_pii_value(v, mode, &salt)))
                    .collect();
                Ok(Some(out.into_series()))
            },
            GetOutput::from_type(DataType::String),
        );
    }
    e
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
    if is_parquet {
        let (lf, plan) = parquet_conversion_plan(path, config)?;
        let typed: Vec<(&ColumnSchemaConfig, Expr)> = plan
            .into_iter()
            .filter(|(c, _)| c.dtype != ColDataType::Utf8)
            .collect();
        if typed.is_empty() {
            return Ok(Vec::new());
        }
        return failures_from_text(lf, &typed, config.parse_accounting_negatives);
    }
    let typed: Vec<&ColumnSchemaConfig> = config
        .columns
        .iter()
        .filter(|c| c.dtype != ColDataType::Utf8)
        .collect();
    if typed.is_empty() {
        return Ok(Vec::new());
    }
    let lf = scan_csv_text(path, config, temp_dir)?;
    let pairs: Vec<(&ColumnSchemaConfig, Expr)> =
        typed.into_iter().map(|c| (c, raw_text_expr(c, config))).collect();
    failures_from_text(lf, &pairs, config.parse_accounting_negatives)
}

/// Shared by CSV and Parquet: `pairs` holds each typed column and its text expression.
fn failures_from_text(
    lf: LazyFrame,
    pairs: &[(&ColumnSchemaConfig, Expr)],
    accounting_negatives: bool,
) -> Result<Vec<ParseFailure>> {
    let mut exprs = Vec::with_capacity(pairs.len() * 2);
    for (i, (c, text)) in pairs.iter().enumerate() {
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
    let df = lf
        .select(exprs)
        .collect()
        .context("Failed to count values that could not be converted")?;

    let mut out = Vec::new();
    for (i, (c, _)) in pairs.iter().enumerate() {
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
        out.push(ParseFailure {
            column: c.name.clone(),
            count,
            examples,
        });
    }
    Ok(out)
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
        .map(|f| format!("{} {}", f.column, f.count))
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
        } => {
            let mut lfs = Vec::with_capacity(files.len());
            for (path, is_parq, cfg) in files {
                let mut sub_lf = build_lazy_pipeline(path, *is_parq, cfg, temp_dir)?;
                if *include_source_col {
                    let fname = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    sub_lf = sub_lf.with_column(lit(fname).alias("Source_File"));
                }
                lfs.push(sub_lf);
            }
            concat(&lfs, UnionArgs::default()).context("Failed to stack files vertically")?
        }

        TabSource::JoinedTabs {
            left_source,
            left_steps,
            right_source,
            right_steps,
            join_kind,
            join_keys,
            right_cols_to_keep,
        } => {
            let left_lf = compile_tab_lazy(left_source, left_steps, temp_dir)?;
            let mut right_lf = compile_tab_lazy(right_source, right_steps, temp_dir)?;

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
        TransformStep::SelectAndReorderColumns { cols } => {
            let exprs: Vec<Expr> = cols.iter().map(|c| col(c)).collect();
            Ok(lf.select(exprs))
        }
        TransformStep::RenameColumn { old_name, new_name } => {
            Ok(lf.rename([old_name.as_str()], [new_name.as_str()]))
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
                let scalar = right_val_or_col.trim().parse::<f64>().unwrap_or(1.0);
                lit(scalar)
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
        } => apply_ftp_curve_map(
            lf,
            opndat_col,
            matdat_col,
            bal_col,
            rate_col,
            curve,
            *is_liability,
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
            let dtype = schema.get(col_name).ok_or_else(|| {
                anyhow::anyhow!(
                    "Column '{}' was not found. Check the column chosen in this filter.",
                    col_name
                )
            })?;
            let (lhs, rhs) = if dtype.is_string() || dtype.is_categorical() {
                (c.cast(DataType::String), lit(trimmed.to_string()))
            } else if dtype.is_numeric() {
                let num = trimmed.parse::<f64>().map_err(|_| {
                    anyhow::anyhow!(
                        "'{}' is not a number for numeric column '{}'",
                        trimmed,
                        col_name
                    )
                })?;
                (c.cast(DataType::Float64), lit(num))
            } else if matches!(dtype, DataType::Date | DataType::Datetime(_, _)) {
                let d = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").map_err(|_| {
                    anyhow::anyhow!(
                        "'{}' is not a date for date column '{}'. Enter it as YYYY-MM-DD (e.g. 2026-10-31).",
                        trimmed,
                        col_name
                    )
                })?;
                let epoch = NaiveDate::from_ymd_opt(1970, 1, 1)
                    .context("Internal date error (epoch)")?;
                let days = (d - epoch).num_days() as i32;
                (c.cast(DataType::Date), lit(days).cast(DataType::Date))
            } else {
                anyhow::bail!(
                    "Column '{}' is {}; comparison filters work on text, number and date columns only",
                    col_name,
                    dtype
                );
            };
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
                // Only rows where both value and weight are known count, in numerator and denominator
                let mask = v.clone().is_not_null().and(w.clone().is_not_null());
                let num = (v * w.clone()).filter(mask.clone()).sum();
                let den = w.filter(mask).sum();
                when(den.clone().eq(lit(0.0)))
                    .then(lit(NULL).cast(DataType::Float64))
                    .otherwise(num / den)
                    .alias(&spec.alias)
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

fn mask_pii_value(val: &str, mode: PiiMaskMode, salt: &str) -> String {
    let trimmed = val.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(salt.as_bytes());
    hasher.update(trimmed.as_bytes());
    let hex = hasher.finalize().to_hex().to_string().to_uppercase();

    match mode {
        PiiMaskMode::None => trimmed.to_string(),
        PiiMaskMode::ClientNameHash => format!("CLIENT_{}", &hex[..8]),
        PiiMaskMode::AccountNumber => {
            let chars: Vec<char> = trimmed.chars().collect();
            if chars.len() >= 7 {
                let prefix: String = chars[..3].iter().collect();
                let suffix: String = chars[chars.len() - 3..].iter().collect();
                format!("{}{}{}", prefix, &hex[..5], suffix)
            } else {
                format!("ACCT_{}", &hex[..6])
            }
        }
    }
}

pub fn fetch_viewport_slice(lf: LazyFrame, offset: usize, limit: usize) -> Result<DataFrame> {
    let df = lf
        .slice(offset as i64, limit as IdxSize)
        .collect()
        .context("Failed to collect viewport slice")?;
    Ok(df)
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

pub fn export_to_parquet_stream(
    lf: LazyFrame,
    out_path: &Path,
    dialect: ParquetDialect,
) -> Result<()> {
    let compression = match dialect {
        ParquetDialect::ArchiveZstd => ParquetCompression::Zstd(None),
        ParquetDialect::PowerBiSnappy => ParquetCompression::Snappy,
    };

    let mut df = lf
        .with_streaming(true)
        .collect()
        .context("Failed to evaluate pipeline for Parquet export")?;

    let file = File::create(out_path).context("Failed to create output Parquet file on disk")?;
    ParquetWriter::new(BufWriter::with_capacity(8 * 1024 * 1024, file))
        .with_compression(compression)
        .with_row_group_size(Some(122_880))
        .finish(&mut df)
        .context("Failed to write Parquet data stream")?;
    Ok(())
}

pub fn export_to_csv_stream(lf: LazyFrame, out_path: &Path, mdy_dates: bool) -> Result<()> {
    let date_fmt = if mdy_dates { "%m/%d/%Y" } else { "%Y-%m-%d" };

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
    Ok(())
}
