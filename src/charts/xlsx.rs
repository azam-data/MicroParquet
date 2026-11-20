//! Renderer C of the chart studio (Phase 7 C4): native, editable Excel charts. The chart's numbers
//! (`ChartData`) are written to a sheet and a chart that reads them is placed beside the table, so the
//! reader can change either in Excel. The look comes from the same tokens as the other renderers:
//! the palette, Segoe UI, no chart border and light, solid gridlines.
//!
//! What is written: column and bar charts (side by side, stacked, 100% stacked), line, area (and
//! stacked), scatter and bubble (a scatter: Excel's bubble is not available), histogram (a column chart),
//! box plot (stacked columns with an invisible base and error-bar whiskers), waterfall (a stacked
//! column with an invisible base series), doughnut, linked panels and small multiples (one chart per
//! panel, stacked) and the heatmap (the data range under a 3-colour conditional format). Sankey and
//! treemap are not written as charts; their data is, with a note on the sheet.

use super::axis::{decimals_for_step, nice_ticks, ValueKind};
use super::colours::ColourMap;
use super::data::{heatmap, waterfall_from_data, waterfall_stack, ChartData, StepKind};
use super::layout::{chart_title, scaled, value_kind, LayoutOpts, DONUT_REFUSAL};
use super::spec::{ChartKind, ChartSpec, Stack, Transform};
use super::tokens as t;
use anyhow::{bail, Result};
use rust_xlsxwriter::{
    Chart, ChartAxisLabelPosition, ChartDataLabel, ChartAxisTickType, ChartErrorBars, ChartErrorBarsType, ChartFont, ChartFormat, ChartLegendPosition, ChartLine, ChartMarker, ChartMarkerType, ChartPoint, ChartRange, ChartSolidFill, ChartType,
    Color, ConditionalFormat3ColorScale, ConditionalFormatCell, ConditionalFormatCellRule, Format, Workbook, Worksheet,
};
use std::path::Path;

/// The note written on a sheet whose chart only the HTML page can draw.
pub const NOTE_HTML_ONLY: &str = "Sankey and treemap charts are not written to Excel. Their data is in the table on this sheet; use the chart's HTML page for the chart.";
pub const NOTE_STEPPED: &str = "Excel has no stepped line, so the chart is an ordinary line through the same points.";
pub const NOTE_BUBBLE: &str = "Excel has no bubble chart here, so the chart is a scatter; the bubble size is in the Size column.";

/// What `add_chart_sheet` wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetInfo {
    pub sheet: String,
    /// Native charts on the sheet.
    pub charts: usize,
    /// The note written under the table, when there is one.
    pub note: Option<String>,
}

fn rgb(h: u32) -> Color {
    Color::RGB(h & 0xFF_FFFF)
}

fn font(size: f64, bold: bool, colour: u32) -> ChartFont {
    let mut f = ChartFont::new();
    f.set_name(t::FONT_EXCEL).set_size(size).set_color(rgb(colour));
    if bold {
        f.set_bold();
    }
    f
}

fn fill(c: u32) -> ChartFormat {
    let mut f = ChartFormat::new();
    f.set_solid_fill(&ChartSolidFill::new().set_color(rgb(c)));
    f.set_no_border();
    f
}

fn no_fill() -> ChartFormat {
    let mut f = ChartFormat::new();
    f.set_no_fill().set_no_border();
    f
}

fn line_fmt(c: u32, w: f64) -> ChartFormat {
    let mut f = ChartFormat::new();
    f.set_line(&ChartLine::new().set_color(rgb(c)).set_width(w));
    f
}

/// A sheet name Excel accepts: at most 31 characters, none of `[]:*?/\`.
pub fn safe_sheet_name(name: &str) -> String {
    let s: String = name.chars().map(|c| if "[]:*?/\\".contains(c) { ' ' } else { c }).collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let s: String = s.chars().take(31).collect();
    let s = s.trim().trim_matches('\'').to_string();
    if s.is_empty() {
        "Chart".to_string()
    } else {
        s
    }
}

fn x(e: rust_xlsxwriter::XlsxError) -> anyhow::Error {
    anyhow::anyhow!("Excel could not write the chart: {}", e)
}

struct Cx<'a> {
    data: &'a ChartData,
    opts: &'a LayoutOpts,
    ink: t::ChartInk,
    cm: ColourMap,
    sheet: String,
    num: Format,
    num_pct: Format,
    int: Format,
    head: Format,
    text: Format,
}

impl<'a> Cx<'a> {
    fn colour(&self, name: &str, i: usize, n: usize) -> u32 {
        let slot = if name.is_empty() && n == 1 {
            Some(0)
        } else if name.is_empty() {
            Some(i.min(t::SLOTS - 1))
        } else {
            None
        };
        let base = match slot {
            Some(s) => t::series_colour(s, false).unwrap_or(0),
            None => match self.cm.get(name) {
                Some(c) => c.hex(false),
                None => t::series_colour(i.min(t::SLOTS - 1), false).unwrap_or(0),
            },
        };
        match &self.data.spec.highlight {
            Some(h) if !h.is_empty() && n > 1 && name != h => self.cm.dimmed(name, false),
            _ => base,
        }
    }

    fn num_fmt(&self, vk: ValueKind) -> &Format {
        match vk {
            ValueKind::Percent => &self.num_pct,
            ValueKind::Count => &self.int,
            _ => &self.num,
        }
    }

    fn axis_num_format(vk: ValueKind) -> String {
        match vk {
            ValueKind::Percent => num_code(0, true, false),
            ValueKind::Count => num_code(0, false, true),
            _ => num_code(0, false, true),
        }
    }

    /// The shared look of every chart: Segoe UI, the title, no border, light solid gridlines.
    fn style(&self, chart: &mut Chart, title: &str, vk: ValueKind, horizontal: bool, legend: bool) {
        let ink = self.ink;
        chart.title().set_name(&t::tidy(title)).set_font(&font(t::TITLE_PT as f64 - 1.0, true, ink.ink));
        chart.chart_area().set_format(ChartFormat::new().set_no_border().set_solid_fill(&ChartSolidFill::new().set_color(rgb(ink.surface))));
        chart.plot_area().set_format(ChartFormat::new().set_no_border().set_no_fill());
        let tick_font = font(t::TICK_PT as f64, false, ink.ink2);
        let grid = ChartLine::new().set_color(rgb(ink.grid)).set_width(0.75).to_owned();
        // category axis: the baseline only
        {
            let ax = if horizontal { chart.y_axis() } else { chart.x_axis() };
            ax.set_font(&tick_font).set_major_gridlines(false).set_major_tick_type(ChartAxisTickType::None).set_label_position(ChartAxisLabelPosition::Low);
            ax.set_format(&mut line_fmt(ink.baseline, 1.0));
            if horizontal {
                ax.set_reverse();
            }
        }
        // value axis: light, solid gridlines, no axis line
        {
            let ax = if horizontal { chart.x_axis() } else { chart.y_axis() };
            ax.set_font(&font(t::TICK_PT as f64, false, ink.muted)).set_major_gridlines(true).set_major_gridlines_line(&grid).set_major_tick_type(ChartAxisTickType::None);
            ax.set_num_format(&Self::axis_num_format(vk));
            ax.set_format(ChartFormat::new().set_no_line());
        }
        if legend {
            chart.legend().set_position(ChartLegendPosition::Top).set_font(&font(t::LABEL_PT as f64, false, ink.ink2));
        } else {
            chart.legend().set_hidden();
        }
    }

