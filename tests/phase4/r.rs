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

// ------------------------------------------------------------------------------------------ 4

#[test]
fn i4_the_gap_ladder_ends_with_a_totals_row_and_its_percentage_cells_are_blank() {
    use micro_parquet::banking::*;
    let def = CustomBucketDef {
        name: "Test ladder".into(),
        rules: vec![
            BucketRuleItem { max_days: 31, label: "0\u{2013}1 month".into() },
            BucketRuleItem { max_days: 92, label: "1\u{2013}3 months".into() },
        ],
        overflow_label: "Over 3 months".into(),
        overdue_label: "Overdue".into(),
        null_sight_label: "0\u{2013}1 month".into(),
        null_long_label: "Over 3 months".into(),
    };
    let spec = |role| GapInputSpec { role, amount_col: "Balance".into(), maturity_col: "Matdat".into() };
    let assets = DataFrame::new(vec![Series::new("Balance".into(), &[100.0, 200.0]), dates("Matdat", &["2026-11-15", "2026-12-31"])]).expect("assets");
    let liab = DataFrame::new(vec![Series::new("Balance".into(), &[150.0, 50.0]), dates("Matdat", &["2026-11-20", "2026-12-15"])]).expect("liab");
    let limits = GapLimitsFile { limits: vec![GapLimit { bucket: "0\u{2013}1 month".into(), min_cumulative_gap_pct: -10.0 }], ..Default::default() };
    let out = build_gap_ladder(vec![(assets.lazy(), spec(GapRole::Asset)), (liab.lazy(), spec(GapRole::Liability))], &def, "2026-10-31", true, Some(&limits)).expect("ladder");
    let last = out.height() - 1;
    assert_eq!(strings(&out, "Bucket")[last].as_deref(), Some("Total"));
    assert_eq!(floats(&out, "Inflows")[last], Some(300.0));
    assert_eq!(floats(&out, "Outflows")[last], Some(200.0));
    assert_eq!(floats(&out, "Net gap")[last], Some(100.0));
    for c in ["Cumulative gap", "Net gap % of total assets", "Cumulative gap % of total assets", "Cumulative gap limit %"] {
        assert_eq!(floats(&out, c)[last], None, "{c} is blank on the totals row");
    }
    assert_eq!(strings(&out, "Breach")[last].as_deref(), Some(""), "the totals row is never a breach");
    // the rows above it add up to it
    let sum: f64 = floats(&out, "Net gap")[..last].iter().flatten().sum();
    assert_eq!(sum, 100.0);
}

// ------------------------------------------------------------------------------------------ 6

fn savings_book() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Product".into(), &["SAVINGS", "FDR", "OTHER"]),
        Series::new("Side".into(), &["Liability", "Liability", "Liability"]),
        Series::new("Balance".into(), &[1_000_000.0, 1_000_000.0, 1_000_000.0]),
        Series::new("Rate".into(), &[1.5, 6.0, 3.0]),
        dates("Matdat", &["", "", ""]),
    ])
    .expect("book")
}

fn shock_spec(bps: f64) -> micro_parquet::banking::RateShockSpec {
    micro_parquet::banking::RateShockSpec {
        bal_col: "Balance".into(),
        rate_col: "Rate".into(),
        mat_col: "Matdat".into(),
        cutoff_date: "2026-10-31".into(),
        shock_bps: bps,
        side_col: Some("Side".into()),
        reprice_col: None,
        beta_file: Some("b".into()),
        product_col: Some("Product".into()),
        floor_pct: None,
    }
}

fn nii_with(rows: &[micro_parquet::betas::BetaRow], bps: f64) -> Vec<Option<f64>> {
    let out = micro_parquet::banking::apply_rate_shock_betas(savings_book().lazy(), &shock_spec(bps), rows).expect("runs").collect().expect("collect");
    floats(&out, "NII_Impact_BDT")
}

const BETA_JSON: &str = r#"{ "source": "test", "betas": [
    { "product": "SAVINGS", "beta_pct": 40, "beta_up_pct": 30, "beta_down_pct": 60 },
    { "product": "FDR", "beta_pct": 100 },
    { "product": "OTHER", "beta_up": 20, "beta_down": 50 } ] }"#;

