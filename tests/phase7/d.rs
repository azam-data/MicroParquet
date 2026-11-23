//! Owner decisions on Session A: heatmap order and missing pairs, stacked-area opacity, the Δ sign.

use micro_parquet::chart::{ChartAgg, ChartKind};
use micro_parquet::charts::calc::compute;
use micro_parquet::charts::data::ChartData;
use micro_parquet::charts::layout::{hit_test, layout_chart, ApproxMeasure, ChartLayout, LayoutOpts, MarkKind, Prim, Role};
use micro_parquet::charts::spec::{ChartSpec, MeasureSpec, Stack, Transform};
use micro_parquet::charts::tokens as t;
use micro_parquet::sort_lists::SortOrder;
use polars::prelude::*;

use egui::{pos2, Rect};

fn lay(d: &ChartData, dark: bool) -> ChartLayout {
    layout_chart(d, Rect::from_min_size(pos2(10.0, 20.0), egui::vec2(1000.0, 640.0)), &LayoutOpts { dark, divisor: 1.0, decimals: 2, unit: "BDT".into(), bare: false, colours: None }, &ApproxMeasure)
}

fn rgb(h: u32) -> egui::Color32 {
    egui::Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8)
}

/// A 5×5 CL migration where the data is in a scrambled order and some pairs are missing.
fn migration5(order: SortOrder) -> ChartData {
    let prev = ["BL", "DF", "STD", "SS", "SMA", "STD", "SMA", "SS", "DF", "BL", "STD"];
    let cur = ["BL", "BL", "STD", "DF", "SMA", "SMA", "SS", "SS", "DF", "BL", "SS"];
    let bal = [40.0, 15.0, 900.0, 25.0, 600.0, 35.0, 90.0, 700.0, 120.0, 10.0, 5.0];
    let df = DataFrame::new(vec![Series::new("Previous".into(), prev).into(), Series::new("Current".into(), cur).into(), Series::new("Balance".into(), bal).into()]).unwrap();
    let mut s = ChartSpec { category: "Current".into(), series: Some("Previous".into()), value: Some("Balance".into()), kind: ChartKind::Heatmap, order, ..Default::default() };
    s.measures = vec![MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, transform: Transform::ShareOfSeries, ..Default::default() }];
    compute(df.lazy(), &s).unwrap()
}

/// The category labels of a heatmap: columns left to right, rows top to bottom.
fn axis_labels(l: &ChartLayout) -> (Vec<String>, Vec<String>) {
    let mut cats: Vec<_> = l.texts.iter().filter(|x| x.role == Role::Category).collect();
    let top = cats.iter().map(|x| x.rect.top()).fold(f32::MAX, f32::min);
    let mut cols: Vec<_> = cats.iter().filter(|x| (x.rect.top() - top).abs() < 1.0).cloned().collect();
    cols.sort_by(|a, b| a.rect.left().partial_cmp(&b.rect.left()).unwrap());
    cats.retain(|x| (x.rect.top() - top).abs() >= 1.0);
    cats.sort_by(|a, b| a.rect.top().partial_cmp(&b.rect.top()).unwrap());
    (cols.iter().map(|x| x.text.clone()).collect(), cats.iter().map(|x| x.text.clone()).collect())
}

#[test]
fn d1_a_cl_migration_heatmap_follows_the_cl_order_and_draws_every_pair() {
    for dark in [false, true] {
        let l = lay(&migration5(SortOrder::Natural), dark);
        assert!(l.message.is_none(), "{:?}", l.message);
        let (cols, rows) = axis_labels(&l);
        assert_eq!(cols, ["STD", "SMA", "SS", "DF", "BL"]);
        assert_eq!(rows, ["STD", "SMA", "SS", "DF", "BL"]);
        let cells: Vec<_> = l.marks.iter().filter(|m| m.kind == MarkKind::Cell).collect();
        assert_eq!(cells.len(), 25, "every row × column pair is drawn");
        let mut zeros = 0;
        for c in &cells {
            let fill = l.prims.iter().find_map(|p| if let Prim::Rect { rect, fill, .. } = p { (*rect == c.rect).then_some(*fill) } else { None }).expect("a cell rect");
            if c.value == Some(0.0) {
                zeros += 1;
                assert_eq!(fill, rgb(t::heat_neutral(dark)), "a missing pair is the neutral near-surface fill (the ramp is for values above zero)");
                // its value is written inside it
                assert!(l.texts.iter().any(|x| x.role == Role::Cell && c.rect.contains_rect(x.rect) && x.text == "0.0"), "a missing pair carries its 0.0 label");
                let tip = hit_test(&l, c.rect.center()).unwrap().tip;
                assert!(tip.contains("0.0%") || tip.contains("0.00%"), "{tip}");
                assert!(tip.contains("0 rows"), "{tip}");
            }
        }
        // the data has 10 distinct (previous, current) pairs; the other 15 are zero cells
        assert_eq!(zeros, 25 - 10, "the pairs the data does not have");
        // 2 px gaps and 2 px corners
        for p in &l.prims {
            if let Prim::Rect { rect, round, .. } = p {
                if cells.iter().any(|c| c.rect == *rect) {
                    assert_eq!(round[0], t::HEAT_RADIUS_PX);
                }
            }
        }
    }
}

