# IRRBB cash-flow ladder

## 1. What it does

Turns each loan and deposit of the book into dated cash flows from the cutoff date and puts them into the 19 time buckets of the Bangladesh Bank IRRBB guideline (BRPD *Guidelines on Interest Rate Risk in the Banking Book*, 13 April 2021, Table 3). It shows assets, liabilities, the net and the cumulative net for each bucket and each currency. It is the cash-flow base of the IRRBB (six scenarios) tool and of the repricing view of the banking book.

## 2. When to use it

- Monthly or quarterly IRRBB work for ALCO and the Bangladesh Bank IRRBB return: the repricing profile of the banking book.
- Checking a book's cash flows before the six-scenario ΔEVE and ΔNII run.
- Seeing the effect of behavioural assumptions (core part of current and savings accounts, loan prepayment, early withdrawal of term deposits) with a behaviour profile.

## 3. Before you start

- Open the asset and liability extracts as tabs. Each needs the outstanding principal as a number, the maturity date as a Date and (when interest counts) the annual rate in percent. Apply filters first, for example one currency or one product group.
- Decide the cutoff date and whether interest counts: "Principal + interest" (default) or "Principal only".
- Optional: a behaviour profile saved by Banking ▸ Treasury & ALM ▸ "Behaviour profile" (file `portable_data/behaviour/<YYYY-MM-DD>.json`).

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Side | Yes | Text, or the tab's role | Asset, Liability, A, L, DEPOSIT | Choose "Counts as" Asset / Liability for the whole tab, or "Side column (A / L)" and pick the column. |
| Outstanding amount | Yes | Number, not negative | 1,000,000.00 | The side decides the sign. A negative amount stops the run. |
| Maturity date | Yes | Date | 2031-04-30 | May be blank only for a non-maturity deposit that the behaviour profile covers. |
| Rate (%) | With interest | Number | 9.5 | Not needed in "Principal only". |
| Rate type | No | Text: Fixed, Floating | Floating | "If no column" gives the default. |
| Next repricing date | Floating rows | Date | 2026-11-30 | The earlier of this and the maturity is used. |
| Repayment type | No | Text: Bullet, Equal instalment, Equal principal | Equal instalment | Default Bullet. |
| Frequency | No | Text: Monthly, Quarterly, Half-yearly, Annual, At maturity | Monthly | Default At maturity. |
| Product code | With a profile | Text | SAV01 | The behaviour profile is read against it. |
| Currency | No | Text | BDT, USD | Default BDT. Amounts are taken as BDT or BDT-equivalent: there is no currency conversion. |

## 5. Step by step

1. Open the tabs. Choose Banking ▸ Treasury & ALM ▸ "IRRBB cash-flow ladder…" (or Ctrl+K and type "irrbb").
2. In the top rows type the "Cutoff date (YYYY-MM-DD)" and choose the "Cash-flow mode", the "Scenario" (Base unless you want the behavioural effect of a shock) and the "Behaviour profile" ("(none)" for contractual cash flows only).
3. In the card "Tab 1" choose the "Tab", what it "Counts as", and its columns: "Outstanding amount", "Maturity date" and the optional roles. A column set to "(none)" uses the value in "If no column".
4. Click "Add tab" for each further tab and fill its card.
5. Read the red line at the top if the button is grey; it names what is missing.
6. Click "Build cash-flow ladder". The result opens in the tab "IRRBB cash-flow ladder".

## 6. What you get

One row per bucket and currency, BDT first. `Midpoint (years)` is the bucket's midpoint from BB Table 3. `Assets` are inflows (positive) and `Liabilities` outflows (negative), in BDT; `Net` is their sum and `Cumulative net` the running sum within the currency. After each currency's buckets a row "Matured assets, not in ladder (N)" gives the number and total principal of assets whose maturity has passed. Buckets: O/N (up to 1 day), then up to 1M, 3M, 6M, 9M, 1Y, 1.5Y, 2Y, 3Y, 4Y, 5Y, 6Y, 7Y, 8Y, 9Y, 10Y, 15Y, 20Y and over 20Y; each includes its upper bound. Time is days from the cutoff divided by 365.

How the flows are made: payment dates run back from the maturity by the frequency and only those after the cutoff count. Interest of a period is the outstanding × rate ÷ payments per year (a full period, also for the first). An equal instalment loan is an annuity on the remaining payments. A floating row has one flow on its next repricing date: all its principal plus one period's interest. A matured liability goes to O/N.

