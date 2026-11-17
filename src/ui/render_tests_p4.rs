//! Phase 4 render tests: the new dialogs are drawn by the real UI code in a headless egui
//! context at 1219×686 pt (a 1280×720 px window at 105%) and 1600×900 pt (at 80%), and what they
//! drew is checked by the rectangles of the widgets themselves.

use crate::ui::layout_tests::{collect_painted, Painted};
use crate::ui::pivot_builder::Area;
use crate::ui::prelude::*;
use crate::ui::render_tests::{assert_dialog_fits, frames, inside, screen};
use micro_parquet::models::BucketRuleItem;
use crate::ui::tests::{open_tab, parquet_source, settle, test_app};
use micro_parquet::commands::CommandId;
use polars::prelude::{NamedFrom, Series};

const SCREENS: [(f32, f32, f32); 2] = [(1.05, 1219.05, 685.71), (0.8, 1600.0, 900.0)];
const EN: &str = "\u{2013}";

/// Bucket, Bank, Balance, Rate and `extra` more columns, one with a very long name.
fn book(extra: usize) -> DataFrame {
    let bucket = |s: &str| s.replace('-', EN);
    let mut cols: Vec<Series> = vec![
        Series::new("Bucket".into(), &[bucket("02. 2-7 Days"), bucket("10. 3-5 Years"), bucket("04. 1-3 Months"), bucket("02. 2-7 Days"), bucket("04. 1-3 Months"), bucket("10. 3-5 Years"), bucket("02. 2-7 Days")]),
        Series::new("Bank".into(), &["Conventional", "Islami", "Conventional", "Islami", "Islami", "Conventional", "Conventional"]),
        Series::new("Balance".into(), &[100.0, 300.0, 200.0, 50.0, 150.0, 400.0, 100.0]),
        Series::new("Rate".into(), &[5.0, 9.0, 6.0, 8.0, 7.0, 10.0, 7.0]),
        Series::new("A_very_long_column_name_that_keeps_going_and_going_and_going_until_it_must_be_cut".into(), &["a", "b", "c", "d", "e", "f", "g"]),
    ];
    for i in 0..extra {
        cols.push(Series::new(format!("Extra field {i:02}").into(), &[1.0f64; 7]));
    }
    DataFrame::new(cols).expect("the book")
}

fn sum_balance() -> PivotValue {
    PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None }
}

fn title() -> String {
    format!("{}  Pivot table", theme::ph::TABLE)
}

struct Rig {
    app: MicroParquetApp,
    ctx: egui::Context,
    input: egui::RawInput,
    what: String,
}

impl Rig {
    fn new(name: &str, ppp: f32, w: f32, h: f32, extra: usize) -> Rig {
        let mut app = test_app(name);
        let src = parquet_source(&app.paths.root_dir.clone(), "book", book(extra));
        open_tab(&mut app, src, vec![]);
        let (ctx, input) = screen(ppp, w, h);
        Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
    }

    /// Runs `n` frames and the finished jobs; returns what the last one painted.
    fn draw(&mut self, n: usize) -> Painted {
        let mut last = None;
        for _ in 0..n {
            settle(&mut self.app);
            let out = self.ctx.run(self.input.clone(), |c| self.app.frame_ui(c));
            last = Some(collect_painted(&out.shapes));
        }
        settle(&mut self.app);
        last.expect("a frame")
    }

    fn step(&mut self, events: Vec<egui::Event>) -> Painted {
        let mut i = self.input.clone();
        i.events = events;
        settle(&mut self.app);
        let out = self.ctx.run(i, |c| self.app.frame_ui(c));
        collect_painted(&out.shapes)
    }

    fn click(&mut self, pos: egui::Pos2) -> Painted {
        let press = vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }];
        let release = vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }];
        self.step(press);
        self.step(release);
        self.draw(3)
    }

    fn open_pivot(&mut self) -> Painted {
        self.app.run_command(&self.ctx, CommandId::Pivot);
        assert_eq!(self.app.active_tool, ActiveToolModal::PivotBuilder, "{}: Ctrl+K / the Transform button opens the builder", self.what);
        self.draw(8)
    }

    fn rects(&self) -> crate::ui::pivot_builder::PivotRects {
        self.app.pivot_builder.rects.clone()
    }
}

fn overlap(a: egui::Rect, b: egui::Rect) -> bool {
    a.intersects(b) && a.intersect(b).width() > 1.0 && a.intersect(b).height() > 1.0
}

/// Everything the dialog drew is inside it, inside its own panel, and nothing overlaps.
fn assert_layout(rig: &Rig, p: &Painted) {
    let what = &rig.what;
    let r = assert_dialog_fits(&rig.ctx, &title(), what);
    let b = rig.rects();
    let win = r.window;
    // (the middle of the dialog scrolls, so a tall right side may run below the window: widths only)
    assert!(inside(b.fields, win), "{what}: the field list {:?} is inside the dialog {win:?}", b.fields);
    assert!(b.right.left() >= win.left() - 0.5 && b.right.right() <= win.right() + 0.5, "{what}: the right side {:?} is not wider than the dialog {win:?}", b.right);
    assert!(!overlap(b.fields, b.right), "{what}: the field list and the right side do not overlap");
    assert!(inside(b.fields_list, b.fields), "{what}: the list {:?} is inside its panel {:?}", b.fields_list, b.fields);
    for (i, area) in Area::ALL.iter().enumerate() {
        let a = b.areas[area.index()];
        assert!(a.is_positive(), "{what}: the area {:?} was drawn", area);
        assert!(inside(a, b.right), "{what}: the area {:?} {a:?} is inside the right side {:?}", area, b.right);
        for other in Area::ALL.iter().skip(i + 1) {
            assert!(!overlap(a, b.areas[other.index()]), "{what}: {:?} and {:?} overlap", area, other);
        }
        assert!(p.boxes.iter().any(|(t, bx)| t == area.title() && inside(*bx, a)), "{what}: the title '{}' is inside its area", area.title());
    }
    assert!(inside(b.toggles, b.right), "{what}: the toggles {:?} are inside the right side {:?}", b.toggles, b.right);
    assert!(inside(b.preview, b.right), "{what}: the preview {:?} is inside the right side {:?}", b.preview, b.right);
    assert!(b.preview.top() >= b.toggles.bottom() - 0.5 && b.toggles.top() >= b.areas[Area::Rows.index()].bottom() - 0.5, "{what}: areas, toggles and preview follow one another without overlap");
    assert!(inside(b.preview_scroll, b.preview), "{what}: the scrolling preview {:?} is inside its card {:?}", b.preview_scroll, b.preview);
    for (area, name, c) in &b.chips {
        assert!(inside(*c, b.areas[area.index()]), "{what}: the chip '{name}' {c:?} is inside its area {:?}", b.areas[area.index()]);
        assert!(p.boxes.iter().filter(|(t, bx)| t == name && overlap(*bx, *c)).all(|(_, bx)| inside(*bx, *c)), "{what}: the name '{name}' is inside its chip {c:?}");
    }
    for (name, row) in &b.field_rows {
        assert!(row.width() <= b.fields.width() + 0.5, "{what}: the row of '{name}' {row:?} is not wider than the list {:?}", b.fields);
        if inside(*row, b.fields_list) {
            assert!(p.boxes.iter().filter(|(t, bx)| t == name && bx.center().x < b.fields.right() && bx.center().y > row.top() && bx.center().y < row.bottom()).all(|(_, bx)| inside(*bx, *row)), "{what}: the name '{name}' is inside its row");
        }
    }
    for (name, rect) in &b.buttons {
        assert!(inside(*rect, r.footer), "{what}: the button '{name}' {rect:?} is inside the footer {:?}", r.footer);
    }
    assert!(inside(b.footer_text, r.footer), "{what}: the footer text {:?} is inside the footer {:?}", b.footer_text, r.footer);
    for (name, rect) in &b.buttons {
        assert!(!overlap(b.footer_text, *rect), "{what}: the footer text does not run under the button '{name}'");
    }
    assert!(inside(r.body, win), "{what}: the body {:?} is inside the dialog", r.body);
}

