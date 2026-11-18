//! IFRS 9 staging and expected credit loss (Phase 5 R7, BRPD Circular No. 06 of 8 March 2026).
//!
//! Each loan gets a stage (1, 2 or 3) from its days past due, its classification grade, forbearance,
//! SICR assessment and rating downgrade, with the cure rules of section 4.5.5, and then its ECL:
//! Stage 1 is the first year, Stage 2 the lifetime (probability of default term structure by year,
//! scenario multipliers, discounting at the effective interest rate) and Stage 3 is LGD × EAD. The
//! staging rules are the dated rule file `portable_data/rules/ifrs9_staging/<date>.json`; the
//! parameters (PD curves, LGD, CCF, scenarios) are `portable_data/ecl_params/<date>.json`, a sample
//! the owner replaces with the bank's own.

use crate::banking::normalise_code;
use crate::problems::{row_keys, Problems};
use crate::rules::{load_latest, RuleFile};
use anyhow::{anyhow, bail, Context, Result};
use chrono::NaiveDate;
use polars::prelude::*;
use serde::Deserialize;

pub const ECL_PARAMS_FOLDER: &str = "ecl_params";

/// The output columns.
pub const OUT_COLUMNS: [&str; 6] = ["Stage", "Stage_Reason", "EAD", "ECL", "ECL_Rule_File", "ECL_Params_File"];

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct StagingRules {
    /// Days past due from which credit risk has increased significantly (Stage 2).
    pub sicr_dpd: i64,
    /// Days past due from which the loan is in default (Stage 3).
    pub default_dpd: i64,
    /// The CL_Final grades that count as Stage 3 when the option is on.
    #[serde(default)]
    pub classified_grades: Vec<String>,
    /// The default of the dialog's option "Classified grades are Stage 3" (the app's alignment).
    #[serde(default = "yes")]
    pub classified_is_stage3: bool,
    /// A downgrade of this many notches or more is a significant increase in credit risk.
    pub rating_downgrade_notches: i64,
    /// Days performing before one stage back (§4.5.5), and for forborne exposures.
    pub cure_days: i64,
    pub cure_days_forborne: i64,
    #[serde(default)]
    pub notes: Vec<String>,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct EclParams {
    pub segments: Vec<Segment>,
    pub scenarios: Vec<Scenario>,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Segment {
    pub name: String,
    /// Cumulative probability of default by year in percent: year 1, year 2, …
    pub cumulative_pd_pct: Vec<f64>,
    pub lgd_pct: f64,
    /// The credit conversion factor of the undrawn and off-balance-sheet amount, in percent.
    pub ccf_pct: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Scenario {
    pub name: String,
    pub weight_pct: f64,
    pub pd_multiplier: f64,
}

/// Checks the staging rule file.
pub fn check_staging(r: &StagingRules, file: &str) -> Result<()> {
    if !(r.sicr_dpd > 0 && r.default_dpd > r.sicr_dpd) {
        bail!("The rule file {}: default_dpd ({}) must be above sicr_dpd ({}), and both above 0.", file, r.default_dpd, r.sicr_dpd);
    }
    if r.rating_downgrade_notches < 1 || r.cure_days < 1 || r.cure_days_forborne < 1 {
        bail!("The rule file {}: rating_downgrade_notches, cure_days and cure_days_forborne must each be at least 1.", file);
    }
    for g in &r.classified_grades {
        if crate::overdue::grade_rank(g).is_none() {
            bail!("The rule file {}: the classified grade '{}' is not one of {}.", file, g, crate::overdue::GRADES.join(", "));
        }
    }
    Ok(())
}

/// Checks the parameter file (§5.9): one scenario (its weight is not used), or three or more whose
/// weights sum to 100; exactly two are refused. The warnings are always empty today.
pub fn check_params(p: &EclParams, file: &str) -> Result<Vec<String>> {
    if p.segments.is_empty() {
        bail!("The parameter file {} has no segment.", file);
    }
    let mut seen: Vec<String> = Vec::new();
    for s in &p.segments {
        let n = normalise_code(&s.name);
        if n.is_empty() || seen.contains(&n) {
            bail!("The parameter file {} has a segment with no name, or '{}' twice.", file, s.name);
        }
        seen.push(n);
        if s.cumulative_pd_pct.is_empty() || s.cumulative_pd_pct.iter().any(|v| !(v.is_finite() && (0.0..=100.0).contains(v))) || s.cumulative_pd_pct.windows(2).any(|w| w[1] < w[0]) {
            bail!("The parameter file {}: the cumulative PD of '{}' must be percentages from 0 to 100 that do not fall from one year to the next.", file, s.name);
        }
        for (what, v) in [("lgd_pct", s.lgd_pct), ("ccf_pct", s.ccf_pct)] {
            if !(v.is_finite() && (0.0..=100.0).contains(&v)) {
                bail!("The parameter file {}: {} of '{}' must be between 0 and 100.", file, what, s.name);
            }
        }
    }
    if p.scenarios.is_empty() {
        bail!("The parameter file {} has no scenario.", file);
    }
    if p.scenarios.iter().any(|s| !(s.weight_pct.is_finite() && s.weight_pct >= 0.0 && s.pd_multiplier.is_finite() && s.pd_multiplier >= 0.0)) {
        bail!("The parameter file {}: each scenario needs a weight and a PD multiplier that are not negative.", file);
    }
    // BRPD Circular 06/2026 section 5.9: one scenario (no weighting), or at least three weighted to 100
    match p.scenarios.len() {
        1 => {}
        2 => bail!("The parameter file {} has 2 scenarios. BRPD Circular 06/2026 section 5.9 allows one scenario (no weighting) or at least three scenarios with weights that sum to 100; add a scenario or remove one.", file),
        _ => {
            let total: f64 = p.scenarios.iter().map(|s| s.weight_pct).sum();
            if (total - 100.0).abs() > 1e-6 {
                bail!("The parameter file {}: the scenario weights sum to {}, not 100. Fix the weights (BRPD Circular 06/2026, section 5.9).", file, trim_num(total));
            }
        }
    }
    Ok(Vec::new())
}

fn trim_num(v: f64) -> String {
    let s = format!("{:.6}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn load_staging(cutoff: NaiveDate) -> Result<RuleFile<StagingRules>> {
    let f: RuleFile<StagingRules> = load_latest(&crate::rules::rules_dir("ifrs9_staging"), "ifrs9_staging", cutoff, Some(crate::rules::IFRS9_STAGING))?;
    check_staging(&f.data, &f.meta.file)?;
    Ok(f)
}

pub fn load_params(cutoff: NaiveDate) -> Result<(RuleFile<EclParams>, Vec<String>)> {
    let dir = crate::rules::data_dir().join(ECL_PARAMS_FOLDER);
    let f: RuleFile<EclParams> = load_latest(&dir, ECL_PARAMS_FOLDER, cutoff, Some(crate::rules::ECL_PARAMS))?;
    let w = check_params(&f.data, &f.meta.file)?;
    Ok((f, w))
}

/// The ECL of one loan: Stage 3 is LGD × EAD; Stage 1 is the first year (or the remaining term if
/// shorter), Stage 2 every year to the remaining term, a final part-year scaling its marginal PD by
/// the fraction; each year's loss is discounted at the EIR as (1 + EIR)^−y; the scenarios are
/// weighted. `eir` is a fraction (0.10 for 10%).
pub fn ecl_of(ead: f64, stage: u8, term_years: f64, eir: f64, seg: &Segment, scenarios: &[Scenario]) -> Result<f64> {
    let lgd = seg.lgd_pct / 100.0;
    if stage >= 3 {
        return Ok(lgd * ead);
    }
    let horizon = if stage == 1 { term_years.min(1.0) } else { term_years };
    if horizon <= 0.0 {
        return Ok(0.0);
    }
    let years = horizon.ceil() as usize;
    if years > seg.cumulative_pd_pct.len() {
        bail!("the remaining term of {:.2} years is longer than the {} years of the PD curve of '{}'", term_years, seg.cumulative_pd_pct.len(), seg.name);
    }
    let mut total = 0.0;
    for sc in scenarios {
        let mut e = 0.0;
        for y in 1..=years {
            // the scenario scales the cumulative PD, which is capped at 1; the marginal PD is the difference
            // of the capped cumulatives, so the losses of all years never add up to more than LGD × EAD
            let cum = |i: usize| if i == 0 { 0.0 } else { (seg.cumulative_pd_pct[i - 1] / 100.0 * sc.pd_multiplier).min(1.0) };
            let marginal = (cum(y) - cum(y - 1)).max(0.0);
            let frac = (horizon - (y as f64 - 1.0)).min(1.0);
            e += marginal * frac * lgd * ead / (1.0 + eir).powi(y as i32);
        }
        // a single scenario is not weighted
        total += if scenarios.len() == 1 { 1.0 } else { sc.weight_pct / 100.0 } * e;
    }
    Ok(total)
}

/// The remaining term in years: the complete calendar years from the cutoff to the maturity, plus
/// the days left after the last anniversary ÷ 365. A loan maturing exactly two years after the
/// cutoff has 2.0 years (days ÷ 365 would say 2.0027 across a leap day). 0 when the maturity is on
/// or before the cutoff.
pub fn remaining_years(cutoff: NaiveDate, maturity: NaiveDate) -> f64 {
    let Some(months) = crate::overdue::complete_months(cutoff, maturity) else {
        return 0.0;
    };
    if maturity <= cutoff {
        return 0.0;
    }
    let years = months / 12;
    let anniversary = cutoff.checked_add_months(chrono::Months::new((years * 12) as u32)).unwrap_or(cutoff);
    years as f64 + (maturity - anniversary).num_days().max(0) as f64 / 365.0
}

/// What the dialog asks for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EclSpec {
    pub segment_col: Option<String>,
    pub fixed_segment: String,
    pub drawn_col: String,
    pub undrawn_col: Option<String>,
    /// The effective interest rate in percent.
    pub eir_col: String,
    pub maturity_col: String,
    pub dpd_col: Option<String>,
    pub grade_col: Option<String>,
    pub classified_stage3: bool,
    pub forborne_col: Option<String>,
    pub sicr_unassessed_col: Option<String>,
    pub rating_orig_col: Option<String>,
    pub rating_now_col: Option<String>,
    pub prev_stage_col: Option<String>,
    pub days_performing_col: Option<String>,
    pub cutoff: Option<NaiveDate>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EclStats {
    pub rows: usize,
    pub stage_counts: [usize; 3],
    /// Stage 1 and 2 rows whose maturity is on or before the cutoff: no remaining term, ECL 0.
    pub no_term: usize,
    /// Rows that stay in the previous stage because their cure period is not over.
    pub cure_kept: usize,
    /// Rows with no rating, so the downgrade test was not applied.
    pub no_rating: usize,
    pub warnings: Vec<String>,
}

impl EclStats {
    pub fn note(&self, label: &str) -> String {
        let mut s = format!("{}: Stage 1 {}, Stage 2 {}, Stage 3 {} loan(s).", label, self.stage_counts[0], self.stage_counts[1], self.stage_counts[2]);
        if self.cure_kept > 0 {
            s.push_str(&format!(" {} loan(s) stay in their previous stage until the cure period is over.", self.cure_kept));
        }
        if self.no_term > 0 {
            s.push_str(&format!(" {} Stage 1 or 2 loan(s) have a maturity on or before the cutoff: no remaining term, ECL 0.", self.no_term));
        }
        if self.no_rating > 0 {
            s.push_str(&format!(" {} loan(s) have no rating, so the rating downgrade test was not applied.", self.no_rating));
        }
        for w in &self.warnings {
            s.push(' ');
            s.push_str(w);
        }
        s
    }
}

/// The result of the run: the six columns.
pub struct EclOutput {
    pub stage: Vec<i64>,
    pub reason: Vec<String>,
    pub ead: Vec<f64>,
    pub ecl: Vec<f64>,
    pub rule_file: String,
    pub params_file: String,
    pub stats: EclStats,
}

fn truthy(v: &AnyValue, what: &str, col: &str) -> std::result::Result<bool, String> {
    Ok(match v {
        AnyValue::Null => false,
        AnyValue::Boolean(b) => *b,
        AnyValue::String(s) => parse_flag(s).ok_or_else(|| format!("'{}' is not Y/N, Yes/No, True/False or 1/0 (the {} column '{}')", s.trim(), what, col))?,
        AnyValue::StringOwned(s) => parse_flag(s.as_str()).ok_or_else(|| format!("'{}' is not Y/N, Yes/No, True/False or 1/0 (the {} column '{}')", s.trim(), what, col))?,
        other => match other.extract::<f64>() {
            Some(x) if x == 0.0 => false,
            Some(x) if x == 1.0 => true,
            _ => return Err(format!("{:?} is not 1 or 0 (the {} column '{}')", other, what, col)),
        },
    })
}

fn parse_flag(s: &str) -> Option<bool> {
    match s.trim().to_uppercase().as_str() {
        "" | "N" | "NO" | "FALSE" | "F" | "0" => Some(false),
        "Y" | "YES" | "TRUE" | "T" | "1" => Some(true),
        _ => None,
    }
}

fn parse_stage(s: &str) -> Option<u8> {
    let t = s.trim().to_uppercase();
    let t = t.trim_start_matches("STAGE").trim_start_matches('S').trim();
    match t {
        "1" | "1.0" => Some(1),
        "2" | "2.0" => Some(2),
        "3" | "3.0" => Some(3),
        _ => None,
    }
}

/// Runs staging and ECL over the rows of `df`.
pub fn compute(df: &DataFrame, spec: &EclSpec, staging: &RuleFile<StagingRules>, params: &RuleFile<EclParams>, params_warnings: &[String], cl_map: &[(String, String)]) -> Result<EclOutput> {
    let who = "IFRS 9 ECL";
    let cutoff = spec.cutoff.ok_or_else(|| anyhow!("{}: type the cutoff date as YYYY-MM-DD (for example 2026-10-31).", who))?;
    let height = df.height();
    let (keys, key_label) = row_keys(df);
    let col = |name: &str, what: &str| -> Result<Series> {
        df.column(name).cloned().map_err(|_| anyhow!("{}: the {} column '{}' was not found. Choose it again.", who, what, name))
    };
    let given = |c: &Option<String>| c.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let number = |name: &str, what: &str| -> Result<Vec<Option<f64>>> {
        let c = col(name, what)?;
        if !c.dtype().is_numeric() {
            bail!("{}: the {} column '{}' is {}, not a number. Set it to a number in the Schema & Ingestion settings.", who, what, name, c.dtype());
        }
        Ok(c.cast(&DataType::Float64)?.f64()?.into_iter().map(|v| v.filter(|x| x.is_finite())).collect())
    };
    let mut problems = Problems::default();

    // exposure
    let drawn = number(&spec.drawn_col, "drawn amount")?;
    let undrawn = match given(&spec.undrawn_col) {
        Some(c) => Some(number(&c, "undrawn amount")?),
        None => None,
    };
    let eir = number(&spec.eir_col, "effective interest rate")?;
    let mat_col = col(&spec.maturity_col, "maturity date")?;
    let maturity: Vec<Option<NaiveDate>> = match mat_col.dtype() {
        DataType::Date | DataType::Datetime(_, _) => {
            let days = mat_col.cast(&DataType::Date)?.cast(&DataType::Int32)?;
            let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).ok_or_else(|| anyhow!("calendar"))?;
            days.i32()?.into_iter().map(|d| d.and_then(|d| chrono::TimeDelta::try_days(d as i64).and_then(|t| epoch.checked_add_signed(t)))).collect()
        }
        other => bail!("{}: the maturity column '{}' is {}, not a date. Set it to a Date type in the Schema & Ingestion settings.", who, spec.maturity_col, other),
    };
    // segments
    let seg_names: Vec<String> = params.data.segments.iter().map(|s| s.name.clone()).collect();
    let mut seg_of: Vec<usize> = vec![0; height];
    match given(&spec.segment_col) {
        Some(sc) => {
            let c = col(&sc, "segment")?.cast(&DataType::String)?;
            let vals: Vec<Option<String>> = c.str()?.into_iter().map(|v| v.map(str::to_string)).collect();
            let mut bad: Vec<String> = Vec::new();
            let mut n_bad = 0;
            for (i, v) in vals.iter().enumerate() {
                let code = normalise_code(v.as_deref().unwrap_or(""));
                match params.data.segments.iter().position(|s| normalise_code(&s.name) == code) {
                    Some(p) => seg_of[i] = p,
                    None => {
                        n_bad += 1;
                        let shown = if code.is_empty() { "(blank)".to_string() } else { format!("'{}'", v.clone().unwrap_or_default().trim()) };
                        if !bad.contains(&shown) && bad.len() < 10 {
                            bad.push(shown);
                        }
                    }
                }
            }
            if n_bad > 0 {
                bail!("{}: the segment column '{}' has values that are not segments of {}: {}, on {} row(s). The segments are {}.", who, sc, params.meta.file, bad.join(", "), n_bad, seg_names.join(", "));
            }
        }
        None => {
            let p = params.data.segments.iter().position(|s| normalise_code(&s.name) == normalise_code(&spec.fixed_segment));
            let Some(p) = p else {
                bail!("{}: choose the segment of the rows ({}), or a column that holds it.", who, seg_names.join(", "));
            };
            seg_of.iter_mut().for_each(|s| *s = p);
        }
    }
    // days past due, grade, flags, ratings, previous stage
    let dpd = match given(&spec.dpd_col) {
        Some(c) => Some(number(&c, "days past due")?),
        None => None,
    };
    // the grade of each row as one of the seven (STD-0 … B/L), read through cl_code_map.json; a code that is
    // not in it, or a blank, stops the run (the Phase 1 rule)
    let grades: Option<Vec<usize>> = match given(&spec.grade_col) {
        Some(c) => {
            let vals: Vec<Option<String>> = col(&c, "grade")?.cast(&DataType::String)?.str()?.into_iter().map(|v| v.map(str::to_string)).collect();
            let mut ranks = Vec::with_capacity(height);
            let mut bad: Vec<String> = Vec::new();
            for v in &vals {
                let code = v.as_deref().unwrap_or("").trim();
                match crate::cl1::grade_of(code, cl_map).and_then(crate::overdue::grade_rank) {
                    Some(r) => ranks.push(r),
                    None => {
                        ranks.push(0);
                        let shown = if code.is_empty() { "(blank)".to_string() } else { format!("'{}'", code) };
                        if !bad.contains(&shown) && bad.len() < 20 {
                            bad.push(shown);
                        }
                    }
                }
            }
            if !bad.is_empty() {
                bail!("{}", crate::banking::cl_code_error(&c, &bad));
            }
            Some(ranks)
        }
        None => None,
    };
    let flag = |c: &Option<String>, what: &str, problems: &mut Problems| -> Result<Option<Vec<bool>>> {
        let Some(name) = given(c) else { return Ok(None) };
        let column = col(&name, what)?;
        let mut v = vec![false; height];
        for (i, slot) in v.iter_mut().enumerate() {
            match truthy(&column.get(i)?, what, &name) {
                Ok(b) => *slot = b,
                Err(e) => problems.add(&e, || keys[i].clone()),
            }
        }
        Ok(Some(v))
    };
    let forborne = flag(&spec.forborne_col, "forborne", &mut problems)?;
    let unassessed = flag(&spec.sicr_unassessed_col, "SICR not assessed", &mut problems)?;
    let (r0, r1) = match (given(&spec.rating_orig_col), given(&spec.rating_now_col)) {
        (Some(a), Some(b)) => (Some(number(&a, "rating at origination")?), Some(number(&b, "current rating")?)),
        (None, None) => (None, None),
        _ => bail!("{}: choose both the rating at origination and the current rating, or neither.", who),
    };
    let (prev, perf) = match (given(&spec.prev_stage_col), given(&spec.days_performing_col)) {
        (Some(p), Some(d)) => {
            let c = col(&p, "previous stage")?;
            let mut v: Vec<Option<u8>> = vec![None; height];
            for (i, slot) in v.iter_mut().enumerate() {
                let val = c.get(i)?;
                *slot = match &val {
                    AnyValue::Null => None,
                    AnyValue::String(s) if s.trim().is_empty() => None,
                    AnyValue::String(s) => match parse_stage(s) {
                        Some(x) => Some(x),
                        None => {
                            problems.add(&format!("the previous stage in '{}' is not 1, 2 or 3", p), || keys[i].clone());
                            None
                        }
                    },
                    other => match other.extract::<f64>().and_then(|x| if x.fract() == 0.0 { Some(x as i64) } else { None }) {
                        Some(1) => Some(1),
                        Some(2) => Some(2),
                        Some(3) => Some(3),
                        _ => {
                            problems.add(&format!("the previous stage in '{}' is not 1, 2 or 3", p), || keys[i].clone());
                            None
                        }
                    },
                };
            }
            (Some(v), Some((d.clone(), number(&d, "days performing")?)))
        }
        (None, None) => (None, None),
        _ => bail!("{}: choose both the previous stage and the days performing, or neither (the cure rule needs both).", who),
    };

    // 1. the stage of each row
    let rules = &staging.data;
    let classified: Vec<usize> = rules.classified_grades.iter().filter_map(|g| crate::overdue::grade_rank(g)).collect();
    let mut stats = EclStats { rows: height, warnings: params_warnings.to_vec(), ..Default::default() };
    let mut stage = vec![1u8; height];
    let mut reason = vec![String::new(); height];
    for i in 0..height {
        let (mut s, mut why) = (1u8, "Performing".to_string());
        let d = match &dpd {
            Some(v) => match v[i] {
                Some(x) if x >= 0.0 && x.fract() == 0.0 => x as i64,
                Some(_) => {
                    problems.add(&format!("the days past due in '{}' is negative or not a whole number", spec.dpd_col.clone().unwrap_or_default()), || keys[i].clone());
                    continue;
                }
                None => {
                    problems.add(&format!("the days past due in '{}' is blank", spec.dpd_col.clone().unwrap_or_default()), || keys[i].clone());
                    continue;
                }
            },
            None => 0,
        };
        let grade_rank = grades.as_ref().map(|g| g[i]);
        let is_forborne = forborne.as_ref().is_some_and(|f| f[i]);
        let mut set = |stage: u8, text: String| {
            if stage > s {
                s = stage;
                why = text;
            }
        };
        if d >= rules.default_dpd {
            set(3, format!("{}+ DPD", rules.default_dpd));
        }
        if let (true, Some(r)) = (spec.classified_stage3, grade_rank) {
            if classified.contains(&r) {
                set(3, format!("Classified {}", crate::overdue::GRADES[r]));
            }
        }
        if d >= rules.sicr_dpd {
            set(2, format!("{}+ DPD", rules.sicr_dpd));
        }
        if is_forborne {
            set(2, "Forborne".to_string());
        }
        if unassessed.as_ref().is_some_and(|u| u[i]) {
            set(2, "SICR not assessed".to_string());
        }
        if let (Some(a), Some(b)) = (&r0, &r1) {
            match (a[i], b[i]) {
                (Some(a), Some(b)) => {
                    if b - a >= rules.rating_downgrade_notches as f64 {
                        set(2, format!("Rating down {} notches or more", rules.rating_downgrade_notches));
                    }
                }
                _ => stats.no_rating += 1,
            }
        }
        // cure (section 4.5.5): one stage back for each full cure period of days performing
        if let (Some(p), Some((dcol, days))) = (&prev, &perf) {
            if let Some(pv) = p[i] {
                if pv > s {
                    let Some(dp) = days[i] else {
                        problems.add(&format!("the days performing in '{}' is blank where the previous stage was worse", dcol), || keys[i].clone());
                        continue;
                    };
                    let period = if is_forborne { rules.cure_days_forborne } else { rules.cure_days } as f64;
                    let steps = (dp / period).floor().max(0.0) as u8;
                    let allowed = pv.saturating_sub(steps).max(s);
                    if allowed > s {
                        stats.cure_kept += 1;
                        s = allowed;
                        why = format!("Cure period not over ({} of {} days performing)", trim_num(dp), period as i64);
                    }
                }
            }
        }
        stage[i] = s;
        reason[i] = why;
    }

    // 2. EAD and ECL
    let mut ead = vec![0.0f64; height];
    let mut ecl = vec![0.0f64; height];
    for i in 0..height {
        let seg = &params.data.segments[seg_of[i]];
        let Some(dr) = drawn[i] else {
            problems.add(&format!("the drawn amount in '{}' is blank or not a number", spec.drawn_col), || keys[i].clone());
            continue;
        };
        let un = match &undrawn {
            Some(u) => match u[i] {
                Some(x) => x,
                None => {
                    problems.add(&format!("the undrawn amount in '{}' is blank or not a number", spec.undrawn_col.clone().unwrap_or_default()), || keys[i].clone());
                    continue;
                }
            },
            None => 0.0,
        };
        if dr < 0.0 || un < 0.0 {
            problems.add("a drawn or undrawn amount is negative (use positive amounts)", || keys[i].clone());
            continue;
        }
        ead[i] = dr + seg.ccf_pct / 100.0 * un;
        if stage[i] == 3 {
            ecl[i] = ecl_of(ead[i], 3, 0.0, 0.0, seg, &params.data.scenarios)?;
            continue;
        }
        let Some(e) = eir[i] else {
            problems.add(&format!("the effective interest rate in '{}' is blank or not a number", spec.eir_col), || keys[i].clone());
            continue;
        };
        let Some(m) = maturity[i] else {
            problems.add(&format!("the maturity date in '{}' is blank (a Stage 1 or 2 loan needs its remaining term)", spec.maturity_col), || keys[i].clone());
            continue;
        };
        let term = remaining_years(cutoff, m);
        if term <= 0.0 {
            stats.no_term += 1;
            continue;
        }
        match ecl_of(ead[i], stage[i], term, e / 100.0, seg, &params.data.scenarios) {
            Ok(v) => ecl[i] = v,
            Err(err) => problems.add(&format!("{}", err), || keys[i].clone()),
        }
    }
    problems.into_result(who, &key_label)?;
    for s in &stage {
        stats.stage_counts[(*s as usize).clamp(1, 3) - 1] += 1;
    }
    Ok(EclOutput { stage: stage.iter().map(|s| *s as i64).collect(), reason, ead, ecl, rule_file: staging.meta.file.clone(), params_file: params.meta.file.clone(), stats })
}

/// Loads the two files for the cutoff and runs the table through them.
pub fn classify_lazy(lf: LazyFrame, spec: &EclSpec) -> Result<(LazyFrame, EclStats)> {
    let cutoff = spec.cutoff.ok_or_else(|| anyhow!("IFRS 9 ECL: type the cutoff date as YYYY-MM-DD (for example 2026-10-31)."))?;
    let staging = load_staging(cutoff)?;
    let (params, warnings) = load_params(cutoff)?;
    let mut lf = lf;
    let schema = lf.collect_schema().context("Could not read the columns for IFRS 9 ECL")?;
    for c in OUT_COLUMNS {
        if schema.get(c).is_some() {
            bail!("IFRS 9 ECL: the table already has a column '{}'. Remove that column or the earlier step that made it first.", c);
        }
    }
    let lf = match schema.get(spec.maturity_col.as_str()) {
        Some(_) => crate::banking::ensure_date(lf, &spec.maturity_col)?,
        None => lf,
    };
    let df = lf.collect().context("IFRS 9 ECL could not read the table")?;
    let cl_map = crate::banking::ProvisionConfig::load_or_create(&crate::rules::data_dir())?.cl_code_map;
    let out = compute(&df, spec, &staging, &params, &warnings, &cl_map)?;
    let mut df = df;
    let n = df.height();
    df.with_column(Series::new("Stage".into(), out.stage))?;
    df.with_column(Series::new("Stage_Reason".into(), out.reason))?;
    df.with_column(Series::new("EAD".into(), out.ead))?;
    df.with_column(Series::new("ECL".into(), out.ecl))?;
    df.with_column(Series::new("ECL_Rule_File".into(), vec![out.rule_file; n]))?;
    df.with_column(Series::new("ECL_Params_File".into(), vec![out.params_file; n]))?;
    Ok((df.lazy(), out.stats))
}

pub fn apply_ifrs_ecl(lf: LazyFrame, spec: &EclSpec) -> Result<LazyFrame> {
    classify_lazy(lf, spec).map(|r| r.0)
}

/// The spec of an `IfrsEcl` step.
pub fn spec_of(step: &crate::models::TransformStep) -> Result<EclSpec> {
    let crate::models::TransformStep::IfrsEcl { segment_col, fixed_segment, drawn_col, undrawn_col, eir_col, maturity_col, dpd_col, grade_col, classified_stage3, forborne_col, sicr_unassessed_col, rating_orig_col, rating_now_col, prev_stage_col, days_performing_col, cutoff_date } = step else {
        bail!("internal: not an IFRS 9 ECL step");
    };
    let g = |c: &Option<String>| c.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let cutoff = NaiveDate::parse_from_str(cutoff_date.trim(), "%Y-%m-%d").map_err(|_| anyhow!("IFRS 9 ECL: type the cutoff date as YYYY-MM-DD (for example 2026-10-31)."))?;
    Ok(EclSpec {
        segment_col: g(segment_col),
        fixed_segment: fixed_segment.trim().to_string(),
        drawn_col: drawn_col.trim().to_string(),
        undrawn_col: g(undrawn_col),
        eir_col: eir_col.trim().to_string(),
        maturity_col: maturity_col.trim().to_string(),
        dpd_col: g(dpd_col),
        grade_col: g(grade_col),
        classified_stage3: *classified_stage3,
        forborne_col: g(forborne_col),
        sicr_unassessed_col: g(sicr_unassessed_col),
        rating_orig_col: g(rating_orig_col),
        rating_now_col: g(rating_now_col),
        prev_stage_col: g(prev_stage_col),
        days_performing_col: g(days_performing_col),
        cutoff: Some(cutoff),
    })
}

// ------------------------------------------------------------------------------------- the summary

/// The columns of the summary tab.
pub const SUMMARY_COLUMNS: [&str; 8] = ["Section", "Group", "Loans", "EAD", "ECL", "Regulatory provision", "Higher of", "Shortfall to Loan Loss Reserve"];

/// What the summary reads from a table that has the ECL columns.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EclSummarySpec {
    /// The classification grade (`CL_Final`).
    pub grade_col: String,
    /// The regulatory provision of the Phase 1 Provision step (`Required_Provision_BDT`).
    pub provision_col: String,
}

/// The grade group of a CL code: STD and SMA are Standard; SS, DF and B/L are their own.
fn group_of(code: &str, cl_map: &[(String, String)]) -> Option<&'static str> {
    let grade = crate::cl1::grade_of(code, cl_map)?;
    Some(match crate::overdue::grade_rank(grade)? {
        0..=3 => "Standard",
        4 => "SS",
        5 => "DF",
        _ => "B/L",
    })
}

/// The §9.1 summary: ECL, regulatory provision and the higher of the two, by grade group and by
/// stage, and the shortfall to the Loan Loss Reserve (the total regulatory provision less the total
/// ECL, not below 0).
pub fn summary_frame(df: &DataFrame, spec: &EclSummarySpec, cl_map: &[(String, String)]) -> Result<DataFrame> {
    let who = "IFRS 9 summary";
    let (keys, key_label) = row_keys(df);
    for c in ["Stage", "EAD", "ECL"] {
        if df.column(c).is_err() {
            bail!("{}: the tab has no column '{}'. Run the IFRS 9 ECL step first (it adds Stage, EAD and ECL).", who, c);
        }
    }
    let num = |name: &str| -> Result<Vec<Option<f64>>> {
        let c = df.column(name).map_err(|_| anyhow!("{}: the column '{}' was not found. Choose it again.", who, name))?;
        if !c.dtype().is_numeric() {
            bail!("{}: the column '{}' is {}, not a number.", who, name, c.dtype());
        }
        Ok(c.cast(&DataType::Float64)?.f64()?.into_iter().collect())
    };
    let stage = num("Stage")?;
    let ead = num("EAD")?;
    let ecl = num("ECL")?;
    let prov = num(&spec.provision_col)?;
    let grades = df.column(&spec.grade_col).map_err(|_| anyhow!("{}: the grade column '{}' was not found. Choose it again.", who, spec.grade_col))?.cast(&DataType::String)?;
    let mut problems = Problems::default();
    // [group][ (loans, ead, ecl, prov) ]
    let groups = ["Standard", "SS", "DF", "B/L"];
    let mut by_group = [[0.0f64; 4]; 4];
    let mut by_stage = [[0.0f64; 4]; 3];
    for i in 0..df.height() {
        let g = group_of(grades.str()?.get(i).unwrap_or(""), cl_map).and_then(|g| groups.iter().position(|x| *x == g));
        let (Some(g), Some(Some(s)), Some(Some(e)), Some(Some(l)), Some(Some(p))) = (g, stage.get(i).copied(), ead.get(i).copied(), ecl.get(i).copied(), prov.get(i).copied()) else {
            problems.add(&format!("the grade in '{}' is not a known code, or the stage, EAD, ECL or '{}' is blank", spec.grade_col, spec.provision_col), || keys[i].clone());
            continue;
        };
        let si = (s as usize).clamp(1, 3) - 1;
        for (arr, idx) in [(&mut by_group[g], 0usize), (&mut by_stage[si], 0usize)] {
            let _ = idx;
            arr[0] += 1.0;
            arr[1] += e;
            arr[2] += l;
            arr[3] += p;
        }
    }
    problems.into_result(who, &key_label)?;
    let (mut section, mut group, mut loans, mut e, mut l, mut p, mut h, mut sf): (Vec<String>, Vec<String>, Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>, Vec<Option<f64>>) = Default::default();
    let mut push = |sec: &str, name: &str, a: [f64; 4], shortfall: Option<f64>| {
        section.push(sec.to_string());
        group.push(name.to_string());
        loans.push(a[0]);
        e.push(a[1]);
        l.push(a[2]);
        p.push(a[3]);
        h.push(a[2].max(a[3]));
        sf.push(shortfall);
    };
    let total = |rows: &[[f64; 4]]| -> [f64; 4] { rows.iter().fold([0.0; 4], |mut t, r| {
        for k in 0..4 {
            t[k] += r[k];
        }
        t
    }) };
    for (gi, name) in groups.iter().enumerate() {
        push("Grade group", name, by_group[gi], None);
    }
    let tg = total(&by_group);
    push("Grade group", "Total", tg, Some((tg[3] - tg[2]).max(0.0)));
    for (si, name) in ["Stage 1", "Stage 2", "Stage 3"].iter().enumerate() {
        push("Stage", name, by_stage[si], None);
    }
    let ts = total(&by_stage);
    push("Stage", "Total", ts, Some((ts[3] - ts[2]).max(0.0)));
    Ok(DataFrame::new(vec![
        Series::new(SUMMARY_COLUMNS[0].into(), section),
        Series::new(SUMMARY_COLUMNS[1].into(), group),
        Series::new(SUMMARY_COLUMNS[2].into(), loans),
        Series::new(SUMMARY_COLUMNS[3].into(), e),
        Series::new(SUMMARY_COLUMNS[4].into(), l),
        Series::new(SUMMARY_COLUMNS[5].into(), p),
        Series::new(SUMMARY_COLUMNS[6].into(), h),
        Series::new(SUMMARY_COLUMNS[7].into(), sf),
    ])?)
}
