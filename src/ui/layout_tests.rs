//! Layout tests (Phase 3 fixes, part 2): the screens are drawn in a headless egui frame at a
//! given window size and scale, and the rectangles of what was drawn are checked.

use crate::ui::prelude::*;
use crate::ui::tests::test_app;
use crate::ui::top_bar::{plan_bar, BarKey, BAR_ITEMS, FOLD_ORDER, LADDER};
use micro_parquet::commands::CommandId;

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

/// Every text the bar paints at this window width.
fn bar_texts_at(px_w: f32) -> Vec<String> {
    let mut app = test_app("bartexts");
    let (ctx, input) = headless(px_w, WIN_H_PX, UI_SCALE);
    let mut texts = Vec::new();
    for _ in 0..3 {
        let out = ctx.run(input.clone(), |ctx| {
            app.render_top_bar(ctx);
        });
        texts = collect_painted(&out.shapes).texts.into_iter().map(|(t, _, _)| t).collect();
    }
    texts
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

/// The toolbar buttons drawn in a bar: the names of `bar_rects_at`'s rectangles that are items.
const ITEM_NAMES: [&str; 10] = ["OpenFile", "Stack", "Batch", "Filter", "Sort", "Columns", "CalcColumn", "Pivot", "Banking", "Export"];
/// `FOLD_ORDER` by those names.
const FOLD_NAMES: [&str; 7] = ["CalcColumn", "Columns", "Batch", "Stack", "Sort", "Filter", "Pivot"];

#[test]
fn nothing_overlaps_at_any_window_width_and_open_banking_export_search_never_go() {
    // from the smallest window the app allows up to a wide monitor, in 20 px steps
    let mut w = 800.0;
    while w <= 2560.0 {
        let (screen, rects) = bar_rects_at(w);
        assert_no_overlap(&rects, egui::Rect::from_min_size(screen.min, egui::vec2(screen.width(), theme::TOP_BAR_H)), &format!("{w} px"));
        let has = |n: &str| rects.iter().any(|(r, _)| r == n);
        for never in ["OpenFile", "Banking", "Export", "Search", "More"] {
            assert!(has(never), "{w} px: '{never}' is always in the bar");
        }
        // what is missing is a first part of the fold order, nothing else
        let missing: Vec<&str> = ITEM_NAMES.iter().copied().filter(|n| !has(n)).collect();
        assert!(missing.len() <= FOLD_NAMES.len() && missing.iter().all(|m| FOLD_NAMES[..missing.len()].contains(m)), "{w} px: folded {missing:?} is not a start of {FOLD_NAMES:?}");
        w += 20.0;
    }
}

#[test]
fn items_fold_in_the_set_order_and_icons_come_before_the_second_fold() {
    assert_eq!(FOLD_ORDER.len(), 7);
    let names: Vec<String> = FOLD_ORDER.iter().map(|k| match k { BarKey::Cmd(id) => format!("{id:?}"), other => format!("{other:?}") }).collect();
    assert_eq!(names, FOLD_NAMES, "Calc column, Columns, Batch, Stack, Sort, Filter, Pivot");
    for never in [BarKey::Cmd(CommandId::OpenFile), BarKey::Banking, BarKey::Export] {
        assert!(!FOLD_ORDER.contains(&never) && BAR_ITEMS.iter().any(|(k, _)| *k == never), "{never:?} never folds");
    }
    // every other item of the bar folds
    assert_eq!(BAR_ITEMS.len(), FOLD_ORDER.len() + 3);
    // the ladder: one fold with labels, then icon-only, then more folds (icon-only)
    assert_eq!(LADDER[0], (0, false));
    assert_eq!(LADDER[1], (1, false));
    assert_eq!(LADDER[2], (1, true), "icon-only comes before the second fold");
    assert!(LADDER[2..].windows(2).all(|w| w[1].0 == w[0].0 + 1 && w[1].1));

    // walk the width down: the state only moves along the ladder, never back
    let label = [80.0; 10];
    let icon = [40.0; 10];
    let mut seen: Vec<(usize, bool)> = Vec::new();
    let mut avail = 3000.0;
    while avail > 100.0 {
        let p = plan_bar(avail, 100.0, &label, &icon, 90.0, 20.0, 2.0);
        if seen.last() != Some(&(p.folded, p.icon_only)) {
            seen.push((p.folded, p.icon_only));
        }
        assert!(p.search_w >= theme::SEARCH_ICON_W && p.search_w <= theme::SEARCH_MAX_W);
        avail -= 1.0;
    }
    assert_eq!(seen, LADDER.to_vec(), "every state is reached, in order");
    // exact widths: labels for 8 items need 100 + groups(4*22) + 8*80 + 2 gaps within groups.. + More; check the edge both ways
    let fits = |avail: f32| plan_bar(avail, 100.0, &label, &icon, 90.0, 20.0, 2.0);
    assert_eq!(fits(5000.0).folded, 0);
    let edge = (1000..4000).map(|a| a as f32).find(|a| fits(*a).folded == 0).expect("an edge");
    assert!(fits(edge - 1.0).folded >= 1, "one pixel narrower folds Calc column");
    assert!(!fits(edge - 1.0).icon_only, "a single fold keeps the labels");
}

#[test]
fn the_bar_items_are_the_toolbar_commands() {
    let mut cmds: Vec<CommandId> = micro_parquet::commands::TOOLBAR_BUTTONS_FILE.to_vec();
    cmds.extend_from_slice(micro_parquet::commands::TOOLBAR_BUTTONS_TRANSFORM);
    let in_bar: Vec<CommandId> = BAR_ITEMS.iter().filter_map(|(k, _)| if let BarKey::Cmd(id) = k { Some(*id) } else { None }).collect();
    assert_eq!(in_bar, cmds, "same buttons, same order");
}

#[test]
fn at_1280_by_720_and_105_percent_open_banking_export_and_search_are_all_visible() {
    let (screen, rects) = bar_rects_at(WIN_W_PX);
    let rect = |n: &str| rects.iter().find(|(r, _)| r == n).map(|(_, r)| *r).unwrap_or_else(|| panic!("'{n}' is not in the bar: {rects:?}"));
    for n in ["OpenFile", "Banking", "Export", "Search"] {
        let r = rect(n);
        assert!(r.width() > 20.0 && r.right() <= screen.right() && r.left() >= 0.0, "{n}: {r:?}");
    }
    assert!(rect("Search").width() >= theme::SEARCH_MIN_W - 1.0, "{:?}", rect("Search"));
    // the narrow box keeps its icon and "Search…" (the Ctrl K badge needs more room)
    let texts = bar_texts_at(WIN_W_PX);
    assert!(texts.iter().any(|t| t == "Search\u{2026}"), "{texts:?}");
    // the buttons keep their labels (only Calc column is in More), not icons only
    assert!(rect("Filter").width() > 50.0 && rect("OpenFile").width() > 50.0, "labelled buttons: Filter {:?}", rect("Filter"));
    assert!(!rects.iter().any(|(n, _)| n == "CalcColumn") && rects.iter().any(|(n, _)| n == "Columns" || n == "Pivot"), "only Calc column is folded: {rects:?}");
    assert_no_overlap(&rects, egui::Rect::from_min_size(screen.min, egui::vec2(screen.width(), theme::TOP_BAR_H)), "1280");
}

#[test]
fn narrow_bars_show_icon_only_buttons_that_name_themselves_in_a_tooltip() {
    // find a window narrow enough for icon-only buttons
    let mut icon_w = None;
    for w in (800..=1280).step_by(20) {
        let (_, rects) = bar_rects_at(w as f32);
        let has = |n: &str| rects.iter().any(|(r, _)| r == n);
        if has("Filter") {
            let f = rects.iter().find(|(r, _)| r == "Filter").map(|(_, r)| *r).unwrap();
            let (_, wide) = bar_rects_at(2000.0);
            let wide_f = wide.iter().find(|(r, _)| r == "Filter").map(|(_, r)| *r).unwrap();
            if f.width() < wide_f.width() - 10.0 {
                icon_w = Some((w as f32, f, rects));
                break;
            }
        }
    }
    let (win, filter, rects) = icon_w.expect("some width in 800..1280 shows icon-only buttons");
    // no label text is drawn; hovering a button shows its name (a disabled one, why it is off)
    let hover_texts = |target: egui::Pos2| -> Vec<String> {
        let mut app = test_app("bartip");
        let (ctx, input) = headless(win, WIN_H_PX, UI_SCALE);
        let mut texts = Vec::new();
        for i in 0..40 {
            let mut inp = input.clone();
            inp.time = Some(i as f64 * 0.05);
            inp.predicted_dt = 0.05;
            inp.events = vec![egui::Event::PointerMoved(target)];
            let out = ctx.run(inp, |ctx| {
                app.render_top_bar(ctx);
            });
            texts = collect_painted(&out.shapes).texts.into_iter().map(|(t, _, _)| t).collect();
        }
        texts
    };
    let rect_of = |n: &str| rects.iter().find(|(r, _)| r == n).map(|(_, r)| *r).unwrap_or_else(|| panic!("{n}"));
    let none = hover_texts(egui::pos2(win / UI_SCALE - 5.0, 300.0));
    assert!(!none.iter().any(|t| t == "Open" || t.ends_with("Filter") || t.ends_with("Sort")), "no labels in icon-only mode: {none:?}");
    let open = hover_texts(rect_of("OpenFile").center());
    assert!(open.iter().any(|t| t == "Open"), "hovering the Open icon names it: {open:?}");
    let filter_tip = hover_texts(filter.center());
    assert!(filter_tip.iter().any(|t| t.starts_with("Filter") && t.contains("Open a file first")), "a disabled icon says what it is and why it is off: {filter_tip:?}");
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
    let (ctx, input) = headless(1600.0, WIN_H_PX, UI_SCALE); // wide enough for the Ctrl K badge
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

/// What a dialog's frame and title bar add to its content in these tests.
const OVERHEAD: f32 = 70.0;

#[test]
fn dialog_layout_adds_up_inside_the_bounds() {
    let bounds_h = 600.0;
    for footer in [theme::DIALOG_FOOTER_H, 90.0] {
        let l = theme::dialog_layout(bounds_h, OVERHEAD, 6.0, footer);
        assert!((l.max_total_h - bounds_h).abs() < 0.01, "{l:?}");
        // title bar + frame + middle + spacing + footer add up to the maximum, never more
        let overhead = OVERHEAD;
        assert!(overhead + l.body_max_h + 6.0 + footer <= l.max_total_h + 0.01, "{l:?}");
    }
    // a taller footer (wrapped buttons) leaves less for the middle
    assert!(theme::dialog_layout(bounds_h, OVERHEAD, 6.0, 90.0).body_max_h < theme::dialog_layout(bounds_h, OVERHEAD, 6.0, 56.0).body_max_h);
    assert_eq!(theme::dialog_layout(100.0, OVERHEAD, 6.0, 56.0).body_max_h, theme::DIALOG_BODY_MIN_H);
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
                        ui.horizontal_wrapped(|ui| {
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
        assert!(win.height() <= screen.height() - 2.0 * theme::DIALOG_EDGE + 0.5, "{rows} rows: window {win:?} in screen {screen:?}");
        assert!(win.top() >= screen.top() - 0.5 && win.bottom() <= screen.bottom() + 0.5, "{rows} rows: {win:?}");
        assert_eq!(buttons.len(), 2);
        for b in &buttons {
            assert!(b.bottom() <= win.bottom() && b.top() >= win.top(), "{rows} rows: button {b:?} outside {win:?}");
            assert!(b.bottom() <= screen.bottom() - theme::DIALOG_EDGE + 0.5, "{rows} rows: button {b:?} too low for {screen:?}");
        }
        if rows == 4000 {
            // the middle scrolls: the window is the full height and the footer is at its bottom
            assert!(win.height() >= screen.height() - 2.0 * theme::DIALOG_EDGE - 12.0, "{win:?}");
            assert!(buttons[0].bottom() >= win.bottom() - 2.0 * theme::SPACE_M - 1.0, "footer sits at the bottom: {win:?} {:?}", buttons[0]);
        }
        // a short dialog is not stretched and opens horizontally centred, 24 px below the top
        if rows == 3 {
            assert!(win.height() < 220.0, "{win:?}");
        }
        assert!((win.top() - theme::DIALOG_EDGE).abs() < 1.0, "{rows} rows: opens 16 px below the top: {win:?}");
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
    assert!(win.height() <= screen.height() - 2.0 * theme::DIALOG_EDGE + 0.5, "{win:?} in {screen:?}");
    assert!(win.bottom() <= screen.bottom() - theme::DIALOG_EDGE + 0.5 && win.top() >= theme::DIALOG_EDGE - 0.5, "{win:?} in {screen:?}");
}

// ------------------------------------------------------------------------ schema dialog

use crate::ui::schema::{PendingCsvModal, SchemaCardRects};
use micro_parquet::models::ColumnSchemaConfig;
use polars::prelude::NamedFrom;

pub(crate) fn schema_modal(rows: usize, columns: usize, parquet: bool) -> PendingCsvModal {
    // the preview shows as many columns as the file has (up to 60)
    let names: Vec<String> = (0..columns.clamp(4, 60)).map(|i| format!("Col{i}")).collect();
    let df = DataFrame::new(
        names
            .iter()
            .map(|n| polars::prelude::Series::new(n.as_str().into(), (0..rows).map(|r| format!("v{r}")).collect::<Vec<_>>()).into())
            .collect(),
    )
    .unwrap_or_default();
    let mut config = IngestionConfig::default();
    config.columns = (0..columns)
        .map(|i| ColumnSchemaConfig {
            name: format!("Column {i}"),
            sample_values: vec![format!("{i}"), "20400-01".into(), "FIXED TERM DEPOSIT".into()],
            ..Default::default()
        })
        .collect();
    PendingCsvModal {
        csv_path: PathBuf::from("/data/loans_2026-11-16.csv"),
        raw_preview_df: df,
        config,
        direct_convert_only: false,
        target_tab: None,
        parse_failures: Vec::new(),
        is_parquet: parquet,
    }
}

/// Draws the schema cards in a middle of the height a dialog leaves at 1280×720 / 105%.
/// Returns the card rectangles, the painted texts with their positions and the middle's rect.
fn drawn_schema(rows: usize, columns: usize, parquet: bool) -> (SchemaCardRects, Vec<(String, egui::Pos2)>, egui::Rect) {
    let lay = theme::dialog_layout(WIN_H_PX / UI_SCALE - 2.0 * theme::DIALOG_EDGE, OVERHEAD, 6.0, theme::DIALOG_FOOTER_H);
    drawn_schema_in(lay.body_max_h, rows, columns, parquet)
}

/// The same in a middle whose visible height is `view_h`.
fn drawn_schema_in(view_h: f32, rows: usize, columns: usize, parquet: bool) -> (SchemaCardRects, Vec<(String, egui::Pos2)>, egui::Rect) {
    fn collect(shape: &egui::Shape, out: &mut Vec<(String, egui::Pos2)>) {
        match shape {
            egui::Shape::Text(t) => out.push((t.galley.text().to_string(), t.pos)),
            egui::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
            _ => {}
        }
    }
    let mut app = test_app("schema");
    let mut modal = schema_modal(rows, columns, parquet);
    let (ctx, input) = headless(WIN_W_PX, WIN_H_PX, UI_SCALE);
    let mut rects = None;
    let mut middle = egui::Rect::NOTHING;
    let mut texts = Vec::new();
    for _ in 0..3 {
        let out = ctx.run(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let r = ui
                    .allocate_ui(egui::vec2(theme::SCHEMA_DIALOG_W - 2.0 * theme::SPACE_M, view_h), |ui| {
                        let mut reread = false;
                        app.schema_cards(ui, &mut modal, &mut reread, view_h)
                    });
                middle = r.response.rect;
                rects = Some(r.inner);
            });
        });
        texts.clear();
        for c in &out.shapes {
            collect(&c.shape, &mut texts);
        }
    }
    (rects.expect("drawn"), texts, middle)
}

#[test]
fn schema_dialog_has_three_cards_with_these_headings() {
    let (r, texts, _) = drawn_schema(5, 30, false);
    let all: Vec<&str> = texts.iter().map(|(t, _)| t.as_str()).collect();
    for h in ["Reading options", "Preview", "Columns"] {
        assert!(all.contains(&h), "heading {h}: {all:?}");
    }
    assert!(r.reading.bottom() <= r.preview.top() + 0.5, "{r:?}");
    assert!(r.preview.bottom() <= r.columns.top() + 0.5, "{r:?}");
    // the old numbered headings are gone
    assert!(!all.iter().any(|t| t.contains("Raw 5-Row") || t.contains("2. Set Specific")), "{all:?}");
}

#[test]
fn reading_options_are_two_compact_rows_and_the_key_warning_is_a_tooltip() {
    let (r, texts, _) = drawn_schema(5, 30, false);
    let y_of = |label: &str| texts.iter().find(|(t, _)| t == label).map(|(_, p)| p.y).unwrap_or_else(|| panic!("'{label}' not painted: {texts:?}"));
    // row 1: the three options, separator, title lines, encoding and Re-read are on one line
    let row1 = y_of("Clean line breaks");
    for l in ["Accounting negatives", "Heal broken lines", "Separator", "Title lines", "Encoding"] {
        assert!((y_of(l) - row1).abs() < 4.0, "'{l}' is on row 1 ({} vs {row1})", y_of(l));
    }
    let reread_y = texts.iter().find(|(t, _)| t.contains("Re-read")).map(|(_, p)| p.y).expect("Re-read");
    // the rows fit the card: it is not wider than the dialog's middle
    let dialog_inner_w = theme::SCHEMA_DIALOG_W - 2.0 * theme::SPACE_M;
    assert!(r.reading.width() <= dialog_inner_w + 0.5, "row 1 is wider than the dialog: card {:?}", r.reading);
    assert!((reread_y - row1).abs() < 4.0, "Re-read on row 1");
    // row 2: template and masking key
    let row2 = y_of("Template:");
    assert!(row2 > row1 + 10.0, "row 2 is below row 1");
    for l in ["Masking key:", "not created"] {
        if let Some((_, p)) = texts.iter().find(|(t, _)| t == l) {
            assert!((p.y - row2).abs() < 4.0, "'{l}' is on row 2");
        }
    }
    assert!(texts.iter().any(|(t, p)| t.contains("Save") && (p.y - row2).abs() < 4.0));
    assert!(texts.iter().any(|(t, p)| t.contains("Import") && (p.y - row2).abs() < 4.0));
    // the card holds the heading and two rows: compact
    assert!(r.reading.height() < 4.0 * theme::DATA_ROW_H + 2.0 * theme::SPACE_M, "{:?}", r.reading);
    // the long warning is not drawn in the dialog
    assert!(!texts.iter().any(|(t, _)| t.contains("Masks made with different keys")), "warning belongs in a tooltip");
}

#[test]
fn preview_is_five_rows_whatever_the_file_has() {
    let heights: Vec<f32> = [0usize, 1, 5].iter().map(|n| drawn_schema(*n, 30, false).0.preview.height()).collect();
    assert!((heights[0] - heights[1]).abs() < 0.5 && (heights[1] - heights[2]).abs() < 0.5, "{heights:?}");
    // 5 rows, the header row and the card's heading and padding
    assert!(heights[2] >= theme::HEADER_ROW_H + 5.0 * theme::DATA_ROW_H, "{heights:?}");
    // all five rows are visible without scrolling
    let (_, texts, _) = drawn_schema(5, 30, false);
    for r in 0..5 {
        assert!(texts.iter().any(|(t, _)| *t == format!("v{r}")), "row {r} of the preview is drawn");
    }
}

#[test]
fn columns_card_takes_the_remaining_height_but_never_less_than_its_minimum() {
    for n in [3usize, 60, 400] {
        // room to spare: it ends where the visible middle ends
        let (r, _, middle) = drawn_schema_in(1100.0, 5, n, false);
        assert!((r.columns.bottom() - middle.bottom()).abs() < 1.5, "{n} columns: card ends at {} but the middle at {}", r.columns.bottom(), middle.bottom());
        assert!(r.columns.height() > theme::SCHEMA_COLUMNS_MIN_H, "{n} columns: {:?}", r.columns);
        // a short middle: the card keeps its minimum and the dialog scrolls
        let (r, _, _) = drawn_schema_in(300.0, 5, n, false);
        assert!((r.columns.height() - theme::SCHEMA_COLUMNS_MIN_H).abs() < 3.0, "{n} columns: {:?}", r.columns);
    }
}

#[test]
fn columns_heading_row_stays_above_the_rows() {
    let (r, texts, _) = drawn_schema(5, 400, false);
    let head_y = texts.iter().find(|(t, _)| t == "Column Name").map(|(_, p)| p.y).expect("heading row");
    let first_y = texts.iter().find(|(t, _)| t == "Column 0").map(|(_, p)| p.y).expect("first row");
    assert!(head_y < first_y, "{head_y} < {first_y}");
    // 400 rows do not make the card taller: it scrolls inside
    assert!(r.columns.height() < 700.0);
    for h in ["Target Data Type", "PII Anonymizer", "Dr/Cr Sign", "Parse Failures", "Sample Values"] {
        assert!(texts.iter().any(|(t, _)| t == h), "{h}");
    }
}

#[test]
fn parquet_files_show_the_one_option_that_applies() {
    let (_, texts, _) = drawn_schema(5, 10, true);
    let all: Vec<&str> = texts.iter().map(|(t, _)| t.as_str()).collect();
    assert!(all.contains(&"Accounting negatives"));
    assert!(!all.contains(&"Separator") && !all.contains(&"Encoding") && !all.contains(&"Heal broken lines"), "{all:?}");
}

// ------------------------------------------------------------------ table header (header_bg)

/// What one frame painted: texts with their position and font family, and filled rectangles.
#[derive(Default)]
struct Painted {
    texts: Vec<(String, egui::Pos2, egui::FontFamily)>,
    rects: Vec<(egui::Rect, egui::Color32)>,
}

fn collect_painted(shapes: &[egui::epaint::ClippedShape]) -> Painted {
    fn walk(shape: &egui::Shape, out: &mut Painted) {
        match shape {
            egui::Shape::Text(t) => {
                let fam = t.galley.job.sections.first().map(|s| s.format.font_id.family.clone()).unwrap_or(egui::FontFamily::Proportional);
                out.texts.push((t.galley.text().to_string(), t.pos, fam));
            }
            egui::Shape::Rect(r) => out.rects.push((r.rect, r.fill)),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut p = Painted::default();
    for c in shapes {
        walk(&c.shape, &mut p);
    }
    p
}

fn grid_frame(rows: usize, selected: Option<&str>, events: Vec<egui::Event>, ctx: &egui::Context, input: &egui::RawInput, df: &DataFrame) -> Painted {
    // time moves on a little each frame (egui's smooth scrolling needs it)
    // (a counter per context: tests run in parallel and time must not go backwards)
    let n = ctx.data_mut(|d| {
        let n = d.get_temp::<u64>(egui::Id::new("test_frame")).unwrap_or(0);
        d.insert_temp(egui::Id::new("test_frame"), n + 1);
        n
    });
    let mut input = input.clone();
    input.time = Some(n as f64 * 0.05);
    input.predicted_dt = 0.05;
    input.events = events;
    let out = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let _ = MicroParquetApp::render_data_grid(ui, df, &HashMap::new(), 0, NumberDisplayFormat::Raw, 2, 7, selected, &HashMap::new(), None);
        });
    });
    let _ = rows;
    collect_painted(&out.shapes)
}

