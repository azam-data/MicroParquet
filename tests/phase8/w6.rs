//! W6: drill-down and Show rows. The window's behaviour is tested in `src/ui/render_tests_p8.rs` (`w6_…`); here are the
//! numbers: each level adds up to the value clicked above it, the rows behind a mark are the mark's rows and sum, a date
//! drill on a Datetime column sums to the year, old charts load unchanged, a path that does not fit is refused, "Keep only"
//! and "Exclude" filter the chart, and every export at a level carries the path.

use crate::common::{dir, unzip_entry};
use crate::common8::date_tab;
use micro_parquet::charts::calc::compute;
use micro_parquet::charts::data::ChartData;
use micro_parquet::charts::layout::{subtitle_text, LayoutOpts};
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec, DrillLevel};
use micro_parquet::dates::{CalendarSettings, DateFieldRef, DatePart};
use micro_parquet::engine::filter_by_conditions;
use polars::prelude::*;

/// The synthetic deposit tab: the 1,000 rows of `date_tab` with a Product (five values), a Term (a whole number), a Rate
/// (floats that are whole numbers or not) and a Branch that is blank on every 97th row.
fn deposits() -> DataFrame {
    let mut df = date_tab();
    let n = df.height();
    let branch: Vec<Option<&str>> = (0..n).map(|i| if i % 97 == 0 { None } else { Some(["Dhaka", "Ctg", "Sylhet"][i % 3]) }).collect();
    df.with_column(Series::new("Branch".into(), branch)).expect("branch");
    df.with_column(Series::new("Product".into(), (0..n).map(|i| ["Gold", "Silver", "Classic", "Youth", "Senior"][(i / 3) % 5]).collect::<Vec<_>>())).expect("product");
    df.with_column(Series::new("Term".into(), (0..n).map(|i| (i % 6) as i32 + 1).collect::<Vec<_>>())).expect("term");
    df.with_column(Series::new("Rate".into(), (0..n).map(|i| [8.0, 9.0, 9.5][i % 3]).collect::<Vec<_>>())).expect("rate");
    df
}

fn levels(names: &[&str]) -> Vec<DrillLevel> {
    names.iter().map(|n| DrillLevel::column(n)).collect()
}

/// Sum of Balance by Deposit Type, with Branch and Product below it.
fn spec() -> ChartSpec {
    let mut s = ChartSpec { category: "Deposit Type".into(), value: Some("Balance".into()), agg: ChartAgg::Sum, top_n: 500, ..Default::default() };
    s.set_drill(levels(&["Deposit Type", "Branch", "Product"]));
    s
}

fn chart(df: &DataFrame, spec: &ChartSpec) -> ChartData {
    compute(df.clone().lazy(), spec).unwrap_or_else(|e| panic!("the chart: {e:#}"))
}

fn total(d: &ChartData) -> f64 {
    d.values.iter().flatten().flatten().sum()
}

/// The value and the row count of a category (the first series).
fn mark(d: &ChartData, category: &str) -> (f64, u64) {
    let i = d.categories.iter().position(|c| c == category).unwrap_or_else(|| panic!("'{category}' is a category: {:?}", d.categories));
    (d.values[0][i].unwrap_or(0.0), d.rows[0][i])
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0)
}

/// The rows behind a mark: the tab filtered by the mark's conditions (what the Show rows tab holds).
fn rows_behind(df: &DataFrame, spec: &ChartSpec, category: &str, series: Option<&str>) -> (usize, f64) {
    let conds = spec.mark_conditions(category, series, None).unwrap_or_else(|e| panic!("the conditions of '{category}': {e:#}"));
    let rows = filter_by_conditions(df.clone().lazy(), &conds).and_then(|lf| lf.collect().map_err(Into::into)).unwrap_or_else(|e| panic!("the rows of '{category}': {e:#}"));
    let sum: f64 = rows.column("Balance").expect("Balance").f64().expect("floats").into_iter().flatten().sum();
    (rows.height(), sum)
}

