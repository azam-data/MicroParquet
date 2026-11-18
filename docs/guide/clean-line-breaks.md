# Clean line breaks

## 1. What it does

Replaces every line break inside a text value (a Remarks or address cell that the core banking system exported over two or three lines) with one space, and trims the spaces at the ends of the values that had a break. An option, "Trim spaces on all text values", trims the ends of every value, break or not. Without it a value like "Paid in full / by cheque" stays on two lines in the grid, in Excel and in a PDF, and a CSV that has such values reports "N records contain line breaks inside quotes". It supports clean, one-line-per-account exports for ALCO packs and Bangladesh Bank returns.

## 2. When to use it

- After opening a CBS extract whose status bar says "N records contain line breaks inside quotes".
- Before exporting to Excel or PDF, so each account is one row of one line.
- Before a Join or Pivot on a text column that has stray line breaks.

## 3. Before you start

- Open the file and look at the status bar: "5 rows from 8 lines: 3 records contain line breaks inside quotes" means the row count is right and some values hold line breaks. Nothing is changed when you open the file; the step runs only when you ask.
- The Schema & Ingestion dialog's "Clean line breaks" box does the same thing: ticking it (it is ticked by default for a CSV) puts this step first in the Steps list, for all text columns and with "Trim spaces on all text values" ticked, so it is recorded and replays in macros and the values are the ones the box always gave (spaces at the ends of every text value trimmed). A step made from Transform ▾ has that option off unless you tick it. Unticking the box (Apply to tab) takes that first step out again. This tool works on any text column of the table, at any point of the steps.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Text columns | No (all text columns by default) | Text | Remarks, Address, Acttit | Only text columns are cleaned. Numbers and dates are not touched. A column you choose that is a number or date is refused with a message. |

## 5. Step by step

1. Open the tab. If the status bar shows "N records contain line breaks inside quotes", click "Replace with spaces" beside it (or open "Details" and click it there). The step "Clean line breaks" is added for all text columns and the note goes away.
2. To choose columns, use Transform ▾ → "Clean line breaks…" (or Ctrl+K and type "line breaks"). In "Clean line breaks" choose "All text columns" or "Only these columns:" and tick them. Tick "Trim spaces on all text values" if the spaces at the ends of values without a line break should go too (it is off here by default). Then click "Clean line breaks".
3. To change it later, click the step in the Steps panel: its buttons appear in one row (Edit, Move up, Move down, Turn off, Duplicate, and Delete at the right end, which asks first; each has a tooltip). Click Edit. Undo (Ctrl+Z) takes the step away and the note comes back.
4. Export as usual. The step is recorded, so a saved macro and a batch run repeat it.

## 6. What you get

The same table with the same number of rows. In each cleaned column every value that contained a line break (\r\n, \n or \r) has each run of line breaks replaced by one space and its ends trimmed. A value without a line break is left exactly as it was (spaces at its ends included), unless "Trim spaces on all text values" is ticked: then the spaces (not tabs) at both ends of every value in the chosen columns are trimmed, as the Schema & Ingestion box does. Blank values stay blank. No column is added.

## 7. Check your result

- The row count in the status bar is the same before and after (the step never adds or removes rows).
- The control total is unchanged (the step only touches text).
- The "N records contain line breaks inside quotes" note is gone for a tab that has the step; filter the column on "contains" and a line break to see that none is left.

## 8. Worked example

Five records read from a CSV; three have line breaks inside quotes (written ⏎ here). A4 also has spaces around its break.

| Actnum | Remarks | Balance |
| --- | --- | ---: |
| A1 | Paid in full⏎by cheque | 100.00 |
| A2 | plain | 200.00 |
| A3 | Line one⏎Line two | 300.00 |
| A4 |   leading and trailing⏎⏎⏎   | 400.00 |
| A5 | fine | 500.00 |

Test: `guide_clean_line_breaks`

After "Clean line breaks" for all text columns: one space for each run of breaks, ends trimmed, other values and the numbers as they were.

| Actnum | Remarks | Balance |
| --- | --- | ---: |
| A1 | Paid in full by cheque | 100.00 |
| A2 | plain | 200.00 |
| A3 | Line one Line two | 300.00 |
| A4 | leading and trailing | 400.00 |
| A5 | fine | 500.00 |

Test: `guide_clean_line_breaks`

## 9. Common messages and fixes

- "N records contain line breaks inside quotes" (grey note, status bar): the file is fine; click "Replace with spaces" if you want one line per value.
- "Clean line breaks: the column 'X' was not found. Choose the columns again." A step before it dropped or renamed the column.
- "Clean line breaks: the column 'X' is Float64, not text. Choose text columns only." Untick it.
- "Tick at least one column, or choose All text columns." in the dialog.
- "Rows read N vs file data lines M" (warning colour) stays: the lines are not explained by line breaks alone (check unclosed quotes); this tool does not fix that.

## 10. Limits and assumptions

- "Trim spaces on all text values" trims spaces only, not tabs or other white space (the old box did the same). On a table made from the Schema & Ingestion dialog, a column that is masked is cleaned before it is masked, so a mask does not change.
- It does not decide which line breaks are "real": a value that holds a deliberate list over several lines is joined into one line.
- Only the text columns' line breaks are cleaned; a line break between two records (an unquoted break) is not a value and is not touched.
- The Python export translates this step (a loop over the text columns).
