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

// ------------------------------------------------------------------------ schema dialog

use crate::ui::schema::{PendingCsvModal, SchemaCardRects};
use micro_parquet::models::ColumnSchemaConfig;
use polars::prelude::NamedFrom;

fn schema_modal(rows: usize, columns: usize, parquet: bool) -> PendingCsvModal {
    let names: Vec<String> = (0..4).map(|i| format!("Col{i}")).collect();
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
    let lay = theme::dialog_layout(WIN_H_PX / UI_SCALE, 22.0, 6.0);
    let mut rects = None;
    let mut middle = egui::Rect::NOTHING;
    let mut texts = Vec::new();
    for _ in 0..3 {
        let out = ctx.run(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let r = ui
                    .allocate_ui(egui::vec2(theme::SCHEMA_DIALOG_W - 2.0 * theme::SPACE_M, lay.body_max_h), |ui| {
                        let mut reread = false;
                        app.schema_cards(ui, &mut modal, &mut reread)
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
fn columns_card_takes_the_remaining_height() {
    for n in [3usize, 60, 400] {
        let (r, _, middle) = drawn_schema(5, n, false);
        assert!((r.columns.bottom() - middle.bottom()).abs() < 1.5, "{n} columns: card ends at {} but the middle at {}", r.columns.bottom(), middle.bottom());
        assert!(r.columns.height() > 150.0, "{n} columns: {:?}", r.columns);
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
                        app.schema_cards(ui, &mut modal, &mut reread);
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

