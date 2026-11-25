//! Renderer A of the chart studio (Phase 7 C1): the in-app painter. The geometry is worked out by
//! `micro_parquet::charts::layout` (one rectangle per mark, one per text, every collision checked);
//! this file draws it with the egui painter at the window's scale, in light and dark, and adds the
//! interaction: an outline and tooltip on the mark under the pointer, a crosshair with every series'
//! value on lines and areas, and the Table view. It is not egui_plot: egui_plot cannot meet the mark
//! specs (rounded data-ends, 2 px gaps, collision-checked labels).

use crate::ui::prelude::*;
use micro_parquet::charts::data::{ChartData, MarkRef};
use micro_parquet::charts::layout::{hit_target, hit_test, layout_chart, tooltip_rect, ChartLayout, Face, Hover, LayoutOpts, Measure, Prim, TOOLTIP_PADDING};
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
    /// What a click or the right-click menu asked for this frame (W6); the caller carries it out.
    pub action: Option<ChartAction>,
}

/// What a click on a mark may do where a chart is drawn (Phase 8 W6). `None` passed to `show_chart_with` means the chart is
/// only looked at, as before.
#[derive(Clone, Debug, Default)]
pub struct DrillCtx {
    /// The kind of chart can be clicked (a scatter, a histogram, a box plot and a waterfall cannot).
    pub kind_ok: bool,
    /// The chart has drill levels, so its tooltip says what a click does.
    pub has_drill: bool,
    /// A click goes one level down: there is a level below.
    pub can_down: bool,
    /// The level below, for the tooltip.
    pub next_level: String,
    /// Why "Show rows" cannot open a tab (shown on the greyed menu item); `None` when it can.
    pub rows_unavailable: Option<String>,
    /// Why nothing here can change the chart (a chart tab that cannot be calculated again): every menu item is greyed with it.
    pub blocked: Option<String>,
    /// The chart is an item on a dashboard page (Phase 10 D3): a click filters the page, and the menu has "Drill down" and
    /// "Filter the page by this" only (the numbers of the chart's tab are not changed from a page).
    pub page: bool,
}

/// What a click or a menu item asked for, with the mark it was on.
#[derive(Clone, Debug, PartialEq)]
pub enum ChartAction {
    DrillDown(MarkRef),
    ShowRows(MarkRef),
    Keep(MarkRef),
    Exclude(MarkRef),
    /// The colour picker for the item the mark stands for (Phase 8 W7).
    Colour(MarkRef),
    /// A click on a dashboard item: the page is filtered by this mark (Phase 10 D3).
    CrossFilter(MarkRef),
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
        let font = chart_font(t.pt, t.face);
        if t.angle != 0.0 {
            // an angled category label: turned about its start, the end of it at the tick
            let galley = painter.layout_no_wrap(t.text.clone(), font, t.colour);
            let pos = egui::pos2(t.rect.left(), t.rect.top() + galley.size().x * (-t.angle).sin());
            painter.add(egui::epaint::TextShape::new(pos, galley, t.colour).with_angle(t.angle));
        } else if t.text.contains('\n') {
            // a label on two lines: each line centred
            let mut job = egui::text::LayoutJob::single_section(t.text.clone(), egui::TextFormat::simple(font, t.colour));
            job.halign = egui::Align::Center;
            let galley = painter.layout_job(job);
            painter.galley(egui::pos2(t.rect.center().x, t.rect.top()), galley, t.colour);
        } else {
            painter.text(t.rect.left_top(), egui::Align2::LEFT_TOP, &t.text, font, t.colour);
        }
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
    show_chart_with(ui, id, data, opts, height, None)
}

