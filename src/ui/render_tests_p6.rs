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
        // SQL is switched off in View ▸ Side panel, and the panel is off
        rig.app.settings.side_panel_hidden = vec!["SQL".to_string()];
        rig.app.show_steps_sidebar = false;
        rig.draw(4);
        // View menu: click "View", then "Side panel", then the SQL box
        let p = rig.draw(2);
        let view = p.boxes.iter().find(|(t, _)| t == "View").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the View menu"));
        rig.click(view.center());
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t.contains("Custom sort lists")), "{what}: View lists Custom sort lists");
        let side = p.boxes.iter().find(|(t, _)| t == "Side panel").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: View lists Side panel"));
        rig.click(side.center());
        let p = rig.draw(3);
        let item = p.boxes.iter().find(|(t, _)| t == "SQL").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: Side panel lists SQL: {:?}", p.boxes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>()));
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
    app.set_side_tab_visible(crate::ui::steps::RightTab::Chart, true);
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

/// `shown` is `all` with some names left out, in the same order.
fn is_subsequence(shown: &[String], all: &[&str]) -> bool {
    let mut it = all.iter();
    shown.iter().all(|s| it.any(|a| a == s))
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
        // (the side panel is narrow: a name is left out where two would touch, never reordered)
        assert!(is_subsequence(&labels, &["1m", "1y", "3m", "6m", "100D"]) && labels.len() >= 3, "{what}: the x-axis names the categories: {:?}", p.boxes.iter().filter(|(_, b)| b.top() >= plot.bottom() - 40.0).map(|(t, _)| t.clone()).collect::<Vec<_>>());
        // the title, with the unit of the column's format (Crore is the default scale)
        let title = p.boxes.iter().find(|(t, _)| t.starts_with("Sum of Balance by Term Period: all")).map(|(t, b)| (t.clone(), *b)).unwrap_or_else(|| panic!("{what}: the title"));
        assert_eq!(title.0, "Sum of Balance by Term Period: all 250 rows (Crore)", "{what}");
        // the controls and the title are inside the window and the central area
        let screen_rect = rig.ctx.screen_rect();
        for (n, f) in &r.fields {
            // (the studio sections of Phase 7 are under the chart, reached by scrolling the panel)
            if f.top() > plot.bottom() {
                continue;
            }
            assert!(inside(*f, screen_rect), "{what}: '{n}' {f:?} is in the window");
            assert!(f.left() >= screen_rect.right() - rig.app.right_panel_w - 1.0, "{what}: '{n}' {f:?} is in the side panel (F2)");
        }
        assert!(title.1.left() >= screen_rect.right() - rig.app.right_panel_w - 1.0, "{what}: the title is in the side panel");
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
        let shown = x_labels(&p, plot, &["1m", "1y", "3m", "6m", "100D"]);
        assert!(is_subsequence(&shown, &["1m", "3m", "100D", "6m", "1y"]) && shown.len() >= 3, "{what}: Tenor order 1m, 3m, 100D, 6m, 1y (names that do not fit are left out): {shown:?}");
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
        assert!(d.categories.iter().any(|c| p.boxes.iter().any(|(t, _)| t == c)), "{what}: category names are on the x-axis (every k-th one in the narrow panel)");
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
        again.set_side_tab_visible(crate::ui::steps::RightTab::Chart, true);
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
        for t in ["Select all listed", "Clear"] {
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
        let all = rig.app.picker.rects.select_all.expect("Select all listed");
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
        assert_eq!(rig.app.tabs[0].total_rows, 250 - zone_rows(250, &[DHAKA, CTG]), "{what}: not in list keeps every other row, the blank ones too (G6)");
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
use micro_parquet::dq::{dq_dir, list_rulesets, load_ruleset};

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
    let run = ToolRun::GapLadder(GapLadderRun { inputs: vec![GapInputRun { source: names("Loans"), spec: GapInputSpec { role: P1GapRole::Side("SIDE".into()), amount_col: "BAL".into(), maturity_col: "MAT".into() }, product_col: "ACNO".into() }, GapInputRun { source: names("Sept"), spec: GapInputSpec { role: P1GapRole::Liability, amount_col: "BAL".into(), maturity_col: "MAT".into() }, product_col: String::new() }], preset: CustomBucketDef::default_presets()[1].name.clone(), preset_def: None, cutoff: "2026-09-30".into(), null_as_sight: false, behaviour_file: String::new(), stress_tdrr: false });
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

// ---- P2: report packs

fn packs_title() -> String {
    format!("{}  Report packs", theme::ph::TABLE)
}

fn pack_btn(rig: &Rig, name: &str) -> egui::Rect {
    rig.app.packs.rects.buttons.iter().find(|(n, _)| n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: the button '{name}' was drawn", rig.what))
}

fn pack_field(rig: &Rig, name: &str) -> egui::Rect {
    rig.app.packs.rects.fields.iter().find(|(n, _)| n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: the field '{name}' was drawn", rig.what))
}

/// Loans and Deposits opened from CSV files, a gap ladder made from them, and a data folder of the test's own.
fn packs_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    micro_parquet::rules::set_data_dir_override(Some(app.paths.root_dir.clone()));
    let root = app.paths.root_dir.clone();
    let loans = root.join("loans_oct.csv");
    let deposits = root.join("deposits_oct.csv");
    std::fs::write(&loans, "ACNO,BAL,MAT\nL1,1000000,2027-03-31\nL2,2000000,2028-10-31\n").expect("loans");
    std::fs::write(&deposits, "ACNO,BAL,MAT,PROD\nD1,3000000,2026-12-31,SB\nD2,1500000,2027-06-30,TD\n").expect("deposits");
    for (title, p) in [("Loans", &loans), ("Deposits", &deposits)] {
        let cfg = micro_parquet::engine::auto_detect_smart_schema(p).expect("schema");
        app.create_tab_from_source(title.into(), TabSource::SingleFile { path: p.clone(), is_parquet: false, config: cfg }, vec![], HashMap::new());
        settle(&mut app);
    }
    let src_of = |app: &MicroParquetApp, t: usize| app.tabs[t].source.clone();
    let gi = |src: TabSource, role| micro_parquet::models::GapTabInput { source: src, steps: vec![], spec: micro_parquet::banking::GapInputSpec { role, amount_col: "BAL".into(), maturity_col: "MAT".into() } };
    let source = TabSource::GapLadder {
        inputs: vec![gi(src_of(&app, 0), micro_parquet::banking::GapRole::Asset), gi(src_of(&app, 1), micro_parquet::banking::GapRole::Liability)],
        preset: CustomBucketDef::default_presets()[0].clone(),
        cutoff_date: "2026-10-31".into(),
        null_as_sight: true,
        limits: None,
        behaviour: None,
    };
    app.create_tab_from_source_with("Gap ladder".into(), source, vec![], HashMap::new(), None);
    settle(&mut app);
    assert_eq!(app.tabs.len(), 3, "{}", app.status_message);
    let (ctx, input) = screen(ppp, w, h);
    let mut rig = Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") };
    rig.draw(4);
    rig
}

/// Moves the pointer onto the control first (as a mouse does), then clicks it a second after the last click.
fn pclick_btn(rig: &mut Rig, name: &str) {
    let r = pack_btn(rig, name);
    pclick(rig, r);
}

fn pclick(rig: &mut Rig, r: egui::Rect) {
    rig.step(vec![egui::Event::PointerMoved(r.center())]);
    rig.click_later(r.center());
}

#[test]
fn p2_the_pack_builder_fits_builds_a_pack_by_clicks_saves_it_and_runs_it() {
    use micro_parquet::packs::*;
    for (ppp, w, h) in SCREENS {
        let mut rig = packs_rig("p6p2a", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::Packs);
        assert_eq!(rig.app.active_tool, ActiveToolModal::Packs, "{what}: Banking > Report packs / Ctrl+K opens it");
        rig.draw(6);
        let r = assert_dialog_fits(&rig.ctx, &packs_title(), &what);
        // an item before a slot: refused with the reason
        rig.app.active_tab = 2;
        rig.draw(8);
        pclick_btn(&mut rig, "Add item from result tab");
        assert!(rig.app.packs.pack.items.is_empty() && rig.app.packs.message.as_ref().is_some_and(|m| m.0 && m.1.contains("Add the input slot")), "{what}: {:?}", rig.app.packs.message);
        // slots from the two file tabs
        for tab in [0usize, 1] {
            rig.app.active_tab = tab;
            rig.draw(8);
            let b = pack_btn(&rig, "Add input slot from open tab");
            pclick(&mut rig, b);
        }
        let slots: Vec<String> = rig.app.packs.pack.inputs.iter().map(|i| i.slot.clone()).collect();
        assert_eq!(slots, vec!["Loans", "Deposits"], "{what}: {:?} active {}", rig.app.packs.message, rig.app.active_tab);
        assert_eq!(rig.app.packs.pack.inputs[0].required_columns, vec!["ACNO", "BAL", "MAT"], "{what}: the file's columns are kept");
        // the gap ladder as an item: its sources are the slots
        rig.app.active_tab = 2;
        rig.draw(8);
        pclick_btn(&mut rig, "Add item from result tab");
        assert_eq!(rig.app.packs.pack.items.len(), 1, "{what}: {:?}", rig.app.packs.message);
        assert_eq!(item_slots(&rig.app.packs.pack.items[0]), vec!["Loans", "Deposits"], "{what}");
        // the table of a slot as a second item, then the order changes by the arrows
        pclick_btn(&mut rig, "Add table of slot 2");
        assert_eq!(rig.app.packs.pack.items.len(), 2, "{what}");
        pclick_btn(&mut rig, "Up 2");
        assert_eq!(rig.app.packs.pack.items[0].title, "Deposits table", "{what}: moved up");
        pclick_btn(&mut rig, "Down 1");
        assert_eq!(rig.app.packs.pack.items[1].title, "Deposits table", "{what}: moved down again");
        // the dialog still fits with its rows, and every row and control is inside it
        let p = rig.draw(4);
        let r2 = assert_dialog_fits(&rig.ctx, &packs_title(), &what);
        let _ = (r, p);
        for (n, rect) in rig.app.packs.rects.fields.iter().chain(rig.app.packs.rects.buttons.iter()) {
            if rect.is_positive() && rect.top() < r2.body.bottom() && rect.bottom() > r2.body.top() {
                assert!(rect.left() >= r2.window.left() - 1.0 && rect.right() <= r2.window.right() + 1.0, "{what}: '{n}' {rect:?} is inside the dialog {:?}", r2.window);
            }
        }
        // a slot that an item reads cannot be removed
        pclick_btn(&mut rig, "Remove slot 2");
        assert_eq!(rig.app.packs.pack.inputs.len(), 2, "{what}");
        assert!(rig.app.packs.message.as_ref().is_some_and(|m| m.1.contains("Remove the item first")), "{what}");
        // name, save; saving again under the same loaded name is fine; another pack with that name asks
        rig.app.packs.pack.name = "monthly".into();
        rig.draw(8);
        let sb = pack_btn(&rig, "Save pack");
        pclick(&mut rig, sb);
        assert_eq!(list_packs(&packs_dir()), vec!["monthly".to_string()], "{what}: {:?} button {sb:?} window {:?}", rig.app.packs.message, rig.ctx.screen_rect());
        pclick_btn(&mut rig, "New pack");
        assert!(rig.app.packs.pack.inputs.is_empty(), "{what}");
        rig.app.packs.pack.name = "monthly".into();
        rig.app.packs.pack.items.push(PackItem { title: "x".into(), sheet: "x".into(), source: PackItemKind::Steps { slot: "A".into() }, col_formats: Default::default() });
        rig.app.packs.pack.inputs.push(PackInput { slot: "A".into(), ..Default::default() });
        rig.draw(8);
        pclick_btn(&mut rig, "Save pack");
        assert!(rig.app.packs.confirm_replace && rig.app.packs.message.as_ref().is_some_and(|m| m.1.contains("exists")), "{what}");
        rig.draw(8);
        let _ = pack_btn(&rig, "Replace");
        // open the saved one again
        rig.app.pack_open_for_test("monthly");
        assert_eq!(rig.app.packs.pack.inputs.len(), 2, "{what}: opened from the file");
        // run it: files, cutoff and the workbook typed; Run by a click
        let root = rig.app.paths.root_dir.clone();
        rig.app.packs.files.insert("Loans".into(), root.join("loans_oct.csv").display().to_string());
        rig.app.packs.files.insert("Deposits".into(), root.join("deposits_oct.csv").display().to_string());
        rig.app.packs.cutoff = "2026-10-31".into();
        rig.app.packs.out_path = root.join("out.xlsx").display().to_string();
        rig.draw(8);
        assert!(pack_field(&rig, "File of Loans").width() > 100.0, "{what}");
        pclick_btn(&mut rig, "Run pack");
        settle(&mut rig.app);
        assert!(root.join("out.xlsx").exists(), "{what}: {}", rig.app.status_message);
        assert!(rig.app.status_message.contains("Wrote"), "{what}: {}", rig.app.status_message);
        // a slot without a file stops the run and writes nothing
        std::fs::remove_file(root.join("out.xlsx")).expect("remove");
        rig.app.packs.files.insert("Deposits".into(), String::new());
        rig.draw(8);
        pclick_btn(&mut rig, "Run pack");
        settle(&mut rig.app);
        assert!(!root.join("out.xlsx").exists(), "{what}: nothing written");
        assert!(rig.app.status_message.contains("failed") && rig.app.status_message.contains("'Deposits'"), "{what}: {}", rig.app.status_message);
        micro_parquet::rules::set_data_dir_override(None);
    }
}

// ---- P3: month-on-month variance

fn mom_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    let root = app.paths.root_dir.clone();
    let cr = 10_000_000.0f64;
    let month = |rows: &[(&str, f64, f64)]| {
        DataFrame::new(vec![
            Series::new("Product".into(), rows.iter().map(|r| r.0).collect::<Vec<_>>()),
            Series::new("Balance".into(), rows.iter().map(|r| r.1 * cr).collect::<Vec<_>>()),
            Series::new("Rate".into(), rows.iter().map(|r| r.2).collect::<Vec<_>>()),
        ])
        .expect("month")
    };
    for (title, df) in [("Sept", month(&[("TD", 1000.0, 8.0), ("SB", 500.0, 3.0)])), ("Oct", month(&[("TD", 1100.0, 8.5), ("SB", 450.0, 3.0)]))] {
        let src = parquet_source(&root, &title.to_lowercase(), df);
        app.create_tab_from_source(title.into(), src, vec![], HashMap::new());
        settle(&mut app);
    }
    let (ctx, input) = screen(ppp, w, h);
    let mut rig = Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") };
    rig.draw(4);
    rig
}

fn mom_title() -> String {
    format!("{}  MoM variance", theme::ph::CHART_LINE)
}

#[test]
fn p3_the_mom_dialog_fits_builds_the_variance_and_the_commentary_tabs_by_clicks_and_shows_the_text_box() {
    for (ppp, w, h) in SCREENS {
        let mut rig = mom_rig("p6p3a", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::MomVariance);
        assert_eq!(rig.app.active_tool, ActiveToolModal::MomVariance, "{what}: Banking > Treasury & ALM > MoM variance / Ctrl+K opens it");
        rig.draw(8);
        let r = assert_dialog_fits(&rig.ctx, &mom_title(), &what);
        let m = &rig.app.mom;
        assert_eq!((m.prev_tab, m.curr_tab, m.key1.as_str(), m.bal.as_str()), (0, 1, "Product", "Balance"), "{what}: the open tabs and the columns are chosen");
        assert_eq!((m.amount_text.as_str(), m.pct_text.as_str()), ("50", "5"), "{what}: the defaults of the owner's spec");
        for (n, rect) in m.rects.combos.iter().chain(m.rects.fields.iter()).chain(m.rects.buttons.iter()) {
            assert!(inside(*rect, r.window), "{what}: '{n}' {rect:?} is inside the dialog {:?}", r.window);
        }
        // a bad amount disables Build and says why
        rig.app.mom.amount_text = "lots".into();
        rig.draw(6);
        let build = rig.app.mom.rects.buttons.iter().find(|(n, _)| n == "Build MoM variance").map(|(_, r)| *r).expect("button");
        rig.click_later(build.center());
        assert_eq!(rig.app.tabs.len(), 2, "{what}: nothing is built with a bad amount");
        rig.app.mom.amount_text = "50".into();
        rig.draw(6);
        let build = rig.app.mom.rects.buttons.iter().find(|(n, _)| n == "Build MoM variance").map(|(_, r)| *r).expect("button");
        pclick(&mut rig, build);
        settle(&mut rig.app);
        assert_eq!(rig.app.tabs.len(), 4, "{what}: two tabs: {}", rig.app.status_message);
        assert_eq!(rig.app.active_tab, 3, "{what}: the variance table is the active one");
        let t = &rig.app.tabs[3];
        assert_eq!(t.title, "MoM variance");
        assert_eq!(t.total_rows, 2, "{what}");
        assert_eq!(rig.app.tabs[2].title, "MoM commentary");
        // the commentary tab has the text box above its table, and Copy works by a click
        rig.app.active_tab = 2;
        rig.draw(8);
        let (text, shown, total) = rig.app.mom_commentary_text_for_test().expect("a commentary tab");
        assert_eq!((shown, total), (3, 3), "{what}");
        assert!(text.starts_with("TD rose by 100.00 Cr (10.00%) to 1,100.00 Cr; rate up 50 bp to 8.50%.\nSB fell by 50.00 Cr (10.00%) to 450.00 Cr; rate unchanged at 3.00%."), "{what}: {text}");
        let (bx, copy) = (rig.app.mom.rects.text_box.expect("box"), rig.app.mom.rects.copy.expect("copy"));
        assert!(inside(bx, rig.ctx.screen_rect()) && inside(copy, rig.ctx.screen_rect()), "{what}");
        assert!(bx.width() > 300.0, "{what}: the box is wide enough to read: {bx:?}");
        pclick(&mut rig, copy);
        assert_eq!(rig.app.status_message, "Copied the commentary.", "{what}");
        // the variance tab has no box
        rig.app.active_tab = 3;
        rig.draw(4);
        assert!(rig.app.mom_commentary_text_for_test().is_none(), "{what}");
    }
}

// ---- P4: the data-quality scorecard

const DQ_BOOK: &str = "Account_No,Rate,Open_Date,Maturity,Balance\nA01,9.5,2020-01-01,2025-01-01,100\nA02,8.0,2020-01-01,2025-01-01,100\nA03,abc,2020-01-01,2025-01-01,100\nA04,xyz,2020-01-01,2025-01-01,100\nA05,45,2020-01-01,2025-01-01,100\nA06,7.0,2020-01-01,2019-12-31,100\nA07,6.0,2020-01-01,2025-01-01,100\nA07,6.5,2020-01-01,2025-01-01,100\n,5.5,2020-01-01,2025-01-01,100\nA10,4.0,2020-01-01,2025-01-01,100\n";

fn dq_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    micro_parquet::rules::set_data_dir_override(Some(app.paths.root_dir.clone()));
    let p = app.paths.root_dir.join("book.csv");
    std::fs::write(&p, DQ_BOOK).expect("book");
    let mut cfg = micro_parquet::engine::auto_detect_smart_schema(&p).expect("schema");
    for c in cfg.columns.iter_mut() {
        c.dtype = match c.name.as_str() {
            "Account_No" => micro_parquet::models::ColDataType::Utf8,
            "Rate" | "Balance" => micro_parquet::models::ColDataType::Float64,
            _ => micro_parquet::models::ColDataType::DateISO,
        };
    }
    app.create_tab_from_source("Book".into(), TabSource::SingleFile { path: p, is_parquet: false, config: cfg }, vec![], HashMap::new());
    settle(&mut app);
    let (ctx, input) = screen(ppp, w, h);
    let mut rig = Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") };
    rig.draw(4);
    rig
}

fn dq_btn(rig: &Rig, name: &str) -> egui::Rect {
    rig.app.dq.rects.buttons.iter().find(|(n, _)| n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: the button '{name}' was drawn", rig.what))
}


/// Scrolls the dialog's body with the mouse wheel until the button is inside the window, then clicks it.
fn dq_scroll_click(rig: &mut Rig, name: &str) {
    let screen = rig.ctx.screen_rect();
    let title = format!("{}  Data-quality scorecard", theme::ph::SCALES);
    for _ in 0..40 {
        let b = dq_btn(rig, name);
        let body = theme::dialog_report(&rig.ctx, &title).expect("the dialog").body;
        if b.top() > body.top() + 4.0 && b.bottom() < body.bottom() - 4.0 {
            return pclick(rig, b);
        }
        let at = body.center();
        let dy = if b.top() <= body.top() + 4.0 { 60.0 } else { -60.0 };
        // scrolling is smoothed over time: let the clock run
        rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 0.3);
        rig.step(vec![egui::Event::PointerMoved(at), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, dy), modifiers: egui::Modifiers::NONE }]);
        for _ in 0..3 {
            rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 0.1);
            rig.draw(1);
        }
    }
    panic!("{}: could not bring '{name}' into the window", rig.what);
}

