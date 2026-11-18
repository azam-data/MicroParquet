# Maturity bucketer (SLP / LCR / custom)

## 1. What it does

Puts every row into a remaining-maturity bucket (for example "02. 2 to 7 Days") by comparing its maturity date with a cutoff date. It is the first step of a liquidity gap or Structural Liquidity Profile (SLP) table, and of the Basel III LCR/NSFR time bands. Bucket names start with a number, so they sort in time order.

## 2. When to use it

- Monthly SLP and ALCO liquidity-gap pack: bucket the deposit and loan extracts, then Pivot by bucket.
- LCR / NSFR time-band checks.
- Any report that needs "residual maturity" bands of your own (make a custom preset).

## 3. Before you start

- Open the extract (deposits, loans, investments) with the maturity column typed as a Date in Schema & Ingestion.
- Decide the cutoff date (the reporting date, usually the month end) as YYYY-MM-DD.
- Pick a preset, or make your own with Create Custom Preset (saved in `portable_data/buckets.json`).

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Maturity date | Yes | Date (not text) | 2026-11-05; blank for current/savings | D/M/Y dates are fine once the column is typed as a date. A text column is refused. |
| Cutoff date | Yes | Typed in the dialog, YYYY-MM-DD | 2026-10-31 | Not a column. |
| Preset | Yes | Chosen in the dialog | Bangladesh Bank SLP (9 Liquidity Buckets) | Presets hold the day limits and labels. |
| Output column name | Yes | Text | Maturity_Bucket | Default name; change it if the tab already has that column. |

## 5. Step by step

1. Open a tab with the data, then choose Banking ▾ → "Maturity bucketer (SLP / LCR / custom)…" (or press Ctrl+K and type "maturity").
2. In "Maturity Bucketer & Custom Preset Creator", set "Maturity Date Column:".
3. Type the "Cutoff Date (YYYY-MM-DD):".
4. Choose a preset in "Bucketing Preset:".
5. Tick or untick "Treat Null Maturity Dates (e.g. Current/Savings) as Sight/Overnight". Ticked: a blank maturity goes to the preset's sight bucket. Unticked: it goes to the preset's long bucket.
6. Check "Output Bucket Column Name:" (default Maturity_Bucket).
7. Click "Generate Maturity Buckets". The step is added to the tab; open the Steps tab to edit or turn it off.
8. To make your own preset: click "Create Custom Preset (e.g. c-m-b-1)", enter "Custom Preset Name:", fill the "Up to Days:" / "Bucket Label:" rows ("Add Day Threshold Row" adds one), set "Above Max Days Label:", "Overdue (<0 Days) Label:" and "Null Matdat (Sight) Label:", then "Save Custom Preset to Dropdown".

## 6. What you get

One new text column (default `Maturity_Bucket`). Remaining days = maturity date − cutoff date. A row goes to the first bucket whose "Up to Days" is at least its remaining days (so a deal maturing on the cutoff date plus 7 days is in the 7-day bucket). A negative remaining time gets the overdue label. A blank maturity gets the sight label or the long label (step 5). Anything beyond the last limit gets the "Above Max Days" label. All other columns are unchanged and no row is added or removed.

## 7. Check your result

- The row count and the control total (pinned column sum) must not change.
- Pivot the new column by balance: the grand total must equal the sum of the balance column before bucketing.
- Look at the overdue bucket: matured deals still on the books are usually a data problem (see Hygiene audit).

## 8. Worked example

Input: five deposits and loans, cutoff 2026-10-31, preset Bangladesh Bank SLP (9 Liquidity Buckets), blank maturity treated as sight.

| Actnum | Balance | Matdat |
| --- | ---: | --- |
| D1 | 100.00 | 2026-11-05 |
| D2 | 200.00 | 2027-03-15 |
| D3 | 300.00 | 2031-01-01 |
| D4 | 400.00 | (blank) |
| D5 | 500.00 | 2026-10-15 |

Test: `guide_maturity_bucketer`

Output: the new column. D5 matured on 2026-10-15, before the cutoff, so it is overdue.

| Actnum | Maturity_Bucket |
| --- | --- |
| D1 | 02. 2 to 7 Days |
| D2 | 05. 3 to 6 Months (91-180d) |
| D3 | 08. 3 to 5 Years |
| D4 | 01. Overnight / Sight (Non-Maturing) |
| D5 | 10. Overdue / Matured Unencashed |

Test: `guide_maturity_bucketer`

## 9. Common messages and fixes

- "Column 'X' was not found. Check the date column chosen in this step." The column was renamed or dropped before this step. Pick the column again (Edit on the step).
- "Column 'X' is String, not a date. Set it to a Date type in the Schema & Ingestion settings." Re-open the file and set the column type to a Date format, or add Set to Number / date typing first.
- "Cutoff Date must be in YYYY-MM-DD format (e.g., 2026-10-31)". Type the date with four-digit year first.

## 10. Limits and assumptions

- Buckets use calendar days from the cutoff; no business-day or holiday adjustment.
- Contractual maturity only. Behavioural treatment of non-maturing deposits (core/volatile CASA) is planned for Phase 5; today blank maturities go to one bucket.
- A maturity before the cutoff is "overdue", whatever the status of the account.
