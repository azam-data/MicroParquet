//! Phase 6 render tests: the dialogs and controls of Phase 6 are drawn by the real UI code in a
//! headless egui context at 1219×686 pt (a 1280×720 px window at 105%) and 1600×900 pt (at 80%),
//! and what they drew is checked by the rectangles of the widgets themselves. Every control is
//! clicked at its real rectangle.

use crate::ui::layout_tests::Painted;
use crate::ui::prelude::*;
use crate::ui::render_tests::{assert_dialog_fits, inside, screen};
use crate::ui::render_tests_p4::{click_text, Rig, SCREENS};
use crate::ui::tests::{parquet_source, settle, test_app};
use micro_parquet::commands::CommandId;
use micro_parquet::sort_lists::{load_lists, save_list, sort_lists_dir, SortList, SortOrder};
use polars::prelude::{NamedFrom, Series};

fn overlaps(a: egui::Rect, b: egui::Rect) -> bool {
    a.intersects(b) && a.intersect(b).width() > 1.0 && a.intersect(b).height() > 1.0
}

/// A tab "Loans" with a text column "Term Period" and a number, the data folder of the test as
/// `portable_data`, and a list "Tenor labels" suggested for "Term Period".
fn term_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    micro_parquet::rules::set_data_dir_override(Some(app.paths.root_dir.clone()));
    let root = app.paths.root_dir.clone();
    let df = DataFrame::new(vec![
        Series::new("Term Period".into(), &["6m", "1y", "1m", "3m", "abc", "100D"]),
        Series::new("Balance".into(), &[10i64, 20, 30, 40, 50, 60]),
    ])
    .expect("frame");
    let src = parquet_source(&root, "book", df);
    app.create_tab_from_source("Loans".into(), src, vec![], HashMap::new());
    settle(&mut app);
    let list = SortList { name: "Risk".into(), values: vec!["High".into(), "Medium".into(), "Low".into()], case_insensitive: true, apply_to_columns: vec!["Term Period".into()] };
    save_list(&sort_lists_dir(), &list).expect("saved a list");
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

fn lists_title() -> String {
    format!("{}  Custom sort lists", theme::ph::SORT_ASCENDING)
}

