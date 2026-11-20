//! Phase 3 fixes, part 1: library-level checks (UI-state checks are in `src/ui/tests.rs`).

use micro_parquet::jobs::JobCtx;
use micro_parquet::models::*;
use micro_parquet::tab_jobs::*;
use polars::prelude::*;
use std::path::{Path, PathBuf};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mp_fix_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn parquet(d: &Path, name: &str, mut df: DataFrame) -> TabSource {
    let p = d.join(format!("{name}.parquet"));
    ParquetWriter::new(std::fs::File::create(&p).unwrap()).finish(&mut df).unwrap();
    TabSource::SingleFile { path: p, is_parquet: true, config: IngestionConfig::default() }
}

fn dr_frame() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Branch".into(), &["DHK", "CTG"]).into(),
        Series::new("Amount".into(), &["100", "200 Dr"]).into(),
    ])
    .unwrap()
}

fn to_number() -> TransformStep {
    TransformStep::ConvertToNumber { columns: vec!["Amount".into()] }
}

// ---------------------------------------------------------------------------------- fix 2

#[test]
fn fix2_refresh_reports_a_page_that_cannot_be_read() {
    let d = dir("f2r");
    let req = RefreshRequest {
        tab_id: 1,
        source: parquet(&d, "dr", dr_frame()),
        steps: vec![to_number()],
        offset: 0,
        limit: 100,
        control_col: None,
        recalculate_baseline: false,
        check_join: false,
        added_status: None,
        temp_dir: d.clone(),
        validate: true,
        exec_steps: 1,
    };
    let r = refresh_tab(req, &JobCtx::inert()).unwrap();
    assert!(r.slice.is_none());
    let e = r.page_error.expect("page error");
    assert!(e.contains("Amount"), "{e}");
    assert!(r.error.is_none(), "the pipeline built; only reading failed");
}

#[test]
fn fix2_load_reports_a_first_page_that_cannot_be_read() {
    let d = dir("f2l");
    let req = LoadRequest {
        tab_id: 1,
        title: "Dr".into(),
        source: parquet(&d, "dr", dr_frame()),
        steps: vec![to_number()],
        col_formats: Default::default(),
        temp_dir: d.clone(),
        reconcile_rate_used: None,
    };
    let l = load_tab(req, &JobCtx::inert()).unwrap();
    assert!(l.tab.page_error.as_deref().is_some_and(|e| e.contains("Amount")));
    assert!(l.status.starts_with("Tab created, but its first page could not be read:"), "{}", l.status);
}

// ---------------------------------------------------------------------------------- fix 3

fn filter(value: &str) -> TransformStep {
    TransformStep::FilterRows { column: "Branch".into(), operator: FilterOp::Eq, value: value.into(), keep_matching: true }
}

#[test]
fn fix3_pipeline_hash_sees_settings_on_off_and_source_not_just_the_count() {
    use micro_parquet::steps_edit::{pipeline_hash, toggle_skip};
    let d = dir("f3");
    let src = parquet(&d, "a", dr_frame());
    let base = pipeline_hash(&src, &[filter("DHK")]);
    assert_eq!(base, pipeline_hash(&src, &[filter("DHK")]), "same pipeline, same hash");
    assert_ne!(base, pipeline_hash(&src, &[filter("CTG")]), "same count, other setting");
    let mut off = vec![filter("DHK")];
    toggle_skip(&mut off, 0);
    assert_ne!(base, pipeline_hash(&src, &off), "turned off");
    // another column type in the ingestion settings (the source schema)
    let typed = match &src {
        TabSource::SingleFile { path, is_parquet, config } => {
            let mut config = config.clone();
            config.columns.push(ColumnSchemaConfig { name: "Amount".into(), ..Default::default() });
            TabSource::SingleFile { path: path.clone(), is_parquet: *is_parquet, config }
        }
        _ => unreachable!(),
    };
    assert_ne!(base, pipeline_hash(&typed, &[filter("DHK")]), "schema changed");
    // the file replaced with other content
    let mut df = df_with_three_rows();
    let TabSource::SingleFile { path, .. } = &src else { unreachable!() };
    ParquetWriter::new(std::fs::File::create(path).unwrap()).finish(&mut df).unwrap();
    assert_ne!(base, pipeline_hash(&src, &[filter("DHK")]), "file replaced");
}

