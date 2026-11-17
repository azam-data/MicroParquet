//! G0: guide notes for the banking and data tools (docs/guide/).
//!
//! Every tool has one `docs/guide/<tool>.md` with the ten sections of `<guide_notes>` in the Phase 4
//! spec. Section 8 (Worked example) holds Markdown tables, and each table is followed by a line
//! `Test: \`guide_<tool>\``. The `guide_<tool>` test below computes the same tables from the real
//! engine, asserts the numbers, and compares them with the guide, so an example in a guide is a proven one.
//! `MP_GUIDE_DUMP=<folder> cargo test guide_` writes the computed tables to `<folder>/<tool>.txt`
//! (instead of comparing) so a guide author can paste them.

use crate::common::*;
use micro_parquet::banking::*;
use micro_parquet::models::*;
use polars::prelude::*;
use std::path::PathBuf;

pub const SECTIONS: [&str; 10] = [
    "1. What it does",
    "2. When to use it",
    "3. Before you start",
    "4. Columns you need",
    "5. Step by step",
    "6. What you get",
    "7. Check your result",
    "8. Worked example",
    "9. Common messages and fixes",
    "10. Limits and assumptions",
];

pub const COLUMN_TABLE_HEADER: &str = "| Column role | Required? | Type the app needs | Example values | Notes |";

/// Tool ids (file names in docs/guide/): the 21 tools of Phase 4 task G0, then the tools later tasks added.
pub const GUIDE_TOOLS: [&str; 24] = [
    "maturity-bucketer",
    "rate-profile",
    "rate-shock-nii",
    "duration-dv01-eve",
    "ftp-curve-mapper",
    "cl-provisioning",
    "limit-utilisation",
    "hygiene-audit",
    "name-normaliser",
    "period-reconcile",
    "conditional-column",
    "calc-column",
    "filter",
    "sort",
    "summary-row",
    "column-inspector",
    "schema-ingestion",
    "stack",
    "join",
    "split-export",
    "batch",
    "pivot",
    "cl-migration",
    "gap-ladder",
];

pub fn guide_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs").join("guide")
}

pub fn read_guide(tool: &str) -> String {
    let p = guide_dir().join(format!("{tool}.md"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("the guide {p:?} cannot be read: {e}"))
}

/// The text of one section ("8. Worked example"): the lines after its `## ` heading up to the next one.
pub fn section(text: &str, heading: &str) -> Option<String> {
    let mut lines = text.lines();
    let want = format!("## {heading}");
    lines.by_ref().find(|l| l.trim_end() == want)?;
    let mut body = Vec::new();
    for l in lines {
        if l.starts_with("## ") {
            break;
        }
        body.push(l);
    }
    Some(body.join("\n"))
}

/// The Markdown tables of a text (runs of lines that start with `|`) and, for each, the next
/// non-blank line after it.
pub fn tables_with_followers(body: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = body.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with('|') {
            let start = i;
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                i += 1;
            }
            let table = lines[start..i].iter().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n");
            let follower = lines[i..].iter().find(|l| !l.trim().is_empty()).map(|l| l.to_string()).unwrap_or_default();
            out.push((table, follower));
        } else {
            i += 1;
        }
    }
    out
}

/// Compares the tables a test computed with the guide's section 8; each must be followed by a line
/// that names `test`. With `MP_GUIDE_DUMP` set, writes the tables there instead.
pub fn check_guide(tool: &str, test: &str, computed: &[String]) {
    if let Ok(dump) = std::env::var("MP_GUIDE_DUMP") {
        std::fs::create_dir_all(&dump).expect("create the dump folder");
        std::fs::write(PathBuf::from(&dump).join(format!("{tool}.txt")), computed.join("\n\n===\n\n")).expect("write the dump");
        return;
    }
    let text = read_guide(tool);
    let body = section(&text, "8. Worked example").unwrap_or_else(|| panic!("{tool}: the guide has no '## 8. Worked example'"));
    let in_guide = tables_with_followers(&body);
    assert_eq!(in_guide.len(), computed.len(), "{tool}: the guide has {} tables in section 8, the test computed {}", in_guide.len(), computed.len());
    for (i, ((table, follower), want)) in in_guide.iter().zip(computed).enumerate() {
        assert_eq!(table, want, "{tool}: table {} of section 8 differs from what `{test}` computes", i + 1);
        assert!(follower.contains(&format!("`{test}`")), "{tool}: table {} must be followed by a line naming `{test}`, found {follower:?}", i + 1);
    }
}

// ----------------------------------------------------------------------------------- the checker

fn all_test_sources() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
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

/// The names in backticks on a "Test:" line.
fn test_names(line: &str) -> Vec<String> {
    line.split('`').enumerate().filter(|(i, _)| i % 2 == 1).map(|(_, s)| s.to_string()).collect()
}

/// Checks one guide file: the title, the ten headings in order, the column table header, a worked
/// example whose every table is followed by a "Test:" line naming a test that exists.
pub fn check_guide_file(tool: &str, sources: &str) {
    let text = read_guide(tool);
    assert!(text.lines().next().map(|l| l.starts_with("# ")).unwrap_or(false), "{tool}: the first line must be a '# Title'");
    let headings: Vec<&str> = text.lines().filter(|l| l.starts_with("## ")).map(|l| l.trim_end()).collect();
    let want: Vec<String> = SECTIONS.iter().map(|s| format!("## {s}")).collect();
    assert_eq!(headings, want.iter().map(|s| s.as_str()).collect::<Vec<_>>(), "{tool}: the ten section headings must be present, in this order");
    for s in SECTIONS {
        let body = section(&text, s).unwrap_or_default();
        assert!(!body.trim().is_empty(), "{tool}: section '{s}' is empty");
    }
    let cols = section(&text, "4. Columns you need").unwrap_or_default();
    assert!(cols.lines().any(|l| l.trim() == COLUMN_TABLE_HEADER), "{tool}: section 4 needs the table header `{COLUMN_TABLE_HEADER}`");
    let example = section(&text, "8. Worked example").unwrap_or_default();
    let tables = tables_with_followers(&example);
    assert!(!tables.is_empty(), "{tool}: section 8 has no table");
    for (i, (_, follower)) in tables.iter().enumerate() {
        let names = test_names(follower);
        assert!(follower.trim_start().starts_with("Test:") && !names.is_empty(), "{tool}: table {} of section 8 must be followed by 'Test: `name`', found {follower:?}", i + 1);
        for n in names {
            assert!(sources.contains(&format!("fn {n}(")), "{tool}: the test `{n}` named under table {} does not exist in tests/", i + 1);
        }
    }
    let lines = text.lines().count();
    assert!(lines <= 150, "{tool}: {lines} lines is more than about two pages");
}

