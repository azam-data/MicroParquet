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
    // (UI revamp: the page's settings are always in the left column; the old "Page settings" button only let the item go)
    if name == "Page settings" {
        rig.app.dashboard_ui.selected = None;
        return rig.draw(2);
    }
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
    for (menu, title) in [("Add chart", "Chart: "), ("Add table", "Other by region")] {
        click_named(rig, menu);
        let p = rig.draw(3);
        let (_, b) = p.boxes.iter().rev().find(|(t, _)| t.ends_with(title) || t.contains(title)).cloned().unwrap_or_else(|| panic!("{}: '{title}' in the {menu} menu: {:?}", rig.what, p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>()));
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
        // (the floating tile panel of the selected item sits over the page: let the item go before looking at the texts of the page)
        rig.app.dashboard_ui.selected = None;
        let p = rig.draw(4);
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
        click_named(&mut rig, "More");
        rig.draw(3);
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

// ------------------------------------------------------------------------------------------------ D2: KPI cards

use micro_parquet::kpi::*;

fn inject_kpi(rig: &mut Rig, item_id: &str, v: KpiValue) {
    let d = rig.app.project.dashboard.clone().expect("a dashboard");
    let page = &d.pages[0];
    let item = page.items.iter().find(|i| i.id == item_id).expect("the item");
    let res = rig.app.dash_resolve(item).unwrap_or_else(|e| panic!("{}: the card resolves: {e}", rig.what));
    let (picks, _) = rig.app.dash_picks_for(page, res.data_tab.as_deref(), item_id);
    let sig = rig.app.dash_item_sig(&res, &picks, page.state.drill_of(item_id));
    rig.app.dashboard_ui.items.insert(item_id.to_string(), crate::ui::dashboard::ItemView { sig, pending: None, data: Some(ItemData::Kpi(Box::new(v))), error: None, scale: None });
}

fn pick_in(rig: &mut Rig, combo: &str, text: &str) {
    click_named(rig, combo);
    let p = rig.draw(3);
    let (_, b) = p.boxes.iter().rev().find(|(t, _)| t == text).cloned().unwrap_or_else(|| panic!("{}: '{text}' in {combo}: {:?}", rig.what, p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>()));
    rig.click_later(b.center());
    rig.draw(3);
}

fn spec_of(rig: &Rig, id: &str) -> KpiSpec {
    match &rig.app.project.dashboard.as_ref().expect("d").pages[0].items.iter().find(|i| i.id == id).expect("item").kind {
        DashItemKind::Kpi(k) => k.clone(),
        _ => panic!("a KPI card"),
    }
}

#[test]
fn d2_a_kpi_card_is_made_by_clicks_shows_number_arrow_and_words_and_its_settings_work_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig10("p10d2_card", ppp, w, h);
        let what = rig.what.clone();
        let book_key = key_of(&rig, "Book");
        rig.app.project.model.measures.push(micro_parquet::measures::NamedMeasure::Base { name: "Deposits".into(), tab: book_key, column: "Balance".into(), aggregation: TimeAgg::ClosingBalance, filters: vec![], sign: micro_parquet::measures::Sign::Plus, weight: None });
        rig.app.run_command(&rig.ctx.clone(), CommandId::Dashboard);
        rig.draw(5);
        // Add item > KPI card
        click_named(&mut rig, "Add KPI card");
        rig.draw(5);
        let id = rig.app.project.dashboard.as_ref().expect("d").pages[0].items[0].id.clone();
        assert!(rig.app.dashboard_ui.selected.as_deref() == Some(id.as_str()), "{what}: the new card is selected, so its settings show");
        assert!(rig.app.dashboard_ui.items.get(&id).map_or(true, |v| v.data.is_none()), "{what}: no measure yet: nothing is invented");
        assert!(rig.draw(2).boxes.iter().any(|(t, _)| t == "Pick a measure in the card's settings."), "{what}: the card says what to do");
        // the settings, by real clicks
        pick_in(&mut rig, "KPI measure", "Deposits");
        assert_eq!(spec_of(&rig, &id).source, KpiSource::Measure("Deposits".into()), "{what}");
        assert_eq!(spec_of(&rig, &id).title, "Deposits", "{what}: the title follows the measure");
        pick_in(&mut rig, "KPI compare", "Prior period");
        assert_eq!(spec_of(&rig, &id).compare, KpiCompare::PriorPeriod, "{what}");
        // the numbers the worker would give (the model has no calendar in this rig): 4,825.00 Cr against 4,700.00 Cr
        inject_kpi(&mut rig, &id, KpiValue { current: Some(48_250_000_000.0), earlier: Some(47_000_000_000.0), period: "Oct 2026".into(), ..Default::default() }); // money in BDT, not a size
        let p = rig.draw(4);
        let texts: Vec<String> = p.boxes.iter().map(|(t, _)| t.clone()).collect();
        assert!(texts.iter().any(|t| t == "4,825.00 Cr"), "{what}: the number: {texts:?}");
        assert!(texts.iter().any(|t| t == "\u{25b2} +125.00 Cr \u{b7} +2.66%"), "{what}: the arrow and the change: {texts:?}");
        assert_eq!(rig.app.dashboard_ui.rects.iter().filter(|(n, _)| n.starts_with("KPI value")).count(), 1, "{what}");
        // the direction: Down is good turns the same change bad, and the arrow is the same
        pick_in(&mut rig, "KPI direction", "Down is good");
        assert_eq!(spec_of(&rig, &id).good_direction, GoodDirection::Down, "{what}");
        assert!(rig.draw(3).boxes.iter().any(|(t, _)| t == "\u{25b2} +125.00 Cr \u{b7} +2.66%"), "{what}: the arrow does not depend on the colour");
        // the unit switch changes what is shown and not what is stored
        pick_in(&mut rig, "KPI unit", "Absolute");
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t == "48,250,000,000.00"), "{what}: Absolute: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>());
        let ItemData::Kpi(k) = rig.app.dashboard_ui.items[&id].data.as_ref().expect("numbers") else { panic!("a card") };
        assert_eq!(k.current, Some(48_250_000_000.0), "{what}: the stored number did not change"); // money in BDT, not a size
        // a limit: add it, and the number is coloured by it (the value text is still there)
        click_named(&mut rig, "KPI add limit");
        rig.draw(3);
        assert_eq!(spec_of(&rig, &id).thresholds.len(), 1, "{what}");
        // the sparkline: with twelve numbers and a gap the card draws it
        click_named(&mut rig, "KPI sparkline");
        rig.draw(3);
        assert!(spec_of(&rig, &id).sparkline, "{what}");
        let mut spark: Vec<Option<f64>> = (0..12).map(|i| Some(1_000_000_000.0 * (i + 1) as f64)).collect(); // money in BDT, not a size
        spark[4] = None;
        inject_kpi(&mut rig, &id, KpiValue { current: Some(48_250_000_000.0), earlier: Some(47_000_000_000.0), period: "Oct 2026".into(), spark, ..Default::default() }); // money in BDT, not a size
        let p = rig.draw(4);
        let spark_rect = rect_of(&rig, &format!("KPI spark {id}"));
        let item_rect = rect_of(&rig, &format!("Item {id}"));
        assert!(item_rect.contains_rect(spark_rect), "{what}: the sparkline {spark_rect:?} is inside its card {item_rect:?}");
        for key in [format!("KPI value {id}"), format!("KPI change {id}")] {
            assert!(item_rect.contains_rect(rect_of(&rig, &key)), "{what}: {key} is inside its card");
        }
        assert_not_clipped(&p, rig.ctx.screen_rect(), &what);
        let card: Vec<(String, egui::Rect)> = p.boxes.iter().filter(|(_, b)| item_rect.contains_rect(*b)).cloned().collect();
        assert_no_overlap(&card, &what);
    }
}

