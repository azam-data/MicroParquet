//! Loan classification from the overdue period (Phase 5 R4, BRPD Circular No. 15 of 27 November
//! 2024, effective 1 April 2025).
//!
//! Each loan gets the grade its overdue period earns (STD-0, STD-1, STD-2, SMA, SS, DF, B/L), the
//! worse of that and an optional subjective/override grade. The thresholds are in the dated rule
//! file `portable_data/rules/cl_overdue/<YYYY-MM-DD>.json`; the loan category of each row comes
//! from a code map, `portable_data/loan_category_map.json`, like the Phase 1 segment map. The grade
//! codes are the ones of `cl_code_map.json`, so the Phase 1 provisioning step and the Phase 4
//! migration matrix run on `CL_Final` directly.

use crate::banking::{normalise_code, ProvisionConfig};
use crate::problems::{row_keys, Problems};
use crate::rules::{load_latest, RuleFile};
use anyhow::{anyhow, bail, Context, Result};
use chrono::{Datelike, Months, NaiveDate};
use polars::prelude::*;
use serde::Deserialize;

/// The grades from the best to the worst.
pub const GRADES: [&str; 7] = ["STD-0", "STD-1", "STD-2", "SMA", "SS", "DF", "B/L"];

/// The output columns.
pub const OUT_COLUMNS: [&str; 4] = ["Overdue_Months", "CL_Objective", "CL_Final", "CL_Rule_File"];

pub const CATEGORY_MAP_FILE: &str = "loan_category_map.json";

/// The version of the shipped category map.
const CATEGORY_MAP_VERSION: u32 = 1;

/// The shipped category map: the names of the four categories (the codes of a bank's CBS are added
/// by the owner).
const DEFAULT_CATEGORY_MAP: &[(&str, &str)] = &[
    ("CONTINUOUS", "Continuous"),
    ("DEMAND", "Demand"),
    ("FIXED TERM", "Fixed term"),
    ("FIXED-TERM", "Fixed term"),
    ("TERM", "Fixed term"),
    ("SHORT TERM AGRI & MICRO", "Short-term agri & micro"),
    ("SHORT-TERM AGRI & MICRO", "Short-term agri & micro"),
    ("AGRI ST", "Short-term agri & micro"),
];

/// The rule file: the grade bands of each loan category.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct OverdueFile {
    pub categories: Vec<OverdueCategory>,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct OverdueCategory {
    pub name: String,
    /// From the best grade to the worst; the first has no bound (no overdue).
    pub bands: Vec<Band>,
}

/// A grade and the overdue period from which it applies: complete days (`from_days`) or complete
/// calendar months (`from_months`). The first band of a category has neither.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Band {
    pub grade: String,
    #[serde(default)]
    pub from_days: Option<i64>,
    #[serde(default)]
    pub from_months: Option<i64>,
}

/// The rank of a grade code (0 = STD-0 … 6 = B/L), or `None` for a code that is not one of the seven.
pub fn grade_rank(code: &str) -> Option<usize> {
    let c = normalise_code(code);
    GRADES.iter().position(|g| *g == c)
}

impl OverdueFile {
    pub fn category(&self, name: &str) -> Option<&OverdueCategory> {
        let want = normalise_code(name);
        self.categories.iter().find(|c| normalise_code(&c.name) == want)
    }
}

