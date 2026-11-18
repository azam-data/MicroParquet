//! Phase 5 R6: LCR and NSFR from GL balances.

use crate::common::*;
use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::liquidity::*;
use micro_parquet::models::*;
use micro_parquet::rules::set_data_dir_override;
use polars::prelude::*;

/// A small map with the factors of the acceptance examples (lines are named by what they are).
fn map_json() -> serde_json::Value {
    serde_json::json!({
        "as_of": "2026-06-30",
        "source": "test map (not a circular)",
        "sample": false,
        "lines": [
            {"code": "L1", "label": "Level 1", "ratio": "LCR", "kind": "hqla_l1", "factor_pct": 100, "side": "asset"},
            {"code": "L2A", "label": "Level 2A", "ratio": "LCR", "kind": "hqla_l2a", "factor_pct": 85, "side": "asset"},
            {"code": "L2B", "label": "Level 2B", "ratio": "LCR", "kind": "hqla_l2b", "factor_pct": 50, "side": "asset"},
            {"code": "OUT5", "label": "Stable deposits", "ratio": "LCR", "kind": "outflow", "factor_pct": 5, "side": "liability"},
            {"code": "OUT10", "label": "Less stable deposits", "ratio": "LCR", "kind": "outflow", "factor_pct": 10, "side": "liability"},
            {"code": "OUT50", "label": "Committed lines", "ratio": "LCR", "kind": "outflow", "factor_pct": 50, "side": "liability"},
            {"code": "IN50", "label": "Loan inflows", "ratio": "LCR", "kind": "inflow", "factor_pct": 50, "side": "asset"},
            {"code": "ASF100", "label": "Capital", "ratio": "NSFR", "kind": "asf", "factor_pct": 100, "side": "liability"},
            {"code": "ASF95", "label": "Stable deposits", "ratio": "NSFR", "kind": "asf", "factor_pct": 95, "side": "liability"},
            {"code": "RSFL", "label": "Loans", "ratio": "NSFR", "kind": "rsf", "factor_by_maturity": {"lt_6m": 50, "6m_1y": 50, "ge_1y": 85}, "side": "asset"}
        ],
        "gl_map": [
            {"match": "exact", "value": "1001", "line": "L1"},
            {"match": "exact", "value": "1002", "line": "L2A"},
            {"match": "exact", "value": "1003", "line": "L2B"},
            {"match": "prefix", "value": "2001", "line": "OUT5"},
            {"match": "exact", "value": "2002", "line": "OUT10"},
            {"match": "range", "from": "2003", "to": "2003", "line": "OUT50"},
            {"match": "exact", "value": "1101", "line": "IN50"},
            {"match": "exact", "value": "3001", "line": "ASF100"},
            {"match": "exact", "value": "3002", "line": "ASF95"},
            {"match": "exact", "value": "4001", "line": "RSFL"},
            {"match": "exact", "value": "4002", "line": "RSFL"}
        ],
        "caps": {"level2_max_pct": 40, "level2b_max_pct": 15, "inflow_cap_pct": 75, "lcr_min_pct": 100, "nsfr_min_pct": 100}
    })
}

fn map() -> LiqMap {
    LiqMap::parse(&map_json().to_string(), "liquidity_maps/test.json").expect("the test map")
}

fn gl(rows: &[(&str, f64)]) -> DataFrame {
    DataFrame::new(vec![Series::new("GL_Code".into(), rows.iter().map(|r| r.0).collect::<Vec<_>>()), Series::new("Balance".into(), rows.iter().map(|r| r.1).collect::<Vec<_>>())]).expect("gl")
}

fn spec(sign: BalanceSign) -> LiqSpec {
    LiqSpec { gl_col: "GL_Code".into(), balance_col: "Balance".into(), sign, maturity_col: None, maturity_unit: MaturityUnit::Months }
}

fn run(rows: &[(&str, f64)]) -> anyhow::Result<Evaluation> {
    run_df(&gl(rows), &spec(BalanceSign::NormalSide), &map(), "liquidity_maps/test.json")
}

fn near(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() < 0.005, "{what}: {a} vs {b}");
}

