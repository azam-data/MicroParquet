//! The in-app chart layout (Phase 7 C1): geometry only. `layout_chart` turns a `ChartData` and a
//! rectangle into a `ChartLayout`: a list of primitives to paint, the marks (for hover), every text
//! with its rectangle (for the collision checks) and a report of each panel. The egui painter in
//! `ui/chart_render.rs` draws it; the tests assert on it directly. Every rule of the design system
//! (docs/phase7-spec.md) is applied here: bars at most 24 px, 4 px rounded data-ends, 2 px gaps,
//! one zero baseline, horizontal-only grids, selective labels with collision checks, the legend rules.

use super::axis::{fit_text, nice_ticks, tick_text, value_text, wrap_text, ValueKind, MINUS};
use super::colours::{Colour, ColourMap};
use super::data::{heatmap, sequential_step, waterfall_from_data, ChartData, StepKind, WaterfallStep};
use super::spec::{ChartAgg, ChartKind, ChartSpec, Stack, Status, Transform};
use super::tokens as t;
use egui::{pos2, vec2, Color32, Pos2, Rect, Vec2};

// ---- layout numbers that are not part of the design system's palette or marks ----
/// The share of a category band the bars of one category may fill.
const BAND_FILL: f32 = 0.8;
const BOX_FILL: f32 = 0.6;
const HEADER_GAP: f32 = 4.0;
const SECTION_GAP: f32 = 8.0;
const PANEL_HEADER_H: f32 = 18.0;
const PANEL_GAP: f32 = 20.0;
const MARKERS_MAX_POINTS: usize = 40;
const RING_MAX_POINTS: usize = 600;
const LABEL_MAX_BARS: usize = 12;
const BUBBLE_MIN_R: f32 = 6.0;
const BUBBLE_MAX_R: f32 = 22.0;
const BUBBLE_ALPHA: f32 = 0.7;
const DONUT_HOLE: f32 = 0.62;
const SMALL_PANEL_W: f32 = 190.0;
const SMALL_PANEL_H: f32 = 140.0;
const LINKED_PANEL_H: f32 = 130.0;
const TOOLTIP_PAD: f32 = 6.0;
const MAX_CAT_LABEL_W_SHARE: f32 = 0.34;
const MIN_BODY_H: f32 = 70.0;

/// Measures text. The UI gives the real fonts; the library tests an estimate.
pub trait Measure {
    fn size(&self, text: &str, pt: f32, semibold: bool) -> Vec2;
}

/// A rough measure for tests that need no fonts: half an em per character.
pub struct ApproxMeasure;
impl Measure for ApproxMeasure {
    fn size(&self, text: &str, pt: f32, _semibold: bool) -> Vec2 {
        vec2(text.chars().count() as f32 * pt * 0.5, pt * 1.3)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Regular,
    Semibold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Title,
    Subtitle,
    Footnote,
    Tick,
    Axis,
    Category,
    /// A value written on or beside a mark.
    Label,
    /// A series name at the end (or start) of its line.
    Direct,
    Legend,
    /// The text of a reference line.
    Reference,
    /// The value inside a heatmap cell.
    Cell,
    Panel,
    Message,
    Centre,
}

/// One text and the rectangle it occupies.
#[derive(Clone, Debug, PartialEq)]
pub struct LText {
    pub text: String,
    pub rect: Rect,
    pub pt: f32,
    pub face: Face,
    pub colour: Color32,
    pub role: Role,
}

/// Something to paint, in screen coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    /// `round` is [nw, ne, sw, se].
    Rect { rect: Rect, fill: Color32, round: [f32; 4] },
    Line { a: Pos2, b: Pos2, w: f32, col: Color32 },
    /// Filled triangles (areas, donut slices): no anti-aliased seams.
    Tris { tris: Vec<[Pos2; 3]>, fill: Color32 },
    /// A disc, with an optional ring (width, colour) outside it.
    Circle { c: Pos2, r: f32, fill: Color32, ring: Option<(f32, Color32)> },
    /// A circle outline (a breach ring).
    Outline { c: Pos2, r: f32, w: f32, col: Color32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    Bar,
    Segment,
    Point,
    Cell,
    Slice,
    Box,
}

/// The shape of a donut slice, for hit-testing by angle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliceShape {
    pub centre: Pos2,
    pub r_in: f32,
    pub r_out: f32,
    /// Radians, clockwise from 12 o'clock.
    pub a0: f32,
    pub a1: f32,
}

/// A thing the pointer can hover: its rectangle and what its tooltip says.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub rect: Rect,
    pub kind: MarkKind,
    pub panel: usize,
    pub series: usize,
    pub category: usize,
    pub value: Option<f64>,
    pub tip: String,
    pub slice: Option<SliceShape>,
}

/// A vertical crosshair at one category of a line or area chart, with every series' value in its tip.
#[derive(Clone, Debug, PartialEq)]
pub struct Crosshair {
    pub x: f32,
    pub y0: f32,
    pub y1: f32,
    pub panel: usize,
    pub category: usize,
    pub tip: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegendItem {
    pub name: String,
    pub swatch: Rect,
    pub text: Rect,
    pub colour: Color32,
}

/// What each panel drew: where, over what value range, and its ticks.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelLayout {
    pub kind: ChartKind,
    pub title: String,
    pub plot: Rect,
    pub lo: f64,
    pub hi: f64,
    pub horizontal: bool,
    /// The tick values and where they were drawn (y for columns, x for horizontal bars).
    pub ticks: Vec<(f64, f32)>,
}

impl PanelLayout {
    /// The screen coordinate of a value along the value axis (y for columns, x for horizontal bars).
    pub fn coord(&self, v: f64) -> f32 {
        let f = ((v - self.lo) / (self.hi - self.lo)) as f32;
        if self.horizontal {
            self.plot.left() + f * self.plot.width()
        } else {
            self.plot.bottom() - f * self.plot.height()
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartLayout {
    pub rect: Rect,
    pub prims: Vec<Prim>,
    pub texts: Vec<LText>,
    pub marks: Vec<Mark>,
    pub crosshairs: Vec<Crosshair>,
    pub legend: Vec<LegendItem>,
    pub panels: Vec<PanelLayout>,
    pub title: Option<Rect>,
    pub subtitle: Option<Rect>,
    pub footnote: Option<Rect>,
    /// "No data to chart: …" or the reason the chart is refused; nothing else is drawn but the title.
    pub message: Option<String>,
    /// The colour map with every series of this chart in it (the caller stores it in the spec).
    pub colour_map: ColourMap,
    /// The size the chart needs (small multiples can need more height than they were given).
    pub needed_height: f32,
    pub surface: Color32,
    pub dark: bool,
}

impl ChartLayout {
    fn empty(rect: Rect, surface: Color32, dark: bool) -> ChartLayout {
        ChartLayout {
            rect,
            prims: Vec::new(),
            texts: Vec::new(),
            marks: Vec::new(),
            crosshairs: Vec::new(),
            legend: Vec::new(),
            panels: Vec::new(),
            title: None,
            subtitle: None,
            footnote: None,
            message: None,
            colour_map: ColourMap::default(),
            needed_height: rect.height(),
            surface,
            dark,
        }
    }
}

/// How the numbers are written.
#[derive(Clone, Debug)]
pub struct LayoutOpts {
    pub dark: bool,
    /// Amounts are divided by this (the chart's scale).
    pub divisor: f64,
    pub decimals: usize,
    /// The scale and currency for the subtitle: "BDT Crore".
    pub unit: String,
    /// Only the plot and its legend: no title, subtitle or footnote (the Phase 6 quick chart has its own).
    pub bare: bool,
    /// The colour map stored in the live spec (the data's own copy is from the time it was calculated).
    pub colours: Option<ColourMap>,
}

impl Default for LayoutOpts {
    fn default() -> Self {
        LayoutOpts { dark: false, divisor: 1.0, decimals: 2, unit: String::new(), bare: false, colours: None }
    }
}

fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn inflate(r: Rect, by: f32) -> Rect {
    r.expand(by)
}

// ----------------------------------------------------------------------------------------- builder

#[derive(Clone, Copy)]
enum Anchor {
    TopLeft,
    TopCentre,
    TopRight,
    MidLeft,
    MidRight,
    Centre,
    BottomLeft,
    BottomCentre,
    BottomRight,
}

struct Builder<'a> {
    m: &'a dyn Measure,
    ink: t::ChartInk,
    dark: bool,
    bounds: Rect,
    out: ChartLayout,
}

impl<'a> Builder<'a> {
    fn size(&self, s: &str, pt: f32, face: Face) -> Vec2 {
        self.m.size(&t::delta(s), pt, face == Face::Semibold)
    }

    fn width_fn(&self, pt: f32, face: Face) -> impl Fn(&str) -> f32 + 'a {
        let m = self.m;
        move |s: &str| m.size(&t::delta(s), pt, face == Face::Semibold).x
    }

    fn rect_for(&self, s: &str, pt: f32, face: Face, anchor: Anchor, at: Pos2) -> Rect {
        let sz = self.size(s, pt, face);
        let (x, y) = match anchor {
            Anchor::TopLeft => (at.x, at.y),
            Anchor::TopCentre => (at.x - sz.x / 2.0, at.y),
            Anchor::TopRight => (at.x - sz.x, at.y),
            Anchor::MidLeft => (at.x, at.y - sz.y / 2.0),
            Anchor::MidRight => (at.x - sz.x, at.y - sz.y / 2.0),
            Anchor::Centre => (at.x - sz.x / 2.0, at.y - sz.y / 2.0),
            Anchor::BottomLeft => (at.x, at.y - sz.y),
            Anchor::BottomCentre => (at.x - sz.x / 2.0, at.y - sz.y),
            Anchor::BottomRight => (at.x - sz.x, at.y - sz.y),
        };
        Rect::from_min_size(pos2(x, y), sz)
    }

    /// Writes a text without any check.
    fn put(&mut self, role: Role, s: &str, anchor: Anchor, at: Pos2, pt: f32, face: Face, colour: Color32) -> Rect {
        let rect = self.rect_for(s, pt, face, anchor, at);
        self.out.texts.push(LText { text: t::delta(s), rect, pt, face, colour, role });
        rect
    }

    /// True when `r` is inside the chart, touches no other text and no mark.
    fn free(&self, r: Rect) -> bool {
        if r.left() < self.bounds.left() || r.right() > self.bounds.right() || r.top() < self.bounds.top() || r.bottom() > self.bounds.bottom() {
            return false;
        }
        let pad = inflate(r, 1.0);
        !self.out.texts.iter().any(|x| x.rect.intersects(pad)) && !self.out.marks.iter().any(|m| m.rect.intersects(r))
    }

    /// Writes a text only where it fits (inside the chart, clear of every text and mark); else `None`:
    /// the value stays in the tooltip.
    fn try_put(&mut self, role: Role, s: &str, anchor: Anchor, at: Pos2, pt: f32, face: Face, colour: Color32) -> Option<Rect> {
        let r = self.rect_for(s, pt, face, anchor, at);
        if self.free(r) {
            Some(self.put(role, s, anchor, at, pt, face, colour))
        } else {
            None
        }
    }

    fn line(&mut self, a: Pos2, b: Pos2, w: f32, col: Color32) {
        self.out.prims.push(Prim::Line { a, b, w, col });
    }

    fn rect(&mut self, rect: Rect, fill: Color32, round: [f32; 4]) {
        self.out.prims.push(Prim::Rect { rect, fill, round });
    }
}

// --------------------------------------------------------------------------------------- panel plan

/// One series of one panel.
#[derive(Clone)]
struct Ser {
    name: String,
    vals: Vec<Option<f64>>,
    rows: Vec<u64>,
    /// What each share was a share of (for the tooltip), when the measure is a percentage.
    base: Vec<Option<f64>>,
    colour: Color32,
}

/// A panel to draw: a form over some measure of the chart.
struct Plan<'a> {
    kind: ChartKind,
    title: String,
    measure: usize,
    vk: ValueKind,
    series: Vec<Ser>,
    categories: &'a [String],
    /// Facet or panel number, for the marks.
    index: usize,
}

pub(crate) fn value_kind(spec: &ChartSpec, measure: usize) -> ValueKind {
    match spec.kind {
        ChartKind::Histogram => return ValueKind::Count,
        ChartKind::BoxPlot => return ValueKind::Plain,
        _ => {}
    }
    let ml = spec.measure_list();
    let m = ml.get(measure).or(ml.first());
    match m {
        Some(m) if matches!(m.transform, Transform::ShareOfTotal | Transform::ShareOfCategory | Transform::ShareOfSeries) || m.pct => ValueKind::Percent,
        Some(m) if m.agg == ChartAgg::Count => ValueKind::Count,
        Some(m) if m.agg == ChartAgg::WeightedAvg || m.agg == ChartAgg::Average => ValueKind::Plain,
        _ => ValueKind::Amount,
    }
}

