//! The worker's chart calculation (Phase 7 C0): the Phase 6 aggregation extended to several measures,
//! a size encoding, small-multiple facets and the transforms (percent of total, running sum). It runs
//! over the whole tab after its steps and returns the `ChartData` every renderer reads.

use super::data::{apply_transform, BoxStat, ChartData, FacetData, Histogram, MeasureData, ScatterPoint};
use super::spec::{ChartKind, ChartSpec, MeasureSpec, Transform};
use super::tokens::{MAX_FACETS, MAX_POINTS, MAX_SCATTER_SERIES};
use crate::chart::{compute_chart_fixed, FixedKept, OTHER_LABEL};
use crate::pivot_table::{key_text_expr, BLANK_LABEL};
use anyhow::{bail, Context, Result};
use polars::prelude::*;

/// The name of a measure in legends and titles: its label, else "Sum of Balance" / "Count of rows".
pub fn measure_name(m: &MeasureSpec) -> String {
    if !m.label.trim().is_empty() {
        return m.label.clone();
    }
    match (&m.column, m.agg) {
        (_, crate::chart::ChartAgg::Count) | (None, _) => "Count of rows".to_string(),
        (Some(v), a) => format!("{} of {}", a.title_word(), v),
    }
}

/// One measure's numbers laid on the primary measure's categories and series (by name); a group the
/// measure does not have is blank with 0 rows.
fn align(primary: &ChartData, other: &ChartData, name: String) -> MeasureData {
    let mut values = vec![vec![None; primary.categories.len()]; primary.series.len()];
    let mut rows = vec![vec![0u64; primary.categories.len()]; primary.series.len()];
    for (si, s) in primary.series.iter().enumerate() {
        let Some(os) = other.series.iter().position(|x| x == s) else { continue };
        for (ci, c) in primary.categories.iter().enumerate() {
            let Some(oc) = other.categories.iter().position(|x| x == c) else { continue };
            values[si][ci] = other.values[os][oc];
            rows[si][ci] = other.rows[os][oc];
        }
    }
    MeasureData { name, values, rows }
}

/// A chart never draws a number that is not a number: an added-up value that is NaN or infinite stops the chart,
/// naming the column and the category (and series) it is in.
fn finite_or_stop(values: &[Vec<Option<f64>>], categories: &[String], series: &[String], column: &str) -> Result<()> {
    for (s, row) in values.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            if let Some(x) = v.filter(|x| !x.is_finite()) {
                let cat = categories.get(c).map(String::as_str).unwrap_or("?");
                let ser = series.get(s).filter(|n| !n.is_empty()).map(|n| format!(" and series '{}'", n)).unwrap_or_default();
                bail!(
                    "Chart: '{}' adds up to {} for the category '{}'{}. The column holds a value that is not a number (NaN) or is infinite, perhaps from a division by zero. Fix those rows or filter them out, then chart again.",
                    column,
                    if x.is_nan() { "NaN".to_string() } else if x > 0.0 { "infinity".to_string() } else { "minus infinity".to_string() },
                    cat,
                    ser
                );
            }
        }
    }
    Ok(())
}

/// Calculates the whole chart over all rows of `lf`: every measure, the size and the facets.
pub fn compute(lf: LazyFrame, spec: &ChartSpec) -> Result<ChartData> {
    let mut data = match spec.kind {
        ChartKind::Scatter | ChartKind::Bubble => compute_points(lf.clone(), spec)?,
        ChartKind::Histogram => compute_histogram(lf.clone(), spec)?,
        ChartKind::BoxPlot => compute_boxes(lf.clone(), spec)?,
        _ => compute_grid(lf.clone(), spec)?,
    };
    if let Some(f) = spec.facet.as_deref().filter(|f| !f.trim().is_empty()) {
        data.facets = compute_facets(lf, spec, f)?;
    }
    Ok(data)
}