#[test]
fn w6_drill_deposit_type_branch_product_each_level_adds_up_to_the_value_clicked_above() {
    let df = deposits();
    let top = spec();
    let t = chart(&df, &top);
    let all: f64 = df.column("Balance").expect("b").f64().expect("f").into_iter().flatten().sum();
    assert!(near(total(&t), all), "the top level adds up to the whole tab: {} vs {all}", total(&t));
    assert_eq!(top.depth(), 0);
    assert_eq!(t.spec.category, "Deposit Type");
    let (mut checked, mut blank_branch) = (0, 0);
    for dt in t.categories.clone() {
        let (v, n) = mark(&t, &dt);
        let mut l2 = top.clone();
        assert!(l2.drill_down(&dt), "one level down");
        let d2 = chart(&df, &l2);
        assert_eq!(d2.spec.category, "Branch", "the category is the level");
        assert!(near(total(&d2), v), "Branch under {dt} adds up to the {dt} mark: {} vs {v}", total(&d2));
        assert_eq!(d2.rows[0].iter().sum::<u64>(), n, "and its rows");
        for br in d2.categories.clone() {
            let (v3, n3) = mark(&d2, &br);
            let mut l3 = l2.clone();
            assert!(l3.drill_down(&br));
            let d3 = chart(&df, &l3);
            assert_eq!(d3.spec.category, "Product");
            assert!(near(total(&d3), v3), "Product under {dt} › {br} adds up to its mark");
            assert_eq!(d3.rows[0].iter().sum::<u64>(), n3);
            checked += 1;
            blank_branch += (br == "(blank)") as usize;
            // at the last level a click does nothing
            let mut last = l3.clone();
            assert!(!last.drill_down("Gold") && last.depth() == 2, "the last level has no level below");
            assert!(l3.at_last_level() && !l2.at_last_level());
        }
    }
    assert!(checked >= 12 && blank_branch == 4, "every Deposit Type › Branch pair, the blank branch under each of the four types: {checked}, {blank_branch}");
    // back up: the path is cut
    let mut s = top.clone();
    s.drill_down("FDR");
    s.drill_down("Dhaka");
    assert_eq!(s.drill_path, vec![("Deposit Type".to_string(), "FDR".to_string()), ("Branch".to_string(), "Dhaka".to_string())]);
    s.drill_to(1);
    assert_eq!((s.depth(), s.category.as_str()), (1, "Branch"));
    s.drill_to(0);
    assert_eq!((s.depth(), s.category.as_str(), s.filtered_text().as_str()), (0, "Deposit Type", ""));
    // what the chart tab keeps (the calculated spec) calculates again to the same chart
    let mut l2 = top.clone();
    l2.drill_down("FDR");
    let d2 = chart(&df, &l2);
    let again = chart(&df, &d2.spec);
    assert_eq!((&again.categories, &again.values, &again.spec), (&d2.categories, &d2.values, &d2.spec), "a chart tab saved at level 2 calculates to level 2 again");
}

#[test]
fn w6_the_rows_behind_a_mark_are_the_marks_rows_and_sum_at_every_level_for_text_numbers_blanks_and_a_stack() {
    let df = deposits();
    let mut s = spec();
    // level 1, level 2 (with the blank branch), level 3
    for level in 0..3 {
        let d = chart(&df, &s);
        for c in d.categories.clone() {
            let (v, n) = mark(&d, &c);
            let (h, sum) = rows_behind(&df, &s, &c, None);
            assert_eq!((h as u64, near(sum, v)), (n, true), "level {} mark '{c}': {h} rows, sum {sum} vs the mark's {n} rows, {v}", level + 1);
        }
        if level < 2 {
            let first = d.categories.iter().find(|c| *c != "(blank)").cloned().expect("a category");
            assert!(s.drill_down(&first));
        }
    }
    // the title names the path and the mark
    assert_eq!(s.mark_title("Gold", None, None), "Rows: Deposit Type = CD, Branch = Ctg, Product = Gold".replace("CD", &s.drill_path[0].1).replace("Ctg", &s.drill_path[1].1));
    // a number as the category: Term (Int32) and Rate (a float that is whole for 8 and 9, not for 9.5)
    for col in ["Term", "Rate"] {
        let mut n = ChartSpec { category: col.into(), value: Some("Balance".into()), top_n: 500, ..Default::default() };
        n.set_drill(levels(&[col, "Branch"]));
        let d = chart(&df, &n);
        for c in d.categories.clone() {
            let (v, cnt) = mark(&d, &c);
            let (h, sum) = rows_behind(&df, &n, &c, None);
            assert!(h as u64 == cnt && near(sum, v), "{col} = {c}: {h} rows and {sum}, the mark says {cnt} and {v}");
        }
    }
    // a stacked chart: the mark is a (category, series) cell, and its rows are both
    let stacked = ChartSpec { category: "Deposit Type".into(), series: Some("Branch".into()), value: Some("Balance".into()), kind: ChartKind::Bar, top_n: 500, ..Default::default() };
    let d = chart(&df, &stacked);
    assert!(d.series.len() >= 3);
    for (si, ser) in d.series.clone().iter().enumerate() {
        for (ci, cat) in d.categories.iter().enumerate() {
            let (h, sum) = rows_behind(&df, &stacked, cat, Some(ser));
            assert!(h as u64 == d.rows[si][ci] && near(sum, d.values[si][ci].unwrap_or(0.0)), "segment {cat} / {ser}: {h} rows and {sum}");
        }
    }
    // a mark called "Other" is several values: refused, not guessed
    let few = ChartSpec { category: "Product".into(), value: Some("Balance".into()), top_n: 2, ..Default::default() };
    let d = chart(&df, &few);
    assert!(d.categories.iter().any(|c| c == "Other"), "{:?}", d.categories);
    let e = few.mark_conditions("Other", None, None).expect_err("Other is not one value");
    assert!(e.to_string().contains("Top N"), "{e}");
    assert!(few.clone().add_value_filter("Other", true).is_err());
}

