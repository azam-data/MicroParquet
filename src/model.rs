//! The model of a project (Phase 9 M0): links between a data tab and a lookup tab, and the check of their keys.
//!
//! A link joins a **data** tab to a **lookup** tab on 1 to 3 key columns, many to one. Tabs are named by their project keys (`t1`,
//! `t2`, ...), so renaming a tab keeps its links. A link is usable only while the lookup's keys are unique and none is blank: the
//! key check (on the worker, after the lookup's steps) says so, and lists what has to be fixed at source.
//!
//! Keys match as the Join tool matches them: the same value, nothing trimmed, upper or lower case kept, and a missing key never
//! matches. The one thing this module adds is a name for the keys that cannot match: a text key that is empty or only spaces
//! counts as blank, like a missing one (see M0-3 in `docs/phase9-spec.md`).

use crate::engine::compile_tab_lazy;
use crate::models::{TabSource, TransformStep};
use anyhow::{anyhow, bail, Context, Result};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The most key columns of a link.
pub const MAX_KEY_PAIRS: usize = 3;
/// What the Links dialog and the Key check say with no project open.
pub const NO_PROJECT_MESSAGE: &str = "The model is part of a project. Save the tabs as a project first.";

/// One pair of key columns: the data tab's column and the lookup's.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct KeyPair {
    pub data: String,
    pub lookup: String,
}

/// A link from a data tab to a lookup tab (both by their project keys).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Link {
    pub data_tab: String,
    pub lookup_tab: String,
    pub keys: Vec<KeyPair>,
    /// The data tab's value column whose sum the key check shows for each unmatched key (remembered per link).
    #[serde(default)]
    pub unmatched_value: Option<String>,
}

/// The date column of a data tab, joined to Calendar · Date (Phase 9 M2). A data tab has at most one.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct DateLink {
    /// The data tab's project key.
    #[serde(default)]
    pub data_tab: String,
    /// Its date column (a Date or Datetime).
    #[serde(default)]
    pub column: String,
}

/// The shared calendar (Phase 9 M2): one `CalendarSettings` for the whole project, the date links of the data tabs, and the project
/// key of the generated Calendar tab. The Calendar is saved as a recipe, not as rows.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct ModelCalendar {
    /// Copied from `settings.json` when the model calendar is turned on, and changed here for the project.
    #[serde(default)]
    pub settings: crate::dates::CalendarSettings,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub date_links: Vec<DateLink>,
    /// The Calendar tab's project key.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tab: String,
}

/// Placeholder: what a column means for time intelligence, stock or flow (M3 fills it in).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct ColumnKind {}

/// Placeholder: a named measure (M3 to M5 fill it in).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct NamedMeasure {}

/// The date the model reports as of (Phase 9 M2): the latest snapshot date of all date-linked tabs (the default), or a date the
/// owner picked ("2026-10-31").
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub enum AsOf {
    #[default]
    Latest,
    Date(String),
}

/// The model of a project. Every field is optional in the file, so a project without a model has no `model` section at all.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Model {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar: Option<ModelCalendar>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub column_kinds: Vec<ColumnKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub measures: Vec<NamedMeasure>,
    #[serde(default, skip_serializing_if = "is_latest")]
    pub as_of: AsOf,
}

fn is_latest(a: &AsOf) -> bool {
    *a == AsOf::Latest
}

impl Model {
    /// The links the engine reads: the user's links, then the date link of each date-linked data tab (its date column to Calendar ·
    /// Date). The date links are the calendar's own; the Links dialog lists only the user's.
    pub fn all_links(&self) -> Vec<Link> {
        let mut out = self.links.clone();
        if let Some(c) = &self.calendar {
            if !c.tab.is_empty() {
                out.extend(c.date_links.iter().filter(|l| !l.data_tab.is_empty() && !l.column.is_empty()).map(|l| crate::model_calendar::date_link(&l.data_tab, &l.column, &c.tab)));
            }
        }
        out
    }

    /// Nothing in it: the project is written as version 1, byte for byte as Phase 8 wrote it.
    pub fn is_empty(&self) -> bool {
        *self == Model::default()
    }

    /// The link between these two tabs.
    pub fn link_between(&self, data_tab: &str, lookup_tab: &str) -> Option<usize> {
        self.links.iter().position(|l| l.data_tab == data_tab && l.lookup_tab == lookup_tab)
    }

    pub fn is_lookup(&self, tab: &str) -> bool {
        self.links.iter().any(|l| l.lookup_tab == tab)
    }

    pub fn is_data_tab(&self, tab: &str) -> bool {
        self.links.iter().any(|l| l.data_tab == tab)
    }
}

// ----------------------------------------------------------------------------------------------- making a link

/// What a tab is, for the Links dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKind {
    Table,
    /// A chart tab (it draws a chart).
    Chart,
    /// A tab that did not open.
    Placeholder,
}

/// A tab as the Links dialog and its checks see it.
#[derive(Clone, Debug)]
pub struct TabInfo {
    pub key: String,
    pub title: String,
    pub kind: TabKind,
    /// The columns after the tab's steps, with their types.
    pub columns: Vec<(String, DataType)>,
}

/// Why a tab cannot be part of a link, if it cannot.
pub fn tab_refusal(t: &TabInfo) -> Option<String> {
    match t.kind {
        TabKind::Table => None,
        TabKind::Chart => Some(format!("'{}' is a chart tab; a chart tab can't be linked. Pick the table the chart draws.", t.title)),
        TabKind::Placeholder => Some(format!("'{}' did not open; it can't be linked until it does.", t.title)),
    }
}

