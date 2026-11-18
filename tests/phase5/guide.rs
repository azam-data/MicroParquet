//! Phase 5 guide notes: one `docs/guide/<tool>.md` per new tool, checked like the Phase 4 ones
//! (ten sections, proven worked examples). `MP_GUIDE_DUMP=<folder> cargo test guide_` writes the computed tables.

use crate::common::*;
use crate::guide::{check_guide, check_guide_file, read_guide};
use micro_parquet::banking::GapRole;
use micro_parquet::cashflow::*;
use polars::prelude::*;

/// The tool ids of Phase 5 (file names in docs/guide/), added as each task is done.
pub const P5_GUIDE_TOOLS: [&str; 2] = ["irrbb-cashflow-ladder", "behaviour-profile"];

fn all_test_sources() -> String {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut all = String::new();
    let mut stack = vec![root];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).expect("read the tests folder").flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().map(|x| x == "rs").unwrap_or(false) {
                all.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
            }
        }
    }
    all
}

#[test]
fn every_phase5_tool_has_a_complete_guide_with_proven_examples_listed_in_the_readme() {
    let sources = all_test_sources();
    let readme = read_guide("README");
    for tool in P5_GUIDE_TOOLS {
        check_guide_file(tool, &sources);
        assert!(sources.contains(&format!("fn guide_{}(", tool.replace('-', "_"))), "{tool}: there is no test guide_{}", tool.replace('-', "_"));
        assert!(readme.contains(&format!("({tool}.md)")), "README: no link to {tool}.md");
    }
}

fn d(s: &str) -> chrono::NaiveDate {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("date")
}

#[test]
fn guide_irrbb_cashflow_ladder() {
    let book = DataFrame::new(vec![
        Series::new("Account_No".into(), &["A1", "A2", "L1", "L2"]),
        Series::new("Side".into(), &["Asset", "Asset", "Liability", "Liability"]),
        Series::new("Outstanding".into(), &[1_000_000.0, 250_000.0, 300_000.0, 600_000.0]),
        Series::new("Rate".into(), &[9.0, 9.0, 5.0, 6.0]),
        dates("Matdat", &["2031-04-30", "2026-10-31", "2026-09-30", "2027-10-31"]),
    ])
    .expect("book");
    let spec = CfInputSpec {
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
    };
    let settings = CfSettings { cutoff: d("2026-10-31"), mode: CashFlowMode::PrincipalOnly, scenario: Scenario::Base };
    let res = build_ladder(vec![(book.clone().lazy(), spec)], &settings, None).expect("the ladder builds");
    let full = ladder_frame(&res).expect("frame");
    assert_eq!(full.height(), 20);
    // the rows that have an amount
    let a = full.column("Assets").expect("c").f64().expect("f").clone();
    let l = full.column("Liabilities").expect("c").f64().expect("f").clone();
    let keep: BooleanChunked = a.into_iter().zip(l.into_iter()).map(|(a, l)| Some(a.unwrap_or(0.0) != 0.0 || l.unwrap_or(0.0) != 0.0)).collect();
    let shown = full.filter(&keep).expect("filter");
    assert_eq!(strings(&shown, "Bucket"), vec![Some("O/N".into()), Some("9M\u{2013}1Y".into()), Some("4\u{2013}5Y".into()), Some("Matured assets, not in ladder (1)".into())]);

    // an annuity: 120,000 at 12%, monthly, 12 payments left: the first three payments
    let mut r = CfRow::simple(Side::Asset, 120_000.0, d("2027-10-31"));
    r.rate_pct = 12.0;
    r.repayment = Repayment::EqualInstalment;
    r.frequency = Frequency::Monthly;
    let rf = generate_flows(&r, &CfSettings { cutoff: d("2026-10-31"), mode: CashFlowMode::PrincipalAndInterest, scenario: Scenario::Base }, None).expect("flows");
    assert_eq!(rf.flows.len(), 12);
    close(Some(rf.flows[0].total()), 10_661.85, 0.005, "instalment");
    close(Some(rf.flows.iter().map(|f| f.principal).sum::<f64>()), 120_000.0, 0.01, "principal");
    let first: Vec<&Flow> = rf.flows.iter().take(3).collect();
    let flows = DataFrame::new(vec![
        Series::new("Payment date".into(), first.iter().map(|f| f.date.format("%Y-%m-%d").to_string()).collect::<Vec<_>>()),
        Series::new("Principal".into(), first.iter().map(|f| f.principal).collect::<Vec<_>>()),
        Series::new("Interest".into(), first.iter().map(|f| f.interest).collect::<Vec<_>>()),
        Series::new("Payment".into(), first.iter().map(|f| f.total()).collect::<Vec<_>>()),
    ])
    .expect("flows frame");
    check_guide(
        "irrbb-cashflow-ladder",
        "guide_irrbb_cashflow_ladder",
        &[
            md(&book, &["Account_No", "Side", "Outstanding:2", "Rate:2", "Matdat"]),
            md(&shown, &["Currency", "Bucket", "Midpoint (years):~4", "Assets:2", "Liabilities:2", "Net:2", "Cumulative net:2"]),
            md(&flows, &["Payment date", "Principal:2", "Interest:2", "Payment:2"]),
        ],
    );
}