#[test]
fn w6_a_date_drill_year_quarter_month_day_on_a_datetime_column_sums_to_the_year() {
    let df = deposits();
    let cal = CalendarSettings::default();
    let part = |p: DatePart| DrillLevel::of_part(&DateFieldRef::new("Open Time", p, cal));
    let mut s = ChartSpec { value: Some("Balance".into()), top_n: 500, ..Default::default() };
    s.set_drill(vec![part(DatePart::Year), part(DatePart::Quarter), part(DatePart::Month), part(DatePart::DayOfMonth)]);
    assert_eq!(s.category, "Open Time \u{b7} Year");
    let years = chart(&df, &s);
    let all: f64 = df.column("Balance").expect("b").f64().expect("f").into_iter().flatten().sum();
    assert!(near(total(&years), all), "the years add up to the tab, the blank dates as '(blank)': {:?}", years.categories);
    assert!(years.categories.iter().any(|c| c == "(blank)"), "{:?}", years.categories);
    assert_eq!(mark(&years, "(blank)").1, 3, "the three blank dates");
    let (mut days_checked, mut rows_checked) = (0, 0);
    for y in years.categories.clone() {
        let (yv, yn) = mark(&years, &y);
        let mut q = s.clone();
        q.drill_down(&y);
        let dq = chart(&df, &q);
        assert!(near(total(&dq), yv) && dq.rows[0].iter().sum::<u64>() == yn, "Quarter under {y}");
        for qq in dq.categories.clone() {
            let (qv, _) = mark(&dq, &qq);
            let mut m = q.clone();
            m.drill_down(&qq);
            let dm = chart(&df, &m);
            assert!(near(total(&dm), qv), "Month under {y} {qq}");
            for mm in dm.categories.clone() {
                let (mv, mn) = mark(&dm, &mm);
                let mut dd = m.clone();
                dd.drill_down(&mm);
                let ddays = chart(&df, &dd);
                assert_eq!(ddays.spec.category, "Open Time \u{b7} Day of month");
                assert!(near(total(&ddays), mv), "Day under {y} {qq} {mm}");
                days_checked += 1;
                // the rows behind a month, through three date parts at once
                let (h, sum) = rows_behind(&df, &m, &mm, None);
                assert!(h as u64 == mn && near(sum, mv), "the rows of {y} {qq} {mm}: {h} and {sum} vs {mn} and {mv}");
                rows_checked += 1;
            }
        }
    }
    assert!(days_checked >= 20 && rows_checked == days_checked, "{days_checked}");
    // the title of the rows of a month
    let mut m = s.clone();
    m.drill_down("2025");
    m.drill_down("Q1");
    assert_eq!(m.mark_title("Feb", None, None), "Rows: Open Time \u{b7} Year = 2025, Open Time \u{b7} Quarter = Q1, Open Time \u{b7} Month = Feb");
}

#[test]
fn w6_a_chart_saved_before_phase_8_loads_and_is_calculated_as_it_was_and_the_kinds_without_a_drill_say_why() {
    let df = deposits();
    let phase7 = r#"{"category": "Deposit Type", "series": null, "value": "Balance", "weight": null, "agg": "Sum", "kind": "Bar", "order": "Natural", "by_value": false, "top_n": 12}"#;
    let old: ChartSpec = serde_json::from_str(phase7).expect("a Phase 7 chart reads");
    assert!(old.drill.is_empty() && old.drill_path.is_empty() && old.filters.is_empty());
    assert!(!old.has_drill() && old.at_last_level() && old.current_level().is_none());
    assert_eq!(old.at_level(), old, "a chart with no drill is left alone");
    assert_eq!(old.filtered_text(), "");
    let d = chart(&df, &old);
    assert_eq!(d.categories.len(), 4);
    assert!(near(total(&d), df.column("Balance").expect("b").f64().expect("f").into_iter().flatten().sum()));
    // the calculation key moves with the level, the path and the filters
    let mut k = old.clone();
    k.set_drill(levels(&["Deposit Type", "Branch"]));
    let base = k.calc_key();
    assert_ne!(old.calc_key(), base);
    let mut k2 = k.clone();
    k2.drill_down("FDR");
    assert_ne!(base, k2.calc_key());
    let mut k3 = old.clone();
    k3.add_value_filter("FDR", true).expect("adds");
    assert_ne!(old.calc_key(), k3.calc_key());
    // a chart saved at level 2 reads back as it was
    let json = serde_json::to_string(&k2).expect("json");
    assert!(json.contains("drill_path"));
    let back: ChartSpec = serde_json::from_str(&json).expect("reads");
    assert_eq!(back, k2);
    // the kinds that cannot drill say why; those that can do not
    for kind in [ChartKind::Bar, ChartKind::Line, ChartKind::Area, ChartKind::Donut, ChartKind::Heatmap] {
        assert!(kind.drill_reason().is_none(), "{kind:?}");
    }
    for kind in [ChartKind::Scatter, ChartKind::Bubble, ChartKind::Histogram, ChartKind::BoxPlot, ChartKind::Waterfall, ChartKind::Sankey, ChartKind::Treemap] {
        let why = kind.drill_reason().unwrap_or_else(|| panic!("{kind:?} has a reason"));
        assert!(why.len() > 20 && why.ends_with('.'), "{kind:?}: {why}");
    }
}