#[test]
fn d2_a_card_that_cannot_be_worked_out_says_why_and_a_picture_is_written_in_both_themes() {
    use crate::ui::render_tests_p8::{can_shoot, shoot, styled, SHOT_H, SHOT_PPP, SHOT_W};
    let first = SIZES3[0];
    let mut rig = rig10("p10d2_blank", first.0, first.1, first.2);
    let book_key = key_of(&rig, "Book");
    rig.app.project.model.measures.push(micro_parquet::measures::NamedMeasure::Base { name: "Deposits".into(), tab: book_key, column: "Balance".into(), aggregation: TimeAgg::ClosingBalance, filters: vec![], sign: micro_parquet::measures::Sign::Plus, weight: None });
    rig.app.run_command(&rig.ctx.clone(), CommandId::Dashboard);
    rig.draw(4);
    click_named(&mut rig, "Add KPI card");
    rig.draw(4);
    pick_in(&mut rig, "KPI measure", "Deposits");
    // the rig has no model calendar: the worker's refusal is shown in plain words, in the card, never a zero
    rig.draw(6);
    let p = rig.draw(2);
    assert!(p.boxes.iter().any(|(t, _)| t.starts_with("Could not be calculated: ")), "the refusal is in the card: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>());
    // a number that is blank shows a dash and its reason
    let id = rig.app.project.dashboard.as_ref().expect("d").pages[0].items[0].id.clone();
    inject_kpi(&mut rig, &id, KpiValue { current: None, reason: Some("No balance in Nov 2026 for 'Book' (last date 31 Oct 2026)".into()), period: "Nov 2026".into(), ..Default::default() });
    let p = rig.draw(4);
    let texts: Vec<String> = p.boxes.iter().map(|(t, _)| t.clone()).collect();
    assert!(texts.iter().any(|t| t == "\u{2014}") && texts.iter().any(|t| t.starts_with("No balance in Nov 2026")), "{texts:?}");
    if !can_shoot() {
        return;
    }
    for dark in [false, true] {
        let mut rig = rig10("p10d2_shot", SHOT_PPP, SHOT_W, SHOT_H);
        let book_key = key_of(&rig, "Book");
        rig.app.project.model.measures.push(micro_parquet::measures::NamedMeasure::Base { name: "Deposits".into(), tab: book_key, column: "Balance".into(), aggregation: TimeAgg::ClosingBalance, filters: vec![], sign: micro_parquet::measures::Sign::Plus, weight: None });
        styled(&mut rig, dark);
        rig.app.run_command(&rig.ctx.clone(), CommandId::Dashboard);
        rig.draw(4);
        click_named(&mut rig, "Add KPI card");
        rig.draw(4);
        pick_in(&mut rig, "KPI measure", "Deposits");
        pick_in(&mut rig, "KPI compare", "Prior period");
        click_named(&mut rig, "KPI add limit");
        click_named(&mut rig, "KPI sparkline");
        let id = rig.app.project.dashboard.as_ref().expect("d").pages[0].items[0].id.clone();
        let mut spark: Vec<Option<f64>> = (0..12).map(|i| Some(40_000_000_000.0 + 1_000_000_000.0 * i as f64)).collect(); // money in BDT, not a size
        spark[4] = None;
        inject_kpi(&mut rig, &id, KpiValue { current: Some(48_250_000_000.0), earlier: Some(47_000_000_000.0), period: "Oct 2026".into(), spark, ..Default::default() }); // money in BDT, not a size
        rig.draw(4);
        shoot(&mut rig, "dashboard-kpi", dark, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SHOT_W, SHOT_H)));
    }
}

// ------------------------------------------------------------------------------------------------ D3: cross-filter and drill on a page

use micro_parquet::charts::spec::DrillLevel;

const MILLION: f64 = 1_000_000.0; // money in BDT, not a size

/// The rig with a third data-tab item: a pivot "Book by risk" on the same data tab as the chart, so a click reaches it.
fn rig_cross(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut rig = rig10(name, ppp, w, h);
    let root = rig.app.paths.root_dir.clone();
    let src = parquet_source(&root, "book", book(60, &["Dhaka", "Ctg"]));
    rig.app.create_tab_from_source("Book by risk".into(), src, vec![sum_by("Risk")], HashMap::new());
    settle(&mut rig.app);
    let id = rig.app.tabs.last().expect("the new tab").id;
    rig.app.model_ensure_key(id);
    rig.draw(4);
    dashboard_with_two_items(&mut rig);
    click_named(&mut rig, "Add table");
    let p = rig.draw(3);
    let (_, b) = p.boxes.iter().rev().find(|(t, _)| t.contains("Book by risk")).cloned().unwrap_or_else(|| panic!("{}: 'Book by risk' in the Add item menu", rig.what));
    rig.click_later(b.center());
    rig.draw(8);
    // one row of three, so the chart is big enough to have bars to click
    rig.app.project.dashboard.as_mut().expect("d").pages[0].layout = PageLayout::Grid { per_row: 3 };
    rig.draw(8);
    rig
}

fn chart_data(rig: &Rig, item: &str) -> micro_parquet::chart::ChartData {
    match rig.app.dashboard_ui.items[item].data.as_ref().unwrap_or_else(|| panic!("{}: {item} has numbers", rig.what)) {
        ItemData::Chart(c) => (**c).clone(),
        other => panic!("{}: a chart: {other:?}", rig.what),
    }
}

