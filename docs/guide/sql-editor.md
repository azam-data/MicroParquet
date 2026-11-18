# SQL editor

## 1. What it does

A tab of the right panel ("SQL", next to "Steps" and "Column") where you write a query on the table of the active tab. The table is called `self`. A good query becomes a step ("SQL Query Transform") of the tab, or opens its result in a new tab. The editor has several lines in a monospace font, the last error under it, and a list of the tab's last 10 queries.

## 2. When to use it

- A grouping or join of columns that the Pivot and Calc column tools do not do in one go.
- A quick look: "SELECT * FROM self WHERE Balance > 1000000".
- A calculation you will repeat: the step is saved in the macro and in the Python script.

## 3. Before you start

- Open a file. The table you query is the tab as it stands after its steps.
- Column names with spaces go in double quotes: "Term Period".
- Know that the query reads the whole table, not just the page on screen.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Columns named in the query | Yes | Any | Branch, Balance | A name that is not in the table stops the query with a message. |
| Amount columns you sum | No | Number | 1,234.50 | Text columns do not add up: set the type in Schema & Ingestion first. |

## 5. Step by step

1. View ▸ "SQL" (or Ctrl+K: "SQL editor"). The right panel opens on the "SQL" tab. The editor starts with the tab's last query; "Table name: self" is the reminder.
2. Type or paste the query. It can have many lines.
3. Click "Apply here" (Ctrl+Enter in the editor) to add it as the next step of this tab, or "New tab" (Ctrl+Shift+Enter) to open the result in a new tab.
4. If the query is wrong, the box under the editor shows the message and no step is added.
5. "History" lists this tab's last 10 queries, newest first; click one to put it back in the editor.
6. The panel can be made wider by dragging its left edge (double-click the edge to go back); the width is kept for next time.

## 6. What you get

A step "SQL Query Transform" in the Steps tab (or a new tab holding the result). The step can be turned off, moved, duplicated and deleted like any other, and is part of a saved macro and of the Python export. The editor keeps your text.

## 7. Check your result

- Compare the row count with what you expect: the status bar shows it.
- For a SUM, compare the total with the Summary row of the source tab.
- Open the step again in the Steps tab to read the query that ran.

## 8. Worked example

Input: five accounts.

| Branch | Product | Balance |
| --- | --- | ---: |
| DHK | TD | 500.00 |
| CTG | SB | 120.00 |
| DHK | SB | 80.00 |
| SYL | TD | 40.00 |
| CTG | TD | 300.00 |

Test: `guide_sql_editor`

Query: total of the term deposits by branch, largest first.

```sql
SELECT Branch, SUM(Balance) AS Total, COUNT(*) AS Accounts
FROM self
WHERE Product = 'TD'
GROUP BY Branch
ORDER BY Total DESC
```

| Branch | Total | Accounts |
| --- | ---: | ---: |
| DHK | 500.00 | 1 |
| CTG | 300.00 | 1 |
| SYL | 40.00 | 1 |

Test: `guide_sql_editor`

## 9. Common messages and fixes

- "The query was not added: …" with the message in the box: read it for the column or word that is wrong; a column that is not in the table is the usual cause.
- "Type a query first." The editor is empty.
- "Open a file first." The SQL tab needs a tab to query.
- A name with spaces needs double quotes ("Term Period"), not single quotes.

## 10. Limits and assumptions

- The query sees the table after the steps that run before it; a step you add later changes what a later query sees.
- Only the SQL that the built-in engine understands works (SELECT with WHERE, GROUP BY, ORDER BY, joins of columns of the same table, and common functions).
- The check builds the query on the table's columns; it does not read the data, so a value that fails at run time shows as a broken step.
