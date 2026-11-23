//! W9: date comparisons by part. A date column's "By" choice works with every operator, and the comparison is by the part's
//! order, never by its text: Year numerically, Month-Year and Year-Quarter by time, Month, Quarter and Day name in the calendar
//! order (Day name from the copied week start). A value that is not that part is refused in words, a blank date never matches,
//! old filters load as they were, and the Python export mirrors it. The window (the "By" choice staying when the operator
//! changes, in the tab filter and in the chart's filters) is tested in `src/ui/render_tests_p8.rs` (`w9_…`).

use crate::common::{dir, run_steps};
use crate::common8::*;
use chrono::{Datelike, NaiveDate};
use micro_parquet::dates::*;
use micro_parquet::models::*;
use polars::prelude::*;

fn balances(df: &DataFrame) -> Vec<f64> {
    df.column("Balance").expect("balance").f64().expect("f64").into_iter().map(|v| v.unwrap_or(f64::NAN)).collect()
}

/// The date of every row of a Date column (or of a Datetime, as its date), blank as `None`.
fn dates_of(df: &DataFrame, column: &str) -> Vec<Option<NaiveDate>> {
    let s = df.column(column).expect("dates").cast(&DataType::Date).expect("date").cast(&DataType::Int32).expect("days");
    s.i32().expect("i32").into_iter().map(|d| d.and_then(date_from_days)).collect()
}

fn cond(column: &str, op: CondOp, part: DatePart, cal: CalendarSettings, value: &str, value2: &str) -> FilterCondition {
    FilterCondition { column: column.into(), op, value: value.into(), value2: value2.into(), part: Some(DateFieldRef::new(column, part, cal)), ..Default::default() }
}

fn listed(column: &str, op: CondOp, part: DatePart, values: &[&str], blank: bool) -> FilterCondition {
    FilterCondition { column: column.into(), op, values: values.iter().map(|v| v.to_string()).collect(), or_blank: blank, value: values.join(", "), part: Some(DateFieldRef::new(column, part, CalendarSettings::default())), ..Default::default() }
}

/// The balances (the rows' identity) the filter keeps, in the tab's order.
fn keep(name: &str, df: &DataFrame, conds: Vec<FilterCondition>) -> Result<Vec<f64>, String> {
    run_steps(name, df, &[TransformStep::FilterGroup { conditions: conds, join: GroupJoin::And, keep_matching: true }]).map(|o| balances(&o)).map_err(|e| format!("{e:#}"))
}

/// What a filter must keep, worked out row by row with `chrono` (not through `dates.rs`' order keys).
fn want(df: &DataFrame, column: &str, pred: &dyn Fn(NaiveDate) -> bool) -> Vec<f64> {
    dates_of(df, column).iter().zip(balances(df)).filter(|(d, _)| d.is_some_and(pred)).map(|(_, b)| b).collect()
}

fn ym(d: NaiveDate) -> (i32, u32) {
    (d.year(), d.month())
}

const SAT: CalendarSettings = CalendarSettings { fy_start_month: 1, week_start: Weekday::Sat };
const MON: CalendarSettings = CalendarSettings { fy_start_month: 1, week_start: Weekday::Mon };