/// The aggregated grid: categories by series, for one or several measures.
fn compute_grid(lf: LazyFrame, spec: &ChartSpec) -> Result<ChartData> {
    let measures = spec.measure_list();
    let mut notes: Vec<String> = Vec::new();
    // the primary measure decides the categories, their order and what is folded into "Other"
    let primary_spec = spec.for_measure(&measures[0]);
    let mut data = compute_chart_fixed(lf.clone(), &primary_spec, None)?;
    data.spec = spec.clone();
    data.measure_names = measures.iter().map(measure_name).collect();
    finite_or_stop(&data.values, &data.categories, &data.series, measures[0].column.as_deref().unwrap_or("Count of rows"))?;
    let fixed = FixedKept { categories: data.kept_categories.clone(), series: data.kept_series.clone() };

    for (k, m) in measures.iter().enumerate().skip(1) {
        let other = compute_chart_fixed(lf.clone(), &spec.for_measure(m), Some(&fixed)).with_context(|| format!("Chart: measure {} ({})", k + 1, measure_name(m)))?;
        notes.extend(other.notes.iter().filter(|n| !data.notes.contains(n)).cloned());
        finite_or_stop(&other.values, &other.categories, &other.series, m.column.as_deref().unwrap_or("Count of rows"))?;
        data.extra.push(align(&data, &other, measure_name(m)));
    }
    if let Some(size) = &spec.size {
        let other = compute_chart_fixed(lf.clone(), &spec.for_measure(size), Some(&fixed)).context("Chart: the size measure")?;
        finite_or_stop(&other.values, &other.categories, &other.series, size.column.as_deref().unwrap_or("Count of rows"))?;
        data.size = Some(align(&data, &other, measure_name(size)));
    }

    // the transforms of each measure
    for (k, m) in measures.iter().enumerate() {
        if m.transform == Transform::None {
            continue;
        }
        let src = if k == 0 { &data.values } else { &data.extra[k - 1].values };
        let (out, n) = apply_transform(src, m.transform);
        notes.extend(n);
        if k == 0 {
            data.values = out;
        } else {
            data.extra[k - 1].values = out;
        }
    }
    data.notes.extend(notes);
    Ok(data)
}

/// One chart table per value of the facet column (at most `MAX_FACETS`), largest first by rows.
fn compute_facets(lf: LazyFrame, spec: &ChartSpec, facet: &str) -> Result<Vec<FacetData>> {
    let mut lf = lf;
    let schema = lf.collect_schema().context("Chart: could not read the columns of the tab")?;
    let Some(dt) = schema.get(facet).cloned() else {
        bail!("Chart: the facet column '{}' was not found. Choose another column or none.", facet);
    };
    if facet == spec.category || spec.series.as_deref() == Some(facet) {
        bail!("Chart: the facet column '{}' is also used as the category or series. Choose another one or none.", facet);
    }
    let key = key_text_expr(facet, &dt).fill_null(lit(BLANK_LABEL));
    let counts = lf
        .clone()
        .select([key.clone().alias("__f")])
        .group_by([col("__f")])
        .agg([len().cast(DataType::Int64).alias("__n")])
        .collect()
        .context("Chart: could not list the values of the facet column")?;
    let names: Vec<String> = counts.column("__f")?.str()?.into_iter().map(|v| v.unwrap_or("").to_string()).collect();
    let ns: Vec<i64> = counts.column("__n")?.cast(&DataType::Int64)?.i64()?.into_iter().map(|v| v.unwrap_or(0)).collect();
    if names.len() > MAX_FACETS {
        bail!(
            "Chart: '{}' has {} different values, and small multiples show at most {}. Filter the tab first, or choose a column with fewer values.",
            facet,
            names.len(),
            MAX_FACETS
        );
    }
    let mut order: Vec<usize> = (0..names.len()).collect();
    order.sort_by(|&a, &b| ns[b].cmp(&ns[a]).then_with(|| names[a].cmp(&names[b])));
    let inner = ChartSpec { facet: None, ..spec.clone() };
    let mut out = Vec::new();
    for i in order {
        let part = lf.clone().filter(key.clone().eq(lit(names[i].clone())));
        let d = compute(part, &inner).with_context(|| format!("Chart: the panel '{}'", names[i]))?;
        out.push(FacetData { name: names[i].clone(), data: Box::new(d) });
    }
    Ok(out)
}

// ------------------------------------------------------------------------ scatter, histogram, box plot

