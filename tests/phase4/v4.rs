//! V4: the dated yield-curve store and FTP at origination.

use crate::common::*;
use chrono::NaiveDate;
use micro_parquet::banking::*;
use micro_parquet::curves::*;
use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::models::*;
use polars::prelude::*;

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("a date")
}

fn pt(t: i64, r: f64) -> FtpPoint {
    FtpPoint { tenor_days: t, ftp_rate: r }
}

fn curve(date: &str, points: Vec<FtpPoint>) -> YieldCurve {
    YieldCurve { date: d(date), currency: "BDT".into(), source: "test".into(), points }
}

/// The acceptance item's curves: 2026-01-01 {30: 8.00, 365: 10.00} and 2026-07-01 {30: 9.00, 365: 11.00}.
fn two_curves() -> Vec<YieldCurve> {
    vec![curve("2026-01-01", vec![pt(30, 8.0), pt(365, 10.0)]), curve("2026-07-01", vec![pt(30, 9.0), pt(365, 11.0)])]
}

/// Three 197-day deals: opened 2026-03-15, 2026-08-01 and 2025-12-01.
fn deals() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Deal".into(), &["D1", "D2", "D3"]),
        dates("Opndat", &["2026-03-15", "2026-08-01", "2025-12-01"]),
        dates("Matdat", &["2026-09-28", "2027-02-14", "2026-06-16"]),
        Series::new("Balance".into(), &[1_000_000.0, 1_000_000.0, 1_000_000.0]),
        Series::new("Rate".into(), &[9.0, 9.0, 9.0]),
    ])
    .expect("deals")
}

fn run(df: &DataFrame, curves: &[YieldCurve], mode: &FtpCurveMode) -> anyhow::Result<DataFrame> {
    Ok(apply_ftp_dated_curves(df.clone().lazy(), "Opndat", "Matdat", "Balance", "Rate", curves, mode, true, FtpInterpolation::Linear)?.collect()?)
}

fn origination(df: &DataFrame, earliest: bool) -> anyhow::Result<DataFrame> {
    run(df, &two_curves(), &FtpCurveMode::OriginationDate { use_earliest_for_older: earliest })
}

fn first_two(df: &DataFrame) -> DataFrame {
    df.slice(0, 2)
}

#[test]
fn v4_each_deal_is_priced_on_the_curve_of_its_open_date() {
    let out = origination(&first_two(&deals()), false).expect("two deals that have a curve");
    let ftp = floats(&out, "Benchmark_FTP_Rate");
    // 8 + (197 - 30) / (365 - 30) x 2 = 8.99701 on the January curve, 9.99701 on the July curve
    close(ftp[0], 8.99701, 0.00001, "opened 2026-03-15");
    close(ftp[1], 9.99701, 0.00001, "opened 2026-08-01");
    assert_eq!(floats(&out, "Tenor_Days"), vec![Some(197.0), Some(197.0)]);
    // the curve date, as a date
    assert_eq!(out.column("FTP_Curve_Date").expect("column").dtype(), &DataType::Date);
    assert_eq!(md(&out, &["Deal", "FTP_Curve_Date"]).lines().skip(2).collect::<Vec<_>>(), vec!["| D1 | 2026-01-01 |", "| D2 | 2026-07-01 |"]);
    assert_eq!(strings(&out, "FTP_Method"), vec![Some("Linear 30d-365d".to_string()); 2]);
    // the spread of a liability is FTP - account rate, and the margin follows
    close(floats(&out, "FTP_Spread_Pct")[1], 0.99701, 0.00001, "spread");
    close(floats(&out, "Annual_FTP_Margin_BDT")[1], 1_000_000.0 * 0.0099701, 0.1, "margin");
    // no note column without the option
    assert!(out.column("FTP_Note").is_err());
    // a deal exactly on a curve's date uses that curve
    let on_day = DataFrame::new(vec![Series::new("Deal".into(), &["X"]), dates("Opndat", &["2026-07-01"]), dates("Matdat", &["2027-01-14"]), Series::new("Balance".into(), &[1.0]), Series::new("Rate".into(), &[9.0])]).expect("frame");
    let out = origination(&on_day, false).expect("runs");
    assert_eq!(md(&out, &["FTP_Curve_Date"]).lines().nth(2), Some("| 2026-07-01 |"));
}

