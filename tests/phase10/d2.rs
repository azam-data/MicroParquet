//! D2: KPI cards. The hand-worked numbers are the spec's; the window is tested in `src/ui/render_tests_p10.rs`.

use crate::common::{dir, parquet};
use crate::p9_m3::{dates, nums};
use micro_parquet::kpi::*;
use micro_parquet::measures::{NamedMeasure, Sign};
use micro_parquet::model::*;
use micro_parquet::models::*;
use polars::prelude::*;

fn card(compare: KpiCompare, good: GoodDirection) -> KpiSpec {
    KpiSpec { title: "Deposits".into(), compare, good_direction: good, ..Default::default() }
}

fn value(current: f64, earlier: Option<f64>, ratio: bool) -> KpiValue {
    KpiValue { current: Some(current), earlier, is_ratio: ratio, period: "Oct 2026".into(), ..Default::default() }
}

#[test]
fn d2_closing_deposits_4825_cr_against_4700_cr_is_plus_125_cr_and_plus_2_66_percent_up_and_good() {
    // 48,250,000,000 BDT = 4,825.00 Cr; 47,000,000,000 = 4,700.00 Cr; 125 / 4,700 = 0.026596
    let v = view(&card(KpiCompare::PriorPeriod, GoodDirection::Up), &value(48_250_000_000.0, Some(47_000_000_000.0), false), KpiUnit::Auto);
    assert_eq!(v.value_text, "4,825.00 Cr");
    assert_eq!(v.change_text, "+125.00 Cr \u{b7} +2.66%");
    assert_eq!((v.arrow, v.words, v.tone), ("\u{25b2}", "up", Tone::Good));
}

#[test]
fn d2_a_yield_of_11_against_10_50_is_plus_0_50_pp_and_a_cost_of_funds_down_0_40_is_green_when_down_is_good() {
    let y = view(&card(KpiCompare::Sply, GoodDirection::Up), &value(11.0, Some(10.5), true), KpiUnit::Auto);
    assert_eq!((y.value_text.as_str(), y.change_text.as_str(), y.arrow, y.words), ("11.00%", "+0.50 pp", "\u{25b2}", "up"));
    assert_eq!(y.tone, Tone::Good);
    let c = view(&card(KpiCompare::PriorPeriod, GoodDirection::Down), &value(6.0, Some(6.4), true), KpiUnit::Auto);
    assert_eq!((c.change_text.as_str(), c.arrow, c.words), ("\u{2212}0.40 pp", "\u{25bc}", "down"));
    assert_eq!(c.tone, Tone::Good, "down is good for a cost");
    // the same move with Up good is bad, and with no judgement it is neutral
    assert_eq!(view(&card(KpiCompare::PriorPeriod, GoodDirection::Up), &value(6.0, Some(6.4), true), KpiUnit::Auto).tone, Tone::Bad);
    assert_eq!(view(&card(KpiCompare::PriorPeriod, GoodDirection::Neutral), &value(6.0, Some(6.4), true), KpiUnit::Auto).tone, Tone::Neutral);
}

#[test]
fn d2_an_earlier_value_of_zero_has_no_percent_and_a_blank_one_has_no_change_and_never_a_zero() {
    let s = card(KpiCompare::PriorPeriod, GoodDirection::Up);
    let v = view(&s, &value(10_000_000.0, Some(0.0), false), KpiUnit::Auto);
    assert_eq!(v.change_text, "+1.00 Cr \u{b7} \u{2014}", "the amount stays, the percent is a dash");
    let v = view(&s, &value(10_000_000.0, None, false), KpiUnit::Auto);
    assert_eq!((v.change_text.as_str(), v.arrow), ("\u{2014}", ""), "no earlier value: no arrow, no invented change");
    // a change that rounds to nothing at the card's decimals is a level arrow
    let v = view(&s, &value(10_000_000.0, Some(10_000_001.0), false), KpiUnit::Auto);
    assert_eq!((v.arrow, v.words, v.tone), ("\u{25ba}", "unchanged", Tone::Neutral));
}