/// Checks a rule file: every category has bands from the best grade to the worst, the first with
/// no bound and each later one with exactly one bound that grows.
pub fn check_rules(f: &OverdueFile, file: &str) -> Result<()> {
    if f.categories.is_empty() {
        bail!("The rule file {} has no loan category.", file);
    }
    let mut seen: Vec<String> = Vec::new();
    for c in &f.categories {
        let name = normalise_code(&c.name);
        if name.is_empty() || seen.contains(&name) {
            bail!("The rule file {} has a loan category with no name, or the name '{}' twice.", file, c.name);
        }
        seen.push(name);
        if c.bands.is_empty() {
            bail!("The rule file {}: the category '{}' has no grade bands.", file, c.name);
        }
        let mut last_rank: Option<usize> = None;
        let mut last_bound: Option<(bool, i64)> = None; // (is months, value)
        for (i, b) in c.bands.iter().enumerate() {
            let Some(rank) = grade_rank(&b.grade) else {
                bail!("The rule file {}: the grade '{}' in the category '{}' is not one of {}.", file, b.grade, c.name, GRADES.join(", "));
            };
            if last_rank.is_some_and(|r| rank <= r) {
                bail!("The rule file {}: the grades of the category '{}' must run from the best to the worst ({}).", file, c.name, GRADES.join(", "));
            }
            last_rank = Some(rank);
            match (i, b.from_days, b.from_months) {
                (0, None, None) => {}
                (0, _, _) => bail!("The rule file {}: the first band of the category '{}' ({}) is the no-overdue grade and has no bound.", file, c.name, b.grade),
                (_, Some(d), None) if d >= 1 => {
                    if last_bound.is_some_and(|(months, _)| months) || last_bound.is_some_and(|(_, v)| d <= v) {
                        bail!("The rule file {}: the bounds of the category '{}' must grow (days before months, each larger than the one before).", file, c.name);
                    }
                    last_bound = Some((false, d));
                }
                (_, None, Some(m)) if m >= 1 => {
                    if last_bound.is_some_and(|(months, v)| months && m <= v) {
                        bail!("The rule file {}: the bounds of the category '{}' must grow (days before months, each larger than the one before).", file, c.name);
                    }
                    last_bound = Some((true, m));
                }
                _ => bail!("The rule file {}: the band {} of the category '{}' needs exactly one of from_days or from_months, at least 1.", file, b.grade, c.name),
            }
        }
    }
    Ok(())
}

/// The thresholds in force at `cutoff` (the latest dated file on or before it; the Bangladesh Bank
/// file is built in and written to the folder when it is missing).
pub fn load_overdue_rules(cutoff: NaiveDate) -> Result<RuleFile<OverdueFile>> {
    let f: RuleFile<OverdueFile> = load_latest(&crate::rules::rules_dir("cl_overdue"), "cl_overdue", cutoff, Some(crate::rules::CL_OVERDUE))?;
    check_rules(&f.data, &f.meta.file)?;
    Ok(f)
}

/// What the dialog asks for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OverdueSpec {
    /// The loan category column (its codes go through `loan_category_map.json`); none = one category for all rows.
    pub category_col: Option<String>,
    /// The category of every row when there is no category column.
    pub fixed_category: String,
    /// Exactly one of these two is set.
    pub since_col: Option<String>,
    pub dpd_col: Option<String>,
    pub override_col: Option<String>,
    pub cutoff: Option<NaiveDate>,
}

/// What was counted, for the status message.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OverdueStats {
    pub rows: usize,
    /// Rows with no overdue date (or no days past due): counted as not overdue.
    pub no_overdue_date: usize,
    /// Overrides worse than the objective grade (used).
    pub override_worse: usize,
    /// Overrides better than the objective grade (ignored).
    pub override_ignored: usize,
}

impl OverdueStats {
    pub fn note(&self, label: &str, has_override: bool) -> String {
        let mut s = format!("{}: {} row(s) with no overdue date are counted as not overdue (STD-0).", label, self.no_overdue_date);
        if has_override {
            s.push_str(&format!(" {} override(s) were worse than the overdue grade and were used; {} override(s) were better and were ignored.", self.override_worse, self.override_ignored));
        }
        s
    }
}

/// The category map: its pairs, written with the defaults when the file is missing.
pub fn load_category_map(dir: &std::path::Path) -> Result<Vec<(String, String)>> {
    std::fs::create_dir_all(dir).with_context(|| format!("Could not create {}", dir.display()))?;
    crate::banking::write_if_missing(&dir.join(CATEGORY_MAP_FILE), &crate::banking::pairs_to_json(CATEGORY_MAP_VERSION, DEFAULT_CATEGORY_MAP))?;
    let path = dir.join(CATEGORY_MAP_FILE);
    let file: crate::banking::JsonPairs = crate::banking::read_json_file(&path)?;
    let mut out: Vec<(String, String)> = Vec::new();
    for (k, v) in file.pairs {
        let key = normalise_code(&k);
        if key.is_empty() {
            bail!("{}: an entry has a blank code.", path.display());
        }
        if out.iter().any(|(o, _)| *o == key) {
            bail!("{}: the code '{}' is listed twice.", path.display(), k);
        }
        out.push((key, v.trim().to_string()));
    }
    Ok(out)
}

