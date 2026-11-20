# Chart

## 1. What it does

Draws bars or a line of one number by one category, calculated over the whole tab after its steps (never just the rows on screen). Choose the category (X), an optional series (grouped bars, at most 8), the value and how to add it up (Sum, Count, Average, W.Avg, Min, Max). The x-axis shows the category names, the title says what was drawn and in which unit, and a tooltip gives the category, series, value and the number of rows behind it.

## 2. When to use it

- Balance by maturity bucket or term period for an ALCO pack.
- Number of accounts by branch or by classification grade.
- A weighted-average rate by product, to see which product is out of line.

## 3. Before you start

- Open the tab and apply the steps that shape it (filters, buckets). The chart reads the result.
- The value column must be a number (integer, decimal or whole number). If an amount is still Text, set it to a number in Schema & Ingestion first.
- If your categories have a fixed order (tenors, risk grades), make a list or use Tenor: see [custom sort lists](custom-sort-lists.md).

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Category (X) | Yes | Any | Term Period, Branch | Blank values are one category called (blank), always last. |
| Series | No | Any | Risk grade | More than 8 values: the 7 largest and "Other". |
| Value | For every aggregation except Count | Number | Balance, Rate | A Text, Date or other non-number column is refused with its type. |
| Weight | For W.Avg | Number | Balance | Rows without a value or a weight are left out; weights of both signs give a blank rate. |

## 5. Step by step

1. Click the "Chart" tab of the side panel on the right (or the Chart button of the toolbar, or Ctrl+K: "chart"). If the tab is not there, tick View ▸ Side panel ▸ "Chart". "Pop out" (where the app can open a second window) shows the chart in its own window, which you can move to another monitor; "Dock back" or closing that window returns it to the panel. The controls are in two columns in the narrow panel; in a narrow chart the x-axis names every second (or third…) category, and the tooltip has every name whole.
2. Choose "Category (X)". Optionally choose a "Series".
3. Choose "Aggregation" and, except for Count, the "Value" (for W.Avg also "by": the weight).
4. "Type" is Bar or Line.
5. "Order" is Natural, one of your lists, Tenor, or By value (largest first).
6. "Top N" keeps that many categories; the rest are added into "Other" (the totals stay whole). The default is 30.
7. "Scale" follows the value column's display format (Crore, Lakh, Million, Thousand or plain) unless you choose another.
8. "Open chart data as tab" puts the table behind the chart in a new tab, which can be exported to Excel or PDF.
9. The choices are kept for the tab (by its file name) for the next session.
10. "Export to HTML…" (also Export ▸ "Chart to HTML…") writes the chart as one web page. It has the title, subtitle and footnote, a Table switch, a light/dark switch and a toolbox with "Save as PNG", "Save as SVG" and "Data view". The page works offline and contains no web address: send it by e-mail or put it on a share.
11. "Preview in browser" writes the same page to a temporary file and opens it with your default browser. Sankey and Treemap are drawn only this way (the chart in the app says so).
12. "Advanced options (HTML only)" is a box for JSON that is merged over the page's ECharts option, for what the controls cannot do. It is checked as you type (a mistake is shown with its line and column); "Reset" clears it. The chart in the app and the Excel chart never use it.
13. "Export to Excel…" (also Export ▸ "Chart to Excel…") writes a workbook: the chart's numbers on a sheet and a native Excel chart beside them (editable in Excel and Power BI), in the same palette and Segoe UI. A waterfall is a stacked column with an invisible base series; a heatmap is the table under a three-colour scale; linked panels and small multiples are one chart each, one under the other; a bubble chart is written as a scatter, a stepped line as an ordinary line, and Sankey and treemap only as data, each with a note on the sheet.
14. Charts ▸ "Chart this result" (or Charts ▸ New chart ▸ "Banking templates") charts a tool's result the way a banker would draw it: the gap ladder, repricing gap, NII bridge (MoM variance), CL migration, IRRBB scenarios, LCR / NSFR, ECL by stage, data-quality scorecard, yield curves, rate profile, balance history, deposit / loan mix and classified trend. The chart's own table opens as a tab, the title is a finding worked out from the numbers by fixed rules (for example "Parallel up is the worst case at −10.5% of Tier 1"), and every setting stays editable; a takeaway you type replaces the finding.
15. Charts ▸ "Chart book (HTML)…" puts several saved charts of the active tab on one page: a title, the date and a two-column grid.
16. "Copy as Python" puts a script on the clipboard: the chart's numbers as a pandas DataFrame and matplotlib code in the same style (fonts, palette, marks, title), so you can carry on in Python (`pip install pandas matplotlib`). A Sankey and a treemap use plotly instead (`pip install plotly pandas`) and the script says so. The numbers are the chart's own table in the chart's scale; a flag at the top (`DARK`) switches the theme.
17. A report pack can carry a chart on any of its items (see "Report packs"): the workbook gets the native chart on a sheet beside the item's, and the pack can write an HTML chart book.

## 6. What you get

A chart whose title reads like "Sum of Balance by Term Period: all 250 rows (Crore)". The x-axis names the categories (a long name is cut with "…", the tooltip has it whole). With Top N below the number of categories the title adds "top 3 of 5 categories". The table tab has one row per category (and series) with the value and "Rows".

## 7. Check your result

- The "all N rows" in the title is the row count of the tab: compare it with the status bar.
- The bars add up to the Sum in the Summary row of the tab (Top N keeps the total in "Other").
- Open the chart data as a tab and compare one line with a Pivot of the same fields.

## 8. Worked example

Input: eight loans.

| Term Period | Balance |
| --- | ---: |
| 6m | 500,000,000.00 |
| 1y | 300,000,000.00 |
| 1m | 100,000,000.00 |
| 100D | 50,000,000.00 |
| 3m | 200,000,000.00 |
| 1y | 100,000,000.00 |
| 6m | 250,000,000.00 |
| 1m | 80,000,000.00 |

Test: `guide_chart`

Sum of Balance by Term Period, Order Tenor, Top N 3: the three largest terms in term order, and the rest (1m and 100D) as "Other". The total is unchanged.

| Term Period | Sum of Balance | Rows |
| --- | ---: | ---: |
| 3m | 200,000,000.00 | 1 |
| 6m | 750,000,000.00 | 2 |
| 1y | 400,000,000.00 | 2 |
| Other | 230,000,000.00 | 3 |

Test: `guide_chart`

## 9. Common messages and fixes

- "Chart: Sum needs a number column, but 'Risk' is Text. …" Choose a number column, or Count.
- "Chart: choose the Value column for Sum …" Pick the value, or choose Count.
- "Chart: W.Avg needs a weight column." Choose one (usually Balance).
- "N group(s) have weights of both signs, so their weighted average is left blank." Check the sign of the weights.
- "The 5 series after the first 7 are drawn together as Other." More than 8 series: filter first to see them.

## 10. Limits and assumptions

- Top N ranks categories by the aggregated value, largest first; "Other" is calculated again from its own rows (an average of the rest is the average of its rows, not of averages).
- A category called "Other" in the data is kept apart from the rest ("Other categories").
- Count counts rows; the value column is not used.
- On a pivot table the subtotal and Grand total rows are not charted.
- The HTML page carries Apache ECharts 6.1.0 inside it (about 1 MB); a chart of 1,000 points is under 2 MB. A donut with more than 5 parts, a waterfall that does not add up and invalid advanced JSON stop the export with a message, and no file is written.
- The first chart of a tab picks the first text column and the first number column; change them as needed.
