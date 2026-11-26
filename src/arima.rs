//! Phase 12 A1: ARIMA(p, d, q) in plain Rust, the fourth forecast model.
//!
//! - The series is differenced `d` times (0, 1 or 2: none when
//!   a Dickey-Fuller test at 5% finds no unit root, one more only when the first difference still has one). The differenced series is an ARMA(p, q) around a mean (a drift for d = 1; no
//!   constant for d = 2).
//! - The likelihood is the **exact Gaussian likelihood** of the ARMA, worked out with the Kalman filter on the standard
//!   state-space form (Harvey), started from the stationary state covariance, with the variance concentrated out. It is the likelihood
//!   statsmodels' `SARIMAX` evaluates (checked in `tests/phase12/a1.rs`).
//! - Stationarity and invertibility are kept by the partial-autocorrelation transform (Monahan), so the search is unconstrained and
//!   always lands on a usable model. The search is Nelder-Mead from three fixed starting points: no random numbers, so the same
//!   data always gives the same forecast.
//! - The order is the (p, q) in 0..=2 with the smallest corrected AIC.
//! - With 24 months or more the calendar-month pattern (the same classical decomposition the other models use) is taken out first
//!   and put back on the forecast, so ARIMA does not have to spend its terms on the season. It says so in its notes.
//! - Bands are the model's own prediction intervals (normal errors): the variance at k months ahead is sigma^2 times the sum of the
//!   squared psi-weights of the integrated model. They ignore the uncertainty of the fitted parameters, as statsmodels' do.

use crate::forecast::{Raw, Z80, Z95};
use anyhow::{bail, Result};

const MAX_ORDER: usize = 2;
/// The fewest months an ARIMA is fitted to.
pub const MIN_MONTHS: usize = 12;

// ------------------------------------------------------------------------------------------------ small linear algebra

