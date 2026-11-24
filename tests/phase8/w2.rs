//! W2: date grouping without new columns. A pivot by a date field equals a pivot by the Split date column of
//! the same part; a chart by a part has its labels in Calendar order and the pivot's totals; a filter by Day
//! name keeps exactly the weekend rows; every grouping adds up to the ungrouped total with blanks as
//! "(blank)"; old pivots, charts and filters load unchanged.

use crate::common::{dir, parquet, run_steps};
use crate::common8::*;
use micro_parquet::charts::spec::ChartSpec;
use micro_parquet::dates::*;
use micro_parquet::models::*;
use micro_parquet::sort_lists::SortOrder;
use polars::prelude::*;

fn sum_balance() -> PivotValue {
    PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None }
}

fn pivot_by(field: &str, order: Option<SortOrder>, date_fields: Vec<DateFieldRef>) -> TransformStep {
    TransformStep::from_pivot_spec(PivotSpec {
        rows: vec![field.into()],
        values: vec![sum_balance()],
        row_totals: true,
        column_totals: true,
        field_orders: order.map(|o| vec![PivotFieldOrder { field: field.into(), order: o }]).unwrap_or_default(),
        date_fields,
        ..Default::default()
    })
}

fn texts(df: &DataFrame, name: &str) -> Vec<String> {
    df.column(name).expect("column").cast(&DataType::String).expect("text").str().expect("str").into_iter().map(|v| v.unwrap_or("").to_string()).collect()
}

fn nums(df: &DataFrame, name: &str) -> Vec<f64> {
    df.column(name).expect("column").cast(&DataType::Float64).expect("f64").f64().expect("f64").into_iter().map(|v| v.unwrap_or(f64::NAN)).collect()
}

fn total_balance(df: &DataFrame) -> f64 {
    df.column("Balance").expect("balance").f64().expect("f64").sum().unwrap_or(0.0)
}

#[test]
fn w2_a_pivot_by_a_date_field_equals_a_pivot_by_the_split_date_column() {
    let df = date_tab();
    let cal = CalendarSettings::default();
    for part in [DatePart::MonthYear, DatePart::Quarter, DatePart::DayName, DatePart::Year, DatePart::WeekOfYear, DatePart::FiscalYear] {
        let f = DateFieldRef::new("Open Date", part, cal);
        assert_eq!(f.name, format!("Open Date \u{b7} {}", part.label()));
        let by_field = run_steps("w2_pf", &df, &[pivot_by(&f.name, f.sort_order(), vec![f.clone()])]).expect("the date-field pivot");
        let split = TransformStep::SplitDate { column: "Open Date".into(), parts: vec![part], calendar: cal, prefix: String::new() };
        let col = format!("Open Date {}", part.label());
        let order = micro_parquet::engine::column_orders(&[split.clone()]).remove(&col);
        let by_col = run_steps("w2_pc", &df, &[split, pivot_by(&col, order, vec![])]).expect("the column pivot");
        assert_eq!(texts(&by_field, &f.name), texts(&by_col, &col), "{part:?}: the same labels in the same order");
        assert_eq!(nums(&by_field, "Sum of Balance"), nums(&by_col, "Sum of Balance"), "{part:?}: the same sums");
        // the part is not left in the table: only the label column and the value
        assert_eq!(by_field.width(), 2, "{part:?}");
    }
}

