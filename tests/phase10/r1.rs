//! R1: the HTML export engine. Every count here is worked by hand in the comments.

use crate::common::{dir, parquet};
use crate::p9_m3::{nums, strs};
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec, DrillLevel};
use micro_parquet::dashboard::*;
use micro_parquet::models::*;
use micro_parquet::report_html::*;
use polars::prelude::*;

const CRORE: (f64, usize, &str) = (10_000_000.0, 2, "Crore");

fn scale() -> (f64, usize, String) {
    (CRORE.0, CRORE.1, CRORE.2.to_string())
}

fn names(prefix: &str, n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("{prefix}{i:02}")).collect()
}

fn filter(id: &str, label: &str, field: &str) -> PageFilter {
    PageFilter { id: id.into(), label: label.into(), tab: "t1".into(), field: field.into(), ..Default::default() }
}

/// 12 values of A × 4 of B × 6 of Cat, one row each (288 rows) of 1,000 Crore (10,000,000,000 BDT).
fn grid_book(d: &std::path::Path) -> TabSource {
    let (mut a, mut b, mut c) = (Vec::new(), Vec::new(), Vec::new());
    for i in 1..=12 {
        for j in 1..=4 {
            for k in 1..=6 {
                a.push(format!("a{i:02}"));
                b.push(format!("b{j}"));
                c.push(format!("c{k}"));
            }
        }
    }
    let n = a.len();
    let df = DataFrame::new(vec![Series::new("A".into(), a).into(), Series::new("B".into(), b).into(), Series::new("Cat".into(), c).into(), nums("Balance", &vec![10_000_000_000.0; n])]).expect("data");
    parquet(d, "r1_grid", &df)
}

fn bar(category: &str) -> ChartSpec {
    ChartSpec { category: category.into(), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 500, ..Default::default() }
}

fn chart_item(id: &str, title: &str, src: &TabSource, spec: ChartSpec) -> ReportItem {
    ReportItem { id: id.into(), title: title.into(), source: ItemSource::Chart { base: src.clone(), steps: vec![], spec }, data_tab: Some("t1".into()), scale: scale() }
}

fn plan_of(page: ReportPage, d: &std::path::Path) -> ReportPlan {
    ReportPlan { title: "ALCO pack".into(), pages: vec![page], template_bytes: 0, temp_dir: d.to_path_buf() }
}

#[test]
fn r1_two_filters_of_12_and_4_values_make_65_states_and_a_6_bar_chart_390_numbers_at_8_bytes_each() {
    // hand-worked: (12 + 1) x (4 + 1) = 65 states; 65 x 6 bars = 390 numbers; 390 x 8 = 3,120 bytes
    assert_eq!(state_count(&[12, 4]), 65);
    assert_eq!(predict_numbers(&[12, 4], 6), 390);
    assert_eq!(predict_numbers(&[12, 4], 6) * BYTES_PER_NUMBER, 3120);
    let dd = dir("p10r1_count");
    let src = grid_book(&dd);
    let page = ReportPage { id: "p1".into(), title: "Overview".into(), filters: vec![filter("f1", "A", "A"), filter("f2", "B", "B")], values: vec![names("a", 12), (1..=4).map(|j| format!("b{j}")).collect()], items: vec![chart_item("i1", "By category", &src, bar("Cat"))] };
    let plan = plan_of(page, &dd);
    let answers = compute_answers(&plan).expect("answers");
    let ia = &answers[0].items[0];
    assert_eq!(ia.answers.len(), 65, "one answer per state");
    assert_eq!(ia.numbers, 390, "65 x 6 bars");
    // the bytes of the numbers: 8 a number, within 10% (the values are 1,000.00 to 48,000.00 Crore)
    let bytes = numbers_json_bytes(&plan, &answers) as f64;
    assert!((bytes - 3120.0).abs() / 3120.0 <= 0.10, "{bytes} bytes against 3,120");
    // two answers worked by hand: A = a01 and B = b1 is one row per bar: 10,000,000,000 / 10,000,000 = 1,000.00 Crore; All and All is 48 rows: 48,000.00
    let find = |picks: [Option<&str>; 2]| ia.answers.iter().find(|a| a.picks == picks.map(|p| p.map(String::from)).to_vec() && a.path.is_empty()).expect("the state");
    let one = find([Some("a01"), Some("b1")]);
    let ItemData::Chart(c) = &one.data else { panic!("chart") };
    assert_eq!(c.values[0], vec![Some(10_000_000_000.0); 6]);
    assert_eq!(values_json(one, &scale()), "[[1000.00,1000.00,1000.00,1000.00,1000.00,1000.00]]");
    let all = find([None, None]);
    assert!(values_json(all, &scale()).starts_with("[[48000.00,"));
    // the stub file: the estimate is within 10% of the real thing
    let json = answers_json(&plan, &answers);
    let file = render_stub(&plan, &json);
    let est = estimate(&plan, json.len()).total() as f64;
    assert!((est - file.len() as f64).abs() / file.len() as f64 <= 0.10, "estimate {est} against {}", file.len());
    assert!(file.len() > micro_parquet::charts::echarts::ECHARTS_JS.len(), "the library is in the file");
}

