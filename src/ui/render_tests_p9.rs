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
use crate::ui::render_tests_p8::{assert_no_overlap, assert_not_clipped, click_rect, rig8};
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
    place.draw(rig, 5);
    let menu = place.rects(rig).field_menus.iter().find(|(n, _)| n == field).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the menu of '{field}'"));
    let mut p = place.click(rig, menu);
    // (the first click on the second field's menu of a fresh dialog does not always open it in a headless frame sequence: once more)
    if last_box(&p, &format!("Add to {area}"), menu.top()).is_none() {
        p = place.click(rig, menu);
    }
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
fn p1_the_saved_window_is_used_as_it_is_on_any_monitor_and_it_is_centred_only_when_none_is_saved() {
    let (main, monitor) = monitor_main();
    let centred = |size: egui::Vec2| main.center() - size / 2.0;
    // where it was, and as big as it was, wherever that is: on the main window's monitor, off every screen, on a monitor to the left,
    // above, below or to the right, with next to none of it on a screen
    for at in [[200.0, 150.0], [5000.0, 4000.0], [-1500.0, 100.0], [100.0, 5000.0], [1900.0, 100.0], [1830.0, 50.0], [-990.0, -690.0], [2500.0, 300.0]] {
        let saved = [at[0], at[1], 1000.0, 700.0];
        let r = popout_placement(Some(saved), Some(main), Some(monitor));
        assert_eq!(r, PopoutPlacement { size: egui::vec2(1000.0, 700.0), pos: Some(egui::pos2(at[0], at[1])) }, "{saved:?}: used as it is");
        // and the same when nothing is known of the monitor or of the main window
        assert_eq!(popout_placement(Some(saved), None, None).pos, Some(egui::pos2(at[0], at[1])), "{saved:?}");
    }
    // a size bigger than the monitor is not cut either
    let r = popout_placement(Some([0.0, 0.0, 5000.0, 4000.0]), Some(main), Some(monitor));
    assert_eq!(r, PopoutPlacement { size: egui::vec2(5000.0, 4000.0), pos: Some(egui::pos2(0.0, 0.0)) });
    // never opened: the default size, centred on the main window
    let r = popout_placement(None, Some(main), Some(monitor));
    assert_eq!(r.size, egui::vec2(theme::PIVOT_POPOUT_SIZE[0], theme::PIVOT_POPOUT_SIZE[1]));
    assert_eq!(r.pos, Some(centred(r.size)));
    // the window can never be made smaller than the least it can be
    let r = popout_placement(Some([200.0, 150.0, 300.0, 200.0]), Some(main), Some(monitor));
    assert_eq!(r, PopoutPlacement { size: egui::vec2(theme::PIVOT_POPOUT_MIN_SIZE[0], theme::PIVOT_POPOUT_MIN_SIZE[1]), pos: Some(egui::pos2(200.0, 150.0)) });
    // nothing saved and nothing known about the main window: the system places it
    assert_eq!(popout_placement(None, None, None).pos, None);
    // a damaged number is not trusted: it is as if nothing was saved
    let r = popout_placement(Some([f32::NAN, 150.0, 1000.0, 700.0]), Some(main), Some(monitor));
    assert_eq!(r.size, egui::vec2(theme::PIVOT_POPOUT_SIZE[0], theme::PIVOT_POPOUT_SIZE[1]));
    assert_eq!(r.pos, Some(centred(r.size)));
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
    // a rectangle that is off every screen is used as it is (owner decision on P1-2): the window opens there, and the note's Dock button
    // in the app window, by a real click, brings the builder back
    let mut place = Place::Window(Pop::new(1000.0, 700.0));
    let dock = button(&rig, "Dock").unwrap_or_else(|| {
        place.draw(&mut rig, 3);
        button(&rig, "Dock").expect("Dock")
    });
    place.click(&mut rig, dock);
    rig.app.settings.pivot_popout_rect = Some([9000.0, 7000.0, 1000.0, 700.0]);
    rig.draw(4);
    pop_out(&mut rig, "third");
    rig.draw(4);
    let third = rig.app.pivot_builder.popout_start.expect("where it opened");
    assert_eq!(third, PopoutPlacement { size: egui::vec2(1000.0, 700.0), pos: Some(egui::pos2(9000.0, 7000.0)) }, "off every screen, and not moved");
    assert!(rig.app.pivot_builder.popped, "the window is open, wherever it is");
    let note = rig.app.pivot_builder.rects.note_dock;
    assert!(note.is_positive(), "the app window's note has its Dock button");
    click_rect(&mut rig, note);
    rig.draw(6);
    assert!(!rig.app.pivot_builder.popped && rig.app.pivot_builder.open, "the Dock button in the app window docked it back");
    assert!(rig.app.pivot_builder.popout_start.is_none(), "and the next Pop out places it afresh");
    assert_eq!(rig.app.settings.pivot_popout_rect, Some([9000.0, 7000.0, 1000.0, 700.0]), "the saved rectangle is left as it was: the window never drew");
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

impl Place {
    /// A real key press and release, in the place the builder is drawn.
    fn key(&mut self, rig: &mut Rig, key: egui::Key) -> Painted {
        match self {
            Place::Docked => {
                let ev = |pressed| egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers: egui::Modifiers::NONE };
                rig.step(vec![ev(true), ev(false)]);
                rig.draw(2)
            }
            Place::Window(p) => p.key(rig, key),
        }
    }

    /// A text box has the keyboard (in the app window, or in the builder's window).
    fn wants_keyboard(&self, rig: &Rig) -> bool {
        match self {
            Place::Docked => rig.ctx.wants_keyboard_input(),
            Place::Window(p) => p.ctx.wants_keyboard_input(),
        }
    }
}

/// An editing rig (the pivot step open in the builder) with the builder where `at` says: 0 docked, 1 and 2 the window at 900x600 and 1600x1000.
fn esc_rig(ppp: f32, w: f32, h: f32, at: usize) -> (Rig, Place, serde_json::Value, String) {
    let (mut rig, step) = editing_rig(ppp, w, h);
    let what = format!("{}, {}", rig.what, ["docked", "window 900x600", "window 1600x1000"][at]);
    let place = if at == 0 {
        rig.draw(4);
        Place::Docked
    } else {
        let (pw, ph) = POPS[at - 1];
        popped(&mut rig, pw, ph, &what)
    };
    (rig, place, step, what)
}

