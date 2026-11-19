//! LCR and NSFR from GL balances (Phase 5 R6).
//!
//! A GL trial balance (GL code, balance, and for NSFR lines that depend on it a residual
//! maturity) is mapped through a liquidity map file, `portable_data/liquidity_maps/<name>.json`,
//! to the lines of the two ratios. The map holds the lines (kind and factor), the GL code rules
//! (exact, prefix or range), the caps and the minimums, with its `source` and `as_of`. The map
//! that ships is a SAMPLE with Basel factors; the owner replaces it with the factors of DOS
//! Circular No. 01 of 1 January 2015 and the NSFR lines of SDAD Circular Letter No. 11 of
//! 22 July 2026. Sample factors are never presented as Bangladesh Bank's.

use crate::problems::Problems;
use anyhow::{anyhow, bail, Context, Result};
use polars::prelude::*;
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const MAPS_FOLDER: &str = "liquidity_maps";
pub const SAMPLE_MAP_FILE: &str = "sample_basel.json";
/// The shipped sample (source copy: `assets/liquidity_maps/sample_basel.json`).
pub const SAMPLE_MAP: &str = include_str!("../assets/liquidity_maps/sample_basel.json");

/// The folder of the map files.
pub fn maps_dir() -> PathBuf {
    crate::rules::data_dir().join(MAPS_FOLDER)
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct LiqMap {
    pub as_of: String,
    pub source: String,
    #[serde(default)]
    pub sample: bool,
    pub lines: Vec<LiqLine>,
    pub gl_map: Vec<GlEntry>,
    pub caps: LiqCaps,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct LiqLine {
    pub code: String,
    pub label: String,
    /// "LCR" or "NSFR".
    pub ratio: String,
    /// hqla_l1, hqla_l2a, hqla_l2b, outflow, inflow (LCR); asf, rsf (NSFR).
    pub kind: String,
    #[serde(default)]
    pub factor_pct: Option<f64>,
    #[serde(default)]
    pub factor_by_maturity: Option<MaturityFactors>,
    /// "asset" or "liability": the side on which the balance is positive.
    pub side: String,
}

/// The factor of a line by residual maturity: under 6 months, 6 months to under a year, a year or more.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct MaturityFactors {
    pub lt_6m: f64,
    #[serde(rename = "6m_1y")]
    pub m6_1y: f64,
    pub ge_1y: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct GlEntry {
    /// "exact", "prefix" or "range".
    #[serde(rename = "match")]
    pub how: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    /// The code of the line it maps to.
    pub line: String,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct LiqCaps {
    pub level2_max_pct: f64,
    pub level2b_max_pct: f64,
    pub inflow_cap_pct: f64,
    pub lcr_min_pct: f64,
    pub nsfr_min_pct: f64,
    /// "at_least" (the ratio is met at the minimum) or "greater_than" (it must be above it).
    #[serde(default = "at_least")]
    pub lcr_test: String,
    #[serde(default = "greater_than")]
    pub nsfr_test: String,
}

fn at_least() -> String {
    "at_least".to_string()
}
fn greater_than() -> String {
    "greater_than".to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    L1,
    L2a,
    L2b,
    Outflow,
    Inflow,
    Asf,
    Rsf,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s.trim() {
            "hqla_l1" => Kind::L1,
            "hqla_l2a" => Kind::L2a,
            "hqla_l2b" => Kind::L2b,
            "outflow" => Kind::Outflow,
            "inflow" => Kind::Inflow,
            "asf" => Kind::Asf,
            "rsf" => Kind::Rsf,
            _ => return None,
        })
    }
    fn is_lcr(self) -> bool {
        !matches!(self, Kind::Asf | Kind::Rsf)
    }
}

pub fn normalise_gl(code: &str) -> String {
    code.trim().to_uppercase()
}

/// Compares two GL codes: numerically when both are digits only, else as text.
fn cmp_codes(a: &str, b: &str) -> std::cmp::Ordering {
    let digits = |s: &str| !s.is_empty() && s.len() <= 38 && s.bytes().all(|c| c.is_ascii_digit());
    if digits(a) && digits(b) {
        if let (Ok(x), Ok(y)) = (a.parse::<u128>(), b.parse::<u128>()) {
            return x.cmp(&y);
        }
    }
    a.cmp(b)
}

impl GlEntry {
    fn matches(&self, code: &str) -> bool {
        match self.how.as_str() {
            "exact" => normalise_gl(&self.value) == code,
            "prefix" => code.starts_with(&normalise_gl(&self.value)),
            "range" => cmp_codes(code, &normalise_gl(&self.from)) != std::cmp::Ordering::Less && cmp_codes(code, &normalise_gl(&self.to)) != std::cmp::Ordering::Greater,
            _ => false,
        }
    }
}

impl LiqMap {
    /// Reads and checks a map file.
    pub fn parse(text: &str, file: &str) -> Result<LiqMap> {
        let m: LiqMap = serde_json::from_str(text).map_err(|e| anyhow!("The liquidity map {} is not valid: {}.", file, e))?;
        if m.source.trim().is_empty() || chrono::NaiveDate::parse_from_str(m.as_of.trim(), "%Y-%m-%d").is_err() {
            bail!("The liquidity map {} needs its \"source\" (the circular the factors come from) and an \"as_of\" date written YYYY-MM-DD.", file);
        }
        let mut codes: Vec<&str> = Vec::new();
        for l in &m.lines {
            if l.code.trim().is_empty() || codes.contains(&l.code.as_str()) {
                bail!("The liquidity map {} has a line with no code, or the code '{}' twice.", file, l.code);
            }
            codes.push(&l.code);
            let kind = Kind::parse(&l.kind).ok_or_else(|| anyhow!("The liquidity map {}: the line '{}' has the kind '{}', which is not one of hqla_l1, hqla_l2a, hqla_l2b, outflow, inflow, asf, rsf.", file, l.code, l.kind))?;
            let want = if kind.is_lcr() { "LCR" } else { "NSFR" };
            if l.ratio != want {
                bail!("The liquidity map {}: the line '{}' is of the kind {}, so its ratio must be {}, not '{}'.", file, l.code, l.kind, want, l.ratio);
            }
            if !matches!(l.side.as_str(), "asset" | "liability") {
                bail!("The liquidity map {}: the line '{}' has the side '{}', which is not asset or liability.", file, l.code, l.side);
            }
            match (l.factor_pct, l.factor_by_maturity) {
                (Some(f), None) if f.is_finite() && (0.0..=1000.0).contains(&f) => {}
                (None, Some(m)) if [m.lt_6m, m.m6_1y, m.ge_1y].iter().all(|f| f.is_finite() && (0.0..=1000.0).contains(f)) => {}
                _ => bail!("The liquidity map {}: the line '{}' needs exactly one of factor_pct or factor_by_maturity (lt_6m, 6m_1y, ge_1y), each between 0 and 1000.", file, l.code),
            }
            if l.factor_by_maturity.is_some() && !matches!(kind, Kind::Asf | Kind::Rsf) {
                bail!("The liquidity map {}: the line '{}' has a factor by maturity, which only NSFR lines (asf, rsf) can have.", file, l.code);
            }
        }
        for g in &m.gl_map {
            if !codes.contains(&g.line.as_str()) {
                bail!("The liquidity map {}: a GL rule maps to the line '{}', which is not in \"lines\".", file, g.line);
            }
            match g.how.as_str() {
                "exact" | "prefix" if !g.value.trim().is_empty() => {}
                "range" if !g.from.trim().is_empty() && !g.to.trim().is_empty() && cmp_codes(&normalise_gl(&g.from), &normalise_gl(&g.to)) != std::cmp::Ordering::Greater => {}
                _ => bail!("The liquidity map {}: a GL rule for '{}' needs match exact or prefix with a value, or range with from and to (from not above to).", file, g.line),
            }
        }
        let c = &m.caps;
        for (name, v) in [("level2_max_pct", c.level2_max_pct), ("level2b_max_pct", c.level2b_max_pct), ("inflow_cap_pct", c.inflow_cap_pct)] {
            if !(v.is_finite() && v > 0.0 && v < 100.0 || name == "inflow_cap_pct" && v.is_finite() && v > 0.0 && v <= 1000.0) {
                bail!("The liquidity map {}: caps.{} must be a percentage between 0 and 100 (the inflow cap above 0).", file, name);
            }
        }
        if c.level2b_max_pct > c.level2_max_pct {
            bail!("The liquidity map {}: caps.level2b_max_pct cannot be above caps.level2_max_pct.", file);
        }
        for (name, v) in [("lcr_test", &c.lcr_test), ("nsfr_test", &c.nsfr_test)] {
            if !matches!(v.as_str(), "at_least" | "greater_than") {
                bail!("The liquidity map {}: caps.{} must be at_least or greater_than.", file, name);
            }
        }
        Ok(m)
    }

    pub fn line(&self, code: &str) -> Option<&LiqLine> {
        self.lines.iter().find(|l| l.code == code)
    }
}

/// The map files in `portable_data/liquidity_maps` (file names without the folder), the sample first
/// written when the folder has none of that name.
pub fn list_maps() -> Vec<String> {
    crate::rules::write_text_if_missing(&maps_dir().join(SAMPLE_MAP_FILE), SAMPLE_MAP);
    let mut v: Vec<String> = std::fs::read_dir(maps_dir())
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("json"))).filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// Reads and checks one map file of `portable_data/liquidity_maps`.
pub fn load_map(name: &str) -> Result<LiqMap> {
    let file = if name.to_lowercase().ends_with(".json") { name.to_string() } else { format!("{name}.json") };
    if file.contains('/') || file.contains('\\') || file.contains("..") {
        bail!("The liquidity map name '{}' must be a file name in portable_data/{}.", name, MAPS_FOLDER);
    }
    crate::rules::write_text_if_missing(&maps_dir().join(SAMPLE_MAP_FILE), SAMPLE_MAP);
    let path: PathBuf = maps_dir().join(&file);
    let text = std::fs::read_to_string(&path).with_context(|| format!("Could not read the liquidity map {}", path.display()))?;
    let m = LiqMap::parse(&text, &format!("{}/{}", MAPS_FOLDER, file))?;
    crate::rules::note_used_file("Liquidity map", &format!("{}/{}", MAPS_FOLDER, file), &m.as_of, &m.source, m.sample);
    Ok(m)
}