#[test]
fn w6_a_path_that_does_not_fit_the_levels_is_refused_and_nothing_is_guessed() {
    let df = deposits();
    // a path as long as the levels
    let mut s = spec();
    s.drill_path = vec![("Deposit Type".into(), "FDR".into()), ("Branch".into(), "Dhaka".into()), ("Product".into(), "Gold".into())];
    let e = compute(df.clone().lazy(), &s).expect_err("no level left");
    assert!(e.to_string().contains("no level left"), "{e:#}");
    // a path that names another level
    let mut s = spec();
    s.drill_path = vec![("Branch".into(), "Dhaka".into())];
    let e = compute(df.clone().lazy(), &s).expect_err("wrong level");
    assert!(e.to_string().contains("'Branch' at level 1") && e.to_string().contains("'Deposit Type'"), "{e:#}");
    // a path with no levels
    let mut s = ChartSpec { category: "Deposit Type".into(), value: Some("Balance".into()), ..Default::default() };
    s.drill_path = vec![("Deposit Type".into(), "FDR".into())];
    assert!(compute(df.clone().lazy(), &s).is_err());
    // a level that is also the series
    let mut s = spec();
    s.series = Some("Branch".into());
    let e = compute(df.clone().lazy(), &s).expect_err("series is a level");
    assert!(e.to_string().contains("series or the panels"), "{e:#}");
    // a level twice
    let mut s = spec();
    s.set_drill(levels(&["Deposit Type", "Branch", "Branch"]));
    assert!(compute(df.clone().lazy(), &s).is_err());
    // a column that is not there: the chart says so, it does not draw nothing
    let mut s = spec();
    s.set_drill(levels(&["Deposit Type", "Branh"]));
    s.drill_down("FDR");
    assert!(compute(df.clone().lazy(), &s).is_err());
    // a value that is not on the column: no rows, said in words
    let mut s = spec();
    s.drill_down("Fixed");
    let d = compute(df.clone().lazy(), &s);
    assert!(d.is_err() || d.as_ref().is_ok_and(|d| d.categories.is_empty() || total(d) == 0.0), "a value that matches no row never draws numbers");
}

#[test]
fn w6_keep_only_and_exclude_filter_the_chart_and_the_rows_behind_its_marks() {
    let df = deposits();
    let all: f64 = df.column("Balance").expect("b").f64().expect("f").into_iter().flatten().sum();
    let t = chart(&df, &spec());
    let fdr = mark(&t, "FDR");
    let mut keep = spec();
    assert!(keep.add_value_filter("FDR", true).expect("adds"));
    assert!(!keep.add_value_filter("FDR", true).expect("again"), "the same filter is not added twice");
    let d = chart(&df, &keep);
    assert_eq!(d.categories, vec!["FDR".to_string()]);
    assert!(near(total(&d), fdr.0));
    assert_eq!(keep.filtered_text(), "Filtered: Deposit Type = FDR");
    let mut out = spec();
    out.add_value_filter("FDR", false).expect("adds");
    let d = chart(&df, &out);
    assert!(!d.categories.iter().any(|c| c == "FDR") && d.categories.len() == 3);
    assert!(near(total(&d), all - fdr.0));
    assert_eq!(out.filtered_text(), "Filtered: Deposit Type \u{2260} FDR");
    // the blank branch can be kept or left out too
    let mut b = ChartSpec { category: "Branch".into(), value: Some("Balance".into()), top_n: 500, ..Default::default() };
    let before = chart(&df, &b);
    let blank = mark(&before, "(blank)");
    b.add_value_filter("(blank)", false).expect("adds");
    let d = chart(&df, &b);
    assert!(!d.categories.iter().any(|c| c == "(blank)") && near(total(&d), all - blank.0), "{:?}", d.categories);
    // a filter is part of the rows behind a mark: the chart's rows and the tab's agree
    let mut s = spec();
    s.add_value_filter("SB", false).expect("adds");
    s.drill_down("FDR");
    let d = chart(&df, &s);
    for c in d.categories.clone() {
        let (v, n) = mark(&d, &c);
        let (h, sum) = rows_behind(&df, &s, &c, None);
        assert!(h as u64 == n && near(sum, v), "{c}: {h} rows");
    }
    assert_eq!(s.filtered_text(), "Filtered: Deposit Type = FDR, Deposit Type \u{2260} SB");
}