#[test]
fn d2_below_80_is_amber_on_79_50_and_no_colour_on_80_00_and_the_first_matching_limit_wins() {
    let mut s = KpiSpec { thresholds: vec![Threshold { op: ThresholdOp::Below, limit: 80.0, colour: ThresholdColour::Amber }, Threshold { op: ThresholdOp::Below, limit: 70.0, colour: ThresholdColour::Red }], ..Default::default() };
    assert_eq!(view(&s, &value(79.5, None, true), KpiUnit::Auto).threshold, Some(ThresholdColour::Amber));
    assert_eq!(view(&s, &value(80.0, None, true), KpiUnit::Auto).threshold, None);
    // the first rule that matches colours the number: 65 is below both, and the first rule says amber
    assert_eq!(view(&s, &value(65.0, None, true), KpiUnit::Auto).threshold, Some(ThresholdColour::Amber));
    s.thresholds.reverse();
    assert_eq!(view(&s, &value(65.0, None, true), KpiUnit::Auto).threshold, Some(ThresholdColour::Red));
    // a limit is in the card's unit: 4,825 Cr is not below 80
    let m = KpiSpec { thresholds: vec![Threshold { op: ThresholdOp::Below, limit: 5_000.0, colour: ThresholdColour::Red }], ..Default::default() };
    assert_eq!(view(&m, &value(48_250_000_000.0, None, false), KpiUnit::Auto).threshold, Some(ThresholdColour::Red));
    assert_eq!(view(&KpiSpec { unit: KpiUnit::Absolute, ..m }, &value(48_250_000_000.0, None, false), KpiUnit::Auto).threshold, None, "in Absolute the same number is above 5,000");
}

#[test]
fn d2_a_blank_measure_shows_a_dash_and_its_reason_never_a_zero() {
    let v = KpiValue { current: None, reason: Some("No balance in Nov 2026 for 'Accounts' (last date 31 Oct 2026)".into()), period: "Nov 2026".into(), ..Default::default() };
    let k = view(&card(KpiCompare::PriorPeriod, GoodDirection::Up), &v, KpiUnit::Auto);
    assert_eq!(k.value_text, "\u{2014}");
    assert_eq!(k.reason.as_deref(), Some("No balance in Nov 2026 for 'Accounts' (last date 31 Oct 2026)"));
    assert!(!k.value_text.contains('0') && k.threshold.is_none() && k.arrow.is_empty());
}

#[test]
fn d2_the_unit_switch_changes_the_display_and_not_the_stored_number() {
    let v = value(48_250_000_000.0, None, false);
    let mut s = KpiSpec { unit: KpiUnit::Crore, ..Default::default() };
    assert_eq!(view(&s, &v, KpiUnit::Auto).value_text, "4,825.00 Cr");
    s.unit = KpiUnit::Absolute;
    s.decimals = 0;
    assert_eq!(view(&s, &v, KpiUnit::Auto).value_text, "48,250,000,000");
    s.unit = KpiUnit::Lakh;
    assert_eq!(view(&s, &v, KpiUnit::Auto).value_text, "482,500 Lakh");
    // the stored number is the same in every unit, and the dashboard's default is used by an Automatic card
    assert_eq!(v.current, Some(48_250_000_000.0));
    assert_eq!(view(&KpiSpec { decimals: 0, ..Default::default() }, &v, KpiUnit::Million).value_text, "48,250 M");
}

#[test]
fn d2_the_arrow_and_the_words_carry_the_change_when_the_colours_are_taken_away() {
    for (cur, earlier, arrow, words) in [(2.0, 1.0, "\u{25b2}", "up"), (1.0, 2.0, "\u{25bc}", "down"), (1.0, 1.0, "\u{25ba}", "unchanged")] {
        for good in [GoodDirection::Up, GoodDirection::Down, GoodDirection::Neutral] {
            let v = view(&card(KpiCompare::PriorPeriod, good), &value(cur, Some(earlier), true), KpiUnit::Auto);
            assert_eq!((v.arrow, v.words), (arrow, words), "{good:?}: whatever the tone");
            assert!(v.alt.contains(words) && v.alt.contains("Deposits"), "the alt text says it in words: {}", v.alt);
        }
    }
}

fn deposits() -> NamedMeasure {
    NamedMeasure::Base { name: "Deposits".into(), tab: "t1".into(), column: "Balance".into(), aggregation: TimeAgg::ClosingBalance, filters: vec![], sign: Sign::Plus, weight: None }
}

