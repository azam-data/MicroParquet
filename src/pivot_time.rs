//! Phase 9 M3: the time aggregations of a pivot (closing balance, average balance, total of a flow, period amount, counts at
//! closing) worked out from a daily series.
//!
//! A pivot cell has a **period** P: the days of the Calendar fields in its row and column (a Rows field "Calendar › Month-Year"
//! with the key "Nov 2026" makes P the 30 days of November 2026), cut at the as-of date. A cell with no Calendar field in its key is
//! a total over that field: P is the union of the periods the pivot shows. A subtotal, the Grand total and a Total column are
//! therefore the measure over their own rows and their own period, never the sum of the cells.
//!
//! The series of a cell is the sum of its rows by date, over the pivot's non-Calendar keys and filters (a Calendar key or filter
//! only picks the period, so the carry-forward can reach back before the period's start).

use crate::measures::{evaluate, DayStats, Measured, Timeline};
use crate::model::{self, ModelContext};
use crate::model_calendar::{long_date, snapshot_counts, Frequency, SnapshotInfo};
use crate::models::*;
use crate::pivot_table::{key_text_expr, Cells, Key};
use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use polars::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};

/// What `fill_time_cells` found besides the numbers.
#[derive(Debug, Default)]
pub struct TimeResult {
    /// The tab is a Month-end tab: the average is of the month-ends and is headed so.
    pub month_end: bool,
    /// Why a time cell is blank: (row part, column part, value index) → the reason.
    pub notes: HashMap<(Key, Key, usize), String>,
}

/// The pivot's own Calendar fields: the fields of the date link of its data tab.
fn calendar_fields(spec: &PivotSpec, calendar_tab: &str) -> HashSet<String> {
    spec.model_fields.iter().filter(|f| f.data_tab == spec.data_tab && f.lookup_tab == calendar_tab).map(|f| f.name.clone()).collect()
}

/// The refusals that need no data: what a time value needs from the project.
fn context_of(spec: &PivotSpec) -> Result<(std::sync::Arc<ModelContext>, model::CalendarScope, model::DateLink)> {
    let first = spec.values.iter().find(|v| v.time.is_some()).map(|v| v.label()).unwrap_or_default();
    let Some(ctx) = model::scope() else {
        let _ = &first;
        bail!("{}", model::outside_project_message("the date link of this pivot's tab"));
    };
    if spec.data_tab.is_empty() {
        bail!("Pivot: '{}' needs a date-linked data tab. Save the tabs as a project, turn on the calendar and link this tab's date column (Model \u{25b8} Calendar\u{2026}), then pick the aggregation again.", first);
    }
    let Some(cal) = ctx.calendar.clone() else {
        bail!("Pivot: '{}' needs the model calendar. Turn it on and link this tab's date column in Model \u{25b8} Calendar\u{2026}.", first);
    };
    let title = cal.title_of(&spec.data_tab);
    let Some(link) = cal.date_links.iter().find(|l| l.data_tab == spec.data_tab).cloned() else {
        bail!("Pivot: '{}' needs a date link, and '{}' has none. Pick its date column in Model \u{25b8} Calendar\u{2026}.", first, title);
    };
    Ok((ctx, cal, link))
}

/// The key as the cell holds it: the Rows fields' texts then the Columns fields'.
fn cell_texts(rk: &Key, ck: &Key, nr: usize, at: usize) -> Option<Option<String>> {
    if at < nr {
        rk.get(at).cloned()
    } else {
        ck.get(at - nr).cloned()
    }
}

