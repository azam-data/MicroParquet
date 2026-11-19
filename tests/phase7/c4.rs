//! C4: native Excel charts. For each form the written .xlsx (a zip) is opened and `xl/charts/chartN.xml`
//! is checked: the chart type, the series references (they point at the data range of the sheet), the
//! series colours (the palette) and, for a waterfall, the base series without a fill.
//!
//! The zip is read by `python3` (the standard library's `zipfile`); the tests are skipped with a note
//! when `python3` is not installed.

use crate::p7_c3::{bridge, flow_spec, flows, forms, heat, opts};
use micro_parquet::chart::{ChartAgg, ChartKind};
use micro_parquet::charts::calc::compute;
use micro_parquet::charts::data::ChartData;
use micro_parquet::charts::spec::{ChartSpec, MeasureSpec, PanelSpec, RefLine, Stack, Status};
use micro_parquet::charts::tokens as t;
use micro_parquet::charts::xlsx::{add_chart_sheet, safe_sheet_name, write_chart_xlsx, NOTE_BUBBLE, NOTE_HTML_ONLY};
use polars::prelude::*;
use serde_json::Value;
use std::path::PathBuf;

/// Reads the .xlsx and prints one JSON document: the chart files, their series and the sheet's cells.
const INSPECT: &str = r#"
import sys, json, zipfile, re
import xml.etree.ElementTree as ET
ns = {'c': 'http://schemas.openxmlformats.org/drawingml/2006/chart', 'a': 'http://schemas.openxmlformats.org/drawingml/2006/main',
      'm': 'http://schemas.openxmlformats.org/spreadsheetml/2006/main'}
z = zipfile.ZipFile(sys.argv[1])
names = z.namelist()
out = {'files': names, 'charts': [], 'cells': {}, 'strings': [], 'condfmt': []}
def colour(el):
    if el is None: return None
    f = el.find('a:solidFill/a:srgbClr', ns)
    return f.get('val') if f is not None else None
for n in sorted(x for x in names if re.match(r'xl/charts/chart\d+\.xml$', x)):
    root = ET.fromstring(z.read(n))
    ch = {'file': n, 'plots': [], 'text': ' '.join(t.text or '' for t in root.iter('{%s}t' % ns['a'])),
          'fonts': sorted({l.get('typeface') for l in root.iter('{%s}latin' % ns['a'])}), 'raw_has_noborder': False}
    sp = root.find('c:spPr', ns)
    ch['chart_border_none'] = bool(sp is not None and sp.find('a:ln/a:noFill', ns) is not None)
    pa = root.find('c:chart/c:plotArea', ns)
    for plot in pa:
        tag = plot.tag.split('}')[1]
        if not tag.endswith('Chart'): continue
        p = {'type': tag, 'barDir': None, 'grouping': None, 'series': []}
        bd = plot.find('c:barDir', ns); p['barDir'] = bd.get('val') if bd is not None else None
        g = plot.find('c:grouping', ns); p['grouping'] = g.get('val') if g is not None else None
        hs = plot.find('c:holeSize', ns); p['hole'] = hs.get('val') if hs is not None else None
        for ser in plot.findall('c:ser', ns):
            s = {}
            tx = ser.find('c:tx', ns)
            s['name'] = ''.join(x.text or '' for x in tx.iter('{%s}v' % ns['c'])) if tx is not None else None
            if tx is not None and not s['name']:
                f = tx.find('.//c:f', ns); s['name'] = f.text if f is not None else None
            s['cat'] = (ser.find('c:cat//c:f', ns).text if ser.find('c:cat//c:f', ns) is not None else (ser.find('c:xVal//c:f', ns).text if ser.find('c:xVal//c:f', ns) is not None else None))
            s['val'] = (ser.find('c:val//c:f', ns).text if ser.find('c:val//c:f', ns) is not None else (ser.find('c:yVal//c:f', ns).text if ser.find('c:yVal//c:f', ns) is not None else None))
            spp = ser.find('c:spPr', ns)
            s['fill'] = colour(spp)
            s['noFill'] = bool(spp is not None and spp.find('a:noFill', ns) is not None)
            s['line'] = colour(spp.find('a:ln', ns)) if spp is not None and spp.find('a:ln', ns) is not None else None
            s['lineW'] = spp.find('a:ln', ns).get('w') if spp is not None and spp.find('a:ln', ns) is not None else None
            alpha = spp.find('a:solidFill/a:srgbClr/a:alpha', ns) if spp is not None else None
            s['alpha'] = alpha.get('val') if alpha is not None else None
            s['points'] = [colour(d.find('c:spPr', ns)) for d in ser.findall('c:dPt', ns)]
            s['marker'] = colour(ser.find('c:marker/c:spPr', ns)) if ser.find('c:marker/c:spPr', ns) is not None else None
            s['errBars'] = len(ser.findall('c:errBars', ns))
            p['series'].append(s)
        ch['plots'].append(p)
    ch['gridline'] = None
    for ax in pa.findall('c:valAx', ns):
        gl = ax.find('c:majorGridlines/c:spPr/a:ln', ns)
        if gl is not None:
            ch['gridline'] = colour(gl)
            ch['gridlineDash'] = gl.find('a:prstDash', ns).get('val') if gl.find('a:prstDash', ns) is not None else 'solid'
    out['charts'].append(ch)
