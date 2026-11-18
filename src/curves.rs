//! The dated yield-curve store (Phase 4 V4): `portable_data/curves/<YYYY-MM-DD>.json`, one file
//! per curve date, read by the FTP step and edited in the Curves manager.
//!
//! ```json
//! { "currency": "BDT", "as_of": "2026-01-01", "source": "Treasury, 1 Jan 2026",
//!   "points": [ { "tenor_days": 30, "rate_pct": 8.0 }, { "tenor_days": 365, "rate_pct": 10.0 } ] }
//! ```
//!
//! A curve applies from its date until the next curve's date. Points must have strictly
//! increasing tenors; a file that does not is refused with the reason.

use crate::models::FtpPoint;
use anyhow::{anyhow, bail, Context, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One point as written in the file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CurveFilePoint {
    pub tenor_days: i64,
    pub rate_pct: f64,
}

/// The JSON of a curve file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct CurveFile {
    #[serde(default)]
    pub currency: String,
    /// The date of the curve; when written it must be the file's date.
    #[serde(default)]
    pub as_of: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub points: Vec<CurveFilePoint>,
}

/// A curve of the store.
#[derive(Clone, Debug, PartialEq)]
pub struct YieldCurve {
    pub date: NaiveDate,
    pub currency: String,
    pub source: String,
    pub points: Vec<FtpPoint>,
}

/// The file name of the curve of `date`.
pub fn curve_file_name(date: NaiveDate) -> String {
    format!("{}.json", date.format("%Y-%m-%d"))
}

/// The identity of a curve's content: the first 8 hex characters of a hash of its date, currency,
/// source and points. Editing any of them changes it, so `FTP_Curve_Id` says which version of a
/// curve priced a deal even when the file was changed afterwards. (The hash is over the content,
/// not the file's spacing, so a file saved again without changes keeps its id.)
pub fn curve_id(curve: &YieldCurve) -> String {
    let mut text = format!("{}|{}|{}", curve.date.format("%Y-%m-%d"), curve.currency.trim(), curve.source.trim());
    for p in &curve.points {
        text.push_str(&format!("|{}:{:?}", p.tenor_days, p.ftp_rate));
    }
    blake3::hash(text.as_bytes()).to_hex()[..8].to_string()
}

/// Checks the points of a curve: at least one, whole tenors above 0 that strictly increase, and
/// rates that are numbers.
pub fn validate_points(points: &[FtpPoint]) -> Result<()> {
    check_points(points, None)
}

/// `validate_points` where every point is named by its line in the file (`lines[i]` is the line of
/// point `i`) when they are known, else by its number.
fn check_points(points: &[FtpPoint], lines: Option<&[usize]>) -> Result<()> {
    let at = |i: usize| match lines.and_then(|l| l.get(i)) {
        Some(l) => format!("line {}", l),
        None => format!("point {}", i + 1),
    };
    let at_cap = |i: usize| {
        let t = at(i);
        let mut c = t.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => t,
        }
    };
    if points.is_empty() {
        bail!("The curve has no points. Add at least one tenor and rate.");
    }
    for (i, p) in points.iter().enumerate() {
        if p.tenor_days < 1 {
            bail!("{}: the tenor {} is not a number of days above 0.", at_cap(i), p.tenor_days);
        }
        if !p.ftp_rate.is_finite() {
            bail!("{} (tenor {} days) has no valid rate.", at_cap(i), p.tenor_days);
        }
    }
    for (i, w) in points.windows(2).enumerate() {
        if w[0].tenor_days == w[1].tenor_days {
            bail!("{} and {} both have the tenor {} days ({}% and {}%). Each tenor needs one rate.", at_cap(i), at(i + 1), w[0].tenor_days, w[0].ftp_rate, w[1].ftp_rate);
        }
        if w[0].tenor_days > w[1].tenor_days {
            bail!("The tenors must increase: {} days ({}) is followed by {} days ({}).", w[0].tenor_days, at(i), w[1].tenor_days, at(i + 1));
        }
    }
    Ok(())
}

/// The 1-based line of the first occurrence of `"key"` at or after `from_line` (1-based), or `None`.
fn line_of_key(text: &str, key: &str, from_line: usize) -> Option<usize> {
    let pat = format!("\"{}\"", key);
    text.lines().enumerate().skip(from_line.saturating_sub(1)).find(|(_, l)| l.contains(&pat)).map(|(n, _)| n + 1)
}

/// The line of each point: the line of each `"tenor_days"` key, in order.
fn point_lines(text: &str, count: usize) -> Option<Vec<usize>> {
    let lines: Vec<usize> = text.lines().enumerate().filter(|(_, l)| l.contains("\"tenor_days\"")).map(|(n, _)| n + 1).collect();
    (lines.len() == count).then_some(lines)
}

