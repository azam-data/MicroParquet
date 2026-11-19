//! The chart design tokens of `docs/phase7-spec.md` (`<design_system>`): the colours, sizes and the
//! typeface. Every renderer (the egui painter, the ECharts HTML, the Excel charts) reads these
//! values, so the look is the same everywhere. `ui/theme.rs` exposes them as `Color32`/`f32`
//! tokens for the UI; they live here because the HTML and Excel renderers are in the library and
//! cannot reach the binary's theme. Any change to a colour must be re-validated with the dataviz
//! palette validator (the set is colour-vision-deficiency safe), and `tests/phase7/c0.rs` pins them.

/// One colour set: surface, inks, grid and the greys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChartInk {
    pub surface: u32,
    /// Titles and values.
    pub ink: u32,
    /// Subtitles and labels.
    pub ink2: u32,
    /// Ticks and the footnote.
    pub muted: u32,
    /// 1 px solid gridlines.
    pub grid: u32,
    /// The baseline and the zero line.
    pub baseline: u32,
    /// De-emphasised series (older curves) and "Other".
    pub deemph: u32,
}

pub const INK_LIGHT: ChartInk =
    ChartInk { surface: 0xfcfcfb, ink: 0x0b0b0b, ink2: 0x52514e, muted: 0x898781, grid: 0xe1e0d9, baseline: 0xc3c2b7, deemph: 0xc3c2b7 };
pub const INK_DARK: ChartInk =
    ChartInk { surface: 0x1a1a19, ink: 0xf0efec, ink2: 0xc3c2b7, muted: 0x898781, grid: 0x2c2c2a, baseline: 0x383835, deemph: 0x55544f };

pub fn ink(dark: bool) -> ChartInk {
    if dark {
        INK_DARK
    } else {
        INK_LIGHT
    }
}

/// The categorical slots, in fixed order, never cycled: blue, orange, aqua, yellow, magenta, green, violet, red.
pub const SLOTS: usize = 8;
pub const SERIES_LIGHT: [u32; SLOTS] = [0x2a78d6, 0xeb6834, 0x1baf7a, 0xeda100, 0xe87ba4, 0x008300, 0x6250d6, 0xe34948];
pub const SERIES_DARK: [u32; SLOTS] = [0x3987e5, 0xd95926, 0x199e70, 0xc98500, 0xd55181, 0x0ca30c, 0x9085e9, 0xe66767];
pub const SLOT_NAMES: [&str; SLOTS] = ["blue", "orange", "aqua", "yellow", "magenta", "green", "violet", "red"];

pub fn series_colour(slot: usize, dark: bool) -> Option<u32> {
    let t = if dark { &SERIES_DARK } else { &SERIES_LIGHT };
    t.get(slot).copied()
}

/// The sequential ramp for heatmaps: one hue (blue), 13 steps.
pub const SEQUENTIAL: [u32; 13] =
    [0xcde2fb, 0xb7d3f6, 0x9ec5f4, 0x86b6ef, 0x6da7ec, 0x5598e7, 0x3987e5, 0x2a78d6, 0x256abf, 0x1c5cab, 0x184f95, 0x104281, 0x0d366b];
/// Heatmap cell text is white from this step (0-based: step 7 is `#3987e5`) and darker, ink below.
pub const SEQUENTIAL_WHITE_TEXT_FROM: usize = 6;

/// The sequential step for a value's rank (0 = the lowest value, 12 = the highest) in each theme.
/// Light: low is the lightest step, high the darkest. Dark: low is the darkest step (nearest the
/// surface), high the lightest, so the eye still goes to the large values.
pub fn sequential_index(rank: usize, dark: bool) -> usize {
    let top = SEQUENTIAL.len() - 1;
    let r = rank.min(top);
    if dark {
        top - r
    } else {
        r
    }
}

/// The fill colour of a rank in a theme.
pub fn sequential_colour(rank: usize, dark: bool) -> u32 {
    SEQUENTIAL[sequential_index(rank, dark)]
}

/// Whether the cell text on that fill is white (the fill's step is 7 or darker), in either theme.
pub fn sequential_text_white(rank: usize, dark: bool) -> bool {
    sequential_index(rank, dark) >= SEQUENTIAL_WHITE_TEXT_FROM
}

/// Diverging: positive is blue, negative is red; the zero or midpoint is grey, never a hue.
pub const DIVERGING_POSITIVE: u32 = 0x2a78d6;
pub const DIVERGING_NEGATIVE: u32 = 0xe34948;
pub const DIVERGING_ZERO: u32 = 0xc3c2b7;

/// Status colours: reserved, never a series colour, always shown with a text label.
pub const STATUS_GOOD: u32 = 0x0ca30c;
pub const STATUS_WARNING: u32 = 0xfab219;
pub const STATUS_SERIOUS: u32 = 0xec835a;
pub const STATUS_CRITICAL: u32 = 0xd03b3b;

/// Waterfall totals (start and end bars), as in the reference rendering `02_nii_bridge.png`.
pub const WATERFALL_START: u32 = 0x52514e;
pub const WATERFALL_END: u32 = 0x0d366b;

/// `0xRRGGBB` as "#rrggbb" (HTML and Excel).
pub fn hex_text(rgb: u32) -> String {
    format!("#{:06x}", rgb & 0xFF_FFFF)
}

