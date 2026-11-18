//! Dated, sourced rule and parameter files (Phase 5): `portable_data/rules/<kind>/<YYYY-MM-DD>.json`
//! (and `portable_data/behaviour/<YYYY-MM-DD>.json`). Every file has `as_of` (the file's date) and
//! `source`; a sample the owner must confirm also has `"sample": true`. A run uses the latest file
//! dated on or before its cutoff date and records which one it used.
//!
//! The files the circulars fix (IRRBB shocks and non-maturity deposit caps, and later the loan
//! overdue thresholds and IFRS 9 staging) are shipped as source copies under `assets/rules/`, built
//! in with `include_str!`, and written to `portable_data/rules/<kind>/` only when that file is
//! missing; an existing file is never overwritten (a bank may have edited it). A bank adds a newer
//! dated file next to them when Bangladesh Bank revises a number.

use anyhow::{anyhow, bail, Context, Result};
use chrono::NaiveDate;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

thread_local! {
    static RULES_DIR_OVERRIDE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Makes this thread read `portable_data/rules/<kind>` and `portable_data/behaviour` from `dir`
/// instead (tests, headless runs). `dir` plays the role of `portable_data`.
pub fn set_data_dir_override(dir: Option<PathBuf>) {
    RULES_DIR_OVERRIDE.with(|d| *d.borrow_mut() = dir);
}

fn data_root() -> PathBuf {
    RULES_DIR_OVERRIDE.with(|d| d.borrow().clone()).unwrap_or_else(crate::models::portable_root_dir)
}

/// `portable_data/rules/<kind>`.
pub fn rules_dir(kind: &str) -> PathBuf {
    data_root().join("rules").join(kind)
}

/// `portable_data/behaviour`.
pub fn behaviour_dir() -> PathBuf {
    data_root().join("behaviour")
}

/// The file that was used, for the output ("which rule file was used").
#[derive(Clone, Debug, PartialEq)]
pub struct RuleMeta {
    /// `<kind>/<YYYY-MM-DD>.json`, e.g. `irrbb_shocks/2021-04-13.json`.
    pub file: String,
    pub as_of: NaiveDate,
    pub source: String,
    pub sample: bool,
}

/// A parsed rule file and where it came from.
#[derive(Clone, Debug)]
pub struct RuleFile<T> {
    pub data: T,
    pub meta: RuleMeta,
}

/// The built-in default of a kind: its date and JSON text.
pub type BuiltIn = (&'static str, &'static str);

/// The built-in IRRBB shocks (BB Table 2).
pub const IRRBB_SHOCKS: BuiltIn = ("2021-04-13", include_str!("../assets/rules/irrbb_shocks/2021-04-13.json"));
/// The built-in non-maturity deposit caps (BB Table 4).
pub const IRRBB_NMD_CAPS: BuiltIn = ("2021-04-13", include_str!("../assets/rules/irrbb_nmd_caps/2021-04-13.json"));

/// Every shipped rule file: its folder under `portable_data/rules/` and its built-in copy. The tests
/// go through this list, so a file added here is checked against `assets/rules/` and for the
/// write-only-if-missing rule.
pub const SHIPPED_RULES: &[(&str, BuiltIn)] = &[("irrbb_shocks", IRRBB_SHOCKS), ("irrbb_nmd_caps", IRRBB_NMD_CAPS)];

/// Writes the built-in file into `dir` when `dir` does not have it. An existing file is never
/// touched, and the check and the write are one step (`create_new`), so two runs at once cannot
/// overwrite each other either. A failed write is not an error: the built-in copy is still used.
pub fn write_builtin_if_missing(dir: &Path, builtin: BuiltIn) {
    use std::io::Write;
    let (date, text) = builtin;
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).create_new(true).open(dir.join(format!("{date}.json"))) {
        let _ = f.write_all(text.as_bytes());
    }
}

fn parse_date(text: &str, what: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").map_err(|_| anyhow!("{} must be a date written YYYY-MM-DD, not '{}'.", what, text))
}

/// Reads one rule file's text: the common fields are checked and the rest is read as `T`.
pub fn parse_rule_text<T: DeserializeOwned>(label: &str, file_date: NaiveDate, text: &str) -> Result<RuleFile<T>> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| anyhow!("The rule file {} is not valid JSON: {}.", label, e))?;
    let as_of = value.get("as_of").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("The rule file {} has no \"as_of\" date.", label))?;
    let as_of = parse_date(as_of, &format!("\"as_of\" in the rule file {}", label))?;
    if as_of != file_date {
        bail!("The rule file {} says as_of {} but its name says {}. Make them the same.", label, as_of.format("%Y-%m-%d"), file_date.format("%Y-%m-%d"));
    }
    let source = value.get("source").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("The rule file {} has no \"source\" (the circular or paper the numbers come from).", label))?;
    let sample = value.get("sample").and_then(|v| v.as_bool()).unwrap_or(false);
    let data: T = serde_json::from_value(value.clone()).map_err(|e| anyhow!("The rule file {} could not be read: {}.", label, e))?;
    Ok(RuleFile { data, meta: RuleMeta { file: label.to_string(), as_of, source: source.to_string(), sample } })
}

/// The dated files of `dir` (name date, path), oldest first. A missing folder has none.
fn dated_files(dir: &Path) -> Vec<(NaiveDate, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<(NaiveDate, PathBuf)> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("json")))
        .filter_map(|p| {
            let d = NaiveDate::parse_from_str(p.file_stem()?.to_str()?, "%Y-%m-%d").ok()?;
            Some((d, p))
        })
        .collect();
    v.sort();
    v
}