#[test]
fn every_g0_tool_has_a_complete_guide_with_proven_examples() {
    let sources = all_test_sources();
    for tool in GUIDE_TOOLS {
        check_guide_file(tool, &sources);
        assert!(sources.contains(&format!("fn guide_{}(", tool.replace('-', "_"))), "{tool}: there is no test guide_{}", tool.replace('-', "_"));
    }
}

#[test]
fn the_guide_readme_lists_every_tool_under_the_four_groups() {
    let readme = read_guide("README");
    let mut last = 0usize;
    for group in ["Data preparation", "Treasury & ALM", "Loans & credit", "Export & automation"] {
        let at = readme.find(&format!("## {group}")).unwrap_or_else(|| panic!("README: no '## {group}' group"));
        assert!(at >= last, "README: the groups must come in the order Data preparation, Treasury & ALM, Loans & credit, Export & automation");
        last = at;
    }
    for tool in GUIDE_TOOLS {
        assert!(readme.contains(&format!("({tool}.md)")), "README: no link to {tool}.md");
    }
}

// ------------------------------------------------------------------------------------ examples

fn frame(cols: Vec<Series>) -> DataFrame {
    DataFrame::new(cols).expect("a test frame")
}

fn text(name: &str, v: &[&str]) -> Series {
    Series::new(name.into(), v).into()
}

fn nums(name: &str, v: &[f64]) -> Series {
    Series::new(name.into(), v).into()
}

fn opt_nums(name: &str, v: &[Option<f64>]) -> Series {
    Series::new(name.into(), v).into()
}

#[test]
fn guide_maturity_bucketer() {
    let df = frame(vec![
        text("Actnum", &["D1", "D2", "D3", "D4", "D5"]),
        nums("Balance", &[100.0, 200.0, 300.0, 400.0, 500.0]),
        dates("Matdat", &["2026-11-05", "2027-03-15", "2031-01-01", "", "2026-10-15"]),
    ]);
    let step = TransformStep::MaturityBucket {
        mat_col: "Matdat".into(),
        cutoff_date: "2026-10-31".into(),
        bucket_def: CustomBucketDef::default_presets()[0].clone(),
        null_as_sight: true,
        new_col: "Maturity_Bucket".into(),
    };
    let out = run_steps("g_bucket", &df, &[step]).expect("the bucketer runs");
    // the same buckets as review.rs t05_parquet_datetime_maturity_bucket; the overdue row is new
    assert_eq!(
        strings(&out, "Maturity_Bucket"),
        vec![
            Some("02. 2 to 7 Days".to_string()),
            Some("05. 3 to 6 Months (91-180d)".to_string()),
            Some("08. 3 to 5 Years".to_string()),
            Some("01. Overnight / Sight (Non-Maturing)".to_string()),
            Some("10. Overdue / Matured Unencashed".to_string()),
        ]
    );
    check_guide("maturity-bucketer", "guide_maturity_bucketer", &[md(&df, &["Actnum", "Balance:2", "Matdat"]), md(&out, &["Actnum", "Maturity_Bucket"])]);
}

#[test]
fn guide_rate_profile() {
    let df = frame(vec![text("Actnum", &["L1", "L2", "L3", "L4", "L5"]), opt_nums("Rate", &[Some(9.0), Some(8.5), Some(0.0), Some(15.5), None])]);
    let step = TransformStep::RateBand { rate_col: "Rate".into(), band_def: RateBandDef::default_profile(), new_col: "Rate_Band".into() };
    let out = run_steps("g_rate_band", &df, &[step]).expect("the rate profile runs");
    assert_eq!(
        strings(&out, "Rate_Band"),
        vec![
            Some("09. 8%\u{2013}9%".to_string()),
            Some("09. 8%\u{2013}9%".to_string()),
            Some("00. 0%".to_string()),
            Some("> 15%".to_string()),
            Some("No rate".to_string()),
        ],
        "same bands as p4_6_default_bands_are_upper_inclusive_and_labelled_so_they_sort"
    );
    check_guide("rate-profile", "guide_rate_profile", &[md(&df, &["Actnum", "Rate:2"]), md(&out, &["Actnum", "Rate_Band"])]);
}

#[test]
fn guide_rate_shock_nii() {
    let df = frame(vec![
        text("Product", &["Loan 3m", "Term deposit 3m"]),
        text("Side", &["Asset", "Deposit"]),
        nums("Balance", &[1_000_000.0, 1_000_000.0]),
        nums("Rate", &[12.0, 9.0]),
        dates("Matdat", &["2027-01-29", "2027-01-29"]),
    ]);
    let step = TransformStep::RateShockSim {
        bal_col: "Balance".into(),
        rate_col: "Rate".into(),
        mat_col: "Matdat".into(),
        cutoff_date: "2026-10-31".into(),
        shock_bps: 100.0,
        side_col: Some("Side".into()),
    };
    let out = run_steps("g_nii", &df, &[step]).expect("the rate shock runs");
    // the figures of review.rs t10_nii_shock_signs
    let nii = floats(&out, "NII_Impact_BDT");
    close(nii[0], 7_534.25, 0.01, "loan ΔNII");
    close(nii[1], -7_534.25, 0.01, "deposit ΔNII");
    close(floats(&out, "Shocked_Annual_Int")[1], 97_534.25, 0.01, "deposit interest after the shock");
    check_guide(
        "rate-shock-nii",
        "guide_rate_shock_nii",
        &[md(&df, &["Product", "Side", "Balance:2", "Rate:2", "Matdat"]), md(&out, &["Product", "Current_Annual_Int:2", "NII_Impact_BDT:2", "Shocked_Annual_Int:2"])],
    );
}

