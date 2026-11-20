//! A chart on a PDF page (Phase 7 final fixes, item 6): the chart is drawn as vectors from the same layout the app
//! draws (`ChartLayout` primitives), with the embedded Source Sans 3 fonts, through `printpdf`. A chart tab exported to
//! PDF writes the chart, not its table.

use super::data::ChartData;
use super::layout::{layout_chart, ChartLayout, LayoutOpts, Measure, Prim};
use anyhow::{Context, Result};
use egui::{Color32, Pos2, Rect, Vec2};
use printpdf::*;

/// 1 layout unit (a point) in millimetres.
const MM_PER_PT: f32 = 25.4 / 72.0;

const REGULAR: &[u8] = include_bytes!("../../assets/fonts/SourceSans3-Regular.ttf");
const SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/SourceSans3-Semibold.ttf");

/// The two embedded fonts of a document.
pub struct ChartPdfFonts {
    pub regular: IndirectFontRef,
    pub semibold: IndirectFontRef,
}

/// Embeds Source Sans 3 Regular and Semibold in the document.
pub fn add_fonts(doc: &PdfDocumentReference) -> Result<ChartPdfFonts> {
    let regular = doc.add_external_font(std::io::Cursor::new(REGULAR)).context("Could not embed the chart font")?;
    let semibold = doc.add_external_font(std::io::Cursor::new(SEMIBOLD)).context("Could not embed the chart font")?;
    Ok(ChartPdfFonts { regular, semibold })
}

/// Measures text with the same fonts the PDF embeds (through egui's font engine, headless).
pub struct PdfMeasure {
    ctx: egui::Context,
}

impl PdfMeasure {
    pub fn new() -> PdfMeasure {
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert("sans-regular".into(), egui::FontData::from_static(REGULAR));
        fonts.font_data.insert("sans-semibold".into(), egui::FontData::from_static(SEMIBOLD));
        fonts.families.insert(egui::FontFamily::Proportional, vec!["sans-regular".to_string()]);
        fonts.families.insert(egui::FontFamily::Name("sans-semibold".into()), vec!["sans-semibold".to_string(), "sans-regular".to_string()]);
        ctx.set_fonts(fonts);
        // one empty frame makes the font set current
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        PdfMeasure { ctx }
    }
}

impl Default for PdfMeasure {
    fn default() -> Self {
        Self::new()
    }
}

impl Measure for PdfMeasure {
    fn size(&self, text: &str, pt: f32, semibold: bool) -> Vec2 {
        let family = if semibold { egui::FontFamily::Name("sans-semibold".into()) } else { egui::FontFamily::Proportional };
        self.ctx.fonts(|f| f.layout_no_wrap(text.to_owned(), egui::FontId::new(pt, family), Color32::WHITE).size())
    }
}

/// Where a chart goes on a page, in millimetres: left edge, top edge (from the bottom of the page), width, height.
#[derive(Clone, Copy, Debug)]
pub struct ChartArea {
    pub left_mm: f32,
    pub top_mm: f32,
    pub width_mm: f32,
    pub height_mm: f32,
}

fn rgb(c: Color32, surface: Color32) -> Color {
    // a colour with transparency is laid over the chart's surface (the premultiplied colour plus what shows through)
    let k = 1.0 - c.a() as f32 / 255.0;
    let ch = |v: u8, s: u8| ((v as f32 + s as f32 * k) / 255.0).clamp(0.0, 1.0);
    Color::Rgb(Rgb::new(ch(c.r(), surface.r()), ch(c.g(), surface.g()), ch(c.b(), surface.b()), None))
}

