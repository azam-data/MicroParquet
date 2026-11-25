//! M5: named measures and ratios. A base measure is a column of a tab with its own aggregation, filters and sign; a ratio is a signed
//! sum of base measures over another, annualised with a day count when it is a rate. A subtotal or a total is the ratio of its own
//! numerator and denominator. Every number here is worked by hand.

use crate::common::{dir, parquet};
use crate::p9_m3::*;
use micro_parquet::dates::CalendarSettings;
use micro_parquet::measures::{check_measure, removal_refusal, DayCount, NamedMeasure, RatioFormat, Sign, Term};
use micro_parquet::model::*;
use micro_parquet::model_calendar::date_link;
use micro_parquet::models::*;
use polars::prelude::*;
use std::sync::Arc;

fn cal_tab(cal: CalendarSettings) -> ModelTab {
    ModelTab { key: "t2".into(), title: "Calendar".into(), source: TabSource::Calendar { settings: cal, first_year: 2025, last_year: 2029 }, steps: vec![] }
}

/// The model: t1 "Accounts" with its Calendar t2, a lookup t3 on the column `key` (when given), and the measures.
fn scope5(dd: &std::path::Path, as_of: &str, lookup: Option<(&str, &str, DataFrame)>, measures: Vec<NamedMeasure>) -> Option<Arc<ModelContext>> {
    let cal = settings(1);
    let mut links = vec![ScopeLink { link: date_link("t1", "Date", "t2"), lookup: Some(cal_tab(cal)), refusal: None }];
    if let Some((title, key, df)) = lookup {
        let tab = ModelTab { key: "t3".into(), title: title.into(), source: parquet(dd, "lookup", &df), steps: vec![] };
        links.push(ScopeLink { link: Link { data_tab: "t1".into(), lookup_tab: "t3".into(), keys: vec![KeyPair { data: key.into(), lookup: key.into() }], unmatched_value: None }, lookup: Some(tab), refusal: None });
    }
    Some(Arc::new(ModelContext {
        links,
        temp_dir: dd.to_path_buf(),
        calendar: Some(CalendarScope {
            settings: cal,
            tab: "t2".into(),
            date_links: vec![DateLink { data_tab: "t1".into(), column: "Date".into() }],
            as_of: Some(d(as_of)),
            titles: vec![("t1".into(), "Accounts".into())],
        }),
        kinds: vec![],
        measures,
    }))
}

fn gl_field(column: &str) -> ModelFieldRef {
    let l = Link { data_tab: "t1".into(), lookup_tab: "t3".into(), keys: vec![KeyPair { data: "GL".into(), lookup: "GL".into() }], unmatched_value: None };
    ModelFieldRef::new(&l, "Accounts \u{2192} GL map", "GL map", column)
}

fn head_is(head: &str) -> FilterCondition {
    FilterCondition { column: gl_field("Head").name, op: CondOp::Equals, value: head.into(), lookup: Some(gl_field("Head")), ..Default::default() }
}

fn base(name: &str, column: &str, agg: TimeAgg, head: Option<&str>, sign: Sign) -> NamedMeasure {
    NamedMeasure::Base { name: name.into(), tab: "t1".into(), column: column.into(), aggregation: agg, filters: head.map(|h| vec![head_is(h)]).unwrap_or_default(), sign, weight: None }
}

fn term(m: &str, sign: Sign) -> Term {
    Term { measure: m.into(), sign }
}

fn ratio(name: &str, num: Vec<Term>, den: Vec<Term>, dc: Option<DayCount>) -> NamedMeasure {
    NamedMeasure::Ratio { name: name.into(), tab: "t1".into(), numerator: num, denominator: den, annualise: dc, format: RatioFormat::Percent }
}

fn measure_value(name: &str, time: TimeValue) -> PivotValue {
    PivotValue { field: name.into(), agg: PivotAgg::Sum, weight_col: None, time: Some(TimeValue { agg: TimeAgg::Measure, ..time }) }
}

fn plain_measure(name: &str) -> PivotValue {
    measure_value(name, TimeValue::default())
}

fn gl_map() -> DataFrame {
    DataFrame::new(vec![strs("GL", &["INC", "EXP", "EA"]), strs("Head", &["Interest income", "Interest expense", "Earning assets"])]).expect("gl map")
}

