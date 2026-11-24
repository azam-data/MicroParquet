//! W3: rate bands without serial numbers. The owner's example bands (0, 2–3 … 10–11, No rate) unnumbered come
//! out in band order in the step result, a pivot, the chart, the Excel export and the Python export; an old
//! macro with the step gives the numbered labels byte for byte; the template's title reads "… sit at 8%–9%"
//! in both forms.

use crate::common::{dir, run_steps, strings, unzip_entry};
use micro_parquet::charts::templates::{prepare, Ctx, Template};
use micro_parquet::models::*;
use micro_parquet::sort_lists::SortOrder;
use polars::prelude::*;

/// The owner's book: rates of 0, 2.5 … 10.5 and blanks.
fn book() -> DataFrame {
    let rates: Vec<Option<f64>> = vec![Some(10.5), None, Some(0.0), Some(2.5), Some(8.5), Some(3.5), Some(9.5), Some(4.5), Some(7.5), Some(5.5), Some(6.5), Some(8.25), Some(0.0), None];
    let n = rates.len();
    DataFrame::new(vec![
        Series::new("Rate".into(), rates).into(),
        Series::new("Balance".into(), (0..n).map(|i| 100.0 * (i as f64 + 1.0)).collect::<Vec<_>>()).into(),
    ])
    .expect("book")
}

fn owner_bands(numbered: bool) -> RateBandDef {
    RateBandDef { numbered, ..RateBandDef::custom("Owner", vec![0.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0]) }
}

const IN_ORDER: [&str; 10] = ["0%", "2%\u{2013}3%", "3%\u{2013}4%", "4%\u{2013}5%", "5%\u{2013}6%", "6%\u{2013}7%", "7%\u{2013}8%", "8%\u{2013}9%", "9%\u{2013}10%", "10%\u{2013}11%"];

fn band_step(def: RateBandDef) -> TransformStep {
    TransformStep::RateBand { rate_col: "Rate".into(), band_def: def, new_col: "Rate Band".into() }
}

fn in_band_order(labels: &[String]) -> Vec<String> {
    let want: Vec<String> = IN_ORDER.iter().map(|s| s.to_string()).chain(["No rate".to_string()]).collect();
    labels.iter().filter(|l| want.contains(l)).cloned().collect()
}

fn expected() -> Vec<String> {
    IN_ORDER.iter().map(|s| s.to_string()).chain(["No rate".to_string()]).collect()
}

#[test]
fn w3_numbered_defaults_to_true_and_the_dialog_makes_plain_labels() {
    // every saved step, preset and macro (no field) keeps "09. 8%–9%"
    let old: RateBandDef = serde_json::from_str(r#"{"name": "p", "mode": {"Fixed": {"start_pct": 0.0, "end_pct": 15.0, "step_pct": 1.0}}}"#).expect("an old preset");
    assert!(old.numbered);
    assert_eq!(old.label_for(Some(9.0)).expect("label"), "09. 8%\u{2013}9%");
    assert_eq!(old.label_for(Some(0.0)).expect("label"), "00. 0%");
    assert!(RateBandDef::default_profile().numbered, "the built-in presets as before");
    let plain = RateBandDef { numbered: false, ..RateBandDef::default_profile() };
    let l = plain.labels().expect("labels");
    assert_eq!((l.zero.as_str(), l.bands[8].as_str(), l.above.as_str(), l.below.as_str(), l.blank.as_str()), ("0%", "8%\u{2013}9%", "> 15%", "Below 0%", "No rate"));
    let order = plain.band_order().expect("order");
    assert_eq!(&order[..3], ["Below 0%", "0%", "0%\u{2013}1%"]);
    assert_eq!(&order[order.len() - 2..], ["> 15%", "No rate"]);
    // the declared order: a list in band order for an unnumbered column, nothing for a numbered one
    let o = micro_parquet::engine::column_orders(&[band_step(plain.clone())]);
    assert_eq!(o.get("Rate Band"), Some(&SortOrder::List { name: "Rate bands".into(), values: order, case_insensitive: false }));
    assert!(micro_parquet::engine::column_orders(&[band_step(old)]).is_empty());
    // the plain labels are also a Ranges column, so a tab that lost its steps still orders them
    assert_eq!(micro_parquet::sort_lists::detect_order(plain.band_order().expect("o").iter().map(|s| s.as_str()), &Default::default()), Some(SortOrder::Ranges));
}

#[test]
fn w3_the_owners_bands_unnumbered_come_out_in_band_order_everywhere() {
    let def = owner_bands(false);
    let step = band_step(def.clone());
    let order = micro_parquet::engine::column_orders(&[step.clone()]).remove("Rate Band").expect("declared");
    // the step result, sorted by its band column in the declared order (what a sort level added gets)
    let sorted = run_steps("w3_sort", &book(), &[step.clone(), TransformStep::SortRows { levels: vec![SortLevel { column: "Rate Band".into(), descending: false, order: order.clone() }] }]).expect("sorts");
    let mut distinct: Vec<String> = Vec::new();
    for l in strings(&sorted, "Rate Band").into_iter().flatten() {
        if !distinct.contains(&l) {
            distinct.push(l);
        }
    }
    assert_eq!(distinct, expected(), "the step result in band order");
    // a pivot (the builder gives the field the declared order)
    let pivot = TransformStep::from_pivot_spec(PivotSpec {
        rows: vec!["Rate Band".into()],
        values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None }],
        row_totals: true,
        column_totals: true,
        field_orders: vec![PivotFieldOrder { field: "Rate Band".into(), order: order.clone() }],
        ..Default::default()
    });
    let p = run_steps("w3_pivot", &book(), &[step.clone(), pivot.clone()]).expect("pivots");
    let labels: Vec<String> = strings(&p, "Rate Band").into_iter().flatten().collect();
    assert_eq!(in_band_order(&labels), expected(), "the pivot: {labels:?}");
    // the chart: the template orders the plain labels by Ranges
    let tab = run_steps("w3_tab", &book(), &[step.clone()]).expect("bands");
    let prepared = prepare(Template::RateProfile, &tab, &Ctx::default()).expect("the template");
    assert_eq!(prepared.spec.order, SortOrder::Ranges);
    let d = micro_parquet::charts::calc::compute(tab.clone().lazy(), &prepared.spec).expect("the chart");
    assert_eq!(in_band_order(&d.categories), expected(), "the chart: {:?}", d.categories);
    // the Excel export of the pivot tab
    let dd = dir("w3_xlsx");
    let xlsx = dd.join("bands.xlsx");
    micro_parquet::exporters::write_xlsx_workbook(vec![("Pivot".to_string(), p.clone())], vec![], &xlsx, micro_parquet::exporters::EXCEL_MAX_DATA_ROWS).expect("writes");
    let shared = unzip_entry(&xlsx, "xl/sharedStrings.xml");
    let texts: Vec<String> = shared.split("<t>").skip(1).chain(shared.split("<t xml:space=\"preserve\">").skip(1)).filter_map(|s| s.split("</t>").next()).map(|s| s.replace("&gt;", ">")).collect();
    let in_file: Vec<String> = shared.match_indices("<t").filter_map(|(i, _)| shared[i..].split_once('>').and_then(|(_, r)| r.split("</t>").next())).map(|s| s.replace("&gt;", ">")).collect();
    assert!(!texts.is_empty());
    assert_eq!(in_band_order(&in_file), expected(), "the workbook: {in_file:?}");
    assert!(!in_file.iter().any(|t| t.starts_with("02. ")), "no serial numbers in the workbook");
    // the Python export writes the same labels, and a sort in the declared order lists them in band order
    let py = micro_parquet::macros::generate_python_polars_code(std::path::Path::new("in.parquet"), true, &IngestionConfig::default(), &[step.clone(), TransformStep::SortRows { levels: vec![SortLevel { column: "Rate Band".into(), descending: false, order }] }], None).expect("python");
    assert!(py.contains("pl.lit(\"8%\\u20139%\")") && py.contains("pl.lit(\"0%\")") && !py.contains("\"08. "), "{py}");
    let dict = py.split("_r={").nth(1).and_then(|s| s.split('}').next()).expect("the order of the sort");
    let listed: Vec<String> = dict.split(", ").filter_map(|kv| kv.split(": ").next()).map(|k| serde_json::from_str::<String>(k).expect("a label")).collect();
    assert_eq!(in_band_order(&listed), expected(), "the Python sort: {listed:?}");
}

