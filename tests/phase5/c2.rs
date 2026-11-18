//! Phase 5 C2: the Column inspector's distribution, text and date figures.

use crate::common::*;
use micro_parquet::column_stats::*;
use polars::prelude::*;

fn stats(df: DataFrame, col: &str) -> ColumnStats {
    compute_column_stats(df.lazy(), col, 0).expect("stats")
}

#[test]
fn c2_a_skewed_column_is_binned_between_the_1st_and_99th_percentile_with_the_outlier_counted_above() {
    // 99 values from 1 to 100 and one 1e9
    let mut v: Vec<f64> = (1..=100).map(|i| i as f64).filter(|i| *i != 50.0).collect();
    assert_eq!(v.len(), 99);
    v.push(1e9);
    let n = stats(DataFrame::new(vec![Series::new("X".into(), v)]).expect("df"), "X").number.expect("number stats");
    let filled = n.histogram.iter().filter(|c| **c > 0).count();
    assert!(filled > 1, "more than one non-empty bin: {:?}", n.histogram);
    assert_eq!(n.above, 1, "the 1e9 is above the range");
    assert!(n.range_hi <= 100.0, "the bars end at the 99th percentile, not at 1e9: {}", n.range_hi);
    assert_eq!(n.histogram.iter().sum::<u64>() + n.below + n.above, 100, "bars, below and above add up to every value");
    // the sum, min and max are still those of all the values
    assert_eq!(n.max, 1e9);
    assert_eq!(n.min, 1.0);
    // a column with no outliers keeps its full range: nothing below or above
    let flat: Vec<f64> = (1..=50).map(|i| i as f64).collect();
    let n = stats(DataFrame::new(vec![Series::new("X".into(), flat)]).expect("df"), "X").number.expect("number stats");
    assert_eq!((n.below, n.above, n.range_lo, n.range_hi), (0, 0, 1.0, 50.0));
    assert_eq!(n.histogram.iter().sum::<u64>(), 50);
    // all values equal: one bar, nothing outside
    let n = stats(DataFrame::new(vec![Series::new("X".into(), vec![7.0; 20])]).expect("df"), "X").number.expect("number stats");
    assert_eq!((n.below, n.above, n.histogram[0]), (0, 0, 20));
}

#[test]
fn c2_text_columns_list_the_ten_most_frequent_values_with_counts() {
    let mut v: Vec<String> = Vec::new();
    for i in 0..14 {
        for _ in 0..(20 - i) {
            v.push(format!("v{i:02}"));
        }
    }
    let t = stats(DataFrame::new(vec![Series::new("T".into(), v)]).expect("df"), "T").text.expect("text stats");
    assert_eq!(t.top.len(), 10);
    assert_eq!(t.top[0], ("v00".to_string(), 20));
    assert_eq!(t.top[9], ("v09".to_string(), 11));
    assert_eq!(t.distinct, 14);
}

#[test]
fn c2_date_columns_count_the_values_per_month() {
    let d = dates("D", &["2026-01-05", "2026-01-31", "2026-03-01", "", "2025-12-31", "2026-03-15", "2026-03-20"]);
    let s = stats(DataFrame::new(vec![d]).expect("df"), "D");
    let dt = s.date.expect("date stats");
    assert_eq!(dt.months, vec![(2025, 12, 1), (2026, 1, 2), (2026, 3, 3)], "oldest first, months with no value are not listed, a blank is not counted");
    assert_eq!(s.blanks, 1);
}