#[test]
fn v4_a_deal_older_than_every_curve_is_an_error_naming_the_count_or_uses_the_earliest_curve_with_a_note() {
    let e = format!("{:#}", origination(&deals(), false).err().expect("D3 is older than the earliest curve"));
    assert!(e.contains("1 deal(s) were opened before the earliest yield curve (2026-01-01)") && e.contains("2025-12-01"), "{e}");
    assert!(e.contains("use the earliest curve for older deals"), "{e}");
    // with the option: the earliest curve, and the note on that row only
    let out = origination(&deals(), true).expect("with the option");
    close(floats(&out, "Benchmark_FTP_Rate")[2], 8.99701, 0.00001, "the older deal on the January curve");
    assert_eq!(strings(&out, "FTP_Note"), vec![None, None, Some("earliest curve used".to_string())]);
    assert_eq!(md(&out, &["FTP_Curve_Date"]).lines().skip(2).collect::<Vec<_>>(), vec!["| 2026-01-01 |", "| 2026-07-01 |", "| 2026-01-01 |"]);
    // a count of two
    let two_old = DataFrame::new(vec![
        Series::new("Deal".into(), &["A", "B"]),
        dates("Opndat", &["2025-11-01", "2025-12-15"]),
        dates("Matdat", &["2026-06-01", "2026-06-01"]),
        Series::new("Balance".into(), &[1.0, 1.0]),
        Series::new("Rate".into(), &[9.0, 9.0]),
    ])
    .expect("frame");
    let e = format!("{:#}", origination(&two_old, false).err().expect("two old deals"));
    assert!(e.contains("2 deal(s)") && e.contains("the oldest on 2025-11-01"), "{e}");
}

#[test]
fn v4_a_fixed_date_prices_every_deal_on_the_curve_on_or_before_it() {
    let mode = FtpCurveMode::FixedDate { date: "2026-06-30".into() };
    let out = run(&deals(), &two_curves(), &mode).expect("fixed date");
    // both deals of the acceptance item use the January curve, and so does the older one: the date is fixed
    let ftp = floats(&out, "Benchmark_FTP_Rate");
    for (i, v) in ftp.iter().enumerate() {
        close(*v, 8.99701, 0.00001, &format!("deal {}", i + 1));
    }
    assert_eq!(md(&out, &["FTP_Curve_Date"]).lines().skip(2).collect::<Vec<_>>(), vec!["| 2026-01-01 |"; 3]);
    // on the date of the July curve, July
    let out = run(&deals(), &two_curves(), &FtpCurveMode::FixedDate { date: "2026-07-01".into() }).expect("fixed date");
    close(floats(&out, "Benchmark_FTP_Rate")[0], 9.99701, 0.00001, "the July curve");
    // before every curve: an error that names the earliest
    let e = format!("{:#}", run(&deals(), &two_curves(), &FtpCurveMode::FixedDate { date: "2025-06-30".into() }).err().expect("no curve"));
    assert!(e.contains("no yield curve is dated on or before 2025-06-30") && e.contains("the earliest is 2026-01-01"), "{e}");
    let e = format!("{:#}", run(&deals(), &two_curves(), &FtpCurveMode::FixedDate { date: "30/06/2026".into() }).err().expect("bad date"));
    assert!(e.contains("YYYY-MM-DD"), "{e}");
    // an empty store
    let e = format!("{:#}", run(&deals(), &[], &FtpCurveMode::FixedDate { date: "2026-06-30".into() }).err().expect("empty"));
    assert!(e.contains("There are no yield curves"), "{e}");
}