#[test]
fn i6_beta_up_applies_to_a_rise_beta_down_to_a_fall_and_a_file_with_only_beta_gives_todays_result() {
    use micro_parquet::betas::*;
    let rows = parse_beta_rows("b.json", BETA_JSON).expect("parses");
    // a rise of 100 bp: savings beta_up 30% -> interest +3,000 on a liability -> NII -3,000
    let up = nii_with(&rows, 100.0);
    close(up[0], -3_000.0, 1e-6, "savings, rise, beta_up 30%");
    close(up[1], -10_000.0, 1e-6, "FDR has only beta 100%: both directions use it");
    close(up[2], -2_000.0, 1e-6, "OTHER: the 'beta_up' alias, 20%");
    // a fall of 100 bp: savings beta_down 60% -> interest -6,000 -> NII +6,000
    let down = nii_with(&rows, -100.0);
    close(down[0], 6_000.0, 1e-6, "savings, fall, beta_down 60%");
    close(down[1], 10_000.0, 1e-6, "FDR, fall");
    close(down[2], 5_000.0, 1e-6, "OTHER, fall, beta_down 50%");
    // the single beta of the same product is still 40% (what a file with only beta_pct uses for both)
    let single = parse_beta_file("b.json", BETA_JSON).expect("single view");
    assert_eq!(single[0], ("SAVINGS".to_string(), 0.4));

    // a file with only beta: exactly today's result in both directions (the old function and the new agree)
    let only = r#"{ "betas": [ { "product": "SAVINGS", "beta_pct": 40 }, { "product": "FDR", "beta_pct": 100 }, { "product": "OTHER", "beta_pct": 25 } ] }"#;
    let rows = parse_beta_rows("only.json", only).expect("parses");
    assert!(rows.iter().all(|r| r.up == r.down && r.up == r.base));
    let pairs = parse_beta_file("only.json", only).expect("pairs");
    for bps in [100.0, -100.0] {
        let old = micro_parquet::banking::apply_rate_shock_with(savings_book().lazy(), &shock_spec(bps), &pairs).expect("runs").collect().expect("collect");
        assert_eq!(floats(&old, "NII_Impact_BDT"), nii_with(&rows, bps), "{bps} bp");
    }
    close(nii_with(&rows, 100.0)[0], -4_000.0, 1e-6, "40% of +100 bp on 1,000,000 (the Phase 4 example)");
    close(nii_with(&rows, -100.0)[0], 4_000.0, 1e-6, "and the same for a fall");

    // the file must say what to use
    let e = parse_beta_rows("x.json", r#"{ "betas": [ { "product": "A", "beta_up_pct": 30 } ] }"#).err().expect("incomplete").to_string();
    assert!(e.contains("needs beta_pct, or both beta_up_pct and beta_down_pct"), "{e}");
    let e = parse_beta_rows("x.json", r#"{ "betas": [ { "product": "A", "beta_pct": 40, "beta_down_pct": 120 } ] }"#).err().expect("over 100").to_string();
    assert!(e.contains("beta_down") && e.contains("not a percentage from 0 to 100"), "{e}");
}

#[test]
fn i6_the_step_reads_up_and_down_betas_from_the_file() {
    use micro_parquet::models::*;
    let d = dir("i6step");
    micro_parquet::betas::set_betas_dir_override(Some(d.clone()));
    std::fs::write(d.join("b.json"), BETA_JSON).expect("write");
    let step = |bps: f64| TransformStep::RateShockSim {
        bal_col: "Balance".into(),
        rate_col: "Rate".into(),
        mat_col: "Matdat".into(),
        cutoff_date: "2026-10-31".into(),
        shock_bps: bps,
        side_col: Some("Side".into()),
        reprice_col: None,
        beta_file: Some("b".into()),
        product_col: Some("Product".into()),
        floor_pct: None,
    };
    let up = run_steps("i6u", &savings_book(), &[step(100.0)]).expect("up");
    let down = run_steps("i6d", &savings_book(), &[step(-100.0)]).expect("down");
    close(floats(&up, "NII_Impact_BDT")[0], -3_000.0, 1e-6, "step, rise");
    close(floats(&down, "NII_Impact_BDT")[0], 6_000.0, 1e-6, "step, fall");
    micro_parquet::betas::set_betas_dir_override(None);
}

// ------------------------------------------------------------------------------------------ 7

fn dated_deals() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Deal".into(), &["D1", "D2"]),
        dates("Opndat", &["2026-03-15", "2026-08-01"]),
        dates("Matdat", &["2026-09-28", "2027-02-14"]),
        Series::new("Balance".into(), &[1_000_000.0, 1_000_000.0]),
        Series::new("Rate".into(), &[9.0, 9.0]),
    ])
    .expect("deals")
}

