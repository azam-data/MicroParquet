//! Phase 13 N1: notes on a chart. A numbered marker over the category, and the text in the footnote of every output.

use micro_parquet::charts::calc::{annotation_lines, apply_annotations};
use micro_parquet::charts::data::ChartData;
use micro_parquet::charts::echarts::chart_pair;
use micro_parquet::charts::layout::{layout_chart, ApproxMeasure, LayoutOpts, Prim};
use micro_parquet::charts::spec::*;

fn data(notes: Vec<ChartNote>) -> ChartData {
    let cats: Vec<String> = ["Jan 2026", "Feb 2026", "Mar 2026", "Apr 2026"].iter().map(|s| s.to_string()).collect();
    ChartData {
        spec: ChartSpec { kind: ChartKind::Line, annotations: notes, ..Default::default() },
        categories: cats.clone(),
        series: vec!["Deposits".into()],
        values: vec![vec![Some(10.0), Some(12.0), Some(11.0), Some(15.0)]],
        rows: vec![vec![1; 4]],
        kept_categories: cats,
        kept_series: vec!["Deposits".into()],
        ..Default::default()
    }
}

fn note(category: &str, text: &str) -> ChartNote {
    ChartNote { category: category.into(), text: text.into() }
}

fn opts() -> LayoutOpts {
    LayoutOpts { dark: false, divisor: 1.0, decimals: 2, unit: "BDT".into(), bare: false, colours: None }
}

fn discs(d: &ChartData) -> usize {
    let r = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(900.0, 500.0));
    layout_chart(d, r, &opts(), &ApproxMeasure).prims.iter().filter(|p| matches!(p, Prim::Circle { r, .. } if (*r - 8.0).abs() < 0.01)).count()
}

#[test]
fn n1_a_each_note_is_one_numbered_footnote_line_and_a_note_not_on_the_chart_says_so() {
    let d = apply_annotations(data(vec![note("Feb 2026", "Policy rate cut"), note("Jun 2026", "Budget speech"), note("Apr 2026", "  ")]));
    assert_eq!(d.notes, vec!["[1] Feb 2026: Policy rate cut".to_string(), "Note [2] \"Budget speech\" is not shown: 'Jun 2026' is not on this chart now.".to_string()]);
    // a note with no text writes nothing, and the numbers stay the notes' places in the list
    assert_eq!(annotation_lines(&data(vec![note("Jan 2026", ""), note("Mar 2026", "Eid")])), vec!["[2] Mar 2026: Eid".to_string()]);
}

#[test]
fn n1_b_the_chart_draws_a_marker_for_each_note_on_the_chart_and_none_for_the_rest() {
    assert_eq!(discs(&data(vec![])), 0);
    assert_eq!(discs(&data(vec![note("Feb 2026", "Policy rate cut"), note("Apr 2026", "Eid")])), 2);
    assert_eq!(discs(&data(vec![note("Jun 2026", "gone"), note("Mar 2026", "")])), 0, "a note off the chart or without text has no marker");
}

#[test]
fn n1_c_the_html_chart_has_the_marker_and_the_footnote_carries_the_text_and_a_pdf_layout_too() {
    let d = apply_annotations(data(vec![note("Feb 2026", "Policy rate cut")]));
    let out = chart_pair(&d, &opts()).expect("html");
    let text = out.light.to_string();
    assert!(text.contains("Note 1: Policy rate cut"), "the marker's tooltip");
    assert!(text.contains("[1] Feb 2026: Policy rate cut"), "the footnote");
    assert!(!chart_pair(&apply_annotations(data(vec![])), &opts()).expect("html").light.to_string().contains("\"Notes\""), "no series without notes");
    // the same footnote reaches the layout (screen and PDF draw from it)
    let r = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(900.0, 500.0));
    let l = layout_chart(&d, r, &opts(), &ApproxMeasure);
    assert!(l.texts.iter().any(|t| t.text.contains("[1] Feb 2026: Policy rate cut")), "footnote text in the layout");
}

#[test]
fn n1_d_old_chart_json_has_no_notes_and_notes_round_trip() {
    let s = ChartSpec::default();
    assert!(s.annotations.is_empty() && !serde_json::to_string(&s).expect("json").contains("annotations"));
    let t = ChartSpec { annotations: vec![note("Mar 2026", "Policy rate cut")], ..Default::default() };
    let back: ChartSpec = serde_json::from_str(&serde_json::to_string(&t).expect("json")).expect("round trip");
    assert_eq!(back.annotations, t.annotations);
}

#[test]
fn n1_e_a_powerpoint_slide_footnote_carries_the_notes_of_its_chart_and_only_a_chart() {
    use micro_parquet::dash_export::slide_footnote;
    use micro_parquet::dashboard::ItemData;
    let d = data(vec![note("Feb 2026", "Policy rate cut"), note("Mar 2026", "Eid")]);
    let foot = slide_footnote("31 Oct 2026", &["Wing = Islami".to_string()], &ItemData::Chart(Box::new(d)));
    assert_eq!(foot, "As of 31 Oct 2026 \u{b7} Filters: Wing = Islami \u{b7} [1] Feb 2026: Policy rate cut \u{b7} [2] Mar 2026: Eid");
    let plain = slide_footnote("31 Oct 2026", &[], &ItemData::Problem("x".into()));
    assert_eq!(plain, "As of 31 Oct 2026 \u{b7} Filters: none");
}
