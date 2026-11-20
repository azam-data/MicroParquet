//! C0: the chart model, the design tokens and the data (aggregation with several measures, shares,
//! running sums, waterfall steps, heatmap matrices, the colour map, and old Phase 6 settings).

use micro_parquet::chart::{ChartAgg, ChartKind, ChartScale, ChartSpec};
use micro_parquet::charts::calc::compute;
use micro_parquet::charts::colours::{Colour, ColourMap};
use micro_parquet::charts::data::{heatmap, running_sum, shares, waterfall_from_data, waterfall_steps, ChartData, StepKind};
use micro_parquet::charts::spec::{MeasureSpec, Stack, Transform, WaterfallMode};
use micro_parquet::charts::tokens as t;
use micro_parquet::sort_lists::SortOrder;
use polars::prelude::*;

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

// ---------------------------------------------------------------------------------------- tokens

#[test]
fn c0_every_hex_value_of_the_palette_tables_is_pinned() {
    // Chart surface, inks, gridline, baseline and de-emphasised: light, then dark
    assert_eq!(
        [t::INK_LIGHT.surface, t::INK_LIGHT.ink, t::INK_LIGHT.ink2, t::INK_LIGHT.muted, t::INK_LIGHT.grid, t::INK_LIGHT.baseline, t::INK_LIGHT.deemph],
        [0xfcfcfb, 0x0b0b0b, 0x52514e, 0x898781, 0xe1e0d9, 0xc3c2b7, 0xc3c2b7]
    );
    assert_eq!(
        [t::INK_DARK.surface, t::INK_DARK.ink, t::INK_DARK.ink2, t::INK_DARK.muted, t::INK_DARK.grid, t::INK_DARK.baseline, t::INK_DARK.deemph],
        [0x1a1a19, 0xf0efec, 0xc3c2b7, 0x898781, 0x2c2c2a, 0x383835, 0x55544f]
    );
    // the eight categorical slots, in fixed order
    assert_eq!(t::SERIES_LIGHT, [0x2a78d6, 0xeb6834, 0x1baf7a, 0xeda100, 0xe87ba4, 0x008300, 0x6250d6, 0xe34948]);
    assert_eq!(t::SERIES_DARK, [0x3987e5, 0xd95926, 0x199e70, 0xc98500, 0xd55181, 0x0ca30c, 0x9085e9, 0xe66767]);
    assert_eq!(t::SLOT_NAMES, ["blue", "orange", "aqua", "yellow", "magenta", "green", "violet", "red"]);
    // sequential, diverging, status
    assert_eq!(t::SEQUENTIAL, [0xcde2fb, 0xb7d3f6, 0x9ec5f4, 0x86b6ef, 0x6da7ec, 0x5598e7, 0x3987e5, 0x2a78d6, 0x256abf, 0x1c5cab, 0x184f95, 0x104281, 0x0d366b]);
    assert_eq!((t::DIVERGING_POSITIVE, t::DIVERGING_NEGATIVE, t::DIVERGING_ZERO), (0x2a78d6, 0xe34948, 0xc3c2b7));
    assert_eq!((t::STATUS_GOOD, t::STATUS_WARNING, t::STATUS_SERIOUS, t::STATUS_CRITICAL), (0x0ca30c, 0xfab219, 0xec835a, 0xd03b3b));
    // text is white from step 7 of the ramp (#3987e5)
    assert_eq!(t::SEQUENTIAL[t::SEQUENTIAL_WHITE_TEXT_FROM], 0x3987e5);
    assert_eq!(t::hex_text(0x2a78d6), "#2a78d6");
    assert_eq!(t::hex_text(0x008300), "#008300");
}

