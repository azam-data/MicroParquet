# Interest-rate shock / ΔNII

## 1. What it does

Estimates the one-year change in net interest income (ΔNII) if all rates move in parallel by a number of basis points. Each row's balance is weighted by how much of the next 12 months it reprices (a deal that reprices soon changes its rate early in the year). You can say when a floating deal reprices, how much of the shock a deposit product follows (its beta), and a floor under the rate. A summary table shows the repricing gap by bucket and the total ΔNII. It supports the IRRBB earnings-at-risk view in the ALCO pack.

## 2. When to use it

- Monthly or quarterly ΔNII for +100 bp, +200 bp and −100 bp scenarios.
- Floating loans and deposits with a repricing date and a deposit beta.
- Repricing gap (rate-sensitive assets less liabilities by time to repricing) with the ΔNII beside it.
- Pair with Duration / DV01 / ΔEVE for the economic-value view.

## 3. Before you start

- Open one extract, or stack assets and liabilities into one tab, with a side column telling assets from liabilities.
- Type the maturity (and repricing) columns as Dates and the rate column as a number (percent).
- Decide the cutoff date and the shock in basis points (100 bp = 1%).
- Optional: a beta file in `portable_data/betas/<name>.json` that lists the pass-through % of each product (see the format below). The percentages are your ALCO's; none are built in.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Balance | Yes | Number (BDT) | 1,000,000 | Blank balances count as 0 and are reported in the status message. |
| Interest rate | Yes | Number (percent) | 12.0 | Blank rate counts as 0%. |
| Maturity date | Yes | Date | 2027-01-29; blank = reprices at once | Blank or already matured: weight 1.0 (reprices immediately). |
| Repricing date | No | Date | 2026-11-30 | A floating deal reprices on the earlier of this date and its maturity. A blank repricing date uses the maturity. |
| Side (asset / liability) | Recommended | Text | Asset, Deposit, A, L, Borrowing | A… = asset; L…, Deposit and Borrowing = liability. Other values stop the run. Without it every row is treated as an asset and `Direction_Assumed` says so. |
| Product and beta file | No | Text codes; file in portable_data/betas | SAVINGS; deposit_betas | File: {"source": "...", "betas": [{"product": "SAVINGS", "beta_pct": 40}]}. Codes are matched ignoring case and extra spaces. A product that is not in the file passes 100%. |
| Floor | No | Number (percent) | 0 | The shocked rate never goes below it. |
| Cutoff date and shock | Yes | Typed in the dialog | 2026-10-31, +100 | Shock in basis points; negative for a fall. |

## 5. Step by step

1. Open the tab, then choose Banking ▾ → "Interest rate shock / ΔNII…" (Ctrl+K: "nii").
2. In "Interest Rate Shock / ΔNII Sensitivity Simulator" pick "Balance Col:", "Int Rate Col:", "Maturity Date Col:" and "Asset/Liability Side Col:" (or "(none: treat all rows as assets)").
3. Type "Cutoff Date (YYYY-MM-DD):" and "Parallel Rate Shock (bps, e.g. +100, +200, -150):".
4. Optional, under "Repricing, pass-through and floor": choose "Repricing Date Col:"; choose a "Beta file:" and its "Product Col:"; tick "Floor the shocked rate at" and type the floor in percent.
5. Click "Run Rate Shock Simulation". The result opens in a new tab and is a step you can edit later. Or click "Repricing gap summary" to open the summary table for the same settings in a new tab.

## 6. What you get

Three new columns in the result. `Current_Annual_Int` = balance × rate: gross interest income (assets) or expense (liabilities), unsigned. `NII_Impact_BDT` = the change in NII, signed: plus for assets and minus for liabilities when rates rise. `Shocked_Annual_Int` = gross interest after the shock, unsigned; do not add it across assets and liabilities as NII. Weight = 1 if the effective repricing date is on or before the cutoff or there is none, 0 if more than 365 days away, otherwise (365 − days) / 365. The rate change is the shock times the beta (100% when there is no beta), cut so the rate does not fall below the floor; a rise is never cut, and a rate already under the floor does not fall. Without a side column a `Direction_Assumed` column reads "ASSET (unspecified)".

The summary table has one row for each repricing bucket (0–1m, 1–3m, 3–6m, 6–12m, >12m/non-repricing) and a Total row: `Assets` (positive), `Liabilities` (negative), `Gap` (assets plus liabilities), `Cumulative gap` and `ΔNII (BDT)` for those rows. Rows that reprice at once (no date, or a date on or before the cutoff) are in 0–1m.

## 7. Check your result

