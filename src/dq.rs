//! The data-quality scorecard (Phase 6 P4): rules in `portable_data/dq_rules/<name>.json` run over
//! a tab, each giving a pass percentage (passing rows ÷ rows checked), an overall score (the plain
//! average of those percentages) and a Green, Amber or Red rating from the thresholds of the rules
//! file. A control total is a Pass or a Fail with its difference and stays out of the average.
//!
//! Rows checked leave out blank values for every kind except completeness (which counts them).
//! Uniqueness fails every row whose key appears more than once.

use crate::models::ParseFailure;
use anyhow::{anyhow, bail, Context, Result};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const DQ_FOLDER: &str = "dq_rules";
/// The rule set shipped on first use (marked sample).
pub const SAMPLE_RULES: &str = include_str!("../assets/dq_rules/sample.json");
pub const SAMPLE_FILE: &str = "sample.json";
/// How many failing rows a rule lists in the dialog.
pub const FAILING_PREVIEW: usize = 20;

pub fn dq_dir() -> PathBuf {
    crate::rules::data_dir().join(DQ_FOLDER)
}

/// How the two columns of a consistency rule compare.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DqCompare {
    /// Column A is at least column B.
    #[default]
    Ge,
    /// Column A is at most column B.
    Le,
    /// Column A equals column B.
    Eq,
}

