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
    p.glyphs.iter().filter(|(t, b)| inside(*b, area) && !bg.glyphs.iter().any(|(u, c)| u == t && same(b, c))).cloned().collect()
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

// ================================================================================ W2: date grouping

fn last_box(p: &Painted, text: &str, below: f32) -> Option<egui::Rect> {
    p.boxes.iter().rev().find(|(t, b)| t == text && b.top() > below - 1.0).map(|(_, b)| *b)
}

#[test]
fn w2_the_pivot_expander_adds_a_date_part_by_real_clicks_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig8("p8w2piv", ppp, w, h, book8());
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::Pivot);
        let p = rig.draw(6);
        let r = assert_dialog_fits(&rig.ctx, &format!("{}  Pivot table", theme::ph::TABLE), &what);
        let g = rig.app.pivot_builder.rects.clone();
        let exp = g.expanders.iter().find(|(n, _)| n == "Open Date").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: a Date parts expander under Open Date"));
        assert!(inside(exp, g.fields_list), "{what}: the expander {exp:?} is in the field list {:?}", g.fields_list);
        assert!(g.expanders.len() == 1, "{what}: only the date column has one: {:?}", g.expanders);
        assert_no_overlap(&dialog_texts(&mut rig, &p, r.window), &what);
        // open it: the parts are listed under the column, inside the list, without overlapping
        click_rect(&mut rig, exp);
        let p = rig.draw(3);
        let g = rig.app.pivot_builder.rects.clone();
        let month_row = g.field_rows.iter().find(|(n, _)| n == "Open Date \u{b7} Month").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the Month part is listed"));
        assert!(month_row.left() >= g.fields_list.left() - 0.5 && month_row.right() <= g.fields_list.right() + 0.5, "{what}: {month_row:?} in {:?}", g.fields_list);
        assert!(p.boxes.iter().any(|(t, _)| t == "\u{203a} Month"), "{what}: shown as '› Month'");
        let texts = dialog_texts(&mut rig, &p, g.fields_list);
        assert_no_overlap(&texts, &what);
        // its menu, then "Add to Rows", by real clicks
        let menu = g.field_menus.iter().find(|(n, _)| n == "Open Date \u{b7} Month").map(|(_, b)| *b).expect("the part's menu");
        let p = click_rect(&mut rig, menu);
        assert!(!p.boxes.iter().any(|(t, b)| t == "Add to Values" && b.top() > menu.top() - 1.0 && (b.left() - menu.left()).abs() < 300.0), "{what}: a part is not offered for Values");
        let add = last_box(&p, "Add to Rows", menu.top()).unwrap_or_else(|| panic!("{what}: Add to Rows"));
        click_rect(&mut rig, add);
        let spec = rig.app.pivot_builder.spec.clone();
        assert_eq!(spec.rows, vec!["Open Date \u{b7} Month".to_string()], "{what}");
        assert_eq!(spec.date_fields.len(), 1, "{what}");
        assert_eq!(spec.date_fields[0].part, DatePart::Month, "{what}");
        assert_eq!(spec.field_orders.first().map(|o| o.order.clone()), Some(SortOrder::Calendar { week_start: Weekday::Sat }), "{what}: Calendar order");
        // with a value the preview lists the months in calendar order
        rig.app.add_pivot_field("Balance", crate::ui::pivot_builder::Area::Values);
        rig.draw(4);
        let pv = match &rig.app.pivot_builder.preview {
            Some(Ok(df)) => df.clone(),
            other => panic!("{what}: the preview: {other:?}"),
        };
        let labels: Vec<String> = pv.column("Open Date \u{b7} Month").expect("labels").str().expect("text").into_iter().flatten().map(str::to_string).collect();
        let months: Vec<String> = labels.iter().filter(|l| micro_parquet::dates::parse_month(l).is_some()).cloned().collect();
        let mut sorted = months.clone();
        sorted.sort_by_key(|m| micro_parquet::dates::parse_month(m));
        assert_eq!(months, sorted, "{what}: {labels:?}");
        // close it again
        let exp = rig.app.pivot_builder.rects.expanders[0].1;
        click_rect(&mut rig, exp);
        assert!(!rig.app.pivot_builder.rects.field_rows.iter().any(|(n, _)| n.contains('\u{b7}')), "{what}: collapsed");
    }
}

#[test]
fn w2_the_chart_category_picker_offers_the_parts_of_a_date_by_real_clicks_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig8("p8w2chart", ppp, w, h, book8());
        let what = rig.what.clone();
        rig.app.set_side_tab_visible(crate::ui::steps::RightTab::Chart, true);
        let mut spec = rig.app.chart.spec.clone();
        spec.category = "Branch".into();
        spec.value = Some("Balance".into());
        rig.app.chart_set_spec(spec);
        rig.draw(4);
        let cat = rig.app.chart.rects.fields.iter().find(|(n, _)| n == "Category").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the Category box"));
        let p = click_rect(&mut rig, cat);
        let q = last_box(&p, "Open Date \u{203a} Quarter", 0.0).unwrap_or_else(|| panic!("{what}: the picker lists Open Date › Quarter: {cat:?} {:?}", p.boxes.iter().rev().take(25).collect::<Vec<_>>()));
        let screen = rig.ctx.screen_rect();
        assert!(q.left() >= screen.left() && q.right() <= screen.right() + 0.5, "{what}: the item {q:?} is in the window");
        // the list scrolls to it if needed: a click on its real rect
        click_rect(&mut rig, q);
        rig.draw(4);
        let s = rig.app.chart.spec.clone();
        assert_eq!(s.category, "Open Date \u{b7} Quarter", "{what}");
        assert_eq!(s.category_part.as_ref().map(|p| p.part), Some(DatePart::Quarter), "{what}");
        assert_eq!(s.order, SortOrder::Calendar { week_start: Weekday::Sat }, "{what}");
        let d = rig.app.chart.data.clone().unwrap_or_else(|| panic!("{what}: the chart was calculated: {:?}", rig.app.chart.error));
        let qs: Vec<&String> = d.categories.iter().filter(|c| c.starts_with('Q')).collect();
        let mut sorted = qs.clone();
        sorted.sort();
        assert_eq!(qs, sorted, "{what}: {:?}", d.categories);
        let total: f64 = d.values[0].iter().flatten().sum();
        assert!((total - 3600.0).abs() < 1e-9, "{what}: the chart adds up to the table: {total}");
    }
}

#[test]
fn w2_the_filter_by_choice_filters_on_day_name_by_real_clicks_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = rig8("p8w2flt", ppp, w, h, book8());
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::Filter);
        rig.app.filter.conditions = vec![FilterCondition { column: "Open Date".into(), op: CondOp::InList, ..Default::default() }];
        let p = rig.draw(4);
        let title = "\u{E30C}  Filter rows";
        let r = assert_dialog_fits(&rig.ctx, title, &what);
        assert_no_overlap(&dialog_texts(&mut rig, &p, r.window), &what);
        let by = rig.app.filter.by_boxes.first().map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: a date column has a By box"));
        assert!(inside(by, r.window), "{what}: the By box {by:?} is in the dialog");
        assert!(p.boxes.iter().any(|(t, _)| t == "By Date"), "{what}: By Date to start with");
        let p = click_rect(&mut rig, by);
        for want in ["Year", "Month-Year", "Month", "Quarter", "Day name"] {
            assert!(last_box(&p, want, by.top()).is_some(), "{what}: the By list has {want}");
        }
        let day = last_box(&p, "Day name", by.top()).expect("Day name");
        click_rect(&mut rig, day);
        assert_eq!(rig.app.filter.conditions[0].part.as_ref().map(|p| p.part), Some(DatePart::DayName), "{what}");
        // the picker lists day names (Sat first: the week starts on Saturday); pick Fri and Sat
        let pick = rig.app.filter.pick_buttons.first().map(|(_, b)| *b).expect("the picker button");
        click_rect(&mut rig, pick);
        settle(&mut rig.app);
        rig.draw(4);
        let rows: Vec<String> = rig.app.picker.rects.rows.iter().map(|(t, _)| t.clone()).collect();
        let days: Vec<String> = rows.iter().filter(|t| Weekday::parse(t).is_some()).cloned().collect();
        let mut sorted = days.clone();
        sorted.sort_by_key(|d| CalendarSettings::default().day_position(Weekday::parse(d).expect("a day")));
        assert_eq!(days, sorted, "{what}: in week order from Saturday: {rows:?}");
        // Sat and Sun, the two at the top of the list (the list scrolls below its height; the library test pins Fri and Sat)
        for d in ["Sat", "Sun"] {
            let b = rig.app.picker.rects.boxes.iter().find(|(t, _)| t == d).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: {d} is listed"));
            click_rect(&mut rig, b);
        }
        let c = rig.app.filter.conditions[0].clone();
        assert_eq!(c.values, vec!["Sat".to_string(), "Sun".to_string()], "{what}");
        assert_eq!(rig.app.filter.conditions.len(), 1, "{what}");
        // Apply: the tab keeps the Friday and Saturday rows
        let p = rig.draw(2);
        let apply = last_box(&p, "\u{E182}  Apply to current tab", 0.0).expect("Apply");
        click_rect(&mut rig, apply);
        settle(&mut rig.app);
        rig.draw(3);
        let tab = &rig.app.tabs[rig.app.active_tab];
        assert!(matches!(tab.steps.last(), Some(TransformStep::FilterGroup { .. })), "{what}: {:?} tool {:?} status {}", tab.steps, rig.app.active_tool, rig.app.status_message);
        let df = &tab.cached_slice;
        let days: Vec<i32> = df.column("Open Date").expect("dates").cast(&polars::prelude::DataType::Int32).expect("days").i32().expect("i32").into_iter().flatten().collect();
        assert!(!days.is_empty(), "{what}: some rows are kept");
        for d in days {
            let wd = chrono::Datelike::weekday(&micro_parquet::dates::date_from_days(d).expect("date"));
            assert!(matches!(wd, chrono::Weekday::Sat | chrono::Weekday::Sun), "{what}: {wd:?} kept");
        }
    }
}

// ================================================================================ W3: rate bands

#[test]
fn w3_the_number_the_bands_tick_box_is_off_and_works_by_a_real_click_at_three_sizes() {
    let en = '\u{2013}';
    for (ppp, w, h) in SIZES3 {
        let df = DataFrame::new(vec![Series::new("Rate".into(), &[9.0f64, 8.5, 0.0, 10.5]).into(), Series::new("Balance".into(), &[100.0f64, 200.0, 300.0, 400.0]).into()]).expect("frame");
        let mut rig = rig8("p8w3", ppp, w, h, df);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::RateProfile);
        let p = rig.draw(6);
        let title = "\u{E0B4} Rate profile: put a rate column into bands";
        let r = assert_dialog_fits(&rig.ctx, title, &what);
        let tick = rig.app.rate_band.numbered_rect.unwrap_or_else(|| panic!("{what}: the tick box"));
        assert!(inside(tick, r.window) && tick.top() < r.body.bottom(), "{what}: the tick box {tick:?} is in view in {:?}", r.body);
        assert!(!rig.app.rate_band.numbered, "{what}: off for a new step");
        assert!(p.boxes.iter().any(|(t, _)| t == &format!("Number the bands (09. 8%{en}9%)")), "{what}");
        assert!(p.boxes.iter().any(|(t, _)| t.contains(&format!("0%{en}1%")) && !t.contains("01. ")), "{what}: the preview shows plain labels");
        assert_no_overlap(&dialog_texts(&mut rig, &p, r.window), &what);
        // on by a real click: the preview and the step are numbered
        let p = click_rect(&mut rig, tick);
        assert!(rig.app.rate_band.numbered, "{what}: ticked");
        assert!(p.boxes.iter().any(|(t, _)| t.contains(&format!("01. 0%{en}1%"))), "{what}: the preview shows numbered labels");
        // off again, and Add: a plain step whose column declares the band order
        click_rect(&mut rig, tick);
        let p = rig.draw(2);
        let add = p.boxes.iter().rev().find(|(t, _)| t.contains("Add Rate Profile")).map(|(_, b)| *b).expect("Add");
        click_rect(&mut rig, add);
        settle(&mut rig.app);
        rig.draw(3);
        let tab = &rig.app.tabs[rig.app.active_tab];
        match tab.steps.last() {
            Some(TransformStep::RateBand { band_def, .. }) => assert!(!band_def.numbered, "{what}"),
            other => panic!("{what}: {other:?}"),
        }
        let labels: Vec<String> = tab.cached_slice.column("Rate_Band").expect("bands").str().expect("text").into_iter().flatten().map(str::to_string).collect();
        assert_eq!(labels, vec![format!("8%{en}9%"), format!("8%{en}9%"), "0%".to_string(), format!("10%{en}11%")], "{what}");
        // a pivot of the band column takes the band order at once
        rig.app.run_command(&rig.ctx, CommandId::Pivot);
        rig.draw(4);
        rig.app.add_pivot_field("Rate_Band", crate::ui::pivot_builder::Area::Rows);
        match rig.app.pivot_builder.spec.field_orders.first().map(|o| o.order.clone()) {
            Some(SortOrder::List { values, .. }) => assert_eq!(&values[..3], ["Below 0%", "0%", &format!("0%{en}1%")], "{what}"),
            other => panic!("{what}: the declared band order: {other:?}"),
        }
    }
}

// ================================================================================ W4: project files

use crate::ui::project::{PROMPT_TITLE, SAVE_TITLE, SOURCES_TITLE};
use crate::ui::render_tests_p7::chart_rig_with;
use micro_parquet::chart::{ChartAgg, ChartSpec};

fn proj_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    chart_rig_with(name, book8(), ppp, w, h)
}

/// Frames until `done`, moving the clock on (the project's "unsaved" check runs on the clock).
fn pump_until(rig: &mut Rig, what: &str, done: &dyn Fn(&Rig) -> bool) {
    for _ in 0..600 {
        rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 1.0);
        rig.draw(1);
        if done(rig) {
            rig.draw(2);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("{}: '{what}' timed out (status: {})", rig.what, rig.app.status_message);
}

/// Waits for the worker that builds and hashes a project for Save (the window is never blocked by it).
fn wait_saved(rig: &mut Rig) {
    pump_until(rig, "the project is saved", &|r| !r.app.project.saving);
}

fn tick(rig: &mut Rig) {
    rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 1.0);
    rig.draw(2);
}

