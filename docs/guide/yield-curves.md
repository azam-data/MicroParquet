# Yield curves (dated FTP curves)

## 1. What it does

Keeps the treasury yield curves by date, one file per curve in `portable_data/curves`, so the FTP step can price each deal on the curve of the day it was opened. The manager lists the curves, edits their points, and imports a curve from a two-column CSV. It exists so that a deal booked in January keeps its January FTP rate when you run the book in October.

## 2. When to use it

- Each month or when treasury changes the FTP curve: add the new curve with its date.
- Loading a curve that treasury sent as a spreadsheet (save it as CSV and import).
- Correcting a typo in an earlier curve, then re-running the FTP tab.

## 3. Before you start

- The curve from treasury: tenors in days and rates in percent, and the date it takes effect.
- For an import, a CSV with two columns: tenor in whole days, rate in percent (one heading line at the top is skipped, no more; comma, semicolon or tab; a % after the rate is allowed).

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Curve date | Yes | Date, YYYY-MM-DD | 2026-01-01 | The date the curve takes effect. It is the file name (2026-01-01.json); each date has one curve. |
| Currency, source | No | Text | BDT, Treasury circular 12/2026 | Kept in the file for reference. |
| Tenor | Yes | Whole days above 0, increasing | 30, 91, 365 | Strictly increasing: the same tenor twice, or a smaller tenor after a larger one, is refused. |
| Rate | Yes | Number (percent) | 8.00 | 9.5 means 9.5%. |

## 5. Step by step

1. Choose Banking ▾ → "Yield curves (dated FTP curves)…" (under "Treasury & ALM"; or Ctrl+K and type "curves"). It also opens from the "Yield curves…" button of the FTP dialog.
2. The list on the left shows the curves, newest first, with their currency and number of points; click one to edit it. Files that cannot be used are listed under it in red with the reason.
3. To make a new curve click "New curve", type the "Date (YYYY-MM-DD):", "Currency:" and "Source:", and fill the points ("Add point" adds a row; the cross removes one).
4. To import, click "Import CSV…" and choose the file; its points replace the points in the editor. Set the date, then save.
5. When you open a stored curve, a warning above the fields says which curve it is and its FTP_Curve_Id: saving over it changes the FTP of every deal priced on it. Click "Save as new date" to keep that curve as it is: type the new date first, and the changes are written under it. "Save curve" on a changed stored curve asks "Yes, replace it" / "No" first. A message says if the points are not valid; nothing is written then. A date that already has a curve is not overwritten unless you opened that curve.
6. To remove a curve, open it, click "Delete" and confirm with "Yes, delete it".

## 6. What you get

Files in `portable_data/curves`, one per date, in this form: `{"currency": "BDT", "as_of": "2026-01-01", "source": "...", "points": [{"tenor_days": 30, "rate_pct": 8.0}, ...]}`. The FTP step reads them when it runs. Changing the date of an opened curve and saving makes a new file and leaves the old one. The FTP result carries `FTP_Curve_Id`, the first 8 hex characters of a hash of the curve's date, currency, source and points: it changes whenever any of them is edited, whatever the file's spacing.

## 7. Check your result

- Open the FTP result and pivot by `FTP_Curve_Date`: each deal shows the curve you expect.
- Spot-check one deal by hand: between two points the rate is the straight line between them (60 days on the curve below: 8.00 + 30 / 61 × 0.60 = 8.2951).
- A curve file in the folder that is red in the list stops every FTP step that reads the store: fix or delete it.

## 8. Worked example

The CSV as received from treasury (a heading line and three points).

| tenor_days | rate_pct |
| ---: | ---: |
| 30 | 8.00 |
| 91 | 8.60 |
| 365 | 10.00 |

Test: `guide_yield_curves`

The points the manager takes from it.

| tenor_days | rate_pct |
| ---: | ---: |
| 30 | 8.00 |
| 91 | 8.60 |
| 365 | 10.00 |

Test: `guide_yield_curves`

What the curve gives two deposits on 2026-02-01 that mature after 60 and after 200 days (fixed date 2026-06-30, Linear): between the 30- and 91-day points, and between the 91- and 365-day points.

| Account | Tenor_Days | Benchmark_FTP_Rate | FTP_Curve_Date |
| --- | ---: | ---: | --- |
| T60 | 60 | 8.2951 | 2026-01-01 |
| T200 | 200 | 9.1569 | 2026-01-01 |

Test: `guide_yield_curves`

A CSV that gives two rates for 30 days is refused with "Lines 2 and 3 both have the tenor 30 days (8% and 8.5%). Each tenor needs one rate." and nothing is imported.

| tenor_days | rate_pct |
| ---: | ---: |
| 30 | 8.00 |
| 30 | 8.50 |

Test: `guide_yield_curves`

## 9. Common messages and fixes

- "The tenors must increase: 365 days (line 1) is followed by 30 days (line 2)." Sort the CSV by tenor.
- "Lines 2 and 3 both have the tenor 30 days (8% and 8.5%). Each tenor needs one rate." Remove one line.
- "Line N: expected two columns (tenor_days, rate_pct), found 3." / (a second title or heading line gives "Line 2: 'tenor_days' is not a whole number of days." Delete the extra line) / "Line N: 'abc' is not a whole number of days." / "Line N: 'high' is not a rate in percent." / "Line N: the tenor 0 is not a number of days above 0." Fix that line.
- "The file has no points." The CSV is empty.
- "Type the curve's date as YYYY-MM-DD." / "Type the curve's currency (for example BDT). Every curve needs one." in the editor.
- "A curve dated D already exists. Select it in the list to change it, or choose another date."
- In the list, in red: "Curve file X.json: The file name 'X.json' is not a date…", "The file name says A but 'as_of' says B. Make them the same.", "Line N: it is not a valid curve file (…)", "Line N: the file name says A but 'as_of' says B", "Line 4 and line 5 both have the tenor 30 days (8% and 9%)", "365 days (line 4) is followed by 30 days (line 5)", "Line 4: the tenor 0 is not a number of days above 0" (the line is the one in the file), "The currency is missing…" or "Line 2: the currency is empty…". Fix the file in a text editor, or delete it.

## 10. Limits and assumptions

- One curve per date and file. Every file must say its currency; an FTP step uses only the curves in its own Currency (BDT unless changed), so a USD curve is never used for BDT deals. The deals' own currency is not checked: use a tab that holds one currency.
- One damaged or currency-less file in the folder stops every FTP step that reads the store (the step does not guess), until it is fixed or deleted.
- Rates are percent per year at each tenor; there is no conversion between compounding bases.
- A CSV import replaces the points in the editor, it does not merge.
- There is no history of changes: a saved curve overwrites the file of its date.
