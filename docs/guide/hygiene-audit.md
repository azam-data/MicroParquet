# Hygiene (dirty data / ghost) audit

## 1. What it does

Flags accounts with data problems that distort ALM and regulatory numbers: "ghost" accounts that matured but are still active with a balance, open dates after maturity or close dates, interest rates outside a sane range and negative balances. The result opens in a new tab so you can send the exceptions to the branch or the CBS team.

## 2. When to use it

- Before month-end reporting, to clean the extract.
- Quarterly data-quality review.
- Explaining why the overdue bucket of the maturity profile is not empty.

## 3. Before you start

- An account extract with open, maturity and close dates typed as Dates, a status code, a balance (number), a rate (number, percent) and the account number.
- Know which status codes mean "active" in your CBS (default ACT, ACTIVE, A, OPEN).
- Choose the reporting cutoff date.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Open date | Yes | Date | 2025-01-10 | D/M/Y dates are fine once typed as a date. |
| Maturity date | Yes | Date | 2026-09-30 | Blank is allowed (no maturity). |
| Close date | Yes | Date | blank or 2026-06-30 | Blank is allowed. |
| Status | Yes | Text | ACT, CLS | Matched trimmed and ignoring case. |
| Balance | Yes | Number (BDT) | 500,000 | Blank counts as 0. |
| Interest rate | Yes | Number (percent) | 9.0 | A blank rate is not flagged. |
| Account number | Yes | Text | H1 | Kept as it is. |
| Active status codes, rate bounds | Yes | Typed in the dialog | ACT, ACTIVE; below 0, above 25 | Rates outside the bounds are outliers. |

## 5. Step by step

1. Open the tab, then choose Edit ▸ Data checks ▸ "Core banking dirty-data auditor…" (Ctrl+K: "audit").
2. In "Core Banking Dirty Data & Ghost Account Auditor" pick "Open Date:", "Maturity Date:", "Close Date:", "Status (Acstat):", "Balance Col:", "Int Rate Col:" and the account column.
3. Type "Reporting Cutoff Date (YYYY-MM-DD):" and "Active status codes (comma-separated, any case):".
4. Set "Rate outlier below %:" and "or above %:".
5. Tick "Show Exception Rows Only (Filter out 'OK' accounts)" to list only problem accounts.
6. Click "Run Audit into New Exception Tab".

## 6. What you get

Two columns. `Audit_Exception_Flag` shows the first flag that applies in this order: "1. GHOST_MATURED_ACTIVE" (active status, balance above 0, maturity before the cutoff), "2. BACKWARDS_DATE_ANOMALY" (maturity or close date before the open date), "3. INTEREST_RATE_OUTLIER", "4. NEGATIVE_BALANCE", else "OK". `Audit_All_Flags` lists every flag that applies, separated by "; ".

## 7. Check your result

- Count of exceptions by flag (Pivot on `Audit_Exception_Flag`) and the balance sum of ghost accounts: that is the amount to explain to the CBS team.
- Without "exceptions only", the row count equals the source row count.

## 8. Worked example

Input: five accounts, cutoff 2026-10-31, active codes ACT / ACTIVE / A / OPEN, rates allowed 0% to 25%.

| Actnum | Opndat | Matdat | Clsdat | Acstat | Balance | Rate |
| --- | --- | --- | --- | --- | ---: | ---: |
| H1 | 2025-01-10 | 2026-09-30 | (blank) | ACT | 500,000.00 | 9.00 |
| H2 | 2026-05-01 | 2026-12-01 | 2026-04-01 | ACT | 100,000.00 | 60.00 |
| H3 | 2026-01-01 | 2027-01-01 | (blank) | ACT | -2,000.00 | 9.50 |
| H4 | 2026-01-01 | 2027-06-01 | (blank) | ACT | 300,000.00 | 9.00 |
| H5 | 2024-06-01 | 2026-06-30 | 2026-06-30 | CLS | 0.00 | 8.00 |

Test: `guide_hygiene_audit`

Output: H1 matured on 2026-09-30 and is still active; H2 was closed before it opened and has a 60% rate (two flags); H3 is negative.

| Actnum | Audit_Exception_Flag | Audit_All_Flags |
| --- | --- | --- |
| H1 | 1. GHOST_MATURED_ACTIVE | 1. GHOST_MATURED_ACTIVE |
| H2 | 2. BACKWARDS_DATE_ANOMALY | 2. BACKWARDS_DATE_ANOMALY; 3. INTEREST_RATE_OUTLIER |
| H3 | 4. NEGATIVE_BALANCE | 4. NEGATIVE_BALANCE |
| H4 | OK | OK |
| H5 | OK | OK |

Test: `guide_hygiene_audit`

## 9. Common messages and fixes

- "Hygiene audit: enter at least one active status code (for example ACT, ACTIVE, A, OPEN)."
- "Hygiene audit: the rate outlier bounds a% to b% are not a valid range (the lower bound must not exceed the upper)."
- "Column 'X' is String, not a date. Set it to a Date type…" for any of the three date columns.
- "Cutoff Date must be YYYY-MM-DD".

## 10. Limits and assumptions

- Four checks only; duplicate accounts, missing branches or impossible dates in the future are not checked.
- Status codes and bounds are yours to set; the defaults are only a starting point.
- A blank maturity is never a ghost.