/// Solves `a x = b` (n x n, row-major) by Gaussian elimination with partial pivoting; `None` when singular.
fn solve(mut a: Vec<f64>, mut b: Vec<f64>, n: usize) -> Option<Vec<f64>> {
    for c in 0..n {
        let mut piv = c;
        for r in c + 1..n {
            if a[r * n + c].abs() > a[piv * n + c].abs() {
                piv = r;
            }
        }
        if a[piv * n + c].abs() < 1e-12 {
            return None;
        }
        if piv != c {
            for k in 0..n {
                a.swap(c * n + k, piv * n + k);
            }
            b.swap(c, piv);
        }
        for r in c + 1..n {
            let f = a[r * n + c] / a[c * n + c];
            if f != 0.0 {
                for k in c..n {
                    a[r * n + k] -= f * a[c * n + k];
                }
                b[r] -= f * b[c];
            }
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let mut s = b[r];
        for k in r + 1..n {
            s -= a[r * n + k] * x[k];
        }
        x[r] = s / a[r * n + r];
    }
    Some(x)
}

// ------------------------------------------------------------------------------------------------ the ARMA state space

/// `w_t = mu + phi(B)^-1 theta(B) e_t`, `e ~ N(0, sigma2)`; phi has the sign of statsmodels (`w_t = phi_1 w_{t-1} + ... + e_t + theta_1 e_{t-1}`).
#[derive(Clone, Debug, PartialEq)]
pub struct Arma {
    pub phi: Vec<f64>,
    pub theta: Vec<f64>,
    pub mu: f64,
}

struct Space {
    r: usize,
    /// Transition, row-major r x r.
    t: Vec<f64>,
    /// The loading of the shock on the state (length r).
    rv: Vec<f64>,
}

fn space(m: &Arma) -> Space {
    let (p, q) = (m.phi.len(), m.theta.len());
    let r = p.max(q + 1).max(1);
    let mut t = vec![0.0; r * r];
    for i in 0..r {
        if i < p {
            t[i * r] = m.phi[i];
        }
        if i + 1 < r {
            t[i * r + i + 1] = 1.0;
        }
    }
    let mut rv = vec![0.0; r];
    rv[0] = 1.0;
    for j in 0..q {
        rv[j + 1] = m.theta[j];
    }
    Space { r, t, rv }
}

/// The stationary covariance of the state for unit shock variance: `P = T P T' + R R'`.
fn stationary_cov(s: &Space) -> Option<Vec<f64>> {
    let r = s.r;
    let n = r * r;
    // (I - T (x) T) vec(P) = vec(R R')
    let mut a = vec![0.0; n * n];
    for i in 0..r {
        for j in 0..r {
            let row = i * r + j;
            a[row * n + row] += 1.0;
            for k in 0..r {
                for l in 0..r {
                    a[row * n + k * r + l] -= s.t[i * r + k] * s.t[j * r + l];
                }
            }
        }
    }
    let b: Vec<f64> = (0..n).map(|x| s.rv[x / r] * s.rv[x % r]).collect();
    solve(a, b, n)
}

/// What the Kalman filter found.
pub struct Filtered {
    /// One-step prediction errors and their variances (for unit sigma2).
    pub v: Vec<f64>,
    pub f: Vec<f64>,
    /// The state predicted for the month after the last one.
    pub next_state: Vec<f64>,
}

/// Runs the filter on `w` (already differenced). `None` when the model has no stationary start (the optimiser steps away from it).
pub fn filter(w: &[f64], m: &Arma) -> Option<Filtered> {
    let s = space(m);
    let r = s.r;
    let mut p = stationary_cov(&s)?;
    let mut a = vec![0.0; r];
    let mut v = Vec::with_capacity(w.len());
    let mut f = Vec::with_capacity(w.len());
    for &x in w {
        let ft = p[0];
        if !(ft > 1e-12) || !ft.is_finite() {
            return None;
        }
        let vt = x - m.mu - a[0];
        // K = T P Z' / F, with Z = e1, so P Z' is the first column of P
        let mut k = vec![0.0; r];
        for i in 0..r {
            let mut acc = 0.0;
            for j in 0..r {
                acc += s.t[i * r + j] * p[j * r];
            }
            k[i] = acc / ft;
        }
        // a_next = T a + K v
        let mut an = vec![0.0; r];
        for i in 0..r {
            let mut acc = k[i] * vt;
            for j in 0..r {
                acc += s.t[i * r + j] * a[j];
            }
            an[i] = acc;
        }
        // P_next = T P T' + R R' - K F K'
        let mut tp = vec![0.0; r * r];
        for i in 0..r {
            for j in 0..r {
                let mut acc = 0.0;
                for l in 0..r {
                    acc += s.t[i * r + l] * p[l * r + j];
                }
                tp[i * r + j] = acc;
            }
        }
        let mut pn = vec![0.0; r * r];
        for i in 0..r {
            for j in 0..r {
                let mut acc = s.rv[i] * s.rv[j] - k[i] * ft * k[j];
                for l in 0..r {
                    acc += tp[i * r + l] * s.t[j * r + l];
                }
                pn[i * r + j] = acc;
            }
        }
        p = pn;
        a = an;
        v.push(vt);
        f.push(ft);
    }
    Some(Filtered { v, f, next_state: a })
}

/// The exact Gaussian log-likelihood of `w` for the model with shock variance `sigma2` (what statsmodels' `SARIMAX.loglike` gives).
pub fn loglike(w: &[f64], m: &Arma, sigma2: f64) -> Option<f64> {
    let fl = filter(w, m)?;
    let n = w.len() as f64;
    let mut ll = -0.5 * n * (2.0 * std::f64::consts::PI * sigma2).ln();
    for (v, f) in fl.v.iter().zip(&fl.f) {
        ll -= 0.5 * (f.ln() + v * v / (f * sigma2));
    }
    Some(ll)
}

/// The log-likelihood with the variance at its best value, and that value.
pub fn concentrated(w: &[f64], m: &Arma) -> Option<(f64, f64)> {
    let fl = filter(w, m)?;
    let n = w.len() as f64;
    let s2 = fl.v.iter().zip(&fl.f).map(|(v, f)| v * v / f).sum::<f64>() / n;
    if !(s2 > 0.0) || !s2.is_finite() {
        return None;
    }
    let sum_ln_f: f64 = fl.f.iter().map(|f| f.ln()).sum();
    Some((-0.5 * (n * (2.0 * std::f64::consts::PI).ln() + sum_ln_f + n * s2.ln() + n), s2))
}

// ------------------------------------------------------------------------------------------------ the transform and the search

/// Partial autocorrelations (any real numbers, through tanh) to AR coefficients inside the stationary region (Durbin-Levinson).
pub fn pacf_to_coefs(x: &[f64]) -> Vec<f64> {
    let mut phi: Vec<f64> = Vec::new();
    for (k, xi) in x.iter().enumerate() {
        let r = xi.tanh();
        let prev = phi.clone();
        for j in 0..k {
            phi[j] = prev[j] - r * prev[k - 1 - j];
        }
        phi.push(r);
    }
    phi
}

/// The model of a point of the search: `[mu?, ar..., ma...]`.
fn model_at(x: &[f64], with_mean: bool, p: usize, q: usize) -> Arma {
    let off = usize::from(with_mean);
    let phi = pacf_to_coefs(&x[off..off + p]);
    // the MA polynomial 1 + theta B has its roots outside the circle when -theta is stationary
    let theta: Vec<f64> = pacf_to_coefs(&x[off + p..off + p + q]).into_iter().map(|c| -c).collect();
    Arma { phi, theta, mu: if with_mean { x[0] } else { 0.0 } }
}

fn nelder_mead(f: &dyn Fn(&[f64]) -> f64, x0: &[f64], steps: &[f64]) -> (Vec<f64>, f64) {
    let n = x0.len();
    let mut pts: Vec<Vec<f64>> = vec![x0.to_vec()];
    for i in 0..n {
        let mut x = x0.to_vec();
        x[i] += steps[i];
        pts.push(x);
    }
    let mut vals: Vec<f64> = pts.iter().map(|x| f(x)).collect();
    for _ in 0..(400 * n.max(1)) {
        let mut order: Vec<usize> = (0..=n).collect();
        order.sort_by(|&a, &b| vals[a].partial_cmp(&vals[b]).unwrap_or(std::cmp::Ordering::Equal));
        pts = order.iter().map(|&i| pts[i].clone()).collect();
        vals = order.iter().map(|&i| vals[i]).collect();
        if (vals[n] - vals[0]).abs() < 1e-11 * (1.0 + vals[0].abs()) && n > 0 {
            let size: f64 = (1..=n).map(|i| pts[i].iter().zip(&pts[0]).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max)).fold(0.0, f64::max);
            if size < 1e-8 {
                break;
            }
        }
        let mut c = vec![0.0; n];
        for p in pts.iter().take(n) {
            for (ci, pi) in c.iter_mut().zip(p) {
                *ci += pi / n as f64;
            }
        }
        let at = |t: f64| -> Vec<f64> { c.iter().zip(&pts[n]).map(|(ci, wi)| ci + t * (ci - wi)).collect() };
        let xr = at(1.0);
        let fr = f(&xr);
        if fr < vals[0] {
            let xe = at(2.0);
            let fe = f(&xe);
            if fe < fr {
                pts[n] = xe;
                vals[n] = fe;
            } else {
                pts[n] = xr;
                vals[n] = fr;
            }
        } else if fr < vals[n - 1] {
            pts[n] = xr;
            vals[n] = fr;
        } else {
            let (xc, fc) = if fr < vals[n] {
                let x = at(0.5);
                let v = f(&x);
                (x, v)
            } else {
                let x = at(-0.5);
                let v = f(&x);
                (x, v)
            };
            if fc < vals[n].min(fr) {
                pts[n] = xc;
                vals[n] = fc;
            } else {
                for i in 1..=n {
                    let best = pts[0].clone();
                    pts[i] = pts[i].iter().zip(&best).map(|(a, b)| b + 0.5 * (a - b)).collect();
                    vals[i] = f(&pts[i]);
                }
            }
        }
    }
    let mut best = 0;
    for i in 1..=n {
        if vals[i] < vals[best] {
            best = i;
        }
    }
    (pts[best].clone(), vals[best])
}

