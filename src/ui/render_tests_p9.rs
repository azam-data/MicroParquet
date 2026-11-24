//! Phase 9 render tests: headless egui renders at 1219×686, 1600×900 and 1291×700 pt, by real clicks.
//!
//! P1 (the pop-out pivot builder): the builder's window is a second egui context here (a headless backend has no second window),
//! sized 900×600 and 1600×1000, that draws what the window draws (`pivot_popout_frame`) from the same state as the docked dialog.

use crate::ui::layout_tests::{collect_painted, Painted};
use crate::ui::pivot_builder::{popout_placement, PivotRects, PopoutPlacement, PIVOT_POPOUT_NOTE_TITLE};
use crate::ui::prelude::*;
use crate::ui::render_tests::{assert_dialog_fits, inside, screen};
use crate::ui::render_tests_p4::Rig;
use crate::ui::render_tests_p7::SIZES3;
use crate::ui::render_tests_p8::{assert_no_overlap, click_rect, rig8};
use crate::ui::tests::settle;
use micro_parquet::commands::CommandId;
use polars::prelude::{DataFrame, NamedFrom, Series};

/// The sizes of the builder's window.
const POPS: [(f32, f32); 2] = [(900.0, 600.0), (1600.0, 1000.0)];

/// 150 branches (so the pivot has more rows than any preview shows), three products and a balance.
fn book9() -> DataFrame {
    let n = 150usize;
    DataFrame::new(vec![
        Series::new("Branch".into(), (0..n).map(|i| format!("B{:03}", i + 1)).collect::<Vec<_>>()).into(),
        Series::new("Product".into(), (0..n).map(|i| ["Gold", "Silver", "Classic"][i % 3]).collect::<Vec<_>>()).into(),
        Series::new("Balance".into(), (0..n).map(|i| 1000.0 + 10.0 * i as f64).collect::<Vec<_>>()).into(),
    ])
    .expect("book9")
}

/// A rig on the 150-branch tab, as eframe's windows: a second window can be opened.
fn rig9(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let rig = rig8(name, ppp, w, h, book9());
    rig.ctx.set_embed_viewports(false);
    rig
}

/// The builder's own window: a context of its size with the clock and the pointer of its own.
struct Pop {
    ctx: egui::Context,
    input: egui::RawInput,
    last: Painted,
}

impl Pop {
    fn new(w: f32, h: f32) -> Pop {
        let (ctx, input) = screen(1.0, w, h);
        Pop { ctx, input, last: Painted::default() }
    }

    fn step(&mut self, rig: &mut Rig, events: Vec<egui::Event>) -> Painted {
        settle(&mut rig.app);
        let mut i = self.input.clone();
        i.events = events;
        let out = self.ctx.run(i, |c| rig.app.pivot_popout_frame(c));
        self.last = collect_painted(&out.shapes);
        self.input.time = Some(self.input.time.unwrap_or(0.0) + 0.05);
        std::mem::take(&mut self.last)
    }

    fn draw(&mut self, rig: &mut Rig, n: usize) -> Painted {
        let mut p = Painted::default();
        for _ in 0..n {
            p = self.step(rig, vec![]);
        }
        settle(&mut rig.app);
        p
    }

    /// A real click: the pointer moves there, presses and lets go a second later.
    fn click(&mut self, rig: &mut Rig, r: egui::Rect) -> Painted {
        let pos = r.center();
        self.step(rig, vec![egui::Event::PointerMoved(pos)]);
        self.input.time = Some(self.input.time.unwrap_or(0.0) + 1.0);
        self.step(rig, vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }]);
        self.step(rig, vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
        self.input.time = Some(self.input.time.unwrap_or(0.0) + 1.0);
        self.draw(rig, 3)
    }

    fn key(&mut self, rig: &mut Rig, key: egui::Key) -> Painted {
        let ev = |pressed| egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers: egui::Modifiers::NONE };
        self.step(rig, vec![ev(true), ev(false)]);
        self.draw(rig, 2)
    }
}

/// Where the clicks go: the docked dialog (the app window) or the builder's own window.
enum Place {
    Docked,
    Window(Pop),
}

impl Place {
    fn click(&mut self, rig: &mut Rig, r: egui::Rect) -> Painted {
        match self {
            Place::Docked => click_rect(rig, r),
            Place::Window(p) => p.click(rig, r),
        }
    }
    fn draw(&mut self, rig: &mut Rig, n: usize) -> Painted {
        match self {
            Place::Docked => rig.draw(n),
            Place::Window(p) => p.draw(rig, n),
        }
    }
    fn rects(&self, rig: &Rig) -> PivotRects {
        rig.app.pivot_builder.rects.clone()
    }
}

