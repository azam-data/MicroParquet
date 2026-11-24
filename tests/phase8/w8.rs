//! W8: the guide of Phase 8. The six new notes have the eight sections of `<guide_notes>` in order, every table of an
//! "Example" is the table the code gives (and names a test that exists), every picture a note names exists, the README
//! lists the notes, and the notes that W3 and W9 to W11 changed say what the code now does. The pictures themselves are
//! written by the render tests (`render_tests_p8`, "W8 screenshots") and by `w8_the_chart_pdf_picture_…` below.

use micro_parquet::charts::calc::compute;
use micro_parquet::charts::colours::{pick_warnings, Colour, ColourPick};
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec, DrillLevel};
use micro_parquet::dates::{calendar_rank, CalendarSettings, DatePart, Weekday};
use micro_parquet::models::format_with_commas;
use micro_parquet::sort_lists::SortOrder;
use polars::prelude::*;
use std::path::PathBuf;

fn guide() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/guide")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(guide().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

pub const NEW_NOTES: [&str; 6] = ["split-date.md", "date-grouping.md", "calendar-settings.md", "projects.md", "drill-down.md", "colours.md"];

const SECTIONS: [&str; 8] = ["## 1. What it does", "## 2. When to use it", "## 3. How to use it", "## 4. Example", "## 5. What it refuses and why", "## 6. Python export", "## 7. Saved in", "## 8. Related tools"];

// ------------------------------------------------------------------------------------------ the tables of the examples

fn md(head: &[&str], rows: &[Vec<String>], right: &[usize]) -> String {
    let rule: Vec<&str> = (0..head.len()).map(|i| if right.contains(&i) { "---:" } else { "---" }).collect();
    let mut out = format!("| {} |\n| {} |", head.join(" | "), rule.join(" | "));
    for r in rows {
        out.push_str(&format!("\n| {} |", r.join(" | ")));
    }
    out
}

fn date(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(y, m, d).expect("a date")
}

fn part_cells(d: Option<chrono::NaiveDate>, parts: &[DatePart], cal: &CalendarSettings) -> Vec<String> {
    parts.iter().map(|p| d.map(|d| p.value(d, cal).to_string()).unwrap_or_else(|| "(blank)".to_string())).collect()
}

/// Split date: three dates, the five default parts and Week starting and Week of year (Saturday start, January fiscal year).
fn split_date_table() -> String {
    let cal = CalendarSettings::default();
    let parts = [DatePart::Year, DatePart::Quarter, DatePart::Month, DatePart::MonthYear, DatePart::DayName, DatePart::WeekStarting, DatePart::WeekOfYear];
    let head: Vec<String> = std::iter::once("Open Date".to_string()).chain(parts.iter().map(|p| p.label().to_string())).collect();
    let rows: Vec<Vec<String>> = [Some(date(2026, 1, 15)), Some(date(2026, 1, 1)), None]
        .iter()
        .map(|d| std::iter::once(d.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_else(|| "(blank)".to_string())).chain(part_cells(*d, &parts, &cal)).collect())
        .collect();
    md(&head.iter().map(String::as_str).collect::<Vec<_>>(), &rows, &[])
}

/// The same 15 Jan 2026 under four calendars.
fn calendar_table() -> String {
    let d = date(2026, 1, 15);
    let parts = [DatePart::WeekStarting, DatePart::WeekOfYear, DatePart::FiscalYear, DatePart::FiscalQuarter, DatePart::FiscalMonth];
    let rows: Vec<Vec<String>> = [(Weekday::Sat, 1), (Weekday::Mon, 1), (Weekday::Sat, 7), (Weekday::Sun, 4)]
        .iter()
        .map(|(w, m)| {
            let cal = CalendarSettings { fy_start_month: *m, week_start: *w };
            let mut r = vec![format!("{} / {}", w.long(), micro_parquet::dates::MONTHS_LONG[*m as usize - 1])];
            r.extend(part_cells(Some(d), &parts, &cal));
            r
        })
        .collect();
    let head: Vec<String> = std::iter::once("Week starts / fiscal year from".to_string()).chain(parts.iter().map(|p| p.label().to_string())).collect();
    md(&head.iter().map(String::as_str).collect::<Vec<_>>(), &rows, &[])
}

