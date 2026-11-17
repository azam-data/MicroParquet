//! Layout tests (Phase 3 fixes, part 2): the screens are drawn in a headless egui frame at a
//! given window size and scale, and the rectangles of what was drawn are checked.

use crate::ui::prelude::*;
use crate::ui::tests::test_app;
use crate::ui::top_bar::{plan_bar, BAR_GROUPS};

/// The window of the owner's smallest supported screen: 1280×720 pixels at 105% UI scale.
pub(crate) const WIN_W_PX: f32 = 1280.0;
pub(crate) const WIN_H_PX: f32 = 720.0;
pub(crate) const UI_SCALE: f32 = 1.05;

/// A context set up as the app starts it (fonts, theme, scale) and the screen size in points.
pub(crate) fn headless(px_w: f32, px_h: f32, scale: f32) -> (egui::Context, egui::RawInput) {
    let ctx = egui::Context::default();
    theme::load_fonts(&ctx);
    theme::apply_ui_theme(&ctx, false, scale);
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(px_w / scale, px_h / scale))),
        ..Default::default()
    };
    (ctx, input)
}

fn bar_rects_at(px_w: f32) -> (egui::Rect, Vec<(String, egui::Rect)>) {
    let mut app = test_app("bar");
    let (ctx, input) = headless(px_w, WIN_H_PX, UI_SCALE);
    let mut rects = Vec::new();
    // frames: the first ones only load the fonts and measure
    for _ in 0..3 {
        let _ = ctx.run(input.clone(), |ctx| {
            rects = app.render_top_bar(ctx);
        });
    }
    (input.screen_rect.unwrap_or(egui::Rect::NOTHING), rects)
}

fn assert_no_overlap(rects: &[(String, egui::Rect)], bar: egui::Rect, what: &str) {
    for (name, r) in rects {
        assert!(
            r.left() >= bar.left() - 0.5 && r.right() <= bar.right() + 0.5,
            "{what}: '{name}' {r:?} is outside the bar {bar:?}"
        );
        assert!(r.top() >= bar.top() - 0.5 && r.bottom() <= bar.top() + theme::TOP_BAR_H + 0.5, "{what}: '{name}' {r:?} leaves the 44 px bar");
    }
    for (i, (an, a)) in rects.iter().enumerate() {
        for (bn, b) in rects.iter().skip(i + 1) {
            assert!(
                a.right() <= b.left() + 0.01 || b.right() <= a.left() + 0.01,
                "{what}: '{an}' {a:?} overlaps '{bn}' {b:?}"
            );
        }
    }
}

#[test]
fn the_top_bar_is_one_44_px_strip() {
    let (_screen, rects) = bar_rects_at(WIN_W_PX);
    assert!(!rects.is_empty());
    let top = rects.iter().map(|(_, r)| r.top()).fold(f32::MAX, f32::min);
    let bottom = rects.iter().map(|(_, r)| r.bottom()).fold(f32::MIN, f32::max);
    assert!(top >= 0.0 && bottom <= theme::TOP_BAR_H, "items lie inside the first 44 px: {top}..{bottom}");
    // order: mark, File, View, Help, ..., More, then the search box at the far right
    let names: Vec<&str> = rects.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(&names[..4], &["Mark", "File", "View", "Help"]);
    assert_eq!(*names.last().unwrap_or(&""), "Search");
    let more = names.iter().position(|n| *n == "More").expect("a More menu is always there");
    assert_eq!(more + 2, names.len(), "More is the last item before the search box: {names:?}");
}