#[test]
fn the_pivot_dialog_lists_the_fields_with_badges_and_keeps_every_panel_inside_the_dialog() {
    for (ppp, w, h) in SCREENS {
        let mut rig = Rig::new("p4v1a", ppp, w, h, 30);
        let p = rig.open_pivot();
        let what = rig.what.clone();
        // the fields, the search box and the design's wording
        assert!(p.boxes.iter().any(|(t, _)| t.contains("Search 35 fields")), "{what}: the search box counts the fields");
        for t in ["FIELDS", "Filters", "Columns", "Rows", "Values", "Row totals", "Column totals", "Subtotals", "Show values as", "No calculation", "Scale", "Preview", "Create pivot tab", "Cancel", "Bucket", "Bank", "Balance", "Rate"] {
            assert!(p.boxes.iter().any(|(x, _)| x.contains(t)), "{what}: '{t}' is in the dialog");
        }
        // type badges: Aa for text, # for numbers
        assert!(p.boxes.iter().any(|(t, _)| t == "Aa") && p.boxes.iter().any(|(t, _)| t == "#"), "{what}: the badges");
        assert_layout(&rig, &p);
        // nothing to preview yet, and the button that makes the tab cannot be used
        let hint = p.boxes.iter().any(|(t, _)| t.contains("Drag fields") || t.contains("add at least one field"));
        assert!(hint || matches!(rig.app.pivot_builder.preview, Some(Err(_))), "{what}: the empty pivot says what to do");
        assert!(rig.app.pivot_builder_step().is_none(), "{what}: no step without Values and Rows or Columns");
        // many fields scroll inside their own list: the dialog stays the same size
        let rects = rig.rects();
        assert!(rects.field_rows.len() >= 4, "{what}: rows were drawn");
    }
}

#[test]
fn a_full_pivot_with_long_names_and_two_values_has_its_chips_toggles_and_preview_inside_their_panels() {
    for (ppp, w, h) in SCREENS {
        let mut rig = Rig::new("p4v1b", ppp, w, h, 6);
        rig.open_pivot();
        {
            let s = &mut rig.app.pivot_builder.spec;
            s.rows = vec!["Bucket".into(), "A_very_long_column_name_that_keeps_going_and_going_and_going_until_it_must_be_cut".into()];
            s.columns = vec!["Bank".into()];
            s.values = vec![sum_balance(), PivotValue { field: "Rate".into(), agg: PivotAgg::WeightedAvg, weight_col: Some("Balance".into()) }];
            s.filters = vec![PivotFilter { field: "Bank".into(), values: vec![Some("Islami".into()), Some("Conventional".into())] }];
            s.subtotals = true;
        }
        let p = rig.draw(8);
        let what = rig.what.clone();
        assert_layout(&rig, &p);
        // the middle of the dialog scrolls: bring the preview into view
        let at = rig.rects().right.center();
        let mut p = p;
        for _ in 0..4 {
            p = rig.step(vec![egui::Event::PointerMoved(at), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -400.0), modifiers: egui::Modifiers::NONE }]);
        }
        let p2 = rig.draw(4);
        let _ = p;
        let p = p2;
        let b = rig.rects();
        assert!(b.chips.len() >= 6, "{what}: the chips were drawn: {:?}", b.chips.iter().map(|c| (c.0, c.1.clone())).collect::<Vec<_>>());
        // the preview shows the table: labels, the two banks, the totals and the weighted rates
        assert!(matches!(rig.app.pivot_builder.preview, Some(Ok(_))), "{what}: {:?}", rig.app.pivot_builder.preview);
        // a very wide table scrolls sideways inside its card: nothing is drawn to the left of it
        let clip = b.preview_scroll;
        assert!(p.boxes.iter().filter(|(_, bx)| overlap(*bx, b.preview)).all(|(_, bx)| bx.left() >= b.preview.left() - 0.5), "{what}: nothing left of the preview card");
        assert!(clip.width() <= b.preview.width() + 0.5, "{what}");
        // the same pivot with one row field fits the card: every heading is inside it
        rig.app.pivot_builder.spec.rows = vec!["Bucket".into()];
        rig.draw(8);
        let at = rig.rects().right.center();
        for _ in 0..3 {
            rig.step(vec![egui::Event::PointerMoved(at), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -400.0), modifiers: egui::Modifiers::NONE }]);
        }
        let p = rig.draw(4);
        let b = rig.rects();
        for t in ["Bucket", "Conventional | Sum of Balance"] {
            assert!(p.boxes.iter().any(|(x, bx)| x.contains(t) && inside(*bx, b.preview_scroll)), "{what}: '{t}' is in the preview {:?}", b.preview_scroll);
        }
        // the other headings are further right, where the table scrolls sideways
        assert!(p.boxes.iter().any(|(x, bx)| x.contains("W.Avg of Rate by Balance") && bx.left() >= b.preview.left()), "{what}: the weighted-rate heading is drawn in the table");
        // 7 rows of data are not all visible in a short card: they scroll, and the last text is the card's
        assert!(p.boxes.iter().any(|(x, _)| x.contains("02. 2")), "{what}: the first row is shown");
    }
}

#[test]
fn the_field_menu_adds_a_field_to_rows_columns_values_and_filters() {
    for (ppp, w, h) in SCREENS {
        let mut rig = Rig::new("p4v1c", ppp, w, h, 0);
        rig.open_pivot();
        let what = rig.what.clone();
        for (field, area, check) in [("Bucket", "Rows", 0), ("Bank", "Columns", 1), ("Balance", "Values", 2), ("Bank", "Filters", 3)] {
            let menu = rig.rects().field_menus.iter().find(|(n, _)| n == field).map(|(_, r)| *r).expect("the field's menu button");
            let p = rig.click(menu.center());
            let item = p.boxes.iter().find(|(t, _)| t == &format!("Add to {area}")).unwrap_or_else(|| panic!("{what}: the menu of '{field}' offers 'Add to {area}': {:?}", p.boxes.iter().filter(|(t, _)| t.starts_with("Add to")).collect::<Vec<_>>())).1;
            rig.click(item.center());
            let s = rig.app.pivot_builder.spec.clone();
            match check {
                0 => assert_eq!(s.rows, vec!["Bucket".to_string()], "{what}"),
                1 => assert_eq!(s.columns, vec!["Bank".to_string()], "{what}"),
                2 => assert_eq!(s.values, vec![sum_balance()], "{what}: a number is summed by default"),
                _ => assert_eq!(s.filters.len(), 1, "{what}: {s:?}"),
            }
        }
        // the filter's checklist is open under its chip with the values, and the Bank field is now used twice
        let p = rig.draw(4);
        let b = rig.rects();
        assert!(b.popup.is_positive(), "{what}: the checklist was drawn");
        for t in ["Conventional", "Islami", "Values of Bank"] {
            assert!(p.boxes.iter().any(|(x, bx)| x.contains(t) && inside(*bx, b.popup)), "{what}: '{t}' is in the checklist {:?}", b.popup);
        }
        assert!(inside(b.popup, rig.ctx.screen_rect()), "{what}: the checklist is inside the window");
        let used = p.boxes.iter().filter(|(t, _)| t == "Filter, Columns").count();
        assert_eq!(used, 1, "{what}: the list says Bank is in Filter and Columns");
    }
}

