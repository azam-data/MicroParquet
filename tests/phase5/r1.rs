//! Phase 5 R1: the cash-flow engine and the IRRBB cash-flow ladder.

use crate::common::*;
use chrono::NaiveDate;
use micro_parquet::banking::GapRole;
use micro_parquet::behaviour::{BehaviourFile, BehaviourProduct};
use micro_parquet::cashflow::*;
use polars::prelude::*;

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("a test date")
}

fn settings(mode: CashFlowMode, scenario: Scenario) -> CfSettings {
    CfSettings { cutoff: d("2026-10-31"), mode, scenario }
}

fn row(side: Side, principal: f64, rate: f64, maturity: &str) -> CfRow {
    CfRow { rate_pct: rate, ..CfRow::simple(side, principal, d(maturity)) }
}

fn close_to(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} vs {b}");
}

fn beh(f: impl FnOnce(&mut BehaviourProduct)) -> BehaviourProduct {
    let mut b = BehaviourProduct { product: "P".into(), ..BehaviourProduct::default() };
    f(&mut b);
    b
}

#[test]
fn r1_the_buckets_are_the_19_of_bb_table_3() {
    assert_eq!(IRRBB_BUCKETS.len(), 19);
    let mids: Vec<f64> = IRRBB_BUCKETS.iter().map(|b| b.midpoint).collect();
    assert_eq!(mids, vec![0.0028, 0.0417, 0.1667, 0.375, 0.625, 0.875, 1.25, 1.75, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 9.5, 12.5, 17.5, 25.0]);
    // each bucket includes its upper bound; t <= 0 is O/N
    assert_eq!(bucket_index(-3.0), 0);
    assert_eq!(bucket_index(0.0), 0);
    assert_eq!(bucket_index(1.0 / 365.0), 0, "one day is still O/N");
    assert_eq!(bucket_index(2.0 / 365.0), 1);
    assert_eq!(bucket_index(1.0), 5);
    assert_eq!(bucket_index(1.0 + 1.0 / 365.0), 6);
    assert_eq!(bucket_index(20.0), 17);
    assert_eq!(bucket_index(20.01), 18);
    assert_eq!(bucket_index(80.0), 18);
}

#[test]
fn r1_annuity_120000_at_12_percent_monthly_12_payments() {
    let mut r = row(Side::Asset, 120_000.0, 12.0, "2027-10-31");
    r.repayment = Repayment::EqualInstalment;
    r.frequency = Frequency::Monthly;
    let rf = generate_flows(&r, &settings(CashFlowMode::PrincipalAndInterest, Scenario::Base), None).expect("flows");
    assert_eq!(rf.flows.len(), 12);
    assert_eq!(rf.flows[0].date, d("2026-11-30"));
    assert_eq!(rf.flows[11].date, d("2027-10-31"));
    close_to(rf.flows[0].total(), 10_661.85, 0.005, "the instalment");
    close_to(rf.flows[0].interest, 1_200.00, 0.005, "the first period's interest");
    for f in &rf.flows {
        close_to(f.total(), rf.flows[0].total(), 1e-6, "every instalment is the same");
    }
    close_to(rf.flows.iter().map(|f| f.principal).sum::<f64>(), 120_000.0, 0.01, "the principal adds up");
    // principal only: just the principal parts
    let po = generate_flows(&r, &settings(CashFlowMode::PrincipalOnly, Scenario::Base), None).expect("flows");
    assert!(po.flows.iter().all(|f| f.interest == 0.0));
    close_to(po.flows[0].total(), 10_661.85 - 1_200.0, 0.005, "the first principal part");
}

#[test]
fn r1_equal_principal_pays_a_twelfth_of_the_principal_each_month() {
    let mut r = row(Side::Asset, 120_000.0, 12.0, "2027-10-31");
    r.repayment = Repayment::EqualPrincipal;
    r.frequency = Frequency::Monthly;
    let rf = generate_flows(&r, &settings(CashFlowMode::PrincipalAndInterest, Scenario::Base), None).expect("flows");
    for (k, f) in rf.flows.iter().enumerate() {
        close_to(f.principal, 10_000.0, 1e-6, "principal");
        close_to(f.interest, (120_000.0 - 10_000.0 * k as f64) * 0.01, 1e-6, "interest on the balance");
    }
}

