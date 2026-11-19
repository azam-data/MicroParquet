//! The Islamic window (Phase 6 P5): the Mudaraba profit distribution as a calculation. The method,
//! the weightages and the profit-sharing ratio (PSR) are the bank's Shariah Supervisory
//! Committee-approved ones: every parameter is read from `portable_data/islamic/psr_<date>.json`
//! (a shipped file is a SAMPLE and says so), never fixed in the program.
//!
//! investable Mudaraba = Σ average balance × (1 − CRR); pool share = investable ÷ (investable +
//! other investable funds); pool income = income × pool share; depositors' share = pool income ×
//! PSR (the Mudarib keeps the rest); product profit = depositors' share × weighted balance ÷
//! Σ weighted balances; indicative rate = profit ÷ average balance × 365 ÷ days × 100.

use crate::rules::{parse_rule_text, RuleFile};
use anyhow::{anyhow, bail, Context, Result};
use chrono::NaiveDate;
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const ISLAMIC_FOLDER: &str = "islamic";
/// The shipped sample: its date and text.
pub const SAMPLE_PSR: (&str, &str) = ("2026-01-01", include_str!("../assets/islamic/psr_2026-01-01.json"));
pub const SAMPLE_PSR_FILE: &str = "psr_2026-01-01.json";

pub fn islamic_dir() -> PathBuf {
    crate::rules::data_dir().join(ISLAMIC_FOLDER)
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Weightage {
    pub product: String,
    pub weight: f64,
}

/// The numbers of a `psr_<date>.json` file.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct PsrFile {
    /// The reserve deducted from the deposits before they are investable, in percent.
    pub crr_pct: f64,
    /// The share of the pool income that goes to the depositors, in percent.
    pub psr_pct: f64,
    pub weightages: Vec<Weightage>,
}

/// A product code compared the way the rest of the app compares codes.
pub fn normalise_product(code: &str) -> String {
    code.split_whitespace().collect::<Vec<_>>().join(" ").to_uppercase()
}

fn check_psr(f: &PsrFile, label: &str) -> Result<()> {
    if !(f.crr_pct.is_finite() && (0.0..=100.0).contains(&f.crr_pct)) {
        bail!("The parameter file {}: crr_pct must be from 0 to 100 (it is {}).", label, f.crr_pct);
    }
    if !(f.psr_pct.is_finite() && (0.0..=100.0).contains(&f.psr_pct)) {
        bail!("The parameter file {}: psr_pct must be from 0 to 100 (it is {}).", label, f.psr_pct);
    }
    if f.weightages.is_empty() {
        bail!("The parameter file {} has no weightages.", label);
    }
    let mut seen: Vec<String> = Vec::new();
    for w in &f.weightages {
        if !(w.weight.is_finite() && w.weight >= 0.0) {
            bail!("The parameter file {}: the weightage of '{}' must be a number of 0 or more (it is {}).", label, w.product, w.weight);
        }
        let k = normalise_product(&w.product);
        if k.is_empty() {
            bail!("The parameter file {} has a weightage with no product code.", label);
        }
        if seen.contains(&k) {
            bail!("The parameter file {} lists '{}' twice (codes are matched ignoring case and extra spaces). Keep one.", label, w.product);
        }
        seen.push(k);
    }
    Ok(())
}

/// `(date, path)` of the `psr_<date>.json` files, oldest first.
fn psr_files() -> Vec<(NaiveDate, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(islamic_dir()) else {
        return Vec::new();
    };
    let mut v: Vec<(NaiveDate, PathBuf)> = rd
        .flatten()
        .map(|e| e.path())
        .filter_map(|p| {
            let stem = p.file_stem()?.to_str()?.strip_prefix("psr_")?.to_string();
            if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("json")) {
                return None;
            }
            Some((NaiveDate::parse_from_str(&stem, "%Y-%m-%d").ok()?, p))
        })
        .collect();
    v.sort_by_key(|(d, _)| *d);
    v
}

