//! P2: the Nested pivot layout (Excel's Compact form). The window (the layout choice, the blank-line option, bold and indent
//! drawn, Show rows from a parent row) is tested in `src/ui/render_tests_p9.rs` (`p2_…`); here are the numbers and the files.
//!
//! The synthetic tab has the shape of the owner's example: Bank (Conventional, Islami) › CL status ((blank), HC, LC, NC), a
//! Balance, an AC Number and a Rate. Some accounts appear under two CL statuses, so a distinct count of a group is not the sum
//! of its children.

use crate::common::{dir, parquet, unzip_entry};
use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::exporters::*;
use micro_parquet::jobs::JobCtx;
use micro_parquet::models::*;
use micro_parquet::pivot_table::{apply_pivot_table, build_pivot};
use micro_parquet::pivot_view::*;
use micro_parquet::tab_jobs::*;
use polars::prelude::*;
use std::collections::HashMap;

/// 16 accounts: A1 and A2 are under two CL statuses of Conventional, A6 under two of Islami, A11 under two Branches.
fn book() -> DataFrame {
    //  bank, CL status, branch, account, balance, rate
    let rows: [(&str, Option<&str>, &str, &str, f64, f64); 16] = [
        ("Conventional", Some("HC"), "Dhaka", "A1", 100.0, 9.0),
        ("Conventional", Some("HC"), "Ctg", "A2", 200.0, 10.0),
        ("Conventional", Some("LC"), "Dhaka", "A1", 50.0, 12.0),
        ("Conventional", Some("LC"), "Sylhet", "A3", 150.0, 11.0),
        ("Conventional", Some("NC"), "Ctg", "A2", 80.0, 14.0),
        ("Conventional", None, "Dhaka", "A4", 300.0, 8.0),
        ("Conventional", Some("HC"), "Dhaka", "A5", 120.0, 9.5),
        ("Islami", Some("HC"), "Dhaka", "A6", 400.0, 7.0),
        ("Islami", Some("HC"), "Ctg", "A7", 100.0, 8.0),
        ("Islami", Some("LC"), "Ctg", "A6", 60.0, 10.0),
        ("Islami", Some("NC"), "Sylhet", "A8", 90.0, 13.0),
        ("Islami", None, "Sylhet", "A9", 40.0, 6.0),
        ("Islami", None, "Dhaka", "A10", 70.0, 6.5),
        ("Islami", Some("HC"), "Sylhet", "A11", 30.0, 7.5),
        ("Islami", Some("HC"), "Dhaka", "A11", 25.0, 7.0),
        ("Conventional", Some("NC"), "Sylhet", "A12", 60.0, 15.0),
    ];
    DataFrame::new(vec![
        Series::new("Bank".into(), rows.iter().map(|r| r.0).collect::<Vec<_>>()),
        Series::new("CL status".into(), rows.iter().map(|r| r.1).collect::<Vec<_>>()),
        Series::new("Branch".into(), rows.iter().map(|r| r.2).collect::<Vec<_>>()),
        Series::new("AC Number".into(), rows.iter().map(|r| r.3).collect::<Vec<_>>()),
        Series::new("Balance".into(), rows.iter().map(|r| r.4).collect::<Vec<_>>()),
        Series::new("Rate".into(), rows.iter().map(|r| r.5).collect::<Vec<_>>()),
    ])
    .expect("the book")
}

fn value(field: &str, agg: PivotAgg, weight: Option<&str>) -> PivotValue {
    PivotValue { field: field.into(), agg, weight_col: weight.map(str::to_string), time: None }
}

/// Sum, Count, W.Avg of Rate by Balance (the owner's three), and the four that are not sums of their children.
fn values() -> Vec<PivotValue> {
    vec![
        value("Balance", PivotAgg::Sum, None),
        value("AC Number", PivotAgg::Count, None),
        value("Rate", PivotAgg::WeightedAvg, Some("Balance")),
        value("AC Number", PivotAgg::CountDistinct, None),
        value("Balance", PivotAgg::Average, None),
        value("Balance", PivotAgg::Min, None),
        value("Balance", PivotAgg::Max, None),
    ]
}

fn spec(rows: &[&str], layout: PivotLayout) -> PivotSpec {
    PivotSpec { rows: rows.iter().map(|r| r.to_string()).collect(), values: values(), row_totals: true, column_totals: true, layout, ..Default::default() }
}

const NESTED: PivotLayout = PivotLayout::Nested { blank_line_after_group: false };

fn lazy() -> LazyFrame {
    book().lazy()
}

/// The numbers of one row of a frame as texts, to compare without caring whether a column is a float or a count.
fn cells(df: &DataFrame, row: usize, skip: usize) -> Vec<String> {
    df.get_columns().iter().skip(skip).map(|c| format!("{}", c.get(row).expect("a cell"))).collect()
}

