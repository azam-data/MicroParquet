# ARIMA forecast

## 1. What it does

The fourth model of the [Forecast](forecasting.md): ARIMA (a model that predicts the next month from the last few months and their changes). MicroParquet chooses how many times to difference the series (a Dickey-Fuller test), then the orders by the lowest AICc over a small grid, fits them, and draws 80% and 95% bands. It is plain Rust, offline, and gives the same result for the same data.

## 2. When to use it

- A series with a clear, steady drift and little seasonality, where Holt-Winters over-fits.
- As a check: if ARIMA and Holt-Winters agree, the forecast is more believable.
- With **Auto**, ARIMA is already in the back-test, so you do not need to pick it yourself.

## 3. How to use it

1. Draw the monthly line chart and open Analytics ▸ Forecast ([Forecasting](forecasting.md), steps 1 to 4).
2. Under **Model**, choose "ARIMA" (or "Auto (best back-test)").
3. Read the note under the chart: it gives the back-test error and says when the result is only a rough guide.

## 4. Example

A branch's term deposits for 18 months, climbing about 20 Cr a month with no seasonal pattern. Trend + seasonality and ARIMA both continue the climb. If Auto picks ARIMA, the chart's note names it and gives its error.

## 5. What it refuses and why

- Fewer than 12 months of history: refused ("needs 12 months").
- A poor back-test is marked as a rough guide, as for every model.

## 6. Python export

Not a table step: not in a table's Python export.

## 7. Saved in

With the chart's forecast setting (the model choice).

## 8. Related tools

[Forecasting](forecasting.md) · [What-if scenarios](what-if-scenarios.md) · [Chart](chart.md)