fn df_with_three_rows() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Branch".into(), &["DHK", "CTG", "SYL"]).into(),
        Series::new("Amount".into(), &["1", "2", "3"]).into(),
    ])
    .unwrap()
}

// ---------------------------------------------------------------------------------- fix 4

#[test]
fn fix4_a_step_check_that_cannot_run_is_reported() {
    let d = dir("f4");
    let src = parquet(&d, "gone", dr_frame());
    let TabSource::SingleFile { path, .. } = &src else { unreachable!() };
    std::fs::remove_file(path).unwrap();
    let req = RefreshRequest {
        tab_id: 1,
        source: src.clone(),
        steps: vec![filter("DHK")],
        offset: 0,
        limit: 100,
        control_col: Some("Amount".into()),
        recalculate_baseline: false,
        check_join: false,
        added_status: None,
        temp_dir: d.clone(),
        validate: true,
        exec_steps: 1,
    };
    let r = refresh_tab(req, &JobCtx::inert()).unwrap();
    let e = r.error.expect("the failed check is an error, not 'no issues'");
    assert!(e.starts_with(STEP_CHECK_FAILED), "{e}");
    assert!(r.slice.is_none() && r.page_error.is_some());
    assert_eq!(r.totals.rows, None, "rows show — instead of a number");

    let load = LoadRequest {
        tab_id: 2,
        title: "Gone".into(),
        source: src,
        steps: vec![filter("DHK")],
        col_formats: Default::default(),
        temp_dir: d.clone(),
        reconcile_rate_used: None,
    };
    let err = match load_tab(load, &JobCtx::inert()) {
        Ok(_) => panic!("a tab whose steps cannot be checked must not open"),
        Err(e) => format!("{:#}", e),
    };
    assert!(err.starts_with(STEP_CHECK_FAILED), "{err}");
}

// ---------------------------------------------------------------------------------- fix 6

#[test]
fn fix6_row_counts_are_keyed_on_the_source_and_the_step_prefix() {
    use micro_parquet::steps_edit::{count_keys, reusable_counts};
    let d = dir("f6");
    let src = parquet(&d, "a", dr_frame());
    let steps = vec![filter("DHK"), filter("CTG")];
    let keys = count_keys(&src, &steps);
    assert_eq!(keys.len(), 3);
    let counts = vec![Some(2), Some(1), Some(0)];
    assert_eq!(reusable_counts(&keys, &counts, &count_keys(&src, &steps)), counts, "nothing changed");
    // the last step changes: the source and first-step counts stay
    let other = vec![filter("DHK"), filter("SYL")];
    assert_eq!(reusable_counts(&keys, &counts, &count_keys(&src, &other)), vec![Some(2), Some(1), None]);
    // another ingestion setting (same path, same steps): nothing is reused
    let TabSource::SingleFile { path, is_parquet, config } = &src else { unreachable!() };
    let mut cfg = config.clone();
    cfg.skip_rows = 1;
    let src_cfg = TabSource::SingleFile { path: path.clone(), is_parquet: *is_parquet, config: cfg };
    assert_eq!(reusable_counts(&keys, &counts, &count_keys(&src_cfg, &steps)), vec![None, None, None]);
    // another file at another path: nothing is reused
    let src_b = parquet(&d, "b", dr_frame());
    assert_eq!(reusable_counts(&keys, &counts, &count_keys(&src_b, &steps)), vec![None, None, None]);
    // the same path rewritten with more rows (size and modified time change): nothing is reused
    std::thread::sleep(std::time::Duration::from_millis(20));
    let mut df = df_with_three_rows();
    ParquetWriter::new(std::fs::File::create(path).unwrap()).finish(&mut df).unwrap();
    assert_eq!(reusable_counts(&keys, &counts, &count_keys(&src, &steps)), vec![None, None, None]);
}

// ---------------------------------------------------------------------------------- fix 8

fn sort_rows(df: DataFrame, levels: Vec<SortLevel>) -> Vec<Option<String>> {
    let d = dir(&format!("f8_{}", levels.len() + levels.iter().filter(|l| l.descending).count() * 10));
    let src = parquet(&d, "s", df);
    let lf = micro_parquet::engine::compile_tab_lazy(&src, &[TransformStep::SortRows { levels }], &d).unwrap();
    let out = lf.collect().unwrap();
    out.column("Id").unwrap().str().unwrap().into_iter().map(|v| v.map(String::from)).collect()
}

