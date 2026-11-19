//! P4: the data-quality scorecard.

use crate::common::*;
use micro_parquet::dq::*;
use micro_parquet::engine::*;
use micro_parquet::models::*;
use micro_parquet::tool_runs::*;
use polars::prelude::*;
use std::path::{Path, PathBuf};

/// Ten rows: one blank Account_No, one pair of equal Account_No values, two Rate values that do not
/// parse, one Rate of 45 (range 0 to 30) and one Maturity before its Open_Date.
const BOOK: &str = "Account_No,Rate,Open_Date,Maturity,Balance\n\
A01,9.5,2020-01-01,2025-01-01,100\n\
A02,8.0,2020-01-01,2025-01-01,100\n\
A03,abc,2020-01-01,2025-01-01,100\n\
A04,xyz,2020-01-01,2025-01-01,100\n\
A05,45,2020-01-01,2025-01-01,100\n\
A06,7.0,2020-01-01,2019-12-31,100\n\
A07,6.0,2020-01-01,2025-01-01,100\n\
A07,6.5,2020-01-01,2025-01-01,100\n\
,5.5,2020-01-01,2025-01-01,100\n\
A10,4.0,2020-01-01,2025-01-01,100\n";

fn book(d: &Path) -> TabSource {
    let p = d.join("book.csv");
    std::fs::write(&p, BOOK).expect("write");
    let mut cfg = auto_detect_smart_schema(&p).expect("detected");
    for c in cfg.columns.iter_mut() {
        c.dtype = match c.name.as_str() {
            "Account_No" => ColDataType::Utf8,
            "Rate" | "Balance" => ColDataType::Float64,
            _ => ColDataType::DateISO,
        };
    }
    TabSource::SingleFile { path: p, is_parquet: false, config: cfg }
}

fn rules(extra: Vec<DqRule>) -> DqRuleSet {
    let r = |name: &str, kind: DqKind| DqRule { name: name.into(), kind };
    let mut rules = vec![
        r("Completeness", DqKind::Completeness { column: "Account_No".into() }),
        r("Uniqueness", DqKind::Uniqueness { columns: vec!["Account_No".into()] }),
        r("Type", DqKind::TypeValidity { column: "Rate".into() }),
        r("Range", DqKind::Range { column: "Rate".into(), min: Some(0.0), max: Some(30.0) }),
        r("Consistency", DqKind::Consistency { left: "Maturity".into(), op: DqCompare::Ge, right: "Open_Date".into() }),
    ];
    rules.extend(extra);
    DqRuleSet { name: "acceptance".into(), as_of: "2026-11-01".into(), source: "test".into(), rules, ..Default::default() }
}

fn scorecard(d: &Path, set: &DqRuleSet) -> (DqResult, DataFrame) {
    let src = book(d);
    let df = compile_tab_lazy(&src, &[], d).expect("compiles").collect().expect("collects");
    let types = dq_type_stats(&src, set, d).expect("types");
    (run_scorecard(&df, &types, set, &[]).expect("scorecard"), df)
}

fn pct(r: &RuleResult) -> f64 {
    r.pass_pct.expect("a pass percentage")
}

#[test]
fn p4_the_acceptance_book_gives_the_five_percentages_the_average_and_red() {
    let d = dir("p4_a");
    let (res, _) = scorecard(&d, &rules(vec![]));
    let p: Vec<f64> = res.rules.iter().map(pct).collect();
    // completeness 90.00%, uniqueness 77.78%, type 80.00%, range 87.50%, consistency 90.00%
    for (got, want) in p.iter().zip([90.0, 700.0 / 9.0, 80.0, 87.5, 90.0]) {
        assert!((got - want).abs() < 1e-9, "{p:?}");
    }
    assert_eq!(format!("{:.2}", p[1]), "77.78");
    // overall 85.06%, Red
    let overall = res.overall.expect("overall");
    assert_eq!(format!("{overall:.2}"), "85.06");
    assert_eq!(res.rating, "Red");
    // rows checked: blanks are left out except for completeness
    let checked: Vec<usize> = res.rules.iter().map(|r| r.checked.expect("checked")).collect();
    assert_eq!(checked, vec![10, 9, 10, 8, 10]);
    // uniqueness: both rows of the pair fail
    assert_eq!(res.rules[1].failing_rows, vec![6, 7]);
    assert_eq!(res.rules[0].failing_rows, vec![8], "the blank account number");
    assert_eq!(res.rules[3].failing_rows, vec![4], "the rate of 45");
    assert_eq!(res.rules[4].failing_rows, vec![5], "the maturity before the open date");
    assert_eq!((res.rules[2].failing, res.rules[2].examples.clone()), (2, vec!["abc".to_string(), "xyz".to_string()]));
}

