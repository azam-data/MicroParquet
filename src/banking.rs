use crate::models::{portable_root_dir, CustomBucketDef, FtpInterpolation, FtpPoint, RateBandDef};
use anyhow::{anyhow, bail, Context, Result};
use chrono::NaiveDate;
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Checks that a date input is a real Date column before any day-difference maths.
/// Date passes through, Datetime is cast to Date, anything else (text, numbers) is an error,
/// so a wrongly typed column can never be silently bucketed.
pub(crate) fn ensure_date(mut lf: LazyFrame, col_name: &str) -> Result<LazyFrame> {
    let schema = lf
        .collect_schema()
        .context("Could not read the column types of this table")?;
    let dtype = schema.get(col_name).cloned().ok_or_else(|| {
        anyhow!(
            "Column '{}' was not found. Check the date column chosen in this step.",
            col_name
        )
    })?;
    match dtype {
        DataType::Date => Ok(lf),
        DataType::Datetime(_, _) => Ok(lf.with_column(col(col_name).cast(DataType::Date))),
        other => bail!(
            "Column '{}' is {}, not a date. Set it to a Date type in the Schema & Ingestion settings.",
            col_name,
            other
        ),
    }
}

/// Side text -> +1.0 (asset) / -1.0 (liability) / null (not recognised).
/// Trimmed and case-insensitive: "A..." is an asset; "L...", DEPOSIT and BORROWING are liabilities.
fn side_sign_expr(side: Expr) -> Expr {
    let s = side
        .cast(DataType::String)
        .str()
        .strip_chars(lit(NULL))
        .str()
        .to_uppercase();
    when(s.clone().str().starts_with(lit("A")))
        .then(lit(1.0))
        .when(
            s.clone()
                .str()
                .starts_with(lit("L"))
                .or(s.clone().eq(lit("DEPOSIT")))
                .or(s.eq(lit("BORROWING"))),
        )
        .then(lit(-1.0))
        .otherwise(lit(NULL).cast(DataType::Float64))
}

/// Sign (+1 asset, -1 liability) for signed rate-shock outputs, after checking every side value maps.
/// Blank or unrecognised sides are an error listing up to 10 of them. With no side column
/// (macros saved before sides existed) every row is treated as an asset and a
/// `Direction_Assumed` column says so in the output.
fn direction_sign(lf: LazyFrame, side_col: Option<&str>) -> Result<(LazyFrame, Expr)> {
    let side_col = match side_col.map(str::trim) {
        Some(s) if !s.is_empty() => s,
        _ => {
            return Ok((
                lf.with_column(lit("ASSET (unspecified)").alias("Direction_Assumed")),
                lit(1.0),
            ))
        }
    };
    let bad = lf
        .clone()
        .select([col(side_col).cast(DataType::String).alias("__side")])
        .filter(side_sign_expr(col("__side")).is_null())
        .unique_stable(None, UniqueKeepStrategy::First)
        .limit(10)
        .collect()
        .with_context(|| format!("Could not read the side column '{}'", side_col))?;
    if bad.height() > 0 {
        let vals: Vec<String> = bad
            .column("__side")?
            .str()?
            .into_iter()
            .map(|v| match v {
                Some(s) if !s.trim().is_empty() => format!("'{}'", s),
                _ => "(blank)".to_string(),
            })
            .collect();
        bail!(
            "Side column '{}' has values that are neither asset nor liability: {}. Use A/ASSET for assets and L/LIABILITY/DEPOSIT/BORROWING for liabilities.",
            side_col,
            vals.join(", ")
        );
    }
    Ok((lf, side_sign_expr(col(side_col))))
}

/// Feature B8: Standardizes corporate & counterparty names
pub fn apply_entity_normalizer(lf: LazyFrame, source_col: &str, new_col: &str) -> LazyFrame {
    let norm_expr = col(source_col)
        .cast(DataType::String)
        .str()
        .to_uppercase()
        .str()
        .replace_all(lit(r"[.,\-()/]+"), lit(" "), false)
        .str()
        .replace_all(lit(r"\bLIMITED\b"), lit("LTD"), false)
        .str()
        .replace_all(lit(r"\bINCORPORATED\b"), lit("INC"), false)
        .str()
        .replace_all(lit(r"\bCOMPANY\b"), lit("CO"), false)
        .str()
        .replace_all(lit(r"\bCORPORATION\b"), lit("CORP"), false)
        .str()
        .replace_all(lit(r"\bBANGLADESH\b"), lit("BD"), false)
        .str()
        .replace_all(lit(r"\bPRIVATE\b"), lit("PVT"), false)
        .str()
        .replace_all(lit(r"\s+"), lit(" "), false)
        .str()
        .strip_chars(lit(" "))
        .alias(new_col);

    lf.with_column(norm_expr)
}

/// Feature A5 + v1.2 Custom Presets: Regulatory & Custom Maturity Bucketer
pub fn apply_maturity_bucket(
    lf: LazyFrame,
    mat_col: &str,
    cutoff_date: &str,
    bucket_def: &CustomBucketDef,
    null_as_sight: bool,
    new_col: &str,
) -> Result<LazyFrame> {
    let cutoff = NaiveDate::parse_from_str(cutoff_date.trim(), "%Y-%m-%d")
        .context("Cutoff Date must be in YYYY-MM-DD format (e.g., 2026-10-31)")?;
    let lf = ensure_date(lf, mat_col)?;

    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let cutoff_days_since_epoch = (cutoff - epoch).num_days();

    let rem_days_col = "__rem_days_temp";
    let lf_with_days = lf.with_column(
        (col(mat_col).cast(DataType::Int32).cast(DataType::Int64) - lit(cutoff_days_since_epoch))
            .alias(rem_days_col),
    );

    let null_label = if null_as_sight {
        bucket_def.null_sight_label.as_str()
    } else {
        bucket_def.null_long_label.as_str()
    };

    let mut sorted_rules = bucket_def.rules.clone();
    sorted_rules.sort_by_key(|r| r.max_days);

    // Fold rules in reverse so every step produces a clean Polars `Expr`
    let d = col(rem_days_col);
    let mut expr: Expr = lit(bucket_def.overflow_label.as_str());
    for rule in sorted_rules.iter().rev() {
        expr = when(d.clone().lt_eq(lit(rule.max_days)))
            .then(lit(rule.label.as_str()))
            .otherwise(expr);
    }

    // Null label from the computed remaining days, so a failed conversion can't reach overflow
    let final_bucket_expr = when(d.clone().is_null())
        .then(lit(null_label))
        .when(d.lt(lit(0)))
        .then(lit(bucket_def.overdue_label.as_str()))
        .otherwise(expr)
        .alias(new_col);

    Ok(lf_with_days
        .with_column(final_bucket_expr)
        .drop([rem_days_col]))
}

/// Rate profile: puts a rate column (percent) into the bands of `def`, labelled so that the
/// labels sort in band order. Exactly 0 is its own band, below 0 and beyond the last edge have
/// their own labels, and a blank rate is "No rate". A rate column that is not numeric is an
/// error: a text rate would otherwise be read as blank.
pub fn apply_rate_band(lf: LazyFrame, rate_col: &str, def: &RateBandDef, new_col: &str) -> Result<LazyFrame> {
    let mut lf = lf;
    let schema = lf.collect_schema().context("Could not read the column types for this rate profile")?;
    let dtype = schema
        .get(rate_col)
        .ok_or_else(|| anyhow::anyhow!("Column '{}' was not found. Choose the rate column of this rate profile.", rate_col))?;
    if !dtype.is_numeric() {
        anyhow::bail!(
            "Rate column '{}' is {}, not a number. Set it to a number in the schema first; a text rate cannot be put into bands.",
            rate_col,
            dtype
        );
    }
    let labels = def.labels()?;
    let x = col(rate_col).cast(DataType::Float64);
    let mut chain: Expr = lit(labels.above.as_str());
    for k in (1..labels.edges.len()).rev() {
        let hi = lit(labels.edges[k]);
        let within = if def.lower_inclusive { x.clone().lt(hi) } else { x.clone().lt_eq(hi) };
        chain = when(within).then(lit(labels.bands[k - 1].as_str())).otherwise(chain);
    }
    let band = when(x.clone().is_null().or(x.clone().is_nan()))
        .then(lit(labels.blank.as_str()))
        .when(x.clone().eq(lit(0.0)))
        .then(lit(labels.zero.as_str()))
        .when(x.lt(lit(0.0)))
        .then(lit(labels.below.as_str()))
        .otherwise(chain);
    Ok(lf.with_column(band.alias(new_col)))
}

/// Feature B1: Interest Rate Shock / ΔNII Sensitivity Simulator (1-Year Repricing Horizon)
///
/// Output columns:
/// - `Current_Annual_Int`: balance × rate, gross interest income (assets) or expense
///   (liabilities); unsigned.
/// - `NII_Impact_BDT`: change in net interest income, signed by side: + for assets and − for
///   liabilities under a rate rise.
/// - `Shocked_Annual_Int`: gross interest income or expense after the shock, unsigned
///   (`Current_Annual_Int` + the unsigned change). It is not a net figure and must not be summed
///   across assets and liabilities as NII; use `NII_Impact_BDT` for that.
pub fn apply_rate_shock_sim(
    lf: LazyFrame,
    bal_col: &str,
    rate_col: &str,
    mat_col: &str,
    cutoff_date: &str,
    shock_bps: f64,
    side_col: Option<&str>,
) -> Result<LazyFrame> {
    let spec = RateShockSpec {
        bal_col: bal_col.to_string(),
        rate_col: rate_col.to_string(),
        mat_col: mat_col.to_string(),
        cutoff_date: cutoff_date.to_string(),
        shock_bps,
        side_col: side_col.map(str::to_string),
        reprice_col: None,
        beta_file: None,
        product_col: None,
        floor_pct: None,
    };
    apply_rate_shock_with(lf, &spec, &[])
}

/// Everything the rate-shock step takes (the dialog, the step and the repricing-gap tab share it).
#[derive(Clone, Debug, PartialEq)]
pub struct RateShockSpec {
    pub bal_col: String,
    pub rate_col: String,
    pub mat_col: String,
    pub cutoff_date: String,
    pub shock_bps: f64,
    pub side_col: Option<String>,
    pub reprice_col: Option<String>,
    pub beta_file: Option<String>,
    pub product_col: Option<String>,
    pub floor_pct: Option<f64>,
}

