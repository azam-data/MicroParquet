//! Phase 3 L4: leading apostrophes, and Text columns that look like numbers (synthetic data).

use micro_parquet::engine::*;
use micro_parquet::macros::generate_python_polars_code;
use micro_parquet::models::*;
use polars::prelude::*;
use std::path::{Path, PathBuf};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mp_l4_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn typed_source(path: &Path, types: &[(&str, ColDataType)]) -> TabSource {
    let mut cfg = auto_detect_smart_schema(path).unwrap();
    for c in cfg.columns.iter_mut() {
        if let Some((_, t)) = types.iter().find(|(n, _)| *n == c.name) {
            c.dtype = *t;
            c.sign_convention = SignConvention::CrPositive;
        }
    }
    TabSource::SingleFile { path: path.to_path_buf(), is_parquet: false, config: cfg }
}

#[test]
fn l4_a_leading_apostrophe_is_dropped_by_number_parsing() {
    let cr = |s: &str| parse_banking_amount(s, SignConvention::CrPositive, true);
    assert_eq!(cr("'1,234.50"), AmountCell::Value(1234.5));
    assert_eq!(cr("  '1,234.50  "), AmountCell::Value(1234.5));
    assert_eq!(cr("'(500)"), AmountCell::Value(-500.0));
    assert_eq!(cr("'BDT 2,000"), AmountCell::Value(2000.0));
    assert_eq!(cr("'"), AmountCell::Blank, "a lone apostrophe is a blank cell");
    assert_eq!(cr("''5"), AmountCell::Invalid, "only one apostrophe is dropped");
    assert_eq!(cr("1,234.50"), AmountCell::Value(1234.5), "values without it are unchanged");
    assert!(is_blank_amount_token("'"));
    assert!(!is_parse_failure("'1,234.50", ColDataType::Float64, SignConvention::CrPositive, true));
    assert!(!is_parse_failure("'0012", ColDataType::Int64, SignConvention::CrPositive, true));
    assert!(amount_has_dr_cr("'100 Dr"));
    // dates already ignored the quote; keep it that way
    assert_eq!(parse_flexible_banking_date("'01/09/2026", ColDataType::DateDMY), parse_flexible_banking_date("01/09/2026", ColDataType::DateDMY));
    assert!(parse_flexible_banking_date("'01/09/2026", ColDataType::DateDMY).is_some());
}

#[test]
fn l4_float_column_reads_the_apostrophe_value_and_a_text_column_keeps_it() {
    let d = dir("load");
    let p = d.join("gl.csv");
    std::fs::write(&p, "Actnum,Balance,Remarks\n'0012,'1,234.50,'0012\n'0034,2000.00,x\n").unwrap();
    // "'1,234.50" has a comma, so quote it in the file
    std::fs::write(&p, "Actnum,Balance,Remarks\n'0012,\"'1,234.50\",'0012\n'0034,2000.00,x\n").unwrap();
    let src = typed_source(&p, &[("Balance", ColDataType::Float64)]);
    let lf = compile_tab_lazy(&src, &[], &d).unwrap();
    let df = lf.collect().unwrap();
    let bal: Vec<Option<f64>> = df.column("Balance").unwrap().f64().unwrap().into_iter().collect();
    assert_eq!(bal, vec![Some(1234.5), Some(2000.0)]);
    // Text columns are untouched: the apostrophe stays
    let acct: Vec<Option<&str>> = df.column("Actnum").unwrap().str().unwrap().into_iter().collect();
    assert_eq!(acct, vec![Some("'0012"), Some("'0034")]);
    let rem: Vec<Option<&str>> = df.column("Remarks").unwrap().str().unwrap().into_iter().collect();
    assert_eq!(rem, vec![Some("'0012"), Some("x")]);
    // and the apostrophe value is not a parse failure
    assert!(source_parse_failures(&src, &d).unwrap().is_empty());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn l4_python_export_mirrors_the_apostrophe_rule() {
    let mut cfg = IngestionConfig::default();
    cfg.columns.push(ColumnSchemaConfig { name: "Balance".into(), dtype: ColDataType::Float64, ..Default::default() });
    let py = generate_python_polars_code(
        Path::new("x.csv"),
        false,
        &cfg,
        &[TransformStep::ConvertToNumber { columns: vec!["Remarks".into()] }],
        None,
    )
    .unwrap();
    assert!(py.contains("if s.startswith(\"'\"):"), "{py}");
    assert!(py.contains("pl.col(\"Remarks\").cast(pl.Utf8).map_elements(lambda v: _mp_amount(v, \"Remarks\", \"Unset\", True), return_dtype=pl.Float64)"), "{py}");
}

fn text_frame(name: &str, values: Vec<Option<String>>) -> LazyFrame {
    DataFrame::new(vec![Series::new(name.into(), values).into()]).unwrap().lazy()
}

fn vals(numeric: usize, text: usize, blank: usize) -> Vec<Option<String>> {
    let mut v: Vec<Option<String>> = (0..numeric).map(|i| Some(format!("{},{}.50", i + 1, 100 + i))).collect();
    v.extend((0..text).map(|i| Some(format!("note {i}"))));
    v.extend((0..blank).map(|i| if i % 2 == 0 { None } else { Some("  ".to_string()) }));
    v
}

#[test]
fn l4_text_column_of_numbers_is_flagged_with_the_percent() {
    let lf = text_frame("Amount_Text", vals(49, 1, 5)); // 49 of 50 non-blank = 98%
    let found = text_columns_that_look_numeric(lf).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].column, "Amount_Text");
    assert_eq!((found[0].numeric, found[0].non_blank, found[0].percent()), (49, 50, 98));
    assert_eq!(
        found[0].message(),
        "Amount_Text is Text, but 98% of its values look like numbers. Set it to Number before exporting?"
    );
}

