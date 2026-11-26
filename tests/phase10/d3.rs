//! D3: cross-filter and drill on a page. The hand-worked numbers are in the comments; the window is tested in `src/ui/render_tests_p10.rs`.

use crate::common::{dir, parquet};
use crate::p9_m3::{nums, strs};
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec, DrillLevel};
use micro_parquet::dashboard::*;
use micro_parquet::models::*;
use micro_parquet::project::Project;
use polars::prelude::*;

/// Region · Product · Branch · Balance. By Region (a chart's categories come in alphabetical order): Ctg 100, Dhaka 180, Sylhet 45. By Product: FDR 260, SB 65.
/// Dhaka alone: FDR 150 (Gulshan 100, Motijheel 50), SB 30 (Gulshan 30).
fn book(d: &std::path::Path) -> TabSource {
    let df = DataFrame::new(vec![
        strs("Region", &["Dhaka", "Dhaka", "Dhaka", "Ctg", "Ctg", "Ctg", "Sylhet", "Sylhet"]).into(),
        strs("Product", &["FDR", "FDR", "SB", "FDR", "SB", "SB", "SB", "FDR"]).into(),
        strs("Branch", &["Gulshan", "Motijheel", "Gulshan", "Agrabad", "Agrabad", "Pahartali", "Zindabazar", "Zindabazar"]).into(),
        nums("Balance", &[100.0, 50.0, 30.0, 70.0, 20.0, 10.0, 5.0, 40.0]),
    ])
    .expect("data");
    parquet(d, "d3_book", &df)
}

fn bar(category: &str) -> ChartSpec {
    ChartSpec { category: category.into(), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 500, ..Default::default() }
}

fn run(src: &TabSource, spec: &ChartSpec, picks: Vec<(PageFilter, Vec<String>)>, drill: Vec<String>, d: &std::path::Path) -> micro_parquet::chart::ChartData {
    let req = ItemRequest { item_id: "i".into(), source: ItemSource::Chart { base: src.clone(), steps: vec![], spec: spec.clone() }, picks, drill, temp_dir: d.to_path_buf() };
    match compute_item(&req).expect("the item") {
        ItemData::Chart(c) => *c,
        other => panic!("a chart: {other:?}"),
    }
}

fn shown(c: &micro_parquet::chart::ChartData) -> (Vec<String>, Vec<Option<f64>>) {
    (c.categories.clone(), c.values[0].clone())
}

fn page_with(pick: CrossPick) -> DashPage {
    let mut p = DashPage { id: "p1".into(), title: "P".into(), ..Default::default() };
    p.state.toggle_cross(pick);
    p
}

#[test]
fn d3_clicking_dhaka_gives_the_product_bar_the_numbers_of_dhaka_rows_only_and_the_clicked_bar_keeps_all_its_marks() {
    let dd = dir("p10d3_cross");
    let src = book(&dd);
    let region = bar("Region");
    let all = run(&src, &region, vec![], vec![], &dd);
    assert_eq!(shown(&all), (vec!["Ctg".to_string(), "Dhaka".to_string(), "Sylhet".to_string()], vec![Some(100.0), Some(180.0), Some(45.0)]));
    // the click: Region "Dhaka" on item i1
    let pick = CrossPick::from_chart(&all.spec, "Dhaka", "i1", Some("t1")).expect("a pick");
    assert_eq!((pick.dimension.as_str(), pick.value.as_str(), pick.tab.as_str()), ("Region", "Dhaka", "t1"));
    let page = page_with(pick);
    // the product bar (i2) is filtered: hand-filtered rows are FDR 100 + 50 = 150 and SB 30
    let picks_for_product: Vec<_> = page.cross_for("i2").into_iter().collect();
    assert_eq!(picks_for_product.len(), 1);
    let product = run(&src, &bar("Product"), picks_for_product, vec![], &dd);
    assert_eq!(shown(&product), (vec!["FDR".to_string(), "SB".to_string()], vec![Some(150.0), Some(30.0)]));
    // the hand-filtered table gives the same numbers
    let hand = DataFrame::new(vec![strs("Product", &["FDR", "FDR", "SB"]).into(), nums("Balance", &[100.0, 50.0, 30.0])]).expect("hand");
    let want = micro_parquet::charts::calc::compute(hand.lazy(), &bar("Product")).expect("chart");
    assert_eq!(shown(&product), shown(&want));
    // the clicked item itself is not filtered: all three regions, the same numbers
    assert!(page.cross_for("i1").is_none());
    let own = run(&src, &region, page.cross_for("i1").into_iter().collect(), vec![], &dd);
    assert_eq!(shown(&own), shown(&all));
    // an item on another data tab does not take the click
    assert!(filter_applies(&page.state.cross.as_ref().expect("c").as_filter(), Some("t1")));
    assert!(!filter_applies(&page.state.cross.as_ref().expect("c").as_filter(), Some("t7")), "another data tab: 'filter doesn't apply'");
    assert!(!filter_applies(&page.state.cross.as_ref().expect("c").as_filter(), None));
}