fn given(c: &Option<String>) -> Option<&str> {
    c.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// The beta of each row, as the share of the shock its rate follows: 1.0 for a product that is
/// not in the file (and for a row with no product).
fn beta_factor_expr(product_col: &str, betas: &[(String, f64)]) -> Expr {
    let code = normalise_code_expr(col(product_col));
    betas.iter().rev().fold(lit(1.0), |acc, (k, b)| when(code.clone().eq(lit(k.clone()))).then(lit(*b)).otherwise(acc))
}

/// How many rows have a product the beta file does not list (a blank product is one of them).
pub fn count_unmapped_products(lf: LazyFrame, product_col: &str, betas: &[(String, f64)]) -> Result<usize> {
    let known = betas.iter().fold(lit(false), |acc, (k, _)| acc.or(normalise_code_expr(col(product_col)).eq(lit(k.clone()))));
    let df = lf
        .select([known.fill_null(lit(false)).not().cast(DataType::Int64).sum().alias("n")])
        .collect()
        .with_context(|| format!("Could not check the products of '{}' against the beta file", product_col))?;
    Ok(df.column("n")?.i64()?.get(0).unwrap_or(0) as usize)
}

/// The effective repricing date of each row as days since 1970 (null when there is none): the
/// earlier of the repricing date and the maturity, or the maturity when there is no repricing date.
fn effective_days_expr(spec: &RateShockSpec) -> Expr {
    let mat = col(spec.mat_col.as_str()).cast(DataType::Int32).cast(DataType::Int64);
    match given(&spec.reprice_col) {
        None => mat,
        Some(r) => {
            let rep = col(r).cast(DataType::Int32).cast(DataType::Int64);
            when(rep.clone().is_null())
                .then(mat.clone())
                .when(mat.clone().is_null())
                .then(rep.clone())
                .when(rep.clone().lt_eq(mat.clone()))
                .then(rep)
                .otherwise(mat)
        }
    }
}

/// ΔNII with the options of Phase 4 V5. `betas` is the content of the beta file (`spec.beta_file`
/// is read by the caller, so a test or a headless run needs no folder). With no option set the
/// result is exactly what the step gave before.
///
/// * `reprice_col`: a deal reprices on the earlier of that date and its maturity.
/// * betas: the shock is multiplied by the product's pass-through; an unmapped product passes 100%.
/// * `floor_pct`: a fall in the rate stops at the floor; the change is cut to what fits.
pub fn apply_rate_shock_with(lf: LazyFrame, spec: &RateShockSpec, betas: &[(String, f64)]) -> Result<LazyFrame> {
    let cutoff = NaiveDate::parse_from_str(spec.cutoff_date.trim(), "%Y-%m-%d").context("Cutoff Date must be YYYY-MM-DD")?;
    let lf = ensure_date(lf, &spec.mat_col)?;
    let lf = match given(&spec.reprice_col) {
        Some(r) => ensure_date(lf, r)?,
        None => lf,
    };
    let (lf, sign) = direction_sign(lf, spec.side_col.as_deref())?;
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let cutoff_days = (cutoff - epoch).num_days();
    if let Some(f) = spec.floor_pct {
        if !f.is_finite() {
            bail!("ΔNII: the floor is not a number.");
        }
    }
    if given(&spec.beta_file).is_some() && given(&spec.product_col).is_none() {
        bail!("ΔNII: choose the product column the beta file is read against.");
    }

    let bal = col(spec.bal_col.as_str()).cast(DataType::Float64).fill_null(lit(0.0));
    let rate = col(spec.rate_col.as_str()).cast(DataType::Float64).fill_null(lit(0.0));
    let shock_pct = spec.shock_bps / 100.0;

    // days to the effective repricing date; blank maturity (no date at all) reprices at once
    let repricing_weight = if given(&spec.reprice_col).is_none() {
        let rem_days = col(spec.mat_col.as_str()).cast(DataType::Int32).cast(DataType::Int64) - lit(cutoff_days);
        when(col(spec.mat_col.as_str()).is_null().or(rem_days.clone().lt_eq(lit(0))))
            .then(lit(1.0))
            .when(rem_days.clone().gt(lit(365)))
            .then(lit(0.0))
            .otherwise((lit(365.0) - rem_days.cast(DataType::Float64)) / lit(365.0))
    } else {
        let eff = effective_days_expr(spec);
        let rem_days = eff.clone() - lit(cutoff_days);
        when(eff.is_null().or(rem_days.clone().lt_eq(lit(0))))
            .then(lit(1.0))
            .when(rem_days.clone().gt(lit(365)))
            .then(lit(0.0))
            .otherwise((lit(365.0) - rem_days.cast(DataType::Float64)) / lit(365.0))
    };

    let current_int = (bal.clone() * (rate.clone() / lit(100.0))).alias("Current_Annual_Int");
    // the change in the rate, as a fraction: the shock, times the pass-through, cut by the floor
    let rate_change: Expr = if given(&spec.beta_file).is_none() && spec.floor_pct.is_none() {
        lit(shock_pct / 100.0)
    } else {
        let beta = match (given(&spec.beta_file), given(&spec.product_col)) {
            (Some(_), Some(p)) => beta_factor_expr(p, betas),
            _ => lit(1.0),
        };
        let delta = lit(shock_pct) * beta;
        let delta = match spec.floor_pct {
            // a fall stops at the floor; a rate already at or under it does not fall; a rise is untouched
            Some(f) => when(delta.clone().lt(lit(0.0)))
                .then(when((lit(f) - rate.clone()).gt(delta.clone())).then(lit(f) - rate.clone()).otherwise(delta.clone()).clip_max(lit(0.0)))
                .otherwise(delta),
            None => delta,
        };
        delta / lit(100.0)
    };
    // Change in the interest amount (income on assets, expense on liabilities).
    let int_change = bal * repricing_weight * rate_change;
    let nii_impact = (int_change.clone() * sign).alias("NII_Impact_BDT");
    let shocked_int = (col("Current_Annual_Int") + int_change).alias("Shocked_Annual_Int");

    Ok(lf.with_columns([current_int, nii_impact]).with_column(shocked_int))
}

// ----------------------------------------------------------------------------- repricing gap

/// The buckets of the repricing-gap summary, nearest first.
pub const REPRICING_BUCKETS: [&str; 5] = ["0\u{2013}1m", "1\u{2013}3m", "3\u{2013}6m", "6\u{2013}12m", ">12m/non-repricing"];
pub const REPRICING_TOTAL_ROW: &str = "Total";

/// The repricing-gap summary of a book: for each repricing bucket the assets, the liabilities
/// (negative), their sum (the gap), the cumulative gap and the ΔNII of the shock (with the same
/// reprice date, betas and floor as the step), then a Total row. Rows that reprice at once
/// (blank date, or a date on or before the cutoff) are in the first bucket; days to the
/// effective repricing date 1 to 30, 31 to 90, 91 to 180 and 181 to 365 are in the next four,
/// later or never in the last.
pub fn build_repricing_gap(lf: LazyFrame, spec: &RateShockSpec, betas: &[(String, f64)]) -> Result<DataFrame> {
    let cutoff = NaiveDate::parse_from_str(spec.cutoff_date.trim(), "%Y-%m-%d").context("Cutoff Date must be YYYY-MM-DD")?;
    let cutoff_days = days_since_epoch(cutoff);
    let mut lf = lf;
    let schema = lf.collect_schema().context("Could not read the columns for the repricing gap")?;
    let dt = schema.get(spec.bal_col.as_str()).cloned().ok_or_else(|| anyhow!("Column '{}' was not found. Choose the balance column again.", spec.bal_col))?;
    if !dt.is_numeric() {
        bail!("The balance column '{}' is {}, not a number. Set it to a number in the schema first.", spec.bal_col, dt);
    }
    let shocked = apply_rate_shock_with(lf, spec, betas)?;
    let (shocked, sign) = direction_sign(shocked, spec.side_col.as_deref())?;
    let _ = sign;
    let eff = effective_days_expr(spec);
    let rem = eff.clone() - lit(cutoff_days);
    let bucket = when(eff.is_null().or(rem.clone().lt_eq(lit(0))))
        .then(lit(0i32))
        .when(rem.clone().lt_eq(lit(30)))
        .then(lit(0i32))
        .when(rem.clone().lt_eq(lit(90)))
        .then(lit(1i32))
        .when(rem.clone().lt_eq(lit(180)))
        .then(lit(2i32))
        .when(rem.lt_eq(lit(365)))
        .then(lit(3i32))
        .otherwise(lit(4i32))
        .alias("__bucket");
    let bal = col(spec.bal_col.as_str()).cast(DataType::Float64).fill_null(lit(0.0));
    let side = match spec.side_col.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => side_sign_expr(col(s)),
        None => lit(1.0),
    };
    let df = shocked
        .with_column(bucket)
        .group_by([col("__bucket")])
        .agg([
            when(side.clone().gt(lit(0.0))).then(bal.clone()).otherwise(lit(0.0)).sum().alias("__a"),
            when(side.lt(lit(0.0))).then(bal).otherwise(lit(0.0)).sum().alias("__l"),
            col("NII_Impact_BDT").sum().alias("__n"),
        ])
        .collect()
        .context("Could not calculate the repricing gap")?;
    let idx: Vec<Option<i32>> = df.column("__bucket")?.i32()?.into_iter().collect();
    let (a, l, n) = (df.column("__a")?.f64()?, df.column("__l")?.f64()?, df.column("__n")?.f64()?);
    let mut assets = [0.0f64; 5];
    let mut liabs = [0.0f64; 5];
    let mut nii = [0.0f64; 5];
    for (i, b) in idx.iter().enumerate() {
        let Some(b) = b else { continue };
        let b = (*b).clamp(0, 4) as usize;
        assets[b] += a.get(i).unwrap_or(0.0);
        liabs[b] -= l.get(i).unwrap_or(0.0);
        nii[b] += n.get(i).unwrap_or(0.0);
    }
    let gap: Vec<f64> = assets.iter().zip(&liabs).map(|(x, y)| x + y).collect();
    let mut run = 0.0;
    let cum: Vec<f64> = gap.iter().map(|g| { run += g; run }).collect();
    let mut names: Vec<String> = REPRICING_BUCKETS.iter().map(|s| s.to_string()).collect();
    names.push(REPRICING_TOTAL_ROW.to_string());
    let with_total = |v: &[f64]| -> Vec<f64> { let mut o = v.to_vec(); o.push(v.iter().sum()); o };
    let mut cumulative: Vec<Option<f64>> = cum.iter().map(|x| Some(*x)).collect();
    cumulative.push(None);
    DataFrame::new(vec![
        Series::new("Repricing bucket".into(), names),
        Series::new("Assets".into(), with_total(&assets)),
        Series::new("Liabilities".into(), with_total(&liabs)),
        Series::new("Gap".into(), with_total(&gap)),
        Series::new("Cumulative gap".into(), cumulative),
        Series::new("\u{394}NII (BDT)".into(), with_total(&nii)),
    ])
    .context("Could not arrange the repricing gap")
}

/// Feature B2: Automated FTP (Fund Transfer Pricing) Yield Curve Mapper using Tenor Days
/// FTP rate and method label for `Tenor_Days`, from the curve sorted by tenor. Below the first
/// point the first rate applies, beyond the last point the last rate.
fn ftp_rate_exprs(curve: &[FtpPoint], interpolation: FtpInterpolation) -> Result<(Expr, Expr)> {
    let mut pts = curve.to_vec();
    pts.sort_by_key(|p| p.tenor_days);
    let (Some(first), Some(last)) = (pts.first().cloned(), pts.last().cloned()) else {
        anyhow::bail!("The FTP curve has no points. Add at least one tenor point.");
    };
    for w in pts.windows(2) {
        if w[0].tenor_days == w[1].tenor_days {
            anyhow::bail!(
                "The FTP curve has two points at {} days ({}% and {}%). Keep one rate per tenor.",
                w[0].tenor_days,
                w[0].ftp_rate,
                w[1].ftp_rate
            );
        }
    }
    for p in &pts {
        if !p.ftp_rate.is_finite() {
            anyhow::bail!("The FTP curve point at {} days has no valid rate.", p.tenor_days);
        }
    }
    let t = col("Tenor_Days").cast(DataType::Float64);
    let mut rate: Expr = lit(last.ftp_rate);
    let mut method: Expr = lit(format!("Last point {}d (beyond curve)", last.tenor_days));
    match interpolation {
        FtpInterpolation::NextPointUp => {
            for p in pts.iter().rev() {
                let cond = col("Tenor_Days").lt_eq(lit(p.tenor_days));
                rate = when(cond.clone()).then(lit(p.ftp_rate)).otherwise(rate);
                method = when(cond)
                    .then(lit(format!("Next point up {}d", p.tenor_days)))
                    .otherwise(method);
            }
        }
        FtpInterpolation::Linear => {
            for w in pts.windows(2).rev() {
                let (a, b) = (&w[0], &w[1]);
                let slope = (b.ftp_rate - a.ftp_rate) / (b.tenor_days - a.tenor_days) as f64;
                let cond = col("Tenor_Days").lt_eq(lit(b.tenor_days));
                rate = when(cond.clone())
                    .then(lit(a.ftp_rate) + (t.clone() - lit(a.tenor_days as f64)) * lit(slope))
                    .otherwise(rate);
                method = when(cond)
                    .then(lit(format!("Linear {}d-{}d", a.tenor_days, b.tenor_days)))
                    .otherwise(method);
            }
            let cond = col("Tenor_Days").lt_eq(lit(first.tenor_days));
            rate = when(cond.clone()).then(lit(first.ftp_rate)).otherwise(rate);
            method = when(cond)
                .then(lit(format!("First point {}d (at or below curve start)", first.tenor_days)))
                .otherwise(method);
        }
    }
    Ok((rate, method))
}

/// `FTP_Method` of a row whose open or maturity date is missing (no FTP rate is given).
pub const FTP_MISSING_DATE: &str = "MISSING_DATE";

pub fn apply_ftp_curve_map(
    lf: LazyFrame,
    opndat_col: &str,
    matdat_col: &str,
    bal_col: &str,
    rate_col: &str,
    curve: &[FtpPoint],
    is_liability: bool,
    interpolation: FtpInterpolation,
) -> Result<LazyFrame> {
    let lf = ensure_date(lf, opndat_col)?;
    let lf = ensure_date(lf, matdat_col)?;
    let opn_days = col(opndat_col).cast(DataType::Int32).cast(DataType::Int64);
    let mat_days = col(matdat_col).cast(DataType::Int32).cast(DataType::Int64);
    let raw_diff = mat_days - opn_days;

    // A missing open or maturity date has no tenor: Tenor_Days, the FTP rate and the spread are
    // null and FTP_Method says MISSING_DATE (it used to become a 1-day tenor without a word).
    let missing_date = col(matdat_col).is_null().or(col(opndat_col).is_null());
    let tenor_expr = when(missing_date.clone())
        .then(lit(NULL).cast(DataType::Int64))
        .when(raw_diff.clone().lt(lit(1i64)))
        .then(lit(1i64))
        .otherwise(raw_diff)
        .alias("Tenor_Days");

    let (ftp_rate_expr, method_expr) = ftp_rate_exprs(curve, interpolation)?;
    let ftp_rate_expr = when(missing_date.clone())
        .then(lit(NULL).cast(DataType::Float64))
        .otherwise(ftp_rate_expr)
        .alias("Benchmark_FTP_Rate");
    let method_expr = when(missing_date)
        .then(lit(FTP_MISSING_DATE))
        .otherwise(method_expr)
        .alias("FTP_Method");

    let acct_rate = col(rate_col).cast(DataType::Float64).fill_null(lit(0.0));
    let spread_expr = if is_liability {
        (col("Benchmark_FTP_Rate") - acct_rate).alias("FTP_Spread_Pct")
    } else {
        (acct_rate - col("Benchmark_FTP_Rate")).alias("FTP_Spread_Pct")
    };

    let margin_expr = (col(bal_col).cast(DataType::Float64).fill_null(lit(0.0))
        * (col("FTP_Spread_Pct") / lit(100.0)))
    .alias("Annual_FTP_Margin_BDT");

    Ok(lf
        .with_column(tenor_expr)
        .with_columns([ftp_rate_expr, method_expr])
        .with_column(spread_expr)
        .with_column(margin_expr))
}

