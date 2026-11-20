use crate::engine::{
    compile_tab_lazy, compute_control_totals, export_to_csv_stream, export_to_parquet_stream,
};
use crate::models::{
    classify_column_auto_format, format_metric_value, format_with_commas, resolved_scale, ColumnFormatSpec, MetricFormatType,
    MoneyScale, NumberDisplayFormat, ParquetDialect, SplitExportFormat, SummarySpec, TabSource, TabState, TransformStep,
};
use crate::jobs::{Cancelled, JobCtx, TempOutput};
use crate::summary::{date_text, SummaryCell, SummaryValue};
use crate::models::SummaryAgg;
use crate::table_layout::{SUMMARY_BG, SUMMARY_FG};
use anyhow::{Context, Result};
use polars::datatypes::AnyValue;
use polars::prelude::*;
use printpdf::*;
use rust_xlsxwriter::{Format, Workbook};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

/// Data rows per Excel sheet: 1,048,576 rows minus the header row.
pub const EXCEL_MAX_DATA_ROWS: usize = 1_048_575;
pub const PDF_MAX_ROWS_PER_TAB: usize = 5_000;
/// Whole numbers from here up (16+ digits) are written as text: Excel keeps 15 significant digits.
const EXCEL_EXACT_INT_LIMIT: u64 = 1_000_000_000_000_000;

fn note_text_column(report: &mut XlsxExportReport, tab: &str, column: &str) {
    if !report.text_columns.iter().any(|(t, c)| t == tab && c == column) {
        report.text_columns.push((tab.to_string(), column.to_string()));
    }
}

/// Excel serial number of 1970-01-01.
const EXCEL_EPOCH_OFFSET_DAYS: f64 = 25_569.0;

/// What an Excel export wrote.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct XlsxExportReport {
    /// One entry per exported tab: (tab title, number of sheets it took).
    pub sheets: Vec<(String, usize)>,
    /// Tabs that could not be exported: (tab title, error). Written to the `Export_Errors` sheet.
    pub errors: Vec<(String, String)>,
    /// (tab title, column) of whole-number columns written as text because a value has more
    /// than 15 digits (Excel keeps only 15 significant digits of a number).
    pub text_columns: Vec<(String, String)>,
    /// Sheets exported WITHOUT their totals row because it could not be computed:
    /// (tab title, reason). The data is on its sheet; the reason is also on `Export_Errors`.
    pub totals_errors: Vec<(String, String)>,
}

impl XlsxExportReport {
    /// Plain status text: tabs that spilled onto more sheets, and failed tabs.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        let total: usize = self.sheets.iter().map(|(_, n)| n).sum();
        parts.push(format!("{} tab(s) on {} sheet(s)", self.sheets.len(), total));
        for (t, n) in &self.sheets {
            if *n > 1 {
                parts.push(format!("'{}' has more than {} rows and continues over {} sheets", t, EXCEL_MAX_DATA_ROWS, n));
            }
        }
        if !self.text_columns.is_empty() {
            let cols: Vec<String> = self
                .text_columns
                .iter()
                .map(|(t, c)| format!("'{}' in '{}'", c, t))
                .collect();
            parts.push(format!(
                "⚠ whole numbers with more than 15 digits written as text (Excel would round them): {}",
                cols.join(", ")
            ));
        }
        for (t, why) in &self.totals_errors {
            parts.push(format!(
                "⚠ '{}' was exported WITHOUT its totals row (the data is on its sheet): {} (also on the Export_Errors sheet)",
                t, why
            ));
        }
        if !self.errors.is_empty() {
            let names: Vec<&str> = self.errors.iter().map(|(t, _)| t.as_str()).collect();
            parts.push(format!(
                "⚠ {} tab(s) failed and are listed on the Export_Errors sheet: {}",
                self.errors.len(),
                names.join(", ")
            ));
        }
        parts.join("; ")
    }
}

/// One tab to export: its title, source and steps.
pub type TabToExport<'a> = (String, &'a TabSource, &'a [TransformStep]);

/// Builds and collects every tab. A tab that fails is not skipped: it comes back as
/// `(title, error)` so the workbook lists it on `Export_Errors`.
pub fn collect_tabs_for_export(
    tabs: &[TabToExport],
    temp_dir: &Path,
) -> (Vec<(String, DataFrame)>, Vec<(String, String)>) {
    let mut ok = Vec::new();
    let mut errors = Vec::new();
    for (title, source, steps) in tabs {
        match compile_tab_lazy(source, steps, temp_dir).and_then(|lf| Ok(lf.collect()?)) {
            Ok(df) => ok.push((title.clone(), df)),
            Err(e) => errors.push((title.clone(), format!("{:#}", e))),
        }
    }
    (ok, errors)
}

/// The multi-tab Excel export behind the UI button: collects the tabs and writes the workbook
/// (failed tabs on `Export_Errors`).
pub fn export_selected_tabs_to_xlsx(
    tabs: &[TabToExport],
    out_path: &Path,
    temp_dir: &Path,
) -> Result<XlsxExportReport> {
    let (ok, errors) = collect_tabs_for_export(tabs, temp_dir);
    export_tabs_to_xlsx(ok, errors, out_path)
}

pub fn export_tabs_to_xlsx(
    tabs: Vec<(String, DataFrame)>,
    errors: Vec<(String, String)>,
    out_path: &Path,
) -> Result<XlsxExportReport> {
    write_xlsx_workbook(tabs, errors, out_path, EXCEL_MAX_DATA_ROWS)
}

/// A tab for the scaled Excel export: its data and the per-column formats it is shown with.
pub struct XlsxTab {
    pub title: String,
    pub df: DataFrame,
    pub col_formats: HashMap<String, ColumnFormatSpec>,
    /// The summary row (T3); empty = no totals rows.
    pub summary: Vec<SummaryCell>,
}

/// The scale the workbook is written in; money columns say it in their header ("Balance (BDT M)").
/// Thousands and Millions keep the raw BDT value under a scaling number format (`#,##0.00,` and
/// `#,##0.00,,`), so Excel sums the real amounts; Lakh and Crore, which an Excel format cannot
/// scale, hold value ÷ divisor (see [`xlsx_scaling`]). Data files (Parquet, CSV) never use it;
/// they keep raw values.
#[derive(Clone, Copy, Debug)]
pub struct XlsxScale {
    pub num_format: NumberDisplayFormat,
    pub decimals: usize,
}

