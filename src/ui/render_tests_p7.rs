//! Phase 7 UI tests: the chart tokens in `theme.rs`, and (from C1) the headless render tests of the
//! chart renderer at 1219×686, 1600×900 and 1291×700 pt.

use crate::ui::theme;
use eframe::egui::Color32;

fn rgb(c: Color32) -> u32 {
    ((c.r() as u32) << 16) | ((c.g() as u32) << 8) | c.b() as u32
}

#[test]
fn c0_the_theme_tokens_carry_every_hex_value_of_the_design_system() {
    let l = theme::CHART_LIGHT;
    assert_eq!([l.surface, l.ink, l.ink2, l.muted, l.grid, l.baseline, l.deemph].map(rgb), [0xfcfcfb, 0x0b0b0b, 0x52514e, 0x898781, 0xe1e0d9, 0xc3c2b7, 0xc3c2b7]);
    let d = theme::CHART_DARK;
    assert_eq!([d.surface, d.ink, d.ink2, d.muted, d.grid, d.baseline, d.deemph].map(rgb), [0x1a1a19, 0xf0efec, 0xc3c2b7, 0x898781, 0x2c2c2a, 0x383835, 0x55544f]);
    assert_eq!(theme::CHART_SERIES_LIGHT.map(rgb), [0x2a78d6, 0xeb6834, 0x1baf7a, 0xeda100, 0xe87ba4, 0x008300, 0x6250d6, 0xe34948]);
    assert_eq!(theme::CHART_SERIES_DARK.map(rgb), [0x3987e5, 0xd95926, 0x199e70, 0xc98500, 0xd55181, 0x0ca30c, 0x9085e9, 0xe66767]);
    assert_eq!(
        theme::CHART_SEQUENTIAL.map(rgb),
        [0xcde2fb, 0xb7d3f6, 0x9ec5f4, 0x86b6ef, 0x6da7ec, 0x5598e7, 0x3987e5, 0x2a78d6, 0x256abf, 0x1c5cab, 0x184f95, 0x104281, 0x0d366b]
    );
    assert_eq!([theme::CHART_DIVERGING_POSITIVE, theme::CHART_DIVERGING_NEGATIVE, theme::CHART_DIVERGING_ZERO].map(rgb), [0x2a78d6, 0xe34948, 0xc3c2b7]);
    assert_eq!([theme::CHART_STATUS_GOOD, theme::CHART_STATUS_WARNING, theme::CHART_STATUS_SERIOUS, theme::CHART_STATUS_CRITICAL].map(rgb), [0x0ca30c, 0xfab219, 0xec835a, 0xd03b3b]);
    // the sizes
    assert_eq!((theme::CHART_TITLE_PT, theme::CHART_SUBTITLE_PT, theme::CHART_TICK_PT, theme::CHART_FOOTNOTE_PT), (15.0, 11.0, 10.0, 9.0));
    assert_eq!((theme::CHART_BAR_MAX_PX, theme::CHART_BAR_RADIUS_PX, theme::CHART_SEGMENT_GAP_PX, theme::CHART_MIN_SPACE_PX), (24.0, 4.0, 2.0, 8.0));
    // a slot beyond the eight is the de-emphasised colour
    assert_eq!(theme::chart_series_colour(8, false), theme::CHART_LIGHT.deemph);
    assert_eq!(theme::chart_series_colour(1, true), theme::CHART_SERIES_DARK[1]);
}

// ================================================================================ C1: the renderer

use crate::ui::chart_render::{last_shown, show_chart, ChartShown};
use crate::ui::layout_tests::collect_painted;
use crate::ui::render_tests::screen;
use micro_parquet::chart::{ChartAgg, ChartKind};
use micro_parquet::charts::calc::compute;
use micro_parquet::charts::data::ChartData;
use micro_parquet::charts::layout::{LayoutOpts, MarkKind, Role};
use micro_parquet::charts::spec::{ChartSpec, MeasureSpec, PanelSpec, RefLine, Stack, Status, Transform};
use micro_parquet::charts::tokens as ct;
use micro_parquet::sort_lists::SortOrder;
use polars::prelude::*;

/// The three window sizes of the spec, in points: 1219×686, 1600×900 and 1291×700.
pub(crate) const SIZES3: [(f32, f32, f32); 3] = [(1.05, 1219.05, 685.71), (0.8, 1600.0, 900.0), (1.05, 1291.0, 700.0)];