fn branches() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Id".into(), &["a", "b", "c", "d", "e", "f", "g", "h"]).into(),
        Series::new(
            "Branch".into(),
            &[Some("Branch 10"), Some("branch 2"), Some("Branch 2"), Some("BRANCH 1"), Some(""), None, Some("Branch 02"), Some("ctg")],
        )
        .into(),
    ])
    .unwrap()
}

fn ids(v: &[&str]) -> Vec<Option<String>> {
    v.iter().map(|s| Some(s.to_string())).collect()
}

#[test]
fn fix8_text_sorts_case_insensitive_with_numbers_by_value_and_stable_ties() {
    use micro_parquet::engine::natural_sort_key as k;
    assert!(k("Branch 2") < k("Branch 10"), "Branch 2 before Branch 10");
    assert!(k("Branch 9") < k("Branch 10") && k("A1B2") < k("a1b10"));
    assert_eq!(k("CTG"), k("ctg"), "case is ignored");
    assert_eq!(k("Branch 02"), k("Branch 2"), "the value of a number counts, not its zeros");
    assert!(k("DHK") > k("ctg"), "c before D once case is ignored");
    assert!(k("x0") < k("x00001"));

    let asc = sort_rows(branches(), vec![SortLevel { column: "Branch".into(), descending: false, ..Default::default() }]);
    // BRANCH 1; the three 2s in their source order; Branch 10; ctg; blanks last ("" then null)
    assert_eq!(asc, ids(&["d", "b", "c", "g", "a", "h", "e", "f"]));
    let desc = sort_rows(branches(), vec![SortLevel { column: "Branch".into(), descending: true, ..Default::default() }]);
    // ties keep their source order in either direction; blanks are still last
    assert_eq!(desc, ids(&["h", "a", "b", "c", "g", "d", "e", "f"]));
}

#[test]
fn fix8_python_sort_key_matches_the_app() {
    use micro_parquet::macros::generate_python_polars_code;
    let step = TransformStep::SortRows { levels: vec![SortLevel { column: "Branch".into(), descending: false, ..Default::default() }] };
    let py = generate_python_polars_code(Path::new("x.parquet"), true, &IngestionConfig::default(), &[step], None).unwrap();
    assert!(py.contains("_mp_sort_key(pl.col(\"Branch\")"), "{py}");
    assert!(py.contains("nulls_last=True, maintain_order=True"));
    // run the helper (it needs no Polars) and compare its keys with the app's
    let start = py.find("def _mp_natural_key").unwrap();
    let end = py.find("def _mp_sort_key").unwrap();
    let samples = ["Branch 10", "branch 2", "Branch 02", "CTG", "A1B10", "x0", "Dhaka-007 Main", "Ünïcode 3"];
    let script = format!(
        "import re as _re\n{}\nimport json, sys\nprint(json.dumps([_mp_natural_key(s) for s in json.loads(sys.argv[1])]))\n",
        &py[start..end]
    );
    let out = match std::process::Command::new("python3").arg("-c").arg(&script).arg(serde_json::to_string(&samples).unwrap()).output() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("python3 is not available ({e}); the Python key was not compared");
            return;
        }
    };
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let got: Vec<String> = serde_json::from_slice(&out.stdout).unwrap();
    let want: Vec<String> = samples.iter().map(|s| micro_parquet::engine::natural_sort_key(s)).collect();
    assert_eq!(got, want);
}

// ---------------------------------------------------------------------------------- fix 9

