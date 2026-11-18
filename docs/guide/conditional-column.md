# Conditional column (IF / ELSE)

## 1. What it does

Adds a column whose value depends on tests of other columns: IF a test is true THEN a value, ELSE IF the next test, and so on, ELSE a fallback value. It is how you create flags and groups (size class, watch list, product group) before a Pivot.

## 2. When to use it

- Size classes of loans or deposits (large, medium, small).
- A "Dormant" or "Watch" flag from status and balance.
- Grouping product codes into reporting groups.

## 3. Before you start

- Open the tab. Know the exact tests in order: the first test that is true wins, so put the most specific test first.
- Decide whether the new column is text or a number (tick "Output as Numeric (Float64)" only if every THEN value and the ELSE value are numbers).

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Column to test | Yes | Any column | Balance, Acstat | Comparing a number column with a text value, or the reverse, is refused with both column names. |
| Comparison | Yes | Choice | > (Greater Than), == (Equals), Contains Text, In List (a, b, c) | Text is compared as typed. |
| Value or column | Yes | Text, number or a second column | 5000000, or the column Matamt | Switch to a column to compare two columns. |
| THEN value, ELSE value | Yes | Text or number | Large | ELSE is used when no test is true. |
| New column name | Yes | Text | Size |  |

## 5. Step by step

1. Open the tab, then choose More ▾ → "Conditional column (IF / ELSE)…" (Ctrl+K: "conditional").
2. In "Add Conditional Column (IF / ELSE IF / ELSE)" type "New Column Name:".
3. For the first rule choose the column, the comparison, the value (or switch it to another column) and the "THEN" value.
4. Click "Add ELSE IF Branch" for each further rule.
5. Type "ELSE (Fallback Value):".
6. Under "If either side of a rule is blank, the result is:" choose "no match (try the next rule)" (default, what older macros do) or "this value" with the value to give.
7. Tick "Output as Numeric (Float64)" if the result is a number.
8. Click "Add Column".

## 6. What you get

One new column. Rules are tried from top to bottom; the first true rule gives its THEN value; a row with no true rule gets the ELSE value. A blank on either side of a rule never matches, unless you chose a blank result in step 6, which is given at once without trying the other rules.

## 7. Check your result

- Row count and control total are unchanged.
- Pivot the new column by count: the counts add up to the row count.
- Spot-check one row of each group by hand.

## 8. Worked example

Input: four accounts, one with a blank balance.

| Actnum | Balance |
| --- | ---: |
| A1 | 8,000,000.00 |
| A2 | 2,500,000.00 |
| A3 | 400,000.00 |
| A4 | (blank) |

Test: `guide_conditional_column`

Output: rules are Balance > 5,000,000 → Large, Balance > 1,000,000 → Medium, ELSE Small. The blank balance matches neither rule, so it gets Small; choose a blank result if you would rather flag it.

| Actnum | Balance | Size |
| --- | ---: | --- |
| A1 | 8,000,000.00 | Large |
| A2 | 2,500,000.00 | Medium |
| A3 | 400,000.00 | Small |
| A4 | (blank) | Small |

Test: `guide_conditional_column`

## 9. Common messages and fixes

- A red error on the step naming both columns and their types when a number is compared with text: make both sides the same type (Set to Number, or compare with a text value).
- "Column 'X' was not found" after a rename: use Edit on the step.
- Numeric output with a text THEN value: the step fails; tick "Output as Numeric (Float64)" only for numbers.

## 10. Limits and assumptions

- Comparisons only between a column and a value or another column; no formulas inside a rule (add a Calc column first).
- Rules are tried in order; there is no "all of these" within one rule.