#[test]
fn c0_the_sizes_of_the_design_system_are_pinned() {
    assert_eq!((t::TITLE_PT, t::SUBTITLE_PT, t::TICK_PT, t::LABEL_PT, t::FOOTNOTE_PT), (15.0, 11.0, 10.0, 10.0, 9.0));
    assert!((9.5..=10.0).contains(&t::DATA_LABEL_PT));
    assert_eq!((t::BAR_MAX_PX, t::BAR_RADIUS_PX, t::SEGMENT_GAP_PX), (24.0, 4.0, 2.0));
    assert_eq!((t::LINE_PX, t::MARKER_PX, t::MARKER_RING_PX, t::REFERENCE_LINE_PX, t::GRID_PX), (2.0, 8.0, 2.0, 1.5, 1.0));
    assert_eq!((t::AREA_OPACITY, t::STACKED_AREA_OPACITY, t::HEAT_RADIUS_PX, t::HEAT_GAP_PX, t::PADDING_SHARE, t::MIN_SPACE_PX), (0.10, 0.35, 2.0, 2.0, 0.03, 8.0));
    assert_eq!((t::MAX_POINTS, t::MAX_FACETS, t::MAX_PANELS, t::MAX_SCATTER_SERIES), (5000, 12, 3, 3));
    assert!(t::FONT_HTML.starts_with("'Source Sans 3'") && t::FONT_EXCEL == "Segoe UI");
}

// ---------------------------------------------------------------------------------- colour map

#[test]
fn c0_filtering_out_b_keeps_a_and_c_on_their_colours() {
    let mut m = ColourMap::default();
    m.assign_all(&["A", "B", "C"]);
    assert_eq!((m.get("A"), m.get("B"), m.get("C")), (Some(Colour::Slot(0)), Some(Colour::Slot(1)), Some(Colour::Slot(2))));
    // B is filtered out: the chart now has A and C; the map is the one stored in the spec
    let stored = serde_json::to_string(&m).unwrap();
    let mut again: ColourMap = serde_json::from_str(&stored).unwrap();
    again.assign_all(&["A", "C"]);
    assert_eq!((again.get("A"), again.get("C")), (Some(Colour::Slot(0)), Some(Colour::Slot(2))));
    // B comes back: still its own slot, never given to a newcomer meanwhile
    assert_eq!(again.assign("D"), Colour::Slot(3));
    assert_eq!(again.assign("B"), Colour::Slot(1));
    // light and dark have the same slot, different values
    assert_eq!(Colour::Slot(0).hex(false), 0x2a78d6);
    assert_eq!(Colour::Slot(0).hex(true), 0x3987e5);
}

#[test]
fn c0_a_ninth_category_goes_to_other_and_a_series_called_other_is_always_other() {
    let mut m = ColourMap::default();
    let names: Vec<String> = (1..=9).map(|i| format!("S{i}")).collect();
    m.assign_all(&names);
    for i in 0..8 {
        assert_eq!(m.get(&names[i]), Some(Colour::Slot(i)), "slot {i}");
    }
    assert_eq!(m.get("S9"), Some(Colour::Other));
    assert_eq!(Colour::Other.hex(false), t::INK_LIGHT.deemph);
    assert_eq!(Colour::Other.hex(true), t::INK_DARK.deemph);
    // the 9th keeps being Other after a reload, and "Other" never takes a slot
    let back: ColourMap = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
    assert_eq!(back.get("S9"), Some(Colour::Other));
    let mut fresh = ColourMap::default();
    assert_eq!(fresh.assign("Other"), Colour::Other);
    assert_eq!(fresh.assign("A"), Colour::Slot(0));
}

// ---------------------------------------------------------------------------- shares and sums

#[test]
fn c0_percent_of_total_rows_sum_to_100() {
    let values = vec![vec![Some(1_065.0), Some(1_099.5), None, Some(7.25)], vec![Some(300.0), Some(12.125), Some(80.0), Some(0.75)]];
    for (tr, name) in [(Transform::ShareOfTotal, "total"), (Transform::ShareOfCategory, "category"), (Transform::ShareOfSeries, "row")] {
        let (out, _) = shares(&values, tr);
        match tr {
            Transform::ShareOfTotal => {
                let s: f64 = out.iter().flatten().flatten().sum();
                assert!((s - 100.0).abs() <= 0.01, "{name}: {s}");
            }
            Transform::ShareOfCategory => {
                for c in 0..4 {
                    let s: f64 = out.iter().filter_map(|r| r[c]).sum();
                    assert!((s - 100.0).abs() <= 0.01, "{name} {c}: {s}");
                }
            }
            _ => {
                for r in &out {
                    let s: f64 = r.iter().flatten().sum();
                    assert!((s - 100.0).abs() <= 0.01, "{name}: {s}");
                }
            }
        }
        assert_eq!(out[0][2], None, "a blank stays blank");
    }
}

