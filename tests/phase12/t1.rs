//! Phase 12 T1: targets with red/amber/green bands, bullet and gauge descriptions. Worked by hand.

use micro_parquet::kpi::*;
use micro_parquet::models::*;
use micro_parquet::target::{self, Bands, Direction, Rag};

const CR: f64 = 10_000_000.0;

fn card(dir: GoodDirection, target: f64, unit: KpiUnit, bands: Bands) -> KpiSpec {
    KpiSpec { title: "Deposits".into(), unit, good_direction: dir, target: Some(KpiTarget { source: TargetSource::Fixed(target), bands }), ..Default::default() }
}

fn money(current: f64) -> KpiValue {
    KpiValue { current: Some(current), period: "Oct 2026".into(), ..Default::default() }
}

#[test]
fn t1_a_up_is_good_green_at_the_target_amber_within_10_percent_red_beyond() {
    let spec = card(GoodDirection::Up, 5000.0, KpiUnit::Crore, Bands::default());
    let rag = |cr: f64| view(&spec, &money(cr * CR), KpiUnit::Crore).threshold;
    assert_eq!(rag(5100.0), Some(ThresholdColour::Green));
    assert_eq!(rag(5000.0), Some(ThresholdColour::Green), "exactly on target is green");
    assert_eq!(rag(4900.0), Some(ThresholdColour::Amber));
    assert_eq!(rag(4500.0), Some(ThresholdColour::Amber), "exactly 10% under is still amber");
    assert_eq!(rag(4499.0), Some(ThresholdColour::Red));
    let v = view(&spec, &money(4900.0 * CR), KpiUnit::Crore);
    assert_eq!(v.target_text, "Target 5,000.00 Cr \u{b7} 98.0% of target \u{b7} amber");
    assert!(v.alt.contains("target 5,000.00 cr"), "{}", v.alt);
}

#[test]
fn t1_b_down_is_good_for_a_cost_ratio_and_the_green_margin_can_be_widened() {
    let spec = card(GoodDirection::Down, 5.0, KpiUnit::Auto, Bands { green_within_pct: 2.0, amber_within_pct: 10.0 });
    let ratio = |p: f64| KpiValue { current: Some(p), is_ratio: true, period: "Oct 2026".into(), ..Default::default() };
    let rag = |p: f64| view(&spec, &ratio(p), KpiUnit::Crore).threshold;
    assert_eq!(rag(4.9), Some(ThresholdColour::Green));
    assert_eq!(rag(5.1), Some(ThresholdColour::Green), "5.1 is within 2% of 5.0");
    assert_eq!(rag(5.3), Some(ThresholdColour::Amber));
    assert_eq!(rag(5.5), Some(ThresholdColour::Amber));
    assert_eq!(rag(5.6), Some(ThresholdColour::Red));
    assert!(view(&spec, &ratio(5.3), KpiUnit::Crore).target_text.starts_with("Target 5.00% \u{b7} 106.0% of target"), "{}", view(&spec, &ratio(5.3), KpiUnit::Crore).target_text);
}

#[test]
fn t1_c_a_negative_target_takes_its_margins_of_its_size() {
    let b = Bands::default();
    assert_eq!(target::status(-100.0, -100.0, Direction::Up, &b), Rag::Green);
    assert_eq!(target::status(-105.0, -100.0, Direction::Up, &b), Rag::Amber);
    assert_eq!(target::status(-111.0, -100.0, Direction::Up, &b), Rag::Red);
    assert_eq!(target::status(0.0, 0.0, Direction::Up, &b), Rag::Green);
    assert_eq!(target::status(-1.0, 0.0, Direction::Up, &b), Rag::Red, "a target of 0 has no margin");
    assert_eq!(target::percent_of_target(5.0, 0.0), None);
}

#[test]
fn t1_d_the_target_replaces_the_limits_and_a_missing_or_unusable_target_says_so() {
    let mut spec = card(GoodDirection::Up, 5000.0, KpiUnit::Crore, Bands::default());
    spec.thresholds = vec![Threshold { op: ThresholdOp::Below, limit: 10.0, colour: ThresholdColour::Red }];
    assert_eq!(view(&spec, &money(5100.0 * CR), KpiUnit::Crore).threshold, Some(ThresholdColour::Green), "the limit would have said nothing; the target says green");
    // no judgement has no good side
    spec.good_direction = GoodDirection::Neutral;
    let v = view(&spec, &money(5100.0 * CR), KpiUnit::Crore);
    assert!(v.target_text.contains("Up is good") && v.graphic.is_none(), "{}", v.target_text);
    // a measure target with no number for the month
    let m = KpiSpec { target: Some(KpiTarget { source: TargetSource::Measure("Budget deposits".into()), bands: Bands::default() }), ..Default::default() };
    let none = KpiValue { target: None, target_reason: Some("The plan has no row for Oct 2026".into()), ..money(1.0) };
    let v = view(&m, &none, KpiUnit::Crore);
    assert_eq!(v.target_text, "No target: The plan has no row for Oct 2026");
    assert_eq!(v.threshold, None, "no colour without a target");
    let with = KpiValue { target: Some(4_000.0 * CR), ..money(4_100.0 * CR) };
    assert_eq!(view(&m, &with, KpiUnit::Crore).threshold, Some(ThresholdColour::Green));
    // bands the wrong way round
    let bad = card(GoodDirection::Up, 5000.0, KpiUnit::Crore, Bands { green_within_pct: 20.0, amber_within_pct: 10.0 });
    assert!(view(&bad, &money(1.0), KpiUnit::Crore).target_text.contains("amber margin is smaller"), "{}", view(&bad, &money(1.0), KpiUnit::Crore).target_text);
}

