//! Phase 5 R3: IRRBB, six scenarios, ΔEVE and ΔNII.

use crate::common::*;
use chrono::NaiveDate;
use micro_parquet::banking::GapRole;
use micro_parquet::behaviour::BehaviourFile;
use micro_parquet::betas::BetaRow;
use micro_parquet::cashflow::*;
use micro_parquet::irrbb::*;
use micro_parquet::models::FtpPoint;
use micro_parquet::rules::{load_shocks, set_data_dir_override, CurrencyShock, RuleFile, RuleMeta, ShockFile};
use micro_parquet::curves::YieldCurve;
use polars::prelude::*;

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("a test date")
}

fn bdt() -> CurrencyShock {
    CurrencyShock { currency: "BDT".into(), parallel_bp: 400.0, short_bp: 500.0, long_bp: 300.0 }
}

fn flat(currency: &str, pct: f64) -> YieldCurve {
    YieldCurve { date: d("2026-10-01"), currency: currency.into(), source: "test".into(), points: vec![FtpPoint { tenor_days: 30, ftp_rate: pct }, FtpPoint { tenor_days: 9000, ftp_rate: pct }] }
}

fn shock_file(extra: Vec<CurrencyShock>) -> RuleFile<ShockFile> {
    let mut currencies = vec![bdt()];
    currencies.extend(extra);
    RuleFile { data: ShockFile { decay_x: 4.0, outlier_pct: 15.0, currencies }, meta: RuleMeta { file: "irrbb_shocks/2021-04-13.json".into(), as_of: d("2021-04-13"), source: "BRPD IRRBB 2021".into(), sample: false } }
}

fn spec(tier1: f64, comp: Compounding) -> IrrbbSpec {
    IrrbbSpec { cutoff: d("2026-10-31"), mode: CashFlowMode::PrincipalOnly, tier1, compounding: comp, post_shock_floor_pct: None, nii_floor_pct: None, shocks: shock_file(vec![]), curves: vec![flat("BDT", 10.0)], behaviour: None, betas: vec![] }
}

/// The acceptance book: an asset flow of 1,000,000 in (4, 5] and a liability flow of −600,000 in O/N.
fn acceptance_rows() -> Vec<CfRow> {
    vec![CfRow::simple(Side::Asset, 1_000_000.0, d("2031-04-30")), CfRow::simple(Side::Liability, 600_000.0, d("2026-10-31"))]
}

fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} vs {b}");
}

#[test]
fn r3_the_shocks_at_t_3_5_and_4_5() {
    let cs = bdt();
    close(shock_bp(Scenario::ShortUp, 3.5, &cs, 4.0), 208.4310, 0.0001, "short up at 3.5");
    close(shock_bp(Scenario::Steepener, 3.5, &cs, 4.0), 21.9671, 0.0001, "steepener at 3.5");
    close(shock_bp(Scenario::Flattener, 3.5, &cs, 4.0), 61.7800, 0.0001, "flattener at 3.5");
    close(shock_bp(Scenario::Steepener, 4.5, &cs, 4.0), 76.8318, 0.0001, "steepener at 4.5");
    close(shock_bp(Scenario::Flattener, 4.5, &cs, 4.0), 8.2984, 0.0001, "flattener at 4.5");
    // the parallel and the short shocks, and the base case
    assert_eq!(shock_bp(Scenario::ParallelUp, 9.0, &cs, 4.0), 400.0);
    assert_eq!(shock_bp(Scenario::ParallelDown, 9.0, &cs, 4.0), -400.0);
    close(shock_bp(Scenario::ShortDown, 0.0, &cs, 4.0), -500.0, 1e-9, "short down at 0");
    assert_eq!(shock_bp(Scenario::Base, 3.0, &cs, 4.0), 0.0);
}

#[test]
fn r3_base_eve_and_the_six_eve_changes() {
    let o = run_rows(&acceptance_rows(), &spec(1_000_000.0, Compounding::Continuous)).expect("runs");
    close(o.base_eve("BDT").expect("base"), 37_796.13, 0.05, "base EVE");
    let got = |s: Scenario| o.scenarios.iter().find(|x| x.scenario == s).expect("scenario").eve_change;
    for (s, want) in [
        (Scenario::ParallelUp, -104_969.17),
        (Scenario::ParallelDown, 125_684.16),
        (Scenario::Steepener, -21_723.32),
        (Scenario::Flattener, -2_309.54),
        (Scenario::ShortUp, -44_832.32),
        (Scenario::ShortDown, 48_236.10),
    ] {
        close(got(s), want, 0.01, s.label());
    }
}