/// Fills the time values of every cell of `cells` and returns why some are blank. `lf` is the pivot's input (its lookup fields joined)
/// before the pivot's own filters; `row_keys` and `col_keys` are the keys the pivot shows.
#[allow(clippy::too_many_arguments)]
pub fn fill_time_cells(lf: LazyFrame, spec: &PivotSpec, schema: &Schema, levels: &[Vec<usize>], nr: usize, row_keys: &[Key], col_keys: &[Key], cells: &mut Cells) -> Result<TimeResult> {
    let (ctx, cal, link) = context_of(spec)?;
    let title = cal.title_of(&spec.data_tab);
    let nc = spec.columns.len();
    let fields: Vec<&String> = spec.rows.iter().chain(spec.columns.iter()).collect();
    if let Some(f) = spec.date_fields.iter().find(|d| fields.iter().any(|x| **x == d.name) || spec.filters.iter().any(|x| x.field == d.name)) {
        bail!("Pivot: a time aggregation takes its period from the Calendar fields (Model \u{25b8} Calendar\u{2026}), and '{}' is a date part of the tab's own column. Use the Calendar field instead.", crate::dates::picker_label(&f.column, f.part));
    }
    let cal_names = calendar_fields(spec, &cal.tab);
    let cal_at: Vec<usize> = fields.iter().enumerate().filter(|(_, f)| cal_names.contains(f.as_str())).map(|(i, _)| i).collect();
    let Some(as_of) = cal.as_of else {
        bail!("Pivot: the as-of date is not known yet (the dates of the tabs are still being read). Open the pivot again in a moment, or set the date in Model \u{25b8} As-of date\u{2026}.");
    };
    // the tab's snapshot dates: its dates after its steps and before the cell's filters
    let counts = snapshot_counts(lf.clone(), &link.column).with_context(|| format!("Pivot: the date link of '{}' ('{}')", title, link.column))?;
    let info = SnapshotInfo::from_counts(&counts);
    let month_end = info.frequency == Frequency::MonthEnd;
    let timeline = Timeline { snapshots: counts.iter().map(|c| c.0).collect(), fiscal: cal.settings, tab: title.clone(), as_of };

    // the Calendar: each day with the label every Calendar field of the pivot gives it
    let mut cal_days: Vec<NaiveDate> = Vec::new();
    let mut cal_labels: Vec<Vec<String>> = vec![Vec::new(); cal_at.len()];
    if !cal_at.is_empty() {
        let Some(sl) = ctx.links.iter().find(|l| l.link.data_tab == spec.data_tab && l.link.lookup_tab == cal.tab) else {
            bail!("Pivot: the calendar link of '{}' is not part of the model. Open Model \u{25b8} Calendar\u{2026}.", title);
        };
        if let Some(r) = &sl.refusal {
            bail!("Pivot: {}", r);
        }
        let Some(ct) = &sl.lookup else {
            bail!("Pivot: the Calendar tab is not open. Open Model \u{25b8} Calendar\u{2026} to make it again.");
        };
        let mut clf = crate::engine::compile_tab_lazy(&ct.source, &ct.steps, &ctx.temp_dir).context("Pivot: the Calendar could not be read")?;
        let cschema = clf.collect_schema().context("Pivot: the Calendar's columns could not be read")?;
        let mut exprs = vec![col("Date").cast(DataType::Date).alias("__cd")];
        for (n, &i) in cal_at.iter().enumerate() {
            let field = spec.model_fields.iter().find(|f| &f.name == fields[i]).context("Pivot: a Calendar field is not in the model")?;
            let dt = cschema.get(field.column.as_str()).cloned().with_context(|| format!("Pivot: the Calendar has no column '{}'", field.column))?;
            exprs.push(key_text_expr(&field.column, &dt).fill_null(lit(crate::pivot_table::BLANK_LABEL)).alias(format!("__cl{n}")));
        }
        let df = clf.select(exprs).collect().context("Pivot: the Calendar could not be read")?;
        let days = df.column("__cd")?.cast(&DataType::Int32)?;
        for d in days.i32()?.into_iter() {
            cal_days.push(d.and_then(crate::dates::date_from_days).context("Pivot: the Calendar has a blank date")?);
        }
        for n in 0..cal_at.len() {
            cal_labels[n] = df.column(&format!("__cl{n}"))?.str()?.into_iter().map(|v| v.unwrap_or("").to_string()).collect();
        }
    }

    // the pivot's filters on Calendar fields pick days; the others pick rows
    let cal_filters: Vec<Vec<String>> = cal_at
        .iter()
        .map(|&i| spec.filters.iter().filter(|f| &f.field == fields[i]).flat_map(|f| f.values.iter().flatten().cloned()).collect::<Vec<_>>())
        .collect();
    let cal_filtered: Vec<bool> = cal_at.iter().map(|&i| spec.filters.iter().any(|f| &f.field == fields[i] && !f.values.is_empty())).collect();
    let spec_rows = PivotSpec { filters: spec.filters.iter().filter(|f| !cal_names.contains(&f.field)).cloned().collect(), ..spec.clone() };

    // the series: the rows by the pivot's other keys and the date
    let date_col = link.column.as_str();
    let mut tw = lf;
    if let Some(p) = crate::pivot_table::filter_predicate(&spec_rows, schema) {
        tw = tw.filter(p);
    }
    let mut exprs: Vec<Expr> = vec![col(date_col).cast(DataType::Date).alias("__d")];
    let nc_at: Vec<usize> = (0..fields.len()).filter(|i| !cal_at.contains(i)).collect();
    for &i in &nc_at {
        let dt = schema.get(fields[i].as_str()).cloned().unwrap_or(DataType::String);
        exprs.push(key_text_expr(fields[i], &dt).alias(format!("__k{i}")));
    }
    let time_js: Vec<usize> = spec.values.iter().enumerate().filter(|(_, v)| v.time.is_some()).map(|(j, _)| j).collect();
    for &j in &time_js {
        let v = &spec.values[j];
        let dt = schema.get(v.field.as_str()).cloned().unwrap_or(DataType::String);
        let agg = v.time.map(|t| t.agg).unwrap_or_default();
        if agg.needs_number() {
            exprs.push(col(v.field.as_str()).cast(DataType::Float64).alias(format!("__v{j}")));
        } else {
            exprs.push(lit(NULL).cast(DataType::Float64).alias(format!("__v{j}")));
        }
        exprs.push(key_text_expr(&v.field, &dt).alias(format!("__s{j}")));
        match v.weight_col.as_deref().map(str::trim).filter(|w| !w.is_empty()) {
            Some(w) if agg == TimeAgg::WAvgAtClosing => exprs.push(col(w).cast(DataType::Float64).alias(format!("__w{j}"))),
            _ => exprs.push(lit(NULL).cast(DataType::Float64).alias(format!("__w{j}"))),
        }
    }
    let work = tw.select(exprs).filter(col("__d").is_not_null());

    // one group-by for each distinct set of non-Calendar keys the levels use
    let mut wanted: Vec<Vec<usize>> = Vec::new();
    for level in levels {
        let k: Vec<usize> = level.iter().copied().filter(|i| !cal_at.contains(i)).collect();
        if !wanted.contains(&k) {
            wanted.push(k);
        }
    }
    let frames: Vec<LazyFrame> = wanted
        .iter()
        .map(|keys| {
            let mut aggs: Vec<Expr> = Vec::new();
            for &j in &time_js {
                aggs.push(col(format!("__v{j}")).sum().cast(DataType::Float64).alias(format!("__sum{j}")));
                aggs.push(col(format!("__s{j}")).is_not_null().cast(DataType::Int64).sum().cast(DataType::Float64).alias(format!("__cnt{j}")));
                aggs.push(col(format!("__s{j}")).drop_nulls().n_unique().cast(DataType::Float64).alias(format!("__dst{j}")));
                aggs.push((col(format!("__v{j}")) * col(format!("__w{j}"))).sum().cast(DataType::Float64).alias(format!("__ws{j}")));
                aggs.push(col(format!("__w{j}")).sum().cast(DataType::Float64).alias(format!("__wt{j}")));
            }
            let mut by: Vec<Expr> = keys.iter().map(|i| col(format!("__k{i}"))).collect();
            by.push(col("__d"));
            work.clone().group_by(by).agg(aggs)
        })
        .collect();
    let dfs = collect_all(frames).context("Pivot: the daily series could not be calculated")?;
    // key texts -> (date -> stats of each time value)
    type Series = HashMap<Vec<Option<String>>, BTreeMap<NaiveDate, Vec<DayStats>>>;
    let mut series: Vec<Series> = Vec::new();
    for (keys, df) in wanted.iter().zip(dfs.iter()) {
        let key_cols: Vec<Vec<Option<String>>> = keys.iter().map(|i| -> Result<Vec<Option<String>>> { Ok(df.column(&format!("__k{i}"))?.str()?.into_iter().map(|v| v.map(str::to_string)).collect()) }).collect::<Result<_>>()?;
        let d = df.column("__d")?.cast(&DataType::Int32)?;
        let d: Vec<Option<i32>> = d.i32()?.into_iter().collect();
        let num = |name: String| -> Result<Vec<f64>> { Ok(df.column(&name)?.f64()?.into_iter().map(|v| v.filter(|x| x.is_finite()).unwrap_or(0.0)).collect()) };
        let mut stats: Vec<[Vec<f64>; 5]> = Vec::new();
        for &j in &time_js {
            stats.push([num(format!("__sum{j}"))?, num(format!("__cnt{j}"))?, num(format!("__dst{j}"))?, num(format!("__ws{j}"))?, num(format!("__wt{j}"))?]);
        }
        let mut map: Series = HashMap::new();
        for r in 0..df.height() {
            let Some(date) = d[r].and_then(crate::dates::date_from_days) else { continue };
            let key: Vec<Option<String>> = key_cols.iter().map(|c| c[r].clone()).collect();
            let per_date = map.entry(key).or_default().entry(date).or_default();
            if per_date.is_empty() {
                per_date.resize(time_js.len(), DayStats::default());
            }
            for (n, s) in stats.iter().enumerate() {
                per_date[n] = DayStats { sum: s[0][r], count: s[1][r], distinct: s[2][r], weighted: s[3][r], weight: s[4][r] };
            }
        }
        series.push(map);
    }

    // the keys the cells have, level by level
    let mut targets: Vec<(Key, Key)> = Vec::new();
    let nr_keys: Vec<Key> = row_keys.to_vec();
    let c_keys: Vec<Key> = if nc > 0 { col_keys.to_vec() } else { vec![Vec::new()] };
    for rk in &nr_keys {
        for ck in &c_keys {
            targets.push((rk.clone(), ck.clone()));
        }
    }
    if nc > 0 && nr > 0 {
        for rk in &nr_keys {
            targets.push((rk.clone(), Vec::new()));
        }
        for ck in &c_keys {
            targets.push((Vec::new(), ck.clone()));
        }
    }
    targets.push((Vec::new(), Vec::new()));
    for k in 1..nr {
        let mut seen: HashSet<Key> = HashSet::new();
        for rk in &nr_keys {
            let p: Key = rk[..k].to_vec();
            if seen.insert(p.clone()) {
                targets.push((p.clone(), Vec::new()));
                if nc > 0 {
                    for ck in &c_keys {
                        targets.push((p.clone(), ck.clone()));
                    }
                }
            }
        }
    }
    if nr == 0 {
        targets.retain(|(r, _)| r.is_empty());
    }

    // the labels each Calendar field shows in the pivot
    let shown: Vec<HashSet<String>> = cal_at
        .iter()
        .map(|&i| {
            let mut s: HashSet<String> = HashSet::new();
            let keys: &[Key] = if i < nr { row_keys } else { col_keys };
            let pos = if i < nr { i } else { i - nr };
            for k in keys {
                if let Some(Some(t)) = k.get(pos) {
                    s.insert(t.clone());
                }
            }
            s
        })
        .collect();
    let first_snap = timeline.snapshots.first().copied();
    let mut periods: HashMap<Vec<Option<Option<String>>>, (Vec<NaiveDate>, String)> = HashMap::new();
    let mut out = TimeResult { month_end, notes: HashMap::new() };
    for (rk, ck) in targets {
        let level_len = (rk.len(), ck.len());
        // the cell's non-Calendar key, in field order
        let mut nc_key: Vec<Option<String>> = Vec::new();
        let mut want_keys: Vec<usize> = Vec::new();
        for i in 0..fields.len() {
            let present = if i < nr { i < rk.len() } else { i - nr < ck.len() };
            if present && !cal_at.contains(&i) {
                nc_key.push(cell_texts(&rk, &ck, nr, i).flatten());
                want_keys.push(i);
            }
        }
        let Some(which) = wanted.iter().position(|k| *k == want_keys) else {
            bail!("Pivot: the daily series of a cell could not be found. Open the pivot again.");
        };
        let cal_key: Vec<Option<Option<String>>> = cal_at
            .iter()
            .map(|&i| {
                let present = if i < nr { i < rk.len() } else { i - nr < ck.len() };
                if present { cell_texts(&rk, &ck, nr, i) } else { None }
            })
            .collect();
        let entry = periods
            .entry(cal_key.clone())
            .or_insert_with(|| {
                if cal_at.is_empty() {
                    // no Calendar field: the whole span, from the first snapshot date to the as-of date
                    let mut days = Vec::new();
                    if let Some(first) = first_snap {
                        let mut d = first;
                        while d <= as_of {
                            days.push(d);
                            match d.succ_opt() {
                                Some(n) => d = n,
                                None => break,
                            }
                        }
                    }
                    let text = match (days.first(), days.last()) {
                        (Some(a), Some(b)) => format!("{} to {}", long_date(*a), long_date(*b)),
                        _ => format!("the period up to {}", long_date(as_of)),
                    };
                    return (days, text);
                }
                let mut days: Vec<NaiveDate> = Vec::new();
                for (idx, d) in cal_days.iter().enumerate() {
                    if *d > as_of {
                        continue;
                    }
                    let mut ok = true;
                    for n in 0..cal_at.len() {
                        let label = &cal_labels[n][idx];
                        let fits = match &cal_key[n] {
                            Some(Some(t)) => label == t,
                            Some(None) => false,
                            None => shown[n].contains(label),
                        };
                        if !fits || (cal_filtered[n] && !cal_filters[n].contains(label)) {
                            ok = false;
                            break;
                        }
                    }
                    if ok {
                        days.push(*d);
                    }
                }
                days.sort();
                days.dedup();
                let labels: Vec<String> = cal_key.iter().filter_map(|k| k.as_ref().map(|t| t.clone().unwrap_or_else(|| crate::pivot_table::BLANK_LABEL.to_string()))).collect();
                let text = if labels.len() == cal_at.len() && !labels.is_empty() {
                    labels.join(" \u{b7} ")
                } else {
                    match (days.first(), days.last()) {
                        (Some(a), Some(b)) => format!("{} to {}", long_date(*a), long_date(*b)),
                        _ => "the period".to_string(),
                    }
                };
                (days, text)
            });
        let (period, text) = (&entry.0, &entry.1);
        let empty: BTreeMap<NaiveDate, DayStats> = BTreeMap::new();
        for (n, &j) in time_js.iter().enumerate() {
            let agg = spec.values[j].time.map(|t| t.agg).unwrap_or_default();
            let per_date: BTreeMap<NaiveDate, DayStats> = match series[which].get(&nc_key) {
                Some(m) => m.iter().map(|(d, s)| (*d, s.get(n).copied().unwrap_or_default())).collect(),
                None => empty.clone(),
            };
            let m = evaluate(agg, &per_date, period, text, &timeline, month_end);
            let slot = cells.entry((rk.clone(), ck.clone())).or_insert_with(|| vec![None; spec.values.len()]);
            if slot.len() < spec.values.len() {
                slot.resize(spec.values.len(), None);
            }
            match m {
                Measured::Value(v) => slot[j] = Some(v),
                Measured::Blank(why) => {
                    slot[j] = None;
                    out.notes.insert((rk.clone(), ck.clone(), j), why);
                }
            }
        }
        let _ = level_len;
    }
    Ok(out)
}

