//! Phase 10 render tests: headless egui renders at 1219×686, 1600×900 and 1291×700 pt, by real clicks.
//!
//! D1 (the dashboard view): the Dashboard command, pages, items, page filters, and what each control does when its real rect is clicked.

use crate::ui::layout_tests::Painted;
use crate::ui::prelude::*;
use crate::ui::render_tests::screen;
use crate::ui::render_tests_p4::Rig;
use crate::ui::render_tests_p7::{assert_rules, SIZES3};
use crate::ui::render_tests_p8::{assert_no_overlap, assert_not_clipped};
use crate::ui::tests::{parquet_source, settle, test_app};
use micro_parquet::commands::CommandId;
use micro_parquet::dashboard::*;
use micro_parquet::models::*;
use polars::prelude::{DataFrame, NamedFrom, Series};
use std::collections::HashMap;

fn book(n: usize, regions: &[&str]) -> DataFrame {
    DataFrame::new(vec![
        Series::new("Risk".into(), (0..n).map(|i| ["High", "Medium", "Low"][i % 3].to_string()).collect::<Vec<_>>()).into(),
        Series::new("Region".into(), (0..n).map(|i| regions[i % regions.len()].to_string()).collect::<Vec<_>>()).into(),
        Series::new("Balance".into(), (0..n).map(|i| (10 + i as i64) * 1_000_000).map(|v| v as f64).collect::<Vec<f64>>()).into(),
    ])
    .expect("frame")
}

fn sum_by(field: &str) -> TransformStep {
    TransformStep::from_pivot_spec(PivotSpec { rows: vec![field.into()], values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None }], row_totals: true, column_totals: true, ..Default::default() })
}

/// A project with: the data tab "Book" (Risk, Region, Balance), its chart tab, a second data tab "Other" and a pivot tab on it.
fn rig10(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
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
    rig.draw(5);
    rig
}

