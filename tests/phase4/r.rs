//! PR #6 review items 1-20: one test group per item (named `i<N>_...`).

use crate::common::*;
use micro_parquet::models::*;
use micro_parquet::pivot_table::*;
use polars::prelude::*;

fn agg(target: &str, func: AggFunc, alias: &str, weight: Option<&str>) -> AggSpec {
    AggSpec {
        target_col: target.into(),
        func,
        weight_col: weight.map(str::to_string),
        alias: alias.into(),
        format_spec: ColumnFormatSpec { fmt_type: MetricFormatType::Auto, decimals: 2 },
    }
}

fn old_pivot(aggs: Vec<AggSpec>, sort: Option<&str>, top_n: Option<usize>) -> TransformStep {
    TransformStep::GroupByPivot { group_cols: vec!["Segment".into()], pivot_col: None, aggs, sort_by_col: sort.map(str::to_string), top_n }
}

fn seg_book() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Segment".into(), &["CMSME", "Retail", "CMSME", "Corporate", "Retail"]),
        Series::new("Balance".into(), &[100.0, 200.0, 300.0, 400.0, 50.0]),
        Series::new("Rate".into(), &[10.0, 8.0, 12.0, 9.0, 6.0]),
    ])
    .expect("book")
}

// ------------------------------------------------------------------------------------------ 1

#[test]
fn i1_convert_lists_the_changed_names_and_the_new_pivot_gives_the_same_numbers() {
    let old = old_pivot(
        vec![
            agg("Balance", AggFunc::Sum, "Total_Balance", None),
            agg("Rate", AggFunc::WeightedAvg, "Avg_Rate", Some("Balance")),
            agg("Balance", AggFunc::Mean, "Average of Balance", None),
        ],
        None,
        None,
    );
    let conv = convert_group_by_pivot(&old).expect("convertible");
    assert_eq!(
        conv.renames,
        vec![
            ("Total_Balance".to_string(), "Sum of Balance".to_string()),
            ("Avg_Rate".to_string(), "W.Avg of Rate by Balance".to_string()),
        ]
    );
    assert!(!conv.renames.iter().any(|(o, _)| o == "Average of Balance"), "an unchanged name is not listed");
    let before = run_steps("i1a", &seg_book(), &[old.clone()]).expect("old");
    let after = run_steps("i1b", &seg_book(), &[TransformStep::from_pivot_spec(conv.spec.clone())]).expect("new");
    assert_eq!(before.height(), after.height());
    // same numbers per segment, under the new names
    let seg_old = strings(&before, "Segment");
    let seg_new = strings(&after, "Segment");
    for (old_name, new_name) in [("Total_Balance", "Sum of Balance"), ("Avg_Rate", "W.Avg of Rate by Balance"), ("Average of Balance", "Average of Balance")] {
        let o = floats(&before, old_name);
        let n = floats(&after, new_name);
        for (i, s) in seg_old.iter().enumerate() {
            let j = seg_new.iter().position(|x| x == s).expect("segment");
            close(n[j], o[i].expect("old value"), 1e-9, &format!("{old_name} {s:?}"));
        }
    }
    // no totals row or column was added
    assert_eq!(after.get_column_names().len(), before.get_column_names().len());
    assert_eq!(after.height(), 3);
}

#[test]
fn i1_convert_reports_what_cannot_be_kept_and_refuses_what_it_cannot_express() {
    let with_top = old_pivot(vec![agg("Balance", AggFunc::Sum, "Bal", None)], Some("Bal"), Some(2));
    let conv = convert_group_by_pivot(&with_top).expect("top-n is listed, not silently dropped");
    assert!(conv.notes.iter().any(|n| n.contains("Top 2")), "{:?}", conv.notes);
    assert_eq!(conv.spec.sort_by_value, Some(PivotSortByValue { value_index: 0, descending: true }));
    // all '% of total' metrics become show-as; mixed ones are refused
    let pct = old_pivot(vec![agg("Balance", AggFunc::PctOfTotal, "Share", None)], None, None);
    let c = convert_group_by_pivot(&pct).expect("pct");
    assert_eq!(c.spec.show_as, PivotShowAs::PctOfColumnTotal);
    let mixed = old_pivot(vec![agg("Balance", AggFunc::PctOfTotal, "Share", None), agg("Balance", AggFunc::Sum, "Bal", None)], None, None);
    assert!(convert_group_by_pivot(&mixed).unwrap_err().to_string().contains("mixes"));
    // a weighted average without its weight, two metrics that are the same calculation, nothing to convert
    assert!(convert_group_by_pivot(&old_pivot(vec![agg("Rate", AggFunc::WeightedAvg, "R", None)], None, None)).is_err());
    let twice = old_pivot(vec![agg("Balance", AggFunc::Sum, "A", None), agg("Balance", AggFunc::Sum, "B", None)], None, None);
    assert!(convert_group_by_pivot(&twice).unwrap_err().to_string().contains("Sum of Balance"));
    assert!(convert_group_by_pivot(&old_pivot(vec![], None, None)).is_err());
    assert!(convert_group_by_pivot(&TransformStep::from_pivot_spec(PivotSpec::default())).is_err());
}