fn book() -> DataFrame {
    let months = ["May", "Jun", "Jul", "Aug", "Sep", "Oct"];
    let (mut m, mut p, mut v, mut r) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (i, mo) in months.iter().enumerate() {
        for (j, pr) in ["Term deposits", "Savings", "Current", "Other"].iter().enumerate() {
            m.push(*mo);
            p.push(*pr);
            v.push(600.0 / (j as f64 + 1.0) + i as f64 * 17.0);
            r.push(3.0 + j as f64 + (i % 3) as f64);
        }
    }
    DataFrame::new(vec![Series::new("Month".into(), m).into(), Series::new("Product".into(), p).into(), Series::new("Balance".into(), v).into(), Series::new("Rate".into(), r).into()]).expect("book")
}

fn list(values: &[&str]) -> SortOrder {
    SortOrder::List { name: "o".into(), values: values.iter().map(|s| s.to_string()).collect(), case_insensitive: true }
}

fn months() -> SortOrder {
    list(&["May", "Jun", "Jul", "Aug", "Sep", "Oct"])
}

fn base(kind: ChartKind) -> ChartSpec {
    ChartSpec { category: "Month".into(), series: Some("Product".into()), value: Some("Balance".into()), order: months(), kind, ..Default::default() }
}

fn calc(df: DataFrame, s: &ChartSpec) -> ChartData {
    compute(df.lazy(), s).expect("the chart is calculated")
}

