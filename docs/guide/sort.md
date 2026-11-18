# Sort

## 1. What it does

Orders the rows by one or more columns, each from lowest to highest or the reverse. Numbers sort by value, dates by date, and text in natural order ignoring case, so "Zone 2" comes before "Zone 10". Blanks are always last.

## 2. When to use it

- Largest balances first for a top-N list.
- Branch order with balances inside each branch.
- A report order before an Excel or PDF export.

## 3. Before you start

- Open the tab. Decide the levels: level 1 first, then level 2 for ties, and so on.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Column per level | Yes | Number, date or text | Brancd, Balance | A column can be used once. |
| Direction per level | Yes | Lowest first / Highest first | Highest first for Balance | Blanks are last in either direction. |

## 5. Step by step

1. Open the tab and click the Sort button on the toolbar (or Ctrl+K: "sort"; right-click a column header also offers it).
2. In "Sort rows" choose a column and "Lowest first" or "Highest first" for level 1.
3. Click "Add level" for each further level; "Move up" and "Move down" change the order, "Remove this level" deletes one.
4. Click "Apply sort". To change or take it away later, use "Edit sort" or "Remove sort".

## 6. What you get

The same rows in the new order. The header shows the level and direction ("1 ↑", "2 ↓"). Rows that tie on every level keep their earlier order. Text sorts case-insensitively with digit runs by value: "ctg" and "CTG" tie, "Branch 2" comes before "Branch 10", and "02" and "2" tie.

## 7. Check your result

- Row count and control total are unchanged.
- The first and last rows are the minimum and maximum of the first level (blanks excepted).

## 8. Worked example

Input: six accounts in the order of the file.

| Actnum | Brancd | Balance |
| --- | --- | ---: |
| A1 | DHK | 100.00 |
| A2 | CTG | 300.00 |
| A3 | DHK | 900.00 |
| A4 | CTG | (blank) |
| A5 | Zone 10 | 50.00 |
| A6 | Zone 2 | 60.00 |

Test: `guide_sort`

Output: Brancd A to Z, then Balance highest first. In CTG the blank balance is last; Zone 2 comes before Zone 10.

| Actnum | Brancd | Balance |
| --- | --- | ---: |
| A2 | CTG | 300.00 |
| A4 | CTG | (blank) |
| A3 | DHK | 900.00 |
| A1 | DHK | 100.00 |
| A6 | Zone 2 | 60.00 |
| A5 | Zone 10 | 50.00 |

Test: `guide_sort`

## 9. Common messages and fixes

- "Add at least one level." Pick a column for level 1.
- "{column} is used twice: a column can be a level once." Remove one of the levels.
- "Sort: column 'X' was not found. Pick it again in the sort editor.

## 10. Limits and assumptions

- Only the displayed result is ordered; a later Pivot or filter may change the order again.
- Text order is natural and case-insensitive, not the order of a code table.