#[test]
fn p4_the_scorecard_dialog_fits_runs_the_sample_rules_and_opens_the_tabs_by_clicks() {
    for (ppp, w, h) in SCREENS {
        let mut rig = dq_rig("p6p4a", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::DqScorecard);
        assert_eq!(rig.app.active_tool, ActiveToolModal::DqScorecard, "{what}: Ctrl+K opens it");
        rig.draw(8);
        let title = format!("{}  Data-quality scorecard", theme::ph::SCALES);
        let r = assert_dialog_fits(&rig.ctx, &title, &what);
        assert_eq!(rig.app.dq.set.rules.len(), 5, "{what}: it starts from the sample rule set");
        assert!(rig.app.dq.rects.banner.is_some_and(|b| inside(b, r.window)), "{what}: the sample banner is shown");
        assert_eq!((rig.app.dq.green_text.as_str(), rig.app.dq.amber_text.as_str()), ("98", "95"), "{what}");
        // run: the five percentages and the average of the acceptance book, Red
        let b = dq_btn(&rig, "Run checks");
        pclick(&mut rig, b);
        settle(&mut rig.app);
        rig.draw(8);
        let r_after = assert_dialog_fits(&rig.ctx, &title, &what);
        assert!(r_after.window.height() <= rig.ctx.screen_rect().height(), "{what}: the dialog is not taller than the window");
        let (_, shown) = rig.app.dq.shown.clone().map(|(a, b)| (a, b)).unwrap_or_else(|| panic!("{what}: {:?}", rig.app.dq.message));
        let p: Vec<String> = shown.result.rules.iter().map(|r| format!("{:.2}", r.pass_pct.expect("pct"))).collect();
        assert_eq!(p, vec!["90.00", "77.78", "80.00", "87.50", "90.00"], "{what}");
        assert_eq!((format!("{:.2}", shown.result.overall.expect("overall")), shown.result.rating.as_str()), ("85.06".to_string(), "Red"), "{what}");
        assert_eq!(shown.previews.len(), 4, "{what}: the four rules with failing rows list them (the type rule has examples)");
        // everything of the result is inside the dialog; the failing rows lists have a button each
        let rr = rig.app.dq.rects.result.expect("the result card");
        assert!(rr.left() >= r.window.left() && rr.right() <= r.window.right() + 1.0, "{what}: the result {rr:?} is inside {:?}", r.window);
        // open the scorecard as a tab by a click
        dq_scroll_click(&mut rig, "Open scorecard as tab");
        settle(&mut rig.app);
        assert_eq!(rig.app.tabs.len(), 2, "{what}: {}", rig.app.status_message);
        assert_eq!(rig.app.tabs[1].title, "Data-quality scorecard");
        assert_eq!(rig.app.tabs[1].total_rows, 6, "{what}: five rules and the overall row");
        // ... and the failing rows of the uniqueness rule (rule 2)
        rig.app.active_tab = 0;
        rig.draw(6);
        dq_scroll_click(&mut rig, "Open failing rows 2");
        settle(&mut rig.app);
        assert_eq!(rig.app.tabs.len(), 3, "{what}");
        assert_eq!(rig.app.tabs[2].total_rows, 2, "{what}: the two rows of the duplicate pair");
        // a control total typed in the dialog and added by a click: Fail with the difference
        rig.app.dq.draft.kind = 6;
        rig.app.dq.draft.col = "Balance".into();
        rig.app.dq.draft.gl = "1005".into();
        rig.app.dq.draft.tol = "0.1".into();
        rig.draw(6);
        dq_scroll_click(&mut rig, "Add rule");
        assert_eq!(rig.app.dq.set.rules.len(), 6, "{what}: {:?}", rig.app.dq.message);
        // the checks read the active tab: the book again
        rig.app.active_tab = 0;
        rig.draw(4);
        let b = dq_btn(&rig, "Run checks");
        pclick(&mut rig, b);
        settle(&mut rig.app);
        rig.draw(6);
        let (_, shown) = rig.app.dq.shown.clone().unwrap_or_else(|| panic!("{what}: {:?}", rig.app.dq.message));
        let c = shown.result.rules[5].control.clone().expect("control");
        assert_eq!((c.sum, c.difference, c.pass), (1000.0, -5.0, false), "{what}");
        assert_eq!(format!("{:.2}", shown.result.overall.expect("overall")), "85.06", "{what}: the control stays out of the average");
        // a bad rule is refused with the reason; a saved rule set comes back
        rig.app.dq.draft.kind = 3;
        rig.app.dq.draft.col = "Rate".into();
        rig.app.dq.draft.min = "x".into();
        rig.draw(4);
        dq_scroll_click(&mut rig, "Add rule");
        assert!(rig.app.dq.message.as_ref().is_some_and(|m| m.0 && m.1.contains("minimum")), "{what}: {:?}", rig.app.dq.message);
        rig.app.dq.set.name = "bank".into();
        rig.app.dq.set.sample = false;
        rig.draw(4);
        let b = dq_btn(&rig, "Save rule set");
        pclick(&mut rig, b);
        assert!(list_rulesets(&dq_dir()).contains(&"bank".to_string()), "{what}: {:?}", rig.app.dq.message);
        let bank = load_ruleset(&dq_dir(), "bank").expect("loads");
        assert_eq!(bank.rules.len(), 6, "{what}");
        micro_parquet::rules::set_data_dir_override(None);
    }
}

