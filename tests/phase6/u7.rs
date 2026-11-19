//! U7: pick filter values from a list (the engine side: the values and counts, the typed matching,
//! the comma in a value, blanks, Not in list, large columns, old filters, the Python export).

use crate::common::*;
use micro_parquet::models::*;
use micro_parquet::value_picker::*;
use polars::prelude::*;
use std::collections::BTreeMap;

const ZONES: [&str; 6] = ["DHAKA NORTH", "WEST-1 ZONE", "Khulna, West", "CTG SOUTH", "", "  "];

/// 250 rows (three pages of 100): Zone cycles through six values, the last two blank (empty text and spaces), plus a null.
fn book() -> DataFrame {
    let n = 250usize;
    let zones: Vec<Option<String>> = (0..n).map(|i| if i == 7 { None } else { Some(ZONES[i % 6].to_string()) }).collect();
    DataFrame::new(vec![
        Series::new("Zone".into(), zones),
        Series::new("Balance".into(), (0..n).map(|i| 1000.0 + i as f64).collect::<Vec<f64>>()),
        Series::new("Rate".into(), (0..n).map(|i| [9.0, 9.5, 10.0][i % 3]).collect::<Vec<f64>>()),
        dates("Opndat", &(0..n).map(|i| ["2026-01-05", "2026-02-01", "2026-03-15"][i % 3]).collect::<Vec<_>>()),
    ])
    .expect("the book")
}

fn cond(op: CondOp, values: &[&str], or_blank: bool) -> FilterCondition {
    FilterCondition { column: "Zone".into(), op, value: values.join(", "), values: values.iter().map(|s| s.to_string()).collect(), or_blank, ..Default::default() }
}

