//! The PDF export of a dashboard (Phase 10 X2): one A4 landscape page per dashboard page.
//!
//! Items keep the place they have on the page (the page's 1000-unit width is mapped to the paper with one scale, so a free-layout page
//! keeps its shape); a chart is drawn by the Phase 7 chart PDF code (vectors, embedded fonts), a pivot and a KPI card are drawn natively,
//! the design picture is the page's background, and a footer line says what the page is filtered by. Every number is the one the screen
//! shows (`ItemData`), in the units of the HTML and PowerPoint files.

use crate::charts::pdf::{add_fonts, draw_chart, draw_table_part, table_col_widths_mm, table_rows_fit, ChartArea, ChartPdfFonts, PdfMeasure, TABLE_ROW_MM};
use crate::charts::data_table::DataTableText;
use crate::charts::layout::{Measure, LayoutOpts};
use crate::dashboard::*;
use crate::kpi::{KpiSpec, KpiUnit};
use crate::pptx::{body_for, Body};
use anyhow::{bail, Context, Result};
use printpdf::*;

pub const PAGE_W_MM: f32 = 297.0;
pub const PAGE_H_MM: f32 = 210.0;
const MARGIN_MM: f32 = 8.0;
const HEADER_MM: f32 = 8.0;
const FOOTER_MM: f32 = 7.0;
const PAD_MM: f32 = 1.5;
const MM_PER_PT: f32 = 25.4 / 72.0;

pub struct PdfDashItem {
    pub id: String,
    pub title: String,
    /// Where the item sits on the page, in page units.
    pub rect: DashRect,
    pub data: ItemData,
    /// The item's money unit: divisor, decimals, name ("Crore").
    pub scale: (f64, usize, String),
    pub kpi: Option<(KpiSpec, KpiUnit)>,
}

pub struct PdfDashPage {
    pub title: String,
    /// The page's height in units (its width is 1000).
    pub page_h: f64,
    pub items: Vec<PdfDashItem>,
    /// The design picture (PNG or JPEG bytes), drawn as the background of the page.
    pub background: Option<Vec<u8>>,
    /// "As of 31 Oct 2026 · Filters: Wing = Islami".
    pub footer: String,
}

pub struct DashPdfPlan {
    pub title: String,
    pub pages: Vec<PdfDashPage>,
}

/// Where an item went on the paper, in millimetres from the top left of the page.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub page: usize,
    pub item_id: String,
    pub left_mm: f32,
    pub top_mm: f32,
    pub width_mm: f32,
    pub height_mm: f32,
}

#[derive(Clone, Debug, Default)]
pub struct DashPdfReport {
    pub placed: Vec<Placed>,
    /// Items whose table was cut to fit the paper ("Balances: 12 more rows are in the HTML and PowerPoint files").
    pub cut: Vec<String>,
}

/// The paper area a page of height `page_h` units is drawn in: left, top, millimetres per unit.
pub fn page_fit(page_h: f64) -> (f32, f32, f32) {
    let avail_w = PAGE_W_MM - 2.0 * MARGIN_MM;
    let avail_h = PAGE_H_MM - 2.0 * MARGIN_MM - HEADER_MM - FOOTER_MM;
    let s = (avail_w as f64 / PAGE_W).min(avail_h as f64 / page_h) as f32;
    let left = MARGIN_MM + (avail_w - s * PAGE_W as f32) / 2.0;
    (left, MARGIN_MM + HEADER_MM, s)
}

fn col(hex: u32) -> Color {
    Color::Rgb(Rgb::new(((hex >> 16) & 0xFF) as f32 / 255.0, ((hex >> 8) & 0xFF) as f32 / 255.0, (hex & 0xFF) as f32 / 255.0, None))
}

fn from_top(top_mm: f32) -> f32 {
    PAGE_H_MM - top_mm
}