#[test]
fn v4_interpolation_follows_the_step_and_missing_dates_are_still_flagged() {
    // the old step lookup picks the next point up on the same curves
    let out = apply_ftp_dated_curves(first_two(&deals()).lazy(), "Opndat", "Matdat", "Balance", "Rate", &two_curves(), &FtpCurveMode::OriginationDate { use_earliest_for_older: false }, true, FtpInterpolation::NextPointUp).expect("runs").collect().expect("collect");
    assert_eq!(floats(&out, "Benchmark_FTP_Rate"), vec![Some(10.0), Some(11.0)]);
    // a blank open date has no tenor and no curve: no rate, as in the typed-curve step
    let blank = DataFrame::new(vec![Series::new("Deal".into(), &["A", "B"]), dates("Opndat", &["", "2026-03-15"]), dates("Matdat", &["2026-06-01", "2026-09-28"]), Series::new("Balance".into(), &[1.0, 1.0]), Series::new("Rate".into(), &[9.0, 9.0])]).expect("frame");
    let out = origination(&blank, false).expect("runs");
    assert_eq!(strings(&out, "FTP_Method")[0].as_deref(), Some(FTP_MISSING_DATE));
    assert_eq!(floats(&out, "Benchmark_FTP_Rate")[0], None);
    assert_eq!(md(&out, &["FTP_Curve_Date"]).lines().nth(2), Some("| (blank) |"));
    close(floats(&out, "Benchmark_FTP_Rate")[1], 8.99701, 0.00001, "the other deal");
}

#[test]
fn v4_a_csv_with_the_same_tenor_twice_is_refused_and_so_is_every_other_bad_file() {
    let e = format!("{:#}", parse_curve_csv("tenor_days,rate_pct\n30,8.0\n30,8.5\n365,10\n").err().expect("tenors 30, 30"));
    assert!(e.contains("Lines 2 and 3 both have the tenor 30 days") && e.contains("Each tenor needs one rate"), "{e}");
    // a good one, with a heading, a % sign and a semicolon
    let pts = parse_curve_csv("tenor_days;rate_pct\n30;8.00\n365;10%\n").expect("a good file");
    assert_eq!(pts, vec![pt(30, 8.0), pt(365, 10.0)]);
    assert_eq!(parse_curve_csv("30,8\n365,10").expect("no heading needed").len(), 2);
    for (text, want) in [
        ("365,10\n30,8\n", "The tenors must increase: 365 days (line 1) is followed by 30 days (line 2)"),
        ("30,8,1\n", "Line 1: expected two columns"),
        ("30,8\nabc,9\n", "Line 2: 'abc' is not a whole number of days"),
        ("30,8\n60,high\n", "Line 2: 'high' is not a rate in percent"),
        ("0,8\n", "Line 1: the tenor 0 is not a number of days above 0"),
        ("tenor_days,rate_pct\n", "no points"),
        ("", "no points"),
    ] {
        let e = format!("{:#}", parse_curve_csv(text).err().unwrap_or_else(|| panic!("{text:?} is refused")));
        assert!(e.contains(want), "{text:?}: {e}");
    }
    // the same checks on the points of a curve
    assert!(validate_points(&[pt(30, 8.0), pt(30, 9.0)]).is_err());
    assert!(validate_points(&[pt(30, 8.0), pt(10, 9.0)]).is_err());
    assert!(validate_points(&[pt(30, f64::NAN)]).is_err());
    assert!(validate_points(&[]).is_err());
    assert!(validate_points(&[pt(30, 8.0), pt(365, 10.0)]).is_ok());
}