#[test]
fn r6_hqla_without_the_caps_biting_is_1390() {
    let ev = run(&[("1001", 1000.0), ("1002", 400.0), ("1003", 100.0)]).expect("runs");
    near(ev.l1, 1000.0, "L1");
    near(ev.l2a, 340.0, "L2A after 85%");
    near(ev.l2b, 50.0, "L2B after 50%");
    near(ev.adj15, 0.0, "no 15% adjustment");
    near(ev.adj40, 0.0, "no 40% adjustment");
    near(ev.hqla, 1390.0, "HQLA");
}

#[test]
fn r6_outflows_inflows_capped_at_75_percent_and_the_lcr_is_278() {
    // outflows 10,000 x 5% + 10,000 x 10% + 1,000 x 50% = 2,000; inflows 3,600 x 50% = 1,800, capped at 1,500
    let ev = run(&[("1001", 1000.0), ("1002", 400.0), ("1003", 100.0), ("2001", 10_000.0), ("2002", 10_000.0), ("2003", 1_000.0), ("1101", 3_600.0)]).expect("runs");
    near(ev.outflows, 2000.0, "outflows");
    near(ev.inflows, 1800.0, "inflows");
    near(ev.inflow_counted, 1500.0, "inflows counted (75% of 2,000)");
    near(ev.nco, 500.0, "net cash outflows");
    near(ev.lcr.expect("defined"), 278.0, "LCR");
    // inflows below the cap count in full
    let ev = run(&[("1001", 1000.0), ("2001", 10_000.0), ("1101", 400.0)]).expect("runs");
    near(ev.inflow_counted, 200.0, "400 x 50%");
    near(ev.nco, 300.0, "500 - 200");
}

#[test]
fn r6_the_level_2_caps_bite_and_the_numbers_follow_the_map() {
    // L1 300, L2A 600 (85% = 510), L2B 400 (50% = 200)
    let ev = run(&[("1001", 300.0), ("1002", 600.0), ("1003", 400.0)]).expect("runs");
    near(ev.adj15, 125.0, "adj15");
    near(ev.adj40, 385.0, "adj40");
    near(ev.hqla, 500.0, "HQLA after the caps");
    // the formula with the Basel numbers 15/85, 15/60 and 2/3
    let (a, b, h) = hqla_after_caps(300.0, 510.0, 200.0, 40.0, 15.0);
    near(a, 125.0, "adj15");
    near(b, 385.0, "adj40");
    near(h, 500.0, "HQLA");
    // other cap percentages in the map change the result (the 15/85 and 2/3 are not in the code)
    let (a2, b2, h2) = hqla_after_caps(300.0, 510.0, 200.0, 50.0, 20.0);
    assert!((a2 - a).abs() > 1.0 || (b2 - b).abs() > 1.0 || (h2 - h).abs() > 1.0, "{a2} {b2} {h2}");
    // with L1 alone there is nothing to cap, and L2B cannot be more than 15% of the total HQLA
    let (a, b, h) = hqla_after_caps(850.0, 0.0, 150.0, 40.0, 15.0);
    near(a, 0.0, "exactly at the cap");
    near(b, 0.0, "40% cap");
    near(h, 1000.0, "HQLA");
}

