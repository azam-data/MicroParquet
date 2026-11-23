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

pub(crate) fn inside(inner: egui::Rect, outer: egui::Rect) -> bool {
    inner.left() >= outer.left() - 0.5 && inner.right() <= outer.right() + 0.5 && inner.top() >= outer.top() - 0.5 && inner.bottom() <= outer.bottom() + 0.5
}

/// A dialog's rectangles must lie in the area the panels leave, less 16 px.
pub(crate) fn assert_dialog_fits(ctx: &egui::Context, title: &str, what: &str) -> theme::DialogReport {
    let r = theme::dialog_report(ctx, title).unwrap_or_else(|| panic!("{what}: the dialog '{title}' was not drawn"));
    check_report(ctx, r, &format!("{what} '{title}'"));
    r
}

fn check_report(ctx: &egui::Context, r: theme::DialogReport, what: &str) {
    let area = theme::dialog_area(ctx);
    let screen = ctx.screen_rect();
    assert!(inside(area, screen), "{what}: the area {area:?} is inside the window {screen:?}");
    // the area is the window less the top bar, the tab strip and the status bar: it covers the right panel
    assert!((area.width() - screen.width()).abs() < 0.5, "{what}: the area spans the window's width (the right panel too): {area:?}");
    assert!(area.top() >= theme::TOP_BAR_H + theme::TAB_STRIP_H - 0.5, "{what}: the area starts under the top bar and tab strip: {area:?}");
    assert!(area.bottom() <= screen.bottom() - theme::STATUS_BAR_H + 0.5, "{what}: the area ends above the status bar: {area:?}");
    assert!(inside(r.bounds, area.shrink(theme::DIALOG_EDGE)) && (r.bounds.width() - (area.width() - 32.0)).abs() < 0.6, "{what}: bounds are the area less 16 px: {:?}", r.bounds);
    assert!(inside(r.window, r.bounds), "{what}: the dialog {:?} fits in {:?}", r.window, r.bounds);
    assert!(inside(r.footer, r.window), "{what}: the footer {:?} is inside the dialog {:?}", r.footer, r.window);
    assert!(inside(r.footer, screen), "{what}: the footer {:?} is inside the window {screen:?}", r.footer);
    assert!(r.footer.top() >= r.body.bottom() - 0.5, "{what}: the footer {:?} is outside (below) the scrolling middle {:?}", r.footer, r.body);
}

