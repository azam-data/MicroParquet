# Calc column

## 1. What it does

Adds a column that is the result of one arithmetic operation: a column plus, minus, times or divided by another column or a fixed number. Use it for unit changes (BDT to Crore), simple ratios and differences.

## 2. When to use it

- BDT to Crore or Lakh for a report column.
- Interest = balance × rate, or a difference between two balances.
- A column to feed a Pivot or a Conditional column.

## 3. Before you start

- Open the tab with both columns as numbers (set text amounts to numbers first).
- Decide the new column's name.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Left column | Yes | Number | Balance | A text column is converted; text that is not a number becomes blank. |
| Operator | Yes | +  −  ×  ÷ | ÷ |  |
| Right side | Yes | A number or another column | 10,000,000 or Matamt | A number may have thousands separators; text like "ten million" is refused. |
| New column name | Yes | Text | Balance_Cr |  |

## 5. Step by step

1. Open the tab and click the Calc column button on the toolbar (or Ctrl+K: "calc").
2. In "Add Mathematical Calculated Column" type "New Column Name:".
3. Pick the left column and the operator.
4. Either type "Scalar Value (e.g. 10000000):" or tick "Use Another Column" and pick the right column.
5. Click "Add Calculated Column".

## 6. What you get

One new column of decimal numbers (Float64): left op right, row by row. A blank on either side gives a blank result. Rates are not scaled.

## 7. Check your result

- Spot-check two rows with a calculator.
- For a BDT → Crore column, the sum of the new column × 10,000,000 equals the sum of the original.

## 8. Worked example

Input: four balances in BDT.

| Actnum | Balance |
| --- | ---: |
| A1 | 25,000,000.00 |
| A2 | 5,000,000.00 |
| A3 | 12,500,000.00 |
| A4 | 100,000,000.00 |

Test: `guide_calc_column`

Output: dividing by 10,000,000 gives Crore.

| Actnum | Balance | Balance_Cr |
| --- | ---: | ---: |
| A1 | 25,000,000.00 | 2.5 |
| A2 | 5,000,000.00 | 0.5 |
| A3 | 12,500,000.00 | 1.25 |
| A4 | 100,000,000.00 | 10 |

Test: `guide_calc_column`

## 9. Common messages and fixes

- "'abc' is not a number" with the column name: the scalar must be a number (commas are fine).
- "Column 'X' was not found" after a rename: use Edit on the step.

## 10. Limits and assumptions

- One operation per step; chain steps for longer formulas.
- Division by zero is not checked: the result is not a valid number for that row, so filter those rows or test the divisor first.
