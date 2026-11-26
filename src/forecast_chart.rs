//! Phase 11 F2: a forecast drawn on a line chart. The chart's own calculation gives the months (its Month-Year categories, with a
//! Closing or Average balance time value, are the month-end or average-daily-balance roll-up); this module extends the series with
//! the forecast and its 80% and 95% bands. Every output that reads chart data (the screen, HTML, PDF, PPTX, Excel, the dashboard)
//! therefore shows the forecast without anything of its own.

use crate::charts::data::ChartData;
use crate::charts::spec::{ChartKind, ChartSpec};
use crate::forecast::*;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// The model a forecast chart uses.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModelPick {
    /// The model with the lowest back-test error.
    #[default]
    Auto,
    TrendSeasonal,
    HoltWinters,
    Growth,
    Arima,
}

impl ModelPick {
    pub const ALL: [ModelPick; 5] = [ModelPick::Auto, ModelPick::TrendSeasonal, ModelPick::HoltWinters, ModelPick::Growth, ModelPick::Arima];
    pub fn label(self) -> &'static str {
        match self {
            ModelPick::Auto => "Auto (best back-test)",
            ModelPick::TrendSeasonal => ModelKind::Stl.label(),
            ModelPick::HoltWinters => ModelKind::HoltWinters.label(),
            ModelPick::Growth => ModelKind::Growth.label(),
            ModelPick::Arima => ModelKind::Arima.label(),
        }
    }
    pub fn choice(self) -> Choice {
        match self {
            ModelPick::Auto => Choice::Auto,
            ModelPick::TrendSeasonal => Choice::Fixed(ModelKind::Stl),
            ModelPick::HoltWinters => Choice::Fixed(ModelKind::HoltWinters),
            ModelPick::Growth => Choice::Fixed(ModelKind::Growth),
            ModelPick::Arima => Choice::Fixed(ModelKind::Arima),
        }
    }
}

/// With several series: draw their sum (with bands) or each series on its own.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ForecastShow {
    #[default]
    Total,
    EachSeries,
}

/// A manual adjustment: add `amount` (in the chart's own unit, BDT for money) to a month, or from it onwards.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct OverlaySpec {
    pub year: i32,
    pub month: u32,
    pub amount: f64,
    pub from_here: bool,
    /// One series only (by name); blank for all.
    pub series: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ForecastSpec {
    /// Months ahead, 3 to 12.
    pub horizon: usize,
    pub model: ModelPick,
    pub clean_outliers: bool,
    pub show: ForecastShow,
    pub overlays: Vec<OverlaySpec>,
    /// A back-test error above this many % is called "not good".
    pub good_mape: f64,
}

impl Default for ForecastSpec {
    fn default() -> Self {
        ForecastSpec { horizon: 6, model: ModelPick::Auto, clean_outliers: true, show: ForecastShow::Total, overlays: Vec::new(), good_mape: 10.0 }
    }
}

pub fn parse_month(label: &str) -> Option<(i32, u32)> {
    let mut it = label.split_whitespace();
    let (m, y) = (it.next()?, it.next()?);
    if it.next().is_some() {
        return None;
    }
    let mi = MONTHS.iter().position(|x| x.eq_ignore_ascii_case(m))? as u32 + 1;
    Some((y.parse().ok()?, mi))
}

