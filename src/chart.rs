//! The chart behind View ▸ Chart (Phase 6 U5): a category, an optional series, a value and an
//! aggregation, calculated over the whole tab after its steps (never just the page on screen).
//! The drawer shows the result; "Open chart data as tab" puts the table in a tab.

use crate::pivot_table::{key_text_expr, BLANK_LABEL};
use crate::sort_lists::SortOrder;
use anyhow::{bail, Context, Result};
use polars::prelude::*;
use std::cmp::Ordering;

/// The label of the categories (or series) that are not among the top ones.
pub const OTHER_LABEL: &str = "Other";
/// At most this many series are drawn (the rest are one series called "Other").
pub const MAX_SERIES: usize = 8;
pub use crate::charts::data::ChartData;
pub use crate::charts::spec::{ChartAgg, ChartKind, ChartScale, ChartSpec, DEFAULT_TOP_N};

/// A column the chart can add up: integers, floats and decimals.
pub(crate) fn is_number(dt: &DataType) -> bool {
    dt.is_numeric() || matches!(dt, DataType::Decimal(..))
}

/// The value-column check with the column's type in the message.
pub(crate) fn type_word(dt: &DataType) -> String {
    if dt.is_string() {
        "Text".to_string()
    } else if matches!(dt, DataType::Date | DataType::Datetime(_, _)) {
        "a Date".to_string()
    } else {
        format!("{}", dt)
    }
}

/// Stops with a plain message when the settings do not fit the columns.
pub fn validate_chart(schema: &Schema, spec: &ChartSpec) -> Result<()> {
    if spec.category.trim().is_empty() {
        bail!("Chart: choose the Category (X) column.");
    }
    if schema.get(spec.category.as_str()).is_none() {
        bail!("Chart: the category column '{}' was not found. Choose another column.", spec.category);
    }
    if let Some(s) = &spec.series {
        if schema.get(s.as_str()).is_none() {
            bail!("Chart: the series column '{}' was not found. Choose another column or none.", s);
        }
        if *s == spec.category {
            bail!("Chart: the Series column is the same as the Category column ('{}'). Choose another one or none.", s);
        }
    }
    if spec.agg.needs_value() {
        let Some(v) = spec.value.as_deref().filter(|v| !v.trim().is_empty()) else {
            bail!("Chart: choose the Value column for {} (or choose Count, which needs none).", spec.agg.label());
        };
        let Some(dt) = schema.get(v) else {
            bail!("Chart: the value column '{}' was not found. Choose another column.", v);
        };
        if !is_number(dt) {
            bail!(
                "Chart: {} needs a number column, but '{}' is {}. Set it to a number in Schema & Ingestion (or with Set to Number), choose another column, or choose Count.",
                spec.agg.label(),
                v,
                type_word(dt)
            );
        }
    }
    if spec.agg == ChartAgg::WeightedAvg {
        let Some(w) = spec.weight.as_deref().filter(|w| !w.trim().is_empty()) else {
            bail!("Chart: W.Avg needs a weight column. Choose one (usually Balance).");
        };
        let Some(dt) = schema.get(w) else {
            bail!("Chart: the weight column '{}' was not found. Choose another column.", w);
        };
        if !is_number(dt) {
            bail!("Chart: the weight column '{}' is {}, not a number.", w, type_word(dt));
        }
    }
    if spec.top_n == 0 {
        bail!("Chart: Top N must be at least 1.");
    }
    Ok(())
}

