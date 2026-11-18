//! Phase 5 guide notes: one `docs/guide/<tool>.md` per new tool, checked like the Phase 4 ones
//! (ten sections, proven worked examples). `MP_GUIDE_DUMP=<folder> cargo test guide_` writes the computed tables.

use crate::common::*;
use crate::guide::{check_guide, check_guide_file, read_guide};
use micro_parquet::banking::GapRole;
use micro_parquet::cashflow::*;
use polars::prelude::*;

/// The tool ids of Phase 5 (file names in docs/guide/), added as each task is done.
pub const P5_GUIDE_TOOLS: [&str; 6] = ["irrbb-cashflow-ladder", "behaviour-profile", "irrbb-six-scenarios", "classify-by-overdue", "cl1-summary", "lcr-nsfr"];

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
    let settings = CfSettings { cutoff: d("2026-10-31"), mode: CashFlowMode::PrincipalOnly, scenario: Scenario::Base, blank_maturity_on: false };
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
    let rf = generate_flows(&r, &CfSettings { cutoff: d("2026-10-31"), mode: CashFlowMode::PrincipalAndInterest, scenario: Scenario::Base, blank_maturity_on: false }, None).expect("flows");
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
    // a floating loan with equal principal: it pays its instalments until the repricing date, then everything
    let mut fl = CfRow::simple(Side::Asset, 1_200_000.0, d("2027-10-31"));
    fl.rate_pct = 12.0;
    fl.rate_type = RateType::Floating;
    fl.reprice = Some(d("2027-01-31"));
    fl.repayment = Repayment::EqualPrincipal;
    fl.frequency = Frequency::Monthly;
    let rf = generate_flows(&fl, &CfSettings { cutoff: d("2026-10-31"), mode: CashFlowMode::PrincipalAndInterest, scenario: Scenario::Base, blank_maturity_on: false }, None).expect("flows");
    assert_eq!(rf.flows.len(), 3);
    let floating = DataFrame::new(vec![
        Series::new("Payment date".into(), rf.flows.iter().map(|f| f.date.format("%Y-%m-%d").to_string()).collect::<Vec<_>>()),
        Series::new("Principal".into(), rf.flows.iter().map(|f| f.principal).collect::<Vec<_>>()),
        Series::new("Interest".into(), rf.flows.iter().map(|f| f.interest).collect::<Vec<_>>()),
        Series::new("Payment".into(), rf.flows.iter().map(|f| f.total()).collect::<Vec<_>>()),
    ])
    .expect("floating frame");
    check_guide(
        "irrbb-cashflow-ladder",
        "guide_irrbb_cashflow_ladder",
        &[
            md(&book, &["Account_No", "Side", "Outstanding:2", "Rate:2", "Matdat"]),
            md(&shown, &["Currency", "Bucket", "Midpoint (years):~4", "Assets:2", "Liabilities:2", "Net:2", "Cumulative net:2"]),
            md(&flows, &["Payment date", "Principal:2", "Interest:2", "Payment:2"]),
            md(&floating, &["Payment date", "Principal:2", "Interest:2", "Payment:2"]),
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

#[test]
fn guide_irrbb_six_scenarios() {
    use micro_parquet::irrbb::*;
    use micro_parquet::models::FtpPoint;
    use micro_parquet::rules::{CurrencyShock, RuleFile, RuleMeta, ShockFile};
    let book = DataFrame::new(vec![
        Series::new("Account_No".into(), &["A1", "L1"]),
        Series::new("Side".into(), &["Asset", "Liability"]),
        Series::new("Outstanding".into(), &[1_000_000.0, 600_000.0]),
        dates("Matdat", &["2031-04-30", "2026-10-31"]),
    ])
    .expect("book");
    let cf = CfInputSpec {
        role: GapRole::Side("Side".into()),
        principal_col: "Outstanding".into(),
        rate_col: None,
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
    let shocks = RuleFile {
        data: ShockFile { decay_x: 4.0, outlier_pct: 15.0, currencies: vec![CurrencyShock { currency: "BDT".into(), parallel_bp: 400.0, short_bp: 500.0, long_bp: 300.0 }] },
        meta: RuleMeta { file: "irrbb_shocks/2021-04-13.json".into(), as_of: d("2021-04-13"), source: "BRPD IRRBB 2021".into(), sample: false },
    };
    let curve = micro_parquet::curves::YieldCurve { date: d("2026-10-01"), currency: "BDT".into(), source: "example".into(), points: vec![FtpPoint { tenor_days: 30, ftp_rate: 10.0 }, FtpPoint { tenor_days: 9000, ftp_rate: 10.0 }] };
    let spec = |tier1: f64| IrrbbSpec { cutoff: d("2026-10-31"), mode: CashFlowMode::PrincipalOnly, blank_maturity_on: false, tier1, compounding: Compounding::Continuous, post_shock_floor_pct: None, nii_floor_pct: None, shocks: shocks.clone(), curves: vec![curve.clone()], behaviour: None, betas: vec![] };
    let o = run_irrbb(vec![(book.clone().lazy(), cf.clone())], &spec(1_000_000.0)).expect("runs");
    let small = run_irrbb(vec![(book.clone().lazy(), cf)], &spec(600_000.0)).expect("runs");
    assert!(!o.outlier && small.outlier);
    close(Some(o.max_loss), 104_969.17, 0.01, "max loss");
    close(o.base_eve("BDT"), 37_796.13, 0.05, "base EVE");
    let cash = cashflows_frame(&o).expect("cash flows");
    let keep: BooleanChunked = strings(&cash, "Bucket").iter().zip(floats(&cash, "Net")).map(|(b, n)| Some(n.unwrap_or(0.0) != 0.0 || b.as_deref() == Some("Base EVE"))).collect();
    let cash_shown = cash.filter(&keep).expect("filter");
    let results = results_frame(&o).expect("results");
    let results_small = results_frame(&small).expect("results");
    let pick = |df: &DataFrame, labels: &[&str]| -> DataFrame {
        let sc = strings(df, "Scenario");
        let m: BooleanChunked = sc.iter().map(|s| Some(labels.iter().any(|l| s.as_deref().unwrap_or("").starts_with(l)))).collect();
        df.filter(&m).expect("filter")
    };
    let six = pick(&results, &["Parallel up", "Parallel down", "Steepener", "Flattener", "Short up", "Short down", "Maximum loss", "Outlier"]);
    let six_small = pick(&results_small, &["Maximum loss", "Outlier"]);
    check_guide(
        "irrbb-six-scenarios",
        "guide_irrbb_six_scenarios",
        &[
            md(&book, &["Account_No", "Side", "Outstanding:2", "Matdat"]),
            md(&cash_shown, &["Bucket", "Midpoint (years)", "Net:2", "Discount factor:~6", "Base PV:2"]),
            md(&six, &["Scenario", "EVE change:2", "Loss:2", "Loss % of Tier 1:2", "\u{394}NII:2", "Note"]),
            md(&six_small, &["Scenario", "Loss:2", "Loss % of Tier 1:2", "Note"]),
        ],
    );
}

#[test]
fn guide_classify_by_overdue() {
    use micro_parquet::models::TransformStep;
    let book = DataFrame::new(vec![
        Series::new("Account_No".into(), &["A1", "A2", "A3", "A4", "A5"]),
        Series::new("Loan_Category".into(), &["Continuous", "Demand", "Fixed term", "Fixed term", "Continuous"]),
        dates("Overdue_Since", &["", "2026-10-15", "2026-08-31", "2026-07-31", "2026-04-30"]),
        Series::new("Override_Grade".into(), &[None, None, None, Some("DF"), Some("STD-0")]),
    ])
    .expect("book");
    let step = TransformStep::ClassifyOverdue {
        category_col: Some("Loan_Category".into()),
        fixed_category: String::new(),
        since_col: Some("Overdue_Since".into()),
        dpd_col: None,
        override_col: Some("Override_Grade".into()),
        cutoff_date: "2026-10-31".into(),
    };
    micro_parquet::rules::set_data_dir_override(Some(dir("guide_overdue")));
    let out = run_steps("guide_overdue", &book, std::slice::from_ref(&step)).expect("classifies");
    micro_parquet::rules::set_data_dir_override(None);
    assert_eq!(strings(&out, "CL_Final"), ["STD-0", "STD-1", "SMA", "DF", "DF"].iter().map(|s| Some(s.to_string())).collect::<Vec<_>>());
    check_guide(
        "classify-by-overdue",
        "guide_classify_by_overdue",
        &[
            md(&book, &["Account_No", "Loan_Category", "Overdue_Since", "Override_Grade"]),
            md(&out, &["Account_No", "Overdue_Months:0", "CL_Objective", "CL_Final", "CL_Rule_File"]),
        ],
    );
}

#[test]
fn guide_cl1_summary() {
    use micro_parquet::cl1::*;
    let book = DataFrame::new(vec![
        Series::new("Account_No".into(), &["A1", "A2", "A3", "A4", "A5", "A6"]),
        Series::new("Loan_Category".into(), &["Continuous", "Continuous", "Demand", "Fixed term", "Fixed term", "Short-term agri & micro"]),
        Series::new("CL_Final".into(), &["STD-0", "STD-2", "SMA", "SS", "B/L", "DF"]),
        Series::new("Balance".into(), &[1000.0, 2000.0, 500.0, 300.0, 100.0, 400.0]),
        Series::new("Base_For_Provision_BDT".into(), &[1000.0, 2000.0, 500.0, 250.0, 100.0, 300.0]),
        Series::new("Required_Provision_BDT".into(), &[10.0, 20.0, 25.0, 50.0, 100.0, 150.0]),
    ])
    .expect("book");
    let spec = Cl1Spec {
        category_col: Some("Loan_Category".into()),
        fixed_category: String::new(),
        grade_col: "CL_Final".into(),
        measure_cols: vec![("outstanding".into(), "Balance".into()), ("base".into(), "Base_For_Provision_BDT".into()), ("provision".into(), "Required_Provision_BDT".into())],
    };
    let data = dir("guide_cl1");
    micro_parquet::rules::set_data_dir_override(Some(data.clone()));
    let template = load_template().expect("the shipped template");
    let cat = micro_parquet::overdue::load_category_map(&data).expect("category map");
    let cl = micro_parquet::banking::ProvisionConfig::load_or_create(&data).expect("cl map").cl_code_map;
    micro_parquet::rules::set_data_dir_override(None);
    let out = build_cl1_frame(&book, &spec, &template, &cat, &cl).expect("the summary");
    // the guide writes "STD: Outstanding" for the column "STD | Outstanding" (a bar would split a Markdown table)
    let mut out = out;
    let names: Vec<String> = out.get_column_names().iter().map(|n| n.to_string()).collect();
    for n in names {
        out.rename(&n, n.replace(" | ", ": ").into()).expect("rename");
    }
    check_guide(
        "cl1-summary",
        "guide_cl1_summary",
        &[
            md(&book, &["Account_No", "Loan_Category", "CL_Final", "Balance:2", "Base_For_Provision_BDT:2", "Required_Provision_BDT:2"]),
            md(&out, &["Loan category", "STD: Outstanding:2", "SMA: Outstanding:2", "SS: Outstanding:2", "DF: Outstanding:2", "B/L: Outstanding:2", "Total: Outstanding:2"]),
            md(&out, &["Loan category", "Total: Base for provision:2", "Total: Provision required:2"]),
        ],
    );
}

#[test]
fn guide_lcr_nsfr() {
    use micro_parquet::liquidity::*;
    let mut gl = DataFrame::new(vec![
        Series::new("GL_Code".into(), &["101000", "103000", "104000", "201000", "202000", "203000", "115000", "300100"]),
        Series::new("Balance".into(), &[1000.0, 400.0, 100.0, -10_000.0, -10_000.0, -2_500.0, 3_600.0, -500.0]),
    ])
    .expect("gl");
    gl.with_column(Series::new("Residual_Months".into(), &[None, None, None, None, None, None, Some(24.0), None])).expect("col");
    let data = dir("guide_lcr");
    micro_parquet::rules::set_data_dir_override(Some(data));
    let map = load_map("sample_basel").expect("the shipped sample");
    micro_parquet::rules::set_data_dir_override(None);
    let spec = LiqSpec { gl_col: "GL_Code".into(), balance_col: "Balance".into(), sign: BalanceSign::DebitPositive, maturity_col: Some("Residual_Months".into()), maturity_unit: MaturityUnit::Months };
    let ev = run_df(&gl, &spec, &map, "liquidity_maps/sample_basel.json").expect("runs");
    assert!((ev.hqla - 1390.0).abs() < 0.005 && (ev.nco - 700.0).abs() < 0.005);
    assert!((ev.lcr.expect("defined") - 198.5714).abs() < 0.001 && (ev.nsfr.expect("defined") - 661.7647).abs() < 0.001);
    let summary = summary_frame(&ev, &map, "liquidity_maps/sample_basel.json").expect("summary");
    let keep: BooleanChunked = summary.column("Value").expect("c").f64().expect("f").into_iter().map(|v| Some(v.is_some())).collect();
    let numbers = summary.filter(&keep).expect("filter");
    let lines = lines_frame(&ev).expect("lines");
    let keep: BooleanChunked = lines.column("Amount").expect("c").f64().expect("f").into_iter().map(|v| Some(v.unwrap_or(0.0) != 0.0)).collect();
    let used = lines.filter(&keep).expect("filter");
    check_guide(
        "lcr-nsfr",
        "guide_lcr_nsfr",
        &[
            md(&gl, &["GL_Code", "Balance:2", "Residual_Months:~0"]),
            md(&used, &["Ratio", "Line", "Residual maturity", "Amount:2", "Factor %:2", "Weighted amount:2"]),
            md(&numbers, &["Item", "Value:2", "Unit"]),
        ],
    );
}
