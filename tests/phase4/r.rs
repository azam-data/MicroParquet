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
            PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None },
            PivotValue { field: "Rate".into(), agg: PivotAgg::WeightedAvg, weight_col: Some("Balance".into()), time: None },
            PivotValue { field: "Balance".into(), agg: PivotAgg::Count, weight_col: None, time: None },
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
    // the totals row is followed only by the row of overdue assets (item 10)
    let last = out.height() - 2;
    assert_eq!(strings(&out, "Bucket")[last].as_deref(), Some("Total"));
    assert_eq!(strings(&out, "Bucket")[last + 1].as_deref(), Some("Overdue assets (not in gap)"));
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

// ------------------------------------------------------------------------------------------ 9

fn blank_rate_book(n_blank: usize, with_account: bool) -> DataFrame {
    let mut accts: Vec<String> = Vec::new();
    let mut rates: Vec<Option<f64>> = Vec::new();
    for i in 0..n_blank {
        accts.push(format!("A{}", i + 1));
        rates.push(None);
    }
    accts.push("OK1".into());
    rates.push(Some(2.0));
    let n = accts.len();
    let mut cols = vec![
        Series::new("Balance".into(), vec![1_000_000.0; n]),
        Series::new("Rate".into(), rates),
        Series::new("Side".into(), vec!["Liability"; n]),
        dates("Matdat", &vec![""; n]),
    ];
    if with_account {
        cols.insert(0, Series::new("Account No".into(), accts));
    }
    DataFrame::new(cols).expect("book")
}

#[test]
fn i9_with_a_floor_blank_rates_stop_the_step_with_the_count_and_the_first_five_accounts() {
    let mut s = shock_spec(-100.0);
    s.beta_file = None;
    s.product_col = None;
    // 7 rows without a rate and a floor: an error with the count and the first 5 account numbers
    let df = blank_rate_book(7, true);
    s.floor_pct = Some(0.0);
    let e = format!("{:#}", micro_parquet::banking::apply_rate_shock_with(df.clone().lazy(), &s, &[]).err().expect("blank rates with a floor"));
    assert!(e.contains("7 row(s) have no current rate in 'Rate'") && e.contains("floor of 0%"), "{e}");
    assert!(e.contains("first 5 values of Account No: A1, A2, A3, A4, A5."), "{e}");
    assert!(!e.contains("A6") && !e.contains("OK1"), "only the first five: {e}");
    // fewer than five: all of them
    let e = format!("{:#}", micro_parquet::banking::apply_rate_shock_with(blank_rate_book(2, true).lazy(), &s, &[]).err().expect("two"));
    assert!(e.contains("2 row(s)") && e.contains("first 2 values of Account No: A1, A2."), "{e}");
    // no account column: the rows are named
    let e = format!("{:#}", micro_parquet::banking::apply_rate_shock_with(blank_rate_book(3, false).lazy(), &s, &[]).err().expect("rows"));
    assert!(e.contains("first 3 rows: row 1, row 2, row 3."), "{e}");
    // the step says the same
    let step = TransformStep::RateShockSim {
        bal_col: "Balance".into(), rate_col: "Rate".into(), mat_col: "Matdat".into(), cutoff_date: "2026-10-31".into(), shock_bps: -100.0,
        side_col: Some("Side".into()), reprice_col: None, beta_file: None, product_col: None, floor_pct: Some(0.0),
    };
    let e = format!("{:#}", run_steps("i9", &df, &[step.clone()]).err().expect("the step stops"));
    assert!(e.contains("7 row(s) have no current rate"), "{e}");

    // without a floor nothing changes: the blank rate counts as 0 (what the step always did)
    s.floor_pct = None;
    let out = micro_parquet::banking::apply_rate_shock_with(df.clone().lazy(), &s, &[]).expect("no floor").collect().expect("collect");
    assert_eq!(floats(&out, "Current_Annual_Int")[0], Some(0.0));
    close(floats(&out, "NII_Impact_BDT")[0], 10_000.0, 1e-6, "the fall of 1% on a liability saves 10,000 (blank rate treated as 0, as before)");
    // with a floor and every rate present it runs
    let full = DataFrame::new(vec![
        Series::new("Balance".into(), &[1_000_000.0]), Series::new("Rate".into(), &[1.5]), Series::new("Side".into(), &["Liability"]), dates("Matdat", &[""]),
    ]).expect("full");
    s.floor_pct = Some(0.0);
    let out = micro_parquet::banking::apply_rate_shock_with(full.lazy(), &s, &[]).expect("runs").collect().expect("collect");
    close(floats(&out, "NII_Impact_BDT")[0], 10_000.0, 1e-6, "-100 bp on 1.5% with a 0% floor");
}