fn button(rig: &Rig, name: &str) -> Option<egui::Rect> {
    rig.app.pivot_builder.rects.buttons.iter().find(|(n, _)| n == name).map(|(_, r)| *r)
}

fn last_box(p: &Painted, text: &str, below: f32) -> Option<egui::Rect> {
    p.boxes.iter().rev().find(|(t, b)| t == text && b.top() > below - 1.0).map(|(_, b)| *b)
}

/// "Add to <area>" on a field: its menu, then the item, by real clicks.
fn add_to(place: &mut Place, rig: &mut Rig, field: &str, area: &str, what: &str) {
    place.draw(rig, 2);
    let menu = place.rects(rig).field_menus.iter().find(|(n, _)| n == field).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the menu of '{field}'"));
    let p = place.click(rig, menu);
    let item = last_box(&p, &format!("Add to {area}"), menu.top()).unwrap_or_else(|| panic!("{what}: 'Add to {area}' in the menu of '{field}'"));
    place.click(rig, item);
}

/// The clicks of the owner's example: Branch to Rows, Balance to Values, a filter on Product with Silver unticked.
fn build_pivot(place: &mut Place, rig: &mut Rig, what: &str) {
    add_to(place, rig, "Branch", "Rows", what);
    add_to(place, rig, "Balance", "Values", what);
    add_to(place, rig, "Product", "Filters", what);
    let p = place.draw(rig, 6);
    let silver = p.boxes.iter().rev().find(|(t, _)| t == "Silver").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the Filters list shows Silver: {:?}", p.boxes.iter().map(|b| b.0.clone()).collect::<Vec<_>>()));
    place.click(rig, silver);
    let p = place.draw(rig, 2);
    let done = last_box(&p, "Done", 0.0).unwrap_or_else(|| panic!("{what}: Done"));
    place.click(rig, done);
    place.draw(rig, 4);
}

/// The step the last tab got.
fn last_step(rig: &Rig) -> serde_json::Value {
    let tab = rig.app.tabs.last().expect("a tab");
    serde_json::to_value(tab.steps.last().expect("a step")).expect("json")
}

fn pop_out(rig: &mut Rig, what: &str) {
    let p = rig.draw(6);
    let b = button(rig, "Pop out").unwrap_or_else(|| panic!("{what}: Pop out in the header: {:?}", p.boxes.iter().map(|b| b.0.clone()).collect::<Vec<_>>()));
    click_rect(rig, b);
}

#[test]
fn p1_pop_out_by_a_real_click_edits_the_same_state_and_apply_makes_the_step_the_docked_dialog_makes() {
    for (ppp, w, h) in SIZES3 {
        // the docked dialog, by real clicks
        let mut docked = rig9("p9p1_docked", ppp, w, h);
        let what = docked.what.clone();
        docked.app.run_command(&docked.ctx, CommandId::Pivot);
        docked.draw(8);
        build_pivot(&mut Place::Docked, &mut docked, &what);
        let spec_docked = docked.app.pivot_builder.spec.clone();
        assert_eq!(spec_docked.rows, vec!["Branch".to_string()], "{what}");
        assert_eq!(spec_docked.filters.len(), 1, "{what}");
        let create = button(&docked, "Create pivot tab").unwrap_or_else(|| panic!("{what}: Create pivot tab"));
        click_rect(&mut docked, create);
        docked.draw(6);
        let want = last_step(&docked);

        for (pw, ph) in POPS {
            let what = format!("{what}, window {pw}x{ph}");
            let mut rig = rig9("p9p1_popped", ppp, w, h);
            rig.app.run_command(&rig.ctx, CommandId::Pivot);
            rig.draw(8);
            assert_eq!(rig.app.active_tool, ActiveToolModal::PivotBuilder, "{what}");
            // Pop out: the docked dialog goes, the app window says where the builder is
            pop_out(&mut rig, &what);
            assert!(rig.app.pivot_builder.popped && rig.app.pivot_builder.pop_requests == 1, "{what}: a click on Pop out asked for the window");
            let p = rig.draw(4);
            assert!(rig.app.pivot_builder.popped, "{what}: a backend that cannot open a window does not dock it back by itself");
            assert!(!p.boxes.iter().any(|(t, _)| t.contains("Pivot table")), "{what}: never both: the docked dialog is not drawn while the window has the builder");
            assert!(p.boxes.iter().any(|(t, _)| t.contains("own window")), "{what}: the app window says so");
            assert!(rig.app.pivot_builder.rects.note_dock.is_positive(), "{what}: with a Dock button");
            // the clicks, in the window
            let mut place = Place::Window(Pop::new(pw, ph));
            place.draw(&mut rig, 6);
            build_pivot(&mut place, &mut rig, &what);
            assert_eq!(rig.app.pivot_builder.spec.rows, spec_docked.rows, "{what}");
            assert_eq!(format!("{:?}", rig.app.pivot_builder.spec), format!("{:?}", spec_docked), "{what}: the same state the docked clicks make");
            // the preview follows
            let prev = rig.app.pivot_builder.preview.clone();
            assert!(matches!(prev, Some(Ok(_))), "{what}: the preview updates in the window: {prev:?}");
            // Apply in the window: the step the docked dialog made
            let create = button(&rig, "Create pivot tab").unwrap_or_else(|| panic!("{what}: Create pivot tab in the window"));
            place.click(&mut rig, create);
            rig.draw(6);
            assert_eq!(last_step(&rig), want, "{what}: the same step, as JSON");
            assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: Apply closes the builder");
            assert!(!rig.app.pivot_builder.popped && !rig.app.pivot_builder.open, "{what}");
        }
    }
}

