//! M2: the shared calendar and the as-of date. The model calendar is one `CalendarSettings`, a generated Calendar table with one row
//! per day, and a date link for each data tab (its date column joined to Calendar · Date). Calendar fields are lookup fields through
//! that link. Snapshot dates, their frequency and the as-of date are worked out from the tabs' own dates.

use crate::common::{dir, floats, parquet, strings};
use chrono::{Datelike, NaiveDate};
use micro_parquet::dates::{CalendarSettings, DateFieldRef, DatePart, PartValue, Weekday};
use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::model::*;
use micro_parquet::model_calendar::*;
use micro_parquet::models::*;
use micro_parquet::pivot_table::build_pivot;
use polars::prelude::*;
use std::collections::BTreeMap;
use std::sync::Arc;

fn d(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("a date")
}

fn date_series(name: &str, v: &[Option<&str>]) -> Series {
    let days: Vec<Option<i32>> = v.iter().map(|t| t.map(|t| micro_parquet::dates::days_from_date(d(t)))).collect();
    Series::new(name.into(), days).cast(&DataType::Date).expect("dates")
}

fn july_sunday() -> CalendarSettings {
    CalendarSettings { fy_start_month: 7, week_start: Weekday::Sun }
}

fn calendar_tab(settings: CalendarSettings, first: i32, last: i32) -> ModelTab {
    ModelTab { key: "t3".into(), title: "Calendar".into(), source: TabSource::Calendar { settings, first_year: first, last_year: last }, steps: vec![] }
}

fn text_at(df: &DataFrame, name: &str, i: usize) -> String {
    strings(df, name)[i].clone().unwrap_or_default()
}

#[test]
fn m2_calendar_rows_run_from_1_jan_of_the_earliest_year_to_31_dec_of_the_latest_and_every_part_equals_dates_rs() {
    let cal = july_sunday();
    let df = calendar_frame(&cal, 2025, 2028).expect("calendar");
    assert_eq!(df.height(), 365 * 3 + 366, "2025 to 2028, whole years");
    let names: Vec<String> = df.get_column_names().iter().map(|n| n.to_string()).collect();
    assert_eq!(
        names,
        ["Date", "Year", "Quarter", "Year-Quarter", "Month", "Month number", "Month-Year", "Week of year", "Week starting", "Day name", "Day of month", "Fiscal year", "Fiscal quarter", "Fiscal month", "Month end", "Days in month"]
    );
    let days = df.column("Date").expect("Date").cast(&DataType::Int32).expect("days");
    let days = days.i32().expect("i32");
    assert_eq!(micro_parquet::dates::date_from_days(days.get(0).expect("first")), Some(d("2025-01-01")));
    assert_eq!(micro_parquet::dates::date_from_days(days.get(df.height() - 1).expect("last")), Some(d("2028-12-31")));
    // every part of every row equals the one dates.rs gives for that date with the model's settings
    for i in 0..df.height() {
        let date = micro_parquet::dates::date_from_days(days.get(i).expect("day")).expect("date");
        for part in DatePart::ALL {
            if part == DatePart::Date {
                continue;
            }
            let cell = df.column(part.label()).expect("column").get(i).expect("cell");
            match part.value(date, &cal) {
                PartValue::Int(v) => assert_eq!(cell.extract::<i32>(), Some(v), "{} of {date}", part.label()),
                PartValue::Text(t) => assert_eq!(cell.get_str().map(str::to_string), Some(t), "{} of {date}", part.label()),
                PartValue::Date(x) => assert_eq!(cell.extract::<i32>(), Some(micro_parquet::dates::days_from_date(x)), "{} of {date}", part.label()),
            }
        }
    }
    // Month end and Days in month: Feb 2028 has 29 days (a leap year) and Feb 2026 has 28
    let by_date = |text: &str| (0..df.height()).find(|i| micro_parquet::dates::date_from_days(days.get(*i).expect("day")) == Some(d(text))).unwrap_or_else(|| panic!("{text} is in the calendar"));
    let dim = |text: &str| df.column("Days in month").expect("dim").get(by_date(text)).expect("cell").extract::<i32>().expect("number");
    assert_eq!((dim("2028-02-10"), dim("2026-02-10"), dim("2026-04-01"), dim("2026-12-31")), (29, 28, 30, 31));
    let end = |text: &str| text_at(&df, "Month end", by_date(text));
    assert_eq!((end("2028-02-28").as_str(), end("2028-02-29").as_str(), end("2026-02-28").as_str(), end("2026-03-01").as_str()), ("No", "Yes", "Yes", "No"));
    let february_2028 = (0..df.height()).filter(|i| micro_parquet::dates::date_from_days(days.get(*i).expect("day")).is_some_and(|x| x.year() == 2028 && x.month() == 2)).count();
    assert_eq!(february_2028, 29);
    // the number of Yes rows is one a month
    let yes = strings(&df, "Month end").into_iter().flatten().filter(|v| v == "Yes").count();
    assert_eq!(yes, 48);
}