// ------------------------------------------------------------------------------------------ 11

#[test]
fn i11_the_repricing_gap_names_its_last_bucket_gt_12m_and_notes_how_many_rows_have_no_date() {
    use micro_parquet::banking::*;
    assert_eq!(REPRICING_BUCKETS[4], ">12m");
    let book = |mats: &[&str], reprice: &[&str]| {
        let n = mats.len();
        DataFrame::new(vec![
            Series::new("Side".into(), vec!["Asset"; n]),
            Series::new("Balance".into(), vec![100.0; n]),
            Series::new("Rate".into(), vec![10.0; n]),
            dates("Matdat", mats),
            dates("Reprice", reprice),
        ])
        .expect("book")
    };
    let spec = |reprice: Option<&str>| RateShockSpec {
        bal_col: "Balance".into(), rate_col: "Rate".into(), mat_col: "Matdat".into(), cutoff_date: "2026-10-31".into(), shock_bps: 100.0,
        side_col: Some("Side".into()), reprice_col: reprice.map(str::to_string), beta_file: None, product_col: None, floor_pct: None,
    };
    let labels = |g: &DataFrame| strings(g, "Repricing bucket").into_iter().flatten().collect::<Vec<_>>();
    // three rows with neither date: the note counts exactly them
    let df = book(&["", "2027-03-01", "", ""], &["", "", "", ""]);
    let g = build_repricing_gap(df.clone().lazy(), &spec(None), &[]).expect("builds");
    let l = labels(&g);
    assert_eq!(&l[..6], &["0\u{2013}1m", "1\u{2013}3m", "3\u{2013}6m", "6\u{2013}12m", ">12m", "Total"]);
    assert_eq!(l[6], "3 rows with no maturity or repricing date placed in 0\u{2013}1m (contractual)");
    assert_eq!(l.len(), 7, "the note is the last row, under the Total");
    assert_eq!(floats(&g, "Assets")[6], None, "the note row has no numbers");
    assert_eq!(floats(&g, "Assets")[0], Some(300.0), "the three rows are in 0-1m");
    assert_eq!(floats(&g, "Assets")[5], Some(400.0), "the Total row is not changed by the note");
    // one such row: singular
    let g = build_repricing_gap(book(&["", "2027-03-01"], &["", ""]).lazy(), &spec(None), &[]).expect("builds");
    assert_eq!(labels(&g)[6], "1 row with no maturity or repricing date placed in 0\u{2013}1m (contractual)");
    // a repricing date makes a row dated: with it, only the rows with neither date are counted
    let df = book(&["", "", ""], &["2026-12-01", "", ""]);
    let g = build_repricing_gap(df.lazy(), &spec(Some("Reprice")), &[]).expect("builds");
    assert_eq!(labels(&g)[6], "2 rows with no maturity or repricing date placed in 0\u{2013}1m (contractual)");
    // every row dated: no note
    let g = build_repricing_gap(book(&["2026-12-01", "2027-03-01"], &["", ""]).lazy(), &spec(None), &[]).expect("builds");
    assert_eq!(labels(&g).len(), 6, "no note when every row has a date: {:?}", labels(&g));
    // a date on or before the cutoff is a date (the row reprices at once, but it is not 'no date')
    let g = build_repricing_gap(book(&["2026-09-01"], &[""]).lazy(), &spec(None), &[]).expect("builds");
    assert_eq!(labels(&g).len(), 6);
}

// ------------------------------------------------------------------------------------------ 12

