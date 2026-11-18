# Pivot table (Rows, Columns, Values, Filters)

## 1. What it does

Summarises a table the way Excel's pivot does: the Rows you pick go down the side, the Columns across the top, and each Values field is added up (or counted, averaged, weighted and so on) in every cell, with row totals, column totals and a grand total. It turns an account-level extract into the ALCO tables: balance by bucket and bank, weighted average rate by product, share of each bucket in the book.

## 2. When to use it

- Balance by maturity bucket and bank (the SLP or LCR table).
- Weighted average deposit cost or lending yield by product or branch.
- Number of accounts and distinct customers by segment.
- Each bucket's share of the book (% of row, column or grand total).

## 3. Before you start

- Open the extract and add the steps that prepare it (types, Maturity bucketer, Rate profile, Filter).
- Amounts and rates must be numbers (Schema & Ingestion). Rates are in percent.
- Pin the control column (Balance) in the status bar if you want rates to default to a weighted average.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Rows fields | At least one of Rows or Columns | Any column | Maturity_Bucket, Branch | Numbers and dates keep their order (9 before 10; January before February); text keeps its natural order, so 02. comes before 10. and Zone 2 before Zone 10. |
| Columns fields | See Rows | Any column | Bank, Currency | A field is in Rows or in Columns, not both. At most 500 columns. |
| Values fields | Yes | Number for Sum, Average, Min, Max, W.Avg; any column for Count and Count distinct | Balance, Int Rate, Account No | Each Values field has its own aggregation. W.Avg needs a number weight column (usually Balance). |
| Filters fields | No | Any column | Deposit Type = FDR | Tick the values to keep; the pivot uses only those rows. |
| Totals, Show values as, Scale | No | Choices in the dialog | Row totals, % of row total, Crore | % needs Sum or Count values. |

## 5. Step by step

1. Open the tab and click the Pivot button on the Transform group of the toolbar (or press Ctrl+K and type "pivot"). The dialog "Pivot table" opens; its title line says which tab and step it works on.
2. Find fields in the list under FIELDS (type "Search N fields…" to narrow it). The badge shows the type: Aa text, # number, D date.
3. Drag a field into Rows, Columns, Values or Filters, or click the three dots beside the field and choose "Add to Rows", "Add to Columns", "Add to Values" or "Add to Filters". The cross on a chip removes the field.
4. For each Values field choose the aggregation on its chip: Sum, Count, Count distinct, Average, Min, Max or W.Avg (then choose the weight column after "by").
5. For a Filters field click its chip and tick the values to keep (type in "Search values…" to find one). Everything ticked means no filter.
6. Set "Row totals", "Column totals" and "Subtotals" (subtotals need two Rows fields).
7. Choose "Show values as": No calculation, % of row total, % of column total or % of grand total.
8. Choose "Scale" (As in View, Plain, Thousand, Lakh, Million, Crore). It only changes how the value columns are shown; the numbers stay in BDT. In an Excel export Thousand and Million are a number format (the cell keeps the full BDT amount, so Excel sums the real figures), while Lakh and Crore divide the amount and put the unit in the header ("Sum of Balance (BDT Cr)"). Plain keeps BDT, and rates and counts are never scaled. CSV and Parquet exports keep the BDT amounts. In a PDF export the unit is written once under the title ("Figures in BDT Crore") and no header or value carries it; each column is as wide as its widest header or value, numbers and their headers are right-aligned, a long header wraps to two lines, the bank names of a pivot with Columns span their value columns, the Grand total row is bold under a rule, and columns that do not fit the page width continue on following pages with the row labels repeated.
9. Choose "Sort rows": Natural order (the default) or by one Values field, high to low or low to high.
    To order the labels of one Rows or Columns field by a custom list or by Tenor, click the three dots beside it and choose "Order: Natural / <list> / Tenor / By value" (see [custom sort lists](custom-sort-lists.md)); a list whose "Suggested for columns" names the field is chosen for you. The chip then shows "order: <list>".
10. Check the Preview (the first 20 rows, computed while you work). A red line at the bottom says why there is no table.
11. Click "Create pivot tab". The pivot is a step ("Pivot") of a new tab named "Pivot: " and its first Rows field (for example "Pivot: Segment"). Double-click the tab name to rename it; a name longer than 24 characters is cut with "…" in the strip and shown in full when you point at it. To change it later click "Edit pivot" in the bar above the table (or select the step in the Steps tab and click Edit): the dialog opens with the current Rows, Columns, Values, Filters and Scale, and "Save changes" updates the same tab in place (no new tab; Undo takes it back).
12. Filters: a field in the Filters box limits the pivot to the values you choose; the empty box says "Drop a field here to limit the pivot to chosen values". Rows that are not chosen are removed before anything is added up, so every cell and total (a weighted rate too) is worked out from the chosen rows only, and the filter field does not appear as a row or a column (put it in Rows too if you want to see it). Above the pivot, in the bar next to "Edit pivot", each Filters field has a picker such as "Segment: All". Click it, type in "Search values…" to find a value, and tick the values to keep: the pivot changes at once, and the picker reads "Segment: CMSME" or "Segment: CMSME (+2)" (the first chosen value and how many more). Tick "All" to remove the limit. The filter is saved in the step, so a saved macro repeats it.

## 6. What you get