/// Pops the builder out and gives its window.
fn popped(rig: &mut Rig, pw: f32, ph: f32, what: &str) -> Place {
    pop_out(rig, what);
    rig.draw(3);
    assert!(rig.app.pivot_builder.popped, "{what}: popped out");
    let mut place = Place::Window(Pop::new(pw, ph));
    place.draw(rig, 6);
    place
}

fn chips(rig: &Rig) -> Vec<(String, String)> {
    let mut c: Vec<(String, String)> = rig.app.pivot_builder.rects.chips.iter().map(|(a, f, _)| (a.title().to_string(), f.clone())).collect();
    c.sort();
    c
}

#[test]
fn p1_the_dock_button_and_the_windows_close_button_dock_the_builder_back_with_everything_as_it_was() {
    for (ppp, w, h) in SIZES3 {
        for close_button in [false, true] {
            let mut rig = rig9("p9p1_dock", ppp, w, h);
            let what = format!("{} ({})", rig.what, if close_button { "the window's close button" } else { "Dock" });
            rig.app.run_command(&rig.ctx, CommandId::Pivot);
            rig.draw(8);
            let mut place = popped(&mut rig, 900.0, 600.0, &what);
            // fields, a value and a filter whose list is still open: nothing is applied
            add_to(&mut place, &mut rig, "Branch", "Rows", &what);
            add_to(&mut place, &mut rig, "Product", "Columns", &what);
            add_to(&mut place, &mut rig, "Balance", "Values", &what);
            add_to(&mut place, &mut rig, "Product", "Filters", &what);
            place.draw(&mut rig, 6);
            let spec = format!("{:?}", rig.app.pivot_builder.spec);
            let popup = rig.app.pivot_builder.popup.as_ref().map(|p| p.field.clone());
            assert_eq!(popup.as_deref(), Some("Product"), "{what}: the Filters list is open");
            let before = chips(&rig);
            assert!(before.len() >= 4, "{what}: {before:?}");
            let tabs = rig.app.tabs.len();
            // dock it back
            let Place::Window(pop) = &mut place else { panic!("a window") };
            if close_button {
                pop.input.viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
                pop.draw(&mut rig, 1);
                pop.input.viewports.entry(egui::ViewportId::ROOT).or_default().events.clear();
            } else {
                let dock = button(&rig, "Dock").unwrap_or_else(|| panic!("{what}: Dock in the window's header"));
                pop.click(&mut rig, dock);
            }
            assert!(!rig.app.pivot_builder.popped && rig.app.pivot_builder.open, "{what}: docked, still open");
            assert_eq!(rig.app.tabs.len(), tabs, "{what}: nothing was applied");
            let p = rig.draw(8);
            assert!(p.boxes.iter().any(|(t, _)| t.contains("Pivot table")), "{what}: the docked dialog is back");
            assert!(!p.boxes.iter().any(|(t, _)| t.contains("own window")), "{what}: the note is gone");
            assert_eq!(format!("{:?}", rig.app.pivot_builder.spec), spec, "{what}: every field, value, filter and layout choice as it was");
            assert_eq!(chips(&rig), before, "{what}: the docked dialog shows the same fields");
            // the Filters list is one click from closing: a click outside it (the Dock button) closes it, as it does docked; the window's
            // close button is not a click in the window, and the list stays
            let open_now = rig.app.pivot_builder.popup.as_ref().map(|p| p.field.clone());
            assert_eq!(open_now, if close_button { popup } else { None }, "{what}: the Filters list");
            assert!(matches!(rig.app.pivot_builder.preview, Some(Ok(_))), "{what}: and its preview");
            assert_eq!(rig.app.active_tool, ActiveToolModal::PivotBuilder, "{what}");
            assert_dialog_fits(&rig.ctx, &format!("{}  Pivot table", theme::ph::TABLE), &what);
        }
    }
}

