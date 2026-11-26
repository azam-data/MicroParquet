//! Phase 11 F1: the forecasting maths against reference results made with statsmodels (OLS prediction intervals, ETS with fixed
//! parameters, seasonal_decompose), plus hand-worked cases. The reference numbers are in tests/fixtures/phase11_forecast_ref.json.

use chrono::NaiveDate;
use micro_parquet::forecast::*;
use std::collections::BTreeMap;

fn reference() -> serde_json::Value {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase11_forecast_ref.json");
    serde_json::from_str(&std::fs::read_to_string(p).expect("fixture")).expect("json")
}

fn vec_of(v: &serde_json::Value) -> Vec<f64> {
    v.as_array().expect("array").iter().map(|x| x.as_f64().expect("number")).collect()
}

fn close(a: f64, b: f64, rel: f64, what: &str) {
    assert!((a - b).abs() <= rel * b.abs().max(1.0), "{what}: {a} vs {b}");
}

#[test]
fn f1_a_growth_matches_the_ols_prediction_intervals_of_statsmodels() {
    let r = reference();
    let y = vec_of(&r["y"]);
    let g = growth(&y, 3, 12).expect("growth");
    let want = &r["growth"];
    let (mean, lo80, hi80, lo95, hi95) = (vec_of(&want["mean"]), vec_of(&want["lo80"]), vec_of(&want["hi80"]), vec_of(&want["lo95"]), vec_of(&want["hi95"]));
    for k in 0..3 {
        close(g.point[k], mean[k], 1e-9, "mean");
        close(g.bands[k][0], lo80[k], 1e-9, "lo80");
        close(g.bands[k][1], hi80[k], 1e-9, "hi80");
        close(g.bands[k][2], lo95[k], 1e-9, "lo95");
        close(g.bands[k][3], hi95[k], 1e-9, "hi95");
    }
}

#[test]
fn f1_b_seasonal_indices_match_statsmodels_seasonal_decompose() {
    let r = reference();
    let y = vec_of(&r["y"]);
    let idx = seasonal_indices(&y, 1).expect("indices");
    let want = vec_of(&r["seasonal"]);
    for m in 0..12 {
        close(idx[m], want[m], 1e-9, &format!("month {}", m + 1));
    }
    assert!(idx.iter().sum::<f64>().abs() < 1e-9, "the twelve add to zero");
}

#[test]
fn f1_c_holt_winters_recursion_matches_statsmodels_ets_at_fixed_parameters() {
    let r = reference();
    let y = vec_of(&r["y"]);
    let e = &r["ets"];
    let (l0, b0, s0) = hw_start(&y);
    let run = ets_run(&y, e["alpha"].as_f64().unwrap(), e["beta"].as_f64().unwrap(), e["gamma"].as_f64().unwrap(), l0, b0, Some(&s0), 12);
    close(run.sse, e["sse"].as_f64().unwrap(), 1e-9, "sse");
    let (fc, vm) = (vec_of(&e["forecast"]), vec_of(&e["var_mult"]));
    for k in 0..12 {
        close(run.point[k], fc[k], 1e-9, &format!("forecast {k}"));
        close(run.var_mult[k], vm[k], 1e-9, &format!("variance multiplier {k}"));
    }
}

#[test]
fn f1_d_holts_linear_trend_matches_statsmodels_ets() {
    let r = reference();
    let y: Vec<f64> = vec_of(&r["y"])[..20].to_vec();
    let h = &r["holt"];
    let (l0, b0) = holt_start(&y).expect("start");
    let run = ets_run(&y, h["alpha"].as_f64().unwrap(), h["beta"].as_f64().unwrap(), 0.0, l0, b0, None, 5);
    close(run.sse, h["sse"].as_f64().unwrap(), 1e-9, "sse");
    let (fc, vm) = (vec_of(&h["forecast"]), vec_of(&h["var_mult"]));
    for k in 0..5 {
        close(run.point[k], fc[k], 1e-9, "forecast");
        close(run.var_mult[k], vm[k], 1e-9, "variance multiplier");
    }
}