fn draw_background(layer: &PdfLayerReference, bytes: &[u8], left: f32, top: f32, w: f32, h: f32) -> Result<()> {
    let img = ::image::load_from_memory(bytes).context("The design picture could not be read. Pick the design again in the page settings.")?.to_rgb8();
    let (pw, ph) = (img.width(), img.height());
    let mut jpg = Vec::new();
    ::image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 88).encode_image(&img).context("The design picture could not be prepared for the PDF.")?;
    let x = ImageXObject { width: Px(pw as usize), height: Px(ph as usize), color_space: ColorSpace::Rgb, bits_per_component: ColorBits::Bit8, interpolate: true, image_data: jpg, image_filter: Some(ImageFilter::DCT), clipping_bbox: None, smask: None };
    let dpi = 300.0f32;
    let natural_w = pw as f32 / dpi * 25.4;
    let natural_h = ph as f32 / dpi * 25.4;
    Image::from(x).add_to_layer(layer.clone(), ImageTransform { translate_x: Some(Mm(left)), translate_y: Some(Mm(from_top(top + h))), scale_x: Some(w / natural_w), scale_y: Some(h / natural_h), dpi: Some(dpi), ..Default::default() });
    Ok(())
}

fn surface(layer: &PdfLayerReference, left: f32, top: f32, w: f32, h: f32) {
    layer.set_fill_color(col(0xFCFCFB));
    layer.set_outline_color(col(0xE1E0D9));
    layer.set_outline_thickness(0.3);
    layer.add_rect(Rect::new(Mm(left), Mm(from_top(top + h)), Mm(left + w), Mm(from_top(top))).with_mode(printpdf::path::PaintMode::FillStroke));
}

fn text(layer: &PdfLayerReference, font: &IndirectFontRef, s: &str, pt: f32, x: f32, top: f32, colour: u32) {
    layer.set_fill_color(col(colour));
    // the baseline sits one cap-height under the line's top
    layer.use_text(s.to_string(), pt, Mm(x), Mm(from_top(top + pt * MM_PER_PT * 0.8)), font);
}

