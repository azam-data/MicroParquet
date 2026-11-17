//! Helpers shared by the Phase 4 tests.

use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::models::*;
use polars::prelude::*;
use std::path::{Path, PathBuf};

/// A fresh empty folder under the system temp folder, unique per test name and process.
pub fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mp_p4_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("create the test folder");
    d
}

/// Days since 1970-01-01 of a "YYYY-MM-DD" text.
pub fn day(s: &str) -> i32 {
    let d = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("a test date");
    (d - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch")).num_days() as i32
}

/// A Date column from "YYYY-MM-DD" texts; an empty text is a blank date.
pub fn dates(name: &str, values: &[&str]) -> Series {
    let days: Vec<Option<i32>> = values.iter().map(|v| if v.is_empty() { None } else { Some(day(v)) }).collect();
    Series::new(name.into(), days).cast(&DataType::Date).expect("cast to Date").into()
}

/// Writes the frame to Parquet and returns it as a single-file tab source.
pub fn parquet(d: &Path, name: &str, df: &DataFrame) -> TabSource {
    let p = d.join(format!("{name}.parquet"));
    let mut df = df.clone();
    ParquetWriter::new(std::fs::File::create(&p).expect("create parquet")).finish(&mut df).expect("write parquet");
    TabSource::SingleFile { path: p, is_parquet: true, config: IngestionConfig::default() }
}

/// Runs the steps on a tab made from `df`, through the same compiler the app uses.
pub fn run_steps(name: &str, df: &DataFrame, steps: &[TransformStep]) -> anyhow::Result<DataFrame> {
    let d = dir(name);
    let src = parquet(&d, "in", df);
    let out = compile_tab_lazy(&src, steps, &d).and_then(|lf| Ok(lf.collect()?));
    let _ = std::fs::remove_dir_all(&d);
    out
}

pub fn strings(df: &DataFrame, name: &str) -> Vec<Option<String>> {
    df.column(name).expect("column").str().expect("text column").into_iter().map(|v| v.map(|s| s.to_string())).collect()
}

pub fn floats(df: &DataFrame, name: &str) -> Vec<Option<f64>> {
    df.column(name)
        .expect("column")
        .cast(&DataType::Float64)
        .expect("cast")
        .f64()
        .expect("f64")
        .into_iter()
        .collect()
}

pub fn close(actual: Option<f64>, expected: f64, tol: f64, what: &str) {
    let a = actual.unwrap_or(f64::NAN);
    assert!((a - expected).abs() <= tol, "{what}: expected {expected}, got {actual:?}");
}

/// 1,234.5 style text: thousands separators, at most `dp` decimals, no trailing zeros.
pub fn fmt_num(x: f64, dp: usize) -> String {
    let s = format!("{:.*}", dp, x.abs());
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), f.trim_end_matches('0').to_string()),
        None => (s, String::new()),
    };
    let mut grouped = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let sign = if x < 0.0 && (grouped != "0" || !frac.is_empty()) { "-" } else { "" };
    if frac.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}.{frac}")
    }
}

/// A money amount with exactly two decimals and thousands separators: 1,350.50.
pub fn money(x: f64) -> String {
    let s = fmt_num(x, 2);
    match s.split_once('.') {
        Some((i, f)) => format!("{i}.{f:0<2}"),
        None => format!("{s}.00"),
    }
}

/// One cell as the guide shows it. `fixed` = always that many decimals (money columns).
fn cell(v: AnyValue, dp: Option<usize>, fixed: bool) -> String {
    match v {
        AnyValue::Null => "(blank)".to_string(),
        AnyValue::String(s) if s.trim().is_empty() => "(blank)".to_string(),
        AnyValue::StringOwned(s) if s.trim().is_empty() => "(blank)".to_string(),
        AnyValue::String(s) => s.to_string(),
        AnyValue::StringOwned(s) => s.to_string(),
        AnyValue::Boolean(b) => b.to_string(),
        AnyValue::Float64(x) => {
            if fixed {
                let s = fmt_num(x, dp.unwrap_or(2));
                // keep the zeros of a money column: 1,000.00
                let want = dp.unwrap_or(2);
                match s.split_once('.') {
                    Some((i, f)) => format!("{i}.{f:0<want$}"),
                    None => format!("{s}.{}", "0".repeat(want)),
                }
            } else {
                fmt_num(x, dp.unwrap_or(4))
            }
        }
        AnyValue::Date(d) => {
            let base = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch");
            (base + chrono::Duration::days(d as i64)).format("%Y-%m-%d").to_string()
        }
        AnyValue::Int64(x) => fmt_num(x as f64, 0),
        AnyValue::Int32(x) => fmt_num(x as f64, 0),
        AnyValue::UInt64(x) => fmt_num(x as f64, 0),
        AnyValue::UInt32(x) => fmt_num(x as f64, 0),
        other => format!("{other}"),
    }
}