#[test]
fn f1_e_the_fitted_parameters_do_no_worse_than_the_reference_parameters() {
    let r = reference();
    let y = vec_of(&r["y"]);
    let (l0, b0, s0) = hw_start(&y);
    let (sse, a, b, g) = ets_fit(&y, l0, b0, Some(&s0));
    assert!(sse <= r["ets"]["sse"].as_f64().unwrap() + 1e-9, "{sse}");
    assert!(a > 0.0 && b <= a && g <= 1.0 - a, "admissible: {a} {b} {g}");
    // the search is deterministic
    assert_eq!(ets_fit(&y, l0, b0, Some(&s0)), (sse, a, b, g));
}

#[test]
fn f1_f_a_steady_run_off_is_found_exactly() {
    // 1,000 falling 2% a month: the next three months are 1000 x 0.98^k and the band is a point (no noise)
    let y: Vec<f64> = (0..12).map(|t| 1000.0 * 0.98f64.powi(t)).collect();
    let g = growth(&y, 3, 12).expect("growth");
    for k in 1..=3 {
        close(g.point[k - 1], 1000.0 * 0.98f64.powi(11 + k as i32), 1e-9, "run-off");
        assert!((g.bands[k - 1][3] - g.bands[k - 1][2]).abs() < 1e-6, "no noise, no band");
    }
    assert!(g.notes[0].starts_with("Run-off of -2.00% a month"), "{}", g.notes[0]);
}

#[test]
fn f1_g_trend_and_seasonality_recovers_a_noise_free_pattern() {
    let seas = [30.0, 10.0, -20.0, -40.0, -25.0, 5.0, 20.0, 45.0, 25.0, -10.0, -30.0, -10.0];
    let y: Vec<f64> = (0..48).map(|t| 1000.0 + 5.0 * t as f64 + seas[t % 12]).collect();
    let f = trend_seasonal(&y, 1, 12).expect("model");
    for k in 0..12 {
        let t = 48 + k;
        close(f.point[k], 1000.0 + 5.0 * t as f64 + seas[t % 12], 1e-9, "noise-free");
    }
    assert!(trend_seasonal(&y[..20], 1, 3).is_err(), "24 months are needed");
}

#[test]
fn f1_h_a_spike_is_flagged_and_cleaned_and_a_clean_series_is_left_alone() {
    let reference = reference();
    let mut y = vec_of(&reference["y"]);
    assert!(flag_outliers(&y, 1).is_empty(), "{:?}", flag_outliers(&y, 1));
    let before = y[37];
    y[37] += 400.0;
    let flagged = flag_outliers(&y, 1);
    assert_eq!(flagged, vec![37]);
    let cleaned = clean_outliers(&y, 1, &flagged);
    assert!((cleaned[37] - before).abs() < 25.0, "{} vs {}", cleaned[37], before);
    assert_eq!(cleaned[10], y[10], "other months are untouched");
}

#[test]
fn f1_i_auto_back_tests_every_model_and_says_when_none_is_good() {
    let reference = reference();
    let y = vec_of(&reference["y"]);
    let h = Monthly::new(2022, 1, y.clone());
    let f = forecast_series("Deposits", &h, &Settings::default()).expect("forecast");
    assert_eq!(f.backtest_months, Some(12));
    assert_eq!(f.scores.len(), 4); // Phase 12 added ARIMA as the fourth
    let best = f.scores.iter().filter_map(|s| s.mape.clone().ok()).fold(f64::INFINITY, f64::min);
    assert!(best < 2.0, "a smooth seasonal series back-tests well: {best}");
    assert!(!f.not_good);
    assert_eq!(f.point.len(), 12);
    assert_eq!(f.label_ahead(0), "Jan 2027");
    // wild data: nothing is good, and it is said
    let wild: Vec<f64> = (0..40).map(|t| 100.0 + ((t * 7919) % 97) as f64 * 9.0).collect();
    let f = forecast_series("Wild", &Monthly::new(2022, 1, wild), &Settings::default()).expect("forecast");
    assert!(f.not_good && f.notes.iter().any(|n| n.contains("No model back-tests well")), "{:?}", f.notes);
}