fn step_is(rig: &Rig, step: &serde_json::Value, what: &str) {
    assert_eq!(&serde_json::to_value(&rig.app.tabs.last().expect("the tab").steps[0]).expect("json"), step, "{what}: the step is as it was");
}

#[test]
fn p1_esc_is_cancel_docked_and_in_the_window_unless_a_text_box_or_an_open_list_has_it() {
    for (ppp, w, h) in SIZES3 {
        for at in 0..3 {
            // Esc is Cancel: the builder closes and the step is as it was
            let (mut rig, mut place, step, what) = esc_rig(ppp, w, h, at);
            add_to(&mut place, &mut rig, "Product", "Columns", &what);
            assert!(rig.app.pivot_builder.spec.columns.contains(&"Product".to_string()), "{what}: changed");
            place.key(&mut rig, egui::Key::Escape);
            rig.draw(4);
            assert!(!rig.app.pivot_builder.open && rig.app.editing.is_none() && rig.app.active_tool == ActiveToolModal::None, "{what}: Esc cancelled the builder");
            step_is(&rig, &step, &what);

            // a text box has the keyboard (the field search): Esc is for the text box, not for the builder
            let (mut rig, mut place, step, what) = esc_rig(ppp, w, h, at);
            let p = place.draw(&mut rig, 4);
            let search = p.boxes.iter().find(|(t, _)| t.contains("fields\u{2026}")).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the field search box: {:?}", p.boxes.iter().map(|b| b.0.clone()).collect::<Vec<_>>()));
            place.click(&mut rig, search);
            assert!(place.wants_keyboard(&rig), "{what}: the search box has the keyboard");
            place.key(&mut rig, egui::Key::Escape);
            assert!(rig.app.pivot_builder.open && rig.app.editing.is_some(), "{what}: Esc in a text box did not cancel the builder");
            place.draw(&mut rig, 3);
            assert!(rig.app.pivot_builder.open, "{what}: and the builder is still there");
            // the text box lets go of the keyboard, and the next Esc is Cancel
            assert!(!place.wants_keyboard(&rig), "{what}: Esc took the keyboard from the text box");
            place.key(&mut rig, egui::Key::Escape);
            rig.draw(4);
            assert!(!rig.app.pivot_builder.open, "{what}: the next Esc is Cancel");
            step_is(&rig, &step, &what);

            // the Filters list is open: Esc closes the list
            let (mut rig, mut place, step, what) = esc_rig(ppp, w, h, at);
            add_to(&mut place, &mut rig, "Branch", "Filters", &what);
            place.draw(&mut rig, 4);
            assert!(rig.app.pivot_builder.popup.is_some(), "{what}: the list is open");
            place.key(&mut rig, egui::Key::Escape);
            assert!(rig.app.pivot_builder.popup.is_none() && rig.app.pivot_builder.open, "{what}: Esc closed the list only");
            place.key(&mut rig, egui::Key::Escape);
            rig.draw(4);
            assert!(!rig.app.pivot_builder.open, "{what}: the next Esc is Cancel");
            step_is(&rig, &step, &what);

            // a field's drop-down menu is open: Esc closes it
            let (mut rig, mut place, step, what) = esc_rig(ppp, w, h, at);
            place.draw(&mut rig, 3);
            let menu = place.rects(&rig).field_menus.iter().find(|(n, _)| n == "Branch").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the menu of Branch"));
            let p = place.click(&mut rig, menu);
            assert!(last_box(&p, "Add to Rows", menu.top()).is_some(), "{what}: the menu is open");
            let p = place.key(&mut rig, egui::Key::Escape);
            assert!(rig.app.pivot_builder.open && rig.app.editing.is_some(), "{what}: Esc with a drop-down open did not cancel the builder");
            assert!(last_box(&p, "Add to Rows", menu.top()).is_none(), "{what}: Esc closed the drop-down");
            place.key(&mut rig, egui::Key::Escape);
            rig.draw(4);
            assert!(!rig.app.pivot_builder.open, "{what}: the next Esc is Cancel");
            step_is(&rig, &step, &what);

            // a drop-down list is open (the aggregation of the Values chip): Esc closes it
            let (mut rig, mut place, step, what) = esc_rig(ppp, w, h, at);
            let p = place.draw(&mut rig, 3);
            let values = place.rects(&rig).chips.iter().find(|(a, _, _)| *a == crate::ui::pivot_builder::Area::Values).map(|(_, _, r)| *r).unwrap_or_else(|| panic!("{what}: a value chip"));
            let agg = p.boxes.iter().find(|(t, b)| t == "Sum" && inside(*b, values)).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the aggregation of the value: {:?}", p.boxes.iter().filter(|(_, b)| inside(*b, values)).map(|b| b.0.clone()).collect::<Vec<_>>()));
            let p = place.click(&mut rig, agg);
            assert!(p.boxes.iter().any(|(t, _)| t == "Average"), "{what}: the list of aggregations is open");
            let p = place.key(&mut rig, egui::Key::Escape);
            assert!(rig.app.pivot_builder.open && rig.app.editing.is_some(), "{what}: Esc with a drop-down list open did not cancel the builder");
            assert!(!p.boxes.iter().any(|(t, _)| t == "Average"), "{what}: Esc closed the list");
            place.key(&mut rig, egui::Key::Escape);
            rig.draw(4);
            assert!(!rig.app.pivot_builder.open, "{what}: the next Esc is Cancel");
            step_is(&rig, &step, &what);
        }
    }
}

// ------------------------------------------------------------------------------------------------------------------------------
// P2: the Nested pivot layout. A pivot of Product › Branch (3 parents, 150 leaves) is made by real clicks, switched on its tab, drawn
// with bold parents and indent, and its parent lines give their rows.
// ------------------------------------------------------------------------------------------------------------------------------

use crate::ui::table::nested_grid::{nested_grid_report, NestedGridReport};
use micro_parquet::models::{PivotAgg, PivotLayout, PivotSpec, PivotValue};

/// Runs frames until `done` holds and no job is busy (the worker finishes between frames).
fn pump(rig: &mut Rig, what: &str, done: &dyn Fn(&Rig) -> bool) {
    for _ in 0..600 {
        rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 1.0);
        rig.draw(1);
        if done(rig) && !rig.app.jobs.any_busy() {
            rig.draw(2);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("{}: '{what}' timed out (status: {})", rig.what, rig.app.status_message);
}

/// Two real clicks at the same instant: a double-click.
fn dbl(rig: &mut Rig, pos: egui::Pos2) {
    rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 1.0);
    rig.click(pos);
    rig.click(pos);
    rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 1.0);
}

