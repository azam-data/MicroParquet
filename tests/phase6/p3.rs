//! P3: month-on-month variance and commentary.

use crate::common::*;
use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::models::*;
use micro_parquet::mom::*;
use micro_parquet::tool_runs::*;
use polars::prelude::*;

const CR: f64 = 10_000_000.0;

fn month(rows: &[(&str, f64, f64)]) -> DataFrame {
    DataFrame::new(vec![
        Series::new("Product".into(), rows.iter().map(|r| r.0).collect::<Vec<_>>()),
        Series::new("Balance".into(), rows.iter().map(|r| r.1 * CR).collect::<Vec<_>>()),
        Series::new("Rate".into(), rows.iter().map(|r| r.2).collect::<Vec<_>>()),
    ])
    .expect("month")
}

/// The acceptance book: TD 1,000 Cr at 8.00% and SB 500 Cr at 3.00%, then TD 1,100 Cr at 8.50% and SB 450 Cr at 3.00%.
fn prev() -> DataFrame {
    month(&[("TD", 1000.0, 8.0), ("SB", 500.0, 3.0)])
}
fn curr() -> DataFrame {
    month(&[("TD", 1100.0, 8.5), ("SB", 450.0, 3.0)])
}

fn spec() -> MomSpec {
    MomSpec { key_cols: vec!["Product".into()], bal_col: "Balance".into(), rate_col: Some("Rate".into()), ..Default::default() }
}

fn run(p: &DataFrame, c: &DataFrame, s: &MomSpec) -> anyhow::Result<DataFrame> {
    build_mom(p.clone().lazy(), c.clone().lazy(), s)
}

fn at(df: &DataFrame, key: &str, col: &str) -> Option<f64> {
    let i = strings(df, "Product").iter().position(|k| k.as_deref() == Some(key)).unwrap_or_else(|| panic!("key {key}"));
    floats(df, col)[i]
}

fn near(a: Option<f64>, b: f64, what: &str) {
    let a = a.unwrap_or(f64::NAN);
    assert!((a - b).abs() < 1.0, "{what}: {a} vs {b}");
}

#[test]
fn p3_the_acceptance_book_gives_the_changes_the_interest_and_its_three_effects() {
    let df = run(&prev(), &curr(), &spec()).expect("variance");
    let cols: Vec<String> = df.get_column_names().iter().map(|n| n.to_string()).collect();
    assert_eq!(cols, ["Product", "Status", "Prev balance", "Curr balance", "Change", "Change %", "Prev rate", "Curr rate", "Rate change (bp)", "Prev annual interest", "Curr annual interest", "Annual interest change", "Volume effect", "Rate effect", "Mix effect"]);
    // TD: +100.00 (10.00%), +50 bp; interest 80.00 -> 93.50, +13.50 = volume 8.00 + rate 5.00 + mix 0.50
    near(at(&df, "TD", "Change"), 100.0 * CR, "TD change");
    near(at(&df, "TD", "Change %"), 10.0, "TD change %");
    assert!((at(&df, "TD", "Change %").expect("pct") - 10.0).abs() < 1e-9);
    assert!((at(&df, "TD", "Rate change (bp)").expect("bp") - 50.0).abs() < 1e-9);
    near(at(&df, "TD", "Prev annual interest"), 80.0 * CR, "prev interest");
    near(at(&df, "TD", "Curr annual interest"), 93.5 * CR, "curr interest");
    near(at(&df, "TD", "Annual interest change"), 13.5 * CR, "interest change");
    near(at(&df, "TD", "Volume effect"), 8.0 * CR, "volume");
    near(at(&df, "TD", "Rate effect"), 5.0 * CR, "rate");
    near(at(&df, "TD", "Mix effect"), 0.5 * CR, "mix");
    // SB: -50.00 (-10.00%); interest change -1.50 = volume -1.50
    near(at(&df, "SB", "Change"), -50.0 * CR, "SB change");
    assert!((at(&df, "SB", "Change %").expect("pct") + 10.0).abs() < 1e-9);
    near(at(&df, "SB", "Annual interest change"), -1.5 * CR, "SB interest change");
    near(at(&df, "SB", "Volume effect"), -1.5 * CR, "SB volume");
    near(at(&df, "SB", "Rate effect"), 0.0, "SB rate effect");
    near(at(&df, "SB", "Mix effect"), 0.0, "SB mix");
    // the three effects always add up to the interest change of a key in both months
    for k in ["TD", "SB"] {
        let sum = at(&df, k, "Volume effect").expect("v") + at(&df, k, "Rate effect").expect("r") + at(&df, k, "Mix effect").expect("m");
        near(at(&df, k, "Annual interest change"), sum, "effects add up");
    }
}