fn curve_of(date: &str, a: f64, b: f64) -> micro_parquet::curves::YieldCurve {
    micro_parquet::curves::YieldCurve {
        date: chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").expect("date"),
        currency: "BDT".into(),
        source: "test".into(),
        points: vec![FtpPoint { tenor_days: 30, ftp_rate: a }, FtpPoint { tenor_days: 365, ftp_rate: b }],
    }
}

fn ftp_step(currency_free_mode: FtpCurveMode) -> TransformStep {
    TransformStep::FtpCurveMap {
        opndat_col: "Opndat".into(),
        matdat_col: "Matdat".into(),
        bal_col: "Balance".into(),
        rate_col: "Rate".into(),
        curve: vec![],
        is_liability: true,
        interpolation: FtpInterpolation::Linear,
        curve_mode: currency_free_mode, currency: "BDT".into(),
    }
}

#[test]
fn i7_editing_a_curve_changes_ftp_curve_id_and_only_for_the_deals_priced_on_it() {
    use micro_parquet::curves::*;
    let d = dir("i7");
    let steps = vec![ftp_step(FtpCurveMode::OriginationDate { use_earliest_for_older: false })];
    let src = parquet(&d, "deals", &dated_deals());
    save_curve(&d.join("c"), &curve_of("2026-01-01", 8.0, 10.0)).expect("saves");
    save_curve(&d.join("c"), &curve_of("2026-07-01", 9.0, 11.0)).expect("saves");
    set_curves_dir_override(Some(d.join("c")));
    let run = || {
        let out = micro_parquet::engine::compile_tab_lazy(&src, &steps, &d).expect("compiles").collect().expect("collects");
        strings(&out, "FTP_Curve_Id")
    };
    let before = run();
    assert!(before.iter().all(|i| i.as_ref().is_some_and(|s| s.len() == 8 && s.chars().all(|c| c.is_ascii_hexdigit()))), "8 hex characters: {before:?}");
    assert_ne!(before[0], before[1], "the two deals are priced on two different curves");
    assert_eq!(run(), before, "the same files give the same ids");
    // the id of a curve is a function of its content
    assert_eq!(Some(curve_id(&curve_of("2026-01-01", 8.0, 10.0))), before[0]);
    // edit the January curve (one rate): its id changes, July's does not
    save_curve(&d.join("c"), &curve_of("2026-01-01", 8.0, 10.5)).expect("saves");
    let after = run();
    assert_ne!(after[0], before[0], "the edited curve has a new id");
    assert_eq!(after[1], before[1], "the other curve is untouched");
    // the date, the currency and the source are part of the content too
    let base = curve_id(&curve_of("2026-01-01", 8.0, 10.0));
    let mut c = curve_of("2026-01-01", 8.0, 10.0);
    c.source = "changed".into();
    assert_ne!(curve_id(&c), base);
    let mut c = curve_of("2026-01-01", 8.0, 10.0);
    c.currency = "USD".into();
    assert_ne!(curve_id(&c), base);
    // a fixed date has the column too
    let out = micro_parquet::engine::compile_tab_lazy(&src, &[ftp_step(FtpCurveMode::FixedDate { date: "2026-06-30".into() })], &d).expect("compiles").collect().expect("collects");
    assert_eq!(strings(&out, "FTP_Curve_Id"), vec![after[0].clone(), after[0].clone()]);
    // an old step that types its curve has no such column
    let out = run_steps("i7typed", &dated_deals(), &[TransformStep::FtpCurveMap {
        opndat_col: "Opndat".into(), matdat_col: "Matdat".into(), bal_col: "Balance".into(), rate_col: "Rate".into(),
        curve: vec![FtpPoint { tenor_days: 30, ftp_rate: 8.0 }, FtpPoint { tenor_days: 365, ftp_rate: 10.0 }],
        is_liability: true, interpolation: FtpInterpolation::Linear, curve_mode: FtpCurveMode::Inline, currency: "BDT".into(),
    }]).expect("typed");
    assert!(out.column("FTP_Curve_Id").is_err());
    set_curves_dir_override(None);
}

// ------------------------------------------------------------------------------------------ 8

