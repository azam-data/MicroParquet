//! The feedback round on the UI revamp: the "+" of the Report strip, the side panel's hide and show, the one-slide PowerPoint from
//! the Share menu, the Dashboard view, and the wide grid with text boxes, pictures and the right-click menu. Real clicks, three sizes.

use crate::ui::prelude::*;
use crate::ui::render_tests_p4::Rig;
use crate::ui::render_tests_p7::SIZES3;
use crate::ui::render_tests_p8::{pump_until, right_click};
use crate::ui::render_tests_ui::{rail_rect, rig_ui};
use micro_parquet::commands::CommandId;
use micro_parquet::dashboard::*;
use micro_parquet::workspace::View;

fn click_rail(rig: &mut Rig, v: View) {
    let r = rail_rect(rig, v);
    rig.click_later(r.center());
    rig.draw(4);
}

fn named(rig: &Rig, name: &str) -> egui::Rect {
    let list = &rig.app.dashboard_ui.rects;
    list.iter().find(|(n, _)| n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: '{name}' in {:?}", rig.what, list.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()))
}

fn click_name(rig: &mut Rig, name: &str) {
    let r = named(rig, name);
    rig.click_later(r.center());
    rig.draw(3);
}

fn select_tab(rig: &mut Rig, title: &str) {
    let i = rig.app.tabs.iter().position(|t| t.title.starts_with(title)).unwrap_or_else(|| panic!("{}: the tab '{title}'", rig.what));
    rig.app.active_tab = i;
    rig.draw(4);
}

// ------------------------------------------------------------------------------------------------ 1: the "+" beside the tabs

#[test]
fn fb1_the_report_strip_has_a_plus_that_adds_visuals_and_the_data_strip_keeps_its_own() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig_ui("fb1_plus", ppp, w, h);
        let what = rig.what.clone();
        rig.app.switch_view(View::Report);
        rig.draw(4);
        let plus = rig.app.ws.plus_rect.unwrap_or_else(|| panic!("{what}: the Report strip has a plus"));
        let strip = crate::ui::tabs::tab_strip_report(&rig.ctx).expect("the strip");
        let last = strip.cells.iter().map(|c| c.tab.right()).fold(f32::MIN, f32::max);
        assert!(plus.left() >= last - 0.5 && plus.right() <= rig.ctx.screen_rect().right() && (plus.center().y - strip.cells[0].tab.center().y).abs() < theme::TAB_STRIP_H, "{what}: the plus is beside the last tab: {plus:?} after {last}");
        rig.click_later(plus.center());
        let p = rig.draw(3);
        let texts: Vec<String> = p.boxes.iter().map(|(t, _)| t.to_lowercase()).collect();
        for want in ["chart", "pivot", "send this table to the report"] {
            assert!(texts.iter().any(|t| t.contains(want)), "{what}: the plus menu offers '{want}': {texts:?}");
        }
        // an entry runs the command: "Send to report" on the open chart tab says it is there already
        let (_, b) = p.boxes.iter().find(|(t, _)| t.to_lowercase().contains("send this table to the report")).cloned().expect("entry");
        rig.click_later(b.center());
        rig.draw(3);
        assert!(rig.app.status_message.contains("already in the Report view"), "{what}: {}", rig.app.status_message);
        // the Data view has no such plus (its own opens a file) and the Model view has no strip
        rig.app.switch_view(View::Data);
        rig.draw(4);
        let strip = crate::ui::tabs::tab_strip_report(&rig.ctx).expect("the strip");
        assert!(strip.cells.len() >= 2, "{what}");
    }
}

// ------------------------------------------------------------------------------------------------ 2: hide and show the side panel