// ---------------------------------------------------------------------------------- the calculation

/// How the balance column is signed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum BalanceSign {
    /// A trial balance with debits positive and credits negative: a liability is the balance × −1.
    #[default]
    DebitPositive,
    /// Balances already positive on their normal side.
    NormalSide,
}

impl BalanceSign {
    pub fn label(self) -> &'static str {
        match self {
            BalanceSign::DebitPositive => "Debit positive, credit negative",
            BalanceSign::NormalSide => "Positive on the normal side",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum MaturityUnit {
    #[default]
    Months,
    Days,
    Years,
}

impl MaturityUnit {
    pub const ALL: [MaturityUnit; 3] = [MaturityUnit::Months, MaturityUnit::Days, MaturityUnit::Years];
    pub fn label(self) -> &'static str {
        match self {
            MaturityUnit::Months => "Months",
            MaturityUnit::Days => "Days",
            MaturityUnit::Years => "Years",
        }
    }
    fn to_months(self, v: f64) -> f64 {
        match self {
            MaturityUnit::Months => v,
            MaturityUnit::Days => v / 365.0 * 12.0,
            MaturityUnit::Years => v * 12.0,
        }
    }
}

/// The columns of the GL tab.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LiqSpec {
    pub gl_col: String,
    pub balance_col: String,
    pub sign: BalanceSign,
    /// The residual maturity, for the NSFR lines that depend on it.
    pub maturity_col: Option<String>,
    pub maturity_unit: MaturityUnit,
}