#[test]
fn m2_the_calendars_years_come_from_the_dates_found_and_a_range_that_is_not_a_calendar_is_refused() {
    assert_eq!(year_range(&[d("2025-03-01"), d("2026-11-24"), d("2025-12-31")]), Some((2025, 2026)));
    assert_eq!(year_range(&[]), None);
    let e = calendar_frame(&CalendarSettings::default(), 2030, 2020).err().expect("refused");
    assert!(format!("{e:#}").contains("first year (2030) is after its last year (2020)"), "{e:#}");
    let e = calendar_frame(&CalendarSettings::default(), 1066, 2020).err().expect("refused");
    assert!(format!("{e:#}").contains("check the date column"), "{e:#}");
    // the tab's source compiles to the same table
    let lf = compile_tab_lazy(&TabSource::Calendar { settings: CalendarSettings::default(), first_year: 2026, last_year: 2026 }, &[], &std::env::temp_dir()).expect("compile");
    assert_eq!(lf.collect().expect("rows").height(), 365);
}

/// Accounts with an Open Date (a Date), a Balance and an AC Number.
fn accounts_with_dates() -> DataFrame {
    DataFrame::new(vec![
        date_series("Open Date", &[Some("2025-11-30"), Some("2025-12-15"), Some("2026-01-31"), Some("2026-02-10"), Some("2026-02-28"), Some("2026-07-01"), Some("2026-06-30"), Some("2027-01-05")]).into(),
        Series::new("Balance".into(), [100.0, 200.0, 300.0, 400.0, 500.0, 600.0, 700.0, 800.0]).into(),
        Series::new("AC Number".into(), ["A1", "A2", "A3", "A4", "A5", "A6", "A7", "A8"]).into(),
    ])
    .expect("accounts")
}

fn scope_with(links: Vec<(Link, ModelTab)>, dir: &std::path::Path) -> Option<Arc<ModelContext>> {
    Some(Arc::new(ModelContext { links: links.into_iter().map(|(link, lookup)| ScopeLink { link, lookup: Some(lookup), refusal: None }).collect(), temp_dir: dir.to_path_buf() }))
}

fn pairs(df: &DataFrame) -> Vec<(String, f64)> {
    let names: Vec<String> = df.get_column_names().iter().map(|n| n.to_string()).collect();
    strings(df, &names[0]).into_iter().zip(floats(df, &names[1])).map(|(l, v)| (l.unwrap_or_else(|| "(blank)".into()), v.unwrap_or(f64::NAN))).collect()
}

fn calendar_field(settings: CalendarSettings, column: &str) -> ModelFieldRef {
    let _ = settings;
    ModelFieldRef::new(&date_link("t1", "Open Date", "t3"), "Accounts \u{2192} Calendar", CALENDAR_TITLE, column)
}

