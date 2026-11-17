# Split export

## 1. What it does

Writes one file per value of a column (for example one Excel or CSV file per branch) from the current tab, and a manifest that proves the files hold every row. It prevents the common mistake of sending a branch's data in a file that is missing rows.

## 2. When to use it

- Sending each branch or zone its own file.
- Splitting a large extract by month or product for loading elsewhere.

## 3. Before you start

- Open the tab with the data to split; apply your filters first (the split follows the steps).
- Pick the folder to write to (create an empty one).
- Pin the control column (Balance) so the manifest carries per-file totals.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Split column | Yes | Any column | Branch | A blank value (nothing, or only spaces) goes to BLANK_KEY. |
| Format | Yes | Parquet, CSV or Excel | CSV | Same options as the normal exports. |
| Output folder | Yes | Folder | branch_files | Use an empty folder. |

## 5. Step by step

1. Open the tab and choose Export ▾ → "Split export by column…" (Ctrl+K: "split").
2. In "Smart Partitioned Split Exporter" choose "Partition / Split Column:".
3. Choose the format (Parquet, CSV or Excel).
4. Click "Select Output Folder & Split Now..." and pick the folder.
5. Watch the status bar; open the folder when it says the split is done.

## 6. What you get

One file per value, named after the value with characters that Windows does not allow replaced by "_" (names that collide, ignoring case, get _2, _3). Rows with a blank key are in `BLANK_KEY`. `split_manifest.csv` lists key, file, rows and control_sum per file. The program checks that the files hold every source row (and the same control sum) and stops with an error otherwise.

## 7. Check your result

- Sum of `rows` in the manifest equals the tab's row count; sum of `control_sum` equals the control total.
- Open one file and compare its row count with the manifest.

## 8. Worked example

Input: five accounts; one has no branch and one has a branch of spaces only.

| Actnum | Branch | Balance |
| --- | --- | ---: |
| A1 | DHK | 100.00 |
| A2 | CTG | 250.00 |
| A3 | DHK | 300.00 |
| A4 | (blank) | 40.00 |
| A5 | (blank) | 10.00 |

Test: `guide_split_export`

Output: the manifest. The two blank branches are in one file.

| key | file | rows | control_sum |
| --- | --- | ---: | ---: |
| (blank) | BLANK_KEY.csv | 2 | 50.00 |
| DHK | DHK.csv | 2 | 400.00 |
| CTG | CTG.csv | 1 | 250.00 |

Test: `guide_split_export`

## 9. Common messages and fixes

- "Split export: the N files hold X rows but the source has Y. Do not use these files; see split_manifest.csv." Do not send the files; report it.
- "Split export: the files' control sum … differs from the source's …".
- "Could not create the output folder": choose a folder you may write to.
- Cancel during the run removes the files it already wrote.

## 10. Limits and assumptions

- Excel output is one workbook per key.
- File names come from the key text; check them before sending.