#[test]
fn p3_the_two_commentary_lines_appear_exactly_td_first_and_a_high_threshold_leaves_only_the_total() {
    let df = run(&prev(), &curr(), &spec()).expect("variance");
    let lines = mom_commentary(&df, &spec()).expect("commentary");
    assert_eq!(lines[0], "TD rose by 100.00 Cr (10.00%) to 1,100.00 Cr; rate up 50 bp to 8.50%.");
    assert_eq!(lines[1], "SB fell by 50.00 Cr (10.00%) to 450.00 Cr; rate unchanged at 3.00%.");
    assert_eq!(lines.len(), 3, "two keys and the total");
    assert_eq!(lines[2], "Total: rose by 50.00 Cr (3.33%) to 1,550.00 Cr.");
    // 200 Cr and 20%: nothing is material, only the total is left
    let high = MomSpec { material_amount: 200.0, material_pct: 20.0, ..spec() };
    let lines = mom_commentary(&df, &high).expect("commentary");
    assert_eq!(lines, vec!["Total: rose by 50.00 Cr (3.33%) to 1,550.00 Cr.".to_string()]);
    // either threshold is enough: 500 Cr or 10%
    let by_pct = MomSpec { material_amount: 500.0, material_pct: 10.0, ..spec() };
    assert_eq!(mom_commentary(&df, &by_pct).expect("commentary").len(), 3, "10% reaches both");
    let by_amount = MomSpec { material_amount: 75.0, material_pct: 50.0, ..spec() };
    let l = mom_commentary(&df, &by_amount).expect("commentary");
    assert_eq!((l.len(), l[0].starts_with("TD rose")), (2, true), "only TD moved by 75 Cr or more");
    // other scales say it in their own unit
    let lakh = MomSpec { scale: NumberDisplayFormat::BdtLakh, material_amount: 5000.0, ..spec() };
    assert!(mom_commentary(&df, &lakh).expect("commentary")[0].contains("10,000.00 L"), "100 Cr is 10,000 lakh");
}

#[test]
fn p3_a_key_in_one_month_only_is_new_or_closed_with_no_effects_and_follows_materiality() {
    let p = month(&[("TD", 1000.0, 8.0), ("OLD", 20.0, 6.0)]);
    let c = month(&[("TD", 1000.0, 8.0), ("FD", 10.0, 9.0)]);
    let df = run(&p, &c, &spec()).expect("variance");
    let st = |k: &str| strings(&df, "Status")[strings(&df, "Product").iter().position(|x| x.as_deref() == Some(k)).expect("key")].clone();
    assert_eq!((st("FD").as_deref(), st("OLD").as_deref(), st("TD").as_deref()), (Some("New"), Some("Closed"), Some("")));
    near(at(&df, "FD", "Prev balance"), 0.0, "a new key had nothing");
    assert!(at(&df, "FD", "Change %").is_none() && at(&df, "FD", "Volume effect").is_none() && at(&df, "OLD", "Rate effect").is_none());
    near(at(&df, "FD", "Annual interest change"), 0.9 * CR, "new interest");
    // the effects of the unchanged key are 0
    near(at(&df, "TD", "Volume effect"), 0.0, "TD volume");
    // H3: below the materiality limit they are only counted, in one line
    let lines = mom_commentary(&df, &MomSpec { material_amount: 1000.0, material_pct: 1000.0, ..spec() }).expect("commentary");
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[0], "Not listed, below the materiality limit: 1 new and 1 closed key(s).");
    assert!(lines[1].starts_with("Total:"));
    // at or above it they are named
    let lines = mom_commentary(&df, &MomSpec { material_amount: 15.0, material_pct: 1000.0, ..spec() }).expect("commentary");
    assert_eq!(lines[0], "OLD has closed: it had 20.00 Cr last month.", "{lines:?}");
    assert_eq!(lines[1], "Not listed, below the materiality limit: 1 new key(s).", "FD (10 Cr) is below 15 Cr");
    let lines = mom_commentary(&df, &MomSpec { material_amount: 5.0, material_pct: 1000.0, ..spec() }).expect("commentary");
    assert_eq!(lines[0], "OLD has closed: it had 20.00 Cr last month.");
    assert_eq!(lines[1], "FD is new: 10.00 Cr at 9.00%.");
    assert!(lines[2].starts_with("Total:"));
}

