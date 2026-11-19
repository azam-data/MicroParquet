# MoM variance

## 1. What it does

Compares two months by product (and, if you like, segment): the balance of each month, the change and the change in percent, the balance-weighted rate and the annual interest of each month, and the interest change split into three effects: **volume** (the balance moved, at last month's rate), **rate** (the rate moved, on last month's balance) and **mix** (both moved). It also writes the commentary: one fixed-template sentence for each product that moved by a material amount, the biggest first, and a total line. No estimation and no AI: every number in a sentence is a number of the table.

## 2. When to use it

- The monthly ALCO paper: "what moved and why".
- Explaining a change in interest income: how much is more balance, how much is a better or worse rate.
- Spotting new and closed products between two month-ends.

## 3. Before you start

- Open last month's file and this month's file in two tabs, with the same column names (Schema & Ingestion can rename columns).
- The balance and the rate must be numbers. The rate is in percent (9.5 = 9.5%).
- Every row with a balance needs a rate, if you choose a rate column: the interest would be wrong otherwise, so the tool stops and says how many rows.
- Keys must not be blank.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Key | Yes (one or two columns) | Any | Product, Segment | The same columns must be in both months. |
| Balance | Yes | Number | Balance | BDT; the rows of a key are added up. |
| Rate % | No | Number | Rate | Weighted by balance. A key with balances of both signs has no rate (the Phase 2 rule), so no effects. |
| Account count | No | Number | Accounts | Added up per key; shown as Prev count and Curr count. |

## 5. Step by step

1. Banking ▸ Treasury & ALM ▸ "MoM variance and commentary…" (Ctrl+K: "MoM variance"). It needs two open tabs.
2. Choose "Previous-month tab" and "Current-month tab".
3. Choose "Key" (and "Second key (optional)"), "Balance", and "Rate % (optional)".
4. "Material when the balance moves by" takes an amount in the unit you choose (50 Crore is the default) "or by this share of last month" (5% is the default). A product is material when either is reached.
5. Choose "Build MoM variance". Two tabs open: "MoM commentary" and "MoM variance".
6. On the commentary tab, the text box above the table holds the sentences; "Copy" puts them on the clipboard.
7. Both tabs export like any tab, and can be items of a report pack (see [Report packs](report-packs.md)).

## 6. What you get

"MoM variance": a row per key of either month with Status (New, Closed or empty), Prev balance, Curr balance, Change, Change %, Prev rate, Curr rate, Rate change (bp), Prev annual interest, Curr annual interest, Annual interest change, Volume effect, Rate effect and Mix effect. "MoM commentary": one sentence per row, then "Total: …".

## 7. Check your result

- Volume effect + Rate effect + Mix effect equals the Annual interest change of every key that is in both months.
- The Change column adds up to the change of the totals of the two tabs.
- Change % is the change divided by last month's balance.

## 8. Worked example

Last month, in Crore:

| Product | Balance | Rate |
| --- | ---: | ---: |
| TD | 1,000.00 | 8.00 |
| SB | 500.00 | 3.00 |

Test: `guide_mom_variance`

This month:

| Product | Balance | Rate |
| --- | ---: | ---: |
| TD | 1,100.00 | 8.50 |
| SB | 450.00 | 3.00 |

Test: `guide_mom_variance`

The variance, in Crore (TD: interest 80.00 to 93.50, +13.50 = volume 8.00 + rate 5.00 + mix 0.50):

| Product | Change | Change % | Rate change (bp) | Annual interest change | Volume effect | Rate effect | Mix effect |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| SB | -50.00 | -10.00 | 0.00 | -1.50 | -1.50 | 0.00 | 0.00 |
| TD | 100.00 | 10.00 | 50.00 | 13.50 | 8.00 | 5.00 | 0.50 |

Test: `guide_mom_variance`

The commentary with the default thresholds (50 Cr or 5%):

| Commentary |
| --- |
| TD rose by 100.00 Cr (10.00%) to 1,100.00 Cr; rate up 50 bp to 8.50%. |
| SB fell by 50.00 Cr (10.00%) to 450.00 Cr; rate unchanged at 3.00%. |
| Total: rose by 50.00 Cr (3.33%) to 1,550.00 Cr. |

Test: `guide_mom_variance`

## 9. Common messages and fixes

- "MoM variance: the previous month has no key column 'X'." Choose columns both months have, or rename them first.
- "… the balance column 'Balance' of the current month is Text, not a number." Set its type in Schema & Ingestion.
- "N row(s) of the previous month have a blank key." Fill or filter them out.
- "N row(s) of the current month have a balance but no rate." Fill the rate or filter those rows; interest would be wrong otherwise.
- "Choose two different tabs." The previous and current month cannot be the same tab.

## 10. Limits and assumptions

- A key in only one month is marked New or Closed and has no effects (there is no rate to compare). In the commentary it follows the same materiality as every key: a New or Closed key whose balance is at or above the amount limit is named; the ones below it are counted in one line ("Not listed, below the materiality limit: 2 new and 1 closed key(s)."). The variance table lists all of them. The amounts are typed and shown in the dialog's scale (Crore by default).
- Change % uses the size of last month's balance, so a negative balance does not turn the sign round. A key whose last balance was 0 has no Change %.
- The commentary uses the amount format you chose (Crore, Lakh, Million, Thousand or plain). The thresholds are typed in the same unit and are saved with the tabs.
- The effects use the weighted rate of each month; a key with a mix of positive and negative balances has no rate, so its effects are blank.