/// One row a day for each GL from `from` to `to` (inclusive); amounts by GL and year.
fn tb(ranges: &[(&str, &str)], inc: &dyn Fn(i32) -> f64, exp: f64) -> DataFrame {
    let (mut dts, mut gls, mut bal) = (Vec::new(), Vec::new(), Vec::new());
    for (from, to) in ranges {
        let mut x = d(from);
        while x <= d(to) {
            use chrono::Datelike;
            let t = x.format("%Y-%m-%d").to_string();
            for (gl, v) in [("INC", -inc(x.year())), ("EXP", exp), ("EA", 3650.0)] {
                dts.push(t.clone());
                gls.push(gl);
                bal.push(v);
            }
            x = x.succ_opt().expect("next day");
        }
    }
    let refs: Vec<&str> = dts.iter().map(String::as_str).collect();
    DataFrame::new(vec![dates("Date", &refs).into(), strs("GL", &gls).into(), nums("Balance", &bal)]).expect("trial balance")
}

fn yield_measures() -> Vec<NamedMeasure> {
    vec![
        base("Interest income", "Balance", TimeAgg::TotalFlow, Some("Interest income"), Sign::Minus),
        base("Interest expense", "Balance", TimeAgg::TotalFlow, Some("Interest expense"), Sign::Plus),
        base("Average earning assets", "Balance", TimeAgg::AverageBalance, Some("Earning assets"), Sign::Plus),
        ratio("Yield 365", vec![term("Interest income", Sign::Plus)], vec![term("Average earning assets", Sign::Plus)], Some(DayCount::Actual365)),
        ratio("Yield 360", vec![term("Interest income", Sign::Plus)], vec![term("Average earning assets", Sign::Plus)], Some(DayCount::Actual360)),
        ratio("Yield Act", vec![term("Interest income", Sign::Plus)], vec![term("Average earning assets", Sign::Plus)], Some(DayCount::ActualActual)),
        ratio("NIM", vec![term("Interest income", Sign::Plus), term("Interest expense", Sign::Minus)], vec![term("Average earning assets", Sign::Plus)], Some(DayCount::Actual365)),
    ]
}

fn pct(v: Option<f64>) -> f64 {
    (v.expect("a number") * 100.0).round() / 100.0
}

#[test]
fn m5_a_yield_is_the_income_of_the_month_annualised_over_the_average_earning_assets_by_day_count() {
    let dd = dir("p9m5_yield");
    // credits are negative (1 a day), so Jan 2026 = -31 and, with sign Minus, 31; the earning assets are 3,650 every day
    let df = tb(&[("2026-01-01", "2026-01-31"), ("2028-01-01", "2028-01-31")], &|_| 1.0, 0.5);
    let sc = scope5(&dd, "2028-01-31", Some(("GL map", "GL", gl_map())), yield_measures());
    let s = spec(&[&month_year()], &[], vec![plain_measure("Interest income"), plain_measure("Average earning assets"), plain_measure("Yield 365"), plain_measure("Yield 360"), plain_measure("Yield Act")], &["Month-Year"]);
    let b = run(&dd, &df, sc, &s);
    assert_eq!(names(&b)[1..], ["Interest income", "Average earning assets", "Yield 365", "Yield 360", "Yield Act"]);
    assert_eq!(at(&b, "Jan 2026", "Interest income"), Some(31.0));
    assert_eq!(at(&b, "Jan 2026", "Average earning assets"), Some(3650.0));
    assert_eq!(pct(at(&b, "Jan 2026", "Yield 365")), 10.00);
    assert_eq!(pct(at(&b, "Jan 2026", "Yield 360")), 9.86);
    assert_eq!(pct(at(&b, "Jan 2026", "Yield Act")), 10.00);
    // the same numbers in Jan 2028, a leap year: Actual/Actual gives 31 x 366 / 31 / 3650
    assert_eq!(pct(at(&b, "Jan 2028", "Yield 365")), 10.00);
    assert_eq!(pct(at(&b, "Jan 2028", "Yield 360")), 9.86);
    assert_eq!(pct(at(&b, "Jan 2028", "Yield Act")), 10.03);
}

fn nim_data() -> DataFrame {
    // income 1 a day in 2025, 2 a day in 2026; expense 0.5 a day
    tb(&[("2025-01-01", "2026-02-28")], &|y| if y == 2025 { 1.0 } else { 2.0 }, 0.5)
}