/// Complete calendar months from `since` to `cutoff`: the largest m with `since` + m months on or
/// before `cutoff` (adding months clamps to the month end, so 31 Jan + 1 month is 28/29 Feb).
/// `None` when `since` is after `cutoff`.
pub fn complete_months(since: NaiveDate, cutoff: NaiveDate) -> Option<i64> {
    if since > cutoff {
        return None;
    }
    let diff = (cutoff.year() as i64 - since.year() as i64) * 12 + cutoff.month() as i64 - since.month() as i64;
    let mut m = diff;
    if let Some(d) = u32::try_from(m).ok().and_then(|m| since.checked_add_months(Months::new(m))) {
        if d > cutoff {
            m -= 1;
        }
    }
    Some(m.max(0))
}

/// The grade rank of the overdue period under one category's bands.
fn objective_rank(cat: &OverdueCategory, days: i64, months: i64) -> usize {
    let mut rank = 0;
    for b in &cat.bands {
        let met = match (b.from_days, b.from_months) {
            (None, None) => true,
            (Some(d), _) => days >= d,
            (_, Some(m)) => months >= m,
        };
        if met {
            rank = grade_rank(&b.grade).unwrap_or(rank);
        }
    }
    rank
}

fn epoch() -> NaiveDate {
    NaiveDate::from_ymd_opt(1970, 1, 1).unwrap_or(NaiveDate::MIN)
}

/// The classification of every row of `df` (the table, with dates as Date columns).
pub struct Classified {
    pub months: Vec<i64>,
    pub objective: Vec<&'static str>,
    pub final_: Vec<&'static str>,
    pub stats: OverdueStats,
    pub rule_file: String,
}

/// The first `n` distinct values of `vals` that fail, as 'code' (or "(blank)"), for a message.
fn distinct_codes(vals: &[Option<String>], bad: &[usize], n: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for &i in bad {
        let v = vals.get(i).cloned().flatten().unwrap_or_default();
        let shown = if v.trim().is_empty() { "(blank)".to_string() } else { format!("'{}'", v.trim()) };
        if !out.contains(&shown) {
            out.push(shown);
            if out.len() >= n {
                break;
            }
        }
    }
    out
}

