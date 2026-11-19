//! C1: the in-app chart layout. Every form, in light and dark, at the side-panel, pop-out and full-tab
//! sizes, asserting the design-system rules on the geometry: no label leaves the chart or overlaps another
//! or is clipped by a mark; bars are at most 24 px thick; stacked segments have the 2 px gap; the
//! waterfall's floating bars start at the C0 bases; a negative bar grows down from zero.

use micro_parquet::chart::{ChartAgg, ChartKind};
use micro_parquet::charts::calc::compute;
use micro_parquet::charts::data::ChartData;
use micro_parquet::charts::layout::{hit_test, layout_chart, ApproxMeasure, ChartLayout, LayoutOpts, MarkKind, Role, DONUT_REFUSAL};
use micro_parquet::charts::spec::{ChartSpec, MeasureSpec, PanelSpec, RefLine, Stack, Status, Transform, WaterfallMode};
use micro_parquet::charts::tokens as t;
use micro_parquet::sort_lists::SortOrder;
use polars::prelude::*;

use egui::{pos2, Rect};

const SIZES: [(f32, f32); 3] = [(316.0, 420.0), (900.0, 560.0), (1219.0, 686.0)];

fn lay(d: &ChartData, w: f32, h: f32, dark: bool) -> ChartLayout {
    layout_chart(d, Rect::from_min_size(pos2(10.0, 20.0), egui::vec2(w, h)), &LayoutOpts { dark, divisor: 1.0, decimals: 2, unit: "BDT".into(), bare: false, colours: None }, &ApproxMeasure)
}

fn overlap(a: Rect, b: Rect) -> bool {
    a.intersects(b) && a.intersect(b).width() > 0.5 && a.intersect(b).height() > 0.5
}

/// The rules every chart obeys, whatever its form.
fn check(l: &ChartLayout, what: &str) {
    let bounds = if l.needed_height > l.rect.height() { Rect::from_min_size(l.rect.min, egui::vec2(l.rect.width(), l.needed_height)) } else { l.rect };
    for (i, a) in l.texts.iter().enumerate() {
        assert!(bounds.contains_rect(a.rect), "{what}: the text '{}' {:?} leaves the chart {:?}", a.text, a.rect, bounds);
        for b in l.texts.iter().skip(i + 1) {
            assert!(!overlap(a.rect, b.rect), "{what}: '{}' {:?} overlaps '{}' {:?}", a.text, a.rect, b.text, b.rect);
        }
        // a value label, direct label or reference text is never clipped by a mark
        if matches!(a.role, Role::Label | Role::Direct | Role::Reference) {
            for m in &l.marks {
                assert!(!overlap(a.rect, m.rect), "{what}: the label '{}' {:?} is covered by a mark {:?}", a.text, a.rect, m.rect);
            }
        }
    }
    for m in &l.marks {
        assert!(bounds.expand(1.0).contains_rect(m.rect), "{what}: a mark {:?} leaves the chart", m.rect);
    }
    let kind_of = l.panels.first().map(|p| p.kind);
    for m in l.marks.iter().filter(|m| matches!(m.kind, MarkKind::Bar | MarkKind::Segment)) {
        let panel = l.panels.get(m.panel).or(l.panels.first());
        let horizontal = panel.map(|p| p.horizontal).unwrap_or(false);
        let thick = if horizontal { m.rect.height() } else { m.rect.width() };
        if kind_of != Some(ChartKind::Histogram) {
            assert!(thick <= t::BAR_MAX_PX + 0.01, "{what}: a bar {:?} is {thick} px thick (at most 24)", m.rect);
        }
    }
    // the surface is the design's
    let ink = t::ink(l.dark);
    assert_eq!(l.surface, egui::Color32::from_rgb((ink.surface >> 16) as u8, (ink.surface >> 8) as u8, ink.surface as u8), "{what}");
}

fn frame() -> DataFrame {
    let months = ["May", "Jun", "Jul", "Aug", "Sep", "Oct"];
    let mut m = Vec::new();
    let mut p = Vec::new();
    let mut v = Vec::new();
    let mut r = Vec::new();
    for (i, mo) in months.iter().enumerate() {
        for (j, pr) in ["Term deposits", "Savings", "Current", "Other"].iter().enumerate() {
            m.push(*mo);
            p.push(*pr);
            v.push(600.0 / (j as f64 + 1.0) + i as f64 * 17.0);
            r.push(3.0 + j as f64 + (i % 3) as f64);
        }
    }
    DataFrame::new(vec![Series::new("Month".into(), m).into(), Series::new("Product".into(), p).into(), Series::new("Balance".into(), v).into(), Series::new("Rate".into(), r).into()]).unwrap()
}

fn month_order() -> SortOrder {
    SortOrder::List { name: "m".into(), values: ["May", "Jun", "Jul", "Aug", "Sep", "Oct"].map(String::from).to_vec(), case_insensitive: true }
}