impl DqCompare {
    pub fn label(self) -> &'static str {
        match self {
            DqCompare::Ge => "\u{2265}",
            DqCompare::Le => "\u{2264}",
            DqCompare::Eq => "=",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DqKind {
    /// The column is not blank.
    Completeness { column: String },
    /// The key (one or more columns) appears once.
    Uniqueness { columns: Vec<String> },
    /// The column's values could be read as its type (the schema's parse failures).
    TypeValidity { column: String },
    /// The number is between `min` and `max` (either may be left out).
    Range { column: String, min: Option<f64>, max: Option<f64> },
    /// Column A compares to column B (Maturity ≥ Open date).
    Consistency { left: String, op: DqCompare, right: String },
    /// The value is one of `values`, or a key of the code map `code_map` (`cl_code_map`).
    AllowedValues { column: String, values: Vec<String>, code_map: Option<String>, case_insensitive: bool },
    /// The column's sum equals a typed GL figure within a tolerance in percent of it.
    ControlTotal { column: String, gl_total: f64, tolerance_pct: f64 },
}

impl Default for DqKind {
    fn default() -> Self {
        DqKind::Completeness { column: String::new() }
    }
}

impl DqKind {
    pub fn label(&self) -> &'static str {
        match self {
            DqKind::Completeness { .. } => "Completeness",
            DqKind::Uniqueness { .. } => "Uniqueness",
            DqKind::TypeValidity { .. } => "Type validity",
            DqKind::Range { .. } => "Range",
            DqKind::Consistency { .. } => "Consistency",
            DqKind::AllowedValues { .. } => "Allowed values",
            DqKind::ControlTotal { .. } => "Control total",
        }
    }

    /// The columns the rule reads.
    pub fn columns(&self) -> Vec<String> {
        match self {
            DqKind::Completeness { column } | DqKind::TypeValidity { column } | DqKind::Range { column, .. } | DqKind::AllowedValues { column, .. } | DqKind::ControlTotal { column, .. } => vec![column.clone()],
            DqKind::Uniqueness { columns } => columns.clone(),
            DqKind::Consistency { left, right, .. } => vec![left.clone(), right.clone()],
        }
    }

    /// The rule in words: "Account_No is not blank".
    pub fn describe(&self) -> String {
        match self {
            DqKind::Completeness { column } => format!("{} is not blank", column),
            DqKind::Uniqueness { columns } => format!("{} is unique", columns.join(" + ")),
            DqKind::TypeValidity { column } => format!("{} can be read as its type", column),
            DqKind::Range { column, min, max } => match (min, max) {
                (Some(a), Some(b)) => format!("{} is from {} to {}", column, a, b),
                (Some(a), None) => format!("{} is at least {}", column, a),
                (None, Some(b)) => format!("{} is at most {}", column, b),
                (None, None) => format!("{} has no limit", column),
            },
            DqKind::Consistency { left, op, right } => format!("{} {} {}", left, op.label(), right),
            DqKind::AllowedValues { column, values, code_map, .. } => match code_map {
                Some(m) => format!("{} is a code of {}", column, m),
                None => format!("{} is one of {} value(s)", column, values.len()),
            },
            DqKind::ControlTotal { column, gl_total, tolerance_pct } => format!("Sum of {} is {} within {}%", column, format_with_commas(*gl_total, 2), tolerance_pct),
        }
    }
}

fn format_with_commas(v: f64, d: usize) -> String {
    crate::models::format_with_commas(v, d)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct DqRule {
    /// What the scorecard calls it ("" = the rule in words).
    pub name: String,
    /// The kind and its settings are written next to the name: `"kind": "range", "column": ...`.
    #[serde(flatten)]
    pub kind: DqKind,
}

impl DqRule {
    pub fn title(&self) -> String {
        if self.name.trim().is_empty() {
            self.kind.describe()
        } else {
            self.name.trim().to_string()
        }
    }
}

/// `portable_data/dq_rules/<name>.json`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct DqRuleSet {
    /// The file's name (without `.json`).
    pub name: String,
    pub as_of: String,
    /// Who set these rules and these thresholds.
    pub source: String,
    /// A sample the owner must confirm before the scores go into a paper.
    pub sample: bool,
    /// Green from this overall score (percent) ...
    pub green_min: f64,
    /// ... Amber from this one, Red below.
    pub amber_min: f64,
    pub rules: Vec<DqRule>,
}

impl Default for DqRuleSet {
    fn default() -> Self {
        Self { name: String::new(), as_of: String::new(), source: String::new(), sample: false, green_min: 98.0, amber_min: 95.0, rules: Vec::new() }
    }
}

impl DqRuleSet {
    pub fn check(&self) -> Result<()> {
        if !(self.green_min.is_finite() && self.amber_min.is_finite()) || self.green_min > 100.0 || self.amber_min < 0.0 || self.amber_min > self.green_min {
            bail!("The thresholds must satisfy 0 \u{2264} Amber \u{2264} Green \u{2264} 100 (now Green {} and Amber {}).", self.green_min, self.amber_min);
        }
        if self.rules.is_empty() {
            bail!("The rule set has no rules. Add at least one.");
        }
        for r in &self.rules {
            let t = r.title();
            for c in r.kind.columns() {
                if c.trim().is_empty() {
                    bail!("The rule '{}' has a column that is not chosen.", t);
                }
            }
            match &r.kind {
                DqKind::Uniqueness { columns } if columns.is_empty() => bail!("The rule '{}' needs at least one key column.", t),
                DqKind::Range { min, max, .. } => {
                    if min.is_none() && max.is_none() {
                        bail!("The range rule '{}' needs a minimum, a maximum or both.", t);
                    }
                    if min.is_some_and(|v| !v.is_finite()) || max.is_some_and(|v| !v.is_finite()) || matches!((min, max), (Some(a), Some(b)) if a > b) {
                        bail!("The range rule '{}' has a minimum above its maximum or a value that is not a number.", t);
                    }
                }
                DqKind::AllowedValues { values, code_map, .. } if values.is_empty() && code_map.is_none() => bail!("The rule '{}' needs a list of allowed values or a code map.", t),
                DqKind::ControlTotal { gl_total, tolerance_pct, .. } if !gl_total.is_finite() || !tolerance_pct.is_finite() || *tolerance_pct < 0.0 => bail!("The control total '{}' needs a GL figure and a tolerance of 0 or more.", t),
                _ => {}
            }
        }
        Ok(())
    }

    /// Green, Amber or Red for an overall score.
    pub fn rating(&self, score: f64) -> &'static str {
        if score >= self.green_min {
            "Green"
        } else if score >= self.amber_min {
            "Amber"
        } else {
            "Red"
        }
    }
}

// ---------------------------------------------------------------------------------- the files