/// What a column's type is called in a message.
pub fn type_word(dt: &DataType) -> &'static str {
    match dt {
        DataType::String => "text",
        DataType::Boolean => "true/false",
        DataType::Date => "a date",
        DataType::Datetime(..) => "a date and time",
        d if d.is_numeric() => "a number",
        _ => "another kind of value",
    }
}

fn pair_list(keys: &[KeyPair]) -> String {
    keys.iter().map(|k| format!("{} \u{2194} {}", k.data, k.lookup)).collect::<Vec<_>>().join(", ")
}

/// A link as one line: "Accounts → Branch master".
pub fn link_name(data_title: &str, lookup_title: &str) -> String {
    format!("{} \u{2192} {}", data_title, lookup_title)
}

/// The columns of a link as the Links dialog lists them: "Wing, Product code" (or the pairs when the names differ).
pub fn key_columns_text(l: &Link, data_side: bool) -> String {
    l.keys.iter().map(|k| if data_side { k.data.clone() } else { k.lookup.clone() }).collect::<Vec<_>>().join(", ")
}

/// Refuses a link that cannot be made, in words. `replacing` is the link being edited (it does not count against itself).
/// `data` and `lookup` are the two tabs; the link's tab keys must be theirs.
pub fn validate_link(model: &Model, link: &Link, replacing: Option<usize>, data: &TabInfo, lookup: &TabInfo) -> Result<()> {
    if let Some(m) = tab_refusal(data) {
        bail!("{}", m);
    }
    if let Some(m) = tab_refusal(lookup) {
        bail!("{}", m);
    }
    if data.key == lookup.key {
        bail!("'{}' can't be linked to itself. Pick a different lookup tab.", data.title);
    }
    let others = |i: usize| Some(i) != replacing;
    let all = model.all_links();
    let is_lookup = all.iter().enumerate().any(|(i, l)| others(i) && l.lookup_tab == data.key);
    if is_lookup {
        bail!("'{}' is a lookup; a lookup can't have links of its own", data.title);
    }
    let is_data = all.iter().enumerate().any(|(i, l)| others(i) && l.data_tab == lookup.key);
    if is_data {
        bail!("'{}' is a data tab with links of its own; it can't also be a lookup", lookup.title);
    }
    if all.iter().enumerate().any(|(i, l)| others(i) && l.data_tab == data.key && l.lookup_tab == lookup.key) {
        bail!("'{}' is already linked to '{}'. Edit that link instead.", data.title, lookup.title);
    }
    if link.keys.is_empty() {
        bail!("Pick at least one key column on each side.");
    }
    if link.keys.len() > MAX_KEY_PAIRS {
        bail!("A link has at most {} key columns.", MAX_KEY_PAIRS);
    }
    for (i, k) in link.keys.iter().enumerate() {
        if k.data.is_empty() || k.lookup.is_empty() {
            bail!("Key {}: pick a column on each side.", i + 1);
        }
        if link.keys.iter().take(i).any(|p| p.data == k.data || p.lookup == k.lookup) {
            bail!("Key {}: a column can be used once in a link. Pick a different one.", i + 1);
        }
        let d = data.columns.iter().find(|c| c.0 == k.data).ok_or_else(|| anyhow!("'{}' has no column '{}'.", data.title, k.data))?;
        let l = lookup.columns.iter().find(|c| c.0 == k.lookup).ok_or_else(|| anyhow!("'{}' has no column '{}'.", lookup.title, k.lookup))?;
        if d.1 != l.1 {
            if type_word(&d.1) != type_word(&l.1) {
                bail!(
                    "'{}' is {} in '{}' and {} in '{}'. Set both to text in Schema & Ingestion first.",
                    k.data,
                    type_word(&d.1),
                    data.title,
                    type_word(&l.1),
                    lookup.title
                );
            }
            bail!("'{}' is {} in '{}' and {} in '{}'. Set both to the same type in Schema & Ingestion first.", k.data, d.1, data.title, l.1, lookup.title);
        }
    }
    Ok(())
}

// ----------------------------------------------------------------------------------------------- the key check

/// A tab as the worker reads it: its source and the steps that run.
#[derive(Clone, Debug)]
pub struct ModelTab {
    pub key: String,
    pub title: String,
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
}

/// A key that has no row in the lookup.
#[derive(Clone, Debug, PartialEq)]
pub struct UnmatchedKey {
    /// The key's values, in the link's order, as text.
    pub key: Vec<String>,
    pub rows: usize,
    /// The sum of the chosen value column over those rows; `None` when no column is chosen or every value is blank.
    pub sum: Option<f64>,
}

/// What the key check found.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct KeyCheck {
    pub data_rows: usize,
    pub lookup_rows: usize,
    /// Keys that appear more than once in the lookup, and the rows they cover.
    pub duplicate_keys: usize,
    pub duplicate_rows: usize,
    /// The first of them, with the number of rows each has.
    pub duplicate_examples: Vec<(Vec<String>, usize)>,
    /// Lookup rows with a blank key.
    pub lookup_blank_keys: usize,
    /// Distinct keys of the data with no row in the lookup (all of them, sorted by key).
    pub unmatched: Vec<UnmatchedKey>,
    pub unmatched_rows: usize,
    /// Data rows with a blank key.
    pub data_blank_keys: usize,
    pub matched_rows: usize,
    /// Lookup rows no data row uses (for information).
    pub unused_lookup_rows: usize,
    /// The value column the sums are of, when one was chosen and could be used.
    pub value_column: Option<String>,
    /// Why the chosen value column was not used.
    pub value_note: Option<String>,
}

