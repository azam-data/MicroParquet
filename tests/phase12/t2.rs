//! Phase 12 T2: a target line and red/amber/green zones on a chart. The zones are added to the data's own spec, so every renderer shows
//! them; the user's own reference lines are left alone.

use micro_parquet::charts::calc::apply_target;
use micro_parquet::charts::data::ChartData;
use micro_parquet::charts::echarts::chart_pair;
use micro_parquet::charts::layout::LayoutOpts;
use micro_parquet::charts::spec::*;
use micro_parquet::kpi::KpiUnit;
use micro_parquet::target::{Bands, Direction};

fn data(kind: ChartKind, values: &[f64]) -> ChartData {
    let cats: Vec<String> = (0..values.len()).map(|i| format!("M{}", i + 1)).collect();
    ChartData {
        spec: ChartSpec { kind, ..Default::default() },
        categories: cats.clone(),
        series: vec!["Deposits".into()],
        values: vec![values.iter().map(|v| Some(*v)).collect()],
        rows: vec![vec![1; values.len()]],
        kept_categories: cats,
        kept_series: vec!["Deposits".into()],
        ..Default::default()
    }
}

fn target(value: f64, unit: KpiUnit, dir: Direction) -> ChartTarget {
    ChartTarget { value, unit, direction: dir, ..Default::default() }
}

#[test]
fn t2_a_a_target_adds_one_neutral_line_and_three_zones_in_the_columns_own_unit() {
    let cr = 10_000_000.0;
    let d = apply_target(data(ChartKind::Line, &[4200.0 * cr, 4600.0 * cr, 4800.0 * cr]), &target(5000.0, KpiUnit::Crore, Direction::Up)).expect("target");
    assert_eq!(d.spec.ref_lines.len(), 1);
    let l = &d.spec.ref_lines[0];
    assert_eq!((l.value, l.label.as_str(), l.status), (5000.0 * cr, "Target", Status::Neutral));
    let b = &d.spec.ref_bands;
    assert_eq!(b.len(), 3);
    assert_eq!(b.iter().map(|x| x.status).collect::<Vec<_>>(), vec![Some(Status::Critical), Some(Status::Warning), Some(Status::Good)]);
    assert!((b[0].to - 4500.0 * cr).abs() < 1.0 && (b[1].to - 5000.0 * cr).abs() < 1.0, "red below 4,500, amber to 5,000, green above");
    assert!(b[0].from <= 0.0 && b[2].to >= 5000.0 * cr, "the zones span the axis");
    assert!(b[0].label.starts_with("Red") && b[1].label.starts_with("Amber") && b[2].label.starts_with("Green"), "the colour is named too");
}

#[test]
fn t2_b_down_is_good_turns_the_zones_round_and_zones_can_be_left_off() {
    let d = apply_target(data(ChartKind::Bar, &[4.0, 5.2, 5.9]), &target(5.0, KpiUnit::Absolute, Direction::Down)).expect("target");
    assert_eq!(d.spec.ref_bands.iter().map(|x| x.status).collect::<Vec<_>>(), vec![Some(Status::Good), Some(Status::Warning), Some(Status::Critical)]);
    let t = ChartTarget { show_bands: false, label: "Budget".into(), ..target(5.0, KpiUnit::Absolute, Direction::Up) };
    let d = apply_target(data(ChartKind::Line, &[4.0, 5.2]), &t).expect("target");
    assert!(d.spec.ref_bands.is_empty() && d.spec.ref_lines[0].label == "Budget");
}

#[test]
fn t2_c_the_users_own_reference_lines_stay_and_a_chart_with_no_value_axis_is_refused() {
    let mut base = data(ChartKind::Line, &[1.0, 2.0]);
    base.spec.ref_lines.push(RefLine { value: 1.5, label: "Limit".into(), status: Status::Critical, panel: 0 });
    let d = apply_target(base, &target(2.0, KpiUnit::Absolute, Direction::Up)).expect("target");
    assert_eq!(d.spec.ref_lines.len(), 2);
    let e = apply_target(data(ChartKind::Donut, &[1.0, 2.0]), &target(2.0, KpiUnit::Absolute, Direction::Up)).expect_err("a donut has no axis");
    assert!(e.to_string().contains("column, bar, line or area chart"), "{e}");
    let bad = ChartTarget { bands: Bands { green_within_pct: 20.0, amber_within_pct: 5.0 }, ..target(2.0, KpiUnit::Absolute, Direction::Up) };
    assert!(apply_target(data(ChartKind::Line, &[1.0]), &bad).expect_err("bands").to_string().contains("amber margin"));
}

#[test]
fn t2_d_the_html_chart_draws_the_zones_in_their_colours_and_the_target_line_in_grey() {
    let d = apply_target(data(ChartKind::Line, &[4.0, 5.2, 5.9]), &target(5.0, KpiUnit::Absolute, Direction::Up)).expect("target");
    let out = chart_pair(&d, &LayoutOpts { dark: false, divisor: 1.0, decimals: 2, unit: "BDT".into(), bare: false, colours: None }).expect("html");
    let text = out.light.to_string();
    assert!(text.contains("#d03b3b") && text.contains("#0ca30c") && text.contains("#fab219"), "red, green and amber zones");
    assert!(text.contains("#6b6a65"), "the target line is grey");
    assert!(text.contains("Green: on target"), "the zones are named in words");
}

#[test]
fn t2_e_old_chart_json_has_no_target_and_a_target_round_trips() {
    let s = ChartSpec::default();
    assert!(s.target.is_none() && !serde_json::to_string(&s).expect("json").contains("target"));
    let t = ChartSpec { target: Some(target(5000.0, KpiUnit::Crore, Direction::Down)), ..Default::default() };
    let back: ChartSpec = serde_json::from_str(&serde_json::to_string(&t).expect("json")).expect("round trip");
    assert_eq!(back.target, t.target);
    assert_ne!(s.calc_key(), t.calc_key(), "a changed target is a changed chart");
}