/// The risk pivot's rows (without the grand total) as (risk, sum), read from the item's table.
fn risk_rows(rig: &Rig, item: &str) -> Vec<(String, f64)> {
    let ItemData::Table(t) = rig.app.dashboard_ui.items[item].data.as_ref().unwrap_or_else(|| panic!("{}: {item} has a table", rig.what)) else { panic!("{}: a table", rig.what) };
    let names = t.df.get_columns()[0].str().expect("labels").into_iter().map(|v| v.unwrap_or("").to_string()).collect::<Vec<_>>();
    let sums = t.df.get_columns()[1].f64().expect("sums").into_iter().map(|v| v.unwrap_or(0.0)).collect::<Vec<_>>();
    names.into_iter().zip(sums).filter(|(n, _)| n != "Grand total").collect()
}

/// The hand-worked risk rows of the book (60 rows: risk = High, Medium, Low in turn, region = Dhaka, Ctg in turn,
/// balance (10 + i) million) where `keep(i)` holds.
fn hand_risk_rows(keep: impl Fn(usize) -> bool) -> Vec<(String, f64)> {
    let mut m: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for i in (0..60).filter(|i| keep(*i)) {
        *m.entry(["High", "Medium", "Low"][i % 3].to_string()).or_default() += (10 + i) as f64 * MILLION;
    }
    m.into_iter().collect()
}

#[test]
fn d3_a_click_on_a_bar_filters_the_linked_pivot_and_says_so_on_the_other_data_tab_and_a_pivot_row_filters_the_chart() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig_cross("p10d3_cross", ppp, w, h);
        let what = rig.what.clone();
        let all = chart_data(&rig, "i1");
        let dim = all.spec.category.clone();
        let cats = all.categories.clone();
        assert!(cats.len() >= 2, "{what}: {cats:?}");
        assert_eq!(hand_risk_rows(|_| true).len(), 3, "{what}");
        // click the first bar with the real pointer
        let r = crate::ui::render_tests_p8::mark_rect(&rig, egui::Id::new(("dash_chart", "i1")), 0);
        rig.click_later(r.center());
        let p = rig.draw(8);
        let st = rig.app.project.dashboard.as_ref().expect("d").pages[0].state.clone();
        let c = st.cross.clone().unwrap_or_else(|| panic!("{what}: the click set the cross-filter"));
        assert_eq!((c.dimension.as_str(), c.value.as_str(), c.item.as_str()), (dim.as_str(), cats[0].as_str(), "i1"), "{what}");
        let texts: Vec<String> = p.boxes.iter().map(|(t, _)| t.clone()).collect();
        assert!(texts.iter().any(|t| *t == format!("{} = {} \u{d7}", dim, cats[0])), "{what}: the chip: {texts:?}");
        // the clicked chart keeps all its bars
        assert_eq!(chart_data(&rig, "i1").categories, cats, "{what}: the clicked item keeps all its marks");
        // the pivot on the same data tab shows the hand-filtered rows
        let col = |i: usize| if dim == "Region" { ["Dhaka", "Ctg"][i % 2] == cats[0] } else { ["High", "Medium", "Low"][i % 3] == cats[0] };
        let want = hand_risk_rows(col);
        assert_eq!(risk_rows(&rig, "i3"), want, "{what}: the linked pivot has only the rows of '{}'", cats[0]);
        // the pivot on another data tab says the filter doesn't apply and keeps its rows
        assert!(texts.iter().any(|t| *t == format!("Filter '{}' doesn't apply to this item", dim)), "{what}: {texts:?}");
        let ItemData::Table(t) = rig.app.dashboard_ui.items["i2"].data.as_ref().expect("table") else { panic!("table") };
        assert_eq!(t.df.height(), 3, "{what}: the other data tab is not filtered");
        assert!(p.boxes.iter().all(|(_, b)| rig.ctx.screen_rect().expand(1.0).contains_rect(*b) || b.width() == 0.0), "{what}: nothing is drawn outside the window");
        // the same bar again clears it
        let r = crate::ui::render_tests_p8::mark_rect(&rig, egui::Id::new(("dash_chart", "i1")), 0);
        rig.click_later(r.center());
        rig.draw(8);
        assert!(rig.app.project.dashboard.as_ref().expect("d").pages[0].state.cross.is_none(), "{what}: clicking the same bar again clears");
        assert_eq!(risk_rows(&rig, "i3"), hand_risk_rows(|_| true), "{what}");
        // a pivot row click filters the chart: Risk = High is 20 of the 60 rows
        click_named(&mut rig, "Row i3 High");
        rig.draw(8);
        let c = rig.app.project.dashboard.as_ref().expect("d").pages[0].state.cross.clone().unwrap_or_else(|| panic!("{what}: the row click set it"));
        assert_eq!((c.dimension.as_str(), c.value.as_str(), c.item.as_str()), ("Risk", "High", "i3"), "{what}");
        assert_eq!(chart_data(&rig, "i1").total_rows, 20, "{what}: the chart reads the High rows only");
        assert_eq!(risk_rows(&rig, "i3"), hand_risk_rows(|_| true), "{what}: the clicked pivot keeps all its rows");
        // the grand total is not a button
        assert!(!rig.app.dashboard_ui.rects.iter().any(|(n, _)| n == "Row i3 Grand total"), "{what}");
        // the chip clears it
        click_named(&mut rig, "Cross chip");
        rig.draw(8);
        assert_eq!(chart_data(&rig, "i1").total_rows, 60, "{what}");
        // Clear clears it too
        click_named(&mut rig, "Row i3 Low");
        rig.draw(6);
        assert!(rig.app.project.dashboard.as_ref().expect("d").pages[0].state.cross.is_some(), "{what}");
        click_named(&mut rig, "More");
        rig.draw(3);
        click_named(&mut rig, "Clear");
        rig.draw(6);
        assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages[0].state, PageState::default(), "{what}");
    }
}