/// How a link stands.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkStatus {
    /// Not checked yet, or being checked.
    Checking,
    Ok,
    /// Usable; this many data keys have no lookup row and show as "(no match)".
    OkUnmatched(usize),
    /// Not usable until fixed at source: the reasons ("3 duplicate keys", "2 blank lookup keys").
    Broken(Vec<String>),
    /// The check could not run (a tab is not open, a column is gone): the reason. The link is not usable either.
    Failed(String),
}

impl LinkStatus {
    pub fn text(&self) -> String {
        match self {
            LinkStatus::Checking => "Checking\u{2026}".to_string(),
            LinkStatus::Ok => "OK".to_string(),
            LinkStatus::OkUnmatched(n) => format!("OK \u{b7} {} unmatched {}", n, if *n == 1 { "key" } else { "keys" }),
            LinkStatus::Broken(r) => format!("Broken: {}", r.join(", ")),
            LinkStatus::Failed(m) => Self::cant_check(m),
        }
    }

    /// "Can't check: the tab 'Branch master' is closed": a link whose check cannot run says why in its own words, apart from the
    /// duplicate and blank keys of a link that was checked (M0-4, owner decision on S3).
    pub fn cant_check(why: &str) -> String {
        format!("Can't check: {}", why.trim_end_matches('.'))
    }

    /// The link cannot be used (a broken link or one that cannot be checked).
    pub fn is_broken(&self) -> bool {
        matches!(self, LinkStatus::Broken(_) | LinkStatus::Failed(_))
    }

    /// The reason, without the word "Broken".
    pub fn reason(&self) -> String {
        match self {
            LinkStatus::Broken(r) => r.join(", "),
            LinkStatus::Failed(m) => Self::cant_check(m),
            _ => String::new(),
        }
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", crate::models::format_with_commas(n as f64, 0), if n == 1 { one } else { many })
}

impl KeyCheck {
    pub fn status(&self) -> LinkStatus {
        let mut reasons = Vec::new();
        if self.duplicate_keys > 0 {
            reasons.push(plural(self.duplicate_keys, "duplicate key", "duplicate keys"));
        }
        if self.lookup_blank_keys > 0 {
            reasons.push(plural(self.lookup_blank_keys, "blank lookup key", "blank lookup keys"));
        }
        if !reasons.is_empty() {
            LinkStatus::Broken(reasons)
        } else if !self.unmatched.is_empty() {
            LinkStatus::OkUnmatched(self.unmatched.len())
        } else {
            LinkStatus::Ok
        }
    }
}

/// "'Branch master' has 3 duplicate keys. Open Model ▸ Key check to see them." (what a lookup field of a broken link says)
pub fn broken_message(lookup_title: &str, status: &LinkStatus) -> String {
    if let LinkStatus::Failed(why) = status {
        return format!("The link to '{}' can't be used. {}. Open Model \u{25b8} Key check to see it.", lookup_title, LinkStatus::cant_check(why));
    }
    format!("'{}' has {}. Open Model \u{25b8} Key check to see {}.", lookup_title, status.reason(), if matches!(status, LinkStatus::Broken(r) if r.len() == 1 && r[0].starts_with("1 ")) { "it" } else { "them" })
}

/// The open line with the broken links: "9 tabs opened; link 'Accounts → Branch master' is broken: 3 duplicate keys".
pub fn open_line(base: &str, broken: &[(String, String)]) -> String {
    if broken.is_empty() {
        return base.to_string();
    }
    let mut s = base.trim_end_matches('.').to_string();
    for (name, why) in broken {
        s.push_str(&format!("; link '{}' is broken: {}", name, why));
    }
    s
}

/// "Model: 4 links · 1 broken" for the status bar; `None` with no links.
pub fn status_bar_text(links: usize, broken: usize) -> Option<String> {
    if links == 0 {
        return None;
    }
    let mut s = format!("Model: {}", plural(links, "link", "links"));
    if broken > 0 {
        s.push_str(&format!(" \u{b7} {} broken", broken));
    }
    Some(s)
}

const K: &str = "__mp_k";
const BLANK: &str = "__mp_blank";
const ROWS: &str = "__mp_rows";
const VALUE: &str = "__mp_sum";
const VALUE_N: &str = "__mp_vn";

fn key_names(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("{}{}", K, i)).collect()
}

/// The tab with its key columns as text (`__mp_k0`...; null where blank) and a `__mp_blank` flag. A blank key never matches.
fn keyed(lf: LazyFrame, cols: &[String], string_cols: &[bool]) -> LazyFrame {
    let mut exprs: Vec<Expr> = Vec::new();
    let mut blank = lit(false);
    for (i, c) in cols.iter().enumerate() {
        let as_text = col(c.as_str()).cast(DataType::String);
        let e = if string_cols.get(i).copied().unwrap_or(false) {
            // empty or only spaces counts as blank; the key itself is kept exactly as it is
            when(as_text.clone().str().strip_chars(lit(NULL)).str().len_chars().eq(lit(0u32))).then(lit(NULL).cast(DataType::String)).otherwise(as_text)
        } else {
            as_text
        };
        exprs.push(e.alias(format!("{}{}", K, i)));
        blank = blank.or(col(format!("{}{}", K, i)).is_null());
    }
    lf.with_columns(exprs).with_columns([blank.alias(BLANK)])
}

