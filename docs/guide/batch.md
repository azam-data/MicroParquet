# Batch (recursive folder mirror)

## 1. What it does

Converts every CSV in a folder tree to Parquet (or every Parquet to CSV) and writes the results into a mirror of that tree in another folder, with one log of rows and control sums per file. It is for converting a month of extracts in one go, with a record of what was converted.

## 2. When to use it

- Converting the month-end extract folders to Parquet for faster work.
- Producing CSV copies of a Parquet archive.

## 3. Before you start

- An input folder and an empty or new output folder that is not inside the input folder.
- Decide the schema: Smart Safe Auto-Detect (IDs stay text) or a saved `.schema.json` template that fits every file.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Input root folder | Yes | Folder | extracts | Sub-folders are walked. |
| Output root folder | Yes | Folder | parquet_out | Not the input folder and not inside it. |
| Direction | Yes | CSV -> Parquet or Parquet -> CSV | CSV -> Parquet |  |
| Schema | For CSV | Auto-detect or template | Smart Safe Auto-Detect (Locks Leading Zeros to Utf8) | A template needs every file to have its columns. |

## 5. Step by step

1. Choose File → "Recursive batch folder mirror…" (Ctrl+K: "batch").
2. In "Recursive Folder-Tree Mirror Batch Converter" click "1. Input Root Folder..." and "2. Output Root Folder...".
3. Choose "CSV -> Parquet" or "Parquet -> CSV".
4. Choose "Smart Safe Auto-Detect (Locks Leading Zeros to Utf8)" or "Apply Saved .schema.json Template" and "Pick .schema.json...".
5. Click "Start Mirror Batch". Progress and Cancel are in the status bar.
6. When it finishes read the summary: converted, companion files copied, errors, size before and after. Open `batch_log.csv` in the output folder.

## 6. What you get

The folder tree mirrored under the output root, the converted files, any other files copied alongside, and `batch_log.csv` with one line per file: file, rows, data_lines, control_sum, parse_failures, status (OK, WARNING, NOTICE, COPIED or ERROR) and error. If a new masking key is created, the log and the dialog say "New masking key created — export a backup".

## 7. Check your result

- For each file, `rows` equals the file's data lines; a file with fewer rows is marked WARNING.
- `parse_failures` should be 0 (values that became blank).
- Compare each `control_sum` with the extract's own total.

## 8. Worked example

The input tree and the mirrored output.

| Input | Output |
| --- | --- |
| deposits.csv | deposits.parquet |
| 2026-09/loans.csv | 2026-09/loans.parquet |

Test: `guide_batch`

The log for those two files.

| file | rows | control_sum | status |
| --- | ---: | ---: | --- |
| 2026-09/loans.csv | 2 | 1,350.50 | OK |
| deposits.csv | 1 | 500.00 | OK |

Test: `guide_batch`

## 9. Common messages and fixes

- "Choose the input and output folders first".
- "Batch: 'Apply Saved .schema.json Template' is selected but no template is loaded. Pick a .schema.json first."
- A file with status WARNING: rows are fewer than data lines; open the file and look for broken lines (tick "Pre-Heal Broken Unquoted CSV Lines" in the schema) before using it.
- "N value(s) could not be converted and are null (see parse_failures in the log)": the types do not suit those files.
- An output folder inside the input folder is refused.

## 10. Limits and assumptions

- Files that fail are logged as ERROR and the batch continues; check the log.
- One schema strategy for the whole run.
- The Parquet compression follows the Export menu setting (Archive or Power BI).
