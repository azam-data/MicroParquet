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
    // The first frame of a context still has egui's default screen size (and the zoom factor is
    // applied in it); a dialog opened in it would get a position from that. Real dialogs open
    // after the first frames, so the tests do too.
    for _ in 0..2 {
        let _ = ctx.run(input.clone(), |_| {});
    }
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

// ------------------------------------------------------------------------------- dialogs

const TITLE_H: f32 = 22.0;

#[test]
fn dialog_layout_leaves_48_px_and_keeps_the_footer_in_the_window() {
    let screen_h = WIN_H_PX / UI_SCALE;
    let l = theme::dialog_layout(screen_h, TITLE_H, 6.0);
    assert!((l.max_total_h - (screen_h - 48.0)).abs() < 0.01, "{l:?}");
    // title bar + frame + body + spacing + footer add up to the maximum, never more
    let overhead = 4.0 * theme::SPACE_M + TITLE_H;
    assert!(overhead + l.body_max_h + 6.0 + theme::DIALOG_FOOTER_H <= l.max_total_h + 0.01, "{l:?}");
    assert!(l.body_max_h >= theme::DIALOG_BODY_MIN_H);
    // a taller window gives a taller middle, a tiny one keeps a usable middle
    assert!(theme::dialog_layout(1200.0, TITLE_H, 6.0).body_max_h > l.body_max_h);
    assert_eq!(theme::dialog_layout(100.0, TITLE_H, 6.0).body_max_h, theme::DIALOG_BODY_MIN_H);
}

#[test]
fn a_tall_dialog_is_capped_and_its_footer_buttons_stay_inside_the_window() {
    // Several frames in one pass each: the dialog is shown inside ctx.run.
    let (ctx, input) = headless(WIN_W_PX, WIN_H_PX, UI_SCALE);
    let screen = input.screen_rect.unwrap_or(egui::Rect::NOTHING);
    for rows in [3usize, 40, 4000] {
        let mut win = egui::Rect::NOTHING;
        let mut buttons: Vec<egui::Rect> = Vec::new();
        for _ in 0..4 {
            let _ = ctx.run(input.clone(), |ctx| {
                buttons.clear();
                let r = theme::modal(&format!("Layout test {rows}")).show(ctx, |d| {
                    d.body(|ui| {
                        for i in 0..rows {
                            ui.label(format!("line {i}"));
                        }
                    });
                    d.footer(|ui| {
                        ui.horizontal(|ui| {
                            buttons.push(theme::primary_button(ui, "Apply").rect);
                            buttons.push(ui.button("Cancel").rect);
                        });
                    });
                });
                if let Some(r) = r {
                    win = r.response.rect;
                }
            });
        }
        assert!(win.height() <= screen.height() - 48.0 + 0.5, "{rows} rows: window {win:?} in screen {screen:?}");
        assert!(win.top() >= screen.top() - 0.5 && win.bottom() <= screen.bottom() + 0.5, "{rows} rows: {win:?}");
        assert_eq!(buttons.len(), 2);
        for b in &buttons {
            assert!(b.bottom() <= win.bottom() && b.top() >= win.top(), "{rows} rows: button {b:?} outside {win:?}");
            assert!(b.bottom() <= screen.bottom() - 24.0 + 0.5, "{rows} rows: button {b:?} too low for {screen:?}");
        }
        if rows == 4000 {
            // the middle scrolls: the window is the full height and the footer is at its bottom
            assert!(win.height() >= screen.height() - 48.0 - 8.0, "{win:?}");
            assert!(buttons[0].bottom() >= win.bottom() - 2.0 * theme::SPACE_M - 1.0, "footer sits at the bottom: {win:?} {:?}", buttons[0]);
        }
        // a short dialog is not stretched and opens horizontally centred, 24 px below the top
        if rows == 3 {
            assert!(win.height() < 220.0, "{win:?}");
        }
        assert!((win.top() - 24.0).abs() < 1.0, "{rows} rows: opens 24 px below the top: {win:?}");
        // (a dialog without a set width opens as if it were 520 wide; its real width is its content's)
        assert!(((win.center().x) - screen.center().x).abs() < 100.0, "{rows} rows: roughly centred: {win:?}");
    }
}

#[test]
fn real_dialogs_with_long_content_keep_their_buttons_in_the_window() {
    let mut app = test_app("dlg");
    let (ctx, input) = headless(WIN_W_PX, WIN_H_PX, UI_SCALE);
    let screen = input.screen_rect.unwrap_or(egui::Rect::NOTHING);
    // 80 filter conditions: far more than fit
    app.filter.conditions = (0..80)
        .map(|i| FilterCondition { column: format!("Col{i}"), op: CondOp::Equals, value: format!("{i}"), value2: String::new() })
        .collect();
    let cols: Vec<String> = (0..80).map(|i| format!("Col{i}")).collect();
    for _ in 0..4 {
        let _ = ctx.run(input.clone(), |ctx| {
            let _ = app.render_filter_modal(ctx, cols.clone());
        });
    }
    let id = egui::Id::new("\u{E30C}  Filter rows");
    let win = ctx.memory(|m| m.area_rect(id)).expect("the filter dialog was shown");
    assert!(win.height() <= screen.height() - 48.0 + 0.5, "{win:?} in {screen:?}");
    assert!(win.bottom() <= screen.bottom() - 24.0 + 0.5 && win.top() >= 24.0 - 0.5, "{win:?} in {screen:?}");
}