#[test]
fn r3_outlier_test_at_15_percent_of_tier_1() {
    let o = run_rows(&acceptance_rows(), &spec(1_000_000.0, Compounding::Continuous)).expect("runs");
    close(o.max_loss, 104_969.17, 0.01, "max loss");
    assert_eq!(o.max_loss_scenario, Some(Scenario::ParallelUp));
    assert_eq!(format!("{:.2}", o.max_loss_pct), "10.50");
    assert!(!o.outlier);
    let o = run_rows(&acceptance_rows(), &spec(600_000.0, Compounding::Continuous)).expect("runs");
    assert_eq!(format!("{:.2}", o.max_loss_pct), "17.49");
    assert!(o.outlier);
    // exactly 15% counts
    let l = o.max_loss;
    let o = run_rows(&acceptance_rows(), &spec(l / 0.15, Compounding::Continuous)).expect("runs");
    assert!(o.outlier, "{}", o.max_loss_pct);
    // the table: the footer says it too
    let t = results_frame(&o).expect("table");
    let labels = strings(&t, "Scenario");
    let at = |l: &str| labels.iter().position(|x| x.as_deref().is_some_and(|x| x.starts_with(l))).expect("row");
    assert_eq!(strings(&t, "Note")[at("Outlier")].as_deref(), Some("Yes"));
    assert_eq!(strings(&t, "Note")[at("Rule file")].as_deref(), Some("irrbb_shocks/2021-04-13.json"));
    assert!(strings(&t, "Note")[at("Curve")].as_deref().unwrap_or("").contains("curves/2026-10-01.json (BDT)"));
    assert_eq!(strings(&t, "Note")[at("Behaviour profile")].as_deref(), Some("none"));
    assert_eq!(strings(&t, "Note")[at("Maximum loss")].as_deref(), Some("Parallel up"));
    assert_eq!(t.height(), 6 + 9);
    assert_eq!(floats(&t, "Loss % of Tier 1")[at("Maximum loss")].map(|v| format!("{v:.2}")), Some("15.00".into()));
}

#[test]
fn r3_annual_compounding_converts_the_curve_with_ln_1_plus_y() {
    let c = flat("BDT", 10.0);
    close(discount_factor(&c, 4.5, Compounding::Annual).expect("df"), 0.651228, 0.000001, "annual DF at 4.5");
    close(discount_factor(&c, 4.5, Compounding::Continuous).expect("df"), (-0.45f64).exp(), 1e-12, "continuous DF at 4.5");
    // and a run under annual compounding uses it for the base value
    let o = run_rows(&acceptance_rows(), &spec(1_000_000.0, Compounding::Annual)).expect("runs");
    let want = 1_000_000.0 * 0.651228 - 600_000.0 * (1.1f64).powf(-0.0028);
    close(o.base_eve("BDT").expect("base"), want, 1.0, "base EVE, annual");
}

#[test]
fn r3_the_curve_is_interpolated_linearly_at_the_midpoint_and_flat_beyond_its_ends() {
    let c = YieldCurve { date: d("2026-10-01"), currency: "BDT".into(), source: "t".into(), points: vec![FtpPoint { tenor_days: 365, ftp_rate: 8.0 }, FtpPoint { tenor_days: 1825, ftp_rate: 10.0 }] };
    close(curve_rate_pct(&c, 0.5).expect("rate"), 8.0, 1e-12, "before the first point: flat");
    close(curve_rate_pct(&c, 3.0).expect("rate"), 8.0 + 2.0 * (1095.0 - 365.0) / (1825.0 - 365.0), 1e-12, "between the points");
    close(curve_rate_pct(&c, 9.0).expect("rate"), 10.0, 1e-12, "after the last: flat");
    // the curve of the currency in force at the cutoff
    let older = YieldCurve { date: d("2026-01-01"), ..flat("BDT", 7.0) };
    let newer = YieldCurve { date: d("2026-12-01"), ..flat("BDT", 12.0) };
    let usd = flat("USD", 5.0);
    let all = vec![older.clone(), newer.clone(), usd];
    assert_eq!(pick_curve(&all, "bdt", d("2026-10-31")).expect("curve").date, d("2026-01-01"), "the latest on or before the cutoff");
    assert_eq!(pick_curve(&all, "BDT", d("2027-01-01")).expect("curve").date, d("2026-12-01"));
    let e = format!("{:#}", pick_curve(&all, "BDT", d("2025-12-31")).err().expect("none yet"));
    assert!(e.contains("earliest BDT curve is dated 2026-01-01"), "{e}");
    let e = format!("{:#}", pick_curve(&all, "EUR", d("2026-10-31")).err().expect("no EUR"));
    assert!(e.contains("no EUR yield curve"), "{e}");
}

