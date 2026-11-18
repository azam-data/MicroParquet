//! V5: ΔNII with repricing dates, pass-through (betas) and floors, and the repricing-gap summary.

use crate::common::*;
use micro_parquet::banking::*;
use micro_parquet::betas::*;
use micro_parquet::engine::{blank_balance_notes, compile_tab_lazy};
use micro_parquet::models::*;
use polars::prelude::*;

const CUTOFF: &str = "2026-10-31";

fn spec(shock_bps: f64) -> RateShockSpec {
    RateShockSpec {
        bal_col: "Balance".into(),
        rate_col: "Rate".into(),
        mat_col: "Matdat".into(),
        cutoff_date: CUTOFF.into(),
        shock_bps,
        side_col: Some("Side".into()),
        reprice_col: None,
        beta_file: None,
        product_col: None,
        floor_pct: None,
    }
}

fn nii(df: &DataFrame, s: &RateShockSpec, betas: &[(String, f64)]) -> Vec<Option<f64>> {
    let out = apply_rate_shock_with(df.clone().lazy(), s, betas).unwrap_or_else(|e| panic!("{e:#}")).collect().expect("collect");
    floats(&out, "NII_Impact_BDT")
}

/// A floating loan that reprices in 30 days and matures in 2030, and a savings account.
fn book() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Product".into(), &["FLOAT LOAN", "SAVINGS"]),
        Series::new("Side".into(), &["Asset", "Liability"]),
        Series::new("Balance".into(), &[1_000_000.0, 1_000_000.0]),
        Series::new("Rate".into(), &[12.0, 1.5]),
        dates("Matdat", &["2030-06-30", ""]),
        dates("Reprice", &["2026-11-30", ""]),
    ])
    .expect("book")
}

fn betas(pairs: &[(&str, f64)]) -> Vec<(String, f64)> {
    pairs.iter().map(|(k, v)| (normalise_product(k), v / 100.0)).collect()
}

#[test]
fn v5_a_floating_loan_reprices_on_its_reprice_date_not_its_maturity() {
    let df = book();
    let mut s = spec(100.0);
    // without a repricing date: matures after 12 months, no repricing: 0.00 (what the step gave before)
    let v = nii(&df, &s, &[]);
    assert_eq!(v[0], Some(0.0));
    // with it: reprices in 30 days, weight (365 - 30) / 365, +100 bp on 1,000,000
    s.reprice_col = Some("Reprice".into());
    let v = nii(&df, &s, &[]);
    close(v[0], 9_178.08, 0.005, "floating loan ΔNII");
    close(v[0], 1_000_000.0 * 0.01 * (365.0 - 30.0) / 365.0, 1e-6, "(365 − 30) / 365");
    // the savings account has no maturity and no repricing date: it reprices at once, as before
    close(v[1], -10_000.0, 0.005, "savings, beta 100%");
}

