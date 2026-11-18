//! Phase 5 guide notes: one `docs/guide/<tool>.md` per new tool, checked like the Phase 4 ones
//! (ten sections, proven worked examples). `MP_GUIDE_DUMP=<folder> cargo test guide_` writes the computed tables.

use crate::common::*;
use crate::guide::{check_guide, check_guide_file, read_guide};
use micro_parquet::banking::GapRole;
use micro_parquet::cashflow::*;
use polars::prelude::*;

/// The tool ids of Phase 5 (file names in docs/guide/), added as each task is done.
pub const P5_GUIDE_TOOLS: [&str; 1] = ["irrbb-cashflow-ladder"];

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