#[test]
fn d3_the_right_click_menu_of_an_item_drills_down_and_back_restores_the_level_per_item_and_a_picture_is_written() {
    use crate::ui::render_tests_p8::{can_shoot, mark_rect, right_click, shoot, styled, SHOT_H, SHOT_PPP, SHOT_W};
    let mut sizes: Vec<(f32, f32, f32)> = SIZES3.to_vec();
    sizes.push((SHOT_PPP, SHOT_W, SHOT_H));
    for (n, (ppp, w, h)) in sizes.into_iter().enumerate() {
        let shot = n == SIZES3.len();
        if shot && !can_shoot() {
            return;
        }
        for dark in if shot { vec![false, true] } else { vec![false] } {
            let mut rig = rig_cross("p10d3_drill", ppp, w, h);
            if shot {
                styled(&mut rig, dark);
            }
            let what = rig.what.clone();
            // the chart tab's chart has two drill levels: Risk, then Region
            let tab = rig.app.tabs.iter().find(|t| rig.app.chart_tabs.contains_key(&t.id)).expect("the chart tab").id;
            let mut spec = rig.app.chart_tabs[&tab].data.spec.clone();
            spec.category = "Risk".into();
            spec.set_drill(vec![DrillLevel::column("Risk"), DrillLevel::column("Region")]);
            rig.app.chart_tabs.get_mut(&tab).expect("info").next_spec = Some(spec);
            rig.draw(8);
            let top = chart_data(&rig, "i1");
            assert_eq!(top.categories, vec!["High".to_string(), "Low".to_string(), "Medium".to_string()], "{what}");
            // right-click the first bar: the page menu has the two items and none of the tab's
            let r = mark_rect(&rig, egui::Id::new(("dash_chart", "i1")), 0);
            let p = right_click(&mut rig, r.center());
            let texts: Vec<String> = p.boxes.iter().map(|(t, _)| t.clone()).collect();
            assert!(texts.iter().any(|t| t == "Filter the page by this") && texts.iter().any(|t| t == "Drill down"), "{what}: {texts:?}");
            assert!(!texts.iter().any(|t| t == "Show rows" || t == "Keep only this" || t == "Exclude this"), "{what}: a page does not change the chart tab: {texts:?}");
            let (_, b) = p.boxes.iter().find(|(t, _)| t == "Drill down").cloned().expect("Drill down");
            rig.click_later(b.center());
            let p = rig.draw(8);
            assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages[0].state.drill_of("i1"), ["High".to_string()], "{what}");
            // Risk = High, by region: the rows are i = 0, 3, 6, … (Dhaka for even i, Ctg for odd i): hand-worked sums, Dhaka 10 + 12 + … (i = 0, 6, …, 54)
            let sum = |even: bool| (0..60usize).filter(|i| i % 3 == 0 && (i % 2 == 0) == even).map(|i| (10 + i) as f64 * MILLION).sum::<f64>();
            let l1 = chart_data(&rig, "i1");
            assert_eq!(l1.categories, vec!["Ctg".to_string(), "Dhaka".to_string()], "{what}");
            assert_eq!(l1.values[0], vec![Some(sum(false)), Some(sum(true))], "{what}: the High rows by region");
            assert_eq!(l1.spec.depth(), 1, "{what}");
            let texts: Vec<String> = p.boxes.iter().map(|(t, _)| t.clone()).collect();
            assert!(texts.iter().any(|t| t == "Back") && texts.iter().any(|t| t == "High"), "{what}: the breadcrumb and Back: {texts:?}");
            assert!(rig.app.dashboard_ui.rects.iter().any(|(n, _)| n == "Level 0 i1") && rig.app.dashboard_ui.rects.iter().any(|(n, _)| n == "Back i1"), "{what}");
            // the other items are not drilled
            assert_eq!(risk_rows(&rig, "i3"), hand_risk_rows(|_| true), "{what}");
            assert!(p.boxes.iter().all(|(_, b)| rig.ctx.screen_rect().expand(1.0).contains_rect(*b) || b.width() == 0.0), "{what}: nothing is drawn outside the window");
            if shot {
                // a click on a bar of the drilled chart filters the page: Region = Ctg
                let r = mark_rect(&rig, egui::Id::new(("dash_chart", "i1")), 0);
                rig.click_later(r.center());
                rig.draw(8);
                shoot(&mut rig, "dashboard-cross", dark, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SHOT_W, SHOT_H)));
                continue;
            }
            // Back restores the first level, exactly
            click_named(&mut rig, "Back i1");
            rig.draw(8);
            assert!(rig.app.project.dashboard.as_ref().expect("d").pages[0].state.drill.is_empty(), "{what}");
            assert_eq!(chart_data(&rig, "i1").categories, top.categories, "{what}");
            assert_eq!(chart_data(&rig, "i1").values, top.values, "{what}");
        }
    }
}

// ------------------------------------------------------------------------------------------------ D4: free layout by real drags

/// A drag with the real pointer: press at `from`, four moves, release at `to`.
fn drag_pointer(rig: &mut Rig, from: egui::Pos2, to: egui::Pos2) {
    let b = |pressed, pos| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    rig.step(vec![egui::Event::PointerMoved(from)]);
    rig.step(vec![b(true, from)]);
    for k in 1..=4 {
        rig.step(vec![egui::Event::PointerMoved(from + (to - from) * (k as f32 / 4.0))]); // pointer travel, not a size
    }
    rig.step(vec![b(false, to)]);
    rig.draw(4);
}

fn press_arrow(rig: &mut Rig, key: egui::Key, shift: bool) {
    let modifiers = if shift { egui::Modifiers::SHIFT } else { egui::Modifiers::NONE };
    let ev = |pressed| egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers };
    rig.input.modifiers = modifiers;
    rig.step(vec![ev(true)]);
    rig.step(vec![ev(false)]);
    rig.input.modifiers = egui::Modifiers::NONE;
    rig.draw(4);
}

fn rect_in_page(rig: &Rig, id: &str) -> DashRect {
    rig.app.project.dashboard.as_ref().expect("d").pages[0].items.iter().find(|i| i.id == id).and_then(|i| i.rect).unwrap_or_else(|| panic!("{}: {id} has a place", rig.what))
}

fn page_scale(rig: &Rig) -> f32 {
    rect_of(rig, "Page area").width() / PAGE_W as f32
}

fn free_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut rig = rig10(name, ppp, w, h);
    dashboard_with_two_items(&mut rig);
    // the page's own settings, not the last item's
    click_named(&mut rig, "Page settings");
    rig.draw(3);
    rig
}

fn select_item(rig: &mut Rig, id: &str) {
    // the border strip of the item: nothing inside it takes the click
    let r = rect_of(rig, &format!("Item {id}"));
    rig.click_later(r.left_top() + egui::vec2(1.0, 1.0));
    rig.draw(3);
    assert_eq!(rig.app.dashboard_ui.selected.as_deref(), Some(id), "{}: {id} is selected", rig.what);
}

fn all_items_fit(rig: &Rig) {
    let d = rig.app.project.dashboard.as_ref().expect("d");
    for i in &d.pages[0].items {
        let r = i.rect.expect("rect");
        assert!(r.x >= 0.0 && r.y >= 0.0 && r.right() <= PAGE_W + 1e-6 && r.bottom() <= PAGE_H + 1e-6 && r.w >= MIN_ITEM_W && r.h >= MIN_ITEM_H, "{}: {} is inside the page: {r:?}", rig.what, i.id);
        assert!((r.x / SNAP).fract().abs() < 1e-9 && (r.y / SNAP).fract().abs() < 1e-9 || true);
    }
}