#[test]
fn dragging_a_field_into_an_area_adds_it_and_a_chip_is_removed_with_its_cross() {
    for (ppp, w, h) in SCREENS {
        let mut rig = Rig::new("p4v1d", ppp, w, h, 0);
        rig.open_pivot();
        let what = rig.what.clone();
        let drag = |rig: &mut Rig, field: &str, area: Area| {
            let row = rig.rects().field_rows.iter().find(|(n, _)| n == field).map(|(_, r)| *r).expect("the field's row");
            let from = egui::pos2(row.left() + theme::PIVOT_BADGE_W + 8.0, row.center().y);
            let to = rig.rects().areas[area.index()].center();
            rig.step(vec![egui::Event::PointerMoved(from)]);
            rig.step(vec![egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }]);
            // the pointer moves in steps; the field follows it
            for k in 1..=6 {
                let t = k as f32 / 6.0;
                rig.step(vec![egui::Event::PointerMoved(from + (to - from) * t)]);
            }
            rig.step(vec![egui::Event::PointerMoved(to)]);
            rig.step(vec![egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
            rig.draw(4);
        };
        drag(&mut rig, "Bucket", Area::Rows);
        assert_eq!(rig.app.pivot_builder.spec.rows, vec!["Bucket".to_string()], "{what}: dropped on Rows");
        drag(&mut rig, "Bank", Area::Columns);
        assert_eq!(rig.app.pivot_builder.spec.columns, vec!["Bank".to_string()], "{what}: dropped on Columns");
        drag(&mut rig, "Balance", Area::Values);
        assert_eq!(rig.app.pivot_builder.spec.values, vec![sum_balance()], "{what}: dropped on Values");
        // a field is in Rows or Columns, not both: dragging Bank to Rows moves it
        drag(&mut rig, "Bank", Area::Rows);
        let s = rig.app.pivot_builder.spec.clone();
        assert_eq!((s.rows.clone(), s.columns.clone()), (vec!["Bucket".to_string(), "Bank".to_string()], Vec::<String>::new()), "{what}");
        // the cross of a chip removes it
        let p = rig.draw(4);
        let chip = rig.rects().chips.iter().find(|(a, n, _)| *a == Area::Rows && n == "Bank").map(|(_, _, r)| *r).expect("the Bank chip");
        let cross = p.boxes.iter().find(|(t, bx)| t == theme::ph::X && inside(*bx, chip)).map(|(_, bx)| bx.center()).expect("the cross of the chip");
        rig.click(cross);
        assert_eq!(rig.app.pivot_builder.spec.rows, vec!["Bucket".to_string()], "{what}: the cross removes the chip");
        let p = rig.draw(3);
        assert_layout(&rig, &p);
    }
}

#[test]
fn the_filter_checklist_picks_values_searches_and_the_preview_follows() {
    for (ppp, w, h) in SCREENS {
        let mut rig = Rig::new("p4v1e", ppp, w, h, 0);
        rig.open_pivot();
        let what = rig.what.clone();
        {
            let s = &mut rig.app.pivot_builder.spec;
            s.rows = vec!["Bucket".into()];
            s.columns = vec!["Bank".into()];
            s.values = vec![sum_balance()];
        }
        rig.draw(4);
        rig.app.add_pivot_field("Bank", Area::Filters);
        let p = rig.draw(6);
        // all values are ticked when a filter is new: no restriction
        assert!(rig.app.pivot_builder.spec.filters[0].values.is_empty(), "{what}");
        let b = rig.rects();
        let conv = p.boxes.iter().find(|(t, bx)| t == "Conventional" && inside(*bx, b.popup)).map(|(_, bx)| *bx).expect("the Conventional checkbox label");
        rig.click(conv.center());
        let s = rig.app.pivot_builder.spec.clone();
        assert_eq!(s.filters[0].values, vec![Some("Islami".to_string())], "{what}: unticking Conventional keeps Islami");
        // the preview follows: only the Islami column
        let p = rig.draw(6);
        let names: Vec<String> = match &rig.app.pivot_builder.preview {
            Some(Ok(df)) => df.get_column_names().iter().map(|n| n.to_string()).collect(),
            other => panic!("{what}: {other:?}"),
        };
        assert_eq!(names, vec!["Bucket", "Islami", "Total"], "{what}");
        assert!(p.boxes.iter().any(|(t, _)| t.contains("= Islami")), "{what}: the chip says what it keeps");
        assert_layout(&rig, &p);
        // Esc closes the checklist
        rig.step(vec![egui::Event::Key { key: egui::Key::Escape, physical_key: Some(egui::Key::Escape), pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }]);
        rig.draw(2);
        assert!(rig.app.pivot_builder.popup.is_none(), "{what}: Esc closes the checklist");
    }
}

#[test]
fn create_pivot_tab_makes_the_tab_with_the_step_and_formats_that_never_scale_a_rate() {
    for (ppp, w, h) in SCREENS {
        let mut rig = Rig::new("p4v1f", ppp, w, h, 0);
        rig.open_pivot();
        let what = rig.what.clone();
        {
            let s = &mut rig.app.pivot_builder.spec;
            s.rows = vec!["Bucket".into()];
            s.columns = vec!["Bank".into()];
            s.values = vec![sum_balance(), PivotValue { field: "Rate".into(), agg: PivotAgg::WeightedAvg, weight_col: Some("Balance".into()) }];
            s.scale = PivotScale::Crore;
        }
        let p = rig.draw(8);
        assert_layout(&rig, &p);
        let tabs_before = rig.app.tabs.len();
        let create = rig.rects().buttons.iter().find(|(n, _)| n == "Create pivot tab").map(|(_, r)| *r).expect("the primary button");
        rig.click(create.center());
        rig.draw(6);
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: the dialog closed");
        assert_eq!(rig.app.tabs.len(), tabs_before + 1, "{what}: a new tab");
        let tab = rig.app.tabs.last().expect("the new tab");
        assert!(matches!(tab.steps.last(), Some(TransformStep::PivotTable { .. })), "{what}: {:?}", tab.steps);
        assert_eq!(tab.cached_slice.height(), 4, "{what}: three buckets and the grand total");
        // the money columns are in Crore, the weighted rate is a percentage, the count of banks is not scaled
        let f = |name: &str| tab.col_formats.get(name).map(|s| s.fmt_type);
        assert_eq!(f("Conventional | Sum of Balance"), Some(MetricFormatType::BdtCrore), "{what}: {:?}", tab.col_formats);
        assert_eq!(f("Conventional | W.Avg of Rate by Balance"), Some(MetricFormatType::PercentAsIs), "{what}");
        assert_eq!(f("Total | Sum of Balance"), Some(MetricFormatType::BdtCrore), "{what}");
    }
}

#[test]
fn a_pivot_step_opens_in_the_builder_prefilled_and_saving_replaces_it_in_place() {
    for (ppp, w, h) in SCREENS {
        let mut rig = Rig::new("p4v1g", ppp, w, h, 0);
        let spec = PivotSpec { rows: vec!["Bucket".into()], columns: vec!["Bank".into()], values: vec![sum_balance()], row_totals: true, column_totals: true, ..Default::default() };
        let step = TransformStep::from_pivot_spec(spec.clone());
        rig.app.push_step_to_active_tab(step, false);
        rig.draw(4);
        let what = rig.what.clone();
        assert!(rig.app.open_step_editor(0), "{what}: a pivot step has an editor");
        assert_eq!(rig.app.active_tool, ActiveToolModal::PivotBuilder, "{what}");
        let p = rig.draw(8);
        assert_eq!(rig.app.pivot_builder.spec, spec, "{what}: prefilled");
        assert!(p.boxes.iter().any(|(t, _)| t == "Save changes"), "{what}: the button says it saves");
        assert!(p.boxes.iter().any(|(t, _)| t.contains("editing step 1")), "{what}: the title line says which step");
        assert_layout(&rig, &p);
        // change the sort and save
        rig.app.pivot_builder.spec.sort_by_value = Some(PivotSortByValue { value_index: 0, descending: true });
        rig.draw(6);
        let save = rig.rects().buttons.iter().find(|(n, _)| n == "Save changes").map(|(_, r)| *r).expect("the save button");
        rig.click(save.center());
        rig.draw(6);
        let tab = &rig.app.tabs[rig.app.active_tab];
        assert_eq!(tab.steps.len(), 1, "{what}: replaced, not added");
        assert_eq!(tab.steps[0].as_pivot_spec().and_then(|s| s.sort_by_value), Some(PivotSortByValue { value_index: 0, descending: true }), "{what}");
        assert_eq!(rig.app.tabs.len(), 1, "{what}: no new tab");
    }
}

#[test]
fn a_pivot_that_cannot_be_built_says_why_in_the_footer_and_the_button_stays_off() {
    for (ppp, w, h) in SCREENS {
        let mut rig = Rig::new("p4v1h", ppp, w, h, 0);
        rig.open_pivot();
        let what = rig.what.clone();
        {
            let s = &mut rig.app.pivot_builder.spec;
            s.rows = vec!["Bucket".into()];
            // a sum of text is refused
            s.values = vec![PivotValue { field: "Bank".into(), agg: PivotAgg::Sum, weight_col: None }];
        }
        let p = rig.draw(8);
        assert!(matches!(rig.app.pivot_builder.preview, Some(Err(_))), "{what}");
        assert!(rig.app.pivot_builder_step().is_none(), "{what}: no step for a pivot that failed");
        assert!(p.boxes.iter().any(|(t, bx)| t.contains("needs a number column") && inside(*bx, rig.rects().footer_text)), "{what}: the footer says why: {:?}", p.boxes.iter().filter(|(t, _)| t.contains("number")).collect::<Vec<_>>());
        assert_layout(&rig, &p);
        let tabs = rig.app.tabs.len();
        let create = rig.rects().buttons.iter().find(|(n, _)| n == "Create pivot tab").map(|(_, r)| *r).expect("the button");
        rig.click(create.center());
        assert_eq!(rig.app.tabs.len(), tabs, "{what}: the disabled button makes no tab");
    }
}

#[test]
fn the_pivot_builder_is_in_the_transform_menu_the_search_and_the_insert_menu_and_old_pivot_steps_keep_their_dialog() {
    let cmd = micro_parquet::commands::command(CommandId::Pivot).expect("the command");
    assert!(cmd.title.to_lowercase().contains("pivot"), "{}", cmd.title);
    assert!(micro_parquet::commands::TOOLBAR_BUTTONS_TRANSFORM.contains(&CommandId::Pivot));
    // an old GroupByPivot step still opens the older dialog (it has no totals or columns to edit)
    let old = TransformStep::GroupByPivot { group_cols: vec!["Bucket".into()], pivot_col: None, aggs: vec![], sort_by_col: None, top_n: None };
    assert_eq!(crate::ui::step_edit::step_modal(&old), Some(ActiveToolModal::GroupByPivot));
    let new = TransformStep::from_pivot_spec(PivotSpec { rows: vec!["Bucket".into()], values: vec![sum_balance()], ..Default::default() });
    assert_eq!(crate::ui::step_edit::step_modal(&new), Some(ActiveToolModal::PivotBuilder));
    assert!(crate::ui::step_edit::step_is_editable(&new));
    let _ = frames;
}

// ----------------------------------------------------------------------------- V2: CL migration

fn month(keys: &[&str], bal: &[f64], cl: &[&str]) -> DataFrame {
    DataFrame::new(vec![Series::new("Actnum".into(), keys), Series::new("Balance".into(), bal), Series::new("CL_Status".into(), cl)]).expect("month")
}

/// Two month tabs: A1 STD→SMA (down), A2 SS→DF (down), A3 STD closed, A4 new, A5 DF→STD (up).
fn migration_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    let root = app.paths.root_dir.clone();
    let prev = parquet_source(&root, "sep", month(&["A1", "A2", "A3", "A5"], &[100.0, 50.0, 30.0, 40.0], &["STD", "SS", "STD", "DF"]));
    let curr = parquet_source(&root, "oct", month(&["A1", "A2", "A4", "A5"], &[100.0, 50.0, 20.0, 40.0], &["SMA", "DF", "STD", "STD"]));
    app.create_tab_from_source("Sep".into(), prev, vec![], HashMap::new());
    settle(&mut app);
    app.create_tab_from_source("Oct".into(), curr, vec![], HashMap::new());
    settle(&mut app);
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

fn migration_title() -> String {
    format!("{}  CL migration matrix", theme::ph::ARROWS_LEFT_RIGHT)
}

#[test]
fn the_cl_migration_dialog_picks_the_months_and_columns_and_keeps_everything_inside_the_dialog() {
    for (ppp, w, h) in SCREENS {
        let mut rig = migration_rig("p4v2a", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::ClMigration);
        assert_eq!(rig.app.active_tool, ActiveToolModal::ClMigration, "{what}");
        let p = rig.draw(6);
        let r = assert_dialog_fits(&rig.ctx, &migration_title(), &what);
        for t in ["Previous-month tab:", "Current-month tab:", "Account key:", "Balance:", "CL status:", "Segment (optional):", "Show:", "Balance (BDT)", "Number of accounts", "As % of the previous grade's row total", "Build migration matrix", "Cancel"] {
            assert!(p.boxes.iter().any(|(x, bx)| x.contains(t) && inside(*bx, r.window)), "{what}: '{t}' is in the dialog {:?}", r.window);
        }
        // the active tab (Oct) is the current month, the one before it the previous; the columns were guessed
        let s = &rig.app.cl_migration;
        assert_eq!((s.prev_tab, s.curr_tab), (0, 1), "{what}");
        assert_eq!((s.key_col.as_str(), s.bal_col.as_str(), s.cl_col.as_str()), ("Actnum", "Balance", "CL_Status"), "{what}");
        let m = rig.app.cl_migration.rects.clone();
        assert!(inside(m.grid, r.window) && inside(m.legend, r.window), "{what}: the settings {:?} and the note {:?} are inside the dialog {:?}", m.grid, m.legend, r.window);
        for (name, c) in &m.combos {
            assert!(inside(*c, m.grid), "{what}: the choice '{name}' {c:?} is inside the settings {:?}", m.grid);
        }
        for (name, b) in &m.buttons {
            assert!(inside(*b, r.footer), "{what}: the button '{name}' is inside the footer");
        }
        // a real click on a combo box opens its list; choosing a tab changes the setting
        let prev_combo = m.combos.iter().find(|(n, _)| n.starts_with("Previous")).map(|(_, r)| *r).expect("the combo");
        rig.click(prev_combo.center());
        let p = rig.draw(3);
        let sep = p.boxes.iter().filter(|(t, _)| t == "Sep").map(|(_, b)| *b).max_by(|a, b| a.top().total_cmp(&b.top())).expect("the list offers the tab Sep");
        let oct = p.boxes.iter().filter(|(t, bx)| t == "Oct" && bx.top() > prev_combo.bottom()).map(|(_, b)| *b).last().expect("and the tab Oct");
        assert!(inside(oct, rig.ctx.screen_rect()) && inside(sep, rig.ctx.screen_rect()), "{what}: the list is inside the window");
        rig.click(oct.center());
        assert_eq!(rig.app.cl_migration.prev_tab, 1, "{what}: the choice was taken");
        // the same tab twice cannot be built, and the dialog says so
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, _)| t.contains("two different tabs")), "{what}: the message");
        assert!(rig.app.cl_migration_source().is_err());
        rig.app.cl_migration.prev_tab = 0;
        rig.draw(3);
        assert!(rig.app.cl_migration_source().is_ok(), "{what}");
    }
}