fn grid_df(rows: usize) -> DataFrame {
    DataFrame::new(vec![
        polars::prelude::Series::new("Branch".into(), (0..rows).map(|r| format!("B{r}")).collect::<Vec<_>>()).into(),
        polars::prelude::Series::new("Balance".into(), (0..rows).map(|r| r as f64 * 1000.0).collect::<Vec<_>>()).into(),
    ])
    .unwrap_or_default()
}

#[test]
fn grid_heading_row_is_bold_ink_on_header_bg_and_the_selected_column_keeps_accent_soft() {
    let (ctx, input) = headless(WIN_W_PX, WIN_H_PX, UI_SCALE);
    let df = grid_df(20);
    let mut p = Painted::default();
    for _ in 0..3 {
        p = grid_frame(20, Some("Balance"), vec![], &ctx, &input, &df);
    }
    let tk = theme::LIGHT;
    let branch = p.texts.iter().find(|(t, _, _)| t == "Branch").expect("Branch heading");
    let balance = p.texts.iter().find(|(t, _, _)| t == "Balance").expect("Balance heading");
    assert_eq!(branch.2, theme::family_bold(), "heading text is bold");
    assert_eq!(balance.2, theme::family_bold());
    // a header_bg rectangle under "Branch", an accent_soft one under the selected "Balance"
    let under = |pos: egui::Pos2, color: egui::Color32| p.rects.iter().any(|(r, c)| *c == color && r.contains(pos) && r.height() >= theme::HEADER_ROW_H - 1.0);
    assert!(under(branch.1 + egui::vec2(2.0, 4.0), tk.header_bg), "Branch heading is on header_bg");
    assert!(under(balance.1 + egui::vec2(2.0, 4.0), tk.accent_soft), "the selected column's heading is on accent_soft");
    assert!(!under(branch.1 + egui::vec2(2.0, 4.0), tk.accent_soft));
}

