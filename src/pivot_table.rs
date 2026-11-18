//! The pivot table engine (Phase 4 V1): Rows, Columns, Values and Filters, with row and column
//! totals, subtotals, "show values as" and natural ordering of the labels.
//!
//! The data is aggregated by Polars (one pass per group-by level); the small result is arranged
//! into the wide table here, so the order of the labels is ours: numbers and dates by value,
//! other text in the natural case-insensitive order ("Zone 2" before "Zone 10", "02. 2–7 Days"
//! before "10. 3–5 Years"), blanks last, and never by size unless `sort_by_value` asks for it.
//!
//! Totals are computed from the data (not added up from the cells), so an average, a weighted
//! average, a minimum or a distinct count has a true total too.

use crate::banking::weighted_rate_exprs;
use crate::engine::natural_sort_key;
use crate::sort_lists::SortOrder;
use crate::models::*;
use anyhow::{bail, Context, Result};
use polars::prelude::*;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

/// The text of a blank row or column label.
pub const BLANK_LABEL: &str = "(blank)";
/// The label of the last row.
pub const GRAND_TOTAL_LABEL: &str = "Grand total";
/// The label of the total column.
pub const TOTAL_LABEL: &str = "Total";
/// The most columns (column labels × values) a pivot may have.
pub const MAX_PIVOT_COLUMNS: usize = 500;
/// The most rows a pivot may have.
pub const MAX_PIVOT_ROWS: usize = 200_000;
/// The first column of a pivot without Rows.
pub const NO_ROWS_COLUMN: &str = "Row";
/// Its only label.
pub const NO_ROWS_LABEL: &str = "All rows";

type Key = Vec<Option<String>>;

/// The data of one aggregation level: key (row part, column part) → one number per value field.
type Cells = HashMap<(Key, Key), Vec<Option<f64>>>;

/// Rows of the table that are only totals.
#[derive(Clone, Debug, PartialEq)]
enum RowKind {
    Detail(Key),
    /// The subtotal of the first `k` row fields.
    Subtotal(usize, Key),
    Grand,
}

#[derive(Clone, Debug)]
enum ColKind {
    Cell(Key, usize),
    RowTotal(usize),
}

/// What the dialog and the checks need to know about a built pivot.
pub struct PivotBuild {
    pub df: DataFrame,
    /// Number of label columns at the left (the Rows fields, or one).
    pub label_columns: usize,
}

fn dtype_is_numeric(dt: &DataType) -> bool {
    dt.is_numeric()
}

fn dtype_is_date(dt: &DataType) -> bool {
    matches!(dt, DataType::Date | DataType::Datetime(_, _))
}

/// The text of a key field with blank (null, or text of only spaces) as null.
fn key_text_expr(name: &str, dt: &DataType) -> Expr {
    if dt.is_float() {
        // a whole number is written without ".0": 9, not 9.0
        let f = col(name).cast(DataType::Float64);
        let whole = f.clone().eq(f.clone().floor()).and(f.clone().abs().lt(lit(1e15)));
        return when(whole).then(f.clone().cast(DataType::Int64).cast(DataType::String)).otherwise(f.cast(DataType::String));
    }
    let c = col(name).cast(DataType::String);
    if dt.is_string() || dt.is_categorical() {
        when(c.clone().str().strip_chars(lit(NULL)).eq(lit("")))
            .then(lit(NULL).cast(DataType::String))
            .otherwise(c)
    } else {
        c
    }
}