// ---- P5: Mudaraba profit distribution

fn mud_rig(name: &str, ppp: f32, w: f32, h: f32, extra: bool) -> Rig {
    let mut app = test_app(name);
    micro_parquet::rules::set_data_dir_override(Some(app.paths.root_dir.clone()));
    let root = app.paths.root_dir.clone();
    let mut products = vec!["MSND", "MSB", "MTDR-1Y"];
    let mut avg = vec![200.0, 300.0, 500.0];
    if extra {
        products.push("MFD-X");
        avg.push(40.0);
    }
    let df = DataFrame::new(vec![Series::new("Product".into(), products), Series::new("Avg_Balance".into(), avg)]).expect("deposits");
    let src = parquet_source(&root, "deposits", df);
    app.create_tab_from_source("Deposits".into(), src, vec![], HashMap::new());
    settle(&mut app);
    let (ctx, input) = screen(ppp, w, h);
    let mut rig = Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") };
    rig.draw(4);
    rig
}

fn mud_btn(rig: &Rig, name: &str) -> egui::Rect {
    rig.app.mud.rects.buttons.iter().find(|(n, _)| n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: the button '{name}' was drawn", rig.what))
}

#[test]
fn p5_the_mudaraba_dialog_shows_the_sample_banner_and_parameters_and_builds_the_acceptance_table() {
    for (ppp, w, h) in SCREENS {
        let mut rig = mud_rig("p6p5a", ppp, w, h, false);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::Mudaraba);
        assert_eq!(rig.app.active_tool, ActiveToolModal::Mudaraba, "{what}: Banking > Islamic window / Ctrl+K opens it");
        rig.draw(8);
        let title = format!("{}  Mudaraba profit distribution", theme::ph::SCALES);
        let r = assert_dialog_fits(&rig.ctx, &title, &what);
        let m = &rig.app.mud;
        assert_eq!((m.product_col.as_str(), m.balance_col.as_str()), ("Product", "Avg_Balance"), "{what}");
        assert!(m.rects.banner.is_some_and(|b| inside(b, r.window)), "{what}: the sample banner is shown while the shipped file is in use");
        assert!(m.rects.params.is_some_and(|b| inside(b, r.window)), "{what}");
        // the period and the amounts of the acceptance example; Build is disabled until they are typed
        rig.app.mud.start = "2026-11-01".into();
        rig.app.mud.end = "2026-11-30".into();
        rig.app.mud.other = "240".into();
        rig.draw(6);
        { let b = mud_btn(&rig, "Build distribution"); pclick(&mut rig, b); }
        assert_eq!(rig.app.tabs.len(), 1, "{what}: no income typed, nothing built");
        rig.app.mud.income = "12".into();
        rig.draw(6);
        { let b = mud_btn(&rig, "Build distribution"); pclick(&mut rig, b); }
        settle(&mut rig.app);
        assert_eq!(rig.app.tabs.len(), 2, "{what}: {}", rig.app.status_message);
        let t = &rig.app.tabs[1];
        assert_eq!(t.title, "Mudaraba profit distribution");
        let df = &t.cached_slice;
        let names = micro_parquet::islamic::MUDARABA_COLUMNS;
        assert_eq!(df.get_column_names().iter().map(|n| n.to_string()).collect::<Vec<_>>(), names.to_vec(), "{what}");
        let products: Vec<String> = df.column("Product").expect("c").str().expect("s").into_iter().flatten().map(str::to_string).collect();
        assert!(products.contains(&"Sample parameters".to_string()) && products.contains(&"Parameter file".to_string()), "{what}: {products:?}");
        let profit = df.column("Profit").expect("profit").f64().expect("f64").get(0).expect("first profit");
        assert!((profit - 0.707368).abs() < 5e-7, "{what}: MSND profit {profit}");
        micro_parquet::rules::set_data_dir_override(None);
    }
}

