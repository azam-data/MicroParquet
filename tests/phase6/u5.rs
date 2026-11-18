//! U5: the chart calculation (whole tab, categories, series, aggregations, Top N, order).

use crate::common::*;
use micro_parquet::chart::*;
use micro_parquet::models::*;
use micro_parquet::sort_lists::SortOrder;
use polars::prelude::*;
use std::collections::BTreeMap;

const TENORS: [&str; 5] = ["6m", "1y", "1m", "100D", "3m"];

/// 250 rows (three pages of 100): Term Period in 5 tenors, a Risk grade, Balance, Rate.
fn book() -> DataFrame {
    let n = 250usize;
    DataFrame::new(vec![
        Series::new("Term Period".into(), (0..n).map(|i| TENORS[i % 5].to_string()).collect::<Vec<_>>()),
        Series::new("Risk".into(), (0..n).map(|i| ["High", "Medium", "Low"][i % 3].to_string()).collect::<Vec<_>>()),
        Series::new("Balance".into(), (0..n).map(|i| 1_000.0 + i as f64 * 7.0).collect::<Vec<f64>>()),
        Series::new("Rate".into(), (0..n).map(|i| 5.0 + (i % 7) as f64 * 0.5).collect::<Vec<f64>>()),
        Series::new("Qty".into(), (0..n).map(|i| i as i32).collect::<Vec<i32>>()),
    ])
    .expect("the book")
}

fn spec(category: &str, value: Option<&str>, agg: ChartAgg) -> ChartSpec {
    ChartSpec { category: category.into(), value: value.map(str::to_string), agg, ..Default::default() }
}

fn expected_sums(df: &DataFrame, cat: &str, val: &str) -> BTreeMap<String, f64> {
    let c = strings(df, cat);
    let v = floats(df, val);
    let mut m = BTreeMap::new();
    for (c, v) in c.into_iter().zip(v) {
        *m.entry(c.unwrap_or_default()).or_insert(0.0) += v.unwrap_or(0.0);
    }
    m
}

fn by_cat(d: &ChartData, series: usize) -> BTreeMap<String, f64> {
    d.categories.iter().cloned().zip(d.values[series].iter().map(|v| v.unwrap_or(0.0))).collect()
}

#[test]
fn u5_the_sums_equal_a_group_by_over_all_250_rows_and_not_the_first_page() {
    let df = book();
    let d = compute_chart(df.clone().lazy(), &spec("Term Period", Some("Balance"), ChartAgg::Sum)).expect("chart");
    let want = expected_sums(&df, "Term Period", "Balance");
    let got = by_cat(&d, 0);
    assert_eq!(got.len(), 5);
    for (k, v) in &want {
        assert!((got[k] - v).abs() < 1e-6, "{k}: {} vs {v}", got[k]);
    }
    assert_eq!(d.total_rows, 250);
    assert_eq!(d.rows[0].iter().sum::<u64>(), 250, "every row is in some bar");
    // the first page alone would give a different number
    let page = df.head(Some(100));
    assert!((expected_sums(&page, "Term Period", "Balance")["6m"] - want["6m"]).abs() > 1.0);
    assert!(d.title(Some("Crore")).starts_with("Sum of Balance by Term Period: all 250 rows"), "{}", d.title(Some("Crore")));
    assert!(d.title(Some("Crore")).ends_with("(Crore)"));
    assert!(d.title(None).ends_with("rows"), "no unit, no brackets: {}", d.title(None));
}

#[test]
fn u5_tenor_order_puts_1m_3m_100d_6m_1y_and_a_list_or_value_order_work_too() {
    let df = book();
    let mut s = spec("Term Period", Some("Balance"), ChartAgg::Sum);
    s.order = SortOrder::Tenor;
    let d = compute_chart(df.clone().lazy(), &s).expect("chart");
    assert_eq!(d.categories, vec!["1m", "3m", "100D", "6m", "1y"]);
    // a custom list
    s.order = SortOrder::List { name: "x".into(), values: vec!["1y".into(), "6m".into()], case_insensitive: true };
    let d = compute_chart(df.clone().lazy(), &s).expect("chart");
    assert_eq!(d.categories, vec!["1y", "6m", "1m", "3m", "100D"], "the listed ones first, the rest in natural order");
    // by value: the largest first
    let mut s = spec("Term Period", Some("Balance"), ChartAgg::Sum);
    s.by_value = true;
    let d = compute_chart(df.clone().lazy(), &s).expect("chart");
    let sums = by_cat(&d, 0);
    let ordered: Vec<f64> = d.categories.iter().map(|c| sums[c]).collect();
    assert!(ordered.windows(2).all(|w| w[0] >= w[1]), "{ordered:?}");
    // natural by default: case-insensitive, numbers inside the text by value
    let d = compute_chart(df.lazy(), &spec("Term Period", Some("Balance"), ChartAgg::Sum)).expect("chart");
    assert_eq!(d.categories, vec!["1m", "1y", "3m", "6m", "100D"]);
}

