//! The CL-1 summary (Phase 5 R5): loans by category and grade group, with the outstanding, the
//! base for provision and the provision required, row and column totals, and an Excel layout that
//! follows the template `portable_data/report_templates/cl1.json`.
//!
//! The template is a sample (the owner aligns it with the current CL-1 form). It names the loan
//! categories (rows), the grade groups (STD = STD-0 + STD-1 + STD-2, SMA, SS, DF, B/L), the
//! measures and the cell positions of the Excel sheet.

use crate::banking::{normalise_code, ProvisionConfig};
use crate::overdue::{assign_categories, load_category_map, GRADES};
use crate::problems::{row_keys, Problems};
use anyhow::{anyhow, bail, Context, Result};
use polars::prelude::*;
use serde::Deserialize;
use std::path::Path;

pub const TEMPLATE_FILE: &str = "cl1.json";
/// The shipped sample template (source copy: `assets/report_templates/cl1.json`).
pub const CL1_TEMPLATE: &str = include_str!("../assets/report_templates/cl1.json");

/// The measures a template may name, with the role each is read from.
pub const MEASURE_IDS: [&str; 3] = ["outstanding", "base", "provision"];

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Cl1Template {
    pub source: String,
    #[serde(default)]
    pub sample: bool,
    pub title: String,
    pub categories: Vec<String>,
    pub grade_groups: Vec<GradeGroup>,
    pub measures: Vec<Measure>,
    #[serde(default = "default_total")]
    pub total_label: String,
    #[serde(default)]
    pub layout: Cl1Layout,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct GradeGroup {
    pub label: String,
    pub grades: Vec<String>,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Measure {
    pub id: String,
    pub label: String,
}

/// Where things go on the Excel sheet (rows and columns count from 0).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Cl1Layout {
    pub title_row: u32,
    pub unit_row: u32,
    pub group_row: u32,
    pub measure_row: u32,
    pub first_data_row: u32,
    pub label_col: u16,
}

impl Default for Cl1Layout {
    fn default() -> Self {
        Self { title_row: 0, unit_row: 1, group_row: 3, measure_row: 4, first_data_row: 5, label_col: 0 }
    }
}

fn default_total() -> String {
    "Total".to_string()
}

impl Cl1Template {
    /// Reads and checks a template: every grade it names must be one of the seven, once; the
    /// measures must be known; the rows, groups and measures must not repeat or be empty.
    pub fn parse(text: &str, file: &str) -> Result<Cl1Template> {
        let t: Cl1Template = serde_json::from_str(text).map_err(|e| anyhow!("The template {} is not valid: {}.", file, e))?;
        if t.source.trim().is_empty() {
            bail!("The template {} has no \"source\" (the form it follows).", file);
        }
        if t.categories.is_empty() || t.grade_groups.is_empty() || t.measures.is_empty() {
            bail!("The template {} needs at least one loan category, one grade group and one measure.", file);
        }
        let mut seen_cat: Vec<String> = Vec::new();
        for c in &t.categories {
            let n = normalise_code(c);
            if n.is_empty() || seen_cat.contains(&n) {
                bail!("The template {} has a blank loan category, or '{}' twice.", file, c);
            }
            seen_cat.push(n);
        }
        let mut used: Vec<&str> = Vec::new();
        for g in &t.grade_groups {
            if g.label.trim().is_empty() || g.grades.is_empty() {
                bail!("The template {} has a grade group with no name or no grades.", file);
            }
            for gr in &g.grades {
                let n = normalise_code(gr);
                match GRADES.iter().find(|k| **k == n) {
                    None => bail!("The template {} names the grade '{}' in the group '{}', which is not one of {}.", file, gr, g.label, GRADES.join(", ")),
                    Some(k) if used.contains(k) => bail!("The template {} names the grade '{}' in more than one group.", file, gr),
                    Some(k) => used.push(k),
                }
            }
        }
        let mut ids: Vec<&str> = Vec::new();
        for m in &t.measures {
            if !MEASURE_IDS.contains(&m.id.as_str()) {
                bail!("The template {} names the measure '{}', which is not one of {}.", file, m.id, MEASURE_IDS.join(", "));
            }
            if ids.contains(&m.id.as_str()) || m.label.trim().is_empty() {
                bail!("The template {} has the measure '{}' twice, or a measure with no label.", file, m.id);
            }
            ids.push(&m.id);
        }
        let l = t.layout;
        let rows = [l.title_row, l.unit_row, l.group_row, l.measure_row, l.first_data_row];
        if !(rows[0] < rows[1] && rows[1] < rows[2] && rows[2] < rows[3] && rows[3] < rows[4]) {
            bail!("The template {}: the rows of the layout must run title, unit line, group header, measure header, first data row, each below the one before.", file);
        }
        Ok(t)
    }

    /// The sheet's columns after the label column: (group label or the total label, measure label).
    pub fn value_columns(&self) -> Vec<(String, String)> {
        let mut v = Vec::new();
        for g in &self.grade_groups {
            for m in &self.measures {
                v.push((g.label.clone(), m.label.clone()));
            }
        }
        for m in &self.measures {
            v.push((self.total_label.clone(), m.label.clone()));
        }
        v
    }

    /// The name of a column of the table, "group | measure" (the two-level header of the PDF layout).
    pub fn column_name(group: &str, measure: &str) -> String {
        format!("{} | {}", group, measure)
    }

    /// The group of a grade code, as an index in `grade_groups`.
    fn group_of(&self, grade: &str) -> Option<usize> {
        let n = normalise_code(grade);
        self.grade_groups.iter().position(|g| g.grades.iter().any(|x| normalise_code(x) == n))
    }
}

/// `portable_data/report_templates/cl1.json`: written with the shipped sample when missing (an
/// existing file is never overwritten), then read and checked.
pub fn load_template() -> Result<Cl1Template> {
    let path = crate::rules::data_dir().join("report_templates").join(TEMPLATE_FILE);
    crate::rules::write_text_if_missing(&path, CL1_TEMPLATE);
    let text = std::fs::read_to_string(&path).with_context(|| format!("Could not read the template {}", path.display()))?;
    Cl1Template::parse(&text, &format!("report_templates/{}", TEMPLATE_FILE))
}

/// The columns of the input tab.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Cl1Spec {
    /// The loan category column (through `loan_category_map.json`); none = `fixed_category` for every row.
    pub category_col: Option<String>,
    pub fixed_category: String,
    /// The grade column (`CL_Final` of "Classify by overdue", or the Phase 1 `CL_Grade`).
    pub grade_col: String,
    /// (measure id, column) for every measure of the template.
    pub measure_cols: Vec<(String, String)>,
}