/// The numbers of a column (blank = `None`), refused with the column's type when it is not a number.
fn number_column(lf: &LazyFrame, schema: &Schema, name: &str, what: &str) -> Result<Vec<Option<f64>>> {
    let Some(dt) = schema.get(name) else { bail!("Chart: the {} column '{}' was not found. Choose another column.", what, name) };
    if !crate::chart::is_number(dt) {
        bail!("Chart: the {} column '{}' is {}, not a number. Set it to a number in Schema & Ingestion, or choose another column.", what, name, crate::chart::type_word(dt));
    }
    let df = lf.clone().select([col(name).cast(DataType::Float64).alias("__x")]).collect().with_context(|| format!("Chart: could not read '{}'", name))?;
    let v: Vec<Option<f64>> = df.column("__x")?.f64()?.into_iter().collect();
    let bad = v.iter().filter(|x| x.is_some_and(|x| !x.is_finite())).count();
    if bad > 0 {
        let first = v.iter().position(|x| x.is_some_and(|x| !x.is_finite())).unwrap_or(0) + 1;
        bail!("Chart: the {} column '{}' has {} value(s) that are not numbers (NaN) or are infinite, the first at row {}. Fix those rows or filter them out, then chart again.", what, name, bad, first);
    }
    Ok(v)
}

fn text_column(lf: &LazyFrame, schema: &Schema, name: &str) -> Result<Vec<String>> {
    let dt = schema.get(name).cloned().unwrap_or(DataType::String);
    let df = lf.clone().select([key_text_expr(name, &dt).fill_null(lit(BLANK_LABEL)).alias("__t")]).collect().with_context(|| format!("Chart: could not read '{}'", name))?;
    Ok(df.column("__t")?.str()?.into_iter().map(|v| v.unwrap_or("").to_string()).collect())
}

fn schema_of(lf: &LazyFrame) -> Result<Schema> {
    let mut l = lf.clone();
    Ok((*l.collect_schema().context("Chart: could not read the columns of the tab")?).clone())
}

/// Scatter and bubble: x is the Category column, y the first measure's column (rows are not added up),
/// the bubble size the size column, the colour the series column (at most 3, the rest "Other").
fn compute_points(lf: LazyFrame, spec: &ChartSpec) -> Result<ChartData> {
    if spec.category.trim().is_empty() {
        bail!("Chart: choose the X column (a number) for the scatter.");
    }
    let schema = schema_of(&lf)?;
    let ml = spec.measure_list();
    let Some(ycol) = ml[0].column.clone().filter(|c| !c.trim().is_empty()) else { bail!("Chart: choose the Y column (a number) for the scatter.") };
    let xs = number_column(&lf, &schema, &spec.category, "X")?;
    let ys = number_column(&lf, &schema, &ycol, "Y")?;
    let sizes = match (spec.kind, spec.size.as_ref().and_then(|s| s.column.clone())) {
        (ChartKind::Bubble, Some(c)) => Some(number_column(&lf, &schema, &c, "size")?),
        (ChartKind::Bubble, None) => bail!("Chart: a bubble chart needs a Size column (a number). Choose one, or use a scatter."),
        _ => None,
    };
    let serie = match &spec.series {
        Some(s) => Some(text_column(&lf, &schema, s)?),
        None => None,
    };
    let n = xs.len();
    // the series, by their number of points; more than 3 become the top 2 and "Other"
    let mut notes = Vec::new();
    let mut names: Vec<String> = vec![String::new()];
    let mut assign: Vec<usize> = vec![0; n];
    if let Some(sv) = &serie {
        let mut count: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for (i, t) in sv.iter().enumerate() {
            if xs[i].is_some() && ys[i].is_some() {
                *count.entry(t.as_str()).or_default() += 1;
            }
        }
        let mut ranked: Vec<(&str, usize)> = count.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        let fold = ranked.len() > MAX_SCATTER_SERIES;
        let keep = if fold { MAX_SCATTER_SERIES - 1 } else { ranked.len() };
        names = ranked.iter().take(keep).map(|(t, _)| t.to_string()).collect();
        if fold {
            notes.push(format!("{} series are shown as the {} largest and \"{}\": scatter charts show at most {} colours.", ranked.len(), keep, OTHER_LABEL, MAX_SCATTER_SERIES));
            names.push(OTHER_LABEL.to_string());
        }
        if names.is_empty() {
            names.push(String::new());
        }
        for (i, t) in sv.iter().enumerate() {
            assign[i] = names.iter().position(|x| x == t).unwrap_or(names.len() - 1);
        }
    }
    // the rows with both numbers; at most MAX_POINTS per series (every k-th row)
    let mut per_series: Vec<Vec<usize>> = vec![Vec::new(); names.len()];
    let mut total = 0u64;
    for i in 0..n {
        if xs[i].is_some() && ys[i].is_some() {
            total += 1;
            per_series[assign[i]].push(i);
        }
    }
    let mut points = Vec::new();
    let mut sampled = false;
    for (si, rows) in per_series.iter().enumerate() {
        let stride = rows.len().div_ceil(MAX_POINTS).max(1);
        sampled |= stride > 1;
        for &i in rows.iter().step_by(stride) {
            if let (Some(x), Some(y)) = (xs[i], ys[i]) {
                points.push(ScatterPoint { x, y, size: sizes.as_ref().and_then(|s| s[i]), series: si });
            }
        }
    }
    if sampled {
        notes.push(format!("Showing a {}-point sample of {}", crate::models::format_with_commas(MAX_POINTS as f64, 0), crate::models::format_with_commas(total as f64, 0)));
    }
    let skipped = n as u64 - total;
    if skipped > 0 {
        notes.push(format!("{} row(s) have a blank x or y and are not drawn.", skipped));
    }
    Ok(ChartData {
        spec: spec.clone(),
        series: names,
        total_rows: n as u64,
        points,
        points_total: total,
        measure_names: vec![measure_name(&ml[0])],
        notes,
        ..ChartData::default()
    })
}