/// Every form of the chart studio, with data (name, data).
pub(crate) fn all_forms() -> Vec<(&'static str, ChartData)> {
    let mut v: Vec<(&'static str, ChartData)> = Vec::new();
    let single = |mut s: ChartSpec| {
        s.series = None;
        s
    };
    v.push(("column", calc(book(), &single(base(ChartKind::Bar)))));
    v.push(("grouped", calc(book(), &base(ChartKind::Bar))));
    v.push(("stacked", calc(book(), &ChartSpec { stack: Stack::Stacked, ..base(ChartKind::Bar) })));
    v.push(("percent", calc(book(), &ChartSpec { stack: Stack::Percent, ..base(ChartKind::Bar) })));
    v.push(("bar", calc(book(), &ChartSpec { horizontal: true, by_value: true, ..single(base(ChartKind::Bar)) })));
    v.push(("line", calc(book(), &base(ChartKind::Line))));
    v.push(("stepped", calc(book(), &ChartSpec { stepped: true, ..base(ChartKind::Line) })));
    v.push(("area", calc(book(), &single(base(ChartKind::Area)))));
    v.push(("stacked-area", calc(book(), &ChartSpec { stack: Stack::Stacked, ..base(ChartKind::Area) })));
    let n = 60usize;
    let pts = DataFrame::new(vec![
        Series::new("Tenor".into(), (0..n).map(|i| i as f64 * 0.5).collect::<Vec<_>>()).into(),
        Series::new("Rate".into(), (0..n).map(|i| 8.0 + (i as f64 * 0.37).sin() * 2.0 + i as f64 * 0.05).collect::<Vec<_>>()).into(),
        Series::new("Size".into(), (0..n).map(|i| 100.0 + (i % 7) as f64 * 40.0).collect::<Vec<_>>()).into(),
        Series::new("Branch".into(), (0..n).map(|i| ["N", "S", "E"][i % 3]).collect::<Vec<_>>()).into(),
    ])
    .expect("points");
    let sc = ChartSpec { category: "Tenor".into(), value: Some("Rate".into()), series: Some("Branch".into()), kind: ChartKind::Scatter, ..Default::default() };
    v.push(("scatter", calc(pts.clone(), &sc)));
    v.push(("bubble", calc(pts, &ChartSpec { kind: ChartKind::Bubble, size: Some(MeasureSpec { column: Some("Size".into()), ..Default::default() }), ..sc })));
    let hist = DataFrame::new(vec![Series::new("Rate".into(), (0..1000).map(|i| 5.0 + (i % 100) as f64 * 0.1 + (i / 100) as f64 * 0.01).collect::<Vec<_>>()).into()]).expect("hist");
    v.push(("histogram", calc(hist, &ChartSpec { category: "Rate".into(), value: Some("Rate".into()), kind: ChartKind::Histogram, ..Default::default() })));
    let (mut c, mut x) = (Vec::new(), Vec::new());
    for (g, b) in [("STD", 10.0), ("SMA", 14.0), ("SS", 20.0)] {
        for i in 0..24 {
            c.push(g);
            x.push(b + i as f64 * 0.4);
        }
    }
    c.push("STD");
    x.push(40.0);
    let boxes = DataFrame::new(vec![Series::new("Grade".into(), c).into(), Series::new("Rate".into(), x).into()]).expect("boxes");
    v.push(("box", calc(boxes, &ChartSpec { category: "Grade".into(), value: Some("Rate".into()), kind: ChartKind::BoxPlot, order: list(&["STD", "SMA", "SS"]), ..Default::default() })));
    let steps = ["Sep interest", "Volume", "Rate", "Mix", "Oct interest"];
    let bridge = DataFrame::new(vec![Series::new("Step".into(), steps).into(), Series::new("Amount".into(), [80.0, 8.0, 5.0, 0.5, 93.5]).into()]).expect("bridge");
    v.push(("waterfall", calc(bridge, &ChartSpec { category: "Step".into(), value: Some("Amount".into()), kind: ChartKind::Waterfall, order: list(&steps), ..Default::default() })));
    let mig = DataFrame::new(vec![
        Series::new("Previous".into(), ["STD", "STD", "STD", "SMA", "SMA", "SS", "SS"]).into(),
        Series::new("Current".into(), ["STD", "SMA", "Closed", "SMA", "SS", "SS", "DF"]).into(),
        Series::new("Balance".into(), [930.0, 32.0, 25.0, 615.0, 143.0, 708.0, 169.0]).into(),
    ])
    .expect("migration");
    let mut hs = ChartSpec { category: "Current".into(), series: Some("Previous".into()), value: Some("Balance".into()), kind: ChartKind::Heatmap, order: list(&["STD", "SMA", "SS", "DF", "Closed"]), ..Default::default() };
    hs.measures = vec![MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, transform: Transform::ShareOfSeries, ..Default::default() }];
    v.push(("heatmap", calc(mig, &hs)));
    let donut = DataFrame::new(vec![Series::new("Product".into(), ["Term deposits", "Savings", "Current", "Other"]).into(), Series::new("Balance".into(), [650.0, 220.0, 90.0, 40.0]).into()]).expect("donut");
    v.push(("donut", calc(donut, &ChartSpec { category: "Product".into(), value: Some("Balance".into()), kind: ChartKind::Donut, by_value: true, ..Default::default() })));
    let mut lp = single(base(ChartKind::Bar));
    lp.measures = vec![
        MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, label: "Balance".into(), ..Default::default() },
        MeasureSpec { column: Some("Rate".into()), agg: ChartAgg::Average, label: "Average rate".into(), ..Default::default() },
    ];
    lp.panels = vec![PanelSpec { kind: ChartKind::Bar, measures: vec![0], title: String::new() }, PanelSpec { kind: ChartKind::Line, measures: vec![1], title: String::new() }];
    v.push(("linked-panels", calc(book(), &lp)));
    v.push(("small-multiples", calc(book(), &ChartSpec { facet: Some("Product".into()), ..single(base(ChartKind::Bar)) })));
    let gaps = DataFrame::new(vec![
        Series::new("Bucket".into(), ["Sight", "<=1M", "1-3M", "3-6M", "6-12M", "1-5Y"]).into(),
        Series::new("Gap".into(), [-1850.0, -620.0, 410.0, 780.0, 920.0, 2600.0]).into(),
    ])
    .expect("gaps");
    v.push(("gap-ladder", calc(gaps, &ChartSpec { category: "Bucket".into(), value: Some("Gap".into()), order: list(&["Sight", "<=1M", "1-3M", "3-6M", "6-12M", "1-5Y"]), ..Default::default() })));
    let sc6 = ["Parallel up", "Short up", "Steepener", "Flattener", "Short down", "Parallel down"];
    let eve = DataFrame::new(vec![Series::new("Scenario".into(), sc6).into(), Series::new("dEVE".into(), [-10.5, -4.48, -2.17, -0.23, 4.82, 12.57]).into()]).expect("eve");
    let mut es = ChartSpec { category: "Scenario".into(), value: Some("dEVE".into()), horizontal: true, order: list(&sc6), ..Default::default() };
    es.ref_lines = vec![RefLine { value: -15.0, label: "Outlier -15%".into(), status: Status::Critical, panel: 0 }];
    v.push(("irrbb-reference", calc(eve, &es)));
    v
}