/// The aggregation of one group, over `__v` (and `__w`); `__n` is its row count.
fn agg_exprs(spec: &ChartSpec) -> (Expr, Option<Expr>) {
    match spec.agg {
        ChartAgg::Count => (len().cast(DataType::Float64), None),
        // a group whose values are all blank has no sum (blank, not 0); Count still counts the rows
        ChartAgg::Sum => (when(col("__v").count().gt(lit(0))).then(col("__v").sum().cast(DataType::Float64)).otherwise(lit(NULL).cast(DataType::Float64)), None),
        ChartAgg::Average => (col("__v").mean().cast(DataType::Float64), None),
        ChartAgg::Min => (col("__v").min().cast(DataType::Float64), None),
        ChartAgg::Max => (col("__v").max().cast(DataType::Float64), None),
        ChartAgg::WeightedAvg => {
            let (rate, mixed) = crate::banking::weighted_rate_exprs(col("__v"), col("__w"));
            (rate.cast(DataType::Float64), Some(mixed))
        }
    }
}

/// The data with `__c` (category text), `__s` (series text), `__v` and `__w` (numbers).
fn working(lf: LazyFrame, schema: &Schema, spec: &ChartSpec) -> LazyFrame {
    let text = |name: &str| {
        let dt = schema.get(name).cloned().unwrap_or(DataType::String);
        key_text_expr(name, &dt).fill_null(lit(BLANK_LABEL))
    };
    let mut exprs = vec![text(&spec.category).alias("__c")];
    exprs.push(match &spec.series {
        Some(s) => text(s).alias("__s"),
        None => lit("").alias("__s"),
    });
    exprs.push(match (&spec.value, spec.agg.needs_value()) {
        (Some(v), true) => col(v.as_str()).cast(DataType::Float64).alias("__v"),
        _ => lit(NULL).cast(DataType::Float64).alias("__v"),
    });
    exprs.push(match (&spec.weight, spec.agg == ChartAgg::WeightedAvg) {
        (Some(w), true) => col(w.as_str()).cast(DataType::Float64).alias("__w"),
        _ => lit(NULL).cast(DataType::Float64).alias("__w"),
    });
    lf.select(exprs)
}

fn texts(df: &DataFrame, name: &str) -> Result<Vec<String>> {
    Ok(df.column(name)?.str()?.into_iter().map(|v| v.unwrap_or("").to_string()).collect())
}

fn numbers(df: &DataFrame, name: &str) -> Result<Vec<Option<f64>>> {
    Ok(df.column(name)?.cast(&DataType::Float64)?.f64()?.into_iter().collect())
}

/// The categories and series another measure of the same chart kept, so every measure folds the same ones into "Other".
#[derive(Clone, Debug, Default)]
pub struct FixedKept {
    pub categories: Vec<String>,
    pub series: Vec<String>,
}

/// Calculates the chart over all rows of `lf`.
pub fn compute_chart(lf: LazyFrame, spec: &ChartSpec) -> Result<ChartData> {
    compute_chart_fixed(lf, spec, None)
}