#[test]
fn i12_a_curve_csv_skips_at_most_one_heading_line_and_any_other_bad_line_gives_its_number() {
    use micro_parquet::curves::parse_curve_csv;
    let pts = |t: &str| parse_curve_csv(t).unwrap_or_else(|e| panic!("{t:?}: {e:#}"));
    // no heading, one heading, a heading after blank lines, a BOM, semicolons and a % sign
    assert_eq!(pts("30,8\n365,10\n").len(), 2);
    assert_eq!(pts("tenor_days,rate_pct\n30,8\n365,10\n").len(), 2);
    assert_eq!(pts("\n\n\u{feff}tenor_days;rate_pct\n30;8\n365;10%\n").len(), 2);
    let err = |t: &str| format!("{:#}", parse_curve_csv(t).err().unwrap_or_else(|| panic!("{t:?} must be refused")));
    // two heading-like lines: the second is an error, with its number
    let e = err("Treasury curve\ntenor_days,rate_pct\n30,8\n365,10\n");
    assert!(e.contains("Line 2:") && e.contains("'tenor_days'"), "{e}");
    // a bad line in the middle or at the end
    let e = err("tenor_days,rate_pct\n30,8\nninety,9\n365,10\n");
    assert!(e.contains("Line 3: 'ninety' is not a whole number of days"), "{e}");
    let e = err("30,8\n90,abc\n");
    assert!(e.contains("Line 2: 'abc' is not a rate in percent"), "{e}");
    let e = err("30,8\n365,10\nTotal\n");
    assert!(e.contains("Line 3:"), "{e}");
    // line numbers count the blank lines too, and a heading line is not counted as a point
    let e = err("\ntenor_days,rate_pct\n\n30,8\n\nx,9\n");
    assert!(e.contains("Line 6: 'x' is not a whole number of days"), "{e}");
    // a data-looking first line is never skipped
    let e = err("30,8,1\n");
    assert!(e.contains("Line 1: expected two columns"), "{e}");
    let e = err("abc,9\n30,8\n");
    assert!(e.contains("Line 1: 'abc' is not a whole number of days"), "a line with a rate and no tenor is not a heading: {e}");
    // only a heading: no points
    assert!(err("tenor_days,rate_pct\n").contains("no points"));
}

// ------------------------------------------------------------------------------------------ 14

#[test]
fn i14_a_bad_curve_file_error_names_the_file_and_the_line() {
    use micro_parquet::curves::*;
    let d = dir("i14");
    let write = |name: &str, text: &str| std::fs::write(d.join(name), text).expect("write");
    // duplicate tenors on lines 4 and 5
    write("2026-01-01.json", "{\n  \"currency\": \"BDT\",\n  \"points\": [\n    { \"tenor_days\": 30, \"rate_pct\": 8.0 },\n    { \"tenor_days\": 30, \"rate_pct\": 9.0 }\n  ]\n}\n");
    // tenors out of order on lines 4 and 5
    write("2026-02-01.json", "{\n  \"currency\": \"BDT\",\n  \"points\": [\n    { \"tenor_days\": 365, \"rate_pct\": 10.0 },\n    { \"tenor_days\": 30, \"rate_pct\": 8.0 }\n  ]\n}\n");
    // a syntax error: a missing comma between the points, on line 5
    write("2026-03-01.json", "{\n  \"currency\": \"BDT\",\n  \"points\": [\n    { \"tenor_days\": 30, \"rate_pct\": 8.0 }\n    { \"tenor_days\": 90, \"rate_pct\": 9.0 }\n  ]\n}\n");
    // as_of that disagrees with the name, on line 3
    write("2026-04-01.json", "{\n  \"currency\": \"BDT\",\n  \"as_of\": \"2026-05-01\",\n  \"points\": [ { \"tenor_days\": 30, \"rate_pct\": 8.0 } ]\n}\n");
    // a rate that is not a number is a syntax error of the file: a tenor of 0 on line 4
    write("2026-05-01.json", "{\n  \"currency\": \"BDT\",\n  \"points\": [\n    { \"tenor_days\": 0, \"rate_pct\": 8.0 }\n  ]\n}\n");
    // a currency that is empty, on line 2
    write("2026-06-01.json", "{\n  \"currency\": \"\",\n  \"points\": [ { \"tenor_days\": 30, \"rate_pct\": 8.0 } ]\n}\n");
    write("2026-07-01.json", "{\n  \"currency\": \"BDT\",\n  \"points\": [ { \"tenor_days\": 30, \"rate_pct\": 8.0 } ]\n}\n");
    let store = CurveStore::load(&d);
    assert_eq!(store.curves.len(), 1, "only the good file loads: {:?}", store.problems);
    let reason = |f: &str| store.problems.iter().find(|(n, _)| n == f).map(|(_, r)| r.clone()).unwrap_or_else(|| panic!("{f} should be a problem: {:?}", store.problems));
    let r = reason("2026-01-01.json");
    assert!(r.starts_with("Curve file 2026-01-01.json:") && r.contains("Line 4 and line 5 both have the tenor 30 days"), "{r}");
    let r = reason("2026-02-01.json");
    assert!(r.starts_with("Curve file 2026-02-01.json:") && r.contains("365 days (line 4) is followed by 30 days (line 5)"), "{r}");
    let r = reason("2026-03-01.json");
    assert!(r.starts_with("Curve file 2026-03-01.json:") && r.contains("Line 5:") && r.contains("not a valid curve file"), "{r}");
    let r = reason("2026-04-01.json");
    assert!(r.contains("Line 3: the file name says 2026-04-01 but 'as_of' says 2026-05-01"), "{r}");
    let r = reason("2026-05-01.json");
    assert!(r.contains("Line 4: the tenor 0 is not a number of days above 0"), "{r}");
    let r = reason("2026-06-01.json");
    assert!(r.contains("Line 2: the currency is empty"), "{r}");
    // a step that reads the store stops with the first of them, naming its file
    set_curves_dir_override(Some(d.clone()));
    let src = parquet(&d, "deals", &dated_deals());
    let e = format!("{:#}", micro_parquet::engine::compile_tab_lazy(&src, &[ftp_step(FtpCurveMode::OriginationDate { use_earliest_for_older: true })], &d).and_then(|lf| Ok(lf.collect()?)).err().expect("a bad curve file"));
    assert!(e.contains("2026-01-01.json") && e.contains("Line 4"), "{e}");
    set_curves_dir_override(None);
    // the same checks through the editor's own validation still say "point N"
    let e = validate_points(&[FtpPoint { tenor_days: 30, ftp_rate: 8.0 }, FtpPoint { tenor_days: 30, ftp_rate: 9.0 }]).err().expect("dup").to_string();
    assert!(e.contains("Point 1 and point 2 both have the tenor 30 days"), "{e}");
}