#[test]
fn c0_a_share_of_a_zero_total_is_blank_with_a_note_not_a_made_up_number() {
    let (out, notes) = shares(&vec![vec![Some(5.0), Some(-5.0)]], Transform::ShareOfTotal);
    assert_eq!(out, vec![vec![None, None]]);
    assert!(notes.iter().any(|n| n.contains("total they are a share of is zero")), "{notes:?}");
    let (_, notes) = shares(&vec![vec![Some(5.0), Some(-1.0)]], Transform::ShareOfTotal);
    assert!(notes.iter().any(|n| n.contains("negatives")), "{notes:?}");
}

#[test]
fn c0_a_running_sum_of_the_gaps_is_the_cumulative_gap() {
    assert_eq!(running_sum(&vec![vec![Some(-1850.0), Some(-620.0), Some(410.0)]]), vec![vec![Some(-1850.0), Some(-2470.0), Some(-2060.0)]]);
    // a blank stays blank and does not stop the sum
    assert_eq!(running_sum(&vec![vec![Some(1.0), None, Some(2.0)]]), vec![vec![Some(1.0), None, Some(3.0)]]);
}

// ------------------------------------------------------------------------------------ waterfall

#[test]
fn c0_the_mom_example_gives_the_waterfall_steps_and_floating_bases() {
    let steps = waterfall_steps(("Sep interest", 80.0), &[("Volume".into(), 8.0), ("Rate".into(), 5.0), ("Mix".into(), 0.5)], Some(("Oct interest", 93.5))).unwrap();
    let vals: Vec<f64> = steps.iter().map(|s| s.value).collect();
    assert_eq!(vals, vec![80.0, 8.0, 5.0, 0.5, 93.5]);
    let kinds: Vec<StepKind> = steps.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, vec![StepKind::Start, StepKind::Delta, StepKind::Delta, StepKind::Delta, StepKind::End]);
    // floating bases of the three steps
    assert_eq!((steps[1].base, steps[2].base, steps[3].base), (80.0, 88.0, 93.0));
    assert_eq!((steps[1].top, steps[2].top, steps[3].top), (88.0, 93.0, 93.5));
    // start and end stand on zero
    assert_eq!((steps[0].base, steps[0].top, steps[4].base, steps[4].top), (0.0, 80.0, 0.0, 93.5));
}

#[test]
fn c0_a_negative_step_floats_below_the_level_and_a_bridge_that_does_not_add_up_is_refused() {
    let steps = waterfall_steps(("A", 100.0), &[("Down".into(), -30.0), ("Up".into(), 10.0)], None).unwrap();
    assert_eq!((steps[1].base, steps[1].top, steps[1].level), (70.0, 100.0, 70.0));
    assert_eq!((steps[2].base, steps[2].top), (70.0, 80.0));
    assert_eq!((steps[3].kind, steps[3].value, steps[3].label.as_str()), (StepKind::End, 80.0, "Total"));
    let e = waterfall_steps(("A", 100.0), &[("Down".into(), -30.0)], Some(("B", 75.0))).unwrap_err().to_string();
    assert!(e.contains("does not add up") && e.contains("70.00") && e.contains("75.00"), "{e}");
}

#[test]
fn c0_a_waterfall_is_read_from_the_chart_table_and_a_blank_step_is_refused() {
    let mut d = ChartData::default();
    d.categories = vec!["Sep".into(), "Volume".into(), "Rate".into(), "Mix".into(), "Oct".into()];
    d.series = vec![String::new()];
    d.values = vec![vec![Some(80.0), Some(8.0), Some(5.0), Some(0.5), Some(93.5)]];
    let steps = waterfall_from_data(&d).unwrap();
    assert_eq!(steps.len(), 5);
    d.spec.waterfall = WaterfallMode::StartSteps;
    // the last one is read as one more step now, and the end is calculated
    let steps = waterfall_from_data(&d).unwrap();
    assert_eq!((steps.len(), steps[5].value), (6, 187.0));
    d.values[0][2] = None;
    let e = waterfall_from_data(&d).unwrap_err().to_string();
    assert!(e.contains("'Rate' has no value"), "{e}");
}

