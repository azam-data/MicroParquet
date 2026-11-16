use crate::banking::{
    apply_banking_hygiene_audit, apply_duration_analysis, apply_entity_normalizer,
    apply_ftp_curve_map, apply_loan_classification_provision, apply_loan_limit_utilization,
    apply_maturity_bucket, apply_rate_shock_sim, build_period_reconcile_lazy,
};
use crate::models::{
    AggFunc, AggSpec, ColDataType, ColumnSchemaConfig, FilterOp, IngestionConfig, MathOp,
    ParquetDialect, PiiMaskMode, SqlJoinKind, TabSource, TabState, TransformStep,
};
use anyhow::{Context, Result};
use chrono::NaiveDate;
use polars::prelude::*;
use polars::sql::SQLContext;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

pub fn preview_csv_raw(path: &Path, n_rows: usize) -> Result<DataFrame> {
    let parse_opts = CsvParseOptions::default()
        .with_encoding(CsvEncoding::LossyUtf8)
        .with_truncate_ragged_lines(true);

    let df = CsvReadOptions::default()
        .with_parse_options(parse_opts)
        .with_infer_schema_length(Some(0))
        .with_ignore_errors(true)
        .with_n_rows(Some(n_rows))
        .try_into_reader_with_file_path(Some(path.to_path_buf()))
        .context("Failed to initialize CSV preview reader")?
        .finish()
        .context("Failed to read CSV preview rows")?;
    Ok(df)
}