pub(crate) fn opts(dark: bool) -> LayoutOpts {
    LayoutOpts { dark, divisor: 1.0, decimals: 2, unit: "BDT Crore".into(), bare: false, colours: None }
}

/// One frame of the chart in a window-filling panel; `events` are pointer events for this frame.
pub(crate) fn draw_chart(ctx: &egui::Context, input: &egui::RawInput, data: &ChartData, dark: bool, events: Vec<egui::Event>) -> (ChartShown, crate::ui::layout_tests::Painted) {
    let mut i = input.clone();
    i.events = events;
    let mut shown = None;
    let out = ctx.run(i, |c| {
        egui::CentralPanel::default().frame(egui::Frame::none().inner_margin(egui::Margin::same(ct::MIN_SPACE_PX))).show(c, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let h = (ui.available_height() - ct::MIN_SPACE_PX * 2.0).max(ct::MIN_SPACE_PX);
                shown = Some(show_chart(ui, egui::Id::new("p7_chart"), data, &opts(dark), Some(h)));
            });
        });
    });
    (shown.expect("the chart was drawn"), collect_painted(&out.shapes))
}

fn overlap(a: egui::Rect, b: egui::Rect) -> bool {
    a.intersects(b) && a.intersect(b).width() > 0.5 && a.intersect(b).height() > 0.5
}

/// The rules of the design system, on what the real fonts and egui laid out.
pub(crate) fn assert_rules(s: &ChartShown, window: egui::Rect, what: &str) {
    let l = &s.layout;
    let bounds = egui::Rect::from_min_size(l.rect.min, egui::vec2(l.rect.width(), l.needed_height.max(l.rect.height())));
    assert!(l.rect.min.x >= window.min.x - 0.5 && l.rect.max.x <= window.max.x + 0.5, "{what}: the chart {:?} is inside the window {window:?} (nothing overflows)", l.rect);
    for (i, a) in l.texts.iter().enumerate() {
        assert!(bounds.contains_rect(a.rect), "{what}: the text '{}' {:?} leaves the chart {bounds:?}", a.text, a.rect);
        for b in l.texts.iter().skip(i + 1) {
            assert!(!overlap(a.rect, b.rect), "{what}: '{}' {:?} overlaps '{}' {:?}", a.text, a.rect, b.text, b.rect);
        }
        if matches!(a.role, Role::Label | Role::Direct | Role::Reference) {
            for m in &l.marks {
                assert!(!overlap(a.rect, m.rect), "{what}: '{}' {:?} is clipped by a mark {:?}", a.text, a.rect, m.rect);
            }
        }
    }
    let hist = l.panels.first().map(|p| p.kind) == Some(ChartKind::Histogram);
    for m in l.marks.iter().filter(|m| matches!(m.kind, MarkKind::Bar | MarkKind::Segment)) {
        let horizontal = l.panels.get(m.panel).map(|p| p.horizontal).unwrap_or(false);
        let thick = if horizontal { m.rect.height() } else { m.rect.width() };
        assert!(hist || thick <= ct::BAR_MAX_PX + 0.01, "{what}: a bar {:?} is {thick} px thick", m.rect);
    }
}

#[test]
fn c1_every_form_renders_in_light_and_dark_at_the_three_window_sizes_by_the_design_rules() {
    for (ppp, w, h) in SIZES3 {
        for (name, data) in all_forms() {
            for dark in [false, true] {
                let (ctx, input) = screen(ppp, w, h);
                let what = format!("{name} {w}x{h} dark={dark}");
                // two frames: the second has settled fonts and layout
                let _ = draw_chart(&ctx, &input, &data, dark, vec![]);
                let (shown, painted) = draw_chart(&ctx, &input, &data, dark, vec![]);
                assert!(shown.layout.message.is_none(), "{what}: {:?}", shown.layout.message);
                assert_rules(&shown, ctx.screen_rect(), &what);
                // the title and the surface really were painted
                assert!(painted.boxes.iter().any(|(t, _)| *t == shown.layout.texts.iter().find(|x| x.role == Role::Title).map(|x| x.text.clone()).unwrap_or_default()), "{what}: the title was painted");
                let full = egui::Rect::from_min_size(shown.layout.rect.min, egui::vec2(shown.layout.rect.width(), shown.layout.needed_height.max(shown.layout.rect.height())));
                assert!(painted.rects.iter().any(|(r, c)| *r == full && *c == shown.layout.surface), "{what}: the chart surface was painted");
            }
        }
    }
}

