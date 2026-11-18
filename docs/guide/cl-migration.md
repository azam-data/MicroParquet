# CL migration matrix

## 1. What it does

Shows how classified-loan grades moved between two month-ends: rows are last month's grade (STD, SMA, SS, DF, BL) and New, columns are this month's grade and Closed/Repaid. Each cell holds the balance (or the number of accounts) of the loans that made that move. It supports the monthly classification review and the NPL roll-forward in the ALCO and credit-risk packs. Upgrades are shaded green and downgrades amber.

## 2. When to use it

- Month-end review of upgrades and downgrades (how much of STD went to SMA, how much of SS went to DF).
- Explaining the change in NPL: what became classified, what recovered, what closed.
- A segment view (for example CMSME against other loans).

## 3. Before you start

- Open last month's loan file and this month's in two tabs, with the CL status, the balance (number) and an account key typed correctly. Apply the same filters to both (for example only advances).
- `cl_code_map.json` in `portable_data` must list every CL code your CBS uses (it is also used by the provisioning tool). A code that is missing stops the matrix.
- Decide whether you want balances or numbers of accounts, and whether to see shares of each grade's row.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Account key | Yes | Text or number, in both tabs | Actnum | A key with several rows in a month is combined: its balances are added and its worst grade counts. |
| Balance | Yes | Number (BDT), in both tabs | 100.00 | A text balance is refused. |
| CL status | Yes | Text, in both tabs | STD, UC, SS, B/L | Mapped to STD, SMA, SS, DF, BL by cl_code_map.json. |
| Segment | No | Text | CMS, Medium | Taken from this month, else last month. A blank segment shows as (blank). |

## 5. Step by step

1. Open both months as tabs (previous month first is convenient).
2. Choose Banking ▾ → "CL migration matrix…" (under the heading "Loans & credit"; or Ctrl+K and type "migration"). It needs two open tabs.
3. In "CL migration matrix" choose "Previous-month tab:" and "Current-month tab:".
4. Choose "Account key:", "Balance:" and "CL status:" (only columns that both tabs have are offered) and, if you want it, "Segment (optional):".
5. Choose "Show:" "Balance (BDT)" or "Number of accounts". Choose "Balance basis:" "Previous month (default)" or "Current month" (see section 6), and tick "As % of the previous grade's row total" for shares.
6. Click "Build migration matrix". The matrix opens in a new tab "CL migration (previous to current)". To see the other measure, build it again.

## 6. What you get

One row per previous grade in the order of `cl_code_map.json`: STD, SMA, SS, DF, BL; then "Previous month total" (the five grades added up), "New" (accounts that exist only this month) and "Total". Columns are this month's grade (STD, SMA, SS, DF, BL), "Closed/Repaid" (accounts that exist only last month) and "Total". Which balance a cell holds depends on "Balance basis:". With "Previous month" (the default) an account that was there last month carries last month's balance, so the Previous month total row adds up to last month's balance. With "Current month" an account that is in both months carries this month's balance, and that row is called "Previous-month accounts (current balance)" because it no longer adds up to last month's total. Either way a new account always carries this month's balance and a Closed/Repaid account always last month's. The number of accounts does not depend on the basis. With a segment there is one block of rows for each segment (in natural order) and a last block "All segments". Shaded cells: green is an upgrade (to an earlier grade in STD, SMA, SS, DF, BL), amber a downgrade; the diagonal, New, Closed/Repaid and the totals are not shaded. With the share option each row's cells are percentages of its Total (2 decimals).

## 7. Check your result

- With the default basis, "Previous month total" in the Total column equals the sum of the balance column of last month's tab (the control total).
- The five grade columns of the "Total" row add up to this month's balance total (new accounts included).
- With counts: "Previous month total" equals the number of accounts in last month's tab (several rows with one key count once).
- Look at the Closed/Repaid column against the repayments in the GL.

## 8. Worked example

Previous month (September): three loans.

| Actnum | Balance | CL_Status |
| --- | ---: | --- |
| A1 | 100.00 | STD |
| A2 | 50.00 | SS |
| A3 | 30.00 | STD |

Test: `guide_cl_migration`

Current month (October): A1 and A2 are still there but worse, A3 is gone, A4 is new.

| Actnum | Balance | CL_Status |
| --- | ---: | --- |
| A1 | 100.00 | SMA |
| A2 | 50.00 | DF |
| A4 | 20.00 | STD |

Test: `guide_cl_migration`

The matrix by balance. A1 moved STD to SMA (100), A2 SS to DF (50), A3 STD closed (30), and A4 is New in STD (20). The Previous month total is 180, the September total.

| Previous grade | STD | SMA | SS | DF | BL | Closed/Repaid | Total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| STD | 0.00 | 100.00 | 0.00 | 0.00 | 0.00 | 30.00 | 130.00 |
| SMA | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| SS | 0.00 | 0.00 | 0.00 | 50.00 | 0.00 | 0.00 | 50.00 |
| DF | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| BL | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| Previous month total | 0.00 | 100.00 | 0.00 | 50.00 | 0.00 | 30.00 | 180.00 |
| New | 20.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 20.00 |
| Total | 20.00 | 100.00 | 0.00 | 50.00 | 0.00 | 30.00 | 200.00 |

Test: `guide_cl_migration`

The same by number of accounts.

| Previous grade | STD | SMA | SS | DF | BL | Closed/Repaid | Total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| STD | 0 | 1 | 0 | 0 | 0 | 1 | 2 |
| SMA | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| SS | 0 | 0 | 0 | 1 | 0 | 0 | 1 |
| DF | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| BL | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Previous month total | 0 | 1 | 0 | 1 | 0 | 1 | 3 |
| New | 1 | 0 | 0 | 0 | 0 | 0 | 1 |
| Total | 1 | 1 | 0 | 1 | 0 | 1 | 4 |

Test: `guide_cl_migration`

## 9. Common messages and fixes

- "CL status column 'CL_Status' has codes not in cl_code_map.json: 'XYZ', … Add each code with its grade (STD, SMA, SS, DF or BL) to that file." Both months are checked and all missing codes are listed together.
- "CL migration: the previous month has no column 'X'. Choose columns that both months have."
- "CL migration: the balance column 'Balance' of the previous month is String, not a number. Set it to a number in the schema first."
- "CL migration: the account key column 'Actnum' has N blank value(s) in the current month. Every row needs an account key."
- "Choose two different tabs: the previous month and the current month." in the dialog.

## 10. Limits and assumptions

- The matrix is made when you open the tab; to change the measure or the share option build it again.
- Excel and PDF exports keep the numbers but not the green and amber shading.
- The grade of an account is its worst-graded row, as in the Period reconcile tool; rows are not weighted by balance.
- It compares two months only; a roll-forward over several months is built from several matrices.