A table with the Rows fields as label columns (one column per field), then one value column for each column label (and each Values field: "Islami | Sum of Balance" when there are several), then the Total columns when Row totals is ticked, and a last "Grand total" row when Column totals is ticked. With Subtotals each outer label has a line "A Total". Blank labels show as "(blank)" and come last. Totals are worked out from the data, not added up from the cells, so an average, a weighted average, a minimum or a distinct count has a true total. A W.Avg cell is blank when the weights in it have both signs (a rate of +1,000 and −999 is not meaningful); the status bar counts such cells. Counts are whole numbers. With "% of …" each cell is the share in percent (2 decimals).

## 7. Check your result

- The Grand total of Sum of Balance equals the sum of the balance column of the tab before the pivot (the control total).
- The row totals add up to the Total column's grand total; the column totals add up to it too.
- With "% of row total" each row's cells add up to 100.00 (rounding ±0.01).
- A filtered pivot's grand total is the sum of the rows kept: compare with a Filter on the same values.

## 8. Worked example

Input: seven deposits by bucket and bank.

| Bucket | Bank | Balance | Rate |
| --- | --- | ---: | ---: |
| 02. 2–7 Days | Conventional | 100.00 | 5.00 |
| 10. 3–5 Years | Islami | 300.00 | 9.00 |
| 04. 1–3 Months | Conventional | 200.00 | 6.00 |
| 02. 2–7 Days | Islami | 50.00 | 8.00 |
| 04. 1–3 Months | Islami | 150.00 | 7.00 |
| 10. 3–5 Years | Conventional | 400.00 | 10.00 |
| 02. 2–7 Days | Conventional | 100.00 | 7.00 |

Test: `guide_pivot`

Rows = Bucket, Columns = Bank, Values = Sum of Balance. The buckets are in time order (02, 04, 10), not by size, although 10 is the largest. The Grand total row equals the column sums and the overall sum, 1,300.

| Bucket | Conventional | Islami | Total |
| --- | ---: | ---: | ---: |
| 02. 2–7 Days | 200.00 | 50.00 | 250.00 |
| 04. 1–3 Months | 200.00 | 150.00 | 350.00 |
| 10. 3–5 Years | 400.00 | 300.00 | 700.00 |
| Grand total | 800.00 | 500.00 | 1,300.00 |

Test: `guide_pivot`

The same pivot shown as % of row total: every bucket's cells add up to 100.00.

| Bucket | Conventional | Islami | Total |
| --- | ---: | ---: | ---: |
| 02. 2–7 Days | 80.00 | 20.00 | 100.00 |
| 04. 1–3 Months | 57.14 | 42.86 | 100.00 |
| 10. 3–5 Years | 57.14 | 42.86 | 100.00 |
| Grand total | 61.54 | 38.46 | 100.00 |

Test: `guide_pivot`

Values = W.Avg of Rate by Balance: in 02 / Conventional the two deposits (100 at 5% and 100 at 7%) give 6.00; the totals are weighted too, not averages of the cells.

| Bucket | Conventional | Islami | Total |
| --- | ---: | ---: | ---: |
| 02. 2–7 Days | 6.00 | 8.00 | 6.40 |
| 04. 1–3 Months | 6.00 | 7.00 | 6.43 |
| 10. 3–5 Years | 10.00 | 9.00 | 9.57 |
| Grand total | 8.00 | 8.30 | 8.12 |

Test: `guide_pivot`

## 9. Common messages and fixes

- "Pivot: add at least one field to Values (for example Sum of Balance)." Drag a field into Values.
- "Pivot: add a field to Rows or to Columns." Drag a field into Rows or Columns.
- "Pivot: 'Sum of Balance' needs a number column, but 'Balance' is text. Set it to a number in the schema (or with Set to Number), or use Count or Count distinct." Fix the column type, or change the aggregation.
- "Pivot: 'W.Avg of Rate by Balance' needs a weight column." Choose the weight after "by".
- "Pivot: 'X' is used twice in Rows and Columns." Use each field once.
- "Pivot: 'Average of Balance' cannot be shown as a share (% of grand total): parts of an average do not add up to its total. Use Sum or Count, or choose No calculation."
- "Pivot: N columns is too many (the most is 500)." / "N rows is too many (the most is 200,000)." Remove a field from Columns or Rows, or filter first.
- "Pivot: column 'X' (in Rows) was not found." A step before the pivot dropped or renamed it; pick another field.
- "Pivot: 'Sort by value' points to a value field that is no longer there." Choose Sort rows again.
- "Nothing ticked. Tick at least one value, or tick All." in a Filters picker: the pivot is not changed until one value is ticked.

## 10. Limits and assumptions

- The Python export does not translate Pivot steps (it stops with a clear message); run them in MicroParquet or with a macro.
- Older "Group/Pivot" steps from earlier macros still load and run exactly as before; Edit on one opens the older dialog. That dialog has a "Convert to new pivot…" button: it first lists the output columns whose names will change (for example `Total_Balance` → `Sum of Balance`) and what else changes (a Top-N limit is dropped; the new pivot adds no total row or column until you switch them on with Edit pivot), and converts the step in place only when you click "Convert". "Back" changes nothing. Later steps or macros that use the old column names must be changed to the new ones. A step that mixes "% of total" metrics with other metrics, or has two metrics that are the same calculation, cannot be converted and the button says why.
- Scale sets the display format of the value columns when the tab is made; a column that appears later (a new bank) follows the format of the others after the next refresh, and is shown in the View scale until then.
- At most 2,000 values are listed in a Filters checklist; type in the search box to find others.