/// `show_chart` with clicks: a click on a mark drills one level down, a right-click opens the menu (Drill down, Show rows,
/// Keep only this, Exclude this). What was asked for comes back in `ChartShown::action`.
pub fn show_chart_with(ui: &mut egui::Ui, id: egui::Id, data: &ChartData, opts: &LayoutOpts, height: Option<f32>, drill: Option<&DrillCtx>) -> ChartShown {
    let ctx = ui.ctx().clone();
    let m = EguiMeasure(&ctx);
    let width = ui.available_width().max(theme::CHART_MIN_W);
    let h = height.unwrap_or_else(|| ui.available_height().clamp(theme::CHART_MIN_H, theme::CHART_MAX_H));
    // small multiples can need more height than they were given
    let probe = layout_chart(data, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, h)), opts, &m);
    let h = probe.needed_height.max(h);
    let live = drill.filter(|d| d.kind_ok);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, h), if live.is_some() { egui::Sense::click() } else { egui::Sense::hover() });
    let layout = layout_chart(data, rect, opts, &m);
    paint_layout(&ui.painter_at(rect), &layout);
    let mut hover = None;
    let mut tooltip = None;
    let mut action: Option<ChartAction> = None;
    let target_at = |pos: egui::Pos2| hit_target(&layout, pos).and_then(|t| data.mark_ref(t.panel, t.series, t.category));
    if let Some(pos) = ui.input(|i| i.pointer.hover_pos()).filter(|p| resp.hovered() && rect.contains(*p)) {
        hover = hit_test(&layout, pos);
        if let Some(hv) = &hover {
            // a chart with drill levels says what a click does (or that it does nothing at the last level)
            let mut hv = hv.clone();
            if live.is_some_and(|d| d.page) {
                hv.tip.push_str("\n\u{2192} Click to filter the page by this");
                if let Some(dc) = live.filter(|d| d.has_drill && d.can_down) {
                    hv.tip.push_str(&format!(" \u{b7} right-click to drill into {}", dc.next_level));
                }
            } else if let Some(dc) = live.filter(|d| d.has_drill) {
                hv.tip.push_str(&if dc.can_down { format!("\n\u{2192} Click to drill into {}", dc.next_level) } else { "\n\u{2192} Last level: a click does nothing".to_string() });
            }
            if live.is_some_and(|d| d.page || d.can_down) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            tooltip = Some(paint_hover(ui.ctx(), id, &layout, &hv, pos));
        }
    }
    if let Some(dc) = live {
        if resp.clicked_by(egui::PointerButton::Primary) && (dc.can_down || dc.page) {
            if let Some(mark) = resp.interact_pointer_pos().and_then(&target_at) {
                action = Some(if dc.page { ChartAction::CrossFilter(mark) } else { ChartAction::DrillDown(mark) });
            }
        }
        // the mark under the pointer when the menu was asked for stays the menu's mark while the pointer moves over it
        let menu_id = id.with("drill_menu_mark");
        if resp.secondary_clicked() {
            let at = resp.interact_pointer_pos().and_then(&target_at);
            ui.ctx().data_mut(|d| d.insert_temp(menu_id, at));
        }
        resp.context_menu(|ui| {
            let Some(mark) = ui.ctx().data(|d| d.get_temp::<Option<MarkRef>>(menu_id)).flatten() else {
                ui.close_menu();
                return;
            };
            let (down_ok, down_why) = if dc.can_down {
                (true, String::new())
            } else if dc.has_drill {
                (false, "This is the last level: there is nothing below it.".to_string())
            } else {
                (false, "This chart has no drill levels. Add them under Drill levels in the chart studio.".to_string())
            };
            let blocked = dc.blocked.clone();
            let mut item = |ui: &mut egui::Ui, label: &str, enabled: bool, why: &str, what: ChartAction| {
                let (enabled, why) = match &blocked {
                    Some(b) => (false, b.as_str()),
                    None => (enabled, why),
                };
                let b = ui.add_enabled(enabled, egui::Button::new(label));
                let b = if enabled { b } else { b.on_disabled_hover_text(why) };
                if b.clicked() {
                    action = Some(what);
                    ui.close_menu();
                }
            };
            if dc.page {
                item(ui, "Filter the page by this", true, "", ChartAction::CrossFilter(mark.clone()));
                item(ui, "Drill down", down_ok, &down_why, ChartAction::DrillDown(mark.clone()));
                return;
            }
            item(ui, "Drill down", down_ok, &down_why, ChartAction::DrillDown(mark.clone()));
            item(ui, "Show rows", dc.rows_unavailable.is_none(), dc.rows_unavailable.as_deref().unwrap_or(""), ChartAction::ShowRows(mark.clone()));
            item(ui, "Keep only this", true, "", ChartAction::Keep(mark.clone()));
            item(ui, "Exclude this", true, "", ChartAction::Exclude(mark.clone()));
            // the colour of an item does not change the numbers, so it is not held back by a chart that cannot be calculated again
            if ui.button("Colour\u{2026}").on_hover_text("Pick the colour of this item (a series, or a part of a donut)").clicked() {
                action = Some(ChartAction::Colour(mark.clone()));
                ui.close_menu();
            }
        });
    }
    let shown = ChartShown { layout, rect, hover, tooltip, action };
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
