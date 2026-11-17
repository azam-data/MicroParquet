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

/// Every heading cell of a grid lies inside its frame, and so does what it may paint.
fn assert_grid_inside_frame(rep: &crate::ui::table::GridReport, what: &str) -> egui::Rect {
    let frame = rep.frame.unwrap_or_else(|| panic!("{what}: no frame"));
    for (kind, cells) in [("header", &rep.header), ("selector", &rep.selector), ("summary", &rep.summary)] {
        for (name, c) in cells.iter() {
            let visible = c.layout.intersect(c.clip);
            assert!(visible.width() < 0.0 || inside(visible, frame), "{what}: {kind} cell '{name}' {visible:?} reaches outside the frame {frame:?}");
            assert!(inside(c.clip, frame.expand(0.6)), "{what}: {kind} cell '{name}' may paint {:?}, outside the frame {frame:?}", c.clip);
        }
    }
    frame
}

#[test]
fn schema_preview_headings_never_pass_the_cards_right_edge_at_both_scales() {
    for (ppp, w, h) in SCREENS {
        let mut app = test_app("r2preview");
        let src = parquet_source(&app.paths.root_dir.clone(), "loans", loans_frame());
        open_tab(&mut app, src, vec![]);
        app.pending_csv_modal = Some(schema_modal(5, 40, false));
        let (ctx, input) = screen(ppp, w, h);
        frames(&mut app, &ctx, &input, 8);
        let win = theme::dialog_report(&ctx, crate::ui::schema::SCHEMA_TITLE_CSV).expect("dialog").window;
        // the preview (tab id 0): 40 headings, scrolled to the start, the middle and the end
        for x in [0.0f32, 700.0, 1600.0, 5000.0] {
            crate::ui::table::request_grid_scroll(&ctx, 0, x);
            frames(&mut app, &ctx, &input, 3);
            let what = format!("ppp {ppp} {w}x{h} scrolled to {x}");
            let rep = crate::ui::table::grid_report(&ctx, 0).unwrap_or_else(|| panic!("{what}: the preview was not drawn"));
            assert_eq!(rep.header.len(), 40, "{what}");
            let frame = assert_grid_inside_frame(&rep, &what);
            // the card's padding lies between the frame and the dialog's content edge
            assert!(frame.right() <= win.right() - theme::SPACE_M + 0.5 && frame.left() >= win.left() + theme::SPACE_M - 0.5, "{what}: the preview frame {frame:?} is inside the dialog {win:?}");
        }
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

// ------------------------------------------------------------------------------ the main table

/// A tab with 40 columns and `rows` rows (text, numbers and dates), like a real loan file.
fn open_table_tab(app: &mut MicroParquetApp, rows: usize) -> u64 {
    use polars::prelude::{NamedFrom, Series};
    let mut cols: Vec<_> = vec![Series::new("Account".into(), (0..rows).map(|i| format!("A{i:05}")).collect::<Vec<_>>()).into()];
    for c in 1..40 {
        if c % 4 == 0 {
            cols.push(Series::new(format!("Text column {c}").into(), (0..rows).map(|i| format!("Branch {} of {}", i % 7, c)).collect::<Vec<_>>()).into());
        } else {
            cols.push(Series::new(format!("Balance {c}").into(), (0..rows).map(|i| 1_000_000.0 + (i * c) as f64 * 1.25).collect::<Vec<_>>()).into());
        }
    }
    let df = DataFrame::new(cols).expect("table frame");
    let src = parquet_source(&app.paths.root_dir.clone(), "table40", df);
    let idx = open_tab(app, src, vec![]);
    app.tabs[idx].id
}

#[test]
fn the_main_tables_heading_block_stays_inside_its_frame_and_clear_of_the_right_panel() {
    for (ppp, w, h) in SCREENS {
        let mut app = test_app("r3table");
        let id = open_table_tab(&mut app, 300);
        let (ctx, input) = screen(ppp, w, h);
        frames(&mut app, &ctx, &input, 6);
        let area = theme::dialog_area(&ctx);
        let screen_rect = ctx.screen_rect();
        // the right panel is what lies between the area and the window's right edge
        let right_panel = egui::Rect::from_min_max(egui::pos2(area.right(), area.top()), egui::pos2(screen_rect.right(), screen_rect.bottom()));
        assert!(right_panel.width() >= theme::RIGHT_PANEL_W - 1.0, "ppp {ppp} {w}x{h}: right panel {right_panel:?}");
        // at the start, in the middle and at the far end of 40 columns
        for x in [0.0f32, 900.0, 2400.0, 99_999.0] {
            crate::ui::table::request_grid_scroll(&ctx, id, x);
            frames(&mut app, &ctx, &input, 3);
            let what = format!("ppp {ppp} {w}x{h} scrolled to {x}");
            let rep = crate::ui::table::grid_report(&ctx, id).unwrap_or_else(|| panic!("{what}: the table was not drawn"));
            assert_eq!(rep.header.len(), 39 + 1, "{what}");
            assert_eq!(rep.selector.len(), 40, "{what}: a selector under every heading");
            assert_eq!(rep.summary.len(), 40, "{what}: a summary under every heading");
            let frame = assert_grid_inside_frame(&rep, &what);
            assert!(inside(frame, area), "{what}: the frame {frame:?} is inside the area {area:?}");
            for (kind, cells) in [("heading", &rep.header), ("selector", &rep.selector), ("summary", &rep.summary)] {
                for (name, c) in cells {
                    assert!(!c.clip.intersects(right_panel) || c.clip.intersect(right_panel).width() < 0.5, "{what}: {kind} '{name}' may paint {:?} over the right panel {right_panel:?}", c.clip);
                }
            }
            // the three bands of a column are stacked: name, selector, summary
            for i in 0..rep.header.len() {
                let (h, s, m) = (rep.header[i].1.layout, rep.selector[i].1.layout, rep.summary[i].1.layout);
                assert!((h.left() - s.left()).abs() < 0.5 && (h.left() - m.left()).abs() < 0.5 && (h.width() - m.width()).abs() < 0.5, "{what}: column {i} bands line up");
                assert!(h.bottom() <= s.top() + 0.5 && s.bottom() <= m.top() + 0.5, "{what}: column {i} bands are stacked");
            }
        }
    }
}

#[test]
fn the_heading_block_stays_put_while_the_rows_scroll_up_and_down() {
    use crate::ui::layout_tests::collect_painted;
    for (ppp, w, h) in SCREENS {
        let mut app = test_app("r3scroll");
        let id = open_table_tab(&mut app, 500);
        let (ctx, input) = screen(ppp, w, h);
        frames(&mut app, &ctx, &input, 6);
        let painted = |app: &mut MicroParquetApp| {
            let out = ctx.run(input.clone(), |c| app.frame_ui(c));
            collect_painted(&out.shapes)
        };
        let before = crate::ui::table::grid_report(&ctx, id).expect("table");
        let p0 = painted(&mut app);
        assert!(p0.texts.iter().any(|(t, _, _)| t == "A00000"), "ppp {ppp}: the first row is shown before scrolling");
        crate::ui::table::request_grid_scroll_y(&ctx, id, 4000.0);
        frames(&mut app, &ctx, &input, 3);
        let p1 = painted(&mut app);
        assert!(!p1.texts.iter().any(|(t, _, _)| t == "A00000"), "ppp {ppp}: the first row has scrolled away");
        assert!(p1.texts.iter().any(|(t, _, _)| t.starts_with('A') && t.len() == 6 && t != "A00000"), "ppp {ppp}: later rows are shown");
        let after = crate::ui::table::grid_report(&ctx, id).expect("table");
        for i in 0..before.header.len() {
            let (b, a) = (before.header[i].1.layout, after.header[i].1.layout);
            assert!((b.top() - a.top()).abs() < 0.5 && (b.bottom() - a.bottom()).abs() < 0.5, "ppp {ppp}: heading {i} moved from {b:?} to {a:?}");
            let (b, a) = (before.summary[i].1.layout, after.summary[i].1.layout);
            assert!((b.top() - a.top()).abs() < 0.5, "ppp {ppp}: summary {i} moved from {b:?} to {a:?}");
            let (b, a) = (before.selector[i].1.layout, after.selector[i].1.layout);
            assert!((b.top() - a.top()).abs() < 0.5, "ppp {ppp}: selector {i} moved from {b:?} to {a:?}");
        }
    }
}