#[test]
fn fix9_rate_like_columns_default_to_a_weighted_average_by_the_control_column() {
    use micro_parquet::summary::*;
    let f = DataType::Float64;
    let wavg = |w: &str| SummarySpec { agg: SummaryAgg::WeightedAvg, weight_col: Some(w.into()) };
    for name in ["Int_Rate", "IntRate", "Yield %", "SpreadBps", "Pct_Change", "Coupon Percent", "rate"] {
        assert_eq!(default_spec_for(&f, name, Some("Balance")), wavg("Balance"), "{name}");
        assert_eq!(default_spec_for(&f, name, None).agg, SummaryAgg::None, "{name} without a control column");
        assert_eq!(default_agg_for(&f, name), SummaryAgg::None);
    }
    // not rate-like: the word only inside another word, or not a number
    assert_eq!(default_spec_for(&f, "Corporate_Exposure", Some("Balance")).agg, SummaryAgg::Sum);
    assert_eq!(default_spec_for(&f, "Balance", Some("Balance")).agg, SummaryAgg::Sum);
    assert_eq!(default_spec_for(&DataType::String, "Rate_Type", Some("Balance")).agg, SummaryAgg::Count);
    // a rate column that is itself the control column has no summary
    assert_eq!(default_spec_for(&f, "Rate", Some("Rate")).agg, SummaryAgg::None);
    // the user's choice still wins
    let mut over = std::collections::HashMap::new();
    over.insert("Int_Rate".to_string(), SummarySpec { agg: SummaryAgg::Average, weight_col: None });
    let cols = vec![("Balance".to_string(), f.clone()), ("Int_Rate".to_string(), f.clone())];
    assert_eq!(effective_specs(&cols, &over, Some("Balance"))[1].1.agg, SummaryAgg::Average);

    // the value: Σ(balance × rate) / Σ balance
    let df = DataFrame::new(vec![
        Series::new("Balance".into(), &[100.0f64, 300.0]).into(),
        Series::new("Int_Rate".into(), &[8.0f64, 12.0]).into(),
    ])
    .unwrap();
    let cells = compute_summary(df.clone().lazy(), &effective_specs(&cols, &Default::default(), Some("Balance"))).unwrap();
    let rate = cells.iter().find(|c| c.column == "Int_Rate").unwrap();
    assert_eq!(rate.value, Some(SummaryValue::Number(11.0)));
    let cells = compute_summary(df.lazy(), &effective_specs(&cols, &Default::default(), None)).unwrap();
    let rate = cells.iter().find(|c| c.column == "Int_Rate").unwrap();
    assert_eq!((rate.agg, &rate.value), (SummaryAgg::None, &None));
}

#[test]
fn fix9_summary_job_uses_the_control_column_weight() {
    let d = dir("f9");
    let df = DataFrame::new(vec![
        Series::new("Balance".into(), &[100.0f64, 300.0]).into(),
        Series::new("Int_Rate".into(), &[8.0f64, 12.0]).into(),
    ])
    .unwrap();
    let src = parquet(&d, "r", df);
    let r = compute_tab_summary(
        SummaryRequest { tab_id: 1, source: src.clone(), steps: vec![], overrides: Default::default(), control_col: Some("Balance".into()), temp_dir: d.clone() },
        &JobCtx::inert(),
    )
    .unwrap();
    let cell = r.cells.iter().find(|c| c.column == "Int_Rate").unwrap();
    assert_eq!((cell.agg, cell.weight_col.as_deref()), (micro_parquet::models::SummaryAgg::WeightedAvg, Some("Balance")));
    assert_eq!(cell.value, Some(micro_parquet::summary::SummaryValue::Number(11.0)));
}

// ---------------------------------------------------------------------------------- fix 11

