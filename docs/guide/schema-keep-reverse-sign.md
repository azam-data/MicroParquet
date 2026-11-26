# Keep or drop columns, and reverse a sign (file-open dialog)

## 1. What it does

Two extra choices in the schema dialog that opens with a file (Schema & Ingestion). **Keep** is a box on each column row: a column whose box is cleared is removed from the table, after the renames and before any step, so every later screen sees the narrower table. **Reverse sign** multiplies a number column by -1: advance balances that arrive as negative numbers become positive. A zero stays 0 (never -0).

## 2. When to use it

- A core-banking extract has 60 columns and you use 8: drop the rest once, here, instead of hiding them in every tool.
- Advances or interest paid arrive negative from the CBS and the ADR, NIM or a chart needs them positive.
- A column holds a code or a name that should never reach a report.

## 3. How to use it

1. Open the file (File ▸ Open). The Schema & Ingestion dialog shows one row per column.
2. Clear **Keep** on each column you do not want. The column's other choices become inactive.
3. Tick **Reverse sign** on a number column to flip it. The box is only available when the type is Float64 or Int64: set the type first (hover text says so).
4. Confirm the dialog as usual. The table opens with the kept columns only, signs already reversed.
5. To keep the same choices for next month's file, save the schema as a template; Keep and Reverse sign are saved with it.

## 4. Example

A loans file has the columns Account, Branch, Balance, Rate, Remarks. Balance arrives as -500 for an advance of 500, and one row has 0. Clear Keep on Remarks and tick Reverse sign on Balance: the table opens with four columns, Balance reads 500 and 0 (not -0), and Remarks is gone from every pivot, filter and export.

## 5. What it refuses and why

- Every column dropped: refused with a plain message, never an empty table.
- Reverse sign on a column that is not a number: refused. There is no silent conversion of text to a number.
- A column dropped here is not there for a later step to use: add the column back by editing the schema (the file is read again).

## 6. Python export

Both choices are written into the Python export of the table, so the script drops the same columns and flips the same signs.

## 7. Saved in

With the schema: in the schema file and in schema templates, in saved macros and in the project. Older schema files, which do not have these fields, still load and keep every column.

## 8. Related tools

[Schema & Ingestion](schema-ingestion.md) · [Calc column](calc-column.md) · [Projects](projects.md)