/// Two key columns of a link can be matched when they have the same type; a date and time is matched to the Calendar's date by its day.
fn key_types_agree(d: &DataType, l: &DataType) -> bool {
    d == l || (matches!(d, DataType::Datetime(..)) && *l == DataType::Date)
}

/// The data side's key columns: the column itself, or its day (`__mp_dk<i>`, added) for a date and time matched to a date.
fn data_key_columns(lf: LazyFrame, link: &Link) -> (LazyFrame, Vec<String>) {
    let mut lf = lf;
    let schema = lf.collect_schema().ok();
    let mut adds: Vec<Expr> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for (i, k) in link.keys.iter().enumerate() {
        if schema.as_ref().and_then(|s| s.get(k.data.as_str())).is_some_and(|d| matches!(d, DataType::Datetime(..))) {
            let n = format!("__mp_dk{}", i);
            adds.push(col(k.data.as_str()).cast(DataType::Date).alias(n.as_str()));
            names.push(n);
        } else {
            names.push(k.data.clone());
        }
    }
    (if adds.is_empty() { lf } else { lf.with_columns(adds) }, names)
}

fn tab_columns(lf: &mut LazyFrame, t: &ModelTab) -> Result<Vec<(String, DataType)>> {
    let schema = lf.collect_schema().with_context(|| format!("'{}' could not be read", t.title))?;
    Ok(schema.iter().map(|(n, d)| (n.to_string(), d.clone())).collect())
}

/// Opens both tabs and checks that the link's columns are still there with the same types.
fn open_pair(link: &Link, data: &ModelTab, lookup: &ModelTab, temp_dir: &Path) -> Result<(LazyFrame, LazyFrame, Vec<bool>, Vec<bool>)> {
    let mut dl = compile_tab_lazy(&data.source, &data.steps, temp_dir).with_context(|| format!("'{}' could not be read", data.title))?;
    let mut ll = compile_tab_lazy(&lookup.source, &lookup.steps, temp_dir).with_context(|| format!("'{}' could not be read", lookup.title))?;
    let dc = tab_columns(&mut dl, data)?;
    let lc = tab_columns(&mut ll, lookup)?;
    let (mut ds, mut ls) = (Vec::new(), Vec::new());
    for k in &link.keys {
        let d = dc.iter().find(|c| c.0 == k.data).ok_or_else(|| anyhow!("'{}' has no column '{}'", data.title, k.data))?;
        let l = lc.iter().find(|c| c.0 == k.lookup).ok_or_else(|| anyhow!("'{}' has no column '{}'", lookup.title, k.lookup))?;
        if !key_types_agree(&d.1, &l.1) {
            bail!("'{}' is {} in '{}' and {} in '{}' now. Set both to the same type in Schema & Ingestion first", k.data, type_word(&d.1), data.title, type_word(&l.1), lookup.title);
        }
        ds.push(d.1 == DataType::String);
        ls.push(l.1 == DataType::String);
    }
    Ok((dl, ll, ds, ls))
}

fn text_at(df: &DataFrame, name: &str, i: usize) -> String {
    df.column(name).ok().and_then(|c| c.get(i).ok()).map(|v| match v {
        AnyValue::String(s) => s.to_string(),
        AnyValue::StringOwned(s) => s.to_string(),
        AnyValue::Null => String::new(),
        other => other.to_string(),
    }).unwrap_or_default()
}

fn first_count(df: &DataFrame) -> usize {
    df.get_columns().first().and_then(|c| c.get(0).ok()).and_then(|v| v.extract::<u64>()).unwrap_or(0) as usize
}

