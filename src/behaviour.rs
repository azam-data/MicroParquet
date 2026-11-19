//! Behaviour profiles (Phase 5 R2): the core part and average maturity of non-maturity deposits,
//! and the prepayment (CPR) and early-redemption (TDRR) rates of loans and term deposits, saved as
//! `portable_data/behaviour/<YYYY-MM-DD>.json` and read by the cash-flow engine, the gap ladder and
//! the IRRBB tool.

use crate::rules::{parse_rule_text, RuleFile};
use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// One product of a profile.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct BehaviourProduct {
    pub product: String,
    /// "Retail transactional", "Retail non-transactional" or "Wholesale"; blank for a product that
    /// only has a CPR or a TDRR (it is not treated as a non-maturity deposit).
    #[serde(default)]
    pub category: String,
    /// The core share of the balance, in percent.
    #[serde(default)]
    pub core_pct: f64,
    #[serde(default)]
    pub avg_maturity_years: f64,
    /// "Historical minimum", "Volatility" or "Typed".
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub lookback_months: u32,
    /// The computed core % was above the cap and was cut to it.
    #[serde(default)]
    pub capped: bool,
    /// Annual prepayment rate of a loan, in percent.
    #[serde(default)]
    pub cpr_pct: f64,
    /// Early redemption rate of a term deposit, in percent.
    #[serde(default)]
    pub tdrr_pct: f64,
}

impl BehaviourProduct {
    /// True for a non-maturity deposit (it has a category).
    pub fn is_nmd(&self) -> bool {
        !self.category.trim().is_empty()
    }
}

/// The JSON of a behaviour file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct BehaviourFile {
    #[serde(default)]
    pub as_of: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub products: Vec<BehaviourProduct>,
}

/// A product code compared the way the rest of the app compares codes: trimmed, spaces folded, upper case.
pub fn normalise_product(code: &str) -> String {
    code.split_whitespace().collect::<Vec<_>>().join(" ").to_uppercase()
}

impl BehaviourFile {
    /// The profile of a product code, if it has one.
    pub fn find(&self, product: &str) -> Option<&BehaviourProduct> {
        let want = normalise_product(product);
        self.products.iter().find(|p| normalise_product(&p.product) == want)
    }

    /// Checks the numbers of every product.
    pub fn validate(&self, label: &str) -> Result<()> {
        let mut seen: Vec<String> = Vec::new();
        for p in &self.products {
            let name = normalise_product(&p.product);
            if name.is_empty() {
                bail!("The behaviour file {} has a product with no code.", label);
            }
            if seen.contains(&name) {
                bail!("The behaviour file {} lists the product '{}' twice. Keep one.", label, p.product);
            }
            seen.push(name);
            for (what, v) in [("core_pct", p.core_pct), ("cpr_pct", p.cpr_pct), ("tdrr_pct", p.tdrr_pct)] {
                if !(v.is_finite() && (0.0..=100.0).contains(&v)) {
                    bail!("The behaviour file {}: {} of '{}' must be a number from 0 to 100.", label, what, p.product);
                }
            }
            if !(p.avg_maturity_years.is_finite() && p.avg_maturity_years >= 0.0) {
                bail!("The behaviour file {}: avg_maturity_years of '{}' must be 0 or more.", label, p.product);
            }
            if p.is_nmd() && p.avg_maturity_years <= 0.0 {
                bail!("The behaviour file {}: the non-maturity product '{}' needs an average maturity above 0 years.", label, p.product);
            }
        }
        Ok(())
    }
}

/// The names of the profile files in `dir` (`YYYY-MM-DD.json`), newest first.
pub fn list_behaviour_files(dir: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = rd
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| n.to_lowercase().ends_with(".json") && NaiveDate::parse_from_str(n.trim_end_matches(".json").trim_end_matches(".JSON"), "%Y-%m-%d").is_ok())
        .collect();
    names.sort();
    names.reverse();
    names
}

