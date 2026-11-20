//! Charts in report packs (Phase 7 C6): an item of a pack may carry a chart, a saved chart or a
//! template. The chart is calculated from the item's table when the pack runs, written as a native
//! Excel chart on a sheet beside the item's, and (when the pack asks) as a page of an HTML chart
//! book beside the workbook. A chart that cannot be drawn stops the run with the item's name.

use super::calc::compute;
use super::data::ChartData;
use super::layout::{chart_title, LayoutOpts};
use super::library;
use super::templates::{prepare, Ctx, Template};
use crate::models::{resolved_scale, ColumnFormatSpec, MoneyScale, NumberDisplayFormat};
use anyhow::{anyhow, bail, Result};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// What chart an item carries.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackChart {
    /// A chart saved in `portable_data/charts`, drawn from the item's table (its columns must be in it).
    Saved { name: String },
    /// A banking template (its id, "gap-ladder"), for an item that is a tool's result.
    Template { template: String },
}

impl PackChart {
    /// How the item's row says it: "chart: Deposit mix" or "template: Gap ladder".
    pub fn label(&self) -> String {
        match self {
            PackChart::Saved { name } => format!("chart: {}", name),
            PackChart::Template { template } => format!("template: {}", Template::from_id(template).map(|t| t.name()).unwrap_or("unknown")),
        }
    }
}

/// One chart ready to write: its numbers and how they are written.
#[derive(Clone, Debug)]
pub struct ItemChart {
    pub data: ChartData,
    pub opts: LayoutOpts,
}

/// The sheet name of the item's chart: the item's sheet and " chart" (a second chart of the same item " chart 2").
pub fn chart_sheet_name(sheet: &str, n: usize) -> String {
    let suffix = if n == 0 { " chart".to_string() } else { format!(" chart {}", n + 1) };
    let keep = 31usize.saturating_sub(suffix.chars().count());
    let stem: String = sheet.chars().take(keep).collect();
    format!("{}{}", stem, suffix)
}

/// The longest item sheet name that leaves room for the workbook's "2_" numbering and " chart 2" (Excel allows 31 characters).
pub const MAX_CHARTED_SHEET_CHARS: usize = 20;

fn word(s: &MoneyScale) -> &'static str {
    match s.suffix {
        "Cr" => "Crore",
        "L" => "Lakh",
        "M" => "Million",
        "K" => "Thousand",
        _ => "",
    }
}

fn opts_for(scale: Option<MoneyScale>, decimals: usize) -> LayoutOpts {
    LayoutOpts { dark: false, divisor: scale.map(|s| s.divisor).unwrap_or(1.0), decimals, unit: scale.as_ref().map(word).unwrap_or("").to_string(), bare: false, colours: None }
}

/// The charts of an item: calculated over its whole table with the pack's money scale and decimals.
/// `charts_dir` is `portable_data/charts`.
///
/// `dq` is the (Green, Amber) pair of the data-quality rule set the item's scorecard run carries: a template that
/// needs it and has none stops with a message (no threshold is assumed).
pub fn build_item_charts(df: &DataFrame, chart: &PackChart, charts_dir: &Path, col_formats: &HashMap<String, ColumnFormatSpec>, scale: NumberDisplayFormat, decimals: usize, dq: Option<(f64, f64)>) -> Result<Vec<ItemChart>> {
    match chart {
        PackChart::Saved { name } => {
            let path = library::chart_path(charts_dir, name);
            if !path.is_file() {
                bail!("the saved chart '{}' is not in {}. Save it from the Chart tab, or choose another chart for the item.", name, charts_dir.display());
            }
            let (_, spec) = library::load_chart(&path)?;
            let columns: Vec<String> = df.get_column_names().iter().map(|n| n.to_string()).collect();
            let missing = library::missing_columns(&spec, &columns);
            if !missing.is_empty() {
                bail!("the saved chart '{}' needs the column(s) {}, which the item's table does not have. Its columns are: {}.", name, missing.join(", "), columns.join(", "));
            }
            let value_col = spec.value.clone().unwrap_or_default();
            let sc = match spec.scale.fixed() {
                Some(fixed) => fixed,
                None => resolved_scale(&value_col, col_formats.get(&value_col), scale),
            };
            let data = compute(df.clone().lazy(), &spec).map_err(|e| anyhow!("the saved chart '{}' could not be calculated: {:#}", name, e))?;
            Ok(vec![ItemChart { data, opts: opts_for(sc, decimals) }])
        }
        PackChart::Template { template } => {
            let t = Template::from_id(template).ok_or_else(|| anyhow!("the template '{}' is not one of the banking templates.", template))?;
            if t == Template::YieldCurves {
                bail!("the yield-curves template reads the curve store, not an item's table. Chart it from the Charts menu.");
            }
            let sc = scale.scale();
            let ctx = Ctx { divisor: sc.map(|s| s.divisor).unwrap_or(1.0), suffix: sc.map(|s| s.suffix.to_string()).unwrap_or_default(), decimals, dq_green: dq.map(|d| d.0), dq_amber: dq.map(|d| d.1) };
            let p = prepare(t, df, &ctx).map_err(|e| anyhow!("{:#}", e))?;
            let mut out = Vec::new();
            let mut all = vec![p.clone()];
            if let Some(c) = p.companion.clone() {
                all.push(*c);
            }
            for prep in all {
                let src = prep.table.clone().unwrap_or_else(|| df.clone());
                let data = compute(src.lazy(), &prep.spec).map_err(|e| anyhow!("the {} chart could not be calculated: {:#}", t.name(), e))?;
                out.push(ItemChart { data, opts: opts_for(sc, decimals) });
            }
            Ok(out)
        }
    }
}

/// A line for the Cover: the chart's title and form.
pub fn cover_detail(c: &ItemChart) -> String {
    format!("{} ({})", chart_title(&c.data), c.data.spec.kind.label())
}