ss = []
if 'xl/sharedStrings.xml' in names:
    root = ET.fromstring(z.read('xl/sharedStrings.xml'))
    for si in root.findall('m:si', ns):
        ss.append(''.join(t.text or '' for t in si.iter('{%s}t' % ns['m'])))
out['strings'] = ss
sheet = ET.fromstring(z.read('xl/worksheets/sheet1.xml'))
for c in sheet.iter('{%s}c' % ns['m']):
    v = c.find('m:v', ns)
    if v is None: continue
    out['cells'][c.get('r')] = ss[int(v.text)] if c.get('t') == 's' else float(v.text)
for cf in sheet.iter('{%s}conditionalFormatting' % ns['m']):
    for r in cf.findall('m:cfRule', ns):
        cs = r.find('m:colorScale', ns)
        if cs is not None:
            out['condfmt'].append({'range': cf.get('sqref'), 'type': r.get('type'), 'colours': [c.get('rgb') for c in cs.findall('m:color', ns)]})
print(json.dumps(out))
"#;

fn python() -> bool {
    std::process::Command::new("python3").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mp-c4-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

fn inspect(path: &PathBuf) -> Value {
    let o = std::process::Command::new("python3").arg("-I").arg("-c").arg(INSPECT).arg(path).output().expect("python3 runs");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).expect("the inspector's JSON")
}

fn write(d: &ChartData, name: &str) -> (Value, PathBuf) {
    let p = tmp(name);
    write_chart_xlsx(&p, d, &opts()).unwrap_or_else(|e| panic!("{name}: {e:#}"));
    (inspect(&p), p)
}

fn col(i: usize) -> String {
    let mut s = String::new();
    let mut n = i + 1;
    while n > 0 {
        let r = (n - 1) % 26;
        s.insert(0, (b'A' + r as u8) as char);
        n = (n - 1) / 26;
    }
    s
}

/// "Chart!$B$5:$B$10" for the 0-based rows and column.
fn r(c: usize, r1: usize, r2: usize) -> String {
    format!("Chart!${}${}:${}${}", col(c), r1 + 1, col(c), r2 + 1)
}

fn plots(j: &Value) -> Vec<&Value> {
    j["charts"].as_array().unwrap().iter().flat_map(|c| c["plots"].as_array().unwrap().iter()).collect()
}

fn hex(c: u32) -> String {
    format!("{:06X}", c)
}

fn pick(name: &str) -> ChartData {
    forms().into_iter().find(|(k, _)| *k == name).unwrap().1
}