/// `export_selected_tabs_to_xlsx` for tabs that carry column formats.
pub struct ScaledTabToExport<'a> {
    pub title: String,
    pub source: &'a TabSource,
    pub steps: &'a [TransformStep],
    pub col_formats: &'a HashMap<String, ColumnFormatSpec>,
    /// The tab's summary choices; `None` = no totals rows in the workbook.
    pub summary: Option<&'a HashMap<String, SummarySpec>>,
    /// The tab's pinned control column (weight of rate-like columns by default).
    pub control_col: Option<&'a str>,
}

/// The summary cells of a collected tab (empty when no totals row is asked for). A totals row
/// that cannot be computed is an error, not a missing row.
fn summary_for_export(
    df: &DataFrame,
    overrides: Option<&HashMap<String, SummarySpec>>,
    control_col: Option<&str>,
) -> Result<Vec<SummaryCell>> {
    let Some(over) = overrides else {
        return Ok(Vec::new());
    };
    let cols: Vec<(String, DataType)> = df.get_columns().iter().map(|s| (s.name().to_string(), s.dtype().clone())).collect();
    let specs = crate::summary::effective_specs(&cols, over, control_col);
    crate::summary::compute_summary(df.clone().lazy(), &specs).context("The totals row could not be computed")
}

pub fn export_selected_tabs_to_xlsx_scaled(
    tabs: &[ScaledTabToExport],
    out_path: &Path,
    temp_dir: &Path,
    scale: XlsxScale,
) -> Result<XlsxExportReport> {
    export_scaled_tabs_with(tabs, out_path, temp_dir, scale, &summary_for_export)
}

/// How a collected tab's totals row is computed (tests pass one that fails).
pub type SummaryFn = dyn Fn(&DataFrame, Option<&HashMap<String, SummarySpec>>, Option<&str>) -> Result<Vec<SummaryCell>>;

/// `export_selected_tabs_to_xlsx_scaled` with the totals computation as a parameter. A tab whose
/// totals row cannot be computed is NOT dropped: its sheet is written without the totals row and
/// the reason goes to `Export_Errors` and the report.
pub fn export_scaled_tabs_with(
    tabs: &[ScaledTabToExport],
    out_path: &Path,
    temp_dir: &Path,
    scale: XlsxScale,
    summarize: &SummaryFn,
) -> Result<XlsxExportReport> {
    let mut ok = Vec::new();
    let mut errors = Vec::new();
    let mut totals_errors = Vec::new();
    for t in tabs {
        match compile_tab_lazy(t.source, t.steps, temp_dir).and_then(|lf| Ok(lf.collect()?)) {
            Ok(df) => {
                // The summary is computed from the same collected data the sheet holds.
                // (a pivot keeps its own Grand total and gets no extra totals row)
                let own_totals = crate::pivot_table::total_rows_expr(t.steps).is_some();
                let summary = match if own_totals { Ok(Vec::new()) } else { summarize(&df, t.summary, t.control_col) } {
                    Ok(cells) => cells,
                    Err(e) => {
                        totals_errors.push((t.title.clone(), format!("{:#}", e)));
                        Vec::new()
                    }
                };
                ok.push(XlsxTab { title: t.title.clone(), df, col_formats: t.col_formats.clone(), summary });
            }
            Err(e) => errors.push((t.title.clone(), format!("{:#}", e))),
        }
    }
    write_xlsx_tabs_noted(ok, errors, totals_errors, out_path, EXCEL_MAX_DATA_ROWS, Some(scale))
}

/// `export_tabs_to_xlsx` with the rows-per-sheet limit as a parameter (tests use a small one).
pub fn write_xlsx_workbook(
    tabs: Vec<(String, DataFrame)>,
    errors: Vec<(String, String)>,
    out_path: &Path,
    rows_per_sheet: usize,
) -> Result<XlsxExportReport> {
    let tabs = tabs
        .into_iter()
        .map(|(title, df)| XlsxTab { title, df, col_formats: HashMap::new(), summary: Vec::new() })
        .collect();
    write_xlsx_tabs(tabs, errors, out_path, rows_per_sheet, None)
}

fn xlsx_decimals_format(decimals: usize) -> String {
    if decimals == 0 {
        "#,##0".to_string()
    } else {
        format!("#,##0.{}", "0".repeat(decimals.min(6)))
    }
}

/// How a money column is put in a workbook: the number the cell holds is value ÷ the first
/// value, and the second is added to its number format. K and M: raw value, format scaled by
/// "," / ",,"; Lakh and Crore: divided value, plain format.
pub fn xlsx_scaling(ms: &MoneyScale) -> (f64, &'static str) {
    match ms.suffix {
        "K" => (1.0, ","),
        "M" => (1.0, ",,"),
        _ => (ms.divisor, ""),
    }
}

/// A money column of a scaled workbook.
struct ColScale {
    unit: &'static str,
    /// The cell holds value ÷ this.
    divide: f64,
    /// Appended to the column's number format ("," or ",," for K and M).
    fmt_suffix: &'static str,
    fmt: Format,
}

pub fn write_xlsx_tabs(
    tabs: Vec<XlsxTab>,
    errors: Vec<(String, String)>,
    out_path: &Path,
    rows_per_sheet: usize,
    scale: Option<XlsxScale>,
) -> Result<XlsxExportReport> {
    write_xlsx_tabs_noted(tabs, errors, Vec::new(), out_path, rows_per_sheet, scale)
}

/// `write_xlsx_tabs` plus `totals_errors`: (tab title, reason) for sheets whose totals row was
/// left out. They are written to `Export_Errors` after the failed tabs.
pub fn write_xlsx_tabs_noted(
    tabs: Vec<XlsxTab>,
    errors: Vec<(String, String)>,
    totals_errors: Vec<(String, String)>,
    out_path: &Path,
    rows_per_sheet: usize,
    scale: Option<XlsxScale>,
) -> Result<XlsxExportReport> {
    write_xlsx_tabs_charted(tabs, Vec::new(), errors, totals_errors, out_path, rows_per_sheet, scale)
}