#[test]
fn w6_exports_at_a_level_carry_the_path_in_their_subtitle() {
    let df = deposits();
    let mut s = spec();
    s.drill_down("FDR");
    s.drill_down("Dhaka");
    let d = chart(&df, &s);
    let opts = LayoutOpts::default();
    let want = "Filtered: Deposit Type = FDR, Branch = Dhaka";
    assert!(subtitle_text(&d, &opts).ends_with(want), "{}", subtitle_text(&d, &opts));
    // the HTML page and the chart book
    let html = micro_parquet::charts::echarts::chart_html(&d, &opts).expect("html");
    assert!(html.contains(want), "the HTML page says which rows it is of");
    assert!(html.contains("Product"), "and draws the Product level");
    // the Excel workbook
    let dd = dir("w6_xlsx");
    let xlsx = dd.join("level.xlsx");
    micro_parquet::charts::xlsx::write_chart_xlsx(&xlsx, &d, &opts).expect("xlsx");
    let shared = unzip_entry(&xlsx, "xl/sharedStrings.xml");
    assert!(shared.contains(want), "the workbook's subtitle: {shared}");
    // the Python script
    let py = micro_parquet::charts::python::chart_to_python(&d, &opts).expect("python");
    assert!(py.contains(want), "the script's SUBTITLE");
    // at the top there is nothing to add
    let top = chart(&df, &spec());
    assert!(!subtitle_text(&top, &opts).contains("Filtered"));
    // a typed subtitle keeps its text and gets the path after it
    let mut typed = s.clone();
    typed.subtitle = "Savings book".into();
    let dt = chart(&df, &typed);
    assert_eq!(subtitle_text(&dt, &opts), format!("Savings book \u{b7} {want}"));
}

// ------------------------------------------------------------------------------------------ pivot Show rows

use micro_parquet::models::{PivotAgg, PivotFilter, PivotSpec, PivotValue};
use micro_parquet::pivot_table::{build_pivot, cell_rows};

/// Every value cell of the built pivot gives, through `cell_rows`, the rows that add up to it (and none for an empty cell).
fn every_cell_matches(df: &DataFrame, spec: &PivotSpec) -> (usize, usize) {
    let build = build_pivot(df.clone().lazy(), spec).unwrap_or_else(|e| panic!("the pivot: {e:#}"));
    let names: Vec<String> = build.df.get_column_names().iter().map(|n| n.to_string()).collect();
    let (mut cells, mut empty) = (0, 0);
    for row in 0..build.df.height() {
        for name in names.iter().skip(build.label_columns) {
            let value: Option<f64> = build.df.column(name).expect("column").f64().expect("floats").get(row);
            let cr = cell_rows(spec, &build, row, name).unwrap_or_else(|e| panic!("row {row} '{name}': {e:#}"));
            let rows = filter_by_conditions(df.clone().lazy(), &cr.conditions).and_then(|lf| lf.collect().map_err(Into::into)).unwrap_or_else(|e| panic!("row {row} '{name}': {e:#}"));
            let sum: f64 = rows.column("Balance").expect("Balance").f64().expect("f").into_iter().flatten().sum();
            match value {
                Some(v) => assert!(near(sum, v) && rows.height() > 0, "row {row} '{name}' ({}): the cell says {v}, its {} rows add up to {sum}", cr.title, rows.height()),
                None => {
                    assert_eq!(rows.height(), 0, "row {row} '{name}' ({}) is empty but has {} rows", cr.title, rows.height());
                    empty += 1;
                }
            }
            cells += 1;
        }
    }
    (cells, empty)
}

fn pivot_of(rows: &[&str], columns: &[&str]) -> PivotSpec {
    PivotSpec {
        rows: rows.iter().map(|s| s.to_string()).collect(),
        columns: columns.iter().map(|s| s.to_string()).collect(),
        values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None }],
        row_totals: true,
        column_totals: true,
        subtotals: true,
        ..Default::default()
    }
}