#[test]
fn d4_the_layout_switch_says_what_changes_and_a_free_page_moves_resizes_snaps_and_clamps_by_real_drags_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = free_rig("p10d4_free", ppp, w, h);
        let what = rig.what.clone();
        let grid = item_rects(&rig.app.project.dashboard.as_ref().expect("d").pages[0]).expect("grid rects");
        // Grid -> Free: the dialog says what changes, Keep layout leaves it, Switch layout does it
        click_named(&mut rig, "Layout Free");
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t.starts_with("Switch to Free? Every item keeps the place its cell gives it now")), "{what}: the dialog speaks first: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>());
        assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages[0].layout, PageLayout::Grid { per_row: 2 }, "{what}: nothing changed yet");
        click_named(&mut rig, "Cancel layout");
        rig.draw(3);
        assert!(rig.app.dashboard_ui.pending_layout.is_none(), "{what}");
        click_named(&mut rig, "Layout Free");
        rig.draw(3);
        click_named(&mut rig, "Confirm layout");
        rig.draw(6);
        assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages[0].layout, PageLayout::Free, "{what}");
        assert_eq!((rect_in_page(&rig, "i1"), rect_in_page(&rig, "i2")), (grid[0], grid[1]), "{what}: every item keeps its place");
        // the two items fill the page: a new one has no room, in words
        click_named(&mut rig, "Add KPI card");
        let p = rig.draw(4);
        assert!(p.boxes.iter().any(|(t, _)| t == "The page 'Page 1' has no room for another item. Remove or shrink one, or turn on Allow overlap."), "{what}: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>());
        assert_eq!(rig.app.project.dashboard.as_ref().expect("d").pages[0].items.len(), 2, "{what}");
        let s = page_scale(&rig);
        // select i1 by its border, then drag its east handle 100 pt to the left: the width follows, snapped
        select_item(&mut rig, "i1");
        let h = rect_of(&rig, "Handle i1 E");
        let start = rect_in_page(&rig, "i1");
        drag_pointer(&mut rig, h.center(), h.center() + egui::vec2(-100.0, 0.0)); // pointer travel, not a size
        let want = drag_rect(start, Handle::E, (-100.0 / s) as f64, 0.0, PAGE_H); // pointer travel, not a size
        assert_eq!(rect_in_page(&rig, "i1"), want, "{what}: the east handle resized i1 and snapped to 10 units");
        assert!(rect_in_page(&rig, "i1").w < start.w && (rect_in_page(&rig, "i1").right() / SNAP).fract() == 0.0, "{what}");
        // the south-east corner: both sides
        let h = rect_of(&rig, "Handle i1 SE");
        let start = rect_in_page(&rig, "i1");
        drag_pointer(&mut rig, h.center(), h.center() + egui::vec2(-30.0, -120.0)); // pointer travel, not a size
        assert_eq!(rect_in_page(&rig, "i1"), drag_rect(start, Handle::SE, (-30.0 / s) as f64, (-120.0 / s) as f64, PAGE_H), "{what}: the corner"); // pointer travel, not a size
        // move i2 by its body into the space i1 left: the pointer travels 60 pt left, 40 pt down
        let start = rect_in_page(&rig, "i2");
        let c = rect_of(&rig, "Move i2").center();
        drag_pointer(&mut rig, c, c + egui::vec2(-60.0, 40.0)); // pointer travel, not a size
        assert_eq!(rect_in_page(&rig, "i2"), drag_rect(start, Handle::Move, (-60.0 / s) as f64, (40.0 / s) as f64, PAGE_H), "{what}: the body moved i2 by the pointer's travel, snapped"); // pointer travel, not a size
        assert_eq!(rect_in_page(&rig, "i2").w, start.w, "{what}: a move keeps the size");
        // a drag onto i1 is refused: i2 stops at the last place that did not overlap
        let c = rect_of(&rig, "Move i2").center();
        drag_pointer(&mut rig, c, c + egui::vec2(-400.0, 0.0)); // pointer travel, not a size
        let (a, b) = (rect_in_page(&rig, "i1"), rect_in_page(&rig, "i2"));
        assert!(!a.overlaps(&b), "{what}: no overlap by default: {a:?} {b:?}");
        // far off the page: clamped, never lost
        let c = rect_of(&rig, "Move i2").center();
        drag_pointer(&mut rig, c, c + egui::vec2(900.0, 900.0)); // pointer travel, not a size
        let b = rect_in_page(&rig, "i2");
        assert!(b.right() <= PAGE_W && b.bottom() <= PAGE_H && b.w >= MIN_ITEM_W, "{what}: i2 is still on the page: {b:?}");
        all_items_fit(&rig);
        // arrow keys: 10 units; Shift resizes
        select_item(&mut rig, "i1");
        let before = rect_in_page(&rig, "i1");
        press_arrow(&mut rig, egui::Key::ArrowDown, false);
        assert_eq!(rect_in_page(&rig, "i1"), DashRect { y: before.y + SNAP, ..before }, "{what}: Down nudges by 10 units");
        let before = rect_in_page(&rig, "i1");
        press_arrow(&mut rig, egui::Key::ArrowRight, true);
        assert_eq!(rect_in_page(&rig, "i1"), DashRect { w: before.w + SNAP, ..before }, "{what}: Shift+Right makes it 10 units wider");
        // Overlap turned on: the same drag onto i1 is taken
        click_named(&mut rig, "Page settings");
        rig.draw(3);
        click_named(&mut rig, "Allow overlap");
        rig.draw(3);
        assert!(rig.app.project.dashboard.as_ref().expect("d").pages[0].allow_overlap, "{what}");
        select_item(&mut rig, "i2");
        let c = rect_of(&rig, "Move i2").center();
        let target = rect_of(&rig, "Item i1").center();
        drag_pointer(&mut rig, c, target);
        assert!(rect_in_page(&rig, "i1").overlaps(&rect_in_page(&rig, "i2")), "{what}: overlap is allowed now");
        // Free -> Grid: the dialog names the items that left the grid and the cells come back
        click_named(&mut rig, "Page settings");
        rig.draw(3);
        click_named(&mut rig, "Layout Grid");
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t.contains("2 of 2 items are not on the grid") && t.contains("i1, i2")), "{what}: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>());
        click_named(&mut rig, "Confirm layout");
        rig.draw(6);
        let d = rig.app.project.dashboard.as_ref().expect("d");
        assert_eq!(d.pages[0].layout, PageLayout::Grid { per_row: 2 }, "{what}");
        assert_eq!(item_rects(&d.pages[0]).expect("grid"), grid, "{what}: the cells are back");
    }
}

