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

    let asc = sort_rows(branches(), vec![SortLevel { column: "Branch".into(), descending: false }]);
    // BRANCH 1; the three 2s in their source order; Branch 10; ctg; blanks last ("" then null)
    assert_eq!(asc, ids(&["d", "b", "c", "g", "a", "h", "e", "f"]));
    let desc = sort_rows(branches(), vec![SortLevel { column: "Branch".into(), descending: true }]);
    // ties keep their source order in either direction; blanks are still last
    assert_eq!(desc, ids(&["h", "a", "b", "c", "g", "d", "e", "f"]));
}

#[test]
fn fix8_python_sort_key_matches_the_app() {
    use micro_parquet::macros::generate_python_polars_code;
    let step = TransformStep::SortRows { levels: vec![SortLevel { column: "Branch".into(), descending: false }] };
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
