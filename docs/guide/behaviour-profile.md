# Behaviour profile (core and volatile deposits)

## 1. What it does

Works out how much of each current and savings (non-maturity) deposit product is stable "core" money, and at what average maturity, from the product's own balance history, within the caps of the Bangladesh Bank IRRBB guideline (BRPD *Guidelines on Interest Rate Risk in the Banking Book*, 13 April 2021, §15.13 and Table 4). It also holds the prepayment rate (CPR) of loan products and the early-redemption rate (TDRR) of term-deposit products, which you type. The profile is saved as a dated file that the liquidity gap ladder and the IRRBB tools read.

## 2. When to use it

- Once a year (or when ALCO reviews its behavioural assumptions) to refresh the core share of CASA and savings products.
- Before the IRRBB cash-flow ladder, the six-scenario IRRBB run or a gap ladder that should treat current and savings accounts by behaviour instead of "sight".
- To record CPR and TDRR assumptions with a date and a source.

## 3. Before you start

- Open a tab with the balance history: one row per date and product (or per account: balances of the same product and date are added up). Monthly or daily dates are fine; daily history is reduced to the last date of each month.
- The date column must be typed as a Date and the balance as a number. Every product needs a balance in every month of the look-back.
- The caps come from the rule file `portable_data/rules/irrbb_nmd_caps/<date>.json` (the 2021 guideline's Table 4 is built in: the program writes the file to that folder the first time if it is missing, and never overwrites a file that is there). A later dated file takes over from its date.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Date | Yes | Date | 2026-10-31 | Blank dates stop the read. |
| Product code | Yes | Text | SAV, CA01 | Matched ignoring case and extra spaces. |
| Balance | Yes | Number (BDT) | 1,250,000,000 | Not negative. Several rows of a product and date are summed. |

## 5. Step by step

1. Open the history tab. Choose Banking ▸ Treasury & ALM ▸ "Behaviour profile (core and volatile deposits)…" (or Ctrl+K and type "behaviour").
2. Choose the "History tab" and its "Date", "Product" and "Balance" columns, then click "Read history". One line appears for each product.
3. Set "Look-back (months)" (120 is the guideline's 10 years), "z" (2.33) and the "Profile date (YYYY-MM-DD)" (the file's name; it starts as the last month-end). Type the "Source" of the study.
4. For each product choose a "Category" (Retail transactional, Retail non-transactional, Wholesale), type its "Average maturity (years)" and choose the "Method": Historical minimum, Volatility, or Typed (then type the "Typed core %").
5. Type "CPR %" for loans and "TDRR %" for term deposits (blank = 0). A product that has neither a history nor a category only carries its CPR and TDRR: use "Add product" for it.
6. Read the "Core %" and "Months" columns and the messages under the table. Click "Save profile" ("Replace profile" when that date's file exists).
7. In the liquidity gap ladder tick "Apply behaviour profile", choose the file and each line's product column. The IRRBB tools have a "Behaviour profile" choice too.

## 6. What you get

The file `portable_data/behaviour/<date>.json` with `as_of`, `source` and one entry per product: `product`, `category`, `core_pct` (percent, after the cap), `avg_maturity_years`, `method`, `lookback_months` (months really used; 0 for a typed core), `capped` (true when the computed core was above the cap and was cut), `cpr_pct`, `tdrr_pct`. Historical minimum: core % = lowest month-end balance in the look-back ÷ latest balance × 100. Volatility: core % = (1 − z × σ) × 100, not below 0, with σ the sample standard deviation of the month-on-month changes (as fractions). Caps: core 90% and 5 years for Retail transactional, 70% and 4.5 years for Retail non-transactional, 50% and 4 years for Wholesale.

In the gap ladder the profile splits a deposit: the volatile part (100% − core) goes in the first bucket and the core part in the bucket that holds the average maturity; the deposit's maturity date is not used. Without the profile the result is the same as before.

## 7. Check your result

- The core share should be well below 100% for transactional products and not jump from one year to the next without a reason; compare with the last profile.
- "Months" should be 120 for a full guideline profile; the shorter it is, the less the number can be relied on.
- In the gap ladder the outflows' total is unchanged by the profile; only the buckets move.
- Cross-check the latest balance with the general ledger for the product.

## 8. Worked example

One year of month-end balances of the savings product SAV (a synthetic series):

| Month-end | Product | Balance |
| --- | --- | ---: |
| 2025-11-30 | SAV | 100.00 |
| 2025-12-31 | SAV | 95.00 |
| 2026-01-31 | SAV | 110.00 |
| 2026-02-28 | SAV | 90.00 |
| 2026-03-31 | SAV | 105.00 |
| 2026-04-30 | SAV | 120.00 |
| 2026-05-31 | SAV | 98.00 |
| 2026-06-30 | SAV | 102.00 |
| 2026-07-31 | SAV | 96.00 |
| 2026-08-31 | SAV | 104.00 |
| 2026-09-30 | SAV | 108.00 |
| 2026-10-31 | SAV | 100.00 |

Test: `guide_behaviour_profile`

The lowest balance is 90 and the latest 100, so the minimum method gives 90.00%: as Retail transactional it is at its cap and stays; as Retail non-transactional it is cut to 70.00% and marked capped. The volatility method (σ = 0.126772, z = 2.33) gives 70.46%. The history is 12 months, so the tool warns: "BB IRRBB guideline §15.13 uses 10 years of volume history; this profile uses 12 months."

| Category | Method | Core % | Capped | Months |
| --- | --- | ---: | --- | ---: |
| Retail transactional | Historical minimum | 90.00 | (blank) | 12 |
| Retail non-transactional | Historical minimum | 70.00 | capped | 12 |
| Retail transactional | Volatility | 70.46 | (blank) | 12 |

Test: `guide_behaviour_profile`

Gap ladder, 1,000,000 of savings with no maturity date, profile core 60% and average maturity 3 years, with "Apply behaviour profile" ticked: 400,000 in the first bucket and 600,000 in the bucket that holds 3 years.

| Bucket | Outflows | Net gap |
| --- | ---: | ---: |
| 0-1 month | 400,000.00 | -400,000.00 |
| 1-12 months | 0.00 | 0.00 |
| 1-5 years | 600,000.00 | -600,000.00 |
| Over 5 years | 0.00 | 0.00 |
| Total | 1,000,000.00 | -1,000,000.00 |
| Overdue assets (not in gap) | (blank) | (blank) |

Test: `guide_behaviour_profile`

Unticked, the Phase 4 result: everything in the sight bucket.

| Bucket | Outflows | Net gap |
| --- | ---: | ---: |
| 0-1 month | 1,000,000.00 | -1,000,000.00 |
| 1-12 months | 0.00 | 0.00 |
| 1-5 years | 0.00 | 0.00 |
| Over 5 years | 0.00 | 0.00 |
| Total | 1,000,000.00 | -1,000,000.00 |
| Overdue assets (not in gap) | (blank) | (blank) |

Test: `guide_behaviour_profile`

## 9. Common messages and fixes

- "'SAV' has no balance for 2026-03: the month-ends must be continuous." Fill the missing month, or shorten the look-back to start after it.
- "'X': an average maturity of 5 years is above the Bangladesh Bank cap of 4.5 years for Retail non-transactional (…)" and "'X': a core share of 80% is above the Bangladesh Bank cap of 70% …". Type the cap or less. The message names the rule file and its source.
- "'X': choose a category (…)" A product with a history needs a category; a product with only CPR or TDRR is added with "Add product" and no category.
- "'X' needs at least 3 month-ends for the volatility method." Use a longer look-back or the historical minimum.
- "History: N row(s) have a blank date, product or balance … the first 5: row 3, …" Fix or filter those rows.
- "'X' has a negative or non-numeric balance on …" and "the latest balance is 0". Balances must be positive.
- "Type the profile date as YYYY-MM-DD" and "Type where the profile comes from". Fill the two boxes.
- A warning, not a block: "BB IRRBB guideline §15.13 uses 10 years of volume history; this profile uses N months."

## 10. Limits and assumptions

- Only the two methods above (and a typed core); no seasonal or rate-sensitivity model of deposit behaviour.
- The core share and average maturity apply to the product as a whole, not to customer segments.
- CPR and TDRR are annual percentages you type; the tool does not estimate them from history. The liquidity gap ladder does not use them (only the core and volatile split); the IRRBB cash-flow ladder and IRRBB (six scenarios) do.
- A deposit's volatile part goes in the first bucket of the ladder, not O/N separately.
- Amounts are BDT or BDT-equivalent; no currency conversion.