// ------------------------------------------------------------------------------------------ 15

/// A CSV with 5 records, three of them with line breaks inside quotes (\n, \r\n and a run of two).
fn multiline_csv(d: &std::path::Path) -> TabSource {
    let text = "Actnum,Remarks,Balance\n\
        A1,\"Paid in full\nby cheque\",100\n\
        A2,plain,200\n\
        A3,\"Line one\r\nLine two\",300\n\
        A4,\"  leading and trailing\n\n\n  \",400\n\
        A5,fine,500\n";
    let p = d.join("multiline.csv");
    std::fs::write(&p, text).expect("write the CSV");
    let config = IngestionConfig { sanitize_linebreaks: false, ..IngestionConfig::default() };
    TabSource::SingleFile { path: p, is_parquet: false, config }
}

#[test]
fn i15_clean_line_breaks_gives_one_space_per_run_trims_and_replays_from_a_macro() {
    let d = dir("i15");
    let src = multiline_csv(&d);
    // the file: 7 lines of data but 5 records; the row check explains it with the 3 line breaks inside quotes
    let before = micro_parquet::engine::compile_tab_lazy(&src, &[], &d).expect("compiles").collect().expect("collects");
    assert_eq!(before.height(), 5, "the right row count: 5 records");
    let rc = micro_parquet::engine::source_row_check(&src, &d).expect("checks").expect("a CSV has a row check");
    assert_eq!(rc.multiline_records, 3, "{rc:?}");
    assert!(rc.is_explained(), "the line breaks explain every missing line: {rc:?}");
    assert!(strings(&before, "Remarks")[0].as_deref().unwrap_or("").contains('\n'), "the values still have their line breaks until the step runs");

    let step = TransformStep::CleanLineBreaks { columns: vec![], trim_all: false };
    let after = micro_parquet::engine::compile_tab_lazy(&src, &[step.clone()], &d).expect("compiles").collect().expect("collects");
    assert_eq!(after.height(), 5, "no row is lost or added");
    let remarks: Vec<String> = strings(&after, "Remarks").into_iter().map(|v| v.unwrap_or_default()).collect();
    assert_eq!(remarks, vec!["Paid in full by cheque", "plain", "Line one Line two", "leading and trailing", "fine"], "one space per run of breaks, ends trimmed");
    assert_eq!(strings(&after, "Actnum"), strings(&before, "Actnum"), "other columns are the same");
    assert_eq!(floats(&after, "Balance"), floats(&before, "Balance"), "numbers are not touched");
    // a value without a line break is not touched, even with spaces at its ends
    let spaced = DataFrame::new(vec![Series::new("T".into(), &[" keep  ", "a\nb "])]).expect("frame");
    let out = run_steps("i15s", &spaced, &[step.clone()]).expect("runs");
    assert_eq!(strings(&out, "T"), vec![Some(" keep  ".to_string()), Some("a b".to_string())]);
    // chosen columns: only those; a missing or numeric column is an error
    let only = TransformStep::CleanLineBreaks { columns: vec!["Remarks".into()], trim_all: false };
    let out = micro_parquet::engine::compile_tab_lazy(&src, &[only], &d).expect("compiles").collect().expect("collects");
    assert_eq!(strings(&out, "Remarks")[2].as_deref(), Some("Line one Line two"));
    let e = format!("{:#}", micro_parquet::engine::compile_tab_lazy(&src, &[TransformStep::CleanLineBreaks { columns: vec!["Nope".into()], trim_all: false }], &d).and_then(|lf| Ok(lf.collect()?)).err().expect("missing column"));
    assert!(e.contains("'Nope' was not found"), "{e}");
    let e = format!("{:#}", run_steps("i15n", &spaced.hstack(&[Series::new("N".into(), &[1.0, 2.0]).into()]).expect("hstack"), &[TransformStep::CleanLineBreaks { columns: vec!["N".into()], trim_all: false }]).err().expect("not text"));
    assert!(e.contains("'N'") && e.contains("not text"), "{e}");

    // a macro with the step replays to the same values (and an old macro JSON without 'columns' means all text columns)
    let recipe = MacroRecipe {
        name: "clean".into(), description: String::new(), created_at: String::new(), enforce_balance_control: false, control_column: None,
        optional_ingestion_schema: None, steps: vec![step.clone()], summary: Default::default(),
    };
    let mp = d.join("clean.mpq.json");
    micro_parquet::macros::save_macro_to_file(&recipe, &mp).expect("save");
    let back = micro_parquet::macros::load_macro_from_file(&mp).expect("load");
    assert_eq!(serde_json::to_string(&back.steps).expect("json"), serde_json::to_string(&vec![step.clone()]).expect("json"));
    let replayed = micro_parquet::engine::compile_tab_lazy(&src, &back.steps, &d).expect("compiles").collect().expect("collects");
    assert_eq!(strings(&replayed, "Remarks"), strings(&after, "Remarks"));
    let bare: TransformStep = serde_json::from_str(r#"{"CleanLineBreaks":{}}"#).expect("loads without columns");
    assert_eq!(serde_json::to_string(&bare).expect("json"), serde_json::to_string(&step).expect("json"));
    // the Python export translates it
    let py = micro_parquet::macros::generate_python_polars_code(&d.join("multiline.csv"), false, &IngestionConfig::default(), &back.steps, None).expect("python");
    assert!(py.contains("str.replace_all(r\"[\\r\\n]+\", \" \")") && py.contains("str.strip_chars()"), "{py}");
    let _ = std::fs::remove_dir_all(&d);
}

// ------------------------------------------------------------------------------------------ 20

fn segment_book() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Segment".into(), &["CMSME", "Retail", "CMSME", "Corporate", "Retail", "CMSME", "Corporate"]),
        Series::new("Bucket".into(), &["01. Sight", "02. 1-3m", "02. 1-3m", "01. Sight", "01. Sight", "03. Over 3m", "02. 1-3m"]),
        Series::new("Balance".into(), &[100.0, 200.0, 300.0, 400.0, 50.0, 70.0, 80.0]),
        Series::new("Rate".into(), &[10.0, 8.0, 12.0, 9.0, 6.0, 14.0, 7.0]),
    ])
    .expect("book")
}

