use crate::models::{portable_root_dir, CustomBucketDef, FtpPoint};
use anyhow::{anyhow, bail, Context, Result};
use chrono::NaiveDate;
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Checks that a date input is a real Date column before any day-difference maths.
/// Date passes through, Datetime is cast to Date, anything else (text, numbers) is an error,
/// so a wrongly typed column can never be silently bucketed.
fn ensure_date(mut lf: LazyFrame, col_name: &str) -> Result<LazyFrame> {
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

/// Feature B1: Interest Rate Shock / ΔNII Sensitivity Simulator (1-Year Repricing Horizon)
pub fn apply_rate_shock_sim(
    lf: LazyFrame,
    bal_col: &str,
    rate_col: &str,
    mat_col: &str,
    cutoff_date: &str,
    shock_bps: f64,
    side_col: Option<&str>,
) -> Result<LazyFrame> {
    let cutoff = NaiveDate::parse_from_str(cutoff_date.trim(), "%Y-%m-%d")
        .context("Cutoff Date must be YYYY-MM-DD")?;
    let lf = ensure_date(lf, mat_col)?;
    let (lf, sign) = direction_sign(lf, side_col)?;
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let cutoff_days = (cutoff - epoch).num_days();

    let bal = col(bal_col).cast(DataType::Float64).fill_null(lit(0.0));
    let rate = col(rate_col).cast(DataType::Float64).fill_null(lit(0.0));
    let shock_pct = shock_bps / 100.0;

    let rem_days = col(mat_col).cast(DataType::Int32).cast(DataType::Int64) - lit(cutoff_days);

    let repricing_weight = when(col(mat_col).is_null().or(rem_days.clone().lt_eq(lit(0))))
        .then(lit(1.0))
        .when(rem_days.clone().gt(lit(365)))
        .then(lit(0.0))
        .otherwise((lit(365.0) - rem_days.cast(DataType::Float64)) / lit(365.0));

    let current_int = (bal.clone() * (rate / lit(100.0))).alias("Current_Annual_Int");
    // Change in the interest amount (income on assets, expense on liabilities), always >= 0 for a rise.
    let int_change = bal * repricing_weight * lit(shock_pct / 100.0);
    // NII effect: a rate rise adds income on assets (+) and adds cost on liabilities (-).
    let nii_impact = (int_change.clone() * sign).alias("NII_Impact_BDT");
    let shocked_int = (col("Current_Annual_Int") + int_change).alias("Shocked_Annual_Int");

    Ok(lf
        .with_columns([current_int, nii_impact])
        .with_column(shocked_int))
}

/// Feature B2: Automated FTP (Fund Transfer Pricing) Yield Curve Mapper using Tenor Days
pub fn apply_ftp_curve_map(
    lf: LazyFrame,
    opndat_col: &str,
    matdat_col: &str,
    bal_col: &str,
    rate_col: &str,
    curve: &[FtpPoint],
    is_liability: bool,
) -> Result<LazyFrame> {
    let lf = ensure_date(lf, opndat_col)?;
    let lf = ensure_date(lf, matdat_col)?;
    let opn_days = col(opndat_col).cast(DataType::Int32).cast(DataType::Int64);
    let mat_days = col(matdat_col).cast(DataType::Int32).cast(DataType::Int64);
    let raw_diff = mat_days - opn_days;

    let tenor_expr = when(
        col(matdat_col)
            .is_null()
            .or(col(opndat_col).is_null())
            .or(raw_diff.clone().lt(lit(1i64))),
    )
    .then(lit(1i64))
    .otherwise(raw_diff)
    .alias("Tenor_Days");

    let mut sorted_curve = curve.to_vec();
    sorted_curve.sort_by_key(|p| p.tenor_days);

    let ftp_rate_expr = if sorted_curve.is_empty() {
        lit(0.0).alias("Benchmark_FTP_Rate")
    } else {
        let last_rate = sorted_curve.last().unwrap().ftp_rate;
        let mut expr: Expr = lit(last_rate);
        for pt in sorted_curve.iter().rev() {
            expr = when(col("Tenor_Days").lt_eq(lit(pt.tenor_days)))
                .then(lit(pt.ftp_rate))
                .otherwise(expr);
        }
        expr.alias("Benchmark_FTP_Rate")
    };

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
        .with_column(ftp_rate_expr)
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
) -> Result<LazyFrame> {
    let cutoff = NaiveDate::parse_from_str(cutoff_date.trim(), "%Y-%m-%d")
        .context("Cutoff Date must be YYYY-MM-DD")?;
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

    let is_ghost = col(acstat_col)
        .cast(DataType::String)
        .eq(lit("ACT"))
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
        .and(rate.clone().lt(lit(0.0)).or(rate.gt(lit(25.0))));

    let is_neg_bal = bal.lt(lit(0.0));

    let audit_flag_expr = when(is_ghost)
        .then(lit("1. GHOST_MATURED_ACTIVE"))
        .when(is_bad_date)
        .then(lit("2. BACKWARDS_DATE_ANOMALY"))
        .when(is_rate_outlier)
        .then(lit("3. INTEREST_RATE_OUTLIER"))
        .when(is_neg_bal)
        .then(lit("4. NEGATIVE_BALANCE"))
        .otherwise(lit("OK"))
        .alias("Audit_Exception_Flag");

    let mut out_lf = lf.with_column(audit_flag_expr);
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
    let prev_bal_name = "Prev_Balance";
    let curr_bal_name = "Curr_Balance";

    let mut prev_select = vec![
        col(key_col),
        col(bal_col).cast(DataType::Float64).alias(prev_bal_name),
    ];
    let mut curr_select = vec![
        col(key_col),
        col(bal_col).cast(DataType::Float64).alias(curr_bal_name),
    ];

    if let Some(rc) = rate_col {
        prev_select.push(col(rc).cast(DataType::Float64).alias("Prev_Rate"));
        curr_select.push(col(rc).cast(DataType::Float64).alias("Curr_Rate"));
    }
    if let Some(clc) = cl_status_col {
        prev_select.push(col(clc).cast(DataType::String).alias("Prev_CL"));
        curr_select.push(col(clc).cast(DataType::String).alias("Curr_CL"));
    }

    let mut prev_agg = vec![col(prev_bal_name).sum()];
    let mut curr_agg = vec![col(curr_bal_name).sum()];
    if rate_col.is_some() {
        prev_agg.push(col("Prev_Rate").mean());
        curr_agg.push(col("Curr_Rate").mean());
    }
    if cl_status_col.is_some() {
        prev_agg.push(col("Prev_CL").first());
        curr_agg.push(col("Curr_CL").first());
    }

    let left = prev_lf
        .select(prev_select)
        .group_by([col(key_col)])
        .agg(prev_agg);
    let right = curr_lf
        .select(curr_select)
        .group_by([col(key_col)])
        .agg(curr_agg);

    let joined = left.join(
        right,
        [col(key_col)],
        [col(key_col)],
        JoinArgs::new(JoinType::Full).with_coalesce(JoinCoalesce::CoalesceColumns),
    );

    let pb = col(prev_bal_name).fill_null(lit(0.0));
    let cb = col(curr_bal_name).fill_null(lit(0.0));
    let delta_expr = (cb.clone() - pb.clone()).alias("Balance_Delta");

    let status_expr = when(col(prev_bal_name).is_null())
        .then(lit("1. New Account Opened"))
        .when(col(curr_bal_name).is_null())
        .then(lit("2. Closed / Matured Account"))
        .when((cb.clone() - pb.clone()).gt(lit(0.01)))
        .then(lit("3. Balance Increased"))
        .when((cb - pb).lt(lit(-0.01)))
        .then(lit("4. Balance Decreased"))
        .otherwise(lit("5. Unchanged"))
        .alias("Reconcile_Status");

    let mut final_lf = joined.with_columns([status_expr, delta_expr]);

    if rate_col.is_some() {
        let rate_delta_bps = ((col("Curr_Rate").fill_null(lit(0.0))
            - col("Prev_Rate").fill_null(lit(0.0)))
            * lit(100.0))
        .round(2)
        .alias("Rate_Delta_Bps");
        final_lf = final_lf.with_column(rate_delta_bps);
    }

    if cl_status_col.is_some() {
        let p_cl = col("Prev_CL").fill_null(lit("NEW"));
        let c_cl = col("Curr_CL").fill_null(lit("CLOSED"));
        let cl_mig = concat_str([p_cl, c_cl], " -> ", false).alias("CL_Migration_Matrix");
        final_lf = final_lf.with_column(cl_mig);
    }

    Ok(final_lf)
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
//   cl_code_map.json     {"<CBS CL code>": "<STD|SMA|SS|DF|BL>", ...}
//   segment_map.json     {"<segment code>": "<SHORT_TERM_AGRI|CMSME|OTHER>", ...}
//   provision_rules.json [{grade, category, rate_pct, effective_from, effective_to, source, verify}]
// Codes are matched after normalising (uppercase, trimmed, inner spaces collapsed).
// ---------------------------------------------------------------------------------------------

pub const CL_CODE_MAP_FILE: &str = "cl_code_map.json";
pub const SEGMENT_MAP_FILE: &str = "segment_map.json";
pub const PROVISION_RULES_FILE: &str = "provision_rules.json";

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
    ("BLW", "BL"),
    ("BAD/LOSS", "BL"),
    ("BAD", "BL"),
    ("LOSS", "BL"),
];

const DEFAULT_SEGMENT_MAP: &[(&str, &str)] = &[
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
/// silently overwriting the earlier one.
struct JsonPairs(Vec<(String, String)>);

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
                while let Some((k, v)) = map.next_entry::<String, String>()? {
                    pairs.push((k, v));
                }
                Ok(JsonPairs(pairs))
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
    // serde_json errors end with "at line L column C"
    serde_json::from_str(&raw).map_err(|e| anyhow!("{} is not valid: {}", path.display(), e))
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

fn pairs_to_json(pairs: &[(&str, &str)]) -> String {
    let lines: Vec<String> = pairs
        .iter()
        .map(|(k, v)| format!("  {}: {}", serde_json::json!(k), serde_json::json!(v)))
        .collect();
    format!("{{\n{}\n}}\n", lines.join(",\n"))
}

impl ProvisionConfig {
    /// Seed rules from the owner's review notes. Rows with `verify: true` must be checked
    /// against the circular PDFs; the October 2025 start date is a placeholder.
    pub fn default_rules() -> Vec<ProvisionRule> {
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
        let oct25 = "BRPD circular letter Oct 2025 (believed No. 22 of 2025); start date is a placeholder";
        let dec25 = "BB directive Dec 2025 replacing Circular Letter 22/2025";
        let mut rules = vec![
            rule("STD", "ANY", 1.0, "2025-04-01", None, c15, false),
            rule("SMA", "ANY", 5.0, "2025-04-01", None, c15, false),
            rule("SS", "ANY", 20.0, "2025-04-01", None, c15, false),
            rule("DF", "ANY", 50.0, "2025-04-01", None, c15, false),
            rule("BL", "ANY", 100.0, "2025-04-01", None, c15, false),
        ];
        for cat in ["SHORT_TERM_AGRI", "CMSME"] {
            for grade in ["STD", "SMA"] {
                rules.push(rule(grade, cat, 1.0, "2025-10-01", Some("2025-12-20"), oct25, true));
            }
        }
        for cat in ["SHORT_TERM_AGRI", "CMSME"] {
            for grade in ["STD", "SMA"] {
                rules.push(rule(grade, cat, 0.5, "2025-12-21", Some("2026-12-31"), dec25, true));
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
        }
    }

    /// Creates any of the three files that does not exist yet, with the defaults.
    pub fn ensure_default_files(dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("Could not create {}", dir.display()))?;
        write_if_missing(&dir.join(CL_CODE_MAP_FILE), &pairs_to_json(DEFAULT_CL_CODE_MAP))?;
        write_if_missing(&dir.join(SEGMENT_MAP_FILE), &pairs_to_json(DEFAULT_SEGMENT_MAP))?;
        let rules = serde_json::to_string_pretty(&Self::default_rules())
            .context("Could not prepare default provision rules")?;
        write_if_missing(&dir.join(PROVISION_RULES_FILE), &(rules + "\n"))
    }

    /// Loads and validates the three files from `dir`, creating missing ones first.
    pub fn load_or_create(dir: &Path) -> Result<Self> {
        Self::ensure_default_files(dir)?;
        let cl_path = dir.join(CL_CODE_MAP_FILE);
        let seg_path = dir.join(SEGMENT_MAP_FILE);
        let rules_path = dir.join(PROVISION_RULES_FILE);
        let cl: JsonPairs = read_json_file(&cl_path)?;
        let seg: JsonPairs = read_json_file(&seg_path)?;
        let rules: Vec<ProvisionRule> = read_json_file(&rules_path)?;
        validate_rules(&rules, &rules_path)?;
        Ok(Self {
            cl_code_map: validate_code_map(cl.0, &CL_GRADES, &cl_path)?,
            segment_map: validate_code_map(seg.0, &REG_CATEGORIES, &seg_path)?,
            rules,
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
    let bad = lf
        .clone()
        .select([col(cl_status_col).cast(DataType::String).alias("__cl")])
        .filter(map_codes_expr(normalise_code_expr(col("__cl")), &config.cl_code_map).is_null())
        .unique_stable(None, UniqueKeepStrategy::First)
        .limit(20)
        .collect()
        .with_context(|| format!("Could not read the CL status column '{}'", cl_status_col))?;
    if bad.height() > 0 {
        let codes: Vec<String> = bad
            .column("__cl")?
            .str()?
            .into_iter()
            .map(|v| match v {
                Some(s) if !s.trim().is_empty() => format!("'{}'", s),
                _ => "(blank)".to_string(),
            })
            .collect();
        bail!(
            "CL status column '{}' has codes not in {}: {}. Add each code with its grade (STD, SMA, SS, DF or BL) to that file.",
            cl_status_col,
            CL_CODE_MAP_FILE,
            codes.join(", ")
        );
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
    let raw_net = max_expr(bal.clone() - susp - sec, lit(0.0));
    let floored = max_expr(raw_net.clone(), bal.clone() * lit(0.15));
    let npl_bfp = when(floor_exempt).then(raw_net).otherwise(floored);
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