#[test]
fn r3_a_post_shock_floor_stops_the_shocked_rate_and_is_off_by_default() {
    let mut s = spec(1_000_000.0, Compounding::Continuous);
    s.curves = vec![flat("BDT", 2.0)];
    let off = run_rows(&acceptance_rows(), &s).expect("runs");
    s.post_shock_floor_pct = Some(0.0);
    let on = run_rows(&acceptance_rows(), &s).expect("runs");
    let down = |o: &IrrbbOutput| o.scenarios.iter().find(|x| x.scenario == Scenario::ParallelDown).expect("s").eve_change;
    // the rate 2% less 4% would be −2%; with the floor it stops at 0%, so the gain is smaller
    assert!(down(&on) < down(&off), "{} vs {}", down(&on), down(&off));
    close(down(&on), 1_000_000.0 * (1.0 - (-0.02f64 * 4.5).exp()) - 600_000.0 * (1.0 - (-0.02f64 * 0.0028).exp()) + 0.0, 1.0, "floored at 0%");
    // the upward scenarios are not changed by a floor at 0
    let up = |o: &IrrbbOutput| o.scenarios.iter().find(|x| x.scenario == Scenario::ParallelUp).expect("s").eve_change;
    close(up(&on), up(&off), 1e-9, "a rise is untouched");
}

#[test]
fn r3_delta_nii_of_a_floating_loan_in_the_six_scenarios() {
    let mut r = CfRow::simple(Side::Asset, 1_000_000.0, d("2030-06-30"));
    r.rate_type = RateType::Floating;
    r.reprice = Some(d("2026-11-30"));
    r.rate_pct = 10.0;
    let o = run_rows(&[r.clone()], &spec(1_000_000.0, Compounding::Continuous)).expect("runs");
    let n = |s: Scenario| o.scenarios.iter().find(|x| x.scenario == s).expect("s").nii;
    for (s, want) in [
        (Scenario::ParallelUp, 36_712.33),
        (Scenario::ParallelDown, -36_712.33),
        (Scenario::Steepener, -28_718.10),
        (Scenario::Flattener, 35_629.66),
        (Scenario::ShortUp, 44_957.08),
        (Scenario::ShortDown, -44_957.08),
    ] {
        close(n(s), want, 0.01, s.label());
    }
    // a liability's ΔNII has the opposite sign
    let mut l = r.clone();
    l.side = Side::Liability;
    let o = run_rows(&[l], &spec(1_000_000.0, Compounding::Continuous)).expect("runs");
    close(o.scenarios[0].nii, -36_712.33, 0.01, "a liability pays more");
}