fn segment_spec(filter: Vec<Option<String>>) -> PivotSpec {
    PivotSpec {
        rows: vec!["Bucket".into()],
        values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None }, PivotValue { field: "Rate".into(), agg: PivotAgg::WeightedAvg, weight_col: Some("Balance".into()), time: None }],
        filters: vec![PivotFilter { field: "Segment".into(), values: filter }],
        row_totals: true,
        column_totals: true,
        ..Default::default()
    }
}

#[test]
fn i20_a_pivot_filter_removes_the_other_rows_before_the_aggregation_and_the_field_is_not_a_row_or_column() {
    let df = segment_book();
    let run = |s: &PivotSpec| build_pivot(df.clone().lazy(), s).expect("pivot").df;
    // no restriction: every segment
    let all = run(&segment_spec(vec![]));
    let last = all.height() - 1;
    assert_eq!(strings(&all, "Bucket")[last].as_deref(), Some("Grand total"));
    close(floats(&all, "Sum of Balance")[last], 1200.0, 1e-9, "all segments");
    // Segment = CMSME: the totals of CMSME only (100 + 300 + 70)
    let one = run(&segment_spec(vec![Some("CMSME".into())]));
    let last = one.height() - 1;
    close(floats(&one, "Sum of Balance")[last], 470.0, 1e-9, "the grand total is CMSME's");
    assert_eq!(one.get_column_names().iter().map(|s| s.to_string()).collect::<Vec<_>>(), vec!["Bucket", "Sum of Balance", "W.Avg of Rate by Balance"], "Segment is not a row or a column");
    // by bucket: 01 100, 02 300, 03 70
    assert_eq!(strings(&one, "Bucket").into_iter().flatten().collect::<Vec<_>>(), vec!["01. Sight", "02. 1-3m", "03. Over 3m", "Grand total"]);
    assert_eq!(&floats(&one, "Sum of Balance")[..3], &[Some(100.0), Some(300.0), Some(70.0)]);
    // the weighted rate is over the kept rows only: (10*100 + 12*300 + 14*70) / 470
    close(floats(&one, "W.Avg of Rate by Balance")[last], (10.0 * 100.0 + 12.0 * 300.0 + 14.0 * 70.0) / 470.0, 1e-9, "weighted over CMSME rows only");
    // two segments: Retail and Corporate (200 + 50 + 400 + 80)
    let two = run(&segment_spec(vec![Some("Retail".into()), Some("Corporate".into())]));
    close(floats(&two, "Sum of Balance")[two.height() - 1], 730.0, 1e-9, "two segments");
    // a value that is not in the data keeps nothing: an empty pivot, not a wrong one
    let none = run(&segment_spec(vec![Some("Nobody".into())]));
    assert!(none.height() <= 1, "no row is kept: {}", none.height());
    // the step carries the filter and a macro file keeps it
    let step = TransformStep::from_pivot_spec(segment_spec(vec![Some("CMSME".into())]));
    let d = dir("i20");
    let recipe = MacroRecipe { name: "p".into(), description: String::new(), created_at: String::new(), enforce_balance_control: false, control_column: None, optional_ingestion_schema: None, steps: vec![step.clone()], summary: Default::default() };
    let mp = d.join("p.mpq.json");
    micro_parquet::macros::save_macro_to_file(&recipe, &mp).expect("save");
    let back = micro_parquet::macros::load_macro_from_file(&mp).expect("load");
    assert_eq!(back.steps[0].as_pivot_spec().map(|s| s.filters), Some(vec![PivotFilter { field: "Segment".into(), values: vec![Some("CMSME".into())] }]));
    let out = run_steps("i20s", &df, &back.steps).expect("replays");
    close(floats(&out, "Sum of Balance")[out.height() - 1], 470.0, 1e-9, "the replayed macro gives CMSME's totals");
}

