# Stack

## 1. What it does

Joins several files of the same layout one under the other (for example twelve monthly extracts) into one tab. Each file is read with its own detected types, or all with one schema, and an optional Source_File column tells you which file each row came from. If a file has an extra column, the other files get blanks for it.

## 2. When to use it

- A year of month-end extracts for a trend pivot.
- Assets and liabilities in separate files, combined for a ΔNII run.

## 3. Before you start

- Files with the same columns (extra columns are allowed).
- The same column should have the same type in every file; otherwise the stack stops and names the files.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Files | Yes | CSV or Parquet | sep.csv, oct.csv | At least two. The order you add them is the order of rows. |
| Schema choice | Yes | Choice in the dialog | Each file's own detected types | Or apply the first file's schema, or a saved .schema.json, to every file. |
| Source_File column | No | Tick box | ticked | Adds the file name to every row. |

## 5. Step by step

1. Choose File → "Stack / merge several files…" (Ctrl+K: "stack").
2. In "Vertical Multi-File Merger / Stacker" click "Add Files to Stack..." and pick the files; "Clear List" empties the list.
3. Tick "Append 'Source_File' Column to identify origin month/file" if you want it.
4. Choose how types are decided: "Each file's own detected types (types must agree)", "Apply one schema to every file: the first file's", or "Apply one schema to every file: saved .schema.json" (then "Pick..." the file).
5. Click "Stack & Open as Unified Tab". The tab is titled "Stacked (N files)".

## 6. What you get

One tab holding every row of every file, columns in the order they first appear, `Source_File` last when ticked. A column missing from a file is blank for that file's rows. Row count = sum of the files' row counts.

## 7. Check your result

- Row count equals the sum of the files' rows; the control total equals the sum of the files' totals.
- Pivot by Source_File: each file's total must match that file's own total.

## 8. Worked example

Input: September's file.

| File | Actnum | Balance |
| --- | --- | ---: |
| sep.csv | 001 | 100 |

Test: `guide_stack`

Input: October's file, with an extra Branch column.

| File | Actnum | Balance | Branch |
| --- | --- | ---: | --- |
| oct.csv | 001 | 200 | DHK |

Test: `guide_stack`

Output: September's row has no branch, so Branch is blank for it.

| Actnum | Balance | Branch | Source_File |
| --- | ---: | --- | --- |
| 001 | 100.00 | (blank) | sep.csv |
| 001 | 200.00 | DHK | oct.csv |

Test: `guide_stack`

## 9. Common messages and fixes

- "Stack failed: …" followed by "Column 'Balance' … sep.csv … nov.csv … Float64 … String": the column has different types in two files. Fix the data, or apply one schema to every file (the values that do not fit become blank and are counted).
- "Could not load the schema file …": pick a valid .schema.json.
- A note that a saved schema's column is missing from a file: it will be blank for that file.

## 10. Limits and assumptions

- Files are stacked as they are; there is no de-duplication.
- Different column names for the same thing (Actnum vs Account No) become two columns: rename first.