pub(crate) fn scaled(v: f64, vk: ValueKind, opts: &LayoutOpts) -> f64 {
    if vk == ValueKind::Amount {
        v / opts.divisor
    } else {
        v
    }
}

/// The title when none is typed: descriptive ("Sum of Balance by Term Period").
pub fn auto_title(d: &ChartData) -> String {
    let s = &d.spec;
    let name = d.measure_names.first().cloned().unwrap_or_default();
    match s.kind {
        ChartKind::Histogram => format!("Distribution of {}", s.measure_list().first().and_then(|m| m.column.clone()).unwrap_or_else(|| s.category.clone())),
        ChartKind::Scatter | ChartKind::Bubble => format!("{} against {}", s.measure_list().first().and_then(|m| m.column.clone()).unwrap_or_default(), s.category),
        ChartKind::BoxPlot => format!("{} by {}", s.measure_list().first().and_then(|m| m.column.clone()).unwrap_or_default(), s.category),
        _ => {
            let by = match &s.series {
                Some(x) if !x.is_empty() => format!("{} and {}", s.category, x),
                _ => s.category.clone(),
            };
            if d.measure_names.len() > 1 && s.panels.is_empty() {
                format!("{} by {}", d.measure_names.join(" and "), by)
            } else {
                format!("{} by {}", name, by)
            }
        }
    }
}

/// The title the chart shows: the takeaway, else the typed title, else the auto-title.
pub fn chart_title(d: &ChartData) -> String {
    if !d.spec.takeaway.trim().is_empty() {
        d.spec.takeaway.trim().to_string()
    } else if !d.spec.title.trim().is_empty() {
        d.spec.title.trim().to_string()
    } else {
        auto_title(d)
    }
}

impl ChartLayout {
    /// The layout as JSON (primitives and texts), for the screenshot tool `tools/chart_layout_png.py`.
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        let col = |c: Color32| json!([c.r(), c.g(), c.b(), c.a()]);
        let rc = |r: Rect| json!([r.left(), r.top(), r.right(), r.bottom()]);
        let pt = |p: Pos2| json!([p.x, p.y]);
        let prims: Vec<serde_json::Value> = self
            .prims
            .iter()
            .map(|p| match p {
                Prim::Rect { rect, fill, round } => json!({"k": "rect", "r": rc(*rect), "fill": col(*fill), "round": round}),
                Prim::Line { a, b, w, col: c } => json!({"k": "line", "a": pt(*a), "b": pt(*b), "w": w, "col": col(*c)}),
                Prim::Tris { tris, fill } => json!({"k": "tris", "tris": tris.iter().map(|t| json!([pt(t[0]), pt(t[1]), pt(t[2])])).collect::<Vec<_>>(), "fill": col(*fill)}),
                Prim::Circle { c, r, fill, ring } => json!({"k": "circle", "c": pt(*c), "r": r, "fill": col(*fill), "ring": ring.map(|(w, rc_)| json!({"w": w, "col": col(rc_)}))}),
                Prim::Outline { c, r, w, col: cl } => json!({"k": "outline", "c": pt(*c), "r": r, "w": w, "col": col(*cl)}),
            })
            .collect();
        let texts: Vec<serde_json::Value> = self
            .texts
            .iter()
            .map(|t| json!({"text": t.text, "r": rc(t.rect), "pt": t.pt, "semibold": t.face == Face::Semibold, "col": col(t.colour)}))
            .collect();
        json!({"rect": rc(self.rect), "height": self.needed_height.max(self.rect.height()), "surface": col(self.surface), "prims": prims, "texts": texts})
    }
}

// ------------------------------------------------------------------------------------- the entry

/// Lays the chart out in `rect`.
pub fn layout_chart(data: &ChartData, rect: Rect, opts: &LayoutOpts, m: &dyn Measure) -> ChartLayout {
    let mut out = layout_chart_inner(data, rect, opts, m);
    for mk in out.marks.iter_mut() {
        mk.tip = t::delta(&mk.tip);
    }
    for c in out.crosshairs.iter_mut() {
        c.tip = t::delta(&c.tip);
    }
    out
}

fn layout_chart_inner(data: &ChartData, rect: Rect, opts: &LayoutOpts, m: &dyn Measure) -> ChartLayout {
    let ink = t::ink(opts.dark);
    let mut b = Builder { m, ink, dark: opts.dark, bounds: rect, out: ChartLayout::empty(rect, rgb(ink.surface), opts.dark) };
    let pad = (rect.width() * t::PADDING_SHARE).max(t::MIN_SPACE_PX);
    let outer = rect.shrink(pad);
    if outer.width() < 60.0 || outer.height() < 60.0 {
        b.out.message = Some("The chart is too small to draw. Make the window or the panel larger.".to_string());
        return b.out;
    }

    // title, subtitle
    let ink_c = rgb(ink.ink);
    let ink2_c = rgb(ink.ink2);
    let muted_c = rgb(ink.muted);
    let mut y = outer.top();
    let title = if opts.bare { String::new() } else { chart_title(data) };
    let tw = b.width_fn(t::TITLE_PT, Face::Semibold);
    let lines = if opts.bare { Vec::new() } else { wrap_text(&title, outer.width(), &tw) };
    let mut tr: Option<Rect> = None;
    for l in lines.iter().take(2) {
        let shown = if lines.len() > 2 || b.size(l, t::TITLE_PT, Face::Semibold).x > outer.width() { fit_text(l, outer.width(), &b.width_fn(t::TITLE_PT, Face::Semibold)) } else { l.clone() };
        let r = b.put(Role::Title, &shown, Anchor::TopLeft, pos2(outer.left(), y), t::TITLE_PT, Face::Semibold, ink_c);
        y = r.bottom();
        tr = Some(tr.map_or(r, |q| q.union(r)));
    }
    b.out.title = tr;
    if !opts.bare {
        y += HEADER_GAP;
    }
    let subtitle = if opts.bare {
        String::new()
    } else if !data.spec.subtitle.trim().is_empty() {
        data.spec.subtitle.trim().to_string()
    } else {
        let names = if data.measure_names.is_empty() { String::new() } else { data.measure_names.join(", ") };
        match (names.is_empty(), opts.unit.is_empty()) {
            (false, false) => format!("{} \u{b7} {}", names, opts.unit),
            (false, true) => names,
            (true, false) => opts.unit.clone(),
            (true, true) => String::new(),
        }
    };
    if !subtitle.is_empty() {
        let sw = b.width_fn(t::SUBTITLE_PT, Face::Regular);
        let shown = fit_text(&subtitle, outer.width(), &sw);
        let r = b.put(Role::Subtitle, &shown, Anchor::TopLeft, pos2(outer.left(), y), t::SUBTITLE_PT, Face::Regular, ink2_c);
        b.out.subtitle = Some(r);
        y = r.bottom() + SECTION_GAP;
    }

    // footnote at the bottom: the typed footnote, then the notes of the calculation
    let mut foot: Vec<String> = Vec::new();
    if !opts.bare && !data.spec.footnote.trim().is_empty() {
        foot.push(data.spec.footnote.trim().to_string());
    }
    if !opts.bare {
        foot.extend(data.notes.iter().cloned());
    }
    let mut foot_top = outer.bottom();
    if !foot.is_empty() {
        let fw = b.width_fn(t::FOOTNOTE_PT, Face::Regular);
        let mut lines = wrap_text(&foot.join(" \u{b7} "), outer.width(), &fw);
        if lines.len() > 3 {
            lines.truncate(3);
            if let Some(l) = lines.last_mut() {
                *l = fit_text(&format!("{}\u{2026}", l), outer.width(), &fw);
            }
        }
        let lh = b.size("Ag", t::FOOTNOTE_PT, Face::Regular).y;
        foot_top = outer.bottom() - lh * lines.len() as f32;
        let mut fr: Option<Rect> = None;
        for (i, l) in lines.iter().enumerate() {
            let r = b.put(Role::Footnote, l, Anchor::TopLeft, pos2(outer.left(), foot_top + lh * i as f32), t::FOOTNOTE_PT, Face::Regular, muted_c);
            fr = Some(fr.map_or(r, |q| q.union(r)));
        }
        b.out.footnote = fr;
    }
    let body_bottom = if b.out.footnote.is_some() { foot_top - SECTION_GAP } else { outer.bottom() };

    // the reasons there is nothing to draw
    if data.spec.kind.html_only() {
        b.out.message = Some(format!("{} charts are drawn in a web page: click \"Preview in browser\" in the Chart tab, or use Export \u{25b8} Chart to HTML. Choose another form to see this chart here.", data.spec.kind.label()));
    } else if let Some(r) = data.empty_reason() {
        b.out.message = Some(format!("No data to chart: {}.", r));
    }
    let body = Rect::from_min_max(pos2(outer.left(), y), pos2(outer.right(), body_bottom));
    if b.out.message.is_none() && body.height() < MIN_BODY_H {
        b.out.message = Some("The chart is too small to draw. Make the window or the panel larger.".to_string());
    }
    if let Some(msg) = b.out.message.clone() {
        let w = b.width_fn(t::SUBTITLE_PT, Face::Regular);
        let lines = wrap_text(&msg, body.width().min(520.0), &w);
        let lh = b.size("Ag", t::SUBTITLE_PT, Face::Regular).y;
        let top = body.center().y - lh * lines.len() as f32 / 2.0;
        for (i, l) in lines.iter().enumerate() {
            b.put(Role::Message, l, Anchor::TopCentre, pos2(body.center().x, top + lh * i as f32), t::SUBTITLE_PT, Face::Regular, ink2_c);
        }
        return b.out;
    }

    // colours: every series of the chart takes its slot (the stored map is kept)
    let mut cm = opts.colours.clone().unwrap_or_else(|| data.spec.colour_map.clone());
    let highlight = data.spec.highlight.clone().filter(|h| !h.is_empty());
    let named: Vec<String> = data.series.iter().filter(|s| !s.is_empty()).cloned().collect();
    cm.assign_all(&named);
    let series_colour = |name: &str, i: usize, n: usize, cm: &ColourMap| -> Color32 {
        let base = if name.is_empty() && n == 1 {
            Colour::Slot(0)
        } else if name.is_empty() {
            Colour::Slot(i.min(t::SLOTS - 1))
        } else {
            cm.get(name).unwrap_or(Colour::Slot(i.min(t::SLOTS - 1)))
        };
        let c = rgb(base.hex(opts.dark));
        match &highlight {
            Some(h) if n > 1 && name != h => rgb(ink.deemph),
            _ => c,
        }
    };

    let kind = data.spec.kind;
    match kind {
        ChartKind::Heatmap => heatmap_form(&mut b, data, body, opts),
        ChartKind::Scatter | ChartKind::Bubble => scatter_form(&mut b, data, body, opts, &cm),
        ChartKind::Donut => donut_form(&mut b, data, body, opts, &mut cm),
        _ => {
            // the panels: the spec's linked panels, one per measure when there are several, or one
            let ml = data.spec.measure_list();
            let mut plans: Vec<Plan> = Vec::new();
            let n_series = data.series.len();
            let mk_series = |measure: usize| -> Vec<Ser> {
                let Some(md) = data.measure(measure) else { return Vec::new() };
                let bases = share_bases(&md.values, ml.get(measure).map(|m| m.transform).unwrap_or(Transform::None));
                (0..n_series)
                    .map(|i| Ser {
                        name: data.series.get(i).cloned().unwrap_or_default(),
                        vals: md.values.get(i).cloned().unwrap_or_default(),
                        rows: md.rows.get(i).cloned().unwrap_or_default(),
                        base: bases.get(i).cloned().unwrap_or_default(),
                        colour: series_colour(data.series.get(i).map(String::as_str).unwrap_or(""), i, n_series, &cm),
                    })
                    .collect()
            };
            if !data.facets.is_empty() {
                for (fi, f) in data.facets.iter().enumerate() {
                    let fd = &f.data;
                    let bases = share_bases(&fd.values, ml.first().map(|m| m.transform).unwrap_or(Transform::None));
                    let ns = fd.series.len();
                    let series = (0..ns)
                        .map(|i| Ser {
                            name: fd.series[i].clone(),
                            vals: fd.values.get(i).cloned().unwrap_or_default(),
                            rows: fd.rows.get(i).cloned().unwrap_or_default(),
                            base: bases.get(i).cloned().unwrap_or_default(),
                            colour: series_colour(&fd.series[i], data.series.iter().position(|x| *x == fd.series[i]).unwrap_or(i), n_series.max(ns), &cm),
                        })
                        .collect();
                    plans.push(Plan { kind, title: f.name.clone(), measure: 0, vk: value_kind(&data.spec, 0), series, categories: &fd.categories, index: fi });
                }
            } else if !data.spec.panels.is_empty() {
                for (pi, p) in data.spec.panels.iter().take(t::MAX_PANELS).enumerate() {
                    let mi = p.measures.first().copied().unwrap_or(0).min(data.measure_count().saturating_sub(1));
                    let title = if p.title.is_empty() { data.measure_names.get(mi).cloned().unwrap_or_default() } else { p.title.clone() };
                    plans.push(Plan { kind: p.kind, title, measure: mi, vk: value_kind(&data.spec, mi), series: mk_series(mi), categories: &data.categories, index: pi });
                }
            } else if data.measure_count() > 1 && !matches!(kind, ChartKind::Waterfall | ChartKind::Histogram | ChartKind::BoxPlot | ChartKind::Scatter | ChartKind::Bubble) {
                for mi in 0..data.measure_count().min(t::MAX_PANELS) {
                    plans.push(Plan { kind, title: data.measure_names.get(mi).cloned().unwrap_or_default(), measure: mi, vk: value_kind(&data.spec, mi), series: mk_series(mi), categories: &data.categories, index: mi });
                }
            } else {
                plans.push(Plan { kind, title: String::new(), measure: 0, vk: value_kind(&data.spec, 0), series: mk_series(0), categories: &data.categories, index: 0 });
            }
            panels_form(&mut b, data, &plans, body, opts, &cm);
        }
    }
    // legend (above the plots: added after the plan so the plots start below it)
    b.out.colour_map = cm;
    b.out
}

