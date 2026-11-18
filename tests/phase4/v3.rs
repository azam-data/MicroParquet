//! V3: the liquidity gap ladder.

use crate::common::*;
use micro_parquet::banking::*;
use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::models::*;
use polars::prelude::*;

const EN: &str = "\u{2013}";

/// The acceptance item's preset: "0–1 month" (up to 30 days) and "1–3 months" (up to 90 days).
pub fn two_bucket_preset() -> CustomBucketDef {
    CustomBucketDef {
        name: "Test ladder".into(),
        rules: vec![
            BucketRuleItem { max_days: 30, label: format!("0{EN}1 month") },
            BucketRuleItem { max_days: 90, label: format!("1{EN}3 months") },
        ],
        overflow_label: "Over 3 months".into(),
        overdue_label: "Overdue".into(),
        null_sight_label: format!("0{EN}1 month"),
        null_long_label: "Over 3 months".into(),
    }
}

fn amounts(name: &str, amt: &[f64], mat: &[&str]) -> DataFrame {
    DataFrame::new(vec![Series::new(name.into(), amt), dates("Matdat", mat)]).expect("frame")
}

/// Assets: 100 in 0–1 month (15 days), 200 in 1–3 months (61 days). Liabilities: 150 and 50.
fn assets() -> DataFrame {
    amounts("Balance", &[100.0, 200.0], &["2026-11-15", "2026-12-31"])
}
fn liabilities() -> DataFrame {
    amounts("Balance", &[150.0, 50.0], &["2026-11-20", "2026-12-15"])
}

fn input(df: &DataFrame, role: GapRole) -> (LazyFrame, GapInputSpec) {
    (df.clone().lazy(), GapInputSpec { role, amount_col: "Balance".into(), maturity_col: "Matdat".into() })
}

fn ladder(inputs: Vec<(LazyFrame, GapInputSpec)>, limits: Option<&GapLimitsFile>) -> DataFrame {
    build_gap_ladder(inputs, &two_bucket_preset(), "2026-10-31", true, limits).unwrap_or_else(|e| panic!("{e:#}"))
}

fn limit(bucket: &str, pct: f64) -> GapLimitsFile {
    GapLimitsFile { preset: "Test ladder".into(), source: "test".into(), effective_from: String::new(), limits: vec![GapLimit { bucket: bucket.into(), min_cumulative_gap_pct: pct }] }
}

#[test]
fn v3_the_acceptance_example_gives_net_cumulative_and_percentages() {
    let out = ladder(vec![input(&assets(), GapRole::Asset), input(&liabilities(), GapRole::Liability)], None);
    assert_eq!(
        out.get_column_names().iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        vec!["Bucket", "Inflows", "Outflows", "Net gap", "Cumulative gap", "Net gap % of total assets", "Cumulative gap % of total assets"]
    );
    // one row per bucket of the preset, in its order
    assert_eq!(strings(&out, "Bucket").into_iter().flatten().collect::<Vec<_>>(), vec![format!("0{EN}1 month"), format!("1{EN}3 months"), "Over 3 months".to_string(), "Total".to_string(), "Overdue assets (not in gap)".to_string()]);
    assert_eq!(&floats(&out, "Inflows")[..2], &[Some(100.0), Some(200.0)]);
    assert_eq!(&floats(&out, "Outflows")[..2], &[Some(150.0), Some(50.0)]);
    assert_eq!(&floats(&out, "Net gap")[..2], &[Some(-50.0), Some(150.0)], "net -50 and +150");
    assert_eq!(&floats(&out, "Cumulative gap")[..2], &[Some(-50.0), Some(100.0)], "cumulative -50 and +100");
    // total assets 300
    let r2 = |x: Option<f64>| x.map(|v| (v * 100.0).round() / 100.0);
    let net_pct = floats(&out, "Net gap % of total assets");
    let cum_pct = floats(&out, "Cumulative gap % of total assets");
    assert_eq!((r2(net_pct[0]), r2(net_pct[1])), (Some(-16.67), Some(50.0)));
    assert_eq!((r2(cum_pct[0]), r2(cum_pct[1])), (Some(-16.67), Some(33.33)));
    // the empty buckets carry the cumulative gap on
    assert_eq!(floats(&out, "Cumulative gap")[2], Some(100.0));
    assert_eq!(floats(&out, "Net gap")[2], Some(0.0));
    assert_eq!(floats(&out, "Net gap")[3], Some(100.0), "the totals row");
    assert_eq!(floats(&out, "Inflows")[4], Some(0.0), "no matured asset");
}