/// Twelve month-ends, Nov 2025 to Oct 2026, without March 2026.
fn month_ends() -> DataFrame {
    let ends = ["2025-11-30", "2025-12-31", "2026-01-31", "2026-02-28", "2026-04-30", "2026-05-31", "2026-06-30", "2026-07-31", "2026-08-31", "2026-09-30", "2026-10-31"];
    let bal: Vec<f64> = (0..ends.len()).map(|i| if i == ends.len() - 1 { 48_250_000_000.0 } else if i == ends.len() - 2 { 47_000_000_000.0 } else { 40_000_000_000.0 + i as f64 * 1_000_000_000.0 }).collect();
    DataFrame::new(vec![dates("Date", &ends).into(), nums("Balance", &bal)]).expect("data")
}

fn run(as_of: &str, spec: KpiSpec, name: &str) -> KpiValue {
    let dd = dir(name);
    let src = parquet(&dd, "accounts", &month_ends());
    let sc = crate::p9_m5::scope5(&dd, as_of, None, vec![deposits()]);
    let req = KpiRequest { spec, tab_key: "t1".into(), base: src, steps: vec![], picks: vec![], temp_dir: dd };
    with_scope(sc, || compute_kpi(&req)).unwrap_or_else(|e| panic!("{e:#}"))
}

#[test]
fn d2_the_card_works_out_the_closing_balance_the_prior_period_and_a_sparkline_with_one_gap() {
    let spec = KpiSpec { source: KpiSource::Measure("Deposits".into()), compare: KpiCompare::PriorPeriod, sparkline: true, ..Default::default() };
    let v = run("2026-10-31", spec.clone(), "p10d2_a");
    assert_eq!((v.current, v.earlier, v.period.as_str()), (Some(48_250_000_000.0), Some(47_000_000_000.0), "Oct 2026"));
    // twelve periods, Nov 2025 to Oct 2026; March 2026 has no snapshot and is a gap, not a zero
    assert_eq!(v.spark.len(), 12);
    assert_eq!(v.spark.iter().filter(|x| x.is_none()).count(), 1, "{:?}", v.spark);
    let gap = v.spark_labels.iter().position(|l| l == "Mar 2026").expect("March");
    assert_eq!(v.spark[gap], None);
    assert_eq!(v.spark[11], v.current);
    // and what the card says
    let k = view(&spec, &v, KpiUnit::Auto);
    assert_eq!((k.value_text.as_str(), k.change_text.as_str()), ("4,825.00 Cr", "+125.00 Cr \u{b7} +2.66%"));
    assert_eq!(k.spark.iter().filter(|x| x.is_none()).count(), 1);
}

#[test]
fn d2_a_month_with_no_balance_gives_no_number_and_the_pivots_reason() {
    let v = run("2026-11-30", KpiSpec { source: KpiSource::Measure("Deposits".into()), ..Default::default() }, "p10d2_b");
    assert_eq!(v.current, None);
    let why = v.reason.expect("a reason");
    assert!(why.contains("Nov 2026") && why.contains("31 Oct 2026"), "{why}");
    assert_eq!(view(&KpiSpec::default(), &KpiValue { current: None, reason: Some(why), ..Default::default() }, KpiUnit::Auto).value_text, "\u{2014}");
}

#[test]
fn d2_a_card_without_a_calendar_link_says_what_to_do_and_an_old_card_loads() {
    let dd = dir("p10d2_c");
    let src = parquet(&dd, "accounts", &month_ends());
    let sc = crate::p9_m3::scope(&dd, crate::p9_m3::settings(1), "2026-10-31", vec![]);
    let req = KpiRequest { spec: KpiSpec { source: KpiSource::Measure("Deposits".into()), ..Default::default() }, tab_key: "t9".into(), base: src, steps: vec![], picks: vec![], temp_dir: dd };
    let e = with_scope(sc, || compute_kpi(&req)).expect_err("not linked").to_string();
    assert!(e.contains("not linked to the calendar") && e.contains("Model"), "{e}");
    // a D0 card (a title only) loads with every new field at its default
    let s: KpiSpec = serde_json::from_str(r#"{"title": "CASA"}"#).expect("loads");
    assert_eq!((s.unit, s.decimals, s.compare, s.sparkline, s.thresholds.len()), (KpiUnit::Auto, 2, KpiCompare::None, false, 0));
}
