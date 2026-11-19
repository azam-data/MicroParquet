# Data-quality scorecard

## 1. What it does

Checks a tab against a set of rules and gives each rule a pass percentage (rows that pass ÷ rows checked), then an overall score (the plain average of those percentages) rated Green, Amber or Red. A control total (the sum of a column against a GL figure you type) is a Pass or a Fail with its difference and is not in the average. For every rule that fails, the dialog lists the first 20 failing rows and can open all of them as a tab. It builds on the hygiene audit of the Banking menu, and the rules are kept in a file so the same checks run every month.

## 2. When to use it

- Before a file goes into the ALCO pack or a return: is it fit to use?
- To show an auditor what was checked, what failed and how many rows.
- To follow the quality of a feed month by month with the same rules.

## 3. Before you start

- Open the file in a tab and set its schema (Schema & Ingestion), so that dates are dates and amounts are numbers. The type rule reads the schema's own failures.
- Every rule, the type rule included, is measured on the tab's data as loaded from its file (with the schema's renames and types), before any step of the tab. Filters and other steps of the tab do not apply; the dialog says so. To check filtered rows, save them to a file and open that.
- The first time, the app writes a sample rule set (`portable_data/dq_rules/sample.json`, marked sample). Change the rules and the thresholds to the bank's own and save them under a new name; untick "sample" by editing the file only when you are sure.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Completeness, Uniqueness, Allowed values | As the rule says | Any | Account_No, Product | Blank means empty text or no value. |
| Range, Control total | As the rule says | Number | Rate, Balance | A Text column is refused with its type. |
| Consistency | As the rule says | Two numbers, two dates or two date-times | Maturity, Open_Date | Both columns must be the same kind. |
| Type validity | As the rule says | A column of the file | Rate | Looks at the source as loaded: values the schema could not read. |

## 5. Step by step

1. Open the tab to check. Choose Transform ▸ "Data-quality scorecard…" (Ctrl+K: "data quality").
2. Open a saved rule set ("Open a rule set") or build one: choose a "Kind", the column or columns, the limits, and choose "Add rule". "Remove" deletes a rule.
3. Set "Green from" and "Amber from" (for example 98 and 95). Below Amber is Red. Both are required in the rules file (`green_min` and `amber_min`): a file without them is refused, naming the file and the field, and no default is assumed.
4. Name the rule set and choose "Save rule set" (`portable_data/dq_rules/<name>.json`).
5. Choose "Run checks". The result lists each rule with the rows checked, the rows failing, the pass percentage and, at the end, the overall score and its colour.
6. "Open scorecard as tab" puts the scorecard in a tab (export it, or use it as a report-pack item). Under each failing rule, "Open failing rows as tab" lists all of its failing rows with their row number.

## 6. What you get

A table with a row for each rule (Rule, Kind, Detail, Rows checked, Rows failing, Pass %, Result) and a last row "Overall score". Control totals show Pass or Fail in Result and have no percentage.

## 7. Check your result

- Rows checked is the number of rows with a value for the rule (completeness checks every row).
- Pass % = (Rows checked − Rows failing) ÷ Rows checked.
- The overall score is the average of the Pass % column; add them up and divide to check.
- Open the failing rows of a rule and look at three of them in the file.

## 8. Worked example

Ten accounts: one blank Account_No, one pair of equal account numbers (A07), two Rate values that cannot be read as numbers, one Rate of 45 (the limit is 30) and one Maturity before its Open_Date. The Balance column adds up to 1,000 and the GL says 1,005 (tolerance 0.1%).

| Account_No | Rate | Open_Date | Maturity | Balance |
| --- | ---: | --- | --- | ---: |
| A01 | 9.50 | 2020-01-01 | 2025-01-01 | 100.00 |
| A02 | 8.00 | 2020-01-01 | 2025-01-01 | 100.00 |
| A03 | (blank) | 2020-01-01 | 2025-01-01 | 100.00 |
| A04 | (blank) | 2020-01-01 | 2025-01-01 | 100.00 |
| A05 | 45.00 | 2020-01-01 | 2025-01-01 | 100.00 |
| A06 | 7.00 | 2020-01-01 | 2019-12-31 | 100.00 |
| A07 | 6.00 | 2020-01-01 | 2025-01-01 | 100.00 |
| A07 | 6.50 | 2020-01-01 | 2025-01-01 | 100.00 |
| (blank) | 5.50 | 2020-01-01 | 2025-01-01 | 100.00 |
| A10 | 4.00 | 2020-01-01 | 2025-01-01 | 100.00 |

Test: `guide_data_quality_scorecard`

The scorecard:

| Rule | Rows checked | Rows failing | Pass % | Result |
| --- | ---: | ---: | ---: | --- |
| Completeness | 10 | 1 | 90.00 | (blank) |
| Uniqueness | 9 | 2 | 77.78 | (blank) |
| Type | 10 | 2 | 80.00 | (blank) |
| Range | 8 | 1 | 87.50 | (blank) |
| Consistency | 10 | 1 | 90.00 | (blank) |
| GL control | (blank) | (blank) | (blank) | Fail |
| Overall score | (blank) | (blank) | 85.06 | Red |

Test: `guide_data_quality_scorecard`

## 9. Common messages and fixes

- "The rule 'Rate is from 0 to 30': the column 'Rate' is str, not a number." Set the column's type in Schema & Ingestion.
- "There is no column 'X' in the table." The tab does not have it (it may have been renamed): choose the column again.
- "The rule … compares a date ('Maturity') with a number …" Choose two columns of the same kind.
- "Type validity reads the file's own columns": the type rule's column is not a column of the source after renames.
- "The thresholds must satisfy 0 ≤ Amber ≤ Green ≤ 100."

## 10. Limits and assumptions

- The type rule counts the source's rows and its unreadable values; it cannot list the failing rows, only examples. (Like every other rule, it is measured before any step of the tab.)
- A rule with no rows to check has no percentage and is left out of the average.
- A blank is an empty value or empty text; only completeness counts blanks as failures.
- Uniqueness fails every row of a repeated key (not just the second one); rows with a blank part of the key are not checked.
- The sample rule set and its thresholds are not the bank's policy: the Sample banner shows until the file says `"sample": false`.