#[test]
fn v3_a_limit_flags_the_breached_row_only_and_the_limit_column_shows_what_is_set() {
    let two = || vec![input(&assets(), GapRole::Asset), input(&liabilities(), GapRole::Liability)];
    let out = ladder(two(), Some(&limit(&format!("0{EN}1 month"), -10.0)));
    assert_eq!(out.width(), 9, "the Limit and Breach columns are added");
    assert_eq!(strings(&out, "Breach").into_iter().flatten().collect::<Vec<_>>(), vec!["BREACH", "", "", "", ""], "-16.67% is below -10% in the first bucket only");
    assert_eq!(floats(&out, "Cumulative gap limit %"), vec![Some(-10.0), None, None, None, None]);
    // a limit that is met is not a breach (equal is met), and a looser limit is not breached
    assert_eq!(strings(&out, "Breach")[1].as_deref(), Some(""));
    let out = ladder(two(), Some(&limit(&format!("0{EN}1 month"), -16.66)));
    assert_eq!(strings(&out, "Breach")[0].as_deref(), Some("BREACH"), "-16.6667 is below -16.66");
    let out = ladder(two(), Some(&limit(&format!("0{EN}1 month"), -16.67)));
    assert_eq!(strings(&out, "Breach")[0].as_deref(), Some(""), "-16.6667 is above -16.67");
    let out = ladder(two(), Some(&limit(&format!("0{EN}1 month"), -20.0)));
    assert_eq!(strings(&out, "Breach")[0].as_deref(), Some(""));
    // a limit on the second bucket: 33.33% against a minimum of 40%
    let out = ladder(two(), Some(&limit(&format!("1{EN}3 months"), 40.0)));
    assert_eq!(strings(&out, "Breach").into_iter().flatten().collect::<Vec<_>>(), vec!["", "BREACH", "", "", ""]);
    // no limits: no columns
    assert_eq!(ladder(two(), None).width(), 7);
}

