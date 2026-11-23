//! W7: the colour picker and house colours. The window (the picker, its hex field, the tick box, View ▸ House colours…) is
//! tested in `src/ui/render_tests_p8.rs` (`w7_…`); here are the colours: a pick is drawn in all five renderers, a house
//! colour is the second choice after the chart's own, Reset keeps it, the three warnings fire on known pairs, and a filter or
//! Top N never repaints a picked item.

use crate::p7_c4::{inspect, tmp};
use micro_parquet::charts::calc::compute;
use micro_parquet::charts::colours::{contrast_ratio, delta_e100, house, pick_warnings, Colour, ColourMap, ColourPick, HexColour};
use micro_parquet::charts::data::ChartData;
use micro_parquet::charts::echarts::{chart_book_html, chart_pair, BookItem};
use micro_parquet::charts::layout::{layout_chart, ApproxMeasure, LayoutOpts, MarkKind, Prim};
use micro_parquet::charts::python::chart_to_python;
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec};
use micro_parquet::charts::tokens as t;
use micro_parquet::charts::xlsx::write_chart_xlsx;
use polars::prelude::*;

fn python() -> bool {
    std::process::Command::new("python3").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn opts(dark: bool) -> LayoutOpts {
    LayoutOpts { dark, divisor: 1.0, decimals: 2, unit: "BDT Crore".into(), bare: false, colours: None }
}

/// Balance by Month, one series per product: TD, Savings and Current.
fn products() -> DataFrame {
    let (mut m, mut p, mut b) = (Vec::new(), Vec::new(), Vec::new());
    for (month, vals) in [("Jan", [500.0, 300.0, 200.0]), ("Feb", [540.0, 305.0, 198.0]), ("Mar", [600.0, 310.0, 195.0])] {
        for (i, pr) in ["TD", "Savings", "Current"].iter().enumerate() {
            m.push(month);
            p.push(*pr);
            b.push(vals[i]);
        }
    }
    DataFrame::new(vec![Series::new("Month".into(), m), Series::new("Product".into(), p), Series::new("Balance".into(), b)]).expect("products")
}

fn spec() -> ChartSpec {
    ChartSpec { category: "Month".into(), series: Some("Product".into()), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 500, ..Default::default() }
}

fn chart(spec: &ChartSpec) -> ChartData {
    compute(products().lazy(), spec).expect("the chart")
}

/// The fill of the first bar of the series `name`, as the app lays it out.
fn bar_fill(d: &ChartData, name: &str, dark: bool) -> u32 {
    let l = layout_chart(d, egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(1200.0, 700.0)), &opts(dark), &ApproxMeasure);
    let si = d.series.iter().position(|s| s == name).unwrap_or_else(|| panic!("series {name}: {:?}", d.series));
    let m = l.marks.iter().find(|m| m.series == si && m.kind == MarkKind::Bar).expect("a bar of the series");
    l.prims
        .iter()
        .find_map(|p| if let Prim::Rect { rect, fill, .. } = p { (*rect == m.rect).then_some(((fill.r() as u32) << 16) | ((fill.g() as u32) << 8) | fill.b() as u32) } else { None })
        .expect("the bar's fill")
}

const DEEP_GREEN: u32 = 0x037952;

#[test]
fn w7_the_swatches_are_the_24_of_the_spec_and_a_base_pick_is_the_palette_slot() {
    assert_eq!(t::HUE_NAMES, ["Blue", "Orange", "Green", "Amber", "Pink", "Dark green", "Violet", "Red"]);
    assert_eq!(t::HUE_LIGHT, [0x6ea5ee, 0xffa889, 0x7dd6ac, 0xffd7a0, 0xffb9d0, 0x60a95a, 0x8a86eb, 0xfe8c85]);
    assert_eq!(t::HUE_DEEP, [0x014a94, 0xac3c01, 0x037952, 0xad7502, 0xac5074, 0x024e02, 0x3a2196, 0xa31921]);
    for hue in 0..8 {
        // Light and Deep are fixed colours, the same in both themes; Base is the slot, with its own dark pair
        let (light, base, deep) = (ColourPick::swatch(hue, 0).colour(), ColourPick::swatch(hue, 1).colour(), ColourPick::swatch(hue, 2).colour());
        assert_eq!((light.hex(false), light.hex(true)), (t::HUE_LIGHT[hue], t::HUE_LIGHT[hue]));
        assert_eq!((deep.hex(false), deep.hex(true)), (t::HUE_DEEP[hue], t::HUE_DEEP[hue]));
        assert_eq!(base, Colour::Slot(hue));
        assert_eq!((base.hex(false), base.hex(true)), (t::SERIES_LIGHT[hue], t::SERIES_DARK[hue]));
    }
    // a hex is read as #rrggbb in any case, with or without the #, and anything else is refused in words
    assert_eq!(HexColour::parse("#037952"), Ok(HexColour(0x037952)));
    assert_eq!(HexColour::parse(" 03795A "), Ok(HexColour(0x03795a)));
    for bad in ["", "#03795", "#0379522", "#gg7952", "green"] {
        let e = HexColour::parse(bad).expect_err("refused");
        assert!(e.contains("Write it as #rrggbb"), "{bad}: {e}");
    }
}