#[test]
fn u5_an_int32_and_a_decimal_value_column_give_correct_sums() {
    let mut df = book();
    let dec = Series::new("Dec".into(), (0..250).map(|i| 1.25 + i as f64).collect::<Vec<f64>>()).cast(&DataType::Decimal(Some(18), Some(2))).expect("decimal");
    df.with_column(dec).expect("decimal column");
    let d = compute_chart(df.clone().lazy(), &spec("Risk", Some("Qty"), ChartAgg::Sum)).expect("chart on an Int32");
    let want = expected_sums(&df.clone().lazy().with_column(col("Qty").cast(DataType::Float64)).collect().expect("cast"), "Risk", "Qty");
    for (k, v) in &want {
        assert_eq!(by_cat(&d, 0)[k], *v, "{k}");
    }
    let d = compute_chart(df.clone().lazy(), &spec("Risk", Some("Dec"), ChartAgg::Sum)).expect("chart on a Decimal");
    let want = expected_sums(&df.lazy().with_column(col("Dec").cast(DataType::Float64)).collect().expect("cast"), "Risk", "Dec");
    for (k, v) in &want {
        assert!((by_cat(&d, 0)[k] - v).abs() < 1e-6, "{k}: {} vs {v}", by_cat(&d, 0)[k]);
    }
}