#[test]
fn w9_year_from_2025_keeps_the_later_years_and_month_year_runs_by_time_across_the_year_change() {
    let df = date_tab();
    // Year >= 2025: numbers, so 2025, 2026 and 2027 stay and 2024 goes
    let got = keep("w9_year", &df, vec![cond("Open Date", CondOp::Gte, DatePart::Year, SAT, "2025", "")]).expect("filters");
    let w = want(&df, "Open Date", &|d| d.year() >= 2025);
    assert_eq!(got, w);
    let years: std::collections::BTreeSet<i32> = dates_of(&df, "Open Date").iter().zip(balances(&df)).filter(|(_, b)| got.contains(b)).filter_map(|(d, _)| d.map(|d| d.year())).collect();
    assert_eq!(years.into_iter().collect::<Vec<_>>(), vec![2025, 2026, 2027]);
    assert!(got.len() < df.height() - 3, "2024 is out, and so are the 3 blanks");
    // the other four comparisons
    for (op, v, f) in [
        (CondOp::Gt, "2025", (|y: i32| y > 2025) as fn(i32) -> bool),
        (CondOp::Lt, "2026", |y| y < 2026),
        (CondOp::Lte, "2025", |y| y <= 2025),
        (CondOp::Equals, "2026", |y| y == 2026),
        (CondOp::NotEquals, "2026", |y| y != 2026),
    ] {
        assert_eq!(keep("w9_y", &df, vec![cond("Open Date", op, DatePart::Year, SAT, v, "")]).expect("filters"), want(&df, "Open Date", &|d| f(d.year())), "Year {} {v}", op.label());
    }

    // Month-Year >= Aug 2025 and <= Jan 2026 keeps exactly Aug 2025 to Jan 2026, across the year change (as text, "Jan 2026"
    // would come before "Aug 2025")
    let lo = cond("Open Date", CondOp::Gte, DatePart::MonthYear, SAT, "Aug 2025", "");
    let hi = cond("Open Date", CondOp::Lte, DatePart::MonthYear, SAT, "Jan 2026", "");
    let got = keep("w9_my", &df, vec![lo, hi]).expect("filters");
    let w = want(&df, "Open Date", &|d| ym(d) >= (2025, 8) && ym(d) <= (2026, 1));
    assert_eq!(got, w);
    let months: std::collections::BTreeSet<(i32, u32)> = dates_of(&df, "Open Date").iter().zip(balances(&df)).filter(|(_, b)| got.contains(b)).filter_map(|(d, _)| d.map(ym)).collect();
    assert_eq!(months.into_iter().collect::<Vec<_>>(), vec![(2025, 8), (2025, 9), (2025, 10), (2025, 11), (2025, 12), (2026, 1)]);
    // between is the same two limits; the value is read in other spellings, and a Datetime column gives the same rows
    for (a, b) in [("Aug 2025", "Jan 2026"), ("august 2025", "JAN-2026"), ("2025-08", "01/2026"), ("2025 Aug", "Jan, 2026")] {
        assert_eq!(keep("w9_between", &df, vec![cond("Open Date", CondOp::Between, DatePart::MonthYear, SAT, a, b)]).expect("between"), w, "{a} to {b}");
    }
    assert_eq!(keep("w9_dt", &df, vec![cond("Open Time", CondOp::Between, DatePart::MonthYear, SAT, "Aug 2025", "Jan 2026")]).expect("datetime"), want(&df, "Open Time", &|d| ym(d) >= (2025, 8) && ym(d) <= (2026, 1)));
    // Year-Quarter by time too: after 2025 Q4 is 2026 Q1
    let got = keep("w9_yq", &df, vec![cond("Open Date", CondOp::Gt, DatePart::YearQuarter, SAT, "2025 Q4", "")]).expect("filters");
    assert_eq!(got, want(&df, "Open Date", &|d| (d.year(), (d.month() - 1) / 3) > (2025, 3)));
    assert_eq!(keep("w9_yq2", &df, vec![cond("Open Date", CondOp::Between, DatePart::YearQuarter, SAT, "Q3 2025", "2026-Q1")]).expect("filters"), want(&df, "Open Date", &|d| (d.year(), (d.month() - 1) / 3) >= (2025, 2) && (d.year(), (d.month() - 1) / 3) <= (2026, 0)));
}