/// Classifies the rows of `df`. The columns are checked first; bad values stop the run with the
/// count and the first 5 account numbers (or row numbers).
pub fn classify_df(df: &DataFrame, spec: &OverdueSpec, rules: &RuleFile<OverdueFile>, category_map: &[(String, String)], cl_map: &[(String, String)]) -> Result<Classified> {
    let cutoff = spec.cutoff.ok_or_else(|| anyhow!("Classify by overdue: type the cutoff date as YYYY-MM-DD (for example 2026-10-31)."))?;
    let height = df.height();
    let (keys, key_label) = row_keys(df);
    let need = |name: &str, what: &str| -> Result<()> {
        if df.column(name).is_err() {
            bail!("Classify by overdue: the {} column '{}' was not found. Choose it again.", what, name);
        }
        Ok(())
    };
    let texts = |name: &str| -> Result<Vec<Option<String>>> {
        let c = df.column(name)?.cast(&DataType::String).with_context(|| format!("Classify by overdue: could not read the column '{}' as text", name))?;
        Ok(c.str()?.into_iter().map(|v| v.map(str::to_string)).collect())
    };
    let mut problems = Problems::default();

    // 1. the loan category of each row
    let mut cats: Vec<usize> = vec![0; height];
    match spec.category_col.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(cc) => {
            need(cc, "loan category")?;
            let vals = texts(cc)?;
            let mut bad: Vec<usize> = Vec::new();
            for (i, v) in vals.iter().enumerate() {
                let code = normalise_code(v.as_deref().unwrap_or(""));
                let mapped = category_map.iter().find(|(k, _)| *k == code).map(|(_, c)| c.as_str());
                match mapped.and_then(|name| rules.data.categories.iter().position(|c| normalise_code(&c.name) == normalise_code(name))) {
                    Some(p) => cats[i] = p,
                    None => bad.push(i),
                }
            }
            if !bad.is_empty() {
                let names: Vec<&str> = rules.data.categories.iter().map(|c| c.name.as_str()).collect();
                bail!(
                    "Loan category column '{}' has codes not in {} (or mapped to a category the rule file {} does not have): {}, on {} row(s). Add each code with its category ({}) to {}.",
                    cc,
                    CATEGORY_MAP_FILE,
                    rules.meta.file,
                    distinct_codes(&vals, &bad, 10).join(", "),
                    bad.len(),
                    names.join(", "),
                    CATEGORY_MAP_FILE
                );
            }
        }
        None => {
            let p = rules.data.categories.iter().position(|c| normalise_code(&c.name) == normalise_code(&spec.fixed_category));
            let Some(p) = p else {
                let names: Vec<&str> = rules.data.categories.iter().map(|c| c.name.as_str()).collect();
                bail!("Classify by overdue: choose the loan category of the rows ({}), or a column that holds it.", names.join(", "));
            };
            cats.iter_mut().for_each(|c| *c = p);
        }
    }

    // 2. the overdue-since date of each row
    let mut since: Vec<Option<NaiveDate>> = vec![None; height];
    match (spec.since_col.as_deref().filter(|c| !c.trim().is_empty()), spec.dpd_col.as_deref().filter(|c| !c.trim().is_empty())) {
        (Some(sc), None) => {
            need(sc, "overdue-since date")?;
            let c = df.column(sc)?;
            match c.dtype() {
                DataType::Date | DataType::Datetime(_, _) => {}
                other => bail!("Classify by overdue: the overdue-since column '{}' is {}, not a date. Set it to a Date type in the Schema & Ingestion settings.", sc, other),
            }
            let days = c.cast(&DataType::Date)?.cast(&DataType::Int32)?;
            for (i, d) in days.i32()?.into_iter().enumerate() {
                since[i] = d.map(|d| epoch() + chrono::Duration::days(d as i64));
            }
        }
        (None, Some(dc)) => {
            need(dc, "days past due")?;
            let c = df.column(dc)?;
            if !c.dtype().is_numeric() {
                bail!("Classify by overdue: the days-past-due column '{}' is {}, not a number. Set it to a number in the Schema & Ingestion settings.", dc, c.dtype());
            }
            let v = c.cast(&DataType::Float64)?;
            for (i, d) in v.f64()?.into_iter().enumerate() {
                match d {
                    None => {}
                    Some(x) if !x.is_finite() || x.fract() != 0.0 => problems.add(&format!("the days past due in '{}' is not a whole number of days", dc), || keys[i].clone()),
                    Some(x) if x < 0.0 => problems.add(&format!("the days past due in '{}' is negative", dc), || keys[i].clone()),
                    Some(x) => since[i] = cutoff.checked_sub_signed(chrono::Duration::days(x as i64)),
                }
            }
        }
        _ => bail!("Classify by overdue: choose either the overdue-since date column or the days-past-due column."),
    }

    // 3. the override grade of each row
    let mut over: Vec<Option<usize>> = vec![None; height];
    if let Some(oc) = spec.override_col.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        need(oc, "override grade")?;
        let vals = texts(oc)?;
        let mut bad: Vec<usize> = Vec::new();
        for (i, v) in vals.iter().enumerate() {
            let code = normalise_code(v.as_deref().unwrap_or(""));
            if code.is_empty() {
                continue; // no override on this row
            }
            // a sub-grade code (STD-0, STD-1, STD-2, …) as it is; any other code through cl_code_map.json
            let rank = grade_rank(&code).or_else(|| {
                cl_map.iter().find(|(k, _)| *k == code).and_then(|(_, g)| match g.as_str() {
                    "STD" => Some(0),
                    "SMA" => Some(3),
                    "SS" => Some(4),
                    "DF" => Some(5),
                    "BL" => Some(6),
                    _ => None,
                })
            });
            match rank {
                Some(r) => over[i] = Some(r),
                None => bad.push(i),
            }
        }
        if !bad.is_empty() {
            bail!("{}", crate::banking::cl_code_error(oc, &distinct_codes(&vals, &bad, 10)));
        }
    }

    // 4. the grades
    let mut out = Classified { months: vec![0; height], objective: vec![GRADES[0]; height], final_: vec![GRADES[0]; height], stats: OverdueStats { rows: height, ..Default::default() }, rule_file: rules.meta.file.clone() };
    for i in 0..height {
        let (days, months) = match since[i] {
            None => {
                // a blank date, or a blank days-past-due: not overdue
                if spec.dpd_col.as_deref().is_some_and(|c| !c.trim().is_empty()) || spec.since_col.as_deref().is_some_and(|c| !c.trim().is_empty()) {
                    out.stats.no_overdue_date += 1;
                }
                (0, 0)
            }
            Some(s) => match complete_months(s, cutoff) {
                None => {
                    problems.add(&format!("the overdue-since date is after the cutoff {}", cutoff.format("%Y-%m-%d")), || keys[i].clone());
                    continue;
                }
                Some(m) => ((cutoff - s).num_days(), m),
            },
        };
        let cat = &rules.data.categories[cats[i]];
        let obj = objective_rank(cat, days, months);
        out.months[i] = months;
        out.objective[i] = GRADES[obj];
        let fin = match over[i] {
            Some(o) if o > obj => {
                out.stats.override_worse += 1;
                o
            }
            Some(o) if o < obj => {
                out.stats.override_ignored += 1;
                obj
            }
            _ => obj,
        };
        out.final_[i] = GRADES[fin];
    }
    problems.into_result("Classify by overdue", &key_label)?;
    Ok(out)
}

