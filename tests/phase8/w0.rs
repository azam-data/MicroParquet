//! W0: the calendar core (`src/dates.rs`), the Calendar and Ranges orders, their Python mirrors,
//! `engine::column_orders`, the settings, the version housekeeping and the Phase 7 replay of goal 3.

use crate::common::{dir, replay_to_csv, run_steps, strings};
use crate::common8::*;
use crate::p7_baseline;
use micro_parquet::dates::*;
use micro_parquet::models::*;
use micro_parquet::sort_lists::{detect_order, validate_list, SortList, SortOrder};
use polars::prelude::*;
use std::collections::BTreeMap;
use std::path::Path;

fn sat() -> CalendarSettings {
    CalendarSettings::default()
}

fn with(week_start: Weekday, fy: u32) -> CalendarSettings {
    CalendarSettings { fy_start_month: fy, week_start }
}

fn text(p: DatePart, d: chrono::NaiveDate, cal: &CalendarSettings) -> String {
    p.value(d, cal).to_string()
}

#[test]
fn w0_the_defaults_are_january_and_saturday_and_old_settings_load_them() {
    let c = CalendarSettings::default();
    assert_eq!((c.fy_start_month, c.week_start), (1, Weekday::Sat));
    assert_eq!(c.status_line(), "Week starts Saturday \u{b7} Fiscal year from January");
    assert_eq!(with(Weekday::Mon, 7).status_line(), "Week starts Monday \u{b7} Fiscal year from July");
    // a Phase 7 settings.json has no calendar: it loads with the defaults; a saved one round-trips
    let d = dir("w0_settings");
    std::fs::write(d.join("settings.json"), r#"{"right_panel_width": 420.0, "chart_tabs": []}"#).expect("old settings");
    let (s, msg) = micro_parquet::settings::Settings::load(&d);
    assert!(msg.is_none(), "{msg:?}");
    assert_eq!(s.calendar, CalendarSettings::default());
    let mut s = s;
    s.calendar = with(Weekday::Sun, 7);
    s.save(&d).expect("saves");
    assert_eq!(micro_parquet::settings::Settings::load(&d).0.calendar, with(Weekday::Sun, 7));
    // a hand-edited month outside 1–12 is refused with a plain message, never clamped
    let bad: CalendarSettings = serde_json::from_str(r#"{"fy_start_month": 13}"#).expect("parses");
    assert!(bad.validate().expect_err("refused").to_string().contains("1 (January) to 12"));
    // a missing field takes its default
    let half: CalendarSettings = serde_json::from_str(r#"{"week_start": "Mon"}"#).expect("parses");
    assert_eq!(half, with(Weekday::Mon, 1));
}

#[test]
fn w0_the_week_rules_on_known_dates_with_saturday_and_monday_starts() {
    let d = ymd(2026, 1, 15); // a Thursday
    assert_eq!(text(DatePart::WeekStarting, d, &sat()), "2026-01-10");
    assert_eq!(text(DatePart::DayName, d, &sat()), "Thu");
    assert_eq!(text(DatePart::WeekOfYear, d, &sat()), "W03");
    let mon = with(Weekday::Mon, 1);
    assert_eq!(text(DatePart::WeekStarting, d, &mon), "2026-01-12");
    assert_eq!(text(DatePart::WeekOfYear, d, &mon), "W03");
    // 1 January is in W01 for every start day, in every year (its week may start in December)
    for y in 2020..=2032 {
        for ws in Weekday::ALL {
            let c = with(ws, 1);
            let jan1 = ymd(y, 1, 1);
            assert_eq!(text(DatePart::WeekOfYear, jan1, &c), "W01", "{y} {ws:?}");
            let start = c.week_starting(jan1);
            assert!(start <= jan1 && (jan1 - start).num_days() < 7, "{y} {ws:?}: the week starting is on or before the date");
            assert_eq!(Weekday::from_chrono(chrono::Datelike::weekday(&start)), ws, "{y} {ws:?}: the week starts on its start day");
        }
    }
    // the week of the date before a week start is the week before
    assert_eq!(text(DatePart::WeekOfYear, ymd(2026, 1, 9), &sat()), "W02");
    assert_eq!(text(DatePart::WeekStarting, ymd(2025, 12, 31), &sat()), "2025-12-27");
}

#[test]
fn w0_fiscal_labels_with_july_and_january_starts() {
    let d = ymd(2026, 1, 15);
    let jul = with(Weekday::Sat, 7);
    assert_eq!(text(DatePart::FiscalYear, d, &jul), "FY2025-26");
    assert_eq!(text(DatePart::FiscalQuarter, d, &jul), "FQ3");
    assert_eq!(DatePart::FiscalMonth.value(d, &jul), PartValue::Int(7));
    assert_eq!(text(DatePart::FiscalYear, d, &sat()), "FY2026");
    assert_eq!(text(DatePart::FiscalQuarter, d, &sat()), "FQ1");
    assert_eq!(DatePart::FiscalMonth.value(d, &sat()), PartValue::Int(1));
    // the first day of a July fiscal year
    assert_eq!(text(DatePart::FiscalYear, ymd(2026, 7, 1), &jul), "FY2026-27");
    assert_eq!(text(DatePart::FiscalYear, ymd(2099, 7, 1), &jul), "FY2099-00");
    // the other parts on one date
    let a = ymd(2026, 8, 1);
    let got: Vec<String> = DatePart::ALL.iter().map(|p| text(*p, a, &sat())).collect();
    assert_eq!(got, ["2026", "Q3", "2026 Q3", "Aug", "8", "Aug 2026", "W32", "2026-08-01", "Sat", "1", "2026-08-01", "FY2026", "FQ3", "8"]);
    // types and orders of the table
    use micro_parquet::dates::PartOrder::*;
    let orders: Vec<PartOrder> = DatePart::ALL.iter().map(|p| p.part_order()).collect();
    assert_eq!(orders, [Numeric, Calendar, Calendar, Calendar, Numeric, Calendar, Calendar, Date, Calendar, Numeric, Date, Calendar, Calendar, Numeric]);
}

#[test]
fn w0_every_part_as_an_expression_equals_the_part_of_each_date_and_a_blank_stays_blank() {
    let df = date_tab();
    let cal = with(Weekday::Mon, 7);
    let exprs: Vec<Expr> = DatePart::ALL.iter().map(|p| p.expr("Open Date", cal).alias(p.label())).collect();
    let dt_exprs: Vec<Expr> = DatePart::ALL.iter().map(|p| p.expr("Open Time", cal).alias(format!("t {}", p.label()))).collect();
    let out = df.clone().lazy().select(exprs.into_iter().chain(dt_exprs).collect::<Vec<_>>()).collect().expect("the parts");
    let dates: Vec<Option<chrono::NaiveDate>> = df.column("Open Date").expect("dates").cast(&DataType::Int32).expect("days").i32().expect("i32").into_iter().map(|v| v.and_then(date_from_days)).collect();
    for p in DatePart::ALL {
        let col = out.column(p.label()).expect("part column");
        assert_eq!(col.dtype(), &p.dtype(), "{p:?}");
        let shown = |c: &Series, i: usize| -> Option<String> {
            match c.get(i).expect("row") {
                AnyValue::Null => None,
                AnyValue::Date(d) => date_from_days(d).map(|d| d.format("%Y-%m-%d").to_string()),
                AnyValue::String(s) => Some(s.to_string()),
                AnyValue::StringOwned(s) => Some(s.to_string()),
                v => Some(v.to_string()),
            }
        };
        let tcol = out.column(&format!("t {}", p.label())).expect("datetime part");
        for (i, d) in dates.iter().enumerate() {
            let want = d.map(|d| p.value(d, &cal).to_string());
            assert_eq!(shown(col, i), want, "{p:?} row {i}");
            assert_eq!(shown(tcol, i), want, "{p:?} row {i}: a Datetime gives the parts of its date");
        }
        assert_eq!(col.null_count(), 3, "{p:?}: the 3 blank dates give 3 blank parts");
    }
}

#[test]
fn w0_a_text_or_number_column_is_refused_with_the_message() {
    let e = check_date_column("Open Date", &DataType::String).expect_err("text is refused");
    assert_eq!(e.to_string(), "'Open Date' is text. Set it to Date in Schema & Ingestion (D/M/Y) first.");
    assert!(check_date_column("Balance", &DataType::Float64).is_err());
    assert!(check_date_column("Open Date", &DataType::Date).is_ok());
    assert!(check_date_column("Open Time", &DataType::Datetime(TimeUnit::Milliseconds, None)).is_ok());
}

fn sorted(order: &SortOrder, labels: &[&str]) -> Vec<String> {
    let mut v: Vec<Option<String>> = labels.iter().map(|s| Some(s.to_string())).collect();
    v.sort_by(|a, b| order.compare(a, b));
    v.into_iter().flatten().collect()
}

const RANGES1: [&str; 6] = ["10%\u{2013}11%", "No rate", "0%", "8%\u{2013}9%", "> 15%", "Below 0%"];
const RANGES1_SORTED: [&str; 6] = ["Below 0%", "0%", "8%\u{2013}9%", "10%\u{2013}11%", "> 15%", "No rate"];
const RANGES2: [&str; 5] = ["Below 1 Crore", "25 Crore to 50 Crore", "1 Crore to 5 Crore", "Above 10 Crore", "5 Crore to 10 Crore"];
const RANGES2_SORTED: [&str; 5] = ["Below 1 Crore", "1 Crore to 5 Crore", "5 Crore to 10 Crore", "Above 10 Crore", "25 Crore to 50 Crore"];

#[test]
fn w0_calendar_and_ranges_sort_the_acceptance_lists() {
    let cal = SortOrder::Calendar { week_start: Weekday::Sat };
    assert_eq!(sorted(&cal, &["Mar", "Jan", "Feb"]), ["Jan", "Feb", "Mar"]);
    assert_eq!(sorted(&cal, &["Mon", "Sat", "Fri"]), ["Sat", "Mon", "Fri"]);
    assert_eq!(sorted(&SortOrder::Calendar { week_start: Weekday::Mon }, &["Mon", "Sat", "Fri"]), ["Mon", "Fri", "Sat"]);
    assert_eq!(sorted(&cal, &["december", "MARCH", "Sept", "January"]), ["January", "MARCH", "Sept", "december"]);
    assert_eq!(sorted(&cal, &["Q3", "Q1", "Q4", "Q2"]), ["Q1", "Q2", "Q3", "Q4"]);
    assert_eq!(sorted(&cal, &["FQ2", "FQ10", "FQ1"]), ["FQ1", "FQ2", "FQ10"], "FQ10 is not a fiscal quarter: after them");
    assert_eq!(sorted(&cal, &["W10", "W02", "W53", "W01"]), ["W01", "W02", "W10", "W53"]);
    assert_eq!(sorted(&cal, &["Jan 2027", "Aug 2026", "Dec 2026", "Feb 2026"]), ["Feb 2026", "Aug 2026", "Dec 2026", "Jan 2027"]);
    assert_eq!(sorted(&cal, &["2027 Q1", "2026 Q4", "2026 Q1"]), ["2026 Q1", "2026 Q4", "2027 Q1"]);
    assert_eq!(sorted(&cal, &["FY2026-27", "FY2024-25", "FY2025-26"]), ["FY2024-25", "FY2025-26", "FY2026-27"]);
    assert_eq!(sorted(&cal, &["FY2027", "FY2026"]), ["FY2026", "FY2027"]);
    assert_eq!(sorted(&SortOrder::Ranges, &RANGES1), RANGES1_SORTED);
    assert_eq!(sorted(&SortOrder::Ranges, &RANGES2), RANGES2_SORTED);
    assert_eq!(sorted(&SortOrder::Ranges, &["1 year to 3 years", "Up to 3 months", "3 months to 1 year", "Over 5 years", "No maturity"]), ["Up to 3 months", "3 months to 1 year", "1 year to 3 years", "Over 5 years", "No maturity"]);
    assert_eq!(sorted(&SortOrder::Ranges, &["5 Lakh to 1 Crore", "Up to 5 Lakh", "1 Crore and above"]), ["Up to 5 Lakh", "5 Lakh to 1 Crore", "1 Crore and above"]);
    assert_eq!(sorted(&SortOrder::Ranges, &["\u{2212}2% to 0%", "Below \u{2212}2%", "0% to 2%"]), ["Below \u{2212}2%", "\u{2212}2% to 0%", "0% to 2%"]);
    // the sort step orders the same way (blank labels last)
    let df = DataFrame::new(vec![Series::new("Band".into(), RANGES1.iter().map(|s| Some(s.to_string())).chain([None, Some(" ".into())]).collect::<Vec<_>>()).into()]).expect("frame");
    let out = run_steps("w0_sort", &df, &[TransformStep::SortRows { levels: vec![SortLevel { column: "Band".into(), descending: false, order: SortOrder::Ranges }] }]).expect("sorts");
    let got: Vec<String> = strings(&out, "Band").into_iter().map(|v| v.unwrap_or_else(|| "(null)".into())).collect();
    assert_eq!(&got[..6], RANGES1_SORTED);
    // highest first: ranked ones reversed, the unranked and the blanks still after them
    let out = run_steps("w0_sort_desc", &df, &[TransformStep::SortRows { levels: vec![SortLevel { column: "Band".into(), descending: true, order: SortOrder::Ranges }] }]).expect("sorts");
    let got: Vec<String> = strings(&out, "Band").into_iter().map(|v| v.unwrap_or_else(|| "(null)".into())).collect();
    assert_eq!(&got[..6], ["> 15%", "10%\u{2013}11%", "8%\u{2013}9%", "0%", "Below 0%", "No rate"]);
    let days = DataFrame::new(vec![Series::new("Day".into(), ["Fri", "Mon", "Sat", "Sun"]).into()]).expect("frame");
    let out = run_steps("w0_sort_cal", &days, &[TransformStep::SortRows { levels: vec![SortLevel { column: "Day".into(), descending: false, order: SortOrder::Calendar { week_start: Weekday::Sat } }] }]).expect("sorts");
    assert_eq!(strings(&out, "Day").into_iter().flatten().collect::<Vec<_>>(), ["Sat", "Sun", "Mon", "Fri"]);
}

#[test]
fn w0_calendar_and_ranges_are_reserved_names_and_serialise_as_orders() {
    for name in ["Calendar", "ranges", " RANGES "] {
        let l = SortList { name: name.into(), values: vec!["a".into()], case_insensitive: true, apply_to_columns: vec![] };
        assert!(validate_list(&l).expect("refused").contains("built-in"), "{name}");
    }
    let j = serde_json::to_string(&SortOrder::Calendar { week_start: Weekday::Mon }).expect("json");
    assert_eq!(j, r#"{"Calendar":{"week_start":"Mon"}}"#);
    assert_eq!(serde_json::from_str::<SortOrder>(r#"{"Calendar":{}}"#).expect("default week start"), SortOrder::Calendar { week_start: Weekday::Sat });
    assert_eq!(serde_json::from_str::<SortOrder>(r#""Ranges""#).expect("ranges"), SortOrder::Ranges);
    // Natural is unchanged: a month column without a chosen order still sorts as text
    assert_eq!(sorted(&SortOrder::Natural, &["Mar", "Jan", "Feb"]), ["Feb", "Jan", "Mar"]);
}

#[test]
fn w0_auto_order_picks_calendar_then_ranges_then_nothing() {
    let cal = sat();
    assert_eq!(detect_order(["Jan", "Mar", "", "Feb"], &cal), Some(SortOrder::Calendar { week_start: Weekday::Sat }));
    assert_eq!(detect_order(["Aug 2026", "Sep 2026"], &with(Weekday::Mon, 1)), Some(SortOrder::Calendar { week_start: Weekday::Mon }));
    assert_eq!(detect_order(RANGES1, &cal), Some(SortOrder::Ranges));
    assert_eq!(detect_order(RANGES2, &cal), Some(SortOrder::Ranges));
    assert_eq!(detect_order(["Jan", "Dhaka"], &cal), None, "one label that does not parse: today's default");
    assert_eq!(detect_order(["1", "2", "10"], &cal), None, "plain numbers alone are not ranges");
    assert_eq!(detect_order(["No rate"], &cal), None, "no real range");
    assert_eq!(detect_order(["", " "], &cal), None);
    assert_eq!(detect_order(["Overdraft", "Over 5 years"], &cal), None, "'Overdraft' is not 'Over' a number");
}

#[test]
fn w0_column_orders_is_empty_for_steps_that_declare_none() {
    let steps = vec![TransformStep::RenameColumn { old_name: "a".into(), new_name: "b".into() }, TransformStep::SortRows { levels: vec![] }];
    assert!(micro_parquet::engine::column_orders(&steps).is_empty());
}

/// Rust's and Python's rank of the same labels.
fn python_ranks(d: &Path, labels: &[String]) -> Vec<String> {
    let mut script = String::from(micro_parquet::macros::PY_CALENDAR_RANGES_HELPER);
    script.push_str(&format!("import json\nLABELS = json.loads({:?})\n", serde_json::to_string(labels).expect("json")));
    script.push_str("for v in LABELS:\n    print(json.dumps([_mp_calendar_rank(v, 5), _mp_calendar_rank(v, 0), _mp_range_key(v)]))\n");
    run_python(d, "ranks", &script).lines().map(str::to_string).collect()
}

fn rust_ranks(labels: &[String]) -> Vec<String> {
    let f = |x: Option<f64>| x.map(|v| serde_json::json!(v)).unwrap_or(serde_json::Value::Null);
    labels
        .iter()
        .map(|v| {
            let r = parse_range(v).map(|k| serde_json::json!([k.value, k.kind as f64])).unwrap_or(serde_json::Value::Null);
            serde_json::to_string(&serde_json::json!([f(calendar_rank(v, Weekday::Sat)), f(calendar_rank(v, Weekday::Mon)), r])).expect("json")
        })
        .collect()
}

#[test]
fn w0_the_python_mirrors_rank_every_label_as_the_app_does() {
    if !have_python() {
        eprintln!("w0: python3 is not installed here, so the Python mirror is not run (skipped)");
        return;
    }
    let mut labels: Vec<String> = RANGES1.iter().chain(RANGES2.iter()).map(|s| s.to_string()).collect();
    for s in [
        "Jan", "january", "SEPT", "Sat", "saturday", "Fri", "Q1", "Q5", "q4", "FQ3", "W01", "W54", "W55", "Aug 2026", "2026 Q1", "FY2026", "FY2026-27", "FY2026-28", "FY 2026/2027",
        "Dhaka", "", "  ", "Up to 3 months", "Over 5 years", "Overdraft", "1 year to 3 years", "5 Lakh to 1 Crore", "1 Crore and above", "15%+", "\u{2212}2% to 0%", "Below \u{2212}2%",
        "1,000 to 5,000", "1.5 Cr", "3 to 5 Crore", "≥ 10%", "<= 5%", "5", "abc 5", "100D", "No maturity", "8%-9%", "8% - 9%", "1 to", "10 days to 30 days", "Under 1 K",
    ] {
        labels.push(s.to_string());
    }
    let d = dir("w0_py");
    let py = python_ranks(&d, &labels);
    let rs = rust_ranks(&labels);
    assert_eq!(py.len(), rs.len());
    for (i, l) in labels.iter().enumerate() {
        // Python's json writes 1.0 where Rust writes 1.0 too; compare the parsed values
        let a: serde_json::Value = serde_json::from_str(&py[i]).expect("python json");
        let b: serde_json::Value = serde_json::from_str(&rs[i]).expect("rust json");
        assert_eq!(a, b, "'{l}': Python {} vs Rust {}", py[i], rs[i]);
    }
    // the acceptance orders, sorted in Python the way the generated sort step does (rank, then sub-key)
    let mut script = String::from(micro_parquet::macros::PY_CALENDAR_RANGES_HELPER);
    script.push_str(&format!(
        "import json\nfor labels in json.loads({:?}):\n    k = [(l, _mp_range_key(l)) for l in labels]\n    ranked = sorted([x for x in k if x[1] is not None], key=lambda x: x[1])\n    rest = sorted([x for x in k if x[1] is None], key=lambda x: x[0].lower())\n    print(json.dumps([x[0] for x in ranked + rest]))\n",
        serde_json::to_string(&vec![RANGES1.to_vec(), RANGES2.to_vec()]).expect("json")
    ));
    script.push_str("print(json.dumps(sorted(['Mon', 'Sat', 'Fri'], key=lambda v: _mp_calendar_rank(v, 5))))\nprint(json.dumps(sorted(['Mar', 'Jan', 'Feb'], key=lambda v: _mp_calendar_rank(v, 5))))\n");
    let out = run_python(&d, "orders", &script);
    let lines: Vec<Vec<String>> = out.lines().map(|l| serde_json::from_str(l).expect("a list")).collect();
    assert_eq!(lines[0], RANGES1_SORTED);
    assert_eq!(lines[1], RANGES2_SORTED);
    assert_eq!(lines[2], ["Sat", "Mon", "Fri"]);
    assert_eq!(lines[3], ["Jan", "Feb", "Mar"]);
}

#[test]
fn w0_the_python_export_of_a_calendar_and_a_ranges_sort_uses_the_mirrors() {
    let steps = vec![TransformStep::SortRows {
        levels: vec![
            SortLevel { column: "Month".into(), descending: false, order: SortOrder::Calendar { week_start: Weekday::Mon } },
            SortLevel { column: "Band".into(), descending: true, order: SortOrder::Ranges },
        ],
    }];
    let py = micro_parquet::macros::generate_python_polars_code(Path::new("in.parquet"), true, &IngestionConfig::default(), &steps, None).expect("python");
    assert!(py.contains("def _mp_calendar_rank") && py.contains("def _mp_range_key"), "the helpers are written once");
    assert!(py.contains("lambda v: _mp_calendar_rank(v, 0)"), "Monday is 0 days after Monday");
    assert!(py.contains("*_mp_rank_keys(pl.col(\"Band\"), _mp_range_rank, _mp_range_sub)"));
    // Calendar: group, rank, natural; Ranges: group, rank, sub-key, natural
    assert!(py.contains("descending=[False, False, False, False, True, True, False]"), "{py}");
    if have_python() {
        let d = dir("w0_py_parse");
        let path = d.join("x.py");
        std::fs::write(&path, &py).expect("script");
        let o = std::process::Command::new("python3").arg("-I").arg("-c").arg("import ast,sys; ast.parse(open(sys.argv[1], encoding='utf-8').read())").arg(&path).output().expect("python3");
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }
}

#[test]
fn w0_the_version_is_1_8_0_and_a_tag_must_match_it() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let toml = std::fs::read(root.join("Cargo.toml")).expect("Cargo.toml");
    let text = String::from_utf8_lossy(&toml);
    assert!(text.contains("version = \"1.14.0\"\r\n"), "Cargo.toml is at 1.14.0 with its CRLF line ends");
    assert!(!text.replace("\r\n", "").contains('\n'), "every line of Cargo.toml ends in CRLF");
    assert_eq!(env!("CARGO_PKG_VERSION"), "1.14.0");
    let wf = std::fs::read_to_string(root.join(".github/workflows/main.yml")).expect("workflow");
    let test_job = wf.split("\n  test:").nth(1).and_then(|s| s.split("\n  build:").next()).expect("the test job");
    let check = test_job.find("Check the tag matches Cargo.toml").expect("the check step");
    assert!(check < test_job.find("run: cargo test").expect("cargo test"), "the check runs before the tests");
    assert!(test_job.contains("Tag $TAG does not match Cargo.toml version $VERSION"));
    assert!(test_job.contains("if: github.event_name == 'push'"), "only the tag-push path runs it; a manual run skips the test job");
}

/// Goal 3: every Phase 7 sample macro, the pack and the saved chart load with today's code and give the
/// same rows in the same order (the hashes the Phase 7 code gave, `tests/fixtures/phase7_replay/`).
#[test]
fn w0_the_phase7_samples_replay_to_the_hashes_the_phase7_code_gave() {
    let fx = crate::common::fixtures_dir();
    let base = fx.join("phase7_replay");
    let want: BTreeMap<String, String> = serde_json::from_str(&std::fs::read_to_string(base.join("phase7_replay.hashes.json")).expect("hashes")).expect("json");
    assert_eq!(want.len(), 14, "10 macros, 3 pack sheets and a chart");
    let d = dir("w0_replay");
    let mut got: BTreeMap<String, String> = BTreeMap::new();
    for key in want.keys().filter(|k| k.starts_with("macro:")) {
        let stem = &key["macro:".len()..];
        let file = if base.join(format!("{stem}.mpq.json")).exists() { base.join(format!("{stem}.mpq.json")) } else { fx.join(format!("{stem}.mpq.json")) };
        let r = micro_parquet::macros::load_macro_from_file(&file).unwrap_or_else(|e| panic!("{stem}: {e:#}"));
        got.insert(key.clone(), p7_baseline::hash(&replay_to_csv(&r.steps, &d)));
    }
    let pack = micro_parquet::packs::load_pack(&base, "p7replay").expect("the Phase 7 pack loads");
    let rig = p7_baseline::rig(&std::env::temp_dir().join(format!("w0_replay_pack_{}", std::process::id())));
    for (sheet, h) in p7_baseline::pack_hashes(&pack, &rig) {
        got.insert(format!("pack:{sheet}"), h);
    }
    let (name, spec) = micro_parquet::charts::library::load_chart(&base.join("charts").join("Deposits by product.json")).expect("the Phase 7 chart loads");
    got.insert(format!("chart:{name}"), p7_baseline::chart_hash(&spec, &rig));
    assert_eq!(got, want);
}