/// A tab with a pivot step on it, the tab active, the builder editing that step (docked).
fn editing_rig(ppp: f32, w: f32, h: f32) -> (Rig, serde_json::Value) {
    let mut rig = rig9("p9p1_edit", ppp, w, h);
    let what = rig.what.clone();
    rig.app.run_command(&rig.ctx, CommandId::Pivot);
    rig.draw(8);
    build_pivot(&mut Place::Docked, &mut rig, &what);
    let create = button(&rig, "Create pivot tab").expect("Create pivot tab");
    click_rect(&mut rig, create);
    rig.draw(6);
    let step = last_step(&rig);
    let at = rig.app.tabs.len() - 1;
    rig.app.active_tab = at;
    assert!(rig.app.open_step_editor(0), "{what}: a pivot step has an editor");
    rig.draw(8);
    assert!(rig.app.editing.is_some(), "{what}");
    (rig, step)
}

#[test]
fn p1_cancel_in_the_window_leaves_the_edited_step_unchanged_and_esc_is_cancel_unless_a_list_is_open() {
    for (ppp, w, h) in SIZES3 {
        for (pw, ph) in POPS {
            // Cancel
            let (mut rig, step) = editing_rig(ppp, w, h);
            let what = format!("{}, window {pw}x{ph}", rig.what);
            let mut place = popped(&mut rig, pw, ph, &what);
            add_to(&mut place, &mut rig, "Product", "Columns", &what);
            assert!(rig.app.pivot_builder.spec.columns.contains(&"Product".to_string()), "{what}: changed in the window");
            let cancel = button(&rig, "Cancel").unwrap_or_else(|| panic!("{what}: Cancel in the window"));
            place.click(&mut rig, cancel);
            rig.draw(4);
            let tab = rig.app.tabs.last().expect("the tab");
            assert_eq!(serde_json::to_value(&tab.steps[0]).expect("json"), step, "{what}: Cancel leaves the step as it was");
            assert!(rig.app.editing.is_none() && rig.app.active_tool == ActiveToolModal::None && !rig.app.pivot_builder.open && !rig.app.pivot_builder.popped, "{what}: the builder is closed");

            // Esc is Cancel
            let (mut rig, step) = editing_rig(ppp, w, h);
            let mut place = popped(&mut rig, pw, ph, &what);
            add_to(&mut place, &mut rig, "Product", "Columns", &what);
            let Place::Window(pop) = &mut place else { panic!("a window") };
            pop.key(&mut rig, egui::Key::Escape);
            assert!(rig.app.editing.is_none() && !rig.app.pivot_builder.open, "{what}: Esc closed the builder");
            assert_eq!(serde_json::to_value(&rig.app.tabs.last().expect("the tab").steps[0]).expect("json"), step, "{what}: and left the step as it was");

            // with the Filters list open, Esc closes the list and nothing else
            let (mut rig, _) = editing_rig(ppp, w, h);
            let mut place = popped(&mut rig, pw, ph, &what);
            add_to(&mut place, &mut rig, "Branch", "Filters", &what);
            place.draw(&mut rig, 4);
            assert!(rig.app.pivot_builder.popup.is_some(), "{what}: the list is open");
            let Place::Window(pop) = &mut place else { panic!("a window") };
            pop.key(&mut rig, egui::Key::Escape);
            assert!(rig.app.pivot_builder.popup.is_none() && rig.app.pivot_builder.open && rig.app.pivot_builder.popped, "{what}: Esc closed the list only");
            pop.key(&mut rig, egui::Key::Escape);
            assert!(!rig.app.pivot_builder.open, "{what}: the next Esc is Cancel");
        }
    }
}

/// How many rows of the pivot are on screen in the preview (the rows' first labels inside the scrolling area).
fn rows_on_screen(p: &Painted, area: egui::Rect) -> usize {
    p.glyphs.iter().filter(|(t, b)| t.len() == 4 && t.starts_with('B') && t[1..].chars().all(|c| c.is_ascii_digit()) && inside(*b, area)).count()
}