/// Feature B3: One-Click Core Banking Dirty Data & Ghost Account Auditor
pub fn apply_banking_hygiene_audit(
    lf: LazyFrame,
    opndat_col: &str,
    matdat_col: &str,
    clsdat_col: &str,
    acstat_col: &str,
    bal_col: &str,
    rate_col: &str,
    cutoff_date: &str,
    only_exceptions: bool,
    active_statuses: &[String],
    rate_min_pct: f64,
    rate_max_pct: f64,
) -> Result<LazyFrame> {
    let cutoff = NaiveDate::parse_from_str(cutoff_date.trim(), "%Y-%m-%d")
        .context("Cutoff Date must be YYYY-MM-DD")?;
    let active: Vec<String> = active_statuses
        .iter()
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty())
        .collect();
    if active.is_empty() {
        bail!("Hygiene audit: enter at least one active status code (for example ACT, ACTIVE, A, OPEN).");
    }
    if !(rate_min_pct.is_finite() && rate_max_pct.is_finite()) || rate_min_pct > rate_max_pct {
        bail!(
            "Hygiene audit: the rate outlier bounds {}% to {}% are not a valid range (the lower bound must not exceed the upper).",
            rate_min_pct,
            rate_max_pct
        );
    }
    let lf = ensure_date(lf, opndat_col)?;
    let lf = ensure_date(lf, matdat_col)?;
    let lf = ensure_date(lf, clsdat_col)?;
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let cutoff_days = (cutoff - epoch).num_days();

    let opn = col(opndat_col).cast(DataType::Int32).cast(DataType::Int64);
    let mat = col(matdat_col).cast(DataType::Int32).cast(DataType::Int64);
    let cls = col(clsdat_col).cast(DataType::Int32).cast(DataType::Int64);
    let bal = col(bal_col).cast(DataType::Float64).fill_null(lit(0.0));
    let rate = col(rate_col).cast(DataType::Float64);

    let status = col(acstat_col)
        .cast(DataType::String)
        .str()
        .strip_chars(lit(NULL))
        .str()
        .to_uppercase();
    let is_active = active
        .iter()
        .fold(lit(false), |acc, a| acc.or(status.clone().eq(lit(a.clone()))))
        .fill_null(lit(false));
    let is_ghost = is_active
        .and(bal.clone().gt(lit(0.0)))
        .and(col(matdat_col).is_not_null())
        .and(mat.clone().lt(lit(cutoff_days)));

    let is_bad_date = (col(matdat_col)
        .is_not_null()
        .and(col(opndat_col).is_not_null())
        .and(mat.lt(opn.clone())))
    .or(col(clsdat_col)
        .is_not_null()
        .and(col(opndat_col).is_not_null())
        .and(cls.lt(opn)));

    let is_rate_outlier = col(rate_col)
        .is_not_null()
        .and(rate.clone().lt(lit(rate_min_pct)).or(rate.gt(lit(rate_max_pct))));

    let is_neg_bal = bal.lt(lit(0.0));

    let flags = [
        (is_ghost.fill_null(lit(false)), "1. GHOST_MATURED_ACTIVE"),
        (is_bad_date.fill_null(lit(false)), "2. BACKWARDS_DATE_ANOMALY"),
        (is_rate_outlier.fill_null(lit(false)), "3. INTEREST_RATE_OUTLIER"),
        (is_neg_bal.fill_null(lit(false)), "4. NEGATIVE_BALANCE"),
    ];
    // First flag that applies (unchanged column) ...
    let mut audit_flag_expr: Expr = lit("OK");
    for (cond, label) in flags.iter().rev() {
        audit_flag_expr = when(cond.clone()).then(lit(*label)).otherwise(audit_flag_expr);
    }
    // ... and every flag that applies.
    let parts: Vec<Expr> = flags
        .iter()
        .map(|(cond, label)| when(cond.clone()).then(lit(*label)).otherwise(lit(NULL).cast(DataType::String)))
        .collect();
    let joined = concat_str(parts, "; ", true);
    let all_flags_expr = when(joined.clone().is_null().or(joined.clone().eq(lit(""))))
        .then(lit("OK"))
        .otherwise(joined)
        .alias("Audit_All_Flags");

    let mut out_lf = lf.with_columns([audit_flag_expr.alias("Audit_Exception_Flag"), all_flags_expr]);
    if only_exceptions {
        out_lf = out_lf.filter(col("Audit_Exception_Flag").neq(lit("OK")));
    }
    Ok(out_lf)
}

/// Feature A6 & C2: Two-Period Variance & Optional Loan CL Migration Reconciler
pub fn build_period_reconcile_lazy(
    prev_lf: LazyFrame,
    curr_lf: LazyFrame,
    key_col: &str,
    bal_col: &str,
    rate_col: Option<&str>,
    cl_status_col: Option<&str>,
) -> Result<LazyFrame> {
    // The CL map is only needed (and only loaded) when a CL column is reconciled.
    let cl_map = match cl_status_col {
        Some(_) => ProvisionConfig::load_or_create(&portable_root_dir())?.cl_code_map,
        None => Vec::new(),
    };
    build_period_reconcile_lazy_with(prev_lf, curr_lf, key_col, bal_col, rate_col, cl_status_col, &cl_map)
}

/// Weighted average of `v` by `w` for use inside an aggregation: only rows where both are known
/// count. Returns `(rate, mixed_sign)`. When those weights are a mix of positive and negative
/// (e.g. +1000 and -999) the ratio is not a meaningful rate, so `rate` is null and `mixed_sign`
/// is true. Groups whose weights all have one sign (all positive, or all negative) compute
/// normally; zero weights have no sign. `rate` is also null when the weights sum to 0.
pub fn weighted_rate_exprs(v: Expr, w: Expr) -> (Expr, Expr) {
    let mask = v.clone().is_not_null().and(w.clone().is_not_null());
    let num = (v * w.clone()).filter(mask.clone()).sum();
    let den = w.clone().filter(mask.clone()).sum();
    let count_where = |cond: Expr| cond.and(mask.clone()).cast(DataType::Int64).sum();
    let mixed = count_where(w.clone().gt(lit(0.0)))
        .gt(lit(0i64))
        .and(count_where(w.lt(lit(0.0))).gt(lit(0i64)));
    let rate = when(mixed.clone())
        .then(lit(NULL).cast(DataType::Float64))
        .when(den.clone().eq(lit(0.0)))
        .then(lit(NULL).cast(DataType::Float64))
        .otherwise(num / den);
    (rate, mixed)
}

/// Per key and period: balance sum, balance-weighted rate (rows where rate and balance are both
/// known; null when their balances sum to 0 or are a mix of positive and negative, which is
/// flagged in `Prev_Rate_Mixed_Sign` / `Curr_Rate_Mixed_Sign`), row count, and the CL code of the worst-graded row
/// (grade order STD < SMA < SS < DF < BL via `cl_map`; ties and unmapped codes go to the row with
/// the largest balance).
pub fn build_period_reconcile_lazy_with(
    prev_lf: LazyFrame,
    curr_lf: LazyFrame,
    key_col: &str,
    bal_col: &str,
    rate_col: Option<&str>,
    cl_status_col: Option<&str>,
    cl_map: &[(String, String)],
) -> Result<LazyFrame> {
    let per_key = |lf: LazyFrame, bal_name: &str, rate_name: &str, cl_name: &str, count_name: &str| {
        let bal = col(bal_col).cast(DataType::Float64);
        let mut select = vec![col(key_col), bal.clone().alias(bal_name)];
        if let Some(rc) = rate_col {
            select.push(col(rc).cast(DataType::Float64).alias(rate_name));
        }
        if let Some(clc) = cl_status_col {
            let code = col(clc).cast(DataType::String);
            let grade = map_codes_expr(normalise_code_expr(code.clone()), cl_map);
            let mut rank: Expr = lit(NULL).cast(DataType::Int32);
            for (i, g) in CL_GRADES.iter().enumerate().rev() {
                rank = when(grade.clone().eq(lit(*g))).then(lit(i as i32)).otherwise(rank);
            }
            select.push(code.alias(cl_name));
            select.push(rank.alias("__mp_cl_rank"));
        }
        let mut agg = vec![col(bal_name).sum(), len().cast(DataType::Int64).alias(count_name)];
        if rate_col.is_some() {
            let (rate, mixed) = weighted_rate_exprs(col(rate_name), col(bal_name));
            agg.push(rate.alias(rate_name));
            agg.push(mixed.alias(format!("{}_Mixed_Sign", rate_name)));
        }
        if cl_status_col.is_some() {
            agg.push(
                col(cl_name)
                    .sort_by(
                        [col("__mp_cl_rank"), col(bal_name)],
                        SortMultipleOptions::default()
                            .with_order_descending_multi([true, true])
                            .with_nulls_last(true)
                            .with_maintain_order(true),
                    )
                    .first()
                    .alias(cl_name),
            );
        }
        lf.select(select).group_by([col(key_col)]).agg(agg)
    };

    let left = per_key(prev_lf, "Prev_Balance", "Prev_Rate", "Prev_CL", "Prev_Row_Count");
    let right = per_key(curr_lf, "Curr_Balance", "Curr_Rate", "Curr_CL", "Curr_Row_Count");

    let joined = left.join(
        right,
        [col(key_col)],
        [col(key_col)],
        JoinArgs::new(JoinType::Full).with_coalesce(JoinCoalesce::CoalesceColumns),
    );

    let prev_bal_name = "Prev_Balance";
    let curr_bal_name = "Curr_Balance";
    let pb = col(prev_bal_name).fill_null(lit(0.0));
    let cb = col(curr_bal_name).fill_null(lit(0.0));
    let delta_expr = (cb.clone() - pb.clone()).alias("Balance_Delta");

    // A key missing in a period has no rows there: its row count is 0 (its balance stays null).
    let status_expr = when(col("Prev_Row_Count").is_null())
        .then(lit("1. New Account Opened"))
        .when(col("Curr_Row_Count").is_null())
        .then(lit("2. Closed / Matured Account"))
        .when((cb.clone() - pb.clone()).gt(lit(0.01)))
        .then(lit("3. Balance Increased"))
        .when((cb - pb).lt(lit(-0.01)))
        .then(lit("4. Balance Decreased"))
        .otherwise(lit("5. Unchanged"))
        .alias("Reconcile_Status");

    let mut final_lf = joined
        .with_columns([status_expr, delta_expr])
        .with_columns([
            col("Prev_Row_Count").fill_null(lit(0i64)),
            col("Curr_Row_Count").fill_null(lit(0i64)),
        ]);

    if rate_col.is_some() {
        // Null when either period has no rate (key missing there, or no known rate).
        let rate_delta_bps = ((col("Curr_Rate") - col("Prev_Rate")) * lit(100.0))
            .round(2)
            .alias("Rate_Delta_Bps");
        final_lf = final_lf.with_column(rate_delta_bps).with_columns([
            col("Prev_Rate_Mixed_Sign").fill_null(lit(false)),
            col("Curr_Rate_Mixed_Sign").fill_null(lit(false)),
        ]);
    }

    if cl_status_col.is_some() {
        let p_cl = col("Prev_CL").fill_null(lit("NEW"));
        let c_cl = col("Curr_CL").fill_null(lit("CLOSED"));
        let cl_mig = concat_str([p_cl, c_cl], " -> ", false).alias("CL_Migration_Matrix");
        final_lf = final_lf.with_column(cl_mig);
    }

    Ok(final_lf)
}

/// Keys with more than one row in the previous and in the current period, from a reconcile
/// result (for the status message).
pub fn reconcile_multi_row_keys(reconciled: LazyFrame) -> Result<(usize, usize)> {
    let df = reconciled
        .select([
            col("Prev_Row_Count").gt(lit(1i64)).sum().cast(DataType::Int64).alias("p"),
            col("Curr_Row_Count").gt(lit(1i64)).sum().cast(DataType::Int64).alias("c"),
        ])
        .collect()
        .context("Could not count keys with more than one row")?;
    let get = |name: &str| -> usize {
        df.column(name)
            .ok()
            .and_then(|c| c.i64().ok())
            .and_then(|ca| ca.get(0))
            .unwrap_or(0) as usize
    };
    Ok((get("p"), get("c")))
}

/// Keys whose previous / current rate is null because their balances mix positive and negative
/// (from a reconcile result with a rate column; for the status message).
pub fn reconcile_mixed_sign_keys(reconciled: LazyFrame) -> Result<(usize, usize)> {
    let df = reconciled
        .select([
            col("Prev_Rate_Mixed_Sign").cast(DataType::Int64).sum().alias("p"),
            col("Curr_Rate_Mixed_Sign").cast(DataType::Int64).sum().alias("c"),
        ])
        .collect()
        .context("Could not count keys with mixed-sign balances")?;
    let get = |name: &str| -> usize {
        df.column(name)
            .ok()
            .and_then(|c| c.i64().ok())
            .and_then(|ca| ca.get(0))
            .unwrap_or(0) as usize
    };
    Ok((get("p"), get("c")))
}