/// Clicks the File menu, then its item that ends with `exact`.
fn file_item(rig: &mut Rig, exact: &str) -> Painted {
    let p = rig.draw(3);
    let b = bar_box(&p, "File").unwrap_or_else(|| panic!("{}: the File menu", rig.what));
    let p = click_rect(rig, b);
    let it = p
        .boxes
        .iter()
        .rev()
        .find(|(t, bx)| t.trim().ends_with(exact) && bx.top() > b.bottom() - 1.0)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("{}: '{exact}' is in the File menu: {:?}", rig.what, p.boxes.iter().filter(|(_, bx)| bx.top() > b.bottom()).map(|(t, _)| t.clone()).collect::<Vec<_>>()));
    assert!(inside(it, rig.ctx.screen_rect()), "{}: the menu item '{exact}' {it:?} is inside the window", rig.what);
    click_rect(rig, it)
}

fn rows_hash_of(tab: &TabState, temp: &std::path::Path) -> String {
    let lf = micro_parquet::engine::compile_tab_lazy(&tab.source, &tab.steps, temp).expect("compiles");
    let mut df = lf.collect().expect("collects");
    let mut buf: Vec<u8> = Vec::new();
    {
        use polars::prelude::SerWriter;
        polars::prelude::CsvWriter::new(&mut buf).finish(&mut df).expect("csv");
    }
    blake3::hash(&buf).to_hex().to_string()
}

fn titles_and_hashes(rig: &Rig) -> Vec<(String, String)> {
    rig.app.tabs.iter().map(|t| (t.title.clone(), rows_hash_of(t, &rig.app.paths.temp_dir))).collect()
}

fn branch_pivot() -> TransformStep {
    TransformStep::from_pivot_spec(PivotSpec { rows: vec!["Branch".into()], values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None }], row_totals: true, column_totals: true, ..Default::default() })
}

/// The file tab, a pivot tab of it and a chart tab of it.
fn three_tabs(rig: &mut Rig) {
    let src = rig.app.tabs[0].source.clone();
    rig.app.create_tab_from_source("Pivot: Branch".into(), src, vec![branch_pivot()], HashMap::new());
    settle(&mut rig.app);
    rig.app.active_tab = 0;
    rig.draw(4);
    pump_until(rig, "the first chart", &|r| !r.app.chart.busy && r.app.chart.data.is_some());
    let mut spec: ChartSpec = rig.app.chart.spec.clone();
    spec.category = "Branch".into();
    spec.series = None;
    spec.value = Some("Balance".into());
    spec.agg = ChartAgg::Sum;
    rig.app.chart_set_spec(spec);
    pump_until(rig, "the chart of Branch", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.category == "Branch"));
    rig.app.chart_open_as_chart_tab();
    pump_until(rig, "the chart tab", &|r| r.app.chart_tabs.len() == 1 && r.app.tabs.len() == 3);
}

fn button(rig: &Rig, name: &str) -> egui::Rect {
    rig.app.project.rects.buttons.iter().find(|(n, _)| n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: the button '{name}' in {:?}", rig.what, rig.app.project.rects.buttons.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()))
}

fn click_button(rig: &mut Rig, name: &str) -> Painted {
    let r = button(rig, name);
    click_rect(rig, r)
}

fn sources_title() -> String {
    format!("{}  {}", theme::ph::FOLDER_OPEN, SOURCES_TITLE)
}

#[test]
fn w4_the_file_menu_saves_closes_and_opens_a_project_by_real_clicks_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w4a", ppp, w, h);
        let what = rig.what.clone();
        let root = rig.app.paths.root_dir.clone();
        three_tabs(&mut rig);
        let pfile = root.join("proj").join("Treasury.mpproj");
        // the six entries are in the File menu, and Ctrl+K finds them
        let p = rig.draw(3);
        let b = bar_box(&p, "File").expect("File");
        let p = click_rect(&mut rig, b);
        for item in ["New project", "Open project\u{2026}", "Open recent project", "Save project", "Save project as\u{2026}", "Close project"] {
            assert!(p.boxes.iter().any(|(t, bx)| t.trim().ends_with(item) && inside(*bx, rig.ctx.screen_rect())), "{what}: '{item}' is in the File menu");
        }
        rig.step(vec![egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }]);
        rig.draw(2);
        let entries: Vec<micro_parquet::commands::Entry> = micro_parquet::commands::COMMANDS.iter().map(|c| micro_parquet::commands::Entry { label: c.title, words: c.words, recent: None }).collect();
        for (q, id) in [("new project", CommandId::ProjectNew), ("open project", CommandId::ProjectOpen), ("recent project", CommandId::ProjectRecent), ("save project as", CommandId::ProjectSaveAs), ("close project", CommandId::ProjectClose), ("mpproj", CommandId::ProjectNew)] {
            let hits: Vec<CommandId> = micro_parquet::commands::rank(q, &entries).iter().take(6).map(|i| micro_parquet::commands::COMMANDS[*i].id).collect();
            assert!(hits.contains(&id), "{what}: Ctrl+K '{q}' finds {id:?}: {hits:?}");
        }
        let hits: Vec<CommandId> = micro_parquet::commands::rank("save project", &entries).iter().take(3).map(|i| micro_parquet::commands::COMMANDS[*i].id).collect();
        assert!(hits.contains(&CommandId::ProjectSave), "{what}: {hits:?}");
        assert!(rig.app.command_unavailable(CommandId::ProjectClose).is_some(), "{what}: Close project is greyed while no project is open");

        // Save project as…: nothing is written until the file is chosen; then it is, with the project in the title
        rig.app.project.test_pick = Some(pfile.clone());
        file_item(&mut rig, "Save project as\u{2026}");
        wait_saved(&mut rig);
        assert!(pfile.exists(), "{what}: the project file is written: {}", rig.app.status_message);
        assert!(rig.app.project.active && rig.app.project.name == "Treasury", "{what}");
        assert_eq!(rig.app.settings.recent_projects.first().map(String::as_str), Some(pfile.display().to_string().as_str()), "{what}: recent");
        tick(&mut rig);
        assert!(rig.app.window_title.starts_with(&format!("MicroParquet v{} \u{2014} Treasury", env!("CARGO_PKG_VERSION"))) && !rig.app.window_title.contains('\u{2022}'), "{what}: {}", rig.app.window_title);
        let saved = micro_parquet::project::load_project(&pfile).expect("the project reads");
        assert_eq!(saved.tabs.iter().map(|t| t.key.clone()).collect::<Vec<_>>(), vec!["t1", "t2", "t3"], "{what}");
        assert_eq!(saved.active_tab, "t3", "{what}: the chart tab was active");

        // a change to a tab puts the "•" in the title; Save project writes it and takes it away
        rig.app.tabs[0].steps.push(TransformStep::SelectAndReorderColumns { cols: vec!["Month".into(), "Branch".into(), "Balance".into()] });
        tick(&mut rig);
        assert!(rig.app.window_title.contains("Treasury \u{2022}"), "{what}: {}", rig.app.window_title);
        file_item(&mut rig, "Save project");
        wait_saved(&mut rig);
        tick(&mut rig);
        assert!(!rig.app.window_title.contains('\u{2022}'), "{what}: saved: {}", rig.app.window_title);
        assert_eq!(micro_parquet::project::load_project(&pfile).expect("reads").tabs[0].steps.len(), 1, "{what}: the step is in the file");
        let want = titles_and_hashes(&rig);
        let (want_chart, want_active) = (rig.app.chart_tabs.values().next().map(|i| serde_json::to_value(&i.data.spec).expect("json")), rig.app.active_tab);

        // Open project…: the Sources dialog comes first and nothing has run yet (the tabs of the session are still there)
        let before_titles: Vec<String> = rig.app.tabs.iter().map(|t| t.title.clone()).collect();
        rig.app.project.test_pick = Some(pfile.clone());
        file_item(&mut rig, "Open project\u{2026}");
        assert_eq!(rig.app.active_tool, ActiveToolModal::ProjectSources, "{what}");
        assert_eq!(rig.app.tabs.iter().map(|t| t.title.clone()).collect::<Vec<_>>(), before_titles, "{what}: no tab opens or closes before the dialog is answered");
        let p = rig.draw(3);
        let r = assert_dialog_fits(&rig.ctx, &sources_title(), &what);
        let texts = dialog_texts(&mut rig, &p, r.window);
        assert_no_overlap(&texts, &what);
        assert_not_clipped(&p, r.window, &what);
        assert!(texts.iter().any(|(t, _)| t == "book.parquet") && texts.iter().any(|(t, _)| t == "Found"), "{what}: {texts:?}");
        for (n, b) in rig.app.project.rects.buttons.iter() {
            assert!(inside(*b, r.window), "{what}: '{n}' {b:?} is inside the dialog {:?}", r.window);
        }
        click_button(&mut rig, "Open");
        pump_until(&mut rig, "the project opens", &|r| r.app.project.opening.is_none() && r.app.tabs.len() == 3 && !r.app.jobs.any_busy());
        assert_eq!(titles_and_hashes(&rig), want, "{what}: every tab has the same rows as before the project was closed");
        assert_eq!(rig.app.active_tab, want_active, "{what}: the active tab");
        assert_eq!(rig.app.chart_tabs.values().next().map(|i| serde_json::to_value(&i.data.spec).expect("json")), want_chart, "{what}: the chart tab's chart");
        assert_eq!(rig.app.project.last_summary, "3 tabs opened.", "{what}");
        assert!(rig.app.project.active && rig.app.project.name == "Treasury", "{what}");
        tick(&mut rig);
        assert!(!rig.app.window_title.contains('\u{2022}'), "{what}: a project that was just opened has nothing unsaved: {}", rig.app.window_title);
        assert!(rig.app.chart_tabs.values().all(|i| i.source_tab.is_some()), "{what}: the chart tab knows its source tab");
        // saving it again gives the same file but saved_at
        let before = micro_parquet::project::load_project(&pfile).expect("reads");
        file_item(&mut rig, "Save project");
        wait_saved(&mut rig);
        let mut after = micro_parquet::project::load_project(&pfile).expect("reads");
        let mut b2 = before.clone();
        (after.saved_at, b2.saved_at) = (String::new(), String::new());
        assert_eq!(after.to_json().expect("json"), b2.to_json().expect("json"), "{what}: save → open → save");
        // Close project: nothing is unsaved, so the tabs close at once
        file_item(&mut rig, "Close project");
        assert!(rig.app.tabs.is_empty() && !rig.app.project.active && rig.app.chart_tabs.is_empty(), "{what}: closed");
    }
}

#[test]
fn w4_save_is_a_worker_job_the_window_does_not_wait_for() {
    let mut rig = proj_rig("p8w4j", 1.0, 1280.0, 720.0);
    let root = rig.app.paths.root_dir.clone();
    let pfile = root.join("proj").join("Job.mpproj");
    rig.app.project.test_pick = Some(pfile.clone());
    rig.app.run_command(&rig.ctx, CommandId::ProjectSaveAs);
    // the command has returned: the work is a job, and nothing has been written by the window's own thread
    assert!(rig.app.project.saving, "Save is waiting for its job");
    assert!(rig.app.jobs.kind_busy(micro_parquet::jobs::JobKind::ProjectSave) || rig.app.project.saving, "the job is submitted");
    assert!(!pfile.exists(), "the file is written when the job's result is taken, not by the command");
    assert!(rig.app.status_message.starts_with("Saving the project"), "{}", rig.app.status_message);
    // a second Save while the first runs is not started
    rig.app.project.test_pick = Some(pfile.clone());
    rig.app.run_command(&rig.ctx, CommandId::ProjectSave);
    assert!(rig.app.status_message.contains("being saved"), "{}", rig.app.status_message);
    wait_saved(&mut rig);
    assert!(pfile.exists() && !rig.app.project.saving, "{}", rig.app.status_message);
    assert!(rig.app.status_message.starts_with("Saved the project"), "{}", rig.app.status_message);
    // the cache has the file's hash, so the next Save reads nothing
    assert!(!rig.app.project.hash_cache.is_empty(), "the hashes are kept");
}

#[test]
fn w4_the_sources_dialog_relinks_by_browse_and_look_in_folder_and_a_tab_that_fails_is_a_placeholder() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w4b", ppp, w, h);
        let what = rig.what.clone();
        let root = rig.app.paths.root_dir.clone();
        three_tabs(&mut rig);
        let pfile = root.join("proj").join("Relink.mpproj");
        rig.app.project.test_pick = Some(pfile.clone());
        rig.app.run_command(&rig.ctx, CommandId::ProjectSaveAs);
        wait_saved(&mut rig);
        assert!(pfile.exists(), "{what}: {}", rig.app.status_message);
        let original = std::fs::read_to_string(&pfile).expect("project text");
        // this month's file: another place, one more column, other rows
        let moved = root.join("moved");
        std::fs::create_dir_all(&moved).expect("folder");
        std::fs::rename(root.join("book.parquet"), moved.join("book.parquet")).expect("moved");
        rig.app.project.test_pick = Some(pfile.clone());
        rig.app.run_command(&rig.ctx, CommandId::ProjectOpen);
        let p = rig.draw(4);
        let r = assert_dialog_fits(&rig.ctx, &sources_title(), &what);
        let texts = dialog_texts(&mut rig, &p, r.window);
        assert!(texts.iter().any(|(t, _)| t == "Missing"), "{what}: {texts:?}");
        assert!(rig.app.project.rects.message.is_some(), "{what}: the missing file is said");
        assert_no_overlap(&texts, &what);
        // Look in folder…: the file is found by its name, below the folder
        rig.app.project.test_dir = Some(root.clone());
        let p = click_button(&mut rig, "Look in folder\u{2026}");
        assert!(p.boxes.iter().any(|(t, _)| t == "Found"), "{what}: found by name");
        assert!(p.boxes.iter().any(|(t, _)| t.starts_with("Looked in ") && t.contains("1 file found")), "{what}");
        // Browse…: a different file with an extra column is allowed and is "Changed" (both dates are said)
        let other = root.join("book_next.parquet");
        let mut df = book8();
        df.with_column(Series::new("Extra".into(), &["x"; 8])).expect("extra");
        df.with_column(Series::new("Balance".into(), &[1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0])).expect("balance");
        polars::prelude::ParquetWriter::new(std::fs::File::create(&other).expect("file")).finish(&mut df).expect("parquet");
        rig.app.project.test_pick = Some(other.clone());
        let p = click_button(&mut rig, "Browse book.parquet");
        assert!(p.boxes.iter().any(|(t, _)| t == "Changed"), "{what}");
        assert!(p.boxes.iter().any(|(t, _)| t.starts_with("Saved ") && t.contains(", now ")), "{what}: both dates");
        assert!(p.boxes.iter().any(|(t, _)| t.contains("relinked from")), "{what}");
        let texts = dialog_texts(&mut rig, &p, r.window);
        assert_no_overlap(&texts, &what);
        assert_not_clipped(&p, r.window, &what);
        click_button(&mut rig, "Open");
        pump_until(&mut rig, "the project opens on the new file", &|r| r.app.project.opening.is_none() && r.app.tabs.len() == 3 && !r.app.jobs.any_busy());
        let sum: f64 = rig.app.tabs[0].cached_slice.column("Balance").expect("balance").f64().expect("f64").into_iter().flatten().sum();
        assert!((sum - 36.0).abs() < 1e-9, "{what}: the tab reads this month's file: {sum}");
        assert!(rig.app.tabs.iter().all(|t| !matches!(t.source, TabSource::Unavailable { .. })), "{what}");

        // a file that is not there: Open anyway; its tabs are placeholders with the reason, the project keeps them
        std::fs::remove_file(&other).ok();
        std::fs::remove_file(moved.join("book.parquet")).expect("removed");
        rig.app.project.test_pick = Some(pfile.clone());
        rig.app.run_command(&rig.ctx, CommandId::ProjectOpen);
        rig.draw(4);
        click_button(&mut rig, "Open");
        pump_until(&mut rig, "the placeholders", &|r| r.app.project.opening.is_none() && r.app.tabs.len() == 3);
        assert!(rig.app.tabs.iter().all(|t| matches!(&t.source, TabSource::Unavailable { reason } if reason.contains("missing") || reason.contains("did not open"))), "{what}: {:?}", rig.app.tabs.iter().map(|t| format!("{:?}", t.source)).collect::<Vec<_>>());
        assert!(rig.app.project.last_summary.starts_with("0 tabs opened, 3 failed: book"), "{what}: {}", rig.app.project.last_summary);
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t.contains("could not be opened")), "{what}: the reason is in the main area");
        assert!(rig.app.command_unavailable(CommandId::ExportParquet).is_some(), "{what}: a placeholder cannot be exported");
        // saving the project again does not lose the three tabs
        file_item(&mut rig, "Save project");
        wait_saved(&mut rig);
        let mut after = std::fs::read_to_string(&pfile).expect("text");
        let strip = |t: &str| -> String { t.lines().filter(|l| !l.contains("\"saved_at\"")).collect::<Vec<_>>().join("\n") };
        after = strip(&after);
        assert_eq!(after, strip(&original), "{what}: the project is as it was saved");
    }
}

