//! C8: the guide. The notes of the charts guide quote the finding titles of the banking templates, so a
//! test pins them to what the code gives; every screenshot a note names exists; every note has its ten
//! sections; the guide README lists them all.

use micro_parquet::charts::samples as sm;
use micro_parquet::charts::templates::{prepare, yield_curves, Ctx, Template};
use polars::prelude::DataFrame;
use std::path::PathBuf;

fn guide() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/guide")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(guide().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn ctx() -> Ctx {
    Ctx { divisor: 1.0, suffix: "Cr".into(), ..Default::default() }
}

fn finding(t: Template, df: DataFrame) -> String {
    prepare(t, &df, &ctx()).unwrap_or_else(|e| panic!("{}: {e:#}", t.name())).finding
}

/// (note, the finding the code gives on the note's worked example)
fn expected() -> Vec<(&'static str, String)> {
    vec![
        ("chart-gap-ladder.md", finding(Template::GapLadder, sm::ladder_table())),
        ("chart-repricing-gap.md", finding(Template::RepricingGap, sm::repricing())),
        ("chart-nii-bridge.md", finding(Template::NiiBridge, sm::mom_table())),
        ("chart-cl-migration.md", {
            // the Phase 4 V2 example, as the matrix the tool builds
            let df = sm::cl_matrix();
            finding(Template::ClMigration, df)
        }),
        ("chart-classified-trend.md", finding(Template::ClassifiedTrend, sm::classified())),
        ("chart-irrbb-scenarios.md", finding(Template::IrrbbScenarios, sm::irrbb_results())),
        ("chart-yield-curves.md", yield_curves(&sm::curves()).unwrap().finding),
        ("chart-lcr-nsfr.md", finding(Template::LcrNsfr, sm::lcr_summary())),
        ("chart-ecl-by-stage.md", finding(Template::EclByStage, sm::ecl_summary())),
        ("chart-deposit-mix.md", finding(Template::DepositMix, sm::mix())),
        ("chart-rate-profile.md", finding(Template::RateProfile, sm::rate_bands())),
        ("chart-behaviour-profile.md", finding(Template::BehaviourProfile, sm::history(8))),
        ("chart-data-quality.md", finding(Template::DataQuality, sm::scorecard())),
    ]
}

const SECTIONS: [&str; 10] = [
    "## 1. What it does",
    "## 2. When to use it",
    "## 3. Before you start",
    "## 4. Columns you need",
    "## 5. Step by step",
    "## 6. What you get",
    "## 7. Check your result",
    "## 8. Worked example",
    "## 9. Common messages and fixes",
    "## 10. Limits and assumptions",
];

#[test]
fn c8_every_template_note_has_its_ten_sections_and_the_title_the_code_writes() {
    let want = expected();
    assert_eq!(want.len(), Template::all().len(), "one note for each template");
    for (name, f) in want {
        let text = read(name);
        let mut at = 0;
        for h in SECTIONS {
            let found = text[at..].find(h).unwrap_or_else(|| panic!("{name}: the section '{h}' is missing or out of order"));
            at += found + h.len();
        }
        assert!(text.contains(&format!("**{}**", f)), "{name}: the worked example's title is '{f}'");
    }
}

#[test]
fn c8_the_numbers_the_notes_quote_for_the_two_second_charts_are_the_codes() {
    // the NII bridge's second chart and the LCR's HQLA chart
    let p = prepare(Template::NiiBridge, &sm::mom_table_keys(), &ctx()).unwrap();
    let c = p.companion.expect("effects by key");
    assert!(read("chart-nii-bridge.md").contains(&c.finding), "{}", c.finding);
    let p = prepare(Template::LcrNsfr, &sm::lcr_summary(), &ctx()).unwrap();
    let c = p.companion.expect("hqla");
    assert!(read("chart-lcr-nsfr.md").contains(&c.finding), "{}", c.finding);
    // the gap ladder's breach label
    let p = prepare(Template::GapLadder, &sm::ladder_table(), &ctx()).unwrap();
    assert!(read("chart-gap-ladder.md").contains(&p.spec.limit_ticks[0].breach), "{}", p.spec.limit_ticks[0].breach);
}

#[test]
fn c8_every_screenshot_the_chart_notes_name_exists_and_the_readme_lists_the_notes() {
    let mut names: Vec<String> = vec!["charts.md".to_string(), "chart.md".to_string()];
    names.extend(expected().into_iter().map(|(n, _)| n.to_string()));
    let mut count = 0;
    for n in &names {
        let text = read(n);
        let mut rest = text.as_str();
        while let Some(i) = rest.find("](img/charts/") {
            let tail = &rest[i + 2..];
            let end = tail.find(')').expect("a closing bracket");
            let path = &tail[..end];
            assert!(guide().join(path).is_file(), "{n}: the picture {path} exists");
            count += 1;
            rest = &tail[end..];
        }
    }
    assert!(count >= 60, "{count} pictures are named");
    let readme = read("README.md");
    for (n, _) in expected() {
        assert!(readme.contains(n), "the README lists {n}");
    }
    assert!(readme.contains("charts.md") && readme.contains("Charts, Export and Help"));
    // the primer names every form of the studio
    let primer = read("charts.md");
    for f in ["column", "grouped", "stacked", "percent", "bar", "line", "stepped", "area", "stacked-area", "scatter", "bubble", "histogram", "box", "waterfall", "heatmap", "donut", "linked-panels", "small-multiples"] {
        assert!(primer.contains(&format!("img/charts/{f}.png")) && primer.contains(&format!("img/charts/{f}-dark.png")), "the primer shows {f} in both themes");
    }
}
