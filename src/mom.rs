//! Month-on-month variance and commentary (Phase 6 P3): two months' balances by one or two keys
//! (Product, Segment), the change, the balance-weighted rate and annual interest of each month,
//! the interest change split into a volume, a rate and a mix effect, and commentary lines made
//! from fixed sentence templates for the keys that moved by a material amount.

use crate::models::*;
use anyhow::{anyhow, bail, Result};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const STATUS_NEW: &str = "New";
pub const STATUS_CLOSED: &str = "Closed";
/// The heading of the commentary table (one line per row).
pub const COMMENTARY_COLUMN: &str = "Commentary";

/// What the tool is given besides the two months.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct MomSpec {
    /// One or two columns that name a line (Product, Segment).
    pub key_cols: Vec<String>,
    pub bal_col: String,
    /// The rate column, in percent (9.5 = 9.5%); the rate of a key is the average weighted by balance.
    pub rate_col: Option<String>,
    /// A column holding the number of accounts of a row; it is added up per key.
    pub count_col: Option<String>,
    /// A key is material when its balance moved by at least this amount, in the units of `scale` (50 = 50 Cr) ...
    pub material_amount: f64,
    /// ... or by at least this share of last month's balance, in percent.
    pub material_pct: f64,
    /// The scale the amounts of the commentary (and the threshold amount) are in.
    pub scale: NumberDisplayFormat,
    pub decimals: usize,
}

impl Default for MomSpec {
    fn default() -> Self {
        Self { key_cols: Vec::new(), bal_col: String::new(), rate_col: None, count_col: None, material_amount: 50.0, material_pct: 5.0, scale: NumberDisplayFormat::BdtCrore, decimals: 2 }
    }
}

impl MomSpec {
    /// One unit of `scale` in BDT (1 for plain numbers).
    pub fn divisor(&self) -> f64 {
        self.scale.scale().map(|s| s.divisor).unwrap_or(1.0)
    }

    /// An amount as the commentary shows it: "100.00 Cr".
    pub fn money(&self, v: f64) -> String {
        match self.scale.scale() {
            Some(s) => s.format(v, self.decimals),
            None => format_with_commas(v, self.decimals),
        }
    }

    pub fn check(&self) -> Result<()> {
        if self.key_cols.is_empty() || self.key_cols.len() > 2 || self.key_cols.iter().any(|k| k.trim().is_empty()) {
            bail!("MoM variance: choose one or two key columns (for example Product and Segment).");
        }
        if self.key_cols.len() == 2 && self.key_cols[0] == self.key_cols[1] {
            bail!("MoM variance: the two key columns are the same ('{}'). Choose two different columns, or one.", self.key_cols[0]);
        }
        if self.bal_col.trim().is_empty() {
            bail!("MoM variance: choose the balance column.");
        }
        if !(self.material_amount.is_finite() && self.material_amount >= 0.0) || !(self.material_pct.is_finite() && self.material_pct >= 0.0) {
            bail!("MoM variance: the materiality thresholds must be numbers of 0 or more (for example 50 and 5).");
        }
        Ok(())
    }

    fn has_rate(&self) -> bool {
        self.rate_col.as_deref().is_some_and(|c| !c.trim().is_empty())
    }
    fn has_count(&self) -> bool {
        self.count_col.as_deref().is_some_and(|c| !c.trim().is_empty())
    }
}

/// One key in one month.
#[derive(Clone, Debug, Default)]
struct Agg {
    bal: f64,
    interest: f64,
    count: f64,
    pos: bool,
    neg: bool,
}

impl Agg {
    /// The balance-weighted rate in percent; none for a balance of 0 or one made of both signs (the Phase 2 rule).
    fn rate(&self) -> Option<f64> {
        if (self.pos && self.neg) || self.bal == 0.0 {
            None
        } else {
            Some(self.interest / self.bal * 100.0)
        }
    }
}

/// One month: the sums of each key (matched trimmed and ignoring capitals) and the spellings met.
struct Month {
    aggs: BTreeMap<Vec<String>, Agg>,
    spellings: BTreeMap<Vec<String>, std::collections::BTreeSet<Vec<String>>>,
}

fn numeric(dt: &DataType) -> bool {
    dt.is_numeric() || matches!(dt, DataType::Decimal(..))
}