/// The rows' dates and the dates counted for the stock warning: how many distinct dates the rows a value adds up span.
pub fn distinct_dates(lf: LazyFrame, date_column: &str) -> Result<usize> {
    let df = lf.select([col(date_column).cast(DataType::Date).alias("__d")]).filter(col("__d").is_not_null()).select([col("__d").n_unique().alias("__n")]).collect().context("The dates of the rows could not be counted")?;
    let n = df.column("__n")?.cast(&DataType::UInt64)?;
    Ok(n.u64()?.get(0).unwrap_or(0) as usize)
}

/// The warnings of a pivot (Phase 9 M3): a plain Sum or Count of a column tagged Balance, over rows that span more than one snapshot
/// date, adds balances of different dates. A warning, never a refusal. A pivot with no data tab (outside a project, and every pivot
/// made before M3), a column that is not tagged, or a tab with no date link has none.
pub fn stock_warnings(lf: &LazyFrame, spec: &PivotSpec, schema: &Schema) -> Result<Vec<String>> {
    if spec.data_tab.is_empty() {
        return Ok(Vec::new());
    }
    let plain: Vec<(String, String)> = spec.values.iter().filter(|v| v.time.is_none() && matches!(v.agg, PivotAgg::Sum | PivotAgg::Count)).map(|v| (v.agg.label().to_string(), v.field.clone())).collect();
    let mut rows = lf.clone();
    if let Some(p) = crate::pivot_table::filter_predicate(spec, schema) {
        rows = rows.filter(p);
    }
    stock_warnings_for(rows, &spec.data_tab, &plain)
}

