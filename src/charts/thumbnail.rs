//! The small drawn thumbnails of the chart gallery, in the house style (tokens only): one per form.

use super::spec::{ChartKind, Stack};
use super::tokens as t;
use egui::{pos2, vec2, Color32, Painter, Rect, Rounding, Shape, Stroke};

fn c(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// Draws a thumbnail of the form inside `r`.
pub fn paint_thumbnail(p: &Painter, r: Rect, kind: ChartKind, stack: Stack, horizontal: bool, dark: bool) {
    let ink = t::ink(dark);
    let s = |i: usize| c(t::series_colour(i, dark).unwrap_or(ink.deemph));
    p.rect_filled(r, Rounding::same(t::BAR_RADIUS_PX), c(ink.surface));
    let area = r.shrink(t::MIN_SPACE_PX);
    let (w, h) = (area.width(), area.height());
    let base_y = area.bottom();
    // the baseline and one gridline
    let line = |y: f32| p.line_segment([pos2(area.left(), y), pos2(area.right(), y)], Stroke::new(t::GRID_PX, c(ink.grid)));
    let frac = |f: f32| f;
    match kind {
        ChartKind::Bar => {
            let vals = [0.55f32, 0.9, 0.7, 0.4, 0.8];
            if horizontal {
                let bh = h / 6.0;
                for (i, v) in vals.iter().enumerate() {
                    let y = area.top() + (i as f32 + 0.5) * h / 5.0 - bh / 2.0;
                    p.rect_filled(Rect::from_min_size(pos2(area.left(), y), vec2(w * v, bh)), Rounding { nw: 0.0, sw: 0.0, ne: 2.0, se: 2.0 }, s(0));
                }
            } else {
                line(base_y - h / 2.0);
                let n = vals.len();
                let bw = w / (n as f32 * 2.0);
                for (i, v) in vals.iter().enumerate() {
                    let x = area.left() + (i as f32 + 0.5) * w / n as f32;
                    match stack {
                        Stack::Grouped => {
                            let hh = h * v;
                            p.rect_filled(Rect::from_min_size(pos2(x - bw / 2.0, base_y - hh), vec2(bw, hh)), Rounding { nw: 2.0, ne: 2.0, sw: 0.0, se: 0.0 }, s(0));
                        }
                        Stack::Stacked | Stack::Percent => {
                            let total = if stack == Stack::Percent { 1.0 } else { *v };
                            let parts = [0.5f32, 0.3, 0.2];
                            let mut y = base_y;
                            for (k, pf) in parts.iter().enumerate() {
                                let hh = h * total * pf;
                                p.rect_filled(Rect::from_min_size(pos2(x - bw / 2.0, y - hh + 1.0), vec2(bw, (hh - 1.0).max(1.0))), Rounding::ZERO, s(k));
                                y -= hh;
                            }
                        }
                    }
                }
            }
        }
        ChartKind::Line | ChartKind::Area => {
            line(base_y - h / 2.0);
            let pts: Vec<_> = [0.2f32, 0.45, 0.35, 0.7, 0.6, 0.9].iter().enumerate().map(|(i, v)| pos2(area.left() + i as f32 * w / 5.0, base_y - h * v)).collect();
            if kind == ChartKind::Area {
                let mut mesh = egui::epaint::Mesh::default();
                for k in 0..pts.len() - 1 {
                    let (a, b) = (pts[k], pts[k + 1]);
                    let col = Color32::from_rgba_unmultiplied(s(0).r(), s(0).g(), s(0).b(), (t::AREA_OPACITY * 255.0 * 2.0) as u8);
                    let i = mesh.vertices.len() as u32;
                    for v in [a, b, pos2(b.x, base_y), pos2(a.x, base_y)] {
                        mesh.colored_vertex(v, col);
                    }
                    mesh.add_triangle(i, i + 1, i + 2);
                    mesh.add_triangle(i, i + 2, i + 3);
                }
                p.add(Shape::mesh(mesh));
            }
            p.add(Shape::line(pts.clone(), Stroke::new(t::LINE_PX, s(0))));
            if let Some(last) = pts.last() {
                p.circle_filled(*last, t::MARKER_PX / 2.0, s(0));
            }
        }
        ChartKind::Scatter | ChartKind::Bubble => {
            line(base_y);
            let pts = [(0.1f32, 0.2f32), (0.25, 0.4), (0.4, 0.35), (0.5, 0.6), (0.7, 0.55), (0.85, 0.85), (0.6, 0.75), (0.3, 0.55)];
            for (i, (x, y)) in pts.iter().enumerate() {
                let rad = if kind == ChartKind::Bubble { 3.0 + (i % 4) as f32 * 2.0 } else { t::MARKER_PX / 2.0 };
                let col = if kind == ChartKind::Bubble { Color32::from_rgba_unmultiplied(s(0).r(), s(0).g(), s(0).b(), 170) } else { s(0) };
                p.circle_filled(pos2(area.left() + w * x, base_y - h * y), rad, col);
            }
        }
        ChartKind::Histogram => {
            let vals = [0.2f32, 0.45, 0.8, 1.0, 0.7, 0.4, 0.15];
            let n = vals.len();
            for (i, v) in vals.iter().enumerate() {
                let x = area.left() + i as f32 * w / n as f32;
                p.rect_filled(Rect::from_min_size(pos2(x + 1.0, base_y - h * v), vec2(w / n as f32 - t::SEGMENT_GAP_PX, h * v)), Rounding { nw: 2.0, ne: 2.0, sw: 0.0, se: 0.0 }, s(0));
            }
        }
        ChartKind::BoxPlot => {
            for (i, (lo, q1, md, q3, hi)) in [(0.1f32, 0.3f32, 0.45f32, 0.6f32, 0.8f32), (0.25, 0.45, 0.6, 0.75, 0.95), (0.05, 0.2, 0.3, 0.42, 0.55)].iter().enumerate() {
                let x = area.left() + (i as f32 + 0.5) * w / 3.0;
                let y = |f: f32| base_y - h * f;
                p.line_segment([pos2(x, y(*lo)), pos2(x, y(*hi))], Stroke::new(t::REFERENCE_LINE_PX, c(ink.ink2)));
                p.rect_filled(Rect::from_min_max(pos2(x - 8.0, y(*q3)), pos2(x + 8.0, y(*q1))), Rounding::same(2.0), s(0));
                p.line_segment([pos2(x - 8.0, y(*md)), pos2(x + 8.0, y(*md))], Stroke::new(t::LINE_PX, c(ink.surface)));
            }
        }
        ChartKind::Waterfall => {
            let steps = [(0.0f32, 0.6f32, 0), (0.6, 0.78, 1), (0.78, 0.7, 2), (0.7, 0.9, 1), (0.0, 0.9, 3)];
            let n = steps.len();
            let bw = w / (n as f32 * 1.8);
            for (i, (a, b, kind)) in steps.iter().enumerate() {
                let x = area.left() + (i as f32 + 0.5) * w / n as f32;
                let (lo, hi) = (a.min(*b), a.max(*b));
                let col = match kind {
                    0 => c(t::WATERFALL_START),
                    3 => c(t::WATERFALL_END),
                    1 => c(t::DIVERGING_POSITIVE),
                    _ => c(t::DIVERGING_NEGATIVE),
                };
                p.rect_filled(Rect::from_min_max(pos2(x - bw / 2.0, base_y - h * hi), pos2(x + bw / 2.0, base_y - h * lo)), Rounding::same(2.0), col);
            }
        }
        ChartKind::Heatmap => {
            let (rows, cols) = (4usize, 5usize);
            let (cw, ch) = (w / cols as f32, h / rows as f32);
            for rr in 0..rows {
                for cc in 0..cols {
                    let v = if rr == cc.min(rows - 1) { 12 } else { (cc + rr * 2) % 6 };
                    let cell = Rect::from_min_size(pos2(area.left() + cc as f32 * cw + 1.0, area.top() + rr as f32 * ch + 1.0), vec2(cw - 2.0, ch - 2.0));
                    p.rect_filled(cell, Rounding::same(t::HEAT_RADIUS_SHARE * cell.width().min(cell.height())), c(t::SEQUENTIAL[v.min(12)]));
                }
            }
        }
        ChartKind::Donut => {
            let centre = area.center();
            let (ro, ri) = (h.min(w) / 2.0, h.min(w) / 2.0 * 0.62);
            let parts = [0.5f32, 0.28, 0.14, 0.08];
            let mut a = 0.0f32;
            for (i, f) in parts.iter().enumerate() {
                let (a0, a1) = (a + 0.04, a + f * std::f32::consts::TAU - 0.04);
                a += f * std::f32::consts::TAU;
                let steps = ((a1 - a0) / 0.1).ceil().max(1.0) as usize;
                let mut mesh = egui::epaint::Mesh::default();
                let pt = |ang: f32, rad: f32| pos2(centre.x + rad * ang.sin(), centre.y - rad * ang.cos());
                for k in 0..steps {
                    let (x0, x1) = (a0 + (a1 - a0) * k as f32 / steps as f32, a0 + (a1 - a0) * (k + 1) as f32 / steps as f32);
                    let i0 = mesh.vertices.len() as u32;
                    for v in [pt(x0, ro), pt(x1, ro), pt(x1, ri), pt(x0, ri)] {
                        mesh.colored_vertex(v, s(i));
                    }
                    mesh.add_triangle(i0, i0 + 1, i0 + 2);
                    mesh.add_triangle(i0, i0 + 2, i0 + 3);
                }
                p.add(Shape::mesh(mesh));
            }
        }
        ChartKind::Sankey => {
            for (i, (y0, y1, hh)) in [(0.15f32, 0.1f32, 0.25f32), (0.55, 0.6, 0.12), (0.8, 0.3, 0.08)].iter().enumerate() {
                let steps = 16;
                let mut mesh = egui::epaint::Mesh::default();
                let col = Color32::from_rgba_unmultiplied(s(i).r(), s(i).g(), s(i).b(), 140);
                for k in 0..steps {
                    let (u0, u1) = (k as f32 / steps as f32, (k + 1) as f32 / steps as f32);
                    let ease = |u: f32| u * u * (3.0 - 2.0 * u);
                    let ya = |u: f32| area.top() + h * (y0 + (y1 - y0) * ease(u));
                    let i0 = mesh.vertices.len() as u32;
                    for v in [pos2(area.left() + w * u0, ya(u0)), pos2(area.left() + w * u1, ya(u1)), pos2(area.left() + w * u1, ya(u1) + h * hh), pos2(area.left() + w * u0, ya(u0) + h * hh)] {
                        mesh.colored_vertex(v, col);
                    }
                    mesh.add_triangle(i0, i0 + 1, i0 + 2);
                    mesh.add_triangle(i0, i0 + 2, i0 + 3);
                }
                p.add(Shape::mesh(mesh));
            }
        }
        ChartKind::Treemap => {
            let g = t::SEGMENT_GAP_PX;
            let cells = [(0.0f32, 0.0f32, 0.55f32, 1.0f32, 0usize), (0.55, 0.0, 0.45, 0.55, 1), (0.55, 0.55, 0.25, 0.45, 2), (0.8, 0.55, 0.2, 0.45, 3)];
            for (x, y, cw, ch, i) in cells {
                let cell = Rect::from_min_size(pos2(area.left() + w * x + g / 2.0, area.top() + h * y + g / 2.0), vec2(w * cw - g, h * ch - g));
                p.rect_filled(cell, Rounding::same(2.0), s(i));
            }
        }
    }
    let _ = frac(0.0);
}