#[test]
fn v5_the_effective_date_is_the_earlier_of_reprice_and_maturity() {
    let df = DataFrame::new(vec![
        Series::new("Side".into(), &["Asset"; 5]),
        Series::new("Balance".into(), &[1_000_000.0; 5]),
        Series::new("Rate".into(), &[10.0; 5]),
        // maturity before the reprice date, reprice before maturity, no reprice date, no maturity, neither
        dates("Matdat", &["2026-11-30", "2030-06-30", "2027-03-31", "", ""]),
        dates("Reprice", &["2027-06-30", "2026-11-30", "", "2026-12-31", ""]),
    ])
    .expect("frame");
    let mut s = spec(100.0);
    s.reprice_col = Some("Reprice".into());
    let v = nii(&df, &s, &[]);
    let w = |days: f64| 1_000_000.0 * 0.01 * (365.0 - days) / 365.0;
    close(v[0], w(30.0), 0.005, "matures 2026-11-30 before it reprices: the maturity");
    close(v[1], w(30.0), 0.005, "reprices 2026-11-30 before it matures: the reprice date");
    close(v[2], w(151.0), 0.005, "no reprice date: the maturity, 2027-03-31");
    close(v[3], w(61.0), 0.005, "no maturity: the reprice date, 2026-12-31");
    close(v[4], 10_000.0, 0.005, "neither: reprices at once, full weight");
    // a date on or before the cutoff reprices at once
    let df = DataFrame::new(vec![Series::new("Side".into(), &["Asset"]), Series::new("Balance".into(), &[1_000_000.0]), Series::new("Rate".into(), &[10.0]), dates("Matdat", &["2031-01-01"]), dates("Reprice", &["2026-10-15"])]).expect("frame");
    close(nii(&df, &s, &[])[0], 10_000.0, 0.005, "already due");
    // a repricing date that is not a date is refused
    let bad = DataFrame::new(vec![Series::new("Side".into(), &["Asset"]), Series::new("Balance".into(), &[1.0]), Series::new("Rate".into(), &[1.0]), dates("Matdat", &["2031-01-01"]), Series::new("Reprice".into(), &["2026-11-30"])]).expect("frame");
    let e = format!("{:#}", apply_rate_shock_with(bad.lazy(), &s, &[]).err().expect("text date"));
    assert!(e.contains("'Reprice'") && e.contains("not a date"), "{e}");
}

#[test]
fn v5_a_beta_scales_the_shock_and_an_unmapped_product_passes_100_percent_and_is_counted() {
    let df = book();
    let mut s = spec(100.0);
    s.beta_file = Some("deposit_betas".into());
    s.product_col = Some("Product".into());
    // savings, no maturity, beta 40%: -4,000.00
    let b = betas(&[("savings", 40.0)]);
    let v = nii(&df, &s, &b);
    close(v[1], -4_000.0, 0.005, "savings at 40%");
    // the loan's product is not in the file: 100% (matures after 12 months with no repricing date: weight 0)
    assert_eq!(v[0], Some(0.0));
    s.reprice_col = Some("Reprice".into());
    close(nii(&df, &s, &b)[0], 9_178.08, 0.005, "an unmapped product passes the whole shock");
    assert_eq!(count_unmapped_products(df.clone().lazy(), "Product", &b).expect("count"), 1, "FLOAT LOAN is not in the file");
    assert_eq!(count_unmapped_products(df.clone().lazy(), "Product", &betas(&[("savings", 40.0), (" float  loan ", 100.0)])).expect("count"), 0, "codes are matched ignoring case and spaces");
    // a blank product is unmapped too
    let blank = DataFrame::new(vec![Series::new("Product".into(), &[Some("SAVINGS"), None, Some("  ")])]).expect("frame");
    assert_eq!(count_unmapped_products(blank.lazy(), "Product", &b).expect("count"), 2);
    // a beta of 0 means the rate does not move
    close(nii(&df, &s, &betas(&[("savings", 0.0)]))[1], 0.0, 1e-9, "beta 0");
    // the beta file needs its product column
    s.product_col = None;
    let e = format!("{:#}", apply_rate_shock_with(df.clone().lazy(), &s, &b).err().expect("no product column"));
    assert!(e.contains("choose the product column"), "{e}");
}

