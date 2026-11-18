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
