//! W1: the Split date step on the synthetic 1,000-row tab: every new column equals `dates.rs` part by part,
//! blanks stay blank, a Datetime gives the parts of its date, a text column and a name that exists are refused,
//! the step declares its order, the Python export gives the same columns, and old macros load unchanged.

use crate::common::{dir, run_steps};
use crate::common8::*;
use micro_parquet::dates::*;
use micro_parquet::models::*;
use micro_parquet::sort_lists::SortOrder;
use polars::prelude::*;

fn split(column: &str, parts: &[DatePart], cal: CalendarSettings, prefix: &str) -> TransformStep {
    TransformStep::SplitDate { column: column.into(), parts: parts.to_vec(), calendar: cal, prefix: prefix.into() }
}

fn shown(s: &Series, i: usize) -> Option<String> {
    match s.get(i).expect("row") {
        AnyValue::Null => None,
        AnyValue::Date(d) => date_from_days(d).map(|d| d.format("%Y-%m-%d").to_string()),
        AnyValue::String(t) => Some(t.to_string()),
        AnyValue::StringOwned(t) => Some(t.to_string()),
        v => Some(v.to_string()),
    }
}

fn the_dates(df: &DataFrame) -> Vec<Option<chrono::NaiveDate>> {
    df.column("Open Date").expect("dates").cast(&DataType::Int32).expect("days").i32().expect("i32").into_iter().map(|v| v.and_then(date_from_days)).collect()
}

#[test]
fn w1_every_new_column_equals_the_calendar_core_and_three_blanks_stay_blank() {
    let df = date_tab();
    for cal in [CalendarSettings::default(), CalendarSettings { fy_start_month: 7, week_start: Weekday::Mon }] {
        let out = run_steps("w1_all", &df, &[split("Open Date", &DatePart::ALL, cal, "")]).expect("splits");
        assert_eq!(out.width(), df.width() + 14, "14 new columns");
        let dates = the_dates(&df);
        for p in DatePart::ALL {
            let name = format!("Open Date {}", p.label());
            let c = out.column(&name).unwrap_or_else(|_| panic!("the column '{name}'"));
            assert_eq!(c.dtype(), &p.dtype(), "{name}");
            assert_eq!(c.null_count(), 3, "{name}: the 3 blank dates give 3 blanks");
            for (i, d) in dates.iter().enumerate() {
                assert_eq!(shown(c, i), d.map(|d| p.value(d, &cal).to_string()), "{name} row {i}");
            }
        }
        // the rows and the other columns are untouched
        assert!(out.select(df.get_column_names_str()).expect("select").equals_missing(&df));
    }
}

#[test]
fn w1_a_datetime_column_gives_the_parts_of_its_date() {
    let df = date_tab();
    let cal = CalendarSettings { fy_start_month: 4, week_start: Weekday::Sun };
    let out = run_steps("w1_dt", &df, &[split("Open Date", &DatePart::ALL, cal, "D"), split("Open Time", &DatePart::ALL, cal, "T")]).expect("splits");
    for p in DatePart::ALL {
        let a = out.column(&format!("D {}", p.label())).expect("date part");
        let b = out.column(&format!("T {}", p.label())).expect("datetime part");
        assert!(a.equals_missing(b), "{p:?}: a Datetime gives the same part as its date");
    }
}

