//! Phase 5 R2: behavioural CASA (non-maturity deposit profile).

use crate::common::*;
use chrono::NaiveDate;
use micro_parquet::banking::*;
use micro_parquet::behaviour::*;
use micro_parquet::cashflow::{generate_flows, CashFlowMode, CfRow, CfSettings, Scenario, Side};
use micro_parquet::models::*;
use micro_parquet::rules::{load_nmd_caps, set_data_dir_override};
use polars::prelude::*;

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("a test date")
}

/// The acceptance series: month-end balances of one year.
const SERIES: [f64; 12] = [100.0, 95.0, 110.0, 90.0, 105.0, 120.0, 98.0, 102.0, 96.0, 104.0, 108.0, 100.0];

fn month_ends() -> Vec<(NaiveDate, f64)> {
    let ends = ["2025-11-30", "2025-12-31", "2026-01-31", "2026-02-28", "2026-03-31", "2026-04-30", "2026-05-31", "2026-06-30", "2026-07-31", "2026-08-31", "2026-09-30", "2026-10-31"];
    ends.iter().zip(SERIES).map(|(e, b)| (d(e), b)).collect()
}

fn caps() -> micro_parquet::rules::RuleFile<micro_parquet::rules::NmdCapsFile> {
    let tmp = dir("r2caps");
    set_data_dir_override(Some(tmp));
    let c = load_nmd_caps(d("2026-10-31")).expect("the caps load");
    set_data_dir_override(None);
    c
}

fn input(category: &str, method: CoreMethod) -> RowInput {
    RowInput { product: "SAV".into(), category: category.into(), avg_maturity_years: Some(3.0), method, ..RowInput::default() }
}