/// The keys of one month with their sums. A blank key, a non-number balance or a balance without a rate is an error.
fn month(lf: LazyFrame, spec: &MomSpec, which: &str) -> Result<Month> {
    let schema = lf.clone().collect_schema()?;
    let need = |c: &str, what: &str| -> Result<DataType> { schema.get(c).cloned().ok_or_else(|| anyhow!("MoM variance: the {} has no {} column '{}'. Choose a column both months have.", which, what, c)) };
    for k in &spec.key_cols {
        need(k, "key")?;
    }
    let bt = need(&spec.bal_col, "balance")?;
    if !numeric(&bt) {
        bail!("MoM variance: the balance column '{}' of the {} is {}, not a number. Set it to a number type in Schema & Ingestion.", spec.bal_col, which, bt);
    }
    let rate = spec.rate_col.as_deref().map(str::trim).filter(|c| !c.is_empty());
    if let Some(r) = rate {
        let rt = need(r, "rate")?;
        if !numeric(&rt) {
            bail!("MoM variance: the rate column '{}' of the {} is {}, not a number. Set it to a number type in Schema & Ingestion.", r, which, rt);
        }
    }
    let cnt = spec.count_col.as_deref().map(str::trim).filter(|c| !c.is_empty());
    if let Some(c) = cnt {
        let ct = need(c, "count")?;
        if !numeric(&ct) {
            bail!("MoM variance: the count column '{}' of the {} is {}, not a number.", c, which, ct);
        }
    }
    let mut sel: Vec<Expr> = spec.key_cols.iter().enumerate().map(|(i, k)| col(k.as_str()).cast(DataType::String).alias(format!("__k{i}"))).collect();
    sel.push(col(spec.bal_col.as_str()).cast(DataType::Float64).alias("__b"));
    if let Some(r) = rate {
        sel.push(col(r).cast(DataType::Float64).alias("__r"));
    }
    if let Some(c) = cnt {
        sel.push(col(c).cast(DataType::Float64).alias("__c"));
    }
    let df = lf.select(sel).collect()?;
    let keys: Vec<Vec<Option<String>>> = (0..spec.key_cols.len()).map(|i| Ok(df.column(&format!("__k{i}"))?.str()?.into_iter().map(|v| v.map(str::to_string)).collect())).collect::<Result<_>>()?;
    let bal: Vec<Option<f64>> = df.column("__b")?.f64()?.into_iter().collect();
    let rates: Option<Vec<Option<f64>>> = if rate.is_some() { Some(df.column("__r")?.f64()?.into_iter().collect()) } else { None };
    let counts: Option<Vec<Option<f64>>> = if cnt.is_some() { Some(df.column("__c")?.f64()?.into_iter().collect()) } else { None };
    let mut out: BTreeMap<Vec<String>, Agg> = BTreeMap::new();
    let mut spellings: BTreeMap<Vec<String>, std::collections::BTreeSet<Vec<String>>> = BTreeMap::new();
    let (mut blank_keys, mut no_rate) = (0usize, 0usize);
    // rows with a key but no balance (or no count when a count column is chosen): the run stops
    let (mut blank_bal, mut blank_cnt): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    let (mut n_blank_bal, mut n_blank_cnt) = (0usize, 0usize);
    for i in 0..df.height() {
        let mut key = Vec::with_capacity(keys.len());
        let mut blank = false;
        for k in &keys {
            match k[i].as_deref().map(str::trim) {
                Some(t) if !t.is_empty() => key.push(t.to_string()),
                _ => blank = true,
            }
        }
        if blank {
            blank_keys += 1;
            continue;
        }
        let label = key.join(" / ");
        let norm: Vec<String> = key.iter().map(|t| t.to_lowercase()).collect();
        spellings.entry(norm.clone()).or_default().insert(key);
        let a = out.entry(norm).or_default();
        if let Some(c) = counts.as_ref() {
            match c[i].filter(|v| v.is_finite()) {
                Some(c) => a.count += c,
                None => {
                    n_blank_cnt += 1;
                    if blank_cnt.len() < 5 {
                        blank_cnt.push(label.clone());
                    }
                }
            }
        }
        let Some(b) = bal[i].filter(|b| b.is_finite()) else {
            n_blank_bal += 1;
            if blank_bal.len() < 5 {
                blank_bal.push(label);
            }
            continue;
        };
        a.bal += b;
        a.pos |= b > 0.0;
        a.neg |= b < 0.0;
        if let Some(r) = &rates {
            match r[i].filter(|v| v.is_finite()) {
                Some(rv) => a.interest += b * rv / 100.0,
                None if b != 0.0 => no_rate += 1,
                None => {}
            }
        }
    }
    if blank_keys > 0 {
        bail!("MoM variance: {} row(s) of the {} have a blank key ({}). Fill them or filter them out first.", blank_keys, which, spec.key_cols.join(" / "));
    }
    if n_blank_bal > 0 {
        bail!("MoM variance: {} row(s) of the {} have a blank balance in '{}' (first keys: {}). Fill the balance or filter those rows out; a blank would count as 0.", n_blank_bal, which, spec.bal_col, blank_bal.join(", "));
    }
    if n_blank_cnt > 0 {
        bail!("MoM variance: {} row(s) of the {} have a blank count in '{}' (first keys: {}). Fill the count or filter those rows out.", n_blank_cnt, which, spec.count_col.as_deref().unwrap_or(""), blank_cnt.join(", "));
    }
    if no_rate > 0 {
        bail!("MoM variance: {} row(s) of the {} have a balance but no rate in '{}'. Fill the rate or filter those rows out; the interest would be wrong otherwise.", no_rate, which, rate.unwrap_or(""));
    }
    Ok(Month { aggs: out, spellings })
}

