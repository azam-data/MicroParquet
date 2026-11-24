//! U3: custom sort lists and the Tenor order (sort step, pivot fields, list files, Python export).

use crate::common::*;
use micro_parquet::models::*;
use micro_parquet::pivot_table::*;
use micro_parquet::sort_lists::*;
use polars::prelude::*;

fn sl(column: &str, descending: bool, order: SortOrder) -> SortLevel {
    SortLevel { column: column.into(), descending, order }
}

fn terms(values: &[Option<&str>]) -> DataFrame {
    DataFrame::new(vec![
        Series::new("Term Period".into(), values.iter().map(|v| v.map(|s| s.to_string())).collect::<Vec<_>>()),
        Series::new("Id".into(), (1..=values.len() as i64).collect::<Vec<_>>()),
    ])
    .expect("terms")
}

fn sorted_terms(name: &str, values: &[Option<&str>], level: SortLevel) -> Vec<Option<String>> {
    let out = run_steps(name, &terms(values), &[TransformStep::SortRows { levels: vec![level] }]).expect("the sort runs");
    strings(&out, "Term Period")
}

fn some(v: &[&str]) -> Vec<Option<String>> {
    v.iter().map(|s| Some(s.to_string())).collect()
}

fn high_medium_low() -> SortOrder {
    SortOrder::List { name: "Risk".into(), values: vec!["High".into(), "Medium".into(), "Low".into()], case_insensitive: true }
}

#[test]
fn u3_tenor_sorts_by_days_and_unparsed_values_follow_in_natural_order() {
    let input: Vec<Option<&str>> = ["3y", "1m", "400D", "2m", "1y", "100D", "6m", "3m", "2y", "200D"].iter().map(|s| Some(*s)).collect();
    let got = sorted_terms("u3_tenor", &input, sl("Term Period", false, SortOrder::Tenor));
    assert_eq!(got, some(&["1m", "2m", "3m", "100D", "6m", "200D", "1y", "400D", "2y", "3y"]));
    // "abc" goes after the parsed ones, a blank last of all, and the unparsed are counted
    let mut with_junk = input.clone();
    with_junk.extend([Some("abc"), None, Some("O/N"), Some("1 week")]);
    let got = sorted_terms("u3_tenor2", &with_junk, sl("Term Period", false, SortOrder::Tenor));
    assert_eq!(got[..3].to_vec(), some(&["O/N", "1 week", "1m"]));
    assert_eq!(got[12..].to_vec(), vec![Some("abc".to_string()), None]);
    let texts: Vec<&str> = with_junk.iter().flatten().copied().collect();
    assert_eq!(count_unparsed_tenors(texts), 1, "abc is the one value that is not a tenor");
    // longest first reverses the parsed ones; the unparsed still follow
    let got = sorted_terms("u3_tenor3", &with_junk, sl("Term Period", true, SortOrder::Tenor));
    assert_eq!(got[0].as_deref(), Some("3y"));
    assert_eq!(got[12..].to_vec(), vec![Some("abc".to_string()), None]);
}

#[test]
fn u3_a_custom_list_orders_listed_values_first_then_the_rest_naturally_and_blanks_last() {
    let input = [Some("Low"), Some("High"), Some("Medium"), None, Some("Other")];
    let got = sorted_terms("u3_list_asc", &input, sl("Term Period", false, high_medium_low()));
    assert_eq!(got, vec![Some("High".into()), Some("Medium".into()), Some("Low".into()), Some("Other".into()), None]);
    let got = sorted_terms("u3_list_desc", &input, sl("Term Period", true, high_medium_low()));
    assert_eq!(got, vec![Some("Low".into()), Some("Medium".into()), Some("High".into()), Some("Other".into()), None], "Z to A reverses the list; Other and the blank stay last");
    // case-insensitive by default; trailing spaces do not matter; a blank text is a blank
    let got = sorted_terms("u3_list_case", &[Some("low"), Some(" HIGH "), Some("  ")], sl("Term Period", false, high_medium_low()));
    assert_eq!(got[..2].to_vec(), some(&[" HIGH ", "low"]));
    // case-sensitive: "low" is not listed, so it follows the listed values
    let strict = SortOrder::List { name: "Risk".into(), values: vec!["High".into(), "Medium".into(), "Low".into()], case_insensitive: false };
    let got = sorted_terms("u3_list_strict", &[Some("low"), Some("Low"), Some("High")], sl("Term Period", false, strict));
    assert_eq!(got, some(&["High", "Low", "low"]));
}

#[test]
fn u3_a_second_level_breaks_ties_and_other_types_refuse_a_list() {
    let df = DataFrame::new(vec![
        Series::new("Grade".into(), &["Low", "High", "Low", "High"]),
        Series::new("Balance".into(), &[1.0, 2.0, 3.0, 4.0]),
    ])
    .expect("frame");
    let out = run_steps("u3_two", &df, &[TransformStep::SortRows { levels: vec![sl("Grade", false, high_medium_low()), sl("Balance", true, SortOrder::Natural)] }]).expect("runs");
    assert_eq!(floats(&out, "Balance"), vec![Some(4.0), Some(2.0), Some(3.0), Some(1.0)]);
    let err = run_steps("u3_num", &df, &[TransformStep::SortRows { levels: vec![sl("Balance", false, SortOrder::Tenor)] }]).expect_err("a number is not a tenor");
    assert!(format!("{err:#}").contains("text columns"), "{err:#}");
}