/// What each share was a share of: `[series][category]`.
fn share_bases(values: &[Vec<Option<f64>>], t: Transform) -> Vec<Vec<Option<f64>>> {
    if !matches!(t, Transform::ShareOfTotal | Transform::ShareOfCategory | Transform::ShareOfSeries) {
        return Vec::new();
    }
    // the values here are already shares; the base is not kept, so the tip names the kind of base
    values.iter().map(|r| r.iter().map(|_| None).collect()).collect()
}

pub(crate) fn base_word(t: Transform) -> &'static str {
    match t {
        Transform::ShareOfTotal => "of the total",
        Transform::ShareOfCategory => "of the category total",
        Transform::ShareOfSeries => "of the row total",
        _ => "",
    }
}

// --------------------------------------------------------------------------------- legend and axes

/// A value axis: round ticks and their text.
struct Axis {
    lo: f64,
    hi: f64,
    ticks: Vec<f64>,
    step: f64,
    vk: ValueKind,
}

fn make_axis(lo: f64, hi: f64, include_zero: bool, vk: ValueKind, max_ticks: usize) -> Axis {
    let (mut lo, mut hi) = (lo, hi);
    if include_zero {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
    }
    let (ticks, step) = nice_ticks(lo, hi, max_ticks);
    let (l, h) = (*ticks.first().unwrap_or(&0.0), *ticks.last().unwrap_or(&1.0));
    Axis { lo: l, hi: h, ticks, step, vk }
}

fn tick_labels(ax: &Axis) -> Vec<String> {
    ax.ticks.iter().map(|v| tick_text(*v, ax.step, ax.vk)).collect()
}

/// Draws the legend row above the plots; returns the y below it.
fn legend_row(b: &mut Builder, names: &[(String, Color32)], area: Rect) -> f32 {
    let ink2 = rgb(b.ink.ink2);
    let sw = t::MARKER_PX + 2.0;
    let mut x = area.left();
    let mut y = area.top();
    let lh = b.size("Ag", t::LABEL_PT, Face::Regular).y.max(sw);
    for (name, colour) in names {
        let w = b.size(name, t::LABEL_PT, Face::Regular).x;
        if x + sw + 6.0 + w > area.right() && x > area.left() {
            x = area.left();
            y += lh + 4.0;
        }
        let swatch = Rect::from_min_size(pos2(x, y + (lh - sw) / 2.0), vec2(sw, sw));
        b.rect(swatch, *colour, [2.0; 4]);
        let tr = b.put(Role::Legend, name, Anchor::MidLeft, pos2(x + sw + 6.0, y + lh / 2.0), t::LABEL_PT, Face::Regular, ink2);
        b.out.legend.push(LegendItem { name: name.clone(), swatch, text: tr, colour: *colour });
        x += sw + 6.0 + w + t::MIN_SPACE_PX * 2.0;
    }
    y + lh + SECTION_GAP
}

// ----------------------------------------------------------------------------- the cartesian panels

/// The values a plan puts on its value axis: the extent of the data for its form.
fn extent(plan: &Plan, stack: Stack) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let n = plan.categories.len();
    let stacked = matches!(plan.kind, ChartKind::Bar | ChartKind::Area) && matches!(stack, Stack::Stacked | Stack::Percent);
    if stacked {
        if stack == Stack::Percent {
            return (0.0, 100.0);
        }
        for c in 0..n {
            let (mut p, mut q) = (0.0, 0.0);
            for s in &plan.series {
                if let Some(Some(v)) = s.vals.get(c) {
                    if *v >= 0.0 {
                        p += v;
                    } else {
                        q += v;
                    }
                }
            }
            hi = hi.max(p);
            lo = lo.min(q);
        }
    } else {
        for s in &plan.series {
            for v in s.vals.iter().flatten() {
                lo = lo.min(*v);
                hi = hi.max(*v);
            }
        }
    }
    if !lo.is_finite() || !hi.is_finite() {
        return (0.0, 1.0);
    }
    (lo, hi)
}

fn includes_zero(kind: ChartKind) -> bool {
    matches!(kind, ChartKind::Bar | ChartKind::Area | ChartKind::Histogram | ChartKind::Waterfall)
}

/// All panels, stacked (linked panels) or in a grid (small multiples), under one legend.
fn panels_form(b: &mut Builder, data: &ChartData, plans: &[Plan], body: Rect, opts: &LayoutOpts, cm: &ColourMap) {
    let spec = &data.spec;
    let ink2 = rgb(b.ink.ink2);
    let multi = !data.facets.is_empty();
    let mut area = body;

    // the legend: two or more series, one row above the plots (a waterfall, histogram, box plot and heatmap have none)
    let legend_ok = !matches!(spec.kind, ChartKind::Waterfall | ChartKind::Histogram | ChartKind::BoxPlot) && data.series.len() >= 2;
    if legend_ok {
        let names: Vec<(String, Color32)> = data
            .series
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let c = cm.get(n).map(|c| c.hex(opts.dark)).map(rgb).unwrap_or_else(|| rgb(t::series_colour(i.min(t::SLOTS - 1), opts.dark).unwrap_or(0)));
                (n.clone(), match &spec.highlight {
                    Some(h) if !h.is_empty() && h != n => rgb(b.ink.deemph),
                    _ => c,
                })
            })
            .collect();
        let y = legend_row(b, &names, area);
        area = Rect::from_min_max(pos2(area.left(), y), area.max);
    }
    let _ = ink2;

    // shared value scale across panels of a small multiple (the default); each linked panel has its own
    let mut shared: Option<(f64, f64)> = None;
    if multi {
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for p in plans {
            let (l, h) = extent(p, spec.stack);
            lo = lo.min(l);
            hi = hi.max(h);
        }
        shared = Some((lo, hi));
    }

    // grid of panels
    let n = plans.len().max(1);
    let (cols, rows) = if multi {
        let cols = ((area.width() / SMALL_PANEL_W).floor() as usize).clamp(1, 4).min(n);
        (cols, n.div_ceil(cols))
    } else {
        (1, n)
    };
    let gap = if multi { SECTION_GAP * 2.0 } else { PANEL_GAP };
    let cell_h_avail = (area.height() - gap * (rows as f32 - 1.0)) / rows as f32;
    // small multiples and linked panels never get squeezed below a readable height: the chart grows instead
    let cell_h = if multi { cell_h_avail.max(SMALL_PANEL_H) } else if n > 1 { cell_h_avail.max(LINKED_PANEL_H) } else { cell_h_avail };
    if cell_h > cell_h_avail {
        b.out.needed_height = (area.top() - b.out.rect.top()) + cell_h * rows as f32 + gap * (rows as f32 - 1.0) + (b.out.rect.bottom() - body.bottom()) + 1.0;
        b.out.needed_height = b.out.needed_height.max(b.out.rect.height());
        if b.out.needed_height > b.out.rect.height() {
            // the panels run below the given rectangle: the chart's own bounds grow with them
            b.bounds = Rect::from_min_max(b.bounds.min, pos2(b.bounds.right(), b.bounds.top() + b.out.needed_height));
        }
    }
    let cell_w = (area.width() - gap * (cols as f32 - 1.0)) / cols as f32;

    // value axes first, so linked panels share one left margin
    let mut axes: Vec<Axis> = Vec::new();
    for p in plans {
        let (lo, hi) = shared.unwrap_or_else(|| panel_extent(p, data));
        let vals = [lo, hi];
        let mut extra = Vec::new();
        for r in spec.ref_lines.iter().filter(|r| r.panel == p.index.min(if multi { 0 } else { usize::MAX })) {
            extra.push(r.value);
        }
        for lt in spec.limit_ticks.iter().filter(|l| l.panel == p.index.min(if multi { 0 } else { usize::MAX })) {
            extra.push(lt.value);
        }
        let lo2 = vals.iter().chain(extra.iter()).cloned().fold(f64::INFINITY, f64::min);
        let hi2 = vals.iter().chain(extra.iter()).cloned().fold(f64::NEG_INFINITY, f64::max);
        let max_ticks = ((cell_h / 46.0).floor() as usize).clamp(3, 7);
        let (lo2, hi2) = (scale_v(lo2, p.vk, opts), scale_v(hi2, p.vk, opts));
        axes.push(make_axis(lo2, hi2, includes_zero(p.kind), p.vk, max_ticks));
    }
    let label_w = |ax: &Axis, b: &Builder| -> f32 { tick_labels(ax).iter().map(|s| b.size(s, t::TICK_PT, Face::Regular).x).fold(0.0, f32::max) };
    let horizontal = spec.horizontal && matches!(spec.kind, ChartKind::Bar);
    let left_margin = if horizontal {
        // horizontal bars: the category names stand on the left
        let w = plans.first().map(|p| p.categories.iter().map(|c| b.size(c, t::TICK_PT, Face::Regular).x).fold(0.0, f32::max)).unwrap_or(0.0);
        w.min(cell_w * MAX_CAT_LABEL_W_SHARE) + t::MIN_SPACE_PX
    } else {
        axes.iter().map(|a| label_w(a, b)).fold(0.0, f32::max) + t::MIN_SPACE_PX
    };

    // the room for line-end labels is the same in every panel, so linked panels share one x-axis
    let end_reserve = plans.iter().map(|p| line_end_reserve(b, p, spec, opts)).fold(0.0, f32::max);
    for (i, p) in plans.iter().enumerate() {
        let (r, c) = (i / cols, i % cols);
        let outer = Rect::from_min_size(pos2(area.left() + c as f32 * (cell_w + gap), area.top() + r as f32 * (cell_h + gap)), vec2(cell_w, cell_h));
        let last_row = if multi { r == rows - 1 } else { i == plans.len() - 1 };
        let show_cats = last_row || multi;
        let show_ticks = !multi || c == 0;
        panel(b, data, p, &axes[i], outer, left_margin, end_reserve, show_cats, show_ticks, opts, i, plans.len() > 1 || multi);
    }
}

fn scale_v(v: f64, vk: ValueKind, opts: &LayoutOpts) -> f64 {
    scaled(v, vk, opts)
}

/// The value range a panel needs: the steps' levels for a waterfall, the whiskers and outliers for a box plot.
fn panel_extent(p: &Plan, data: &ChartData) -> (f64, f64) {
    match p.kind {
        ChartKind::Waterfall => match waterfall_from_data(data) {
            Ok(steps) => (steps.iter().map(|s| s.base).fold(0.0, f64::min), steps.iter().map(|s| s.top).fold(0.0, f64::max)),
            Err(_) => (0.0, 1.0),
        },
        ChartKind::BoxPlot => {
            let lo = data.boxes.iter().flat_map(|b| std::iter::once(b.low_whisker).chain(b.outliers.iter().copied())).fold(f64::INFINITY, f64::min);
            let hi = data.boxes.iter().flat_map(|b| std::iter::once(b.high_whisker).chain(b.outliers.iter().copied())).fold(f64::NEG_INFINITY, f64::max);
            if lo.is_finite() && hi.is_finite() { (lo, hi) } else { (0.0, 1.0) }
        }
        _ => extent(p, data.spec.stack),
    }
}