#[test]
fn v5_a_floor_limits_a_fall_in_the_rate_and_the_change_is_cut_to_what_fits() {
    let df = book();
    let b = betas(&[("savings", 100.0)]);
    let mut s = spec(-200.0);
    // savings at 1.50% falling 2%: without a floor +20,000.00 (cost falls by 20,000)
    close(nii(&df, &s, &[])[1], 20_000.0, 0.005, "no floor");
    s.floor_pct = Some(0.0);
    close(nii(&df, &s, &[])[1], 15_000.0, 0.005, "floor 0: the rate falls by 1.50% only");
    s.beta_file = Some("b".into());
    s.product_col = Some("Product".into());
    close(nii(&df, &s, &b)[1], 15_000.0, 0.005, "with a beta of 100% and a floor of 0");
    // a beta of 50% makes the fall 1%, which fits above the floor: not cut
    close(nii(&df, &s, &betas(&[("savings", 50.0)]))[1], 10_000.0, 0.005, "beta 50%: -1% fits");
    // a floor above the new rate cuts to the floor: 1.50 - 2 = -0.5 would be under 1.00, the fall is 0.50
    s.floor_pct = Some(1.0);
    close(nii(&df, &s, &b)[1], 5_000.0, 0.005, "floor 1%: the fall is 0.5%");
    // a rise is never touched by a floor, and a rate already under its floor does not fall further
    s.shock_bps = 200.0;
    close(nii(&df, &s, &b)[1], -20_000.0, 0.005, "a rise passes through");
    s.shock_bps = -200.0;
    s.floor_pct = Some(2.0);
    close(nii(&df, &s, &b)[1], 0.0, 1e-9, "the rate is already under the floor: no change");
    // the shocked interest column follows: current + the change
    let mut s2 = spec(-200.0);
    s2.floor_pct = Some(0.0);
    let out = apply_rate_shock_with(df.lazy(), &s2, &[]).expect("runs").collect().expect("collect");
    close(floats(&out, "Shocked_Annual_Int")[1], 15_000.0 - 15_000.0, 1e-6, "interest 15,000 less the 15,000 fall");
    // an asset has a floor too: a loan at 12% falling 2% with a floor of 11% loses 1%
    let mut s3 = spec(-200.0);
    s3.floor_pct = Some(11.0);
    s3.reprice_col = Some("Reprice".into());
    let v = nii(&book(), &s3, &[]);
    close(v[0], -1_000_000.0 * 0.01 * (365.0 - 30.0) / 365.0, 0.005, "the loan's fall is cut from 2% to 1%");
}

