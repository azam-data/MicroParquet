"""Generates tests/fixtures/phase12_arima_ref.json: the statsmodels reference numbers for src/arima.rs.

Run once (pip install statsmodels numpy scipy); the Rust test reads the JSON, so the build itself needs no Python.
statsmodels 0.15.0 is the version the numbers were taken from.
"""
import json, warnings
import numpy as np
import statsmodels
from statsmodels.tsa.arima.model import ARIMA

warnings.filterwarnings("ignore")
ref = json.load(open("tests/fixtures/phase11_forecast_ref.json"))
y = np.array(ref["y"])

rng = np.random.default_rng(12)
# a stationary ARMA(2,1) around 50, 80 months, simulated with fixed seed and written out so Rust reads the same numbers
e = rng.normal(0, 2.0, 120)
x = np.zeros(120)
for t in range(2, 120):
    x[t] = 0.5 * x[t - 1] - 0.2 * x[t - 2] + e[t] + 0.4 * e[t - 1]
b = 50 + x[40:]

out = {"statsmodels": statsmodels.__version__, "series": {}}

def case(name, s, d, p, q, h=6):
    w = np.diff(s, d) if d else s
    m = ARIMA(w, order=(p, 0, q), trend="c" if d < 2 else "n")
    r = m.fit()
    fc = r.get_forecast(h)
    names = r.param_names
    out["series"][name] = {
        "d": d, "p": p, "q": q, "y": list(map(float, s)),
        "param_names": names, "params": list(map(float, r.params)),
        "llf": float(r.llf), "aic": float(r.aic), "aicc": float(r.aicc),
        "forecast_w": list(map(float, fc.predicted_mean)),
        "se_w": list(map(float, fc.se_mean)),
    }
    # the exact likelihood at fixed, hand-chosen parameters (what the Kalman filter must reproduce)
    return r

case("growth_d1_11", y, 1, 1, 1)
case("growth_d1_01", y, 1, 0, 1)
case("growth_d1_20", y, 1, 2, 0)
case("arma21_d0", b, 0, 2, 1)
case("arma11_d0", b, 0, 1, 1)
case("arma10_d0", b, 0, 1, 0)

# fixed-parameter likelihoods (smooth): ARMA(2,1) and ARMA(1,2) with a constant
fixed = []
for (s, d, p, q, par) in [
    (b, 0, 2, 1, [50.0, 0.45, -0.15, 0.35, 4.5]),
    (b, 0, 1, 2, [49.5, 0.6, 0.3, -0.1, 5.0]),
    (b, 0, 0, 0, [50.3, 9.0]),
    (np.diff(y), 0, 1, 1, [3.0, 0.5, -0.3, 400.0]),
]:
    m = ARIMA(s, order=(p, 0, q), trend="c")
    fixed.append({"y": list(map(float, s)), "p": p, "q": q, "params": par, "llf": float(m.loglike(par))})
out["fixed"] = fixed

# level forecasts of the full ARIMA(1,1,1) with a drift on the growth series, for a loose cross-check
m = ARIMA(y, order=(1, 1, 1), trend="t")
r = m.fit()
fc = r.get_forecast(6)
out["level_111"] = {"mean": list(map(float, fc.predicted_mean)), "se": list(map(float, fc.se_mean)), "params": list(map(float, r.params))}
from statsmodels.tsa.stattools import adfuller
out["adf"] = {}
for k in ["growth_d1_11", "arma21_d0", "arma10_d0"]:
    s = np.array(out["series"][k]["y"])
    out["adf"][k + "_level"] = float(adfuller(s, maxlag=0, regression="c", autolag=None)[0])
    out["adf"][k + "_diff1"] = float(adfuller(np.diff(s), maxlag=0, regression="c", autolag=None)[0])
json.dump(out, open("tests/fixtures/phase12_arima_ref.json", "w"), indent=1)
for k, v in out["series"].items():
    print(k, v["param_names"], [round(x, 4) for x in v["params"]], round(v["llf"], 4))