/// The key check of one link, after the steps of both tabs. `cancel` is asked now and then.
pub fn check_link(link: &Link, data: &ModelTab, lookup: &ModelTab, temp_dir: &Path, cancel: &dyn Fn() -> Result<()>) -> Result<KeyCheck> {
    if link.keys.is_empty() {
        bail!("The link has no key columns.");
    }
    let (dl, ll, ds, ls) = open_pair(link, data, lookup, temp_dir)?;
    let n = link.keys.len();
    let ks = key_names(n);
    let key_exprs = || ks.iter().map(|k| col(k.as_str())).collect::<Vec<_>>();
    let (dl, data_cols) = data_key_columns(dl, link);
    let lookup_cols: Vec<String> = link.keys.iter().map(|k| k.lookup.clone()).collect();
    let d = keyed(dl.clone(), &data_cols, &ds);
    let l = keyed(ll, &lookup_cols, &ls);
    let mut out = KeyCheck::default();

    // the lookup: rows, blank keys, keys that repeat
    out.lookup_rows = first_count(&l.clone().select([len()]).collect()?);
    out.lookup_blank_keys = first_count(&l.clone().filter(col(BLANK)).select([len()]).collect()?);
    cancel()?;
    let lookup_keys = l.clone().filter(col(BLANK).not()).group_by_stable(key_exprs()).agg([len().alias(ROWS)]);
    let dups = lookup_keys.clone().filter(col(ROWS).gt(lit(1))).sort(ks.clone(), SortMultipleOptions::default()).collect()?;
    out.duplicate_keys = dups.height();
    for i in 0..dups.height() {
        let rows = dups.column(ROWS).ok().and_then(|c| c.get(i).ok()).and_then(|v| v.extract::<u64>()).unwrap_or(0) as usize;
        out.duplicate_rows += rows;
        if out.duplicate_examples.len() < 10 {
            out.duplicate_examples.push((ks.iter().map(|k| text_at(&dups, k, i)).collect(), rows));
        }
    }
    cancel()?;

    // the data: rows, blank keys, then each distinct key with its rows (and the sum of the chosen column)
    out.data_rows = first_count(&d.clone().select([len()]).collect()?);
    out.data_blank_keys = first_count(&d.clone().filter(col(BLANK)).select([len()]).collect()?);
    let mut value: Option<String> = None;
    if let Some(v) = link.unmatched_value.as_ref().filter(|v| !v.is_empty()) {
        let mut probe = dl.clone();
        match probe.collect_schema()?.get(v.as_str()) {
            None => out.value_note = Some(format!("'{}' has no column '{}', so no sums are shown", data.title, v)),
            Some(dt) if !dt.is_numeric() => out.value_note = Some(format!("'{}' is {} in '{}', so no sums are shown. Pick a number column", v, type_word(dt), data.title)),
            Some(_) => value = Some(v.clone()),
        }
    }
    let mut aggs = vec![len().alias(ROWS)];
    if let Some(v) = &value {
        aggs.push(col(v.as_str()).cast(DataType::Float64).sum().alias(VALUE));
        aggs.push(col(v.as_str()).count().alias(VALUE_N));
    }
    let per_key = d.clone().filter(col(BLANK).not()).group_by_stable(key_exprs()).agg(aggs);
    let distinct_lookup = lookup_keys.clone().select(key_exprs());
    let unmatched = per_key
        .clone()
        .join(distinct_lookup, key_exprs(), key_exprs(), JoinArgs::new(JoinType::Anti))
        .sort(ks.clone(), SortMultipleOptions::default())
        .collect()?;
    cancel()?;
    out.value_column = value.clone();
    for i in 0..unmatched.height() {
        let rows = unmatched.column(ROWS).ok().and_then(|c| c.get(i).ok()).and_then(|v| v.extract::<u64>()).unwrap_or(0) as usize;
        let sum = if value.is_some() {
            let non_blank = unmatched.column(VALUE_N).ok().and_then(|c| c.get(i).ok()).and_then(|v| v.extract::<u64>()).unwrap_or(0);
            if non_blank == 0 { None } else { unmatched.column(VALUE).ok().and_then(|c| c.get(i).ok()).and_then(|v| v.extract::<f64>()) }
        } else {
            None
        };
        out.unmatched_rows += rows;
        out.unmatched.push(UnmatchedKey { key: ks.iter().map(|k| text_at(&unmatched, k, i)).collect(), rows, sum });
    }
    let keyed_rows = out.data_rows - out.data_blank_keys.min(out.data_rows);
    out.matched_rows = keyed_rows.saturating_sub(out.unmatched_rows);
    // lookup rows no data row uses
    let used = per_key.select(key_exprs());
    out.unused_lookup_rows = first_count(
        &l.filter(col(BLANK).not())
            .join(used, key_exprs(), key_exprs(), JoinArgs::new(JoinType::Anti))
            .select([len()])
            .collect()?,
    );
    Ok(out)
}

/// The lookup rows whose key repeats, sorted by key, with a "Times" column (how many rows share the key).
pub fn duplicate_rows(link: &Link, lookup: &ModelTab, temp_dir: &Path) -> Result<DataFrame> {
    let ll = compile_tab_lazy(&lookup.source, &lookup.steps, temp_dir).with_context(|| format!("'{}' could not be read", lookup.title))?;
    let mut probe = ll.clone();
    let schema = probe.collect_schema()?;
    let mut string_cols = Vec::new();
    for k in &link.keys {
        let dt = schema.get(k.lookup.as_str()).ok_or_else(|| anyhow!("'{}' has no column '{}'", lookup.title, k.lookup))?;
        string_cols.push(*dt == DataType::String);
    }
    let names: Vec<String> = schema.iter().map(|(n, _)| n.to_string()).collect();
    let lookup_cols: Vec<String> = link.keys.iter().map(|k| k.lookup.clone()).collect();
    let ks = key_names(link.keys.len());
    let key_exprs = || ks.iter().map(|k| col(k.as_str())).collect::<Vec<_>>();
    let l = keyed(ll, &lookup_cols, &string_cols).filter(col(BLANK).not());
    let counts = l.clone().group_by_stable(key_exprs()).agg([len().alias(ROWS)]).filter(col(ROWS).gt(lit(1)));
    let mut times = "Times".to_string();
    while names.contains(&times) {
        times.push('_');
    }
    let joined = l.join(counts, key_exprs(), key_exprs(), JoinArgs::new(JoinType::Inner));
    let mut sort_by: Vec<String> = ks.clone();
    sort_by.extend(std::iter::empty());
    let mut select: Vec<Expr> = names.iter().map(|n| col(n.as_str())).collect();
    select.push(col(ROWS).alias(times.as_str()));
    let df = joined.sort(sort_by, SortMultipleOptions::default().with_maintain_order(true)).select(select).collect().context("The duplicate keys could not be listed")?;
    Ok(df)
}