#[test]
fn u5_count_needs_no_number_column_and_min_max_average_work() {
    let df = book();
    let d = compute_chart(df.clone().lazy(), &spec("Risk", None, ChartAgg::Count)).expect("count");
    assert_eq!(by_cat(&d, 0)["High"] + by_cat(&d, 0)["Medium"] + by_cat(&d, 0)["Low"], 250.0);
    assert_eq!(by_cat(&d, 0)["High"], 84.0, "rows 0,3,6.. of 250");
    assert!(d.title(Some("Crore")).starts_with("Count of rows by Risk"), "{}", d.title(Some("Crore")));
    assert!(!d.title(Some("Crore")).contains("(Crore)"), "a count has no money unit");
    let bal = floats(&df, "Balance");
    let high: Vec<f64> = bal.iter().enumerate().filter(|(i, _)| i % 3 == 0).map(|(_, v)| v.unwrap_or(0.0)).collect();
    let avg = compute_chart(df.clone().lazy(), &spec("Risk", Some("Balance"), ChartAgg::Average)).expect("avg");
    assert!((by_cat(&avg, 0)["High"] - high.iter().sum::<f64>() / high.len() as f64).abs() < 1e-6);
    let min = compute_chart(df.clone().lazy(), &spec("Risk", Some("Balance"), ChartAgg::Min)).expect("min");
    let max = compute_chart(df.lazy(), &spec("Risk", Some("Balance"), ChartAgg::Max)).expect("max");
    assert_eq!(by_cat(&min, 0)["High"], high.iter().cloned().fold(f64::INFINITY, f64::min));
    assert_eq!(by_cat(&max, 0)["High"], high.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
}

#[test]
fn u5_a_weighted_average_follows_the_phase_2_rule() {
    let df = DataFrame::new(vec![
        Series::new("Bank".into(), &["A", "A", "B", "B", "C", "C"]),
        Series::new("Rate".into(), &[Some(10.0), Some(6.0), Some(8.0), None, Some(5.0), Some(7.0)]),
        Series::new("Balance".into(), &[300.0, 100.0, 200.0, 900.0, 100.0, -50.0]),
    ])
    .expect("frame");
    let mut s = spec("Bank", Some("Rate"), ChartAgg::WeightedAvg);
    s.weight = Some("Balance".into());
    let d = compute_chart(df.lazy(), &s).expect("w.avg");
    let m = by_cat(&d, 0);
    assert!((m["A"] - (10.0 * 300.0 + 6.0 * 100.0) / 400.0).abs() < 1e-9, "{}", m["A"]);
    assert!((m["B"] - 8.0).abs() < 1e-9, "a row without a rate is left out of both sums: {}", m["B"]);
    let ci = d.categories.iter().position(|c| c == "C").expect("C");
    assert_eq!(d.values[0][ci], None, "weights of both signs: no rate");
    assert!(d.notes.iter().any(|n| n.contains("both signs")), "{:?}", d.notes);
    // the weight column is needed
    s.weight = None;
    let err = compute_chart(DataFrame::new(vec![Series::new("Bank".into(), &["A"]), Series::new("Rate".into(), &[1.0])]).expect("f").lazy(), &s).expect_err("needs a weight");
    assert!(format!("{err:#}").contains("weight column"), "{err:#}");
}

#[test]
fn u5_top_n_3_gives_3_categories_plus_other_and_the_totals_are_kept() {
    let df = book();
    let mut s = spec("Term Period", Some("Balance"), ChartAgg::Sum);
    s.top_n = 3;
    let d = compute_chart(df.clone().lazy(), &s).expect("chart");
    assert_eq!(d.categories.len(), 4, "{:?}", d.categories);
    assert_eq!(d.categories.last().map(String::as_str), Some("Other"));
    let total: f64 = floats(&df, "Balance").iter().flatten().sum();
    assert!((d.values[0].iter().flatten().sum::<f64>() - total).abs() < 1e-6, "the totals are kept");
    assert_eq!((d.distinct_categories, d.folded_categories), (5, 2));
    assert_eq!(d.rows[0].iter().sum::<u64>(), 250);
    assert!(d.title(Some("Crore")).contains("top 3 of 5 categories"), "{}", d.title(Some("Crore")));
    // "Other" is the rest recalculated, not an average of averages: with Average the rest is the mean of its rows
    s.agg = ChartAgg::Average;
    let d = compute_chart(df.clone().lazy(), &s).expect("chart");
    let kept: Vec<&str> = d.categories.iter().map(String::as_str).filter(|c| *c != "Other").collect();
    let rest: Vec<f64> = strings(&df, "Term Period").into_iter().zip(floats(&df, "Balance")).filter(|(c, _)| !kept.contains(&c.clone().unwrap_or_default().as_str())).filter_map(|(_, v)| v).collect();
    let oi = d.categories.iter().position(|c| c == "Other").expect("Other");
    assert!((d.values[0][oi].expect("value") - rest.iter().sum::<f64>() / rest.len() as f64).abs() < 1e-6);
    // a real category called "Other" is not mixed up with the rest
    let df2 = DataFrame::new(vec![Series::new("K".into(), &["Other", "A", "B", "C"]), Series::new("V".into(), &[100.0, 50.0, 40.0, 30.0])]).expect("frame");
    let mut s2 = spec("K", Some("V"), ChartAgg::Sum);
    s2.top_n = 2;
    let d = compute_chart(df2.lazy(), &s2).expect("chart");
    assert!(d.categories.contains(&"Other".to_string()) && d.categories.contains(&"Other categories".to_string()), "{:?}", d.categories);
}

#[test]
fn u5_a_text_value_column_with_sum_is_refused_naming_its_type_and_other_mistakes_too() {
    let df = book();
    let err = compute_chart(df.clone().lazy(), &spec("Term Period", Some("Risk"), ChartAgg::Sum)).expect_err("text value");
    let m = format!("{err:#}");
    assert!(m.contains("'Risk'") && m.contains("Text") && m.contains("Sum") && m.contains("Count"), "{m}");
    let err = compute_chart(df.clone().lazy(), &spec("Term Period", None, ChartAgg::Sum)).expect_err("no value");
    assert!(format!("{err:#}").contains("Value column"));
    let err = compute_chart(df.clone().lazy(), &spec("Nope", Some("Balance"), ChartAgg::Sum)).expect_err("no category");
    assert!(format!("{err:#}").contains("'Nope'"));
    let mut s = spec("Term Period", Some("Balance"), ChartAgg::Sum);
    s.series = Some("Term Period".into());
    assert!(format!("{:#}", compute_chart(df.clone().lazy(), &s).expect_err("same column")).contains("same as the Category"));
    // a date as the value
    let d = DataFrame::new(vec![Series::new("K".into(), &["a"]), dates("When", &["2026-01-01"])]).expect("frame");
    assert!(format!("{:#}", compute_chart(d.lazy(), &spec("K", Some("When"), ChartAgg::Sum)).expect_err("date")).contains("a Date"));
}

#[test]
fn u5_a_series_makes_grouped_bars_with_at_most_eight_and_the_rest_as_other() {
    let df = book();
    let mut s = spec("Term Period", Some("Balance"), ChartAgg::Sum);
    s.series = Some("Risk".into());
    let d = compute_chart(df.clone().lazy(), &s).expect("chart");
    assert_eq!(d.series.len(), 3);
    let total: f64 = d.values.iter().flatten().flatten().sum();
    assert!((total - floats(&df, "Balance").iter().flatten().sum::<f64>()).abs() < 1e-6);
    // twelve series: 7 and "Other"
    let n = 120usize;
    let many = DataFrame::new(vec![
        Series::new("K".into(), (0..n).map(|i| format!("k{}", i % 3)).collect::<Vec<_>>()),
        Series::new("S".into(), (0..n).map(|i| format!("s{:02}", i % 12)).collect::<Vec<_>>()),
        Series::new("V".into(), (0..n).map(|i| i as f64 + 1.0).collect::<Vec<f64>>()),
    ])
    .expect("frame");
    let mut s = spec("K", Some("V"), ChartAgg::Sum);
    s.series = Some("S".into());
    let d = compute_chart(many.clone().lazy(), &s).expect("chart");
    assert_eq!(d.series.len(), MAX_SERIES, "{:?}", d.series);
    assert_eq!(d.series.last().map(String::as_str), Some("Other"));
    let total: f64 = d.values.iter().flatten().flatten().sum();
    assert!((total - (n * (n + 1) / 2) as f64).abs() < 1e-6, "the totals are kept: {total}");
    assert!(d.notes.iter().any(|x| x.contains("drawn together")), "{:?}", d.notes);
}

#[test]
fn u5_blank_categories_go_last_and_the_chart_table_has_one_row_per_bar() {
    let df = DataFrame::new(vec![
        Series::new("K".into(), &[Some("b"), None, Some("a"), Some("  "), Some("b")]),
        Series::new("V".into(), &[1.0, 2.0, 3.0, 4.0, 5.0]),
    ])
    .expect("frame");
    let d = compute_chart(df.lazy(), &spec("K", Some("V"), ChartAgg::Sum)).expect("chart");
    assert_eq!(d.categories, vec!["a", "b", "(blank)"], "blanks (null and spaces) are one category, last");
    assert_eq!(d.values[0], vec![Some(3.0), Some(6.0), Some(6.0)]);
    let t = chart_table(&d).expect("table");
    assert_eq!(t.get_column_names().iter().map(|s| s.to_string()).collect::<Vec<_>>(), vec!["K", "Sum of V", "Rows"]);
    assert_eq!(t.height(), 3);
    assert_eq!(strings(&t, "K"), vec![Some("a".to_string()), Some("b".to_string()), Some("(blank)".to_string())]);
}

#[test]
fn u5_a_spec_saved_without_the_new_fields_loads_and_keeps_its_defaults() {
    let s: ChartSpec = serde_json::from_str(r#"{"category":"Term Period","value":"Balance"}"#).expect("loads");
    assert_eq!((s.agg, s.kind, s.top_n, s.scale, s.by_value), (ChartAgg::Sum, ChartKind::Bar, 30, ChartScale::Follow, false));
    let back: ChartSpec = serde_json::from_str(&serde_json::to_string(&s).expect("json")).expect("back");
    assert_eq!(back, s);
    assert_eq!(ChartScale::Crore.fixed(), Some(MetricFormatType::BdtCrore.scale()));
    assert_eq!(ChartScale::Follow.fixed(), None);
    assert_eq!(ChartScale::Plain.fixed(), Some(None));
}
