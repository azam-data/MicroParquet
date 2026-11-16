//! Phase 3 L2: column minimum widths and count formatting (fixed font metrics, synthetic text).

use micro_parquet::table_layout::*;

/// Every character is `w` wide, headers and selectors have their own widths.
struct Fixed {
    header: f32,
    selector: f32,
    summary: f32,
    cell: f32,
}

impl FontMetrics for Fixed {
    fn header(&self, t: &str) -> f32 {
        t.chars().count() as f32 * self.header
    }
    fn selector(&self, t: &str) -> f32 {
        t.chars().count() as f32 * self.selector
    }
    fn summary(&self, t: &str) -> f32 {
        t.chars().count() as f32 * self.summary
    }
    fn cell(&self, t: &str) -> f32 {
        t.chars().count() as f32 * self.cell
    }
}

const M: Fixed = Fixed { header: 8.0, selector: 6.0, summary: 8.0, cell: 8.0 };

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn l2_the_widest_of_the_four_sources_wins() {
    let pad = 2.0 * CELL_PADDING;
    // header: 10 chars * 8 + sort mark = 110; selector 3 chars; summary 4; samples 5 chars = 40
    let w = column_min_width("Outstandg_", "Sum", "1,23", &s(&["12345", "1"]), &M);
    assert_eq!(w, 10.0 * 8.0 + SORT_MARK_WIDTH + pad, "header widest");
    // selector widest: 30 chars * 6 + chip = 208
    let w = column_min_width("Bal", &"S".repeat(30), "1", &s(&["1"]), &M);
    assert_eq!(w, 30.0 * 6.0 + SELECTOR_CHIP_EXTRA + pad, "selector widest");
    // summary widest: 25 chars * 8 = 200
    let w = column_min_width("Bal", "Sum", &"9".repeat(25), &s(&["1"]), &M);
    assert_eq!(w, 25.0 * 8.0 + pad, "summary widest");
    // a sample widest: 28 chars * 8 = 224
    let w = column_min_width("Bal", "Sum", "1", &s(&["1", &"7".repeat(28), "22"]), &M);
    assert_eq!(w, 28.0 * 8.0 + pad, "sample widest");
}

#[test]
fn l2_width_is_capped_at_420_and_floored() {
    let long = "x".repeat(200);
    let w = column_min_width("H", "", "", &s(&[&long]), &M);
    assert_eq!(w, MAX_COLUMN_WIDTH);
    assert_eq!(MAX_COLUMN_WIDTH, 420.0);
    let tiny = column_min_width("", "", "", &[], &M);
    assert_eq!(tiny, MIN_COLUMN_WIDTH);
}

#[test]
fn l2_only_the_first_100_samples_count() {
    let mut v: Vec<String> = (0..100).map(|_| "1".to_string()).collect();
    v.push("9".repeat(40)); // row 101: not measured
    let w = column_min_width("H", "", "", &v, &M);
    assert_eq!(w, (8.0 + SORT_MARK_WIDTH + 2.0 * CELL_PADDING).max(MIN_COLUMN_WIDTH));
}

#[test]
fn l2_a_date_never_needs_two_lines_at_the_computed_width() {
    // JetBrains Mono is about 8 px per character at 13 px; use a wider test font as well.
    for cell in [8.0_f32, 8.4, 9.1] {
        let m = Fixed { header: 8.0, selector: 6.0, summary: 8.0, cell };
        let samples = s(&["01-Sep-2026", "31-Dec-2026", "15-Mar-2027"]);
        let w = column_min_width("Matdat", "Min", "01-Sep-2026", &samples, &m);
        for d in &samples {
            assert!(fits_on_one_line(d, w, &m), "{d} at {cell}px/char in {w}px");
        }
        assert!(fits_on_one_line("01-Sep-2026", w, &m));
    }
    // and a column too narrow for it is reported as not fitting
    assert!(!fits_on_one_line("01-Sep-2026", 60.0, &M));
}

#[test]
fn l2_counts_have_thousands_separators() {
    assert_eq!(format_count(20_391), "20,391");
    assert_eq!(format_count(0), "0");
    assert_eq!(format_count(999), "999");
    assert_eq!(format_count(1_000), "1,000");
    assert_eq!(format_count(12_345_678), "12,345,678");
    assert_eq!(format_row_range(1, 100, 20_391), "1\u{2013}100 of 20,391");
    assert_eq!(format_row_range(20_301, 20_391, 20_391), "20,301\u{2013}20,391 of 20,391");
    assert_eq!(format_row_range(1, 0, 0), "0 rows");
}