#[test]
fn guide_duration_dv01_eve() {
    let df = frame(vec![
        text("Product", &["Bond (asset)", "Deposit"]),
        text("Side", &["Asset", "Deposit"]),
        nums("Balance", &[1_000_000.0, 1_000_000.0]),
        nums("Rate", &[10.0, 10.0]),
        dates("Matdat", &["2031-10-30", "2031-10-30"]),
    ]);
    let step = TransformStep::DurationAnalysis {
        bal_col: "Balance".into(),
        rate_col: "Rate".into(),
        mat_col: "Matdat".into(),
        cutoff_date: "2026-10-31".into(),
        compounding_freq: 1,
        shock_bps: 200.0,
        default_sight_years: 0.0833,
        side_col: Some("Side".into()),
    };
    let out = run_steps("g_dur", &df, &[step]).expect("the duration runs");
    // review.rs t09_duration_and_eve: Macaulay 4.1699, modified 3.7908 for a 5-year 10% annual bond
    close(floats(&out, "Macaulay_Duration_Yrs")[0], 4.1699, 1e-4, "Macaulay");
    close(floats(&out, "Modified_Duration")[0], 3.7908, 1e-4, "modified");
    close(floats(&out, "Delta_EVE_Impact_BDT")[0], -75_816.0, 0.01, "asset ΔEVE");
    close(floats(&out, "Delta_EVE_Impact_BDT")[1], 75_816.0, 0.01, "deposit ΔEVE");
    check_guide(
        "duration-dv01-eve",
        "guide_duration_dv01_eve",
        &[
            md(&df, &["Product", "Side", "Balance:2", "Rate:2", "Matdat"]),
            md(&out, &["Product", "Residual_Years_T", "Macaulay_Duration_Yrs", "Modified_Duration", "DV01_BPV_BDT:2", "Delta_EVE_Impact_BDT:2"]),
        ],
    );
}

#[test]
fn guide_ftp_curve_mapper() {
    let df = frame(vec![
        text("Account", &["D1", "D2"]),
        dates("Opndat", &["2026-01-01", "2026-01-01"]),
        dates("Matdat", &["2026-04-11", "2026-02-01"]),
        nums("Balance", &[1_000_000.0, 1_000_000.0]),
        nums("Rate", &[9.0, 9.0]),
    ]);
    let step = TransformStep::FtpCurveMap {
        opndat_col: "Opndat".into(),
        matdat_col: "Matdat".into(),
        bal_col: "Balance".into(),
        rate_col: "Rate".into(),
        curve: default_bd_ftp_curve(),
        is_liability: true,
        interpolation: FtpInterpolation::Linear,
    };
    let out = run_steps("g_ftp", &df, &[step]).expect("the FTP mapper runs");
    // review.rs t11_ftp_step_vs_interpolation: 100 days = 9.50 + 9/91 × 0.60, 31 days = 8.75 + 1/61 × 0.75
    close(floats(&out, "Benchmark_FTP_Rate")[0], 9.50 + 9.0 / 91.0 * 0.60, 1e-9, "100-day FTP");
    close(floats(&out, "Benchmark_FTP_Rate")[1], 8.75 + 1.0 / 61.0 * 0.75, 1e-9, "31-day FTP");
    assert_eq!(strings(&out, "FTP_Method"), vec![Some("Linear 91d-182d".to_string()), Some("Linear 30d-91d".to_string())]);
    check_guide(
        "ftp-curve-mapper",
        "guide_ftp_curve_mapper",
        &[
            md(&df, &["Account", "Opndat", "Matdat", "Balance:2", "Rate:2"]),
            md(&out, &["Account", "Tenor_Days", "Benchmark_FTP_Rate:~4", "FTP_Method", "FTP_Spread_Pct:~4", "Annual_FTP_Margin_BDT:2"]),
        ],
    );
}

#[test]
fn guide_cl_provisioning() {
    let df = frame(vec![
        text("Loan", &["L1", "L2", "L3", "L4", "L5"]),
        nums("Balance", &[1_000_000.0; 5]),
        text("CL", &["UC", "SS", "B/L", "STD-1", "BL"]),
        text("Seg", &["CORPORATE", "CORPORATE", "CORPORATE", "SMALL", "CORPORATE"]),
        nums("Susp", &[0.0, 100_000.0, 0.0, 0.0, 0.0]),
        nums("Sec", &[0.0, 0.0, 950_000.0, 0.0, 950_000.0]),
        Series::new("Coll".into(), &[None, None, None, None, Some("DEPOSIT_LIEN")]).into(),
    ]);
    let cfg = ProvisionConfig::defaults();
    let out = apply_loan_classification_provision_with(df.clone().lazy(), "Balance", "CL", Some("Seg"), Some("Susp"), Some("Sec"), Some("Coll"), "2026-10-31", &cfg)
        .expect("the provision runs")
        .collect()
        .expect("collect");
    // review.rs t07_provisioning: UC 10,000; SS 180,000; BL with security 150,000 (15% floor); STD-1 SMALL 5,000; deposit lien 50,000
    let prov = floats(&out, "Required_Provision_BDT");
    for (i, e) in [10_000.0, 180_000.0, 150_000.0, 5_000.0, 50_000.0].iter().enumerate() {
        close(prov[i], *e, 0.005, &format!("loan {} provision", i + 1));
    }
    check_guide(
        "cl-provisioning",
        "guide_cl_provisioning",
        &[
            md(&df, &["Loan", "Balance:2", "CL", "Seg", "Susp:2", "Sec:2", "Coll"]),
            md(&out, &["Loan", "CL_Grade", "Reg_Category", "Base_For_Provision_BDT:2", "Statutory_Prov_Rate_Pct:~2", "Required_Provision_BDT:2"]),
        ],
    );
}

#[test]
fn guide_limit_utilisation() {
    let df = frame(vec![
        text("Actnum", &["C1", "C2", "C3", "C4"]),
        nums("Sanctioned_Limit", &[1_000_000.0, 500_000.0, 200_000.0, 1_000_000.0]),
        nums("Balance", &[500_000.0, 520_000.0, 196_000.0, 50_000.0]),
    ]);
    let out = apply_loan_limit_utilization(df.clone().lazy(), "Sanctioned_Limit", "Balance", 20.0).expect("limits").collect().expect("collect");
    assert_eq!(floats(&out, "Limit_Utilization_Pct"), vec![Some(50.0), Some(104.0), Some(98.0), Some(5.0)]);
    assert_eq!(floats(&out, "Over_Limit_Violation_BDT"), vec![Some(0.0), Some(20_000.0), Some(0.0), Some(0.0)]);
    assert_eq!(floats(&out, "CCF_Credit_Equivalent_BDT"), vec![Some(100_000.0), Some(0.0), Some(800.0), Some(190_000.0)]);
    assert_eq!(
        strings(&out, "Limit_Audit_Status"),
        vec![
            Some("4. NORMAL UTILIZATION".to_string()),
            Some("1. OVER-LIMIT BREACH".to_string()),
            Some("2. NEAR FULL UTILIZATION (>95%)".to_string()),
            Some("3. DORMANT / LOW UTILIZATION (<10%)".to_string()),
        ]
    );
    check_guide(
        "limit-utilisation",
        "guide_limit_utilisation",
        &[
            md(&df, &["Actnum", "Sanctioned_Limit:2", "Balance:2"]),
            md(&out, &["Actnum", "Limit_Utilization_Pct:2", "Undrawn_Commitment_BDT:2", "Over_Limit_Violation_BDT:2", "CCF_Credit_Equivalent_BDT:2", "Limit_Audit_Status"]),
        ],
    );
}