#[test]
fn d3_a_pivot_row_click_filters_the_charts_flat_and_nested_and_the_total_and_a_blank_are_refused() {
    let dd = dir("p10d3_pivot");
    let src = book(&dd);
    let sum = PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None };
    let flat = PivotSpec { rows: vec!["Region".into()], values: vec![sum.clone()], row_totals: true, column_totals: true, ..Default::default() };
    // flat: the row "Ctg": hand-worked product bar of Ctg: FDR 70, SB 20 + 10 = 30
    let pick = CrossPick::from_pivot(&flat, 0, "Ctg", "i3", Some("t1")).expect("a pick");
    let page = page_with(pick);
    let product = run(&src, &bar("Product"), page.cross_for("i1").into_iter().collect(), vec![], &dd);
    assert_eq!(shown(&product), (vec!["FDR".to_string(), "SB".to_string()], vec![Some(70.0), Some(30.0)]));
    // nested: the leaf "SB" at the second level is the Product field: the region bar of SB rows is Dhaka 30, Ctg 30, Sylhet 5
    let nested = PivotSpec { rows: vec!["Region".into(), "Product".into()], values: vec![sum], layout: crate::p10_p9_baseline::NESTED, ..flat.clone() };
    let pick = CrossPick::from_pivot(&nested, 1, "SB", "i3", Some("t1")).expect("a pick");
    assert_eq!(pick.dimension, "Product");
    let page = page_with(pick);
    let region = run(&src, &bar("Region"), page.cross_for("i1").into_iter().collect(), vec![], &dd);
    assert_eq!(region.categories.iter().cloned().zip(region.values[0].iter().cloned()).collect::<Vec<_>>(), vec![("Ctg".to_string(), Some(30.0)), ("Dhaka".to_string(), Some(30.0)), ("Sylhet".to_string(), Some(5.0))]);
    // refused in words
    for (level, value, why) in [(0, "Grand total", "The grand total is every row"), (0, "(blank)", "A blank group cannot filter the page"), (5, "x", "does not belong to a Rows field")] {
        let e = CrossPick::from_pivot(&flat, level, value, "i3", Some("t1")).expect_err("refused").to_string();
        assert!(e.contains(why), "{value}: {e}");
    }
    let e = CrossPick::from_pivot(&flat, 0, "Dhaka", "i3", None).expect_err("no data tab").to_string();
    assert_eq!(e, "This item has no data tab, so a click on it cannot filter the page.");
}

#[test]
fn d3_the_same_mark_again_clears_and_another_mark_replaces_and_clear_resets_the_page() {
    let spec = bar("Region");
    let dhaka = CrossPick::from_chart(&spec, "Dhaka", "i1", Some("t1")).expect("pick");
    let ctg = CrossPick::from_chart(&spec, "Ctg", "i1", Some("t1")).expect("pick");
    let mut st = PageState::default();
    assert!(st.toggle_cross(dhaka.clone()));
    assert!(st.toggle_cross(ctg.clone()), "another mark replaces the first");
    assert_eq!(st.cross, Some(ctg.clone()));
    assert!(!st.toggle_cross(ctg), "the same mark again clears it");
    assert_eq!(st.cross, None);
    assert_eq!(dhaka.chip(), "Region = Dhaka");
    st.toggle_cross(dhaka);
    st.drill_down("i1", "Dhaka");
    st.filter_picks[0] = Some(vec!["x".into()]);
    st.clear();
    assert_eq!(st, PageState::default());
}