/// One fitted ARIMA(p, d, q).
#[derive(Clone, Debug, PartialEq)]
pub struct Fit {
    pub p: usize,
    pub d: usize,
    pub q: usize,
    pub model: Arma,
    pub sigma2: f64,
    /// The log-likelihood of the differenced series (in the series' own units).
    pub loglik: f64,
    pub aicc: f64,
}

const BIG: f64 = 1e30;

/// Fits an ARMA(p, q) with a mean (or none) to the differenced series `w`.
pub fn fit_arma(w: &[f64], p: usize, q: usize, with_mean: bool) -> Result<(Arma, f64, f64)> {
    let n = w.len();
    let mean = w.iter().sum::<f64>() / n as f64;
    let sd = (w.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64).sqrt();
    if !(sd > 0.0) || !sd.is_finite() {
        bail!("the series does not change, so there is nothing for ARIMA to fit");
    }
    // fit in units of the spread so the search is well scaled; the likelihood is put back in the original units below
    let ws: Vec<f64> = w.iter().map(|x| x / sd).collect();
    let k = usize::from(with_mean) + p + q;
    let objective = |x: &[f64]| -> f64 {
        if x.iter().any(|v| !v.is_finite()) || x.iter().skip(usize::from(with_mean)).any(|v| v.abs() > 12.0) {
            return BIG;
        }
        let m = model_at(x, with_mean, p, q);
        match concentrated(&ws, &m) {
            Some((ll, _)) => -ll,
            None => BIG,
        }
    };
    let mut starts: Vec<Vec<f64>> = Vec::new();
    for s in 0..3 {
        let mut x = vec![0.0; k];
        if with_mean {
            x[0] = mean / sd;
        }
        let off = usize::from(with_mean);
        match s {
            1 => {
                for i in 0..p {
                    x[off + i] = if i == 0 { 0.6 } else { 0.0 };
                }
            }
            2 => {
                for i in 0..q {
                    x[off + p + i] = if i == 0 { 0.6 } else { 0.0 };
                }
                for i in 0..p {
                    x[off + i] = if i == 0 { -0.3 } else { 0.0 };
                }
            }
            _ => {}
        }
        starts.push(x);
    }
    let steps: Vec<f64> = (0..k).map(|i| if with_mean && i == 0 { 0.2 } else { 0.4 }).collect();
    let mut best: Option<(Vec<f64>, f64)> = None;
    for x0 in &starts {
        if k == 0 {
            let v = objective(&[]);
            if best.as_ref().is_none_or(|b| v < b.1) {
                best = Some((Vec::new(), v));
            }
            break;
        }
        let (mut x, mut v) = nelder_mead(&objective, x0, &steps);
        // restart once from where it stopped: Nelder-Mead stalls on a collapsed simplex
        let (x2, v2) = nelder_mead(&objective, &x, &steps.iter().map(|s| s * 0.25).collect::<Vec<_>>());
        if v2 < v {
            x = x2;
            v = v2;
        }
        if best.as_ref().is_none_or(|b| v < b.1) {
            best = Some((x, v));
        }
    }
    let Some((x, v)) = best else { bail!("no ARIMA could be fitted") };
    if v >= BIG {
        bail!("no stationary ARMA({}, {}) could be fitted to the series", p, q);
    }
    let mut m = model_at(&x, with_mean, p, q);
    let (_, s2s) = concentrated(&ws, &m).ok_or_else(|| anyhow::anyhow!("the fitted ARMA({}, {}) is not usable", p, q))?;
    m.mu *= sd;
    let sigma2 = s2s * sd * sd;
    // the likelihood in the original units: the scaled one less n ln(sd)
    let ll = -v - n as f64 * sd.ln();
    Ok((m, sigma2, ll))
}