#[test]
fn m5_the_nim_by_month_year_and_its_ytd_and_change_against_last_year_in_percentage_points() {
    let dd = dir("p9m5_nim");
    let df = nim_data();
    let sc = scope5(&dd, "2026-02-28", Some(("GL map", "GL", gl_map())), yield_measures());
    let (v, sply, diff) = (TimeShift::None, TimeShift::Sply, TimeShow::Difference);
    let tvv = |window: TimeWindow, shift: TimeShift, show: TimeShow| TimeValue { window, shift, show, ..Default::default() };
    let s = spec(
        &[&month_year()],
        &[],
        vec![
            plain_measure("NIM"),
            measure_value("NIM", tvv(TimeWindow::Period, sply, TimeShow::Value)),
            measure_value("NIM", tvv(TimeWindow::Period, sply, diff)),
            measure_value("NIM", tvv(TimeWindow::Ytd, v, TimeShow::Value)),
            measure_value("NIM", tvv(TimeWindow::Ytd, sply, diff)),
        ],
        &["Month-Year"],
    );
    let b = run(&dd, &df, sc, &s);
    assert_eq!(names(&b)[1..], ["NIM", "NIM \u{b7} SPLY", "NIM \u{b7} vs SPLY (pp)", "NIM \u{b7} YTD", "NIM \u{b7} YTD \u{b7} vs SPLY (pp)"]);
    // Jan 2026: income 62, expense 15.5, net 46.5 x 365 / 31 / 3650 = 15%; Jan 2025: net 15.5 -> 5%
    assert_eq!(pct(at(&b, "Jan 2026", "NIM")), 15.00);
    assert_eq!(pct(at(&b, "Jan 2025", "NIM")), 5.00);
    assert_eq!(pct(at(&b, "Jan 2026", "NIM \u{b7} SPLY")), 5.00);
    assert_eq!(pct(at(&b, "Jan 2026", "NIM \u{b7} vs SPLY (pp)")), 10.00);
    // Feb 2026: income 56, expense 14, net 42 x 365 / 28 / 3650 = 15%; Feb 2025: net 14 -> 5%
    assert_eq!(pct(at(&b, "Feb 2026", "NIM")), 15.00);
    assert_eq!(pct(at(&b, "Feb 2026", "NIM \u{b7} vs SPLY (pp)")), 10.00);
    // YTD of Feb 2026 (59 days): net 88.5 x 365 / 59 / 3650 = 15%
    assert_eq!(pct(at(&b, "Feb 2026", "NIM \u{b7} YTD")), 15.00);
    assert_eq!(pct(at(&b, "Feb 2026", "NIM \u{b7} YTD \u{b7} vs SPLY (pp)")), 10.00);
    // Jan 2025 has no year before it: blank, never 0
    assert_eq!(at(&b, "Jan 2025", "NIM \u{b7} vs SPLY (pp)"), None);
}

fn casa_tb() -> DataFrame {
    DataFrame::new(vec![
        dates("Date", &["2026-11-24"; 6]).into(),
        strs("Branch", &["B1", "B1", "B2", "B2", "B3", "B3"]).into(),
        strs("Product", &["CASA", "TD", "CASA", "TD", "CASA", "TD"]).into(),
        nums("Balance", &[100.0, 100.0, 50.0, 250.0, 90.0, 10.0]),
    ])
    .expect("deposits")
}

fn branch_master() -> DataFrame {
    DataFrame::new(vec![strs("Branch", &["B1", "B2", "B3"]), strs("Region", &["Dhaka", "Dhaka", "Khulna"])]).expect("branches")
}

fn casa_measures() -> Vec<NamedMeasure> {
    let casa = FilterCondition { column: "Product".into(), op: CondOp::Equals, value: "CASA".into(), ..Default::default() };
    vec![
        NamedMeasure::Base { name: "CASA closing".into(), tab: "t1".into(), column: "Balance".into(), aggregation: TimeAgg::ClosingBalance, filters: vec![casa], sign: Sign::Plus, weight: None },
        base("Deposits closing", "Balance", TimeAgg::ClosingBalance, None, Sign::Plus),
        ratio("CASA ratio", vec![term("CASA closing", Sign::Plus)], vec![term("Deposits closing", Sign::Plus)], None),
    ]
}

