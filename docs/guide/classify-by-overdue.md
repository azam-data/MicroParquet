# Classify by overdue

## 1. What it does

Gives every loan the classification grade its overdue period earns under Bangladesh Bank's loan classification rules (BRPD Circular No. 15 of 27 November 2024, the Master Circular on Loan Classification and Provisioning, effective 1 April 2025): STD-0 (not overdue), STD-1, STD-2, SMA, SS, DF or B/L. An optional subjective/override grade can only make the result worse. The grade codes are the ones of `cl_code_map.json`, so the Provision step and the CL migration matrix run on the result directly.

## 2. When to use it

- Month-end classification of the loan book from the CBS overdue date or days past due, before the provision and BFP calculation and the classified-loan returns.
- Checking the CBS's own classification: compare `CL_Final` with the CBS grade.
- Rolling a past month: set the cutoff date to that month end.

## 3. Before you start

- A loan tab with an overdue-since date (type Date) or days past due (a number). A blank date counts as "not overdue".
- Rule file `portable_data/rules/cl_overdue/<date>.json` (the 2025-04-01 file is built in and written there if missing, with the circular as its source). The latest file on or before the cutoff date is used; a cutoff before 1 April 2025 has none.
- Code map `portable_data/loan_category_map.json` (written on first use): your CBS loan-type codes and their category (Continuous, Demand, Fixed term or Short-term agri & micro). Skip it if one category fits all rows.
- For an override column, its codes must be a sub-grade (STD-0, STD-1, STD-2, SMA, SS, DF, B/L) or a code in `cl_code_map.json`.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Loan category | No | Text | CC, DL, TL | Without it, pick one category for every row. Each code must be in `loan_category_map.json`. |
| Overdue-since date | One of the two | Date | 2026-07-31 | Blank = not overdue. A date after the cutoff stops the run. |
| Days past due | One of the two | Number (whole days) | 92 | Turned into the overdue-since date: cutoff less the days. Blank = not overdue; negative stops the run. |
| Override grade | No | Text | SS, STD-2 | Blank = no override. The worse of this and the overdue grade is used. |
| Overdue amount | No | Number (BDT) | 250,000 | If chosen, a row with an amount above 0 and a blank overdue date (or blank days past due) stops the run. A blank or 0 amount is fine. |
| Account number | No | Text | 0012345678901 | Named in the messages (the first column whose name looks like an account number). |

## 5. Step by step

1. Open the loan tab. Choose Banking ▸ Loans & credit ▸ "Classify by overdue…" (Ctrl+K: "classify" or "overdue").
2. Type the "Cutoff date (YYYY-MM-DD)".
3. Under "Loan category" choose "Same for every row:" and a category, or "From the column:" and the column.
4. Under "Overdue period" choose "Overdue-since date column:" or "Days-past-due column:" and the column.
5. Tick "Override grade column (the worse grade wins):" and choose the column if you have one. Tick "Overdue amount column (an amount above 0 needs a date):" and choose it to catch overdue loans whose date is missing.
6. Read the red line under the date if the button is grey. Click "Classify by overdue". The step is added; the status bar always says how many rows were treated as not overdue because the date was blank (even 0) and how many overrides were worse (used) or better (ignored).

## 6. What you get

Four new columns at the end. `Overdue_Months`: complete calendar months from the overdue-since date to the cutoff (31 Jan + 1 month is the end of February; 0 when not overdue). `CL_Objective`: the grade of the overdue period: not overdue or 0 days → STD-0; 1 day to under 1 month → STD-1; 1 to under 2 months → STD-2; 2 to under 3 → SMA; 3 to under 6 → SS; 6 to under 12 → DF; 12 or more → B/L. `CL_Final`: the worse of `CL_Objective` and the override. `CL_Rule_File`: the rule file used, for the audit trail.

## 7. Check your result

- Rows in = rows out; no row is dropped.
- Count `CL_Final` by grade and compare with the CBS classification and last month's (CL migration matrix). Differences are accounts the CBS classified late or early.
- A large count of "no overdue date" in the status bar on loans you know are overdue means the date column is the wrong one.
- Run the Provision step on `CL_Final`; its total must equal the provision from the CBS grade for accounts that agree.

## 8. Worked example

Cutoff 2026-10-31. Categories and overrides as shown; A4's CBS override is DF, A5's is STD-0.

| Account_No | Loan_Category | Overdue_Since | Override_Grade |
| --- | --- | --- | --- |
| A1 | Continuous | (blank) | (blank) |
| A2 | Demand | 2026-10-15 | (blank) |
| A3 | Fixed term | 2026-08-31 | (blank) |
| A4 | Fixed term | 2026-07-31 | DF |
| A5 | Continuous | 2026-04-30 | STD-0 |

Test: `guide_classify_by_overdue`

A2 is 16 days overdue (STD-1); A3 two complete months (31 Aug + 2 months is 31 Oct: SMA); A4 three months is SS, but the override DF is worse and is used; A5 six months is DF and the better override STD-0 is ignored.

| Account_No | Overdue_Months | CL_Objective | CL_Final | CL_Rule_File |
| --- | ---: | --- | --- | --- |
| A1 | 0 | STD-0 | STD-0 | cl_overdue/2025-04-01.json |
| A2 | 0 | STD-1 | STD-1 | cl_overdue/2025-04-01.json |
| A3 | 2 | SMA | SMA | cl_overdue/2025-04-01.json |
| A4 | 3 | SS | DF | cl_overdue/2025-04-01.json |
| A5 | 6 | DF | DF | cl_overdue/2025-04-01.json |

Test: `guide_classify_by_overdue`

## 9. Common messages and fixes

- "Loan category column 'X' has codes not in loan_category_map.json …: 'TL', (blank), on N row(s)." Add each code with its category to `portable_data/loan_category_map.json`, or fix blanks in the data.
- "CL status column 'X' has codes not in cl_code_map.json …" (for the override column). Use a sub-grade code or add the code to `cl_code_map.json`.
- "Classify by overdue: the overdue-since date is after the cutoff … on N row(s); the first 5 values of Account_No: …" The date is later than the cutoff date: fix the data or the cutoff.
- "… the overdue amount in 'X' is above 0 but the overdue date (or days past due) is blank …" Fill the date for those accounts (the first 5 are named).
- "… the days past due in 'X' is negative / is not a whole number of days / is above 36,500 (about 100 years) …", "… the overdue-since date in 'X' is outside the calendar …" Fix the data.
- "… is String, not a date" / "… not a number." Set the column type in Schema & Ingestion.
- "There is no cl_overdue file dated on or before the cutoff …" The cutoff is before 1 April 2025, or the folder has no file; add one.
- "The rule file … must grow", "… is not one of STD-0, …" The rule file is edited wrongly; fix it or delete it to get the built-in copy back.
- "… already has a column 'CL_Final'" Remove the earlier step that made it.
- "Type the cutoff date as YYYY-MM-DD", "Choose …" in red: fill the missing choice.

## 10. Limits and assumptions

- It classifies from the overdue period (and an override) only; it does not look at the other criteria of the circular (for example rescheduling or the loan's own characteristics).
- Overdue 0 days (overdue since the cutoff itself) is STD-0, because no complete day has passed.
- A blank overdue date or blank days past due means "not overdue"; the count is always in the status bar. Map the "Overdue amount" column and a row that owes an overdue amount without a date stops the run instead.
- The thresholds are the same for the four categories today; the file holds them per category, so a later circular can differ.
- An override is a grade you take from your own judgement; it can raise the grade, never lower it.
- Not translated to the Python export (the step says so).