#[test]
fn p5_a_product_without_a_weightage_stops_the_build_and_is_named() {
    let mut rig = mud_rig("p6p5b", 1.05, 1219.05, 685.71, true);
    rig.app.run_command(&rig.ctx, CommandId::Mudaraba);
    rig.app.mud.start = "2026-11-01".into();
    rig.app.mud.end = "2026-11-30".into();
    rig.app.mud.income = "12".into();
    rig.app.mud.other = "240".into();
    rig.draw(8);
    { let b = mud_btn(&rig, "Build distribution"); pclick(&mut rig, b); }
    settle(&mut rig.app);
    assert_eq!(rig.app.tabs.len(), 1, "no tab is made");
    assert!(rig.app.status_message.contains("MFD-X") && rig.app.status_message.contains("no weightage"), "{}", rig.app.status_message);
    micro_parquet::rules::set_data_dir_override(None);
}

// ---- P6: the gap ladder's TDRR stress option

#[test]
fn p6_the_gap_dialog_has_the_stress_option_that_needs_a_profile_and_names_the_tab_stressed() {
    for (ppp, w, h) in SCREENS {
        let mut rig = p1_rig("p6p6a", ppp, w, h);
        let what = rig.what.clone();
        let root = rig.app.paths.root_dir.clone();
        let dir = root.join("behaviour");
        std::fs::create_dir_all(&dir).expect("folder");
        std::fs::write(dir.join("2026-10-31.json"), r#"{"as_of":"2026-10-31","source":"test","sample":false,"products":[{"product":"A1","tdrr_pct":10}]}"#).expect("profile");
        rig.app.active_tab = 0;
        rig.app.run_command(&rig.ctx, CommandId::GapLadder);
        assert_eq!(rig.app.active_tool, ActiveToolModal::GapLadder, "{what}");
        rig.draw(8);
        let title = format!("{}  Liquidity gap ladder", theme::ph::CHART_BAR);
        let r = assert_dialog_fits(&rig.ctx, &title, &what);
        let setting = |rig: &Rig, n: &str| rig.app.gap.rects.settings.iter().find(|(x, _)| x == n).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: the setting '{n}'", rig.what));
        let stress = setting(&rig, "Stress TDRR");
        assert!(inside(stress, r.window), "{what}: the stress option {stress:?} is inside the dialog {:?}", r.window);
        // it is greyed until a profile is applied: a click does nothing
        pclick(&mut rig, stress);
        assert!(!rig.app.gap.stress_tdrr, "{what}: disabled without a behaviour profile");
        // apply the profile by a click on its checkbox, choose the file, then the stress by a click
        let apply = setting(&rig, "Apply behaviour profile");
        pclick(&mut rig, apply);
        assert!(rig.app.gap.apply_behaviour, "{what}");
        rig.app.gap.behaviour_sel = "2026-10-31.json".into();
        rig.app.gap.rows[0].product_col = "ACNO".into();
        rig.app.gap.rows[0].amount_col = "BAL".into();
        rig.app.gap.rows[0].maturity_col = "MAT".into();
        rig.app.gap.cutoff = "2026-10-31".into();
        rig.draw(8);
        let stress = setting(&rig, "Stress TDRR");
        pclick(&mut rig, stress);
        assert!(rig.app.gap.stress_tdrr, "{what}: ticked by a click");
        let (title, source) = rig.app.gap_ladder_source().unwrap_or_else(|e| panic!("{what}: {e}"));
        assert!(title.ends_with("(stressed)"), "{what}: {title}");
        assert!(matches!(&source, TabSource::GapLadder { behaviour: Some(b), .. } if b.stress_tdrr), "{what}");
        // unticked, the name is as before
        rig.app.gap.stress_tdrr = false;
        assert!(!rig.app.gap_ladder_source().expect("source").0.contains("stressed"), "{what}");
        micro_parquet::rules::set_data_dir_override(None);
    }
}

#[test]
fn g7_two_tabs_of_the_same_title_get_unique_titles_and_a_saved_run_finds_its_file_by_path() {
    use micro_parquet::tool_runs::{PivotTabRun, RunEnv};
    let mut app = test_app("p6g7");
    let root = app.paths.root_dir.clone();
    let mut paths = Vec::new();
    for dir in ["a", "b"] {
        std::fs::create_dir_all(root.join(dir)).expect("dir");
        let p = root.join(dir).join("loans.csv");
        std::fs::write(&p, format!("ACNO,BAL\n{dir}1,10\n{dir}2,20\n")).expect("csv");
        let cfg = micro_parquet::engine::auto_detect_smart_schema(&p).expect("schema");
        app.create_tab_from_source("loans.csv".into(), TabSource::SingleFile { path: p.clone(), is_parquet: false, config: cfg }, vec![], HashMap::new());
        settle(&mut app);
        paths.push(p);
    }
    let titles: Vec<String> = app.tabs.iter().map(|t| t.title.clone()).collect();
    assert_eq!(titles, ["loans.csv", "loans.csv (2)"], "the title names the tab");
    // the run reads the second one; its identity is the file of that tab, not the title
    let mut run = ToolRun::PivotTab(PivotTabRun { source: "loans.csv (2)".into(), pivot: None });
    let ids = run.source_ids(&app.run_env());
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0].path, paths[1].display().to_string());
    assert_eq!(ids[0].blake3, micro_parquet::packs::file_hash(&paths[1]).expect("hash"));
    // the tabs are renamed and reordered: the run still finds the file in the other folder
    app.tabs[1].title = "Second".into();
    app.tabs[0].title = "loans.csv (2)".into();
    let warnings = run.resolve_sources(&ids, &app.run_env()).expect("resolves");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(run.source_names(), vec!["Second".to_string()], "found by its path");
    // the file changed since: a warning, not a refusal
    std::fs::write(&paths[1], "ACNO,BAL\nb1,99\n").expect("change");
    let (_, warn) = app.run_env().resolve(&ids[0]).expect("still found");
    assert!(warn.is_some_and(|w| w.contains("has changed since the run was saved")));
    // open twice: refused naming it; not open at all: refused naming it
    let cfg = micro_parquet::engine::auto_detect_smart_schema(&paths[1]).expect("schema");
    app.create_tab_from_source("again".into(), TabSource::SingleFile { path: paths[1].clone(), is_parquet: false, config: cfg }, vec![], HashMap::new());
    settle(&mut app);
    let err = format!("{:#}", app.run_env().resolve(&ids[0]).expect_err("ambiguous"));
    assert!(err.contains("loans.csv (2)") && err.contains("open in 2 tabs"), "{err}");
    app.tabs.truncate(1);
    let err = format!("{:#}", app.run_env().resolve(&ids[0]).expect_err("missing"));
    assert!(err.contains("is not open") && err.contains("loans.csv (2)"), "{err}");
}