#[test]
fn w7_a_pick_of_deep_green_draws_the_item_in_037952_in_all_five_renderers_and_a_base_pick_follows_the_theme() {
    let mut s = spec();
    s.colour_map.assign_all(&["TD", "Savings", "Current"]);
    let plain = chart(&s);
    let (td_light, td_dark) = (bar_fill(&plain, "TD", false), bar_fill(&plain, "TD", true));
    assert_eq!((td_light, td_dark), (t::SERIES_LIGHT[0], t::SERIES_DARK[0]));
    let savings = (bar_fill(&plain, "Savings", false), bar_fill(&plain, "Savings", true));

    // Deep green: #037952 in both themes, and no other item moves
    s.colour_map.set_pick("TD", ColourPick::swatch(2, 2));
    let d = chart(&s);
    assert_eq!((bar_fill(&d, "TD", false), bar_fill(&d, "TD", true)), (DEEP_GREEN, DEEP_GREEN), "the app");
    assert_eq!((bar_fill(&d, "Savings", false), bar_fill(&d, "Savings", true)), savings, "the others keep their colours");
    // the page: the series of TD in both the light and the dark chart
    let pair = chart_pair(&d, &opts(false)).expect("html");
    let series_colour = |v: &serde_json::Value, name: &str| -> String {
        v["series"].as_array().expect("series").iter().find(|x| x["name"] == name).map(|x| x["itemStyle"]["color"].as_str().unwrap_or("").to_string()).unwrap_or_default()
    };
    assert_eq!((series_colour(&pair.light, "TD"), series_colour(&pair.dark, "TD")), (t::hex_text(DEEP_GREEN), t::hex_text(DEEP_GREEN)), "html");
    assert_eq!(series_colour(&pair.light, "Savings"), t::hex_text(savings.0), "html: the others keep theirs");
    // the script: the same hex for light and dark
    let py = chart_to_python(&d, &opts(false)).expect("python");
    assert!(py.contains("'TD': '#037952' if not DARK else '#037952'") || py.contains("\"TD\": \"#037952\" if not DARK else \"#037952\""), "python: {}", py.lines().filter(|l| l.contains("DARK")).take(3).collect::<Vec<_>>().join("\n"));
    // the workbook
    if python() {
        let p = tmp("w7-deep.xlsx");
        write_chart_xlsx(&p, &d, &opts(false)).expect("xlsx");
        let j = inspect(&p);
        let fills: Vec<(String, String)> = j["charts"][0]["plots"][0]["series"].as_array().expect("series").iter().map(|s| (s["name"].as_str().unwrap_or("").to_string(), s["fill"].as_str().unwrap_or("").to_uppercase())).collect();
        assert!(fills.iter().any(|(n, f)| n == "TD" && *f == "037952"), "excel: {fills:?}");
    }
    // the PDF page: the fill operator of the colour is in the page
    {
        use micro_parquet::exporters::{export_pdf_tabs_with, PdfOptions, PdfTab};
        let table = micro_parquet::chart::chart_table(&d).expect("table");
        let path = tmp("w7-deep.pdf");
        let tab = PdfTab { title: "Chart".into(), df: table, col_formats: Default::default(), summary: vec![], chart: Some((d.clone(), opts(false))) };
        let o = PdfOptions { title: "pack".into(), page: micro_parquet::pdf_layout::PdfPage { landscape: true, legal: false }, chart_tables: false };
        export_pdf_tabs_with(vec![tab], &path, micro_parquet::models::NumberDisplayFormat::BdtCrore, 2, &o).expect("pdf");
        let bytes = std::fs::read(&path).expect("pdf bytes");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("0.0117") && text.contains("0.4745") && text.contains("0.3215"), "pdf: the colour #037952 (0.0117 0.4745 0.3216) is in the page");
    }
    // a chart book carries it
    let book = chart_book_html("Book", "2026-11-23", &[BookItem { data: &d, opts: opts(false) }]).expect("book");
    assert!(book.contains("#037952"), "chart book");

    // Base pick (Amber is slot 4): the item follows the theme, with the validated dark pair
    s.colour_map.set_pick("TD", ColourPick::swatch(3, 1));
    let d = chart(&s);
    assert_eq!((bar_fill(&d, "TD", false), bar_fill(&d, "TD", true)), (t::SERIES_LIGHT[3], t::SERIES_DARK[3]), "the app follows the theme");
    let pair = chart_pair(&d, &opts(false)).expect("html");
    assert_eq!((series_colour(&pair.light, "TD"), series_colour(&pair.dark, "TD")), (t::hex_text(t::SERIES_LIGHT[3]), t::hex_text(t::SERIES_DARK[3])), "html follows the theme");
    // Automatic again: the slot the name had
    s.colour_map.clear_pick("TD");
    let d = chart(&s);
    assert_eq!((bar_fill(&d, "TD", false), bar_fill(&d, "TD", true)), (td_light, td_dark));
}