fn nested_spec(layout: PivotLayout) -> PivotSpec {
    PivotSpec { rows: vec!["Product".into(), "Branch".into()], values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None }], row_totals: true, column_totals: true, layout, ..Default::default() }
}

/// A pivot tab of Product › Branch with `layout` and `later` steps after it, made the active tab and drawn.
fn nested_tab(rig: &mut Rig, layout: PivotLayout, later: Vec<TransformStep>) -> usize {
    rig.app.show_steps_sidebar = false;
    let src = rig.app.tabs[0].source.clone();
    let mut steps = vec![TransformStep::from_pivot_spec(nested_spec(layout))];
    steps.extend(later);
    rig.app.create_tab_from_source("Pivot: Product".into(), src, steps, std::collections::HashMap::new());
    settle(&mut rig.app);
    let at = rig.app.tabs.len() - 1;
    rig.app.active_tab = at;
    pump(rig, "the pivot", &|r| r.app.tabs[at].total_rows > 0);
    at
}

fn report_of(rig: &Rig, at: usize) -> Option<NestedGridReport> {
    nested_grid_report(&rig.ctx, rig.app.tabs[at].id)
}

/// The family a text was painted in.
fn family_of(p: &Painted, text: &str, inside_rect: egui::Rect) -> egui::FontFamily {
    p.texts.iter().find(|(t, pos, _)| t == text && inside_rect.contains(*pos)).map(|(_, _, f)| f.clone()).unwrap_or_else(|| panic!("'{text}' is painted in {inside_rect:?}"))
}

/// Picks `item` in the combo box whose box is `combo`, by real clicks.
fn pick_in_combo(rig: &mut Rig, combo: egui::Rect, item: &str, what: &str) -> Painted {
    let p = click_rect(rig, combo);
    let at = last_box(&p, item, combo.top()).unwrap_or_else(|| panic!("{what}: '{item}' in the list: {:?}", p.boxes.iter().filter(|(_, b)| b.top() > combo.top() - 1.0).map(|b| b.0.clone()).filter(|t| t.len() < 30).collect::<Vec<_>>()));
    click_rect(rig, at);
    rig.draw(3)
}

#[test]
fn p2_the_dialog_chooses_the_layout_and_the_blank_line_and_the_tab_draws_bold_parents_and_the_indent_light_and_dark_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        for dark in [false, true] {
            let mut rig = rig9("p9p2_dialog", ppp, w, h);
            rig.app.dark_mode = dark;
            let what = format!("{} {}", rig.what, if dark { "dark" } else { "light" });
            rig.app.run_command(&rig.ctx, CommandId::Pivot);
            rig.draw(8);
            let mut place = Place::Docked;
            add_to(&mut place, &mut rig, "Product", "Rows", &what);
            add_to(&mut place, &mut rig, "Branch", "Rows", &what);
            add_to(&mut place, &mut rig, "Balance", "Values", &what);
            place.draw(&mut rig, 4);
            // the Layout box is in the dialog, Side by side, and no blank-line option yet
            let title = format!("{}  Pivot table", theme::ph::TABLE);
            let r = assert_dialog_fits(&rig.ctx, &title, &what);
            let g = rig.app.pivot_builder.rects.clone();
            assert!(g.layout_combo.is_positive() && inside(g.layout_combo, r.window), "{what}: the Layout box {:?} is in the dialog {:?}", g.layout_combo, r.window);
            assert_eq!(rig.app.pivot_builder.spec.layout, PivotLayout::SideBySide, "{what}: the default is today's layout");
            assert!(!g.blank_line.is_positive(), "{what}: no blank-line option for Side by side");
            // Nested (Excel style), by real clicks
            pick_in_combo(&mut rig, g.layout_combo, "Nested (Excel style)", &what);
            assert_eq!(rig.app.pivot_builder.spec.layout, PivotLayout::Nested { blank_line_after_group: false }, "{what}");
            let p = rig.draw(4);
            let r = assert_dialog_fits(&rig.ctx, &title, &what);
            let g = rig.app.pivot_builder.rects.clone();
            assert!(g.blank_line.is_positive() && inside(g.blank_line, r.window), "{what}: the blank-line option {:?} is in the dialog {:?}", g.blank_line, r.window);
            // no two labels overlap in the toggles, and nothing is cut
            let texts = crate::ui::render_tests_p8::dialog_texts(&mut rig, &p, r.window);
            assert_no_overlap(&texts, &what);
            assert_not_clipped(&p, r.window, &what);
            // the option, by a real click
            click_rect(&mut rig, g.blank_line);
            rig.draw(3);
            assert_eq!(rig.app.pivot_builder.spec.layout, PivotLayout::Nested { blank_line_after_group: true }, "{what}: the blank-line option is on");
            // back to Side by side: the option goes; then Nested again with the option off, then on
            let g = rig.app.pivot_builder.rects.clone();
            pick_in_combo(&mut rig, g.layout_combo, "Side by side", &what);
            rig.draw(3);
            assert_eq!(rig.app.pivot_builder.spec.layout, PivotLayout::SideBySide, "{what}");
            assert!(!rig.app.pivot_builder.rects.blank_line.is_positive(), "{what}: the option is gone with Side by side");
            let g = rig.app.pivot_builder.rects.clone();
            pick_in_combo(&mut rig, g.layout_combo, "Nested (Excel style)", &what);
            rig.draw(3);
            let g = rig.app.pivot_builder.rects.clone();
            click_rect(&mut rig, g.blank_line);
            rig.draw(3);
            assert_eq!(rig.app.pivot_builder.spec.layout, PivotLayout::Nested { blank_line_after_group: true }, "{what}");

            // Create pivot tab: the step has the layout, the tab draws the Nested view
            let create = button(&rig, "Create pivot tab").unwrap_or_else(|| panic!("{what}: Create pivot tab"));
            click_rect(&mut rig, create);
            let at = rig.app.tabs.len() - 1;
            pump(&mut rig, "the nested view", &|r| r.app.tabs.len() == 2 && r.app.nested_views.contains_key(&r.app.tabs[1].id));
            let at = at.max(1);
            let step = serde_json::to_value(rig.app.tabs[at].steps.last().expect("a step")).expect("json");
            assert_eq!(step["PivotTable"]["layout"], serde_json::json!({"Nested": {"blank_line_after_group": true}}), "{what}: the step has the layout: {step}");
            let p = rig.draw(4);
            let rep = report_of(&rig, at).unwrap_or_else(|| panic!("{what}: the Nested grid was drawn"));
            let frame = rep.frame.expect("the frame");
            assert!(inside(frame, rig.ctx.screen_rect()), "{what}: the grid {frame:?} is on the screen");
            assert_eq!(rep.header.first().map(|h| h.0.as_str()), Some("Row Labels"), "{what}: one label column");
            assert!(!rep.header.iter().any(|h| h.0 == "Product" || h.0 == "Branch"), "{what}: the Rows fields are not columns of their own: {:?}", rep.header);
            // the first line: a parent (Classic), bold, with its subtotal on the line, bold too; its first child indented one level, not bold
            let l0 = &rep.lines[0];
            let parent = l0.label_text.expect("the parent's label");
            let fam = family_of(&p, "Classic", frame);
            assert_eq!(fam, theme::family_bold(), "{what}: the parent is bold");
            let l1 = &rep.lines[1];
            let child = l1.label_text.expect("the child's label");
            assert!((child.left() - parent.left() - theme::NESTED_INDENT_PX).abs() < 1.0, "{what}: the child {child:?} is one level ({}) right of its parent {parent:?}", theme::NESTED_INDENT_PX);
            assert_ne!(family_of(&p, "B003", frame), theme::family_bold(), "{what}: a leaf is not bold");
            let (v0, v1) = (&l0.cells[0].1, &l1.cells[0].1);  // the numbers' own rectangles
            assert!(p.texts.iter().any(|(t, pos, f)| v0.contains(*pos) && !t.is_empty() && *f == theme::family_mono_semibold()), "{what}: the parent's subtotal is bold");
            assert!(p.texts.iter().any(|(t, pos, f)| v1.contains(*pos) && !t.is_empty() && *f == egui::FontFamily::Monospace), "{what}: the leaf's number is not");
            // no two labels overlap, and nothing is cut
            let area = |a: egui::Rect| p.glyphs.iter().filter(|(_, b)| inside(*b, a)).cloned().collect::<Vec<_>>();
            assert_no_overlap(&area(frame), &what);
            assert_not_clipped(&p, frame, &what);
            // the end of the table: a blank line, then the Grand total, bold, at level 0
            rig.ctx.data_mut(|d| d.insert_temp(egui::Id::new(("grid_scroll_y", rig.app.tabs[at].id)), 1.0e6f32));
            rig.draw(4);
            let p = rig.draw(2);
            let rep = report_of(&rig, at).expect("the grid");
            let last = rep.lines.last().expect("lines");
            assert_eq!(last.index, 3 + 150 + 3, "{what}: 3 parents, 150 children, 3 blank lines and the Grand total: the last line is index 156");
            let gt = last.label_text.expect("the Grand total's label");
            assert_eq!(family_of(&p, "Grand total", frame), theme::family_bold(), "{what}: the Grand total is bold");
            assert!((gt.left() - parent.left()).abs() < 1.0, "{what}: the Grand total {gt:?} is at level 0, like the parent {parent:?}");
            let before = &rep.lines[rep.lines.len() - 2];
            assert!(before.label_text.is_none(), "{what}: the line before the Grand total is the blank line after the last group");
        }
    }
}