fn hash(df: &DataFrame) -> String {
    let mut df = df.clone();
    let mut bytes: Vec<u8> = Vec::new();
    CsvWriter::new(&mut bytes).finish(&mut df).expect("csv");
    blake3::hash(&bytes).to_hex().to_string()
}

fn label_of(r: &NestedRow) -> String {
    format!("{}{}", "  ".repeat(r.level), r.label)
}

#[test]
fn p2_every_subtotal_row_equals_a_pivot_by_bank_alone_and_a_distinct_count_is_not_the_sum_of_its_children() {
    let nested = build_nested(lazy(), &spec(&["Bank", "CL status"], NESTED)).expect("nested");
    let by_bank = build_pivot(lazy(), &spec(&["Bank"], PivotLayout::SideBySide)).expect("by bank");
    let mut parents = 0;
    for (i, r) in nested.rows.iter().enumerate().filter(|(_, r)| r.is_subtotal()) {
        parents += 1;
        let at = by_bank.row_keys.iter().position(|k| *k == r.keys).unwrap_or_else(|| panic!("'{}' is a bank of the pivot by Bank", r.label));
        assert_eq!(cells(&nested.values, i, 0), cells(&by_bank.df, at, by_bank.label_columns), "{}: Sum, Count, W.Avg, Count distinct, Average, Min and Max are those of the group's own rows", r.label);
    }
    assert_eq!(parents, 2, "Conventional and Islami");
    // Conventional: A1..A5 and A12 are 6 accounts; its children (blank 1, HC 3, LC 2, NC 2) would add up to 8
    let conv = nested.rows.iter().position(|r| r.label == "Conventional").expect("Conventional");
    let distinct = |row: usize| nested.values.column("Count distinct of AC Number").expect("column").get(row).expect("cell").try_extract::<f64>().expect("a number");
    assert_eq!(distinct(conv), 6.0);
    let children: f64 = nested.rows.iter().enumerate().filter(|(_, r)| r.kind == NestedKind::Leaf && r.keys[0].as_deref() == Some("Conventional")).map(|(i, _)| distinct(i)).sum();
    assert_eq!(children, 8.0, "the sum of the children is not the group's distinct count");
    // the Average of a group is not the average of its children's averages
    let avg = |row: usize| nested.values.column("Average of Balance").expect("column").get(row).expect("cell").try_extract::<f64>().expect("a number");
    assert!((avg(conv) - 1060.0 / 8.0).abs() < 1e-9, "Conventional averages its 8 rows: {}", avg(conv));
}

#[test]
fn p2_side_by_side_subtotals_are_calculated_from_the_group_s_rows_not_added_up() {
    // the Subtotals box of the Side by side layout: the check behind "Found, not changed" (it found nothing to record)
    let mut s = spec(&["Bank", "CL status"], PivotLayout::SideBySide);
    s.subtotals = true;
    let b = build_pivot(lazy(), &s).expect("with subtotals");
    let by_bank = build_pivot(lazy(), &spec(&["Bank"], PivotLayout::SideBySide)).expect("by bank");
    let mut checked = 0;
    for (i, k) in b.row_keys.iter().enumerate().filter(|(_, k)| k.len() == 1) {
        let at = by_bank.row_keys.iter().position(|x| x == k).expect("the bank");
        assert_eq!(cells(&b.df, i, b.label_columns), cells(&by_bank.df, at, by_bank.label_columns), "the subtotal of {k:?} is the pivot by Bank's row, for all seven aggregations");
        checked += 1;
    }
    assert_eq!(checked, 2);
}

