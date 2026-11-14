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
use polars::prelude::*;
use polars::sql::SQLContext;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

pub fn preview_csv_raw(path: &Path, n_rows: usize) -> Result<DataFrame> {
    let df = CsvReadOptions::default()
        .with_infer_schema_length(Some(0))
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
            || trimmed.eq_ignore_ascii_case("null")
            || trimmed.eq_ignore_ascii_case("none")
        {
            continue;
        }
        non_empty += 1;

        if trimmed.len() > 1 && trimmed.starts_with('0') && !trimmed.starts_with("0.") {
            has_leading_zero = true;
        }

        let cleaned_num = trimmed
            .trim_start_matches('(')
            .trim_end_matches(')')
            .replace(',', "");
        if cleaned_num.parse::<f64>().is_err() {
            all_float = false;
        }

        if chrono::NaiveDate::parse_from_str(trimmed, "%m/%d/%Y").is_err() {
            all_mdy_date = false;
        }
        if chrono::NaiveDate::parse_from_str(trimmed, "%d/%m/%Y").is_err() {
            all_dmy_date = false;
        }
        if chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").is_err() {
            all_iso_date = false;
        }
    }

    if non_empty == 0 || has_leading_zero {
        return ColDataType::Utf8;
    }
    if all_iso_date {
        return ColDataType::DateISO;
    }
    if all_mdy_date {
        return ColDataType::DateMDY;
    }
    if all_dmy_date {
        return ColDataType::DateDMY;
    }
    if all_float {
        return ColDataType::Float64;
    }

    ColDataType::Utf8
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
            ColDataType::DateMDY => e.str().to_date(StrptimeOptions {
                format: Some("%m/%d/%Y".into()),
                strict: false,
                exact: true,
                cache: true,
            }),
            ColDataType::DateDMY => e.str().to_date(StrptimeOptions {
                format: Some("%d/%m/%Y".into()),
                strict: false,
                exact: true,
                cache: true,
            }),
            ColDataType::DateISO => e.str().to_date(StrptimeOptions {
                format: Some("%Y-%m-%d".into()),
                strict: false,
                exact: true,
                cache: true,
            }),
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
            Ok(lf.rename([old_name.as_str()], [new_name.as_str()], true))
        }
        TransformStep::FilterRows {
            column,
            operator,
            value,
            keep_matching,
        } => {
            let cond = build_filter_expr(column, *operator, value);
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
            let first_cond =
                build_filter_expr(&rules[0].col_name, rules[0].op, &rules[0].compare_val);
            let first_out = parse_val_expr(&rules[0].output_val, *output_as_float);
            let mut chain = when(first_cond).then(first_out);

            for r in rules.iter().skip(1) {
                let c = build_filter_expr(&r.col_name, r.op, &r.compare_val);
                let o = parse_val_expr(&r.output_val, *output_as_float);
                chain = chain.when(c).then(o);
            }

            let fb = parse_val_expr(fallback, *output_as_float);
            Ok(lf.with_column(chain.otherwise(fb).alias(new_col)))
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
            preset,
            null_as_sight,
            new_col,
        } => apply_maturity_bucket(lf, mat_col, cutoff_date, *preset, *null_as_sight, new_col),
        TransformStep::RateShockSim {
            bal_col,
            rate_col,
            mat_col,
            cutoff_date,
            shock_bps,
        } => apply_rate_shock_sim(lf, bal_col, rate_col, mat_col, cutoff_date, *shock_bps),
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
        } => apply_duration_analysis(
            lf,
            bal_col,
            rate_col,
            mat_col,
            cutoff_date,
            *compounding_freq,
            *shock_bps,
            *default_sight_years,
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

fn build_filter_expr(col_name: &str, op: FilterOp, raw_val: &str) -> Expr {
    let c = col(col_name);
    let trimmed = raw_val.trim();

    match op {
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
        _ => {
            if let Ok(num) = trimmed.parse::<f64>() {
                let cf = c.cast(DataType::Float64);
                match op {
                    FilterOp::Eq => cf.eq(lit(num)),
                    FilterOp::NotEq => cf.neq(lit(num)),
                    FilterOp::Gt => cf.gt(lit(num)),
                    FilterOp::Gte => cf.gt_eq(lit(num)),
                    FilterOp::Lt => cf.lt(lit(num)),
                    FilterOp::Lte => cf.lt_eq(lit(num)),
                    _ => unreachable!(),
                }
            } else {
                let cs = c.cast(DataType::String);
                match op {
                    FilterOp::Eq => cs.eq(lit(trimmed)),
                    FilterOp::NotEq => cs.neq(lit(trimmed)),
                    FilterOp::Gt => cs.gt(lit(trimmed)),
                    FilterOp::Gte => cs.gt_eq(lit(trimmed)),
                    FilterOp::Lt => cs.lt(lit(trimmed)),
                    FilterOp::Lte => cs.lt_eq(lit(trimmed)),
                    _ => unreachable!(),
                }
            }
        }
    }
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
                let w_name = spec.weight_col.as_deref().unwrap_or(&spec.target_col);
                let w = col(w_name).cast(DataType::Float64);
                let v = tc.cast(DataType::Float64);
                ((v * w.clone()).sum() / w.sum()).alias(&spec.alias)
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

    let res = lf.select(aggs).collect()?;
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

/// Exports a LazyFrame to Parquet using Zstd or Power BI Safe Snappy
pub fn export_to_parquet_stream(
    lf: LazyFrame,
    out_path: &Path,
    dialect: ParquetDialect,
) -> Result<()> {
    let compression = match dialect {
        ParquetDialect::ArchiveZstd => ParquetCompression::Zstd(None),
        ParquetDialect::PowerBiSnappy => ParquetCompression::Snappy,
    };

    let mut df = lf.collect().context("Failed to evaluate pipeline for Parquet export")?;
    let file = File::create(out_path).context("Failed to create output Parquet file")?;
    ParquetWriter::new(BufWriter::new(file))
        .with_compression(compression)
        .finish(&mut df)
        .context("Failed to write Parquet file")?;
    Ok(())
}

/// Exports a LazyFrame to CSV (no scientific notation, optional US M/D/YYYY dates)
pub fn export_to_csv_stream(lf: LazyFrame, out_path: &Path, mdy_dates: bool) -> Result<()> {
    let date_fmt = if mdy_dates { "%m/%d/%Y" } else { "%Y-%m-%d" };

    let mut df = lf.collect().context("Failed to evaluate pipeline for CSV export")?;
    let file = File::create(out_path).context("Failed to create output CSV file")?;
    CsvWriter::new(BufWriter::new(file))
        .include_header(true)
        .with_date_format(Some(date_fmt.to_string()))
        .with_float_scientific(Some(false))
        .finish(&mut df)
        .context("Failed to write CSV file")?;
    Ok(())
}
