# Column inspector

## 1. What it does

Shows the facts of one column over the whole result: how many rows and blanks, and for numbers the sum, minimum, maximum, mean, number of distinct values and a ten-bar distribution; for dates the earliest and latest; for text the number of distinct values and the five most frequent ones.

## 2. When to use it

- Checking an extract before using it: blanks, outliers, odd dates.
- Seeing which branch or product dominates.
- Explaining a total in a meeting.

## 3. Before you start

- Open the tab. The side panel must be shown with its "Column" tab on (View ▸ Side panel ▸ "Column", or Ctrl+K: "panel").

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Column | Yes | Any column | Balance, Brancd | Chosen by clicking its header. |

## 5. Step by step

1. Open the tab and click a column header.
2. Open the Column tab in the right panel.
3. Read the type, rows and blanks, then the figures for that type.
4. Click another header to inspect another column; the numbers follow the steps (they are recomputed after a filter or other step).

## 6. What you get

Rows and blanks (blank = null, or text of only spaces). Numbers: sum, min, max, mean, distinct count, and ten equal-width bars between the 1st and 99th percentile (so one huge value does not flatten the others); under the bars a caption gives the number of bars and the range, and a line says how many values are below and above that range (for example "1 value above range"). "Log scale" draws the bars by the logarithm of their count, so a few tall bars do not hide the short ones; hover a bar for its range and count. Dates: earliest and latest, and a bar for each calendar month that has values (the latest 36 months when there are more), with the month and count on hover. Text: distinct count and the ten most frequent values with their counts (ties in text order). Every figure card shows its whole value (a value too long for half a card gets the full width) and the full value on hover.

## 7. Check your result

- The sum equals the summary row's Sum for the same column.
- Rows − blanks = the count of values; the bars plus the values below and above the range add up to it.

## 8. Worked example

Input: five rows with one blank balance and one blank branch.

| Brancd | Balance |
| --- | ---: |
| DHK | 100.00 |
| CTG | 200.00 |
| DHK | 300.00 |
| DHK | 400.00 |
| (blank) | (blank) |

Test: `guide_column_inspector`

Output: the inspector figures for Balance and for Brancd.

| Column | Rows | Blank | Sum | Min | Max | Mean | Distinct | Histogram (10 bars) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Balance | 5 | 1 | 1,000.00 | 100.00 | 400.00 | 250.00 | 4 | 1 0 0 1 0 0 1 0 0 1 |
| Brancd | 5 | 1 | — | — | — | — | 2 | most frequent: DHK (3), CTG (1) |

Test: `guide_column_inspector`

## 9. Common messages and fixes

- "Column 'X' is not in the result." The column was removed by a step; pick another.
- Figures show as out of date for a moment after you change a step; wait for them to refresh.

## 10. Limits and assumptions

- Statistics cover the result up to the last valid step; a broken step stops them there.
- Blank text is never counted as a value, so a column of only blanks shows no distinct values.
- The bar range is the 1st to the 99th percentile (the nearest actual value), so with 100 values or more the smallest and largest 1% fall outside the bars and are counted in the line under them; with fewer values every value is in the bars.