fn order_of(order: &SortOrder, labels: &[&str]) -> Vec<String> {
    let mut ranked: Vec<(&str, (f64, f64))> = labels.iter().filter_map(|l| order.rank_key(l).map(|k| (*l, k))).collect();
    ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).expect("numbers"));
    let mut rest: Vec<&str> = labels.iter().filter(|l| order.rank_key(l).is_none()).cloned().collect();
    rest.sort();
    ranked.into_iter().map(|(l, _)| l.to_string()).chain(rest.into_iter().map(String::from)).collect()
}

/// The Calendar order: months, and days from a Saturday start.
fn calendar_order_table() -> String {
    let sat = SortOrder::Calendar { week_start: Weekday::Sat };
    let rows = vec![
        vec!["Mar, Jan, Feb".to_string(), order_of(&sat, &["Mar", "Jan", "Feb"]).join(", ")],
        vec!["Mon, Sat, Fri".to_string(), order_of(&sat, &["Mon", "Sat", "Fri"]).join(", ")],
        vec!["Mon, Sat, Fri".to_string(), order_of(&SortOrder::Calendar { week_start: Weekday::Mon }, &["Mon", "Sat", "Fri"]).join(", ")],
    ];
    assert!(calendar_rank("Sat", Weekday::Sat) < calendar_rank("Mon", Weekday::Sat));
    md(&["Labels", "In Calendar order"], &rows, &[])
}

/// Six month-end balances of three branches' fixed deposits and a blank date, by Month-Year, and the stock trap.
fn grouping_rows() -> Vec<(Option<chrono::NaiveDate>, f64)> {
    vec![
        (Some(date(2026, 1, 31)), 100.0),
        (Some(date(2026, 2, 28)), 100.0),
        (Some(date(2026, 3, 31)), 100.0),
        (Some(date(2026, 1, 31)), 250.0),
        (Some(date(2026, 3, 31)), 250.0),
        (None, 40.0),
    ]
}

fn grouping_table() -> String {
    let cal = CalendarSettings::default();
    let rows = grouping_rows();
    let mut by: Vec<(i64, String, f64, usize)> = Vec::new();
    for (d, v) in &rows {
        let (key, label) = match d {
            Some(d) => (DatePart::MonthYear.order_key(*d, &cal).expect("key"), DatePart::MonthYear.value(*d, &cal).to_string()),
            None => (i64::MAX, "(blank)".to_string()),
        };
        match by.iter_mut().find(|x| x.0 == key) {
            Some(x) => {
                x.2 += v;
                x.3 += 1;
            }
            None => by.push((key, label, *v, 1)),
        }
    }
    by.sort_by_key(|x| x.0);
    let total: f64 = rows.iter().map(|(_, v)| v).sum();
    let mut out: Vec<Vec<String>> = by.iter().map(|x| vec![x.1.clone(), format_with_commas(x.2, 2), x.3.to_string()]).collect();
    assert_eq!(by.iter().map(|x| x.2).sum::<f64>(), total, "the groups add up to the ungrouped total, the blank included");
    out.push(vec!["Grand total".to_string(), format_with_commas(total, 2), rows.len().to_string()]);
    md(&["Open Date \u{b7} Month-Year", "Sum of Balance", "Rows"], &out, &[1, 2])
}

/// The deposit book of the render tests (8 rows) drilled Branch > Month > Band.
fn book() -> DataFrame {
    let days: Vec<i32> = (0..8).map(|i| 20_454 + i * 37).collect();
    DataFrame::new(vec![
        Series::new("Month".into(), &["Mar", "Jan", "Feb", "Mar", "Dec", "Jan", "Aug", "Feb"]).into(),
        Series::new("Band".into(), &["10%\u{2013}11%", "No rate", "0%", "8%\u{2013}9%", "> 15%", "Below 0%", "0%", "8%\u{2013}9%"]).into(),
        Series::new("Branch".into(), &["Dhaka", "Ctg", "Dhaka", "Sylhet", "Ctg", "Dhaka", "Sylhet", "Ctg"]).into(),
        Series::new("Open Date".into(), days).cast(&DataType::Date).expect("dates").into(),
        Series::new("Balance".into(), &[100.0f64, 200.0, 300.0, 400.0, 500.0, 600.0, 700.0, 800.0]).into(),
    ])
    .expect("book")
}