fn diff(y: &[f64], d: usize) -> Vec<f64> {
    let mut w = y.to_vec();
    for _ in 0..d {
        w = w.windows(2).map(|x| x[1] - x[0]).collect();
    }
    w
}

/// The Dickey-Fuller t statistic (no lagged differences, with a constant): the t of `g` in `dy_t = a + g y_{t-1} + e`.
/// `None` when the regression has no spread to work with.
pub fn adf_t(y: &[f64]) -> Option<f64> {
    let n = y.len();
    if n < 8 {
        return None;
    }
    let x: Vec<f64> = y[..n - 1].to_vec();
    let z: Vec<f64> = y.windows(2).map(|v| v[1] - v[0]).collect();
    let m = x.len() as f64;
    let (mx, mz) = (x.iter().sum::<f64>() / m, z.iter().sum::<f64>() / m);
    let sxx: f64 = x.iter().map(|v| (v - mx).powi(2)).sum();
    if sxx <= 0.0 {
        return None;
    }
    let sxz: f64 = x.iter().zip(&z).map(|(a, b)| (a - mx) * (b - mz)).sum();
    let g = sxz / sxx;
    let a = mz - g * mx;
    let sse: f64 = x.iter().zip(&z).map(|(xi, zi)| (zi - a - g * xi).powi(2)).sum();
    let s2 = sse / (m - 2.0);
    if s2 <= 1e-24 * (1.0 + mz * mz) {
        return None;
    }
    Some(g / (s2 / sxx).sqrt())
}