#[test]
fn w6_every_cell_of_a_pivot_totals_and_blank_keys_included_shows_exactly_its_rows() {
    let df = deposits();
    // rows with a subtotal level, columns, the Total column and the Grand total row
    let (cells, _) = every_cell_matches(&df, &pivot_of(&["Branch", "Product"], &["Deposit Type"]));
    assert!(cells > 100, "{cells}");
    // columns only, rows only, and neither fields' blanks
    every_cell_matches(&df, &pivot_of(&["Branch"], &[]));
    every_cell_matches(&df, &pivot_of(&[], &["Product"]));
    // a number field (Term) and a float field (Rate: 8 and 9 are whole, 9.5 is not)
    every_cell_matches(&df, &pivot_of(&["Term"], &["Rate"]));
    // a date part as a Rows field (blank dates are the "(blank)" row) and one as a Columns field
    let cal = CalendarSettings::default();
    let month_year = DateFieldRef::new("Open Date", DatePart::MonthYear, cal);
    let day_name = DateFieldRef::new("Open Time", DatePart::DayName, cal);
    let mut s = pivot_of(&[month_year.name.as_str()], &[day_name.name.as_str()]);
    s.date_fields = vec![month_year.clone(), day_name.clone()];
    let (cells, _) = every_cell_matches(&df, &s);
    assert!(cells > 100, "{cells}");
    // the pivot's own filters are part of every cell's rows
    let mut f = pivot_of(&["Branch"], &["Deposit Type"]);
    f.filters = vec![PivotFilter { field: "Deposit Type".into(), values: vec![Some("FDR".into()), Some("SB".into())] }, PivotFilter { field: "Branch".into(), values: vec![Some("Dhaka".into()), None] }];
    every_cell_matches(&df, &f);
    // a row key is never a guess: the label columns and a column that is not there are refused in words
    let build = build_pivot(df.clone().lazy(), &pivot_of(&["Branch"], &["Deposit Type"])).expect("pivot");
    let e = cell_rows(&pivot_of(&["Branch"], &["Deposit Type"]), &build, 0, "Branch").expect_err("a label");
    assert!(e.to_string().contains("value cells"), "{e}");
    assert!(cell_rows(&pivot_of(&["Branch"], &["Deposit Type"]), &build, 0, "Nope").is_err());
    assert!(cell_rows(&pivot_of(&["Branch"], &["Deposit Type"]), &build, 9999, "Total").is_err());
}

#[test]
fn w6_the_title_of_a_pivot_cell_names_its_fields_and_a_total_says_all_rows() {
    let df = deposits();
    let spec = pivot_of(&["Branch"], &["Deposit Type"]);
    let build = build_pivot(df.lazy(), &spec).expect("pivot");
    let names: Vec<String> = build.df.get_column_names().iter().map(|n| n.to_string()).collect();
    let branch_at = |label: &str| (0..build.df.height()).find(|r| build.df.column("Branch").expect("Branch").str().expect("text").get(*r) == Some(label)).unwrap_or_else(|| panic!("{label}"));
    let dhaka = branch_at("Dhaka");
    let fdr = names.iter().find(|n| n.as_str() == "FDR").expect("an FDR column").clone();
    assert_eq!(cell_rows(&spec, &build, dhaka, &fdr).expect("cell").title, "Rows: Branch = Dhaka, Deposit Type = FDR");
    // the Total column: the row's fields only; the Grand total row: the column's only; both: all rows
    assert_eq!(cell_rows(&spec, &build, dhaka, "Total").expect("total").title, "Rows: Branch = Dhaka");
    let grand = branch_at("Grand total");
    assert_eq!(cell_rows(&spec, &build, grand, &fdr).expect("col total").title, "Rows: Deposit Type = FDR");
    assert_eq!(cell_rows(&spec, &build, grand, "Total").expect("grand").title, "Rows: all rows of the pivot");
    // a blank key reads "(blank)"
    let blank = branch_at("(blank)");
    assert_eq!(cell_rows(&spec, &build, blank, "Total").expect("blank").title, "Rows: Branch = (blank)");
}

#[test]
fn w6_the_chart_book_and_the_shared_layout_of_the_app_and_the_pdf_carry_the_path() {
    use micro_parquet::charts::echarts::{chart_book_html, BookItem};
    use micro_parquet::charts::layout::{hit_target, layout_chart, ApproxMeasure};
    let df = deposits();
    let mut s = spec();
    s.drill_down("SB");
    let d = chart(&df, &s);
    let opts = LayoutOpts::default();
    let top = chart(&df, &spec());
    // the chart book: the drilled chart says so, the other one in it does not
    let html = chart_book_html("Deposits", "23 Nov 2026", &[BookItem { data: &d, opts: opts.clone() }, BookItem { data: &top, opts: opts.clone() }]).expect("the book");
    let alone = micro_parquet::charts::echarts::chart_html(&d, &opts).expect("the page").matches("Filtered: Deposit Type = SB").count();
    assert!(alone >= 1, "the drilled chart's own page says the path");
    assert_eq!(html.matches("Filtered: Deposit Type = SB").count(), alone, "in the book only the drilled chart says it, as many times as on its own page");
    assert_eq!(html.matches("Filtered: Deposit Type").count(), alone, "and the chart at the top says nothing of the kind");
    // the layout the in-app chart and the PDF both draw from: its subtitle text
    let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(1000.0, 640.0));
    let l = layout_chart(&d, rect, &opts, &ApproxMeasure);
    assert!(l.texts.iter().any(|t| t.text.ends_with("Filtered: Deposit Type = SB")), "{:?}", l.texts.iter().map(|t| t.text.clone()).take(8).collect::<Vec<_>>());
    let lt = layout_chart(&top, rect, &opts, &ApproxMeasure);
    assert!(!lt.texts.iter().any(|t| t.text.contains("Filtered")));
    // a click on a mark names the category that was clicked, by its index in the layout
    let mark = l.marks.iter().find(|m| m.category == 1).expect("a mark for the second category");
    let hit = hit_target(&l, mark.rect.center()).expect("a mark is under its own centre");
    assert_eq!(d.mark_ref(hit.panel, hit.series, hit.category).map(|m| m.category), Some(d.categories[1].clone()));
    // a place with no mark is no target
    assert!(hit_target(&l, egui::pos2(0.0, 0.0)).is_none());
}