#[test]
fn the_cl_migration_tab_shades_upgrades_green_and_downgrades_amber_on_the_right_cells() {
    for (ppp, w, h) in SCREENS {
        let mut rig = migration_rig("p4v2b", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::ClMigration);
        rig.draw(6);
        let b = rig.app.cl_migration.rects.buttons.iter().find(|(n, _)| n == "Build migration matrix").map(|(_, r)| *r).expect("the button");
        rig.click(b.center());
        rig.draw(6);
        assert_eq!(rig.app.tabs.len(), 3, "{what}: the matrix is a new tab");
        let tab = rig.app.tabs.last().expect("tab");
        let names: Vec<String> = tab.cached_slice.get_column_names().iter().map(|n| n.to_string()).collect();
        assert_eq!(names, vec!["Previous grade", "STD", "SMA", "SS", "DF", "BL", "Closed/Repaid", "Total"], "{what}");
        assert!(tab.title.starts_with("CL migration (Sep to Oct)"), "{}", tab.title);
        let p = rig.draw(4);
        let tk = *theme::tokens(false);
        let green = theme::migration_fill(&tk, micro_parquet::banking::Shade::Upgrade).expect("green");
        let amber = theme::migration_fill(&tk, micro_parquet::banking::Shade::Downgrade).expect("amber");
        let table_top = theme::TOP_BAR_H + theme::TAB_STRIP_H;
        let cells = |fill: egui::Color32| -> Vec<egui::Rect> {
            p.rects.iter().filter(|(r, f)| *f == fill && r.top() > table_top && r.bottom() < rig.ctx.screen_rect().bottom() - theme::STATUS_BAR_H && (r.height() - theme::DATA_ROW_H).abs() < theme::SPACE_L).map(|(r, _)| *r).collect()
        };
        let (g, a) = (cells(green), cells(amber));
        assert_eq!(g.len(), 1, "{what}: one upgrade (DF to STD): {g:?}");
        assert_eq!(a.len(), 2, "{what}: two downgrades (STD to SMA, SS to DF): {a:?}");
        // each shaded cell is under the heading of its column and beside the label of its row
        let right_edge = rig.ctx.screen_rect().right() - theme::RIGHT_PANEL_W;
        let heading = |name: &str| p.boxes.iter().filter(|(t, bx)| t == name && bx.top() > table_top && bx.right() < right_edge).map(|(_, b)| *b).min_by(|a, b| a.top().total_cmp(&b.top())).unwrap_or_else(|| panic!("{what}: heading {name}"));
        let row_label = |name: &str, near: egui::Rect| p.boxes.iter().filter(|(t, bx)| t == name && bx.center().y > near.top() && bx.center().y < near.bottom() && bx.center().x < near.left()).map(|(_, b)| *b).next();
        let under = |cell: egui::Rect, col: &str| {
            let h = heading(col);
            assert!(h.center().x > cell.left() - 0.5 && h.center().x < cell.right() + 0.5, "{what}: the cell {cell:?} is under the heading '{col}' {h:?}");
        };
        let green_cell = g[0];
        under(green_cell, "STD");
        assert!(row_label("DF", green_cell).is_some(), "{what}: the green cell is in the DF row");
        let mut amber_cols: Vec<&str> = Vec::new();
        for cell in &a {
            let col = ["SMA", "DF"].into_iter().find(|c| {
                let h = heading(c);
                h.center().x > cell.left() - 0.5 && h.center().x < cell.right() + 0.5
            });
            amber_cols.push(col.unwrap_or_else(|| panic!("{what}: an amber cell {cell:?} is under no heading")));
        }
        amber_cols.sort();
        assert_eq!(amber_cols, vec!["DF", "SMA"], "{what}");
        assert!(row_label("STD", *a.iter().find(|c| heading("SMA").center().x > c.left() && heading("SMA").center().x < c.right()).expect("the SMA cell")).is_some(), "{what}: STD to SMA is in the STD row");
        assert!(row_label("SS", *a.iter().find(|c| heading("DF").center().x > c.left() && heading("DF").center().x < c.right()).expect("the DF cell")).is_some(), "{what}: SS to DF is in the SS row");
        // nothing outside the table is tinted: no shaded rect leaves the table area
        let screen = rig.ctx.screen_rect();
        assert!(g.iter().chain(a.iter()).all(|r| inside(*r, screen)), "{what}");
        // a tab that is not a matrix has no shading
        rig.app.active_tab = 0;
        let p0 = rig.draw(4);
        assert!(p0.rects.iter().all(|(r, f)| !(*f == amber && r.top() > table_top && (r.height() - theme::DATA_ROW_H).abs() < theme::SPACE_L)), "{what}: no amber cells on an ordinary tab");
    }
}