/// A GL row, read.
#[derive(Clone, Debug, PartialEq)]
pub struct GlRow {
    pub code: String,
    pub balance: f64,
    pub months: Option<f64>,
}

/// The GL codes that match no rule of the map, each with the sum of its balances, by code.
#[derive(Debug)]
pub struct UnmappedError {
    pub map_file: String,
    pub codes: Vec<(String, f64)>,
}

impl std::fmt::Display for UnmappedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let first: Vec<&str> = self.codes.iter().take(10).map(|(c, _)| c.as_str()).collect();
        write!(
            f,
            "LCR / NSFR: {} GL code(s) are not in the liquidity map {}; the first {}: {}. Add a rule for each to the map file (or use Export unmapped to list them all).",
            self.codes.len(),
            self.map_file,
            first.len(),
            first.join(", ")
        )
    }
}

impl std::error::Error for UnmappedError {}

/// One line of the result: a line of the map (and a maturity bucket for a line that has factors by maturity).
#[derive(Clone, Debug, PartialEq)]
pub struct LineResult {
    pub ratio: String,
    pub code: String,
    pub label: String,
    pub kind: String,
    /// "<6 months", "6 months to <1 year", ">=1 year" or empty.
    pub maturity: String,
    /// The balance turned to the normal side (BDT).
    pub amount: f64,
    pub factor_pct: f64,
    pub weighted: f64,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Evaluation {
    pub lines: Vec<LineResult>,
    pub l1: f64,
    pub l2a: f64,
    pub l2b: f64,
    pub adj15: f64,
    pub adj40: f64,
    pub hqla: f64,
    pub outflows: f64,
    pub inflows: f64,
    pub inflow_counted: f64,
    pub nco: f64,
    pub lcr: Option<f64>,
    pub asf: f64,
    pub rsf: f64,
    pub nsfr: Option<f64>,
    pub warnings: Vec<String>,
}

/// HQLA after the Level 2 caps (Basel): `l2a` and `l2b` are after their factors. The two
/// percentages of the map give the Basel numbers 15/85, 15/60 and 2/3 for 40% and 15%.
pub fn hqla_after_caps(l1: f64, l2a: f64, l2b: f64, level2_max_pct: f64, level2b_max_pct: f64) -> (f64, f64, f64) {
    let (c2, c2b) = (level2_max_pct / 100.0, level2b_max_pct / 100.0);
    let adj15 = (l2b - c2b / (1.0 - c2b) * (l1 + l2a)).max(l2b - c2b / (1.0 - c2) * l1).max(0.0);
    let adj40 = (l2a + l2b - adj15 - c2 / (1.0 - c2) * l1).max(0.0);
    (adj15, adj40, l1 + l2a + l2b - adj15 - adj40)
}

fn maturity_bucket(months: f64) -> (usize, &'static str) {
    if months < 6.0 {
        (0, "<6 months")
    } else if months < 12.0 {
        (1, "6 months to <1 year")
    } else {
        (2, ">=1 year")
    }
}

/// Reads the GL rows of a table. A blank or non-numeric balance, and a blank maturity where the
/// map has a line that needs one, stop the run with the count and the first 5 GL codes.
pub fn rows_from_df(df: &DataFrame, spec: &LiqSpec) -> Result<Vec<GlRow>> {
    let who = "LCR / NSFR";
    let gl = df.column(spec.gl_col.trim()).map_err(|_| anyhow!("{}: the GL code column '{}' was not found. Choose it again.", who, spec.gl_col))?.cast(&DataType::String)?;
    let bal = df.column(spec.balance_col.trim()).map_err(|_| anyhow!("{}: the balance column '{}' was not found. Choose it again.", who, spec.balance_col))?;
    if !bal.dtype().is_numeric() {
        bail!("{}: the balance column '{}' is {}, not a number. Set it to a number in the Schema & Ingestion settings.", who, spec.balance_col, bal.dtype());
    }
    let bal = bal.cast(&DataType::Float64)?;
    let mat = match spec.maturity_col.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => {
            let s = df.column(c).map_err(|_| anyhow!("{}: the residual maturity column '{}' was not found. Choose it again.", who, c))?;
            if !s.dtype().is_numeric() {
                bail!("{}: the residual maturity column '{}' is {}, not a number. Set it to a number in the Schema & Ingestion settings.", who, c, s.dtype());
            }
            Some(s.cast(&DataType::Float64)?)
        }
        None => None,
    };
    let mut problems = Problems::default();
    let mut rows = Vec::with_capacity(df.height());
    for i in 0..df.height() {
        let code = normalise_gl(gl.str()?.get(i).unwrap_or(""));
        let key = || if code.is_empty() { format!("row {}", i + 1) } else { code.clone() };
        let b = bal.f64()?.get(i).filter(|v| v.is_finite());
        let Some(b) = b else {
            problems.add(&format!("the balance in '{}' is blank or not a number", spec.balance_col), key);
            continue;
        };
        let months = mat.as_ref().and_then(|m| m.f64().ok().and_then(|c| c.get(i))).filter(|v| v.is_finite()).map(|v| spec.maturity_unit.to_months(v));
        rows.push(GlRow { code, balance: b, months });
    }
    problems.into_result(who, "GL codes")?;
    Ok(rows)
}

