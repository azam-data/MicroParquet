//! Phase 8 render tests: the dialogs and controls of Phase 8 are drawn by the real UI code in a headless
//! egui context at 1219×686, 1600×900 and 1291×700 pt, and checked by the rectangles they drew: nothing
//! overflows or is clipped, no two labels overlap, and every control works when its real rect is clicked.

use crate::ui::layout_tests::Painted;
use crate::ui::prelude::*;
use crate::ui::render_tests::{assert_dialog_fits, inside, screen};
use crate::ui::render_tests_p4::Rig;
use crate::ui::render_tests_p7::SIZES3;
use crate::ui::tests::{parquet_source, settle, test_app};
use micro_parquet::commands::CommandId;
use micro_parquet::dates::{CalendarSettings, DatePart, Weekday};
use micro_parquet::sort_lists::SortOrder;
use polars::prelude::{NamedFrom, Series};

/// A tab "Deposits" made from `df`, with the test's data folder as `portable_data`.
pub(crate) fn rig8(name: &str, ppp: f32, w: f32, h: f32, df: DataFrame) -> Rig {
    let mut app = test_app(name);
    micro_parquet::rules::set_data_dir_override(Some(app.paths.root_dir.clone()));
    let root = app.paths.root_dir.clone();
    let src = parquet_source(&root, "deposits", df);
    app.create_tab_from_source("Deposits".into(), src, vec![], HashMap::new());
    settle(&mut app);
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

/// A small deposit book: a month label column, a rate band column, a date and a balance.
pub(crate) fn book8() -> DataFrame {
    let days: Vec<i32> = (0..8).map(|i| 20_454 + i * 37).collect();
    DataFrame::new(vec![
        Series::new("Month".into(), &["Mar", "Jan", "Feb", "Mar", "Dec", "Jan", "Aug", "Feb"]).into(),
        Series::new("Band".into(), &["10%\u{2013}11%", "No rate", "0%", "8%\u{2013}9%", "> 15%", "Below 0%", "0%", "8%\u{2013}9%"]).into(),
        Series::new("Branch".into(), &["Dhaka", "Ctg", "Dhaka", "Sylhet", "Ctg", "Dhaka", "Sylhet", "Ctg"]).into(),
        Series::new("Open Date".into(), days).cast(&polars::prelude::DataType::Date).expect("dates").into(),
        Series::new("Balance".into(), &[100.0f64, 200.0, 300.0, 400.0, 500.0, 600.0, 700.0, 800.0]).into(),
    ])
    .expect("book")
}

/// The texts a dialog drew: those of `p` inside `area` that a frame without the dialog does not draw (what is
/// under the dialog is drawn in both).
pub(crate) fn dialog_texts(rig: &mut Rig, p: &Painted, area: egui::Rect) -> Vec<(String, egui::Rect)> {
    let tool = rig.app.active_tool;
    rig.app.active_tool = ActiveToolModal::None;
    let bg = rig.draw(1);
    rig.app.active_tool = tool;
    rig.draw(2);
    let same = |a: &egui::Rect, b: &egui::Rect| (a.min - b.min).length() < 0.5 && (a.max - b.max).length() < 0.5;
    p.boxes.iter().filter(|(t, b)| inside(*b, area) && !bg.boxes.iter().any(|(u, c)| u == t && same(b, c))).cloned().collect()
}

/// No two of these texts overlap (by more than a pixel each way).
pub(crate) fn assert_no_overlap(boxes: &[(String, egui::Rect)], what: &str) {
    let boxes: Vec<&(String, egui::Rect)> = boxes.iter().filter(|(t, _)| !t.trim().is_empty()).collect();
    for (i, (ta, a)) in boxes.iter().enumerate() {
        for (tb, b) in boxes.iter().skip(i + 1) {
            let x = a.intersect(*b);
            assert!(!(a.intersects(*b) && x.width() > 1.0 && x.height() > 1.0), "{what}: '{ta}' {a:?} overlaps '{tb}' {b:?}");
        }
    }
}

/// Every text drawn inside `area` is shown whole (not cut with "…") and lies inside it.
pub(crate) fn assert_not_clipped(p: &Painted, area: egui::Rect, what: &str) {
    for (t, pos) in &p.shown {
        if area.contains(*pos) && !t.trim().is_empty() {
            assert!(!t.ends_with('\u{2026}') || p.texts.iter().any(|(x, q, _)| x == t && q == pos), "{what}: '{t}' is cut");
        }
    }
}

fn bar_box(p: &Painted, text: &str) -> Option<egui::Rect> {
    p.boxes.iter().find(|(t, b)| t.trim() == text && b.top() < theme::TOP_BAR_H).map(|(_, b)| *b)
}

pub(crate) fn click_rect(rig: &mut Rig, r: egui::Rect) -> Painted {
    rig.step(vec![egui::Event::PointerMoved(r.center())]);
    rig.click_later(r.center())
}

/// Clicks the menu `menu` of the top bar, then its item whose text contains `item`.
pub(crate) fn menu_item(rig: &mut Rig, menu: &str, item: &str) -> Painted {
    let p = rig.draw(3);
    let b = bar_box(&p, menu).unwrap_or_else(|| panic!("{}: the menu '{menu}' is in the bar", rig.what));
    let p = click_rect(rig, b);
    // the menu is painted last: its item is the last text that says it
    let it = p.boxes.iter().rev().find(|(t, bx)| t.contains(item) && bx.top() > b.bottom() - 1.0).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: '{item}' is in the {menu} menu", rig.what));
    click_rect(rig, it)
}

fn cal_title() -> String {
    format!("{}  {}", theme::ph::CALENDAR, crate::ui::calendar::CALENDAR_TITLE)
}

#[test]
fn w0_view_calendar_fits_at_three_sizes_and_saves_by_real_clicks() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig8("p8w0cal", ppp, w, h, book8());
        let what = rig.what.clone();
        menu_item(&mut rig, "View", "Calendar");
        assert_eq!(rig.app.active_tool, ActiveToolModal::Calendar, "{what}: View ▸ Calendar… opens the dialog");
        let p = rig.draw(4);
        let r = assert_dialog_fits(&rig.ctx, &cal_title(), &what);
        let texts = dialog_texts(&mut rig, &p, r.window);
        assert!(texts.iter().any(|(t, _)| t.contains("FISCAL YEAR STARTS IN")), "{what}: {texts:?}");
        assert_no_overlap(&texts, &what);
        assert_not_clipped(&p, r.window, &what);
        let g = rig.app.calendar_dialog.rects.clone();
        assert_eq!((g.months.len(), g.days.len()), (12, 7), "{what}");
        for (n, b) in g.months.iter().chain(g.days.iter()).chain(g.buttons.iter()) {
            assert!(inside(*b, r.window), "{what}: '{n}' {b:?} is inside the dialog {:?}", r.window);
        }
        assert!(p.boxes.iter().any(|(t, _)| t == "Week starts Saturday \u{b7} Fiscal year from January"), "{what}: the calendar line with the defaults");
        // July and Monday by real clicks: the line follows
        let july = g.months.iter().find(|(n, _)| n == "July").map(|(_, b)| *b).expect("July");
        click_rect(&mut rig, july);
        let monday = rig.app.calendar_dialog.rects.days.iter().find(|(n, _)| n == "Monday").map(|(_, b)| *b).expect("Monday");
        let p = click_rect(&mut rig, monday);
        assert_eq!(rig.app.calendar_dialog.draft, CalendarSettings { fy_start_month: 7, week_start: Weekday::Mon }, "{what}");
        assert!(p.boxes.iter().any(|(t, _)| t == "Week starts Monday \u{b7} Fiscal year from July"), "{what}: the line shows the choice");
        assert_eq!(rig.app.settings.calendar, CalendarSettings::default(), "{what}: nothing is saved before Save");
        let save = rig.app.calendar_dialog.rects.buttons.iter().find(|(n, _)| n == "Save").map(|(_, b)| *b).expect("Save");
        click_rect(&mut rig, save);
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: Save closes");
        assert_eq!(rig.app.settings.calendar, CalendarSettings { fy_start_month: 7, week_start: Weekday::Mon }, "{what}");
        let (saved, _) = micro_parquet::settings::Settings::load(&rig.app.paths.root_dir);
        assert_eq!(saved.calendar, rig.app.settings.calendar, "{what}: written to settings.json");
        // Defaults then Cancel leaves the saved calendar alone
        rig.app.run_command(&rig.ctx, CommandId::CalendarSettings);
        rig.draw(4);
        let defaults = rig.app.calendar_dialog.rects.buttons.iter().find(|(n, _)| n == "Defaults").map(|(_, b)| *b).expect("Defaults");
        click_rect(&mut rig, defaults);
        assert_eq!(rig.app.calendar_dialog.draft, CalendarSettings::default(), "{what}: Defaults");
        let cancel = rig.app.calendar_dialog.rects.buttons.iter().find(|(n, _)| n == "Cancel").map(|(_, b)| *b).expect("Cancel");
        click_rect(&mut rig, cancel);
        assert_eq!(rig.app.settings.calendar.week_start, Weekday::Mon, "{what}: Cancel keeps the saved calendar");
    }
    // Ctrl+K finds it
    let entries: Vec<micro_parquet::commands::Entry> = micro_parquet::commands::COMMANDS.iter().map(|c| micro_parquet::commands::Entry { label: c.title, words: c.words, recent: None }).collect();
    for q in ["calendar", "fiscal year", "week start"] {
        let top = micro_parquet::commands::rank(q, &entries).first().map(|i| micro_parquet::commands::COMMANDS[*i].id);
        assert_eq!(top, Some(CommandId::CalendarSettings), "Ctrl+K '{q}'");
    }
}

