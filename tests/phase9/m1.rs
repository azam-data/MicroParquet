//! M1: lookup fields. A lookup column seen from a data tab (a pivot's Rows, Columns, Filters and Values, a chart's category, series,
//! facet and drill level, a filter condition) is added by a left join on the worker and dropped after, so the data tab's row count
//! never changes. A key with no lookup row is "(no match)", a blank key "(blank key)", and a matched lookup row with a blank cell "(blank)"; all three sort last.

use crate::common::{dir, floats, parquet, strings};
use micro_parquet::charts::spec::{ChartSpec, DrillLevel};
use micro_parquet::engine::{compile_tab_lazy, filter_by_conditions};
use micro_parquet::model::*;
use micro_parquet::models::*;
use micro_parquet::pivot_table::{build_pivot, cell_rows};
use polars::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;

fn strs(name: &str, v: &[&str]) -> Series {
    Series::new(name.into(), v.to_vec())
}
fn opt_strs(name: &str, v: &[Option<&str>]) -> Series {
    Series::new(name.into(), v.to_vec())
}
fn nums(name: &str, v: &[f64]) -> Series {
    Series::new(name.into(), v.to_vec())
}

/// Accounts: nine rows. B1, B2 are Dhaka, B3 Khulna, B4 Chattogram; B9 has no branch row; two keys are blank.
/// Dhaka 610, Khulna 400, Chattogram 800, no match 50, blank 30; all 1,890.
pub fn accounts() -> DataFrame {
    DataFrame::new(vec![
        opt_strs("Branch", &[Some("B1"), Some("B1"), Some("B2"), Some("B3"), Some("B4"), Some("B9"), None, Some("  "), Some("B2")]),
        strs("Wing", &["Conventional", "Islami", "Conventional", "Conventional", "Islami", "Conventional", "Conventional", "Islami", "Islami"]),
        strs("Product code", &["SB01", "SB01", "FD01", "SB01", "FD01", "SB01", "FD01", "SB01", "FD01"]),
        nums("Balance", &[100.0, 300.0, 200.0, 400.0, 800.0, 50.0, 25.0, 5.0, 10.0]),
        strs("AC Number", &["A1", "A2", "A3", "A4", "A5", "A6", "A7", "A8", "A9"]),
    ])
    .expect("accounts")
}

fn master(key: &str) -> DataFrame {
    DataFrame::new(vec![
        strs(key, &["B1", "B2", "B3", "B4", "B5"]),
        strs("Branch name", &["Gulshan", "Motijheel", "Khulna Main", "Agrabad", "Sylhet"]),
        strs("Region", &["Dhaka", "Dhaka", "Khulna", "Chattogram", "Sylhet"]),
        nums("Limit", &[1000.0, 2000.0, 3000.0, 4000.0, 5000.0]),
    ])
    .expect("master")
}

pub struct Fx {
    pub d: PathBuf,
    pub data: ModelTab,
    pub lookup: ModelTab,
    pub link: Link,
}

fn tab(key: &str, title: &str, source: TabSource) -> ModelTab {
    ModelTab { key: key.into(), title: title.into(), source, steps: vec![] }
}

pub fn fx(name: &str) -> Fx {
    let d = dir(name);
    let data = tab("t1", "Accounts", parquet(&d, "accounts", &accounts()));
    let lookup = tab("t2", "Branch master", parquet(&d, "master", &master("Code")));
    let link = Link { data_tab: "t1".into(), lookup_tab: "t2".into(), keys: vec![KeyPair { data: "Branch".into(), lookup: "Code".into() }], unmatched_value: None };
    Fx { d, data, lookup, link }
}

pub fn scope_of(f: &Fx, refusal: Option<String>) -> Option<Arc<ModelContext>> {
    Some(Arc::new(ModelContext { links: vec![ScopeLink { link: f.link.clone(), lookup: Some(f.lookup.clone()), refusal }], temp_dir: f.d.clone(), ..Default::default() }))
}

pub fn field(f: &Fx, column: &str) -> ModelFieldRef {
    ModelFieldRef::new(&f.link, "Accounts \u{2192} Branch master", "Branch master", column)
}

pub fn lazy(f: &Fx) -> LazyFrame {
    compile_tab_lazy(&f.data.source, &[], &f.d).expect("the data tab")
}

fn sum_balance() -> PivotValue {
    PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None }
}

pub fn pivot_of(rows: &[&ModelFieldRef]) -> PivotSpec {
    PivotSpec {
        rows: rows.iter().map(|r| r.name.clone()).collect(),
        values: vec![sum_balance()],
        row_totals: true,
        column_totals: true,
        model_fields: rows.iter().map(|r| (*r).clone()).collect(),
        ..Default::default()
    }
}

/// The label column and the first value column of a pivot, as (label, number) pairs.
pub fn pairs(df: &DataFrame) -> Vec<(String, f64)> {
    let names: Vec<String> = df.get_column_names().iter().map(|n| n.to_string()).collect();
    let labels = strings(df, &names[0]);
    let vals = floats(df, &names[1]);
    labels.into_iter().zip(vals).map(|(l, v)| (l.unwrap_or_else(|| "(blank)".to_string()), v.unwrap_or(f64::NAN))).collect()
}

pub fn run_pivot(f: &Fx, spec: &PivotSpec) -> Vec<(String, f64)> {
    let b = with_scope(scope_of(f, None), || build_pivot(lazy(f), spec)).expect("the pivot");
    pairs(&b.df)
}

fn sum_of(df: &DataFrame, col: &str) -> f64 {
    floats(df, col).into_iter().flatten().sum()
}

fn in_scope<R>(f: &Fx, run: impl FnOnce() -> R) -> R {
    with_scope(scope_of(f, None), run)
}

fn keep(conditions: Vec<FilterCondition>) -> TransformStep {
    TransformStep::FilterGroup { conditions, join: GroupJoin::And, keep_matching: true }
}

fn picked(r: &ModelFieldRef, values: &[&str], blank: bool) -> FilterCondition {
    // as the pickers store it (M1-4): "(no match)" is the condition's own flag, never a value
    let mut c = FilterCondition { column: r.name.clone(), op: CondOp::InList, or_blank: blank, lookup: Some(r.clone()), ..Default::default() };
    c.set_picked(values.iter().map(|v| v.to_string()).collect());
    c
}

fn ids(df: &DataFrame) -> Vec<String> {
    strings(df, "AC Number").into_iter().flatten().collect()
}