fn drill_chart(path: &[&str]) -> micro_parquet::charts::data::ChartData {
    let mut s = ChartSpec { category: "Branch".into(), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 50, order: SortOrder::Calendar { week_start: Weekday::Sat }, ..Default::default() };
    s.set_drill(vec![DrillLevel::column("Branch"), DrillLevel::column("Month"), DrillLevel::column("Band")]);
    for v in path {
        assert!(s.drill_down(v), "one level down on {v}");
    }
    // the chart's order for a level of numbers or text with no calendar: its own
    if s.category == "Branch" || s.category == "Band" {
        s.order = SortOrder::Natural;
    }
    compute(book().lazy(), &s).unwrap_or_else(|e| panic!("{e:#}"))
}

fn level_table(level: &str, d: &micro_parquet::charts::data::ChartData) -> String {
    let rows: Vec<Vec<String>> = d.categories.iter().enumerate().map(|(i, c)| vec![c.clone(), format_with_commas(d.values[0][i].unwrap_or(0.0), 2), d.rows[0][i].to_string()]).collect();
    md(&[level, "Sum of Balance", "Rows"], &rows, &[1, 2])
}

fn drill_tables() -> Vec<String> {
    vec![level_table("Branch", &drill_chart(&[])), level_table("Month", &drill_chart(&["Dhaka"])), level_table("Band", &drill_chart(&["Dhaka", "Jan"]))]
}

fn colour_table() -> String {
    use micro_parquet::charts::tokens as t;
    let rows: Vec<Vec<String>> = (0..8).map(|h| vec![t::HUE_NAMES[h].to_string(), t::hex_text(t::HUE_LIGHT[h]), format!("slot {}", h + 1), t::hex_text(t::HUE_DEEP[h])]).collect();
    md(&["Hue", "Light", "Base", "Deep"], &rows, &[])
}

fn warning_table() -> String {
    let others = vec![("Savings".to_string(), Colour::Slot(1)), ("Current".to_string(), Colour::Slot(2))];
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (label, pick) in [("Deep Blue", ColourPick::swatch(0, 2)), ("Base Orange", ColourPick::swatch(1, 1)), ("Light Amber", ColourPick::swatch(3, 0)), ("Deep Green", ColourPick::swatch(2, 2))] {
        let w = pick_warnings(&pick, &others);
        rows.push(vec![label.to_string(), if w.is_empty() { "none".to_string() } else { w.join("; ") }]);
    }
    md(&["Pick for 'TD' (other items: Savings in Orange, Current in Green)", "What the picker says"], &rows, &[])
}

fn project_table() -> String {
    let fail = vec![("Gap ladder".to_string(), "the file has no 'MATDAT' column".to_string())];
    let rows = vec![
        vec!["9".to_string(), "none".to_string(), micro_parquet::project::opened_summary(9, &[])],
        vec!["8".to_string(), "Gap ladder".to_string(), micro_parquet::project::opened_summary(8, &fail)],
    ];
    md(&["Tabs that opened", "Tabs that failed", "The line you read afterwards"], &rows, &[])
}

/// (note, the test the table is followed by, the table the code gives), in the order of the notes' Example sections.
fn examples() -> Vec<(&'static str, &'static str, String)> {
    let d = drill_tables();
    let mut v = vec![
        ("split-date.md", "w8_the_split_date_example_is_what_the_calendar_core_gives", split_date_table()),
        ("calendar-settings.md", "w8_the_calendar_examples_are_what_the_calendar_core_gives", calendar_table()),
        ("calendar-settings.md", "w8_the_calendar_examples_are_what_the_calendar_core_gives", calendar_order_table()),
        ("date-grouping.md", "w8_the_date_grouping_example_adds_up_to_the_ungrouped_total", grouping_table()),
    ];
    for t in d {
        v.push(("drill-down.md", "w8_the_drill_example_adds_up_at_every_level", t));
    }
    v.push(("colours.md", "w8_the_colour_tables_are_the_codes", colour_table()));
    v.push(("colours.md", "w8_the_colour_tables_are_the_codes", warning_table()));
    v.push(("projects.md", "w8_the_project_line_is_the_one_the_app_writes", project_table()));
    v
}