/// The grade of a code, as one of the seven: a sub-grade as it is; a coarse code of
/// `cl_code_map.json` (STD, SMA, SS, DF, BL) as the first sub-grade of its class.
pub(crate) fn grade_of(code: &str, cl_map: &[(String, String)]) -> Option<&'static str> {
    let n = normalise_code(code);
    if let Some(g) = GRADES.iter().find(|g| **g == n) {
        return Some(g);
    }
    let coarse = cl_map.iter().find(|(k, _)| *k == n).map(|(_, g)| g.as_str())?;
    match coarse {
        "STD" => Some(GRADES[0]),
        "SMA" => Some("SMA"),
        "SS" => Some("SS"),
        "DF" => Some("DF"),
        "BL" => Some("B/L"),
        _ => None,
    }
}

/// The summary table of `df`: a row per loan category of the template and a total row; for each
/// grade group (and the total) the measures. Amounts in BDT.
pub fn build_cl1_frame(df: &DataFrame, spec: &Cl1Spec, template: &Cl1Template, category_map: &[(String, String)], cl_map: &[(String, String)]) -> Result<DataFrame> {
    let who = "CL-1 summary";
    let (keys, key_label) = row_keys(df);
    let height = df.height();
    let cats = assign_categories(df, spec.category_col.as_deref(), &spec.fixed_category, &template.categories, category_map, &format!("the template {}", TEMPLATE_FILE), who)?;
    // grades
    let gcol = spec.grade_col.trim();
    let g = df.column(gcol).map_err(|_| anyhow!("{}: the grade column '{}' was not found. Choose it again (CL_Final from Classify by overdue).", who, gcol))?;
    let g = g.cast(&DataType::String)?;
    let mut problems = Problems::default();
    let mut groups: Vec<usize> = vec![0; height];
    let mut unknown: Vec<String> = Vec::new();
    for (i, v) in g.str()?.into_iter().enumerate() {
        let code = v.unwrap_or("").trim();
        match grade_of(code, cl_map).and_then(|gr| template.group_of(gr)) {
            Some(p) => groups[i] = p,
            None => {
                let shown = if code.is_empty() { "(blank)".to_string() } else { format!("'{}'", code) };
                if !unknown.contains(&shown) && unknown.len() < 10 {
                    unknown.push(shown.clone());
                }
                problems.add(&format!("the grade in '{}' is not one of the grades of the template ({})", gcol, template.grade_groups.iter().flat_map(|g| g.grades.iter().map(String::as_str)).collect::<Vec<_>>().join(", ")), || keys[i].clone());
            }
        }
    }
    // amounts
    let mut amounts: Vec<(usize, Vec<f64>)> = Vec::new(); // (measure index, values)
    for (mi, m) in template.measures.iter().enumerate() {
        let col = spec.measure_cols.iter().find(|(id, _)| *id == m.id).map(|(_, c)| c.trim().to_string()).filter(|c| !c.is_empty());
        let Some(col) = col else {
            bail!("{}: choose the column of '{}'.", who, m.label);
        };
        let c = df.column(&col).map_err(|_| anyhow!("{}: the column '{}' for '{}' was not found. Choose it again.", who, col, m.label))?;
        if !c.dtype().is_numeric() {
            bail!("{}: the column '{}' for '{}' is {}, not a number. Set it to a number in the Schema & Ingestion settings.", who, col, m.label, c.dtype());
        }
        let c = c.cast(&DataType::Float64)?;
        let mut vals = vec![0.0; height];
        for (i, v) in c.f64()?.into_iter().enumerate() {
            match v.filter(|x| x.is_finite()) {
                Some(x) => vals[i] = x,
                None => problems.add(&format!("'{}' ({}) is blank or not a number", col, m.label), || keys[i].clone()),
            }
        }
        amounts.push((mi, vals));
    }
    problems.into_result(who, &key_label)?;

    // sums: [category][group][measure]
    let (nc, ng, nm) = (template.categories.len(), template.grade_groups.len(), template.measures.len());
    let mut sums = vec![vec![vec![0.0f64; nm]; ng]; nc];
    for i in 0..height {
        for (mi, vals) in &amounts {
            sums[cats[i]][groups[i]][*mi] += vals[i];
        }
    }
    let cols = template.value_columns();
    let mut data: Vec<Vec<f64>> = vec![Vec::new(); cols.len()];
    let mut labels: Vec<String> = Vec::new();
    let mut push_row = |label: String, cells: &[Vec<f64>]| {
        labels.push(label);
        let mut k = 0;
        for g in cells {
            for v in g {
                data[k].push(*v);
                k += 1;
            }
        }
        for mi in 0..nm {
            data[k + mi].push(cells.iter().map(|g| g[mi]).sum());
        }
    };
    for (ci, name) in template.categories.iter().enumerate() {
        push_row(name.clone(), &sums[ci]);
    }
    let column_totals: Vec<Vec<f64>> = (0..ng).map(|gi| (0..nm).map(|mi| (0..nc).map(|ci| sums[ci][gi][mi]).sum()).collect()).collect();
    push_row(template.total_label.clone(), &column_totals);
    let mut out = vec![Series::new("Loan category".into(), labels)];
    for ((g, m), v) in cols.iter().zip(data) {
        out.push(Series::new(Cl1Template::column_name(g, m).into(), v));
    }
    Ok(DataFrame::new(out)?)
}

