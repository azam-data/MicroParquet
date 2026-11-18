//! Phase 5 R7: IFRS 9 staging and ECL (BRPD Circular No. 06 of 8 March 2026).

use crate::common::*;
use micro_parquet::ecl::*;
use micro_parquet::models::*;
use micro_parquet::rules::{set_data_dir_override, SHIPPED_RULES};
use polars::prelude::*;

fn params_json(scenarios: serde_json::Value) -> String {
    serde_json::json!({
        "as_of": "2026-03-08",
        "source": "test parameters (not a circular)",
        "sample": true,
        "segments": [
            {"name": "Corporate", "cumulative_pd_pct": [2.0, 5.0], "lgd_pct": 45, "ccf_pct": 50},
            {"name": "Retail", "cumulative_pd_pct": [4.0, 9.0, 12.0], "lgd_pct": 60, "ccf_pct": 40}
        ],
        "scenarios": scenarios
    })
    .to_string()
}

fn one_scenario() -> serde_json::Value {
    serde_json::json!([{"name": "Base", "weight_pct": 100, "pd_multiplier": 1.0}])
}

fn three_scenarios() -> serde_json::Value {
    serde_json::json!([
        {"name": "Upside", "weight_pct": 25, "pd_multiplier": 0.8},
        {"name": "Base", "weight_pct": 50, "pd_multiplier": 1.0},
        {"name": "Downside", "weight_pct": 25, "pd_multiplier": 1.5}
    ])
}

/// A data folder holding these parameters under the date of the shipped sample (so they are the ones read).
fn data_with(name: &str, scenarios: serde_json::Value) -> std::path::PathBuf {
    let data = dir(name);
    let folder = data.join("ecl_params");
    std::fs::create_dir_all(&folder).expect("folder");
    std::fs::write(folder.join("2026-03-08.json"), params_json(scenarios)).expect("write");
    data
}

fn step(extra: &dyn Fn(&mut serde_json::Value)) -> TransformStep {
    let mut v = serde_json::json!({
        "IfrsEcl": {
            "fixed_segment": "Corporate", "drawn_col": "Drawn", "eir_col": "EIR", "maturity_col": "Matdat",
            "classified_stage3": true, "cutoff_date": "2026-10-31"
        }
    });
    extra(&mut v["IfrsEcl"]);
    serde_json::from_value(v).expect("a step")
}

fn book(drawn: &[f64], matdat: &[&str]) -> DataFrame {
    let n = drawn.len();
    DataFrame::new(vec![
        Series::new("Account_No".into(), (1..=n).map(|i| format!("A{i}")).collect::<Vec<_>>()),
        Series::new("Drawn".into(), drawn.to_vec()),
        Series::new("EIR".into(), vec![10.0; n]),
        dates("Matdat", matdat),
    ])
    .expect("book")
}

fn run(name: &str, data: &std::path::Path, df: &DataFrame, s: &TransformStep) -> anyhow::Result<DataFrame> {
    set_data_dir_override(Some(data.to_path_buf()));
    let out = run_steps(name, df, std::slice::from_ref(s));
    set_data_dir_override(None);
    out
}

fn near(a: Option<f64>, b: f64, what: &str) {
    let a = a.unwrap_or(f64::NAN);
    assert!((a - b).abs() < 0.005, "{what}: {a} vs {b}");
}

/// A loan in a stage (set through a previous-stage free route: DPD picks the stage).
fn stage_book(dpd: i64) -> DataFrame {
    let mut df = book(&[1_000_000.0], &["2028-10-31"]);
    df.with_column(Series::new("DPD".into(), &[dpd])).expect("dpd");
    df
}

#[test]
fn r7_single_scenario_stage_1_is_8181_82_stage_2_19338_84_stage_3_450000() {
    // EAD 1,000,000, cumulative PD 2% and 5%, LGD 45%, EIR 10%, two years left, one scenario
    let data = data_with("r7a", one_scenario());
    let s = step(&|s| s["dpd_col"] = serde_json::json!("DPD"));
    for (d, stage, want) in [(0, 1, 8181.82), (45, 2, 19338.84), (95, 3, 450_000.0)] {
        let out = run("r7a1", &data, &stage_book(d), &s).expect("runs");
        assert_eq!(ints(&out, "Stage"), vec![Some(stage)], "DPD {d}");
        near(floats(&out, "ECL")[0], want, &format!("stage {stage}"));
        near(floats(&out, "EAD")[0], 1_000_000.0, "EAD");
    }
    let out = run("r7a2", &data, &stage_book(0), &s).expect("runs");
    assert_eq!(strings(&out, "ECL_Rule_File")[0].as_deref(), Some("ifrs9_staging/2026-03-08.json"));
    assert_eq!(strings(&out, "ECL_Params_File")[0].as_deref(), Some("ecl_params/2026-03-08.json"));
}