#[test]
fn l4_the_threshold_is_95_percent_of_non_blank_values() {
    assert_eq!(text_columns_that_look_numeric(text_frame("Amt_Text", vals(19, 1, 0))).unwrap().len(), 1, "19/20 = 95%");
    assert!(text_columns_that_look_numeric(text_frame("Amt_Text", vals(18, 2, 0))).unwrap().is_empty(), "18/20 = 90%");
    assert_eq!(text_columns_that_look_numeric(text_frame("Amt_Text", vals(18, 0, 40))).unwrap().len(), 1, "blanks do not count");
    assert!(text_columns_that_look_numeric(text_frame("Amt_Text", vals(0, 0, 10))).unwrap().is_empty(), "an empty column is not numbers");
    assert!(text_columns_that_look_numeric(text_frame("Narration", vals(2, 30, 0))).unwrap().is_empty());
    // an apostrophe-protected number counts
    let v: Vec<Option<String>> = (0..20).map(|i| Some(format!("'{}", i + 1))).collect();
    assert_eq!(text_columns_that_look_numeric(text_frame("Amt_Text", v)).unwrap().len(), 1);
}

#[test]
fn l4_id_columns_with_leading_zeros_are_not_flagged() {
    // They parse as numbers, but the Phase 1 tokeniser says these names are IDs.
    let ids: Vec<Option<String>> = (0..30).map(|i| Some(format!("{:05}", i))).collect();
    for name in ["Actnum", "Brancd", "CIF_Number", "Customer_ID", "GL_Code"] {
        assert!(TabState::is_likely_id_column(name), "{name}");
        assert!(text_columns_that_look_numeric(text_frame(name, ids.clone())).unwrap().is_empty(), "{name} must not be flagged");
    }
    // the same values in a column that is not an ID name are flagged
    assert_eq!(text_columns_that_look_numeric(text_frame("Reference_Value", ids)).unwrap().len(), 1);
}

#[test]
fn l4_set_to_number_converts_and_survives_a_macro_round_trip() {
    let d = dir("step");
    let p = d.join("t.csv");
    std::fs::write(&p, "Actnum,Amount_Text\nA1,\"1,000.50\"\nA2,'2000\nA3,\n").unwrap();
    let src = typed_source(&p, &[]); // everything Text
    let step = TransformStep::ConvertToNumber { columns: vec!["Amount_Text".into()] };
    let df = compile_tab_lazy(&src, &[step.clone()], &d).unwrap().collect().unwrap();
    let col = df.column("Amount_Text").unwrap();
    assert_eq!(col.dtype(), &DataType::Float64);
    let v: Vec<Option<f64>> = col.f64().unwrap().into_iter().collect();
    assert_eq!(v, vec![Some(1000.5), Some(2000.0), None]);
    // a column that does not exist is an error naming it
    let bad = TransformStep::ConvertToNumber { columns: vec!["Nope".into()] };
    let err = compile_tab_lazy(&src, &[bad], &d).and_then(|lf| Ok(lf.collect()?)).err().unwrap();
    assert!(format!("{err:#}").contains("'Nope'"), "{err:#}");
    // JSON round trip, and the label
    let json = serde_json::to_string(&step).unwrap();
    let back: TransformStep = serde_json::from_str(&json).unwrap();
    assert_eq!(back.short_label(), "Set to Number: Amount_Text");
    // after the step, the check finds nothing to flag
    assert!(text_columns_that_look_numeric(compile_tab_lazy(&src, &[step], &d).unwrap()).unwrap().is_empty());
    std::fs::remove_dir_all(&d).ok();
}