#[test]
fn d3_a_click_on_other_a_date_part_or_a_blank_is_refused_with_what_to_do() {
    let spec = bar("Region");
    let other = CrossPick::from_chart(&spec, micro_parquet::chart::OTHER_LABEL, "i1", Some("t1")).expect_err("other").to_string();
    assert!(other.contains("values that Top N puts together"), "{other}");
    let part = ChartSpec { category: "Open Date \u{b7} Month".into(), category_part: Some(Default::default()), ..bar("x") };
    let e = CrossPick::from_chart(&part, "Jan", "i1", Some("t1")).expect_err("date part").to_string();
    assert!(e.contains("is a part of a date") && e.contains("page filter"), "{e}");
    let e = CrossPick::from_chart(&spec, "", "i1", Some("t1")).expect_err("blank").to_string();
    assert!(e.contains("blank group"), "{e}");
}

#[test]
fn d3_back_after_two_drill_levels_restores_the_first_level_and_each_item_drills_on_its_own() {
    let dd = dir("p10d3_drill");
    let src = book(&dd);
    let mut spec = bar("Region");
    spec.set_drill(vec![DrillLevel::column("Region"), DrillLevel::column("Product"), DrillLevel::column("Branch")]);
    let mut st = PageState::default();
    let at = |st: &PageState, item: &str| run(&src, &spec, vec![], st.drill_of(item).to_vec(), &dd);
    let top = at(&st, "i1");
    assert_eq!(shown(&top), (vec!["Ctg".to_string(), "Dhaka".to_string(), "Sylhet".to_string()], vec![Some(100.0), Some(180.0), Some(45.0)]));
    // level 1: Dhaka by product: FDR 150, SB 30
    st.drill_down("i1", "Dhaka");
    let l1 = at(&st, "i1");
    assert_eq!(shown(&l1), (vec!["FDR".to_string(), "SB".to_string()], vec![Some(150.0), Some(30.0)]));
    // level 2: Dhaka · FDR by branch: Gulshan 100, Motijheel 50
    st.drill_down("i1", "FDR");
    let l2 = at(&st, "i1");
    assert_eq!(shown(&l2), (vec!["Gulshan".to_string(), "Motijheel".to_string()], vec![Some(100.0), Some(50.0)]));
    assert_eq!(l2.spec.depth(), 2);
    // another item of the page is not drilled by it
    assert_eq!(shown(&at(&st, "i9")), shown(&top));
    // Back from level 2 gives level 1 again, exactly
    st.drill_back_to("i1", 1);
    assert_eq!(shown(&at(&st, "i1")), shown(&l1));
    // the breadcrumb's "All" goes to the top, and the path is gone from the state
    st.drill_back_to("i1", 0);
    assert_eq!(shown(&at(&st, "i1")), shown(&top));
    assert!(st.drill.is_empty());
    // a drill value past the last level is left out, never an error
    let deep = chart_spec_drilled(&spec, &["Dhaka".into(), "FDR".into(), "Gulshan".into(), "extra".into()]);
    assert_eq!(deep.depth(), 2);
}

#[test]
fn d3_the_cross_pick_and_the_drill_paths_are_saved_and_restored_with_the_project() {
    let mut p = Project::from_json(&std::fs::read_to_string(crate::common::fixtures_dir().join("phase9_replay/proj/phase9_replay.mpproj")).expect("fixture"), "x").expect("opens");
    let mut page = DashPage { id: "p1".into(), title: "Overview".into(), ..Default::default() };
    page.state.toggle_cross(CrossPick { dimension: "Region".into(), value: "Dhaka".into(), item: "i1".into(), tab: "t1".into(), model: None });
    page.state.drill_down("i2", "Dhaka");
    page.state.drill_down("i2", "FDR");
    let d = Dashboard { id: "d1".into(), title: "ALCO".into(), pages: vec![page], ..Default::default() };
    p.dashboard = Some(d.clone());
    let json = p.to_json().expect("json");
    let again = Project::from_json(&json, "x").expect("opens");
    assert_eq!(again.dashboard, Some(d));
    let st = &again.dashboard.as_ref().expect("d").pages[0].state;
    assert_eq!(st.cross.as_ref().map(|c| c.chip()), Some("Region = Dhaka".to_string()));
    assert_eq!(st.drill_of("i2"), ["Dhaka".to_string(), "FDR".to_string()]);
    assert_eq!(again.to_json().expect("json"), json, "save, open, save is stable");
    // a Phase 10 file written before D3 (a cross pick with only a dimension and a value) still opens
    let old = json.replace("\"item\": \"i1\",", "").replace("\"tab\": \"t1\",", "");
    assert!(Project::from_json(&old, "x").is_ok());
}
