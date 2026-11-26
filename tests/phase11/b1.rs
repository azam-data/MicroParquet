//! Phase 11 B1: the variance bridge (volume, mix, rate), worked by hand.

use micro_parquet::bridge::*;
use micro_parquet::mom::*;
use polars::prelude::*;

fn it(k: &str, b0: f64, i0: f64, b1: f64, i1: f64) -> Item {
    Item { key: k.into(), b0, i0, b1, i1 }
}

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} vs {b}");
}

#[test]
fn b1_a_two_products_by_hand() {
    // A: 100 at 5% (5) -> 150 at 6% (9). B: 100 at 10% (10) -> 50 at 8% (4). Average rate before 15/200 = 7.5%.
    let v = volume_rate_mix(&[it("A", 100.0, 5.0, 150.0, 9.0), it("B", 100.0, 10.0, 50.0, 4.0)]).expect("bridge");
    near(v.open, 15.0);
    near(v.volume, 0.0); // the total balance is 200 both times
    near(v.mix, 150.0 * 0.05 + 50.0 * 0.10 - 200.0 * 0.075); // -2.5: balance moved to the cheaper product
    near(v.rate, (9.0 - 7.5) + (4.0 - 5.0)); // 0.5
    near(v.close, 13.0);
    near(v.volume + v.mix + v.rate, v.close - v.open);
}

#[test]
fn b1_b_a_new_product_takes_the_average_rate_before_and_a_closed_one_leaves() {
    let v = volume_rate_mix(&[it("A", 100.0, 5.0, 150.0, 9.0), it("B", 100.0, 10.0, 50.0, 4.0), it("C", 0.0, 0.0, 100.0, 8.0)]).expect("bridge");
    near(v.volume, 100.0 * 0.075);
    near(v.mix, 7.5 + 5.0 + 100.0 * 0.075 - 300.0 * 0.075);
    near(v.rate, 0.5 + (8.0 - 7.5)); // C's 8% against the 7.5% average
    near(v.volume + v.mix + v.rate, 21.0 - 15.0);
    // B closes: no balance now, no interest now
    let v = volume_rate_mix(&[it("A", 100.0, 5.0, 150.0, 9.0), it("B", 100.0, 10.0, 0.0, 0.0)]).expect("bridge");
    near(v.volume + v.mix + v.rate, 9.0 - 15.0);
    near(v.rate, 9.0 - 150.0 * 0.05);
}

#[test]
fn b1_c_bad_inputs_stop_with_the_reason() {
    let e = volume_rate_mix(&[it("A", -100.0, -5.0, 150.0, 9.0)]).unwrap_err().to_string();
    assert!(e.contains("negative balance") && e.contains("Reverse sign"), "{e}");
    let e = volume_rate_mix(&[it("A", 0.0, 5.0, 150.0, 9.0)]).unwrap_err().to_string();
    assert!(e.contains("no balance"), "{e}");
    let e = volume_rate_mix(&[it("A", 0.0, 0.0, 150.0, 9.0)]).unwrap_err().to_string();
    assert!(e.contains("no balance in the earlier period"), "{e}");
}

fn month(rows: &[(&str, f64, f64)]) -> LazyFrame {
    DataFrame::new(vec![
        Series::new("Product".into(), rows.iter().map(|r| r.0).collect::<Vec<_>>()).into(),
        Series::new("Bal".into(), rows.iter().map(|r| r.1).collect::<Vec<_>>()).into(),
        Series::new("Rate".into(), rows.iter().map(|r| r.2).collect::<Vec<_>>()).into(),
    ])
    .expect("month")
    .lazy()
}

#[test]
fn b1_d_the_mom_bridge_table_and_the_balance_bridge() {
    let spec = MomSpec { key_cols: vec!["Product".into()], bal_col: "Bal".into(), rate_col: Some("Rate".into()), ..Default::default() };
    let (prev, curr) = (month(&[("A", 100.0, 5.0), ("B", 100.0, 10.0)]), month(&[("A", 150.0, 6.0), ("B", 50.0, 8.0)]));
    let (df, _) = build_mom_noted(prev.clone(), curr.clone(), &spec).expect("mom");
    let b = bridge_of(&df, &spec).expect("bridge");
    let steps: Vec<String> = b.column(STEP).unwrap().str().unwrap().into_iter().map(|s| s.unwrap().to_string()).collect();
    assert_eq!(steps, vec!["Prior interest", "Volume", "Mix", "Rate", "Current interest"]);
    let v: Vec<f64> = b.column(INTEREST).unwrap().f64().unwrap().into_iter().map(|x| x.unwrap()).collect();
    // annual interest of 100 at 5% is 5: the same hand-worked numbers
    for (a, e) in v.iter().zip([15.0, 0.0, -2.5, 0.5, 13.0]) {
        near(*a, e);
    }
    // no rate column: the balance bridge, largest changes first
    let spec = MomSpec { rate_col: None, ..spec };
    let (df, _) = build_mom_noted(prev, curr, &spec).expect("mom");
    let b = bridge_of(&df, &spec).expect("bridge");
    let steps: Vec<String> = b.column(STEP).unwrap().str().unwrap().into_iter().map(|s| s.unwrap().to_string()).collect();
    assert_eq!(steps, vec!["Balance before", "A", "B", "Balance now"]);
    let v: Vec<f64> = b.column(BALANCE).unwrap().f64().unwrap().into_iter().map(|x| x.unwrap()).collect();
    assert_eq!(v, vec![200.0, 50.0, -50.0, 200.0]);
}