#[test]
fn f1_j_overlay_moves_the_point_and_the_bands_and_stops_at_the_month_or_goes_on() {
    let reference = reference();
    let y = vec_of(&reference["y"]);
    let h = Monthly::new(2022, 1, y);
    let mut f = forecast_series("D", &h, &Settings { horizon: 6, choice: Choice::Fixed(ModelKind::Growth), ..Settings::default() }).expect("forecast");
    let base = f.clone();
    apply_overlays(&mut f, &[Overlay { year: 2027, month: 3, amount: 200.0, from_here: false, group: None }]).expect("overlay");
    assert_eq!(f.point[2] - base.point[2], 200.0);
    assert_eq!(f.bands[2][0] - base.bands[2][0], 200.0);
    assert_eq!(f.point[3], base.point[3], "one month only");
    let mut g = base.clone();
    apply_overlays(&mut g, &[Overlay { year: 2027, month: 3, amount: 200.0, from_here: true, group: None }]).expect("overlay");
    assert_eq!(g.point[5] - base.point[5], 200.0);
    let e = apply_overlays(&mut g, &[Overlay { year: 2028, month: 1, amount: 1.0, from_here: false, group: None }]).expect_err("past the end");
    assert!(e.to_string().contains("after the last forecast month"), "{e}");
}

#[test]
fn f1_k_groups_are_forecast_one_by_one_and_summed_and_a_failed_group_stops_the_total() {
    let reference = reference();
    let y = vec_of(&reference["y"]);
    let a = Monthly::new(2022, 1, y.clone());
    let b = Monthly::new(2022, 1, y.iter().map(|v| v * 0.5).collect());
    let s = Settings { horizon: 3, choice: Choice::Fixed(ModelKind::Growth), ..Settings::default() };
    let r = forecast_groups(&[("CASA".into(), a.clone()), ("FDR".into(), b)], &s, &[]);
    let total = r.total.expect("total");
    let (fa, fb) = (r.groups[0].as_ref().expect("a"), r.groups[1].as_ref().expect("b"));
    for k in 0..3 {
        close(total.point[k], fa.point[k] + fb.point[k], 1e-12, "sum of points");
        close(total.bands[k][3], fa.bands[k][3] + fb.bands[k][3], 1e-12, "sum of upper bounds");
    }
    assert_eq!(total.history.len(), 60);
    // a group with too little history stops the total and names itself
    let short = Monthly::new(2026, 8, vec![1.0, 2.0, 3.0]);
    let r = forecast_groups(&[("CASA".into(), a), ("New".into(), short)], &s, &[]);
    assert!(r.groups[0].is_ok() && r.groups[1].is_err());
    let msg = r.total.expect_err("no total");
    assert!(msg.contains("'New'") && msg.contains("at least 6"), "{msg}");
}

