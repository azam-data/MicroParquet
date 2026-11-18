# Summary row

## 1. What it does

The band under the column names that totals every column over the whole result (not just the rows on screen): sum, average, count, minimum, maximum, count distinct or weighted average. It is also written under the data in Excel and PDF exports, so the printed total is the one you saw.

## 2. When to use it

- A quick total and weighted average rate of the book on screen.
- Checking a control total after filters and joins.
- The totals row of an Excel or PDF export.

## 3. Before you start

- Open the tab. Pin the control column (usually Balance) so rate columns default to a weighted average by balance.
- For a weighted average pick the weight column (a number).

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Column | Yes | Any column | Balance, Int Rate | Each column has its own choice. |
| Summary | Yes | Choice | Sum, W.Avg, Count | Defaults: numbers Sum, text Count, dates Min; rate-like numbers W.Avg by the control column. |
| Weight column | For W.Avg | Number | Balance | Weights of mixed signs give no result: a rate of +1000 and −999 is not meaningful. |

## 5. Step by step

1. Open the tab. Look at the selector row under the column names (it shows the current summary, for example "Sum").
2. Click the selector of a column and choose None, Count, Count distinct, Sum, Average, Min, Max or W.Avg. For W.Avg also choose the weight column.
3. Read the value in the dark band under the selectors. Hover it for the reason when it shows "—".
4. To hide the summary row (the selectors and the dark band) and give the table more room, press Ctrl+K and choose "Show/hide summary row" (or untick "Summary row" in the View menu). Run it again to bring the row back; your choices are kept while it is hidden.
5. Export to Excel or PDF: the choices are carried into the export. In the PDF the band is aligned with the columns above it and, like the table, carries no unit (the unit is stated once under the title).

## 6. What you get

One value per column: counts and sums with thousands separators, dates as YYYY-MM-DD, amounts in the chosen scale (Crore, Lakh and so on). "—" means there is no value (hover for the reason). Blank cells are skipped in sums, averages, minimum and maximum; text that is empty or only spaces does not count as a value.

## 7. Check your result

- Sum of the control column equals the control total in the status bar.
- Weighted average rate = sum(rate × balance) / sum(balance) over rows where both are known: check once by hand.

## 8. Worked example

Input: three loans.

| Actnum | Balance | Int Rate |
| --- | ---: | ---: |
| A1 | 1,000.00 | 10.00 |
| A2 | 3,000.00 | 8.00 |
| A3 | 2,000.00 | 6.00 |

Test: `guide_summary_row`

Output: the summary choices and values. The weighted average rate is (1,000 × 10 + 3,000 × 8 + 2,000 × 6) / 6,000 = 7.6667, not the simple average 8.

| Column | Summary | Value |
| --- | --- | ---: |
| Actnum | Count | 3 |
| Balance | Sum | 6,000.00 |
| Int Rate | W.Avg by Balance | 7.6667 |

Test: `guide_summary_row`

## 9. Common messages and fixes

- "—" with a hover note: the choice does not fit the column (for example Sum of text) or the weights have mixed signs. Choose another summary.
- The band follows the steps: after a pivot or join the columns are different; pick the choices again.

## 10. Limits and assumptions

- One summary per column, computed over the whole result up to the last valid step.
- The summary is not a step and is not replayed by macros, but it is saved with them.