fn stem(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        bail!("Type a name for the rule set.");
    }
    if n.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) || n.contains("..") {
        bail!("The rule set name '{}' cannot be a file name. Leave out / \\ : * ? \" < > | and \"..\".", n);
    }
    Ok(n.trim_end_matches(".json").to_string())
}

pub fn list_rulesets(dir: &Path) -> Vec<String> {
    crate::rules::write_text_if_missing(&dir.join(SAMPLE_FILE), SAMPLE_RULES);
    let mut v: Vec<String> = std::fs::read_dir(dir).map(|rd| rd.flatten().filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".json").map(str::to_string)).collect()).unwrap_or_default();
    v.sort_by_key(|n| n.to_lowercase());
    v
}

pub fn load_ruleset(dir: &Path, name: &str) -> Result<DqRuleSet> {
    let s = stem(name)?;
    crate::rules::write_text_if_missing(&dir.join(SAMPLE_FILE), SAMPLE_RULES);
    let path = dir.join(format!("{}.json", s));
    let text = std::fs::read_to_string(&path).with_context(|| format!("Could not read the rule set {}", path.display()))?;
    let mut set: DqRuleSet = serde_json::from_str(&text).map_err(|e| anyhow!("The rule set {} is not valid: {}.", path.display(), e))?;
    set.name = s;
    set.check().map_err(|e| anyhow!("The rule set {}: {:#}", path.display(), e))?;
    crate::rules::note_used_file("Data-quality rules", &format!("{}/{}.json", DQ_FOLDER, set.name), &set.as_of, &set.source, set.sample);
    Ok(set)
}

pub fn save_ruleset(dir: &Path, set: &DqRuleSet) -> Result<PathBuf> {
    set.check()?;
    let s = stem(&set.name)?;
    std::fs::create_dir_all(dir).with_context(|| format!("Could not create the folder {}", dir.display()))?;
    let path = dir.join(format!("{}.json", s));
    let tmp = path.with_extension(format!("json.tmp{}", std::process::id()));
    std::fs::write(&tmp, serde_json::to_string_pretty(set)?).with_context(|| format!("Could not write {}", path.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("Could not write {}", path.display()))?;
    Ok(path)
}

// --------------------------------------------------------------------------------- the checks

/// What the schema's reading of one column found (the source before any step).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypeStats {
    /// Rows of the source.
    pub rows: usize,
    /// Rows whose typed value is blank (a blank in the file, or a value that could not be read).
    pub typed_blank: usize,
    /// Values that could not be read as the column's type.
    pub failures: usize,
    pub examples: Vec<String>,
}

impl TypeStats {
    /// The statistics of the columns of `df` (the typed source) with the parse failures of that source.
    pub fn from_source(df: &DataFrame, failures: &[ParseFailure]) -> HashMap<String, TypeStats> {
        let mut m = HashMap::new();
        for c in df.get_columns() {
            let f = failures.iter().find(|f| f.column == c.name().as_str());
            m.insert(c.name().to_string(), TypeStats { rows: df.height(), typed_blank: c.null_count(), failures: f.map(|f| f.count).unwrap_or(0), examples: f.map(|f| f.examples.clone()).unwrap_or_default() });
        }
        m
    }
}