/// Draws `data` on `layer` inside `area`: the title, subtitle, plot, legend, labels and footnote of the app's layout.
pub fn draw_chart(layer: &PdfLayerReference, fonts: &ChartPdfFonts, data: &ChartData, opts: &LayoutOpts, area: ChartArea) -> Result<ChartLayout> {
    let measure = PdfMeasure::new();
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(area.width_mm / MM_PER_PT, area.height_mm / MM_PER_PT));
    // a PDF is always the light chart
    let opts = LayoutOpts { dark: false, ..opts.clone() };
    let l = layout_chart(data, rect, &opts, &measure);
    let at = |p: Pos2| -> Point { Point::new(Mm(area.left_mm + p.x * MM_PER_PT), Mm(area.top_mm - p.y * MM_PER_PT)) };
    let surface = l.surface;
    layer.set_line_cap_style(LineCapStyle::Round);
    layer.set_line_join_style(LineJoinStyle::Round);

    let fill_poly = |pts: Vec<Pos2>, c: Color32| {
        if pts.len() < 3 {
            return;
        }
        layer.set_fill_color(rgb(c, surface));
        layer.add_polygon(Polygon { rings: vec![pts.into_iter().map(|p| (at(p), false)).collect()], mode: path::PaintMode::Fill, winding_order: path::WindingOrder::NonZero });
    };
    for p in &l.prims {
        match p {
            Prim::Rect { rect, fill, round } => {
                // corners are arcs of a few straight pieces (nw, ne, sw, se)
                let mut pts: Vec<Pos2> = Vec::new();
                let arc = |pts: &mut Vec<Pos2>, cx: f32, cy: f32, r: f32, a0: f32| {
                    if r <= 0.01 {
                        pts.push(Pos2::new(cx, cy));
                        return;
                    }
                    for k in 0..=5 {
                        let a = a0 + std::f32::consts::FRAC_PI_2 * k as f32 / 5.0;
                        pts.push(Pos2::new(cx + r * a.cos(), cy + r * a.sin()));
                    }
                };
                let (nw, ne, sw, se) = (round[0].min(rect.width() / 2.0), round[1].min(rect.width() / 2.0), round[2].min(rect.width() / 2.0), round[3].min(rect.width() / 2.0));
                let pi = std::f32::consts::PI;
                arc(&mut pts, rect.left() + nw, rect.top() + nw, nw, pi);
                arc(&mut pts, rect.right() - ne, rect.top() + ne, ne, 1.5 * pi);
                arc(&mut pts, rect.right() - se, rect.bottom() - se, se, 0.0);
                arc(&mut pts, rect.left() + sw, rect.bottom() - sw, sw, 0.5 * pi);
                fill_poly(pts, *fill);
            }
            Prim::Line { a, b, w, col } => {
                layer.set_outline_color(rgb(*col, surface));
                layer.set_outline_thickness(*w);
                layer.add_line(Line { points: vec![(at(*a), false), (at(*b), false)], is_closed: false });
            }
            Prim::Tris { tris, fill } => {
                // a hairline in the fill's own colour closes the seams between neighbouring triangles
                layer.set_outline_color(rgb(*fill, surface));
                layer.set_outline_thickness(0.3);
                layer.set_fill_color(rgb(*fill, surface));
                for t in tris {
                    layer.add_polygon(Polygon { rings: vec![t.iter().map(|p| (at(*p), false)).collect()], mode: path::PaintMode::FillStroke, winding_order: path::WindingOrder::NonZero });
                }
            }
            Prim::Circle { c, r, fill, ring } => {
                if let Some((w, col)) = ring {
                    fill_poly(circle(*c, *r + *w), *col);
                }
                fill_poly(circle(*c, *r), *fill);
            }
            Prim::Outline { c, r, w, col } => {
                layer.set_outline_color(rgb(*col, surface));
                layer.set_outline_thickness(*w);
                layer.add_line(Line { points: circle(*c, *r).into_iter().map(|p| (at(p), false)).collect(), is_closed: true });
            }
        }
    }
    // texts: the baseline is a little under the middle of the line box
    for t in &l.texts {
        let font = match t.face {
            super::layout::Face::Semibold => &fonts.semibold,
            super::layout::Face::Regular => &fonts.regular,
        };
        layer.set_fill_color(rgb(t.colour, surface));
        if t.angle != 0.0 {
            // an angled category label: written along its slope from its start
            let w = measure.size(&t.text, t.pt, false).x;
            let (sn, cs) = ((-t.angle).sin(), (-t.angle).cos());
            let top_left = Pos2::new(t.rect.left(), t.rect.top() + w * sn);
            let base = Pos2::new(top_left.x + t.pt * 0.95 * sn, top_left.y + t.pt * 0.95 * cs);
            let p = at(base);
            layer.begin_text_section();
            layer.set_font(font, t.pt);
            layer.set_text_matrix(TextMatrix::TranslateRotate(p.x, p.y, (-t.angle).to_degrees()));
            layer.write_text(t.text.clone(), font);
            layer.end_text_section();
            continue;
        }
        let lines: Vec<&str> = t.text.lines().collect();
        let n = lines.len().max(1) as f32;
        let line_h = t.rect.height() / n;
        for (i, line) in lines.iter().enumerate() {
            let lw = measure.size(line, t.pt, t.face == super::layout::Face::Semibold).x;
            // one line keeps the left edge of its box; several lines are centred in it
            let x = if lines.len() > 1 { t.rect.center().x - lw / 2.0 } else { t.rect.left() };
            let base = Pos2::new(x, t.rect.top() + line_h * (i as f32 + 0.5) + t.pt * 0.34);
            let p = at(base);
            layer.use_text(line.to_string(), t.pt, Mm::from(p.x), Mm::from(p.y), font);
        }
    }
    Ok(l)
}

fn circle(c: Pos2, r: f32) -> Vec<Pos2> {
    (0..24).map(|k| {
        let a = std::f32::consts::TAU * k as f32 / 24.0;
        Pos2::new(c.x + r * a.cos(), c.y + r * a.sin())
    }).collect()
}
