//! Phase 6 guide notes: one `docs/guide/<tool>.md` per new tool, checked like the Phase 4 and 5 ones
//! (ten sections, proven worked examples). `MP_GUIDE_DUMP=<folder> cargo test guide_` writes the computed tables.

use crate::common::*;
use crate::guide::{check_guide, check_guide_file, read_guide};
use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::models::*;
use micro_parquet::sort_lists::SortOrder;
use polars::prelude::*;

/// The tool ids of Phase 6 (file names in docs/guide/), added as each task is done.
pub const P6_GUIDE_TOOLS: [&str; 8] = ["custom-sort-lists", "sql-editor", "chart", "saved-tool-runs", "report-packs", "mom-variance", "data-quality-scorecard", "mudaraba-profit-distribution"];

fn all_test_sources() -> String {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut all = String::new();
    let mut stack = vec![root];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).expect("read the tests folder").flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().map(|x| x == "rs").unwrap_or(false) {
                all.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
            }
        }
    }
    all
}

#[test]
fn every_phase6_tool_has_a_complete_guide_with_proven_examples_listed_in_the_readme() {
    let sources = all_test_sources();
    let readme = read_guide("README");
    for tool in P6_GUIDE_TOOLS {
        check_guide_file(tool, &sources);
        assert!(sources.contains(&format!("fn guide_{}(", tool.replace('-', "_"))), "{tool}: there is no test guide_{}", tool.replace('-', "_"));
        assert!(readme.contains(&format!("({tool}.md)")), "README: no link to {tool}.md");
    }
}

#[test]
fn guide_custom_sort_lists() {
    let df = DataFrame::new(vec![
        Series::new("Id".into(), &["T1", "T2", "T3", "T4", "T5", "T6"]),
        Series::new("Term Period".into(), &["3y", "6m", "100D", "1y", "abc", "1m"]),
        Series::new("Risk".into(), &["Low", "High", "Medium", "Other", "Low", "High"]),
    ])
    .expect("frame");
    let level = |c: &str, descending: bool, order: SortOrder| SortLevel { column: c.into(), descending, order };
    let by_tenor = run_steps("g_tenor", &df, &[TransformStep::SortRows { levels: vec![level("Term Period", false, SortOrder::Tenor)] }]).expect("tenor sort");
    assert_eq!(strings(&by_tenor, "Term Period"), vec!["1m", "100D", "6m", "1y", "3y", "abc"].into_iter().map(|s| Some(s.to_string())).collect::<Vec<_>>(), "months and days by length, abc last");
    let list = SortOrder::List { name: "Risk".into(), values: vec!["High".into(), "Medium".into(), "Low".into()], case_insensitive: true };
    let by_list = run_steps("g_list", &df, &[TransformStep::SortRows { levels: vec![level("Risk", false, list)] }]).expect("list sort");
    assert_eq!(strings(&by_list, "Risk"), vec!["High", "High", "Medium", "Low", "Low", "Other"].into_iter().map(|s| Some(s.to_string())).collect::<Vec<_>>(), "listed values in list order, Other after them");
    check_guide(
        "custom-sort-lists",
        "guide_custom_sort_lists",
        &[md(&df, &["Id", "Term Period", "Risk"]), md(&by_tenor, &["Id", "Term Period"]), md(&by_list, &["Id", "Risk"])],
    );
}

#[test]
fn guide_sql_editor() {
    let df = DataFrame::new(vec![
        Series::new("Branch".into(), &["DHK", "CTG", "DHK", "SYL", "CTG"]),
        Series::new("Product".into(), &["TD", "SB", "SB", "TD", "TD"]),
        Series::new("Balance".into(), &[500.0, 120.0, 80.0, 40.0, 300.0]),
    ])
    .expect("frame");
    let query = "SELECT Branch, SUM(Balance) AS Total, COUNT(*) AS Accounts\nFROM self\nWHERE Product = 'TD'\nGROUP BY Branch\nORDER BY Total DESC";
    let out = run_steps("g_sql", &df, &[TransformStep::SqlQuery { query: query.to_string() }]).expect("the query runs");
    assert_eq!(strings(&out, "Branch"), vec![Some("DHK".to_string()), Some("CTG".to_string()), Some("SYL".to_string())]);
    assert_eq!(floats(&out, "Total"), vec![Some(500.0), Some(300.0), Some(40.0)]);
    let bad = run_steps("g_sql_bad", &df, &[TransformStep::SqlQuery { query: "SELECT Missing FROM self".to_string() }]);
    assert!(bad.is_err(), "a column that is not in the table is an error, not a blank");
    check_guide("sql-editor", "guide_sql_editor", &[md(&df, &["Branch", "Product", "Balance:2"]), md(&out, &["Branch", "Total:2", "Accounts"])]);
}

