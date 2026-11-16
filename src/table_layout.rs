//! Table layout rules (Phase 3 L2): column minimum widths and the count formatters. The width
//! calculation is a pure function of the texts and a font-metrics object, so it can be tested
//! with fixed metrics; the UI passes metrics measured from the real fonts.

use crate::models::format_with_commas;

/// Space left and right of a cell's text.
pub const CELL_PADDING: f32 = 12.0;
/// No column is wider than this at its minimum; longer text ends in "…" and shows in full on hover.
pub const MAX_COLUMN_WIDTH: f32 = 420.0;
/// No column is narrower than this.
pub const MIN_COLUMN_WIDTH: f32 = 56.0;
/// Room kept in the header for the sort mark ("1 ↑").
pub const SORT_MARK_WIDTH: f32 = 30.0;
/// The aggregation selector is a chip: its padding and the dropdown arrow.
pub const SELECTOR_CHIP_EXTRA: f32 = 28.0;
/// The summary band's colours (RGB hex); the UI's `summary_bg` / `summary_fg` tokens and the
/// totals row of Excel and PDF exports use these.
pub const SUMMARY_BG: u32 = 0x1F3A5F;
pub const SUMMARY_FG: u32 = 0xFFFFFF;
/// How many visible rows are measured.
pub const SAMPLE_ROWS: usize = 100;

/// Text widths in the fonts a column uses, in logical pixels.
pub trait FontMetrics {
    /// Header text (Source Sans 3 SemiBold 14).
    fn header(&self, text: &str) -> f32;
    /// Aggregation selector label (SemiBold 12).
    fn selector(&self, text: &str) -> f32;
    /// Summary band value (JetBrains Mono SemiBold).
    fn summary(&self, text: &str) -> f32;
    /// Cell text (mono for numbers, dates and IDs, Source Sans 3 for text).
    fn cell(&self, text: &str) -> f32;
}

/// Minimum width of a column: the widest of its header (with room for the sort mark), its
/// selector label (with chip padding), its summary value and the first 100 sample cell values,
/// plus padding on both sides, between `MIN_COLUMN_WIDTH` and `MAX_COLUMN_WIDTH`.
pub fn column_min_width(
    header: &str,
    selector_label: &str,
    summary_value: &str,
    samples: &[String],
    metrics: &dyn FontMetrics,
) -> f32 {
    let mut widest = metrics.header(header) + SORT_MARK_WIDTH;
    if !selector_label.is_empty() {
        widest = widest.max(metrics.selector(selector_label) + SELECTOR_CHIP_EXTRA);
    }
    if !summary_value.is_empty() {
        widest = widest.max(metrics.summary(summary_value));
    }
    for s in samples.iter().take(SAMPLE_ROWS) {
        widest = widest.max(metrics.cell(s));
    }
    (widest + 2.0 * CELL_PADDING).clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH)
}

/// True when `text` fits on one line in a cell of `column_width` (padding included).
pub fn fits_on_one_line(text: &str, column_width: f32, metrics: &dyn FontMetrics) -> bool {
    metrics.cell(text) + 2.0 * CELL_PADDING <= column_width + 0.01
}

/// A count with thousands separators: 20391 -> "20,391".
pub fn format_count(n: usize) -> String {
    format_with_commas(n as f64, 0)
}

/// "1–100 of 20,391" (en dash). `first` is the 1-based number of the first row shown. With no
/// rows it says "0 rows".
pub fn format_row_range(first: usize, last: usize, total: usize) -> String {
    if total == 0 || last < first {
        return "0 rows".to_string();
    }
    format!("{}\u{2013}{} of {}", format_count(first), format_count(last), format_count(total))
}