/// The summary of a tab: reads the files of `portable_data` (the category map, `cl_code_map.json`).
pub fn cl1_lazy(lf: LazyFrame, spec: &Cl1Spec, template: &Cl1Template) -> Result<LazyFrame> {
    let dir = crate::rules::data_dir();
    let category_map = load_category_map(&dir)?;
    let cl_map = ProvisionConfig::load_or_create(&dir)?.cl_code_map;
    let df = lf.collect().context("The CL-1 summary could not read the table")?;
    Ok(build_cl1_frame(&df, spec, template, &category_map, &cl_map)?.lazy())
}

// --------------------------------------------------------------------------------- the Excel layout

fn unit_name(ms: Option<crate::models::MoneyScale>) -> &'static str {
    match ms.map(|m| m.suffix) {
        Some("Cr") => "BDT Crore",
        Some("L") => "BDT Lakh",
        Some("M") => "BDT Million",
        Some("K") => "BDT Thousand",
        _ => "BDT",
    }
}

/// The unit line under the title ("Figures in BDT Crore").
pub fn unit_line(num_format: crate::models::NumberDisplayFormat) -> String {
    format!("Figures in {}", unit_name(num_format.scale()))
}

/// The Excel column letters of a 0-based column.
pub fn col_letters(mut c: u32) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (c % 26) as u8) as char);
        if c < 26 {
            break;
        }
        c = c / 26 - 1;
    }
    s
}

/// `A1`-style reference of a 0-based row and column.
pub fn cell_ref(row: u32, col: u32) -> String {
    format!("{}{}", col_letters(col), row + 1)
}

