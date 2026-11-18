# Liquidity gap ladder

## 1. What it does

Lays out the bank's inflows (assets) and outflows (liabilities) by maturity bucket and shows, for each bucket, the net gap, the cumulative gap and both as a percentage of total assets. With a limits file for the bucket preset it also shows the minimum cumulative gap allowed and flags a BREACH where the cumulative gap % is below it. It is the structural liquidity profile (SLP) table of the ALCO and Bangladesh Bank liquidity returns, built from the same buckets as the Maturity bucketer.

## 2. When to use it

- Monthly SLP / maturity-mismatch table for ALCO.
- Checking cumulative gap limits by bucket.
- Seeing the effect of a different bucket preset or cutoff date.

## 3. Before you start

- Open the asset extract(s) (loans, investments, placements) and the liability extract(s) (deposits, borrowings), each with a maturity date typed as a Date and an amount typed as a number. Apply the filters you need first (for example only BDT).
- Choose the cutoff date (the reporting date) and a bucket preset (built in, or one you made in the Maturity bucketer).
- Optional limits: `portable_data/gap_limits/<preset name>.json` with the minimum cumulative gap % for each bucket (see the sample in `docs/samples/gap_limits`). The numbers must come from your ALCO policy; none are built in.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Tab role | Yes | Choice per tab | Asset / inflow, Liability / outflow, or a side column | A tab can hold both: choose Side column and pick the column that says A / ASSET or L / LIABILITY / DEPOSIT / BORROWING. |
| Amount | Yes | Number (BDT) | 1,000,000 | Blank amounts count as 0. A negative amount reduces the bucket. |
| Maturity date | Yes | Date | 2026-11-15; blank for current and savings | A blank maturity goes to the preset's sight bucket (or its long bucket if you untick the box). A maturity before the cutoff is overdue. |
| Bucket preset, cutoff date | Yes | Chosen and typed in the dialog | Bangladesh Bank SLP (9 Liquidity Buckets), 2026-10-31 | Cutoff as YYYY-MM-DD. |

## 5. Step by step

1. Open the asset and liability tabs.
2. Choose Banking ▾ → "Liquidity gap ladder…" (under "Treasury & ALM"; or Ctrl+K and type "gap").
3. In "Liquidity gap ladder" set the first line: the "Tab", "Counts as" (Asset / inflow, Liability / outflow, or Side column (A / L)), the "Side column" if you chose that, the "Amount" and the "Maturity date". Click "Add tab" for each further tab and set its line the same way.
4. Choose "Bucket preset:" and type "Cutoff date (YYYY-MM-DD):".
5. Leave "Treat a blank maturity as sight" ticked for current and savings accounts, or untick it to put them in the preset's long bucket.
6. Read the note under the settings: it says whether a limits file was found for the preset.
7. Click "Build gap ladder". The ladder opens in a new tab "Gap ladder (preset)". Limits are read when the tab is built: build again after you change the file.

## 6. What you get

One row per bucket of the preset, in its order (sight, the day limits, over the last limit, overdue). `Inflows` is the sum of asset amounts and `Outflows` the sum of liability amounts in the bucket; `Net gap` = inflows − outflows; `Cumulative gap` is the running sum of the net gap; `Net gap % of total assets` and `Cumulative gap % of total assets` divide by total assets (all inflows added up) and show percent. A last row "Total" adds up Inflows, Outflows and Net gap over all buckets; its cumulative gap and percentage cells (and the limit and Breach cells) are blank. If total assets are 0 the percentages are blank. With a limits file there are two more columns: `Cumulative gap limit %` (the minimum, blank for a bucket with no limit) and `Breach` ("BREACH" when the cumulative gap % is below the limit; the cells are tinted in the danger colour). A cumulative gap equal to the limit is not a breach.

## 7. Check your result

- The sum of Inflows equals the asset balances you meant to include, and the sum of Outflows the liabilities (add the column in a Pivot or read the control totals of the tabs).
- The last Cumulative gap equals total inflows − total outflows.
- Overdue should be small: a large figure means matured deals still on the books (run the Hygiene audit).
- Compare the cumulative gap % of each bucket with the figure of the previous month's pack.

## 8. Worked example

Assets (inflows): 100 matures in the first month, 200 in the second and third month.

| Balance | Matdat |
| ---: | --- |
| 100.00 | 2026-11-15 |
| 200.00 | 2026-12-31 |

Test: `guide_gap_ladder`

Liabilities (outflows): 150 and 50 in the same buckets.

| Balance | Matdat |
| ---: | --- |
| 150.00 | 2026-11-20 |
| 50.00 | 2026-12-15 |

Test: `guide_gap_ladder`

The ladder with the preset "Test ladder" (0–1 month, 1–3 months, over 3 months, overdue), cutoff 2026-10-31, and a limit of −10% on the first bucket. Net gap is −50 and +150; cumulative −50 and +100; total assets are 300, so −16.67% and +50.00% net, −16.67% and +33.33% cumulative. The first bucket is below its −10% limit: BREACH.

| Bucket | Inflows | Outflows | Net gap | Cumulative gap | Net gap % of total assets | Cumulative gap % of total assets | Cumulative gap limit % | Breach |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 0–1 month | 100.00 | 150.00 | -50.00 | -50.00 | -16.67 | -16.67 | -10.00 | BREACH |
| 1–3 months | 200.00 | 50.00 | 150.00 | 100.00 | 50.00 | 33.33 | (blank) | (blank) |
| Over 3 months | 0.00 | 0.00 | 0.00 | 100.00 | 0.00 | 33.33 | (blank) | (blank) |
| Overdue | 0.00 | 0.00 | 0.00 | 100.00 | 0.00 | 33.33 | (blank) | (blank) |
| Total | 300.00 | 200.00 | 100.00 | (blank) | (blank) | (blank) | (blank) | (blank) |

Test: `guide_gap_ladder`

## 9. Common messages and fixes

- "Gap ladder: add at least one tab (assets or liabilities)." Add a line for a tab.
- "Type the cutoff date as YYYY-MM-DD (for example 2026-10-31)." in the dialog; "Cutoff Date must be in YYYY-MM-DD format" from the bucketer.
- "Gap ladder: the amount column 'X' is String, not a number. Set it to a number in the schema first."
- "Column 'X' is String, not a date. Set it to a Date type in the Schema & Ingestion settings." for the maturity column.
- "Side column 'X' has values that are neither asset nor liability: … Use A/ASSET for assets and L/LIABILITY/DEPOSIT/BORROWING for liabilities."
- "The gap limits file … has a limit for 'X', which is not a bucket of the preset 'Y'." / "has two limits for 'X'." / "is not valid". Fix the file; bucket names must be written exactly as the bucketer writes them.
- "Gap ladder: total assets are 0, so the limits (percent of total assets) cannot be checked. Add an asset tab with amounts."
- "Line N: that tab no longer exists." Choose the tab again.

## 10. Limits and assumptions

- Behavioural treatment of current and savings accounts (core and volatile parts) is planned for Phase 5. Until then a deposit with no maturity date is placed wholly in the sight bucket (or the long bucket), which overstates the early outflows.
- Contractual maturity only; no prepayment, rollover or off-balance-sheet items unless you add them as rows.
- One currency at a time: filter the tabs first.
- The limits are minimum cumulative gaps as a percentage of total assets; other limit types are not checked.