#[test]
fn m1_a_pivot_by_a_lookup_field_equals_the_join_tools_pivot_with_no_match_holding_the_unmatched_sum_and_the_total_is_the_ungrouped_total() {
    let f = fx("p9m1_pivot");
    let region = field(&f, "Region");
    let ours = run_pivot(&f, &pivot_of(&[&region]));
    let labels: Vec<&str> = ours.iter().map(|p| p.0.as_str()).collect();
    assert_eq!(labels, ["Chattogram", "Dhaka", "Khulna", "(blank key)", "(no match)", "Grand total"], "values in order, then (blank key), then (no match)");
    let at = |l: &str| ours.iter().find(|p| p.0 == l).expect(l).1;
    assert_eq!((at("Chattogram"), at("Dhaka"), at("Khulna")), (800.0, 610.0, 400.0));
    assert_eq!((at("(blank key)"), at("(no match)")), (30.0, 50.0), "the two blank keys and the unmatched key B9 are told apart");
    assert_eq!(at("Grand total"), sum_of(&accounts(), "Balance"), "the total equals the ungrouped total");
    assert_eq!(at("Grand total"), 1890.0);

    // the same pivot on the table the Join tool makes
    let joined_master = parquet(&f.d, "master_join", &master("Branch"));
    let join = TabSource::JoinedTabs {
        left_source: Box::new(f.data.source.clone()),
        left_steps: vec![],
        right_source: Box::new(joined_master),
        right_steps: vec![],
        join_kind: SqlJoinKind::Left,
        join_keys: vec!["Branch".into()],
        right_cols_to_keep: vec!["Region".into()],
        keep_first_duplicate_right: false,
    };
    let joined = compile_tab_lazy(&join, &[], &f.d).expect("the join");
    let theirs = pairs(&build_pivot(joined, &PivotSpec { rows: vec!["Region".into()], values: vec![sum_balance()], row_totals: true, column_totals: true, ..Default::default() }).expect("the pivot of the join").df);
    for region in ["Chattogram", "Dhaka", "Khulna"] {
        assert_eq!(theirs.iter().find(|p| p.0 == region).map(|p| p.1), Some(at(region)), "{region}: row for row");
    }
    let their_blank = theirs.iter().find(|p| p.0 == "(blank)").expect("the join leaves the unmatched and blank keys blank").1;
    assert_eq!(their_blank, at("(blank key)") + at("(no match)"), "what the Join tool leaves blank is (blank key) and (no match) together");
    assert_eq!(theirs.last().map(|p| p.1), Some(at("Grand total")));
}

#[test]
fn m1_linking_never_changes_the_data_tabs_row_count_on_any_fixture() {
    let f = fx("p9m1_rows");
    let wants = |names: &[&str]| -> Vec<LookupWant> { names.iter().map(|n| LookupWant { unmatched_out: None, blank_key_out: None, field: field(&f, n), out: field(&f, n).name, as_: LookupAs::Text }).collect() };
    let rows = |lf: LazyFrame| lf.collect().expect("collect").height();
    let base = rows(lazy(&f));
    assert_eq!(base, 9);
    // every kind of fixture: matched, unmatched, blank keys, several lookup columns, numbers as text, a lookup with unused rows
    for names in [&["Region"][..], &["Region", "Branch name"], &["Region", "Branch name", "Limit"]] {
        let n = in_scope(&f, || rows(with_lookup_columns(lazy(&f), &wants(names), "Pivot").expect("joined")));
        assert_eq!(n, base, "{names:?}: the row count is the data tab's");
    }
    // a lookup with no row for any key, and a lookup of one row
    for (i, lk) in [DataFrame::new(vec![strs("Code", &["Z1"]), strs("Branch name", &["x"]), strs("Region", &["r"]), nums("Limit", &[1.0])]).expect("lk"), master("Code").head(Some(1))].into_iter().enumerate() {
        let g = Fx { d: f.d.clone(), data: f.data.clone(), lookup: tab("t2", "Branch master", parquet(&f.d, &format!("lk{i}"), &lk)), link: f.link.clone() };
        let n = with_scope(scope_of(&g, None), || rows(with_lookup_columns(lazy(&g), &wants(&["Region"]), "Pivot").expect("joined")));
        assert_eq!(n, base, "lookup fixture {i}");
    }
    // a composite key
    let g = composite();
    let n = with_scope(scope_of(&g, None), || rows(with_lookup_columns(lazy(&g), &[LookupWant { unmatched_out: None, blank_key_out: None, field: field(&g, "Group"), out: field(&g, "Group").name, as_: LookupAs::Text }], "Pivot").expect("joined")));
    assert_eq!(n, base, "a composite key");
    // a filter step on a lookup field leaves its rows, never more
    let step = keep(vec![picked(&field(&f, "Region"), &["Dhaka"], false)]);
    let kept = in_scope(&f, || compile_tab_lazy(&f.data.source, &[step.clone()], &f.d).and_then(|lf| Ok(lf.collect()?))).expect("filter");
    assert!(kept.height() <= base);
    assert_eq!(kept.get_column_names().iter().map(|n| n.to_string()).collect::<Vec<_>>(), ["Branch", "Wing", "Product code", "Balance", "AC Number"], "no column is added to the tab");
}

fn composite() -> Fx {
    let d = dir("p9m1_composite");
    let data = tab("t1", "Accounts", parquet(&d, "accounts", &accounts()));
    let products = DataFrame::new(vec![
        strs("Wing", &["Conventional", "Conventional", "Islami", "Islami"]),
        strs("Code", &["SB01", "FD01", "SB01", "FD01"]),
        strs("Group", &["Savings", "Term deposit", "Mudaraba savings", "Mudaraba term"]),
    ])
    .expect("products");
    let lookup = tab("t2", "Product master", parquet(&d, "products", &products));
    let link = Link { data_tab: "t1".into(), lookup_tab: "t2".into(), keys: vec![KeyPair { data: "Wing".into(), lookup: "Wing".into() }, KeyPair { data: "Product code".into(), lookup: "Code".into() }], unmatched_value: None };
    Fx { d, data, lookup, link }
}

#[test]
fn m1_a_composite_key_gives_the_right_product_group_for_sb01_in_each_wing() {
    let g = composite();
    let group = ModelFieldRef::new(&g.link, "Accounts \u{2192} Product master", "Product master", "Group");
    let spec = PivotSpec { rows: vec!["Wing".into(), group.name.clone()], values: vec![sum_balance()], model_fields: vec![group.clone()], row_totals: true, column_totals: true, ..Default::default() };
    let b = with_scope(scope_of(&g, None), || build_pivot(lazy(&g), &spec)).expect("the pivot");
    let names: Vec<String> = b.df.get_column_names().iter().map(|n| n.to_string()).collect();
    let (wing, grp, val) = (strings(&b.df, &names[0]), strings(&b.df, &names[1]), floats(&b.df, &names[2]));
    let of = |w: &str, gr: &str| -> Option<f64> { (0..wing.len()).find(|&i| wing[i].as_deref() == Some(w) && grp[i].as_deref() == Some(gr)).and_then(|i| val[i]) };
    assert_eq!(of("Conventional", "Savings"), Some(100.0 + 400.0 + 50.0), "SB01 in the Conventional wing is Savings (A1, A4, A6)");
    assert_eq!(of("Islami", "Mudaraba savings"), Some(300.0 + 5.0), "SB01 in the Islami wing is Mudaraba savings (A2, A8)");
    assert_eq!(of("Conventional", "Mudaraba savings"), None, "never crossed");
    assert_eq!(of("Islami", "Savings"), None);
    assert_eq!(of("Conventional", "Term deposit"), Some(200.0 + 25.0));
    assert_eq!(of("Islami", "Mudaraba term"), Some(800.0 + 10.0));
}

