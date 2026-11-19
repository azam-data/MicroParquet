//! P6: the gap ladder's TDRR stress and the build workflow.

use crate::common::*;
use micro_parquet::banking::*;
use micro_parquet::behaviour::{BehaviourFile, BehaviourProduct};
use micro_parquet::models::*;
use polars::prelude::*;

const EN: &str = "\u{2013}";

fn preset() -> CustomBucketDef {
    CustomBucketDef {
        name: "Test ladder".into(),
        rules: vec![BucketRuleItem { max_days: 30, label: format!("0{EN}1 month") }, BucketRuleItem { max_days: 90, label: format!("1{EN}3 months") }],
        overflow_label: "Over 3 months".into(),
        overdue_label: "Overdue".into(),
        null_sight_label: format!("0{EN}1 month"),
        null_long_label: "Over 3 months".into(),
    }
}

fn td(product: &str, tdrr: f64) -> BehaviourProduct {
    BehaviourProduct { product: product.into(), tdrr_pct: tdrr, ..BehaviourProduct::default() }
}

fn profile(products: Vec<BehaviourProduct>, stress: bool) -> GapBehaviour {
    GapBehaviour { file_name: "2026-10-31.json".into(), file: BehaviourFile { as_of: "2026-10-31".into(), source: "test".into(), products }, product_cols: vec!["Product".into()], stress_tdrr: stress }
}

/// One term deposit of 1,000 maturing in 1–3 months (45 days after the cutoff), and a few other rows.
fn book() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Product".into(), &["TD-1Y", "TD-1Y", "FDR-X", "SAV"]),
        Series::new("Balance".into(), &[1000.0, 500.0, 300.0, 200.0]),
        dates("Matdat", &["2026-12-15", "2026-11-10", "2026-12-15", "2026-12-15"]),
    ])
    .expect("book")
}

fn ladder(df: &DataFrame, role: GapRole, b: Option<&GapBehaviour>) -> anyhow::Result<DataFrame> {
    let spec = GapInputSpec { role, amount_col: "Balance".into(), maturity_col: "Matdat".into() };
    build_gap_ladder_behaviour(vec![(df.clone().lazy(), spec)], &preset(), "2026-10-31", true, None, b)
}

fn cell(df: &DataFrame, bucket: &str, col: &str) -> f64 {
    let i = strings(df, "Bucket").iter().position(|b| b.as_deref() == Some(bucket)).unwrap_or_else(|| panic!("bucket {bucket}"));
    floats(df, col)[i].unwrap_or(f64::NAN)
}

#[test]
fn p6_a_term_deposit_of_1000_with_tdrr_10_gives_100_in_the_first_bucket_and_900_stressed() {
    let one = DataFrame::new(vec![Series::new("Product".into(), &["TD-1Y"]), Series::new("Balance".into(), &[1000.0]), dates("Matdat", &["2026-12-15"])]).expect("one");
    let first = format!("0{EN}1 month");
    let second = format!("1{EN}3 months");
    // stressed: 100 in the first bucket, 900 in 1-3 months
    let b = profile(vec![td("TD-1Y", 10.0)], true);
    let s = ladder(&one, GapRole::Liability, Some(&b)).expect("stressed");
    assert_eq!((cell(&s, &first, "Outflows"), cell(&s, &second, "Outflows")), (100.0, 900.0));
    // not stressed: 1,000 in 1-3 months, the same as the Phase 4 ladder
    let b = profile(vec![td("TD-1Y", 10.0)], false);
    let n = ladder(&one, GapRole::Liability, Some(&b)).expect("not stressed");
    assert_eq!((cell(&n, &first, "Outflows"), cell(&n, &second, "Outflows")), (0.0, 1000.0));
    let plain = ladder(&one, GapRole::Liability, None).expect("plain");
    assert!(n.equals_missing(&plain), "with the option off the results are unchanged");
    // the total outflow is the same stressed or not: the money only moves
    let s = ladder(&one, GapRole::Liability, Some(&profile(vec![td("TD-1Y", 10.0)], true))).expect("stressed");
    assert_eq!(cell(&s, "Total", "Outflows"), 1000.0);
    // the cumulative gap of the first bucket is now -100
    assert_eq!(cell(&s, &first, "Net gap"), -100.0);
}

