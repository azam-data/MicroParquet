//! Phase 11 F1: forecasting 3 to 12 months ahead, in plain Rust (no new crates).
//!
//! Everything works on a **monthly series** (month-end balances or average daily balances, rolled up from daily rows by
//! `roll_up_daily`). Three models, all with 80% and 95% bands:
//!
//! - **Trend + seasonality** (the default for deposits): the seasonal pattern of each calendar month is taken from a classical
//!   additive decomposition (centred 2x12 moving average, then the mean of each month's deviations, centred to sum to zero), the
//!   deseasonalised series gets a straight trend from its last 24 months, and the forecast is trend plus the month's seasonal. It
//!   needs 24 months. This is an STL-style model, not Cleveland's Loess STL (decision F-2 in `docs/phase11-decisions.md`).
//! - **Holt-Winters** (additive trend and additive seasonality, additive errors; period 12): smoothing parameters are found by
//!   minimising the squared one-step errors on a fixed grid and then a deterministic coordinate search. With fewer than 24 months
//!   it drops the seasonality and is Holt's linear trend (it says so).
//! - **Growth / run-off**: a steady monthly % growth (or run-off) fitted to the last 12 months (log-linear when every value is
//!   positive, straight line otherwise).
//!
//! The bands are model-based prediction intervals (normal errors). They are estimates: every result carries the word.
//! Reference results for the maths (statsmodels' OLS, ETS and seasonal_decompose) are pinned in `tests/phase11/f1.rs`.

use anyhow::{bail, Result};
use chrono::{Datelike, NaiveDate};
use std::collections::BTreeMap;

/// Normal quantiles of the 80% and 95% two-sided bands.
pub const Z80: f64 = 1.2815515655446004;
pub const Z95: f64 = 1.959963984540054;
/// The label every forecast carries.
pub const ESTIMATE: &str = "Estimate";
const M: usize = 12;

/// Months in a row: the first month and one value for each.
#[derive(Clone, Debug, PartialEq)]
pub struct Monthly {
    pub year: i32,
    pub month: u32,
    pub values: Vec<f64>,
}

impl Monthly {
    pub fn new(year: i32, month: u32, values: Vec<f64>) -> Self {
        Monthly { year, month, values }
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    /// (year, month) of position `i` (0 is the first month; a position past the end is a later month).
    pub fn ym(&self, i: usize) -> (i32, u32) {
        let total = self.year * 12 + (self.month as i32 - 1) + i as i32;
        (total.div_euclid(12), (total.rem_euclid(12) + 1) as u32)
    }
    pub fn label(&self, i: usize) -> String {
        let (y, m) = self.ym(i);
        format!("{} {}", MONTHS[(m - 1) as usize], y)
    }
    /// The calendar month (1 to 12) of position `i`.
    pub fn month_at(&self, i: usize) -> u32 {
        self.ym(i).1
    }
    /// Position of a (year, month) in this series (may be past the end), or `None` before the start.
    pub fn position_of(&self, year: i32, month: u32) -> Option<usize> {
        let d = (year * 12 + month as i32) - (self.year * 12 + self.month as i32);
        (d >= 0).then_some(d as usize)
    }
}

pub const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModelKind {
    /// Trend plus seasonality (STL-style).
    Stl,
    HoltWinters,
    Growth,
}

impl ModelKind {
    pub fn all() -> [ModelKind; 3] {
        [ModelKind::Stl, ModelKind::HoltWinters, ModelKind::Growth]
    }
    pub fn label(self) -> &'static str {
        match self {
            ModelKind::Stl => "Trend + seasonality",
            ModelKind::HoltWinters => "Holt-Winters",
            ModelKind::Growth => "Growth / run-off",
        }
    }
}

/// Which model to use: one of them, or the one with the lowest back-test error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Auto,
    Fixed(ModelKind),
}

/// What one model gives for the months ahead.
#[derive(Clone, Debug, PartialEq)]
pub struct Raw {
    pub point: Vec<f64>,
    /// [lower 80, upper 80, lower 95, upper 95] for each month ahead.
    pub bands: Vec<[f64; 4]>,
    pub notes: Vec<String>,
}

fn symmetric(point: &[f64], sd: &[f64]) -> Vec<[f64; 4]> {
    point.iter().zip(sd).map(|(p, s)| [p - Z80 * s, p + Z80 * s, p - Z95 * s, p + Z95 * s]).collect()
}

/// Like `symmetric`, with Student's t quantiles for `df` degrees of freedom (a straight line through few months).
fn symmetric_t(point: &[f64], sd: &[f64], df: f64) -> Vec<[f64; 4]> {
    let (t80, t95) = (t_quantile(0.9, df), t_quantile(0.975, df));
    point.iter().zip(sd).map(|(p, s)| [p - t80 * s, p + t80 * s, p - t95 * s, p + t95 * s]).collect()
}