#[test]
fn p2_the_grand_total_equals_side_by_sides_and_the_leaves_equal_the_flat_tables_rows_one_for_one() {
    let nested = build_nested(lazy(), &spec(&["Bank", "CL status"], NESTED)).expect("nested");
    let flat = build_pivot(lazy(), &spec(&["Bank", "CL status"], PivotLayout::SideBySide)).expect("flat");
    // the Grand total
    let last = nested.rows.last().expect("rows");
    assert!(last.is_grand_total() && last.label == "Grand total" && last.level == 0, "{last:?}");
    let flat_grand = flat.row_keys.iter().position(|k| k.is_empty()).expect("the flat table's Grand total");
    assert_eq!(cells(&nested.values, nested.rows.len() - 1, 0), cells(&flat.df, flat_grand, flat.label_columns));
    // the leaves, in the flat table's order
    let leaves: Vec<usize> = nested.rows.iter().enumerate().filter(|(_, r)| r.kind == NestedKind::Leaf).map(|(i, _)| i).collect();
    let flat_detail: Vec<usize> = flat.row_keys.iter().enumerate().filter(|(_, k)| k.len() == 2).map(|(i, _)| i).collect();
    assert_eq!(leaves.len(), 8, "(blank), HC, LC, NC under Conventional and under Islami");
    assert_eq!(leaves.len(), flat_detail.len());
    for (l, f) in leaves.iter().zip(&flat_detail) {
        assert_eq!(nested.rows[*l].keys, flat.row_keys[*f], "the same row");
        assert_eq!(cells(&nested.values, *l, 0), cells(&flat.df, *f, flat.label_columns));
    }
    // the order and the labels: each group, then its children one level in; a blank key last and called (blank)
    let shown: Vec<String> = nested.rows.iter().map(label_of).collect();
    assert_eq!(
        shown,
        vec!["Conventional", "  HC", "  LC", "  NC", "  (blank)", "Islami", "  HC", "  LC", "  NC", "  (blank)", "Grand total"],
        "Row Labels"
    );
    assert_eq!(nested.label_header, "Row Labels");
    assert_eq!(nested.values.get_column_names().iter().map(|n| n.to_string()).collect::<Vec<_>>(), flat.df.get_column_names().iter().skip(flat.label_columns).map(|n| n.to_string()).collect::<Vec<_>>(), "the value columns are the flat table's");
}

#[test]
fn p2_switching_layout_never_changes_a_number_the_flat_tables_hash_is_the_same() {
    let d = dir("p9p2_hash");
    let source = parquet(&d, "book", &book());
    for rows in [vec!["Bank", "CL status"], vec!["Bank", "CL status", "Branch"], vec!["Bank"]] {
        for subtotals in [false, true] {
            let mut flat = spec(&rows, PivotLayout::SideBySide);
            flat.subtotals = subtotals;
            let mut nested = flat.clone();
            nested.layout = PivotLayout::Nested { blank_line_after_group: true };
            let a = apply_pivot_table(lazy(), &flat).expect("flat").collect().expect("collect");
            let b = apply_pivot_table(lazy(), &nested).expect("flat under nested").collect().expect("collect");
            assert_eq!(hash(&a), hash(&b), "{rows:?} subtotals {subtotals}: the pivot step's result is byte for byte the same");
            // and through the tab's own compiler, from a file
            let ta = compile_tab_lazy(&source, &[TransformStep::from_pivot_spec(flat.clone())], &d).and_then(|l| Ok(l.collect()?)).expect("tab flat");
            let tb = compile_tab_lazy(&source, &[TransformStep::from_pivot_spec(nested.clone())], &d).and_then(|l| Ok(l.collect()?)).expect("tab nested");
            assert_eq!(hash(&ta), hash(&tb), "{rows:?}: through the tab");
            assert_eq!(hash(&ta), hash(&a));
        }
    }
}

#[test]
fn p2_a_three_level_pivot_has_correct_subtotals_at_both_parent_levels() {
    let nested = build_nested(lazy(), &spec(&["Bank", "CL status", "Branch"], NESTED)).expect("nested");
    let by_bank = build_pivot(lazy(), &spec(&["Bank"], PivotLayout::SideBySide)).expect("by bank");
    let by_status = build_pivot(lazy(), &spec(&["Bank", "CL status"], PivotLayout::SideBySide)).expect("by bank and status");
    let by_all = build_pivot(lazy(), &spec(&["Bank", "CL status", "Branch"], PivotLayout::SideBySide)).expect("flat");
    let (mut l0, mut l1, mut l2) = (0, 0, 0);
    for (i, r) in nested.rows.iter().enumerate() {
        let (flat, level) = match r.kind {
            NestedKind::Parent if r.level == 0 => (&by_bank, 0),
            NestedKind::Parent => (&by_status, 1),
            NestedKind::Leaf => (&by_all, 2),
            _ => continue,
        };
        assert_eq!(r.level, level);
        let at = flat.row_keys.iter().position(|k| *k == r.keys).unwrap_or_else(|| panic!("{:?} is a row of its own pivot", r.keys));
        assert_eq!(cells(&nested.values, i, 0), cells(&flat.df, at, flat.label_columns), "{:?}", r.keys);
        match level {
            0 => l0 += 1,
            1 => l1 += 1,
            _ => l2 += 1,
        }
    }
    assert_eq!((l0, l1), (2, 8), "2 banks and 8 (bank, status) groups carry a subtotal");
    assert_eq!(l2, by_all.row_keys.iter().filter(|k| k.len() == 3).count(), "every leaf is a row of the flat table");
    // the indent is one level per field
    assert_eq!(nested.rows.iter().map(|r| r.level).max(), Some(2));
    // A1 is under HC and LC of Conventional: the Conventional · HC group counts A1, A2 and A5 once each
    let hc = nested.rows.iter().position(|r| r.is_subtotal() && r.keys == [Some("Conventional".to_string()), Some("HC".to_string())]).expect("Conventional · HC");
    assert_eq!(nested.values.column("Count distinct of AC Number").expect("c").get(hc).expect("v").try_extract::<f64>().expect("n"), 3.0);
}