#[test]
fn v3_blank_maturity_a_side_column_several_tabs_and_overdue_liabilities_and_assets() {
    // a blank maturity is a sight deposit: the sight bucket
    let liab = DataFrame::new(vec![Series::new("Balance".into(), &[40.0, 25.0, 10.0]), dates("Matdat", &["", "2026-09-30", "2028-01-01"])]).expect("frame");
    let out = ladder(vec![input(&assets(), GapRole::Asset), input(&liab, GapRole::Liability)], None);
    // a matured liability (25, due 2026-09-30) still has to be paid: it is in the FIRST bucket with the sight 40
    assert_eq!(strings(&out, "Bucket").into_iter().flatten().collect::<Vec<_>>(), vec![format!("0{EN}1 month"), format!("1{EN}3 months"), "Over 3 months".to_string(), "Total".to_string(), "Overdue assets (not in gap)".to_string()]);
    assert_eq!(floats(&out, "Outflows"), vec![Some(65.0), Some(0.0), Some(10.0), Some(75.0), None], "first bucket 40 + 25, then 0, 10; total 75");
    assert_eq!(&floats(&out, "Net gap")[..3], &[Some(100.0 - 65.0), Some(200.0), Some(-10.0)]);
    assert_eq!(&floats(&out, "Cumulative gap")[..3], &[Some(35.0), Some(235.0), Some(225.0)], "the matured liability counts in the cumulative gap");
    // ... or, when blank maturity is not sight, the preset's long bucket (the matured 25 is still in the first bucket)
    let out = build_gap_ladder(vec![input(&assets(), GapRole::Asset), input(&liab, GapRole::Liability)], &two_bucket_preset(), "2026-10-31", false, None).expect("builds");
    assert_eq!(floats(&out, "Outflows"), vec![Some(25.0), Some(0.0), Some(50.0), Some(75.0), None], "the blank 40 joins over 3 months, the matured 25 the first bucket");
    // a matured asset that is still unpaid is NOT in the gap: a row of its own, below the ladder
    let assets_with_overdue = DataFrame::new(vec![Series::new("Balance".into(), &[100.0, 200.0, 30.0]), dates("Matdat", &["2026-11-15", "2026-12-31", "2026-08-31"])]).expect("frame");
    let out = ladder(vec![input(&assets_with_overdue, GapRole::Asset), input(&liab, GapRole::Liability)], None);
    assert_eq!(strings(&out, "Bucket")[4].as_deref(), Some("Overdue assets (not in gap)"));
    assert_eq!(floats(&out, "Inflows"), vec![Some(100.0), Some(200.0), Some(0.0), Some(300.0), Some(30.0)], "the 30 is in its own row; the Total row stays 300");
    assert_eq!(&floats(&out, "Cumulative gap")[..3], &[Some(35.0), Some(235.0), Some(225.0)], "the overdue asset is not in the cumulative gap");
    for c in ["Net gap", "Cumulative gap", "Net gap % of total assets", "Cumulative gap % of total assets", "Outflows"] {
        assert_eq!(floats(&out, c)[4], None, "{c}: blank on the overdue assets row");
    }
    // total assets for the percentages is every asset, the matured one included: 330
    let pct = floats(&out, "Net gap % of total assets");
    close(pct[0], 35.0 / 330.0 * 100.0, 1e-9, "net gap % of 330");
    // one tab with a side column
    let both = DataFrame::new(vec![
        Series::new("Side".into(), &["Asset", "Deposit", "Asset", "Liab"]),
        Series::new("Balance".into(), &[100.0, 150.0, 200.0, 50.0]),
        dates("Matdat", &["2026-11-15", "2026-11-20", "2026-12-31", "2026-12-15"]),
    ])
    .expect("frame");
    let spec = (both.clone().lazy(), GapInputSpec { role: GapRole::Side("Side".into()), amount_col: "Balance".into(), maturity_col: "Matdat".into() });
    let out = ladder(vec![spec], None);
    assert_eq!(&floats(&out, "Net gap")[..2], &[Some(-50.0), Some(150.0)], "the same ladder from one tab");
    // an unknown side value stops with the message of the rate tools
    let bad = DataFrame::new(vec![Series::new("Side".into(), &["Asset", "Funds"]), Series::new("Balance".into(), &[1.0, 2.0]), dates("Matdat", &["2026-11-15", "2026-11-20"])]).expect("frame");
    let e = format!("{:#}", build_gap_ladder(vec![(bad.lazy(), GapInputSpec { role: GapRole::Side("Side".into()), amount_col: "Balance".into(), maturity_col: "Matdat".into() })], &two_bucket_preset(), "2026-10-31", true, None).err().expect("bad side"));
    assert!(e.contains("neither asset nor liability") && e.contains("'Funds'"), "{e}");
}