/// The `q`-quantile (0 to 1) of sorted values by linear interpolation.
pub fn quantile_sorted(sorted: &[f64], q: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let pos = q.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    let (a, b) = (sorted.get(lo).copied()?, sorted.get(hi).copied()?);
    Some(a + (b - a) * (pos - lo as f64))
}

/// A label for a bin edge: as few decimals as tell the edges apart.
fn edge_label(v: f64, step: f64) -> String {
    let decimals = if step >= 10.0 { 0 } else if step >= 1.0 { 1 } else if step >= 0.1 { 2 } else { 4 };
    crate::models::format_with_commas(v, decimals)
}

/// Histogram of the first measure's column: equal bins between the 1st and 99th percentile (Phase 5).
fn compute_histogram(lf: LazyFrame, spec: &ChartSpec) -> Result<ChartData> {
    let schema = schema_of(&lf)?;
    let ml = spec.measure_list();
    let column = match ml[0].column.clone().filter(|c| !c.trim().is_empty()).or_else(|| Some(spec.category.clone()).filter(|c| !c.trim().is_empty())) {
        Some(c) => c,
        None => bail!("Chart: choose the number column for the histogram."),
    };
    let all = number_column(&lf, &schema, &column, "histogram")?;
    let total = all.len() as u64;
    let mut vals: Vec<f64> = all.into_iter().flatten().filter(|v| v.is_finite()).collect();
    if vals.is_empty() {
        bail!("Chart: '{}' has no numbers to put in a histogram.", column);
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let bins = spec.bins.unwrap_or(crate::column_stats::HISTOGRAM_BINS).clamp(2, 100);
    let nearest = |q: f64| vals.get((q * (vals.len() - 1) as f64).round() as usize).copied().unwrap_or(vals[0]);
    let (lo, hi) = (nearest(0.01), nearest(0.99));
    let span = hi - lo;
    let mut counts = vec![0u64; bins];
    let (mut below, mut above) = (0u64, 0u64);
    for v in &vals {
        if *v < lo {
            below += 1;
        } else if *v > hi {
            above += 1;
        } else if span > 0.0 {
            counts[(((v - lo) / span * bins as f64).floor() as usize).min(bins - 1)] += 1;
        } else {
            counts[0] += 1;
        }
    }
    let step = if span > 0.0 { span / bins as f64 } else { 1.0 };
    let edges: Vec<f64> = (0..=bins).map(|i| lo + step * i as f64).collect();
    let categories: Vec<String> = (0..bins).map(|i| format!("{} to {}", edge_label(edges[i], step), edge_label(edges[i + 1], step))).collect();
    let mut notes = Vec::new();
    if below + above > 0 {
        notes.push(format!("{} value(s) below {} and {} above {} are not in the bars (the bars span the 1st to the 99th percentile).", below, edge_label(lo, step), above, edge_label(hi, step)));
    }
    if total as usize > vals.len() {
        notes.push(format!("{} blank value(s) are not counted.", total as usize - vals.len()));
    }
    Ok(ChartData {
        spec: spec.clone(),
        categories,
        series: vec![String::new()],
        values: vec![counts.iter().map(|c| Some(*c as f64)).collect()],
        rows: vec![counts.clone()],
        total_rows: total,
        distinct_categories: bins,
        measure_names: vec![format!("Rows by {}", column)],
        hist: Some(Histogram { edges, counts, below, above }),
        notes,
        ..ChartData::default()
    })
}

/// Box plot: one box per category over the first measure's column.
fn compute_boxes(lf: LazyFrame, spec: &ChartSpec) -> Result<ChartData> {
    if spec.category.trim().is_empty() {
        bail!("Chart: choose the Category (X) column for the box plot.");
    }
    let schema = schema_of(&lf)?;
    let ml = spec.measure_list();
    let Some(vcol) = ml[0].column.clone().filter(|c| !c.trim().is_empty()) else { bail!("Chart: choose the Value column (a number) for the box plot.") };
    if schema.get(spec.category.as_str()).is_none() {
        bail!("Chart: the category column '{}' was not found. Choose another column.", spec.category);
    }
    let cats = text_column(&lf, &schema, &spec.category)?;
    let vals = number_column(&lf, &schema, &vcol, "value")?;
    let mut groups: std::collections::HashMap<String, Vec<f64>> = std::collections::HashMap::new();
    for (c, v) in cats.iter().zip(vals.iter()) {
        if let Some(v) = v.filter(|v| v.is_finite()) {
            groups.entry(c.clone()).or_default().push(v);
        }
    }
    let distinct = groups.len();
    let mut names: Vec<String> = groups.keys().cloned().collect();
    // the categories with most values stay (Top N), in the chart's order
    names.sort_by(|a, b| groups[b].len().cmp(&groups[a].len()).then_with(|| a.cmp(b)));
    let mut notes = Vec::new();
    if names.len() > spec.top_n.max(1) {
        notes.push(format!("{} categories have numbers; the {} with most values are shown.", names.len(), spec.top_n.max(1)));
        names.truncate(spec.top_n.max(1));
    }
    let key = |c: &String| -> Option<String> { if c == BLANK_LABEL { None } else { Some(c.clone()) } };
    names.sort_by(|a, b| spec.order.compare(&key(a), &key(b)));
    let mut boxes = Vec::new();
    for c in &names {
        let mut v = groups[c].clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let (q1, med, q3) = (quantile_sorted(&v, 0.25), quantile_sorted(&v, 0.5), quantile_sorted(&v, 0.75));
        let (Some(q1), Some(median), Some(q3)) = (q1, med, q3) else { continue };
        let iqr = q3 - q1;
        let (lo_fence, hi_fence) = (q1 - 1.5 * iqr, q3 + 1.5 * iqr);
        let low_whisker = v.iter().copied().find(|x| *x >= lo_fence).unwrap_or(q1);
        let high_whisker = v.iter().rev().copied().find(|x| *x <= hi_fence).unwrap_or(q3);
        let outliers: Vec<f64> = v.iter().copied().filter(|x| *x < lo_fence || *x > hi_fence).take(200).collect();
        boxes.push(BoxStat { category: c.clone(), n: v.len() as u64, min: v[0], q1, median, q3, max: v[v.len() - 1], low_whisker, high_whisker, outliers });
    }
    let total: u64 = boxes.iter().map(|b| b.n).sum();
    Ok(ChartData {
        spec: spec.clone(),
        categories: boxes.iter().map(|b| b.category.clone()).collect(),
        series: vec![String::new()],
        values: vec![boxes.iter().map(|b| Some(b.median)).collect()],
        rows: vec![boxes.iter().map(|b| b.n).collect()],
        total_rows: total,
        distinct_categories: distinct,
        measure_names: vec![format!("Median of {}", vcol)],
        boxes,
        notes,
        ..ChartData::default()
    })
}