#[allow(clippy::too_many_arguments)]
fn draw_table(layer: &PdfLayerReference, fonts: &ChartPdfFonts, m: &PdfMeasure, title: &str, head: &[String], rows: &[Vec<String>], indent: &[usize], note: Option<&str>, left: f32, top: f32, w: f32, h: f32, report: &mut DashPdfReport) {
    text(layer, &fonts.semibold, title, 9.0, left + PAD_MM, top + PAD_MM, 0x0B0B0B);
    let rows: Vec<Vec<String>> = rows.iter().enumerate().map(|(i, r)| r.iter().enumerate().map(|(j, c)| if j == 0 { format!("{}{}", "\u{a0}\u{a0}".repeat(indent.get(i).copied().unwrap_or(0)), c) } else { c.clone() }).collect()).collect();
    let t = DataTableText { caption: String::new(), header: head.to_vec(), rows: rows.clone(), total: None, no_total_note: None };
    let mut widths = table_col_widths_mm(&t, m);
    let inner_w = w - 2.0 * PAD_MM;
    let sum: f32 = widths.iter().sum();
    if sum > inner_w && sum > 0.0 {
        let k = inner_w / sum;
        widths.iter_mut().for_each(|x| *x *= k);
    }
    let body_top = top + PAD_MM + 5.0;
    let avail = (top + h - PAD_MM - body_top).max(0.0);
    // all rows when they fit (with the item's note under them when there is one); otherwise fewer, with a line saying how many are left out
    let all = table_rows_fit(avail - if note.is_some() { TABLE_ROW_MM } else { 0.0 }, false, false);
    let shown = if rows.len() <= all { rows.len() } else { table_rows_fit(avail - TABLE_ROW_MM, false, false).min(rows.len()) };
    draw_table_part(layer, fonts, m, &t, &widths, left + PAD_MM, from_top(body_top), &rows[..shown], None, None);
    let mut foot = note.map(|n| n.to_string());
    if shown < rows.len() {
        let msg = format!("{} more rows are in the HTML and PowerPoint files.", rows.len() - shown);
        report.cut.push(format!("{}: {}", title, msg));
        foot = Some(match foot {
            Some(n) => format!("{} {}", n, msg),
            None => msg,
        });
    }
    if let Some(f) = foot {
        text(layer, &fonts.regular, &f, 7.0, left + PAD_MM, body_top + (1 + shown) as f32 * TABLE_ROW_MM + 0.8, 0x898781);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_card(layer: &PdfLayerReference, fonts: &ChartPdfFonts, title: &str, body: &Body, left: f32, top: f32, w: f32, h: f32) {
    text(layer, &fonts.semibold, title, 9.0, left + PAD_MM, top + PAD_MM, 0x0B0B0B);
    let Body::Kpi { value, value_colour, change, change_colour, words, period, reason, spark, target, graphic } = body else { return };
    let size = (h * 0.30 / MM_PER_PT).clamp(14.0, 34.0);
    let mut y = top + PAD_MM + 7.0;
    text(layer, &fonts.semibold, value, size, left + PAD_MM, y, value_colour.unwrap_or(0x0B0B0B));
    y += size * MM_PER_PT * 1.1;
    if !change.is_empty() {
        text(layer, &fonts.regular, change, 10.0, left + PAD_MM, y, change_colour.unwrap_or(0x52514E));
        y += 5.0;
    }
    if !words.is_empty() {
        text(layer, &fonts.regular, words, 8.0, left + PAD_MM, y, 0x52514E);
        y += 4.0;
    }
    if let Some(r) = reason {
        text(layer, &fonts.regular, r, 8.0, left + PAD_MM, y, 0x52514E);
        y += 4.0;
    }
    if !target.is_empty() {
        text(layer, &fonts.regular, target, 8.0, left + PAD_MM, y, 0x52514E);
        y += 4.0;
    }
    if !period.is_empty() {
        text(layer, &fonts.regular, period, 8.0, left + PAD_MM, y, 0x898781);
    }
    if let Some(g) = graphic {
        let gw = (w * 0.45).min(55.0);
        draw_graphic(layer, g, left + w - gw - PAD_MM * 2.0, top + PAD_MM + 8.0, gw);
    }
    // the sparkline: a line over the periods, broken where a period has no value
    if let Some(c) = spark {
        let vals: Vec<(usize, f64)> = c.series.first().map(|s| s.values.iter().enumerate().filter_map(|(i, v)| v.map(|x| (i, x))).collect()).unwrap_or_default();
        if vals.len() > 1 {
            let (lo, hi) = vals.iter().fold((f64::MAX, f64::MIN), |(a, b), (_, x)| (a.min(*x), b.max(*x)));
            let (sw, sh) = ((w * 0.45).min(45.0), (h * 0.3).min(14.0));
            let (sx, sy) = (left + w - sw - PAD_MM * 2.0, top + h - sh - PAD_MM * 2.0);
            let n = c.categories.len().max(2) - 1;
            layer.set_outline_color(col(colour_of_spark()));
            layer.set_outline_thickness(0.8);
            let mut run: Vec<(Point, bool)> = Vec::new();
            let mut last: Option<usize> = None;
            for (i, v) in &vals {
                if last.is_some_and(|l| *i != l + 1) && run.len() > 1 {
                    layer.add_line(Line { points: std::mem::take(&mut run), is_closed: false });
                } else if last.is_some_and(|l| *i != l + 1) {
                    run.clear();
                }
                let fx = sx + sw * (*i as f32 / n as f32);
                let fy = if hi > lo { sy + sh * (1.0 - ((*v - lo) / (hi - lo)) as f32) } else { sy + sh / 2.0 };
                run.push((Point::new(Mm(fx), Mm(from_top(fy))), false));
                last = Some(*i);
            }
            if run.len() > 1 {
                layer.add_line(Line { points: run, is_closed: false });
            }
        }
    }
}

/// A light tint of a colour on white (a PDF fill has no alpha here).
fn tint(c: u32, k: f32) -> u32 {
    let ch = |shift: u32| -> u32 {
        let v = ((c >> shift) & 0xFF) as f32;
        ((v * k + 255.0 * (1.0 - k)).round() as u32).min(255)
    };
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

fn rag_colour(r: crate::target::Rag) -> u32 {
    match r {
        crate::target::Rag::Green => crate::charts::tokens::STATUS_GOOD,
        crate::target::Rag::Amber => crate::charts::tokens::STATUS_WARNING,
        crate::target::Rag::Red => crate::charts::tokens::STATUS_CRITICAL,
    }
}

fn fill_polygon(layer: &PdfLayerReference, pts: &[(f64, f64)], colour: u32) {
    layer.set_fill_color(col(colour));
    let ring: Vec<(Point, bool)> = pts.iter().map(|p| (Point::new(Mm(p.0 as f32), Mm(from_top(p.1 as f32))), false)).collect();
    layer.add_polygon(Polygon { rings: vec![ring], mode: path::PaintMode::Fill, winding_order: path::WindingOrder::NonZero });
}

/// The bullet bar or gauge of a card with a target, from the same description the screen and the PowerPoint use.
fn draw_graphic(layer: &PdfLayerReference, g: &crate::kpi::Graphic, left: f32, top: f32, w: f32) {
    use crate::kpi::{ring_segment, KpiVisual};
    let ink = 0x0B0B0B;
    match g.kind {
        KpiVisual::Gauge => {
            let (cx, cy) = ((left + w / 2.0) as f64, (top + w / 2.0) as f64);
            let (ro, ri) = (w as f64 / 2.0, w as f64 / 2.0 * 0.62);
            for (from, to, rag) in &g.bands {
                fill_polygon(layer, &ring_segment(cx, cy, ro, ri, g.frac(*from), g.frac(*to), 24), tint(rag_colour(*rag), 0.35));
            }
            let at = |f: f64, r: f64| {
                let a = std::f64::consts::PI * (1.0 - f);
                Point::new(Mm((cx + r * a.cos()) as f32), Mm(from_top((cy - r * a.sin()) as f32)))
            };
            layer.set_outline_color(col(ink));
            layer.set_outline_thickness(1.0);
            layer.add_line(Line { points: vec![(at(g.frac(g.target), ri * 0.92), false), (at(g.frac(g.target), ro * 1.04), false)], is_closed: false });
            layer.set_outline_thickness(1.4);
            layer.add_line(Line { points: vec![(Point::new(Mm(cx as f32), Mm(from_top(cy as f32))), false), (at(g.frac(g.value), ro * 0.96), false)], is_closed: false });
        }
        _ => {
            let h = 5.0f64;
            let x = |f: f64| left as f64 + w as f64 * f;
            let y = top as f64;
            for (from, to, rag) in &g.bands {
                let (a, b) = (x(g.frac(*from)), x(g.frac(*to)));
                fill_polygon(layer, &[(a, y), (b, y), (b, y + h), (a, y + h)], tint(rag_colour(*rag), 0.35));
            }
            let (a, b) = (left as f64, x(g.frac(g.value)));
            fill_polygon(layer, &[(a, y + h * 0.3), (b, y + h * 0.3), (b, y + h * 0.7), (a, y + h * 0.7)], ink);
            layer.set_outline_color(col(ink));
            layer.set_outline_thickness(1.0);
            let t = x(g.frac(g.target)) as f32;
            layer.add_line(Line { points: vec![(Point::new(Mm(t), Mm(from_top(top))), false), (Point::new(Mm(t), Mm(from_top(top + h as f32))), false)], is_closed: false });
        }
    }
}

fn colour_of_spark() -> u32 {
    crate::charts::tokens::series_colour(0, false).unwrap_or(0x2a78d6)
}

pub fn export_dashboard_pdf(plan: &DashPdfPlan, out: &std::path::Path) -> Result<DashPdfReport> {
    if plan.pages.is_empty() {
        bail!("The dashboard has no pages to put in a PDF.");
    }
    let (doc, p0, l0) = PdfDocument::new(plan.title.as_str(), Mm(PAGE_W_MM), Mm(PAGE_H_MM), "Layer 1");
    let fonts = add_fonts(&doc)?;
    let measure = PdfMeasure::new();
    let mut report = DashPdfReport::default();
    let total = plan.pages.len();
    for (pi, page) in plan.pages.iter().enumerate() {
        let (pg, ly) = if pi == 0 { (p0, l0) } else { doc.add_page(Mm(PAGE_W_MM), Mm(PAGE_H_MM), "Layer 1") };
        let layer = doc.get_page(pg).get_layer(ly);
        let (left0, top0, s) = page_fit(page.page_h);
        text(&layer, &fonts.semibold, &format!("{} \u{b7} {}", plan.title, page.title), 11.0, MARGIN_MM, MARGIN_MM, 0x0B0B0B);
        let (pw, ph) = (s * PAGE_W as f32, s * page.page_h as f32);
        match &page.background {
            Some(b) => draw_background(&layer, b, left0, top0, pw, ph)?,
            None => {}
        }
        for it in &page.items {
            let (l, t, w, h) = (left0 + it.rect.x as f32 * s, top0 + it.rect.y as f32 * s, it.rect.w as f32 * s, it.rect.h as f32 * s);
            if !matches!(it.data, ItemData::Text(_) | ItemData::Image(_)) {
                surface(&layer, l, t, w, h);
            }
            report.placed.push(Placed { page: pi, item_id: it.id.clone(), left_mm: l, top_mm: t, width_mm: w, height_mm: h });
            match &it.data {
                ItemData::Chart(c) => {
                    let unit = if it.scale.2.is_empty() { String::new() } else { format!("BDT {}", it.scale.2) };
                    let opts = LayoutOpts { dark: false, divisor: it.scale.0, decimals: it.scale.1, unit, bare: false, colours: None };
                    draw_chart(&layer, &fonts, c, &opts, ChartArea { left_mm: l + PAD_MM, top_mm: from_top(t + PAD_MM), width_mm: w - 2.0 * PAD_MM, height_mm: h - 2.0 * PAD_MM })
                        .with_context(|| format!("The chart '{}' could not be drawn on the PDF page", it.title))?;
                }
                other => match body_for(other, &it.scale, &it.kpi) {
                    Body::Table { head, rows, indent, note, .. } => draw_table(&layer, &fonts, &measure, &it.title, &head, &rows, &indent, note.as_deref(), l, t, w, h, &mut report),
                    b @ Body::Kpi { .. } => draw_card(&layer, &fonts, &it.title, &b, l, t, w, h),
                    Body::Note(m) => {
                        text(&layer, &fonts.semibold, &it.title, 9.0, l + PAD_MM, t + PAD_MM, 0x0B0B0B);
                        text(&layer, &fonts.regular, &m, 8.0, l + PAD_MM, t + PAD_MM + 7.0, 0x52514E);
                    }
                    Body::Text { text: body, size_hundredths_pt, bold, algn } => {
                        // the slide is 13.33 in wide for 1000 page units, so one slide point is this many PDF points
                        let pt = (size_hundredths_pt as f32 / 100.0) * (s * PAGE_W as f32 / 338.67);
                        let line_mm = pt * MM_PER_PT * 1.25;
                        let font = if bold { &fonts.semibold } else { &fonts.regular };
                        let mut y = t + PAD_MM;
                        for line in body.lines() {
                            if y + line_mm > t + h {
                                report.cut.push("A text box is longer than its frame; the rest is in the HTML and PowerPoint files.".to_string());
                                break;
                            }
                            let wd = measure.size(line, pt, bold).x * MM_PER_PT;
                            let x = match algn {
                                "ctr" => l + ((w - wd) / 2.0).max(PAD_MM),
                                "r" => l + (w - wd - PAD_MM).max(PAD_MM),
                                _ => l + PAD_MM,
                            };
                            text(&layer, font, line, pt, x, y, 0x0B0B0B);
                            y += line_mm;
                        }
                    }
                    Body::Image { png, w: iw, h: ih, .. } => {
                        if iw > 0 && ih > 0 {
                            let k = (w / iw as f32).min(h / ih as f32);
                            let (fw, fh) = (iw as f32 * k, ih as f32 * k);
                            draw_background(&layer, &png, l + (w - fw) / 2.0, t + (h - fh) / 2.0, fw, fh).context("A picture on the dashboard could not be drawn on the PDF page")?;
                        }
                    }
                    Body::Chart(_) => {}
                },
            }
        }
        let foot = format!("{}   \u{b7}   Page {} of {}", page.footer, pi + 1, total);
        text(&layer, &fonts.regular, &foot, 8.0, MARGIN_MM, PAGE_H_MM - MARGIN_MM - 3.0, 0x52514E);
    }
    let tmp = out.with_extension("pdf.part");
    let file = std::fs::File::create(&tmp).with_context(|| format!("The file {} could not be written. Close it in your PDF reader and try again.", out.display()))?;
    doc.save(&mut std::io::BufWriter::new(file)).context("The PDF could not be saved")?;
    std::fs::rename(&tmp, out).with_context(|| format!("The file {} could not be replaced. Close it in your PDF reader and try again.", out.display()))?;
    let _ = measure.size("", 8.0, false);
    Ok(report)
}