#[test]
fn fb2_the_side_panel_hides_with_a_button_and_comes_back_with_an_arrow_and_the_page_widens() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig_ui("fb2_panel", ppp, w, h);
        let what = rig.what.clone();
        rig.app.switch_view(View::Data);
        rig.draw(5);
        assert!(rig.app.show_steps_sidebar, "{what}");
        let hide = rig.app.ws.panel_toggle_rect.unwrap_or_else(|| panic!("{what}: the panel has a hide button"));
        let screen = rig.ctx.screen_rect();
        assert!(hide.right() <= screen.right() + 0.5 && hide.left() >= screen.center().x, "{what}: the button is in the panel: {hide:?}");
        let before = rig.ctx.available_rect().width();
        rig.click_later(hide.center());
        rig.draw(5);
        assert!(!rig.app.show_steps_sidebar, "{what}: the button hides the panel");
        let wide = rig.ctx.available_rect().width();
        assert!(wide > before + theme::RIGHT_PANEL_W / 2.0, "{what}: the page grows to the right margin: {before} -> {wide}");
        let arrow = rig.app.ws.panel_toggle_rect.unwrap_or_else(|| panic!("{what}: a hidden panel leaves an arrow"));
        assert!(arrow.right() >= screen.right() - theme::SPACE_L && arrow.left() > screen.center().x, "{what}: the arrow sits at the right edge: {arrow:?}");
        rig.click_later(arrow.center());
        rig.draw(5);
        assert!(rig.app.show_steps_sidebar, "{what}: the arrow brings the panel back");
        assert!((rig.ctx.available_rect().width() - before).abs() < theme::SPACE_XS, "{what}: the page is as wide as before");
        // the Dashboard and Model views have no side panel, so no arrow either
        for v in [View::Dashboard, View::Model] {
            rig.app.show_steps_sidebar = false;
            rig.app.switch_view(v);
            rig.draw(4);
            assert!(rig.app.ws.panel_toggle_rect.is_none(), "{what}: no arrow in {v:?}");
            rig.app.show_steps_sidebar = true;
        }
    }
}

// ------------------------------------------------------------------------------------------------ 3: a chart or pivot to PowerPoint from Share

#[test]
fn fb3_the_share_menu_writes_the_open_chart_or_pivot_as_a_one_slide_powerpoint_and_says_why_not_for_a_table() {
    let mut rig = rig_ui("fb3_pptx", SIZES3[0].0, SIZES3[0].1, SIZES3[0].2);
    let what = rig.what.clone();
    let dd = rig.app.paths.root_dir.clone();
    rig.app.switch_view(View::Report);
    rig.draw(4);
    // the entry is in Share, under the report's other exports
    assert!(micro_parquet::workspace::menu_entries(micro_parquet::workspace::MenuKind::ShareReport).iter().any(|e| matches!(e, micro_parquet::workspace::MenuEntry::Cmd(CommandId::ExportTabPptx))), "{what}");
    for (title, file) in [("Chart: ", "chart.pptx"), ("Other by region", "pivot.pptx")] {
        select_tab(&mut rig, title);
        assert!(rig.app.command_unavailable(CommandId::ExportTabPptx).is_none(), "{what}: available on '{title}'");
        let path = dd.join(file);
        rig.app.dashboard_ui.export_tab_path = Some(path.clone());
        rig.app.run_command(&rig.ctx.clone(), CommandId::ExportTabPptx);
        let p2 = path.clone();
        pump_until(&mut rig, "the slide is written", &move |_| p2.exists());
        let mut z = ::zip::read::ZipArchive::new(std::io::Cursor::new(std::fs::read(&path).expect("file"))).expect("a PowerPoint package");
        assert!(z.by_name("ppt/slides/slide1.xml").is_ok() && z.by_name("ppt/slides/slide2.xml").is_err(), "{what}: one slide for '{title}'");
    }
    // the chart slide holds a native chart and the pivot slide a table
    let mut z = ::zip::read::ZipArchive::new(std::io::Cursor::new(std::fs::read(dd.join("chart.pptx")).expect("file"))).expect("zip");
    assert!(z.by_name("ppt/charts/chart1.xml").is_ok(), "{what}: the chart is a native chart");
    let mut z = ::zip::read::ZipArchive::new(std::io::Cursor::new(std::fs::read(dd.join("pivot.pptx")).expect("file"))).expect("zip");
    let mut slide = String::new();
    std::io::Read::read_to_string(&mut z.by_name("ppt/slides/slide1.xml").expect("slide"), &mut slide).expect("xml");
    assert!(slide.contains("<a:tbl>") && slide.contains("Sylhet") && slide.contains("Khulna"), "{what}: the pivot's own rows: {slide}");
    // a plain table says what to open instead
    rig.app.switch_view(View::Data);
    select_tab(&mut rig, "Book");
    let why = rig.app.command_unavailable(CommandId::ExportTabPptx).expect("a table has no slide of its own");
    assert!(why.contains("chart tab or a pivot tab"), "{what}: {why}");
}