/// The variance table: a row per key of either month.
pub fn build_mom(prev: LazyFrame, curr: LazyFrame, spec: &MomSpec) -> Result<DataFrame> {
    Ok(build_mom_noted(prev, curr, spec)?.0)
}

/// The variance table and the notes about it (keys that were matched although they were spelt differently).
/// Keys are matched trimmed and ignoring capitals; the table shows the current month's spelling
/// (the previous month's for a closed key).
pub fn build_mom_noted(prev: LazyFrame, curr: LazyFrame, spec: &MomSpec) -> Result<(DataFrame, Vec<String>)> {
    spec.check()?;
    let pm = month(prev, spec, "previous month")?;
    let cm = month(curr, spec, "current month")?;
    let (p, c) = (&pm.aggs, &cm.aggs);
    let mut keys: Vec<Vec<String>> = p.keys().chain(c.keys()).cloned().collect();
    keys.sort();
    keys.dedup();
    let n = keys.len();
    // the shown spelling, and the keys that were merged
    let mut shown: Vec<Vec<String>> = Vec::with_capacity(n);
    let mut merged: Vec<String> = Vec::new();
    for k in &keys {
        let in_curr = cm.spellings.get(k);
        let mut all: std::collections::BTreeSet<&Vec<String>> = pm.spellings.get(k).into_iter().flatten().collect();
        all.extend(in_curr.into_iter().flatten());
        let pick = in_curr.and_then(|s| s.iter().next()).or_else(|| pm.spellings.get(k).and_then(|s| s.iter().next())).cloned().unwrap_or_else(|| k.clone());
        if all.len() > 1 {
            merged.push(format!("{} (shown as {})", all.iter().map(|s| s.join(" / ")).collect::<Vec<_>>().join(" + "), pick.join(" / ")));
        }
        shown.push(pick);
    }
    let notes: Vec<String> = if merged.is_empty() { Vec::new() } else { vec![format!("Note: keys that differ only in capital letters or spaces around them were added together: {}.", merged.join("; "))] };
    let with_rate = spec.has_rate();
    let with_count = spec.has_count();
    let mut cols: Vec<Series> = Vec::new();
    for (i, name) in spec.key_cols.iter().enumerate() {
        cols.push(Series::new(name.as_str().into(), shown.iter().map(|k| k[i].clone()).collect::<Vec<String>>()));
    }
    let zero = Agg::default();
    let status: Vec<String> = keys.iter().map(|k| match (p.contains_key(k), c.contains_key(k)) {
        (false, true) => STATUS_NEW.to_string(),
        (true, false) => STATUS_CLOSED.to_string(),
        _ => String::new(),
    }).collect();
    let pv = |k: &Vec<String>| p.get(k).unwrap_or(&zero).clone();
    let cv = |k: &Vec<String>| c.get(k).unwrap_or(&zero).clone();
    let f = |name: &str, v: Vec<Option<f64>>| Series::new(name.into(), v);
    cols.push(Series::new("Status".into(), status.clone()));
    cols.push(f("Prev balance", keys.iter().map(|k| Some(pv(k).bal)).collect()));
    cols.push(f("Curr balance", keys.iter().map(|k| Some(cv(k).bal)).collect()));
    cols.push(f("Change", keys.iter().map(|k| Some(cv(k).bal - pv(k).bal)).collect()));
    cols.push(f("Change %", keys.iter().map(|k| { let pb = pv(k).bal; if pb == 0.0 { None } else { Some((cv(k).bal - pb) / pb.abs() * 100.0) } }).collect()));
    if with_count {
        cols.push(f("Prev count", keys.iter().map(|k| Some(pv(k).count)).collect()));
        cols.push(f("Curr count", keys.iter().map(|k| Some(cv(k).count)).collect()));
    }
    if with_rate {
        let rates = |side: &dyn Fn(&Vec<String>) -> Agg| -> Vec<Option<f64>> { keys.iter().map(|k| side(k).rate()).collect() };
        let (pr, cr) = (rates(&pv), rates(&cv));
        let both = |i: usize| -> Option<(f64, f64)> { Some((pr[i]?, cr[i]?)) };
        // a key in only one month has no rate to compare: no effects
        let continuing = |i: usize| status[i].is_empty();
        cols.push(f("Prev rate", pr.clone()));
        cols.push(f("Curr rate", cr.clone()));
        cols.push(f("Rate change (bp)", (0..n).map(|i| both(i).filter(|_| continuing(i)).map(|(a, b)| (b - a) * 100.0)).collect()));
        cols.push(f("Prev annual interest", keys.iter().map(|k| Some(pv(k).interest)).collect()));
        cols.push(f("Curr annual interest", keys.iter().map(|k| Some(cv(k).interest)).collect()));
        cols.push(f("Annual interest change", keys.iter().map(|k| Some(cv(k).interest - pv(k).interest)).collect()));
        let eff = |g: &dyn Fn(f64, f64, f64, f64) -> f64| -> Vec<Option<f64>> {
            (0..n).map(|i| {
                let (a, b) = both(i).filter(|_| continuing(i))?;
                Some(g(pv(&keys[i]).bal, cv(&keys[i]).bal, a / 100.0, b / 100.0))
            }).collect()
        };
        cols.push(f("Volume effect", eff(&|pb, cb, pr, _| (cb - pb) * pr)));
        cols.push(f("Rate effect", eff(&|pb, _, pr, cr| pb * (cr - pr))));
        cols.push(f("Mix effect", eff(&|pb, cb, pr, cr| (cb - pb) * (cr - pr))));
    }
    Ok((DataFrame::new(cols)?, notes))
}

