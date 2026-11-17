//! Render tests (Phase 3 fixes, part 3): the real UI code is drawn in a headless egui context at
//! the window sizes and UI scales of a real build, and what it drew is checked by its rectangles
//! (not by a layout function). The window is 1280×720 pixels at 80% and 105% UI scale, which is
//! 1600×900 and 1219×686 points; the same 1280×720 in points is tried at both scales as well.

use crate::ui::layout_tests::{headless, schema_modal};
use crate::ui::prelude::*;
use crate::ui::tests::{loans_frame, open_tab, parquet_source, test_app};

/// (pixels per point, width in points, height in points).
pub(crate) const SCREENS: [(f32, f32, f32); 4] = [
    (0.8, 1600.0, 900.0),
    (0.8, 1280.0, 720.0),
    (1.05, 1219.05, 685.71),
    (1.05, 1280.0, 720.0),
];

/// A context with the app's fonts and theme at this scale and a window of this size.
pub(crate) fn screen(ppp: f32, w: f32, h: f32) -> (egui::Context, egui::RawInput) {
    let (ctx, mut input) = headless(w * ppp, h * ppp, ppp);
    input.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h)));
    (ctx, input)
}

/// Runs `n` whole frames of the app.
pub(crate) fn frames(app: &mut MicroParquetApp, ctx: &egui::Context, input: &egui::RawInput, n: usize) {
    for _ in 0..n {
        let _ = ctx.run(input.clone(), |c| app.frame_ui(c));
    }
}

fn inside(inner: egui::Rect, outer: egui::Rect) -> bool {
    inner.left() >= outer.left() - 0.5 && inner.right() <= outer.right() + 0.5 && inner.top() >= outer.top() - 0.5 && inner.bottom() <= outer.bottom() + 0.5
}

/// A dialog's rectangles must lie in the area the panels leave, less 16 px.
fn assert_dialog_fits(ctx: &egui::Context, title: &str, what: &str) -> theme::DialogReport {
    let r = theme::dialog_report(ctx, title).unwrap_or_else(|| panic!("{what}: the dialog '{title}' was not drawn"));
    check_report(ctx, r, &format!("{what} '{title}'"));
    r
}

fn check_report(ctx: &egui::Context, r: theme::DialogReport, what: &str) {
    let area = theme::dialog_area(ctx);
    let screen = ctx.screen_rect();
    assert!(inside(area, screen), "{what}: the area {area:?} is inside the window {screen:?}");
    assert!(area.width() < screen.width() - theme::RIGHT_PANEL_W + 1.0, "{what}: the right panel is not in the area: {area:?}");
    assert!(inside(r.bounds, area.shrink(theme::DIALOG_EDGE)) && (r.bounds.width() - (area.width() - 32.0)).abs() < 0.6, "{what}: bounds are the area less 16 px: {:?}", r.bounds);
    assert!(inside(r.window, r.bounds), "{what}: the dialog {:?} fits in {:?}", r.window, r.bounds);
    assert!(inside(r.footer, r.window), "{what}: the footer {:?} is inside the dialog {:?}", r.footer, r.window);
    assert!(inside(r.footer, screen), "{what}: the footer {:?} is inside the window {screen:?}", r.footer);
    assert!(r.footer.top() >= r.body.bottom() - 0.5, "{what}: the footer {:?} is outside (below) the scrolling middle {:?}", r.footer, r.body);
}

#[test]
fn schema_dialog_with_40_columns_keeps_its_footer_inside_the_window_at_both_scales() {
    for (ppp, w, h) in SCREENS {
        let what = format!("ppp {ppp}, {w}x{h} pt");
        let mut app = test_app("r1schema");
        // a tab is open, as in real use: the tab strip and the right panel are there
        let src = parquet_source(&app.paths.root_dir.clone(), "loans", loans_frame());
        open_tab(&mut app, src, vec![]);
        let (ctx, input) = screen(ppp, w, h);
        app.pending_csv_modal = Some(schema_modal(5, 40, false));
        for frame in 0..8 {
            let _ = ctx.run(input.clone(), |c| app.frame_ui(c));
            // from the first visible frame on (the first is the invisible sizing pass)
            if frame >= 2 {
                assert_dialog_fits(&ctx, crate::ui::schema::SCHEMA_TITLE_CSV, &format!("{what}, frame {frame}"));
            }
        }
        let r = assert_dialog_fits(&ctx, crate::ui::schema::SCHEMA_TITLE_CSV, &what);
        // the footer is a row of real buttons, not an empty strip
        assert!(r.footer.height() >= 30.0, "{what}: {:?}", r.footer);
        // the dialog is as tall as the area allows: the 40 columns scroll inside it
        assert!(r.window.height() > r.bounds.height() - 40.0, "{what}: {:?} in {:?}", r.window, r.bounds);
        assert!(app.pending_csv_modal.is_some(), "still open");
    }
}