fn tab_layout(rig: &Rig, at: usize) -> PivotLayout {
    rig.app.tabs[at].steps.last().and_then(|s| s.as_pivot_spec()).expect("a pivot step").layout
}

fn view_lines(rig: &Rig, at: usize) -> Option<usize> {
    match rig.app.nested_views.get(&rig.app.tabs[at].id).map(|c| &c.state) {
        Some(crate::ui::pivot_layout::NestedState::Ready(v)) => Some(v.rows.len()),
        _ => None,
    }
}

#[test]
fn p2_a_pivot_tab_switches_its_layout_from_the_bar_without_rebuilding_the_flat_table_and_undo_takes_it_back_light_and_dark_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        for dark in [false, true] {
            let mut rig = rig9("p9p2_bar", ppp, w, h);
            rig.app.dark_mode = dark;
            let what = format!("{} {}", rig.what, if dark { "dark" } else { "light" });
            let at = nested_tab(&mut rig, PivotLayout::SideBySide, vec![]);
            let p = rig.draw(4);
            // an old pivot is Side by side: the flat grid, the bar says so and offers the layout
            assert!(report_of(&rig, at).is_none() && crate::ui::table::grid_report(&rig.ctx, rig.app.tabs[at].id).is_some(), "{what}: Side by side draws the flat grid");
            let bar = rig.app.pivot_bar.rects.clone();
            assert!(bar.layout.is_positive() && inside(bar.layout, bar.bar), "{what}: the Layout box {:?} is in the bar {:?}", bar.layout, bar.bar);
            assert!(!bar.blank_line.is_positive(), "{what}: no blank-line option for Side by side");
            assert!(p.boxes.iter().any(|(t, b)| t == "Side by side" && inside(*b, bar.bar)), "{what}: it says Side by side");
            // a marker no refresh would leave: the switch does not rebuild the flat table
            rig.app.tabs[at].total_rows = 777_777;
            pick_in_combo(&mut rig, bar.layout, "Nested (Excel style)", &what);
            pump(&mut rig, "the nested view", &|r| r.app.nested_views.contains_key(&r.app.tabs[at].id));
            assert_eq!(tab_layout(&rig, at), PivotLayout::Nested { blank_line_after_group: false }, "{what}: the step has the layout");
            assert_eq!(rig.app.tabs[at].total_rows, 777_777, "{what}: the flat table was not rebuilt");
            assert_eq!(view_lines(&rig, at), Some(3 + 150 + 1), "{what}: 3 parents, 150 children and the Grand total");
            let rep = report_of(&rig, at).unwrap_or_else(|| panic!("{what}: the Nested grid is drawn"));
            assert_eq!(rep.header.first().map(|h| h.0.as_str()), Some("Row Labels"), "{what}");
            let bar = rig.app.pivot_bar.rects.clone();
            assert!(bar.blank_line.is_positive() && inside(bar.blank_line, bar.bar), "{what}: the blank-line option {:?} is in the bar {:?}", bar.blank_line, bar.bar);
            // the blank-line option, by a real click: 3 more lines, still no rebuild
            click_rect(&mut rig, bar.blank_line);
            pump(&mut rig, "the view with blank lines", &|r| view_lines(r, at) == Some(3 + 150 + 3 + 1));
            assert_eq!(tab_layout(&rig, at), PivotLayout::Nested { blank_line_after_group: true }, "{what}");
            assert_eq!(rig.app.tabs[at].total_rows, 777_777, "{what}: still not rebuilt");
            // nothing overlaps or is cut in the bar and the grid
            let p = rig.draw(3);
            let bar = rig.app.pivot_bar.rects.clone();
            let area = |a: egui::Rect| p.glyphs.iter().filter(|(_, b)| inside(*b, a)).cloned().collect::<Vec<_>>();
            assert_no_overlap(&area(bar.bar), &what);
            assert_not_clipped(&p, bar.bar, &what);
            let frame = report_of(&rig, at).and_then(|r| r.frame).expect("the frame");
            assert_no_overlap(&area(frame), &what);
            assert_not_clipped(&p, frame, &what);
            // back to Side by side by the box
            let bar = rig.app.pivot_bar.rects.clone();
            pick_in_combo(&mut rig, bar.layout, "Side by side", &what);
            pump(&mut rig, "the flat grid", &|r| !r.app.nested_views.contains_key(&r.app.tabs[at].id));
            assert_eq!(tab_layout(&rig, at), PivotLayout::SideBySide, "{what}");
            assert!(report_of(&rig, at).is_none() && !rig.app.pivot_bar.rects.blank_line.is_positive(), "{what}: the flat grid again, and no blank-line option");
            assert_eq!(rig.app.tabs[at].total_rows, 777_777, "{what}: never rebuilt by the three switches");
            // undo takes the switches back one at a time (an undo rebuilds the tab, as every undo does)
            rig.app.undo_steps();
            pump(&mut rig, "undo 1", &|r| tab_layout(r, at) == PivotLayout::Nested { blank_line_after_group: true } && view_lines(r, at).is_some());
            rig.app.undo_steps();
            pump(&mut rig, "undo 2", &|r| tab_layout(r, at) == PivotLayout::Nested { blank_line_after_group: false } && view_lines(r, at) == Some(154));
            rig.app.undo_steps();
            pump(&mut rig, "undo 3", &|r| tab_layout(r, at) == PivotLayout::SideBySide && !r.app.nested_views.contains_key(&r.app.tabs[at].id));
            assert!(report_of(&rig, at).is_none(), "{what}: Side by side after the third undo");
        }
    }
}

