//! K4: copying the table to the clipboard: plain numbers in the shown unit with the unit in the
//! header (Ctrl+C / "Copy"), the text as shown ("Copy as shown") and the stored numbers ("Copy raw (BDT)").

use micro_parquet::exporters::{copy_table_text, CopyKind};
use micro_parquet::models::*;
use polars::prelude::*;
use std::collections::HashMap;

fn frame() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Branch".into(), &["Dhaka", "Ctg\tMain", "Sylhet"]).into(),
        Series::new("Sum of Balance".into(), &[Some(449_406_400_000.0f64), Some(-12_345_678.9), None]).into(),
        Series::new("Sum of Deposit".into(), &[Some(1_234_567.891f64), Some(-2_500_000.0), Some(0.5)]).into(),
        Series::new("Rate".into(), &[Some(8.5f64), None, Some(9.25)]).into(),
        Series::new("Accounts".into(), &[1_234i64, -5, 20_000]).into(),
    ])
    .expect("frame")
}

fn formats() -> HashMap<String, ColumnFormatSpec> {
    let mut m = HashMap::new();
    m.insert("Sum of Balance".to_string(), ColumnFormatSpec { fmt_type: MetricFormatType::BdtCrore, decimals: 2 });
    m.insert("Sum of Deposit".to_string(), ColumnFormatSpec { fmt_type: MetricFormatType::BdtMillion, decimals: 2 });
    m
}

fn rows(txt: &str) -> Vec<Vec<String>> {
    txt.lines().map(|l| l.split('\t').map(str::to_string).collect()).collect()
}

#[test]
fn k4_copy_writes_plain_numbers_in_the_shown_unit_with_the_unit_in_the_header() {
    let t = rows(&copy_table_text(&frame(), &formats(), NumberDisplayFormat::BdtCrore, 2, CopyKind::Plain));
    assert_eq!(t[0], vec!["Branch", "Sum of Balance (BDT Cr)", "Sum of Deposit (BDT M)", "Rate (%)", "Accounts"]);
    // Crore: no separators, no suffix, trailing zeros trimmed; negative keeps its sign; a blank is an empty cell
    assert_eq!(t[1], vec!["Dhaka", "44940.64", "1.234567891", "8.5", "1234"]);
    assert_eq!(t[2], vec!["Ctg Main", "-1.23456789", "-2.5", "", "-5"], "a tab inside a text is a space");
    assert_eq!(t[3], vec!["Sylhet", "", "0.0000005", "9.25", "20000"]);
}

#[test]
fn k4_copy_as_shown_is_the_exact_display_text() {
    let t = rows(&copy_table_text(&frame(), &formats(), NumberDisplayFormat::BdtCrore, 2, CopyKind::AsShown));
    assert_eq!(t[0], vec!["Branch", "Sum of Balance", "Sum of Deposit", "Rate", "Accounts"]);
    assert_eq!(t[1], vec!["Dhaka", "44,940.64 Cr", "1.23 M", "8.50%", "1,234"]);
    assert_eq!(t[2], vec!["Ctg Main", "-1.23 Cr", "-2.50 M", "", "-5"]);
    assert_eq!(t[3], vec!["Sylhet", "", "0.00 M", "9.25%", "20,000"]);
}

#[test]
fn k4_copy_raw_is_the_unscaled_bdt_number() {
    let t = rows(&copy_table_text(&frame(), &formats(), NumberDisplayFormat::BdtCrore, 2, CopyKind::Raw));
    assert_eq!(t[0], vec!["Branch", "Sum of Balance (BDT)", "Sum of Deposit (BDT)", "Rate", "Accounts"]);
    assert_eq!(t[1], vec!["Dhaka", "449406400000", "1234567.891", "8.5", "1234"]);
    assert_eq!(t[2], vec!["Ctg Main", "-12345678.9", "-2500000", "", "-5"]);
    assert_eq!(t[3], vec!["Sylhet", "", "0.5", "9.25", "20000"]);
}

#[test]
fn k4_an_auto_column_follows_the_global_scale_and_a_float_sum_carries_no_noise() {
    // Auto columns (no own format) use the View scale: Million here
    let df = DataFrame::new(vec![Series::new("Balance".into(), &[3_478_848_729_999.9995f64, 0.1 + 0.2]).into()]).expect("frame");
    let t = rows(&copy_table_text(&df, &HashMap::new(), NumberDisplayFormat::BdtMillion, 2, CopyKind::Plain));
    assert_eq!(t[0], vec!["Balance (BDT M)"]);
    assert_eq!(t[1], vec!["3478848.73"]);
    assert_eq!(t[2], vec!["0.0000003"]);
    // plain numbers (View: commas): no unit in the header
    let t = rows(&copy_table_text(&df, &HashMap::new(), NumberDisplayFormat::StandardCommas, 2, CopyKind::Plain));
    assert_eq!(t[0], vec!["Balance"]);
    assert_eq!(t[2], vec!["0.3"]);
}
