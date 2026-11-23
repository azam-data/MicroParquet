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
    lp.panels = vec![PanelSpec { kind: ChartKind::Bar, measures: vec![0], title: String::new(), single: false }, PanelSpec { kind: ChartKind::Line, measures: vec![1], title: String::new(), single: false }];
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
    let mut forms = all_forms();
    forms.extend(template_forms());
    forms.extend(label_forms());
    for (name, data) in forms {
        for dark in [false, true] {
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

/// The pictures for the data labels, the donut and the category labels of the guide (final fixes B, item 11).
fn label_forms() -> Vec<(&'static str, ChartData)> {
    use micro_parquet::charts::spec::{LabelMode, LabelSpec};
    let mut v: Vec<(&'static str, ChartData)> = Vec::new();
    // Labels = All and None on the same grouped columns: All prints every label that fits and leaves the rest out
    for (name, mode) in [("labels-all", LabelMode::All), ("labels-none", LabelMode::None)] {
        let spec = ChartSpec { labels: LabelSpec { mode, ..Default::default() }, ..base(ChartKind::Bar) };
        v.push((name, calc(book(), &spec)));
    }
    // a donut of nine products, every part with its label outside the ring; and the same grouped to Top 4 + Other
    let products = ["Term deposits", "Savings", "Current", "DPS", "FDR", "Call money", "Foreign currency", "Sundry", "Other"];
    let shares = [310.0, 220.0, 150.0, 90.0, 70.0, 55.0, 40.0, 25.0, 12.0];
    let donut = DataFrame::new(vec![Series::new("Product".into(), products).into(), Series::new("Balance".into(), shares).into()]).expect("donut");
    let spec = ChartSpec { category: "Product".into(), value: Some("Balance".into()), kind: ChartKind::Donut, by_value: true, ..Default::default() };
    v.push(("donut-9", calc(donut.clone(), &spec)));
    v.push(("donut-top4", calc(donut, &ChartSpec { donut_top: Some(4), ..spec })));
    // ten maturity buckets: every category keeps its label (two lines, or angled when two lines do not fit)
    let buckets = ["0\u{2013}1 month", "1\u{2013}3 months", "3\u{2013}6 months", "6\u{2013}12 months", "1\u{2013}2 years", "2\u{2013}3 years", "3\u{2013}5 years", "5\u{2013}7 years", "7\u{2013}10 years", "Over 10 years"];
    let df = DataFrame::new(vec![Series::new("Bucket".into(), buckets).into(), Series::new("Balance".into(), (0..10).map(|i| 100.0 + i as f64 * 37.0).collect::<Vec<_>>()).into()]).expect("buckets");
    let spec = ChartSpec { category: "Bucket".into(), value: Some("Balance".into()), kind: ChartKind::Bar, order: list(&buckets), ..Default::default() };
    v.push(("column-10-buckets", calc(df, &spec)));
    v
}

/// The banking templates on their guide examples (name of the screenshot, chart), for the guide's images.
fn template_forms() -> Vec<(&'static str, ChartData)> {
    use micro_parquet::charts::samples as sm;
    use micro_parquet::charts::templates::{prepare, yield_curves, Ctx, Template};
    let ctx = Ctx { divisor: 1.0, suffix: "Cr".into(), dq_green: Some(98.0), dq_amber: Some(95.0), ..Default::default() };
    let mut out: Vec<(&'static str, ChartData)> = Vec::new();
    let mut add = |name: &'static str, t: Template, df: DataFrame| {
        let p = prepare(t, &df, &ctx).unwrap_or_else(|e| panic!("{name}: {e:#}"));
        let src = p.table.clone().unwrap_or(df);
        out.push((name, compute(src.lazy(), &p.spec).unwrap_or_else(|e| panic!("{name}: {e:#}"))));
        // a second chart of the same result (HQLA composition, NII effects by key) has its own picture
        if let Some(c) = p.companion {
            let second: &'static str = match name {
                "tpl-lcr-nsfr" => "tpl-lcr-nsfr-hqla",
                "tpl-nii-bridge-new" => "tpl-nii-effects-by-key-new",
                _ => "tpl-nii-effects-by-key",
            };
            out.push((second, compute(c.table.expect("table").lazy(), &c.spec).unwrap_or_else(|e| panic!("{second}: {e:#}"))));
        }
    };
    add("tpl-gap-ladder", Template::GapLadder, sm::ladder_table());
    add("tpl-repricing-gap", Template::RepricingGap, sm::repricing());
    add("tpl-nii-bridge", Template::NiiBridge, sm::mom_table());
    add("tpl-nii-bridge-keys", Template::NiiBridge, sm::mom_table_keys());
    add("tpl-nii-bridge-new", Template::NiiBridge, sm::mom_table_new_closed());
    add("tpl-cl-migration", Template::ClMigration, sm::cl_matrix_large());
    add("tpl-classified-trend", Template::ClassifiedTrend, sm::classified());
    add("tpl-classified-trend-no-loans", Template::ClassifiedTrend, sm::classified_empty_month());
    add("tpl-irrbb-scenarios", Template::IrrbbScenarios, sm::irrbb_results());
    add("tpl-lcr-nsfr", Template::LcrNsfr, sm::lcr_summary());
    add("tpl-ecl-by-stage", Template::EclByStage, sm::ecl_summary());
    add("tpl-deposit-mix", Template::DepositMix, sm::mix());
    add("tpl-rate-profile", Template::RateProfile, sm::rate_bands());
    add("tpl-behaviour-profile", Template::BehaviourProfile, sm::history(8));
    add("tpl-data-quality", Template::DataQuality, sm::scorecard());
    let y = yield_curves(&sm::curves()).expect("curves");
    out.push(("tpl-yield-curves", compute(y.table.expect("table").lazy(), &y.spec).expect("curves chart")));
    out
}

// ------------------------------------------------------------------- Table toggle and the Chart tab

use crate::ui::render_tests_p4::Rig;
use crate::ui::tests::{open_tab as _open_tab_unused, parquet_source, test_app};
use std::collections::HashMap;

fn chart_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let n = 250usize;
    let tenors = ["6m", "1y", "1m", "100D", "3m"];
    let df = DataFrame::new(vec![
        Series::new("Term Period".into(), (0..n).map(|i| tenors[i % 5].to_string()).collect::<Vec<_>>()).into(),
        Series::new("Risk".into(), (0..n).map(|i| ["High", "Medium", "Low"][i % 3].to_string()).collect::<Vec<_>>()).into(),
        Series::new("Balance".into(), (0..n).map(|i| 10_000_000.0 + i as f64 * 1_000_000.0).collect::<Vec<f64>>()).into(),
    ])
    .expect("frame");
    chart_rig_with(name, df, ppp, w, h)
}

pub(crate) fn chart_rig_with(name: &str, df: DataFrame, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    micro_parquet::rules::set_data_dir_override(Some(app.paths.root_dir.clone()));
    let root = app.paths.root_dir.clone();
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

// ================================================================================ C2: menu, gallery, studio

use crate::ui::layout_tests::Painted;
use crate::ActiveToolModal;
use crate::ui::render_tests::{assert_dialog_fits, inside};
use micro_parquet::commands::CommandId;

/// Moves the clock on from where egui's own is (the rig's first frames run without a clock).
fn bump(rig: &mut Rig, dt: f64) {
    let now = rig.ctx.input(|i| i.time);
    rig.input.time = Some(now + dt);
}

pub(crate) fn pclick(rig: &mut Rig, r: egui::Rect) {
    rig.step(vec![egui::Event::PointerMoved(r.center())]);
    bump(rig, 1.0);
    rig.click(r.center());
    bump(rig, 1.0);
}

/// Scrolls with the wheel over `at` until `rect_of` is inside the window (no animation: one step at a time).
pub(crate) fn scroll_into_view(rig: &mut Rig, at: egui::Pos2, rect_of: &dyn Fn(&Rig) -> Option<egui::Rect>, what: &str) -> egui::Rect {
    rig.ctx.style_mut(|s| s.scroll_animation = egui::style::ScrollAnimation::none());
    let screen = rig.ctx.screen_rect();
    for _ in 0..60 {
        rig.draw(2);
        if let Some(r) = rect_of(rig) {
            let visible = egui::Rect::from_min_max(egui::pos2(screen.left(), theme::TOP_BAR_H + theme::TAB_STRIP_H), egui::pos2(screen.right(), screen.bottom() - theme::STATUS_BAR_H));
            if inside(r, visible) && r.is_positive() {
                return r;
            }
        }
        bump(rig, 0.2);
        rig.step(vec![egui::Event::PointerMoved(at), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -120.0), modifiers: egui::Modifiers::NONE }]);
    }
    panic!("{}: {what} could not be scrolled into view", rig.what);
}