/// Scrolls the Mudaraba dialog's body with the wheel until the control is inside the window, then clicks it.
fn mud_scroll_click(rig: &mut Rig, name: &str) {
    let title = format!("{}  Mudaraba profit distribution", theme::ph::SCALES);
    for _ in 0..40 {
        let b = mud_btn(rig, name);
        let body = theme::dialog_report(&rig.ctx, &title).expect("the dialog").body;
        if b.top() > body.top() + 4.0 && b.bottom() < body.bottom() - 4.0 {
            return pclick(rig, b);
        }
        let at = body.center();
        let dy = if b.top() <= body.top() + 4.0 { 60.0 } else { -60.0 };
        rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 0.3);
        rig.step(vec![egui::Event::PointerMoved(at), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, dy), modifiers: egui::Modifiers::NONE }]);
        for _ in 0..3 {
            rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 0.1);
            rig.draw(1);
        }
    }
    panic!("{}: could not bring '{name}' into the window", rig.what);
}

#[test]
fn e1_edit_parameters_saves_a_new_file_dated_today_and_never_overwrites() {
    for (ppp, w, h) in SCREENS {
        let mut rig = mud_rig("p6e1", ppp, w, h, false);
        let what = rig.what.clone();
        let dir = micro_parquet::islamic::islamic_dir();
        rig.app.run_command(&rig.ctx, CommandId::Mudaraba);
        rig.draw(8);
        let sample = dir.join("psr_2026-01-01.json");
        let sample_before = std::fs::read(&sample).expect("the sample was written");
        mud_scroll_click(&mut rig, "Edit parameters\u{2026}");
        rig.draw(6);
        let title = format!("{}  Mudaraba profit distribution", theme::ph::SCALES);
        assert_dialog_fits(&rig.ctx, &title, &what);
        let d = rig.app.mud.edit.clone().unwrap_or_else(|| panic!("{what}: the editor opened"));
        assert_eq!((d.crr.as_str(), d.psr.as_str(), d.weightages.len()), ("4", "70", 5), "{what}: it starts from the file in force");
        // no source typed: refused, nothing written
        mud_scroll_click(&mut rig, "Save new file");
        rig.draw(4);
        let today = chrono::Local::now().date_naive().format("%Y-%m-%d").to_string();
        let new_file = dir.join(format!("psr_{today}.json"));
        assert!(!new_file.exists(), "{what}: refused without a source");
        assert!(rig.app.mud.edit.as_ref().and_then(|d| d.message.clone()).is_some_and(|(bad, m)| bad && m.contains("source")), "{what}: {:?}", rig.app.mud.edit.as_ref().map(|d| d.message.clone()));
        // type new numbers and save
        {
            let e = rig.app.mud.edit.as_mut().expect("editor");
            e.crr = "5".into();
            e.psr = "65".into();
            e.source = "SSC approval 2026-11-19".into();
            e.weightages[0].1 = "0.5".into();
        }
        rig.draw(4);
        mud_scroll_click(&mut rig, "Save new file");
        rig.draw(4);
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&new_file).unwrap_or_else(|e| panic!("{what}: the new file: {e}"))).expect("json");
        assert_eq!((saved["crr_pct"].as_f64(), saved["psr_pct"].as_f64(), saved["sample"].as_bool(), saved["as_of"].as_str()), (Some(5.0), Some(65.0), Some(false), Some(today.as_str())), "{what}");
        assert_eq!(saved["source"].as_str(), Some("SSC approval 2026-11-19"));
        assert_eq!(saved["weightages"][0]["weight"].as_f64(), Some(0.5));
        assert_eq!(std::fs::read(&sample).expect("sample"), sample_before, "{what}: the old file is untouched");
        assert!(rig.app.mud.edit.is_none(), "{what}: the editor closed");
        // a second save on the same day is refused and the file keeps its content
        let before = std::fs::read(&new_file).expect("read");
        mud_scroll_click(&mut rig, "Edit parameters\u{2026}");
        rig.draw(4);
        rig.app.mud.edit.as_mut().expect("editor").source = "again".into();
        rig.draw(2);
        mud_scroll_click(&mut rig, "Save new file");
        rig.draw(4);
        assert!(rig.app.mud.edit.as_ref().and_then(|d| d.message.clone()).is_some_and(|(bad, m)| bad && m.contains("never overwritten")), "{what}: {:?}", rig.app.mud.edit.as_ref().map(|d| d.message.clone()));
        assert_eq!(std::fs::read(&new_file).expect("read"), before, "{what}: never overwritten");
        micro_parquet::rules::set_data_dir_override(None);
    }
}

