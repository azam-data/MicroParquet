# Filter

## 1. What it does

Keeps or removes rows that meet one or more conditions (equals, greater than, between, contains, in a list, is blank and so on), joined by "and" or "or". Rows are never changed, only kept or dropped, and the control total shows what was removed.

## 2. When to use it

- Cut a deposit extract to one product, branch or currency.
- Remove closed accounts or test rows before reporting.
- Open a filtered view in its own tab.

## 3. Before you start

- Open the tab. Know whether you want to keep the rows that match, or delete them.
- Dates in conditions are typed like the column (YYYY-MM-DD works for Date columns).

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Column to test | Yes | Any column | Brancd, Balance | Account numbers with leading zeros stay text: use text values such as 0034. |
| Condition | Yes | Choice | equals, >, between, contains, is in list, is blank | A blank value never matches a comparison. |
| Value | Yes (not for blank tests) | Text, number or date | DHK, 1500 | between needs both ends. For equals, does not equal, is in list and is not in list you pick the values from the column's own list (see step 3). |
| Join of conditions | Yes | all (and) / any (or) | all conditions (and) |  |
| Keep or delete | Yes | Choice | Keep matching rows |  |

## 5. Step by step

1. Open the tab and click the Filter button on the toolbar (or Ctrl+K: "filter"). The same button edits the tab's first filter.
2. In "Filter rows" choose "Rows must meet": "all conditions (and)" or "any condition (or)".
3. Pick a column, a condition and a value; click "+ Add condition" for more. For "equals", "does not equal", "is in list" and "is not in list" the value box is a picker: click "pick values" (or "3 of 42 selected") and tick the values from the column's own list, with the number of rows of each, counted over the whole tab as it stands before this filter. Use "Search values", "Select all" and "Clear"; "(blank)" is an entry of its own and adds the blank rows. One value stores "equals", several store "is in list" ("is not in list" when you started from "does not equal"). A value with a comma in it, such as "Khulna, West", is one value. A column with more than 1,000 values lists the 1,000 most frequent with the note "Showing 1,000 of N; search to find others"; the search looks through all of them.
4. Choose "Keep matching rows" or "Delete matching rows".
5. Click "Apply to current tab", or "Open filtered view in new tab".
6. To change it later, click the filter chip above the table, or Edit on the step, then "Save filter". "Remove this condition" takes one condition out; removing the last one removes the filter.

## 6. What you get

The same columns with fewer rows. Conditions on rows whose value is blank are "unknown": "Keep matching" drops those rows and "Delete matching" keeps them. The status bar shows the new row count and the control total.

## 7. Check your result

- Rows kept + rows deleted = the original row count: filter with "Keep matching", then with "Delete matching" and add the two counts, or read the removed amount in the control-total comparison.
- The pinned control column's sum before and after shows how much balance the filter removed.

## 8. Worked example

Input: five loans, one with a blank balance.

| Actnum | Brancd | Balance |
| --- | --- | ---: |
| 0012 | DHK | 1,000.00 |
| 0034 | DHK | 2,500.00 |
| 12 | CTG | 5,000.00 |
| 0099 | CTG | (blank) |
| 0100 | DHK | 8,000.00 |

Test: `guide_filter`

Output: Branch equals DHK and balance greater than 1,500, keep matching: the 1,000 loan fails the balance test and the other branch fails the branch test.

| Actnum | Brancd | Balance |
| --- | --- | ---: |
| 0034 | DHK | 2,500.00 |
| 0100 | DHK | 8,000.00 |

Test: `guide_filter`

## 9. Common messages and fixes

- "X: pick at least one value." The condition has no value yet: open the picker and tick one, or choose "(blank)".
- "'abc' is not a number for numeric column 'Rate'." A picked value must fit the column's type: numbers and dates (YYYY-MM-DD) are matched as typed values, not as text, so 9.50 matches 9.5.
- "Add at least one condition." / "{column}: enter a value." / "{column}: enter the upper limit of the range." Fill the missing field.
- "Pick a column." Choose a column for each condition.
- A filter step that turns red after you delete or rename a column it uses: restore the column or edit the filter.

## 10. Limits and assumptions

- Conditions compare a column with a value; to compare two columns use a Conditional column first.
- Text comparisons are exact apart from the "contains", "starts with" and "ends with" tests.