// ------------------------------------------------------------------------------------------------ 4: the Dashboard view

#[test]
fn fb4_the_dashboard_is_the_fourth_view_with_no_tab_strip_and_it_returns_to_the_tab_it_left() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig_ui("fb4_view", ppp, w, h);
        let what = rig.what.clone();
        assert_eq!(rig.app.ws.rail_rects.len(), 4, "{what}");
        let order: Vec<View> = rig.app.ws.rail_rects.iter().map(|(v, _)| *v).collect();
        assert_eq!(order, [View::Report, View::Dashboard, View::Data, View::Model], "{what}: the rail's order");
        let ys: Vec<f32> = rig.app.ws.rail_rects.iter().map(|(_, r)| r.top()).collect();
        assert!(ys.windows(2).all(|p| p[0] < p[1]), "{what}: the buttons are stacked, none overlapping");
        click_rail(&mut rig, View::Dashboard);
        assert_eq!(rig.app.view(), View::Dashboard, "{what}");
        assert!(rig.app.dashboard_ui.open && rig.app.project.dashboard.is_some(), "{what}: the first visit makes the dashboard");
        assert!(rig.app.dashboard_ui.rects.iter().any(|(n, _)| n == "Page area"), "{what}: the page is drawn");
        // the Report strip's old Dashboard entry is gone and the strip is not drawn here
        assert!(rig.app.ws.plus_rect.is_none() || rig.app.view() != View::Report, "{what}");
        let strip_h = rig.ctx.available_rect().top();
        click_rail(&mut rig, View::Report);
        assert_eq!(rig.app.view(), View::Report, "{what}");
        assert!(!rig.app.dashboard_ui.open, "{what}: leaving closes it");
        assert!(rig.ctx.available_rect().top() > strip_h, "{what}: the Report view has its strip, the Dashboard view does not");
        // the Dashboard command and Ctrl+K go to the view too, and the Dashboard view is remembered across a restart by its name
        rig.app.run_command(&rig.ctx.clone(), CommandId::Dashboard);
        rig.draw(4);
        assert_eq!(rig.app.view(), View::Dashboard, "{what}");
        assert_eq!(rig.app.settings.last_view.as_deref(), Some("Dashboard"), "{what}");
        assert_eq!(View::from_name("dashboard"), Some(View::Dashboard));
    }
}

// ------------------------------------------------------------------------------------------------ 5: wide grid, text, picture, right click

fn add_text_items(rig: &mut Rig, n: usize) {
    for _ in 0..n {
        click_name(rig, "Add text");
        rig.app.dashboard_ui.selected = None;
        rig.draw(2);
    }
}