#[test]
fn w1_a_text_date_column_and_a_name_that_exists_are_refused() {
    let df = date_tab();
    let e = run_steps("w1_text", &df, &[split("Open Text", &[DatePart::Year], CalendarSettings::default(), "")]).expect_err("text is refused");
    assert!(format!("{e:#}").contains("'Open Text' is text. Set it to Date in Schema & Ingestion (D/M/Y) first."), "{e:#}");
    // a column called "Open Date Year" already
    let steps = vec![TransformStep::RenameColumn { old_name: "Branch".into(), new_name: "Open Date Year".into() }, split("Open Date", &[DatePart::Year, DatePart::Month], CalendarSettings::default(), "")];
    let e = run_steps("w1_clash", &df, &steps).expect_err("the name is taken");
    assert!(format!("{e:#}").contains("a column 'Open Date Year' already exists. Change the prefix"), "{e:#}");
    // with another prefix it works
    let steps = vec![steps[0].clone(), split("Open Date", &[DatePart::Year], CalendarSettings::default(), "Opened")];
    let out = run_steps("w1_prefix", &df, &steps).expect("another prefix");
    assert!(out.column("Opened Year").is_ok());
    // no part, a missing column, a fiscal month out of range
    assert!(format!("{:#}", run_steps("w1_none", &df, &[split("Open Date", &[], CalendarSettings::default(), "")]).expect_err("no part")).contains("tick at least one part"));
    assert!(format!("{:#}", run_steps("w1_missing", &df, &[split("Opened", &[DatePart::Year], CalendarSettings::default(), "")]).expect_err("missing")).contains("'Opened' was not found"));
    let bad = CalendarSettings { fy_start_month: 0, week_start: Weekday::Sat };
    assert!(format!("{:#}", run_steps("w1_bad", &df, &[split("Open Date", &[DatePart::FiscalYear], bad, "")]).expect_err("bad month")).contains("1 (January) to 12"));
}

#[test]
fn w1_the_step_declares_calendar_orders_and_a_pivot_by_day_name_lists_sat_to_fri() {
    let cal = CalendarSettings::default();
    let steps = vec![split("Open Date", &DatePart::DEFAULT_SPLIT, cal, "")];
    let orders = micro_parquet::engine::column_orders(&steps);
    let mut names: Vec<&String> = orders.keys().collect();
    names.sort();
    assert_eq!(names, ["Open Date Day name", "Open Date Month", "Open Date Month-Year", "Open Date Quarter"], "the text parts only (Year is a number)");
    assert!(orders.values().all(|o| *o == SortOrder::Calendar { week_start: Weekday::Sat }));
    // a rename keeps the order; a column choice that drops it loses it; a turned-off step declares nothing
    let mut more = steps.clone();
    more.push(TransformStep::RenameColumn { old_name: "Open Date Month".into(), new_name: "Month".into() });
    more.push(TransformStep::SelectAndReorderColumns { cols: vec!["Month".into(), "Balance".into()] });
    let o = micro_parquet::engine::column_orders(&more);
    assert_eq!(o.keys().collect::<Vec<_>>(), ["Month"]);
    assert!(micro_parquet::engine::column_orders(&[TransformStep::Skipped { step: Box::new(steps[0].clone()) }]).is_empty());
    // the pivot the builder makes from it (the field's order comes from the steps)
    let day = "Open Date Day name".to_string();
    let pivot = TransformStep::PivotTable {
        rows: vec![day.clone()],
        columns: vec![],
        values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None }],
        filters: vec![],
        row_totals: true,
        column_totals: true,
        subtotals: false,
        show_as: PivotShowAs::NoCalculation,
        scale: PivotScale::default(),
        sort_by_value: None,
        field_orders: vec![PivotFieldOrder { field: day.clone(), order: orders[&day].clone() }],
    };
    let out = run_steps("w1_pivot", &date_tab(), &[steps[0].clone(), pivot]).expect("pivots");
    let labels: Vec<Option<String>> = out.column(&day).expect("labels").str().expect("text").into_iter().map(|v| v.map(str::to_string)).collect();
    let labels: Vec<String> = labels.into_iter().flatten().collect();
    assert_eq!(&labels[..7], ["Sat", "Sun", "Mon", "Tue", "Wed", "Thu", "Fri"], "{labels:?}");
    assert_eq!(labels[7], "(blank)", "the 3 blank dates are one (blank) row: {labels:?}");
}