- Sum `NII_Impact_BDT` over assets and over liabilities separately, then together: that is the net ΔNII. It equals the Total of `ΔNII (BDT)` in the summary.
- A +100 bp shock on a fully repricing book of BDT 1,000,000 changes interest by 10,000 a year; no row's impact is more than that (a beta, a floor or a weight only reduces it).
- Rows with weight 0 (long-dated fixed) must be the long maturities; check a few by hand.
- The status bar says how many rows have a product that is not in the beta file: look at them.

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

With a repricing date, a beta and a floor: a floating loan that reprices on 2026-11-30 (and matures in 2030) and a savings account with no maturity. Cutoff 2026-10-31.

| Product | Side | Balance | Rate | Matdat | Reprice |
| --- | --- | ---: | ---: | --- | --- |
| FLOAT LOAN | Asset | 1,000,000.00 | 12.00 | 2030-06-30 | 2026-11-30 |
| SAVINGS | Liability | 1,000,000.00 | 1.50 | (blank) | (blank) |

Test: `guide_rate_shock_nii`

+100 bp, repricing date chosen, beta file with SAVINGS at 40%: the loan reprices in 30 days, weight (365 − 30) / 365, so +9,178.08 (without the repricing date it would be 0.00: it matures after 12 months). The savings rate follows 40% of the shock: −4,000.00.

| Product | NII_Impact_BDT |
| --- | ---: |
| FLOAT LOAN | 9,178.08 |
| SAVINGS | -4,000.00 |

Test: `guide_rate_shock_nii`

−200 bp on the savings account at 1.50%, beta 100%: without a floor the cost falls by 20,000.00; with a floor of 0% the rate can fall only 1.50%, so the saving is 15,000.00.

| Floor | NII_Impact_BDT of SAVINGS |
| --- | ---: |
| none | 20,000.00 |
| 0.00% | 15,000.00 |

Test: `guide_rate_shock_nii`

Repricing gap summary: six rows (a blank maturity is a deposit that reprices at once).

| Side | Balance | Matdat |
| --- | ---: | --- |
| Asset | 100.00 | 2026-11-15 |
| Asset | 200.00 | 2027-01-15 |
| Liability | 50.00 | 2027-03-15 |
| Liability | 80.00 | 2027-08-01 |
| Asset | 300.00 | 2030-01-01 |
| Liability | 40.00 | (blank) |

Test: `guide_rate_shock_nii`

The summary at +100 bp: liabilities are negative, the gap is assets plus liabilities, and the ΔNII column adds up to the net ΔNII of the step.

| Repricing bucket | Assets | Liabilities | Gap | Cumulative gap | ΔNII (BDT) |
| --- | ---: | ---: | ---: | ---: | ---: |
| 0–1m | 100.00 | -40.00 | 60.00 | 60.00 | 0.56 |
| 1–3m | 200.00 | 0.00 | 200.00 | 260.00 | 1.58 |
| 3–6m | 0.00 | -50.00 | -50.00 | 210.00 | -0.32 |
| 6–12m | 0.00 | -80.00 | -80.00 | 130.00 | -0.20 |
| >12m/non-repricing | 300.00 | 0.00 | 300.00 | 430.00 | 0.00 |
| Total | 600.00 | -170.00 | 430.00 | (blank) | 1.63 |

Test: `guide_rate_shock_nii`

## 9. Common messages and fixes

- "Side column 'X' has values that are neither asset nor liability: 'Loan', … Use A/ASSET for assets and L/LIABILITY/DEPOSIT/BORROWING for liabilities." Fix the codes (the message lists up to 10) or add a mapped side column. Note "Loan" starts with L and is read as a liability, so check it.
- "Column 'X' is String, not a date. Set it to a Date type…" for the maturity or the repricing date column.
- "Cutoff Date must be YYYY-MM-DD".
- "ΔNII: choose the product column the beta file is read against." Pick the Product Col.
- "The beta file 'x' was not found in …\betas." / "The beta file x.json is not valid…" / "…the beta of 'SAVINGS' is 120, not a percentage from 0 to 100." / "…lists 'SAVINGS' twice…" Fix the file.
- "ΔNII: the floor is not a number."
- Status bar: "Rate shock: N row(s) have a product that is not in the beta file 'x' (or no product): they pass through 100%". Add the products to the file if that is wrong.
- Blank balance note in the status bar: rows with a blank balance were counted as 0; check the extract.

## 10. Limits and assumptions

- A parallel shock only, over a 12-month horizon; no behavioural prepayment, no repricing of the new business rolled over, no basis risk between products.
- A beta is the share of the shock the rate follows, applied to the whole shock in either direction; asymmetric betas (up and down) are not modelled.
- A floor applies to a fall in the rate only; caps and options are not modelled.
- The effective repricing date is the earlier of the repricing date and the maturity; a deal whose repricing date is already past is treated as repricing at once.
- The summary groups by the effective repricing date and uses balances, not the ΔNII-weighted amounts.
