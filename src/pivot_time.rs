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
    /// The dates whose rows make up a time cell's number: (row part, column part, value index) → rows. A blank cell has none.
    pub rows: HashMap<(Key, Key, usize), crate::measures::TimeRows>,
    /// The tab's date column, for Show rows.
    pub date_column: String,
    /// The date column is a Date and time column (Show rows can't pick its days by value).
    pub date_is_datetime: bool,
    /// Value index, first date, last date, as-of date: the span a plain whole-data value covers. Only when the pivot has no Calendar field.
    pub spans: Vec<(usize, NaiveDate, NaiveDate, NaiveDate)>,
    /// Notes of named measures (a ratio of averages of month-ends says so).
    pub extra: Vec<String>,
}

/// The pivot's own Calendar fields: the fields of the date link of its data tab.
pub(crate) fn calendar_fields(spec: &PivotSpec, calendar_tab: &str) -> HashSet<String> {
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
/// One daily series of a time value: a column, how it is aggregated, the rows it reads and its sign.
struct Comp {
    column: String,
    agg: TimeAgg,
    weight: Option<String>,
    filters: Vec<FilterCondition>,
    sign: f64,
    /// A plain time value (no named measure): Show rows can list its rows.
    plain: bool,
    group: usize,
    pos: usize,
}

/// What a time value is made of: one component, or a ratio of signed components (the term's sign beside its index).
enum Res {
    /// A budget or target (Phase 11): the plan series `pc`, set against the actual (a `Single` or a `Ratio`) when the value shows a difference or a percent.
    Plan { name: String, actual: Option<Box<Res>>, pc: usize, agg: crate::measures::PlanAgg, show: crate::measures::PlanShow },
    Single(usize),
    Ratio { name: String, num: Vec<(usize, f64)>, den: Vec<(usize, f64)>, annualise: Option<crate::measures::DayCount>, percent: bool },
}

/// A plan series of a pivot: the amount column of a plan table, by month and the pivot's keys.
struct PlanComp {
    plan_tab: String,
    column: String,
}

/// The components behind a value, in the order its terms are given.
fn res_cis(res: &Res) -> Vec<usize> {
    match res {
        Res::Single(ci) => vec![*ci],
        Res::Ratio { num, den, .. } => num.iter().chain(den.iter()).map(|(ci, _)| *ci).collect(),
        Res::Plan { actual, .. } => actual.as_deref().map(res_cis).unwrap_or_default(),
    }
}

fn column_check(schema: &Schema, who: &str, column: &str, agg: TimeAgg) -> Result<()> {
    let Some(dt) = schema.get(column) else {
        bail!("Pivot: the measure '{}' reads the column '{}', which this tab no longer has. Edit it in Model \u{25b8} Measures\u{2026}.", who, column);
    };
    if agg.needs_number() && !dtype_is_number(dt) {
        bail!("Pivot: the measure '{}' needs a number column, but '{}' is not one. Set it to a number in the schema.", who, column);
    }
    Ok(())
}

fn dtype_is_number(dt: &DataType) -> bool {
    crate::pivot_table::dtype_is_numeric(dt)
}

fn base_comp(m: &crate::measures::NamedMeasure, schema: &Schema, sign: f64) -> Result<Comp> {
    let crate::measures::NamedMeasure::Base { name, column, aggregation, filters, sign: s, weight, .. } = m else {
        bail!("Pivot: '{}' is a ratio; a ratio is made of base measures.", m.name());
    };
    if aggregation.needs_number() || !column.is_empty() {
        column_check(schema, name, column, *aggregation)?;
    }
    Ok(Comp { column: column.clone(), agg: *aggregation, weight: weight.clone(), filters: filters.clone(), sign: sign * s.factor(), plain: false, group: 0, pos: 0 })
}

/// The components of the value `j` of the pivot: its own column, or the base measures behind a named measure.
fn resolve_value(spec: &PivotSpec, j: usize, ctx: &ModelContext, schema: &Schema, comps: &mut Vec<Comp>, plans: &mut Vec<PlanComp>) -> Result<Res> {
    let v = &spec.values[j];
    let t = v.time.unwrap_or_default();
    if t.agg != TimeAgg::Measure {
        comps.push(Comp { column: v.field.clone(), agg: t.agg, weight: v.weight_col.clone(), filters: Vec::new(), sign: 1.0, plain: true, group: 0, pos: 0 });
        return Ok(Res::Single(comps.len() - 1));
    }
    let Some(m) = ctx.measures.iter().find(|m| m.name() == v.field) else {
        bail!("Pivot: the measure '{}' is not in the model of this project. Add it in Model \u{25b8} Measures\u{2026} or remove it from Values.", v.field);
    };
    if m.tab() != spec.data_tab {
        bail!("Pivot: the measure '{}' belongs to another tab. Measures can only be used on the tab they are made for.", v.field);
    }
    resolve_measure(m, ctx, schema, comps, plans)
}

fn resolve_measure(m: &crate::measures::NamedMeasure, ctx: &ModelContext, schema: &Schema, comps: &mut Vec<Comp>, plans: &mut Vec<PlanComp>) -> Result<Res> {
    match m {
        crate::measures::NamedMeasure::Base { .. } => {
            comps.push(base_comp(m, schema, 1.0)?);
            Ok(Res::Single(comps.len() - 1))
        }
        crate::measures::NamedMeasure::Plan { name, plan_tab, column, agg, show, actual, tab } => {
            let Some(sp) = ctx.plans.iter().find(|p| p.link.data_tab == *tab && p.link.plan_tab == *plan_tab) else {
                bail!("Pivot: the measure '{}' needs the plan table linked to this tab. Open Model \u{25b8} Links \u{25b8} Budget and target tables and link it.", name);
            };
            if let Some(r) = &sp.refusal {
                bail!("Pivot: {}", r);
            }
            if sp.table.is_none() {
                bail!("Pivot: the plan table of '{}' is not open. Open it (or Model \u{25b8} Links) to use the measure.", name);
            }
            plans.push(PlanComp { plan_tab: plan_tab.clone(), column: column.clone() });
            let pc = plans.len() - 1;
            let actual_res = if *show == crate::measures::PlanShow::Plan {
                None
            } else {
                let Some(a) = ctx.measures.iter().find(|x| x.name() == actual.as_str()) else {
                    bail!("Pivot: the measure '{}' is set against '{}', which is not in the model. Edit it in Model \u{25b8} Measures\u{2026}.", name, actual);
                };
                if a.is_plan() {
                    bail!("Pivot: the measure '{}' is set against '{}', which is itself a plan measure.", name, actual);
                }
                Some(Box::new(resolve_measure(a, ctx, schema, comps, plans)?))
            };
            Ok(Res::Plan { name: name.clone(), actual: actual_res, pc, agg: *agg, show: *show })
        }
        crate::measures::NamedMeasure::Ratio { name, numerator, denominator, annualise, format, .. } => {
            let mut side = |terms: &[crate::measures::Term], numerator_side: bool| -> Result<Vec<(usize, f64)>> {
                let mut out = Vec::new();
                for term in terms {
                    let Some(b) = ctx.measures.iter().find(|x| x.name() == term.measure) else {
                        bail!("Pivot: the ratio '{}' uses the measure '{}', which is not in the model. Edit the ratio in Model \u{25b8} Measures\u{2026}.", name, term.measure);
                    };
                    if numerator_side && annualise.is_some() {
                        if let crate::measures::NamedMeasure::Base { aggregation, .. } = b {
                            if !crate::measures::is_flow(*aggregation) {
                                bail!("Pivot: {}", crate::measures::annualise_refusal(&term.measure));
                            }
                        }
                    }
                    comps.push(base_comp(b, schema, 1.0)?);
                    out.push((comps.len() - 1, term.sign.factor()));
                }
                Ok(out)
            };
            let num = side(numerator, true)?;
            let den = side(denominator, false)?;
            Ok(Res::Ratio { name: name.clone(), num, den, annualise: *annualise, percent: *format == crate::measures::RatioFormat::Percent })
        }
    }
}

type PlanSeries = HashMap<Vec<Option<String>>, BTreeMap<(i32, u32), f64>>;

/// Reads a plan table into its monthly amounts by the pivot's non-Calendar keys (one map for each key set the pivot's levels use).
/// `fields` are the pivot's field names, `nc_at` the positions of the non-Calendar ones, `wanted` the key sets of the levels.
#[allow(clippy::too_many_arguments)]
fn plan_series_of(ctx: &ModelContext, spec: &PivotSpec, pc: &PlanComp, who: &str, fields: &[&String], nc_at: &[usize], wanted: &[Vec<usize>], filters: &[PivotFilter]) -> Result<Vec<PlanSeries>> {
    let Some(sp) = ctx.plans.iter().find(|p| p.link.data_tab == spec.data_tab && p.link.plan_tab == pc.plan_tab) else {
        bail!("Pivot: the plan table of '{}' is not linked to this tab.", who);
    };
    let Some(table) = &sp.table else {
        bail!("Pivot: the plan table of '{}' is not open.", who);
    };
    let link = &sp.link;
    let mut plf = crate::engine::compile_tab_lazy(&table.source, &table.steps, &ctx.temp_dir).with_context(|| format!("Pivot: the plan table '{}' could not be read", table.title))?;
    let pschema = plf.collect_schema().with_context(|| format!("Pivot: the plan table '{}' could not be read", table.title))?;
    model::plan_columns_ok(link, &pschema, &table.title)?;
    match pschema.get(pc.column.as_str()) {
        None => bail!("Pivot: the plan table '{}' has no column '{}' (used by '{}'). Edit the measure in Model \u{25b8} Measures\u{2026}.", table.title, pc.column, who),
        Some(dt) if !dtype_is_number(dt) => bail!("Pivot: the plan column '{}' of '{}' is not a number. Set it to a number in the schema.", pc.column, table.title),
        _ => {}
    }
    // each non-Calendar field of the pivot must be a key of the plan: a plan can't be split by anything else
    let plan_col_of = |field: &str| -> Option<&String> { link.keys.iter().find(|k| k.data == field).map(|k| &k.lookup) };
    for &i in nc_at {
        if plan_col_of(fields[i]).is_none() {
            let keys: Vec<&str> = link.keys.iter().map(|k| k.data.as_str()).collect();
            bail!("Pivot: '{}' is a plan, which is set by {} only, and the pivot is split by '{}'. Split the pivot by those (or add that column to the plan link).", who, keys.join(" and "), fields[i]);
        }
    }
    // the pivot's filters on the plan's keys apply to the plan too; any other filter can't be applied to it
    for f in filters {
        if f.values.is_empty() {
            continue;
        }
        let Some(pcol) = plan_col_of(&f.field) else {
            bail!("Pivot: '{}' is a plan, which can't be filtered by '{}'. Remove that filter or filter by the plan's keys only.", who, f.field);
        };
        let dt = pschema.get(pcol.as_str()).cloned().unwrap_or(DataType::String);
        let texts: Vec<String> = f.values.iter().flatten().cloned().collect();
        let mut keep = if texts.is_empty() { lit(false) } else { key_text_expr(pcol, &dt).is_in(lit(Series::new("v".into(), texts))) };
        if f.values.iter().any(|v| v.is_none()) {
            keep = keep.or(key_text_expr(pcol, &dt).is_null());
        }
        plf = plf.filter(keep);
    }
    let mut work_exprs: Vec<Expr> = vec![col(link.month_column.as_str()).cast(DataType::Date).cast(DataType::Int32).alias("__pd"), col(pc.column.as_str()).cast(DataType::Float64).alias("__pv")];
    for &i in nc_at {
        let pcol = plan_col_of(fields[i]).cloned().unwrap_or_default();
        let dt = pschema.get(pcol.as_str()).cloned().unwrap_or(DataType::String);
        work_exprs.push(key_text_expr(&pcol, &dt).alias(format!("__k{i}")));
    }
    let work = plf.select(work_exprs).filter(col("__pd").is_not_null());
    let frames: Vec<LazyFrame> = wanted
        .iter()
        .map(|keys| {
            let mut by: Vec<Expr> = keys.iter().map(|i| col(format!("__k{i}"))).collect();
            by.push(col("__pd"));
            work.clone().group_by(by).agg([col("__pv").sum().alias("__sum")])
        })
        .collect();
    let dfs = collect_all(frames).with_context(|| format!("Pivot: the plan table '{}' could not be calculated", table.title))?;
    let mut out: Vec<PlanSeries> = Vec::new();
    for (keys, df) in wanted.iter().zip(dfs.iter()) {
        let key_cols: Vec<Vec<Option<String>>> = keys.iter().map(|i| -> Result<Vec<Option<String>>> { Ok(df.column(&format!("__k{i}"))?.str()?.into_iter().map(|v| v.map(str::to_string)).collect()) }).collect::<Result<_>>()?;
        let days: Vec<Option<i32>> = df.column("__pd")?.i32()?.into_iter().collect();
        let sums: Vec<Option<f64>> = df.column("__sum")?.f64()?.into_iter().collect();
        let mut map: PlanSeries = HashMap::new();
        for r in 0..df.height() {
            let Some(date) = days[r].and_then(crate::dates::date_from_days) else { continue };
            let key: Vec<Option<String>> = key_cols.iter().map(|c| c[r].clone()).collect();
            *map.entry(key).or_default().entry((chrono::Datelike::year(&date), chrono::Datelike::month(&date))).or_insert(0.0) += sums[r].filter(|x| x.is_finite()).unwrap_or(0.0);
        }
        out.push(map);
    }
    Ok(out)
}

/// The plan of a period: the months with a day in it add up (a month that has started counts in full) or the last of them, for the cell's key.
/// A month the plan has no row for is a blank with its reason, never a smaller sum.
fn plan_value(series: Option<&BTreeMap<(i32, u32), f64>>, agg: crate::measures::PlanAgg, days: &[NaiveDate], who: &str, key_text: &str, period_text: &str) -> Measured {
    let mut months: Vec<(i32, u32)> = days.iter().map(|d| (chrono::Datelike::year(d), chrono::Datelike::month(d))).collect();
    months.sort();
    months.dedup();
    if months.is_empty() {
        return Measured::Blank(format!("No days in {} for '{}'", period_text, who));
    }
    let Some(series) = series else {
        return Measured::Blank(format!("The plan has no rows for {} ('{}')", key_text, who));
    };
    let name = |m: (i32, u32)| NaiveDate::from_ymd_opt(m.0, m.1, 1).map(|d| d.format("%b %Y").to_string()).unwrap_or_default();
    match agg {
        crate::measures::PlanAgg::Sum => {
            let mut total = 0.0;
            for m in &months {
                match series.get(m) {
                    Some(v) => total += v,
                    None => return Measured::Blank(format!("The plan has no row for {} in {} ('{}')", key_text, name(*m), who)),
                }
            }
            Measured::Value(total)
        }
        crate::measures::PlanAgg::Closing => {
            let m = months[months.len() - 1];
            match series.get(&m) {
                Some(v) => Measured::Value(*v),
                None => Measured::Blank(format!("The plan has no row for {} in {} ('{}')", key_text, name(m), who)),
            }
        }
    }
}

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
    // the components of the time values: a plain value is one, a named measure is one (a base) or several (a ratio's terms)
    let nc_at: Vec<usize> = (0..fields.len()).filter(|i| !cal_at.contains(i)).collect();
    let time_js: Vec<usize> = spec.values.iter().enumerate().filter(|(_, v)| v.time.is_some()).map(|(j, _)| j).collect();
    let mut comps: Vec<Comp> = Vec::new();
    let mut plans: Vec<PlanComp> = Vec::new();
    let mut resolved: HashMap<usize, Res> = HashMap::new();
    for &j in &time_js {
        let r = resolve_value(spec, j, &ctx, schema, &mut comps, &mut plans)?;
        resolved.insert(j, r);
    }
    let mut groups: Vec<Vec<FilterCondition>> = Vec::new();
    for c in comps.iter_mut() {
        let g = match groups.iter().position(|f| *f == c.filters) {
            Some(g) => g,
            None => {
                groups.push(c.filters.clone());
                groups.len() - 1
            }
        };
        c.group = g;
    }
    let mut in_group: Vec<Vec<usize>> = vec![Vec::new(); groups.len()];
    for (ci, c) in comps.iter_mut().enumerate() {
        c.pos = in_group[c.group].len();
        in_group[c.group].push(ci);
    }

    // one group-by for each distinct set of non-Calendar keys the levels use, in each group of filters
    let mut wanted: Vec<Vec<usize>> = Vec::new();
    for level in levels {
        let k: Vec<usize> = level.iter().copied().filter(|i| !cal_at.contains(i)).collect();
        if !wanted.contains(&k) {
            wanted.push(k);
        }
    }
    let mut frames: Vec<LazyFrame> = Vec::new();
    for (g, filters) in groups.iter().enumerate() {
        let mut tg = tw.clone();
        if !filters.is_empty() {
            tg = crate::engine::filter_by_conditions(tg, filters).context("Pivot: the filters of a named measure could not be applied")?;
        }
        let mut exprs: Vec<Expr> = vec![col(date_col).cast(DataType::Date).alias("__d")];
        for &i in &nc_at {
            let dt = schema.get(fields[i].as_str()).cloned().unwrap_or(DataType::String);
            exprs.push(key_text_expr(fields[i], &dt).alias(format!("__k{i}")));
        }
        for (n, &ci) in in_group[g].iter().enumerate() {
            let c = &comps[ci];
            let dt = schema.get(c.column.as_str()).cloned().unwrap_or(DataType::String);
            if c.agg.needs_number() {
                exprs.push(col(c.column.as_str()).cast(DataType::Float64).alias(format!("__v{n}")));
            } else {
                exprs.push(lit(NULL).cast(DataType::Float64).alias(format!("__v{n}")));
            }
            exprs.push(key_text_expr(&c.column, &dt).alias(format!("__s{n}")));
            match c.weight.as_deref().map(str::trim).filter(|w| !w.is_empty()) {
                Some(w) if c.agg == TimeAgg::WAvgAtClosing => exprs.push(col(w).cast(DataType::Float64).alias(format!("__w{n}"))),
                _ => exprs.push(lit(NULL).cast(DataType::Float64).alias(format!("__w{n}"))),
            }
        }
        let work = tg.select(exprs).filter(col("__d").is_not_null());
        for keys in &wanted {
            let mut aggs: Vec<Expr> = Vec::new();
            for n in 0..in_group[g].len() {
                aggs.push(col(format!("__v{n}")).sum().cast(DataType::Float64).alias(format!("__sum{n}")));
                aggs.push(col(format!("__s{n}")).is_not_null().cast(DataType::Int64).sum().cast(DataType::Float64).alias(format!("__cnt{n}")));
                aggs.push(col(format!("__s{n}")).drop_nulls().n_unique().cast(DataType::Float64).alias(format!("__dst{n}")));
                aggs.push((col(format!("__v{n}")) * col(format!("__w{n}"))).sum().cast(DataType::Float64).alias(format!("__ws{n}")));
                aggs.push(col(format!("__w{n}")).sum().cast(DataType::Float64).alias(format!("__wt{n}")));
            }
            let mut by: Vec<Expr> = keys.iter().map(|i| col(format!("__k{i}"))).collect();
            by.push(col("__d"));
            frames.push(work.clone().group_by(by).agg(aggs));
        }
    }
    let dfs = collect_all(frames).context("Pivot: the daily series could not be calculated")?;
    // the plan tables behind the plan measures (Phase 11), by month and the pivot's keys
    let mut plan_series: Vec<Vec<PlanSeries>> = Vec::new();
    for (pi, pc) in plans.iter().enumerate() {
        let who = resolved.values().find_map(|r| match r {
            Res::Plan { name, pc: x, .. } if *x == pi => Some(name.clone()),
            _ => None,
        });
        plan_series.push(plan_series_of(&ctx, spec, pc, &who.unwrap_or_else(|| pc.column.clone()), &fields, &nc_at, &wanted, &spec_rows.filters)?);
    }
    // key texts -> (date -> stats of each component of the group)
    type Series = HashMap<Vec<Option<String>>, BTreeMap<NaiveDate, Vec<DayStats>>>;
    let mut series: Vec<Vec<Series>> = Vec::new();
    let mut dfi = dfs.iter();
    for g in 0..groups.len() {
        let mut per_keys: Vec<Series> = Vec::new();
        for keys in wanted.iter() {
            let Some(df) = dfi.next() else {
                bail!("Pivot: the daily series of a measure could not be found. Open the pivot again.");
            };
            let key_cols: Vec<Vec<Option<String>>> = keys.iter().map(|i| -> Result<Vec<Option<String>>> { Ok(df.column(&format!("__k{i}"))?.str()?.into_iter().map(|v| v.map(str::to_string)).collect()) }).collect::<Result<_>>()?;
            let d = df.column("__d")?.cast(&DataType::Int32)?;
            let d: Vec<Option<i32>> = d.i32()?.into_iter().collect();
            let num = |name: String| -> Result<Vec<f64>> { Ok(df.column(&name)?.f64()?.into_iter().map(|v| v.filter(|x| x.is_finite()).unwrap_or(0.0)).collect()) };
            let mut stats: Vec<[Vec<f64>; 5]> = Vec::new();
            for n in 0..in_group[g].len() {
                stats.push([num(format!("__sum{n}"))?, num(format!("__cnt{n}"))?, num(format!("__dst{n}"))?, num(format!("__ws{n}"))?, num(format!("__wt{n}"))?]);
            }
            let mut map: Series = HashMap::new();
            for r in 0..df.height() {
                let Some(date) = d[r].and_then(crate::dates::date_from_days) else { continue };
                let key: Vec<Option<String>> = key_cols.iter().map(|c| c[r].clone()).collect();
                let per_date = map.entry(key).or_default().entry(date).or_default();
                if per_date.is_empty() {
                    per_date.resize(in_group[g].len(), DayStats::default());
                }
                for (n, s) in stats.iter().enumerate() {
                    per_date[n] = DayStats { sum: s[0][r], count: s[1][r], distinct: s[2][r], weighted: s[3][r], weight: s[4][r] };
                }
            }
            per_keys.push(map);
        }
        series.push(per_keys);
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

    // M4: what the Calendar fields of Rows and Columns name. A field that names one period (Year, Year-Quarter, Month-Year, Week
    // starting, Date, Fiscal year) gives the cell its period and its unit; one that mixes years (Month, Quarter, Day name ...) gives none.
    let mut period_unit: Option<crate::dates::PeriodUnit> = None;
    let mut mixing: Option<String> = None;
    for &i in &cal_at {
        let column = spec.model_fields.iter().find(|f| &f.name == fields[i]).map(|f| f.column.clone()).unwrap_or_default();
        match crate::dates::DatePart::from_label(&column).and_then(|p| p.period_unit()) {
            Some(u) => period_unit = Some(period_unit.map_or(u, |x| x.min(u))),
            None => {
                if mixing.is_none() {
                    mixing = Some(fields[i].replacen(" \u{b7} ", " \u{203a} ", 1));
                }
            }
        }
    }
    // the refusals of the M4 choices that need no data
    for &j in &time_js {
        let v = &spec.values[j];
        let Some(t) = v.time else { continue };
        if t.show != TimeShow::Value && t.shift == TimeShift::None {
            bail!("Pivot: '{}' shows a difference or a % change, which needs a shift. Choose Prior period or Same period last year, or show the Value.", v.label());
        }
        if t.smooth != Smooth::None && (t.window != TimeWindow::Period || t.shift != TimeShift::None) {
            bail!("Pivot: '{}': a moving average works on the cell's own period. Set the window to Period and the shift to None, or use a rolling window.", v.label());
        }
        if t.is_m4() {
            if let Some(m) = &mixing {
                bail!("Pivot: '{}' mixes years. Use Month-Year (or Year, Year-Quarter, Week starting, Date, Fiscal year) for the period of '{}'.", m, v.label());
            }
            if t.window == TimeWindow::Period && period_unit.is_none() {
                bail!("Pivot: '{}': Add a Calendar field that names a period (Calendar \u{203a} Month-Year, for example) to Rows or Columns, or choose MTD, QTD or YTD.", v.label());
            }
        }
    }

    // Phase 12: a scenario's marks must fit the measures they name (a Volume is an amount, a Rate a ratio in percent)
    if let Some(w) = &ctx.what_if {
        for &j in &time_js {
            let v = &spec.values[j];
            if v.time.unwrap_or_default().agg != TimeAgg::Measure {
                continue;
            }
            if let (Some(role), Some(m)) = (w.role_of(&v.field), ctx.measures.iter().find(|m| m.name() == v.field)) {
                if let Err(e) = crate::whatif::check_role(m, role) {
                    bail!("Pivot: scenario '{}': {}", w.name, e);
                }
            }
        }
    }
    let mut scenario_measures: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    struct PeriodInfo {
        days: Vec<NaiveDate>,
        text: String,
        base: Option<crate::measures::Base>,
    }
    let mut periods: HashMap<Vec<Option<Option<String>>>, PeriodInfo> = HashMap::new();
    let mut out = TimeResult { month_end, notes: HashMap::new(), rows: HashMap::new(), date_column: link.column.clone(), date_is_datetime: schema.get(link.column.as_str()).is_some_and(|d| matches!(d, DataType::Datetime(..))), spans: Vec::new(), extra: Vec::new() };
    // C0: notes for a month whose last calendar day has no rows (kept once each, in order)
    let mut tail_notes: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (rk, ck) in targets {
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
        let entry = periods.entry(cal_key.clone()).or_insert_with(|| {
            if cal_at.is_empty() {
                // no Calendar field: the whole span, from the first snapshot date to the as-of date
                let mut days = Vec::new();
                if let Some(first) = first_snap {
                    days = crate::measures::days_between(first, as_of);
                }
                let text = match (days.first(), days.last()) {
                    (Some(a), Some(b)) => format!("{} to {}", long_date(*a), long_date(*b)),
                    _ => format!("the period up to {}", long_date(as_of)),
                };
                return PeriodInfo { days, text, base: None };
            }
            let mut days: Vec<NaiveDate> = Vec::new();
            let (mut first_day, mut last_day): (Option<NaiveDate>, Option<NaiveDate>) = (None, None);
            for (idx, d) in cal_days.iter().enumerate() {
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
                if !ok {
                    continue;
                }
                first_day = Some(first_day.map_or(*d, |x| x.min(*d)));
                last_day = Some(last_day.map_or(*d, |x| x.max(*d)));
                if *d <= as_of {
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
            let base = match (first_day, last_day, period_unit) {
                (Some(start), Some(end), Some(unit)) => Some(crate::measures::Base { start, end, unit }),
                _ => None,
            };
            PeriodInfo { days, text, base }
        });
        let (period, text) = (&entry.days, &entry.text);
        let empty: BTreeMap<NaiveDate, DayStats> = BTreeMap::new();
        for &j in &time_js {
            let tv = spec.values[j].time.unwrap_or_default();
            let res = &resolved[&j];
            let apply_scenario = |m: Measured, seen: &mut std::collections::BTreeSet<String>| -> Measured {
                match (&ctx.what_if, tv.agg == TimeAgg::Measure) {
                    (Some(w), true) => match (w.role_of(&spec.values[j].field), m) {
                        (Some(role), Measured::Value(v)) if entry.base.is_none_or(|b| b.end >= crate::dates::month_start(as_of)) => {
                            seen.insert(spec.values[j].field.clone());
                            Measured::Value(w.apply(role, v))
                        }
                        (_, other) => other,
                    },
                    _ => m,
                }
            };
            let dates_of = |ci: usize| -> BTreeMap<NaiveDate, DayStats> {
                let c = &comps[ci];
                match series[c.group][which].get(&nc_key) {
                    Some(m) => m.iter().map(|(d, s)| (*d, s.get(c.pos).copied().unwrap_or_default())).collect(),
                    None => empty.clone(),
                }
            };
            let term_dates: Vec<(usize, BTreeMap<NaiveDate, DayStats>)> = res_cis(res).into_iter().map(|ci| (ci, dates_of(ci))).collect();
            let base_value = |res: &Res, days: &[NaiveDate], text: &str| -> Measured {
                let eval = |ci: usize| -> Measured {
                    let per = term_dates.iter().find(|(c, _)| *c == ci).map(|(_, m)| m);
                    let m = evaluate(comps[ci].agg, per.unwrap_or(&empty), days, text, &timeline, month_end);
                    match m {
                        Measured::Value(v) => Measured::Value(v * comps[ci].sign),
                        b => b,
                    }
                };
                match res {
                    Res::Single(ci) => eval(*ci),
                    Res::Ratio { name, num, den, annualise, percent } => {
                        let nums: Vec<(Measured, f64)> = num.iter().map(|(ci, s)| (eval(*ci), *s)).collect();
                        let dens: Vec<(Measured, f64)> = den.iter().map(|(ci, s)| (eval(*ci), *s)).collect();
                        let annual = match (annualise, days.last()) {
                            (Some(dc), Some(end)) if !days.is_empty() => Some(dc.year_days(*end, &timeline.fiscal) / days.len() as f64),
                            _ => None,
                        };
                        crate::measures::ratio_value(name, &nums, &dens, annual, *percent, text)
                    }
                    Res::Plan { .. } => Measured::Blank("A plan measure can't be set against another plan measure.".to_string()),
                }
            };
            let value_in = |days: &[NaiveDate], text: &str| -> Measured {
                let Res::Plan { name, actual, pc, agg, show } = res else {
                    return base_value(res, days, text);
                };
                let key_text = if nc_key.is_empty() { "all lines".to_string() } else { nc_key.iter().map(|k| k.clone().unwrap_or_else(|| crate::pivot_table::BLANK_LABEL.to_string())).collect::<Vec<_>>().join(" / ") };
                let plan = plan_value(plan_series[*pc][which].get(&nc_key), *agg, days, name, &key_text, text);
                let plan_v = match plan {
                    Measured::Value(v) => v,
                    b => return b,
                };
                let act = match actual {
                    None => 0.0,
                    Some(a) => match base_value(a, days, text) {
                        Measured::Value(v) => v,
                        b => return b,
                    },
                };
                match show {
                    crate::measures::PlanShow::Plan => Measured::Value(plan_v),
                    crate::measures::PlanShow::Variance => Measured::Value(act - plan_v),
                    crate::measures::PlanShow::PercentAchieved => {
                        if plan_v == 0.0 {
                            Measured::Blank(format!("The plan of '{}' for {} is 0 in {}, so % achieved can't be worked out.", name, key_text, text))
                        } else {
                            Measured::Value(act / plan_v * 100.0)
                        }
                    }
                }
            };
            let (m, rows_days): (Measured, Vec<NaiveDate>) = if !tv.is_m4() {
                (value_in(period, text), period.clone())
            } else {
                let plan = match crate::measures::plan(&tv, entry.base, as_of, &timeline.fiscal) {
                    Ok(p) => p,
                    Err(e) => bail!("Pivot: '{}': {}", spec.values[j].label(), e),
                };
                // Phase 12: a rolling window or a moving average that reaches back before the first data is blank, never a shorter sum
                let first_month = first_snap.map(crate::dates::month_start);
                let too_early = |start: NaiveDate| first_month.is_some_and(|f| start < f);
                let blank = |why: String| -> (Measured, Vec<NaiveDate>) { (Measured::Blank(why), Vec::new()) };
                if let (Some(n), Some((a, b))) = (tv.window.rolling_months(), plan.current) {
                    if too_early(a) {
                        let first = first_month.unwrap_or(a);
                        let why = format!("Rolling {} months to {} start in {}, before the first data ({}), so the window would be short.", n, long_date(b), a.format("%b %Y"), first.format("%b %Y"));
                        let slot = cells.entry((rk.clone(), ck.clone())).or_insert_with(|| vec![None; spec.values.len()]);
                        if slot.len() < spec.values.len() {
                            slot.resize(spec.values.len(), None);
                        }
                        slot[j] = None;
                        out.notes.insert((rk.clone(), ck.clone(), j), why);
                        continue;
                    }
                }
                if tv.smooth != Smooth::None {
                    let n = tv.smooth.periods();
                    let (m, rows) = match (entry.base, plan.current) {
                        (Some(b), Some((cs, ce))) => {
                            let point_in_time = res_cis(res).iter().all(|ci| matches!(comps[*ci].agg, TimeAgg::ClosingBalance | TimeAgg::OpeningBalance | TimeAgg::CountAtClosing | TimeAgg::DistinctCountAtClosing | TimeAgg::WAvgAtClosing));
                            if ce < b.end && !point_in_time {
                                blank(format!("{} is not finished (data to {}), so a moving average that includes it would mix a part period with whole periods.", text, long_date(ce)))
                            } else {
                                let mut sum = 0.0;
                                let mut failed: Option<String> = None;
                                for k in 0..n as i32 {
                                    let (s0, e0) = if k == 0 {
                                        (cs, ce)
                                    } else {
                                        use crate::dates::{add_months, add_months_end, PeriodUnit};
                                        match b.unit {
                                            PeriodUnit::Month => (add_months(b.start, -k), add_months_end(b.end, -k)),
                                            PeriodUnit::Quarter => (add_months(b.start, -3 * k), add_months_end(b.end, -3 * k)),
                                            PeriodUnit::Year => (add_months(b.start, -12 * k), add_months_end(b.end, -12 * k)),
                                            PeriodUnit::Week => (b.start - chrono::Duration::days(7 * k as i64), b.end - chrono::Duration::days(7 * k as i64)),
                                            PeriodUnit::Day => (b.start - chrono::Duration::days(k as i64), b.end - chrono::Duration::days(k as i64)),
                                        }
                                    };
                                    if too_early(s0) {
                                        failed = Some(format!("The {}-period moving average of {} reaches back to {}, before the first data ({}).", n, text, long_date(s0), first_month.map(|f| f.format("%b %Y").to_string()).unwrap_or_default()));
                                        break;
                                    }
                                    let days = crate::measures::days_between(s0, e0.min(as_of));
                                    let t0 = format!("{} to {}", long_date(s0), long_date(e0.min(as_of)));
                                    match value_in(&days, &t0) {
                                        Measured::Value(v) => sum += v,
                                        Measured::Blank(why) => {
                                            failed = Some(format!("The {}-period moving average of {} has no number for {}: {}", n, text, t0, why));
                                            break;
                                        }
                                    }
                                }
                                match failed {
                                    Some(w) => blank(w),
                                    None => (Measured::Value(sum / n as f64), period.clone()),
                                }
                            }
                        }
                        _ => blank(format!("{} has no days up to the as-of date.", text)),
                    };
                    let m = apply_scenario(m, &mut scenario_measures);
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
                    let _ = rows;
                    continue;
                }
                let (cur_days, cur_text): (Vec<NaiveDate>, String) = match plan.current {
                    None => (Vec::new(), text.clone()),
                    Some((a, b)) if tv.window == TimeWindow::Period => {
                        let _ = (a, b);
                        (period.clone(), text.clone())
                    }
                    Some((a, b)) => (crate::measures::days_between(a, b), format!("{} to {}", long_date(a), long_date(b))),
                };
                let current = value_in(&cur_days, &cur_text);
                match plan.earlier.filter(|_| plan.current.is_some()) {
                    None => (current, cur_days),
                    Some((a, b)) => {
                        let e_days = crate::measures::days_between(a, b);
                        let e_text = format!("{} to {}", long_date(a), long_date(b));
                        let earlier = value_in(&e_days, &e_text);
                        let shows_earlier = tv.show == TimeShow::Value;
                        let rows_days = if shows_earlier { e_days } else { cur_days };
                        (crate::measures::combine(tv.show, true, current, Some((earlier, e_text))), rows_days)
                    }
                }
            };
            if m.value().is_some() {
                let income_after = match res {
                    Res::Single(ci) => comps[*ci].agg == TimeAgg::PeriodAmount,
                    Res::Ratio { num, den, annualise, .. } => annualise.is_some() || num.iter().chain(den.iter()).any(|(ci, _)| comps[*ci].agg == TimeAgg::PeriodAmount),
                    Res::Plan { .. } => res_cis(res).iter().any(|ci| comps[*ci].agg == TimeAgg::PeriodAmount),
                };
                if let (true, Some(b)) = (income_after, entry.base) {
                    if let Some(n) = month_end_note(&b, text, &timeline) {
                        tail_notes.insert(n);
                    }
                }
                if let Res::Single(ci) = res {
                    if comps[*ci].plain {
                        if let Some(r) = crate::measures::rows_of(comps[*ci].agg, &rows_days, &timeline, month_end) {
                            out.rows.insert((rk.clone(), ck.clone(), j), r);
                        }
                    }
                }
            }
            // Phase 12: the active scenario changes the marked measures from the as-of month on; closed history stays actual
            let m = apply_scenario(m, &mut scenario_measures);
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
    }
    out.extra.extend(tail_notes);
    if let (Some(w), false) = (&ctx.what_if, scenario_measures.is_empty()) {
        let names: Vec<&str> = scenario_measures.iter().map(String::as_str).collect();
        out.extra.push(w.note(&names));
    }
    if month_end {
        for (j, r) in &resolved {
            if let Res::Ratio { name, num, den, .. } = r {
                if num.iter().chain(den.iter()).any(|(ci, _)| comps[*ci].agg == TimeAgg::AverageBalance) {
                    out.extra.push(format!("{}: its average terms are averages of month-ends (the tab holds month-end data).", name));
                }
                let _ = j;
            }
        }
        out.extra.sort();
        out.extra.dedup();
    }
    // a plain value with no Calendar field keeps M3's whole-span number: say which span that is
    if cal_at.is_empty() {
        if let Some(first) = first_snap.filter(|f| *f <= as_of) {
            for &j in &time_js {
                if !spec.values[j].time.unwrap_or_default().is_m4() {
                    out.spans.push((j, first, as_of, as_of));
                }
            }
        }
    }
    Ok(out)
}

/// C0: "The last date with rows in Oct 2026 is 29 Oct; income posted after it counts in Nov 2026." when a finished month's last calendar
/// day has no rows (a Friday or Saturday month-end). The numbers are unchanged; `None` when the month's last day has rows, when the
/// month is not over at the as-of date, when it has no rows at all, or when the period is not a month.
pub fn month_end_note(base: &crate::measures::Base, period_text: &str, tl: &crate::measures::Timeline) -> Option<String> {
    if base.unit != crate::dates::PeriodUnit::Month || base.end > tl.as_of {
        return None;
    }
    let last = tl.snapshots.iter().copied().filter(|d| *d >= base.start && *d <= base.end).max()?;
    if last >= base.end {
        return None;
    }
    let next = base.end.succ_opt()?;
    Some(format!("The last date with rows in {} is {}; income posted after it counts in {}.", period_text, last.format("%-d %b"), next.format("%b %Y")))
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