#[test]
fn f1_l_daily_rows_roll_up_to_month_end_or_average_daily_balance() {
    let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    let mut daily = BTreeMap::new();
    // Jan 2026: balance 100 from the 1st, 130 from the 11th; month ends on Saturday the 31st, last row Thursday the 29th? use the 31st
    daily.insert(d("2026-01-01"), 100.0);
    daily.insert(d("2026-01-11"), 130.0);
    daily.insert(d("2026-01-31"), 140.0);
    daily.insert(d("2026-02-02"), 150.0);
    daily.insert(d("2026-02-26"), 160.0); // 28 Feb is 2 days away: the month counts
    let (m, notes) = roll_up_daily(&daily, Basis::MonthEnd).expect("roll up");
    assert_eq!((m.year, m.month, m.values.clone()), (2026, 1, vec![140.0, 160.0]));
    assert!(notes.is_empty(), "{notes:?}");
    // average daily: Jan = (10 days x 100 + 20 days x 130 + 1 day x 140) / 31
    let (m, _) = roll_up_daily(&daily, Basis::AverageDaily).expect("average");
    close(m.values[0], (10.0 * 100.0 + 20.0 * 130.0 + 140.0) / 31.0, 1e-12, "January average");
    // Feb: 1 Feb carries 140 in; 2 Feb to 25 Feb at 150 (24 days) and 26 to 28 at 160 (3 days) -> (1x140 + 24x150 + 3x160) / 28
    close(m.values[1], (140.0 + 24.0 * 150.0 + 3.0 * 160.0) / 28.0, 1e-12, "February average");
    // a month with no data between two others is an error, never a zero
    let mut gap = BTreeMap::new();
    gap.insert(d("2026-01-31"), 1.0);
    gap.insert(d("2026-03-31"), 3.0);
    let e = roll_up_daily(&gap, Basis::MonthEnd).expect_err("gap");
    assert!(e.to_string().contains("Feb 2026 has no data"), "{e}");
    // a data set that stops mid-month leaves that month out and says so
    let mut part = daily.clone();
    part.insert(d("2026-03-10"), 170.0);
    let (m, notes) = roll_up_daily(&part, Basis::MonthEnd).expect("part");
    assert_eq!(m.len(), 2);
    assert!(notes.iter().any(|n| n.contains("Mar 2026 is left out")), "{notes:?}");
}

#[test]
fn f1_m_student_t_quantiles_match_the_tables() {
    // t(0.975, 10) = 2.228138852, t(0.9, 10) = 1.372183641, t(0.975, 22) = 2.073873068, t(0.975, 1) = 12.7062047
    close(t_quantile(0.975, 10.0), 2.228138852, 1e-8, "t95 df 10");
    close(t_quantile(0.9, 10.0), 1.372183641, 1e-8, "t80 df 10");
    close(t_quantile(0.975, 22.0), 2.073873068, 1e-8, "t95 df 22");
    close(t_quantile(0.975, 1.0), 12.7062047, 1e-6, "t95 df 1");
    close(t_quantile(0.975, 1000.0), 1.962339, 1e-5, "t95 df 1000");
}

// ------------------------------------------------------------------ F2: the forecast on a chart

use micro_parquet::charts::data::ChartData;
use micro_parquet::charts::spec::{ChartKind, ChartSpec};
use micro_parquet::forecast_chart::*;

fn line_data(values: &[f64], series: &str) -> ChartData {
    let m = Monthly::new(2023, 1, values.to_vec());
    let n = values.len();
    ChartData {
        spec: ChartSpec { kind: ChartKind::Line, ..Default::default() },
        categories: (0..n).map(|i| m.label(i)).collect(),
        series: vec![series.to_string()],
        values: vec![values.iter().map(|v| Some(*v)).collect()],
        rows: vec![vec![1; n]],
        kept_categories: (0..n).map(|i| m.label(i)).collect(),
        kept_series: vec![series.to_string()],
        ..Default::default()
    }
}

#[test]
fn f2_a_forecast_extends_the_line_with_bands_and_says_estimate() {
    let y = vec_of(&reference()["y"]);
    let d = apply_forecast(line_data(&y, "Deposits"), &ForecastSpec { horizon: 6, ..Default::default() }).expect("forecast");
    assert_eq!(d.categories.len(), 66);
    assert_eq!(d.categories[60], "Jan 2028");
    assert_eq!(d.series, vec!["Deposits", "Deposits forecast (estimate)", "80% low (estimate)", "80% high (estimate)", "95% low (estimate)", "95% high (estimate)"]);
    // history only on the first line; the forecast line starts at the last actual so the lines join
    assert_eq!(d.values[0][59], Some(y[59]));
    assert_eq!(d.values[0][60], None);
    assert_eq!(d.values[1][59], Some(y[59]));
    assert!(d.values[1][60].is_some() && d.values[1][65].is_some() && d.values[1][58].is_none());
    // the bands are ordered around the point
    for k in 60..66 {
        let (lo95, lo80, p, hi80, hi95) = (d.values[4][k].unwrap(), d.values[2][k].unwrap(), d.values[1][k].unwrap(), d.values[3][k].unwrap(), d.values[5][k].unwrap());
        assert!(lo95 < lo80 && lo80 < p && p < hi80 && hi80 < hi95, "{k}: {lo95} {lo80} {p} {hi80} {hi95}");
    }
    assert!(d.notes[0].starts_with("Forecast: an estimate"), "{:?}", d.notes);
    assert!(d.notes.iter().any(|n| n.starts_with("Back-test on the last 12 months")), "{:?}", d.notes);
    assert_eq!(d.rows.len(), d.values.len());
}