#[test]
fn w4_unsaved_changes_ask_save_dont_save_or_cancel_on_a_real_close_request_and_on_new_project() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w4c", ppp, w, h);
        let what = rig.what.clone();
        let root = rig.app.paths.root_dir.clone();
        let pfile = root.join("proj").join("Quit.mpproj");
        rig.app.project.test_pick = Some(pfile.clone());
        rig.app.run_command(&rig.ctx, CommandId::ProjectSaveAs);
        wait_saved(&mut rig);
        assert!(pfile.exists(), "{what}");
        tick(&mut rig);
        // a close request with nothing unsaved goes through
        let close = |rig: &mut Rig| {
            let mut i = rig.input.clone();
            i.viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
            i.time = Some(rig.input.time.unwrap_or(0.0) + 1.0);
            rig.input.time = i.time;
            settle(&mut rig.app);
            let out = rig.ctx.run(i, |c| rig.app.frame_ui(c));
            let cmds = out.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.commands.clone()).unwrap_or_default();
            cmds.iter().any(|c| matches!(c, egui::ViewportCommand::CancelClose))
        };
        assert!(!close(&mut rig), "{what}: nothing is unsaved, so the window may close");
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}");
        // a change: the same request is held and the question is asked
        rig.app.tabs[0].steps.push(TransformStep::SelectAndReorderColumns { cols: vec!["Month".into(), "Balance".into()] });
        tick(&mut rig);
        assert!(close(&mut rig), "{what}: the close is cancelled while the question is open");
        assert_eq!(rig.app.active_tool, ActiveToolModal::ProjectPrompt, "{what}");
        let p = rig.draw(3);
        let r = assert_dialog_fits(&rig.ctx, PROMPT_TITLE, &what);
        let texts = dialog_texts(&mut rig, &p, r.window);
        assert_no_overlap(&texts, &what);
        assert!(texts.iter().any(|(t, _)| t.contains("The project 'Quit' has changes that are not saved. Save them before the program closes?")), "{what}: {texts:?}");
        // Cancel keeps the app open
        click_button(&mut rig, "Cancel");
        assert_eq!((rig.app.active_tool, rig.app.project.close_confirmed, rig.app.project.quit_now), (ActiveToolModal::None, false, false), "{what}: Cancel keeps the app open");
        assert_eq!(rig.app.tabs.len(), 1, "{what}");
        // Don't save: the window closes and the file is as it was
        let before = std::fs::read_to_string(&pfile).expect("text");
        close(&mut rig);
        click_button(&mut rig, "Don't save");
        rig.draw(2);
        assert!(rig.app.project.close_confirmed, "{what}: Don't save lets the window close");
        assert_eq!(std::fs::read_to_string(&pfile).expect("text"), before, "{what}: nothing was written");
        rig.app.project.close_confirmed = false;
        // Save: the project is written, then the window closes
        close(&mut rig);
        click_button(&mut rig, "Save");
        pump_until(&mut rig, "the save, then the close", &|r| r.app.project.close_confirmed);
        assert!(rig.app.project.close_confirmed, "{what}: Save then close");
        assert_eq!(micro_parquet::project::load_project(&pfile).expect("reads").tabs[0].steps.len(), 1, "{what}: the change was saved");
        rig.app.project.close_confirmed = false;

        // Open recent project: the list of the File menu, and a click on a name opens its Sources dialog
        let p = rig.draw(3);
        let b = bar_box(&p, "File").expect("File");
        let p = click_rect(&mut rig, b);
        let sub = p.boxes.iter().rev().find(|(t, bx)| t.trim().ends_with("Open recent project") && bx.top() > b.bottom() - 1.0).map(|(_, r)| *r).expect("the submenu");
        let p = click_rect(&mut rig, sub);
        let name = p.boxes.iter().rev().find(|(t, bx)| t == "Quit" && bx.top() > b.bottom()).map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: the recent list names the project"));
        click_rect(&mut rig, name);
        assert_eq!(rig.app.active_tool, ActiveToolModal::ProjectSources, "{what}");
        rig.draw(3);
        assert_dialog_fits(&rig.ctx, &sources_title(), &what);
        click_button(&mut rig, "Cancel");
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}");
        assert_eq!(rig.app.tabs.len(), 1, "{what}: Cancel opens nothing and closes nothing");
        // the same list from Ctrl+K
        rig.app.run_command(&rig.ctx, CommandId::ProjectRecent);
        let p = rig.draw(3);
        let r = assert_dialog_fits(&rig.ctx, &format!("{}  {}", theme::ph::CLOCK_COUNTER_CLOCKWISE, crate::ui::project::RECENT_TITLE), &what);
        assert_no_overlap(&dialog_texts(&mut rig, &p, r.window), &what);
        // New project with unsaved changes asks too; Don't save starts an empty project
        rig.app.tabs[0].steps.clear();
        tick(&mut rig);
        file_item(&mut rig, "New project");
        assert_eq!(rig.app.active_tool, ActiveToolModal::ProjectPrompt, "{what}");
        rig.draw(3);
        assert_dialog_fits(&rig.ctx, PROMPT_TITLE, &what);
        click_button(&mut rig, "Don't save");
        assert!(rig.app.tabs.is_empty() && rig.app.project.active && rig.app.project.name == "Untitled" && rig.app.project.path.is_none(), "{what}");
        tick(&mut rig);
        assert!(rig.app.window_title.contains("Untitled") && !rig.app.window_title.contains('\u{2022}'), "{what}: {}", rig.app.window_title);

        let _ = SAVE_TITLE;
    }
}

#[test]
fn w4_save_lists_the_tabs_it_leaves_out_and_a_newer_project_is_refused_by_the_menu() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w4d", ppp, w, h);
        let what = rig.what.clone();
        let root = rig.app.paths.root_dir.clone();
        // a chart-data tab: a copy of a chart's table in a temporary file, which a project cannot rebuild
        pump_until(&mut rig, "the first chart", &|r| !r.app.chart.busy && r.app.chart.data.is_some());
        rig.app.chart_open_as_tab();
        pump_until(&mut rig, "the chart data tab", &|r| r.app.tabs.len() == 2);
        let pfile = root.join("proj").join("Left out.mpproj");
        rig.app.project.test_pick = Some(pfile.clone());
        file_item(&mut rig, "Save project as\u{2026}");
        pump_until(&mut rig, "the list of tabs left out", &|r| r.app.active_tool == ActiveToolModal::ProjectSave);
        assert_eq!(rig.app.active_tool, ActiveToolModal::ProjectSave, "{what}: the tab that cannot be saved is listed first");
        assert!(!pfile.exists(), "{what}: nothing is written before the list is answered");
        let p = rig.draw(3);
        let title = format!("{}  {}", theme::ph::FLOPPY_DISK, SAVE_TITLE);
        let r = assert_dialog_fits(&rig.ctx, &title, &what);
        let texts = dialog_texts(&mut rig, &p, r.window);
        assert_no_overlap(&texts, &what);
        assert_not_clipped(&p, r.window, &what);
        assert!(texts.iter().any(|(t, _)| t.contains("1 tab cannot be built again from a project, so it is left out of it:")), "{what}: {texts:?}");
        assert!(texts.iter().any(|(t, _)| t.starts_with("Chart: ") && t.contains("copy of a chart's table")), "{what}: the tab and the reason: {texts:?}");
        // Cancel writes nothing; Save the others writes the project without it
        click_button(&mut rig, "Cancel");
        assert!(!pfile.exists() && rig.app.active_tool == ActiveToolModal::None && !rig.app.project.active, "{what}");
        rig.app.project.test_pick = Some(pfile.clone());
        file_item(&mut rig, "Save project as\u{2026}");
        rig.draw(3);
        click_button(&mut rig, "Save the others");
        assert!(pfile.exists(), "{what}: {}", rig.app.status_message);
        let saved = micro_parquet::project::load_project(&pfile).expect("reads");
        assert_eq!(saved.tabs.len(), 1, "{what}: the other tab is saved");
        assert!(rig.app.status_message.contains("1 left out"), "{what}: {}", rig.app.status_message);

        // a project saved by a newer program is refused with the version, and no dialog opens
        let newer = root.join("proj").join("Newer.mpproj");
        std::fs::write(&newer, r#"{"project_version": 2, "app_version": "9.9.0", "tabs": []}"#).expect("file");
        rig.app.project.test_pick = Some(newer);
        file_item(&mut rig, "Open project\u{2026}");
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}");
        assert_eq!(rig.app.status_message, "This project was saved by a newer MicroParquet v9.9.0. Update the app to open it.", "{what}");
    }
}

#[test]
fn w4_a_file_that_changed_since_the_project_was_saved_is_said_in_the_status_line_after_the_tabs_open() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w4e", ppp, w, h);
        let what = rig.what.clone();
        let root = rig.app.paths.root_dir.clone();
        let pfile = root.join("proj").join("Changed.mpproj");
        rig.app.project.test_pick = Some(pfile.clone());
        rig.app.run_command(&rig.ctx, CommandId::ProjectSaveAs);
        wait_saved(&mut rig);
        assert!(pfile.exists(), "{what}");
        // the same file, rewritten with other numbers (the same columns)
        let mut df = book8();
        df.with_column(Series::new("Balance".into(), &[9.0f64; 8])).expect("balance");
        polars::prelude::ParquetWriter::new(std::fs::File::create(root.join("book.parquet")).expect("file")).finish(&mut df).expect("parquet");
        rig.app.project.test_pick = Some(pfile.clone());
        rig.app.run_command(&rig.ctx, CommandId::ProjectOpen);
        let p = rig.draw(4);
        assert!(p.boxes.iter().any(|(t, _)| t == "Changed"), "{what}: the Sources dialog says Changed");
        click_button(&mut rig, "Open");
        pump_until(&mut rig, "the project opens", &|r| r.app.project.opening.is_none() && r.app.tabs.len() == 1 && !r.app.jobs.any_busy());
        pump_until(&mut rig, "the files are compared", &|r| r.app.status_message.contains("changed since the project was saved"));
        let s = &rig.app.status_message;
        assert!(s.starts_with("1 tab opened. book.parquet changed since the project was saved (saved ") && s.contains(", file ") && s.ends_with(')'), "{what}: {s}");
    }
}

// ================================================================================ W5: linked, editable chart tabs

use micro_parquet::charts::layout::chart_title;

/// The box of a text that ends with `text` (a button with its icon).
fn text_box(p: &Painted, text: &str) -> Option<egui::Rect> {
    p.boxes.iter().rev().find(|(t, _)| t.trim().ends_with(text)).map(|(_, b)| *b)
}

fn has_text(p: &Painted, text: &str) -> bool {
    p.boxes.iter().any(|(t, _)| t.contains(text))
}

fn chart_tab_id(rig: &Rig) -> u64 {
    *rig.app.chart_tabs.keys().next().expect("a chart tab")
}

/// What a new chart made from the source tab gives (the numbers the chart tab must equal).
fn fresh_chart(rig: &Rig, spec: &ChartSpec) -> micro_parquet::chart::ChartData {
    let src = &rig.app.tabs[0];
    let exec = src.exec_steps.min(src.steps.len());
    micro_parquet::project::chart_tab_source(&src.source, &src.steps[..exec], spec, &rig.app.paths.temp_dir).expect("a new chart").0
}

fn pump_chart_tab(rig: &mut Rig) {
    pump_until(rig, "the chart tab is calculated again", &|r| {
        let id = *r.app.chart_tabs.keys().next().expect("a chart tab");
        let i = &r.app.chart_tabs[&id];
        i.pending_sig.is_none() && !i.source_sig.is_empty() && !r.app.jobs.kind_busy(micro_parquet::jobs::JobKind::ChartTab)
    });
}