/// Reads the profile file called `name` (for example `2026-10-31.json`) from `dir`.
pub fn load_behaviour(dir: &Path, name: &str) -> Result<RuleFile<BehaviourFile>> {
    let stem = name.trim().trim_end_matches(".json");
    let date = NaiveDate::parse_from_str(stem, "%Y-%m-%d").map_err(|_| anyhow::anyhow!("'{}' is not a behaviour file name. Use a name like 2026-10-31.json.", name))?;
    let path = dir.join(format!("{stem}.json"));
    let text = std::fs::read_to_string(&path).with_context(|| format!("Could not read the behaviour file {}", path.display()))?;
    let f: RuleFile<BehaviourFile> = parse_rule_text(&format!("behaviour/{stem}.json"), date, &text)?;
    f.data.validate(&f.meta.file)?;
    crate::rules::note_used_file("Behaviour profile", &f.meta.file, &f.data.as_of, &f.data.source, f.meta.sample);
    // a file can be edited by hand: check it against the Bangladesh Bank caps again, as the dialog did
    let raw: serde_json::Value = serde_json::from_str(&text).with_context(|| format!("The behaviour file {} is not valid JSON.", f.meta.file))?;
    let raw_products = raw.get("products").and_then(|p| p.as_array()).cloned().unwrap_or_default();
    let caps = crate::rules::load_nmd_caps(date)?;
    for (p, r) in f.data.products.iter().zip(raw_products.iter()) {
        if !p.is_nmd() {
            continue;
        }
        if r.get("core_pct").map_or(true, |v| v.is_null()) {
            bail!("The behaviour file {}: the non-maturity product '{}' has no core_pct. Type the core share in percent.", f.meta.file, p.product);
        }
        let cap = caps.data.cap(&p.category).ok_or_else(|| {
            anyhow::anyhow!("The behaviour file {}: '{}' has the category '{}', which is not a category of {}. Choose one of: {}.", f.meta.file, p.product, p.category, caps.meta.file, caps.data.categories.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "))
        })?;
        if p.core_pct > cap.core_cap_pct {
            bail!("The behaviour file {}: '{}' has a core share of {}%, above the cap of {}% for {} ({}).", f.meta.file, p.product, p.core_pct, cap.core_cap_pct, cap.name, caps.meta.file);
        }
        if p.avg_maturity_years > cap.max_avg_maturity_years {
            bail!("The behaviour file {}: '{}' has an average maturity of {} years, above the cap of {} years for {} ({}).", f.meta.file, p.product, p.avg_maturity_years, cap.max_avg_maturity_years, cap.name, caps.meta.file);
        }
    }
    Ok(f)
}

/// The latest profile file dated on or before `cutoff`, or `None` when the folder has none.
pub fn latest_behaviour(dir: &Path, cutoff: NaiveDate) -> Result<Option<RuleFile<BehaviourFile>>> {
    for n in list_behaviour_files(dir) {
        let d = NaiveDate::parse_from_str(n.trim_end_matches(".json"), "%Y-%m-%d");
        if d.is_ok_and(|d| d <= cutoff) {
            return load_behaviour(dir, &n).map(Some);
        }
    }
    Ok(None)
}

// ------------------------------------------------------------------------------ the calculation

use crate::rules::{NmdCapsFile, NmdCap};
use chrono::Datelike;

/// z of the volatility method (BB IRRBB guideline §15.13 style: 2.33 is the 99% one-sided value).
pub const DEFAULT_Z: f64 = 2.33;
/// The guideline's history: 10 years of monthly volumes.
pub const FULL_HISTORY_MONTHS: u32 = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CoreMethod {
    #[default]
    HistoricalMinimum,
    Volatility,
    /// The core % is typed by the user.
    Typed,
}

impl CoreMethod {
    pub const ALL: [CoreMethod; 3] = [CoreMethod::HistoricalMinimum, CoreMethod::Volatility, CoreMethod::Typed];
    pub fn label(self) -> &'static str {
        match self {
            CoreMethod::HistoricalMinimum => "Historical minimum",
            CoreMethod::Volatility => "Volatility",
            CoreMethod::Typed => "Typed",
        }
    }
}

fn month_key(d: NaiveDate) -> i32 {
    d.year() * 12 + d.month0() as i32
}

/// The balance of each calendar month: the last date of the month that has one. `points` are
/// (date, balance) with one balance per date; the result is sorted by date. Monthly data passes
/// through; daily data is reduced to month-end.
pub fn month_end_balances(points: &[(NaiveDate, f64)]) -> Vec<(NaiveDate, f64)> {
    let mut sorted = points.to_vec();
    sorted.sort_by_key(|p| p.0);
    let mut out: Vec<(NaiveDate, f64)> = Vec::new();
    for p in sorted {
        match out.last_mut() {
            Some(last) if month_key(last.0) == month_key(p.0) => *last = p,
            _ => out.push(p),
        }
    }
    out
}