/// The latest parameter file dated on or before `end` (the sample is written when the folder has none).
pub fn load_psr(end: NaiveDate) -> Result<RuleFile<PsrFile>> {
    let dir = islamic_dir();
    if psr_files().is_empty() {
        crate::rules::write_text_if_missing(&dir.join(SAMPLE_PSR_FILE), SAMPLE_PSR.1);
    }
    let files = psr_files();
    let Some((d, p)) = files.iter().rev().find(|(d, _)| *d <= end) else {
        match files.first() {
            Some((d, _)) => bail!("There is no PSR file dated on or before {}. The earliest is {}. Add psr_<date>.json to {}.", end.format("%Y-%m-%d"), d.format("%Y-%m-%d"), dir.display()),
            None => bail!("There is no PSR file in {}. Add psr_<YYYY-MM-DD>.json there.", dir.display()),
        }
    };
    let label = format!("{}/{}", ISLAMIC_FOLDER, p.file_name().and_then(|n| n.to_str()).unwrap_or(""));
    let text = std::fs::read_to_string(p).with_context(|| format!("Could not read the parameter file {}", p.display()))?;
    let f: RuleFile<PsrFile> = parse_rule_text(&label, *d, &text)?;
    check_psr(&f.data, &label)?;
    Ok(f)
}

/// What the dialog gives besides the deposit tab.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct MudarabaSpec {
    pub product_col: String,
    /// The average balance of the period, or (with `daily`) a balance for each day.
    pub balance_col: String,
    /// The rows are daily balances: the average is their sum ÷ the days of the period.
    pub daily: bool,
    /// With `daily`: the column holding the day of each balance (a date, or text as YYYY-MM-DD).
    pub date_col: String,
    pub start: String,
    pub end: String,
    /// The investment income of the period.
    pub income: f64,
    /// The bank's own and other non-Mudaraba investable funds.
    pub other_investable: f64,
}

impl MudarabaSpec {
    pub fn dates(&self) -> Result<(NaiveDate, NaiveDate)> {
        let p = |t: &str, what: &str| NaiveDate::parse_from_str(t.trim(), "%Y-%m-%d").map_err(|_| anyhow!("Type the {} date as YYYY-MM-DD (for example 2026-10-31).", what));
        let (s, e) = (p(&self.start, "start")?, p(&self.end, "end")?);
        if e < s {
            bail!("The period ends ({}) before it starts ({}).", e.format("%Y-%m-%d"), s.format("%Y-%m-%d"));
        }
        Ok((s, e))
    }

    pub fn days(&self) -> Result<i64> {
        let (s, e) = self.dates()?;
        Ok((e - s).num_days() + 1)
    }
}

pub const MUDARABA_COLUMNS: [&str; 7] = ["Product", "Average balance", "Weightage", "Weighted balance", "Profit", "Indicative rate % p.a.", "Note"];