#[test]
fn p1_the_window_previews_as_many_rows_as_fit_more_than_the_docked_dialog_and_uses_its_width() {
    let ppp = 1.05;
    let (w, h) = (1291.0, 700.0);
    // docked: the first 20 rows
    let mut docked = rig9("p9p1_rows_d", ppp, w, h);
    docked.app.run_command(&docked.ctx, CommandId::Pivot);
    docked.draw(8);
    add_to(&mut Place::Docked, &mut docked, "Branch", "Rows", "docked");
    add_to(&mut Place::Docked, &mut docked, "Balance", "Values", "docked");
    let p = docked.draw(6);
    let d_df = match &docked.app.pivot_builder.preview {
        Some(Ok(df)) => df.clone(),
        other => panic!("docked preview: {other:?}"),
    };
    assert_eq!(d_df.height(), 20, "the docked dialog brings back 20 rows");
    let d_rects = docked.app.pivot_builder.rects.clone();
    let d_on_screen = rows_on_screen(&p, d_rects.preview_scroll);
    assert!(d_on_screen >= 1 && d_on_screen < 20, "the docked preview shows only what fits: {d_on_screen}");

    let mut got: Vec<(f32, f32, usize, usize, f32)> = Vec::new();
    for (pw, ph) in POPS {
        let what = format!("window {pw}x{ph}");
        let mut rig = rig9("p9p1_rows_p", ppp, w, h);
        rig.app.run_command(&rig.ctx, CommandId::Pivot);
        rig.draw(8);
        let mut place = popped(&mut rig, pw, ph, &what);
        add_to(&mut place, &mut rig, "Branch", "Rows", &what);
        add_to(&mut place, &mut rig, "Balance", "Values", &what);
        // the first preview is cut for what fitted when it was asked for; the next frames ask again for the window's height
        let p = place.draw(&mut rig, 10);
        let b = &rig.app.pivot_builder;
        let df = match &b.preview {
            Some(Ok(df)) => df.clone(),
            other => panic!("{what}: preview {other:?}"),
        };
        let r = b.rects.clone();
        let screen2 = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(pw, ph));
        let on = rows_on_screen(&p, r.preview_scroll);
        let fit = ((r.preview_scroll.height() - theme::PIVOT_PREVIEW_ROW_H) / theme::PIVOT_PREVIEW_ROW_H).floor() as usize;
        assert!(df.height() >= 20 && df.height() >= fit, "{what}: {} rows back, {fit} fit", df.height());
        assert!(df.height() % 20 == 0 && df.height() <= 500, "{what}: in steps of 20: {}", df.height());
        assert!(on >= fit.saturating_sub(1).min(df.height()), "{what}: {on} rows on screen, {fit} fit");
        assert!(inside(r.preview, screen2) && inside(r.right, screen2) && inside(r.fields, screen2), "{what}: the panels are inside the window {screen2:?}: {r:?}");
        // the columns use the window's width: the right panel ends at the window's right edge, less its margin and bar
        assert!(screen2.right() - r.right.right() < 40.0, "{what}: the right panel {:?} reaches the window's edge", r.right);
        assert!(r.preview.width() > pw * 0.55, "{what}: the preview {:?} uses the width", r.preview);
        assert!(r.preview.bottom() > ph * 0.8, "{what}: the preview fills the height: {:?}", r.preview);
        got.push((pw, ph, df.height(), on, r.preview.width()));
    }
    let (small, big) = (got[0], got[1]);
    assert!(big.2 > 20 && big.2 > d_df.height(), "the 1600x1000 window brings back more rows than the docked dialog: {got:?}");
    assert!(big.3 > d_on_screen && big.3 > small.3, "and shows more of them: {got:?} against {d_on_screen}");
    assert!(big.4 > small.4 && big.4 > d_rects.preview.width(), "and the preview is wider: {got:?}");
    // docked again, it is the docked count
    let mut rig = rig9("p9p1_rows_back", ppp, w, h);
    rig.app.run_command(&rig.ctx, CommandId::Pivot);
    rig.draw(8);
    let mut place = popped(&mut rig, 1600.0, 1000.0, "back");
    add_to(&mut place, &mut rig, "Branch", "Rows", "back");
    add_to(&mut place, &mut rig, "Balance", "Values", "back");
    place.draw(&mut rig, 10);
    assert!(rig.app.pivot_builder.preview_limit > 20);
    let dock = button(&rig, "Dock").expect("Dock");
    place.click(&mut rig, dock);
    rig.draw(10);
    assert_eq!(rig.app.pivot_builder.preview_limit, 20, "docked, the preview is cut to 20 again");
}

fn monitor_main() -> (egui::Rect, egui::Vec2) {
    (egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(1219.0, 686.0)), egui::vec2(1920.0, 1080.0))
}