#[test]
fn r6_the_nsfr_uses_the_residual_maturity_of_the_lines_that_depend_on_it() {
    // ASF 1,000 x 100% + 10,000 x 95% = 10,500; RSF 8,000 x 85% (1 year or more) + 2,000 x 50% (under 6 months) = 7,800
    let mut df = gl(&[("3001", 1000.0), ("3002", 10_000.0), ("4001", 8000.0), ("4002", 2000.0)]);
    df.with_column(Series::new("Residual".into(), &[None, None, Some(24.0), Some(3.0)])).expect("col");
    let s = LiqSpec { maturity_col: Some("Residual".into()), ..spec(BalanceSign::NormalSide) };
    let ev = run_df(&df, &s, &map(), "liquidity_maps/test.json").expect("runs");
    near(ev.asf, 10_500.0, "ASF");
    near(ev.rsf, 7800.0, "RSF");
    near(ev.nsfr.expect("defined"), 134.62, "NSFR");
    let buckets: Vec<String> = ev.lines.iter().filter(|l| l.code == "RSFL").map(|l| l.maturity.clone()).collect();
    assert_eq!(buckets, vec!["<6 months", ">=1 year"], "only the buckets that hold something are listed");
    // the bucket edges: under 6 months, 6 to under 12, 12 and over; days and years are converted
    for (m, unit, want) in [(5.99, MaturityUnit::Months, 50.0), (6.0, MaturityUnit::Months, 50.0), (11.99, MaturityUnit::Months, 50.0), (12.0, MaturityUnit::Months, 85.0), (364.0, MaturityUnit::Days, 50.0), (365.0, MaturityUnit::Days, 85.0), (1.0, MaturityUnit::Years, 85.0), (0.4, MaturityUnit::Years, 50.0)] {
        let mut d = gl(&[("4001", 100.0)]);
        d.with_column(Series::new("Residual".into(), &[m])).expect("col");
        let s = LiqSpec { maturity_col: Some("Residual".into()), maturity_unit: unit, ..spec(BalanceSign::NormalSide) };
        let ev = run_df(&d, &s, &map(), "t.json").expect("runs");
        near(ev.lines.iter().find(|l| l.code == "RSFL").expect("the loans line").factor_pct, want, &format!("{m} {unit:?}"));
    }
    // the bucket factors of 6m_1y differ from the others in a map that says so
    let mut j = map_json();
    j["lines"][9]["factor_by_maturity"] = serde_json::json!({"lt_6m": 10, "6m_1y": 20, "ge_1y": 30});
    let m2 = LiqMap::parse(&j.to_string(), "t.json").expect("map");
    let mut d = gl(&[("4001", 100.0)]);
    d.with_column(Series::new("Residual".into(), &[8.0])).expect("col");
    let s = LiqSpec { maturity_col: Some("Residual".into()), ..spec(BalanceSign::NormalSide) };
    near(run_df(&d, &s, &m2, "t.json").expect("runs").rsf, 20.0, "6 months to under a year: 20%");
    // a line that depends on maturity with no maturity column, and with a blank one
    let e = format!("{:#}", run(&[("4001", 100.0), ("4002", 100.0)]).err().expect("no maturity column"));
    assert!(e.contains("the line 'RSFL' needs the residual maturity and it is not chosen on 2 row(s)") && e.contains("4001, 4002"), "{e}");
    let mut d = gl(&[("4001", 100.0), ("4002", 100.0)]);
    d.with_column(Series::new("Residual".into(), &[Some(3.0), None])).expect("col");
    let e = format!("{:#}", run_df(&d, &s, &map(), "t.json").err().expect("blank maturity"));
    assert!(e.contains("it is blank on 1 row(s)") && e.contains("4002") && !e.contains("4001"), "{e}");
}

#[test]
fn r6_the_ratios_are_flagged_against_their_minimums_and_the_nsfr_must_be_greater_than() {
    let m = map();
    let flag = |lcr: &[(&str, f64)]| -> (Option<f64>, String) {
        let ev = run_df(&gl(lcr), &spec(BalanceSign::NormalSide), &m, "t.json").expect("runs");
        let s = summary_frame(&ev, &m, "t.json").expect("summary");
        let items = strings(&s, "Item");
        let notes = strings(&s, "Note");
        let at = items.iter().position(|i| i.as_deref() == Some("LCR status")).expect("LCR status");
        (ev.lcr, notes[at].clone().unwrap_or_default())
    };
    // HQLA 1,000 against net cash outflows of 1,000 (outflows 20,000 x 5%): LCR exactly 100% meets it (at least)
    let (l, f) = flag(&[("1001", 1000.0), ("2001", 20_000.0)]);
    near(l.expect("defined"), 100.0, "LCR");
    assert_eq!(f, "Meets the minimum");
    let (_, f) = flag(&[("1001", 999.0), ("2001", 20_000.0)]);
    assert_eq!(f, "Below the minimum");
    // NSFR exactly 100% is not "greater than" 100%
    let nsfr = |asf: f64| -> String {
        let mut d = gl(&[("3001", asf), ("4001", 100.0)]);
        d.with_column(Series::new("R".into(), &[None, Some(24.0)])).expect("col");
        let s = LiqSpec { maturity_col: Some("R".into()), ..spec(BalanceSign::NormalSide) };
        let ev = run_df(&d, &s, &m, "t.json").expect("runs");
        let sf = summary_frame(&ev, &m, "t.json").expect("summary");
        let at = strings(&sf, "Item").iter().position(|i| i.as_deref() == Some("NSFR status")).expect("NSFR status");
        strings(&sf, "Note")[at].clone().unwrap_or_default()
    };
    assert_eq!(nsfr(85.0), "Below the minimum", "ASF 85 over RSF 85 is exactly 100%: not greater than");
    assert_eq!(nsfr(85.01), "Meets the minimum");
    // no net outflows: the ratio is not defined, never a made-up number
    let ev = run(&[("1001", 1000.0)]).expect("runs");
    assert!(ev.lcr.is_none() && ev.nsfr.is_none());
    let s = summary_frame(&ev, &map(), "t.json").expect("summary");
    assert!(floats(&s, "Value")[strings(&s, "Item").iter().position(|i| i.as_deref() == Some("LCR")).expect("LCR")].is_none());
}