/// One rule's outcome.
#[derive(Clone, Debug, PartialEq)]
pub struct RuleResult {
    pub name: String,
    pub kind: String,
    pub detail: String,
    /// Rows the rule looked at (blank values left out, except for completeness). None for a control total.
    pub checked: Option<usize>,
    pub failing: usize,
    /// Passing rows ÷ rows checked, in percent. None for a control total or a rule with nothing to check.
    pub pass_pct: Option<f64>,
    /// Pass or Fail for a control total.
    pub control: Option<ControlResult>,
    /// The rows (0-based) that failed, in file order. A type rule cannot say which: it has the examples.
    pub failing_rows: Vec<usize>,
    pub examples: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlResult {
    pub sum: f64,
    pub gl_total: f64,
    /// sum − GL.
    pub difference: f64,
    pub tolerance_pct: f64,
    pub pass: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DqResult {
    pub rows: usize,
    pub rules: Vec<RuleResult>,
    /// The simple average of the pass percentages (control totals not in it).
    pub overall: Option<f64>,
    pub rating: String,
}

fn strings(df: &DataFrame, name: &str) -> Result<Vec<Option<String>>> {
    let c = df.column(name).map_err(|_| anyhow!("There is no column '{}' in the table. Choose the column again.", name))?;
    let s = c.cast(&DataType::String).map_err(|e| anyhow!("The column '{}' could not be read as text: {}", name, e))?;
    Ok(s.str()?.into_iter().map(|v| v.map(str::to_string)).collect())
}

fn blank(v: &Option<String>) -> bool {
    v.as_deref().map(|t| t.trim().is_empty()).unwrap_or(true)
}

fn numbers(df: &DataFrame, name: &str, what: &str) -> Result<Vec<Option<f64>>> {
    let c = df.column(name).map_err(|_| anyhow!("There is no column '{}' in the table. Choose the column again.", name))?;
    let dt = c.dtype();
    if !(dt.is_numeric() || matches!(dt, DataType::Decimal(..))) {
        bail!("{}: the column '{}' is {}, not a number. Set its type in Schema & Ingestion.", what, name, dt);
    }
    Ok(c.cast(&DataType::Float64)?.f64()?.into_iter().map(|v| v.filter(|x| x.is_finite())).collect())
}

/// The values of a column as numbers for comparing: numbers as they are, dates as days (or ticks).
fn comparable(df: &DataFrame, name: &str) -> Result<(Vec<Option<f64>>, &'static str)> {
    let c = df.column(name).map_err(|_| anyhow!("There is no column '{}' in the table. Choose the column again.", name))?;
    let kind = match c.dtype() {
        d if d.is_numeric() || matches!(d, DataType::Decimal(..)) => "number",
        DataType::Date => "date",
        DataType::Datetime(..) => "date and time",
        d => bail!("The column '{}' is {}: a consistency rule compares numbers or dates. Set its type in Schema & Ingestion.", name, d),
    };
    let v: Vec<Option<f64>> = match kind {
        "number" => c.cast(&DataType::Float64)?.f64()?.into_iter().collect(),
        _ => c.cast(&DataType::Int64)?.i64()?.into_iter().map(|v| v.map(|x| x as f64)).collect(),
    };
    Ok((v, kind))
}

fn judged(name: &str, kind: &DqKind, detail: String, checked: usize, failing_rows: Vec<usize>) -> RuleResult {
    let failing = failing_rows.len();
    RuleResult { name: name.to_string(), kind: kind.label().to_string(), detail, checked: Some(checked), failing, pass_pct: if checked == 0 { None } else { Some((checked - failing.min(checked)) as f64 / checked as f64 * 100.0) }, control: None, failing_rows, examples: Vec::new() }
}

/// Runs the rules of `set` over `df` (the tab after its steps). `types`: the schema statistics of the
/// source for the type rules.
pub fn run_scorecard(df: &DataFrame, types: &HashMap<String, TypeStats>, set: &DqRuleSet, cl_code_map: &[(String, String)]) -> Result<DqResult> {
    set.check()?;
    let n = df.height();
    let mut out: Vec<RuleResult> = Vec::new();
    for r in &set.rules {
        let title = r.title();
        let detail = r.kind.describe();
        let res = match &r.kind {
            DqKind::Completeness { column } => {
                let v = strings(df, column)?;
                let failing: Vec<usize> = (0..n).filter(|i| blank(&v[*i])).collect();
                judged(&title, &r.kind, detail, n, failing)
            }
            DqKind::Uniqueness { columns } => {
                let cols: Vec<Vec<Option<String>>> = columns.iter().map(|c| strings(df, c)).collect::<Result<_>>()?;
                let key = |i: usize| -> Option<Vec<String>> {
                    let mut k = Vec::with_capacity(cols.len());
                    for c in &cols {
                        if blank(&c[i]) {
                            return None;
                        }
                        k.push(c[i].clone().unwrap_or_default().trim().to_string());
                    }
                    Some(k)
                };
                let mut counts: HashMap<Vec<String>, usize> = HashMap::new();
                let mut checked = 0usize;
                for i in 0..n {
                    if let Some(k) = key(i) {
                        checked += 1;
                        *counts.entry(k).or_insert(0) += 1;
                    }
                }
                let failing: Vec<usize> = (0..n).filter(|i| key(*i).is_some_and(|k| counts.get(&k).copied().unwrap_or(0) > 1)).collect();
                judged(&title, &r.kind, detail, checked, failing)
            }
            DqKind::TypeValidity { column } => {
                let t = types.get(column).ok_or_else(|| anyhow!("The rule '{}': the source has no column '{}'. Type validity reads the file's own columns (after renames).", title, column))?;
                // blanks in the file are not failures; a value that could not be read is
                let blanks = t.typed_blank.saturating_sub(t.failures);
                let checked = t.rows.saturating_sub(blanks);
                let mut res = judged(&title, &r.kind, detail, checked, Vec::new());
                res.failing = t.failures;
                res.pass_pct = if checked == 0 { None } else { Some((checked - t.failures.min(checked)) as f64 / checked as f64 * 100.0) };
                res.examples = t.examples.clone();
                res
            }
            DqKind::Range { column, min, max } => {
                let v = numbers(df, column, &format!("The rule '{}'", title))?;
                let mut checked = 0usize;
                let mut failing = Vec::new();
                for (i, x) in v.iter().enumerate() {
                    let Some(x) = x else { continue };
                    checked += 1;
                    if min.is_some_and(|m| *x < m) || max.is_some_and(|m| *x > m) {
                        failing.push(i);
                    }
                }
                judged(&title, &r.kind, detail, checked, failing)
            }
            DqKind::Consistency { left, op, right } => {
                let ((a, ka), (b, kb)) = (comparable(df, left)?, comparable(df, right)?);
                if ka != kb {
                    bail!("The rule '{}' compares a {} ('{}') with a {} ('{}'). Choose two columns of the same kind.", title, ka, left, kb, right);
                }
                let mut checked = 0usize;
                let mut failing = Vec::new();
                for i in 0..n {
                    let (Some(x), Some(y)) = (a[i], b[i]) else { continue };
                    checked += 1;
                    let ok = match op {
                        DqCompare::Ge => x >= y,
                        DqCompare::Le => x <= y,
                        DqCompare::Eq => x == y,
                    };
                    if !ok {
                        failing.push(i);
                    }
                }
                judged(&title, &r.kind, detail, checked, failing)
            }
            DqKind::AllowedValues { column, values, code_map, case_insensitive } => {
                let key = |s: &str| if *case_insensitive { s.trim().to_lowercase() } else { s.trim().to_string() };
                let mut allowed: Vec<String> = values.iter().map(|v| key(v)).collect();
                if let Some(m) = code_map {
                    if m != "cl_code_map" {
                        bail!("The rule '{}' names the code map '{}', which is not one the app has (cl_code_map).", title, m);
                    }
                    allowed.extend(cl_code_map.iter().map(|(k, _)| key(k)));
                }
                let v = strings(df, column)?;
                let mut checked = 0usize;
                let mut failing = Vec::new();
                for (i, x) in v.iter().enumerate() {
                    if blank(x) {
                        continue;
                    }
                    checked += 1;
                    if !allowed.contains(&key(x.as_deref().unwrap_or(""))) {
                        failing.push(i);
                    }
                }
                judged(&title, &r.kind, detail, checked, failing)
            }
            DqKind::ControlTotal { column, gl_total, tolerance_pct } => {
                let v = numbers(df, column, &format!("The control total '{}'", title))?;
                let sum: f64 = v.iter().flatten().sum();
                let diff = sum - gl_total;
                let pass = diff.abs() <= gl_total.abs() * tolerance_pct / 100.0;
                RuleResult {
                    name: title.clone(),
                    kind: r.kind.label().to_string(),
                    detail: format!("Sum {} vs GL {}, difference {} (tolerance {}%)", format_with_commas(sum, 2), format_with_commas(*gl_total, 2), format_with_commas(diff, 2), tolerance_pct),
                    checked: None,
                    failing: 0,
                    pass_pct: None,
                    control: Some(ControlResult { sum, gl_total: *gl_total, difference: diff, tolerance_pct: *tolerance_pct, pass }),
                    failing_rows: Vec::new(),
                    examples: Vec::new(),
                }
            }
        };
        out.push(res);
    }
    let pcts: Vec<f64> = out.iter().filter_map(|r| r.pass_pct).collect();
    let overall = if pcts.is_empty() { None } else { Some(pcts.iter().sum::<f64>() / pcts.len() as f64) };
    let rating = overall.map(|o| set.rating(o).to_string()).unwrap_or_default();
    Ok(DqResult { rows: n, rules: out, overall, rating })
}

pub const SCORECARD_COLUMNS: [&str; 7] = ["Rule", "Kind", "Detail", "Rows checked", "Rows failing", "Pass %", "Result"];
pub const OVERALL_LABEL: &str = "Overall score";

/// The scorecard as a table: a row per rule, then the overall score.
pub fn scorecard_frame(res: &DqResult) -> Result<DataFrame> {
    let s = |f: &dyn Fn(&RuleResult) -> String| -> Vec<String> { res.rules.iter().map(f).collect() };
    let mut rule = s(&|r| r.name.clone());
    let mut kind = s(&|r| r.kind.clone());
    let mut detail = s(&|r| r.detail.clone());
    let mut checked: Vec<Option<i64>> = res.rules.iter().map(|r| r.checked.map(|c| c as i64)).collect();
    let mut failing: Vec<Option<i64>> = res.rules.iter().map(|r| if r.control.is_some() { None } else { Some(r.failing as i64) }).collect();
    let mut pct: Vec<Option<f64>> = res.rules.iter().map(|r| r.pass_pct).collect();
    let mut result: Vec<String> = res.rules.iter().map(|r| match &r.control {
        Some(c) => if c.pass { "Pass".to_string() } else { "Fail".to_string() },
        None => String::new(),
    }).collect();
    rule.push(OVERALL_LABEL.to_string());
    kind.push(String::new());
    detail.push(format!("Average of {} rule(s) over {} row(s)", res.rules.iter().filter(|r| r.pass_pct.is_some()).count(), res.rows));
    checked.push(None);
    failing.push(None);
    pct.push(res.overall);
    result.push(res.rating.clone());
    Ok(DataFrame::new(vec![
        Series::new("Rule".into(), rule),
        Series::new("Kind".into(), kind),
        Series::new("Detail".into(), detail),
        Series::new("Rows checked".into(), checked),
        Series::new("Rows failing".into(), failing),
        Series::new("Pass %".into(), pct),
        Series::new("Result".into(), result),
    ])?)
}

/// The failing rows of rule `i`: a "Row" column (1-based, as the file's data rows) then the table's columns.
pub fn failing_frame(df: &DataFrame, res: &DqResult, i: usize, limit: Option<usize>) -> Result<DataFrame> {
    let r = res.rules.get(i).ok_or_else(|| anyhow!("There is no rule number {}.", i + 1))?;
    if r.failing_rows.is_empty() {
        bail!("The rule '{}' has no failing rows to list{}.", r.name, if r.examples.is_empty() { String::new() } else { format!(" (the values that could not be read: {})", r.examples.join(" | ")) });
    }
    let rows: Vec<usize> = r.failing_rows.iter().copied().take(limit.unwrap_or(usize::MAX)).collect();
    let idx = IdxCa::from_vec("idx".into(), rows.iter().map(|x| *x as IdxSize).collect());
    let mut out = df.take(&idx)?;
    // "Row" unless the table has a column of that name already
    let name = if df.get_column_names().iter().any(|n| n.as_str() == "Row") { "Row number" } else { "Row" };
    out.insert_column(0, Series::new(name.into(), rows.iter().map(|x| *x as i64 + 1).collect::<Vec<i64>>()))?;
    Ok(out)
}