#[test]
fn r2_the_nmd_caps_are_the_bb_table_4_numbers_with_their_source() {
    let c = caps();
    assert_eq!(c.meta.file, "irrbb_nmd_caps/2021-04-13.json");
    assert!(c.meta.source.contains("13 April 2021") && !c.meta.sample);
    let get = |n: &str| c.data.cap(n).map(|c| (c.core_cap_pct, c.max_avg_maturity_years));
    assert_eq!(get("Retail transactional"), Some((90.0, 5.0)));
    assert_eq!(get("Retail non-transactional"), Some((70.0, 4.5)));
    assert_eq!(get("Wholesale"), Some((50.0, 4.0)));
    // a rule file dated after the cutoff is not used; one before it is
    let tmp = dir("r2caps2");
    set_data_dir_override(Some(tmp.clone()));
    let kind = tmp.join("rules").join("irrbb_nmd_caps");
    std::fs::create_dir_all(&kind).expect("folder");
    let newer = r#"{"as_of":"2027-01-01","source":"a later circular","categories":[{"name":"Wholesale","core_cap_pct":40,"max_avg_maturity_years":3}]}"#;
    std::fs::write(kind.join("2027-01-01.json"), newer).expect("write");
    assert_eq!(load_nmd_caps(d("2026-10-31")).expect("loads").meta.file, "irrbb_nmd_caps/2021-04-13.json");
    let later = load_nmd_caps(d("2027-02-01")).expect("loads");
    assert_eq!(later.meta.file, "irrbb_nmd_caps/2027-01-01.json");
    assert_eq!(later.data.cap("Wholesale").map(|c| c.core_cap_pct), Some(40.0));
    // a file whose as_of differs from its name, or that has no source, is refused naming it
    std::fs::write(kind.join("2027-06-01.json"), r#"{"as_of":"2027-06-02","source":"x","categories":[]}"#).expect("write");
    let e = format!("{:#}", load_nmd_caps(d("2027-07-01")).err().expect("mismatch"));
    assert!(e.contains("2027-06-01.json") && e.contains("as_of"), "{e}");
    std::fs::write(kind.join("2027-06-01.json"), r#"{"as_of":"2027-06-01","categories":[]}"#).expect("write");
    let e = format!("{:#}", load_nmd_caps(d("2027-07-01")).err().expect("no source"));
    assert!(e.contains("no \"source\""), "{e}");
    set_data_dir_override(None);
}

#[test]
fn r2_historical_minimum_gives_90_and_the_cap_cuts_non_transactional_to_70() {
    let caps = caps();
    let s = month_ends();
    let rt = build_product(&input("Retail transactional", CoreMethod::HistoricalMinimum), Some(&s), 120, DEFAULT_Z, &caps).expect("builds");
    assert!((rt.product.core_pct - 90.0).abs() < 1e-9, "{}", rt.product.core_pct);
    assert!(!rt.product.capped, "90 is not above the cap of 90");
    assert_eq!(rt.months_used, 12);
    let rn = build_product(&input("Retail non-transactional", CoreMethod::HistoricalMinimum), Some(&s), 120, DEFAULT_Z, &caps).expect("builds");
    assert!((rn.product.core_pct - 70.0).abs() < 1e-9 && rn.product.capped, "{:?}", rn.product);
    assert!((rn.computed_core_pct.unwrap_or(0.0) - 90.0).abs() < 1e-9, "the computed share is kept");
    assert_eq!(rn.product.lookback_months, 12);
    assert_eq!(rn.product.method, "Historical minimum");
}

#[test]
fn r2_volatility_gives_sigma_and_a_core_of_70_46() {
    let caps = caps();
    let s = month_ends();
    let o = build_product(&input("Retail transactional", CoreMethod::Volatility), Some(&s), 120, DEFAULT_Z, &caps).expect("builds");
    let sigma = o.sigma.expect("sigma");
    assert!((sigma - 0.126772).abs() <= 0.000001, "{sigma}");
    assert_eq!(format!("{:.2}", o.product.core_pct), "70.46");
    assert!(!o.product.capped);
    // floored at 0 with a large z
    let (_, c) = core_volatility("SAV", &SERIES, 100.0).expect("volatility");
    assert_eq!(c, 0.0);
    // too few months
    let e = format!("{:#}", core_volatility("SAV", &SERIES[..2], 2.33).err().expect("two points"));
    assert!(e.contains("at least 3 month-ends"), "{e}");
}

#[test]
fn r2_typed_values_above_the_cap_are_refused_with_the_cap_and_the_source() {
    let caps = caps();
    let mut i = input("Retail non-transactional", CoreMethod::Typed);
    i.typed_core_pct = Some(80.0);
    let e = format!("{:#}", build_product(&i, None, 120, DEFAULT_Z, &caps).err().expect("80% is above 70%"));
    assert!(e.contains("80%") && e.contains("cap of 70%") && e.contains("Retail non-transactional") && e.contains("13 April 2021"), "{e}");
    i.typed_core_pct = Some(70.0);
    let ok = build_product(&i, None, 120, DEFAULT_Z, &caps).expect("70 is at the cap");
    assert_eq!((ok.product.core_pct, ok.product.capped, ok.product.lookback_months), (70.0, false, 0));
    i.avg_maturity_years = Some(5.0);
    let e = format!("{:#}", build_product(&i, None, 120, DEFAULT_Z, &caps).err().expect("5 years is above 4.5"));
    assert!(e.contains("5 years") && e.contains("cap of 4.5 years") && e.contains("13 April 2021"), "{e}");
    // no average maturity, no category, an unknown category
    i.avg_maturity_years = None;
    assert!(format!("{:#}", build_product(&i, None, 120, DEFAULT_Z, &caps).err().expect("none")).contains("average maturity"));
    let e = format!("{:#}", build_product(&input("", CoreMethod::Typed), Some(&month_ends()), 120, DEFAULT_Z, &caps).err().expect("no category for a history product"));
    assert!(e.contains("choose a category"), "{e}");
    let e = format!("{:#}", build_product(&input("Retail", CoreMethod::Typed), None, 120, DEFAULT_Z, &caps).err().expect("unknown"));
    assert!(e.contains("not a category") && e.contains("Wholesale"), "{e}");
    // a product with no category and no history only has CPR and TDRR
    let loan = RowInput { product: "HL".into(), cpr_pct: 10.0, ..RowInput::default() };
    let o = build_product(&loan, None, 120, DEFAULT_Z, &caps).expect("a loan product");
    assert!(!o.product.is_nmd() && o.product.cpr_pct == 10.0 && o.product.tdrr_pct == 0.0);
    let e = format!("{:#}", build_product(&RowInput { cpr_pct: 120.0, ..loan }, None, 120, DEFAULT_Z, &caps).err().expect("120%"));
    assert!(e.contains("CPR"), "{e}");
}

#[test]
fn r2_a_short_history_gives_the_warning_with_the_months() {
    assert_eq!(short_history_warning(12).as_deref(), Some("BB IRRBB guideline §15.13 uses 10 years of volume history; this profile uses 12 months."));
    assert_eq!(short_history_warning(119).is_some(), true);
    assert_eq!(short_history_warning(120), None);
    // the months used are what the history has within the look-back
    let caps = caps();
    let s = month_ends();
    let o = build_product(&input("Wholesale", CoreMethod::HistoricalMinimum), Some(&s), 6, DEFAULT_Z, &caps).expect("builds");
    assert_eq!(o.months_used, 6, "a look-back of 6 months uses 6");
    // the last six balances are 98, 102, 96, 104, 108, 100: minimum 96 over latest 100
    assert!((o.computed_core_pct.unwrap_or(0.0) - 96.0).abs() < 1e-9);
    assert_eq!(o.product.core_pct, 50.0, "capped to the wholesale cap");
}

#[test]
fn r2_daily_history_is_reduced_to_month_end_and_a_missing_month_is_an_error() {
    let pts = vec![(d("2026-01-30"), 5.0), (d("2026-01-31"), 7.0), (d("2026-01-02"), 99.0), (d("2026-02-27"), 8.0), (d("2026-03-31"), 9.0)];
    let me = month_end_balances(&pts);
    assert_eq!(me, vec![(d("2026-01-31"), 7.0), (d("2026-02-27"), 8.0), (d("2026-03-31"), 9.0)], "the last date of each month, in date order");
    let gap = vec![(d("2026-01-31"), 1.0), (d("2026-02-28"), 1.0), (d("2026-04-30"), 1.0)];
    let e = format!("{:#}", lookback_window("SAV", &gap, 120).err().expect("March is missing"));
    assert!(e.contains("2026-03") && e.contains("continuous"), "{e}");
    // a look-back that starts after the gap is fine
    assert_eq!(lookback_window("SAV", &gap, 1).expect("one month").len(), 1);
    let e = format!("{:#}", lookback_window("SAV", &[(d("2026-01-31"), -5.0)], 12).err().expect("negative"));
    assert!(e.contains("negative"), "{e}");
}

#[test]
fn r2_the_history_is_read_from_a_tab_summed_per_product_and_date() {
    let df = DataFrame::new(vec![
        dates("Date", &["2026-09-30", "2026-09-30", "2026-10-31", "2026-10-31", "2026-10-31"]),
        Series::new("Product".into(), &["sav", "SAV ", "SAV", "CD", "CD"]),
        Series::new("Balance".into(), &[10.0, 5.0, 20.0, 1.0, 2.0]),
    ])
    .expect("history");
    let rows = history_from_lazy(df.lazy(), "Date", "Product", "Balance").expect("reads");
    assert_eq!(rows, vec![("CD".to_string(), d("2026-10-31"), 3.0), ("SAV".to_string(), d("2026-09-30"), 15.0), ("SAV".to_string(), d("2026-10-31"), 20.0)]);
    let by = series_by_product(&rows);
    assert_eq!(by.iter().map(|b| b.0.as_str()).collect::<Vec<_>>(), vec!["CD", "SAV"]);
    // blank values stop the step with the count and the first rows
    let bad = DataFrame::new(vec![dates("Date", &["2026-09-30", ""]), Series::new("Product".into(), &["SAV", "SAV"]), Series::new("Balance".into(), &[1.0, 2.0])]).expect("bad");
    let e = format!("{:#}", history_from_lazy(bad.lazy(), "Date", "Product", "Balance").err().expect("blank date"));
    assert!(e.contains("1 row(s)") && e.contains("row 2"), "{e}");
    let txt = DataFrame::new(vec![Series::new("Date".into(), &["x"]), Series::new("Product".into(), &["SAV"]), Series::new("Balance".into(), &[1.0])]).expect("bad");
    assert!(format!("{:#}", history_from_lazy(txt.lazy(), "Date", "Product", "Balance").err().expect("text date")).contains("not a date"));
}

#[test]
fn r2_save_profile_writes_the_dated_file_that_the_tools_read_back() {
    let tmp = dir("r2save");
    let dirp = tmp.join("behaviour");
    let caps = caps();
    let o = build_product(&input("Retail transactional", CoreMethod::HistoricalMinimum), Some(&month_ends()), 120, DEFAULT_Z, &caps).expect("builds");
    let file = BehaviourFile { as_of: "2026-10-31".into(), source: "ALM study to 2026-10-31".into(), products: vec![o.product.clone(), BehaviourProduct { product: "HL".into(), cpr_pct: 10.0, ..BehaviourProduct::default() }] };
    let path = save_behaviour(&dirp, &file).expect("saves");
    assert!(path.ends_with("2026-10-31.json"));
    let text = std::fs::read_to_string(&path).expect("read");
    for f in ["\"as_of\"", "\"source\"", "\"products\"", "\"product\"", "\"category\"", "\"core_pct\"", "\"avg_maturity_years\"", "\"method\"", "\"lookback_months\"", "\"capped\"", "\"cpr_pct\"", "\"tdrr_pct\""] {
        assert!(text.contains(f), "the file has {f}: {text}");
    }
    let back = load_behaviour(&dirp, "2026-10-31.json").expect("loads");
    assert_eq!(back.data, file);
    assert_eq!(back.meta.file, "behaviour/2026-10-31.json");
    assert_eq!(list_behaviour_files(&dirp), vec!["2026-10-31.json".to_string()]);
    assert!(latest_behaviour(&dirp, d("2026-09-30")).expect("reads").is_none(), "a profile dated after the cutoff is not used");
    assert!(latest_behaviour(&dirp, d("2026-11-30")).expect("reads").is_some());
    // refusals
    let e = format!("{:#}", save_behaviour(&dirp, &BehaviourFile { as_of: "31/10/2026".into(), ..file.clone() }).err().expect("bad date"));
    assert!(e.contains("YYYY-MM-DD"), "{e}");
    let e = format!("{:#}", save_behaviour(&dirp, &BehaviourFile { source: " ".into(), ..file.clone() }).err().expect("no source"));
    assert!(e.contains("where the profile comes from"), "{e}");
    let mut twice = file.clone();
    twice.products.push(twice.products[0].clone());
    assert!(format!("{:#}", save_behaviour(&dirp, &twice).err().expect("twice")).contains("twice"));
}

fn savings_book() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Product".into(), &["SAV", "FDR"]),
        Series::new("Balance".into(), &[1_000_000.0, 500_000.0]),
        dates("Matdat", &["", "2027-10-31"]),
    ])
    .expect("book")
}