#[test]
fn w9_month_before_apr_keeps_jan_to_mar_of_every_year_and_day_name_runs_from_the_week_start() {
    let df = date_tab();
    // Month < Apr: Jan, Feb and Mar of every year (the calendar order, not "Apr" < "Aug" < "Dec" < "Feb")
    let got = keep("w9_month", &df, vec![cond("Open Date", CondOp::Lt, DatePart::Month, SAT, "Apr", "")]).expect("filters");
    assert_eq!(got, want(&df, "Open Date", &|d| d.month() < 4));
    let months: std::collections::BTreeSet<u32> = dates_of(&df, "Open Date").iter().zip(balances(&df)).filter(|(_, b)| got.contains(b)).filter_map(|(d, _)| d.map(|d| d.month())).collect();
    assert_eq!(months.into_iter().collect::<Vec<_>>(), vec![1, 2, 3]);
    let years: std::collections::BTreeSet<i32> = dates_of(&df, "Open Date").iter().zip(balances(&df)).filter(|(_, b)| got.contains(b)).filter_map(|(d, _)| d.map(|d| d.year())).collect();
    assert!(years.len() >= 3, "every year of the tab: {years:?}");
    assert_eq!(keep("w9_month_long", &df, vec![cond("Open Date", CondOp::Gte, DatePart::Month, SAT, "September", "")]).expect("filters"), want(&df, "Open Date", &|d| d.month() >= 9));
    assert_eq!(keep("w9_month_btw", &df, vec![cond("Open Date", CondOp::Between, DatePart::Month, SAT, "mar", "Jun")]).expect("filters"), want(&df, "Open Date", &|d| (3..=6).contains(&d.month())));
    // Quarter >= Q3
    assert_eq!(keep("w9_q", &df, vec![cond("Open Date", CondOp::Gte, DatePart::Quarter, SAT, "q3", "")]).expect("filters"), want(&df, "Open Date", &|d| d.month() >= 7));

    // Day name >= Wed with a Saturday start: Wed, Thu, Fri (Sat, Sun, Mon, Tue are earlier in that week)
    use chrono::Weekday as W;
    let got = keep("w9_day", &df, vec![cond("Open Date", CondOp::Gte, DatePart::DayName, SAT, "Wed", "")]).expect("filters");
    assert_eq!(got, want(&df, "Open Date", &|d| matches!(d.weekday(), W::Wed | W::Thu | W::Fri)));
    let days: std::collections::BTreeSet<String> = dates_of(&df, "Open Date").iter().zip(balances(&df)).filter(|(_, b)| got.contains(b)).filter_map(|(d, _)| d.map(|d| d.weekday().to_string())).collect();
    assert_eq!(days.into_iter().collect::<Vec<_>>(), vec!["Fri".to_string(), "Thu".to_string(), "Wed".to_string()]);
    // with a Monday start the same text keeps Wed to Sun
    let got = keep("w9_day_mon", &df, vec![cond("Open Date", CondOp::Gte, DatePart::DayName, MON, "Wednesday", "")]).expect("filters");
    assert_eq!(got, want(&df, "Open Date", &|d| matches!(d.weekday(), W::Wed | W::Thu | W::Fri | W::Sat | W::Sun)));
    // between Sun and Tue with a Saturday start
    assert_eq!(keep("w9_day_btw", &df, vec![cond("Open Date", CondOp::Between, DatePart::DayName, SAT, "Sun", "Tue")]).expect("filters"), want(&df, "Open Date", &|d| matches!(d.weekday(), W::Sun | W::Mon | W::Tue)));
}

