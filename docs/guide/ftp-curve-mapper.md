# FTP yield curve mapper

## 1. What it does

Gives each deposit or loan a funds-transfer-pricing (FTP) benchmark rate from a yield curve by its tenor (maturity date minus open date), then the spread to the account's own rate and the annual margin in BDT. It supports product profitability and branch margin reporting.

## 2. When to use it

- Monthly branch or product margin: deposits (cost against FTP) and loans (yield against FTP).
- Re-pricing a book against a new treasury curve.

## 3. Before you start

- Open the extract with the open date and maturity date typed as Dates, and the rate as a number in percent.
- Have the treasury curve ready: tenor in days and FTP rate in percent for each point. The built-in curve is only a starting sample; replace it with the bank's own.
- Decide whether the book is liabilities (deposits, borrowings) or assets.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Open date | Yes | Date | 2026-01-01 | A blank open date has no tenor: the row gets `FTP_Method` = MISSING_DATE and no FTP rate. |
| Maturity date | Yes | Date | 2026-04-11 | Same for a blank maturity. |
| Balance | Yes | Number (BDT) | 1,000,000 | Blank counts as 0. |
| Account rate | Yes | Number (percent) | 9.0 | Blank counts as 0%. |
| Curve points | Yes | Tenor days (whole) and rate % | 30 → 8.75, 91 → 9.50 | One rate per tenor; two points at the same tenor are refused. |
| Book type | Yes | Tick box | Liability book | Decides the sign of the spread. |

## 5. Step by step

1. Open the tab, then choose Banking ▾ → "FTP yield curve mapper…" (Ctrl+K: "ftp").
2. In "Automated FTP Yield Curve Tenor Mapper" pick "Open Date:", "Maturity Date:", "Balance Col:" and "Int Rate Col:".
3. Edit "Benchmark Yield Curve (Tenor Days -> FTP Rate %):": type "Tenor Days:" and "FTP %:" to add a point; remove the sample points you do not want.
4. Choose "Rate between tenor points:" Linear or next point up.
5. Tick "Liability Book (Deposits/Borrowings: Spread = FTP Rate - Account Rate)" for deposits and borrowings; leave it unticked for loans.
6. Click "Map FTP Rates & Compute Margin".

## 6. What you get

`Tenor_Days` (maturity − open, at least 1), `Benchmark_FTP_Rate` (percent), `FTP_Method` (which rule gave the rate: "Linear 91d-182d", "Next point up 91d", "First point 30d (at or below curve start)", "Last point 1825d (beyond curve)" or "MISSING_DATE"), `FTP_Spread_Pct` (liability: FTP − account rate; asset: account rate − FTP) and `Annual_FTP_Margin_BDT` = balance × spread / 100. Below the first curve point the first rate applies, beyond the last the last rate.

## 7. Check your result

- Spread × balance, summed, is the book's FTP margin; compare with the NII from the GL for the same book.
- Spot-check one row: 100 days between the 91-day (9.50) and 182-day (10.10) points = 9.50 + 9/91 × 0.60 = 9.5593.
- Count of MISSING_DATE rows should be zero; the status bar warns when it is not.

## 8. Worked example

Input: two deposits opened on 2026-01-01, BDT 1,000,000 each at 9%, on the sample curve with Linear interpolation, liability book.

| Account | Opndat | Matdat | Balance | Rate |
| --- | --- | --- | ---: | ---: |
| D1 | 2026-01-01 | 2026-04-11 | 1,000,000.00 | 9.00 |
| D2 | 2026-01-01 | 2026-02-01 | 1,000,000.00 | 9.00 |

Test: `guide_ftp_curve_mapper`

Output: the 100-day deposit sits between the 91- and 182-day points; the 31-day deposit between 30 and 91.

| Account | Tenor_Days | Benchmark_FTP_Rate | FTP_Method | FTP_Spread_Pct | Annual_FTP_Margin_BDT |
| --- | ---: | ---: | --- | ---: | ---: |
| D1 | 100 | 9.5593 | Linear 91d-182d | 0.5593 | 5,593.41 |
| D2 | 31 | 8.7623 | Linear 30d-91d | -0.2377 | -2,377.05 |

Test: `guide_ftp_curve_mapper`

## 9. Common messages and fixes

- "The FTP curve has no points. Add at least one tenor point."
- "The FTP curve has two points at N days (a% and b%). Keep one rate per tenor."
- "The FTP curve point at N days has no valid rate."
- "Column 'X' is String, not a date. Set it to a Date type…"
- Status-bar warning about rows with a missing open or maturity date: fix the dates in the source or exclude those rows.

## 10. Limits and assumptions

- One curve for the whole step. Dated curves, "curve on the origination date" and curve files are planned in Phase 4 task V4.
- Tenor is contractual (maturity − open); no repricing dates or amortisation.
- The sample curve is not a market curve; the bank must supply its own.