#[test]
fn p4_the_control_total_is_pass_or_fail_with_its_difference_and_stays_out_of_the_average() {
    let d = dir("p4_b");
    // the balances add up to 1,000 against a GL of 1,005 at 0.1%: Fail, difference -5
    let ctl = |gl: f64, tol: f64| DqRule { name: "GL control".into(), kind: DqKind::ControlTotal { column: "Balance".into(), gl_total: gl, tolerance_pct: tol } };
    let (res, _) = scorecard(&d, &rules(vec![ctl(1005.0, 0.1)]));
    let c = res.rules[5].control.clone().expect("control");
    assert_eq!((c.sum, c.gl_total, c.difference, c.pass), (1000.0, 1005.0, -5.0, false));
    assert!(res.rules[5].pass_pct.is_none() && res.rules[5].checked.is_none());
    assert_eq!(format!("{:.2}", res.overall.expect("overall")), "85.06", "the same average without the control");
    assert!(res.rules[5].detail.contains("difference -5.00"), "{}", res.rules[5].detail);
    // within tolerance it passes; 0.5% of 1,005 is 5.025
    let (res, _) = scorecard(&d, &rules(vec![ctl(1005.0, 0.5)]));
    assert!(res.rules[5].control.as_ref().expect("control").pass);
    let frame = scorecard_frame(&res).expect("frame");
    assert_eq!(strings(&frame, "Result")[5].as_deref(), Some("Pass"));
    assert_eq!(strings(&frame, "Rule").last().cloned().flatten().as_deref(), Some("Overall score"));
}

#[test]
fn p4_rating_thresholds_come_from_the_rules_file_and_the_scorecard_table_has_a_row_per_rule() {
    let d = dir("p4_c");
    let mut set = rules(vec![]);
    set.green_min = 90.0;
    set.amber_min = 80.0;
    let (res, df) = scorecard(&d, &set);
    assert_eq!(res.rating, "Amber", "85.06 with Green from 90 and Amber from 80");
    set.amber_min = 85.07;
    assert!(set.check().is_ok());
    assert_eq!(set.rating(85.06), "Red");
    assert_eq!((set.rating(98.0), DqRuleSet::default().rating(95.0), DqRuleSet::default().rating(94.99)), ("Green", "Amber", "Red"));
    let frame = scorecard_frame(&res).expect("frame");
    assert_eq!(frame.get_column_names().iter().map(|n| n.to_string()).collect::<Vec<_>>(), SCORECARD_COLUMNS.to_vec());
    assert_eq!(frame.height(), 6, "five rules and the overall row");
    assert_eq!(strings(&frame, "Result").last().cloned().flatten().as_deref(), Some("Amber"));
    // the failing rows of a rule: a Row column (1-based) then the table's columns; the first 20 only for the dialog
    let f = failing_frame(&df, &res, 1, None).expect("failing rows");
    assert_eq!(floats(&f, "Row"), vec![Some(7.0), Some(8.0)]);
    assert_eq!(strings(&f, "Account_No"), vec![Some("A07".to_string()), Some("A07".to_string())]);
    assert_eq!(failing_frame(&df, &res, 1, Some(1)).expect("limited").height(), 1);
    // a type rule cannot list rows, and says what it found
    let e = format!("{:#}", failing_frame(&df, &res, 2, None).expect_err("type rule"));
    assert!(e.contains("abc | xyz"), "{e}");
}