#[test]
fn p1_the_saved_window_is_restored_where_the_monitor_shows_it_and_an_off_screen_one_is_centred() {
    let (main, monitor) = monitor_main();
    let d = |size: egui::Vec2| -> egui::Vec2 { size };
    // where it was, on the main window's monitor
    let r = popout_placement(Some([200.0, 150.0, 1000.0, 700.0]), Some(main), Some(monitor));
    assert_eq!(r, PopoutPlacement { size: d(egui::vec2(1000.0, 700.0)), pos: Some(egui::pos2(200.0, 150.0)) });
    // far off the screen (a monitor that is gone), or on a monitor the app cannot see: centred on the main window
    let centred = |size: egui::Vec2| main.center() - size / 2.0;
    for off in [[5000.0, 4000.0, 1000.0, 700.0], [-1500.0, 100.0, 1000.0, 700.0], [100.0, 5000.0, 1000.0, 700.0], [1900.0, 100.0, 1000.0, 700.0]] {
        let r = popout_placement(Some(off), Some(main), Some(monitor));
        assert_eq!(r.size, egui::vec2(1000.0, 700.0), "{off:?}");
        assert_eq!(r.pos, Some(centred(r.size)), "{off:?}: centred on the main window");
    }
    // a strip of the title bar still on the screen counts as seen
    let r = popout_placement(Some([1800.0, 50.0, 1000.0, 700.0]), Some(main), Some(monitor));
    assert_eq!(r.pos, Some(egui::pos2(1800.0, 50.0)), "120 pt of the title bar are on the screen");
    let r = popout_placement(Some([1830.0, 50.0, 1000.0, 700.0]), Some(main), Some(monitor));
    assert_eq!(r.pos, Some(main.center() - r.size / 2.0), "90 pt are not enough: centred");
    // never opened: the default size, centred
    let r = popout_placement(None, Some(main), Some(monitor));
    assert_eq!(r.size, egui::vec2(theme::PIVOT_POPOUT_SIZE[0], theme::PIVOT_POPOUT_SIZE[1]));
    assert_eq!(r.pos, Some(centred(r.size)));
    // too small is made the least it can be; bigger than the monitor is made to fit it
    let r = popout_placement(Some([200.0, 150.0, 300.0, 200.0]), Some(main), Some(monitor));
    assert_eq!(r.size, egui::vec2(theme::PIVOT_POPOUT_MIN_SIZE[0], theme::PIVOT_POPOUT_MIN_SIZE[1]));
    let r = popout_placement(Some([0.0, 0.0, 5000.0, 4000.0]), Some(main), Some(monitor));
    assert!(r.size.x <= monitor.x && r.size.y <= monitor.y, "{r:?}");
    // a window that is not on the first monitor's grid cell: the main window's own monitor is the one that counts
    let on_second = egui::Rect::from_min_size(egui::pos2(2100.0, 100.0), egui::vec2(1219.0, 686.0));
    let r = popout_placement(Some([2200.0, 150.0, 1000.0, 700.0]), Some(on_second), Some(monitor));
    assert_eq!(r.pos, Some(egui::pos2(2200.0, 150.0)), "the monitor to the right of the first");
    // nothing known about the main window: the system places it; a damaged number is not trusted
    assert_eq!(popout_placement(Some([200.0, 150.0, 1000.0, 700.0]), None, None).pos, None);
    let r = popout_placement(Some([f32::NAN, 150.0, 1000.0, 700.0]), Some(main), Some(monitor));
    assert_eq!(r.size, egui::vec2(theme::PIVOT_POPOUT_SIZE[0], theme::PIVOT_POPOUT_SIZE[1]));
}