#[test]
fn guide_hygiene_audit() {
    let df = frame(vec![
        text("Actnum", &["H1", "H2", "H3", "H4", "H5"]),
        dates("Opndat", &["2025-01-10", "2026-05-01", "2026-01-01", "2026-01-01", "2024-06-01"]),
        dates("Matdat", &["2026-09-30", "2026-12-01", "2027-01-01", "2027-06-01", "2026-06-30"]),
        dates("Clsdat", &["", "2026-04-01", "", "", "2026-06-30"]),
        text("Acstat", &["ACT", "ACT", "ACT", "ACT", "CLS"]),
        nums("Balance", &[500_000.0, 100_000.0, -2_000.0, 300_000.0, 0.0]),
        nums("Rate", &[9.0, 60.0, 9.5, 9.0, 8.0]),
    ]);
    let out = apply_banking_hygiene_audit(df.clone().lazy(), "Opndat", "Matdat", "Clsdat", "Acstat", "Balance", "Rate", "2026-10-31", false, &default_active_statuses(), 0.0, 25.0)
        .expect("the audit runs")
        .collect()
        .expect("collect");
    assert_eq!(
        strings(&out, "Audit_Exception_Flag"),
        vec![
            Some("1. GHOST_MATURED_ACTIVE".to_string()),
            Some("2. BACKWARDS_DATE_ANOMALY".to_string()),
            Some("4. NEGATIVE_BALANCE".to_string()),
            Some("OK".to_string()),
            Some("OK".to_string()),
        ]
    );
    assert_eq!(strings(&out, "Audit_All_Flags")[1].as_deref(), Some("2. BACKWARDS_DATE_ANOMALY; 3. INTEREST_RATE_OUTLIER"));
    check_guide(
        "hygiene-audit",
        "guide_hygiene_audit",
        &[
            md(&df, &["Actnum", "Opndat", "Matdat", "Clsdat", "Acstat", "Balance:2", "Rate:2"]),
            md(&out, &["Actnum", "Audit_Exception_Flag", "Audit_All_Flags"]),
        ],
    );
}

#[test]
fn guide_name_normaliser() {
    let df = frame(vec![
        text("Acttit", &["ABC Industries Limited", "abc industries ltd.", "Karim & Sons (Private) Limited", "Bangladesh Steel Company Ltd"]),
    ]);
    let step = TransformStep::NormalizeEntity { source_col: "Acttit".into(), new_col: "Normalized_Entity".into() };
    let out = run_steps("g_norm", &df, &[step]).expect("the normaliser runs");
    assert_eq!(
        strings(&out, "Normalized_Entity"),
        vec![
            Some("ABC INDUSTRIES LTD".to_string()),
            Some("ABC INDUSTRIES LTD".to_string()),
            Some("KARIM & SONS PVT LTD".to_string()),
            Some("BD STEEL CO LTD".to_string()),
        ]
    );
    check_guide("name-normaliser", "guide_name_normaliser", &[md(&df, &["Acttit"]), md(&out, &["Acttit", "Normalized_Entity"])]);
}

#[test]
fn guide_period_reconcile() {
    // review.rs t12_reconcile: A1 has two rows in September (100 @ 5% and 900 @ 10%) that weigh to 9.5%
    let prev = frame(vec![text("Actnum", &["A1", "A1", "A2"]), nums("Balance", &[100.0, 900.0, 500.0]), nums("Rate", &[5.0, 10.0, 8.0])]);
    let curr = frame(vec![text("Actnum", &["A1", "A3"]), nums("Balance", &[1000.0, 50.0]), nums("Rate", &[9.5, 11.0])]);
    let out = build_period_reconcile_lazy_with(prev.clone().lazy(), curr.clone().lazy(), "Actnum", "Balance", Some("Rate"), None, &[])
        .expect("the reconcile runs")
        .sort(["Actnum"], Default::default())
        .collect()
        .expect("collect");
    assert_eq!(
        strings(&out, "Reconcile_Status"),
        vec![Some("5. Unchanged".to_string()), Some("2. Closed / Matured Account".to_string()), Some("1. New Account Opened".to_string())]
    );
    assert_eq!(floats(&out, "Balance_Delta"), vec![Some(0.0), Some(-500.0), Some(50.0)]);
    close(floats(&out, "Prev_Rate")[0], 9.5, 1e-9, "A1 September rate (balance-weighted)");
    check_guide(
        "period-reconcile",
        "guide_period_reconcile",
        &[
            md(&prev, &["Actnum", "Balance:2", "Rate:2"]),
            md(&curr, &["Actnum", "Balance:2", "Rate:2"]),
            md(&out, &["Actnum", "Prev_Balance:2", "Curr_Balance:2", "Balance_Delta:2", "Reconcile_Status", "Prev_Rate:~2", "Curr_Rate:~2", "Rate_Delta_Bps:~2", "Prev_Row_Count", "Curr_Row_Count"]),
        ],
    );
}

#[test]
fn guide_conditional_column() {
    let df = frame(vec![text("Actnum", &["A1", "A2", "A3", "A4"]), opt_nums("Balance", &[Some(8_000_000.0), Some(2_500_000.0), Some(400_000.0), None])]);
    let rule = |v: &str, out: &str| CondRule { col_name: "Balance".into(), op: FilterOp::Gt, compare_val: v.into(), output_val: out.into(), compare_is_col: false };
    let step = TransformStep::ConditionalColumn {
        new_col: "Size".into(),
        rules: vec![rule("5000000", "Large"), rule("1000000", "Medium")],
        fallback: "Small".into(),
        output_as_float: false,
        blank_result: BlankRule::default(),
    };
    let out = run_steps("g_cond", &df, &[step]).expect("the conditional column runs");
    assert_eq!(strings(&out, "Size"), vec![Some("Large".to_string()), Some("Medium".to_string()), Some("Small".to_string()), Some("Small".to_string())]);
    check_guide("conditional-column", "guide_conditional_column", &[md(&df, &["Actnum", "Balance:2"]), md(&out, &["Actnum", "Balance:2", "Size"])]);
}