fn ints(df: &DataFrame, name: &str) -> Vec<Option<i64>> {
    df.column(name).expect("column").cast(&DataType::Int64).expect("cast").i64().expect("i64").into_iter().collect()
}

#[test]
fn r7_three_scenarios_weight_the_pd_multipliers() {
    // 25/50/25 with x0.8 / x1.0 / x1.5: the ECL is 1.075 times the single-scenario figure
    let data = data_with("r7b", three_scenarios());
    let s = step(&|s| s["dpd_col"] = serde_json::json!("DPD"));
    near(floats(&run("r7b1", &data, &stage_book(0), &s).expect("runs"), "ECL")[0], 8795.45, "stage 1");
    near(floats(&run("r7b2", &data, &stage_book(45), &s).expect("runs"), "ECL")[0], 20789.26, "stage 2");
    // Stage 3 does not depend on the scenarios: LGD x EAD
    near(floats(&run("r7b3", &data, &stage_book(95), &s).expect("runs"), "ECL")[0], 450_000.0, "stage 3");
    // the marginal PD is capped at 1: a multiplier that would give more than 100% counts 100%
    let seg = Segment { name: "S".into(), cumulative_pd_pct: vec![60.0, 90.0], lgd_pct: 100.0, ccf_pct: 0.0 };
    let sc = [Scenario { name: "x".into(), weight_pct: 100.0, pd_multiplier: 3.0 }];
    near(Some(ecl_of(1000.0, 1, 2.0, 0.0, &seg, &sc).expect("ecl")), 1000.0, "60% x 3 is capped at 100%");
}

#[test]
fn r7_the_undrawn_amount_goes_into_ead_through_the_ccf() {
    let data = data_with("r7c", one_scenario());
    let mut df = book(&[800_000.0], &["2028-10-31"]);
    df.with_column(Series::new("Undrawn".into(), &[200_000.0])).expect("col");
    let s = step(&|s| s["undrawn_col"] = serde_json::json!("Undrawn"));
    let out = run("r7c1", &data, &df, &s).expect("runs");
    near(floats(&out, "EAD")[0], 900_000.0, "800,000 + 50% of 200,000");
    near(floats(&out, "ECL")[0], 7363.64, "stage 1 on that EAD");
    // another segment has another CCF
    let s2 = step(&|s| {
        s["undrawn_col"] = serde_json::json!("Undrawn");
        s["fixed_segment"] = serde_json::json!("retail");
    });
    near(floats(&run("r7c2", &data, &df, &s2).expect("runs"), "EAD")[0], 880_000.0, "40% of 200,000");
}

#[test]
fn r7_part_years_and_short_terms_scale_the_marginal_pd_by_the_fraction() {
    let seg = Segment { name: "S".into(), cumulative_pd_pct: vec![2.0, 5.0], lgd_pct: 45.0, ccf_pct: 0.0 };
    let sc = [Scenario { name: "x".into(), weight_pct: 100.0, pd_multiplier: 1.0 }];
    // 1.5 years, stage 2: year 1 in full and half of year 2
    near(Some(ecl_of(1_000_000.0, 2, 1.5, 0.10, &seg, &sc).expect("ecl")), 8181.82 + 6750.0 / 1.21, "1.5 years");
    // half a year left, stage 1: half of year 1
    near(Some(ecl_of(1_000_000.0, 1, 0.5, 0.10, &seg, &sc).expect("ecl")), 4090.91, "6 months");
    // stage 1 with 3 years left is still only the first year
    near(Some(ecl_of(1_000_000.0, 1, 3.0, 0.10, &seg, &sc).expect("ecl")), 8181.82, "12 months");
    // a term longer than the PD curve is an error naming the curve
    let e = format!("{:#}", ecl_of(1.0, 2, 3.0, 0.1, &seg, &sc).err().expect("too long"));
    assert!(e.contains("3.00 years") && e.contains("2 years of the PD curve of 'S'"), "{e}");
    // matured stage 1 or 2 loans have no remaining term: ECL 0 and the count is said
    let data = data_with("r7d", one_scenario());
    let df = book(&[1000.0, 1000.0], &["2026-10-31", "2026-01-31"]);
    set_data_dir_override(Some(data));
    let (_, stats) = classify_lazy(df.lazy(), &spec_of(&step(&|_| {})).expect("spec")).expect("runs");
    set_data_dir_override(None);
    assert_eq!(stats.no_term, 2);
    assert!(stats.note("IFRS 9 ECL").contains("2 Stage 1 or 2 loan(s) have a maturity on or before the cutoff"), "{}", stats.note("x"));
}