/// One cartesian panel.
#[allow(clippy::too_many_arguments)]
fn panel(b: &mut Builder, data: &ChartData, p: &Plan, axis: &Axis, outer: Rect, left_margin: f32, end_reserve: f32, show_cats: bool, show_ticks: bool, opts: &LayoutOpts, panel_idx: usize, titled: bool) {
    let spec = &data.spec;
    let ink = b.ink;
    let (ink_c, ink2_c, muted_c, grid_c, base_c) = (rgb(ink.ink), rgb(ink.ink2), rgb(ink.muted), rgb(ink.grid), rgb(ink.baseline));
    let surface = rgb(ink.surface);
    let n_cat = p.categories.len();
    let horizontal = spec.horizontal && matches!(p.kind, ChartKind::Bar);

    // the waterfall has its own value range (the steps' levels)
    let wsteps: Option<Vec<WaterfallStep>> = if p.kind == ChartKind::Waterfall {
        match waterfall_from_data(data) {
            Ok(s) => Some(s),
            Err(e) => {
                let w = b.width_fn(t::SUBTITLE_PT, Face::Regular);
                let lines = wrap_text(&e.to_string(), outer.width().min(520.0), &w);
                let lh = b.size("Ag", t::SUBTITLE_PT, Face::Regular).y;
                let top = outer.center().y - lh * lines.len() as f32 / 2.0;
                for (i, l) in lines.iter().enumerate() {
                    b.put(Role::Message, l, Anchor::TopCentre, pos2(outer.center().x, top + lh * i as f32), t::SUBTITLE_PT, Face::Regular, ink2_c);
                }
                b.out.message = Some(e.to_string());
                return;
            }
        }
    } else {
        None
    };
    // header
    let mut top = outer.top();
    if titled && !p.title.is_empty() {
        let w = b.width_fn(t::LABEL_PT, Face::Semibold);
        let shown = fit_text(&p.title, outer.width(), &w);
        b.put(Role::Panel, &shown, Anchor::TopLeft, pos2(outer.left(), top), t::LABEL_PT, Face::Semibold, ink2_c);
        top += PANEL_HEADER_H;
    }

    // room for the value labels above the tallest mark
    let tick_h = b.size("0", t::TICK_PT, Face::Regular).y;
    let label_h = b.size("0", t::DATA_LABEL_PT, Face::Regular).y;
    let ref_room = if spec.ref_lines.is_empty() { 0.0 } else { label_h + t::MIN_SPACE_PX };
    let top_inset = (if horizontal { tick_h / 2.0 } else { (label_h + t::MIN_SPACE_PX).max(tick_h / 2.0) }) + ref_room;
    let cat_h = if show_cats && !horizontal { tick_h + t::MIN_SPACE_PX } else { 0.0 };
    let bottom_axis_h = if horizontal { tick_h + t::MIN_SPACE_PX } else { cat_h };
    let left = outer.left() + if show_ticks || horizontal { left_margin } else { t::MIN_SPACE_PX };
    // a horizontal bar chart's last tick label is centred on the plot's right edge: leave room for half of it
    let right_pad = if horizontal { tick_labels(axis).iter().map(|s| b.size(s, t::TICK_PT, Face::Regular).x).fold(0.0, f32::max) / 2.0 + 1.0 } else { end_reserve };
    let plot = Rect::from_min_max(pos2(left, top + top_inset), pos2(outer.right() - right_pad, outer.bottom() - bottom_axis_h));
    if plot.width() < 20.0 || plot.height() < 20.0 {
        return;
    }
    b.out.panels.push(PanelLayout { kind: p.kind, title: p.title.clone(), plot, lo: axis.lo, hi: axis.hi, horizontal, ticks: Vec::new() });
    let pi = b.out.panels.len() - 1;
    let geom = b.out.panels[pi].clone();
    let v_of = |v: f64| geom.coord(v);
    let band = if horizontal { plot.height() / n_cat.max(1) as f32 } else { plot.width() / n_cat.max(1) as f32 };
    let band_centre = |i: usize| -> f32 {
        if horizontal {
            plot.top() + band * (i as f32 + 0.5)
        } else {
            plot.left() + band * (i as f32 + 0.5)
        }
    };

    // gridlines (horizontal for columns, vertical for horizontal bars), tick labels, the baseline
    let ticks_text = tick_labels(axis);
    let mut tick_pos = Vec::new();
    for (v, s) in axis.ticks.iter().zip(ticks_text.iter()) {
        let c = v_of(*v);
        tick_pos.push((*v, c));
        let is_zero = *v == 0.0;
        if horizontal {
            b.line(pos2(c, plot.top()), pos2(c, plot.bottom()), t::GRID_PX, grid_c);
            b.put(Role::Tick, s, Anchor::TopCentre, pos2(c, plot.bottom() + t::MIN_SPACE_PX / 2.0), t::TICK_PT, Face::Regular, muted_c);
        } else {
            b.line(pos2(plot.left(), c), pos2(plot.right(), c), t::GRID_PX, if is_zero && includes_zero(p.kind) { base_c } else { grid_c });
            if show_ticks {
                b.put(Role::Tick, s, Anchor::MidRight, pos2(plot.left() - t::MIN_SPACE_PX, c), t::TICK_PT, Face::Regular, muted_c);
            }
        }
    }
    b.out.panels[pi].ticks = tick_pos;
    // the zero line of a horizontal bar chart / the baseline of a column chart is drawn on top of the grid
    if includes_zero(p.kind) && axis.lo < 0.0 && axis.hi > 0.0 {
        let z = v_of(0.0);
        if horizontal {
            b.line(pos2(z, plot.top()), pos2(z, plot.bottom()), t::GRID_PX, base_c);
        } else {
            b.line(pos2(plot.left(), z), pos2(plot.right(), z), t::GRID_PX, base_c);
        }
    } else if !horizontal {
        b.line(pos2(plot.left(), plot.bottom()), pos2(plot.right(), plot.bottom()), t::GRID_PX, base_c);
    }

    // category names
    category_labels(b, p, plot, band, horizontal, show_cats);

    // marks
    let vk = p.vk;
    let fmt = |v: f64, signed: bool| value_text(scaled(v, vk, opts), vk, opts.decimals, signed);
    let tipv = |s: usize, c: usize, v: Option<f64>| -> String { tip_text(data, p, s, c, v, vk, opts) };
    let _ = &tipv;
    match p.kind {
        ChartKind::Bar | ChartKind::Histogram => bars(b, data, p, &geom, band, &band_centre, opts, &fmt, p.kind == ChartKind::Histogram, panel_idx),
        ChartKind::Waterfall => waterfall(b, data, p, wsteps.as_deref().unwrap_or(&[]), &geom, band, &band_centre, opts, &fmt, panel_idx),
        ChartKind::Line | ChartKind::Area => lines(b, data, p, &geom, band, &band_centre, opts, &fmt, panel_idx),
        ChartKind::BoxPlot => boxes(b, data, &geom, band, &band_centre, opts, panel_idx),
        _ => {}
    }

    // reference lines and bands (value axis), after the marks so they sit on top
    if matches!(p.kind, ChartKind::Bar | ChartKind::Line | ChartKind::Area | ChartKind::Histogram) {
        for band_spec in spec.ref_bands.iter().filter(|r| r.panel == if multi_index(data) { 0 } else { panel_idx }) {
            let (a, c) = (v_of(scaled(band_spec.from, vk, opts)), v_of(scaled(band_spec.to, vk, opts)));
            let r = if horizontal { Rect::from_x_y_ranges(a.min(c)..=a.max(c), plot.y_range()) } else { Rect::from_x_y_ranges(plot.x_range(), a.min(c)..=a.max(c)) };
            b.rect(r.intersect(plot), alpha(rgb(ink.deemph), 0.45), [0.0; 4]);
            if !band_spec.label.is_empty() {
                b.try_put(Role::Reference, &band_spec.label, Anchor::TopLeft, pos2(plot.left() + 4.0, r.intersect(plot).top() + 2.0), t::LABEL_PT, Face::Regular, ink2_c);
            }
        }
        for rl in spec.ref_lines.iter().filter(|r| r.panel == if multi_index(data) { 0 } else { panel_idx }) {
            let c = v_of(scaled(rl.value, vk, opts));
            let col = rgb(rl.status.hex());
            if horizontal {
                b.line(pos2(c, plot.top()), pos2(c, plot.bottom()), t::REFERENCE_LINE_PX, col);
            } else {
                b.line(pos2(plot.left(), c), pos2(plot.right(), c), t::REFERENCE_LINE_PX, col);
            }
            let text = if rl.label.is_empty() { format!("{}: {}", rl.status.label(), fmt(rl.value, false)) } else { format!("{} ({})", rl.label, rl.status.label()) };
            // the label goes beside the line where there is room: along the line, above or below it
            let size = b.size(&text, t::LABEL_PT, Face::Regular);
            let mut placed = false;
            'scan: for side in if horizontal { [1.0f32, -1.0] } else { [-1.0f32, 1.0] } {
                for k in 0..=8usize {
                    let f = if k % 2 == 0 { 1.0 - k as f32 / 16.0 } else { k as f32 / 16.0 };
                    let (anchor, at) = if horizontal {
                        // a vertical line: the label to its right (side 1) or left (side -1), near the top
                        let y = plot.top() + 2.0 + size.y * (k as f32 / 2.0);
                        if side > 0.0 { (Anchor::TopLeft, pos2(c + 4.0, y)) } else { (Anchor::TopRight, pos2(c - 4.0, y)) }
                    } else {
                        let x = plot.left() + 4.0 + (plot.width() - size.x - 8.0).max(0.0) * f;
                        if side < 0.0 { (Anchor::BottomLeft, pos2(x, c - 3.0)) } else { (Anchor::TopLeft, pos2(x, c + 3.0)) }
                    };
                    if b.try_put(Role::Reference, &text, anchor, at, t::LABEL_PT, Face::Regular, ink_c).is_some() {
                        placed = true;
                        break 'scan;
                    }
                }
            }
            if !placed {
                // no room beside the line (it crosses the bars): the label stands above the plot, after a short
                // piece of the line in its colour, so it is still tied to the line
                let sw = 14.0;
                let y = plot.top() - size.y / 2.0 - 2.0;
                let x = if horizontal { c } else { plot.right() - size.x - sw - 6.0 };
                let x = x.clamp(plot.left(), (plot.right() - size.x - sw - 6.0).max(plot.left()));
                b.line(pos2(x, y), pos2(x + sw, y), t::REFERENCE_LINE_PX, col);
                b.put(Role::Reference, &text, Anchor::MidLeft, pos2(x + sw + 4.0, y), t::LABEL_PT, Face::Regular, ink_c);
            }
        }
    }
    // per-category limit ticks, and the ring and text of a breach (drawn last: on top of the marks)
    if matches!(p.kind, ChartKind::Bar | ChartKind::Line | ChartKind::Area) {
        limit_ticks(b, data, p, &geom, band, &band_centre, opts, if multi_index(data) { 0 } else { panel_idx }, plot);
    }
    let _ = (surface, label_h);
}