#[test]
fn w7_a_house_colour_colours_a_new_chart_and_a_template_and_the_charts_own_pick_overrides_it() {
    let root = crate::common::dir("w7_house");
    micro_parquet::rules::set_data_dir_override(Some(root.clone()));
    // no file: the palette
    assert_eq!(house::list(), vec![]);
    let mut fresh = spec();
    fresh.colour_map.assign_all(&["TD", "Savings", "Current"]);
    let d0 = chart(&fresh);
    let auto = bar_fill(&d0, "TD", false);
    assert_eq!(auto, t::SERIES_LIGHT[0]);

    // the house says TD is Deep green; the name matches trimmed and in any case
    house::set(" td ", ColourPick::swatch(2, 2)).expect("saved");
    assert!(root.join("house_colours.json").exists());
    let d = chart(&ChartSpec { colour_map: ColourMap::default(), ..spec() });
    assert_eq!((bar_fill(&d, "TD", false), bar_fill(&d, "TD", true)), (DEEP_GREEN, DEEP_GREEN), "a new chart");
    // a template: Deposit / loan mix over a table with TD
    let (mix_spec, mix) = {
        let mut df = micro_parquet::charts::samples::mix();
        let names: Vec<&str> = df.column("Product").expect("Product").str().expect("text").into_iter().map(|v| if v == Some("Term deposit") { "TD" } else { v.unwrap_or("") }).collect();
        df.with_column(Series::new("Product".into(), names)).expect("renamed");
        let tmpl = micro_parquet::charts::templates::prepare(micro_parquet::charts::templates::Template::DepositMix, &df, &micro_parquet::charts::templates::Ctx { divisor: 1.0, suffix: "Cr".into(), ..Default::default() }).expect("the template");
        let d = compute(tmpl.table.clone().expect("table").lazy(), &tmpl.spec).expect("the template's chart");
        (tmpl.spec, d)
    };
    assert_eq!(mix_spec.colour_map.get("TD"), Some(Colour::Fixed(DEEP_GREEN)), "a template: its own slot for TD is outranked");
    assert_ne!(mix_spec.colour_map.get("Savings"), Some(Colour::Fixed(DEEP_GREEN)));
    let l = layout_chart(&mix, egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(1200.0, 700.0)), &opts(false), &ApproxMeasure);
    let green = egui::Color32::from_rgb(3, 121, 82);
    assert!(format!("{:?}", l.prims).contains(&format!("{:?}", green)), "a template's chart draws TD in the house colour");
    assert!(chart_pair(&mix, &opts(false)).expect("html").light.to_string().contains("#037952"), "and so does its page");
    // a chart that was made before the house colour: its stored slot for TD is outranked
    assert_eq!(bar_fill(&chart(&fresh), "TD", false), DEEP_GREEN, "a chart whose map has a slot for TD");
    // the chart's own pick comes first
    let mut own = fresh.clone();
    own.colour_map.set_pick("TD", ColourPick::swatch(6, 2));
    assert_eq!(bar_fill(&chart(&own), "TD", false), t::HUE_DEEP[6], "the chart's own pick overrides the house colour");
    // Reset colours (a new map with the names in order) clears the chart's picks and keeps the house colour
    let mut reset = ColourMap::default();
    reset.assign_all(&["TD", "Savings", "Current"]);
    assert!(reset.custom.is_empty());
    assert_eq!(reset.get("TD"), Some(Colour::Fixed(DEEP_GREEN)), "the house colour stays after a reset");
    assert_eq!(reset.get("Savings"), Some(Colour::Slot(0)), "and the others take the slots from the first");
    // a chart book and the chart's HTML carry it
    let html = chart_pair(&chart(&fresh), &opts(false)).expect("html").light.to_string();
    assert!(html.contains("#037952"));
    // removing it gives the palette back
    house::remove("TD").expect("removed");
    assert_eq!(bar_fill(&chart(&fresh), "TD", false), auto);

    // a file that is broken is refused with its line, and the charts use the palette
    std::fs::write(root.join("house_colours.json"), "[\n  { \"name\": \"TD\", \"pick\": { \"kind\": \"hex\", \"hex\": \"#03795\" } }\n]\n").expect("write");
    house::invalidate();
    let e = house::error().expect("refused");
    assert!(e.contains("line 2") && e.contains("is not a colour") && e.contains("house_colours.json"), "{e}");
    assert_eq!(bar_fill(&chart(&fresh), "TD", false), auto, "a broken file means the palette");
    for (text, why) in [("[{\"name\": \"\", \"pick\": {\"kind\": \"slot\", \"slot\": 1}}]", "without a name"), ("[{\"name\": \"TD\", \"pick\": {\"kind\": \"slot\", \"slot\": 9}}]", "slot 10"), ("[{\"name\": \"TD\", \"pick\": {\"kind\": \"slot\", \"slot\": 1}}, {\"name\": \"td\", \"pick\": {\"kind\": \"slot\", \"slot\": 2}}]", "twice")] {
        std::fs::write(root.join("house_colours.json"), text).expect("write");
        let e = house::load_from(&root.join("house_colours.json")).expect_err("refused");
        assert!(e.contains(why), "{why}: {e}");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn w7_the_three_warnings_fire_on_known_pairs_and_not_on_the_default_palette() {
    let names: Vec<String> = ["TD", "Savings", "Current", "DPS", "FDR", "Call", "FCY", "Sundry"].iter().map(|s| s.to_string()).collect();
    // the default palette: automatic items have no pick, so a chart of eight has no colour note
    // a donut colours each of its parts, so its names are the categories
    let mut s = ChartSpec { category: "Product".into(), value: Some("Balance".into()), kind: ChartKind::Donut, ..Default::default() };
    s.colour_map.assign_all(&names);
    let df = DataFrame::new(vec![Series::new("Product".into(), names.clone()), Series::new("Balance".into(), (1..=8).map(|i| i as f64 * 10.0).collect::<Vec<_>>())]).expect("df");
    let d = compute(df.lazy(), &s).expect("chart");
    assert!(micro_parquet::charts::colours::colour_notes(&d, &s.colour_map).is_empty(), "the default palette has no notes");
    // every Base pick of a hue in a chart whose other items are the other seven slots: no contrast or status warning, only
    // the hard-to-tell of two slots that are close
    let mut pairs = 0;
    for hue in 0..8 {
        let others: Vec<(String, Colour)> = (0..8).filter(|i| *i != hue).map(|i| (names[i].clone(), Colour::Slot(i))).collect();
        let w = pick_warnings(&ColourPick::swatch(hue, 1), &others);
        assert!(w.iter().all(|x| x.starts_with("Hard to tell from '")), "Base {}: {w:?}", t::HUE_NAMES[hue]);
        pairs += w.len();
    }
    // (orange and red are 7.1 apart, and a few more pairs are close in one theme: that is the palette's own, and it is said)
    assert!(pairs >= 2, "{pairs}");

    // 1. hard to tell: a pick one step from another item's colour
    let savings = vec![("Savings".to_string(), Colour::Slot(1))];
    let near = ColourPick::Hex { hex: HexColour(t::SERIES_LIGHT[1] + 0x000102) };
    assert!(delta_e100(Colour::Slot(1).hex(false), near.colour().hex(false)) < 1.0);
    assert!(pick_warnings(&near, &savings).iter().any(|w| w == "Hard to tell from 'Savings'"));
    let far = ColourPick::swatch(0, 1);
    assert!(pick_warnings(&far, &savings).is_empty(), "blue and orange are easy to tell apart");
    // 2. low contrast: Deep blue on the dark background (2.0:1), and Light amber on the light one (1.3:1)
    let deep_blue = ColourPick::swatch(0, 2);
    assert!(contrast_ratio(t::HUE_DEEP[0], t::INK_DARK.surface) < 3.0);
    let w = pick_warnings(&deep_blue, &[]);
    assert_eq!(w, vec!["Low contrast on the dark background".to_string()]);
    let w = pick_warnings(&ColourPick::swatch(3, 0), &[]);
    assert_eq!(w, vec!["Low contrast on the light background".to_string()]);
    assert!(pick_warnings(&ColourPick::swatch(2, 2), &[]).is_empty(), "Deep green is fine in both themes: {:?}", pick_warnings(&ColourPick::swatch(2, 2), &[]));
    // 3. a status colour: within ΔE 5 of good, warning, serious or critical
    for (name, rgb) in t::STATUS_COLOURS {
        let w = pick_warnings(&ColourPick::Hex { hex: HexColour(rgb) }, &[]);
        assert!(w.iter().any(|x| x.starts_with("This is a status colour") && x.contains(name)), "{name}: {w:?}");
    }
    // the warnings are in the chart's notes, for the item that has the pick, and in every renderer's footnote
    s.colour_map.set_pick("DPS", ColourPick::swatch(0, 2));
    let notes = micro_parquet::charts::colours::colour_notes(&d, &s.colour_map);
    assert_eq!(notes, vec!["Colour of 'DPS': low contrast on the dark background.".to_string()]);
    let d = compute(DataFrame::new(vec![Series::new("Product".into(), names.clone()), Series::new("Balance".into(), (1..=8).map(|i| i as f64 * 10.0).collect::<Vec<_>>())]).expect("df").lazy(), &s).expect("chart");
    let l = layout_chart(&d, egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(1200.0, 700.0)), &opts(false), &ApproxMeasure);
    assert!(l.texts.iter().any(|x| x.text.contains("low contrast on the dark background")), "the app's footnote");
    assert!(chart_pair(&d, &opts(false)).expect("html").light.to_string().contains("low contrast on the dark background"), "the page's footnote");
}