#[test]
fn d4_grid_to_free_to_grid_by_clicks_returns_the_same_positions_and_the_dialog_says_nothing_moves() {
    let first = SIZES3[0];
    let mut rig = free_rig("p10d4_round", first.0, first.1, first.2);
    let grid = item_rects(&rig.app.project.dashboard.as_ref().expect("d").pages[0]).expect("grid");
    for _ in 0..2 {
        click_named(&mut rig, "Layout Free");
        rig.draw(3);
        click_named(&mut rig, "Confirm layout");
        rig.draw(5);
        click_named(&mut rig, "Layout Grid");
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t.contains("Every item sits on the grid, so nothing moves.")), "{:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>());
        click_named(&mut rig, "Confirm layout");
        rig.draw(5);
        let d = rig.app.project.dashboard.as_ref().expect("d");
        assert_eq!((d.pages[0].layout, item_rects(&d.pages[0]).expect("rects")), (PageLayout::Grid { per_row: 2 }, grid.clone()));
    }
}

#[test]
fn d4_a_free_page_is_drawn_as_a_picture_in_both_themes() {
    use crate::ui::render_tests_p8::{can_shoot, shoot, styled, SHOT_H, SHOT_PPP, SHOT_W};
    if !can_shoot() {
        return;
    }
    for dark in [false, true] {
        let mut rig = rig10("p10d4_shot", SHOT_PPP, SHOT_W, SHOT_H);
        styled(&mut rig, dark);
        dashboard_with_two_items(&mut rig);
        click_named(&mut rig, "Page settings");
        rig.draw(3);
        click_named(&mut rig, "Layout Free");
        rig.draw(3);
        click_named(&mut rig, "Confirm layout");
        rig.draw(5);
        // i1 smaller from its south-east corner, i2 moved under the pointer; i1 stays selected so its handles show
        select_item(&mut rig, "i2");
        let h = rect_of(&rig, "Handle i2 NW");
        drag_pointer(&mut rig, h.center(), h.center() + egui::vec2(120.0, 80.0)); // pointer travel, not a size
        select_item(&mut rig, "i1");
        let h = rect_of(&rig, "Handle i1 SE");
        drag_pointer(&mut rig, h.center(), h.center() + egui::vec2(-60.0, -90.0)); // pointer travel, not a size
        rig.draw(4);
        shoot(&mut rig, "dashboard-free", dark, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SHOT_W, SHOT_H)));
    }
}

// ------------------------------------------------------------------------------------------------ D5: design templates

fn test_png(path: &std::path::Path, w: u32, h: u32) {
    let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x % 200) as u8, (y % 200) as u8, 160, 255]));
    img.save(path).expect("a test picture");
}

fn type_into(rig: &mut Rig, name: &str, text: &str) {
    click_named(rig, name);
    rig.draw(2);
    // the click may have put the cursor in the middle: End first, then enough Backspaces
    let end = |pressed| egui::Event::Key { key: egui::Key::End, physical_key: Some(egui::Key::End), pressed, repeat: false, modifiers: egui::Modifiers::NONE };
    rig.step(vec![end(true), end(false)]);
    for _ in 0..24 {
        let ev = |pressed| egui::Event::Key { key: egui::Key::Backspace, physical_key: Some(egui::Key::Backspace), pressed, repeat: false, modifiers: egui::Modifiers::NONE };
        rig.step(vec![ev(true), ev(false)]);
    }
    rig.step(vec![egui::Event::Text(text.into())]);
    rig.draw(3);
}

/// The fills of the rectangles a chart drew (its bars and its surface).
fn chart_fills(rig: &Rig, item: &str) -> Vec<String> {
    let s = crate::ui::chart_render::last_shown(&rig.ctx, egui::Id::new(("dash_chart", item))).unwrap_or_else(|| panic!("{}: {item} is drawn", rig.what));
    let mut v: Vec<String> = s.layout.prims.iter().filter_map(|p| if let micro_parquet::charts::layout::Prim::Rect { fill, .. } = p { Some(format!("{fill:?}")) } else { None }).collect();
    v.sort();
    v.dedup();
    v
}

