//! The layout of a table in the PDF report (PR #6 item 17): where every header and every value
//! goes, worked out before anything is drawn, from the real widths of the characters in the font
//! the PDF uses (the built-in Helvetica, regular for values and bold for headers). The exporter
//! only draws what is here, and the tests check this data.
//!
//! * A column is as wide as its widest header line or value, plus padding on both sides. The
//!   header and the values of a column use the same x and the same width.
//! * Number columns are right-aligned in the header and in the body (they end at the same x); text
//!   columns are left-aligned in both.
//! * The unit is stated once, under the title ("Figures in BDT Crore"); no header or cell carries it.
//! * A header wider than its column's values wraps to at most 2 lines. Pivot headers of the form
//!   "Islami | Sum of Balance" become two levels: "Islami" spans its child columns.
//! * Columns that do not fit the page width go to further parts, each with the row labels repeated.

use crate::models::{format_metric_value, format_with_commas, resolved_scale, ColumnFormatSpec, NumberDisplayFormat};
use crate::summary::{date_text, SummaryCell, SummaryValue};
use polars::prelude::*;
use std::collections::HashMap;

pub const PAGE_W_MM: f32 = 297.0;
pub const PAGE_H_MM: f32 = 210.0;
pub const LEFT_MM: f32 = 12.0;
pub const USABLE_W_MM: f32 = 273.0;
/// The space above the first line of a page.
pub const TOP_MARGIN_MM: f32 = 16.0;

/// The paper of a PDF export (L2): A4 (210 × 297 mm) or Legal (8.5 × 14 in), upright or on its side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PdfPage {
    pub landscape: bool,
    pub legal: bool,
}

impl Default for PdfPage {
    /// A4 on its side: what the report always used.
    fn default() -> Self {
        Self { landscape: true, legal: false }
    }
}

impl PdfPage {
    fn short_long(&self) -> (f32, f32) {
        if self.legal { (215.9, 355.6) } else { (210.0, 297.0) }
    }

    pub fn width_mm(&self) -> f32 {
        let (s, l) = self.short_long();
        if self.landscape { l } else { s }
    }

    pub fn height_mm(&self) -> f32 {
        let (s, l) = self.short_long();
        if self.landscape { s } else { l }
    }

    /// The width the table may use (the same margin on both sides).
    pub fn usable_w_mm(&self) -> f32 {
        self.width_mm() - 2.0 * LEFT_MM
    }

    /// Where the first line of a page is written.
    pub fn top_mm(&self) -> f32 {
        self.height_mm() - TOP_MARGIN_MM
    }
}

/// `text` cut into lines no wider than `max_mm` at the given size, at spaces (a single word wider
/// than a line stays whole).
pub fn wrap_text_mm(text: &str, max_mm: f32, size_pt: f32, font: FontKind) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for para in text.split(['\n', '\r']).filter(|p| !p.trim().is_empty()) {
        let mut cur = String::new();
        for word in para.split_whitespace() {
            let next = if cur.is_empty() { word.to_string() } else { format!("{} {}", cur, word) };
            if !cur.is_empty() && text_width_mm(&next, size_pt, font) > max_mm {
                lines.push(std::mem::take(&mut cur));
                cur = word.to_string();
            } else {
                cur = next;
            }
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
    }
    lines
}
/// Space between a column's edge and its text, on each side.
pub const PAD_MM: f32 = 1.5;
pub const MIN_COL_MM: f32 = 10.0;
pub const MAX_COL_MM: f32 = 55.0;
/// A header wraps (to at most 2 lines) when it is wider than this and wider than the column's values.
pub const HEADER_WRAP_MM: f32 = 30.0;
pub const CELL_PT: f32 = 8.0;
pub const HEADER_PT: f32 = 8.5;
pub const TITLE_PT: f32 = 12.0;
pub const NOTE_PT: f32 = 8.0;
pub const ROW_H_MM: f32 = 4.8;
pub const HEADER_LINE_MM: f32 = 3.8;
/// The row labels (leading text columns) may take at most this share of the page width.
const LABEL_SHARE: f32 = 0.4;
const ELLIPSIS: &str = "...";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontKind {
    Regular,
    Bold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfAlign {
    Left,
    Right,
    Center,
}

// Adobe's metrics (AFM) of Helvetica and Helvetica-Bold, 1/1000 em, for the characters 32 to 126.
#[rustfmt::skip]
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556,
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556,
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];
#[rustfmt::skip]
const HELVETICA_BOLD: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611,
    975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556,
    333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611,
    611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
];

