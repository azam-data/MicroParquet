use crate::models::{BucketPreset, FtpPoint};
use anyhow::{Context, Result};
use chrono::NaiveDate;
use polars::prelude::*;

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

/// Feature A5: Regulatory Maturity Bucketer (Bangladesh Bank SLP & Basel III LCR/NSFR)
pub fn apply_maturity_bucket(
    lf: LazyFrame,
    mat_col: &str,
    cutoff_date: &str,
    preset: BucketPreset,
    null_as_sight: bool,
    new_col: &str,
) -> Result<LazyFrame> {
    let cutoff = NaiveDate::parse_from_str(cutoff_date.trim(), "%Y-%m-%d")
        .context("Cutoff Date must be in YYYY-MM-DD format (e.g., 2026-10-31)")?;

    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let cutoff_days_since_epoch = (cutoff - epoch).num_days();

    let rem_days_col = "__rem_days_temp";
    let lf_with_days = lf.with_column(
        (col(mat_col).cast(DataType::Int32).cast(DataType::Int64) - lit(cutoff_days_since_epoch))
            .alias(rem_days_col),
    );

    let null_label = if null_as_sight {
        "01. Overnight / Sight (Non-Maturing)"
    } else {
        "09. Over 5 Years (Core)"
    };

    let d = col(rem_days_col);
    let bucket_expr = match preset {
        BucketPreset::BangladeshBankSlp => when(col(mat_col).is_null())
            .then(lit(null_label))
            .when(d.clone().lt(lit(0)))
            .then(lit("10. Overdue / Matured Unencashed"))
            .when(d.clone().lt_eq(lit(1)))
            .then(lit("01. Overnight / 1 Day"))
            .when(d.clone().lt_eq(lit(7)))
            .then(lit("02. 2 to 7 Days"))
            .when(d.clone().lt_eq(lit(30)))
            .then(lit("03. 8 to 30 Days"))
            .when(d.clone().lt_eq(lit(90)))
            .then(lit("04. 1 to 3 Months (31-90d)"))
            .when(d.clone().lt_eq(lit(180)))
            .then(lit("05. 3 to 6 Months (91-180d)"))
            .when(d.clone().lt_eq(lit(365)))
            .then(lit("06. 6 to 12 Months (181-365d)"))
            .when(d.clone().lt_eq(lit(1095)))
            .then(lit("07. 1 to 3 Years"))
            .when(d.lt_eq(lit(1825)))
            .then(lit("08. 3 to 5 Years"))
            .otherwise(lit("09. Over 5 Years"))
            .alias(new_col),

        BucketPreset::BaselLcrNsfr => when(col(mat_col).is_null())
            .then(lit("01. Sight / Open Maturity"))
            .when(d.clone().lt(lit(0)))
            .then(lit("06. Overdue / Past Maturity"))
            .when(d.clone().lt_eq(lit(30)))
            .then(lit("01. Within 30 Days (LCR Window)"))
            .when(d.clone().lt_eq(lit(90)))
            .then(lit("02. 31 to 90 Days"))
            .when(d.clone().lt_eq(lit(180)))
            .then(lit("03. 91 to 180 Days"))
            .when(d.lt_eq(lit(365)))
            .then(lit("04. 181 to 365 Days"))
            .otherwise(lit("05. Over 1 Year (NSFR Stable)"))
            .alias(new_col),
    };

    Ok(lf_with_days.with_column(bucket_expr).drop([rem_days_col]))
}

/// Feature B1: Interest Rate Shock / ΔNII Sensitivity Simulator (1-Year Repricing Horizon)
pub fn apply_rate_shock_sim(
    lf: LazyFrame,
    bal_col: &str,
    rate_col: &str,
    mat_col: &str,
    cutoff_date: &str,
    shock_bps: f64,
) -> Result<LazyFrame> {
    let cutoff = NaiveDate::parse_from_str(cutoff_date.trim(), "%Y-%m-%d")
        .context("Cutoff Date must be YYYY-MM-DD")?;
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
    let nii_impact = (bal * repricing_weight * lit(shock_pct / 100.0)).alias("NII_Impact_BDT");
    let shocked_int =
        (col("Current_Annual_Int") + col("NII_Impact_BDT")).alias("Shocked_Annual_Int");

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
        let mut chain = when(col("Tenor_Days").lt_eq(lit(sorted_curve[0].tenor_days)))
            .then(lit(sorted_curve[0].ftp_rate));
        for pt in sorted_curve.iter().skip(1) {
            chain = chain
                .when(col("Tenor_Days").lt_eq(lit(pt.tenor_days)))
                .then(lit(pt.ftp_rate));
        }
        let last_rate = sorted_curve.last().unwrap().ftp_rate;
        chain.otherwise(lit(last_rate)).alias("Benchmark_FTP_Rate")
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
) -> Result<LazyFrame> {
    let cutoff = NaiveDate::parse_from_str(cutoff_date.trim(), "%Y-%m-%d")
        .context("Cutoff Date must be YYYY-MM-DD")?;
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

    let delta_eve = (lit(0.0) - (bal * col("Modified_Duration") * lit(shock_dec)))
        .round(2)
        .alias("Delta_EVE_Impact_BDT");

    Ok(lf
        .with_column(tenor_years)
        .with_column(mac_dur)
        .with_column(mod_dur)
        .with_columns([dv01, delta_eve]))
}