// ---- typeface and sizes (pt) ----
pub const FONT_HTML: &str = "\"Source Sans 3\", \"Segoe UI\", system-ui, sans-serif";
pub const FONT_EXCEL: &str = "Segoe UI";
pub const TITLE_PT: f32 = 15.0;
pub const SUBTITLE_PT: f32 = 11.0;
pub const TICK_PT: f32 = 10.0;
pub const LABEL_PT: f32 = 10.0;
pub const DATA_LABEL_PT: f32 = 9.5;
pub const FOOTNOTE_PT: f32 = 9.0;

// ---- marks (px) ----
/// Bars never fill their band: at most this thick.
pub const BAR_MAX_PX: f32 = 24.0;
/// The rounded data-end of a bar (square at the baseline).
pub const BAR_RADIUS_PX: f32 = 4.0;
/// The gap between stacked segments and touching bars, in the surface colour.
pub const SEGMENT_GAP_PX: f32 = 2.0;
pub const LINE_PX: f32 = 2.0;
pub const MARKER_PX: f32 = 8.0;
pub const MARKER_RING_PX: f32 = 2.0;
/// The fill under an area, as a share of the series colour's opacity.
pub const AREA_OPACITY: f32 = 0.10;
/// The fill of a stacked area layer (a 2 px line of the full colour runs along its top).
pub const STACKED_AREA_OPACITY: f32 = 0.35;
pub const GRID_PX: f32 = 1.0;
/// Every bar and waterfall step is at least this tall, so a small value is still a visible mark.
pub const MIN_BAR_PX: f32 = 2.0;
pub const REFERENCE_LINE_PX: f32 = 1.5;
/// Heatmap cell corner radius and the surface gap between cells (owner decision, Session A review).
pub const HEAT_RADIUS_PX: f32 = 2.0;
pub const HEAT_GAP_PX: f32 = 2.0;
/// Padding around the plot, as a share of the chart.
pub const PADDING_SHARE: f32 = 0.03;
/// No element closer than this to another.
pub const MIN_SPACE_PX: f32 = 8.0;
/// The hit target of a mark is larger than the mark by this much on each side.
pub const HIT_SLOP_PX: f32 = 6.0;

// ---- data limits (conventions) ----
/// At most this many points per series; above it a scatter shows a sample and a line asks for an aggregation.
pub const MAX_POINTS: usize = 5_000;
/// A donut has 2 to this many slices.
pub const DONUT_MAX_SLICES: usize = 5;
/// Small multiples: at most this many panels.
pub const MAX_FACETS: usize = 12;
/// Linked panels: at most this many.
pub const MAX_PANELS: usize = 3;
/// Scatter, bubble and small-multiple colour series are capped at this many.
pub const MAX_SCATTER_SERIES: usize = 3;

/// Writes the change symbol as a delta sign: "dEVE" becomes "ΔEVE" and "dNII" "ΔNII" (whole words only),
/// in every renderer (titles, axes, tooltips, HTML, Excel).
pub fn delta(text: &str) -> String {
    if !text.contains("dEVE") && !text.contains("dNII") {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 2);
    let mut i = 0;
    while i < chars.len() {
        let rest: String = chars[i..chars.len().min(i + 4)].iter().collect();
        let before_ok = i == 0 || !chars[i - 1].is_alphanumeric();
        let after = chars.get(i + 4).copied();
        let after_ok = after.map_or(true, |c| !c.is_alphanumeric());
        if before_ok && after_ok && (rest == "dEVE" || rest == "dNII") {
            out.push('\u{394}');
            out.push_str(&rest[1..]);
            i += 4;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// The true minus sign (U+2212) for a hyphen written before a number: "-10.5%" becomes "−10.5%".
/// A hyphen inside a word or between digits ("Mar-24", "2024-01-31") is left alone.
pub fn minus(text: &str) -> String {
    if !text.contains('-') {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 2);
    for (i, &c) in chars.iter().enumerate() {
        let next_digit = chars.get(i + 1).map_or(false, |n| n.is_ascii_digit() || (*n == '.' && chars.get(i + 2).map_or(false, |d| d.is_ascii_digit())));
        let prev_word = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
        if c == '-' && next_digit && !prev_word {
            out.push('\u{2212}');
        } else {
            out.push(c);
        }
    }
    out
}

/// The first hyphen written before a digit that should be a true minus, if any (for tests).
pub fn find_hyphen_minus(text: &str) -> Option<String> {
    let m = minus(text);
    if m == text {
        return None;
    }
    let pos = text.chars().zip(m.chars()).position(|(a, b)| a != b)?;
    let lo = pos.saturating_sub(12);
    Some(text.chars().skip(lo).take(30).collect())
}

/// Every chart string goes through this: the delta sign for "dEVE"/"dNII" and the true minus sign.
pub fn tidy(text: &str) -> String {
    singular_row(&minus(&delta(text)))
}

/// "1 rows" is "1 row"; every other count keeps "rows".
fn singular_row(text: &str) -> String {
    if !text.contains("1 rows") {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let pat: Vec<char> = "1 rows".chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let hit = chars[i..].starts_with(&pat) && (i == 0 || !(chars[i - 1].is_ascii_digit() || chars[i - 1] == ',' || chars[i - 1] == '.'));
        if hit {
            out.push_str("1 row");
            i += pat.len();
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}