fn staged(name: &str, df: DataFrame, s: &TransformStep) -> (Vec<Option<i64>>, Vec<Option<String>>) {
    let data = data_with(name, one_scenario());
    let out = run(name, &data, &df, s).expect("runs");
    (ints(&out, "Stage"), strings(&out, "Stage_Reason"))
}

#[test]
fn r7_staging_by_dpd_classified_grade_forborne_unassessed_and_rating() {
    // DPD 45 -> Stage 2 "30+ DPD"; DPD 95 -> Stage 3; DPD 29 and 30, 89 and 90 are the edges
    let mut df = book(&[1.0; 6], &["2028-10-31"; 6]);
    df.with_column(Series::new("DPD".into(), &[45i64, 95, 29, 30, 89, 90])).expect("dpd");
    let (st, why) = staged("r7e1", df, &step(&|s| s["dpd_col"] = serde_json::json!("DPD")));
    assert_eq!(st, vec![Some(2), Some(3), Some(1), Some(2), Some(2), Some(3)]);
    assert_eq!(why[0].as_deref(), Some("30+ DPD"));
    assert_eq!(why[1].as_deref(), Some("90+ DPD"));
    assert_eq!(why[2].as_deref(), Some("Performing"));
    // a classified grade is Stage 3 with the option on (and DPD 0), Stage 1 with it off; Standard and SMA are not classified
    let mut df = book(&[1.0; 4], &["2028-10-31"; 4]);
    df.with_column(Series::new("CL_Final".into(), &["SS", "DF", "B/L", "SMA"])).expect("grade");
    df.with_column(Series::new("DPD".into(), &[0i64; 4])).expect("dpd");
    let on = step(&|s| {
        s["grade_col"] = serde_json::json!("CL_Final");
        s["dpd_col"] = serde_json::json!("DPD");
    });
    let (st, why) = staged("r7e2", df.clone(), &on);
    assert_eq!(st, vec![Some(3), Some(3), Some(3), Some(1)]);
    assert_eq!(why[0].as_deref(), Some("Classified SS"));
    let off = step(&|s| {
        s["grade_col"] = serde_json::json!("CL_Final");
        s["dpd_col"] = serde_json::json!("DPD");
        s["classified_stage3"] = serde_json::json!(false);
    });
    assert_eq!(staged("r7e3", df, &off).0, vec![Some(1); 4]);
    // forborne -> Stage 2; SICR not assessed -> Stage 2; the flags may be text, numbers or booleans
    let mut df = book(&[1.0; 4], &["2028-10-31"; 4]);
    df.with_column(Series::new("Forborne".into(), &["Y", "N", "yes", ""])).expect("col");
    df.with_column(Series::new("Unassessed".into(), &[0i64, 1, 0, 0])).expect("col");
    let s = step(&|s| {
        s["forborne_col"] = serde_json::json!("Forborne");
        s["sicr_unassessed_col"] = serde_json::json!("Unassessed");
    });
    let (st, why) = staged("r7e4", df, &s);
    assert_eq!(st, vec![Some(2), Some(2), Some(2), Some(1)]);
    assert_eq!((why[0].as_deref(), why[1].as_deref()), (Some("Forborne"), Some("SICR not assessed")));
    // a rating downgrade of 2 notches or more (the file's N) -> Stage 2; no rating -> not tested and counted
    let mut df = book(&[1.0; 4], &["2028-10-31"; 4]);
    df.with_column(Series::new("Rating0".into(), &[Some(3.0), Some(3.0), Some(3.0), None])).expect("col");
    df.with_column(Series::new("Rating1".into(), &[Some(5.0), Some(4.0), Some(8.0), Some(9.0)])).expect("col");
    let s = step(&|s| {
        s["rating_orig_col"] = serde_json::json!("Rating0");
        s["rating_now_col"] = serde_json::json!("Rating1");
    });
    let (st, why) = staged("r7e5", df.clone(), &s);
    assert_eq!(st, vec![Some(2), Some(1), Some(2), Some(1)]);
    assert_eq!(why[0].as_deref(), Some("Rating down 2 notches or more"));
    let data = data_with("r7e6", one_scenario());
    set_data_dir_override(Some(data));
    let (_, stats) = classify_lazy(df.lazy(), &spec_of(&s).expect("spec")).expect("runs");
    set_data_dir_override(None);
    assert_eq!(stats.no_rating, 1);
    // only one of the two rating columns is refused
    let one = step(&|s| s["rating_orig_col"] = serde_json::json!("Rating0"));
    let e = format!("{:#}", run("r7e7", &data_with("r7e7d", one_scenario()), &book(&[1.0], &["2028-10-31"]), &one).err().expect("one rating column"));
    assert!(e.contains("both the rating at origination and the current rating"), "{e}");
}