#[test]
fn guide_calc_column() {
    let df = frame(vec![text("Actnum", &["A1", "A2", "A3", "A4"]), nums("Balance", &[25_000_000.0, 5_000_000.0, 12_500_000.0, 100_000_000.0])]);
    let step = TransformStep::MathFormula {
        new_col: "Balance_Cr".into(),
        left_col: "Balance".into(),
        op: MathOp::Div,
        right_is_col: false,
        right_val_or_col: "10,000,000".into(),
    };
    let out = run_steps("g_calc", &df, &[step]).expect("the calc column runs");
    // review.rs t28_control_totals_state_and_math_scalar: a scalar with thousands separators is read as a number
    assert_eq!(floats(&out, "Balance_Cr"), vec![Some(2.5), Some(0.5), Some(1.25), Some(10.0)]);
    check_guide("calc-column", "guide_calc_column", &[md(&df, &["Actnum", "Balance:2"]), md(&out, &["Actnum", "Balance:2", "Balance_Cr:~4"])]);
}

fn loans_frame() -> DataFrame {
    // the loans of phase3 t1: IDs that look like numbers, a blank balance
    frame(vec![
        text("Actnum", &["0012", "0034", "12", "0099", "0100"]),
        text("Brancd", &["DHK", "DHK", "CTG", "CTG", "DHK"]),
        opt_nums("Balance", &[Some(1000.0), Some(2500.0), Some(5000.0), None, Some(8000.0)]),
    ])
}

#[test]
fn guide_filter() {
    let df = loans_frame();
    let cond = |c: &str, op: CondOp, v: &str| FilterCondition { column: c.into(), op, value: v.into(), value2: String::new() };
    let step = TransformStep::FilterGroup {
        conditions: vec![cond("Brancd", CondOp::Equals, "DHK"), cond("Balance", CondOp::Gt, "1500")],
        join: GroupJoin::And,
        keep_matching: true,
    };
    let out = run_steps("g_filter", &df, &[step]).expect("the filter runs");
    // phase3 t1_and_and_or_give_the_expected_rows: DHK and above 1,500 gives 0034 and 0100
    assert_eq!(strings(&out, "Actnum"), vec![Some("0034".to_string()), Some("0100".to_string())]);
    check_guide("filter", "guide_filter", &[md(&df, &["Actnum", "Brancd", "Balance:2"]), md(&out, &["Actnum", "Brancd", "Balance:2"])]);
}

#[test]
fn guide_sort() {
    let df = frame(vec![
        text("Actnum", &["A1", "A2", "A3", "A4", "A5", "A6"]),
        text("Brancd", &["DHK", "CTG", "DHK", "CTG", "Zone 10", "Zone 2"]),
        opt_nums("Balance", &[Some(100.0), Some(300.0), Some(900.0), None, Some(50.0), Some(60.0)]),
    ]);
    let step = TransformStep::SortRows {
        levels: vec![SortLevel { column: "Brancd".into(), descending: false }, SortLevel { column: "Balance".into(), descending: true }],
    };
    let out = run_steps("g_sort", &df, &[step]).expect("the sort runs");
    assert_eq!(
        strings(&out, "Actnum"),
        vec!["A2", "A4", "A3", "A1", "A6", "A5"].into_iter().map(|s| Some(s.to_string())).collect::<Vec<_>>(),
        "branch A to Z (Zone 2 before Zone 10), balance highest first, the blank balance last in its branch"
    );
    check_guide("sort", "guide_sort", &[md(&df, &["Actnum", "Brancd", "Balance:2"]), md(&out, &["Actnum", "Brancd", "Balance:2"])]);
}

#[test]
fn guide_summary_row() {
    use micro_parquet::summary::*;
    let df = frame(vec![
        text("Actnum", &["A1", "A2", "A3"]),
        nums("Balance", &[1000.0, 3000.0, 2000.0]),
        nums("Int Rate", &[10.0, 8.0, 6.0]),
    ]);
    let specs = vec![
        ("Actnum".to_string(), SummarySpec { agg: SummaryAgg::Count, weight_col: None }),
        ("Balance".to_string(), SummarySpec { agg: SummaryAgg::Sum, weight_col: None }),
        ("Int Rate".to_string(), SummarySpec { agg: SummaryAgg::WeightedAvg, weight_col: Some("Balance".into()) }),
    ];
    let cells = compute_summary(df.clone().lazy(), &specs).expect("the summary runs");
    assert_eq!(cells[0].value, Some(SummaryValue::Count(3)));
    assert_eq!(cells[1].value, Some(SummaryValue::Number(6000.0)));
    match &cells[2].value {
        Some(SummaryValue::Number(x)) => assert!((x - 46_000.0 / 6_000.0).abs() < 1e-9, "weighted rate {x}"),
        other => panic!("expected a number, got {other:?}"),
    }
    let mut table = String::from("| Column | Summary | Value |\n| --- | --- | ---: |");
    for c in &cells {
        let shown = match &c.value {
            Some(SummaryValue::Count(n)) => fmt_num(*n as f64, 0),
            Some(SummaryValue::Number(x)) if c.agg == SummaryAgg::WeightedAvg => fmt_num(*x, 4),
            Some(SummaryValue::Number(x)) => money(*x),
            _ => "\u{2014}".to_string(),
        };
        let agg = match &c.weight_col {
            Some(w) => format!("{} by {}", c.agg.label(), w),
            None => c.agg.label().to_string(),
        };
        table.push_str(&format!("\n| {} | {} | {} |", c.column, agg, shown));
    }
    check_guide("summary-row", "guide_summary_row", &[md(&df, &["Actnum", "Balance:2", "Int Rate:2"]), table]);
}