/// The warnings for `plain` values (aggregation label, column) over the rows `rows`, of the data tab `data_tab`.
pub fn stock_warnings_for(rows: LazyFrame, data_tab: &str, plain: &[(String, String)]) -> Result<Vec<String>> {
    let Some(ctx) = model::scope() else {
        return Ok(Vec::new());
    };
    let Some(cal) = &ctx.calendar else {
        return Ok(Vec::new());
    };
    let Some(link) = cal.date_links.iter().find(|l| l.data_tab == data_tab) else {
        return Ok(Vec::new());
    };
    let tagged: Vec<&(String, String)> = plain.iter().filter(|(_, column)| ctx.kinds.iter().any(|k| k.tab == data_tab && &k.column == column && k.kind == crate::measures::ColumnTag::Balance)).collect();
    if tagged.is_empty() {
        return Ok(Vec::new());
    }
    let mut rows = rows;
    if rows.collect_schema().ok().and_then(|s| s.get(link.column.as_str()).cloned()).is_none() {
        return Ok(Vec::new());
    }
    let n = distinct_dates(rows, &link.column)?;
    if n < 2 {
        return Ok(Vec::new());
    }
    Ok(tagged.iter().map(|(label, column)| crate::measures::stock_warning(label, column, n)).collect())
}
