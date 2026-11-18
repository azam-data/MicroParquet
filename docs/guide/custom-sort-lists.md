# Custom sort lists and Tenor order

## 1. What it does

Puts text values in the order a bank reads them, not the order of the alphabet. A custom list is a saved sequence of values (High, Medium, Low). Tenor is built in: it reads "1m", "100D", "1y" and orders them by length in days. Both work in Sort, in the Rows and Columns of a Pivot, and later in the Chart and the filter picker.

## 2. When to use it

- Risk grades, rating bands or statuses that have a fixed order.
- Term periods or tenors typed as text, which the alphabet puts in the wrong place ("100D" before "1m").
- A report whose rows must follow your template rather than A to Z.

## 3. Before you start

- A list is a file in `portable_data/sort_lists/`, one per list. A sort or pivot that uses a list keeps its own copy of the values, so a saved macro gives the same order later even if you edit the list.
- Tenor needs no setup: a number and a unit, with or without a space, any case. D, day, days = 1 day; W, wk, week = 7; M, mon, month = 30; Y, yr, year = 365. O/N, ON and Overnight = 1 day.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Column to order | Yes | Text | Term Period, Risk Grade | Number and date columns sort by value and do not take a list. |
| Suggested for columns (list only) | No | Column names | Term Period | A pivot field with this name starts with the list selected. |

## 5. Step by step

1. View ▸ "Custom sort lists" (or Ctrl+K: "custom sort lists"). Click "New list".
2. Type the Name. Add values with "Add value", or paste one per line in "Add several values" and click "Add these lines". "Move up", "Move down" and the cross change the order.
3. Or open a file, choose a column in "Fill from column" and click "Fill from column": its distinct values arrive in natural order, ready to reorder.
4. Click "Save list". "Rename" changes the name of the selected list; "Delete" asks first.
5. In Sort, each level has an "Order" choice: Natural, one of your lists, or Tenor. "Lowest first" follows the list; "Highest first" reverses it.
6. In the Pivot builder, open the "…" menu of a field in Rows or Columns and choose "Order: Natural / <list> / Tenor / By value".

## 6. What you get

Listed values come first, in list order. Values not in the list follow, in natural order. Blanks are always last. With Tenor, parsed values are ordered by days (ties keep natural order) and the values that are not tenors follow; the status line says how many there were.

## 7. Check your result

- The row count is unchanged by a sort.
- With Tenor, the first and last parsed rows are the shortest and longest term.
- A value you expected in the list but see after it is spelt differently: check spaces and the "Ignore upper and lower case" box.

## 8. Worked example

Input: six deals.

| Id | Term Period | Risk |
| --- | --- | --- |
| T1 | 3y | Low |
| T2 | 6m | High |
| T3 | 100D | Medium |
| T4 | 1y | Other |
| T5 | abc | Low |
| T6 | 1m | High |

Test: `guide_custom_sort_lists`

Sorted by Term Period with Order Tenor: 1m, 100D (100 days), 6m (180 days), 1y, 3y; "abc" is not a tenor and goes last.

| Id | Term Period |
| --- | --- |
| T6 | 1m |
| T3 | 100D |
| T2 | 6m |
| T4 | 1y |
| T1 | 3y |
| T5 | abc |

Test: `guide_custom_sort_lists`

Sorted by Risk with the list High, Medium, Low: "Other" is not in the list, so it follows the listed values.

| Id | Risk |
| --- | --- |
| T2 | High |
| T6 | High |
| T3 | Medium |
| T1 | Low |
| T5 | Low |
| T4 | Other |

Test: `guide_custom_sort_lists`

## 9. Common messages and fixes

- "'Tenor' is a built-in order. Choose another name for the list." Natural, Tenor and By value are names of the menus.
- "'High' is in the list twice." Each value has one place; remove the duplicate.
- "The list has no values." Add values, or use "Fill from column".
- "Sort: the order 'Tenor' is for text columns." The column is a number or a date: choose Natural.
- "Files that cannot be used" in the manager: a file in `sort_lists` is damaged; fix or delete it.

## 10. Limits and assumptions

- Tenor uses 30 days for a month and 365 for a year, so "12m" (360 days) comes before "1y" (365).
- A number without a unit ("5") is not a tenor.
- In the pivot, "By value" orders the rows by the first value field, high to low, and applies to Rows only.
- A list matches whole values after trimming spaces; it does not match parts of a value.
