//! Phase 6 guide notes: one `docs/guide/<tool>.md` per new tool, checked like the Phase 4 and 5 ones
//! (ten sections, proven worked examples). `MP_GUIDE_DUMP=<folder> cargo test guide_` writes the computed tables.

use crate::common::*;
use crate::guide::{check_guide, check_guide_file, read_guide};
use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::models::*;
use micro_parquet::sort_lists::SortOrder;
use polars::prelude::*;

/// The tool ids of Phase 6 (file names in docs/guide/), added as each task is done.
pub const P6_GUIDE_TOOLS: [&str; 4] = ["custom-sort-lists", "sql-editor", "chart", "saved-tool-runs"];

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