#[test]
fn m5_a_ratios_subtotal_and_total_come_from_its_own_numerator_and_denominator_not_the_average_of_the_regions() {
    let dd = dir("p9m5_casa");
    let df = casa_tb();
    let sc = scope5(&dd, "2026-11-24", Some(("Branch master", "Branch", branch_master())), casa_measures());
    let link = Link { data_tab: "t1".into(), lookup_tab: "t3".into(), keys: vec![KeyPair { data: "Branch".into(), lookup: "Branch".into() }], unmatched_value: None };
    let region = ModelFieldRef::new(&link, "Accounts \u{2192} Branch master", "Branch master", "Region");
    let mut s = spec(&[&region.name], &[], vec![plain_measure("CASA ratio"), plain_measure("CASA closing"), plain_measure("Deposits closing")], &[]);
    s.model_fields.push(region.clone());
    // the lookup key of this fixture is "Branch": the lookup tab is joined on it
    let b = run(&dd, &df, sc, &s);
    // Dhaka 150 / 500 = 30%, Khulna 90 / 100 = 90%, the total 240 / 600 = 40% (the average of the regions would be 60%)
    assert_eq!(pct(at(&b, "Dhaka", "CASA ratio")), 30.00);
    assert_eq!(pct(at(&b, "Khulna", "CASA ratio")), 90.00);
    assert_eq!(pct(at(&b, "Grand total", "CASA ratio")), 40.00);
    assert_eq!(at(&b, "Grand total", "CASA closing"), Some(240.0));
    assert_eq!(at(&b, "Grand total", "Deposits closing"), Some(600.0));
}

#[test]
fn m5_a_zero_denominator_is_blank_with_its_reason_and_an_annualised_balance_or_a_missing_measure_is_refused() {
    let dd = dir("p9m5_refuse");
    let df = casa_tb();
    // a denominator of 0: the CASA of nothing
    let zero = FilterCondition { column: "Product".into(), op: CondOp::Equals, value: "Nothing".into(), ..Default::default() };
    let mut ms = casa_measures();
    ms.push(NamedMeasure::Base { name: "No such".into(), tab: "t1".into(), column: "Balance".into(), aggregation: TimeAgg::ClosingBalance, filters: vec![zero], sign: Sign::Plus, weight: None });
    ms.push(ratio("Odd", vec![term("CASA closing", Sign::Plus)], vec![term("No such", Sign::Plus)], None));
    let sc = scope5(&dd, "2026-11-24", None, ms);
    let b = run(&dd, &df, sc.clone(), &spec(&["Branch"], &[], vec![plain_measure("Odd")], &[]));
    assert_eq!(at(&b, "B1", "Odd"), None);
    assert!(b.notes.iter().any(|n| n.text.contains("The denominator of 'Odd' is 0")), "{:?}", b.notes.iter().map(|n| n.text.clone()).collect::<Vec<_>>());
    // a measure that is not in the model
    let e = run_err(&dd, &df, sc, &spec(&["Branch"], &[], vec![plain_measure("Ghost")], &[]));
    assert!(e.contains("the measure 'Ghost' is not in the model"), "{e}");
    // the library refusals: annualising a balance, a duplicate name, a ratio of a missing measure
    let loans = base("Loans closing", "Balance", TimeAgg::ClosingBalance, None, Sign::Plus);
    let bad = ratio("Loan yield", vec![term("Loans closing", Sign::Plus)], vec![term("Loans closing", Sign::Plus)], Some(DayCount::Actual365));
    assert_eq!(check_measure(&[loans.clone()], &bad).as_deref(), Some("Only flows and period amounts can be annualised; 'Loans closing' is a balance."));
    assert!(check_measure(&[loans.clone()], &loans).is_some_and(|m| m.contains("already exists")));
    assert!(check_measure(&[], &bad).is_some_and(|m| m.contains("does not exist")));
    // a measure in use can't be removed, and the refusal lists the users
    let r = ratio("Share", vec![term("Loans closing", Sign::Plus)], vec![term("Loans closing", Sign::Plus)], None);
    let all = vec![loans.clone(), r];
    let m = removal_refusal(&all, "Loans closing", &["pivot 'NIM by month'".to_string()]).expect("refused");
    assert!(m.contains("ratio 'Share'") && m.contains("pivot 'NIM by month'"), "{m}");
    assert!(removal_refusal(&all, "Share", &[]).is_none());
    // the pivot itself refuses an annualised ratio of a balance
    let sc2 = scope5(&dd, "2026-11-24", None, vec![loans, ratio("Loan yield", vec![term("Loans closing", Sign::Plus)], vec![term("Loans closing", Sign::Plus)], Some(DayCount::Actual365))]);
    let e = run_err(&dd, &df, sc2, &spec(&["Branch"], &[], vec![plain_measure("Loan yield")], &[]));
    assert!(e.contains("Only flows and period amounts can be annualised; 'Loans closing' is a balance."), "{e}");
}