// ------------------------------------------------------------------------------------------ F2

fn tab_label_boxes(p: &Painted) -> Vec<(String, egui::Rect)> {
    ["Column", "SQL", "Chart"].iter().filter_map(|n| p.boxes.iter().find(|(t, b)| t == n && b.height() < 24.0 && b.top() < 120.0).map(|(t, b)| (t.clone(), *b))).chain(p.boxes.iter().filter(|(t, b)| t.starts_with("Steps \u{b7} ") && b.top() < 120.0).map(|(t, b)| (t.clone(), *b))).collect()
}

#[test]
fn f2_four_tab_labels_fit_one_row_at_the_panels_minimum_width_at_both_scales() {
    for (ppp, w, h) in SCREENS {
        let mut rig = chart_rig("p6f2a", ppp, w, h);
        let what = rig.what.clone();
        // many steps: "Steps · 99" is the widest label
        rig.app.right_tab = RightTab::Steps;
        let p = rig.draw(6);
        assert_eq!(rig.app.right_panel_w, theme::RIGHT_PANEL_W, "{what}: the minimum width");
        let screen = rig.ctx.screen_rect();
        let mut labels = tab_label_boxes(&p);
        assert_eq!(labels.len(), 4, "{what}: all four tab labels are drawn: {labels:?}");
        labels.sort_by(|a, b| a.1.left().partial_cmp(&b.1.left()).unwrap_or(std::cmp::Ordering::Equal));
        let names: Vec<&str> = labels.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(&names[1..], &["Column", "SQL", "Chart"], "{what}");
        assert!(names[0].starts_with("Steps"), "{what}");
        let top = labels[0].1.top();
        for (n, b) in &labels {
            assert!((b.top() - top).abs() < 3.0, "{what}: '{n}' is in the same row: {b:?}");
            assert!(b.left() >= screen.right() - theme::RIGHT_PANEL_W && b.right() <= screen.right(), "{what}: '{n}' {b:?} is inside the panel");
            assert!(b.height() < 24.0, "{what}: '{n}' is on one line, not wrapped or cut: {b:?}");
        }
        for pair in labels.windows(2) {
            assert!(pair[0].1.right() <= pair[1].1.left() + 0.5, "{what}: {:?} and {:?} do not overlap", pair[0], pair[1]);
        }
        // the widest label: a tab of 99 steps
        let step = TransformStep::FilterRows { column: "Term Period".into(), operator: FilterOp::IsNotNull, value: String::new(), keep_matching: true };
        rig.app.tabs[0].steps = vec![step; 99];
        let p = rig.draw(4);
        let b = p.boxes.iter().find(|(t, _)| t == "Steps \u{b7} 99").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the label with 99 steps"));
        assert!(b.height() < 24.0 && b.right() <= screen.right(), "{what}: {b:?}");
    }
}

#[test]
fn f2_view_side_panel_switches_tabs_on_and_off_saves_them_and_hiding_all_hides_the_panel() {
    for (ppp, w, h) in SCREENS {
        let mut rig = chart_rig("p6f2b", ppp, w, h);
        let what = rig.what.clone();
        let open_side_panel_menu = |rig: &mut Rig| -> Painted {
            let p = rig.draw(3);
            let view = p.boxes.iter().find(|(t, _)| t == "View").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: View"));
            pclick(rig, view);
            let p = rig.draw(3);
            let side = p.boxes.iter().find(|(t, _)| t == "Side panel").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: Side panel"));
            pclick(rig, side);
            rig.draw(3)
        };
        // the submenu has the four boxes, all ticked
        let p = open_side_panel_menu(&mut rig);
        for n in ["Steps", "Column", "SQL", "Chart"] {
            assert!(p.boxes.iter().any(|(t, _)| t == n), "{what}: the submenu lists {n}");
        }
        // switch Column off: it leaves the panel and settings.json
        let item = p.boxes.iter().filter(|(t, _)| t == "Column").map(|(_, b)| *b).last().expect("Column box");
        pclick(&mut rig, item);
        let p = rig.draw(4);
        assert_eq!(rig.app.settings.side_panel_hidden, vec!["Column".to_string()], "{what}");
        assert!(!tab_label_boxes(&p).iter().any(|(n, _)| n == "Column"), "{what}: the Column tab is gone");
        assert!(tab_label_boxes(&p).len() == 3, "{what}: three tabs left: {:?}", tab_label_boxes(&p));
        let text = std::fs::read_to_string(micro_parquet::settings::settings_path(&rig.app.paths.root_dir)).expect("settings.json");
        assert!(text.contains("\"Column\""), "{what}: saved: {text}");
        // a restarted app takes it back from the file
        let (again, _) = micro_parquet::settings::Settings::load(&rig.app.paths.root_dir);
        assert_eq!(again.side_panel_hidden, vec!["Column".to_string()], "{what}");
        // hiding all four hides the panel
        for t in [RightTab::Steps, RightTab::Sql, RightTab::Chart] {
            rig.app.set_side_tab_visible(t, false);
        }
        rig.draw(4);
        assert!(!rig.app.side_panel_shown(), "{what}: with every tab off the panel is not drawn");
        let p = rig.draw(2);
        assert!(tab_label_boxes(&p).is_empty(), "{what}");
        // switching one on brings the panel back on that tab
        rig.app.set_side_tab_visible(RightTab::Chart, true);
        let p = rig.draw(4);
        assert!(rig.app.side_panel_shown() && rig.app.right_tab == RightTab::Chart, "{what}");
        assert!(tab_label_boxes(&p).iter().any(|(n, _)| n == "Chart"), "{what}");
        micro_parquet::rules::set_data_dir_override(None);
    }
}