// ------------------------------------------------------------------------------------------ 17

fn pivot_for_pdf(banks: usize, scale: PivotScale) -> (DataFrame, std::collections::HashMap<String, ColumnFormatSpec>) {
    let n = banks * 3;
    let df = DataFrame::new(vec![
        Series::new("Segment".into(), (0..n).map(|i| ["CMSME", "Retail", "Corporate"][i % 3].to_string()).collect::<Vec<_>>()),
        Series::new("Bank".into(), (0..n).map(|i| format!("Bank {:02}", i / 3)).collect::<Vec<_>>()),
        Series::new("Balance".into(), (0..n).map(|i| 25_000_000.0 + 1_234_567_890.0 * ((i % 7) as f64 + 1.0)).collect::<Vec<_>>()),
        Series::new("Rate".into(), (0..n).map(|i| 6.0 + (i % 5) as f64 * 0.75).collect::<Vec<_>>()),
    ])
    .expect("book");
    let spec = PivotSpec {
        rows: vec!["Segment".into()],
        columns: vec!["Bank".into()],
        values: vec![
            PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None },
            PivotValue { field: "Rate".into(), agg: PivotAgg::WeightedAvg, weight_col: Some("Balance".into()), time: None },
        ],
        row_totals: true,
        column_totals: true,
        scale,
        ..Default::default()
    };
    let out = build_pivot(df.lazy(), &spec).expect("pivot").df;
    let cols: Vec<String> = out.get_column_names().iter().map(|s| s.to_string()).collect();
    let formats = pivot_column_formats(&spec, &cols, 2).into_iter().collect();
    (out, formats)
}

const UNIT_WORDS: [&str; 6] = ["BDT", "Crore", " Cr", "(Cr", "Lakh", "Million"];