fn pivot_by(field: &ModelFieldRef, order: Option<micro_parquet::sort_lists::SortOrder>) -> PivotSpec {
    PivotSpec {
        rows: vec![field.name.clone()],
        values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None }],
        row_totals: true,
        column_totals: true,
        model_fields: vec![field.clone()],
        field_orders: order.map(|o| vec![PivotFieldOrder { field: field.name.clone(), order: o }]).unwrap_or_default(),
        ..Default::default()
    }
}

#[test]
fn m2_a_pivot_by_calendar_month_year_equals_a_pivot_by_the_w2_date_field_made_with_the_same_settings_in_the_same_order() {
    let dd = dir("p9m2_pivot");
    let settings = july_sunday();
    let src = parquet(&dd, "accounts", &accounts_with_dates());
    let lf = || compile_tab_lazy(&src, &[], &dd).expect("accounts");
    let order = DatePart::MonthYear.sort_order(&settings);
    let ours_spec = pivot_by(&calendar_field(settings, "Month-Year"), order.clone());
    let scope = scope_with(vec![(date_link("t1", "Open Date", "t3"), calendar_tab(settings, 2025, 2027))], &dd);
    let ours = with_scope(scope, || build_pivot(lf(), &ours_spec)).expect("pivot by Calendar").df;
    // the W2 date field of the same column, with the same settings
    let field = DateFieldRef::new("Open Date", DatePart::MonthYear, settings);
    let w2_spec = PivotSpec {
        rows: vec![field.name.clone()],
        values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None }],
        row_totals: true,
        column_totals: true,
        date_fields: vec![field.clone()],
        field_orders: vec![PivotFieldOrder { field: field.name.clone(), order: order.expect("an order") }],
        ..Default::default()
    };
    let theirs = build_pivot(lf(), &w2_spec).expect("pivot by the W2 field").df;
    assert_eq!(pairs(&ours), pairs(&theirs), "row for row, in the same order");
    let labels: Vec<String> = pairs(&ours).into_iter().map(|p| p.0).collect();
    assert_eq!(labels, ["Nov 2025", "Dec 2025", "Jan 2026", "Feb 2026", "Jun 2026", "Jul 2026", "Jan 2027", "Grand total"], "chronological, not alphabetical");
    assert_eq!(pairs(&ours).last().map(|p| p.1), Some(3600.0), "the total is the ungrouped total");
}