#[test]
fn p6_only_term_deposits_of_the_profile_liabilities_move_and_other_rows_stay() {
    let first = format!("0{EN}1 month");
    let second = format!("1{EN}3 months");
    // TD-1Y at 10% (1,500 in all: 500 matures in 11 days, 1,000 in 45), FDR-X at 50%, SAV is not in the profile
    let b = profile(vec![td("TD-1Y", 10.0), td("fdr-x", 50.0)], true);
    let s = ladder(&book(), GapRole::Liability, Some(&b)).expect("stressed");
    // first bucket: the 11-day deposit's rest (500 x 0.9) and the early parts: 100 + 50 (TD-1Y) and 150 (FDR-X, 50% of 300)
    assert_eq!(cell(&s, &first, "Outflows"), 450.0 + 100.0 + 50.0 + 150.0, "the 11-day deposit's rest, and the early parts of the three term deposits");
    assert_eq!(cell(&s, &second, "Outflows"), 900.0 + 150.0 + 200.0, "the rest of the 45-day deposits and the saving row, untouched");
    // assets are never stressed
    let a = ladder(&book(), GapRole::Asset, Some(&b)).expect("assets");
    let plain = ladder(&book(), GapRole::Asset, None).expect("plain assets");
    assert!(a.equals_missing(&plain));
    // a profile with a non-maturity deposit and a term deposit: both apply
    let mut sav = td("SAV", 0.0);
    sav.category = "Retail non-transactional".into();
    sav.core_pct = 50.0;
    sav.avg_maturity_years = 0.0;
    let both = profile(vec![td("TD-1Y", 10.0), sav], true);
    let r = ladder(&book(), GapRole::Liability, Some(&both)).expect("both");
    assert_eq!(cell(&r, "Total", "Outflows"), 2000.0, "nothing is lost");
}

#[test]
fn p6_the_stress_without_a_term_deposit_tdrr_is_an_error_not_a_silent_no_op() {
    let one = DataFrame::new(vec![Series::new("Product".into(), &["TD-1Y"]), Series::new("Balance".into(), &[1000.0]), dates("Matdat", &["2026-12-15"])]).expect("one");
    let e = format!("{:#}", ladder(&one, GapRole::Liability, Some(&profile(vec![td("TD-1Y", 0.0)], true))).expect_err("no TDRR"));
    assert!(e.contains("Stress: early withdrawal (TDRR)") && e.contains("no term-deposit product with a TDRR above 0"), "{e}");
    // a TDRR of 100% moves the whole balance
    let all = ladder(&one, GapRole::Liability, Some(&profile(vec![td("TD-1Y", 100.0)], true))).expect("all");
    assert_eq!(cell(&all, &format!("0{EN}1 month"), "Outflows"), 1000.0);
}

fn workflow() -> String {
    std::fs::read_to_string(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".github").join("workflows").join("main.yml")).expect("main.yml")
}