#[test]
fn r1_five_point_three_mb_by_email_is_refused_with_the_suggestions_in_order_and_fewer_filter_values_bring_it_under_5_mb() {
    // 67 items of 120 numbers each; two filters of 12 and 4 values: 65 states x 120 x 8 = 62,400 bytes an item
    let shapes = |a: usize| -> Vec<(Vec<usize>, usize)> { (0..67).map(|_| (vec![a, 4], 120)).collect() };
    let plan = ReportPlan { title: "ALCO pack".into(), pages: vec![], template_bytes: 0, temp_dir: std::env::temp_dir() };
    let big = estimate(&plan, predict_answer_bytes(&shapes(12)));
    assert_eq!(predict_answer_bytes(&shapes(12)), 67 * 62_400);
    assert!((5_250_000..=5_350_000).contains(&big.total()), "about 5.3 MB: {}", big.total());
    let g = gate(true, &big, true, 3);
    assert!(!g.allowed && g.bytes == big.total());
    assert_eq!(g.suggestions, vec!["Use fewer filter values.", "Use fewer drill levels.", "Put Top N on the biggest chart.", "Use fewer pages."]);
    assert!(g.message.contains("over the 5 MB an email takes"), "{}", g.message);
    // what does not apply is left out; the design picture is named when there is one
    let with_pic = SizeParts { template: 400_000, ..big };
    assert_eq!(gate(true, &with_pic, false, 1).suggestions, vec!["Use fewer filter values.", "Put Top N on the biggest chart.", "Leave out the design picture."]);
    // "no" to the email question: no limit
    assert!(gate(false, &big, true, 3).allowed);
    // 6 values of A: 7 x 5 = 35 states, 35 x 120 x 8 = 33,600 an item: under 5 MB
    let small = estimate(&plan, predict_answer_bytes(&shapes(6)));
    assert!(small.total() < EMAIL_LIMIT_BYTES, "{}", small.total());
    assert!(gate(true, &small, true, 3).allowed);
    // exactly 5,000,000 is allowed, one byte more is not
    let edge = SizeParts { echarts: 0, shell: 0, template: 0, answers: EMAIL_LIMIT_BYTES };
    assert!(gate(true, &edge, false, 1).allowed);
    assert!(!gate(true, &SizeParts { answers: EMAIL_LIMIT_BYTES + 1, ..edge }, false, 1).allowed);
}