/// The sentence(s) the status bar shows after a reconcile: keys with more than one row per
/// period, and (with a rate column) keys whose rate is null because balances mix signs. Empty
/// when there is nothing to say about the mixed signs; the multi-row sentence is always there.
pub fn reconcile_status_note(reconciled: LazyFrame, has_rate: bool) -> Result<String> {
    let mut note = String::new();
    if has_rate {
        let (p, c) = reconcile_mixed_sign_keys(reconciled.clone())?;
        if p + c > 0 {
            note.push_str(&format!(
                " ⚠ Rate is null for {} key(s) in the previous period and {} in the current: their balances mix positive and negative (see Prev_Rate_Mixed_Sign / Curr_Rate_Mixed_Sign).",
                p, c
            ));
        }
    }
    let (p, c) = reconcile_multi_row_keys(reconciled)?;
    note.push_str(&format!(
        " Keys with more than one row: {} in the previous period, {} in the current (rows are combined per key; see Prev_Row_Count / Curr_Row_Count).",
        p, c
    ));
    Ok(note)
}

/// IRRBB Suite: Macaulay Duration, Modified Duration, DV01 (1 bp BPV), and ΔEVE Impact
pub fn apply_duration_analysis(
    lf: LazyFrame,
    bal_col: &str,
    rate_col: &str,
    mat_col: &str,
    cutoff_date: &str,
    compounding_freq: usize,
    shock_bps: f64,
    default_sight_years: f64,
    side_col: Option<&str>,
) -> Result<LazyFrame> {
    let cutoff = NaiveDate::parse_from_str(cutoff_date.trim(), "%Y-%m-%d")
        .context("Cutoff Date must be YYYY-MM-DD")?;
    let lf = ensure_date(lf, mat_col)?;
    let (lf, sign) = direction_sign(lf, side_col)?;
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let cutoff_days = (cutoff - epoch).num_days();

    let bal = col(bal_col).cast(DataType::Float64).fill_null(lit(0.0));
    let raw_rate_dec = col(rate_col).cast(DataType::Float64).fill_null(lit(0.0)) / lit(100.0);
    let rate_dec = when(raw_rate_dec.clone().lt(lit(0.0001)))
        .then(lit(0.0001))
        .otherwise(raw_rate_dec);

    let k = (compounding_freq.max(1)) as f64;
    let shock_dec = shock_bps / 10_000.0;

    let rem_days = col(mat_col).cast(DataType::Int32).cast(DataType::Int64) - lit(cutoff_days);

    let tenor_years = when(col(mat_col).is_null())
        .then(lit(default_sight_years))
        .when(rem_days.clone().lt_eq(lit(1)))
        .then(lit(1.0 / 365.0))
        .otherwise(rem_days.cast(DataType::Float64) / lit(365.0))
        .alias("Residual_Years_T");

    let y_per_k = rate_dec.clone() / lit(k);
    let total_periods = col("Residual_Years_T") * lit(k);

    let pv_factor =
        (lit(0.0) - total_periods * (lit(1.0) + y_per_k.clone()).log(std::f64::consts::E)).exp();
    let annuity_mac = ((lit(1.0) + y_per_k.clone()) / rate_dec) * (lit(1.0) - pv_factor);

    let mac_dur = when(col("Residual_Years_T").lt_eq(lit(0.5)))
        .then(col("Residual_Years_T"))
        .when(annuity_mac.clone().gt(col("Residual_Years_T")))
        .then(col("Residual_Years_T"))
        .otherwise(annuity_mac)
        .round(4)
        .alias("Macaulay_Duration_Yrs");

    let mod_dur = (col("Macaulay_Duration_Yrs") / (lit(1.0) + y_per_k))
        .round(4)
        .alias("Modified_Duration");

    let dv01 = (bal.clone() * col("Modified_Duration") * lit(0.0001))
        .round(2)
        .alias("DV01_BPV_BDT");

    // A rate rise lowers the value of assets and lowers the value owed on liabilities (EVE up).
    // DV01 above stays unsigned.
    let delta_eve = ((lit(0.0) - (bal * col("Modified_Duration") * lit(shock_dec))) * sign)
        .round(2)
        .alias("Delta_EVE_Impact_BDT");

    Ok(lf
        .with_column(tenor_years)
        .with_column(mac_dur)
        .with_column(mod_dur)
        .with_columns([dv01, delta_eve]))
}

// ---------------------------------------------------------------------------------------------
// Loan classification provisioning rules (BRPD Circular 15/2024 and later), held as data files
// in portable_data/ so rates can be changed without a new build:
//   cl_code_map.json     {"defaults_version": 2, "<CBS CL code>": "<STD|SMA|SS|DF|BL>", ...}
//   segment_map.json     {"defaults_version": 2, "<segment code>": "<SHORT_TERM_AGRI|CMSME|OTHER>", ...}
//   provision_rules.json {"defaults_version": 2,
//                         "rules": [{grade, category, rate_pct, effective_from, effective_to, source, verify}]}
// Files from before versioning (flat map / bare array, no defaults_version) still load.
// Codes are matched after normalising (uppercase, trimmed, inner spaces collapsed).
// ---------------------------------------------------------------------------------------------

pub const CL_CODE_MAP_FILE: &str = "cl_code_map.json";
pub const SEGMENT_MAP_FILE: &str = "segment_map.json";
pub const PROVISION_RULES_FILE: &str = "provision_rules.json";

/// Version of the built-in default rule files. Bump it whenever a default changes, and keep
/// the older defaults reachable through `default_*(version)` so untouched files can be upgraded.
pub const RULE_FILES_VERSION: u32 = 3;

const CL_GRADES: [&str; 5] = ["STD", "SMA", "SS", "DF", "BL"];
const REG_CATEGORIES: [&str; 3] = ["SHORT_TERM_AGRI", "CMSME", "OTHER"];
/// Collateral types whose eligible security is deducted without the 15%-of-outstanding floor.
const FLOOR_EXEMPT_COLLATERAL: [&str; 5] = [
    "DEPOSIT_LIEN",
    "GOVT_SECURITY",
    "GOVT_GUARANTEE",
    "BB_GUARANTEE",
    "AAA_MDB_GUARANTEE",
];

const DEFAULT_CL_CODE_MAP: &[(&str, &str)] = &[
    ("STD", "STD"),
    ("STD-0", "STD"),
    ("STD-1", "STD"),
    ("STD-2", "STD"),
    ("UC", "STD"),
    ("STANDARD", "STD"),
    ("UNCLASSIFIED", "STD"),
    ("SMA", "SMA"),
    ("SS", "SS"),
    ("SUB-STANDARD", "SS"),
    ("SUBSTANDARD", "SS"),
    ("SUB STANDARD", "SS"),
    ("DF", "DF"),
    ("DOUBTFUL", "DF"),
    ("BL", "BL"),
    ("B/L", "BL"),
    ("BAD/LOSS", "BL"),
    ("BAD", "BL"),
    ("LOSS", "BL"),
];

/// Version 1 (first T7 build) segment defaults, kept only to recognise untouched old files.
const V1_SEGMENT_MAP: &[(&str, &str)] = &[
    ("CMSME", "CMSME"),
    ("SME", "CMSME"),
    ("CMS", "CMSME"),
    ("COTTAGE", "CMSME"),
    ("MICRO", "CMSME"),
    ("SMALL", "CMSME"),
    ("MEDIUM", "CMSME"),
    ("AGRI ST", "SHORT_TERM_AGRI"),
    ("SHORT TERM AGRI", "SHORT_TERM_AGRI"),
];

/// CMSME concession covers cottage, micro and small loans only, not medium (owner decision).
const DEFAULT_SEGMENT_MAP: &[(&str, &str)] = &[
    ("CMS", "CMSME"),
    ("COTTAGE", "CMSME"),
    ("MICRO", "CMSME"),
    ("SMALL", "CMSME"),
    ("AGRI ST", "SHORT_TERM_AGRI"),
    ("SHORT TERM AGRI", "SHORT_TERM_AGRI"),
    ("CORPORATE", "OTHER"),
    ("NON-CMSME", "OTHER"),
    ("MEDIUM", "OTHER"),
];

/// One dated provisioning rate. `effective_to` = None means open-ended. Both ends inclusive.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProvisionRule {
    pub grade: String,
    /// SHORT_TERM_AGRI, CMSME, OTHER, or ANY (fallback when no category-specific row applies)
    pub category: String,
    pub rate_pct: f64,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub source: String,
    /// true = rate or dates still to be checked against the circular
    #[serde(default)]
    pub verify: bool,
}

/// The three provisioning rule files, loaded and validated.
#[derive(Clone, Debug)]
pub struct ProvisionConfig {
    /// normalised CBS code -> grade
    pub cl_code_map: Vec<(String, String)>,
    /// normalised segment code -> category
    pub segment_map: Vec<(String, String)>,
    pub rules: Vec<ProvisionRule>,
    /// Plain-language notices for the status bar (e.g. an edited rule file from an older version)
    pub warnings: Vec<String>,
}

/// Uppercase, trim and collapse inner whitespace (the code-matching rule for all three files).
pub fn normalise_code(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_uppercase()
}

/// Same normalisation as `normalise_code`, as a Polars expression.
fn normalise_code_expr(e: Expr) -> Expr {
    e.cast(DataType::String)
        .str()
        .strip_chars(lit(NULL))
        .str()
        .replace_all(lit(r"\s+"), lit(" "), false)
        .str()
        .to_uppercase()
}

/// A JSON object read as ordered (key, value) pairs, so a repeated key is caught instead of
/// silently overwriting the earlier one. An optional numeric "defaults_version" key is split out.
struct JsonPairs {
    version: Option<u32>,
    pairs: Vec<(String, String)>,
}

impl<'de> Deserialize<'de> for JsonPairs {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct PairVisitor;
        impl<'de> serde::de::Visitor<'de> for PairVisitor {
            type Value = JsonPairs;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an object of \"code\": \"value\" pairs")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<JsonPairs, A::Error> {
                let mut pairs = Vec::new();
                let mut version = None;
                while let Some(k) = map.next_key::<String>()? {
                    if k == "defaults_version" {
                        if version.is_some() {
                            return Err(serde::de::Error::custom("defaults_version is listed twice"));
                        }
                        version = Some(map.next_value::<u32>()?);
                    } else {
                        let v: String = map.next_value()?;
                        pairs.push((k, v));
                    }
                }
                Ok(JsonPairs { version, pairs })
            }
        }
        d.deserialize_map(PairVisitor)
    }
}

fn parse_rule_date(s: &str, file: &Path, idx: usize, field: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map_err(|_| {
        anyhow!(
            "{}: rule {} has {} '{}', which is not a YYYY-MM-DD date.",
            file.display(),
            idx + 1,
            field,
            s
        )
    })
}

/// Normalises keys and checks values against `allowed`; a code listed twice is an error.
fn validate_code_map(
    pairs: Vec<(String, String)>,
    allowed: &[&str],
    file: &Path,
) -> Result<Vec<(String, String)>> {
    let mut out: Vec<(String, String)> = Vec::with_capacity(pairs.len());
    for (raw_key, raw_val) in pairs {
        let key = normalise_code(&raw_key);
        let val = normalise_code(&raw_val);
        if key.is_empty() {
            bail!("{}: an entry has a blank code.", file.display());
        }
        if !allowed.contains(&val.as_str()) {
            bail!(
                "{}: code '{}' maps to '{}', which is not one of {}.",
                file.display(),
                raw_key,
                raw_val,
                allowed.join(", ")
            );
        }
        if let Some((_, prev)) = out.iter().find(|(k, _)| *k == key) {
            bail!(
                "{}: code '{}' is listed more than once (as {} and {}). Keep one entry.",
                file.display(),
                key,
                prev,
                val
            );
        }
        out.push((key, val));
    }
    Ok(out)
}

fn validate_rules(rules: &[ProvisionRule], file: &Path) -> Result<()> {
    for (i, r) in rules.iter().enumerate() {
        if !CL_GRADES.contains(&r.grade.as_str()) {
            bail!(
                "{}: rule {} has grade '{}'; use one of {}.",
                file.display(),
                i + 1,
                r.grade,
                CL_GRADES.join(", ")
            );
        }
        if r.category != "ANY" && !REG_CATEGORIES.contains(&r.category.as_str()) {
            bail!(
                "{}: rule {} has category '{}'; use ANY or one of {}.",
                file.display(),
                i + 1,
                r.category,
                REG_CATEGORIES.join(", ")
            );
        }
        if !r.rate_pct.is_finite() || r.rate_pct < 0.0 || r.rate_pct > 100.0 {
            bail!(
                "{}: rule {} has rate_pct {}; it must be between 0 and 100 (percent).",
                file.display(),
                i + 1,
                r.rate_pct
            );
        }
        let from = parse_rule_date(&r.effective_from, file, i, "effective_from")?;
        if let Some(to) = &r.effective_to {
            let to = parse_rule_date(to, file, i, "effective_to")?;
            if to < from {
                bail!(
                    "{}: rule {} ends ({}) before it starts ({}).",
                    file.display(),
                    i + 1,
                    to,
                    from
                );
            }
        }
    }
    Ok(())
}

fn read_json_file<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Could not read {}", path.display()))?;
    parse_json_text(&raw, path)
}

