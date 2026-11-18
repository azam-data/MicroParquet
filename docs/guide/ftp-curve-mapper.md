# FTP yield curve mapper

## 1. What it does

Gives each deposit or loan a funds-transfer-pricing (FTP) benchmark rate from a yield curve by its tenor (maturity date minus open date), then the spread to the account's own rate and the annual margin in BDT. The curve can be typed in the dialog, or taken from the dated curves in `portable_data/curves` (see Yield curves): the curve on one date, or, for each deal, the curve that applied on the day it was opened. It supports product profitability and branch margin reporting.

## 2. When to use it

- Monthly branch or product margin: deposits (cost against FTP) and loans (yield against FTP).
- Pricing each deal at the curve of its origination date, so an old fixed deposit keeps the FTP rate of the day it was booked.
- Re-pricing a book against a new treasury curve (fixed date).

## 3. Before you start

- Open the extract with the open date and maturity date typed as Dates, and the rate as a number in percent.
- Have the treasury curve ready: either type its points in the dialog (tenor in days, FTP rate in percent), or save dated curves in Banking ▾ → "Yield curves (dated FTP curves)…". The built-in sample curve is only a starting point; replace it with the bank's own.
- Decide whether the book is liabilities (deposits, borrowings) or assets.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Open date | Yes | Date | 2026-01-01 | A blank open date has no tenor (and no origination curve): the row gets `FTP_Method` = MISSING_DATE and no FTP rate. |
| Maturity date | Yes | Date | 2026-04-11 | Same for a blank maturity. |
| Balance | Yes | Number (BDT) | 1,000,000 | Blank counts as 0. |
| Account rate | Yes | Number (percent) | 9.0 | Blank counts as 0%. |
| Curve | Yes | Typed points, or the curve store | 30 → 8.75, 91 → 9.50; or curves/2026-01-01.json | Typed points: one rate per tenor. Store: each curve file's tenors strictly increase. |
| Book type | Yes | Tick box | Liability book | Decides the sign of the spread. |

## 5. Step by step

1. Open the tab, then choose Banking ▾ → "FTP yield curve mapper…" (Ctrl+K: "ftp").
2. In "Automated FTP Yield Curve Tenor Mapper" pick "Open Date:", "Maturity Date:", "Balance Col:" and "Int Rate Col:".
3. Tick "Liability Book (Deposits/Borrowings: Spread = FTP Rate - Account Rate)" for deposits and borrowings; leave it unticked for loans.
4. Under "Yield curve:" choose where the curve comes from:
   - "Stored curves (origination date)": every deal is priced on the curve of its origination date (the latest curve on or before its open date). Tick "Use the earliest curve for older deals" if some deals are older than your oldest curve. The dialog opens on this choice when the folder portable_data/curves has curve files.
   - "Stored curve on or before one date:" and type the date (YYYY-MM-DD): every deal is priced on the latest curve dated on or before that date.
   - Under the two stored choices, "Currency:" (BDT by default): only stored curves in that currency are used. Curves in other currencies are skipped, and the status bar lists them.
   - "Ad-hoc curve (typed below)": edit "Tenor Days:" and "FTP %:" in the list that follows. The dialog opens on this choice when there are no curve files, and an older step that used a typed curve keeps it when you edit it.
5. The line under the choices says how many curves the store has; "Yield curves…" opens the manager to add or change them.
6. Choose "Rate between tenor points:" Linear (the default for a new step) or next point up.
7. Click "Map FTP Rates & Compute Margin".

## 6. What you get

`Tenor_Days` (maturity − open, at least 1), `Benchmark_FTP_Rate` (percent), `FTP_Method` (which rule gave the rate: "Linear 91d-182d", "Next point up 91d", "First point 30d (at or below curve start)", "Last point 1825d (beyond curve)" or "MISSING_DATE"), `FTP_Spread_Pct` (liability: FTP − account rate; asset: account rate − FTP) and `Annual_FTP_Margin_BDT` = balance × spread / 100. Below the first curve point the first rate applies, beyond the last the last rate. With a curve from the store these columns more: `FTP_Curve_Date` (the date of the curve that priced the deal), `FTP_Curve_Id` (8 hex characters that identify the content of that curve: if anyone edits the curve file, the id of the deals priced afterwards changes, so a number in an ALCO pack can be traced to the exact curve) and, with the option for older deals, `FTP_Note` ("earliest curve used" on the deals that are older than every curve).