/// Like `scroll_into_view`, but the control must also be clear of the panel's own header (a control
/// scrolled under it is on screen but cannot be clicked): `min_top` is the lowest y it may have.
fn scroll_clear_of_header(rig: &mut Rig, at: egui::Pos2, rect_of: &dyn Fn(&Rig) -> Option<egui::Rect>, what: &str, min_top: f32) -> egui::Rect {
    rig.ctx.style_mut(|s| s.scroll_animation = egui::style::ScrollAnimation::none());
    let screen = rig.ctx.screen_rect();
    for _ in 0..80 {
        rig.draw(2);
        let mut dir = -120.0;
        if let Some(r) = rect_of(rig) {
            if r.top() >= min_top && r.bottom() <= screen.bottom() - theme::STATUS_BAR_H && r.is_positive() {
                return r;
            }
            if r.top() < min_top {
                dir = 120.0;
            }
        }
        bump(rig, 0.2);
        rig.step(vec![egui::Event::PointerMoved(at), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, dir), modifiers: egui::Modifiers::NONE }]);
    }
    panic!("{}: {what} could not be scrolled clear of the header", rig.what);
}

fn bar_box(p: &Painted, text: &str) -> Option<egui::Rect> {
    let in_bar = |b: &egui::Rect| b.top() < theme::TOP_BAR_H;
    p.boxes.iter().find(|(t, b)| t.trim() == text && in_bar(b)).or_else(|| p.boxes.iter().find(|(t, b)| t.contains(text) && in_bar(b))).map(|(_, b)| *b)
}