#[test]
fn p3_two_keys_a_weighted_rate_a_count_and_mixed_signs() {
    let p = DataFrame::new(vec![
        Series::new("Product".into(), &["TD", "TD", "SB"]),
        Series::new("Segment".into(), &["Retail", "Retail", "Retail"]),
        Series::new("Balance".into(), &[600.0 * CR, 400.0 * CR, 100.0 * CR]),
        Series::new("Rate".into(), &[8.0, 9.0, 3.0]),
        Series::new("Accounts".into(), &[10.0, 5.0, 40.0]),
    ])
    .expect("prev");
    let c = DataFrame::new(vec![
        Series::new("Product".into(), &["TD", "SB", "SB"]),
        Series::new("Segment".into(), &["Retail", "Retail", "Retail"]),
        Series::new("Balance".into(), &[1200.0 * CR, 150.0 * CR, -50.0 * CR]),
        Series::new("Rate".into(), &[8.4, 3.0, 3.0]),
        Series::new("Accounts".into(), &[16.0, 30.0, 5.0]),
    ])
    .expect("curr");
    let s = MomSpec { key_cols: vec!["Product".into(), "Segment".into()], count_col: Some("Accounts".into()), ..spec() };
    let df = run(&p, &c, &s).expect("variance");
    assert_eq!(strings(&df, "Product"), vec![Some("SB".to_string()), Some("TD".to_string())], "sorted by key");
    // TD last month: (600 x 8 + 400 x 9) / 1000 = 8.4%
    assert!((floats(&df, "Prev rate")[1].expect("rate") - 8.4).abs() < 1e-9);
    assert_eq!(floats(&df, "Prev count"), vec![Some(40.0), Some(15.0)]);
    assert_eq!(floats(&df, "Curr count"), vec![Some(35.0), Some(16.0)]);
    // SB this month is +150 and -50: a mix of signs has no rate (the Phase 2 rule), and no effects
    assert!(floats(&df, "Curr rate")[0].is_none() && floats(&df, "Rate effect")[0].is_none());
    near(floats(&df, "Curr balance")[0], 100.0 * CR, "SB balance is the sum");
    let lines = mom_commentary(&df, &MomSpec { material_amount: 0.0, material_pct: 0.0, ..s.clone() }).expect("commentary");
    assert_eq!(lines[0], "TD / Retail rose by 200.00 Cr (20.00%) to 1,200.00 Cr; rate unchanged at 8.40%.", "{lines:?}");
    assert!(!lines.iter().any(|l| l.contains("SB / Retail")), "SB did not change: {lines:?}");
}