// -------------------------------------------------------------------------- aggregation and heatmap

/// The Phase 4 V2 example: previous A1 STD 100, A2 SS 50, A3 STD 30; current A1 SMA 100, A2 DF 50, A4 STD 20.
fn migration() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Previous".into(), ["STD", "SS", "STD", "New"]).into(),
        Series::new("Current".into(), ["SMA", "DF", "Closed/Repaid", "STD"]).into(),
        Series::new("Balance".into(), [100.0, 50.0, 30.0, 20.0]).into(),
    ])
    .unwrap()
}

fn spec(category: &str, series: Option<&str>, value: Option<&str>) -> ChartSpec {
    ChartSpec { category: category.into(), series: series.map(str::to_string), value: value.map(str::to_string), ..Default::default() }
}

#[test]
fn c0_each_heatmap_row_of_the_v2_example_as_percent_of_row_sums_to_100() {
    let mut s = spec("Current", Some("Previous"), Some("Balance"));
    s.kind = ChartKind::Heatmap;
    s.order = SortOrder::List { name: "CL".into(), values: ["STD", "SMA", "SS", "DF", "Closed/Repaid"].map(String::from).to_vec(), case_insensitive: true };
    let d = compute(migration().lazy(), &s).unwrap();
    let h = heatmap(&d, Transform::ShareOfSeries);
    assert_eq!(h.cols, ["STD", "SMA", "DF", "Closed/Repaid"].map(String::from).to_vec().into_iter().filter(|c| h.cols.contains(c)).collect::<Vec<_>>());
    for (ri, r) in h.rows.iter().enumerate() {
        let sum: f64 = h.cells[ri].iter().flatten().sum();
        assert!((sum - 100.0).abs() <= 0.01, "row {r}: {sum}");
    }
    // the cells: SS -> DF is 100% of SS's row; STD -> SMA is 100/130
    let (ss, std_) = (h.rows.iter().position(|r| r == "SS").unwrap(), h.rows.iter().position(|r| r == "STD").unwrap());
    let df = h.cols.iter().position(|c| c == "DF").unwrap();
    let sma = h.cols.iter().position(|c| c == "SMA").unwrap();
    assert!(near(h.cells[ss][df].unwrap(), 100.0));
    assert!((h.cells[std_][sma].unwrap() - 76.923_076_9).abs() < 1e-6);
    assert_eq!(h.row_totals[std_], Some(130.0));
    assert_eq!((h.min.is_some(), h.max.map(|m| (m - 100.0).abs() < 1e-9)), (true, Some(true)));
}

#[test]
fn c0_the_transform_is_applied_on_the_worker_and_percent_of_total_sums_to_100() {
    let mut s = spec("Current", None, Some("Balance"));
    s.measures = vec![MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, transform: Transform::ShareOfTotal, ..Default::default() }];
    let d = compute(migration().lazy(), &s).unwrap();
    let total: f64 = d.values[0].iter().flatten().sum();
    assert!((total - 100.0).abs() <= 0.01, "{total}");
    assert_eq!(d.measure_names, vec!["Sum of Balance".to_string()]);
}

#[test]
fn c0_a_running_sum_measure_follows_the_category_order() {
    let df = DataFrame::new(vec![
        Series::new("Bucket".into(), ["1-3M", "Sight", "≤1M"]).into(),
        Series::new("Gap".into(), [410.0, -1850.0, -620.0]).into(),
    ])
    .unwrap();
    let mut s = spec("Bucket", None, Some("Gap"));
    s.order = SortOrder::List { name: "b".into(), values: ["Sight", "≤1M", "1-3M"].map(String::from).to_vec(), case_insensitive: true };
    s.measures = vec![MeasureSpec { column: Some("Gap".into()), agg: ChartAgg::Sum, transform: Transform::RunningSum, ..Default::default() }];
    let d = compute(df.lazy(), &s).unwrap();
    assert_eq!(d.categories, vec!["Sight", "≤1M", "1-3M"]);
    assert_eq!(d.values, vec![vec![Some(-1850.0), Some(-2470.0), Some(-2060.0)]]);
}