#[test]
fn guide_column_inspector() {
    use micro_parquet::column_stats::*;
    let df = frame(vec![
        text("Brancd", &["DHK", "CTG", "DHK", "DHK", " "]),
        opt_nums("Balance", &[Some(100.0), Some(200.0), Some(300.0), Some(400.0), None]),
    ]);
    let n = compute_column_stats(df.clone().lazy(), "Balance", 0).expect("number stats");
    let ns = n.number.clone().expect("number stats present");
    assert_eq!((n.rows, n.blanks), (5, 1));
    assert_eq!((ns.sum, ns.min, ns.max, ns.mean, ns.distinct), (1000.0, 100.0, 400.0, 250.0, 4));
    assert_eq!(ns.histogram, vec![1, 0, 0, 1, 0, 0, 1, 0, 0, 1], "ten bars of 30 between 100 and 400; the maximum is in the last");
    let t = compute_column_stats(df.clone().lazy(), "Brancd", 0).expect("text stats");
    let ts = t.text.clone().expect("text stats present");
    assert_eq!((t.rows, t.blanks, ts.distinct), (5, 1, 2));
    assert_eq!(ts.top, vec![("DHK".to_string(), 3), ("CTG".to_string(), 1)]);
    let bars: Vec<String> = ns.histogram.iter().map(|b| b.to_string()).collect();
    let table = format!(
        "| Column | Rows | Blank | Sum | Min | Max | Mean | Distinct | Histogram (10 bars) |\n| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |\n| Balance | {} | {} | {} | {} | {} | {} | {} | {} |\n| Brancd | {} | {} | \u{2014} | \u{2014} | \u{2014} | \u{2014} | {} | most frequent: {} |",
        n.rows,
        n.blanks,
        money(ns.sum),
        money(ns.min),
        money(ns.max),
        money(ns.mean),
        ns.distinct,
        bars.join(" "),
        t.rows,
        t.blanks,
        ts.distinct,
        ts.top.iter().map(|(v, c)| format!("{v} ({c})")).collect::<Vec<_>>().join(", ")
    );
    check_guide("column-inspector", "guide_column_inspector", &[md(&df, &["Brancd", "Balance:2"]), table]);
}

fn write_file(d: &std::path::Path, name: &str, body: &str) -> PathBuf {
    let p = d.join(name);
    std::fs::write(&p, body).expect("write a test file");
    p
}

#[test]
fn guide_schema_ingestion() {
    use micro_parquet::engine::*;
    let d = dir("g_schema");
    let p = write_file(&d, "loans.csv", "Actnum,Opndat,Balance\n0012,05/11/2026,\"1,234.50\"\n0034,15/11/2026,\"(500.00)\"\n0099,01/12/2026,\"2,000.00\"\n");
    let cfg = auto_detect_smart_schema(&p).expect("the schema is detected");
    let kind = |name: &str| cfg.columns.iter().find(|c| c.name == name).expect("column").dtype;
    assert_eq!((kind("Actnum"), kind("Opndat"), kind("Balance")), (ColDataType::Utf8, ColDataType::DateDMY, ColDataType::Float64), "leading zeros stay text, D/M/Y dates are preferred");
    let out = build_lazy_pipeline(&p, false, &cfg, &d).expect("the CSV loads").collect().expect("collect");
    assert_eq!(strings(&out, "Actnum"), vec![Some("0012".to_string()), Some("0034".to_string()), Some("0099".to_string())]);
    assert_eq!(floats(&out, "Balance"), vec![Some(1234.5), Some(-500.0), Some(2000.0)], "(500.00) is an accounting negative");
    let raw_table = "| Actnum | Opndat | Balance |\n| --- | --- | --- |\n| 0012 | 05/11/2026 | 1,234.50 |\n| 0034 | 15/11/2026 | (500.00) |\n| 0099 | 01/12/2026 | 2,000.00 |".to_string();
    let types = format!(
        "| Column | Type chosen | Sample read |\n| --- | --- | --- |\n| Actnum | {} | {} |\n| Opndat | {} | {} |\n| Balance | {} | {} |",
        kind("Actnum").label(),
        out.column("Actnum").expect("col").get(0).expect("row").to_string().trim_matches('"'),
        kind("Opndat").label(),
        cell_text(&out, "Opndat"),
        kind("Balance").label(),
        cell_text(&out, "Balance"),
    );
    let _ = std::fs::remove_dir_all(&d);
    check_guide("schema-ingestion", "guide_schema_ingestion", &[raw_table, types, md(&out, &["Actnum", "Opndat", "Balance:2"])]);
}

fn cell_text(df: &DataFrame, name: &str) -> String {
    md(&df.select([name]).expect("select"), &[if name == "Balance" { "Balance:2" } else { name }]).lines().nth(2).unwrap_or_default().trim_matches(|c| c == '|' || c == ' ').to_string()
}

#[test]
fn guide_stack() {
    use micro_parquet::engine::*;
    let d = dir("g_stack");
    let a = write_file(&d, "sep.csv", "Actnum,Balance\n001,100\n");
    let b = write_file(&d, "oct.csv", "Actnum,Balance,Branch\n001,200,DHK\n");
    let files = vec![(a.clone(), false, auto_detect_smart_schema(&a).expect("schema")), (b.clone(), false, auto_detect_smart_schema(&b).expect("schema"))];
    let src = TabSource::VerticalStack { files, include_source_col: true };
    let out = compile_tab_lazy(&src, &[], &d).expect("the stack compiles").collect().expect("collect");
    // review.rs t16_stack_schema_drift: October's extra Branch column gives null for September
    assert_eq!(out.height(), 2);
    assert_eq!(strings(&out, "Branch"), vec![None, Some("DHK".to_string())]);
    assert_eq!(strings(&out, "Source_File"), vec![Some("sep.csv".to_string()), Some("oct.csv".to_string())]);
    // two files whose Balance has different types stop with a message naming the files
    let c = write_file(&d, "nov.csv", "Actnum,Balance\n002,abc\n");
    let (specs, _) = stack_file_configs(&[a.clone(), c.clone()], None).expect("configs");
    let bad = TabSource::VerticalStack { files: specs, include_source_col: true };
    let err = format!("{:#}", compile_tab_lazy(&bad, &[], &d).err().expect("types differ"));
    assert!(err.contains("Balance") && err.contains("sep.csv") && err.contains("nov.csv"), "{err}");
    let in1 = "| File | Actnum | Balance |\n| --- | --- | ---: |\n| sep.csv | 001 | 100 |".to_string();
    let in2 = "| File | Actnum | Balance | Branch |\n| --- | --- | ---: | --- |\n| oct.csv | 001 | 200 | DHK |".to_string();
    let _ = std::fs::remove_dir_all(&d);
    check_guide("stack", "guide_stack", &[in1, in2, md(&out, &["Actnum", "Balance:2", "Branch", "Source_File"])]);
}