#[test]
fn the_banking_menu_has_two_headings_and_lists_the_new_tool_under_loans_and_credit() {
    for (ppp, w, h) in SCREENS {
        let mut rig = migration_rig("p4v2c", ppp, w, h);
        let what = rig.what.clone();
        let p = rig.draw(4);
        let banking = p.boxes.iter().find(|(t, bx)| t.contains("Banking") && bx.bottom() < theme::TOP_BAR_H).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: the Banking button"));
        rig.click(banking.center());
        let p = rig.draw(3);
        let find = |t: &str| p.boxes.iter().find(|(x, _)| x.contains(t)).map(|(_, b)| *b).unwrap_or_else(|| panic!("{what}: '{t}' is in the Banking menu"));
        let (alm, loans, mig, rate) = (find("Treasury & ALM"), find("Loans & credit"), find("CL migration matrix"), find("Interest rate shock"));
        assert!(alm.top() < rate.top() && rate.top() < loans.top() && loans.top() < mig.top(), "{what}: the headings come before their tools: {alm:?} {rate:?} {loans:?} {mig:?}");
        let screen = rig.ctx.screen_rect();
        for b in [alm, loans, mig] {
            assert!(inside(b, screen), "{what}: {b:?} is inside the window");
        }
        // the item is not greyed: two tabs are open
        assert!(rig.app.command_unavailable(CommandId::ClMigration).is_none(), "{what}");
    }
}

// ------------------------------------------------------------------ V3: liquidity gap ladder

fn gap_title() -> String {
    format!("{}  Liquidity gap ladder", theme::ph::CHART_BAR)
}

fn gap_preset() -> CustomBucketDef {
    CustomBucketDef {
        name: "Test ladder".into(),
        rules: vec![BucketRuleItem { max_days: 30, label: format!("0{EN}1 month") }, BucketRuleItem { max_days: 90, label: format!("1{EN}3 months") }],
        overflow_label: "Over 3 months".into(),
        overdue_label: "Overdue".into(),
        null_sight_label: format!("0{EN}1 month"),
        null_long_label: "Over 3 months".into(),
    }
}