#[test]
fn w1_the_label_json_and_old_macros() {
    let s = split("Open Date", &DatePart::DEFAULT_SPLIT, CalendarSettings::default(), "");
    assert_eq!(s.short_label(), "Split date: Open Date \u{2192} 5 columns");
    assert_eq!(s.title(), "Split date");
    // every field has a default, so a hand-written or older file with fewer fields loads
    let j: TransformStep = serde_json::from_str(r#"{"SplitDate": {"column": "Open Date", "parts": ["Year"]}}"#).expect("loads");
    assert!(matches!(j, TransformStep::SplitDate { calendar, .. } if calendar == CalendarSettings::default()));
    // a macro saved with the step comes back the same, with its own calendar
    let d = dir("w1_macro");
    let mine = split("Open Date", &[DatePart::WeekOfYear], CalendarSettings { fy_start_month: 7, week_start: Weekday::Mon }, "Opened");
    let recipe = MacroRecipe { name: "m".into(), description: String::new(), created_at: String::new(), enforce_balance_control: false, control_column: None, optional_ingestion_schema: None, steps: vec![mine.clone()], summary: Default::default() };
    micro_parquet::macros::save_macro_to_file(&recipe, &d.join("m.mpq.json")).expect("saves");
    let back = micro_parquet::macros::load_macro_from_file(&d.join("m.mpq.json")).expect("loads");
    assert_eq!(serde_json::to_string(&back.steps[0]).expect("json"), serde_json::to_string(&mine).expect("json"));
    // the Phase 1 and 2 macros (no Split date) still load: their replay is in w0's goal-3 test
    for f in ["phase1_every_step.mpq.json", "phase2_every_step.mpq.json"] {
        micro_parquet::macros::load_macro_from_file(&crate::common::fixtures_dir().join(f)).expect("an old macro loads");
    }
}

#[test]
fn w1_the_generated_python_gives_identical_columns() {
    let cal = CalendarSettings { fy_start_month: 7, week_start: Weekday::Sat };
    let step = split("Open Date", &DatePart::ALL, cal, "");
    let py = micro_parquet::macros::generate_python_polars_code(std::path::Path::new("in.parquet"), true, &IngestionConfig::default(), &[step.clone()], None).expect("python");
    assert!(py.contains("def _mp_date_part") && py.contains(".alias(\"Open Date Fiscal year\")"), "{py}");
    assert!(micro_parquet::macros::python_untranslated_steps(&[step.clone()]).is_empty(), "Split date has a translation");
    if !have_python() {
        eprintln!("w1: python3 is not installed here, so the generated Python is not run (skipped)");
        return;
    }
    let d = dir("w1_py");
    let path = d.join("whole.py");
    std::fs::write(&path, &py).expect("script");
    let o = std::process::Command::new("python3").arg("-I").arg("-c").arg("import ast,sys; ast.parse(open(sys.argv[1], encoding='utf-8').read())").arg(&path).output().expect("python3");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    // the part function of the script, on every date and time of the synthetic tab (Polars is not needed for it)
    let df = date_tab();
    let out = run_steps("w1_py_rs", &df, &[step]).expect("splits");
    let ms: Vec<Option<i64>> = df.column("Open Time").expect("times").cast(&DataType::Int64).expect("ms").i64().expect("i64").into_iter().collect();
    let mut script = String::from(micro_parquet::macros::PY_DATE_PART_HELPER);
    script.push_str(&format!("import json\nMS = json.loads({:?})\nPARTS = json.loads({:?})\n", serde_json::to_string(&ms).expect("json"), serde_json::to_string(&DatePart::ALL.iter().map(|p| p.label()).collect::<Vec<_>>()).expect("json")));
    script.push_str(
        "EPOCH = _dt.datetime(1970, 1, 1)\nfor m in MS:\n    t = None if m is None else EPOCH + _dt.timedelta(milliseconds=m)\n    row = []\n    for p in PARTS:\n        v = _mp_date_part(t, p, 7, 5)\n        row.append(v.isoformat() if isinstance(v, _dt.date) else v)\n    print(json.dumps(row))\n",
    );
    let lines: Vec<serde_json::Value> = run_python(&d, "parts", &script).lines().map(|l| serde_json::from_str(l).expect("a row")).collect();
    assert_eq!(lines.len(), 1000);
    for (pi, p) in DatePart::ALL.iter().enumerate() {
        let c = out.column(&format!("Open Date {}", p.label())).expect("column");
        for (i, row) in lines.iter().enumerate() {
            let want = shown(c, i);
            let got = match &row[pi] {
                serde_json::Value::Null => None,
                serde_json::Value::String(s) => Some(s.clone()),
                v => Some(v.to_string()),
            };
            assert_eq!(got, want, "{p:?} row {i}: Python and the app agree");
        }
    }
}