/// The native Excel charts of a tab (Phase 7 C6): each chart gets a sheet of its own right after the
/// tab's sheet, named "<sheet> chart", with the chart's table and the chart beside it.
#[derive(Clone, Debug)]
pub struct TabCharts {
    /// The title of the tab the charts belong to (as in `XlsxTab::title`).
    pub tab_title: String,
    pub charts: Vec<(crate::charts::data::ChartData, crate::charts::layout::LayoutOpts)>,
}

/// `write_xlsx_tabs_noted` with native charts after the sheets of the tabs that have them.
pub fn write_xlsx_tabs_charted(
    tabs: Vec<XlsxTab>,
    tab_charts: Vec<TabCharts>,
    errors: Vec<(String, String)>,
    totals_errors: Vec<(String, String)>,
    out_path: &Path,
    rows_per_sheet: usize,
    scale: Option<XlsxScale>,
) -> Result<XlsxExportReport> {
    let rows_per_sheet = rows_per_sheet.max(1);
    let mut workbook = Workbook::new();
    let mut report = XlsxExportReport::default();

    let header_fmt = Format::new()
        .set_bold()
        .set_background_color(rust_xlsxwriter::Color::RGB(0x1E293B))
        .set_font_color(rust_xlsxwriter::Color::White);

    let num_fmt = Format::new().set_num_format("#,##0.00");
    let date_fmt = Format::new().set_num_format("dd-mmm-yyyy");
    let datetime_fmt = Format::new().set_num_format("dd-mmm-yyyy hh:mm:ss");

    for (idx, tab) in tabs.into_iter().enumerate() {
        let XlsxTab { title: raw_title, df, col_formats, summary } = tab;
        let base_name = sanitize_sheet_name(&raw_title, idx + 1);
        let cols = df.get_columns();
        let height = df.height();
        // Money columns in the chosen scale.
        let col_scales: Vec<Option<ColScale>> = cols
            .iter()
            .map(|c| {
                let sc = scale?;
                if !matches!(c.dtype(), DataType::Float64 | DataType::Float32) {
                    return None;
                }
                let spec = col_formats.get(c.name().as_str());
                let ms = resolved_scale(c.name().as_str(), spec, sc.num_format)?;
                let dec = spec.map(|s| s.decimals).unwrap_or(sc.decimals);
                let (divide, fmt_suffix) = xlsx_scaling(&ms);
                let fmt = Format::new().set_num_format(format!("{}{}", xlsx_decimals_format(dec), fmt_suffix));
                Some(ColScale { unit: ms.unit, divide, fmt_suffix, fmt })
            })
            .collect();
        let sheet_count = height.div_ceil(rows_per_sheet).max(1);

        for part in 0..sheet_count {
            let worksheet = workbook.add_worksheet();
            let name = if part == 0 {
                base_name.clone()
            } else {
                let suffix = format!(" ({})", part + 1);
                let keep = 31usize.saturating_sub(suffix.chars().count());
                let stem: String = base_name.chars().take(keep).collect();
                format!("{}{}", stem, suffix)
            };
            worksheet
                .set_name(&name)
                .with_context(|| format!("Excel: invalid sheet name '{}'", name))?;

            for (c_idx, series) in cols.iter().enumerate() {
                let header_text = match col_scales.get(c_idx).and_then(|c| c.as_ref()) {
                    Some(cs) => format!("{} ({})", series.name().as_str(), cs.unit),
                    None => series.name().to_string(),
                };
                worksheet.write_string_with_format(0, c_idx as u16, &header_text, &header_fmt)?;
                worksheet.set_column_width(c_idx as u16, 18.0)?;
            }
            worksheet.set_freeze_panes(1, 0)?;

            let first = part * rows_per_sheet;
            let last = (first + rows_per_sheet).min(height);
            for r_idx in first..last {
                let xls_row = (r_idx - first + 1) as u32;
                for (c_idx, series) in cols.iter().enumerate() {
                    let xls_col = c_idx as u16;
                    match series.get(r_idx) {
                        Ok(AnyValue::Null) => {}
                        Ok(AnyValue::Float64(v)) => match col_scales.get(c_idx).and_then(|c| c.as_ref()) {
                            Some(cs) => {
                                worksheet.write_number_with_format(xls_row, xls_col, v / cs.divide, &cs.fmt)?;
                            }
                            None => {
                                worksheet.write_number_with_format(xls_row, xls_col, v, &num_fmt)?;
                            }
                        },
                        Ok(AnyValue::Float32(v)) => match col_scales.get(c_idx).and_then(|c| c.as_ref()) {
                            Some(cs) => {
                                worksheet.write_number_with_format(xls_row, xls_col, v as f64 / cs.divide, &cs.fmt)?;
                            }
                            None => {
                                worksheet.write_number_with_format(xls_row, xls_col, v as f64, &num_fmt)?;
                            }
                        },
                        Ok(AnyValue::Int64(v)) if v.unsigned_abs() >= EXCEL_EXACT_INT_LIMIT => {
                            worksheet.write_string(xls_row, xls_col, v.to_string())?;
                            note_text_column(&mut report, &raw_title, series.name().as_str());
                        }
                        Ok(AnyValue::UInt64(v)) if v >= EXCEL_EXACT_INT_LIMIT => {
                            worksheet.write_string(xls_row, xls_col, v.to_string())?;
                            note_text_column(&mut report, &raw_title, series.name().as_str());
                        }
                        Ok(AnyValue::UInt64(v)) => {
                            worksheet.write_number(xls_row, xls_col, v as f64)?;
                        }
                        Ok(AnyValue::Int64(v)) => {
                            worksheet.write_number(xls_row, xls_col, v as f64)?;
                        }
                        Ok(AnyValue::Int32(v)) => {
                            worksheet.write_number(xls_row, xls_col, v as f64)?;
                        }
                        Ok(AnyValue::UInt32(v)) => {
                            worksheet.write_number(xls_row, xls_col, v as f64)?;
                        }
                        Ok(AnyValue::Date(d)) => {
                            worksheet.write_number_with_format(
                                xls_row,
                                xls_col,
                                d as f64 + EXCEL_EPOCH_OFFSET_DAYS,
                                &date_fmt,
                            )?;
                        }
                        Ok(AnyValue::Datetime(v, unit, _)) => {
                            let per_day = match unit {
                                TimeUnit::Nanoseconds => 86_400_000_000_000.0,
                                TimeUnit::Microseconds => 86_400_000_000.0,
                                TimeUnit::Milliseconds => 86_400_000.0,
                            };
                            worksheet.write_number_with_format(
                                xls_row,
                                xls_col,
                                v as f64 / per_day + EXCEL_EPOCH_OFFSET_DAYS,
                                &datetime_fmt,
                            )?;
                        }
                        Ok(AnyValue::String(s)) => {
                            worksheet.write_string(xls_row, xls_col, s)?;
                        }
                        Ok(other) => {
                            worksheet.write_string(xls_row, xls_col, format!("{}", other))?;
                        }
                        Err(e) => {
                            anyhow::bail!(
                                "Excel: could not read row {} of column '{}' in '{}': {}",
                                r_idx + 1,
                                series.name(),
                                raw_title,
                                e
                            );
                        }
                    }
                }
            }
            // Totals: two rows (what each cell is, then its value) under the last part of the data.
            if part + 1 == sheet_count && !summary.is_empty() {
                write_xlsx_totals(worksheet, &summary, cols, last - first, &col_scales, &col_formats, scale)?;
            }
        }
        // the tab's charts: a sheet each, right after the tab's own
        if let Some(tc) = tab_charts.iter().find(|t| t.tab_title == raw_title) {
            for (n, (data, opts)) in tc.charts.iter().enumerate() {
                let name = crate::charts::pack::chart_sheet_name(&base_name, n);
                crate::charts::xlsx::add_chart_sheet(&mut workbook, &name, data, opts).with_context(|| format!("Excel: the chart of '{}' could not be written", raw_title))?;
            }
        }
        report.sheets.push((raw_title, sheet_count));
    }

    if !errors.is_empty() || !totals_errors.is_empty() {
        let ws = workbook.add_worksheet();
        ws.set_name("Export_Errors")?;
        ws.write_string_with_format(0, 0, "Tab", &header_fmt)?;
        ws.write_string_with_format(0, 1, "Error", &header_fmt)?;
        ws.set_column_width(0, 30.0)?;
        ws.set_column_width(1, 120.0)?;
        for (i, (tab, err)) in errors.iter().enumerate() {
            ws.write_string((i + 1) as u32, 0, tab)?;
            ws.write_string((i + 1) as u32, 1, err)?;
        }
        for (j, (tab, why)) in totals_errors.iter().enumerate() {
            let row = (errors.len() + j + 1) as u32;
            ws.write_string(row, 0, tab)?;
            ws.write_string(
                row,
                1,
                format!("Sheet exported WITHOUT its totals row (the data is complete): {}", why),
            )?;
        }
        report.errors = errors;
        report.totals_errors = totals_errors;
    }

    if report.sheets.is_empty() && report.errors.is_empty() {
        anyhow::bail!("Excel: nothing to export. Tick at least one tab.");
    }

    workbook
        .save(out_path)
        .context("Failed to save consolidated Excel workbook")?;
    Ok(report)
}