#[test]
fn p3_bad_input_stops_with_a_message_that_names_the_fix() {
    let ok = |p: &DataFrame, c: &DataFrame, s: &MomSpec| format!("{:#}", run(p, c, s).expect_err("an error"));
    // no key, three keys, the same key twice
    assert!(ok(&prev(), &curr(), &MomSpec { key_cols: vec![], ..spec() }).contains("one or two key columns"));
    assert!(ok(&prev(), &curr(), &MomSpec { key_cols: vec!["Product".into(), "Product".into()], ..spec() }).contains("same"));
    assert!(ok(&prev(), &curr(), &MomSpec { bal_col: String::new(), ..spec() }).contains("balance column"));
    assert!(ok(&prev(), &curr(), &MomSpec { material_amount: -1.0, ..spec() }).contains("thresholds"));
    // a column the previous month lacks
    let no_rate = prev().drop("Rate").expect("drop");
    let e = ok(&no_rate, &curr(), &spec());
    assert!(e.contains("previous month") && e.contains("'Rate'"), "{e}");
    // a text balance names its type
    let text = DataFrame::new(vec![Series::new("Product".into(), &["TD"]), Series::new("Balance".into(), &["1,000"]), Series::new("Rate".into(), &[8.0])]).expect("text");
    let e = ok(&prev(), &text, &spec());
    assert!(e.contains("current month") && e.contains("not a number"), "{e}");
    // a blank key, a balance without a rate
    let blank = DataFrame::new(vec![Series::new("Product".into(), &[Some("TD"), None]), Series::new("Balance".into(), &[1.0, 2.0]), Series::new("Rate".into(), &[8.0, 8.0])]).expect("blank");
    assert!(ok(&blank, &curr(), &spec()).contains("1 row(s) of the previous month have a blank key"));
    let norate = DataFrame::new(vec![Series::new("Product".into(), &["TD", "SB"]), Series::new("Balance".into(), &[1.0, 2.0]), Series::new("Rate".into(), &[Some(8.0), None])]).expect("norate");
    let e = ok(&prev(), &norate, &spec());
    assert!(e.contains("1 row(s) of the current month have a balance but no rate"), "{e}");
    // without a rate column the table has no rate or interest columns
    let plain = MomSpec { rate_col: None, ..spec() };
    let df = run(&prev(), &curr(), &plain).expect("variance");
    assert_eq!(df.width(), 6, "Product, Status, Prev, Curr, Change, Change %");
    assert!(mom_commentary(&df, &plain).expect("commentary")[0].ends_with("to 1,100.00 Cr."), "no rate clause without a rate");
}

/// A tiny set of open tabs for the saved-run round trip.
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

#[test]
fn p3_the_tab_source_gives_both_tables_and_the_run_is_saved_and_made_again() {
    let d = dir("p3_tab");
    let (ps, cs) = (parquet(&d, "sept", &prev()), parquet(&d, "oct", &curr()));
    let mk = |view| TabSource::MomVariance { prev_source: Box::new(ps.clone()), prev_steps: vec![], curr_source: Box::new(cs.clone()), curr_steps: vec![], spec: spec(), view };
    let table = compile_tab_lazy(&mk(MomView::Variance), &[], &d).expect("compiles").collect().expect("collects");
    assert_eq!(table.height(), 2);
    let text = compile_tab_lazy(&mk(MomView::Commentary), &[], &d).expect("compiles").collect().expect("collects");
    assert_eq!(strings(&text, COMMENTARY_COLUMN).iter().flatten().count(), 3);
    // a step after the table works as on any tab
    let steps = [TransformStep::FilterRows { column: "Product".into(), operator: FilterOp::Eq, value: "TD".into(), keep_matching: true }];
    assert_eq!(compile_tab_lazy(&mk(MomView::Variance), &steps, &d).expect("compiles").collect().expect("collects").height(), 1);
    // run -> save -> reload -> make again
    let paths = PortablePaths { root_dir: d.clone(), macros_dir: d.join("m"), schemas_dir: d.join("s"), curves_dir: d.join("c"), buckets_file: d.join("b.json"), temp_dir: d.join("t") };
    let env = Env { tabs: vec![("Sept".into(), RunTab { source: ps.clone(), steps: vec![] }), ("Oct".into(), RunTab { source: cs.clone(), steps: vec![] })], paths };
    let run = ToolRun::from_source(&mk(MomView::Variance), &[], &env).expect("run");
    assert_eq!(run.source_names(), vec!["Sept".to_string(), "Oct".to_string()]);
    save_run(&d.join("runs"), "mom", &run).expect("saves");
    let back = load_run(&d.join("runs"), "mom").expect("loads");
    let (title, src, st) = back.run.build(&env).expect("builds").remove(0);
    assert_eq!(title, "MoM variance");
    let again = compile_tab_lazy(&src, &st, &d).expect("compiles").collect().expect("collects");
    assert!(table.equals_missing(&again));
    // the thresholds are part of the saved run (Phase 6 P3: stored with the step)
    let ToolRun::MomVariance(r) = &back.run else { panic!("a MoM run") };
    assert_eq!((r.spec.material_amount, r.spec.material_pct), (50.0, 5.0));
    // the run takes a pack's cutoff without complaint: it has none
    let mut c = back.run.clone();
    assert!(!c.set_cutoff("2026-10-31"));
}