#[test]
fn v5_the_beta_file_is_read_and_checked() {
    let d = dir("v5_betas");
    let write = |name: &str, body: &str| std::fs::write(d.join(format!("{name}.json")), body).expect("write");
    write("ok", r#"{"source":"ALCO test","betas":[{"product":"SAVINGS","beta_pct":40},{"product":"Current  Deposit","beta_pct":0},{"product":"FDR","beta_pct":100}]}"#);
    let b = load_beta_file(&d, "ok").expect("loads");
    assert_eq!(b, vec![("SAVINGS".to_string(), 0.4), ("CURRENT DEPOSIT".to_string(), 0.0), ("FDR".to_string(), 1.0)]);
    assert_eq!(list_beta_files(&d), vec!["ok".to_string()]);
    write("high", r#"{"betas":[{"product":"X","beta_pct":120}]}"#);
    assert!(format!("{:#}", load_beta_file(&d, "high").err().expect("over 100")).contains("not a percentage from 0 to 100"));
    write("neg", r#"{"betas":[{"product":"X","beta_pct":-5}]}"#);
    assert!(format!("{:#}", load_beta_file(&d, "neg").err().expect("negative")).contains("'X'"));
    write("dup", r#"{"betas":[{"product":"Savings","beta_pct":40},{"product":" SAVINGS ","beta_pct":50}]}"#);
    assert!(format!("{:#}", load_beta_file(&d, "dup").err().expect("twice")).contains("lists 'SAVINGS' twice") || format!("{:#}", load_beta_file(&d, "dup").err().expect("twice")).contains("twice"));
    write("empty", r#"{"betas":[]}"#);
    assert!(format!("{:#}", load_beta_file(&d, "empty").err().expect("empty")).contains("lists no products"));
    write("bad", "nope");
    assert!(format!("{:#}", load_beta_file(&d, "bad").err().expect("bad json")).contains("is not valid"));
    let e = format!("{:#}", load_beta_file(&d, "missing").err().expect("no such file"));
    assert!(e.contains("'missing' was not found"), "{e}");
    for name in ["", "../x", "a/b", "a\\b"] {
        assert!(format!("{:#}", load_beta_file(&d, name).err().expect("a path")).contains("is not a beta file name"), "{name:?}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

fn step(reprice: bool, beta: Option<&str>, floor: Option<f64>, bps: f64) -> TransformStep {
    TransformStep::RateShockSim {
        bal_col: "Balance".into(),
        rate_col: "Rate".into(),
        mat_col: "Matdat".into(),
        cutoff_date: CUTOFF.into(),
        shock_bps: bps,
        side_col: Some("Side".into()),
        reprice_col: reprice.then(|| "Reprice".to_string()),
        beta_file: beta.map(str::to_string),
        product_col: beta.map(|_| "Product".to_string()),
        floor_pct: floor,
    }
}

#[test]
fn v5_the_step_reads_the_beta_file_says_how_many_products_it_lacked_and_old_steps_replay_to_what_main_gave() {
    let d = dir("v5_step");
    std::fs::write(d.join("dep.json"), r#"{"source":"t","betas":[{"product":"SAVINGS","beta_pct":40}]}"#).expect("write");
    set_betas_dir_override(Some(d.clone()));
    let src = parquet(&d, "book", &book());
    // the step compiles with the file and gives -4,000 for the savings account
    let out = compile_tab_lazy(&src, &[step(true, Some("dep"), None, 100.0)], &d).expect("compiles").collect().expect("collect");
    close(floats(&out, "NII_Impact_BDT")[0], 9_178.08, 0.005, "loan");
    close(floats(&out, "NII_Impact_BDT")[1], -4_000.0, 0.005, "savings");
    // the status note counts the products the file did not list
    let notes = blank_balance_notes(&src, &[step(true, Some("dep"), None, 100.0)], &d).expect("notes");
    assert!(notes.iter().any(|n| n.contains("1 row(s)") && n.contains("not in the beta file 'dep'") && n.contains("100%")), "{notes:?}");
    // a missing file stops the step, naming it
    let e = format!("{:#}", compile_tab_lazy(&src, &[step(true, Some("nope"), None, 100.0)], &d).and_then(|lf| Ok(lf.collect()?)).err().expect("no file"));
    assert!(e.contains("'nope' was not found"), "{e}");
    set_betas_dir_override(None);
    // old macros: no new field in the JSON, and today's output is exactly what main's code wrote
    assert_fixture_replays("phase4_rate_shock_main", &["RateShockSim"]);
    assert_fixture_replays("phase4_rate_shock_side_main", &["RateShockSim"]);
    let old: TransformStep = serde_json::from_str(r#"{"RateShockSim":{"bal_col":"Balance","rate_col":"Rate","mat_col":"Matdat","cutoff_date":"2026-10-31","shock_bps":100.0}}"#).expect("a Phase 1 step loads");
    assert!(matches!(old, TransformStep::RateShockSim { reprice_col: None, beta_file: None, product_col: None, floor_pct: None, side_col: None, .. }));
    // the new fields survive a macro file
    let recipe = MacroRecipe { name: "n".into(), description: String::new(), created_at: String::new(), enforce_balance_control: true, control_column: None, optional_ingestion_schema: None, steps: vec![step(true, Some("dep"), Some(0.0), -200.0)], summary: Default::default() };
    let p = d.join("m.mpq.json");
    micro_parquet::macros::save_macro_to_file(&recipe, &p).expect("save");
    let back = micro_parquet::macros::load_macro_from_file(&p).expect("load");
    assert!(matches!(&back.steps[0], TransformStep::RateShockSim { reprice_col: Some(r), beta_file: Some(b), floor_pct: Some(f), .. } if r == "Reprice" && b == "dep" && *f == 0.0));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn v5_the_repricing_gap_summary_has_five_buckets_a_total_and_the_delta_nii_of_the_step() {
    let df = DataFrame::new(vec![
        Series::new("Side".into(), &["Asset", "Asset", "Liability", "Liability", "Asset", "Liability"]),
        Series::new("Balance".into(), &[100.0, 200.0, 50.0, 80.0, 300.0, 40.0]),
        Series::new("Rate".into(), &[10.0; 6]),
        // maturities: 15 days, 76 days, 135 days, 274 days, 2030, none
        dates("Matdat", &["2026-11-15", "2027-01-15", "2027-03-15", "2027-08-01", "2030-01-01", ""]),
    ])
    .expect("frame");
    let g = build_repricing_gap(df.clone().lazy(), &spec(100.0), &[]).expect("the summary builds");
    assert_eq!(
        strings(&g, "Repricing bucket").into_iter().flatten().collect::<Vec<_>>(),
        vec!["0\u{2013}1m", "1\u{2013}3m", "3\u{2013}6m", "6\u{2013}12m", ">12m", "Total", "1 row with no maturity or repricing date placed in 0\u{2013}1m (contractual)"]
    );
    assert_eq!(g.get_column_names().iter().map(|s| s.to_string()).collect::<Vec<_>>(), vec!["Repricing bucket", "Assets", "Liabilities", "Gap", "Cumulative gap", "\u{394}NII (BDT)"]);
    // assets positive, liabilities negative, gap = assets + liabilities; the blank maturity reprices at once
    assert_eq!(floats(&g, "Assets"), vec![Some(100.0), Some(200.0), Some(0.0), Some(0.0), Some(300.0), Some(600.0), None]);
    assert_eq!(floats(&g, "Liabilities"), vec![Some(-40.0), Some(0.0), Some(-50.0), Some(-80.0), Some(0.0), Some(-170.0), None]);
    assert_eq!(floats(&g, "Gap"), vec![Some(60.0), Some(200.0), Some(-50.0), Some(-80.0), Some(300.0), Some(430.0), None]);
    assert_eq!(floats(&g, "Cumulative gap"), vec![Some(60.0), Some(260.0), Some(210.0), Some(130.0), Some(430.0), None, None]);
    // the ΔNII of the step, bucket by bucket and in total
    let w = |d: f64| (365.0 - d) / 365.0;
    let want = [100.0 * w(15.0) * 0.01 - 40.0 * 0.01, 200.0 * w(76.0) * 0.01, -50.0 * w(135.0) * 0.01, -80.0 * w(274.0) * 0.01, 0.0];
    let got = floats(&g, "\u{394}NII (BDT)");
    for i in 0..5 {
        close(got[i], want[i], 1e-9, &format!("bucket {i}"));
    }
    close(got[5], want.iter().sum(), 1e-9, "total ΔNII");
    // the total is the sum of NII_Impact_BDT of the step on the same rows
    let detail = apply_rate_shock_with(df.clone().lazy(), &spec(100.0), &[]).expect("runs").collect().expect("collect");
    close(got[5], floats(&detail, "NII_Impact_BDT").iter().flatten().sum(), 1e-9, "the same total as the detail");
    // with a repricing date, a beta and a floor the buckets move with the step
    let mut s = spec(-100.0);
    s.floor_pct = Some(9.5);
    let g2 = build_repricing_gap(df.clone().lazy(), &s, &[]).expect("builds");
    close(floats(&g2, "\u{394}NII (BDT)")[1], 200.0 * w(76.0) * -0.005, 1e-9, "the fall is cut to 0.5%");
    // a text balance is refused
    let bad = DataFrame::new(vec![Series::new("Side".into(), &["Asset"]), Series::new("Balance".into(), &["1"]), Series::new("Rate".into(), &[1.0]), dates("Matdat", &["2027-01-01"])]).expect("frame");
    let e = format!("{:#}", build_repricing_gap(bad.lazy(), &spec(100.0), &[]).err().expect("text balance"));
    assert!(e.contains("not a number"), "{e}");
}
