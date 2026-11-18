# Schema & Ingestion (types, Dr/Cr, masking)

## 1. What it does

The dialog that opens when you open a CSV or Parquet file. It decides how each column is read: text that keeps leading zeros, decimal amounts (with Dr/Cr and accounting negatives), whole numbers or dates in D/M/Y, M/D/Y or ISO form, plus how the file itself is read (separator, encoding, title lines) and which columns are masked. A wrong type here makes every later number wrong, so the dialog also counts values it could not convert.

## 2. When to use it

- Opening any new extract from the core banking system.
- Locking one agreed schema as a template (`.schema.json`) for the next month's extract.
- Masking customer names or account numbers before a file leaves the bank.

## 3. Before you start

- Know the file's separator and encoding (CBS exports are often semicolon or Windows-1252).
- Know the date style (CBS dates are usually D/M/Y) and whether amounts carry "Dr" and "Cr".
- If you want the same masks on another PC, export the masking key first (and import it there).

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Account numbers, IDs, codes | Yes | Text / ID (Utf8 - Keeps Zeros) | 0012, 00034567 | Never Decimal: leading zeros would be lost. |
| Amounts | Yes | Decimal / Balance (Float64) | 1,234.50, (500.00), 1,200.00 Dr | Thousands separators and brackets are read; Dr/Cr amounts need a sign choice. |
| Dates | Yes | Date (D/M/YYYY or D-M-YYYY), Date (M/D/YYYY…) or Date (YYYY-MM-DD) | 05/11/2026 | Choose the style the file really uses; the preview shows how it was read. |
| Whole numbers | No | Whole Integer (Int64) | 36 | Counts, tenors in months. |
| Masked columns | No | No Masking / Mask Name / Mask Acct | Acttit | Masks use this PC's secret key. |

## 5. Step by step

1. Choose File → "Open CSV or Parquet file…" (Ctrl+K: "open") and pick the file. The dialog "Schema & Ingestion: CSV 5-Row Raw Preview, Linebreak Sanitizer & Strict Schema Lock" opens (for Parquet: "Schema & Ingestion (Parquet): Types & PII Masking").
2. Under "Reading options" set "Separator", "Encoding" and "Lines above the header row that are skipped" if needed; click "Re-read" to apply them. Tick "Clean line breaks" (it adds the recorded step "Clean line breaks" as the first step of the tab, see clean-line-breaks.md; unticking it in "Apply to tab" takes the step out), "Parse Accounting Negatives (1,234.50 -> -1234.50)" and "Pre-Heal Broken Unquoted CSV Lines" as your file needs.
3. Under "Columns" check each column's "Target Data Type". A column with Dr/Cr values shows a warning: choose "Dr is +" or "Cr is +".
4. Choose "No Masking", "Mask Name (CLIENT_XXXX)" or "Mask Acct (ACCT_XXXX)" for sensitive columns under "PII Anonymizer".
5. Read the "Preview" and the "Parse Failures" list: values that could not be converted become blank and are counted there.
6. Optional: "Save Schema Template (.schema.json)" to reuse, or "Load Saved Schema Template".
7. Click "Lock Schema & Open" (or press Enter) to open the tab, or "Convert to Parquet…" to convert without opening a tab. "Cancel" (Esc) closes without changes. To change the schema of an open tab later, use "Apply Schema to This Tab".

## 6. What you get

A tab with typed columns. Text columns keep their text exactly (leading zeros). Decimal columns hold numbers: thousands separators removed, "(500.00)" is −500, Dr/Cr amounts get the sign you chose. Date columns hold real dates. Masked columns hold CLIENT_ or ACCT_ tokens made with this PC's key. Values that could not be converted are blank and counted under Parse Failures.

## 7. Check your result

- Compare the row count with the number of data lines in the file; the program warns when rows are missing.
- Compare the sum of the amount column with the control total of the extract (the report footer or GL).
- Check the date range in the Column inspector: a wrong date style shows impossible months or years.

## 8. Worked example

The file as written (three loans; amounts with commas and brackets, D/M/Y dates).

| Actnum | Opndat | Balance |
| --- | --- | --- |
| 0012 | 05/11/2026 | 1,234.50 |
| 0034 | 15/11/2026 | (500.00) |
| 0099 | 01/12/2026 | 2,000.00 |

Test: `guide_schema_ingestion`

The types the program chose.

| Column | Type chosen | Sample read |
| --- | --- | --- |
| Actnum | Text / ID (Utf8 - Keeps Zeros) | 0012 |
| Opndat | Date (D/M/YYYY or D-M-YYYY) | 2026-11-05 |
| Balance | Decimal / Balance (Float64) | 1,234.50 |

Test: `guide_schema_ingestion`

The tab after loading: leading zeros kept, dates read as D/M/Y, the bracketed amount is negative.

| Actnum | Opndat | Balance |
| --- | --- | ---: |
| 0012 | 2026-11-05 | 1,234.50 |
| 0034 | 2026-11-15 | -500.00 |
| 0099 | 2026-12-01 | 2,000.00 |

Test: `guide_schema_ingestion`

## 9. Common messages and fixes

- "Column 'X' has Dr/Cr amounts. Choose 'Dr is +' or 'Cr is +' for it in Schema & Ingestion." Choose the sign in the Columns card.
- "Could not be converted (now null), e.g.: …" under Parse Failures: wrong type for those values; change the type or clean the file.
- "Could not re-read the file: …" or "Error reading the file preview: …": check separator and encoding.
- "Key export failed" / "Key import failed": check the folder or file you chose.

## 10. Limits and assumptions

- Masks are one-way tokens made with this PC's secret key. Another PC needs the same key to get the same tokens.
- The type is chosen per column for the whole file; mixed formats in one column are not detected.
- Excel files are not read directly; save them as CSV first.