#[test]
fn a_filter_dialog_with_80_conditions_fits_at_both_scales_and_scrolls_inside() {
    for (ppp, w, h) in SCREENS {
        let what = format!("ppp {ppp}, {w}x{h} pt");
        let mut app = test_app("r1filter");
        let src = parquet_source(&app.paths.root_dir.clone(), "loans", loans_frame());
        open_tab(&mut app, src, vec![]);
        app.filter.conditions = (0..80)
            .map(|i| FilterCondition { column: format!("Col{i}"), op: CondOp::Equals, value: format!("{i}"), value2: String::new() })
            .collect();
        app.active_tool = ActiveToolModal::FilterRows;
        let (ctx, input) = screen(ppp, w, h);
        frames(&mut app, &ctx, &input, 8);
        let r = assert_dialog_fits(&ctx, "\u{E30C}  Filter rows", &what);
        assert!(r.window.height() > r.bounds.height() - 40.0, "{what}: tall content fills the dialog: {:?} in {:?}", r.window, r.bounds);
    }
}

/// A tab with 40 columns, so every tool modal has plenty to list.
fn open_wide_tab(app: &mut MicroParquetApp) {
    use polars::prelude::{NamedFrom, Series};
    let cols: Vec<_> = (0..40).map(|i| Series::new(format!("Column number {i}").into(), &[1.5f64 + i as f64, 2.5, 3.5]).into()).collect();
    let df = DataFrame::new(cols).expect("wide frame");
    let src = parquet_source(&app.paths.root_dir.clone(), "wide", df);
    open_tab(app, src, vec![]);
}

#[test]
fn every_tool_dialog_fits_the_area_at_every_scale() {
    use crate::ActiveToolModal as T;
    let tools = [
        T::FilterRows, T::SortRows, T::ManageColumns, T::ConditionalCol, T::MathFormula, T::NormalizeEntity,
        T::GroupByPivot, T::StackFiles, T::JoinTabs, T::ReconcilePeriods, T::MaturityBucket, T::RateShockSim,
        T::FtpCurveMap, T::BankingAudit, T::DurationAnalysis, T::LoanProvision, T::LoanLimitUtil, T::SaveMacro,
        T::PythonCodePreview, T::MultiTabExport, T::SplitByColumn,
    ];
    let mut problems = Vec::new();
    for (ppp, w, h) in SCREENS {
        for tool in tools {
            let mut app = test_app("r1tools");
            open_wide_tab(&mut app);
            app.active_tool = tool;
            let (ctx, input) = screen(ppp, w, h);
            frames(&mut app, &ctx, &input, 6);
            theme::clear_dialog_log(&ctx);
            frames(&mut app, &ctx, &input, 1);
            let log = theme::dialog_log(&ctx);
            if log.is_empty() {
                problems.push(format!("ppp {ppp} {w}x{h}: {tool:?} drew no dialog"));
            }
            for (title, r) in log {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check_report(&ctx, r, &format!("ppp {ppp} {w}x{h} {tool:?} '{title}'"))));
                if let Err(e) = res {
                    let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                    problems.push(msg);
                }
            }
        }
    }
    assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn the_batch_about_and_quit_dialogs_fit_too() {
    for (ppp, w, h) in SCREENS {
        let mut app = test_app("r1others");
        open_wide_tab(&mut app);
        app.batch_modal.open = true;
        app.show_about = true;
        app.quit_guard = true;
        let (ctx, input) = screen(ppp, w, h);
        frames(&mut app, &ctx, &input, 6);
        theme::clear_dialog_log(&ctx);
        frames(&mut app, &ctx, &input, 1);
        let log = theme::dialog_log(&ctx);
        assert_eq!(log.len(), 3, "ppp {ppp} {w}x{h}: {:?}", log.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>());
        for (title, r) in log {
            check_report(&ctx, r, &format!("ppp {ppp} {w}x{h} '{title}'"));
        }
    }
}

#[test]
fn every_dialog_comes_from_the_one_shared_helper() {
    // no screen builds its own egui::Window: all 27 dialogs use theme::modal(..).show(..)
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir("src/ui")
        .expect("src/ui")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    files.push("src/main.rs".into());
    let mut modals = 0;
    for f in files {
        let name = f.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let text = std::fs::read_to_string(&f).expect("read");
        if !matches!(name.as_str(), "theme.rs" | "layout_tests.rs" | "render_tests.rs" | "tests.rs" | "step_edit.rs") {
            assert!(!text.contains("egui::Window::new"), "{name} builds its own window");
        }
        modals += text.matches("theme::modal(").count();
    }
    assert!(modals >= 27, "{modals} dialogs use the helper");
}
