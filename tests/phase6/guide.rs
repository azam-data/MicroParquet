//! Phase 6 guide notes: one `docs/guide/<tool>.md` per new tool, checked like the Phase 4 and 5 ones
//! (ten sections, proven worked examples). `MP_GUIDE_DUMP=<folder> cargo test guide_` writes the computed tables.

use crate::common::*;
use crate::guide::{check_guide, check_guide_file, read_guide};
use micro_parquet::models::*;
use micro_parquet::sort_lists::SortOrder;
use polars::prelude::*;

/// The tool ids of Phase 6 (file names in docs/guide/), added as each task is done.
pub const P6_GUIDE_TOOLS: [&str; 2] = ["custom-sort-lists", "sql-editor"];

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