/// The summary as two rows in the summary colour, one blank row below the data: the label of each
/// aggregation ("Sum", "W.Avg by Balance", ...), then its value. Money columns use the same scale
/// as the data above them; counts are plain numbers; dates are dates.
fn write_xlsx_totals(
    worksheet: &mut rust_xlsxwriter::Worksheet,
    summary: &[SummaryCell],
    cols: &[Series],
    data_rows: usize,
    col_scales: &[Option<ColScale>],
    col_formats: &HashMap<String, ColumnFormatSpec>,
    scale: Option<XlsxScale>,
) -> Result<()> {
    let bg = rust_xlsxwriter::Color::RGB(SUMMARY_BG);
    let fg = rust_xlsxwriter::Color::RGB(SUMMARY_FG);
    let label_fmt = Format::new().set_bold().set_font_size(9).set_background_color(bg).set_font_color(fg);
    let text_fmt = Format::new().set_bold().set_background_color(bg).set_font_color(fg);
    let blank_fmt = Format::new().set_background_color(bg);
    let label_row = (data_rows + 2) as u32;
    let value_row = (data_rows + 3) as u32;
    for (c_idx, series) in cols.iter().enumerate() {
        let col = c_idx as u16;
        let Some(cell) = summary.iter().find(|c| c.column == series.name().as_str()) else {
            worksheet.write_blank(label_row, col, &blank_fmt)?;
            worksheet.write_blank(value_row, col, &blank_fmt)?;
            continue;
        };
        let label = match (&cell.agg, &cell.weight_col) {
            (SummaryAgg::None, _) => String::new(),
            (SummaryAgg::WeightedAvg, Some(w)) => format!("{} by {}", cell.agg.label(), w),
            (agg, _) => agg.label().to_string(),
        };
        worksheet.write_string_with_format(label_row, col, &label, &label_fmt)?;
        let decimals = col_formats
            .get(series.name().as_str())
            .map(|s| s.decimals)
            .or_else(|| scale.map(|s| s.decimals))
            .unwrap_or(2);
        let num_code = xlsx_decimals_format(decimals);
        match &cell.value {
            None => {
                worksheet.write_blank(value_row, col, &blank_fmt)?;
            }
            Some(SummaryValue::Count(n)) => {
                let f = Format::new().set_bold().set_background_color(bg).set_font_color(fg).set_num_format("#,##0");
                worksheet.write_number_with_format(value_row, col, *n as f64, &f)?;
            }
            Some(SummaryValue::Number(x)) => {
                // the same rule as the cells above, so the row equals Excel's SUM of the column
                let (v, code) = match col_scales.get(c_idx).and_then(|c| c.as_ref()) {
                    Some(cs) => (*x / cs.divide, format!("{}{}", num_code, cs.fmt_suffix)),
                    None => (*x, num_code.clone()),
                };
                let f = Format::new().set_bold().set_background_color(bg).set_font_color(fg).set_num_format(code);
                worksheet.write_number_with_format(value_row, col, v, &f)?;
            }
            Some(SummaryValue::Date(d)) => {
                let f = Format::new().set_bold().set_background_color(bg).set_font_color(fg).set_num_format("dd-mmm-yyyy");
                worksheet.write_number_with_format(value_row, col, *d as f64 + EXCEL_EPOCH_OFFSET_DAYS, &f)?;
            }
            Some(SummaryValue::Text(t)) => {
                worksheet.write_string_with_format(value_row, col, t, &text_fmt)?;
            }
        }
    }
    let _ = date_text; // dates are written as Excel dates; the text form is for the screen and PDF
    Ok(())
}