## 7. Check your result

- Spread × balance, summed, is the book's FTP margin; compare with the NII from the GL for the same book.
- Spot-check one row: 100 days between the 91-day (9.50) and 182-day (10.10) points = 9.50 + 9/91 × 0.60 = 9.5593.
- Keep the `FTP_Curve_Id` of the month-end run with the pack: if a later run shows another id for the same deals, the curve file was edited.
- Pivot the result by `FTP_Curve_Date`: every deal opened in a period should show the curve of that period.
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

Dated curves: three 197-day deposits. The store holds the curves of 2026-01-01 (30 days 8.00%, 365 days 10.00%) and 2026-07-01 (9.00% and 11.00%). Origination curve, option for older deals ticked.

| Account | Opndat | Matdat | Balance | Rate |
| --- | --- | --- | ---: | ---: |
| D1 | 2026-03-15 | 2026-09-28 | 1,000,000.00 | 9.00 |
| D2 | 2026-08-01 | 2027-02-14 | 1,000,000.00 | 9.00 |
| D3 | 2025-12-01 | 2026-06-16 | 1,000,000.00 | 9.00 |

Test: `guide_ftp_curve_mapper`

Output: D1 (opened 2026-03-15) is priced on the January curve: 8 + (197 − 30) / (365 − 30) × 2 = 8.99701. D2 (opened 2026-08-01) is priced on the July curve: 9.99701. D3 was opened before the first curve, so it uses the earliest curve and says so.

| Account | Tenor_Days | Benchmark_FTP_Rate | FTP_Curve_Date | FTP_Note |
| --- | ---: | ---: | --- | --- |
| D1 | 197 | 8.99701 | 2026-01-01 | (blank) |
| D2 | 197 | 9.99701 | 2026-07-01 | (blank) |
| D3 | 197 | 8.99701 | 2026-01-01 | earliest curve used |

Test: `guide_ftp_curve_mapper`

## 9. Common messages and fixes

- "The FTP curve has no points. Add at least one tenor point."
- "The FTP curve has two points at N days (a% and b%). Keep one rate per tenor."
- "The FTP curve point at N days has no valid rate."
- "FTP: N deal(s) were opened before the earliest yield curve (2026-01-01), the oldest on D. Add an earlier curve in Banking > Yield curves, or use the earliest curve for older deals." Add a curve dated on or before the oldest deal, or tick the option.
- "FTP: no yield curve is dated on or before D; the earliest is E." (fixed date). Choose a later date or add an earlier curve.
- "FTP: the curve date 'X' is not a date. Write it as YYYY-MM-DD."
- "There are no yield curves in portable_data/curves." Add one in Yield curves.
- "FTP: there is no yield curve in BDT in portable_data/curves (found 2026-01-01 (USD))…" Every stored curve is in another currency: add a BDT curve, or change "Currency:".
- "Curve file X.json: The currency is missing…" Add `"currency": "BDT"` to the file.
- Status bar: "skipped N curve(s) in another currency than BDT: 2026-03-01 (USD)". Not an error; those curves were not used.
- "Curve file 2026-03-01.json: …" a curve file in the store cannot be used (the reason follows). Fix or delete it; the step does not guess.
- "Column 'X' is String, not a date. Set it to a Date type…"
- Status-bar warning about rows with a missing open or maturity date: fix the dates in the source or exclude those rows.

## 10. Limits and assumptions

- Tenor is contractual (maturity − open); no repricing dates or amortisation.
- A curve applies from its date until the next curve's date; there is no interpolation between two curves in time.
- Curves are read from the files each time the tab is built or refreshed; editing a curve changes the result of every tab that uses the store.
- The sample curve is not a market curve; the bank must supply its own.