#[test]
fn m2_changing_the_fiscal_start_to_july_changes_calendar_fiscal_year_on_two_data_tabs_at_once() {
    let dd = dir("p9m2_fiscal");
    // two data tabs, one date column each (the second is a date and time), the same Calendar
    let a = parquet(&dd, "accounts", &accounts_with_dates());
    let tb = DataFrame::new(vec![
        Series::new(
            "Value Date".into(),
            [("2026-06-30", 36_900_000_000i64), ("2026-07-01", 86_340_000_000), ("2027-01-31", 0)].iter().map(|(t, us)| micro_parquet::dates::days_from_date(d(t)) as i64 * 86_400_000_000 + us).collect::<Vec<i64>>(),
        )
        .cast(&DataType::Datetime(TimeUnit::Microseconds, None))
        .expect("datetimes")
        .into(),
        Series::new("Balance".into(), [10.0, 20.0, 30.0]).into(),
        Series::new("AC Number".into(), ["G1", "G2", "G3"]).into(),
    ])
    .expect("tb");
    let b = parquet(&dd, "tb", &tb);
    let link_a = date_link("t1", "Open Date", "t3");
    let link_b = date_link("t2", "Value Date", "t3");
    let run = |settings: CalendarSettings| -> (Vec<(String, f64)>, Vec<(String, f64)>) {
        let scope = scope_with(vec![(link_a.clone(), calendar_tab(settings, 2025, 2027)), (link_b.clone(), calendar_tab(settings, 2025, 2027))], &dd);
        let fa = ModelFieldRef::new(&link_a, "Accounts \u{2192} Calendar", CALENDAR_TITLE, "Fiscal year");
        let fb = ModelFieldRef::new(&link_b, "GL TB \u{2192} Calendar", CALENDAR_TITLE, "Fiscal year");
        let ra = with_scope(scope.clone(), || build_pivot(compile_tab_lazy(&a, &[], &dd).expect("a"), &pivot_by(&fa, None))).expect("pivot a");
        let rb = with_scope(scope, || build_pivot(compile_tab_lazy(&b, &[], &dd).expect("b"), &pivot_by(&fb, None))).expect("pivot b");
        (pairs(&ra.df), pairs(&rb.df))
    };
    let (a_jan, b_jan) = run(CalendarSettings { fy_start_month: 1, week_start: Weekday::Sat });
    let (a_jul, b_jul) = run(CalendarSettings { fy_start_month: 7, week_start: Weekday::Sat });
    let got = |v: &[(String, f64)], l: &str| v.iter().find(|p| p.0 == l).map(|p| p.1);
    assert_eq!((got(&a_jan, "FY2025"), got(&a_jan, "FY2026"), got(&a_jan, "FY2027")), (Some(300.0), Some(2500.0), Some(800.0)), "January start: calendar years");
    assert_eq!((got(&b_jan, "FY2026"), got(&b_jan, "FY2027")), (Some(30.0), Some(30.0)));
    // July: 1 Jul 2026 starts FY2026-27; 30 Jun 2026 is still FY2025-26
    assert_eq!((got(&a_jul, "FY2025-26"), got(&a_jul, "FY2026-27")), (Some(2200.0), Some(1400.0)), "the first tab moved");
    assert_eq!((got(&b_jul, "FY2025-26"), got(&b_jul, "FY2026-27")), (Some(10.0), Some(50.0)), "the second tab moved with the same setting");
}

#[test]
fn m2_a_date_link_on_a_text_column_is_refused_with_the_phase_8_message() {
    let dd = dir("p9m2_text");
    let df = DataFrame::new(vec![Series::new("Open Date".into(), ["31/01/2026", "28/02/2026"]).into(), Series::new("Balance".into(), [1.0, 2.0]).into()]).expect("text dates");
    let src = parquet(&dd, "t", &df);
    let e = snapshot_counts(compile_tab_lazy(&src, &[], &dd).expect("tab"), "Open Date").err().expect("refused");
    assert_eq!(format!("{e:#}"), "'Open Date' is text. Set it to Date in Schema & Ingestion (D/M/Y) first.");
    let e = snapshot_counts(compile_tab_lazy(&src, &[], &dd).expect("tab"), "Balance").err().expect("refused");
    assert_eq!(format!("{e:#}"), "'Balance' is a number, not a date. Set it to Date in Schema & Ingestion (D/M/Y) first.");
}

fn counts(v: &[(&str, usize)]) -> Vec<(NaiveDate, usize)> {
    v.iter().map(|(t, n)| (d(t), *n)).collect()
}