#[test]
fn w2_every_date_grouping_adds_up_to_the_ungrouped_total_with_blanks_as_blank() {
    let df = date_tab();
    let total = total_balance(&df);
    let blank_sum: f64 = [10usize, 500, 999].iter().map(|i| 1000.0 + *i as f64).sum();
    for cal in [CalendarSettings::default(), CalendarSettings { fy_start_month: 7, week_start: Weekday::Mon }] {
        for part in DatePart::ALL {
            let f = DateFieldRef::new("Open Date", part, cal);
            let out = run_steps("w2_tot", &df, &[pivot_by(&f.name, f.sort_order(), vec![f.clone()])]).expect("pivots");
            let labels = texts(&out, &f.name);
            let sums = nums(&out, "Sum of Balance");
            let n = sums.len();
            assert_eq!(labels[n - 1], "Grand total", "{part:?}");
            assert!((sums[n - 1] - total).abs() < 1e-6, "{part:?}: grand total {} = {}", sums[n - 1], total);
            assert!((sums[..n - 1].iter().sum::<f64>() - total).abs() < 1e-6, "{part:?}: the rows add up");
            let b = labels.iter().position(|l| l == "(blank)").unwrap_or_else(|| panic!("{part:?}: a (blank) row: {labels:?}"));
            assert!((sums[b] - blank_sum).abs() < 1e-9, "{part:?}: the 3 blank dates");
            assert_eq!(b, n - 2, "{part:?}: (blank) is the last label");
        }
    }
    // Calendar order: Q1…Q4, Jan…Dec, Sat…Fri
    let q = DateFieldRef::new("Open Date", DatePart::Quarter, CalendarSettings::default());
    let out = run_steps("w2_q", &df, &[pivot_by(&q.name, q.sort_order(), vec![q.clone()])]).expect("pivots");
    assert_eq!(texts(&out, &q.name), ["Q1", "Q2", "Q3", "Q4", "(blank)", "Grand total"]);
}

#[test]
fn w2_a_chart_by_quarter_has_q1_to_q4_in_order_and_the_pivots_totals() {
    let df = date_tab();
    let f = DateFieldRef::new("Open Date", DatePart::Quarter, CalendarSettings::default());
    let spec = ChartSpec { category: f.name.clone(), value: Some("Balance".into()), order: f.sort_order().expect("Calendar"), category_part: Some(f.clone()), ..Default::default() };
    let d = micro_parquet::charts::calc::compute(df.clone().lazy(), &spec).expect("the chart");
    assert_eq!(&d.categories[..4], ["Q1", "Q2", "Q3", "Q4"], "{:?}", d.categories);
    let pivot = run_steps("w2_cq", &df, &[pivot_by(&f.name, f.sort_order(), vec![f.clone()])]).expect("pivots");
    let ps = nums(&pivot, "Sum of Balance");
    for (i, c) in d.categories.iter().enumerate() {
        let j = texts(&pivot, &f.name).iter().position(|l| l == c).unwrap_or_else(|| panic!("'{c}' is a pivot row"));
        assert!((d.values[0][i].unwrap_or(0.0) - ps[j]).abs() < 1e-6, "{c}: chart {:?} = pivot {}", d.values[0][i], ps[j]);
    }
    let chart_total: f64 = d.values[0].iter().flatten().sum();
    assert!((chart_total - total_balance(&df)).abs() < 1e-6, "the chart adds up to the table");
    // a series and a facet by a part
    let y = DateFieldRef::new("Open Date", DatePart::Year, CalendarSettings::default());
    let fy = DateFieldRef::new("Open Date", DatePart::FiscalYear, CalendarSettings::default());
    let s2 = ChartSpec { series: Some(y.name.clone()), series_part: Some(y.clone()), facet: Some(fy.name.clone()), facet_part: Some(fy.clone()), ..spec.clone() };
    let d2 = micro_parquet::charts::calc::compute(df.clone().lazy(), &s2).expect("series and facet by a part");
    assert!(d2.series.iter().any(|s| s == "2025") && d2.facets.len() >= 3, "{:?} / {}", d2.series, d2.facets.len());
    // a part left behind after another column was picked is ignored, and the chart needs the date column
    let mut stale = spec.clone();
    stale.category = "Branch".into();
    assert!(stale.date_fields().is_empty());
    assert_eq!(micro_parquet::charts::library::missing_columns(&spec, &["Balance".to_string()]), vec!["Open Date".to_string()]);
    assert!(micro_parquet::charts::library::missing_columns(&spec, &["Balance".to_string(), "Open Date".to_string()]).is_empty());
    // the calendar is part of the calculation key
    let mon = DateFieldRef::new("Open Date", DatePart::Quarter, CalendarSettings { fy_start_month: 1, week_start: Weekday::Mon });
    assert_ne!(spec.calc_key(), ChartSpec { category_part: Some(mon), ..spec.clone() }.calc_key());
}