/// Checks the settings against the columns the data has, with messages a banker can act on.
pub fn validate_pivot(schema: &Schema, spec: &PivotSpec) -> Result<()> {
    if spec.values.is_empty() {
        bail!("Pivot: add at least one field to Values (for example Sum of Balance).");
    }
    if spec.rows.is_empty() && spec.columns.is_empty() {
        bail!("Pivot: add a field to Rows or to Columns.");
    }
    let mut seen: HashSet<&str> = HashSet::new();
    for (area, fields) in [("Rows", &spec.rows), ("Columns", &spec.columns)] {
        for f in fields.iter() {
            if schema.get(f.as_str()).is_none() {
                bail!("Pivot: column '{}' (in {}) was not found. Remove it or pick another column.", f, area);
            }
            if !seen.insert(f.as_str()) {
                bail!("Pivot: '{}' is used twice in Rows and Columns. Use each field once.", f);
            }
        }
    }
    for f in &spec.filters {
        if schema.get(f.field.as_str()).is_none() {
            bail!("Pivot: filter column '{}' was not found. Remove the filter or pick another column.", f.field);
        }
    }
    let mut labels: HashSet<String> = HashSet::new();
    for v in &spec.values {
        let dt = schema
            .get(v.field.as_str())
            .ok_or_else(|| anyhow::anyhow!("Pivot: column '{}' (in Values) was not found. Remove it or pick another column.", v.field))?;
        let needs_number = matches!(v.agg, PivotAgg::Sum | PivotAgg::Average | PivotAgg::Min | PivotAgg::Max | PivotAgg::WeightedAvg);
        if needs_number && !dtype_is_numeric(dt) {
            bail!(
                "Pivot: '{}' needs a number column, but '{}' is {}. Set it to a number in the schema (or with Set to Number), or use Count or Count distinct.",
                v.label(),
                v.field,
                if dt.is_string() { "text".to_string() } else if dtype_is_date(dt) { "a date".to_string() } else { dt.to_string() }
            );
        }
        if v.agg == PivotAgg::WeightedAvg {
            let w = v.weight_col.as_deref().map(str::trim).filter(|w| !w.is_empty());
            let Some(w) = w else {
                bail!("Pivot: '{}' needs a weight column. Choose one (usually Balance).", v.label());
            };
            let wdt = schema.get(w).ok_or_else(|| anyhow::anyhow!("Pivot: the weight column '{}' of '{}' was not found.", w, v.label()))?;
            if !dtype_is_numeric(wdt) {
                bail!("Pivot: the weight column '{}' of '{}' is not a number.", w, v.label());
            }
        }
        if !labels.insert(v.label()) {
            bail!("Pivot: '{}' is in Values twice. Remove one.", v.label());
        }
    }
    if spec.show_as != PivotShowAs::NoCalculation {
        if let Some(v) = spec.values.iter().find(|v| !v.agg.is_additive()) {
            bail!(
                "Pivot: '{}' cannot be shown as a share ({}): parts of an {} do not add up to its total. Use Sum or Count, or choose No calculation.",
                v.label(),
                spec.show_as.label(),
                v.agg.label().to_lowercase()
            );
        }
    }
    if let Some(s) = &spec.sort_by_value {
        if s.value_index >= spec.values.len() {
            bail!("Pivot: 'Sort by value' points to a value field that is no longer there. Choose it again.");
        }
    }
    Ok(())
}

/// The Polars expressions of one value field (a number in `__a{j}`, and `__m{j}` = 1 when the
/// weights of a weighted average mix positive and negative).
fn value_exprs(j: usize, v: &PivotValue) -> Vec<Expr> {
    let a = format!("__a{j}");
    match v.agg {
        PivotAgg::Sum => vec![col(format!("__v{j}")).sum().cast(DataType::Float64).alias(&a)],
        PivotAgg::Average => vec![col(format!("__v{j}")).mean().cast(DataType::Float64).alias(&a)],
        PivotAgg::Min => vec![col(format!("__v{j}")).min().cast(DataType::Float64).alias(&a)],
        PivotAgg::Max => vec![col(format!("__v{j}")).max().cast(DataType::Float64).alias(&a)],
        PivotAgg::Count => vec![col(format!("__s{j}")).is_not_null().cast(DataType::Int64).sum().cast(DataType::Float64).alias(&a)],
        PivotAgg::CountDistinct => vec![col(format!("__s{j}")).drop_nulls().n_unique().cast(DataType::Float64).alias(&a)],
        PivotAgg::WeightedAvg => {
            let (rate, mixed) = weighted_rate_exprs(col(format!("__v{j}")), col(format!("__w{j}")));
            vec![rate.cast(DataType::Float64).alias(&a), mixed.cast(DataType::Int64).alias(format!("__m{j}"))]
        }
    }
}

/// The filter of a pivot table as one condition (`None` when nothing is restricted).
fn filter_predicate(spec: &PivotSpec, schema: &Schema) -> Option<Expr> {
    let mut all: Option<Expr> = None;
    for f in &spec.filters {
        if f.values.is_empty() {
            continue;
        }
        let Some(dt) = schema.get(f.field.as_str()) else {
            continue;
        };
        let text = key_text_expr(&f.field, dt);
        let wanted: Vec<String> = f.values.iter().flatten().cloned().collect();
        let has_blank = f.values.iter().any(|v| v.is_none());
        let mut pred = lit(false);
        if !wanted.is_empty() {
            pred = pred.or(text.clone().is_in(lit(Series::new("wanted".into(), wanted))).fill_null(lit(false)));
        }
        if has_blank {
            pred = pred.or(text.is_null());
        }
        all = Some(match all {
            Some(a) => a.and(pred),
            None => pred,
        });
    }
    all
}