#[test]
fn i17_every_pdf_column_has_one_x_and_width_for_header_and_values_numbers_end_where_their_header_ends_and_no_cell_has_the_unit() {
    use micro_parquet::pdf_layout::*;
    // Helvetica's widths: "0" is 556/1000 em; at 10 pt that is 5.56 pt
    let mm = |pt: f32| pt * 25.4 / 72.0;
    assert!((text_width_mm("0", 10.0, FontKind::Regular) - mm(5.56)).abs() < 1e-4);
    // Bold is wider than regular for the same text
    assert!(text_width_mm("Total", 10.0, FontKind::Bold) > text_width_mm("Total", 10.0, FontKind::Regular));
    for banks in [2usize, 6, 40] {
        let (df, formats) = pivot_for_pdf(banks, PivotScale::Crore);
        let lay = layout_pdf_tab("Exposure", &df, &formats, NumberDisplayFormat::Raw, 2, &[], 5000);
        assert_eq!(lay.unit_line.as_deref(), Some("Figures in BDT Crore"), "{banks} banks: the unit is stated once under the title");
        if banks == 40 {
            assert!(lay.parts.len() >= 2, "80 value columns do not fit one page: {} part(s)", lay.parts.len());
        }
        let mut printed: Vec<String> = Vec::new();
        for (pn, part) in lay.parts.iter().enumerate() {
            let what = format!("{banks} banks, part {}", pn + 1);
            assert_eq!(part.columns[0].name, "Segment", "{what}: the row labels are repeated");
            let right_edge = part.columns.last().map(|c| c.x + c.w).expect("columns");
            assert!(right_edge <= LEFT_MM + USABLE_W_MM + 0.01, "{what}: inside the page width ({right_edge})");
            for (k, col) in part.columns.iter().enumerate() {
                // header: same x and width as the column
                let h = &part.header[k];
                assert!((h.col_x - col.x).abs() < 1e-4 && (h.col_w - col.w).abs() < 1e-4, "{what}: header of '{}' is in its column", col.name);
                assert!(h.lines.len() <= 2, "{what}: at most 2 header lines");
                for row in &part.rows {
                    let c = &row.cells[k];
                    // values: same x and width as the column
                    assert!((c.col_x - col.x).abs() < 1e-4 && (c.col_w - col.w).abs() < 1e-4, "{what}: a value of '{}' is in its column", col.name);
                    assert_eq!(c.align, col.align);
                    let right = col.x + col.w - PAD_MM;
                    match col.align {
                        PdfAlign::Right => {
                            assert!((c.text.x + c.text.w - right).abs() < 1e-3, "{what}: '{}' ends at the column's right edge", c.text.text);
                            for l in &h.lines {
                                assert!((l.x + l.w - right).abs() < 1e-3, "{what}: header '{}' ends at the same x as the numbers", l.text);
                            }
                        }
                        PdfAlign::Left => {
                            assert!((c.text.x - (col.x + PAD_MM)).abs() < 1e-3);
                            for l in &h.lines {
                                assert!((l.x - (col.x + PAD_MM)).abs() < 1e-3, "{what}: a text header starts where its values start");
                            }
                        }
                        PdfAlign::Center => unreachable!(),
                    }
                    // the unit is not in any cell
                    assert!(UNIT_WORDS.iter().all(|u| !c.text.text.contains(u)), "{what}: no unit in '{}'", c.text.text);
                    assert!(c.text.x >= col.x - 1e-3 && c.text.x + c.text.w <= col.x + col.w + 1e-3, "{what}: '{}' is inside its column", c.text.text);
                }
                for l in &h.lines {
                    assert!(UNIT_WORDS.iter().all(|u| !l.text.contains(u)), "{what}: no unit in the header '{}'", l.text);
                    assert!(l.x >= col.x - 1e-3 && l.x + l.w <= col.x + col.w + 1e-3, "{what}: the header '{}' does not overflow its column", l.text);
                }
                // the width is the widest header line or value plus the padding
                let widest_value = part.rows.iter().map(|r| r.cells[k].text.w).fold(0.0f32, f32::max);
                let widest_header = h.lines.iter().map(|l| l.w).fold(0.0f32, f32::max);
                let want = widest_value.max(widest_header) + 2.0 * PAD_MM;
                assert!((col.w - want.clamp(MIN_COL_MM, MAX_COL_MM)).abs() < 0.2 || col.w > want, "{what}: '{}' is {} wide, content needs {}", col.name, col.w, want);
                if k >= 1 {
                    printed.push(col.name.clone());
                }
            }
            // two levels: a bank spans its two value columns, and sits over them
            let groups = part.group_header.as_ref().expect("a pivot with columns has two header levels");
            for g in groups {
                assert_eq!(g.span, 2, "{what}: '{}' spans its two value columns", g.lines[0].text);
                let kids = &part.columns[g.first_col..g.first_col + g.span];
                assert!((g.col_x - kids[0].x).abs() < 1e-4 && (g.col_w - kids.iter().map(|c| c.w).sum::<f32>()).abs() < 1e-3);
                let l = &g.lines[0];
                assert!((l.x + l.w / 2.0 - (g.col_x + g.col_w / 2.0)).abs() < 1e-3, "{what}: centred over its columns");
                assert!(l.x >= g.col_x - 1e-3 && l.x + l.w <= g.col_x + g.col_w + 1e-3, "{what}: the group label fits its span");
            }
            // the bottom level holds the value names, not "Bank | Sum of Balance"
            assert!(part.header.iter().skip(1).all(|h| !h.lines[0].text.contains(" | ")), "{what}");
            // the grand total row is bold with a rule above it, and only it
            let totals: Vec<&PdfRow> = part.rows.iter().filter(|r| r.total).collect();
            assert_eq!(totals.len(), 1, "{what}");
            assert_eq!(totals[0].cells[0].text.text, "Grand total");
            assert!(part.rows.last().map(|r| r.total).unwrap_or(false), "{what}: it is the last row");
        }
        // every value column of the pivot is printed once, in order
        assert_eq!(printed.len(), df.width() - 1, "{banks} banks: every column is on some page");
    }
}