/// Extends a line chart's data with the forecast. The chart must be a line with Month-Year categories in date order and one measure.
pub fn apply_forecast(mut data: ChartData, fs: &ForecastSpec) -> Result<ChartData> {
    let spec: &ChartSpec = &data.spec;
    if spec.kind != ChartKind::Line {
        bail!("A forecast is drawn on a line chart. Change the chart to a line, with a Month-Year category.");
    }
    if !data.extra.is_empty() || !data.facets.is_empty() || data.size.is_some() {
        bail!("A forecast works on a chart with one measure and no small multiples. Remove the extra measures or the facet.");
    }
    if !(3..=12).contains(&fs.horizon) {
        bail!("Forecast 3 to 12 months ahead.");
    }
    if data.categories.len() < 6 {
        bail!("The chart has {} months; a forecast needs at least 6. Use a Month-Year category over a longer period.", data.categories.len());
    }
    let mut ym = Vec::new();
    for c in &data.categories {
        match parse_month(c) {
            Some(p) => ym.push(p),
            None => bail!("A forecast needs Month-Year categories (like \"Nov 2026\"); '{}' is not one. Use the Calendar \u{203a} Month-Year field as the category.", c),
        }
    }
    for w in ym.windows(2) {
        let (a, b) = (w[0].0 * 12 + w[0].1 as i32, w[1].0 * 12 + w[1].1 as i32);
        if b != a + 1 {
            bail!("The months are not one after the other ({} is followed by {}). Put the months in date order with none missing, or filter the chart to a run of months.", MONTHS[(w[0].1 - 1) as usize].to_string() + &format!(" {}", w[0].0), MONTHS[(w[1].1 - 1) as usize].to_string() + &format!(" {}", w[1].0));
        }
    }
    let n = ym.len();
    let series_names: Vec<String> = data.series.iter().map(|s| if s.is_empty() { "Actual".to_string() } else { s.clone() }).collect();
    let mut groups: Vec<(String, Monthly)> = Vec::new();
    for (si, name) in series_names.iter().enumerate() {
        let mut v = Vec::with_capacity(n);
        for (ci, x) in data.values[si].iter().enumerate().take(n) {
            match x {
                Some(x) => v.push(*x),
                None => bail!("'{}' has no value for {}. A forecast needs every month: fix the data or start the chart later.", name, data.categories[ci]),
            }
        }
        groups.push((name.clone(), Monthly::new(ym[0].0, ym[0].1, v)));
    }
    let settings = Settings { horizon: fs.horizon, choice: fs.model.choice(), clean_outliers: fs.clean_outliers, good_mape: fs.good_mape };
    let overlays: Vec<Overlay> = fs
        .overlays
        .iter()
        .map(|o| Overlay { year: o.year, month: o.month, amount: o.amount, from_here: o.from_here, group: if o.series.trim().is_empty() { None } else { Some(o.series.clone()) } })
        .collect();
    let gf = forecast_groups(&groups, &settings, &overlays);

    let mut notes: Vec<String> = Vec::new();
    let mut cats = data.categories.clone();
    let first_ahead = Monthly::new(ym[0].0, ym[0].1, Vec::new());
    for k in 0..fs.horizon {
        cats.push(first_ahead.label(n + k));
    }
    let width = n + fs.horizon;
    let (mut series, mut values): (Vec<String>, Vec<Vec<Option<f64>>>) = (Vec::new(), Vec::new());
    let history_of = |g: &Monthly| -> Vec<Option<f64>> {
        let mut row: Vec<Option<f64>> = g.values.iter().map(|v| Some(*v)).collect();
        row.resize(width, None);
        row
    };
    let ahead_row = |f: &SeriesForecast, pick: &dyn Fn(usize) -> f64, last: f64| -> Vec<Option<f64>> {
        let mut row: Vec<Option<f64>> = vec![None; width];
        row[n - 1] = Some(last);
        for k in 0..f.point.len() {
            row[n + k] = Some(pick(k));
        }
        row
    };
    match (fs.show, groups.len()) {
        (ForecastShow::EachSeries, g) if g > 1 => {
            for (i, (name, m)) in groups.iter().enumerate() {
                series.push(name.clone());
                values.push(history_of(m));
                match &gf.groups[i] {
                    Ok(f) => {
                        series.push(format!("{} forecast (estimate)", name));
                        values.push(ahead_row(f, &|k| f.point[k], m.values[n - 1]));
                        notes.push(format!("{}: {}.", name, summary(f)));
                    }
                    Err(e) => notes.push(format!("{}: no forecast. {}", name, e)),
                }
            }
            if let Err(e) = &gf.total {
                notes.push(e.clone());
            }
        }
        _ => {
            let total = match &gf.total {
                Ok(t) => t,
                Err(e) => bail!("{}", e),
            };
            let name = if groups.len() == 1 { groups[0].0.clone() } else { "Total".to_string() };
            let hist = Monthly::new(ym[0].0, ym[0].1, total.history.values.clone());
            series.push(name.clone());
            values.push(history_of(&hist));
            let last = hist.values[n - 1];
            series.push(format!("{} forecast (estimate)", name));
            values.push(ahead_row(total, &|k| total.point[k], last));
            for (label, i) in [("80% low", 0usize), ("80% high", 1), ("95% low", 2), ("95% high", 3)] {
                series.push(format!("{} (estimate)", label));
                values.push(ahead_row(total, &|k| total.bands[k][i], last));
            }
            if groups.len() == 1 {
                notes.push(format!("{}.", summary(total)));
                notes.extend(gf.groups[0].as_ref().ok().map(|f| f.notes.clone()).unwrap_or_default());
                if let Some(Ok(f)) = gf.groups.first() {
                    notes.extend(score_lines(f));
                }
            } else {
                notes.extend(total.notes.clone());
                for g in gf.groups.iter().flatten() {
                    notes.push(format!("{}: {}.", g.name, summary(g)));
                    notes.extend(score_lines(g));
                }
            }
        }
    }
    notes.insert(0, "Forecast: an estimate from past months, not a result. The bands are the range the model expects 8 times in 10 and 19 times in 20.".to_string());
    let rows: Vec<Vec<u64>> = values.iter().map(|_| vec![0u64; width]).collect();
    data.categories = cats.clone();
    data.kept_categories = cats;
    data.kept_series = series.clone();
    data.series = series;
    data.values = values;
    data.rows = rows;
    data.distinct_categories = width;
    data.folded_categories = 0;
    data.notes.extend(notes);
    Ok(data)
}