#[test]
fn r6_one_unmapped_gl_code_stops_the_run_naming_it_and_many_are_listed_ten_at_a_time() {
    let e = run(&[("1001", 100.0), ("9999", 5.0)]).err().expect("unmapped");
    let msg = format!("{e:#}");
    assert!(msg.contains("1 GL code(s) are not in the liquidity map") && msg.contains("9999") && msg.contains("liquidity_maps/test.json"), "{msg}");
    let u = e.downcast_ref::<UnmappedError>().expect("the full list is in the error");
    assert_eq!(u.codes, vec![("9999".to_string(), 5.0)]);
    // 13 codes: the message names the first 10, the error holds all 13 with their balances summed per code
    let mut rows: Vec<(String, f64)> = (0..13).map(|i| (format!("9{i:03}"), 1.0)).collect();
    rows.push(("9000".to_string(), 2.0));
    rows.push(("1001".to_string(), 7.0));
    let r: Vec<(&str, f64)> = rows.iter().map(|(c, b)| (c.as_str(), *b)).collect();
    let e = run(&r).err().expect("unmapped");
    let msg = format!("{e:#}");
    assert!(msg.contains("13 GL code(s)") && msg.contains("the first 10: 9000, 9001") && !msg.contains("9010"), "{msg}");
    let u = e.downcast_ref::<UnmappedError>().expect("error");
    assert_eq!(u.codes.len(), 13);
    assert_eq!(u.codes[0], ("9000".to_string(), 3.0), "9000 appears twice: summed");
    // the file for the owner
    let p = dir("r6u").join("unmapped.csv");
    export_unmapped_csv(&u.codes, &p).expect("writes");
    let text = std::fs::read_to_string(&p).expect("read");
    assert!(text.starts_with("GL code,Balance\n9000,3\n9001,1\n"), "{text}");
    assert_eq!(text.lines().count(), 14);
    // a blank code is unmapped too
    let e = format!("{:#}", run(&[("", 1.0)]).err().expect("blank"));
    assert!(e.contains("1 GL code(s)"), "{e}");
}

#[test]
fn r6_a_code_that_matches_two_lines_of_one_ratio_stops_the_run() {
    let mut j = map_json();
    j["gl_map"].as_array_mut().expect("rules").push(serde_json::json!({"match": "prefix", "value": "10", "line": "L2B"}));
    let m = LiqMap::parse(&j.to_string(), "t.json").expect("map");
    let e = format!("{:#}", run_df(&gl(&[("1001", 1.0)]), &spec(BalanceSign::NormalSide), &m, "liquidity_maps/t.json").err().expect("two lines"));
    assert!(e.contains("'1001' matches two LCR lines") && e.contains("'L1' and 'L2B'") && e.contains("liquidity_maps/t.json"), "{e}");
    // the same line twice (two rules) is fine, and one LCR line and one NSFR line is fine
    let mut j = map_json();
    j["gl_map"].as_array_mut().expect("rules").push(serde_json::json!({"match": "prefix", "value": "10", "line": "L1"}));
    j["gl_map"].as_array_mut().expect("rules").push(serde_json::json!({"match": "exact", "value": "1001", "line": "RSFL"}));
    let m = LiqMap::parse(&j.to_string(), "t.json").expect("map");
    let mut d = gl(&[("1001", 100.0)]);
    d.with_column(Series::new("R".into(), &[24.0])).expect("col");
    let s = LiqSpec { maturity_col: Some("R".into()), ..spec(BalanceSign::NormalSide) };
    let ev = run_df(&d, &s, &m, "t.json").expect("runs");
    near(ev.l1, 100.0, "counted once");
    near(ev.rsf, 85.0, "and in the NSFR");
}