    fn write_title_block(&self, ws: &mut Worksheet, title: &str) -> Result<u32> {
        let sub = subtitle_of(self.data, self.opts);
        let mut row = 0u32;
        ws.write_string_with_format(row, 0, t::tidy(title), &Format::new().set_bold().set_font_name(t::FONT_EXCEL).set_font_size(t::TITLE_PT as f64)).map_err(x)?;
        row += 1;
        if !sub.is_empty() {
            ws.write_string_with_format(row, 0, t::tidy(&sub), &Format::new().set_font_name(t::FONT_EXCEL).set_font_size(t::SUBTITLE_PT as f64).set_font_color(rgb(self.ink.ink2))).map_err(x)?;
            row += 1;
        }
        let mut foot: Vec<String> = Vec::new();
        if !self.data.spec.footnote.trim().is_empty() {
            foot.push(self.data.spec.footnote.trim().to_string());
        }
        foot.extend(self.data.notes.iter().cloned());
        if !foot.is_empty() {
            ws.write_string_with_format(row, 0, t::tidy(&foot.join(" \u{b7} ")), &Format::new().set_font_name(t::FONT_EXCEL).set_font_size(t::FOOTNOTE_PT as f64).set_font_color(rgb(self.ink.muted))).map_err(x)?;
            row += 1;
        }
        Ok(row + 1)
    }
}

fn subtitle_of(data: &ChartData, opts: &LayoutOpts) -> String {
    if !data.spec.subtitle.trim().is_empty() {
        return data.spec.subtitle.trim().to_string();
    }
    let names = data.measure_names.join(", ");
    match (names.is_empty(), opts.unit.is_empty()) {
        (false, false) => format!("{} \u{b7} {}", names, opts.unit),
        (false, true) => names,
        (true, false) => opts.unit.clone(),
        (true, true) => String::new(),
    }
}

fn range(sheet: &str, r1: u32, c1: u16, r2: u32, c2: u16) -> ChartRange {
    ChartRange::new_from_range(sheet, r1, c1, r2, c2)
}

/// Writes `data` to a new sheet of `wb`, with the chart(s) beside it.
pub fn add_chart_sheet(wb: &mut Workbook, sheet_name: &str, data: &ChartData, opts: &LayoutOpts) -> Result<SheetInfo> {
    let spec = &data.spec;
    if let Some(r) = data.empty_reason() {
        bail!("No data to chart: {}.", r);
    }
    let name = safe_sheet_name(sheet_name);
    let ink = t::ink(false);
    let mut cm = opts.colours.clone().unwrap_or_else(|| spec.colour_map.clone());
    let named: Vec<String> = data.series.iter().filter(|s| !s.is_empty()).cloned().collect();
    cm.assign_all(&named);
    let cx = Cx {
        data,
        opts,
        ink,
        cm,
        sheet: name.clone(),
        num: Format::new().set_num_format(num_code(opts.decimals, false, true)).set_font_name(t::FONT_EXCEL),
        num_pct: Format::new().set_num_format(num_code(1, true, false)).set_font_name(t::FONT_EXCEL),
        int: Format::new().set_num_format(num_code(0, false, true)).set_font_name(t::FONT_EXCEL),
        head: Format::new().set_bold().set_font_name(t::FONT_EXCEL).set_border_bottom(rust_xlsxwriter::FormatBorder::Thin),
        text: Format::new().set_font_name(t::FONT_EXCEL),
    };
    let mut ws = Worksheet::new();
    ws.set_name(&name).map_err(x)?;
    // printed, the table and its charts are one landscape page
    ws.set_landscape().set_print_fit_to_pages(1, 1);
    let title = chart_title(data);
    let top = cx.write_title_block(&mut ws, &title)?;
    let (charts, note) = match spec.kind {
        ChartKind::Sankey | ChartKind::Treemap => {
            let end = write_grid(&cx, &mut ws, top, 0)?;
            ws.write_string_with_format(end.next_row + 1, 0, NOTE_HTML_ONLY, &cx.text).map_err(x)?;
            (0, Some(NOTE_HTML_ONLY.to_string()))
        }
        ChartKind::Heatmap => {
            heat_sheet(&cx, &mut ws, top)?;
            (0, None)
        }
        ChartKind::Waterfall => (waterfall_sheet(&cx, &mut ws, top, &title)?, None),
        ChartKind::Scatter | ChartKind::Bubble => scatter_sheet(&cx, &mut ws, top, &title)?,
        ChartKind::Histogram => (histogram_sheet(&cx, &mut ws, top, &title)?, None),
        ChartKind::BoxPlot => (box_sheet(&cx, &mut ws, top, &title)?, None),
        ChartKind::Donut => (donut_sheet(&cx, &mut ws, top, &title)?, None),
        ChartKind::Line if spec.stepped && data.facets.is_empty() => {
            let n = grid_sheet(&cx, &mut ws, top, &title)?;
            ws.write_string_with_format(top + data.categories.len() as u32 + 2, 0, NOTE_STEPPED, &cx.text).map_err(x)?;
            (n, Some(NOTE_STEPPED.to_string()))
        }
        _ => (grid_sheet(&cx, &mut ws, top, &title)?, None),
    };
    wb.push_worksheet(ws);
    Ok(SheetInfo { sheet: name, charts, note })
}

/// A workbook with the one chart sheet.
pub fn write_chart_xlsx(path: &Path, data: &ChartData, opts: &LayoutOpts) -> Result<SheetInfo> {
    let mut wb = Workbook::new();
    let info = add_chart_sheet(&mut wb, "Chart", data, opts)?;
    wb.save(path).map_err(|e| anyhow::anyhow!("Could not write {}: {}. Choose another folder, or close the file if it is open in Excel.", path.display(), e))?;
    Ok(info)
}

// ------------------------------------------------------------------------------------- the table

/// Where the table ended.
struct GridEnd {
    next_row: u32,
    next_col: u16,
}

/// The cartesian table: a column of categories and one column per (measure, series). Returns the layout
/// of the columns so the charts can point at them.
struct GridCols {
    header_row: u32,
    first_row: u32,
    last_row: u32,
    /// (measure, series, column)
    cols: Vec<(usize, usize, u16)>,
}