#[test]
fn r7_a_better_stage_applies_only_after_the_cure_period() {
    // previous Stage 3, performing now (Stage 1 by the indicators): 60 days -> stays Stage 3; 95 days -> Stage 2;
    // 185 days -> Stage 1 (one stage back for each full period of 90 days)
    let mut df = book(&[1.0; 4], &["2028-10-31"; 4]);
    df.with_column(Series::new("Prev".into(), &[3i64, 3, 3, 2])).expect("col");
    df.with_column(Series::new("Days".into(), &[60.0, 95.0, 185.0, 95.0])).expect("col");
    let s = step(&|s| {
        s["prev_stage_col"] = serde_json::json!("Prev");
        s["days_performing_col"] = serde_json::json!("Days");
    });
    let (st, why) = staged("r7f1", df, &s);
    assert_eq!(st, vec![Some(3), Some(2), Some(1), Some(1)]);
    assert!(why[0].as_deref().unwrap_or("").contains("Cure period not over (60 of 90 days performing)"), "{why:?}");
    assert!(why[1].as_deref().unwrap_or("").contains("95 of 90"), "{why:?}");
    assert_eq!(why[3].as_deref(), Some("Performing"), "stage 2 -> stage 1 after one full period: cured");
    // a forborne exposure needs 365 days: 95 days change nothing, 400 days give one stage back
    let mut df = book(&[1.0; 2], &["2028-10-31"; 2]);
    df.with_column(Series::new("Prev".into(), &[3i64, 3])).expect("col");
    df.with_column(Series::new("Days".into(), &[95.0, 400.0])).expect("col");
    df.with_column(Series::new("Forborne".into(), &["Y", "Y"])).expect("col");
    let s = step(&|s| {
        s["prev_stage_col"] = serde_json::json!("Prev");
        s["days_performing_col"] = serde_json::json!("Days");
        s["forborne_col"] = serde_json::json!("Forborne");
    });
    let (st, _) = staged("r7f2", df, &s);
    assert_eq!(st, vec![Some(3), Some(2)], "forborne: stage 2 anyway; 400 days is one period of 365 from stage 3");
    // the previous stage is never made worse than the indicators by the cure rule, and a blank previous stage means a new loan
    let mut df = book(&[1.0; 3], &["2028-10-31"; 3]);
    df.with_column(Series::new("Prev".into(), &[Some("Stage 1"), None, Some("3")])).expect("col");
    df.with_column(Series::new("Days".into(), &[None, None, Some(0.0)])).expect("col");
    df.with_column(Series::new("DPD".into(), &[45i64, 0, 0])).expect("col");
    let s = step(&|s| {
        s["prev_stage_col"] = serde_json::json!("Prev");
        s["days_performing_col"] = serde_json::json!("Days");
        s["dpd_col"] = serde_json::json!("DPD");
    });
    assert_eq!(staged("r7f3", df, &s).0, vec![Some(2), Some(1), Some(3)]);
    // only one of the two columns is refused; a bad previous stage and a blank days-performing name the rows
    let one = step(&|s| s["prev_stage_col"] = serde_json::json!("Prev"));
    let e = format!("{:#}", run("r7f4", &data_with("r7f4d", one_scenario()), &book(&[1.0], &["2028-10-31"]), &one).err().expect("one column"));
    assert!(e.contains("both the previous stage and the days performing"), "{e}");
    let mut bad = book(&[1.0, 1.0], &["2028-10-31"; 2]);
    bad.with_column(Series::new("Prev".into(), &[3i64, 7])).expect("col");
    bad.with_column(Series::new("Days".into(), &[None::<f64>, Some(1.0)])).expect("col");
    let e = format!("{:#}", run("r7f5", &data_with("r7f5d", one_scenario()), &bad, &s_prev()).err().expect("bad values"));
    assert!(e.contains("the previous stage in 'Prev' is not 1, 2 or 3 on 1 row(s)") && e.contains("the days performing in 'Days' is blank where the previous stage was worse on 1 row(s)"), "{e}");
}

