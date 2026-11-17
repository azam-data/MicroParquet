//! Statistics of one column over the whole result (Phase 3 T5), for the Column inspector.

use anyhow::{Context, Result};
use polars::prelude::*;

/// How many bars the distribution has.
pub const HISTOGRAM_BINS: usize = 10;
/// How many of the most frequent text values are listed.
pub const TOP_VALUES: usize = 5;

#[derive(Clone, Debug, PartialEq)]
pub enum ColumnKind {
    Number,
    Date,
    Text,
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NumberStats {
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub distinct: u64,
    /// Counts per bin between `min` and `max` (the maximum falls in the last bin); they add up
    /// to the number of non-null values.
    pub histogram: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DateStats {
    /// Days since 1970-01-01.
    pub min: i32,
    pub max: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextStats {
    pub distinct: u64,
    /// The most frequent values and their counts, most frequent first (ties in text order).
    pub top: Vec<(String, u64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnStats {
    pub column: String,
    /// The type as Polars names it ("Float64", "String", "Date").
    pub dtype: String,
    pub kind: ColumnKind,
    /// All rows of the result.
    pub rows: u64,
    /// Rows with no value: null, or text that is empty after trimming.
    pub blanks: u64,
    pub number: Option<NumberStats>,
    pub date: Option<DateStats>,
    pub text: Option<TextStats>,
    /// Identifies the steps these numbers belong to (a result for older steps is stale).
    pub key: u64,
}

fn blank_text(name: &str) -> Expr {
    col(name).is_null().or(col(name).str().strip_chars(lit(NULL)).eq(lit("")))
}

fn first_f64(df: &DataFrame, name: &str) -> Option<f64> {
    df.column(name).ok().and_then(|s| s.get(0).ok()).and_then(|v| match v {
        AnyValue::Float64(x) if x.is_finite() => Some(x),
        AnyValue::Float32(x) if x.is_finite() => Some(x as f64),
        _ => None,
    })
}

fn first_u64(df: &DataFrame, name: &str) -> u64 {
    df.column(name)
        .ok()
        .and_then(|s| s.cast(&DataType::UInt64).ok())
        .and_then(|s| s.u64().ok().and_then(|c| c.get(0)))
        .unwrap_or(0)
}

/// Statistics of `column` over everything `lf` produces. Numbers: sum, min, max, mean, distinct
/// count and a 10-bin histogram. Dates: min and max. Text: distinct count and the 5 most
/// frequent values. Blank text counts as a blank, not as a value.
pub fn compute_column_stats(lf: LazyFrame, column: &str, key: u64) -> Result<ColumnStats> {
    let mut lf = lf;
    let schema = lf.collect_schema().context("Could not read the column types for the inspector")?;
    let dt = schema
        .get(column)
        .ok_or_else(|| anyhow::anyhow!("Column '{}' is not in the result.", column))?
        .clone();
    let kind = if dt.is_numeric() {
        ColumnKind::Number
    } else if matches!(dt, DataType::Date | DataType::Datetime(_, _)) {
        ColumnKind::Date
    } else if dt.is_string() || dt.is_categorical() {
        ColumnKind::Text
    } else {
        ColumnKind::Other
    };
    let is_text = kind == ColumnKind::Text;
    let c = col(column);

    let mut exprs = vec![len().cast(DataType::UInt64).alias("__rows")];
    exprs.push(if is_text {
        blank_text(column).cast(DataType::UInt64).sum().alias("__blanks")
    } else {
        c.clone().is_null().cast(DataType::UInt64).sum().alias("__blanks")
    });
    match kind {
        ColumnKind::Number => {
            let f = c.clone().cast(DataType::Float64);
            exprs.push(f.clone().sum().alias("__sum"));
            exprs.push(f.clone().min().alias("__min"));
            exprs.push(f.clone().max().alias("__max"));
            exprs.push(f.clone().mean().alias("__mean"));
            exprs.push(f.drop_nulls().n_unique().cast(DataType::UInt64).alias("__distinct"));
        }
        ColumnKind::Date => {
            exprs.push(c.clone().cast(DataType::Date).min().cast(DataType::Int32).alias("__dmin"));
            exprs.push(c.clone().cast(DataType::Date).max().cast(DataType::Int32).alias("__dmax"));
        }
        ColumnKind::Text => {
            let non_blank = when(blank_text(column)).then(lit(NULL).cast(DataType::String)).otherwise(c.clone());
            exprs.push(non_blank.drop_nulls().n_unique().cast(DataType::UInt64).alias("__distinct"));
        }
        ColumnKind::Other => {}
    }
    let agg = lf.clone().select(exprs).collect().context("Failed to compute the column statistics")?;
    let rows = first_u64(&agg, "__rows");
    let blanks = first_u64(&agg, "__blanks");

    let mut stats = ColumnStats {
        column: column.to_string(),
        dtype: format!("{}", dt),
        kind: kind.clone(),
        rows,
        blanks,
        number: None,
        date: None,
        text: None,
        key,
    };

    match kind {
        ColumnKind::Number => {
            let non_null = rows.saturating_sub(blanks);
            if non_null > 0 {
                let (min, max) = (first_f64(&agg, "__min").unwrap_or(0.0), first_f64(&agg, "__max").unwrap_or(0.0));
                let histogram = histogram(lf.clone(), column, min, max)?;
                stats.number = Some(NumberStats {
                    sum: first_f64(&agg, "__sum").unwrap_or(0.0),
                    min,
                    max,
                    mean: first_f64(&agg, "__mean").unwrap_or(0.0),
                    distinct: first_u64(&agg, "__distinct"),
                    histogram,
                });
            }
        }
        ColumnKind::Date => {
            let get = |n: &str| {
                agg.column(n)
                    .ok()
                    .and_then(|s| s.cast(&DataType::Int32).ok())
                    .and_then(|s| s.i32().ok().and_then(|c| c.get(0)))
            };
            if let (Some(min), Some(max)) = (get("__dmin"), get("__dmax")) {
                stats.date = Some(DateStats { min, max });
            }
        }
        ColumnKind::Text => {
            let top = top_values(lf, column)?;
            stats.text = Some(TextStats { distinct: first_u64(&agg, "__distinct"), top });
        }
        ColumnKind::Other => {}
    }
    Ok(stats)
}

/// Counts per bin: `HISTOGRAM_BINS` equal bins from `min` to `max`; when all values are equal
/// they all fall in the first bin. The counts add up to the non-null values.
fn histogram(lf: LazyFrame, column: &str, min: f64, max: f64) -> Result<Vec<u64>> {
    let mut bins = vec![0u64; HISTOGRAM_BINS];
    let span = max - min;
    let bin_expr = if span > 0.0 && span.is_finite() {
        ((col(column).cast(DataType::Float64) - lit(min)) / lit(span) * lit(HISTOGRAM_BINS as f64))
            .floor()
            .clip(lit(0.0), lit((HISTOGRAM_BINS - 1) as f64))
            .cast(DataType::Int64)
    } else {
        // every value is the same: all in the first bin (zero for each row, not one literal)
        col(column).is_not_null().cast(DataType::Int64) - lit(1i64)
    };
    let df = lf
        .filter(col(column).is_not_null())
        .select([bin_expr.alias("__bin")])
        .group_by([col("__bin")])
        .agg([len().cast(DataType::UInt64).alias("__n")])
        .collect()
        .context("Failed to compute the histogram")?;
    let b = df.column("__bin")?.cast(&DataType::Int64)?;
    let n = df.column("__n")?.cast(&DataType::UInt64)?;
    for (bin, count) in b.i64()?.into_iter().zip(n.u64()?.into_iter()) {
        if let (Some(bin), Some(count)) = (bin, count) {
            if let Some(slot) = bins.get_mut(bin.clamp(0, HISTOGRAM_BINS as i64 - 1) as usize) {
                *slot += count;
            }
        }
    }
    Ok(bins)
}

fn top_values(lf: LazyFrame, column: &str) -> Result<Vec<(String, u64)>> {
    let df = lf
        .filter(blank_text(column).not())
        .group_by([col(column)])
        .agg([len().cast(DataType::UInt64).alias("__n")])
        .sort_by_exprs(
            [col("__n"), col(column)],
            SortMultipleOptions::default().with_order_descending_multi([true, false]).with_maintain_order(true),
        )
        .limit(TOP_VALUES as IdxSize)
        .collect()
        .context("Failed to find the most frequent values")?;
    let v = df.column(column)?.cast(&DataType::String)?;
    let n = df.column("__n")?.cast(&DataType::UInt64)?;
    Ok(v.str()?
        .into_iter()
        .zip(n.u64()?.into_iter())
        .filter_map(|(v, n)| Some((v?.to_string(), n?)))
        .collect())
}