#[test]
fn grid_heading_row_is_outside_the_scrolling_rows() {
    // The heading row is drawn above a body that scrolls on its own (egui_extras' table keeps
    // the header outside the body's scroll area). A synthetic mouse wheel cannot be delivered
    // to a scroll area in a headless frame, so this checks the structure: with 500 rows only the
    // rows in view are drawn (the body is virtual and clipped), the heading is above all of
    // them and drawn at the same place whatever the number of rows.
    let (ctx, input) = headless(WIN_W_PX, WIN_H_PX, UI_SCALE);
    let head_y = |rows: usize| {
        let df = grid_df(rows);
        let mut p = Painted::default();
        for _ in 0..3 {
            p = grid_frame(rows, None, vec![], &ctx, &input, &df);
        }
        let y = p.texts.iter().find(|(t, _, _)| t == "Branch").map(|(_, pos, _)| pos.y).expect("heading");
        let first = p.texts.iter().find(|(t, _, _)| t == "B0").map(|(_, pos, _)| pos.y).expect("first row");
        let drawn_rows = p.texts.iter().filter(|(t, _, _)| t.len() > 1 && t.starts_with('B') && t[1..].chars().all(|c| c.is_ascii_digit())).count();
        (y, first, drawn_rows)
    };
    let (y_small, first_small, n_small) = head_y(10);
    let (y_big, first_big, n_big) = head_y(500);
    assert!((y_small - y_big).abs() < 0.5 && (first_small - first_big).abs() < 0.5, "same layout for 10 and 500 rows");
    assert!(y_big < first_big, "the heading is above the rows");
    assert!(n_small == 10 && n_big < 40, "only the rows in view are drawn: {n_small}, {n_big}");
}