fn keep(df: &DataFrame, conds: Vec<FilterCondition>, keep_matching: bool) -> DataFrame {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let name = format!("u7_run{}", N.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    run_steps(&name, df, &[TransformStep::FilterGroup { conditions: conds, join: GroupJoin::And, keep_matching }]).expect("the filter runs")
}

fn zones_of(df: &DataFrame) -> Vec<Option<String>> {
    strings(df, "Zone")
}

#[test]
fn u7_the_picker_lists_every_distinct_value_of_the_whole_tab_with_its_count_and_blanks_as_one_entry() {
    let df = book();
    let p = picker_values(df.clone().lazy(), "Zone", None, PICKER_LIMIT).expect("values");
    assert_eq!(p.distinct, 4 + 1, "four zones and the blank entry");
    assert!(!p.more);
    let map: BTreeMap<Option<String>, u64> = p.values.iter().cloned().collect();
    let count = |z: &str| zones_of(&df).iter().filter(|v| v.as_deref() == Some(z)).count() as u64;
    for z in ["DHAKA NORTH", "WEST-1 ZONE", "Khulna, West", "CTG SOUTH"] {
        assert_eq!(map[&Some(z.to_string())], count(z), "{z}");
    }
    let blanks = zones_of(&df).iter().filter(|v| v.as_deref().is_none_or(|t| t.trim().is_empty())).count() as u64;
    assert_eq!(map[&None], blanks, "null, empty and spaces are one blank entry");
    assert_eq!(p.values.iter().map(|(_, n)| n).sum::<u64>(), 250, "every row is counted: the whole tab, not a page");
    assert!(p.values.windows(2).all(|w| w[0].1 >= w[1].1), "the most frequent first");
    // numbers and dates are listed as the text the picker shows
    let r = picker_values(df.clone().lazy(), "Rate", None, PICKER_LIMIT).expect("rates");
    let rates: Vec<String> = r.values.iter().filter_map(|(k, _)| k.clone()).collect();
    assert!(rates.contains(&"9".to_string()) && rates.contains(&"9.5".to_string()) && rates.contains(&"10".to_string()), "{rates:?}");
    let d = picker_values(df.lazy(), "Opndat", None, PICKER_LIMIT).expect("dates");
    assert!(d.values.iter().any(|(k, _)| k.as_deref() == Some("2026-01-05")));
}

#[test]
fn u7_two_picked_values_give_their_rows_and_a_value_with_a_comma_is_one_value() {
    let df = book();
    let all = zones_of(&df);
    let want = |names: &[&str]| all.iter().filter(|v| v.as_deref().is_some_and(|z| names.contains(&z))).count();
    let two = keep(&df, vec![cond(CondOp::InList, &["DHAKA NORTH", "WEST-1 ZONE"], false)], true);
    assert_eq!(two.height(), want(&["DHAKA NORTH", "WEST-1 ZONE"]));
    assert!(zones_of(&two).iter().all(|z| matches!(z.as_deref(), Some("DHAKA NORTH") | Some("WEST-1 ZONE"))));
    // "Khulna, West" is one value (the comma is not a separator)
    let k = keep(&df, vec![cond(CondOp::Equals, &["Khulna, West"], false)], true);
    assert_eq!(k.height(), want(&["Khulna, West"]));
    assert!(k.height() > 0);
    let k2 = keep(&df, vec![cond(CondOp::InList, &["Khulna, West", "CTG SOUTH"], false)], true);
    assert_eq!(k2.height(), want(&["Khulna, West", "CTG SOUTH"]));
    // the old typed list "Khulna, West" split at the comma: no row is "Khulna" or "West"
    let old = FilterCondition { column: "Zone".into(), op: CondOp::InList, value: "Khulna, West".into(), ..Default::default() };
    assert_eq!(keep(&df, vec![old], true).height(), 0, "an old comma-separated list still splits (and replays as it did)");
}

#[test]
fn u7_blank_adds_the_blank_rows_and_not_in_list_leaves_the_picked_values_out() {
    let df = book();
    let all = zones_of(&df);
    let blank_rows = all.iter().filter(|v| v.as_deref().is_none_or(|t| t.trim().is_empty())).count();
    let named = |z: &str| all.iter().filter(|v| v.as_deref() == Some(z)).count();
    // "(blank)" with one value: that value's rows and the blank rows
    let with_blank = keep(&df, vec![cond(CondOp::Equals, &["CTG SOUTH"], true)], true);
    assert_eq!(with_blank.height(), named("CTG SOUTH") + blank_rows);
    // only "(blank)": the blank rows
    let only_blank = keep(&df, vec![cond(CondOp::Equals, &[], true)], true);
    assert_eq!(only_blank.height(), blank_rows);
    // Not in list: the picked values are out; the blank rows ("", spaces and null) are not in the list, so they stay
    let not_in = keep(&df, vec![cond(CondOp::NotInList, &["DHAKA NORTH", "CTG SOUTH"], false)], true);
    assert_eq!(not_in.height(), named("WEST-1 ZONE") + named("Khulna, West") + blank_rows);
    assert!(zones_of(&not_in).iter().flatten().all(|z| z != "DHAKA NORTH" && z != "CTG SOUTH"));
    // "(blank)" picked as well: blanks are left out for certain, so Delete matching keeps only the others
    let gone = keep(&df, vec![cond(CondOp::NotInList, &["DHAKA NORTH"], true)], false);
    assert_eq!(gone.height(), 250 - (named("DHAKA NORTH") + blank_rows) + 0, "Delete matching of 'not (in list or blank)' keeps the picked rows");
    // one picked value with not-equals is the old comparison
    let ne = keep(&df, vec![cond(CondOp::NotEquals, &["DHAKA NORTH"], false)], true);
    assert!(zones_of(&ne).iter().flatten().all(|z| z != "DHAKA NORTH"));
}

#[test]
fn u7_numbers_and_dates_are_matched_as_typed_values_not_as_text() {
    let df = book();
    let rate = |vals: &[&str]| FilterCondition { column: "Rate".into(), op: CondOp::InList, values: vals.iter().map(|s| s.to_string()).collect(), ..Default::default() };
    // "9.50" is the number 9.5 (as text it would not match "9.5")
    let out = keep(&df, vec![rate(&["9.50", "10"])], true);
    let want = (0..250).filter(|i| i % 3 != 0).count();
    assert_eq!(out.height(), want);
    let date = FilterCondition { column: "Opndat".into(), op: CondOp::InList, values: vec!["2026-03-15".into(), "2026-01-05".into()], ..Default::default() };
    assert_eq!(keep(&df, vec![date], true).height(), (0..250).filter(|i| i % 3 != 1).count());
    // a value that does not fit the column is an error naming it
    let bad = FilterCondition { column: "Rate".into(), op: CondOp::InList, values: vec!["abc".into()], ..Default::default() };
    let err = run_steps("u7_bad", &df, &[TransformStep::FilterGroup { conditions: vec![bad], join: GroupJoin::And, keep_matching: true }]).expect_err("not a number");
    assert!(format!("{err:#}").contains("'abc' is not a number"), "{err:#}");
    let bad = FilterCondition { column: "Opndat".into(), op: CondOp::Equals, values: vec!["05/01/2026".into()], ..Default::default() };
    let err = run_steps("u7_bad2", &df, &[TransformStep::FilterGroup { conditions: vec![bad], join: GroupJoin::And, keep_matching: true }]).expect_err("not a date");
    assert!(format!("{err:#}").contains("YYYY-MM-DD"), "{err:#}");
}

#[test]
fn u7_a_column_with_2000_distinct_values_shows_the_1000_most_frequent_and_search_finds_the_others() {
    let n = 3_000usize;
    // k0000..k1999: the first 1,000 values appear twice, the other 1,000 once
    let vals: Vec<String> = (0..n).map(|i| if i < 2000 { format!("k{:04}", i % 1000) } else { format!("k{:04}", 1000 + (i - 2000)) }).collect();
    let df = DataFrame::new(vec![Series::new("Code".into(), vals), Series::new("V".into(), vec![1.0f64; n])]).expect("frame");
    let p = picker_values(df.clone().lazy(), "Code", None, PICKER_LIMIT).expect("values");
    assert_eq!((p.values.len(), p.distinct, p.more), (1000, 2000, true));
    assert!(p.values.iter().all(|(_, c)| *c == 2), "the most frequent ones are listed");
    assert!(!p.values.iter().any(|(k, _)| k.as_deref() == Some("k1500")), "a value outside the top 1,000 is not listed");
    // the search asks the worker for all values
    let s = picker_values(df.clone().lazy(), "Code", Some("K1500"), PICKER_LIMIT).expect("search");
    assert_eq!(s.values, vec![(Some("k1500".to_string()), 1)], "not case-sensitive, and found beyond the top 1,000");
    let s = picker_values(df.lazy(), "Code", Some("k15"), PICKER_LIMIT).expect("search");
    assert_eq!(s.distinct, 100, "k1500..k1599");
    assert_eq!(picker_summary(3, false, Some(42)), "3 of 42 selected");
    assert_eq!(picker_summary(3, true, Some(2000)), "4 of 2,000 selected");
    assert_eq!(picker_summary(0, false, None), "pick values");
    assert_eq!(picker_summary(2, false, None), "2 selected");
}

#[test]
fn u7_an_old_comma_separated_in_list_and_old_conditions_without_the_new_fields_replay_identically() {
    let old = r#"{"FilterGroup":{"conditions":[{"column":"Zone","op":"InList","value":"DHAKA NORTH, CTG SOUTH"}],"join":"And","keep_matching":true}}"#;
    let step: TransformStep = serde_json::from_str(old).expect("an old filter loads");
    let TransformStep::FilterGroup { conditions, .. } = &step else { panic!("a filter group") };
    assert!(conditions[0].values.is_empty() && !conditions[0].or_blank && !conditions[0].uses_values());
    let df = book();
    let out = run_steps("u7_old", &df, &[step]).expect("runs");
    let want = zones_of(&df).iter().filter(|z| matches!(z.as_deref(), Some("DHAKA NORTH") | Some("CTG SOUTH"))).count();
    assert_eq!(out.height(), want);
    // the Phase 3 macro on the fixture book
    assert_fixture_replays("phase6_filter_inlist_old", &["FilterGroup"]);
    // an old conditional-column rule has no values
    let rule: CondRule = serde_json::from_str(r#"{"col_name":"Zone","op":"InList","compare_val":"A, B","output_val":"x"}"#).expect("an old rule loads");
    assert!(rule.values.is_empty());
    // chips and labels
    let c = cond(CondOp::InList, &["A", "B", "C", "D", "E"], true);
    assert_eq!(c.describe(), "Zone is in list A; B; C (+2); (blank)");
    assert_eq!(cond(CondOp::NotInList, &["A"], false).describe(), "Zone is not in list A");
}

#[test]
fn u7_a_conditional_column_rule_with_picked_values_matches_them() {
    let df = book();
    let rule = CondRule { col_name: "Zone".into(), op: FilterOp::InList, compare_val: String::new(), output_val: "Core".into(), values: vec!["DHAKA NORTH".into(), "Khulna, West".into()], ..Default::default() };
    let step = TransformStep::ConditionalColumn { new_col: "Flag".into(), rules: vec![rule], fallback: "Other".into(), output_as_float: false, blank_result: BlankRule::FallThrough };
    let out = run_steps("u7_cc", &df, &[step]).expect("runs");
    let (flags, zones) = (strings(&out, "Flag"), zones_of(&out));
    for (f, z) in flags.iter().zip(zones.iter()) {
        let core = matches!(z.as_deref(), Some("DHAKA NORTH") | Some("Khulna, West"));
        assert_eq!(f.as_deref(), Some(if core { "Core" } else { "Other" }));
    }
    let not = CondRule { col_name: "Zone".into(), op: FilterOp::NotInList, output_val: "Out".into(), values: vec!["DHAKA NORTH".into()], ..Default::default() };
    let step = TransformStep::ConditionalColumn { new_col: "Flag".into(), rules: vec![not], fallback: "In".into(), output_as_float: false, blank_result: BlankRule::FallThrough };
    let out = run_steps("u7_cc2", &df, &[step]).expect("runs");
    let n_out = strings(&out, "Flag").iter().filter(|f| f.as_deref() == Some("Out")).count();
    assert_eq!(n_out, zones_of(&df).iter().filter(|z| z.as_deref() != Some("DHAKA NORTH")).count(), "blank rows are not in the list, so they are 'Out' too (as in a filter)");
}

#[test]
fn u7_the_python_export_has_the_picked_values_with_their_commas() {
    let step = TransformStep::FilterGroup { conditions: vec![cond(CondOp::InList, &["Khulna, West", "CTG SOUTH"], true)], join: GroupJoin::And, keep_matching: true };
    let py = micro_parquet::macros::generate_python_polars_code(std::path::Path::new("x.parquet"), true, &IngestionConfig::default(), &[step], None).expect("python");
    assert!(py.contains("def _mp_in_values"), "{py}");
    assert!(py.contains("_mp_in_values(\"Zone\", [\"Khulna, West\", \"CTG SOUTH\"], lf.collect_schema()[\"Zone\"], True, False)"), "{py}");
    let none = micro_parquet::macros::generate_python_polars_code(std::path::Path::new("x.parquet"), true, &IngestionConfig::default(), &[TransformStep::FilterGroup { conditions: vec![FilterCondition { column: "Z".into(), op: CondOp::InList, value: "a, b".into(), ..Default::default() }], join: GroupJoin::And, keep_matching: true }], None).expect("python");
    assert!(!none.contains("_mp_in_values"), "an old list is exported as before");
}

fn one_col(name: &str, s: Series) -> DataFrame {
    DataFrame::new(vec![s.with_name(name.into())]).expect("frame")
}

fn picked(col: &str, values: &[&str]) -> FilterCondition {
    FilterCondition { column: col.into(), op: CondOp::InList, values: values.iter().map(|s| s.to_string()).collect(), ..Default::default() }
}

#[test]
fn g1_picked_numbers_are_compared_in_the_columns_own_type_never_through_f64() {
    // two 17-digit values that are the same f64
    assert_eq!(12345678901234567_i64 as f64, 12345678901234568_i64 as f64, "the premise: f64 cannot tell them apart");
    let df = one_col("Acct", Series::new("x".into(), [12345678901234567_i64, 12345678901234568, 5]));
    let one = keep_col(&df, vec![picked("Acct", &["12345678901234567"])]);
    assert_eq!(one.height(), 1, "picking the first returns one row");
    assert_eq!(one.column("Acct").expect("col").i64().expect("i64").get(0), Some(12345678901234567));
    let both = keep_col(&df, vec![picked("Acct", &["12,345,678,901,234,568", "5"])]);
    assert_eq!(both.height(), 2);
    // unsigned 64-bit above i64::MAX
    let big = 18_000_000_000_000_000_001_u64;
    let u = one_col("N", Series::new("x".into(), [big, big + 1, 3_u64]));
    assert_eq!(keep_col(&u, vec![picked("N", &["18000000000000000001"])]).height(), 1);
    // decimals compare as decimal text: "12.5" is "12.50" in a two-decimal column
    let dec = Series::new("x".into(), [12.5_f64, 12.51, 7.0]).cast(&DataType::Decimal(Some(18), Some(2))).expect("decimal");
    let d = one_col("Amt", dec);
    assert_eq!(keep_col(&d, vec![picked("Amt", &["12.5"])]).height(), 1);
    assert_eq!(keep_col(&d, vec![picked("Amt", &["12.51", "7"])]).height(), 2);
    let err = run_steps("g1_dec", &d, &[TransformStep::FilterGroup { conditions: vec![picked("Amt", &["12.505"])], join: GroupJoin::And, keep_matching: true }]).expect_err("too many decimals");
    assert!(format!("{err:#}").contains("'12.505' is not a number with at most 2 decimals"), "{err:#}");
    // a fraction in a whole-number column is refused naming the value
    let err = run_steps("g1_int", &df, &[TransformStep::FilterGroup { conditions: vec![picked("Acct", &["5.5"])], join: GroupJoin::And, keep_matching: true }]).expect_err("not whole");
    assert!(format!("{err:#}").contains("'5.5' is not a whole number"), "{err:#}");
}

fn keep_col(df: &DataFrame, conds: Vec<FilterCondition>) -> DataFrame {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let name = format!("u7_col{}", N.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    run_steps(&name, df, &[TransformStep::FilterGroup { conditions: conds, join: GroupJoin::And, keep_matching: true }]).expect("the filter runs")
}

#[test]
fn g6_typed_and_picked_not_in_list_keep_blank_rows_unless_blank_is_picked() {
    let df = one_col("Zone", Series::new("x".into(), [Some("A"), Some(""), None, Some("B")]));
    // picker
    let not_a = keep_col(&df, vec![FilterCondition { column: "Zone".into(), op: CondOp::NotInList, values: vec!["A".into()], ..Default::default() }]);
    assert_eq!(not_a.height(), 3, "picker: '', null and B stay");
    let not_a_blank = keep_col(&df, vec![FilterCondition { column: "Zone".into(), op: CondOp::NotInList, values: vec!["A".into()], or_blank: true, ..Default::default() }]);
    assert_eq!(strings(&not_a_blank, "Zone"), vec![Some("B".to_string())], "with (blank): only B");
    // typed list: the same rows
    let typed = keep_col(&df, vec![FilterCondition { column: "Zone".into(), op: CondOp::NotInList, value: "A".into(), ..Default::default() }]);
    assert_eq!(typed.height(), 3, "typed: the same three rows");
}