/// The data with only the columns the pivot needs, under their working names: `__k{i}` (key text),
/// `__o{i}` (key order, numbers and dates), `__v{j}`, `__w{j}`, `__s{j}`.
fn working_frame(lf: LazyFrame, spec: &PivotSpec, schema: &Schema) -> LazyFrame {
    let lf = match filter_predicate(spec, schema) {
        Some(p) => lf.filter(p),
        None => lf,
    };
    let mut exprs: Vec<Expr> = Vec::new();
    for (i, f) in spec.rows.iter().chain(spec.columns.iter()).enumerate() {
        let dt = schema.get(f.as_str()).cloned().unwrap_or(DataType::String);
        exprs.push(key_text_expr(f, &dt).alias(format!("__k{i}")));
        if dtype_is_numeric(&dt) {
            exprs.push(col(f.as_str()).cast(DataType::Float64).alias(format!("__o{i}")));
        } else if dtype_is_date(&dt) {
            exprs.push(col(f.as_str()).cast(DataType::Date).cast(DataType::Int32).cast(DataType::Float64).alias(format!("__o{i}")));
        }
    }
    for (j, v) in spec.values.iter().enumerate() {
        let dt = schema.get(v.field.as_str()).cloned().unwrap_or(DataType::String);
        match v.agg {
            PivotAgg::Count | PivotAgg::CountDistinct => exprs.push(key_text_expr(&v.field, &dt).alias(format!("__s{j}"))),
            _ => exprs.push(col(v.field.as_str()).cast(DataType::Float64).alias(format!("__v{j}"))),
        }
        if v.agg == PivotAgg::WeightedAvg {
            if let Some(w) = v.weight_col.as_deref().map(str::trim).filter(|w| !w.is_empty()) {
                exprs.push(col(w).cast(DataType::Float64).alias(format!("__w{j}")));
            }
        }
    }
    lf.select(exprs)
}

/// The aggregation of `work` by the key fields `keys` (indexes into rows then columns); no keys
/// is one row for the whole data.
fn aggregate(work: &LazyFrame, spec: &PivotSpec, keys: &[usize], order_keys: &[usize]) -> LazyFrame {
    let mut aggs: Vec<Expr> = Vec::new();
    for (j, v) in spec.values.iter().enumerate() {
        aggs.extend(value_exprs(j, v));
    }
    if keys.is_empty() {
        return work.clone().select(aggs);
    }
    for &i in keys {
        if order_keys.contains(&i) {
            aggs.push(col(format!("__o{i}")).first().alias(format!("__o{i}")));
        }
    }
    work.clone().group_by(keys.iter().map(|i| col(format!("__k{i}"))).collect::<Vec<_>>()).agg(aggs)
}

fn read_cells(
    df: &DataFrame,
    spec: &PivotSpec,
    keys: &[usize],
    row_n: usize,
    out: &mut Cells,
    orders: &mut [HashMap<Option<String>, f64>],
    mixed: &mut [usize],
) -> Result<()> {
    let key_cols: Vec<Vec<Option<String>>> = keys
        .iter()
        .map(|i| -> Result<Vec<Option<String>>> {
            Ok(df.column(&format!("__k{i}"))?.str()?.into_iter().map(|v| v.map(str::to_string)).collect())
        })
        .collect::<Result<_>>()?;
    let order_cols: Vec<Option<Vec<Option<f64>>>> = keys
        .iter()
        .map(|i| df.column(&format!("__o{i}")).ok().and_then(|s| s.f64().ok().map(|c| c.into_iter().collect())))
        .collect();
    let mut value_cols: Vec<Vec<Option<f64>>> = Vec::new();
    let mut mixed_cols: Vec<Option<Vec<Option<i64>>>> = Vec::new();
    for j in 0..spec.values.len() {
        value_cols.push(df.column(&format!("__a{j}"))?.f64()?.into_iter().map(|x| x.filter(|v| v.is_finite())).collect());
        mixed_cols.push(df.column(&format!("__m{j}")).ok().and_then(|s| s.i64().ok().map(|c| c.into_iter().collect())));
    }
    for r in 0..df.height() {
        let mut rk: Key = Vec::new();
        let mut ck: Key = Vec::new();
        for (pos, &i) in keys.iter().enumerate() {
            let text = key_cols.get(pos).and_then(|c| c.get(r)).cloned().flatten();
            if let Some(Some(oc)) = order_cols.get(pos).and_then(|o| o.as_ref()).map(|c| c.get(r).copied().flatten()) {
                orders[i].insert(text.clone(), oc);
            }
            if i < row_n {
                rk.push(text);
            } else {
                ck.push(text);
            }
        }
        for (j, m) in mixed_cols.iter().enumerate() {
            if let Some(m) = m {
                if m.get(r).copied().flatten().unwrap_or(0) > 0 {
                    mixed[j] += 1;
                }
            }
        }
        out.insert((rk, ck), value_cols.iter().map(|c| c.get(r).copied().flatten()).collect());
    }
    Ok(())
}