/// The column formats of the variance table: percentages as percentages, rates with their decimals.
pub fn mom_formats(spec: &MomSpec) -> std::collections::HashMap<String, ColumnFormatSpec> {
    let pct = ColumnFormatSpec { fmt_type: MetricFormatType::PercentAsIs, decimals: 2 };
    let mut m = std::collections::HashMap::new();
    for c in ["Change %", "Prev rate", "Curr rate"] {
        m.insert(c.to_string(), pct);
    }
    m.insert("Rate change (bp)".to_string(), ColumnFormatSpec { fmt_type: MetricFormatType::StandardCommas, decimals: 0 });
    let _ = spec;
    m
}

fn key_label(df: &DataFrame, spec: &MomSpec, i: usize) -> String {
    spec.key_cols.iter().map(|k| df.column(k).ok().and_then(|c| c.str().ok().and_then(|s| s.get(i).map(str::to_string))).unwrap_or_default()).collect::<Vec<_>>().join(" / ")
}

fn f64s(df: &DataFrame, name: &str) -> Option<Vec<Option<f64>>> {
    Some(df.column(name).ok()?.cast(&DataType::Float64).ok()?.f64().ok()?.into_iter().collect())
}

/// The commentary of a variance table: one sentence for each key that moved by a material amount
/// (and for each New or Closed key), the largest change first, then a total line. Fixed
/// templates, no estimation: every number in a sentence is a number of the table.
pub fn mom_commentary(df: &DataFrame, spec: &MomSpec) -> Result<Vec<String>> {
    let miss = |c: &str| anyhow!("The variance table has no column '{}'.", c);
    let prev = f64s(df, "Prev balance").ok_or_else(|| miss("Prev balance"))?;
    let curr = f64s(df, "Curr balance").ok_or_else(|| miss("Curr balance"))?;
    let change = f64s(df, "Change").ok_or_else(|| miss("Change"))?;
    let pct = f64s(df, "Change %").ok_or_else(|| miss("Change %"))?;
    let status: Vec<Option<String>> = df.column("Status").map_err(|_| miss("Status"))?.str()?.into_iter().map(|v| v.map(str::to_string)).collect();
    let crate_ = f64s(df, "Curr rate");
    let rchg = f64s(df, "Rate change (bp)");
    let threshold = spec.material_amount * spec.divisor();
    let mut lines: Vec<(f64, String, String)> = Vec::new();
    // New and Closed keys follow the same materiality as the rest; the ones below it are only counted
    let (mut small_new, mut small_closed) = (0usize, 0usize);
    for i in 0..df.height() {
        let (ch, st) = (change[i].unwrap_or(0.0), status[i].clone().unwrap_or_default());
        let label = key_label(df, spec, i);
        let material = ch.abs() >= threshold && ch != 0.0 || pct[i].is_some_and(|p| p.abs() >= spec.material_pct && ch != 0.0);
        let text = if (st == STATUS_NEW || st == STATUS_CLOSED) && !material {
            if st == STATUS_NEW {
                small_new += 1;
            } else {
                small_closed += 1;
            }
            continue;
        } else if st == STATUS_NEW {
            let rate = crate_.as_ref().and_then(|r| r[i]).map(|r| format!(" at {:.2}%", r)).unwrap_or_default();
            format!("{} is new: {}{}.", label, spec.money(curr[i].unwrap_or(0.0)), rate)
        } else if st == STATUS_CLOSED {
            format!("{} has closed: it had {} last month.", label, spec.money(prev[i].unwrap_or(0.0)))
        } else if material {
            let verb = if ch > 0.0 { "rose" } else { "fell" };
            let share = pct[i].map(|p| format!(" ({}%)", format_with_commas(p.abs(), 2))).unwrap_or_default();
            let rate = match (crate_.as_ref().and_then(|r| r[i]), rchg.as_ref().and_then(|r| r[i])) {
                (Some(cr), Some(bp)) => {
                    let whole = bp.round();
                    if whole == 0.0 {
                        format!("; rate unchanged at {:.2}%", cr)
                    } else {
                        format!("; rate {} {} bp to {:.2}%", if whole > 0.0 { "up" } else { "down" }, format_with_commas(whole.abs(), 0), cr)
                    }
                }
                _ => String::new(),
            };
            format!("{} {} by {}{} to {}{}.", label, verb, spec.money(ch.abs()), share, spec.money(curr[i].unwrap_or(0.0)), rate)
        } else {
            continue;
        };
        lines.push((ch.abs(), label, text));
    }
    lines.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
    let mut out: Vec<String> = lines.into_iter().map(|l| l.2).collect();
    if small_new + small_closed > 0 {
        let part = |n: usize, what: &str| (n > 0).then(|| format!("{} {}", n, what));
        let both: Vec<String> = [part(small_new, "new"), part(small_closed, "closed")].into_iter().flatten().collect();
        out.push(format!("Not listed, below the materiality limit: {} key(s).", both.join(" and ")));
    }
    let (tp, tc): (f64, f64) = (prev.iter().flatten().sum(), curr.iter().flatten().sum());
    let tch = tc - tp;
    let total = if tch == 0.0 {
        format!("Total: unchanged at {}.", spec.money(tc))
    } else {
        let share = if tp == 0.0 { String::new() } else { format!(" ({}%)", format_with_commas(tch / tp.abs() * 100.0, 2).trim_start_matches('-')) };
        format!("Total: {} by {}{} to {}.", if tch > 0.0 { "rose" } else { "fell" }, spec.money(tch.abs()), share, spec.money(tc))
    };
    out.push(total);
    Ok(out)
}

/// The commentary as a one-column table (one line per row), for a tab and for exports.
pub fn commentary_frame(lines: &[String]) -> Result<DataFrame> {
    Ok(DataFrame::new(vec![Series::new(COMMENTARY_COLUMN.into(), lines.to_vec())])?)
}