pub fn auto_detect_smart_schema(path: &Path) -> Result<IngestionConfig> {
    let sample_df = preview_csv_raw(path, 200)?;
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
        columns.push(ColumnSchemaConfig {
            name: col_name,
            dtype,
            pii_mask: PiiMaskMode::None,
            sample_values: samples,
        });
    }

    Ok(IngestionConfig {
        columns,
        ..Default::default()
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

        let cleaned_num = trimmed
            .trim_start_matches('(')
            .trim_end_matches(')')
            .replace(',', "");
        if cleaned_num.parse::<f64>().is_err() {
            all_float = false;
        }

        let fit = classify_date_value(trimmed);
        all_iso_date &= fit.iso;
        all_dmy_date &= fit.dmy;
        all_mdy_date &= fit.mdy;
    }

    // Order: empty -> ISO date -> D/M/Y vs M/D/Y -> float -> text
    if non_empty == 0 {
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

/// Ultra-fast native Rust date parser that handles single-digit or double-digit months/days
/// (e.g., "2/8/2024", "02/08/2024", "11/8/2026", "11/30/2023", "31-Oct-2026", "2024-02-08",
/// 2-digit years, and trailing timestamps) and returns days since 1970-01-01.
pub fn parse_flexible_banking_date(raw: &str, mode: ColDataType) -> Option<i32> {
    let s = raw.trim().trim_matches('"').trim_matches('\'').trim();
    if s.is_empty()
        || s == "-"
        || s == "0"
        || s.eq_ignore_ascii_case("null")
        || s.eq_ignore_ascii_case("none")
        || s.eq_ignore_ascii_case("na")
        || s.eq_ignore_ascii_case("n/a")
    {
        return None;
    }

    // Strip trailing timestamp if present (e.g. "02/08/2024 00:00:00" or "2024-02-08T12:30:00")
    let date_part = strip_time_part(s);

    let mut parts = date_part.split(['/', '-', '.']);
    let p1_str = parts.next()?.trim();
    let p2_str = parts.next()?.trim();
    let p3_str = parts.next()?.trim();

    let p1: i32 = p1_str.parse().ok()?;
    let p2_month_name = parse_month_name(p2_str);
    let p2: u32 = match p2_month_name {
        Some(m) => m,
        None => p2_str.parse().ok()?,
    };
    let p3: i32 = p3_str.parse().ok()?;

    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1)?;

    // Case 1: First token is a 4-digit year (YYYY-MM-DD or YYYY/MM/DD)
    if p1 >= 1900 && p1 <= 2200 {
        let year = p1;
        let (m, d) = if p2 <= 12 {
            (p2, p3 as u32)
        } else {
            (p3 as u32, p2)
        };
        let dt = NaiveDate::from_ymd_opt(year, m, d)?;
        return Some((dt - epoch).num_days() as i32);
    }

    // Case 2: Third token is the year (2-digit or 4-digit)
    let year = if p3 < 100 {
        if p3 >= 50 {
            1900 + p3
        } else {
            2000 + p3
        }
    } else {
        p3
    };
    if year < 1900 || year > 2200 {
        return None;
    }

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
    if !all_digits(c) || !(c.len() == 2 || c.len() == 4) {
        return fit;
    }
    let Ok(y_raw) = c.parse::<i32>() else {
        return fit;
    };
    let year = match c.len() {
        2 if y_raw >= 50 => 1900 + y_raw,
        2 => 2000 + y_raw,
        _ => y_raw,
    };
    if !(1900..=2200).contains(&year) || !all_digits(a) {
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

pub fn heal_broken_csv_file(input_path: &Path, temp_dir: &Path) -> Result<PathBuf> {
    let healed_path = temp_dir.join(format!(
        "healed_{}",
        input_path.file_name().unwrap_or_default().to_string_lossy()
    ));
    let reader = BufReader::new(File::open(input_path)?);
    let mut writer = BufWriter::new(File::create(&healed_path)?);

    let mut lines = reader.lines();
    let Some(Ok(header)) = lines.next() else {
        anyhow::bail!("CSV file is empty");
    };
    let expected_commas = count_unquoted_commas(&header);
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

        if count_unquoted_commas(&pending_line) >= expected_commas {
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

fn count_unquoted_commas(s: &str) -> usize {
    let mut in_quotes = false;
    let mut count = 0;
    for c in s.chars() {
        if c == '"' {
            in_quotes = !in_quotes;
        } else if c == ',' && !in_quotes {
            count += 1;
        }
    }
    count
}

pub fn build_lazy_pipeline(
    path: &Path,
    is_parquet: bool,
    config: &IngestionConfig,
    temp_dir: &Path,
) -> Result<LazyFrame> {
    if is_parquet {
        let lf = LazyFrame::scan_parquet(path, ScanArgsParquet::default())
            .context("Failed to scan Parquet file")?;
        return Ok(lf);
    }

    let actual_csv_path = if config.heal_broken_csv_lines {
        heal_broken_csv_file(path, temp_dir)?
    } else {
        path.to_path_buf()
    };

    let mut schema = Schema::with_capacity(config.columns.len());
    for col_cfg in &config.columns {
        schema.with_column(col_cfg.name.clone().into(), DataType::String);
    }

    let mut lf = LazyCsvReader::new(&actual_csv_path)
        .with_dtype_overwrite(Some(Arc::new(schema)))
        .with_encoding(CsvEncoding::LossyUtf8)
        .with_ignore_errors(true)
        .with_truncate_ragged_lines(true)
        .with_try_parse_dates(false)
        .with_infer_schema_length(Some(0))
        .finish()
        .context("Failed to build LazyCsvReader")?;

    let mut exprs: Vec<Expr> = Vec::with_capacity(config.columns.len());

    for col_cfg in &config.columns {
        let cname = &col_cfg.name;
        let mut e = col(cname);

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

        e = match col_cfg.dtype {
            ColDataType::Utf8 => e,
            ColDataType::Float64 => {
                if config.parse_accounting_negatives {
                    e = e
                        .str()
                        .replace_all(lit(","), lit(""), true)
                        .str()
                        .replace(lit(r"^\((.*)\)$"), lit("-${1}"), false);
                }
                e.cast(DataType::Float64)
            }
            ColDataType::Int64 => {
                if config.parse_accounting_negatives {
                    e = e.str().replace_all(lit(","), lit(""), true);
                }
                e.cast(DataType::Int64)
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
        };

        exprs.push(e.alias(cname));
    }

    if !exprs.is_empty() {
        lf = lf.with_columns(exprs);
    }

    Ok(lf)
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
        } => apply_loan_classification_provision(
            lf,
            bal_col,
            cl_status_col,
            segment_col.as_deref(),
            suspense_col.as_deref(),
            security_col.as_deref(),
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