fn write_grid_table(cx: &Cx, ws: &mut Worksheet, top: u32, d: &ChartData, label: &str) -> Result<(GridCols, GridEnd)> {
    let spec = &d.spec;
    let mut cols = Vec::new();
    let header = top;
    let cat_head = if spec.category.is_empty() { "Category".to_string() } else { spec.category.clone() };
    ws.write_string_with_format(header, 0, t::tidy(&cat_head), &cx.head).map_err(x)?;
    let mut c: u16 = 1;
    for m in 0..d.measure_count() {
        for s in 0..d.series.len().max(1) {
            let ser = d.series.get(s).cloned().unwrap_or_default();
            let mname = d.measure_names.get(m).cloned().unwrap_or_default();
            let head = match (ser.is_empty(), d.measure_count() > 1, label.is_empty()) {
                (true, _, _) => mname.clone(),
                (false, true, _) => format!("{} \u{b7} {}", ser, mname),
                (false, false, _) => ser.clone(),
            };
            ws.write_string_with_format(header, c, t::tidy(&head), &cx.head).map_err(x)?;
            cols.push((m, s, c));
            c += 1;
        }
    }
    ws.write_string_with_format(header, c, "Rows", &cx.head).map_err(x)?;
    let first = header + 1;
    for (i, cat) in d.categories.iter().enumerate() {
        let r = first + i as u32;
        ws.write_string_with_format(r, 0, t::tidy(cat), &cx.text).map_err(x)?;
        for (m, s, col) in &cols {
            let vk = value_kind(spec, *m);
            let v = d.measure(*m).and_then(|md| md.values.get(*s).and_then(|row| row.get(i)).copied().flatten());
            if let Some(v) = v {
                ws.write_number_with_format(r, *col, scaled(v, vk, cx.opts), cx.num_fmt(vk)).map_err(x)?;
            }
        }
        let rows: u64 = d.rows.iter().map(|rw| rw.get(i).copied().unwrap_or(0)).sum();
        ws.write_number_with_format(r, c, rows as f64, &cx.int).map_err(x)?;
    }
    let last = first + d.categories.len().max(1) as u32 - 1;
    ws.set_column_width(0, 24).map_err(x)?;
    for k in 1..=c {
        ws.set_column_width(k, 16).map_err(x)?;
    }
    Ok((GridCols { header_row: header, first_row: first, last_row: last, cols }, GridEnd { next_row: last + 1, next_col: c + 1 }))
}

fn write_grid(cx: &Cx, ws: &mut Worksheet, top: u32, _unused: u32) -> Result<GridEnd> {
    write_grid_table(cx, ws, top, cx.data, "").map(|x| x.1)
}

// ------------------------------------------------------------------------------- cartesian charts

fn chart_type_for(kind: ChartKind, stack: Stack, horizontal: bool, many: bool) -> ChartType {
    let stacked = many && matches!(stack, Stack::Stacked);
    let pct = many && matches!(stack, Stack::Percent);
    match kind {
        ChartKind::Line => ChartType::Line,
        ChartKind::Area => match (stacked, pct) {
            (_, true) => ChartType::AreaPercentStacked,
            (true, _) => ChartType::AreaStacked,
            _ => ChartType::Area,
        },
        _ => match (horizontal, stacked, pct) {
            (false, _, true) => ChartType::ColumnPercentStacked,
            (false, true, _) => ChartType::ColumnStacked,
            (false, _, _) => ChartType::Column,
            (true, _, true) => ChartType::BarPercentStacked,
            (true, true, _) => ChartType::BarStacked,
            (true, _, _) => ChartType::Bar,
        },
    }
}

/// An Excel number format with the true minus sign for negatives: `#,##0.00;"−"#,##0.00`.
/// `commas` writes thousands separators; `pct` appends a literal "%" (the value is already a percentage).
fn num_code(decimals: usize, pct: bool, commas: bool) -> String {
    let body = format!("{}{}", if commas { "#,##0" } else { "0" }, if decimals > 0 { format!(".{}", "0".repeat(decimals)) } else { String::new() });
    let tail = if pct { "\"%\"" } else { "" };
    format!("{body}{tail};\"\u{2212}\"{body}{tail}")
}

/// A diverging bar's data-label format: always signed ("+4.82%", "−10.50%").
fn signed_code(decimals: usize, pct: bool) -> String {
    let body = format!("#,##0{}", if decimals > 0 { format!(".{}", "0".repeat(decimals)) } else { String::new() });
    let tail = if pct { "\"%\"" } else { "" };
    format!("\"+\"{body}{tail};\"\u{2212}\"{body}{tail};{body}{tail}")
}

fn series_format(kind: ChartKind, colour: u32, surface: u32) -> (ChartFormat, Option<ChartMarker>) {
    match kind {
        ChartKind::Line => {
            let mut m = ChartMarker::new();
            m.set_type(ChartMarkerType::Circle).set_size(t::MARKER_PX as u8);
            let mut mf = ChartFormat::new();
            mf.set_solid_fill(&ChartSolidFill::new().set_color(rgb(colour))).set_border(&ChartLine::new().set_color(rgb(surface)).set_width(t::MARKER_RING_PX as f64));
            m.set_format(&mut mf);
            (line_fmt(colour, t::LINE_PX as f64), Some(m))
        }
        ChartKind::Area => {
            let mut f = ChartFormat::new();
            f.set_solid_fill(&ChartSolidFill::new().set_color(rgb(colour)).set_transparency(90)).set_line(&ChartLine::new().set_color(rgb(colour)).set_width(t::LINE_PX as f64));
            (f, None)
        }
        _ => (fill(colour), None),
    }
}

