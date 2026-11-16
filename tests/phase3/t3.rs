//! Phase 3 T3: the summary row (synthetic frame with nulls).

use micro_parquet::summary::*;
use micro_parquet::models::*;
use polars::prelude::*;
use std::collections::HashMap;

fn day(s: &str) -> i32 {
    (chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap() - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days() as i32
}

/// Six rows. Balance has a null, Remarks has a null and a blank, Opndat has a null, Weight has
/// one sign, MixedW has weights of both signs.
fn frame() -> DataFrame {
    let opn = Series::new(
        "Opndat".into(),
        &[Some(day("2026-03-01")), Some(day("2026-01-15")), None, Some(day("2026-02-01")), Some(day("2026-01-15")), Some(day("2026-04-01"))],
    )
    .cast(&DataType::Date)
    .unwrap();
    DataFrame::new(vec![
        Series::new("Actnum".into(), &["A1", "A2", "A3", "A4", "A5", "A6"]).into(),
        Series::new("Brancd".into(), &["DHK", "DHK", "CTG", "CTG", "DHK", "SYL"]).into(),
        Series::new("Balance".into(), &[Some(1000.0f64), Some(2000.0), None, Some(3000.0), Some(4000.0), Some(0.0)]).into(),
        Series::new("Int Rate".into(), &[Some(10.0f64), Some(8.0), Some(9.0), None, Some(12.0), Some(6.0)]).into(),
        Series::new("Weight".into(), &[1000.0f64, 2000.0, 500.0, 3000.0, 4000.0, 100.0]).into(),
        Series::new("MixedW".into(), &[1000.0f64, -999.0, 500.0, 3000.0, 4000.0, 100.0]).into(),
        Series::new("Remarks".into(), &[Some("ok"), Some("late"), None, Some("  "), Some("ok"), Some("")]).into(),
        Series::new("Count".into(), &[1i64, 2, 3, 4, 5, 6]).into(),
        opn.into(),
    ])
    .unwrap()
}

fn spec(agg: SummaryAgg) -> SummarySpec {
    SummarySpec { agg, weight_col: None }
}

fn one(column: &str, spec: SummarySpec) -> SummaryCell {
    compute_summary(frame().lazy(), &[(column.to_string(), spec)]).unwrap().remove(0)
}

#[test]
fn t3_every_aggregation_on_a_frame_with_nulls() {
    use SummaryAgg::*;
    // numbers: nulls are skipped
    assert_eq!(one("Balance", spec(Sum)).value, Some(SummaryValue::Number(10_000.0)));
    assert_eq!(one("Balance", spec(Average)).value, Some(SummaryValue::Number(2_000.0)), "10,000 over the 5 known values");
    assert_eq!(one("Balance", spec(Min)).value, Some(SummaryValue::Number(0.0)));
    assert_eq!(one("Balance", spec(Max)).value, Some(SummaryValue::Number(4_000.0)));
    assert_eq!(one("Balance", spec(Count)).value, Some(SummaryValue::Count(5)), "the null is not counted");
    assert_eq!(one("Balance", spec(CountDistinct)).value, Some(SummaryValue::Count(5)));
    assert_eq!(one("Int Rate", spec(Count)).value, Some(SummaryValue::Count(5)));
    // integers
    assert_eq!(one("Count", spec(Sum)).value, Some(SummaryValue::Number(21.0)));
    // text: blanks (null, spaces, empty) are not values
    assert_eq!(one("Remarks", spec(Count)).value, Some(SummaryValue::Count(3)));
    assert_eq!(one("Remarks", spec(CountDistinct)).value, Some(SummaryValue::Count(2)), "ok and late");
    assert_eq!(one("Remarks", spec(Min)).value, Some(SummaryValue::Text("late".into())));
    assert_eq!(one("Remarks", spec(Max)).value, Some(SummaryValue::Text("ok".into())));
    assert_eq!(one("Brancd", spec(CountDistinct)).value, Some(SummaryValue::Count(3)));
    assert_eq!(one("Brancd", spec(Count)).value, Some(SummaryValue::Count(6)));
    // dates: the null is ignored
    assert_eq!(one("Opndat", spec(Min)).value, Some(SummaryValue::Date(day("2026-01-15"))));
    assert_eq!(one("Opndat", spec(Max)).value, Some(SummaryValue::Date(day("2026-04-01"))));
    assert_eq!(one("Opndat", spec(Count)).value, Some(SummaryValue::Count(5)));
    assert_eq!(one("Opndat", spec(CountDistinct)).value, Some(SummaryValue::Count(4)));
    // None computes nothing
    let none = one("Balance", spec(None));
    assert!(none.value.is_none() && none.note.is_none());
}

#[test]
fn t3_weighted_average_uses_the_phase2_rule() {
    // rows with both a rate and a weight: (10*1000 + 8*2000 + 12*4000 + 6*100) / 7100 ... Int Rate has a null on row 4
    let w = SummarySpec { agg: SummaryAgg::WeightedAvg, weight_col: Some("Weight".into()) };
    let c = one("Int Rate", w);
    let expect = (10.0 * 1000.0 + 8.0 * 2000.0 + 9.0 * 500.0 + 12.0 * 4000.0 + 6.0 * 100.0) / (1000.0 + 2000.0 + 500.0 + 4000.0 + 100.0);
    match c.value {
        Some(SummaryValue::Number(x)) => assert!((x - expect).abs() < 1e-9, "{x} vs {expect}"),
        other => panic!("{other:?} {:?}", c.note),
    }
    // weights of both signs: no value, and the reason
    let mixed = SummarySpec { agg: SummaryAgg::WeightedAvg, weight_col: Some("MixedW".into()) };
    let c = one("Int Rate", mixed);
    assert_eq!(c.value, None);
    assert!(c.note.unwrap().contains("mix of positive and negative"), "tooltip says why");
    // no weight column chosen
    let c = one("Int Rate", SummarySpec { agg: SummaryAgg::WeightedAvg, weight_col: None });
    assert_eq!(c.value, None);
    assert!(c.note.unwrap().contains("weight column"));
    // an unknown weight column
    let c = one("Int Rate", SummarySpec { agg: SummaryAgg::WeightedAvg, weight_col: Some("Nope".into()) });
    assert!(c.note.unwrap().contains("'Nope'"));
}

#[test]
fn t3_a_choice_that_does_not_fit_the_column_has_no_value_and_says_why() {
    let c = one("Remarks", spec(SummaryAgg::Sum));
    assert_eq!(c.value, None);
    assert!(c.note.unwrap().contains("needs numbers"));
    let c = one("Opndat", spec(SummaryAgg::Average));
    assert_eq!(c.value, None);
    assert!(c.note.unwrap().contains("Opndat"));
    // a column that is gone
    let c = one("Gone", spec(SummaryAgg::Sum));
    assert!(c.note.unwrap().contains("not in the result"));
}

#[test]
fn t3_defaults_numbers_sum_text_counts_dates_show_the_earliest() {
    let df = frame();
    let cols: Vec<(String, DataType)> = df.get_columns().iter().map(|s| (s.name().to_string(), s.dtype().clone())).collect();
    let specs = effective_specs(&cols, &HashMap::new());
    let agg = |n: &str| specs.iter().find(|(c, _)| c == n).unwrap().1.agg;
    assert_eq!(agg("Balance"), SummaryAgg::Sum);
    assert_eq!(agg("Weight"), SummaryAgg::Sum);
    assert_eq!(agg("Brancd"), SummaryAgg::Count);
    assert_eq!(agg("Remarks"), SummaryAgg::Count);
    assert_eq!(agg("Opndat"), SummaryAgg::Min);
    // an ID-like numeric column is counted, not summed
    assert_eq!(default_agg_for(&DataType::Int64, "Actnum"), SummaryAgg::Count);
    // the user's choice wins, and only for that column
    let mut over = HashMap::new();
    over.insert("Balance".to_string(), SummarySpec { agg: SummaryAgg::Average, weight_col: None });
    let specs = effective_specs(&cols, &over);
    assert_eq!(specs.iter().find(|(c, _)| c == "Balance").unwrap().1.agg, SummaryAgg::Average);
    assert_eq!(specs.iter().find(|(c, _)| c == "Weight").unwrap().1.agg, SummaryAgg::Sum);
    // all defaults computed together in one go
    let cells = compute_summary(df.lazy(), &effective_specs(&cols, &HashMap::new())).unwrap();
    assert_eq!(cells.len(), cols.len());
    let bal = cells.iter().find(|c| c.column == "Balance").unwrap();
    assert_eq!(bal.value, Some(SummaryValue::Number(10_000.0)));
}

#[test]
fn t3_values_are_shown_with_separators_scale_and_dates() {
    let mut cells = compute_summary(
        frame().lazy(),
        &[
            ("Balance".into(), spec(SummaryAgg::Sum)),
            ("Brancd".into(), spec(SummaryAgg::Count)),
            ("Opndat".into(), spec(SummaryAgg::Min)),
            ("Remarks".into(), spec(SummaryAgg::Max)),
            ("Int Rate".into(), SummarySpec { agg: SummaryAgg::WeightedAvg, weight_col: Some("MixedW".into()) }),
        ],
    )
    .unwrap();
    let f = |c: &SummaryCell, n: NumberDisplayFormat| format_summary_value(c, None, n, 2);
    assert_eq!(f(&cells[0], NumberDisplayFormat::StandardCommas), "10,000.00");
    assert_eq!(f(&cells[0], NumberDisplayFormat::BdtLakh), "0.10 L");
    assert_eq!(f(&cells[1], NumberDisplayFormat::BdtCrore), "6", "counts are never scaled");
    assert_eq!(f(&cells[2], NumberDisplayFormat::BdtCrore), "2026-01-15");
    assert_eq!(f(&cells[3], NumberDisplayFormat::BdtCrore), "ok");
    assert_eq!(f(&cells[4], NumberDisplayFormat::BdtCrore), "\u{2014}", "no value shows a dash");
    assert!(is_scalable(&cells[0]) && !is_scalable(&cells[1]));
    cells[1].value = Some(SummaryValue::Count(20_391));
    assert_eq!(f(&cells[1], NumberDisplayFormat::Raw), "20,391");
}

#[test]
fn t3_summary_choices_are_saved_in_macros_and_old_macros_load_without_them() {
    let mut over = HashMap::new();
    over.insert("Int Rate".to_string(), SummarySpec { agg: SummaryAgg::WeightedAvg, weight_col: Some("Balance".into()) });
    let recipe = MacroRecipe {
        name: "s".into(),
        description: String::new(),
        created_at: String::new(),
        enforce_balance_control: false,
        control_column: None,
        optional_ingestion_schema: None,
        steps: vec![],
        summary: over.clone(),
    };
    let dir = std::env::temp_dir().join(format!("mp_t3_macro_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.mpq.json");
    micro_parquet::macros::save_macro_to_file(&recipe, &p).unwrap();
    assert_eq!(micro_parquet::macros::load_macro_from_file(&p).unwrap().summary, over);
    // a macro from before this task has no summary key
    let old = r#"{"name":"o","description":"","created_at":"","enforce_balance_control":false,"control_column":null,"optional_ingestion_schema":null,"steps":[]}"#;
    std::fs::write(&p, old).unwrap();
    assert!(micro_parquet::macros::load_macro_from_file(&p).unwrap().summary.is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------- exports

fn unzip_entry(zip: &std::path::Path, entry: &str) -> String {
    let out = std::process::Command::new("unzip").arg("-p").arg(zip).arg(entry).output().expect("unzip must be installed");
    assert!(out.status.success(), "unzip {entry}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn cell_value(sheet: &str, cell: &str) -> Option<String> {
    let key = format!("r=\"{}\"", cell);
    let i = sheet.find(&key)?;
    let rest = &sheet[i..];
    let end = rest.find("</c>")?;
    let c = &rest[..end];
    let v0 = c.find("<v>")? + 3;
    let v1 = c[v0..].find("</v>")? + v0;
    Some(c[v0..v1].to_string())
}

fn summary_for(df: &DataFrame, over: &HashMap<String, SummarySpec>) -> Vec<SummaryCell> {
    let cols: Vec<(String, DataType)> = df.get_columns().iter().map(|s| (s.name().to_string(), s.dtype().clone())).collect();
    compute_summary(df.clone().lazy(), &effective_specs(&cols, over)).unwrap()
}

#[test]
fn t3_excel_has_a_totals_row_with_labels_in_the_summary_colour() {
    use micro_parquet::exporters::*;
    let dir = std::env::temp_dir().join(format!("mp_t3_xlsx_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let df = frame();
    let mut over = HashMap::new();
    over.insert("Int Rate".to_string(), SummarySpec { agg: SummaryAgg::WeightedAvg, weight_col: Some("Weight".into()) });
    over.insert("Remarks".to_string(), SummarySpec { agg: SummaryAgg::CountDistinct, weight_col: None });
    let cells = summary_for(&df, &over);
    let tab = XlsxTab { title: "Loans".into(), df: df.clone(), col_formats: HashMap::new(), summary: cells };
    let p = dir.join("totals.xlsx");
    write_xlsx_tabs(vec![tab], vec![], &p, 1000, None).unwrap();
    let sheet = unzip_entry(&p, "xl/worksheets/sheet1.xml");
    let strings = unzip_entry(&p, "xl/sharedStrings.xml");
    let styles = unzip_entry(&p, "xl/styles.xml");
    // 6 data rows: header row 1, data rows 2-7, a blank row 8, labels row 9, values row 10.
    // Columns: A Actnum, B Brancd, C Balance, D Int Rate, E Weight, F MixedW, G Remarks, H Count, I Opndat
    assert_eq!(cell_value(&sheet, "C10").as_deref(), Some("10000"), "Balance sum");
    assert_eq!(cell_value(&sheet, "E10").as_deref(), Some("10600"), "Weight sum");
    assert_eq!(cell_value(&sheet, "H10").as_deref(), Some("21"));
    assert_eq!(cell_value(&sheet, "B10").as_deref().map(|v| v.parse::<f64>().is_ok()), Some(true), "branch count is a number");
    let wavg = cell_value(&sheet, "D10").unwrap().parse::<f64>().unwrap();
    let expect = (10.0 * 1000.0 + 8.0 * 2000.0 + 9.0 * 500.0 + 12.0 * 4000.0 + 6.0 * 100.0) / 7600.0;
    assert!((wavg - expect).abs() < 1e-9, "{wavg} vs {expect}");
    // Opndat minimum is a date serial (2026-01-15 = 46037 in Excel)
    assert_eq!(cell_value(&sheet, "I10").as_deref(), Some("46037"));
    // labels name each aggregation
    for label in ["Sum", "Count", "Min", "W.Avg by Weight", "Count distinct"] {
        assert!(strings.contains(&format!(">{}<", label)), "label {label} in {strings}");
    }
    // in the summary colour
    assert!(styles.to_uppercase().contains("1F3A5F"), "{styles}");
    // the data rows above are untouched
    assert_eq!(cell_value(&sheet, "C2").as_deref(), Some("1000"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn t3_excel_totals_use_the_chosen_scale_and_sit_on_the_last_sheet() {
    use micro_parquet::exporters::*;
    let dir = std::env::temp_dir().join(format!("mp_t3_xlsx2_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let df = frame();
    let cells = summary_for(&df, &HashMap::new());
    let tab = XlsxTab { title: "Loans".into(), df, col_formats: HashMap::new(), summary: cells };
    let p = dir.join("scaled.xlsx");
    // 2 rows per sheet: three sheets; Million scale
    write_xlsx_tabs(vec![tab], vec![], &p, 2, Some(XlsxScale { num_format: NumberDisplayFormat::BdtMillion, decimals: 2 })).unwrap();
    let s1 = unzip_entry(&p, "xl/worksheets/sheet1.xml");
    let s3 = unzip_entry(&p, "xl/worksheets/sheet3.xml");
    assert!(cell_value(&s1, "C5").is_none() && !s1.contains("1F3A5F"), "no totals on the first sheet");
    // last sheet holds rows 5-6 (+ header): header 1, data 2-3, blank 4, labels 5, values 6
    assert_eq!(cell_value(&s3, "C6").as_deref(), Some("0.01"), "10,000 in millions");
    assert_eq!(cell_value(&s3, "H6").as_deref(), Some("21"), "a count column is not scaled");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn t3_pdf_report_adds_the_totals_band() {
    use micro_parquet::exporters::*;
    let dir = std::env::temp_dir().join(format!("mp_t3_pdf_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let df = frame();
    let cells = summary_for(&df, &HashMap::new());
    let plain = dir.join("plain.pdf");
    let with = dir.join("with.pdf");
    export_pdf_tabs(vec![PdfTab { title: "Loans".into(), df: df.clone(), col_formats: HashMap::new(), summary: Vec::new() }], &plain, NumberDisplayFormat::BdtCrore, 2).unwrap();
    export_pdf_tabs(vec![PdfTab { title: "Loans".into(), df, col_formats: HashMap::new(), summary: cells }], &with, NumberDisplayFormat::BdtCrore, 2).unwrap();
    let (a, b) = (std::fs::metadata(&plain).unwrap().len(), std::fs::metadata(&with).unwrap().len());
    assert!(b > a, "the band adds drawing and text ({b} vs {a})");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn t3_the_export_job_computes_the_summary_on_the_whole_result() {
    use micro_parquet::jobs::JobCtx;
    use micro_parquet::tab_jobs::*;
    let dir = std::env::temp_dir().join(format!("mp_t3_job_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("loans.parquet");
    let mut df = frame();
    ParquetWriter::new(std::fs::File::create(&p).unwrap()).finish(&mut df).unwrap();
    let src = TabSource::SingleFile { path: p, is_parquet: true, config: IngestionConfig::default() };
    // summary job
    let r = compute_tab_summary(
        SummaryRequest { tab_id: 3, source: src.clone(), steps: vec![], overrides: HashMap::new(), temp_dir: dir.clone() },
        &JobCtx::inert(),
    )
    .unwrap();
    assert_eq!((r.tab_id, r.steps_len), (3, 0));
    assert_eq!(r.cells.iter().find(|c| c.column == "Balance").unwrap().value, Some(SummaryValue::Number(10_000.0)));
    // after a filter the summary follows the result, not the source
    let step = TransformStep::FilterRows { column: "Brancd".into(), operator: FilterOp::Eq, value: "DHK".into(), keep_matching: true };
    let r = compute_tab_summary(
        SummaryRequest { tab_id: 3, source: src.clone(), steps: vec![step], overrides: HashMap::new(), temp_dir: dir.clone() },
        &JobCtx::inert(),
    )
    .unwrap();
    assert_eq!(r.steps_len, 1);
    assert_eq!(r.cells.iter().find(|c| c.column == "Balance").unwrap().value, Some(SummaryValue::Number(7_000.0)));
    // the Excel export job writes the totals row
    let out = dir.join("job.xlsx");
    let tab = ExportTab { title: "Loans".into(), source: src, steps: vec![], col_formats: HashMap::new(), summary: HashMap::new() };
    export_tabs_xlsx(&[tab], &dir, &out, NumberDisplayFormat::StandardCommas, 2, None, &JobCtx::inert()).unwrap();
    let sheet = unzip_entry(&out, "xl/worksheets/sheet1.xml");
    assert_eq!(cell_value(&sheet, "C10").as_deref(), Some("10000"));
    std::fs::remove_dir_all(&dir).ok();
}