#[test]
fn p1_the_window_rectangle_is_saved_in_settings_when_the_builder_docks_and_opens_there_next_time() {
    let (ppp, w, h) = (1.05, 1291.0, 700.0);
    let (main, monitor) = monitor_main();
    let mut rig = rig9("p9p1_rect", ppp, w, h);
    {
        let v = rig.input.viewports.entry(egui::ViewportId::ROOT).or_default();
        v.outer_rect = Some(main);
        v.monitor_size = Some(monitor);
    }
    rig.app.run_command(&rig.ctx, CommandId::Pivot);
    rig.draw(8);
    // the first time: the default size, centred
    pop_out(&mut rig, "first");
    rig.draw(3);
    let first = rig.app.pivot_builder.popout_start.expect("where it opened");
    assert_eq!(first, popout_placement(None, Some(main), Some(monitor)));
    // the window is moved and made bigger: its rectangle is read from the window, and kept when it docks
    let mut place = Place::Window(Pop::new(1000.0, 700.0));
    if let Place::Window(pop) = &mut place {
        let v = pop.input.viewports.entry(egui::ViewportId::ROOT).or_default();
        v.outer_rect = Some(egui::Rect::from_min_size(egui::pos2(320.0, 210.0), egui::vec2(1016.0, 739.0)));
        v.inner_rect = Some(egui::Rect::from_min_size(egui::pos2(328.0, 240.0), egui::vec2(1000.0, 700.0)));
    }
    place.draw(&mut rig, 4);
    assert_eq!(rig.app.settings.pivot_popout_rect, None, "nothing is written while the window is open and nothing closed it");
    let dock = button(&rig, "Dock").expect("Dock");
    place.click(&mut rig, dock);
    assert_eq!(rig.app.settings.pivot_popout_rect, Some([320.0, 210.0, 1000.0, 700.0]), "the window's place and size");
    let (loaded, note) = micro_parquet::settings::Settings::load(&rig.app.paths.root_dir);
    assert!(note.is_none());
    assert_eq!(loaded.pivot_popout_rect, Some([320.0, 210.0, 1000.0, 700.0]), "in settings.json");
    // the next time it opens there
    rig.draw(4);
    pop_out(&mut rig, "second");
    rig.draw(3);
    let second = rig.app.pivot_builder.popout_start.expect("where it opened");
    assert_eq!(second, PopoutPlacement { size: egui::vec2(1000.0, 700.0), pos: Some(egui::pos2(320.0, 210.0)) });
    // a rectangle that is off every screen is centred instead
    let mut place = Place::Window(Pop::new(1000.0, 700.0));
    let dock = button(&rig, "Dock").unwrap_or_else(|| {
        place.draw(&mut rig, 3);
        button(&rig, "Dock").expect("Dock")
    });
    place.click(&mut rig, dock);
    rig.app.settings.pivot_popout_rect = Some([9000.0, 7000.0, 1000.0, 700.0]);
    rig.draw(4);
    pop_out(&mut rig, "third");
    rig.draw(3);
    let third = rig.app.pivot_builder.popout_start.expect("where it opened");
    assert_eq!(third.size, egui::vec2(1000.0, 700.0));
    assert_eq!(third.pos, Some(main.center() - third.size / 2.0), "centred on the main window");
}

#[test]
fn p1_where_windows_cannot_open_there_is_no_pop_out_and_a_popped_builder_docks_back() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig8("p9p1_embedded", ppp, w, h, book9());
        let what = rig.what.clone();
        assert!(rig.ctx.embed_viewports(), "a headless backend embeds its windows");
        rig.app.run_command(&rig.ctx, CommandId::Pivot);
        let p = rig.draw(8);
        assert!(button(&rig, "Pop out").is_none(), "{what}: no Pop out where windows cannot open");
        assert!(!p.boxes.iter().any(|(t, _)| t.contains("Pop out")), "{what}");
        rig.app.pivot_builder.popped = true;
        let p = rig.draw(4);
        assert!(!rig.app.pivot_builder.popped, "{what}: docked back");
        assert!(p.boxes.iter().any(|(t, _)| t.contains("Pivot table")), "{what}: the dialog is in the app window");
    }
}