/// The data rows whose key has no row in the lookup (a blank key is not in the list: it is counted on its own).
pub fn unmatched_rows(link: &Link, data: &ModelTab, lookup: &ModelTab, temp_dir: &Path) -> Result<DataFrame> {
    let (dl, ll, ds, ls) = open_pair(link, data, lookup, temp_dir)?;
    let mut probe = dl.clone();
    let names: Vec<String> = probe.collect_schema()?.iter().map(|(n, _)| n.to_string()).collect();
    let (dl, data_cols) = data_key_columns(dl, link);
    let lookup_cols: Vec<String> = link.keys.iter().map(|k| k.lookup.clone()).collect();
    let ks = key_names(link.keys.len());
    let key_exprs = || ks.iter().map(|k| col(k.as_str())).collect::<Vec<_>>();
    let d = keyed(dl, &data_cols, &ds).filter(col(BLANK).not());
    let l = keyed(ll, &lookup_cols, &ls).filter(col(BLANK).not()).select(key_exprs()).unique_stable(None, UniqueKeepStrategy::First);
    d.join(l, key_exprs(), key_exprs(), JoinArgs::new(JoinType::Anti))
        .select(names.iter().map(|n| col(n.as_str())).collect::<Vec<_>>())
        .collect()
        .context("The unmatched rows could not be listed")
}

/// The unmatched keys as text to paste: one line per key with its rows and the sum, tab separated, headed by the data columns.
pub fn unmatched_list_text(link: &Link, check: &KeyCheck) -> String {
    let mut head: Vec<String> = link.keys.iter().map(|k| k.data.clone()).collect();
    head.push("Rows".to_string());
    if let Some(v) = &check.value_column {
        head.push(format!("Sum of {}", v));
    }
    let mut out = head.join("\t");
    out.push('\n');
    for u in &check.unmatched {
        let mut cells = u.key.clone();
        cells.push(u.rows.to_string());
        if check.value_column.is_some() {
            cells.push(u.sum.map(|s| format!("{}", s)).unwrap_or_default());
        }
        out.push_str(&cells.join("\t"));
        out.push('\n');
    }
    out
}

/// Where the Key check says to look for what the link joins: "Wing + Product code ↔ Wing + Code".
pub fn pairs_text(l: &Link) -> String {
    pair_list(&l.keys)
}

// ----------------------------------------------------------------------------------------------- lookup fields (Phase 9 M1)

/// The value of every lookup field for a data row whose key has no row in the lookup.
pub const NO_MATCH_LABEL: &str = "(no match)";

/// "Branch master · Region": the stored name of a lookup field.
pub fn field_name(lookup_title: &str, column: &str) -> String {
    format!("{} \u{b7} {}", lookup_title, column)
}

/// "Branch master › Region": how the pickers list a lookup's column under its link.
pub fn picker_label(lookup_title: &str, column: &str) -> String {
    format!("{} \u{203a} {}", lookup_title, column)
}

/// A lookup column seen from a data tab (a "lookup field"): a pivot's Rows, Columns, Filters or Values field, a chart's category,
/// series, facet or drill level, a filter condition. The link is named by the two tabs' project keys; a data tab links to a given
/// lookup once, so the pair says which link. It carries the link's name for the messages that have no project to read it from.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct ModelFieldRef {
    /// The field's name in the spec ("Branch master · Region").
    #[serde(default)]
    pub name: String,
    /// The data tab's project key (the link's own side).
    #[serde(default)]
    pub data_tab: String,
    /// The lookup tab's project key.
    #[serde(default)]
    pub lookup_tab: String,
    /// "Accounts → Branch master", for messages.
    #[serde(default)]
    pub link_name: String,
    /// The lookup's column.
    #[serde(default)]
    pub column: String,
}

impl ModelFieldRef {
    pub fn new(link: &Link, link_name: &str, lookup_title: &str, column: &str) -> ModelFieldRef {
        ModelFieldRef { name: field_name(lookup_title, column), data_tab: link.data_tab.clone(), lookup_tab: link.lookup_tab.clone(), link_name: link_name.to_string(), column: column.to_string() }
    }

    /// "Branch master › Region".
    pub fn picker(&self) -> String {
        self.name.replacen(" \u{b7} ", " \u{203a} ", 1)
    }
}

/// Where a label of a lookup field sorts (M1): the values first (0), then "(blank)" (1), then "(no match)" (2), whatever the order.
pub fn label_rank(label: &str) -> u8 {
    if label == crate::pivot_table::BLANK_LABEL {
        1
    } else if label == NO_MATCH_LABEL {
        2
    } else {
        0
    }
}

/// What a step that needs the model says outside a project.
pub fn outside_project_message(link_name: &str) -> String {
    format!("This step uses the model of a project (link '{}'). Open the project to run it.", link_name)
}

/// What the Python export says about a step that reads a lookup field.
pub const PYTHON_REFUSAL: &str = "Python export can't follow links between tabs yet. Use the Join tool to copy the column, or export the result tab.";

/// A link as the worker sees it while it builds a table: the link, the lookup read after its own steps, and why it can't be used.
#[derive(Clone, Debug)]
pub struct ScopeLink {
    pub link: Link,
    /// `None` when the lookup tab is not open (then `refusal` says so).
    pub lookup: Option<ModelTab>,
    /// Set when the link is broken or its check can't run: what a lookup field of it says instead of numbers.
    pub refusal: Option<String>,
}

/// The model as the worker needs it: its links and where to put temporary files.
#[derive(Clone, Debug, Default)]
pub struct ModelContext {
    pub links: Vec<ScopeLink>,
    pub temp_dir: std::path::PathBuf,
}