#[test]
fn r3_delta_nii_uses_betas_the_floor_and_the_weight_of_the_repricing_time() {
    let mut r = CfRow::simple(Side::Liability, 1_000_000.0, d("2028-01-01"));
    r.rate_type = RateType::Floating;
    r.reprice = Some(d("2026-11-30"));
    r.rate_pct = 1.0;
    r.product = "sav".into();
    let cs = bdt();
    let betas = vec![BetaRow { product: "SAV".into(), base: 0.4, up: 0.3, down: 0.6 }];
    let setup = NiiSetup { cutoff: d("2026-10-31"), betas: &betas, floor_pct: None };
    let w = 1.0 - 30.0 / 365.0;
    close(row_nii(&r, Scenario::ParallelUp, &cs, 4.0, &setup), -1_000_000.0 * w * 0.04 * 0.3, 1e-6, "the up beta");
    close(row_nii(&r, Scenario::ParallelDown, &cs, 4.0, &setup), 1_000_000.0 * w * 0.04 * 0.6, 1e-6, "the down beta");
    // with a floor at 0% a rate of 1% can fall by 1 point only: −2.4 points × 0.6 would be −2.4 but the floor allows −1
    let floored = NiiSetup { cutoff: d("2026-10-31"), betas: &betas, floor_pct: Some(0.0) };
    close(row_nii(&r, Scenario::ParallelDown, &cs, 4.0, &floored), 1_000_000.0 * w * 0.01, 1e-6, "cut to the floor");
    // beyond a year: no weight
    r.reprice = Some(d("2027-12-31"));
    assert_eq!(row_nii(&r, Scenario::ParallelUp, &cs, 4.0, &setup), 0.0);
    // no dates: reprices at once, weight 1
    let mut nodate = CfRow::simple(Side::Asset, 1_000_000.0, d("2026-10-31"));
    nodate.maturity = None;
    close(row_nii(&nodate, Scenario::ParallelUp, &cs, 4.0, &NiiSetup { cutoff: d("2026-10-31"), betas: &[], floor_pct: None }), 40_000.0, 1e-6, "blank date");
}

#[test]
fn r3_two_currencies_run_on_their_own_curves_and_the_loss_has_no_offsets() {
    let usd = CurrencyShock { currency: "USD".into(), parallel_bp: 200.0, short_bp: 250.0, long_bp: 150.0 };
    let mut s = spec(1_000_000.0, Compounding::Continuous);
    s.shocks = shock_file(vec![usd]);
    s.curves = vec![flat("BDT", 10.0), flat("USD", 4.0)];
    // BDT: the acceptance book (loses in parallel up); USD: a liability that gains in parallel up
    let mut u = CfRow::simple(Side::Liability, 500_000.0, d("2031-04-30"));
    u.currency = "USD".into();
    let mut rows = acceptance_rows();
    rows.push(u);
    let o = run_rows(&rows, &s).expect("runs");
    let up = o.scenarios.iter().find(|x| x.scenario == Scenario::ParallelUp).expect("s");
    let bdt_c = up.by_currency.iter().find(|b| b.0 == "BDT").expect("bdt");
    let usd_c = up.by_currency.iter().find(|b| b.0 == "USD").expect("usd");
    close(bdt_c.1, -104_969.17, 0.01, "BDT change");
    assert!(usd_c.1 > 0.0 && usd_c.2 == 0.0, "USD gains: {usd_c:?}");
    close(up.loss, bdt_c.2 + usd_c.2, 1e-9, "the loss is the sum of the currencies' losses");
    close(up.loss, 104_969.17, 0.01, "so the USD gain does not offset");
    assert!(up.eve_change > bdt_c.1, "the signed EVE change does include it");
    assert!(o.notes.iter().any(|n| n.contains("no offsets")), "{:?}", o.notes);
    assert_eq!(o.curve_files.len(), 2);
    // a currency with no shock entry or no curve is an error naming it
    let mut e1 = s.clone();
    e1.shocks = shock_file(vec![]);
    assert!(format!("{:#}", run_rows(&rows, &e1).err().expect("no USD shock")).contains("no shocks for USD"));
    let mut e2 = s.clone();
    e2.curves = vec![flat("BDT", 10.0)];
    assert!(format!("{:#}", run_rows(&rows, &e2).err().expect("no USD curve")).contains("no USD yield curve"));
    // the aggregate with a BDT loss of 100 and a USD gain of 50 is 100: the rule itself
    let by = [("BDT", -100.0f64), ("USD", 50.0)];
    let agg: f64 = by.iter().map(|(_, c)| (-c).max(0.0)).sum();
    assert_eq!(agg, 100.0);
}

#[test]
fn r3_the_tier_1_and_the_shock_file_are_checked() {
    let e = format!("{:#}", run_rows(&acceptance_rows(), &spec(0.0, Compounding::Continuous)).err().expect("no tier 1"));
    assert!(e.contains("Tier 1 capital"), "{e}");
    let mut bad = spec(1.0, Compounding::Continuous);
    bad.shocks.data.decay_x = 0.0;
    assert!(format!("{:#}", run_rows(&acceptance_rows(), &bad).err().expect("decay")).contains("decay_x"));
}