#[test]
fn guide_chart() {
    use micro_parquet::chart::*;
    let df = DataFrame::new(vec![
        Series::new("Term Period".into(), &["6m", "1y", "1m", "100D", "3m", "1y", "6m", "1m"]),
        Series::new("Balance".into(), &[500_000_000.0, 300_000_000.0, 100_000_000.0, 50_000_000.0, 200_000_000.0, 100_000_000.0, 250_000_000.0, 80_000_000.0]),
    ])
    .expect("frame");
    let spec = ChartSpec { category: "Term Period".into(), value: Some("Balance".into()), order: SortOrder::Tenor, top_n: 3, ..Default::default() };
    let data = compute_chart(df.clone().lazy(), &spec).expect("the chart");
    assert_eq!(data.categories, vec!["3m", "6m", "1y", "Other"], "the three largest in tenor order, then Other");
    let table = chart_table(&data).expect("table");
    let total: f64 = floats(&df, "Balance").iter().flatten().sum();
    assert!((floats(&table, "Sum of Balance").iter().flatten().sum::<f64>() - total).abs() < 1e-6, "Other keeps the total");
    check_guide("chart", "guide_chart", &[md(&df, &["Term Period", "Balance:2"]), md(&table, &["Term Period", "Sum of Balance:2", "Rows"])]);
}

#[test]
fn guide_saved_tool_runs() {
    use micro_parquet::tool_runs::*;
    struct Env {
        tabs: Vec<(String, RunTab)>,
        paths: PortablePaths,
    }
    impl RunEnv for Env {
        fn tab(&self, name: &str) -> anyhow::Result<RunTab> {
            self.tabs.iter().find(|(n, _)| n == name).map(|(_, t)| t.clone()).ok_or_else(|| anyhow::anyhow!("The tab '{name}' is not open."))
        }
        fn name_of(&self, source: &TabSource, steps: &[TransformStep]) -> anyhow::Result<String> {
            let key = |s: &TabSource, st: &[TransformStep]| format!("{:?}|{:?}", s, st);
            self.tabs.iter().find(|(_, t)| key(&t.source, &t.steps) == key(source, steps)).map(|(n, _)| n.clone()).ok_or_else(|| anyhow::anyhow!("no tab"))
        }
        fn paths(&self) -> &PortablePaths {
            &self.paths
        }
    }
    let d = dir("g_runs");
    let left = DataFrame::new(vec![Series::new("ACNO".into(), &["A1", "A2", "A3"]), Series::new("BAL".into(), &[1.0, 2.0, 3.0])]).expect("left");
    let right = DataFrame::new(vec![Series::new("ACNO".into(), &["A1", "A3", "A3"]), Series::new("Grade".into(), &["STD", "SS", "SS"])]).expect("right");
    let (l, r) = (parquet(&d, "loans", &left), parquet(&d, "grades", &right));
    let paths = PortablePaths { root_dir: d.clone(), macros_dir: d.join("m"), schemas_dir: d.join("s"), curves_dir: d.join("c"), buckets_file: d.join("b.json"), temp_dir: d.join("t") };
    let env = Env { tabs: vec![("Loans".into(), RunTab { source: l.clone(), steps: vec![] }), ("Grades".into(), RunTab { source: r.clone(), steps: vec![] })], paths };
    let join = TabSource::JoinedTabs { left_source: Box::new(l), left_steps: vec![], right_source: Box::new(r), right_steps: vec![], join_kind: SqlJoinKind::Left, join_keys: vec!["ACNO".into()], right_cols_to_keep: vec![], keep_first_duplicate_right: true };
    let first = compile_tab_lazy(&join, &[], &d).expect("compiles").collect().expect("collects");
    let run = ToolRun::from_source(&join, &[], &env).expect("the run");
    save_run(&d.join("runs"), "monthly join", &run).expect("saves");
    let loaded = load_run(&d.join("runs"), "monthly join").expect("loads");
    let (_, again_src, again_steps) = loaded.run.build(&env).expect("builds").remove(0);
    let again = compile_tab_lazy(&again_src, &again_steps, &d).expect("compiles").collect().expect("collects");
    assert!(first.equals_missing(&again), "the made-again tab is the same");
    check_guide("saved-tool-runs", "guide_saved_tool_runs", &[md(&left, &["ACNO", "BAL:2"]), md(&right, &["ACNO", "Grade"]), md(&again, &["ACNO", "BAL:2", "Grade"])]);
}