#[test]
fn schema_preview_and_columns_list_use_the_same_heading_style() {
    let tk = theme::LIGHT;
    fn collect_all(modal: PendingCsvModal) -> Painted {
        let mut app = test_app("hdr");
        let mut modal = modal;
        let (ctx, input) = headless(WIN_W_PX, WIN_H_PX, UI_SCALE);
        let mut p = Painted::default();
        for _ in 0..3 {
            let out = ctx.run(input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.allocate_ui(egui::vec2(1000.0, 600.0), |ui| {
                        let mut reread = false;
                        app.schema_cards(ui, &mut modal, &mut reread, 600.0);
                    });
                });
            });
            p = collect_painted(&out.shapes);
        }
        p
    }
    let p = collect_all(schema_modal(5, 10, false));
    for h in ["Col0", "Column Name", "Target Data Type", "Sample Values"] {
        let t = p.texts.iter().find(|(t, _, _)| t == h).unwrap_or_else(|| panic!("{h} not painted"));
        assert_eq!(t.2, theme::family_bold(), "{h} is bold");
        assert!(
            p.rects.iter().any(|(r, c)| *c == tk.header_bg && r.contains(t.1 + egui::vec2(2.0, 4.0))),
            "{h} is on header_bg"
        );
    }
}


// ------------------------------------------------------------------ mono by content (layout 5)