/// Reads one curve file's text. `file_name` is the name it has (`2026-01-01.json`): its date is the
/// curve's date, and an `as_of` inside must say the same.
pub fn parse_curve_file(file_name: &str, text: &str) -> Result<YieldCurve> {
    let stem = file_name.strip_suffix(".json").unwrap_or(file_name);
    let date = NaiveDate::parse_from_str(stem, "%Y-%m-%d").map_err(|_| anyhow!("The file name '{}' is not a date. Name a curve file after its date: 2026-01-01.json.", file_name))?;
    let f: CurveFile = serde_json::from_str(text).map_err(|e| anyhow!("Line {}: it is not a valid curve file ({}). Expected {{\"currency\", \"as_of\", \"source\", \"points\": [{{\"tenor_days\": 30, \"rate_pct\": 8.0}}]}}.", e.line(), e))?;
    if !f.as_of.trim().is_empty() {
        let as_line = line_of_key(text, "as_of", 1).unwrap_or(1);
        let as_of = NaiveDate::parse_from_str(f.as_of.trim(), "%Y-%m-%d").map_err(|_| anyhow!("Line {}: 'as_of' ({}) is not a date. Write it as YYYY-MM-DD.", as_line, f.as_of))?;
        if as_of != date {
            bail!("Line {}: the file name says {} but 'as_of' says {}. Make them the same.", as_line, date, as_of);
        }
    }
    if f.currency.trim().is_empty() {
        match line_of_key(text, "currency", 1) {
            Some(l) => bail!("Line {}: the currency is empty. Every curve needs one: write \"currency\": \"BDT\" (or the curve's currency).", l),
            None => bail!("The currency is missing. Every curve needs one: add \"currency\": \"BDT\" (or the curve's currency) near the top of the file."),
        }
    }
    let points: Vec<FtpPoint> = f.points.iter().map(|p| FtpPoint { tenor_days: p.tenor_days, ftp_rate: p.rate_pct }).collect();
    check_points(&points, point_lines(text, points.len()).as_deref())?;
    Ok(YieldCurve { date, currency: f.currency, source: f.source, points })
}

/// Reads one curve file; every message names the file.
pub fn load_curve_file(path: &Path) -> Result<YieldCurve> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    let text = std::fs::read_to_string(path).with_context(|| format!("Could not read the curve file {}", path.display()))?;
    parse_curve_file(&name, &text).map_err(|e| anyhow!("Curve file {}: {}", name, e))
}

/// Writes a curve as `<dir>/<date>.json` (after checking it) and returns the path.
pub fn save_curve(dir: &Path, curve: &YieldCurve) -> Result<PathBuf> {
    validate_points(&curve.points)?;
    std::fs::create_dir_all(dir).with_context(|| format!("Could not create the curves folder {}", dir.display()))?;
    let file = CurveFile {
        currency: curve.currency.clone(),
        as_of: curve.date.format("%Y-%m-%d").to_string(),
        source: curve.source.clone(),
        points: curve.points.iter().map(|p| CurveFilePoint { tenor_days: p.tenor_days, rate_pct: p.ftp_rate }).collect(),
    };
    let path = dir.join(curve_file_name(curve.date));
    let text = serde_json::to_string_pretty(&file)?;
    crate::jobs::write_atomic(&path, &crate::jobs::JobCtx::inert(), |tmp| {
        std::fs::write(tmp, text.as_bytes())?;
        Ok(())
    })
    .with_context(|| format!("Could not write {}", path.display()))?;
    Ok(path)
}

/// Deletes the curve of `date`.
pub fn delete_curve(dir: &Path, date: NaiveDate) -> Result<()> {
    let path = dir.join(curve_file_name(date));
    std::fs::remove_file(&path).with_context(|| format!("Could not delete {}", path.display()))
}

/// The curves of a folder, oldest first, and the files that could not be read with the reason.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CurveStore {
    pub curves: Vec<YieldCurve>,
    pub problems: Vec<(String, String)>,
}

impl CurveStore {
    /// Reads every `*.json` file of `dir`. A missing folder is an empty store.
    pub fn load(dir: &Path) -> CurveStore {
        let mut store = CurveStore::default();
        let Ok(rd) = std::fs::read_dir(dir) else {
            return store;
        };
        let mut paths: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|x| x.eq_ignore_ascii_case("json")).unwrap_or(false)).collect();
        paths.sort();
        for p in paths {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            match load_curve_file(&p) {
                Ok(c) => store.curves.push(c),
                Err(e) => store.problems.push((name, format!("{:#}", e))),
            }
        }
        store.curves.sort_by_key(|c| c.date);
        store
    }

    /// The curves, or an error naming the first file that could not be read: the FTP step does
    /// not guess which curve a damaged file would have been.
    pub fn strict(&self) -> Result<&[YieldCurve]> {
        if let Some((_, reason)) = self.problems.first() {
            bail!("{}", reason);
        }
        Ok(&self.curves)
    }

    /// The latest curve dated on or before `date`.
    pub fn on_or_before(&self, date: NaiveDate) -> Option<&YieldCurve> {
        self.curves.iter().rev().find(|c| c.date <= date)
    }

    pub fn earliest(&self) -> Option<&YieldCurve> {
        self.curves.first()
    }

    /// The curves in `currency` (compared ignoring case and spaces) and the curves of other
    /// currencies that are skipped. A file that cannot be read stops this, naming the file (the
    /// FTP step does not guess a damaged file's currency).
    pub fn for_currency(&self, currency: &str) -> Result<(Vec<YieldCurve>, Vec<YieldCurve>)> {
        self.strict()?;
        let want = currency.trim().to_uppercase();
        if want.is_empty() {
            bail!("FTP: choose the currency of the curves (for example BDT).");
        }
        let (used, skipped): (Vec<YieldCurve>, Vec<YieldCurve>) = self.curves.iter().cloned().partition(|c| c.currency.trim().to_uppercase() == want);
        if used.is_empty() && !skipped.is_empty() {
            bail!(
                "FTP: there is no yield curve in {} in portable_data/curves (found {}). Add a {} curve in Banking > Yield curves, or change the step's currency.",
                want,
                skipped.iter().map(|c| format!("{} ({})", c.date.format("%Y-%m-%d"), c.currency.trim())).collect::<Vec<_>>().join(", "),
                want
            );
        }
        Ok((used, skipped))
    }
}

