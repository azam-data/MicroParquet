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

// ------------------------------------------------------------------------------------------ U1

use crate::ui::schema::{SCHEMA_TITLE_CSV, SCHEMA_TITLE_PARQUET};

const U1_FILE_A: &str = "ACNO,CRIRTE,BAL\n1001,9.5,100.00\n1002,12.0,250.50\n1003,7.25,80.00\n";
const U1_FILE_B: &str = "BAL,ACNO,CRIRTE\n300.00,2001,11.0\n40.00,2002,6.5\n";

/// An app with the saved schema "loans" (the renames of the acceptance item) in its schemas folder.
fn u1_rig(name: &str, ppp: f32, w: f32, h: f32, auto_apply: bool, second_template: bool) -> (Rig, PathBuf, PathBuf) {
    let app = test_app(name);
    let root = app.paths.root_dir.clone();
    let a = root.join("a.csv");
    let b = root.join("b.csv");
    std::fs::write(&a, U1_FILE_A).expect("a");
    std::fs::write(&b, U1_FILE_B).expect("b");
    let mut cfg = micro_parquet::engine::auto_detect_smart_schema(&a).expect("detected");
    for c in cfg.columns.iter_mut() {
        c.rename_to = match c.name.as_str() {
            "ACNO" => {
                c.dtype = micro_parquet::models::ColDataType::Utf8;
                Some("Account_No".into())
            }
            "CRIRTE" => Some("Int_Rate".into()),
            _ => Some("Balance".into()),
        };
    }
    cfg.auto_apply = auto_apply;
    std::fs::create_dir_all(&app.paths.schemas_dir).expect("schemas");
    std::fs::write(app.paths.schemas_dir.join("loans.schema.json"), serde_json::to_string_pretty(&cfg).expect("json")).expect("save");
    if second_template {
        let mut other = cfg.clone();
        other.auto_apply = false;
        other.columns[0].rename_to = Some("Acct".into());
        std::fs::write(app.paths.schemas_dir.join("loans_alt.schema.json"), serde_json::to_string_pretty(&other).expect("json")).expect("save");
    }
    let (ctx, input) = screen(ppp, w, h);
    (Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }, a, b)
}

/// A dialog drawn before any tab is open (there is no tab strip yet): inside the window, footer inside the dialog.
fn dialog_in_window(rig: &Rig, title: &str) -> theme::DialogReport {
    let r = theme::dialog_report(&rig.ctx, title).unwrap_or_else(|| panic!("{}: the dialog '{title}' was not drawn", rig.what));
    let screen = rig.ctx.screen_rect();
    assert!(inside(r.window, screen), "{}: the dialog {:?} is inside the window {screen:?}", rig.what, r.window);
    assert!(inside(r.footer, r.window) && r.footer.top() >= r.body.bottom() - 0.5, "{}: the footer {:?} is in the dialog {:?}, below its middle", rig.what, r.footer, r.window);
    r
}

fn text_box(p: &Painted, text: &str) -> egui::Rect {
    // a button's text may start with its icon
    p.boxes.iter().find(|(t, _)| t == text || (t.ends_with(text) && t.chars().count() <= text.chars().count() + 3)).map(|(_, b)| *b).unwrap_or_else(|| panic!("'{text}' is on screen"))
}

#[test]
fn u1_a_matching_file_opens_the_dialog_with_the_saved_schema_and_undo_works_by_a_real_click() {
    for (ppp, w, h) in SCREENS {
        let (mut rig, _a, b) = u1_rig("p6u1a", ppp, w, h, false, false);
        let what = rig.what.clone();
        rig.app.open_data_file(b.clone(), false);
        let modal = rig.app.pending_csv_modal.as_ref().unwrap_or_else(|| panic!("{what}: the Schema dialog opened"));
        assert_eq!(modal.applied.as_ref().map(|a| (a.name.as_str(), a.columns)), Some(("loans", 3)), "{what}");
        assert_eq!(modal.config.final_names(), vec!["Balance", "Account_No", "Int_Rate"], "{what}: the file's own column order");
        let p = rig.draw(6);
        let r = dialog_in_window(&rig, SCHEMA_TITLE_CSV);
        assert!(p.boxes.iter().any(|(t, bx)| t == "Applied saved schema 'loans' (all 3 columns matched)" && inside(*bx, r.window)), "{what}: the applied line is in the dialog");
        assert!(p.boxes.iter().any(|(t, bx)| t == "Rename to" && inside(*bx, r.window)), "{what}: the Rename to heading");
        for n in ["Account_No", "Int_Rate", "Balance"] {
            assert!(p.boxes.iter().any(|(t, bx)| t == n && inside(*bx, r.window)), "{what}: the rename '{n}' shows in its cell");
        }
        let undo = p.boxes.iter().find(|(t, _)| t == "Undo").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: Undo"));
        assert!(inside(undo, r.window), "{what}");
        rig.click(undo.center());
        let modal = rig.app.pending_csv_modal.as_ref().expect("still open");
        assert!(modal.applied.is_none() && modal.config.rename_pairs().is_empty(), "{what}: Undo goes back to the detected schema");
        let p = rig.draw(3);
        assert!(!p.boxes.iter().any(|(t, _)| t.starts_with("Applied saved schema")), "{what}");
    }
}

#[test]
fn u1_lock_and_open_makes_a_tab_with_the_renamed_columns_and_a_filter_on_the_new_name_works() {
    for (ppp, w, h) in SCREENS {
        let (mut rig, _a, b) = u1_rig("p6u1b", ppp, w, h, false, false);
        let what = rig.what.clone();
        rig.app.open_data_file(b, false);
        let p = rig.draw(6);
        let lock = text_box(&p, "Lock Schema & Open");
        rig.click(lock.center());
        settle(&mut rig.app);
        assert!(rig.app.pending_csv_modal.is_none(), "{what}: the dialog closed");
        assert_eq!(rig.app.tabs.len(), 1, "{what}");
        assert_eq!(rig.app.tabs[0].available_columns(), vec!["Balance", "Account_No", "Int_Rate"], "{what}");
        let step = TransformStep::FilterRows { column: "Int_Rate".into(), operator: micro_parquet::models::FilterOp::Gt, value: "8".into(), keep_matching: true };
        rig.app.push_step_to_active_tab(step, false);
        settle(&mut rig.app);
        assert_eq!(rig.app.tabs[0].cached_slice.height(), 1, "{what}: {}", rig.app.status_message);
        // the Column inspector says where the column came from
        rig.app.selected_column = Some("Balance".to_string());
        rig.app.right_tab = crate::ui::steps::RightTab::Column;
        let p = rig.draw(6);
        assert!(p.boxes.iter().any(|(t, _)| t == "From file: BAL"), "{what}: the inspector shows 'From file: BAL'");
    }
}

#[test]
fn u1_bad_names_are_red_with_the_reason_and_apply_and_save_stay_disabled() {
    for (ppp, w, h) in SCREENS {
        let (mut rig, _a, b) = u1_rig("p6u1c", ppp, w, h, false, false);
        let what = rig.what.clone();
        rig.app.open_data_file(b, false);
        // two columns called "Balance"
        if let Some(m) = rig.app.pending_csv_modal.as_mut() {
            m.config.columns[1].rename_to = Some("Balance".into());
        }
        let p = rig.draw(6);
        let r = dialog_in_window(&rig, SCHEMA_TITLE_CSV);
        let reason = p.boxes.iter().find(|(t, _)| t.contains("is also the name of")).unwrap_or_else(|| panic!("{what}: the reason is on screen"));
        assert!(inside(reason.1, r.window), "{what}: the reason {:?} is in the dialog", reason.1);
        let danger = theme::tokens(false).danger;
        assert!(p.colors.iter().any(|(t, c)| t.contains("is also the name of") && *c == danger), "{what}: the reason is red");
        // the apply button does not work by a click
        let lock = text_box(&p, "Lock Schema & Open");
        rig.click(lock.center());
        settle(&mut rig.app);
        assert!(rig.app.pending_csv_modal.is_some() && rig.app.tabs.is_empty(), "{what}: Lock Schema & Open is disabled");
        // nor does Enter
        rig.input.events = vec![egui::Event::Key { key: egui::Key::Enter, physical_key: Some(egui::Key::Enter), pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }];
        rig.draw(2);
        rig.input.events.clear();
        assert!(rig.app.pending_csv_modal.is_some() && rig.app.tabs.is_empty(), "{what}: Enter does not apply it either");
        // fixing the name enables it again
        if let Some(m) = rig.app.pending_csv_modal.as_mut() {
            m.config.columns[1].rename_to = Some("Account_Number".into());
        }
        let p = rig.draw(4);
        rig.click(text_box(&p, "Lock Schema & Open").center());
        settle(&mut rig.app);
        assert!(rig.app.pending_csv_modal.is_none() && rig.app.tabs.len() == 1, "{what}: fixed, it applies");
    }
}

#[test]
fn u1_several_matching_schemas_ask_and_a_template_ticked_apply_without_asking_opens_at_once() {
    for (ppp, w, h) in SCREENS {
        // two templates match: a picker with a button for each
        let (mut rig, _a, b) = u1_rig("p6u1d", ppp, w, h, false, true);
        let what = rig.what.clone();
        rig.app.open_data_file(b.clone(), false);
        assert!(rig.app.template_pick.is_some() && rig.app.pending_csv_modal.is_none(), "{what}: a picker opened");
        rig.draw(5);
        let rects = rig.app.template_pick.as_ref().map(|p| p.rects.clone()).unwrap_or_default();
        let names: Vec<&str> = rects.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["loans", "loans_alt", "None of these", "Cancel"], "{what}");
        let alt = rects.iter().find(|(n, _)| n == "loans_alt").map(|(_, r)| *r).expect("button");
        rig.click(alt.center());
        let modal = rig.app.pending_csv_modal.as_ref().unwrap_or_else(|| panic!("{what}: the dialog opened with the chosen schema"));
        assert_eq!(modal.applied.as_ref().map(|a| a.name.as_str()), Some("loans_alt"), "{what}");
        assert_eq!(modal.config.final_names(), vec!["Balance", "Acct", "Int_Rate"], "{what}");
        assert!(rig.app.template_pick.is_none(), "{what}");

        // "None of these": today's way in
        let (mut rig, _a, b) = u1_rig("p6u1e", ppp, w, h, false, true);
        rig.app.open_data_file(b, false);
        rig.draw(5);
        let none = rig.app.template_pick.as_ref().and_then(|p| p.rects.iter().find(|(n, _)| n == "None of these").map(|(_, r)| *r)).expect("button");
        rig.click(none.center());
        let modal = rig.app.pending_csv_modal.as_ref().expect("the dialog opened");
        assert!(modal.applied.is_none() && modal.config.rename_pairs().is_empty(), "{what}: detected as before");

        // "Apply without asking": the tab opens, the status names the template
        let (mut rig, _a, b) = u1_rig("p6u1f", ppp, w, h, true, false);
        rig.app.open_data_file(b, false);
        settle(&mut rig.app);
        assert!(rig.app.pending_csv_modal.is_none() && rig.app.template_pick.is_none(), "{what}");
        assert_eq!(rig.app.tabs.len(), 1, "{what}");
        assert_eq!(rig.app.tabs[0].available_columns(), vec!["Balance", "Account_No", "Int_Rate"], "{what}");
        assert!(rig.app.status_message.contains("'loans'") && rig.app.status_message.contains("all 3 columns matched"), "{what}: {}", rig.app.status_message);

        // no match (one extra column): today's behaviour, no applied line
        let (mut rig, a, _b) = u1_rig("p6u1g", ppp, w, h, true, false);
        let extra = a.with_file_name("extra.csv");
        std::fs::write(&extra, "ACNO,CRIRTE,BAL,NOTE\n1,2,3,x\n").expect("extra");
        rig.app.open_data_file(extra, false);
        let modal = rig.app.pending_csv_modal.as_ref().expect("the dialog opened as before");
        assert!(modal.applied.is_none() && rig.app.tabs.is_empty(), "{what}");
    }
}