#[test]
fn guide_report_packs() {
    use micro_parquet::jobs::JobCtx;
    use micro_parquet::packs::*;
    let d = dir("g_packs");
    micro_parquet::rules::set_data_dir_override(Some(d.clone()));
    let paths = PortablePaths { root_dir: d.clone(), macros_dir: d.join("m"), schemas_dir: d.join("s"), curves_dir: d.join("c"), buckets_file: d.join("b.json"), temp_dir: d.join("t") };
    std::fs::create_dir_all(&paths.temp_dir).expect("temp");
    let file = d.join("deposits.csv");
    std::fs::write(&file, "ACNO,BAL,PROD\nD1,100,SB\nD2,50,TD\nD3,25,TD\n").expect("csv");
    let deposits = DataFrame::new(vec![Series::new("ACNO".into(), &["D1", "D2", "D3"]), Series::new("BAL".into(), &[100.0, 50.0, 25.0]), Series::new("PROD".into(), &["SB", "TD", "TD"])]).expect("frame");
    let pivot = TransformStep::from_pivot_spec(PivotSpec { rows: vec!["PROD".into()], values: vec![PivotValue { field: "BAL".into(), agg: PivotAgg::Sum, weight_col: None, time: None }], row_totals: true, column_totals: true, ..Default::default() });
    let pack = ReportPack {
        name: "mini".into(),
        inputs: vec![PackInput { slot: "Deposits".into(), required_columns: vec!["ACNO".into(), "BAL".into(), "PROD".into()], ..Default::default() }],
        items: vec![PackItem { title: "By product".into(), sheet: "By product".into(), source: PackItemKind::Pivot { slot: "Deposits".into(), pivot: Some(pivot) }, col_formats: Default::default(), chart: None }],
        output: PackOutput::default(),
    };
    let spec = PackRunSpec { files: std::collections::BTreeMap::from([("Deposits".to_string(), file)]), cutoff: "2026-10-31".into() };
    let r = run_pack(&pack, &spec, &paths, "guide", &JobCtx::inert()).expect("the pack runs");
    micro_parquet::rules::set_data_dir_override(None);
    // the Cover's input line (the hash is the file's blake3 and is not copied into the guide)
    let section = strings(&r.cover, "Section");
    let keep: Vec<bool> = section.iter().map(|s| s.as_deref() == Some("Input")).collect();
    let mask = BooleanChunked::from_slice("m".into(), &keep);
    let input_rows = r.cover.filter(&mask).expect("filter");
    let sheet = &r.sheets[0].df;
    assert_eq!(strings(sheet, "PROD"), vec![Some("SB".to_string()), Some("TD".to_string()), Some("Grand total".to_string())]);
    check_guide("report-packs", "guide_report_packs", &[md(&deposits, &["ACNO", "BAL:2", "PROD"]), md(&input_rows, &["Name", "Detail", "Rows"]), md(sheet, &["PROD", "Sum of BAL:2"])]);
}

#[test]
fn guide_mom_variance() {
    use micro_parquet::mom::*;
    const CR: f64 = 10_000_000.0;
    let month = |rows: &[(&str, f64, f64)]| {
        DataFrame::new(vec![
            Series::new("Product".into(), rows.iter().map(|r| r.0).collect::<Vec<_>>()),
            Series::new("Balance".into(), rows.iter().map(|r| r.1 * CR).collect::<Vec<_>>()),
            Series::new("Rate".into(), rows.iter().map(|r| r.2).collect::<Vec<_>>()),
        ])
        .expect("month")
    };
    let (prev, curr) = (month(&[("TD", 1000.0, 8.0), ("SB", 500.0, 3.0)]), month(&[("TD", 1100.0, 8.5), ("SB", 450.0, 3.0)]));
    let spec = MomSpec { key_cols: vec!["Product".into()], bal_col: "Balance".into(), rate_col: Some("Rate".into()), ..Default::default() };
    let var = build_mom(prev.clone().lazy(), curr.clone().lazy(), &spec).expect("variance");
    // the guide shows the money in Crore, as the sheet does
    let mut in_cr = var.clone();
    for c in ["Change", "Annual interest change", "Volume effect", "Rate effect", "Mix effect"] {
        let v: Vec<Option<f64>> = floats(&var, c).into_iter().map(|x| x.map(|y| y / CR)).collect();
        in_cr.with_column(Series::new(c.into(), v)).expect("scaled");
    }
    let lines = mom_commentary(&var, &spec).expect("commentary");
    let text = commentary_frame(&lines).expect("text");
    let cr = |df: &DataFrame| {
        let mut d = df.clone();
        let v: Vec<Option<f64>> = floats(df, "Balance").into_iter().map(|x| x.map(|y| y / CR)).collect();
        d.with_column(Series::new("Balance".into(), v)).expect("scaled");
        d
    };
    check_guide(
        "mom-variance",
        "guide_mom_variance",
        &[
            md(&cr(&prev), &["Product", "Balance:2", "Rate:2"]),
            md(&cr(&curr), &["Product", "Balance:2", "Rate:2"]),
            md(&in_cr, &["Product", "Change:2", "Change %:2", "Rate change (bp):2", "Annual interest change:2", "Volume effect:2", "Rate effect:2", "Mix effect:2"]),
            md(&text, &["Commentary"]),
        ],
    );
}