#[test]
fn guide_join() {
    use micro_parquet::engine::*;
    let d = dir("g_join");
    let left = frame(vec![text("Actnum", &["A1", "A2"]), nums("Balance", &[1_000_000.0, 500_000.0])]);
    let right = frame(vec![text("Actnum", &["A1", "A3"]), text("Sector", &["RMG", "Textile"])]);
    let right_dup = frame(vec![text("Actnum", &["A1", "A1"]), text("Sector", &["RMG", "RMG (dup row)"])]);
    let single = |name: &str, df: &DataFrame| Box::new(parquet(&d, name, df));
    let join = |r: Box<TabSource>, keep_first: bool| TabSource::JoinedTabs {
        left_source: single("left", &left),
        left_steps: vec![],
        right_source: r,
        right_steps: vec![],
        join_kind: SqlJoinKind::Left,
        join_keys: vec!["Actnum".into()],
        right_cols_to_keep: vec![],
        keep_first_duplicate_right: keep_first,
    };
    let out = compile_tab_lazy(&join(single("right", &right), false), &[], &d).expect("the join runs").collect().expect("collect");
    assert_eq!(out.height(), 2);
    assert_eq!(strings(&out, "Sector"), vec![Some("RMG".to_string()), None]);
    assert_eq!(floats(&out, "Balance"), vec![Some(1_000_000.0), Some(500_000.0)], "a left join keeps the left total");
    // review.rs t17_join_duplicates_inflate: a repeated right key is an error naming the key, never a doubled balance
    let err = format!("{:#}", compile_tab_lazy(&join(single("right_dup", &right_dup), false), &[], &d).err().expect("duplicate right keys"));
    assert!(err.contains("A1") && err.contains("Actnum") && err.contains("Keep first row per key"), "{err}");
    let _ = std::fs::remove_dir_all(&d);
    check_guide("join", "guide_join", &[md(&left, &["Actnum", "Balance:2"]), md(&right, &["Actnum", "Sector"]), md(&out, &["Actnum", "Balance:2", "Sector"])]);
}

#[test]
fn guide_split_export() {
    use micro_parquet::exporters::split_and_export_by_column;
    let d = dir("g_split");
    let df = frame(vec![
        text("Actnum", &["A1", "A2", "A3", "A4", "A5"]),
        Series::new("Branch".into(), &[Some("DHK"), Some("CTG"), Some("DHK"), None, Some("  ")]).into(),
        nums("Balance", &[100.0, 250.0, 300.0, 40.0, 10.0]),
    ]);
    let files = split_and_export_by_column(df.clone().lazy(), "Branch", &d, SplitExportFormat::Csv, ParquetDialect::ArchiveZstd, false, Some("Balance")).expect("the split runs");
    assert_eq!(files, 3);
    let man = CsvReadOptions::default().with_infer_schema_length(Some(0)).try_into_reader_with_file_path(Some(d.join("split_manifest.csv"))).expect("manifest").finish().expect("read the manifest");
    let mut table = String::from("| key | file | rows | control_sum |\n| --- | --- | ---: | ---: |");
    let (k, f, r, s) = (strings(&man, "key"), strings(&man, "file"), strings(&man, "rows"), strings(&man, "control_sum"));
    for i in 0..man.height() {
        let sum: f64 = s[i].as_deref().unwrap_or("0").parse().expect("a number");
        table.push_str(&format!("\n| {} | {} | {} | {} |", k[i].clone().unwrap_or_else(|| "(blank)".into()), f[i].clone().unwrap_or_default(), r[i].clone().unwrap_or_default(), money(sum)));
    }
    // a blank branch (nothing or spaces only) goes to BLANK_KEY.csv; the files hold every row and the manifest sums to the total
    assert!(d.join("BLANK_KEY.csv").exists() && d.join("DHK.csv").exists() && d.join("CTG.csv").exists());
    let total: usize = r.iter().map(|x| x.as_deref().unwrap_or("0").parse::<usize>().expect("a count")).sum();
    assert_eq!(total, 5);
    let _ = std::fs::remove_dir_all(&d);
    check_guide("split-export", "guide_split_export", &[md(&df, &["Actnum", "Branch", "Balance:2"]), table]);
}

#[test]
fn guide_batch() {
    use micro_parquet::batch::run_folder_mirror_batch;
    let d = dir("g_batch");
    let input = d.join("in");
    let output = d.join("out");
    std::fs::create_dir_all(input.join("2026-09")).expect("folder");
    write_file(&input.join("2026-09"), "loans.csv", "Actnum,Balance\n001,100\n002,\"1,250.50\"\n");
    write_file(&input, "deposits.csv", "Actnum,Balance\n101,500\n");
    let report = run_folder_mirror_batch(&input, &output, BatchDirection::CsvToParquet, BatchSchemaStrategy::SmartSafeAutoDetect, None, ParquetDialect::ArchiveZstd, false, &d).expect("the batch runs");
    assert_eq!((report.files_converted, report.errors.len()), (2, 0));
    assert!(output.join("2026-09").join("loans.parquet").exists() && output.join("deposits.parquet").exists(), "the folder tree is mirrored");
    let log = CsvReadOptions::default().with_infer_schema_length(Some(0)).try_into_reader_with_file_path(Some(output.join("batch_log.csv"))).expect("log").finish().expect("read the log");
    let mut rows: Vec<(String, String, String, String)> = (0..log.height())
        .map(|i| {
            let g = |n: &str| strings(&log, n)[i].clone().unwrap_or_default();
            (g("file").replace('\\', "/"), g("rows"), g("control_sum"), g("status"))
        })
        .collect();
    rows.sort();
    let mut table = String::from("| file | rows | control_sum | status |\n| --- | ---: | ---: | --- |");
    for (f, r, s, st) in &rows {
        let sum: f64 = s.parse().expect("a control sum");
        table.push_str(&format!("\n| {} | {} | {} | {} |", f, r, money(sum), st));
    }
    let tree = "| Input | Output |\n| --- | --- |\n| deposits.csv | deposits.parquet |\n| 2026-09/loans.csv | 2026-09/loans.parquet |".to_string();
    let _ = std::fs::remove_dir_all(&d);
    check_guide("batch", "guide_batch", &[tree, table]);
}

