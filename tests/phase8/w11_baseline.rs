//! The Phase 7 baseline of W11: what the renderers give for a few stacked and plain charts when "Segment labels" and "Data table"
//! are off. `tests/fixtures/phase8_w11_off.hashes.json` holds the blake3 hashes of that output as the Phase 7 code (`main` at
//! f3e6f91, in a temporary worktree) gave it; `w11_with_both_options_off_the_output_equals_phase_7s` makes the same output with
//! today's code and compares. This file uses only APIs that Phase 7 had, so the same code made the hashes. Do not edit the fixture
//! by hand.

#![allow(dead_code)]

use micro_parquet::charts::calc::compute;
use micro_parquet::charts::echarts::{chart_book_html, chart_pair, BookItem};
use micro_parquet::charts::layout::{layout_chart, ApproxMeasure, LayoutOpts};
use micro_parquet::charts::python::chart_to_python;
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec, Stack};
use micro_parquet::charts::xlsx::write_chart_xlsx;
use polars::prelude::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// Deposits by month and product: 12 months of three products (some of them thin), deterministic.
pub fn frame(n_months: usize) -> DataFrame {
    let (mut m, mut p, mut v) = (Vec::new(), Vec::new(), Vec::new());
    for (c, month) in MONTHS.iter().take(n_months).enumerate() {
        for (s, product) in ["Term deposit", "Savings", "Current"].iter().enumerate() {
            m.push(*month);
            p.push(*product);
            let base = [900.0, 420.0, 35.0][s];
            v.push(base + 17.0 * c as f64 * (s as f64 + 1.0) + if (c + s) % 5 == 0 { 60.0 } else { 0.0 });
        }
    }
    DataFrame::new(vec![Series::new("Month".into(), m).into(), Series::new("Product".into(), p).into(), Series::new("Balance".into(), v).into()]).expect("the frame")
}

fn spec(kind: ChartKind, stack: Stack, horizontal: bool, series: bool) -> ChartSpec {
    ChartSpec { category: "Month".into(), series: series.then(|| "Product".to_string()), value: Some("Balance".into()), agg: ChartAgg::Sum, kind, stack, horizontal, top_n: 500, ..Default::default() }
}

/// The charts of the baseline: (name, the spec, months in the table).
pub fn charts() -> Vec<(&'static str, ChartSpec, usize)> {
    vec![
        ("stacked_column", spec(ChartKind::Bar, Stack::Stacked, false, true), 12),
        ("percent_bar", spec(ChartKind::Bar, Stack::Percent, true, true), 6),
        ("grouped_column", spec(ChartKind::Bar, Stack::Grouped, false, true), 6),
        ("line", spec(ChartKind::Line, Stack::Grouped, false, true), 12),
        ("single_bar", spec(ChartKind::Bar, Stack::Grouped, false, false), 8),
    ]
}

fn opts(dark: bool) -> LayoutOpts {
    LayoutOpts { dark, divisor: 1.0, decimals: 2, unit: "BDT Crore".into(), bare: false, colours: None }
}

fn hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mp_w11_baseline_{}", std::process::id()));
    std::fs::create_dir_all(&d).expect("temp dir");
    d.join(name)
}

/// The text of every chart part of a workbook, as python3 reads it: the hash of each `xl/charts/chart*.xml`.
fn xlsx_parts(path: &Path) -> String {
    let script = "import sys, zipfile, hashlib\nz = zipfile.ZipFile(sys.argv[1])\nfor n in sorted(z.namelist()):\n    if n.startswith('xl/charts/chart') or n.startswith('xl/worksheets/sheet'):\n        print(n, hashlib.sha256(z.read(n)).hexdigest())\n";
    let o = std::process::Command::new("python3").arg("-I").arg("-c").arg(script).arg(path).output().expect("python3 runs");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).to_string()
}

/// The text on the pages of a PDF as `pdftotext -layout` reads it.
pub fn pdf_text(path: &Path) -> String {
    let o = std::process::Command::new("pdftotext").arg("-layout").arg(path).arg("-").output().expect("pdftotext runs");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).to_string()
}

/// Every output of every baseline chart, as (name, blake3).
pub fn outputs() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut book_items: Vec<micro_parquet::charts::data::ChartData> = Vec::new();
    for (name, s, months) in charts() {
        let d = compute(frame(months).lazy(), &s).expect("the chart");
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(1200.0, 700.0));
        for (theme, dark) in [("light", false), ("dark", true)] {
            let l = layout_chart(&d, rect, &opts(dark), &ApproxMeasure);
            out.insert(format!("{name}/app/{theme}"), hash(format!("{:?}\n{:?}\n{:?}", l.prims, l.texts, l.marks.iter().map(|m| (m.rect, m.tip.clone())).collect::<Vec<_>>()).as_bytes()));
        }
        // the narrow window of the side panel too
        let l = layout_chart(&d, egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(420.0, 300.0)), &opts(false), &ApproxMeasure);
        out.insert(format!("{name}/app/narrow"), hash(format!("{:?}\n{:?}", l.prims, l.texts).as_bytes()));
        let pair = chart_pair(&d, &opts(false)).expect("html");
        out.insert(format!("{name}/html/light"), hash(pair.light.to_string().as_bytes()));
        out.insert(format!("{name}/html/dark"), hash(pair.dark.to_string().as_bytes()));
        out.insert(format!("{name}/python"), hash(chart_to_python(&d, &opts(false)).expect("python").as_bytes()));
        let x = tmp(&format!("{name}.xlsx"));
        write_chart_xlsx(&x, &d, &opts(false)).expect("xlsx");
        out.insert(format!("{name}/xlsx"), hash(xlsx_parts(&x).as_bytes()));
        // the chart as a PDF page: the text of the page and the number of pages
        {
            use micro_parquet::exporters::{export_pdf_tabs_with, PdfOptions, PdfTab};
            let p = tmp(&format!("{name}.pdf"));
            let tab = PdfTab { title: "Chart".into(), df: micro_parquet::chart::chart_table(&d).expect("table"), col_formats: Default::default(), summary: vec![], chart: Some((d.clone(), opts(false))) };
            let o = PdfOptions { title: "Deposits".into(), page: micro_parquet::pdf_layout::PdfPage { landscape: true, legal: false }, chart_tables: false };
            export_pdf_tabs_with(vec![tab], &p, micro_parquet::models::NumberDisplayFormat::BdtCrore, 2, &o).expect("pdf");
            let text = pdf_text(&p);
            out.insert(format!("{name}/pdf/text"), hash(text.as_bytes()));
            out.insert(format!("{name}/pdf/pages"), text.matches('\x0c').count().to_string());
        }
        book_items.push(d);
    }
    let items: Vec<BookItem> = book_items.iter().map(|d| BookItem { data: d, opts: opts(false) }).collect();
    out.insert("book".to_string(), hash(chart_book_html("Book", "2026-11-23", &items).expect("book").as_bytes()));
    out
}
