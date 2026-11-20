//! Review fixes (Phase 7, Session E): the chart book, the chart-data tab, the colour map. The library-side
//! checks of the same review are in `tests/phase7/r.rs`.

use crate::ui::render_tests_p4::Rig;
use crate::ui::render_tests_p7::{chart_rig_with, pclick};
use crate::ActiveToolModal;
use micro_parquet::chart::{ChartAgg, ChartKind, ChartSpec};
use micro_parquet::charts::colours::Colour;
use micro_parquet::charts::echarts::chart_pair;
use micro_parquet::commands::CommandId;
use polars::prelude::*;

const SIZE: (f32, f32, f32) = (1.05, 1291.0, 700.0);

/// Months by products, in BDT (hundreds of crore, so a second division by Crore would show as a tiny number).
fn bdt() -> DataFrame {
    let (mut m, mut p, mut v) = (Vec::new(), Vec::new(), Vec::new());
    for (i, mo) in ["Jul", "Aug", "Sep", "Oct"].iter().enumerate() {
        for (j, pr) in ["Term deposits", "Savings", "Current"].iter().enumerate() {
            m.push(mo.to_string());
            p.push(pr.to_string());
            v.push((i as f64 + 1.0) * 1_500_000_000.0 + j as f64 * 340_000_000.0);
        }
    }
    DataFrame::new(vec![Series::new("Month".into(), m).into(), Series::new("Product".into(), p).into(), Series::new("Balance".into(), v).into()]).expect("bdt")
}

fn rig(name: &str, df: DataFrame) -> Rig {
    let (ppp, w, h) = SIZE;
    chart_rig_with(name, df, ppp, w, h)
}

