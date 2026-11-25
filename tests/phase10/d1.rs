//! D1: the numbers of a dashboard item and its page filters (the window is tested in `src/ui/render_tests_p10.rs`).

use crate::common::{dir, parquet};
use crate::p10_p9_baseline as base;
use crate::p9_m3::{dates, nums, strs};
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec};
use micro_parquet::dashboard::*;
use micro_parquet::model::*;
use micro_parquet::models::*;
use micro_parquet::project::chart_tab_source;
use polars::prelude::*;

fn accounts_source(d: &std::path::Path) -> TabSource {
    let p = crate::common::fixtures_dir().join("phase9_replay/data/Accounts.csv");
    TabSource::SingleFile { path: p, is_parquet: false, config: micro_parquet::engine::auto_detect_smart_schema(&p_of(d)).expect("a schema") }
}

fn p_of(_d: &std::path::Path) -> std::path::PathBuf {
    crate::common::fixtures_dir().join("phase9_replay/data/Accounts.csv")
}

fn digest(d: &micro_parquet::chart::ChartData) -> String {
    format!("{:?}|{:?}|{:?}|{:?}|{}|{:?}", d.categories, d.series, d.values, d.rows, d.total_rows, d.notes)
}

fn request(source: ItemSource, picks: Vec<(PageFilter, Vec<String>)>, d: &std::path::Path) -> ItemRequest {
    ItemRequest { item_id: "i1".into(), source, picks, drill: vec![], temp_dir: d.to_path_buf() }
}

#[test]
fn d1_a_dashboard_item_has_the_numbers_of_the_chart_tab_and_the_pivot_tab_on_every_fixture() {
    let dd = dir("p10d1_same");
    let src = accounts_source(&dd);
    // the charts: the chart tab's own calculation (`chart_tab_source`) against the item's
    let by_product = ChartSpec { category: "Product code".into(), series: Some("Wing".into()), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 500, ..Default::default() };
    for spec in [base::wing_chart(), by_product] {
        let (want, _) = chart_tab_source(&src, &[], &spec, &dd).expect("the chart tab's numbers");
        let got = match compute_item(&request(ItemSource::Chart { base: src.clone(), steps: vec![], spec: spec.clone() }, vec![], &dd)).expect("the item") {
            ItemData::Chart(c) => c,
            other => panic!("a chart: {other:?}"),
        };
        assert_eq!(digest(&got), digest(&want), "{}", spec.category);
        assert!(!got.categories.is_empty());
    }
    // the pivots: the plain one, the Nested one (the pivot tab's own table, and the Nested view)
    for spec in [PivotSpec { rows: vec!["Wing".into()], values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None }], row_totals: true, column_totals: true, ..Default::default() }, base::nested_pivot()] {
        let lf = micro_parquet::engine::compile_tab_lazy(&src, &[], &dd).expect("the table");
        let want = micro_parquet::pivot_table::apply_pivot_table(lf.clone(), &spec).expect("the pivot").collect().expect("rows");
        let got = match compute_item(&request(ItemSource::Pivot { base: src.clone(), steps: vec![], spec: spec.clone() }, vec![], &dd)).expect("the item") {
            ItemData::Table(t) => t,
            other => panic!("a table: {other:?}"),
        };
        assert!(got.df.equals_missing(&want), "the pivot's own table");
        assert_eq!(got.nested.is_some(), spec.layout.is_nested());
        if let Some(n) = &got.nested {
            let own = micro_parquet::pivot_view::build_nested(lf, &spec).expect("nested");
            assert_eq!(format!("{:?}", n.rows.iter().map(|r| (r.level, r.label.clone())).collect::<Vec<_>>()), format!("{:?}", own.rows.iter().map(|r| (r.level, r.label.clone())).collect::<Vec<_>>()));
            assert!(n.values.equals_missing(&own.values));
        }
    }
}

fn gl_field() -> ModelFieldRef {
    let l = Link { data_tab: "t1".into(), lookup_tab: "t3".into(), keys: vec![KeyPair { data: "GL".into(), lookup: "GL".into() }], unmatched_value: None };
    ModelFieldRef::new(&l, "Accounts \u{2192} GL map", "GL map", "Head")
}