/// Ticks across the category bands at their limits; a breach is ringed on the mark and written beside it.
#[allow(clippy::too_many_arguments)]
fn limit_ticks(b: &mut Builder, data: &ChartData, p: &Plan, g: &PanelLayout, band: f32, centre: &dyn Fn(usize) -> f32, opts: &LayoutOpts, panel_idx: usize, plot: Rect) {
    let spec = &data.spec;
    let ink_c = rgb(b.ink.ink);
    let muted_c = rgb(b.ink.muted);
    let ticks: Vec<_> = spec.limit_ticks.iter().filter(|l| l.panel == panel_idx).collect();
    if ticks.is_empty() {
        return;
    }
    let half = (band * 0.35).clamp(8.0, 30.0);
    let mut breaches: Vec<(Pos2, String, Color32, f32)> = Vec::new();
    for lt in &ticks {
        let Some(ci) = p.categories.iter().position(|c| *c == lt.category) else { continue };
        let col = rgb(lt.status.hex());
        let (c, v) = (centre(ci), g.coord(scaled(lt.value, p.vk, opts)));
        if g.horizontal {
            b.line(pos2(v, c - half), pos2(v, c + half), t::REFERENCE_LINE_PX, col);
        } else {
            b.line(pos2(c - half, v), pos2(c + half, v), t::REFERENCE_LINE_PX, col);
        }
        if !lt.breach.is_empty() {
            // the first series' mark of that category
            let val = p.series.first().and_then(|s| s.vals.get(ci)).copied().flatten();
            if let Some(val) = val {
                let m = g.coord(scaled(val, p.vk, opts));
                let at = if g.horizontal { pos2(m, c) } else { pos2(c, m) };
                b.out.prims.push(Prim::Outline { c: at, r: t::MARKER_PX / 2.0 + 5.0, w: t::REFERENCE_LINE_PX, col });
                breaches.push((at, lt.breach.clone(), col, v));
            }
        }
    }
    for (at, text, _col, tick_v) in breaches {
        let r = t::MARKER_PX / 2.0 + 5.0 + 3.0;
        let size = b.size(&text, t::LABEL_PT, Face::Regular);
        // above the ring and its limit tick, then right of it, then below it: where the text touches nothing
        let above = if g.horizontal { at.y - r } else { (at.y - r).min(tick_v - 4.0) };
        let tries = [
            (Anchor::BottomLeft, pos2(at.x - r, above)),
            (Anchor::BottomLeft, pos2(at.x - size.x / 2.0, above)),
            (Anchor::MidLeft, pos2(at.x + r, at.y)),
            (Anchor::TopLeft, pos2(at.x - r, at.y + r)),
            (Anchor::BottomRight, pos2(at.x + r, at.y - r)),
        ];
        let mut done = false;
        for (anchor, pos) in tries {
            if b.try_put(Role::Reference, &text, anchor, pos, t::LABEL_PT, Face::Regular, ink_c).is_some() {
                done = true;
                break;
            }
        }
        if !done {
            // no free place: the text goes above the plot, never clipped
            let y = plot.top() - size.y / 2.0 - 2.0;
            b.put(Role::Reference, &text, Anchor::MidLeft, pos2(plot.left() + 4.0, y), t::LABEL_PT, Face::Regular, ink_c);
        }
    }
    if !spec.limit_note.trim().is_empty() {
        let note = fit_text(&format!("\u{2014} {}", spec.limit_note.trim()), (plot.width() - 6.0).max(40.0), &b.width_fn(t::LABEL_PT, Face::Regular));
        let size = b.size(&note, t::LABEL_PT, Face::Regular);
        let cands = [pos2(plot.right() - 2.0, plot.bottom() - 3.0), pos2(plot.right() - 2.0, plot.top() + size.y + 3.0), pos2(plot.left() + size.x + 2.0, plot.top() + size.y + 3.0)];
        for (k, at) in cands.iter().enumerate() {
            let anchor = if k < 2 { Anchor::BottomRight } else { Anchor::BottomRight };
            if b.try_put(Role::Reference, &note, anchor, *at, t::LABEL_PT, Face::Regular, muted_c).is_some() {
                break;
            }
        }
    }
}

fn multi_index(data: &ChartData) -> bool {
    !data.facets.is_empty()
}

/// The room at the right of a line chart for its end labels.
fn line_end_reserve(b: &Builder, p: &Plan, spec: &ChartSpec, opts: &LayoutOpts) -> f32 {
    if !matches!(p.kind, ChartKind::Line) {
        return 0.0;
    }
    let mut w: f32 = 0.0;
    for s in &p.series {
        if let Some(Some(v)) = s.vals.last() {
            w = w.max(b.size(&value_text(scaled(*v, p.vk, opts), p.vk, opts.decimals, false), t::DATA_LABEL_PT, Face::Regular).x);
        }
        if p.series.len() <= 4 {
            w = w.max(b.size(&s.name, t::LABEL_PT, Face::Regular).x);
        }
    }
    let _ = spec;
    if w > 0.0 {
        w + t::MIN_SPACE_PX * 2.0
    } else {
        0.0
    }
}

/// Category names under the columns (or left of the bars): every name when they fit, a cut name when
/// that fits, else every k-th name; the whole name is always in the tooltip.
fn category_labels(b: &mut Builder, p: &Plan, plot: Rect, band: f32, horizontal: bool, show: bool) {
    if !show || p.categories.is_empty() {
        return;
    }
    let ink_c = rgb(b.ink.ink);
    let tick_h = b.size("0", t::TICK_PT, Face::Regular).y;
    if horizontal {
        let max_w = (plot.left() - b.bounds.left()).max(20.0) - t::MIN_SPACE_PX;
        let every = ((tick_h + 4.0) / band).ceil().max(1.0) as usize;
        for (i, c) in p.categories.iter().enumerate().step_by(every) {
            let shown = fit_text(c, max_w, &b.width_fn(t::TICK_PT, Face::Regular));
            let y = plot.top() + band * (i as f32 + 0.5);
            b.put(Role::Category, &shown, Anchor::MidRight, pos2(plot.left() - t::MIN_SPACE_PX, y), t::TICK_PT, Face::Regular, ink_c);
        }
        return;
    }
    let widths: Vec<f32> = p.categories.iter().map(|c| b.size(c, t::TICK_PT, Face::Regular).x).collect();
    let max_w = widths.iter().cloned().fold(0.0, f32::max);
    let slot = band - 4.0;
    let (every, cut) = if max_w <= slot {
        (1usize, None)
    } else {
        // cut every name to the slot, if that leaves at least a few characters; else show every k-th whole
        let fits = (slot / (t::TICK_PT * 0.5)).floor() as usize;
        if fits >= 5 {
            (1, Some(slot))
        } else {
            let k = ((max_w + t::MIN_SPACE_PX) / band).ceil().max(1.0) as usize;
            (k, None)
        }
    };
    let y = plot.bottom() + t::MIN_SPACE_PX / 2.0 + 1.0;
    let mut last_right = f32::NEG_INFINITY;
    for (i, c) in p.categories.iter().enumerate().step_by(every) {
        let shown = match cut {
            Some(w) => fit_text(c, w, &b.width_fn(t::TICK_PT, Face::Regular)),
            None => c.clone(),
        };
        let x = plot.left() + band * (i as f32 + 0.5);
        let r = b.rect_for(&shown, t::TICK_PT, Face::Regular, Anchor::TopCentre, pos2(x, y));
        if r.left() < last_right + 4.0 || r.left() < b.bounds.left() || r.right() > b.bounds.right() || b.out.texts.iter().any(|x| x.rect.intersects(inflate(r, 1.0))) {
            continue;
        }
        last_right = r.right();
        b.put(Role::Category, &shown, Anchor::TopCentre, pos2(x, y), t::TICK_PT, Face::Regular, ink_c);
    }
}

