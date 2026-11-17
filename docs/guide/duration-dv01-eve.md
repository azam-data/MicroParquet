# Duration, DV01 and ΔEVE

## 1. What it does

Calculates each row's residual years, Macaulay duration, modified duration, DV01 (the change in value for one basis point) and the change in economic value of equity (ΔEVE) for a parallel shock. It supports the IRRBB economic-value measure. Each row is treated as a bullet instrument whose coupon equals its rate.

## 2. When to use it

- Quarterly ΔEVE and duration gap for ALCO and the IRRBB report.
- DV01 limit monitoring for the investment book.
- Comparing the value sensitivity of assets and liabilities.

## 3. Before you start

- An extract of assets and liabilities (stack them if they are in separate files) with a side column.
- Maturity typed as a Date, rate as a number in percent.
- Cutoff date, compounding frequency, the shock in basis points and the residual tenor you want for blank maturities (non-maturing deposits).

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Balance | Yes | Number (BDT) | 1,000,000 | Blank counts as 0. |
| Interest / yield rate | Yes | Number (percent) | 10.0 | Below 0.01% is treated as 0.01% to avoid dividing by zero. |
| Maturity date | Yes | Date | 2031-10-30; blank = default tenor | Blank rows use the Null Matdat Tenor (Yrs) you type. |
| Side (asset / liability) | Recommended | Text | Asset, Deposit | As in the ΔNII tool. Without it all rows are treated as assets and `Direction_Assumed` says so. |
| Cutoff, frequency, shock | Yes | Typed in the dialog | 2026-10-31, 1, +200 | Frequency: 1 annual, 2 semi, 4 quarterly, 12 monthly. |

## 5. Step by step

1. Open the tab, then choose Banking ▾ → "Duration, DV01 and ΔEVE…" (Ctrl+K: "eve").
2. In "IRRBB Duration, DV01 & ΔEVE Sensitivity Engine" pick "Balance Col:", "Int Rate / Yield Col (%):", "Maturity Date Col:" and "Asset/Liability Side Col:".
3. Type "Cutoff Date (YYYY-MM-DD):", "Compounding Frequency per Year (1=Ann, 2=Semi, 4=Qtr, 12=Mo):", "Null Matdat Tenor (Yrs):" and "Parallel Rate Shock for ΔEVE (bps):".
4. Click "Compute Duration & ΔEVE Columns".

## 6. What you get

`Residual_Years_T` (years to maturity; at least 1/365; blank maturity uses your default tenor), `Macaulay_Duration_Yrs` and `Modified_Duration` (4 decimals), `DV01_BPV_BDT` (balance × modified duration × 0.0001, 2 decimals, always positive) and `Delta_EVE_Impact_BDT` (signed: a rate rise lowers the value of an asset, so ΔEVE is negative for assets and positive for liabilities). Without a side column, `Direction_Assumed` reads "ASSET (unspecified)".

## 7. Check your result

- A 5-year 10% annual bullet at par has Macaulay duration 4.1699 and modified duration 3.7908 (textbook values).
- ΔEVE = −balance × modified duration × shock for assets (sign flipped for liabilities). Check one row by hand.
- Total ΔEVE across assets and liabilities, divided by Tier 1 capital, is the figure compared with the IRRBB limit.

## 8. Worked example

Input: a 5-year bond-like asset and a 5-year deposit, both BDT 1,000,000 at 10%, cutoff 2026-10-31, +200 bp, annual compounding.

| Product | Side | Balance | Rate | Matdat |
| --- | --- | ---: | ---: | --- |
| Bond (asset) | Asset | 1,000,000.00 | 10.00 | 2031-10-30 |
| Deposit | Deposit | 1,000,000.00 | 10.00 | 2031-10-30 |

Test: `guide_duration_dv01_eve`

Output: both rows have duration 4.1699 and modified duration 3.7908; ΔEVE is −75,816.00 for the asset and +75,816.00 for the deposit.

| Product | Residual_Years_T | Macaulay_Duration_Yrs | Modified_Duration | DV01_BPV_BDT | Delta_EVE_Impact_BDT |
| --- | ---: | ---: | ---: | ---: | ---: |
| Bond (asset) | 5 | 4.1699 | 3.7908 | 379.08 | -75,816.00 |
| Deposit | 5 | 4.1699 | 3.7908 | 379.08 | 75,816.00 |

Test: `guide_duration_dv01_eve`

## 9. Common messages and fixes

- "Side column 'X' has values that are neither asset nor liability…" Fix the side codes (see ΔNII).
- "Column 'X' is String, not a date. Set it to a Date type…" Type the maturity column as a Date.
- "Cutoff Date must be YYYY-MM-DD".

## 10. Limits and assumptions

- Each row is a bullet at par (coupon = rate); amortising loans and instruments with a different coupon are not modelled.
- Non-maturing deposits use the single default tenor you type; behavioural treatment is planned for Phase 5.
- Parallel shock only; no convexity term.