#[test]
fn guide_behaviour_profile() {
    use micro_parquet::banking::*;
    use micro_parquet::behaviour::*;
    use micro_parquet::models::*;
    let dir_p = dir("g_beh");
    micro_parquet::rules::set_data_dir_override(Some(dir_p.clone()));
    let caps = micro_parquet::rules::load_nmd_caps(d("2026-10-31")).expect("caps");
    micro_parquet::rules::set_data_dir_override(None);
    let balances = [100.0, 95.0, 110.0, 90.0, 105.0, 120.0, 98.0, 102.0, 96.0, 104.0, 108.0, 100.0];
    let ends = ["2025-11-30", "2025-12-31", "2026-01-31", "2026-02-28", "2026-03-31", "2026-04-30", "2026-05-31", "2026-06-30", "2026-07-31", "2026-08-31", "2026-09-30", "2026-10-31"];
    let history = DataFrame::new(vec![dates("Month-end", &ends), Series::new("Product".into(), vec!["SAV"; 12]), Series::new("Balance".into(), balances.to_vec())]).expect("history");
    let series: Vec<(chrono::NaiveDate, f64)> = ends.iter().map(|e| d(e)).zip(balances).collect();
    let run = |cat: &str, m: CoreMethod| build_product(&RowInput { product: "SAV".into(), category: cat.into(), avg_maturity_years: Some(3.0), method: m, ..RowInput::default() }, Some(&series), 120, DEFAULT_Z, &caps).expect("builds");
    let a = run("Retail transactional", CoreMethod::HistoricalMinimum);
    let b = run("Retail non-transactional", CoreMethod::HistoricalMinimum);
    let c = run("Retail transactional", CoreMethod::Volatility);
    assert!((a.product.core_pct - 90.0).abs() < 1e-9 && !a.product.capped);
    assert!((b.product.core_pct - 70.0).abs() < 1e-9 && b.product.capped);
    assert!((c.sigma.unwrap_or(0.0) - 0.126772).abs() < 1e-6);
    assert_eq!(format!("{:.2}", c.product.core_pct), "70.46");
    let rows = DataFrame::new(vec![
        Series::new("Category".into(), &[a.product.category.as_str(), b.product.category.as_str(), c.product.category.as_str()]),
        Series::new("Method".into(), &[a.product.method.as_str(), b.product.method.as_str(), c.product.method.as_str()]),
        Series::new("Core %".into(), &[a.product.core_pct, b.product.core_pct, c.product.core_pct]),
        Series::new("Capped".into(), &[if a.product.capped { "capped" } else { "" }, if b.product.capped { "capped" } else { "" }, if c.product.capped { "capped" } else { "" }]),
        Series::new("Months".into(), &[a.months_used as f64, b.months_used as f64, c.months_used as f64]),
    ])
    .expect("rows");
    assert_eq!(short_history_warning(12).as_deref(), Some("BB IRRBB guideline §15.13 uses 10 years of volume history; this profile uses 12 months."));
    // the gap ladder: 1,000,000 of savings, core 60%, 3 years
    let book = DataFrame::new(vec![Series::new("Product".into(), &["SAV"]), Series::new("Balance".into(), &[1_000_000.0]), dates("Matdat", &[""])]).expect("book");
    let preset = CustomBucketDef {
        name: "Example".into(),
        rules: vec![BucketRuleItem { max_days: 30, label: "0-1 month".into() }, BucketRuleItem { max_days: 365, label: "1-12 months".into() }, BucketRuleItem { max_days: 1825, label: "1-5 years".into() }],
        overflow_label: "Over 5 years".into(),
        overdue_label: "Overdue".into(),
        null_sight_label: "0-1 month".into(),
        null_long_label: "Over 5 years".into(),
    };
    let beh = GapBehaviour {
        file_name: "2026-10-31.json".into(),
        file: BehaviourFile { as_of: "2026-10-31".into(), source: "example".into(), products: vec![BehaviourProduct { product: "SAV".into(), category: "Retail non-transactional".into(), core_pct: 60.0, avg_maturity_years: 3.0, method: "Typed".into(), ..BehaviourProduct::default() }] },
        product_cols: vec!["Product".into()],
    };
    let spec = GapInputSpec { role: GapRole::Liability, amount_col: "Balance".into(), maturity_col: "Matdat".into() };
    let with = build_gap_ladder_behaviour(vec![(book.clone().lazy(), spec.clone())], &preset, "2026-10-31", true, None, Some(&beh)).expect("ladder");
    let without = build_gap_ladder(vec![(book.clone().lazy(), spec)], &preset, "2026-10-31", true, None).expect("ladder");
    check_guide(
        "behaviour-profile",
        "guide_behaviour_profile",
        &[
            md(&history, &["Month-end", "Product", "Balance:2"]),
            md(&rows, &["Category", "Method", "Core %:2", "Capped", "Months"]),
            md(&with, &["Bucket", "Outflows:2", "Net gap:2"]),
            md(&without, &["Bucket", "Outflows:2", "Net gap:2"]),
        ],
    );
}