#[test]
fn w7_a_filter_or_top_n_never_repaints_a_picked_item_and_old_charts_are_written_as_they_were() {
    let mut s = spec();
    s.colour_map.assign_all(&["TD", "Savings", "Current"]);
    s.colour_map.set_pick("Savings", ColourPick::swatch(6, 2));
    let all = chart(&s);
    let picked = bar_fill(&all, "Savings", false);
    assert_eq!(picked, t::HUE_DEEP[6]);
    // a filter that leaves out TD
    let mut f = s.clone();
    f.filters = vec![micro_parquet::models::FilterCondition { column: "Product".into(), op: micro_parquet::models::CondOp::NotEquals, value: "TD".into(), ..Default::default() }];
    let filtered = micro_parquet::charts::calc::compute(micro_parquet::engine::filter_by_conditions(products().lazy(), &f.filters).expect("filter"), &f).expect("chart");
    assert!(!filtered.series.iter().any(|x| x == "TD"));
    assert_eq!(bar_fill(&filtered, "Savings", false), picked, "a filter");
    // Top N: only the largest series keep their places, the picked one stays on its colour
    let mut top = s.clone();
    top.top_n = 1;
    let t1 = chart(&top);
    assert_eq!(bar_fill(&t1, "Savings", false), picked, "top n: {:?}", t1.series);
    // the pick survives the file: a saved spec has it, an old one has none and is written without the field
    let json = serde_json::to_string(&s).expect("json");
    assert!(json.contains("\"custom\""));
    let back: ChartSpec = serde_json::from_str(&json).expect("loads");
    assert_eq!(back.colour_map.pick_of("Savings"), Some(&ColourPick::swatch(6, 2)));
    let mut old = spec();
    old.colour_map.assign_all(&["TD", "Savings"]);
    let old_json = serde_json::to_string(&old).expect("json");
    assert!(!old_json.contains("custom"), "a chart with no picks is written as before: {old_json}");
    let again: ChartSpec = serde_json::from_str(&old_json).expect("loads");
    assert!(again.colour_map.custom.is_empty());
    // a pick that is not a colour is refused when the chart is read, not drawn as black
    let bad = json.replace("#3a2196", "#3a219");
    assert!(serde_json::from_str::<ChartSpec>(&bad).is_err());
}