fn parse_json_text<T: serde::de::DeserializeOwned>(raw: &str, path: &Path) -> Result<T> {
    // serde_json errors end with "at line L column C"
    serde_json::from_str(raw).map_err(|e| anyhow!("{} is not valid: {}", path.display(), e))
}

/// provision_rules.json as written from version 2 on.
#[derive(Serialize, Deserialize)]
struct VersionedRules {
    #[serde(default)]
    defaults_version: Option<u32>,
    rules: Vec<ProvisionRule>,
}

/// Reads provision_rules.json: the versioned object, or a bare array (before versioning).
fn read_rules_file(path: &Path) -> Result<(Option<u32>, Vec<ProvisionRule>)> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Could not read {}", path.display()))?;
    let shape: serde_json::Value = parse_json_text(&raw, path)?;
    if shape.is_array() {
        Ok((None, parse_json_text(&raw, path)?))
    } else {
        let v: VersionedRules = parse_json_text(&raw, path)?;
        Ok((v.defaults_version, v.rules))
    }
}

/// Atomic write (temp file + rename), replacing any existing file.
fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let tmp = path.with_extension(format!("json.tmp{}", std::process::id()));
    std::fs::write(&tmp, contents)
        .with_context(|| format!("Could not write {}", path.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("Could not write {}", path.display()))
}

fn same_pairs(a: &[(String, String)], b: &[(&str, &str)]) -> bool {
    let mut a: Vec<(&str, &str)> = a.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut b = b.to_vec();
    a.sort_unstable();
    b.sort_unstable();
    a == b
}

fn same_rules(a: &[ProvisionRule], b: &[ProvisionRule]) -> bool {
    a.len() == b.len() && a.iter().all(|r| b.contains(r)) && b.iter().all(|r| a.contains(r))
}

fn rules_to_json(version: u32, rules: &[ProvisionRule]) -> Result<String> {
    let v = VersionedRules { defaults_version: Some(version), rules: rules.to_vec() };
    Ok(serde_json::to_string_pretty(&v).context("Could not prepare default provision rules")? + "\n")
}

fn outdated_warning(path: &Path, version: Option<u32>) -> String {
    format!(
        "{} was saved by an older MicroParquet ({}) and has been edited, so it was kept as is. Compare it with the current defaults (defaults_version {}), then set \"defaults_version\": {} in the file.",
        path.display(),
        version.map_or("no defaults_version".to_string(), |v| format!("defaults_version {}", v)),
        RULE_FILES_VERSION,
        RULE_FILES_VERSION
    )
}

/// Writes `contents` to `path` unless it exists; via a temp file + rename so a reader never
/// sees a half-written file.
fn write_if_missing(path: &Path, contents: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let tmp = path.with_extension(format!("json.tmp{}", std::process::id()));
    std::fs::write(&tmp, contents)
        .with_context(|| format!("Could not write default {}", path.display()))?;
    if path.exists() {
        let _ = std::fs::remove_file(&tmp);
        return Ok(());
    }
    std::fs::rename(&tmp, path)
        .with_context(|| format!("Could not write default {}", path.display()))
}

fn pairs_to_json(version: u32, pairs: &[(&str, &str)]) -> String {
    let mut lines = vec![format!("  \"defaults_version\": {}", version)];
    lines.extend(
        pairs
            .iter()
            .map(|(k, v)| format!("  {}: {}", serde_json::json!(k), serde_json::json!(v))),
    );
    format!("{{\n{}\n}}\n", lines.join(",\n"))
}

/// Built-in CBS code map of a given defaults version (1 = first T7 build; unchanged since 2).
pub fn default_cl_code_map(version: u32) -> Vec<(&'static str, &'static str)> {
    let mut v = DEFAULT_CL_CODE_MAP.to_vec();
    if version == 1 {
        let at = v.iter().position(|(k, _)| *k == "B/L").map_or(v.len(), |i| i + 1);
        v.insert(at, ("BLW", "BL"));
    }
    v
}

/// Built-in segment map of a given defaults version (1 = first T7 build; unchanged since 2).
pub fn default_segment_map(version: u32) -> Vec<(&'static str, &'static str)> {
    if version == 1 {
        V1_SEGMENT_MAP.to_vec()
    } else {
        DEFAULT_SEGMENT_MAP.to_vec()
    }
}

impl ProvisionConfig {
    /// Seed rules from the owner's review notes. Rows with `verify: true` must be checked
    /// against the circular PDFs (none are flagged since version 3).
    pub fn default_rules() -> Vec<ProvisionRule> {
        Self::default_rules_for(RULE_FILES_VERSION)
    }

    /// Built-in rules of a given defaults version (1 = first T7 build, 2 = owner answers,
    /// 3 = current: Oct and Dec 2025 rows checked by the owner, Dec source named).
    pub fn default_rules_for(version: u32) -> Vec<ProvisionRule> {
        let rule = |grade: &str, category: &str, rate_pct: f64, from: &str, to: Option<&str>, source: &str, verify: bool| ProvisionRule {
            grade: grade.to_string(),
            category: category.to_string(),
            rate_pct,
            effective_from: from.to_string(),
            effective_to: to.map(str::to_string),
            source: source.to_string(),
            verify,
        };
        let c15 = "BRPD Circular 15, 27 Nov 2024";
        let (oct25, oct25_from) = if version == 1 {
            (
                "BRPD circular letter Oct 2025 (believed No. 22 of 2025); start date is a placeholder",
                "2025-10-01",
            )
        } else {
            ("BRPD Circular Letter No. 22, 13 Oct 2025", "2025-10-13")
        };
        let dec25 = if version <= 2 {
            "BB directive Dec 2025 replacing Circular Letter 22/2025"
        } else {
            "BRPD Circular Letter No. 29, 21 Dec 2025"
        };
        // The owner checked the Oct and Dec 2025 rows against the circulars for version 3.
        let check = version <= 2;
        let mut rules = vec![
            rule("STD", "ANY", 1.0, "2025-04-01", None, c15, false),
            rule("SMA", "ANY", 5.0, "2025-04-01", None, c15, false),
            rule("SS", "ANY", 20.0, "2025-04-01", None, c15, false),
            rule("DF", "ANY", 50.0, "2025-04-01", None, c15, false),
            rule("BL", "ANY", 100.0, "2025-04-01", None, c15, false),
        ];
        for cat in ["SHORT_TERM_AGRI", "CMSME"] {
            for grade in ["STD", "SMA"] {
                rules.push(rule(grade, cat, 1.0, oct25_from, Some("2025-12-20"), oct25, check));
            }
        }
        for cat in ["SHORT_TERM_AGRI", "CMSME"] {
            for grade in ["STD", "SMA"] {
                rules.push(rule(grade, cat, 0.5, "2025-12-21", Some("2026-12-31"), dec25, check));
            }
        }
        rules
    }

    /// Built-in defaults (the same content as the files written on first run).
    pub fn defaults() -> Self {
        let own = |p: &[(&str, &str)]| -> Vec<(String, String)> {
            p.iter().map(|(k, v)| (normalise_code(k), v.to_string())).collect()
        };
        Self {
            cl_code_map: own(DEFAULT_CL_CODE_MAP),
            segment_map: own(DEFAULT_SEGMENT_MAP),
            rules: Self::default_rules(),
            warnings: Vec::new(),
        }
    }

    /// Creates any of the three files that does not exist yet, with the defaults.
    pub fn ensure_default_files(dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("Could not create {}", dir.display()))?;
        let v = RULE_FILES_VERSION;
        write_if_missing(&dir.join(CL_CODE_MAP_FILE), &pairs_to_json(v, &default_cl_code_map(v)))?;
        write_if_missing(&dir.join(SEGMENT_MAP_FILE), &pairs_to_json(v, &default_segment_map(v)))?;
        write_if_missing(&dir.join(PROVISION_RULES_FILE), &rules_to_json(v, &Self::default_rules())?)
    }

    /// Loads and validates the three files from `dir`, creating missing ones first.
    /// A file older than `RULE_FILES_VERSION` (or with no version) is replaced by the current
    /// default when it still equals an earlier built-in default; an edited old file is kept and
    /// a warning naming it is added to `warnings`.
    pub fn load_or_create(dir: &Path) -> Result<Self> {
        Self::ensure_default_files(dir)?;
        let cur = RULE_FILES_VERSION;
        let mut warnings = Vec::new();
        let old = |v: Option<u32>| v.map_or(true, |v| v < cur);

        let mut maps = Vec::new();
        for (name, defaults) in [
            (CL_CODE_MAP_FILE, default_cl_code_map as fn(u32) -> Vec<(&'static str, &'static str)>),
            (SEGMENT_MAP_FILE, default_segment_map),
        ] {
            let path = dir.join(name);
            let mut file: JsonPairs = read_json_file(&path)?;
            if old(file.version) {
                if (1..=cur).any(|v| same_pairs(&file.pairs, &defaults(v))) {
                    write_atomic(&path, &pairs_to_json(cur, &defaults(cur)))?;
                    file.pairs = defaults(cur)
                        .into_iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect();
                } else {
                    warnings.push(outdated_warning(&path, file.version));
                }
            }
            maps.push((path, file.pairs));
        }

        let rules_path = dir.join(PROVISION_RULES_FILE);
        let (version, mut rules) = read_rules_file(&rules_path)?;
        if old(version) {
            if (1..=cur).any(|v| same_rules(&rules, &Self::default_rules_for(v))) {
                rules = Self::default_rules();
                write_atomic(&rules_path, &rules_to_json(cur, &rules)?)?;
            } else {
                warnings.push(outdated_warning(&rules_path, version));
            }
        }
        validate_rules(&rules, &rules_path)?;

        let mut maps = maps.into_iter();
        let (cl_path, cl) = maps.next().ok_or_else(|| anyhow!("internal: CL map missing"))?;
        let (seg_path, seg) = maps.next().ok_or_else(|| anyhow!("internal: segment map missing"))?;
        Ok(Self {
            cl_code_map: validate_code_map(cl, &CL_GRADES, &cl_path)?,
            segment_map: validate_code_map(seg, &REG_CATEGORIES, &seg_path)?,
            rules,
            warnings,
        })
    }

    /// The single rule for (grade, category) in force on `cutoff`. A category-specific row wins
    /// over category ANY. Two rows in force at the same level, or none at all, is an error.
    pub fn lookup(&self, grade: &str, category: &str, cutoff: NaiveDate) -> Result<&ProvisionRule> {
        let in_force = |r: &&ProvisionRule| -> bool {
            let from = NaiveDate::parse_from_str(&r.effective_from, "%Y-%m-%d").ok();
            let to = match &r.effective_to {
                Some(t) => NaiveDate::parse_from_str(t, "%Y-%m-%d").ok(),
                None => Some(NaiveDate::MAX),
            };
            matches!((from, to), (Some(f), Some(t)) if f <= cutoff && cutoff <= t)
        };
        // No rules exist before the first one in the file (Circular 15/2024 regime start).
        let earliest = self
            .rules
            .iter()
            .filter_map(|r| NaiveDate::parse_from_str(&r.effective_from, "%Y-%m-%d").ok())
            .min();
        if let Some(first) = earliest {
            if cutoff < first {
                bail!(
                    "No provisioning rule covers cutoff {}; rules start {} (BRPD Circular 15/2024)",
                    cutoff,
                    first
                );
            }
        }
        for cat in [category, "ANY"] {
            let hits: Vec<&ProvisionRule> = self
                .rules
                .iter()
                .filter(|r| r.grade == grade && r.category == cat)
                .filter(in_force)
                .collect();
            match hits.as_slice() {
                [] => continue,
                [one] => return Ok(one),
                [a, b, ..] => bail!(
                    "{} has {} rules for grade {} / category {} in force on {}: '{}' ({} to {}) and '{}' ({} to {}). Fix the overlapping dates.",
                    PROVISION_RULES_FILE,
                    hits.len(),
                    grade,
                    cat,
                    cutoff,
                    a.source,
                    a.effective_from,
                    a.effective_to.as_deref().unwrap_or("open"),
                    b.source,
                    b.effective_from,
                    b.effective_to.as_deref().unwrap_or("open")
                ),
            }
        }
        bail!(
            "{} has no rule for grade {} / category {} (or ANY) in force on {}.",
            PROVISION_RULES_FILE,
            grade,
            category,
            cutoff
        )
    }
}

/// Maps `e` (already normalised) through `pairs`; null when the code is not listed.
fn map_codes_expr(e: Expr, pairs: &[(String, String)]) -> Expr {
    pairs
        .iter()
        .rev()
        .fold(lit(NULL).cast(DataType::String), |acc, (k, v)| {
            when(e.clone().eq(lit(k.clone())))
                .then(lit(v.clone()))
                .otherwise(acc)
        })
}

fn max_expr(a: Expr, b: Expr) -> Expr {
    when(a.clone().gt_eq(b.clone())).then(a).otherwise(b)
}

/// Loans & Advances Suite (C1): Bangladesh Bank CL provisioning and Base For Provision (BFP).
/// Rates, CBS code mapping and segment mapping come from the rule files in portable_data/.
#[allow(clippy::too_many_arguments)]
pub fn apply_loan_classification_provision(
    lf: LazyFrame,
    bal_col: &str,
    cl_status_col: &str,
    segment_col: Option<&str>,
    suspense_col: Option<&str>,
    security_col: Option<&str>,
    collateral_type_col: Option<&str>,
    cutoff_date: &str,
) -> Result<LazyFrame> {
    let config = ProvisionConfig::load_or_create(&portable_root_dir())?;
    apply_loan_classification_provision_with(
        lf,
        bal_col,
        cl_status_col,
        segment_col,
        suspense_col,
        security_col,
        collateral_type_col,
        cutoff_date,
        &config,
    )
}