/// The ratios of GL rows under a map. Unmapped codes and a code that matches two lines of the same
/// ratio stop the run; a line whose total is negative gives a warning naming it.
pub fn evaluate(rows: &[GlRow], map: &LiqMap, map_file: &str, sign: BalanceSign, has_maturity_col: bool) -> Result<Evaluation> {
    let who = "LCR / NSFR";
    // 1. the lines of each row
    let mut unmapped: Vec<(String, f64)> = Vec::new();
    let mut assigned: Vec<Vec<usize>> = Vec::with_capacity(rows.len()); // line indexes per row
    for r in rows {
        let mut lines: Vec<usize> = Vec::new();
        for g in map.gl_map.iter().filter(|g| g.matches(&r.code)) {
            let li = map.lines.iter().position(|l| l.code == g.line).ok_or_else(|| anyhow!("{}: the map {} maps to the line '{}', which it does not have.", who, map_file, g.line))?;
            if !lines.contains(&li) {
                lines.push(li);
            }
        }
        if lines.is_empty() {
            match unmapped.iter_mut().find(|(c, _)| *c == r.code) {
                Some(u) => u.1 += r.balance,
                None => unmapped.push((r.code.clone(), r.balance)),
            }
        }
        for ratio in ["LCR", "NSFR"] {
            let of_ratio: Vec<&LiqLine> = lines.iter().map(|i| &map.lines[*i]).filter(|l| l.ratio == ratio).collect();
            if of_ratio.len() > 1 {
                bail!("{}: the GL code '{}' matches two {} lines in the map {}: '{}' and '{}'. Make the rules of the map not overlap.", who, if r.code.is_empty() { "(blank)" } else { &r.code }, ratio, map_file, of_ratio[0].code, of_ratio[1].code);
            }
        }
        assigned.push(lines);
    }
    if !unmapped.is_empty() {
        unmapped.sort_by(|a, b| a.0.cmp(&b.0));
        return Err(anyhow::Error::new(UnmappedError { map_file: map_file.to_string(), codes: unmapped }));
    }
    // 2. amounts per line (and per maturity bucket)
    let mut amounts: Vec<[f64; 3]> = vec![[0.0; 3]; map.lines.len()];
    let mut problems = Problems::default();
    for (r, lines) in rows.iter().zip(&assigned) {
        for &li in lines {
            let l = &map.lines[li];
            let amount = match (l.side.as_str(), sign) {
                ("liability", BalanceSign::DebitPositive) => -r.balance,
                _ => r.balance,
            };
            let bucket = if l.factor_by_maturity.is_some() {
                match r.months {
                    Some(m) => maturity_bucket(m).0,
                    None => {
                        problems.add(&format!("the line '{}' needs the residual maturity and it is {}", l.code, if has_maturity_col { "blank" } else { "not chosen" }), || r.code.clone());
                        continue;
                    }
                }
            } else {
                0
            };
            amounts[li][bucket] += amount;
        }
    }
    problems.into_result(who, "GL codes")?;
    // 3. the lines
    let mut ev = Evaluation::default();
    for (li, l) in map.lines.iter().enumerate() {
        let buckets: Vec<(usize, f64, &'static str)> = match l.factor_by_maturity {
            Some(m) => vec![(0, m.lt_6m, "<6 months"), (1, m.m6_1y, "6 months to <1 year"), (2, m.ge_1y, ">=1 year")],
            None => vec![(0, l.factor_pct.unwrap_or(0.0), "")],
        };
        let total: f64 = amounts[li].iter().sum();
        if total < -1e-9 {
            ev.warnings.push(format!("The line '{}' ({}) has a negative total of {:.2} after the balances were turned to its normal side ({}); check the sign of the GL balances.", l.code, l.label, total, l.side));
        }
        for (b, f, label) in buckets {
            let amount = amounts[li][b];
            if l.factor_by_maturity.is_some() && amount == 0.0 {
                continue;
            }
            ev.lines.push(LineResult { ratio: l.ratio.clone(), code: l.code.clone(), label: l.label.clone(), kind: l.kind.clone(), maturity: label.to_string(), amount, factor_pct: f, weighted: amount * f / 100.0 });
        }
    }
    let sum = |k: Kind| -> f64 { ev.lines.iter().filter(|r| Kind::parse(&r.kind) == Some(k)).map(|r| r.weighted).sum() };
    ev.l1 = sum(Kind::L1);
    ev.l2a = sum(Kind::L2a);
    ev.l2b = sum(Kind::L2b);
    let (a15, a40, hqla) = hqla_after_caps(ev.l1, ev.l2a, ev.l2b, map.caps.level2_max_pct, map.caps.level2b_max_pct);
    (ev.adj15, ev.adj40, ev.hqla) = (a15, a40, hqla);
    ev.outflows = sum(Kind::Outflow);
    ev.inflows = sum(Kind::Inflow);
    ev.inflow_counted = ev.inflows.min(map.caps.inflow_cap_pct / 100.0 * ev.outflows);
    ev.nco = ev.outflows - ev.inflow_counted;
    ev.lcr = if ev.nco > 0.0 { Some(ev.hqla / ev.nco * 100.0) } else { None };
    ev.asf = sum(Kind::Asf);
    ev.rsf = sum(Kind::Rsf);
    ev.nsfr = if ev.rsf > 0.0 { Some(ev.asf / ev.rsf * 100.0) } else { None };
    Ok(ev)
}

