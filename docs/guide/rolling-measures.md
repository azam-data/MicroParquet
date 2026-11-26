# Rolling and moving measures

## 1. What it does

Adds two more windows to the time choices of a pivot, chart or KPI card: **Rolling 3M** and **Rolling 12M** (the calendar months ending with the row), alongside MTD, QTD and YTD (the running total that starts again each fiscal year; the fiscal year starts in January unless you changed it). It also adds a **moving average** of 3, 6 or 12 periods.

## 2. When to use it

- "Interest income of the last twelve months" on each month-end row of an ALCO page.
- Smoothing a noisy monthly series (daily inflow, fees) with a 3 or 6 month average.
- A KPI card that shows a rolling 12 month figure with its 12-month sparkline.

## 3. How to use it

1. Link the date column and set the as-of date ([Shared calendar](model-calendar.md)).
2. In the Pivot dialog, on a value of a date-linked tab, press **Time** on its chip. Under **Window** choose Period, MTD, QTD or YTD; under **Rolling** choose Rolling 3M or Rolling 12M; under **Average** choose None, 3, 6 or 12. In the chart studio the same choices are under the aggregation ("Moving average").
3. Put a Calendar field that names one month (Month-Year) in Rows or Columns, so each row is a month.
4. KPI cards take a window, an average and the 12-month sparkline in their settings.

## 4. Example

Monthly flow of 10, 20, 30, 40 for January to April (all the data). Rolling 3M: April 20 + 30 + 40 = 90, March 10 + 20 + 30 = 60; January and February are blank. Moving average of 3 periods: April (20 + 30 + 40) / 3 = 30, March 20; January and February blank.

## 5. What it refuses and why

- A window that would reach before the first data is **blank with a note**, never a short window.
- A moving average is blank when a flow period is unfinished or reaches before the data.
- A moving average cannot be combined with a shift (prior period, same period last year) or a non-Period window: the app says so.
- A Period window with no period field: "Add a Calendar field that names a period…" ([Time intelligence](time-intelligence.md)).

## 6. Python export

Refused: "Python export can't follow links between tabs yet."

## 7. Saved in

With the pivot, chart or card value. A value that uses none of them writes none, so old files are unchanged.

## 8. Related tools

[Time intelligence](time-intelligence.md) · [Balances and flows](balances-and-flows.md) · [Measures and ratios](measures-and-ratios.md) · [KPI cards](dashboard-kpi-cards.md)
