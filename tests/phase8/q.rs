//! Q1 and Q2 (owner decisions on Session A): chart series and facet orders, and the Python Tenor day count.

use crate::common::dir;
use crate::common8::*;
use micro_parquet::charts::spec::ChartSpec;
use micro_parquet::dates::*;
use polars::prelude::*;
use micro_parquet::sort_lists::{parse_tenor_days, SortOrder};

fn year() -> DateFieldRef {
    DateFieldRef::new("Open Date", DatePart::Year, CalendarSettings::default())
}

fn base() -> ChartSpec {
    ChartSpec { category: "Branch".into(), value: Some("Balance".into()), ..Default::default() }
}

#[test]
fn q1_a_new_chart_with_series_year_lists_2024_2025_2026_and_a_phase7_chart_keeps_its_order() {
    let df = date_tab();
    let y = year();
    // the series by size comes first (Phase 7 behaviour): 2026 is the smallest year in this tab, so it is not in year order
    let phase7 = ChartSpec { series: Some(y.name.clone()), series_part: Some(y.clone()), ..base() };
    let old = micro_parquet::charts::calc::compute(df.clone().lazy(), &phase7).expect("the Phase 7 chart");
    let by_size: Vec<String> = old.series.clone();
    assert_ne!(by_size, ["2024", "2025", "2026"], "the tab is built so that size order differs from year order: {by_size:?}");
    // a new chart: the order of the part (Natural for a year) goes in series_order
    let new = ChartSpec { series_order: Some(y.sort_order().unwrap_or(SortOrder::Natural)), ..phase7.clone() };
    let d = micro_parquet::charts::calc::compute(df.clone().lazy(), &new).expect("the new chart");
    assert_eq!(&d.series[..3], ["2024", "2025", "2026"], "{:?}", d.series);
    // the numbers are the same, only the order differs
    let total = |d: &micro_parquet::charts::data::ChartData| d.values.iter().flatten().flatten().sum::<f64>();
    assert!((total(&d) - total(&old)).abs() < 1e-6);
    for (i, s) in d.series.iter().enumerate() {
        let j = old.series.iter().position(|o| o == s).expect("same series");
        assert_eq!(d.values[i], old.values[j], "series {s}");
    }
    // a Phase 7 saved chart (JSON without the fields) loads with None and is drawn exactly as before
    let mut v = serde_json::to_value(&phase7).expect("json");
    v.as_object_mut().expect("object").remove("series_order");
    v.as_object_mut().expect("object").remove("facet_order");
    let loaded: ChartSpec = serde_json::from_value(v).expect("a Phase 7 chart loads");
    assert_eq!((loaded.series_order.clone(), loaded.facet_order.clone()), (None, None));
    let again = micro_parquet::charts::calc::compute(df.clone().lazy(), &loaded).expect("the loaded chart");
    assert_eq!(again.series, by_size);
    assert_ne!(phase7.calc_key(), new.calc_key(), "the order is in the calculation key");
    // a text series in the Calendar order (the quarter of the date)
    let q = DateFieldRef::new("Open Date", DatePart::Quarter, CalendarSettings::default());
    let cal = ChartSpec { series: Some(q.name.clone()), series_part: Some(q.clone()), series_order: q.sort_order(), ..base() };
    let dq = micro_parquet::charts::calc::compute(df.lazy(), &cal).expect("quarters");
    let named: Vec<&str> = dq.series.iter().map(|s| s.as_str()).filter(|s| s.starts_with('Q')).collect();
    assert_eq!(named, ["Q1", "Q2", "Q3", "Q4"], "{:?}", dq.series);
}

#[test]
fn q1_facet_panels_follow_the_facet_order_and_without_one_the_largest_first() {
    let df = date_tab();
    let q = DateFieldRef::new("Open Date", DatePart::DayName, CalendarSettings::default());
    let plain = ChartSpec { facet: Some(q.name.clone()), facet_part: Some(q.clone()), ..base() };
    let before = micro_parquet::charts::calc::compute(df.clone().lazy(), &plain).expect("facets by size");
    let sized: Vec<String> = before.facets.iter().map(|f| f.name.clone()).collect();
    let week = ["Sat", "Sun", "Mon", "Tue", "Wed", "Thu", "Fri"]; // the default calendar starts the week on Saturday
    let in_week = |names: &[String]| names.iter().filter(|n| week.contains(&n.as_str())).cloned().collect::<Vec<_>>();
    assert_ne!(in_week(&sized), week, "size order is not week order: {sized:?}");
    let ordered = ChartSpec { facet_order: q.sort_order(), ..plain.clone() };
    let d = micro_parquet::charts::calc::compute(df.lazy(), &ordered).expect("facets in Calendar order");
    let names: Vec<String> = d.facets.iter().map(|f| f.name.clone()).collect();
    assert_eq!(in_week(&names), week, "{names:?}");
    assert_eq!(names.last().map(|s| s.as_str()), Some("(blank)"), "blank last: {names:?}");
    assert_eq!(d.facets.len(), before.facets.len());
}