#[test]
fn r6_balances_are_turned_to_the_normal_side_and_a_negative_line_total_gives_a_warning_naming_the_line() {
    // a trial balance with credits negative: deposits of -10,000 are an outflow base of +10,000
    let ev = run_df(&gl(&[("2001", -10_000.0)]), &spec(BalanceSign::DebitPositive), &map(), "t.json").expect("runs");
    near(ev.outflows, 500.0, "10,000 x 5%");
    assert!(ev.warnings.is_empty());
    // a debit balance on a liability line (an overdrawn deposit) is negative after the turn: a warning
    let ev = run_df(&gl(&[("2001", 400.0)]), &spec(BalanceSign::DebitPositive), &map(), "t.json").expect("runs");
    assert_eq!(ev.warnings.len(), 1);
    assert!(ev.warnings[0].contains("'OUT5'") && ev.warnings[0].contains("-400.00") && ev.warnings[0].contains("liability"), "{:?}", ev.warnings);
    // assets are not turned
    let ev = run_df(&gl(&[("1001", 1000.0)]), &spec(BalanceSign::DebitPositive), &map(), "t.json").expect("runs");
    near(ev.l1, 1000.0, "asset line");
    let ev = run_df(&gl(&[("1001", -50.0)]), &spec(BalanceSign::DebitPositive), &map(), "t.json").expect("runs");
    assert!(ev.warnings[0].contains("'L1'"), "{:?}", ev.warnings);
    // the warning is in the summary table
    let s = summary_frame(&ev, &map(), "t.json").expect("summary");
    assert!(strings(&s, "Item").iter().any(|i| i.as_deref() == Some("Warning")));
    // the other sign convention takes the balance as it is
    let ev = run_df(&gl(&[("2001", 10_000.0)]), &spec(BalanceSign::NormalSide), &map(), "t.json").expect("runs");
    near(ev.outflows, 500.0, "positive on the normal side");
}

#[test]
fn r6_bad_columns_and_values_stop_the_run_with_the_count_and_the_first_five_gl_codes() {
    let n = 7;
    let codes: Vec<String> = (0..n).map(|i| format!("100{i}")).collect();
    let df = DataFrame::new(vec![Series::new("GL_Code".into(), codes.clone()), Series::new("Balance".into(), vec![None::<f64>; n])]).expect("gl");
    let e = format!("{:#}", run_df(&df, &spec(BalanceSign::NormalSide), &map(), "t.json").err().expect("blank balances"));
    assert!(e.contains("the balance in 'Balance' is blank or not a number on 7 row(s)") && e.contains("the first 5 GL codes: 1000, 1001, 1002, 1003, 1004") && !e.contains("1005"), "{e}");
    let e = format!("{:#}", run_df(&gl(&[("1001", 1.0)]), &LiqSpec { gl_col: "Nope".into(), ..spec(BalanceSign::NormalSide) }, &map(), "t.json").err().expect("missing"));
    assert!(e.contains("GL code column 'Nope' was not found"), "{e}");
    let text = DataFrame::new(vec![Series::new("GL_Code".into(), &["1001"]), Series::new("Balance".into(), &["1,000"])]).expect("gl");
    let e = format!("{:#}", run_df(&text, &spec(BalanceSign::NormalSide), &map(), "t.json").err().expect("text balance"));
    assert!(e.contains("not a number"), "{e}");
}

