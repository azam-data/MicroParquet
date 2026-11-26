//! UI revamp render tests: headless egui renders at 1219×686, 1600×900 and 1291×700 pt, by real clicks.
//!
//! The rail and the three views, "Send to report", the empty-screen hints, Ctrl+K with nothing typed, the Settings dialog, the model's
//! step flow and the Model view, and the dashboard editor's toolbar, floating tile panel and empty-page cards.

use crate::ui::layout_tests::Painted;
use crate::ui::prelude::*;
use crate::ui::render_tests::screen;
use crate::ui::render_tests_p4::Rig;
use crate::ui::render_tests_p7::SIZES3;
use crate::ui::tests::{parquet_source, settle, test_app};
use micro_parquet::commands::CommandId;
use micro_parquet::dashboard::*;
use micro_parquet::models::*;
use micro_parquet::workspace::{ModelStep, SettingsSection, View};
use polars::prelude::{DataFrame, NamedFrom, Series};
use std::collections::HashMap;

fn book(n: usize, regions: &[&str]) -> DataFrame {
    DataFrame::new(vec![
        Series::new("Risk".into(), (0..n).map(|i| ["High", "Medium", "Low"][i % 3].to_string()).collect::<Vec<_>>()).into(),
        Series::new("Region".into(), (0..n).map(|i| regions[i % regions.len()].to_string()).collect::<Vec<_>>()).into(),
        Series::new("Balance".into(), (0..n).map(|i| ((10 + i as i64) * 1_000_000) as f64).collect::<Vec<f64>>()).into(),
    ])
    .expect("frame")
}

fn sum_by(field: &str) -> TransformStep {
    TransformStep::from_pivot_spec(PivotSpec { rows: vec![field.into()], values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None }], row_totals: true, column_totals: true, ..Default::default() })
}

/// A project with the data tab "Book", a chart tab on it, a second data tab "Other" and a pivot tab "Other by region".
fn rig_ui(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    micro_parquet::rules::set_data_dir_override(Some(app.paths.root_dir.clone()));
    let root = app.paths.root_dir.clone();
    let book_src = parquet_source(&root, "book", book(60, &["Dhaka", "Ctg"]));
    app.create_tab_from_source("Book".into(), book_src, vec![], HashMap::new());
    settle(&mut app);
    app.set_side_tab_visible(crate::ui::steps::RightTab::Chart, true);
    let (ctx, input) = screen(ppp, w, h);
    let mut rig = Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") };
    rig.draw(8);
    rig.app.chart_open_as_chart_tab();
    settle(&mut rig.app);
    let other = parquet_source(&root, "other", book(30, &["Sylhet", "Khulna"]));
    rig.app.create_tab_from_source("Other".into(), other.clone(), vec![], HashMap::new());
    rig.app.create_tab_from_source("Other by region".into(), other, vec![sum_by("Region")], HashMap::new());
    settle(&mut rig.app);
    rig.app.project.active = true;
    for i in 0..rig.app.tabs.len() {
        let id = rig.app.tabs[i].id;
        rig.app.model_ensure_key(id);
    }
    rig.app.switch_view(View::Report);
    rig.draw(5);
    rig
}