fn ln_gamma(x: f64) -> f64 {
    // Lanczos approximation (g = 7, n = 9)
    const C: [f64; 9] = [0.99999999999980993, 676.5203681218851, -1259.1392167224028, 771.32342877765313, -176.61502916214059, 12.507343278686905, -0.13857109526572012, 9.9843695780195716e-6, 1.5056327351493116e-7];
    if x < 0.5 {
        return (std::f64::consts::PI / (std::f64::consts::PI * x).sin()).ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut a = C[0];
    let t = x + 7.5;
    for (i, c) in C.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

/// The regularised incomplete beta function (continued fraction, Numerical Recipes).
fn inc_beta(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let bt = (ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (1.0 - x).ln()).exp();
    let cf = |a: f64, b: f64, x: f64| -> f64 {
        let (qab, qap, qam) = (a + b, a + 1.0, a - 1.0);
        let tiny = 1e-300;
        let mut c = 1.0;
        let mut d = 1.0 - qab * x / qap;
        if d.abs() < tiny {
            d = tiny;
        }
        d = 1.0 / d;
        let mut h = d;
        for m in 1..300 {
            let m = m as f64;
            let m2 = 2.0 * m;
            let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
            d = 1.0 + aa * d;
            if d.abs() < tiny {
                d = tiny;
            }
            c = 1.0 + aa / c;
            if c.abs() < tiny {
                c = tiny;
            }
            d = 1.0 / d;
            h *= d * c;
            let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
            d = 1.0 + aa * d;
            if d.abs() < tiny {
                d = tiny;
            }
            c = 1.0 + aa / c;
            if c.abs() < tiny {
                c = tiny;
            }
            d = 1.0 / d;
            let del = d * c;
            h *= del;
            if (del - 1.0).abs() < 1e-15 {
                break;
            }
        }
        h
    };
    if x < (a + 1.0) / (a + b + 2.0) {
        bt * cf(a, b, x) / a
    } else {
        1.0 - bt * cf(b, a, 1.0 - x) / b
    }
}

/// The cumulative distribution of Student's t with `df` degrees of freedom.
pub fn t_cdf(t: f64, df: f64) -> f64 {
    let x = df / (df + t * t);
    let tail = 0.5 * inc_beta(df / 2.0, 0.5, x);
    if t >= 0.0 {
        1.0 - tail
    } else {
        tail
    }
}

/// The quantile of Student's t (`p` between 0.5 and 1), found by bisection on the distribution function.
pub fn t_quantile(p: f64, df: f64) -> f64 {
    let (mut lo, mut hi) = (0.0f64, 1e4);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if t_cdf(mid, df) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

fn check_finite(y: &[f64]) -> Result<()> {
    if let Some(i) = y.iter().position(|v| !v.is_finite()) {
        bail!("Month {} of the series is not a number. Fix the data before forecasting.", i + 1);
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------ least squares

/// A straight line fitted by least squares through (x, y).
#[derive(Clone, Copy, Debug)]
pub struct Line {
    pub intercept: f64,
    pub slope: f64,
    pub sse: f64,
    pub n: usize,
    pub xbar: f64,
    pub sxx: f64,
}

impl Line {
    pub fn fit(x: &[f64], y: &[f64]) -> Result<Line> {
        let n = x.len();
        if n < 3 || n != y.len() {
            bail!("A trend needs at least 3 months.");
        }
        let nf = n as f64;
        let xbar = x.iter().sum::<f64>() / nf;
        let ybar = y.iter().sum::<f64>() / nf;
        let sxx: f64 = x.iter().map(|v| (v - xbar) * (v - xbar)).sum();
        if sxx == 0.0 {
            bail!("A trend needs months that differ.");
        }
        let sxy: f64 = x.iter().zip(y).map(|(a, b)| (a - xbar) * (b - ybar)).sum();
        let slope = sxy / sxx;
        let intercept = ybar - slope * xbar;
        let sse: f64 = x.iter().zip(y).map(|(a, b)| (b - intercept - slope * a).powi(2)).sum();
        Ok(Line { intercept, slope, sse, n, xbar, sxx })
    }
    pub fn at(&self, x: f64) -> f64 {
        self.intercept + self.slope * x
    }
    /// The standard error of a new observation at `x` (prediction interval of ordinary least squares).
    pub fn pred_se(&self, x: f64) -> f64 {
        let s2 = self.sse / (self.n as f64 - 2.0);
        (s2 * (1.0 + 1.0 / self.n as f64 + (x - self.xbar).powi(2) / self.sxx)).sqrt()
    }
}

// ------------------------------------------------------------------------------------------------ growth / run-off

/// Growth or run-off: the last `window` months (12 at most) on a straight line, in logs when every value is positive.
pub fn growth(y: &[f64], h: usize, window: usize) -> Result<Raw> {
    check_finite(y)?;
    if y.len() < 3 {
        bail!("Growth / run-off needs at least 3 months of history.");
    }
    let w = window.min(y.len());
    let tail = &y[y.len() - w..];
    let x: Vec<f64> = (0..w).map(|i| i as f64).collect();
    let positive = tail.iter().all(|v| *v > 0.0);
    let mut notes = Vec::new();
    if positive {
        let ly: Vec<f64> = tail.iter().map(|v| v.ln()).collect();
        let line = Line::fit(&x, &ly)?;
        let g = line.slope.exp() - 1.0;
        notes.push(format!("{} of {:.2}% a month, fitted to the last {} months.", if g < 0.0 { "Run-off" } else { "Growth" }, g * 100.0, w));
        let mut point = Vec::new();
        let mut bands = Vec::new();
        let (t80, t95) = (t_quantile(0.9, w as f64 - 2.0), t_quantile(0.975, w as f64 - 2.0));
        for k in 1..=h {
            let xk = (w - 1 + k) as f64;
            let mu = line.at(xk);
            let se = line.pred_se(xk);
            point.push(mu.exp());
            bands.push([(mu - t80 * se).exp(), (mu + t80 * se).exp(), (mu - t95 * se).exp(), (mu + t95 * se).exp()]);
        }
        Ok(Raw { point, bands, notes })
    } else {
        let line = Line::fit(&x, tail)?;
        notes.push(format!("A straight line of {:+.0} a month, fitted to the last {} months (the series has zero or negative values, so no % rate).", line.slope, w));
        let mut point = Vec::new();
        let mut sd = Vec::new();
        for k in 1..=h {
            let xk = (w - 1 + k) as f64;
            point.push(line.at(xk));
            sd.push(line.pred_se(xk));
        }
        let bands = symmetric_t(&point, &sd, w as f64 - 2.0);
        Ok(Raw { point, bands, notes })
    }
}

// ------------------------------------------------------------------------------------------------ trend + seasonality

/// The seasonal pattern of a series by a classical additive decomposition: a centred 2x12 moving average for the trend, the mean of
/// each calendar month's deviations from it, centred so the twelve add to zero. `first_month` is the calendar month (1 to 12) of
/// `y[0]`. Returns the 12 values indexed by calendar month - 1. Needs at least 24 months.
pub fn seasonal_indices(y: &[f64], first_month: u32) -> Result<[f64; 12]> {
    seasonal_indices_with(y, first_month, false)
}

/// As `seasonal_indices`; with `robust` each calendar month takes the median of its deviations instead of the mean, so one spike does
/// not move the pattern (used to find unusual months).
pub fn seasonal_indices_with(y: &[f64], first_month: u32, robust: bool) -> Result<[f64; 12]> {
    check_finite(y)?;
    let n = y.len();
    if n < 2 * M {
        bail!("Seasonality needs at least 24 months of history; this series has {}.", n);
    }
    let mut sums = [0.0f64; 12];
    let mut counts = [0usize; 12];
    let mut devs: Vec<Vec<f64>> = vec![Vec::new(); 12];
    for t in 6..n - 6 {
        let mut ma = 0.5 * y[t - 6] + 0.5 * y[t + 6];
        for v in &y[t - 5..=t + 5] {
            ma += v;
        }
        ma /= 12.0;
        let mi = ((first_month as usize - 1) + t) % 12;
        sums[mi] += y[t] - ma;
        counts[mi] += 1;
        devs[mi].push(y[t] - ma);
    }
    let mut idx = [0.0f64; 12];
    for i in 0..12 {
        if counts[i] == 0 {
            bail!("A calendar month has no value left after the moving average. Give the model more months.");
        }
        idx[i] = if robust { median(&devs[i]) } else { sums[i] / counts[i] as f64 };
    }
    let mean = idx.iter().sum::<f64>() / 12.0;
    for v in idx.iter_mut() {
        *v -= mean;
    }
    Ok(idx)
}

/// Trend plus seasonality: the seasonal indices above, then a straight trend through the last 24 deseasonalised months.
pub fn trend_seasonal(y: &[f64], first_month: u32, h: usize) -> Result<Raw> {
    let n = y.len();
    let idx = seasonal_indices(y, first_month)?;
    let month_of = |t: usize| ((first_month as usize - 1) + t) % 12;
    let d: Vec<f64> = (0..n).map(|t| y[t] - idx[month_of(t)]).collect();
    let l = n.min(2 * M);
    let x: Vec<f64> = (0..l).map(|i| i as f64).collect();
    let line = Line::fit(&x, &d[n - l..])?;
    let years = (n as f64 / 12.0).floor().max(1.0);
    let mut point = Vec::new();
    let mut sd = Vec::new();
    for k in 1..=h {
        let xk = (l - 1 + k) as f64;
        point.push(line.at(xk) + idx[month_of(n - 1 + k)]);
        // the trend's own prediction error plus the uncertainty of a seasonal index estimated from `years` years
        let s2 = line.sse / (line.n as f64 - 2.0);
        sd.push((line.pred_se(xk).powi(2) + s2 / years).sqrt());
    }
    let bands = symmetric_t(&point, &sd, l as f64 - 2.0);
    let notes = vec![format!("Trend of {:+.0} a month on the deseasonalised series (last {} months), plus the usual pattern of each calendar month.", line.slope, l)];
    Ok(Raw { point, bands, notes })
}

// ------------------------------------------------------------------------------------------------ Holt-Winters (ETS A,A,A)

/// The result of running the smoothing recursion over a series with fixed parameters.
#[derive(Clone, Debug)]
pub struct EtsRun {
    pub sse: f64,
    /// Forecasts for the months ahead.
    pub point: Vec<f64>,
    /// 1 + the sum of c_j squared for each month ahead: the variance of the forecast error in units of the error variance.
    pub var_mult: Vec<f64>,
}

/// Additive error, additive trend, additive seasonality (Hyndman's ETS(A,A,A) form: l = l + b + a*e, b = b + beta*e, s = s + gamma*e,
/// with the one-step error e). `seasonal` is `Some(initial seasonals)` (12 values, oldest first) or `None` for Holt's trend only.
pub fn ets_run(y: &[f64], alpha: f64, beta: f64, gamma: f64, l0: f64, b0: f64, seasonal: Option<&[f64]>, h: usize) -> EtsRun {
    let mut l = l0;
    let mut b = b0;
    let mut s: Vec<f64> = seasonal.map(|s| s.to_vec()).unwrap_or_default();
    let m = s.len();
    let mut sse = 0.0;
    for (t, yt) in y.iter().enumerate() {
        let st = if m > 0 { s[t % m] } else { 0.0 };
        let e = yt - (l + b + st);
        sse += e * e;
        l += b + alpha * e;
        b += beta * e;
        if m > 0 {
            s[t % m] = st + gamma * e;
        }
    }
    let n = y.len();
    let mut point = Vec::new();
    let mut var_mult = Vec::new();
    for k in 1..=h {
        let st = if m > 0 { s[(n + k - 1) % m] } else { 0.0 };
        point.push(l + k as f64 * b + st);
        let mut v = 1.0;
        for j in 1..k {
            let dj = if m > 0 && j % m == 0 { gamma } else { 0.0 };
            let c = alpha + j as f64 * beta + dj;
            v += c * c;
        }
        var_mult.push(v);
    }
    EtsRun { sse, point, var_mult }
}

/// Starting level, trend and seasonals of Holt-Winters: the level is the mean of the first year, the trend the change between the
/// first two years' means over 12, and each seasonal the first year's deviation from the level. `first_month` rotates nothing:
/// the seasonals are in the order of the series (position t uses `s[t % 12]`).
pub fn hw_start(y: &[f64]) -> (f64, f64, Vec<f64>) {
    let m1 = y[..M].iter().sum::<f64>() / 12.0;
    let m2 = y[M..2 * M].iter().sum::<f64>() / 12.0;
    let b0 = (m2 - m1) / 12.0;
    // the level before the first month: the first year's mean, less half a year of trend, so month 1 is predicted from the start
    let l0 = m1 - b0 * 5.5 - b0;
    let s0: Vec<f64> = (0..M).map(|i| y[i] - (m1 + b0 * (i as f64 - 5.5))).collect();
    (l0, b0, s0)
}

/// Holt's linear trend start: a line through the first up-to-6 months.
pub fn holt_start(y: &[f64]) -> Result<(f64, f64)> {
    let k = y.len().min(6);
    let x: Vec<f64> = (0..k).map(|i| i as f64).collect();
    let line = Line::fit(&x, &y[..k])?;
    Ok((line.intercept - line.slope, line.slope))
}

/// Finds alpha, beta and gamma (0 <= beta <= alpha, 0 <= gamma <= 1 - alpha) that minimise the squared one-step errors: a fixed
/// grid, then a coordinate search with a shrinking step. The same series always gives the same parameters.
pub fn ets_fit(y: &[f64], l0: f64, b0: f64, seasonal: Option<&[f64]>) -> (f64, f64, f64, f64) {
    let cost = |a: f64, b: f64, g: f64| ets_run(y, a, b, g, l0, b0, seasonal, 0).sse;
    let admissible = |a: f64, b: f64, g: f64| a > 0.001 && a < 0.999 && b >= 0.0 && b <= a && g >= 0.0 && g <= 1.0 - a;
    let fr = [0.0, 0.05, 0.1, 0.2, 0.4, 0.6, 0.8, 1.0];
    let mut best = (f64::INFINITY, 0.5, 0.0, 0.0);
    for a in [0.01, 0.05, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.98] {
        for bf in fr {
            for gf in if seasonal.is_some() { fr.as_slice() } else { &[0.0][..] } {
                let (b, g) = (a * bf, (1.0 - a) * gf);
                let c = cost(a, b, g);
                if c < best.0 {
                    best = (c, a, b, g);
                }
            }
        }
    }
    let (mut c, mut a, mut b, mut g) = best;
    let mut step = 0.05;
    while step > 0.0005 {
        let mut moved = true;
        while moved {
            moved = false;
            for which in 0..3 {
                if which == 2 && seasonal.is_none() {
                    continue;
                }
                for dir in [-1.0, 1.0] {
                    let (mut na, mut nb, mut ng) = (a, b, g);
                    match which {
                        0 => na += dir * step,
                        1 => nb += dir * step,
                        _ => ng += dir * step,
                    }
                    if admissible(na, nb, ng) {
                        let nc = cost(na, nb, ng);
                        if nc < c - 1e-12 * c.abs().max(1.0) {
                            (c, a, b, g) = (nc, na, nb, ng);
                            moved = true;
                        }
                    }
                }
            }
        }
        step /= 2.0;
    }
    (c, a, b, g)
}

/// Holt-Winters forecast: additive trend and seasonality with 24 months or more, Holt's linear trend with 6 to 23.
pub fn holt_winters(y: &[f64], h: usize) -> Result<Raw> {
    check_finite(y)?;
    let n = y.len();
    let mut notes = Vec::new();
    let (run, params, npar) = if n >= 2 * M {
        let (l0, b0, s0) = hw_start(y);
        let (sse, a, b, g) = ets_fit(y, l0, b0, Some(&s0));
        let _ = sse;
        notes.push(format!("Holt-Winters with smoothing level {:.3}, trend {:.3}, seasonal {:.3} (fitted).", a, b, g));
        (ets_run(y, a, b, g, l0, b0, Some(&s0), h), (a, b, g), 3 + 2 + M)
    } else if n >= 6 {
        let (l0, b0) = holt_start(y)?;
        let (sse, a, b, _) = ets_fit(y, l0, b0, None);
        let _ = sse;
        notes.push(format!("Fewer than 24 months, so no seasonality: Holt's linear trend (level {:.3}, trend {:.3}).", a, b));
        (ets_run(y, a, b, 0.0, l0, b0, None, h), (a, b, 0.0), 2 + 2)
    } else {
        bail!("Holt-Winters needs at least 6 months of history; this series has {}.", n);
    };
    let _ = params;
    if n <= npar + 3 {
        bail!("Holt-Winters has {} months to estimate {} numbers: too few. Use Growth / run-off or give it more history.", n, npar);
    }
    let sigma2 = run.sse / (n - npar) as f64;
    let sd: Vec<f64> = run.var_mult.iter().map(|v| (sigma2 * v).sqrt()).collect();
    let bands = symmetric(&run.point, &sd);
    Ok(Raw { point: run.point, bands, notes })
}

// ------------------------------------------------------------------------------------------------ outliers

/// Positions whose value is far from what its neighbours and its calendar month say: the remainder after a centred 5-month running
/// median (and the seasonal pattern when there are 24 months) with a modified z-score (0.6745 x deviation / median absolute
/// deviation) above 3.5 and the month at least 5% of the typical level away from its neighbours.
pub fn flag_outliers(y: &[f64], first_month: u32) -> Vec<usize> {
    let n = y.len();
    if n < 8 || y.iter().any(|v| !v.is_finite()) {
        return Vec::new();
    }
    let seasonal = if n >= 2 * M { seasonal_indices_with(y, first_month, true).ok() } else { None };
    let month_of = |t: usize| ((first_month as usize - 1) + t) % 12;
    let adj: Vec<f64> = (0..n).map(|t| y[t] - seasonal.map_or(0.0, |s| s[month_of(t)])).collect();
    let rem: Vec<f64> = (0..n)
        .map(|t| {
            let lo = t.saturating_sub(2);
            let hi = (t + 2).min(n - 1);
            adj[t] - median(&adj[lo..=hi])
        })
        .collect();
    let med = median(&rem);
    let mad = median(&rem.iter().map(|r| (r - med).abs()).collect::<Vec<_>>());
    if mad == 0.0 {
        return Vec::new();
    }
    // a month is unusual only when it is also materially off: 5% of the series' typical level (a very smooth series has a tiny MAD)
    let level = median(&adj.iter().map(|v| v.abs()).collect::<Vec<_>>());
    (0..n).filter(|t| (0.6745 * (rem[*t] - med).abs() / mad) > 3.5 && (rem[*t] - med).abs() > 0.05 * level).collect()
}

/// The series with flagged months replaced by what their neighbours and calendar month say (`y - remainder`).
pub fn clean_outliers(y: &[f64], first_month: u32, flagged: &[usize]) -> Vec<f64> {
    let n = y.len();
    let seasonal = if n >= 2 * M { seasonal_indices_with(y, first_month, true).ok() } else { None };
    let month_of = |t: usize| ((first_month as usize - 1) + t) % 12;
    let adj: Vec<f64> = (0..n).map(|t| y[t] - seasonal.map_or(0.0, |s| s[month_of(t)])).collect();
    let mut out = y.to_vec();
    for &t in flagged {
        let lo = t.saturating_sub(2);
        let hi = (t + 2).min(n - 1);
        // the median of the neighbours without the point itself
        let others: Vec<f64> = (lo..=hi).filter(|j| *j != t && !flagged.contains(j)).map(|j| adj[j]).collect();
        if !others.is_empty() {
            out[t] = median(&others) + seasonal.map_or(0.0, |s| s[month_of(t)]);
        }
    }
    out
}

fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = s.len();
    if n == 0 {
        0.0
    } else if n % 2 == 1 {
        s[n / 2]
    } else {
        0.5 * (s[n / 2 - 1] + s[n / 2])
    }
}

// ------------------------------------------------------------------------------------------------ one model, back-test, auto

fn run_model(kind: ModelKind, y: &[f64], first_month: u32, h: usize) -> Result<Raw> {
    match kind {
        ModelKind::Stl => trend_seasonal(y, first_month, h),
        ModelKind::HoltWinters => holt_winters(y, h),
        ModelKind::Growth => growth(y, h, 12),
    }
}

/// How one model did when the last `months` of the series were hidden from it.
#[derive(Clone, Debug, PartialEq)]
pub struct Score {
    pub kind: ModelKind,
    /// The mean absolute % error over the hidden months (months whose actual value is 0 are left out), or why it could not run.
    pub mape: std::result::Result<f64, String>,
}

/// The months hidden for the back-test: 12 with 36 months or more of history, else 6 with 18 or more, else none.
pub fn backtest_months(n: usize) -> Option<usize> {
    if n >= 36 {
        Some(12)
    } else if n >= 18 {
        Some(6)
    } else {
        None
    }
}

pub fn mape(actual: &[f64], forecast: &[f64]) -> Option<f64> {
    let mut sum = 0.0;
    let mut k = 0;
    for (a, f) in actual.iter().zip(forecast) {
        if *a != 0.0 {
            sum += ((a - f) / a).abs();
            k += 1;
        }
    }
    (k > 0).then(|| 100.0 * sum / k as f64)
}

/// Back-tests every model (and the "same as last month" benchmark, which is not a choice) on the last `k` months.
pub fn backtest(y: &[f64], first_month: u32, k: usize) -> (Vec<Score>, Option<f64>) {
    let train = &y[..y.len() - k];
    let test = &y[y.len() - k..];
    let scores = ModelKind::all()
        .iter()
        .map(|&kind| {
            let mape = run_model(kind, train, first_month, k).map_err(|e| e.to_string()).and_then(|r| mape_of(test, &r.point));
            Score { kind, mape }
        })
        .collect();
    let naive = mape(test, &vec![train[train.len() - 1]; k]);
    (scores, naive)
}

fn mape_of(test: &[f64], f: &[f64]) -> std::result::Result<f64, String> {
    mape(test, f).ok_or_else(|| "Every hidden month is 0, so a % error cannot be worked out.".to_string())
}

/// What a forecast was asked to do.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub horizon: usize,
    pub choice: Choice,
    /// Replace flagged months by what their neighbours say before fitting.
    pub clean_outliers: bool,
    /// A back-test error above this (%) is "not good": the forecast is shown but marked.
    pub good_mape: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { horizon: 12, choice: Choice::Auto, clean_outliers: true, good_mape: 10.0 }
    }
}

/// A manual adjustment of the forecast: add `amount` to the month's value, and to every later month too when `from_here`.
#[derive(Clone, Debug, PartialEq)]
pub struct Overlay {
    pub year: i32,
    pub month: u32,
    pub amount: f64,
    pub from_here: bool,
    /// Applies to this group only; `None` for every group (or the single series).
    pub group: Option<String>,
}

/// The forecast of one series.
#[derive(Clone, Debug, PartialEq)]
pub struct SeriesForecast {
    pub name: String,
    pub history: Monthly,
    /// Months flagged as outliers (positions in `history`).
    pub outliers: Vec<usize>,
    pub kind: ModelKind,
    pub scores: Vec<Score>,
    pub naive_mape: Option<f64>,
    pub backtest_months: Option<usize>,
    /// The first month ahead, then `point.len()` months.
    pub first_ahead: (i32, u32),
    pub point: Vec<f64>,
    pub bands: Vec<[f64; 4]>,
    pub notes: Vec<String>,
    /// The chosen model's back-test error was above the "good" limit, or could not be worked out.
    pub not_good: bool,
}

impl SeriesForecast {
    pub fn label_ahead(&self, k: usize) -> String {
        let m = Monthly::new(self.first_ahead.0, self.first_ahead.1, Vec::new());
        m.label(k)
    }
}

pub fn forecast_series(name: &str, history: &Monthly, s: &Settings) -> Result<SeriesForecast> {
    if s.horizon == 0 || s.horizon > 12 {
        bail!("Forecast 1 to 12 months ahead.");
    }
    let y = &history.values;
    check_finite(y)?;
    if y.len() < 6 {
        bail!("'{}' has {} months of history; a forecast needs at least 6.", name, y.len());
    }
    let fm = history.month;
    let outliers = flag_outliers(y, fm);
    let fit_y = if s.clean_outliers && !outliers.is_empty() { clean_outliers(y, fm, &outliers) } else { y.clone() };
    let mut notes = Vec::new();
    if !outliers.is_empty() {
        let list: Vec<String> = outliers.iter().map(|&i| history.label(i)).collect();
        notes.push(format!(
            "Unusual month{}: {}. {}",
            if outliers.len() == 1 { "" } else { "s" },
            list.join(", "),
            if s.clean_outliers { "Replaced by what the neighbouring months say before fitting." } else { "Kept in the fit." }
        ));
    }
    let k = backtest_months(fit_y.len());
    let (scores, naive, kind, not_good) = match (s.choice, k) {
        (Choice::Fixed(kind), Some(k)) => {
            let (sc, nv) = backtest(&fit_y, fm, k);
            let own = sc.iter().find(|x| x.kind == kind).and_then(|x| x.mape.clone().ok());
            let bad = own.is_none_or(|m| m > s.good_mape);
            (sc, nv, kind, bad)
        }
        (Choice::Fixed(kind), None) => {
            notes.push("Too little history to back-test (18 months needed).".to_string());
            (Vec::new(), None, kind, true)
        }
        (Choice::Auto, Some(k)) => {
            let (sc, nv) = backtest(&fit_y, fm, k);
            let best = sc.iter().filter_map(|x| x.mape.clone().ok().map(|m| (x.kind, m))).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            match best {
                Some((kind, m)) => {
                    let bad = m > s.good_mape;
                    if bad {
                        notes.push(format!("No model back-tests well: the best ({}) was off by {:.1}% on average over the last {} months, more than {:.0}%. Treat this forecast as a rough guide.", kind.label(), m, k, s.good_mape));
                    }
                    if let Some(nv) = nv {
                        if nv <= m {
                            notes.push(format!("Even \"same as last month\" did as well ({:.1}% against {:.1}%): the models add little here.", nv, m));
                        }
                    }
                    (sc, nv, kind, bad)
                }
                None => bail!("'{}': no model could be back-tested ({}). Choose a model by hand.", name, sc.iter().filter_map(|x| x.mape.clone().err()).next().unwrap_or_default()),
            }
        }
        (Choice::Auto, None) => {
            // too short to back-test: the simplest model that can run
            notes.push("Too little history to back-test (18 months needed): Growth / run-off is used.".to_string());
            (Vec::new(), None, ModelKind::Growth, true)
        }
    };
    let raw = run_model(kind, &fit_y, fm, s.horizon).map_err(|e| anyhow::anyhow!("'{}': {}", name, e))?;
    notes.extend(raw.notes);
    Ok(SeriesForecast {
        name: name.to_string(),
        history: history.clone(),
        outliers,
        kind,
        scores,
        naive_mape: naive,
        backtest_months: k,
        first_ahead: history.ym(history.len()),
        point: raw.point,
        bands: raw.bands,
        notes,
        not_good,
    })
}

/// Applies the manual overlays that name this series (or no series) to its forecast, bands included.
pub fn apply_overlays(f: &mut SeriesForecast, overlays: &[Overlay]) -> Result<()> {
    for o in overlays.iter().filter(|o| o.group.as_deref().is_none_or(|g| g == f.name)) {
        let first = Monthly::new(f.first_ahead.0, f.first_ahead.1, Vec::new());
        let Some(pos) = first.position_of(o.year, o.month) else {
            bail!("The adjustment for {} {} is before the first forecast month ({}).", MONTHS[(o.month.clamp(1, 12) - 1) as usize], o.year, f.label_ahead(0));
        };
        if pos >= f.point.len() {
            bail!("The adjustment for {} {} is after the last forecast month ({}).", MONTHS[(o.month.clamp(1, 12) - 1) as usize], o.year, f.label_ahead(f.point.len() - 1));
        }
        let range = if o.from_here { pos..f.point.len() } else { pos..pos + 1 };
        for i in range {
            f.point[i] += o.amount;
            for b in f.bands[i].iter_mut() {
                *b += o.amount;
            }
        }
    }
    Ok(())
}

/// Forecasts each group and adds them up. The total's point is the sum of the points; its bands add the groups' bounds (the groups
/// are assumed to move together, so the total's band is wider than a forecast of the total series would give: decision F-5).
#[derive(Clone, Debug, PartialEq)]
pub struct GroupedForecast {
    pub groups: Vec<std::result::Result<SeriesForecast, String>>,
    /// `None` when a group could not be forecast (the message says which).
    pub total: std::result::Result<SeriesForecast, String>,
}

pub fn forecast_groups(groups: &[(String, Monthly)], s: &Settings, overlays: &[Overlay]) -> GroupedForecast {
    let mut out: Vec<std::result::Result<SeriesForecast, String>> = Vec::new();
    for (name, m) in groups {
        let r = forecast_series(name, m, s).and_then(|mut f| {
            apply_overlays(&mut f, overlays)?;
            Ok(f)
        });
        out.push(r.map_err(|e| format!("{:#}", e)));
    }
    let total = sum_groups(&out, groups, overlays);
    GroupedForecast { groups: out, total }
}

fn sum_groups(res: &[std::result::Result<SeriesForecast, String>], groups: &[(String, Monthly)], overlays: &[Overlay]) -> std::result::Result<SeriesForecast, String> {
    let bad: Vec<String> = res.iter().filter_map(|r| r.as_ref().err().cloned()).collect();
    if !bad.is_empty() {
        return Err(format!("The total is not shown because {} could not be forecast: {}", if bad.len() == 1 { "one group" } else { "some groups" }, bad.join(" ")));
    }
    let oks: Vec<&SeriesForecast> = res.iter().filter_map(|r| r.as_ref().ok()).collect();
    let Some(first) = oks.first() else {
        return Err("There is nothing to forecast.".to_string());
    };
    // every group must end in the same month and look ahead the same distance
    for f in &oks {
        if f.first_ahead != first.first_ahead || f.point.len() != first.point.len() {
            return Err(format!("The groups end in different months ('{}' ahead from {}, '{}' from {}). Cut them to the same last month first.", first.name, first.label_ahead(0), f.name, f.label_ahead(0)));
        }
    }
    let n = first.point.len();
    let mut point = vec![0.0; n];
    let mut bands = vec![[0.0; 4]; n];
    for f in &oks {
        for i in 0..n {
            point[i] += f.point[i];
            for j in 0..4 {
                bands[i][j] += f.bands[i][j];
            }
        }
    }
    // the total's history: the groups' months added up (over the months every group has)
    let start = groups.iter().map(|(_, m)| m.year * 12 + m.month as i32 - 1).max().unwrap_or(0);
    let end = groups.iter().map(|(_, m)| m.year * 12 + m.month as i32 - 1 + m.len() as i32).min().unwrap_or(0);
    let len = (end - start).max(0) as usize;
    let mut hist = vec![0.0; len];
    for (_, m) in groups {
        let off = (start - (m.year * 12 + m.month as i32 - 1)) as usize;
        for i in 0..len {
            hist[i] += m.values[off + i];
        }
    }
    let history = Monthly::new(start.div_euclid(12), (start.rem_euclid(12) + 1) as u32, hist);
    let mut notes: Vec<String> = vec![format!("The sum of {} forecasts; the band adds the groups' bands.", oks.len())];
    notes.extend(oks.iter().filter(|f| f.not_good).map(|f| format!("'{}' did not back-test well.", f.name)));
    let _ = overlays;
    Ok(SeriesForecast {
        name: "Total".to_string(),
        history,
        outliers: Vec::new(),
        kind: first.kind,
        scores: Vec::new(),
        naive_mape: None,
        backtest_months: None,
        first_ahead: first.first_ahead,
        point,
        bands,
        notes,
        not_good: oks.iter().any(|f| f.not_good),
    })
}

// ------------------------------------------------------------------------------------------------ daily rows to months

/// How a month's value is taken from daily rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Basis {
    /// The last date of the month that has data (a balance at month-end).
    MonthEnd,
    /// The average over every calendar day of the month, carrying the last balance forward over days with no data.
    AverageDaily,
}

/// Months from daily totals (`daily`: date to the sum of the day's rows). A month with no data inside the series is an error (never a
/// silent 0). The first month is left out when data starts after the 3rd and the last when data stops before 3 days from month-end;
/// the second value says what was left out.
pub fn roll_up_daily(daily: &BTreeMap<NaiveDate, f64>, basis: Basis) -> Result<(Monthly, Vec<String>)> {
    let (Some((&first, _)), Some((&last, _))) = (daily.iter().next(), daily.iter().next_back()) else {
        bail!("There are no rows to forecast from.");
    };
    if daily.values().any(|v| !v.is_finite()) {
        bail!("A daily total is not a number. Fix the data before forecasting.");
    }
    let mut notes = Vec::new();
    let month_end_of = |d: NaiveDate| -> NaiveDate {
        let (y, m) = if d.month() == 12 { (d.year() + 1, 1) } else { (d.year(), d.month() + 1) };
        NaiveDate::from_ymd_opt(y, m, 1).and_then(|n| n.pred_opt()).unwrap_or(d)
    };
    let mut start = (first.year(), first.month());
    let mut end = (last.year(), last.month());
    if first.day() > 3 {
        notes.push(format!("{} {} is left out: the data starts on the {}.", MONTHS[start.1 as usize - 1], start.0, first.day()));
        start = if start.1 == 12 { (start.0 + 1, 1) } else { (start.0, start.1 + 1) };
    }
    if (month_end_of(last) - last).num_days() > 2 {
        notes.push(format!("{} {} is left out: the data stops on {} (not month-end).", MONTHS[end.1 as usize - 1], end.0, last.format("%d %b")));
        end = if end.1 == 1 { (end.0 - 1, 12) } else { (end.0, end.1 - 1) };
    }
    if (end.0, end.1) < (start.0, start.1) {
        bail!("The data covers less than one whole month.");
    }
    let mut values = Vec::new();
    let (mut y, mut m) = start;
    loop {
        let first_day = NaiveDate::from_ymd_opt(y, m, 1).ok_or_else(|| anyhow::anyhow!("bad month"))?;
        let last_day = month_end_of(first_day);
        let in_month: Vec<(&NaiveDate, &f64)> = daily.range(first_day..=last_day).collect();
        if in_month.is_empty() {
            bail!("{} {} has no data between other months. Add it or forecast from a series without the gap.", MONTHS[m as usize - 1], y);
        }
        match basis {
            Basis::MonthEnd => values.push(*in_month[in_month.len() - 1].1),
            Basis::AverageDaily => {
                // the balance carried in from before the month, when the first day has no row
                let mut cur = daily.range(..first_day).next_back().map(|(_, v)| *v);
                let mut sum = 0.0;
                let mut d = first_day;
                while d <= last_day {
                    if let Some(v) = daily.get(&d) {
                        cur = Some(*v);
                    }
                    match cur {
                        Some(v) => sum += v,
                        None => bail!("{} {} starts before the first row ({}): its average needs a balance to carry in.", MONTHS[m as usize - 1], y, first.format("%d %b %Y")),
                    }
                    d = d.succ_opt().ok_or_else(|| anyhow::anyhow!("date overflow"))?;
                }
                values.push(sum / last_day.day() as f64);
            }
        }
        if (y, m) == end {
            break;
        }
        if m == 12 {
            y += 1;
            m = 1;
        } else {
            m += 1;
        }
    }
    Ok((Monthly::new(start.0, start.1, values), notes))
}