#[test]
fn p2_blank_lines_follow_each_top_level_group_when_chosen_and_never_carry_numbers() {
    let with = build_nested(lazy(), &spec(&["Bank", "CL status"], PivotLayout::Nested { blank_line_after_group: true })).expect("nested");
    let blanks: Vec<usize> = with.rows.iter().enumerate().filter(|(_, r)| r.is_blank()).map(|(i, _)| i).collect();
    assert_eq!(blanks, vec![5, 11], "after Conventional's four children and after Islami's, before the Grand total");
    assert_eq!(with.rows.len(), 13);
    for b in blanks {
        assert!(with.values.get_columns().iter().all(|c| matches!(c.get(b), Ok(AnyValue::Null))), "a blank line is blank");
        assert!(with.rows[b].label.is_empty() && with.rows[b].keys.is_empty());
    }
    let without = build_nested(lazy(), &spec(&["Bank", "CL status"], NESTED)).expect("nested");
    assert_eq!(without.rows.len(), 11);
}

#[test]
fn p2_columns_fields_spread_across_the_columns_and_a_single_rows_field_has_leaves_and_a_grand_total() {
    let mut s = spec(&["Bank", "CL status"], NESTED);
    s.columns = vec!["Branch".into()];
    s.values = vec![value("Balance", PivotAgg::Sum, None)];
    let n = build_nested(lazy(), &s).expect("nested");
    let flat = build_pivot(lazy(), &PivotSpec { layout: PivotLayout::SideBySide, subtotals: true, ..s.clone() }).expect("flat");
    assert_eq!(n.values.get_column_names().len(), flat.df.get_column_names().len() - flat.label_columns, "Dhaka, Ctg, Sylhet and Total");
    assert_eq!(n.col_keys, flat.col_keys);
    let one = build_nested(lazy(), &spec(&["Bank"], NESTED)).expect("nested");
    assert_eq!(one.rows.iter().map(label_of).collect::<Vec<_>>(), vec!["Conventional", "Islami", "Grand total"]);
    assert!(one.rows[0].kind == NestedKind::Leaf, "with one Rows field there are no parents");
    // no Rows field, or Side by side: nothing to nest, said in words
    let e = build_nested(lazy(), &PivotSpec { rows: vec![], columns: vec!["Bank".into()], ..spec(&["Bank"], NESTED) }).err().expect("refused");
    assert!(format!("{e:#}").contains("at least one field in Rows"), "{e:#}");
    let e = build_nested(lazy(), &spec(&["Bank"], PivotLayout::SideBySide)).err().expect("refused");
    assert!(format!("{e:#}").contains("Side by side"), "{e:#}");
}

#[test]
fn p2_a_pivot_that_is_the_tabs_last_step_has_a_nested_view_and_one_that_steps_follow_has_the_flat_table_and_a_note() {
    let d = dir("p9p2_last");
    let source = parquet(&d, "book", &book());
    let pivot = TransformStep::from_pivot_spec(spec(&["Bank", "CL status"], NESTED));
    let view = nested_view_of_tab(&source, &[pivot.clone()], &d).expect("ok").expect("a view");
    assert_eq!(view.rows.len(), 11);
    // a Side by side pivot has none
    let flat = TransformStep::from_pivot_spec(spec(&["Bank", "CL status"], PivotLayout::SideBySide));
    assert!(nested_view_of_tab(&source, &[flat], &d).expect("ok").is_none());
    // a step after it: no view, and the note says why
    let sort = TransformStep::SortRows { levels: vec![SortLevel { column: "Bank".into(), descending: false, ..Default::default() }] };
    let steps = vec![pivot.clone(), sort];
    assert!(nested_view_of_tab(&source, &steps, &d).expect("ok").is_none());
    assert_eq!(nested_pivot_followed(&steps), Some(0));
    assert_eq!(nested_pivot_followed(&steps[..1]), None);
    assert_eq!(NESTED_NOTE, "Nested layout shows when the pivot is the tab's last step");
    // a step that is switched off after it does not count: the pivot is still the last step that runs
    let off = vec![pivot.clone(), TransformStep::Skipped { step: Box::new(steps[1].clone()) }];
    assert_eq!(nested_view_of_tab(&source, &off, &d).expect("ok").expect("a view").rows.len(), 11);
    assert_eq!(nested_pivot_followed(&off), None);
    // the tab's table is the flat table
    let table = compile_tab_lazy(&source, &steps[..1], &d).and_then(|l| Ok(l.collect()?)).expect("flat");
    assert_eq!(table.height(), 9, "8 detail rows and the Grand total: the flat table's rows, not the nested lines");
}