fn pump(rig: &mut Rig, what: &str, until: &dyn Fn(&Rig) -> bool) {
    for _ in 0..400 {
        rig.draw(2);
        if until(rig) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    panic!("{what}: timed out");
}

fn chart_ready(r: &Rig) -> bool {
    !r.app.chart.busy && r.app.chart.data.is_some()
}

fn set_spec(rig: &mut Rig, f: &dyn Fn(&mut ChartSpec)) {
    let mut spec = rig.app.chart.spec.clone();
    f(&mut spec);
    rig.app.chart_set_spec(spec);
    pump(rig, "the chart is calculated", &chart_ready);
    rig.draw(4);
}

fn month_chart(rig: &mut Rig) {
    pump(rig, "first chart", &chart_ready);
    set_spec(rig, &|s| {
        s.category = "Month".into();
        s.series = None;
        s.value = Some("Balance".into());
        s.agg = ChartAgg::Sum;
        s.kind = ChartKind::Bar;
        s.order = micro_parquet::sort_lists::SortOrder::List { name: "o".into(), values: ["Jul", "Aug", "Sep", "Oct"].map(String::from).to_vec(), case_insensitive: true };
    });
}

/// The script element of a page, as JSON.
fn page_data(html: &str) -> serde_json::Value {
    let at = html.find("id=\"mp-data\"").expect("the page's data element");
    let from = at + html[at..].find('>').expect("tag end") + 1;
    let to = from + html[from..].find("</script>").expect("script end");
    serde_json::from_str(&html[from..to]).expect("the page data is valid JSON")
}

#[test]
fn r1_a_chart_book_of_two_chart_tabs_writes_their_snapshots_and_nothing_is_calculated_again() {
    let mut rig = rig("r1book", bdt());
    month_chart(&mut rig);
    rig.app.chart_open_as_chart_tab();
    pump(&mut rig, "first chart tab", &|r| r.app.chart_tabs.len() == 1);
    // the second chart is a different one: by product, in its own scale (Million), on the source tab again
    rig.app.active_tab = 0;
    rig.draw(4);
    pump(&mut rig, "the studio is on the source tab", &|r| chart_ready(r) && r.app.chart.tab_id == Some(r.app.tabs[0].id));
    set_spec(&mut rig, &|s| {
        s.category = "Product".into();
        s.order = micro_parquet::sort_lists::SortOrder::Natural;
        s.by_value = true;
        s.scale = micro_parquet::charts::spec::ChartScale::Million;
    });
    rig.app.chart_open_as_chart_tab();
    pump(&mut rig, "second chart tab", &|r| r.app.chart_tabs.len() == 2);
    // the book, with the app in dark mode: it is written light, with each chart's own scale
    rig.app.dark_mode = true;
    let ctx = rig.ctx.clone();
    rig.app.run_command(&ctx, CommandId::ChartBook);
    rig.draw(4);
    assert_eq!(rig.app.active_tool, ActiveToolModal::ChartBook, "Charts > Chart book opens the dialog");
    let infos: Vec<(u64, crate::ui::chart::ChartTabInfo)> = rig.app.chart_tabs.iter().map(|(k, v)| (*k, v.clone())).collect();
    assert_eq!(infos.len(), 2);
    for (id, info) in &infos {
        let title = rig.app.tabs.iter().find(|t| t.id == *id).map(|t| t.title.clone()).expect("the chart tab");
        let snap = rig.app.chart_book.snaps.iter().find(|s| s.name == title).unwrap_or_else(|| panic!("the chart tab '{title}' is listed"));
        assert!(snap.checked, "ticked by default");
        assert_eq!(snap.data, info.data, "the snapshot is the tab's own calculated chart");
        assert_eq!((snap.opts.divisor, snap.opts.decimals, snap.opts.unit.as_str()), (info.divisor, info.decimals, info.unit.as_str()), "and its own scale and decimals");
        assert!(!snap.opts.dark, "dark mode is off in the book");
    }
    assert!(rig.app.chart_book.entries.iter().all(|e| !e.checked), "saved charts are an unticked second section");
    // up and down by real clicks
    let first = rig.app.chart_book.snaps.iter().position(|s| s.name.contains("Month")).expect("the Month chart");
    let before: Vec<String> = rig.app.chart_book.snaps.iter().map(|s| s.name.clone()).collect();
    let last = before.len() - 1;
    let (btn, other) = if first < last { (format!("Down {first}"), first + 1) } else { (format!("Up {first}"), first - 1) };
    let r = rig.app.chart_book.rects.iter().find(|(n, _)| *n == btn).map(|(_, r)| *r).unwrap_or_else(|| panic!("the button {btn}"));
    pclick(&mut rig, r);
    let after: Vec<String> = rig.app.chart_book.snaps.iter().map(|s| s.name.clone()).collect();
    assert_eq!(after[other], before[first], "the chart moved by a click on {btn}");
    assert_eq!(after[first], before[other]);
    // write it
    let dir = rig.app.paths.root_dir.clone();
    let book = dir.join("r1-book.html");
    rig.app.chart_book.path_override = Some(book.clone());
    rig.app.chart_book.title = "Review".into();
    rig.draw(3);
    let w = rig.app.chart_book.rects.iter().find(|(n, _)| n == "Write chart book").map(|(_, r)| *r).expect("Write chart book");
    pclick(&mut rig, w);
    pump(&mut rig, "the book is written", &|_| book.exists());
    let html = std::fs::read_to_string(&book).expect("the book");
    let d = page_data(&html);
    let charts = d["charts"].as_array().expect("charts");
    let ticked: Vec<&crate::ui::chart_studio::BookSnap> = rig.app.chart_book.snaps.iter().filter(|s| s.checked).collect();
    assert_eq!(charts.len(), ticked.len());
    for (k, snap) in ticked.iter().enumerate() {
        let pair = chart_pair(&snap.data, &snap.opts).expect("pair");
        assert_eq!(charts[k]["light"], pair.light, "chart {k} ('{}') in the book is the snapshot, in the book's order", snap.name);
    }
    // the numbers of a chart tab are the ones its own page would have: Month chart in Crore, Product chart in Million
    let month = infos.iter().find(|(_, i)| i.data.spec.category == "Month").expect("month info");
    assert_eq!(month.1.divisor, 10_000_000.0);
    let prod = infos.iter().find(|(_, i)| i.data.spec.category == "Product").expect("product info");
    assert_eq!(prod.1.divisor, 1_000_000.0);
}

#[test]
fn r2_a_chart_made_on_a_chart_data_tab_gives_the_same_numbers_and_never_divides_twice() {
    for plain in [false, true] {
        let mut rig = rig(if plain { "r2plain" } else { "r2crore" }, bdt());
        month_chart(&mut rig);
        if plain {
            set_spec(&mut rig, &|s| s.scale = micro_parquet::charts::spec::ChartScale::Plain);
        }
        let orig = rig.app.chart.data.clone().expect("the first chart");
        let orig_div = rig.app.chart_scale(&rig.app.chart.spec).map(|s| s.divisor).unwrap_or(1.0);
        assert_eq!(orig_div, if plain { 1.0 } else { 10_000_000.0 });
        rig.app.chart_open_as_tab();
        pump(&mut rig, "the data tab", &|r| r.app.tabs.len() == 2 && r.app.tabs[r.app.active_tab].title.starts_with("Chart:"));
        // the tab holds the sums in BDT, unscaled
        let tab = &rig.app.tabs[rig.app.active_tab];
        let col = tab.cached_slice.column("Sum of Balance").expect("the value column").f64().expect("numbers").into_iter().flatten().collect::<Vec<f64>>();
        let want: Vec<f64> = orig.values[0].iter().flatten().copied().collect();
        assert_eq!(col.len(), want.len());
        for (a, b) in col.iter().zip(&want) {
            assert_eq!(a, b, "plain={plain}: the tab stores absolute BDT, never a scaled number");
        }
        // a chart of that tab: the same numbers, shown in the same scale
        rig.draw(4);
        let tab_id = rig.app.tabs[rig.app.active_tab].id;
        pump(&mut rig, "the chart of the data tab", &|r| chart_ready(r) && r.app.chart.tab_id == Some(tab_id));
        let spec = rig.app.chart.spec.clone();
        assert_eq!(spec.value.as_deref(), Some("Sum of Balance"), "the chart adds up the tab's value column");
        let div = rig.app.chart_scale(&spec).map(|s| s.divisor).unwrap_or(1.0);
        assert_eq!(div, orig_div, "plain={plain}: the scale is applied once, on display");
        let new = rig.app.chart.data.clone().expect("data");
        let orig_by: std::collections::HashMap<&String, Option<f64>> = orig.categories.iter().zip(&orig.values[0]).map(|(c, v)| (c, *v)).collect();
        for (c, v) in new.categories.iter().zip(&new.values[0]) {
            assert_eq!(orig_by.get(c).copied().flatten(), *v, "plain={plain}: {c} gives the same number as the original chart");
        }
    }
}

#[test]
fn r3_changing_the_category_column_restarts_the_colours_at_slot_one_and_a_filter_does_not() {
    use micro_parquet::charts::spec::ChartScale;
    let mut rig = rig("r3colours", bdt());
    pump(&mut rig, "first chart", &chart_ready);
    // a donut by product
    set_spec(&mut rig, &|s| {
        s.category = "Product".into();
        s.series = None;
        s.value = Some("Balance".into());
        s.kind = ChartKind::Donut;
        s.by_value = true;
        s.scale = ChartScale::Crore;
    });
    rig.draw(6);
    let first_map = rig.app.chart.spec.colour_map.clone();
    assert_eq!(first_map.slots.len(), 3, "three products have colours: {:?}", first_map.slots);
    // Top N changes what is shown, not the columns: the colours stay
    set_spec(&mut rig, &|s| s.top_n = 2);
    rig.draw(6);
    for (n, slot) in &first_map.slots {
        assert_eq!(rig.app.chart.spec.colour_map.get(n), Some(Colour::Slot(*slot)), "{n} keeps its colour when only Top N changes");
    }
    set_spec(&mut rig, &|s| s.top_n = 30);
    // the category column changes: the old names' slots are gone and the new names start at slot 1
    rig.app.chart_set_spec({
        let mut s = rig.app.chart.spec.clone();
        s.category = "Month".into();
        s
    });
    assert!(rig.app.chart.spec.colour_map.slots.is_empty(), "the colour map is emptied the moment a column changes");
    pump(&mut rig, "the month donut", &chart_ready);
    rig.draw(6);
    let d = rig.app.chart.data.clone().expect("data");
    assert!(d.categories.len() >= 3);
    for (i, name) in d.categories.iter().take(3).enumerate() {
        assert_eq!(rig.app.chart.spec.colour_map.get(name), Some(Colour::Slot(i)), "the {}st/nd/rd name '{}' takes slot {}", i + 1, name, i);
    }
}

#[test]
fn r25_the_data_labels_controls_work_by_real_clicks_at_the_three_sizes() {
    use crate::ui::chart_render::last_shown;
    use crate::ui::render_tests_p7::{scroll_into_view, SIZES3};
    use micro_parquet::charts::layout::Role;
    use micro_parquet::charts::spec::LabelMode;
    for (ppp, w, h) in SIZES3 {
        let mut rig = chart_rig_with("r25labels", bdt(), ppp, w, h);
        let what = rig.what.clone();
        pump(&mut rig, "first chart", &chart_ready);
        set_spec(&mut rig, &|s| {
            s.category = "Month".into();
            s.series = None;
            s.value = Some("Balance".into());
            s.kind = ChartKind::Bar;
        });
        rig.draw(6);
        let screen = rig.ctx.screen_rect();
        let at = egui::pos2(screen.right() - 120.0, screen.center().y);
        let field = |name: &'static str| move |rig: &Rig| rig.app.chart.rects.fields.iter().find(|(n, _)| n == name).map(|(_, r)| *r);
        let value_labels = |rig: &Rig| last_shown(&rig.ctx, egui::Id::new("chart_plot")).expect("the chart").layout.texts.iter().filter(|t| t.role == Role::Label).count();
        let r = scroll_into_view(&mut rig, at, &field("Titles and look"), "Titles and look");
        pclick(&mut rig, r);
        rig.draw(3);
        assert_eq!(value_labels(&rig), 4, "{what}: Auto labels each of the 4 bars");
        // the Data labels drop-down: open it by its real rect, click "Ends only" where the list painted it
        let r = scroll_into_view(&mut rig, at, &field("Data labels"), "Data labels");
        pclick(&mut rig, r);
        let p = rig.draw(3);
        let item = p.boxes.iter().find(|(t, _)| t == "Ends only").map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: the list offers Ends only"));
        pclick(&mut rig, item);
        rig.draw(6);
        assert_eq!(rig.app.chart.spec.labels.mode, LabelMode::EndsOnly, "{what}");
        assert_eq!(value_labels(&rig), 2, "{what}: Ends only labels the first and the last");
        // the same list: None
        let r = scroll_into_view(&mut rig, at, &field("Data labels"), "Data labels");
        pclick(&mut rig, r);
        let p = rig.draw(3);
        let item = p.boxes.iter().find(|(t, _)| t == "None").map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: the list offers None"));
        pclick(&mut rig, item);
        rig.draw(6);
        assert_eq!(rig.app.chart.spec.labels.mode, LabelMode::None, "{what}");
        assert_eq!(value_labels(&rig), 0, "{what}: None prints no value label");
        // decimals: the tick box makes them the user's, and the number is a drag value
        let r = scroll_into_view(&mut rig, at, &field("Label decimals set"), "Label decimals tick");
        pclick(&mut rig, r);
        rig.draw(4);
        assert_eq!(rig.app.chart.spec.labels.decimals, Some(rig.app.global_decimals.min(4)), "{what}: ticking sets the decimals to the view's");
        // percent labels
        let r = scroll_into_view(&mut rig, at, &field("Percent labels"), "Percent labels");
        pclick(&mut rig, r);
        rig.draw(4);
        assert!(!rig.app.chart.spec.labels.percent, "{what}: the Percent labels box was clicked");
        // the unit drop-down
        let r = scroll_into_view(&mut rig, at, &field("Label unit"), "Label unit");
        pclick(&mut rig, r);
        let p = rig.draw(3);
        let item = p.boxes.iter().find(|(t, _)| t == "Lakh").map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: the list offers Lakh"));
        pclick(&mut rig, item);
        rig.draw(4);
        assert_eq!(rig.app.chart.spec.labels.unit, micro_parquet::charts::spec::LabelUnit::Lakh, "{what}");
    }
    micro_parquet::rules::set_data_dir_override(None);
}