#[test]
fn p4_range_consistency_allowed_values_and_uniqueness_on_two_columns() {
    let df = DataFrame::new(vec![
        Series::new("Branch".into(), &["DHK", "DHK", "CTG", "CTG", "SYL"]),
        Series::new("Product".into(), &["TD", "TD", "SB", "XX", " "]),
        Series::new("Grade".into(), &["STD", "sma", "SS", "ZZ", "BL"]),
        Series::new("Open".into(), &[1i64, 5, 3, 4, 5]),
        Series::new("Close".into(), &[Some(2i64), Some(4), None, Some(9), Some(5)]),
    ])
    .expect("frame");
    let r = |kind| DqRule { name: String::new(), kind };
    let set = DqRuleSet {
        rules: vec![
            r(DqKind::Uniqueness { columns: vec!["Branch".into(), "Product".into()] }),
            r(DqKind::AllowedValues { column: "Product".into(), values: vec!["td".into(), "SB".into()], code_map: None, case_insensitive: true }),
            r(DqKind::AllowedValues { column: "Grade".into(), values: vec![], code_map: Some("cl_code_map".into()), case_insensitive: true }),
            r(DqKind::Consistency { left: "Close".into(), op: DqCompare::Ge, right: "Open".into() }),
            r(DqKind::Range { column: "Open".into(), min: Some(2.0), max: None }),
        ],
        ..Default::default()
    };
    let cl = vec![("STD".to_string(), "STD".to_string()), ("SMA".to_string(), "SMA".to_string()), ("SS".to_string(), "SS".to_string()), ("BL".to_string(), "BL".to_string())];
    let res = run_scorecard(&df, &Default::default(), &set, &cl).expect("scorecard");
    // two keys: (DHK, TD) twice; the blank product is not checked
    assert_eq!((res.rules[0].checked, res.rules[0].failing_rows.clone()), (Some(4), vec![0, 1]));
    // allowed products: XX fails, the blank is left out, "td" matches ignoring case
    assert_eq!((res.rules[1].checked, res.rules[1].failing_rows.clone()), (Some(4), vec![3]));
    // the code map: ZZ is not a code
    assert_eq!(res.rules[2].failing_rows, vec![3]);
    // Close >= Open: row 2 fails (4 < 5); the blank close is left out
    assert_eq!((res.rules[3].checked, res.rules[3].failing_rows.clone()), (Some(4), vec![1]));
    assert_eq!(res.rules[4].failing_rows, vec![0], "Open 1 is below the minimum 2");
    assert_eq!(res.overall.map(|o| (o * 100.0).round() / 100.0), Some(((50.0 + 75.0 + 80.0 + 75.0 + 80.0) / 5.0f64 * 100.0).round() / 100.0));
}