fn book() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Product".into(), ["TD", "TD", "SB", "SB", "CD", "TD"]).into(),
        Series::new("Branch".into(), ["N", "S", "N", "S", "N", "N"]).into(),
        Series::new("Balance".into(), [100.0, 50.0, 40.0, 10.0, 5.0, 25.0]).into(),
        Series::new("Rate".into(), [10.0, 8.0, 4.0, 4.0, 0.0, 12.0]).into(),
        Series::new("Limit".into(), [3.0, 1.0, 2.0, 1.0, 1.0, 2.0]).into(),
    ])
    .unwrap()
}

#[test]
fn c0_several_measures_a_size_and_a_weighted_average_share_the_categories() {
    let mut s = spec("Product", None, None);
    s.by_value = true;
    s.measures = vec![
        MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, ..Default::default() },
        MeasureSpec { column: Some("Rate".into()), agg: ChartAgg::WeightedAvg, weight: Some("Balance".into()), label: "Avg rate".into(), ..Default::default() },
        MeasureSpec { column: None, agg: ChartAgg::Count, ..Default::default() },
    ];
    s.size = Some(MeasureSpec { column: Some("Limit".into()), agg: ChartAgg::Sum, ..Default::default() });
    let d = compute(book().lazy(), &s).unwrap();
    assert_eq!(d.categories, vec!["TD", "SB", "CD"]);
    assert_eq!(d.values, vec![vec![Some(175.0), Some(50.0), Some(5.0)]]);
    assert_eq!(d.measure_names, vec!["Sum of Balance", "Avg rate", "Count of rows"]);
    // W.Avg of TD = (100*10 + 50*8 + 25*12) / 175
    let wa = d.extra[0].values[0][0].unwrap();
    assert!((wa - (1000.0 + 400.0 + 300.0) / 175.0).abs() < 1e-9, "{wa}");
    assert_eq!(d.extra[1].values, vec![vec![Some(3.0), Some(2.0), Some(1.0)]]);
    assert_eq!(d.size.as_ref().unwrap().values, vec![vec![Some(6.0), Some(3.0), Some(1.0)]]);
    assert_eq!(d.measure(2).unwrap().name, "Count of rows");
    assert!(d.measure(9).is_none());
}

#[test]
fn c0_every_measure_folds_the_same_categories_into_other() {
    let mut s = spec("Product", None, None);
    s.top_n = 1;
    s.measures = vec![
        MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, ..Default::default() },
        MeasureSpec { column: Some("Limit".into()), agg: ChartAgg::Sum, ..Default::default() },
    ];
    let d = compute(book().lazy(), &s).unwrap();
    assert_eq!(d.categories, vec!["TD", "Other"]);
    assert_eq!(d.extra[0].values, vec![vec![Some(6.0), Some(4.0)]]);
    assert_eq!(d.values, vec![vec![Some(175.0), Some(55.0)]]);
}

#[test]
fn c0_a_facet_gives_one_chart_table_per_value_and_more_than_twelve_is_refused() {
    let mut s = spec("Product", None, Some("Balance"));
    s.facet = Some("Branch".into());
    let d = compute(book().lazy(), &s).unwrap();
    let names: Vec<&str> = d.facets.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["N", "S"], "largest first by rows");
    let n = &d.facets[0].data;
    assert_eq!(n.categories.iter().zip(n.values[0].iter()).map(|(c, v)| (c.as_str(), v.unwrap())).collect::<Vec<_>>().len(), 3);
    assert_eq!(d.facets[1].data.values[0].iter().flatten().sum::<f64>(), 60.0);
    // 13 values
    let many = DataFrame::new(vec![
        Series::new("P".into(), vec!["a"; 13]).into(),
        Series::new("F".into(), (0..13).map(|i| format!("f{i}")).collect::<Vec<_>>()).into(),
        Series::new("V".into(), vec![1.0; 13]).into(),
    ])
    .unwrap();
    let mut s = spec("P", None, Some("V"));
    s.facet = Some("F".into());
    let e = compute(many.lazy(), &s).unwrap_err().to_string();
    assert!(e.contains("13 different values") && e.contains("at most 12"), "{e}");
    s.facet = Some("P".into());
    assert!(compute(book().lazy(), &ChartSpec { facet: Some("Product".into()), ..spec("Product", None, Some("Balance")) }).is_err());
}