#[test]
fn mono_font_follows_the_content_not_the_name() {
    use crate::ui::table::{cell_kind, CellKind};
    use polars::prelude::NamedFrom;
    let s = |name: &str, v: &[&str]| polars::prelude::Series::new(name.into(), v);
    // "Gl Name" sounds like an ID by its name, but holds words: Source Sans 3
    assert_eq!(cell_kind(&s("Gl Name", &["FIXED TERM DEPOSIT", "SAVINGS DEPOSIT"])), CellKind::Text);
    // "Glcode" holds codes: JetBrains Mono
    assert_eq!(cell_kind(&s("Glcode", &["20400-01", "20400-02"])), CellKind::MonoText);
    // the name does not matter either way
    assert_eq!(cell_kind(&s("Customer Name", &["20400-01"])), CellKind::MonoText);
    assert_eq!(cell_kind(&s("Account No", &["ACC-001", "ACC-002"])), CellKind::Text);
    // Float, Int and Date columns are mono whatever they are called
    assert_eq!(cell_kind(&polars::prelude::Series::new("Notes".into(), &[1.5f64])), CellKind::Number);
    assert_eq!(cell_kind(&polars::prelude::Series::new("Notes".into(), &[1i64])), CellKind::Number);
    let d = polars::prelude::Series::new("Notes".into(), &[19_000i32]).cast(&polars::prelude::DataType::Date).expect("date");
    assert_eq!(cell_kind(&d), CellKind::MonoText);
}