/// As `apply_loan_classification_provision`, with the rule set passed in.
#[allow(clippy::too_many_arguments)]
pub fn apply_loan_classification_provision_with(
    lf: LazyFrame,
    bal_col: &str,
    cl_status_col: &str,
    segment_col: Option<&str>,
    suspense_col: Option<&str>,
    security_col: Option<&str>,
    collateral_type_col: Option<&str>,
    cutoff_date: &str,
    config: &ProvisionConfig,
) -> Result<LazyFrame> {
    let given = |c: Option<&str>| c.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let (segment_col, suspense_col, security_col, collateral_type_col) = (
        given(segment_col),
        given(suspense_col),
        given(security_col),
        given(collateral_type_col),
    );

    // Cutoff: blank = today, and the output says so.
    let (cutoff, mut lf) = if cutoff_date.trim().is_empty() {
        let today = chrono::Local::now().date_naive();
        let lf = lf.with_column(lit(today.format("%Y-%m-%d").to_string()).alias("Cutoff_Assumed"));
        (today, lf)
    } else {
        let d = NaiveDate::parse_from_str(cutoff_date.trim(), "%Y-%m-%d")
            .context("Provision cutoff date must be YYYY-MM-DD")?;
        (d, lf)
    };

    // 1. CL grade: every code must be in cl_code_map.json; unknown codes are an error, never 1%.
    let bad = unmapped_cl_codes(&lf, cl_status_col, &config.cl_code_map)?;
    if !bad.is_empty() {
        bail!("{}", cl_code_error(cl_status_col, &bad));
    }
    let grade_expr = map_codes_expr(normalise_code_expr(col(cl_status_col)), &config.cl_code_map);

    // 2. Regulatory category from segment_map.json (exact match); unlisted -> OTHER, flagged.
    let seg_mapped = match &segment_col {
        Some(s) => map_codes_expr(normalise_code_expr(col(s.as_str())), &config.segment_map),
        None => lit(NULL).cast(DataType::String),
    };
    lf = lf.with_columns([
        grade_expr.alias("CL_Grade"),
        seg_mapped.clone().fill_null(lit("OTHER")).alias("Reg_Category"),
        seg_mapped.is_null().alias("Segment_Unmapped"),
    ]);

    // 3. Rate and source for every grade x category in force on the cutoff (fails on gaps/overlaps).
    let mut rate_expr = lit(NULL).cast(DataType::Float64);
    let mut source_expr = lit(NULL).cast(DataType::String);
    for grade in CL_GRADES {
        for cat in REG_CATEGORIES {
            let rule = config.lookup(grade, cat, cutoff)?;
            let hit = col("CL_Grade").eq(lit(grade)).and(col("Reg_Category").eq(lit(cat)));
            let source = if rule.verify {
                format!("{} [verify]", rule.source)
            } else {
                rule.source.clone()
            };
            rate_expr = when(hit.clone()).then(lit(rule.rate_pct)).otherwise(rate_expr);
            source_expr = when(hit).then(lit(source)).otherwise(source_expr);
        }
    }

    // 4. Base for provision.
    let num = |c: &Option<String>| match c {
        Some(s) => col(s.as_str()).cast(DataType::Float64).fill_null(lit(0.0)),
        None => lit(0.0),
    };
    let bal = col(bal_col).cast(DataType::Float64).fill_null(lit(0.0));
    let susp = num(&suspense_col);
    // Phase 1: the security column is taken as already eligible (after haircuts); haircuts
    // on eligible security are not applied here.
    let sec = num(&security_col);
    let floor_exempt = match &collateral_type_col {
        Some(c) => {
            let t = normalise_code_expr(col(c.as_str())).str().replace_all(lit(r"[ \-]"), lit("_"), false);
            FLOOR_EXEMPT_COLLATERAL
                .iter()
                .fold(lit(false), |acc, k| acc.or(t.clone().eq(lit(*k))))
                .fill_null(lit(false))
        }
        None => lit(false),
    };
    let is_npl = col("CL_Grade").is_in(lit(Series::new("npl".into(), ["SS", "DF", "BL"])));
    let raw_net = max_expr(bal.clone() - susp - sec.clone(), lit(0.0));
    // 15%-of-outstanding floor only when security is deducted and the collateral is not exempt.
    let floor_applies = sec.gt(lit(0.0)).and(floor_exempt.not());
    let floored = max_expr(raw_net.clone(), bal.clone() * lit(0.15));
    let npl_bfp = when(floor_applies).then(floored).otherwise(raw_net);
    let bfp_expr = when(is_npl.clone())
        .then(npl_bfp)
        .otherwise(max_expr(bal, lit(0.0)))
        .round(2)
        .alias("Base_For_Provision_BDT");

    let npl_flag = when(is_npl)
        .then(lit("NPL (Classified)"))
        .otherwise(lit("Performing (UC/SMA)"))
        .alias("NPL_Category");

    let req_prov = (col("Base_For_Provision_BDT") * (col("Statutory_Prov_Rate_Pct") / lit(100.0)))
        .round(2)
        .alias("Required_Provision_BDT");

    Ok(lf
        .with_columns([
            npl_flag,
            bfp_expr,
            rate_expr.alias("Statutory_Prov_Rate_Pct"),
            source_expr.alias("Rule_Source"),
        ])
        .with_column(req_prov))
}

/// Loans & Advances Suite (C5): Limit Utilization, Over-Limit Audit & Undrawn LCR/CRAR Commitment
pub fn apply_loan_limit_utilization(
    lf: LazyFrame,
    limit_col: &str,
    bal_col: &str,
    ccf_pct: f64,
) -> Result<LazyFrame> {
    let limit = col(limit_col).cast(DataType::Float64).fill_null(lit(0.0));
    let bal = col(bal_col).cast(DataType::Float64).fill_null(lit(0.0));

    let util_pct = when(limit.clone().gt(lit(0.0)))
        .then((bal.clone() / limit.clone()) * lit(100.0))
        .otherwise(lit(0.0))
        .round(2)
        .alias("Limit_Utilization_Pct");

    let raw_undrawn = limit.clone() - bal.clone();
    let undrawn = when(raw_undrawn.clone().gt(lit(0.0)))
        .then(raw_undrawn)
        .otherwise(lit(0.0))
        .round(2)
        .alias("Undrawn_Commitment_BDT");

    let raw_over = bal - limit;
    let over_limit = when(raw_over.clone().gt(lit(0.0)))
        .then(raw_over)
        .otherwise(lit(0.0))
        .round(2)
        .alias("Over_Limit_Violation_BDT");

    let ccf_equiv = (col("Undrawn_Commitment_BDT") * lit(ccf_pct / 100.0))
        .round(2)
        .alias("CCF_Credit_Equivalent_BDT");

    let status_flag = when(col("Over_Limit_Violation_BDT").gt(lit(1.0)))
        .then(lit("1. OVER-LIMIT BREACH"))
        .when(col("Limit_Utilization_Pct").gt(lit(95.0)))
        .then(lit("2. NEAR FULL UTILIZATION (>95%)"))
        .when(col("Limit_Utilization_Pct").lt(lit(10.0)))
        .then(lit("3. DORMANT / LOW UTILIZATION (<10%)"))
        .otherwise(lit("4. NORMAL UTILIZATION"))
        .alias("Limit_Audit_Status");

    Ok(lf
        .with_columns([util_pct, undrawn, over_limit])
        .with_columns([ccf_equiv, status_flag]))
}

/// The CL codes of `column` that `cl_map` does not know (at most 20, as they are written in the
/// data; a blank is "(blank)").
pub fn unmapped_cl_codes(lf: &LazyFrame, column: &str, cl_map: &[(String, String)]) -> Result<Vec<String>> {
    let bad = lf
        .clone()
        .select([col(column).cast(DataType::String).alias("__cl")])
        .filter(map_codes_expr(normalise_code_expr(col("__cl")), cl_map).is_null())
        .unique_stable(None, UniqueKeepStrategy::First)
        .limit(20)
        .collect()
        .with_context(|| format!("Could not read the CL status column '{}'", column))?;
    Ok(bad
        .column("__cl")?
        .str()?
        .into_iter()
        .map(|v| match v {
            Some(s) if !s.trim().is_empty() => format!("'{}'", s),
            _ => "(blank)".to_string(),
        })
        .collect())
}

/// The message for codes that are not in `cl_code_map.json` (the Phase 1 wording).
pub fn cl_code_error(column: &str, codes: &[String]) -> String {
    format!(
        "CL status column '{}' has codes not in {}: {}. Add each code with its grade (STD, SMA, SS, DF or BL) to that file.",
        column,
        CL_CODE_MAP_FILE,
        codes.join(", ")
    )
}

// ---------------------------------------------------------------------------------------------
// CL migration matrix (Phase 4 V2)
// ---------------------------------------------------------------------------------------------

/// What the cells of the migration matrix hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MigrationMeasure {
    /// Balance in BDT: the previous month's balance of an account that was there, the current
    /// month's for a new one.
    #[default]
    Balance,
    /// Number of accounts.
    Count,
}

/// The row of accounts that exist only in the current month.
pub const MIGRATION_NEW_ROW: &str = "New";
/// The column of accounts that exist only in the previous month.
pub const MIGRATION_CLOSED_COL: &str = "Closed/Repaid";
/// The last row and column.
pub const MIGRATION_TOTAL: &str = "Total";
/// The row that adds up the five grades: the previous month's total.
pub const MIGRATION_PREV_TOTAL_ROW: &str = "Previous month total";
/// The first column of the matrix (after "Segment", when there is one).
pub const MIGRATION_ROW_HEADING: &str = "Previous grade";
/// The label of the block that adds up all segments.
pub const MIGRATION_ALL_SEGMENTS: &str = "All segments";

/// The grades in `cl_code_map.json` order.
pub fn cl_grades() -> [&'static str; 5] {
    CL_GRADES
}

/// How a cell of a result table is shaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shade {
    None,
    /// A limit is breached (the liquidity gap ladder).
    Breach,
    /// To a better grade.
    Upgrade,
    /// To a worse grade.
    Downgrade,
}

/// Upgrade (to a grade earlier in STD, SMA, SS, DF, BL), downgrade, or neither (the diagonal,
/// New, Closed/Repaid, totals).
pub fn migration_shade(from: &str, to: &str) -> Shade {
    let rank = |g: &str| CL_GRADES.iter().position(|x| *x == g);
    match (rank(from), rank(to)) {
        (Some(a), Some(b)) if b < a => Shade::Upgrade,
        (Some(a), Some(b)) if b > a => Shade::Downgrade,
        _ => Shade::None,
    }
}

/// The settings of a migration matrix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClMigrationSpec {
    pub key_col: String,
    pub bal_col: String,
    pub cl_col: String,
    pub segment_col: Option<String>,
    pub measure: MigrationMeasure,
    /// Each cell as a percentage of its row's total.
    pub pct_of_row: bool,
}

/// The migration matrix of two months, with the code map of `portable_data`.
pub fn build_cl_migration(prev: LazyFrame, curr: LazyFrame, spec: &ClMigrationSpec) -> Result<DataFrame> {
    let cl_map = ProvisionConfig::load_or_create(&portable_root_dir())?.cl_code_map;
    build_cl_migration_with(prev, curr, spec, &cl_map)
}

/// One account per row: its balance, its worst grade (the worst-graded row decides, as in the
/// reconcile tool) and its segment.
fn migration_per_key(lf: LazyFrame, spec: &ClMigrationSpec, cl_map: &[(String, String)], suffix: &str) -> LazyFrame {
    let bal = format!("__bal_{suffix}");
    let grade = format!("__grade_{suffix}");
    let seg = format!("__seg_{suffix}");
    let g = map_codes_expr(normalise_code_expr(col(spec.cl_col.as_str())), cl_map);
    let mut rank: Expr = lit(NULL).cast(DataType::Int32);
    for (i, name) in CL_GRADES.iter().enumerate().rev() {
        rank = when(g.clone().eq(lit(*name))).then(lit(i as i32)).otherwise(rank);
    }
    let mut select = vec![
        col(spec.key_col.as_str()).cast(DataType::String).alias("__key"),
        col(spec.bal_col.as_str()).cast(DataType::Float64).alias(&bal),
        g.alias("__g"),
        rank.alias("__r"),
    ];
    if let Some(sc) = &spec.segment_col {
        let t = col(sc.as_str()).cast(DataType::String).str().strip_chars(lit(NULL));
        select.push(when(t.clone().is_null().or(t.clone().eq(lit("")))).then(lit(NULL).cast(DataType::String)).otherwise(t).alias("__seg"));
    }
    let mut agg = vec![
        col(&bal).sum().alias(&bal),
        col("__g")
            .sort_by(
                [col("__r"), col(&bal)],
                SortMultipleOptions::default().with_order_descending_multi([true, true]).with_nulls_last(true).with_maintain_order(true),
            )
            .first()
            .alias(&grade),
    ];
    if spec.segment_col.is_some() {
        agg.push(col("__seg").drop_nulls().first().alias(&seg));
    }
    lf.select(select).group_by([col("__key")]).agg(agg)
}