// ------------------------------------------------------------------------------------------ 2

fn pivot_for_export(scale: PivotScale) -> (DataFrame, std::collections::HashMap<String, ColumnFormatSpec>) {
    let df = DataFrame::new(vec![
        Series::new("Bank".into(), &["Conventional", "Islami", "Conventional"]),
        Series::new("Balance".into(), &[250_000_000.0, 100_000_000.0, 50_000_000.0]),
        Series::new("Rate".into(), &[8.0, 10.0, 6.0]),
    ])
    .expect("book");
    let spec = PivotSpec {
        rows: vec!["Bank".into()],
        values: vec![
            PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None },
            PivotValue { field: "Rate".into(), agg: PivotAgg::WeightedAvg, weight_col: Some("Balance".into()) },
            PivotValue { field: "Balance".into(), agg: PivotAgg::Count, weight_col: None },
        ],
        column_totals: true,
        scale,
        ..Default::default()
    };
    let out = build_pivot(df.lazy(), &spec).expect("pivot").df;
    let cols: Vec<String> = out.get_column_names().iter().map(|s| s.to_string()).collect();
    let formats = pivot_column_formats(&spec, &cols, 2).into_iter().collect();
    (out, formats)
}

fn write_pivot_xlsx(name: &str, scale: PivotScale, global: NumberDisplayFormat) -> (std::path::PathBuf, DataFrame) {
    use micro_parquet::exporters::*;
    let (df, formats) = pivot_for_export(scale);
    let d = dir(name);
    let p = d.join("pivot.xlsx");
    let tab = XlsxTab { title: "Pivot".into(), df: df.clone(), col_formats: formats, summary: Vec::new() };
    write_xlsx_tabs(vec![tab], vec![], &p, 1000, Some(XlsxScale { num_format: global, decimals: 2 })).expect("xlsx");
    (p, df)
}

#[test]
fn i2_pivot_scale_does_not_change_the_data_only_the_display_and_the_excel_columns_follow_the_phase3_rule() {
    // the pivot itself is in BDT whatever the Scale says
    let (plain, _) = pivot_for_export(PivotScale::Plain);
    let (crore, _) = pivot_for_export(PivotScale::Crore);
    assert_eq!(floats(&plain, "Sum of Balance"), floats(&crore, "Sum of Balance"));
    close(floats(&crore, "Sum of Balance")[0], 300_000_000.0, 1e-6, "Conventional in BDT");

    // Crore: the cell is divided, the unit is in the header; the weighted rate and the count are not scaled
    let (p, df) = write_pivot_xlsx("i2c", PivotScale::Crore, NumberDisplayFormat::Raw);
    let sheet = unzip_entry(&p, "xl/worksheets/sheet1.xml");
    let strings = unzip_entry(&p, "xl/sharedStrings.xml");
    let names: Vec<String> = df.get_column_names().iter().map(|s| s.to_string()).collect();
    assert_eq!(names, vec!["Bank", "Sum of Balance", "W.Avg of Rate by Balance", "Count of Balance"]);
    assert!(strings.contains("Sum of Balance (BDT Cr)"), "the unit is in the header: {strings}");
    assert!(!strings.contains("W.Avg of Rate by Balance (") && !strings.contains("Count of Balance ("), "{strings}");
    close(cell_value(&sheet, "B2").and_then(|v| v.parse().ok()), 30.0, 1e-9, "300,000,000 / 1e7 = 30.00 Cr");
    close(cell_value(&sheet, "B4").and_then(|v| v.parse().ok()), 40.0, 1e-9, "grand total 400,000,000 = 40.00 Cr");
    let rate: f64 = cell_value(&sheet, "C2").and_then(|v| v.parse().ok()).expect("rate");
    close(Some(rate), (8.0 * 250.0 + 6.0 * 50.0) / 300.0, 1e-9, "the weighted rate is not divided");

    // Million: the raw BDT value, the scale is the number format (',,') and the header says M
    let (p, _) = write_pivot_xlsx("i2m", PivotScale::Million, NumberDisplayFormat::Raw);
    let sheet = unzip_entry(&p, "xl/worksheets/sheet1.xml");
    let styles = unzip_entry(&p, "xl/styles.xml");
    close(cell_value(&sheet, "B2").and_then(|v| v.parse().ok()), 300_000_000.0, 1e-6, "raw value under a scaling format");
    assert!(cell_format_code(&sheet, &styles, "B2").ends_with(",,"), "{}", cell_format_code(&sheet, &styles, "B2"));
    assert!(unzip_entry(&p, "xl/sharedStrings.xml").contains("Sum of Balance (BDT M)"));

    // Thousand: a single ',' ; Plain: nothing, even when the workbook's global scale is Crore
    let (p, _) = write_pivot_xlsx("i2k", PivotScale::Thousand, NumberDisplayFormat::BdtCrore);
    let sheet = unzip_entry(&p, "xl/worksheets/sheet1.xml");
    let styles = unzip_entry(&p, "xl/styles.xml");
    assert!(cell_format_code(&sheet, &styles, "B2").ends_with(',') && !cell_format_code(&sheet, &styles, "B2").ends_with(",,"), "{}", cell_format_code(&sheet, &styles, "B2"));
    let (p, _) = write_pivot_xlsx("i2p", PivotScale::Plain, NumberDisplayFormat::BdtCrore);
    let sheet = unzip_entry(&p, "xl/worksheets/sheet1.xml");
    close(cell_value(&sheet, "B2").and_then(|v| v.parse().ok()), 300_000_000.0, 1e-6, "plain stays in BDT");
    let strings = unzip_entry(&p, "xl/sharedStrings.xml");
    assert!(strings.contains("Sum of Balance") && !strings.contains("Sum of Balance ("), "{strings}");
}

