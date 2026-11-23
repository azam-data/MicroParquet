//! Custom sort lists and the Tenor order (Phase 6 U3).
//!
//! A list is a file `portable_data/sort_lists/<name>.json`. The sort step and the pivot keep a
//! copy of the list they use (name, values, case flag), so a macro replays the same order even if
//! the list file is edited or missing later. "Tenor" is built in: a number and a unit, ordered by
//! days.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

/// The names a list cannot take: they are the built-in orders of the menus.
pub const RESERVED_NAMES: [&str; 7] = ["tenor", "natural", "by value", "a\u{2192}z", "z\u{2192}a", "calendar", "ranges"];

fn default_true() -> bool {
    true
}

/// How the text values of a column are ordered.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub enum SortOrder {
    /// Case-insensitive, numbers inside the text by value (Phase 3).
    #[default]
    Natural,
    /// Terms such as 1M, 100D, 1Y by their length in days.
    Tenor,
    /// The values of a custom list, in the list's order.
    List {
        #[serde(default)]
        name: String,
        #[serde(default)]
        values: Vec<String>,
        #[serde(default = "default_true")]
        case_insensitive: bool,
    },
    /// Month names, day names (in week order from the copied week start), quarters, weeks,
    /// "Aug 2026", "2026 Q1", "FY2026" and "FY2026-27" (Phase 8 W0).
    Calendar {
        #[serde(default)]
        week_start: crate::dates::Weekday,
    },
    /// Labels of numeric ranges ("Below 0%", "0%", "8%\u{2013}9%", "> 15%", "1 Crore to 5 Crore") by their
    /// first number and its unit; "No …" and other labels after them (Phase 8 W0).
    Ranges,
}

impl SortOrder {
    /// The words of the menus.
    pub fn label(&self) -> String {
        match self {
            SortOrder::Natural => "Natural".to_string(),
            SortOrder::Tenor => "Tenor".to_string(),
            SortOrder::List { name, .. } => name.clone(),
            SortOrder::Calendar { .. } => "Calendar".to_string(),
            SortOrder::Ranges => "Ranges".to_string(),
        }
    }

    /// The Calendar order with the week start of a calendar.
    pub fn calendar(cal: &crate::dates::CalendarSettings) -> SortOrder {
        SortOrder::Calendar { week_start: cal.week_start }
    }

    pub fn is_natural(&self) -> bool {
        matches!(self, SortOrder::Natural)
    }

    /// The order of a list.
    pub fn from_list(list: &SortList) -> SortOrder {
        SortOrder::List { name: list.name.clone(), values: list.values.clone(), case_insensitive: list.case_insensitive }
    }

    /// Where a value stands: `Some(position)` for a value the order knows (a listed value, or a
    /// term that parses as a tenor), `None` for the rest. Blank text is not ranked either, so
    /// callers test blanks first.
    pub fn rank(&self, value: &str) -> Option<f64> {
        self.rank_key(value).map(|(r, _)| r)
    }

    /// The rank and a second key that breaks its ties (only Ranges uses it: "Below x", an exact "x",
    /// a range from x, "Above x").
    pub fn rank_key(&self, value: &str) -> Option<(f64, f64)> {
        match self {
            SortOrder::Natural => None,
            SortOrder::Tenor => parse_tenor_days(value).map(|r| (r, 0.0)),
            SortOrder::List { values, case_insensitive, .. } => {
                let v = value.trim();
                values
                    .iter()
                    .position(|x| if *case_insensitive { x.trim().to_lowercase() == v.to_lowercase() } else { x.trim() == v })
                    .map(|i| (i as f64, 0.0))
            }
            SortOrder::Calendar { week_start } => crate::dates::calendar_rank(value, *week_start).map(|r| (r, 0.0)),
            SortOrder::Ranges => crate::dates::parse_range(value).map(|k| (k.value, k.kind as f64)),
        }
    }

    /// Compares two labels, lowest first: ranked values by rank (ties and the others in natural
    /// order), ranked before the rest. Blanks (`None`) go last.
    pub fn compare(&self, a: &Option<String>, b: &Option<String>) -> Ordering {
        match (a, b) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(x), Some(y)) => {
                let (rx, ry) = (self.rank_key(x), self.rank_key(y));
                match (rx, ry) {
                    (Some(p), Some(q)) => p
                        .0
                        .partial_cmp(&q.0)
                        .unwrap_or(Ordering::Equal)
                        .then_with(|| p.1.partial_cmp(&q.1).unwrap_or(Ordering::Equal))
                        .then_with(|| natural_cmp(x, y)),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => natural_cmp(x, y),
                }
            }
        }
    }
}