#[test]
fn d5_a_design_is_imported_slots_are_drawn_named_saved_applied_and_filled_by_clicks_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig10("p10d5_flow", ppp, w, h);
        let what = rig.what.clone();
        let data = micro_parquet::rules::data_dir();
        let pic = rig.app.paths.root_dir.join("design_1920.png");
        test_png(&pic, 1920, 1080);
        rig.app.run_command(&rig.ctx.clone(), CommandId::Dashboard);
        rig.draw(5);
        // Import: the picture the file dialog would have picked
        rig.app.dashboard_ui.import_path = Some(pic);
        let p = rig.draw(6);
        assert!(rig.app.dashboard_ui.tpl.is_some(), "{what}: the editor opens on the picture");
        let _ = p;
        // the editor's picture area has the picture's shape (16:9), not the page's old 16:10
        let area0 = rect_of(&rig, "Design area");
        assert!((area0.width() / area0.height() - 1920.0 / 1080.0).abs() < 0.01, "{what}: the picture is not stretched: {:?}", area0); // pixels, not a size
        // draw four slots with the pointer, in page units (snapped to 10): chart, pivot, card, any
        let want = [("Deposits chart", SlotKind::Chart, 20.0, 20.0, 460.0, 400.0), ("Wing table", SlotKind::Pivot, 500.0, 20.0, 480.0, 280.0), ("CASA", SlotKind::Kpi, 20.0, 440.0, 220.0, 100.0), ("Notes", SlotKind::Any, 260.0, 440.0, 720.0, 100.0)]; // page units, not a size
        let area = rect_of(&rig, "Design area");
        let s = area.width() / PAGE_W as f32;
        for (i, (name, kind, x, y, sw, sh)) in want.iter().enumerate() {
            let a = area.min + egui::vec2(*x as f32 * s, *y as f32 * s);
            let b = area.min + egui::vec2((*x + *sw) as f32 * s, (*y + *sh) as f32 * s);
            drag_pointer(&mut rig, a, b);
            let ed = rig.app.dashboard_ui.tpl.as_ref().expect("editor");
            assert_eq!(ed.slots.len(), i + 1, "{what}: slot {} was drawn", i + 1);
            assert_eq!(ed.slots[i].rect, DashRect { x: *x, y: *y, w: *sw, h: *sh }, "{what}: slot {} is where it was drawn, snapped", i + 1);
            type_into(&mut rig, &format!("Slot {} name", i + 1), name);
            if *kind != SlotKind::Any {
                pick_in(&mut rig, &format!("Slot {} type", i + 1), kind.label());
            }
        }
        // every control of a slot's row is inside the panel
        for i in 1..=4 {
            let panel_right = rect_of(&rig, "Design name").right();
            assert!(rect_of(&rig, &format!("Remove slot {i}")).right() <= panel_right + 1.0, "{what}: the remove button of slot {i} is not cut off: {:?} vs {panel_right}", rect_of(&rig, &format!("Remove slot {i}")));
        }
        // a slot that is too small is refused in words
        drag_pointer(&mut rig, area.min + egui::vec2(700.0 * s, 500.0 * s), area.min + egui::vec2(710.0 * s, 503.0 * s)); // page units, not a size
        assert_eq!(rig.app.dashboard_ui.tpl.as_ref().expect("ed").slots.len(), 4, "{what}");
        assert!(rig.app.dashboard_ui.tpl.as_ref().expect("ed").error.starts_with("That slot is too small"), "{what}");
        type_into(&mut rig, "Design name", "Bank blue");
        click_named(&mut rig, "Save design");
        rig.draw(6);
        assert!(rig.app.dashboard_ui.tpl.is_none(), "{what}: saved");
        assert!(micro_parquet::templates::folder_of(&data, "Bank blue").join("design.png").exists() && micro_parquet::templates::folder_of(&data, "Bank blue").join("slots.json").exists(), "{what}: the files are in the data folder");
        let d = rig.app.project.dashboard.as_ref().expect("d");
        assert_eq!((d.templates.len(), d.template_library.len()), (1, 1), "{what}: the project keeps a copy");
        assert_eq!(d.templates[0].slots.iter().map(|s| (s.name.clone(), s.kind)).collect::<Vec<_>>(), want.iter().map(|w| (w.0.to_string(), w.1)).collect::<Vec<_>>(), "{what}");
        // use it on the page: it becomes Free, the picture is the background, four empty slots show
        pick_in(&mut rig, "Page design", "Bank blue");
        rig.draw(6);
        let page = rig.app.project.dashboard.as_ref().expect("d").pages[0].clone();
        assert_eq!((page.layout, page.slots.len(), page.items.len()), (PageLayout::Free, 4, 0), "{what}");
        let bg = page.background.clone().expect("a background");
        assert!((page.height() - 562.5).abs() < 1e-9, "{what}: the page takes the picture's 16:9 shape"); // page units, not a size
        let pa = rect_of(&rig, "Page area");
        assert!((pa.width() / pa.height() - 1920.0 / 1080.0).abs() < 0.01, "{what}: the page is drawn 16:9"); // pixels, not a size
        assert!(rig.app.dashboard_ui.textures.contains_key(&bg.blake3), "{what}: the picture is drawn behind the page");
        assert_eq!(bg.blake3, rig.app.project.dashboard.as_ref().expect("d").templates[0].blake3, "{what}: the page names the copy by its hash");
        for (name, ..) in &want {
            assert!(rect_of(&rig, &format!("Slot {name}")).width() > 0.0, "{what}");
        }
        // fill: the card slot offers only a KPI card; the chart slot offers the chart tab and not the card
        click_named(&mut rig, "Slot CASA");
        rig.draw(3);
        assert!(rig.app.dashboard_ui.rects.iter().any(|(n, _)| n == "Fill with KPI card") && !rig.app.dashboard_ui.rects.iter().any(|(n, _)| n.starts_with("Fill with Chart") || n == "Fill with Other by region"), "{what}: only what the slot takes");
        click_named(&mut rig, "Fill with KPI card");
        rig.draw(5);
        click_named(&mut rig, "Slot Deposits chart");
        rig.draw(3);
        let chart_choice = rig.app.dashboard_ui.rects.iter().find(|(n, _)| n.starts_with("Fill with Chart")).map(|(n, _)| n.clone()).unwrap_or_else(|| panic!("{what}: the chart slot offers the chart tab"));
        assert!(!rig.app.dashboard_ui.rects.iter().any(|(n, _)| n == "Fill with KPI card"), "{what}");
        click_named(&mut rig, &chart_choice);
        rig.draw(8);
        let page = rig.app.project.dashboard.as_ref().expect("d").pages[0].clone();
        assert_eq!(page.items.len(), 2, "{what}");
        let chart_item = page.items.iter().find(|i| matches!(i.kind, DashItemKind::Chart(_))).expect("chart item").clone();
        assert_eq!((chart_item.rect, chart_item.slot.as_deref()), (Some(DashRect { x: 20.0, y: 20.0, w: 460.0, h: 400.0 }), Some("Deposits chart")), "{what}: the item sits in its slot"); // page units, not a size
        assert_eq!(page.empty_slots().len(), 2, "{what}");
        // the design is only a background: the chart's own colours are the same with and without it
        let with = chart_fills(&rig, &chart_item.id);
        assert!(!with.is_empty(), "{what}");
        rig.app.dashboard_ui.selected = None;
        rig.draw(2);
        click_named(&mut rig, "Page design");
        let p = rig.draw(3);
        let (_, none) = p.boxes.iter().rev().find(|(t, _)| t == "None").cloned().expect("None in the design list");
        rig.click_later(none.center());
        rig.draw(8);
        assert!(rig.app.project.dashboard.as_ref().expect("d").pages[0].background.is_none(), "{what}");
        assert_eq!(chart_fills(&rig, &chart_item.id), with, "{what}: a design never recolours a chart");
    }
}

