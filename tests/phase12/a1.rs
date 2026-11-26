//! Phase 12 A1: ARIMA against statsmodels 0.15 (`ARIMA`, `loglike`). The reference numbers are in
//! tests/fixtures/phase12_arima_ref.json, made by tools/gen_arima_ref.py.

use micro_parquet::arima::*;
use micro_parquet::forecast::*;

fn reference() -> serde_json::Value {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase12_arima_ref.json");
    serde_json::from_str(&std::fs::read_to_string(p).expect("fixture")).expect("json")
}

fn vec_of(v: &serde_json::Value) -> Vec<f64> {
    v.as_array().expect("array").iter().map(|x| x.as_f64().expect("number")).collect()
}

fn close(a: f64, b: f64, rel: f64, what: &str) {
    assert!((a - b).abs() <= rel * b.abs().max(1.0), "{what}: {a} vs {b}");
}

fn diff(y: &[f64], d: usize) -> Vec<f64> {
    let mut w = y.to_vec();
    for _ in 0..d {
        w = w.windows(2).map(|x| x[1] - x[0]).collect();
    }
    w
}

#[test]
fn a1_a_the_exact_likelihood_at_fixed_parameters_equals_statsmodels() {
    let r = reference();
    for f in r["fixed"].as_array().expect("fixed") {
        let y = vec_of(&f["y"]);
        let (p, q) = (f["p"].as_u64().expect("p") as usize, f["q"].as_u64().expect("q") as usize);
        let par = vec_of(&f["params"]);
        let m = Arma { mu: par[0], phi: par[1..1 + p].to_vec(), theta: par[1 + p..1 + p + q].to_vec() };
        let ll = loglike(&y, &m, par[1 + p + q]).expect("filter");
        close(ll, f["llf"].as_f64().expect("llf"), 1e-9, &format!("loglike ARMA({p},{q})"));
    }
}

#[test]
fn a1_b_fitted_models_reach_the_statsmodels_likelihood_and_parameters() {
    let r = reference();
    for (name, c) in r["series"].as_object().expect("series") {
        let (d, p, q) = (c["d"].as_u64().expect("d") as usize, c["p"].as_u64().expect("p") as usize, c["q"].as_u64().expect("q") as usize);
        let y = vec_of(&c["y"]);
        let w = diff(&y, d);
        let (m, s2, ll) = fit_arma(&w, p, q, d < 2).expect("fit");
        let want_ll = c["llf"].as_f64().expect("llf");
        // statsmodels' optimiser may stop on the edge of invertibility (a root at 0.9995); ours stays inside, so the allowed gap is small
        assert!(ll >= want_ll - 0.05, "{name}: our log-likelihood {ll} is below statsmodels' {want_ll}");
        assert!(ll <= want_ll + 0.5, "{name}: our log-likelihood {ll} is above statsmodels' {want_ll}: one of the two is wrong");
        if name != "growth_d1_11" && name != "growth_d1_01" {
            let params = vec_of(&c["params"]);
            close(m.mu, params[0], 0.02, &format!("{name} mean"));
            for i in 0..p {
                assert!((m.phi[i] - params[1 + i]).abs() < 0.03, "{name} ar{} {} vs {}", i + 1, m.phi[i], params[1 + i]);
            }
            for i in 0..q {
                assert!((m.theta[i] - params[1 + p + i]).abs() < 0.03, "{name} ma{} {} vs {}", i + 1, m.theta[i], params[1 + p + i]);
            }
            close(s2, params[params.len() - 1], 0.02, &format!("{name} sigma2"));
        }
    }
}

#[test]
fn a1_c_forecast_and_band_width_match_statsmodels_on_the_differenced_scale() {
    let r = reference();
    for name in ["arma21_d0", "arma11_d0", "arma10_d0", "growth_d1_20"] {
        let c = &r["series"][name];
        let (d, p, q) = (c["d"].as_u64().expect("d") as usize, c["p"].as_u64().expect("p") as usize, c["q"].as_u64().expect("q") as usize);
        let y = vec_of(&c["y"]);
        let params = vec_of(&c["params"]);
        // our fit and statsmodels' agree to a few decimals, so run both on the statsmodels parameters: the forecast recursion must match
        let m = Arma { mu: params[0], phi: params[1..1 + p].to_vec(), theta: params[1 + p..1 + p + q].to_vec() };
        let s2 = params[params.len() - 1];
        let fit = Fit { p, d: 0, q, model: m.clone(), sigma2: s2, loglik: 0.0, aicc: 0.0 };
        let w = diff(&y, d);
        let (f, sd) = forecast_levels(&w, &fit, 6).expect("forecast");
        let (want, want_se) = (vec_of(&c["forecast_w"]), vec_of(&c["se_w"]));
        for k in 0..6 {
            close(f[k], want[k], 1e-6, &format!("{name} forecast {k}"));
            close(sd[k], want_se[k], 1e-6, &format!("{name} se {k}"));
        }
    }
}

