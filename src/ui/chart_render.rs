//! Renderer A of the chart studio (Phase 7 C1): the in-app painter. The geometry is worked out by
//! `micro_parquet::charts::layout` (one rectangle per mark, one per text, every collision checked);
//! this file draws it with the egui painter at the window's scale, in light and dark, and adds the
//! interaction: an outline and tooltip on the mark under the pointer, a crosshair with every series'
//! value on lines and areas, and the Table view. It is not egui_plot: egui_plot cannot meet the mark
//! specs (rounded data-ends, 2 px gaps, collision-checked labels).

use crate::ui::prelude::*;
use micro_parquet::charts::data::ChartData;
use micro_parquet::charts::layout::{hit_test, layout_chart, tooltip_rect, ChartLayout, Face, Hover, LayoutOpts, Measure, Prim, TOOLTIP_PADDING};
use micro_parquet::charts::tokens as ct;

/// Measures text with the real fonts (Source Sans 3 regular and semibold).
pub struct EguiMeasure<'a>(pub &'a egui::Context);

pub fn chart_font(pt: f32, face: Face) -> egui::FontId {
    match face {
        Face::Regular => egui::FontId::new(pt, egui::FontFamily::Proportional),
        Face::Semibold => egui::FontId::new(pt, theme::family_semibold()),
    }
}

impl Measure for EguiMeasure<'_> {
    fn size(&self, text: &str, pt: f32, semibold: bool) -> egui::Vec2 {
        let font = chart_font(pt, if semibold { Face::Semibold } else { Face::Regular });
        self.0.fonts(|f| f.layout_no_wrap(text.to_owned(), font, egui::Color32::WHITE).size())
    }
}

/// What `show_chart` drew (the render tests read it).
#[derive(Clone, Debug)]
pub struct ChartShown {
    pub layout: ChartLayout,
    /// The whole chart, in screen coordinates.
    pub rect: egui::Rect,
    pub hover: Option<Hover>,
    /// The tooltip box, when one is shown.
    pub tooltip: Option<egui::Rect>,
}

/// The layout of the chart last shown under `id` (tests, and the "Open as tab" / save paths).
pub fn last_shown(ctx: &egui::Context, id: egui::Id) -> Option<ChartShown> {
    ctx.data(|d| d.get_temp::<ChartShown>(id))
}

fn round(r: [f32; 4]) -> egui::Rounding {
    egui::Rounding { nw: r[0], ne: r[1], sw: r[2], se: r[3] }
}

/// Paints a finished layout.
pub fn paint_layout(painter: &egui::Painter, l: &ChartLayout) {
    let full = egui::Rect::from_min_size(l.rect.min, egui::vec2(l.rect.width(), l.needed_height.max(l.rect.height())));
    painter.rect_filled(full, 0.0, l.surface);
    for p in &l.prims {
        match p {
            Prim::Rect { rect, fill, round: r } => {
                painter.rect_filled(*rect, round(*r), *fill);
            }
            Prim::Line { a, b, w, col } => {
                painter.line_segment([*a, *b], egui::Stroke::new(*w, *col));
            }
            Prim::Tris { tris, fill } => {
                let mut mesh = egui::epaint::Mesh::default();
                for t in tris {
                    let base = mesh.vertices.len() as u32;
                    for v in t {
                        mesh.colored_vertex(*v, *fill);
                    }
                    mesh.add_triangle(base, base + 1, base + 2);
                }
                painter.add(egui::Shape::mesh(mesh));
            }
            Prim::Circle { c, r, fill, ring } => {
                if let Some((w, col)) = ring {
                    painter.circle_filled(*c, *r + *w, *col);
                }
                painter.circle_filled(*c, *r, *fill);
            }
            Prim::Outline { c, r, w, col } => {
                painter.circle_stroke(*c, *r, egui::Stroke::new(*w, *col));
            }
        }
    }
    for t in &l.texts {
        painter.text(t.rect.left_top(), egui::Align2::LEFT_TOP, &t.text, chart_font(t.pt, t.face), t.colour);
    }
}

