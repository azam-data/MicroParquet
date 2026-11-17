//! The summary row (Phase 3 T3): one aggregation per column over the whole result, computed in a
//! single pass. Exports add the same values as a totals row.

use crate::banking::weighted_rate_exprs;
use crate::models::{
    format_metric_value, format_with_commas, ColumnFormatSpec, NumberDisplayFormat, SummaryAgg,
    SummarySpec, TabState,
};
use anyhow::{Context, Result};
use polars::prelude::*;
use std::collections::HashMap;

/// A computed summary value.
#[derive(Clone, Debug, PartialEq)]
pub enum SummaryValue {
    /// A whole number of rows or values.
    Count(u64),
    /// A sum, average, minimum, maximum or weighted average of numbers.
    Number(f64),
    /// A minimum or maximum date, as days since 1970-01-01.
    Date(i32),
    /// A minimum or maximum of text.
    Text(String),
}

/// One column's summary: what was asked, the value, and why there is none.
#[derive(Clone, Debug, PartialEq)]
pub struct SummaryCell {
    pub column: String,
    pub agg: SummaryAgg,
    pub weight_col: Option<String>,
    pub value: Option<SummaryValue>,
    /// The reason a chosen summary has no value (shown as a tooltip).
    pub note: Option<String>,
}

fn is_text(dt: &DataType) -> bool {
    dt.is_string() || dt.is_categorical()
}

fn is_date(dt: &DataType) -> bool {
    matches!(dt, DataType::Date | DataType::Datetime(_, _))
}

/// Name tokens that mark a rate-like number column (owner answer to Q5).
pub const RATE_NAME_TOKENS: [&str; 5] = ["rate", "pct", "percent", "yield", "spread"];

/// True when a column's name tokens (Phase 1 tokeniser: "IntRate", "Int_Rate", "Yield %")
/// include rate, pct, percent, yield or spread.
pub fn is_rate_like_name(col_name: &str) -> bool {
    crate::models::column_name_tokens(col_name)
        .iter()
        .any(|t| RATE_NAME_TOKENS.contains(&t.to_lowercase().as_str()))
}

/// The default summary of a column. Rate-like number columns (see [`is_rate_like_name`]) are a
/// weighted average by the pinned control column, or no summary when there is none (a sum of
/// rates is meaningless). Otherwise as [`default_agg_for`].
pub fn default_spec_for(dtype: &DataType, col_name: &str, control_col: Option<&str>) -> SummarySpec {
    if dtype.is_numeric() && is_rate_like_name(col_name) {
        return match control_col.filter(|c| *c != col_name) {
            Some(c) => SummarySpec { agg: SummaryAgg::WeightedAvg, weight_col: Some(c.to_string()) },
            None => SummarySpec { agg: SummaryAgg::None, weight_col: None },
        };
    }
    SummarySpec { agg: default_agg_for(dtype, col_name), weight_col: None }
}

/// The default summary without a control column: numbers sum, text counts, dates show their
/// earliest value. Columns named like IDs (account numbers, codes) count even when they are
/// stored as numbers: their sum is meaningless. Rate-like number columns have none.
pub fn default_agg_for(dtype: &DataType, col_name: &str) -> SummaryAgg {
    if dtype.is_numeric() && is_rate_like_name(col_name) {
        SummaryAgg::None
    } else if dtype.is_numeric() && !TabState::is_likely_id_column(col_name) {
        SummaryAgg::Sum
    } else if is_date(dtype) {
        SummaryAgg::Min
    } else {
        SummaryAgg::Count
    }
}

/// The summary of every column: the user's choice where there is one, else the default
/// (`control_col` = the tab's pinned control column, the weight of rate-like columns).
pub fn effective_specs(
    columns: &[(String, DataType)],
    overrides: &HashMap<String, SummarySpec>,
    control_col: Option<&str>,
) -> Vec<(String, SummarySpec)> {
    columns
        .iter()
        .map(|(name, dt)| {
            let spec = overrides
                .get(name)
                .cloned()
                .unwrap_or_else(|| default_spec_for(dt, name, control_col));
            (name.clone(), spec)
        })
        .collect()
}