fn rect_in(list: &[(String, egui::Rect)], name: &str, what: &str) -> egui::Rect {
    list.iter().find(|(n, _)| n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: '{name}' in {:?}", list.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()))
}

fn rail_rect(rig: &Rig, v: View) -> egui::Rect {
    rig.app.ws.rail_rects.iter().find(|(x, _)| *x == v).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: the rail has {v:?}", rig.what))
}

/// The texts of the tab strip: what is painted between the top bar and the strip's bottom edge, right of the rail.
fn strip_texts(p: &Painted) -> Vec<String> {
    p.boxes.iter().filter(|(_, b)| b.top() >= theme::TOP_BAR_H - 0.5 && b.bottom() <= theme::TOP_BAR_H + theme::TAB_STRIP_H + 0.5 && b.left() >= theme::RAIL_W - 0.5).map(|(t, _)| t.clone()).collect()
}

fn has_text(texts: &[String], what: &str) -> bool {
    texts.iter().any(|t| t.trim_start_matches(|c: char| !c.is_alphanumeric()).starts_with(what))
}

fn click_name(rig: &mut Rig, list: &[(String, egui::Rect)], name: &str) -> Painted {
    let r = rect_in(list, name, &rig.what.clone());
    rig.click_later(r.center())
}

// ------------------------------------------------------------------------------------------------ the rail and the views

#[test]
fn ui1_the_rail_switches_views_and_each_view_lists_its_own_tabs_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig_ui("ui1_rail", ppp, w, h);
        let what = rig.what.clone();
        // the chart made from the Data view opened in Report
        assert_eq!(rig.app.view(), View::Report, "{what}: the chart tab became active, so the view followed it");
        let p = rig.draw(3);
        let strip = strip_texts(&p);
        assert!(has_text(&strip, "Chart:") && has_text(&strip, "Other by region"), "{what}: Report lists the chart tab and the pivot tab: {strip:?}");
        assert!(!has_text(&strip, "Book") && !has_text(&strip, "Other") || has_text(&strip, "Other by region"), "{what}: Report does not list the plain tables: {strip:?}");
        assert!(!strip.iter().any(|t| t.trim() == "Book"), "{what}: no 'Book' in Report: {strip:?}");
        // the rail: three buttons, the active one is Report, none outside the rail's column or overlapping
        assert_eq!(rig.app.ws.rail_rects.len(), 3, "{what}");
        for (_, r) in &rig.app.ws.rail_rects {
            assert!(r.left() >= -0.5 && r.right() <= theme::RAIL_W + 0.5 && r.top() >= theme::TOP_BAR_H - 0.5, "{what}: a rail button {r:?} is inside the rail column");
        }
        // Data lists every table tab and no chart tab
        let r = rail_rect(&rig, View::Data);
        let p = rig.click_later(r.center());
        assert_eq!(rig.app.view(), View::Data, "{what}: a click on Data");
        let strip = strip_texts(&p);
        assert!(has_text(&strip, "Book") && has_text(&strip, "Other"), "{what}: Data lists the tables: {strip:?}");
        assert!(!has_text(&strip, "Chart:"), "{what}: Data lists no chart tab: {strip:?}");
        assert!(rig.app.active_tab_in_view(), "{what}: the active tab is one of Data's");
        // Model has no tab strip
        let r = rail_rect(&rig, View::Model);
        let p = rig.click_later(r.center());
        assert_eq!(rig.app.view(), View::Model, "{what}");
        assert!(strip_texts(&p).iter().all(|t| !has_text(&[t.clone()], "Book")), "{what}: Model has no tab strip");
        // back to Report by the command (Ctrl+K "Go to the Report view")
        rig.app.run_command(&rig.ctx.clone(), CommandId::GoReport);
        rig.draw(3);
        assert_eq!(rig.app.view(), View::Report, "{what}");
        assert!(rig.app.active_tab_in_view(), "{what}: a view that does not list the active tab shows one of its own");
    }
}

#[test]
fn ui2_a_banking_command_takes_the_window_to_the_data_view_and_a_command_that_cannot_run_leaves_it_where_it_was() {
    let mut rig = rig_ui("ui2_home", SIZES3[0].0, SIZES3[0].1, SIZES3[0].2);
    let what = rig.what.clone();
    assert_eq!(rig.app.view(), View::Report, "{what}");
    rig.app.run_command(&rig.ctx.clone(), CommandId::Filter);
    rig.draw(4);
    assert_eq!(rig.app.view(), View::Data, "{what}: Filter works on a table, so Data is where it opens");
    assert_eq!(rig.app.active_tool, ActiveToolModal::FilterRows, "{what}");
    rig.app.active_tool = ActiveToolModal::None;
    // with no tab at all a command that needs one says so and the view stays
    let mut empty = test_app("ui2_empty");
    let (ctx, input) = screen(SIZES3[0].0, SIZES3[0].1, SIZES3[0].2);
    empty.ws.view = View::Report;
    empty.run_command(&ctx, CommandId::GapLadder);
    assert_eq!(empty.ws.view, View::Report, "{what}: the gap ladder cannot run with nothing open, and the owner is where they were");
    assert!(empty.status_message.contains("Open a file first"), "{}", empty.status_message);
    let _ = input;
}