fn preset() -> CustomBucketDef {
    CustomBucketDef {
        name: "Test".into(),
        rules: vec![
            micro_parquet::models::BucketRuleItem { max_days: 30, label: "0-1 month".into() },
            micro_parquet::models::BucketRuleItem { max_days: 365, label: "1-12 months".into() },
            micro_parquet::models::BucketRuleItem { max_days: 1825, label: "1-5 years".into() },
        ],
        overflow_label: "Over 5 years".into(),
        overdue_label: "Overdue".into(),
        null_sight_label: "0-1 month".into(),
        null_long_label: "Over 5 years".into(),
    }
}

fn savings_profile() -> GapBehaviour {
    let p = BehaviourProduct { product: "sav".into(), category: "Retail non-transactional".into(), core_pct: 60.0, avg_maturity_years: 3.0, method: "Typed".into(), ..BehaviourProduct::default() };
    GapBehaviour { file_name: "2026-10-31.json".into(), file: BehaviourFile { as_of: "2026-10-31".into(), source: "test".into(), products: vec![p] }, product_cols: vec!["Product".into()], stress_tdrr: false }
}

#[test]
fn r2_the_gap_ladder_splits_savings_into_the_first_bucket_and_the_bucket_of_the_average_maturity() {
    let spec = GapInputSpec { role: GapRole::Liability, amount_col: "Balance".into(), maturity_col: "Matdat".into() };
    let with = build_gap_ladder_behaviour(vec![(savings_book().lazy(), spec.clone())], &preset(), "2026-10-31", true, None, Some(&savings_profile())).expect("ladder");
    let labels = strings(&with, "Bucket");
    let out = floats(&with, "Outflows");
    let at = |l: &str| out[labels.iter().position(|x| x.as_deref() == Some(l)).expect("bucket")].unwrap_or(0.0);
    assert_eq!(at("0-1 month"), 400_000.0, "the volatile part is in the first bucket");
    assert_eq!(at("1-5 years"), 600_000.0, "the core part is in the bucket that holds 3 years (1095 days)");
    assert_eq!(at("1-12 months"), 500_000.0, "the FDR deposit has no profile and stays in the bucket of its maturity (365 days)");
    // unticking: the Phase 4 result, byte for byte
    let without = build_gap_ladder_behaviour(vec![(savings_book().lazy(), spec.clone())], &preset(), "2026-10-31", true, None, None).expect("ladder");
    let phase4 = build_gap_ladder(vec![(savings_book().lazy(), spec.clone())], &preset(), "2026-10-31", true, None).expect("ladder");
    assert!(without.equals_missing(&phase4), "no profile: exactly the Phase 4 ladder");
    let first = floats(&without, "Outflows");
    assert_eq!(first[0], Some(1_000_000.0), "without it a blank maturity is sight: everything in the first bucket");
    // totals agree
    assert_eq!(floats(&with, "Outflows").iter().flatten().take(4).sum::<f64>(), 1_500_000.0);
    // an asset of a profiled product is not split (the profile is about deposits)
    let asset = GapInputSpec { role: GapRole::Asset, ..spec.clone() };
    let a = build_gap_ladder_behaviour(vec![(savings_book().lazy(), asset)], &preset(), "2026-10-31", true, None, Some(&savings_profile())).expect("ladder");
    assert_eq!(floats(&a, "Inflows")[0], Some(1_000_000.0));
    // a missing product column is an error
    let mut bad = savings_profile();
    bad.product_cols = vec![String::new()];
    let e = format!("{:#}", build_gap_ladder_behaviour(vec![(savings_book().lazy(), spec)], &preset(), "2026-10-31", true, None, Some(&bad)).err().expect("no product column"));
    assert!(e.contains("choose the product column of line 1"), "{e}");
}