/// `compute_chart` with the kept categories and series given (the second and later measures of a chart).
pub fn compute_chart_fixed(lf: LazyFrame, spec: &ChartSpec, fixed: Option<&FixedKept>) -> Result<ChartData> {
    let mut lf = lf;
    let schema = lf.collect_schema().context("Chart: could not read the columns of the tab")?;
    validate_chart(&schema, spec)?;
    let work = working(lf, &schema, spec);
    let (value, mixed) = agg_exprs(spec);
    let mut notes = Vec::new();

    // 1. The categories (and series) with their own totals: which are the top ones, and in what order.
    let by_category = work
        .clone()
        .group_by([col("__c")])
        .agg([value.clone().alias("__a")])
        .collect()
        .context("Chart: could not add up the categories")?;
    let cat_names = texts(&by_category, "__c")?;
    let cat_values = numbers(&by_category, "__a")?;
    let distinct_categories = cat_names.len();
    // largest first (a blank total last): these are the categories that stay
    // (by the size of the value, whatever its sign: a large negative total is a top category)
    let mut ranked: Vec<usize> = (0..cat_names.len()).collect();
    ranked.sort_by(|&a, &b| match (cat_values[a], cat_values[b]) {
        (Some(x), Some(y)) => y.abs().partial_cmp(&x.abs()).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
    .then_with(|| cat_names[a].cmp(&cat_names[b])));
    let keep_n = spec.top_n.max(1);
    let kept: Vec<String> = match fixed {
        Some(f) => f.categories.clone(),
        None => ranked.iter().take(keep_n).map(|&i| cat_names[i].clone()).collect(),
    };
    let folded_categories = distinct_categories.saturating_sub(kept.len());
    let by_value_of: std::collections::HashMap<String, f64> = ranked.iter().filter_map(|&i| cat_values[i].map(|v| (cat_names[i].clone(), v))).collect();
    let other_cat = if kept.iter().any(|k| k == OTHER_LABEL) { "Other categories".to_string() } else { OTHER_LABEL.to_string() };

    let series_kept: Vec<String> = if let (Some(f), true) = (fixed, spec.series.is_some()) {
        f.series.clone()
    } else if spec.series.is_some() {
        let by_series = work
            .clone()
            .group_by([col("__s")])
            .agg([value.clone().alias("__a")])
            .collect()
            .context("Chart: could not add up the series")?;
        let names = texts(&by_series, "__s")?;
        let vals = numbers(&by_series, "__a")?;
        let mut idx: Vec<usize> = (0..names.len()).collect();
        idx.sort_by(|&a, &b| match (vals[a], vals[b]) {
            (Some(x), Some(y)) => y.abs().partial_cmp(&x.abs()).unwrap_or(Ordering::Equal),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
        .then_with(|| names[a].cmp(&names[b])));
        if names.len() > MAX_SERIES {
            notes.push(format!("The {} series after the first {} are drawn together as \"{}\".", names.len() - (MAX_SERIES - 1), MAX_SERIES - 1, OTHER_LABEL));
            idx.iter().take(MAX_SERIES - 1).map(|&i| names[i].clone()).collect()
        } else {
            idx.iter().map(|&i| names[i].clone()).collect()
        }
    } else {
        vec![String::new()]
    };
    // Q1: a series order chosen for the chart lists the kept series in it (the series that stay are still the largest)
    let mut series_kept = series_kept;
    if let (None, Some(order), true) = (fixed, spec.series_order.as_ref(), spec.series.is_some()) {
        series_kept.sort_by(|a, b| order.compare(&(if a == BLANK_LABEL { None } else { Some(a.clone()) }), &(if b == BLANK_LABEL { None } else { Some(b.clone()) })));
    }
    if spec.series_model.is_some() {
        series_kept.sort_by_key(|c| crate::model::label_rank(c));
    }
    let other_series = if series_kept.iter().any(|k| k == OTHER_LABEL) { "Other series".to_string() } else { OTHER_LABEL.to_string() };

    // 2. The cells: every row of the tab goes to its category (or "Other") and series (or "Other").
    let kept_cat = Series::new("kept_c".into(), kept.clone());
    let kept_ser = Series::new("kept_s".into(), series_kept.clone());
    let c2 = when(col("__c").is_in(lit(kept_cat))).then(col("__c")).otherwise(lit(other_cat.clone())).alias("__c2");
    let s2 = if spec.series.is_some() { when(col("__s").is_in(lit(kept_ser))).then(col("__s")).otherwise(lit(other_series.clone())).alias("__s2") } else { col("__s").alias("__s2") };
    let mut aggs = vec![value.alias("__a"), len().cast(DataType::Int64).alias("__n")];
    if let Some(m) = mixed {
        aggs.push(m.cast(DataType::Int64).alias("__m"));
    }
    let cells = work
        .with_columns([c2, s2])
        .group_by([col("__c2"), col("__s2")])
        .agg(aggs)
        .collect()
        .context("Chart: could not add up the chart")?;
    let (cc, ss) = (texts(&cells, "__c2")?, texts(&cells, "__s2")?);
    let (aa, nn) = (numbers(&cells, "__a")?, cells.column("__n")?.cast(&DataType::Int64)?.i64()?.into_iter().map(|v| v.unwrap_or(0).max(0) as u64).collect::<Vec<u64>>());
    if let Ok(m) = cells.column("__m") {
        let n = m.cast(&DataType::Int64)?.i64()?.into_iter().flatten().filter(|v| *v > 0).count();
        if n > 0 {
            notes.push(format!("{} group(s) have weights of both signs, so their weighted average is left blank.", n));
        }
    }

    // 3. The order of the categories, and the matrix.
    let mut categories: Vec<String> = kept.clone();
    let has_other = folded_categories > 0 && cc.iter().any(|c| *c == other_cat);
    let key = |c: &String| -> Option<String> { if c == BLANK_LABEL { None } else { Some(c.clone()) } };
    if spec.by_value {
        categories.sort_by(|a, b| match (by_value_of.get(a), by_value_of.get(b)) {
            (Some(x), Some(y)) => y.partial_cmp(x).unwrap_or(Ordering::Equal),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
        .then_with(|| SortOrder::Natural.compare(&key(a), &key(b))));
    } else {
        categories.sort_by(|a, b| spec.order.compare(&key(a), &key(b)));
    }
    // a lookup field's "(blank)" and "(no match)" come last, in that order, whatever the order (Phase 9 M1)
    if spec.category_model.is_some() {
        categories.sort_by_key(|c| crate::model::label_rank(c));
    }
    if has_other {
        categories.push(other_cat.clone());
    }
    let mut series: Vec<String> = series_kept.clone();
    if spec.series.is_some() && cc.iter().zip(ss.iter()).any(|(_, s)| *s == other_series) {
        series.push(other_series.clone());
    }
    let mut values = vec![vec![None; categories.len()]; series.len()];
    let mut rows = vec![vec![0u64; categories.len()]; series.len()];
    let mut total_rows = 0u64;
    for i in 0..cc.len() {
        total_rows += nn[i];
        let (Some(ci), Some(si)) = (categories.iter().position(|c| *c == cc[i]), series.iter().position(|s| *s == ss[i])) else {
            continue;
        };
        values[si][ci] = aa[i];
        rows[si][ci] = nn[i];
    }
    Ok(ChartData { spec: spec.clone(), categories, series, values, rows, total_rows, distinct_categories, folded_categories, notes, kept_categories: kept, kept_series: series_kept, ..ChartData::default() })
}

/// The name of the value column of the chart's table: "Sum of Balance", "Count of rows".
pub fn chart_value_name(data: &ChartData) -> String {
    let s = &data.spec;
    match (&s.value, s.agg) {
        (_, ChartAgg::Count) | (None, _) => "Count of rows".to_string(),
        (Some(v), a) => format!("{} of {}", a.title_word(), v),
    }
}

/// The chart as a table, for "Open chart data as tab": one row per category and series.
pub fn chart_table(data: &ChartData) -> Result<DataFrame> {
    let s = &data.spec;
    let value_name = chart_value_name(data);
    let mut cats: Vec<String> = Vec::new();
    let mut sers: Vec<String> = Vec::new();
    let mut vals: Vec<Option<f64>> = Vec::new();
    let mut rows: Vec<i64> = Vec::new();
    for (j, name) in data.series.iter().enumerate() {
        for (i, c) in data.categories.iter().enumerate() {
            if data.rows[j][i] == 0 {
                continue;
            }
            cats.push(c.clone());
            sers.push(name.clone());
            vals.push(data.values[j][i]);
            rows.push(data.rows[j][i] as i64);
        }
    }
    let mut cols: Vec<Series> = vec![Series::new(s.category.as_str().into(), cats)];
    if let Some(sn) = &s.series {
        cols.push(Series::new(sn.as_str().into(), sers));
    }
    cols.push(Series::new(value_name.as_str().into(), vals));
    cols.push(Series::new("Rows".into(), rows));
    DataFrame::new(cols).context("Chart: could not make the table")
}