#[test]
fn m1_a_filter_on_a_lookup_field_keeps_exactly_that_regions_rows_and_nothing_flows_back_to_the_lookup() {
    let f = fx("p9m1_filter");
    let region = field(&f, "Region");
    let before = compile_tab_lazy(&f.lookup.source, &f.lookup.steps, &f.d).expect("lookup").collect().expect("collect");
    let step = keep(vec![picked(&region, &["Dhaka"], false)]);
    let out = in_scope(&f, || compile_tab_lazy(&f.data.source, &[step.clone()], &f.d).and_then(|lf| Ok(lf.collect()?))).expect("filter");
    assert_eq!(ids(&out), ["A1", "A2", "A3", "A9"], "exactly the rows of Dhaka's branches B1 and B2, in the tab's order");
    assert_eq!(sum_of(&out, "Balance"), 610.0);
    let after = compile_tab_lazy(&f.lookup.source, &f.lookup.steps, &f.d).expect("lookup").collect().expect("collect");
    assert!(before.equals(&after), "the branch master tab's rows are unchanged (no flow back)");
    assert_eq!(after.height(), 5);
    // leaving Dhaka out keeps the other seven, the unmatched and blank keys included
    let out = in_scope(&f, || {
        compile_tab_lazy(&f.data.source, &[TransformStep::FilterGroup { conditions: vec![picked(&region, &["Dhaka"], false)], join: GroupJoin::And, keep_matching: false }], &f.d).and_then(|lf| Ok(lf.collect()?))
    })
    .expect("filter");
    assert_eq!(ids(&out), ["A4", "A5", "A6", "A7", "A8"]);
    // a condition that is not on a lookup field is untouched
    let out = in_scope(&f, || {
        compile_tab_lazy(&f.data.source, &[keep(vec![FilterCondition { column: "Wing".into(), op: CondOp::InList, values: vec!["Islami".into()], ..Default::default() }, picked(&region, &["Dhaka"], false)])], &f.d)
            .and_then(|lf| Ok(lf.collect()?))
    })
    .expect("filter");
    assert_eq!(ids(&out), ["A2", "A9"], "both conditions apply");
}

#[test]
fn m1_a_filter_on_no_match_keeps_exactly_the_unmatched_rows_and_blank_keys_are_picked_apart() {
    let f = fx("p9m1_nomatch");
    let region = field(&f, "Region");
    let run = |c: FilterCondition| -> Vec<String> {
        let out = in_scope(&f, || compile_tab_lazy(&f.data.source, &[keep(vec![c])], &f.d).and_then(|lf| Ok(lf.collect()?))).expect("filter");
        ids(&out)
    };
    assert_eq!(run(picked(&region, &["(no match)"], false)), ["A6"], "B9 has no branch row: exactly that row");
    assert_eq!(run(picked(&region, &["(blank key)"], false)), ["A7", "A8"], "(blank key) is the two blank keys, not the unmatched one");
    assert_eq!(run(picked(&region, &[], true)), Vec::<String>::new(), "(blank) is a matched row with a blank cell: there is none here");
    assert_eq!(run(picked(&region, &["(no match)", "(blank key)"], false)), ["A6", "A7", "A8"], "both can be picked together");
    let not = FilterCondition { op: CondOp::NotInList, ..picked(&region, &["(no match)", "(blank key)"], false) };
    assert_eq!(run(not), ["A1", "A2", "A3", "A4", "A5", "A9"], "everything else");
    // the same two as the pivot's cells: Show rows of the (no match) and (blank) rows
    let spec = pivot_of(&[&region]);
    let b = in_scope(&f, || build_pivot(lazy(&f), &spec)).expect("pivot");
    let names: Vec<String> = b.df.get_column_names().iter().map(|n| n.to_string()).collect();
    let labels: Vec<String> = pairs(&b.df).into_iter().map(|p| p.0).collect();
    for (label, rows, sum) in [("(no match)", vec!["A6"], 50.0), ("(blank key)", vec!["A7", "A8"], 30.0), ("Dhaka", vec!["A1", "A2", "A3", "A9"], 610.0)] {
        let row = labels.iter().position(|l| l == label).expect(label);
        let c = cell_rows(&spec, &b, row, &names[1]).expect("show rows");
        let out = in_scope(&f, || filter_by_conditions(lazy(&f), &c.conditions).and_then(|lf| Ok(lf.collect()?))).expect("rows");
        assert_eq!(ids(&out), rows, "{label}: the rows tab has the cell's rows");
        assert_eq!(sum_of(&out, "Balance"), sum, "{label}: its control total equals the cell");
    }
}

#[test]
fn m1_a_chart_by_a_lookup_field_equals_the_pivot_and_a_drill_region_to_branch_name_adds_up_and_its_marks_show_their_rows() {
    let f = fx("p9m1_chart");
    let region = field(&f, "Region");
    let name = field(&f, "Branch name");
    let spec = ChartSpec { category: region.name.clone(), category_model: Some(region.clone()), value: Some("Balance".into()), agg: micro_parquet::charts::spec::ChartAgg::Sum, ..Default::default() };
    let d = in_scope(&f, || micro_parquet::charts::calc::compute(lazy(&f), &spec)).expect("the chart");
    assert_eq!(d.categories, ["Chattogram", "Dhaka", "Khulna", "(blank key)", "(no match)"], "(blank key) then (no match), last");
    let pivot = run_pivot(&f, &pivot_of(&[&region]));
    for (i, c) in d.categories.iter().enumerate() {
        assert_eq!(d.values[0][i], pivot.iter().find(|p| &p.0 == c).map(|p| p.1), "{c}: the chart shows the pivot's number");
    }
    assert_eq!(d.values[0].iter().flatten().sum::<f64>(), 1890.0);

    // Show rows from a chart mark
    let conditions = spec.mark_conditions("Dhaka", None, None).expect("the mark");
    let out = in_scope(&f, || filter_by_conditions(lazy(&f), &conditions).and_then(|lf| Ok(lf.collect()?))).expect("rows");
    assert_eq!((out.height(), sum_of(&out, "Balance")), (4, 610.0), "the rows tab of the mark equals the mark");
    let conditions = spec.mark_conditions("(no match)", None, None).expect("the mark");
    let out = in_scope(&f, || filter_by_conditions(lazy(&f), &conditions).and_then(|lf| Ok(lf.collect()?))).expect("rows");
    assert_eq!((ids(&out), sum_of(&out, "Balance")), (vec!["A6".to_string()], 50.0));

    // a drill Region > Branch name: each level adds up to the level above
    let mut drill = ChartSpec { value: Some("Balance".into()), agg: micro_parquet::charts::spec::ChartAgg::Sum, ..Default::default() };
    drill.set_drill(vec![DrillLevel::of_model(&region), DrillLevel::of_model(&name)]);
    assert_eq!(drill.category, region.name);
    assert_eq!(drill.category_model.as_ref(), Some(&region), "the level's lookup field is the chart's category");
    let top = in_scope(&f, || micro_parquet::charts::calc::compute(lazy(&f), &drill)).expect("level 1");
    assert_eq!(top.categories, ["Chattogram", "Dhaka", "Khulna", "(blank key)", "(no match)"]);
    assert!(drill.drill_down("Dhaka"));
    assert_eq!(drill.category, name.name, "one level down");
    let low = in_scope(&f, || micro_parquet::charts::calc::compute(lazy(&f), &drill)).expect("level 2");
    assert_eq!(low.categories.len(), 2, "Dhaka has two branches: Gulshan and Motijheel");
    let (gul, mot) = (low.values[0][low.categories.iter().position(|c| c == "Gulshan").expect("Gulshan")], low.values[0][low.categories.iter().position(|c| c == "Motijheel").expect("Motijheel")]);
    assert_eq!((gul, mot), (Some(400.0), Some(210.0)));
    assert_eq!(low.values[0].iter().flatten().sum::<f64>(), top.values[0][top.categories.iter().position(|c| c == "Dhaka").expect("Dhaka")].expect("Dhaka's total"), "Dhaka's branches add up to Dhaka");
    // the rows of the mark Gulshan: both levels' conditions
    let conditions = drill.mark_conditions("Gulshan", None, None).expect("the mark");
    let out = in_scope(&f, || filter_by_conditions(lazy(&f), &conditions).and_then(|lf| Ok(lf.collect()?))).expect("rows");
    assert_eq!((ids(&out), sum_of(&out, "Balance")), (vec!["A1".to_string(), "A2".to_string()], 400.0));
    // series and facet: stored beside the category and left behind with it
    let both = ChartSpec { category: "Wing".into(), series: Some(region.name.clone()), series_model: Some(region.clone()), value: Some("Balance".into()), agg: micro_parquet::charts::spec::ChartAgg::Sum, ..Default::default() };
    let d = in_scope(&f, || micro_parquet::charts::calc::compute(lazy(&f), &both)).expect("a lookup series");
    assert_eq!(d.series.last().map(String::as_str), Some("(no match)"));
    let faceted = ChartSpec { facet: Some(region.name.clone()), facet_model: Some(region.clone()), category: "Wing".into(), value: Some("Balance".into()), agg: micro_parquet::charts::spec::ChartAgg::Sum, ..Default::default() };
    let d = in_scope(&f, || micro_parquet::charts::calc::compute(lazy(&f), &faceted)).expect("lookup panels");
    let panels: Vec<String> = d.facets.iter().map(|p| p.name.clone()).collect();
    assert_eq!(panels.iter().rev().take(2).rev().cloned().collect::<Vec<_>>(), ["(blank key)", "(no match)"], "{panels:?}");
    let mut stale = both.clone();
    stale.series = Some("Product code".into());
    stale.drop_stale_date_parts();
    assert!(stale.series_model.is_none() && stale.model_fields().is_empty(), "a lookup field left behind after another column was picked is ignored");
    assert_eq!(micro_parquet::charts::library::missing_columns(&spec, &["Balance".to_string()]), Vec::<String>::new(), "a lookup field is not a column of the tab");
}