#[test]
fn d1_a_custom_order_list_orders_rows_and_columns_and_other_text_sorts_naturally() {
    let list = SortOrder::List { name: "rev".into(), values: ["BL", "DF", "SS", "SMA", "STD"].map(String::from).to_vec(), case_insensitive: true };
    let (cols, rows) = axis_labels(&lay(&migration5(list), false));
    assert_eq!(cols, ["BL", "DF", "SS", "SMA", "STD"]);
    assert_eq!(rows, ["BL", "DF", "SS", "SMA", "STD"]);
    // not CL codes: natural order (b2 before b10), on both axes
    let df = DataFrame::new(vec![Series::new("A".into(), ["b10", "b2", "a", "b10"]).into(), Series::new("B".into(), ["b2", "b10", "a", "a"]).into(), Series::new("V".into(), [1.0, 2.0, 3.0, 4.0]).into()]).unwrap();
    let s = ChartSpec { category: "B".into(), series: Some("A".into()), value: Some("V".into()), kind: ChartKind::Heatmap, ..Default::default() };
    let l = lay(&compute(df.lazy(), &s).unwrap(), false);
    let (cols, rows) = axis_labels(&l);
    assert_eq!(cols, ["a", "b2", "b10"]);
    assert_eq!(rows, ["a", "b2", "b10"]);
    assert_eq!(l.marks.iter().filter(|m| m.kind == MarkKind::Cell).count(), 9);
}

#[test]
fn d2_a_stacked_area_layer_is_filled_at_35_percent_with_a_two_pixel_line_on_top() {
    let df = DataFrame::new(vec![
        Series::new("Month".into(), ["May", "May", "Jun", "Jun", "Jul", "Jul"]).into(),
        Series::new("Product".into(), ["A", "B", "A", "B", "A", "B"]).into(),
        Series::new("Balance".into(), [10.0, 20.0, 12.0, 18.0, 15.0, 25.0]).into(),
    ])
    .unwrap();
    let s = ChartSpec { category: "Month".into(), series: Some("Product".into()), value: Some("Balance".into()), kind: ChartKind::Area, stack: Stack::Stacked, order: SortOrder::List { name: "m".into(), values: ["May", "Jun", "Jul"].map(String::from).to_vec(), case_insensitive: true }, ..Default::default() };
    let d = compute(df.lazy(), &s).unwrap();
    for dark in [false, true] {
        let l = lay(&d, dark);
        assert_eq!(t::STACKED_AREA_OPACITY, 0.35);
        let fills: Vec<_> = l.prims.iter().filter_map(|p| if let Prim::Tris { fill, .. } = p { Some(*fill) } else { None }).collect();
        assert_eq!(fills.len(), 2, "one fill per layer");
        for f in &fills {
            assert_eq!(f.a(), (0.35f32 * 255.0).round() as u8);
        }
        let series = if dark { t::SERIES_DARK } else { t::SERIES_LIGHT };
        for slot in 0..2 {
            let c = rgb(series[slot]);
            let n = l.prims.iter().filter(|p| matches!(p, Prim::Line { w, col, .. } if *w == t::LINE_PX && *col == c)).count();
            assert_eq!(n, 2, "a 2 px line of the layer's colour runs along its top (3 months = 2 segments)");
        }
    }
}

#[test]
fn d3_the_change_sign_is_a_delta_never_a_d() {
    assert_eq!(t::delta("dEVE"), "\u{394}EVE");
    assert_eq!(t::delta("Sum of dNII by Scenario (dEVE % of Tier 1)"), "Sum of \u{394}NII by Scenario (\u{394}EVE % of Tier 1)");
    // whole words only
    assert_eq!(t::delta("deviation, dEVEx, adNII"), "deviation, dEVEx, adNII");
    assert_eq!(t::delta("\u{394}EVE"), "\u{394}EVE");
    // the in-app layout: title, subtitle, axis, tooltip
    let df = DataFrame::new(vec![Series::new("Scenario".into(), ["Parallel up", "Parallel down"]).into(), Series::new("dEVE".into(), [-10.5, 4.8]).into()]).unwrap();
    let s = ChartSpec { category: "Scenario".into(), value: Some("dEVE".into()), kind: ChartKind::Bar, horizontal: true, ..Default::default() };
    let l = lay(&compute(df.lazy(), &s).unwrap(), false);
    let all: String = l.texts.iter().map(|x| x.text.clone()).collect::<Vec<_>>().join("|");
    assert!(all.contains("\u{394}EVE"), "{all}");
    assert!(!all.contains("dEVE"), "{all}");
    let m = l.marks.first().unwrap();
    assert!(m.tip.contains("\u{394}EVE") && !m.tip.contains("dEVE"), "{}", m.tip);
}