#[test]
fn w9_every_operator_by_a_part_leaves_a_blank_date_out_and_a_value_that_is_not_the_part_is_refused() {
    let df = date_tab();
    let blanks: Vec<f64> = dates_of(&df, "Open Date").iter().zip(balances(&df)).filter(|(d, _)| d.is_none()).map(|(_, b)| b).collect();
    assert_eq!(blanks.len(), 3);
    let by = DatePart::Month;
    let ops = [
        cond("Open Date", CondOp::Equals, by, SAT, "Jan", ""),
        cond("Open Date", CondOp::NotEquals, by, SAT, "Jan", ""),
        cond("Open Date", CondOp::Gt, by, SAT, "Jan", ""),
        cond("Open Date", CondOp::Gte, by, SAT, "Jan", ""),
        cond("Open Date", CondOp::Lt, by, SAT, "Dec", ""),
        cond("Open Date", CondOp::Lte, by, SAT, "Dec", ""),
        cond("Open Date", CondOp::Between, by, SAT, "Jan", "Dec"),
        listed("Open Date", CondOp::InList, by, &["Jan", "Feb"], false),
        listed("Open Date", CondOp::Equals, by, &["Jan"], false),
        listed("Open Date", CondOp::NotEquals, by, &["Jan"], false),
    ];
    for c in ops {
        let got = keep("w9_blank", &df, vec![c.clone()]).expect("filters");
        assert!(blanks.iter().all(|b| !got.contains(b)), "{}: a blank date never matches", c.describe());
    }
    // "not in list" keeps the rows with no date, as it does for every column, unless (blank) is picked too
    let kept = keep("w9_notin", &df, vec![listed("Open Date", CondOp::NotInList, by, &["Jan"], false)]).expect("filters");
    assert!(blanks.iter().all(|b| kept.contains(b)));
    let mut w: Vec<f64> = dates_of(&df, "Open Date").iter().zip(balances(&df)).filter(|(d, _)| d.map_or(true, |d| d.month() != 1)).map(|(_, b)| b).collect();
    w.sort_by(f64::total_cmp);
    let mut k = kept.clone();
    k.sort_by(f64::total_cmp);
    assert_eq!(k, w, "every row that is not in January, the blanks included");
    let kept = keep("w9_notin_blank", &df, vec![listed("Open Date", CondOp::NotInList, by, &["Jan"], true)]).expect("filters");
    assert!(blanks.iter().all(|b| !kept.contains(b)), "(blank) picked: they are left out too");
    let only_blank = keep("w9_blank_only", &df, vec![listed("Open Date", CondOp::InList, by, &[], true)]).expect("filters");
    assert_eq!(only_blank, blanks, "(blank) picked in a list matches the blank dates");

    // a value that is not that part is refused, in words, naming the value and the part
    let df = date_tab();
    for (part, bad, example) in [
        (DatePart::Month, "13", "Jan"),
        (DatePart::Month, "Janu", "Jan"),
        (DatePart::Year, "Aug 2025", "2026"),
        (DatePart::Year, "25", "2026"),
        (DatePart::Quarter, "Q5", "Q1"),
        (DatePart::Quarter, "3", "Q1"),
        (DatePart::MonthYear, "2025", "Aug 2026"),
        (DatePart::MonthYear, "Foo 2025", "Aug 2026"),
        (DatePart::MonthYear, "Aug 25", "Aug 2026"),
        (DatePart::YearQuarter, "2025", "2026 Q1"),
        (DatePart::YearQuarter, "2025 Q9", "2026 Q1"),
        (DatePart::DayName, "Someday", "Sat"),
    ] {
        let e = keep("w9_bad", &df, vec![cond("Open Date", CondOp::Gt, part, SAT, bad, "")]).expect_err("refused");
        assert!(e.contains(&format!("'{}' is not a {}. Write it like {}.", bad, part.label(), example)), "{} {bad}: {e}", part.label());
        // in a list, the same refusal
        let e = keep("w9_bad_list", &df, vec![listed("Open Date", CondOp::InList, part, &[bad], false)]).expect_err("refused");
        assert!(e.contains("is not a ") && e.contains(bad), "{} {bad}: {e}", part.label());
    }
    let e = keep("w9_empty", &df, vec![cond("Open Date", CondOp::Gte, DatePart::Month, SAT, "  ", "")]).expect_err("refused");
    assert!(e.contains("Enter a Month, like Jan."), "{e}");
    // a range written back to front, a text test, a part with no order, a text column
    let e = keep("w9_back", &df, vec![cond("Open Date", CondOp::Between, DatePart::MonthYear, SAT, "Jan 2026", "Aug 2025")]).expect_err("refused");
    assert!(e.contains("'Jan 2026' comes after 'Aug 2025'. Write the earlier one first."), "{e}");
    let e = keep("w9_contains", &df, vec![cond("Open Date", CondOp::Contains, DatePart::Month, SAT, "Ja", "")]).expect_err("refused");
    assert!(e.contains("'contains' cannot be used on Open Date \u{203a} Month") && e.contains("set By to Date"), "{e}");
    let e = keep("w9_week", &df, vec![cond("Open Date", CondOp::Gt, DatePart::WeekOfYear, SAT, "W05", "")]).expect_err("refused");
    assert!(e.contains("that part has no order to compare by"), "{e}");
    let e = keep("w9_text", &df, vec![cond("Open Text", CondOp::Gt, DatePart::Month, SAT, "Jan", "")]).expect_err("refused");
    assert!(e.contains("'Open Text' is text. Set it to Date"), "{e}");
}