#[test]
fn r6_the_map_file_is_checked_and_refused_with_its_name() {
    let bad = |f: &dyn Fn(&mut serde_json::Value)| -> String {
        let mut j = map_json();
        f(&mut j);
        format!("{:#}", LiqMap::parse(&j.to_string(), "liquidity_maps/x.json").err().expect("refused"))
    };
    assert!(bad(&|j| j["lines"][0]["kind"] = serde_json::json!("hqla_l3")).contains("'hqla_l3'"));
    assert!(bad(&|j| j["lines"][0]["ratio"] = serde_json::json!("NSFR")).contains("must be LCR"));
    assert!(bad(&|j| { j["lines"][0].as_object_mut().expect("o").remove("factor_pct"); }).contains("exactly one of factor_pct or factor_by_maturity"));
    assert!(bad(&|j| j["lines"][0]["factor_by_maturity"] = serde_json::json!({"lt_6m": 1, "6m_1y": 2, "ge_1y": 3})).contains("exactly one of factor_pct or factor_by_maturity"));
    assert!(bad(&|j| j["lines"][4]["factor_by_maturity"] = serde_json::json!({"lt_6m": 1, "6m_1y": 2, "ge_1y": 3})).contains("exactly one"));
    assert!(bad(&|j| j["gl_map"][0]["line"] = serde_json::json!("NOPE")).contains("'NOPE'"));
    assert!(bad(&|j| j["gl_map"][5]["from"] = serde_json::json!("9999")).contains("from not above to"));
    assert!(bad(&|j| j["gl_map"][0]["match"] = serde_json::json!("suffix")).contains("match exact or prefix"));
    assert!(bad(&|j| j["lines"][1]["code"] = serde_json::json!("L1")).contains("'L1' twice"));
    assert!(bad(&|j| j["lines"][0]["side"] = serde_json::json!("both")).contains("not asset or liability"));
    assert!(bad(&|j| j["caps"]["level2b_max_pct"] = serde_json::json!(50)).contains("level2b_max_pct cannot be above"));
    assert!(bad(&|j| j["caps"]["nsfr_test"] = serde_json::json!("above")).contains("at_least or greater_than"));
    assert!(bad(&|j| j["source"] = serde_json::json!("")).contains("needs its \"source\""));
    assert!(bad(&|j| j["as_of"] = serde_json::json!("30/06/2026")).contains("YYYY-MM-DD"));
    let e = format!("{:#}", LiqMap::parse("{ nope", "liquidity_maps/x.json").err().expect("json"));
    assert!(e.contains("liquidity_maps/x.json is not valid"), "{e}");
}

#[test]
fn r6_the_shipped_map_is_a_basel_sample_written_only_when_missing_and_never_called_bangladesh_banks() {
    let src = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join("liquidity_maps").join("sample_basel.json")).expect("the source copy is in the repository");
    assert_eq!(src, SAMPLE_MAP);
    let data = dir("r6map");
    set_data_dir_override(Some(data.clone()));
    assert_eq!(list_maps(), vec!["sample_basel.json".to_string()], "written on first use");
    let m = load_map("sample_basel").expect("loads");
    assert!(m.sample, "marked sample");
    assert!(m.source.contains("SAMPLE") && m.source.contains("not the Bangladesh Bank map") && m.source.contains("DOS Circular No. 01") && m.source.contains("SDAD Circular Letter No. 11"));
    assert!(m.lines.iter().any(|l| l.kind == "rsf" && l.factor_by_maturity.is_some()), "a line by maturity");
    // the Basel factors
    let f = |c: &str| m.line(c).and_then(|l| l.factor_pct).unwrap_or(f64::NAN);
    assert_eq!((f("L1_CASH"), f("L2A_SEC"), f("L2B_SEC"), f("ASF_CAPITAL")), (100.0, 85.0, 50.0, 100.0));
    assert_eq!((m.caps.level2_max_pct, m.caps.level2b_max_pct, m.caps.inflow_cap_pct, m.caps.lcr_min_pct, m.caps.nsfr_min_pct, m.caps.nsfr_test.as_str()), (40.0, 15.0, 75.0, 100.0, 100.0, "greater_than"));
    // an edited file is never overwritten
    let p = data.join("liquidity_maps").join("sample_basel.json");
    let edited = SAMPLE_MAP.replace("\"sample\": true", "\"sample\": false");
    std::fs::write(&p, &edited).expect("edit");
    assert!(!load_map("sample_basel").expect("loads").sample);
    assert_eq!(std::fs::read_to_string(&p).expect("read"), edited);
    // another file in the folder is listed and read
    std::fs::write(data.join("liquidity_maps").join("bank.json"), map_json().to_string()).expect("write");
    assert_eq!(list_maps(), vec!["bank.json".to_string(), "sample_basel.json".to_string()]);
    assert!(load_map("../x").is_err(), "a name is a file name, not a path");
    set_data_dir_override(None);
    // the sample's summary says so
    let ev = Evaluation::default();
    let s = summary_frame(&ev, &LiqMap::parse(SAMPLE_MAP, "sample_basel.json").expect("parses"), "liquidity_maps/sample_basel.json").expect("summary");
    let notes = strings(&s, "Note").into_iter().flatten().collect::<Vec<_>>();
    assert!(notes.iter().any(|n| n.contains("SAMPLE (Basel), not Bangladesh Bank's")), "{notes:?}");
}