#[test]
fn w2_a_filter_day_name_in_fri_sat_keeps_exactly_the_weekend_rows() {
    let df = date_tab();
    let f = DateFieldRef::new("Open Date", DatePart::DayName, CalendarSettings::default());
    let cond = FilterCondition { column: "Open Date".into(), op: CondOp::InList, values: vec!["Fri".into(), "Sat".into()], part: Some(f), ..Default::default() };
    assert_eq!(cond.describe(), "Open Date \u{203a} Day name is in list Fri; Sat".replace("is in list", CondOp::InList.label()));
    let step = TransformStep::FilterGroup { conditions: vec![cond.clone()], join: GroupJoin::And, keep_matching: true };
    let out = run_steps("w2_flt", &df, &[step]).expect("filters");
    let want: Vec<f64> = df
        .column("Open Date")
        .expect("dates")
        .cast(&DataType::Int32)
        .expect("days")
        .i32()
        .expect("i32")
        .into_iter()
        .zip(nums(&df, "Balance"))
        .filter(|(d, _)| d.and_then(date_from_days).is_some_and(|d| matches!(chrono::Datelike::weekday(&d), chrono::Weekday::Fri | chrono::Weekday::Sat)))
        .map(|(_, b)| b)
        .collect();
    assert_eq!(nums(&out, "Balance"), want, "exactly the Friday and Saturday rows, in their order");
    assert_eq!(out.get_column_names_str(), df.get_column_names_str(), "the part's column is dropped after the filter");
    // Delete matching keeps the others (and the blanks)
    let del = TransformStep::FilterGroup { conditions: vec![cond.clone()], join: GroupJoin::And, keep_matching: false };
    assert_eq!(run_steps("w2_del", &df, &[del]).expect("filters").height(), df.height() - want.len());
    // by Year (a number) and with another condition
    let y = FilterCondition { column: "Open Date".into(), op: CondOp::InList, values: vec!["2025".into()], part: Some(DateFieldRef::new("Open Date", DatePart::Year, CalendarSettings::default())), ..Default::default() };
    let b = FilterCondition { column: "Branch".into(), op: CondOp::Equals, value: "Dhaka".into(), ..Default::default() };
    let out = run_steps("w2_year", &df, &[TransformStep::FilterGroup { conditions: vec![y, b], join: GroupJoin::And, keep_matching: true }]).expect("filters");
    assert!(out.height() > 0 && texts(&out, "Branch").iter().all(|t| t == "Dhaka"));
    // a part of a text column is refused with the message
    let bad = FilterCondition { column: "Open Text".into(), op: CondOp::InList, values: vec!["Fri".into()], part: Some(DateFieldRef::new("Open Text", DatePart::DayName, CalendarSettings::default())), ..Default::default() };
    let e = run_steps("w2_bad", &df, &[TransformStep::FilterGroup { conditions: vec![bad], join: GroupJoin::And, keep_matching: true }]).expect_err("refused");
    assert!(format!("{e:#}").contains("'Open Text' is text. Set it to Date"), "{e:#}");
    // the generated Python makes the part, filters on it, and drops it
    let py = micro_parquet::macros::generate_python_polars_code(std::path::Path::new("in.parquet"), true, &IngestionConfig::default(), &[TransformStep::FilterGroup { conditions: vec![cond], join: GroupJoin::And, keep_matching: true }], None).expect("python");
    // (W9: a part with an order is tested by its order key, so the picked days are written as their places in the week)
    assert!(py.contains("_mp_date_key(d, \"Day name\", 1, 5)") && py.contains(".alias(\"__mp_part0\")") && py.contains("lf = lf.drop([\"__mp_part0\"])"), "{py}");
    if have_python() {
        let d = dir("w2_py");
        let p = d.join("f.py");
        std::fs::write(&p, &py).expect("script");
        let o = std::process::Command::new("python3").arg("-I").arg("-c").arg("import ast,sys; ast.parse(open(sys.argv[1], encoding='utf-8').read())").arg(&p).output().expect("python3");
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }
}

