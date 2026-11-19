//! P5: Mudaraba profit distribution (weightage / PSR view).

use crate::common::*;
use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::islamic::*;
use micro_parquet::models::*;
use micro_parquet::rules::set_data_dir_override;
use micro_parquet::tool_runs::*;
use polars::prelude::*;
use std::path::PathBuf;

fn params(d: &PathBuf, crr: f64, psr: f64) {
    let dir = d.join("islamic");
    std::fs::create_dir_all(&dir).expect("folder");
    std::fs::write(
        dir.join("psr_2026-01-01.json"),
        serde_json::json!({
            "as_of": "2026-01-01", "source": "test parameters (SSC paper)", "sample": false, "crr_pct": crr, "psr_pct": psr,
            "weightages": [{"product": "MSND", "weight": 0.4}, {"product": "MSB", "weight": 0.6}, {"product": "MTDR-1Y", "weight": 1.0}]
        })
        .to_string(),
    )
    .expect("write");
}

fn deposits() -> DataFrame {
    DataFrame::new(vec![Series::new("Product".into(), &["MSND", "MSB", "MTDR-1Y"]), Series::new("Avg".into(), &[200.0, 300.0, 500.0])]).expect("deposits")
}

fn spec() -> MudarabaSpec {
    MudarabaSpec { product_col: "Product".into(), balance_col: "Avg".into(), daily: false, date_col: String::new(), start: "2026-11-01".into(), end: "2026-11-30".into(), income: 12.0, other_investable: 240.0 }
}

fn run(df: &DataFrame, s: &MudarabaSpec) -> anyhow::Result<DataFrame> {
    let p = load_psr(s.dates()?.1)?;
    build_mudaraba(df, s, &p)
}

fn cell(df: &DataFrame, row: &str, col: &str) -> Option<f64> {
    let i = strings(df, "Product").iter().position(|k| k.as_deref() == Some(row)).unwrap_or_else(|| panic!("row {row}"));
    floats(df, col)[i]
}

fn near(a: Option<f64>, b: f64, tol: f64, what: &str) {
    let a = a.unwrap_or(f64::NAN);
    assert!((a - b).abs() <= tol, "{what}: {a} vs {b}");
}