#[test]
fn r6_the_tab_sources_give_the_two_tables() {
    let tmp = dir("r6tab");
    let rows = [("1001", 1000.0), ("1002", 400.0), ("1003", 100.0), ("2001", 10_000.0), ("2002", 10_000.0), ("2003", 1_000.0), ("1101", 3_600.0)];
    let input = parquet(&tmp, "gl", &gl(&rows));
    let make = |view| TabSource::Liquidity { source: Box::new(input.clone()), steps: vec![], spec: spec(BalanceSign::NormalSide), map: Box::new(map()), map_file: "liquidity_maps/test.json".into(), view };
    let lines = compile_tab_lazy(&make(LiquidityView::Lines), &[], &tmp).and_then(|lf| Ok(lf.collect()?)).expect("lines");
    assert_eq!(lines.get_column_names().iter().map(|n| n.to_string()).collect::<Vec<_>>(), LINE_COLUMNS.to_vec());
    let codes = strings(&lines, "Line");
    let at = |c: &str| codes.iter().position(|x| x.as_deref() == Some(c)).expect("line");
    assert_eq!(floats(&lines, "Amount")[at("L2A")], Some(400.0));
    assert_eq!(floats(&lines, "Factor %")[at("L2A")], Some(85.0));
    assert_eq!(floats(&lines, "Weighted amount")[at("L2A")], Some(340.0));
    assert_eq!(strings(&lines, "Kind")[at("OUT50")].as_deref(), Some("outflow"));
    let summary = compile_tab_lazy(&make(LiquidityView::Summary), &[], &tmp).and_then(|lf| Ok(lf.collect()?)).expect("summary");
    assert_eq!(summary.get_column_names().iter().map(|n| n.to_string()).collect::<Vec<_>>(), SUMMARY_COLUMNS.to_vec());
    let items = strings(&summary, "Item");
    let v = |i: &str| floats(&summary, "Value")[items.iter().position(|x| x.as_deref() == Some(i)).expect("item")].unwrap_or(f64::NAN);
    near(v("HQLA after caps"), 1390.0, "HQLA");
    near(v("Net cash outflows"), 500.0, "NCO");
    near(v("LCR"), 278.0, "LCR");
    // an unmapped code in the tab's table is an error of the tab, naming the code
    let bad = parquet(&tmp, "gl_bad", &gl(&[("1001", 1.0), ("7777", 1.0)]));
    let src = TabSource::Liquidity { source: Box::new(bad), steps: vec![], spec: spec(BalanceSign::NormalSide), map: Box::new(map()), map_file: "liquidity_maps/test.json".into(), view: LiquidityView::Summary };
    let e = format!("{:#}", compile_tab_lazy(&src, &[], &tmp).and_then(|lf| Ok(lf.collect()?)).err().expect("unmapped"));
    assert!(e.contains("7777"), "{e}");
}