/// Every label the Q2 test puts to both the app and the Python mirror: each unit word the app reads (and the words of
/// the Ranges grammar and some near misses, which neither may read) in four spellings, and the odd numbers.
fn q2_labels() -> Vec<String> {
    let units = [
        "d", "day", "days", "w", "wk", "wks", "week", "weeks", "m", "mo", "mos", "mon", "mons", "month", "months", "y", "yr", "yrs", "year", "years",
        // near misses and the Ranges grammar's words: not tenor units, so both must refuse them
        "min", "mins", "mth", "mths", "hr", "h", "q", "qtr", "fortnight", "fortnights", "k", "cr", "crore", "lakh", "bn", "%", "dd", "ww", "yy", "mm", "monthly", "",
    ];
    let mut out: Vec<String> = Vec::new();
    for n in ["3", "1.5", "0", "12", "100"] {
        for u in units {
            out.push(format!("{n}{u}"));
            out.push(format!("{n} {u}"));
            out.push(format!("  {n}  {}  ", u.to_uppercase()));
            out.push(format!("{n}\u{a0}{u}"));
        }
    }
    for t in ["12M", "1Y", "18M", "2Y", "30D", "1M", "6 months", "3 mo", "2 mos", "1.5y", "2 weeks", "O/N", "ON", "o/n", "Overnight", "7 wks", "3 Mon", "5 mons", "abc", "12", "3 fortnights", ""] {
        out.push(t.to_string());
    }
    // numbers that are not whole tenors, or too big to hold
    let huge = "9".repeat(400);
    for t in [format!("{huge}d"), format!("{huge}m"), ".5d".into(), "5.d".into(), ".d".into(), "..d".into(), "1.2.3d".into(), "1e5d".into(), "-3m".into(), "+3m".into(), "m3".into(), "3 m m".into(), "3 2m".into(), "1,5m".into(), "\u{0663}m".into()] {
        out.push(t);
    }
    out
}

#[test]
fn q2_the_python_tenor_day_count_equals_the_apps() {
    if !have_python() {
        eprintln!("q2: python3 is not installed here, so the Python mirror is not run (skipped)");
        return;
    }
    let labels = q2_labels();
    let d = dir("q2_py");
    let mut script = String::from("import re as _re\nimport json\n");
    script.push_str(micro_parquet::macros::PY_ORDER_HELPER);
    script.push_str(&format!("print(json.dumps([_mp_tenor_days(t) for t in {}]))\n", serde_json::to_string(&labels).expect("json")));
    let out = run_python(&d, "q2", &script);
    let py: Vec<Option<f64>> = serde_json::from_str(out.trim()).expect("python json");
    assert_eq!(py.len(), labels.len());
    let (mut read, mut refused) = (0, 0);
    for (t, p) in labels.iter().zip(py.iter()) {
        let a = parse_tenor_days(t);
        match (a, p) {
            (Some(a), Some(p)) => {
                read += 1;
                assert!((a - p).abs() <= 1e-9 * a.abs().max(1.0), "'{t}': app {a} vs Python {p}");
            }
            (None, None) => refused += 1,
            _ => panic!("'{t}': the app gives {a:?} and the Python mirror {p:?}: they must accept exactly the same labels"),
        }
    }
    // the test reaches both sides: the app reads many labels and refuses many
    assert!(read > 400 && refused > 200, "read {read}, refused {refused}");
    // the units of the owner's question are read by both ("mo" and "mos" included)
    for t in ["3 mo", "2 mos", "3mo", "  3  MOS  "] {
        let i = labels.iter().position(|l| l == t).unwrap_or_else(|| panic!("'{t}' is in the list"));
        assert!(parse_tenor_days(t).is_some() && py[i].is_some(), "'{t}' is read by the app and the mirror");
    }
    // 12M and 1Y tie, as in the app
    let at = |t: &str| py[labels.iter().position(|l| l == t).expect("in the list")];
    assert_eq!(at("12M"), at("1Y"));
}