/// A Markdown table of the chosen columns. A column name may end in `:2` (always two decimals,
/// for money) or `:~4` (up to four decimals, no trailing zeros); numbers are right-aligned.
pub fn md(df: &DataFrame, cols: &[&str]) -> String {
    let mut head = String::from("|");
    let mut rule = String::from("|");
    let mut specs: Vec<(String, Option<usize>, bool)> = Vec::new();
    for c in cols {
        let (name, dp, fixed) = match c.rsplit_once(':') {
            Some((n, "2")) => (n, Some(2), true),
            Some((n, "0")) => (n, Some(0), true),
            Some((n, t)) if t.starts_with('~') => (n, t[1..].parse::<usize>().ok(), false),
            _ => (*c, None, false),
        };
        let column = df.column(name).unwrap_or_else(|_| panic!("column {name} is not in the frame"));
        let numeric = column.dtype().is_numeric();
        head.push_str(&format!(" {name} |"));
        rule.push_str(if numeric { " ---: |" } else { " --- |" });
        specs.push((name.to_string(), dp, fixed));
    }
    let mut out = format!("{head}\n{rule}\n");
    for i in 0..df.height() {
        let mut line = String::from("|");
        for (name, dp, fixed) in &specs {
            let v = df.column(name).expect("column").get(i).expect("row");
            line.push_str(&format!(" {} |", cell(v, *dp, *fixed)));
        }
        out.push_str(&line);
        out.push('\n');
    }
    out.trim_end().to_string()
}

// ------------------------------------------------------------------ the old-macro fixtures

/// The synthetic loan book the old-macro fixtures are replayed on (the same as in tests/phase3).
pub fn fixture_frame() -> DataFrame {
    let d = |v: &[&str], name: &str| dates(name, v);
    DataFrame::new(vec![
        Series::new("Actnum".into(), &["1001", "1002", "1003", "1004", "1005", "1006", "1007", "1008"]),
        Series::new("Brancd".into(), &["DHK", "DHK", "CTG", "CTG", "SYL", "DHK", "XXX", "CTG"]),
        Series::new("Acttit".into(), &["Rahim Traders Ltd.", "RAHIM TRADERS LIMITED", "Karim & Sons", "Karim and Sons", "Delta Agro", "Bay Shipping", "Test", "Delta Agro Ltd"]),
        Series::new("Balance".into(), &[50_000_000.0f64, 25_000_000.0, 120_000_000.0, 8_000_000.0, 0.0, 300_000_000.0, 1.0, 45_000_000.0]),
        Series::new("Int Rate".into(), &[9.5f64, 10.25, 11.0, 8.75, 12.0, 7.5, 9.0, 10.5]),
        d(&["2024-01-15", "2025-03-01", "2023-07-10", "2025-11-20", "2022-02-28", "2026-01-05", "2024-09-09", "2025-06-30"], "Opndat"),
        d(&["2026-07-15", "2026-09-30", "", "2027-12-31", "2026-05-31", "2031-01-05", "2026-08-01", "2028-06-30"], "Matdat"),
        d(&["", "", "", "", "2026-05-31", "", "", ""], "Clsdat"),
        Series::new("Acstat".into(), &["ACT", "DMT", "ACT", "ACT", "CLS", "ACT", "ACT", "DMT"]),
        Series::new("CL_Status".into(), &["UC", "SMA", "UC", "SS", "UC", "UC", "UC", "BL"]),
        Series::new("Sector".into(), &["SMALL", "MICRO", "OTHER", "SMALL", "AGRI ST", "OTHER", "OTHER", "COTTAGE"]),
        Series::new("Sanctioned_Limit".into(), &[60_000_000.0f64, 25_000_000.0, 150_000_000.0, 10_000_000.0, 5_000_000.0, 300_000_000.0, 1.0, 50_000_000.0]),
        Series::new("Side".into(), &["Asset", "Asset", "Asset", "Liability", "Asset", "Asset", "Asset", "Asset"]),
    ])
    .expect("the fixture frame")
}

/// Replays `steps` on the fixture frame and returns the result as CSV text.
pub fn replay_to_csv(steps: &[TransformStep], d: &Path) -> String {
    let src = parquet(d, "fixture_loans", &fixture_frame());
    let mut out = compile_tab_lazy(&src, steps, d).expect("the steps compile").collect().expect("the steps run");
    let mut buf = Vec::new();
    CsvWriter::new(&mut buf).finish(&mut out).expect("write the CSV");
    String::from_utf8(buf).expect("UTF-8")
}

pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures")
}

/// Loads an old macro fixture, replays it and compares it with the CSV the old code wrote.
pub fn assert_fixture_replays(name: &str, expected_kinds: &[&str]) {
    let json = fixtures_dir().join(format!("{name}.mpq.json"));
    let recipe = micro_parquet::macros::load_macro_from_file(&json).unwrap_or_else(|e| panic!("{name}: {e:#}"));
    for k in expected_kinds {
        assert!(recipe.steps.iter().any(|s| format!("{s:?}").starts_with(&format!("{k} "))), "{name} has a {k} step");
    }
    let d = dir(&format!("fx_{name}"));
    let src = parquet(&d, "fixture_loans", &fixture_frame());
    let issues = micro_parquet::engine::validate_steps(&src, &recipe.steps, &d).expect("validate");
    assert!(issues.is_empty(), "{name}: {issues:?}");
    let got = replay_to_csv(&recipe.steps, &d);
    let want = std::fs::read_to_string(fixtures_dir().join(format!("{name}.expected.csv"))).expect("expected CSV").replace("\r\n", "\n");
    assert_eq!(got, want, "{name}: today's code gives what the code that saved the macro gave");
    let _ = std::fs::remove_dir_all(&d);
}
