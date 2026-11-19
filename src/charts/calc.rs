//! The worker's chart calculation (Phase 7 C0): the Phase 6 aggregation extended to several measures,
//! a size encoding, small-multiple facets and the transforms (percent of total, running sum). It runs
//! over the whole tab after its steps and returns the `ChartData` every renderer reads.

use super::data::{apply_transform, ChartData, FacetData, MeasureData};
use super::spec::{ChartSpec, MeasureSpec, Transform};
use super::tokens::MAX_FACETS;
use crate::chart::{compute_chart_fixed, FixedKept};
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

/// Calculates the whole chart over all rows of `lf`: every measure, the size and the facets.
pub fn compute(lf: LazyFrame, spec: &ChartSpec) -> Result<ChartData> {
    let measures = spec.measure_list();
    let mut notes: Vec<String> = Vec::new();
    // the primary measure decides the categories, their order and what is folded into "Other"
    let primary_spec = spec.for_measure(&measures[0]);
    let mut data = compute_chart_fixed(lf.clone(), &primary_spec, None)?;
    data.spec = spec.clone();
    data.measure_names = measures.iter().map(measure_name).collect();
    let fixed = FixedKept { categories: data.kept_categories.clone(), series: data.kept_series.clone() };

    for (k, m) in measures.iter().enumerate().skip(1) {
        let other = compute_chart_fixed(lf.clone(), &spec.for_measure(m), Some(&fixed)).with_context(|| format!("Chart: measure {} ({})", k + 1, measure_name(m)))?;
        notes.extend(other.notes.iter().filter(|n| !data.notes.contains(n)).cloned());
        data.extra.push(align(&data, &other, measure_name(m)));
    }
    if let Some(size) = &spec.size {
        let other = compute_chart_fixed(lf.clone(), &spec.for_measure(size), Some(&fixed)).context("Chart: the size measure")?;
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

    if let Some(f) = spec.facet.as_deref().filter(|f| !f.trim().is_empty()) {
        data.facets = compute_facets(lf, spec, f)?;
    }
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