fn meets(ratio: Option<f64>, min: f64, test: &str) -> &'static str {
    match ratio {
        None => "Not defined",
        Some(r) => {
            let ok = if test == "greater_than" { r > min } else { r >= min };
            if ok {
                "Meets the minimum"
            } else {
                "Below the minimum"
            }
        }
    }
}

pub const LINE_COLUMNS: [&str; 8] = ["Ratio", "Line", "Label", "Kind", "Residual maturity", "Amount", "Factor %", "Weighted amount"];
pub const SUMMARY_COLUMNS: [&str; 4] = ["Item", "Value", "Unit", "Note"];

/// The line detail table (code, label, amount, factor, weighted).
pub fn lines_frame(ev: &Evaluation) -> Result<DataFrame> {
    let col = |f: &dyn Fn(&LineResult) -> String| -> Vec<String> { ev.lines.iter().map(f).collect() };
    Ok(DataFrame::new(vec![
        Series::new(LINE_COLUMNS[0].into(), col(&|r| r.ratio.clone())),
        Series::new(LINE_COLUMNS[1].into(), col(&|r| r.code.clone())),
        Series::new(LINE_COLUMNS[2].into(), col(&|r| r.label.clone())),
        Series::new(LINE_COLUMNS[3].into(), col(&|r| r.kind.clone())),
        Series::new(LINE_COLUMNS[4].into(), col(&|r| r.maturity.clone())),
        Series::new(LINE_COLUMNS[5].into(), ev.lines.iter().map(|r| r.amount).collect::<Vec<_>>()),
        Series::new(LINE_COLUMNS[6].into(), ev.lines.iter().map(|r| r.factor_pct).collect::<Vec<_>>()),
        Series::new(LINE_COLUMNS[7].into(), ev.lines.iter().map(|r| r.weighted).collect::<Vec<_>>()),
    ])?)
}