/// One chart over the columns of one measure (a panel).
#[allow(clippy::too_many_arguments)]
fn cartesian_chart(cx: &Cx, g: &GridCols, kind: ChartKind, measure: usize, title: &str, d: &ChartData, single: bool) -> Result<Chart> {
    let spec = &d.spec;
    let sheet = &cx.sheet;
    let horizontal = spec.horizontal && kind == ChartKind::Bar;
    let n_series = d.series.len().max(1);
    let ct = chart_type_for(kind, spec.stack, horizontal, n_series > 1);
    let mut chart = Chart::new(ct);
    let vk = value_kind(spec, measure);
    cx.style(&mut chart, title, vk, horizontal, n_series > 1);
    let stacked_area = kind == ChartKind::Area && n_series > 1 && matches!(spec.stack, Stack::Stacked | Stack::Percent);
    // a `single` panel (a ratio that is the same in every series) is drawn once, named after the measure, in the first colour
    for (m, s, col) in g.cols.iter().filter(|(m, _, _)| *m == measure).take(if single { 1 } else { usize::MAX }) {
        let _ = m;
        let name = if single { d.measure_names.get(measure).cloned().unwrap_or_default() } else { d.series.get(*s).cloned().unwrap_or_default() };
        let colour = if single { t::series_colour(0, false).unwrap_or(0) } else { cx.colour(&name, *s, n_series) };
        let (mut fmt, marker) = series_format(kind, colour, cx.ink.surface);
        if stacked_area {
            fmt = ChartFormat::new();
            fmt.set_solid_fill(&ChartSolidFill::new().set_color(rgb(colour)).set_transparency(65)).set_line(&ChartLine::new().set_color(rgb(colour)).set_width(t::LINE_PX as f64));
        }
        let series = chart.add_series();
        series.set_categories(&range(sheet, g.first_row, 0, g.last_row, 0)).set_values(&range(sheet, g.first_row, *col, g.last_row, *col));
        let series_name = if name.is_empty() { d.measure_names.get(measure).cloned().unwrap_or_default() } else { name };
        series.set_name(&t::tidy(&series_name)).set_format(&mut fmt);
        let older_curve = spec.highlight.as_ref().map_or(false, |h| !h.is_empty() && n_series > 1 && *h != d.series.get(*s).cloned().unwrap_or_default());
        if let Some(mk) = marker {
            if older_curve {
                // only the highlighted curve carries markers
                let mut none = ChartMarker::new();
                none.set_none();
                series.set_marker(&none);
            } else {
                series.set_marker(&mk);
            }
        }
        if kind == ChartKind::Bar && n_series == 1 && !spec.bar_status.is_empty() {
            // a status bar is filled in its status colour (the words are in the "Status" column)
            let points: Vec<ChartPoint> = d
                .categories
                .iter()
                .map(|cat| {
                    match spec.bar_status.iter().find(|b| b.category == *cat) {
                        Some(st) => ChartPoint::new().set_format(&mut fill(st.status.hex())),
                        None => ChartPoint::new(),
                    }
                })
                .collect();
            series.set_points(&points);
        }
        if spec.segment_labels && kind == ChartKind::Bar && spec.stack == Stack::Stacked && n_series > 1 && !horizontal {
            // each segment says its value and its share of the stack (Excel puts it in the segment)
            let labels: Vec<ChartDataLabel> = (0..d.categories.len())
                .map(|i| {
                    let val = |si: usize| d.measure(measure).and_then(|md| md.values.get(si).and_then(|r| r.get(i)).copied().flatten()).unwrap_or(0.0);
                    let total: f64 = (0..n_series).map(|si| val(si).abs()).sum();
                    let x = scaled(val(*s), vk, cx.opts);
                    let body = if (x - x.round()).abs() < 1e-9 { super::axis::value_text(x, vk, 0, false) } else { super::axis::value_text(x, vk, cx.opts.decimals, false) };
                    let text = if total == 0.0 { body } else { format!("{} \u{b7} {}%", body, crate::models::format_with_commas(val(*s).abs() / total * 100.0, 1)) };
                    ChartDataLabel::new().set_value(text.as_str()).to_custom()
                })
                .collect();
            series.set_custom_data_labels(&labels);
        }
        if let Some(dl) = diverging_labels(cx, d, measure, kind) {
            series.set_data_label(&dl);
        }
        if matches!(ct, ChartType::Column | ChartType::Bar) {
            series.set_gap(80);
        }
        if matches!(ct, ChartType::ColumnStacked | ChartType::BarStacked | ChartType::ColumnPercentStacked | ChartType::BarPercentStacked) {
            series.set_gap(60);
        }
    }
    chart.set_width(720).set_height(360);
    Ok(chart)
}