fn rect_of(rig: &Rig, name: &str) -> egui::Rect {
    rig.app.dashboard_ui.rects.iter().find(|(n, _)| n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: the control '{name}' in {:?}", rig.what, rig.app.dashboard_ui.rects.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()))
}

fn click_named(rig: &mut Rig, name: &str) -> Painted {
    let r = rect_of(rig, name);
    rig.click_later(r.center())
}

fn key_of(rig: &Rig, title: &str) -> String {
    let id = rig.app.tabs.iter().find(|t| t.title == title || t.title.starts_with(title)).unwrap_or_else(|| panic!("{}: the tab '{title}'", rig.what)).id;
    rig.app.project.keys[&id].clone()
}

/// Opens the dashboard and puts the chart tab and the pivot tab on its first page, by clicks.
fn dashboard_with_two_items(rig: &mut Rig) {
    rig.app.run_command(&rig.ctx.clone(), CommandId::Dashboard);
    assert!(rig.app.dashboard_ui.open, "{}: the Dashboard command opens the view", rig.what);
    rig.draw(5);
    for title in ["Chart: ", "Other by region"] {
        click_named(rig, "Add item");
        let p = rig.draw(3);
        let (_, b) = p.boxes.iter().rev().find(|(t, _)| t.ends_with(title) || t.contains(title)).cloned().unwrap_or_else(|| panic!("{}: '{title}' in the Add item menu: {:?}", rig.what, p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>()));
        rig.click_later(b.center());
        rig.draw(8);
    }
}

#[test]
fn d1_outside_a_project_the_dashboard_says_so() {
    let mut rig = rig10("p10d1_noproj", SIZES3[0].0, SIZES3[0].1, SIZES3[0].2);
    rig.app.project.active = false;
    rig.app.run_command(&rig.ctx.clone(), CommandId::Dashboard);
    assert!(!rig.app.dashboard_ui.open && rig.app.project.dashboard.is_none());
    assert_eq!(rig.app.status_message, "Dashboards belong to a project. Save or open a project first.");
}

#[test]
fn d1_items_are_added_by_clicks_draw_the_tabs_own_numbers_and_fit_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig10("p10d1_items", ppp, w, h);
        let what = rig.what.clone();
        dashboard_with_two_items(&mut rig);
        let d = rig.app.project.dashboard.clone().expect("a dashboard");
        let page = &d.pages[0];
        assert_eq!(page.items.len(), 2, "{what}");
        assert!(matches!(page.items[0].kind, DashItemKind::Chart(_)) && matches!(page.items[1].kind, DashItemKind::Pivot(_)), "{what}");
        let p = rig.draw(6);
        // the chart item shows the chart tab's own numbers
        let chart_tab_id = rig.app.tabs.iter().find(|t| rig.app.chart_tabs.contains_key(&t.id)).expect("the chart tab").id;
        let own = &rig.app.chart_tabs[&chart_tab_id].data;
        let view = &rig.app.dashboard_ui.items[&page.items[0].id];
        let ItemData::Chart(c) = view.data.as_ref().unwrap_or_else(|| panic!("{what}: the chart item has its numbers")) else { panic!("{what}: a chart") };
        assert_eq!((&c.categories, &c.values, c.total_rows), (&own.categories, &own.values, own.total_rows), "{what}: the item has the chart tab's numbers");
        // and the pivot item has the pivot tab's table: three rows (two regions and the total)
        let ItemData::Table(t) = rig.app.dashboard_ui.items[&page.items[1].id].data.as_ref().unwrap_or_else(|| panic!("{what}: the pivot item has its table")) else { panic!("{what}: a table") };
        assert_eq!(t.df.height(), 3, "{what}: Sylhet, Khulna and the grand total");
        // everything fits: the chart's own rules, no overlap of two labels, nothing cut
        let shown = crate::ui::chart_render::last_shown(&rig.ctx, egui::Id::new(("dash_chart", page.items[0].id.clone()))).unwrap_or_else(|| panic!("{what}: the chart is drawn"));
        assert_rules(&shown, rig.ctx.screen_rect(), &what);
        let area = rect_of(&rig, "Page area");
        for key in ["Item i1", "Item i2"] {
            assert!(area.contains_rect(rect_of(&rig, key).shrink(0.5)), "{what}: {key} is inside the page");
        }
        assert!(!rect_of(&rig, "Item i1").intersects(rect_of(&rig, "Item i2")), "{what}: the two items do not overlap");
        let items_area = area;
        let in_page: Vec<(String, egui::Rect)> = p.boxes.iter().filter(|(_, b)| items_area.contains_rect(*b)).cloned().collect();
        assert_no_overlap(&in_page.iter().filter(|(t, _)| t.starts_with("Chart: ") || t.starts_with("Other") || t == "Region" || t.contains("Sum of")).cloned().collect::<Vec<_>>(), &what);
        assert_not_clipped(&p, rig.ctx.screen_rect(), &what);
        assert!(p.boxes.iter().all(|(_, b)| rig.ctx.screen_rect().expand(1.0).contains_rect(*b) || b.width() == 0.0), "{what}: nothing is drawn outside the window");
    }
}