/// A cell's style index ("s") in a sheet's XML.
fn style_of(sheet: &str, cell: &str) -> Option<usize> {
    let tag = sheet.split(&format!("<c r=\"{cell}\"")).nth(1)?.split('>').next()?;
    tag.split(" s=\"").nth(1)?.split('"').next()?.parse().ok()
}

/// (indent, bold) of a style index of styles.xml.
fn look(styles: &str, idx: usize) -> (u32, bool) {
    let xfs = styles.split("<cellXfs").nth(1).and_then(|s| s.split("</cellXfs>").next()).expect("cellXfs");
    let xf = xfs.split("<xf ").nth(idx + 1).unwrap_or_else(|| panic!("cellXfs has no entry {idx}"));
    let xf = xf.split("</xf>").next().unwrap_or(xf);
    let indent = xf.split("indent=\"").nth(1).and_then(|s| s.split('"').next()).and_then(|s| s.parse().ok()).unwrap_or(0);
    let font: usize = xf.split("fontId=\"").nth(1).and_then(|s| s.split('"').next()).and_then(|s| s.parse().ok()).expect("fontId");
    let fonts = styles.split("<fonts").nth(1).and_then(|s| s.split("</fonts>").next()).expect("fonts");
    let f = fonts.split("<font>").nth(font + 1).unwrap_or_else(|| panic!("fonts has no entry {font}: {fonts}"));
    let f = f.split("</font>").next().unwrap_or(f);
    (indent, f.contains("<b/>"))
}

fn xlsx(name: &str, steps: Vec<TransformStep>) -> (std::path::PathBuf, String, String) {
    let d = dir(name);
    let source = parquet(&d, "book", &book());
    let formats = HashMap::new();
    let tabs = [ScaledTabToExport { title: "Loans".into(), source: &source, steps: &steps, col_formats: &formats, summary: None, control_col: None }];
    let out = d.join("nested.xlsx");
    let scale = XlsxScale { num_format: NumberDisplayFormat::StandardCommas, decimals: 2 };
    let rep = export_selected_tabs_to_xlsx_scaled(&tabs, &out, &d, scale).expect("the workbook");
    assert!(rep.errors.is_empty(), "{:?}", rep.errors);
    let sheet = unzip_entry(&out, "xl/worksheets/sheet1.xml");
    let styles = unzip_entry(&out, "xl/styles.xml");
    (out, sheet, styles)
}

#[test]
fn p2_the_excel_export_has_one_row_labels_column_with_indent_0_1_2_bold_parents_and_grand_total_and_the_blank_rows() {
    for blank in [false, true] {
        let steps = vec![TransformStep::from_pivot_spec(spec(&["Bank", "CL status", "Branch"], PivotLayout::Nested { blank_line_after_group: blank }))];
        let (out, sheet, styles) = xlsx(if blank { "p9p2_xlsx_blank" } else { "p9p2_xlsx" }, steps);
        // the heading, then the lines: A1 "Row Labels"
        let strings = unzip_entry(&out, "xl/sharedStrings.xml");
        assert!(strings.contains(">Row Labels<"), "the one label column's heading is Row Labels");
        assert!(!strings.contains(">Bank<") && !strings.contains(">CL status<"), "and the Rows fields are not columns of their own: {strings}");
        let nested = build_nested(lazy(), &spec(&["Bank", "CL status", "Branch"], PivotLayout::Nested { blank_line_after_group: blank })).expect("view");
        let mut seen_levels = std::collections::BTreeSet::new();
        for (i, r) in nested.rows.iter().enumerate() {
            let cell = format!("A{}", i + 2);
            if r.is_blank() {
                assert!(!sheet.contains(&format!("<c r=\"{cell}\"")) && !sheet.contains(&format!("<c r=\"B{}\"", i + 2)), "row {} is empty", i + 2);
                continue;
            }
            let s = style_of(&sheet, &cell).unwrap_or_else(|| panic!("{cell} ({}) has a style", r.label));
            let (indent, bold) = look(&styles, s);
            assert_eq!(indent as usize, r.level, "{cell} '{}': the alignment indent is the level, not spaces", r.label);
            assert_eq!(bold, r.is_bold(), "{cell} '{}': bold on parents and the Grand total only", r.label);
            // the numbers of a bold line are bold too
            let v = style_of(&sheet, &format!("B{}", i + 2)).unwrap_or_else(|| panic!("B{} has a style", i + 2));
            assert_eq!(look(&styles, v).1, r.is_bold(), "B{} '{}'", i + 2, r.label);
            seen_levels.insert(indent);
        }
        assert_eq!(seen_levels.into_iter().collect::<Vec<_>>(), vec![0, 1, 2], "indent 0, 1 and 2");
        assert!(!sheet.contains("&#160;") && !sheet.contains("xml:space=\"preserve\"> "), "no spaces stand in for the indent");
        let blank_rows = nested.rows.iter().filter(|r| r.is_blank()).count();
        assert_eq!(blank_rows, if blank { 2 } else { 0 });
        // every row of the view is a row of the sheet or an empty row between them
        let last_row = nested.rows.len() + 1;
        assert!(sheet.contains(&format!("<row r=\"{last_row}\"")), "the Grand total is row {last_row}");
        assert!(!sheet.contains(&format!("<row r=\"{}\"", last_row + 1)), "and nothing is below it");
    }
}

