# Interest-rate shock / ΔNII

## 1. What it does

Estimates the one-year change in net interest income (ΔNII) if all rates move in parallel by a number of basis points. Each row's balance is weighted by how much of the next 12 months it reprices (a deal that matures soon reprices early in the year). It supports the IRRBB earnings-at-risk view in the ALCO pack.

## 2. When to use it

- Monthly or quarterly ΔNII for +100 bp, +200 bp and −100 bp scenarios.
- Quick sensitivity of a single product book (for example term deposits).
- Pair with Duration / DV01 / ΔEVE for the economic-value view.

## 3. Before you start

- Open one extract, or stack assets and liabilities into one tab, with a side column telling assets from liabilities.
- Type the maturity column as a Date and the rate column as a number (percent).
- Decide the cutoff date and the shock in basis points (100 bp = 1%).

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Balance | Yes | Number (BDT) | 1,000,000 | Blank balances count as 0 and are reported in the status message. |
| Interest rate | Yes | Number (percent) | 12.0 | Blank rate counts as 0%. |
| Maturity date | Yes | Date | 2027-01-29; blank = reprices at once | Blank or already matured: weight 1.0 (reprices immediately). |
| Side (asset / liability) | Recommended | Text | Asset, Deposit, A, L, Borrowing | A… = asset; L…, Deposit and Borrowing = liability. Other values stop the run. Without it every row is treated as an asset and `Direction_Assumed` says so. |
| Cutoff date and shock | Yes | Typed in the dialog | 2026-10-31, +100 | Shock in basis points; negative for a fall. |

## 5. Step by step

1. Open the tab, then choose Banking ▾ → "Interest rate shock / ΔNII…" (Ctrl+K: "nii").
2. In "Interest Rate Shock / ΔNII Sensitivity Simulator" pick "Balance Col:", "Int Rate Col:", "Maturity Date Col:" and "Asset/Liability Side Col:" (or "(none: treat all rows as assets)").
3. Type "Cutoff Date (YYYY-MM-DD):" and "Parallel Rate Shock (bps, e.g. +100, +200, -150):".
4. Click "Run Rate Shock Simulation". The result opens in a new tab.

## 6. What you get

Three new columns. `Current_Annual_Int` = balance × rate: gross interest income (assets) or expense (liabilities), unsigned. `NII_Impact_BDT` = the change in NII, signed: plus for assets and minus for liabilities when rates rise. `Shocked_Annual_Int` = gross interest after the shock, unsigned; do not add it across assets and liabilities as NII. Weight = 1 if matured or no maturity, 0 if more than 365 days away, otherwise (365 − days to maturity) / 365. Without a side column a `Direction_Assumed` column reads "ASSET (unspecified)".

## 7. Check your result

- Sum `NII_Impact_BDT` over assets and over liabilities separately, then together: that is the net ΔNII.
- A +100 bp shock on a fully repricing book of BDT 1,000,000 changes interest by 10,000 a year; any row's impact is never more than that.
- Rows with weight 0 (long-dated fixed) must be the long maturities; check a few by hand.

## 8. Worked example

Input: a three-month loan and a three-month term deposit, cutoff 2026-10-31, shock +100 bp.

| Product | Side | Balance | Rate | Matdat |
| --- | --- | ---: | ---: | --- |
| Loan 3m | Asset | 1,000,000.00 | 12.00 | 2027-01-29 |
| Term deposit 3m | Deposit | 1,000,000.00 | 9.00 | 2027-01-29 |

Test: `guide_rate_shock_nii`

Output: both rows are 90 days from maturity, weight (365 − 90) / 365 = 0.7534, so ±7,534.25. The two cancel in total.

| Product | Current_Annual_Int | NII_Impact_BDT | Shocked_Annual_Int |
| --- | ---: | ---: | ---: |
| Loan 3m | 120,000.00 | 7,534.25 | 127,534.25 |
| Term deposit 3m | 90,000.00 | -7,534.25 | 97,534.25 |

Test: `guide_rate_shock_nii`

## 9. Common messages and fixes

- "Side column 'X' has values that are neither asset nor liability: 'Loan', … Use A/ASSET for assets and L/LIABILITY/DEPOSIT/BORROWING for liabilities." Fix the codes (the message lists up to 10) or add a mapped side column. Note "Loan" starts with L and is read as a liability, so check it.
- "Column 'X' is String, not a date. Set it to a Date type…" Type the maturity column as a Date.
- "Cutoff Date must be YYYY-MM-DD".
- Blank balance note in the status bar: rows with a blank balance were counted as 0; check the extract.

## 10. Limits and assumptions

- Repricing is assumed at maturity: a floating loan that reprices earlier is treated as a fixed deal until its maturity (planned in Phase 4 task V5: repricing dates, pass-through and floors).
- A parallel shock only; one horizon (12 months); no behavioural prepayment or deposit betas.
- Balances are assumed to be rolled over at the shocked rate only for the repricing share.