## 7. Check your result

- The sum of `Assets` (principal only) plus the excluded row equals the asset book's outstanding total; the sum of `Liabilities` equals the liabilities' total, negative.
- With "Principal + interest" the totals are higher by the interest still to come.
- The excluded row should be small; a large figure means matured deals still on the books (see the Hygiene audit).
- The first bucket's liabilities are overnight money, matured deposits and, with a profile, the non-core part of current and savings accounts.

## 8. Worked example

Cutoff 2026-10-31, "Principal only", the side column "Side":

| Account_No | Side | Outstanding | Rate | Matdat |
| --- | --- | ---: | ---: | --- |
| A1 | Asset | 1,000,000.00 | 9.00 | 2031-04-30 |
| A2 | Asset | 250,000.00 | 9.00 | 2026-10-31 |
| L1 | Liability | 300,000.00 | 5.00 | 2026-09-30 |
| L2 | Liability | 600,000.00 | 6.00 | 2027-10-31 |

Test: `guide_irrbb_cashflow_ladder`

A1 falls in the 4–5Y bucket (midpoint 4.5), L1 has matured so it is in O/N, L2 is due in exactly 365 days so it is in 9M–1Y, and A2 is a matured asset, left out of the buckets. Only the rows with an amount are shown here; the tab has all 19 buckets.

| Currency | Bucket | Midpoint (years) | Assets | Liabilities | Net | Cumulative net |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| BDT | O/N | 0.0028 | 0.00 | -300,000.00 | -300,000.00 | -300,000.00 |
| BDT | 9M–1Y | 0.875 | 0.00 | -600,000.00 | -600,000.00 | -900,000.00 |
| BDT | 4–5Y | 4.5 | 1,000,000.00 | 0.00 | 1,000,000.00 | 100,000.00 |
| BDT | Matured assets, not in ladder (1) | (blank) | 250,000.00 | (blank) | (blank) | (blank) |

Test: `guide_irrbb_cashflow_ladder`

An equal-instalment loan of 120,000 at 12%, paid monthly, 12 payments left ("Principal + interest"): every payment is 10,661.85, the first period's interest is 1,200.00 and the principal parts add up to 120,000.00. The first three payments:

| Payment date | Principal | Interest | Payment |
| --- | ---: | ---: | ---: |
| 2026-11-30 | 9,461.85 | 1,200.00 | 10,661.85 |
| 2026-12-31 | 9,556.47 | 1,105.38 | 10,661.85 |
| 2027-01-31 | 9,652.04 | 1,009.82 | 10,661.85 |

Test: `guide_irrbb_cashflow_ladder`

## 9. Common messages and fixes

- "Cash flows: the maturity date is blank on N row(s); the first 5 values of Account_No: …" Fill the maturity dates, or cover the product with a behaviour profile if it is a non-maturity deposit. The message names the account column it took the numbers from, or lists rows ("row 12").
- "… the rate is blank or not a number …" Fill the rate, or switch to "Principal only".
- "… a floating row has no next repricing date …" Fill the repricing date, or set the rate type to Fixed.
- "… the frequency 'X' is not Monthly, Quarterly, Half-yearly, Annual or At maturity" (also for the repayment type and the rate type). Correct the values or use a lookup first.
- "… the outstanding amount in 'X' is negative (use positive amounts; the side decides the sign)". Use positive balances.
- "Cash flows: choose the rate column, or switch the cash-flow mode to Principal only." and "choose the product column, which the behaviour profile is read against."
- "Choose the … column." / "Type the cutoff date as YYYY-MM-DD" in red at the top of the dialog: fix it and the button turns on.
- "Column 'X' is Y, not a date. Set it to a Date type in the Schema & Ingestion settings."

## 10. Limits and assumptions

- A full period of interest is used for the first period too (no accrued-interest split), as the specification says.
- A floating row is not amortised before its repricing date: all remaining principal is on that date. A loan prepayment rate (CPR) is not applied to floating rows or to "At maturity" rows.
- "At maturity" rows pay simple interest on the principal for the time left (days ÷ 365).
- Non-maturity deposits use principal only, with no interest flows.
- No currency conversion: amounts are BDT or BDT-equivalent. Each currency has its own ladder.
- Matured assets are left out of the buckets (as in the liquidity gap ladder); no collateral, option or off-balance-sheet flows are modelled.