fn s_prev() -> TransformStep {
    step(&|s| {
        s["prev_stage_col"] = serde_json::json!("Prev");
        s["days_performing_col"] = serde_json::json!("Days");
    })
}

#[test]
fn r7_scenario_weights_that_do_not_sum_to_100_are_refused_and_fewer_than_three_scenarios_warn() {
    let bad = serde_json::json!([{"name": "Base", "weight_pct": 60, "pd_multiplier": 1.0}, {"name": "Down", "weight_pct": 30, "pd_multiplier": 1.5}]);
    let data = data_with("r7g", bad);
    let e = format!("{:#}", run("r7g1", &data, &book(&[1.0], &["2028-10-31"]), &step(&|_| {})).err().expect("weights 90"));
    assert!(e.contains("sum to 90, not 100") && e.contains("ecl_params/2026-03-08.json") && e.contains("section 5.9"), "{e}");
    // one scenario at 100: allowed, with a warning in the note (the spec's single-scenario acceptance)
    let data = data_with("r7g2", one_scenario());
    set_data_dir_override(Some(data));
    let (_, stats) = classify_lazy(book(&[1.0], &["2028-10-31"]).lazy(), &spec_of(&step(&|_| {})).expect("spec")).expect("runs");
    set_data_dir_override(None);
    assert!(stats.note("IFRS 9 ECL").contains("has 1 scenario(s); BRPD Circular 06/2026 section 5.9 expects at least three"), "{}", stats.note("x"));
    // three scenarios: no warning
    let data = data_with("r7g3", three_scenarios());
    set_data_dir_override(Some(data));
    let (_, stats) = classify_lazy(book(&[1.0], &["2028-10-31"]).lazy(), &spec_of(&step(&|_| {})).expect("spec")).expect("runs");
    set_data_dir_override(None);
    assert!(stats.warnings.is_empty());
    // other mistakes in the parameter file
    let check = |f: &dyn Fn(&mut serde_json::Value)| -> String {
        let mut v: serde_json::Value = serde_json::from_str(&params_json(three_scenarios())).expect("json");
        f(&mut v);
        let p: EclParams = serde_json::from_value(v).expect("params");
        format!("{:#}", check_params(&p, "ecl_params/x.json").err().expect("refused"))
    };
    assert!(check(&|v| v["segments"][0]["cumulative_pd_pct"] = serde_json::json!([5.0, 2.0])).contains("do not fall"));
    assert!(check(&|v| v["segments"][0]["lgd_pct"] = serde_json::json!(120)).contains("lgd_pct of 'Corporate'"));
    assert!(check(&|v| v["segments"][1]["name"] = serde_json::json!("corporate")).contains("twice"));
    assert!(check(&|v| v["scenarios"][0]["pd_multiplier"] = serde_json::json!(-1.0)).contains("not negative"));
    assert!(check(&|v| v["scenarios"] = serde_json::json!([])).contains("no scenario"));
}