/// Loans & Advances Suite (C1): Bangladesh Bank CL Provisioning & Base For Provision (BFP) Calculator
pub fn apply_loan_classification_provision(
    lf: LazyFrame,
    bal_col: &str,
    cl_status_col: &str,
    segment_col: Option<&str>,
    suspense_col: Option<&str>,
    security_col: Option<&str>,
) -> Result<LazyFrame> {
    let bal = col(bal_col).cast(DataType::Float64).fill_null(lit(0.0));
    let susp = match suspense_col {
        Some(s) if !s.trim().is_empty() => col(s).cast(DataType::Float64).fill_null(lit(0.0)),
        _ => lit(0.0),
    };
    let sec = match security_col {
        Some(s) if !s.trim().is_empty() => col(s).cast(DataType::Float64).fill_null(lit(0.0)),
        _ => lit(0.0),
    };

    let cl_upper = col(cl_status_col)
        .cast(DataType::String)
        .str()
        .to_uppercase()
        .str()
        .strip_chars(lit(" "));

    let is_cmsme = match segment_col {
        Some(seg) if !seg.trim().is_empty() => col(seg)
            .cast(DataType::String)
            .str()
            .to_uppercase()
            .str()
            .contains(lit("SME|CMSME|AGRI|MICRO|COTTAGE"), false),
        _ => lit(false),
    };

    let is_npl = cl_upper
        .clone()
        .eq(lit("SS"))
        .or(cl_upper.clone().eq(lit("DF")))
        .or(cl_upper.clone().eq(lit("BL")))
        .or(cl_upper.clone().contains(lit("SUB|DOUBT|BAD|LOSS"), false));

    let npl_flag = when(is_npl.clone())
        .then(lit("NPL (Classified)"))
        .otherwise(lit("Performing (UC/SMA)"))
        .alias("NPL_Category");

    let raw_net = bal.clone() - susp - sec;
    let net_bfp = when(raw_net.clone().gt(lit(0.0)))
        .then(raw_net)
        .otherwise(lit(0.0));
    let pos_bal = when(bal.clone().gt(lit(0.0))).then(bal).otherwise(lit(0.0));

    let bfp_expr = when(is_npl)
        .then(net_bfp)
        .otherwise(pos_bal)
        .round(2)
        .alias("Base_For_Provision_BDT");

    let prov_rate_pct = when(
        cl_upper
            .clone()
            .eq(lit("BL"))
            .or(cl_upper.clone().contains(lit("BAD|LOSS"), false)),
    )
    .then(lit(100.0))
    .when(
        cl_upper
            .clone()
            .eq(lit("DF"))
            .or(cl_upper.clone().contains(lit("DOUBT"), false)),
    )
    .then(when(is_cmsme.clone()).then(lit(20.0)).otherwise(lit(50.0)))
    .when(
        cl_upper
            .clone()
            .eq(lit("SS"))
            .or(cl_upper.clone().contains(lit("SUB"), false)),
    )
    .then(when(is_cmsme.clone()).then(lit(5.0)).otherwise(lit(20.0)))
    .when(cl_upper.eq(lit("SMA")))
    .then(when(is_cmsme.clone()).then(lit(0.25)).otherwise(lit(1.0)))
    .otherwise(when(is_cmsme).then(lit(0.25)).otherwise(lit(1.0)))
    .alias("Statutory_Prov_Rate_Pct");

    let req_prov = (col("Base_For_Provision_BDT") * (col("Statutory_Prov_Rate_Pct") / lit(100.0)))
        .round(2)
        .alias("Required_Provision_BDT");

    Ok(lf
        .with_columns([npl_flag, bfp_expr, prov_rate_pct])
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