/// The width of one character in 1/1000 em.
fn char_units(c: char, font: FontKind) -> u32 {
    let table = match font {
        FontKind::Regular => &HELVETICA,
        FontKind::Bold => &HELVETICA_BOLD,
    };
    match c {
        ' '..='~' => table.get(c as usize - 32).copied().unwrap_or(556) as u32,
        '\u{2013}' => 556,
        '\u{2014}' | '\u{2026}' => 1000,
        '\u{0394}' => match font {
            FontKind::Regular => 667,
            FontKind::Bold => 722,
        },
        _ => match font {
            FontKind::Regular => 556,
            FontKind::Bold => 611,
        },
    }
}

/// The width of `text` in mm, set in Helvetica (regular or bold) at `size_pt`.
pub fn text_width_mm(text: &str, size_pt: f32, font: FontKind) -> f32 {
    let units: u32 = text.chars().map(|c| char_units(c, font)).sum();
    units as f32 / 1000.0 * size_pt * 25.4 / 72.0
}

/// A piece of text and where it starts: `x` is the left end of the text, `w` its width.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfText {
    pub text: String,
    pub x: f32,
    pub w: f32,
}

/// A column: its left edge and width (padding included) and its alignment.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfCol {
    pub name: String,
    pub align: PdfAlign,
    pub x: f32,
    pub w: f32,
}

impl PdfCol {
    /// Where the text of a cell of this width starts, and so where a right-aligned text ends.
    fn text_x(&self, text_w: f32) -> f32 {
        match self.align {
            PdfAlign::Left => self.x + PAD_MM,
            PdfAlign::Right => self.x + self.w - PAD_MM - text_w,
            PdfAlign::Center => self.x + (self.w - text_w) / 2.0,
        }
    }
}

/// A header over one column or over several (a group): the lines of text (at most 2) in the column
/// span `[col_x, col_x + col_w]`.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfHeaderCell {
    pub lines: Vec<PdfText>,
    pub col_x: f32,
    pub col_w: f32,
    pub align: PdfAlign,
    /// First column (index in the part) and number of columns it covers.
    pub first_col: usize,
    pub span: usize,
}

/// One value: its text and the column it is in.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfCell {
    pub text: PdfText,
    pub col_x: f32,
    pub col_w: f32,
    pub align: PdfAlign,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PdfRow {
    pub cells: Vec<PdfCell>,
    /// A total row: bold, with a rule above it.
    pub total: bool,
}

/// The summary band: per column a small label and the value (empty where there is none).
#[derive(Clone, Debug, PartialEq)]
pub struct PdfSummaryCell {
    pub label: PdfText,
    pub value: PdfCell,
}

/// The columns that fit one page width, with the row labels in front.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfPart {
    pub columns: Vec<PdfCol>,
    /// The top header level of a pivot with columns (None when the table has one level).
    pub group_header: Option<Vec<PdfHeaderCell>>,
    pub header: Vec<PdfHeaderCell>,
    pub rows: Vec<PdfRow>,
    pub summary: Vec<Option<PdfSummaryCell>>,
    /// Names of the first and last data column in this part (for "columns A to B").
    pub first_data: String,
    pub last_data: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PdfTabLayout {
    pub title: String,
    /// "Figures in BDT Crore", stated once under the title.
    pub unit_line: Option<String>,
    pub parts: Vec<PdfPart>,
}

/// The names of the units a table's money columns are shown in ("BDT Crore"), each with the
/// columns that use it.
fn unit_name(unit: &str) -> &'static str {
    match unit {
        "BDT Cr" => "BDT Crore",
        "BDT L" => "BDT Lakh",
        "BDT M" => "BDT Million",
        "BDT K" => "BDT Thousand",
        _ => "BDT",
    }
}