#[test]
fn u3_a_pivot_with_a_list_on_its_row_field_lists_the_rows_in_list_order() {
    let df = terms(&[Some("Low"), Some("High"), Some("Other"), Some("Medium"), Some("High")]);
    let pv = |orders: Vec<PivotFieldOrder>| {
        let spec = PivotSpec {
            rows: vec!["Term Period".into()],
            values: vec![PivotValue { field: "Id".into(), agg: PivotAgg::Sum, weight_col: None, time: None }],
            row_totals: true,
            column_totals: true,
            field_orders: orders,
            ..Default::default()
        };
        strings(&build_pivot(df.clone().lazy(), &spec).expect("pivot").df, "Term Period")
    };
    assert_eq!(pv(vec![]), vec![Some("High".into()), Some("Low".into()), Some("Medium".into()), Some("Other".into()), Some("Grand total".into())], "natural order without a list");
    let got = pv(vec![PivotFieldOrder { field: "Term Period".into(), order: high_medium_low() }]);
    assert_eq!(got[..4].to_vec(), some(&["High", "Medium", "Low", "Other"]));
    let got = pv(vec![PivotFieldOrder { field: "Term Period".into(), order: SortOrder::Tenor }]);
    assert_eq!(got.len(), 5);
}

#[test]
fn u3_pivot_columns_follow_a_tenor_order_too() {
    let df = DataFrame::new(vec![
        Series::new("Tenor".into(), &["1y", "3m", "100D", "6m", "1m"]),
        Series::new("Bank".into(), &["A", "A", "A", "A", "A"]),
        Series::new("Balance".into(), &[1.0, 2.0, 3.0, 4.0, 5.0]),
    ])
    .expect("frame");
    let spec = PivotSpec {
        rows: vec!["Bank".into()],
        columns: vec!["Tenor".into()],
        values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None }],
        row_totals: false,
        column_totals: false,
        field_orders: vec![PivotFieldOrder { field: "Tenor".into(), order: SortOrder::Tenor }],
        ..Default::default()
    };
    let out = build_pivot(df.lazy(), &spec).expect("pivot").df;
    let names: Vec<String> = out.get_column_names().iter().map(|s| s.to_string()).collect();
    assert_eq!(names, vec!["Bank", "1m", "3m", "100D", "6m", "1y"]);
}

#[test]
fn u3_old_sort_and_pivot_steps_without_order_fields_load_and_replay() {
    let old_sort = r#"{"SortRows":{"levels":[{"column":"Brancd","descending":false},{"column":"Balance","descending":true}]}}"#;
    let step: TransformStep = serde_json::from_str(old_sort).expect("an old sort loads");
    let TransformStep::SortRows { levels } = &step else { panic!("a sort") };
    assert!(levels.iter().all(|l| l.order == SortOrder::Natural));
    let old_pivot = r#"{"PivotTable":{"rows":["Brancd"],"columns":[],"values":[{"field":"Balance","agg":"Sum","weight_col":null}],"filters":[],"row_totals":true,"column_totals":true,"subtotals":false,"show_as":"NoCalculation","scale":"Global","sort_by_value":null}}"#;
    let step: TransformStep = serde_json::from_str(old_pivot).expect("an old pivot loads");
    assert!(step.as_pivot_spec().expect("a pivot").field_orders.is_empty());
    // saved again, an unset order is written but reads back as natural
    let text = serde_json::to_string(&TransformStep::SortRows { levels: vec![sl("A", false, SortOrder::Natural)] }).expect("json");
    let back: TransformStep = serde_json::from_str(&text).expect("back");
    assert_eq!(back.sort_levels().expect("a sort").to_vec(), vec![sl("A", false, SortOrder::Natural)]);
}

#[test]
fn u3_a_phase_four_sort_and_pivot_macro_replays_identically() {
    assert_fixture_replays("phase6_sort_pivot_unset", &["SortRows", "PivotTable"]);
}

