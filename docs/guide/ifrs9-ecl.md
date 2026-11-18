# IFRS 9 ECL

## 1. What it does

Puts each loan in Stage 1, 2 or 3 and works out its expected credit loss (ECL), following BRPD Circular No. 06 of 8 March 2026 (Guidance for Banks in Implementing IFRS 9): staging in section 4.5 (significant increase in credit risk at 30 days past due, default at 90 days past due), cure in section 4.5.5, forward-looking scenarios in section 5.9, and the comparison with the regulatory provision in section 9.1. Stage 1 is the loss over the first year, Stage 2 the loss over the remaining life, Stage 3 is loss given default times exposure. A summary tab sets the ECL against the regulatory provision of the Provision step and gives the shortfall to the Loan Loss Reserve. **The ECL parameters that ship (probability of default curves, LGD, credit conversion factors, scenarios) are a sample; replace them with the bank's own.**

## 2. When to use it

- Half-yearly and annual IFRS 9 reporting, and the comparison with the Bangladesh Bank regulatory provision.
- Seeing how many loans (and how much exposure) sit in each stage, and why.
- Testing the effect of other scenario weights or PD curves (edit a copy of the parameter file).

## 3. Before you start

- A loan tab with the drawn amount, the effective interest rate (%), the maturity date (Date) and, for the staging, the days past due and the classification grade (`CL_Final` from "Classify by overdue"). Optional: the undrawn amount, forborne flag, "SICR not assessed" flag, the ratings at origination and now (numbers; a bigger number is worse), the previous stage and the days performing.
- The staging rules `portable_data/rules/ifrs9_staging/<date>.json` (built in as 2026-03-08, written if missing; 30 and 90 days past due, 90 and 365 days for cure and the classified grades: the circular's own numbers).
- The bank policy `portable_data/policy/ifrs9_bank_policy.json` (built in, written if missing, marked "sample": true until the bank confirms it): whether classified grades are Stage 3 (the default of the dialog's tick box; true when the file does not say) and the rating downgrade in notches (2 in the sample). These are the bank's own choices, not the circular's. While the file is a sample, the banner shows. A file with no "sample" key counts as a sample.
- The parameters `portable_data/ecl_params/<date>.json` (a sample is written if missing): for each segment the cumulative PD by year, LGD % and CCF %; and the scenarios with weight % and PD multiplier. Section 5.9: one scenario (its weight is not used, nothing is weighted), or three or more whose weights sum to 100; exactly two are refused. The latest file dated on or before the cutoff is used; an existing file is never overwritten.
- For the summary: the Provision step's `Required_Provision_BDT`.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Segment | Yes (a column, or one segment for all) | Text | Corporate | Must be a segment name of the parameter file. |
| Drawn amount | Yes | Number (BDT) | 1,000,000 | Blank or negative stops the run. |
| Undrawn amount | No | Number (BDT) | 200,000 | Added to the exposure through the segment's CCF. |
| Effective interest rate (%) | Yes (Stage 1 and 2) | Number | 10 | Discounts each year as (1 + rate)^−year. |
| Maturity date | Yes (Stage 1 and 2) | Date | 2028-10-31 | The remaining term is the complete years from the cutoff plus the days left ÷ 365. |
| Days past due | No | Whole number | 45 | Blank stops the run (use 0 for not overdue). |
| Classification grade | No | Text | SS, BL | Read through `cl_code_map.json`; the grades SS, DF and B/L are Stage 3 if the option is ticked. An unknown or blank code stops the run. |
| Forborne, SICR not assessed | No | Y/N, Yes/No, True/False, 1/0 | Y | A blank value stops the run: write N for No. |
| Rating at origination, current rating | No (both or neither) | Number | 3, 5 | A fall of 2 notches or more is Stage 2; a blank rating skips the test. |
| Previous stage, days performing | No (both or neither) | 1, 2, 3 and a number | 3, 95 | The cure rule. |

## 5. Step by step

1. Open the loan tab. Choose Banking ▸ Loans & credit ▸ "IFRS 9 ECL…" (Ctrl+K: "ifrs 9").
2. Read the yellow "Sample parameters" line while the parameter file is a sample. Type the "Cutoff date (YYYY-MM-DD)".
3. Choose the "Segment" (the same for every row, or from a column) and the columns of the roles above. Leave optional roles on "(none)".
4. Tick or untick "Classified grades (SS, DF, B/L) are Stage 3" (ticked by default; it is the bank policy's choice, because the circular gives no mapping from the classification grades to stages; edit `classified_stage3` in the policy file to change the default).
5. Click "Calculate IFRS 9 ECL" to add the step. Or choose the "Regulatory provision (summary)" column and click "Calculate ECL and open the summary" to add the step and open the tab "IFRS 9 summary".

## 6. What you get

Six new columns at the end: `Stage` (1, 2 or 3), `Stage_Reason` (the first thing that put the loan in that stage: "90+ DPD", "Classified SS", "30+ DPD", "Forborne", "SICR not assessed", "Rating down 2 notches or more", "Cure period not over (N of 90 days performing)", or "Performing"), `EAD` (drawn + CCF × undrawn), `ECL`, `ECL_Rule_File` and `ECL_Params_File` (the files used: the staging file and the bank policy file). The status bar counts the loans in each stage.

How the stage is set: Stage 3 at 90 or more days past due, or a classified grade; Stage 2 at 30 or more days past due, forborne, SICR not assessed, or a rating downgrade; otherwise Stage 1. Cure: a loan whose previous stage was worse moves back one stage for each full cure period of days performing (90 days; 365 days for a forborne exposure), never better than the stage its current criteria give (a loan cured to Stage 1 that is 45 days past due now is Stage 2).

ECL: for each scenario, the cumulative PD of each year is multiplied by the scenario's PD multiplier and capped at 100%, and the marginal PD of year y is that of year y less that of year y−1 (so the losses never add up to more than LGD × EAD); each year's loss is the marginal PD × LGD × EAD ÷ (1 + EIR)^y; Stage 1 takes year 1 (or the remaining term if shorter), Stage 2 every year to the remaining term with a final part-year taking its fraction of the marginal PD; the scenarios are weighted. Stage 3 is LGD × EAD.

The "IFRS 9 summary" tab: rows by grade group (Standard includes SMA, then SS, DF, B/L) and by stage, with `Loans`, `EAD`, `ECL`, `Regulatory provision`, `Higher of` (the larger of the two for the row) and, on the Total rows, `Shortfall to Loan Loss Reserve` = the total regulatory provision less the total ECL, not below 0.

## 7. Check your result

- The loans in the three stages add up to the loans of the tab; the EAD total equals the drawn total plus the CCF-weighted undrawn total.
- Stage 3 ECL is LGD × EAD for each loan; a Stage 1 ECL is far below a Stage 2 ECL of the same loan.
- The "Total" rows of the grade groups and of the stages agree.
- The shortfall is what the Loan Loss Reserve must cover when the regulatory provision is above the ECL (section 9.1: the higher of the two).

## 8. Worked example

Four loans of the sample "Corporate" segment, two years to run, EIR 10%, cutoff 2026-10-31, "Classified grades are Stage 3" ticked. The sample scenarios are 50% at x1.0, 25% at x0.8 and 25% at x1.5 (a weighted multiplier of 1.075).

| Account_No | Drawn | Undrawn | DPD | CL_Final | Required_Provision_BDT |
| --- | ---: | ---: | ---: | --- | ---: |
| A1 | 800,000.00 | 200,000.00 | 0 | STD-0 | 8,000.00 |
| A2 | 1,000,000.00 | 0.00 | 45 | STD-2 | 10,000.00 |
| A3 | 1,000,000.00 | 0.00 | 0 | SS | 200,000.00 |
| A4 | 1,000,000.00 | 0.00 | 95 | B/L | 1,000,000.00 |

Test: `guide_ifrs9_ecl`

A1 has 200,000 undrawn at a CCF of 50%, so its EAD is 900,000. A2 is 45 days past due (Stage 2). A3 is not overdue but classified SS (Stage 3), and A4 is 95 days past due (Stage 3).

| Account_No | Stage | Stage_Reason | EAD | ECL |
| --- | ---: | --- | ---: | ---: |
| A1 | 1 | Performing | 900,000.00 | 7,915.91 |
| A2 | 2 | 30+ DPD | 1,000,000.00 | 20,789.26 |
| A3 | 3 | Classified SS | 1,000,000.00 | 450,000.00 |
| A4 | 3 | 90+ DPD | 1,000,000.00 | 450,000.00 |

Test: `guide_ifrs9_ecl`

The summary against the regulatory provision.

| Section | Group | Loans | EAD | ECL | Regulatory provision | Higher of | Shortfall to Loan Loss Reserve |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Grade group | Standard | 2 | 1,900,000.00 | 28,705.17 | 18,000.00 | 28,705.17 | (blank) |
| Grade group | SS | 1 | 1,000,000.00 | 450,000.00 | 200,000.00 | 450,000.00 | (blank) |
| Grade group | DF | 0 | 0.00 | 0.00 | 0.00 | 0.00 | (blank) |
| Grade group | B/L | 1 | 1,000,000.00 | 450,000.00 | 1,000,000.00 | 1,000,000.00 | (blank) |
| Grade group | Total | 4 | 3,900,000.00 | 928,705.17 | 1,218,000.00 | 1,218,000.00 | 289,294.83 |
| Stage | Stage 1 | 1 | 900,000.00 | 7,915.91 | 8,000.00 | 8,000.00 | (blank) |
| Stage | Stage 2 | 1 | 1,000,000.00 | 20,789.26 | 10,000.00 | 20,789.26 | (blank) |
| Stage | Stage 3 | 2 | 2,000,000.00 | 900,000.00 | 1,200,000.00 | 1,200,000.00 | (blank) |
| Stage | Total | 4 | 3,900,000.00 | 928,705.17 | 1,218,000.00 | 1,218,000.00 | 289,294.83 |

Test: `guide_ifrs9_ecl`

## 9. Common messages and fixes

- "IFRS 9 ECL: the effective interest rate in 'X' is blank or not a number on N row(s); the first 5 values of Account_No: …" Fill the rate (Stage 3 loans need none).
- "… the maturity date in 'X' is blank (a Stage 1 or 2 loan needs its remaining term) …" Fill the date.
- "… the days past due in 'X' is blank …" Fill it (0 for not overdue). "… the forborne in 'X' is blank (write Y or N) …" Fill the flag.
- "… the remaining term of N years is longer than the M years of the PD curve of 'S' …" Extend the cumulative PD of the segment in the parameter file.
- "… the segment column 'X' has values that are not segments of ecl_params/… " Use the segment names of the file.
- "… the scenario weights sum to 90, not 100 …" Fix the weights (section 5.9). "… has 2 scenarios …": use one scenario or at least three.
- "… choose both the rating columns / both the previous stage and the days performing …" Choose the pair or neither.
- "CL status column 'X' has codes not in cl_code_map.json: …" Add the code to that file, or fill the blank.
- "… the previous stage in 'X' is not 1, 2 or 3 …", "… is not Y/N …" Fix the values.
- "IFRS 9 summary: the tab has no column 'Stage' …" Run the IFRS 9 ECL step on the tab first.
- "There is no ifrs9_staging file dated on or before the cutoff …" The cutoff is before 8 March 2026; add a file.
- "Type the cutoff date as YYYY-MM-DD", "Choose …" in red: fill the missing choice.

## 10. Limits and assumptions

- The parameters that ship are a sample (illustrative PD, LGD, CCF and scenarios); the ECL is only as good as the parameters you put in.
- "Classified grades are Stage 3" and the rating downgrade of 2 notches are the bank's own choices, in `portable_data/policy/ifrs9_bank_policy.json` (a sample until confirmed): the circular gives no mapping from Circular 15/2024 grades to stages and names no number of notches.
- Cure is read as one stage back for each full cure period (90 days, or 365 for forborne), which is one reading of section 4.5.5; check it against your policy.
- A Stage 1 or 2 loan with a maturity on or before the cutoff uses a remaining term of 1 year (the status bar counts them); a loan that needs a longer PD curve than the file has stops the run.
- Year-end discounting at the effective interest rate; no collateral cash-flow model, no stage-3 time value of recoveries, no off-balance-sheet products other than the undrawn amount through the CCF.
- Amounts are BDT; no currency conversion. The step is not translated to the Python export.
