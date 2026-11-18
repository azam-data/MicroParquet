# CL-1 summary

## 1. What it does

Summarises the loan book for the Bangladesh Bank CL-1 return: one row for each loan category, grouped by classification grade (STD, SMA, SS, DF, B/L), with the outstanding balance, the base for provision and the provision required, and the totals across grades and across categories. The row, column and measure layout, and the Excel cell positions, come from a template file. The template that ships is a **sample**: align it with the current CL-1 form before the figures go into a return. Grades are those of BRPD Circular No. 15 of 27 November 2024.

## 2. When to use it

- Monthly or quarterly classified-loan reporting after the provision calculation.
- ALCO or board packs that show the book by grade and category.
- Reconciling the provision total of the Provision step with the return.

## 3. Before you start

- A loan tab with the grade of each loan (`CL_Final` from "Classify by overdue", or the grade of the Provision step), the loan category, the outstanding balance, and the two Provision step outputs `Base_For_Provision_BDT` and `Required_Provision_BDT`.
- The template `portable_data/report_templates/cl1.json` (written there from the built-in sample the first time if it is missing; an existing file is never overwritten). Edit it to match the form: the loan categories (rows), the grade groups, the measures and the cell positions.
- `portable_data/loan_category_map.json` if the category comes from a column (see "Classify by overdue").

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Loan category | No | Text | CC, TL, Demand | Through `loan_category_map.json` to a category of the template. Without it, one category for every row. |
| Grade | Yes | Text | STD-0, SS, B/L | A sub-grade, or a code of `cl_code_map.json` (STD counts as STD-0, BL as B/L). A grade the template does not place in a group stops the run. |
| Outstanding | Yes | Number (BDT) | 1,000,000 | Blank stops the run. |
| Base for provision | Yes | Number (BDT) | 950,000 | From the Provision step. |
| Provision required | Yes | Number (BDT) | 190,000 | From the Provision step. |

## 5. Step by step

1. Open the loan tab (with the Provision step applied). Choose Banking ▸ Loans & credit ▸ "CL-1 summary…" (Ctrl+K: "cl-1").
2. Read the "Sample parameters" line: the template is a sample until you replace it.
3. Under "Loan category" choose "Same for every row:" and a category, or "From the column:" and the column.
4. Choose the "Grade column", then the column for each measure ("Outstanding", "Base for provision", "Provision required").
5. Click "Build CL-1 summary". The tab "CL-1 summary" opens.
6. With that tab open, choose Export ▸ "Export the CL-1 summary to Excel…" (Ctrl+K: "cl-1 excel"). The sheet is laid out by the template, in the number scale you chose in the toolbar.

## 6. What you get

The tab: `Loan category`, then for each grade group and the total the three measures, named "STD | Outstanding" and so on, in BDT; the last row is "Total". The "Total" columns add the grades of the row. The Excel sheet: the title (merged across the table), the unit line under it ("Figures in BDT Crore"), a merged header for each grade group over its measures, the measure labels, the category rows and the total row at the template's positions, amounts in the chosen scale. A note under a sample layout says it is a sample.

## 7. Check your result

- "Total | Outstanding" of the last row equals the outstanding total of the loan tab; "Total | Provision required" equals the provision total of the Provision step.
- The STD, SMA, SS, DF and B/L columns of the last row equal the balances by grade of the loan tab (Pivot on the grade).
- The Excel unit line must say the scale you meant (Crore).

## 8. Worked example

Six synthetic loans.

| Account_No | Loan_Category | CL_Final | Balance | Base_For_Provision_BDT | Required_Provision_BDT |
| --- | --- | --- | ---: | ---: | ---: |
| A1 | Continuous | STD-0 | 1,000.00 | 1,000.00 | 10.00 |
| A2 | Continuous | STD-2 | 2,000.00 | 2,000.00 | 20.00 |
| A3 | Demand | SMA | 500.00 | 500.00 | 25.00 |
| A4 | Fixed term | SS | 300.00 | 250.00 | 50.00 |
| A5 | Fixed term | B/L | 100.00 | 100.00 | 100.00 |
| A6 | Short-term agri & micro | DF | 400.00 | 300.00 | 150.00 |

Test: `guide_cl1_summary`

Outstanding by category and grade (STD is A1 and A2 together). The tab names these columns "STD | Outstanding" and so on; a colon is written here because a bar would split the table.

| Loan category | STD: Outstanding | SMA: Outstanding | SS: Outstanding | DF: Outstanding | B/L: Outstanding | Total: Outstanding |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Continuous | 3,000.00 | 0.00 | 0.00 | 0.00 | 0.00 | 3,000.00 |
| Demand | 0.00 | 500.00 | 0.00 | 0.00 | 0.00 | 500.00 |
| Fixed term | 0.00 | 0.00 | 300.00 | 0.00 | 100.00 | 400.00 |
| Short-term agri & micro | 0.00 | 0.00 | 0.00 | 400.00 | 0.00 | 400.00 |
| Total | 3,000.00 | 500.00 | 300.00 | 400.00 | 100.00 | 4,300.00 |

Test: `guide_cl1_summary`

The totals across grades of the other two measures: the base is 4,150.00 and the provision 355.00 (the sum of the column).

| Loan category | Total: Base for provision | Total: Provision required |
| --- | ---: | ---: |
| Continuous | 3,000.00 | 30.00 |
| Demand | 500.00 | 25.00 |
| Fixed term | 350.00 | 150.00 |
| Short-term agri & micro | 300.00 | 150.00 |
| Total | 4,150.00 | 355.00 |

Test: `guide_cl1_summary`

## 9. Common messages and fixes

- "The template … names the grade 'X' in the group 'Y', which is not one of STD-0, STD-1, STD-2, SMA, SS, DF, B/L." Fix the template; the name is the one it holds.
- "… names the grade 'X' in more than one group", "… the measure 'X', which is not one of outstanding, base, provision", "… each below the one before": fix the template file.
- "The template … is not valid" / "has no source": the JSON is broken or incomplete.
- "CL-1 summary: the grade in 'X' is not one of the grades of the template (…) on N row(s); the first 5 values of Account_No: …" A grade code the template does not place: use `CL_Final`, or add the code to `cl_code_map.json`.
- "'X' (Outstanding) is blank or not a number on N row(s) …" Fill the amounts.
- "Loan category column 'X' has codes not in loan_category_map.json …" Add the codes to that file.
- "Choose the …" in red: fill the missing choice.
- "The active tab is not a CL-1 summary" when exporting: open the summary tab first.
- "The CL-1 table has N columns … Build the summary again": the template changed since the tab was built.

## 10. Limits and assumptions

- The shipped template is a sample, not the Bangladesh Bank form; its categories, groups, measures and positions must be confirmed by the owner.
- Amounts are BDT as in the tab; there is no currency conversion. The Excel scale is the toolbar scale.
- Steps added to the CL-1 tab afterwards are not in the Excel sheet; the sheet is the summary as built.
- A coarse grade (STD) goes with the first sub-grade of its class, so a template that splits STD-1 or STD-2 from STD-0 needs `CL_Final`.
- No ratios, no provision requirement check against the circular: the figures are summed from the columns you choose.
