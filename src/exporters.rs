use crate::engine::{compute_control_totals, export_to_csv_stream, export_to_parquet_stream};
use crate::models::{
    format_metric_value, ColumnFormatSpec, NumberDisplayFormat, ParquetDialect, SplitExportFormat,
};
use anyhow::{Context, Result};
use polars::datatypes::AnyValue;
use polars::prelude::*;
use printpdf::*;
use rust_xlsxwriter::{Format, Workbook};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

const EXCEL_MAX_ROWS: usize = 1_048_500;
const PDF_MAX_ROWS_PER_TAB: usize = 5_000;

pub fn export_tabs_to_xlsx(tabs: Vec<(String, DataFrame)>, out_path: &Path) -> Result<()> {
    let mut workbook = Workbook::new();

    let header_fmt = Format::new()
        .set_bold()
        .set_background_color(rust_xlsxwriter::Color::RGB(0x1E293B))
        .set_font_color(rust_xlsxwriter::Color::White);

    let num_fmt = Format::new().set_num_format("#,##0.00");

    for (idx, (raw_title, df)) in tabs.into_iter().enumerate() {
        let worksheet = workbook.add_worksheet();
        let clean_name = sanitize_sheet_name(&raw_title, idx + 1);
        let _ = worksheet.set_name(&clean_name);

        let cols = df.get_columns();
        let row_limit = df.height().min(EXCEL_MAX_ROWS);

        for (c_idx, series) in cols.iter().enumerate() {
            worksheet.write_string_with_format(
                0,
                c_idx as u16,
                series.name().as_str(),
                &header_fmt,
            )?;
            worksheet.set_column_width(c_idx as u16, 18.0)?;
        }
        worksheet.set_freeze_panes(1, 0)?;

        for r_idx in 0..row_limit {
            let xls_row = (r_idx + 1) as u32;
            for (c_idx, series) in cols.iter().enumerate() {
                let xls_col = c_idx as u16;
                match series.get(r_idx) {
                    Ok(AnyValue::Null) => {}
                    Ok(AnyValue::Float64(v)) => {
                        worksheet.write_number_with_format(xls_row, xls_col, v, &num_fmt)?;
                    }
                    Ok(AnyValue::Float32(v)) => {
                        worksheet.write_number_with_format(xls_row, xls_col, v as f64, &num_fmt)?;
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
                    Ok(AnyValue::String(s)) => {
                        worksheet.write_string(xls_row, xls_col, s)?;
                    }
                    Ok(other) => {
                        worksheet.write_string(xls_row, xls_col, &format!("{}", other))?;
                    }
                    Err(_) => {}
                }
            }
        }
    }

    workbook
        .save(out_path)
        .context("Failed to save consolidated Excel workbook")?;
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

pub fn export_tabs_to_pdf(
    tabs: Vec<(String, DataFrame, HashMap<String, ColumnFormatSpec>)>,
    out_path: &Path,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
) -> Result<()> {
    let (doc, page1, layer1) = PdfDocument::new(
        "MicroParquet Banking & ALCO Report",
        Mm(297.0),
        Mm(210.0),
        "Layer 1",
    );

    let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold)?;
    let font_reg = doc.add_builtin_font(BuiltinFont::Helvetica)?;

    let mut current_page = page1;
    let mut current_layer = layer1;
    let mut first_section = true;

    for (title, df, col_formats) in tabs {
        if !first_section {
            let (p, l) = doc.add_page(Mm(297.0), Mm(210.0), "Layer");
            current_page = p;
            current_layer = l;
        }
        first_section = false;

        let cols = df.get_columns();
        let total_rows = df.height().min(PDF_MAX_ROWS_PER_TAB);
        let col_count = cols.len().max(1);
        let col_width_mm = (273.0 / col_count as f32).clamp(18.0, 55.0);

        let mut y_mm = 194.0f32;

        let layer_ref = doc.get_page(current_page).get_layer(current_layer);
        layer_ref.use_text(
            format!(
                "MicroParquet Report — {} (Scale: {})",
                title,
                num_format.label()
            ),
            12.0,
            Mm(12.0),
            Mm(y_mm),
            &font_bold,
        );
        y_mm -= 8.0;

        for (c_idx, series) in cols.iter().enumerate() {
            let x_mm = 12.0 + (c_idx as f32 * col_width_mm);
            let hdr: String = series.name().as_str().chars().take(18).collect();
            layer_ref.use_text(hdr, 8.5, Mm(x_mm), Mm(y_mm), &font_bold);
        }
        y_mm -= 5.5;

        for r_idx in 0..total_rows {
            if y_mm < 15.0 {
                let (p, l) = doc.add_page(Mm(297.0), Mm(210.0), "Layer");
                current_page = p;
                current_layer = l;
                y_mm = 194.0;

                let new_layer = doc.get_page(current_page).get_layer(current_layer);
                for (c_idx, series) in cols.iter().enumerate() {
                    let x_mm = 12.0 + (c_idx as f32 * col_width_mm);
                    let hdr: String = series.name().as_str().chars().take(18).collect();
                    new_layer.use_text(hdr, 8.5, Mm(x_mm), Mm(y_mm), &font_bold);
                }
                y_mm -= 5.5;
            }

            let active_layer = doc.get_page(current_page).get_layer(current_layer);
            for (c_idx, series) in cols.iter().enumerate() {
                let x_mm = 12.0 + (c_idx as f32 * col_width_mm);
                let cname = series.name().as_str();
                let spec = col_formats.get(cname);
                let cell_str = format_cell_val(
                    series.get(r_idx).ok(),
                    cname,
                    spec,
                    num_format,
                    global_decimals,
                );
                let truncated: String = cell_str.chars().take(20).collect();
                active_layer.use_text(truncated, 8.0, Mm(x_mm), Mm(y_mm), &font_reg);
            }
            y_mm -= 4.8;
        }
    }

    doc.save(&mut BufWriter::new(File::create(out_path)?))
        .context("Failed to write PDF file")?;
    Ok(())
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

    for (key, sub_lf) in groups {
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
        match format {
            SplitExportFormat::Parquet => export_to_parquet_stream(sub_lf, &p, dialect)?,
            SplitExportFormat::Csv => export_to_csv_stream(sub_lf, &p, mdy_dates)?,
            SplitExportFormat::Xlsx => {
                let sub_df = sub_lf.collect()?;
                export_tabs_to_xlsx(vec![(stem.clone(), sub_df)], &p)?;
            }
        }
        total_rows += rows;
        manifest_key.push(key);
        manifest_file.push(file_name);
        manifest_rows.push(rows as u64);
        manifest_sum.push(sum);
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