/// The distribution: a row for each product, then the footer (pool and parameters).
pub fn build_mudaraba(df: &DataFrame, spec: &MudarabaSpec, params: &RuleFile<PsrFile>) -> Result<DataFrame> {
    let days = spec.days()? as f64;
    for (what, v) in [("investment income", spec.income), ("other investable funds", spec.other_investable)] {
        if !v.is_finite() || v < 0.0 {
            bail!("Mudaraba: the {} must be a number of 0 or more.", what);
        }
    }
    let col = |name: &str, what: &str| df.column(name).map_err(|_| anyhow!("Mudaraba: the {} column '{}' is not in the deposit tab. Choose it again.", what, name));
    let prod = col(&spec.product_col, "product code")?.cast(&DataType::String)?;
    let bal_c = col(&spec.balance_col, "balance")?;
    let dt = bal_c.dtype();
    if !(dt.is_numeric() || matches!(dt, DataType::Decimal(..))) {
        bail!("Mudaraba: the balance column '{}' is {}, not a number. Set its type in Schema & Ingestion.", spec.balance_col, dt);
    }
    let bal = bal_c.cast(&DataType::Float64)?;
    // average balance of each product, in the order the products first appear
    let mut order: Vec<String> = Vec::new();
    let mut avg: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    let mut shown: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let (mut blank_codes, mut negatives) = (0usize, 0usize);
    let (mut blank_bal, mut n_blank_bal) = (Vec::<String>::new(), 0usize);
    // daily balances: the day of each row (checked against the period) and the days each product has
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).context("Internal date error (epoch)")?;
    let day_of: Option<Vec<Option<i32>>> = if spec.daily {
        let dc = spec.date_col.trim();
        if dc.is_empty() {
            bail!("Mudaraba: with daily balances, choose the column that holds the day of each balance.");
        }
        let c = col(dc, "date")?;
        let before = c.null_count();
        let as_date = c.cast(&DataType::Date).map_err(|_| anyhow!("Mudaraba: the date column '{}' is {}, not a date. Dates are YYYY-MM-DD.", dc, c.dtype()))?;
        if as_date.null_count() > before {
            bail!("Mudaraba: {} value(s) of the date column '{}' are not dates (YYYY-MM-DD). Fix them in Schema & Ingestion.", as_date.null_count() - before, dc);
        }
        Some(as_date.cast(&DataType::Int32)?.i32()?.into_iter().collect())
    } else {
        None
    };
    let (p_start, p_end) = spec.dates()?;
    let (d0, d1) = ((p_start - epoch).num_days() as i32, (p_end - epoch).num_days() as i32);
    let mut outside = 0usize;
    let mut days_seen: std::collections::HashMap<String, std::collections::BTreeSet<i32>> = std::collections::HashMap::new();
    for (i, (p, b)) in prod.str()?.into_iter().zip(bal.f64()?.into_iter()).enumerate() {
        let Some(code) = p.map(str::trim).filter(|c| !c.is_empty()) else {
            blank_codes += 1;
            continue;
        };
        let Some(b) = b.filter(|b| b.is_finite()) else {
            n_blank_bal += 1;
            if blank_bal.len() < 5 {
                blank_bal.push(code.to_string());
            }
            continue;
        };
        if let Some(days_col) = &day_of {
            match days_col[i] {
                Some(d) if d >= d0 && d <= d1 => {
                    days_seen.entry(normalise_product(code)).or_default().insert(d);
                }
                _ => {
                    outside += 1;
                    continue;
                }
            }
        }
        if b < 0.0 {
            negatives += 1;
        }
        let k = normalise_product(code);
        if !avg.contains_key(&k) {
            order.push(k.clone());
            shown.insert(k.clone(), code.to_string());
        }
        *avg.entry(k).or_insert(0.0) += b;
    }
    if n_blank_bal > 0 {
        bail!("Mudaraba: {} row(s) have a blank balance in '{}' (first products: {}). Fill them or filter them out; a blank would count as 0.", n_blank_bal, spec.balance_col, blank_bal.join(", "));
    }
    if outside > 0 {
        bail!("Mudaraba: {} row(s) are dated outside the period {} to {} (or have no date) in '{}'. Filter them out or correct the period.", outside, p_start.format("%Y-%m-%d"), p_end.format("%Y-%m-%d"), spec.date_col.trim());
    }
    if spec.daily {
        let miss: Vec<String> = order.iter().filter_map(|k| {
            let have = days_seen.get(k).map_or(0, |s| s.len()) as i64;
            (have < days as i64).then(|| format!("{} ({} of {} days missing)", shown[k], days as i64 - have, days as i64))
        }).collect();
        if !miss.is_empty() {
            bail!("Mudaraba: daily balances are missing for {}. Every product needs a balance for every day of the period; add the missing days (a missing day is not counted as 0).", miss.join("; "));
        }
    }
    if blank_codes > 0 {
        bail!("Mudaraba: {} row(s) have no product code in '{}'. Fill them or filter them out.", blank_codes, spec.product_col);
    }
    if negatives > 0 {
        bail!("Mudaraba: {} row(s) of '{}' are negative. A deposit balance cannot be negative: check the file.", negatives, spec.balance_col);
    }
    if order.is_empty() {
        bail!("Mudaraba: the deposit tab has no balances.");
    }
    if spec.daily {
        for v in avg.values_mut() {
            *v /= days;
        }
    }
    // every product needs a weightage: the run stops and names the ones that have none
    let unmapped: Vec<String> = order.iter().filter(|k| !params.data.weightages.iter().any(|w| normalise_product(&w.product) == **k)).map(|k| format!("{} (balance {})", shown[k], crate::models::format_with_commas(avg[k], 2))).collect();
    if !unmapped.is_empty() {
        bail!("Mudaraba: no weightage for the product(s) {}. Add them to {} and run again; nothing is guessed.", unmapped.join(", "), params.meta.file);
    }
    let weight = |k: &str| params.data.weightages.iter().find(|w| normalise_product(&w.product) == k).map(|w| w.weight).unwrap_or(0.0);
    let total_avg: f64 = order.iter().map(|k| avg[k]).sum();
    let crr = params.data.crr_pct / 100.0;
    let psr = params.data.psr_pct / 100.0;
    let investable = total_avg * (1.0 - crr);
    let denominator = investable + spec.other_investable;
    if denominator <= 0.0 {
        bail!("Mudaraba: the investable funds are 0, so there is no pool to share the income of.");
    }
    let pool_share = investable / denominator;
    let pool_income = spec.income * pool_share;
    let depositors = pool_income * psr;
    let mudarib = pool_income - depositors;
    let weighted: Vec<f64> = order.iter().map(|k| avg[k] * weight(k)).collect();
    let sum_weighted: f64 = weighted.iter().sum();
    if sum_weighted <= 0.0 {
        bail!("Mudaraba: the weighted balances add up to 0 (every weightage is 0 or every balance is), so the depositors' share cannot be divided.");
    }
    let profit: Vec<f64> = weighted.iter().map(|w| depositors * w / sum_weighted).collect();
    let rate: Vec<Option<f64>> = order.iter().zip(&profit).map(|(k, p)| if avg[k] > 0.0 { Some(p / avg[k] * 365.0 / days * 100.0) } else { None }).collect();

    // the table: products, then a blank line and the footer
    let mut name: Vec<Option<String>> = order.iter().map(|k| Some(shown[k].clone())).collect();
    let mut a: Vec<Option<f64>> = order.iter().map(|k| Some(avg[k])).collect();
    let mut w: Vec<Option<f64>> = order.iter().map(|k| Some(weight(k))).collect();
    let mut wb: Vec<Option<f64>> = weighted.iter().map(|x| Some(*x)).collect();
    let mut pr: Vec<Option<f64>> = profit.iter().map(|x| Some(*x)).collect();
    let mut rt: Vec<Option<f64>> = rate;
    let mut note: Vec<Option<String>> = vec![None; order.len()];
    let mut push = |label: &str, avg_v: Option<f64>, wb_v: Option<f64>, profit_v: Option<f64>, rate_v: Option<f64>, n: Option<String>| {
        name.push(Some(label.to_string()));
        a.push(avg_v);
        w.push(None);
        wb.push(wb_v);
        pr.push(profit_v);
        rt.push(rate_v);
        note.push(n);
    };
    let blended = if total_avg > 0.0 { Some(depositors / total_avg * 365.0 / days * 100.0) } else { None };
    push("Total", Some(total_avg), Some(sum_weighted), Some(depositors), blended, None);
    push("Investable Mudaraba funds", Some(investable), None, None, None, Some(format!("Average balances \u{d7} (1 \u{2212} {}% CRR)", params.data.crr_pct)));
    push("Other investable funds", Some(spec.other_investable), None, None, None, None);
    push("Pool share (%)", None, None, None, Some(pool_share * 100.0), None);
    push("Investment income", None, None, Some(spec.income), None, None);
    push("Pool income", None, None, Some(pool_income), None, None);
    push("Depositors' share", None, None, Some(depositors), Some(params.data.psr_pct), Some("Pool income \u{d7} the depositors' PSR (the rate column holds the PSR %)".to_string()));
    push("Mudarib share", None, None, Some(mudarib), Some(100.0 - params.data.psr_pct), Some("The rest of the pool income".to_string()));
    push("Period", None, None, None, None, Some(format!("{} to {}, {} day(s)", spec.start.trim(), spec.end.trim(), days as i64)));
    push("Parameter file", None, None, None, None, Some(format!("{}, as of {}, source: {}", params.meta.file, params.meta.as_of.format("%Y-%m-%d"), params.meta.source)));
    if params.meta.sample {
        push("Sample parameters", None, None, None, None, Some("The parameter file is marked sample: the weightages and the PSR must be the bank's Shariah Supervisory Committee-approved ones before these figures are used.".to_string()));
    }
    Ok(DataFrame::new(vec![
        Series::new("Product".into(), name),
        Series::new("Average balance".into(), a),
        Series::new("Weightage".into(), w),
        Series::new("Weighted balance".into(), wb),
        Series::new("Profit".into(), pr),
        Series::new("Indicative rate % p.a.".into(), rt),
        Series::new("Note".into(), note),
    ])?)
}