#[test]
fn r1_bullet_pays_interest_each_period_and_the_principal_at_the_end() {
    let mut r = row(Side::Liability, 1_000_000.0, 8.0, "2027-10-31");
    r.frequency = Frequency::Quarterly;
    let rf = generate_flows(&r, &settings(CashFlowMode::PrincipalAndInterest, Scenario::Base), None).expect("flows");
    assert_eq!(rf.flows.len(), 4);
    assert!(rf.flows.iter().all(|f| (f.interest - 20_000.0).abs() < 1e-6));
    assert_eq!(rf.flows.iter().map(|f| f.principal).collect::<Vec<_>>(), vec![0.0, 0.0, 0.0, 1_000_000.0]);
    // at maturity: one flow with interest for the time left
    let mut r = row(Side::Liability, 1_000_000.0, 8.0, "2027-10-31");
    r.frequency = Frequency::AtMaturity;
    let rf = generate_flows(&r, &settings(CashFlowMode::PrincipalAndInterest, Scenario::Base), None).expect("flows");
    assert_eq!(rf.flows.len(), 1);
    close_to(rf.flows[0].interest, 1_000_000.0 * 0.08 * 365.0 / 365.0, 1e-6, "one year of interest");
}

#[test]
fn r1_slotting_matured_liability_is_in_o_n_and_matured_asset_is_left_out_with_a_note() {
    let s = settings(CashFlowMode::PrincipalOnly, Scenario::Base);
    // bullet asset maturing 2031-04-30: bucket (4, 5], midpoint 4.5
    let rows = vec![
        row(Side::Asset, 1_000_000.0, 0.0, "2031-04-30"),
        row(Side::Liability, 300_000.0, 0.0, "2026-09-30"),
        row(Side::Asset, 250_000.0, 0.0, "2026-10-31"),
    ];
    let res = ladder_from_rows(&rows, &s, None).expect("ladder");
    let b = res.for_currency("BDT").expect("BDT");
    assert_eq!(IRRBB_BUCKETS[10].midpoint, 4.5);
    assert_eq!(b.assets[10], 1_000_000.0, "the asset is in bucket 4–5Y");
    assert_eq!(b.assets.iter().sum::<f64>(), 1_000_000.0, "and nowhere else");
    assert_eq!(b.liabilities[0], -300_000.0, "a matured liability is in O/N, negative");
    assert_eq!(res.excluded, vec![("BDT".to_string(), 1, 250_000.0)]);
    assert_eq!(res.excluded_count(), 1);
    assert!(res.notes.iter().any(|n| n.contains("1 matured asset row(s) with a total of 250000.00")), "{:?}", res.notes);
    // the table: 19 buckets and the excluded row
    let df = ladder_frame(&res).expect("frame");
    assert_eq!(df.height(), 20);
    assert_eq!(strings(&df, "Bucket")[19].as_deref(), Some("Matured assets, not in ladder (1)"));
    assert_eq!(floats(&df, "Assets")[19], Some(250_000.0));
    assert_eq!(floats(&df, "Net")[10], Some(1_000_000.0));
    assert_eq!(floats(&df, "Cumulative net")[18], Some(700_000.0), "net of the liability and the asset");
    assert_eq!(floats(&df, "Midpoint (years)")[10], Some(4.5));
}

#[test]
fn r1_floating_loan_has_one_flow_on_the_repricing_date() {
    let mut r = row(Side::Asset, 1_000_000.0, 10.0, "2030-06-30");
    r.rate_type = RateType::Floating;
    r.reprice = Some(d("2026-11-30"));
    r.frequency = Frequency::Quarterly;
    let rf = generate_flows(&r, &settings(CashFlowMode::PrincipalAndInterest, Scenario::Base), None).expect("flows");
    assert_eq!(rf.flows.len(), 1);
    assert_eq!(rf.flows[0].date, d("2026-11-30"));
    close_to(rf.flows[0].principal, 1_000_000.0, 1e-9, "principal");
    close_to(rf.flows[0].interest, 25_000.0, 1e-9, "one quarter's interest");
    close_to(rf.flows[0].total(), 1_025_000.0, 1e-9, "total");
    // it lands in the bucket of the repricing date (30 days: 'O/N–1M')
    let res = ladder_from_rows(&[r.clone()], &settings(CashFlowMode::PrincipalAndInterest, Scenario::Base), None).expect("ladder");
    assert_eq!(res.for_currency("BDT").expect("BDT").assets[1], 1_025_000.0);
    // a repricing date after the maturity: the maturity counts
    r.reprice = Some(d("2031-01-01"));
    let rf = generate_flows(&r, &settings(CashFlowMode::PrincipalOnly, Scenario::Base), None).expect("flows");
    assert_eq!(rf.flows[0].date, d("2030-06-30"));
}

