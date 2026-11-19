# Mudaraba profit distribution

## 1. What it does

Shares the investment income of a period among the Mudaraba deposit products, using the weightage method: the pool income is split between the depositors and the bank as Mudarib by the profit-sharing ratio (PSR), and the depositors' share is divided between the products in proportion to average balance × weightage. For each product it shows the profit and the indicative rate per year.

**It only calculates.** The method, the weightages, the CRR and the PSR must be the ones approved by the bank's Shariah Supervisory Committee (SSC). Every one of them is read from a file, `portable_data/islamic/psr_<YYYY-MM-DD>.json`; none is fixed in the program and none is typed in the dialog. The file that ships with the app is a **sample** (it says so, and a banner shows while it is in use): replace it with the bank's own before any figure is used.

## 2. When to use it

- The monthly or quarterly profit distribution to Mudaraba depositors (savings, notice deposits, term deposits).
- To check the indicative rates by product before they are announced.
- To redo a past period with the parameter file that applied then.

## 3. Before you start

- Have the deposits of the period in a tab: a product code and the **average balance** of the period for each row (or the daily balances).
- Know the investment income of the period (the amount to distribute) and the other investable funds (the bank's own and other non-Mudaraba funds in the pool).
- Check `portable_data/islamic/` has a parameter file dated on or before the end of the period. The app uses the latest one dated on or before the period's end date.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Product code | Yes | Text | MSND, MSB, MTDR-1Y | Every code must have a weightage in the file; a missing one stops the run and is named. Codes match ignoring case and extra spaces. |
| Balance | Yes | Number, 0 or more | 200 | The average of the period; or, with "a daily balance", one row per day and account: the average is their sum ÷ the days. |

The file `psr_<date>.json` holds `crr_pct`, `psr_pct` and the list of weightages (`product`, `weight`), with `as_of`, `source` and `sample`.

## 5. Step by step

1. Open the deposit tab. Choose Banking ▸ Islamic window ▸ "Mudaraba profit distribution…" (Ctrl+K: "Mudaraba").
2. Choose "Product code" and "Balance", and say whether the balance is the average of the period or a daily balance.
3. Type "Period start" and "Period end" (YYYY-MM-DD). The days are end − start + 1.
4. Type "Investment income of the period" and "Other investable funds".
5. Read the Parameters box: the file in force, its CRR, PSR and weightages. If it is a sample, the banner says so.
6. Choose "Build distribution".

## 6. What you get

A tab with a row for each product (Average balance, Weightage, Weighted balance, Profit, Indicative rate % p.a.), a "Total" row, and a footer: Investable Mudaraba funds, Other investable funds, Pool share (%), Investment income, Pool income, Depositors' share, Mudarib share, the Period, the Parameter file used (with its date and source) and, for a sample, a "Sample parameters" line.

## 7. Check your result

- The product profits add up to the Depositors' share; the Depositors' share and the Mudarib share add up to the Pool income.
- Pool share = investable ÷ (investable + other investable funds).
- The rate of a product = profit ÷ average balance × 365 ÷ days × 100.

## 8. Worked example

Three products over 30 days, with the sample parameters (CRR 4%, PSR 70%, weightages 0.4, 0.6 and 1.0); income 12.00, other investable funds 240.00.

| Product | Avg_Balance |
| --- | ---: |
| MSND | 200.00 |
| MSB | 300.00 |
| MTDR-1Y | 500.00 |

Test: `guide_mudaraba_profit_distribution`

The investable funds are 960.00, the pool share 80.00%, the pool income 9.60, the depositors' share 6.72 and the Mudarib's 2.88. The products:

| Product | Weighted balance | Profit | Indicative rate % p.a. |
| --- | ---: | ---: | ---: |
| MSND | 80.00 | 0.707368 | 4.3032 |
| MSB | 180.00 | 1.591579 | 6.4547 |
| MTDR-1Y | 500.00 | 4.421053 | 10.7579 |

Test: `guide_mudaraba_profit_distribution`

## 9. Common messages and fixes

- "Mudaraba: no weightage for the product(s) X (balance …). Add them to islamic/psr_….json." The SSC-approved weightage of the product must be added to the file; nothing is guessed.
- "There is no PSR file dated on or before …" Add a `psr_<date>.json` dated on or before the period's end.
- "The parameter file …: psr_pct must be from 0 to 100." The file has a wrong number; fix it.
- "Mudaraba: N row(s) are negative." A deposit balance cannot be negative: check the file.
- "Type the end date as YYYY-MM-DD."

## 10. Limits and assumptions

- The calculation is the one in section 1; a bank whose approved method differs (for example weightages by deposit tenor within a product, or a profit equalisation reserve) must not use this tool as it stands.
- 365 days a year is used for the indicative rate.
- With "a daily balance", each row is one account's balance for one day, and a missing day counts as 0 for that account.
- The CRR is a deduction for the reserve requirement from the Mudaraba deposits; the file holds the number the bank applies.
- The sample file is for trying the tool: its numbers are not any bank's.