#[test]
fn c1_hovering_a_bar_cell_or_slice_by_its_real_rect_shows_its_tooltip_and_a_line_shows_a_crosshair() {
    for (ppp, w, h) in SIZES3 {
        for (name, data) in all_forms() {
            if matches!(name, "scatter" | "bubble") {
                continue;
            }
            let (ctx, input) = screen(ppp, w, h);
            let _ = draw_chart(&ctx, &input, &data, false, vec![]);
            let (shown, _) = draw_chart(&ctx, &input, &data, false, vec![]);
            let what = format!("{name} {w}x{h}");
            let target = shown.layout.marks.first().map(|m| (m.rect.center(), m.tip.clone())).or_else(|| shown.layout.crosshairs.first().map(|c| (egui::pos2(c.x, (c.y0 + c.y1) / 2.0), c.tip.clone())));
            let Some((pos, tip)) = target else { panic!("{what}: nothing to hover") };
            let ev = vec![egui::Event::PointerMoved(pos)];
            let _ = draw_chart(&ctx, &input, &data, false, ev.clone());
            let (s2, painted) = draw_chart(&ctx, &input, &data, false, ev);
            let hv = s2.hover.clone().unwrap_or_else(|| panic!("{what}: hovering {pos:?} shows nothing"));
            assert!(hv.tip.lines().count() >= 2, "{what}: {}", hv.tip);
            assert!(painted.boxes.iter().any(|(t, _)| *t == hv.tip), "{what}: the tooltip text was painted: {:?}", hv.tip);
            let tb = s2.tooltip.unwrap_or_else(|| panic!("{what}: a tooltip box"));
            assert!(tb.left() >= ctx.screen_rect().left() - 0.5 && tb.right() <= ctx.screen_rect().right() + 0.5, "{what}: the tooltip {tb:?} is inside the window");
            let _ = tip;
        }
    }
}

