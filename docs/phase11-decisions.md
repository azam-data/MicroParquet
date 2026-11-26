# Phase 11 decisions

Taken without asking the owner, who asked for the list at the end of the phase. Newest last. Phase 11 = file-open column choices, variance bridge, forecasting, budget and target tables. Moved to Phase 12 (agreed in the thread): target bands with bullet and gauge visuals, rolling and moving measures, what-if sliders, ARIMA.

## S1 Keep/drop and reverse sign (file-open schema dialog)
- S1-1 The owner wrote "rows" for the keep/drop boxes; the schema dialog's rows are the file's columns, so each column row got a **Keep** box. Dropped columns are removed after the renames, before any step, so every later screen sees the narrower table.
- S1-2 A file with every column dropped is refused with a plain message (not an empty table).
- S1-3 **Reverse sign** multiplies a number column by -1 (advance balances arrive negative). A zero stays 0 (no -0). It is refused on a column that is not a number: no silent cast.
- S1-4 Both options are saved with the schema (new fields use `#[serde(default)]`, so old files and templates load), copied by schema templates, and written into the Python export.

## F1 Forecasting (Report view, Analytics pane)
- F1-1 Forecasting rides on the chart's own time aggregation (a Month-Year Calendar field with Closing or Average balance), so it is a setting of a chart (`ChartSpec.forecast`), not a new tab type. The forecast is added to the chart data, so HTML, PDF, PPTX, Excel and the dashboard all show it with no extra work.
- F1-2 Daily data is rolled up to month-end or average daily balance by the chart's own aggregation before fitting; the forecast starts the month after the last full month.
- F1-3 Models: trend plus seasonality (classical decomposition with a straight-line trend; default for deposits), Holt-Winters (additive trend and season, parameters by a fixed grid then a coordinate search; no random numbers, so the same data always gives the same forecast), and growth/run-off (a straight-line or compound path, for a book that runs off). Written in plain Rust; no new crates.
- F1-4 Bands: 80% and 95%, from the model's own residual spread (student-t quantiles for the straight-line model, so short histories get wider bands). A forecast is always labelled "estimate".
- F1-5 Auto mode holds out the last 6 or 12 months, fits each model on the earlier months, and shows each model's MAPE. When the best model is still off by more than 10% on average (the limit is a setting), the forecast is shown but marked "No model back-tests well ... treat this as a rough guide"; it never passes a poor forecast off as a good one.
- F1-6 Outliers are flagged before fitting (modified z-score above 3.5 on the deseasonalised series, and at least 5% of the level). They are listed; the user chooses to keep them or replace them with the fitted value.
- F1-7 Group forecasts (per product or wing, then summed) add the bounds of the parts, which is wider than a combined band; a wider band is the safe error.
- F1-8 Manual overlay ("+200 Cr in March"): an amount added to one forecast month, or from that month onwards ("from here", a level shift for a balance). It can be for one series or all. The model line is still drawn so the effect of the overlay is visible.
- F1-9 Maths is tested against statsmodels reference results pinned in `tests/fixtures/phase11_forecast_ref.json` (OLS bands, decomposition, Holt-Winters fit, MAPE).

## B1 Variance bridge
- B1-1 The bridge is a MoM view (third tab "MoM bridge") and reuses the Phase 6 data; Phase 6 numbers are unchanged. It splits the change of balances, NII or cost of funds into **volume**, **rate** and **mix**.
- B1-2 Identity (exact, checked on every run; a gap is an error, not hidden): volume = (total balance now - before) x the average rate before; mix = sum of (balance now x own rate before) - total balance now x the average rate before; rate = sum of balance now x (rate now - rate before). They add to the change in interest with nothing left over. The Phase 6 "Mix effect" column is a different (joint) split and is left as it was; the tab says which formulas it uses.
- B1-3 The waterfall is the existing waterfall chart (the NII bridge template, extended), so every export works.
- B1-4 A product that is new (no balance before) is taken to have had the average rate before, so its rate gap is in the rate effect (B-2). A closed product has nothing in the rate effect.
- B1-5 Refused with a plain message: a negative balance (use Reverse sign in the schema first), interest with no balance, or no balance at all in the earlier period.

## P1 Budget and target tables
- P1-1 A plan table is linked to a data tab in **Model > Links** (a section under the links): a month column and 1 to 3 key pairs (for example Product). It is a different thing from a lookup link, because a plan has one row per month and key, and a lookup must have unique keys.
- P1-2 Plan measures are a third kind in **Model > Measures** ("Add budget / target"): pick the plan table, the amount column, how months add up, and what to show: the plan, **actual less plan**, or **% achieved** (actual over plan, in percent). They are used like any other measure, so pivots, charts, KPI cards and dashboards work with no further change.
- P1-3 Months add up as **Sum** (a flow budget: income, cost) or **Last month** (a closing target). A month that has started counts in full: a month-to-date actual is set against the whole month's plan.
- P1-4 The month column is a date; any day of the month will do (1st or 15th or month-end).
- P1-5 Fail loudly: a repeated month and key is refused when the link is added and again when the pivot is built (never summed); a blank month or key is refused; a month the plan has no row for makes the cell blank with the reason ("The plan has no row for B in Feb 2026"), never a smaller sum or a zero; a plan of 0 gives no % achieved.
- P1-6 A plan can only be split by its own keys. A pivot split by anything else, or filtered by a column that is not a plan key, is refused with the keys named. A filter on a plan key (Product = A) applies to the plan as well as the actual.
- P1-7 Actual less plan is not signed by favourability: a cost above budget shows a positive number. The sign of each base measure (Plus or Minus) is the one already set, and the plan amount is used as it is in the plan table (enter budgets with the same sign convention as the measure).
- P1-8 The actual is a base measure or a ratio of the same tab; a plan measure cannot be set against another plan measure or used as a ratio term. A measure that is the actual of a plan measure, or a plan table that a plan measure reads, can't be removed until those are.
- P1-9 The plan table must stay open as a tab in the project (as lookup tabs do); its own steps (filters, renames) apply before it is read.