#[test]
fn f2_b_a_forecast_needs_a_line_with_consecutive_months_and_no_blanks() {
    let y = vec_of(&reference()["y"]);
    let mut bar = line_data(&y, "D");
    bar.spec.kind = ChartKind::Bar;
    assert!(apply_forecast(bar, &ForecastSpec::default()).unwrap_err().to_string().contains("line chart"));
    let mut skip = line_data(&y, "D");
    skip.categories.remove(10);
    skip.values[0].remove(10);
    let e = apply_forecast(skip, &ForecastSpec::default()).unwrap_err().to_string();
    assert!(e.contains("not one after the other"), "{e}");
    let mut blank = line_data(&y, "D");
    blank.values[0][20] = None;
    let e = apply_forecast(blank, &ForecastSpec::default()).unwrap_err().to_string();
    assert!(e.contains("no value for Sep 2024"), "{e}");
    let mut text = line_data(&y, "D");
    text.categories[3] = "Mid-year".into();
    assert!(apply_forecast(text, &ForecastSpec::default()).unwrap_err().to_string().contains("Month-Year categories"));
}

#[test]
fn f2_c_two_series_give_a_total_with_the_sum_of_the_forecasts_or_each_line() {
    let y = vec_of(&reference()["y"]);
    let mut d = line_data(&y, "CASA");
    d.series.push("FDR".into());
    d.kept_series.push("FDR".into());
    d.values.push(y.iter().map(|v| Some(v * 0.5)).collect());
    d.rows.push(vec![1; 60]);
    let fs = ForecastSpec { horizon: 3, model: ModelPick::Growth, ..Default::default() };
    let total = apply_forecast(d.clone(), &fs).expect("total");
    assert_eq!(total.series[0], "Total");
    let hist = total.values[0][59].unwrap();
    assert!((hist - y[59] * 1.5).abs() < 1e-6, "{hist}");
    let each = apply_forecast(d, &ForecastSpec { show: ForecastShow::EachSeries, ..fs }).expect("each");
    assert_eq!(each.series, vec!["CASA", "CASA forecast (estimate)", "FDR", "FDR forecast (estimate)"]);
    let sum = each.values[1][61].unwrap() + each.values[3][61].unwrap();
    assert!((sum - total.values[1][61].unwrap()).abs() < 1e-6, "total forecast is the sum of the two");
}

#[test]
fn f2_d_old_chart_json_has_no_forecast_and_a_forecast_round_trips() {
    let spec: ChartSpec = serde_json::from_str("{\"category\":\"Month\"}").expect("old json");
    assert!(spec.forecast.is_none());
    let mut with = spec.clone();
    with.forecast = Some(ForecastSpec { horizon: 9, overlays: vec![OverlaySpec { year: 2027, month: 3, amount: 2.0e9, from_here: false, series: String::new() }], ..Default::default() });
    let back: ChartSpec = serde_json::from_str(&serde_json::to_string(&with).unwrap()).unwrap();
    assert_eq!(back.forecast, with.forecast);
    assert_ne!(spec.calc_key(), with.calc_key(), "a forecast recalculates the chart");
    assert!(!serde_json::to_string(&spec).unwrap().contains("forecast"), "no field written when there is no forecast");
}