#[test]
fn m2_snapshot_frequency_is_month_end_for_month_end_dates_and_daily_for_daily_ones_with_the_single_date_months_and_the_incomplete_dates_listed() {
    // month-end: one date a month
    let me = SnapshotInfo::from_counts(&counts(&[("2025-01-31", 100), ("2025-02-28", 100), ("2025-03-31", 100), ("2025-12-31", 100)]));
    assert_eq!((me.frequency, me.dates), (Frequency::MonthEnd, 4));
    assert_eq!((me.first, me.last), (Some(d("2025-01-31")), Some(d("2025-12-31"))));
    assert_eq!(me.summary(), "31 Jan 2025 to 31 Dec 2025 \u{b7} 4 snapshot dates \u{b7} Month-end");
    assert!(me.single_date_months.is_empty() && me.single_months_text().is_none(), "a Month-end tab lists no single-date months");

    // daily: Jan to Oct 2025 one date each (a month-end), then Nov 2025 daily
    let mut v: Vec<(NaiveDate, usize)> = (1..=10).map(|m| (NaiveDate::from_ymd_opt(2025, m, days_in_month(2025, m)).expect("date"), 50)).collect();
    for day in 1..=6 {
        v.push((NaiveDate::from_ymd_opt(2025, 11, day).expect("date"), 50));
    }
    let daily = SnapshotInfo::from_counts(&v);
    assert_eq!(daily.frequency, Frequency::Daily);
    assert_eq!(daily.dates, 16);
    assert_eq!(daily.single_date_months, (1..=10).map(|m| (2025, m)).collect::<Vec<_>>());
    assert_eq!(daily.single_months_text().as_deref(), Some("Jan\u{2013}Oct 2025 have one date each; their average balances carry that date forward."));
    assert!(daily.incomplete.is_empty() && daily.incomplete_text().is_none());
    // one single-date month, and runs apart
    let mut v2: Vec<(NaiveDate, usize)> = vec![(d("2025-01-31"), 10), (d("2025-03-31"), 10), (d("2025-04-30"), 10)];
    v2.extend((1..=3).map(|day| (NaiveDate::from_ymd_opt(2025, 2, day).expect("date"), 10)));
    let one = SnapshotInfo::from_counts(&v2);
    assert_eq!(one.single_months_text().as_deref(), Some("Jan 2025, Mar\u{2013}Apr 2025 have one date each; their average balances carry that date forward."));
    let just_one = SnapshotInfo::from_counts(&counts(&[("2025-01-31", 10), ("2025-02-01", 10), ("2025-02-02", 10), ("2025-03-31", 10)]));
    assert_eq!(just_one.single_months_text().as_deref(), Some("Jan 2025, Mar 2025 have one date each; their average balances carry that date forward."));
    let single = SnapshotInfo::from_counts(&counts(&[("2025-01-31", 10), ("2025-02-01", 10), ("2025-02-02", 10)]));
    assert_eq!(single.single_months_text().as_deref(), Some("Jan 2025 has one date; its average balance carries that date forward."));

    // a date with a quarter of the median rows looks incomplete; one with exactly half does not
    let inc = SnapshotInfo::from_counts(&counts(&[("2026-11-20", 100), ("2026-11-21", 100), ("2026-11-22", 100), ("2026-11-23", 25), ("2026-11-24", 50)]));
    assert_eq!(inc.median_rows, 100.0);
    assert_eq!(inc.incomplete, vec![Incomplete { date: d("2026-11-23"), rows: 25, median: 100.0 }], "25 is under half of 100; 50 is exactly half");
    assert_eq!(inc.incomplete_text().as_deref(), Some("Looks incomplete: 23 Nov 2026 has 25 rows (the median is 100)."));
    // no rows at all
    let none = SnapshotInfo::from_counts(&[]);
    assert_eq!((none.dates, none.first, none.summary().as_str()), (0, None, "no dates"));
}

#[test]
fn m2_snapshot_dates_are_the_distinct_dates_with_rows_after_the_tabs_steps_and_blank_dates_are_ignored() {
    let dd = dir("p9m2_counts");
    let df = DataFrame::new(vec![
        date_series("Date", &[Some("2026-11-02"), Some("2026-11-02"), Some("2026-11-03"), None, Some("2026-11-03"), Some("2026-11-03")]).into(),
        Series::new("Bank".into(), ["C", "I", "C", "C", "C", "C"]).into(),
    ])
    .expect("tab");
    let src = parquet(&dd, "tb", &df);
    let all = snapshot_counts(compile_tab_lazy(&src, &[], &dd).expect("tab"), "Date").expect("counts");
    assert_eq!(all, counts(&[("2026-11-02", 2), ("2026-11-03", 3)]), "the blank date is ignored");
    let step = TransformStep::FilterGroup { conditions: vec![FilterCondition { column: "Bank".into(), op: CondOp::Equals, value: "I".into(), values: vec!["I".into()], ..Default::default() }], join: GroupJoin::And, keep_matching: true };
    let after = snapshot_counts(compile_tab_lazy(&src, &[step], &dd).expect("tab"), "Date").expect("counts");
    assert_eq!(after, counts(&[("2026-11-02", 1)]), "after the tab's steps");
}