#[test]
fn r1_the_answers_equal_compute_item_for_20_random_states_of_a_chart_a_drilled_chart_and_a_nested_pivot() {
    let dd = dir("p10r1_random");
    let p = crate::common::fixtures_dir().join("phase9_replay/data/Accounts.csv");
    let src = TabSource::SingleFile { path: p.clone(), is_parquet: false, config: micro_parquet::engine::auto_detect_smart_schema(&p).expect("a schema") };
    let f_wing = filter("f1", "Wing", "Wing");
    let f_prod = filter("f2", "Product", "Product code");
    let values: Vec<Vec<String>> = [&f_wing, &f_prod].iter().map(|f| field_values(&src, &[], f, &dd).expect("values").0).collect();
    assert!(values[0].len() >= 2 && values[1].len() >= 2, "{values:?}");
    let mut drilled = bar("Wing");
    drilled.set_drill(vec![DrillLevel::column("Wing"), DrillLevel::column("Product code")]);
    let nested = crate::p10_p9_baseline::nested_pivot();
    let pivot = ReportItem { id: "i3".into(), title: "Nested".into(), source: ItemSource::Pivot { base: src.clone(), steps: vec![], spec: nested }, data_tab: Some("t1".into()), scale: scale() };
    let page = ReportPage { id: "p1".into(), title: "P".into(), filters: vec![f_wing.clone(), f_prod.clone()], values: values.clone(), items: vec![chart_item("i1", "Plain", &src, bar("Branch")), chart_item("i2", "Drilled", &src, drilled), pivot] };
    let plan = plan_of(page, &dd);
    let answers = compute_answers(&plan).expect("answers");
    // states: (wings + 1) x (products + 1) for each item; the drilled chart has one more answer per top-level category
    let states = (values[0].len() + 1) * (values[1].len() + 1);
    assert_eq!(answers[0].items[0].answers.len(), states);
    assert_eq!(answers[0].items[2].answers.len(), states);
    assert!(answers[0].items[1].answers.len() > states, "the drilled chart has its levels too");
    assert!(answers[0].items[1].answers.iter().any(|a| a.path.len() == 1), "a drill path of one value");
    // 20 pseudo-random (item, answer) pairs: recompute each from its picks and path, and compare what the file would show
    let mut x: u64 = 987_654_321;
    let mut next = |n: usize| {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((x >> 33) as usize) % n
    };
    for _ in 0..20 {
        let ii = next(3);
        let ia = &answers[0].items[ii];
        let a = &ia.answers[next(ia.answers.len())];
        let picks: Vec<(PageFilter, Vec<String>)> = a.picks.iter().enumerate().filter_map(|(i, p)| p.clone().map(|v| (plan.pages[0].filters[i].clone(), vec![v]))).collect();
        let req = ItemRequest { item_id: "x".into(), source: plan.pages[0].items[ii].source.clone(), picks, drill: a.path.clone(), temp_dir: dd.clone() };
        let again = compute_item(&req).expect("again");
        match (&a.data, &again) {
            (ItemData::Chart(g), ItemData::Chart(w)) => assert_eq!((&g.categories, &g.values), (&w.categories, &w.values), "item {ii} picks {:?} path {:?}", a.picks, a.path),
            (ItemData::Table(g), ItemData::Table(w)) => {
                assert!(g.df.equals_missing(&w.df), "item {ii} picks {:?}", a.picks);
                assert_eq!(g.nested.as_ref().map(|n| n.rows.iter().map(|r| r.label.clone()).collect::<Vec<_>>()), w.nested.as_ref().map(|n| n.rows.iter().map(|r| r.label.clone()).collect::<Vec<_>>()));
            }
            other => panic!("the same kind: {other:?}"),
        }
    }
    // a filter that picks nothing in the data leaves "All" for the others: a state with a pick has fewer rows than All
    let all = &answers[0].items[0].answers[0];
    assert!(all.picks.iter().all(|p| p.is_none()));
}

#[test]
fn r1_the_items_that_do_not_react_to_clicks_are_listed_by_name_with_why() {
    let dd = dir("p10r1_linked");
    let src = grid_book(&dd);
    let mut avg = bar("Cat");
    avg.agg = ChartAgg::Average;
    let sum_pivot = PivotSpec { rows: vec!["Cat".into()], values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None }], ..Default::default() };
    let avg_pivot = PivotSpec { values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Average, weight_col: None, time: None }], ..sum_pivot.clone() };
    let pct_pivot = PivotSpec { show_as: PivotShowAs::PctOfGrandTotal, ..sum_pivot.clone() };
    let item = |id: &str, title: &str, source: ItemSource| ReportItem { id: id.into(), title: title.into(), source, data_tab: Some("t1".into()), scale: scale() };
    let page = ReportPage {
        id: "p1".into(),
        title: "Overview".into(),
        filters: vec![],
        values: vec![],
        items: vec![
            chart_item("i1", "Deposits by category", &src, bar("Cat")),
            chart_item("i2", "Average ticket", &src, avg),
            item("i3", "Balance table", ItemSource::Pivot { base: src.clone(), steps: vec![], spec: sum_pivot }),
            item("i4", "Average table", ItemSource::Pivot { base: src.clone(), steps: vec![], spec: avg_pivot }),
            item("i5", "Share table", ItemSource::Pivot { base: src.clone(), steps: vec![], spec: pct_pivot }),
        ],
    };
    let plan = plan_of(page, &dd);
    let answers = compute_answers(&plan).expect("answers");
    let listed = not_linked(&plan, &answers);
    assert_eq!(listed.len(), 3, "{listed:?}");
    assert!(listed.iter().any(|l| l.contains("Average ticket") && l.contains("'Average'")), "{listed:?}");
    assert!(listed.iter().any(|l| l.contains("Average table")));
    assert!(listed.iter().any(|l| l.contains("Share table") && l.contains("percent of a total")));
    assert!(!listed.iter().any(|l| l.contains("Deposits by category") || l.contains("Balance table")), "sums are linked");
    assert!(answers[0].items[0].linked.is_ok() && answers[0].items[2].linked.is_ok());
}