fn sanitize_sheet_name(name: &str, fallback_idx: usize) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '[' | ']' | ':' | '*' | '?' | '/' | '\\' => '_',
            _ => c,
        })
        .take(28)
        .collect();
    if cleaned.trim().is_empty() {
        format!("View_{}", fallback_idx)
    } else {
        format!("{}_{}", fallback_idx, cleaned.trim())
            .chars()
            .take(31)
            .collect()
    }
}

/// What a PDF export left out (columns that did not fit, rows past the per-tab limit).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PdfExportReport {
    /// (tab title, omitted column names) for tabs with more columns than fit the page.
    pub omitted_columns: Vec<(String, Vec<String>)>,
    /// Tabs with more than `PDF_MAX_ROWS_PER_TAB` rows (only the first ones are printed).
    pub truncated_tabs: Vec<String>,
}

impl PdfExportReport {
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        for (t, cols) in &self.omitted_columns {
            parts.push(format!(
                "'{}': columns {} omitted (they do not fit the page) — use Excel export",
                t,
                cols.join(", ")
            ));
        }
        for t in &self.truncated_tabs {
            parts.push(format!(
                "'{}': only the first {} rows printed — use Excel export",
                t, PDF_MAX_ROWS_PER_TAB
            ));
        }
        parts.join("; ")
    }
}

pub fn export_tabs_to_pdf(
    tabs: Vec<(String, DataFrame, HashMap<String, ColumnFormatSpec>)>,
    out_path: &Path,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
) -> Result<PdfExportReport> {
    let tabs = tabs
        .into_iter()
        .map(|(title, df, col_formats)| PdfTab { title, df, col_formats, summary: Vec::new(), chart: None })
        .collect();
    export_pdf_tabs(tabs, out_path, num_format, global_decimals)
}

/// A tab for the PDF report: the rows to print (at most `PDF_MAX_ROWS_PER_TAB` + 1), the column
/// formats, and the summary of the whole result (computed before the rows were cut).
pub struct PdfTab {
    pub title: String,
    pub df: DataFrame,
    pub col_formats: HashMap<String, ColumnFormatSpec>,
    /// Empty = no totals band.
    pub summary: Vec<SummaryCell>,
    /// A chart tab: the page shows this chart (vector drawing, embedded fonts), not the table; the table follows on the
    /// next page only when `PdfOptions::chart_tables` is set.
    pub chart: Option<(crate::charts::data::ChartData, crate::charts::layout::LayoutOpts)>,
}

/// What the PDF export dialog asks (L2).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PdfOptions {
    /// The heading of every page; blank = no heading line.
    pub title: String,
    pub page: crate::pdf_layout::PdfPage,
    /// "Add the data table on the next page": a chart tab also prints its table (off by default).
    pub chart_tables: bool,
}

pub fn export_pdf_tabs(
    tabs: Vec<PdfTab>,
    out_path: &Path,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
) -> Result<PdfExportReport> {
    export_pdf_tabs_with(tabs, out_path, num_format, global_decimals, &PdfOptions::default())
}