fn section(text: &str, heading: &str) -> String {
    let mut lines = text.lines();
    lines.by_ref().find(|l| l.trim_end() == heading);
    lines.take_while(|l| !l.starts_with("## ")).collect::<Vec<_>>().join("\n")
}

fn tables_of(body: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = body.lines().collect();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < lines.len() {
        if lines[i].trim_start().starts_with('|') {
            let s = i;
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                i += 1;
            }
            let follower = lines[i..].iter().find(|l| !l.trim().is_empty()).map(|l| l.to_string()).unwrap_or_default();
            out.push((lines[s..i].iter().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n"), follower));
        } else {
            i += 1;
        }
    }
    out
}

#[test]
fn w8_the_six_new_notes_have_their_eight_sections_and_their_examples_are_the_tables_the_code_gives() {
    if let Ok(dump) = std::env::var("MP_GUIDE_DUMP") {
        std::fs::create_dir_all(&dump).expect("dump folder");
        let all: Vec<String> = examples().into_iter().map(|(n, t, tab)| format!("{n} ({t})\n{tab}")).collect();
        std::fs::write(PathBuf::from(&dump).join("phase8.txt"), all.join("\n\n===\n\n")).expect("write");
        return;
    }
    let sources = all_test_sources();
    for n in NEW_NOTES {
        let text = read(n);
        assert!(text.lines().next().is_some_and(|l| l.starts_with("# ")), "{n}: a title");
        let headings: Vec<&str> = text.lines().filter(|l| l.starts_with("## ")).map(str::trim_end).collect();
        assert_eq!(headings, SECTIONS.to_vec(), "{n}: the eight sections, in order");
        for s in SECTIONS {
            assert!(!section(&text, s).trim().is_empty(), "{n}: '{s}' is empty");
        }
        let how = section(&text, "## 3. How to use it");
        assert!(how.lines().any(|l| l.starts_with("1. ")) && how.lines().any(|l| l.starts_with("2. ")), "{n}: numbered steps");
        // every table of the example is followed by a test that exists
        for (i, (_, follower)) in tables_of(&section(&text, "## 4. Example")).iter().enumerate() {
            let name = follower.split('`').nth(1).unwrap_or("");
            assert!(follower.trim_start().starts_with("Test:") && sources.contains(&format!("fn {name}(")), "{n}: table {} is followed by 'Test: `name`' of a test that exists: {follower:?}", i + 1);
        }
        let lines = text.lines().count();
        assert!(lines <= 150, "{n}: {lines} lines");
        // plain English: none of the words of the code
        for w in ["Polars", "Rust", "serde", "LazyFrame", "Expr"] {
            assert!(!text.contains(w), "{n}: '{w}' is a word of the code, not of a banker");
        }
    }
    // the tables of the examples are the ones in the notes, in order, each once
    for note in NEW_NOTES {
        let text = read(note);
        let in_note: Vec<(String, String)> = tables_of(&section(&text, "## 4. Example"));
        let want: Vec<(&str, String)> = examples().into_iter().filter(|(n, _, _)| *n == note).map(|(_, t, tab)| (t, tab)).collect();
        assert_eq!(in_note.len(), want.len(), "{note}: the tables of the example");
        for (i, ((table, follower), (test, tab))) in in_note.iter().zip(want).enumerate() {
            assert_eq!(table, &tab, "{note}: table {} differs from what the code gives", i + 1);
            assert!(follower.contains(&format!("`{test}`")), "{note}: table {} names {test}: {follower}", i + 1);
        }
    }
}

fn all_test_sources() -> String {
    let mut all = String::new();
    let mut stack = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).expect("tests folder").flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                all.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
            }
        }
    }
    all
}