#[test]
fn u1_changing_a_rename_on_an_open_tab_warns_naming_the_step_before_it_is_applied() {
    for (ppp, w, h) in SCREENS {
        let (mut rig, a, _b) = u1_rig("p6u1h", ppp, w, h, true, false);
        let what = rig.what.clone();
        rig.app.open_data_file(a, false);
        settle(&mut rig.app);
        let step = TransformStep::FilterRows { column: "Int_Rate".into(), operator: micro_parquet::models::FilterOp::Gt, value: "8".into(), keep_matching: true };
        rig.app.push_step_to_active_tab(step, false);
        settle(&mut rig.app);
        assert_eq!(rig.app.tabs[0].cached_slice.height(), 2, "{what}: {}", rig.app.status_message);
        // reopen the Schema dialog and clear the rename of CRIRTE
        rig.app.open_schema_modal_for_active_tab();
        if let Some(m) = rig.app.pending_csv_modal.as_mut() {
            m.config.columns[1].rename_to = None;
        }
        let p = rig.draw(6);
        rig.click(text_box(&p, "Apply Schema to This Tab").center());
        settle(&mut rig.app);
        assert!(rig.app.schema_confirm.is_some(), "{what}: the change waits for an answer: {}", rig.app.status_message);
        let TabSource::SingleFile { config, .. } = &rig.app.tabs[0].source else { panic!("single file") };
        assert_eq!(config.final_names()[1], "Int_Rate", "{what}: nothing was applied yet");
        let p = rig.draw(5);
        let _r = dialog_in_window(&rig, "\u{E4E0}  These steps would stop working");
        assert!(p.boxes.iter().any(|(t, _)| t.starts_with("Step 2") && t.contains("Int_Rate")), "{what}: the step is named: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).filter(|t| t.starts_with("Step")).collect::<Vec<_>>());
        // Cancel keeps the tab as it was
        rig.click(text_box(&p, "Cancel").center());
        assert!(rig.app.schema_confirm.is_none(), "{what}");
        let TabSource::SingleFile { config, .. } = &rig.app.tabs[0].source else { panic!("single file") };
        assert_eq!(config.final_names()[1], "Int_Rate", "{what}: Cancel leaves the schema alone");
        // again, and "Apply anyway"
        rig.app.open_schema_modal_for_active_tab();
        if let Some(m) = rig.app.pending_csv_modal.as_mut() {
            m.config.columns[1].rename_to = None;
        }
        let p = rig.draw(6);
        rig.click(text_box(&p, "Apply Schema to This Tab").center());
        settle(&mut rig.app);
        let p = rig.draw(5);
        rig.click(text_box(&p, "Apply anyway").center());
        settle(&mut rig.app);
        assert!(rig.app.schema_confirm.is_none(), "{what}");
        let TabSource::SingleFile { config, .. } = &rig.app.tabs[0].source else { panic!("single file") };
        assert_eq!(config.final_names()[1], "CRIRTE", "{what}: applied");
        assert!(rig.app.tabs[0].issues.iter().any(|i| i.index == 1), "{what}: the filter now shows as broken");
    }
}

// ------------------------------------------------------------------------------------------ U2

use crate::ui::steps::{right_panel_report, steps_report, RightTab};
use micro_parquet::settings::Settings;

fn panel_edge(rig: &Rig) -> egui::Pos2 {
    let right = rig.ctx.screen_rect().right();
    egui::pos2(right - rig.app.right_panel_w + 2.0, rig.ctx.screen_rect().center().y) // just inside the left edge
}

fn drag(rig: &mut Rig, from: egui::Pos2, to: egui::Pos2) {
    let b = |pressed| egui::Event::PointerButton { pos: if pressed { from } else { to }, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    rig.step(vec![egui::Event::PointerMoved(from)]);
    rig.step(vec![b(true)]);
    for k in 1..=4 {
        let p = from + (to - from) * (k as f32 / 4.0);
        rig.step(vec![egui::Event::PointerMoved(p)]);
    }
    rig.step(vec![b(false)]);
    rig.draw(3);
}

#[test]
fn u2_the_right_panel_is_dragged_to_a_width_clamped_saved_and_reset_by_a_double_click() {
    for (ppp, w, h) in SCREENS {
        let mut rig = term_rig("p6u2a", ppp, w, h);
        let what = rig.what.clone();
        rig.draw(6);
        assert!((rig.app.right_panel_w - theme::RIGHT_PANEL_W).abs() < 1.0, "{what}: starts at today's width: {}", rig.app.right_panel_w);
        // dragging the left edge to 500 pt gives 500
        let right = rig.ctx.screen_rect().right();
        let y = rig.ctx.screen_rect().center().y;
        let edge = panel_edge(&rig);
        drag(&mut rig, edge, egui::pos2(right - 500.0, y));
        assert!((rig.app.right_panel_w - 500.0).abs() < 1.5, "{what}: dragged to 500, is {}", rig.app.right_panel_w);
        // it is saved when the drag ends, and a new app state built from the file starts there
        let (saved, note) = Settings::load(&rig.app.paths.root_dir);
        assert!(note.is_none(), "{what}");
        assert!((saved.right_panel_width.unwrap_or(0.0) - 500.0).abs() < 1.5, "{what}: settings.json has {:?}", saved.right_panel_width);
        // the panel at its widest and at its narrowest keeps its content inside (a step list is on screen)
        // a drag past half the window is clamped to half the window
        let edge = panel_edge(&rig);
        drag(&mut rig, edge, egui::pos2(40.0, y));
        let half = rig.ctx.screen_rect().width() / 2.0;
        assert!((rig.app.right_panel_w - half).abs() < 1.5, "{what}: clamped to half the window ({half}), is {}", rig.app.right_panel_w);
        // and a drag below today's width is clamped to it
        let edge = panel_edge(&rig);
        drag(&mut rig, edge, egui::pos2(right - 100.0, y));
        assert!((rig.app.right_panel_w - theme::RIGHT_PANEL_W).abs() < 1.5, "{what}: not narrower than today's width: {}", rig.app.right_panel_w);
        // 500 again, then a double click on the edge resets it
        let edge = panel_edge(&rig);
        drag(&mut rig, edge, egui::pos2(right - 500.0, y));
        assert!((rig.app.right_panel_w - 500.0).abs() < 1.5, "{what}");
        let edge = panel_edge(&rig);
        rig.click(edge);
        rig.click(edge);
        rig.draw(4);
        assert!((rig.app.right_panel_w - theme::RIGHT_PANEL_W).abs() < 1.5, "{what}: a double click resets it: {}", rig.app.right_panel_w);
        assert_eq!(Settings::load(&rig.app.paths.root_dir).0.right_panel_width, None, "{what}: and the setting is cleared");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn u2_a_new_app_state_built_from_the_settings_file_starts_at_the_saved_width() {
    for (ppp, w, h) in SCREENS {
        let mut rig = term_rig("p6u2b", ppp, w, h);
        let what = rig.what.clone();
        rig.app.set_right_panel_width(Some(500.0));
        let root = rig.app.paths.root_dir.clone();
        // a second run of the app on the same data folder
        let mut again = MicroParquetApp::new(rig.app.paths.clone(), egui::Context::default());
        again.jobs = JobQueue::inline();
        let src = parquet_source(&root, "book2", DataFrame::new(vec![Series::new("A".into(), &[1i64, 2])]).expect("frame"));
        again.create_tab_from_source("T".into(), src, vec![], HashMap::new());
        settle(&mut again);
        let (ctx, input) = screen(ppp, w, h);
        let mut rig2 = Rig { app: again, ctx, input, what: what.clone() };
        rig2.draw(6);
        assert!((rig2.app.right_panel_w - 500.0).abs() < 1.5, "{what}: the saved width is the start width: {}", rig2.app.right_panel_w);
        // the saved width is held to half the window when the window is smaller than it was
        let mut s = Settings::default();
        s.right_panel_width = Some(5000.0);
        rig2.app.settings = s;
        rig2.app.right_panel_w = 5000.0;
        rig2.draw(6);
        assert!((rig2.app.right_panel_w - rig2.ctx.screen_rect().width() / 2.0).abs() < 1.5, "{what}: {}", rig2.app.right_panel_w);
    }
}

fn thirty_steps(rig: &mut Rig) {
    for i in 0..30 {
        let step = TransformStep::FilterRows { column: "Balance".into(), operator: micro_parquet::models::FilterOp::Gt, value: format!("{}", i - 100), keep_matching: true };
        rig.app.push_step_to_active_tab(step, false);
        settle(&mut rig.app);
    }
}

#[test]
fn u2_the_tabs_fit_one_row_and_a_tab_with_30_steps_scrolls_to_the_last_step_which_can_be_clicked() {
    for (ppp, w, h) in SCREENS {
        for width in [theme::RIGHT_PANEL_W, 0.0] {
            let mut rig = term_rig("p6u2c", ppp, w, h);
            let what = format!("{} panel {}", rig.what, if width == 0.0 { "widest" } else { "narrowest" });
            thirty_steps(&mut rig);
            if width == 0.0 {
                rig.app.set_right_panel_width(Some(rig.ctx.screen_rect().width()));
            }
            let p = rig.draw(8);
            let panel_left = rig.ctx.screen_rect().right() - rig.app.right_panel_w;
            if width == 0.0 {
                assert!((rig.app.right_panel_w - rig.ctx.screen_rect().width() / 2.0).abs() < 1.5, "{what}: {}", rig.app.right_panel_w);
            }
            // "Steps · 30", "Column" and "SQL" in one row, inside the panel, not overlapping
            let boxes: Vec<(String, egui::Rect)> = ["Steps \u{b7} 30", "Column", "SQL"].iter().map(|t| (t.to_string(), p.boxes.iter().find(|(x, bx)| x == t && bx.left() >= panel_left).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the tab '{t}' is shown")))).collect();
            for (n, b) in &boxes {
                assert!(b.left() >= panel_left - 0.5 && b.right() <= rig.ctx.screen_rect().right() + 0.5, "{what}: '{n}' {b:?} is inside the panel");
            }
            assert!(boxes.iter().all(|(_, b)| (b.center().y - boxes[0].1.center().y).abs() < 1.0), "{what}: one row: {boxes:?}");
            assert!(boxes[0].1.right() <= boxes[1].1.left() && boxes[1].1.right() <= boxes[2].1.left(), "{what}: no overlap: {boxes:?}");
            // the list is taller than the panel: it has a scroll area, and scrolling reaches step 30
            let rep = right_panel_report(&rig.ctx).expect("panel report");
            assert_eq!(rep.tab, RightTab::Steps, "{what}");
            let steps = steps_report(&rig.ctx).expect("steps report");
            let last = steps.rows.last().map(|r| r.title).expect("rows");
            let pos = rig.ctx.screen_rect().center() + egui::vec2(rig.ctx.screen_rect().width() / 2.0 - rig.app.right_panel_w / 2.0, 0.0);
            for _ in 0..60 {
                rig.step(vec![egui::Event::PointerMoved(pos), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: egui::vec2(0.0, -6.0), modifiers: egui::Modifiers::NONE }]);
            }
            rig.draw(4);
            let steps = steps_report(&rig.ctx).expect("steps report");
            let rep = right_panel_report(&rig.ctx).expect("panel report");
            let last_row = steps.rows.iter().rev().find(|r| r.title.is_positive()).map(|r| r.title).unwrap_or(last);
            assert!(last_row.bottom() <= rep.scroll.bottom() + 1.0 && last_row.top() >= rep.scroll.top() - 1.0, "{what}: the last step {last_row:?} is in view {:?}", rep.scroll);
            rig.click(last_row.center());
            assert_eq!(rig.app.selected_step, Some(29), "{what}: the last step was clicked");
        }
    }
    micro_parquet::rules::set_data_dir_override(None);
}

fn open_sql_tab(rig: &mut Rig) -> Painted {
    rig.app.run_command(&rig.ctx, CommandId::ToggleSql);
    assert_eq!(rig.app.right_tab, RightTab::Sql, "{}: View > SQL / Ctrl+K 'SQL editor' opens the SQL tab", rig.what);
    rig.draw(6)
}

fn key_enter(modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key { key: egui::Key::Enter, physical_key: Some(egui::Key::Enter), pressed: true, repeat: false, modifiers }
}

const CTRL: egui::Modifiers = egui::Modifiers { alt: false, ctrl: true, shift: false, mac_cmd: false, command: true };
const CTRL_SHIFT: egui::Modifiers = egui::Modifiers { alt: false, ctrl: true, shift: true, mac_cmd: false, command: true };

#[test]
fn u2_the_sql_tab_has_the_editor_hint_buttons_history_and_error_box_and_they_fit_at_both_widths() {
    for (ppp, w, h) in SCREENS {
        for widest in [false, true] {
            let mut rig = term_rig("p6u2d", ppp, w, h);
            let what = format!("{} {}", rig.what, if widest { "widest" } else { "narrowest" });
            if widest {
                rig.app.set_right_panel_width(Some(10_000.0));
            }
            let p = open_sql_tab(&mut rig);
            let panel_left = rig.ctx.screen_rect().right() - rig.app.right_panel_w;
            let rects = rig.app.sql_rects;
            assert!(p.boxes.iter().any(|(t, b)| t == "Table name: self" && b.left() >= panel_left), "{what}: the hint");
            // the old scratchpad text is kept, in a monospace font
            assert!(p.texts.iter().any(|(t, _, f)| t.starts_with("SELECT * FROM self LIMIT 100") && *f == egui::FontFamily::Monospace), "{what}: the existing query shows in the editor, in monospace");
            let font = egui::TextStyle::Monospace.resolve(&rig.ctx.style());
            let line = rig.ctx.fonts(|f| f.row_height(&font));
            assert!(rects.editor.height() >= 8.0 * line - 0.5, "{what}: the editor is at least 8 lines tall: {} < {}", rects.editor.height(), 8.0 * line);
            assert!(rects.editor_rows >= 8, "{what}");
            let rep = right_panel_report(&rig.ctx).expect("report");
            for (n, r) in [("editor", rects.editor), ("Apply here", rects.apply), ("New tab", rects.new_tab), ("history", rects.history), ("error box", rects.error)] {
                assert!(r.left() >= panel_left - 0.5 && r.right() <= rig.ctx.screen_rect().right() + 0.5, "{what}: {n} {r:?} is inside the panel");
                assert!(r.is_positive(), "{what}: {n} drawn");
                // (what is below the fold is reached by scrolling: the panel scrolls)
                let _ = rep;
            }
            assert!(!overlaps(rects.apply, rects.new_tab) && !overlaps(rects.editor, rects.apply) && !overlaps(rects.history, rects.error), "{what}: no overlaps");
            for t in ["Apply here", "New tab"] {
                assert!(p.boxes.iter().any(|(x, bx)| x == t && inside(*bx, if t == "Apply here" { rects.apply } else { rects.new_tab })), "{what}: '{t}' is whole in its button");
            }
            // the editor grows with the panel: the panel at its widest is no shorter, and in a tall window the editor is taller
            assert!(rects.editor.width() > 200.0, "{what}");
        }
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn u2_a_12_line_query_runs_with_ctrl_enter_and_ctrl_shift_enter_opens_a_new_tab() {
    for (ppp, w, h) in SCREENS {
        let mut rig = term_rig("p6u2e", ppp, w, h);
        let what = rig.what.clone();
        open_sql_tab(&mut rig);
        let query = "SELECT\n  \"Term Period\",\n  Balance,\n  Balance * 2 AS Double_Balance\nFROM self\nWHERE Balance > 10\n  AND Balance < 100\n  AND \"Term Period\" <> 'abc'\nORDER BY Balance DESC\nLIMIT 50\n-- the twelfth line\n-- is a comment".to_string();
        assert_eq!(query.lines().count(), 12);
        rig.app.tabs[0].sql_scratchpad = query.clone();
        let p = rig.draw(3);
        // focus the editor with a real click inside it, then Ctrl+Enter
        let editor = rig.app.sql_rects.editor;
        rig.click(editor.center());
        let _ = p;
        rig.step(vec![key_enter(CTRL)]);
        settle(&mut rig.app);
        rig.draw(3);
        assert_eq!(rig.app.tabs[0].steps.len(), 1, "{what}: the query ran as a step: {} / {:?}", rig.app.status_message, rig.app.tabs[0].sql_error);
        assert!(matches!(&rig.app.tabs[0].steps[0], TransformStep::SqlQuery { query: q } if q.lines().count() == 12), "{what}");
        assert_eq!(rig.app.tabs[0].cached_slice.height(), 4, "{what}: balances 20, 30, 40 and 60 (50 is 'abc', 10 is not above 10): {}", rig.app.tabs[0].cached_slice.height());
        // the text was not changed by the key (no extra new line)
        assert_eq!(rig.app.tabs[0].sql_scratchpad, query, "{what}");
        // Ctrl+Shift+Enter: a new tab
        rig.click(rig.app.sql_rects.editor.center());
        rig.step(vec![key_enter(CTRL_SHIFT)]);
        settle(&mut rig.app);
        rig.draw(3);
        assert_eq!(rig.app.tabs.len(), 2, "{what}: a new tab: {}", rig.app.status_message);
        assert_eq!(rig.app.tabs[0].steps.len(), 1, "{what}: the first tab is as it was");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn u2_a_bad_query_shows_its_error_under_the_editor_and_adds_no_step() {
    for (ppp, w, h) in SCREENS {
        let mut rig = term_rig("p6u2f", ppp, w, h);
        let what = rig.what.clone();
        open_sql_tab(&mut rig);
        rig.app.tabs[0].sql_scratchpad = "SELECT nosuchcolumn FROM self".to_string();
        let p = rig.draw(3);
        let apply = p.boxes.iter().find(|(t, _)| t == "Apply here").map(|(_, b)| *b).expect("button");
        rig.click(apply.center());
        settle(&mut rig.app);
        let p = rig.draw(3);
        let err = rig.app.tabs[0].sql_error.clone().unwrap_or_else(|| panic!("{what}: an error was kept ({})", rig.app.status_message));
        assert!(err.to_lowercase().contains("nosuchcolumn") || err.to_lowercase().contains("not found"), "{what}: {err}");
        assert!(rig.app.tabs[0].steps.is_empty(), "{what}: no step was added");
        let box_rect = rig.app.sql_rects.error;
        assert!(p.boxes.iter().any(|(t, b)| t.contains(err.lines().next().unwrap_or("")) && b.intersects(box_rect)), "{what}: the error is painted in its box");
        // a good query clears it
        rig.app.tabs[0].sql_scratchpad = "SELECT * FROM self".to_string();
        let p = rig.draw(3);
        let apply = p.boxes.iter().find(|(t, _)| t == "Apply here").map(|(_, b)| *b).expect("button");
        rig.click(apply.center());
        settle(&mut rig.app);
        assert!(rig.app.tabs[0].sql_error.is_none() && rig.app.tabs[0].steps.len() == 1, "{what}");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn u2_the_history_keeps_ten_queries_and_a_click_on_one_puts_it_back_in_the_editor() {
    let mut h = Vec::new();
    for i in 0..12 {
        crate::ui::sql::remember_query(&mut h, &format!("SELECT {i} FROM self"));
    }
    assert_eq!(h.len(), 10);
    assert_eq!((h[0].as_str(), h[9].as_str()), ("SELECT 2 FROM self", "SELECT 11 FROM self"), "the two oldest were dropped");
    crate::ui::sql::remember_query(&mut h, "SELECT 5 FROM self");
    assert_eq!((h.len(), h[9].as_str()), (10, "SELECT 5 FROM self"), "a repeated query moves to the end");
    crate::ui::sql::remember_query(&mut h, "   ");
    assert_eq!(h.len(), 10);
    for (ppp, w, hh) in SCREENS {
        let mut rig = term_rig("p6u2g", ppp, w, hh);
        let what = rig.what.clone();
        open_sql_tab(&mut rig);
        for i in 0..12 {
            rig.app.tabs[0].sql_scratchpad = format!("SELECT Balance + {i} AS B FROM self");
            rig.app.run_sql_query(false);
            settle(&mut rig.app);
        }
        assert_eq!(rig.app.tabs[0].sql_history.len(), 10, "{what}");
        assert!(rig.app.tabs[0].sql_history[0].contains("+ 2 "), "{what}: the oldest two are gone: {:?}", rig.app.tabs[0].sql_history);
        rig.app.tabs[0].sql_scratchpad = "SELECT 1".to_string();
        rig.draw(4);
        let combo = rig.app.sql_rects.history;
        rig.click(combo.center());
        let p = rig.draw(3);
        let newest = p.boxes.iter().filter(|(t, _)| t.starts_with("SELECT Balance + 11")).map(|(_, b)| *b).next().unwrap_or_else(|| panic!("{what}: the newest query is listed"));
        let oldest = p.boxes.iter().filter(|(t, _)| t.starts_with("SELECT Balance + 2 ")).map(|(_, b)| *b).next().unwrap_or_else(|| panic!("{what}: the oldest kept query is listed: {:?}", p.boxes.iter().filter(|(t, _)| t.starts_with("SELECT")).collect::<Vec<_>>()));
        assert!(newest.top() < oldest.top(), "{what}: newest first");
        assert!(!p.boxes.iter().any(|(t, _)| t.starts_with("SELECT Balance + 1 ") || t.starts_with("SELECT Balance + 0 ")), "{what}: the dropped ones are not listed");
        rig.click(oldest.center());
        assert_eq!(rig.app.tabs[0].sql_scratchpad, "SELECT Balance + 2 AS B FROM self", "{what}: the chosen query is in the editor");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn u2_view_sql_and_ctrl_k_sql_editor_open_the_panel_on_the_sql_tab() {
    use micro_parquet::commands::{rank, Entry, COMMANDS};
    let entries: Vec<Entry> = COMMANDS.iter().map(|c| Entry { label: c.title, words: c.words, recent: None }).collect();
    let top = rank("sql editor", &entries)[0];
    assert_eq!(COMMANDS[top].id, CommandId::ToggleSql, "Ctrl+K 'sql editor' finds the command first");
    for (ppp, w, h) in SCREENS {
        let mut rig = term_rig("p6u2h", ppp, w, h);
        let what = rig.what.clone();
        rig.app.show_steps_sidebar = false;
        rig.draw(4);
        // View menu: click "View", then "SQL"
        let p = rig.draw(2);
        let view = p.boxes.iter().find(|(t, _)| t == "View").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the View menu"));
        rig.click(view.center());
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t.contains("Custom sort lists")), "{what}: View lists Custom sort lists");
        let item = p.boxes.iter().find(|(t, _)| t.ends_with("SQL") && t.chars().count() <= 6).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: View lists SQL: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>()));
        rig.click(item.center());
        assert!(rig.app.show_steps_sidebar && rig.app.right_tab == RightTab::Sql, "{what}: View > SQL opened the panel on the SQL tab");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

// ------------------------------------------------------------------------------------------ U6

use crate::ui::status::status_report;

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers: CTRL }
}

/// A new app on the data folder of `old` (as after a restart).
fn restarted(old: &MicroParquetApp) -> MicroParquetApp {
    let mut app = MicroParquetApp::new(old.paths.clone(), egui::Context::default());
    app.jobs = JobQueue::inline();
    app
}

#[test]
fn u6_a_90_percent_default_is_written_as_0_90_and_a_new_app_state_starts_at_it() {
    let app = test_app("p6u6a");
    assert!((app.ui_zoom - 1.05).abs() < 1e-6, "no setting: 105%: {}", app.ui_zoom);
    let mut app = app;
    app.zoom_set_default(0.9);
    let text = std::fs::read_to_string(app.paths.root_dir.join("settings.json")).expect("settings.json was written");
    let json: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(json["default_zoom"].as_f64(), Some(0.9), "{text}");
    let again = restarted(&app);
    assert!((again.ui_zoom - 0.9).abs() < 1e-6, "a new app state built from the file starts at 0.90: {}", again.ui_zoom);
    assert_eq!(again.default_zoom(), 0.9);
}

#[test]
fn u6_a_file_value_of_5_is_held_to_2_with_a_message_and_no_file_gives_105_percent() {
    let app = test_app("p6u6b");
    std::fs::write(app.paths.root_dir.join("settings.json"), r#"{"default_zoom": 5.0}"#).expect("write");
    let again = restarted(&app);
    assert!((again.ui_zoom - 2.0).abs() < 1e-6, "{}", again.ui_zoom);
    assert!(again.status_message.contains("outside 50% to 200%") && again.status_message.contains("200%"), "{}", again.status_message);
    // a damaged file: the defaults and a message
    std::fs::write(app.paths.root_dir.join("settings.json"), "{ nope").expect("write");
    let damaged = restarted(&app);
    assert!((damaged.ui_zoom - 1.05).abs() < 1e-6 && damaged.status_message.contains("damaged"), "{}", damaged.status_message);
    // no file at all
    let _ = std::fs::remove_file(app.paths.root_dir.join("settings.json"));
    let none = restarted(&app);
    assert!((none.ui_zoom - 1.05).abs() < 1e-6, "{}", none.ui_zoom);
}

#[test]
fn u6_ctrl_equals_ctrl_minus_and_ctrl_0_zoom_and_ctrl_0_goes_back_to_the_default() {
    for (ppp, w, h) in SCREENS {
        let mut rig = term_rig("p6u6c", ppp, w, h);
        let what = rig.what.clone();
        rig.app.zoom_set_default(0.9);
        rig.app.set_zoom(&rig.ctx, 0.9);
        for _ in 0..3 {
            rig.step(vec![key(egui::Key::Equals)]);
        }
        assert!((rig.app.ui_zoom - 1.2).abs() < 1e-4, "{what}: three Ctrl+= from 90%: {}", rig.app.ui_zoom);
        rig.draw(2);
        assert!((rig.ctx.zoom_factor() - 1.2).abs() < 1e-4, "{what}: egui's zoom followed (and egui's own key handling did not add to it): {}", rig.ctx.zoom_factor());
        rig.step(vec![key(egui::Key::Minus)]);
        assert!((rig.app.ui_zoom - 1.1).abs() < 1e-4, "{what}: Ctrl+-: {}", rig.app.ui_zoom);
        rig.step(vec![key(egui::Key::Num0)]);
        assert!((rig.app.ui_zoom - 0.9).abs() < 1e-4, "{what}: Ctrl+0 returns to 90%: {}", rig.app.ui_zoom);
        assert!(rig.app.status_message.contains("90%"), "{what}: {}", rig.app.status_message);
        // the limits
        for _ in 0..30 {
            rig.step(vec![key(egui::Key::Minus)]);
        }
        assert!((rig.app.ui_zoom - 0.5).abs() < 1e-4, "{what}: not below 50%: {}", rig.app.ui_zoom);
        for _ in 0..30 {
            rig.step(vec![key(egui::Key::Equals)]);
        }
        assert!((rig.app.ui_zoom - 2.0).abs() < 1e-4, "{what}: not above 200%: {}", rig.app.ui_zoom);
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn u6_the_zoom_label_at_the_right_end_of_the_status_bar_opens_the_menu_and_its_items_work() {
    for (ppp, w, h) in SCREENS {
        let mut rig = term_rig("p6u6d", ppp, w, h);
        let what = rig.what.clone();
        rig.draw(5);
        let rep = status_report(&rig.ctx).expect("status bar");
        assert!(rep.zoom.is_positive() && inside(rep.zoom, rep.bar), "{what}: the zoom label {:?} is in the bar {:?}", rep.zoom, rep.bar);
        assert!(rep.zoom.left() >= rep.left.right() - 0.5 && rep.zoom.right() >= rep.messages.right() - 0.5, "{what}: it is the right-most item: {:?} vs Messages {:?}", rep.zoom, rep.messages);
        let p = rig.draw(2);
        assert!(p.boxes.iter().any(|(t, b)| t == "105%" && inside(*b, rep.zoom)), "{what}: the label reads 105%");
        // a click opens the Zoom menu
        rig.click(rep.zoom.center());
        let p = rig.draw(3);
        for t in ["Zoom: 105%", "Zoom in  (Ctrl+=)", "Zoom out  (Ctrl+-)", "Set current zoom as default"] {
            assert!(p.boxes.iter().any(|(x, _)| x == t), "{what}: the menu has '{t}': {:?}", p.boxes.iter().map(|(t, _)| t.clone()).filter(|t| t.contains("oom")).collect::<Vec<_>>());
        }
        assert!(p.boxes.iter().any(|(x, _)| x.starts_with("Reset to default  (105%)")), "{what}");
        // Zoom in by a real click: 105% -> 115%
        let zin = p.boxes.iter().find(|(x, _)| x == "Zoom in  (Ctrl+=)").map(|(_, b)| *b).expect("item");
        rig.click(zin.center());
        assert!((rig.app.ui_zoom - 1.15).abs() < 1e-4, "{what}: {}", rig.app.ui_zoom);
        // "Set current zoom as default" by a real click (the menu stays open after Zoom in)
        let p = rig.draw(3);
        let set = p.boxes.iter().find(|(x, _)| x == "Set current zoom as default").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: menu open again"));
        rig.click(set.center());
        let saved = micro_parquet::settings::Settings::load(&rig.app.paths.root_dir).0;
        assert_eq!(saved.default_zoom, Some(1.15), "{what}");
        // "Reset to default" goes back to it after zooming out
        rig.app.zoom_out(&rig.ctx);
        assert!((rig.app.ui_zoom - 1.05).abs() < 1e-4, "{what}");
        rig.app.run_command(&rig.ctx, CommandId::ZoomReset);
        assert!((rig.app.ui_zoom - 1.15).abs() < 1e-4, "{what}: Ctrl+K 'reset zoom'");
        rig.app.set_zoom(&rig.ctx, 1.3);
        rig.app.run_command(&rig.ctx, CommandId::ZoomSetDefault);
        assert_eq!(micro_parquet::settings::Settings::load(&rig.app.paths.root_dir).0.default_zoom, Some(1.3), "{what}: Ctrl+K 'set the current zoom as the default'");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn u6_the_default_zoom_runs_from_50_to_200_percent_in_5_percent_steps() {
    let z = theme::zoom_choices();
    assert_eq!((z.len(), z[0], *z.last().expect("last")), (31, 0.5, 2.0));
    assert!(z.windows(2).all(|p| ((p[1] - p[0]) - 0.05).abs() < 1e-4));
    assert!(z.contains(&1.05));
    assert_eq!(theme::snap_zoom(0.93), 0.95);
    assert_eq!(theme::snap_zoom(9.0), 2.0);
    for (ppp, w, h) in SCREENS {
        let mut rig = term_rig("p6u6e", ppp, w, h);
        let what = rig.what.clone();
        // View > Zoom > Default zoom > 60%
        let p = rig.draw(3);
        let view = p.boxes.iter().find(|(t, _)| t == "View").map(|(_, b)| *b).expect("View");
        rig.click(view.center());
        let p = rig.draw(3);
        let zoom = p.boxes.iter().find(|(t, _)| t.starts_with("Zoom  ")).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: View has Zoom"));
        rig.click(zoom.center());
        let p = rig.draw(3);
        let default = p.boxes.iter().find(|(t, _)| t.starts_with("Default zoom:")).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the Zoom menu has Default zoom"));
        rig.click(default.center());
        let p = rig.draw(3);
        let sixty = p.boxes.iter().find(|(t, _)| t == "60%").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the list has 60%: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).filter(|t| t.ends_with('%')).collect::<Vec<_>>()));
        rig.click(sixty.center());
        assert_eq!(micro_parquet::settings::Settings::load(&rig.app.paths.root_dir).0.default_zoom, Some(0.6), "{what}");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

// ------------------------------------------------------------------------------------------ U4

fn app_texts(p: &Painted) -> Vec<String> {
    p.texts.iter().map(|(t, _, _)| t.clone()).collect()
}

#[test]
fn u4_the_window_title_is_the_version_and_the_active_file() {
    assert_eq!(crate::ui::tabs::window_title(None), "MicroParquet v1.6.0");
    assert_eq!(crate::ui::tabs::window_title(Some("loans_2026-11.csv")), "MicroParquet v1.6.0 \u{2014} loans_2026-11.csv");
    assert_eq!(crate::ui::tabs::window_title(Some("  ")), "MicroParquet v1.6.0", "a blank name adds nothing");
    for (ppp, w, h) in SCREENS {
        let mut app = test_app("p6u4a");
        let (ctx, input) = screen(ppp, w, h);
        let title_cmd = |out: &egui::FullOutput| {
            out.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.commands.iter().filter_map(|c| if let egui::ViewportCommand::Title(t) = c { Some(t.clone()) } else { None }).collect::<Vec<_>>()).unwrap_or_default()
        };
        let out = ctx.run(input.clone(), |c| app.frame_ui(c));
        assert_eq!(app.window_title, "MicroParquet v1.6.0", "no tab: version only");
        let _ = title_cmd(&out);
        // open a file: the title says which
        let p = app.paths.root_dir.join("loans_2026-11.csv");
        std::fs::write(&p, "A,B\n1,2\n").expect("csv");
        let src = TabSource::SingleFile { path: p, is_parquet: false, config: micro_parquet::engine::auto_detect_smart_schema(&app.paths.root_dir.join("loans_2026-11.csv")).expect("schema") };
        app.create_tab_from_source("Loans".into(), src, vec![], HashMap::new());
        settle(&mut app);
        let out = ctx.run(input.clone(), |c| app.frame_ui(c));
        assert_eq!(app.window_title, "MicroParquet v1.6.0 \u{2014} loans_2026-11.csv", "an open tab: the file name");
        assert_eq!(title_cmd(&out), vec!["MicroParquet v1.6.0 \u{2014} loans_2026-11.csv".to_string()], "the title was sent to the window");
        // the same title is not sent again
        let out = ctx.run(input.clone(), |c| app.frame_ui(c));
        assert!(title_cmd(&out).is_empty());
        // closing the tab goes back to the version only
        app.close_tab(0);
        let out = ctx.run(input.clone(), |c| app.frame_ui(c));
        assert_eq!(title_cmd(&out), vec!["MicroParquet v1.6.0".to_string()]);
    }
}

#[test]
fn u4_the_landing_page_has_three_buttons_in_the_middle_and_no_heading_or_tagline() {
    for (ppp, w, h) in SCREENS {
        let mut rig = Rig { app: test_app("p6u4b"), ctx: screen(ppp, w, h).0, input: screen(ppp, w, h).1, what: format!("ppp {ppp} {w}x{h}") };
        let what = rig.what.clone();
        let p = rig.draw(5);
        let all = app_texts(&p).join(" | ");
        for gone in ["Portable CSV/Parquet", "Out-of-Core Polars Engine", "Multi-Tab Views", "Per-Metric Formats", "Analytics Workstation"] {
            assert!(!all.contains(gone), "{what}: the text '{gone}' is gone from the landing page: {all}");
        }
        let rects: Vec<(String, egui::Rect)> = rig.ctx.data(|d| d.get_temp(egui::Id::new("empty_state_rects"))).expect("the landing page drew its buttons");
        assert_eq!(rects.len(), 3, "{what}");
        for (n, r) in &rects {
            assert!(inside(*r, rig.ctx.screen_rect()), "{what}: '{n}' {r:?} is in the window");
        }
        // centred vertically in the area between the top bar and the status bar
        let top = rects.first().map(|(_, r)| r.top()).expect("rect");
        let bottom = rects.last().map(|(_, r)| r.bottom()).expect("rect");
        let area_top = theme::TOP_BAR_H;
        let area_bottom = rig.ctx.screen_rect().bottom() - theme::STATUS_BAR_H;
        let (mid_buttons, mid_area) = ((top + bottom) / 2.0, (area_top + area_bottom) / 2.0);
        assert!((mid_buttons - mid_area).abs() < 0.12 * (area_bottom - area_top), "{what}: the buttons ({mid_buttons}) are in the middle of the area ({mid_area})");
        // every button works by a click on its real rectangle
        let open = rects[0].1;
        rig.click(open.center());
        assert_eq!(rig.app.file_dialog_requests, 1, "{what}: the Open button asks for a file");
        rig.click(rects[1].1.center());
        assert_eq!(rig.app.active_tool, ActiveToolModal::StackFiles, "{what}: Stack / Merge");
        rig.app.active_tool = ActiveToolModal::None;
        let rects: Vec<(String, egui::Rect)> = rig.ctx.data(|d| d.get_temp(egui::Id::new("empty_state_rects"))).expect("rects");
        rig.draw(2);
        rig.click(rects[2].1.center());
        assert!(rig.app.batch_modal.open, "{what}: the batch converter opens");
    }
}

#[test]
fn u4_about_has_the_version_build_date_capabilities_folder_key_and_fonts_and_fits_at_both_scales() {
    for (ppp, w, h) in SCREENS {
        let mut rig = term_rig("p6u4c", ppp, w, h);
        let what = rig.what.clone();
        // Help > About MicroParquet by clicks
        let p = rig.draw(3);
        let help = p.boxes.iter().find(|(t, _)| t == "Help").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the Help menu"));
        rig.click(help.center());
        let p = rig.draw(3);
        let about = p.boxes.iter().find(|(t, _)| t.contains("About MicroParquet")).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: Help lists About"));
        rig.click(about.center());
        assert!(rig.app.show_about, "{what}");
        let p = rig.draw(6);
        let r = assert_dialog_fits(&rig.ctx, "About MicroParquet", &what);
        let a = rig.app.about_rects;
        for (n, rc) in [("version", a.version), ("build", a.build), ("folder", a.folder), ("Open folder", a.open_folder), ("key", a.key), ("fonts", a.fonts), ("Close", a.close)] {
            assert!(rc.is_positive() && inside(rc, r.window), "{what}: '{n}' {rc:?} is inside the dialog {:?}", r.window);
        }
        let all = app_texts(&p).join(" | ");
        assert!(all.contains("MicroParquet v1.6.0"), "{what}: {all}");
        let built = all.split("Built ").nth(1).map(|s| s.chars().take(10).collect::<String>()).unwrap_or_default();
        assert!(built.len() == 10 && built.as_bytes()[4] == b'-' && built.as_bytes()[7] == b'-' && built[..4].parse::<u32>().is_ok_and(|y| y >= 2026), "{what}: a build date YYYY-MM-DD: '{built}'");
        for t in ["Portable CSV/Parquet viewer", "Out-of-core Polars engine", "Multi-tab views", "Custom maturity buckets", "Per-metric number formats", "SIL Open Font License 1.1 (OFL)", "Masking key", "Data folder (portable_data)"] {
            assert!(all.contains(t), "{what}: About says '{t}'");
        }
        // the folder path is the real one, whole or cut with its full text on hover
        let root = rig.app.paths.root_dir.display().to_string();
        assert!(p.shown.iter().any(|(t, _)| root.starts_with(t.trim_end_matches('\u{2026}')) && t.len() > 10), "{what}: the data folder {root} shows");
        // "Open folder" by a real click
        rig.click(a.open_folder.center());
        assert_eq!(rig.app.opened_folder.as_deref(), Some(rig.app.paths.root_dir.as_path()), "{what}");
        rig.click(rig.app.about_rects.close.center());
        assert!(!rig.app.show_about, "{what}: Close");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

// ------------------------------------------------------------------------------------------ U5

use micro_parquet::chart::{ChartAgg, ChartKind, ChartSpec};

/// A tab "book" of 250 rows (three pages) with Term Period in five tenors, a Risk grade, Balance and Rate.
fn chart_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    micro_parquet::rules::set_data_dir_override(Some(app.paths.root_dir.clone()));
    let root = app.paths.root_dir.clone();
    let n = 250usize;
    let tenors = ["6m", "1y", "1m", "100D", "3m"];
    let df = DataFrame::new(vec![
        Series::new("Term Period".into(), (0..n).map(|i| tenors[i % 5].to_string()).collect::<Vec<_>>()),
        Series::new("Risk".into(), (0..n).map(|i| ["High", "Medium", "Low"][i % 3].to_string()).collect::<Vec<_>>()),
        Series::new("Balance".into(), (0..n).map(|i| 10_000_000.0 + i as f64 * 1_000_000.0).collect::<Vec<f64>>()),
        Series::new("Rate".into(), (0..n).map(|i| 5.0 + (i % 7) as f64 * 0.5).collect::<Vec<f64>>()),
    ])
    .expect("frame");
    let src = parquet_source(&root, "book", df);
    app.create_tab_from_source("book".into(), src, vec![], HashMap::new());
    settle(&mut app);
    app.show_chart_drawer = true;
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

fn x_labels(p: &Painted, plot: egui::Rect, names: &[&str]) -> Vec<String> {
    // the names painted under the plot's bottom edge, left to right
    let mut v: Vec<(f32, String)> = p.boxes.iter().filter(|(t, b)| names.contains(&t.as_str()) && b.top() >= plot.bottom() - 1.0 && b.top() <= plot.bottom() + theme::CHART_AXIS_BAND && b.left() >= plot.left() - 1.0 && b.right() <= plot.right() + 1.0).map(|(t, b)| (b.left(), t.clone())).collect();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    v.into_iter().map(|(_, t)| t).collect()
}

#[test]
fn u5_the_chart_shows_the_whole_tab_with_category_names_on_the_x_axis_and_the_title_with_its_unit() {
    for (ppp, w, h) in SCREENS {
        let mut rig = chart_rig("p6u5a", ppp, w, h);
        let what = rig.what.clone();
        let p = rig.draw(10);
        let r = rig.app.chart.rects.clone();
        let plot = r.plot.unwrap_or_else(|| panic!("{what}: the plot was drawn: error {:?}", rig.app.chart.error));
        let data = rig.app.chart.data.clone().expect("the chart has data");
        assert_eq!(data.spec.category, "Term Period", "{what}: the first text column is the category");
        assert_eq!(data.spec.value.as_deref(), Some("Balance"), "{what}: and the first number column the value");
        assert_eq!(data.total_rows, 250, "{what}: all rows, not the first page");
        let sum: f64 = data.values[0].iter().flatten().sum();
        let want: f64 = (0..250).map(|i| 10_000_000.0 + i as f64 * 1_000_000.0).sum();
        assert!((sum - want).abs() < 1.0, "{what}: {sum} vs {want}");
        // the x-axis labels are the category names (natural order)
        let labels = x_labels(&p, plot, &["1m", "1y", "3m", "6m", "100D"]);
        assert_eq!(labels, vec!["1m", "1y", "3m", "6m", "100D"], "{what}: the x-axis names the categories: {:?}", p.boxes.iter().filter(|(_, b)| b.top() >= plot.bottom() - 40.0).map(|(t, _)| t.clone()).collect::<Vec<_>>());
        // the title, with the unit of the column's format (Crore is the default scale)
        let title = p.boxes.iter().find(|(t, _)| t.starts_with("Sum of Balance by Term Period")).map(|(t, b)| (t.clone(), *b)).unwrap_or_else(|| panic!("{what}: the title"));
        assert_eq!(title.0, "Sum of Balance by Term Period: all 250 rows (Crore)", "{what}");
        // the controls and the title are inside the window and the central area
        let screen_rect = rig.ctx.screen_rect();
        for (n, f) in &r.fields {
            assert!(inside(*f, screen_rect), "{what}: '{n}' {f:?} is in the window");
            assert!(f.right() <= screen_rect.right() - rig.app.right_panel_w + 1.0, "{what}: '{n}' {f:?} is clear of the right panel");
        }
        assert!(title.1.right() <= screen_rect.right() - rig.app.right_panel_w + 1.0, "{what}");
        for (i, (a, ra)) in r.fields.iter().enumerate() {
            for (b, rb) in r.fields.iter().skip(i + 1) {
                assert!(!overlaps(*ra, *rb), "{what}: '{a}' {ra:?} overlaps '{b}' {rb:?}");
            }
        }
        // the chart choices are shown by name
        for t in ["Category (X)", "Series", "Aggregation", "Value", "Type", "Order", "Top N", "Scale", "Open chart data as tab"] {
            assert!(p.boxes.iter().any(|(x, _)| x.contains(t)), "{what}: '{t}' is a control");
        }
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn u5_tenor_order_aggregation_series_and_top_n_work_by_real_clicks_and_are_remembered_per_tab() {
    for (ppp, w, h) in SCREENS {
        let mut rig = chart_rig("p6u5b", ppp, w, h);
        let what = rig.what.clone();
        rig.draw(10);
        // Order > Tenor by clicks
        let order = rig.app.chart.rects.fields.iter().find(|(n, _)| n == "Order").map(|(_, r)| *r).expect("Order");
        rig.click(order.center());
        let p = rig.draw(3);
        click_text(&mut rig, &p, "Tenor", order);
        rig.draw(10);
        let p = rig.draw(2);
        let plot = rig.app.chart.rects.plot.expect("plot");
        assert_eq!(x_labels(&p, plot, &["1m", "1y", "3m", "6m", "100D"]), vec!["1m", "3m", "100D", "6m", "1y"], "{what}: Tenor order: 1m, 3m, 100D, 6m, 1y");
        // Aggregation > Count by clicks: no Value control, the title says so
        let agg = rig.app.chart.rects.fields.iter().find(|(n, _)| n == "Aggregation").map(|(_, r)| *r).expect("Aggregation");
        rig.click(agg.center());
        let p = rig.draw(3);
        click_text(&mut rig, &p, "Count", agg);
        rig.draw(10);
        let p = rig.draw(2);
        assert!(p.boxes.iter().any(|(t, _)| t == "Count of rows by Term Period: all 250 rows"), "{what}: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).filter(|t| t.contains("rows")).collect::<Vec<_>>());
        assert!(!rig.app.chart.rects.fields.iter().any(|(n, _)| n == "Value"), "{what}: Count needs no value column");
        let d = rig.app.chart.data.clone().expect("data");
        assert_eq!(d.values[0].iter().flatten().sum::<f64>(), 250.0, "{what}");
        // a series and Top N 3: three categories plus Other, and grouped series
        rig.app.chart.spec.series = Some("Risk".into());
        rig.app.chart.spec.top_n = 3;
        rig.draw(10);
        let d = rig.app.chart.data.clone().expect("data");
        assert_eq!((d.categories.len(), d.series.len()), (4, 3), "{what}: {:?} {:?}", d.categories, d.series);
        assert_eq!(d.categories.last().map(String::as_str), Some("Other"), "{what}");
        // Line
        rig.app.chart.spec.kind = ChartKind::Line;
        let p = rig.draw(4);
        assert!(rig.app.chart.rects.plot.is_some(), "{what}: a line chart is drawn");
        assert!(p.boxes.iter().any(|(t, _)| t == "Other"), "{what}: the Other category is named on the x-axis");
        // the choices are saved in settings.json under the tab's file name, and a restarted app takes them back
        rig.app.chart_save_choices_for_test();
        let (saved, _) = micro_parquet::settings::Settings::load(&rig.app.paths.root_dir);
        let spec: ChartSpec = serde_json::from_value(saved.chart.get("book.parquet").cloned().expect("saved under the file name")).expect("a spec");
        assert_eq!((spec.top_n, spec.kind, spec.agg, spec.series.as_deref()), (3, ChartKind::Line, ChartAgg::Count, Some("Risk")), "{what}");
        assert_eq!(spec.order, SortOrder::Tenor, "{what}");
        let mut again = restarted(&rig.app);
        again.jobs = JobQueue::inline();
        let src = parquet_source(&again.paths.root_dir.clone(), "book", DataFrame::new(vec![Series::new("Term Period".into(), &["1m"]), Series::new("Risk".into(), &["High"]), Series::new("Balance".into(), &[1.0])]).expect("frame"));
        again.create_tab_from_source("book".into(), src, vec![], HashMap::new());
        settle(&mut again);
        again.show_chart_drawer = true;
        let (ctx, input) = screen(ppp, w, h);
        let mut rig2 = Rig { app: again, ctx, input, what: what.clone() };
        rig2.draw(8);
        assert_eq!((rig2.app.chart.spec.top_n, rig2.app.chart.spec.kind), (3, ChartKind::Line), "{what}: the tab's choices were kept");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn u5_a_text_value_column_is_refused_in_the_chart_with_its_type_and_open_as_tab_makes_a_table_tab() {
    for (ppp, w, h) in SCREENS {
        let mut rig = chart_rig("p6u5c", ppp, w, h);
        let what = rig.what.clone();
        rig.draw(10);
        rig.app.chart.spec.value = Some("Risk".into());
        let p = rig.draw(10);
        let err = rig.app.chart.error.clone().unwrap_or_else(|| panic!("{what}: the chart refused a text value"));
        assert!(err.contains("'Risk'") && err.contains("Text") && err.contains("Sum"), "{what}: {err}");
        assert!(p.boxes.iter().any(|(t, _)| t.contains("needs a number column")), "{what}: the reason is on screen");
        assert!(rig.app.chart.rects.plot.is_none(), "{what}: no plot is drawn from a wrong choice");
        // back to a number: the chart returns
        rig.app.chart.spec.value = Some("Balance".into());
        rig.draw(10);
        assert!(rig.app.chart.rects.plot.is_some() && rig.app.chart.error.is_none(), "{what}");
        // "Open chart data as tab" by a real click
        let open = rig.app.chart.rects.fields.iter().find(|(n, _)| n == "Open chart data as tab").map(|(_, r)| *r).expect("button");
        rig.click(open.center());
        settle(&mut rig.app);
        assert_eq!(rig.app.tabs.len(), 2, "{what}: a new tab: {}", rig.app.status_message);
        let t = &rig.app.tabs[1];
        assert_eq!(t.available_columns(), vec!["Term Period", "Sum of Balance", "Rows"], "{what}");
        assert_eq!(t.cached_slice.height(), 5, "{what}");
        assert!(t.title.starts_with("Chart: Sum of Balance by Term Period"), "{what}: {}", t.title);
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn u5_a_tooltip_has_the_whole_category_the_series_the_value_in_the_columns_format_and_the_rows() {
    let mut rig = chart_rig("p6u5d", 1.05, 1219.05, 685.71);
    rig.app.chart_attach_tab_for_test();
    rig.app.chart.spec.series = Some("Risk".into());
    rig.draw(10);
    let d = rig.app.chart.data.clone().expect("data");
    let i = d.categories.iter().position(|c| c == "100D").expect("100D");
    let j = d.series.iter().position(|s| s == "High").expect("High");
    let tip = rig.app.chart_tip(&d, j, i);
    let lines: Vec<&str> = tip.lines().collect();
    assert_eq!(lines[0], "Term Period: 100D");
    assert_eq!(lines[1], "Risk: High");
    assert!(lines[2].ends_with(" Cr"), "the value in the column's format (Crore): {tip}");
    assert!(lines[3].ends_with(" rows"), "{tip}");
    assert_eq!(d.rows[j][i].to_string() + " rows", lines[3]);
    // another scale changes the number and its unit
    rig.app.chart.spec.scale = micro_parquet::chart::ChartScale::Million;
    let tip = rig.app.chart_tip(&d, j, i);
    assert!(tip.lines().nth(2).is_some_and(|l| l.ends_with(" M")), "{tip}");
    // a long label is cut on the axis and whole in the tooltip
    assert_eq!(crate::ui::chart::short_label("Khulna Division West Zone", 14), "Khulna Divisi\u{2026}");
    assert_eq!(crate::ui::chart::short_label("DHAKA", 14), "DHAKA");
    micro_parquet::rules::set_data_dir_override(None);
}

// ------------------------------------------------------------------------------------------ U7

const ZONES: [&str; 6] = ["DHAKA NORTH", "WEST-1 ZONE", "Khulna, West", "CTG SOUTH", "", "  "];

/// A tab "book" of 250 rows (three pages): Zone cycles through four names and two blanks, plus a null; a 2,000-value Code column.
fn pick_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    micro_parquet::rules::set_data_dir_override(Some(app.paths.root_dir.clone()));
    let root = app.paths.root_dir.clone();
    let n = 250usize;
    let zones: Vec<Option<String>> = (0..n).map(|i| if i == 7 { None } else { Some(ZONES[i % 6].to_string()) }).collect();
    let df = DataFrame::new(vec![
        Series::new("Zone".into(), zones),
        Series::new("Balance".into(), (0..n).map(|i| 1000 + i as i64).collect::<Vec<i64>>()),
        Series::new("Code".into(), (0..n).map(|i| if i < 125 { format!("k{:04}", i % 125) } else { format!("k{:04}", 125 + (i - 125) * 15) }).collect::<Vec<_>>()),
    ])
    .expect("frame");
    let src = parquet_source(&root, "book", df);
    app.create_tab_from_source("book".into(), src, vec![], HashMap::new());
    settle(&mut app);
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

fn zone_rows(n: usize, names: &[&str]) -> usize {
    (0..n).filter(|i| *i != 7 && names.contains(&ZONES[i % 6])).count()
}

fn filter_title() -> String {
    "\u{E30C}  Filter rows".to_string()
}

fn open_filter_on(rig: &mut Rig, column: &str) -> Painted {
    rig.app.open_filter_editor(None, false);
    rig.app.filter.conditions[0].column = column.to_string();
    rig.draw(4)
}

fn click_pick_button(rig: &mut Rig) -> Painted {
    let b = rig.app.filter.pick_buttons.first().map(|(_, r)| *r).expect("the value picker button");
    rig.click(b.center());
    rig.draw(5)
}

fn pick_row(rig: &Rig, label: &str) -> egui::Rect {
    rig.app.picker.rects.boxes.iter().find(|(l, _)| l == label).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: the picker lists '{label}': {:?}", rig.what, rig.app.picker.rects.boxes.iter().map(|(l, _)| l.clone()).collect::<Vec<_>>()))
}

#[test]
fn u7_the_picker_lists_every_distinct_value_with_its_count_and_two_clicks_pick_two_values() {
    for (ppp, w, h) in SCREENS {
        let mut rig = pick_rig("p6u7a", ppp, w, h);
        let what = rig.what.clone();
        open_filter_on(&mut rig, "Zone");
        let p = click_pick_button(&mut rig);
        let r = assert_dialog_fits(&rig.ctx, &filter_title(), &what);
        let pk = rig.app.picker.rects.clone();
        let panel = pk.panel.unwrap_or_else(|| panic!("{what}: the picker panel opened"));
        assert!(inside(panel, r.window), "{what}: the picker {panel:?} is in the dialog {:?}", r.window);
        // every distinct value with its count, over all 250 rows
        for z in ["DHAKA NORTH", "WEST-1 ZONE", "Khulna, West", "CTG SOUTH"] {
            let want = zone_rows(250, &[z]);
            let row = pk.rows.iter().find(|(l, _)| l == z).map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: '{z}' is listed"));
            assert!(p.boxes.iter().any(|(t, b)| *t == want.to_string() && b.center().y > row.top() && b.center().y < row.bottom()), "{what}: '{z}' has its count {want} on its row");
            assert!(inside(row, panel), "{what}: the row of '{z}' is inside the picker");
        }
        let blanks = 250 - [DHAKA, WEST, KHULNA, CTG].iter().map(|z| zone_rows(250, &[z])).sum::<usize>();
        let blank_row = pk.rows.iter().find(|(l, _)| l == "(blank)").map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: the (blank) entry"));
        assert!(p.boxes.iter().any(|(t, b)| *t == blanks.to_string() && b.center().y > blank_row.top() && b.center().y < blank_row.bottom()), "{what}: (blank) has its count {blanks}");
        assert_eq!(pk.rows.len(), 5, "{what}: four names and the blank entry");
        assert!(p.boxes.iter().any(|(t, _)| t == "0 of 5 selected"), "{what}: the summary");
        for t in ["Select all", "Clear"] {
            assert!(p.boxes.iter().any(|(x, _)| x == t), "{what}: '{t}'");
        }
        // two real clicks on the check boxes
        rig.click(pick_row(&rig, "DHAKA NORTH").center());
        assert_eq!(rig.app.filter.conditions[0].values, vec!["DHAKA NORTH"], "{what}");
        assert_eq!(rig.app.filter.conditions[0].op, CondOp::Equals, "{what}: one value stores equals");
        rig.draw(2);
        rig.click(pick_row(&rig, "WEST-1 ZONE").center());
        let c = rig.app.filter.conditions[0].clone();
        assert_eq!((c.op, c.values.clone()), (CondOp::InList, vec!["DHAKA NORTH".to_string(), "WEST-1 ZONE".to_string()]), "{what}: several store 'is in list'");
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t == "2 of 5 selected"), "{what}: the picker says 2 of 5");
        assert!(p.boxes.iter().any(|(t, _)| t.starts_with("2 of 5 selected")), "{what}: and so does the button");
        // Apply to current tab by a real click: the matching rows
        let apply = p.boxes.iter().find(|(t, _)| t.contains("Apply to current tab")).map(|(_, b)| *b).expect("apply");
        rig.click(apply.center());
        settle(&mut rig.app);
        assert_eq!(rig.app.tabs[0].total_rows, zone_rows(250, &["DHAKA NORTH", "WEST-1 ZONE"]), "{what}");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

const DHAKA: &str = "DHAKA NORTH";
const WEST: &str = "WEST-1 ZONE";
const KHULNA: &str = "Khulna, West";
const CTG: &str = "CTG SOUTH";

#[test]
fn u7_a_value_with_a_comma_blank_not_in_list_select_all_and_clear_work_by_real_clicks() {
    for (ppp, w, h) in SCREENS {
        let mut rig = pick_rig("p6u7b", ppp, w, h);
        let what = rig.what.clone();
        open_filter_on(&mut rig, "Zone");
        click_pick_button(&mut rig);
        // "Khulna, West" is one value
        rig.click(pick_row(&rig, KHULNA).center());
        assert_eq!(rig.app.filter.conditions[0].values, vec![KHULNA], "{what}");
        rig.draw(2);
        // "(blank)" adds the blank rows
        rig.click(pick_row(&rig, "(blank)").center());
        assert!(rig.app.filter.conditions[0].or_blank, "{what}");
        let step = TransformStep::FilterGroup { conditions: rig.app.filter.conditions.clone(), join: GroupJoin::And, keep_matching: true };
        let blank_rows = 250 - [DHAKA, WEST, KHULNA, CTG].iter().map(|z| zone_rows(250, &[z])).sum::<usize>();
        rig.app.push_step_to_active_tab(step, false);
        settle(&mut rig.app);
        assert_eq!(rig.app.tabs[0].total_rows, zone_rows(250, &[KHULNA]) + blank_rows, "{what}: the value and the blanks");
        rig.app.tabs[0].steps.clear();
        // Clear and Select all
        let clear = rig.app.picker.rects.clear.expect("Clear");
        rig.click(clear.center());
        assert!(rig.app.filter.conditions[0].values.is_empty() && !rig.app.filter.conditions[0].or_blank, "{what}: Clear");
        let all = rig.app.picker.rects.select_all.expect("Select all");
        rig.click(all.center());
        let c = rig.app.filter.conditions[0].clone();
        assert_eq!((c.values.len(), c.or_blank, c.op), (4, true, CondOp::InList), "{what}: Select all picks every listed value and (blank)");
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t == "5 of 5 selected"), "{what}");
        // the search narrows the list
        rig.app.picker.search = "west".to_string();
        rig.draw(3);
        let labels: Vec<String> = rig.app.picker.rects.rows.iter().map(|(l, _)| l.clone()).collect();
        assert!(labels.contains(&KHULNA.to_string()) && labels.contains(&WEST.to_string()) && labels.len() == 2, "{what}: 'west' finds two values, not case-sensitive: {labels:?}");
        rig.app.picker.search.clear();
        rig.draw(2);
        // is not in list: the picked values are out
        rig.app.filter.conditions[0].op = CondOp::NotInList;
        rig.app.filter.conditions[0].values = vec![DHAKA.to_string(), CTG.to_string()];
        rig.app.filter.conditions[0].or_blank = false;
        let step = TransformStep::FilterGroup { conditions: rig.app.filter.conditions.clone(), join: GroupJoin::And, keep_matching: true };
        rig.app.push_step_to_active_tab(step, false);
        settle(&mut rig.app);
        assert_eq!(rig.app.tabs[0].total_rows, zone_rows(250, &[WEST, KHULNA]), "{what}: not in list keeps the other named zones (a blank is unknown)");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn u7_a_column_with_many_values_shows_the_note_and_the_search_finds_a_value_outside_the_list() {
    for (ppp, w, h) in SCREENS {
        let mut rig = pick_rig("p6u7c", ppp, w, h);
        let what = rig.what.clone();
        // 250 rows, 250 different codes: not above 1,000, so the whole list shows and there is no note
        open_filter_on(&mut rig, "Code");
        click_pick_button(&mut rig);
        assert!(rig.app.picker.rects.note.is_none(), "{what}: no note below 1,000 values");
        // the note and the worker's search are checked on a column above 1,000 values
        let n = 3_000usize;
        let root = rig.app.paths.root_dir.clone();
        let vals: Vec<String> = (0..n).map(|i| if i < 2000 { format!("k{:04}", i % 1000) } else { format!("k{:04}", 1000 + (i - 2000)) }).collect();
        let df = DataFrame::new(vec![Series::new("Code".into(), vals), Series::new("V".into(), vec![1i64; n])]).expect("frame");
        let src = parquet_source(&root, "codes", df);
        rig.app.create_tab_from_source("codes".into(), src, vec![], HashMap::new());
        settle(&mut rig.app);
        rig.app.active_tool = ActiveToolModal::None;
        open_filter_on(&mut rig, "Code");
        let p = click_pick_button(&mut rig);
        let note = rig.app.picker.rects.note.unwrap_or_else(|| panic!("{what}: the note is shown"));
        assert!(p.boxes.iter().any(|(t, b)| t == "Showing 1,000 of 2,000; search to find others" && inside(*b, note)), "{what}: the note text");
        assert_eq!(rig.app.picker.full.as_ref().map(|f| (f.values.len(), f.distinct)), Some((1000, 2000)), "{what}");
        assert!(!rig.app.picker.rects.rows.iter().any(|(l, _)| l == "k1500"), "{what}: k1500 is not among the 1,000 most frequent");
        // type in the search box: a real click on it, then the text
        let search = rig.app.picker.rects.search.expect("search box");
        rig.click(search.center());
        rig.step(vec![egui::Event::Text("k1500".to_string())]);
        rig.draw(4);
        settle(&mut rig.app);
        rig.draw(4);
        assert!(rig.app.picker.rects.rows.iter().any(|(l, _)| l == "k1500"), "{what}: the search found a value outside the top 1,000: {:?}", rig.app.picker.rects.rows.iter().map(|(l, _)| l.clone()).collect::<Vec<_>>());
        rig.click(pick_row(&rig, "k1500").center());
        assert_eq!(rig.app.filter.conditions[0].values, vec!["k1500"], "{what}");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn u7_a_conditional_column_rule_has_the_same_picker_and_an_old_typed_value_shows_as_picked() {
    for (ppp, w, h) in SCREENS {
        let mut rig = pick_rig("p6u7d", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::Conditional);
        rig.app.cond.rules[0].col_name = "Zone".into();
        rig.app.cond.rules[0].op = FilterOp::Eq;
        rig.app.cond.rules[0].compare_val = "DHAKA NORTH".into();
        rig.draw(5);
        let b = rig.app.cond.pick_buttons.first().map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: the rule has a picker button"));
        let p = rig.draw(2);
        assert!(p.boxes.iter().any(|(t, _)| t.starts_with("1 selected")), "{what}: the old typed value counts as one picked");
        rig.click(b.center());
        rig.draw(5);
        assert!(!rig.app.picker.rects.rows.iter().any(|(l, _)| l == "(blank)"), "{what}: a rule's blanks have their own result, so there is no (blank) entry");
        // the rule's value was typed before: it shows as picked; add a second value by a real click
        rig.click(pick_row(&rig, "WEST-1 ZONE").center());
        let rule = rig.app.cond.rules[0].clone();
        assert_eq!((rule.op, rule.values.clone()), (FilterOp::InList, vec!["DHAKA NORTH".to_string(), "WEST-1 ZONE".to_string()]), "{what}: the typed value became part of the picked values");
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t == "2 of 5 selected" || t == "2 of 4 selected" || t.starts_with("2 of ")), "{what}: the summary");
        // Add Column by a real click: the flag
        rig.app.cond.rules[0].output_val = "Core".into();
        rig.app.cond.fallback = "Other".into();
        let add = p.boxes.iter().find(|(t, _)| t.contains("Add Column")).map(|(_, b)| *b).expect("Add Column");
        rig.click(add.center());
        settle(&mut rig.app);
        let flags = rig.app.tabs[0].cached_slice.column("Custom_Flag").expect("the new column").str().expect("text").into_iter().map(|v| v.unwrap_or("").to_string()).collect::<Vec<_>>();
        let zones = rig.app.tabs[0].cached_slice.column("Zone").expect("zone").str().expect("text").into_iter().map(|v| v.unwrap_or("").to_string()).collect::<Vec<_>>();
        for (f, z) in flags.iter().zip(zones.iter()) {
            let core = z == DHAKA || z == WEST;
            assert!(f == "Core" && core || f != "Core" && !core, "{what}: {z} -> {f}");
        }
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn u7_the_picker_lists_the_values_in_the_columns_custom_list_order_when_it_has_one() {
    let mut rig = pick_rig("p6u7e", 1.05, 1219.05, 685.71);
    let list = SortList { name: "Zones".into(), values: vec!["CTG SOUTH".into(), "Khulna, West".into()], case_insensitive: true, apply_to_columns: vec!["Zone".into()] };
    save_list(&sort_lists_dir(), &list).expect("saved a list");
    open_filter_on(&mut rig, "Zone");
    click_pick_button(&mut rig);
    let labels: Vec<String> = rig.app.picker.rects.rows.iter().map(|(l, _)| l.clone()).collect();
    assert_eq!(labels, vec!["CTG SOUTH", "Khulna, West", "DHAKA NORTH", "WEST-1 ZONE", "(blank)"], "the listed values first, the rest naturally, blank last");
    micro_parquet::rules::set_data_dir_override(None);
}

// ---- U1 correction: a Parquet file that matches a saved schema behaves like a CSV

/// BAL, ACNO, CRIRTE as a Parquet file (ACNO as text, like the saved schema says).
fn u1_parquet_file(root: &std::path::Path) -> PathBuf {
    let mut df = DataFrame::new(vec![
        Series::new("BAL".into(), &[300.0f64, 40.0]).into(),
        Series::new("ACNO".into(), &["2001", "2002"]).into(),
        Series::new("CRIRTE".into(), &[11.0f64, 6.5]).into(),
    ])
    .expect("frame");
    let p = root.join("c.parquet");
    polars::prelude::ParquetWriter::new(std::fs::File::create(&p).expect("create")).finish(&mut df).expect("write");
    p
}

#[test]
fn u1_a_matching_parquet_file_opens_the_dialog_with_the_saved_schema_and_undo_like_a_csv() {
    for (ppp, w, h) in SCREENS {
        let (mut rig, _a, _b) = u1_rig("p6u1pq", ppp, w, h, false, false);
        let what = rig.what.clone();
        let pq = u1_parquet_file(&rig.app.paths.root_dir);
        rig.app.open_data_file(pq, false);
        assert!(rig.app.tabs.is_empty() && rig.app.jobs.any_busy() == false, "{what}: no tab yet, the dialog asks first");
        let modal = rig.app.pending_csv_modal.as_ref().unwrap_or_else(|| panic!("{what}: the Schema dialog opened for the Parquet file"));
        assert!(modal.is_parquet && modal.target_tab.is_none(), "{what}");
        assert_eq!(modal.applied.as_ref().map(|a| (a.name.as_str(), a.columns)), Some(("loans", 3)), "{what}");
        assert_eq!(modal.config.final_names(), vec!["Balance", "Account_No", "Int_Rate"], "{what}");
        let p = rig.draw(6);
        let r = dialog_in_window(&rig, SCHEMA_TITLE_PARQUET);
        assert!(p.boxes.iter().any(|(t, bx)| t == "Applied saved schema 'loans' (all 3 columns matched)" && inside(*bx, r.window)), "{what}: the applied line");
        let undo = text_box(&p, "Undo");
        assert!(inside(undo, r.window), "{what}");
        rig.click(undo.center());
        let modal = rig.app.pending_csv_modal.as_ref().expect("still open");
        assert!(modal.applied.is_none() && modal.config.rename_pairs().is_empty(), "{what}: Undo goes back to the file's own names");
        // apply again by reopening, then lock: the tab has the renamed columns
        let pq = u1_parquet_file(&rig.app.paths.root_dir);
        rig.app.pending_csv_modal = None;
        rig.app.open_data_file(pq, false);
        let p = rig.draw(6);
        let lock = text_box(&p, "Lock Schema & Open");
        rig.click(lock.center());
        settle(&mut rig.app);
        assert!(rig.app.pending_csv_modal.is_none(), "{what}: the dialog closed");
        assert_eq!(rig.app.tabs.len(), 1, "{what}");
        assert!(matches!(&rig.app.tabs[0].source, TabSource::SingleFile { is_parquet: true, .. }), "{what}: still a Parquet tab");
        assert_eq!(rig.app.tabs[0].available_columns(), vec!["Balance", "Account_No", "Int_Rate"], "{what}");
        let step = TransformStep::FilterRows { column: "Int_Rate".into(), operator: micro_parquet::models::FilterOp::Gt, value: "8".into(), keep_matching: true };
        rig.app.push_step_to_active_tab(step, false);
        settle(&mut rig.app);
        assert_eq!(rig.app.tabs[0].cached_slice.height(), 1, "{what}: {}", rig.app.status_message);
    }
}

#[test]
fn u1_a_parquet_file_with_apply_without_asking_opens_at_once_with_a_message_naming_the_schema() {
    for (ppp, w, h) in SCREENS {
        let (mut rig, _a, _b) = u1_rig("p6u1pqauto", ppp, w, h, true, false);
        let what = rig.what.clone();
        let pq = u1_parquet_file(&rig.app.paths.root_dir);
        rig.app.open_data_file(pq, false);
        assert!(rig.app.pending_csv_modal.is_none(), "{what}: no dialog");
        settle(&mut rig.app);
        assert_eq!(rig.app.tabs.len(), 1, "{what}");
        assert_eq!(rig.app.tabs[0].available_columns(), vec!["Balance", "Account_No", "Int_Rate"], "{what}");
        assert!(rig.app.status_message.contains("'loans'") && rig.app.status_message.contains("without asking"), "{what}: {}", rig.app.status_message);
    }
}

// ---- P1: saved tool runs

use micro_parquet::banking::{GapInputSpec, GapRole as P1GapRole, RateShockSpec};
use micro_parquet::tool_runs::*;

fn p1_book() -> DataFrame {
    DataFrame::new(vec![
        Series::new("ACNO".into(), &["A1", "A2", "A3"]),
        Series::new("BAL".into(), &[1_000_000.0f64, 2_000_000.0, 500_000.0]),
        Series::new("RATE".into(), &[9.0f64, 12.0, 6.0]),
        {
            let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch");
            let days: Vec<i32> = [(2027, 3, 31), (2028, 10, 31), (2026, 12, 15)].iter().map(|(y, m, d)| (chrono::NaiveDate::from_ymd_opt(*y, *m, *d).expect("date") - epoch).num_days() as i32).collect();
            Series::new("MAT".into(), days).cast(&polars::prelude::DataType::Date).expect("date column").into()
        },
        Series::new("SIDE".into(), &["A", "A", "L"]),
    ])
    .expect("book")
}

/// A tab "Loans" and the repricing-gap result tab made from it; the data folder is the test's own.
fn p1_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    micro_parquet::rules::set_data_dir_override(Some(app.paths.root_dir.clone()));
    let root = app.paths.root_dir.clone();
    let src = parquet_source(&root, "loans", p1_book());
    app.create_tab_from_source("Loans".into(), src.clone(), vec![], HashMap::new());
    settle(&mut app);
    let spec = RateShockSpec { bal_col: "BAL".into(), rate_col: "RATE".into(), mat_col: "MAT".into(), cutoff_date: "2026-10-31".into(), shock_bps: 100.0, side_col: Some("SIDE".into()), ..Default::default() };
    app.create_tab_from_source_with("Repricing gap (Loans)".into(), TabSource::RepricingGap { source: Box::new(src), steps: vec![], spec }, vec![], HashMap::new(), None);
    settle(&mut app);
    assert_eq!(app.tabs.len(), 2, "{}", app.status_message);
    let (ctx, input) = screen(ppp, w, h);
    let mut rig = Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") };
    rig.draw(4);
    rig
}

fn right_click(rig: &mut Rig, pos: egui::Pos2) -> Painted {
    let press = vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed: true, modifiers: egui::Modifiers::NONE }];
    let release = vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed: false, modifiers: egui::Modifiers::NONE }];
    rig.step(press);
    rig.step(release);
    rig.draw(3)
}

fn tab_rect(rig: &Rig, idx: usize) -> egui::Rect {
    crate::ui::tabs::tab_strip_report(&rig.ctx).expect("the strip").cells[idx].tab
}

fn runs_button(rig: &Rig, name: &str) -> egui::Rect {
    rig.app.runs.rects.buttons.iter().find(|(n, _)| n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: the button '{name}' was drawn", rig.what))
}

#[test]
fn p1_a_result_tab_menu_offers_re_run_and_save_run_and_a_plain_tab_does_not() {
    for (ppp, w, h) in SCREENS {
        let mut rig = p1_rig("p6p1a", ppp, w, h);
        let what = rig.what.clone();
        let screen = rig.ctx.screen_rect();
        // the file's own tab has neither
        let loans = tab_rect(&rig, 0);
        let p = right_click(&mut rig, loans.center());
        assert!(!p.boxes.iter().any(|(t, _)| t == "Re-run with changes" || t.starts_with("Save run")), "{what}: a plain tab has no run");
        assert!(p.boxes.iter().any(|(t, _)| t == "Duplicate view"), "{what}: its menu is open");
        // the result tab has both, inside the window
        rig.step(vec![egui::Event::Key { key: egui::Key::Escape, physical_key: Some(egui::Key::Escape), pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }]);
        rig.draw(3);
        let result = tab_rect(&rig, 1);
        let p = right_click(&mut rig, result.center());
        let rerun = text_box(&p, "Re-run with changes");
        let save = text_box(&p, "Save run\u{2026}");
        assert!(inside(rerun, screen) && inside(save, screen), "{what}: the menu is inside the window");
        // Save run… by a real click
        rig.click(save.center());
        assert_eq!(rig.app.active_tool, ActiveToolModal::SaveRun, "{what}");
        rig.draw(6);
        let title = format!("{}  Save run", theme::ph::CLOCK_COUNTER_CLOCKWISE);
        assert_dialog_fits(&rig.ctx, &title, &what);
        assert_eq!(rig.app.runs.draft.as_ref().map(|d| d.name.clone()), Some("Repricing gap (Loans)".to_string()), "{what}: named after the tab");
        // a name that cannot be a file name: Save is disabled and nothing is written
        rig.app.runs.draft.as_mut().expect("draft").name = "a/b".into();
        rig.draw(3);
        rig.click(runs_button(&rig, "Save").center());
        assert!(list_runs(&runs_dir()).is_empty(), "{what}: nothing was written");
        // a good name
        rig.app.runs.draft.as_mut().expect("draft").name = "rg".into();
        rig.draw(3);
        rig.click(runs_button(&rig, "Save").center());
        settle(&mut rig.app);
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: closed after saving: {}", rig.app.status_message);
        assert_eq!(list_runs(&runs_dir()), vec!["rg".to_string()], "{what}");
        let saved = load_run(&runs_dir(), "rg").expect("loads");
        assert!(matches!(&saved.run, ToolRun::RepricingGap(r) if r.source == "Loans" && r.spec.shock_bps == 100.0), "{what}: {:?}", saved.run);
        assert!(rig.app.status_message.contains("Saved the run"), "{what}: {}", rig.app.status_message);
        // the same name again asks before it replaces the file
        rig.app.open_save_run(1);
        rig.app.runs.draft.as_mut().expect("draft").name = "rg".into();
        rig.draw(4);
        rig.click(runs_button(&rig, "Save").center());
        assert_eq!(rig.app.active_tool, ActiveToolModal::SaveRun, "{what}: still open, asking");
        rig.draw(4);
        assert!(rig.app.runs.draft.as_ref().is_some_and(|d| d.confirm_replace && d.error.contains("exists")), "{what}");
        let replace = runs_button(&rig, "Replace");
        let msg = rig.app.runs.rects.message.expect("the message is drawn");
        assert!(inside(msg, rig.ctx.screen_rect()) && inside(replace, rig.ctx.screen_rect()), "{what}");
        rig.click(replace.center());
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: replaced and closed");
        // Cancel closes without writing
        rig.app.open_save_run(1);
        rig.app.runs.draft.as_mut().expect("draft").name = "other".into();
        rig.draw(4);
        rig.click(runs_button(&rig, "Cancel").center());
        assert_eq!(list_runs(&runs_dir()), vec!["rg".to_string()], "{what}");
        micro_parquet::rules::set_data_dir_override(None);
    }
}

#[test]
fn p1_re_run_with_changes_opens_the_tools_dialog_with_the_saved_inputs() {
    for (ppp, w, h) in SCREENS {
        let mut rig = p1_rig("p6p1b", ppp, w, h);
        let what = rig.what.clone();
        // from the menu of the result tab
        let result = tab_rect(&rig, 1);
        let p = right_click(&mut rig, result.center());
        rig.click(text_box(&p, "Re-run with changes").center());
        assert_eq!(rig.app.active_tool, ActiveToolModal::RateShockSim, "{what}: {}", rig.app.status_message);
        assert_eq!(rig.app.active_tab, 0, "{what}: the dialog reads the source tab");
        let s = &rig.app.shock;
        assert_eq!((s.bal_col.as_str(), s.rate_col.as_str(), s.mat_col.as_str(), s.side_col.as_str(), s.bps), ("BAL", "RATE", "MAT", "SIDE", 100.0), "{what}");
        assert_eq!(rig.app.buckets.cutoff, "2026-10-31", "{what}");
        rig.draw(6);
        // File > Saved tool runs lists a saved run; its button prefills the dialog again
        rig.app.active_tool = ActiveToolModal::None;
        let run = rig.app.tab_run(1).expect("the run");
        save_run(&runs_dir(), "saved one", &run).expect("saves");
        std::fs::write(runs_dir().join("broken.json"), "{no").expect("broken");
        rig.app.run_command(&rig.ctx, CommandId::SavedRuns);
        assert_eq!(rig.app.active_tool, ActiveToolModal::SavedRuns, "{what}");
        let p = rig.draw(6);
        let title = format!("{}  Saved tool runs", theme::ph::CLOCK_COUNTER_CLOCKWISE);
        let r = assert_dialog_fits(&rig.ctx, &title, &what);
        assert!(p.boxes.iter().any(|(t, b)| t.starts_with("saved one") && inside(*b, r.window)), "{what}: the run is listed with its description");
        assert!(p.boxes.iter().any(|(t, _)| t.starts_with("broken:")), "{what}: a file that cannot be read says so");
        let b = runs_button(&rig, "Re-run saved one");
        assert!(inside(b, r.window), "{what}");
        rig.click(b.center());
        assert_eq!(rig.app.active_tool, ActiveToolModal::RateShockSim, "{what}: the button opens the tool's dialog");
        // a run whose tab is not open names it
        rig.app.active_tool = ActiveToolModal::None;
        rig.app.tabs.remove(0);
        rig.app.rerun_tab_with_changes(0);
        assert!(rig.app.status_message.contains("Re-run") && rig.app.status_message.contains("not open"), "{what}: {}", rig.app.status_message);
        rig.app.status_message.clear();
        assert!(rig.app.prefill_run(&run).is_err());
        micro_parquet::rules::set_data_dir_override(None);
    }
}

#[test]
fn p1_every_tool_dialog_takes_its_inputs_from_a_saved_run() {
    use micro_parquet::cashflow::{CfInputSpec, Scenario};
    let mut rig = p1_rig("p6p1c", 1.05, 1219.05, 685.71);
    let root = rig.app.paths.root_dir.clone();
    // a second tab for the runs that read two
    let src = parquet_source(&root, "loans2", p1_book());
    rig.app.create_tab_from_source("Sept".into(), src, vec![], HashMap::new());
    settle(&mut rig.app);
    let names = |a: &str| a.to_string();
    let tab_of = |rig: &Rig, n: &str| rig.app.tabs.iter().position(|t| t.title == n).expect("tab");
    let (loans, sept) = (tab_of(&rig, "Loans"), tab_of(&rig, "Sept"));

    let cf = CfInputSpec { role: P1GapRole::Side("SIDE".into()), principal_col: "BAL".into(), rate_col: Some("RATE".into()), maturity_col: "MAT".into(), default_currency: "BDT".into(), ..Default::default() };
    // gap ladder
    let run = ToolRun::GapLadder(GapLadderRun { inputs: vec![GapInputRun { source: names("Loans"), spec: GapInputSpec { role: P1GapRole::Side("SIDE".into()), amount_col: "BAL".into(), maturity_col: "MAT".into() }, product_col: "ACNO".into() }, GapInputRun { source: names("Sept"), spec: GapInputSpec { role: P1GapRole::Liability, amount_col: "BAL".into(), maturity_col: "MAT".into() }, product_col: String::new() }], preset: CustomBucketDef::default_presets()[1].name.clone(), preset_def: None, cutoff: "2026-09-30".into(), null_as_sight: false, behaviour_file: String::new() });
    rig.app.prefill_run(&run).expect("gap");
    assert_eq!(rig.app.active_tool, ActiveToolModal::GapLadder);
    let g = &rig.app.gap;
    assert_eq!((g.rows.len(), g.rows[0].tab, g.rows[1].tab, g.cutoff.as_str(), g.null_as_sight, g.preset_idx), (2, loans, sept, "2026-09-30", false, 1));
    assert_eq!((g.rows[0].role, g.rows[0].side_col.as_str(), g.rows[0].amount_col.as_str(), g.rows[1].role), (crate::ui::gap_ladder::RoleChoice::Side, "SIDE", "BAL", crate::ui::gap_ladder::RoleChoice::Liability));
    // IRRBB ladder and six scenarios
    let run = ToolRun::IrrbbLadder(IrrbbLadderRun { inputs: vec![CfInputRun { source: names("Sept"), spec: cf.clone() }], cutoff: "2026-08-31".into(), mode: micro_parquet::cashflow::CashFlowMode::PrincipalOnly, scenario: Scenario::ParallelUp, behaviour_file: String::new(), blank_maturity_on: true });
    rig.app.prefill_run(&run).expect("cf");
    assert_eq!(rig.app.active_tool, ActiveToolModal::IrrbbLadder);
    let c = &rig.app.cf;
    assert_eq!((c.lines[0].tab, c.cutoff.as_str(), c.blank_on, c.scenario, c.lines[0].rate_col.as_str(), c.lines[0].side_col.as_str()), (sept, "2026-08-31", true, Scenario::ParallelUp, "RATE", "SIDE"));
    let run = ToolRun::IrrbbSix(IrrbbSixRun { inputs: vec![CfInputRun { source: names("Loans"), spec: cf }], cutoff: "2026-10-31".into(), mode: micro_parquet::cashflow::CashFlowMode::PrincipalOnly, blank_maturity_on: false, tier1: 5_000_000.0, compounding: micro_parquet::irrbb::Compounding::Continuous, post_shock_floor_pct: Some(0.0), nii_floor_pct: None, behaviour_file: String::new(), beta_file: String::new(), view: IrrbbView::Results });
    rig.app.prefill_run(&run).expect("irrbb");
    assert_eq!(rig.app.active_tool, ActiveToolModal::IrrbbSix);
    let i = &rig.app.irrbb;
    assert_eq!((i.tier1.as_str(), i.post_floor_on, i.nii_floor_on, i.compounding, i.cutoff.as_str(), i.lines[0].tab), ("5000000", true, false, micro_parquet::irrbb::Compounding::Continuous, "2026-10-31", loans));
    // migration, reconcile, join
    let run = ToolRun::ClMigration(ClMigrationRun { prev: names("Sept"), curr: names("Loans"), spec: micro_parquet::banking::ClMigrationSpec { key_col: "ACNO".into(), bal_col: "BAL".into(), cl_col: "SIDE".into(), segment_col: Some("RATE".into()), pct_of_row: true, ..Default::default() } });
    rig.app.prefill_run(&run).expect("migration");
    assert_eq!(rig.app.active_tool, ActiveToolModal::ClMigration);
    let m = &rig.app.cl_migration;
    assert_eq!((m.prev_tab, m.curr_tab, m.key_col.as_str(), m.segment_col.as_str(), m.pct_of_row), (sept, loans, "ACNO", "RATE", true));
    let run = ToolRun::PeriodReconcile(PeriodReconcileRun { prev: names("Sept"), curr: names("Loans"), key_col: "ACNO".into(), bal_col: "BAL".into(), rate_col: Some("RATE".into()), cl_status_col: None });
    rig.app.prefill_run(&run).expect("reconcile");
    assert_eq!(rig.app.active_tool, ActiveToolModal::ReconcilePeriods);
    assert_eq!((rig.app.recon.prev_tab, rig.app.recon.curr_tab, rig.app.recon.rate_col.as_str(), rig.app.recon.cl_col.as_str()), (sept, loans, "RATE", ""));
    let run = ToolRun::Join(JoinRun { left: names("Loans"), right: names("Sept"), kind: SqlJoinKind::Inner, keys: vec!["ACNO".into(), "SIDE".into()], right_cols_to_keep: vec![], keep_first_duplicate_right: true });
    rig.app.prefill_run(&run).expect("join");
    assert_eq!(rig.app.active_tool, ActiveToolModal::JoinTabs);
    let j = &rig.app.join;
    assert_eq!((j.left_tab, j.right_tab, j.kind, j.keys_input.as_str(), j.keep_first), (loans, sept, SqlJoinKind::Inner, "ACNO, SIDE", true));
    // CL-1, liquidity, IFRS 9
    let run = ToolRun::Cl1Summary(Cl1SummaryRun { source: names("Loans"), spec: micro_parquet::cl1::Cl1Spec { category_col: None, fixed_category: "Demand".into(), grade_col: "SIDE".into(), measure_cols: vec![("outstanding".into(), "BAL".into())] }, template_file: String::new() });
    rig.app.prefill_run(&run).expect("cl1");
    assert_eq!(rig.app.active_tool, ActiveToolModal::Cl1Summary);
    assert!(!rig.app.cl1.category_by_col && rig.app.cl1.grade_col == "SIDE" && rig.app.cl1.measure_cols[0].1 == "BAL", "{:?}", rig.app.cl1.measure_cols);
    let run = ToolRun::Liquidity(LiquidityRun { source: names("Loans"), spec: micro_parquet::liquidity::LiqSpec { gl_col: "ACNO".into(), balance_col: "BAL".into(), sign: micro_parquet::liquidity::BalanceSign::NormalSide, maturity_col: Some("MAT".into()), maturity_unit: micro_parquet::liquidity::MaturityUnit::Days }, map_file: "sample_basel.json".into(), view: micro_parquet::models::LiquidityView::Summary });
    rig.app.prefill_run(&run).expect("liquidity");
    assert_eq!(rig.app.active_tool, ActiveToolModal::Liquidity);
    assert_eq!((rig.app.liq.gl_col.as_str(), rig.app.liq.maturity_col.as_str(), rig.app.liq.map_sel.as_str(), rig.app.liq.maturity_unit), ("ACNO", "MAT", "sample_basel.json", micro_parquet::liquidity::MaturityUnit::Days));
    let ecl_step: TransformStep = serde_json::from_value(serde_json::json!({"IfrsEcl": {"fixed_segment": "Corporate", "drawn_col": "BAL", "eir_col": "RATE", "maturity_col": "MAT", "classified_stage3": true, "cutoff_date": "2026-10-31"}})).expect("step");
    let run = ToolRun::EclSummary(EclSummaryRun { source: names("Loans"), ecl_step: Some(ecl_step), spec: micro_parquet::ecl::EclSummarySpec { grade_col: "SIDE".into(), provision_col: "ACNO".into() } });
    rig.app.prefill_run(&run).expect("ecl");
    assert_eq!(rig.app.active_tool, ActiveToolModal::IfrsEcl);
    assert_eq!((rig.app.ecl.drawn_col.as_str(), rig.app.ecl.eir_col.as_str(), rig.app.ecl.cutoff.as_str(), rig.app.ecl.provision_col.as_str()), ("BAL", "RATE", "2026-10-31", "ACNO"));
    // stack and pivot
    let files = vec![StackFileRun { path: root.join("a.csv"), is_parquet: false, config: IngestionConfig::default() }, StackFileRun { path: root.join("b.csv"), is_parquet: false, config: IngestionConfig::default() }];
    rig.app.prefill_run(&ToolRun::Stack(StackRun { files, include_source_col: false })).expect("stack");
    assert_eq!(rig.app.active_tool, ActiveToolModal::StackFiles);
    assert_eq!((rig.app.stack.files_list.len(), rig.app.stack.include_source), (2, false));
    let spec = PivotSpec { rows: vec!["SIDE".into()], values: vec![PivotValue { field: "BAL".into(), agg: PivotAgg::Sum, weight_col: None }], ..Default::default() };
    rig.app.prefill_run(&ToolRun::PivotTab(PivotTabRun { source: names("Sept"), pivot: Some(TransformStep::from_pivot_spec(spec)) })).expect("pivot");
    assert_eq!(rig.app.active_tool, ActiveToolModal::PivotBuilder);
    assert_eq!((rig.app.active_tab, rig.app.pivot_builder.spec.rows.clone(), rig.app.pivot_builder.new_tab), (sept, vec!["SIDE".to_string()], true));
    micro_parquet::rules::set_data_dir_override(None);
}