#[test]
fn m5_the_adr_on_a_month_end_tab_uses_averages_of_month_ends_and_says_so() {
    let dd = dir("p9m5_adr");
    // month-end accounts: advances 80, 100, 120 and deposits 100, 100, 100 at Jan, Feb, Mar 2026
    let df = DataFrame::new(vec![
        dates("Date", &["2026-01-31", "2026-01-31", "2026-02-28", "2026-02-28", "2026-03-31", "2026-03-31"]).into(),
        strs("GL", &["ADV", "DEP", "ADV", "DEP", "ADV", "DEP"]).into(),
        nums("Balance", &[80.0, 100.0, 100.0, 100.0, 120.0, 100.0]),
    ])
    .expect("month-ends");
    let map = DataFrame::new(vec![strs("GL", &["ADV", "DEP"]), strs("Head", &["Advances", "Deposits"])]).expect("map");
    let ms = vec![
        base("Advances avg", "Balance", TimeAgg::AverageBalance, Some("Advances"), Sign::Plus),
        base("Deposits avg", "Balance", TimeAgg::AverageBalance, Some("Deposits"), Sign::Plus),
        ratio("ADR", vec![term("Advances avg", Sign::Plus)], vec![term("Deposits avg", Sign::Plus)], None),
    ];
    let sc = scope5(&dd, "2026-03-31", Some(("GL map", "GL", map)), ms);
    let b = run(&dd, &df, sc, &spec(&[&month_year()], &[], vec![plain_measure("ADR")], &["Month-Year"]));
    // each month has its one month-end: 80 / 100, 100 / 100, 120 / 100; the total is the average of the three month-ends: 100 / 100
    assert_eq!(pct(at(&b, "Jan 2026", "ADR")), 80.00);
    assert_eq!(pct(at(&b, "Feb 2026", "ADR")), 100.00);
    assert_eq!(pct(at(&b, "Mar 2026", "ADR")), 120.00);
    assert_eq!(pct(at(&b, "Grand total", "ADR")), 100.00);
    assert!(b.warnings.iter().any(|w| w == "ADR: its average terms are averages of month-ends (the tab holds month-end data)."), "{:?}", b.warnings);
}

// ------------------------------------------------------------------------------------------------ files

fn csv_source(path: &std::path::Path) -> TabSource {
    TabSource::SingleFile { path: path.to_path_buf(), is_parquet: false, config: micro_parquet::engine::auto_detect_smart_schema(path).expect("a schema") }
}

fn snap(id: u64, key: &str, title: &str, source: TabSource, steps: Vec<TransformStep>) -> micro_parquet::project::TabSnapshot {
    let n = steps.len();
    micro_parquet::project::TabSnapshot { id, key: Some(key.into()), title: title.into(), source, steps, exec_steps: n, col_formats: Default::default(), summary: Default::default(), control_col: None, chart: None, chart_tab: None, kept: None }
}

