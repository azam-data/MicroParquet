# Forecasting a monthly line

## 1. What it does

Extends a line chart of month-end (or average) balances by 3 to 12 months, with an 80% and a 95% band. It offers four models: **Trend + seasonality**, **Holt-Winters**, **Growth / run-off** and **ARIMA** (see [ARIMA forecast](arima-forecast.md)), or **Auto**, which tries each on the earlier months, holds out the last 6 or 12, and picks the one with the lowest error. The forecast is always an **estimate**; it is drawn as a continuation of the line with its bands and is never mixed into actuals.

## 2. When to use it

- ALCO or budget discussion: where will deposits be in six months if the pattern holds?
- Run-off of a closing book (a scheme with no new disbursement).
- A first view of the seasonal pattern (Eid, year-end) in a balance.

## 3. How to use it

1. Open a table with a date column and a balance, link the date column in the model if you use the Calendar fields ([Shared calendar](model-calendar.md)).
2. Report view ▸ Chart tab: draw a **line** chart with a Month-Year Calendar field on the axis and a balance measure (Closing or Average balance). Daily data is rolled up to months by the chart's own aggregation; the forecast starts the month after the last full month.
3. Open the **Analytics** tab of the side panel, card **Forecast**. Tick "Add a forecast to this chart". If the chart is not a line, "Make it a line chart" does that.
4. Choose **Months ahead** (3 to 12) and the **Model** (Auto is a good first choice).
5. With a series (Wing, Product), choose "Forecast each, show the total" or "Forecast and show each".
6. Tick "Set unusual months aside before fitting" if a month was one-off (a large one-day deposit). It is replaced by what its neighbours say for the fit only; the chart still shows the real value, and the unusual months are listed.
7. For a known future inflow or outflow, "Add an adjustment": an amount in Crore (+200 Cr in March), for one month or "and after" (a level shift). The model line is still drawn so the effect of the adjustment is visible.
8. The forecast goes with the chart: HTML, PDF, PowerPoint, Excel and the dashboard show it too.

## 4. Example

Twenty-four months of closing deposits, rising with a rise every December. Auto holds out the last 12 months, fits the four models on the first 12 and reports, say, Holt-Winters 3.1% error and Trend + seasonality 4.4%: Holt-Winters is chosen and drawn with its 80% and 95% bands. If every model were more than 10% off on average, the chart would say "No model back-tests well; treat this as a rough guide".

## 5. What it refuses and why

- Fewer months than the model needs (ARIMA needs 12 months): refused with the number.
- A poor back-test is never hidden: the forecast is shown but marked as a rough guide.
- A group forecast adds the bounds of its parts, so the combined band is wider than a band fitted to the total. A wider band is the safe error.
- The forecast is fitted to actuals only. A what-if scenario is applied afterwards ([What-if scenarios](what-if-scenarios.md)).

## 6. Python export

The forecast belongs to the chart, not to a table step, so a table's Python export does not contain it.

## 7. Saved in

In the chart's settings: in the project, in saved charts, and in the chart tab. A chart saved without a forecast loads without one.

## 8. Related tools

[ARIMA forecast](arima-forecast.md) · [What-if scenarios](what-if-scenarios.md) · [Target bands](target-bands.md) · [Chart](chart.md) · [Time intelligence](time-intelligence.md)
