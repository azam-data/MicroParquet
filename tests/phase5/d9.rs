//! Phase 5 D9: rule files without "sample", behaviour files checked on load, floating rows with a
//! past repricing date.

use crate::common::*;
use chrono::NaiveDate;
use micro_parquet::behaviour::*;
use micro_parquet::cashflow::{generate_flows, ladder_from_rows, CashFlowMode, CfRow, CfSettings, RateType, Scenario, Side};
use micro_parquet::rules::{parse_rule_text, set_data_dir_override};

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("a test date")
}

#[derive(serde::Deserialize)]
struct Anything {}

#[test]
fn d9_a_rule_file_without_sample_is_a_sample_and_a_non_boolean_is_refused() {
    let text = |sample: &str| format!(r#"{{"as_of": "2026-01-01", "source": "x"{sample}}}"#);
    let at = d("2026-01-01");
    assert!(parse_rule_text::<Anything>("t/2026-01-01.json", at, &text("")).expect("reads").meta.sample, "no key: a sample");
    assert!(parse_rule_text::<Anything>("t/2026-01-01.json", at, &text(r#", "sample": null"#)).expect("reads").meta.sample);
    assert!(parse_rule_text::<Anything>("t/2026-01-01.json", at, &text(r#", "sample": true"#)).expect("reads").meta.sample);
    assert!(!parse_rule_text::<Anything>("t/2026-01-01.json", at, &text(r#", "sample": false"#)).expect("reads").meta.sample);
    for bad in [r#", "sample": "false""#, r#", "sample": 0"#] {
        let e = format!("{:#}", parse_rule_text::<Anything>("t/2026-01-01.json", at, &text(bad)).err().expect("refused"));
        assert!(e.contains("t/2026-01-01.json") && e.contains("not true or false"), "{e}");
    }
}

fn write_profile(dirp: &std::path::Path, products: &str) {
    std::fs::create_dir_all(dirp).expect("folder");
    std::fs::write(dirp.join("2026-10-31.json"), format!(r#"{{"as_of": "2026-10-31", "source": "ALM study", "sample": false, "products": [{products}]}}"#)).expect("write");
}

#[test]
fn d9_a_behaviour_file_is_checked_again_on_load() {
    let tmp = dir("d9beh");
    set_data_dir_override(Some(tmp.clone()));
    let dirp = tmp.join("behaviour");
    let load = |products: &str| {
        write_profile(&dirp, products);
        load_behaviour(&dirp, "2026-10-31.json").map(|_| ()).map_err(|e| format!("{:#}", e))
    };
    let sav = |extra: &str| format!(r#"{{"product": "SAV", "category": "Retail non-transactional", "avg_maturity_years": 3.0{extra}}}"#);
    assert!(load(&sav(r#", "core_pct": 60"#)).is_ok());
    // core_pct is required for a non-maturity deposit (a missing one is not read as 0)
    let e = load(&sav("")).err().expect("refused");
    assert!(e.contains("behaviour/2026-10-31.json") && e.contains("'SAV'") && e.contains("no core_pct"), "{e}");
    // a loan with no category needs none
    assert!(load(r#"{"product": "HL", "cpr_pct": 10}"#).is_ok());
    // the category must be one of the caps file's
    let e = load(&sav(r#", "core_pct": 60"#).replace("Retail non-transactional", "Gold")).err().expect("refused");
    assert!(e.contains("'Gold'") && e.contains("irrbb_nmd_caps"), "{e}");
    // the core share and the average maturity are held to the caps of the category
    let e = load(&sav(r#", "core_pct": 95"#)).err().expect("refused");
    assert!(e.contains("95") && e.contains("above the cap of 70%"), "{e}");
    let e = load(&sav(r#", "core_pct": 60"#).replace("3.0", "9.0")).err().expect("refused");
    assert!(e.contains("average maturity of 9 years") && e.contains("above the cap"), "{e}");
    set_data_dir_override(None);
}

fn floating(reprice: &str) -> CfRow {
    CfRow { rate_pct: 12.0, rate_type: RateType::Floating, reprice: Some(d(reprice)), ..CfRow::simple(Side::Asset, 1_000_000.0, d("2028-10-31")) }
}

#[test]
fn d9_a_floating_row_with_a_past_repricing_date_goes_to_o_n_principal_only_and_is_counted() {
    let s = CfSettings { cutoff: d("2026-10-31"), mode: CashFlowMode::PrincipalAndInterest, scenario: Scenario::Base, blank_maturity_on: false };
    for reprice in ["2026-10-31", "2026-06-30"] {
        let rf = generate_flows(&floating(reprice), &s, None).expect("flows");
        assert_eq!(rf.flows.len(), 1, "{reprice}");
        assert_eq!((rf.flows[0].date, rf.flows[0].principal, rf.flows[0].interest), (d("2026-10-31"), 1_000_000.0, 0.0), "{reprice}: principal only, no interest");
        assert_eq!(rf.past_reprice, Some(1_000_000.0));
    }
    // a repricing date after the cutoff is not past
    assert_eq!(generate_flows(&floating("2026-11-30"), &s, None).expect("flows").past_reprice, None);
    let res = ladder_from_rows(&[floating("2026-06-30"), floating("2026-11-30"), floating("2026-10-31")], &s, None).expect("ladder");
    assert_eq!(res.past_reprice_rows, 2);
    assert!(res.notes.iter().any(|n| n.contains("2 floating row(s) had a repricing date on or before the cutoff and were put in O/N, principal only")), "{:?}", res.notes);
    assert_eq!(res.for_currency("BDT").expect("BDT").assets[0], 2_000_000.0, "both past rows are in O/N");
}