/// The value of a numeric cell of the first line of the view's first value column.
fn view_value(rig: &Rig, at: usize, line: usize) -> f64 {
    match rig.app.nested_views.get(&rig.app.tabs[at].id).map(|c| &c.state) {
        Some(crate::ui::pivot_layout::NestedState::Ready(v)) => v.values.get_columns()[0].get(line).expect("a cell").try_extract::<f64>().expect("a number"),
        _ => panic!("a Nested view"),
    }
}

fn rows_sum(rig: &Rig, at: usize) -> (usize, f64) {
    let t = &rig.app.tabs[at];
    let df = micro_parquet::engine::compile_tab_lazy(&t.source, &t.steps, &rig.app.paths.temp_dir).expect("compiles").collect().expect("collects");
    (df.height(), df.column("Balance").expect("Balance").f64().expect("f").into_iter().flatten().sum())
}

#[test]
fn p2_show_rows_on_a_parent_line_gives_every_row_of_its_group_a_leaf_and_the_grand_total_work_as_before_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig9("p9p2_rows", ppp, w, h);
        let what = rig.what.clone();
        let at = nested_tab(&mut rig, PivotLayout::Nested { blank_line_after_group: false }, vec![]);
        pump(&mut rig, "the nested view", &|r| r.app.nested_views.contains_key(&r.app.tabs[at].id));
        rig.draw(3);
        let n = rig.app.tabs.len();
        let rep = report_of(&rig, at).unwrap_or_else(|| panic!("{what}: the Nested grid"));
        // Classic's parent line: its 50 branches and their sum, which is the subtotal on the line
        let subtotal = view_value(&rig, at, 0);
        let cell = rep.lines[0].cells[0].1;
        assert!(inside(cell, rig.ctx.screen_rect()), "{what}: the cell {cell:?}");
        dbl(&mut rig, cell.center());
        pump(&mut rig, "the rows of Classic", &|r| r.app.tabs.len() == n + 1);
        assert_eq!(rig.app.tabs[n].title, "Rows: Product = Classic", "{what}");
        let (rows, sum) = rows_sum(&rig, n);
        assert_eq!(rows, 50, "{what}: every row of the group");
        assert_eq!(sum, subtotal, "{what}: their sum is the subtotal on the line");
        assert_eq!(sum, 87_750.0, "{what}");
        // a leaf gives its own rows, as before
        rig.app.active_tab = at;
        rig.draw(4);
        let rep = report_of(&rig, at).expect("the grid");
        let cell = rep.lines[1].cells[0].1;
        dbl(&mut rig, cell.center());
        pump(&mut rig, "the rows of a leaf", &|r| r.app.tabs.len() == n + 2);
        assert_eq!(rig.app.tabs[n + 1].title, "Rows: Product = Classic, Branch = B003", "{what}");
        assert_eq!(rows_sum(&rig, n + 1).0, 1, "{what}");
        // the Grand total by the cell's menu: every row
        rig.app.active_tab = at;
        rig.draw(3);
        rig.ctx.data_mut(|d| d.insert_temp(egui::Id::new(("grid_scroll_y", rig.app.tabs[at].id)), 1.0e6f32));
        rig.draw(5);
        let rep = report_of(&rig, at).expect("the grid");
        let last = rep.lines.last().expect("lines");
        assert!(last.label_text.is_some(), "{what}: the Grand total is on the screen");
        let cell = last.cells[0].1;
        let pos = cell.center();
        rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 1.0);
        rig.step(vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed: true, modifiers: egui::Modifiers::NONE }]);
        rig.step(vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed: false, modifiers: egui::Modifiers::NONE }]);
        let p = rig.draw(3);
        let item = p.boxes.iter().rev().find(|(t, _)| t.trim() == "Show rows").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: Show rows in the cell's menu"));
        assert!(inside(item, rig.ctx.screen_rect()), "{what}");
        click_rect(&mut rig, item);
        pump(&mut rig, "the rows of the Grand total", &|r| r.app.tabs.len() == n + 3);
        assert_eq!(rig.app.tabs[n + 2].title, "Rows: all rows of the pivot", "{what}");
        assert_eq!(rows_sum(&rig, n + 2).0, 150, "{what}");
        // a double-click on a label shows no rows
        rig.app.active_tab = at;
        rig.draw(4);
        let rep = report_of(&rig, at).expect("the grid");
        let label = rep.lines[0].label_text.expect("a label");
        dbl(&mut rig, label.center());
        for _ in 0..4 {
            rig.draw(2);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(rig.app.tabs.len(), n + 3, "{what}: a label shows no rows");
    }
}

