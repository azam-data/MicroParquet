# Phase 12 decisions

Built autonomously; each choice below was made without asking. Rates are in percent, shocks in basis points, money in BDT shown in Crore.

## Target bands (T1–T4)
- A target is a fixed number (in the card's unit) or a Phase 11 plan measure on the same tab. Fixed numbers are typed in the unit shown (Crore by default), stored as typed.
- Bands are margins around the target as percentages of it: green at or on the good side, amber within 10% (default), red beyond. The good direction (up or down) is set per target. A card or chart never colours by hue alone: status is also written in words.
- Visuals: Number (default), Bullet, Gauge. Drawn in the app, PowerPoint (native shapes), PDF and HTML. Excel charts get the target as a line; the coloured zones do not reach Excel (a chart-library limit), the line label says the target.
- Charts: a neutral target line plus three tinted bands (alpha 0.16) on column, bar, line and area charts.

## Rolling and moving measures (W2)
- Windows: Rolling 3M and Rolling 12M join MTD, QTD, YTD. YTD is the running total that resets each fiscal year; the fiscal year starts in January (Bangladesh banks report on the calendar year).
- A window that would reach before the first data is blank with a note, never a short window.
- Moving average (3, 6, 12 periods) is the mean of that many periods, each worked out as the measure would be. It is blank when a flow period is unfinished or reaches before the data. It cannot be combined with a shift or a non-Period window; the app says so.
- KPI cards take a window, an average and the 12-month sparkline.

## What-if (W4)
- Scenarios: Base (0/0), Optimistic (+5% growth), Stressed (−10% growth), plus your own. The starter sizes are assumptions to edit, not regulatory numbers. The IRRBB parallel shock is copied from the rule file in force at the as-of date by a button, never typed in the code.
- A measure is marked Volume (multiplied by 1 + growth) or Rate (shift added; 100 bp = 1 percentage point). Only amounts can be Volume, only ratios shown in percent can be Rate, plan measures cannot be marked; a wrong mark is refused with a plain message.
- Only the as-of month and later change; closed history stays actual. A window that reaches the as-of month is changed as a whole.
- Every output that shows a changed number carries "Scenario NAME: … These are not actual figures." KPI cards add the scenario to the period line.
- Forecasts are fitted to actuals only; the scenario is applied afterwards to the as-of month and the forecast months.
- Scenarios live in the project (old projects load with none). Renaming or removing a measure updates the marks.

## ARIMA (A1)
- Fourth model in the forecast (Auto's back-test includes it). Plain Rust: exact Kalman likelihood, differencing chosen by a Dickey-Fuller test, orders by AICc over a small grid, Nelder-Mead fit, 80% and 95% bands from psi weights.
- Checked against statsmodels 0.15 reference numbers (fixtures in tests/fixtures, generator in tools/): log-likelihood to 1e-9, forecasts to 1e-6 on the differenced scale, level forecast within 1%. Needs 12 months of history, else it refuses.
- No new dependency.

## Known limits
- Excel charts: target line yes, zones no.
- What-if marks are model-wide, not per dashboard.