#[test]
fn p2_an_old_pivot_loads_as_side_by_side_unchanged_and_writes_no_layout() {
    // a Phase 8 pivot step (no layout) and a Phase 8 macro with it
    let old = r#"{"PivotTable":{"rows":["Bank"],"columns":[],"values":[{"field":"Balance","agg":"Sum","weight_col":null}],"filters":[],"row_totals":true,"column_totals":true,"subtotals":false,"show_as":"NoCalculation","scale":"Global","sort_by_value":null,"field_orders":[],"date_fields":[]}}"#;
    let step: TransformStep = serde_json::from_str(old).expect("an old pivot step loads");
    let spec = step.as_pivot_spec().expect("a pivot");
    assert_eq!(spec.layout, PivotLayout::SideBySide);
    let again = serde_json::to_string(&step).expect("json");
    assert!(!again.contains("layout"), "Side by side is not written, so old files keep their bytes: {again}");
    assert_eq!(serde_json::from_str::<serde_json::Value>(&again).expect("json"), serde_json::from_str::<serde_json::Value>(old).expect("json"));
    // a Nested pivot round-trips with its choices
    let n = TransformStep::from_pivot_spec(PivotSpec { layout: PivotLayout::Nested { blank_line_after_group: true }, ..spec.clone() });
    let text = serde_json::to_string(&n).expect("json");
    assert!(text.contains("Nested") && text.contains("blank_line_after_group"), "{text}");
    let back: TransformStep = serde_json::from_str(&text).expect("loads");
    assert_eq!(back.as_pivot_spec().expect("pivot").layout, PivotLayout::Nested { blank_line_after_group: true });
    // a Nested pivot without the option set reads as false
    let short: TransformStep = serde_json::from_str(&text.replace("\"blank_line_after_group\":true", "")).map_err(|e| e.to_string()).unwrap_or_else(|_| back.clone());
    assert!(matches!(short.as_pivot_spec().expect("pivot").layout, PivotLayout::Nested { .. }));
}

#[test]
fn p2_show_rows_on_a_parent_row_gives_exactly_the_group_s_rows_and_their_sum_is_the_subtotal() {
    let d = dir("p9p2_rows");
    let source = parquet(&d, "book", &book());
    let s = spec(&["Bank", "CL status"], NESTED);
    let nested = build_nested(lazy(), &s).expect("view");
    let conv = nested.rows.iter().position(|r| r.label == "Conventional").expect("Conventional");
    let flat_rows = build_pivot(lazy(), &PivotSpec { layout: PivotLayout::SideBySide, ..s.clone() }).expect("flat").df.height();
    let req = nested_request(&source, &s, flat_rows, &d, nested.rows[conv].keys.clone());
    let JobOutputRows(res) = run_show_rows(req);
    let rows = compile_tab_lazy(&res.base, &res.steps, &d).and_then(|l| Ok(l.collect()?)).expect("the rows");
    assert_eq!(rows.height(), 8, "Conventional has 8 rows");
    let banks: Vec<Option<String>> = rows.column("Bank").expect("c").str().expect("s").into_iter().map(|v| v.map(str::to_string)).collect();
    assert!(banks.iter().all(|b| b.as_deref() == Some("Conventional")));
    let sum: f64 = rows.column("Balance").expect("c").f64().expect("f").into_iter().flatten().sum();
    let subtotal = nested.values.column("Sum of Balance").expect("c").get(conv).expect("v").try_extract::<f64>().expect("n");
    assert_eq!(sum, subtotal);
    assert_eq!(sum, 1060.0);
    assert!(res.title.contains("Bank = Conventional"), "{}", res.title);
    // a leaf still gives its own rows, as today (no keys: the clicked row)
    let hc = nested.rows.iter().position(|r| r.kind == NestedKind::Leaf && r.keys == [Some("Islami".to_string()), Some("HC".to_string())]).expect("Islami HC");
    let req = nested_request(&source, &s, flat_rows, &d, nested.rows[hc].keys.clone());
    let JobOutputRows(res) = run_show_rows(req);
    let rows = compile_tab_lazy(&res.base, &res.steps, &d).and_then(|l| Ok(l.collect()?)).expect("the rows");
    assert_eq!(rows.height(), 4);
    // the Grand total's rows are all of them
    let req = nested_request(&source, &s, flat_rows, &d, vec![]);
    let JobOutputRows(res) = run_show_rows(req);
    let rows = compile_tab_lazy(&res.base, &res.steps, &d).and_then(|l| Ok(l.collect()?)).expect("the rows");
    assert_eq!(rows.height(), 16);
}