/// The tooltip of one mark: the category, the series, the value, the rows behind it, and the base of a percentage.
fn tip_text(data: &ChartData, p: &Plan, s: usize, c: usize, v: Option<f64>, vk: ValueKind, opts: &LayoutOpts) -> String {
    let spec = &data.spec;
    let cat = p.categories.get(c).cloned().unwrap_or_default();
    let mut out = format!("{}: {}", if spec.category.is_empty() { "Category" } else { &spec.category }, cat);
    let ser = p.series.get(s);
    if let Some(sr) = ser {
        if !sr.name.is_empty() {
            out.push_str(&format!("\n{}: {}", spec.series.clone().unwrap_or_else(|| "Series".to_string()), sr.name));
        }
    }
    let name = data.measure_names.get(p.measure).cloned().unwrap_or_default();
    let vt = match v {
        Some(x) => value_text(scaled(x, vk, opts), vk, opts.decimals + if vk == ValueKind::Percent { 1 } else { 0 }, false),
        None => "no value".to_string(),
    };
    out.push_str(&format!("\n{}: {}", if name.is_empty() { "Value" } else { &name }, vt));
    if let Some(sr) = ser {
        let rows = sr.rows.get(c).copied().unwrap_or(0);
        out.push_str(&format!("\n{} rows", crate::models::format_with_commas(rows as f64, 0)));
    }
    if vk == ValueKind::Percent {
        let ml = spec.measure_list();
        let tr = ml.get(p.measure).map(|m| m.transform).unwrap_or(Transform::None);
        if !base_word(tr).is_empty() {
            out.push_str(&format!("\nPercent {}", base_word(tr)));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------- bar forms

fn rounded(positive: bool, horizontal: bool, r: f32, h: f32) -> [f32; 4] {
    let r = r.min(h / 2.0).max(0.0);
    // [nw, ne, sw, se]; the data-end is rounded, the baseline end square
    match (horizontal, positive) {
        (false, true) => [r, r, 0.0, 0.0],
        (false, false) => [0.0, 0.0, r, r],
        (true, true) => [0.0, r, 0.0, r],
        (true, false) => [r, 0.0, r, 0.0],
    }
}

#[allow(clippy::too_many_arguments)]
fn bars(b: &mut Builder, data: &ChartData, p: &Plan, g: &PanelLayout, band: f32, centre: &dyn Fn(usize) -> f32, opts: &LayoutOpts, fmt: &dyn Fn(f64, bool) -> String, touching: bool, pidx: usize) {
    let spec = &data.spec;
    let horizontal = g.horizontal;
    let ink = b.ink;
    let surface = rgb(ink.surface);
    let n_cat = p.categories.len();
    let n_ser = p.series.len().max(1);
    let stacked = matches!(spec.stack, Stack::Stacked | Stack::Percent) && n_ser > 1 && !touching;
    let one = n_ser == 1;
    let has_negative = p.series.iter().any(|s| s.vals.iter().flatten().any(|v| *v < 0.0));
    let diverging = one && has_negative && !touching;
    let highlight = spec.highlight.clone().filter(|h| !h.is_empty());
    let gap = t::SEGMENT_GAP_PX;
    // the thickness of one bar (a stack is one bar)
    let thick = if touching {
        (band - gap).max(1.0)
    } else if stacked || one {
        (band * BAND_FILL).min(t::BAR_MAX_PX).max(1.0)
    } else {
        ((band * BAND_FILL - gap * (n_ser as f32 - 1.0)) / n_ser as f32).min(t::BAR_MAX_PX).max(1.0)
    };
    let labels_ok = n_cat <= LABEL_MAX_BARS && !touching || (touching && n_cat <= LABEL_MAX_BARS);
    let mut totals: Vec<Option<(f64, f64)>> = vec![None; n_cat]; // (positive top, negative bottom) of each category

    for c in 0..n_cat {
        let mid = centre(c);
        let (mut pos_acc, mut neg_acc) = (0.0f64, 0.0f64);
        for (si, s) in p.series.iter().enumerate() {
            let Some(Some(v0)) = s.vals.get(c) else { continue };
            let v = if spec.stack == Stack::Percent && stacked { *v0 } else { *v0 };
            let v = if spec.stack == Stack::Percent && stacked {
                // 100% stacked: this category's share of its own total
                let total: f64 = p.series.iter().filter_map(|x| x.vals.get(c).copied().flatten()).map(f64::abs).sum();
                if total == 0.0 { 0.0 } else { v / total * 100.0 }
            } else {
                v
            };
            let sv = scaled(v, p.vk, opts);
            let _ = sv;
            let positive = v >= 0.0;
            let (from, to) = if stacked {
                let a = if positive { pos_acc } else { neg_acc };
                let z = a + v;
                if positive {
                    pos_acc = z;
                } else {
                    neg_acc = z;
                }
                (a, z)
            } else {
                (0.0, v)
            };
            let (sf, st) = (scaled(from, p.vk, opts), scaled(to, p.vk, opts));
            let (cf, ct) = (g.coord(sf), g.coord(st));
            // offset of this bar across the band
            let off = if stacked || one || touching { 0.0 } else { (si as f32 - (n_ser as f32 - 1.0) / 2.0) * (thick + gap) };
            let colour = {
                let base = if diverging {
                    if v < 0.0 { rgb(t::DIVERGING_NEGATIVE) } else { rgb(t::DIVERGING_POSITIVE) }
                } else {
                    s.colour
                };
                match &highlight {
                    Some(h) if one && p.categories.get(c).map(|x| x != h).unwrap_or(false) => rgb(ink.deemph),
                    _ => base,
                }
            };
            let mut r = if horizontal {
                Rect::from_min_max(pos2(cf.min(ct), mid + off - thick / 2.0), pos2(cf.max(ct), mid + off + thick / 2.0))
            } else {
                Rect::from_min_max(pos2(mid + off - thick / 2.0, ct.min(cf)), pos2(mid + off + thick / 2.0, ct.max(cf)))
            };
            // at least one pixel, so a small value is still seen
            if horizontal && r.width() < 1.0 {
                r = Rect::from_min_max(r.min, pos2(r.min.x + 1.0, r.max.y));
            } else if !horizontal && r.height() < 1.0 {
                r = Rect::from_min_max(pos2(r.min.x, r.max.y - 1.0), r.max);
            }
            // the 2 px gap between stacked segments, in the surface colour (taken from the segment itself)
            let is_outer;
            if stacked {
                let next_has = p.series.iter().skip(si + 1).any(|x| x.vals.get(c).copied().flatten().map(|y| (y >= 0.0) == positive).unwrap_or(false));
                let first = if positive { from == 0.0 } else { from == 0.0 };
                is_outer = !next_has;
                let half = gap / 2.0;
                if horizontal {
                    if !first {
                        r = if positive { Rect::from_min_max(pos2(r.left() + half, r.top()), r.max) } else { Rect::from_min_max(r.min, pos2(r.right() - half, r.bottom())) };
                    }
                    if next_has {
                        r = if positive { Rect::from_min_max(r.min, pos2(r.right() - half, r.bottom())) } else { Rect::from_min_max(pos2(r.left() + half, r.top()), r.max) };
                    }
                } else {
                    if !first {
                        r = if positive { Rect::from_min_max(r.min, pos2(r.right(), r.bottom() - half)) } else { Rect::from_min_max(pos2(r.left(), r.top() + half), r.max) };
                    }
                    if next_has {
                        r = if positive { Rect::from_min_max(pos2(r.left(), r.top() + half), r.max) } else { Rect::from_min_max(r.min, pos2(r.right(), r.bottom() - half)) };
                    }
                }
            } else {
                is_outer = true;
            }
            let round = if is_outer && !touching { rounded(positive, horizontal, t::BAR_RADIUS_PX, if horizontal { r.width() } else { r.height() }) } else if touching { rounded(true, false, t::BAR_RADIUS_PX, r.height()) } else { [0.0; 4] };
            b.rect(r, colour, round);
            let _ = surface;
            let tip = tip_text(data, p, si, c, Some(*v0), p.vk, opts);
            b.out.marks.push(Mark { rect: r, kind: if stacked { MarkKind::Segment } else { MarkKind::Bar }, panel: pidx, series: si, category: c, value: Some(*v0), tip, slice: None });
        }
        if stacked {
            totals[c] = Some((pos_acc, neg_acc));
        }
    }

    // value labels: single series (12 bars or fewer): at the tip; stacked columns: the total on top only
    if !labels_ok {
        return;
    }
    let ink_c = rgb(ink.ink);
    let label_pt = t::DATA_LABEL_PT;
    for c in 0..n_cat {
        let mid = centre(c);
        let (value, tip_v, positive) = if stacked && spec.stack == Stack::Stacked {
            let Some((pa, na)) = totals[c] else { continue };
            let total = pa + na;
            (total, if total >= 0.0 { pa } else { na }, total >= 0.0)
        } else if one {
            let Some(Some(v)) = p.series[0].vals.get(c) else { continue };
            (*v, *v, *v >= 0.0)
        } else {
            continue;
        };
        let text = fmt(value, false);
        let at = g.coord(scaled(tip_v, p.vk, opts));
        let gapl = 4.0;
        let (anchor, pos) = if horizontal {
            if positive { (Anchor::MidLeft, pos2(at + gapl, mid)) } else { (Anchor::MidRight, pos2(at - gapl, mid)) }
        } else if positive {
            (Anchor::BottomCentre, pos2(mid, at - gapl))
        } else {
            (Anchor::TopCentre, pos2(mid, at + gapl))
        };
        b.try_put(Role::Label, &text, anchor, pos, label_pt, Face::Regular, ink_c);
    }
}

#[allow(clippy::too_many_arguments)]
fn waterfall(b: &mut Builder, data: &ChartData, p: &Plan, steps: &[WaterfallStep], g: &PanelLayout, band: f32, centre: &dyn Fn(usize) -> f32, opts: &LayoutOpts, fmt: &dyn Fn(f64, bool) -> String, pidx: usize) {
    let ink = b.ink;
    let thick = (band * BAND_FILL).min(t::BAR_MAX_PX).max(1.0);
    let ink_c = rgb(ink.ink);
    let grid_c = rgb(ink.baseline);
    let mut prev_right: Option<(f32, f32)> = None; // right edge and the level's coordinate of the last bar
    for (i, s) in steps.iter().enumerate() {
        let mid = centre(i);
        let (cb, ct) = (g.coord(scaled(s.base, p.vk, opts)), g.coord(scaled(s.top, p.vk, opts)));
        let mut r = Rect::from_min_max(pos2(mid - thick / 2.0, ct.min(cb)), pos2(mid + thick / 2.0, ct.max(cb)));
        if r.height() < 1.0 {
            r = Rect::from_min_max(pos2(r.left(), r.bottom() - 1.0), r.max);
        }
        let (colour, positive) = match s.kind {
            StepKind::Start => (rgb(t::WATERFALL_START), s.value >= 0.0),
            StepKind::End => (rgb(t::WATERFALL_END), s.value >= 0.0),
            StepKind::Delta => (if s.value >= 0.0 { rgb(t::DIVERGING_POSITIVE) } else { rgb(t::DIVERGING_NEGATIVE) }, s.value >= 0.0),
        };
        // a connector from the previous bar's level to this bar's start
        if let Some((pr, y)) = prev_right {
            b.line(pos2(pr, y), pos2(r.left(), y), t::GRID_PX, grid_c);
        }
        b.rect(r, colour, rounded(positive, false, t::BAR_RADIUS_PX, r.height()));
        let level_y = g.coord(scaled(s.level, p.vk, opts));
        prev_right = Some((r.right(), level_y));
        let signed = s.kind == StepKind::Delta;
        let tip = format!("{}\n{}: {}", s.label, if s.kind == StepKind::Delta { "Change" } else { "Total" }, fmt(s.value, signed));
        b.out.marks.push(Mark { rect: r, kind: MarkKind::Bar, panel: pidx, series: 0, category: i, value: Some(s.value), tip, slice: None });
    }
    // every step is labelled, above its bar
    let _ = data;
    for (i, s) in steps.iter().enumerate() {
        let mid = centre(i);
        let top = g.coord(scaled(s.top, p.vk, opts));
        let text = fmt(s.value, s.kind == StepKind::Delta);
        b.try_put(Role::Label, &text, Anchor::BottomCentre, pos2(mid, top - 4.0), t::DATA_LABEL_PT, Face::Regular, ink_c);
    }
}

// ---------------------------------------------------------------------------------- line and area

#[allow(clippy::too_many_arguments)]
fn lines(b: &mut Builder, data: &ChartData, p: &Plan, g: &PanelLayout, band: f32, centre: &dyn Fn(usize) -> f32, opts: &LayoutOpts, fmt: &dyn Fn(f64, bool) -> String, pidx: usize) {
    let spec = &data.spec;
    let ink = b.ink;
    let surface = rgb(ink.surface);
    let area = p.kind == ChartKind::Area;
    let stacked_area = area && matches!(spec.stack, Stack::Stacked | Stack::Percent) && p.series.len() > 1;
    let n_cat = p.categories.len();
    let n_ser = p.series.len();
    let highlight = spec.highlight.clone().filter(|h| !h.is_empty());
    let ink_c = rgb(ink.ink);
    let ink2_c = rgb(ink.ink2);
    let baseline_y = g.coord(g.lo.max(0.0).min(g.hi));
    let markers = n_cat <= MARKERS_MAX_POINTS;

    // cumulative tops for stacked areas
    let mut cum: Vec<Vec<f64>> = vec![vec![0.0; n_cat]; n_ser + 1];
    if stacked_area {
        for c in 0..n_cat {
            let total: f64 = p.series.iter().filter_map(|s| s.vals.get(c).copied().flatten()).sum();
            for si in 0..n_ser {
                let v = p.series[si].vals.get(c).copied().flatten().unwrap_or(0.0);
                let v = if spec.stack == Stack::Percent { if total == 0.0 { 0.0 } else { v / total * 100.0 } } else { v };
                cum[si + 1][c] = cum[si][c] + v;
            }
        }
    }
    let point = |si: usize, c: usize| -> Option<Pos2> {
        let v = p.series.get(si)?.vals.get(c).copied().flatten()?;
        let val = if stacked_area { cum[si + 1][c] } else { v };
        Some(pos2(centre(c), g.coord(scaled(val, p.vk, opts))))
    };
    let _ = band;

    // draw the later series first so the first is on top
    for si in (0..n_ser).rev() {
        let s = &p.series[si];
        let emphasised = highlight.as_ref().map(|h| *h == s.name).unwrap_or(true);
        let colour = s.colour;
        let pts: Vec<Option<Pos2>> = (0..n_cat).map(|c| point(si, c)).collect();
        // area fill under the line (a band between cumulative tops when stacked)
        if area {
            let fill = if stacked_area { alpha(colour, t::STACKED_AREA_OPACITY) } else { alpha(colour, t::AREA_OPACITY) };
            let mut tris: Vec<[Pos2; 3]> = Vec::new();
            for c in 0..n_cat.saturating_sub(1) {
                if let (Some(a), Some(d)) = (pts[c], pts[c + 1]) {
                    let (ba, bd) = if stacked_area {
                        (pos2(a.x, g.coord(scaled(cum[si][c], p.vk, opts))), pos2(d.x, g.coord(scaled(cum[si][c + 1], p.vk, opts))))
                    } else {
                        (pos2(a.x, baseline_y), pos2(d.x, baseline_y))
                    };
                    tris.push([a, d, bd]);
                    tris.push([a, bd, ba]);
                }
            }
            b.out.prims.push(Prim::Tris { tris, fill });
        }
        // the line
        let w = t::LINE_PX;
        let path = |b: &mut Builder, a: Pos2, d: Pos2| {
            if spec.stepped {
                let mid = pos2(d.x, a.y);
                b.line(a, mid, w, colour);
                b.line(mid, d, w, colour);
            } else {
                b.line(a, d, w, colour);
            }
        };
        for c in 0..n_cat.saturating_sub(1) {
            if let (Some(a), Some(d)) = (pts[c], pts[c + 1]) {
                path(b, a, d);
            }
        }
        // round joins
        for q in pts.iter().flatten() {
            b.out.prims.push(Prim::Circle { c: *q, r: w / 2.0, fill: colour, ring: None });
        }
        if markers && emphasised {
            for (c, q) in pts.iter().enumerate() {
                if let Some(q) = q {
                    b.out.prims.push(Prim::Circle { c: *q, r: t::MARKER_PX / 2.0, fill: colour, ring: Some((t::MARKER_RING_PX, surface)) });
                    let r = Rect::from_center_size(*q, vec2(t::MARKER_PX, t::MARKER_PX));
                    let v = p.series[si].vals.get(c).copied().flatten();
                    let tip = tip_text(data, p, si, c, v, p.vk, opts);
                    b.out.marks.push(Mark { rect: r, kind: MarkKind::Point, panel: pidx, series: si, category: c, value: v, tip, slice: None });
                }
            }
        }
    }
    // the crosshair of each category: all series' values
    let plot = g.plot;
    for c in 0..n_cat {
        let mut lines_txt = vec![format!("{}: {}", if spec.category.is_empty() { "Category" } else { &spec.category }, p.categories[c])];
        for s in p.series.iter() {
            let v = s.vals.get(c).copied().flatten();
            let label = if s.name.is_empty() { data.measure_names.get(p.measure).cloned().unwrap_or_else(|| "Value".to_string()) } else { s.name.clone() };
            lines_txt.push(format!("{}: {}", label, v.map(|v| fmt(v, false)).unwrap_or_else(|| "no value".to_string())));
        }
        let rows: u64 = p.series.iter().map(|s| s.rows.get(c).copied().unwrap_or(0)).sum();
        lines_txt.push(format!("{} rows", crate::models::format_with_commas(rows as f64, 0)));
        b.out.crosshairs.push(Crosshair { x: centre(c), y0: plot.top(), y1: plot.bottom(), panel: pidx, category: c, tip: lines_txt.join("\n") });
    }

    // labels at the line ends: the value for one series; the name for 2 to 4 (a collision drops it to the legend)
    if p.kind == ChartKind::Line && n_cat > 0 {
        for si in 0..n_ser {
            let s = &p.series[si];
            let last = (0..n_cat).rev().find(|c| pts_at(p, si, *c).is_some());
            let first = (0..n_cat).find(|c| pts_at(p, si, *c).is_some());
            let (Some(lc), Some(fc)) = (last, first) else { continue };
            let (Some(lp), Some(fp)) = (point(si, lc), point(si, fc)) else { continue };
            let emphasised = highlight.as_ref().map(|h| *h == s.name).unwrap_or(true);
            if n_ser == 1 {
                if let Some(v) = s.vals[lc] {
                    b.try_put(Role::Label, &fmt(v, false), Anchor::MidLeft, pos2(lp.x + t::MARKER_PX, lp.y), t::DATA_LABEL_PT, Face::Regular, ink_c);
                }
            } else if n_ser <= 4 {
                if emphasised {
                    b.try_put(Role::Direct, &s.name, Anchor::MidLeft, pos2(lp.x + t::MARKER_PX, lp.y), t::LABEL_PT, Face::Regular, ink_c);
                } else {
                    // a de-emphasised curve is named at its start
                    b.try_put(Role::Direct, &s.name, Anchor::BottomLeft, pos2(fp.x + 2.0, fp.y - 6.0), t::LABEL_PT, Face::Regular, ink2_c);
                }
            }
        }
    }
}

fn pts_at(p: &Plan, si: usize, c: usize) -> Option<f64> {
    p.series.get(si)?.vals.get(c).copied().flatten()
}

// ----------------------------------------------------------------------------------------- box plot

fn boxes(b: &mut Builder, data: &ChartData, g: &PanelLayout, band: f32, centre: &dyn Fn(usize) -> f32, opts: &LayoutOpts, pidx: usize) {
    let ink = b.ink;
    let surface = rgb(ink.surface);
    let ink2 = rgb(ink.ink2);
    let colour = rgb(t::series_colour(0, opts.dark).unwrap_or(0));
    let w = (band * BOX_FILL).min(t::BAR_MAX_PX).max(2.0);
    for (i, bx) in data.boxes.iter().enumerate() {
        let x = centre(i);
        let y = |v: f64| g.coord(v);
        let (q1, q3, med, lw, hw) = (y(bx.q1), y(bx.q3), y(bx.median), y(bx.low_whisker), y(bx.high_whisker));
        b.line(pos2(x, hw), pos2(x, q3), t::REFERENCE_LINE_PX, ink2);
        b.line(pos2(x, q1), pos2(x, lw), t::REFERENCE_LINE_PX, ink2);
        b.line(pos2(x - w / 4.0, hw), pos2(x + w / 4.0, hw), t::REFERENCE_LINE_PX, ink2);
        b.line(pos2(x - w / 4.0, lw), pos2(x + w / 4.0, lw), t::REFERENCE_LINE_PX, ink2);
        let r = Rect::from_min_max(pos2(x - w / 2.0, q3.min(q1)), pos2(x + w / 2.0, q3.max(q1)));
        let r = if r.height() < 1.0 { Rect::from_min_max(r.min, pos2(r.max.x, r.min.y + 1.0)) } else { r };
        b.rect(r, colour, [t::BAR_RADIUS_PX.min(r.height() / 2.0); 4]);
        b.line(pos2(x - w / 2.0, med), pos2(x + w / 2.0, med), t::LINE_PX, surface);
        for o in &bx.outliers {
            let c = pos2(x, y(*o));
            b.out.prims.push(Prim::Circle { c, r: t::MARKER_PX / 2.0, fill: colour, ring: Some((t::MARKER_RING_PX, surface)) });
        }
        let tip = format!(
            "{}: {}\n{} rows\nMax {}\nQ3 {}\nMedian {}\nQ1 {}\nMin {}",
            data.spec.category,
            bx.category,
            crate::models::format_with_commas(bx.n as f64, 0),
            crate::models::format_with_commas(bx.max, opts.decimals),
            crate::models::format_with_commas(bx.q3, opts.decimals),
            crate::models::format_with_commas(bx.median, opts.decimals),
            crate::models::format_with_commas(bx.q1, opts.decimals),
            crate::models::format_with_commas(bx.min, opts.decimals)
        );
        let full = Rect::from_min_max(pos2(x - w / 2.0, hw.min(lw)), pos2(x + w / 2.0, hw.max(lw)));
        b.out.marks.push(Mark { rect: full, kind: MarkKind::Box, panel: pidx, series: 0, category: i, value: Some(bx.median), tip, slice: None });
    }
}

// ----------------------------------------------------------------------------------- scatter, bubble

/// Scatter and bubble: two numeric axes, the points (at most 5,000 a series), the colour by series.
fn scatter_form(b: &mut Builder, data: &ChartData, body: Rect, opts: &LayoutOpts, cm: &ColourMap) {
    let ink = b.ink;
    let (ink_c, ink2_c, muted_c, grid_c, base_c) = (rgb(ink.ink), rgb(ink.ink2), rgb(ink.muted), rgb(ink.grid), rgb(ink.baseline));
    let surface = rgb(ink.surface);
    let spec = &data.spec;
    let mut area = body;
    let n_ser = data.series.len();
    let colour_of = |si: usize| -> Color32 {
        let name = data.series.get(si).map(String::as_str).unwrap_or("");
        let base = if name.is_empty() { Colour::Slot(0) } else { cm.get(name).unwrap_or(Colour::Slot(si.min(t::SLOTS - 1))) };
        let c = rgb(base.hex(opts.dark));
        match &spec.highlight {
            Some(h) if !h.is_empty() && name != h && n_ser > 1 => rgb(ink.deemph),
            _ => c,
        }
    };
    if n_ser >= 2 {
        let names: Vec<(String, Color32)> = (0..n_ser).map(|i| (data.series[i].clone(), colour_of(i))).collect();
        let y = legend_row(b, &names, area);
        area = Rect::from_min_max(pos2(area.left(), y), area.max);
    }
    let (xs0, xs1) = data.points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| (a.0.min(p.x), a.1.max(p.x)));
    let (ys0, ys1) = data.points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| (a.0.min(scaled(p.y, ValueKind::Amount, opts)), a.1.max(scaled(p.y, ValueKind::Amount, opts))));
    let tick_h = b.size("0", t::TICK_PT, Face::Regular).y;
    let ax = make_axis(xs0, xs1, false, ValueKind::Plain, 7);
    let ay = make_axis(ys0, ys1, false, ValueKind::Plain, ((area.height() / 46.0).floor() as usize).clamp(3, 7));
    let ylab = tick_labels(&ay);
    let left = ylab.iter().map(|s| b.size(s, t::TICK_PT, Face::Regular).x).fold(0.0, f32::max) + t::MIN_SPACE_PX;
    let ml = spec.measure_list();
    let (xname, yname) = (spec.category.clone(), ml.first().and_then(|m| m.column.clone()).unwrap_or_default());
    let top = area.top() + tick_h + t::MIN_SPACE_PX; // the y axis name stands above the plot
    let plot = Rect::from_min_max(pos2(area.left() + left, top), pos2(area.right() - t::MIN_SPACE_PX * 2.0, area.bottom() - tick_h * 2.0 - t::MIN_SPACE_PX * 2.0));
    if plot.width() < 20.0 || plot.height() < 20.0 {
        return;
    }
    let (sx, sy) = (plot.width() / (ax.hi - ax.lo) as f32, plot.height() / (ay.hi - ay.lo) as f32);
    let px = |x: f64| plot.left() + (x - ax.lo) as f32 * sx;
    let py = |y: f64| plot.bottom() - (y - ay.lo) as f32 * sy;
    b.out.panels.push(PanelLayout { kind: spec.kind, title: String::new(), plot, lo: ay.lo, hi: ay.hi, horizontal: false, ticks: ay.ticks.iter().map(|v| (*v, py(*v))).collect() });
    for (v, s) in ay.ticks.iter().zip(ylab.iter()) {
        b.line(pos2(plot.left(), py(*v)), pos2(plot.right(), py(*v)), t::GRID_PX, grid_c);
        b.put(Role::Tick, s, Anchor::MidRight, pos2(plot.left() - t::MIN_SPACE_PX, py(*v)), t::TICK_PT, Face::Regular, muted_c);
    }
    b.line(pos2(plot.left(), plot.bottom()), pos2(plot.right(), plot.bottom()), t::GRID_PX, base_c);
    let xlab = tick_labels(&ax);
    let mut last_right = f32::NEG_INFINITY;
    for (v, s) in ax.ticks.iter().zip(xlab.iter()) {
        let r = b.rect_for(s, t::TICK_PT, Face::Regular, Anchor::TopCentre, pos2(px(*v), plot.bottom() + t::MIN_SPACE_PX / 2.0));
        if r.left() > last_right + 6.0 && r.right() <= b.bounds.right() {
            last_right = r.right();
            b.put(Role::Tick, s, Anchor::TopCentre, pos2(px(*v), plot.bottom() + t::MIN_SPACE_PX / 2.0), t::TICK_PT, Face::Regular, muted_c);
        }
    }
    b.put(Role::Axis, &yname, Anchor::TopLeft, pos2(area.left(), area.top()), t::TICK_PT, Face::Regular, ink2_c);
    b.put(Role::Axis, &xname, Anchor::TopRight, pos2(plot.right(), plot.bottom() + tick_h + t::MIN_SPACE_PX * 1.5), t::TICK_PT, Face::Regular, ink2_c);
    let bubble = spec.kind == ChartKind::Bubble;
    let max_size = data.points.iter().filter_map(|p| p.size).fold(0.0f64, f64::max);
    let rings = data.points.len() <= RING_MAX_POINTS;
    for (i, p) in data.points.iter().enumerate() {
        let c = pos2(px(p.x), py(scaled(p.y, ValueKind::Amount, opts)));
        let colour = colour_of(p.series);
        let r = if bubble && max_size > 0.0 {
            BUBBLE_MIN_R + ((p.size.unwrap_or(0.0).max(0.0) / max_size).sqrt() as f32) * (BUBBLE_MAX_R - BUBBLE_MIN_R)
        } else {
            t::MARKER_PX / 2.0
        };
        let fill = if bubble { alpha(colour, BUBBLE_ALPHA) } else { colour };
        b.out.prims.push(Prim::Circle { c, r, fill, ring: if rings { Some((t::MARKER_RING_PX, surface)) } else { None } });
        let rect = Rect::from_center_size(c, vec2(r * 2.0, r * 2.0));
        let mut tip = format!("{}: {}\n{}: {}", xname, crate::models::format_with_commas(p.x, 2), yname, crate::models::format_with_commas(scaled(p.y, ValueKind::Amount, opts), opts.decimals));
        if let Some(sz) = p.size {
            tip.push_str(&format!("\nSize: {}", crate::models::format_with_commas(sz, 2)));
        }
        if let Some(n) = data.series.get(p.series).filter(|n| !n.is_empty()) {
            tip.push_str(&format!("\n{}: {}", spec.series.clone().unwrap_or_else(|| "Series".to_string()), n));
        }
        b.out.marks.push(Mark { rect, kind: MarkKind::Point, panel: 0, series: p.series, category: i, value: Some(p.y), tip, slice: None });
    }
    let _ = ink_c;
}

// ------------------------------------------------------------------------------------------ heatmap

fn heatmap_form(b: &mut Builder, data: &ChartData, body: Rect, opts: &LayoutOpts) {
    let ink = b.ink;
    let ink_c = rgb(ink.ink);
    let ink2_c = rgb(ink.ink2);
    let transform = data.spec.measure_list().first().map(|m| m.transform).unwrap_or(Transform::None);
    let h = heatmap(data, Transform::None);
    let vk = value_kind(&data.spec, 0);
    let (rows, cols) = (h.rows.len(), h.cols.len());
    if rows == 0 || cols == 0 {
        return;
    }
    let lh = b.size("Ag", t::TICK_PT, Face::Regular).y;
    let row_w = h.rows.iter().map(|r| b.size(r, t::TICK_PT, Face::Regular).x).fold(0.0, f32::max).min(body.width() * MAX_CAT_LABEL_W_SHARE) + t::MIN_SPACE_PX;
    let grid = Rect::from_min_max(pos2(body.left() + row_w, body.top() + lh + t::MIN_SPACE_PX), body.max);
    let (cw, ch) = (grid.width() / cols as f32, grid.height() / rows as f32);
    let (lo, hi) = (h.min.unwrap_or(0.0), h.max.unwrap_or(1.0));
    for (ci, c) in h.cols.iter().enumerate() {
        let shown = fit_text(c, cw - 2.0, &b.width_fn(t::TICK_PT, Face::Regular));
        b.put(Role::Category, &shown, Anchor::TopCentre, pos2(grid.left() + cw * (ci as f32 + 0.5), body.top()), t::TICK_PT, Face::Regular, ink_c);
    }
    for (ri, r) in h.rows.iter().enumerate() {
        let shown = fit_text(r, row_w - t::MIN_SPACE_PX, &b.width_fn(t::TICK_PT, Face::Regular));
        b.put(Role::Category, &shown, Anchor::MidRight, pos2(grid.left() - t::MIN_SPACE_PX, grid.top() + ch * (ri as f32 + 0.5)), t::TICK_PT, Face::Regular, ink_c);
    }
    let gap = t::HEAT_GAP_PX;
    for ri in 0..rows {
        for ci in 0..cols {
            let Some(v) = h.cells[ri][ci] else { continue };
            let missing = h.counts.get(ri).and_then(|r| r.get(ci)).copied().unwrap_or(0) == 0;
            let cell = Rect::from_min_size(pos2(grid.left() + cw * ci as f32 + gap / 2.0, grid.top() + ch * ri as f32 + gap / 2.0), vec2((cw - gap).max(1.0), (ch - gap).max(1.0)));
            // a missing pair is a 0.0 cell in the lightest step
            let step = if missing { 0 } else { sequential_step(v, lo, hi) };
            let fill = rgb(t::SEQUENTIAL[step]);
            let radius = t::HEAT_RADIUS_PX.min(cell.width().min(cell.height()) / 2.0);
            b.rect(cell, fill, [radius; 4]);
            let text = if vk == ValueKind::Percent { crate::models::format_with_commas(v, 1) } else { value_text(scaled(v, vk, opts), vk, opts.decimals, false) };
            // the pale steps are pale in the dark theme too, so their text is the light theme's dark ink
            let tc = if step >= t::SEQUENTIAL_WHITE_TEXT_FROM { Color32::WHITE } else { rgb(t::INK_LIGHT.ink) };
            let tr = b.rect_for(&text, t::DATA_LABEL_PT, Face::Regular, Anchor::Centre, cell.center());
            if cell.contains_rect(tr) {
                b.put(Role::Cell, &text, Anchor::Centre, cell.center(), t::DATA_LABEL_PT, Face::Regular, tc);
            }
            let name = if data.spec.series.is_some() { format!("{}: {}\n{}: {}", data.spec.series.clone().unwrap_or_default(), h.rows[ri], data.spec.category, h.cols[ci]) } else { format!("{}: {}", data.spec.category, h.cols[ci]) };
            let rows_n = h.counts[ri][ci];
            let mut tip = format!("{}\n{}: {}\n{} rows", name, data.measure_names.first().cloned().unwrap_or_else(|| "Value".to_string()), if vk == ValueKind::Percent { format!("{}%", crate::models::format_with_commas(v, 2)) } else { value_text(scaled(v, vk, opts), vk, opts.decimals, false) }, crate::models::format_with_commas(rows_n as f64, 0));
            if vk == ValueKind::Percent {
                tip.push_str(&format!("\nPercent {}", base_word(transform)));
                if let Some(Some(tot)) = h.row_totals.get(ri) {
                    if transform == Transform::ShareOfSeries {
                        tip.push_str(&format!(" ({} in the row)", value_text(scaled(*tot, ValueKind::Amount, opts), ValueKind::Amount, opts.decimals, false)));
                    }
                }
            }
            b.out.marks.push(Mark { rect: cell, kind: MarkKind::Cell, panel: 0, series: ri, category: ci, value: Some(v), tip, slice: None });
        }
    }
    let _ = ink2_c;
    b.out.panels.push(PanelLayout { kind: ChartKind::Heatmap, title: String::new(), plot: grid, lo, hi, horizontal: false, ticks: Vec::new() });
}

// -------------------------------------------------------------------------------------------- donut

fn donut_form(b: &mut Builder, data: &ChartData, body: Rect, opts: &LayoutOpts, cm: &mut ColourMap) {
    let ink = b.ink;
    let ink_c = rgb(ink.ink);
    let ink2_c = rgb(ink.ink2);
    let vk = value_kind(&data.spec, 0);
    let parts: Vec<(String, f64, u64)> = data
        .categories
        .iter()
        .enumerate()
        .filter_map(|(i, c)| data.values.first().and_then(|r| r.get(i)).copied().flatten().map(|v| (c.clone(), v, data.rows.first().and_then(|r| r.get(i)).copied().unwrap_or(0))))
        .collect();
    let refuse = |b: &mut Builder, msg: &str| {
        let w = b.width_fn(t::SUBTITLE_PT, Face::Regular);
        let lines = wrap_text(msg, body.width().min(520.0), &w);
        let lh = b.size("Ag", t::SUBTITLE_PT, Face::Regular).y;
        let top = body.center().y - lh * lines.len() as f32 / 2.0;
        for (i, l) in lines.iter().enumerate() {
            b.put(Role::Message, l, Anchor::TopCentre, pos2(body.center().x, top + lh * i as f32), t::SUBTITLE_PT, Face::Regular, ink2_c);
        }
        b.out.message = Some(msg.to_string());
    };
    if parts.len() > t::DONUT_MAX_SLICES || parts.len() < 2 {
        let msg = if parts.len() > t::DONUT_MAX_SLICES { DONUT_REFUSAL.to_string() } else { "A donut needs 2 to 5 parts. Use a bar chart for a single value.".to_string() };
        refuse(b, &msg);
        return;
    }
    if parts.iter().any(|p| p.1 < 0.0) {
        refuse(b, "A donut shows parts of a whole, so every part must be positive. Use a bar chart for values with negatives.");
        return;
    }
    let total: f64 = parts.iter().map(|p| p.1).sum();
    if total <= 0.0 {
        refuse(b, "A donut needs parts that add up to more than zero.");
        return;
    }
    cm.assign_all(&parts.iter().map(|p| p.0.clone()).collect::<Vec<_>>());
    let legend_w = parts.iter().map(|p| b.size(&format!("{}  {}", p.0, value_text(scaled(p.1, vk, opts), vk, opts.decimals, false)), t::LABEL_PT, Face::Regular).x).fold(0.0, f32::max) + 40.0;
    let ring_area = Rect::from_min_max(body.min, pos2((body.right() - legend_w - t::MIN_SPACE_PX).max(body.left() + 60.0), body.bottom()));
    let r_out = (ring_area.width().min(ring_area.height()) / 2.0 - t::MIN_SPACE_PX).max(10.0);
    let c = ring_area.center();
    let r_in = r_out * DONUT_HOLE;
    let gap_angle = t::SEGMENT_GAP_PX / ((r_out + r_in) / 2.0);
    let mut a = 0.0f32;
    for (i, (name, v, rows)) in parts.iter().enumerate() {
        let frac = (*v / total) as f32;
        let (a0, a1) = (a, a + frac * std::f32::consts::TAU);
        a = a1;
        let colour = rgb(cm.get(name).unwrap_or(Colour::Slot(i)).hex(opts.dark));
        let (s0, s1) = (a0 + gap_angle / 2.0, (a1 - gap_angle / 2.0).max(a0 + gap_angle / 2.0));
        let steps = (((s1 - s0) / 0.05).ceil() as usize).max(1);
        let pt = |ang: f32, r: f32| pos2(c.x + r * ang.sin(), c.y - r * ang.cos());
        let mut tris = Vec::new();
        for k in 0..steps {
            let (x0, x1) = (s0 + (s1 - s0) * k as f32 / steps as f32, s0 + (s1 - s0) * (k + 1) as f32 / steps as f32);
            tris.push([pt(x0, r_out), pt(x1, r_out), pt(x1, r_in)]);
            tris.push([pt(x0, r_out), pt(x1, r_in), pt(x0, r_in)]);
        }
        b.out.prims.push(Prim::Tris { tris, fill: colour });
        let mid = (a0 + a1) / 2.0;
        let rect = Rect::from_center_size(pt(mid, (r_out + r_in) / 2.0), vec2(r_out - r_in, r_out - r_in));
        let tip = format!("{}: {}\n{}: {}\n{}% of the total\n{} rows", data.spec.category, name, data.measure_names.first().cloned().unwrap_or_else(|| "Value".to_string()), value_text(scaled(*v, vk, opts), vk, opts.decimals, false), crate::models::format_with_commas(*v / total * 100.0, 1), crate::models::format_with_commas(*rows as f64, 0));
        b.out.marks.push(Mark { rect, kind: MarkKind::Slice, panel: 0, series: 0, category: i, value: Some(*v), tip, slice: Some(SliceShape { centre: c, r_in, r_out, a0, a1 }) });
    }
    b.put(Role::Centre, &value_text(scaled(total, vk, opts), vk, opts.decimals, false), Anchor::Centre, pos2(c.x, c.y - 2.0), t::TITLE_PT, Face::Semibold, ink_c);
    b.put(Role::Centre, "Total", Anchor::TopCentre, pos2(c.x, c.y + t::TITLE_PT * 0.6), t::LABEL_PT, Face::Regular, ink2_c);
    // legend on the right with the values and shares
    let lh = b.size("Ag", t::LABEL_PT, Face::Regular).y + 8.0;
    let top = body.center().y - lh * parts.len() as f32 / 2.0;
    let sw = t::MARKER_PX + 2.0;
    for (i, (name, v, _)) in parts.iter().enumerate() {
        let y = top + lh * i as f32;
        let colour = rgb(cm.get(name).unwrap_or(Colour::Slot(i)).hex(opts.dark));
        let x = body.right() - legend_w;
        let swatch = Rect::from_min_size(pos2(x, y + (lh - sw) / 2.0), vec2(sw, sw));
        b.rect(swatch, colour, [2.0; 4]);
        let text = format!("{}  {} ({}%)", name, value_text(scaled(*v, vk, opts), vk, opts.decimals, false), crate::models::format_with_commas(*v / total * 100.0, 0));
        let tr = b.put(Role::Legend, &text, Anchor::MidLeft, pos2(x + sw + 6.0, y + lh / 2.0), t::LABEL_PT, Face::Regular, ink2_c);
        b.out.legend.push(LegendItem { name: name.clone(), swatch, text: tr, colour });
    }
    b.out.panels.push(PanelLayout { kind: ChartKind::Donut, title: String::new(), plot: ring_area, lo: 0.0, hi: 1.0, horizontal: false, ticks: Vec::new() });
}

/// The message shown when a donut has more than 5 parts.
pub const DONUT_REFUSAL: &str = "Use a bar chart: more than 5 parts are hard to compare as slices";

// ------------------------------------------------------------------------------------------ hover

/// What the pointer is over.
#[derive(Clone, Debug, PartialEq)]
pub struct Hover {
    pub tip: String,
    /// The mark to outline (none for a crosshair).
    pub rect: Option<Rect>,
    /// A crosshair line: x, y0, y1.
    pub cross: Option<(f32, f32, f32)>,
}

fn in_slice(s: &SliceShape, p: Pos2) -> bool {
    let d = p - s.centre;
    let r = d.length();
    if r < s.r_in || r > s.r_out {
        return false;
    }
    let mut ang = d.x.atan2(-d.y);
    if ang < 0.0 {
        ang += std::f32::consts::TAU;
    }
    ang >= s.a0 && ang <= s.a1
}

/// The mark (or crosshair) under `pos`: the hit target is larger than the mark. Marks win over crosshairs.
pub fn hit_test(l: &ChartLayout, pos: Pos2) -> Option<Hover> {
    if l.message.is_some() {
        return None;
    }
    let mut best: Option<(f32, &Mark)> = None;
    for m in &l.marks {
        let hit = match &m.slice {
            Some(s) => in_slice(s, pos),
            None => m.rect.expand(t::HIT_SLOP_PX).contains(pos),
        };
        if hit {
            let d = (m.rect.center() - pos).length();
            if best.map(|(bd, _)| d < bd).unwrap_or(true) {
                best = Some((d, m));
            }
        }
    }
    if let Some((_, m)) = best {
        return Some(Hover { tip: m.tip.clone(), rect: Some(m.rect), cross: None });
    }
    // a line chart: the crosshair of the nearest category, inside its plot
    let mut near: Option<(f32, &Crosshair)> = None;
    for c in &l.crosshairs {
        let Some(panel) = l.panels.get(c.panel) else { continue };
        if !panel.plot.expand2(vec2(t::HIT_SLOP_PX, 0.0)).contains(pos) {
            continue;
        }
        let d = (c.x - pos.x).abs();
        if near.map(|(bd, _)| d < bd).unwrap_or(true) {
            near = Some((d, c));
        }
    }
    near.map(|(_, c)| Hover { tip: c.tip.clone(), rect: None, cross: Some((c.x, c.y0, c.y1)) })
}

/// Where to put a tooltip box of `size` for a pointer at `pos`, kept inside `bounds`.
pub fn tooltip_rect(pos: Pos2, size: Vec2, bounds: Rect) -> Rect {
    let mut p = pos + vec2(12.0, 12.0);
    if p.x + size.x > bounds.right() {
        p.x = (pos.x - 12.0 - size.x).max(bounds.left());
    }
    if p.y + size.y > bounds.bottom() {
        p.y = (pos.y - 12.0 - size.y).max(bounds.top());
    }
    Rect::from_min_size(p, size)
}

/// The padding inside the tooltip box.
pub const TOOLTIP_PADDING: f32 = TOOLTIP_PAD;

/// The status colour of a reference line, for the Table view and tests.
pub fn status_hex(s: Status) -> u32 {
    s.hex()
}

/// The minus sign used on the value axis.
pub const AXIS_MINUS: char = MINUS;