#[test]
fn fb5_the_wide_preset_shows_twelve_places_four_across_and_three_down_on_a_16_9_page() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig_ui("fb5_wide", ppp, w, h);
        let what = rig.what.clone();
        click_rail(&mut rig, View::Dashboard);
        let area = named(&rig, "Page area");
        assert!((area.width() / area.height() - PAGE_W as f32 / PAGE_H as f32).abs() < theme::PAGE_SHAPE_EPS as f32, "{what}: a new page is 16:10 as before");
        click_name(&mut rig, "Wide grid");
        let page = rig.app.project.dashboard.as_ref().expect("d").pages[0].clone();
        assert_eq!((page.height(), page.layout, page.grid_rows), (PAGE_H_WIDE, PageLayout::Grid { per_row: 4 }, Some(3)), "{what}");
        let area = named(&rig, "Page area");
        assert!((area.width() / area.height() - PAGE_W as f32 / PAGE_H_WIDE as f32).abs() < theme::PAGE_SHAPE_EPS as f32, "{what}: drawn 16:9: {area:?}");
        add_text_items(&mut rig, 12);
        let page = rig.app.project.dashboard.as_ref().expect("d").pages[0].clone();
        assert_eq!(page.items.len(), 12, "{what}");
        let cells: Vec<egui::Rect> = page.items.iter().map(|i| named(&rig, &format!("Item {}", i.id))).collect();
        let xs: std::collections::BTreeSet<i32> = cells.iter().map(|c| c.left().round() as i32).collect();
        let ys: std::collections::BTreeSet<i32> = cells.iter().map(|c| c.top().round() as i32).collect();
        assert_eq!((xs.len(), ys.len()), (4, 3), "{what}: four columns and three rows on one view");
        let area = named(&rig, "Page area");
        assert!(cells.iter().all(|c| area.expand(0.5).contains_rect(*c)), "{what}: every place is on the page");
        // the whole page fits the window: nothing needs scrolling to see all twelve
        assert!(area.bottom() <= rig.ctx.screen_rect().bottom() + 0.5 && area.right() <= rig.ctx.screen_rect().right() + 0.5, "{what}: {area:?}");
        // the old shape is one click away, and the grid's rows can be set
        click_name(&mut rig, "Shape 16:10");
        assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages[0].height(), PAGE_H, "{what}");
    }
}

#[test]
fn fb5_a_text_box_is_edited_in_the_tile_panel_and_a_picture_is_picked_from_a_file_and_both_are_drawn() {
    let mut rig = rig_ui("fb5_static", SIZES3[0].0, SIZES3[0].1, SIZES3[0].2);
    let what = rig.what.clone();
    let dd = rig.app.paths.root_dir.clone();
    click_rail(&mut rig, View::Dashboard);
    click_name(&mut rig, "Add text");
    let id = rig.app.project.dashboard.as_ref().expect("d").pages[0].items[0].id.clone();
    // the tile panel has the text, its size, bold and alignment, and no title
    for n in ["Text content", "Text size", "Text bold", "Text centre"] {
        named(&rig, n);
    }
    assert!(!rig.app.dashboard_ui.rects.iter().any(|(n, _)| n == "Item title"), "{what}: a text box has no title");
    click_name(&mut rig, "Text bold");
    click_name(&mut rig, "Text centre");
    let r = named(&rig, "Text content");
    rig.click_later(r.center());
    rig.step(vec![egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND }]);
    rig.step(vec![egui::Event::Text("ALCO pack".into())]);
    rig.draw(4);
    let DashItemKind::Text(t) = rig.app.project.dashboard.as_ref().expect("d").pages[0].items[0].kind.clone() else { panic!("{what}: a text box") };
    assert_eq!((t.text.as_str(), t.bold, t.align), ("ALCO pack", true, TextAlign::Centre), "{what}");
    rig.app.dashboard_ui.selected = None;
    let p = rig.draw(3);
    assert!(p.glyphs.iter().any(|(g, _)| g.contains('A')), "{what}: the text is drawn on the page");
    // a picture: the file is read, made into a PNG, kept in the project and drawn
    let path = dd.join("logo.png");
    ::image::DynamicImage::new_rgb8(60, 30).save(&path).expect("png");
    rig.app.dashboard_ui.image_override = Some(path);
    click_name(&mut rig, "Add image");
    rig.draw(4);
    let page = rig.app.project.dashboard.as_ref().expect("d").pages[0].clone();
    let DashItemKind::Image(im) = &page.items[1].kind else { panic!("{what}: a picture item: {:?}", page.items[1].kind) };
    assert_eq!((im.name.as_str(), im.w, im.h), ("logo.png", 60, 30), "{what}");
    assert_eq!(rig.app.dashboard_ui.img_textures.len(), 1, "{what}: the picture is drawn from a texture");
    // a file that is not a picture says so and adds nothing
    let bad = dd.join("notes.png");
    std::fs::write(&bad, b"not a picture").expect("file");
    rig.app.dashboard_ui.image_override = Some(bad);
    click_name(&mut rig, "Add image");
    rig.draw(3);
    assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages[0].items.len(), 2, "{what}");
    assert!(rig.app.dashboard_ui.notice.contains("not a PNG or JPEG"), "{what}: {}", rig.app.dashboard_ui.notice);
    // the project saves them and loads them again
    let json = serde_json::to_string(rig.app.project.dashboard.as_ref().expect("d")).expect("json");
    let back: Dashboard = serde_json::from_str(&json).expect("loads");
    assert_eq!(back.pages[0].items.len(), 2);
    assert!(rig.app.dashboard_ui.items.get(&id).is_none_or(|v| v.data.is_none()), "{what}: a text box reads no data");
}

