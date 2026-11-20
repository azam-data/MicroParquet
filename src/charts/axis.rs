//! Axis arithmetic and number text for the chart renderers: clean round tick values, the text of a
//! tick or a data label, and word wrapping. Plain functions, no drawing.

use crate::models::format_with_commas;

/// The unicode minus used on axes (the reference renderings show "−1000", "−10%").
pub const MINUS: char = '\u{2212}';

/// What a measure's numbers mean; decides how they are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    /// Money (BDT), shown in the chart's scale.
    Amount,
    /// A share (percent of something): "12.5%".
    Percent,
    /// A count of rows or accounts.
    Count,
    /// Any other number (a rate in percent, a ratio): as it is.
    Plain,
}

/// A step (1, 2, 2.5 or 5 times a power of ten) giving about `target` intervals over `range`.
pub fn nice_step(range: f64, target: usize) -> f64 {
    if !(range > 0.0) || !range.is_finite() {
        return 1.0;
    }
    let raw = range / target.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let nice = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 2.5 {
        2.5
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

/// The tick values covering `lo..=hi` with round numbers: the first is at or below `lo`, the last at or
/// above `hi`. Returns the ticks and the step. At most about `max_ticks` ticks.
pub fn nice_ticks(lo: f64, hi: f64, max_ticks: usize) -> (Vec<f64>, f64) {
    let (mut lo, mut hi) = (lo, hi);
    if !lo.is_finite() || !hi.is_finite() {
        return (vec![0.0, 1.0], 1.0);
    }
    if lo > hi {
        std::mem::swap(&mut lo, &mut hi);
    }
    if (hi - lo).abs() < f64::EPSILON {
        // a flat series: open the range around the value
        let pad = if lo == 0.0 { 1.0 } else { lo.abs() * 0.1 };
        lo -= pad;
        hi += pad;
    }
    let step = nice_step(hi - lo, max_ticks.saturating_sub(1).max(1));
    let first = (lo / step).floor();
    let last = (hi / step).ceil();
    let n = (last - first) as usize;
    let ticks = (0..=n).map(|i| clean((first + i as f64) * step, step)).collect();
    (ticks, step)
}

/// Removes floating noise: 0.30000000000000004 -> 0.3.
/// The lowest value of an axis that starts one tick below the data's minimum.
pub fn padded_floor(lo: f64, hi: f64, max_ticks: usize) -> f64 {
    let (_, step) = nice_ticks(lo, hi, max_ticks);
    lo - step
}

fn clean(v: f64, step: f64) -> f64 {
    let d = decimals_for_step(step) + 2;
    let f = 10f64.powi(d as i32);
    let r = (v * f).round() / f;
    if r == 0.0 {
        0.0
    } else {
        r
    }
}

/// The decimals needed to write multiples of `step`: 5 -> 0, 0.5 -> 1, 0.25 -> 2.
pub fn decimals_for_step(step: f64) -> usize {
    if !(step > 0.0) || !step.is_finite() {
        return 0;
    }
    for d in 0..=6usize {
        let f = 10f64.powi(d as i32);
        let x = step * f;
        if (x - x.round()).abs() < 1e-9 * x.max(1.0) {
            return d;
        }
    }
    6
}

fn with_minus(s: String, minus: char) -> String {
    match s.strip_prefix('-') {
        Some(rest) => format!("{}{}", minus, rest),
        None => s,
    }
}

/// An axis tick: thousands separators, the decimals the step needs, a unicode minus, "%" for percentages.
pub fn tick_text(v: f64, step: f64, kind: ValueKind) -> String {
    let d = decimals_for_step(step);
    let v = if v == 0.0 { 0.0 } else { v };
    let body = format_with_commas(v, d);
    let s = match kind {
        ValueKind::Percent => format!("{}%", body),
        _ => body,
    };
    with_minus(s, MINUS)
}

/// A data label: `decimals` for amounts, one for percentages, none for counts, with the true minus
/// sign for a negative. `signed` writes a "+" before a positive (waterfall steps).
pub fn value_text(v: f64, kind: ValueKind, decimals: usize, signed: bool) -> String {
    let d = match kind {
        ValueKind::Amount | ValueKind::Plain => decimals,
        ValueKind::Percent => 1,
        ValueKind::Count => 0,
    };
    signed_body(v, kind, d, signed)
}

/// The label on a diverging bar: always a sign, and two decimals for a percentage ("+4.82%",
/// "−10.50%"); an amount keeps the chart's decimals.
pub fn diverging_text(v: f64, kind: ValueKind, decimals: usize) -> String {
    let d = match kind {
        ValueKind::Percent => 2,
        ValueKind::Count => 0,
        _ => decimals,
    };
    signed_body(v, kind, d, true)
}

pub(crate) fn signed_body(v: f64, kind: ValueKind, d: usize, signed: bool) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
    let mut s = format_with_commas(v, d);
    // a negative that rounds to nothing is a plain zero, not "−0.00"
    if s.starts_with('-') && !s.chars().any(|c| c.is_ascii_digit() && c != '0') {
        s.remove(0);
    }
    let positive = !s.starts_with('-') && s.chars().any(|c| c.is_ascii_digit() && c != '0');
    if kind == ValueKind::Percent {
        s.push('%');
    }
    if signed && positive {
        s.insert(0, '+');
    }
    with_minus(s, MINUS)
}

/// Text cut to `max_w` (as measured by `width`) with an ellipsis; the whole text when it fits.
pub fn fit_text(text: &str, max_w: f32, width: &dyn Fn(&str) -> f32) -> String {
    if width(text) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len();
    while n > 1 {
        n -= 1;
        let cut: String = chars[..n].iter().collect::<String>().trim_end().to_string() + "\u{2026}";
        if width(&cut) <= max_w {
            return cut;
        }
    }
    "\u{2026}".to_string()
}

/// Greedy word wrap to `max_w`; a single word longer than the line stays on its own line.
pub fn wrap_text(text: &str, max_w: f32, width: &dyn Fn(&str) -> f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for para in text.split('\n') {
        let mut cur = String::new();
        for word in para.split_whitespace() {
            let cand = if cur.is_empty() { word.to_string() } else { format!("{} {}", cur, word) };
            if cur.is_empty() || width(&cand) <= max_w {
                cur = cand;
            } else {
                lines.push(std::mem::take(&mut cur));
                cur = word.to_string();
            }
        }
        lines.push(cur);
    }
    lines
}