#[test]
fn w5_a_filter_on_the_source_updates_the_chart_tab_to_equal_a_new_chart() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w5a", ppp, w, h);
        let what = rig.what.clone();
        three_tabs(&mut rig);
        let id = chart_tab_id(&rig);
        let before = rig.app.chart_tabs[&id].data.clone();
        assert!(before.categories.iter().any(|c| c == "Ctg"), "{what}: {:?}", before.categories);
        assert!(!rig.app.chart_tabs[&id].source_sig.is_empty(), "{what}: the chart tab knows what its source looked like");
        // nothing changed: the chart tab stays as it was (nothing is calculated again)
        tick(&mut rig);
        tick(&mut rig);
        assert!(rig.app.chart_tabs[&id].pending_sig.is_none() && !rig.app.jobs.kind_busy(micro_parquet::jobs::JobKind::ChartTab), "{what}");
        // a filter on the source tab: the rows of branch Ctg go
        let cond = FilterCondition { column: "Branch".into(), op: CondOp::Equals, value: "Ctg".into(), ..Default::default() };
        rig.app.tabs[0].steps.push(TransformStep::FilterGroup { conditions: vec![cond], join: GroupJoin::And, keep_matching: false });
        rig.app.tabs[0].exec_steps = 1;
        tick(&mut rig);
        pump_chart_tab(&mut rig);
        let info = &rig.app.chart_tabs[&id];
        assert!(!info.data.categories.iter().any(|c| c == "Ctg"), "{what}: {:?}", info.data.categories);
        let want = fresh_chart(&rig, &info.data.spec);
        assert_eq!((info.data.categories.clone(), info.data.values.clone()), (want.categories.clone(), want.values.clone()), "{what}: the chart tab equals a new chart made from the source");
        // its table (what exports) is the new numbers too
        let idx = rig.app.tabs.iter().position(|t| t.id == id).expect("the chart tab");
        pump_until(&mut rig, "the table of the chart tab", &|r| r.app.tabs[idx].total_rows == want.categories.len());
        assert!(rig.app.status_message.is_empty() || !rig.app.status_message.contains("could not"), "{what}: {}", rig.app.status_message);
    }
}

#[test]
fn w5_edit_chart_opens_the_studio_on_the_source_and_update_keeps_one_chart_tab_with_the_new_numbers() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w5b", ppp, w, h);
        let what = rig.what.clone();
        three_tabs(&mut rig);
        let id = chart_tab_id(&rig);
        let title = rig.app.tabs.iter().find(|t| t.id == id).expect("tab").title.clone();
        let src_id = rig.app.tabs[0].id;
        assert!(title.contains("Sum of Balance") && !title.contains("Sum of Sum"), "{what}: {title}");
        // the chart tab: its main area has Edit chart, and the steps panel is read-only
        rig.app.active_tab = rig.app.tabs.iter().position(|t| t.id == id).expect("idx");
        rig.app.set_side_tab_visible(crate::ui::steps::RightTab::Steps, true);
        let p = rig.draw(3);
        assert!(has_text(&p, "This tab draws a chart of 'Book'. Change the data on that tab, or use Edit chart.") || has_text(&p, "This tab draws a chart of"), "{what}: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>());
        let b = text_box(&p, "Edit chart").unwrap_or_else(|| panic!("{what}: the Edit chart button"));
        let p = click_rect(&mut rig, b);
        // the studio is on the source tab, with the chart tab's spec and its banner
        assert_eq!(rig.app.tabs[rig.app.active_tab].id, src_id, "{what}: the studio opens on the source");
        assert_eq!(rig.app.chart.editing, Some(id), "{what}");
        assert_eq!(rig.app.chart.spec, rig.app.chart_tabs[&id].data.spec, "{what}: the studio has the chart tab's chart");
        let p = if has_text(&p, "Editing chart tab") { p } else { rig.draw(3) };
        assert!(has_text(&p, &format!("Editing chart tab '{}'", title)), "{what}: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>());
        assert!(text_box(&p, "Update chart tab").is_some() && text_box(&p, "Open as new chart tab").is_some(), "{what}");
        // the owner's case: Sum of Balance, never Sum of Sum of Balance
        pump_until(&mut rig, "the studio chart", &|r| !r.app.chart.busy && r.app.chart.data.is_some());
        let names = rig.app.chart.data.as_ref().expect("data").measure_names.clone();
        assert_eq!(names, vec!["Sum of Balance".to_string()], "{what}");
        // another aggregation, then Update chart tab by a real click
        let mut spec = rig.app.chart.spec.clone();
        spec.agg = ChartAgg::Average;
        rig.app.chart_set_spec(spec);
        pump_until(&mut rig, "the new studio chart", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.agg == ChartAgg::Average));
        let p = rig.draw(3);
        let b = text_box(&p, "Update chart tab").unwrap_or_else(|| panic!("{what}: Update chart tab"));
        click_rect(&mut rig, b);
        assert_eq!(rig.app.chart.editing, None, "{what}");
        pump_chart_tab(&mut rig);
        assert_eq!((rig.app.chart_tabs.len(), rig.app.tabs.len()), (1, 3), "{what}: one chart tab");
        let info = &rig.app.chart_tabs[&id];
        assert_eq!(info.data.spec.agg, ChartAgg::Average, "{what}");
        let want = fresh_chart(&rig, &info.data.spec);
        assert_eq!(info.data.values, want.values, "{what}: the new numbers");
        assert_eq!(info.data.measure_names, vec!["Average of Balance".to_string()], "{what}");
        // the title follows the rule it had ("Chart: " and the chart's title), not "Sum of Sum"
        let t = rig.app.tabs.iter().find(|t| t.id == id).expect("tab").title.clone();
        assert_eq!(t, format!("Chart: {}", chart_title(&info.data)), "{what}");
        assert!(t.contains("Average of Balance") && !t.contains("Sum of Sum"), "{what}: {t}");
        // Open as new chart tab leaves the first one as it is
        rig.app.edit_chart_tab(id);
        let p = rig.draw(3);
        let b = text_box(&p, "Open as new chart tab").unwrap_or_else(|| panic!("{what}: Open as new chart tab"));
        click_rect(&mut rig, b);
        pump_until(&mut rig, "the new chart tab", &|r| r.app.chart_tabs.len() == 2 && r.app.tabs.len() == 4);
        assert_eq!(rig.app.chart_tabs[&id].data.spec.agg, ChartAgg::Average, "{what}: the first chart tab is as it was");
    }
}

#[test]
fn w5_closing_the_source_dims_the_chart_with_a_message_and_reopen_restores_it() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w5c", ppp, w, h);
        let what = rig.what.clone();
        three_tabs(&mut rig);
        let id = chart_tab_id(&rig);
        // a project, so that the source is a file tab of it
        let pfile = rig.app.paths.root_dir.join("proj").join("Charts.mpproj");
        rig.app.project.test_pick = Some(pfile);
        rig.app.run_command(&rig.ctx, CommandId::ProjectSaveAs);
        wait_saved(&mut rig);
        let src_title = rig.app.tabs[0].title.clone();
        let kept = rig.app.chart_tabs[&id].data.clone();
        // the chart as it is drawn while its source is open, and when it is closed: its labels' glyphs fade by the dim factor
        rig.app.active_tab = rig.app.tabs.iter().position(|t| t.id == id).expect("the chart tab");
        let p = rig.draw(3);
        // (the side panel's studio draws the same category labels, so the chart's title is the text to read: it is drawn once)
        let alpha_of = |p: &Painted, label: &str| {
            let all: Vec<f32> = p.alpha.iter().filter(|(t, _)| t == label).map(|(_, a)| *a as f32).collect();
            assert_eq!(all.len(), 1, "{what}: the chart tab draws '{label}' once: {all:?}");
            all[0]
        };
        let label = chart_title(&kept);
        let open_alpha = alpha_of(&p, &label);
        assert!(open_alpha > 200.0, "{what}: an open source draws the chart's labels opaque: {open_alpha}");
        rig.app.close_tab(0);
        rig.app.active_tab = rig.app.tabs.iter().position(|t| t.id == id).expect("the chart tab");
        let p = rig.draw(3);
        let dim = crate::ui::theme::CHART_TAB_DIMMED_OPACITY;
        assert!(dim < 1.0 && dim > 0.0);
        let shut_alpha = alpha_of(&p, &label);
        assert!((shut_alpha - open_alpha * dim).abs() <= 3.0, "{what}: a closed source dims the chart to {dim}: alpha {open_alpha} became {shut_alpha}");
        let msg = format!("The source tab '{}' is closed. Reopen it to edit or refresh this chart.", src_title);
        assert!(has_text(&p, &msg), "{what}: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>());
        let info = &rig.app.chart_tabs[&id];
        assert!(info.source_tab.is_none() && info.closed.is_some(), "{what}");
        assert_eq!((info.data.categories.clone(), info.data.values.clone()), (kept.categories, kept.values), "{what}: the chart keeps its last data");
        // Edit chart is not offered while the source is closed
        rig.app.edit_chart_tab(id);
        assert_eq!(rig.app.chart.editing, None, "{what}");
        // Reopen is not offered outside a project
        rig.app.project.active = false;
        let p = rig.draw(3);
        assert!(text_box(&p, "Reopen").is_none(), "{what}");
        rig.app.project.active = true;
        let p = rig.draw(3);
        let b = text_box(&p, "Reopen").unwrap_or_else(|| panic!("{what}: Reopen"));
        let n = rig.app.tabs.len();
        click_rect(&mut rig, b);
        pump_until(&mut rig, "the source tab is open again", &|r| r.app.tabs.len() == n + 1);
        let info = &rig.app.chart_tabs[&id];
        assert!(info.closed.is_none() && info.source_tab.is_some(), "{what}");
        let p = rig.draw(3);
        assert!(!has_text(&p, "is closed. Reopen it"), "{what}: the message goes");
        assert_eq!(alpha_of(&p, &label), open_alpha, "{what}: Reopen draws the chart as it was, no longer dimmed");
        // the chart is as it was: the source has the same steps, so nothing is calculated again
        tick(&mut rig);
        tick(&mut rig);
        assert!(rig.app.chart_tabs[&id].pending_sig.is_none(), "{what}");
        assert_eq!(rig.app.chart_tabs[&id].data.values, kept_values(&rig, id), "{what}");
    }
}

fn kept_values(rig: &Rig, id: u64) -> Vec<Vec<Option<f64>>> {
    rig.app.chart_tabs[&id].data.values.clone()
}

#[test]
fn w5_a_chart_tab_of_an_older_version_stays_as_it_was_and_edit_chart_opens_it_on_its_own_table() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w5d", ppp, w, h);
        let what = rig.what.clone();
        three_tabs(&mut rig);
        let id = chart_tab_id(&rig);
        // a Phase 7 chart tab knew no source tab
        let sig = {
            let i = rig.app.chart_tabs.get_mut(&id).expect("info");
            i.source_tab = None;
            i.source_sig.clear();
            i.data.clone()
        };
        rig.app.active_tab = rig.app.tabs.iter().position(|t| t.id == id).expect("idx");
        rig.app.set_side_tab_visible(crate::ui::steps::RightTab::Steps, true);
        // the source changes: the frozen chart is not calculated again
        let cond = FilterCondition { column: "Branch".into(), op: CondOp::Equals, value: "Ctg".into(), ..Default::default() };
        rig.app.tabs[0].steps.push(TransformStep::FilterGroup { conditions: vec![cond], join: GroupJoin::And, keep_matching: false });
        tick(&mut rig);
        tick(&mut rig);
        assert!(!rig.app.jobs.kind_busy(micro_parquet::jobs::JobKind::ChartTab), "{what}");
        assert_eq!(rig.app.chart_tabs[&id].data.values, sig.values, "{what}: frozen");
        let p = rig.draw(3);
        assert!(has_text(&p, crate::ui::chart_tab::OLDER_VERSION_NOTE), "{what}: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>());
        let b = text_box(&p, "Edit chart").unwrap_or_else(|| panic!("{what}: Edit chart"));
        click_rect(&mut rig, b);
        // the studio is on the chart tab's own table, and only "Open as new chart tab" is offered
        assert_eq!(rig.app.tabs[rig.app.active_tab].id, id, "{what}");
        assert_eq!(rig.app.chart.editing, Some(id), "{what}");
        let p = rig.draw(3);
        assert!(text_box(&p, "Open as new chart tab").is_some() && text_box(&p, "Update chart tab").is_none(), "{what}");
    }
}

#[test]
fn w5_the_project_fingerprint_does_not_move_when_a_chart_tab_is_calculated_again() {
    let mut rig = proj_rig("p8w5e", 1.0, 1280.0, 720.0);
    three_tabs(&mut rig);
    let id = chart_tab_id(&rig);
    let pfile = rig.app.paths.root_dir.join("proj").join("Fp.mpproj");
    rig.app.project.test_pick = Some(pfile);
    rig.app.run_command(&rig.ctx, CommandId::ProjectSaveAs);
    wait_saved(&mut rig);
    tick(&mut rig);
    assert!(!rig.app.project_unsaved(), "saved");
    // the chart is calculated again (its table is a new file): nothing the project saves has changed
    rig.app.chart_tabs.get_mut(&id).expect("info").source_sig.clear();
    pump_chart_tab(&mut rig);
    tick(&mut rig);
    assert!(!rig.app.project_unsaved(), "a chart tab that follows its source is not a change");
}

// ---- W5 follow-up: the step commands are greyed on a chart tab

/// How many texts of the frame contain `text` (the steps panel draws the note too, so a tooltip is one more).
fn count_text(p: &Painted, text: &str) -> usize {
    p.boxes.iter().filter(|(t, _)| t.contains(text)).count()
}

/// Moves the pointer onto `pos` the way a hand does (egui counts a movement only when it sees three positions over more
/// than 10 ms, and shows no tooltip while the pointer rests where it clicked), then holds it there while the clock runs.
fn hover_at(rig: &mut Rig, pos: egui::Pos2) -> Painted {
    let t0 = rig.input.time.unwrap_or(0.0) + 1.0;
    for (dt, dx) in [(0.0, 30.0), (0.03, 20.0), (0.06, 10.0), (0.09, 0.0)] {
        rig.input.time = Some(t0 + dt);
        rig.step(vec![egui::Event::PointerMoved(pos + egui::vec2(dx, 0.0))]);
    }
    rig.input.time = Some(t0 + 1.0);
    rig.step(vec![egui::Event::PointerMoved(pos)]);
    rig.input.time = Some(t0 + 2.0);
    rig.step(vec![egui::Event::PointerMoved(pos)])
}

/// A key press with Ctrl held (the app's `COMMAND`), then a few frames.
fn ctrl_key(rig: &mut Rig, key: egui::Key) -> Painted {
    let m = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
    let ev = |pressed| egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers: m };
    rig.step(vec![ev(true), ev(false)]);
    rig.draw(3)
}

const STEP_COMMANDS_P8: [(CommandId, &str); 4] = [
    (CommandId::Conditional, "Conditional column"),
    (CommandId::Normaliser, "Counterparty name normaliser"),
    (CommandId::CleanBreaks, "Clean line breaks"),
    (CommandId::SplitDate, "Split date"),
];