#[test]
fn f2_the_chart_tab_has_a_pop_out_button_only_where_windows_work_and_the_window_has_the_chart() {
    for (ppp, w, h) in SCREENS {
        let mut rig = chart_rig("p6f2c", ppp, w, h);
        let what = rig.what.clone();
        rig.app.right_tab = RightTab::Chart;
        let p = rig.draw(8);
        // headless: no native windows, so no Pop out; the chart is in the tab, the controls inside the panel
        assert!(!p.boxes.iter().any(|(t, _)| t.contains("Pop out")), "{what}: no Pop out without native windows");
        assert!(rig.app.chart.rects.plot.is_some(), "{what}: the chart is in the tab");
        // a native backend: the button is there and a click puts the chart in its own window
        rig.ctx.set_embed_viewports(false);
        let p = rig.draw(4);
        let pop = p.boxes.iter().find(|(t, _)| t.contains("Pop out")).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: Pop out"));
        assert!(pop.left() >= rig.ctx.screen_rect().right() - theme::RIGHT_PANEL_W, "{what}: in the panel");
        pclick(&mut rig, pop);
        // this backend has no second window: the chart is told "embedded" and docks back at once; a real one keeps it out
        assert_eq!(rig.app.chart_pop_requests, 1, "{what}: the click asked for the chart's own window");
        rig.draw(4);
        assert!(!rig.app.chart_popped, "{what}: docked back where there is no second window");
        // while it is out, the tab says so and has Dock back
        rig.app.chart_popped = true;
        rig.ctx.set_embed_viewports(true);
        let p = rig.draw(1);
        assert!(p.boxes.iter().any(|(t, _)| t == "Dock back") && p.boxes.iter().any(|(t, _)| t.contains("own window")), "{what}: the tab says the chart is out and has Dock back");
        // the window's own content: the same chart, drawn in a window of its size, inside it
        rig.app.chart_popped = false;
        let size = theme::CHART_POPOUT_SIZE;
        let (ctx2, input2) = screen(1.0, size[0], size[1]);
        let mut last = None;
        for _ in 0..6 {
            settle(&mut rig.app);
            let out = ctx2.run(input2.clone(), |c| rig.app.chart_popout_ui(c));
            last = Some(crate::ui::layout_tests::collect_painted(&out.shapes));
        }
        let q = last.expect("frames");
        let screen2 = ctx2.screen_rect();
        let plot = rig.app.chart.rects.plot.unwrap_or_else(|| panic!("{what}: the chart in its window"));
        assert!(inside(plot, screen2), "{what}: the plot {plot:?} is inside the window {screen2:?}");
        for (n, f) in &rig.app.chart.rects.fields {
            assert!(inside(*f, screen2), "{what}: '{n}' {f:?} is in the window");
        }
        assert!(q.boxes.iter().any(|(t, _)| t.starts_with("Sum of Balance by Term Period")), "{what}: the title");
    }
}

// ------------------------------------------------------------------------------------------ F3

fn bar_box(p: &Painted, text: &str) -> Option<egui::Rect> {
    let in_bar = |b: &egui::Rect| b.top() < theme::TOP_BAR_H;
    p.boxes.iter().find(|(t, b)| t.trim() == text.trim() && in_bar(b)).or_else(|| p.boxes.iter().find(|(t, b)| matches!(text, "Banking" | "Export") && t.contains(text) && in_bar(b))).map(|(_, b)| *b)
}

fn click_menu(rig: &mut Rig, name: &str) -> Painted {
    let p = rig.draw(3);
    let b = bar_box(&p, name).unwrap_or_else(|| panic!("{}: the menu '{name}' is in the bar", rig.what));
    pclick(rig, b);
    rig.draw(3)
}

#[test]
fn f3_the_menus_are_file_edit_view_banking_export_help_and_edit_has_undo_redo_the_transforms_and_data_checks() {
    for (ppp, w, h) in SCREENS {
        let mut rig = term_rig("p6f3a", ppp, w, h);
        let what = rig.what.clone();
        let p = rig.draw(4);
        // left to right: File, Edit, View, Banking, Charts, Export, Help, then the toolbar buttons
        let xs: Vec<f32> = ["File", "Edit", "View", "Help"].iter().map(|n| bar_box(&p, n).unwrap_or_else(|| panic!("{what}: {n}")).left()).collect();
        let banking = p.boxes.iter().find(|(t, b)| t.contains("Banking") && b.top() < theme::TOP_BAR_H).map(|(_, b)| b.left()).expect("Banking");
        let export = p.boxes.iter().find(|(t, b)| t.contains("Export") && b.top() < theme::TOP_BAR_H).map(|(_, b)| b.left()).expect("Export");
        let charts = p.boxes.iter().find(|(t, b)| t.contains("Charts") && b.top() < theme::TOP_BAR_H).map(|(_, b)| b.left()).expect("Charts");
        assert!(xs[0] < xs[1] && xs[1] < xs[2] && xs[2] < banking && banking < charts && charts < export && export < xs[3], "{what}: the order of the menus: {xs:?} {banking} {charts} {export}");
        assert!(bar_box(&p, "More").is_none(), "{what}: no More menu any more");
        // Edit: Undo, Redo, the transforms, Data checks
        let p = click_menu(&mut rig, "Edit");
        for want in ["Undo", "Redo", "Conditional column", "Counterparty name normaliser", "Clean line breaks", "Data checks"] {
            assert!(p.boxes.iter().any(|(t, _)| t.contains(want)), "{what}: Edit lists '{want}': {:?}", p.boxes.iter().filter(|(_, b)| b.top() > 40.0 && b.top() < 400.0 && b.left() < 400.0).map(|(t, _)| t.clone()).collect::<Vec<_>>());
        }
        assert!(!p.boxes.iter().any(|(t, _)| t.contains("hygiene") || t.contains("dirty-data")), "{what}: the auditor is under Data checks");
        let dc = p.boxes.iter().find(|(t, _)| t.trim() == "Data checks").map(|(_, b)| *b).expect("Data checks");
        pclick(&mut rig, dc);
        let p = rig.draw(3);
        let audit = p.boxes.iter().find(|(t, _)| t.contains("dirty-data auditor")).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: Data checks lists the hygiene audit"));
        assert!(p.boxes.iter().any(|(t, _)| t.contains("Data-quality scorecard")), "{what}: and the scorecard");
        pclick(&mut rig, audit);
        rig.draw(3);
        assert_eq!(rig.app.active_tool, ActiveToolModal::BankingAudit, "{what}: Edit > Data checks > the auditor opens it");
        rig.app.active_tool = ActiveToolModal::None;
        // the scorecard through the same menu
        let _ = click_menu(&mut rig, "Edit");
        let p = rig.draw(3);
        let dc = p.boxes.iter().find(|(t, _)| t.trim() == "Data checks").map(|(_, b)| *b).expect("Data checks");
        pclick(&mut rig, dc);
        let p = rig.draw(3);
        let sc = p.boxes.iter().find(|(t, _)| t.contains("Data-quality scorecard")).map(|(_, b)| *b).expect("scorecard");
        pclick(&mut rig, sc);
        rig.draw(3);
        assert_eq!(rig.app.active_tool, ActiveToolModal::DqScorecard, "{what}: and the scorecard");
        rig.app.active_tool = ActiveToolModal::None;
        // Banking no longer lists the auditor; Ctrl+K still finds both
        let p = click_menu(&mut rig, "Banking");
        assert!(!p.boxes.iter().any(|(t, _)| t.contains("dirty-data auditor")), "{what}: not in Banking");
        let entries: Vec<micro_parquet::commands::Entry> = micro_parquet::commands::COMMANDS.iter().map(|c| micro_parquet::commands::Entry { label: c.title, words: c.words, recent: None }).collect();
        for (q, id) in [("hygiene audit", CommandId::Audit), ("data checks", CommandId::DqScorecard), ("data quality", CommandId::DqScorecard)] {
            let top: Vec<CommandId> = micro_parquet::commands::rank(q, &entries).iter().take(3).map(|i| micro_parquet::commands::COMMANDS[*i].id).collect();
            assert!(top.contains(&id), "{what}: Ctrl+K '{q}' finds {id:?}: {top:?}");
        }
        micro_parquet::rules::set_data_dir_override(None);
    }
}