fn natural_cmp(a: &str, b: &str) -> Ordering {
    crate::engine::natural_sort_key(a).cmp(&crate::engine::natural_sort_key(b)).then_with(|| a.cmp(b))
}

/// The length in days of a tenor such as "3y", "6 M", "100D", "2 weeks" or "O/N": a number and a
/// unit (D, day, days = 1; W, wk, week = 7; M, mo, mos, mon, month = 365/12 days; Y, yr, year = 365; any case,
/// with or without a space), or O/N, ON, Overnight = 1 day. `None` when the text is not a tenor.
pub fn parse_tenor_days(text: &str) -> Option<f64> {
    let t = text.trim().to_lowercase();
    if t.is_empty() {
        return None;
    }
    if matches!(t.as_str(), "o/n" | "on" | "overnight") {
        return Some(1.0);
    }
    let split = t.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    if num.is_empty() {
        return None;
    }
    let n: f64 = num.parse().ok()?;
    if !n.is_finite() {
        return None;
    }
    // a month is 365/12 days, so 12M is exactly 1Y (the two then sort in natural order)
    let days = match unit.trim() {
        "d" | "day" | "days" => n,
        "w" | "wk" | "wks" | "week" | "weeks" => n * 7.0,
        "m" | "mo" | "mos" | "mon" | "mons" | "month" | "months" => n * 365.0 / 12.0,
        "y" | "yr" | "yrs" | "year" | "years" => n * 365.0,
        _ => return None,
    };
    Some(days)
}

/// How many of the distinct values do not parse as a tenor (blanks are not counted).
pub fn count_unparsed_tenors<'a>(values: impl IntoIterator<Item = &'a str>) -> usize {
    let mut seen = std::collections::HashSet::new();
    values
        .into_iter()
        .filter(|v| !v.trim().is_empty() && seen.insert(v.to_string()))
        .filter(|v| parse_tenor_days(v).is_none())
        .count()
}

// ------------------------------------------------------------------------------------ lists

/// A saved list: its name, the values in order, and the columns it is suggested for.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct SortList {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default = "default_true")]
    pub case_insensitive: bool,
    #[serde(default)]
    pub apply_to_columns: Vec<String>,
}

/// `portable_data/sort_lists` (or the folder a test put in its place).
pub fn sort_lists_dir() -> PathBuf {
    crate::rules::data_dir().join("sort_lists")
}

fn file_for(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.json", crate::paths::sanitize_file_name(name)))
}

/// What is wrong with a list, in words a banker can act on; `None` when it can be saved.
pub fn validate_list(list: &SortList) -> Option<String> {
    let name = list.name.trim();
    if name.is_empty() {
        return Some("Type a name for the list.".to_string());
    }
    if RESERVED_NAMES.contains(&name.to_lowercase().as_str()) {
        return Some(format!("'{}' is a built-in order. Choose another name for the list.", name));
    }
    let values: Vec<&str> = list.values.iter().map(|v| v.trim()).filter(|v| !v.is_empty()).collect();
    if values.is_empty() {
        return Some("The list has no values. Type them one per line, or use \"Fill from column\".".to_string());
    }
    let mut seen = std::collections::HashSet::new();
    for v in values {
        let key = if list.case_insensitive { v.to_lowercase() } else { v.to_string() };
        if !seen.insert(key) {
            return Some(format!("'{}' is in the list twice. A value can have one place in the order.", v));
        }
    }
    None
}

/// The list as it is stored: names and values trimmed, blank lines dropped.
pub fn cleaned(list: &SortList) -> SortList {
    SortList {
        name: list.name.trim().to_string(),
        values: list.values.iter().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect(),
        case_insensitive: list.case_insensitive,
        apply_to_columns: list.apply_to_columns.iter().map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect(),
    }
}

