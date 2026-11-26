//! The worker's chart calculation (Phase 7 C0): the Phase 6 aggregation extended to several measures,
//! a size encoding, small-multiple facets and the transforms (percent of total, running sum). It runs
//! over the whole tab after its steps and returns the `ChartData` every renderer reads.

use super::data::{apply_transform, BoxStat, ChartData, FacetData, Histogram, MeasureData, ScatterPoint};
use super::spec::{ChartKind, ChartSpec, MeasureSpec, Transform};
use super::tokens::{MAX_BARS, MAX_FACETS, MAX_HEAT_COLUMNS, MAX_POINTS, MAX_SCATTER_SERIES};
use crate::chart::{compute_chart_fixed, FixedKept, OTHER_LABEL};
use crate::pivot_table::{key_text_expr, BLANK_LABEL};
use anyhow::{bail, Context, Result};
use polars::prelude::*;

/// The name of a measure in legends and titles: its label, else "Sum of Balance" / "Count of rows".
pub fn measure_name(m: &MeasureSpec) -> String {
    if !m.label.trim().is_empty() {
        return m.label.clone();
    }
    if let (Some(t), Some(c)) = (&m.time, &m.column) {
        return t.heading(c, m.weight.as_deref().map(str::trim).filter(|w| !w.is_empty()), false);
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
    // Phase 12: a forecast is fitted to the actual numbers, never to a scenario. When the chart's measure is marked for the active
    // what-if scenario, the base data is worked out with the scenario switched off, and the scenario is then applied to the as-of month
    // and the forecast months together.
    let scenario = crate::model::scope().and_then(|c| {
        let w = c.what_if.clone()?;
        let name = spec.measure_list().first().and_then(|m| m.column.clone().filter(|_| m.time.is_some_and(|t| t.agg == crate::models::TimeAgg::Measure)))?;
        let role = w.role_of(&name)?;
        let as_of = c.calendar.as_ref().and_then(|k| k.as_of)?;
        Some((c.clone(), w, role, name, as_of))
    });
    let (data, scenario) = match (&spec.forecast, scenario) {
        (Some(_), Some((ctx, w, role, name, as_of))) => {
            let mut plain = (*ctx).clone();
            plain.what_if = None;
            let data = crate::model::with_scope(Some(std::sync::Arc::new(plain)), || compute_base(lf, spec))?;
            (data, Some((w, role, name, as_of)))
        }
        _ => (compute_base(lf, spec)?, None),
    };
    let data = match &spec.forecast {
        Some(f) => crate::forecast_chart::apply_forecast(data, f)?,
        None => data,
    };
    let data = match scenario {
        Some((w, role, name, as_of)) => crate::forecast_chart::apply_scenario(data, &w, role, &name, as_of)?,
        None => data,
    };
    match spec.target.clone() {
        Some(t) => apply_target(data, &t),
        None => Ok(data),
    }
}

/// Phase 12: a target line and its red, amber and green zones. They are added to the spec the data carries, so the screen, HTML, PDF,
/// the Python export and Excel (which draws the line) all show them; the user's own reference lines are left as they are.
pub fn apply_target(mut data: ChartData, t: &crate::charts::spec::ChartTarget) -> Result<ChartData> {
    use crate::charts::spec::{ChartKind, RefBand, RefLine, Status};
    use crate::target::{bands_over, Direction, Rag};
    if !matches!(data.spec.kind, ChartKind::Bar | ChartKind::Line | ChartKind::Area) {
        bail!("A target line needs a column, bar, line or area chart. Change the chart's form, or take the target off.");
    }
    if !t.value.is_finite() {
        bail!("The target is not a number.");
    }
    t.bands.check().map_err(|e| anyhow::anyhow!(e))?;
    let target = t.raw();
    let label = if t.label.trim().is_empty() { "Target".to_string() } else { t.label.trim().to_string() };
    data.spec.ref_lines.push(RefLine { value: target, label, status: Status::Neutral, panel: 0 });
    if t.show_bands {
        let all: Vec<f64> = data.values.iter().flatten().flatten().copied().filter(|v| v.is_finite()).collect();
        let (dmin, dmax) = all.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
        let size = if target != 0.0 { target.abs() } else if dmax.is_finite() && dmax != 0.0 { dmax.abs() } else { 1.0 };
        let lo = (0.0f64).min(if dmin.is_finite() { dmin } else { 0.0 }).min(target - size);
        let hi = (if dmax.is_finite() { dmax * 1.05 } else { target }).max(target + size * (0.3f64).max(t.bands.amber_within_pct / 100.0 + 0.15)).max(lo + size);
        let dir: Direction = t.direction;
        for (from, to, rag) in bands_over(lo, hi, target, dir, &t.bands) {
            let (status, label) = match (rag, dir) {
                (Rag::Green, _) => (Status::Good, format!("Green: on target or within {}%", t.bands.green_within_pct)),
                (Rag::Amber, _) => (Status::Warning, format!("Amber: within {}% of target", t.bands.amber_within_pct)),
                (Rag::Red, _) => (Status::Critical, format!("Red: more than {}% off target", t.bands.amber_within_pct)),
            };
            data.spec.ref_bands.push(RefBand { from, to, label, panel: 0, status: Some(status) });
        }
    }
    Ok(data)
}

fn compute_base(lf: LazyFrame, spec: &ChartSpec) -> Result<ChartData> {
    // W6: the chart is drawn at its drill level, from the rows its path and its own filters leave
    spec.check_drill()?;
    spec.check_filters()?;
    let level = spec.at_level();
    let spec = &level;
    let conditions = spec.row_conditions();
    let lf = if conditions.is_empty() { lf } else { crate::engine::filter_by_conditions(lf, &conditions).context("Chart: the drill path or the chart's filters could not be applied")? };
    // the date parts of the category, series or facet (W2) are added as columns first
    let fields = spec.date_fields();
    let lf = if fields.is_empty() {
        lf
    } else {
        let mut lf = lf;
        let schema = lf.collect_schema().context("Could not read the column types for the chart")?;
        crate::dates::with_date_fields(lf, &fields, &schema, "Chart")?
    };
    // time values (M4): the pivot's own calculation, so a chart shows the pivot's number
    if spec.measure_list().iter().any(|m| m.time.is_some()) {
        return compute_time_chart(lf, spec);
    }
    // the stock warning (M3): a Sum or Count of a column tagged Balance over rows of more than one date
    let warnings = if spec.data_tab.is_empty() {
        Vec::new()
    } else {
        use crate::charts::spec::ChartAgg;
        let plain: Vec<(String, String)> = spec
            .measure_list()
            .iter()
            .filter(|m| matches!(m.agg, ChartAgg::Sum | ChartAgg::Count))
            .filter_map(|m| m.column.as_ref().map(|c| ((if m.agg == ChartAgg::Sum { "Sum" } else { "Count" }).to_string(), c.clone())))
            .collect();
        crate::pivot_time::stock_warnings_for(lf.clone(), &spec.data_tab, &plain)?
    };
    // lookup fields (M1) are text, with "(no match)" for a key with no lookup row
    let lf = crate::model::with_lookup_columns(lf, &chart_lookup_wants(spec), "Chart")?;
    let mut data = compute_on(lf, spec)?;
    data.notes.extend(warnings);
    Ok(data)
}

/// A chart whose measures are time values (Phase 9 M4): the same value type as the pivot, worked out by the pivot's own code
/// (`build_pivot`) for the chart's category (rows) and series (columns), so a mark is exactly the pivot's cell. The categories come
/// in the pivot's order (Calendar order for a Calendar field). A chart filter or a drill level has already taken its rows away, so
/// the tab's snapshot dates are those of the rows left.
fn compute_time_chart(lf: LazyFrame, spec: &ChartSpec) -> Result<ChartData> {
    use crate::models::{PivotAgg, PivotFieldOrder, PivotSpec, PivotValue};
    let measures = spec.measure_list();
    if measures.iter().any(|m| m.time.is_none()) {
        bail!("Chart: a time value (Closing balance, Average balance, a shift ...) can't share a chart with an ordinary aggregation yet. Use time values for every measure of this chart, or none.");
    }
    if matches!(spec.kind, ChartKind::Scatter | ChartKind::Bubble | ChartKind::Histogram | ChartKind::BoxPlot) {
        bail!("Chart: time values are drawn as columns, bars, lines or areas by a category. Choose one of those forms, or an ordinary aggregation.");
    }
    if spec.facet.as_deref().is_some_and(|f| !f.trim().is_empty()) || spec.size.is_some() {
        bail!("Chart: time values can't be split into small multiples or sized yet. Remove the facet and the size, or use an ordinary aggregation.");
    }
    if !spec.date_fields().is_empty() {
        bail!("Chart: a time value takes its period from a Calendar field (Model \u{25b8} Calendar\u{2026}), and '{}' is a date part of the tab's own column. Use the Calendar field instead.", spec.date_fields().first().map(|f| f.name.clone()).unwrap_or_default());
    }
    if spec.data_tab.is_empty() {
        bail!("Chart: a time value needs a date-linked data tab in a project. Save the tabs as a project, turn on the calendar and link this tab's date column (Model \u{25b8} Calendar\u{2026}).");
    }
    if spec.category.trim().is_empty() {
        bail!("Chart: choose a category (a Calendar field such as Calendar \u{203a} Month-Year) for the time values.");
    }
    let settings = crate::model::scope().and_then(|c| c.calendar.as_ref().map(|c| c.settings)).unwrap_or_default();
    let mut model_fields = spec.model_fields();
    model_fields.retain(|f| Some(f.name.as_str()) == Some(spec.category.as_str()) || spec.series.as_deref() == Some(f.name.as_str()));
    let field_orders: Vec<PivotFieldOrder> = model_fields
        .iter()
        .filter_map(|f| crate::dates::DatePart::from_label(&f.column).and_then(|p| p.sort_order(&settings)).map(|order| PivotFieldOrder { field: f.name.clone(), order }))
        .collect();
    let series_field = spec.series.clone().filter(|s| !s.trim().is_empty());
    let pivot = PivotSpec {
        rows: vec![spec.category.clone()],
        columns: series_field.iter().cloned().collect(),
        values: measures.iter().map(|m| PivotValue { field: m.column.clone().unwrap_or_default(), agg: PivotAgg::Sum, weight_col: m.weight.clone(), time: m.time }).collect(),
        row_totals: false,
        column_totals: false,
        model_fields,
        field_orders,
        data_tab: spec.data_tab.clone(),
        ..Default::default()
    };
    let total_rows = lf.clone().select([len().alias("__n")]).collect().context("Chart: the rows could not be counted")?.column("__n")?.cast(&DataType::UInt64)?.u64()?.get(0).unwrap_or(0);
    let build = crate::pivot_table::build_pivot(lf, &pivot).context("Chart")?;
    let df = &build.df;
    let categories: Vec<String> = df.column(df.get_column_names().first().map(|n| n.as_str()).unwrap_or(""))?.str()?.into_iter().map(|v| v.unwrap_or("").to_string()).collect();
    let n = categories.len();
    // the series: the Columns keys in the pivot's order (one unnamed series with no series field)
    let mut series: Vec<String> = Vec::new();
    let mut keys: Vec<Vec<Option<String>>> = Vec::new();
    for k in &build.col_keys {
        if !keys.contains(k) {
            keys.push(k.clone());
            series.push(k.first().cloned().flatten().unwrap_or_default());
        }
    }
    let mut grids: Vec<Vec<Vec<Option<f64>>>> = vec![vec![vec![None; n]; series.len()]; measures.len()];
    let mut heads: Vec<String> = measures.iter().map(measure_name).collect();
    for (c, name) in df.get_column_names().iter().enumerate().skip(build.label_columns) {
        let (Some(&j), Some(key)) = (build.col_values.get(c - build.label_columns), build.col_keys.get(c - build.label_columns)) else { continue };
        let Some(si) = keys.iter().position(|k| k == key) else { continue };
        let vals: Vec<Option<f64>> = df.column(name.as_str())?.cast(&DataType::Float64)?.f64()?.into_iter().collect();
        for (ci, v) in vals.into_iter().enumerate().take(n) {
            grids[j][si][ci] = v;
        }
        // the pivot's heading names the value (a Month-end tab says "Average of month-ends")
        if measures[j].label.trim().is_empty() {
            heads[j] = if measures.len() > 1 && series_field.is_some() { name.as_str().split_once(" | ").map(|x| x.1.to_string()).unwrap_or_else(|| name.to_string()) } else if series_field.is_some() { heads[j].clone() } else { name.to_string() };
        }
    }
    let rows_of = |g: &Vec<Vec<Option<f64>>>| -> Vec<Vec<u64>> { g.iter().map(|r| r.iter().map(|v| v.is_some() as u64).collect()).collect() };
    let limit = match spec.kind {
        ChartKind::Bar => Some((MAX_BARS, "bars or columns")),
        ChartKind::Heatmap => Some((MAX_HEAT_COLUMNS, "heatmap columns")),
        _ => None,
    };
    if let Some((limit, what)) = limit {
        if n > limit {
            bail!("Chart: '{}' would give {} {}, and more than {} cannot be read side by side. Use a filter to show fewer.", spec.category, n, what, limit);
        }
    }
    // a % change is already a percent: its measure says so (no Crore scale on it)
    let mut shown_spec = spec.clone();
    if measures.iter().any(|m| m.time.is_some_and(|t| t.show == crate::models::TimeShow::PercentChange)) {
        shown_spec.measures = measures.iter().cloned().map(|mut m| { m.pct = m.time.is_some_and(|t| t.show == crate::models::TimeShow::PercentChange); m }).collect();
    }
    let mut data = ChartData {
        spec: shown_spec,
        categories: categories.clone(),
        series: series.clone(),
        values: grids[0].clone(),
        rows: rows_of(&grids[0]),
        total_rows,
        distinct_categories: n,
        folded_categories: 0,
        notes: build.warnings.clone(),
        kept_categories: categories.clone(),
        kept_series: series.clone(),
        measure_names: heads.clone(),
        ..Default::default()
    };
    for (k, g) in grids.iter().enumerate().skip(1) {
        data.extra.push(MeasureData { name: heads[k].clone(), values: g.clone(), rows: rows_of(g) });
    }
    // why a mark is missing: the reason of the first few blank cells
    let mut why: Vec<String> = Vec::new();
    for cell in build.notes.iter().take(4) {
        if let Some(label) = categories.get(cell.row) {
            why.push(format!("{}: {}", label, cell.text));
        }
    }
    if build.notes.len() > 4 {
        why.push(format!("and {} more blank values", build.notes.len() - 4));
    }
    data.notes.extend(why);
    finite_or_stop(&data.values, &data.categories, &data.series, measures[0].column.as_deref().unwrap_or("Count of rows"))?;
    for (k, m) in measures.iter().enumerate() {
        if m.transform == Transform::None {
            continue;
        }
        let src = if k == 0 { &data.values } else { &data.extra[k - 1].values };
        let (out, notes) = apply_transform(src, m.transform);
        data.notes.extend(notes);
        if k == 0 {
            data.values = out;
        } else {
            data.extra[k - 1].values = out;
        }
    }
    Ok(data)
}

/// The lookup fields of a chart as columns to add.
pub fn chart_lookup_wants(spec: &ChartSpec) -> Vec<crate::model::LookupWant> {
    spec.model_fields().into_iter().map(|f| crate::model::LookupWant { out: f.name.clone(), field: f, as_: crate::model::LookupAs::Text, unmatched_out: None, blank_key_out: None }).collect()
}

/// The chart of a table that already has the chart's date parts as columns (a facet's panel).
fn compute_on(lf: LazyFrame, spec: &ChartSpec) -> Result<ChartData> {
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
    let mut primary_spec = spec.for_measure(&measures[0]);
    if spec.kind == ChartKind::Donut {
        // a donut draws every part unless "Group small parts" asks for the n largest and "Other"
        primary_spec.top_n = spec.donut_top.map(|n| n.clamp(2, 12)).unwrap_or(500);
    }
    let mut data = compute_chart_fixed(lf.clone(), &primary_spec, None)?;
    data.spec = spec.clone();
    data.measure_names = measures.iter().map(measure_name).collect();
    let (n, limit, what) = match spec.kind {
        ChartKind::Bar => (data.categories.len(), MAX_BARS, "bars or columns"),
        ChartKind::Heatmap => (data.categories.len(), MAX_HEAT_COLUMNS, "heatmap columns"),
        _ => (0, usize::MAX, ""),
    };
    if n > limit {
        bail!("Chart: '{}' would give {} {}, and more than {} cannot be read side by side. Use Top N or a filter to show fewer.", spec.category, n, what, limit);
    }
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
    check_single_panels(&data)?;
    Ok(data)
}

/// A panel marked `single` draws one series for a measure that is the same in every series (a ratio of each category).
/// If the series disagree for a category, it is not one number and the chart stops.
fn check_single_panels(data: &ChartData) -> Result<()> {
    for p in data.spec.panels.iter().filter(|p| p.single) {
        let Some(&mi) = p.measures.first() else { continue };
        let Some(m) = data.measure(mi) else { continue };
        for (ci, cat) in data.categories.iter().enumerate() {
            let vals: Vec<(usize, f64)> = m.values.iter().enumerate().filter_map(|(si, r)| r.get(ci).copied().flatten().map(|v| (si, v))).collect();
            if let Some((s0, v0)) = vals.first().copied() {
                if let Some((s1, v1)) = vals.iter().copied().find(|(_, v)| (v - v0).abs() > 1e-9 * v0.abs().max(1.0)) {
                    bail!(
                        "Chart: '{}' is drawn once, so it must be the same in every series, but for '{}' it is {} in '{}' and {} in '{}'. Fix the table so the ratio repeats on every row of that category.",
                        m.name,
                        cat,
                        v0,
                        data.series.get(s0).cloned().unwrap_or_default(),
                        v1,
                        data.series.get(s1).cloned().unwrap_or_default()
                    );
                }
            }
        }
    }
    Ok(())
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
    match spec.facet_order.as_ref() {
        // Q1: the panels in the order chosen for the facet
        Some(o) => {
            let key = |i: usize| -> Option<String> { if names[i] == BLANK_LABEL { None } else { Some(names[i].clone()) } };
            order.sort_by(|&a, &b| o.compare(&key(a), &key(b)));
        }
        None => order.sort_by(|&a, &b| ns[b].cmp(&ns[a]).then_with(|| names[a].cmp(&names[b]))),
    }
    if spec.facet_model.is_some() {
        order.sort_by_key(|&i| crate::model::label_rank(&names[i]));
    }
    let inner = ChartSpec { facet: None, ..spec.clone() };
    let mut out = Vec::new();
    for i in order {
        let part = lf.clone().filter(key.clone().eq(lit(names[i].clone())));
        let d = compute_on(part, &inner).with_context(|| format!("Chart: the panel '{}'", names[i]))?;
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