#[test]
fn i17_a_header_wider_than_its_values_wraps_to_two_lines_and_a_text_column_is_left_aligned_in_both() {
    use micro_parquet::pdf_layout::*;
    let df = DataFrame::new(vec![
        Series::new("Account holder name".into(), &["A", "B"]),
        Series::new("Cumulative gap % of total assets".into(), &[1.5, -2.5]),
        Series::new("Outstanding_Balance_Amount_Total_In_Taka_And_More".into(), &[10.0, 20.0]),
        Series::new("N".into(), &[1.0, 2.0]),
    ])
    .expect("frame");
    let lay = layout_pdf_tab("T", &df, &Default::default(), NumberDisplayFormat::StandardCommas, 2, &[], 100);
    let part = &lay.parts[0];
    assert_eq!(part.header[1].lines.len(), 2, "wrapped: {:?}", part.header[1].lines);
    assert_eq!(part.header[1].lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join(" "), "Cumulative gap % of total assets");
    assert!(part.header[2].lines.len() <= 2, "{:?}", part.header[2].lines);
    for h in &part.header {
        for l in &h.lines {
            assert!(l.x >= h.col_x - 1e-3 && l.x + l.w <= h.col_x + h.col_w + 1e-3, "'{}' stays inside its column {:?}", l.text, (h.col_x, h.col_w));
        }
    }
    assert_eq!(part.columns[0].align, PdfAlign::Left);
    assert_eq!(part.columns[1].align, PdfAlign::Right);
    // a short header is on one line and the column is as wide as the values need
    assert_eq!(part.header[3].lines.len(), 1);
    assert!(part.columns[3].w <= MIN_COL_MM + 0.01, "{}", part.columns[3].w);
    // no unit: the table has no scaled column
    assert_eq!(lay.unit_line, None);
}

#[test]
fn i17_the_pdf_file_says_the_unit_once_under_the_title_and_has_no_unit_in_headers_or_cells() {
    use micro_parquet::exporters::*;
    let (df, formats) = pivot_for_pdf(3, PivotScale::Crore);
    let d = dir("i17pdf");
    let p = d.join("pivot.pdf");
    let rep = export_pdf_tabs(vec![PdfTab { title: "Pivot: Segment".into(), df, col_formats: formats, summary: Vec::new(), chart: None }], &p, NumberDisplayFormat::BdtCrore, 2).expect("pdf");
    assert!(rep.omitted_columns.is_empty() && rep.truncated_tabs.is_empty());
    assert!(std::fs::metadata(&p).map(|m| m.len() > 800).unwrap_or(false));
    // the text of the file, when pdftotext is installed
    if let Ok(o) = std::process::Command::new("pdftotext").arg("-layout").arg(&p).arg("-").output() {
        if o.status.success() {
            let text = String::from_utf8_lossy(&o.stdout).to_string();
            assert_eq!(text.matches("Figures in BDT Crore").count(), 1, "{text}");
            assert!(!text.contains("(BDT Cr)") && !text.contains(" Cr\n") && !text.contains("BDT Cr)"), "{text}");
            assert!(!text.lines().any(|l| l.trim_end().ends_with(" Cr")), "no unit after a value: {text}");
            assert!(text.contains("Grand total") && text.contains("Sum of Balance"), "{text}");
            if let Ok(dir) = std::env::var("MP_PDF_OUT") {
                let _ = std::fs::copy(&p, std::path::Path::new(&dir).join("pivot.pdf"));
            }
        }
    }
}
