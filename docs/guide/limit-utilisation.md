# Limit utilisation, over-limit and undrawn CCF

## 1. What it does

Compares each loan's outstanding balance with its sanctioned limit: utilisation percent, undrawn commitment, over-limit amount, and the credit-equivalent of the undrawn part using a credit conversion factor (CCF). It supports limit monitoring and off-balance-sheet exposure (for example for the LCR and capital adequacy).

## 2. When to use it

- Monthly review of over-limit and near-limit accounts.
- Finding dormant limits with low use.
- Undrawn commitments for liquidity and capital calculations.

## 3. Before you start

- A loan extract with the sanctioned limit and the outstanding balance as numbers (BDT).
- The CCF in percent that your policy applies to undrawn commitments (for example 20).

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Sanctioned limit | Yes | Number (BDT) | 1,000,000 | Blank counts as 0; a limit of 0 gives utilisation 0.00 and any balance over BDT 1 is a breach. |
| Outstanding balance | Yes | Number (BDT) | 500,000 | Blank counts as 0. |
| CCF % | Yes | Number in the dialog | 20 | Applied to the undrawn part only. |

## 5. Step by step

1. Open the tab, then choose Banking ▾ → "Limit utilisation, over-limit and undrawn CCF…" (Ctrl+K: "limit").
2. In "Loan Limit Utilization, Over-Limit Audit & Undrawn CCF" pick "Sanctioned Limit Col:" and "Outstanding Balance Col:".
3. Type "Credit Conversion Factor (CCF % for Undrawn Portion):".
4. Click "Calculate Limit Utilization & Undrawn CCF".

## 6. What you get

`Limit_Utilization_Pct` (balance / limit × 100, 2 decimals), `Undrawn_Commitment_BDT` (limit − balance, never below 0), `Over_Limit_Violation_BDT` (balance − limit, never below 0), `CCF_Credit_Equivalent_BDT` (undrawn × CCF / 100) and `Limit_Audit_Status`: "1. OVER-LIMIT BREACH" (over by more than BDT 1), "2. NEAR FULL UTILIZATION (>95%)", "3. DORMANT / LOW UTILIZATION (<10%)" or "4. NORMAL UTILIZATION". All amounts are rounded to 2 decimals.

## 7. Check your result

- Sum of balances in the result equals the balance total of the tab.
- Sum of the undrawn column plus the sum of the balances below limit equals the sum of limits of those accounts.
- Filter the status column to "1. OVER-LIMIT BREACH" and compare with the credit administration's exception list.

## 8. Worked example

Input: four loans, CCF 20%.

| Actnum | Sanctioned_Limit | Balance |
| --- | ---: | ---: |
| C1 | 1,000,000.00 | 500,000.00 |
| C2 | 500,000.00 | 520,000.00 |
| C3 | 200,000.00 | 196,000.00 |
| C4 | 1,000,000.00 | 50,000.00 |

Test: `guide_limit_utilisation`

Output: C2 is over its limit by 20,000; C3 is at 98%; C4 uses 5% of its limit. The CCF equivalent is 20% of the undrawn amount.

| Actnum | Limit_Utilization_Pct | Undrawn_Commitment_BDT | Over_Limit_Violation_BDT | CCF_Credit_Equivalent_BDT | Limit_Audit_Status |
| --- | ---: | ---: | ---: | ---: | --- |
| C1 | 50.00 | 500,000.00 | 0.00 | 100,000.00 | 4. NORMAL UTILIZATION |
| C2 | 104.00 | 0.00 | 20,000.00 | 0.00 | 1. OVER-LIMIT BREACH |
| C3 | 98.00 | 4,000.00 | 0.00 | 800.00 | 2. NEAR FULL UTILIZATION (>95%) |
| C4 | 5.00 | 950,000.00 | 0.00 | 190,000.00 | 3. DORMANT / LOW UTILIZATION (<10%) |

Test: `guide_limit_utilisation`

## 9. Common messages and fixes

- A status-bar note when some balances are blank: they were counted as 0, so check the extract.
- "Column 'X' was not found" after a rename: pick the column again with Edit on the step.

## 10. Limits and assumptions

- Status thresholds (95%, 10%, BDT 1) are fixed in this version.
- One CCF for all rows; no CCF by facility type.
- No check that the limit is still valid (expiry) or that the facility is revolving.