/// `export_pdf_tabs` with the title and the paper chosen in the export dialog.
pub fn export_pdf_tabs_with(
    tabs: Vec<PdfTab>,
    out_path: &Path,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
    opts: &PdfOptions,
) -> Result<PdfExportReport> {
    use crate::pdf_layout::*;
    let (page_w, page_h) = (opts.page.width_mm(), opts.page.height_mm());
    let heading: Vec<String> = wrap_text_mm(opts.title.trim(), opts.page.usable_w_mm(), TITLE_PT, FontKind::Bold);
    let doc_title = if heading.is_empty() { "Report".to_string() } else { heading.join(" ") };
    let (doc, page1, layer1) = PdfDocument::new(doc_title, Mm(page_w), Mm(page_h), "Layer 1");

    let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold)?;
    let font_reg = doc.add_builtin_font(BuiltinFont::Helvetica)?;

    let mut report = PdfExportReport::default();
    let mut current_page = page1;
    let mut current_layer = layer1;
    let mut first_page = true;
    let black = Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None));
    let rgb = |hex: u32| {
        Color::Rgb(Rgb::new(((hex >> 16) & 0xFF) as f32 / 255.0, ((hex >> 8) & 0xFF) as f32 / 255.0, (hex & 0xFF) as f32 / 255.0, None))
    };
    // a horizontal rule from x1 to x2 (mm) at y (mm)
    let rule = |layer: &PdfLayerReference, x1: f32, x2: f32, y: f32, thickness: f32| {
        layer.set_outline_color(black.clone());
        layer.set_outline_thickness(thickness);
        layer.add_line(Line { points: vec![(Point::new(Mm(x1), Mm(y)), false), (Point::new(Mm(x2), Mm(y)), false)], is_closed: false });
    };

    let mut chart_fonts: Option<crate::charts::pdf::ChartPdfFonts> = None;
    for PdfTab { title, df, col_formats, summary, chart } in tabs {
        // a chart tab: the chart on its own page (the heading of the export dialog above it); the table only when asked for
        if let Some((cdata, copts)) = chart {
            if !first_page {
                let (p, l) = doc.add_page(Mm(page_w), Mm(page_h), "Layer");
                current_page = p;
                current_layer = l;
            }
            first_page = false;
            if chart_fonts.is_none() {
                chart_fonts = Some(crate::charts::pdf::add_fonts(&doc)?);
            }
            let layer = doc.get_page(current_page).get_layer(current_layer);
            let mut y = opts.page.top_mm();
            for line in &heading {
                layer.use_text(line.clone(), TITLE_PT, Mm(LEFT_MM), Mm(y), &font_bold);
                y -= 5.5;
            }
            let top = y + 2.0;
            let area = crate::charts::pdf::ChartArea { left_mm: LEFT_MM, top_mm: top, width_mm: opts.page.usable_w_mm(), height_mm: (top - 12.0).max(40.0) };
            if let Some(f) = chart_fonts.as_ref() {
                crate::charts::pdf::draw_chart(&layer, f, &cdata, &copts, area).with_context(|| format!("Chart '{}' could not be drawn on the page", title))?;
            }
            if !opts.chart_tables {
                continue;
            }
        }
        let truncated = df.height() > PDF_MAX_ROWS_PER_TAB;
        if truncated {
            report.truncated_tabs.push(title.clone());
        }
        let layout = layout_pdf_tab_on(&opts.page, &title, &df, &col_formats, num_format, global_decimals, &summary, PDF_MAX_ROWS_PER_TAB);
        let n_parts = layout.parts.len();
        let total_height = df.height();

        for (pi, part) in layout.parts.iter().enumerate() {
            if !first_page {
                let (p, l) = doc.add_page(Mm(page_w), Mm(page_h), "Layer");
                current_page = p;
                current_layer = l;
            }
            first_page = false;
            let width_mm: f32 = part.columns.iter().map(|c| c.w).sum();
            let right_mm = LEFT_MM + width_mm;

            // Title, unit, notes and the column headers at the top of every page of the part.
            let page_header = |layer: &PdfLayerReference| -> f32 {
                let mut y = opts.page.top_mm();
                // the heading is what was typed in the export dialog; with none there is no heading line
                for line in &heading {
                    layer.use_text(line.clone(), TITLE_PT, Mm(LEFT_MM), Mm(y), &font_bold);
                    y -= 5.5;
                }
                if let Some(u) = &layout.unit_line {
                    layer.use_text(u.clone(), NOTE_PT, Mm(LEFT_MM), Mm(y), &font_reg);
                    y -= 4.5;
                }
                if n_parts > 1 {
                    layer.use_text(
                        format!("Part {} of {}: columns {} to {} (the row labels are repeated)", pi + 1, n_parts, part.first_data, part.last_data),
                        NOTE_PT,
                        Mm(LEFT_MM),
                        Mm(y),
                        &font_reg,
                    );
                    y -= 4.5;
                }
                if truncated {
                    layer.use_text(
                        format!("Only the first {} of {} rows are printed — use Excel export", PDF_MAX_ROWS_PER_TAB, total_height),
                        NOTE_PT,
                        Mm(LEFT_MM),
                        Mm(y),
                        &font_bold,
                    );
                    y -= 4.5;
                }
                y -= 2.0;
                if let Some(groups) = &part.group_header {
                    for g in groups {
                        for l in &g.lines {
                            layer.use_text(l.text.clone(), HEADER_PT, Mm(l.x), Mm(y), &font_bold);
                        }
                        rule(layer, g.col_x + PAD_MM, g.col_x + g.col_w - PAD_MM, y - 1.0, 0.2);
                    }
                    y -= HEADER_LINE_MM + 1.2;
                }
                let max_lines = part.header.iter().map(|h| h.lines.len()).max().unwrap_or(1).max(1);
                for h in &part.header {
                    // a one-line header sits on the last line, next to the values
                    let skip = max_lines - h.lines.len();
                    for (j, l) in h.lines.iter().enumerate() {
                        layer.use_text(l.text.clone(), HEADER_PT, Mm(l.x), Mm(y - (skip + j) as f32 * HEADER_LINE_MM), &font_bold);
                    }
                }
                y -= (max_lines - 1) as f32 * HEADER_LINE_MM;
                rule(layer, LEFT_MM, right_mm, y - 1.4, 0.4);
                y - 6.0
            };

            let mut y_mm = page_header(&doc.get_page(current_page).get_layer(current_layer));

            for row in &part.rows {
                if y_mm < 15.0 {
                    let (p, l) = doc.add_page(Mm(page_w), Mm(page_h), "Layer");
                    current_page = p;
                    current_layer = l;
                    y_mm = page_header(&doc.get_page(current_page).get_layer(current_layer));
                }
                let layer = doc.get_page(current_page).get_layer(current_layer);
                if row.total {
                    rule(&layer, LEFT_MM, right_mm, y_mm + 3.4, 0.4);
                }
                for cell in &row.cells {
                    if cell.text.text.is_empty() {
                        continue;
                    }
                    layer.use_text(cell.text.text.clone(), CELL_PT, Mm(cell.text.x), Mm(y_mm), if row.total { &font_bold } else { &font_reg });
                }
                y_mm -= ROW_H_MM;
            }

            // The summary of the whole result as a band in the summary colour under the rows.
            if part.summary.iter().any(|c| c.is_some()) {
                if y_mm < 26.0 {
                    let (p, l) = doc.add_page(Mm(page_w), Mm(page_h), "Layer");
                    current_page = p;
                    current_layer = l;
                    y_mm = page_header(&doc.get_page(current_page).get_layer(current_layer));
                }
                let layer = doc.get_page(current_page).get_layer(current_layer);
                let (x0, x1) = (LEFT_MM - 1.0, right_mm - 1.0);
                let (y_bottom, y_top) = (y_mm - 5.0, y_mm + 4.5);
                layer.set_fill_color(rgb(SUMMARY_BG));
                layer.add_rect(Rect::new(Mm(x0), Mm(y_bottom), Mm(x1), Mm(y_top)).with_mode(printpdf::path::PaintMode::Fill));
                layer.set_fill_color(rgb(SUMMARY_FG));
                for c in part.summary.iter().flatten() {
                    layer.use_text(c.label.text.clone(), 6.5, Mm(c.label.x), Mm(y_mm + 1.0), &font_bold);
                    layer.use_text(c.value.text.text.clone(), CELL_PT, Mm(c.value.text.x), Mm(y_mm - 3.0), &font_bold);
                }
                layer.set_fill_color(rgb(0x000000));
            }
        }
    }

    doc.save(&mut BufWriter::new(File::create(out_path)?))
        .context("Failed to write PDF file")?;
    Ok(report)
}

/// Writes one file per value of `split_col`. Rows whose key is null or blank go to
/// `BLANK_KEY.<ext>`; file names that collide after sanitising (compared case-insensitively)
/// get `_2`, `_3` … suffixes. Writes `split_manifest.csv` (key, file, rows, control sum) and
/// fails if the files do not hold every source row. Returns the number of files written.
pub fn split_and_export_by_column(
    lf: LazyFrame,
    split_col: &str,
    out_dir: &Path,
    format: SplitExportFormat,
    dialect: ParquetDialect,
    mdy_dates: bool,
    control_col: Option<&str>,
) -> Result<usize> {
    split_and_export_by_column_ctx(
        lf, split_col, out_dir, format, dialect, mdy_dates, control_col, &JobCtx::inert(),
    )
}