// ---- the owner's follow-up on C1: Show rows of a pivot that steps follow ----

use micro_parquet::models::{CondOp, FilterCondition, GroupJoin, IngestionConfig, SortLevel, TabSource, TransformStep};
use micro_parquet::pivot_table::{pivot_rows_of, PivotRows};
use micro_parquet::tab_jobs::{show_rows_job, JobOutput, ShowRowsRequest, ShowRowsResult};

fn deposits_source(name: &str) -> (TabSource, std::path::PathBuf) {
    let d = dir(name);
    let path = d.join("deposits.parquet");
    let mut df = deposits();
    ParquetWriter::new(std::fs::File::create(&path).expect("file")).finish(&mut df).expect("parquet");
    (TabSource::SingleFile { path, is_parquet: true, config: IngestionConfig::default() }, d.join("temp"))
}

fn show(source: &TabSource, temp: &std::path::Path, steps: &[TransformStep], row: usize, column: &str, expected_rows: usize) -> anyhow::Result<ShowRowsResult> {
    let PivotRows::Readable { at, spec, later_made } = pivot_rows_of(steps, steps.len()) else { anyhow::bail!("not readable: {:?}", pivot_rows_of(steps, steps.len())) };
    let req = ShowRowsRequest { base: source.clone(), steps: steps.to_vec(), pivot_at: at, spec, later_made, row, column: column.to_string(), expected_rows, temp_dir: temp.to_path_buf() };
    match show_rows_job(req, &micro_parquet::jobs::JobCtx::inert())? {
        JobOutput::ShowRows(r) => Ok(*r),
        _ => anyhow::bail!("not the rows of a cell"),
    }
}

fn cell_text(df: &DataFrame, name: &str, row: usize) -> String {
    match df.column(name).expect("column").get(row).expect("cell") {
        AnyValue::Null => String::new(),
        AnyValue::String(s) => s.to_string(),
        AnyValue::StringOwned(s) => s.to_string(),
        v => format!("{v}"),
    }
}

fn rows_of(source: &TabSource, temp: &std::path::Path, r: &ShowRowsResult) -> (String, usize, f64) {
    let df = micro_parquet::engine::compile_tab_lazy(source, &r.steps, temp).and_then(|lf| lf.collect().map_err(Into::into)).expect("the rows tab");
    let sum: f64 = df.column("Balance").expect("Balance").f64().expect("f").into_iter().flatten().sum();
    (r.title.clone(), df.height(), sum)
}