fn unzip_entry(zip: &Path, entry: &str) -> String {
    let out = std::process::Command::new("unzip").arg("-p").arg(zip).arg(entry).output().expect("unzip must be installed");
    assert!(out.status.success(), "unzip {entry}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn loans_tab(d: &Path) -> ExportTab {
    let df = DataFrame::new(vec![
        Series::new("Branch".into(), &["DHK", "CTG", "SYL"]).into(),
        Series::new("Balance".into(), &[10_000_000.0f64, 20_000_000.0, 30_000_000.0]).into(),
    ])
    .unwrap();
    ExportTab {
        title: "Loans".into(),
        source: parquet(d, "loans", df),
        steps: vec![],
        col_formats: Default::default(),
        summary: Default::default(),
        control_col: Some("Balance".into()),
        chart: None,
    }
}

#[test]
fn fix11_totals_row_follows_the_checkbox_and_never_reaches_csv_or_parquet() {
    let d = dir("f11");
    let tab = loans_tab(&d);
    let ctx = JobCtx::inert();
    // Excel: off -> no labels row; on -> "Sum" / "Count" labels and the summary colour
    let off = d.join("off.xlsx");
    let on = d.join("on.xlsx");
    export_tabs_xlsx(std::slice::from_ref(&tab), &d, &off, NumberDisplayFormat::StandardCommas, 2, false, None, &ctx).unwrap();
    export_tabs_xlsx(std::slice::from_ref(&tab), &d, &on, NumberDisplayFormat::StandardCommas, 2, true, None, &ctx).unwrap();
    let (s_off, s_on) = (unzip_entry(&off, "xl/sharedStrings.xml"), unzip_entry(&on, "xl/sharedStrings.xml"));
    assert!(!s_off.contains(">Sum<") && !s_off.contains(">Count<"), "{s_off}");
    assert!(s_on.contains(">Sum<") && s_on.contains(">Count<"), "{s_on}");
    let sheet_off = unzip_entry(&off, "xl/worksheets/sheet1.xml");
    assert!(!sheet_off.contains("r=\"A6\""), "header + 3 rows only: {sheet_off}");
    assert!(unzip_entry(&on, "xl/worksheets/sheet1.xml").contains("r=\"B6\""), "labels row 5, values row 6");
    // PDF: the band adds drawing and text
    let (p_off, p_on) = (d.join("off.pdf"), d.join("on.pdf"));
    export_tabs_pdf(std::slice::from_ref(&tab), &d, &p_off, NumberDisplayFormat::BdtCrore, 2, false, None, &ctx).unwrap();
    export_tabs_pdf(std::slice::from_ref(&tab), &d, &p_on, NumberDisplayFormat::BdtCrore, 2, true, None, &ctx).unwrap();
    let (a, b) = (std::fs::metadata(&p_off).unwrap().len(), std::fs::metadata(&p_on).unwrap().len());
    assert!(b > a, "{b} vs {a}");
    // CSV and Parquet: data rows only, whatever the summary choices
    let csv = d.join("out.csv");
    export_tab_csv(&tab.source, &[], &d, &csv, false, None, &ctx).unwrap();
    let text = std::fs::read_to_string(&csv).unwrap();
    assert_eq!(text.lines().count(), 4, "{text}");
    let pq = d.join("out.parquet");
    export_tab_parquet(&tab.source, &[], &d, &pq, ParquetDialect::ArchiveZstd, None, &ctx).unwrap();
    let back = LazyFrame::scan_parquet(&pq, Default::default()).unwrap().collect().unwrap();
    assert_eq!(back.height(), 3);
}

// ---------------------------------------------------------------------------------- fix 12

/// The numbers in a sheet's column (`col` letter), rows `rows`, as written.
fn column_values(sheet: &str, col: &str, rows: std::ops::RangeInclusive<usize>) -> Vec<f64> {
    rows.map(|r| {
        let tag = format!("r=\"{col}{r}\"");
        let at = sheet.find(&tag).unwrap_or_else(|| panic!("no cell {col}{r}"));
        let rest = &sheet[at..];
        let end = rest.find("</c>").unwrap();
        let c = &rest[..end];
        let v0 = c.find("<v>").unwrap() + 3;
        let v1 = c[v0..].find("</v>").unwrap() + v0;
        c[v0..v1].parse::<f64>().unwrap()
    })
    .collect()
}

#[test]
fn fix12_excel_thousands_and_millions_by_format_lakh_and_crore_divided_and_sums_agree() {
    use micro_parquet::exporters::*;
    use micro_parquet::summary::*;
    let d = dir("f12");
    let raw = [12_345_678.9f64, -2_500_000.0, 987_654_321.0, 1.0];
    let df = DataFrame::new(vec![
        Series::new("Branch".into(), &["DHK", "CTG", "SYL", "KHL"]).into(),
        Series::new("Balance".into(), &raw).into(),
    ])
    .unwrap();
    let cols: Vec<(String, DataType)> = df.get_columns().iter().map(|s| (s.name().to_string(), s.dtype().clone())).collect();
    let cells = compute_summary(df.clone().lazy(), &effective_specs(&cols, &Default::default(), None)).unwrap();
    let raw_total: f64 = raw.iter().sum();
    for (fmt, unit, divide, code) in [
        (NumberDisplayFormat::BdtThousand, "BDT K", 1.0, "#,##0.00,"),
        (NumberDisplayFormat::BdtMillion, "BDT M", 1.0, "#,##0.00,,"),
        (NumberDisplayFormat::BdtLakh, "BDT L", 100_000.0, "#,##0.00"),
        (NumberDisplayFormat::BdtCrore, "BDT Cr", 10_000_000.0, "#,##0.00"),
    ] {
        let p = d.join(format!("{unit}.xlsx").replace(' ', "_"));
        let tab = XlsxTab { title: "Loans".into(), df: df.clone(), col_formats: Default::default(), summary: cells.clone() };
        write_xlsx_tabs(vec![tab], vec![], &p, 1000, Some(XlsxScale { num_format: fmt, decimals: 2 })).unwrap();
        let strings = unzip_entry(&p, "xl/sharedStrings.xml");
        let sheet = unzip_entry(&p, "xl/worksheets/sheet1.xml");
        let styles = unzip_entry(&p, "xl/styles.xml");
        assert!(strings.contains(&format!("Balance ({unit})")), "{unit}: {strings}");
        // the cells: raw for K and M, divided for Lakh and Crore
        let cells_b = column_values(&sheet, "B", 2..=5);
        for (got, r) in cells_b.iter().zip(raw) {
            assert!((got - r / divide).abs() <= 1e-9 * (r / divide).abs().max(1.0), "{unit}: {got} vs {}", r / divide);
        }
        // the number format scales K and M (",", ",,"); Lakh and Crore use the plain one
        assert!(styles.contains(&format!("formatCode=\"{code}\"")), "{unit}: {styles}");
        if divide != 1.0 {
            assert!(!styles.contains("#,##0.00,"), "{unit}: no scaling format for divided values");
        }
        // SUM consistency: Excel's SUM of the cells equals the totals row (blank row 6, labels 7, values 8)
        let excel_sum: f64 = cells_b.iter().sum();
        let totals = column_values(&sheet, "B", 8..=8)[0];
        assert!((excel_sum - totals).abs() <= 1e-9 * totals.abs(), "{unit}: SUM {excel_sum} vs totals {totals}");
        assert!((totals * divide - raw_total).abs() <= 1e-6, "{unit}: the totals row is the raw total in the column's unit");
    }
    // CSV and Parquet keep raw values whatever the screen scale
    let src = parquet(&d, "raw", df.clone());
    let csv = d.join("raw.csv");
    export_tab_csv(&src, &[], &d, &csv, false, None, &JobCtx::inert()).unwrap();
    assert!(std::fs::read_to_string(&csv).unwrap().contains("987654321"));
    let pq = d.join("raw_out.parquet");
    export_tab_parquet(&src, &[], &d, &pq, ParquetDialect::ArchiveZstd, None, &JobCtx::inert()).unwrap();
    let back = LazyFrame::scan_parquet(&pq, Default::default()).unwrap().collect().unwrap();
    assert_eq!(back.column("Balance").unwrap().f64().unwrap().get(2), Some(987_654_321.0));
}

// ---------------------------------------------------------------------------------- fix 13

/// Macros saved by the Phase 1 (c6e780d) and Phase 2 (4278ef5) merge-commit code, each with
/// every step type, and the result that code gave on the fixture frame (see tests/fixtures).
const OLD_MACROS: [(&str, &str); 2] = [
    ("phase1_every_step.mpq.json", "phase1_every_step.expected.csv"),
    ("phase2_every_step.mpq.json", "phase2_every_step.expected.csv"),
];

#[test]
fn fix13_genuine_old_macros_load_and_replay_to_the_results_their_code_gave() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures");
    let kinds = [
        "SelectAndReorderColumns", "RenameColumn", "FilterRows", "ConditionalColumn", "MathFormula",
        "NormalizeEntity", "MaturityBucket", "RateShockSim", "FtpCurveMap", "BankingHygieneAudit",
        "DurationAnalysis", "LoanClassificationProvision", "LoanLimitUtilization", "GroupByPivot", "SqlQuery",
    ];
    for (json, expected) in OLD_MACROS {
        let recipe = micro_parquet::macros::load_macro_from_file(&fixtures.join(json)).unwrap_or_else(|e| panic!("{json}: {e:#}"));
        assert_eq!(recipe.steps.len(), kinds.len(), "{json}");
        for k in kinds {
            assert!(recipe.steps.iter().any(|s| format!("{:?}", s).starts_with(&format!("{k} "))), "{json} has a {k} step");
        }
        // old fields are filled by their defaults: Phase 1 FTP steps keep the interpolation they had
        let ftp = recipe.steps.iter().find_map(|s| match s {
            TransformStep::FtpCurveMap { interpolation, .. } => Some(*interpolation),
            _ => None,
        });
        let want = if json.starts_with("phase1") { FtpInterpolation::NextPointUp } else { FtpInterpolation::Linear };
        assert_eq!(ftp, Some(want), "{json}");
        let d = dir(&format!("f13_{}", &json[..6]));
        let src = parquet(&d, "fixture_loans", fixture_frame());
        let issues = micro_parquet::engine::validate_steps(&src, &recipe.steps, &d).unwrap();
        assert!(issues.is_empty(), "{json}: {issues:?}");
        let got = replay_to_csv(&recipe.steps, &d);
        // a Windows checkout may turn the file's line ends into CRLF
        let want = std::fs::read_to_string(fixtures.join(expected)).unwrap().replace("\r\n", "\n");
        assert_eq!(got, want, "{json}: today's code gives what the code that saved it gave");
    }
}
/// The synthetic loan book the fixtures are replayed on (the same in every version's test).
pub fn fixture_frame() -> polars::prelude::DataFrame {
    use polars::prelude::*;
    let day = |s: &str| (chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap() - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days() as i32;
    let dates = |v: &[Option<&str>]| -> Series {
        let days: Vec<Option<i32>> = v.iter().map(|d| d.map(day)).collect();
        Series::new("d".into(), days).cast(&DataType::Date).unwrap()
    };
    let mut opn = dates(&[Some("2024-01-15"), Some("2025-03-01"), Some("2023-07-10"), Some("2025-11-20"), Some("2022-02-28"), Some("2026-01-05"), Some("2024-09-09"), Some("2025-06-30")]);
    opn.rename("Opndat".into());
    let mut mat = dates(&[Some("2026-07-15"), Some("2026-09-30"), None, Some("2027-12-31"), Some("2026-05-31"), Some("2031-01-05"), Some("2026-08-01"), Some("2028-06-30")]);
    mat.rename("Matdat".into());
    let mut cls = dates(&[None, None, None, None, Some("2026-05-31"), None, None, None]);
    cls.rename("Clsdat".into());
    DataFrame::new(vec![
        Series::new("Actnum".into(), &["1001", "1002", "1003", "1004", "1005", "1006", "1007", "1008"]).into(),
        Series::new("Brancd".into(), &["DHK", "DHK", "CTG", "CTG", "SYL", "DHK", "XXX", "CTG"]).into(),
        Series::new("Acttit".into(), &["Rahim Traders Ltd.", "RAHIM TRADERS LIMITED", "Karim & Sons", "Karim and Sons", "Delta Agro", "Bay Shipping", "Test", "Delta Agro Ltd"]).into(),
        Series::new("Balance".into(), &[50_000_000.0f64, 25_000_000.0, 120_000_000.0, 8_000_000.0, 0.0, 300_000_000.0, 1.0, 45_000_000.0]).into(),
        Series::new("Int Rate".into(), &[9.5f64, 10.25, 11.0, 8.75, 12.0, 7.5, 9.0, 10.5]).into(),
        opn.into(),
        mat.into(),
        cls.into(),
        Series::new("Acstat".into(), &["ACT", "DMT", "ACT", "ACT", "CLS", "ACT", "ACT", "DMT"]).into(),
        Series::new("CL_Status".into(), &["UC", "SMA", "UC", "SS", "UC", "UC", "UC", "BL"]).into(),
        Series::new("Sector".into(), &["SMALL", "MICRO", "OTHER", "SMALL", "AGRI ST", "OTHER", "OTHER", "COTTAGE"]).into(),
        Series::new("Sanctioned_Limit".into(), &[60_000_000.0f64, 25_000_000.0, 150_000_000.0, 10_000_000.0, 5_000_000.0, 300_000_000.0, 1.0, 50_000_000.0]).into(),
        Series::new("Side".into(), &["Asset", "Asset", "Asset", "Liability", "Asset", "Asset", "Asset", "Asset"]).into(),
    ])
    .unwrap()
}

/// Replays `steps` on the fixture frame and returns the result as CSV text.
pub fn replay_to_csv(steps: &[micro_parquet::models::TransformStep], dir: &std::path::Path) -> String {
    use polars::prelude::*;
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.join("fixture_loans.parquet");
    let mut df = fixture_frame();
    ParquetWriter::new(std::fs::File::create(&p).unwrap()).finish(&mut df).unwrap();
    let src = micro_parquet::models::TabSource::SingleFile { path: p, is_parquet: true, config: Default::default() };
    let mut out = micro_parquet::engine::compile_tab_lazy(&src, steps, dir).unwrap().collect().unwrap();
    let mut buf = Vec::new();
    CsvWriter::new(&mut buf).finish(&mut out).unwrap();
    String::from_utf8(buf).unwrap()
}