#[test]
fn c1_the_screenshots_of_every_form_are_written_when_python_with_pillow_is_there() {
    // one PNG per form (light) at 1219×686, and the dark ones of three forms, under docs/guide/img/charts/
    let out = std::path::Path::new("docs/guide/img/charts");
    let py = std::process::Command::new("python3").args(["-c", "import PIL"]).output();
    if !py.map(|o| o.status.success()).unwrap_or(false) {
        eprintln!("skipped: python3 with Pillow is not available, so no chart screenshots were written");
        return;
    }
    std::fs::create_dir_all(out).expect("the screenshot folder");
    let tmp = std::env::temp_dir().join(format!("mp_p7_shots_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("tmp");
    let (ppp, w, h) = SIZES3[0];
    for (name, data) in all_forms() {
        for dark in [false, true] {
            if dark && !matches!(name, "stacked" | "heatmap" | "irrbb-reference") {
                continue;
            }
            let (ctx, input) = screen(ppp, w, h);
            let _ = draw_chart(&ctx, &input, &data, dark, vec![]);
            let (shown, _) = draw_chart(&ctx, &input, &data, dark, vec![]);
            let json = tmp.join(format!("{name}.json"));
            std::fs::write(&json, serde_json::to_string(&shown.layout.to_json()).expect("json")).expect("write json");
            let png = out.join(format!("{name}{}.png", if dark { "-dark" } else { "" }));
            let r = std::process::Command::new("python3").args(["-I", "tools/chart_layout_png.py"]).arg(&json).arg(&png).output().expect("python3 runs");
            assert!(r.status.success(), "{name}: {}", String::from_utf8_lossy(&r.stderr));
            assert!(png.metadata().map(|m| m.len() > 1000).unwrap_or(false), "{name}: the PNG has content");
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    let _ = last_shown;
}

// ------------------------------------------------------------------- Table toggle and the Chart tab

use crate::ui::render_tests_p4::Rig;
use crate::ui::tests::{open_tab as _open_tab_unused, parquet_source, test_app};
use std::collections::HashMap;

fn chart_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    micro_parquet::rules::set_data_dir_override(Some(app.paths.root_dir.clone()));
    let root = app.paths.root_dir.clone();
    let n = 250usize;
    let tenors = ["6m", "1y", "1m", "100D", "3m"];
    let df = DataFrame::new(vec![
        Series::new("Term Period".into(), (0..n).map(|i| tenors[i % 5].to_string()).collect::<Vec<_>>()).into(),
        Series::new("Risk".into(), (0..n).map(|i| ["High", "Medium", "Low"][i % 3].to_string()).collect::<Vec<_>>()).into(),
        Series::new("Balance".into(), (0..n).map(|i| 10_000_000.0 + i as f64 * 1_000_000.0).collect::<Vec<f64>>()).into(),
    ])
    .expect("frame");
    let src = parquet_source(&root, "book", df);
    app.create_tab_from_source("book".into(), src, vec![], HashMap::new());
    crate::ui::tests::settle(&mut app);
    app.set_side_tab_visible(crate::ui::steps::RightTab::Chart, true);
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

fn field(rig: &Rig, name: &str) -> egui::Rect {
    rig.app.chart.rects.fields.iter().find(|(n, _)| n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: the control '{name}'", rig.what))
}

#[test]
fn c1_the_table_toggle_and_open_as_chart_tab_work_when_their_real_rects_are_clicked() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = chart_rig("p7c1tab", ppp, w, h);
        let what = rig.what.clone();
        rig.draw(10);
        assert!(rig.app.chart.rects.plot.is_some(), "{what}: the chart is drawn");
        // Table toggle: the chart's data as a table (headers painted), then back
        rig.click(field(&rig, "Table").center());
        assert!(rig.app.chart.show_table, "{what}: the Table toggle was clicked");
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t == "Term Period") && p.boxes.iter().any(|(t, _)| t == "Rows"), "{what}: the table has its headers: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).take(40).collect::<Vec<_>>());
        rig.click(field(&rig, "Table").center());
        assert!(!rig.app.chart.show_table, "{what}: back to the chart");
        rig.draw(3);
        // Open as chart tab: a second tab that fills the main area with the chart
        let tabs_before = rig.app.tabs.len();
        rig.click(field(&rig, "Open as chart tab").center());
        rig.draw(10);
        assert_eq!(rig.app.tabs.len(), tabs_before + 1, "{what}: a tab was added");
        assert!(rig.app.active_is_chart_tab(), "{what}: it is the active tab");
        assert_eq!(rig.app.chart_tabs.len(), 1, "{what}");
        let id = rig.app.tabs[rig.app.active_tab].id;
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t.starts_with("Chart: ")), "{what}: the tab is listed in the tab strip");
        let shown = last_shown(&rig.ctx, egui::Id::new(("chart_tab", id))).unwrap_or_else(|| panic!("{what}: the chart fills the main area"));
        assert_rules(&shown, rig.ctx.screen_rect(), &what);
        assert!(shown.rect.width() > 500.0 || w < 1000.0, "{what}: the chart tab is full size, not the side panel's: {:?}", shown.rect);
        assert!(shown.layout.texts.iter().any(|t| t.role == Role::Title && t.text.starts_with("Sum of Balance by Term Period")), "{what}");
        // its Table toggle (a real click on the button's text)
        let (_, tb) = p.boxes.iter().find(|(t, _)| t.ends_with("  Table")).cloned().unwrap_or_else(|| panic!("{what}: the Table button of the chart tab"));
        rig.click(tb.center());
        assert!(rig.app.chart_tabs[&id].show_table, "{what}: the chart tab's Table toggle");
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t == "Rows"), "{what}: the table of the chart tab");
        // the chart is kept with the session, and closing the tab forgets the chart
        assert_eq!(rig.app.settings.chart_tabs.len(), 1, "{what}");
        let at = rig.app.active_tab;
        rig.app.close_tab(at);
        assert!(rig.app.chart_tabs.is_empty(), "{what}: closed");
    }
    micro_parquet::rules::set_data_dir_override(None);
}
