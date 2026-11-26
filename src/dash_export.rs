//! The dashboard's export (Phase 10): what the export dialog hands to a worker, and what the worker does with it.
//!
//! The dialog collects the pages of the dashboard as they are on the screen (sources, filters in force, places of the items, designs)
//! into an `ExportSrc`; this module calculates every item through `compute_item` (the one compute path) and writes the HTML file, the
//! PowerPoint file or the PDF with `report_html`, `pptx` and `dashboard_pdf`.

use crate::dashboard::*;
use crate::dashboard_pdf::{export_dashboard_pdf, DashPdfPlan, DashPdfReport, PdfDashItem, PdfDashPage};
use crate::kpi::{KpiSpec, KpiUnit};
use crate::pptx::{footnote, write_pptx, PptxPlan, PptxSlide};
use crate::report_html::*;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct ExportItemSrc {
    pub id: String,
    pub title: String,
    pub source: ItemSource,
    pub data_tab: Option<String>,
    /// A chart's divisor, decimals and unit name, or a pivot's unit (see `unit_scale`).
    pub scale: (f64, usize, String),
    pub kpi: Option<(KpiSpec, KpiUnit)>,
    pub rect: DashRect,
    /// What the item is filtered by on the screen (page filters and the click), and its drill path: PowerPoint and PDF show this state.
    pub picks: Vec<(PageFilter, Vec<String>)>,
    pub drill: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ExportPageSrc {
    pub id: String,
    pub title: String,
    pub page_h: f64,
    pub items: Vec<ExportItemSrc>,
    pub filters: Vec<PageFilter>,
    /// The values of each filter, as its dropdown lists them.
    pub values: Vec<Vec<String>>,
    /// "Wing = Islami", "Region = Dhaka": the filters in force on the screen.
    pub filter_text: Vec<String>,
    /// The design picture (PNG bytes).
    pub background: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct ExportSrc {
    pub title: String,
    pub pages: Vec<ExportPageSrc>,
    pub kpi_unit: KpiUnit,
    pub temp_dir: PathBuf,
}

/// The as-of date as the model says it ("31 Oct 2026"), or why there is none.
pub fn as_of_text() -> String {
    crate::model::scope().and_then(|c| c.calendar.as_ref().and_then(|cal| cal.as_of)).map(crate::model_calendar::long_date).unwrap_or_else(|| "date not set".to_string())
}

// ------------------------------------------------------------------------------------------------ HTML

/// The file, ready to write, with what the dialog shows before anything is written.
#[derive(Clone, Debug)]
pub struct HtmlPrepared {
    pub html: String,
    pub parts: SizeParts,
    pub not_linked: Vec<String>,
    pub has_drill: bool,
    pub pages: usize,
}

fn data_uri(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", crate::templates::base64_encode(png))
}

/// Calculates every answer of the file and renders it. An item over the limit stops here with what to cut; the size and the email rule
/// are applied by the dialog (`gate`) on `parts`.
pub fn prepare_html(src: &ExportSrc) -> Result<HtmlPrepared> {
    let pages: Vec<ReportPage> = src
        .pages
        .iter()
        .map(|p| ReportPage {
            id: p.id.clone(),
            title: p.title.clone(),
            filters: p.filters.clone(),
            values: p.values.clone(),
            items: p.items.iter().map(|i| ReportItem { id: i.id.clone(), title: i.title.clone(), source: i.source.clone(), data_tab: i.data_tab.clone(), scale: i.scale.clone() }).collect(),
        })
        .collect();
    let looks: Vec<PageLook> = src
        .pages
        .iter()
        .map(|p| PageLook { page_h: p.page_h, rects: p.items.iter().map(|i| [i.rect.x, i.rect.y, i.rect.w, i.rect.h]).collect(), background: p.background.as_deref().map(data_uri) })
        .collect();
    let template_bytes = looks.iter().filter_map(|l| l.background.as_ref()).map(|b| b.len()).sum();
    let plan = ReportPlan { title: src.title.clone(), pages, template_bytes, temp_dir: src.temp_dir.clone() };
    check_plan(&plan)?;
    let answers = compute_answers(&plan)?;
    let json = answers_json_with(&plan, &answers, src.kpi_unit);
    let parts = estimate_page(&plan, json.len(), &looks);
    let has_drill = answers.iter().flat_map(|p| &p.items).any(|i| i.answers.iter().any(|a| !a.path.is_empty()));
    let not_linked = crate::report_html::not_linked(&plan, &answers);
    let html = render_page(&plan, &json, &looks)?;
    Ok(HtmlPrepared { html, parts, not_linked, has_drill, pages: plan.pages.len() })
}

/// Writes the prepared file (through a temporary file) after the email rule has been applied by the caller.
pub fn write_html(path: &Path, p: &HtmlPrepared) -> Result<String> {
    let tmp = path.with_extension("html.part");
    std::fs::write(&tmp, p.html.as_bytes()).with_context(|| format!("The file {} could not be written. Close it and try again.", path.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("The file {} could not be replaced. Close it and try again.", path.display()))?;
    Ok(format!("HTML report written to {} ({:.1} MB).", path.display(), p.html.len() as f64 / 1_000_000.0))
}

// ------------------------------------------------------------------------------------------------ PowerPoint and PDF

fn calculated(src: &ExportSrc) -> Result<Vec<Vec<ItemData>>> {
    let mut out = Vec::new();
    for p in &src.pages {
        let mut items = Vec::new();
        for i in &p.items {
            let req = ItemRequest { item_id: i.id.clone(), source: i.source.clone(), picks: i.picks.clone(), drill: i.drill.clone(), temp_dir: src.temp_dir.clone() };
            items.push(compute_item(&req).with_context(|| format!("The item '{}' on the page '{}' could not be calculated", i.title, p.title))?);
        }
        out.push(items);
    }
    Ok(out)
}

fn foot(p: &ExportPageSrc, as_of: &str) -> String {
    footnote(as_of, &p.filter_text, &[])
}

/// The slide's footnote with the notes of a chart (Phase 13): the as-of date, the filters, then "[1] Mar 2026: Policy rate cut".
fn foot_with(p: &ExportPageSrc, as_of: &str, data: &ItemData) -> String {
    slide_footnote(as_of, &p.filter_text, data)
}

/// The footnote of an item's slide: for a chart it carries the chart's notes, because its PowerPoint chart is a native one that has no marker.
pub fn slide_footnote(as_of: &str, filters: &[String], data: &ItemData) -> String {
    match data {
        ItemData::Chart(c) => footnote(as_of, filters, &crate::charts::calc::annotation_lines(c)),
        _ => footnote(as_of, filters, &[]),
    }
}

/// One slide per item (a long pivot continues on more); every slide's footnote has the as-of date and the filters in force.
pub fn export_pptx(src: &ExportSrc, use_design: bool, path: &Path) -> Result<String> {
    if src.pages.iter().all(|p| p.items.is_empty()) {
        bail!("The dashboard has no items to put on slides. Add a chart, a pivot or a card first.");
    }
    let data = calculated(src)?;
    let as_of = as_of_text();
    let mut slides = Vec::new();
    for (p, d) in src.pages.iter().zip(data) {
        for (i, data) in p.items.iter().zip(d) {
            let footnote = foot_with(p, &as_of, &data);
            slides.push(PptxSlide { title: i.title.clone(), data, scale: i.scale.clone(), kpi: i.kpi.clone(), footnote });
        }
    }
    let background = if use_design { src.pages.iter().find_map(|p| p.background.clone()) } else { None };
    let n = slides.len();
    write_pptx(path, &PptxPlan { title: src.title.clone(), slides, background })?;
    Ok(format!("PowerPoint file with {} slide(s) written to {}.", n, path.display()))
}

/// One A4 landscape page per dashboard page.
pub fn export_pdf(src: &ExportSrc, path: &Path) -> Result<(String, DashPdfReport)> {
    if src.pages.is_empty() {
        bail!("The dashboard has no pages to put in a PDF.");
    }
    let data = calculated(src)?;
    let as_of = as_of_text();
    let pages: Vec<PdfDashPage> = src
        .pages
        .iter()
        .zip(data)
        .map(|(p, d)| PdfDashPage {
            title: p.title.clone(),
            page_h: p.page_h,
            items: p.items.iter().zip(d).map(|(i, data)| PdfDashItem { id: i.id.clone(), title: i.title.clone(), rect: i.rect, data, scale: i.scale.clone(), kpi: i.kpi.clone() }).collect(),
            background: p.background.clone(),
            footer: foot(p, &as_of),
        })
        .collect();
    let report = export_dashboard_pdf(&DashPdfPlan { title: src.title.clone(), pages }, path)?;
    let mut msg = format!("PDF with {} page(s) written to {}.", src.pages.len(), path.display());
    if !report.cut.is_empty() {
        msg.push_str(&format!(" {} table(s) were cut to fit the page: {}", report.cut.len(), report.cut.join(" ")));
    }
    Ok((msg, report))
}
