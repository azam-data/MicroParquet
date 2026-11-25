//! D9: the guide of Phase 10. The nine notes have the eight sections in order, every table of an "Example" is the table the code gives
//! (and names a test that exists), every picture a note names exists in light and dark, the README lists the notes, and the notes that
//! Phase 10 changed say what the code now does. The tables are made by the same calls the dashboard makes (`compute_item`, `kpi::view`).

use crate::common::{dir, parquet};
use crate::p9_m3::{nums, strs};
use crate::p9_m6::{all_test_sources, guide, md, read, section, tables_of, SECTIONS};
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec};
use micro_parquet::dashboard::*;
use micro_parquet::kpi::*;
use micro_parquet::models::*;
use polars::prelude::*;
use std::path::PathBuf;

const NOTES: [&str; 9] = [
    "dashboards.md",
    "dashboard-kpi-cards.md",
    "dashboard-filters-and-drill.md",
    "dashboard-layouts.md",
    "dashboard-templates.md",
    "export-html-report.md",
    "export-pptx.md",
    "export-pdf-pages.md",
    "monthly-dashboard-routine.md",
];

/// The same twelve-row book as D3: By Region Ctg 100, Dhaka 180, Sylhet 45; by Product FDR 260, SB 65; Dhaka alone FDR 150, SB 30.
fn book(d: &std::path::Path) -> TabSource {
    let df = DataFrame::new(vec![
        strs("Region", &["Dhaka", "Dhaka", "Dhaka", "Ctg", "Ctg", "Ctg", "Sylhet", "Sylhet"]).into(),
        strs("Product", &["FDR", "FDR", "SB", "FDR", "SB", "SB", "SB", "FDR"]).into(),
        strs("Branch", &["Gulshan", "Motijheel", "Gulshan", "Agrabad", "Agrabad", "Pahartali", "Zindabazar", "Zindabazar"]).into(),
        nums("Balance", &[100.0, 50.0, 30.0, 70.0, 20.0, 10.0, 5.0, 40.0]),
    ])
    .expect("data");
    parquet(d, "d9_book", &df)
}

fn bar(category: &str) -> ChartSpec {
    ChartSpec { category: category.into(), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 500, ..Default::default() }
}

fn numbers(src: &TabSource, category: &str, picks: Vec<(PageFilter, Vec<String>)>, d: &std::path::Path) -> (Vec<String>, Vec<f64>) {
    let req = ItemRequest { item_id: "i".into(), source: ItemSource::Chart { base: src.clone(), steps: vec![], spec: bar(category) }, picks, drill: vec![], temp_dir: d.to_path_buf() };
    match compute_item(&req).expect("the item") {
        ItemData::Chart(c) => (c.categories.clone(), c.values[0].iter().map(|v| v.expect("a number")).collect()),
        other => panic!("a chart: {other:?}"),
    }
}

fn two(v: f64) -> String {
    format!("{v:.2}")
}

fn region_table() -> String {
    let dd = dir("p10d9_region");
    let (cats, vals) = numbers(&book(&dd), "Region", vec![], &dd);
    md(&["Region".into(), "Sum of Balance".into()], &cats.iter().zip(&vals).map(|(c, v)| vec![c.clone(), two(*v)]).collect::<Vec<_>>(), 1)
}

fn drill_table() -> String {
    let dd = dir("p10d9_cross");
    let src = book(&dd);
    let (cats, all) = numbers(&src, "Region", vec![], &dd);
    assert_eq!(cats[1], "Dhaka");
    let region = ChartSpec { category: "Region".into(), ..bar("Region") };
    let pick = CrossPick::from_chart(&region, "Dhaka", "i1", Some("t1")).expect("a pick");
    let mut page = DashPage { id: "p1".into(), title: "P".into(), ..Default::default() };
    page.state.toggle_cross(pick);
    let picks: Vec<_> = page.cross_for("i2").into_iter().collect();
    let (pc, before) = numbers(&src, "Product", vec![], &dd);
    let (pc2, after) = numbers(&src, "Product", picks, &dd);
    assert_eq!(pc, pc2);
    assert_eq!(all.len(), 3);
    md(&["Product".into(), "All regions".into(), "After clicking Dhaka".into()], &pc.iter().enumerate().map(|(i, p)| vec![p.clone(), two(before[i]), two(after[i])]).collect::<Vec<_>>(), 1)
}