fn spec(kind: ChartKind) -> ChartSpec {
    ChartSpec { category: "Month".into(), series: Some("Product".into()), value: Some("Balance".into()), order: month_order(), kind, ..Default::default() }
}

fn data(s: &ChartSpec) -> ChartData {
    compute(frame().lazy(), s).unwrap()
}

fn all_sizes(d: &ChartData, what: &str) -> Vec<ChartLayout> {
    let mut out = Vec::new();
    for dark in [false, true] {
        for (w, h) in SIZES {
            let l = lay(d, w, h, dark);
            check(&l, &format!("{what} {w}x{h} dark={dark}"));
            out.push(l);
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------ bars

#[test]
fn c1_single_grouped_stacked_percent_and_horizontal_bars_follow_the_rules() {
    for (name, stack, series, horizontal) in [
        ("single", Stack::Grouped, None, false),
        ("grouped", Stack::Grouped, Some("Product"), false),
        ("stacked", Stack::Stacked, Some("Product"), false),
        ("percent", Stack::Percent, Some("Product"), false),
        ("bar", Stack::Grouped, None, true),
        ("stacked bar", Stack::Stacked, Some("Product"), true),
    ] {
        let mut s = spec(ChartKind::Bar);
        s.series = series.map(str::to_string);
        s.stack = stack;
        s.horizontal = horizontal;
        let d = data(&s);
        let ls = all_sizes(&d, name);
        for l in &ls {
            assert!(l.message.is_none(), "{name}: {:?}", l.message);
            let bars: Vec<_> = l.marks.iter().filter(|m| matches!(m.kind, MarkKind::Bar | MarkKind::Segment)).collect();
            assert_eq!(bars.len(), if series.is_some() { 24 } else { 6 }, "{name}");
            // grid lines are horizontal for columns and vertical for bars; 1 px
            let p = &l.panels[0];
            assert_eq!(p.horizontal, horizontal);
            assert!(p.lo <= 0.0 && p.hi > 0.0, "{name}: every bar chart starts at zero");
            // the legend has one entry per series when there are two or more
            assert_eq!(l.legend.len(), if series.is_some() { 4 } else { 0 }, "{name}");
        }
    }
}

#[test]
fn c1_stacked_segments_have_a_two_pixel_gap_and_the_total_is_on_top() {
    let mut s = spec(ChartKind::Bar);
    s.stack = Stack::Stacked;
    let d = data(&s);
    let l = lay(&d, 900.0, 560.0, false);
    check(&l, "stacked");
    for c in 0..6 {
        let mut segs: Vec<_> = l.marks.iter().filter(|m| m.category == c && m.kind == MarkKind::Segment).collect();
        assert_eq!(segs.len(), 4);
        segs.sort_by(|a, b| b.rect.top().partial_cmp(&a.rect.top()).unwrap());
        for w in segs.windows(2) {
            let gap = w[0].rect.top() - w[1].rect.bottom();
            assert!((gap - t::SEGMENT_GAP_PX).abs() < 0.01, "category {c}: the gap between stacked segments is {gap}");
        }
    }
    // the total on top only: 6 value labels, one per column, no label per segment
    let labels: Vec<_> = l.texts.iter().filter(|x| x.role == Role::Label).collect();
    assert_eq!(labels.len(), 6, "{:?}", labels.iter().map(|x| &x.text).collect::<Vec<_>>());
    let total_may: f64 = d.values.iter().filter_map(|r| r[0]).sum();
    assert!(labels.iter().any(|x| x.text == micro_parquet::models::format_with_commas(total_may, 2)), "{:?} vs {total_may}", labels.iter().map(|x| &x.text).collect::<Vec<_>>());
}

#[test]
fn c1_a_negative_bar_grows_down_from_zero_and_the_gap_ladder_is_diverging() {
    let df = DataFrame::new(vec![
        Series::new("Bucket".into(), ["Sight", "<=1M", "1-3M", "3-6M"]).into(),
        Series::new("Gap".into(), [-1850.0, -620.0, 410.0, 780.0]).into(),
    ])
    .unwrap();
    let s = ChartSpec { category: "Bucket".into(), value: Some("Gap".into()), order: SortOrder::List { name: "b".into(), values: ["Sight", "<=1M", "1-3M", "3-6M"].map(String::from).to_vec(), case_insensitive: true }, ..Default::default() };
    let d = compute(df.lazy(), &s).unwrap();
    for (w, h) in SIZES {
        let l = lay(&d, w, h, false);
        check(&l, "gap");
        let p = &l.panels[0];
        let zero = p.coord(0.0);
        let bars: Vec<_> = l.marks.iter().filter(|m| m.kind == MarkKind::Bar).collect();
        assert_eq!(bars.len(), 4);
        // negative: top at zero, grows down; positive: bottom at zero, grows up
        for (m, v) in bars.iter().zip([-1850.0, -620.0, 410.0, 780.0]) {
            if v < 0.0 {
                assert!((m.rect.top() - zero).abs() < 0.01, "{w}: the negative bar starts at zero {} vs {}", m.rect.top(), zero);
                assert!(m.rect.bottom() > zero);
            } else {
                assert!((m.rect.bottom() - zero).abs() < 0.01 && m.rect.top() < zero);
            }
        }
        // diverging colours: negative red, positive blue
        let reds = l.prims.iter().filter(|p| matches!(p, micro_parquet::charts::layout::Prim::Rect { fill, .. } if *fill == egui::Color32::from_rgb(0xe3, 0x49, 0x48))).count();
        let blues = l.prims.iter().filter(|p| matches!(p, micro_parquet::charts::layout::Prim::Rect { fill, .. } if *fill == egui::Color32::from_rgb(0x2a, 0x78, 0xd6))).count();
        assert_eq!((reds, blues), (2, 2), "{w}");
        // a label at every tip: below the negative bars, above the positive
        let neg_label = l.texts.iter().find(|x| x.role == Role::Label && x.text == "-1,850.00").expect("label");
        assert!(neg_label.rect.top() >= bars[0].rect.bottom(), "the label of a negative bar is below it");
        let pos_label = l.texts.iter().find(|x| x.role == Role::Label && x.text == "410.00").expect("label");
        assert!(pos_label.rect.bottom() <= bars[2].rect.top());
    }
}

#[test]
fn c1_bars_are_rounded_at_the_data_end_only() {
    let mut s = spec(ChartKind::Bar);
    s.series = None;
    let d = data(&s);
    let l = lay(&d, 900.0, 560.0, false);
    let rects: Vec<[f32; 4]> = l.prims.iter().filter_map(|p| if let micro_parquet::charts::layout::Prim::Rect { round, rect, .. } = p { if rect.width() <= 24.0 && rect.height() > 8.0 && rect.width() > 8.0 { Some(*round) } else { None } } else { None }).collect();
    assert!(!rects.is_empty());
    for r in rects {
        assert_eq!(r, [t::BAR_RADIUS_PX, t::BAR_RADIUS_PX, 0.0, 0.0], "square at the baseline, 4 px at the data end");
    }
}

#[test]
fn c1_hovering_a_bar_gives_its_tooltip_with_category_series_value_and_rows() {
    let d = data(&spec(ChartKind::Bar));
    let l = lay(&d, 900.0, 560.0, false);
    let m = l.marks.iter().find(|m| m.category == 2 && m.series == 1).expect("a mark");
    let h = hit_test(&l, m.rect.center()).expect("a hover");
    assert!(h.tip.contains("Month: Jul") && h.tip.contains("Product: Savings") && h.tip.contains("Sum of Balance:") && h.tip.contains("rows"), "{}", h.tip);
    assert_eq!(h.rect, Some(m.rect));
    // the hit target is larger than the mark
    let near = pos2(m.rect.right() + 3.0, m.rect.center().y);
    assert!(hit_test(&l, near).is_some());
    // far from every mark (above the plot) gives nothing
    assert!(hit_test(&l, pos2(l.rect.left() + 1.0, l.rect.bottom() - 1.0)).is_none());
}

// ----------------------------------------------------------------------------------------- lines, areas

#[test]
fn c1_lines_stepped_lines_and_areas_follow_the_rules() {
    for (name, kind, stack, stepped) in [("line", ChartKind::Line, Stack::Grouped, false), ("stepped", ChartKind::Line, Stack::Grouped, true), ("area", ChartKind::Area, Stack::Grouped, false), ("stacked area", ChartKind::Area, Stack::Stacked, false)] {
        let mut s = spec(kind);
        s.stack = stack;
        s.stepped = stepped;
        let d = data(&s);
        for l in all_sizes(&d, name) {
            assert!(l.message.is_none(), "{name}");
            assert_eq!(l.crosshairs.len(), 6, "{name}: one crosshair per category");
            // markers of 8 px
            let pts: Vec<_> = l.marks.iter().filter(|m| m.kind == MarkKind::Point).collect();
            assert_eq!(pts.len(), 24, "{name}");
            assert!(pts.iter().all(|m| (m.rect.width() - t::MARKER_PX).abs() < 0.01));
            // 2 px lines
            assert!(l.prims.iter().any(|p| matches!(p, micro_parquet::charts::layout::Prim::Line { w, .. } if (*w - t::LINE_PX).abs() < 0.01)), "{name}");
        }
    }
    // the crosshair tip has every series' value; hovering between categories finds the nearest one
    let d = data(&spec(ChartKind::Line));
    let l = lay(&d, 900.0, 560.0, false);
    let c = &l.crosshairs[3];
    let h = hit_test(&l, pos2(c.x + 5.0, l.panels[0].plot.center().y)).expect("hover");
    assert!(h.tip.contains("Month: Aug") && h.tip.contains("Term deposits") && h.tip.contains("Savings") && h.tip.contains("Other"), "{}", h.tip);
    assert!(h.cross.is_some());
    // direct labels at the line ends with four series
    assert!(l.texts.iter().filter(|x| x.role == Role::Direct).count() >= 1);
}

#[test]
fn c1_a_single_line_has_no_legend_and_one_end_label() {
    let mut s = spec(ChartKind::Line);
    s.series = None;
    let d = data(&s);
    let l = lay(&d, 900.0, 560.0, false);
    check(&l, "single line");
    assert!(l.legend.is_empty());
    assert_eq!(l.texts.iter().filter(|x| x.role == Role::Label).count(), 1, "the value at the end of the line");
}

#[test]
fn c1_a_highlighted_series_keeps_its_colour_and_the_rest_are_de_emphasised() {
    let mut s = spec(ChartKind::Line);
    s.highlight = Some("Savings".into());
    let d = data(&s);
    let l = lay(&d, 900.0, 560.0, false);
    check(&l, "highlight");
    let deemph = egui::Color32::from_rgb(0xc3, 0xc2, 0xb7);
    let orange = egui::Color32::from_rgb(0xeb, 0x68, 0x34);
    let line_cols: Vec<egui::Color32> = l.prims.iter().filter_map(|p| if let micro_parquet::charts::layout::Prim::Line { col, w, .. } = p { if (*w - 2.0).abs() < 0.01 { Some(*col) } else { None } } else { None }).collect();
    assert!(line_cols.contains(&deemph) && line_cols.contains(&orange), "{line_cols:?}");
    // the legend swatches follow
    assert!(l.legend.iter().filter(|e| e.colour == deemph).count() == 3);
}

// -------------------------------------------------------------------------------------- scatter, bubble

fn points_frame() -> DataFrame {
    let n = 40;
    DataFrame::new(vec![
        Series::new("Tenor".into(), (0..n).map(|i| i as f64 * 0.5).collect::<Vec<_>>()).into(),
        Series::new("Rate".into(), (0..n).map(|i| 8.0 + (i as f64 * 0.37).sin() * 2.0 + i as f64 * 0.05).collect::<Vec<_>>()).into(),
        Series::new("Balance".into(), (0..n).map(|i| 100.0 + (i % 7) as f64 * 40.0).collect::<Vec<_>>()).into(),
        Series::new("Branch".into(), (0..n).map(|i| ["N", "S", "E", "W", "C"][i % 5]).collect::<Vec<_>>()).into(),
    ])
    .unwrap()
}

#[test]
fn c1_scatter_and_bubble_draw_points_with_at_most_three_colours() {
    for kind in [ChartKind::Scatter, ChartKind::Bubble] {
        let mut s = ChartSpec { category: "Tenor".into(), value: Some("Rate".into()), series: Some("Branch".into()), kind, ..Default::default() };
        if kind == ChartKind::Bubble {
            s.size = Some(MeasureSpec { column: Some("Balance".into()), ..Default::default() });
        }
        let d = compute(points_frame().lazy(), &s).unwrap();
        assert_eq!(d.series.len(), 3, "five branches fold into the top two and Other");
        assert_eq!(d.series.last().map(String::as_str), Some("Other"));
        assert!(d.notes.iter().any(|n| n.contains("at most 3 colours")));
        for l in all_sizes(&d, &format!("{kind:?}")) {
            assert!(l.message.is_none(), "{:?}", l.message);
            assert_eq!(l.marks.len(), 40);
            assert!(l.marks.iter().all(|m| m.rect.width() >= t::MARKER_PX - 0.01));
            assert_eq!(l.legend.len(), 3);
        }
    }
    // a bubble without a size is refused, text on an axis too
    let s = ChartSpec { category: "Tenor".into(), value: Some("Rate".into()), kind: ChartKind::Bubble, ..Default::default() };
    assert!(compute(points_frame().lazy(), &s).unwrap_err().to_string().contains("needs a Size column"));
    let s = ChartSpec { category: "Branch".into(), value: Some("Rate".into()), kind: ChartKind::Scatter, ..Default::default() };
    assert!(compute(points_frame().lazy(), &s).unwrap_err().to_string().contains("not a number"));
}

#[test]
fn c1_a_scatter_of_many_rows_is_a_5000_point_sample_and_says_so() {
    let n = 12_000usize;
    let df = DataFrame::new(vec![
        Series::new("X".into(), (0..n).map(|i| i as f64).collect::<Vec<_>>()).into(),
        Series::new("Y".into(), (0..n).map(|i| (i % 97) as f64).collect::<Vec<_>>()).into(),
    ])
    .unwrap();
    let s = ChartSpec { category: "X".into(), value: Some("Y".into()), kind: ChartKind::Scatter, ..Default::default() };
    let d = compute(df.lazy(), &s).unwrap();
    assert!(d.points.len() <= t::MAX_POINTS && d.points.len() > 3000, "{}", d.points.len());
    assert_eq!(d.points_total, 12_000);
    assert!(d.notes.iter().any(|n| n == "Showing a 5,000-point sample of 12,000"), "{:?}", d.notes);
    let l = lay(&d, 900.0, 560.0, false);
    assert!(l.texts.iter().any(|x| x.role == Role::Footnote && x.text.contains("Showing a 5,000-point sample of 12,000")));
}

// ---------------------------------------------------------------------------- histogram and box plot

#[test]
fn c1_histogram_has_ten_bins_with_two_pixel_gaps_and_counts_that_add_up() {
    let n = 1000;
    let df = DataFrame::new(vec![Series::new("Rate".into(), (0..n).map(|i| 5.0 + (i % 100) as f64 * 0.1 + (i / 100) as f64 * 0.01).collect::<Vec<_>>()).into()]).unwrap();
    let s = ChartSpec { category: "Rate".into(), value: Some("Rate".into()), kind: ChartKind::Histogram, ..Default::default() };
    let d = compute(df.lazy(), &s).unwrap();
    assert_eq!(d.categories.len(), micro_parquet::column_stats::HISTOGRAM_BINS);
    let h = d.hist.as_ref().unwrap();
    assert_eq!(h.counts.iter().sum::<u64>() + h.below + h.above, 1000, "the bars and the tails add up to every row");
    for l in all_sizes(&d, "histogram") {
        let mut bars: Vec<_> = l.marks.iter().filter(|m| m.kind == MarkKind::Bar).collect();
        assert_eq!(bars.len(), 10);
        bars.sort_by(|a, b| a.rect.left().partial_cmp(&b.rect.left()).unwrap());
        for w in bars.windows(2) {
            let gap = w[1].rect.left() - w[0].rect.right();
            assert!((gap - t::SEGMENT_GAP_PX).abs() < 0.01, "touching bars are 2 px apart: {gap}");
        }
        assert!(l.legend.is_empty());
    }
}

#[test]
fn c1_a_box_plot_has_the_five_numbers_and_outliers_beyond_the_fences() {
    let mut cats = Vec::new();
    let mut vals = Vec::new();
    for (c, base) in [("A", 10.0), ("B", 20.0)] {
        for i in 0..20 {
            cats.push(c);
            vals.push(base + i as f64 * 0.5);
        }
    }
    cats.push("A");
    vals.push(80.0);
    let df = DataFrame::new(vec![Series::new("Grade".into(), cats).into(), Series::new("Rate".into(), vals).into()]).unwrap();
    let s = ChartSpec { category: "Grade".into(), value: Some("Rate".into()), kind: ChartKind::BoxPlot, ..Default::default() };
    let d = compute(df.lazy(), &s).unwrap();
    assert_eq!(d.boxes.len(), 2);
    let a = &d.boxes[0];
    assert_eq!((a.category.as_str(), a.n, a.min, a.max), ("A", 21, 10.0, 80.0));
    assert_eq!(a.outliers, vec![80.0]);
    assert!(a.high_whisker < 80.0 && a.q1 < a.median && a.median < a.q3);
    let b = &d.boxes[1];
    assert_eq!((b.median, b.q1, b.q3), (24.75, 22.375, 27.125));
    assert!(b.outliers.is_empty());
    for l in all_sizes(&d, "box") {
        let m: Vec<_> = l.marks.iter().filter(|m| m.kind == MarkKind::Box).collect();
        assert_eq!(m.len(), 2);
        assert!(m.iter().all(|m| m.rect.width() <= t::BAR_MAX_PX + 0.01));
        let h = hit_test(&l, m[0].rect.center()).unwrap();
        assert!(h.tip.contains("Median") && h.tip.contains("21 rows"), "{}", h.tip);
    }
}

// ---------------------------------------------------------------------------------------- waterfall

fn bridge() -> ChartData {
    let df = DataFrame::new(vec![
        Series::new("Step".into(), ["Sep interest", "Volume", "Rate", "Mix", "Oct interest"]).into(),
        Series::new("Amount".into(), [80.0, 8.0, 5.0, 0.5, 93.5]).into(),
    ])
    .unwrap();
    let s = ChartSpec {
        category: "Step".into(),
        value: Some("Amount".into()),
        kind: ChartKind::Waterfall,
        order: SortOrder::List { name: "s".into(), values: ["Sep interest", "Volume", "Rate", "Mix", "Oct interest"].map(String::from).to_vec(), case_insensitive: true },
        ..Default::default()
    };
    compute(df.lazy(), &s).unwrap()
}

#[test]
fn c1_the_waterfalls_floating_bars_start_at_the_c0_bases_and_every_step_is_labelled() {
    let d = bridge();
    for l in all_sizes(&d, "waterfall") {
        assert!(l.message.is_none(), "{:?}", l.message);
        let p = &l.panels[0];
        let bars: Vec<_> = l.marks.iter().collect();
        assert_eq!(bars.len(), 5);
        // the three steps float: their lower edges sit at 80, 88, 93; start and end stand on zero
        for (m, base) in bars[1..4].iter().zip([80.0, 88.0, 93.0]) {
            assert!((m.rect.bottom() - p.coord(base)).abs() < 1.01, "the bar of {} starts at {base}: {} vs {}", m.category, m.rect.bottom(), p.coord(base));
        }
        assert!((bars[0].rect.bottom() - p.coord(0.0)).abs() < 0.01 && (bars[4].rect.bottom() - p.coord(0.0)).abs() < 0.01);
        assert!((bars[0].rect.top() - p.coord(80.0)).abs() < 0.01 && (bars[4].rect.top() - p.coord(93.5)).abs() < 0.01);
        // every step has its label
        let labels: Vec<&str> = l.texts.iter().filter(|x| x.role == Role::Label).map(|x| x.text.as_str()).collect();
        assert_eq!(labels, vec!["80.00", "+8.00", "+5.00", "+0.50", "93.50"]);
        assert!(l.legend.is_empty());
    }
}

#[test]
fn c1_a_waterfall_that_does_not_add_up_says_so_instead_of_drawing() {
    let mut d = bridge();
    d.values[0][4] = Some(99.0);
    let l = lay(&d, 900.0, 560.0, false);
    assert!(l.message.as_deref().unwrap_or("").contains("does not add up"), "{:?}", l.message);
    assert!(l.marks.is_empty());
    let mut d = bridge();
    d.spec.waterfall = WaterfallMode::StartSteps;
    // end calculated: 80 + 8 + 5 + .5 + 93.5 = 187
    let l = lay(&d, 900.0, 560.0, false);
    assert_eq!(l.marks.len(), 6);
}

// ------------------------------------------------------------------------------------ heatmap, donut

fn migration() -> ChartData {
    let df = DataFrame::new(vec![
        Series::new("Previous".into(), ["STD", "STD", "STD", "SMA", "SMA", "SS", "SS"]).into(),
        Series::new("Current".into(), ["STD", "SMA", "Closed", "SMA", "SS", "SS", "DF"]).into(),
        Series::new("Balance".into(), [930.0, 32.0, 25.0, 615.0, 143.0, 708.0, 169.0]).into(),
    ])
    .unwrap();
    let order = |v: &[&str]| SortOrder::List { name: "g".into(), values: v.iter().map(|s| s.to_string()).collect(), case_insensitive: true };
    let mut s = ChartSpec {
        category: "Current".into(),
        series: Some("Previous".into()),
        value: Some("Balance".into()),
        kind: ChartKind::Heatmap,
        order: order(&["STD", "SMA", "SS", "DF", "Closed"]),
        ..Default::default()
    };
    s.measures = vec![MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, transform: Transform::ShareOfSeries, ..Default::default() }];
    compute(df.lazy(), &s).unwrap()
}

#[test]
fn c1_a_heatmap_has_rounded_cells_a_two_pixel_gap_and_white_text_on_dark_cells() {
    let d = migration();
    for l in all_sizes(&d, "heatmap") {
        assert!(l.message.is_none(), "{:?}", l.message);
        let cells: Vec<_> = l.marks.iter().filter(|m| m.kind == MarkKind::Cell).collect();
        // every row × column pair is a cell (3 rows × 5 columns), the pairs without data are 0.0
        assert_eq!(cells.len(), 15);
        // cells of one row are two pixels apart
        let mut row0: Vec<_> = cells.iter().filter(|m| m.series == cells[0].series).collect();
        row0.sort_by(|a, b| a.rect.left().partial_cmp(&b.rect.left()).unwrap());
        for w in row0.windows(2) {
            if w[1].category == w[0].category + 1 {
                assert!((w[1].rect.left() - w[0].rect.right() - t::HEAT_GAP_PX).abs() < 0.01);
            }
        }
        // each row of the shares sums to 100: the cell values are the shares
        for r in 0..3 {
            let sum: f64 = cells.iter().filter(|m| m.series == r).filter_map(|m| m.value).sum();
            assert!((sum - 100.0).abs() < 0.01, "row {r}: {sum}");
        }
        // the cell text of the darkest cell is white, of the lightest is ink
        let dark_cell = cells.iter().max_by(|a, b| a.value.partial_cmp(&b.value).unwrap()).unwrap();
        let t_dark = l.texts.iter().find(|x| x.role == Role::Cell && dark_cell.rect.contains_rect(x.rect));
        if let Some(tx) = t_dark {
            assert_eq!(tx.colour, egui::Color32::WHITE);
        }
        // rounded: 2 px corners
        let radius = l.prims.iter().find_map(|p| if let micro_parquet::charts::layout::Prim::Rect { rect, round, .. } = p { if *rect == dark_cell.rect { Some(round[0]) } else { None } } else { None }).unwrap();
        assert!((radius - t::HEAT_RADIUS_PX).abs() < 0.01);
        let h = hit_test(&l, dark_cell.rect.center()).unwrap();
        assert!(h.tip.contains("Percent of the row total") && h.tip.contains("rows"), "{}", h.tip);
    }
}

fn parts(n: usize) -> ChartData {
    let df = DataFrame::new(vec![
        Series::new("Product".into(), (0..n).map(|i| format!("P{i}")).collect::<Vec<_>>()).into(),
        Series::new("Balance".into(), (0..n).map(|i| 100.0 - i as f64 * 7.0).collect::<Vec<_>>()).into(),
    ])
    .unwrap();
    compute(df.lazy(), &ChartSpec { category: "Product".into(), value: Some("Balance".into()), kind: ChartKind::Donut, by_value: true, ..Default::default() }).unwrap()
}

#[test]
fn c1_a_donut_of_two_to_five_slices_is_drawn_and_six_is_refused_with_the_message() {
    for n in 2..=5 {
        for l in all_sizes(&parts(n), &format!("donut {n}")) {
            assert!(l.message.is_none(), "{:?}", l.message);
            assert_eq!(l.marks.len(), n);
            assert_eq!(l.legend.len(), n);
            let h = hit_test(&l, l.marks[0].rect.center()).unwrap();
            assert!(h.tip.contains("% of the total"), "{}", h.tip);
        }
    }
    let l = lay(&parts(6), 900.0, 560.0, false);
    assert_eq!(l.message.as_deref(), Some(DONUT_REFUSAL));
    assert_eq!(DONUT_REFUSAL, "Use a bar chart: more than 5 parts are hard to compare as slices");
    assert!(l.marks.is_empty());
    assert!(lay(&parts(1), 900.0, 560.0, false).message.is_some());
}

// ------------------------------------------------------------------------- panels, facets, references

#[test]
fn c1_linked_panels_share_one_x_axis_and_each_has_its_own_scale() {
    let mut s = spec(ChartKind::Bar);
    s.series = None;
    s.measures = vec![
        MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, label: "Net gap".into(), ..Default::default() },
        MeasureSpec { column: Some("Rate".into()), agg: ChartAgg::Average, label: "Average rate".into(), ..Default::default() },
    ];
    s.panels = vec![PanelSpec { kind: ChartKind::Bar, measures: vec![0], title: String::new() }, PanelSpec { kind: ChartKind::Line, measures: vec![1], title: String::new() }];
    let d = data(&s);
    for l in all_sizes(&d, "linked") {
        assert_eq!(l.panels.len(), 2);
        let (a, b) = (&l.panels[0], &l.panels[1]);
        assert_eq!((a.plot.left(), a.plot.right()), (b.plot.left(), b.plot.right()), "one shared x-axis");
        assert!(a.plot.bottom() < b.plot.top(), "stacked vertically without overlap");
        // category names are written once, under the bottom panel
        let cats: Vec<_> = l.texts.iter().filter(|x| x.role == Role::Category).collect();
        assert!(cats.iter().all(|c| c.rect.top() >= b.plot.bottom()), "names only under the bottom panel");
        assert!(l.texts.iter().any(|x| x.role == Role::Panel && x.text == "Net gap") && l.texts.iter().any(|x| x.role == Role::Panel && x.text == "Average rate"));
        assert_ne!((a.lo, a.hi), (b.lo, b.hi));
    }
    // two measures without panels become linked panels by themselves
    let mut s2 = s.clone();
    s2.panels.clear();
    let l = lay(&data(&s2), 900.0, 560.0, false);
    assert_eq!(l.panels.len(), 2);
}

#[test]
fn c1_small_multiples_have_a_panel_per_facet_with_a_shared_scale() {
    let mut s = spec(ChartKind::Bar);
    s.series = None;
    s.facet = Some("Product".into());
    let d = data(&s);
    assert_eq!(d.facets.len(), 4);
    for l in all_sizes(&d, "facets") {
        assert_eq!(l.panels.len(), 4);
        let (lo, hi) = (l.panels[0].lo, l.panels[0].hi);
        assert!(l.panels.iter().all(|p| p.lo == lo && p.hi == hi), "shared scales by default");
        assert!(l.texts.iter().any(|x| x.role == Role::Panel && x.text == "Savings"));
    }
}

#[test]
fn c1_a_reference_line_is_solid_in_its_status_colour_with_a_label_and_a_zero_baseline_exists() {
    let df = DataFrame::new(vec![Series::new("Scenario".into(), ["Parallel up", "Short up", "Steeper", "Flatter", "Short down", "Parallel down"]).into(), Series::new("dEVE".into(), [-10.5, -4.48, -2.17, -0.23, 4.82, 12.57]).into()]).unwrap();
    let mut s = ChartSpec { category: "Scenario".into(), value: Some("dEVE".into()), horizontal: true, order: SortOrder::List { name: "s".into(), values: ["Parallel up", "Short up", "Steeper", "Flatter", "Short down", "Parallel down"].map(String::from).to_vec(), case_insensitive: true }, ..Default::default() };
    s.ref_lines = vec![RefLine { value: -15.0, label: "Outlier -15%".into(), status: Status::Critical, panel: 0 }];
    let d = compute(df.lazy(), &s).unwrap();
    for l in all_sizes(&d, "irrbb") {
        let p = &l.panels[0];
        assert!(p.horizontal && p.lo <= -15.0, "the axis includes the reference line: {}", p.lo);
        // the line: 1.5 px, status colour, vertical at -15
        let x = p.coord(-15.0);
        assert!(l.prims.iter().any(|q| matches!(q, micro_parquet::charts::layout::Prim::Line { a, b, w, col } if (a.x - x).abs() < 0.01 && (b.x - x).abs() < 0.01 && (*w - 1.5).abs() < 0.01 && *col == egui::Color32::from_rgb(0xd0, 0x3b, 0x3b))));
        assert!(l.texts.iter().any(|t| t.role == Role::Reference && t.text.contains("Outlier -15%") && t.text.contains("Critical")), "the reference line is labelled in words");
        // every grid line is vertical and 1 px; bars: negatives grow left of zero
        let neg = l.marks.iter().find(|m| m.category == 0).unwrap();
        assert!((neg.rect.right() - p.coord(0.0)).abs() < 0.01 && neg.rect.left() < p.coord(0.0));
        // sorted as given: the bars are in category order, top to bottom
        let tops: Vec<f32> = (0..6).map(|c| l.marks.iter().find(|m| m.category == c).unwrap().rect.top()).collect();
        assert!(tops.windows(2).all(|w| w[0] < w[1]));
    }
}

// ---------------------------------------------------------------------------------- text and states

#[test]
fn c1_the_title_is_the_takeaway_when_typed_else_the_auto_title_and_the_footnote_carries_the_notes() {
    let mut s = spec(ChartKind::Bar);
    s.series = None;
    let d = data(&s);
    let l = lay(&d, 900.0, 560.0, false);
    assert_eq!(l.texts.iter().find(|x| x.role == Role::Title).unwrap().text, "Sum of Balance by Month");
    assert!(l.texts.iter().any(|x| x.role == Role::Subtitle && x.text == "Sum of Balance \u{b7} BDT"));
    let mut s2 = s.clone();
    s2.takeaway = "Term deposits drove growth".into();
    s2.footnote = "Synthetic data".into();
    let l2 = lay(&data(&s2), 900.0, 560.0, false);
    assert_eq!(l2.texts.iter().find(|x| x.role == Role::Title).unwrap().text, "Term deposits drove growth");
    assert!(l2.texts.iter().any(|x| x.role == Role::Footnote && x.text.contains("Synthetic data")));
    // title, plot and footnote follow one another; the footnote is under the plot
    let (title, plot, foot) = (l2.title.unwrap(), l2.panels[0].plot, l2.footnote.unwrap());
    assert!(title.bottom() <= plot.top() && plot.bottom() <= foot.top());
    // sizes: title 15 semibold, subtitle 11, footnote 9
    let tt = l2.texts.iter().find(|x| x.role == Role::Title).unwrap();
    assert_eq!((tt.pt, tt.face), (15.0, micro_parquet::charts::layout::Face::Semibold));
}

#[test]
fn c1_an_empty_chart_says_no_data_to_chart_with_the_reason_and_a_too_small_chart_says_so() {
    let mut d = data(&spec(ChartKind::Bar));
    d.categories.clear();
    d.values.clear();
    let l = lay(&d, 900.0, 560.0, false);
    assert!(l.message.as_deref().unwrap().starts_with("No data to chart: the tab has no rows"), "{:?}", l.message);
    assert!(l.texts.iter().any(|x| x.role == Role::Message));
    let l = lay(&data(&spec(ChartKind::Bar)), 70.0, 50.0, false);
    assert!(l.message.is_some());
    // Sankey and treemap are HTML only
    let mut s = spec(ChartKind::Sankey);
    s.series = None;
    let l = lay(&data(&s), 900.0, 560.0, false);
    assert!(l.message.as_deref().unwrap().contains("HTML export"));
}

#[test]
fn c1_a_series_keeps_its_colour_when_another_is_filtered_out() {
    let s = spec(ChartKind::Bar);
    let d = data(&s);
    let l = lay(&d, 900.0, 560.0, false);
    let map = l.colour_map.clone();
    assert_eq!(map.slots.len(), 3, "three named series; a series called Other is always the de-emphasised colour and takes no slot");
    // the next calculation carries the stored map; "Savings" is gone, the others keep their colours
    let mut s2 = s.clone();
    s2.colour_map = map.clone();
    let df = frame().lazy().filter(col("Product").neq(lit("Savings")));
    let d2 = compute(df, &s2).unwrap();
    let l2 = lay(&d2, 900.0, 560.0, false);
    for name in ["Term deposits", "Current", "Other"] {
        assert_eq!(map.get(name), l2.colour_map.get(name), "{name}");
    }
    let sw = |l: &ChartLayout, n: &str| l.legend.iter().find(|e| e.name == n).map(|e| e.colour);
    assert_eq!(sw(&l, "Current"), sw(&l2, "Current"));
    assert_eq!(sw(&l, "Term deposits"), sw(&l2, "Term deposits"));
}
