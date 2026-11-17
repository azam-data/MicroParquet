# Period reconcile (two-period variance and CL migration)

## 1. What it does

Compares two periods of the same book (for example September and October) per account key: new accounts, closed accounts, balance change, change in the balance-weighted rate, and (for loans) the change in classification. It explains the movement of balances between two month-ends for the ALCO pack.

## 2. When to use it

- Month-on-month deposit or loan movement by account.
- NPL and CL migration review for loans.
- Checking that all accounts of last month are explained this month.

## 3. Before you start

- Two tabs open: the previous period and the current period, each with the same key column, balance and (optionally) rate and CL status columns.
- Prepare the tabs first: filter or fix them as you would for reporting.
- For the CL part, `cl_code_map.json` in `portable_data` must list every CL code.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Account key | Yes | Text | A1 | Same column name in both tabs. Several rows per key are combined. |
| Balance | Yes | Number (BDT) | 1,000.00 | Summed per key. |
| Interest rate | No | Number (percent) | 9.5 | Balance-weighted per key; null when the balances mix positive and negative. |
| CL status | No | Text | STD, SS | Mapped through cl_code_map.json; the worst grade of the key's rows is used. |

## 5. Step by step

1. Open both periods as tabs.
2. Choose Banking ▾ → "Two-period variance and CL migration…" (Ctrl+K: "reconcile"). It needs two open tabs.
3. In "Two-Period Variance & Loan CL Migration Reconciler" pick "Previous Period Tab:" and "Current Period Tab:".
4. Pick "Primary Key (e.g. Actnum):" and "Balance Col:".
5. Optionally pick "Optional Rate Col (Int Rate):" and "Optional Loan CL Col (CL_Status):".
6. Click "Reconcile Periods into New Tab".

## 6. What you get

One row per key. `Prev_Balance`, `Curr_Balance`, `Prev_Row_Count`, `Curr_Row_Count`; `Reconcile_Status`: "1. New Account Opened", "2. Closed / Matured Account", "3. Balance Increased", "4. Balance Decreased" (changes under 0.01 count as "5. Unchanged"); `Balance_Delta` (current − previous, a missing side counts as 0). With a rate: `Prev_Rate`, `Curr_Rate`, `Rate_Delta_Bps` (blank when a side has no rate) and `Prev_Rate_Mixed_Sign` / `Curr_Rate_Mixed_Sign`. With CL: `Prev_CL`, `Curr_CL` and `CL_Migration_Matrix` such as "STD -> SMA" ("NEW" and "CLOSED" for missing sides).

## 7. Check your result

- Sum of `Prev_Balance` equals the previous tab's balance total; sum of `Curr_Balance` equals the current tab's total.
- Sum of `Balance_Delta` equals current total − previous total.
- The status bar says how many keys have more than one row in a period (they are combined).

## 8. Worked example

Previous period: A1 appears twice (100 at 5% and 900 at 10%).

| Actnum | Balance | Rate |
| --- | ---: | ---: |
| A1 | 100.00 | 5.00 |
| A1 | 900.00 | 10.00 |
| A2 | 500.00 | 8.00 |

Test: `guide_period_reconcile`

Current period: A1 appears once at 9.5%; A3 is new; A2 is gone.

| Actnum | Balance | Rate |
| --- | ---: | ---: |
| A1 | 1,000.00 | 9.50 |
| A3 | 50.00 | 11.00 |

Test: `guide_period_reconcile`

Output, sorted by key. A1's September rate is the balance-weighted 9.5%, so its rate change is 0 bps; A2 is closed and A3 is new, so their rate changes are blank.

| Actnum | Prev_Balance | Curr_Balance | Balance_Delta | Reconcile_Status | Prev_Rate | Curr_Rate | Rate_Delta_Bps | Prev_Row_Count | Curr_Row_Count |
| --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| A1 | 1,000.00 | 1,000.00 | 0.00 | 5. Unchanged | 9.5 | 9.5 | 0 | 2 | 1 |
| A2 | 500.00 | (blank) | -500.00 | 2. Closed / Matured Account | 8 | (blank) | (blank) | 1 | 0 |
| A3 | (blank) | 50.00 | 50.00 | 1. New Account Opened | (blank) | 11 | (blank) | 0 | 1 |

Test: `guide_period_reconcile`

## 9. Common messages and fixes

- "Reconcile: the selected previous or current tab no longer exists. Pick both tabs again."
- "CL status column 'X' has codes not in cl_code_map.json: …" Add the codes to the file.
- The status bar notes "Keys with more than one row: n in the previous period, m in the current (rows are combined per key…)". Check that the key really is unique; a branch code used as key would combine everything.
- "Rate is null for n key(s)… their balances mix positive and negative": review those keys' balances.

## 10. Limits and assumptions

- The CL output is a text pair per account. A matrix of grade to grade (counts and balances) is planned in Phase 4 task V2.
- Different key names in the two tabs are not supported; rename a column first.
- A key that appears twice is combined; it is not an error.