#[test]
fn p2_with_steps_after_the_pivot_the_tab_shows_the_flat_table_and_says_so_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig9("p9p2_after", ppp, w, h);
        let what = rig.what.clone();
        let sort = TransformStep::SortRows { levels: vec![micro_parquet::models::SortLevel { column: "Product".into(), descending: false, ..Default::default() }] };
        let at = nested_tab(&mut rig, PivotLayout::Nested { blank_line_after_group: false }, vec![sort]);
        for _ in 0..6 {
            rig.draw(2);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let p = rig.draw(3);
        assert!(report_of(&rig, at).is_none() && rig.app.nested_views.is_empty(), "{what}: no Nested view when a step follows the pivot");
        assert!(crate::ui::table::grid_report(&rig.ctx, rig.app.tabs[at].id).is_some(), "{what}: the flat grid is drawn");
        let note = p.boxes.iter().find(|(t, _)| t == "Nested layout shows when the pivot is the tab's last step").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the note is on the screen"));
        assert!(inside(note, rig.ctx.screen_rect()), "{what}: {note:?}");
        // the flat table is the pivot's own: Product, Branch and the value, then the Grand total
        let t = &rig.app.tabs[at];
        assert_eq!(t.cached_slice.get_column_names().iter().map(|n| n.to_string()).collect::<Vec<_>>(), vec!["Product", "Branch", "Sum of Balance"], "{what}");
        // taking the step off puts the Nested view back
        rig.app.tabs[at].steps.pop();
        rig.app.refresh_tab_at(at, false, None);
        pump(&mut rig, "the nested view", &|r| r.app.nested_views.contains_key(&r.app.tabs[at].id));
        let p = rig.draw(3);
        assert!(report_of(&rig, at).is_some(), "{what}: the Nested grid is back");
        assert!(!p.boxes.iter().any(|(t, _)| t.contains("Nested layout shows when")), "{what}: and the note is gone");
    }
}

/// Picks `item` in the combo box `combo` of the place the builder is drawn in, by real clicks.
fn pick_in_place(place: &mut Place, rig: &mut Rig, combo: egui::Rect, item: &str, what: &str) {
    let p = place.click(rig, combo);
    let at = last_box(&p, item, combo.top()).unwrap_or_else(|| panic!("{what}: '{item}' in the list"));
    place.click(rig, at);
    place.draw(rig, 3);
}

#[test]
fn p2_the_pop_out_builder_chooses_the_layout_too_and_apply_makes_the_step_the_docked_dialog_makes() {
    for (ppp, w, h) in SIZES3 {
        // the docked dialog's step
        let mut docked = rig9("p9p2_pop_d", ppp, w, h);
        let what = docked.what.clone();
        docked.app.run_command(&docked.ctx, CommandId::Pivot);
        docked.draw(8);
        let mut pl = Place::Docked;
        for (f, a) in [("Product", "Rows"), ("Branch", "Rows"), ("Balance", "Values")] {
            add_to(&mut pl, &mut docked, f, a, &what);
        }
        let combo = docked.app.pivot_builder.rects.layout_combo;
        pick_in_place(&mut pl, &mut docked, combo, "Nested (Excel style)", &what);
        let blank = docked.app.pivot_builder.rects.blank_line;
        click_rect(&mut docked, blank);
        docked.draw(3);
        let create = button(&docked, "Create pivot tab").expect("Create pivot tab");
        click_rect(&mut docked, create);
        docked.draw(6);
        let want = last_step(&docked);
        assert_eq!(want["PivotTable"]["layout"], serde_json::json!({"Nested": {"blank_line_after_group": true}}), "{what}: {want}");

        for (pw, ph) in POPS {
            let what = format!("{what}, window {pw}x{ph}");
            let mut rig = rig9("p9p2_pop_w", ppp, w, h);
            rig.app.run_command(&rig.ctx, CommandId::Pivot);
            rig.draw(8);
            let mut place = popped(&mut rig, pw, ph, &what);
            for (f, a) in [("Product", "Rows"), ("Branch", "Rows"), ("Balance", "Values")] {
                add_to(&mut place, &mut rig, f, a, &what);
            }
            place.draw(&mut rig, 4);
            let screen2 = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(pw, ph));
            let g = rig.app.pivot_builder.rects.clone();
            assert!(g.layout_combo.is_positive() && inside(g.layout_combo, screen2), "{what}: the Layout box {:?} is in the window", g.layout_combo);
            pick_in_place(&mut place, &mut rig, g.layout_combo, "Nested (Excel style)", &what);
            let g = rig.app.pivot_builder.rects.clone();
            assert!(g.blank_line.is_positive() && inside(g.blank_line, screen2), "{what}: the blank-line option {:?} is in the window", g.blank_line);
            place.click(&mut rig, g.blank_line);
            place.draw(&mut rig, 3);
            assert_eq!(rig.app.pivot_builder.spec.layout, PivotLayout::Nested { blank_line_after_group: true }, "{what}");
            // the header: the layout controls, the tab's name and Dock, none over another
            let p = place.draw(&mut rig, 3);
            let g = rig.app.pivot_builder.rects.clone();
            let dock = button(&rig, "Dock").expect("Dock");
            assert!(g.blank_line.right() <= dock.left() + 0.5, "{what}: the controls {:?} are left of Dock {dock:?}", g.blank_line);
            let header = egui::Rect::from_min_max(screen2.min, egui::pos2(screen2.right(), g.body.top()));
            let area = p.glyphs.iter().filter(|(_, b)| inside(*b, header)).cloned().collect::<Vec<_>>();
            assert_no_overlap(&area, &what);
            // docking back keeps the choice; popping out again too
            place.click(&mut rig, dock);
            rig.draw(6);
            assert_eq!(rig.app.pivot_builder.spec.layout, PivotLayout::Nested { blank_line_after_group: true }, "{what}: docked back with it");
            assert!(rig.app.pivot_builder.rects.blank_line.is_positive(), "{what}: and the docked dialog shows the option");
            pop_out(&mut rig, &what);
            rig.draw(3);
            let mut place = Place::Window(Pop::new(pw, ph));
            place.draw(&mut rig, 6);
            let create = button(&rig, "Create pivot tab").unwrap_or_else(|| panic!("{what}: Create pivot tab in the window"));
            place.click(&mut rig, create);
            rig.draw(6);
            assert_eq!(last_step(&rig), want, "{what}: the step the docked dialog makes, layout included");
        }
    }
}