/// The step: adds `Overdue_Months`, `CL_Objective`, `CL_Final` and `CL_Rule_File`, with the files
/// read from `portable_data`, and returns what was counted.
pub fn classify_lazy(lf: LazyFrame, spec: &OverdueSpec) -> Result<(LazyFrame, OverdueStats)> {
    let cutoff = spec.cutoff.ok_or_else(|| anyhow!("Classify by overdue: type the cutoff date as YYYY-MM-DD (for example 2026-10-31)."))?;
    let rules = load_overdue_rules(cutoff)?;
    let dir = crate::rules::data_dir();
    let category_map = load_category_map(&dir)?;
    let cl_map = ProvisionConfig::load_or_create(&dir)?.cl_code_map;
    // every grade of the rule file must be a code of cl_code_map.json, so the provisioning step reads CL_Final
    for c in &rules.data.categories {
        for b in &c.bands {
            let code = normalise_code(&b.grade);
            if !cl_map.iter().any(|(k, _)| *k == code) {
                bail!("The rule file {} has the grade '{}', which is not a code in cl_code_map.json. Add it there (STD, SMA, SS, DF or BL) so the provisioning step can read CL_Final.", rules.meta.file, b.grade);
            }
        }
    }
    let mut lf = lf;
    let schema = lf.collect_schema().context("Could not read the columns for Classify by overdue")?;
    for c in OUT_COLUMNS {
        if schema.get(c).is_some() {
            bail!("Classify by overdue: the table already has a column '{}'. Remove that column or the earlier step that made it first.", c);
        }
    }
    let lf = match spec.since_col.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(sc) if schema.get(sc).is_some() => crate::banking::ensure_date(lf, sc)?,
        _ => lf,
    };
    let df = lf.collect().context("Classify by overdue could not read the table")?;
    let c = classify_df(&df, spec, &rules, &category_map, &cl_map)?;
    let mut df = df;
    df.with_column(Series::new("Overdue_Months".into(), c.months.clone()))?;
    df.with_column(Series::new("CL_Objective".into(), c.objective.iter().map(|s| s.to_string()).collect::<Vec<_>>()))?;
    df.with_column(Series::new("CL_Final".into(), c.final_.iter().map(|s| s.to_string()).collect::<Vec<_>>()))?;
    df.with_column(Series::new("CL_Rule_File".into(), vec![c.rule_file.clone(); df.height()]))?;
    Ok((df.lazy(), c.stats))
}

/// The step, without the counts.
pub fn apply_classify_overdue(lf: LazyFrame, spec: &OverdueSpec) -> Result<LazyFrame> {
    classify_lazy(lf, spec).map(|r| r.0)
}

/// The spec of a `ClassifyOverdue` step.
pub fn spec_of(step: &crate::models::TransformStep) -> Result<OverdueSpec> {
    let crate::models::TransformStep::ClassifyOverdue { category_col, fixed_category, since_col, dpd_col, override_col, cutoff_date } = step else {
        bail!("internal: not a Classify by overdue step");
    };
    let given = |c: &Option<String>| c.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let cutoff = NaiveDate::parse_from_str(cutoff_date.trim(), "%Y-%m-%d").map_err(|_| anyhow!("Classify by overdue: type the cutoff date as YYYY-MM-DD (for example 2026-10-31)."))?;
    Ok(OverdueSpec { category_col: given(category_col), fixed_category: fixed_category.trim().to_string(), since_col: given(since_col), dpd_col: given(dpd_col), override_col: given(override_col), cutoff: Some(cutoff) })
}