// ------------------------------------------------------------------------------------------ 3

fn mig_frame(keys: &[&str], bal: &[f64], cl: &[&str]) -> DataFrame {
    DataFrame::new(vec![Series::new("Actnum".into(), keys), Series::new("Balance".into(), bal), Series::new("CL_Status".into(), cl)]).expect("frame")
}

fn mig_at(df: &DataFrame, row: &str, col: &str) -> f64 {
    let labels = strings(df, micro_parquet::banking::MIGRATION_ROW_HEADING);
    let i = labels.iter().position(|l| l.as_deref() == Some(row)).unwrap_or_else(|| panic!("no row {row}: {labels:?}"));
    floats(df, col)[i].unwrap_or(f64::NAN)
}

#[test]
fn i3_balance_basis_previous_is_the_default_and_current_changes_only_accounts_in_both_months() {
    use micro_parquet::banking::*;
    // previous A1 STD 100, A2 SS 50, A3 STD 30; current A1 SMA 120, A2 DF 60, A4 STD 20
    let prev = mig_frame(&["A1", "A2", "A3"], &[100.0, 50.0, 30.0], &["STD", "SS", "STD"]);
    let curr = mig_frame(&["A1", "A2", "A4"], &[120.0, 60.0, 20.0], &["SMA", "DF", "STD"]);
    let map = ProvisionConfig::defaults().cl_code_map;
    let spec = |basis| ClMigrationSpec {
        key_col: "Actnum".into(),
        bal_col: "Balance".into(),
        cl_col: "CL_Status".into(),
        segment_col: None,
        measure: MigrationMeasure::Balance,
        pct_of_row: false,
        balance_basis: basis,
    };
    assert_eq!(MigrationBasis::default(), MigrationBasis::Previous);
    let go = |basis| build_cl_migration_with(prev.clone().lazy(), curr.clone().lazy(), &spec(basis), &map).expect("builds");

    let p = go(MigrationBasis::Previous);
    assert_eq!(mig_at(&p, "STD", "SMA"), 100.0, "last month's balance");
    assert_eq!(mig_at(&p, "SS", "DF"), 50.0);
    assert_eq!(mig_at(&p, "STD", "Closed/Repaid"), 30.0);
    assert_eq!(mig_at(&p, "New", "STD"), 20.0);
    assert_eq!(mig_at(&p, "Previous month total", "Total"), 180.0, "adds up to last month's total");

    let c = go(MigrationBasis::Current);
    assert_eq!(mig_at(&c, "STD", "SMA"), 120.0, "this month's balance of an account in both months");
    assert_eq!(mig_at(&c, "SS", "DF"), 60.0);
    assert_eq!(mig_at(&c, "STD", "Closed/Repaid"), 30.0, "a closed account always uses last month's");
    assert_eq!(mig_at(&c, "New", "STD"), 20.0, "a new account always uses this month's");
    assert_eq!(mig_at(&c, MIGRATION_PREV_TOTAL_ROW_CURRENT, "Total"), 210.0, "120 + 60 + 30");
    assert_eq!(mig_at(&c, "Total", "Total"), 230.0);
    // the counts do not depend on the basis
    let counts = |b| build_cl_migration_with(prev.clone().lazy(), curr.clone().lazy(), &ClMigrationSpec { measure: MigrationMeasure::Count, ..spec(b) }, &map).expect("counts");
    assert_eq!(floats(&counts(MigrationBasis::Previous), "Total"), floats(&counts(MigrationBasis::Current), "Total"));
}