/// A number as the PDF shows it: scaled and with thousands separators, and no unit text.
pub fn pdf_number_text(val: f64, col_name: &str, spec: Option<&ColumnFormatSpec>, num_format: NumberDisplayFormat, global_decimals: usize) -> String {
    let dec = spec.map(|s| s.decimals).unwrap_or(global_decimals);
    match resolved_scale(col_name, spec, num_format) {
        Some(sc) => format_with_commas(val / sc.divisor, dec),
        None => format_metric_value(val, col_name, spec, num_format, global_decimals),
    }
}

fn cell_text(val: Option<AnyValue>, name: &str, spec: Option<&ColumnFormatSpec>, num_format: NumberDisplayFormat, dec: usize) -> String {
    match val {
        None | Some(AnyValue::Null) => String::new(),
        Some(AnyValue::Float64(v)) => pdf_number_text(v, name, spec, num_format, dec),
        Some(AnyValue::Float32(v)) => pdf_number_text(v as f64, name, spec, num_format, dec),
        Some(AnyValue::String(s)) => s.to_string(),
        Some(other) => format!("{}", other),
    }
}

/// `text` cut with "..." so it is at most `max_w` wide in `font` at `size_pt`.
fn fit_text(text: &str, max_w: f32, size_pt: f32, font: FontKind) -> String {
    if text_width_mm(text, size_pt, font) <= max_w {
        return text.to_string();
    }
    let dots = text_width_mm(ELLIPSIS, size_pt, font);
    let mut out = String::new();
    for c in text.chars() {
        let next = format!("{out}{c}");
        if text_width_mm(&next, size_pt, font) + dots > max_w {
            break;
        }
        out = next;
    }
    format!("{}{}", out.trim_end(), ELLIPSIS)
}

/// A header in at most 2 lines: one line when it fits `target_w`, else split at a space, "_", "-"
/// or "/" so that the first line is as long as fits; the second line is cut with "..." when it is still too long.
fn wrap_header(text: &str, target_w: f32, cap_w: f32) -> Vec<String> {
    let one = |t: &str| text_width_mm(t, HEADER_PT, FontKind::Bold);
    if one(text) <= target_w {
        return vec![text.to_string()];
    }
    // pieces that keep their delimiter at the end
    let mut pieces: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        cur.push(c);
        if matches!(c, ' ' | '_' | '-' | '/') {
            pieces.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        pieces.push(cur);
    }
    let mut first = String::new();
    let mut used = 0;
    for (i, p) in pieces.iter().enumerate() {
        let cand = format!("{first}{p}");
        if !first.is_empty() && one(cand.trim_end()) > target_w {
            break;
        }
        first = cand;
        used = i + 1;
    }
    let rest: String = pieces[used..].concat();
    let l1 = fit_text(first.trim_end(), cap_w, HEADER_PT, FontKind::Bold);
    if rest.trim().is_empty() {
        return vec![l1];
    }
    vec![l1, fit_text(rest.trim(), cap_w.max(target_w), HEADER_PT, FontKind::Bold)]
}

struct ColInfo {
    name: String,
    /// The group (the part before the last " | "), when the table has two levels.
    group: Option<String>,
    numeric: bool,
    cells: Vec<String>,
    lines: Vec<String>,
    w: f32,
}

fn is_total_label(s: &str) -> bool {
    let t = s.trim().to_lowercase();
    t == "grand total" || t == "total"
}

/// Lays out one tab. `max_rows` rows are used. The tab's summary (when it has one) is laid out as a band.
pub fn layout_pdf_tab(
    title: &str,
    df: &DataFrame,
    col_formats: &HashMap<String, ColumnFormatSpec>,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
    summary: &[SummaryCell],
    max_rows: usize,
) -> PdfTabLayout {
    layout_pdf_tab_on(&PdfPage::default(), title, df, col_formats, num_format, global_decimals, summary, max_rows)
}