#[test]
fn w9_old_filters_load_unchanged_and_the_parts_with_no_order_still_filter_by_their_labels() {
    let df = date_tab();
    // a filter saved before W2 has no part; one saved by W2 has the part and its picked labels. Both load and filter as they did.
    let before_w2 = r#"{"column":"Open Date","op":"Gte","value":"2025-06-01","value2":"","values":[],"or_blank":false}"#;
    let c: FilterCondition = serde_json::from_str(before_w2).expect("an old filter loads");
    assert!(c.part.is_none());
    assert_eq!(keep("w9_old", &df, vec![c]).expect("filters"), want(&df, "Open Date", &|d| d >= NaiveDate::from_ymd_opt(2025, 6, 1).expect("date")));
    let w2 = serde_json::to_string(&listed("Open Date", CondOp::InList, DatePart::DayName, &["Fri", "Sat"], false)).expect("json");
    let c: FilterCondition = serde_json::from_str(&w2).expect("a W2 filter loads");
    assert_eq!(c.part.as_ref().map(|p| p.part), Some(DatePart::DayName));
    use chrono::Weekday as W;
    assert_eq!(keep("w9_w2", &df, vec![c.clone()]).expect("filters"), want(&df, "Open Date", &|d| matches!(d.weekday(), W::Fri | W::Sat)));
    // and its saved form is the same shape: the part is stored once, with its calendar
    assert_eq!(serde_json::from_str::<FilterCondition>(&serde_json::to_string(&c).expect("json")).expect("loads"), c);
    // a part with no order (Week of year) still picks by its labels, as before
    let wk = listed("Open Date", CondOp::InList, DatePart::WeekOfYear, &["W05"], false);
    let got = keep("w9_week_label", &df, vec![wk]).expect("filters");
    assert_eq!(got, want(&df, "Open Date", &|d| SAT.week_of_year(d) == 5));
    assert!(!got.is_empty());
    // the chip says what the filter does
    assert_eq!(cond("Open Date", CondOp::Gte, DatePart::MonthYear, SAT, "Aug 2025", "").describe(), "Open Date \u{203a} Month-Year \u{2265} Aug 2025");
    assert_eq!(cond("Open Date", CondOp::Between, DatePart::DayName, SAT, "Sun", "Tue").describe(), "Open Date \u{203a} Day name between Sun and Tue");
    // the filter's own list of parts
    assert_eq!(DatePart::FILTER_BY, [DatePart::Year, DatePart::MonthYear, DatePart::YearQuarter, DatePart::Month, DatePart::Quarter, DatePart::DayName]);
}

#[test]
fn w9_the_python_export_compares_by_the_same_keys_and_keeps_the_same_rows() {
    let df = date_tab();
    let conds = vec![cond("Open Date", CondOp::Gte, DatePart::MonthYear, SAT, "Aug 2025", ""), cond("Open Date", CondOp::Lte, DatePart::MonthYear, SAT, "Jan 2026", ""), cond("Open Date", CondOp::Lt, DatePart::DayName, SAT, "Thu", ""), listed("Open Date", CondOp::NotInList, DatePart::Quarter, &["Q2"], false)];
    let step = TransformStep::FilterGroup { conditions: conds.clone(), join: GroupJoin::And, keep_matching: true };
    let py = micro_parquet::macros::generate_python_polars_code(std::path::Path::new("in.parquet"), true, &IngestionConfig::default(), &[step], None).expect("python");
    assert!(py.contains("_mp_date_key(d, \"Month-Year\", 1, 5), return_dtype=pl.Int64).alias(\"__mp_part0\")"), "{py}");
    assert!(py.contains("_mp_date_key(d, \"Day name\", 1, 5), return_dtype=pl.Int64).alias(\"__mp_part2\")"), "{py}");
    assert!(py.contains("pl.col(\"__mp_part0\") >= 24307") && py.contains("pl.col(\"__mp_part1\") <= 24312"), "Aug 2025 = 2025*12+7, Jan 2026 = 2026*12: {py}");
    assert!(py.contains("lf = lf.drop([\"__mp_part0\", \"__mp_part1\", \"__mp_part2\", \"__mp_part3\"])"), "{py}");
    // a value that is not the part: the script refuses, as the app does
    let bad = TransformStep::FilterGroup { conditions: vec![cond("Open Date", CondOp::Gt, DatePart::Month, SAT, "13", "")], join: GroupJoin::And, keep_matching: true };
    let py_bad = micro_parquet::macros::generate_python_polars_code(std::path::Path::new("in.parquet"), true, &IngestionConfig::default(), &[bad], None).expect("python");
    assert!(py_bad.contains("raise ValueError(\"Filter: '13' is not a Month. Write it like Jan.\")"), "{py_bad}");

    if have_python() {
        // Polars is not installed here, so the generated script's key function and its limits are run in plain Python over the
        // tab's 1,000 dates, and the rows it keeps are the rows the app keeps
        let d = dir("w9_py");
        std::fs::write(d.join("gen.py.txt"), &py).expect("script");
        let rows: Vec<String> = dates_of(&df, "Open Date").iter().map(|x| x.map(|x| format!("\"{}\"", x.format("%Y-%m-%d"))).unwrap_or_else(|| "null".into())).collect();
        std::fs::write(d.join("dates.json"), format!("[{}]", rows.join(","))).expect("dates");
        let script = format!(
            r#"{helper}
import json, re, sys
src = open({gen:?}, encoding="utf-8").read()
dates = [None if x is None else _dt.date.fromisoformat(x) for x in json.load(open({dates:?}, encoding="utf-8"))]
cols = {{}}
for part, fy, ws, idx in re.findall(r'_mp_date_key\(d, "([^"]+)", (\d+), (\d+)\), return_dtype=pl\.Int64\)\.alias\("__mp_part(\d+)"\)', src):
    cols[idx] = [_mp_date_key(d, part, int(fy), int(ws)) for d in dates]
ops = {{">=": lambda a, b: a >= b, "<=": lambda a, b: a <= b, ">": lambda a, b: a > b, "<": lambda a, b: a < b}}
tests = []
for idx, op, lit in re.findall(r'pl\.col\("__mp_part(\d+)"\) (>=|<=|>|<) (-?\d+)', src):
    tests.append((idx, ops[op], int(lit)))
lists = re.findall(r'_mp_in_values\("__mp_part(\d+)", \[([^\]]*)\]', src)
keep = []
for i in range(len(dates)):
    ok = all(cols[idx][i] is not None and f(cols[idx][i], lit) for idx, f, lit in tests)
    for idx, items in lists:
        vals = [int(x.strip().strip('"')) for x in items.split(",") if x.strip()]
        k = cols[idx][i]
        ok = ok and (k is None or k not in vals)
    if ok:
        keep.append(i)
print(json.dumps(keep))
"#,
            helper = micro_parquet::macros::PY_DATE_PART_HELPER,
            gen = d.join("gen.py.txt").to_string_lossy(),
            dates = d.join("dates.json").to_string_lossy()
        );
        let out = run_python(&d, "w9_keys", &script).replace('\n', "");
        let kept_idx: Vec<usize> = serde_json::from_str(out.trim()).expect("the rows the script keeps");
        let all = balances(&df);
        let py_rows: Vec<f64> = kept_idx.iter().map(|i| all[*i]).collect();
        assert_eq!(py_rows, keep("w9_py_app", &df, conds).expect("the app's rows"), "the script keeps the app's rows");
        assert!(!py_rows.is_empty());
    }
}