#[test]
fn p5_the_acceptance_example_gives_the_pool_the_weights_the_profits_and_the_rates() {
    let d = dir("p5_a");
    set_data_dir_override(Some(d.clone()));
    params(&d, 4.0, 70.0);
    let df = run(&deposits(), &spec()).expect("distribution");
    assert_eq!(df.get_column_names().iter().map(|n| n.to_string()).collect::<Vec<_>>(), MUDARABA_COLUMNS.to_vec());
    // investable 960.00, pool share 80.00%, pool income 9.60, depositors 6.72, Mudarib 2.88
    near(cell(&df, "Investable Mudaraba funds", "Average balance"), 960.0, 1e-9, "investable");
    near(cell(&df, "Pool share (%)", "Indicative rate % p.a."), 80.0, 1e-9, "pool share");
    near(cell(&df, "Pool income", "Profit"), 9.6, 1e-9, "pool income");
    near(cell(&df, "Depositors' share", "Profit"), 6.72, 1e-9, "depositors");
    near(cell(&df, "Mudarib share", "Profit"), 2.88, 1e-9, "Mudarib");
    // weighted 80, 180, 500 (sum 760)
    for (p, w) in [("MSND", 80.0), ("MSB", 180.0), ("MTDR-1Y", 500.0)] {
        near(cell(&df, p, "Weighted balance"), w, 1e-9, p);
    }
    near(cell(&df, "Total", "Weighted balance"), 760.0, 1e-9, "sum weighted");
    // profits 0.707368, 1.591579, 4.421053
    for (p, v) in [("MSND", 0.707368), ("MSB", 1.591579), ("MTDR-1Y", 4.421053)] {
        near(cell(&df, p, "Profit"), v, 5e-7, p);
    }
    // rates 4.3032%, 6.4547%, 10.7579%
    for (p, v) in [("MSND", 4.3032), ("MSB", 6.4547), ("MTDR-1Y", 10.7579)] {
        near(cell(&df, p, "Indicative rate % p.a."), v, 5e-5, p);
    }
    // the profits add up to the depositors' share
    near(cell(&df, "Total", "Profit"), 6.72, 1e-9, "profits add up");
    // the parameter file is in the footer, with the period; this one is not a sample
    let notes: Vec<String> = strings(&df, "Note").into_iter().flatten().collect();
    assert!(notes.iter().any(|n| n.contains("islamic/psr_2026-01-01.json") && n.contains("test parameters")), "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("30 day(s)")), "{notes:?}");
    assert!(!strings(&df, "Product").iter().any(|p| p.as_deref() == Some("Sample parameters")));
    set_data_dir_override(None);
}

#[test]
fn p5_the_shipped_file_is_a_sample_and_says_so_and_the_acceptance_numbers_hold_with_it() {
    let d = dir("p5_b");
    set_data_dir_override(Some(d.clone()));
    let p = load_psr(chrono::NaiveDate::from_ymd_opt(2026, 11, 30).expect("date")).expect("the sample is written and read");
    assert!(p.meta.sample && p.meta.source.contains("SAMPLE") && p.meta.source.contains("Shariah Supervisory Committee"));
    assert_eq!(p.meta.file, "islamic/psr_2026-01-01.json");
    assert!(d.join("islamic").join("psr_2026-01-01.json").exists(), "written on first use");
    let df = run(&deposits(), &spec()).expect("distribution");
    near(cell(&df, "Depositors' share", "Profit"), 6.72, 1e-9, "the sample gives the acceptance numbers");
    let notes: Vec<String> = strings(&df, "Note").into_iter().flatten().collect();
    assert!(notes.iter().any(|n| n.contains("marked sample")), "the footer carries the sample warning");
    assert_eq!(strings(&df, "Product").last().cloned().flatten().as_deref(), Some("Sample parameters"));
    // an edited file is not overwritten, and a later file is used for a later period
    std::fs::write(d.join("islamic").join("psr_2026-06-30.json"), std::fs::read_to_string(d.join("islamic").join("psr_2026-01-01.json")).expect("read").replace("2026-01-01", "2026-06-30").replace("\"psr_pct\": 70.0", "\"psr_pct\": 60.0")).expect("write");
    let later = run(&deposits(), &spec()).expect("distribution");
    near(cell(&later, "Depositors' share", "Profit"), 9.6 * 0.6, 1e-9, "the newer file");
    let mut early = spec();
    early.end = "2026-03-31".into();
    early.start = "2026-03-01".into();
    near(cell(&run(&deposits(), &early).expect("distribution"), "Depositors' share", "Profit"), 6.72, 1e-9, "the file in force at the period end");
    let mut before = spec();
    before.start = "2025-12-01".into();
    before.end = "2025-12-31".into();
    let e = format!("{:#}", run(&deposits(), &before).expect_err("no file yet"));
    assert!(e.contains("no PSR file dated on or before 2025-12-31") && e.contains("2026-01-01"), "{e}");
    set_data_dir_override(None);
}

#[test]
fn p5_an_unmapped_product_is_refused_and_named_and_other_bad_input_is_an_error() {
    let d = dir("p5_c");
    set_data_dir_override(Some(d.clone()));
    params(&d, 4.0, 70.0);
    let mut df = deposits();
    df = df.vstack(&DataFrame::new(vec![Series::new("Product".into(), &["MFD-X", "ZZ top"]), Series::new("Avg".into(), &[40.0, 10.0])]).expect("more")).expect("vstack");
    let e = format!("{:#}", run(&df, &spec()).expect_err("unmapped"));
    assert!(e.contains("MFD-X (balance 40.00)") && e.contains("ZZ top") && e.contains("islamic/psr_2026-01-01.json") && e.contains("nothing is guessed"), "{e}");
    // codes are matched ignoring case and spaces
    let ok = DataFrame::new(vec![Series::new("Product".into(), &["msnd ", "Msb", "MTDR-1Y"]), Series::new("Avg".into(), &[200.0, 300.0, 500.0])]).expect("ok");
    near(cell(&run(&ok, &spec()).expect("matches"), "Depositors' share", "Profit"), 6.72, 1e-9, "case and spaces");
    let bad = |f: &dyn Fn(&mut MudarabaSpec), d: &DataFrame| {
        let mut s = spec();
        f(&mut s);
        format!("{:#}", run(d, &s).expect_err("an error"))
    };
    assert!(bad(&|s| s.end = "2026-10-01".into(), &deposits()).contains("ends (2026-10-01) before it starts"));
    assert!(bad(&|s| s.start = "1/11/2026".into(), &deposits()).contains("YYYY-MM-DD"));
    assert!(bad(&|s| s.income = -1.0, &deposits()).contains("investment income"));
    assert!(bad(&|s| s.balance_col = "Nope".into(), &deposits()).contains("'Nope'"));
    let text = DataFrame::new(vec![Series::new("Product".into(), &["MSB"]), Series::new("Avg".into(), &["100"])]).expect("text");
    assert!(bad(&|_| {}, &text).contains("not a number"));
    let neg = DataFrame::new(vec![Series::new("Product".into(), &["MSB"]), Series::new("Avg".into(), &[-5.0])]).expect("neg");
    assert!(bad(&|_| {}, &neg).contains("negative"));
    let blank = DataFrame::new(vec![Series::new("Product".into(), &[Some("MSB"), None]), Series::new("Avg".into(), &[5.0, 1.0])]).expect("blank");
    assert!(bad(&|_| {}, &blank).contains("1 row(s) have no product code"));
    assert!(bad(&|s| { s.other_investable = 0.0; }, &DataFrame::new(vec![Series::new("Product".into(), &["MSB"]), Series::new("Avg".into(), &[0.0])]).expect("zero")).contains("investable funds are 0"));
    // a damaged parameter file is named
    std::fs::write(d.join("islamic").join("psr_2026-01-01.json"), r#"{"as_of":"2026-01-01","source":"x","sample":false,"crr_pct":4,"psr_pct":170,"weightages":[{"product":"A","weight":1}]}"#).expect("write");
    let e = format!("{:#}", run(&deposits(), &spec()).expect_err("psr over 100"));
    assert!(e.contains("psr_pct must be from 0 to 100"), "{e}");
    set_data_dir_override(None);
}

#[test]
fn p5_daily_balances_are_reduced_to_the_average_and_the_tab_and_the_saved_run_work() {
    let d = dir("p5_d");
    set_data_dir_override(Some(d.clone()));
    params(&d, 4.0, 70.0);
    // 30 days of 200 / 300 / 500 give the same averages as the acceptance example
    let mut p = Vec::new();
    let mut b = Vec::new();
    let mut days = Vec::new();
    for day in 1..=30 {
        for (k, v) in [("MSND", 200.0), ("MSB", 300.0), ("MTDR-1Y", 500.0)] {
            p.push(k);
            b.push(v);
            days.push(format!("2026-11-{day:02}"));
        }
    }
    let daily = DataFrame::new(vec![Series::new("Product".into(), p), Series::new("Avg".into(), b), Series::new("Day".into(), days)]).expect("daily");
    let s = MudarabaSpec { daily: true, date_col: "Day".into(), ..spec() };
    let df = run(&daily, &s).expect("daily distribution");
    near(cell(&df, "MSND", "Average balance"), 200.0, 1e-9, "the average of the daily balances");
    near(cell(&df, "Depositors' share", "Profit"), 6.72, 1e-9, "the same pool");
    // the tab source gives the same table and steps can follow
    let src = parquet(&d, "dep", &deposits());
    let tab = TabSource::MudarabaPool { source: Box::new(src.clone()), steps: vec![], spec: spec() };
    let t = compile_tab_lazy(&tab, &[], &d).expect("compiles").collect().expect("collects");
    assert!(t.equals_missing(&run(&deposits(), &spec()).expect("distribution")));
    // the parameter file is among the files a run read
    let (_, used) = micro_parquet::rules::track_used_files(|| compile_tab_lazy(&tab, &[], &d).and_then(|lf| Ok(lf.collect()?)).expect("runs"));
    assert!(used.iter().any(|u| u.kind.contains("Mudaraba") && u.file == "islamic/psr_2026-01-01.json" && !u.sample), "{used:?}");
    // run -> save -> reload -> make again
    struct Env(Vec<(String, RunTab)>, PortablePaths);
    impl RunEnv for Env {
        fn tab(&self, name: &str) -> anyhow::Result<RunTab> {
            self.0.iter().find(|(n, _)| n == name).map(|(_, t)| t.clone()).ok_or_else(|| anyhow::anyhow!("The tab '{name}' is not open."))
        }
        fn name_of(&self, source: &TabSource, steps: &[TransformStep]) -> anyhow::Result<String> {
            let key = |s: &TabSource, st: &[TransformStep]| format!("{:?}|{:?}", s, st);
            self.0.iter().find(|(_, t)| key(&t.source, &t.steps) == key(source, steps)).map(|(n, _)| n.clone()).ok_or_else(|| anyhow::anyhow!("no tab"))
        }
        fn paths(&self) -> &PortablePaths {
            &self.1
        }
    }
    let paths = PortablePaths { root_dir: d.clone(), macros_dir: d.join("m"), schemas_dir: d.join("s"), curves_dir: d.join("c"), buckets_file: d.join("b.json"), temp_dir: d.join("t") };
    let env = Env(vec![("Deposits".into(), RunTab { source: src, steps: vec![] })], paths);
    let run_ = ToolRun::from_source(&tab, &[], &env).expect("run");
    save_run(&d.join("runs"), "mud", &run_).expect("saves");
    let back = load_run(&d.join("runs"), "mud").expect("loads");
    let (title, s2, st2) = back.run.build(&env).expect("builds").remove(0);
    assert_eq!(title, "Mudaraba profit distribution");
    assert!(t.equals_missing(&compile_tab_lazy(&s2, &st2, &d).expect("compiles").collect().expect("collects")));
    set_data_dir_override(None);
}

#[test]
fn g3_blank_balances_dates_outside_the_period_and_missing_days_stop_the_run_naming_them() {
    let d = dir("g3_isl");
    set_data_dir_override(Some(d.clone()));
    params(&d, 4.0, 70.0);
    let frame = |rows: &[(&str, Option<f64>, &str)]| DataFrame::new(vec![
        Series::new("Product".into(), rows.iter().map(|r| r.0).collect::<Vec<_>>()),
        Series::new("Avg".into(), rows.iter().map(|r| r.1).collect::<Vec<_>>()),
        Series::new("Day".into(), rows.iter().map(|r| r.2).collect::<Vec<_>>()),
    ]).expect("frame");
    // a blank average balance: the count and the products
    let blank = frame(&[("MSND", Some(1.0), "2026-11-01"), ("MSB", None, "2026-11-01"), ("MTDR-1Y", None, "2026-11-01")]);
    let msg = format!("{:#}", run(&blank, &spec()).expect_err("blank"));
    assert!(msg.contains("2 row(s) have a blank balance") && msg.contains("MSB, MTDR-1Y"), "{msg}");
    // daily: every product has a balance for every one of the 30 days
    let daily = MudarabaSpec { daily: true, date_col: "Day".into(), ..spec() };
    let full = |skip: Option<(&str, u32)>, extra: Option<&str>| {
        let mut rows: Vec<(String, Option<f64>, String)> = Vec::new();
        for day in 1..=30u32 {
            for k in ["MSND", "MSB", "MTDR-1Y"] {
                if skip == Some((k, day)) || skip == Some((k, 99)) && day <= 3 {
                    continue;
                }
                rows.push((k.to_string(), Some(100.0), format!("2026-11-{day:02}")));
            }
        }
        if let Some(e) = extra {
            rows.push(("MSND".to_string(), Some(100.0), e.to_string()));
        }
        DataFrame::new(vec![
            Series::new("Product".into(), rows.iter().map(|r| r.0.clone()).collect::<Vec<_>>()),
            Series::new("Avg".into(), rows.iter().map(|r| r.1).collect::<Vec<_>>()),
            Series::new("Day".into(), rows.iter().map(|r| r.2.clone()).collect::<Vec<_>>()),
        ]).expect("frame")
    };
    run(&full(None, None), &daily).expect("a full month runs");
    let msg = format!("{:#}", run(&full(None, Some("2026-12-02")), &daily).expect_err("outside"));
    assert!(msg.contains("1 row(s) are dated outside the period 2026-11-01 to 2026-11-30"), "{msg}");
    let msg = format!("{:#}", run(&full(Some(("MSB", 99)), None), &daily).expect_err("missing days"));
    assert!(msg.contains("MSB (3 of 30 days missing)") && !msg.contains("MSND ("), "{msg}");
    let no_col = MudarabaSpec { date_col: String::new(), ..daily.clone() };
    assert!(format!("{:#}", run(&full(None, None), &no_col).expect_err("no date column")).contains("choose the column that holds the day"));
}