// ------------------------------------------------------------------------------------------ the checks the examples name

#[test]
fn w8_the_split_date_example_is_what_the_calendar_core_gives() {
    let cal = CalendarSettings::default();
    let d = date(2026, 1, 15);
    assert_eq!((DatePart::DayName.value(d, &cal).to_string(), DatePart::WeekStarting.value(d, &cal).to_string(), DatePart::WeekOfYear.value(d, &cal).to_string()), ("Thu".to_string(), "2026-01-10".to_string(), "W03".to_string()));
    assert_eq!(DatePart::WeekOfYear.value(date(2026, 1, 1), &cal).to_string(), "W01");
    let t = split_date_table();
    assert!(t.contains("| 2026-01-15 | 2026 | Q1 | Jan | Jan 2026 | Thu | 2026-01-10 | W03 |"), "{t}");
    assert!(t.contains("| (blank) | (blank) | (blank) | (blank) | (blank) | (blank) | (blank) | (blank) |"), "{t}");
}

#[test]
fn w8_the_calendar_examples_are_what_the_calendar_core_gives() {
    let d = date(2026, 1, 15);
    let mon = CalendarSettings { fy_start_month: 1, week_start: Weekday::Mon };
    assert_eq!(DatePart::WeekStarting.value(d, &mon).to_string(), "2026-01-12");
    let jul = CalendarSettings { fy_start_month: 7, week_start: Weekday::Sat };
    assert_eq!((DatePart::FiscalYear.value(d, &jul).to_string(), DatePart::FiscalQuarter.value(d, &jul).to_string(), DatePart::FiscalMonth.value(d, &jul).to_string()), ("FY2025-26".to_string(), "FQ3".to_string(), "7".to_string()));
    assert_eq!(order_of(&SortOrder::Calendar { week_start: Weekday::Sat }, &["Mar", "Jan", "Feb"]), vec!["Jan", "Feb", "Mar"]);
    assert_eq!(order_of(&SortOrder::Calendar { week_start: Weekday::Sat }, &["Mon", "Sat", "Fri"]), vec!["Sat", "Mon", "Fri"]);
    assert_eq!(order_of(&SortOrder::Ranges, &["10%\u{2013}11%", "No rate", "0%", "8%\u{2013}9%", "> 15%", "Below 0%"]), vec!["Below 0%", "0%", "8%\u{2013}9%", "10%\u{2013}11%", "> 15%", "No rate"]);
    assert_eq!(
        order_of(&SortOrder::Ranges, &["Below 1 Crore", "25 Crore to 50 Crore", "1 Crore to 5 Crore", "Above 10 Crore", "5 Crore to 10 Crore"]),
        vec!["Below 1 Crore", "1 Crore to 5 Crore", "5 Crore to 10 Crore", "Above 10 Crore", "25 Crore to 50 Crore"]
    );
}

#[test]
fn w8_the_date_grouping_example_adds_up_to_the_ungrouped_total() {
    // grouping_table asserts that the groups add up to the total, the blank included
    let t = grouping_table();
    assert!(t.contains("| Grand total | 840.00 | 6 |"), "{t}");
}

#[test]
fn w8_the_drill_example_adds_up_at_every_level() {
    let top = drill_chart(&[]);
    let total = |d: &micro_parquet::charts::data::ChartData| d.values[0].iter().flatten().sum::<f64>();
    assert_eq!(total(&top), 3600.0);
    let dhaka = drill_chart(&["Dhaka"]);
    assert_eq!(total(&dhaka), top.values[0][top.categories.iter().position(|c| c == "Dhaka").expect("Dhaka")].expect("value"));
    let jan = drill_chart(&["Dhaka", "Jan"]);
    assert_eq!(total(&jan), dhaka.values[0][dhaka.categories.iter().position(|c| c == "Jan").expect("Jan")].expect("value"));
}

#[test]
fn w8_the_colour_tables_are_the_codes() {
    let t = warning_table();
    assert!(t.contains("Deep Blue | Low contrast on the dark background"), "{t}");
    assert!(t.contains("Deep Green | none"), "{t}");
    assert_eq!(colour_table().lines().count(), 10);
}