/// The latest file of `dir` dated on or before `cutoff` (`label_dir` is the folder name shown in
/// messages). With a built-in default, it is used when the folder has no file that qualifies, and
/// it is written to the folder when the folder lacks it (a failed write is not an error).
pub fn load_latest<T: DeserializeOwned>(dir: &Path, label_dir: &str, cutoff: NaiveDate, builtin: Option<BuiltIn>) -> Result<RuleFile<T>> {
    if let Some(b) = builtin {
        write_builtin_if_missing(dir, b);
    }
    let files = dated_files(dir);
    if let Some((d, p)) = files.iter().rev().find(|(d, _)| *d <= cutoff) {
        let label = format!("{}/{}", label_dir, p.file_name().and_then(|n| n.to_str()).unwrap_or(""));
        let text = std::fs::read_to_string(p).with_context(|| format!("Could not read the rule file {}", p.display()))?;
        return parse_rule_text(&label, *d, &text);
    }
    if let Some((date, text)) = builtin {
        let d = parse_date(date, "the built-in rule date")?;
        if d <= cutoff {
            return parse_rule_text(&format!("{}/{}.json (built in)", label_dir, date), d, text);
        }
    }
    match files.first() {
        Some((d, _)) => bail!(
            "There is no {} file dated on or before the cutoff {}. The earliest is {}. Add a file dated on or before the cutoff in {}.",
            label_dir,
            cutoff.format("%Y-%m-%d"),
            d.format("%Y-%m-%d"),
            dir.display()
        ),
        None => bail!("There is no {} file in {}. Add a file named <YYYY-MM-DD>.json dated on or before the cutoff {}.", label_dir, dir.display(), cutoff.format("%Y-%m-%d")),
    }
}

/// The IRRBB shocks of the rule file in force at `cutoff` (BB Table 2).
#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
pub struct ShockFile {
    #[serde(default = "default_decay")]
    pub decay_x: f64,
    #[serde(default = "default_outlier")]
    pub outlier_pct: f64,
    pub currencies: Vec<CurrencyShock>,
}

fn default_decay() -> f64 {
    4.0
}
fn default_outlier() -> f64 {
    15.0
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
pub struct CurrencyShock {
    pub currency: String,
    pub parallel_bp: f64,
    pub short_bp: f64,
    pub long_bp: f64,
}

pub fn load_shocks(cutoff: NaiveDate) -> Result<RuleFile<ShockFile>> {
    let f: RuleFile<ShockFile> = load_latest(&rules_dir("irrbb_shocks"), "irrbb_shocks", cutoff, Some(IRRBB_SHOCKS))?;
    check_shocks(&f.data, &f.meta.file)?;
    Ok(f)
}

/// A shock file must have finite sizes, a positive decay and one entry per currency.
pub fn check_shocks(s: &ShockFile, label: &str) -> Result<()> {
    if !(s.decay_x.is_finite() && s.decay_x > 0.0) {
        bail!("The rule file {} needs a decay_x above 0 (the guideline uses 4).", label);
    }
    if !(s.outlier_pct.is_finite() && s.outlier_pct > 0.0) {
        bail!("The rule file {} needs an outlier_pct above 0 (the guideline uses 15).", label);
    }
    let mut seen: Vec<String> = Vec::new();
    for c in &s.currencies {
        let code = c.currency.trim().to_uppercase();
        if code.is_empty() {
            bail!("The rule file {} has a shock entry with no currency.", label);
        }
        if seen.contains(&code) {
            bail!("The rule file {} has two shock entries for {}. Keep one.", label, code);
        }
        seen.push(code.clone());
        for (n, v) in [("parallel_bp", c.parallel_bp), ("short_bp", c.short_bp), ("long_bp", c.long_bp)] {
            if !v.is_finite() {
                bail!("The rule file {} has no valid number for {} of {}.", label, n, code);
            }
        }
    }
    Ok(())
}

impl ShockFile {
    /// The shocks of a currency (compared ignoring case and spaces); a currency the file does not
    /// list is an error naming it and the file.
    pub fn for_currency(&self, currency: &str, label: &str) -> Result<&CurrencyShock> {
        let want = currency.trim().to_uppercase();
        self.currencies.iter().find(|c| c.currency.trim().to_uppercase() == want).ok_or_else(|| {
            anyhow!(
                "The IRRBB shock file {} has no shocks for {}. Add an entry for {} (Bangladesh Bank sets the shock sizes per currency).",
                label,
                want,
                want
            )
        })
    }
}

/// The caps of the non-maturity deposit categories (BB Table 4).
#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
pub struct NmdCapsFile {
    pub categories: Vec<NmdCap>,
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
pub struct NmdCap {
    pub name: String,
    pub core_cap_pct: f64,
    pub max_avg_maturity_years: f64,
}

pub fn load_nmd_caps(on: NaiveDate) -> Result<RuleFile<NmdCapsFile>> {
    let f: RuleFile<NmdCapsFile> = load_latest(&rules_dir("irrbb_nmd_caps"), "irrbb_nmd_caps", on, Some(IRRBB_NMD_CAPS))?;
    for c in &f.data.categories {
        if c.name.trim().is_empty() || !(c.core_cap_pct.is_finite() && (0.0..=100.0).contains(&c.core_cap_pct)) || !(c.max_avg_maturity_years.is_finite() && c.max_avg_maturity_years > 0.0) {
            bail!("The rule file {} has a category with no name, a core cap outside 0–100 or a maturity cap that is not above 0.", f.meta.file);
        }
    }
    Ok(f)
}

impl NmdCapsFile {
    pub fn cap(&self, category: &str) -> Option<&NmdCap> {
        let want = category.trim().to_lowercase();
        self.categories.iter().find(|c| c.name.trim().to_lowercase() == want)
    }
}