#[test]
fn guide_data_quality_scorecard() {
    use micro_parquet::dq::*;
    use micro_parquet::engine::*;
    let d = dir("g_dq");
    let p = d.join("book.csv");
    std::fs::write(&p, "Account_No,Rate,Open_Date,Maturity,Balance\nA01,9.5,2020-01-01,2025-01-01,100\nA02,8.0,2020-01-01,2025-01-01,100\nA03,abc,2020-01-01,2025-01-01,100\nA04,xyz,2020-01-01,2025-01-01,100\nA05,45,2020-01-01,2025-01-01,100\nA06,7.0,2020-01-01,2019-12-31,100\nA07,6.0,2020-01-01,2025-01-01,100\nA07,6.5,2020-01-01,2025-01-01,100\n,5.5,2020-01-01,2025-01-01,100\nA10,4.0,2020-01-01,2025-01-01,100\n").expect("csv");
    let mut cfg = auto_detect_smart_schema(&p).expect("schema");
    for c in cfg.columns.iter_mut() {
        c.dtype = match c.name.as_str() {
            "Account_No" => ColDataType::Utf8,
            "Rate" | "Balance" => ColDataType::Float64,
            _ => ColDataType::DateISO,
        };
    }
    let src = TabSource::SingleFile { path: p, is_parquet: false, config: cfg };
    let df = compile_tab_lazy(&src, &[], &d).expect("compiles").collect().expect("collects");
    let r = |name: &str, kind: DqKind| DqRule { name: name.into(), kind };
    let set = DqRuleSet {
        name: "guide".into(),
        rules: vec![
            r("Completeness", DqKind::Completeness { column: "Account_No".into() }),
            r("Uniqueness", DqKind::Uniqueness { columns: vec!["Account_No".into()] }),
            r("Type", DqKind::TypeValidity { column: "Rate".into() }),
            r("Range", DqKind::Range { column: "Rate".into(), min: Some(0.0), max: Some(30.0) }),
            r("Consistency", DqKind::Consistency { left: "Maturity".into(), op: DqCompare::Ge, right: "Open_Date".into() }),
            r("GL control", DqKind::ControlTotal { column: "Balance".into(), gl_total: 1005.0, tolerance_pct: 0.1 }),
        ],
        ..Default::default()
    };
    let types = dq_type_stats(&src, &set, &d).expect("types");
    let res = run_scorecard(&df, &types, &set, &[]).expect("scorecard");
    let table = scorecard_frame(&res).expect("table");
    check_guide("data-quality-scorecard", "guide_data_quality_scorecard", &[md(&df, &["Account_No", "Rate:2", "Open_Date", "Maturity", "Balance:2"]), md(&table, &["Rule", "Rows checked", "Rows failing", "Pass %:2", "Result"])]);
}

#[test]
fn guide_mudaraba_profit_distribution() {
    use micro_parquet::islamic::*;
    let d = dir("g_mud");
    micro_parquet::rules::set_data_dir_override(Some(d.clone()));
    let deposits = DataFrame::new(vec![Series::new("Product".into(), &["MSND", "MSB", "MTDR-1Y"]), Series::new("Avg_Balance".into(), &[200.0, 300.0, 500.0])]).expect("deposits");
    let spec = MudarabaSpec { product_col: "Product".into(), balance_col: "Avg_Balance".into(), daily: false, date_col: String::new(), zero_fill_missing_days: false, start: "2026-11-01".into(), end: "2026-11-30".into(), income: 12.0, other_investable: 240.0 };
    // the sample file is written and read: its numbers are the example's
    let params = load_psr(spec.dates().expect("dates").1).expect("the sample parameters");
    assert!(params.meta.sample);
    let out = build_mudaraba(&deposits, &spec, &params).expect("distribution");
    micro_parquet::rules::set_data_dir_override(None);
    let names: Vec<String> = strings(&out, "Product").into_iter().flatten().collect();
    let keep: Vec<bool> = names.iter().map(|n| ["MSND", "MSB", "MTDR-1Y"].contains(&n.as_str())).collect();
    let mask = BooleanChunked::from_slice("m".into(), &keep);
    let products = out.filter(&mask).expect("filter");
    check_guide(
        "mudaraba-profit-distribution",
        "guide_mudaraba_profit_distribution",
        &[md(&deposits, &["Product", "Avg_Balance:2"]), md(&products, &["Product", "Weighted balance:2", "Profit:~6", "Indicative rate % p.a.:~4"])],
    );
}