#[test]
fn r1_term_deposit_early_redemption_goes_to_o_n_and_follows_the_scenario() {
    let td = beh(|b| b.tdrr_pct = 10.0);
    let r = row(Side::Liability, 1_000_000.0, 0.0, "2027-10-31");
    for (sc, on, at_maturity) in [
        (Scenario::Base, 100_000.0, 900_000.0),
        (Scenario::ParallelUp, 120_000.0, 880_000.0),
        (Scenario::ParallelDown, 80_000.0, 920_000.0),
    ] {
        let rf = generate_flows(&r, &settings(CashFlowMode::PrincipalOnly, sc), Some(&td)).expect("flows");
        assert_eq!(rf.flows.len(), 2, "{sc:?}");
        close_to(rf.flows[0].principal, on, 1e-6, "O/N part");
        assert_eq!(rf.flows[0].date, d("2026-10-31"));
        close_to(rf.flows[1].principal, at_maturity, 1e-6, "at maturity");
        assert_eq!(rf.flows[1].date, d("2027-10-31"));
    }
    // the multipliers of the guideline
    for sc in [Scenario::ParallelUp, Scenario::Flattener, Scenario::ShortUp] {
        assert_eq!(sc.tdrr_multiplier(), 1.2);
    }
    for sc in [Scenario::ParallelDown, Scenario::Steepener, Scenario::ShortDown] {
        assert_eq!(sc.tdrr_multiplier(), 0.8);
    }
    for sc in [Scenario::ParallelUp, Scenario::Steepener, Scenario::ShortUp] {
        assert_eq!(sc.cpr_multiplier(), 0.8);
    }
    for sc in [Scenario::ParallelDown, Scenario::Flattener, Scenario::ShortDown] {
        assert_eq!(sc.cpr_multiplier(), 1.2);
    }
}

#[test]
fn r1_loan_prepayment_cpr_10_percent_on_a_bullet_loan() {
    let loan = beh(|b| b.cpr_pct = 10.0);
    let mut r = row(Side::Asset, 1_000_000.0, 0.0, "2028-10-31");
    r.frequency = Frequency::Annual;
    for (sc, y1, y2) in [(Scenario::Base, 100_000.0, 900_000.0), (Scenario::ParallelUp, 80_000.0, 920_000.0), (Scenario::ParallelDown, 120_000.0, 880_000.0)] {
        let rf = generate_flows(&r, &settings(CashFlowMode::PrincipalOnly, sc), Some(&loan)).expect("flows");
        assert_eq!(rf.flows.len(), 2);
        assert_eq!(rf.flows[0].date, d("2027-10-31"));
        close_to(rf.flows[0].principal, y1, 1e-6, "year 1");
        close_to(rf.flows[1].principal, y2, 1e-6, "year 2");
    }
    // a monthly loan: the per-period fraction is 1 - (1 - CPR)^(1/12) of the balance
    let mut m = row(Side::Asset, 1_000_000.0, 0.0, "2027-01-31");
    m.frequency = Frequency::Monthly;
    let rf = generate_flows(&m, &settings(CashFlowMode::PrincipalOnly, Scenario::Base), Some(&loan)).expect("flows");
    close_to(rf.flows[0].principal, 1_000_000.0 * (1.0 - 0.9f64.powf(1.0 / 12.0)), 1e-6, "monthly prepayment");
    close_to(rf.flows.iter().map(|f| f.principal).sum::<f64>(), 1_000_000.0, 1e-6, "everything is paid");
}