/// Writes `<dir>/<name>.json`. A list of that name is replaced (the caller asks first).
pub fn save_list(dir: &Path, list: &SortList) -> Result<PathBuf> {
    if let Some(problem) = validate_list(list) {
        bail!("{}", problem);
    }
    let list = cleaned(list);
    std::fs::create_dir_all(dir).with_context(|| format!("Could not create the folder {}", dir.display()))?;
    let path = file_for(dir, &list.name);
    // two names that make the same file name would overwrite each other
    if let Ok(old) = read_list(&path) {
        if old.name.to_lowercase() != list.name.to_lowercase() {
            bail!("The name '{}' gives the same file name as the list '{}'. Choose another name.", list.name, old.name);
        }
    }
    let text = serde_json::to_string_pretty(&list).context("Could not write the list")?;
    std::fs::write(&path, text).with_context(|| format!("Could not save {}", path.display()))?;
    Ok(path)
}

fn read_list(path: &Path) -> Result<SortList> {
    let text = std::fs::read_to_string(path).with_context(|| format!("Could not read {}", path.display()))?;
    let list: SortList = serde_json::from_str(&text).with_context(|| format!("{} is not a sort list file", path.display()))?;
    Ok(list)
}

/// Every list of the folder, by name, and the files that could not be used (file, reason).
pub fn load_lists(dir: &Path) -> (Vec<SortList>, Vec<(String, String)>) {
    let mut lists = Vec::new();
    let mut problems = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return (lists, problems);
    };
    let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("json"))).collect();
    files.sort();
    for p in files {
        let file = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        match read_list(&p) {
            Ok(l) => match validate_list(&l) {
                None => lists.push(cleaned(&l)),
                Some(why) => problems.push((file, why)),
            },
            Err(e) => problems.push((file, format!("{:#}", e))),
        }
    }
    lists.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    (lists, problems)
}

/// The file that holds the list called `name` (the one whose name field it is).
fn file_of(dir: &Path, name: &str) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    rd.flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("json")))
        .find(|p| read_list(p).is_ok_and(|l| l.name.trim() == name.trim()))
}

/// Deletes the list's file.
pub fn delete_list(dir: &Path, name: &str) -> Result<()> {
    let Some(path) = file_of(dir, name) else {
        bail!("There is no saved list called '{}'.", name);
    };
    std::fs::remove_file(&path).with_context(|| format!("Could not delete the list '{}'", name))
}

/// Renames a list: the new file is written first, then the old one removed.
pub fn rename_list(dir: &Path, old: &str, new: &str) -> Result<()> {
    let (lists, _) = load_lists(dir);
    let Some(mut list) = lists.into_iter().find(|l| l.name == old) else {
        bail!("There is no saved list called '{}'.", old);
    };
    if old.trim().to_lowercase() == new.trim().to_lowercase() && old.trim() == new.trim() {
        return Ok(());
    }
    if load_lists(dir).0.iter().any(|l| l.name.to_lowercase() == new.trim().to_lowercase() && l.name != old) {
        bail!("A list called '{}' already exists. Choose another name.", new.trim());
    }
    let old_file = file_of(dir, old);
    list.name = new.trim().to_string();
    let new_file = save_list(dir, &list)?;
    if let Some(f) = old_file.filter(|f| *f != new_file) {
        std::fs::remove_file(&f).with_context(|| format!("Could not remove the old file {}", f.display()))?;
    }
    Ok(())
}

/// The list suggested for a column: the first whose `apply_to_columns` names it (case-insensitive).
pub fn suggested_for<'a>(lists: &'a [SortList], column: &str) -> Option<&'a SortList> {
    lists.iter().find(|l| l.apply_to_columns.iter().any(|c| c.trim().eq_ignore_ascii_case(column.trim())))
}

// ------------------------------------------------------------------------------------ sort keys

use polars::prelude::*;