thread_local! {
    static SCOPE: std::cell::RefCell<Option<std::sync::Arc<ModelContext>>> = const { std::cell::RefCell::new(None) };
}

/// The model of the open project, as this thread sees it (`None` outside a project). The app sets it on its own thread each
/// frame, and a job takes the one current when it is submitted (`jobs.rs`), so every table a job builds reads the same model.
pub fn scope() -> Option<std::sync::Arc<ModelContext>> {
    SCOPE.with(|s| s.borrow().clone())
}

pub fn set_scope(ctx: Option<std::sync::Arc<ModelContext>>) {
    SCOPE.with(|s| *s.borrow_mut() = ctx);
}

/// Runs `f` with `ctx` as this thread's model, and puts the old one back.
pub fn with_scope<R>(ctx: Option<std::sync::Arc<ModelContext>>, f: impl FnOnce() -> R) -> R {
    let old = scope();
    set_scope(ctx);
    let r = f();
    set_scope(old);
    r
}

impl LinkStatus {
    /// What a lookup field of this link says when the link can't be used; `None` when it can.
    pub fn refusal(&self, lookup_title: &str) -> Option<String> {
        if self.is_broken() {
            Some(broken_message(lookup_title, self))
        } else {
            None
        }
    }
}

/// How a lookup field becomes a column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupAs {
    /// Text, with "(no match)" for a key with no lookup row and a blank for a blank key or a blank value (a label: Rows, Columns,
    /// Filters, a chart's category, series, facet and drill level).
    Text,
    /// The lookup's own value, a blank where there is no match (a Values field of a pivot, which sums and counts).
    Raw,
    /// Text as above for a text column; the lookup's own value for any other (a filter condition, which compares numbers as numbers).
    TextIfText,
}

/// One lookup field to add: the field, the column it is added as, and how.
#[derive(Clone, Debug)]
pub struct LookupWant {
    pub field: ModelFieldRef,
    pub out: String,
    pub as_: LookupAs,
    /// A column of true/false that is true for a data row whose key is not blank and has no row in the lookup (a filter's
    /// "(no match)", which is never a value).
    pub unmatched_out: Option<String>,
}

const HIT: &str = "__mp_hit";
const FIELD: &str = "__mp_f";