#[test]
fn g2_keys_are_matched_trimmed_and_ignoring_capitals_shown_as_the_current_month_and_noted() {
    let p = month(&[("TD", 600.0, 8.0), ("td ", 400.0, 8.0), ("SB", 500.0, 3.0)]);
    let c = month(&[("td", 1100.0, 8.5), ("sb", 450.0, 3.0)]);
    let (df, notes) = build_mom_noted(p.clone().lazy(), c.clone().lazy(), &spec()).expect("variance");
    assert_eq!(df.height(), 2, "TD/td is one row, SB/sb is one row");
    let keys: Vec<String> = strings(&df, "Product").into_iter().flatten().collect();
    assert_eq!(keys, vec!["sb", "td"], "shown with the current month's spelling");
    near(at(&df, "td", "Prev balance"), 1000.0 * CR, "the two previous spellings add up");
    near(at(&df, "td", "Change"), 100.0 * CR, "change");
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("TD + td") && notes[0].contains("shown as td") && notes[0].contains("SB + sb"), "{notes:?}");
    assert!(df.column("Status").expect("status").str().expect("str").into_iter().all(|s| s == Some("")), "none is New or Closed");
    // the same case in both months: nothing to note
    let (_, none) = build_mom_noted(prev().lazy(), curr().lazy(), &spec()).expect("variance");
    assert!(none.is_empty());
    // the commentary tab ends with the note
    let d = dir("g2_tab");
    let (ps, cs) = (parquet(&d, "sept", &p), parquet(&d, "oct", &c));
    let tab = TabSource::MomVariance { prev_source: Box::new(ps), prev_steps: vec![], curr_source: Box::new(cs), curr_steps: vec![], spec: spec(), view: MomView::Commentary };
    let text = compile_tab_lazy(&tab, &[], &d).expect("compiles").collect().expect("collects");
    let lines: Vec<String> = strings(&text, COMMENTARY_COLUMN).into_iter().flatten().collect();
    assert!(lines.last().is_some_and(|l| l.starts_with("Note: keys that differ only in capital letters")), "{lines:?}");
}

#[test]
fn g3_a_blank_balance_or_count_stops_the_run_with_the_count_and_the_first_five_keys() {
    let blank_bal = DataFrame::new(vec![
        Series::new("Product".into(), ["A", "B", "C", "D", "E", "F", "G"]),
        Series::new("Balance".into(), [Some(1.0), None, None, None, None, None, None, ]),
        Series::new("Rate".into(), [8.0; 7]),
    ])
    .expect("frame");
    let msg = format!("{:#}", run(&blank_bal, &curr(), &spec()).expect_err("blank balance"));
    assert!(msg.contains("6 row(s) of the previous month have a blank balance") && msg.contains("first keys: B, C, D, E, F") && !msg.contains("G)"), "{msg}");
    let with_count = MomSpec { count_col: Some("N".into()), ..spec() };
    let mut c = curr();
    c.with_column(Series::new("N".into(), [Some(3.0), None])).expect("count");
    let mut p = prev();
    p.with_column(Series::new("N".into(), [1.0, 2.0])).expect("count");
    let msg = format!("{:#}", run(&p, &c, &with_count).expect_err("blank count"));
    assert!(msg.contains("1 row(s) of the current month have a blank count") && msg.contains("SB"), "{msg}");
}

#[test]
fn g7_the_same_file_twice_is_refused_as_both_months_but_a_different_file_is_not() {
    let d = dir("g7_same");
    let a = parquet(&d, "sept", &prev());
    let TabSource::SingleFile { path, .. } = &a else { panic!("a file source") };
    let copy = d.join("sept_copy.parquet");
    std::fs::copy(path, &copy).expect("copy");
    let b = TabSource::SingleFile { path: copy, is_parquet: true, config: IngestionConfig::default() };
    let mk = |p: &TabSource, c: &TabSource| TabSource::MomVariance { prev_source: Box::new(p.clone()), prev_steps: vec![], curr_source: Box::new(c.clone()), curr_steps: vec![], spec: spec(), view: MomView::Variance };
    let err = compile_tab_lazy(&mk(&a, &b), &[], &d).err().expect("refused");
    assert!(format!("{err:#}").contains("the previous and the current month are the same file"), "{err:#}");
    let other = parquet(&d, "oct", &curr());
    compile_tab_lazy(&mk(&a, &other), &[], &d).expect("two different files compile");
}