#[test]
fn m1_a_numeric_lookup_field_in_values_sums_and_counts_and_leaves_the_unmatched_rows_blank_not_zero() {
    let f = fx("p9m1_values");
    let limit = field(&f, "Limit");
    let region = field(&f, "Region");
    let spec = PivotSpec {
        rows: vec!["Wing".into()],
        values: vec![
            PivotValue { field: limit.name.clone(), agg: PivotAgg::Sum, weight_col: None, time: None },
            PivotValue { field: limit.name.clone(), agg: PivotAgg::Count, weight_col: None, time: None },
            PivotValue { field: region.name.clone(), agg: PivotAgg::CountDistinct, weight_col: None, time: None },
        ],
        model_fields: vec![limit.clone(), region.clone()],
        row_totals: false,
        column_totals: true,
        ..Default::default()
    };
    let b = in_scope(&f, || build_pivot(lazy(&f), &spec)).expect("the pivot");
    let names: Vec<String> = b.df.get_column_names().iter().map(|n| n.to_string()).collect();
    let wing = strings(&b.df, &names[0]);
    let at = |w: &str, col: usize| floats(&b.df, &names[col])[wing.iter().position(|x| x.as_deref() == Some(w)).expect(w)];
    // Conventional: A1 B1 1000, A3 B2 2000, A4 B3 3000, A6 B9 (no match), A7 blank key: 3 matched rows
    assert_eq!(at("Conventional", 1), Some(6000.0), "the unmatched and blank rows add nothing, and are not counted as 0");
    assert_eq!(at("Conventional", 2), Some(3.0), "Count counts the rows that have a value");
    assert_eq!(at("Conventional", 3), Some(2.0), "Dhaka and Khulna: (no match) is not a value of Count distinct");
    // a numeric field as a label is text
    let by_limit = PivotSpec { rows: vec![limit.name.clone()], values: vec![sum_balance()], model_fields: vec![limit.clone()], row_totals: true, column_totals: true, ..Default::default() };
    let p = run_pivot(&f, &by_limit);
    assert_eq!(p.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["1000", "2000", "3000", "4000", "(blank key)", "(no match)", "Grand total"]);
}

#[test]
fn m1_a_broken_link_refuses_its_fields_with_the_reason_instead_of_numbers_and_a_link_that_cant_be_checked_says_why() {
    let f = fx("p9m1_broken");
    let region = field(&f, "Region");
    let broken = LinkStatus::Broken(vec!["3 duplicate keys".to_string()]);
    let why = broken.refusal("Branch master").expect("a broken link refuses");
    assert_eq!(why, "'Branch master' has 3 duplicate keys. Open Model \u{25b8} Key check to see them.");
    let e = with_scope(scope_of(&f, Some(why.clone())), || build_pivot(lazy(&f), &pivot_of(&[&region]))).err().expect("refused");
    assert_eq!(format!("{e:#}"), why, "the pivot shows the reason");
    let e = with_scope(scope_of(&f, Some(why.clone())), || compile_tab_lazy(&f.data.source, &[keep(vec![picked(&region, &["Dhaka"], false)])], &f.d).and_then(|lf| Ok(lf.collect()?))).err().expect("refused");
    assert!(format!("{e:#}").contains("3 duplicate keys"), "a filter says it too: {e:#}");
    let e = with_scope(scope_of(&f, Some(why.clone())), || micro_parquet::charts::calc::compute(lazy(&f), &ChartSpec { category: region.name.clone(), category_model: Some(region.clone()), value: Some("Balance".into()), ..Default::default() })).err().expect("refused");
    assert_eq!(format!("{e:#}"), why, "and a chart");
    // a duplicate key that appeared since the check: the join itself refuses, so no row is counted twice
    let dup = DataFrame::new(vec![strs("Code", &["B1", "B1"]), strs("Branch name", &["a", "b"]), strs("Region", &["Dhaka", "Khulna"]), nums("Limit", &[1.0, 2.0])]).expect("dup");
    let g = Fx { d: f.d.clone(), data: f.data.clone(), lookup: tab("t2", "Branch master", parquet(&f.d, "dup", &dup)), link: f.link.clone() };
    let e = with_scope(scope_of(&g, None), || build_pivot(lazy(&g), &pivot_of(&[&region]))).err();
    assert!(e.is_some(), "a repeated key is refused, never joined twice");
    // a check that can't run says why in its own words (M0-4, owner decision on S3)
    let closed = LinkStatus::Failed("the tab 'Branch master' is closed".to_string());
    assert_eq!(closed.text(), "Can't check: the tab 'Branch master' is closed");
    assert_eq!(LinkStatus::Failed("'Accounts' has no column 'Branch Code'".to_string()).text(), "Can't check: 'Accounts' has no column 'Branch Code'");
    assert!(closed.is_broken());
    assert_eq!(closed.reason(), "Can't check: the tab 'Branch master' is closed");
    assert_ne!(closed.text(), LinkStatus::Broken(vec!["3 duplicate keys".to_string()]).text(), "apart from duplicate and blank keys");
    assert!(!closed.text().starts_with("Broken"), "it is not the Broken of a checked link");
    assert_eq!(
        closed.refusal("Branch master").as_deref(),
        Some("The link to 'Branch master' can't be used. Can't check: the tab 'Branch master' is closed. Open Model \u{25b8} Key check to see it.")
    );
    assert_eq!(LinkStatus::Ok.refusal("Branch master"), None);
    assert_eq!(LinkStatus::OkUnmatched(3).refusal("Branch master"), None, "unmatched keys do not break a link");
}

