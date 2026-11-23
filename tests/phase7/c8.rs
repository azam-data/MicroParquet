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
    Ctx { divisor: 1.0, suffix: "Cr".into(), dq_green: Some(98.0), dq_amber: Some(95.0), ..Default::default() }
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

/// (width, height) of a PNG, from its header.
fn png_size(path: &std::path::Path) -> (u32, u32) {
    let b = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert!(b.len() > 24 && &b[1..4] == b"PNG", "{}: a PNG", path.display());
    (u32::from_be_bytes([b[16], b[17], b[18], b[19]]), u32::from_be_bytes([b[20], b[21], b[22], b[23]]))
}

/// The in-app render is 1203 pt wide at two pixels to the point; an HTML page is 1100 px wide. A picture of one kind
/// that has the size of the other was made by the wrong tool (the HTML tool once overwrote the in-app images).
#[test]
fn c8_every_screenshot_has_the_size_of_its_kind() {
    let dir = guide().join("img/charts");
    let (mut app, mut html, mut xlsx) = (0, 0, 0);
    for e in std::fs::read_dir(&dir).unwrap() {
        let path = e.unwrap().path();
        if path.extension().map_or(true, |x| x != "png") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let (w, h) = png_size(&path);
        if name.starts_with("studio-") || name.starts_with("pdf-") {
            // pictures of the app's own controls (a cropped panel) and of a PDF page: their own sizes, light and dark twins for the panel only
            assert!(w >= 600 && h >= 600, "{name}: {w}x{h}");
            if name.starts_with("studio-") && !name.ends_with("-dark.png") {
                assert!(dir.join(name.replace(".png", "-dark.png")).is_file(), "{name}: its dark twin exists");
            }
            continue;
        }
        if name.starts_with("html-") {
            html += 1;
            assert_eq!(w, 1100, "{name}: an HTML page is 1100 px wide, it is {w}");
            assert!(h >= 300 && h <= 1500, "{name}: height {h}");
        } else if name.starts_with("xlsx-") {
            xlsx += 1;
            assert!(w >= 800 && h >= 300, "{name}: {w}x{h}");
        } else {
            app += 1;
            assert_eq!(w, 2406, "{name}: an in-app render is 2406 px wide (1203 pt at 2x), it is {w}: was it overwritten by another tool?");
            assert!(h >= 600 && h <= 2800, "{name}: height {h}");
        }
        // every picture has a dark twin
        if !name.ends_with("-dark.png") && !name.starts_with("xlsx-") {
            let twin = dir.join(name.replace(".png", "-dark.png"));
            assert!(twin.is_file(), "{name}: its dark twin exists");
        }
    }
    assert!(app >= 70 && html == 48 && xlsx == 24, "{app} in-app, {html} html, {xlsx} excel pictures");
}
