//! Helpers shared by the Phase 8 tests (compiled into the phase3 binary, like `tests/phase7/`).

#![allow(dead_code)]

use polars::prelude::*;
use std::path::Path;

/// True when `python3` runs here; the Python mirror tests are skipped with a note when it does not.
pub fn have_python() -> bool {
    std::process::Command::new("python3").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// Runs a Python script (isolated mode, from a file in `d`) and returns its standard output.
pub fn run_python(d: &Path, name: &str, script: &str) -> String {
    let path = d.join(format!("{name}.py"));
    std::fs::write(&path, script).expect("write the script");
    let out = std::process::Command::new("python3").arg("-I").arg(&path).output().expect("python3 runs");
    assert!(out.status.success(), "{name}.py failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// A date: days since 1970-01-01.
pub fn ymd(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(y, m, d).expect("a test date")
}

/// The synthetic 1,000-row deposit tab of W1 and W2: an "Open Date" (Date) that walks a day at a time
/// from 25 Dec 2024 (so it crosses years, quarters, weeks and every weekday) with 3 blanks (rows 10, 500
/// and 999), the same dates as a Datetime with a time of day ("Open Time"), the same dates as text
/// ("Open Text", D/M/Y), a Branch, a Deposit Type and a Balance of 1,000 + row.
pub fn date_tab() -> DataFrame {
    let start = ymd(2024, 12, 25);
    let n = 1000usize;
    let blank = |i: usize| i == 10 || i == 500 || i == 999;
    let days: Vec<Option<i32>> = (0..n).map(|i| if blank(i) { None } else { Some(micro_parquet::dates::days_from_date(start + chrono::Duration::days((i as i64 * 3) % 800))) }).collect();
    let ms: Vec<Option<i64>> = days.iter().enumerate().map(|(i, d)| d.map(|d| d as i64 * 86_400_000 + ((i as i64 * 7919) % 86_400_000))).collect();
    let text: Vec<Option<String>> = days.iter().map(|d| d.and_then(micro_parquet::dates::date_from_days).map(|d| d.format("%d/%m/%Y").to_string())).collect();
    DataFrame::new(vec![
        Series::new("Open Date".into(), days).cast(&DataType::Date).expect("date").into(),
        Series::new("Open Time".into(), ms).cast(&DataType::Datetime(TimeUnit::Milliseconds, None)).expect("datetime").into(),
        Series::new("Open Text".into(), text).into(),
        Series::new("Branch".into(), (0..n).map(|i| ["Dhaka", "Ctg", "Sylhet"][i % 3]).collect::<Vec<_>>()).into(),
        Series::new("Deposit Type".into(), (0..n).map(|i| ["FDR", "SB", "CD", "SND"][i % 4]).collect::<Vec<_>>()).into(),
        Series::new("Balance".into(), (0..n).map(|i| 1000.0 + i as f64).collect::<Vec<_>>()).into(),
    ])
    .expect("the date tab")
}