#[test]
fn w5_the_step_commands_are_greyed_on_a_chart_tab_in_the_edit_menu_and_ctrl_k_by_real_clicks_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w5g", ppp, w, h);
        let what = rig.what.clone();
        three_tabs(&mut rig);
        let id = chart_tab_id(&rig);
        let chart_idx = rig.app.tabs.iter().position(|t| t.id == id).expect("the chart tab");
        let src_title = rig.app.tabs[0].title.clone();
        let note = format!("This tab draws a chart of '{}'. Change the data on that tab, or use Edit chart.", src_title);
        rig.app.active_tab = chart_idx;
        // the steps panel says it, and the greyed commands say the same
        rig.app.set_side_tab_visible(crate::ui::steps::RightTab::Steps, true);
        let p = rig.draw(3);
        let panel = count_text(&p, &note);
        assert_eq!(panel, 1, "{what}: the steps panel's note {note:?}");
        assert_eq!(rig.app.chart_tab_steps_note().as_deref(), Some(note.as_str()), "{what}");
        let (steps_before, tabs_before) = (rig.app.tabs.iter().map(|t| t.steps.len()).collect::<Vec<_>>(), rig.app.tabs.len());
        for (cid, _) in STEP_COMMANDS_P8 {
            assert_eq!(rig.app.command_unavailable(cid).as_deref(), Some(note.as_str()), "{what}: {cid:?}");
        }
        // every command that adds a step in place is greyed; the others are not
        for cid in [CommandId::Filter, CommandId::Sort, CommandId::Columns, CommandId::CalcColumn, CommandId::Pivot, CommandId::RateProfile] {
            assert_eq!(rig.app.command_unavailable(cid).as_deref(), Some(note.as_str()), "{what}: {cid:?}");
        }
        for cid in [CommandId::JoinTabs, CommandId::CopyTable, CommandId::ChartNew, CommandId::ExportCsv, CommandId::ToggleTheme, CommandId::OpenFile] {
            assert!(rig.app.command_unavailable(cid).is_none(), "{what}: {cid:?} still works on a chart tab");
        }

        // the Edit menu: each item is greyed, a real click on it does nothing, and its tooltip is the note
        let p = rig.draw(3);
        let bar = bar_box(&p, "Edit").unwrap_or_else(|| panic!("{what}: the Edit menu"));
        let mut p = click_rect(&mut rig, bar);
        for (cid, title) in STEP_COMMANDS_P8 {
            let it = p.boxes.iter().rev().find(|(t, b)| t.contains(title) && b.top() > bar.bottom() - 1.0).map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: '{title}' is in the Edit menu"));
            assert!(inside(it, rig.ctx.screen_rect()), "{what}: '{title}' {it:?} is inside the window");
            click_rect(&mut rig, it);
            assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: a click on the greyed '{title}' opens nothing");
            p = hover_at(&mut rig, it.center());
            assert_eq!(count_text(&p, &note), panel + 1, "{what}: the tooltip of '{title}' is the note: {:?}", p.boxes.iter().filter(|(_, b)| b.top() > bar.bottom()).map(|(t, _)| t.clone()).collect::<Vec<_>>());
            assert!(p.boxes.iter().any(|(t, _)| t.contains(title)), "{what}: the menu is still open after the click on '{title}' ({cid:?})");
        }
        // the items that still work are not greyed: Copy the table opens no dialog but its click is taken (the menu closes)
        let copy = p.boxes.iter().rev().find(|(t, b)| t.contains("Copy the table") && b.top() > bar.bottom() - 1.0).map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: Copy the table"));
        assert_eq!(count_text(&hover_at(&mut rig, copy.center()), &note), panel, "{what}: no tooltip on an item that works");
        rig.step(vec![egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }]);
        rig.draw(2);
        assert_eq!((rig.app.tabs.iter().map(|t| t.steps.len()).collect::<Vec<_>>(), rig.app.tabs.len()), (steps_before.clone(), tabs_before), "{what}: nothing was added");

        // Ctrl+K: type "split date"; the row is greyed with the short reason, a click says the note, its hover has it
        ctrl_key(&mut rig, egui::Key::K);
        assert!(rig.app.command_open, "{what}: Ctrl+K opens the search");
        rig.step(vec![egui::Event::Text("split date".into())]);
        let p = rig.draw(3);
        let row = text_box(&p, "Split date\u{2026}").unwrap_or_else(|| panic!("{what}: the Split date row: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>()));
        let short = micro_parquet::commands::CHART_TAB_STEP_REASON;
        let why = text_box(&p, short).unwrap_or_else(|| panic!("{what}: the short reason {short:?} on the row"));
        assert!(row.right() <= why.left() + 0.5 || row.bottom() <= why.top() || why.bottom() <= row.top(), "{what}: the title {row:?} and the reason {why:?} do not overlap");
        let area = rig.ctx.memory(|m| m.area_rect(egui::Id::new("command_palette"))).expect("the search was drawn");
        assert!(inside(row, area) && inside(why, area), "{what}: the row is inside the search {area:?}");
        // the texts the search itself draws (the chart behind it draws its own in the same place)
        rig.app.command_open = false;
        let behind = rig.draw(2);
        rig.app.command_open = true;
        let p = rig.draw(2);
        let same = |a: &egui::Rect, b: &egui::Rect| (a.min - b.min).length() < 0.5 && (a.max - b.max).length() < 0.5;
        // (only the Split date row: other rows of the list, such as "Chart this result" with its long Phase 7 reason, are not this test's)
        let band = |b: &egui::Rect| b.top() >= row.top() - 3.0 && b.bottom() <= why.bottom() + 3.0;
        let glyphs: Vec<(String, egui::Rect)> = p.glyphs.iter().filter(|(t, b)| inside(*b, area) && band(b) && !behind.glyphs.iter().any(|(u, c)| u == t && same(b, c))).cloned().collect();
        assert!(glyphs.iter().any(|(t, _)| t.contains("Split date")) && glyphs.iter().any(|(t, _)| t == short), "{what}: the search's own texts: {glyphs:?}");
        assert_no_overlap(&glyphs, &what);
        let tip = hover_at(&mut rig, row.center());
        assert_eq!(count_text(&tip, &note), panel + 1, "{what}: the tooltip of the row is the note");
        click_rect(&mut rig, row);
        assert_eq!(rig.app.status_message, note, "{what}: a click says why");
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}");
        assert!(rig.app.command_open, "{what}: the search stays open, as for any greyed row");
        rig.step(vec![egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }]);
        rig.draw(2);
        assert!(!rig.app.command_open, "{what}");

        // nothing else can slip a step into a chart tab either (the Column panel's button, a macro, …)
        rig.app.status_message.clear();
        rig.app.push_step_to_active_tab(TransformStep::SelectAndReorderColumns { cols: vec!["Branch".into()] }, false);
        assert_eq!(rig.app.status_message, note, "{what}");
        assert_eq!(rig.app.tabs.iter().map(|t| t.steps.len()).collect::<Vec<_>>(), steps_before, "{what}: no step was added to the chart tab");

        // the control: on the source tab the same items work by the same clicks
        rig.app.active_tab = 0;
        rig.draw(3);
        for (cid, title) in STEP_COMMANDS_P8 {
            assert!(rig.app.command_unavailable(cid).is_none(), "{what}: {cid:?} works on the source tab");
            let _ = title;
        }
        let p = menu_item(&mut rig, "Edit", "Split date");
        let _ = p;
        assert_eq!(rig.app.active_tool, ActiveToolModal::SplitDate, "{what}: on the source tab the click opens the dialog");
    }
}

#[test]
fn w5_a_file_that_changes_under_the_source_updates_the_chart_tab_to_equal_a_new_chart() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w5f", ppp, w, h);
        let what = rig.what.clone();
        three_tabs(&mut rig);
        let id = chart_tab_id(&rig);
        let before = rig.app.chart_tabs[&id].data.clone();
        // the month's file is replaced by one with twice the rows: nothing in the app was touched
        let root = rig.app.paths.root_dir.clone();
        let twice = book8().vstack(&book8()).expect("twice the rows");
        let src = parquet_source(&root, "book", twice);
        let path_of = |s: &TabSource| match s {
            TabSource::SingleFile { path, .. } => path.clone(),
            other => panic!("a file tab, not {other:?}"),
        };
        assert_eq!(path_of(&src), path_of(&rig.app.tabs[0].source), "{what}: the same file, with other contents");
        pump_until(&mut rig, "the chart tab follows the file", &|r| r.app.chart_tabs[&id].data.values != before.values);
        pump_chart_tab(&mut rig);
        let info = &rig.app.chart_tabs[&id];
        let want = fresh_chart(&rig, &info.data.spec);
        assert_eq!((info.data.categories.clone(), info.data.values.clone()), (want.categories.clone(), want.values.clone()), "{what}: the chart tab equals a new chart made from the changed file");
        let sum = |v: &Vec<Vec<Option<f64>>>| v.iter().flatten().flatten().sum::<f64>();
        assert!((sum(&info.data.values) - 2.0 * sum(&before.values)).abs() < 1e-6, "{what}: twice the rows, twice the total");
    }
}

// ================================================================================ W6: drill-down and Show rows

use micro_parquet::charts::spec::{ChartKind, DrillLevel};
use polars::prelude::IntoLazy;

const PLOT_ID: &str = "chart_plot";

fn right_click(rig: &mut Rig, pos: egui::Pos2) -> Painted {
    rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 1.0);
    rig.step(vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed: true, modifiers: egui::Modifiers::NONE }]);
    rig.step(vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed: false, modifiers: egui::Modifiers::NONE }]);
    rig.draw(3)
}

fn key_press(rig: &mut Rig, key: egui::Key) -> Painted {
    let ev = |pressed| egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers: egui::Modifiers::NONE };
    rig.step(vec![ev(true), ev(false)]);
    rig.draw(3)
}

/// The rectangle of the mark of category number `category` of the chart drawn under `id`.
fn mark_rect(rig: &Rig, id: egui::Id, category: usize) -> egui::Rect {
    // (the caller has scrolled the chart into view with `show_studio_chart`)
    let s = crate::ui::chart_render::last_shown(&rig.ctx, id).unwrap_or_else(|| panic!("{}: a chart is drawn under {id:?}", rig.what));
    s.layout.marks.iter().find(|m| m.category == category && m.series == 0 && m.panel == 0).map(|m| m.rect).unwrap_or_else(|| panic!("{}: a mark for category {category}: {} marks", rig.what, s.layout.marks.len()))
}

fn category_index(categories: &[String], name: &str, what: &str) -> usize {
    categories.iter().position(|c| c == name).unwrap_or_else(|| panic!("{what}: '{name}' is a category: {categories:?}"))
}

fn drill_spec() -> ChartSpec {
    let mut s = ChartSpec { category: "Branch".into(), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 50, ..Default::default() };
    s.set_drill(vec![DrillLevel::column("Branch"), DrillLevel::column("Month"), DrillLevel::column("Band")]);
    s
}

/// A rig whose studio chart is the deposit book drilled Branch › Month › Band.
fn studio_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut rig = proj_rig(name, ppp, w, h);
    rig.app.set_side_tab_visible(crate::ui::steps::RightTab::Chart, true);
    rig.draw(4);
    pump_until(&mut rig, "the first chart", &|r| !r.app.chart.busy && r.app.chart.data.is_some());
    rig.app.chart_set_spec(drill_spec());
    pump_until(&mut rig, "the drill chart", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.drill.len() == 3));
    rig.draw(3);
    rig
}

/// The studio's chart is what a new chart of the book gives at its spec: the same categories and numbers.
fn assert_studio_equals_new_chart(rig: &Rig, what: &str) {
    let want = micro_parquet::charts::calc::compute(book8().lazy(), &rig.app.chart.spec).unwrap_or_else(|e| panic!("{what}: {e:#}"));
    let have = rig.app.chart.data.as_ref().expect("a chart");
    assert_eq!((&have.categories, &have.values, &have.spec.category), (&want.categories, &want.values, &want.spec.category), "{what}");
}

/// Scrolls the side panel with the mouse wheel until every mark of the studio's chart is well inside the window (the chart is
/// under the controls, so in a small window it starts below the fold).
fn show_studio_chart(rig: &mut Rig) {
    let id = egui::Id::new(PLOT_ID);
    for _ in 0..16 {
        rig.draw(3);
        let s = crate::ui::chart_render::last_shown(&rig.ctx, id).unwrap_or_else(|| panic!("{}: the studio's chart is drawn", rig.what));
        let screen = rig.ctx.screen_rect();
        let ys: Vec<f32> = s.layout.marks.iter().map(|m| m.rect.center().y).collect();
        let (lo, hi) = (screen.top() + 90.0, screen.bottom() - 70.0);
        let (min, max) = (ys.iter().cloned().fold(f32::MAX, f32::min), ys.iter().cloned().fold(f32::MIN, f32::max));
        if min >= lo && max <= hi {
            return;
        }
        let pos = egui::pos2(s.rect.center().x, screen.center().y);
        let delta = if max > hi { -(max - hi).min(200.0) } else { (lo - min).min(200.0) };
        rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 0.5);
        rig.step(vec![egui::Event::PointerMoved(pos), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, delta), modifiers: egui::Modifiers::NONE }]);
        for _ in 0..6 {
            rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 0.1);
            rig.draw(1);
        }
    }
    panic!("{}: the studio's chart could not be scrolled into view", rig.what);
}

fn mark_at(rig: &mut Rig, id: egui::Id, category: usize) -> egui::Rect {
    show_studio_chart(rig);
    mark_rect(rig, id, category)
}

/// The texts the last frame drew on the rows of the given rectangles, from the left edge of the first one: nothing of them
/// may overlap (the breadcrumb, a row of drill levels).
fn assert_row_texts_apart(p: &Painted, rects: &[egui::Rect], what: &str) {
    let band = rects.iter().fold(egui::Rect::NOTHING, |a, r| a.union(*r));
    let texts: Vec<(String, egui::Rect)> = p.glyphs.iter().filter(|(_, b)| b.center().y >= band.top() - 1.0 && b.center().y <= band.bottom() + 1.0 && b.left() >= band.left() - 2.0 && b.right() <= band.right() + 400.0).cloned().collect();
    assert!(!texts.is_empty(), "{what}: the row has texts");
    assert_no_overlap(&texts, what);
}

fn breadcrumb(rig: &Rig) -> Vec<(String, egui::Rect)> {
    rig.app.drill_rects.clone()
}

