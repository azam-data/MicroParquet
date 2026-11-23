# Report packs

## 1. What it does

Turns a monthly report into a file. A pack names the input files it needs (slots such as Loans and Deposits), the sheets it makes (a slot's table, a pivot, or a saved tool run such as the gap ladder or the CL-1 summary) and the output. Next month you choose that month's files and the cutoff date, and the pack writes one Excel workbook: a Cover sheet, then one sheet per item. Nothing is written unless every item worked.

## 2. When to use it

- The ALCO pack or a return that is the same set of tables every month.
- A handover: a colleague runs the pack from one file instead of repeating twenty steps.
- A check that last month's pack and this month's use the same rules, files and cutoff (the Cover says which).

## 3. Before you start

- Open this month's files (or last month's) in tabs and make each table once: schema, steps, then the result tabs you want (gap ladder, CL-1, pivot).
- Save the schema with "Save Schema Template" if the file's columns have a fixed layout (see [Schema & Ingestion](schema-ingestion.md)).
- The tools of the items need their rule files in `portable_data` (see each tool's guide). A sample file is marked on the Cover.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Each slot's file | Yes, one per slot | CSV or Parquet | `loans_oct.csv` | Every column the slot was made with must be in the file (the order does not matter); one that is missing stops the run and is listed. |
| Columns of the items | As each tool needs | As the tool says | BAL, MAT, GRADE | Taken from the tabs the items were made from. |
| Cutoff date | Yes | YYYY-MM-DD | 2026-10-31 | Replaces the cutoff of every item that has one (gap ladder, IRRBB, repricing gap, IFRS 9). |

## 5. Step by step

1. Banking ▸ Reports ▸ "Report packs…" (Ctrl+K: "Report packs").
2. Open a file's tab and choose "Add input slot from open tab". The slot keeps the tab's schema and steps; rename it (for example Loans).
3. Open a result tab (a tool's result or a pivot) and choose "Add item from result tab". The item reads the slots whose tabs it was made from. "Add its table as an item" on a slot row adds the slot's own table.
4. Rename items and sheets, and move them with the arrows. Tick "Also write a PDF" if you want one; choose the money scale and decimals.
5. Type a name and choose "Save pack" (`portable_data/packs/<name>.json`).
6. To run: choose a file for each slot, type the cutoff date and, if you like, the workbook's name, then "Run pack". The status bar shows the item being made.
7. From the command line: `MicroParquet --headless --pack monthly --slot Loans=loans_oct.csv --slot Deposits=deposits_oct.csv --cutoff 2026-10-31 --export-xlsx out.xlsx` gives the same workbook.

## 6. What you get

One workbook. Its first sheet, "Cover", lists the pack, the run time, the app version, the cutoff date, each input (slot, file name, rows after the slot's steps, blake3 hash of the file), each sheet, and every rule, curve, profile and parameter file the items read (with its date, source and a "Sample" flag). If any of them is a sample, a Warning line says so. Then one sheet per item, numbered as in every export ("2_Gap ladder").

## 7. Check your result

- The hash of a file on the Cover is its blake3 hash: the same file gives the same hash, and a changed file does not.
- Run the pack twice on the same files: every sheet is the same; only the run time on the Cover differs.
- Compare one sheet with the tab you made it from.

## 8. Worked example

One slot, Deposits, with a pivot of the balance by product.

| ACNO | BAL | PROD |
| --- | ---: | --- |
| D1 | 100.00 | SB |
| D2 | 50.00 | TD |
| D3 | 25.00 | TD |

Test: `guide_report_packs`

The Cover's input line:

| Name | Detail | Rows |
| --- | --- | --- |
| Deposits | deposits.csv | 3 |

Test: `guide_report_packs`

The sheet "By product":

| PROD | Sum of BAL |
| --- | ---: |
| SB | 100.00 |
| TD | 75.00 |
| Grand total | 175.00 |

Test: `guide_report_packs`

### Charts in a pack

An item can carry a chart. In the Items card, each item has a **Chart** choice under its row:

- **No chart** (the default);
- **Saved chart: <name>**, a chart saved from the Chart tab (it must use columns the item's table has);
- **Template: <name>**, one of the banking templates (for example Gap ladder on the liquidity gap ladder item). The yield-curves template is not offered: it reads the curve store, not a table.

When the pack runs, the chart is calculated over the item's whole table, in the pack's money scale and decimals. The workbook gets, right after the item's sheet, a sheet called "<sheet> chart" with the chart's table and a native, editable Excel chart beside it (a template with a companion chart, such as the LCR/NSFR HQLA composition, gets "<sheet> chart 2" too). The Cover lists each chart under its sheet. Tick **Also write an HTML chart book** in the Output card to write `<pack>-charts.html` next to the workbook: every chart of the pack on one page, dated at the cutoff.

Headless: a pack that asks for the book writes `<pack>-charts.html` beside the `--export-xlsx` file; `--export-html <file>` names the book yourself (and asks for it even when the pack does not). A pack with no chart cannot make a book, and says so.

If any chart cannot be drawn (the saved chart is missing, a column is missing, the template does not fit the item's table) the run stops with the item's name and the reason, and **no file is written**: not the workbook, not the book.

The data-quality template takes its Green and Amber from the rule set stored with the scorecard item, and the IRRBB template its outlier limit from the result's own outlier row (the shock file's outlier_pct): none is assumed. A pack file whose scorecard item has a chart but no thresholds is refused when it is opened, naming the item and the field; the data-quality template on an item that is not a scorecard stops the run.

Limits: a chart sheet's name is the item's sheet plus " chart", so the sheet name of a charted item is at most 20 characters; a pack chart whose Data table is on gets a page of its own in the PDF after its item's table (the chart and its table); a chart with the option off is not in the PDF.

## 9. Common messages and fixes

- "The pack 'monthly' needs a file for the slot 'Deposits'." Choose the file (headless: `--slot Deposits=<file>`).
- "The pack has no slot called 'X'." A typing mistake in `--slot`; the message lists the slots.
- "Slot 'Loans': the file … lacks the column(s) …" The file is not the layout the slot was made with: use the right file or make the slot again.
- "Slot 'Loans': values that could not be converted…" Fix the file or the slot's schema; a pack does not leave such values out.
- "The item 'Gap ladder' (sheet 'Gap') failed: …" The reason follows (a missing column, a missing rule file). No workbook is written.
- "The item 'Deposits table' (sheet 'Deposits') could not be charted (chart: …)": the saved chart is not in the charts folder, needs a column the table lacks, or the template does not fit. Fix the item's Chart choice; nothing is written.
- "Add the input slot from the open tab 'Loans' first" when adding an item: add that tab as a slot, then the item.

## 10. Limits and assumptions

- Items made from a tool keep that tool's inputs; only the cutoff date is replaced. Dates typed inside a slot's steps (for example a rate-shock step) are not.
- The files listed on the Cover are those read while the slots and items were made; rule files read only by a step inside a slot are listed too, but a file read in some other way is not.
- A slot with a total of values that did not convert, or a mixed-sign weight, is reported on the Cover or stops the run; a pack does not guess.
- Packs hold names and settings, never data; keep them out of the shared repository if they name internal files.
