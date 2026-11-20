//! Category labels (Phase 7 final fixes, item 2): every category keeps its label, in all four renderers.
//! The rule, one for all: the full text when it fits its slot; else every long label wrapped to two lines
//! (at a space); else every label angled 35°. A label is never cut, never left out. When even the angled
//! labels would run into each other the chart says so ("Labels don't fit; try horizontal bars").

/// The angle of angled labels, in degrees.
pub const ANGLE_DEG: f32 = 35.0;

/// How the category labels of a chart are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatFit {
    /// Each on one line, level.
    Full,
    /// The long ones on two lines, level.
    Wrapped,
    /// All on one line, angled 35° up to the right, ending at the tick.
    Angled,
}

/// The decision for one axis.
#[derive(Clone, Debug)]
pub struct CatPlan {
    pub fit: CatFit,
    /// The labels as written (a wrapped one has a line break), same order as the categories.
    pub labels: Vec<String>,
    /// The height the labels need under the plot.
    pub height: f32,
    /// False when even angled labels run into each other.
    pub fits: bool,
}

/// Splits a label into two lines at the space that makes the longer line shortest; `None` when it has no space.
pub fn wrap_two(label: &str, width_of: &dyn Fn(&str) -> f32) -> Option<(String, String)> {
    let mut best: Option<(f32, String, String)> = None;
    for (i, c) in label.char_indices() {
        if c != ' ' {
            continue;
        }
        let (a, b) = (label[..i].trim_end().to_string(), label[i + 1..].trim_start().to_string());
        if a.is_empty() || b.is_empty() {
            continue;
        }
        let w = width_of(&a).max(width_of(&b));
        if best.as_ref().is_none_or(|x| w < x.0) {
            best = Some((w, a, b));
        }
    }
    best.map(|(_, a, b)| (a, b))
}

/// Plans the labels of a vertical axis: `band` is the width of one category, `width_of` measures a text, `line_h` is
/// the height of one line.
pub fn plan(names: &[String], band: f32, width_of: &dyn Fn(&str) -> f32, line_h: f32) -> CatPlan {
    let slot = (band - 4.0).max(1.0);
    let widest = names.iter().map(|n| width_of(n)).fold(0.0, f32::max);
    if widest <= slot {
        return CatPlan { fit: CatFit::Full, labels: names.to_vec(), height: line_h, fits: true };
    }
    // wrapped: every label that is too wide is split in two, and then every line fits
    let mut wrapped: Vec<String> = Vec::with_capacity(names.len());
    let mut ok = true;
    for n in names {
        if width_of(n) <= slot {
            wrapped.push(n.clone());
            continue;
        }
        match wrap_two(n, width_of) {
            Some((a, b)) if width_of(&a).max(width_of(&b)) <= slot => wrapped.push(format!("{}\n{}", a, b)),
            _ => {
                ok = false;
                break;
            }
        }
    }
    if ok {
        let two = wrapped.iter().any(|w| w.contains('\n'));
        return CatPlan { fit: CatFit::Wrapped, labels: wrapped, height: if two { line_h * 2.0 } else { line_h }, fits: true };
    }
    // angled: the labels run up to the right; neighbours are far enough apart when the band, seen along the slope, is a line high
    let (s, c) = (ANGLE_DEG.to_radians().sin(), ANGLE_DEG.to_radians().cos());
    let height = widest * s + line_h * c + 2.0;
    CatPlan { fit: CatFit::Angled, labels: names.to_vec(), height, fits: band * s >= line_h }
}

/// The widest label line (what a renderer reserves beside horizontal bars): the full names.
pub fn longest(names: &[String], width_of: &dyn Fn(&str) -> f32) -> f32 {
    names.iter().map(|n| width_of(n)).fold(0.0, f32::max)
}