/// Reads a two-column CSV (tenor_days, rate_pct) into points: one heading line is allowed (no more), the
/// separator is a comma, a semicolon or a tab, and a rate may end in a %. The points must be valid
/// (see [`validate_points`]); every message names the line.
pub fn parse_curve_csv(text: &str) -> Result<Vec<FtpPoint>> {
    let mut points: Vec<FtpPoint> = Vec::new();
    let mut lines_of: Vec<usize> = Vec::new();
    // At most one heading line is skipped: the first line with anything on it, and only when it does
    // not start with a number of days. Any other line that cannot be read is an error with its number.
    let mut seen_content = false;
    for (n, raw) in text.lines().enumerate() {
        let line_no = n + 1;
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }
        let first_content = !seen_content;
        seen_content = true;
        let cells: Vec<&str> = line.split([',', ';', '\t']).map(str::trim).collect();
        let tenor: Result<i64, _> = cells[0].parse::<i64>();
        if first_content && tenor.is_err() {
            let rate_unreadable = cells.len() != 2 || cells[1].trim_end_matches('%').trim().replace(',', ".").parse::<f64>().is_err();
            if rate_unreadable {
                // the heading line
                continue;
            }
        }
        if cells.len() != 2 {
            bail!("Line {}: expected two columns (tenor_days, rate_pct), found {}.", line_no, cells.len());
        }
        let rate: Result<f64, _> = cells[1].trim_end_matches('%').trim().replace(',', ".").parse::<f64>();
        let tenor = tenor.map_err(|_| anyhow!("Line {}: '{}' is not a whole number of days.", line_no, cells[0]))?;
        let rate = rate.map_err(|_| anyhow!("Line {}: '{}' is not a rate in percent.", line_no, cells[1]))?;
        points.push(FtpPoint { tenor_days: tenor, ftp_rate: rate });
        lines_of.push(line_no);
    }
    if points.is_empty() {
        bail!("The file has no points. Write one line per tenor: tenor_days, rate_pct.");
    }
    // the same checks as a curve file, with line numbers
    for p in points.iter().enumerate() {
        let (i, p) = p;
        if p.tenor_days < 1 {
            bail!("Line {}: the tenor {} is not a number of days above 0.", lines_of[i], p.tenor_days);
        }
        if !p.ftp_rate.is_finite() {
            bail!("Line {}: the rate is not a number.", lines_of[i]);
        }
    }
    for (i, w) in points.windows(2).enumerate() {
        if w[0].tenor_days == w[1].tenor_days {
            bail!("Lines {} and {} both have the tenor {} days ({}% and {}%). Each tenor needs one rate.", lines_of[i], lines_of[i + 1], w[0].tenor_days, w[0].ftp_rate, w[1].ftp_rate);
        }
        if w[0].tenor_days > w[1].tenor_days {
            bail!("The tenors must increase: {} days (line {}) is followed by {} days (line {}).", w[0].tenor_days, lines_of[i], w[1].tenor_days, lines_of[i + 1]);
        }
    }
    Ok(points)
}

thread_local! {
    static CURVES_DIR_OVERRIDE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Makes this thread read curves from `dir` instead of `portable_data/curves` (tests, headless runs).
pub fn set_curves_dir_override(dir: Option<PathBuf>) {
    CURVES_DIR_OVERRIDE.with(|d| *d.borrow_mut() = dir);
}

/// Where the curves are: `portable_data/curves`, unless overridden on this thread.
pub fn curves_dir() -> PathBuf {
    CURVES_DIR_OVERRIDE.with(|d| d.borrow().clone()).unwrap_or_else(|| crate::models::portable_root_dir().join("curves"))
}

/// A fingerprint of the curve files (names, sizes and modified times), so a tab whose FTP step
/// reads the store notices an edited or added curve.
pub fn curves_stamp() -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let dir = curves_dir();
    let mut entries: Vec<(String, u64, Option<std::time::SystemTime>)> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let m = e.metadata().ok()?;
                    Some((e.file_name().to_string_lossy().to_string(), m.len(), m.modified().ok()))
                })
                .collect()
        })
        .unwrap_or_default();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.hash(&mut h);
    h.finish()
}
