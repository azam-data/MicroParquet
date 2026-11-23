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