fn card(compare: KpiCompare, good: GoodDirection) -> KpiSpec {
    KpiSpec { title: "Deposits".into(), compare, good_direction: good, ..Default::default() }
}

fn value(current: f64, earlier: Option<f64>, ratio: bool) -> KpiValue {
    KpiValue { current: Some(current), earlier, is_ratio: ratio, period: "Oct 2026".into(), ..Default::default() }
}

fn tone(t: Tone) -> &'static str {
    match t {
        Tone::Good => "good",
        Tone::Bad => "bad",
        Tone::Neutral => "neutral",
    }
}

fn kpi_rows(rows: &[(&str, KpiSpec, KpiValue)]) -> String {
    let body: Vec<Vec<String>> = rows
        .iter()
        .map(|(name, s, v)| {
            let w = view(s, v, KpiUnit::Auto);
            vec![name.to_string(), w.value_text, w.change_text, format!("{} {}", w.arrow, w.words), tone(w.tone).to_string()]
        })
        .collect();
    md(&["Card".into(), "Number".into(), "Change".into(), "Arrow".into(), "Tone".into()], &body, 5)
}

fn kpi_money_table() -> String {
    kpi_rows(&[("Closing deposits", card(KpiCompare::PriorPeriod, GoodDirection::Up), value(48_250_000_000.0, Some(47_000_000_000.0), false))])
}

fn kpi_ratio_table() -> String {
    kpi_rows(&[
        ("Yield on loans, up is good", card(KpiCompare::Sply, GoodDirection::Up), value(11.0, Some(10.5), true)),
        ("Cost of deposits, down is good", card(KpiCompare::PriorPeriod, GoodDirection::Down), value(6.0, Some(6.4), true)),
        ("Cost of deposits, no judgement", card(KpiCompare::PriorPeriod, GoodDirection::Neutral), value(6.0, Some(6.4), true)),
    ])
}

/// (note, the test the example names, the table), in the order the notes show them.
pub fn examples() -> Vec<(&'static str, &'static str, String)> {
    vec![
        ("dashboards.md", "d9_the_example_tables_of_the_notes_are_made_by_the_dashboards_own_calls", region_table()),
        ("dashboard-kpi-cards.md", "d2_closing_deposits_4825_cr_against_4700_cr_is_plus_125_cr_and_plus_2_66_percent_up_and_good", kpi_money_table()),
        ("dashboard-kpi-cards.md", "d2_a_yield_of_11_against_10_50_is_plus_0_50_pp_and_a_cost_of_funds_down_0_40_is_green_when_down_is_good", kpi_ratio_table()),
        ("dashboard-filters-and-drill.md", "d3_clicking_dhaka_gives_the_product_bar_the_numbers_of_dhaka_rows_only_and_the_clicked_bar_keeps_all_its_marks", drill_table()),
    ]
}

#[test]
fn d9_dump_the_examples_of_the_notes() {
    let Ok(dump) = std::env::var("MP_GUIDE_DUMP") else { return };
    std::fs::create_dir_all(&dump).expect("dump folder");
    let all: Vec<String> = examples().into_iter().map(|(n, t, tab)| format!("{n} ({t})\n{tab}")).collect();
    std::fs::write(PathBuf::from(&dump).join("phase10_examples.txt"), all.join("\n\n===\n\n")).expect("write");
}

#[test]
fn d9_the_example_tables_of_the_notes_are_made_by_the_dashboards_own_calls() {
    // hand-worked: by region 100 + 180 + 45; by product 260 and 65; Dhaka alone 150 and 30
    assert_eq!(region_table().lines().skip(2).collect::<Vec<_>>(), ["| Ctg | 100.00 |", "| Dhaka | 180.00 |", "| Sylhet | 45.00 |"]);
    assert_eq!(drill_table().lines().skip(2).collect::<Vec<_>>(), ["| FDR | 260.00 | 150.00 |", "| SB | 65.00 | 30.00 |"]);
}