/// The 5% critical value of the Dickey-Fuller test with a constant (MacKinnon, about 50 to 100 observations).
pub const ADF_5PCT: f64 = -2.9;

/// The number of differences: none when a Dickey-Fuller test (5%) rejects a unit root, else one, and a second only when the
/// first difference still has one. A series with no spread left after a difference stops there.
pub fn choose_d(y: &[f64]) -> usize {
    let mut d = 0;
    while d < 2 {
        let w = diff(y, d);
        if w.len() < 8 {
            break;
        }
        match adf_t(&w) {
            Some(t) if t < ADF_5PCT => break,
            _ => d += 1,
        }
    }
    d
}

/// Fits every (p, q) up to 2 for a fixed `d` and returns the one with the smallest corrected AIC, and what each scored.
pub fn fit_best(y: &[f64], d: usize) -> Result<(Fit, Vec<(usize, usize, f64)>)> {
    let w = diff(y, d);
    if w.len() < 8 {
        bail!("ARIMA needs at least {} months; this series has {}.", MIN_MONTHS, y.len());
    }
    let with_mean = d < 2;
    let n = w.len() as f64;
    let mut best: Option<Fit> = None;
    let mut table = Vec::new();
    for p in 0..=MAX_ORDER {
        for q in 0..=MAX_ORDER {
            let Ok((model, sigma2, loglik)) = fit_arma(&w, p, q, with_mean) else { continue };
            let k = (usize::from(with_mean) + p + q + 1) as f64;
            if n - k - 1.0 <= 0.0 {
                continue;
            }
            let aicc = -2.0 * loglik + 2.0 * k + 2.0 * k * (k + 1.0) / (n - k - 1.0);
            table.push((p, q, aicc));
            if best.as_ref().is_none_or(|b| aicc < b.aicc - 1e-9) {
                best = Some(Fit { p, d, q, model, sigma2, loglik, aicc });
            }
        }
    }
    match best {
        Some(b) => Ok((b, table)),
        None => bail!("no ARIMA could be fitted to this series"),
    }
}