#[test]
fn c2_the_charts_menu_sits_between_banking_and_export_is_clickable_and_its_items_work() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = chart_rig("p7c2menu", ppp, w, h);
        let what = rig.what.clone();
        let p = rig.draw(6);
        let screen = rig.ctx.screen_rect();
        let order = ["File", "Edit", "View", "Banking", "Charts", "Export", "Help"];
        let boxes: Vec<egui::Rect> = order.iter().map(|n| bar_box(&p, n).unwrap_or_else(|| panic!("{what}: the menu '{n}' is in the bar"))).collect();
        for pair in boxes.windows(2) {
            assert!(pair[0].right() <= pair[1].left() + 0.5, "{what}: the menus follow File, Edit, View, Banking, Charts, Export, Help without overlap: {boxes:?}");
        }
        assert!(boxes.iter().all(|b| inside(*b, screen)), "{what}: no menu folds out of the window: {boxes:?}");
        // click Charts by its real rect: its four items
        pclick(&mut rig, boxes[4]);
        let p = rig.draw(3);
        let item = |p: &Painted, t: &str| p.boxes.iter().find(|(x, b)| x.contains(t) && b.top() > theme::TOP_BAR_H - 1.0).map(|(_, b)| *b);
        for t in ["New chart\u{2026}", "Chart this result", "Open saved chart\u{2026}", "Chart book (HTML)\u{2026}"] {
            let r = item(&p, t).unwrap_or_else(|| panic!("{what}: the Charts menu lists '{t}'"));
            assert!(inside(r, screen), "{what}: '{t}' is inside the window");
        }
        // on a file tab "Chart this result" says why it is greyed; the others are open
        assert!(rig.app.command_unavailable(CommandId::ChartThis).is_some(), "{what}");
        assert!(rig.app.command_unavailable(CommandId::ChartNew).is_none() && rig.app.command_unavailable(CommandId::ChartOpenSaved).is_none() && rig.app.command_unavailable(CommandId::ChartBook).is_none(), "{what}");
        // New chart… opens the gallery
        let r = item(&p, "New chart\u{2026}").expect("New chart");
        pclick(&mut rig, r);
        rig.draw(4);
        assert_eq!(rig.app.active_tool, ActiveToolModal::ChartGallery, "{what}: Charts > New chart opens the gallery");
        // Open saved chart… (second menu pass)
        rig.app.active_tool = ActiveToolModal::None;
        let p = rig.draw(3);
        let b = bar_box(&p, "Charts").expect("Charts");
        pclick(&mut rig, b);
        let p = rig.draw(3);
        let r = item(&p, "Open saved chart\u{2026}").expect("Open saved chart");
        pclick(&mut rig, r);
        rig.draw(4);
        assert_eq!(rig.app.active_tool, ActiveToolModal::ChartOpen, "{what}: Charts > Open saved chart opens the list");
        // Ctrl+K finds the commands
        assert!(micro_parquet::commands::command(CommandId::ChartNew).is_some());
    }
    micro_parquet::rules::set_data_dir_override(None);
}

fn gallery_title() -> String {
    format!("{}  New chart", theme::ph::CHART_BAR)
}