#[test]
fn w9_a_chart_filter_by_a_part_keeps_the_rows_between_its_limits_and_says_so_in_the_subtitle() {
    use micro_parquet::charts::calc::compute;
    use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec};
    let df = date_tab();
    let f = cond("Open Date", CondOp::Between, DatePart::MonthYear, SAT, "Aug 2025", "Jan 2026");
    let spec = ChartSpec { category: "Branch".into(), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 50, filters: vec![f.clone()], ..Default::default() };
    spec.check_filters().expect("a filter with its limits is fine");
    let data = compute(micro_parquet::engine::filter_by_conditions(df.clone().lazy(), &spec.filters).expect("filtered"), &ChartSpec { filters: vec![], ..spec.clone() }).expect("chart");
    let want: f64 = want(&df, "Open Date", &|d| ym(d) >= (2025, 8) && ym(d) <= (2026, 1)).iter().sum();
    let got: f64 = data.values.iter().flatten().flatten().sum();
    assert!((got - want).abs() < 1e-6, "{got} vs {want}");
    assert_eq!(spec.filtered_text(), "Filtered: Open Date \u{b7} Month-Year between Aug 2025 and Jan 2026");
    let gt = ChartSpec { filters: vec![cond("Open Date", CondOp::Gt, DatePart::Quarter, SAT, "Q2", "")], ..spec.clone() };
    assert_eq!(gt.filtered_text(), "Filtered: Open Date \u{b7} Quarter > Q2");
    // a comparison with no limit is refused in words; a back-to-front range is refused by the calculation
    let empty = ChartSpec { filters: vec![cond("Open Date", CondOp::Gte, DatePart::Month, SAT, "", "")], ..spec.clone() };
    let e = format!("{:#}", empty.check_filters().expect_err("refused"));
    assert!(e.contains("has no value. Enter it, or remove the filter."), "{e}");
    let back = micro_parquet::engine::filter_by_conditions(df.clone().lazy(), &[cond("Open Date", CondOp::Between, DatePart::MonthYear, SAT, "Jan 2026", "Aug 2025")]).and_then(|l| l.collect().map_err(Into::into));
    assert!(format!("{:#}", back.expect_err("refused")).contains("comes after"));
    // the spec keeps the filter through its file
    let json = serde_json::to_string(&spec).expect("json");
    let again: ChartSpec = serde_json::from_str(&json).expect("loads");
    assert_eq!(again.filters, spec.filters);
}