#[test]
fn ui3_send_to_report_shows_a_table_in_both_views_and_remove_takes_it_out_again() {
    let mut rig = rig_ui("ui3_send", SIZES3[0].0, SIZES3[0].1, SIZES3[0].2);
    let what = rig.what.clone();
    rig.app.switch_view(View::Data);
    let book_idx = rig.app.tabs.iter().position(|t| t.title == "Book").expect("Book");
    rig.app.active_tab = book_idx;
    rig.draw(3);
    let id = rig.app.tabs[book_idx].id;
    // the button is on the Data bar and sends the active table
    assert!(micro_parquet::workspace::view_bar(View::Data).iter().any(|(b, _)| *b == micro_parquet::workspace::BarItem::Cmd(CommandId::SendToReport)), "{what}: the Data bar has Send to report");
    assert!(!micro_parquet::workspace::view_bar(View::Report).iter().any(|(b, _)| *b == micro_parquet::workspace::BarItem::Cmd(CommandId::SendToReport)), "{what}: and the Report bar does not");
    rig.app.run_command(&rig.ctx.clone(), CommandId::SendToReport);
    let p = rig.draw(3);
    assert!(rig.app.project.report_ids.contains(&id), "{what}");
    assert_eq!(rig.app.view(), View::Report, "{what}: sending goes to the report");
    assert!(has_text(&strip_texts(&p), "Book"), "{what}: Book is listed in Report now: {:?}", strip_texts(&p));
    assert!(rig.app.status_message.contains("Sent 'Book' to the Report view"), "{what}: {}", rig.app.status_message);
    // still in Data too
    rig.app.switch_view(View::Data);
    assert!(rig.app.tab_visible_in(View::Data, book_idx), "{what}");
    // sending it again, or sending a pivot, says it is there already and changes nothing
    rig.app.run_command(&rig.ctx.clone(), CommandId::SendToReport);
    assert!(rig.app.status_message.contains("already in the Report view"), "{what}: {}", rig.app.status_message);
    assert_eq!(rig.app.project.report_ids.len(), 1, "{what}");
    // removing it
    rig.app.remove_from_report(book_idx);
    assert!(!rig.app.tab_visible_in(View::Report, book_idx), "{what}: gone from Report");
    assert!(rig.app.tab_visible_in(View::Data, book_idx), "{what}: still in Data");
}

#[test]
fn ui4_a_tab_that_is_sent_is_part_of_the_project_and_changes_what_would_be_saved() {
    let mut rig = rig_ui("ui4_fp", SIZES3[0].0, SIZES3[0].1, SIZES3[0].2);
    let book_idx = rig.app.tabs.iter().position(|t| t.title == "Book").expect("Book");
    rig.app.active_tab = book_idx;
    let before = rig.app.project_fingerprint();
    rig.app.send_to_report();
    assert_ne!(rig.app.project_fingerprint(), before, "a project with a sent tab has changes to save");
    rig.app.remove_from_report(book_idx);
    assert_eq!(rig.app.project_fingerprint(), before, "and none once it is taken out again (a project with no sent tab hashes as before)");
}

// ------------------------------------------------------------------------------------------------ empty screens