#[test]
fn r2_the_bucket_of_an_average_maturity_follows_the_preset() {
    let p = preset();
    for (days, want) in [(-1, "Overdue"), (0, "0-1 month"), (30, "0-1 month"), (31, "1-12 months"), (1095, "1-5 years"), (1825, "1-5 years"), (1826, "Over 5 years")] {
        assert_eq!(bucket_label_for_days(&p, days), want, "{days} days");
    }
}

#[test]
fn r2_a_ladder_tab_source_with_a_profile_compiles_and_an_old_one_is_unchanged() {
    let tmp = dir("r2src");
    let src = |b: Option<GapBehaviour>| TabSource::GapLadder {
        inputs: vec![GapTabInput { source: parquet(&tmp, "book", &savings_book()), steps: vec![], spec: GapInputSpec { role: GapRole::Liability, amount_col: "Balance".into(), maturity_col: "Matdat".into() } }],
        preset: preset(),
        cutoff_date: "2026-10-31".into(),
        null_as_sight: true,
        limits: None,
        behaviour: b,
    };
    let with = micro_parquet::engine::compile_tab_lazy(&src(Some(savings_profile())), &[], &tmp).expect("compiles").collect().expect("collects");
    assert_eq!(floats(&with, "Outflows")[0], Some(400_000.0));
    let without = micro_parquet::engine::compile_tab_lazy(&src(None), &[], &tmp).expect("compiles").collect().expect("collects");
    assert_eq!(floats(&without, "Outflows")[0], Some(1_000_000.0));
}