#[test]
fn d1_a_filter_on_a_lookup_field_filters_a_linked_item_and_an_unlinked_item_is_told_it_does_not_apply() {
    let dd = dir("p10d1_lookup");
    let df = DataFrame::new(vec![
        dates("Date", &["2026-01-31", "2026-01-31", "2026-01-31", "2026-02-28"]).into(),
        strs("GL", &["INC", "EXP", "EA", "INC"]).into(),
        nums("Balance", &[10.0, 4.0, 100.0, 12.0]),
    ])
    .expect("data");
    let src = parquet(&dd, "accounts", &df);
    let sc = crate::p9_m5::scope5(&dd, "2026-02-28", Some(("GL map", "GL", crate::p9_m5::gl_map())), vec![]);
    let spec = ChartSpec { category: "GL".into(), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 500, ..Default::default() };
    let f = PageFilter { id: "f1".into(), label: "Head".into(), tab: "t1".into(), field: gl_field().name, model: Some(gl_field()), multi: false };
    let run = |picks: Vec<(PageFilter, Vec<String>)>| match with_scope(sc.clone(), || compute_item(&request(ItemSource::Chart { base: src.clone(), steps: vec![], spec: spec.clone() }, picks, &dd))).expect("the item") {
        ItemData::Chart(c) => c,
        other => panic!("a chart: {other:?}"),
    };
    let all = run(vec![]);
    assert_eq!(all.categories.len(), 3, "three GL codes without the filter: {:?}", all.categories);
    // hand-worked: 'Interest income' is the GL INC: 10 + 12 = 22; the other two codes are gone
    let one = run(vec![(f.clone(), vec!["Interest income".into()])]);
    assert_eq!(one.categories, vec!["INC".to_string()]);
    assert_eq!(one.values[0][0], Some(22.0));
    // the filter's values: the lookup's heads that the data has, as text, in order
    let (values, truncated) = with_scope(sc.clone(), || field_values(&src, &[], &f, &dd)).expect("values");
    assert_eq!((values, truncated), (vec!["Earning assets".to_string(), "Interest expense".to_string(), "Interest income".to_string()], false));
    // which items the filter reaches: the ones that read its data tab
    assert!(filter_applies(&f, Some("t1")));
    assert!(!filter_applies(&f, Some("t7")), "an unlinked item: 'filter doesn't apply'");
    assert!(!filter_applies(&f, None));
}

#[test]
fn d1_the_fifth_filter_is_refused_and_removing_a_filter_moves_the_picks_up() {
    let mut p = DashPage::default();
    for i in 0..4 {
        p.add_filter(PageFilter { id: format!("f{i}"), label: format!("F{i}"), ..Default::default() }).expect("the first four");
        p.state.filter_picks[i] = Some(vec![format!("v{i}")]);
    }
    let e = p.add_filter(PageFilter { id: "f4".into(), ..Default::default() }).expect_err("the fifth").to_string();
    assert_eq!(e, "A page can have 4 filters. Remove one first.");
    assert_eq!(p.filters.len(), 4);
    p.remove_filter(1);
    assert_eq!(p.filters.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), ["f0", "f2", "f3"]);
    assert_eq!(p.state.filter_picks.iter().map(|x| x.as_ref().map(|v| v[0].clone())).collect::<Vec<_>>(), [Some("v0".to_string()), Some("v2".to_string()), Some("v3".to_string()), None]);
    // "All" (no pick) and an empty pick are not active picks
    p.state.filter_picks[0] = Some(vec![]);
    assert_eq!(p.active_picks().iter().map(|(i, _, _)| *i).collect::<Vec<_>>(), vec![1, 2]);
}

#[test]
fn d1_a_pick_for_a_plain_column_is_added_to_the_chart_and_replaces_the_pivots_own_filter_on_that_field() {
    let f = PageFilter { id: "f".into(), label: "Wing".into(), tab: "t1".into(), field: "Wing".into(), ..Default::default() };
    let picks = vec![(f, vec!["Islami".to_string()])];
    let chart = ChartSpec { filters: vec![FilterCondition { column: "Branch".into(), op: CondOp::InList, values: vec!["BR01".into()], ..Default::default() }], ..base::wing_chart() };
    let c = chart_spec_with(&chart, &picks);
    assert_eq!(c.filters.len(), 2, "the chart's own filter stays");
    assert_eq!((c.filters[1].column.as_str(), c.filters[1].values.clone()), ("Wing", vec!["Islami".to_string()]));
    let mut pv = base::nested_pivot();
    pv.filters = vec![PivotFilter { field: "Wing".into(), values: vec![Some("Conventional".into())] }];
    let p = pivot_spec_with(&pv, &picks);
    assert_eq!(p.filters.len(), 1);
    assert_eq!(p.filters[0].values, vec![Some("Islami".to_string())], "the page's pick is the reader's latest word");
}