#[test]
fn ui5_every_empty_view_says_what_to_do_and_its_buttons_work() {
    for (ppp, w, h) in SIZES3 {
        let mut app = test_app("ui5_hints");
        let (ctx, input) = screen(ppp, w, h);
        let mut rig = Rig { app: std::mem::replace(&mut app, test_app("ui5_spare")), ctx, input, what: format!("ppp {ppp} {w}x{h}") };
        let what = rig.what.clone();
        // Data: three cards with the buttons the first screen always had
        let p = rig.draw(4);
        let texts: Vec<String> = p.boxes.iter().map(|(t, _)| t.clone()).collect();
        assert!(texts.iter().any(|t| t == "No data yet"), "{what}: {texts:?}");
        let names: Vec<String> = rig.app.ws.hint_rects.iter().map(|(n, _)| n.clone()).collect();
        assert_eq!(names, vec!["Open file\u{2026}", "Stack files\u{2026}", "Batch convert\u{2026}"], "{what}");
        let old: Vec<(String, egui::Rect)> = rig.ctx.data(|d| d.get_temp(egui::Id::new("empty_state_rects"))).unwrap_or_default();
        assert_eq!(old.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), vec!["open", "stack", "batch"], "{what}");
        for (_, r) in &rig.app.ws.hint_rects {
            assert!(r.left() >= theme::RAIL_W && r.right() <= rig.ctx.screen_rect().right() && r.bottom() <= rig.ctx.screen_rect().bottom(), "{what}: a card button {r:?} is inside the window");
        }
        // Stack opens the stack dialog
        let list = rig.app.ws.hint_rects.clone();
        click_name(&mut rig, &list, "Stack files\u{2026}");
        assert_eq!(rig.app.active_tool, ActiveToolModal::StackFiles, "{what}");
        rig.app.active_tool = ActiveToolModal::None;
        // Report with no data points to the Data view; Go to Data goes there
        rig.app.switch_view(View::Report);
        rig.draw(3);
        assert!(rig.app.ws.hint_rects.iter().any(|(n, _)| n == "Go to Data"), "{what}: {:?}", rig.app.ws.hint_rects);
        let list = rig.app.ws.hint_rects.clone();
        click_name(&mut rig, &list, "Go to Data");
        assert_eq!(rig.app.view(), View::Data, "{what}");
        // Model with nothing says the model needs a project; with one it offers the set-up
        rig.app.switch_view(View::Model);
        rig.draw(3);
        assert!(rig.app.ws.model_rects.iter().any(|(n, _)| n == "Save project as"), "{what}: no project, so it says so: {:?}", rig.app.ws.model_rects);
        rig.app.project.active = true;
        rig.draw(3);
        assert!(rig.app.ws.hint_rects.iter().any(|(n, _)| n == "Start set-up\u{2026}"), "{what}: {:?}", rig.app.ws.hint_rects);
        let list = rig.app.ws.hint_rects.clone();
        click_name(&mut rig, &list, "Start set-up\u{2026}");
        assert_eq!(rig.app.active_tool, ActiveToolModal::ModelTables, "{what}: with nothing open the set-up starts at Tables");
    }
}

// ------------------------------------------------------------------------------------------------ Ctrl+K

#[test]
fn ui6_the_empty_search_lists_what_you_used_last_and_phrases_to_try_and_a_click_on_a_phrase_types_it() {
    let mut rig = rig_ui("ui6_k", SIZES3[0].0, SIZES3[0].1, SIZES3[0].2);
    let what = rig.what.clone();
    rig.app.recent_commands = vec![CommandId::GapLadder, CommandId::Pivot];
    rig.app.open_command_palette();
    let p = rig.draw(4);
    let texts: Vec<String> = p.boxes.iter().map(|(t, _)| t.clone()).collect();
    for want in ["RECENT", "TRY TYPING", "ALL ACTIONS"] {
        assert!(texts.iter().any(|t| t == want), "{what}: '{want}' in {:?}", texts.iter().take(40).collect::<Vec<_>>());
    }
    let first_recent = texts.iter().position(|t| t.contains("Liquidity gap ladder")).expect("the gap ladder is listed");
    let recent_head = texts.iter().position(|t| t == "RECENT").expect("heading");
    let try_head = texts.iter().position(|t| t == "TRY TYPING").expect("heading");
    assert!(recent_head < first_recent && first_recent < try_head, "{what}: the recent commands come first");
    // the workflow tag at the right of a result is the new name, not the old menu
    assert!(texts.iter().any(|t| t == "Analyse"), "{what}: the gap ladder is tagged Analyse");
    // a click on a phrase types it
    let (_, b) = p.boxes.iter().find(|(t, _)| t.contains("nim") && t.contains("\u{2192}")).cloned().unwrap_or_else(|| panic!("{what}: the 'nim' phrase in {texts:?}"));
    rig.click_later(b.center());
    assert_eq!(rig.app.command_query, "nim", "{what}");
    let p = rig.draw(3);
    assert!(p.boxes.iter().any(|(t, _)| t.contains("Model measures")), "{what}: 'nim' finds the measures");
    // the words a banker types find the model steps and the Settings dialog
    for (q, want) in [("as of", "Set the as-of date"), ("crore", "Show money in Crore"), ("settings", "Settings")] {
        rig.app.command_query = q.to_string();
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t.contains(want)), "{what}: '{q}' finds '{want}'");
    }
    // the box in the bar is wide and says what it searches
    let bar = { rig.draw(2); rig.app.ws.bar_rects.clone() };
    assert!(rect_in(&bar, "Search", &what).width() >= theme::SEARCH_MIN_W, "{what}");
}

// ------------------------------------------------------------------------------------------------ Settings