#[test]
fn d9_the_nine_notes_have_their_eight_sections_and_their_examples_are_the_tables_the_code_gives() {
    if std::env::var("MP_GUIDE_DUMP").is_ok_and(|d| !d.is_empty()) {
        return;
    }
    let sources = all_test_sources();
    for n in NOTES {
        let text = read(n);
        assert!(text.lines().next().is_some_and(|l| l.starts_with("# ")), "{n}: a title");
        let headings: Vec<&str> = text.lines().filter(|l| l.starts_with("## ")).map(str::trim_end).collect();
        assert_eq!(headings, SECTIONS.to_vec(), "{n}: the eight sections, in order");
        for s in SECTIONS {
            assert!(!section(&text, s).trim().is_empty(), "{n}: '{s}' is empty");
        }
        let how = section(&text, "## 3. How to use it");
        assert!(how.lines().any(|l| l.starts_with("1. ")) && how.lines().any(|l| l.starts_with("2. ")), "{n}: numbered steps");
        // every note names at least one test that exists, in its example
        let example = section(&text, "## 4. Example");
        let named: Vec<&str> = example.lines().filter(|l| l.trim_start().starts_with("Test:")).flat_map(|l| l.split('`').skip(1).step_by(2)).collect();
        assert!(!named.is_empty(), "{n}: the example names a test");
        for t in named {
            assert!(sources.contains(&format!("fn {t}(")), "{n}: the test {t} exists");
        }
        let lines = text.lines().count();
        assert!(lines <= 150, "{n}: {lines} lines");
        for w in ["Polars", "Rust", "serde", "LazyFrame", "Expr"] {
            assert!(!text.contains(w), "{n}: '{w}' is a word of the code, not of a banker");
        }
    }
    let all = examples();
    for note in NOTES {
        let in_note = tables_of(&section(&read(note), "## 4. Example"));
        let want: Vec<(&str, String)> = all.iter().filter(|(n, _, _)| *n == note).map(|(_, t, tab)| (*t, tab.clone())).collect();
        assert_eq!(in_note.len(), want.len(), "{note}: the tables of the example");
        for (i, ((table, follower), (test, tab))) in in_note.iter().zip(want).enumerate() {
            assert_eq!(table, &tab, "{note}: table {} differs from what the code gives", i + 1);
            assert!(follower.contains(&format!("`{test}`")), "{note}: table {} names {test}: {follower}", i + 1);
        }
    }
}

const PICTURES: [&str; 5] = ["dashboard-page", "dashboard-kpi", "dashboard-cross", "dashboard-free", "dashboard-design"];

#[test]
fn d9_every_picture_a_note_names_exists_in_light_and_dark_and_the_readme_lists_the_notes() {
    let img = guide().join("img");
    let mut named = std::collections::BTreeSet::new();
    for n in NOTES {
        for line in read(n).lines() {
            let mut rest = line;
            while let Some(i) = rest.find("](img/") {
                let tail = &rest[i + 6..];
                let file = tail.split(')').next().unwrap_or("");
                assert!(img.join(file).metadata().map(|m| m.len() > 3000).unwrap_or(false), "{n}: the picture {file} exists");
                if !file.contains("-dark") {
                    assert!(img.join(file.replace(".png", "-dark.png")).metadata().map(|m| m.len() > 3000).unwrap_or(false), "{n}: {file} has a dark version");
                }
                named.insert(file.to_string());
                rest = &tail[file.len()..];
            }
        }
    }
    for p in PICTURES {
        assert!(named.contains(&format!("{p}.png")) && named.contains(&format!("{p}-dark.png")), "a note shows {p} in both themes");
    }
    let readme = read("README.md");
    for n in NOTES {
        assert!(readme.contains(&format!("]({n})")), "the README lists {n}");
    }
    assert!(readme.contains("## Dashboards and reports (Phase 10)"));
}

#[test]
fn d9_the_notes_that_phase_10_changed_say_what_the_code_now_does() {
    for (note, needles) in [
        ("projects.md", vec!["dashboard", "dashboards.md"]),
        ("report-packs.md", vec!["export-pdf-pages.md", "export-html-report.md"]),
        ("pivot.md", vec!["Unit for money columns", "dashboards.md"]),
        ("chart.md", vec!["dashboards.md"]),
    ] {
        let text = read(note);
        for n in needles {
            assert!(text.contains(n), "{note} says {n}");
        }
    }
    // the sentences the notes quote from the export dialog are in the window's code
    let src = all_test_sources();
    for s in ["Will you email this file?", "Use design as slide background"] {
        assert!(src.contains(s), "the window says: {s}");
    }
}