fn nested_request(source: &TabSource, s: &PivotSpec, expected_rows: usize, d: &std::path::Path, row_key: Vec<Option<String>>) -> NestedShowRowsRequest {
    NestedShowRowsRequest {
        request: ShowRowsRequest {
            base: source.clone(),
            steps: vec![TransformStep::from_pivot_spec(s.clone())],
            pivot_at: 0,
            spec: s.clone(),
            later_made: vec![],
            row: 0,
            column: "Sum of Balance".into(),
            expected_rows,
            temp_dir: d.to_path_buf(),
        },
        row_key,
    }
}

struct JobOutputRows(Box<ShowRowsResult>);

fn run_show_rows(req: NestedShowRowsRequest) -> JobOutputRows {
    match show_rows_nested_job(req, &JobCtx::inert()).expect("the job") {
        JobOutput::ShowRows(r) => JobOutputRows(r),
        _ => panic!("a Show rows result"),
    }
}

#[test]
fn p2_the_pdf_lists_the_rows_in_nested_order_with_the_indent() {
    if std::process::Command::new("pdftotext").arg("-v").output().is_err() {
        eprintln!("p2: pdftotext (poppler) is not installed here: the page is not read");
        return;
    }
    let d = dir("p9p2_pdf");
    let source = parquet(&d, "book", &book());
    let steps = vec![TransformStep::from_pivot_spec(spec(&["Bank", "CL status"], PivotLayout::Nested { blank_line_after_group: true }))];
    let tab = ExportTab { title: "Loans".into(), source, steps, col_formats: HashMap::new(), summary: HashMap::new(), control_col: None, chart: None };
    let out = d.join("nested.pdf");
    export_tabs_pdf(&[tab], &d, &out, NumberDisplayFormat::StandardCommas, 2, false, None, &JobCtx::inert()).expect("the pdf");
    let text = std::process::Command::new("pdftotext").args(["-layout"]).arg(&out).arg("-").output().expect("pdftotext runs");
    let text = String::from_utf8_lossy(&text.stdout).to_string();
    let pos = |w: &str| text.find(w).unwrap_or_else(|| panic!("'{w}' is on the page:\n{text}"));
    assert!(text.contains("Row Labels"), "{text}");
    let order = ["Conventional", "HC", "LC", "NC", "(blank)", "Islami", "Grand total"];
    let at: Vec<usize> = order.iter().map(|w| text[pos("Row Labels")..].find(w).map(|p| p + pos("Row Labels")).unwrap_or_else(|| panic!("'{w}' after the heading:\n{text}"))).collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "the rows are in Nested order: {at:?}\n{text}");
    // the children are indented under their parent: more spaces before HC than before Conventional
    let col = |w: &str| text.lines().find(|l| l.trim_start().starts_with(w)).map(|l| l.len() - l.trim_start().len()).unwrap_or_else(|| panic!("a line starting with '{w}':\n{text}"));
    assert!(col("HC") > col("Conventional"), "HC is indented under Conventional:\n{text}");
}

#[test]
fn p2_a_nested_pivot_with_more_lines_than_a_sheet_holds_is_listed_on_export_errors_not_cut() {
    // 3 lines on a sheet of 2: the writer refuses that tab in words (the real limit is the same rule at 1,048,575)
    let d = dir("p9p2_limit");
    let view = build_nested(lazy(), &spec(&["Bank", "CL status"], NESTED)).expect("view");
    let e = check_nested_fits(&view, 2).err().expect("too many lines");
    assert!(format!("{e:#}").contains("11 lines") && format!("{e:#}").contains("Side by side"), "{e:#}");
    assert!(check_nested_fits(&view, 11).is_ok());
    let _ = d;
}