/// The psi-weights of the ARMA (the effect of a shock k months later), `n` of them.
pub fn psi_weights(m: &Arma, n: usize) -> Vec<f64> {
    let mut psi = vec![0.0; n];
    if n == 0 {
        return psi;
    }
    psi[0] = 1.0;
    for j in 1..n {
        let mut v = if j <= m.theta.len() { m.theta[j - 1] } else { 0.0 };
        for i in 1..=m.phi.len().min(j) {
            v += m.phi[i - 1] * psi[j - i];
        }
        psi[j] = v;
    }
    psi
}

/// The forecast `h` months ahead on the differenced scale, then summed back to the level, with the standard deviation of each.
pub fn forecast_levels(y: &[f64], fit: &Fit, h: usize) -> Result<(Vec<f64>, Vec<f64>)> {
    let w = diff(y, fit.d);
    let fl = filter(&w, &fit.model).ok_or_else(|| anyhow::anyhow!("the fitted ARIMA could not be run over the series"))?;
    let s = space(&fit.model);
    let r = s.r;
    let mut a = fl.next_state.clone();
    let mut wf = Vec::with_capacity(h);
    for _ in 0..h {
        wf.push(fit.model.mu + a[0]);
        let mut an = vec![0.0; r];
        for i in 0..r {
            for j in 0..r {
                an[i] += s.t[i * r + j] * a[j];
            }
        }
        a = an;
    }
    // integrate: level_{T+k} from the last d levels
    let mut level = wf.clone();
    for k in 1..=fit.d {
        // the last value of the series differenced d - k times
        let base = *diff(y, fit.d - k).last().unwrap_or(&0.0);
        let mut acc = base;
        for v in level.iter_mut() {
            acc += *v;
            *v = acc;
        }
    }
    let mut psi = psi_weights(&fit.model, h);
    for _ in 0..fit.d {
        let mut acc = 0.0;
        for v in psi.iter_mut() {
            acc += *v;
            *v = acc;
        }
    }
    let mut cum = 0.0;
    let sd: Vec<f64> = psi
        .iter()
        .map(|p| {
            cum += p * p;
            (fit.sigma2 * cum).sqrt()
        })
        .collect();
    Ok((level, sd))
}

/// The forecast for the Auto back-test and the chart: ARIMA with the order chosen by corrected AIC.
pub fn arima(y: &[f64], first_month: u32, h: usize) -> Result<Raw> {
    let n = y.len();
    if n < MIN_MONTHS {
        bail!("ARIMA needs at least {} months of history; this series has {}.", MIN_MONTHS, n);
    }
    let month_of = |t: usize| ((first_month as usize).saturating_sub(1) + t) % 12;
    let mut notes = Vec::new();
    let (adj, idx): (Vec<f64>, Option<[f64; 12]>) = if n >= 24 {
        let idx = crate::forecast::seasonal_indices(y, first_month)?;
        ((0..n).map(|t| y[t] - idx[month_of(t)]).collect(), Some(idx))
    } else {
        (y.to_vec(), None)
    };
    let d = choose_d(&adj);
    let (fit, _) = fit_best(&adj, d)?;
    let (lv, sd) = forecast_levels(&adj, &fit, h)?;
    let point: Vec<f64> = (0..h).map(|k| lv[k] + idx.map(|i| i[month_of(n + k)]).unwrap_or(0.0)).collect();
    let bands = point.iter().zip(&sd).map(|(p, s)| [p - Z80 * s, p + Z80 * s, p - Z95 * s, p + Z95 * s]).collect();
    notes.push(format!(
        "ARIMA({}, {}, {}) chosen by corrected AIC ({:.1}){}.",
        fit.p,
        fit.d,
        fit.q,
        fit.aicc,
        if idx.is_some() { "; the pattern of each calendar month was taken out first and put back" } else { "; fewer than 24 months, so no seasonal pattern is used" }
    ));
    Ok(Raw { point, bands, notes })
}