#[test]
fn p4_bad_rules_and_bad_columns_stop_with_a_message_that_names_the_fix() {
    let df = DataFrame::new(vec![Series::new("Name".into(), &["a"]), Series::new("Open".into(), &[1i64]), Series::new("When".into(), &[1i32]).cast(&DataType::Date).expect("date"), Series::new("Rate".into(), &[1.0f64])]).expect("frame");
    let run = |k: DqKind| format!("{:#}", run_scorecard(&df, &Default::default(), &DqRuleSet { rules: vec![DqRule { name: "r".into(), kind: k }], ..Default::default() }, &[]).expect_err("an error"));
    assert!(run(DqKind::Range { column: "Name".into(), min: Some(0.0), max: None }).contains("is str, not a number") || run(DqKind::Range { column: "Name".into(), min: Some(0.0), max: None }).contains("not a number"));
    assert!(run(DqKind::Completeness { column: "Nope".into() }).contains("no column 'Nope'"));
    assert!(run(DqKind::Consistency { left: "Open".into(), op: DqCompare::Ge, right: "When".into() }).contains("same kind"));
    assert!(run(DqKind::Consistency { left: "Name".into(), op: DqCompare::Ge, right: "Name".into() }).contains("numbers or dates"));
    assert!(run(DqKind::TypeValidity { column: "Rate".into() }).contains("file's own columns"));
    assert!(run(DqKind::AllowedValues { column: "Name".into(), values: vec![], code_map: Some("other".into()), case_insensitive: true }).contains("code map 'other'"));
    // set-level checks
    let bad = |f: &dyn Fn(&mut DqRuleSet)| {
        let mut s = rules(vec![]);
        f(&mut s);
        format!("{:#}", s.check().expect_err("rejected"))
    };
    assert!(bad(&|s| s.rules.clear()).contains("no rules"));
    assert!(bad(&|s| s.amber_min = 99.0).contains("Amber"));
    assert!(bad(&|s| s.green_min = 120.0).contains("Green"));
    assert!(bad(&|s| s.rules[3].kind = DqKind::Range { column: "Rate".into(), min: None, max: None }).contains("minimum, a maximum or both"));
    assert!(bad(&|s| s.rules[3].kind = DqKind::Range { column: "Rate".into(), min: Some(5.0), max: Some(1.0) }).contains("above its maximum"));
    assert!(bad(&|s| s.rules[0].kind = DqKind::Completeness { column: " ".into() }).contains("not chosen"));
}

#[test]
fn p4_a_rule_set_is_a_file_with_the_sample_written_on_first_use_and_marked() {
    let dirp = dir("p4_files").join("dq_rules");
    let list = list_rulesets(&dirp);
    assert_eq!(list, vec!["sample".to_string()], "the sample is written when the folder has none");
    let s = load_ruleset(&dirp, "sample").expect("loads");
    assert!(s.sample && s.source.contains("SAMPLE"), "marked sample");
    assert_eq!((s.green_min, s.amber_min, s.rules.len()), (98.0, 95.0, 5), "Green from 98, Amber from 95");
    assert!(matches!(s.rules[3].kind, DqKind::Range { min: Some(0.0), max: Some(30.0), .. }));
    // saved and read back; a name that is not a file name is refused; an edited file is not overwritten
    let mut mine = s.clone();
    mine.name = "bank".into();
    mine.sample = false;
    mine.rules.push(DqRule { name: "GL".into(), kind: DqKind::ControlTotal { column: "Balance".into(), gl_total: 10.0, tolerance_pct: 0.1 } });
    save_ruleset(&dirp, &mine).expect("saves");
    assert_eq!(load_ruleset(&dirp, "bank").expect("loads"), mine);
    assert!(save_ruleset(&dirp, &DqRuleSet { name: "a/b".into(), ..mine.clone() }).is_err());
    std::fs::write(dirp.join("sample.json"), "{\"green_min\": 98, \"amber_min\": 95, \"rules\": []}").expect("edit");
    assert!(format!("{:#}", load_ruleset(&dirp, "sample").expect_err("no rules")).contains("no rules"));
    assert!(load_ruleset(&dirp, "missing").is_err());
    // the file's JSON puts the kind next to the name
    let text = std::fs::read_to_string(dirp.join("bank.json")).expect("read");
    assert!(text.contains("\"kind\": \"completeness\"") && text.contains("\"kind\": \"control_total\""), "{text}");
}

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

fn d_path(d: &PathBuf) -> PortablePaths {
    PortablePaths { root_dir: d.clone(), macros_dir: d.join("m"), schemas_dir: d.join("s"), curves_dir: d.join("c"), buckets_file: d.join("b.json"), temp_dir: d.join("t") }
}