#[test]
fn d1_a_filter_on_a_data_field_filters_the_linked_item_and_the_other_item_says_it_does_not_apply() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig10("p10d1_filter", ppp, w, h);
        let what = rig.what.clone();
        dashboard_with_two_items(&mut rig);
        // Add filter: the data tab "Book", the field "Risk"
        click_named(&mut rig, "Add filter");
        rig.draw(3);
        assert!(rig.app.dashboard_ui.add_filter.is_some(), "{what}: the add-filter row is open");
        for (combo, text) in [("Filter data tab", "Book"), ("Filter field", "Risk")] {
            click_named(&mut rig, combo);
            let p = rig.draw(3);
            let (_, b) = p.boxes.iter().rev().find(|(t, _)| t == text).cloned().unwrap_or_else(|| panic!("{what}: '{text}' in {combo}: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>()));
            rig.click_later(b.center());
            rig.draw(3);
        }
        click_named(&mut rig, "Filter Add");
        rig.draw(8);
        let f = rig.app.project.dashboard.as_ref().expect("d").pages[0].filters[0].clone();
        assert_eq!((f.field.as_str(), f.label.as_str(), f.tab.as_str()), ("Risk", "Risk", key_of(&rig, "Book").as_str()), "{what}");
        assert_eq!(rig.app.dashboard_ui.values[&f.id].values, vec!["High".to_string(), "Low".to_string(), "Medium".to_string()], "{what}: the dropdown lists the field's values");
        // pick "High": the chart item has a third of the rows, the pivot item (another data tab) says so and keeps all of its rows
        click_named(&mut rig, "Filter Risk");
        let p = rig.draw(3);
        let (_, b) = p.boxes.iter().rev().find(|(t, _)| t == "High").cloned().unwrap_or_else(|| panic!("{what}: 'High' in the dropdown"));
        rig.click_later(b.center());
        let p = rig.draw(8);
        assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages[0].state.filter_picks[0], Some(vec!["High".to_string()]), "{what}");
        let ItemData::Chart(c) = rig.app.dashboard_ui.items["i1"].data.as_ref().expect("numbers") else { panic!("chart") };
        assert_eq!(c.total_rows, 20, "{what}: 60 rows, a third are High");
        let ItemData::Table(t) = rig.app.dashboard_ui.items["i2"].data.as_ref().expect("table") else { panic!("table") };
        assert_eq!(t.df.height(), 3, "{what}: the pivot item is not filtered");
        assert!(p.boxes.iter().any(|(t, _)| t.contains("Filter 'Risk' doesn't apply to this item")), "{what}: the unlinked item says so: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>());
        assert_not_clipped(&p, rig.ctx.screen_rect(), &what);
        // Clear: all values again
        click_named(&mut rig, "Clear");
        rig.draw(8);
        assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages[0].state.filter_picks[0], None, "{what}");
        let ItemData::Chart(c) = rig.app.dashboard_ui.items["i1"].data.as_ref().expect("numbers") else { panic!("chart") };
        assert_eq!(c.total_rows, 60, "{what}: Clear puts every row back");
        // the fifth filter is refused
        for i in 0..3 {
            let d = rig.app.project.dashboard.as_mut().expect("d");
            d.pages[0].filters.push(PageFilter { id: format!("x{i}"), label: format!("X{i}"), tab: f.tab.clone(), field: "Region".into(), ..Default::default() });
        }
        rig.draw(5);
        click_named(&mut rig, "Add filter");
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t == "A page can have 4 filters. Remove one first."), "{what}: the fifth filter is refused in words");
        assert!(rig.app.dashboard_ui.add_filter.is_none(), "{what}");
        // the cross removes a filter
        click_named(&mut rig, "Remove filter X2");
        rig.draw(3);
        assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages[0].filters.len(), 3, "{what}");
    }
}

#[test]
fn d1_pages_are_added_renamed_reordered_and_deleted_after_a_confirm_and_items_move_and_go() {
    let mut rig = rig10("p10d1_pages", SIZES3[0].0, SIZES3[0].1, SIZES3[0].2);
    let what = rig.what.clone();
    dashboard_with_two_items(&mut rig);
    click_named(&mut rig, "Add page");
    rig.draw(3);
    assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages.len(), 2, "{what}");
    assert_eq!(rig.app.dashboard_ui.page, 1, "{what}: the new page is shown");
    // rename: the page's title box
    let r = rect_of(&rig, "Page title");
    rig.click_later(r.center());
    rig.step(vec![egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND }]);
    rig.step(vec![egui::Event::Text("Liquidity".into())]);
    rig.draw(3);
    assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages[1].title, "Liquidity", "{what}");
    // move it up: it becomes the first page
    click_named(&mut rig, "Page earlier");
    rig.draw(3);
    let d = rig.app.project.dashboard.clone().expect("d");
    assert_eq!((d.pages[0].title.as_str(), d.pages[1].items.len()), ("Liquidity", 2), "{what}: the page moved with its items");
    // delete asks first
    click_named(&mut rig, "Delete page");
    rig.draw(3);
    assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages.len(), 2, "{what}: nothing is deleted before the confirm");
    click_named(&mut rig, "Keep page");
    rig.draw(3);
    assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages.len(), 2, "{what}: Keep keeps it");
    click_named(&mut rig, "Delete page");
    rig.draw(3);
    click_named(&mut rig, "Confirm delete page");
    rig.draw(3);
    assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages.len(), 1, "{what}");
    // items per row, and moving and removing an item
    rig.app.dashboard_ui.selected = Some("i1".into());
    rig.draw(3);
    click_named(&mut rig, "Item later");
    rig.draw(3);
    assert!(matches!(rig.app.project.dashboard.as_ref().expect("d").pages[0].items[1].kind, DashItemKind::Chart(_)), "{what}: the chart moved after the pivot");
    click_named(&mut rig, "Remove item");
    rig.draw(3);
    assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages[0].items.len(), 1, "{what}");
    // the dashboard is part of the project: it is unsaved now
    assert!(rig.app.project_unsaved(), "{what}: a change to the dashboard is a change to the project");
}