/// A blank is a null, or text that is empty after trimming.
fn blank_text(name: &str) -> Expr {
    col(name).is_null().or(col(name).str().strip_chars(lit(NULL)).eq(lit("")))
}

/// A column with its blank text turned into nulls (other types unchanged).
fn non_blank(name: &str, dt: &DataType) -> Expr {
    if is_text(dt) {
        when(blank_text(name)).then(lit(NULL).cast(DataType::String)).otherwise(col(name))
    } else {
        col(name)
    }
}

/// Computes every chosen summary in one aggregation pass over `lf`. A summary that cannot be
/// computed (a sum of text, a weighted average without a weight, weights of mixed sign) has no
/// value and a note saying why; it is not an error.
pub fn compute_summary(lf: LazyFrame, specs: &[(String, SummarySpec)]) -> Result<Vec<SummaryCell>> {
    let mut lf = lf;
    let schema = lf.collect_schema().context("Could not read the column types for the summary")?;
    let mut cells: Vec<SummaryCell> = specs
        .iter()
        .map(|(name, spec)| SummaryCell {
            column: name.clone(),
            agg: spec.agg,
            weight_col: spec.weight_col.clone(),
            value: None,
            note: None,
        })
        .collect();

    let mut exprs: Vec<Expr> = Vec::new();
    // aliases per cell: (value alias, mixed-sign flag alias)
    let mut aliases: Vec<Option<(String, Option<String>)>> = vec![None; cells.len()];
    for (i, (name, spec)) in specs.iter().enumerate() {
        let Some(dt) = schema.get(name.as_str()) else {
            cells[i].note = Some(format!("Column '{}' is not in the result.", name));
            continue;
        };
        let alias = format!("__su_{i}");
        let numeric_only = |what: &str, cell: &mut SummaryCell| {
            cell.note = Some(format!("{} needs numbers; '{}' is {}.", what, name, dt));
        };
        match spec.agg {
            SummaryAgg::None => {}
            SummaryAgg::Count => {
                let c = non_blank(name, dt);
                exprs.push(c.is_not_null().cast(DataType::UInt64).sum().alias(&alias));
                aliases[i] = Some((alias, None));
            }
            SummaryAgg::CountDistinct => {
                exprs.push(non_blank(name, dt).drop_nulls().n_unique().cast(DataType::UInt64).alias(&alias));
                aliases[i] = Some((alias, None));
            }
            SummaryAgg::Sum if dt.is_numeric() => {
                exprs.push(col(name.as_str()).cast(DataType::Float64).sum().alias(&alias));
                aliases[i] = Some((alias, None));
            }
            SummaryAgg::Average if dt.is_numeric() => {
                exprs.push(col(name.as_str()).cast(DataType::Float64).mean().alias(&alias));
                aliases[i] = Some((alias, None));
            }
            SummaryAgg::Sum => numeric_only("Sum", &mut cells[i]),
            SummaryAgg::Average => numeric_only("Average", &mut cells[i]),
            SummaryAgg::Min => {
                exprs.push(non_blank(name, dt).min().alias(&alias));
                aliases[i] = Some((alias, None));
            }
            SummaryAgg::Max => {
                exprs.push(non_blank(name, dt).max().alias(&alias));
                aliases[i] = Some((alias, None));
            }
            SummaryAgg::WeightedAvg => {
                let w_name = spec.weight_col.as_deref().map(str::trim).filter(|w| !w.is_empty());
                match w_name {
                    None => {
                        cells[i].note = Some("Pick a weight column for the weighted average (Column tab).".into());
                    }
                    Some(w) if schema.get(w).is_none() => {
                        cells[i].note = Some(format!("Weight column '{}' is not in the result.", w));
                    }
                    Some(_) if !dt.is_numeric() => numeric_only("A weighted average", &mut cells[i]),
                    Some(w) => {
                        let (rate, mixed) = weighted_rate_exprs(
                            col(name.as_str()).cast(DataType::Float64),
                            col(w).cast(DataType::Float64),
                        );
                        let flag = format!("__sm_{i}");
                        exprs.push(rate.alias(&alias));
                        exprs.push(mixed.alias(&flag));
                        aliases[i] = Some((alias, Some(flag)));
                    }
                }
            }
        }
    }

    if exprs.is_empty() {
        return Ok(cells);
    }
    let df = lf.select(exprs).collect().context("Failed to compute the summary row")?;

    for (i, al) in aliases.iter().enumerate() {
        let Some((alias, flag)) = al else {
            continue;
        };
        let any = df.column(alias).ok().and_then(|s| s.get(0).ok());
        cells[i].value = any.and_then(|v| match v {
            AnyValue::Null => None,
            AnyValue::UInt64(n) => Some(SummaryValue::Count(n)),
            AnyValue::UInt32(n) => Some(SummaryValue::Count(n as u64)),
            AnyValue::Int64(n) if cells[i].agg == SummaryAgg::Count || cells[i].agg == SummaryAgg::CountDistinct => {
                Some(SummaryValue::Count(n.max(0) as u64))
            }
            AnyValue::Float64(x) if x.is_finite() => Some(SummaryValue::Number(x)),
            AnyValue::Float32(x) if x.is_finite() => Some(SummaryValue::Number(x as f64)),
            AnyValue::Int64(n) => Some(SummaryValue::Number(n as f64)),
            AnyValue::Int32(n) => Some(SummaryValue::Number(n as f64)),
            AnyValue::UInt8(n) => Some(SummaryValue::Number(n as f64)),
            AnyValue::UInt16(n) => Some(SummaryValue::Number(n as f64)),
            AnyValue::Int8(n) => Some(SummaryValue::Number(n as f64)),
            AnyValue::Int16(n) => Some(SummaryValue::Number(n as f64)),
            AnyValue::Date(d) => Some(SummaryValue::Date(d)),
            AnyValue::Datetime(v, unit, _) => {
                let per_day: i64 = match unit {
                    TimeUnit::Nanoseconds => 86_400_000_000_000,
                    TimeUnit::Microseconds => 86_400_000_000,
                    TimeUnit::Milliseconds => 86_400_000,
                };
                Some(SummaryValue::Date(v.div_euclid(per_day) as i32))
            }
            AnyValue::String(s) => Some(SummaryValue::Text(s.to_string())),
            AnyValue::StringOwned(s) => Some(SummaryValue::Text(s.to_string())),
            _ => None,
        });
        if let Some(flag) = flag {
            let mixed = df
                .column(flag)
                .ok()
                .and_then(|s| s.bool().ok().and_then(|b| b.get(0)))
                .unwrap_or(false);
            if mixed {
                cells[i].value = None;
                cells[i].note = Some(
                    "The weights are a mix of positive and negative amounts, so a weighted average is not a meaningful rate."
                        .to_string(),
                );
            } else if cells[i].value.is_none() {
                cells[i].note = Some("The weights add up to 0 or no row has both a value and a weight.".to_string());
            }
        } else if cells[i].value.is_none() && matches!(cells[i].agg, SummaryAgg::Average | SummaryAgg::Min | SummaryAgg::Max) {
            cells[i].note = Some("No values in this column.".to_string());
        }
    }
    Ok(cells)
}

/// The days-since-1970 value as `YYYY-MM-DD`.
pub fn date_text(days: i32) -> String {
    chrono::NaiveDate::from_ymd_opt(1970, 1, 1)
        .and_then(|e| e.checked_add_signed(chrono::Duration::days(days as i64)))
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// How a summary value is shown: counts with separators, numbers in the column's format and the
/// chosen scale, dates as YYYY-MM-DD, "—" when there is no value.
pub fn format_summary_value(
    cell: &SummaryCell,
    spec: Option<&ColumnFormatSpec>,
    num_format: NumberDisplayFormat,
    decimals: usize,
) -> String {
    match &cell.value {
        None => "\u{2014}".to_string(),
        Some(SummaryValue::Count(n)) => format_with_commas(*n as f64, 0),
        Some(SummaryValue::Number(x)) => format_metric_value(*x, &cell.column, spec, num_format, decimals),
        Some(SummaryValue::Date(d)) => date_text(*d),
        Some(SummaryValue::Text(t)) => t.clone(),
    }
}

/// True when the value is a money-like number a scale applies to (not a count or a date).
pub fn is_scalable(cell: &SummaryCell) -> bool {
    matches!(cell.value, Some(SummaryValue::Number(_)))
}