#[test]
fn p4_the_scorecard_is_a_tab_with_a_failing_rows_view_a_saved_run_and_steps_before_it_count() {
    let d = dir("p4_tab");
    let src = book(&d);
    let set = rules(vec![]);
    let mk = |view, steps: Vec<TransformStep>| TabSource::DqScorecard { source: Box::new(src.clone()), steps, rules: Box::new(set.clone()), view };
    let t = compile_tab_lazy(&mk(DqView::Scorecard, vec![]), &[], &d).expect("compiles").collect().expect("collects");
    assert_eq!(t.height(), 6);
    assert_eq!(floats(&t, "Pass %").iter().flatten().map(|v| format!("{v:.2}")).collect::<Vec<_>>(), vec!["90.00", "77.78", "80.00", "87.50", "90.00", "85.06"]);
    // the failing rows of the range rule as a tab
    let f = compile_tab_lazy(&mk(DqView::Failing(3), vec![]), &[], &d).expect("compiles").collect().expect("collects");
    assert_eq!((f.height(), floats(&f, "Row")[0]), (1, Some(5.0)));
    // steps before the scorecard shape the rows it checks: without A07 the uniqueness is 100%
    let drop = vec![TransformStep::FilterRows { column: "Account_No".into(), operator: FilterOp::NotEq, value: "A07".into(), keep_matching: true }];
    let t2 = compile_tab_lazy(&mk(DqView::Scorecard, drop.clone()), &[], &d).expect("compiles").collect().expect("collects");
    assert_eq!(floats(&t2, "Pass %")[1], Some(100.0));
    // the source is checked for the type rule too: the type statistics read the source before the steps
    assert_eq!(floats(&t2, "Pass %")[2], Some(80.0));
    // run -> save -> reload -> make again
    let env = Env { tabs: vec![("Book".into(), RunTab { source: src.clone(), steps: vec![] })], paths: d_path(&d) };
    let run = ToolRun::from_source(&mk(DqView::Scorecard, vec![]), &[], &env).expect("run");
    assert_eq!(run.source_names(), vec!["Book".to_string()]);
    save_run(&d.join("runs"), "dq", &run).expect("saves");
    let back = load_run(&d.join("runs"), "dq").expect("loads");
    let (title, s2, st2) = back.run.build(&env).expect("builds").remove(0);
    assert_eq!(title, "Data-quality scorecard");
    let again = compile_tab_lazy(&s2, &st2, &d).expect("compiles").collect().expect("collects");
    assert!(t.equals_missing(&again));
    // the rules file is a parameter file of the run, with its sample flag
    micro_parquet::rules::set_data_dir_override(Some(d.clone()));
    let (_, used) = micro_parquet::rules::track_used_files(|| compile_tab_lazy(&mk(DqView::Scorecard, vec![]), &[], &d).and_then(|lf| Ok(lf.collect()?)).expect("runs"));
    micro_parquet::rules::set_data_dir_override(None);
    assert!(used.iter().any(|u| u.kind == "Data-quality rules" && u.file == "dq_rules/acceptance.json"), "{used:?}");
}

#[test]
fn g4_a_rules_file_without_green_min_or_amber_min_is_refused_naming_the_file_and_the_field() {
    let dirp = dir("g4_dq");
    let mine = DqRuleSet { name: "bank".into(), green_min: 97.0, amber_min: 90.0, rules: vec![DqRule { name: "r".into(), kind: DqKind::Range { column: "Balance".into(), min: Some(0.0), max: None } }], ..Default::default() };
    save_ruleset(&dirp, &mine).expect("saves");
    let text = std::fs::read_to_string(dirp.join("bank.json")).expect("read");
    let mut v: serde_json::Value = serde_json::from_str(&text).expect("json");
    for field in ["green_min", "amber_min"] {
        let mut w = v.clone();
        w.as_object_mut().expect("object").remove(field);
        std::fs::write(dirp.join("bank.json"), serde_json::to_string_pretty(&w).expect("text")).expect("write");
        let msg = format!("{:#}", load_ruleset(&dirp, "bank").expect_err("missing threshold"));
        assert!(msg.contains("bank.json") && msg.contains(&format!("no '{field}'")), "{msg}");
    }
    v["green_min"] = serde_json::json!("high");
    std::fs::write(dirp.join("bank.json"), serde_json::to_string_pretty(&v).expect("text")).expect("write");
    assert!(format!("{:#}", load_ruleset(&dirp, "bank").expect_err("not a number")).contains("no 'green_min'"));
}