macro_rules! need_python {
    () => {
        if !python() {
            eprintln!("python3 is not installed: the .xlsx chart check is skipped");
            return;
        }
    };
}

#[test]
fn c4_columns_and_bars_are_native_charts_over_the_data_range_in_the_palette() {
    need_python!();
    // the table: title, subtitle, a blank row, then the header at row 4 (index 3) and the data below it
    let (header, first) = (3usize, 4usize);
    for (name, bar_dir, grouping, series) in [("grouped", "col", "clustered", 4), ("stacked", "col", "stacked", 4), ("percent", "col", "percentStacked", 4), ("column", "col", "clustered", 1), ("bar", "bar", "clustered", 1)] {
        let d = pick(name);
        let (j, _) = write(&d, &format!("{name}.xlsx"));
        let files: Vec<&str> = j["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).collect();
        assert!(files.contains(&"xl/charts/chart1.xml"), "{name}: {files:?}");
        let ps = plots(&j);
        assert_eq!(ps.len(), 1, "{name}");
        let p = ps[0];
        assert_eq!((p["type"].as_str(), p["barDir"].as_str(), p["grouping"].as_str()), (Some("barChart"), Some(bar_dir), Some(grouping)), "{name}");
        let ss = p["series"].as_array().unwrap();
        assert_eq!(ss.len(), series, "{name}");
        let last = first + d.categories.len() - 1;
        for (k, s) in ss.iter().enumerate() {
            // the categories are column A of the data range, the values column B, C, ...
            assert_eq!(s["cat"], r(0, first, last), "{name}");
            assert_eq!(s["val"], r(1 + k, first, last), "{name}");
            // the colours are the palette's slots, in order
            assert_eq!(s["fill"].as_str(), Some(hex(t::SERIES_LIGHT[k]).as_str()), "{name}: series {k}");
            // and the sheet has the numbers the chart reads
            for (i, c) in d.categories.iter().enumerate() {
                assert_eq!(j["cells"][format!("A{}", first + i + 1)].as_str(), Some(c.as_str()), "{name}");
                let want = d.values[k][i].unwrap();
                assert_eq!(j["cells"][format!("{}{}", col(1 + k), first + i + 1)].as_f64(), Some(want), "{name}");
            }
        }
        assert_eq!(j["cells"][format!("A{}", header + 1)].as_str(), Some("Month"), "{name}: the header row");
        let c0 = &j["charts"][0];
        // no chart border, Segoe UI, light solid 1-pixel-ish gridlines
        assert_eq!(c0["chart_border_none"], true, "{name}");
        assert!(c0["fonts"].as_array().unwrap().iter().any(|f| f == "Segoe UI"), "{name}: {:?}", c0["fonts"]);
        assert_eq!(c0["gridline"].as_str(), Some("E1E0D9"), "{name}");
        assert_eq!(c0["gridlineDash"].as_str(), Some("solid"), "{name}");
        assert!(c0["text"].as_str().unwrap().contains("Sum of Balance by Month"), "{name}: the title: {}", c0["text"]);
        // the subtitle is in the sheet, under the title
        assert!(j["cells"]["A2"].as_str().unwrap().contains("Sum of Balance"), "{name}");
    }
}

#[test]
fn c4_lines_areas_and_the_reference_line() {
    need_python!();
    // a stepped line is an ordinary line in Excel, and the sheet says so
    let (j, _) = write(&pick("stepped"), "stepped-note.xlsx");
    assert!(j["strings"].as_array().unwrap().iter().any(|s| s.as_str() == Some(micro_parquet::charts::xlsx::NOTE_STEPPED)));
    for (name, tag, grouping) in [("line", "lineChart", "standard"), ("stepped", "lineChart", "standard"), ("area", "areaChart", "standard"), ("stacked-area", "areaChart", "stacked")] {
        let d = pick(name);
        let (j, _) = write(&d, &format!("{name}.xlsx"));
        let ps = plots(&j);
        assert_eq!(ps.len(), 1, "{name}");
        assert_eq!((ps[0]["type"].as_str(), ps[0]["grouping"].as_str()), (Some(tag), Some(grouping)), "{name}");
        for (k, s) in ps[0]["series"].as_array().unwrap().iter().enumerate() {
            assert_eq!(s["val"], r(1 + k, 4, 9), "{name}");
            let want = hex(t::SERIES_LIGHT[k]);
            if tag == "lineChart" {
                assert_eq!(s["line"].as_str(), Some(want.as_str()), "{name}: a 2 px line in the series colour");
                assert_eq!(s["lineW"].as_str(), Some("25400"), "{name}: 2 pt = 25400 EMU");
                assert_eq!(s["marker"].as_str(), Some(want.as_str()), "{name}");
            } else {
                assert_eq!(s["fill"].as_str(), Some(want.as_str()), "{name}");
                assert_eq!(s["line"].as_str(), Some(want.as_str()), "{name}: the line on top of the layer");
                assert_eq!(s["alpha"].as_str(), Some(if grouping == "stacked" { "35000" } else { "10000" }), "{name}: area fill opacity");
            }
        }
    }
    // a reference line: a second (line) series over a helper column, 1.5 px, solid, in the status colour
    let mut s = micro_parquet::charts::spec::ChartSpec { category: "Month".into(), value: Some("Balance".into()), kind: ChartKind::Bar, order: crate::p7_c3::months(), ..Default::default() };
    s.ref_lines = vec![RefLine { value: 1300.0, label: "Limit".into(), status: Status::Critical, panel: 0 }];
    let d = compute(crate::p7_c3::frame().lazy(), &s).unwrap();
    let (j, _) = write(&d, "refline.xlsx");
    let ps = plots(&j);
    assert_eq!(ps.len(), 2, "the columns and the limit line");
    let line = ps.iter().find(|p| p["type"] == "lineChart").expect("a line chart");
    assert_eq!(line["series"][0]["line"].as_str(), Some("D03B3B"));
    assert_eq!(line["series"][0]["lineW"].as_str(), Some("19050"), "1.5 pt");
    assert_eq!(line["series"][0]["name"].as_str(), Some("Limit"));
}

#[test]
fn c4_a_waterfall_is_a_stacked_column_with_an_invisible_base_at_the_c0_bases() {
    need_python!();
    let (j, _) = write(&bridge(), "waterfall.xlsx");
    let ps = plots(&j);
    assert_eq!(ps.len(), 1);
    let p = ps[0];
    assert_eq!((p["type"].as_str(), p["barDir"].as_str(), p["grouping"].as_str()), (Some("barChart"), Some("col"), Some("stacked")));
    let ss = p["series"].as_array().unwrap();
    assert_eq!(ss.len(), 3);
    // the base series has no fill; the other two are coloured by point: totals grey and navy, steps blue
    assert_eq!(ss[0]["name"], "Base");
    assert_eq!(ss[0]["noFill"], true, "the base series has no fill");
    assert!(ss[0]["fill"].is_null());
    assert_eq!(ss[0]["val"], r(2, 4, 8));
    assert_eq!(ss[1]["val"], r(3, 4, 8));
    assert_eq!(ss[1]["noFill"], false);
    let pts: Vec<&str> = ss[1]["points"].as_array().unwrap().iter().map(|c| c.as_str().unwrap()).collect();
    assert_eq!(pts, ["52514E", "2A78D6", "2A78D6", "2A78D6", "0D366B"]);
    // the sheet holds the bases the chart floats on: 0, 80, 88, 93, 0 (the C0 acceptance)
    let bases: Vec<f64> = (0..5).map(|i| j["cells"][format!("C{}", 5 + i)].as_f64().unwrap()).collect();
    assert_eq!(bases, vec![0.0, 80.0, 88.0, 93.0, 0.0]);
    let ups: Vec<f64> = (0..5).map(|i| j["cells"][format!("D{}", 5 + i)].as_f64().unwrap()).collect();
    assert_eq!(ups, vec![80.0, 8.0, 5.0, 0.5, 93.5]);
    // a bridge that does not add up is refused for Excel as everywhere else
    let df = DataFrame::new(vec![Series::new("Step".into(), ["a", "b", "c"]).into(), Series::new("Amount".into(), [10.0, 5.0, 99.0]).into()]).unwrap();
    let s = ChartSpec { category: "Step".into(), value: Some("Amount".into()), kind: ChartKind::Waterfall, order: micro_parquet::sort_lists::SortOrder::List { name: "s".into(), values: ["a", "b", "c"].map(String::from).to_vec(), case_insensitive: true }, ..Default::default() };
    let err = write_chart_xlsx(&tmp("bad.xlsx"), &compute(df.lazy(), &s).unwrap(), &opts()).unwrap_err().to_string();
    assert!(err.contains("does not add up"), "{err}");
}

#[test]
fn c4_a_heatmap_is_the_data_range_under_a_three_colour_scale_with_every_pair() {
    need_python!();
    let d = heat();
    let (j, _) = write(&d, "heat.xlsx");
    assert!(j["charts"].as_array().unwrap().is_empty(), "a heatmap is cells, not a chart object");
    // the colour scale (the other conditional rule makes the text white on the dark steps)
    let cf = j["condfmt"].as_array().unwrap();
    assert_eq!(cf.len(), 1);
    assert_eq!(cf[0]["colours"], serde_json::json!(["FFCDE2FB", "FF3987E5", "FF0D366B"]), "the ramp's two ends and its middle");
    // 4 rows × 4 columns under the header at row 4: B5:E8
    assert_eq!(cf[0]["range"], "B5:E8");
    // every pair is a cell, a missing one is 0 (the CL order STD, SMA, SS, DF on both axes)
    let cols: Vec<&str> = ["B", "C", "D", "E"].iter().map(|c| j["cells"][format!("{c}4")].as_str().unwrap()).collect();
    assert_eq!(cols, ["STD", "SMA", "SS", "DF"]);
    let rows: Vec<&str> = (5..9).map(|i| j["cells"][format!("A{i}")].as_str().unwrap()).collect();
    assert_eq!(rows, ["STD", "SMA", "SS", "DF"]);
    for rr in 5..9 {
        for c in ["B", "C", "D", "E"] {
            assert!(j["cells"][format!("{c}{rr}")].is_number(), "{c}{rr}");
        }
    }
    assert_eq!(j["cells"]["E5"].as_f64(), Some(0.0), "STD to DF has no data: 0");
    let row_sum: f64 = ["B", "C", "D", "E"].iter().map(|c| j["cells"][format!("{c}5")].as_f64().unwrap()).sum();
    assert!((row_sum - 100.0).abs() < 1e-9, "% of row");
}

#[test]
fn c4_scatter_bubble_histogram_box_and_donut() {
    need_python!();
    let (j, _) = write(&pick("scatter"), "scatter.xlsx");
    let ps = plots(&j);
    assert_eq!(ps[0]["type"], "scatterChart");
    let ss = ps[0]["series"].as_array().unwrap();
    assert_eq!(ss.len(), 3, "at most three colour series");
    // the two largest series take slots 1 and 2, the folded "Other" is the de-emphasised grey
    let want = [hex(t::SERIES_LIGHT[0]), hex(t::SERIES_LIGHT[1]), hex(t::INK_LIGHT.deemph)];
    for (k, s) in ss.iter().enumerate() {
        assert_eq!(s["marker"].as_str(), Some(want[k].as_str()), "series {k}");
    }
    // the series' ranges are contiguous blocks of the table: a title, a subtitle and the note of the folded
    // series, a blank row, the header (index 4) and the rows from index 5
    let first = 5;
    let n0 = pick("scatter").points.iter().filter(|p| p.series == 0).count();
    assert_eq!(ss[0]["val"], r(1, first, first + n0 - 1));
    assert_eq!(ss[0]["cat"], r(0, first, first + n0 - 1));
    assert_eq!(ss[1]["val"], r(1, first + n0, first + n0 + pick("scatter").points.iter().filter(|p| p.series == 1).count() - 1));
    // a bubble is a scatter with a note (Excel has no bubble chart in this writer)
    let b = pick("bubble");
    let mut wb = rust_xlsxwriter::Workbook::new();
    let info = add_chart_sheet(&mut wb, "Chart", &b, &opts()).unwrap();
    assert_eq!(info.note.as_deref(), Some(NOTE_BUBBLE));
    assert_eq!(info.charts, 1);
    let (j, _) = write(&b, "bubble.xlsx");
    assert_eq!(plots(&j)[0]["type"], "scatterChart");
    assert!(j["strings"].as_array().unwrap().iter().any(|s| s.as_str() == Some(NOTE_BUBBLE)));
    // histogram: a column chart of the counts
    let (j, _) = write(&pick("histogram"), "hist.xlsx");
    assert_eq!(plots(&j)[0]["type"], "barChart");
    assert_eq!(plots(&j)[0]["series"][0]["fill"].as_str(), Some("2A78D6"));
    // box plot: stacked columns, an invisible base, error-bar whiskers
    let (j, _) = write(&pick("box"), "box.xlsx");
    let p = plots(&j)[0].clone();
    assert_eq!((p["type"].as_str(), p["grouping"].as_str()), (Some("barChart"), Some("stacked")));
    let ss = p["series"].as_array().unwrap();
    assert_eq!(ss.len(), 3);
    assert_eq!(ss[0]["noFill"], true, "the base has no fill");
    assert_eq!(ss[0]["errBars"], 1, "the lower whisker");
    assert_eq!(ss[2]["errBars"], 1, "the upper whisker");
    // donut: the slot colours by slice
    let (j, _) = write(&pick("donut"), "donut.xlsx");
    let p = plots(&j)[0].clone();
    assert_eq!(p["type"], "doughnutChart");
    let pts: Vec<&str> = p["series"][0]["points"].as_array().unwrap().iter().map(|c| c.as_str().unwrap()).collect();
    assert_eq!(pts.len(), 4);
    assert_eq!(pts[0], "2A78D6");
    // six parts are refused with the message
    let df = DataFrame::new(vec![Series::new("P".into(), ["a", "b", "c", "d", "e", "f"]).into(), Series::new("V".into(), [6.0, 5.0, 4.0, 3.0, 2.0, 1.0]).into()]).unwrap();
    let bad = compute(df.lazy(), &ChartSpec { category: "P".into(), value: Some("V".into()), kind: ChartKind::Donut, ..Default::default() }).unwrap();
    assert_eq!(write_chart_xlsx(&tmp("bad2.xlsx"), &bad, &opts()).unwrap_err().to_string(), "Use a bar chart: more than 5 parts are hard to compare as slices");
}

#[test]
fn c4_linked_panels_and_small_multiples_are_one_chart_per_panel() {
    need_python!();
    let (j, _) = write(&pick("linked-panels"), "linked.xlsx");
    let ps = plots(&j);
    assert_eq!(ps.len(), 2, "one chart per panel");
    assert_eq!((ps[0]["type"].as_str(), ps[1]["type"].as_str()), (Some("barChart"), Some("lineChart")));
    // each panel reads its own measure's column
    assert_eq!(ps[0]["series"][0]["val"], r(1, 4, 9));
    assert_eq!(ps[1]["series"][0]["val"], r(2, 4, 9));
    assert!(j["charts"][0]["text"].as_str().unwrap().contains("Net gap"));
    assert!(j["charts"][1]["text"].as_str().unwrap().contains("Average rate"));
    let (j, _) = write(&pick("small-multiples"), "multiples.xlsx");
    assert_eq!(plots(&j).len(), 4, "one chart per facet value");
    // Sankey and treemap: no chart, the data and a note
    for (name, d) in [("sankey", compute(flows().lazy(), &flow_spec(ChartKind::Sankey, true)).unwrap()), ("treemap", pick("treemap"))] {
        let (j, _) = write(&d, &format!("{name}.xlsx"));
        assert!(j["charts"].as_array().unwrap().is_empty(), "{name}: not written as a chart");
        assert!(j["strings"].as_array().unwrap().iter().any(|s| s.as_str() == Some(NOTE_HTML_ONLY)), "{name}: the note");
        assert!(j["cells"].as_object().unwrap().values().any(|v| v.is_number()), "{name}: the data is there");
    }
}

#[test]
fn c4_delta_is_the_delta_sign_in_the_workbook_and_sheet_names_are_safe() {
    need_python!();
    let df = DataFrame::new(vec![Series::new("Scenario".into(), ["Parallel up", "Parallel down"]).into(), Series::new("dEVE".into(), [-10.5, 4.8]).into()]).unwrap();
    let s = ChartSpec { category: "Scenario".into(), value: Some("dEVE".into()), kind: ChartKind::Bar, horizontal: true, ..Default::default() };
    let (j, _) = write(&compute(df.lazy(), &s).unwrap(), "delta.xlsx");
    let mut all = j["charts"].as_array().unwrap().iter().map(|c| c["text"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    all.extend(j["strings"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()));
    let text = all.join("|");
    assert!(text.contains("\u{394}EVE"), "{text}");
    assert!(!text.contains("dEVE"), "{text}");
    assert_eq!(safe_sheet_name("Sum of Balance: by [Month]/Product?"), "Sum of Balance by Month Product");
    assert_eq!(safe_sheet_name("x".repeat(40).as_str()).len(), 31);
    assert_eq!(safe_sheet_name("***"), "Chart");
}

#[test]
fn c4_a_multi_measure_chart_and_an_empty_chart() {
    need_python!();
    let mut s = ChartSpec { category: "Month".into(), value: Some("Balance".into()), kind: ChartKind::Bar, order: crate::p7_c3::months(), stack: Stack::Grouped, ..Default::default() };
    s.measures = vec![MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, label: "Net gap".into(), ..Default::default() }, MeasureSpec { column: Some("Rate".into()), agg: ChartAgg::Average, label: "Average rate".into(), ..Default::default() }];
    s.panels = vec![PanelSpec { kind: ChartKind::Bar, measures: vec![0], title: String::new() }, PanelSpec { kind: ChartKind::Line, measures: vec![1], title: String::new() }];
    let d = compute(crate::p7_c3::frame().lazy(), &s).unwrap();
    let (j, _) = write(&d, "two.xlsx");
    assert_eq!(plots(&j).len(), 2);
    // an unwritable folder is a message a banker can act on
    let bad = std::env::temp_dir().join("mp-c4-nonexistent-folder").join("x.xlsx");
    let err = write_chart_xlsx(&bad, &d, &opts()).unwrap_err().to_string();
    assert!(err.starts_with("Could not write") && err.contains("close the file if it is open in Excel"), "{err}");
}

/// With `MP_XLSX_OUT=<folder>` set, writes every form's workbook there (for a visual check in a spreadsheet program).
#[test]
fn c4_every_form_can_be_written_for_a_visual_check() {
    let out = std::env::var("MP_XLSX_OUT").ok().map(PathBuf::from);
    for (name, d) in forms() {
        let path = out.clone().map(|d| d.join(format!("{name}.xlsx"))).unwrap_or_else(|| tmp(&format!("{name}-all.xlsx")));
        if let Some(dir) = &out {
            std::fs::create_dir_all(dir).unwrap();
        }
        write_chart_xlsx(&path, &d, &opts()).unwrap_or_else(|e| panic!("{name}: {e:#}"));
    }
}