/// `layout_pdf_tab` for a chosen paper: the columns that do not fit its width go to further parts.
#[allow(clippy::too_many_arguments)]
pub fn layout_pdf_tab_on(
    page: &PdfPage,
    title: &str,
    df: &DataFrame,
    col_formats: &HashMap<String, ColumnFormatSpec>,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
    summary: &[SummaryCell],
    max_rows: usize,
) -> PdfTabLayout {
    let usable_w = page.usable_w_mm();
    let cols = df.get_columns();
    let n_rows = df.height().min(max_rows);
    let inner_cap = MAX_COL_MM - 2.0 * PAD_MM;

    // the units, each with the columns that use it
    let mut units: Vec<(&'static str, Vec<String>)> = Vec::new();
    for c in cols {
        if matches!(c.dtype(), DataType::Float64 | DataType::Float32) {
            if let Some(sc) = resolved_scale(c.name().as_str(), col_formats.get(c.name().as_str()), num_format) {
                let u = unit_name(sc.unit);
                match units.iter_mut().find(|(n, _)| *n == u) {
                    Some((_, v)) => v.push(c.name().to_string()),
                    None => units.push((u, vec![c.name().to_string()])),
                }
            }
        }
    }
    let unit_line = match units.len() {
        0 => None,
        1 => Some(format!("Figures in {}", units[0].0)),
        _ => Some(format!(
            "Figures in {}",
            units.iter().map(|(u, v)| format!("{} ({})", u, v.join(", "))).collect::<Vec<_>>().join(" and ")
        )),
    };

    // total rows: the first column says "Grand total" / "Total"
    let first_text: Vec<Option<String>> = match cols.first().map(|c| c.dtype().clone()) {
        Some(DataType::String) => cols[0].str().map(|s| s.into_iter().take(n_rows).map(|v| v.map(str::to_string)).collect()).unwrap_or_default(),
        _ => Vec::new(),
    };
    let total_rows: Vec<bool> = (0..n_rows).map(|r| first_text.get(r).and_then(|v| v.as_deref()).is_some_and(is_total_label)).collect();

    // the columns: header texts (two levels for "Group | Value" pivot headers), cells, widths
    let split: Vec<Option<(String, String)>> = cols.iter().map(|c| c.name().as_str().rsplit_once(" | ").map(|(a, b)| (a.to_string(), b.to_string()))).collect();
    let two_level = {
        let grouped: Vec<&(String, String)> = split.iter().flatten().collect();
        // at least two columns share a group, so there is something to span
        grouped.iter().enumerate().any(|(i, g)| grouped.iter().enumerate().any(|(j, h)| i != j && g.0 == h.0))
    };
    let mut infos: Vec<ColInfo> = Vec::with_capacity(cols.len());
    for (ci, c) in cols.iter().enumerate() {
        let name = c.name().to_string();
        let spec = col_formats.get(name.as_str());
        let numeric = c.dtype().is_numeric();
        let mut cells: Vec<String> = (0..n_rows).map(|r| cell_text(c.get(r).ok(), &name, spec, num_format, global_decimals)).collect();
        if !numeric {
            for t in cells.iter_mut() {
                *t = fit_text(t, inner_cap, CELL_PT, FontKind::Regular);
            }
        }
        let (group, header) = match (&split[ci], two_level) {
            (Some((g, h)), true) => (Some(g.clone()), h.clone()),
            _ => (None, name.clone()),
        };
        let val_w = cells
            .iter()
            .enumerate()
            .map(|(r, t)| text_width_mm(t, CELL_PT, if total_rows.get(r).copied().unwrap_or(false) { FontKind::Bold } else { FontKind::Regular }))
            .fold(0.0f32, f32::max);
        let target = val_w.max(HEADER_WRAP_MM);
        let lines = wrap_header(&header, target, inner_cap);
        let hdr_w = lines.iter().map(|l| text_width_mm(l, HEADER_PT, FontKind::Bold)).fold(0.0f32, f32::max);
        let w = (hdr_w.max(val_w) + 2.0 * PAD_MM).clamp(MIN_COL_MM, MAX_COL_MM);
        infos.push(ColInfo { name, group, numeric, cells, lines, w });
    }
    // a group label wider than the columns it spans widens them
    if two_level {
        let mut i = 0;
        while i < infos.len() {
            if let Some(g) = infos[i].group.clone() {
                let mut j = i;
                while j + 1 < infos.len() && infos[j + 1].group.as_deref() == Some(g.as_str()) {
                    j += 1;
                }
                let span: f32 = infos[i..=j].iter().map(|c| c.w).sum();
                let need = text_width_mm(&g, HEADER_PT, FontKind::Bold) + 2.0 * PAD_MM;
                if need > span {
                    let add = (need - span) / (j - i + 1) as f32;
                    for c in infos[i..=j].iter_mut() {
                        c.w += add;
                    }
                }
                i = j + 1;
            } else {
                i += 1;
            }
        }
    }

    // the row labels: the leading text columns, as long as they take at most 40% of the page, and the first at least
    let mut n_label = 0;
    let mut label_w = 0.0;
    for c in &infos {
        if c.numeric || (n_label > 0 && label_w + c.w > usable_w * LABEL_SHARE) {
            break;
        }
        label_w += c.w;
        n_label += 1;
    }
    n_label = n_label.max(1).min(infos.len().max(1));
    let label_w: f32 = infos.iter().take(n_label).map(|c| c.w).sum();

    // the parts: the labels and as many of the other columns as fit; a group is kept together when it can be
    let data: Vec<usize> = (n_label..infos.len()).collect();
    let mut groups_of_cols: Vec<Vec<usize>> = Vec::new();
    for &i in &data {
        match (infos[i].group.as_deref(), groups_of_cols.last().and_then(|g| g.first()).and_then(|&f| infos[f].group.as_deref())) {
            (Some(a), Some(b)) if a == b => {
                if let Some(g) = groups_of_cols.last_mut() {
                    g.push(i);
                }
            }
            _ => groups_of_cols.push(vec![i]),
        }
    }
    let capacity = (usable_w - label_w).max(MIN_COL_MM);
    let mut part_cols: Vec<Vec<usize>> = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    let mut used = 0.0;
    for g in groups_of_cols {
        let gw: f32 = g.iter().map(|&i| infos[i].w).sum();
        if gw <= capacity {
            if used + gw > capacity && !cur.is_empty() {
                part_cols.push(std::mem::take(&mut cur));
                used = 0.0;
            }
            used += gw;
            cur.extend(g);
        } else {
            // a group wider than a page is split column by column
            for i in g {
                if used + infos[i].w > capacity && !cur.is_empty() {
                    part_cols.push(std::mem::take(&mut cur));
                    used = 0.0;
                }
                used += infos[i].w;
                cur.push(i);
            }
        }
    }
    if !cur.is_empty() || part_cols.is_empty() {
        part_cols.push(cur);
    }

    let parts = part_cols
        .into_iter()
        .map(|data_cols| {
            let idx: Vec<usize> = (0..n_label).chain(data_cols.iter().copied()).collect();
            // columns with their x
            let mut columns: Vec<PdfCol> = Vec::new();
            let mut x = LEFT_MM;
            for &i in &idx {
                let c = &infos[i];
                columns.push(PdfCol { name: c.name.clone(), align: if c.numeric { PdfAlign::Right } else { PdfAlign::Left }, x, w: c.w });
                x += c.w;
            }
            // header cells
            let header: Vec<PdfHeaderCell> = idx
                .iter()
                .enumerate()
                .map(|(k, &i)| {
                    let col = &columns[k];
                    let lines = infos[i]
                        .lines
                        .iter()
                        .map(|t| {
                            let w = text_width_mm(t, HEADER_PT, FontKind::Bold);
                            PdfText { text: t.clone(), x: col.text_x(w), w }
                        })
                        .collect();
                    PdfHeaderCell { lines, col_x: col.x, col_w: col.w, align: col.align, first_col: k, span: 1 }
                })
                .collect();
            // the group level
            let group_header = if two_level && idx.iter().any(|&i| infos[i].group.is_some()) {
                let mut cells: Vec<PdfHeaderCell> = Vec::new();
                let mut k = 0;
                while k < idx.len() {
                    let g = infos[idx[k]].group.clone();
                    let mut e = k;
                    while e + 1 < idx.len() && infos[idx[e + 1]].group == g && g.is_some() {
                        e += 1;
                    }
                    if let Some(label) = g {
                        let col_x = columns[k].x;
                        let col_w: f32 = columns[k..=e].iter().map(|c| c.w).sum();
                        let w = text_width_mm(&label, HEADER_PT, FontKind::Bold);
                        cells.push(PdfHeaderCell {
                            lines: vec![PdfText { text: label, x: col_x + (col_w - w) / 2.0, w }],
                            col_x,
                            col_w,
                            align: PdfAlign::Center,
                            first_col: k,
                            span: e - k + 1,
                        });
                    }
                    k = e + 1;
                }
                Some(cells)
            } else {
                None
            };
            // body rows
            let rows: Vec<PdfRow> = (0..n_rows)
                .map(|r| {
                    let total = total_rows.get(r).copied().unwrap_or(false);
                    let cells = idx
                        .iter()
                        .enumerate()
                        .map(|(k, &i)| {
                            let col = &columns[k];
                            let text = infos[i].cells[r].clone();
                            let w = text_width_mm(&text, CELL_PT, if total { FontKind::Bold } else { FontKind::Regular });
                            PdfCell { text: PdfText { x: col.text_x(w), w, text }, col_x: col.x, col_w: col.w, align: col.align }
                        })
                        .collect();
                    PdfRow { cells, total }
                })
                .collect();
            // the summary band
            let summary_cells: Vec<Option<PdfSummaryCell>> = if summary.is_empty() {
                Vec::new()
            } else {
                idx.iter()
                    .enumerate()
                    .map(|(k, &i)| {
                        let cell = summary.iter().find(|c| c.column == infos[i].name && c.agg != crate::models::SummaryAgg::None)?;
                        let col = &columns[k];
                        let label_text = match (&cell.agg, &cell.weight_col) {
                            (crate::models::SummaryAgg::WeightedAvg, Some(w)) => format!("{} by {}", cell.agg.label(), w),
                            (agg, _) => agg.label().to_string(),
                        };
                        let label_text = fit_text(&label_text, col.w - 2.0 * PAD_MM, 6.5, FontKind::Bold);
                        let value = pdf_summary_text(cell, col_formats.get(infos[i].name.as_str()), num_format, global_decimals);
                        let vw = text_width_mm(&value, CELL_PT, FontKind::Bold);
                        let lw = text_width_mm(&label_text, 6.5, FontKind::Bold);
                        Some(PdfSummaryCell {
                            label: PdfText { text: label_text, x: col.x + PAD_MM, w: lw },
                            value: PdfCell { text: PdfText { x: col.text_x(vw), w: vw, text: value }, col_x: col.x, col_w: col.w, align: col.align },
                        })
                    })
                    .collect()
            };
            PdfPart {
                first_data: data_cols.first().map(|&i| infos[i].name.clone()).unwrap_or_default(),
                last_data: data_cols.last().map(|&i| infos[i].name.clone()).unwrap_or_default(),
                columns,
                group_header,
                header,
                rows,
                summary: summary_cells,
            }
        })
        .collect();
    PdfTabLayout { title: title.to_string(), unit_line, parts }
}

/// A summary value as the PDF shows it: like a table value, without a unit.
pub fn pdf_summary_text(cell: &SummaryCell, spec: Option<&ColumnFormatSpec>, num_format: NumberDisplayFormat, decimals: usize) -> String {
    match &cell.value {
        None => "\u{2014}".to_string(),
        Some(SummaryValue::Count(n)) => format_with_commas(*n as f64, 0),
        Some(SummaryValue::Number(x)) => pdf_number_text(*x, &cell.column, spec, num_format, decimals),
        Some(SummaryValue::Date(d)) => date_text(*d),
        Some(SummaryValue::Text(t)) => t.clone(),
    }
}