#[test]
fn c0_an_empty_chart_says_why() {
    let empty = DataFrame::new(vec![Series::new("Product".into(), Vec::<String>::new()).into(), Series::new("Balance".into(), Vec::<f64>::new()).into()]).unwrap();
    let d = compute(empty.lazy(), &spec("Product", None, Some("Balance"))).unwrap();
    assert!(d.is_empty());
    assert!(d.empty_reason().unwrap().contains("no rows"));
    let blank = DataFrame::new(vec![Series::new("Product".into(), ["a"]).into(), Series::new("Balance".into(), [None::<f64>]).into()]).unwrap();
    let d = compute(blank.lazy(), &spec("Product", None, Some("Balance"))).unwrap();
    assert!(d.empty_reason().unwrap().contains("blank"));
}

#[test]
fn c0_the_phase_6_engine_still_gives_the_same_numbers() {
    // one measure through the Phase 7 path equals the Phase 6 call
    let s = spec("Product", Some("Branch"), Some("Balance"));
    let a = micro_parquet::chart::compute_chart(book().lazy(), &s).unwrap();
    let b = compute(book().lazy(), &s).unwrap();
    assert_eq!((a.categories, a.series, a.values, a.rows, a.total_rows), (b.categories, b.series, b.values, b.rows, b.total_rows));
}

// ----------------------------------------------------------------------------------- old settings

#[test]
fn c0_an_old_phase_6_chart_setting_loads_unchanged() {
    let old = r#"{"category":"Term Period","series":null,"value":"Balance","weight":null,"agg":"Sum","kind":"Bar","order":"Natural","by_value":false,"top_n":30,"scale":"Follow"}"#;
    let s: ChartSpec = serde_json::from_str(old).unwrap();
    assert_eq!(s, ChartSpec { category: "Term Period".into(), value: Some("Balance".into()), ..Default::default() });
    assert_eq!((s.kind, s.scale, s.stack, s.top_n), (ChartKind::Bar, ChartScale::Follow, Stack::Grouped, 30));
    assert!(s.measures.is_empty() && s.panels.is_empty() && s.ref_lines.is_empty() && s.facet.is_none());
    // its one measure is the old value / aggregation
    let ml = s.measure_list();
    assert_eq!((ml.len(), ml[0].column.as_deref(), ml[0].agg), (1, Some("Balance"), ChartAgg::Sum));
    // a Line with a series and a list order
    let old2 = r#"{"category":"Day","series":"Risk","value":"Rate","weight":"Balance","agg":"WeightedAvg","kind":"Line","order":{"List":{"name":"x","values":["a"],"case_insensitive":true}},"by_value":true,"top_n":5,"scale":"Crore"}"#;
    let s2: ChartSpec = serde_json::from_str(old2).unwrap();
    assert_eq!((s2.kind, s2.agg, s2.scale, s2.top_n, s2.by_value), (ChartKind::Line, ChartAgg::WeightedAvg, ChartScale::Crore, 5, true));
    // an empty object (a damaged setting) is the default chart, and a new spec round-trips
    assert_eq!(serde_json::from_str::<ChartSpec>("{}").unwrap(), ChartSpec::default());
    let mut n = ChartSpec { category: "A".into(), kind: ChartKind::Waterfall, stack: Stack::Percent, takeaway: "x".into(), ..Default::default() };
    n.colour_map.assign_all(&["p", "q"]);
    assert_eq!(serde_json::from_str::<ChartSpec>(&serde_json::to_string(&n).unwrap()).unwrap(), n);
}

#[test]
fn c0_the_calculation_key_ignores_the_look_and_sees_the_numbers() {
    let a = spec("Product", None, Some("Balance"));
    let mut b = a.clone();
    b.title = "x".into();
    b.kind = ChartKind::Line;
    b.scale = ChartScale::Crore;
    b.colour_map.assign("TD");
    assert_eq!(a.calc_key(), b.calc_key());
    b.facet = Some("Branch".into());
    assert_ne!(a.calc_key(), b.calc_key());
    let mut c = a.clone();
    c.measures = vec![MeasureSpec { column: Some("Rate".into()), ..Default::default() }];
    assert_ne!(a.calc_key(), c.calc_key());
}