/// The value axis the app draws: the same round ticks over the same range (a line is not forced to
/// start at zero; a bar or area is), written as the chart's fixed minimum, maximum and step.
fn set_value_axis(cx: &Cx, chart: &mut Chart, g: &GridCols, kind: ChartKind, measure: usize, d: &ChartData, extras: &[f64]) {
    let spec = &d.spec;
    let vk = value_kind(spec, measure);
    let stacked = matches!(kind, ChartKind::Bar | ChartKind::Area) && d.series.len() > 1 && matches!(spec.stack, Stack::Stacked | Stack::Percent);
    let horizontal = spec.horizontal && kind == ChartKind::Bar;
    let cols: Vec<usize> = g.cols.iter().filter(|(m, _, _)| *m == measure).map(|(_, s, _)| *s).collect();
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    if stacked && spec.stack == Stack::Percent {
        lo = 0.0;
        hi = 100.0;
    } else if stacked {
        for i in 0..d.categories.len() {
            let (mut pos, mut neg) = (0.0, 0.0);
            for s in &cols {
                if let Some(v) = d.measure(measure).and_then(|md| md.values.get(*s).and_then(|r| r.get(i)).copied().flatten()) {
                    let v = scaled(v, vk, cx.opts);
                    if v >= 0.0 { pos += v } else { neg += v }
                }
            }
            hi = hi.max(pos);
            lo = lo.min(neg);
        }
    } else {
        for s in &cols {
            for i in 0..d.categories.len() {
                if let Some(v) = d.measure(measure).and_then(|md| md.values.get(*s).and_then(|r| r.get(i)).copied().flatten()) {
                    let v = scaled(v, vk, cx.opts);
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
            }
        }
    }
    for e in extras {
        let e = scaled(*e, vk, cx.opts);
        lo = lo.min(e);
        hi = hi.max(e);
    }
    if !lo.is_finite() || !hi.is_finite() {
        return;
    }
    if matches!(kind, ChartKind::Bar | ChartKind::Area | ChartKind::Histogram) {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
    }
    if spec.pad_axis_below && kind == ChartKind::Line {
        lo = super::axis::padded_floor(lo, hi, 6);
    }
    let (ticks, step) = nice_ticks(lo, hi, 6);
    let (Some(first), Some(last)) = (ticks.first(), ticks.last()) else { return };
    let ax = if horizontal { chart.x_axis() } else { chart.y_axis() };
    ax.set_min(*first).set_max(*last).set_major_unit(step);
    ax.set_num_format(&num_code(decimals_for_step(step), vk == ValueKind::Percent, true));
}

/// Data labels on a diverging bar (one series with negative values, 12 bars or fewer): always signed.
fn diverging_labels(cx: &Cx, d: &ChartData, measure: usize, kind: ChartKind) -> Option<ChartDataLabel> {
    let spec = &d.spec;
    if kind != ChartKind::Bar || d.series.len() > 1 {
        return None;
    }
    let has_neg = d.measure(measure).map_or(false, |md| md.values.iter().flatten().flatten().any(|v| *v < 0.0));
    if !has_neg || d.categories.len() > 12 {
        return None;
    }
    let vk = value_kind(spec, measure);
    let code = signed_code(if vk == ValueKind::Percent { 2 } else { cx.opts.decimals }, vk == ValueKind::Percent);
    let mut label = ChartDataLabel::new();
    label.show_value().set_num_format(&code).set_font(&font(t::DATA_LABEL_PT as f64, false, cx.ink.ink));
    Some(label)
}

fn grid_sheet(cx: &Cx, ws: &mut Worksheet, top: u32, title: &str) -> Result<usize> {
    let d = cx.data;
    let spec = &d.spec;
    let mut charts = 0;
    if !d.facets.is_empty() {
        // small multiples: the table and the chart of each facet, one under the other
        let mut row = top;
        for f in &d.facets {
            ws.write_string_with_format(row, 0, t::tidy(&f.name), &cx.head).map_err(x)?;
            let (g, end) = write_grid_table(cx, ws, row + 1, &f.data, "facet")?;
            let mut chart = cartesian_chart(cx, &g, spec.kind, 0, &format!("{}: {}", title, f.name), &f.data, false)?;
            set_value_axis(cx, &mut chart, &g, spec.kind, 0, &f.data, &[]);
            ws.insert_chart(row, end.next_col + 1, &chart).map_err(x)?;
            let _ = &mut chart;
            charts += 1;
            row = end.next_row.max(row + 19) + 2;
        }
        return Ok(charts);
    }
    let (g, end) = write_grid_table(cx, ws, top, d, "")?;
    // the panels: the spec's linked panels, else one per measure, else one
    let panels: Vec<(ChartKind, usize, String, bool)> = if !spec.panels.is_empty() {
        spec.panels
            .iter()
            .take(t::MAX_PANELS)
            .map(|p| {
                let mi = p.measures.first().copied().unwrap_or(0).min(d.measure_count().saturating_sub(1));
                (p.kind, mi, if p.title.is_empty() { d.measure_names.get(mi).cloned().unwrap_or_default() } else { p.title.clone() }, p.single)
            })
            .collect()
    } else if d.measure_count() > 1 {
        (0..d.measure_count().min(t::MAX_PANELS)).map(|mi| (spec.kind, mi, d.measure_names.get(mi).cloned().unwrap_or_default(), false)).collect()
    } else {
        vec![(spec.kind, 0, String::new(), false)]
    };
    let linked = panels.len() > 1;
    let mut row = top;
    let mut panel_no = 0usize;
    for (kind, mi, ptitle, single) in &panels {
        let heading = if linked { format!("{}: {}", title, ptitle) } else { title.to_string() };
        let mut chart = cartesian_chart(cx, &g, *kind, *mi, &heading, d, *single)?;
        add_reference_lines(cx, ws, &g, &end, &mut chart, *mi, *kind)?;
        let free = end.next_col + spec.ref_lines.len() as u16;
        let free = add_limit_ticks(cx, ws, &g, &end, &mut chart, panel_no, *mi, *kind, free)?;
        add_bar_status(cx, ws, &g, panel_no, *kind, free)?;
        add_net_marker(cx, ws, &g, &mut chart, *mi, *kind, free)?;
        let mut extras: Vec<f64> = spec.ref_lines.iter().filter(|r| r.panel == (*mi).min(spec.measures.len().saturating_sub(1)).min(*mi)).map(|r| r.value).collect();
        extras.extend(spec.limit_ticks.iter().filter(|l| l.panel == panel_no).map(|l| l.value));
        set_value_axis(cx, &mut chart, &g, *kind, *mi, d, &extras);
        panel_no += 1;
        ws.insert_chart(row, end.next_col + 1, &chart).map_err(x)?;
        charts += 1;
        row += 19;
    }
    Ok(charts)
}

/// Reference lines as a helper column and a line series on the same chart (1.5 px, solid, status colour).
fn add_reference_lines(cx: &Cx, ws: &mut Worksheet, g: &GridCols, end: &GridEnd, chart: &mut Chart, measure: usize, kind: ChartKind) -> Result<()> {
    let spec = &cx.data.spec;
    let lines: Vec<_> = spec.ref_lines.iter().filter(|r| r.panel == measure.min(spec.measures.len().saturating_sub(1)).min(measure)).collect();
    if lines.is_empty() || !matches!(kind, ChartKind::Bar | ChartKind::Line | ChartKind::Area) || (spec.horizontal && kind == ChartKind::Bar) {
        return Ok(());
    }
    let vk = value_kind(spec, measure);
    let mut line_chart = Chart::new(ChartType::Line);
    for (k, r) in lines.iter().enumerate() {
        let col = end.next_col + k as u16;
        let label = if r.label.is_empty() { r.status.label().to_string() } else { r.label.clone() };
        ws.write_string_with_format(g.header_row, col, t::tidy(&label), &cx.head).map_err(x)?;
        for i in g.first_row..=g.last_row {
            ws.write_number_with_format(i, col, scaled(r.value, vk, cx.opts), cx.num_fmt(vk)).map_err(x)?;
        }
        let s = line_chart.add_series();
        s.set_categories(&range(&cx.sheet, g.first_row, 0, g.last_row, 0)).set_values(&range(&cx.sheet, g.first_row, col, g.last_row, col)).set_name(&t::tidy(&label));
        s.set_format(&mut line_fmt(r.status.hex(), t::REFERENCE_LINE_PX as f64));
        let mut mk = ChartMarker::new();
        mk.set_none();
        s.set_marker(&mk);
    }
    chart.combine(&line_chart);
    Ok(())
}

/// The net of each stack of a stacked column chart (`net_marker`): a "Net" column and a dark dash marker on the chart with
/// a signed data label; the stacks themselves carry no labels.
fn add_net_marker(cx: &Cx, ws: &mut Worksheet, g: &GridCols, chart: &mut Chart, measure: usize, kind: ChartKind, col: u16) -> Result<()> {
    let d = cx.data;
    let spec = &d.spec;
    if !spec.net_marker || kind != ChartKind::Bar || spec.stack != Stack::Stacked || d.series.len() < 2 || spec.horizontal {
        return Ok(());
    }
    let vk = value_kind(spec, measure);
    ws.write_string_with_format(g.header_row, col, "Net", &cx.head).map_err(x)?;
    for (i, _) in d.categories.iter().enumerate() {
        let net: f64 = (0..d.series.len()).filter_map(|s| d.measure(measure).and_then(|md| md.values.get(s).and_then(|r| r.get(i)).copied().flatten())).sum();
        ws.write_number_with_format(g.first_row + i as u32, col, scaled(net, vk, cx.opts), cx.num_fmt(vk)).map_err(x)?;
    }
    let mut line_chart = Chart::new(ChartType::Line);
    let mut mk = ChartMarker::new();
    let mut mf = ChartFormat::new();
    mf.set_solid_fill(&ChartSolidFill::new().set_color(rgb(cx.ink.ink))).set_border(&ChartLine::new().set_color(rgb(cx.ink.ink)).set_width(2.0));
    mk.set_type(ChartMarkerType::LongDash).set_size(16).set_format(&mut mf);
    let s = line_chart.add_series();
    s.set_categories(&range(&cx.sheet, g.first_row, 0, g.last_row, 0)).set_values(&range(&cx.sheet, g.first_row, col, g.last_row, col)).set_name("Net");
    s.set_format(&mut ChartFormat::new().set_no_line().clone());
    s.set_marker(&mk);
    let mut label = ChartDataLabel::new();
    label.show_value().set_num_format(&signed_code(if vk == ValueKind::Percent { 2 } else { cx.opts.decimals }, vk == ValueKind::Percent)).set_font(&font(t::DATA_LABEL_PT as f64, false, cx.ink.ink));
    s.set_data_label(&label);
    chart.combine(&line_chart);
    ws.set_column_width(col, 14).map_err(x)?;
    Ok(())
}

/// Bars in a status colour: the bar is filled in the status colour and a "Status" column says it in words
/// (the colour is never alone).
fn add_bar_status(cx: &Cx, ws: &mut Worksheet, g: &GridCols, panel: usize, kind: ChartKind, col: u16) -> Result<()> {
    let d = cx.data;
    let spec = &d.spec;
    if spec.bar_status.is_empty() || kind != ChartKind::Bar || panel != 0 || d.series.len() > 1 {
        return Ok(());
    }
    ws.write_string_with_format(g.header_row, col, "Status", &cx.head).map_err(x)?;
    ws.set_column_width(col, 16).map_err(x)?;
    for (i, cat) in d.categories.iter().enumerate() {
        if let Some(st) = spec.bar_status.iter().find(|b| b.category == *cat) {
            ws.write_string_with_format(g.first_row + i as u32, col, t::tidy(&st.label), &cx.text).map_err(x)?;
        }
    }
    Ok(())
}

/// Per-category limits: a "Limit" column (and the breach text) in the table and dash markers on the chart.
fn add_limit_ticks(cx: &Cx, ws: &mut Worksheet, g: &GridCols, end: &GridEnd, chart: &mut Chart, panel: usize, measure: usize, kind: ChartKind, first_free_col: u16) -> Result<u16> {
    let d = cx.data;
    let spec = &d.spec;
    let ticks: Vec<_> = spec.limit_ticks.iter().filter(|l| l.panel == panel).collect();
    if ticks.is_empty() || !matches!(kind, ChartKind::Bar | ChartKind::Line | ChartKind::Area) || (spec.horizontal && kind == ChartKind::Bar) {
        return Ok(first_free_col);
    }
    let vk = value_kind(spec, measure);
    let col = first_free_col.max(end.next_col);
    ws.write_string_with_format(g.header_row, col, "Limit", &cx.head).map_err(x)?;
    ws.write_string_with_format(g.header_row, col + 1, "Breach", &cx.head).map_err(x)?;
    for lt in &ticks {
        if let Some(i) = d.categories.iter().position(|c| *c == lt.category) {
            let r = g.first_row + i as u32;
            ws.write_number_with_format(r, col, scaled(lt.value, vk, cx.opts), cx.num_fmt(vk)).map_err(x)?;
            if !lt.breach.is_empty() {
                ws.write_string_with_format(r, col + 1, t::tidy(&lt.breach), &cx.text).map_err(x)?;
            }
        }
    }
    let colour = ticks.first().map(|l| l.status.hex()).unwrap_or(t::STATUS_CRITICAL);
    let mut line_chart = Chart::new(ChartType::Line);
    let mut mk = ChartMarker::new();
    let mut mf = ChartFormat::new();
    mf.set_solid_fill(&ChartSolidFill::new().set_color(rgb(colour))).set_border(&ChartLine::new().set_color(rgb(colour)).set_width(1.5));
    mk.set_type(ChartMarkerType::LongDash).set_size(14).set_format(&mut mf);
    let s = line_chart.add_series();
    s.set_categories(&range(&cx.sheet, g.first_row, 0, g.last_row, 0)).set_values(&range(&cx.sheet, g.first_row, col, g.last_row, col)).set_name(if spec.limit_note.trim().is_empty() { "Limit" } else { spec.limit_note.trim() });
    s.set_format(&mut ChartFormat::new().set_no_line().clone());
    s.set_marker(&mk);
    chart.combine(&line_chart);
    ws.set_column_width(col, 14).map_err(x)?;
    ws.set_column_width(col + 1, 30).map_err(x)?;
    Ok(col + 2)
}

// ---------------------------------------------------------------------------------------- waterfall

fn waterfall_sheet(cx: &Cx, ws: &mut Worksheet, top: u32, title: &str) -> Result<usize> {
    let d = cx.data;
    let steps = waterfall_from_data(d)?;
    let vk = value_kind(&d.spec, 0);
    let st = waterfall_stack(&steps);
    let heads = [&d.spec.category as &str, "Value", "Base (no fill)", "Above zero", "Below zero"];
    for (c, h) in heads.iter().enumerate() {
        ws.write_string_with_format(top, c as u16, t::tidy(if h.is_empty() { "Step" } else { h }), &cx.head).map_err(x)?;
    }
    let first = top + 1;
    for (i, s) in steps.iter().enumerate() {
        let r = first + i as u32;
        ws.write_string_with_format(r, 0, t::tidy(&s.label), &cx.text).map_err(x)?;
        ws.write_number_with_format(r, 1, scaled(s.value, vk, cx.opts), cx.num_fmt(vk)).map_err(x)?;
        ws.write_number_with_format(r, 2, scaled(st.base[i], vk, cx.opts), cx.num_fmt(vk)).map_err(x)?;
        ws.write_number_with_format(r, 3, scaled(st.pos[i], vk, cx.opts), cx.num_fmt(vk)).map_err(x)?;
        ws.write_number_with_format(r, 4, scaled(st.neg[i], vk, cx.opts), cx.num_fmt(vk)).map_err(x)?;
    }
    let last = first + steps.len() as u32 - 1;
    ws.set_column_width(0, 24).map_err(x)?;
    for c in 1..5u16 {
        ws.set_column_width(c, 16).map_err(x)?;
    }
    let mut chart = Chart::new(ChartType::ColumnStacked);
    cx.style(&mut chart, title, vk, false, false);
    let points: Vec<ChartPoint> = steps
        .iter()
        .map(|s| {
            let c = match s.kind {
                StepKind::Start => t::WATERFALL_START,
                StepKind::End => t::WATERFALL_END,
                StepKind::Delta if s.value >= 0.0 => t::DIVERGING_POSITIVE,
                StepKind::Delta => t::DIVERGING_NEGATIVE,
            };
            ChartPoint::new().set_format(&mut fill(c))
        })
        .collect();
    // the invisible base: the bars above it float
    let s = chart.add_series();
    s.set_categories(&range(&cx.sheet, first, 0, last, 0)).set_values(&range(&cx.sheet, first, 2, last, 2)).set_name("Base").set_format(&mut no_fill()).set_gap(60);
    for (col, name) in [(3u16, "Above zero"), (4u16, "Below zero")] {
        let s = chart.add_series();
        s.set_categories(&range(&cx.sheet, first, 0, last, 0)).set_values(&range(&cx.sheet, first, col, last, col)).set_name(name).set_points(&points).set_gap(60);
    }
    chart.legend().set_hidden();
    chart.set_width(720).set_height(360);
    ws.insert_chart(top, 6, &chart).map_err(x)?;
    Ok(1)
}

// -------------------------------------------------------------------------------------- the heatmap

fn heat_sheet(cx: &Cx, ws: &mut Worksheet, top: u32) -> Result<()> {
    let d = cx.data;
    let h = heatmap(d, Transform::None);
    let vk = value_kind(&d.spec, 0);
    ws.write_string_with_format(top, 0, t::tidy(if d.spec.series.is_some() { d.spec.series.as_deref().unwrap_or("") } else { "" }), &cx.head).map_err(x)?;
    for (c, name) in h.cols.iter().enumerate() {
        ws.write_string_with_format(top, 1 + c as u16, t::tidy(name), &cx.head).map_err(x)?;
    }
    for (r, name) in h.rows.iter().enumerate() {
        ws.write_string_with_format(top + 1 + r as u32, 0, t::tidy(name), &cx.text).map_err(x)?;
        for c in 0..h.cols.len() {
            // every row × column pair is a cell; a pair with no data is 0
            let v = h.cells.get(r).and_then(|row| row.get(c)).copied().flatten().unwrap_or(0.0);
            ws.write_number_with_format(top + 1 + r as u32, 1 + c as u16, scaled(v, vk, cx.opts), cx.num_fmt(vk)).map_err(x)?;
        }
    }
    ws.set_column_width(0, 24).map_err(x)?;
    for c in 0..h.cols.len() {
        ws.set_column_width(1 + c as u16, 14).map_err(x)?;
    }
    if !h.rows.is_empty() && !h.cols.is_empty() {
        let scale = ConditionalFormat3ColorScale::new().set_minimum_color(rgb(t::SEQUENTIAL[0])).set_midpoint_color(rgb(t::SEQUENTIAL[6])).set_maximum_color(rgb(t::SEQUENTIAL[12]));
        let (r1, c1, r2, c2) = (top + 1, 1, top + h.rows.len() as u32, h.cols.len() as u16);
        ws.add_conditional_format(r1, c1, r2, c2, &scale).map_err(x)?;
        // the text is white from the sequential ramp's 7th step, as in the chart
        if let (Some(lo), Some(hi)) = (h.min, h.max) {
            if hi > lo {
                let limit = scaled(lo + (hi - lo) * (t::SEQUENTIAL_WHITE_TEXT_FROM as f64 - 0.5) / (t::SEQUENTIAL.len() - 1) as f64, vk, cx.opts);
                let white = ConditionalFormatCell::new().set_rule(ConditionalFormatCellRule::GreaterThanOrEqualTo(limit)).set_format(Format::new().set_font_color(Color::White));
                ws.add_conditional_format(r1, c1, r2, c2, &white).map_err(x)?;
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------- scatter, bubble, others

fn scatter_sheet(cx: &Cx, ws: &mut Worksheet, top: u32, title: &str) -> Result<(usize, Option<String>)> {
    let d = cx.data;
    let spec = &d.spec;
    let bubble = spec.kind == ChartKind::Bubble;
    let ycol = spec.measure_list().first().and_then(|m| m.column.clone()).unwrap_or_default();
    let mut heads = vec![spec.category.clone(), ycol.clone()];
    if bubble {
        heads.push("Size".to_string());
    }
    heads.push("Series".to_string());
    for (c, h) in heads.iter().enumerate() {
        ws.write_string_with_format(top, c as u16, t::tidy(h), &cx.head).map_err(x)?;
    }
    let n_series = d.series.len().max(1);
    let mut chart = Chart::new(ChartType::Scatter);
    cx.style(&mut chart, title, ValueKind::Plain, false, n_series > 1);
    {
        let xax = chart.x_axis();
        xax.set_major_gridlines(false).set_name(&t::tidy(&spec.category));
        let yax = chart.y_axis();
        yax.set_name(&t::tidy(&ycol));
    }
    let mut row = top + 1;
    for si in 0..n_series {
        let pts: Vec<_> = d.points.iter().filter(|p| p.series == si).collect();
        if pts.is_empty() {
            continue;
        }
        let start = row;
        for p in &pts {
            ws.write_number_with_format(row, 0, p.x, &cx.num).map_err(x)?;
            ws.write_number_with_format(row, 1, p.y, &cx.num).map_err(x)?;
            let mut c = 2;
            if bubble {
                if let Some(z) = p.size {
                    ws.write_number_with_format(row, 2, z, &cx.num).map_err(x)?;
                }
                c = 3;
            }
            ws.write_string_with_format(row, c, t::tidy(d.series.get(si).map(String::as_str).unwrap_or("")), &cx.text).map_err(x)?;
            row += 1;
        }
        let name = d.series.get(si).cloned().unwrap_or_default();
        let colour = cx.colour(&name, si, n_series);
        let mut mk = ChartMarker::new();
        let mut mf = ChartFormat::new();
        mf.set_solid_fill(&ChartSolidFill::new().set_color(rgb(colour))).set_border(&ChartLine::new().set_color(rgb(cx.ink.surface)).set_width(1.0));
        mk.set_type(ChartMarkerType::Circle).set_size(t::MARKER_PX as u8).set_format(&mut mf);
        let s = chart.add_series();
        s.set_categories(&range(&cx.sheet, start, 0, row - 1, 0)).set_values(&range(&cx.sheet, start, 1, row - 1, 1)).set_marker(&mk).set_format(ChartFormat::new().set_no_line());
        if !name.is_empty() {
            s.set_name(&t::tidy(&name));
        }
    }
    ws.set_column_width(0, 18).map_err(x)?;
    for c in 1..heads.len() as u16 {
        ws.set_column_width(c, 16).map_err(x)?;
    }
    chart.set_width(720).set_height(420);
    ws.insert_chart(top, heads.len() as u16 + 1, &chart).map_err(x)?;
    let note = bubble.then(|| NOTE_BUBBLE.to_string());
    if let Some(n) = &note {
        ws.write_string_with_format(row + 1, 0, n, &cx.text).map_err(x)?;
    }
    Ok((1, note))
}

fn histogram_sheet(cx: &Cx, ws: &mut Worksheet, top: u32, title: &str) -> Result<usize> {
    let d = cx.data;
    let (g, end) = write_grid_table(cx, ws, top, d, "")?;
    let mut chart = Chart::new(ChartType::Column);
    cx.style(&mut chart, title, ValueKind::Count, false, false);
    let colour = cx.colour("", 0, 1);
    let s = chart.add_series();
    let col = g.cols.first().map(|c| c.2).unwrap_or(1);
    s.set_categories(&range(&cx.sheet, g.first_row, 0, g.last_row, 0)).set_values(&range(&cx.sheet, g.first_row, col, g.last_row, col)).set_name("Count").set_format(&mut fill(colour)).set_gap(10);
    chart.legend().set_hidden();
    chart.set_width(720).set_height(360);
    ws.insert_chart(top, end.next_col + 1, &chart).map_err(x)?;
    Ok(1)
}

fn box_sheet(cx: &Cx, ws: &mut Worksheet, top: u32, title: &str) -> Result<usize> {
    let d = cx.data;
    let heads = [&d.spec.category as &str, "Rows", "Min", "Q1", "Median", "Q3", "Max", "Lower whisker", "Upper whisker", "Base (no fill)", "Q1 to median", "Median to Q3", "Down to lower whisker", "Up to upper whisker"];
    for (c, h) in heads.iter().enumerate() {
        ws.write_string_with_format(top, c as u16, t::tidy(if h.is_empty() { "Category" } else { h }), &cx.head).map_err(x)?;
    }
    let first = top + 1;
    for (i, b) in d.boxes.iter().enumerate() {
        let r = first + i as u32;
        ws.write_string_with_format(r, 0, t::tidy(&b.category), &cx.text).map_err(x)?;
        ws.write_number_with_format(r, 1, b.n as f64, &cx.int).map_err(x)?;
        let vals = [b.min, b.q1, b.median, b.q3, b.max, b.low_whisker, b.high_whisker, b.q1, b.median - b.q1, b.q3 - b.median, b.q1 - b.low_whisker, b.high_whisker - b.q3];
        for (k, v) in vals.iter().enumerate() {
            ws.write_number_with_format(r, 2 + k as u16, *v, &cx.num).map_err(x)?;
        }
    }
    let last = first + d.boxes.len() as u32 - 1;
    ws.set_column_width(0, 20).map_err(x)?;
    for c in 1..heads.len() as u16 {
        ws.set_column_width(c, 14).map_err(x)?;
    }
    let colour = cx.colour("", 0, 1);
    let mut chart = Chart::new(ChartType::ColumnStacked);
    cx.style(&mut chart, title, ValueKind::Plain, false, false);
    // the whiskers are custom error bars: a column of zeros is "no bar" on the other side
    let zero_col = heads.len() as u16;
    ws.write_string_with_format(top, zero_col, "Zero", &cx.head).map_err(x)?;
    for r in first..=last {
        ws.write_number_with_format(r, zero_col, 0.0, &cx.num).map_err(x)?;
    }
    let zero = range(&cx.sheet, first, zero_col, last, zero_col);
    let mut box_fmt = ChartFormat::new();
    box_fmt.set_solid_fill(&ChartSolidFill::new().set_color(rgb(colour)).set_transparency(70)).set_line(&ChartLine::new().set_color(rgb(colour)).set_width(1.5));
    let cats = range(&cx.sheet, first, 0, last, 0);
    // the invisible base, with the lower whisker as a downward error bar
    let mut down = ChartErrorBars::new();
    down.set_type(ChartErrorBarsType::Custom(zero.clone(), range(&cx.sheet, first, 12, last, 12))).set_end_cap(true).set_format(&mut line_fmt(colour, 1.5));
    chart.add_series().set_categories(&cats).set_values(&range(&cx.sheet, first, 9, last, 9)).set_name("Base (no fill)").set_format(&mut no_fill()).set_gap(80).set_y_error_bars(&down);
    chart.add_series().set_categories(&cats).set_values(&range(&cx.sheet, first, 10, last, 10)).set_name("Q1 to median").set_format(&mut box_fmt).set_gap(80);
    let mut up = ChartErrorBars::new();
    up.set_type(ChartErrorBarsType::Custom(range(&cx.sheet, first, 13, last, 13), zero)).set_end_cap(true).set_format(&mut line_fmt(colour, 1.5));
    chart.add_series().set_categories(&cats).set_values(&range(&cx.sheet, first, 11, last, 11)).set_name("Median to Q3").set_format(&mut box_fmt).set_gap(80).set_y_error_bars(&up);
    chart.legend().set_hidden();
    chart.set_width(720).set_height(380);
    ws.insert_chart(top, zero_col + 2, &chart).map_err(x)?;
    Ok(1)
}

fn donut_sheet(cx: &Cx, ws: &mut Worksheet, top: u32, title: &str) -> Result<usize> {
    let d = cx.data;
    let vk = value_kind(&d.spec, 0);
    let parts: Vec<(String, f64)> = d.categories.iter().enumerate().filter_map(|(i, c)| d.values.first().and_then(|r| r.get(i)).copied().flatten().map(|v| (c.clone(), v))).collect();
    if parts.len() > t::DONUT_MAX_SLICES {
        bail!("{}", DONUT_REFUSAL);
    }
    if parts.len() < 2 {
        bail!("A donut needs at least 2 parts, but this chart has {}. Choose another form.", parts.len());
    }
    if parts.iter().any(|(_, v)| *v < 0.0) {
        bail!("A donut cannot show negative values. Use a bar chart.");
    }
    ws.write_string_with_format(top, 0, t::tidy(if d.spec.category.is_empty() { "Category" } else { &d.spec.category }), &cx.head).map_err(x)?;
    ws.write_string_with_format(top, 1, t::tidy(&d.measure_names.first().cloned().unwrap_or_else(|| "Value".to_string())), &cx.head).map_err(x)?;
    let first = top + 1;
    for (i, (c, v)) in parts.iter().enumerate() {
        ws.write_string_with_format(first + i as u32, 0, t::tidy(c), &cx.text).map_err(x)?;
        ws.write_number_with_format(first + i as u32, 1, scaled(*v, vk, cx.opts), cx.num_fmt(vk)).map_err(x)?;
    }
    let last = first + parts.len() as u32 - 1;
    ws.set_column_width(0, 24).map_err(x)?;
    ws.set_column_width(1, 16).map_err(x)?;
    let mut chart = Chart::new(ChartType::Doughnut);
    chart.title().set_name(&t::tidy(title)).set_font(&font(t::TITLE_PT as f64 - 1.0, true, cx.ink.ink));
    chart.chart_area().set_format(ChartFormat::new().set_no_border().set_solid_fill(&ChartSolidFill::new().set_color(rgb(cx.ink.surface))));
    chart.plot_area().set_format(ChartFormat::new().set_no_border().set_no_fill());
    chart.legend().set_position(ChartLegendPosition::Right).set_font(&font(t::LABEL_PT as f64, false, cx.ink.ink2));
    let points: Vec<ChartPoint> = parts
        .iter()
        .enumerate()
        .map(|(i, (c, _))| {
            let mut f = fill(cx.colour(c, i, parts.len()));
            f.set_border(&ChartLine::new().set_color(rgb(cx.ink.surface)).set_width(2.0));
            ChartPoint::new().set_format(&mut f)
        })
        .collect();
    let s = chart.add_series();
    s.set_categories(&range(&cx.sheet, first, 0, last, 0)).set_values(&range(&cx.sheet, first, 1, last, 1)).set_points(&points);
    chart.set_hole_size(60).set_width(560).set_height(360);
    ws.insert_chart(top, 3, &chart).map_err(x)?;
    Ok(1)
}

/// The spec's order of the chart kinds Excel can draw natively (for the menu text and tests).
pub fn is_native(spec: &ChartSpec) -> bool {
    !matches!(spec.kind, ChartKind::Sankey | ChartKind::Treemap | ChartKind::Heatmap)
}