#[test]
fn f3_view_toolbar_buttons_switch_each_button_off_and_on_and_are_saved() {
    // (a wide window, so that every button is on the bar before one is switched off)
    for (ppp, w, h) in [(1.0, 2400.0, 800.0), (0.8, 2600.0, 900.0)] {
        let mut rig = term_rig("p6f3b", ppp, w, h);
        let what = rig.what.clone();
        let icon = |id: CommandId| crate::ui::commands::command_icon(id).to_string();
        let p = rig.draw(4);
        for id in micro_parquet::commands::toolbar_buttons() {
            assert!(p.boxes.iter().any(|(t, b)| b.top() < theme::TOP_BAR_H && (t.trim() == icon(id) || t.contains(crate::ui::commands::toolbar_label(id)))), "{what}: {id:?} is on the bar");
        }
        let _ = click_menu(&mut rig, "View");
        let p = rig.draw(3);
        let tb = p.boxes.iter().find(|(t, _)| t.trim() == "Toolbar buttons").map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: View lists Toolbar buttons"));
        pclick(&mut rig, tb);
        let p = rig.draw(3);
        for id in micro_parquet::commands::toolbar_buttons() {
            let n = crate::ui::commands::toolbar_label(id);
            assert!(p.boxes.iter().any(|(t, b)| t == n && b.top() > theme::TOP_BAR_H), "{what}: the submenu has a box for '{n}'");
        }
        // untick Pivot and Chart
        for n in ["Pivot", "Chart"] {
            let b = rig.app_view_box(n);
            pclick(&mut rig, b);
            rig.draw(3);
        }
        assert_eq!(rig.app.settings.toolbar_hidden, vec!["Pivot".to_string(), "Chart".to_string()], "{what}");
        rig.app.active_tool = ActiveToolModal::None;
        rig.ctx.request_repaint();
        // close the menu and look at the bar
        rig.step(vec![egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }]);
        let p = rig.draw(4);
        let has_icon = |id: CommandId| p.boxes.iter().any(|(t, b)| b.top() < theme::TOP_BAR_H && (t.trim() == icon(id) || t.trim().ends_with(&format!("  {}", crate::ui::commands::toolbar_label(id))) || t.trim() == crate::ui::commands::toolbar_label(id)));
        assert!(!has_icon(CommandId::Pivot) && !has_icon(CommandId::ToggleChart), "{what}: Pivot and Chart are off the bar");
        assert!(has_icon(CommandId::Filter) && has_icon(CommandId::ToggleSql) && has_icon(CommandId::RateProfile), "{what}: the others stay");
        let (again, _) = micro_parquet::settings::Settings::load(&rig.app.paths.root_dir);
        assert_eq!(again.toolbar_hidden, vec!["Pivot".to_string(), "Chart".to_string()], "{what}: saved in settings.json");
        micro_parquet::rules::set_data_dir_override(None);
    }
}

impl Rig {
    /// The box of a checkbox of the open View submenu (below the bar).
    fn app_view_box(&mut self, name: &str) -> egui::Rect {
        let p = self.draw(2);
        p.boxes.iter().filter(|(t, b)| t == name && b.top() > theme::TOP_BAR_H).map(|(_, b)| *b).last().unwrap_or_else(|| panic!("{}: a box for '{name}'", self.what))
    }
}

#[test]
fn j1_buttons_keep_their_labels_at_both_scales_and_at_1000_px_the_rightmost_fold_into_the_overflow_menu_with_labels() {
    for (ppp, w, h) in [(1.05, 1219.05, 685.71), (0.8, 1600.0, 900.0), (1.0, 1000.0, 686.0)] {
        let mut rig = term_rig("p6j1", ppp, w, h);
        let what = rig.what.clone();
        let p = rig.draw(6);
        let all = micro_parquet::commands::toolbar_buttons();
        let label_of = |id: CommandId| crate::ui::commands::toolbar_label(id);
        let painted = |p: &Painted, id: CommandId| p.boxes.iter().any(|(t, b)| b.top() < theme::TOP_BAR_H && t.trim_start_matches(|c: char| !c.is_alphanumeric()).trim() == label_of(id));
        // the shown buttons are a front part of the list, all labelled
        let shown: Vec<bool> = all.iter().map(|id| painted(&p, *id)).collect();
        let n_shown = shown.iter().filter(|s| **s).count();
        assert!(shown.iter().take(n_shown).all(|s| *s) && n_shown >= 3, "{what}: the buttons shown are the first ones, with their labels: {shown:?}");
        for n in ["File", "Edit", "View", "Help"] {
            assert!(bar_box(&p, n).is_some(), "{what}: menu {n}");
        }
        assert!(!p.boxes.iter().any(|(t, b)| b.top() < theme::TOP_BAR_H && all.iter().any(|id| t.trim() == crate::ui::commands::command_icon(*id))), "{what}: no icon-only button");
        let folded: Vec<CommandId> = all.iter().zip(&shown).filter(|(_, s)| !**s).map(|(id, _)| *id).collect();
        let over = bar_box(&p, "\u{bb}");
        assert_eq!(over.is_some(), !folded.is_empty(), "{what}: » exactly when something is folded");
        if let Some(over) = over {
            // the overflow menu lists the folded buttons by their labels, and a click runs one
            pclick(&mut rig, over);
            let p = rig.draw(3);
            for id in &folded {
                assert!(p.boxes.iter().any(|(t, b)| b.top() > theme::TOP_BAR_H && t.ends_with(label_of(*id))), "{what}: the overflow menu has '{}'", label_of(*id));
            }
            let sql = p.boxes.iter().filter(|(t, b)| b.top() > theme::TOP_BAR_H && t.ends_with("SQL")).map(|(_, b)| *b).last();
            if let Some(item) = sql {
                rig.app.show_steps_sidebar = false;
                pclick(&mut rig, item);
                rig.draw(3);
                assert!(rig.app.show_steps_sidebar && rig.app.right_tab == RightTab::Sql, "{what}: the folded SQL button ran");
            }
        }
        if w < 1100.0 {
            assert!(!folded.is_empty(), "{what}: at 1000 px something is folded");
        }
        micro_parquet::rules::set_data_dir_override(None);
    }
}

#[test]
fn j2_the_zero_fill_box_decides_whether_a_missing_day_stops_the_run_or_is_listed_in_the_footer() {
    for (ppp, w, h) in SCREENS {
        let mut rig = mud_rig("p6j2", ppp, w, h, false);
        let what = rig.what.clone();
        // November: MSND has every day, MSB lacks the 30th
        let (mut p, mut b, mut d) = (Vec::new(), Vec::new(), Vec::new());
        for day in 1..=30 {
            for (k, v) in [("MSND", 200.0), ("MSB", 300.0), ("MTDR-1Y", 500.0)] {
                if k == "MSB" && day == 30 {
                    continue;
                }
                p.push(k.to_string());
                b.push(v);
                d.push(format!("2026-11-{day:02}"));
            }
        }
        let df = DataFrame::new(vec![Series::new("Product".into(), p), Series::new("Avg_Balance".into(), b), Series::new("Day".into(), d)]).expect("daily");
        let src = parquet_source(&rig.app.paths.root_dir.clone(), "daily", df);
        rig.app.create_tab_from_source("Daily".into(), src, vec![], HashMap::new());
        settle(&mut rig.app);
        rig.app.active_tab = 1;
        rig.app.run_command(&rig.ctx, CommandId::Mudaraba);
        rig.draw(6);
        {
            let m = &mut rig.app.mud;
            (m.start, m.end, m.income, m.other, m.daily, m.date_col) = ("2026-11-01".into(), "2026-11-30".into(), "12".into(), "240".into(), true, "Day".into());
        }
        rig.draw(6);
        mud_scroll_click(&mut rig, "Count missing days as zero balance");
        assert!(rig.app.mud.zero_fill, "{what}: the click ticked the box");
        rig.draw(4);
        { let b = mud_btn(&rig, "Build distribution"); pclick(&mut rig, b); }
        settle(&mut rig.app);
        assert_eq!(rig.app.tabs.len(), 3, "{what}: {}", rig.app.status_message);
        let t = &rig.app.tabs[2];
        let products: Vec<String> = t.cached_slice.column("Product").expect("c").str().expect("s").into_iter().flatten().map(str::to_string).collect();
        assert!(products.contains(&"Zero-filled days: MSB".to_string()), "{what}: the footer lists the product: {products:?}");
        // the spec in the tab source stores the choice
        assert!(matches!(&t.source, TabSource::MudarabaPool { spec, .. } if spec.zero_fill_missing_days), "{what}: stored in the step");
        // unticking stores the other choice (the engine test shows that the run then stops naming the product)
        rig.app.active_tab = 1;
        rig.app.run_command(&rig.ctx, CommandId::Mudaraba);
        rig.draw(4);
        mud_scroll_click(&mut rig, "Count missing days as zero balance");
        assert!(!rig.app.mud.zero_fill, "{what}: unticked");
        assert!(rig.app.mudaraba_source().is_ok_and(|(_, src)| matches!(src, TabSource::MudarabaPool { spec, .. } if !spec.zero_fill_missing_days)), "{what}: the source stores the box as off");
        micro_parquet::rules::set_data_dir_override(None);
    }
}