/// The summary table: the LCR build-up and the NSFR, the minimums and flags, the map file used and
/// the warnings.
pub fn summary_frame(ev: &Evaluation, map: &LiqMap, map_file: &str) -> Result<DataFrame> {
    let mut rows: Vec<(String, Option<f64>, &str, String)> = Vec::new();
    let mut add = |item: &str, v: Option<f64>, unit: &'static str, note: &str| rows.push((item.to_string(), v, unit, note.to_string()));
    let c = &map.caps;
    add("Level 1 assets (after factors)", Some(ev.l1), "BDT", "");
    add("Level 2A assets (after factors)", Some(ev.l2a), "BDT", "");
    add("Level 2B assets (after factors)", Some(ev.l2b), "BDT", "");
    add("Adjustment for the Level 2B cap", Some(ev.adj15), "BDT", &format!("Level 2B at most {}% of HQLA", c.level2b_max_pct));
    add("Adjustment for the Level 2 cap", Some(ev.adj40), "BDT", &format!("Level 2 at most {}% of HQLA", c.level2_max_pct));
    add("HQLA after caps", Some(ev.hqla), "BDT", "");
    add("Cash outflows (weighted)", Some(ev.outflows), "BDT", "");
    add("Cash inflows (weighted)", Some(ev.inflows), "BDT", "");
    add("Inflows counted", Some(ev.inflow_counted), "BDT", &format!("at most {}% of the outflows", c.inflow_cap_pct));
    add("Net cash outflows", Some(ev.nco), "BDT", "");
    add("LCR", ev.lcr, "%", if ev.lcr.is_none() { "The net cash outflows are zero: the ratio is not defined" } else { "" });
    add("LCR minimum", Some(c.lcr_min_pct), "%", if c.lcr_test == "greater_than" { "must be greater than" } else { "at least" });
    add("LCR status", None, "", meets(ev.lcr, c.lcr_min_pct, &c.lcr_test));
    add("Available stable funding", Some(ev.asf), "BDT", "");
    add("Required stable funding", Some(ev.rsf), "BDT", "");
    add("NSFR", ev.nsfr, "%", if ev.nsfr.is_none() { "The required stable funding is zero: the ratio is not defined" } else { "" });
    add("NSFR minimum", Some(c.nsfr_min_pct), "%", if c.nsfr_test == "greater_than" { "must be greater than" } else { "at least" });
    add("NSFR status", None, "", meets(ev.nsfr, c.nsfr_min_pct, &c.nsfr_test));
    add("Liquidity map", None, "", &format!("{} (as of {}; {})", map_file, map.as_of, map.source));
    if map.sample {
        add("Sample parameters", None, "", "The factors in this map are a SAMPLE (Basel), not Bangladesh Bank's. Do not put these ratios in a return until the owner has replaced them.");
    }
    for w in &ev.warnings {
        add("Warning", None, "", w);
    }
    Ok(DataFrame::new(vec![
        Series::new(SUMMARY_COLUMNS[0].into(), rows.iter().map(|r| r.0.clone()).collect::<Vec<_>>()),
        Series::new(SUMMARY_COLUMNS[1].into(), rows.iter().map(|r| r.1).collect::<Vec<_>>()),
        Series::new(SUMMARY_COLUMNS[2].into(), rows.iter().map(|r| r.2.to_string()).collect::<Vec<_>>()),
        Series::new(SUMMARY_COLUMNS[3].into(), rows.iter().map(|r| r.3.clone()).collect::<Vec<_>>()),
    ])?)
}