/// Writes the summary table `df` (from [`build_cl1_frame`]) as one sheet laid out like the
/// template: the title, the unit line under it, a merged header for each grade group (and the
/// total), the measure labels under it, the category rows and the total row, at the positions the
/// template gives. Amounts are written in the chosen scale.
pub fn write_cl1_xlsx(df: &DataFrame, template: &Cl1Template, out_path: &Path, num_format: crate::models::NumberDisplayFormat, decimals: usize) -> Result<()> {
    use rust_xlsxwriter::{Format, FormatAlign, FormatBorder, Workbook};
    let l = template.layout;
    let label_col = l.label_col;
    let value_cols = template.value_columns();
    let nm = template.measures.len() as u16;
    let first_value_col = label_col + 1;
    let last_col = first_value_col + value_cols.len() as u16 - 1;
    let expected = 1 + value_cols.len();
    if df.width() != expected || df.height() != template.categories.len() + 1 {
        bail!("The CL-1 table has {} columns and {} rows, but the template {} gives {} and {}. Build the summary again.", df.width(), df.height(), TEMPLATE_FILE, expected, template.categories.len() + 1);
    }
    let ms = num_format.scale();
    let (divide, suffix) = match &ms {
        Some(m) => crate::exporters::xlsx_scaling(m),
        None => (1.0, ""),
    };
    let dec = if decimals == 0 { "#,##0".to_string() } else { format!("#,##0.{}", "0".repeat(decimals.min(6))) };
    let title_fmt = Format::new().set_bold().set_font_size(14);
    let unit_fmt = Format::new().set_italic();
    let head_fmt = Format::new().set_bold().set_align(FormatAlign::Center).set_align(FormatAlign::VerticalCenter).set_border(FormatBorder::Thin).set_background_color(rust_xlsxwriter::Color::RGB(0xE2E8F0));
    let label_fmt = Format::new().set_border(FormatBorder::Thin);
    let num_fmt = Format::new().set_num_format(format!("{}{}", dec, suffix)).set_border(FormatBorder::Thin);
    let total_label_fmt = Format::new().set_bold().set_border(FormatBorder::Thin);
    let total_num_fmt = Format::new().set_bold().set_num_format(format!("{}{}", dec, suffix)).set_border(FormatBorder::Thin);

    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("CL-1")?;
    ws.merge_range(l.title_row, label_col, l.title_row, last_col, &template.title, &title_fmt)?;
    ws.merge_range(l.unit_row, label_col, l.unit_row, last_col, &unit_line(num_format), &unit_fmt)?;
    // the label column header spans the group row and the measure row
    ws.merge_range(l.group_row, label_col, l.measure_row, label_col, "Loan category", &head_fmt)?;
    let mut c = first_value_col;
    let mut groups: Vec<&str> = template.grade_groups.iter().map(|g| g.label.as_str()).collect();
    groups.push(template.total_label.as_str());
    for g in groups {
        if nm == 1 {
            ws.write_string_with_format(l.group_row, c, g, &head_fmt)?;
        } else {
            ws.merge_range(l.group_row, c, l.group_row, c + nm - 1, g, &head_fmt)?;
        }
        for (mi, m) in template.measures.iter().enumerate() {
            ws.write_string_with_format(l.measure_row, c + mi as u16, &m.label, &head_fmt)?;
        }
        c += nm;
    }
    let last_row = df.height() - 1;
    for r in 0..df.height() {
        let total = r == last_row;
        let row = l.first_data_row + r as u32;
        let label = df.get_columns()[0].str()?.get(r).unwrap_or("").to_string();
        ws.write_string_with_format(row, label_col, &label, if total { &total_label_fmt } else { &label_fmt })?;
        for k in 0..value_cols.len() {
            let v = df.get_columns()[k + 1].f64()?.get(r).unwrap_or(0.0);
            ws.write_number_with_format(row, first_value_col + k as u16, v / divide, if total { &total_num_fmt } else { &num_fmt })?;
        }
    }
    ws.set_column_width(label_col, 26.0)?;
    for k in 0..value_cols.len() {
        ws.set_column_width(first_value_col + k as u16, 16.0)?;
    }
    if template.sample {
        let note = "Sample layout: the owner must align this sheet with the current CL-1 form before it goes into a return.";
        ws.write_string_with_format(l.first_data_row + df.height() as u32 + 1, label_col, note, &unit_fmt)?;
    }
    wb.save(out_path).with_context(|| format!("Could not write the Excel file {}", out_path.display()))?;
    Ok(())
}
