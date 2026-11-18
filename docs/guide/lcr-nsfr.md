# LCR / NSFR from GL balances

## 1. What it does

Works out the Liquidity Coverage Ratio (LCR: high-quality liquid assets over net cash outflows for 30 days) and the Net Stable Funding Ratio (NSFR: available over required stable funding) from the GL balances of a trial balance. Each GL code is mapped by a file you control to a line of the ratio (a Level 1 asset, an outflow, a funding source and so on) with its factor. It follows the structure of Bangladesh Bank's DOS Circular No. 01 of 1 January 2015, as amended by SDAD Circular Letter No. 11 of 22 July 2026 (NSFR reporting changes from 30 June 2026). **The map that ships is a sample with Basel factors; it is not Bangladesh Bank's.** The owner must replace its factors and GL codes before the ratios go into a return.

## 2. When to use it

- Monthly or quarterly LCR and NSFR monitoring and the returns that follow from them.
- Testing how much a deposit run-off assumption or a change in the securities book moves the ratios (edit the map's factors on a copy).
- Checking that every GL code of the trial balance is covered by the map.

## 3. Before you start

- A tab with the GL trial balance: the GL code, the balance, and for NSFR lines that depend on maturity the residual maturity (a number, in months, days or years).
- A liquidity map in `portable_data/liquidity_maps/<name>.json`: `lines` (code, label, ratio LCR or NSFR, kind, `factor_pct` or `factor_by_maturity` with `lt_6m`, `6m_1y`, `ge_1y`, and the normal side asset or liability), `gl_map` (GL codes by `exact`, `prefix` or `range` to a line), `caps` (Level 2 40%, Level 2B 15%, inflows 75%, the two minimums and their test) and its `source` and `as_of`. The sample `sample_basel.json` is written there the first time if missing; an existing file is never overwritten. Copy it to a new name and edit it.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| GL code | Yes | Text | 101000 | Compared as text (a range of digits compares as numbers). |
| Balance | Yes | Number (BDT) | -10,000 | Blank or text stops the run. See "Balance sign". |
| Residual maturity | Only for lines with factors by maturity | Number | 24 | In the unit you choose (Months, Days or Years). Under 6 months, 6 months to under a year, a year or more. |

## 5. Step by step

1. Open the GL tab. Choose Banking ▸ Treasury & ALM ▸ "LCR / NSFR from GL balances…" (Ctrl+K: "lcr").
2. Choose the "Liquidity map". A yellow "Sample parameters" line says while the map is a sample.
3. Choose the "GL code column" and the "Balance column".
4. Choose the "Balance sign": "Debit positive, credit negative" (liabilities are negative in the trial balance; the tool turns them) or "Positive on the normal side".
5. Choose the "Residual maturity column (NSFR)" and its unit if the map has lines by maturity; otherwise "(none)".
6. Click "Calculate LCR and NSFR". If a GL code is not in the map, the run stops and says how many and the first 10; click "Export unmapped" to save all of them (GL code, balance) as a CSV, add rules for them to the map file and run again.
7. Two tabs open: "LCR/NSFR lines" and "LCR/NSFR summary".

## 6. What you get

"LCR/NSFR lines": for each line of the map (and each maturity bucket of a line by maturity that holds something) `Ratio`, `Line`, `Label`, `Kind`, `Residual maturity`, `Amount` (the balances turned to the line's normal side, BDT), `Factor %` and `Weighted amount` (amount × factor). "LCR/NSFR summary": the Level 1, 2A and 2B assets after their factors, the two cap adjustments, `HQLA after caps`, the weighted outflows and inflows, the inflows counted (at most the inflow cap of the outflows), `Net cash outflows`, `LCR`, the minimum and a status ("Meets the minimum", "Below the minimum" or "Not defined" when the denominator is zero), then the same for the NSFR, the map file with its source and, for a sample, a "Sample parameters" row and any warnings.

HQLA caps (Basel): adjustment for the Level 2B cap = the larger of (Level 2B − 15/85 × (Level 1 + Level 2A)) and (Level 2B − 15/60 × Level 1), not below 0; adjustment for the Level 2 cap = (Level 2A + Level 2B − that adjustment − 2/3 × Level 1), not below 0; HQLA = Level 1 + Level 2A + Level 2B − both adjustments. The 15, 40 and the fractions are worked out from the caps of the map. The NSFR must be greater than its minimum; the LCR must be at least its minimum (the map says which).

## 7. Check your result

- Every GL code of the trial balance is mapped (there is no "unmapped" stop), so the sums of the lines are the balances of the trial balance for the mapped GLs.
- The "Amount" of a line equals its GL balances: compare with the trial balance (Pivot on the GL code).
- HQLA, net cash outflows, ASF and RSF against the return you are preparing; the ratio flags against the minimums.
- A warning "negative total" usually means the wrong "Balance sign".

## 8. Worked example

A small trial balance, with the shipped sample map (illustrative GL codes), "Debit positive, credit negative" and the residual maturity in months.

| GL_Code | Balance | Residual_Months |
| --- | ---: | ---: |
| 101000 | 1,000.00 | (blank) |
| 103000 | 400.00 | (blank) |
| 104000 | 100.00 | (blank) |
| 201000 | -10,000.00 | (blank) |
| 202000 | -10,000.00 | (blank) |
| 203000 | -2,500.00 | (blank) |
| 115000 | 3,600.00 | 24 |
| 300100 | -500.00 | (blank) |

Test: `guide_lcr_nsfr`

The lines that hold something. The loan of 3,600 is both a LCR inflow (50%) and an NSFR required-funding line by maturity (a year or more: 85%).

| Ratio | Line | Residual maturity | Amount | Factor % | Weighted amount |
| --- | --- | --- | ---: | ---: | ---: |
| LCR | L1_CASH | (blank) | 1,000.00 | 100.00 | 1,000.00 |
| LCR | L2A_SEC | (blank) | 400.00 | 85.00 | 340.00 |
| LCR | L2B_SEC | (blank) | 100.00 | 50.00 | 50.00 |
| LCR | OUT_RETAIL_STABLE | (blank) | 10,000.00 | 5.00 | 500.00 |
| LCR | OUT_RETAIL_LESS_STABLE | (blank) | 10,000.00 | 10.00 | 1,000.00 |
| LCR | OUT_WHOLESALE | (blank) | 2,500.00 | 40.00 | 1,000.00 |
| LCR | IN_LOANS | (blank) | 3,600.00 | 50.00 | 1,800.00 |
| NSFR | ASF_CAPITAL | (blank) | 500.00 | 100.00 | 500.00 |
| NSFR | ASF_STABLE_DEP | (blank) | 10,000.00 | 95.00 | 9,500.00 |
| NSFR | ASF_LESS_STABLE_DEP | (blank) | 10,000.00 | 90.00 | 9,000.00 |
| NSFR | ASF_WHOLESALE | (blank) | 2,500.00 | 50.00 | 1,250.00 |
| NSFR | RSF_CASH | (blank) | 1,000.00 | 0.00 | 0.00 |
| NSFR | RSF_LOANS | >=1 year | 3,600.00 | 85.00 | 3,060.00 |

Test: `guide_lcr_nsfr`

The summary. Outflows are 500 + 1,000 + 1,000 = 2,500; the inflows of 1,800 are under the 75% cap (1,875), so net cash outflows are 700 and the LCR is 1,390 ÷ 700 = 198.57%. ASF is 500 + 9,500 + 9,000 + 1,250 = 20,250 and RSF is 3,060, so the NSFR is 661.76%.

| Item | Value | Unit |
| --- | ---: | --- |
| Level 1 assets (after factors) | 1,000.00 | BDT |
| Level 2A assets (after factors) | 340.00 | BDT |
| Level 2B assets (after factors) | 50.00 | BDT |
| Adjustment for the Level 2B cap | 0.00 | BDT |
| Adjustment for the Level 2 cap | 0.00 | BDT |
| HQLA after caps | 1,390.00 | BDT |
| Cash outflows (weighted) | 2,500.00 | BDT |
| Cash inflows (weighted) | 1,800.00 | BDT |
| Inflows counted | 1,800.00 | BDT |
| Net cash outflows | 700.00 | BDT |
| LCR | 198.57 | % |
| LCR minimum | 100.00 | % |
| Available stable funding | 20,250.00 | BDT |
| Required stable funding | 3,060.00 | BDT |
| NSFR | 661.76 | % |
| NSFR minimum | 100.00 | % |

Test: `guide_lcr_nsfr`

## 9. Common messages and fixes

- "LCR / NSFR: N GL code(s) are not in the liquidity map …; the first 10: …" Add rules for them to the map (use "Export unmapped"), or fix the codes.
- "… the GL code 'X' matches two LCR lines (or NSFR lines) in the map …" Make the rules of the map not overlap (one line of each ratio per GL).
- "… the line 'X' needs the residual maturity and it is not chosen / blank on N row(s) …" Choose the residual maturity column, or fill the blanks.
- "… the balance in 'X' is blank or not a number on N row(s); the first 5 GL codes: …" Fix the balances.
- "The line 'X' (…) has a negative total … check the sign of the GL balances" (a warning in the summary): the "Balance sign" is probably wrong, or a GL is overdrawn.
- "The liquidity map … is not valid / names the kind … / needs exactly one of factor_pct or factor_by_maturity …" Fix the map file; the message names the line.
- "Choose the …" in red: fill the missing choice.

## 10. Limits and assumptions

- The shipped map is a sample with Basel factors and illustrative six-digit GL codes. Never present its ratios as Bangladesh Bank's.
- A GL code can be mapped to one LCR line and one NSFR line, not to two of the same ratio.
- Balances are BDT or BDT-equivalent: there is no currency conversion and no split by currency.
- Residual maturity is per GL row, so a GL with several maturities needs a row for each (the trial balance by maturity).
- Contractual cash flows over 30 days are not generated: the outflow and inflow factors are applied to the balances.
- Alternative liquidity approaches, operational requirements and the Bangladesh Bank reporting forms are not covered.