#[test]
fn r3_the_shipped_bb_rule_file_has_the_table_2_numbers_and_its_source() {
    let tmp = dir("r3rules");
    set_data_dir_override(Some(tmp.clone()));
    let f = load_shocks(d("2026-10-31")).expect("loads");
    assert_eq!(f.meta.file, "irrbb_shocks/2021-04-13.json");
    assert!(!f.meta.sample && f.meta.source.contains("13 April 2021") && f.meta.source.contains("Table 2"));
    let b = f.data.for_currency("bdt", &f.meta.file).expect("BDT");
    assert_eq!((b.parallel_bp, b.short_bp, b.long_bp, f.data.decay_x, f.data.outlier_pct), (400.0, 500.0, 300.0, 4.0, 15.0));
    // the file was written next to the program's data so that the owner can add a newer dated file
    assert!(tmp.join("rules").join("irrbb_shocks").join("2021-04-13.json").exists());
    // a later dated file with other numbers takes over from its date
    std::fs::write(tmp.join("rules").join("irrbb_shocks").join("2027-01-01.json"), r#"{"as_of":"2027-01-01","source":"BB revision","currencies":[{"currency":"BDT","parallel_bp":300,"short_bp":400,"long_bp":250}]}"#).expect("write");
    assert_eq!(load_shocks(d("2026-12-31")).expect("loads").meta.file, "irrbb_shocks/2021-04-13.json");
    let later = load_shocks(d("2027-03-31")).expect("loads");
    assert_eq!(later.meta.file, "irrbb_shocks/2027-01-01.json");
    assert_eq!(later.data.for_currency("BDT", "x").expect("BDT").parallel_bp, 300.0);
    assert_eq!(later.data.outlier_pct, 15.0, "defaults to the guideline's 15");
    // before the first file: the message names the date
    let e = format!("{:#}", load_shocks(d("2020-01-01")).err().expect("too early"));
    assert!(e.contains("dated on or before the cutoff 2020-01-01"), "{e}");
    set_data_dir_override(None);
}

fn book_frame() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Account_No".into(), &["A1", "L1"]),
        Series::new("Side".into(), &["Asset", "Liability"]),
        Series::new("Outstanding".into(), &[1_000_000.0, 600_000.0]),
        Series::new("Rate".into(), &[9.0, 4.0]),
        dates("Matdat", &["2031-04-30", "2026-10-31"]),
    ])
    .expect("book")
}

fn cf_spec() -> CfInputSpec {
    CfInputSpec {
        role: GapRole::Side("Side".into()),
        principal_col: "Outstanding".into(),
        rate_col: Some("Rate".into()),
        rate_type_col: None,
        default_rate_type: RateType::Fixed,
        maturity_col: "Matdat".into(),
        reprice_col: None,
        repay_col: None,
        default_repayment: Repayment::Bullet,
        freq_col: None,
        default_frequency: Frequency::AtMaturity,
        product_col: None,
        currency_col: None,
        default_currency: "BDT".into(),
    }
}