#[test]
fn w2_a_real_column_with_the_fields_name_is_refused_and_the_value_lists_read_parts() {
    let df = date_tab();
    let f = DateFieldRef::new("Open Date", DatePart::Month, CalendarSettings::default());
    let clash = TransformStep::RenameColumn { old_name: "Branch".into(), new_name: f.name.clone() };
    let e = run_steps("w2_clash", &df, &[clash, pivot_by(&f.name, None, vec![f.clone()])]).expect_err("refused");
    assert!(format!("{e:#}").contains("already has a column called 'Open Date \u{b7} Month'"), "{e:#}");
    // the values of a date field, as the pivot's Filters checklist and the filter's picker read them
    let d = dir("w2_values");
    let src = parquet(&d, "t", &df);
    let req = micro_parquet::tab_jobs::PivotValuesRequest { tab_id: 1, source: src, steps: vec![TransformStep::date_field_step(&f)], field: f.name.clone(), temp_dir: d.clone() };
    let r = micro_parquet::tab_jobs::compute_pivot_values(req, &micro_parquet::jobs::JobCtx::inert()).expect("listed");
    let (values, _) = r.values.expect("values");
    let mut months: Vec<String> = values.iter().flatten().cloned().collect();
    months.sort_by(|a, b| SortOrder::Calendar { week_start: Weekday::Sat }.compare(&Some(a.clone()), &Some(b.clone())));
    assert_eq!(months, MONTHS_SHORT.to_vec());
    assert!(values.contains(&None), "the blank dates are a (blank) value");
    // a pivot filter on a date field
    let mut spec = PivotSpec { rows: vec!["Branch".into()], values: vec![sum_balance()], date_fields: vec![f.clone()], ..Default::default() };
    spec.filters = vec![PivotFilter { field: f.name.clone(), values: vec![Some("Jan".into())] }];
    let out = run_steps("w2_pfilter", &df, &[TransformStep::from_pivot_spec(spec)]).expect("pivots");
    let jan: f64 = run_steps("w2_jan", &df, &[TransformStep::FilterGroup { conditions: vec![FilterCondition { column: "Open Date".into(), op: CondOp::InList, values: vec!["Jan".into()], part: Some(f.clone()), ..Default::default() }], join: GroupJoin::And, keep_matching: true }])
        .map(|t| total_balance(&t))
        .expect("filters");
    assert!((nums(&out, "Sum of Balance").iter().sum::<f64>() - jan).abs() < 1e-6, "the pivot filter keeps the January rows");
}

#[test]
fn w2_old_pivots_charts_and_filters_load_unchanged() {
    // the Phase 7 forms, without the new fields
    let p: TransformStep = serde_json::from_str(r#"{"PivotTable": {"rows": ["Branch"], "values": [{"field": "Balance", "agg": "Sum"}]}}"#).expect("an old pivot");
    assert!(p.as_pivot_spec().expect("spec").date_fields.is_empty());
    let c: ChartSpec = serde_json::from_str(r#"{"category": "Branch", "value": "Balance", "kind": "Bar"}"#).expect("an old chart");
    assert!(c.category_part.is_none() && c.series_part.is_none() && c.facet_part.is_none() && c.date_fields().is_empty());
    let f: FilterCondition = serde_json::from_str(r#"{"column": "Branch", "op": "InList", "values": ["Dhaka"]}"#).expect("an old condition");
    assert!(f.part.is_none());
    assert_eq!(f.describe(), format!("Branch {} Dhaka", CondOp::InList.label()));
    // and the Phase 7 samples replay to the same rows (w0's goal-3 test covers pivots, the pack and the chart)
    let base = crate::common::fixtures_dir().join("phase7_replay");
    let (_, spec) = micro_parquet::charts::library::load_chart(&base.join("charts").join("Deposits by product.json")).expect("loads");
    assert!(spec.category_part.is_none());
}