/// Adds lookup fields to a table by a left join on the link's keys (the W2 pattern: nothing is added to the tab, the caller drops
/// the columns after it has grouped or filtered). A link is used only while its lookup keys are unique (the key check), and the
/// join itself refuses a repeated key, so the row count of `lf` can't change. `what` starts the messages ("Pivot", "Chart", "Filter").
pub fn with_lookup_columns(lf: LazyFrame, wants: &[LookupWant], what: &str) -> Result<LazyFrame> {
    if wants.is_empty() {
        return Ok(lf);
    }
    let mut lf = lf;
    let schema = lf.collect_schema().with_context(|| format!("{}: the column types could not be read", what))?;
    for w in wants {
        if schema.get(w.out.as_str()).is_some() {
            bail!("{}: the table already has a column called '{}'. Rename that column, or use it instead of the lookup field.", what, w.out);
        }
    }
    let ctx = scope();
    let mut ids: Vec<(String, String)> = Vec::new();
    for w in wants {
        let id = (w.field.data_tab.clone(), w.field.lookup_tab.clone());
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    for id in &ids {
        let group: Vec<&LookupWant> = wants.iter().filter(|w| (&w.field.data_tab, &w.field.lookup_tab) == (&id.0, &id.1)).collect();
        let link_name = group[0].field.link_name.clone();
        let Some(ctx) = ctx.as_ref() else {
            bail!("{}", outside_project_message(&link_name));
        };
        let Some(sl) = ctx.links.iter().find(|l| (&l.link.data_tab, &l.link.lookup_tab) == (&id.0, &id.1)) else {
            bail!("{}: the link '{}' is not in this project's model. Add it in Model \u{25b8} Links, or remove the field.", what, link_name);
        };
        if let Some(r) = &sl.refusal {
            bail!("{}", r);
        }
        let Some(lookup) = &sl.lookup else {
            bail!("{}: the lookup tab of the link '{}' is not open.", what, link_name);
        };
        lf = join_lookup(lf, &schema, sl, lookup, &group, ctx, what)?;
    }
    Ok(lf)
}

fn join_lookup(lf: LazyFrame, schema: &Schema, sl: &ScopeLink, lookup: &ModelTab, group: &[&LookupWant], ctx: &ModelContext, what: &str) -> Result<LazyFrame> {
    let link = &sl.link;
    let mut ll = compile_tab_lazy(&lookup.source, &lookup.steps, &ctx.temp_dir).with_context(|| format!("{}: '{}' could not be read", what, lookup.title))?;
    let lschema = ll.collect_schema().with_context(|| format!("{}: '{}' could not be read", what, lookup.title))?;
    let (mut ds, mut ls) = (Vec::new(), Vec::new());
    for k in &link.keys {
        let d = schema.get(k.data.as_str()).ok_or_else(|| anyhow!("{}: there is no column '{}' at this step, and the link '{}' needs it.", what, k.data, group[0].field.link_name))?;
        let l = lschema.get(k.lookup.as_str()).ok_or_else(|| anyhow!("{}: '{}' has no column '{}'. Open Model \u{25b8} Key check to see the link.", what, lookup.title, k.lookup))?;
        if !key_types_agree(d, l) {
            bail!("{}: '{}' is {} in the data and {} in '{}'. Set both to the same type in Schema & Ingestion first.", what, k.data, type_word(d), type_word(l), lookup.title);
        }
        ds.push(*d == DataType::String);
        ls.push(*l == DataType::String);
    }
    for w in group {
        if lschema.get(w.field.column.as_str()).is_none() {
            bail!("{}: '{}' has no column '{}'. Remove the field or pick another column.", what, lookup.title, w.field.column);
        }
    }
    let n = link.keys.len();
    let ks = key_names(n);
    let key_exprs = || ks.iter().map(|k| col(k.as_str())).collect::<Vec<_>>();
    let (lf, data_cols) = data_key_columns(lf, link);
    let lookup_cols: Vec<String> = link.keys.iter().map(|k| k.lookup.clone()).collect();
    let d = keyed(lf, &data_cols, &ds);
    let mut pick: Vec<Expr> = key_exprs();
    for (i, w) in group.iter().enumerate() {
        pick.push(col(w.field.column.as_str()).alias(format!("{}{}", FIELD, i)));
    }
    pick.push(lit(true).alias(HIT));
    // a blank lookup key never matches (M0-3), so those rows are left out of the join
    let l = keyed(ll, &lookup_cols, &ls).filter(col(BLANK).not()).select(pick);
    // a lookup whose keys repeat at the time of the join (the file or the lookup's steps changed after the check): refuse in the Key
    // check's words and tell the app, which re-checks the link and marks it Broken at once (M1-6)
    let repeated = keyed(compile_tab_lazy(&lookup.source, &lookup.steps, &ctx.temp_dir).with_context(|| format!("{}: '{}' could not be read", what, lookup.title))?, &lookup_cols, &ls)
        .filter(col(BLANK).not())
        .group_by(key_exprs())
        .agg([len().alias(ROWS)])
        .filter(col(ROWS).gt(lit(1)))
        .select([len()])
        .collect()
        .with_context(|| format!("{}: the keys of '{}' could not be checked", what, lookup.title))?;
    let repeated = first_count(&repeated);
    if repeated > 0 {
        let status = LinkStatus::Broken(vec![plural(repeated, "duplicate key", "duplicate keys")]);
        note_broken_at_join(&link.data_tab, &link.lookup_tab, status.clone());
        bail!("{}", broken_message(&lookup.title, &status));
    }
    let mut args = JoinArgs::new(JoinType::Left);
    args.validation = JoinValidation::ManyToOne;
    let joined = d.join(l, key_exprs(), key_exprs(), args);
    let mut exprs: Vec<Expr> = Vec::new();
    let mut temps: Vec<String> = ks.clone();
    temps.push(BLANK.to_string());
    temps.push(HIT.to_string());
    temps.extend(data_cols.iter().filter(|c| c.starts_with("__mp_dk")).cloned());
    for (i, w) in group.iter().enumerate() {
        let f = format!("{}{}", FIELD, i);
        let dt = lschema.get(w.field.column.as_str()).cloned().unwrap_or(DataType::String);
        let as_text = match w.as_ {
            LookupAs::Text => true,
            LookupAs::Raw => false,
            LookupAs::TextIfText => dt == DataType::String,
        };
        let e = if as_text {
            when(col(BLANK))
                .then(lit(NULL).cast(DataType::String))
                .when(col(HIT).is_null())
                .then(lit(NO_MATCH_LABEL))
                .otherwise(crate::pivot_table::key_text_expr(&f, &dt))
        } else {
            col(f.as_str())
        };
        exprs.push(e.alias(w.out.as_str()));
        if let Some(u) = &w.unmatched_out {
            exprs.push(col(BLANK).not().and(col(HIT).is_null()).alias(u.as_str()));
        }
        temps.push(f);
    }
    Ok(joined.with_columns(exprs).drop(temps.iter().map(|t| t.as_str()).collect::<Vec<_>>()))
}

static BROKEN_AT_JOIN: std::sync::Mutex<Vec<(String, String, LinkStatus)>> = std::sync::Mutex::new(Vec::new());

/// A join found repeated lookup keys: the link is to be marked Broken at once and checked again (taken by the app each frame).
fn note_broken_at_join(data_tab: &str, lookup_tab: &str, status: LinkStatus) {
    if let Ok(mut q) = BROKEN_AT_JOIN.lock() {
        q.push((data_tab.to_string(), lookup_tab.to_string(), status));
    }
}

/// The links joins found broken since the last call: (data tab key, lookup tab key, status).
pub fn take_broken_at_join() -> Vec<(String, String, LinkStatus)> {
    BROKEN_AT_JOIN.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}

/// The links a step needs when it reads a lookup field ("Accounts → Branch master"); empty for every other step.
pub fn step_links(step: &TransformStep) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |s: &str| {
        if !out.iter().any(|o| o == s) {
            out.push(s.to_string());
        }
    };
    match step.inner() {
        TransformStep::FilterGroup { conditions, .. } => {
            for c in conditions {
                if let Some(l) = &c.lookup {
                    add(&l.link_name);
                }
            }
        }
        TransformStep::PivotTable { .. } => {
            if let Some(spec) = step.as_pivot_spec() {
                for f in &spec.model_fields {
                    add(&f.link_name);
                }
            }
        }
        _ => {}
    }
    out
}