/// The last `lookback` months of a month-end series, which must have a balance for every month
/// in between (a missing month would make the changes look larger than they were).
pub fn lookback_window<'a>(product: &str, series: &'a [(NaiveDate, f64)], lookback: u32) -> Result<&'a [(NaiveDate, f64)]> {
    let Some(last) = series.last() else {
        bail!("'{}' has no balances in the history.", product);
    };
    let first_key = month_key(last.0) - (lookback.max(1) as i32 - 1);
    let start = series.iter().position(|p| month_key(p.0) >= first_key).unwrap_or(series.len() - 1);
    let w = &series[start..];
    for pair in w.windows(2) {
        if month_key(pair[1].0) - month_key(pair[0].0) != 1 {
            let missing = month_key(pair[0].0) + 1;
            bail!(
                "'{}' has no balance for {}-{:02}: the month-ends must be continuous. Fill the month in the history, or shorten the look-back to start after it.",
                product,
                missing.div_euclid(12),
                missing.rem_euclid(12) + 1
            );
        }
    }
    if let Some(bad) = w.iter().find(|p| !p.1.is_finite() || p.1 < 0.0) {
        bail!("'{}' has a negative or non-numeric balance on {}. Balances must be positive amounts.", product, bad.0.format("%Y-%m-%d"));
    }
    Ok(w)
}

/// Historical minimum: min(balance over the look-back) ÷ latest balance × 100.
pub fn core_historical_minimum(product: &str, balances: &[f64]) -> Result<f64> {
    let Some(latest) = balances.last().copied() else {
        bail!("'{}' has no balances in the look-back.", product);
    };
    if !(latest > 0.0) {
        bail!("'{}': the latest balance is 0, so the core share cannot be worked out.", product);
    }
    let min = balances.iter().copied().fold(f64::INFINITY, f64::min);
    Ok(min / latest * 100.0)
}

/// The month-on-month changes (b_t − b_t−1) ÷ b_t−1 of a series, as fractions.
pub fn month_on_month_changes(product: &str, balances: &[f64]) -> Result<Vec<f64>> {
    let mut out = Vec::with_capacity(balances.len().saturating_sub(1));
    for w in balances.windows(2) {
        if !(w[0] > 0.0) {
            bail!("'{}' has a month with a zero balance, so the next month's change cannot be worked out.", product);
        }
        out.push((w[1] - w[0]) / w[0]);
    }
    Ok(out)
}