fn button(rig: &Rig, name: &str) -> egui::Rect {
    rig.app.sort_lists.rects.buttons.iter().find(|(n, _)| n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: the button '{name}' was drawn", rig.what))
}

fn field(rig: &Rig, name: &str) -> egui::Rect {
    rig.app.sort_lists.rects.fields.iter().find(|(n, _)| n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: the field '{name}' was drawn", rig.what))
}

#[test]
fn u3_the_sort_list_manager_fits_and_every_button_works_by_a_real_click() {
    for (ppp, w, h) in SCREENS {
        let mut rig = term_rig("p6u3a", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::SortLists);
        assert_eq!(rig.app.active_tool, ActiveToolModal::SortLists, "{what}: View > Custom sort lists / Ctrl+K opens it");
        let p: Painted = rig.draw(6);
        let r = assert_dialog_fits(&rig.ctx, &lists_title(), &what);
        // the saved list is opened: its name and values are in the fields
        assert_eq!(rig.app.sort_lists.editor.name, "Risk", "{what}");
        assert_eq!(rig.app.sort_lists.editor.values, vec!["High", "Medium", "Low"], "{what}");
        for t in ["LISTS", "Name:", "Suggested for columns:", "Values, first to last", "Fill from column:", "Save list", "New list", "Rename", "Delete", "Close"] {
            assert!(p.boxes.iter().any(|(x, bx)| x.contains(t) && inside(*bx, r.window)), "{what}: '{t}' is in the dialog {:?}", r.window);
        }
        let g = rig.app.sort_lists.rects.clone();
        assert!(inside(g.list, r.window) && inside(g.editor, r.window), "{what}: the list {:?} and the editor {:?} are inside {:?}", g.list, g.editor, r.window);
        assert!(!overlaps(g.list, g.editor), "{what}: list and editor do not overlap");
        for (n, rc) in &g.fields {
            assert!(inside(*rc, g.editor), "{what}: the field '{n}' {rc:?} is inside the editor {:?}", g.editor);
        }
        for (n, b) in &g.buttons {
            assert!(inside(*b, r.window), "{what}: '{n}' {b:?} is inside the dialog");
        }
        assert_eq!(g.value_rows.len(), 3, "{what}");
        for row in &g.value_rows {
            assert!(inside(*row, g.editor), "{what}: the value row {row:?} is inside the editor");
        }
        assert!(g.message.width() <= g.editor.width() + 0.5, "{what}: the message {:?} is not wider than the editor", g.message);

        // "Move down" on the first value (real click): High, Medium, Low -> Medium, High, Low
        rig.click(field(&rig, "Move down").center());
        assert_eq!(rig.app.sort_lists.editor.values, vec!["Medium", "High", "Low"], "{what}: Move down");
        // "Remove value" on the first value
        rig.click(field(&rig, "Remove value").center());
        assert_eq!(rig.app.sort_lists.editor.values, vec!["High", "Low"], "{what}: Remove");
        // "Add value" adds an empty row; a blank value is dropped when saving, so the list is still fine
        let p = rig.draw(2);
        let add = p.boxes.iter().find(|(t, _)| t.contains("Add value")).map(|(_, b)| *b).expect("the Add value button");
        rig.click(add.center());
        assert_eq!(rig.app.sort_lists.editor.values.len(), 3, "{what}: Add value");
        rig.app.sort_lists.editor.values.pop();
        rig.draw(2);

        // Save list asks first: it changes a stored list
        rig.click(button(&rig, "Save list").center());
        assert!(rig.app.sort_lists.confirm_overwrite, "{what}: saving over a stored list asks");
        rig.draw(2);
        rig.click(button(&rig, "Yes, replace it").center());
        assert_eq!(load_lists(&sort_lists_dir()).0[0].values, vec!["High", "Low"], "{what}: saved");

        // Fill from column: the worker lists the values of "Term Period" in natural order
        rig.click(button(&rig, "New list").center());
        assert!(rig.app.sort_lists.editor.values.is_empty() && rig.app.sort_lists.editor.opened_from.is_none(), "{what}: New list");
        rig.draw(2);
        rig.app.sort_lists.fill_column = "Term Period".into();
        rig.click(button(&rig, "Fill from column").center());
        settle(&mut rig.app);
        rig.draw(2);
        assert_eq!(rig.app.sort_lists.editor.values, vec!["1m", "1y", "3m", "6m", "100D", "abc"], "{what}: natural order: {:?} / {:?}", rig.app.sort_lists.editor.values, rig.app.sort_lists.message);
        // a name is needed; with one, Save list creates the file
        assert!(rig.app.sort_lists.problem().is_some(), "{what}: no name yet");
        rig.app.sort_lists.editor.name = "Terms".into();
        rig.draw(2);
        rig.click(button(&rig, "Save list").center());
        let names: Vec<String> = load_lists(&sort_lists_dir()).0.into_iter().map(|l| l.name).collect();
        assert_eq!(names, vec!["Risk", "Terms"], "{what}");
        // rename "Terms" (typed name differs)
        rig.app.sort_lists.editor.name = "Term labels".into();
        rig.draw(2);
        rig.click(button(&rig, "Rename").center());
        let names: Vec<String> = load_lists(&sort_lists_dir()).0.into_iter().map(|l| l.name).collect();
        assert_eq!(names, vec!["Risk", "Term labels"], "{what}: Rename");
        // delete asks, then deletes
        rig.click(button(&rig, "Delete").center());
        assert!(rig.app.sort_lists.confirm_delete, "{what}");
        rig.draw(2);
        rig.click(button(&rig, "Yes, delete it").center());
        let names: Vec<String> = load_lists(&sort_lists_dir()).0.into_iter().map(|l| l.name).collect();
        assert_eq!(names, vec!["Risk"], "{what}: Delete");
        // a click on a list row opens that list; Close closes the dialog
        let row = rig.app.sort_lists.rects.list_rows.iter().find(|(n, _)| n == "Risk").map(|(_, r)| *r).expect("row");
        rig.click(row.center());
        assert_eq!(rig.app.sort_lists.editor.opened_from.as_deref(), Some("Risk"), "{what}");
        rig.click(button(&rig, "Close").center());
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: Close");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn u3_the_sort_dialog_offers_the_order_choice_and_a_list_or_tenor_sorts_the_tab() {
    for (ppp, w, h) in SCREENS {
        let mut rig = term_rig("p6u3b", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::Sort);
        assert_eq!(rig.app.active_tool, ActiveToolModal::SortRows, "{what}");
        rig.app.sort.levels = vec![SortLevel { column: "Term Period".into(), ..Default::default() }];
        let p = rig.draw(5);
        let r = assert_dialog_fits(&rig.ctx, "\u{E444}  Sort rows", &what);
        // the row: level number, column, order, the two directions and the three buttons, in the dialog
        let combo = p.boxes.iter().filter(|(t, bx)| t == "Natural" && inside(*bx, r.window)).map(|(_, b)| *b).next().unwrap_or_else(|| panic!("{what}: the Order choice shows Natural"));
        for t in ["Lowest first", "Highest first", "Add level", "Apply sort", "Cancel"] {
            assert!(p.boxes.iter().any(|(x, bx)| x.contains(t) && inside(*bx, r.window)), "{what}: '{t}' is in the dialog {:?}", r.window);
        }
        let col = p.boxes.iter().find(|(t, _)| t == "Term Period").map(|(_, b)| *b).expect("the column choice");
        assert!(col.right() <= combo.left() + 0.5, "{what}: the column {col:?} is left of the order {combo:?}");
        // choose Tenor by a real click on the combo, then on its item
        rig.click(combo.center());
        let p = rig.draw(3);
        for t in ["Natural", "Risk", "Tenor"] {
            assert!(p.boxes.iter().any(|(x, _)| x == t), "{what}: the menu lists '{t}'");
        }
        click_text(&mut rig, &p, "Tenor", combo);
        assert_eq!(rig.app.sort.levels[0].order, SortOrder::Tenor, "{what}");
        // Apply sort by a real click
        let p = rig.draw(3);
        let apply = p.boxes.iter().find(|(t, _)| t.contains("Apply sort")).map(|(_, b)| *b).expect("Apply sort");
        rig.click(apply.center());
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}");
        settle(&mut rig.app);
        let col: Vec<String> = rig.app.tabs[0].cached_slice.column("Term Period").expect("col").str().expect("text").into_iter().map(|v| v.unwrap_or("").to_string()).collect();
        assert_eq!(col, vec!["1m", "3m", "100D", "6m", "1y", "abc"], "{what}: Tenor order, the value that is not a tenor last");
        // the worker's count of what is not a tenor reaches the status line
        settle(&mut rig.app);
        assert!(rig.app.status_message.contains("1 value(s) are not tenors"), "{what}: {}", rig.app.status_message);
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn u3_the_pivot_field_menu_has_the_order_choice_and_a_suggested_list_is_preselected() {
    for (ppp, w, h) in SCREENS {
        let mut rig = term_rig("p6u3c", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::Pivot);
        rig.draw(8);
        // adding "Term Period" to Rows pre-selects the list suggested for it
        rig.app.add_pivot_field("Term Period", crate::ui::pivot_builder::Area::Rows);
        rig.app.add_pivot_field("Balance", crate::ui::pivot_builder::Area::Values);
        let orders = rig.app.pivot_builder.spec.field_orders.clone();
        assert_eq!(orders.len(), 1, "{what}: {orders:?}");
        assert_eq!(orders[0].order.label(), "Risk", "{what}");
        let p = rig.draw(8);
        assert!(p.boxes.iter().any(|(t, _)| t.contains("order: Risk")), "{what}: the chip shows the order");
        // the field's menu: its button is at its real rect; Order is a submenu
        let menu = rig.app.pivot_builder.rects.field_menus.iter().find(|(n, _)| n == "Term Period").map(|(_, r)| *r).expect("the field's menu button");
        rig.click(menu.center());
        let p = rig.draw(3);
        let order_item = p.boxes.iter().find(|(t, _)| t.starts_with("Order: Risk")).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the menu has 'Order: Risk'"));
        rig.click(order_item.center());
        let p = rig.draw(3);
        for t in ["Natural", "Risk", "Tenor", "By value"] {
            assert!(p.boxes.iter().any(|(x, bx)| x == t && bx.left() >= order_item.left() - 1.0), "{what}: the Order menu lists '{t}'");
        }
        click_text(&mut rig, &p, "Tenor", order_item);
        assert_eq!(rig.app.pivot_builder.spec.field_orders[0].order, SortOrder::Tenor, "{what}");
        rig.draw(6);
        // the preview lists the rows in tenor order
        let Some(Ok(df)) = rig.app.pivot_builder.preview.clone() else { panic!("{what}: a preview") };
        let rows: Vec<String> = df.column("Term Period").expect("col").str().expect("text").into_iter().map(|v| v.unwrap_or("").to_string()).collect();
        assert_eq!(rows[..5].to_vec(), vec!["1m", "3m", "100D", "6m", "1y"], "{what}: {rows:?}");
        assert_eq!(rows[5], "abc", "{what}");
    }
    micro_parquet::rules::set_data_dir_override(None);
}