fn summary(f: &SeriesForecast) -> String {
    let months = f.point.len();
    format!("{} for {} months from {}{}", f.kind.label(), months, f.label_ahead(0), if f.not_good { " (does not back-test well)" } else { "" })
}

fn score_lines(f: &SeriesForecast) -> Vec<String> {
    let Some(k) = f.backtest_months else {
        return Vec::new();
    };
    let mut parts: Vec<String> = f
        .scores
        .iter()
        .map(|s| match &s.mape {
            Ok(m) => format!("{} {:.1}%", s.kind.label(), m),
            Err(_) => format!("{} not run", s.kind.label()),
        })
        .collect();
    if let Some(n) = f.naive_mape {
        parts.push(format!("same as last month {:.1}%", n));
    }
    let mut lines = vec![format!("Back-test on the last {} months (average error): {}.", k, parts.join(", "))];
    for s in &f.scores {
        if let Err(why) = &s.mape {
            lines.push(format!("{}: {}", s.kind.label(), why));
        }
    }
    lines.extend(f.notes.iter().filter(|n| n.starts_with("Unusual") || n.starts_with("No model")).cloned());
    lines
}

/// Phase 12: the active what-if scenario on a chart that has a forecast: the as-of month and every later month (the forecast and its
/// bands) change; the months before stay the actual numbers. Says so in the chart's notes.
pub fn apply_scenario(mut data: ChartData, w: &crate::whatif::WhatIfScope, role: crate::whatif::Role, measure: &str, as_of: chrono::NaiveDate) -> Result<ChartData> {
    use chrono::Datelike;
    let from = (as_of.year(), as_of.month());
    let mut changed = false;
    for (ci, c) in data.categories.clone().iter().enumerate() {
        let Some(ym) = parse_month(c) else {
            bail!("A scenario on a chart with a forecast needs Month-Year categories (like \"Nov 2026\"); '{}' is not one.", c);
        };
        if ym < from {
            continue;
        }
        for row in data.values.iter_mut() {
            if let Some(v) = row.get_mut(ci).and_then(|x| x.as_mut()) {
                *v = w.apply(role, *v);
                changed = true;
            }
        }
    }
    if changed {
        data.notes.push(w.note(&[measure]));
    }
    Ok(data)
}
