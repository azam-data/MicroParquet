//! Phase 14 render tests: the assistant panel in every view, at three window sizes, by real clicks.

use crate::ui::prelude::*;
use crate::ui::render_tests_p4::Rig;
use crate::ui::render_tests_p7::SIZES3;
use crate::ui::render_tests_ui::{rail_rect, rig_ui};
use micro_parquet::assistant::AnswerKind;
use micro_parquet::workspace::View;

fn rect_of(rig: &Rig, name: &str) -> egui::Rect {
    rig.app.assistant.rects.iter().find(|(n, _)| n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: '{name}' in {:?}", rig.what, rig.app.assistant.rects.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()))
}

fn ask_rail(rig: &Rig) -> egui::Rect {
    rig.app.assistant.rail_rect.unwrap_or_else(|| panic!("{}: the rail has an Ask button", rig.what))
}

fn key(rig: &mut Rig, k: egui::Key) {
    let ev = |pressed| egui::Event::Key { key: k, physical_key: Some(k), pressed, repeat: false, modifiers: egui::Modifiers::NONE };
    rig.step(vec![ev(true)]);
    rig.step(vec![ev(false)]);
    rig.draw(3);
}

fn ask(rig: &mut Rig, q: &str) {
    rig.app.assistant.input = q.to_string();
    rig.draw(2); // the button is enabled once there is a question
    let r = rect_of(rig, "Ask");
    rig.click_later(r.center());
    rig.draw(3);
}

#[test]
fn ui14_a_the_rail_button_opens_the_panel_in_every_view_and_nothing_overlaps_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig_ui("p14_a", ppp, w, h);
        let what = rig.what.clone();
        assert!(!rig.app.assistant.open, "{what}: closed at the start");
        for v in [View::Report, View::Dashboard, View::Data, View::Model] {
            let r = rail_rect(&rig, v);
            rig.click_later(r.center());
            rig.draw(4);
            // the Ask button is in the rail, below the four view buttons
            let ask_btn = ask_rail(&rig);
            assert!(ask_btn.left() >= -0.5 && ask_btn.right() <= theme::RAIL_W + 0.5, "{what} {v:?}: Ask {ask_btn:?} is in the rail's column");
            for (_, vr) in &rig.app.ws.rail_rects {
                assert!(!vr.intersects(ask_btn), "{what} {v:?}: Ask {ask_btn:?} does not overlap a view button {vr:?}");
            }
            rig.click_later(ask_btn.center());
            rig.draw(4);
            assert!(rig.app.assistant.open, "{what} {v:?}: a click on Ask opens the assistant");
            let screen = rig.ctx.screen_rect();
            for n in ["Close", "Input", "Ask", "Setup header"] {
                let r = rect_of(&rig, n);
                assert!(r.left() >= theme::RAIL_W - 0.5 && r.right() <= screen.right() + 0.5 && r.bottom() <= screen.bottom() + 0.5, "{what} {v:?}: '{n}' {r:?} is inside the window and right of the rail");
            }
            // the input and the Ask button are side by side, not overlapping
            assert!(!rect_of(&rig, "Input").intersects(rect_of(&rig, "Ask")), "{what} {v:?}: input and Ask do not overlap");
            // the panel took room from the page: the central area ends where the panel begins
            let central = theme::central_area(&rig.ctx);
            assert!(central.right() <= rect_of(&rig, "Input").left() + 0.5, "{what} {v:?}: the page {central:?} stops before the panel");
            // F1 closes it, F1 opens it
            key(&mut rig, egui::Key::F1);
            assert!(!rig.app.assistant.open, "{what} {v:?}: F1 closes");
            key(&mut rig, egui::Key::F1);
            assert!(rig.app.assistant.open, "{what} {v:?}: F1 opens");
            let c = rect_of(&rig, "Close");
            rig.click_later(c.center());
            rig.draw(3);
            assert!(!rig.app.assistant.open, "{what} {v:?}: the cross closes");
            assert!(!rig.app.settings.assistant_open, "{what}: and the choice is kept");
        }
    }
}