#[test]
fn m2_the_as_of_date_is_the_latest_date_of_all_date_linked_tabs_a_picked_one_is_kept_and_differing_ends_are_said() {
    let ends = vec![("Accounts".to_string(), d("2026-10-31")), ("GL TB".to_string(), d("2026-11-24"))];
    assert_eq!(latest_of(&ends), Some(d("2026-11-24")));
    assert_eq!(resolve(&AsOf::Latest, &ends), Some((d("2026-11-24"), true)));
    assert_eq!(status_text(d("2026-11-24"), true), "As of 24 Nov 2026 (latest data)");
    assert_eq!(resolve(&AsOf::Date("2026-10-31".into()), &ends), Some((d("2026-10-31"), false)));
    assert_eq!(status_text(d("2026-10-31"), false), "As of 31 Oct 2026");
    assert_eq!(ends_note(&ends).as_deref(), Some("Accounts ends 31 Oct 2026; GL TB ends 24 Nov 2026."));
    assert_eq!(ends_note(&[("Accounts".to_string(), d("2026-10-31")), ("GL TB".to_string(), d("2026-10-31"))]), None, "the same end: no note");
    assert_eq!(ends_note(&ends[..1]), None);
    assert_eq!(resolve(&AsOf::Latest, &[]), None, "no date-linked tab: no as-of date");
    // dates as the CBS writes them
    assert_eq!(parse_date("24/11/2026").ok(), Some(d("2026-11-24")));
    assert_eq!(parse_date(" 2026-11-24 ").ok(), Some(d("2026-11-24")));
    assert_eq!(parse_date("24 Nov 2026").ok(), Some(d("2026-11-24")));
    assert!(format!("{:#}", parse_date("31/02/2026").err().expect("refused")).contains("is not a date"));
    assert!(picked_note(d("2026-12-31"), &ends).is_some_and(|n| n.contains("no data after 24 Nov 2026")));
    assert_eq!(picked_note(d("2026-10-31"), &ends), None);
}