#[test]
fn c2_the_gallery_shows_every_card_with_its_use_and_avoid_lines_and_a_click_takes_the_card() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = chart_rig("p7c2gal", ppp, w, h);
        let what = rig.what.clone();
        rig.draw(6);
        rig.app.run_command(&rig.ctx, CommandId::ChartNew);
        assert_eq!(rig.app.active_tool, ActiveToolModal::ChartGallery, "{what}");
        let p = rig.draw(6);
        let dlg = assert_dialog_fits(&rig.ctx, &gallery_title(), &what);
        let g = rig.app.chart_gallery.rects.clone();
        assert_eq!(g.cards.len(), 17, "{what}: every card is drawn");
        for (i, (a, ra)) in g.cards.iter().enumerate() {
            for (b, rb) in g.cards.iter().skip(i + 1) {
                assert!(!overlap(*ra, *rb), "{what}: the cards '{a}' and '{b}' overlap");
            }
        }
        // the headings and the guide lines
        for t in ["Compare", "Suggest a chart", "Bar", "Use it when you rank"] {
            assert!(p.boxes.iter().any(|(x, _)| x.contains(t)), "{what}: '{t}' is in the gallery");
        }
        assert!(inside(g.suggest.expect("Suggest"), dlg.window) && inside(g.close.expect("Close"), dlg.window), "{what}: Suggest and Close are in the dialog");
        // "Suggest": nothing picked says so
        pclick(&mut rig, g.suggest.expect("Suggest"));
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t.contains("at least one column")), "{what}: the reason is shown");
        // pick the text and the number column, then Suggest: a sorted bar
        let g = rig.app.chart_gallery.rects.clone();
        for name in ["Risk", "Balance"] {
            let r = g.columns.iter().find(|(n, _)| n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: chip {name}"));
            pclick(&mut rig, r);
            rig.draw(2);
        }
        assert_eq!(rig.app.chart_gallery.picked, vec!["Risk".to_string(), "Balance".to_string()], "{what}");
        let g = rig.app.chart_gallery.rects.clone();
        pclick(&mut rig, g.suggest.expect("Suggest"));
        rig.draw(3);
        match &rig.app.chart_gallery.suggestion {
            Some(Ok(s)) => assert_eq!((s.kind, s.horizontal, s.by_value), (ChartKind::Bar, true, true), "{what}: text + number is a sorted bar"),
            other => panic!("{what}: {other:?}"),
        }
        let g = rig.app.chart_gallery.rects.clone();
        pclick(&mut rig, g.use_suggestion.expect("Use this chart"));
        rig.draw(4);
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: taking a suggestion closes the gallery");
        assert_eq!((rig.app.chart.spec.kind, rig.app.chart.spec.horizontal, rig.app.chart.spec.by_value), (ChartKind::Bar, true, true), "{what}");
        // take cards by their real rects, scrolling the dialog to them
        for (id, check) in [("bar", ChartKind::Bar), ("line", ChartKind::Line), ("donut", ChartKind::Donut), ("waterfall", ChartKind::Waterfall), ("treemap", ChartKind::Treemap)] {
            rig.app.run_command(&rig.ctx, CommandId::ChartNew);
            rig.draw(4);
            let dlg = theme::dialog_report(&rig.ctx, &gallery_title()).expect("the dialog");
            let at = dlg.body.center();
            let r = scroll_into_view(&mut rig, at, &|rig: &Rig| rig.app.chart_gallery.rects.cards.iter().find(|(n, _)| n == id).map(|(_, r)| *r), &format!("the card {id}"));
            assert!(inside(r, dlg.window), "{what}: the card {id} {r:?} is in the dialog {:?}", dlg.window);
            pclick(&mut rig, r);
            rig.draw(4);
            assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: {id}");
            assert_eq!(rig.app.chart.spec.kind, check, "{what}: the card {id} sets the form");
        }
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn c2_the_studio_controls_work_by_real_clicks_titles_reference_lines_and_measures() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = chart_rig("p7c2studio", ppp, w, h);
        let what = rig.what.clone();
        rig.draw(10);
        let screen = rig.ctx.screen_rect();
        let at = egui::pos2(screen.right() - 120.0, screen.center().y);
        let field_rect = |name: &'static str| move |rig: &Rig| rig.app.chart.rects.fields.iter().find(|(n, _)| n == name).map(|(_, r)| *r);
        // Titles and look: open the section, type a takeaway
        let r = scroll_into_view(&mut rig, at, &field_rect("Titles and look"), "Titles and look");
        pclick(&mut rig, r);
        rig.draw(3);
        let r = scroll_into_view(&mut rig, at, &field_rect("Takeaway"), "Takeaway");
        pclick(&mut rig, r);
        rig.step(vec![egui::Event::Text("Term deposits drove growth".into())]);
        rig.draw(6);
        assert_eq!(rig.app.chart.spec.takeaway, "Term deposits drove growth", "{what}: typed into the Takeaway field");
        let shown = last_shown(&rig.ctx, egui::Id::new("chart_plot")).expect("the chart");
        assert!(shown.layout.texts.iter().any(|t| t.role == Role::Title && t.text == "Term deposits drove growth"), "{what}: the takeaway is the title of the chart");
        let r = scroll_into_view(&mut rig, at, &field_rect("Footnote"), "Footnote");
        pclick(&mut rig, r);
        rig.step(vec![egui::Event::Text("Synthetic data".into())]);
        rig.draw(4);
        assert_eq!(rig.app.chart.spec.footnote, "Synthetic data", "{what}");
        // Type: Bar (Column / bar) stays; choose Stacked in the Stack combo by clicking
        // Reference lines: open, add one, it is drawn with its status written out
        let r = scroll_into_view(&mut rig, at, &field_rect("Reference lines"), "Reference lines");
        pclick(&mut rig, r);
        rig.draw(3);
        let r = scroll_into_view(&mut rig, at, &field_rect("Add reference line"), "Add reference line");
        pclick(&mut rig, r);
        rig.draw(4);
        assert_eq!(rig.app.chart.spec.ref_lines.len(), 1, "{what}: a reference line was added");
        let shown = last_shown(&rig.ctx, egui::Id::new("chart_plot")).expect("the chart");
        assert!(shown.layout.texts.iter().any(|t| t.role == Role::Reference && t.text.contains("Critical")), "{what}: the line is labelled with its status");
        let r = scroll_into_view(&mut rig, at, &field_rect("Line 0 remove"), "Remove");
        pclick(&mut rig, r);
        rig.draw(3);
        assert!(rig.app.chart.spec.ref_lines.is_empty(), "{what}: removed");
        // Measures and panels: a second measure gives linked panels
        let r = scroll_into_view(&mut rig, at, &field_rect("Measures and panels"), "Measures and panels");
        pclick(&mut rig, r);
        rig.draw(3);
        let r = scroll_into_view(&mut rig, at, &field_rect("Add measure"), "Add measure");
        pclick(&mut rig, r);
        rig.draw(12);
        assert_eq!(rig.app.chart.spec.measures.len(), 2, "{what}: a measure was added");
        let shown = last_shown(&rig.ctx, egui::Id::new("chart_plot")).expect("the chart");
        assert_eq!(shown.layout.panels.len(), 2, "{what}: two measures are two linked panels, never a second axis: {:?} {:?} {:?}", shown.layout.message, rig.app.chart.error, rig.app.chart.data.as_ref().map(|d| (d.measure_count(), d.categories.len())));
        assert_eq!(rig.app.chart.data.as_ref().map(|d| d.measure_count()), Some(2), "{what}");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn c2_a_chart_is_saved_by_name_and_opened_again_identically_by_real_clicks() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = chart_rig("p7c2save", ppp, w, h);
        let what = rig.what.clone();
        rig.draw(10);
        rig.app.chart.spec.takeaway = "Saved takeaway".into();
        rig.app.chart.spec.horizontal = true;
        rig.draw(6);
        let screen = rig.ctx.screen_rect();
        let at = egui::pos2(screen.right() - 120.0, screen.center().y);
        let field_rect = |name: &'static str| move |rig: &Rig| rig.app.chart.rects.fields.iter().find(|(n, _)| n == name).map(|(_, r)| *r);
        let r = scroll_into_view(&mut rig, at, &field_rect("Save chart"), "Save chart");
        pclick(&mut rig, r);
        rig.draw(4);
        assert_eq!(rig.app.active_tool, ActiveToolModal::ChartSave, "{what}: Save chart opens the dialog");
        let title = format!("{}  Save chart", theme::ph::FLOPPY_DISK);
        assert_dialog_fits(&rig.ctx, &title, &what);
        assert!(!rig.app.chart_save.name.is_empty(), "{what}: the name is filled in from the title");
        rig.app.chart_save.name = "Deposit mix".into();
        let saved = rig.app.chart.spec.clone();
        rig.draw(3);
        let save = rig.app.chart_save.rects.iter().find(|(n, _)| n == "Save").map(|(_, r)| *r).expect("Save button");
        pclick(&mut rig, save);
        rig.draw(4);
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: saved and closed");
        let file = rig.app.paths.root_dir.join("charts").join("Deposit mix.json");
        assert!(file.exists(), "{what}: portable_data/charts/Deposit mix.json was written");
        // change the chart, then open the saved one
        rig.app.chart.spec.takeaway = "Changed".into();
        rig.app.chart.spec.horizontal = false;
        rig.draw(4);
        let r = scroll_into_view(&mut rig, at, &field_rect("Open saved chart"), "Open saved chart");
        pclick(&mut rig, r);
        rig.draw(4);
        assert_eq!(rig.app.active_tool, ActiveToolModal::ChartOpen, "{what}");
        let open = rig.app.chart_open.rects.iter().find(|(n, _)| n == "Open Deposit mix").map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: the saved chart is listed"));
        pclick(&mut rig, open);
        rig.draw(6);
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}");
        assert_eq!(rig.app.chart.spec, saved, "{what}: a saved chart reloads identically");
        // a chart that names a column this tab does not have is refused, naming it
        let mut other = saved.clone();
        other.value = Some("Nonexistent".into());
        micro_parquet::charts::library::save_chart(&rig.app.paths.root_dir.join("charts"), "Other tab", &other).expect("save");
        rig.app.open_chart_open();
        rig.draw(4);
        let open = rig.app.chart_open.rects.iter().find(|(n, _)| n == "Open Other tab").map(|(_, r)| *r).expect("listed");
        pclick(&mut rig, open);
        let p = rig.draw(4);
        assert_eq!(rig.app.active_tool, ActiveToolModal::ChartOpen, "{what}: the dialog stays open on a refusal");
        assert!(p.boxes.iter().any(|(t, _)| t.contains("does not have: Nonexistent")), "{what}: the refusal names the column");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