#[test]
fn u3_list_files_save_load_rename_and_delete_and_name_a_suggested_column() {
    let d = dir("u3_files");
    let dir = d.join("sort_lists");
    let list = SortList { name: "Term Period order".into(), values: vec!["1M".into(), " 3M ".into(), "".into(), "6M".into()], case_insensitive: true, apply_to_columns: vec!["Term Period".into()] };
    let path = save_list(&dir, &list).expect("saved");
    assert!(path.ends_with("Term Period order.json"));
    let (lists, problems) = load_lists(&dir);
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(lists[0].values, vec!["1M", "3M", "6M"], "values are trimmed and blank lines dropped");
    assert_eq!(suggested_for(&lists, "term period").map(|l| l.name.as_str()), Some("Term Period order"));
    assert!(suggested_for(&lists, "Other").is_none());
    // the file holds name, values, case_insensitive and apply_to_columns
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    for k in ["name", "values", "case_insensitive", "apply_to_columns"] {
        assert!(json.get(k).is_some(), "{k}");
    }
    // a file with only a name and values still loads, with the defaults
    std::fs::write(dir.join("min.json"), r#"{"name":"Min","values":["a","b"]}"#).expect("write");
    let (lists, _) = load_lists(&dir);
    let min = lists.iter().find(|l| l.name == "Min").expect("Min");
    assert!(min.case_insensitive && min.apply_to_columns.is_empty());
    rename_list(&dir, "Min", "Minimal").expect("renamed");
    let (lists, _) = load_lists(&dir);
    assert!(lists.iter().any(|l| l.name == "Minimal") && !lists.iter().any(|l| l.name == "Min"));
    assert!(!dir.join("min.json").exists());
    assert!(rename_list(&dir, "Minimal", "term period ORDER").expect_err("taken").to_string().contains("already exists"));
    delete_list(&dir, "Minimal").expect("deleted");
    assert_eq!(load_lists(&dir).0.len(), 1);
    // a damaged file is listed, not fatal
    std::fs::write(dir.join("bad.json"), "{not json").expect("write");
    let (lists, problems) = load_lists(&dir);
    assert_eq!(lists.len(), 1);
    assert_eq!(problems.len(), 1);
    assert!(problems[0].0 == "bad.json", "{problems:?}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn u3_a_list_is_refused_when_it_cannot_order_anything() {
    let mk = |name: &str, values: &[&str], ci: bool| SortList { name: name.into(), values: values.iter().map(|s| s.to_string()).collect(), case_insensitive: ci, apply_to_columns: vec![] };
    let d = dir("u3_refuse");
    assert!(save_list(&d, &mk("", &["a"], true)).expect_err("no name").to_string().contains("name"));
    assert!(save_list(&d, &mk("Tenor", &["a"], true)).expect_err("built in").to_string().contains("built-in"));
    assert!(save_list(&d, &mk("X", &[], true)).expect_err("empty").to_string().contains("no values"));
    assert!(save_list(&d, &mk("X", &["a", "A"], true)).expect_err("twice").to_string().contains("twice"));
    assert!(save_list(&d, &mk("X", &["a", "A"], false)).is_ok());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn u3_the_python_export_orders_by_the_list_and_the_tenor() {
    let steps = vec![TransformStep::SortRows { levels: vec![sl("Risk", false, high_medium_low()), sl("Term", true, SortOrder::Tenor), sl("Name", false, SortOrder::Natural)] }];
    let py = micro_parquet::macros::generate_python_polars_code(std::path::Path::new("x.parquet"), true, &IngestionConfig::default(), &steps, None).expect("python");
    assert!(py.contains("def _mp_rank_keys"), "{py}");
    assert!(py.contains("def _mp_tenor_days"));
    assert!(py.contains("\"high\": 0.0") && py.contains("\"low\": 2.0"), "the list's ranks are in the script");
    assert!(py.contains("*_mp_rank_keys(pl.col(\"Risk\")") && py.contains("*_mp_rank_keys(pl.col(\"Term\"), _mp_tenor_days)"));
    assert!(py.contains("descending=[False, False, False, False, True, False, False]"), "{py}");
    // the Phase 3 sort script is unchanged
    let old = micro_parquet::macros::generate_python_polars_code(std::path::Path::new("x.parquet"), true, &IngestionConfig::default(), &[TransformStep::SortRows { levels: vec![sl("A", false, SortOrder::Natural)] }], None).expect("python");
    assert!(!old.contains("_mp_rank_keys"));
}

#[test]
fn u3_tenor_units_and_spellings() {
    for (t, d) in [("1M", 365.0 / 12.0), ("1 m", 365.0 / 12.0), ("12 Months", 365.0), ("12M", 365.0), ("3 mo", 91.25), ("6 mos", 182.5), ("1Y", 365.0), ("2 yrs", 730.0), ("5d", 5.0), ("2 wk", 14.0), ("ON", 1.0), ("o/n", 1.0), ("Overnight", 1.0)] {
        assert_eq!(parse_tenor_days(t), Some(d), "{t}");
    }
    for t in ["", "abc", "10", "M", "1x"] {
        assert_eq!(parse_tenor_days(t), None, "{t}");
    }
    // H1: 12M is exactly 1Y, and a tie falls back to the natural order of the text
    assert_eq!(parse_tenor_days("12M"), parse_tenor_days("1Y"));
    let mut v = vec!["1Y".to_string(), "12M".to_string(), "6M".to_string(), "13M".to_string()];
    v.sort_by(|a, b| SortOrder::Tenor.compare(&Some(a.clone()), &Some(b.clone())));
    assert_eq!(v, ["6M", "1Y", "12M", "13M"], "12M and 1Y tie, then the natural order: {v:?}");
}

