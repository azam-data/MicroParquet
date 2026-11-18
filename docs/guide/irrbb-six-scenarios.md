# IRRBB (six scenarios): ΔEVE and ΔNII

## 1. What it does

Runs the six interest-rate shock scenarios of the Bangladesh Bank IRRBB guideline (BRPD *Guidelines on Interest Rate Risk in the Banking Book*, 13 April 2021, §14.2 and Table 2) on your banking book: parallel up and down, short up and down, steepener and flattener. For each it gives the change in economic value of equity (ΔEVE), the loss as a percentage of Tier 1 capital, the outlier test of §13.1 (a loss of 15% of Tier 1 or more) and the change in net interest income (ΔNII, Appendix 2 Table B). It uses the IRRBB cash-flow ladder for the cash flows and, if you choose one, a behaviour profile.

## 2. When to use it

- Quarterly or half-yearly IRRBB return and the ALCO's rate-risk pack.
- Checking the outlier test after a change in the balance sheet or in the BDT shock sizes.
- Comparing the effect of behavioural assumptions (profile on or off).

## 3. Before you start

- Open the asset and liability extracts as tabs, with the columns of the IRRBB cash-flow ladder (outstanding, maturity date, rate, repricing date, product, currency, as needed).
- A yield curve of each currency dated on or before the cutoff: Banking ▸ Treasury & ALM ▸ Yield curves.
- The shock file `portable_data/rules/irrbb_shocks/<date>.json` (BB Table 2: BDT parallel 400, short 500, long 300 bp, decay x = 4, outlier 15%; built in as the 2021-04-13 file). The app uses the latest file dated on or before the cutoff. When Bangladesh Bank revises the BDT shocks, add a newer dated file.
- Optional: a behaviour profile and a beta file for ΔNII. Know your Tier 1 capital in BDT.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Side | Yes | Text, or the tab's role | Asset, Liability | As in the cash-flow ladder. |
| Outstanding amount | Yes | Number, not negative | 1,000,000.00 | BDT or BDT-equivalent. |
| Maturity date | Yes | Date | 2031-04-30 | Blank only for non-maturity deposits covered by the profile, or for any row when "Treat blank maturity as O/N" is ticked (it is in the dialog's header next to "Behaviour profile"). |
| Rate (%) | With interest, or a ΔNII floor | Number | 9.5 | Not needed in "Principal only" without a floor. |
| Next repricing date, rate type, repayment type, frequency | No | Date / text | Floating, Monthly | As in the cash-flow ladder; floating rows need the repricing date. |
| Product code | With a profile or a beta file | Text | SAV01 | |
| Currency | No | Text | BDT, USD | Each currency has its own curve and shock sizes. |

## 5. Step by step

1. Open the tabs. Choose Banking ▸ Treasury & ALM ▸ "IRRBB (six scenarios): ΔEVE and ΔNII…" (or Ctrl+K and type "irrbb").
2. In the top rows type the "Cutoff date (YYYY-MM-DD)" and the "Tier 1 capital (BDT)", and choose the "Cash-flow mode".
3. Choose "Curve rates are": "Annual compounding (converted with ln(1 + y))" (the default) or "Continuous".
4. Tick "Post-shock floor (%)" only if you want the shocked rate never below a floor (the guideline sets none). Choose a "Behaviour profile" and a "Beta file (ΔNII)" if you use them; tick "ΔNII rate floor (%)" to stop falls in loan rates at a floor.
5. Fill the card of each tab (the roles and columns) and use "Add tab" for more. A red line at the top says what is missing. A yellow "Sample parameters" line means the shock file is marked sample.
6. Click "Run IRRBB". Two tabs open: "IRRBB cash flows" and "IRRBB results". To send both to one Excel workbook, use File ▸ "Export tabs to an Excel workbook…" and tick both.

## 6. What you get

"IRRBB results": one row per scenario with `EVE change` (negative for a loss), `Loss` (positive; 0 for a gain), `Loss % of Tier 1` and `ΔNII` (positive is more income). Below: "Maximum loss" (the largest loss, its percentage and scenario), "Outlier" (Yes when the maximum loss is 15% of Tier 1 or more; the 15% comes from the rule file), "Tier 1 capital", "Base EVE", and the files used: the shock rule file and its source, the curve file of each currency, the compounding and the behaviour profile. With several currencies each runs with its own curve and shocks, `EVE change` and `ΔNII` are the sums, and `Loss` is the sum of each currency's loss with no offsets (BB §15.32).

"IRRBB cash flows": the cash-flow ladder of the base case (19 buckets, assets, liabilities, net, cumulative net) with the `Discount factor` e^(−R·t) at each bucket's midpoint and the `Base PV`, and a "Base EVE" row per currency.

How it works: EVE change = Σ (net cash flow of the bucket) × (e^(−(R + ΔR)·t) − e^(−R·t)), with R the curve rate (interpolated linearly at the midpoint; ln(1 + y) under annual compounding) and ΔR the scenario's shock at the midpoint: parallel ±R_par; short ±R_short·s(t); steepener −0.65·R_short·s(t) + 0.9·R_long·(1 − s(t)); flattener +0.8·R_short·s(t) − 0.6·R_long·(1 − s(t)), with s(t) = e^(−t/4). ΔNII uses each row's repricing time t (the earlier of its repricing date and maturity), the shock ΔR(t) there, the weight 1 − t (1 if it reprices now, 0 beyond a year) and the betas and floor of the interest-rate shock tool.

## 7. Check your result

- Base EVE should be near the value of equity on your books on a present-value basis; compare with the market-value or capital figures of the ALM team.
- A parallel up loss on a book with long assets funded short is the largest; the steepener's and flattener's are usually smaller.
- Re-run with another Tier 1 to see where the 15% line falls: Loss % is Loss ÷ Tier 1 × 100.
- For ΔNII compare with the interest rate shock tool on one scenario for a row whose repricing time is known.

## 8. Worked example

Cutoff 2026-10-31, Tier 1 1,000,000, "Principal only", a flat 10% BDT curve with continuous compounding, the BB 2021 shocks. The liability has matured, so it is in O/N; the asset falls in the 4–5Y bucket.

| Account_No | Side | Outstanding | Matdat |
| --- | --- | ---: | --- |
| A1 | Asset | 1,000,000.00 | 2031-04-30 |
| L1 | Liability | 600,000.00 | 2026-10-31 |

Test: `guide_irrbb_six_scenarios`

"IRRBB cash flows" (the buckets with an amount): base EVE 37,796.13.

| Bucket | Midpoint (years) | Net | Discount factor | Base PV |
| --- | ---: | ---: | ---: | ---: |
| O/N | 0.0028 | -600,000.00 | 0.99972 | -599,832.02 |
| 4–5Y | 4.5 | 1,000,000.00 | 0.637628 | 637,628.15 |
| Base EVE | (blank) | (blank) | (blank) | 37,796.13 |

Test: `guide_irrbb_six_scenarios`

"IRRBB results": the maximum loss is the parallel up, 104,969.17, which is 10.50% of Tier 1: not an outlier.

| Scenario | EVE change | Loss | Loss % of Tier 1 | ΔNII | Note |
| --- | ---: | ---: | ---: | ---: | --- |
| Parallel up | -104,969.17 | 104,969.17 | 10.50 | -24,000.00 | (blank) |
| Parallel down | 125,684.16 | 0.00 | 0.00 | 24,000.00 | (blank) |
| Steepener | -21,723.32 | 21,723.32 | 2.17 | 19,500.00 | (blank) |
| Flattener | -2,309.54 | 2,309.54 | 0.23 | -24,000.00 | (blank) |
| Short up | -44,832.32 | 44,832.32 | 4.48 | -30,000.00 | (blank) |
| Short down | 48,236.10 | 0.00 | 0.00 | 30,000.00 | (blank) |
| Maximum loss | (blank) | 104,969.17 | 10.50 | (blank) | Parallel up |
| Outlier (max loss ≥ 15% of Tier 1) | (blank) | (blank) | (blank) | (blank) | No |

Test: `guide_irrbb_six_scenarios`

With Tier 1 of 600,000 the same loss is 17.49% of Tier 1: an outlier.

| Scenario | Loss | Loss % of Tier 1 | Note |
| --- | ---: | ---: | --- |
| Maximum loss | 104,969.17 | 17.49 | Parallel up |
| Outlier (max loss ≥ 15% of Tier 1) | (blank) | (blank) | Yes |

Test: `guide_irrbb_six_scenarios`

## 9. Common messages and fixes

- "Type the Tier 1 capital in BDT (a number above 0)…" The outlier test is a percentage of it.
- "IRRBB: there is no BDT yield curve in portable_data/curves…" and "the earliest BDT curve is dated …, after the cutoff". Add a curve dated on or before the cutoff in Banking ▸ Yield curves.
- "The IRRBB shock file … has no shocks for USD." Add a USD entry to a dated shock file (Bangladesh Bank sets sizes per currency).
- "There is no irrbb_shocks file dated on or before the cutoff …" Add a dated file, or use a later cutoff.
- "The rule file … says as_of … but its name says …" and "… has no source". Fix the file: every rule file needs `as_of` equal to its date and a `source`.
- "ΔNII: the current rate is blank, and a floor is set …" with the count and the first 5 account numbers: fill the rates or untick the floor.
- The messages of the cash-flow ladder (blank maturity, unknown frequency, and so on) apply here too. "Treat blank maturity as O/N" works as in the ladder: those rows are in O/N with principal only, and for ΔNII they reprice at once.

## 10. Limits and assumptions

- The cash flows are those of the cash-flow ladder (see its limits); interest uses a full period for the first period.
- With a behaviour profile the cash flows depend on the scenario (CPR and TDRR multipliers); EVE change is then PV of the scenario's flows at shocked rates less the base PV of the base flows.
- The curve is flat beyond its first and last points; its rates are linearly interpolated at the bucket midpoint, not at each cash flow.
- ΔNII looks one year ahead with the Phase 4 rules; it does not model new business. Different currencies are added in BDT-equivalent with no conversion.
- Matured assets are not in the ladder, so they are not in EVE. The 15% test and the shock sizes come only from the rule file.