/// The sample standard deviation (n − 1); `None` for fewer than two values.
pub fn sample_std(xs: &[f64]) -> Option<f64> {
    if xs.len() < 2 {
        return None;
    }
    let n = xs.len() as f64;
    let mean = xs.iter().sum::<f64>() / n;
    Some((xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt())
}

/// Volatility: core % = (1 − z × σ) × 100, floored at 0, with σ the sample standard deviation of
/// the month-on-month changes (as fractions). Returns σ and the core %.
pub fn core_volatility(product: &str, balances: &[f64], z: f64) -> Result<(f64, f64)> {
    if !(z.is_finite() && z >= 0.0) {
        bail!("z must be a number that is not negative (the guideline's 99% value is {}).", DEFAULT_Z);
    }
    let changes = month_on_month_changes(product, balances)?;
    let Some(sigma) = sample_std(&changes) else {
        bail!("'{}' needs at least 3 month-ends for the volatility method; it has {}.", product, balances.len());
    };
    Ok((sigma, ((1.0 - z * sigma) * 100.0).max(0.0)))
}

/// What the user typed for one product.
#[derive(Clone, Debug, Default)]
pub struct RowInput {
    pub product: String,
    pub category: String,
    pub avg_maturity_years: Option<f64>,
    pub method: CoreMethod,
    pub typed_core_pct: Option<f64>,
    pub cpr_pct: f64,
    pub tdrr_pct: f64,
}

/// One product's result.
#[derive(Clone, Debug, PartialEq)]
pub struct RowOutcome {
    pub product: BehaviourProduct,
    /// The core % before the cap (the computed one), when there is one.
    pub computed_core_pct: Option<f64>,
    pub sigma: Option<f64>,
    pub months_used: u32,
}

fn cap_note(caps: &NmdCap, meta: &crate::rules::RuleMeta) -> String {
    format!("{}: core cap {}%, average maturity cap {} years; source: {}", meta.file, caps.core_cap_pct, caps.max_avg_maturity_years, meta.source)
}

/// Builds one product of the profile.
///
/// * A product with a category is a non-maturity deposit: it needs an average maturity (typed,
///   above 0, not above the cap of its category) and a core %: computed from `series` by the
///   method, or typed (not above the cap). A computed core above the cap is cut to it and marked capped.
/// * A product with no category only has a CPR and a TDRR (a loan or a term deposit).
pub fn build_product(input: &RowInput, series: Option<&[(NaiveDate, f64)]>, lookback: u32, z: f64, caps: &crate::rules::RuleFile<NmdCapsFile>) -> Result<RowOutcome> {
    let name = input.product.trim();
    if name.is_empty() {
        bail!("A product has no code.");
    }
    for (what, v) in [("CPR", input.cpr_pct), ("TDRR", input.tdrr_pct)] {
        if !(v.is_finite() && (0.0..=100.0).contains(&v)) {
            bail!("'{}': {} must be a number from 0 to 100.", name, what);
        }
    }
    let mut p = BehaviourProduct { product: name.to_string(), cpr_pct: input.cpr_pct, tdrr_pct: input.tdrr_pct, ..BehaviourProduct::default() };
    let category = input.category.trim();
    if category.is_empty() {
        if series.is_some() {
            bail!("'{}': choose a category (Retail transactional, Retail non-transactional or Wholesale).", name);
        }
        return Ok(RowOutcome { product: p, computed_core_pct: None, sigma: None, months_used: 0 });
    }
    let cap = caps.data.cap(category).ok_or_else(|| {
        anyhow::anyhow!("'{}': '{}' is not a category of {}. Choose one of: {}.", name, category, caps.meta.file, caps.data.categories.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "))
    })?;
    p.category = cap.name.clone();
    let years = input.avg_maturity_years.ok_or_else(|| anyhow::anyhow!("'{}': type the average maturity in years.", name))?;
    if !(years.is_finite() && years > 0.0) {
        bail!("'{}': the average maturity must be a number above 0 years.", name);
    }
    if years > cap.max_avg_maturity_years {
        bail!("'{}': an average maturity of {} years is above the Bangladesh Bank cap of {} years for {} ({}). Type {} or less.", name, years, cap.max_avg_maturity_years, cap.name, cap_note(cap, &caps.meta), cap.max_avg_maturity_years);
    }
    p.avg_maturity_years = years;
    p.method = input.method.label().to_string();
    let (mut core, sigma, months, computed) = match input.method {
        CoreMethod::Typed => {
            let c = input.typed_core_pct.ok_or_else(|| anyhow::anyhow!("'{}': type the core share in percent.", name))?;
            if !(c.is_finite() && (0.0..=100.0).contains(&c)) {
                bail!("'{}': the core share must be a number from 0 to 100.", name);
            }
            if c > cap.core_cap_pct {
                bail!("'{}': a core share of {}% is above the Bangladesh Bank cap of {}% for {} ({}). Type {} or less.", name, c, cap.core_cap_pct, cap.name, cap_note(cap, &caps.meta), cap.core_cap_pct);
            }
            (c, None, 0u32, None)
        }
        m => {
            let series = series.ok_or_else(|| anyhow::anyhow!("'{}': there is no history for this product. Read a history that has it, or choose the method Typed.", name))?;
            let w = lookback_window(name, series, lookback)?;
            let balances: Vec<f64> = w.iter().map(|p| p.1).collect();
            let months = balances.len() as u32;
            if m == CoreMethod::HistoricalMinimum {
                let c = core_historical_minimum(name, &balances)?;
                (c, None, months, Some(c))
            } else {
                let (s, c) = core_volatility(name, &balances, z)?;
                (c, Some(s), months, Some(c))
            }
        }
    };
    if core > cap.core_cap_pct {
        core = cap.core_cap_pct;
        p.capped = true;
    }
    p.core_pct = core;
    p.lookback_months = months;
    Ok(RowOutcome { product: p, computed_core_pct: computed, sigma, months_used: months })
}

/// The short-history warning for a profile built on `months` months (none at 120 or more).
pub fn short_history_warning(months: u32) -> Option<String> {
    (months < FULL_HISTORY_MONTHS).then(|| format!("BB IRRBB guideline §15.13 uses 10 years of volume history; this profile uses {} months.", months))
}

/// Writes a profile to `dir/<as_of>.json` and returns the path. `as_of` must be the file's date.
pub fn save_behaviour(dir: &Path, file: &BehaviourFile) -> Result<std::path::PathBuf> {
    let date = NaiveDate::parse_from_str(file.as_of.trim(), "%Y-%m-%d").map_err(|_| anyhow::anyhow!("Type the profile date as YYYY-MM-DD (for example 2026-10-31)."))?;
    if file.source.trim().is_empty() {
        bail!("Type where the profile comes from (for example the ALM team's study and its period).");
    }
    file.validate("being saved")?;
    std::fs::create_dir_all(dir).with_context(|| format!("Could not create the folder {}", dir.display()))?;
    let path = dir.join(format!("{}.json", date.format("%Y-%m-%d")));
    let text = serde_json::to_string_pretty(file)?;
    std::fs::write(&path, text).with_context(|| format!("Could not write the behaviour file {}", path.display()))?;
    Ok(path)
}

/// The (product, date, balance) rows of a table, summed per product and date. Blank or
/// unparseable dates, products or balances stop with the count and the first 5 values.
pub fn history_from_lazy(lf: polars::prelude::LazyFrame, date_col: &str, product_col: &str, balance_col: &str) -> Result<Vec<(String, NaiveDate, f64)>> {
    use polars::prelude::*;
    let mut lf = lf;
    let schema = lf.collect_schema().context("Could not read the columns of the history tab")?;
    for (c, what) in [(date_col, "date"), (product_col, "product"), (balance_col, "balance")] {
        if schema.get(c).is_none() {
            bail!("The history tab has no column '{}'. Choose the {} column again.", c, what);
        }
    }
    let dt = schema.get(balance_col).cloned().unwrap_or(DataType::Null);
    if !dt.is_numeric() {
        bail!("The balance column '{}' is {}, not a number. Set it to a number in the schema first.", balance_col, dt);
    }
    let lf = crate::banking::ensure_date(lf, date_col).map_err(|e| anyhow::anyhow!("The history's date column: {:#}", e))?;
    let df = lf
        .select([col(date_col).cast(DataType::Int32).alias("__d"), col(product_col).cast(DataType::String).alias("__p"), col(balance_col).cast(DataType::Float64).alias("__b")])
        .collect()
        .context("Could not read the history")?;
    let d: Vec<Option<i32>> = df.column("__d")?.i32()?.into_iter().collect();
    let p: Vec<Option<String>> = df.column("__p")?.str()?.into_iter().map(|v| v.map(str::to_string)).collect();
    let b: Vec<Option<f64>> = df.column("__b")?.f64()?.into_iter().collect();
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).ok_or_else(|| anyhow::anyhow!("calendar"))?;
    let mut bad: Vec<(&str, usize)> = Vec::new();
    let mut first: Vec<String> = Vec::new();
    let mut sums: std::collections::BTreeMap<(String, NaiveDate), f64> = std::collections::BTreeMap::new();
    for i in 0..d.len() {
        let code = p[i].as_deref().map(normalise_product).unwrap_or_default();
        let ok = d[i].is_some() && !code.is_empty() && b[i].is_some_and(|v| v.is_finite());
        if !ok {
            if first.len() < 5 {
                first.push(format!("row {}", i + 1));
            }
            bad.push(("", i));
            continue;
        }
        let date = epoch + chrono::Duration::days(d[i].unwrap_or(0) as i64);
        *sums.entry((code, date)).or_insert(0.0) += b[i].unwrap_or(0.0);
    }
    if !bad.is_empty() {
        bail!("History: {} row(s) have a blank date, product or balance in '{}', '{}' or '{}'; the first {}: {}. Fix or filter those rows first.", bad.len(), date_col, product_col, balance_col, first.len(), first.join(", "));
    }
    Ok(sums.into_iter().map(|((p, d), v)| (p, d, v)).collect())
}

/// The month-end series of each product of a history, products in A–Z order.
pub fn series_by_product(rows: &[(String, NaiveDate, f64)]) -> Vec<(String, Vec<(NaiveDate, f64)>)> {
    let mut by: std::collections::BTreeMap<String, Vec<(NaiveDate, f64)>> = std::collections::BTreeMap::new();
    for (p, d, v) in rows {
        by.entry(p.clone()).or_default().push((*d, *v));
    }
    by.into_iter().map(|(p, pts)| (p, month_end_balances(&pts))).collect()
}