#[test]
fn t1_e_bullet_and_gauge_descriptions_cover_the_axis_with_the_three_bands_and_place_the_marks() {
    let mut spec = card(GoodDirection::Up, 5000.0, KpiUnit::Crore, Bands::default());
    spec.visual = KpiVisual::Bullet;
    let g = view(&spec, &money(4800.0 * CR), KpiUnit::Crore).graphic.expect("bullet");
    assert_eq!((g.value, g.target), (4800.0, 5000.0));
    assert!(g.lo <= 0.0 && g.hi >= 5000.0 * 1.3 - 1e-9 && g.hi >= g.value);
    // red 0..4500, amber 4500..5000, green 5000..hi, one after the other
    let b = &g.bands;
    assert_eq!(b.len(), 3);
    assert_eq!((b[0].2, b[1].2, b[2].2), (Rag::Red, Rag::Amber, Rag::Green));
    assert!((b[0].1 - 4500.0).abs() < 1e-9 && (b[1].1 - 5000.0).abs() < 1e-9);
    assert!(b[0].1 == b[1].0 && b[1].1 == b[2].0 && b[0].0 == g.lo && b[2].1 == g.hi);
    assert!((g.frac(g.target) - (5000.0 - g.lo) / (g.hi - g.lo)).abs() < 1e-12);
    assert_eq!(g.target_text, "5,000.00 Cr");
    // down is good puts green first; a gauge is the same description with another kind
    let mut down = card(GoodDirection::Down, 5.0, KpiUnit::Percent, Bands::default());
    down.visual = KpiVisual::Gauge;
    let r = KpiValue { current: Some(4.0), is_ratio: true, ..Default::default() };
    let g = view(&down, &r, KpiUnit::Crore).graphic.expect("gauge");
    assert_eq!(g.kind, KpiVisual::Gauge);
    assert_eq!(g.bands.iter().map(|b| b.2).collect::<Vec<_>>(), vec![Rag::Green, Rag::Amber, Rag::Red]);
    // a plain number card has no graphic
    spec.visual = KpiVisual::Number;
    assert!(view(&spec, &money(1.0), KpiUnit::Crore).graphic.is_none());
}

#[test]
fn t1_f_a_ring_segment_of_the_gauge_runs_on_the_half_circle_between_its_two_radii() {
    let pts = ring_segment(100.0, 100.0, 50.0, 30.0, 0.0, 1.0, 8);
    assert_eq!(pts.len(), 18);
    assert!((pts[0].0 - 50.0).abs() < 1e-9 && (pts[0].1 - 100.0).abs() < 1e-9, "starts at the left end of the outer arc");
    assert!((pts[8].0 - 150.0).abs() < 1e-9, "ends at the right end of the outer arc");
    assert!(pts.iter().all(|p| p.1 <= 100.0 + 1e-9), "only the upper half (y grows downwards)");
    let top = ring_segment(100.0, 100.0, 50.0, 30.0, 0.5, 0.5001, 2);
    assert!((top[0].1 - 50.0).abs() < 0.01, "the middle of the arc is straight up");
}

#[test]
fn t1_g_old_cards_load_without_the_new_fields_and_a_target_round_trips() {
    let old: KpiSpec = serde_json::from_str("{\"title\":\"Deposits\",\"decimals\":2}").expect("a Phase 10 card loads");
    assert!(old.target.is_none() && old.visual == KpiVisual::Number && old.window == TimeWindow::Period && old.smooth == Smooth::None);
    let new = KpiSpec { window: TimeWindow::Rolling12, smooth: Smooth::Moving3, visual: KpiVisual::Gauge, target: Some(KpiTarget { source: TargetSource::Measure("Budget".into()), bands: Bands { green_within_pct: 1.0, amber_within_pct: 5.0 } }), ..Default::default() };
    let back: KpiSpec = serde_json::from_str(&serde_json::to_string(&new).expect("json")).expect("round trip");
    assert_eq!(back, new);
}