#[test]
fn r7_the_staging_rule_file_is_the_circulars_not_a_sample_and_the_params_are_a_sample() {
    let (_, (date, text)) = SHIPPED_RULES.iter().find(|(k, _)| *k == "ifrs9_staging").expect("shipped");
    assert_eq!(*date, "2026-03-08");
    let v: serde_json::Value = serde_json::from_str(text).expect("json");
    assert_eq!(v["sample"], serde_json::json!(false));
    let src = v["source"].as_str().unwrap_or("");
    assert!(src.contains("Circular No. 06") && src.contains("8 March 2026") && src.contains("4.5.5") && src.contains("app's alignment") && src.contains("sample value"), "{src}");
    let data = dir("r7h");
    set_data_dir_override(Some(data.clone()));
    let cutoff = chrono::NaiveDate::from_ymd_opt(2026, 10, 31).expect("date");
    let r = load_staging(cutoff).expect("loads");
    assert_eq!((r.data.sicr_dpd, r.data.default_dpd, r.data.rating_downgrade_notches, r.data.cure_days, r.data.cure_days_forborne), (30, 90, 2, 90, 365));
    assert_eq!(r.data.classified_grades, vec!["SS", "DF", "B/L"]);
    assert!(r.data.classified_is_stage3, "the option is on by default");
    assert!(r.data.notes.iter().any(|n| n.contains("app's alignment")));
    assert!(!r.meta.sample);
    // the parameters are a sample, written to portable_data/ecl_params when missing
    let (p, warnings) = load_params(cutoff).expect("loads");
    assert!(p.meta.sample && p.meta.source.contains("SAMPLE") && p.data.scenarios.len() >= 3 && warnings.is_empty());
    assert!(data.join("ecl_params").join("2026-03-08.json").exists());
    let src_copy = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join("ecl_params").join("2026-03-08.json")).expect("source copy");
    assert_eq!(src_copy, micro_parquet::rules::ECL_PARAMS.1);
    // an edited file is left alone
    let f = data.join("ecl_params").join("2026-03-08.json");
    let edited = src_copy.replace("\"sample\": true", "\"sample\": false");
    std::fs::write(&f, &edited).expect("edit");
    assert!(!load_params(cutoff).expect("loads").0.meta.sample);
    assert_eq!(std::fs::read_to_string(&f).expect("read"), edited);
    // before the file's date there is none
    let e = format!("{:#}", load_staging(chrono::NaiveDate::from_ymd_opt(2026, 3, 7).expect("date")).err().expect("too early"));
    assert!(e.contains("dated on or before the cutoff 2026-03-07"), "{e}");
    // the rule file is checked
    let mut bad: StagingRules = serde_json::from_str(text).expect("rules");
    bad.default_dpd = 20;
    assert!(format!("{:#}", check_staging(&bad, "ifrs9_staging/x.json").err().expect("refused")).contains("default_dpd (20) must be above sicr_dpd (30)"));
    set_data_dir_override(None);
}

#[test]
fn r7_bad_values_stop_the_run_with_the_count_and_the_first_five_accounts() {
    let data = data_with("r7i", one_scenario());
    let n = 7;
    // blank EIR on 7 stage 1 rows
    let mut df = book(&vec![1000.0; n], &vec!["2028-10-31"; n]);
    df.with_column(Series::new("EIR".into(), vec![None::<f64>; n])).expect("col");
    let e = format!("{:#}", run("r7i1", &data, &df, &step(&|_| {})).err().expect("blank EIR"));
    assert!(e.contains("the effective interest rate in 'EIR' is blank or not a number on 7 row(s)") && e.contains("the first 5 values of Account_No: A1, A2, A3, A4, A5") && !e.contains("A6"), "{e}");
    // a blank maturity on a stage 1 row, a blank drawn amount, a blank DPD
    let mut df = book(&[1000.0, 1000.0], &["", "2028-10-31"]);
    df.with_column(Series::new("DPD".into(), &[Some(0i64), None])).expect("col");
    let e = format!("{:#}", run("r7i2", &data, &df, &step(&|s| s["dpd_col"] = serde_json::json!("DPD"))).err().expect("blanks"));
    assert!(e.contains("the maturity date in 'Matdat' is blank (a Stage 1 or 2 loan needs its remaining term) on 1 row(s)") && e.contains("the days past due in 'DPD' is blank on 1 row(s)"), "{e}");
    // a Stage 3 row needs neither maturity nor EIR
    let mut df = book(&[1000.0], &[""]);
    df.with_column(Series::new("EIR".into(), &[None::<f64>])).expect("col");
    df.with_column(Series::new("DPD".into(), &[120i64])).expect("col");
    near(floats(&run("r7i3", &data, &df, &step(&|s| s["dpd_col"] = serde_json::json!("DPD"))).expect("runs"), "ECL")[0], 450.0, "stage 3 without maturity and EIR");
    // an unknown segment, a text amount, a negative amount, a term longer than the curve
    let e = format!("{:#}", run("r7i4", &data, &book(&[1.0], &["2028-10-31"]), &step(&|s| {
        s["segment_col"] = serde_json::json!("Account_No");
    })).err().expect("segment"));
    assert!(e.contains("'A1'") && e.contains("not segments of ecl_params/2026-03-08.json") && e.contains("Corporate, Retail"), "{e}");
    // (a Stage 1 loan only uses its first year, however long it runs; a Stage 2 loan needs the curve to its end)
    let mut long = book(&[1.0], &["2040-10-31"]);
    long.with_column(Series::new("Forborne".into(), &["Y"])).expect("col");
    near(floats(&run("r7i5a", &data, &book(&[1000.0], &["2040-10-31"]), &step(&|_| {})).expect("stage 1 runs"), "ECL")[0], 8.18, "stage 1 on a 14-year loan");
    let e = format!("{:#}", run("r7i5", &data, &long, &step(&|s| s["forborne_col"] = serde_json::json!("Forborne"))).err().expect("long term"));
    assert!(e.contains("longer than the 2 years of the PD curve of 'Corporate'"), "{e}");
    let e = format!("{:#}", run("r7i6", &data, &book(&[-5.0], &["2028-10-31"]), &step(&|_| {})).err().expect("negative"));
    assert!(e.contains("negative"), "{e}");
    let text = DataFrame::new(vec![Series::new("Drawn".into(), &["1"]), Series::new("EIR".into(), &[10.0]), dates("Matdat", &["2028-10-31"])]).expect("book");
    let e = format!("{:#}", run("r7i7", &data, &text, &step(&|_| {})).err().expect("text drawn"));
    assert!(e.contains("not a number"), "{e}");
    let e = format!("{:#}", run("r7i8", &data, &book(&[1.0], &["2028-10-31"]), &step(&|s| s["fixed_segment"] = serde_json::json!("Nope"))).err().expect("segment name"));
    assert!(e.contains("choose the segment of the rows (Corporate, Retail)"), "{e}");
    let e = format!("{:#}", run("r7i9", &data, &book(&[1.0], &["2028-10-31"]), &step(&|s| s["cutoff_date"] = serde_json::json!(""))).err().expect("cutoff"));
    assert!(e.contains("type the cutoff date"), "{e}");
}