#[test]
fn a1_d_level_forecast_of_an_integrated_model_is_close_to_statsmodels_arima_111() {
    let r = reference();
    let y = vec_of(&r["series"]["growth_d1_11"]["y"]);
    // the same orders as the reference; our search picks its own order, so run the fixed one through the lower-level calls
    let (m, s2, ll) = fit_arma(&diff(&y, 1), 1, 1, true).expect("fit");
    let fit = Fit { p: 1, d: 1, q: 1, model: m, sigma2: s2, loglik: ll, aicc: 0.0 };
    let (lv, sd) = forecast_levels(&y, &fit, 6).expect("forecast");
    let want = vec_of(&r["level_111"]["mean"]);
    let want_se = vec_of(&r["level_111"]["se"]);
    for k in 0..6 {
        // statsmodels starts the integrated model with a diffuse state and stops on the invertibility edge, so allow 1% here
        close(lv[k], want[k], 0.01, &format!("level {k}"));
        close(sd[k], want_se[k], 0.05, &format!("se {k}"));
    }
}

#[test]
fn a1_e_a_random_walk_with_drift_is_found_and_forecast_as_a_straight_line() {
    let y: Vec<f64> = (0..36).map(|t| 1000.0 + 25.0 * t as f64).collect();
    // a perfectly straight series has no spread left after one difference; ARIMA has nothing to fit and says so plainly
    let e = arima(&y, 1, 6).expect_err("a noise-free line has no shocks to fit");
    assert!(e.to_string().contains("does not change") || e.to_string().contains("no ARIMA"), "{e}");
    // with a little shock the drift is recovered
    let noisy: Vec<f64> = (0..48).map(|t| 1000.0 + 25.0 * t as f64 + ((t * 7919 % 13) as f64 - 6.0) * 2.0).collect();
    let raw = arima(&noisy, 1, 6).expect("arima");
    for k in 0..6 {
        let want = 1000.0 + 25.0 * (48 + k) as f64;
        assert!((raw.point[k] - want).abs() < 30.0, "month {k}: {} vs {}", raw.point[k], want);
    }
    for b in &raw.bands {
        assert!(b[2] < b[0] && b[0] < b[1] && b[1] < b[3], "bands nest: {:?}", b);
    }
    assert!(raw.bands[5][3] - raw.bands[5][2] > raw.bands[0][3] - raw.bands[0][2], "bands widen with the horizon");
    assert!(raw.notes.iter().any(|n| n.starts_with("ARIMA(")), "{:?}", raw.notes);
}

#[test]
fn a1_f_d_is_one_for_a_trend_and_zero_for_a_stationary_series() {
    let r = reference();
    assert_eq!(choose_d(&vec_of(&r["series"]["growth_d1_11"]["y"])), 1);
    assert_eq!(choose_d(&vec_of(&r["series"]["arma21_d0"]["y"])), 0);
    assert_eq!(choose_d(&vec_of(&r["series"]["arma10_d0"]["y"])), 0);
    // the Dickey-Fuller statistic is the one statsmodels' adfuller gives (no lags, with a constant)
    for (k, key) in [("growth_d1_11", "growth_d1_11_level"), ("arma21_d0", "arma21_d0_level"), ("arma10_d0", "arma10_d0_level")] {
        close(adf_t(&vec_of(&r["series"][k]["y"])).expect("adf"), r["adf"][key].as_f64().expect("ref"), 1e-9, key);
    }
    close(adf_t(&diff(&vec_of(&r["series"]["growth_d1_11"]["y"]), 1)).expect("adf"), r["adf"]["growth_d1_11_diff1"].as_f64().expect("ref"), 1e-9, "diff1");
}

#[test]
fn a1_g_the_same_data_always_gives_the_same_forecast() {
    let r = reference();
    let y = vec_of(&r["series"]["growth_d1_11"]["y"]);
    assert_eq!(arima(&y, 1, 6).expect("a"), arima(&y, 1, 6).expect("b"));
}

#[test]
fn a1_h_arima_is_a_fourth_model_in_the_auto_back_test() {
    let r = reference();
    let y = vec_of(&r["series"]["growth_d1_11"]["y"]);
    let h = Monthly::new(2021, 1, y);
    let f = forecast_series("D", &h, &Settings { horizon: 6, ..Settings::default() }).expect("forecast");
    assert_eq!(f.scores.len(), 4, "four models are scored");
    let a = f.scores.iter().find(|s| s.kind == ModelKind::Arima).expect("arima scored");
    assert!(a.mape.is_ok(), "{:?}", a.mape);
    let fixed = forecast_series("D", &h, &Settings { horizon: 6, choice: Choice::Fixed(ModelKind::Arima), ..Settings::default() }).expect("fixed");
    assert_eq!(fixed.kind, ModelKind::Arima);
    assert_eq!(fixed.point.len(), 6);
}

#[test]
fn a1_i_too_little_history_is_refused_in_plain_words() {
    let e = arima(&[1.0, 2.0, 3.5, 4.0, 5.2, 6.0, 7.1], 1, 3).expect_err("short");
    assert!(e.to_string().contains("at least 12 months"), "{e}");
}

#[test]
fn a1_j_a_constant_series_is_refused_not_given_a_zero_width_band() {
    let e = arima(&vec![100.0; 30], 1, 3).expect_err("flat");
    assert!(!e.to_string().is_empty());
}