#[test]
fn v3_bad_input_is_a_message_and_a_zero_total_has_no_percentages() {
    let run = |inputs: Vec<(LazyFrame, GapInputSpec)>, cutoff: &str, limits: Option<&GapLimitsFile>| format!("{:#}", build_gap_ladder(inputs, &two_bucket_preset(), cutoff, true, limits).err().expect("an error"));
    assert!(build_gap_ladder(vec![], &two_bucket_preset(), "2026-10-31", true, None).err().expect("empty").to_string().contains("add at least one tab"));
    assert!(run(vec![input(&assets(), GapRole::Asset)], "31/10/2026", None).contains("YYYY-MM-DD"));
    let text_amount = DataFrame::new(vec![Series::new("Balance".into(), &["1,000"]), dates("Matdat", &["2026-11-15"])]).expect("frame");
    let e = run(vec![input(&text_amount, GapRole::Asset)], "2026-10-31", None);
    assert!(e.contains("amount column 'Balance'") && e.contains("not a number"), "{e}");
    let text_date = DataFrame::new(vec![Series::new("Balance".into(), &[1.0]), Series::new("Matdat".into(), &["2026-11-15"])]).expect("frame");
    let e = run(vec![input(&text_date, GapRole::Asset)], "2026-10-31", None);
    assert!(e.contains("not a date"), "{e}");
    let missing = GapInputSpec { role: GapRole::Asset, amount_col: "Nope".into(), maturity_col: "Matdat".into() };
    assert!(run(vec![(assets().lazy(), missing)], "2026-10-31", None).contains("no column 'Nope'"));
    // only liabilities: the ladder is made, the percentages are blank, and limits cannot be checked
    let out = ladder(vec![input(&liabilities(), GapRole::Liability)], None);
    assert_eq!(floats(&out, "Net gap % of total assets"), vec![None, None, None, None, None]);
    assert_eq!(floats(&out, "Net gap")[0], Some(-150.0));
    let e = run(vec![input(&liabilities(), GapRole::Liability)], "2026-10-31", Some(&limit(&format!("0{EN}1 month"), -10.0)));
    assert!(e.contains("total assets are 0"), "{e}");
    // a limit for a bucket the preset does not have
    let e = run(vec![input(&assets(), GapRole::Asset)], "2026-10-31", Some(&limit("Nowhere", -10.0)));
    assert!(e.contains("'Nowhere'") && e.contains("not a bucket of the preset"), "{e}");
}