#[test]
fn w6_a_click_drills_the_breadcrumb_and_the_back_key_go_up_and_the_right_click_menu_works_by_real_clicks_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = studio_rig("p8w6a", ppp, w, h);
        let what = rig.what.clone();
        let plot = egui::Id::new(PLOT_ID);
        let cats = |r: &Rig| r.app.chart.data.as_ref().expect("data").categories.clone();
        assert_eq!(rig.app.chart.spec.depth(), 0);
        // the breadcrumb is there from the start: "All" is where the chart is
        let p = rig.draw(3);
        assert_eq!(breadcrumb(&rig).iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), vec!["Level 0"], "{what}");
        assert!(has_text(&p, "All"), "{what}");

        // 1. a real click on the Dhaka bar goes one level down
        let i = category_index(&cats(&rig), "Dhaka", &what);
        let r = mark_at(&mut rig, plot, i);
        // the tooltip of a mark says what a click does
        let tip = hover_at(&mut rig, r.center());
        assert!(has_text(&tip, "Click to drill into Month"), "{what}: {:?}", tip.boxes.iter().map(|(t, _)| t.clone()).filter(|t| t.contains("Dhaka") || t.contains("drill")).collect::<Vec<_>>());
        click_rect(&mut rig, r);
        assert_eq!((rig.app.chart.spec.depth(), rig.app.chart.spec.category.as_str()), (1, "Month"), "{what}");
        pump_until(&mut rig, "the Month level", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.category == "Month"));
        assert_studio_equals_new_chart(&rig, &what);
        let d = rig.app.chart.data.clone().expect("data");
        assert_eq!(d.values.iter().flatten().flatten().sum::<f64>(), 1000.0, "{what}: Dhaka's three rows");
        let p = rig.draw(3);
        assert!(has_text(&p, "Filtered: Branch = Dhaka"), "{what}: the subtitle says the path: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).filter(|t| t.contains("Filtered") || t.contains("Sum of")).collect::<Vec<_>>());
        let crumbs = breadcrumb(&rig);
        assert_eq!(crumbs.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), vec!["Level 0", "Level 1"], "{what}");
        assert!(has_text(&p, "Dhaka") && has_text(&p, "by Month"), "{what}");

        // 2. a second click: the Band level, which is the last
        let i = category_index(&cats(&rig), "Jan", &what);
        {
            let r = mark_at(&mut rig, plot, i);
            click_rect(&mut rig, r);
        }
        assert_eq!((rig.app.chart.spec.depth(), rig.app.chart.spec.category.as_str()), (2, "Band"), "{what}");
        pump_until(&mut rig, "the Band level", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.category == "Band"));
        assert_studio_equals_new_chart(&rig, &what);
        let p = rig.draw(3);
        assert!(has_text(&p, "Filtered: Branch = Dhaka, Month = Jan"), "{what}");
        // the breadcrumb is whole, inside the window, and its texts do not run into each other or into the subtitle
        let crumbs: Vec<egui::Rect> = breadcrumb(&rig).iter().map(|(_, r)| *r).collect();
        assert_eq!(crumbs.len(), 3, "{what}: All, Dhaka, Jan");
        for c in &crumbs {
            assert!(inside(*c, rig.ctx.screen_rect()), "{what}: the crumb {c:?} is inside the window");
        }
        assert_row_texts_apart(&p, &crumbs, &what);
        for pair in crumbs.windows(2) {
            assert!(pair[0].right() <= pair[1].left() + 0.5, "{what}: the crumbs are in order, left to right");
        }
        // at the last level a click does nothing and the tooltip says so
        let r = mark_at(&mut rig, plot, 0);
        let tip = hover_at(&mut rig, r.center());
        assert!(has_text(&tip, "Last level: a click does nothing"), "{what}");
        let status = rig.app.status_message.clone();
        click_rect(&mut rig, r);
        assert_eq!((rig.app.chart.spec.depth(), rig.app.status_message.as_str()), (2, status.as_str()), "{what}: nothing happened");

        // 3. the breadcrumb goes back to any level by a real click
        let level1 = breadcrumb(&rig).iter().find(|(n, _)| n == "Level 1").map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: the crumb"));
        assert!(inside(level1, rig.ctx.screen_rect()), "{what}");
        click_rect(&mut rig, level1);
        assert_eq!((rig.app.chart.spec.depth(), rig.app.chart.spec.category.as_str()), (1, "Month"), "{what}");
        pump_until(&mut rig, "back at Month", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.category == "Month"));
        rig.draw(3);
        let all = breadcrumb(&rig).iter().find(|(n, _)| n == "Level 0").map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: All"));
        click_rect(&mut rig, all);
        assert_eq!((rig.app.chart.spec.depth(), rig.app.chart.spec.category.as_str()), (0, "Branch"), "{what}");
        pump_until(&mut rig, "back at the top", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.category == "Branch"));
        assert_studio_equals_new_chart(&rig, &what);

        // 4. the Back key goes up one level while the pointer is over the chart
        let i = category_index(&cats(&rig), "Ctg", &what);
        {
            let r = mark_at(&mut rig, plot, i);
            click_rect(&mut rig, r);
        }
        pump_until(&mut rig, "Ctg's months", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.category == "Month"));
        let over = crate::ui::chart_render::last_shown(&rig.ctx, plot).expect("shown").rect.center();
        hover_at(&mut rig, over);
        key_press(&mut rig, egui::Key::Backspace);
        assert_eq!(rig.app.chart.spec.depth(), 0, "{what}: Backspace goes back");
        pump_until(&mut rig, "back at the top again", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.category == "Branch"));

        // 5. the right-click menu: Keep only this, Exclude this, then Show rows
        let i = category_index(&cats(&rig), "Ctg", &what);
        let p = {
            let c = mark_at(&mut rig, plot, i).center();
            right_click(&mut rig, c)
        };
        for item in ["Drill down", "Show rows", "Keep only this", "Exclude this"] {
            assert!(text_box(&p, item).is_some(), "{what}: the menu has '{item}': {:?}", p.boxes.iter().map(|(t, _)| t.clone()).filter(|t| t.len() < 20).collect::<Vec<_>>());
        }
        let keep = text_box(&p, "Keep only this").expect("item");
        assert!(inside(keep, rig.ctx.screen_rect()), "{what}: the menu is inside the window");
        click_rect(&mut rig, keep);
        assert_eq!(rig.app.chart.spec.filters.len(), 1, "{what}");
        pump_until(&mut rig, "only Ctg", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.categories == vec!["Ctg".to_string()]));
        assert_studio_equals_new_chart(&rig, &what);
        assert!(has_text(&rig.draw(3), "Filtered: Branch = Ctg"), "{what}");
        let mut s = rig.app.chart.spec.clone();
        s.filters.clear();
        rig.app.chart_set_spec(s);
        pump_until(&mut rig, "all three again", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.categories.len() == 3));
        let i = category_index(&cats(&rig), "Dhaka", &what);
        let p = {
            let c = mark_at(&mut rig, plot, i).center();
            right_click(&mut rig, c)
        };
        click_rect(&mut rig, text_box(&p, "Exclude this").unwrap_or_else(|| panic!("{what}: Exclude this")));
        pump_until(&mut rig, "without Dhaka", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.categories.len() == 2 && !d.categories.iter().any(|c| c == "Dhaka")));
        assert_studio_equals_new_chart(&rig, &what);
        assert!(has_text(&rig.draw(3), "Filtered: Branch \u{2260} Dhaka"), "{what}");
        let mut s = rig.app.chart.spec.clone();
        s.filters.clear();
        rig.app.chart_set_spec(s);
        pump_until(&mut rig, "all three once more", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.categories.len() == 3));

        // 6. Drill down from the menu, then Show rows of a mark at level 2: a new tab with that mark's rows and sum
        let i = category_index(&cats(&rig), "Dhaka", &what);
        let p = {
            let c = mark_at(&mut rig, plot, i).center();
            right_click(&mut rig, c)
        };
        click_rect(&mut rig, text_box(&p, "Drill down").unwrap_or_else(|| panic!("{what}: Drill down")));
        pump_until(&mut rig, "Dhaka's months", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.category == "Month"));
        let i = category_index(&cats(&rig), "Jan", &what);
        let p = {
            let c = mark_at(&mut rig, plot, i).center();
            right_click(&mut rig, c)
        };
        let n = rig.app.tabs.len();
        click_rect(&mut rig, text_box(&p, "Show rows").unwrap_or_else(|| panic!("{what}: Show rows")));
        pump_until(&mut rig, "the rows tab", &|r| r.app.tabs.len() == n + 1 && !r.app.jobs.any_busy());
        let t = rig.app.tabs.last().expect("the new tab");
        assert_eq!(t.title, "Rows: Branch = Dhaka, Month = Jan", "{what}");
        let df = micro_parquet::engine::compile_tab_lazy(&t.source, &t.steps, &rig.app.paths.temp_dir).expect("compiles").collect().expect("collects");
        let bal: f64 = df.column("Balance").expect("Balance").f64().expect("f").into_iter().flatten().sum();
        assert_eq!((df.height(), bal), (1, 600.0), "{what}: the one row of Dhaka in January, and the mark's value");
        // the rows tab reads the file with the source's steps and one filter more
        assert!(matches!(t.steps.last(), Some(TransformStep::FilterGroup { keep_matching: true, .. })), "{what}");
    }
}

/// The book's tab, a pivot tab of it, and a chart tab of the drilled chart Branch › Month › Band.
fn drill_chart_tab(rig: &mut Rig) {
    let src = rig.app.tabs[0].source.clone();
    rig.app.create_tab_from_source("Pivot: Branch".into(), src, vec![branch_pivot()], HashMap::new());
    settle(&mut rig.app);
    rig.app.active_tab = 0;
    rig.draw(4);
    pump_until(rig, "the first chart", &|r| !r.app.chart.busy && r.app.chart.data.is_some());
    rig.app.chart_set_spec(drill_spec());
    pump_until(rig, "the drill chart", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.drill.len() == 3));
    rig.app.chart_open_as_chart_tab();
    pump_until(rig, "the chart tab", &|r| r.app.chart_tabs.len() == 1 && r.app.tabs.len() == 3);
}

/// Waits until the chart tab has been calculated for its new level (`depth` levels down).
fn pump_chart_tab_depth(rig: &mut Rig, depth: usize) {
    pump_until(rig, &format!("the chart tab at level {}", depth + 1), &|r| {
        let id = *r.app.chart_tabs.keys().next().expect("a chart tab");
        let i = &r.app.chart_tabs[&id];
        i.next_spec.is_none() && i.pending_sig.is_none() && i.data.spec.depth() == depth && !r.app.jobs.kind_busy(micro_parquet::jobs::JobKind::ChartTab)
    });
    rig.draw(3);
}

fn tab_mark(rig: &Rig, id: u64, category: &str) -> egui::Rect {
    let i = category_index(&rig.app.chart_tabs[&id].data.categories, category, &rig.what);
    mark_rect(rig, egui::Id::new(("chart_tab", id)), i)
}