#[test]
fn m1_the_check_of_a_link_whose_column_is_gone_names_the_column_and_the_tab() {
    let f = fx("p9m1_gone");
    let gone = Link { keys: vec![KeyPair { data: "Branch Code".into(), lookup: "Code".into() }], ..f.link.clone() };
    let e = check_link(&gone, &f.data, &f.lookup, &f.d, &|| Ok(())).err().expect("refused");
    assert_eq!(LinkStatus::Failed(format!("{e:#}")).text(), "Can't check: 'Accounts' has no column 'Branch Code'");
    let gone = Link { keys: vec![KeyPair { data: "Branch".into(), lookup: "Gone".into() }], ..f.link.clone() };
    let e = check_link(&gone, &f.data, &f.lookup, &f.d, &|| Ok(())).err().expect("refused");
    assert_eq!(LinkStatus::Failed(format!("{e:#}")).text(), "Can't check: 'Branch master' has no column 'Gone'");
}

#[test]
fn m1_outside_a_project_a_step_with_a_lookup_filter_gives_the_conventions_message_and_the_python_export_gives_its_own() {
    let f = fx("p9m1_outside");
    let region = field(&f, "Region");
    let step = keep(vec![picked(&region, &["Dhaka"], false)]);
    let e = compile_tab_lazy(&f.data.source, &[step.clone()], &f.d).and_then(|lf| Ok(lf.collect()?)).err().expect("refused outside a project");
    assert_eq!(format!("{e:#}"), "This step uses the model of a project (link 'Accounts \u{2192} Branch master'). Open the project to run it.");
    let pivot = TransformStep::from_pivot_spec(pivot_of(&[&region]));
    let e = compile_tab_lazy(&f.data.source, &[pivot.clone()], &f.d).and_then(|lf| Ok(lf.collect()?)).err().expect("a pivot too");
    assert_eq!(format!("{e:#}"), "This step uses the model of a project (link 'Accounts \u{2192} Branch master'). Open the project to run it.");
    // a project whose model lacks the link says so
    let none = Some(Arc::new(ModelContext { links: vec![], temp_dir: f.d.clone(), ..Default::default() }));
    let e = with_scope(none, || compile_tab_lazy(&f.data.source, &[step.clone()], &f.d).and_then(|lf| Ok(lf.collect()?))).err().expect("no such link");
    assert!(format!("{e:#}").contains("is not in this project's model"), "{e:#}");
    // the scope is left as it was
    assert!(scope().is_none());

    // the Python export
    let TabSource::SingleFile { path, is_parquet, config } = &f.data.source else { panic!("a file") };
    let e = micro_parquet::macros::generate_python_polars_code(path, *is_parquet, config, &[step], None).err().expect("refused");
    assert_eq!(format!("{e:#}"), "Python export can't follow links between tabs yet. Use the Join tool to copy the column, or export the result tab.");
    let e = micro_parquet::macros::generate_python_polars_code(path, *is_parquet, config, &[pivot], None).err().expect("refused");
    assert_eq!(format!("{e:#}"), PYTHON_REFUSAL);
    // everything else exports as before
    let plain = micro_parquet::macros::generate_python_polars_code(path, *is_parquet, config, &[keep(vec![FilterCondition { column: "Wing".into(), op: CondOp::Equals, value: "Islami".into(), ..Default::default() }])], None).expect("a plain filter exports");
    assert!(plain.contains("Islami"));
}

