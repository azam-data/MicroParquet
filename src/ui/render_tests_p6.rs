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