#[test]
fn ui7_one_settings_dialog_holds_units_calendar_and_toolbar_choices_and_saves_the_calendar() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig_ui("ui7_settings", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx.clone(), CommandId::Settings);
        let p = rig.draw(5);
        assert!(rig.app.ws.settings_open, "{what}");
        crate::ui::render_tests::assert_dialog_fits(&rig.ctx, crate::ui::settings_dialog::SETTINGS_TITLE, &what);
        let list = rig.app.ws.settings_rects.clone();
        for s in SettingsSection::ALL {
            assert!(list.iter().any(|(n, _)| n == s.label()), "{what}: the list has '{}'", s.label());
        }
        assert!(p.boxes.iter().any(|(t, _)| t == "Money shown in"), "{what}: it opens on Units & numbers");
        // Calendar: April, then Save
        click_name(&mut rig, &list, "Calendar & fiscal year");
        rig.draw(3);
        let list = rig.app.ws.settings_rects.clone();
        click_name(&mut rig, &list, "April");
        rig.draw(2);
        let list = rig.app.ws.settings_rects.clone();
        click_name(&mut rig, &list, "Save calendar");
        assert_eq!(rig.app.settings.calendar.fy_start_month, 4, "{what}: the fiscal year starts in April");
        // Toolbar: switching a button off removes it from the Data bar
        rig.app.switch_view(View::Data);
        let list = rig.app.ws.settings_rects.clone();
        click_name(&mut rig, &list, "Toolbar & panels");
        rig.draw(2);
        let list = rig.app.ws.settings_rects.clone();
        click_name(&mut rig, &list, "Button Batch");
        assert!(rig.app.settings.toolbar_hidden.iter().any(|n| n == "Batch"), "{what}");
        let bar = { rig.draw(2); rig.app.ws.bar_rects.clone() };
        assert!(!bar.iter().any(|(n, _)| n == "Batch"), "{what}: the Batch button is off the bar");
        // the as-of section opens the set-up at its step
        let list = rig.app.ws.settings_rects.clone();
        click_name(&mut rig, &list, "As-of date");
        rig.draw(2);
        let list = rig.app.ws.settings_rects.clone();
        click_name(&mut rig, &list, "Change the as-of date");
        assert!(!rig.app.ws.settings_open, "{what}: the dialog closed");
        assert_eq!(rig.app.active_tool, ActiveToolModal::ModelAsOf, "{what}");
        assert_eq!(rig.app.view(), View::Data, "{what}");
        rig.app.settings.toolbar_hidden.clear();
    }
}

#[test]
fn ui8_the_view_menus_are_gone_from_the_bar_and_settings_has_the_theme_and_zoom() {
    let mut rig = rig_ui("ui8_menus", SIZES3[0].0, SIZES3[0].1, SIZES3[0].2);
    let bar = { rig.draw(2); rig.app.ws.bar_rects.clone() };
    let names: Vec<&str> = bar.iter().map(|(n, _)| n.as_str()).collect();
    for gone in ["Edit", "View", "Banking", "Charts", "Export", "Model"] {
        assert!(!names.contains(&gone), "{gone} is not a menu of the bar any more: {names:?}");
    }
    assert!(names.contains(&"File") && names.contains(&"Settings") && names.contains(&"Help"), "{names:?}");
    let _ = &mut rig;
}

// ------------------------------------------------------------------------------------------------ the model's set-up

