//! The values of one column with their row counts, for the filter's value picker (Phase 6 U7):
//! calculated over the whole tab as it stands before the filter, never just the page on screen.

use crate::pivot_table::key_text_expr;
use anyhow::{bail, Context, Result};
use polars::prelude::*;

/// How many values the picker lists at most; a column with more shows the most frequent ones and
/// the search asks the worker for the rest.
pub const PICKER_LIMIT: usize = 1000;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct PickerValues {
    /// The value as text (`None` = blank: null, or text of only spaces) and the rows that have it,
    /// the most frequent first.
    pub values: Vec<(Option<String>, u64)>,
    /// How many distinct values match (after the search), whatever `limit` cut.
    pub distinct: usize,
    /// There are more than `values` lists.
    pub more: bool,
}

/// The distinct values of `field` with their counts. With `search` only the values that contain
/// the text (not case-sensitive; "(blank)" finds the blanks); at most `limit` of them, the most
/// frequent first.
pub fn picker_values(lf: LazyFrame, field: &str, search: Option<&str>, limit: usize) -> Result<PickerValues> {
    let mut lf = lf;
    let schema = lf.collect_schema().context("Could not read the column types")?;
    let Some(dt) = schema.get(field).cloned() else {
        bail!("Column '{}' was not found.", field);
    };
    let text = key_text_expr(field, &dt);
    let df = lf
        .select([text.alias("__k")])
        .group_by([col("__k")])
        .agg([len().cast(DataType::Int64).alias("__n")])
        .collect()
        .with_context(|| format!("Could not list the values of '{}'", field))?;
    let keys: Vec<Option<String>> = df.column("__k")?.str()?.into_iter().map(|v| v.map(str::to_string)).collect();
    let counts: Vec<u64> = df.column("__n")?.cast(&DataType::Int64)?.i64()?.into_iter().map(|v| v.unwrap_or(0).max(0) as u64).collect();
    let needle = search.map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
    let mut values: Vec<(Option<String>, u64)> = keys
        .into_iter()
        .zip(counts)
        .filter(|(k, _)| match &needle {
            None => true,
            Some(n) => match k {
                Some(t) => t.to_lowercase().contains(n.as_str()),
                None => "(blank)".contains(n.as_str()),
            },
        })
        .collect();
    values.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let distinct = values.len();
    let more = distinct > limit;
    values.truncate(limit);
    Ok(PickerValues { values, distinct, more })
}

/// "3 of 42 selected" (or "3 selected" before the list is known); "(blank)" counts as one.
pub fn picker_summary(selected: usize, blank: bool, total: Option<usize>) -> String {
    let n = selected + usize::from(blank);
    match total {
        Some(t) => format!("{} of {} selected", n, crate::models::format_with_commas(t as f64, 0)),
        None if n == 0 => "pick values".to_string(),
        None => format!("{} selected", n),
    }
}