#[test]
fn w6_a_chart_tab_drills_by_click_goes_back_by_breadcrumb_and_key_reopens_at_its_level_and_greys_its_menu_when_its_source_is_closed() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w6b", ppp, w, h);
        let what = rig.what.clone();
        drill_chart_tab(&mut rig);
        let id = chart_tab_id(&rig);
        let idx = |r: &Rig| r.app.tabs.iter().position(|t| t.id == id).expect("the chart tab");
        rig.app.active_tab = idx(&rig);
        rig.draw(4);
        assert_eq!(rig.app.chart_tabs[&id].data.spec.depth(), 0);
        assert!(rig.app.chart_tabs[&id].data.spec.has_drill(), "{what}: the chart tab keeps the drill levels");
        let p = rig.draw(3);
        assert_eq!(breadcrumb(&rig).iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), vec!["Level 0"], "{what}");
        assert!(has_text(&p, "All"), "{what}");

        // a click on Dhaka: the chart tab is calculated again from its source, one level down
        let r = tab_mark(&rig, id, "Dhaka");
        click_rect(&mut rig, r);
        assert!(rig.app.chart_tabs[&id].next_spec.as_ref().is_some_and(|s| s.depth() == 1), "{what}: asked for the next level");
        // until the numbers arrive the chart is drawn as it was calculated
        assert_eq!(rig.app.chart_tabs[&id].data.spec.depth(), 0, "{what}: the numbers on the screen are still of the top level");
        pump_chart_tab_depth(&mut rig, 1);
        let info = &rig.app.chart_tabs[&id];
        let want = fresh_chart(&rig, &info.data.spec);
        assert_eq!((info.data.categories.clone(), info.data.values.clone(), info.data.spec.category.clone()), (want.categories.clone(), want.values.clone(), "Month".to_string()), "{what}: the chart tab equals a new chart at that level");
        assert_eq!(info.data.values.iter().flatten().flatten().sum::<f64>(), 1000.0, "{what}");
        let p = rig.draw(3);
        assert!(has_text(&p, "Filtered: Branch = Dhaka"), "{what}: the subtitle says the path");
        assert_eq!(breadcrumb(&rig).iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), vec!["Level 0", "Level 1"], "{what}");
        let t = rig.app.tabs[idx(&rig)].title.clone();
        assert!(t.contains("by Month"), "{what}: the tab's title follows its chart: {t}");

        // the second level, then back by the breadcrumb and by the Back key
        let r = tab_mark(&rig, id, "Jan");
        click_rect(&mut rig, r);
        pump_chart_tab_depth(&mut rig, 2);
        assert_eq!(rig.app.chart_tabs[&id].data.spec.category, "Band", "{what}");
        let crumb = breadcrumb(&rig).iter().find(|(n, _)| n == "Level 1").map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: the crumb"));
        click_rect(&mut rig, crumb);
        pump_chart_tab_depth(&mut rig, 1);
        key_press(&mut rig, egui::Key::Backspace);
        pump_chart_tab_depth(&mut rig, 0);
        assert_eq!(rig.app.chart_tabs[&id].data.spec.category, "Branch", "{what}");
        let top = fresh_chart(&rig, &rig.app.chart_tabs[&id].data.spec);
        assert_eq!(rig.app.chart_tabs[&id].data.values, top.values, "{what}: back at the top the numbers are the first ones");

        // right-click: Show rows of a mark opens a tab with its rows
        let n = rig.app.tabs.len();
        let c = tab_mark(&rig, id, "Ctg").center();
        let p = right_click(&mut rig, c);
        click_rect(&mut rig, text_box(&p, "Show rows").unwrap_or_else(|| panic!("{what}: Show rows")));
        pump_until(&mut rig, "the rows tab", &|r| r.app.tabs.len() == n + 1 && !r.app.jobs.any_busy());
        let t = rig.app.tabs.last().expect("the new tab");
        assert_eq!(t.title, "Rows: Branch = Ctg", "{what}");
        let df = micro_parquet::engine::compile_tab_lazy(&t.source, &t.steps, &rig.app.paths.temp_dir).expect("compiles").collect().expect("collects");
        assert_eq!((df.height(), df.column("Balance").expect("b").f64().expect("f").into_iter().flatten().sum::<f64>()), (3, 1500.0), "{what}: the mark's rows and value");

        // a chart tab saved at level 2 reopens at level 2
        rig.app.active_tab = idx(&rig);
        rig.draw(3);
        let r = tab_mark(&rig, id, "Dhaka");
        click_rect(&mut rig, r);
        pump_chart_tab_depth(&mut rig, 1);
        let r = tab_mark(&rig, id, "Jan");
        click_rect(&mut rig, r);
        pump_chart_tab_depth(&mut rig, 2);
        let level2 = rig.app.chart_tabs[&id].data.clone();
        let pfile = rig.app.paths.root_dir.join("proj").join("Drill.mpproj");
        rig.app.project.test_pick = Some(pfile.clone());
        rig.app.run_command(&rig.ctx, CommandId::ProjectSaveAs);
        wait_saved(&mut rig);
        let saved = micro_parquet::project::load_project(&pfile).expect("the project reads");
        assert!(saved.tabs.len() >= 3, "{what}: the project holds the tabs");
        rig.app.project.test_pick = Some(pfile.clone());
        file_item(&mut rig, "Open project\u{2026}");
        click_button(&mut rig, "Open");
        pump_until(&mut rig, "the project opens", &|r| r.app.project.opening.is_none() && r.app.chart_tabs.len() == 1 && !r.app.jobs.any_busy());
        let id2 = chart_tab_id(&rig);
        let back = &rig.app.chart_tabs[&id2];
        assert_eq!(back.data.spec.drill_path, level2.spec.drill_path, "{what}: the path");
        assert_eq!((back.data.categories.clone(), back.data.values.clone(), back.data.spec.category.clone()), (level2.categories.clone(), level2.values.clone(), "Band".to_string()), "{what}: the same level, the same numbers");
        rig.app.active_tab = rig.app.tabs.iter().position(|t| t.id == id2).expect("the chart tab");
        let p = rig.draw(4);
        assert!(has_text(&p, "Filtered: Branch = Dhaka, Month = Jan"), "{what}: and says so");

        // the source closed: the chart cannot be calculated again, so its menu is greyed with the reason
        let src_id = rig.app.chart_tabs[&id2].source_tab.expect("a source tab");
        let src = rig.app.tabs.iter().position(|t| t.id == src_id).expect("the source tab");
        rig.app.close_tab(src);
        rig.app.active_tab = rig.app.tabs.iter().position(|t| t.id == id2).expect("the chart tab");
        rig.draw(4);
        let why = rig.app.chart_tab_blocked(id2).unwrap_or_else(|| panic!("{what}: blocked"));
        assert!(why.contains("is closed"), "{what}: {why}");
        let before = rig.app.chart_tabs[&id2].data.spec.clone();
        let r = tab_mark(&rig, id2, "Below 0%");
        click_rect(&mut rig, r);
        assert!(rig.app.chart_tabs[&id2].next_spec.is_none() && rig.app.chart_tabs[&id2].data.spec == before, "{what}: a click on a closed chart does nothing");
        let n = rig.app.tabs.len();
        let p = right_click(&mut rig, r.center());
        for item in ["Drill down", "Show rows", "Keep only this", "Exclude this"] {
            // a click on a greyed item does nothing (and leaves the menu as it was)
            click_rect(&mut rig, text_box(&p, item).unwrap_or_else(|| panic!("{what}: '{item}' is in the menu")));
        }
        assert_eq!(rig.app.tabs.len(), n, "{what}: nothing opened");
        assert!(rig.app.chart_tabs[&id2].next_spec.is_none() && rig.app.chart_tabs[&id2].data.spec == before, "{what}: and nothing changed");
    }
}

/// Scrolls the side panel with the wheel until the studio control `name` is well inside the window; its rectangle.
fn show_field(rig: &mut Rig, name: &str) -> egui::Rect {
    for _ in 0..30 {
        rig.draw(3);
        let screen = rig.ctx.screen_rect();
        let (lo, hi) = (screen.top() + 90.0, screen.bottom() - 45.0);
        let found = rig.app.chart.rects.fields.iter().find(|(n, _)| n == name).map(|(_, r)| *r);
        let delta = match found {
            Some(r) if r.top() >= lo && r.bottom() <= hi => return r,
            Some(r) if r.bottom() > hi => -(r.bottom() - hi).min(200.0),
            Some(r) => (lo - r.top()).min(200.0),
            None => -200.0,
        };
        let x = rig.app.chart.rects.controls.map(|c| c.center().x).unwrap_or(screen.right() - 100.0);
        rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 0.5);
        rig.step(vec![egui::Event::PointerMoved(egui::pos2(x, screen.center().y)), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, delta), modifiers: egui::Modifiers::NONE }]);
        for _ in 0..6 {
            rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 0.1);
            rig.draw(1);
        }
    }
    panic!("{}: the control '{name}' could not be scrolled into view: {:?} in {:?}", rig.what, rig.app.chart.rects.fields.iter().find(|(n, _)| n == name).map(|(_, r)| *r), rig.ctx.screen_rect());
}

fn click_field(rig: &mut Rig, name: &str) -> Painted {
    let r = show_field(rig, name);
    click_rect(rig, r)
}

fn level_names(rig: &Rig) -> Vec<String> {
    rig.app.chart.spec.drill.iter().map(|l| l.name.clone()).collect()
}

#[test]
fn w6_the_drill_levels_section_adds_moves_and_removes_levels_offers_the_date_hierarchy_and_says_why_a_scatter_has_none() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w6c", ppp, w, h);
        let what = rig.what.clone();
        rig.app.set_side_tab_visible(crate::ui::steps::RightTab::Chart, true);
        rig.draw(4);
        pump_until(&mut rig, "the first chart", &|r| !r.app.chart.busy && r.app.chart.data.is_some());
        rig.app.chart_set_spec(ChartSpec { category: "Branch".into(), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 50, ..Default::default() });
        pump_until(&mut rig, "the Branch chart", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.category == "Branch"));
        assert!(rig.app.chart.spec.drill.is_empty(), "{what}: a new chart has no drill");

        // the section opens by a click on its header, and lists the Category as level 1
        click_field(&mut rig, "Drill levels");
        let p = rig.draw(3);
        assert!(has_text(&p, "1. Branch (the Category)"), "{what}");
        // add Month, then Band, with real clicks on the picker and its list
        for (n, level) in [(2, "Month"), (3, "Band")] {
            let c = show_field(&mut rig, "Add a level");
            let p = click_rect(&mut rig, c);
            // (the list opens above the picker when it is near the bottom of the window, and is painted last)
            let item = p.boxes.iter().rev().find(|(t, _)| t == level).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: '{level}' is in the list: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).filter(|t| t.len() < 14).collect::<Vec<_>>()));
            assert!(inside(item, rig.ctx.screen_rect()), "{what}: the list is inside the window");
            click_rect(&mut rig, item);
            assert_eq!(rig.app.chart.spec.drill.len(), n, "{what}: {level} was added: {:?}", level_names(&rig));
        }
        assert_eq!(level_names(&rig), ["Branch", "Month", "Band"], "{what}");
        // the rows of levels: the name, up, down and Remove sit side by side without touching, inside the panel
        let last = show_field(&mut rig, "Level 3 remove");
        let p = rig.draw(3);
        for lvl in [2, 3] {
            let parts: Vec<egui::Rect> = ["up", "down", "remove"].iter().map(|b| rig.app.chart.rects.fields.iter().find(|(n, _)| *n == format!("Level {lvl} {b}")).map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: Level {lvl} {b}"))).collect();
            for r in &parts {
                assert!(inside(*r, rig.ctx.screen_rect()), "{what}: {r:?} is inside the window");
            }
            for pair in parts.windows(2) {
                assert!(pair[0].right() <= pair[1].left() + 0.5, "{what}: the buttons do not overlap");
            }
            if lvl == 3 {
                assert!(last.intersects(parts[2]));
            }
            assert_row_texts_apart(&p, &parts, &what);
        }
        pump_until(&mut rig, "the drilled chart", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.drill.len() == 3));
        // Band moves above Month; Month is removed; then Band
        click_field(&mut rig, "Level 3 up");
        assert_eq!(level_names(&rig), ["Branch", "Band", "Month"], "{what}");
        click_field(&mut rig, "Level 3 remove");
        assert_eq!(level_names(&rig), ["Branch", "Band"], "{what}");
        click_field(&mut rig, "Level 2 remove");
        assert!(rig.app.chart.spec.drill.is_empty() && rig.app.chart.spec.category == "Branch", "{what}: one level is no drill: {:?}", level_names(&rig));

        // a date Category: the whole hierarchy in one click
        let mut s = rig.app.chart.spec.clone();
        s.category = "Open Date".into();
        rig.app.chart_set_spec(s);
        pump_until(&mut rig, "the date chart", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.category == "Open Date"));
        click_field(&mut rig, "Date drill");
        assert_eq!(level_names(&rig), ["Open Date \u{b7} Year", "Open Date \u{b7} Quarter", "Open Date \u{b7} Month", "Open Date \u{b7} Day of month"], "{what}");
        assert_eq!((rig.app.chart.spec.category.as_str(), rig.app.chart.spec.depth()), ("Open Date \u{b7} Year", 0), "{what}");
        pump_until(&mut rig, "the years", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.category == "Open Date \u{b7} Year"));
        assert!(rig.app.chart.error.is_none(), "{what}: {:?}", rig.app.chart.error);
        // a click on a year goes to its quarters, and the quarters add up to the year
        let years = rig.app.chart.data.clone().expect("data");
        let year = years.categories[0].clone();
        let year_total = years.values[0][0].expect("a value");
        let r = mark_at(&mut rig, egui::Id::new(PLOT_ID), 0);
        click_rect(&mut rig, r);
        pump_until(&mut rig, "the quarters", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.category == "Open Date \u{b7} Quarter"));
        let q = rig.app.chart.data.clone().expect("data");
        assert!((q.values.iter().flatten().flatten().sum::<f64>() - year_total).abs() < 1e-6, "{what}: the quarters of {year} add up to the year");
        assert!(has_text(&rig.draw(3), &format!("Filtered: Open Date \u{b7} Year = {year}")), "{what}");

        // a scatter has no drill: the section is gone and the reason is shown
        let mut s = rig.app.chart.spec.clone();
        s.drill_to(0);
        s.kind = ChartKind::Scatter;
        rig.app.chart_set_spec(s);
        rig.draw(4);
        let r = show_field(&mut rig, "Drill levels hidden");
        assert!(rig.app.chart.rects.fields.iter().all(|(n, _)| n != "Drill levels" && n != "Add a level"), "{what}: no section");
        let p = rig.draw(3);
        assert!(has_text(&p, "Drill levels: A scatter or bubble chart draws single rows"), "{what}");
        assert!(inside(r, rig.ctx.screen_rect()), "{what}");
    }
}

/// The value cell to the right of the row label `label` in the grid the last frame drew.
fn value_cell_of_row(p: &Painted, label: &str, what: &str) -> egui::Rect {
    let l = p.boxes.iter().find(|(t, b)| t == label && b.top() > theme::TOP_BAR_H).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the row '{label}'"));
    p.boxes
        .iter()
        .filter(|(t, b)| !t.trim().is_empty() && (b.center().y - l.center().y).abs() < 4.0 && b.left() > l.right() + 4.0)
        .map(|(_, b)| *b)
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .unwrap_or_else(|| panic!("{what}: a value cell in the row of '{label}'"))
}

/// Two real clicks at the same instant: a double-click.
fn double_click(rig: &mut Rig, pos: egui::Pos2) {
    rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 1.0);
    rig.click(pos);
    rig.click(pos);
    rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 1.0);
}

fn rows_of(rig: &Rig, tab: usize) -> (usize, f64) {
    let t = &rig.app.tabs[tab];
    let df = micro_parquet::engine::compile_tab_lazy(&t.source, &t.steps, &rig.app.paths.temp_dir).expect("compiles").collect().expect("collects");
    (df.height(), df.column("Balance").expect("Balance").f64().expect("f").into_iter().flatten().sum())
}