#[test]
fn d1_renaming_a_source_tab_keeps_the_item_and_a_closed_one_says_source_tab_removed_and_the_title_opens_the_tab() {
    let mut rig = rig10("p10d1_rename", SIZES3[0].0, SIZES3[0].1, SIZES3[0].2);
    let what = rig.what.clone();
    dashboard_with_two_items(&mut rig);
    // rename the pivot tab: the item follows (it holds the tab's key, not its name)
    let at = rig.app.tabs.iter().position(|t| t.title == "Other by region").expect("tab");
    rig.app.tabs[at].title = "Regional balances".into();
    let d = rig.app.project.dashboard.clone().expect("d");
    let res = rig.app.dash_resolve(&d.pages[0].items[1]).unwrap_or_else(|e| panic!("{what}: {e}"));
    assert_eq!(res.title, "Regional balances", "{what}");
    let p = rig.draw(4);
    assert!(p.boxes.iter().any(|(t, _)| t == "Regional balances"), "{what}: the item shows the new name");
    // clicking the item's title opens its source tab and leaves the dashboard
    click_named(&mut rig, "Title i2");
    rig.draw(3);
    assert!(!rig.app.dashboard_ui.open && rig.app.tabs[rig.app.active_tab].title == "Regional balances", "{what}: the title opens the tab");
    // a deleted tab: the item stays and says so
    let d = rig.app.project.dashboard.as_mut().expect("d");
    d.pages[0].items[1].kind = DashItemKind::Pivot("t99".into());
    rig.app.dashboard_ui.open = true;
    let p = rig.draw(4);
    assert!(p.boxes.iter().any(|(t, _)| t == "Source tab removed"), "{what}: 'Source tab removed' on the item: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>());
    // the tab strip's Dashboard entry brings it back from a tab
    rig.app.dashboard_ui.open = false;
    rig.draw(3);
    let r = rig.app.dashboard_ui.strip_rect.expect("the strip has a Dashboard entry once the project has a dashboard");
    rig.click_later(r.center());
    rig.draw(3);
    assert!(rig.app.dashboard_ui.open, "{what}: the Dashboard entry of the tab strip opens it");
    click_named(&mut rig, "Close dashboard");
    rig.draw(3);
    assert!(!rig.app.dashboard_ui.open, "{what}");
}

/// The picture of the dashboard page (light, then dark): `docs/guide/img/dashboard-page[-dark].png`.
#[test]
fn d1_the_dashboard_page_is_drawn_as_a_picture_in_both_themes() {
    use crate::ui::render_tests_p8::{can_shoot, shoot, styled, SHOT_H, SHOT_PPP, SHOT_W};
    if !can_shoot() {
        return;
    }
    for dark in [false, true] {
        let mut rig = rig10("p10d1_shot", SHOT_PPP, SHOT_W, SHOT_H);
        styled(&mut rig, dark);
        dashboard_with_two_items(&mut rig);
        rig.draw(6);
        shoot(&mut rig, "dashboard-page", dark, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SHOT_W, SHOT_H)));
    }
}