#[test]
fn r3_tabs_give_the_same_numbers_and_the_two_output_tables() {
    let o = run_irrbb(vec![(book_frame().lazy(), cf_spec())], &spec(1_000_000.0, Compounding::Continuous)).expect("runs");
    close(o.base_eve("BDT").expect("base"), 37_796.13, 0.05, "base EVE");
    let t = results_frame(&o).expect("results");
    assert_eq!(strings(&t, "Scenario")[..6].iter().map(|s| s.clone().unwrap_or_default()).collect::<Vec<_>>(), vec!["Parallel up", "Parallel down", "Steepener", "Flattener", "Short up", "Short down"]);
    close(floats(&t, "EVE change")[0].unwrap_or(0.0), -104_969.17, 0.01, "table EVE change");
    close(floats(&t, "Loss")[0].unwrap_or(0.0), 104_969.17, 0.01, "table loss");
    assert_eq!(floats(&t, "Loss")[1], Some(0.0), "a gain is no loss");
    // ΔNII: the asset is fixed 9% to 2031 (no repricing in a year), the liability matured: weight 1 → pays 4% more
    close(floats(&t, "\u{394}NII")[0].unwrap_or(0.0), -600_000.0 * 0.04, 0.01, "parallel up ΔNII from the matured liability");
    let c = cashflows_frame(&o).expect("cash flows");
    assert_eq!(c.height(), 19 + 1);
    let b = strings(&c, "Bucket");
    assert_eq!(b[10].as_deref(), Some("4\u{2013}5Y"));
    assert_eq!(b[19].as_deref(), Some("Base EVE"));
    close(floats(&c, "Base PV")[19].unwrap_or(0.0), 37_796.13, 0.05, "Base EVE row");
    close(floats(&c, "Discount factor")[10].unwrap_or(0.0), (-0.45f64).exp(), 1e-9, "DF of the bucket 4–5Y");
    // a blank maturity stops the run with the count and the first accounts
    let bad = DataFrame::new(vec![
        Series::new("Account_No".into(), &["B1", "B2"]),
        Series::new("Side".into(), &["Asset", "Asset"]),
        Series::new("Outstanding".into(), &[1.0, 1.0]),
        Series::new("Rate".into(), &[1.0, 1.0]),
        dates("Matdat", &["", ""]),
    ])
    .expect("bad");
    let e = format!("{:#}", run_irrbb(vec![(bad.lazy(), cf_spec())], &spec(1.0, Compounding::Continuous)).err().expect("blank maturities"));
    assert!(e.contains("the maturity date is blank on 2 row(s)") && e.contains("first 2 values of Account_No: B1, B2"), "{e}");
    // with a floor on current rates a blank rate stops ΔNII with the same kind of message
    let norate = DataFrame::new(vec![
        Series::new("Account_No".into(), &["C1"]),
        Series::new("Side".into(), &["Asset"]),
        Series::new("Outstanding".into(), &[1.0]),
        Series::new("Rate".into(), &[None::<f64>]),
        dates("Matdat", &["2031-04-30"]),
    ])
    .expect("norate");
    let mut s = spec(1.0, Compounding::Continuous);
    s.nii_floor_pct = Some(0.0);
    let e = format!("{:#}", run_irrbb(vec![(norate.lazy(), cf_spec())], &s).err().expect("blank rate with a floor"));
    assert!(e.contains("\u{394}NII") && e.contains("the current rate is blank") && e.contains("C1"), "{e}");
}

#[test]
fn r3_cpr_and_tdrr_change_the_flows_by_scenario() {
    use micro_parquet::behaviour::BehaviourProduct;
    // a term deposit of 1,000,000 with TDRR 10%: the O/N part grows in the up scenarios, so the deposit's PV changes
    let mut r = CfRow::simple(Side::Liability, 1_000_000.0, d("2031-10-31"));
    r.product = "TD".into();
    let beh = BehaviourFile { as_of: "2026-10-31".into(), source: "t".into(), products: vec![BehaviourProduct { product: "TD".into(), tdrr_pct: 10.0, ..BehaviourProduct::default() }] };
    let mut s = spec(1_000_000.0, Compounding::Continuous);
    let without = run_rows(&[r.clone()], &s).expect("runs");
    s.behaviour = Some(("2026-10-31.json".into(), beh));
    let with = run_rows(&[r], &s).expect("runs");
    assert_ne!(without.scenarios[0].eve_change, with.scenarios[0].eve_change, "the behavioural flows move the result");
    assert_eq!(with.behaviour_file.as_deref(), Some("behaviour/2026-10-31.json"));
    let t = results_frame(&with).expect("results");
    let labels = strings(&t, "Scenario");
    let i = labels.iter().position(|x| x.as_deref() == Some("Behaviour profile")).expect("row");
    assert_eq!(strings(&t, "Note")[i].as_deref(), Some("behaviour/2026-10-31.json"));
}

#[test]
fn r3_both_tables_go_into_one_excel_workbook() {
    let o = run_irrbb(vec![(book_frame().lazy(), cf_spec())], &spec(1_000_000.0, Compounding::Continuous)).expect("runs");
    let tmp = dir("r3xlsx").join("irrbb.xlsx");
    let rep = micro_parquet::exporters::export_tabs_to_xlsx(vec![("IRRBB results".into(), results_frame(&o).expect("r")), ("IRRBB cash flows".into(), cashflows_frame(&o).expect("c"))], Vec::new(), &tmp).expect("exports");
    assert_eq!(rep.sheets, vec![("IRRBB results".to_string(), 1), ("IRRBB cash flows".to_string(), 1)]);
    assert!(tmp.exists());
}