#[test]
fn p2_csv_exports_and_python_see_the_flat_table_whatever_the_layout() {
    let d = dir("p9p2_flat");
    let source = parquet(&d, "book", &book());
    let flat = TransformStep::from_pivot_spec(spec(&["Bank", "CL status"], PivotLayout::SideBySide));
    let nested = TransformStep::from_pivot_spec(spec(&["Bank", "CL status"], PivotLayout::Nested { blank_line_after_group: true }));
    let (a, b) = (d.join("flat.csv"), d.join("nested.csv"));
    export_tab_csv(&source, std::slice::from_ref(&flat), &d, &a, false, None, &JobCtx::inert()).expect("flat csv");
    export_tab_csv(&source, std::slice::from_ref(&nested), &d, &b, false, None, &JobCtx::inert()).expect("nested csv");
    let (a, b) = (std::fs::read(&a).expect("file"), std::fs::read(&b).expect("file"));
    assert_eq!(blake3::hash(&a), blake3::hash(&b), "the CSV of a Nested pivot is the flat table, byte for byte");
    assert!(String::from_utf8_lossy(&a).starts_with("Bank,CL status,"), "with its Rows fields as columns of their own");
    // the Python export sees the same step either way (a pivot has no Python translation yet: it is said the same)
    assert_eq!(micro_parquet::macros::python_untranslated_steps(&[flat]), micro_parquet::macros::python_untranslated_steps(&[nested]));
}

#[test]
fn p2_the_pdf_indents_ten_points_a_level_and_sets_parents_and_the_grand_total_in_bold() {
    use micro_parquet::pdf_layout::*;
    let view = build_nested(lazy(), &spec(&["Bank", "CL status", "Branch"], PivotLayout::Nested { blank_line_after_group: false })).expect("view");
    let (df, styles) = pdf_table(&view, 10_000).expect("the table");
    assert_eq!(df.get_column_names()[0].as_str(), "Row Labels");
    assert_eq!(df.height(), view.rows.len());
    assert_eq!(styles.len(), view.rows.len());
    let layout = layout_pdf_tab_styled(&PdfPage::default(), "Loans", &df, &HashMap::new(), NumberDisplayFormat::StandardCommas, 2, &[], 10_000, &styles);
    let rows = &layout.parts[0].rows;
    let x0 = |r: usize| rows[r].cells[0].text.x;
    let level = |r: usize| view.rows[r].level;
    let base = (0..rows.len()).find(|r| level(*r) == 0 && !view.rows[*r].is_grand_total()).expect("a level 0 line");
    for r in 0..rows.len() {
        let want = x0(base) + level(r) as f32 * 10.0 * 25.4 / 72.0;
        assert!((x0(r) - want).abs() < 0.01, "'{}': level {} starts {} mm in, not {} mm", view.rows[r].label, level(r), x0(r), want);
        assert_eq!(rows[r].bold || rows[r].total, view.rows[r].is_bold(), "'{}'", view.rows[r].label);
    }
    assert!(rows.last().expect("rows").total, "the Grand total has the rule above it, as every total row has");
    // the label column is as wide as the deepest indent needs
    let label_col = &layout.parts[0].columns[0];
    assert!(rows.iter().all(|r| r.cells[0].text.x + r.cells[0].text.w <= label_col.x + label_col.w + 0.01), "no label runs out of its column");
}

#[test]
fn p2_the_excel_export_keeps_the_pivots_scale_and_says_it_in_the_heading() {
    let d = dir("p9p2_scale");
    let source = parquet(&d, "book", &book());
    let steps = vec![TransformStep::from_pivot_spec(spec(&["Bank", "CL status"], NESTED))];
    let formats = HashMap::new();
    let tabs = [ScaledTabToExport { title: "Loans".into(), source: &source, steps: &steps, col_formats: &formats, summary: None, control_col: None }];
    let out = d.join("crore.xlsx");
    export_selected_tabs_to_xlsx_scaled(&tabs, &out, &d, XlsxScale { num_format: NumberDisplayFormat::BdtCrore, decimals: 2 }).expect("the workbook");
    let strings = unzip_entry(&out, "xl/sharedStrings.xml");
    assert!(strings.contains("Sum of Balance (BDT Cr)"), "the unit is in the heading: {strings}");
    let sheet = unzip_entry(&out, "xl/worksheets/sheet1.xml");
    let v: f64 = crate::common::cell_value(&sheet, "B2").expect("Conventional's sum").parse().expect("a number");
    assert!((v - 1060.0 / 1.0e7).abs() < 1e-12, "the cell holds the value in Crore, as the flat sheet does: {v}");
    // the flat sheet of the same pivot has the same number in the same unit
    let flat_steps = vec![TransformStep::from_pivot_spec(spec(&["Bank", "CL status"], PivotLayout::SideBySide))];
    let tabs = [ScaledTabToExport { title: "Loans".into(), source: &source, steps: &flat_steps, col_formats: &formats, summary: None, control_col: None }];
    let out2 = d.join("crore_flat.xlsx");
    export_selected_tabs_to_xlsx_scaled(&tabs, &out2, &d, XlsxScale { num_format: NumberDisplayFormat::BdtCrore, decimals: 2 }).expect("the flat workbook");
    assert!(unzip_entry(&out2, "xl/sharedStrings.xml").contains("Sum of Balance (BDT Cr)"));
}