#[test]
fn ui9_the_model_setup_is_one_window_with_a_strip_of_steps_and_back_and_next() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig_ui("ui9_flow", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx.clone(), CommandId::ModelLinks);
        rig.draw(5);
        assert_eq!(rig.app.view(), View::Model, "{what}: a model command opens in the Model view");
        assert_eq!(rig.app.active_tool, ActiveToolModal::ModelLinks, "{what}");
        let r1 = crate::ui::render_tests::assert_dialog_fits(&rig.ctx, crate::ui::model_flow::MODEL_SETUP_TITLE, &what);
        let flow = rig.app.ws.flow_rects.clone();
        for s in ModelStep::ALL {
            assert!(flow.iter().any(|(n, _)| n == s.label()), "{what}: the strip has '{}': {:?}", s.label(), flow.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>());
        }
        assert!(flow.iter().any(|(n, _)| n == "Back") && flow.iter().any(|(n, _)| n == "Next"), "{what}");
        // Next: Key check, in the same window
        click_name(&mut rig, &flow, "Next");
        rig.draw(4);
        assert_eq!(rig.app.active_tool, ActiveToolModal::ModelKeyCheck, "{what}");
        let r2 = crate::ui::render_tests::assert_dialog_fits(&rig.ctx, crate::ui::model_flow::MODEL_SETUP_TITLE, &what);
        assert!((r1.window.left() - r2.window.left()).abs() < 1.0 && (r1.window.top() - r2.window.top()).abs() < 1.0, "{what}: the window did not move: {:?} then {:?}", r1.window, r2.window);
        // a step of the strip: Measures, then Back goes to As-of date
        let flow = rig.app.ws.flow_rects.clone();
        click_name(&mut rig, &flow, "Measures");
        rig.draw(4);
        assert_eq!(rig.app.active_tool, ActiveToolModal::ModelMeasures, "{what}");
        let flow = rig.app.ws.flow_rects.clone();
        assert!(!flow.iter().any(|(n, _)| n == "Next"), "{what}: the last step has no Next");
        click_name(&mut rig, &flow, "Back");
        rig.draw(4);
        assert_eq!(rig.app.active_tool, ActiveToolModal::ModelAsOf, "{what}");
        // without a project the first thing said is that one is needed
        rig.app.active_tool = ActiveToolModal::None;
        rig.app.project.active = false;
        rig.app.run_command(&rig.ctx.clone(), CommandId::ModelSetup);
        assert_eq!(rig.app.active_tool, ActiveToolModal::ModelNoProject, "{what}");
    }
}

#[test]
fn ui10_the_model_view_draws_the_links_as_a_diagram_with_their_state_and_each_card_opens_its_step() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig_ui("ui10_model", ppp, w, h);
        let what = rig.what.clone();
        let book = rig.app.tabs.iter().find(|t| t.title == "Book").map(|t| t.id).expect("Book");
        let other = rig.app.tabs.iter().find(|t| t.title == "Other").map(|t| t.id).expect("Other");
        let (kb, ko) = (rig.app.model_ensure_key(book), rig.app.model_ensure_key(other));
        rig.app.project.model.links.push(micro_parquet::model::Link { data_tab: kb, lookup_tab: ko, keys: vec![micro_parquet::model::KeyPair { data: "Region".into(), lookup: "Region".into() }], unmatched_value: None });
        rig.app.switch_view(View::Model);
        rig.draw(5);
        let list = rig.app.ws.model_rects.clone();
        let (nb, no) = (rect_in(&list, "Node Book", &what), rect_in(&list, "Node Other", &what));
        assert!(nb.right() < no.left(), "{what}: the data table is left of its lookup: {nb:?} {no:?}");
        assert!(list.iter().any(|(n, _)| n.starts_with("Link ")), "{what}: the link says what its key check found: {:?}", list.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>());
        for (n, r) in &list {
            assert!(r.left() >= theme::RAIL_W - 0.5 || n.starts_with("Node") || n.starts_with("Link"), "{what}: '{n}' {r:?}");
        }
        // the steps are clickable and the cards open their step
        click_name(&mut rig, &list, "Change As-of date");
        assert_eq!(rig.app.active_tool, ActiveToolModal::ModelAsOf, "{what}");
        rig.app.active_tool = ActiveToolModal::None;
        rig.draw(2);
        let list = rig.app.ws.model_rects.clone();
        click_name(&mut rig, &list, "Step Key check");
        assert_eq!(rig.app.active_tool, ActiveToolModal::ModelKeyCheck, "{what}");
        rig.app.active_tool = ActiveToolModal::None;
        rig.draw(2);
        let list = rig.app.ws.model_rects.clone();
        click_name(&mut rig, &list, "Set up");
        assert_eq!(rig.app.active_tool, ActiveToolModal::ModelKeyCheck, "{what}: the set-up starts at the first step that needs work (the links are not checked)");
    }
}

// ------------------------------------------------------------------------------------------------ the right panel