#[test]
fn i8_a_curve_in_another_currency_is_not_used_and_a_file_without_a_currency_is_an_error_naming_it() {
    use micro_parquet::curves::*;
    let d = dir("i8");
    let cdir = d.join("c");
    let src = parquet(&d, "deals", &dated_deals());
    let bdt_step = |mode| ftp_step(mode);
    let origination = FtpCurveMode::OriginationDate { use_earliest_for_older: true };
    save_curve(&cdir, &curve_of("2026-01-01", 8.0, 10.0)).expect("saves");
    // a USD curve in between: dated after the BDT one and before the second deal
    let mut usd = curve_of("2026-05-01", 1.0, 2.0);
    usd.currency = "USD".into();
    save_curve(&cdir, &usd).expect("saves");
    set_curves_dir_override(Some(cdir.clone()));

    // the BDT step ignores it: both deals are priced on the BDT curve of January (the second deal would have used May's)
    let out = micro_parquet::engine::compile_tab_lazy(&src, &[bdt_step(origination.clone())], &d).expect("compiles").collect().expect("collects");
    let cd = out.column("FTP_Curve_Date").expect("col").cast(&DataType::String).expect("cast");
    let cd: Vec<Option<String>> = cd.str().expect("str").into_iter().map(|v| v.map(str::to_string)).collect();
    assert_eq!(cd, vec![Some("2026-01-01".to_string()), Some("2026-01-01".to_string())], "the USD curve of 2026-05-01 was not used");
    close(floats(&out, "Benchmark_FTP_Rate")[1], 8.0 + (197.0 - 30.0) / (365.0 - 30.0) * 2.0, 1e-5, "the 197-day deal on the January BDT curve");
    // the same step in USD uses the USD curve only
    let mut usd_step = bdt_step(origination.clone());
    if let TransformStep::FtpCurveMap { currency, .. } = &mut usd_step {
        *currency = "usd".into();
    }
    let out = micro_parquet::engine::compile_tab_lazy(&src, &[usd_step], &d).expect("compiles").collect().expect("collects");
    let rates = floats(&out, "Benchmark_FTP_Rate");
    assert!(rates[1].is_some_and(|r| r < 3.0), "the August deal is priced on the USD curve: {rates:?}");
    // the skipped curve is listed in a note
    let notes = micro_parquet::engine::blank_balance_notes(&src, &[bdt_step(origination.clone())], &d).expect("notes");
    assert!(notes.iter().any(|n| n.contains("skipped 1 curve(s) in another currency than BDT") && n.contains("2026-05-01 (USD)")), "{notes:?}");
    // only a USD file in the store: a BDT step stops and says what it found
    std::fs::remove_file(cdir.join("2026-01-01.json")).expect("remove");
    let e = format!("{:#}", micro_parquet::engine::compile_tab_lazy(&src, &[bdt_step(origination.clone())], &d).and_then(|lf| Ok(lf.collect()?)).err().expect("no BDT curve"));
    assert!(e.contains("no yield curve in BDT") && e.contains("2026-05-01 (USD)"), "{e}");

    // a file with no currency: an error that names the file, from the parser and from the step
    std::fs::write(cdir.join("2026-02-01.json"), r#"{ "as_of": "2026-02-01", "points": [ { "tenor_days": 30, "rate_pct": 8.0 } ] }"#).expect("write");
    let e = parse_curve_file("2026-02-01.json", &std::fs::read_to_string(cdir.join("2026-02-01.json")).expect("read")).err().expect("no currency").to_string();
    assert!(e.contains("currency is missing"), "{e}");
    let store = CurveStore::load(&cdir);
    assert!(store.problems.iter().any(|(f, r)| f == "2026-02-01.json" && r.contains("Curve file 2026-02-01.json") && r.contains("currency is missing")), "{:?}", store.problems);
    let e = format!("{:#}", micro_parquet::engine::compile_tab_lazy(&src, &[bdt_step(origination)], &d).and_then(|lf| Ok(lf.collect()?)).err().expect("a curve without currency"));
    assert!(e.contains("2026-02-01.json") && e.contains("currency is missing"), "{e}");
    // a blank currency in the editor is refused too (it is saved only with one)
    assert!(parse_curve_file("2026-03-01.json", r#"{ "currency": "  ", "points": [ { "tenor_days": 30, "rate_pct": 8.0 } ] }"#).is_err());
    // an old step without the field reads BDT
    let old: TransformStep = serde_json::from_str(r#"{"FtpCurveMap":{"opndat_col":"O","matdat_col":"M","bal_col":"B","rate_col":"R","curve":[],"is_liability":true,"curve_mode":{"OriginationDate":{"use_earliest_for_older":false}}}}"#).expect("loads");
    assert!(matches!(old, TransformStep::FtpCurveMap { ref currency, .. } if currency == "BDT"));
    set_curves_dir_override(None);
}