#[test]
fn ui14_b_an_example_is_answered_from_the_guide_and_its_section_can_be_read() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig_ui("p14_b", ppp, w, h);
        let what = rig.what.clone();
        rig.app.set_assistant_open(true);
        rig.draw(4);
        assert!(rig.app.assistant.messages.is_empty());
        let ex = rect_of(&rig, "Example 1");
        rig.click_later(ex.center());
        rig.draw(4);
        assert_eq!(rig.app.assistant.messages.len(), 1, "{what}");
        let m = &rig.app.assistant.messages[0];
        assert_eq!(m.answer.kind, AnswerKind::Guide, "{what}");
        assert!(!m.waiting, "{what}: no model, nothing to wait for");
        assert!(!m.answer.sources.is_empty(), "{what}");
        // the first source opens in the panel
        let s1 = rect_of(&rig, "Source 1");
        let p = rig.click_later(s1.center());
        assert!(rig.app.assistant.reading.is_some(), "{what}: a click on a source opens it");
        let body_first_word = micro_parquet::assistant::embedded().section(rig.app.assistant.reading.expect("reading").section).expect("s").body.split_whitespace().next().unwrap_or("").trim_matches(|c: char| !c.is_alphanumeric()).to_string();
        let _ = (p, body_first_word);
        rig.draw(3);
        let again = rect_of(&rig, "Source 1");
        rig.click_later(again.center());
        assert!(rig.app.assistant.reading.is_none(), "{what}: a second click closes it");
    }
}

#[test]
fn ui14_c_a_request_for_a_figure_is_refused_in_the_panel_and_points_to_the_tool() {
    let (ppp, w, h) = SIZES3[0];
    let mut rig = rig_ui("p14_c", ppp, w, h);
    rig.app.set_assistant_open(true);
    rig.draw(4);
    ask(&mut rig, "calculate the NIM for 4,500 Cr at 9.5%");
    let m = rig.app.assistant.messages.last().expect("a message");
    assert_eq!(m.answer.kind, AnswerKind::NotCalculated);
    assert!(m.answer.text.starts_with("I do not calculate numbers"));
    assert!(rig.app.assistant.input.is_empty(), "the box is emptied after Ask");
    // a question the guide does not cover is said so
    ask(&mut rig, "recipe for hilsa curry");
    assert_eq!(rig.app.assistant.messages.last().expect("m").answer.kind, AnswerKind::NotCovered);
}

#[test]
fn ui14_d_without_a_model_the_setup_says_so_and_start_is_off() {
    let (ppp, w, h) = SIZES3[2];
    let mut rig = rig_ui("p14_d", ppp, w, h);
    rig.app.set_assistant_open(true);
    rig.draw(4);
    let hd = rect_of(&rig, "Setup header");
    rig.click_later(hd.center());
    rig.draw(4);
    assert!(rig.app.assistant.setup_open);
    let setup = rig.app.assistant.setup.clone().expect("checked when the panel opened");
    assert!(!setup.complete());
    assert!(setup.status_line().contains("No model installed"), "{}", setup.status_line());
    // the folder is under the data folder
    assert!(setup.dir.starts_with(&rig.app.paths.root_dir));
    // the button is drawn; a start refused in words, never a crash
    let _ = rect_of(&rig, "Start model");
    rig.app.assistant_start_model();
    assert!(matches!(rig.app.assistant.model, crate::ui::assistant::ModelState::Failed(_)));
    rig.draw(3);
    // the app and its other panels draw as before
    assert!(rig.app.status_message.is_empty() || !rig.app.status_message.contains("panicked"));
}

/// The picture of the panel (light, then dark): `docs/guide/img/assistant-panel[-dark].png`.
#[test]
fn ui14_e_the_panel_is_drawn_as_a_picture_in_both_themes() {
    use crate::ui::render_tests_p8::{can_shoot, shoot, styled, SHOT_H, SHOT_PPP, SHOT_W};
    if !can_shoot() {
        return;
    }
    for dark in [false, true] {
        let mut rig = rig_ui("p14_shot", SHOT_PPP, SHOT_W, SHOT_H);
        styled(&mut rig, dark);
        rig.app.switch_view(View::Data);
        rig.app.set_assistant_open(true);
        rig.draw(4);
        ask(&mut rig, "How do I run a liquidity gap ladder?");
        let s1 = rect_of(&rig, "Source 1");
        rig.click_later(s1.center());
        rig.draw(4);
        shoot(&mut rig, "assistant-panel", dark, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SHOT_W, SHOT_H)));
    }
}

/// The picture of the Report view for the README (light, then dark): `docs/guide/img/report-view[-dark].png`.
#[test]
fn ui14_f_the_report_view_is_drawn_as_a_picture_in_both_themes() {
    use crate::ui::render_tests_p8::{can_shoot, shoot, styled, SHOT_H, SHOT_PPP, SHOT_W};
    if !can_shoot() {
        return;
    }
    for dark in [false, true] {
        let mut rig = rig_ui("p14_report_shot", SHOT_PPP, SHOT_W, SHOT_H);
        styled(&mut rig, dark);
        rig.app.switch_view(View::Report);
        rig.draw(5);
        shoot(&mut rig, "report-view", dark, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SHOT_W, SHOT_H)));
    }
}