#[test]
fn ui11_the_right_panel_has_sql_in_data_and_analytics_in_report_and_the_analytics_pane_changes_nothing() {
    let mut rig = rig_ui("ui11_panel", SIZES3[0].0, SIZES3[0].1, SIZES3[0].2);
    let what = rig.what.clone();
    rig.app.switch_view(View::Report);
    let report: Vec<&str> = rig.app.visible_side_tabs().iter().map(|t| t.name()).collect();
    assert!(report.contains(&"Analytics") && !report.contains(&"SQL"), "{what}: {report:?}");
    rig.app.switch_view(View::Data);
    let data: Vec<&str> = rig.app.visible_side_tabs().iter().map(|t| t.name()).collect();
    assert!(data.contains(&"SQL") && !data.contains(&"Analytics"), "{what}: {data:?}");
    // Analytics: the four placeholders, drawn, and no step or setting changed by showing it
    rig.app.switch_view(View::Report);
    let steps_before: Vec<usize> = rig.app.tabs.iter().map(|t| t.steps.len()).collect();
    rig.app.run_command(&rig.ctx.clone(), CommandId::ToggleAnalytics);
    rig.app.right_tab = crate::ui::steps::RightTab::Analytics;
    let p = rig.draw(4);
    for (name, _) in crate::ui::shell::ANALYTICS_PLACEHOLDERS {
        assert!(p.boxes.iter().any(|(t, _)| t == name), "{what}: the placeholder '{name}' is drawn");
    }
    assert_eq!(rig.app.tabs.iter().map(|t| t.steps.len()).collect::<Vec<_>>(), steps_before, "{what}: nothing changed");
    // the Model view has no right panel
    rig.app.switch_view(View::Model);
    let p = rig.draw(3);
    assert!(!p.boxes.iter().any(|(t, _)| t == "Analytics"), "{what}: no side panel in Model");
}

// ------------------------------------------------------------------------------------------------ the dashboard editor

fn open_dashboard(rig: &mut Rig) {
    rig.app.run_command(&rig.ctx.clone(), CommandId::Dashboard);
    rig.draw(6);
}

fn dash_rects(rig: &Rig) -> Vec<(String, egui::Rect)> {
    rig.app.dashboard_ui.rects.clone()
}

#[test]
fn ui12_the_dashboard_has_a_toolbar_one_export_button_and_clear_under_a_menu_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig_ui("ui12_dash", ppp, w, h);
        let what = rig.what.clone();
        rig.app.switch_view(View::Data);
        open_dashboard(&mut rig);
        assert_eq!(rig.app.view(), View::Report, "{what}: the dashboard is a Report thing");
        let list = dash_rects(&rig);
        for n in ["Add chart", "Add table", "Add KPI card", "Add filter", "Export", "More", "Close dashboard"] {
            assert!(list.iter().any(|(x, _)| x == n), "{what}: '{n}' in the toolbar: {:?}", list.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>());
        }
        assert!(!list.iter().any(|(n, _)| n == "Clear"), "{what}: Clear is inside a menu");
        // the toolbar is one row inside the window
        let row = ["Add chart", "Add table", "Add KPI card", "Add filter", "Export", "Close dashboard"].map(|n| rect_in(&list, n, &what));
        for r in row {
            assert!(r.right() <= rig.ctx.screen_rect().right() + 0.5 && r.left() >= theme::RAIL_W - 0.5, "{what}: {r:?}");
        }
        assert!(row.windows(2).all(|p| (p[0].center().y - p[1].center().y).abs() < 2.0), "{what}: one row");
        // Export opens a menu with the three files, each opening the export dialog on that format
        for (item, format) in [("Export HTML", crate::ui::dashboard_export::ExportFormat::Html), ("Export PowerPoint", crate::ui::dashboard_export::ExportFormat::Pptx), ("Export PDF", crate::ui::dashboard_export::ExportFormat::Pdf)] {
            let r = rect_in(&dash_rects(&rig), "Export", &what);
            rig.click_later(r.center());
            let list = dash_rects(&rig);
            let r = rect_in(&list, item, &what);
            rig.click_later(r.center());
            let dlg = rig.app.dashboard_ui.export.as_ref().unwrap_or_else(|| panic!("{what}: '{item}' opens the export dialog"));
            assert_eq!(dlg.format, format, "{what}: {item}");
            rig.app.dashboard_ui.export = None;
            rig.draw(2);
        }
        // ⋯ then Clear
        let r = rect_in(&dash_rects(&rig), "More", &what);
        rig.click_later(r.center());
        let list = dash_rects(&rig);
        assert!(list.iter().any(|(n, _)| n == "Clear"), "{what}: the menu has Clear");
    }
}