// ------------------------------------------------------------ A5: no CPR or TDRR in the gap ladder

#[test]
fn a5_the_gap_ladder_does_not_use_cpr_or_tdrr() {
    let book = DataFrame::new(vec![
        Series::new("Product".into(), &["SAV", "FDR", "HL"]),
        Series::new("Balance".into(), &[1_000_000.0, 500_000.0, 800_000.0]),
        dates("Matdat", &["", "2027-10-31", "2028-10-31"]),
    ])
    .expect("book");
    let profile = |cpr: f64, tdrr: f64| {
        let sav = BehaviourProduct { product: "SAV".into(), category: "Retail non-transactional".into(), core_pct: 60.0, avg_maturity_years: 3.0, method: "Typed".into(), cpr_pct: cpr, tdrr_pct: tdrr, ..BehaviourProduct::default() };
        // a term deposit and a loan: only a CPR or a TDRR, no category
        let fdr = BehaviourProduct { product: "FDR".into(), tdrr_pct: tdrr, ..BehaviourProduct::default() };
        let hl = BehaviourProduct { product: "HL".into(), cpr_pct: cpr, ..BehaviourProduct::default() };
        GapBehaviour { file_name: "2026-10-31.json".into(), file: BehaviourFile { as_of: "2026-10-31".into(), source: "test".into(), products: vec![sav, fdr, hl] }, product_cols: vec!["Product".into()], stress_tdrr: false }
    };
    for role in [GapRole::Liability, GapRole::Asset] {
        let spec = GapInputSpec { role, amount_col: "Balance".into(), maturity_col: "Matdat".into() };
        let ladder = |p: &GapBehaviour| build_gap_ladder_behaviour(vec![(book.clone().lazy(), spec.clone())], &preset(), "2026-10-31", true, None, Some(p)).expect("ladder");
        let none = ladder(&profile(0.0, 0.0));
        let big = ladder(&profile(10.0, 10.0));
        assert!(none.equals_missing(&big), "a CPR of 10% and a TDRR of 10% change nothing in the gap ladder");
    }
    // while the IRRBB cash-flow ladder does use them (the term deposit's 10% goes to O/N)
    let r = CfRow { rate_pct: 0.0, ..CfRow::simple(Side::Liability, 500_000.0, d("2027-10-31")) };
    let td = BehaviourProduct { product: "FDR".into(), tdrr_pct: 10.0, ..BehaviourProduct::default() };
    let rf = generate_flows(&r, &CfSettings::new(d("2026-10-31"), CashFlowMode::PrincipalOnly, Scenario::Base), Some(&td)).expect("flows");
    assert_eq!(rf.flows[0].principal, 50_000.0);
}