#[test]
fn nothing_overlaps_at_1280_by_720_and_105_percent() {
    let (screen, rects) = bar_rects_at(WIN_W_PX);
    eprintln!("1280x720 @105%: screen {screen:?}");
    for (n, r) in &rects {
        eprintln!("  {n:12} x {:7.1}..{:7.1}  w {:6.1}", r.left(), r.right(), r.width());
    }
    assert_no_overlap(&rects, egui::Rect::from_min_size(screen.min, egui::vec2(screen.width(), theme::TOP_BAR_H)), "1280");
    // the search box sits at the far right and is wide enough to use
    let search = rects.iter().find(|(n, _)| n == "Search").map(|(_, r)| *r).expect("search box");
    assert!(search.right() > screen.right() - 20.0, "far right: {search:?} in {screen:?}");
    assert!(search.width() >= theme::SEARCH_MIN_W - 1.0, "{search:?}");
}

#[test]
fn nothing_overlaps_at_any_window_width() {
    // from the smallest window the app allows up to a wide monitor, in 20 px steps
    let mut w = 800.0;
    let mut last_shown = 0usize;
    while w <= 2560.0 {
        let (screen, rects) = bar_rects_at(w);
        assert_no_overlap(&rects, egui::Rect::from_min_size(screen.min, egui::vec2(screen.width(), theme::TOP_BAR_H)), &format!("{w} px"));
        // groups only come back as the window grows
        let shown = rects.iter().filter(|(n, _)| matches!(n.as_str(), "Banking" | "Export" | "Filter" | "OpenFile")).count();
        assert!(shown >= last_shown || w < 900.0, "{w}: {shown} < {last_shown}");
        last_shown = shown;
        w += 20.0;
    }
}

#[test]
fn groups_that_do_not_fit_fold_into_more_from_the_right() {
    // one fixed width of 100, groups 200 / 300 / 110 / 100, More 90, gap 20, search at least 170
    let g = [200.0, 300.0, 110.0, 100.0];
    let fit = |avail: f32| plan_bar(avail, 100.0, &g, 90.0, 20.0).shown;
    assert_eq!(fit(2000.0), 4);
    assert_eq!(fit(100.0 + 4.0 * 20.0 + 710.0 + 20.0 + 90.0 + 20.0 + 170.0), 4, "exactly enough");
    assert_eq!(fit(100.0 + 4.0 * 20.0 + 710.0 + 20.0 + 90.0 + 20.0 + 170.0 - 1.0), 3, "one pixel less folds the last group");
    assert_eq!(fit(100.0 + 20.0 + 200.0 + 20.0 + 90.0 + 20.0 + 170.0), 1);
    assert_eq!(fit(300.0), 0, "nothing fits: everything is in More");
    // the search box gets what is left, up to its preferred width
    assert_eq!(plan_bar(5000.0, 100.0, &g, 90.0, 20.0).search_w, theme::SEARCH_MAX_W);
    assert!(plan_bar(300.0, 100.0, &g, 90.0, 20.0).search_w >= theme::SEARCH_ICON_W);
    assert_eq!(BAR_GROUPS.len(), g.len());
}

#[test]
fn the_version_is_in_help_about_not_in_the_bar() {
    fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
            _ => {}
        }
    }
    let mut app = test_app("barv");
    let (ctx, input) = headless(WIN_W_PX, WIN_H_PX, UI_SCALE);
    let mut texts = Vec::new();
    for _ in 0..2 {
        let out = ctx.run(input.clone(), |ctx| {
            app.render_top_bar(ctx);
        });
        texts.clear();
        for c in &out.shapes {
            collect(&c.shape, &mut texts);
        }
    }
    let all = texts.join(" | ");
    assert!(!all.contains(env!("CARGO_PKG_VERSION")), "the bar shows no version: {all}");
    assert!(all.contains("Help") && all.contains("Ctrl K"), "{all}");
    // About still says it
    app.show_about = true;
    let mut about = Vec::new();
    // the window fades in over a few frames
    for _ in 0..4 {
        let out = ctx.run(input.clone(), |ctx| app.render_about(ctx));
        about.clear();
        for c in &out.shapes {
            collect(&c.shape, &mut about);
        }
    }
    assert!(about.join(" ").contains(env!("CARGO_PKG_VERSION")), "{about:?}");
}
