//! W10: filters on a chart. The window (the tick lists, All and None, the "Filters on this chart" section) is tested in
//! `src/ui/render_tests_p8.rs` (`w10_…`); here are the numbers: a chart with a filter adds up what the source filtered by hand
//! adds up to, the source is not touched, a kept item keeps its colour, a filter that Keep only this made is the same kind of
//! thing as one ticked in a list, and a saved chart carries it.

use crate::common::dir;
use crate::common8::date_tab;
use micro_parquet::charts::calc::compute;
use micro_parquet::charts::colours::Colour;
use micro_parquet::charts::data::ChartData;
use micro_parquet::charts::layout::{layout_chart, subtitle_text, ApproxMeasure, LayoutOpts, MarkKind, Prim};
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec};
use micro_parquet::dates::{CalendarSettings, DateFieldRef, DatePart};
use micro_parquet::engine::filter_by_conditions;
use micro_parquet::models::{CondOp, FilterCondition};
use polars::prelude::*;

/// Balance by Bucket, with a Bank of three kinds and a blank Bank on every 10th row.
fn book() -> DataFrame {
    let n = 60usize;
    let bank: Vec<Option<&str>> = (0..n).map(|i| if i % 10 == 9 { None } else { Some(["Conventional", "Islamic", "OBU"][i % 3]) }).collect();
    DataFrame::new(vec![
        Series::new("Bucket".into(), (0..n).map(|i| ["0-1M", "1-3M", "3-6M", "6-12M"][(i / 3) % 4]).collect::<Vec<_>>()).into(),
        Series::new("Bank".into(), bank).into(),
        Series::new("Balance".into(), (0..n).map(|i| 100.0 + i as f64 * 7.5).collect::<Vec<_>>()).into(),
    ])
    .expect("book")
}

fn spec() -> ChartSpec {
    ChartSpec { category: "Bucket".into(), series: Some("Bank".into()), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 500, ..Default::default() }
}

fn keep(column: &str, values: &[&str], blank: bool) -> FilterCondition {
    FilterCondition { column: column.into(), op: CondOp::InList, values: values.iter().map(|s| s.to_string()).collect(), or_blank: blank, ..Default::default() }
}