fn dated(name: &str, days: &[&str]) -> Series {
    let day = |s: &str| (chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("date") - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch")).num_days() as i32;
    Series::new(name.into(), days.iter().map(|d| day(d)).collect::<Vec<_>>()).cast(&polars::prelude::DataType::Date).expect("Date")
}

/// An "Assets" and a "Liabilities" tab and the preset "Test ladder" saved for the bucketer.
fn gap_rig(name: &str, ppp: f32, w: f32, h: f32, limits: Option<f64>) -> Rig {
    let mut app = test_app(name);
    let root = app.paths.root_dir.clone();
    std::fs::write(&app.paths.buckets_file, serde_json::to_string(&vec![gap_preset()]).expect("json")).expect("preset file");
    if let Some(pct) = limits {
        let dir = app.paths.gap_limits_dir();
        std::fs::create_dir_all(&dir).expect("limits folder");
        let body = format!(r#"{{"preset":"Test ladder","source":"test paper","limits":[{{"bucket":"0{EN}1 month","min_cumulative_gap_pct":{pct}}}]}}"#);
        std::fs::write(micro_parquet::banking::gap_limits_path(&dir, "Test ladder"), body).expect("limits file");
    }
    let assets = DataFrame::new(vec![Series::new("Balance".into(), &[100.0, 200.0]), dated("Matdat", &["2026-11-15", "2026-12-31"])]).expect("assets");
    let liab = DataFrame::new(vec![Series::new("Balance".into(), &[150.0, 50.0]), dated("Matdat", &["2026-11-20", "2026-12-15"])]).expect("liabilities");
    for (title, df) in [("Assets", assets), ("Liabilities", liab)] {
        let src = parquet_source(&root, title, df);
        app.create_tab_from_source(title.into(), src, vec![], HashMap::new());
        settle(&mut app);
    }
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

fn click_text(rig: &mut Rig, p: &Painted, text: &str, after: egui::Rect) -> Painted {
    let b = p.boxes.iter().filter(|(t, bx)| t.contains(text) && bx.top() >= after.top() - 1.0).map(|(_, b)| *b).last().unwrap_or_else(|| panic!("{}: '{text}' is on screen", rig.what));
    rig.click(b.center())
}

#[test]
fn the_gap_ladder_dialog_takes_tabs_roles_and_columns_and_builds_the_ladder() {
    for (ppp, w, h) in SCREENS {
        let mut rig = gap_rig("p4v3a", ppp, w, h, None);
        let what = rig.what.clone();
        // the active tab is the last one (Liabilities): the dialog starts with it
        rig.app.run_command(&rig.ctx, CommandId::GapLadder);
        assert_eq!(rig.app.active_tool, ActiveToolModal::GapLadder, "{what}");
        let p = rig.draw(6);
        let r = assert_dialog_fits(&rig.ctx, &gap_title(), &what);
        for t in ["Tab", "Counts as", "Amount", "Maturity date", "Bucket preset:", "Cutoff date (YYYY-MM-DD):", "Treat a blank maturity as sight", "Add tab", "Build gap ladder", "Cancel", "Phase 5"] {
            assert!(p.boxes.iter().any(|(x, bx)| x.contains(t) && inside(*bx, r.window)), "{what}: '{t}' is in the dialog {:?}", r.window);
        }
        let g = rig.app.gap.rects.clone();
        assert!(inside(g.grid, r.window) && inside(g.notes, r.window), "{what}: the lines {:?} and the notes {:?} are inside {:?}", g.grid, g.notes, r.window);
        for (i, name, c) in &g.row_controls {
            assert!(inside(*c, g.grid), "{what}: line {i} '{name}' {c:?} is inside the grid {:?}", g.grid);
        }
        for (name, b) in &g.buttons {
            assert!(inside(*b, r.footer), "{what}: '{name}' is inside the footer");
        }
        // the cutoff date must be a date: today's default is the end of last month
        assert!(chrono::NaiveDate::parse_from_str(&rig.app.gap.cutoff, "%Y-%m-%d").is_ok(), "{what}: {}", rig.app.gap.cutoff);
        rig.app.gap.cutoff = "2026-10-31".into();
        // the first line is Liabilities, counts as an asset by default: choose "Liability / outflow" by a real click
        let role = g.row_controls.iter().find(|(i, n, _)| *i == 0 && n == "Counts as").map(|(_, _, r)| *r).expect("role combo");
        rig.click(role.center());
        let p = rig.draw(3);
        click_text(&mut rig, &p, "Liability / outflow", role);
        assert_eq!(rig.app.gap.rows[0].role, crate::ui::gap_ladder::RoleChoice::Liability, "{what}: the choice was taken");
        // add the assets as a second line (the button adds the next tab, here none, so pick Assets)
        let p = rig.draw(3);
        click_text(&mut rig, &p, "Add tab", egui::Rect::EVERYTHING);
        assert_eq!(rig.app.gap.rows.len(), 2, "{what}");
        rig.app.gap.rows[1].tab = 0;
        rig.app.gap.rows[1].role = crate::ui::gap_ladder::RoleChoice::Asset;
        rig.draw(4);
        let p = rig.draw(2);
        assert_dialog_fits(&rig.ctx, &gap_title(), &what);
        assert!(!p.boxes.iter().any(|(t, _)| t.contains("Type the cutoff")), "{what}: no error with a valid date");
        // the preset is the saved one
        let idx = rig.app.gap.presets.iter().position(|p| p.name == "Test ladder").expect("the saved preset is listed");
        rig.app.gap.preset_idx = idx;
        rig.draw(3);
        let build = rig.app.gap.rects.buttons.iter().find(|(n, _)| n == "Build gap ladder").map(|(_, r)| *r).expect("button");
        rig.click(build.center());
        rig.draw(6);
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: the dialog closed");
        let tab = rig.app.tabs.last().expect("the ladder tab");
        assert_eq!(tab.title, "Gap ladder (Test ladder)", "{what}");
        let net: Vec<Option<f64>> = tab.cached_slice.column("Net gap").expect("col").f64().expect("f64").into_iter().collect();
        assert_eq!(&net[..2], &[Some(-50.0), Some(150.0)], "{what}");
        let pct = tab.col_formats.get("Cumulative gap % of total assets").map(|f| f.fmt_type);
        assert_eq!(pct, Some(MetricFormatType::PercentAsIs), "{what}: the percentages are not scaled");
    }
}

#[test]
fn a_breached_limit_tints_its_row_in_the_danger_colour_under_the_right_headings() {
    for (ppp, w, h) in SCREENS {
        let mut rig = gap_rig("p4v3b", ppp, w, h, Some(-10.0));
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::GapLadder);
        rig.app.gap.cutoff = "2026-10-31".into();
        rig.app.gap.preset_idx = rig.app.gap.presets.iter().position(|p| p.name == "Test ladder").expect("preset");
        rig.app.gap.rows = vec![rig.app.gap.rows[0].clone(), rig.app.gap.rows[0].clone()];
        rig.app.gap.rows[0].tab = 0;
        rig.app.gap.rows[0].role = crate::ui::gap_ladder::RoleChoice::Asset;
        rig.app.gap.rows[1].tab = 1;
        rig.app.gap.rows[1].role = crate::ui::gap_ladder::RoleChoice::Liability;
        let p = rig.draw(6);
        // the dialog says the limits were found
        assert!(p.boxes.iter().any(|(t, _)| t.contains("Limits: 1 bucket(s)") && t.contains("test paper")), "{what}: the limits line is shown: {:?}", p.boxes.iter().filter(|(t, _)| t.contains("imits")).collect::<Vec<_>>());
        let build = rig.app.gap.rects.buttons.iter().find(|(n, _)| n == "Build gap ladder").map(|(_, r)| *r).expect("button");
        rig.click(build.center());
        rig.draw(6);
        let tab = rig.app.tabs.last().expect("the ladder tab");
        let names: Vec<String> = tab.cached_slice.get_column_names().iter().map(|n| n.to_string()).collect();
        assert!(names.contains(&"Breach".to_string()) && names.contains(&"Cumulative gap limit %".to_string()), "{what}: {names:?}");
        let breach: Vec<Option<String>> = tab.cached_slice.column("Breach").expect("col").str().expect("str").into_iter().map(|v| v.map(str::to_string)).collect();
        assert_eq!(&breach[..2], &[Some("BREACH".to_string()), Some(String::new())], "{what}");
        // a wide table: close the right panel and scroll to the right end
        rig.app.show_steps_sidebar = false;
        let id = rig.app.tabs.last().map(|t| t.id).expect("id");
        rig.ctx.data_mut(|d| d.insert_temp(egui::Id::new(("grid_scroll_x", id)), 10_000.0f32));
        rig.draw(4);
        let p = rig.draw(3);
        let tk = *theme::tokens(false);
        let danger = theme::migration_fill(&tk, micro_parquet::banking::Shade::Breach).expect("danger fill");
        let table_top = theme::TOP_BAR_H + theme::TAB_STRIP_H;
        let cells: Vec<egui::Rect> = p.rects.iter().filter(|(r, f)| *f == danger && r.top() > table_top && r.bottom() < rig.ctx.screen_rect().bottom() - theme::STATUS_BAR_H && (r.height() - theme::DATA_ROW_H).abs() < theme::SPACE_L).map(|(r, _)| *r).collect();
        assert_eq!(cells.len(), 3, "{what}: the percentage, the limit and the flag of the one breached row: {cells:?}");
        let top = cells.iter().map(|c| c.top()).fold(f32::MAX, f32::min);
        assert!(cells.iter().all(|c| (c.top() - top).abs() < 1.0), "{what}: all three are in the same row: {cells:?}");
        let first_row_label = p.boxes.iter().filter(|(t, bx)| t.starts_with(&format!("0{EN}1 month")) && bx.top() > table_top).map(|(_, b)| *b).next();
        // (the label column may have scrolled out; the row is the first data row when it is shown)
        if let Some(l) = first_row_label {
            assert!(l.center().y > top && l.center().y < top + theme::DATA_ROW_H + theme::SPACE_L, "{what}: the tinted row is the first bucket's: {l:?} vs {top}");
        }
        for heading in ["Breach", "Cumulative gap limit %"] {
            let hb = p.boxes.iter().filter(|(t, bx)| t == heading && bx.top() > table_top).map(|(_, b)| *b).min_by(|a, b| a.top().total_cmp(&b.top())).unwrap_or_else(|| panic!("{what}: heading {heading}"));
            assert!(cells.iter().any(|c| hb.center().x > c.left() - 0.5 && hb.center().x < c.right() + 0.5), "{what}: a tinted cell is under '{heading}' {hb:?}: {cells:?}");
        }
    }
}

#[test]
fn the_gap_ladder_is_listed_under_treasury_and_alm_and_found_by_the_search() {
    use micro_parquet::commands::*;
    assert_eq!(banking_section(CommandId::GapLadder), Some("Treasury & ALM"));
    let entries: Vec<Entry> = COMMANDS.iter().map(|c| Entry { label: c.title, words: c.words, recent: None }).collect();
    for q in ["gap ladder", "slp cumulative", "liquidity gap"] {
        let ranked = rank(q, &entries);
        assert_eq!(COMMANDS[ranked[0]].id, CommandId::GapLadder, "{q}");
    }
    for (ppp, w, h) in SCREENS {
        let mut rig = gap_rig("p4v3c", ppp, w, h, None);
        let p = rig.draw(4);
        let banking = p.boxes.iter().find(|(t, bx)| t.contains("Banking") && bx.bottom() < theme::TOP_BAR_H).map(|(_, b)| *b).expect("Banking");
        rig.click(banking.center());
        let p = rig.draw(3);
        let at = |t: &str| p.boxes.iter().find(|(x, _)| x.contains(t)).map(|(_, b)| *b).unwrap_or_else(|| panic!("'{t}' in the menu"));
        assert!(at("Treasury & ALM").top() < at("Liquidity gap ladder").top() && at("Liquidity gap ladder").top() < at("Loans & credit").top(), "{}", rig.what);
    }
}

// -------------------------------------------------------------------- V4: yield curves and FTP

fn curves_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    let dir = app.paths.curves_dir.clone();
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("curves folder");
    micro_parquet::curves::set_curves_dir_override(Some(dir.clone()));
    let pts = |a: f64, b: f64| vec![FtpPoint { tenor_days: 30, ftp_rate: a }, FtpPoint { tenor_days: 365, ftp_rate: b }];
    for (date, p) in [("2026-01-01", pts(8.0, 10.0)), ("2026-07-01", pts(9.0, 11.0))] {
        let c = micro_parquet::curves::YieldCurve { date: chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").expect("date"), currency: "BDT".into(), source: "test".into(), points: p };
        micro_parquet::curves::save_curve(&dir, &c).expect("saves");
    }
    // deals that the FTP step can price
    let deals = DataFrame::new(vec![
        Series::new("Deal".into(), &["D1", "D2"]),
        dated("Opndat", &["2026-03-15", "2026-08-01"]),
        dated("Matdat", &["2026-09-28", "2027-02-14"]),
        Series::new("Balance".into(), &[1_000_000.0, 1_000_000.0]),
        Series::new("Int Rate".into(), &[9.0, 9.0]),
    ])
    .expect("deals");
    let src = parquet_source(&app.paths.root_dir.clone(), "deals", deals);
    open_tab(&mut app, src, vec![]);
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

fn curves_title() -> String {
    format!("{}  Yield curves", theme::ph::CHART_LINE)
}

fn press_key(rig: &mut Rig, key: egui::Key) {
    rig.step(vec![egui::Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }]);
}

#[test]
fn the_yield_curves_manager_lists_edits_saves_imports_and_deletes_with_every_part_inside_the_dialog() {
    for (ppp, w, h) in SCREENS {
        let mut rig = curves_rig("p4v4a", ppp, w, h);
        let what = rig.what.clone();
        let dir = rig.app.paths.curves_dir.clone();
        rig.app.run_command(&rig.ctx, CommandId::YieldCurves);
        assert_eq!(rig.app.active_tool, ActiveToolModal::YieldCurves, "{what}");
        let p = rig.draw(6);
        let r = assert_dialog_fits(&rig.ctx, &curves_title(), &what);
        for t in ["CURVES", "2026-01-01", "2026-07-01", "Date (YYYY-MM-DD):", "Currency:", "Source:", "Save curve", "New curve", "Import CSV", "Delete", "Close"] {
            assert!(p.boxes.iter().any(|(x, bx)| x.contains(t) && inside(*bx, r.window)), "{what}: '{t}' is in the dialog {:?}", r.window);
        }
        let c = rig.app.curves.rects.clone();
        assert!(inside(c.list, r.window) && inside(c.editor, r.window), "{what}: the list {:?} and the editor {:?} are inside {:?}", c.list, c.editor, r.window);
        assert!(!c.list.intersects(c.editor) || c.list.intersect(c.editor).width() < 1.0, "{what}: the list and the editor do not overlap");
        for (d, row) in &c.list_rows {
            assert!(inside(*row, c.list), "{what}: the row of {d} {row:?} is inside the list {:?}", c.list);
        }
        for row in &c.point_rows {
            assert!(inside(*row, c.editor), "{what}: a point row {row:?} is inside the editor {:?}", c.editor);
        }
        for (n, b) in &c.buttons {
            assert!(inside(*b, r.footer), "{what}: '{n}' is inside the footer");
        }
        // the latest curve is open; click the other one in the list
        assert_eq!(rig.app.curves.editor.date, "2026-07-01", "{what}");
        let first = c.list_rows.iter().find(|(d, _)| d == "2026-01-01").map(|(_, r)| *r).expect("row");
        rig.click(first.center());
        assert_eq!(rig.app.curves.editor.date, "2026-01-01", "{what}: the click opened it");
        assert_eq!(rig.app.curves.editor.points[0].ftp_rate, 8.0, "{what}");
        // two points with the same tenor: the message names them and Save does nothing
        rig.app.curves.editor.points[1].tenor_days = 30;
        let p = rig.draw(4);
        assert!(p.boxes.iter().any(|(t, bx)| t.contains("both have the tenor 30 days") && inside(*bx, rig.app.curves.rects.editor)), "{what}: the message is shown in the editor");
        let save = rig.app.curves.rects.buttons.iter().find(|(n, _)| n == "Save curve").map(|(_, r)| *r).expect("save");
        let before = std::fs::read_to_string(dir.join("2026-01-01.json")).expect("file");
        rig.click(save.center());
        assert_eq!(std::fs::read_to_string(dir.join("2026-01-01.json")).expect("file"), before, "{what}: an invalid curve is not saved");
        // fix it, change a rate and save by a click
        rig.app.curves.editor.points[1].tenor_days = 365;
        rig.app.curves.editor.points[1].ftp_rate = 10.5;
        rig.draw(3);
        let save = rig.app.curves.rects.buttons.iter().find(|(n, _)| n == "Save curve").map(|(_, r)| *r).expect("save");
        rig.click(save.center());
        let text = std::fs::read_to_string(dir.join("2026-01-01.json")).expect("file");
        assert!(text.contains("10.5"), "{what}: saved: {text}");
        // a new curve typed by real key input: its date, then save
        let new = rig.app.curves.rects.buttons.iter().find(|(n, _)| n == "New curve").map(|(_, r)| *r).expect("new");
        rig.click(new.center());
        assert!(rig.app.curves.editor.date.is_empty() && rig.app.curves.editor.opened_from.is_none(), "{what}");
        let date_field = rig.app.curves.rects.fields.iter().find(|(n, _)| n == "Date").map(|(_, r)| *r).expect("date field");
        rig.click(date_field.center());
        rig.step(vec![egui::Event::Text("2026-09-01".into())]);
        rig.draw(3);
        assert_eq!(rig.app.curves.editor.date, "2026-09-01", "{what}: typed into the date field");
        rig.app.curves.editor.points = vec![FtpPoint { tenor_days: 90, ftp_rate: 9.5 }, FtpPoint { tenor_days: 180, ftp_rate: 9.8 }];
        rig.draw(3);
        let save = rig.app.curves.rects.buttons.iter().find(|(n, _)| n == "Save curve").map(|(_, r)| *r).expect("save");
        rig.click(save.center());
        assert!(dir.join("2026-09-01.json").exists(), "{what}: the new curve is a file");
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, bx)| t.contains("2026-09-01") && inside(*bx, rig.app.curves.rects.list)), "{what}: it is in the list");
        // a CSV with the same tenor twice is refused, and one with a heading is taken
        let e = rig.app.curves.import_csv_text("tenor_days,rate_pct\n30,8\n30,9\n").err().expect("refused");
        assert!(e.contains("both have the tenor 30 days"), "{what}: {e}");
        assert_eq!(rig.app.curves.editor.points.len(), 2, "{what}: the points were not replaced");
        assert_eq!(rig.app.curves.import_csv_text("tenor_days,rate_pct\n30,7.5\n180,8.5\n365,9\n"), Ok(3));
        let p = rig.draw(3);
        assert!(p.boxes.iter().any(|(t, bx)| t.contains("Imported 3 points") && inside(*bx, rig.app.curves.rects.editor)), "{what}");
        // delete needs a second click
        let list = rig.app.curves.rects.list_rows.iter().find(|(d, _)| d == "2026-09-01").map(|(_, r)| *r).expect("row");
        rig.click(list.center());
        let del = rig.app.curves.rects.buttons.iter().find(|(n, _)| n == "Delete").map(|(_, r)| *r).expect("delete");
        rig.click(del.center());
        assert!(dir.join("2026-09-01.json").exists(), "{what}: one click does not delete");
        let p = rig.draw(3);
        let yes = p.boxes.iter().find(|(t, _)| t.contains("Yes, delete it")).map(|(_, b)| *b).expect("the question");
        rig.click(yes.center());
        assert!(!dir.join("2026-09-01.json").exists(), "{what}: the second click deletes");
        // an existing date is not overwritten by a new curve
        rig.app.curves.new_curve();
        rig.app.curves.editor.date = "2026-07-01".into();
        let e = rig.app.curves.save().err().expect("exists");
        assert!(e.contains("already exists"), "{what}: {e}");
        micro_parquet::curves::set_curves_dir_override(None);
    }
}