#[test]
fn r1_non_maturity_deposit_core_at_its_average_maturity_and_the_rest_in_o_n() {
    let nmd = BehaviourProduct { product: "SAV".into(), category: "Retail non-transactional".into(), core_pct: 60.0, avg_maturity_years: 3.0, method: "Typed".into(), ..BehaviourProduct::default() };
    let file = BehaviourFile { as_of: "2026-10-31".into(), source: "test".into(), products: vec![nmd] };
    let mut r = row(Side::Liability, 1_000_000.0, 4.0, "2026-10-31");
    r.product = " sav ".into();
    r.maturity = None; // no maturity date is needed
    let res = ladder_from_rows(&[r], &settings(CashFlowMode::PrincipalAndInterest, Scenario::Base), Some(&file)).expect("ladder");
    let b = res.for_currency("BDT").expect("BDT");
    assert_eq!(b.liabilities[0], -400_000.0, "non-core in O/N");
    assert_eq!(IRRBB_BUCKETS[8].upper_years, 3.0);
    assert_eq!(b.liabilities[8], -600_000.0, "core in the bucket that holds 3 years (2–3Y)");
    assert!(res.notes.iter().any(|n| n.contains("1 non-maturity deposit row(s)")), "{:?}", res.notes);
}

#[test]
fn r1_each_currency_has_its_own_ladder() {
    let mut a = row(Side::Asset, 100.0, 0.0, "2027-10-31");
    a.currency = "USD".into();
    let b = row(Side::Asset, 50.0, 0.0, "2027-10-31");
    let res = ladder_from_rows(&[a, b], &settings(CashFlowMode::PrincipalOnly, Scenario::Base), None).expect("ladder");
    assert_eq!(res.currencies.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), vec!["BDT", "USD"], "BDT first");
    let df = ladder_frame(&res).expect("frame");
    assert_eq!(df.height(), 38);
    assert_eq!(floats(&df, "Assets").iter().flatten().sum::<f64>(), 150.0);
}

fn tab_frame() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Account_No".into(), &["A1", "A2", "A3", "A4"]),
        Series::new("Side".into(), &["Asset", "Asset", "Liability", "Liability"]),
        Series::new("Outstanding".into(), &[120_000.0, 1_000_000.0, 500_000.0, 700_000.0]),
        Series::new("Rate".into(), &[12.0, 10.0, 6.0, 5.0]),
        dates("Matdat", &["2027-10-31", "2030-06-30", "2026-12-31", "2026-09-30"]),
        dates("Reprice", &["", "2026-11-30", "", ""]),
        Series::new("Type".into(), &["Fixed", "Floating", "Fixed", "Fixed"]),
        Series::new("Repay".into(), &["Equal instalment", "Bullet", "Bullet", "Bullet"]),
        Series::new("Freq".into(), &["Monthly", "Quarterly", "At maturity", "At maturity"]),
        Series::new("Product".into(), &["HL", "CC", "TD", "TD"]),
    ])
    .expect("tab")
}

fn spec() -> CfInputSpec {
    CfInputSpec {
        role: GapRole::Side("Side".into()),
        principal_col: "Outstanding".into(),
        rate_col: Some("Rate".into()),
        rate_type_col: Some("Type".into()),
        default_rate_type: RateType::Fixed,
        maturity_col: "Matdat".into(),
        reprice_col: Some("Reprice".into()),
        repay_col: Some("Repay".into()),
        default_repayment: Repayment::Bullet,
        freq_col: Some("Freq".into()),
        default_frequency: Frequency::AtMaturity,
        product_col: Some("Product".into()),
        currency_col: None,
        default_currency: "BDT".into(),
    }
}

#[test]
fn r1_a_tab_is_read_with_its_roles_and_gives_the_ladder() {
    let s = settings(CashFlowMode::PrincipalAndInterest, Scenario::Base);
    let res = build_ladder(vec![(tab_frame().lazy(), spec())], &s, None).expect("ladder");
    let b = res.for_currency("BDT").expect("BDT");
    // A1: 12 instalments of 10,661.85; the first in 30 days ('O/N–1M'), the rest later
    let total_assets: f64 = b.assets.iter().sum();
    close_to(total_assets, 12.0 * 10_661.851_5 + 1_025_000.0, 0.5, "A1 instalments plus A2's reprice flow");
    // A4 matured: its principal is in O/N; A3 matures in 61 days with 6% on its principal at maturity
    close_to(b.liabilities[0], -700_000.0, 1e-6, "matured liability in O/N");
    let a3 = 500_000.0 * (1.0 + 0.06 * 61.0 / 365.0);
    close_to(b.liabilities[2], -a3, 1e-6, "A3 at maturity, 61 days: bucket 1–3M");
    assert!(res.excluded.is_empty());
}