fn cells(d: &ChartData) -> Vec<(String, String, Option<f64>)> {
    let mut out = Vec::new();
    for (si, s) in d.series.iter().enumerate() {
        for (ci, c) in d.categories.iter().enumerate() {
            out.push((s.clone(), c.clone(), d.values.get(si).and_then(|v| v.get(ci)).copied().flatten()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    out
}

/// The fill of the first bar of the series `name`, as the app lays it out.
fn bar_fill(d: &ChartData, name: &str) -> u32 {
    let l = layout_chart(d, egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(1200.0, 700.0)), &LayoutOpts { dark: false, divisor: 1.0, decimals: 2, unit: "BDT".into(), bare: false, colours: None }, &ApproxMeasure);
    let si = d.series.iter().position(|s| s == name).unwrap_or_else(|| panic!("series {name}: {:?}", d.series));
    let m = l.marks.iter().find(|m| m.series == si && m.kind == MarkKind::Bar).expect("a bar");
    l.prims
        .iter()
        .find_map(|p| if let Prim::Rect { rect, fill, .. } = p { (*rect == m.rect).then_some(((fill.r() as u32) << 16) | ((fill.g() as u32) << 8) | fill.b() as u32) } else { None })
        .expect("the bar's fill")
}

#[test]
fn w10_balance_by_bucket_by_bank_keeping_conventional_equals_a_chart_of_the_source_filtered_by_hand_and_the_colour_stays() {
    let df = book();
    let rows_before = df.height();
    // the chart of all three banks; the names take their colours, as the studio stores them
    let mut all = spec();
    let everything = compute(df.clone().lazy(), &all).expect("chart");
    all.colour_map.assign_all(&everything.series);
    let slot = all.colour_map.get("Conventional");
    assert!(matches!(slot, Some(Colour::Slot(_))), "{slot:?}");
    let before = bar_fill(&compute(df.clone().lazy(), &all).expect("chart"), "Conventional");

    // keep Conventional: the chart's own filter
    let mut kept = all.clone();
    kept.filters = vec![keep("Bank", &["Conventional"], false)];
    let d = compute(df.clone().lazy(), &kept).expect("the filtered chart");
    // the same chart of the source filtered to Bank = Conventional first
    let by_hand = df.clone().lazy().filter(col("Bank").eq(lit("Conventional"))).collect().expect("filtered");
    let want = compute(by_hand.lazy(), &all).expect("the chart of the filtered source");
    assert_eq!(d.series, vec!["Conventional".to_string()]);
    assert_eq!((&d.categories, &d.values), (&want.categories, &want.values), "the totals equal a chart of the source filtered by hand");
    assert_eq!(cells(&d), cells(&want));
    assert!(d.values.iter().flatten().flatten().sum::<f64>() > 0.0);
    // the source is as it was (the chart reads a copy of its rows)
    assert_eq!(df.height(), rows_before);
    // Conventional keeps its colour: the names belong to their slots, and a filter never repaints
    assert_eq!(kept.colour_map.get("Conventional"), slot);
    assert_eq!(bar_fill(&d, "Conventional"), before, "Conventional is drawn in the colour it had");
    // and the other banks, when they come back, have theirs
    assert_eq!(kept.colour_map.get("Islamic"), all.colour_map.get("Islamic"));
    // the subtitle says so, in every renderer's words
    let o = LayoutOpts { dark: false, divisor: 1.0, decimals: 2, unit: "BDT".into(), bare: false, colours: None };
    assert!(subtitle_text(&d, &o).contains("Filtered: Bank = Conventional"), "{}", subtitle_text(&d, &o));
}

#[test]
fn w10_a_tick_list_of_two_values_and_the_blank_and_a_date_part_keep_exactly_those_rows() {
    let df = book();
    // Conventional or Islamic or blank: a list of values with the blank ticked
    let mut s = spec();
    s.filters = vec![keep("Bank", &["Conventional", "Islamic"], true)];
    let d = compute(df.clone().lazy(), &s).expect("chart");
    let want_rows = df.clone().lazy().filter(col("Bank").neq(lit("OBU")).or(col("Bank").is_null())).collect().expect("filtered");
    let total_want: f64 = want_rows.column("Balance").expect("Balance").f64().expect("f").into_iter().flatten().sum();
    let total: f64 = d.values.iter().flatten().flatten().sum();
    assert!((total - total_want).abs() < 1e-6, "{total} vs {total_want}");
    assert!(d.series.iter().any(|x| x == "(blank)") && !d.series.iter().any(|x| x == "OBU"), "{:?}", d.series);
    let o = LayoutOpts { dark: false, divisor: 1.0, decimals: 2, unit: "BDT".into(), bare: false, colours: None };
    assert!(subtitle_text(&d, &o).contains("Filtered: Bank = Conventional or Islamic or (blank)"), "{}", subtitle_text(&d, &o));

    // a date column "By Day name": Fri and Sat of the synthetic tab, as the tab's own filter keeps them
    let dates = date_tab();
    let cal = CalendarSettings::default();
    let part = DateFieldRef::new("Open Date", DatePart::DayName, cal);
    let mut s = ChartSpec { category: "Branch".into(), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 50, ..Default::default() };
    s.filters = vec![FilterCondition { column: "Open Date".into(), op: CondOp::InList, values: vec!["Fri".into(), "Sat".into()], part: Some(part.clone()), ..Default::default() }];
    let d = compute(dates.clone().lazy(), &s).expect("chart");
    // by hand: the weekday of each date (Saturday-start weeks do not change a day's name)
    let days = dates.column("Open Date").expect("Open Date").cast(&DataType::Int32).expect("days");
    let balance = dates.column("Balance").expect("Balance").f64().expect("f");
    let mut want = 0.0;
    for (i, day) in days.i32().expect("i32").into_iter().enumerate() {
        if let Some(date) = day.and_then(micro_parquet::dates::date_from_days) {
            if matches!(chrono::Datelike::weekday(&date), chrono::Weekday::Fri | chrono::Weekday::Sat) {
                want += balance.get(i).unwrap_or(0.0);
            }
        }
    }
    let total: f64 = d.values.iter().flatten().flatten().sum();
    assert!((total - want).abs() < 1e-6, "{total} vs {want}");
    assert!(subtitle_text(&d, &o).contains("Filtered: Open Date \u{b7} Day name = Fri or Sat"), "{}", subtitle_text(&d, &o));
}

#[test]
fn w10_a_filter_with_no_value_ticked_is_refused_in_words_and_the_filters_of_keep_only_are_the_same_kind_of_thing() {
    let df = book();
    let mut s = spec();
    s.filters = vec![keep("Bank", &[], false)];
    let e = compute(df.clone().lazy(), &s).err().expect("refused").to_string();
    assert!(e.contains("the filter on 'Bank' has no value ticked") && e.contains("press All"), "{e}");
    // "Keep only this" writes the same condition a tick list writes: an "in list" of the ticked value
    let mut s = spec();
    let mut by_click = ChartSpec { category: "Bank".into(), value: Some("Balance".into()), agg: ChartAgg::Sum, ..Default::default() };
    assert!(by_click.add_value_filter("Islamic", true).expect("added"));
    assert_eq!(by_click.filters, vec![FilterCondition { column: "Bank".into(), op: CondOp::InList, values: vec!["Islamic".into()], ..Default::default() }]);
    // so the list shows it, and removing it gives every bank back
    let d = compute(df.clone().lazy(), &by_click).expect("chart");
    assert_eq!(d.categories, vec!["Islamic".to_string()]);
    by_click.filters.clear();
    let d = compute(df.clone().lazy(), &by_click).expect("chart");
    assert_eq!(d.categories.len(), 4, "Conventional, Islamic, OBU and the blank: {:?}", d.categories);
    // Exclude this is the opposite kind, and an old chart (no filters at all) loads and is calculated as it was
    s.filters.clear();
    let old: ChartSpec = serde_json::from_str(r#"{"category":"Bucket","series":"Bank","value":"Balance","agg":"Sum","kind":"Bar"}"#).expect("loads");
    assert!(old.filters.is_empty());
    assert_eq!(cells(&compute(df.clone().lazy(), &old).expect("old")), cells(&compute(df.clone().lazy(), &ChartSpec { top_n: 500, ..old.clone() }).expect("old")));
}

#[test]
fn w10_a_saved_chart_carries_its_filters_and_show_rows_of_a_mark_includes_them() {
    let d = dir("w10_saved");
    let mut s = spec();
    s.filters = vec![keep("Bank", &["Conventional", "OBU"], false)];
    micro_parquet::charts::library::save_chart(&d, "filtered", &s).expect("saved");
    let (_, back) = micro_parquet::charts::library::load_chart(&micro_parquet::charts::library::chart_path(&d, "filtered")).expect("loaded");
    assert_eq!(back.filters, s.filters, "a saved chart keeps its filters");
    assert_eq!(back, s);
    // a chart with none is written without the field, as before... and one that has them says so in the file
    let json = serde_json::to_string(&s).expect("json");
    assert!(json.contains("\"filters\""));
    // the rows behind a mark of the filtered chart are the rows of the mark among the rows the filters keep
    let df = book();
    let data = compute(df.clone().lazy(), &s).expect("chart");
    let ci = 0;
    let (si, series) = (0usize, data.series[0].clone());
    let category = data.categories[ci].clone();
    let value = data.values[si][ci].expect("a value");
    let conds = s.at_level().mark_conditions(&category, Some(series.as_str()), None).expect("conditions");
    assert!(conds.iter().any(|c| c.column == "Bank" && c.op == CondOp::InList && c.values == vec!["Conventional".to_string(), "OBU".to_string()]), "the chart's filter is in the rows' conditions: {conds:?}");
    let rows = filter_by_conditions(df.lazy(), &conds).and_then(|lf| lf.collect().map_err(Into::into)).expect("rows");
    let sum: f64 = rows.column("Balance").expect("Balance").f64().expect("f").into_iter().flatten().sum();
    assert!((sum - value).abs() < 1e-6, "the rows of the mark add up to the mark: {sum} vs {value}");
    assert!(rows.height() > 0);
    assert!(s.at_level().mark_title(&category, Some(series.as_str()), None).starts_with("Rows: "));
}