/// A tab's rows through the map: the two tables.
pub fn run_df(df: &DataFrame, spec: &LiqSpec, map: &LiqMap, map_file: &str) -> Result<Evaluation> {
    let rows = rows_from_df(df, spec)?;
    evaluate(&rows, map, map_file, spec.sign, spec.maturity_col.as_deref().is_some_and(|c| !c.trim().is_empty()))
}

/// The unmapped GL codes as a table (code, balance) for the "Export unmapped" button.
pub fn unmapped_frame(codes: &[(String, f64)]) -> Result<DataFrame> {
    Ok(DataFrame::new(vec![
        Series::new("GL code".into(), codes.iter().map(|c| c.0.clone()).collect::<Vec<_>>()),
        Series::new("Balance".into(), codes.iter().map(|c| c.1).collect::<Vec<_>>()),
    ])?)
}

/// Writes the unmapped codes to a CSV file (GL code, balance), for the owner to add to the map.
pub fn export_unmapped_csv(codes: &[(String, f64)], path: &Path) -> Result<()> {
    let mut text = String::from("GL code,Balance\n");
    for (c, b) in codes {
        let field = if c.contains(',') || c.contains('"') { format!("\"{}\"", c.replace('"', "\"\"")) } else { c.clone() };
        text.push_str(&format!("{},{}\n", field, b));
    }
    std::fs::write(path, text).with_context(|| format!("Could not write {}", path.display()))
}
