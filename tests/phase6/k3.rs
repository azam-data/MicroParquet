//! K3: CSV export writes Float64 values with at most 15 significant digits, trailing zeros trimmed, so
//! Excel reads every value as a number. Integers and text are unchanged; Parquet is unchanged.

use crate::common::*;
use micro_parquet::engine::*;
use polars::prelude::*;

fn frame() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Id".into(), &[1i64, 22, 333, 4, 5, 6, 7, 8]).into(),
        Series::new("Name".into(), &["a", "b,c", "d", "e", "f", "g", "h", "i"]).into(),
        Series::new(
            "Balance".into(),
            &[Some(3478848.7299999995f64), Some(224703214160.54498), Some(0.1 + 0.2), Some(12.5), None, Some(-1234.5600000000002), Some(100.0), Some(0.0)],
        )
        .into(),
    ])
    .expect("frame")
}

fn write(name: &str, mode: ExportMode) -> (Vec<Vec<String>>, ExportPath) {
    let d = dir(name);
    let src = parquet(&d, "in", &frame());
    let lf = compile_tab_lazy(&src, &[], &d).expect("lf");
    let out = d.join("out.csv");
    let how = export_to_csv(lf, &out, false, mode).expect("export");
    let text = std::fs::read_to_string(&out).expect("csv");
    let rows = text.lines().map(|l| l.split(',').map(str::to_string).collect()).collect();
    (rows, how)
}

#[test]
fn k3_csv_floats_have_at_most_15_significant_digits_with_trailing_zeros_trimmed() {
    for mode in [ExportMode::Auto, ExportMode::CollectOnly] {
        let (rows, _) = write("k3a", mode);
        let bal: Vec<&str> = rows.iter().skip(1).map(|r| r.last().map(String::as_str).unwrap_or("?")).collect();
        assert_eq!(bal, vec!["3478848.73", "224703214160.545", "0.3", "12.5", "", "-1234.56", "100", "0"], "{mode:?}");
        assert_eq!(rows[0], vec!["Id", "Name", "Balance"]);
    }
}

#[test]
fn k3_integers_and_text_are_unchanged_and_a_comma_in_text_is_still_quoted() {
    let d = dir("k3b");
    let src = parquet(&d, "in", &frame());
    let lf = compile_tab_lazy(&src, &[], &d).expect("lf");
    let out = d.join("out.csv");
    export_to_csv(lf, &out, false, ExportMode::Auto).expect("export");
    let text = std::fs::read_to_string(&out).expect("csv");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[1], "1,a,3478848.73");
    assert_eq!(lines[2], "22,\"b,c\",224703214160.545");
    assert_eq!(lines[3], "333,d,0.3");
}

#[test]
fn k3_the_helper_rounds_to_15_digits_and_never_uses_scientific_notation() {
    assert_eq!(csv_float_text(3478848.7299999995), "3478848.73");
    assert_eq!(csv_float_text(224703214160.54498), "224703214160.545");
    assert_eq!(csv_float_text(1e21), "1000000000000000000000");
    assert_eq!(csv_float_text(0.0000001), "0.0000001");
    assert_eq!(csv_float_text(-0.0), "0");
    assert_eq!(csv_float_text(f64::NAN), "NaN");
    assert_eq!(csv_float_text(f64::INFINITY), "inf");
    assert_eq!(csv_float_text(123456789012345.0), "123456789012345");
}

#[test]
fn k3_parquet_keeps_the_exact_values() {
    let d = dir("k3c");
    let src = parquet(&d, "in", &frame());
    let lf = compile_tab_lazy(&src, &[], &d).expect("lf");
    let out = d.join("out.parquet");
    export_to_parquet(lf, &out, micro_parquet::models::ParquetDialect::ArchiveZstd, ExportMode::Auto).expect("export");
    let back = LazyFrame::scan_parquet(&out, Default::default()).expect("scan").collect().expect("collect");
    assert_eq!(floats(&back, "Balance")[0], Some(3478848.7299999995));
}