#[test]
fn w8_the_project_line_is_the_one_the_app_writes() {
    assert_eq!(micro_parquet::project::opened_summary(8, &[("Gap ladder".to_string(), "the file has no 'MATDAT' column".to_string())]), "8 tabs opened, 1 failed: Gap ladder \u{2014} the file has no 'MATDAT' column");
}

// ------------------------------------------------------------------------------------------ pictures and the README

/// The pictures of Phase 8 under docs/guide/img/: (file, drawn in both themes?). Written by the render tests.
pub const PICTURES: [(&str, bool); 12] = [
    ("split-date-dialog", true),
    ("calendar-dialog", true),
    ("pivot-date-expander", true),
    ("filter-by-choice", true),
    ("sources-dialog", true),
    ("drill-breadcrumb", true),
    ("drill-right-click-menu", true),
    ("colour-picker-warning", true),
    ("house-colours", true),
    ("chart-filter-tick-list", true),
    ("segment-labels-straight-and-turned", true),
    ("chart-pdf-table-below", false),
];

fn png_size(path: &std::path::Path) -> (u32, u32) {
    let b = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert!(b.len() > 24 && &b[1..4] == b"PNG", "{}: a PNG", path.display());
    (u32::from_be_bytes([b[16], b[17], b[18], b[19]]), u32::from_be_bytes([b[20], b[21], b[22], b[23]]))
}

fn pictures_named_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("](img/") {
        let tail = &rest[i + 2..];
        let end = tail.find(')').expect("a closing bracket");
        out.push(tail[..end].to_string());
        rest = &tail[end..];
    }
    out
}

#[test]
fn w8_every_picture_the_phase_8_notes_name_exists_every_listed_picture_is_used_and_the_readme_lists_the_notes() {
    let notes: Vec<&str> = NEW_NOTES.iter().cloned().chain(["chart.md", "charts.md", "filter.md", "pivot.md", "custom-sort-lists.md", "rate-profile.md", "chart-rate-profile.md", "report-packs.md"]).collect();
    let mut named: Vec<String> = Vec::new();
    for n in &notes {
        for p in pictures_named_in(&read(n)) {
            let f = guide().join(&p);
            assert!(f.is_file(), "{n}: the picture {p} exists");
            let (w, h) = png_size(&f);
            assert!(w >= 200 && h >= 60, "{n}: {p} is {w}x{h}");
            named.push(p);
        }
    }
    for (name, both) in PICTURES {
        assert!(named.iter().any(|p| p == &format!("img/{name}.png")), "a note shows img/{name}.png");
        assert!(guide().join(format!("img/{name}.png")).is_file(), "img/{name}.png exists");
        if both {
            assert!(named.iter().any(|p| p == &format!("img/{name}-dark.png")), "a note shows img/{name}-dark.png");
            assert!(guide().join(format!("img/{name}-dark.png")).is_file(), "img/{name}-dark.png exists");
        }
    }
    let readme = read("README.md");
    for n in NEW_NOTES {
        assert!(readme.contains(&format!("({n})")), "the README lists {n}");
    }
    for part in ["Project", "Calendar"] {
        assert!(readme.contains(part), "the README says something about {part}");
    }
}