#[test]
fn the_grid_draws_codes_in_mono_and_words_in_source_sans() {
    let df = DataFrame::new(vec![
        polars::prelude::Series::new("Gl Name".into(), &["FIXED TERM DEPOSIT", "SAVINGS DEPOSIT"]).into(),
        polars::prelude::Series::new("Glcode".into(), &["20400-01", "20400-02"]).into(),
    ])
    .unwrap_or_default();
    let (ctx, input) = headless(WIN_W_PX, WIN_H_PX, UI_SCALE);
    let mut p = Painted::default();
    for _ in 0..3 {
        p = grid_frame(2, None, vec![], &ctx, &input, &df);
    }
    let fam = |text: &str| p.texts.iter().find(|(t, _, _)| t == text).map(|(_, _, f)| f.clone()).unwrap_or_else(|| panic!("'{text}' not painted"));
    assert_eq!(fam("FIXED TERM DEPOSIT"), egui::FontFamily::Proportional, "words are Source Sans 3");
    assert_eq!(fam("20400-01"), egui::FontFamily::Monospace, "codes are JetBrains Mono");
}

// ------------------------------------------------------------------ load progress (layout 6)

/// Paints the status bar of `app` and returns every text it drew, joined.
fn status_bar_text(app: &mut MicroParquetApp) -> String {
    let (ctx, input) = headless(2400.0, 600.0, 1.0);
    let mut p = Painted::default();
    for _ in 0..3 {
        let out = ctx.run(input.clone(), |ctx| app.render_status_bar(ctx));
        p = collect_painted(&out.shapes);
    }
    p.texts.iter().map(|(t, _, _)| t.clone()).collect::<Vec<_>>().join(" | ")
}