#[test]
fn w6_a_pivot_followed_by_a_sort_and_a_filter_gives_the_same_rows_for_a_cell() {
    let (source, temp) = deposits_source("w6_pivot_later");
    let pivot = TransformStep::from_pivot_spec(pivot_of(&["Branch", "Product"], &["Deposit Type"]));
    let sort = TransformStep::SortRows { levels: vec![SortLevel { column: "Total".into(), descending: true, ..Default::default() }] };
    let filter = TransformStep::FilterGroup { conditions: vec![FilterCondition { column: "Branch".into(), op: CondOp::NotEquals, value: "Ctg".into(), ..Default::default() }], join: GroupJoin::And, keep_matching: true };
    let math = TransformStep::MathFormula { new_col: "Half".into(), left_col: "Total".into(), op: micro_parquet::models::MathOp::Div, right_is_col: false, right_val_or_col: "2".into() };
    let plain = vec![pivot.clone()];
    let later = vec![pivot.clone(), sort, filter, math];
    let plain_df = micro_parquet::engine::compile_tab_lazy(&source, &plain, &temp).and_then(|lf| lf.collect().map_err(Into::into)).expect("the pivot");
    let later_df = micro_parquet::engine::compile_tab_lazy(&source, &later, &temp).and_then(|lf| lf.collect().map_err(Into::into)).expect("the pivot, sorted and filtered");
    assert!(later_df.height() > 0 && later_df.height() < plain_df.height(), "the filter removed rows: {} of {}", later_df.height(), plain_df.height());
    // the rows come in another order
    assert_ne!((0..later_df.height()).map(|r| cell_text(&later_df, "Product", r)).collect::<Vec<_>>(), (0..later_df.height()).map(|r| cell_text(&plain_df, "Product", r)).collect::<Vec<_>>());
    let names: Vec<String> = later_df.get_column_names().iter().map(|n| n.to_string()).collect();
    let mut checked = 0;
    for r in 0..later_df.height() {
        let (b, p) = (cell_text(&later_df, "Branch", r), cell_text(&later_df, "Product", r));
        let plain_row = (0..plain_df.height()).find(|i| cell_text(&plain_df, "Branch", *i) == b && cell_text(&plain_df, "Product", *i) == p).expect("the same row in the plain pivot");
        for name in names.iter().skip(2).filter(|n| n.as_str() != "Half") {
            let want = show(&source, &temp, &plain, plain_row, name, plain_df.height()).unwrap_or_else(|e| panic!("plain: {e:#}"));
            let got = show(&source, &temp, &later, r, name, later_df.height()).unwrap_or_else(|e| panic!("row {r} '{name}': {e:#}"));
            assert_eq!(serde_json::to_string(&got.steps).expect("json"), serde_json::to_string(&want.steps).expect("json"), "row {r} '{name}'");
            assert_eq!(rows_of(&source, &temp, &got), rows_of(&source, &temp, &want), "row {r} '{name}'");
            checked += 1;
        }
    }
    assert!(checked > 50, "{checked}");
    // a column that a later step made has no rows of its own, and it is said in words
    let e = show(&source, &temp, &later, 0, "Half", later_df.height()).err().expect("a calculated column");
    assert!(e.to_string().contains("'Half' was added"), "{e}");
    // a table that changed since it was drawn is refused
    assert!(show(&source, &temp, &later, 0, "Total", later_df.height() + 1).is_err());
}

#[test]
fn w6_a_pivot_followed_by_a_rename_of_a_row_field_or_a_new_grouping_has_no_show_rows_and_says_why() {
    let pivot = TransformStep::from_pivot_spec(pivot_of(&["Branch", "Product"], &["Deposit Type"]));
    let read = |later: Vec<TransformStep>| {
        let mut steps = vec![pivot.clone()];
        steps.extend(later);
        pivot_rows_of(&steps, steps.len())
    };
    let off = |later: Vec<TransformStep>| match read(later) {
        PivotRows::Unavailable(why) => why,
        other => panic!("expected a reason, got {other:?}"),
    };
    let why = off(vec![TransformStep::RenameColumn { old_name: "Branch".into(), new_name: "Office".into() }]);
    assert!(why.contains("renames 'Branch', a row field of the pivot, to 'Office'") && why.starts_with("Show rows is off:"), "{why}");
    assert!(off(vec![TransformStep::SelectAndReorderColumns { cols: vec!["Product".into(), "Total".into()] }]).contains("leaves out 'Branch'"));
    assert!(off(vec![TransformStep::ConvertToNumber { columns: vec!["Product".into()] }]).contains("sets 'Product'"));
    assert!(off(vec![TransformStep::GroupByPivot { group_cols: vec!["Branch".into()], pivot_col: None, aggs: vec![], sort_by_col: None, top_n: None }]).contains("groups or pivots the table again"));
    // a second pivot is the tab's pivot: the cells on the screen are its cells
    assert!(matches!(read(vec![pivot.clone()]), PivotRows::Readable { at: 1, .. }));
    assert!(off(vec![TransformStep::SqlQuery { query: "select * from t".into() }]).contains("makes a new table"));
    // the steps that keep the row fields leave it on; a rename of another column, a hidden step and a step that is off too
    for later in [
        vec![TransformStep::SortRows { levels: vec![SortLevel { column: "Total".into(), ..Default::default() }] }],
        vec![TransformStep::RenameColumn { old_name: "Total".into(), new_name: "All".into() }],
        vec![TransformStep::SelectAndReorderColumns { cols: vec!["Branch".into(), "Product".into(), "Total".into()] }],
        vec![TransformStep::Skipped { step: Box::new(TransformStep::RenameColumn { old_name: "Branch".into(), new_name: "Office".into() }) }],
    ] {
        assert!(matches!(read(later.clone()), PivotRows::Readable { .. }), "{later:?}");
    }
    // a rename of a value column is read, and the column it made is listed as one that has no rows of its own
    let PivotRows::Readable { later_made, .. } = read(vec![TransformStep::RenameColumn { old_name: "Total".into(), new_name: "All".into() }]) else { panic!("readable") };
    assert_eq!(later_made, vec!["All".to_string()]);
    // a tab with no pivot has nothing to show
    assert_eq!(pivot_rows_of(&[], 0), PivotRows::NotAPivot);
}