#[test]
fn r1_bad_values_stop_with_the_count_and_the_first_five_accounts() {
    let s = settings(CashFlowMode::PrincipalAndInterest, Scenario::Base);
    // a blank maturity on 7 rows
    let n = 7;
    let accts: Vec<String> = (1..=n).map(|i| format!("B{i}")).collect();
    let df = DataFrame::new(vec![
        Series::new("Account_No".into(), accts),
        Series::new("Outstanding".into(), vec![100.0; n]),
        Series::new("Rate".into(), vec![5.0; n]),
        dates("Matdat", &vec![""; n]),
    ])
    .expect("tab");
    let sp = CfInputSpec { role: GapRole::Asset, rate_type_col: None, reprice_col: None, repay_col: None, freq_col: None, product_col: None, ..spec() };
    let e = format!("{:#}", build_ladder(vec![(df.lazy(), sp.clone())], &s, None).err().expect("blank maturities"));
    assert!(e.contains("the maturity date is blank on 7 row(s)"), "{e}");
    assert!(e.contains("the first 5 values of Account_No: B1, B2, B3, B4, B5."), "{e}");
    assert!(!e.contains("B6"), "{e}");

    // unknown text values, a negative amount, a floating row without a repricing date, a missing rate
    let df = DataFrame::new(vec![
        Series::new("Account_No".into(), &["C1", "C2", "C3", "C4"]),
        Series::new("Outstanding".into(), &[100.0, -5.0, 100.0, 100.0]),
        Series::new("Rate".into(), &[Some(5.0), Some(5.0), Some(5.0), None]),
        dates("Matdat", &["2028-01-01"; 4]),
        Series::new("Type".into(), &["Floating", "Fixed", "Fixed", "Fixed"]),
        Series::new("Freq".into(), &["Monthly", "Monthly", "Fortnightly", "Monthly"]),
    ])
    .expect("tab");
    let sp = CfInputSpec { rate_type_col: Some("Type".into()), freq_col: Some("Freq".into()), ..sp };
    let e = format!("{:#}", build_ladder(vec![(df.clone().lazy(), sp.clone())], &s, None).err().expect("bad frequency"));
    assert!(e.contains("the frequency 'Fortnightly' is not Monthly") && e.contains("C3"), "{e}");
    assert!(e.contains("negative") && e.contains("C2"), "{e}");
    // with those rows out, the other two problems show
    let ok = df.slice(0, 1).vstack(&df.slice(3, 1)).expect("rows");
    let e = format!("{:#}", build_ladder(vec![(ok.lazy(), sp.clone())], &s, None).err().expect("floating and rate"));
    assert!(e.contains("a floating row has no next repricing date on 1 row(s)") || e.contains("floating"), "{e}");
    assert!(e.contains("the rate is blank or not a number") && e.contains("C4"), "{e}");
    // principal-only mode needs no rate
    let po = settings(CashFlowMode::PrincipalOnly, Scenario::Base);
    let only_rate = DataFrame::new(vec![
        Series::new("Account_No".into(), &["D1"]),
        Series::new("Outstanding".into(), &[100.0]),
        Series::new("Rate".into(), &[None::<f64>]),
        dates("Matdat", &["2028-01-01"]),
    ])
    .expect("tab");
    let sp2 = CfInputSpec { rate_type_col: None, freq_col: None, ..sp };
    build_ladder(vec![(only_rate.clone().lazy(), sp2.clone())], &po, None).expect("principal only needs no rate");
    // the interest mode with no rate column at all
    let sp3 = CfInputSpec { rate_col: None, ..sp2 };
    let e = format!("{:#}", build_ladder(vec![(only_rate.lazy(), sp3)], &s, None).err().expect("no rate column"));
    assert!(e.contains("choose the rate column"), "{e}");
}

#[test]
fn r1_the_behaviour_profile_needs_the_product_column() {
    let s = settings(CashFlowMode::PrincipalOnly, Scenario::Base);
    let file = BehaviourFile { as_of: "2026-10-31".into(), source: "t".into(), products: vec![] };
    let sp = CfInputSpec { product_col: None, ..spec() };
    let e = format!("{:#}", build_ladder(vec![(tab_frame().lazy(), sp)], &s, Some(&file)).err().expect("product column"));
    assert!(e.contains("choose the product column"), "{e}");
}