#[test]
fn the_status_bar_shows_stages_and_elapsed_time_and_a_percentage_only_for_bytes() {
    let mut app = test_app("l6bar");
    app.jobs = JobQueue::new(|| {});
    let (up_tx, up_rx) = std::sync::mpsc::channel::<()>();
    let (step_tx, step_rx) = std::sync::mpsc::channel::<u8>();
    app.jobs.submit(JobKey::unique(JobKind::Load), "Opening loans.csv", true, move |ctx| {
        ctx.set_stages(&["Reading", "Parsing", "Checking"]);
        ctx.set_stage(1);
        let _ = up_tx.send(());
        // 1: a loop reports bytes   2: it finishes   3: leave
        while let Ok(cmd) = step_rx.recv_timeout(std::time::Duration::from_secs(20)) {
            match cmd {
                1 => ctx.set_bytes("Counting the file's lines", 1_048_576, 4_194_304),
                2 => ctx.set_bytes("Counting the file's lines", 4_194_304, 0),
                _ => break,
            }
            // give the test time to paint
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(JobOutput::Exported(micro_parquet::tab_jobs::ExportOutcome { message: String::new(), path_used: None, written: None }))
    });
    up_rx.recv_timeout(std::time::Duration::from_secs(10)).expect("the job runs");

    // stages only: every stage is named, with the time; no percentage anywhere
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let t = status_bar_text(&mut app);
    for lab in ["Reading 1/3", "Parsing 2/3", "Checking 3/3"] {
        assert!(t.contains(lab), "{lab} in: {t}");
    }
    assert!(t.contains("1 s") || t.contains("2 s"), "the elapsed time is shown: {t}");
    assert!(!t.contains('%'), "no percentage without bytes: {t}");

    // a loop that reads bytes itself: a real percentage (1 MB of 4 MB = 25%), with what it does
    let _ = step_tx.send(1);
    std::thread::sleep(std::time::Duration::from_millis(100));
    let t = status_bar_text(&mut app);
    assert!(t.contains("25%"), "25% of the bytes read: {t}");
    assert!(t.contains("Counting the file's lines"), "{t}");
    assert!(t.contains("Parsing 2/3"), "the stages stay: {t}");

    // the loop ends: the percentage goes with it
    let _ = step_tx.send(2);
    std::thread::sleep(std::time::Duration::from_millis(100));
    let t = status_bar_text(&mut app);
    assert!(!t.contains('%'), "no percentage after the loop: {t}");
    let _ = step_tx.send(3);
}

#[test]
fn bytes_are_written_in_the_unit_a_banker_reads() {
    use crate::ui::status::format_bytes;
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(1536), "1.5 KB");
    assert_eq!(format_bytes(4_194_304), "4.0 MB");
    assert_eq!(format_bytes(5_368_709_120), "5.0 GB");
}

// ------------------------------------------------------------------------- fonts (layout 7)

#[test]
fn all_fonts_are_compiled_in_and_wired_to_their_families() {
    let fonts = theme::font_definitions();
    for key in ["sans-regular", "sans-semibold-font", "sans-bold-font", "jbm-regular", "jbm-semibold", "phosphor"] {
        assert!(fonts.font_data.contains_key(key), "font {key}");
    }
    let first = |f: egui::FontFamily| fonts.families.get(&f).and_then(|v| v.first().cloned()).unwrap_or_default();
    assert_eq!(first(egui::FontFamily::Proportional), "sans-regular");
    assert_eq!(first(theme::family_semibold()), "sans-semibold-font");
    assert_eq!(first(theme::family_bold()), "sans-bold-font");
    assert_eq!(first(egui::FontFamily::Monospace), "jbm-regular");
    assert_eq!(first(theme::family_mono_semibold()), "jbm-semibold");
    // the compiled-in bytes are the files in assets/fonts (so a changed file is picked up by a build)
    for (key, file) in [
        ("sans-regular", "SourceSans3-Regular.ttf"),
        ("sans-semibold-font", "SourceSans3-Semibold.ttf"),
        ("sans-bold-font", "SourceSans3-Bold.ttf"),
        ("jbm-regular", "JetBrainsMono-Regular.ttf"),
        ("jbm-semibold", "JetBrainsMono-SemiBold.ttf"),
    ] {
        let on_disk = std::fs::read(format!("assets/fonts/{file}")).expect("font file in the repository");
        assert_eq!(fonts.font_data[key].font.as_ref(), on_disk.as_slice(), "{key} is {file}");
    }
}

#[test]
fn the_text_font_really_is_source_sans_3_and_the_number_font_jetbrains_mono() {
    // Different fonts measure the same text differently; compare with egui's own default font.
    let width = |fonts: egui::FontDefinitions, family: egui::FontFamily| {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts);
        let _ = ctx.run(Default::default(), |_| {});
        ctx.fonts(|f| f.layout_no_wrap("Fixed term deposit 20400-01".into(), egui::FontId::new(14.0, family), egui::Color32::WHITE).size().x)
    };
    let ours = theme::font_definitions();
    let sans = width(ours.clone(), egui::FontFamily::Proportional);
    let default = width(egui::FontDefinitions::default(), egui::FontFamily::Proportional);
    assert!((sans - default).abs() > 1.0, "Source Sans 3 is not egui's default font: {sans} vs {default}");
    // JetBrains Mono: every character the same width (26 characters at 14 px)
    let mono_a = width(ours.clone(), egui::FontFamily::Monospace);
    assert!((mono_a / 27.0 - 0.6 * 14.0).abs() < 1.0, "mono advance is 0.6 em: {}", mono_a / 27.0);
}