#[test]
fn w0_a_field_added_to_a_pivot_a_chart_or_a_sort_takes_calendar_or_ranges() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig8("p8w0auto", ppp, w, h, book8());
        let what = rig.what.clone();
        // pivot: Month to Rows, Band to Columns, Branch to Rows (Branch stays natural)
        rig.app.run_command(&rig.ctx, CommandId::Pivot);
        rig.draw(6);
        rig.app.add_pivot_field("Month", crate::ui::pivot_builder::Area::Rows);
        rig.app.add_pivot_field("Band", crate::ui::pivot_builder::Area::Columns);
        rig.app.add_pivot_field("Branch", crate::ui::pivot_builder::Area::Rows);
        rig.draw(4);
        let orders: Vec<(String, SortOrder)> = rig.app.pivot_builder.spec.field_orders.iter().map(|o| (o.field.clone(), o.order.clone())).collect();
        assert_eq!(orders, vec![("Month".to_string(), SortOrder::Calendar { week_start: Weekday::Sat }), ("Band".to_string(), SortOrder::Ranges)], "{what}");
        rig.app.active_tool = ActiveToolModal::None;
        rig.app.pivot_builder.open = false;
        rig.draw(2);

        // sort: a level on Month gets Calendar; the order box lists Calendar and Ranges and a click picks one
        rig.app.run_command(&rig.ctx, CommandId::Sort);
        rig.draw(3);
        rig.app.sort.levels = vec![SortLevel { column: "Branch".into(), descending: false, order: SortOrder::Natural }];
        let p = rig.draw(3);
        // the level's column box, then "Month" in its list, by real clicks
        let r = theme::dialog_report(&rig.ctx, "\u{E444}  Sort rows").expect("the sort dialog");
        let col_box = dialog_texts(&mut rig, &p, r.window).into_iter().find(|(t, _)| t == "Branch").map(|(_, b)| b).expect("the column box shows Branch");
        let p = click_rect(&mut rig, col_box);
        let month = p.boxes.iter().filter(|(t, b)| t == "Month" && b.top() > col_box.top()).map(|(_, b)| *b).last().unwrap_or_else(|| panic!("{what}: Month is in the list"));
        click_rect(&mut rig, month);
        assert_eq!(rig.app.sort.levels[0].column, "Month", "{what}");
        let p = rig.draw(4);
        assert_eq!(rig.app.sort.levels[0].order, SortOrder::Calendar { week_start: Weekday::Sat }, "{what}: the sort level");
        let r = theme::dialog_report(&rig.ctx, "\u{E444}  Sort rows").expect("the sort dialog");
        let texts = dialog_texts(&mut rig, &p, r.window);
        assert_no_overlap(&texts, &what);
        let combo = texts.into_iter().find(|(t, _)| t == "Calendar").map(|(_, b)| b).expect("the order box shows Calendar");
        let p = click_rect(&mut rig, combo);
        let ranges = p.boxes.iter().find(|(t, b)| t == "Ranges" && b.top() > combo.top()).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the order list has Ranges"));
        click_rect(&mut rig, ranges);
        assert_eq!(rig.app.sort.levels[0].order, SortOrder::Ranges, "{what}: picked by a real click");
        rig.app.active_tool = ActiveToolModal::None;
        rig.draw(2);

        // chart: the category Band (no order chosen) gets Ranges once its labels are read
        let mut spec = rig.app.chart.spec.clone();
        spec.category = "Branch".into();
        rig.app.chart_set_spec(spec);
        rig.draw(2);
        if let Some(o) = rig.app.auto_order("Band", crate::ui::auto_order::AutoTarget::ChartCategory) {
            panic!("{what}: Band declares no order, so it must be read: {o:?}");
        }
        let mut spec = rig.app.chart.spec.clone();
        spec.category = "Band".into();
        rig.app.chart_set_spec(spec);
        rig.draw(4);
        assert_eq!(rig.app.chart.spec.order, SortOrder::Ranges, "{what}: the chart's category");
    }
}