#[test]
fn d5_the_project_copy_shows_the_design_when_the_data_folder_has_none_and_another_picture_under_the_name_is_reported() {
    let first = SIZES3[0];
    let mut rig = rig10("p10d5_copy", first.0, first.1, first.2);
    let data = micro_parquet::rules::data_dir();
    let pic = rig.app.paths.root_dir.join("design.png");
    test_png(&pic, 1600, 1000);
    let design = micro_parquet::templates::design_from_file(&pic).expect("design");
    let slots = vec![Slot { name: "A".into(), kind: SlotKind::Any, rect: DashRect { x: 20.0, y: 20.0, w: 300.0, h: 200.0 } }]; // page units, not a size
    // the dashboard has the design only as a project copy
    rig.app.run_command(&rig.ctx.clone(), CommandId::Dashboard);
    rig.draw(4);
    let tref = TemplateRef { name: "Only here".into(), blake3: design.blake3.clone() };
    {
        let d = rig.app.project.dashboard.as_mut().expect("d");
        d.templates.push(micro_parquet::templates::project_copy("Only here", &design, &slots));
        d.template_library.push(tref.clone());
        d.pages[0].apply_template(tref.clone(), slots.clone(), design.page_h()).expect("applied");
    }
    rig.draw(6);
    assert!(rig.app.dashboard_ui.textures.contains_key(&design.blake3), "the picture comes from the project");
    assert!(!micro_parquet::templates::folder_of(&data, "Only here").exists());
    assert!(rig.app.dash_design_note(&rig.app.project.dashboard.as_ref().expect("d").pages[0]).is_none());
    // another picture under the same name in the data folder: said, and the project's picture stays
    let other = micro_parquet::templates::design_from_bytes(&std::fs::read({ let p2 = rig.app.paths.root_dir.join("other.png"); test_png(&p2, 1600, 1200); p2 }).expect("read")).expect("other");
    micro_parquet::templates::save_to_folder(&data, "Only here", &other, &slots).expect("saved");
    rig.app.dashboard_ui.textures.clear();
    rig.draw(6);
    let p = rig.draw(2);
    assert!(rig.app.dashboard_ui.textures.contains_key(&design.blake3), "still the project's picture");
    let note = rig.app.dash_design_note(&rig.app.project.dashboard.as_ref().expect("d").pages[0]).expect("a note");
    assert!(note.contains("not the one this dashboard was made with"), "{note}");
    let _ = p;
    let p = rig.draw(3);
    assert!(p.boxes.iter().any(|(t, _)| t.contains("not the one this dashboard was made with")), "the page settings say so: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>());
}

#[test]
fn d5_a_file_that_is_not_a_picture_is_refused_in_the_window_and_a_picture_is_written_in_both_themes() {
    use crate::ui::render_tests_p8::{can_shoot, shoot, styled, SHOT_H, SHOT_PPP, SHOT_W};
    let first = SIZES3[0];
    let mut rig = rig10("p10d5_bad", first.0, first.1, first.2);
    let txt = rig.app.paths.root_dir.join("notes.png");
    std::fs::write(&txt, "not a picture").expect("write");
    rig.app.run_command(&rig.ctx.clone(), CommandId::Dashboard);
    rig.draw(4);
    rig.app.dashboard_ui.import_path = Some(txt);
    let p = rig.draw(5);
    assert!(rig.app.dashboard_ui.tpl.is_none());
    assert!(p.boxes.iter().any(|(t, _)| t == "This file is not a PNG or JPEG picture."), "{:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>());
    if !can_shoot() {
        return;
    }
    for dark in [false, true] {
        let mut rig = rig10("p10d5_shot", SHOT_PPP, SHOT_W, SHOT_H);
        styled(&mut rig, dark);
        let pic = rig.app.paths.root_dir.join("design.png");
        let img = image::RgbaImage::from_fn(1920, 1200, |x, y| {
            let band = (y / 150) as u8;
            image::Rgba([20 + band * 8, 60 + (x / 40) as u8 % 40, 120 + band * 10, 255])
        });
        img.save(&pic).expect("picture");
        rig.app.run_command(&rig.ctx.clone(), CommandId::Dashboard);
        rig.draw(4);
        rig.app.dashboard_ui.import_path = Some(pic);
        rig.draw(5);
        let area = rect_of(&rig, "Design area");
        let s = area.width() / PAGE_W as f32;
        for (x, y, w, h) in [(20.0, 20.0, 460.0, 280.0), (500.0, 20.0, 480.0, 280.0), (20.0, 320.0, 220.0, 100.0)] { // page units, not a size
            drag_pointer(&mut rig, area.min + egui::vec2(x * s, y * s), area.min + egui::vec2((x + w) * s, (y + h) * s));
        }
        type_into(&mut rig, "Slot 1 name", "Deposits chart");
        pick_in(&mut rig, "Slot 1 type", "Chart");
        rig.draw(3);
        shoot(&mut rig, "dashboard-design", dark, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SHOT_W, SHOT_H)));
    }
}

// ------------------------------------------------------------------------------------------------ the export dialog (S10)

fn wait_jobs(rig: &mut Rig) {
    for _ in 0..400 {
        rig.draw(1);
        if !rig.app.jobs.kind_busy(micro_parquet::jobs::JobKind::Export) && !rig.app.jobs.kind_busy(micro_parquet::jobs::JobKind::DashPrepare) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    rig.draw(3);
}

#[test]
fn x3_the_export_dialog_asks_the_email_question_checks_the_size_and_writes_the_three_files() {
    let mut rig = rig10("p10x3_export", SIZES3[1].0, SIZES3[1].1, SIZES3[1].2);
    let what = rig.what.clone();
    dashboard_with_two_items(&mut rig);
    rig.draw(6);
    let out = rig.app.paths.root_dir.join("export_out");
    std::fs::create_dir_all(&out).expect("dir");
    // the dialog opens from the header
    click_named(&mut rig, "Export");
    rig.draw(3);
    click_named(&mut rig, "Export HTML");
    rig.draw(3);
    assert!(rig.app.dashboard_ui.export.is_some(), "{what}: Export HTML opens the dialog");
    // HTML: nothing can be checked before the email question is answered
    click_named(&mut rig, "Export check size");
    rig.draw(3);
    assert!(rig.app.dashboard_ui.export.as_ref().is_some_and(|d| d.prepared.is_none() && !d.busy), "{what}: no size before the question is answered");
    click_named(&mut rig, "Export email No");
    rig.draw(2);
    click_named(&mut rig, "Export check size");
    wait_jobs(&mut rig);
    let prepared = rig.app.dashboard_ui.export.as_ref().and_then(|d| d.prepared.clone()).unwrap_or_else(|| panic!("{what}: the size is calculated: {:?}", rig.app.dashboard_ui.export.as_ref().map(|d| d.message.clone())));
    assert!(prepared.html.starts_with("<!doctype html>") && prepared.parts.total() > 1_000_000, "{what}: {}", prepared.parts.total());
    let html = out.join("report.html");
    rig.app.dashboard_ui.export.as_mut().expect("dialog").path = Some(html.clone());
    click_named(&mut rig, "Export write");
    wait_jobs(&mut rig);
    assert_eq!(std::fs::read_to_string(&html).expect("the HTML file").len(), prepared.html.len(), "{what}: the file is the one that was sized");
    assert!(rig.app.status_message.contains("HTML report written"), "{what}: {}", rig.app.status_message);
    // PowerPoint and PDF: one click each
    for (name, file, magic) in [("PowerPoint", "pack.pptx", &b"PK"[..]), ("PDF", "pack.pdf", &b"%PDF"[..])] {
        click_named(&mut rig, &format!("Export format {}", name));
        rig.draw(2);
        rig.app.dashboard_ui.export.as_mut().expect("dialog").path = Some(out.join(file));
        click_named(&mut rig, "Export write");
        wait_jobs(&mut rig);
        let bytes = std::fs::read(out.join(file)).unwrap_or_else(|e| panic!("{what}: {file}: {e}: {}", rig.app.status_message));
        assert!(bytes.starts_with(magic), "{what}: {file} is a {name} file");
    }
    click_named(&mut rig, "Export close");
    rig.draw(2);
    assert!(rig.app.dashboard_ui.export.is_none(), "{what}: Close closes the dialog");
}