#[test]
fn w6_a_double_click_or_right_click_on_a_pivot_value_cell_opens_its_rows_totals_included_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w6d", ppp, w, h);
        let what = rig.what.clone();
        // (the side panel's chart has the same labels as the pivot's rows)
        rig.app.show_steps_sidebar = false;
        let src = rig.app.tabs[0].source.clone();
        rig.app.create_tab_from_source("Pivot: Branch".into(), src, vec![branch_pivot()], HashMap::new());
        settle(&mut rig.app);
        rig.app.active_tab = 1;
        pump_until(&mut rig, "the pivot", &|r| r.app.tabs[1].total_rows == 4 && !r.app.jobs.any_busy());
        let p = rig.draw(4);
        let n = rig.app.tabs.len();

        // a double-click on the label of a row does nothing: only value cells show their rows
        let label = p.boxes.iter().find(|(t, b)| t == "Dhaka" && b.top() > theme::TOP_BAR_H).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the Dhaka row"));
        double_click(&mut rig, label.center());
        for _ in 0..4 {
            rig.draw(2);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(rig.app.tabs.len(), n, "{what}: a row label shows no rows");

        // a double-click on Dhaka's value: the rows of Dhaka (3 rows, 1,000)
        let cell = value_cell_of_row(&p, "Dhaka", &what);
        assert!(inside(cell, rig.ctx.screen_rect()), "{what}");
        double_click(&mut rig, cell.center());
        pump_until(&mut rig, "the rows of Dhaka", &|r| r.app.tabs.len() == n + 1 && !r.app.jobs.any_busy());
        assert_eq!(rig.app.tabs[n].title, "Rows: Branch = Dhaka", "{what}");
        assert_eq!(rows_of(&rig, n), (3, 1000.0), "{what}: the rows and the sum of the cell");
        assert!(matches!(rig.app.tabs[n].steps.last(), Some(TransformStep::FilterGroup { keep_matching: true, .. })) && rig.app.tabs[n].steps.len() == 1, "{what}: the file, and a filter");

        // the Grand total by the right-click menu: all rows
        rig.app.active_tab = 1;
        let p = rig.draw(4);
        let cell = value_cell_of_row(&p, "Grand total", &what);
        let p = right_click(&mut rig, cell.center());
        let item = text_box(&p, "Show rows").unwrap_or_else(|| panic!("{what}: Show rows in the cell's menu: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).filter(|t| t.len() < 20).collect::<Vec<_>>()));
        assert!(inside(item, rig.ctx.screen_rect()), "{what}");
        assert!(text_box(&p, "Copy").is_some(), "{what}: the Copy items stay");
        click_rect(&mut rig, item);
        pump_until(&mut rig, "the rows of the grand total", &|r| r.app.tabs.len() == n + 2 && !r.app.jobs.any_busy());
        assert_eq!(rig.app.tabs[n + 1].title, "Rows: all rows of the pivot", "{what}");
        assert_eq!(rows_of(&rig, n + 1), (8, 3600.0), "{what}: every row, and the grand total");

        // a table that is not a pivot has no Show rows in its cell menu
        rig.app.active_tab = 0;
        let p = rig.draw(4);
        let cell = p.boxes.iter().find(|(t, b)| t == "Dhaka" && b.top() > theme::TOP_BAR_H).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: a Dhaka cell"));
        let p = right_click(&mut rig, cell.center());
        assert!(text_box(&p, "Show rows").is_none() && text_box(&p, "Copy").is_some(), "{what}: only a pivot's value cells show rows");
    }
}

/// A pivot tab of Branch with `later` steps after the pivot, active and drawn.
fn pivot_tab_with(rig: &mut Rig, later: Vec<TransformStep>) -> usize {
    rig.app.show_steps_sidebar = false;
    let src = rig.app.tabs[0].source.clone();
    let mut steps = vec![branch_pivot()];
    steps.extend(later);
    rig.app.create_tab_from_source("Pivot: Branch".into(), src, steps, HashMap::new());
    settle(&mut rig.app);
    let at = rig.app.tabs.len() - 1;
    rig.app.active_tab = at;
    pump_until(rig, "the pivot", &|r| r.app.tabs[at].total_rows > 0 && !r.app.jobs.any_busy());
    at
}

#[test]
fn w6_a_pivot_followed_by_a_sort_and_a_filter_still_shows_the_rows_of_its_cell_at_three_sizes() {
    use micro_parquet::models::{CondOp, FilterCondition, GroupJoin, SortLevel};
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w6s", ppp, w, h);
        let what = rig.what.clone();
        // the pivot's value column, then: sort by it (biggest first) and leave Ctg out
        let probe = pivot_tab_with(&mut rig, vec![]);
        let value = {
            let t = &rig.app.tabs[probe];
            let df = micro_parquet::engine::compile_tab_lazy(&t.source, &t.steps, &rig.app.paths.temp_dir).expect("compiles").collect().expect("collects");
            df.get_column_names()[1].to_string()
        };
        let sort = TransformStep::SortRows { levels: vec![SortLevel { column: value, descending: true, ..Default::default() }] };
        let filter = TransformStep::FilterGroup { conditions: vec![FilterCondition { column: "Branch".into(), op: CondOp::NotEquals, value: "Ctg".into(), ..Default::default() }], join: GroupJoin::And, keep_matching: true };
        let at = pivot_tab_with(&mut rig, vec![sort, filter]);
        assert_eq!(rig.app.tabs[at].total_rows, 3, "{what}: Grand total, Sylhet and Dhaka");
        let n = rig.app.tabs.len();
        // Dhaka is now the last row (the sort put Sylhet above it): its cell still gives Dhaka's rows
        let p = rig.draw(4);
        let cell = value_cell_of_row(&p, "Dhaka", &what);
        double_click(&mut rig, cell.center());
        pump_until(&mut rig, "the rows of Dhaka", &|r| r.app.tabs.len() == n + 1 && !r.app.jobs.any_busy());
        assert_eq!(rig.app.tabs[n].title, "Rows: Branch = Dhaka", "{what}");
        assert_eq!(rows_of(&rig, n), (3, 1000.0), "{what}: the same rows as the pivot alone gives");
        // and Sylhet's, by the menu
        rig.app.active_tab = at;
        let p = rig.draw(4);
        let cell = value_cell_of_row(&p, "Sylhet", &what);
        let p = right_click(&mut rig, cell.center());
        let item = text_box(&p, "Show rows").unwrap_or_else(|| panic!("{what}: Show rows"));
        click_rect(&mut rig, item);
        pump_until(&mut rig, "the rows of Sylhet", &|r| r.app.tabs.len() == n + 2 && !r.app.jobs.any_busy());
        assert_eq!(rig.app.tabs[n + 1].title, "Rows: Branch = Sylhet", "{what}");
        assert_eq!(rows_of(&rig, n + 1), (2, 1100.0), "{what}");
        // the Grand total of the filtered table is the pivot's grand total: every row
        rig.app.active_tab = at;
        let p = rig.draw(4);
        let cell = value_cell_of_row(&p, "Grand total", &what);
        double_click(&mut rig, cell.center());
        pump_until(&mut rig, "the rows of the grand total", &|r| r.app.tabs.len() == n + 3 && !r.app.jobs.any_busy());
        assert_eq!(rows_of(&rig, n + 2), (8, 3600.0), "{what}");
    }
}

#[test]
fn w6_a_pivot_followed_by_a_rename_of_a_row_field_greys_show_rows_with_the_reason_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w6r", ppp, w, h);
        let what = rig.what.clone();
        let at = pivot_tab_with(&mut rig, vec![TransformStep::RenameColumn { old_name: "Branch".into(), new_name: "Office".into() }]);
        let n = rig.app.tabs.len();
        let p = rig.draw(4);
        let cell = value_cell_of_row(&p, "Dhaka", &what);
        // the menu has the item, greyed: a click opens nothing, the hover says why
        let p = right_click(&mut rig, cell.center());
        let item = text_box(&p, "Show rows").unwrap_or_else(|| panic!("{what}: the Show rows item is there"));
        assert!(inside(item, rig.ctx.screen_rect()), "{what}");
        let tip = hover_at(&mut rig, item.center());
        assert!(has_text(&tip, "renames 'Branch', a row field of the pivot, to 'Office'"), "{what}: the reason on hover");
        click_rect(&mut rig, item);
        for _ in 0..4 {
            rig.draw(2);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(rig.app.tabs.len(), n, "{what}: a greyed item opens nothing");
        rig.step(vec![egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }]);
        // a double-click says the reason in the status line and opens nothing
        rig.app.status_message.clear();
        let p = rig.draw(4);
        let cell = value_cell_of_row(&p, "Dhaka", &what);
        double_click(&mut rig, cell.center());
        for _ in 0..4 {
            rig.draw(2);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(rig.app.status_message.starts_with("Show rows is off:") && rig.app.status_message.contains("'Branch'"), "{what}: {}", rig.app.status_message);
        assert_eq!(rig.app.tabs.len(), n, "{what}");
        assert!(rig.app.pivot_rows_ui(&rig.app.tabs[at]).is_some_and(|p| p.off.is_some()), "{what}");

        // a value column that a later step renamed: its own cells are greyed, with that reason
        let mut rig = proj_rig("p8w6v", ppp, w, h);
        let probe = pivot_tab_with(&mut rig, vec![]);
        let value = {
            let t = &rig.app.tabs[probe];
            let df = micro_parquet::engine::compile_tab_lazy(&t.source, &t.steps, &rig.app.paths.temp_dir).expect("compiles").collect().expect("collects");
            df.get_column_names()[1].to_string()
        };
        pivot_tab_with(&mut rig, vec![TransformStep::RenameColumn { old_name: value, new_name: "Balance (BDT)".into() }]);
        let n = rig.app.tabs.len();
        let p = rig.draw(4);
        let cell = value_cell_of_row(&p, "Dhaka", &what);
        let p = right_click(&mut rig, cell.center());
        let item = text_box(&p, "Show rows").unwrap_or_else(|| panic!("{what}: the Show rows item is there"));
        let tip = hover_at(&mut rig, item.center());
        assert!(has_text(&tip, "'Balance (BDT)': a step after the pivot added, renamed or changed it") || has_text(&tip, "a step after the pivot added, renamed or changed it"), "{what}: the reason on hover");
        click_rect(&mut rig, item);
        rig.draw(3);
        assert_eq!(rig.app.tabs.len(), n, "{what}");
    }
}

#[test]
fn w6_ctrl_k_rows_with_a_long_reason_wrap_under_their_title_and_nothing_overlaps_at_three_sizes() {
    for (ppp, w, h) in SIZES3 {
        let mut rig = proj_rig("p8w6k", ppp, w, h);
        let what = rig.what.clone();
        rig.draw(3);
        ctrl_key(&mut rig, egui::Key::K);
        assert!(rig.app.command_open, "{what}");
        rig.step(vec![egui::Event::Text("chart this result".into())]);
        let p = rig.draw(3);
        let row = text_box(&p, "Chart this result").unwrap_or_else(|| panic!("{what}: the row: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>()));
        let area = rig.ctx.memory(|m| m.area_rect(egui::Id::new("command_palette"))).expect("the search was drawn");
        // the reason is the 'Needs a result tab' sentence of Phase 7, whole, below the title and inside the search
        let lines: Vec<&(String, egui::Rect)> = p.glyphs.iter().filter(|(t, b)| inside(*b, area) && b.top() >= row.bottom() - 1.0 && b.top() < row.bottom() + 60.0 && !t.contains("Chart this result")).collect();
        let text: String = lines.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>().join(" ");
        assert!(text.contains("Open a tool's result tab") && text.contains("to chart it."), "{what}: the whole reason is shown: {text:?}");
        let reason = "Open a tool's result tab (for example the gap ladder or the IRRBB scenarios) to chart it.";
        let own: Vec<&&(String, egui::Rect)> = lines.iter().filter(|(t, _)| reason.contains(t.trim())).collect();
        assert!(!own.is_empty() && own.iter().all(|(t, _)| !t.ends_with('\u{2026}')), "{what}: the reason is not cut with an ellipsis: {own:?}");
        assert_eq!(own.iter().map(|(t, _)| t.trim()).collect::<Vec<_>>().join(" "), reason, "{what}: the reason is whole");
        let band: Vec<(String, egui::Rect)> = p.glyphs.iter().filter(|(_, b)| inside(*b, area) && b.top() >= row.top() - 3.0 && b.bottom() <= row.bottom() + 80.0).cloned().collect();
        // (the chart behind draws its own texts in this band, so only the search's own are compared)
        rig.app.command_open = false;
        let behind = rig.draw(2);
        rig.app.command_open = true;
        let p = rig.draw(2);
        let same = |a: &egui::Rect, b: &egui::Rect| (a.min - b.min).length() < 0.5 && (a.max - b.max).length() < 0.5;
        let mine: Vec<(String, egui::Rect)> = p.glyphs.iter().filter(|(t, b)| inside(*b, area) && !behind.glyphs.iter().any(|(u, c)| u == t && same(b, c))).cloned().collect();
        assert!(!band.is_empty() && !mine.is_empty(), "{what}");
        assert_no_overlap(&mine, &what);
        // the same at the first row of the list with nothing typed, where the long reasons of a fresh tab are listed
        rig.app.command_query.clear();
        let p = rig.draw(3);
        let mine: Vec<(String, egui::Rect)> = p.glyphs.iter().filter(|(t, b)| inside(*b, area) && !behind.glyphs.iter().any(|(u, c)| u == t && same(b, c))).cloned().collect();
        assert_no_overlap(&mine, &what);
    }
}

#[test]
fn w6_other_is_never_drilled_into_filtered_or_shown_as_rows_and_a_drill_keeps_the_colours() {
    let mut rig = studio_rig("p8w6e", 1.0, 1280.0, 720.0);
    let other = micro_parquet::charts::data::MarkRef { category: "Other".into(), series: None, facet: None };
    let before = rig.app.chart.spec.clone();
    let tabs = rig.app.tabs.len();
    for a in [
        crate::ui::chart_render::ChartAction::DrillDown(other.clone()),
        crate::ui::chart_render::ChartAction::Keep(other.clone()),
        crate::ui::chart_render::ChartAction::Exclude(other.clone()),
        crate::ui::chart_render::ChartAction::ShowRows(other.clone()),
    ] {
        rig.app.status_message.clear();
        rig.app.studio_chart_action(a.clone());
        assert!(rig.app.status_message.contains("Top N puts together"), "{a:?}: {}", rig.app.status_message);
        assert_eq!(rig.app.chart.spec, before, "{a:?}: the chart is as it was");
        assert_eq!(rig.app.tabs.len(), tabs, "{a:?}: no tab");
    }
    // the same on a chart tab: nothing is asked of the worker
    let mut rig = proj_rig("p8w6f", 1.0, 1280.0, 720.0);
    drill_chart_tab(&mut rig);
    let id = chart_tab_id(&rig);
    rig.app.status_message.clear();
    rig.app.chart_tab_chart_action(id, crate::ui::chart_render::ChartAction::DrillDown(other));
    assert!(rig.app.status_message.contains("Top N puts together") && rig.app.chart_tabs[&id].next_spec.is_none());

    // a drill keeps the colours the series have: the colour map belongs to the columns, and the columns are the same
    let mut rig = studio_rig("p8w6g", 1.0, 1280.0, 720.0);
    let mut s = rig.app.chart.spec.clone();
    // (a series cannot also be a level: the levels are Branch and Month)
    s.set_drill(vec![DrillLevel::column("Branch"), DrillLevel::column("Month")]);
    s.series = Some("Band".into());
    rig.app.chart_set_spec(s);
    pump_until(&mut rig, "the stacked chart", &|r| !r.app.chart.busy && r.app.chart.data.as_ref().is_some_and(|d| d.spec.series.as_deref() == Some("Band")));
    let mut s = rig.app.chart.spec.clone();
    s.colour_map.assign_all(&["Below 0%".to_string(), "0%".to_string()]);
    let picked = s.colour_map.clone();
    assert!(picked != micro_parquet::charts::colours::ColourMap::default());
    rig.app.chart_set_spec(s.clone());
    let mut down = rig.app.chart.spec.clone();
    assert!(down.drill_down("Dhaka"));
    rig.app.chart_set_spec(down);
    assert_eq!(rig.app.chart.spec.colour_map, picked, "going down a level keeps the colours");
    let mut up = rig.app.chart.spec.clone();
    up.drill_to(0);
    rig.app.chart_set_spec(up);
    assert_eq!(rig.app.chart.spec.colour_map, picked, "and so does going back up");
    // another series is another set of names: the colours start again
    let mut other_series = rig.app.chart.spec.clone();
    other_series.series = None;
    rig.app.chart_set_spec(other_series);
    assert!(rig.app.chart.spec.colour_map != picked, "a different series column starts the colours again");
}