// ================================================================================ W1: Split date

fn split_title() -> String {
    format!("{}  {}", theme::ph::CALENDAR, crate::ui::split_date::SPLIT_DATE_TITLE)
}

fn split_button(rig: &Rig, starts: &str) -> egui::Rect {
    rig.app.split_date.rects.buttons.iter().find(|(n, _)| n.starts_with(starts)).map(|(_, b)| *b).unwrap_or_else(|| panic!("{}: the button '{starts}'", rig.what))
}

fn split_part(rig: &Rig, name: &str) -> egui::Rect {
    rig.app.split_date.rects.parts.iter().find(|(n, _)| n == name).map(|(_, b)| *b).unwrap_or_else(|| panic!("{}: the tick box '{name}'", rig.what))
}

#[test]
fn w1_split_date_from_the_column_panel_and_the_edit_menu_by_real_clicks_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig8("p8w1", ppp, w, h, book8());
        let what = rig.what.clone();
        // the Column panel offers it for a date column, not for a text one
        rig.app.selected_column = Some("Month".into());
        rig.app.set_side_tab_visible(crate::ui::steps::RightTab::Column, true);
        rig.app.right_tab = crate::ui::steps::RightTab::Column;
        let p = rig.draw(4);
        assert!(!p.boxes.iter().any(|(t, _)| t.contains("Split date")), "{what}: no button for a text column");
        rig.app.selected_column = Some("Open Date".into());
        let p = rig.draw(4);
        let b = p.boxes.iter().find(|(t, _)| t.contains("Split date\u{2026}")).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the Column panel has Split date…"));
        click_rect(&mut rig, b);
        assert_eq!(rig.app.active_tool, ActiveToolModal::SplitDate, "{what}");
        assert_eq!(rig.app.split_date.column, "Open Date", "{what}: the inspected column");
        let p = rig.draw(4);
        let r = assert_dialog_fits(&rig.ctx, &split_title(), &what);
        let texts = dialog_texts(&mut rig, &p, r.window);
        assert_no_overlap(&texts, &what);
        let g = rig.app.split_date.rects.clone();
        assert_eq!(g.parts.len(), 14, "{what}");
        for (n, b) in g.parts.iter().chain(g.buttons.iter()) {
            assert!(inside(*b, r.window), "{what}: '{n}' {b:?} is inside the dialog {:?}", r.window);
        }
        // the preview is in the scrolling middle: as wide as the dialog allows, and it starts in view
        let pv = g.preview.expect("the preview");
        assert!(pv.left() >= r.window.left() - 0.5 && pv.right() <= r.window.right() + 0.5 && pv.top() < r.body.bottom(), "{what}: the preview {pv:?} in {:?} / {:?}", r.window, r.body);
        assert!(texts.iter().any(|(t, _)| t == "Week starts Saturday \u{b7} Fiscal year from January"), "{what}: the calendar line");
        // the default ticks, the preview's columns and 5 rows
        assert_eq!(rig.app.split_date.parts, DatePart::DEFAULT_SPLIT.to_vec(), "{what}");
        for c in ["Open Date Year", "Open Date Quarter", "Open Date Month", "Open Date Month-Year", "Open Date Day name"] {
            assert!(texts.iter().any(|(t, _)| t == c), "{what}: the preview has '{c}'");
        }
        // untick Day name, tick Fiscal year, by real clicks
        let b = split_part(&rig, "Day name");
        click_rect(&mut rig, b);
        let b = split_part(&rig, "Fiscal year");
        click_rect(&mut rig, b);
        assert!(!rig.app.split_date.parts.contains(&DatePart::DayName) && rig.app.split_date.parts.contains(&DatePart::FiscalYear), "{what}: {:?}", rig.app.split_date.parts);
        rig.draw(2);
        assert!(rig.app.split_date.rects.buttons.iter().any(|(n, _)| n == "Add 5 columns"), "{what}");
        // a text column: the message, and the button does nothing
        let col_box = rig.app.split_date.rects.column.expect("the column box");
        let p = click_rect(&mut rig, col_box);
        let month = p.boxes.iter().filter(|(t, b)| t == "Month" && b.top() > col_box.top()).map(|(_, b)| *b).last().unwrap_or_else(|| panic!("{what}: Month is listed under Not dates"));
        let p = click_rect(&mut rig, month);
        assert_eq!(rig.app.split_date.column, "Month", "{what}");
        assert!(p.boxes.iter().any(|(t, _)| t.contains("'Month' is text. Set it to Date in Schema & Ingestion (D/M/Y) first.")), "{what}: the message");
        let b = split_button(&rig, "Add");
        click_rect(&mut rig, b);
        assert_eq!(rig.app.active_tool, ActiveToolModal::SplitDate, "{what}: nothing is added for a text column");
        // back to the date column (the prefix follows it) and Add
        rig.app.split_date.column = "Open Date".into();
        rig.app.split_date.prefix = "Open Date".into();
        rig.draw(3);
        let b = split_button(&rig, "Add 5 columns");
        click_rect(&mut rig, b);
        settle(&mut rig.app);
        rig.draw(3);
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: closed");
        let tab = &rig.app.tabs[rig.app.active_tab];
        assert!(matches!(tab.steps.last(), Some(TransformStep::SplitDate { .. })), "{what}");
        assert_eq!(tab.steps.last().map(|s| s.short_label()), Some("Split date: Open Date \u{2192} 5 columns".to_string()), "{what}");
        let cols = tab.available_columns();
        for c in ["Open Date Year", "Open Date Quarter", "Open Date Month", "Open Date Month-Year", "Open Date Fiscal year"] {
            assert!(cols.iter().any(|x| x == c), "{what}: '{c}' in {cols:?}");
        }
        // the Edit menu opens it again; the same names are refused now, naming the column
        menu_item(&mut rig, "Edit", "Split date");
        assert_eq!(rig.app.active_tool, ActiveToolModal::SplitDate, "{what}: Edit ▸ Split date…");
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t.contains("a column 'Open Date Year' already exists. Change the prefix")), "{what}: the clash is named");
        // a pivot of a part lists it in Calendar order without any choice (the order the step declares)
        rig.app.active_tool = ActiveToolModal::None;
        rig.app.run_command(&rig.ctx, CommandId::Pivot);
        rig.draw(4);
        rig.app.add_pivot_field("Open Date Month", crate::ui::pivot_builder::Area::Rows);
        assert_eq!(rig.app.pivot_builder.spec.field_orders.first().map(|o| o.order.clone()), Some(SortOrder::Calendar { week_start: Weekday::Sat }), "{what}: known at once from the steps");
    }
}