/// The sort keys for one text column under an order other than Natural: each is an expression and
/// whether it sorts highest-first. Blank text and nulls get null keys, which the caller puts last.
/// Keys: 1. the group (0 = ranked, 1 = the rest), 2. the rank (in the direction of the sort, so a
/// reversed list or a longest-first tenor puts the unranked values after the ranked ones either
/// way), 3. the natural key (ties and unranked values).
pub fn order_sort_keys(column: &str, order: &SortOrder, descending: bool) -> Vec<(Expr, bool)> {
    let blank = col(column).str().strip_chars(lit(NULL)).eq(lit(""));
    let text = when(blank).then(lit(NULL).cast(DataType::String)).otherwise(col(column));
    let o1 = order.clone();
    let group = text.clone().map(
        move |s: Series| {
            let ca = s.str()?;
            let out: Int32Chunked = ca.into_iter().map(|v| v.map(|t| if o1.rank(t).is_some() { 0 } else { 1 })).collect();
            Ok(Some(out.with_name(s.name().clone()).into_series()))
        },
        GetOutput::from_type(DataType::Int32),
    );
    let o2 = order.clone();
    let rank = text.clone().map(
        move |s: Series| {
            let ca = s.str()?;
            let out: Float64Chunked = ca.into_iter().map(|v| v.and_then(|t| o2.rank(t))).collect();
            Ok(Some(out.with_name(s.name().clone()).into_series()))
        },
        GetOutput::from_type(DataType::Float64),
    );
    let mut keys = vec![(group, false), (rank, descending)];
    if matches!(order, SortOrder::Ranges) {
        // "Below x", an exact "x", a range from x, "Above x": the second key of a Ranges rank
        let o3 = order.clone();
        let sub = text.clone().map(
            move |s: Series| {
                let ca = s.str()?;
                let out: Float64Chunked = ca.into_iter().map(|v| v.and_then(|t| o3.rank_key(t)).map(|k| k.1)).collect();
                Ok(Some(out.with_name(s.name().clone()).into_series()))
            },
            GetOutput::from_type(DataType::Float64),
        );
        keys.push((sub, descending));
    }
    let natural = text.map(
        |s: Series| {
            let ca = s.str()?;
            let out: StringChunked = ca.into_iter().map(|v| v.map(crate::engine::natural_sort_key)).collect();
            Ok(Some(out.with_name(s.name().clone()).into_series()))
        },
        GetOutput::from_type(DataType::String),
    );
    keys.push((natural, false));
    keys
}

/// The order a field gets when it is added to a pivot, a chart or a sort (W0 "Auto order"): Calendar
/// when every non-blank label is a Calendar label, else Ranges when every one is a range label (or a
/// "No …" label, with at least one real range among them), else none (today's default). The caller
/// tries the column's declared order (`engine::column_orders`) first.
pub fn detect_order<'a>(labels: impl IntoIterator<Item = &'a str>, cal: &crate::dates::CalendarSettings) -> Option<SortOrder> {
    let labels: Vec<&str> = labels.into_iter().map(str::trim).filter(|v| !v.is_empty()).collect();
    if labels.is_empty() {
        return None;
    }
    if labels.iter().all(|l| crate::dates::calendar_rank(l, cal.week_start).is_some()) {
        return Some(SortOrder::calendar(cal));
    }
    let mut ranged = false;
    for l in &labels {
        match crate::dates::parse_range(l) {
            Some(k) => ranged |= !k.plain,
            None if crate::dates::is_no_label(l) => {}
            None => return None,
        }
    }
    ranged.then_some(SortOrder::Ranges)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenor_parses_numbers_and_units() {
        for (t, d) in [("1m", 365.0 / 12.0), ("3 Y", 1095.0), ("400D", 400.0), ("2 weeks", 14.0), ("1wk", 7.0), ("O/N", 1.0), ("overnight", 1.0), ("ON", 1.0), ("6 months", 182.5), ("1.5y", 547.5), ("3 mo", 91.25), ("2 mos", 365.0 / 6.0)] {
            assert_eq!(parse_tenor_days(t), Some(d), "{t}");
        }
        for t in ["abc", "", "5", "m", "-3m", "3x"] {
            assert_eq!(parse_tenor_days(t), None, "{t}");
        }
    }

    #[test]
    fn a_list_needs_a_name_values_and_no_duplicates() {
        let mut l = SortList { name: "Risk".into(), values: vec!["High".into(), "high".into()], case_insensitive: true, apply_to_columns: vec![] };
        assert!(validate_list(&l).expect("duplicate").contains("twice"));
        l.case_insensitive = false;
        assert!(validate_list(&l).is_none());
        l.name = " tenor ".into();
        assert!(validate_list(&l).expect("reserved").contains("built-in"));
        l.name = " ".into();
        assert!(validate_list(&l).expect("empty").contains("name"));
    }
}