#[test]
fn r1_no_account_row_goes_into_the_file_and_an_item_over_20000_numbers_is_refused_with_what_to_cut() {
    let dd = dir("p10r1_rows");
    let df = DataFrame::new(vec![strs("Account", &["9988776655", "9988776656", "9988776657", "9988776658"]).into(), strs("Region", &["Dhaka", "Ctg", "Dhaka", "Ctg"]).into(), nums("Balance", &[10_000_000_000.0, 20_000_000_000.0, 30_000_000_000.0, 40_000_000_000.0])]).expect("data");
    let src = parquet(&dd, "r1_rows", &df);
    let f = filter("f1", "Region", "Region");
    let page = ReportPage { id: "p1".into(), title: "P".into(), filters: vec![f], values: vec![vec!["Ctg".into(), "Dhaka".into()]], items: vec![chart_item("i1", "By region", &src, bar("Region"))] };
    let plan = plan_of(page, &dd);
    let answers = compute_answers(&plan).expect("answers");
    let json = answers_json(&plan, &answers);
    let file = render_stub(&plan, &json);
    for acct in ["9988776655", "9988776656", "9988776657", "9988776658"] {
        assert!(!json.contains(acct) && !file.contains(acct), "the account {acct} is not in the file");
    }
    assert!(json.contains("Dhaka") && json.contains("Ctg"));
    // Dhaka is 1,000 + 3,000 = 4,000.00 Crore; Ctg 2,000 + 4,000 = 6,000.00
    assert!(json.contains("[[6000.00,4000.00]]"), "{json}");
    // a filter with 25,000 values makes 25,001 states: refused before any work, with what to cut
    let big = ReportPage { id: "p2".into(), title: "Big".into(), filters: vec![filter("f1", "Account", "Account")], values: vec![(0..25_000).map(|i| format!("v{i}")).collect()], items: vec![chart_item("i1", "By region", &src, bar("Region"))] };
    let e = compute_answers(&plan_of(big, &dd)).expect_err("too many").to_string();
    assert_eq!(e, "The item 'By region' would hold 25001 numbers in the file; the limit is 20000. Cut the filter values, put Top N on the chart, or use fewer drill levels.");
    // nothing to export
    let empty = ReportPlan { title: "x".into(), pages: vec![ReportPage { id: "p".into(), title: "P".into(), filters: vec![], values: vec![], items: vec![] }], template_bytes: 0, temp_dir: dd.clone() };
    assert!(check_plan(&empty).expect_err("empty").to_string().contains("no items to put in a file"));
}

#[test]
fn r1_the_states_are_all_first_and_a_filter_that_does_not_reach_an_item_leaves_its_answers_alone() {
    let a = vec!["x".to_string(), "y".to_string()];
    let b = vec!["p".to_string()];
    let s = states(&[&a, &b]);
    assert_eq!(s.len(), 6);
    assert_eq!(s[0], vec![None, None]);
    assert_eq!(s[1], vec![None, Some("p".to_string())]);
    assert_eq!(s[5], vec![Some("y".to_string()), Some("p".to_string())]);
    assert_eq!(states(&[]).len(), 1, "no filters: one state");
    // an item on another data tab: two filters, none reach it: one answer
    let dd = dir("p10r1_unlinked");
    let src = grid_book(&dd);
    let mut item = chart_item("i1", "Elsewhere", &src, bar("Cat"));
    item.data_tab = Some("t9".into());
    let page = ReportPage { id: "p1".into(), title: "P".into(), filters: vec![filter("f1", "A", "A"), filter("f2", "B", "B")], values: vec![names("a", 12), vec!["b1".into()]], items: vec![item] };
    let answers = compute_answers(&plan_of(page, &dd)).expect("answers");
    assert_eq!(answers[0].items[0].answers.len(), 1);
    assert!(answers[0].items[0].filters.is_empty());
    assert_eq!(answers[0].items[0].answers[0].picks, vec![None, None]);
}