/// As `split_and_export_by_column`, with progress and cancel. Each file is written through a
/// temporary name; a cancel removes the files this run already wrote and writes no manifest.
#[allow(clippy::too_many_arguments)]
pub fn split_and_export_by_column_ctx(
    lf: LazyFrame,
    split_col: &str,
    out_dir: &Path,
    format: SplitExportFormat,
    dialect: ParquetDialect,
    mdy_dates: bool,
    control_col: Option<&str>,
    ctx: &JobCtx,
) -> Result<usize> {
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("Could not create the output folder {:?}", out_dir))?;

    let key_text = col(split_col).cast(DataType::String);
    let blank = key_text.clone().is_null().or(key_text
        .clone()
        .str()
        .strip_chars(lit(NULL))
        .eq(lit("")));

    let (source_rows, source_sum) = compute_control_totals(lf.clone(), control_col)?;

    let unique_df = lf
        .clone()
        .filter(blank.clone().not())
        .select([key_text.clone().alias("__mp_key")])
        .unique_stable(None, UniqueKeepStrategy::First)
        .collect()
        .context("Failed to collect unique partition values")?;
    let keys: Vec<String> = unique_df
        .column("__mp_key")?
        .str()?
        .into_iter()
        .flatten()
        .map(|s| s.to_string())
        .collect();

    // The blank group first, so it gets the plain BLANK_KEY name.
    let mut groups: Vec<(Option<String>, LazyFrame)> = Vec::with_capacity(keys.len() + 1);
    groups.push((None, lf.clone().filter(blank)));
    for k in keys {
        let sub = lf.clone().filter(key_text.clone().eq(lit(k.clone())));
        groups.push((Some(k), sub));
    }

    let ext = match format {
        SplitExportFormat::Parquet => "parquet",
        SplitExportFormat::Csv => "csv",
        SplitExportFormat::Xlsx => "xlsx",
    };
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut manifest_key: Vec<Option<String>> = Vec::new();
    let mut manifest_file: Vec<String> = Vec::new();
    let mut manifest_rows: Vec<u64> = Vec::new();
    let mut manifest_sum: Vec<Option<f64>> = Vec::new();
    let mut total_rows = 0usize;

    let mut written_paths: Vec<PathBuf> = Vec::new();
    let n_groups = groups.len().max(1);
    for (gi, (key, sub_lf)) in groups.into_iter().enumerate() {
        if ctx.is_cancelled() {
            for w in &written_paths {
                let _ = std::fs::remove_file(w);
            }
            return Err(Cancelled.into());
        }
        ctx.set_progress(
            format!("Writing file {} of {}…", gi + 1, n_groups),
            Some(gi as f32 / n_groups as f32),
        );
        let (rows, sum) = compute_control_totals(sub_lf.clone(), control_col)?;
        if key.is_none() && rows == 0 {
            continue;
        }
        let base = match &key {
            None => "BLANK_KEY".to_string(),
            Some(k) => sanitize_file_stem(k),
        };
        let mut stem = base.clone();
        let mut n = 2;
        while used.contains(&stem.to_lowercase()) {
            stem = format!("{}_{}", base, n);
            n += 1;
        }
        used.insert(stem.to_lowercase());
        let file_name = format!("{}.{}", stem, ext);
        let p = out_dir.join(&file_name);
        let tmp = TempOutput::new(&p);
        match format {
            SplitExportFormat::Parquet => export_to_parquet_stream(sub_lf, tmp.path(), dialect)?,
            SplitExportFormat::Csv => export_to_csv_stream(sub_lf, tmp.path(), mdy_dates)?,
            SplitExportFormat::Xlsx => {
                let sub_df = sub_lf.collect()?;
                export_tabs_to_xlsx(vec![(stem.clone(), sub_df)], Vec::new(), tmp.path())?;
            }
        }
        tmp.commit()?;
        written_paths.push(p.clone());
        total_rows += rows;
        manifest_key.push(key);
        manifest_file.push(file_name);
        manifest_rows.push(rows as u64);
        manifest_sum.push(sum);
    }

    if ctx.is_cancelled() {
        for w in &written_paths {
            let _ = std::fs::remove_file(w);
        }
        return Err(Cancelled.into());
    }
    let files = manifest_file.len();
    let mut manifest = DataFrame::new(vec![
        Series::new("key".into(), manifest_key).into(),
        Series::new("file".into(), manifest_file).into(),
        Series::new("rows".into(), manifest_rows).into(),
        Series::new("control_sum".into(), manifest_sum).into(),
    ])?;
    let manifest_path = out_dir.join("split_manifest.csv");
    let mut f = File::create(&manifest_path)
        .with_context(|| format!("Could not write {:?}", manifest_path))?;
    CsvWriter::new(&mut f)
        .include_header(true)
        .finish(&mut manifest)
        .with_context(|| format!("Could not write {:?}", manifest_path))?;

    if total_rows != source_rows {
        anyhow::bail!(
            "Split export: the {} files hold {} rows but the source has {}. Do not use these files; see {:?}.",
            files,
            total_rows,
            source_rows,
            manifest_path
        );
    }
    if let Some(src_sum) = source_sum {
        let split_sum: f64 = manifest.column("control_sum")?.f64()?.into_iter().flatten().sum();
        if (split_sum - src_sum).abs() > 0.005_f64.max(src_sum.abs() * 1e-9) {
            anyhow::bail!(
                "Split export: the files' control sum {:.2} differs from the source's {:.2}. Do not use these files; see {:?}.",
                split_sum,
                src_sum,
                manifest_path
            );
        }
    }
    Ok(files)
}