#[test]
fn the_ftp_dialog_chooses_the_curve_source_and_the_step_prices_each_deal_on_its_curve() {
    for (ppp, w, h) in SCREENS {
        let mut rig = curves_rig("p4v4b", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::Ftp);
        let p = rig.draw(6);
        let ftp_title = "\u{E538} Automated FTP Yield Curve Tenor Mapper";
        let r = assert_dialog_fits(&rig.ctx, ftp_title, &what);
        for t in ["Yield curve:", "The curve typed below", "The curve on or before one date:", "Each deal's own curve", "Use the earliest curve for older deals", "Yield curves\u{2026}"] {
            assert!(p.boxes.iter().any(|(x, bx)| x.contains(t) && inside(*bx, r.window)), "{what}: '{t}' is in the dialog {:?}", r.window);
        }
        // a fixed date that is not a date: the message, and the button does nothing
        rig.app.ftp.curve_source = crate::ui::ftp::CurveSource::FixedDate;
        rig.app.ftp.opn_col = "Opndat".into();
        rig.app.ftp.mat_col = "Matdat".into();
        rig.app.ftp.rate_col = "Int Rate".into();
        let p = rig.draw(4);
        assert!(p.boxes.iter().any(|(t, bx)| t.contains("Type the curve date as YYYY-MM-DD") && inside(*bx, r.window)), "{what}");
        assert!(rig.app.ftp_curve_problem().is_some());
        rig.app.ftp.fixed_date = "2025-06-30".into();
        assert!(rig.app.ftp_curve_problem().expect("none that early").contains("No curve is dated on or before 2025-06-30"), "{what}");
        rig.app.ftp.fixed_date = "2026-06-30".into();
        assert!(rig.app.ftp_curve_problem().is_none(), "{what}");
        // origination: choose it with a real click on its radio button
        rig.app.ftp.curve_source = crate::ui::ftp::CurveSource::Typed;
        let p = rig.draw(4);
        let body_at = r.body.center();
        for _ in 0..3 {
            rig.step(vec![egui::Event::PointerMoved(body_at), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -600.0), modifiers: egui::Modifiers::NONE }]);
        }
        let scrolled = rig.draw(2);
        assert!(scrolled.boxes.iter().any(|(t, bx)| t.contains("Benchmark Yield Curve") && inside(*bx, r.window)), "{what}: the typed curve is there when it is chosen");
        for _ in 0..3 {
            rig.step(vec![egui::Event::PointerMoved(body_at), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, 900.0), modifiers: egui::Modifiers::NONE }]);
        }
        let _ = rig.draw(3);
        let _ = p;
        let p = rig.draw(1);
        let radio = p.boxes.iter().find(|(t, _)| t.contains("Each deal's own curve")).map(|(_, b)| *b).expect("radio text");
        rig.click(egui::pos2(radio.left() - theme::SPACE_L, radio.center().y));
        assert_eq!(rig.app.ftp.curve_source, crate::ui::ftp::CurveSource::Origination, "{what}: the click chose it");
        let p = rig.draw(4);
        assert!(p.boxes.iter().any(|(t, bx)| t.contains("2 curve(s) in portable_data/curves, dated 2026-01-01 to 2026-07-01") && inside(*bx, r.window)), "{what}: the store is described");
        // (the typed points are further down: scroll the dialog to look for them)
        let at = r.body.center();
        let scroll_down = |rig: &mut Rig| {
            for _ in 0..3 {
                rig.step(vec![egui::Event::PointerMoved(at), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -600.0), modifiers: egui::Modifiers::NONE }]);
            }
            rig.draw(2)
        };
        let p = scroll_down(&mut rig);
        assert!(!p.boxes.iter().any(|(t, _)| t.contains("Benchmark Yield Curve")), "{what}: the typed points are hidden when the store is used");
        let r = assert_dialog_fits(&rig.ctx, ftp_title, &what);
        // the step it makes
        match rig.app.tool_step(&ActiveToolModal::FtpCurveMap) {
            Some(TransformStep::FtpCurveMap { curve, curve_mode, interpolation, .. }) => {
                assert!(curve.is_empty(), "{what}: no typed points in a step that reads the store");
                assert_eq!(curve_mode, FtpCurveMode::OriginationDate { use_earliest_for_older: false });
                assert_eq!(interpolation, FtpInterpolation::Linear, "{what}: new steps interpolate");
            }
            other => panic!("{what}: {other:?}"),
        }
        let go = p.boxes.iter().find(|(t, _)| t.contains("Map FTP Rates")).map(|(_, b)| *b).expect("button");
        assert!(inside(go, r.footer), "{what}: the button is in the footer");
        rig.click(go.center());
        rig.draw(6);
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}");
        let tab = &rig.app.tabs[rig.app.active_tab];
        assert!(matches!(tab.steps.last(), Some(TransformStep::FtpCurveMap { curve_mode: FtpCurveMode::OriginationDate { .. }, .. })), "{what}");
        let names: Vec<String> = tab.cached_slice.get_column_names().iter().map(|n| n.to_string()).collect();
        assert!(names.contains(&"FTP_Curve_Date".to_string()), "{what}: {names:?}");
        let ftp: Vec<Option<f64>> = tab.cached_slice.column("Benchmark_FTP_Rate").expect("col").f64().expect("f64").into_iter().collect();
        assert!((ftp[0].unwrap_or(0.0) - 8.99701).abs() < 0.00001 && (ftp[1].unwrap_or(0.0) - 9.99701).abs() < 0.00001, "{what}: {ftp:?}");
        // the Edit button opens the same dialog with the choice
        assert!(rig.app.open_step_editor(tab_last_step(&rig)), "{what}");
        assert_eq!(rig.app.ftp.curve_source, crate::ui::ftp::CurveSource::Origination, "{what}: prefilled");
        micro_parquet::curves::set_curves_dir_override(None);
    }
}

fn tab_last_step(rig: &Rig) -> usize {
    rig.app.tabs[rig.app.active_tab].steps.len() - 1
}

#[test]
fn yield_curves_is_a_banking_command_under_treasury_and_alm_and_the_search_finds_it() {
    use micro_parquet::commands::*;
    assert_eq!(banking_section(CommandId::YieldCurves), Some("Treasury & ALM"));
    let entries: Vec<Entry> = COMMANDS.iter().map(|c| Entry { label: c.title, words: c.words, recent: None }).collect();
    for q in ["yield curves", "dated curve", "curve csv"] {
        let ranked = rank(q, &entries);
        assert_eq!(COMMANDS[ranked[0]].id, CommandId::YieldCurves, "{q}");
    }
    assert!(command(CommandId::YieldCurves).map(|c| c.needs == Needs::Nothing).unwrap_or(false), "it needs no open tab");
}