#[test]
fn w3_an_old_macro_with_the_step_gives_the_numbered_labels_byte_for_byte() {
    // saved by the Phase 7 code (tests/fixtures/phase7_replay): its RateBand step has no `numbered`
    let fx = crate::common::fixtures_dir().join("phase7_replay").join("phase7_rows.mpq.json");
    let text = std::fs::read_to_string(&fx).expect("the fixture");
    assert!(text.contains("\"RateBand\"") && !text.contains("numbered"), "a Phase 7 step");
    let r = micro_parquet::macros::load_macro_from_file(&fx).expect("loads");
    let band = r.steps.iter().find_map(|s| match s {
        TransformStep::RateBand { band_def, .. } => Some(band_def.clone()),
        _ => None,
    });
    assert!(band.expect("a rate band step").numbered);
    let out = crate::common::replay_to_csv(&r.steps, &dir("w3_old"));
    assert!(out.contains("10. 9%\u{2013}10%") || out.contains("09. 8%\u{2013}9%"), "numbered labels: {out}");
    // the same bytes as the Phase 7 code gave (the hash w0 pins too)
    let want: std::collections::BTreeMap<String, String> = serde_json::from_str(&std::fs::read_to_string(crate::common::fixtures_dir().join("phase7_replay").join("phase7_replay.hashes.json")).expect("hashes")).expect("json");
    assert_eq!(crate::p7_baseline::hash(&out), want["macro:phase7_rows"]);
}

#[test]
fn w3_the_template_title_reads_sit_at_8_to_9_in_both_forms() {
    let rates: Vec<f64> = vec![8.5, 8.25, 8.75, 5.0, 12.0];
    let bal: Vec<f64> = vec![500.0, 300.0, 200.0, 100.0, 50.0];
    for numbered in [true, false] {
        let def = RateBandDef { numbered, ..RateBandDef::default_profile() };
        let df = DataFrame::new(vec![Series::new("Rate".into(), rates.clone()).into(), Series::new("Balance".into(), bal.clone()).into()]).expect("frame");
        let tab = run_steps("w3_title", &df, &[TransformStep::RateBand { rate_col: "Rate".into(), band_def: def, new_col: "Rate_Band".into() }]).expect("bands");
        let p = prepare(Template::RateProfile, &tab, &Ctx::default()).expect("the template");
        assert_eq!(p.finding, "87.0% of balances sit at 8%\u{2013}9%", "numbered {numbered}");
        assert_eq!(p.spec.order, if numbered { SortOrder::Natural } else { SortOrder::Ranges }, "numbered labels sort as text, as in Phase 7");
    }
}