/// The migration matrix with an explicit code map: rows = the previous month's grade (STD, SMA,
/// SS, DF, BL), then "New" (only in the current month); columns = the current month's grade, then
/// "Closed/Repaid" (only in the previous month). A code that is not in the map is an error that
/// lists it. With a segment column there is one block per segment and a last block for all.
pub fn build_cl_migration_with(prev: LazyFrame, curr: LazyFrame, spec: &ClMigrationSpec, cl_map: &[(String, String)]) -> Result<DataFrame> {
    let (mut prev, mut curr) = (prev, curr);
    for (name, lf) in [("previous month", &mut prev), ("current month", &mut curr)] {
        let schema = lf.collect_schema().with_context(|| format!("Could not read the columns of the {name}"))?;
        let mut wanted = vec![spec.key_col.as_str(), spec.bal_col.as_str(), spec.cl_col.as_str()];
        if let Some(sc) = &spec.segment_col {
            wanted.push(sc.as_str());
        }
        for c in wanted {
            if schema.get(c).is_none() {
                bail!("CL migration: the {} has no column '{}'. Choose columns that both months have.", name, c);
            }
        }
        let dt = schema.get(spec.bal_col.as_str()).cloned().unwrap_or(DataType::String);
        if !dt.is_numeric() {
            bail!("CL migration: the balance column '{}' of the {} is {}, not a number. Set it to a number in the schema first.", spec.bal_col, name, dt);
        }
        let blank_keys = lf
            .clone()
            .filter(col(spec.key_col.as_str()).is_null())
            .select([len().alias("n")])
            .collect()
            .with_context(|| format!("Could not check the account keys of the {name}"))?
            .column("n")?
            .cast(&DataType::Int64)?
            .i64()?
            .get(0)
            .unwrap_or(0);
        if blank_keys > 0 {
            bail!("CL migration: the account key column '{}' has {} blank value(s) in the {}. Every row needs an account key.", spec.key_col, blank_keys, name);
        }
    }
    // unmapped codes of either month, in one message
    let mut bad = unmapped_cl_codes(&prev, &spec.cl_col, cl_map)?;
    for c in unmapped_cl_codes(&curr, &spec.cl_col, cl_map)? {
        if !bad.contains(&c) {
            bad.push(c);
        }
    }
    if !bad.is_empty() {
        bail!("{}", cl_code_error(&spec.cl_col, &bad));
    }

    let p = migration_per_key(prev, spec, cl_map, "p");
    let c = migration_per_key(curr, spec, cl_map, "c");
    let joined = p.join(c, [col("__key")], [col("__key")], JoinArgs::new(JoinType::Full).with_coalesce(JoinCoalesce::CoalesceColumns));
    let from = col("__grade_p");
    let to = col("__grade_c");
    // the previous month's balance of an account that was there; the current month's for a new one
    let cell_bal = when(from.clone().is_not_null()).then(col("__bal_p")).otherwise(col("__bal_c")).fill_null(lit(0.0));
    let mut keys: Vec<Expr> = vec![from.clone().fill_null(lit(MIGRATION_NEW_ROW)).alias("__from"), to.clone().fill_null(lit(MIGRATION_CLOSED_COL)).alias("__to")];
    if spec.segment_col.is_some() {
        keys.push(col("__seg_c").fill_null(col("__seg_p")).fill_null(lit(crate::pivot_table::BLANK_LABEL)).alias("__seg"));
    }
    let mut group_by: Vec<Expr> = vec![col("__from"), col("__to")];
    if spec.segment_col.is_some() {
        group_by.push(col("__seg"));
    }
    let grouped = joined
        .with_columns(keys)
        .with_column(cell_bal.alias("__cell"))
        .group_by(group_by)
        .agg([col("__cell").sum().alias("__sum"), len().cast(DataType::Int64).alias("__n")])
        .collect()
        .context("Could not calculate the CL migration matrix")?;

    let froms: Vec<Option<String>> = grouped.column("__from")?.str()?.into_iter().map(|v| v.map(str::to_string)).collect();
    let tos: Vec<Option<String>> = grouped.column("__to")?.str()?.into_iter().map(|v| v.map(str::to_string)).collect();
    let sums: Vec<Option<f64>> = grouped.column("__sum")?.f64()?.into_iter().collect();
    let counts: Vec<Option<i64>> = grouped.column("__n")?.i64()?.into_iter().collect();
    let segs: Vec<Option<String>> = if spec.segment_col.is_some() {
        grouped.column("__seg")?.str()?.into_iter().map(|v| v.map(str::to_string)).collect()
    } else {
        vec![None; grouped.height()]
    };

    // the blocks: one per segment (natural order), then all of them
    let mut seg_names: Vec<String> = Vec::new();
    if spec.segment_col.is_some() {
        for s in segs.iter().flatten() {
            if !seg_names.contains(s) {
                seg_names.push(s.clone());
            }
        }
        seg_names.sort_by(|a, b| crate::engine::natural_sort_key(a).cmp(&crate::engine::natural_sort_key(b)).then_with(|| a.cmp(b)));
    }
    let blocks: Vec<Option<String>> = if spec.segment_col.is_some() {
        seg_names.iter().cloned().map(Some).chain(std::iter::once(Some(MIGRATION_ALL_SEGMENTS.to_string()))).collect()
    } else {
        vec![None]
    };

    let col_labels: Vec<&str> = CL_GRADES.iter().copied().chain(std::iter::once(MIGRATION_CLOSED_COL)).collect();
    let mut seg_out: Vec<String> = Vec::new();
    let mut row_out: Vec<String> = Vec::new();
    let mut values: Vec<Vec<Option<f64>>> = vec![Vec::new(); col_labels.len() + 1];
    for block in &blocks {
        // the matrix of this block: [row][col] of the measure
        let mut m = vec![vec![0.0f64; col_labels.len()]; CL_GRADES.len() + 1];
        for i in 0..grouped.height() {
            let in_block = match block {
                Some(b) if b == MIGRATION_ALL_SEGMENTS => true,
                Some(b) => segs[i].as_deref() == Some(b.as_str()),
                None => true,
            };
            if !in_block {
                continue;
            }
            let (Some(f), Some(t)) = (froms[i].as_deref(), tos[i].as_deref()) else {
                continue;
            };
            let r = if f == MIGRATION_NEW_ROW { Some(CL_GRADES.len()) } else { CL_GRADES.iter().position(|g| *g == f) };
            let c = col_labels.iter().position(|g| *g == t);
            if let (Some(r), Some(c)) = (r, c) {
                m[r][c] += match spec.measure {
                    MigrationMeasure::Balance => sums[i].unwrap_or(0.0),
                    MigrationMeasure::Count => counts[i].unwrap_or(0) as f64,
                };
            }
        }
        // rows: the five grades, the previous month's total, New, the grand total
        let grade_rows: Vec<Vec<f64>> = m[..CL_GRADES.len()].to_vec();
        let prev_total: Vec<f64> = (0..col_labels.len()).map(|c| grade_rows.iter().map(|r| r[c]).sum()).collect();
        let new_row = m[CL_GRADES.len()].clone();
        let grand: Vec<f64> = prev_total.iter().zip(new_row.iter()).map(|(a, b)| a + b).collect();
        let mut lines: Vec<(String, Vec<f64>)> = CL_GRADES.iter().zip(grade_rows).map(|(g, r)| (g.to_string(), r)).collect();
        lines.push((MIGRATION_PREV_TOTAL_ROW.to_string(), prev_total));
        lines.push((MIGRATION_NEW_ROW.to_string(), new_row));
        lines.push((MIGRATION_TOTAL.to_string(), grand));
        for (label, cells) in lines {
            let total: f64 = cells.iter().sum();
            seg_out.push(block.clone().unwrap_or_default());
            row_out.push(label);
            for (c, v) in cells.iter().enumerate() {
                values[c].push(Some(if spec.pct_of_row { if total != 0.0 { v / total * 100.0 } else { 0.0 } } else { *v }));
            }
            values[col_labels.len()].push(Some(if spec.pct_of_row { if total != 0.0 { 100.0 } else { 0.0 } } else { total }));
        }
    }
    let mut series: Vec<Series> = Vec::new();
    if spec.segment_col.is_some() {
        series.push(Series::new("Segment".into(), seg_out));
    }
    series.push(Series::new(MIGRATION_ROW_HEADING.into(), row_out));
    let names: Vec<&str> = col_labels.iter().copied().chain(std::iter::once(MIGRATION_TOTAL)).collect();
    for (name, vals) in names.iter().zip(values) {
        if spec.measure == MigrationMeasure::Count && !spec.pct_of_row {
            let ints: Vec<Option<i64>> = vals.iter().map(|v| v.map(|x| x.round() as i64)).collect();
            series.push(Series::new((*name).into(), ints));
        } else {
            series.push(Series::new((*name).into(), vals));
        }
    }
    DataFrame::new(series).context("Could not arrange the CL migration matrix")
}

// ---------------------------------------------------------------------------------------------
// Liquidity gap ladder (Phase 4 V3)
// ---------------------------------------------------------------------------------------------

/// How the rows of one tab count in the ladder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GapRole {
    /// Every row is an asset (an inflow).
    Asset,
    /// Every row is a liability (an outflow).
    Liability,
    /// A column says which: A / ASSET for assets; L / LIABILITY / DEPOSIT / BORROWING for liabilities.
    Side(String),
}

/// What a tab contributes to the ladder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GapInputSpec {
    pub role: GapRole,
    pub amount_col: String,
    pub maturity_col: String,
}

/// One minimum cumulative gap, in percent of total assets, for one bucket.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GapLimit {
    pub bucket: String,
    pub min_cumulative_gap_pct: f64,
}

/// `portable_data/gap_limits/<preset>.json`: the limits ALCO set for a bucket preset.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct GapLimitsFile {
    #[serde(default)]
    pub preset: String,
    /// Where the limits come from (the ALCO paper, the circular).
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub effective_from: String,
    #[serde(default)]
    pub limits: Vec<GapLimit>,
}

/// The name of the folder of gap limit files.
pub const GAP_LIMITS_DIR: &str = "gap_limits";
/// The ladder's columns.
pub const GAP_COLUMNS: [&str; 7] = [
    "Bucket",
    "Inflows",
    "Outflows",
    "Net gap",
    "Cumulative gap",
    "Net gap % of total assets",
    "Cumulative gap % of total assets",
];
pub const GAP_LIMIT_COLUMN: &str = "Cumulative gap limit %";
pub const GAP_BREACH_COLUMN: &str = "Breach";
pub const GAP_BREACH_FLAG: &str = "BREACH";

/// The limits file of a preset in `dir`.
pub fn gap_limits_path(dir: &Path, preset_name: &str) -> std::path::PathBuf {
    dir.join(format!("{}.json", crate::paths::sanitize_file_name(preset_name)))
}

/// The buckets of a preset in the order of the ladder: the sight bucket (or the long bucket for
/// a blank maturity), the buckets by their day limit, the overflow bucket, then overdue; a label
/// that appears twice is kept at its first place.
pub fn gap_bucket_order(def: &crate::models::CustomBucketDef, null_as_sight: bool) -> Vec<String> {
    let mut rules = def.rules.clone();
    rules.sort_by_key(|r| r.max_days);
    let mut order: Vec<String> = Vec::new();
    let mut push = |l: &str| {
        if !order.iter().any(|x| x == l) {
            order.push(l.to_string());
        }
    };
    if null_as_sight {
        push(&def.null_sight_label);
    }
    for r in &rules {
        push(&r.label);
    }
    push(&def.overflow_label);
    if !null_as_sight {
        push(&def.null_long_label);
    }
    push(&def.overdue_label);
    order
}

/// Reads the limits of a preset. `None` when there is no file; a file that is not valid, lists a
/// bucket the preset does not have, lists one twice or has a limit that is not a number is an
/// error that says what to fix.
pub fn load_gap_limits(dir: &Path, def: &crate::models::CustomBucketDef) -> Result<Option<GapLimitsFile>> {
    let path = gap_limits_path(dir, &def.name);
    if !path.exists() {
        return Ok(None);
    }
    let txt = std::fs::read_to_string(&path).with_context(|| format!("Could not read the gap limits file {}", path.display()))?;
    let file: GapLimitsFile = serde_json::from_str(&txt).map_err(|e| anyhow!("The gap limits file {} is not valid: {}. Expected {{\"limits\": [{{\"bucket\": \"...\", \"min_cumulative_gap_pct\": -10}}]}}.", path.display(), e))?;
    validate_gap_limits(&file, def, &path.display().to_string())?;
    Ok(Some(file))
}

fn validate_gap_limits(file: &GapLimitsFile, def: &crate::models::CustomBucketDef, name: &str) -> Result<()> {
    let known: Vec<String> = gap_bucket_order(def, true).into_iter().chain(gap_bucket_order(def, false)).collect();
    let mut seen: Vec<&str> = Vec::new();
    for l in &file.limits {
        if !known.iter().any(|k| *k == l.bucket) {
            bail!("The gap limits file {} has a limit for '{}', which is not a bucket of the preset '{}'. Use the bucket names exactly as the bucketer writes them.", name, l.bucket, def.name);
        }
        if seen.contains(&l.bucket.as_str()) {
            bail!("The gap limits file {} has two limits for '{}'. Keep one.", name, l.bucket);
        }
        seen.push(&l.bucket);
        if !l.min_cumulative_gap_pct.is_finite() {
            bail!("The gap limits file {} has no valid number for the limit of '{}'.", name, l.bucket);
        }
    }
    Ok(())
}

