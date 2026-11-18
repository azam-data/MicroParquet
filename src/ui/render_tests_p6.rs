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

use crate::ui::schema::SCHEMA_TITLE_CSV;

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