#[test]
fn m1_old_pivots_charts_and_filters_load_unchanged_and_write_no_new_fields() {
    // a pivot, a chart and a filter condition written before M1
    let old_pivot = r#"{"rows":["Bank"],"columns":[],"values":[{"field":"Balance","agg":"Sum","weight_col":null}],"filters":[],"row_totals":true,"column_totals":true,"subtotals":false,"show_as":"NoCalculation","scale":"Global","sort_by_value":null,"field_orders":[],"date_fields":[]}"#;
    let spec: PivotSpec = serde_json::from_str(old_pivot).expect("an old pivot loads");
    assert!(spec.model_fields.is_empty());
    let text = serde_json::to_string(&spec).expect("writes");
    assert!(!text.contains("model_fields"), "nothing new is written: {text}");
    let step = TransformStep::from_pivot_spec(spec);
    assert!(!serde_json::to_string(&step).expect("writes").contains("model_fields"));
    let old_step: TransformStep = serde_json::from_str(r#"{"PivotTable":{"rows":["Bank"],"columns":[],"values":[{"field":"Balance","agg":"Sum","weight_col":null}]}}"#).expect("an old step loads");
    assert!(old_step.as_pivot_spec().expect("a pivot").model_fields.is_empty());
    let cond: FilterCondition = serde_json::from_str(r#"{"column":"Bank","op":"Equals","value":"X"}"#).expect("an old condition loads");
    assert!(cond.lookup.is_none());
    assert!(!serde_json::to_string(&cond).expect("writes").contains("lookup"));
    let chart = ChartSpec::default();
    let text = serde_json::to_string(&chart).expect("writes");
    assert!(!text.contains("category_model") && !text.contains("series_model") && !text.contains("facet_model"), "a saved chart keeps its bytes");
    let back: ChartSpec = serde_json::from_str(&text).expect("loads");
    assert!(back.category_model.is_none() && back.model_fields().is_empty());
    let level = DrillLevel::column("Branch");
    assert!(!serde_json::to_string(&level).expect("writes").contains("model"));
    // and with the new fields set they round-trip
    let region = ModelFieldRef { name: "Branch master \u{b7} Region".into(), data_tab: "t1".into(), lookup_tab: "t2".into(), link_name: "Accounts \u{2192} Branch master".into(), column: "Region".into() };
    let c = FilterCondition { column: region.name.clone(), lookup: Some(region.clone()), op: CondOp::InList, values: vec!["Dhaka".into()], ..Default::default() };
    let back: FilterCondition = serde_json::from_str(&serde_json::to_string(&c).expect("writes")).expect("loads");
    assert_eq!(back, c);
    assert_eq!(c.describe(), "Branch master \u{203a} Region is in list Dhaka");
    assert_eq!(c.subject(), "Branch master \u{203a} Region");
}

#[test]
fn m1_the_scope_is_taken_by_a_job_when_it_is_submitted_and_left_alone_after() {
    use micro_parquet::jobs::{JobKey, JobKind, JobQueue};
    let f = fx("p9m1_jobs");
    let q: JobQueue<String> = JobQueue::new(|| {});
    set_scope(scope_of(&f, None));
    let (d, step) = (f.d.clone(), keep(vec![picked(&field(&f, "Region"), &["Dhaka"], false)]));
    let src = f.data.source.clone();
    q.submit(JobKey::unique(JobKind::ModelCheck), "a job", false, move |_| {
        let out = compile_tab_lazy(&src, &[step], &d)?.collect()?;
        Ok(format!("{}", out.height()))
    });
    set_scope(None);
    assert!(q.wait_idle(std::time::Duration::from_secs(60)));
    let r = q.poll();
    assert_eq!(r.len(), 1);
    match &r[0].outcome {
        micro_parquet::jobs::Outcome::Done(v) => assert_eq!(v, "4", "the worker read the model the project had at submit"),
        other => panic!("the job failed: {:?}", match other { micro_parquet::jobs::Outcome::Failed(m) => m.clone(), _ => "cancelled".to_string() }),
    }
    // a job submitted outside a project is refused the same way
    let (d, step, src) = (f.d.clone(), keep(vec![picked(&field(&f, "Region"), &["Dhaka"], false)]), f.data.source.clone());
    q.submit(JobKey::unique(JobKind::ModelCheck), "a job", false, move |_| Ok(format!("{}", compile_tab_lazy(&src, &[step], &d)?.collect()?.height())));
    assert!(q.wait_idle(std::time::Duration::from_secs(60)));
    match &q.poll()[0].outcome {
        micro_parquet::jobs::Outcome::Failed(m) => assert!(m.contains("Open the project to run it"), "{m}"),
        _ => panic!("expected a refusal outside a project"),
    }
}


// ------------------------------------------------------------------------------------------------ follow-ups of S4 (S5)

fn dates(name: &str, v: &[Option<&str>]) -> Series {
    let days: Vec<Option<i32>> = v.iter().map(|d| d.map(|d| (chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").expect("date") - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch")).num_days() as i32)).collect();
    Series::new(name.into(), days).cast(&DataType::Date).expect("dates")
}

/// A fixture with number and date lookup columns that have a blank cell on a matched row. Eight accounts: B2 (A3 200, A7 10) has a
/// lookup row with a blank Limit and Opened; B9 (A6 50, A8 7) has no lookup row; B5 (A9 3) has a Region that reads "(no match)".
fn typed_fx(name: &str) -> Fx {
    let d = dir(name);
    let data = DataFrame::new(vec![
        strs("Branch", &["B1", "B1", "B2", "B3", "B4", "B9", "B2", "B9", "B5"]),
        nums("Balance", &[100.0, 300.0, 200.0, 400.0, 800.0, 50.0, 10.0, 7.0, 3.0]),
        strs("AC Number", &["A1", "A2", "A3", "A4", "A5", "A6", "A7", "A8", "A9"]),
    ])
    .expect("data");
    let lookup = DataFrame::new(vec![
        strs("Code", &["B1", "B2", "B3", "B4", "B5"]),
        strs("Region", &["Dhaka", "Dhaka", "Khulna", "Chattogram", "(no match)"]),
        Series::new("Limit".into(), [Some(1000.0), None, Some(3000.0), Some(4000.0), Some(5000.0)]),
        dates("Opened", &[Some("2020-01-31"), None, Some("2021-06-30"), Some("2022-12-31"), Some("2023-03-31")]),
    ])
    .expect("lookup");
    let link = Link { data_tab: "t1".into(), lookup_tab: "t2".into(), keys: vec![KeyPair { data: "Branch".into(), lookup: "Code".into() }], unmatched_value: None };
    Fx { d: d.clone(), data: tab("t1", "Accounts", parquet(&d, "accounts_t", &data)), lookup: tab("t2", "Branch master", parquet(&d, "master_t", &lookup)), link }
}

fn run_filter(f: &Fx, c: FilterCondition) -> Vec<String> {
    let out = in_scope(f, || compile_tab_lazy(&f.data.source, &[keep(vec![c])], &f.d).and_then(|lf| Ok(lf.collect()?))).expect("filter");
    ids(&out)
}

#[test]
fn m1_no_match_and_blank_are_two_separate_filters_on_a_number_and_a_date_lookup_column() {
    let f = typed_fx("p9m1_typed_filter");
    for column in ["Limit", "Opened"] {
        let r = field(&f, column);
        let unmatched = run_filter(&f, picked(&r, &["(no match)"], false));
        let blank = run_filter(&f, picked(&r, &[], true));
        assert_eq!(unmatched, ["A6", "A8"], "{column}: (no match) keeps exactly the rows whose key has no lookup row");
        assert_eq!(blank, ["A3", "A7"], "{column}: (blank) keeps exactly the matched rows whose cell is blank, never an unmatched row");
        assert!(unmatched.iter().all(|u| !blank.contains(u)), "{column}: the two sets don't overlap");
        let both = run_filter(&f, picked(&r, &[], true).tap_no_match());
        assert_eq!(both, ["A3", "A6", "A7", "A8"], "{column}: both together are the union");
        // everything else: neither blank nor unmatched
        let rest = run_filter(&f, FilterCondition { op: CondOp::NotInList, ..picked(&r, &[], true).tap_no_match() });
        assert_eq!(rest, ["A1", "A2", "A4", "A5", "A9"], "{column}: not in (blank, no match)");
        // "does not equal" a value keeps an unmatched row unless (no match) is picked too
        let not_a_value = run_filter(&f, FilterCondition { op: CondOp::NotInList, ..picked(&r, &[if column == "Limit" { "1000" } else { "2020-01-31" }], false) });
        assert!(not_a_value.contains(&"A6".to_string()) && !not_a_value.contains(&"A1".to_string()), "{column}: an unmatched row is not that value");
    }
}

trait TapNoMatch {
    fn tap_no_match(self) -> Self;
}
impl TapNoMatch for FilterCondition {
    fn tap_no_match(mut self) -> Self {
        self.no_match = true;
        self
    }
}

#[test]
fn m1_no_match_is_a_flag_of_its_own_so_a_real_value_called_no_match_never_collides_with_it() {
    let f = typed_fx("p9m1_flag");
    let r = field(&f, "Region");
    // B5's Region is literally "(no match)": the value finds A9, the flag finds the unmatched rows
    let mut by_value = picked(&r, &[], false);
    by_value.values = vec!["(no match)".to_string()];
    assert_eq!(run_filter(&f, by_value.clone()), ["A9"], "the value");
    assert_eq!(run_filter(&f, picked(&r, &["(no match)"], false)), ["A6", "A8"], "the flag");
    assert!(by_value.no_match.eq(&false));
    // the flag is a field of the condition, saved only when on
    let on = picked(&r, &["(no match)"], false);
    assert!(on.no_match && on.values.is_empty(), "the picker's '(no match)' became the flag, not a value");
    let json = serde_json::to_string(&on).expect("json");
    assert!(json.contains("\"no_match\":true"), "{json}");
    assert!(!serde_json::to_string(&by_value).expect("json").contains("no_match"), "an old condition is written as it was");
    let back: FilterCondition = serde_json::from_str(&json).expect("back");
    assert_eq!(back, on);
    // an old file without the flag loads with it off
    let old: FilterCondition = serde_json::from_str(r#"{"column":"Region","op":"InList","values":["Dhaka"]}"#).expect("old");
    assert!(!old.no_match);
}

#[test]
fn m1_a_pivot_by_a_number_or_a_date_lookup_column_shows_blank_and_no_match_as_two_rows_blank_first_and_the_total_is_the_ungrouped_total() {
    let f = typed_fx("p9m1_typed_pivot");
    for column in ["Limit", "Opened"] {
        let r = field(&f, column);
        let ours = run_pivot(&f, &pivot_of(&[&r]));
        let labels: Vec<&str> = ours.iter().map(|p| p.0.as_str()).collect();
        let n = labels.len();
        assert_eq!(&labels[n - 3..], ["(blank)", "(no match)", "Grand total"], "{column}: two rows, (blank) before (no match): {labels:?}");
        let at = |l: &str| ours.iter().find(|p| p.0 == l).expect(l).1;
        assert_eq!(at("(blank)"), 210.0, "{column}: the matched row B2 with a blank cell (200 + 10)");
        assert_eq!(at("(no match)"), 57.0, "{column}: the unmatched key B9 (50 + 7)");
        let total: f64 = ours.iter().filter(|p| p.0 != "Grand total").map(|p| p.1).sum();
        assert_eq!(at("Grand total"), 1870.0, "{column}");
        assert_eq!(total, 1870.0, "{column}: the rows add up to the total");
        assert_eq!(at("Grand total"), sum_of(&typed_accounts_df(&f), "Balance"), "{column}: equals the ungrouped total");
    }
}

fn typed_accounts_df(f: &Fx) -> DataFrame {
    lazy(f).collect().expect("accounts")
}

#[test]
fn m1_show_rows_of_the_no_match_and_blank_rows_of_a_number_pivot_give_the_cells_rows() {
    let f = typed_fx("p9m1_typed_show");
    let r = field(&f, "Limit");
    let spec = pivot_of(&[&r]);
    let b = in_scope(&f, || build_pivot(lazy(&f), &spec)).expect("pivot");
    let names: Vec<String> = b.df.get_column_names().iter().map(|n| n.to_string()).collect();
    let labels: Vec<String> = pairs(&b.df).into_iter().map(|p| p.0).collect();
    for (label, rows, sum) in [("(no match)", vec!["A6", "A8"], 57.0), ("(blank)", vec!["A3", "A7"], 210.0)] {
        let row = labels.iter().position(|l| l == label).expect(label);
        let c = cell_rows(&spec, &b, row, &names[1]).expect("show rows");
        assert!(c.conditions.iter().all(|c| c.values.iter().all(|v| v != "(no match)")), "the flag, never a value");
        let out = in_scope(&f, || filter_by_conditions(lazy(&f), &c.conditions).and_then(|lf| Ok(lf.collect()?))).expect("rows");
        assert_eq!(ids(&out), rows, "{label}");
        assert_eq!(sum_of(&out, "Balance"), sum, "{label}: the control total equals the cell");
    }
}

#[test]
fn m1_repeated_keys_at_the_time_of_a_join_are_refused_in_the_key_checks_words_and_the_link_is_reported_broken() {
    let f = typed_fx("p9m1_dup_at_join");
    let r = field(&f, "Region");
    let spec = pivot_of(&[&r]);
    assert!(in_scope(&f, || build_pivot(lazy(&f), &spec)).is_ok(), "fine before the file changes");
    let _ = take_broken_at_join();
    // the lookup file changes after the check: B1 now appears twice and B2 twice
    let d2 = DataFrame::new(vec![
        strs("Code", &["B1", "B1", "B2", "B2", "B3"]),
        strs("Region", &["Dhaka", "Dhaka", "Dhaka", "Barishal", "Khulna"]),
        Series::new("Limit".into(), [Some(1.0), Some(1.0), Some(2.0), Some(3.0), Some(4.0)]),
        dates("Opened", &[Some("2020-01-31"), Some("2020-01-31"), None, None, None]),
    ])
    .expect("changed");
    let path = match &f.lookup.source {
        TabSource::SingleFile { path, .. } => path.clone(),
        _ => panic!("a file"),
    };
    let mut d2 = d2;
    ParquetWriter::new(std::fs::File::create(&path).expect("file")).finish(&mut d2).expect("write");
    let e = in_scope(&f, || build_pivot(lazy(&f), &spec)).err().expect("refused");
    assert_eq!(format!("{:#}", e), "'Branch master' has 2 duplicate keys. Open Model \u{25b8} Key check to see them.");
    let broken = take_broken_at_join();
    assert_eq!(broken.len(), 1);
    assert_eq!((broken[0].0.as_str(), broken[0].1.as_str()), ("t1", "t2"));
    assert_eq!(broken[0].2, LinkStatus::Broken(vec!["2 duplicate keys".to_string()]));
    // a filter takes the same way
    let e = in_scope(&f, || compile_tab_lazy(&f.data.source, &[keep(vec![picked(&r, &["Dhaka"], false)])], &f.d).and_then(|lf| Ok(lf.collect()?))).err().expect("refused");
    assert!(format!("{:#}", e).starts_with("'Branch master' has 2 duplicate keys."), "{e:#}");
}


// ------------------------------------------------------------------------------------------------ S6: three labels (M1-12 replaced)

/// Accounts with every kind of row a lookup field can meet: B2 has a lookup row whose Region, Limit and Opened are blank (A3 200,
/// A7 10), B9 has no lookup row (A6 50, A8 7), and two keys are blank (A10 25 is missing, A11 5 is only spaces). All 1,900.
fn three_fx(name: &str) -> Fx {
    let d = dir(name);
    let data = DataFrame::new(vec![
        opt_strs("Branch", &[Some("B1"), Some("B1"), Some("B2"), Some("B3"), Some("B4"), Some("B9"), Some("B2"), Some("B9"), Some("B5"), None, Some("  ")]),
        nums("Balance", &[100.0, 300.0, 200.0, 400.0, 800.0, 50.0, 10.0, 7.0, 3.0, 25.0, 5.0]),
        strs("AC Number", &["A1", "A2", "A3", "A4", "A5", "A6", "A7", "A8", "A9", "A10", "A11"]),
    ])
    .expect("data");
    let lookup = DataFrame::new(vec![
        strs("Code", &["B1", "B2", "B3", "B4", "B5"]),
        opt_strs("Region", &[Some("Dhaka"), None, Some("Khulna"), Some("Chattogram"), Some("Sylhet")]),
        Series::new("Limit".into(), [Some(1000.0), None, Some(3000.0), Some(4000.0), Some(5000.0)]),
        dates("Opened", &[Some("2020-01-31"), None, Some("2021-06-30"), Some("2022-12-31"), Some("2023-03-31")]),
    ])
    .expect("lookup");
    let link = Link { data_tab: "t1".into(), lookup_tab: "t2".into(), keys: vec![KeyPair { data: "Branch".into(), lookup: "Code".into() }], unmatched_value: None };
    Fx { d: d.clone(), data: tab("t1", "Accounts", parquet(&d, "accounts_3", &data)), lookup: tab("t2", "Branch master", parquet(&d, "master_3", &lookup)), link }
}

fn flag_blank_key(mut c: FilterCondition) -> FilterCondition {
    c.blank_key = true;
    c
}

#[test]
fn m1_a_pivot_by_a_text_a_number_and_a_date_lookup_column_has_three_separate_rows_blank_blank_key_no_match_in_that_order_and_the_total_is_the_ungrouped_total() {
    let f = three_fx("p9m1_three_pivot");
    for column in ["Region", "Limit", "Opened"] {
        let r = field(&f, column);
        let spec = PivotSpec {
            rows: vec![r.name.clone()],
            values: vec![sum_balance(), PivotValue { field: "AC Number".into(), agg: PivotAgg::Count, weight_col: None, time: None }],
            row_totals: true,
            column_totals: true,
            model_fields: vec![r.clone()],
            ..Default::default()
        };
        let b = in_scope(&f, || build_pivot(lazy(&f), &spec)).expect("pivot");
        let names: Vec<String> = b.df.get_column_names().iter().map(|n| n.to_string()).collect();
        let labels: Vec<String> = strings(&b.df, &names[0]).into_iter().map(|l| l.unwrap_or_default()).collect();
        let n = labels.len();
        assert_eq!(labels[n - 4..], ["(blank)", "(blank key)", "(no match)", "Grand total"], "{column}: three rows, in that order, last: {labels:?}");
        let sums = floats(&b.df, &names[1]);
        let counts = floats(&b.df, &names[2]);
        let at = |l: &str| labels.iter().position(|x| x == l).expect(l);
        assert_eq!((sums[at("(blank)")], counts[at("(blank)")]), (Some(210.0), Some(2.0)), "{column}: the matched row B2 with a blank cell (200 + 10)");
        assert_eq!((sums[at("(blank key)")], counts[at("(blank key)")]), (Some(30.0), Some(2.0)), "{column}: the missing and the only-spaces keys (25 + 5)");
        assert_eq!((sums[at("(no match)")], counts[at("(no match)")]), (Some(57.0), Some(2.0)), "{column}: the unmatched key B9 (50 + 7)");
        let total = sum_of(&lazy(&f).collect().expect("accounts"), "Balance");
        assert_eq!(sums[at("Grand total")], Some(total), "{column}: the total equals the ungrouped total");
        assert_eq!(sums.iter().take(n - 1).flatten().sum::<f64>(), total, "{column}: the rows add up to it");
        assert_eq!(counts[at("Grand total")], Some(11.0), "{column}: and so does the count");
    }
}

#[test]
fn m1_a_filter_on_blank_blank_key_and_no_match_keeps_exactly_each_ones_rows_the_three_sets_never_overlap_and_the_flag_is_its_own_field() {
    let f = three_fx("p9m1_three_filter");
    for column in ["Region", "Limit", "Opened"] {
        let r = field(&f, column);
        let blank = run_filter(&f, picked(&r, &[], true));
        let blank_key = run_filter(&f, picked(&r, &["(blank key)"], false));
        let no_match = run_filter(&f, picked(&r, &["(no match)"], false));
        assert_eq!(blank, ["A3", "A7"], "{column}: (blank) is the matched row with a blank cell");
        assert_eq!(blank_key, ["A10", "A11"], "{column}: (blank key) is the missing and the only-spaces keys");
        assert_eq!(no_match, ["A6", "A8"], "{column}: (no match) is the key with no lookup row");
        for (a, b) in [(&blank, &blank_key), (&blank, &no_match), (&blank_key, &no_match)] {
            assert!(a.iter().all(|x| !b.contains(x)), "{column}: the sets don't overlap");
        }
        let all_three = run_filter(&f, flag_blank_key(picked(&r, &["(no match)"], true)));
        assert_eq!(all_three, ["A3", "A6", "A7", "A8", "A10", "A11"], "{column}: all three together");
        let rest = run_filter(&f, FilterCondition { op: CondOp::NotInList, ..flag_blank_key(picked(&r, &["(no match)"], true)) });
        assert_eq!(rest, ["A1", "A2", "A4", "A5", "A9"], "{column}: not in the three keeps the rest");
        // leaving one out keeps the others, the matched rows too
        let not_blank_key = run_filter(&f, FilterCondition { op: CondOp::NotInList, ..picked(&r, &["(blank key)"], false) });
        assert_eq!(not_blank_key, ["A1", "A2", "A3", "A4", "A5", "A6", "A7", "A8", "A9"], "{column}: not in (blank key)");
    }
    // the flag is a field of its own, written only when on, and an old condition loads with it off
    let r = field(&f, "Region");
    let on = picked(&r, &["(blank key)"], false);
    assert!(on.blank_key && !on.no_match && !on.or_blank && on.values.is_empty(), "the picker's '(blank key)' became the flag, not a value");
    let json = serde_json::to_string(&on).expect("json");
    assert!(json.contains("\"blank_key\":true") && !json.contains("no_match"), "{json}");
    assert!(!serde_json::to_string(&picked(&r, &["Dhaka"], false)).expect("json").contains("blank_key"), "an old condition is written as it was");
    let back: FilterCondition = serde_json::from_str(&json).expect("back");
    assert_eq!(back, on);
    let old: FilterCondition = serde_json::from_str(r#"{"column":"Region","op":"InList","values":["Dhaka"],"no_match":true}"#).expect("old");
    assert!(!old.blank_key && old.no_match);
    assert_eq!(on.describe(), "Branch master \u{203a} Region is in list (blank key)");
}

#[test]
fn m1_show_rows_from_each_of_the_three_rows_gives_exactly_that_cells_rows_and_a_chart_shows_the_same_three() {
    let f = three_fx("p9m1_three_show");
    for column in ["Region", "Limit"] {
        let r = field(&f, column);
        let spec = pivot_of(&[&r]);
        let b = in_scope(&f, || build_pivot(lazy(&f), &spec)).expect("pivot");
        let names: Vec<String> = b.df.get_column_names().iter().map(|n| n.to_string()).collect();
        let labels: Vec<String> = pairs(&b.df).into_iter().map(|p| p.0).collect();
        for (label, rows, sum) in [("(blank)", vec!["A3", "A7"], 210.0), ("(blank key)", vec!["A10", "A11"], 30.0), ("(no match)", vec!["A6", "A8"], 57.0)] {
            let row = labels.iter().position(|l| l == label).expect(label);
            let c = cell_rows(&spec, &b, row, &names[1]).expect("show rows");
            assert!(c.conditions.iter().all(|c| c.values.iter().all(|v| v != "(no match)" && v != "(blank key)")), "{label}: the flags, never values");
            let out = in_scope(&f, || filter_by_conditions(lazy(&f), &c.conditions).and_then(|lf| Ok(lf.collect()?))).expect("rows");
            assert_eq!(ids(&out), rows, "{column} {label}: the rows tab has the cell's rows");
            assert_eq!(sum_of(&out, "Balance"), sum, "{column} {label}: its control total equals the cell");
        }
    }
    // the chart's categories are the pivot's labels, in the same order, with the same numbers, and a mark's rows are the cell's
    let r = field(&f, "Region");
    let spec = ChartSpec { category: r.name.clone(), category_model: Some(r.clone()), value: Some("Balance".into()), agg: micro_parquet::charts::spec::ChartAgg::Sum, ..Default::default() };
    let d = in_scope(&f, || micro_parquet::charts::calc::compute(lazy(&f), &spec)).expect("the chart");
    assert_eq!(d.categories, ["Chattogram", "Dhaka"].iter().chain(["Khulna", "Sylhet"].iter()).chain(["(blank)", "(blank key)", "(no match)"].iter()).map(|s| s.to_string()).collect::<Vec<_>>());
    let pivot = run_pivot(&f, &pivot_of(&[&r]));
    for (i, c) in d.categories.iter().enumerate() {
        assert_eq!(d.values[0][i], pivot.iter().find(|p| &p.0 == c).map(|p| p.1), "{c}: the chart shows the pivot's number");
    }
    let level = DrillLevel::of_model(&r);
    for (label, rows) in [("(blank)", vec!["A3", "A7"]), ("(blank key)", vec!["A10", "A11"]), ("(no match)", vec!["A6", "A8"])] {
        let c = level.condition(label, true);
        let out = in_scope(&f, || filter_by_conditions(lazy(&f), &[c]).and_then(|lf| Ok(lf.collect()?))).expect("rows");
        assert_eq!(ids(&out), rows, "a mark of {label}");
    }
}