#[test]
fn no_font_is_read_from_disk_at_run_time_and_one_licence_file_per_family() {
    let theme_src = std::fs::read_to_string("src/ui/theme.rs").expect("theme.rs");
    // (the strings are built here so this test does not match itself)
    for banned in [["current", "_exe"].concat(), ["CARGO_MANIFEST", "_DIR"].concat(), ["fs::", "read("].concat(), ["read", "_sans"].concat()] {
        assert!(!theme_src.contains(&banned), "theme.rs still has `{banned}`");
    }
    assert!(theme_src.matches("include_bytes!").count() >= 5, "all five fonts are compiled in");
    let names: Vec<String> = std::fs::read_dir("assets/fonts")
        .expect("assets/fonts")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    let ofl_source_sans = names.iter().filter(|n| n.to_lowercase().contains("ofl") && n.to_lowercase().contains("sourcesans3")).count();
    assert_eq!(ofl_source_sans, 1, "one Source Sans 3 licence file: {names:?}");
    assert!(names.iter().any(|n| n == "OFL-SourceSans3.md") && names.iter().any(|n| n == "OFL-JetBrainsMono.txt"));
    let notices = std::fs::read_to_string("THIRD_PARTY_NOTICES.md").expect("notices");
    assert!(notices.contains("OFL-SourceSans3.md") && notices.contains("OFL-JetBrainsMono.txt"));
    assert!(notices.contains("SourceSans3-Regular") || notices.contains("SourceSans3-*.ttf"));
    assert!(!notices.contains("not in this repository"), "the notices no longer say the TTFs are missing");
}

// ------------------------------------------------------------- no size literals outside theme.rs

/// The float literals of one line of Rust, outside strings, characters and `//` comments, and
/// whether the line carries the marker that says its numbers are not sizes.
fn float_literals(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut in_str = false;
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            if c == '\\' {
                i += 2;
                continue;
            }
            if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_str = true;
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            break;
        }
        if c == '\'' {
            // a character literal such as '\n' or 'x' (a lifetime has no closing quote soon)
            if let Some(close) = chars[i + 1..].iter().take(4).position(|&k| k == '\'') {
                i += close + 2;
                continue;
            }
        }
        let prev_is_word = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_' || chars[i - 1] == '.');
        if c.is_ascii_digit() && !prev_is_word {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
                i += 1;
            }
            let mut is_float = false;
            if chars.get(i) == Some(&'.') && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
                is_float = true;
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
                    i += 1;
                }
            }
            let rest: String = chars[i..].iter().take(4).collect();
            if rest.starts_with("_f32") || rest.starts_with("f32") {
                is_float = true;
                i += if rest.starts_with('_') { 4 } else { 3 };
            }
            if is_float {
                out.push(chars[start..i].iter().collect());
            }
            continue;
        }
        i += 1;
    }
    out
}

#[test]
fn the_literal_scanner_finds_sizes_and_skips_strings_comments_and_arithmetic() {
    assert_eq!(float_literals("ui.add_space(8.0);"), ["8.0"]);
    assert_eq!(float_literals("x.size(11.0).color(c); // 12.0"), ["11.0"]);
    assert_eq!(float_literals("ui.label(\"1,234.50 and 7.5\");"), Vec::<String>::new());
    assert_eq!(float_literals("let a = 10_000_000.0 / 3.0;"), ["10_000_000.0", "3.0"]);
    assert_eq!(float_literals("Stroke::new(1.0_f32, c)"), ["1.0_f32"]);
    assert_eq!(float_literals("v.min(5f32)"), ["5f32"]);
    assert_eq!(float_literals("let c = '.'; self.0.5"), Vec::<String>::new());
    assert_eq!(float_literals("a[0].0 + b.1"), Vec::<String>::new());
}

#[test]
fn no_size_literal_is_left_outside_theme_rs() {
    // The acceptance check of the layout round. Colours: no hex colour, `from_rgb`, `from_black_alpha`
    // and the like outside theme.rs. Sizes: no float literal other than 0.0, 0.5, 1.0 and 2.0 (zero,
    // a half-pixel line offset, one and a half) in the UI sources, except on a line that says
    // `// not a size: <why>` (a rate, a unit, a drag speed, an example in a text).
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir("src/ui")
        .expect("src/ui")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            !matches!(n, "theme.rs" | "tests.rs" | "layout_tests.rs" | "render_tests.rs")
        })
        .collect();
    files.push("src/main.rs".into());
    let mut bad = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).expect("read source");
        for (n, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            let code = line.split("//").next().unwrap_or(line);
            if code.contains("Color32::from_") && !code.contains("Color32::from_black_alpha(theme::") {
                bad.push(format!("{}:{}: colour literal: {}", f.display(), n + 1, line.trim()));
            }
            if line.contains("not a size") {
                continue;
            }
            for lit in float_literals(line) {
                if !matches!(lit.as_str(), "0.0" | "0.5" | "1.0" | "2.0") {
                    bad.push(format!("{}:{}: size literal {}: {}", f.display(), n + 1, lit, line.trim()));
                }
            }
        }
    }
    assert!(bad.is_empty(), "literals that belong in theme.rs:\n{}", bad.join("\n"));
}