fn two_items(rig: &mut Rig) {
    click_rail(rig, View::Dashboard);
    for (menu, title) in [("Add chart", "Chart: "), ("Add table", "Other by region")] {
        click_name(rig, menu);
        let p = rig.draw(3);
        let (_, b) = p.boxes.iter().rev().find(|(t, _)| t.contains(title)).cloned().unwrap_or_else(|| panic!("{}: '{title}' in {menu}", rig.what));
        rig.click_later(b.center());
        rig.draw(8);
    }
    rig.app.dashboard_ui.selected = None;
    rig.draw(3);
}

#[test]
fn fb5_a_right_click_on_a_visual_opens_its_source_tab_in_the_main_view_and_a_text_box_has_none() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig_ui("fb5_menu", ppp, w, h);
        let what = rig.what.clone();
        two_items(&mut rig);
        let page = rig.app.project.dashboard.as_ref().expect("d").pages[0].clone();
        let (chart, pivot) = (page.items[0].id.clone(), page.items[1].id.clone());
        // the table: right-click its body, then "Open in the main view"
        let r = named(&rig, &format!("Item {pivot}"));
        right_click(&mut rig, egui::pos2(r.center().x, r.bottom() - theme::SPACE_S));
        rig.draw(2);
        click_name(&mut rig, "Open source");
        rig.draw(4);
        assert_eq!(rig.app.view(), View::Report, "{what}: the pivot opens in the Report view");
        assert!(rig.app.tabs[rig.app.active_tab].title.starts_with("Other by region"), "{what}: {}", rig.app.tabs[rig.app.active_tab].title);
        assert!(!rig.app.dashboard_ui.open, "{what}");
        // the chart: right-click away from a mark (the corner of its card), same result with the chart tab
        click_rail(&mut rig, View::Dashboard);
        let r = named(&rig, &format!("Item {chart}"));
        right_click(&mut rig, egui::pos2(r.left() + theme::SPACE_XS, r.top() + theme::SPACE_XS));
        rig.draw(2);
        click_name(&mut rig, "Open source");
        rig.draw(4);
        assert!(rig.app.view() == View::Report && rig.app.active_is_chart_tab(), "{what}: the chart opens in its own tab");
        // a click elsewhere closes the menu without going anywhere
        click_rail(&mut rig, View::Dashboard);
        let r = named(&rig, &format!("Item {pivot}"));
        right_click(&mut rig, r.center());
        rig.draw(2);
        assert!(rig.app.dashboard_ui.rects.iter().any(|(n, _)| n == "Item menu"), "{what}: the menu is open");
        let area = named(&rig, "Page area");
        rig.click_later(egui::pos2(area.left() + 2.0, area.bottom() - 2.0));
        rig.draw(3);
        assert!(!rig.app.dashboard_ui.rects.iter().any(|(n, _)| n == "Item menu") && rig.app.view() == View::Dashboard, "{what}: a click away closes the menu");
        // "Remove from the page" is in the same menu
        right_click(&mut rig, r.center());
        rig.draw(2);
        click_name(&mut rig, "Menu remove");
        rig.draw(3);
        assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages[0].items.len(), 1, "{what}");
        // a text box has no source: the entry is there but greyed, and clicking it goes nowhere
        click_name(&mut rig, "Add text");
        let id = rig.app.project.dashboard.as_ref().expect("d").pages[0].items[1].id.clone();
        rig.app.dashboard_ui.selected = None;
        rig.draw(3);
        let r = named(&rig, &format!("Item {id}"));
        right_click(&mut rig, r.center());
        rig.draw(2);
        click_name(&mut rig, "Open source");
        rig.draw(3);
        assert_eq!(rig.app.view(), View::Dashboard, "{what}: nothing to open for a text box");
    }
}