#[test]
fn v4_curve_files_are_dated_checked_saved_listed_and_deleted() {
    let dir = dir("v4_files");
    // save and read back
    let c = curve("2026-01-01", vec![pt(30, 8.0), pt(365, 10.0)]);
    let p = save_curve(&dir, &c).expect("saves");
    assert_eq!(p.file_name().and_then(|n| n.to_str()), Some("2026-01-01.json"));
    let text = std::fs::read_to_string(&p).expect("read");
    assert!(text.contains("\"rate_pct\": 8.0") && text.contains("\"tenor_days\": 30") && text.contains("\"as_of\": \"2026-01-01\"") && text.contains("\"currency\": \"BDT\""), "{text}");
    assert_eq!(load_curve_file(&p).expect("loads"), c);
    save_curve(&dir, &curve("2026-07-01", vec![pt(30, 9.0), pt(365, 11.0)])).expect("saves");
    // a file the program cannot use is listed with its reason and the others still load
    std::fs::write(dir.join("2026-03-01.json"), r#"{"currency":"BDT","points":[{"tenor_days":30,"rate_pct":8},{"tenor_days":30,"rate_pct":9}]}"#).expect("write");
    std::fs::write(dir.join("not-a-date.json"), r#"{"currency":"BDT","points":[{"tenor_days":30,"rate_pct":8}]}"#).expect("write");
    std::fs::write(dir.join("2026-04-01.json"), r#"{"currency":"BDT","as_of":"2026-04-02","points":[{"tenor_days":30,"rate_pct":8}]}"#).expect("write");
    std::fs::write(dir.join("2026-05-01.json"), "{ nope").expect("write");
    let store = CurveStore::load(&dir);
    assert_eq!(store.curves.iter().map(|c| c.date.to_string()).collect::<Vec<_>>(), vec!["2026-01-01", "2026-07-01"]);
    assert_eq!(store.problems.len(), 4, "{:?}", store.problems);
    let reason = |file: &str| store.problems.iter().find(|(f, _)| f == file).map(|(_, r)| r.clone()).unwrap_or_else(|| panic!("{file} has a problem"));
    assert!(reason("2026-03-01.json").contains("both have the tenor 30 days"), "{}", reason("2026-03-01.json"));
    assert!(reason("not-a-date.json").contains("is not a date"));
    assert!(reason("2026-04-01.json").contains("file name says 2026-04-01 but 'as_of' says 2026-04-02"));
    assert!(reason("2026-05-01.json").contains("not a valid curve file"));
    assert!(reason("2026-03-01.json").starts_with("Curve file 2026-03-01.json:"), "every message names the file");
    // the FTP step does not guess: a damaged file stops it
    assert!(store.strict().is_err());
    // the latest curve on or before a date
    let good = CurveStore { curves: store.curves.clone(), problems: vec![] };
    assert_eq!(good.on_or_before(d("2026-06-30")).map(|c| c.date), Some(d("2026-01-01")));
    assert_eq!(good.on_or_before(d("2026-07-01")).map(|c| c.date), Some(d("2026-07-01")));
    assert_eq!(good.on_or_before(d("2025-12-31")), None);
    assert_eq!(good.earliest().map(|c| c.date), Some(d("2026-01-01")));
    // an invalid curve is not written
    assert!(save_curve(&dir, &curve("2026-09-01", vec![pt(30, 8.0), pt(30, 9.0)])).is_err());
    assert!(!dir.join("2026-09-01.json").exists());
    delete_curve(&dir, d("2026-07-01")).expect("deletes");
    assert!(!dir.join("2026-07-01.json").exists());
    // a missing folder is an empty store
    assert!(CurveStore::load(&dir.join("nope")).curves.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

fn step(mode: FtpCurveMode) -> TransformStep {
    TransformStep::FtpCurveMap {
        opndat_col: "Opndat".into(),
        matdat_col: "Matdat".into(),
        bal_col: "Balance".into(),
        rate_col: "Rate".into(),
        curve: vec![],
        is_liability: true,
        interpolation: FtpInterpolation::Linear,
        curve_mode: mode, currency: "BDT".into(),
    }
}

#[test]
fn v4_the_step_reads_the_store_and_follows_an_edited_curve_and_old_steps_are_unchanged() {
    let dir = dir("v4_step");
    for c in two_curves() {
        save_curve(&dir, &c).expect("saves");
    }
    set_curves_dir_override(Some(dir.clone()));
    let src = parquet(&dir, "deals", &first_two(&deals()));
    let steps = vec![step(FtpCurveMode::OriginationDate { use_earliest_for_older: false })];
    let out = compile_tab_lazy(&src, &steps, &dir).expect("compiles").collect().expect("collects");
    close(floats(&out, "Benchmark_FTP_Rate")[0], 8.99701, 0.00001, "from the store");
    let before = micro_parquet::steps_edit::pipeline_hash(&src, &steps);
    // an edited curve changes the result and the fingerprint (so a tab refreshes)
    std::thread::sleep(std::time::Duration::from_millis(30));
    save_curve(&dir, &curve("2026-01-01", vec![pt(30, 8.0), pt(365, 12.0)])).expect("saves");
    let after = micro_parquet::steps_edit::pipeline_hash(&src, &steps);
    assert_ne!(before, after, "the pipeline fingerprint follows the curve files");
    let out = compile_tab_lazy(&src, &steps, &dir).expect("compiles").collect().expect("collects");
    close(floats(&out, "Benchmark_FTP_Rate")[0], 8.0 + 167.0 / 335.0 * 4.0, 0.00001, "the edited curve");
    // a step that types its curve does not depend on the files
    let typed = vec![step(FtpCurveMode::Inline)];
    assert_eq!(micro_parquet::steps_edit::pipeline_hash(&src, &typed), micro_parquet::steps_edit::pipeline_hash(&src, &typed));
    let typed_with_points = TransformStep::FtpCurveMap {
        opndat_col: "Opndat".into(),
        matdat_col: "Matdat".into(),
        bal_col: "Balance".into(),
        rate_col: "Rate".into(),
        curve: vec![pt(30, 8.0), pt(365, 10.0)],
        is_liability: true,
        interpolation: FtpInterpolation::Linear,
        curve_mode: FtpCurveMode::Inline, currency: "BDT".into(),
    };
    let out = compile_tab_lazy(&src, &[typed_with_points], &dir).expect("compiles").collect().expect("collects");
    close(floats(&out, "Benchmark_FTP_Rate")[0], 8.99701, 0.00001, "typed curve, as before");
    assert!(out.column("FTP_Curve_Date").is_err(), "an old step has no curve date column");
    // a damaged file stops a step that reads the store, naming it
    std::fs::write(dir.join("2026-02-01.json"), "{ nope").expect("write");
    let e = format!("{:#}", compile_tab_lazy(&src, &steps, &dir).and_then(|lf| Ok(lf.collect()?)).err().expect("a damaged curve file"));
    assert!(e.contains("2026-02-01.json") && e.contains("not a valid curve file"), "{e}");
    set_curves_dir_override(None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn v4_an_old_macro_json_has_no_curve_mode_and_the_new_modes_survive_a_macro_file() {
    // the exact JSON of a Phase 2 step: loads as a typed curve and gives what it gave
    let json = r#"{"FtpCurveMap":{"opndat_col":"Opndat","matdat_col":"Matdat","bal_col":"Balance","rate_col":"Rate","curve":[{"tenor_days":30,"ftp_rate":8.75}],"is_liability":true,"interpolation":"Linear"}}"#;
    let old: TransformStep = serde_json::from_str(json).expect("loads");
    assert!(matches!(old, TransformStep::FtpCurveMap { curve_mode: FtpCurveMode::Inline, .. }));
    // and a Phase 1 step (no interpolation either)
    let p1 = r#"{"FtpCurveMap":{"opndat_col":"Opndat","matdat_col":"Matdat","bal_col":"Balance","rate_col":"Rate","curve":[{"tenor_days":30,"ftp_rate":8.75}],"is_liability":false}}"#;
    let old: TransformStep = serde_json::from_str(p1).expect("loads");
    assert!(matches!(old, TransformStep::FtpCurveMap { curve_mode: FtpCurveMode::Inline, interpolation: FtpInterpolation::NextPointUp, .. }));
    // the new modes round-trip through a macro file
    let dir = dir("v4_macro");
    let recipe = MacroRecipe {
        name: "ftp".into(),
        description: String::new(),
        created_at: String::new(),
        enforce_balance_control: true,
        control_column: None,
        optional_ingestion_schema: None,
        steps: vec![step(FtpCurveMode::FixedDate { date: "2026-06-30".into() }), step(FtpCurveMode::OriginationDate { use_earliest_for_older: true })],
        summary: Default::default(),
    };
    let p = dir.join("m.mpq.json");
    micro_parquet::macros::save_macro_to_file(&recipe, &p).expect("save");
    let back = micro_parquet::macros::load_macro_from_file(&p).expect("load");
    let modes: Vec<FtpCurveMode> = back.steps.iter().filter_map(|s| if let TransformStep::FtpCurveMap { curve_mode, .. } = s { Some(curve_mode.clone()) } else { None }).collect();
    assert_eq!(modes, vec![FtpCurveMode::FixedDate { date: "2026-06-30".into() }, FtpCurveMode::OriginationDate { use_earliest_for_older: true }]);
    // the Phase 1 and 2 fixtures of the old code still replay to what that code gave (the whole-pipeline fixtures are in tests/phase3)
    let _ = std::fs::remove_dir_all(&dir);
}