// ------------------------------------------------------------------------------------------------ C3

#[test]
fn c3_export_preview_advanced_options_and_the_chart_book_work_by_real_clicks() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = chart_rig("p7c3html", ppp, w, h);
        let what = rig.what.clone();
        rig.draw(10);
        let dir = rig.app.paths.root_dir.join("c3out");
        std::fs::create_dir_all(&dir).expect("folder");
        let page = dir.join("chart.html");
        rig.app.chart.export_path_override = Some(page.clone());
        rig.app.chart.no_browser = true;
        let screen = rig.ctx.screen_rect();
        let at = egui::pos2(screen.right() - 120.0, screen.center().y);
        let field_rect = |name: &'static str| move |rig: &Rig| rig.app.chart.rects.fields.iter().find(|(n, _)| n == name).map(|(_, r)| *r);
        // Export to HTML: the file is written, offline, and the studio says where
        let r = scroll_clear_of_header(&mut rig, at, &field_rect("Export to HTML"), "Export to HTML", 190.0);
        pclick(&mut rig, r);
        rig.draw(4);
        let html = std::fs::read_to_string(&page).unwrap_or_else(|e| panic!("{what}: the page was written: {e}"));
        assert!(html.contains("class=\"mp-chart\"") && html.contains("echarts"), "{what}");
        let msg = rig.app.chart.export_message.clone().expect("a message");
        assert!(!msg.0 && msg.1.contains("chart.html") && msg.1.contains("needs no internet"), "{what}: {msg:?}");
        // Export to Excel: a workbook with the numbers and a native chart
        let book_x = dir.join("chart.xlsx");
        rig.app.chart.export_path_override = Some(book_x.clone());
        let r = scroll_clear_of_header(&mut rig, at, &field_rect("Export to Excel"), "Export to Excel", 190.0);
        pclick(&mut rig, r);
        rig.draw(4);
        let bytes = std::fs::read(&book_x).unwrap_or_else(|e| panic!("{what}: the workbook was written: {e}"));
        assert!(bytes.starts_with(b"PK"), "{what}: an .xlsx is a zip");
        let msg = rig.app.chart.export_message.clone().expect("a message");
        assert!(!msg.0 && msg.1.contains("chart.xlsx") && msg.1.contains("native chart"), "{what}: {msg:?}");
        rig.app.chart.export_path_override = Some(page.clone());
        // Preview in browser: a temporary file, no browser started by the test
        let r = scroll_clear_of_header(&mut rig, at, &field_rect("Preview in browser"), "Preview in browser", 190.0);
        pclick(&mut rig, r);
        rig.draw(4);
        let msg = rig.app.chart.export_message.clone().expect("a message");
        assert!(!msg.0 && msg.1.contains("Preview opened"), "{what}: {msg:?}");
        // Copy as Python: a script on the clipboard, with the chart's numbers and the install line
        let r = scroll_clear_of_header(&mut rig, at, &field_rect("Copy as Python"), "Copy as Python", 190.0);
        pclick(&mut rig, r);
        rig.draw(4);
        let msg = rig.app.chart.export_message.clone().expect("a message");
        assert!(!msg.0 && msg.1.contains("Copied a Python script"), "{what}: {msg:?}");
        let script = rig.app.chart.copied_python.clone().expect("the script");
        assert!(script.contains("import matplotlib.pyplot as plt") && script.contains("pd.DataFrame"), "{what}");
        // Advanced options: open, type invalid JSON, the line and column are shown and the export is refused
        let r = scroll_clear_of_header(&mut rig, at, &field_rect("Advanced options header"), "Advanced options", 190.0);
        pclick(&mut rig, r);
        rig.draw(3);
        let r = scroll_clear_of_header(&mut rig, at, &field_rect("Advanced options"), "the advanced box", 190.0);
        pclick(&mut rig, r);
        rig.step(vec![egui::Event::Text("{ \"legend\": ".into())]);
        rig.draw(4);
        assert_eq!(rig.app.chart.spec.advanced_json, "{ \"legend\": ", "{what}: typed into the box");
        let p = rig.draw(2);
        assert!(p.boxes.iter().any(|(t, _)| t.contains("not valid JSON") && t.contains("line 1")), "{what}: the mistake is shown with its place");
        std::fs::remove_file(&page).ok();
        let r = scroll_clear_of_header(&mut rig, at, &field_rect("Export to HTML"), "Export to HTML", 190.0);
        pclick(&mut rig, r);
        rig.draw(4);
        assert!(!page.exists(), "{what}: nothing is written when the advanced JSON is invalid");
        let msg = rig.app.chart.export_message.clone().expect("a message");
        assert!(msg.0 && msg.1.contains("not valid JSON"), "{what}: {msg:?}");
        // Reset clears the box; a valid object is merged into the page and the chart in the app ignores it
        let r = scroll_clear_of_header(&mut rig, at, &field_rect("Advanced reset"), "Reset", 190.0);
        pclick(&mut rig, r);
        rig.draw(3);
        assert!(rig.app.chart.spec.advanced_json.is_empty(), "{what}: Reset");
        rig.app.chart.spec.advanced_json = "{ \"legend\": { \"show\": false } }".into();
        rig.draw(6);
        let shown = last_shown(&rig.ctx, egui::Id::new("chart_plot")).expect("the chart");
        assert!(shown.layout.message.is_none(), "{what}: the chart in the app ignores the advanced options");
        let r = scroll_clear_of_header(&mut rig, at, &field_rect("Export to HTML"), "Export to HTML", 190.0);
        pclick(&mut rig, r);
        rig.draw(4);
        assert!(std::fs::read_to_string(&page).expect("written").contains("\"legend\":{\"show\":false}"), "{what}: the advanced option is in the page");
        // the chart book: save the chart, open the dialog through the menu command, tick, write
        let dirc = rig.app.charts_dir();
        let mut spec = rig.app.chart.spec.clone();
        spec.advanced_json.clear();
        micro_parquet::charts::library::save_chart(&dirc, "Term mix", &spec).expect("saved");
        let ctx = rig.ctx.clone();
        rig.app.run_command(&ctx, CommandId::ChartBook);
        rig.draw(4);
        assert_eq!(rig.app.active_tool, ActiveToolModal::ChartBook, "{what}: Charts > Chart book opens the dialog");
        assert_dialog_fits(&rig.ctx, &format!("{}  Chart book (HTML)", theme::ph::FILES), &what);
        // the saved charts are a second section, unticked: the open charts (here the studio chart) come first, ticked
        assert!(rig.app.chart_book.entries.iter().any(|e| e.name == "Term mix" && !e.checked && e.problem.is_none()), "{what}");
        assert_eq!(rig.app.chart_book.snaps.len(), 1, "{what}: the studio chart is listed");
        assert!(rig.app.chart_book.snaps[0].checked, "{what}: and ticked");
        rig.draw(3);
        let tick = rig.app.chart_book.rects.iter().find(|(n, _)| n == "Chart 0").map(|(_, r)| *r).expect("Chart 0");
        pclick(&mut rig, tick);
        assert!(rig.app.chart_book.entries[0].checked, "{what}: the saved chart is ticked by a click");
        let book = dir.join("book.html");
        rig.app.chart_book.path_override = Some(book.clone());
        rig.app.chart_book.title = "ALCO charts".into();
        rig.draw(3);
        let write = rig.app.chart_book.rects.iter().find(|(n, _)| n == "Write chart book").map(|(_, r)| *r).expect("Write chart book");
        pclick(&mut rig, write);
        for _ in 0..200 {
            rig.draw(2);
            if book.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let b = std::fs::read_to_string(&book).unwrap_or_else(|e| panic!("{what}: the chart book was written by the worker: {e}"));
        assert_eq!(b.matches("class=\"mp-chart\"").count(), 2, "{what}: the studio chart and the saved chart");
        assert!(b.contains("<h1>ALCO charts</h1>"), "{what}");
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: the dialog closed");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

// ------------------------------------------------------------------------------------------------ C5

use micro_parquet::charts::samples::ladder_table;

fn settle_tabs(rig: &mut Rig, tabs: usize) {
    for _ in 0..200 {
        rig.draw(2);
        if rig.app.tabs.len() >= tabs && !rig.app.chart.busy && rig.app.chart.data.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn c5_the_gallery_lists_the_banking_templates_and_a_click_on_the_gap_ladder_charts_it_with_the_breach_ringed() {
    use micro_parquet::charts::layout::Prim;
    for (ppp, w, h) in SIZES3 {
        let mut rig = chart_rig_with("p7c5gal", ladder_table(), ppp, w, h);
        let what = rig.what.clone();
        rig.draw(6);
        rig.app.run_command(&rig.ctx, CommandId::ChartNew);
        rig.draw(6);
        let dlg = theme::dialog_report(&rig.ctx, &gallery_title()).expect("the dialog");
        let g = rig.app.chart_gallery.rects.clone();
        assert_eq!(g.templates.len(), 13, "{what}: every banking template has a card");
        for (i, (a, ra)) in g.templates.iter().enumerate() {
            for (b, rb) in g.templates.iter().skip(i + 1) {
                assert!(!overlap(*ra, *rb), "{what}: the template cards '{a}' and '{b}' overlap");
            }
        }
        // a card that does not fit this tab says why, and the gallery stays open
        let at = dlg.body.center();
        let r = scroll_into_view(&mut rig, at, &|rig: &Rig| rig.app.chart_gallery.rects.templates.iter().find(|(n, _)| n == "irrbb-scenarios").map(|(_, r)| *r), "the IRRBB card");
        assert!(inside(r, dlg.window), "{what}: the IRRBB card is in the dialog");
        pclick(&mut rig, r);
        rig.draw(4);
        assert_eq!(rig.app.active_tool, ActiveToolModal::ChartGallery, "{what}: a template that does not fit leaves the gallery open");
        let msg = rig.app.chart_gallery.template_message.clone().unwrap_or_default();
        assert!(msg.contains("no column 'Scenario'") && msg.contains("IRRBB"), "{what}: {msg}");
        // the gap ladder card fits: click it
        let r = scroll_clear_of_header(&mut rig, at, &|rig: &Rig| rig.app.chart_gallery.rects.templates.iter().find(|(n, _)| n == "gap-ladder").map(|(_, r)| *r), "the gap ladder card", dlg.body.top() + 4.0);
        assert!(inside(r, dlg.window), "{what}: the gap ladder card is in the dialog");
        pclick(&mut rig, r);
        settle_tabs(&mut rig, 2);
        rig.draw(6);
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: the gallery closed");
        assert_eq!(rig.app.tabs.len(), 2, "{what}: the chart's table is a tab of its own");
        assert_eq!(rig.app.tabs[rig.app.active_tab].title, "Chart: gap ladder", "{what}");
        assert_eq!(rig.app.chart.spec.title, "Cumulative gap turns positive after 1\u{2013}3 months", "{what}: the finding is the title");
        assert_eq!(rig.app.chart.spec.panels.len(), 2, "{what}: linked panels");
        let shown = last_shown(&rig.ctx, egui::Id::new("chart_plot")).expect("the chart");
        assert!(shown.layout.message.is_none(), "{what}: {:?} {:?}", shown.layout.message, rig.app.chart.error);
        assert!(shown.layout.texts.iter().any(|t| t.role == Role::Title && t.text.starts_with("Cumulative gap turns positive")), "{what}: the finding is drawn as the title");
        assert!(shown.layout.texts.iter().any(|t| t.role == Role::Reference && t.text == "Breach: \u{2212}16.7% vs \u{2212}10% limit"), "{what}: the breach is labelled");
        assert_eq!(shown.layout.prims.iter().filter(|p| matches!(p, Prim::Outline { .. })).count(), 1, "{what}: and ringed");
        // (the side panel's right edge can round up by a pixel at a fractional scale)
        assert_rules(&shown, rig.ctx.screen_rect().expand(2.0), &what);
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn c5_chart_this_applies_the_one_template_that_fits_and_says_so_when_none_does() {
    let mut rig = chart_rig_with("p7c5this", ladder_table(), 1.05, 1291.0, 700.0);
    rig.draw(6);
    rig.app.chart_this_result();
    settle_tabs(&mut rig, 2);
    rig.draw(4);
    // (the status line is taken over by the new tab's loading message: the finding is the chart's title)
    assert_eq!(rig.app.tabs.len(), 2, "the template's table opened as a tab");
    assert_eq!(rig.app.chart.spec.title, "Cumulative gap turns positive after 1\u{2013}3 months");
    assert!(rig.app.chart.spec.takeaway.is_empty(), "the takeaway is left for the user: the studio is still editable");
    // a tab no template fits: the studio opens and the message points at the gallery
    let mut rig = chart_rig("p7c5none", 1.05, 1291.0, 700.0);
    rig.draw(6);
    rig.app.chart_this_result();
    assert!(rig.app.status_message.contains("No banking template fits"), "{}", rig.app.status_message);
    micro_parquet::rules::set_data_dir_override(None);
}