#[test]
fn p1_the_pop_out_button_and_the_windows_header_fit_and_nothing_overlaps_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig9("p9p1_fit", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::Pivot);
        let p = rig.draw(8);
        let title = format!("{}  Pivot table", theme::ph::TABLE);
        let r = assert_dialog_fits(&rig.ctx, &title, &what);
        let pop = button(&rig, "Pop out").unwrap_or_else(|| panic!("{what}: Pop out"));
        assert!(inside(pop, r.header.expand2(egui::vec2(0.0, 6.0))), "{what}: Pop out {pop:?} is in the header {:?}", r.header);
        assert!(r.header.right() - pop.right() < 16.0, "{what}: at the right edge of the header: {pop:?} {:?}", r.header);
        let tab = p.boxes.iter().find(|(t, b)| t == "Deposits" && inside(*b, r.header)).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the tab's name is in the header"));
        assert!(tab.right() <= pop.left() + 0.5, "{what}: the tab's name {tab:?} is left of Pop out {pop:?}");
        let texts = crate::ui::render_tests_p8::dialog_texts(&mut rig, &p, r.window);
        assert_no_overlap(&texts, &what);
        // the window, at both sizes
        for (pw, ph) in POPS {
            let what = format!("{what}, window {pw}x{ph}");
            let mut rig = rig9("p9p1_fit_w", ppp, w, h);
            rig.app.run_command(&rig.ctx, CommandId::Pivot);
            rig.draw(8);
            let mut place = popped(&mut rig, pw, ph, &what);
            add_to(&mut place, &mut rig, "Branch", "Rows", &what);
            add_to(&mut place, &mut rig, "Balance", "Values", &what);
            let p = place.draw(&mut rig, 10);
            let screen2 = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(pw, ph));
            let g = rig.app.pivot_builder.rects.clone();
            for (n, b) in &g.buttons {
                assert!(inside(*b, screen2), "{what}: the button '{n}' {b:?} is inside the window {screen2:?}");
            }
            for name in ["Dock", "Cancel", "Create pivot tab"] {
                assert!(g.buttons.iter().any(|(n, _)| n == name), "{what}: '{name}' is in the window: {:?}", g.buttons);
            }
            let (dock, cancel, create) = (button(&rig, "Dock").expect("Dock"), button(&rig, "Cancel").expect("Cancel"), button(&rig, "Create pivot tab").expect("Create"));
            assert!(dock.top() < g.body.top() && cancel.top() > g.body.bottom() - 1.0 && create.top() > g.body.bottom() - 1.0, "{what}: Dock above the fields, Cancel and Create below them");
            assert!(cancel.right() <= create.left() + 0.5 || create.right() <= cancel.left() + 0.5, "{what}: Cancel and Create do not touch");
            assert!(inside(g.body, screen2) && inside(g.preview, screen2), "{what}: {g:?}");
            // no two labels overlap in the header, the field list, the areas, the footer
            let area = |a: egui::Rect| p.glyphs.iter().filter(|(_, b)| inside(*b, a)).cloned().collect::<Vec<_>>();
            for part in [g.fields, g.areas[0], g.areas[1], g.areas[2], g.areas[3], g.toggles] {
                assert_no_overlap(&area(part), &what);
            }
            let header = egui::Rect::from_min_max(screen2.min, egui::pos2(screen2.right(), g.body.top()));
            let footer = egui::Rect::from_min_max(egui::pos2(screen2.left(), g.body.bottom()), screen2.max);
            assert_no_overlap(&area(header), &what);
            assert_no_overlap(&area(footer), &what);
            crate::ui::render_tests_p8::assert_not_clipped(&p, header, &what);
        }
    }
}

#[test]
fn p1_closing_the_source_tab_with_the_builder_open_behaves_the_same_docked_and_in_its_window() {
    for popped_out in [false, true] {
        let (ppp, w, h) = (1.05, 1291.0, 700.0);
        let mut rig = rig9("p9p1_close", ppp, w, h);
        let what = format!("{} popped {popped_out}", rig.what);
        // a second tab, so there is one left when the first closes
        let src = crate::ui::tests::parquet_source(&rig.app.paths.root_dir.clone(), "other", book9());
        rig.app.create_tab_from_source("Other".into(), src, vec![], std::collections::HashMap::new());
        settle(&mut rig.app);
        rig.app.active_tab = 0;
        rig.app.run_command(&rig.ctx, CommandId::Pivot);
        rig.draw(8);
        let mut place = if popped_out { popped(&mut rig, 900.0, 600.0, &what) } else { Place::Docked };
        add_to(&mut place, &mut rig, "Branch", "Rows", &what);
        let spec = format!("{:?}", rig.app.pivot_builder.spec);
        rig.app.close_tab(0);
        place.draw(&mut rig, 4);
        rig.draw(4);
        assert!(rig.app.pivot_builder.open && rig.app.pivot_builder.popped == popped_out, "{what}: the builder stays where it was, as the docked dialog does");
        assert_eq!(format!("{:?}", rig.app.pivot_builder.spec), spec, "{what}: with its choices");
        // and with no tab left nothing panics
        rig.app.close_tab(0);
        place.draw(&mut rig, 4);
        rig.draw(4);
        assert!(rig.app.pivot_builder.open, "{what}: still open");
    }
}

#[test]
fn p1_the_note_in_the_app_window_docks_the_builder_by_a_real_click() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig9("p9p1_note", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::Pivot);
        rig.draw(8);
        let _place = popped(&mut rig, 900.0, 600.0, &what);
        let p = rig.draw(4);
        let r = assert_dialog_fits(&rig.ctx, PIVOT_POPOUT_NOTE_TITLE, &what);
        let dock = rig.app.pivot_builder.rects.note_dock;
        assert!(inside(dock, r.window), "{what}: Dock {dock:?} is in the note {:?}", r.window);
        assert!(p.boxes.iter().any(|(t, _)| t.contains("own window")), "{what}");
        click_rect(&mut rig, dock);
        rig.draw(6);
        assert!(!rig.app.pivot_builder.popped && rig.app.pivot_builder.open, "{what}: docked by the note's button");
    }
}