#[test]
fn m2_the_model_calendar_and_the_as_of_date_round_trip_and_an_old_model_has_neither() {
    let mut m = Model::default();
    assert!(m.is_empty());
    m.calendar = Some(ModelCalendar { settings: july_sunday(), date_links: vec![DateLink { data_tab: "t1".into(), column: "Open Date".into() }], tab: "t3".into() });
    m.as_of = AsOf::Date("2026-10-31".into());
    let text = serde_json::to_string(&m).expect("json");
    let back: Model = serde_json::from_str(&text).expect("back");
    assert_eq!(back, m);
    assert!(!m.is_empty());
    // the date link is a link to the Calendar tab on its Date column
    let links = m.all_links();
    assert_eq!(links.len(), 1);
    assert_eq!((links[0].data_tab.as_str(), links[0].lookup_tab.as_str()), ("t1", "t3"));
    assert_eq!((links[0].keys[0].data.as_str(), links[0].keys[0].lookup.as_str()), ("Open Date", "Date"));
    // a model written before M2, with links only, loads with no calendar and the latest data
    let old: Model = serde_json::from_str(r#"{"links": [{"data_tab": "t1", "lookup_tab": "t2", "keys": [{"data": "A", "lookup": "B"}]}]}"#).expect("old");
    assert!(old.calendar.is_none() && old.as_of == AsOf::Latest && old.all_links().len() == 1);
    // nothing of M2 is written when it is off
    let off = serde_json::to_string(&old).expect("json");
    assert!(!off.contains("calendar") && !off.contains("as_of"), "{off}");
    // the settings only: the other fields have their defaults
    let bare: Model = serde_json::from_str(r#"{"calendar": {"settings": {"fy_start_month": 4}}}"#).expect("bare");
    let c = bare.calendar.expect("calendar");
    assert_eq!((c.settings.fy_start_month, c.settings.week_start, c.date_links.len(), c.tab.as_str()), (4, Weekday::Sat, 0, ""));
    assert!(bare.as_of == AsOf::Latest);
}

// ------------------------------------------------------------------------------------------------ the project

fn project_rig(name: &str) -> (std::path::PathBuf, PortablePaths) {
    let root = dir(name);
    for f in ["data", "proj", "temp"] {
        std::fs::create_dir_all(root.join(f)).expect("folder");
    }
    std::fs::write(root.join("data/Accounts.csv"), "OPENED,BAL\n2026-01-31,100\n2026-02-28,200\n").expect("accounts");
    micro_parquet::rules::set_data_dir_override(Some(root.clone()));
    let paths = PortablePaths { root_dir: root.clone(), macros_dir: root.join("m"), schemas_dir: root.join("s"), curves_dir: root.join("c"), buckets_file: root.join("b.json"), temp_dir: root.join("temp") };
    (root, paths)
}

#[test]
fn m2_the_calendar_tab_is_saved_as_a_recipe_not_rows_and_the_calendar_the_links_and_the_as_of_date_come_back_with_the_project() {
    use micro_parquet::project::*;
    let (root, paths) = project_rig("p9m2_project");
    let csv = root.join("data/Accounts.csv");
    let accounts = TabSnapshot {
        id: 1,
        key: Some("t1".into()),
        title: "Accounts".into(),
        source: TabSource::SingleFile { path: csv.clone(), is_parquet: false, config: micro_parquet::engine::auto_detect_smart_schema(&csv).expect("a schema") },
        steps: vec![],
        exec_steps: 0,
        col_formats: Default::default(),
        summary: Default::default(),
        control_col: None,
        chart: None,
        chart_tab: None,
        kept: None,
    };
    let calendar = TabSnapshot { id: 2, key: Some("t2".into()), title: "Calendar".into(), source: TabSource::Calendar { settings: july_sunday(), first_year: 2026, last_year: 2027 }, ..accounts.clone() };
    let pfile = root.join("proj/cal.mpproj");
    let mut cache = HashCache::new();
    let mut saved = build_project(&[accounts, calendar], Some(1), &project_dir_of(&pfile), &paths, &mut cache).expect("builds");
    assert!(saved.excluded.is_empty(), "the Calendar tab is kept: {:?}", saved.excluded.iter().map(|e| e.reason.clone()).collect::<Vec<_>>());
    saved.project.model = Model {
        calendar: Some(ModelCalendar { settings: july_sunday(), date_links: vec![DateLink { data_tab: "t1".into(), column: "OPENED".into() }], tab: "t2".into() }),
        as_of: AsOf::Date("2026-10-31".into()),
        ..Default::default()
    };
    write_project(&pfile, &saved.project).expect("writes");
    let text = std::fs::read_to_string(&pfile).expect("text");
    let v: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(v["project_version"], 2);
    assert_eq!(v["model"]["calendar"]["tab"], "t2");
    assert_eq!(v["model"]["calendar"]["settings"]["fy_start_month"], 7);
    assert_eq!(v["model"]["as_of"]["Date"], "2026-10-31");
    // the Calendar tab is a recipe: its settings and years, never its 730 rows
    let tab = v["tabs"].as_array().expect("tabs").iter().find(|t| t["key"] == "t2").expect("the Calendar tab");
    let recipe = tab["recipe"].to_string();
    assert!(recipe.contains("\"calendar\"") && recipe.contains("first_year") && recipe.contains("2027"), "{recipe}");
    assert!(text.len() < 20_000, "no rows are saved: {} bytes", text.len());
    // it opens again: the model is as it was, and the Calendar tab is made from its recipe
    let project = load_project(&pfile).expect("opens");
    assert_eq!(project.model, saved.project.model);
    let mut opener = Opener::new(project.clone(), &pfile, BTreeMap::new(), paths.clone());
    let mut sources = Vec::new();
    for i in opener.order() {
        let pt = project.tabs[i].clone();
        match opener.prepare(i) {
            Ok(Prepared::Table { source, steps }) => {
                opener.built(&pt.key, source.clone(), steps);
                sources.push((pt.key.clone(), source));
            }
            other => panic!("the tab {} did not open: {:?}", pt.key, other.is_ok()),
        }
    }
    let cal = sources.iter().find(|s| s.0 == "t2").expect("the Calendar").1.clone();
    match cal {
        TabSource::Calendar { settings, first_year, last_year } => assert_eq!((settings, first_year, last_year), (july_sunday(), 2026, 2027)),
        other => panic!("{other:?}"),
    }
    // save -> open -> save gives the same bytes but the time
    let again = root.join("proj/again.mpproj");
    write_project(&again, &project).expect("writes again");
    let strip = |t: &str| t.lines().filter(|l| !l.trim_start().starts_with("\"saved_at\"")).collect::<Vec<_>>().join("\n");
    assert_eq!(strip(&std::fs::read_to_string(&again).expect("again")), strip(&text));
}

#[test]
fn m2_a_date_link_joins_a_date_and_time_column_to_the_calendar_by_its_day_and_a_blank_date_is_blank_not_a_match() {
    let dd = dir("p9m2_datetime");
    let micros = |t: &str, us: i64| micro_parquet::dates::days_from_date(d(t)) as i64 * 86_400_000_000 + us;
    let tb = DataFrame::new(vec![
        Series::new("Value Date".into(), [Some(micros("2026-11-23", 3_600_000_000)), Some(micros("2026-11-23", 80_000_000_000)), Some(micros("2026-11-24", 0)), None]).cast(&DataType::Datetime(TimeUnit::Microseconds, None)).expect("datetimes"),
        Series::new("Balance".into(), [1.0, 2.0, 4.0, 8.0]),
    ])
    .expect("tb");
    let src = parquet(&dd, "tb", &tb);
    let link = date_link("t1", "Value Date", "t3");
    let scope = scope_with(vec![(link.clone(), calendar_tab(CalendarSettings::default(), 2026, 2026))], &dd);
    let field = ModelFieldRef::new(&link, "GL TB \u{2192} Calendar", CALENDAR_TITLE, "Day name");
    let before = tb.height();
    let b = with_scope(scope.clone(), || build_pivot(compile_tab_lazy(&src, &[], &dd).expect("tb"), &pivot_by(&field, None))).expect("pivot by Day name");
    let got = pairs(&b.df);
    // 23 Nov 2026 is a Monday and 24 Nov a Tuesday; the blank date is "(blank)"
    assert_eq!(got.iter().find(|p| p.0 == "Mon").map(|p| p.1), Some(3.0), "both times of 23 Nov count on the day: {got:?}");
    assert_eq!(got.iter().find(|p| p.0 == "Tue").map(|p| p.1), Some(4.0));
    assert_eq!(got.iter().find(|p| p.0 == "(blank)").map(|p| p.1), Some(8.0), "a blank date is not a match");
    assert!(got.iter().all(|p| p.0 != "(no match)"), "no date is outside the Calendar: {got:?}");
    assert_eq!(got.last().map(|p| p.1), Some(15.0));
    // the join never changes the row count
    let rows = with_scope(scope, || compile_tab_lazy(&src, &[], &dd).and_then(|lf| Ok(lf.collect()?))).expect("rows").height();
    assert_eq!(rows, before);
}