/// What the older notes now say about the work of W3 and W9 to W11 (a sentence each, kept to the facts the tests pin).
#[test]
fn w8_the_notes_that_phase_8_changed_say_what_the_code_now_does() {
    let filter = read("filter.md");
    for s in ["By", "Month-Year", "Day name", "Aug 2025", "blank date", "Filters on this chart"] {
        assert!(filter.contains(s), "filter.md says '{s}'");
    }
    let chart = read("chart.md");
    for s in ["Edit chart", "Update chart tab", "Filters on this chart", "Segment labels", "Hide below", "Data table", "Below the chart", "Beside the chart", "Drill levels", "Colours", "date-grouping.md", "drill-down.md", "colours.md"] {
        assert!(chart.contains(s), "chart.md says '{s}'");
    }
    let charts = read("charts.md");
    for s in ["Data table", "Below the chart", "Beside the chart", "8 pt", "continues on the next page", "(continued)"] {
        assert!(charts.contains(s), "charts.md (the PDF note) says '{s}'");
    }
    let pivot = read("pivot.md");
    for s in ["Date parts", "Add to Rows", "Show rows"] {
        assert!(pivot.contains(s), "pivot.md says '{s}'");
    }
    let sort = read("custom-sort-lists.md");
    for s in ["Calendar", "Ranges", "Below 0%", "No rate", "Saturday"] {
        assert!(sort.contains(s), "custom-sort-lists.md says '{s}'");
    }
    let packs = read("report-packs.md");
    assert!(packs.contains("Data table") && !packs.contains("the PDF has no charts"), "report-packs.md: a pack chart with the Data table on has a page in the PDF");
    // the pack's wording and the app's are one
    for s in ["Hard to tell from", "Low contrast on the dark background", "This is a status colour"] {
        assert!(read("colours.md").contains(s), "colours.md quotes '{s}'");
    }
}

// ------------------------------------------------------------------------------------------ the chart PDF picture

/// Writes the picture of a chart PDF page with the data table below it (the PDF as the app writes it, rasterised by
/// `pdftoppm`), light only: a PDF page is always the light chart. Skipped, with a note, where `pdftoppm` is missing.
#[test]
fn w8_the_chart_pdf_picture_shows_the_page_with_its_table_below() {
    use micro_parquet::charts::data::ChartData;
    use micro_parquet::charts::layout::LayoutOpts;
    use micro_parquet::charts::spec::{DataTable, Stack};
    use micro_parquet::exporters::{export_pdf_tabs_with, PdfOptions, PdfTab};
    if std::process::Command::new("pdftoppm").arg("-v").output().is_err() {
        eprintln!("skipped: pdftoppm is not available, so the chart PDF picture was not written");
        return;
    }
    let (mut m, mut p, mut v) = (Vec::new(), Vec::new(), Vec::new());
    for (c, month) in ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"].iter().enumerate() {
        for (s, product) in ["Term deposit", "Savings", "Current"].iter().enumerate() {
            m.push(*month);
            p.push(*product);
            v.push([900.0, 420.0, 135.0][s] + 17.25 * c as f64 * (s as f64 + 1.0) + 0.37 * s as f64);
        }
    }
    let df = DataFrame::new(vec![Series::new("Month".into(), m).into(), Series::new("Product".into(), p).into(), Series::new("Balance".into(), v).into()]).expect("df");
    let order = SortOrder::Calendar { week_start: Weekday::Sat };
    let spec = ChartSpec { category: "Month".into(), series: Some("Product".into()), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, stack: Stack::Stacked, top_n: 500, order, inside_labels: true, data_table: DataTable::Below, ..Default::default() };
    let data: ChartData = compute(df.lazy(), &spec).expect("the chart");
    let dir = crate::common::dir("w8_pdf_picture");
    let path = dir.join("chart.pdf");
    let opts = LayoutOpts { dark: false, divisor: 1.0, decimals: 2, unit: String::new(), bare: false, colours: None };
    let tab = PdfTab { title: "Chart".into(), df: micro_parquet::chart::chart_table(&data).expect("table"), col_formats: Default::default(), summary: vec![], chart: Some((data, opts)) };
    let o = PdfOptions { title: "Deposits by product".into(), page: micro_parquet::pdf_layout::PdfPage::default(), chart_tables: false };
    export_pdf_tabs_with(vec![tab], &path, micro_parquet::models::NumberDisplayFormat::BdtCrore, 2, &o).expect("pdf");
    let out = guide().join("img");
    std::fs::create_dir_all(&out).expect("img folder");
    let stem = dir.join("page");
    let r = std::process::Command::new("pdftoppm").args(["-png", "-r", "110", "-f", "1", "-l", "1", "-singlefile"]).arg(&path).arg(&stem).output().expect("pdftoppm runs");
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    std::fs::copy(stem.with_extension("png"), out.join("chart-pdf-table-below.png")).expect("copy the picture");
    assert!(png_size(&out.join("chart-pdf-table-below.png")).0 > 600);
}