/// Letters, digits, '-' and '_' are kept; anything else becomes '_'.
fn sanitize_file_stem(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn format_df_for_clipboard(
    df: &DataFrame,
    col_formats: &HashMap<String, ColumnFormatSpec>,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
    as_markdown: bool,
) -> String {
    let cols = df.get_columns();
    let rows = df.height().min(2000);
    let mut out = String::new();

    if as_markdown {
        let headers: Vec<&str> = cols.iter().map(|c| c.name().as_str()).collect();
        out.push_str("| ");
        out.push_str(&headers.join(" | "));
        out.push_str(" |\n|");
        for _ in cols {
            out.push_str(" --- |");
        }
        out.push('\n');

        for r in 0..rows {
            out.push_str("| ");
            let vals: Vec<String> = cols
                .iter()
                .map(|c| {
                    let cname = c.name().as_str();
                    format_cell_val(
                        c.get(r).ok(),
                        cname,
                        col_formats.get(cname),
                        num_format,
                        global_decimals,
                    )
                })
                .collect();
            out.push_str(&vals.join(" | "));
            out.push_str(" |\n");
        }
    } else {
        let headers: Vec<&str> = cols.iter().map(|c| c.name().as_str()).collect();
        out.push_str(&headers.join("\t"));
        out.push('\n');

        for r in 0..rows {
            let vals: Vec<String> = cols
                .iter()
                .map(|c| {
                    let cname = c.name().as_str();
                    format_cell_val(
                        c.get(r).ok(),
                        cname,
                        col_formats.get(cname),
                        num_format,
                        global_decimals,
                    )
                })
                .collect();
            out.push_str(&vals.join("\t"));
            out.push('\n');
        }
    }

    out
}


/// What the clipboard gets (K4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyKind {
    /// Ctrl+C and "Copy": numbers in the unit the grid shows them in, as plain numbers (no thousands
    /// separators, no unit suffix, at most 15 significant digits), the unit in the header.
    Plain,
    /// "Copy as shown": the text of the grid's cells.
    AsShown,
    /// "Copy raw (BDT)": the stored numbers, unscaled.
    Raw,
}

/// How a decimal column is shown: in a money scale, as a percentage, or as it is.
enum ShownIn {
    Scale(MoneyScale),
    Percent { times_100: bool },
    Plain,
}

fn shown_in(col_name: &str, spec: Option<&ColumnFormatSpec>, global: NumberDisplayFormat) -> ShownIn {
    let t = spec.map(|s| s.fmt_type).unwrap_or(MetricFormatType::Auto);
    let resolved = if t == MetricFormatType::Auto { classify_column_auto_format(col_name) } else { t };
    match resolved {
        MetricFormatType::PercentAsIs => ShownIn::Percent { times_100: false },
        MetricFormatType::PercentDiv100 => ShownIn::Percent { times_100: true },
        MetricFormatType::Auto => global.scale().map(ShownIn::Scale).unwrap_or(ShownIn::Plain),
        other => other.scale().map(ShownIn::Scale).unwrap_or(ShownIn::Plain),
    }
}

/// A tab or line break inside a text would move the cells of a pasted table: it becomes a space.
fn clip_text(s: &str) -> String {
    s.chars().map(|c| if c == '\t' || c == '\n' || c == '\r' { ' ' } else { c }).collect()
}

/// The table as tab-separated text for the clipboard (the first 2,000 rows). A blank cell is an
/// empty cell in all three kinds. Decimal columns only are scaled; whole numbers, dates and text
/// are written as they are.
pub fn copy_table_text(
    df: &DataFrame,
    col_formats: &HashMap<String, ColumnFormatSpec>,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
    kind: CopyKind,
) -> String {
    use crate::engine::csv_float_text;
    let cols = df.get_columns();
    let rows = df.height().min(2000);
    let is_decimal = |c: &Series| matches!(c.dtype(), DataType::Float64 | DataType::Float32);
    // the header: the column's name, and for Plain the unit its numbers are in
    let header: Vec<String> = cols
        .iter()
        .map(|c| {
            let name = c.name().as_str();
            if !is_decimal(c) {
                return clip_text(name);
            }
            let unit = match (kind, shown_in(name, col_formats.get(name), num_format)) {
                (CopyKind::Plain, ShownIn::Scale(sc)) => Some(sc.unit),
                (CopyKind::Plain, ShownIn::Percent { .. }) => Some("%"),
                (CopyKind::Raw, ShownIn::Scale(_)) => Some("BDT"),
                _ => None,
            };
            match unit {
                Some(u) => clip_text(&format!("{} ({})", name, u)),
                None => clip_text(name),
            }
        })
        .collect();
    let mut out = header.join("\t");
    out.push('\n');
    for r in 0..rows {
        let cells: Vec<String> = cols
            .iter()
            .map(|c| {
                let name = c.name().as_str();
                let spec = col_formats.get(name);
                let v = c.get(r).unwrap_or(AnyValue::Null);
                let number = match &v {
                    AnyValue::Float64(x) => Some(*x),
                    AnyValue::Float32(x) => format!("{}", x).parse::<f64>().ok(),
                    _ => None,
                };
                match (&v, number) {
                    (AnyValue::Null, _) => String::new(),
                    (_, Some(x)) => match kind {
                        CopyKind::AsShown => format_metric_value(x, name, spec, num_format, global_decimals),
                        CopyKind::Raw => csv_float_text(x),
                        CopyKind::Plain => csv_float_text(match shown_in(name, spec, num_format) {
                            ShownIn::Scale(sc) => x / sc.divisor,
                            ShownIn::Percent { times_100: true } => x * 100.0,
                            _ => x,
                        }),
                    },
                    (AnyValue::Int64(x), _) if kind == CopyKind::AsShown && !TabState::is_likely_id_column(name) => format_with_commas(*x as f64, 0),
                    (AnyValue::String(t), _) => clip_text(t),
                    (other, _) => clip_text(&format!("{}", other)),
                }
            })
            .collect();
        out.push_str(&cells.join("\t"));
        out.push('\n');
    }
    out
}

fn format_cell_val(
    val: Option<AnyValue>,
    col_name: &str,
    spec: Option<&ColumnFormatSpec>,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
) -> String {
    match val {
        None | Some(AnyValue::Null) => "".to_string(),
        Some(AnyValue::Float64(v)) => {
            format_metric_value(v, col_name, spec, num_format, global_decimals)
        }
        Some(AnyValue::Float32(v)) => {
            format_metric_value(v as f64, col_name, spec, num_format, global_decimals)
        }
        Some(AnyValue::String(s)) => s.to_string(),
        Some(other) => format!("{}", other),
    }
}