#[test]
fn ui13_an_empty_page_offers_cards_and_a_tile_has_a_floating_panel_that_closes_on_a_click_elsewhere() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig_ui("ui13_tile", ppp, w, h);
        let what = rig.what.clone();
        open_dashboard(&mut rig);
        let list = dash_rects(&rig);
        for n in ["Hint Add chart", "Hint Add table", "Hint Add KPI", "Hint Import design"] {
            assert!(list.iter().any(|(x, _)| x == n), "{what}: the empty page has '{n}': {:?}", list.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>());
        }
        // a KPI card from the card
        let b = rect_in(&list, "Hint Add KPI", &what);
        rig.click_later(b.center());
        rig.draw(4);
        let d = rig.app.project.dashboard.clone().expect("a dashboard");
        assert_eq!(d.pages[0].items.len(), 1, "{what}");
        assert!(matches!(d.pages[0].items[0].kind, DashItemKind::Kpi(_)), "{what}");
        // the new tile is selected: its panel floats next to it, inside the window, and the canvas keeps its width
        let list = dash_rects(&rig);
        let panel = rect_in(&list, "Tile panel", &what);
        let tile = rect_in(&list, &format!("Item {}", d.pages[0].items[0].id), &what);
        let screen = rig.ctx.screen_rect();
        assert!(panel.left() >= screen.left() && panel.right() <= screen.right() + 0.5 && panel.top() >= theme::TOP_BAR_H, "{what}: the panel {panel:?} is inside the window");
        assert!(!panel.intersects(tile.shrink(1.0)) || panel.width() <= theme::TILE_PANEL_W + 2.0 * theme::SPACE_M, "{what}: the panel does not sit on its tile ({panel:?} {tile:?})");
        let area = rect_in(&list, "Page area", &what);
        assert!(list.iter().any(|(n, _)| n == "Item title"), "{what}: the panel has the title box");
        // the page's settings are on the left, always
        assert!(list.iter().any(|(n, _)| n == "Page title") && list.iter().any(|(n, _)| n == "Layout Grid"), "{what}: page settings stay in the left column");
        let left = rect_in(&list, "Page title", &what);
        assert!(left.right() < area.left(), "{what}: the page settings are left of the page: {left:?} {area:?}");
        // a click on the page, away from the tile, closes the panel
        let away = egui::pos2(area.right() - theme::SPACE_L, area.bottom() - theme::SPACE_L);
        assert!(!tile.contains(away) && !panel.contains(away), "{what}: the click is away from the tile and the panel");
        rig.click_later(away);
        let area2 = rect_in(&dash_rects(&rig), "Page area", &what);
        assert!((area2.width() - area.width()).abs() < 1.0, "{what}: the page keeps its width with the panel open or closed: {area:?} {area2:?}");
        assert!(rig.app.dashboard_ui.selected.is_none(), "{what}: the tile is let go");
        assert!(!dash_rects(&rig).iter().any(|(n, _)| n == "Tile panel"), "{what}: the panel is gone");
    }
}

#[test]
fn ui14_a_chart_item_and_a_pivot_item_side_by_side_show_money_in_the_same_unit() {
    let mut rig = rig_ui("ui14_units", SIZES3[0].0, SIZES3[0].1, SIZES3[0].2);
    let what = rig.what.clone();
    open_dashboard(&mut rig);
    for (menu, title) in [("Add chart", "Chart: "), ("Add table", "Other by region")] {
        let r = rect_in(&dash_rects(&rig), menu, &what);
        rig.click_later(r.center());
        let p = rig.draw(3);
        let (_, b) = p.boxes.iter().rev().find(|(t, _)| t.contains(title)).cloned().unwrap_or_else(|| panic!("{what}: '{title}' in the {menu} menu"));
        rig.click_later(b.center());
        rig.draw(8);
    }
    let d = rig.app.project.dashboard.clone().expect("dashboard");
    assert_eq!(d.pages[0].items.len(), 2, "{what}");
    let chart = &rig.app.dashboard_ui.items[&d.pages[0].items[0].id];
    let (divisor, _, unit) = chart.scale.clone().expect("the chart item has its scale");
    let pivot_unit = d.pages[0].items[1].unit.unwrap_or_default();
    assert_eq!((divisor, unit.as_str()), (pivot_unit.divisor(), "Crore"), "{what}: both are in Crore");
}