/// The tooltip box and the outline or crosshair of the hovered mark, in a layer above everything.
fn paint_hover(ctx: &egui::Context, id: egui::Id, l: &ChartLayout, h: &Hover, pos: egui::Pos2) -> egui::Rect {
    let ink = ct::ink(l.dark);
    let rgb = theme::hex;
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, id.with("hover")));
    if let Some(r) = h.rect {
        painter.rect_stroke(r.expand(ct::GRID_PX), egui::Rounding::same(ct::BAR_RADIUS_PX), egui::Stroke::new(ct::LINE_PX, rgb(ink.ink)));
    }
    if let Some((x, y0, y1)) = h.cross {
        painter.line_segment([egui::pos2(x, y0), egui::pos2(x, y1)], egui::Stroke::new(ct::GRID_PX, rgb(ink.muted)));
    }
    let galley = ctx.fonts(|f| f.layout(h.tip.clone(), chart_font(ct::LABEL_PT, Face::Regular), rgb(ink.ink), theme::CHART_TOOLTIP_W));
    let size = galley.size() + egui::vec2(TOOLTIP_PADDING * 2.0, TOOLTIP_PADDING * 2.0);
    let bounds = egui::Rect::from_min_max(l.rect.min, egui::pos2(l.rect.right(), l.rect.top() + l.needed_height.max(l.rect.height())));
    let box_rect = tooltip_rect(pos, size, bounds.union(egui::Rect::from_min_size(pos, egui::Vec2::ZERO)));
    painter.rect(box_rect, egui::Rounding::same(ct::BAR_RADIUS_PX), rgb(ink.surface), egui::Stroke::new(ct::GRID_PX, rgb(ink.baseline)));
    painter.galley(box_rect.min + egui::vec2(TOOLTIP_PADDING, TOOLTIP_PADDING), galley, rgb(ink.ink));
    box_rect
}

/// Draws the chart in the space the ui has (`height` if given), with hover. Facet charts that need more
/// height than there is make the ui taller (put the chart in a vertical scroll area).
pub fn show_chart(ui: &mut egui::Ui, id: egui::Id, data: &ChartData, opts: &LayoutOpts, height: Option<f32>) -> ChartShown {
    let ctx = ui.ctx().clone();
    let m = EguiMeasure(&ctx);
    let width = ui.available_width().max(theme::CHART_MIN_W);
    let h = height.unwrap_or_else(|| ui.available_height().clamp(theme::CHART_MIN_H, theme::CHART_MAX_H));
    // small multiples can need more height than they were given
    let probe = layout_chart(data, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, h)), opts, &m);
    let h = probe.needed_height.max(h);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::hover());
    let layout = layout_chart(data, rect, opts, &m);
    paint_layout(&ui.painter_at(rect), &layout);
    let mut hover = None;
    let mut tooltip = None;
    if let Some(pos) = ui.input(|i| i.pointer.hover_pos()).filter(|p| resp.hovered() && rect.contains(*p)) {
        hover = hit_test(&layout, pos);
        if let Some(hv) = &hover {
            tooltip = Some(paint_hover(ui.ctx(), id, &layout, hv, pos));
        }
    }
    let shown = ChartShown { layout, rect, hover, tooltip };
    ui.ctx().data_mut(|d| d.insert_temp(id, shown.clone()));
    shown
}

/// The Table view: the chart's `ChartData` as a table, one row per category and one column per series
/// (per measure when there are several).
pub fn show_table(ui: &mut egui::Ui, data: &ChartData, opts: &LayoutOpts) {
    let tk = *theme::tokens(opts.dark);
    if let Some(r) = data.empty_reason() {
        ui.label(egui::RichText::new(format!("No data to chart: {}.", r)).color(tk.muted));
        return;
    }
    use micro_parquet::charts::axis::{value_text, ValueKind};
    let fmt = |v: Option<f64>| match v {
        Some(x) => value_text(x / opts.divisor, ValueKind::Amount, opts.decimals, false),
        None => String::new(),
    };
    egui::ScrollArea::both().auto_shrink([false, true]).show(ui, |ui| {
        egui::Grid::new(ui.id().with("chart_table")).striped(true).num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_XS)).show(ui, |ui| {
            ui.label(theme::semibold(if data.spec.category.is_empty() { "Category" } else { &data.spec.category }));
            let mut heads: Vec<(usize, usize)> = Vec::new();
            for m in 0..data.measure_count() {
                for s in 0..data.series.len().max(1) {
                    heads.push((m, s));
                }
            }
            for (m, s) in &heads {
                let name = data.measure_names.get(*m).cloned().unwrap_or_default();
                let ser = data.series.get(*s).cloned().unwrap_or_default();
                ui.label(theme::semibold(if ser.is_empty() { name } else { format!("{} \u{b7} {}", ser, name) }));
            }
            ui.label(theme::semibold("Rows"));
            ui.end_row();
            for (i, c) in data.categories.iter().enumerate() {
                ui.label(c);
                for (m, s) in &heads {
                    let v = data.measure(*m).and_then(|md| md.values.get(*s).and_then(|r| r.get(i)).copied().flatten());
                    ui.label(fmt(v));
                }
                let rows: u64 = data.rows.iter().map(|r| r.get(i).copied().unwrap_or(0)).sum();
                ui.label(micro_parquet::models::format_with_commas(rows as f64, 0));
                ui.end_row();
            }
        });
    });
}