#[test]
fn p6_the_build_workflow_chooses_its_platforms_and_a_release_needs_a_tag_push_after_the_tests() {
    let text = workflow();
    // (the workflow files are checked by actionlint in the Tests workflow; this test reads what they must say)
    let tests = std::fs::read_to_string(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".github").join("workflows").join("test.yml")).expect("test.yml");
    assert!(tests.contains("actionlint") && tests.contains("cargo test"), "the Tests workflow runs cargo test and actionlint");
    for key in ["  plan:", "  test:", "  build:", "    strategy:", "      matrix: ${{ fromJSON(needs.plan.outputs.matrix) }}"] {
        assert!(text.contains(key), "main.yml has '{key}'");
    }
    assert!(!text.contains("3b. Run Calculation Tests"), "the old test step inside the build is gone");
    // the manual build chooses its platforms: windows by default
    let dispatch = text.split("workflow_dispatch:").nth(1).and_then(|t| t.split("  push:").next()).expect("the dispatch block");
    for needle in ["platforms:", "type: choice", "- windows", "- macos", "- both", "default: windows"] {
        assert!(dispatch.contains(needle), "the dispatch input has '{needle}': {dispatch}");
    }
    // a tag push still builds both, after the tests passed
    assert!(text.contains("tags:") && text.contains("- 'v*'") && text.contains("|| 'both'"));
    let test_job = text.split("  test:").nth(1).and_then(|t| t.split("  build:").next()).expect("the test job");
    assert!(test_job.contains("if: github.event_name == 'push'") && test_job.contains("run: cargo test"), "{test_job}");
    assert!(text.contains("needs: [plan, test]") && text.contains("needs.test.result == 'success'"), "the build waits for the tests");
    // a release is published by a tag push only: a manual run (even on a tag) never publishes
    let publish = text.split("7. Publish to GitHub Releases").nth(1).expect("the publish step");
    assert!(publish.contains("if: github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v')"), "{publish}");
    // the manual run has artifacts only: the upload step has no condition
    let upload = text.split("6. Upload Build Artifact").nth(1).and_then(|t| t.split("7. Publish").next()).expect("the upload step");
    assert!(!upload.contains("if:"), "{upload}");
}

/// Runs the plan step's script for each choice and reads the matrix it writes.
#[test]
fn p6_the_plan_step_makes_the_matrix_for_windows_macos_both_and_a_tag() {
    let text = workflow();
    let script = text.split("        run: |\n").nth(1).and_then(|t| t.split("\n  # A tag push runs the tests").next()).expect("the plan script");
    let script: String = script.lines().map(|l| l.strip_prefix("          ").unwrap_or(l)).collect::<Vec<_>>().join("\n");
    let d = dir("p6_plan");
    let run = |platforms: &str| -> Result<Vec<String>, String> {
        let out = d.join("out.txt");
        let _ = std::fs::remove_file(&out);
        let o = std::process::Command::new("bash").arg("-c").arg(&script).env("PLATFORMS", platforms).env("GITHUB_OUTPUT", &out).output().expect("bash");
        if !o.status.success() {
            return Err(String::from_utf8_lossy(&o.stdout).to_string());
        }
        let line = std::fs::read_to_string(&out).expect("output");
        let json: serde_json::Value = serde_json::from_str(line.trim().strip_prefix("matrix=").expect("matrix=")).expect("the matrix is JSON");
        Ok(json["include"].as_array().expect("include").iter().map(|e| e["runner"].as_str().expect("runner").to_string()).collect())
    };
    assert_eq!(run("windows").expect("windows"), vec!["windows-latest"]);
    assert_eq!(run("macos").expect("macos"), vec!["macos-latest"]);
    assert_eq!(run("both").expect("both"), vec!["windows-latest", "macos-latest"], "a tag build (platforms is 'both') makes both");
    let e = run("linux").expect_err("an unknown choice");
    assert!(e.contains("Unknown platforms 'linux'"), "{e}");
    // every entry has what the build needs
    let out = d.join("out.txt");
    let _ = run("both");
    let json: serde_json::Value = serde_json::from_str(std::fs::read_to_string(&out).expect("read").trim().strip_prefix("matrix=").expect("prefix")).expect("json");
    for e in json["include"].as_array().expect("include") {
        for k in ["os_name", "runner", "target", "bin_name", "archive_name"] {
            assert!(e[k].as_str().is_some_and(|v| !v.is_empty()), "{e} has {k}");
        }
    }
}