/// Draws until `done`, then a few more frames (the worker's jobs finished).
fn wait_in(place: &mut Place, rig: &mut Rig, what: &str, done: &dyn Fn(&Rig) -> bool) -> Painted {
    for _ in 0..600 {
        rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 1.0);
        place.draw(rig, 1);
        if done(rig) && !rig.app.jobs.any_busy() {
            return place.draw(rig, 3);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("{}: '{what}' timed out", rig.what);
}

fn is_nested_preview(r: &Rig, blank: bool) -> bool {
    let b = &r.app.pivot_builder;
    b.preview_nested.as_ref().map(|v| v.blank_line_after_group == blank).unwrap_or(false) && b.spec.layout == PivotLayout::Nested { blank_line_after_group: blank }
}

/// The tab's Nested view, when ready.
fn tab_view(rig: &Rig, at: usize) -> micro_parquet::pivot_view::NestedView {
    match rig.app.nested_views.get(&rig.app.tabs[at].id).map(|c| &c.state) {
        Some(crate::ui::pivot_layout::NestedState::Ready(v)) => v.clone(),
        _ => panic!("the tab's Nested view"),
    }
}

#[test]
fn p2_the_dialogs_preview_shows_the_nested_view_the_tab_shows_docked_and_in_the_window_at_three_sizes() {
    use micro_parquet::pivot_view::NestedView;
    // the lines as the grid draws them: label, level, bold flag, the values
    fn lines(v: &NestedView) -> (Vec<(String, usize, bool, bool)>, Vec<Vec<String>>) {
        let l = v.rows.iter().map(|r| (r.label.clone(), r.level, r.is_bold(), r.is_blank())).collect();
        let vals = (0..v.rows.len()).map(|i| v.values.get_columns().iter().map(|c| format!("{:?}", c.get(i).expect("a cell"))).collect()).collect();
        (l, vals)
    }
    let places: [Option<(f32, f32)>; 3] = [None, Some(POPS[0]), Some(POPS[1])];
    for (ppp, w, h) in SIZES3 {
        for dark in [false, true] {
            for pop in places {
                let mut rig = rig9("p9p2_prev", ppp, w, h);
                rig.app.dark_mode = dark;
                let what = format!("{} {} {}", rig.what, if dark { "dark" } else { "light" }, pop.map(|p| format!("window {}x{}", p.0, p.1)).unwrap_or("docked".into()));
                rig.app.run_command(&rig.ctx, CommandId::Pivot);
                rig.draw(8);
                let mut place = match pop {
                    None => Place::Docked,
                    Some((pw, ph)) => popped(&mut rig, pw, ph, &what),
                };
                for (f, a) in [("Product", "Rows"), ("Branch", "Rows"), ("Balance", "Values")] {
                    add_to(&mut place, &mut rig, f, a, &what);
                }
                wait_in(&mut place, &mut rig, "the flat preview", &|r| matches!(r.app.pivot_builder.preview, Some(Ok(_))));
                // Side by side previews the flat table
                assert!(rig.app.pivot_builder.preview_nested.is_none(), "{what}: Side by side has no Nested preview");
                let flat_head: Vec<String> = match &rig.app.pivot_builder.preview {
                    Some(Ok(df)) => df.get_column_names().iter().map(|n| n.to_string()).collect(),
                    _ => unreachable!(),
                };
                assert_eq!(flat_head, ["Product", "Branch", "Sum of Balance"], "{what}: the flat table's columns");
                // Nested, by a real click: the preview becomes the Nested view, the rest is as it was
                let spec_before = rig.app.pivot_builder.spec.clone();
                let g = place.rects(&rig);
                pick_in_place(&mut place, &mut rig, g.layout_combo, "Nested (Excel style)", &what);
                let p = wait_in(&mut place, &mut rig, "the Nested preview", &|r| is_nested_preview(r, false));
                let mut after = rig.app.pivot_builder.spec.clone();
                after.layout = spec_before.layout;
                assert_eq!(format!("{after:?}"), format!("{spec_before:?}"), "{what}: only the layout changed");
                let g = place.rects(&rig);
                // drawn: one "Row Labels" column, no column of its own for the Rows fields, the first parent bold, its child one level in
                let pv = rig.app.pivot_builder.preview_nested.clone().expect("the preview's view");
                let area = g.preview_scroll;
                let in_area = |t: &str| p.texts.iter().find(|(x, pos, _)| x == t && area.contains(*pos)).cloned();
                assert!(in_area("Row Labels").is_some(), "{what}: the preview has a Row Labels column");
                assert!(in_area("Product").is_none() && in_area("Branch").is_none(), "{what}: no column of its own for a Rows field");
                let (parent, child) = (pv.rows[0].label.clone(), pv.rows[1].label.clone());
                let pt = in_area(&parent).unwrap_or_else(|| panic!("{what}: '{parent}' drawn"));
                assert_eq!(pt.2, theme::family_bold(), "{what}: the parent is bold, as on the tab");
                // (the docked dialog at the smaller sizes has room for one line under the heading: a child is checked where one is on show)
                match in_area(&child) {
                    Some(ct) => {
                        assert_ne!(ct.2, theme::family_bold(), "{what}: a leaf is not bold");
                        assert!((ct.1.x - pt.1.x - theme::NESTED_INDENT_PX).abs() < 1.0, "{what}: the child {:?} is one level right of its parent {:?}", ct.1, pt.1);
                    }
                    None => assert!(pop.is_none(), "{what}: '{child}' drawn in the window"),
                }
                let rows_here = if pop.is_none() { crate::ui::render_tests_p8::dialog_texts(&mut rig, &p, area) } else { p.glyphs.iter().filter(|(_, b)| inside(*b, area)).cloned().collect::<Vec<_>>() };
                assert_no_overlap(&rows_here, &what);
                // the blank-line option, by a real click: it changes the preview and nothing else
                let g = place.rects(&rig);
                assert!(g.blank_line.is_positive(), "{what}: the option");
                place.click(&mut rig, g.blank_line);
                wait_in(&mut place, &mut rig, "the blank lines", &|r| is_nested_preview(r, true));
                // Side by side again, then Nested with the option kept
                let g = place.rects(&rig);
                pick_in_place(&mut place, &mut rig, g.layout_combo, "Side by side", &what);
                wait_in(&mut place, &mut rig, "the flat preview again", &|r| r.app.pivot_builder.preview_nested.is_none() && matches!(r.app.pivot_builder.preview, Some(Ok(_))) && r.app.pivot_builder.spec.layout == PivotLayout::SideBySide);
                let g = place.rects(&rig);
                pick_in_place(&mut place, &mut rig, g.layout_combo, "Nested (Excel style)", &what);
                let g = place.rects(&rig);
                place.click(&mut rig, g.blank_line);
                wait_in(&mut place, &mut rig, "the Nested preview again", &|r| is_nested_preview(r, true));
                let preview = rig.app.pivot_builder.preview_nested.clone().expect("the preview's view");
                assert!(preview.rows.iter().any(|r| r.is_blank()) || preview.rows.len() < 52, "{what}: the blank line after the first group shows once 50 children are in view");
                // Apply: the tab's Nested view has the same lines
                let create = button(&rig, "Create pivot tab").unwrap_or_else(|| panic!("{what}: Create pivot tab"));
                place.click(&mut rig, create);
                rig.draw(4);
                pump(&mut rig, "the tab's Nested view", &|r| r.app.tabs.len() == 2 && r.app.nested_views.contains_key(&r.app.tabs[1].id) && view_lines(r, 1).is_some());
                let tab = tab_view(&rig, 1);
                let n = preview.rows.len();
                assert!(n >= 20 && n <= tab.rows.len(), "{what}: the preview has {n} of the tab's {} lines", tab.rows.len());
                let (pl, pvv) = lines(&preview);
                let (tl, tvv) = lines(&tab.head(n));
                assert_eq!(pl, tl, "{what}: labels, levels, bold flags and blank lines are the tab's");
                assert_eq!(pvv, tvv, "{what}: the values are the tab's");
                assert_eq!(preview.rows.iter().map(|r| (r.kind, r.keys.clone())).collect::<Vec<_>>(), tab.rows.iter().take(n).map(|r| (r.kind, r.keys.clone())).collect::<Vec<_>>(), "{what}: kinds and keys");
                if pop == Some(POPS[1]) {
                    assert!(n > 20, "{what}: the big window previews more than the docked 20 lines: {n}");
                }
                if pop.is_none() {
                    assert_eq!(n, 20, "{what}: docked: 20 lines");
                }
            }
        }
    }
}

/// 16 loan lines of the owner's shape: Bank (Conventional, Islami) and CL status (some blank).
fn book_cl() -> DataFrame {
    let rows: [(&str, Option<&str>, &str, f64, f64); 16] = [
        ("Conventional", Some("HC"), "A1", 100.0, 9.0),
        ("Conventional", Some("HC"), "A2", 200.0, 10.0),
        ("Conventional", Some("LC"), "A1", 50.0, 12.0),
        ("Conventional", Some("LC"), "A3", 150.0, 11.0),
        ("Conventional", Some("NC"), "A2", 80.0, 14.0),
        ("Conventional", None, "A4", 300.0, 8.0),
        ("Conventional", Some("HC"), "A5", 120.0, 9.5),
        ("Conventional", Some("NC"), "A12", 60.0, 15.0),
        ("Islami", Some("HC"), "A6", 400.0, 7.0),
        ("Islami", Some("HC"), "A7", 100.0, 8.0),
        ("Islami", Some("LC"), "A6", 60.0, 10.0),
        ("Islami", Some("NC"), "A8", 90.0, 13.0),
        ("Islami", None, "A9", 40.0, 6.0),
        ("Islami", None, "A10", 70.0, 6.5),
        ("Islami", Some("HC"), "A11", 30.0, 7.5),
        ("Islami", Some("HC"), "A11", 25.0, 7.0),
    ];
    DataFrame::new(vec![
        Series::new("Bank".into(), rows.iter().map(|r| r.0).collect::<Vec<_>>()).into(),
        Series::new("CL status".into(), rows.iter().map(|r| r.1).collect::<Vec<_>>()).into(),
        Series::new("AC Number".into(), rows.iter().map(|r| r.2).collect::<Vec<_>>()).into(),
        Series::new("Balance".into(), rows.iter().map(|r| r.3 * 10_000_000.0).collect::<Vec<_>>()).into(),
        Series::new("Rate".into(), rows.iter().map(|r| r.4).collect::<Vec<_>>()).into(),
    ])
    .expect("book_cl")
}

#[test]
fn p2_pictures_of_the_layout_in_the_dialog_and_of_the_nested_tab() {
    use crate::ui::render_tests_p8::{can_shoot, shoot, styled, SHOT_H, SHOT_PPP, SHOT_W};
    if !can_shoot() {
        return;
    }
    for dark in [false, true] {
        let mut rig = rig8("p9p2shot", SHOT_PPP, SHOT_W, SHOT_H, book_cl());
        rig.ctx.set_embed_viewports(false);
        styled(&mut rig, dark);
        let what = format!("{} {}", rig.what, if dark { "dark" } else { "light" });
        rig.app.run_command(&rig.ctx, CommandId::Pivot);
        rig.draw(8);
        let mut place = Place::Docked;
        for (f, a) in [("Bank", "Rows"), ("CL status", "Rows"), ("Balance", "Values")] {
            add_to(&mut place, &mut rig, f, a, &what);
        }
        let combo = rig.app.pivot_builder.rects.layout_combo;
        pick_in_place(&mut place, &mut rig, combo, "Nested (Excel style)", &what);
        let blank = rig.app.pivot_builder.rects.blank_line;
        click_rect(&mut rig, blank);
        rig.draw(6);
        let title = format!("{}  Pivot table", theme::ph::TABLE);
        let r = assert_dialog_fits(&rig.ctx, &title, &what);
        shoot(&mut rig, "pivot-layout-dialog", dark, r.window.expand(10.0));
        // the tab
        let create = button(&rig, "Create pivot tab").expect("Create pivot tab");
        click_rect(&mut rig, create);
        pump(&mut rig, "the nested view", &|r| r.app.tabs.len() == 2 && r.app.nested_views.contains_key(&r.app.tabs[1].id));
        rig.app.show_steps_sidebar = false;
        rig.draw(6);
        let frame = report_of(&rig, 1).and_then(|r| r.frame).expect("the Nested grid");
        let bar = rig.app.pivot_bar.rects.bar;
        shoot(&mut rig, "pivot-nested-tab", dark, bar.union(frame).expand(10.0));
    }
}