#[test]
fn schema_dialog_with_40_columns_keeps_its_header_and_cards_inside_the_window_at_both_scales() {
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
        // this dialog has no bottom footer: its buttons are in the header
        assert!(!r.footer.is_positive(), "{what}: no footer expected: {:?}", r.footer);
        assert!(r.header.is_positive() && inside(r.header, r.window), "{what}: header {:?} in {:?}", r.header, r.window);
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
fn a_modal_dialog_may_cover_the_right_panel_and_the_schema_dialog_is_not_squeezed() {
    // 105%: the 1280x720 px window is 1219x686 pt; the right panel is 316 wide
    let (ppp, w, h) = (1.05, 1219.05, 685.71);
    let mut app = test_app("r1cover");
    let src = parquet_source(&app.paths.root_dir.clone(), "loans", loans_frame());
    open_tab(&mut app, src, vec![]);
    app.pending_csv_modal = Some(schema_modal(5, 40, false));
    let (ctx, input) = screen(ppp, w, h);
    frames(&mut app, &ctx, &input, 8);
    let r = assert_dialog_fits(&ctx, crate::ui::schema::SCHEMA_TITLE_CSV, "105%");
    let area = theme::dialog_area(&ctx);
    let central = theme::central_area(&ctx);
    assert!((area.width() - 1219.05).abs() < 0.5 && (r.bounds.width() - (1219.05 - 32.0)).abs() < 0.6, "the dialog may be {} wide at most: {:?}", 1219.05 - 32.0, r.bounds);
    assert!(central.right() < area.right() - theme::RIGHT_PANEL_W + 1.0, "the right panel lies beside the central area: {central:?}");
    // the Schema dialog asks for 1040 + the frame: it gets it, more than the 871 the central area would allow
    assert!(r.window.width() > 1000.0 && r.window.width() <= 1187.5, "{:?}", r.window);
    assert!(r.window.right() > central.right(), "it covers the right panel: dialog {:?}, central area {central:?}", r.window);
    assert!(inside(r.footer, area.shrink(theme::DIALOG_EDGE)), "the footer {:?} is inside the area {area:?}", r.footer);
    // on screen
    assert!(r.footer.right() <= ctx.screen_rect().right() && r.footer.bottom() <= ctx.screen_rect().bottom() - theme::STATUS_BAR_H);
}

#[test]
fn a_filter_dialog_with_80_conditions_fits_at_both_scales_and_scrolls_inside() {
    for (ppp, w, h) in SCREENS {
        let what = format!("ppp {ppp}, {w}x{h} pt");
        let mut app = test_app("r1filter");
        let src = parquet_source(&app.paths.root_dir.clone(), "loans", loans_frame());
        open_tab(&mut app, src, vec![]);
        app.filter.conditions = (0..80)
            .map(|i| FilterCondition { column: format!("Col{i}"), op: CondOp::Equals, value: format!("{i}"), value2: String::new(), ..Default::default() })
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
        T::GroupByPivot, T::StackFiles, T::JoinTabs, T::ReconcilePeriods, T::MaturityBucket, T::RateBand, T::RateShockSim,
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
        if !matches!(name.as_str(), "theme.rs" | "layout_tests.rs" | "render_tests.rs" | "render_tests_p4.rs" | "tests.rs" | "step_edit.rs") {
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
        let area = theme::central_area(&ctx);
        let screen_rect = ctx.screen_rect();
        // the right panel is what lies between the central area and the window's right edge
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

// ------------------------------------------------------------------------ row-count warning

fn open_csv_tab(app: &mut MicroParquetApp, name: &str, body: &str) {
    let p = app.paths.root_dir.join(name);
    std::fs::write(&p, body).expect("csv");
    let config = micro_parquet::engine::auto_detect_smart_schema(&p).expect("schema");
    open_tab(app, TabSource::SingleFile { path: p, is_parquet: false, config }, vec![]);
}

fn csv_body(rows: usize, multiline_at: &[usize], malformed_at: &[usize]) -> String {
    let mut s = String::from("Account,Name,Balance\n");
    for i in 0..rows {
        if multiline_at.contains(&i) {
            s.push_str(&format!("A{i},\"Rahim Trading\nDhaka branch {i}\",{}.25\n", 1000 + i));
        } else if malformed_at.contains(&i) {
            s.push_str(&format!("A{i},{}\n", "x".repeat(100)));
        } else {
            s.push_str(&format!("A{i},Customer {i},{}.25\n", 1000 + i));
        }
    }
    s
}

#[test]
fn the_row_count_note_is_grey_when_quotes_explain_it_and_a_warning_when_not_with_a_details_button() {
    use crate::ui::layout_tests::collect_painted;
    let tk = theme::LIGHT;
    for (ppp, w, h) in SCREENS.iter().copied().chain([(1.05, 1291.0, 700.0)]) {
        let (ctx, input) = screen(ppp, w, h);
        let paint = |app: &mut MicroParquetApp| {
            let mut p = None;
            for _ in 0..4 {
                let out = ctx.run(input.clone(), |c| app.frame_ui(c));
                p = Some(collect_painted(&out.shapes));
            }
            p.expect("frames")
        };
        // only line breaks inside quotes: the line-break chip, and no row-count warning
        let mut app = test_app("r4grey");
        open_csv_tab(&mut app, "grey.csv", &csv_body(1300, &[10, 500, 1200], &[]));
        let p = paint(&mut app);
        // (K1: the note is the chip "⚠ 3 line breaks · Details"; the long words are on its hover and in the load message)
        let c = p.colors.iter().find(|(t, _)| t.ends_with(" 3 line breaks")).unwrap_or_else(|| panic!("ppp {ppp}: the chip is not painted: {:?}", p.texts.iter().map(|(t, _, _)| t.clone()).filter(|t| t.contains("break")).collect::<Vec<_>>()));
        assert_eq!(c.1, tk.warning, "ppp {ppp}: the chip is a warning until the tab cleans the breaks");
        assert!(p.texts.iter().any(|(t, _, _)| t == "Details"), "ppp {ppp}: a Details button");
        // a malformed line as well: a warning in the warning colour
        let mut app = test_app("r4warn");
        open_csv_tab(&mut app, "warn.csv", &csv_body(40, &[4, 11], &[7]));
        let p = paint(&mut app);
        let warn = p.colors.iter().find(|(t, _)| t.contains("Rows read 40 vs file data lines 42")).unwrap_or_else(|| panic!("ppp {ppp}: warning not painted"));
        assert_eq!(warn.1, tk.warning, "ppp {ppp}");
        assert!(p.texts.iter().any(|(t, _, _)| t == "Details"));
        // Details: the dialog lists the lines, with the start of each, and fits
        app.row_details_open = true;
        frames(&mut app, &ctx, &input, 6);
        let r = theme::dialog_report(&ctx, "Row count details").unwrap_or_else(|| panic!("ppp {ppp}: the details dialog was not drawn"));
        check_report(&ctx, r, &format!("ppp {ppp} details"));
        let p = paint(&mut app);
        for want in ["Line 6", "Line 10", "Line 14"] {
            assert!(p.texts.iter().any(|(t, _, _)| t == want), "ppp {ppp}: '{want}' is listed");
        }
        assert!(p.texts.iter().any(|(t, _, _)| t == "A4,\"Rahim Trading"), "ppp {ppp}: the start of the line is shown");
    }
}

// ------------------------------------------------------------------------------- status bar

/// Every width the window can have: the four test screens and the smallest window (800 px at 105%).
fn status_screens() -> Vec<(f32, f32, f32)> {
    let mut v = SCREENS.to_vec();
    v.push((1.05, 800.0 / 1.05, 560.0 / 1.05));
    // the owner's real window at 105%
    v.push((1.05, 1291.0, 700.0));
    v
}

#[test]
fn the_status_bar_never_passes_the_window_width_and_cuts_a_long_message_with_dots() {
    use crate::ui::layout_tests::collect_painted;
    use crate::ui::status::status_report;
    let long = "Exported multi-tab Excel workbook to \"C:\\Users\\Treasury\\Documents\\ALCO packs\\2026-11\\ALCO_pack_final_v7.xlsx\": 3 tab(s) on 3 sheet(s); \u{26A0} 'Loans' was exported WITHOUT its totals row (the data is on its sheet): The totals row could not be computed (also on the Export_Errors sheet)";
    for (ppp, w, h) in status_screens() {
        let what = format!("ppp {ppp} {w}x{h}");
        let mut app = test_app("r5status");
        // a tab with a note that is long (a row-count explanation) and a job that is running
        open_csv_tab(&mut app, "s.csv", &csv_body(1300, &[10, 500, 1200], &[]));
        app.jobs = JobQueue::new(|| {});
        let (up_tx, up_rx) = std::sync::mpsc::channel::<()>();
        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
        app.jobs.submit(JobKey::unique(JobKind::Load), "Opening loans.csv", true, move |ctx| {
            ctx.set_stages(&["Reading", "Parsing", "Checking"]);
            ctx.set_stage(1);
            ctx.set_bytes("Counting the file's lines", 1_048_576, 4_194_304);
            let _ = up_tx.send(());
            let _ = go_rx.recv_timeout(std::time::Duration::from_secs(20));
            Ok(JobOutput::Exported(micro_parquet::tab_jobs::ExportOutcome { message: String::new(), path_used: None, written: None }))
        });
        up_rx.recv_timeout(std::time::Duration::from_secs(10)).expect("job runs");
        app.status_message = long.to_string();
        let (ctx, input) = screen(ppp, w, h);
        let mut painted = None;
        for _ in 0..5 {
            let out = ctx.run(input.clone(), |c| app.frame_ui(c));
            painted = Some(collect_painted(&out.shapes));
        }
        let p = painted.expect("frames");
        let rep = status_report(&ctx).expect("status bar");
        let screen_rect = ctx.screen_rect();
        assert!(inside(rep.bar, screen_rect), "{what}: bar {:?} in window {screen_rect:?}", rep.bar);
        assert!(inside(rep.messages, rep.bar) && rep.messages.left() >= rep.left.right() - 0.5, "{what}: the Messages button {:?} sits right of {:?}", rep.messages, rep.left);
        assert!(rep.content.right() <= rep.left.right() + 0.5, "{what}: the items {:?} stay left of the button, inside {:?}: {:?}", rep.content, rep.left, p.texts.iter().filter(|(_, pos, _)| pos.y >= rep.bar.top()).map(|(t, pos, _)| (t.clone(), pos.x)).collect::<Vec<_>>());
        // every text painted in the bar ends inside the window
        let bar_top = rep.bar.top();
        for (t, pos, _) in p.texts.iter().filter(|(_, pos, _)| pos.y >= bar_top - 1.0) {
            assert!(pos.x >= -0.5 && pos.x < screen_rect.right(), "{what}: '{t}' starts at {pos:?}, outside the window");
        }
        // the long message is cut with "…" and ends before the Messages button
        let cut = p.shown.iter().find(|(t, pos)| pos.y >= bar_top - 1.0 && t.ends_with('\u{2026}') && t.starts_with("Exported"));
        // in the smallest window the line-break chip, the control total and the running job leave no room
        // for the message (it gives way first); the chip and Cancel are still whole
        if w * ppp < 900.0 {
            let shown: Vec<&str> = p.texts.iter().filter(|(_, pos, _)| pos.y >= bar_top - 1.0).map(|(t, _, _)| t.as_str()).collect();
            assert!(shown.contains(&"Details") && shown.contains(&"Cancel"), "{what}: the chip's Details and Cancel stay: {shown:?}");
            let _ = go_tx.send(());
            continue;
        }
        assert!(cut.is_some(), "{what}: the long message ends in '…': {:?}", p.shown.iter().filter(|(_, pos)| pos.y >= bar_top).map(|(t, _)| t.clone()).collect::<Vec<_>>());
        let _ = go_tx.send(());
    }
}

#[test]
fn a_short_message_is_whole_and_the_full_text_shows_on_hover_when_it_is_cut() {
    use crate::ui::layout_tests::collect_painted;
    let (ctx, input) = screen(1.05, 1219.05, 685.71);
    let mut app = test_app("r5hover");
    open_wide_tab(&mut app);
    app.status_message = "Copied formatted table to clipboard.".into();
    frames(&mut app, &ctx, &input, 4);
    let out = ctx.run(input.clone(), |c| app.frame_ui(c));
    let p = collect_painted(&out.shapes);
    assert!(p.texts.iter().any(|(t, _, _)| t == "Copied formatted table to clipboard."), "a short message is shown whole");
    // a long one is cut; hovering it shows all of it
    let long = "x".repeat(400) + " THE END";
    app.status_message = long.clone();
    frames(&mut app, &ctx, &input, 4);
    let out = ctx.run(input.clone(), |c| app.frame_ui(c));
    let p = collect_painted(&out.shapes);
    let (cut, pos) = p.shown.iter().find(|(t, _)| t.starts_with("xxxx") && t.ends_with('\u{2026}')).expect("the cut message").clone();
    assert!(cut.len() < long.len());
    let mut shown = false;
    for i in 0..30 {
        let mut inp = input.clone();
        inp.time = Some(10.0 + i as f64 * 0.05);
        inp.predicted_dt = 0.05;
        inp.events = vec![egui::Event::PointerMoved(pos + egui::vec2(20.0, 6.0))];
        let out = ctx.run(inp, |c| app.frame_ui(c));
        shown = collect_painted(&out.shapes).texts.iter().any(|(t, _, _)| *t == long);
    }
    assert!(shown, "the whole text is the tooltip of the cut message");
}

#[test]
fn messages_keeps_the_last_fifty_newest_first() {
    use crate::ui::layout_tests::collect_painted;
    for (ppp, w, h) in SCREENS {
        let mut app = test_app("r5log");
        open_wide_tab(&mut app);
        let (ctx, input) = screen(ppp, w, h);
        for i in 0..60 {
            app.status_message = format!("message number {i}");
            frames(&mut app, &ctx, &input, 1);
        }
        assert_eq!(app.status_log.len(), 50, "ppp {ppp}");
        // the same message twice in a row is kept once
        frames(&mut app, &ctx, &input, 3);
        assert_eq!(app.status_log.len(), 50);
        assert_eq!(app.status_log.last().map(|(_, t)| t.as_str()), Some("message number 59"));
        assert_eq!(app.status_log.first().map(|(_, t)| t.as_str()), Some("message number 10"));
        app.messages_open = true;
        frames(&mut app, &ctx, &input, 6);
        let r = theme::dialog_report(&ctx, "Messages").unwrap_or_else(|| panic!("ppp {ppp}: Messages not drawn"));
        check_report(&ctx, r, &format!("ppp {ppp} Messages"));
        let out = ctx.run(input.clone(), |c| app.frame_ui(c));
        let p = collect_painted(&out.shapes);
        // newest first
        let bar_top = crate::ui::status::status_report(&ctx).expect("status bar").bar.top();
        let y = |n: usize| p.texts.iter().find(|(t, pos, _)| *t == format!("message number {n}") && pos.y < bar_top).map(|(_, pos, _)| pos.y);
        assert!(y(59).is_some(), "ppp {ppp}: the newest is listed");
        if let (Some(a), Some(b)) = (y(59), y(58)) {
            assert!(a < b, "ppp {ppp}: newest first");
        }
        assert!(y(5).is_none(), "ppp {ppp}: the oldest ones are gone");
    }
}

// ------------------------------------------------------------------------------- blank cells

#[test]
fn blank_cells_show_a_muted_dash_never_the_word_null() {
    use crate::ui::layout_tests::collect_painted;
    use polars::prelude::{NamedFrom, Series};
    let tk = theme::LIGHT;
    for (ppp, w, h) in SCREENS {
        let mut app = test_app("r6blank");
        let df = DataFrame::new(vec![
            Series::new("Name".into(), &[Some("Rahim"), None, Some("null"), Some("Karim")]).into(),
            Series::new("Balance".into(), &[Some(1_500_000.0f64), None, Some(2.5), None]).into(),
            Series::new("Branch no".into(), &[None::<i64>, Some(12), None, Some(7)]).into(),
        ])
        .expect("frame");
        let src = parquet_source(&app.paths.root_dir.clone(), "blanks", df);
        open_tab(&mut app, src, vec![]);
        let (ctx, input) = screen(ppp, w, h);
        frames(&mut app, &ctx, &input, 5);
        let out = ctx.run(input.clone(), |c| app.frame_ui(c));
        let p = collect_painted(&out.shapes);
        let area = theme::central_area(&ctx);
        let in_table = |pos: &egui::Pos2| area.contains(*pos);
        // the blanks: 1 in Name, 2 in Balance, 2 in Branch no
        let dashes: Vec<&(String, egui::Color32)> = p.colors.iter().filter(|(t, _)| t == theme::BLANK_CELL).collect();
        assert!(dashes.len() >= 5, "ppp {ppp}: {} dashes drawn", dashes.len());
        assert!(dashes.iter().all(|(_, c)| *c == tk.muted), "ppp {ppp}: the dashes are muted");
        // the word null appears only as the data of the cell that really says "null", in the ink colour
        let nulls: Vec<_> = p.texts.iter().zip(p.colors.iter()).filter(|((t, pos, _), _)| t == "null" && in_table(pos)).collect();
        assert_eq!(nulls.len(), 1, "ppp {ppp}: only the text that says null: {nulls:?}");
        assert_eq!(nulls[0].1 .1, tk.ink, "ppp {ppp}: real data keeps the ink colour");
    }
}

#[test]
fn exports_keep_blanks_as_they_were() {
    // the table's dash is only how the grid shows a blank: the clipboard text and exports are not
    // touched (a blank is still written as it was before)
    use polars::prelude::{NamedFrom, Series};
    let df = DataFrame::new(vec![Series::new("A".into(), &[Some(1.0f64), None]).into()]).expect("frame");
    let txt = micro_parquet::exporters::format_df_for_clipboard(&df, &HashMap::new(), NumberDisplayFormat::Raw, 2, false);
    assert!(!txt.contains(theme::BLANK_CELL), "{txt}");
}

// -------------------------------------------------------------------------- delta vs source

#[test]
fn delta_vs_source_is_grey_after_filters_and_a_warning_only_without_one() {
    use crate::ui::layout_tests::collect_painted;
    let tk = theme::LIGHT;
    let filter = || TransformStep::FilterRows { column: "Branch".into(), operator: FilterOp::Eq, value: "DHK".into(), keep_matching: true };
    for (ppp, w, h) in SCREENS {
        let (ctx, input) = screen(ppp, w, h);
        let paint = |app: &mut MicroParquetApp| {
            let mut p = None;
            for _ in 0..4 {
                let out = ctx.run(input.clone(), |c| app.frame_ui(c));
                p = Some(collect_painted(&out.shapes));
            }
            p.expect("frames")
        };
        let colour = |p: &crate::ui::layout_tests::Painted, t: &str| p.colors.iter().find(|(x, _)| x == t).map(|(_, c)| *c);
        let new_app = |name: &str, steps: Vec<TransformStep>| {
            let mut app = test_app(name);
            let src = parquet_source(&app.paths.root_dir.clone(), "loans", loans_frame());
            open_tab(&mut app, src, steps);
            app
        };
        // 1. a filter: the total is 1,000,000 below the source's; label says why and everything is grey
        let mut app = new_app("r7filter", vec![filter()]);
        assert!(app.tabs[app.active_tab].has_active_filter());
        let p = paint(&mut app);
        assert_eq!(colour(&p, "\u{394} vs source (after filters)"), Some(tk.muted), "ppp {ppp}: the label is grey");
        let value = p.colors.iter().find(|(t, _)| t.ends_with(" Cr") && t.starts_with('-')).unwrap_or_else(|| panic!("ppp {ppp}: the difference is shown: {:?}", p.colors.iter().map(|c| c.0.clone()).filter(|t| t.contains("Cr")).collect::<Vec<_>>()));
        assert_eq!(value.1, tk.muted, "ppp {ppp}: the difference is grey, not a warning");
        assert!(colour(&p, "\u{394} vs source").is_none(), "ppp {ppp}: only the filtered label");
        // 2. a filter that is turned off does not count
        let off = TransformStep::Skipped { step: Box::new(filter()) };
        let mut app = new_app("r7off", vec![off]);
        assert!(!app.tabs[app.active_tab].has_active_filter());
        let p = paint(&mut app);
        assert_eq!(colour(&p, "\u{394} vs source"), Some(tk.muted), "ppp {ppp}: plain label without an active filter");
        // 3. no filter and the total moved: a warning
        let mut app = new_app("r7warn", vec![]);
        let idx = app.active_tab;
        app.tabs[idx].baseline_control_sum = app.tabs[idx].control_sum.map(|s| s - 25_000_000.0);
        let p = paint(&mut app);
        assert_eq!(colour(&p, "\u{394} vs source"), Some(tk.muted), "ppp {ppp}");
        let value = p.colors.iter().find(|(t, _)| t.starts_with('+') && t.ends_with(" Cr")).unwrap_or_else(|| panic!("ppp {ppp}: +2.50 Cr shown"));
        assert_eq!((value.0.as_str(), value.1), ("+2.50 Cr", tk.warning), "ppp {ppp}");
        // 4. no filter and nothing moved: the green zero
        let mut app = new_app("r7zero", vec![]);
        let p = paint(&mut app);
        assert_eq!(colour(&p, "0.00 Cr"), Some(tk.success), "ppp {ppp}");
    }
}


// ------------------------------------------------------------ part 4, item 1: schema dialog header

use crate::ui::layout_tests::collect_painted;
use crate::ui::schema::{SchemaKeys, SchemaOutcome};

fn key(k: egui::Key) -> Vec<egui::Event> {
    vec![egui::Event::Key { key: k, physical_key: Some(k), pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }]
}

/// The box of the text `t` painted in this frame.
fn box_of(p: &crate::ui::layout_tests::Painted, t: &str) -> egui::Rect {
    p.boxes.iter().find(|(x, _)| x.contains(t)).map(|(_, r)| *r).unwrap_or_else(|| panic!("'{t}' was not painted: {:?}", p.boxes.iter().map(|b| b.0.clone()).collect::<Vec<_>>()))
}

#[test]
fn schema_dialog_buttons_are_in_the_header_right_aligned_and_the_cards_come_in_order() {
    for (ppp, w, h) in [(1.05, 1219.05, 685.71), (0.8, 1600.0, 900.0)] {
        let what = format!("ppp {ppp}, {w}x{h} pt");
        let mut app = test_app("p4header");
        let src = parquet_source(&app.paths.root_dir.clone(), "loans", loans_frame());
        open_tab(&mut app, src, vec![]);
        app.pending_csv_modal = Some(schema_modal(5, 40, false));
        let (ctx, input) = screen(ppp, w, h);
        ctx.set_embed_viewports(false); // as eframe's windows: the Pop out button is offered
        let mut p = None;
        for _ in 0..8 {
            let out = ctx.run(input.clone(), |c| app.frame_ui(c));
            p = Some(collect_painted(&out.shapes));
        }
        let p = p.expect("frames");
        let r = assert_dialog_fits(&ctx, crate::ui::schema::SCHEMA_TITLE_CSV, &what);
        let (lock, convert, cancel) = (box_of(&p, "Lock Schema & Open"), box_of(&p, "Convert to Parquet"), box_of(&p, "Cancel"));
        let pop = box_of(&p, "Pop out");
        eprintln!("{what}: window {:?}\n  header {:?}\n  Pop out {pop:?}\n  Lock {lock:?}\n  Convert {convert:?}\n  Cancel {cancel:?}\n  body {:?}", r.window, r.header, r.body);
        // in the header row, left to right: Lock, Convert, Cancel, at its right edge
        for (n, b) in [("Pop out", pop), ("Lock", lock), ("Convert", convert), ("Cancel", cancel)] {
            assert!(inside(b, r.header.expand2(egui::vec2(0.0, 6.0))), "{what}: {n} {b:?} is in the header {:?}", r.header);
            assert!((b.center().y - r.header.center().y).abs() < 6.0, "{what}: {n} is on the header row");
        }
        assert!(pop.right() <= lock.left() && lock.right() <= convert.left() && convert.right() <= cancel.left(), "{what}: order Pop out, Lock, Convert, Cancel");
        // right-aligned: Cancel ends at the dialog's content edge (its button padding is a few points)
        assert!(r.header.right() - cancel.right() < 16.0 && r.header.right() - cancel.right() > 0.0, "{what}: Cancel {cancel:?} at the right edge of {:?}", r.header);
        // the middle starts under the header, and nothing but the middle's scroll bar is below it
        assert!(r.body.top() >= r.header.bottom() - 0.5, "{what}: body {:?} under header {:?}", r.body, r.header);
        assert!(r.window.bottom() - r.body.bottom() < 2.0 * theme::SPACE_M + 14.0, "{what}: no footer strip under the middle: {:?} {:?}", r.window, r.body);
        // no button at the bottom any more
        assert!(cancel.bottom() < r.body.top(), "{what}: Cancel is above the cards");
        // cards: Reading options, Columns, Preview from top to bottom
        let exact = |t: &str| p.boxes.iter().find(|(x, b)| x == t && b.left() >= r.window.left() && b.right() <= r.window.right()).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: heading '{t}' not drawn"));
        let (ro, co) = (exact("Reading options"), exact("Columns"));
        assert!(ro.top() < co.top() && co.bottom() < r.window.bottom(), "{what}: order {ro:?} {co:?}");
        // The Columns card keeps its minimum height, so in a short window (the 686 pt one) the
        // Preview under it is below the fold: the middle scrolls to it (egui does not paint what
        // is scrolled out). In a tall window it is in view, last.
        if h > 800.0 {
            let pv = exact("Preview");
            assert!(co.top() < pv.top() && pv.top() - co.top() >= theme::SCHEMA_COLUMNS_MIN_H - 1.0, "{what}: Columns keeps at least its minimum: {co:?} {pv:?}");
            assert!(pv.bottom() <= r.window.bottom(), "{what}: Preview heading {pv:?} is inside the dialog {:?}", r.window);
        } else {
            assert!(p.boxes.iter().all(|(t, _)| t != "Preview"), "{what}: the Preview is scrolled out of view");
        }
    }
}

#[test]
fn enter_locks_the_schema_and_esc_cancels_it() {
    for (key_pressed, opens) in [(egui::Key::Enter, true), (egui::Key::Escape, false)] {
        let mut app = test_app("p4keys");
        let src = parquet_source(&app.paths.root_dir.clone(), "loans", loans_frame());
        open_tab(&mut app, src, vec![]);
        app.status_message.clear();
        app.pending_csv_modal = Some(schema_modal(5, 6, false));
        let (ctx, input) = screen(1.05, 1219.05, 685.71);
        frames(&mut app, &ctx, &input, 5);
        assert!(app.pending_csv_modal.is_some());
        let mut with_key = input.clone();
        with_key.events = key(key_pressed);
        let _ = ctx.run(with_key, |c| app.frame_ui(c));
        assert!(app.pending_csv_modal.is_none(), "{key_pressed:?} closes the dialog");
        // Enter starts opening the file in a tab (the job says "Loading…"), Esc starts nothing
        assert_eq!(app.status_message.starts_with("Loading"), opens, "{key_pressed:?}: status '{}'", app.status_message);
    }
}

#[test]
fn keys_decide_only_when_no_button_was_clicked_and_convert_only_dialogs_convert() {
    let mut modal = schema_modal(1, 3, false);
    let enter = SchemaKeys { enter: true, esc: false };
    let esc = SchemaKeys { enter: false, esc: true };
    let mut out = SchemaOutcome::default();
    enter.apply(&modal, &mut out);
    assert_eq!(out, SchemaOutcome { lock: true, ..Default::default() });
    let mut out = SchemaOutcome::default();
    esc.apply(&modal, &mut out);
    assert_eq!(out, SchemaOutcome { cancel: true, ..Default::default() });
    // a click in the same frame wins
    let mut out = SchemaOutcome { reread: true, ..Default::default() };
    enter.apply(&modal, &mut out);
    assert_eq!(out, SchemaOutcome { reread: true, ..Default::default() });
    // the dialog opened only to convert has no Lock button: Enter converts
    modal.direct_convert_only = true;
    let mut out = SchemaOutcome::default();
    enter.apply(&modal, &mut out);
    assert_eq!(out, SchemaOutcome { convert: true, ..Default::default() });
}

#[test]
fn enter_in_a_text_field_or_with_a_list_open_does_not_close_the_dialog() {
    let (ctx, input) = screen(1.05, 1219.05, 685.71);
    // a focused widget (a button reached with Tab, a field being edited) takes Enter itself
    let id = egui::Id::new("some_field");
    let mut with_key = input.clone();
    with_key.events = key(egui::Key::Enter);
    let mut keys = None;
    let _ = ctx.run(input.clone(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut s = String::new();
            let r = ui.add(egui::TextEdit::singleline(&mut s).id(id));
            r.request_focus();
        });
    });
    let _ = ctx.run(with_key, |ctx| {
        keys = Some(SchemaKeys::read(ctx));
    });
    let k = keys.expect("read");
    assert!(!k.enter, "Enter belongs to the text field");
}

#[test]
fn pop_out_shows_a_button_only_where_native_windows_work_and_docks_back_where_they_do_not() {
    let (ctx, input) = screen(1.05, 1219.05, 685.71);
    let mut app = test_app("p4pop");
    app.pending_csv_modal = Some(schema_modal(5, 6, false));
    // a headless context embeds viewports: no native windows, so no Pop out button
    assert!(ctx.embed_viewports());
    let mut p = None;
    for _ in 0..4 {
        let out = ctx.run(input.clone(), |c| app.frame_ui(c));
        p = Some(collect_painted(&out.shapes));
    }
    assert!(!p.expect("p").texts.iter().any(|(t, _, _)| t.contains("Pop out")), "no Pop out when windows cannot be opened");
    // a native backend (eframe) turns embedding off: the button is there; a click pops the dialog out
    ctx.set_embed_viewports(false);
    let mut p = None;
    for _ in 0..3 {
        let out = ctx.run(input.clone(), |c| app.frame_ui(c));
        p = Some(collect_painted(&out.shapes));
    }
    let b = box_of(&p.expect("p"), "Pop out");
    let click = |pressed: bool| egui::Event::PointerButton { pos: b.center(), button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    let mut run = |events: Vec<egui::Event>, app: &mut MicroParquetApp| {
        let mut i = input.clone();
        i.events = events;
        let _ = ctx.run(i, |c| app.frame_ui(c));
    };
    run(vec![egui::Event::PointerMoved(b.center())], &mut app);
    run(vec![click(true)], &mut app);
    run(vec![click(false)], &mut app);
    // this backend has no renderer for a second window, so the dialog is told "embedded" and docks
    // back at once; a real one keeps it out until Dock or closing the window
    assert!(app.pending_csv_modal.is_some(), "the dialog is not lost");
    run(vec![], &mut app);
    run(vec![], &mut app);
    let m = app.pending_csv_modal.as_ref().expect("modal");
    assert!(!m.popped_out, "docked back: no second window on this backend");
}

#[test]
fn the_pop_out_window_has_the_same_header_and_cards_at_its_default_and_maximised_sizes() {
    for (ppp, w, h) in [(1.0, 1100.0, 800.0), (0.8, 1600.0, 900.0), (1.05, 1219.05, 685.71), (1.0, 1920.0, 1080.0)] {
        let what = format!("ppp {ppp}, pop-out {w}x{h} pt");
        let mut app = test_app("p4viewport");
        let mut modal = schema_modal(5, 40, false);
        modal.popped_out = true;
        let (ctx, input) = screen(ppp, w, h);
        let mut rects = None;
        let mut p = None;
        for _ in 0..6 {
            let mut out = SchemaOutcome::default();
            let o = ctx.run(input.clone(), |c| rects = Some(app.schema_viewport_ui(c, &mut modal, &mut out)));
            p = Some(collect_painted(&o.shapes));
        }
        let (r, p) = (rects.expect("rects"), p.expect("p"));
        let win = ctx.screen_rect();
        let (dock, lock, cancel) = (box_of(&p, "Dock"), box_of(&p, "Lock Schema & Open"), box_of(&p, "Cancel"));
        eprintln!("{what}: reading {:?}\n  columns {:?}\n  preview {:?}\n  Dock {dock:?} Lock {lock:?} Cancel {cancel:?}", r.reading, r.columns, r.preview);
        assert!(inside(dock, win) && inside(lock, win) && inside(cancel, win), "{what}: header buttons are in the window {win:?}");
        assert!(dock.right() <= lock.left() && lock.right() <= cancel.left(), "{what}: order");
        assert!(win.right() - cancel.right() < 24.0, "{what}: right-aligned: {cancel:?} in {win:?}");
        assert!(r.reading.right() <= win.right() && r.columns.right() <= win.right() && r.preview.right() <= win.right(), "{what}: cards inside the window width");
        assert!(r.reading.bottom() <= r.columns.top() + 0.5 && r.columns.bottom() <= r.preview.top() + 0.5, "{what}: order Reading, Columns, Preview");
        if h >= 780.0 {
            // room enough: all three cards are in view, the Preview last
            assert!(r.preview.bottom() <= win.bottom() + 0.5, "{what}: Preview {:?} in {win:?}", r.preview);
            assert!(r.columns.height() >= theme::SCHEMA_COLUMNS_MIN_H, "{what}");
        }
    }
}

// -------------------------------------------------------------------- part 4, item 2: steps panel

use crate::ui::steps::{step_row_head, steps_report, StepRowSpec};

fn right_panel_rect(ctx: &egui::Context) -> egui::Rect {
    let area = theme::dialog_area(ctx);
    egui::Rect::from_min_max(egui::pos2(theme::central_area(ctx).right(), area.top()), area.right_bottom())
}

#[test]
fn nothing_in_the_steps_panel_passes_its_inner_width_at_both_scales() {
    use crate::ui::layout_tests::collect_painted;
    let long = "A_Very_Long_Column_Name_That_Keeps_Going_And_Going_Past_The_Panel";
    for (ppp, w, h) in [(1.05, 1219.05, 685.71), (0.8, 1600.0, 900.0)] {
        let mut app = test_app("p4steps");
        let src = parquet_source(&app.paths.root_dir.clone(), "loans", loans_frame());
        let steps = vec![
            TransformStep::RenameColumn { old_name: "Branch".into(), new_name: long.into() },
            TransformStep::FilterRows { column: "Branch".into(), operator: FilterOp::Eq, value: "DHK".into(), keep_matching: true },
            TransformStep::Skipped { step: Box::new(TransformStep::RenameColumn { old_name: "Balance".into(), new_name: long.into() }) },
            TransformStep::RenameColumn { old_name: "Amount".into(), new_name: "Amt".into() },
        ];
        let n = steps.len();
        open_tab(&mut app, src, steps);
        let (ctx, input) = screen(ppp, w, h);
        for sel in [None, Some(0usize), Some(2), Some(3)] {
            for confirm in [false, true] {
                let idx = app.active_tab;
                app.tabs[idx].step_counts = vec![Some(1_234_567); n + 1];
                app.selected_step = sel;
                app.confirm_delete_step = if confirm { sel } else { None };
                let mut p = None;
                for _ in 0..3 {
                    let out = ctx.run(input.clone(), |c| app.frame_ui(c));
                    p = Some(collect_painted(&out.shapes));
                }
                let what = format!("ppp {ppp} {w}x{h}, selected {sel:?}, confirm {confirm}");
                let rep = steps_report(&ctx).unwrap_or_else(|| panic!("{what}: no steps report"));
                assert_eq!(rep.rows.len(), n, "{what}");
                let panel = right_panel_rect(&ctx);
                if ppp > 1.0 && sel.is_none() && !confirm {
                    eprintln!("{what}: panel {panel:?} inner {:?}", rep.inner);
                    for (i, r) in rep.rows.iter().enumerate() {
                        eprintln!("  step {i}: card {:?} title {:?} tag {:?} (on title {}) count {:?} detail {:?} cut {}/{}", rep.cards[i], r.title, r.tag, r.tag_on_title, r.count, r.detail, r.title_cut, r.detail_cut);
                    }
                }
                assert!(rep.inner.right() <= panel.right() + 0.5 && rep.inner.left() >= panel.left(), "{what}: inner {:?} in panel {panel:?}", rep.inner);
                for (i, r) in rep.rows.iter().enumerate() {
                    for (name, b) in [("title", Some(r.title)), ("count", Some(r.count)), ("detail", Some(r.detail)), ("tag", r.tag)].into_iter().filter_map(|(n, b)| b.map(|b| (n, b))) {
                        if b.is_positive() {
                            assert!(b.right() <= rep.inner.right() + 0.5 && b.left() >= rep.inner.left() - 0.5, "{what}: step {i} {name} {b:?} passes the panel's inner width {:?}", rep.inner);
                        }
                    }
                    // the title and the count never overlap
                    assert!(r.title.right() <= r.count.left() + 0.5, "{what}: step {i} title {:?} overlaps the count {:?}", r.title, r.count);
                    assert!(r.count.right() <= rep.inner.right() + 0.5);
                }
                for c in rep.cards.iter().chain(rep.buttons.iter()) {
                    assert!(c.right() <= rep.inner.right() + 0.5, "{what}: {c:?} passes {:?}", rep.inner);
                }
                // every text painted in the panel is inside the inner width
                let p = p.expect("frames");
                for (t, b) in &p.boxes {
                    if b.left() >= panel.left() && b.top() >= panel.top() && b.left() < panel.right() {
                        assert!(b.right() <= panel.right() - 4.0 + 0.5, "{what}: text '{t}' {b:?} reaches the panel's edge {panel:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn a_60_character_title_and_a_7_digit_count_keep_the_tag_off_the_title_line_and_end_in_an_ellipsis() {
    use crate::ui::layout_tests::collect_painted;
    let title60 = "T".repeat(60);
    for (ppp, w, h) in [(1.05, 1219.05, 685.71), (0.8, 1600.0, 900.0)] {
        let (ctx, input) = screen(ppp, w, h);
        let tk = theme::LIGHT;
        let mut app = test_app("p4row");
        let _ = &mut app;
        // the width a panel gives a step row (316 less the margins, the rail and the card padding)
        let row_w = theme::RIGHT_PANEL_W - 2.0 * 16.0 - theme::STEP_LIST_INDENT - 16.0;
        for (title, tag, count, detail) in [
            (title60.as_str(), Some(("Off", tk.muted)), "1,234,567", "Rename A_long_old_name_here -> and_an_even_longer_new_name_for_it_still"),
            ("Filter", Some(("Not run", tk.muted)), "1,234,567", "Branch = DHK"),
            ("Conditional column", Some(("Error", tk.danger)), "1,234,567", "x"),
            ("Filter", None, "…", ""),
        ] {
            let mut rects = None;
            let mut p = None;
            for _ in 0..3 {
                let out = ctx.run(input.clone(), |c| {
                    egui::CentralPanel::default().show(c, |ui| {
                        ui.allocate_ui(egui::vec2(row_w, 200.0), |ui| {
                            let spec = StepRowSpec { title, struck: false, title_color: tk.ink, tag, count, detail, detail_color: tk.muted };
                            ui.set_width(row_w);
                            rects = Some((ui.min_rect(), step_row_head(ui, &spec)));
                        });
                    });
                });
                p = Some(collect_painted(&out.shapes));
            }
            let ((area, r), p) = (rects.expect("rects"), p.expect("p"));
            let what = format!("ppp {ppp}, title '{}' count {count}", &title[..title.len().min(12)]);
            let right = area.left() + row_w;
            assert!(r.title.right() <= r.count.left() + 0.5, "{what}: {:?} {:?}", r.title, r.count);
            assert!(r.count.right() <= right + 0.5, "{what}: count {:?} passes {right}", r.count);
            assert!(r.detail.right() <= right + 0.5 || !r.detail.is_positive(), "{what}: detail {:?} passes {right}", r.detail);
            assert!(r.title.right() <= right + 0.5);
            if let Some(t) = r.tag {
                assert!(t.right() <= right + 0.5, "{what}: tag {t:?}");
                if r.tag_on_title {
                    assert!(t.right() <= r.count.left() + 0.5 && t.left() >= r.title.right() - 0.5, "{what}: tag between title and count");
                } else {
                    assert!(t.top() >= r.title.bottom() - 0.5, "{what}: the tag moved to the detail line {t:?} under the title {:?}", r.title);
                }
            }
            match title.len() {
                60 => {
                    assert!(r.title_cut && !r.tag_on_title, "{what}: the long title is cut and gives its tag to the detail line");
                    assert!(r.detail_cut, "{what}: the long detail is cut as well");
                    // what is painted ends in an ellipsis
                    let shown = p.shown.iter().find(|(t, _)| t.starts_with("TTT")).map(|(t, _)| t.clone()).expect("title painted");
                    assert!(shown.ends_with('\u{2026}') && shown.len() < 60, "{what}: shown '{shown}'");
                }
                6 if count != "\u{2026}" => assert!(!r.title_cut && r.tag_on_title, "{what}: 'Filter' + 'Not run' + 1,234,567 fit in {row_w} on one line: {r:?}"),
                _ => {}
            }
            eprintln!("{what}: row {row_w} wide: title {:?} tag {:?} on title {} count {:?} detail {:?} cut {}/{}", r.title, r.tag, r.tag_on_title, r.count, r.detail, r.title_cut, r.detail_cut);
        }
    }
}

// ------------------------------------------------------ part 4, item 3: conditional column dialog

#[test]
fn the_conditional_column_dialog_offers_a_column_on_the_right_and_a_blank_result_and_fits_both_scales() {
    use crate::ui::layout_tests::collect_painted;
    for (ppp, w, h) in [(1.05, 1219.05, 685.71), (0.8, 1600.0, 900.0)] {
        let what = format!("ppp {ppp} {w}x{h}");
        let mut app = test_app("p4cond");
        let src = parquet_source(&app.paths.root_dir.clone(), "loans", loans_frame());
        open_tab(&mut app, src, vec![]);
        app.cond.rules[0].col_name = "Balance".into();
        app.cond.rules[0].op = FilterOp::Lte;
        app.cond.rules[0].compare_is_col = true;
        app.cond.rules[0].compare_val = "Amount".into();
        app.active_tool = ActiveToolModal::ConditionalCol;
        let (ctx, input) = screen(ppp, w, h);
        let mut p = None;
        for _ in 0..8 {
            let out = ctx.run(input.clone(), |c| app.frame_ui(c));
            p = Some(collect_painted(&out.shapes));
        }
        let p = p.expect("frames");
        let r = assert_dialog_fits(&ctx, "\u{E3D4} Add Conditional Column (IF / ELSE IF / ELSE)", &what);
        let has = |t: &str| p.boxes.iter().any(|(x, b)| x.contains(t) && inside(*b, r.window));
        assert!(has("column"), "{what}: the right-hand side says it is a column");
        assert!(has("Amount"), "{what}: and shows the chosen column");
        assert!(has("If either side of a rule is blank"), "{what}: the blank result is offered");
        assert!(has("blank") && has("this value") && has("no match"), "{what}: its three choices in {:?}: {:?}", r.window, p.boxes.iter().filter(|(_, b)| b.intersects(r.window)).collect::<Vec<_>>());
        // the step the dialog makes carries both
        match app.tool_step(&ActiveToolModal::ConditionalCol) {
            Some(TransformStep::ConditionalColumn { rules, blank_result, .. }) => {
                assert!(rules[0].compare_is_col && rules[0].compare_val == "Amount", "{what}");
                assert_eq!(blank_result, BlankRule::Result(String::new()), "{what}: the default is a blank cell");
            }
            other => panic!("{what}: {other:?}"),
        }
        eprintln!("{what}: dialog {:?} body {:?}", r.window, r.body);
    }
}

// ------------------------------------------------------- part 4, item 4: search overlay closes

#[test]
fn the_search_overlay_closes_on_a_click_outside_on_esc_and_when_the_window_loses_focus() {
    for (ppp, w, h) in [(1.05, 1219.05, 685.71), (0.8, 1600.0, 900.0)] {
        let what = format!("ppp {ppp} {w}x{h}");
        let mut app = test_app("p4palette");
        let src = parquet_source(&app.paths.root_dir.clone(), "loans", loans_frame());
        open_tab(&mut app, src, vec![]);
        let (ctx, input) = screen(ppp, w, h);
        let id = egui::Id::new("command_palette");
        let area = |ctx: &egui::Context| ctx.memory(|m| m.area_rect(id));
        let step = |app: &mut MicroParquetApp, events: Vec<egui::Event>, focused: bool| {
            let mut i = input.clone();
            i.events = events;
            i.focused = focused;
            let _ = ctx.run(i, |c| app.frame_ui(c));
        };
        let press = |pos: egui::Pos2| vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }];
        let release = |pos: egui::Pos2| vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }];
        let open = |app: &mut MicroParquetApp| {
            app.open_command_palette();
            for _ in 0..3 {
                step(app, vec![], true);
            }
        };

        // 1. a click on the table, outside the overlay
        open(&mut app);
        assert!(app.command_open, "{what}");
        let rect = area(&ctx).expect("the overlay was drawn");
        eprintln!("{what}: overlay {rect:?}");
        let outside = egui::pos2(rect.left() + 20.0, rect.bottom() + 150.0);
        assert!(!rect.contains(outside), "{what}: {outside:?} is outside {rect:?}");
        step(&mut app, press(outside), true);
        assert!(!app.command_open, "{what}: a click outside closes the search");
        step(&mut app, release(outside), true);

        // 2. a click inside it (on its text field) does not
        open(&mut app);
        let rect = area(&ctx).expect("drawn");
        let inside = rect.center_top() + egui::vec2(0.0, 20.0);
        step(&mut app, press(inside), true);
        step(&mut app, release(inside), true);
        assert!(app.command_open, "{what}: a click inside keeps it open");

        // 3. Esc
        step(&mut app, vec![egui::Event::Key { key: egui::Key::Escape, physical_key: Some(egui::Key::Escape), pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }], true);
        assert!(!app.command_open, "{what}: Esc closes it");

        // 4. the window loses focus
        open(&mut app);
        step(&mut app, vec![], true);
        assert!(app.command_open);
        step(&mut app, vec![egui::Event::WindowFocused(false)], false);
        assert!(!app.command_open, "{what}: it closes when the window loses focus");
        step(&mut app, vec![egui::Event::WindowFocused(true)], true);

        // 5. a click on the search box in the bar opens it, and the next click on it closes it
        let b = app.command_box_rect;
        assert!(b.is_positive(), "{what}: the search box was drawn");
        step(&mut app, press(b.center()), true);
        step(&mut app, release(b.center()), true);
        assert!(app.command_open, "{what}: the box opens the search");
        for _ in 0..2 {
            step(&mut app, vec![], true);
        }
        step(&mut app, press(b.center()), true);
        step(&mut app, release(b.center()), true);
        assert!(!app.command_open, "{what}: the box closes it again (it does not close and reopen)");
    }
}

// ------------------------------------------------------------ part 4, item 6: Rate profile dialog

#[test]
fn the_rate_profile_dialog_shows_its_settings_and_the_labels_and_makes_the_step() {
    use crate::ui::layout_tests::collect_painted;
    let en = "\u{2013}";
    for (ppp, w, h) in [(1.05, 1219.05, 685.71), (0.8, 1600.0, 900.0)] {
        let what = format!("ppp {ppp} {w}x{h}");
        let mut app = test_app("p4rate");
        let src = parquet_source(&app.paths.root_dir.clone(), "loans", loans_frame());
        open_tab(&mut app, src, vec![]);
        let (ctx, input) = screen(ppp, w, h);
        app.run_command(&ctx, micro_parquet::commands::CommandId::RateProfile);
        assert_eq!(app.active_tool, ActiveToolModal::RateBand, "{what}: Ctrl+K / the Banking menu opens it");
        let mut p = None;
        for _ in 0..8 {
            let out = ctx.run(input.clone(), |c| app.frame_ui(c));
            p = Some(collect_painted(&out.shapes));
        }
        let p = p.expect("frames");
        let r = assert_dialog_fits(&ctx, "\u{E0B4} Rate profile: put a rate column into bands", &what);
        let has = |t: &str| p.boxes.iter().any(|(x, b)| x.contains(t) && inside(*b, r.window));
        for t in ["Rate column", "Output column", "Preset:", "Fixed width", "Custom edges", "Start %", "End %", "Step %", "the lower band", "the upper band", "Add Rate Profile"] {
            assert!(has(t), "{what}: '{t}' is in the dialog {:?}", r.window);
        }
        // a new step's labels have no serial number (Phase 8 W3; the tick box adds it)
        assert!(has(&format!("0%{en}1%")) && !has(&format!("01. 0%{en}1%")), "{what}: the preview lists the labels");
        assert!(has("15 bands"), "{what}: and counts them");
        // the dialog's step: default (a, b], 0 to 15 by 1, output Rate_Band
        match app.tool_step(&ActiveToolModal::RateBand) {
            Some(TransformStep::RateBand { band_def, new_col, .. }) => {
                assert_eq!(new_col, "Rate_Band");
                assert!(!band_def.lower_inclusive);
                assert_eq!(band_def.label_for(Some(9.0)).unwrap(), format!("8%{en}9%"), "{what}");
            }
            other => panic!("{what}: {other:?}"),
        }
        // a bad setting: the step cannot be made and the dialog says why
        app.rate_band.step = 4.0;
        assert!(app.tool_step(&ActiveToolModal::RateBand).is_none());
        let out = ctx.run(input.clone(), |c| app.frame_ui(c));
        let p = collect_painted(&out.shapes);
        assert!(p.boxes.iter().any(|(t, _)| t.contains("whole number of steps")), "{what}: the error is shown");
        eprintln!("{what}: dialog {:?} body {:?}", r.window, r.body);
    }
}

#[test]
fn the_en_dash_and_the_at_least_sign_of_the_rate_labels_exist_in_every_font_the_table_uses() {
    let ctx = egui::Context::default();
    theme::load_fonts(&ctx);
    let _ = ctx.run(Default::default(), |_| {});
    let missing: Vec<String> = ctx.fonts(|f| {
        let mut m = Vec::new();
        for fam in [egui::FontFamily::Proportional, theme::family_semibold(), theme::family_bold(), egui::FontFamily::Monospace] {
            let id = egui::FontId::new(14.0, fam.clone());
            for c in ['\u{2013}', '\u{2265}', '%', '>'] {
                if !f.has_glyph(&id, c) {
                    m.push(format!("{c:?} in {fam:?}"));
                }
            }
        }
        m
    });
    assert!(missing.is_empty(), "{missing:?}");
}
