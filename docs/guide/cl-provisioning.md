# BB classified-loan provisioning and Base for Provision

## 1. What it does

Maps each loan's CBS classification code to a Bangladesh Bank grade (STD, SMA, SS, DF, BL), works out the Base for Provision (BFP) after interest suspense and eligible security, and applies the provisioning rate in force on the cutoff date. It produces the required provision per loan for the monthly CL and provision return.

## 2. When to use it

- Month-end provision requirement and shortfall against the books.
- What-if with a different cutoff date (a rate change takes effect on a date).
- Checking the CBS provision against an independent calculation.

## 3. Before you start

- A loan extract with balance, CL status, and optionally segment, interest suspense, eligible security and collateral type.
- The three rule files in `portable_data/` (created with defaults on first run): `cl_code_map.json` (CBS code → grade), `segment_map.json` (segment code → SHORT_TERM_AGRI / CMSME / OTHER) and `provision_rules.json` (rate by grade and category with effective dates). Check them against the current circulars before an official run; rates are never fixed in the program.
- The cutoff date (blank means today and the output says so).

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Outstanding balance | Yes | Number (BDT) | 1,000,000 | Blank counts as 0. |
| CL status | Yes | Text | UC, STD-1, SS, B/L | Every code must be in cl_code_map.json; an unknown code stops the run. |
| Segment / sector | No | Text | SMALL, CORPORATE | Matched exactly in segment_map.json; unlisted segments are OTHER and flagged `Segment_Unmapped`. |
| Interest suspense | No | Number (BDT) | 100,000 | Deducted from the base for classified loans. |
| Eligible security | No | Number (BDT) | 950,000 | Taken as already after haircuts. With security, the base is at least 15% of the outstanding. |
| Collateral type | No | Text | DEPOSIT_LIEN | DEPOSIT_LIEN, GOVT_SECURITY, GOVT_GUARANTEE, BB_GUARANTEE, AAA_MDB_GUARANTEE skip the 15% floor. |
| Cutoff date | Yes | YYYY-MM-DD or blank | 2026-10-31 | Selects the rule rows in force on that date. |

## 5. Step by step

1. Open the loan tab, then choose Banking ▾ → "Classified-loan provision and BFP…" (Ctrl+K: "provision").
2. In "Bangladesh Bank CL Provision & BFP Calculator" pick "Outstanding Balance Col:" and "CL Status Col (UC/SMA/SS/DF/BL):".
3. Pick the optional columns: "Optional Segment/Sector Col:", "Optional Interest Suspense Col:", "Optional Eligible Security Col:", "Optional Collateral Type Col:".
4. Type "Cutoff Date (YYYY-MM-DD, blank = today):".
5. Click "Calculate CL Provision & BFP".

## 6. What you get

New columns: `CL_Grade` (STD, SMA, SS, DF, BL), `Reg_Category`, `Segment_Unmapped` (true when the segment was not listed), `NPL_Category` ("NPL (Classified)" for SS, DF, BL, else "Performing (UC/SMA)"), `Base_For_Provision_BDT` (performing loans: the balance; classified loans: balance − suspense − security, with the 15% floor when security is deducted), `Statutory_Prov_Rate_Pct`, `Rule_Source` (the circular; "[verify]" if the rule row is still marked to be checked) and `Required_Provision_BDT` = base × rate / 100. A blank cutoff adds `Cutoff_Assumed`.

## 7. Check your result

- Sum of `Required_Provision_BDT` against the provision in the GL (the difference is the shortfall or excess).
- Sum of classified outstanding / total outstanding is the NPL ratio.
- Look at `Rule_Source` for "[verify]" and at the Segment_Unmapped count before using the figures.

## 8. Worked example

Input: five loans of BDT 1,000,000 with different codes, suspense and security, cutoff 2026-10-31.

| Loan | Balance | CL | Seg | Susp | Sec | Coll |
| --- | ---: | --- | --- | ---: | ---: | --- |
| L1 | 1,000,000.00 | UC | CORPORATE | 0.00 | 0.00 | (blank) |
| L2 | 1,000,000.00 | SS | CORPORATE | 100,000.00 | 0.00 | (blank) |
| L3 | 1,000,000.00 | B/L | CORPORATE | 0.00 | 950,000.00 | (blank) |
| L4 | 1,000,000.00 | STD-1 | SMALL | 0.00 | 0.00 | (blank) |
| L5 | 1,000,000.00 | BL | CORPORATE | 0.00 | 950,000.00 | DEPOSIT_LIEN |

Test: `guide_cl_provisioning`

Output: L3 has security 950,000, so the base is 150,000 (the 15% floor) at 100%; L5 has deposit-lien security, so no floor and the base is 50,000; L4 is a small-segment standard loan at 0.5%.

| Loan | CL_Grade | Reg_Category | Base_For_Provision_BDT | Statutory_Prov_Rate_Pct | Required_Provision_BDT |
| --- | --- | --- | ---: | ---: | ---: |
| L1 | STD | OTHER | 1,000,000.00 | 1 | 10,000.00 |
| L2 | SS | OTHER | 900,000.00 | 20 | 180,000.00 |
| L3 | BL | OTHER | 150,000.00 | 100 | 150,000.00 |
| L4 | STD | CMSME | 1,000,000.00 | 0.5 | 5,000.00 |
| L5 | BL | OTHER | 50,000.00 | 100 | 50,000.00 |

Test: `guide_cl_provisioning`

## 9. Common messages and fixes

- "CL status column 'X' has codes not in cl_code_map.json: 'XYZ', … Add each code with its grade (STD, SMA, SS, DF or BL) to that file." Add the code and grade to the file in `portable_data`, then run again.
- "No provisioning rule covers cutoff D; rules start D2 (BRPD Circular 15/2024)". The cutoff is before the first rule date in provision_rules.json.
- "provision_rules.json has N rules for grade G / category C in force on D: … Fix the overlapping dates." Edit the effective dates in the file.
- "provision_rules.json has no rule for grade G / category C (or ANY) in force on D."
- "Provision cutoff date must be YYYY-MM-DD".

## 10. Limits and assumptions

- Security is taken as the eligible value you supply; haircuts are not applied here.
- The rule files hold the rates and dates; the owner must keep them in step with circulars. Defaults are provided with their sources and a "verify" flag where the date was not confirmed.
- Provision on off-balance-sheet exposure (guarantees, undrawn limits) is not calculated.