#[test]
fn m5_a_project_with_measures_round_trips_and_headless_writes_the_same_pivot_tab_as_the_window() {
    use micro_parquet::project::*;
    let root = dir("p9m5_project");
    for f in ["data", "proj", "temp", "out"] {
        std::fs::create_dir_all(root.join(f)).expect("folder");
    }
    micro_parquet::rules::set_data_dir_override(Some(root.clone()));
    let paths = PortablePaths { root_dir: root.clone(), macros_dir: root.join("m"), schemas_dir: root.join("s"), curves_dir: root.join("c"), buckets_file: root.join("b.json"), temp_dir: root.join("temp") };
    // the yield example as files
    let df = tb(&[("2026-01-01", "2026-01-31"), ("2028-01-01", "2028-01-31")], &|_| 1.0, 0.5);
    let mut text = String::from("Date,GL,Balance\n");
    let (days, gls, bal) = (df.column("Date").expect("dates").cast(&DataType::Int32).expect("days"), crate::common::strings(&df, "GL"), crate::common::floats(&df, "Balance"));
    for (i, n) in days.i32().expect("days").into_iter().enumerate() {
        let date = micro_parquet::dates::date_from_days(n.expect("a date")).expect("a day").format("%Y-%m-%d");
        text.push_str(&format!("{},{},{}\n", date, gls[i].clone().unwrap_or_default(), bal[i].unwrap_or(0.0)));
    }
    std::fs::write(root.join("data/Accounts.csv"), text).expect("accounts");
    std::fs::write(root.join("data/GLmap.csv"), "GL,Head\nINC,Interest income\nEXP,Interest expense\nEA,Earning assets\n").expect("map");
    let s = spec(&[&month_year()], &[], vec![plain_measure("Yield 365"), plain_measure("Yield 360"), plain_measure("Yield Act")], &["Month-Year"]);
    let step = TransformStep::from_pivot_spec(s.clone());
    let snaps = vec![
        snap(1, "t1", "Accounts", csv_source(&root.join("data/Accounts.csv")), vec![]),
        snap(2, "t2", "Calendar", TabSource::Calendar { settings: settings(1), first_year: 2025, last_year: 2029 }, vec![]),
        snap(3, "t3", "GL map", csv_source(&root.join("data/GLmap.csv")), vec![]),
        // the pivot tab: the same rows as the data tab, with the pivot step that reads its model
        snap(4, "t4", "Yield pivot", csv_source(&root.join("data/Accounts.csv")), vec![step]),
    ];
    let link = Link { data_tab: "t1".into(), lookup_tab: "t3".into(), keys: vec![KeyPair { data: "GL".into(), lookup: "GL".into() }], unmatched_value: None };
    let model = Model {
        links: vec![link],
        calendar: Some(ModelCalendar { settings: settings(1), date_links: vec![DateLink { data_tab: "t1".into(), column: "Date".into() }], tab: "t2".into() }),
        measures: yield_measures(),
        ..Default::default()
    };
    let pfile = root.join("proj/yield.mpproj");
    let mut cache = HashCache::new();
    let mut saved = build_project(&snaps, Some(1), &project_dir_of(&pfile), &paths, &mut cache).expect("builds");
    saved.project.model = model.clone();
    write_project(&pfile, &saved.project).expect("writes");
    // the measures are in the file, as version 2, and open again unchanged
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&pfile).expect("text")).expect("json");
    assert_eq!(v["project_version"], 2);
    assert_eq!(v["model"]["measures"][3]["kind"], "Ratio");
    assert_eq!(v["model"]["measures"][3]["annualise"], "Actual/365");
    assert_eq!(v["model"]["measures"][0]["sign"], "Minus");
    assert_eq!(load_project(&pfile).expect("opens").model, model);
    // headless: the pivot tab holds the numbers of the window
    let written = export_project_headless(&pfile, &root.join("out"), &paths).unwrap_or_else(|e| panic!("headless: {e:#}"));
    let accounts = written.iter().find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("Yield pivot"))).expect("the pivot tab");
    let out = LazyFrame::scan_parquet(accounts, Default::default()).expect("scan").collect().expect("the tab");
    let labels = crate::common::strings(&out, out.get_column_names()[0]);
    let row = |l: &str, col: &str| crate::common::floats(&out, col)[labels.iter().position(|x| x.as_deref() == Some(l)).unwrap_or_else(|| panic!("row {l} in {labels:?}"))];
    assert_eq!(pct(row("Jan 2026", "Yield 365")), 10.00);
    assert_eq!(pct(row("Jan 2026", "Yield 360")), 9.86);
    assert_eq!(pct(row("Jan 2028", "Yield Act")), 10.03);
}

#[test]
fn m5_a_chart_of_the_nim_by_month_year_equals_the_pivot() {
    use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec};
    let dd = dir("p9m5_chart");
    let df = nim_data();
    let sc = scope5(&dd, "2026-02-28", Some(("GL map", "GL", gl_map())), yield_measures());
    let pivot = run(&dd, &df, sc.clone(), &spec(&[&month_year()], &[], vec![plain_measure("NIM")], &["Month-Year"]));
    let f = cal_field("Month-Year");
    let src = parquet(&dd, "accounts", &df);
    let chart_spec = ChartSpec { category: f.name.clone(), category_model: Some(f), value: Some("NIM".into()), agg: ChartAgg::Sum, time: Some(TimeValue::of(TimeAgg::Measure)), kind: ChartKind::Bar, data_tab: "t1".into(), ..Default::default() };
    let chart = micro_parquet::model::with_scope(sc, || micro_parquet::charts::calc::compute(micro_parquet::engine::compile_tab_lazy(&src, &[], &dd).expect("source"), &chart_spec)).unwrap_or_else(|e| panic!("the chart: {e:#}"));
    assert_eq!(chart.categories.len(), 14, "Jan 2025 to Feb 2026");
    for (i, c) in chart.categories.iter().enumerate() {
        let want = at(&pivot, c, "NIM");
        assert!(want.is_some(), "{c}");
        assert!(close(chart.values[0][i], want.unwrap_or(f64::NAN)), "{c}: chart {:?} pivot {:?}", chart.values[0][i], want);
    }
    assert_eq!(pct(chart.values[0][12]), 15.00, "Jan 2026");
}