/// The liquidity gap ladder: for each bucket of the preset, the inflows (assets), the outflows
/// (liabilities), the net gap, the cumulative gap and both as a percentage of total assets (the
/// sum of all inflows). With limits, also the limit of each bucket and a BREACH flag where the
/// cumulative gap % is below it. A blank maturity goes to the preset's sight bucket (or its long
/// bucket); a maturity before the cutoff goes to the overdue bucket.
pub fn build_gap_ladder(
    inputs: Vec<(LazyFrame, GapInputSpec)>,
    def: &crate::models::CustomBucketDef,
    cutoff_date: &str,
    null_as_sight: bool,
    limits: Option<&GapLimitsFile>,
) -> Result<DataFrame> {
    if inputs.is_empty() {
        bail!("Gap ladder: add at least one tab (assets or liabilities).");
    }
    if let Some(l) = limits {
        validate_gap_limits(l, def, "of this preset")?;
    }
    let mut parts: Vec<LazyFrame> = Vec::new();
    for (lf, spec) in inputs {
        let mut lf = lf;
        let schema = lf.collect_schema().context("Could not read the columns of a tab of the gap ladder")?;
        let amount_dt = schema.get(spec.amount_col.as_str()).cloned().ok_or_else(|| anyhow!("Gap ladder: a tab has no column '{}'. Choose its amount column again.", spec.amount_col))?;
        if !amount_dt.is_numeric() {
            bail!("Gap ladder: the amount column '{}' is {}, not a number. Set it to a number in the schema first.", spec.amount_col, amount_dt);
        }
        // +1 for an asset, -1 for a liability
        let (lf, sign) = match &spec.role {
            GapRole::Asset => (lf, lit(1.0)),
            GapRole::Liability => (lf, lit(-1.0)),
            GapRole::Side(c) => direction_sign(lf, Some(c))?,
        };
        let lf = apply_maturity_bucket(lf, &spec.maturity_col, cutoff_date, def, null_as_sight, "__bucket")?;
        let amt = col(spec.amount_col.as_str()).cast(DataType::Float64).fill_null(lit(0.0));
        parts.push(lf.select([
            col("__bucket"),
            when(sign.clone().gt(lit(0.0))).then(amt.clone()).otherwise(lit(0.0)).alias("__in"),
            when(sign.lt(lit(0.0))).then(amt).otherwise(lit(0.0)).alias("__out"),
        ]));
    }
    let grouped = concat(parts, UnionArgs::default())?
        .group_by([col("__bucket")])
        .agg([col("__in").sum(), col("__out").sum()])
        .collect()
        .context("Could not calculate the gap ladder")?;
    let names: Vec<Option<String>> = grouped.column("__bucket")?.str()?.into_iter().map(|v| v.map(str::to_string)).collect();
    let ins: Vec<Option<f64>> = grouped.column("__in")?.f64()?.into_iter().collect();
    let outs: Vec<Option<f64>> = grouped.column("__out")?.f64()?.into_iter().collect();
    let order = gap_bucket_order(def, null_as_sight);
    let mut inflow = vec![0.0f64; order.len()];
    let mut outflow = vec![0.0f64; order.len()];
    for (i, n) in names.iter().enumerate() {
        let Some(n) = n else { continue };
        let Some(pos) = order.iter().position(|o| o == n) else {
            bail!("Gap ladder: the bucket '{}' is not in the preset '{}'.", n, def.name);
        };
        inflow[pos] += ins[i].unwrap_or(0.0);
        outflow[pos] += outs[i].unwrap_or(0.0);
    }
    let total_assets: f64 = inflow.iter().sum();
    let net: Vec<f64> = inflow.iter().zip(&outflow).map(|(a, b)| a - b).collect();
    let mut cum = Vec::with_capacity(net.len());
    let mut run = 0.0;
    for n in &net {
        run += n;
        cum.push(run);
    }
    let pct = |x: f64| -> Option<f64> { if total_assets != 0.0 { Some(x / total_assets * 100.0) } else { None } };
    let net_pct: Vec<Option<f64>> = net.iter().map(|x| pct(*x)).collect();
    let cum_pct: Vec<Option<f64>> = cum.iter().map(|x| pct(*x)).collect();
    let mut series = vec![
        Series::new(GAP_COLUMNS[0].into(), order.clone()),
        Series::new(GAP_COLUMNS[1].into(), inflow),
        Series::new(GAP_COLUMNS[2].into(), outflow),
        Series::new(GAP_COLUMNS[3].into(), net),
        Series::new(GAP_COLUMNS[4].into(), cum),
        Series::new(GAP_COLUMNS[5].into(), net_pct),
        Series::new(GAP_COLUMNS[6].into(), cum_pct.clone()),
    ];
    if let Some(l) = limits {
        if total_assets == 0.0 && !l.limits.is_empty() {
            bail!("Gap ladder: total assets are 0, so the limits (percent of total assets) cannot be checked. Add an asset tab with amounts.");
        }
        let limit_for = |b: &str| l.limits.iter().find(|x| x.bucket == b).map(|x| x.min_cumulative_gap_pct);
        let lim: Vec<Option<f64>> = order.iter().map(|b| limit_for(b)).collect();
        let breach: Vec<String> = lim.iter().zip(&cum_pct).map(|(l, c)| match (l, c) {
            (Some(l), Some(c)) if c < l => GAP_BREACH_FLAG.to_string(),
            _ => String::new(),
        }).collect();
        series.push(Series::new(GAP_LIMIT_COLUMN.into(), lim));
        series.push(Series::new(GAP_BREACH_COLUMN.into(), breach));
    }
    DataFrame::new(series).context("Could not arrange the gap ladder")
}

// ---------------------------------------------------------------------------------------------
// FTP with the dated curves of the store (Phase 4 V4)
// ---------------------------------------------------------------------------------------------

/// The `FTP_Note` of a deal priced on the earliest curve because it is older than every curve.
pub const FTP_NOTE_EARLIEST: &str = "earliest curve used";
/// The column that says which curve priced each deal.
pub const FTP_CURVE_DATE_COLUMN: &str = "FTP_Curve_Date";

fn days_since_epoch(d: NaiveDate) -> i64 {
    (d - NaiveDate::from_ymd_opt(1970, 1, 1).unwrap_or(NaiveDate::MIN)).num_days()
}

/// FTP rates from the dated curves of the store: `FixedDate` prices every deal on the curve dated
/// on or before one date; `OriginationDate` prices each deal on the latest curve dated on or
/// before its open date. Adds `FTP_Curve_Date` (the curve used), and with the option for older
/// deals `FTP_Note`. The other columns are those of [`apply_ftp_curve_map`]: Tenor_Days,
/// Benchmark_FTP_Rate, FTP_Method, FTP_Spread_Pct, Annual_FTP_Margin_BDT.
#[allow(clippy::too_many_arguments)]
pub fn apply_ftp_dated_curves(
    lf: LazyFrame,
    opndat_col: &str,
    matdat_col: &str,
    bal_col: &str,
    rate_col: &str,
    curves: &[crate::curves::YieldCurve],
    mode: &crate::models::FtpCurveMode,
    is_liability: bool,
    interpolation: FtpInterpolation,
) -> Result<LazyFrame> {
    use crate::models::FtpCurveMode;
    let mut sorted: Vec<&crate::curves::YieldCurve> = curves.iter().collect();
    sorted.sort_by_key(|c| c.date);
    let Some(earliest) = sorted.first().copied() else {
        bail!("There are no yield curves in portable_data/curves. Add a curve in Banking > Yield curves, dated on or before the deals' open dates.");
    };
    match mode {
        FtpCurveMode::Inline => bail!("FTP: this step uses the curve typed in it, not the curve store."),
        FtpCurveMode::FixedDate { date } => {
            let d = NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d").map_err(|_| anyhow!("FTP: the curve date '{}' is not a date. Write it as YYYY-MM-DD (for example 2026-06-30).", date))?;
            let Some(c) = sorted.iter().rev().find(|c| c.date <= d) else {
                bail!("FTP: no yield curve is dated on or before {}; the earliest is {}. Add an earlier curve or choose a later date.", d, earliest.date);
            };
            let lf = apply_ftp_curve_map(lf, opndat_col, matdat_col, bal_col, rate_col, &c.points, is_liability, interpolation)?;
            Ok(lf.with_column(lit(days_since_epoch(c.date) as i32).cast(DataType::Date).alias(FTP_CURVE_DATE_COLUMN)))
        }
        FtpCurveMode::OriginationDate { use_earliest_for_older } => {
            let lf = ensure_date(lf, opndat_col)?;
            let lf = ensure_date(lf, matdat_col)?;
            let earliest_days = days_since_epoch(earliest.date);
            let opn_days = col(opndat_col).cast(DataType::Int32).cast(DataType::Int64);
            let mat_days = col(matdat_col).cast(DataType::Int32).cast(DataType::Int64);
            let older = opn_days.clone().lt(lit(earliest_days));
            // deals opened before the earliest curve
            let cnt = lf
                .clone()
                .filter(col(opndat_col).is_not_null().and(older.clone()))
                .select([len().cast(DataType::Int64).alias("n"), col(opndat_col).cast(DataType::Int32).min().alias("first")])
                .collect()
                .context("Could not count the deals opened before the earliest yield curve")?;
            let n = cnt.column("n")?.i64()?.get(0).unwrap_or(0);
            if n > 0 && !use_earliest_for_older {
                let first = cnt.column("first")?.i32()?.get(0).and_then(|d| NaiveDate::from_ymd_opt(1970, 1, 1).and_then(|e| e.checked_add_signed(chrono::Duration::days(d as i64))));
                bail!(
                    "FTP: {} deal(s) were opened before the earliest yield curve ({}){}. Add an earlier curve in Banking > Yield curves, or use the earliest curve for older deals.",
                    n,
                    earliest.date,
                    first.map(|f| format!(", the oldest on {}", f)).unwrap_or_default()
                );
            }
            // the curve of each deal: the latest curve dated on or before its open date
            let mut curve_days: Expr = lit(NULL).cast(DataType::Int64);
            for c in &sorted {
                curve_days = when(opn_days.clone().gt_eq(lit(days_since_epoch(c.date)))).then(lit(days_since_epoch(c.date))).otherwise(curve_days);
            }
            if *use_earliest_for_older {
                curve_days = when(curve_days.clone().is_null().and(col(opndat_col).is_not_null())).then(lit(earliest_days)).otherwise(curve_days);
            }
            let raw_diff = mat_days - opn_days.clone();
            let missing_date = col(matdat_col).is_null().or(col(opndat_col).is_null());
            let tenor_expr = when(missing_date.clone())
                .then(lit(NULL).cast(DataType::Int64))
                .when(raw_diff.clone().lt(lit(1i64)))
                .then(lit(1i64))
                .otherwise(raw_diff)
                .alias("Tenor_Days");
            let mut rate: Expr = lit(NULL).cast(DataType::Float64);
            let mut method: Expr = lit(NULL).cast(DataType::String);
            for c in &sorted {
                let (r, m) = ftp_rate_exprs(&c.points, interpolation).with_context(|| format!("The yield curve of {} cannot be used", c.date))?;
                let hit = col("__ftp_curve_days").eq(lit(days_since_epoch(c.date)));
                rate = when(hit.clone()).then(r).otherwise(rate);
                method = when(hit).then(m).otherwise(method);
            }
            let rate = when(missing_date.clone()).then(lit(NULL).cast(DataType::Float64)).otherwise(rate).alias("Benchmark_FTP_Rate");
            let method = when(missing_date).then(lit(FTP_MISSING_DATE)).otherwise(method).alias("FTP_Method");
            let acct_rate = col(rate_col).cast(DataType::Float64).fill_null(lit(0.0));
            let spread = if is_liability { (col("Benchmark_FTP_Rate") - acct_rate).alias("FTP_Spread_Pct") } else { (acct_rate - col("Benchmark_FTP_Rate")).alias("FTP_Spread_Pct") };
            let margin = (col(bal_col).cast(DataType::Float64).fill_null(lit(0.0)) * (col("FTP_Spread_Pct") / lit(100.0))).alias("Annual_FTP_Margin_BDT");
            let mut out = lf
                .with_column(tenor_expr)
                .with_column(curve_days.alias("__ftp_curve_days"))
                .with_columns([rate, method])
                .with_column(spread)
                .with_column(margin)
                .with_column(col("__ftp_curve_days").cast(DataType::Int32).cast(DataType::Date).alias(FTP_CURVE_DATE_COLUMN));
            if *use_earliest_for_older {
                out = out.with_column(when(col(opndat_col).is_not_null().and(older)).then(lit(FTP_NOTE_EARLIEST)).otherwise(lit(NULL).cast(DataType::String)).alias("FTP_Note"));
            }
            Ok(out.drop(["__ftp_curve_days"]))
        }
    }
}