#[test]
fn r7_the_summary_by_grade_group_and_stage_gives_the_higher_of_and_the_shortfall() {
    // one Stage 2 loan with ECL 19,338.84 and a regulatory provision of 30,000: shortfall 10,661.16
    let df = DataFrame::new(vec![
        Series::new("Account_No".into(), &["A1"]),
        Series::new("CL_Final".into(), &["SMA"]),
        Series::new("Required_Provision_BDT".into(), &[30_000.0]),
        Series::new("Stage".into(), &[2i64]),
        Series::new("EAD".into(), &[1_000_000.0]),
        Series::new("ECL".into(), &[19_338.84]),
    ])
    .expect("tab");
    let data = dir("r7j");
    set_data_dir_override(Some(data.clone()));
    let cl = micro_parquet::banking::ProvisionConfig::load_or_create(&data).expect("cl map").cl_code_map;
    set_data_dir_override(None);
    let spec = EclSummarySpec { grade_col: "CL_Final".into(), provision_col: "Required_Provision_BDT".into() };
    let s = summary_frame(&df, &spec, &cl).expect("summary");
    assert_eq!(s.get_column_names().iter().map(|n| n.to_string()).collect::<Vec<_>>(), SUMMARY_COLUMNS.to_vec());
    let groups = strings(&s, "Group");
    let at = |sec: &str, g: &str| (0..s.height()).find(|i| strings(&s, "Section")[*i].as_deref() == Some(sec) && groups[*i].as_deref() == Some(g)).expect("row");
    let v = |c: &str, r: usize| floats(&s, c)[r];
    near(v("ECL", at("Grade group", "Total")), 19_338.84, "total ECL");
    near(v("Regulatory provision", at("Grade group", "Total")), 30_000.0, "total regulatory provision");
    near(v("Shortfall to Loan Loss Reserve", at("Grade group", "Total")), 10_661.16, "shortfall");
    near(v("Higher of", at("Grade group", "Total")), 30_000.0, "higher of the two totals");
    near(v("Shortfall to Loan Loss Reserve", at("Stage", "Total")), 10_661.16, "the same by stage");
    // the SMA loan is in Standard and in Stage 2; the other groups are empty
    near(v("Loans", at("Grade group", "Standard")), 1.0, "one Standard loan");
    near(v("ECL", at("Grade group", "Standard")), 19_338.84, "Standard ECL");
    near(v("Loans", at("Grade group", "SS")), 0.0, "no SS loan");
    near(v("ECL", at("Stage", "Stage 2")), 19_338.84, "Stage 2 ECL");
    near(v("Regulatory provision", at("Stage", "Stage 1")), 0.0, "no Stage 1 provision");
    assert!(v("Shortfall to Loan Loss Reserve", at("Stage", "Stage 2")).is_none(), "the shortfall is a total, not a row figure");
    // ECL above the provision: no shortfall (never negative); the higher of is the ECL
    let mut df2 = df.clone();
    df2.with_column(Series::new("Required_Provision_BDT".into(), &[5_000.0])).expect("col");
    let s2 = summary_frame(&df2, &spec, &cl).expect("summary");
    let t = (0..s2.height()).find(|i| strings(&s2, "Group")[*i].as_deref() == Some("Total")).expect("total");
    near(floats(&s2, "Shortfall to Loan Loss Reserve")[t], 0.0, "no shortfall");
    near(floats(&s2, "Higher of")[t], 19_338.84, "the ECL is the higher");
    // a table without the ECL columns, an unknown grade
    let e = format!("{:#}", summary_frame(&df.drop("Stage").expect("drop"), &spec, &cl).err().expect("no Stage"));
    assert!(e.contains("no column 'Stage'") && e.contains("IFRS 9 ECL step"), "{e}");
    let mut df3 = df.clone();
    df3.with_column(Series::new("CL_Final".into(), &["ZZ"])).expect("col");
    let e = format!("{:#}", summary_frame(&df3, &spec, &cl).err().expect("grade"));
    assert!(e.contains("not a known code") && e.contains("A1"), "{e}");
    // grade groups from sub-grades and coarse codes
    let df4 = DataFrame::new(vec![
        Series::new("Account_No".into(), &["A1", "A2", "A3", "A4", "A5"]),
        Series::new("CL_Final".into(), &["STD-0", "STD-2", "SS", "DF", "BL"]),
        Series::new("Required_Provision_BDT".into(), &[1.0, 1.0, 20.0, 50.0, 100.0]),
        Series::new("Stage".into(), &[1i64, 2, 3, 3, 3]),
        Series::new("EAD".into(), &[10.0; 5]),
        Series::new("ECL".into(), &[0.1, 0.2, 4.5, 4.5, 4.5]),
    ])
    .expect("tab");
    let s4 = summary_frame(&df4, &spec, &cl).expect("summary");
    let g4 = strings(&s4, "Group");
    let l = |g: &str, sec: &str| floats(&s4, "Loans")[(0..s4.height()).find(|i| g4[*i].as_deref() == Some(g) && strings(&s4, "Section")[*i].as_deref() == Some(sec)).expect("row")];
    assert_eq!((l("Standard", "Grade group"), l("SS", "Grade group"), l("DF", "Grade group"), l("B/L", "Grade group")), (Some(2.0), Some(1.0), Some(1.0), Some(1.0)));
    assert_eq!((l("Stage 1", "Stage"), l("Stage 2", "Stage"), l("Stage 3", "Stage")), (Some(1.0), Some(1.0), Some(3.0)));
}

#[test]
fn r7_the_step_is_saved_in_a_macro_and_a_step_list_without_it_is_unchanged() {
    let s = step(&|s| {
        s["dpd_col"] = serde_json::json!("DPD");
        s["prev_stage_col"] = serde_json::json!("Prev");
    });
    let back: TransformStep = serde_json::from_str(&serde_json::to_string(&s).expect("json")).expect("loads");
    assert_eq!(format!("{back:?}"), format!("{s:?}"));
    assert_eq!(s.title(), "IFRS 9 ECL");
    assert!(micro_parquet::macros::python_untranslated_steps(std::slice::from_ref(&s)).iter().any(|m| m.contains("IFRS 9 ECL")));
    // the new columns may not already be there
    let mut df = book(&[1.0], &["2028-10-31"]);
    df.with_column(Series::new("ECL".into(), &[1.0])).expect("col");
    let e = format!("{:#}", run("r7k", &data_with("r7kd", one_scenario()), &df, &s_prev_less()).err().expect("existing column"));
    assert!(e.contains("already has a column 'ECL'"), "{e}");
}

fn s_prev_less() -> TransformStep {
    step(&|_| {})
}