#[test]
fn m5_a_ytd_window_of_a_month_reads_up_to_that_month_and_not_to_the_as_of_date() {
    let dd = dir("p9m5_ytd_anchor");
    // as of the end of Feb 2026 the data runs on to May: Jan's YTD is Jan alone (31 days), Feb's is 59 days
    let df = tb(&[("2025-01-01", "2026-05-31")], &|y| if y == 2025 { 1.0 } else { 2.0 }, 0.5);
    let sc = scope5(&dd, "2026-05-31", Some(("GL map", "GL", gl_map())), yield_measures());
    let ytd = TimeValue { window: TimeWindow::Ytd, ..Default::default() };
    let b = run(&dd, &df, sc, &spec(&[&month_year()], &[], vec![measure_value("NIM", ytd)], &["Month-Year"]));
    // every month of 2026 earns the same 15%, so the YTD is 15% from January on; its income shows the window: Jan 62, Feb 118
    assert_eq!(pct(at(&b, "Jan 2026", "NIM \u{b7} YTD")), 15.00);
    assert_eq!(pct(at(&b, "Mar 2026", "NIM \u{b7} YTD")), 15.00);
    let income = TimeValue { window: TimeWindow::Ytd, ..Default::default() };
    let sc2 = scope5(&dd, "2026-05-31", Some(("GL map", "GL", gl_map())), yield_measures());
    let b = run(&dd, &df, sc2, &spec(&[&month_year()], &[], vec![measure_value("Interest income", income)], &["Month-Year"]));
    assert_eq!(at(&b, "Jan 2026", "Interest income \u{b7} YTD"), Some(62.0));
    assert_eq!(at(&b, "Feb 2026", "Interest income \u{b7} YTD"), Some(118.0));
    assert_eq!(at(&b, "May 2026", "Interest income \u{b7} YTD"), Some(302.0));
}