fn natural_cmp(a: &str, b: &str) -> Ordering {
    natural_sort_key(a).cmp(&natural_sort_key(b)).then_with(|| a.cmp(b))
}

/// One label of a key field: blanks last, numbers and dates by value, text in natural order.
fn cmp_label(orders: &HashMap<Option<String>, f64>, numeric: bool, order: &SortOrder, a: &Option<String>, b: &Option<String>) -> Ordering {
    if !order.is_natural() {
        // a custom list or Tenor (U3): blanks last, then the listed or parsed values in order
        return order.compare(a, b);
    }
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => {
            if numeric {
                if let (Some(p), Some(q)) = (orders.get(a), orders.get(b)) {
                    return p.partial_cmp(q).unwrap_or(Ordering::Equal);
                }
            }
            natural_cmp(x, y)
        }
    }
}

fn shown(k: &Option<String>) -> String {
    k.clone().unwrap_or_else(|| BLANK_LABEL.to_string())
}

fn key_label(k: &Key) -> String {
    k.iter().map(shown).collect::<Vec<_>>().join(" / ")
}

/// The pivot table of `lf` as a frame: label columns (the Rows fields), then the value columns
/// (one per column label and value field, then the totals), and a last row of column totals.
pub fn build_pivot(lf: LazyFrame, spec: &PivotSpec) -> Result<PivotBuild> {
    let mut lf = lf;
    let schema = lf.collect_schema().context("Could not read the column types for this pivot")?;
    validate_pivot(&schema, spec)?;
    let (nr, nc) = (spec.rows.len(), spec.columns.len());
    let all_fields: Vec<&String> = spec.rows.iter().chain(spec.columns.iter()).collect();
    let numeric_key: Vec<bool> = all_fields
        .iter()
        .map(|f| schema.get(f.as_str()).map(|dt| dtype_is_numeric(dt) || dtype_is_date(dt)).unwrap_or(false))
        .collect();

    let work = working_frame(lf, spec, &schema);
    let rows_idx: Vec<usize> = (0..nr).collect();
    let cols_idx: Vec<usize> = (nr..nr + nc).collect();
    let all_idx: Vec<usize> = (0..nr + nc).collect();

    // the levels to aggregate; the first is the cells
    let mut levels: Vec<Vec<usize>> = vec![all_idx.clone()];
    if nc > 0 && nr > 0 {
        levels.push(rows_idx.clone()); // row totals
        levels.push(cols_idx.clone()); // column totals
    }
    levels.push(Vec::new()); // grand total
    // the subtotals (and the order by value of nested rows) need the first k row fields
    for k in 1..nr {
        let prefix: Vec<usize> = (0..k).collect();
        levels.push(prefix.clone());
        if nc > 0 {
            let mut with_cols = prefix;
            with_cols.extend(cols_idx.iter().copied());
            levels.push(with_cols);
        }
    }
    let order_keys: Vec<usize> = (0..nr + nc).filter(|i| numeric_key[*i]).collect();
    let frames: Vec<LazyFrame> = levels.iter().enumerate().map(|(n, keys)| aggregate(&work, spec, keys, if n == 0 { &order_keys } else { &[] })).collect();
    let dfs = collect_all(frames).context("Could not calculate the pivot table")?;

    let mut cells: Cells = HashMap::new();
    let mut orders: Vec<HashMap<Option<String>, f64>> = vec![HashMap::new(); nr + nc];
    let mut mixed = vec![0usize; spec.values.len()];
    for (keys, df) in levels.iter().zip(dfs.iter()) {
        read_cells(df, spec, keys, nr, &mut cells, &mut orders, &mut mixed)?;
    }

    // the keys that exist
    let mut row_keys: Vec<Key> = Vec::new();
    let mut col_keys: Vec<Key> = Vec::new();
    {
        let mut seen_r: HashSet<Key> = HashSet::new();
        let mut seen_c: HashSet<Key> = HashSet::new();
        for (rk, ck) in cells.keys() {
            if rk.len() == nr && ck.len() == nc {
                if seen_r.insert(rk.clone()) {
                    row_keys.push(rk.clone());
                }
                if seen_c.insert(ck.clone()) {
                    col_keys.push(ck.clone());
                }
            }
        }
    }
    if row_keys.len() > MAX_PIVOT_ROWS {
        bail!("Pivot: {} rows is too many (the most is {}). Remove a field from Rows or filter first.", row_keys.len(), MAX_PIVOT_ROWS);
    }
    if col_keys.len() * spec.values.len() > MAX_PIVOT_COLUMNS {
        bail!(
            "Pivot: {} columns is too many (the most is {}). Remove a field from Columns, use fewer values or filter first.",
            col_keys.len() * spec.values.len(),
            MAX_PIVOT_COLUMNS
        );
    }

    let sort = spec.sort_by_value;
    let field_orders: Vec<SortOrder> = all_fields
        .iter()
        .map(|f| spec.field_orders.iter().find(|o| o.field == **f).map(|o| o.order.clone()).unwrap_or_default())
        .collect();
    let by_labels = |a: &Key, b: &Key, idx0: usize| -> Ordering {
        for (p, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            let o = cmp_label(&orders[idx0 + p], numeric_key[idx0 + p], &field_orders[idx0 + p], x, y);
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    };
    row_keys.sort_by(|a, b| {
        if let Some(s) = sort {
            // level by level: the group's total first, then its label
            for k in 1..=a.len() {
                let total = |key: &Key| -> Option<f64> { cells.get(&(key[..k].to_vec(), Vec::new())).and_then(|v| v.get(s.value_index).copied().flatten()) };
                let (ta, tb) = (total(a), total(b));
                let o = match (ta, tb) {
                    (Some(x), Some(y)) => {
                        let o = x.partial_cmp(&y).unwrap_or(Ordering::Equal);
                        if s.descending { o.reverse() } else { o }
                    }
                    (None, None) => Ordering::Equal,
                    (None, Some(_)) => Ordering::Greater,
                    (Some(_), None) => Ordering::Less,
                };
                let o = o.then_with(|| cmp_label(&orders[k - 1], numeric_key[k - 1], &field_orders[k - 1], &a[k - 1], &b[k - 1]));
                if o != Ordering::Equal {
                    return o;
                }
            }
            Ordering::Equal
        } else {
            by_labels(a, b, 0)
        }
    });
    col_keys.sort_by(|a, b| by_labels(a, b, nr));

    // the rows of the table
    let mut out_rows: Vec<RowKind> = Vec::new();
    for (n, rk) in row_keys.iter().enumerate() {
        out_rows.push(RowKind::Detail(rk.clone()));
        if spec.subtotals && nr >= 2 {
            let next = row_keys.get(n + 1);
            for k in (1..nr).rev() {
                let ends = match next {
                    Some(nx) => nx[..k] != rk[..k],
                    None => true,
                };
                if ends {
                    out_rows.push(RowKind::Subtotal(k, rk[..k].to_vec()));
                }
            }
        }
    }
    let show_grand_row = spec.column_totals && nr > 0;
    if show_grand_row {
        out_rows.push(RowKind::Grand);
    }
    if nr == 0 {
        out_rows = vec![RowKind::Grand];
    }

    // the value columns
    let multi = spec.values.len() > 1;
    let mut out_cols: Vec<(String, ColKind)> = Vec::new();
    if nc == 0 {
        for (j, v) in spec.values.iter().enumerate() {
            out_cols.push((v.label(), ColKind::Cell(Vec::new(), j)));
        }
    } else {
        for ck in &col_keys {
            for (j, v) in spec.values.iter().enumerate() {
                let name = if multi { format!("{} | {}", key_label(ck), v.label()) } else { key_label(ck) };
                out_cols.push((name, ColKind::Cell(ck.clone(), j)));
            }
        }
        if spec.row_totals {
            for (j, v) in spec.values.iter().enumerate() {
                let name = if multi { format!("{} | {}", TOTAL_LABEL, v.label()) } else { TOTAL_LABEL.to_string() };
                out_cols.push((name, ColKind::RowTotal(j)));
            }
        }
    }
    // names must be unique
    let mut used: HashSet<String> = spec.rows.iter().cloned().collect();
    for (name, _) in out_cols.iter_mut() {
        let base = name.clone();
        let mut n = 2;
        while !used.insert(name.clone()) {
            *name = format!("{base} ({n})");
            n += 1;
        }
    }

    // a cell of the table, before "show values as"
    let lookup = |r: &RowKind, c: &ColKind, j: usize| -> Option<f64> {
        let rowpart: Key = match r {
            RowKind::Detail(k) => k.clone(),
            RowKind::Subtotal(_, k) => k.clone(),
            RowKind::Grand => Vec::new(),
        };
        let colpart: Key = match c {
            ColKind::Cell(k, _) => k.clone(),
            ColKind::RowTotal(_) => Vec::new(),
        };
        cells.get(&(rowpart, colpart)).and_then(|v| v.get(j).copied().flatten())
    };
    let denominator = |r: &RowKind, c: &ColKind, j: usize| -> Option<f64> {
        let rowpart: Key = match r {
            RowKind::Detail(k) | RowKind::Subtotal(_, k) => k.clone(),
            RowKind::Grand => Vec::new(),
        };
        let colpart: Key = match c {
            ColKind::Cell(k, _) => k.clone(),
            ColKind::RowTotal(_) => Vec::new(),
        };
        let key = match spec.show_as {
            PivotShowAs::NoCalculation => return None,
            PivotShowAs::PctOfRowTotal => (rowpart, Vec::new()),
            PivotShowAs::PctOfColumnTotal => (Vec::new(), colpart),
            PivotShowAs::PctOfGrandTotal => (Vec::new(), Vec::new()),
        };
        cells.get(&key).and_then(|v| v.get(j).copied().flatten())
    };

    // label columns
    let label_names: Vec<String> = if nr == 0 { vec![NO_ROWS_COLUMN.to_string()] } else { spec.rows.clone() };
    let mut label_cols: Vec<Vec<String>> = vec![Vec::new(); label_names.len()];
    for r in &out_rows {
        let labels: Vec<String> = match r {
            RowKind::Detail(k) => k.iter().map(shown).collect(),
            RowKind::Subtotal(k, key) => {
                let mut l: Vec<String> = key.iter().map(shown).collect();
                if let Some(last) = l.last_mut() {
                    *last = format!("{} {}", last, TOTAL_LABEL);
                }
                l.resize(nr, String::new());
                let _ = k;
                l
            }
            RowKind::Grand => {
                if nr == 0 {
                    vec![NO_ROWS_LABEL.to_string()]
                } else {
                    let mut l = vec![String::new(); nr];
                    l[0] = GRAND_TOTAL_LABEL.to_string();
                    l
                }
            }
        };
        for (i, c) in label_cols.iter_mut().enumerate() {
            c.push(labels.get(i).cloned().unwrap_or_default());
        }
    }
    let mut series: Vec<Series> = label_names.iter().zip(label_cols).map(|(n, c)| Series::new(n.as_str().into(), c)).collect();
    for (name, ck) in &out_cols {
        let j = match ck {
            ColKind::Cell(_, j) | ColKind::RowTotal(j) => *j,
        };
        let values: Vec<Option<f64>> = out_rows
            .iter()
            .map(|r| {
                let v = lookup(r, ck, j);
                if spec.show_as == PivotShowAs::NoCalculation {
                    v
                } else {
                    match (v, denominator(r, ck, j)) {
                        (Some(x), Some(d)) if d != 0.0 => Some(x / d * 100.0),
                        _ => None,
                    }
                }
            })
            .collect();
        let is_count = matches!(spec.values[j].agg, PivotAgg::Count | PivotAgg::CountDistinct) && spec.show_as == PivotShowAs::NoCalculation;
        if is_count {
            let ints: Vec<Option<i64>> = values.iter().map(|v| v.map(|x| x.round() as i64)).collect();
            series.push(Series::new(name.as_str().into(), ints));
        } else {
            series.push(Series::new(name.as_str().into(), values));
        }
    }
    let df = DataFrame::new(series).context("Could not arrange the pivot table")?;
    Ok(PivotBuild { df, label_columns: label_names.len() })
}

/// The pivot as a step of a pipeline: the table, then lazy again.
pub fn apply_pivot_table(lf: LazyFrame, spec: &PivotSpec) -> Result<LazyFrame> {
    Ok(build_pivot(lf, spec)?.df.lazy())
}

/// For each weighted-average value field: how many cells (groups of Rows × Columns) have weights
/// of both signs, and so no rate. For the status-bar note.
pub fn pivot_mixed_sign_cells(lf: LazyFrame, spec: &PivotSpec) -> Result<Vec<(String, usize)>> {
    let mut lf = lf;
    let schema = lf.collect_schema().context("Could not read the column types for this pivot")?;
    validate_pivot(&schema, spec)?;
    if !spec.values.iter().any(|v| v.agg == PivotAgg::WeightedAvg) {
        return Ok(Vec::new());
    }
    let (nr, nc) = (spec.rows.len(), spec.columns.len());
    let work = working_frame(lf, spec, &schema);
    let keys: Vec<usize> = (0..nr + nc).collect();
    let df = aggregate(&work, spec, &keys, &[]).collect().context("Could not check the weights for mixed signs")?;
    let mut out = Vec::new();
    for (j, v) in spec.values.iter().enumerate() {
        if v.agg != PivotAgg::WeightedAvg {
            continue;
        }
        let n = df
            .column(&format!("__m{j}"))
            .ok()
            .and_then(|c| c.i64().ok().map(|c| c.into_iter().flatten().filter(|m| *m > 0).count()))
            .unwrap_or(0);
        if n > 0 {
            out.push((v.label(), n));
        }
    }
    Ok(out)
}

/// The distinct values of a field for the Filters checklist, as text (`None` = blank), in natural
/// order with blank last, at most `limit` of them; the flag says there are more.
pub fn distinct_field_values(lf: LazyFrame, field: &str, limit: usize) -> Result<(Vec<Option<String>>, bool)> {
    let mut lf = lf;
    let schema = lf.collect_schema().context("Could not read the column types")?;
    let dt = schema
        .get(field)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Column '{}' was not found.", field))?;
    let numeric = dtype_is_numeric(&dt) || dtype_is_date(&dt);
    let order = if dtype_is_numeric(&dt) {
        col(field).cast(DataType::Float64)
    } else if dtype_is_date(&dt) {
        col(field).cast(DataType::Date).cast(DataType::Int32).cast(DataType::Float64)
    } else {
        lit(NULL).cast(DataType::Float64)
    };
    let df = lf
        .select([key_text_expr(field, &dt).alias("__k"), order.alias("__o")])
        .group_by([col("__k")])
        .agg([col("__o").first()])
        .sort([if numeric { "__o" } else { "__k" }], SortMultipleOptions::default().with_nulls_last(true))
        .limit((limit + 1) as IdxSize)
        .collect()
        .with_context(|| format!("Could not list the values of '{}'", field))?;
    let texts: Vec<Option<String>> = df.column("__k")?.str()?.into_iter().map(|v| v.map(str::to_string)).collect();
    let orders: Vec<Option<f64>> = df.column("__o")?.f64()?.into_iter().collect();
    let mut order_map: HashMap<Option<String>, f64> = HashMap::new();
    for (t, o) in texts.iter().zip(orders.iter()) {
        if let Some(o) = o {
            order_map.insert(t.clone(), *o);
        }
    }
    let mut values = texts;
    values.sort_by(|a, b| cmp_label(&order_map, numeric, &SortOrder::Natural, a, b));
    let more = values.len() > limit;
    values.truncate(limit);
    Ok((values, more))
}

/// How one value column of a pivot is shown. The pivot's columns carry names of banks or buckets,
/// which say nothing about money or rates, so each one gets an explicit format: a share as a
/// percentage, a count as a whole number, a rate as it is (never divided into Crore), and money
/// in the pivot's scale (or the global one).
pub fn pivot_value_format(v: &PivotValue, spec: &PivotSpec, decimals: usize) -> ColumnFormatSpec {
    if spec.show_as != PivotShowAs::NoCalculation {
        return ColumnFormatSpec { fmt_type: MetricFormatType::PercentAsIs, decimals: 2 };
    }
    if matches!(v.agg, PivotAgg::Count | PivotAgg::CountDistinct) {
        return ColumnFormatSpec { fmt_type: MetricFormatType::StandardCommas, decimals: 0 };
    }
    match classify_column_auto_format(&v.field) {
        MetricFormatType::Auto => ColumnFormatSpec { fmt_type: spec.scale.format_type().unwrap_or(MetricFormatType::Auto), decimals },
        other => ColumnFormatSpec { fmt_type: other, decimals },
    }
}

/// The formats of the value columns of a pivot result with these `columns` (all columns of the
/// result: the label columns come first).
pub fn pivot_column_formats(spec: &PivotSpec, columns: &[String], decimals: usize) -> Vec<(String, ColumnFormatSpec)> {
    let label_n = spec.rows.len().max(1);
    if spec.values.is_empty() {
        return Vec::new();
    }
    columns
        .iter()
        .skip(label_n)
        .enumerate()
        .map(|(p, name)| (name.clone(), pivot_value_format(&spec.values[p % spec.values.len()], spec, decimals)))
        .collect()
}

// ------------------------------------------------------------------ converting an old pivot step

/// What converting an old "Group/Pivot" step (`GroupByPivot`) to the new pivot (`PivotTable`) does:
/// the new settings, the output columns whose names change, and what the new pivot cannot keep.
#[derive(Clone, Debug, PartialEq)]
pub struct PivotConversion {
    pub spec: PivotSpec,
    /// (old column name, new column name), only the columns whose name changes.
    pub renames: Vec<(String, String)>,
    /// Other changes the owner must know about before confirming.
    pub notes: Vec<String>,
}

/// Plans the conversion of an old `GroupByPivot` step. Nothing is changed here; the caller shows
/// `renames` and `notes` and replaces the step only after the user confirms. A step the new
/// pivot cannot express exactly is refused with the reason.
pub fn convert_group_by_pivot(step: &TransformStep) -> Result<PivotConversion> {
    let TransformStep::GroupByPivot { group_cols, pivot_col, aggs, sort_by_col, top_n } = step.inner() else {
        bail!("Only an old Group/Pivot step can be converted.");
    };
    if group_cols.is_empty() {
        bail!("This Group/Pivot step has no grouping columns, so there is nothing to convert. Tick the grouping columns first.");
    }
    if aggs.is_empty() {
        bail!("This Group/Pivot step has no metrics, so there is nothing to convert. Add a metric first.");
    }
    let pct_count = aggs.iter().filter(|a| a.func == AggFunc::PctOfTotal).count();
    if pct_count != 0 && pct_count != aggs.len() {
        bail!(
            "This step mixes '% of total' metrics with other metrics. The new pivot shows all its values the same way ('Show values as'), so it cannot keep both. Split the step in two or remove one kind, then convert."
        );
    }
    let mut values: Vec<PivotValue> = Vec::with_capacity(aggs.len());
    for a in aggs {
        let (agg, weight_col) = match a.func {
            AggFunc::Sum | AggFunc::PctOfTotal => (PivotAgg::Sum, None),
            AggFunc::Count => (PivotAgg::Count, None),
            AggFunc::Mean => (PivotAgg::Average, None),
            AggFunc::Min => (PivotAgg::Min, None),
            AggFunc::Max => (PivotAgg::Max, None),
            AggFunc::WeightedAvg => {
                let w = a.weight_col.as_deref().map(str::trim).filter(|w| !w.is_empty()).ok_or_else(|| {
                    anyhow::anyhow!("The metric '{}' is a weighted average without a weight column; choose one before converting.", a.alias)
                })?;
                (PivotAgg::WeightedAvg, Some(w.to_string()))
            }
        };
        let v = PivotValue { field: a.target_col.clone(), agg, weight_col };
        if values.iter().any(|o| o.label() == v.label()) {
            bail!(
                "The metrics '{}' and another one both calculate '{}'. The new pivot allows each calculation once; remove one, then convert.",
                a.alias,
                v.label()
            );
        }
        values.push(v);
    }
    let renames: Vec<(String, String)> = aggs
        .iter()
        .zip(&values)
        .filter(|(a, v)| a.alias != v.label())
        .map(|(a, v)| (a.alias.clone(), v.label()))
        .collect();
    let mut notes: Vec<String> = Vec::new();
    let mut sort_by_value = None;
    if let Some(sc) = sort_by_col.as_deref() {
        match aggs.iter().position(|a| a.alias == sc) {
            Some(i) => sort_by_value = Some(PivotSortByValue { value_index: i, descending: true }),
            None => notes.push(format!("The old step sorts by '{sc}', which is not a metric; the new pivot sorts the rows by their labels instead.")),
        }
    }
    if let Some(n) = top_n.filter(|n| *n > 0) {
        notes.push(format!("Top {n}: the new pivot has no Top-N limit, so it shows every row, not the first {n}."));
    }
    if pivot_col.as_deref().map(str::trim).is_some_and(|p| !p.is_empty()) {
        notes.push("The old step's pivot column was never used for the output; the new pivot has no Columns field set.".to_string());
    }
    if aggs.iter().any(|a| a.format_spec.fmt_type != MetricFormatType::Auto || a.format_spec.decimals != 2) {
        notes.push("Each metric's own format and decimals are replaced by the new pivot's Scale (as in View).".to_string());
    }
    if pct_count > 0 {
        notes.push("'% of total' metrics become Sum values shown as '% of column total' (the same shares, not rounded to 2 decimals in the data).".to_string());
    }
    notes.push("Rows are listed in natural label order, and the new pivot adds no Grand total row or Total column (switch them on with Edit pivot).".to_string());
    Ok(PivotConversion {
        spec: PivotSpec {
            rows: group_cols.clone(),
            columns: Vec::new(),
            values,
            filters: Vec::new(),
            row_totals: false,
            column_totals: false,
            subtotals: false,
            show_as: if pct_count > 0 { PivotShowAs::PctOfColumnTotal } else { PivotShowAs::NoCalculation },
            scale: PivotScale::Global,
            sort_by_value,
            field_orders: Vec::new(),
        },
        renames,
        notes,
    })
}