#[test]
fn v3_the_buckets_follow_the_preset_and_the_limits_file_is_read_and_checked() {
    // the SLP preset: sight first (or the long bucket), the day limits in order, overflow, overdue last
    let slp = &CustomBucketDef::default_presets()[0];
    let order = gap_bucket_order(slp, true);
    assert_eq!(order.first().map(String::as_str), Some("01. Overnight / Sight (Non-Maturing)"));
    assert_eq!(order.last().map(String::as_str), Some("10. Overdue / Matured Unencashed"));
    assert_eq!(order.len(), 11, "sight + 8 limits + overflow + overdue");
    let order = gap_bucket_order(slp, false);
    assert!(order.contains(&"09. Over 5 Years (Core)".to_string()) && !order.contains(&"01. Overnight / Sight (Non-Maturing)".to_string()));
    // the file of a preset lives in gap_limits/<preset>.json (the name is made safe for a file)
    let d = dir("v3_limits");
    let p = gap_limits_path(&d, "Bangladesh Bank SLP (9 Liquidity Buckets)");
    assert_eq!(p.file_name().and_then(|n| n.to_str()), Some("Bangladesh Bank SLP (9 Liquidity Buckets).json"));
    assert_eq!(gap_limits_path(&d, "a/b:c").file_name().and_then(|n| n.to_str()), Some("a_b_c.json"));
    let preset = two_bucket_preset();
    assert!(load_gap_limits(&d, &preset).expect("no file is fine").is_none());
    let write = |body: &str| std::fs::write(gap_limits_path(&d, &preset.name), body).expect("write");
    write(&format!(r#"{{"preset":"Test ladder","source":"ALCO test paper","limits":[{{"bucket":"0{EN}1 month","min_cumulative_gap_pct":-10.0}}]}}"#));
    let f = load_gap_limits(&d, &preset).expect("reads").expect("a file");
    assert_eq!((f.source.as_str(), f.limits.len(), f.limits[0].min_cumulative_gap_pct), ("ALCO test paper", 1, -10.0));
    write(r#"{"limits":[{"bucket":"Nowhere","min_cumulative_gap_pct":-10}]}"#);
    assert!(format!("{:#}", load_gap_limits(&d, &preset).err().expect("unknown bucket")).contains("'Nowhere'"));
    write(&format!(r#"{{"limits":[{{"bucket":"Overdue","min_cumulative_gap_pct":-10}},{{"bucket":"Overdue","min_cumulative_gap_pct":-5}}]}}"#));
    assert!(format!("{:#}", load_gap_limits(&d, &preset).err().expect("twice")).contains("two limits for 'Overdue'"));
    write("not json");
    assert!(format!("{:#}", load_gap_limits(&d, &preset).err().expect("bad json")).contains("is not valid"));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn v3_the_ladder_is_a_tab_source_with_steps_after_it() {
    let d = dir("v3_source");
    let src = TabSource::GapLadder {
        inputs: vec![
            GapTabInput { source: parquet(&d, "assets", &assets()), steps: vec![], spec: GapInputSpec { role: GapRole::Asset, amount_col: "Balance".into(), maturity_col: "Matdat".into() } },
            GapTabInput { source: parquet(&d, "liab", &liabilities()), steps: vec![], spec: GapInputSpec { role: GapRole::Liability, amount_col: "Balance".into(), maturity_col: "Matdat".into() } },
        ],
        preset: two_bucket_preset(),
        cutoff_date: "2026-10-31".into(),
        null_as_sight: true,
        limits: Some(limit(&format!("0{EN}1 month"), -10.0)),
    };
    let steps = vec![TransformStep::FilterRows { column: "Breach".into(), operator: FilterOp::Eq, value: "BREACH".into(), keep_matching: true }];
    let out = compile_tab_lazy(&src, &steps, &d).expect("compiles").collect().expect("collects");
    assert_eq!(out.height(), 1, "a step after the ladder keeps the breached rows");
    // the tab's own steps feed the ladder: drop the second asset
    let filtered = TabSource::GapLadder {
        inputs: vec![GapTabInput {
            source: parquet(&d, "assets2", &assets()),
            steps: vec![TransformStep::FilterRows { column: "Balance".into(), operator: FilterOp::Eq, value: "100".into(), keep_matching: true }],
            spec: GapInputSpec { role: GapRole::Asset, amount_col: "Balance".into(), maturity_col: "Matdat".into() },
        }],
        preset: two_bucket_preset(),
        cutoff_date: "2026-10-31".into(),
        null_as_sight: true,
        limits: None,
    };
    let out = compile_tab_lazy(&filtered, &[], &d).expect("compiles").collect().expect("collects");
    assert_eq!(floats(&out, "Inflows")[1], Some(0.0));
    assert_ne!(micro_parquet::steps_edit::pipeline_hash(&src, &[]), micro_parquet::steps_edit::pipeline_hash(&filtered, &[]));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn v3_the_shipped_sample_limits_file_is_marked_as_a_sample_and_fits_the_slp_preset() {
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs").join("samples").join("gap_limits").join("SAMPLE Bangladesh Bank SLP (9 Liquidity Buckets).json");
    let f: GapLimitsFile = serde_json::from_str(&std::fs::read_to_string(&sample).expect("the sample file")).expect("it parses");
    assert!(f.source.starts_with("SAMPLE ONLY"), "a sample must say so: {}", f.source);
    // its buckets are real buckets of the preset (so copying it into portable_data works), and nothing is built in
    let d = dir("v3_sample");
    let slp = &CustomBucketDef::default_presets()[0];
    std::fs::write(gap_limits_path(&d, &slp.name), std::fs::read_to_string(&sample).expect("read")).expect("copy");
    let loaded = load_gap_limits(&d, slp).expect("valid for the preset").expect("found");
    assert_eq!(loaded.limits.len(), 3);
    let _ = std::fs::remove_dir_all(&d);
}