#[test]
fn m5_a_renamed_measure_gives_the_same_numbers_in_a_ratio_a_pivot_and_a_chart_and_keeps_its_name_in_a_project() {
    use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec};
    use micro_parquet::project::*;
    let dd = dir("p9m5_rename");
    let df = nim_data();
    let src = parquet(&dd, "accounts", &df);
    let (old, new) = ("Interest income", "Interest earned");
    let f = cal_field("Month-Year");
    let chart_spec = |name: &str| ChartSpec { category: f.name.clone(), category_model: Some(f.clone()), value: Some(name.into()), agg: ChartAgg::Sum, time: Some(TimeValue::of(TimeAgg::Measure)), kind: ChartKind::Bar, data_tab: "t1".into(), ..Default::default() };
    let chart_of = |sc: Option<Arc<ModelContext>>, name: &str| {
        micro_parquet::model::with_scope(sc, || micro_parquet::charts::calc::compute(micro_parquet::engine::compile_tab_lazy(&src, &[], &dd).expect("source"), &chart_spec(name))).unwrap_or_else(|e| panic!("the chart: {e:#}"))
    };
    // before: NIM (a ratio that uses the measure), the measure in a pivot, and in a chart
    let before_measures = yield_measures();
    let sc = scope5(&dd, "2026-02-28", Some(("GL map", "GL", gl_map())), before_measures.clone());
    let pivot_spec = spec(&[&month_year()], &[], vec![plain_measure("NIM"), plain_measure(old)], &["Month-Year"]);
    let before = run(&dd, &df, sc.clone(), &pivot_spec);
    let chart_before = chart_of(sc, old);
    assert_eq!(at(&before, "Jan 2026", old), Some(62.0));
    // the rename, as the dialog does it: the measure, the ratios' terms, the saved pivot step, the chart's value
    let mut measures = before_measures.clone();
    let i = measures.iter().position(|m| m.name() == old).expect("the measure");
    match &mut measures[i] {
        NamedMeasure::Base { name, .. } => *name = new.to_string(),
        other => panic!("a base measure, not {other:?}"),
    }
    micro_parquet::measures::rename_in_ratios(&mut measures, old, new);
    let mut step = TransformStep::from_pivot_spec(pivot_spec);
    assert!(step.rename_measure(old, new));
    let mut chart_spec_after = chart_spec(old);
    assert!(chart_spec_after.rename_measure(old, new));
    assert_eq!(chart_spec_after.value.as_deref(), Some(new));
    // NIM's terms follow, and the other ratios too
    for m in measures.iter().filter(|m| m.is_ratio()) {
        if let NamedMeasure::Ratio { numerator, denominator, .. } = m {
            assert!(numerator.iter().chain(denominator.iter()).all(|t| t.measure != old), "{}: {:?}", m.name(), numerator);
        }
    }
    let sc = scope5(&dd, "2026-02-28", Some(("GL map", "GL", gl_map())), measures.clone());
    let after = run(&dd, &df, sc.clone(), &step.as_pivot_spec().expect("a pivot"));
    assert_eq!(names(&after)[1..], ["NIM", new]);
    for month in ["Jan 2025", "Jan 2026", "Feb 2026"] {
        assert_eq!(at(&after, month, "NIM"), at(&before, month, "NIM"), "NIM {month}");
        assert_eq!(at(&after, month, new), at(&before, month, old), "{new} {month}");
    }
    let chart_after = chart_of(sc, new);
    assert_eq!(chart_after.categories, chart_before.categories);
    assert_eq!(chart_after.values, chart_before.values);
    // a duplicate name is refused in words (case ignored), an empty one too
    let others: Vec<NamedMeasure> = measures.iter().filter(|m| m.name() != new).cloned().collect();
    let dup = base("interest EXPENSE", "Balance", TimeAgg::TotalFlow, None, Sign::Plus);
    assert_eq!(check_measure(&others, &dup).as_deref(), Some("A measure called 'interest EXPENSE' already exists. Names must be different."));
    assert_eq!(check_measure(&others, &base("  ", "Balance", TimeAgg::TotalFlow, None, Sign::Plus)).as_deref(), Some("Give the measure a name."));
    // the project keeps the new name everywhere
    let root = dir("p9m5_rename_project");
    for d in ["data", "proj", "temp"] {
        std::fs::create_dir_all(root.join(d)).expect("folder");
    }
    std::fs::write(root.join("data/Accounts.csv"), "Date,GL,Balance\n2026-01-01,INC,1\n").expect("data");
    let paths = PortablePaths { root_dir: root.clone(), macros_dir: root.join("m"), schemas_dir: root.join("s"), curves_dir: root.join("c"), buckets_file: root.join("b.json"), temp_dir: root.join("temp") };
    let snaps = vec![snap(1, "t1", "Accounts", csv_source(&root.join("data/Accounts.csv")), vec![step.clone()])];
    let pfile = root.join("proj/rename.mpproj");
    let mut cache = HashCache::new();
    let mut saved = build_project(&snaps, Some(1), &project_dir_of(&pfile), &paths, &mut cache).expect("builds");
    saved.project.model = Model { measures: measures.clone(), ..Default::default() };
    write_project(&pfile, &saved.project).expect("writes");
    let loaded = load_project(&pfile).expect("opens");
    assert_eq!(loaded.model.measures, measures);
    assert!(loaded.model.measures.iter().any(|m| m.name() == new) && loaded.model.measures.iter().all(|m| m.name() != old));
    // the name itself, the terms of the four ratios and the pivot step's value: no value, measure or term still says the old one
    let text = std::fs::read_to_string(&pfile).expect("text");
    assert!(text.matches(new).count() >= 6, "the new name is written {} times", text.matches(new).count());
    assert!(!text.contains(&format!("\"measure\": \"{old}\"")) && !text.contains(&format!("\"field\": \"{old}\"")), "no term or value keeps the old name");
}

#[test]
fn m5_a_pivot_column_of_a_named_measure_is_told_from_a_column_value() {
    use micro_parquet::pivot_table::is_measure_column;
    let one = spec(&[&month_year()], &[], vec![plain_measure("NIM")], &["Month-Year"]);
    assert!(is_measure_column(&one, "NIM"));
    assert!(is_measure_column(&one, "Dhaka"), "one value with Columns fields: every value column is the measure's");
    let tv = TimeValue { window: TimeWindow::Ytd, shift: TimeShift::Sply, show: TimeShow::Difference, ..Default::default() };
    let two = spec(&[&month_year()], &[], vec![plain_measure("NIM"), measure_value("NIM", tv), PivotValue { field: "Balance".into(), time: Some(TimeValue::of(TimeAgg::ClosingBalance)), ..Default::default() }], &["Month-Year"]);
    assert!(is_measure_column(&two, "NIM") && is_measure_column(&two, "Dhaka | NIM \u{b7} YTD \u{b7} vs SPLY (pp)") && is_measure_column(&two, "NIM (pp)"));
    assert!(!is_measure_column(&two, "Closing balance of Balance") && !is_measure_column(&two, "Dhaka | Closing balance of Balance"));
}