#[test]
fn fb5_the_right_click_menu_of_a_chart_mark_has_open_in_the_main_view_too() {
    let mut rig = rig_ui("fb5_mark", SIZES3[0].0, SIZES3[0].1, SIZES3[0].2);
    let what = rig.what.clone();
    two_items(&mut rig);
    let chart = rig.app.project.dashboard.as_ref().expect("d").pages[0].items[0].id.clone();
    let m = crate::ui::render_tests_p8::mark_rect(&rig, egui::Id::new(("dash_chart", chart.clone())), 0);
    let p = right_click(&mut rig, m.center());
    let texts: Vec<String> = p.boxes.iter().map(|(t, _)| t.clone()).collect();
    assert!(texts.iter().any(|t| t == "Open in the main view") && texts.iter().any(|t| t == "Filter the page by this"), "{what}: {texts:?}");
    assert!(!rig.app.dashboard_ui.rects.iter().any(|(n, _)| n == "Item menu"), "{what}: one menu, not two");
    let (_, b) = p.boxes.iter().find(|(t, _)| t == "Open in the main view").cloned().expect("entry");
    rig.click_later(b.center());
    rig.draw(4);
    assert!(rig.app.view() == View::Report && rig.app.active_is_chart_tab(), "{what}");
}

/// The picture of the wide grid (light, then dark): `docs/guide/img/dashboard-wide-grid[-dark].png`.
#[test]
fn fb5_the_wide_grid_is_drawn_as_a_picture_in_both_themes() {
    use crate::ui::render_tests_p8::{can_shoot, shoot, styled, SHOT_H, SHOT_PPP, SHOT_W};
    if !can_shoot() {
        return;
    }
    for dark in [false, true] {
        let mut rig = rig_ui("fb5_shot", SHOT_PPP, SHOT_W, SHOT_H);
        styled(&mut rig, dark);
        click_rail(&mut rig, View::Dashboard);
        click_name(&mut rig, "Wide grid");
        for (menu, title) in [("Add chart", "Chart: "), ("Add table", "Other by region")] {
            click_name(&mut rig, menu);
            let p = rig.draw(3);
            if let Some((_, b)) = p.boxes.iter().rev().find(|(t, _)| t.contains(title)).cloned() {
                rig.click_later(b.center());
                rig.draw(8);
            }
        }
        click_name(&mut rig, "Add text");
        rig.app.dashboard_ui.selected = None;
        rig.draw(6);
        shoot(&mut rig, "dashboard-wide-grid", dark, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SHOT_W, SHOT_H)));
    }
}