fn pivot_book() -> DataFrame {
    let b = |n: &[&str]| n.iter().map(|x| x.replace('-', "\u{2013}")).collect::<Vec<_>>();
    frame(vec![
        Series::new("Bucket".into(), b(&["02. 2-7 Days", "10. 3-5 Years", "04. 1-3 Months", "02. 2-7 Days", "04. 1-3 Months", "10. 3-5 Years", "02. 2-7 Days"])),
        Series::new("Bank".into(), &["Conventional", "Islami", "Conventional", "Islami", "Islami", "Conventional", "Conventional"]),
        nums("Balance", &[100.0, 300.0, 200.0, 50.0, 150.0, 400.0, 100.0]),
        nums("Rate", &[5.0, 9.0, 6.0, 8.0, 7.0, 10.0, 7.0]),
    ])
}

#[test]
fn guide_pivot() {
    use micro_parquet::pivot_table::build_pivot;
    let df = pivot_book();
    let sum = PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None };
    let spec = PivotSpec { rows: vec!["Bucket".into()], columns: vec!["Bank".into()], values: vec![sum.clone()], row_totals: true, column_totals: true, ..Default::default() };
    let out = build_pivot(df.clone().lazy(), &spec).expect("the pivot builds").df;
    assert_eq!(floats(&out, "Total"), vec![Some(250.0), Some(350.0), Some(700.0), Some(1300.0)]);
    let share = PivotSpec { show_as: PivotShowAs::PctOfRowTotal, ..spec.clone() };
    let pct = build_pivot(df.clone().lazy(), &share).expect("the share builds").df;
    close(floats(&pct, "Conventional")[0], 80.0, 1e-9, "02 Conventional share of its row");
    let wavg = PivotSpec { values: vec![PivotValue { field: "Rate".into(), agg: PivotAgg::WeightedAvg, weight_col: Some("Balance".into()) }], ..spec.clone() };
    let rates = build_pivot(df.clone().lazy(), &wavg).expect("the weighted rates build").df;
    close(floats(&rates, "Conventional")[0], 6.0, 1e-9, "(100 x 5 + 100 x 7) / 200");
    check_guide(
        "pivot",
        "guide_pivot",
        &[
            md(&df, &["Bucket", "Bank", "Balance:2", "Rate:2"]),
            md(&out, &["Bucket", "Conventional:2", "Islami:2", "Total:2"]),
            md(&pct, &["Bucket", "Conventional:2", "Islami:2", "Total:2"]),
            md(&rates, &["Bucket", "Conventional:2", "Islami:2", "Total:2"]),
        ],
    );
}

#[test]
fn guide_cl_migration() {
    use micro_parquet::banking::*;
    let mk = |keys: &[&str], bal: &[f64], cl: &[&str]| frame(vec![text("Actnum", keys), nums("Balance", bal), text("CL_Status", cl)]);
    let prev = mk(&["A1", "A2", "A3"], &[100.0, 50.0, 30.0], &["STD", "SS", "STD"]);
    let curr = mk(&["A1", "A2", "A4"], &[100.0, 50.0, 20.0], &["SMA", "DF", "STD"]);
    let spec = ClMigrationSpec { key_col: "Actnum".into(), bal_col: "Balance".into(), cl_col: "CL_Status".into(), segment_col: None, measure: MigrationMeasure::Balance, pct_of_row: false };
    let cl_map = ProvisionConfig::defaults().cl_code_map;
    let m = build_cl_migration_with(prev.clone().lazy(), curr.clone().lazy(), &spec, &cl_map).expect("the matrix builds");
    let cols = ["Previous grade", "STD:2", "SMA:2", "SS:2", "DF:2", "BL:2", "Closed/Repaid:2", "Total:2"];
    let counts = build_cl_migration_with(prev.clone().lazy(), curr.clone().lazy(), &ClMigrationSpec { measure: MigrationMeasure::Count, ..spec.clone() }, &cl_map).expect("the counts build");
    let count_cols = ["Previous grade", "STD", "SMA", "SS", "DF", "BL", "Closed/Repaid", "Total"];
    // the figures of the acceptance example
    assert_eq!(floats(&m, "Total"), vec![Some(130.0), Some(0.0), Some(50.0), Some(0.0), Some(0.0), Some(180.0), Some(20.0), Some(200.0)]);
    assert_eq!(floats(&m, "SMA")[0], Some(100.0));
    assert_eq!(floats(&counts, "Total")[0], Some(2.0));
    check_guide(
        "cl-migration",
        "guide_cl_migration",
        &[md(&prev, &["Actnum", "Balance:2", "CL_Status"]), md(&curr, &["Actnum", "Balance:2", "CL_Status"]), md(&m, &cols), md(&counts, &count_cols)],
    );
}

#[test]
fn guide_gap_ladder() {
    use crate::v3_support::*;
    let assets = frame(vec![nums("Balance", &[100.0, 200.0]), dates("Matdat", &["2026-11-15", "2026-12-31"])]);
    let liab = frame(vec![nums("Balance", &[150.0, 50.0]), dates("Matdat", &["2026-11-20", "2026-12-15"])]);
    let limits = GapLimitsFile { preset: "Test ladder".into(), source: "test".into(), effective_from: String::new(), limits: vec![GapLimit { bucket: "0\u{2013}1 month".into(), min_cumulative_gap_pct: -10.0 }] };
    let spec = |role| GapInputSpec { role, amount_col: "Balance".into(), maturity_col: "Matdat".into() };
    let ladder = build_gap_ladder(vec![(assets.clone().lazy(), spec(GapRole::Asset)), (liab.clone().lazy(), spec(GapRole::Liability))], &two_bucket_preset(), "2026-10-31", true, Some(&limits)).expect("the ladder builds");
    assert_eq!(strings(&ladder, "Breach")[0].as_deref(), Some("BREACH"));
    check_guide(
        "gap-ladder",
        "guide_gap_ladder",
        &[
            md(&assets, &["Balance:2", "Matdat"]),
            md(&liab, &["Balance:2", "Matdat"]),
            md(&ladder, &["Bucket", "Inflows:2", "Outflows:2", "Net gap:2", "Cumulative gap:2", "Net gap % of total assets:2", "Cumulative gap % of total assets:2", "Cumulative gap limit %:2", "Breach"]),
        ],
    );
}
