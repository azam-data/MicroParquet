# MicroParquet — Phase 5 specification (Risk & regulatory)

Source: the agreed roadmap (Phase 5 = risk & regulatory), the owner's Phase 4 decisions, and these Bangladesh Bank documents (cite them in rule files and guide notes):
- **IRRBB:** BRPD *Guidelines on Interest Rate Risk in the Banking Book*, 13 April 2021 (§13.1 outlier test, §14 shock scenarios, §15 measurement, Table 2 shocks, Table 3 buckets, Table 4 non-maturity deposit caps, Appendix 2).
- **Loan classification:** BRPD Circular No. 15, 27 November 2024 (Master Circular: Loan Classification and Provisioning), effective 1 April 2025.
- **IFRS 9:** BRPD Circular No. 06, 8 March 2026 (*Guidance for Banks in Implementing IFRS 9*): §4.5 staging, §4.5.5 cure, §5.9 scenarios, §9.1 higher of ECL and regulatory provision.
- **LCR/NSFR:** DOS Circular No. 01, 1 January 2015, as amended by SDAD Circular Letter No. 11, 22 July 2026 (NSFR reporting changes from 30 June 2026).

Sessions are told which task IDs to do. Read only those task sections, plus `<constraints>`, `<guide_notes>`, `<workflow>` and `<report>`.

<goal>
Phase 5 is done when:
1. Tasks F0 and R1–R7 are implemented, and `cargo build --release` and `cargo test` pass.
2. Every acceptance item is an automated assertion. Every UI item also has a headless egui render test at 1219×686 pt (105%) and 1600×900 pt (80%) that asserts on real response rects: nothing overflows its panel, dialog or card.
3. Every new tool has guide notes in `docs/guide/` following `<guide_notes>`, and `docs/guide/README.md` lists it.
4. Results of existing steps are unchanged except where a task says so, and Phase 1–4 macro, schema, preset, rule, curve and beta files still load.
</goal>

<guide_notes>
The owner will receive a tutorial-style book built from these notes, so they must be exact and practical. Write one file per tool, `docs/guide/<tool-id>.md`, in plain English for a bank ALM, treasury or credit-risk officer, with these sections in this order:
1. **What it does:** two or three sentences, including the regulatory or business purpose and the circular it follows.
2. **When to use it:** typical monthly, quarterly or half-yearly tasks and returns.
3. **Before you start:** files needed, settings and rule files to prepare, and steps that must run first.
4. **Columns you need:** a table of Column role | Required? | Type the app needs | Example values | Notes. Use roles, not fixed names.
5. **Step by step:** numbered clicks from the open tab to the result, naming each control exactly as in the UI, including the Ctrl+K command name.
6. **What you get:** every output column or table, with its name, meaning, unit and sign convention.
7. **Check your result:** control totals, what to reconcile against (GL, trial balance, the regulatory return), and typical sanity ranges.
8. **Worked example:** a small synthetic input table and the exact expected output, copied from the tool's tests. Name the test under the table.
9. **Common messages and fixes:** every error or warning text the tool can show, what it means and what to do.
10. **Limits and assumptions:** what the tool does not do yet, and any simplification versus the circular.

Keep each file under about two pages and use the UI's exact labels. When a task changes a tool, update its guide file in the same commit.
</guide_notes>

<conventions>
These apply to all Phase 5 tools:
- **Time in years:** days from the cutoff date ÷ 365.
- **IRRBB buckets (BB Table 3):** 19 buckets. Upper bounds in years: O/N 1/365, 1M 1/12, 3M 3/12, 6M 6/12, 9M 9/12, then 1, 1.5, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 20, and >20. Each bucket includes its upper bound. Midpoints: 0.0028, 0.0417, 0.1667, 0.375, 0.625, 0.875, 1.25, 1.75, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 9.5, 12.5, 17.5, 25. A cash flow at t ≤ 0 goes in O/N.
- **Signs:** assets and inflows are positive, liabilities and outflows negative. "EVE change" is negative for a loss. "Loss" columns are positive numbers.
- **Amounts:** all amounts are BDT or BDT-equivalent. Currency conversion is out of scope; say so in the guides.
- **Regulatory parameters live in dated, sourced rule files** under `portable_data/rules/…/<YYYY-MM-DD>.json`, each with `source` and `as_of`. The app picks the latest file on or before the cutoff date, and the output records which file was used. Sample files the owner must confirm carry `"sample": true`. The UI shows a "Sample parameters" banner while one is in use.
</conventions>

<tasks>

<task id="F0" name="Phase 4 follow-ups">
- **Clean line breaks:** ticking the Schema & Ingestion "Clean line breaks" check box now adds the same recorded "Clean line breaks" step (Phase 4 item 15) as the tab's first step, so it shows in Steps and replays in macros. There is one implementation. Old schema files that have the box ticked load and produce the same values as before. Test it.
- **ΔNII blank-rate error:** the error (Phase 4 item 9) names the column its account numbers came from, e.g. "first 5 values of Account_No". Test it.
</task>

<task id="R1" name="Cash-flow engine and IRRBB cash-flow ladder">
- **New module:** `src/cashflow.rs`. It is pure (no UI) and fully unit-tested. For each row it generates dated cash flows from the cutoff date.
- **Inputs (column roles):** side (A/L, or a side column), outstanding principal, annual rate %, rate type (Fixed/Floating), maturity date, next repricing date (floating), repayment type (Bullet / Equal instalment / Equal principal), frequency (Monthly, Quarterly, Half-yearly, Annual, At maturity), product code, currency.
- **Schedule:**
  - Payment dates run back from maturity by the frequency, keeping those after the cutoff.
  - Each period's interest = outstanding × rate ÷ frequency. A full period is used for the first period too; note this in the limits.
  - Equal instalment is an annuity on the remaining periods with periodic rate r/f.
  - **Floating rows:** flows run only to the next repricing date. All remaining principal, plus interest to that date, flows on the repricing date.
  - **Matured or overdue rows** (maturity ≤ cutoff): liabilities go to O/N. Assets are left out, and a note gives their count and total; this matches the Phase 4 gap ladder.
- **Cash-flow mode:** "Principal + interest" (default) or "Principal only".
- **Behavioural inputs:** when a behaviour profile (R2) is selected, two things change:
  - **Non-maturity deposits:** the core part goes in the bucket containing the product's average maturity; the non-core part goes to O/N (BB §15.16). These use principal only.
  - **Prepayment (CPR) and term deposit early redemption (TDRR), BB §15.22–15.28:**
    - Per-period prepayment fraction = 1 − (1 − CPR)^(1/f), taken from outstanding principal before the scheduled payment.
    - Expected early redemptions, TDRR × balance, go to O/N.
    - Scenario multipliers: CPR γ = 0.8 for parallel up, steepener and short up, and 1.2 for the others. TDRR u = 1.2 for parallel up, flattener and short up, and 0.8 for the others.
- **New tool:** Banking ▸ Treasury & ALM ▸ "IRRBB cash-flow ladder". Its output tab has one row per bucket: assets, liabilities, net, cumulative net, by currency.
- **Acceptance:**
  - **Annuity:** 120,000 at 12%, monthly, 12 payments left → instalment 10,661.85; the first period's interest is 1,200.00; principal sums to 120,000.00 (±0.01).
  - **Slotting:** a bullet asset maturing 2031-04-30 with cutoff 2026-10-31 → bucket (4, 5], midpoint 4.5. A matured liability → O/N. A matured asset → excluded, with note count 1.
  - **Floating:** 1,000,000 floating at 10%, quarterly, maturity 2030-06-30, repricing 2026-11-30 → one flow on 2026-11-30 of 1,000,000 plus one period's interest (25,000.00).
  - **TDRR:** a term deposit of 1,000,000 with TDRR 10% → base 100,000 in O/N and 900,000 at maturity. Parallel up → 120,000 / 880,000. Parallel down → 80,000 / 920,000.
  - **CPR:** a bullet loan of 1,000,000, annual, 2 years left, CPR 10%, principal only → base 100,000 at year 1 and 900,000 at year 2. Parallel up (γ 0.8) → 80,000 / 920,000. Parallel down (γ 1.2) → 120,000 / 880,000.
</task>

<task id="R2" name="Behavioural CASA (non-maturity deposit profile)">
- **New tool:** Banking ▸ Treasury & ALM ▸ "Behaviour profile". Its input is a history tab with date, product code and balance (monthly or daily; daily is reduced to month-end).
- **Two methods per product:**
  - **Historical minimum (default):** core % = min(balance over the look-back) ÷ latest balance × 100.
  - **Volatility:** core % = (1 − z × σ) × 100, where σ is the sample standard deviation of month-on-month % changes over the look-back. z defaults to 2.33. Floored at 0.
- **Category:** each product gets one of Retail transactional, Retail non-transactional or Wholesale, plus an average maturity in years. The user enters both, with the history defaulting to blank.
- **Caps (BB Table 4)** live in the dated rule file `portable_data/rules/irrbb_nmd_caps/2021-04-13.json`:
  - Retail transactional: core 90%, maturity 5 years.
  - Retail non-transactional: core 70%, maturity 4.5 years.
  - Wholesale: core 50%, maturity 4 years.
- **Applying caps:** a computed core % above the cap is capped and marked "capped". A typed core % or average maturity above the cap is refused with the cap and the source.
- **Short history:** if the look-back is under 120 months, a warning (not a block) says: "BB IRRBB guideline §15.13 uses 10 years of volume history; this profile uses N months."
- **Saving:** "Save profile" writes `portable_data/behaviour/<YYYY-MM-DD>.json` with fields `as_of`, `source`, and `products[]` (product, category, core_pct, avg_maturity_years, method, lookback_months, capped, cpr_pct, tdrr_pct). CPR and TDRR are entered by hand and default to 0.
- **Gap ladder (Phase 4 V3):** gains "Apply behaviour profile". With it, the volatile part of each profiled product goes in the first bucket and the core part in the bucket containing its average maturity. Without it, results are unchanged.
- **Acceptance:**
  - **Historical minimum:** month-end balances 100, 95, 110, 90, 105, 120, 98, 102, 96, 104, 108, 100 give core 90.00%. As Retail transactional it stays 90.00%; as Retail non-transactional it is capped to 70.00%.
  - **Volatility:** the same series gives σ = 0.126772 (±0.000001) and core 70.46%.
  - **Typed values:** a typed core of 80% for Retail non-transactional is refused, and so is a typed average maturity of 5 years.
  - **Short-history warning** shown, giving 12 months.
  - **Gap ladder:** savings of 1,000,000 with core 60% and maturity 3 years → 400,000 in the first bucket and 600,000 in the bucket containing 3 years. Unticking gives the Phase 4 result.
</task>

<task id="R3" name="IRRBB: six scenarios, ΔEVE and ΔNII">
- **New tool:** Banking ▸ Treasury & ALM ▸ "IRRBB (six scenarios)". It runs on one or more tabs (as in the gap ladder) and uses R1 for cash flows and R2 optionally.
- **Shocks** live in the dated rule file `portable_data/rules/irrbb_shocks/2021-04-13.json`: BDT parallel 400, short 500, long 300 bp, x = 4 (BB Table 2; BB revises BDT shocks from time to time). One entry per currency.
- **Scenarios (BB §14.2):** with s(t) = e^(−t/4):
  - parallel ±R_par
  - short ±R_short·s(t)
  - steepener −0.65·R_short·s(t) + 0.9·R_long·(1 − s(t))
  - flattener +0.8·R_short·s(t) − 0.6·R_long·(1 − s(t))
- **Discounting (BB §15.8):** each bucket's net cash flow is discounted at its midpoint with e^(−R·t). R comes from the Phase 4 curve store (same currency, the curve on or before the cutoff), linearly interpolated at the midpoint. Curve setting: "Rates are Annual compounding (converted with ln(1 + y)) / Continuous", default Annual.
  - EVE change_s = Σ CF_k × (e^(−(R_k + ΔR_s,k)·t_k) − e^(−R_k·t_k)).
  - Optional "Post-shock floor", default off (the BB guideline sets none). When on, the shocked rate is never below the floor typed in.
- **Tier 1 capital** is typed in the dialog and stored in the step.
  - Loss_s = max(0, −EVE change_s).
  - Max loss = the largest Loss_s.
  - "Loss % of Tier 1" = Loss_s ÷ Tier 1 × 100.
  - **Outlier = Yes** when max loss ≥ 15% of Tier 1 (BB §13.1). The 15% comes from the rule file.
- **ΔNII** for all six scenarios (BB Appendix 2 Table B) uses the Phase 4 V5 logic (repricing date, beta up/down, floor). The shock at each row's repricing time t is ΔR_s(t), and the weight is (1 − t) for t < 1.
- **Several currencies:** each runs with its own curve and shocks. The aggregate loss per scenario is the sum of each currency's Loss_s, with no offsets (BB §15.32).
- **Output tabs:**
  - "IRRBB results": scenario, EVE change, Loss, Loss % of Tier 1, ΔNII, plus a footer giving the max loss, the outlier flag, and the rule, curve and profile files used.
  - "IRRBB cash flows": the R1 ladder plus base PV per bucket.
- **Excel export:** both tabs go in one workbook.
- **Acceptance:** cutoff 2026-10-31, flat curve 10% continuous, an asset flow of 1,000,000 in bucket (4, 5] and a liability flow of −600,000 in O/N.
  - **Shocks at t = 3.5:** short up +208.4310 bp, steepener +21.9671 bp, flattener +61.7800 bp. At t = 4.5: steepener +76.8318 bp, flattener +8.2984 bp.
  - **Base EVE:** 37,796.13.
  - **EVE change:** parallel up −104,969.17; parallel down +125,684.16; steepener −21,723.32; flattener −2,309.54; short up −44,832.32; short down +48,236.10 (all ±0.01).
  - **Outlier test:** Tier 1 1,000,000 → max loss 104,969.17 = 10.50%, Outlier No. Tier 1 600,000 → 17.49%, Outlier Yes.
  - **Annual compounding:** a curve of 10% gives a discount factor at 4.5 of 0.651228.
  - **ΔNII:** a floating loan of 1,000,000 repricing in 30 days, beta 100%, no floor: parallel up +36,712.33; parallel down −36,712.33; steepener −28,718.10; flattener +35,629.66; short up +44,957.08; short down −44,957.08.
  - **Two currencies:** with BDT loss 100 and USD gain 50 in a scenario, the aggregate loss is 100.
</task>

<task id="R4" name="Loan classification from overdue period (BRPD 15/2024)">
- **New tool:** Banking ▸ Loans & credit ▸ "Classify by overdue". Inputs:
  - loan category (Continuous, Demand, Fixed term, Short-term agri & micro), through a code map like the Phase 1 segment map
  - "overdue since" date or a DPD-days column (DPD is turned into an overdue-since date: cutoff − DPD days)
  - an optional subjective/override grade column
- **Overdue months** are complete calendar months from the overdue-since date to the cutoff. Adding months clamps to month end, so 31 Jan + 1 month = 28/29 Feb.
- **Thresholds** live in `portable_data/rules/cl_overdue/2025-04-01.json`, per category and the same for all four today:
  - STD-0: no overdue
  - STD-1: ≥ 1 day and < 1 month
  - STD-2: ≥ 1 and < 2 months
  - SMA: ≥ 2 and < 3 months
  - SS: ≥ 3 and < 6 months
  - DF: ≥ 6 and < 12 months
  - B/L: ≥ 12 months
- **Final grade:** the worse of the objective and override grades. The status message counts overrides that were worse and overrides that were ignored because they were better.
- **Output columns:** `Overdue_Months`, `CL_Objective`, `CL_Final`, `CL_Rule_File`. The codes match `cl_code_map.json`, so the Phase 1 provisioning step and the Phase 4 migration matrix run on `CL_Final` directly.
- **Acceptance** (cutoff 2026-10-31):
  - no overdue → STD-0
  - since 2026-10-15 → STD-1
  - 2026-09-30 → STD-2
  - 2026-08-31 → SMA
  - 2026-08-01 → SMA
  - 2026-07-31 → SS
  - 2026-04-30 → DF
  - 2025-10-31 → B/L
  - cutoff 2026-02-28, since 2026-01-31 → STD-2
  - DPD 92 at cutoff 2026-10-31 → since 2026-07-31 → SS
  - objective STD-2 with override SS → SS; objective SS with override STD-0 → SS, counted as ignored
</task>

<task id="R5" name="CL-1 summary export">
- **New tool:** Banking ▸ Loans & credit ▸ "CL-1 summary". It runs on a tab that has `CL_Final` and the Phase 1 provisioning outputs.
- **Layout** comes from `portable_data/report_templates/cl1.json` (shipped as a sample for the owner to align with the current CL-1 form):
  - rows: loan categories in order
  - column groups: grades (STD = STD-0 + STD-1 + STD-2, SMA, SS, DF, B/L)
  - measures: Outstanding, Base for provision, Provision required, each picked from a column role
  - row and column totals
- **Output:** a tab, plus an Excel export laid out like the template (merged group headers, unit line under the title per Phase 4 item 17).
- **Acceptance:** a 6-row synthetic book → each cell equals the hand total, the grand totals equal the column sums, and the Excel cell positions match the template. A template that names an unknown grade is refused with the name.
</task>

<task id="R6" name="LCR and NSFR from GL balances">
- **New tool:** Banking ▸ Treasury & ALM ▸ "LCR / NSFR". Input: a GL tab (GL code, balance, and an optional residual-maturity column for NSFR lines that depend on maturity).
- **Map file** `portable_data/liquidity_maps/<name>.json`:
  - `lines[]`: code, label, ratio (LCR/NSFR), kind (hqla_l1, hqla_l2a, hqla_l2b, outflow, inflow, asf, rsf), factor_pct or factor_by_maturity {lt_6m, 6m_1y, ge_1y}, and normal side (asset/liability)
  - `gl_map[]`: match exact, prefix or range → line
  - `caps`: level2_max_pct 40, level2b_max_pct 15, inflow_cap_pct 75, lcr_min_pct 100, nsfr_min_pct 100 with "greater than" for NSFR
  - `source` and `as_of`
- **Sample map:** ship one marked sample, using Basel factors, for the owner to replace with DOS 01/2015 factors and the SDAD CL 11/2026 NSFR lines. Never present sample factors as BB's.
- **Balances:** each is turned so its normal side is positive. A line total that ends up negative gives a warning naming the line.
- **Calculation:**
  - HQLA after caps (Basel formula):
    - adj15 = max(L2B − 15/85·(L1 + L2A), L2B − 15/60·L1, 0)
    - adj40 = max(L2A + L2B − adj15 − 2/3·L1, 0)
    - L2A and L2B are taken after haircuts.
  - Net cash outflows = outflows − min(inflows, 75% of outflows).
  - LCR = HQLA ÷ NCO.
  - NSFR = ASF ÷ RSF.
  - Each ratio is flagged against its minimum.
- **Errors:**
  - Unmapped GL codes stop the run, with the count, the first 10 codes and an "Export unmapped" button.
  - A code that matches two lines stops the run.
- **Output tabs:** line detail (code, label, amount, factor, weighted) and summary.
- **Acceptance:**
  - **HQLA without the caps biting:** L1 1,000, L2A 400 (85%), L2B 100 (50%) → HQLA 1,390.00.
  - **Outflows and LCR:** outflows 10,000 × 5% + 10,000 × 10% + 1,000 × 50% = 2,000. Inflows 3,600 × 50% = 1,800, capped to 1,500, so NCO = 500 and LCR = 278.00%.
  - **Caps biting:** L1 300, L2A 600, L2B 400 → adj15 125.00, adj40 385.00, HQLA 500.00.
  - **NSFR:** ASF 1,000 × 100% + 10,000 × 95% = 10,500; RSF 8,000 × 85% (≥ 1y) + 2,000 × 50% (< 6m) = 7,800 → NSFR 134.62%.
  - **Unmapped code:** one unmapped GL stops the run, naming it.
</task>

<task id="R7" name="IFRS 9 staging and ECL (BRPD 06/2026)">
- **New tool:** Banking ▸ Loans & credit ▸ "IFRS 9 ECL".
- **Staging rules** live in `portable_data/rules/ifrs9_staging/2026-03-08.json`:
  - SICR at 30 DPD
  - default at 90 DPD
  - classified grades (SS, DF, B/L in `CL_Final`) are Stage 3: an option, default on, noted as the app's alignment because the circular gives no mapping
  - forborne Stage 1 → Stage 2
  - "SICR not assessed" → Stage 2
  - a rating downgrade of N notches or more → Stage 2 (N in the file; sample 2)
- **Cure:** with a previous-stage column and a days-performing column, a better stage applies only after 90 days performing, or 365 days for forborne exposures (§4.5.5). Otherwise the previous stage is kept.
- **Parameters** live in `portable_data/ecl_params/<YYYY-MM-DD>.json` (sample), per segment:
  - cumulative PD by year (term structure)
  - LGD %
  - CCF % for undrawn and off-balance-sheet amounts
  - scenarios: at least 3, with weight % and PD multiplier; weights must sum to 100 (§5.9)
- **Calculation:**
  - EAD = drawn + CCF × undrawn.
  - Marginal PD for year y = CumPD_y − CumPD_y−1, after PD × multiplier, capped at 1.
  - Discounting at the row's EIR: (1 + EIR)^−y.
  - **Stage 1:** year 1 only.
  - **Stage 2:** every year up to the remaining term. A final part-year scales its marginal PD by the fraction.
  - **Stage 3:** LGD × EAD.
  - ECL = Σ weight × ECL_scenario.
- **Output columns:** `Stage`, `Stage_Reason`, `EAD`, `ECL`, `ECL_Rule_File`, `ECL_Params_File`.
- **Summary tab (§9.1, Annex F style):** by grade group (Standard, SS, DF, B/L) and by stage: ECL, regulatory provision (column picked from the Phase 1 provisioning output), "Higher of", and "Shortfall to Loan Loss Reserve" = max(0, total regulatory provision − total ECL).
- **Acceptance:**
  - **Single scenario:** EAD 1,000,000, cumulative PD 2% and 5%, LGD 45%, EIR 10%, 2 years left, one scenario (100%, ×1.0) → Stage 1 8,181.82; Stage 2 19,338.84; Stage 3 450,000.00.
  - **Three scenarios** (25/50/25, ×0.8/1.0/1.5) → Stage 1 8,795.45; Stage 2 20,789.26.
  - **Undrawn amount:** drawn 800,000 + undrawn 200,000 at CCF 50% → EAD 900,000, Stage 1 7,363.64.
  - **Staging:** DPD 45 → Stage 2, reason "30+ DPD". DPD 95 → Stage 3. CL_Final SS with DPD 0 → Stage 3 when the option is on. Forborne → Stage 2. Previous Stage 3 with 60 days performing → stays Stage 3; with 95 days → Stage 2.
  - **Weights** summing to 90 are refused.
  - **Shortfall:** total regulatory provision 30,000 against ECL 19,338.84 → shortfall 10,661.16.
</task>

</tasks>

<constraints>
- No new crates or upgrades. Use std `f64::exp` and `ln`.
- Every new field on a serialised type gets `#[serde(default)]`. Old macros replay identically; add fixtures for the gap ladder and rate-shock steps with the new options unset.
- Regulatory and bank-policy parameters live only in dated or named files in `portable_data`, never in code. Ship samples clearly marked. The four BB rule files above (IRRBB shocks, NMD caps, CL overdue thresholds, IFRS 9 staging) carry the circular's numbers and its source string, and are not marked sample. CPR/TDRR, the ECL parameters, the liquidity map and the CL-1 template are samples.
- No `unwrap()`, `expect()` or index panics on user-data paths or UI input. Blank or unparseable required values stop the step with the count and the first 5 key values (the Phase 4 rule).
- UI follows `docs/design/` and the Phase 3 theme tokens. Every new dialog uses the shared dialog helper and fits 1280×720 at 80% and 105%.
- Test data is synthetic only.
</constraints>

<workflow>
- Never run `cargo clean`. Run `cargo check` while editing and the full `cargo test` before each commit.
- Make one commit per task (`Phase 5 <ID>: …`) and push after each. Update the tool's guide file and `docs/guide/README.md` in the same commit.
- Keep `docs/phase5-progress.md` updated after each task, and resume from it after a stop or compaction.
- Never stop to ask the owner mid-run. Take the option that changes the fewest existing results, record it in `<accepted_deviations>` here, and list it in the report.
- If blocked three times on the same failure, mark the task BLOCKED and continue. Dependencies: R3 needs R1, and uses R2 if present; R5 needs R4; R7 uses R4's `CL_Final` if present. F0, R2, R4 and R6 are independent.
- Open (or update) one PR from `phase5` to main, wait for the Tests check, fix failures, and don't merge.
</workflow>

<report>
Reply in no more than 30 lines: a table (Task | Status | Tests added | Deviations), the final `cargo test` summary, questions for the owner, and a "look at by eye" list.
</report>

<accepted_deviations>
- F0 (Clean line breaks) — owner answer A1: the step "Clean line breaks" has the option "Trim spaces on all text values" (`trim_all`, `#[serde(default)]`, so old macros load with it off). It is on when the Schema & Ingestion box makes the step and off by default from the Transform menu, the Ctrl+K command and the row-count warning button. With it on, every value of the chosen text columns has line breaks replaced by a space and the spaces at both ends trimmed, exactly as the old box did, so an old schema file with the box ticked gives the pre-Phase-5 values (test `f0_an_old_schema_file_with_the_box_ticked_gives_the_pre_phase_5_values`). A source that still has the old setting on (an old macro, batch, headless run, and Parquet) cleans again where it always did, before the typing and the mask, so those results are exactly as before (the earlier Phase 5 change that moved them to the end of the pipeline is reversed). Only the dialog's own tab differs in one narrow case: the step runs after the typing and the mask, so a masked column that has a line break inside a value would hash it before the cleaning; the setting `trim_before_mask` (set by the box) trims masked columns before they are masked, so their tokens are the old ones. A first all-columns step with the option off (from the Transform menu) is not the box's step: unticking the box does not remove it and the dialog does not show the box ticked for it.
- F0 (ΔNII blank-rate error): the text is now "The first 5 values of Account_No: A1, …" (rows: "The first 3 rows: row 1, …"); the Phase 4 test and the guide were updated.
- R1 (At maturity): the spec's "interest = outstanding × rate ÷ frequency" has no frequency for "At maturity", so such a row gets one payment at maturity with simple interest for the time left (days ÷ 365). A floating "At maturity" row uses the time to its repricing date the same way.
- R1 (floating, CPR) — owner answer A3: a floating row's scheduled payments dated before its next repricing date flow on their own dates (principal by the repayment type, interest of the outstanding × rate ÷ payments per year); on the repricing date (or the maturity, if that comes first) all the remaining principal plus that period's interest flow, and nothing comes after. The acceptance case (1,200,000, equal principal, monthly, 12%, cutoff 2026-10-31, maturity 2027-10-31, repricing 2027-01-31) gives 100,000 + 12,000 on 2026-11-30, 100,000 + 11,000 on 2026-12-31 and 1,000,000 + 10,000 on 2027-01-31 (test `a3_*`). The spec's bullet floating test is unchanged (one flow of 1,000,000 + 25,000 on 2026-11-30: no payment date falls before the repricing date). A repricing date that is not a payment date gets a full period's interest (the same convention as the first period). A floating row with frequency "At maturity" has no payments before the repricing date, so it is as before (simple interest for the time left). CPR is still not applied to floating rows or to "At maturity" rows; TDRR still applies to a floating deposit. In an existing assertion, a floating row whose repricing date is after its maturity now also has its scheduled interest dates before the maturity, so its last flow, not its first, is the one at maturity.
- R1 (blank values) — owner answer A2: a blank rate in "Principal + interest" mode, a floating row with no repricing date, an unknown frequency / repayment / rate type text, a blank currency (when a currency column is chosen) and a negative outstanding amount stop the run with the count and the first 5 account numbers (the Phase 4 rule). A blank maturity: a liability row whose product is a non-maturity deposit in the selected behaviour profile needs none (as before). I read "rows whose product is in the selected behaviour profile" as the profile's non-maturity deposits (a product with a category); a profile line that has only CPR/TDRR (no category) or an asset row still needs a maturity, because no flow can be dated without one. Every other blank maturity still stops the run by default, and the message names the option. The option "Treat blank maturity as O/N" (check box in the cash-flow ladder and in the IRRBB six-scenario dialog, next to "Behaviour profile"; off by default) slots such rows, assets and liabilities, in O/N, principal only (no interest, no CPR or TDRR); the ladder gives the count in the note and in a row "Blank maturity, in O/N above (N)" under each currency's buckets (information: the amounts are already in O/N). In the six-scenario run those rows also reprice at once for ΔNII. Tests `a2_*` in tests/phase5/r1.rs and render test `a2_*`.
- R1 (excluded assets): the matured-asset count and total are the last row of each currency ("Matured assets, not in ladder (N)"), as the gap ladder does, in place of a separate note.
- R1 (rule files) — owner answer A4: the source copies of the shipped rule files are in the repository under `assets/rules/<kind>/<date>.json` and built into the program with `include_str!` (`rules::SHIPPED_RULES` lists them; today `irrbb_shocks` and `irrbb_nmd_caps`, and R4 and R7 add `cl_overdue` and `ifrs9_staging`). A file is written to `portable_data/rules/<kind>/` only when it is missing, with `create_new` so that an existing file, edited or broken, is never overwritten; a newer dated file placed next to it takes over from its date. The same mechanism is used for the shipped samples (ECL parameters, liquidity map, CL-1 template). Tests `a4_*` (tests/phase5/a4.rs) check the source copies against the built-in text, the write when missing and that an edited or broken file is left alone.
- R2 (product matching and sides): the profile is matched on the product code of each gap-ladder line (a "Product" column the user chooses when the box is ticked) and splits liability rows only (a deposit); an asset of a profiled product is bucketed as before. The spec does not name the product column for the gap ladder.
- R2 (history): several rows with the same product and date are added together (accounts of one product); a missing month inside the look-back is an error (continuous month-ends), not a silent gap; daily data keeps the last date of each month.
- R2 (look-back): the "Look-back (months)" box defaults to 120; the months really used (shorter when the history is shorter) are stored in `lookback_months` and drive the short-history warning.
- R2 (typed core): a third method "Typed" lets the user type a core % (refused above the cap), because the spec refuses a typed core but gives no other way to enter one.
- R2 (products without a category): a product with no category is kept in the profile for its CPR and TDRR only (it is not a non-maturity deposit); a product that has a history needs a category.
- R2 (gap ladder) — owner answer A5: there is no CPR or TDRR in the gap ladder; only the volatile/core split of the behaviour profile is used. The gap-ladder guide says so under "Limits and assumptions", and test `a5_the_gap_ladder_does_not_use_cpr_or_tdrr` shows a CPR and a TDRR of 10% change nothing.
- R3 (EVE with a behaviour profile): the CPR and TDRR multipliers change the cash flows by scenario, so EVE change_s is the present value of scenario s's flows at the shocked rates minus the base present value of the base flows. Without a profile the flows are the same in every scenario and this is exactly the spec's formula Σ CF_k (e^(−(R_k+ΔR)t_k) − e^(−R_k t_k)).
- R3 (curve): the curve rate at a bucket midpoint is interpolated linearly in days (t × 365) and is flat beyond the first and last point; "Curve rates are" Annual (default) or Continuous; a post-shock floor is typed in percent on the same basis as the curve and applies to the shocked rate only (the base is discounted with no floor).
- R3 (Tier 1 and files): Tier 1 must be above 0. The shock file, the curves, the betas and the profile are read when "Run IRRBB" is clicked and stored in the tab source, so a refresh of the tab gives the same numbers even if a file changes; the results footer names the files. The "stored in the step" of the spec is the tab source (the IRRBB tabs are not recorded steps and are not in macros).
- R3 (output): the results tab has a "Note" column; its footer rows are Maximum loss (the scenario in Note), Outlier (Yes or No in Note), Tier 1 capital, Base EVE, Rule file, Rule source, Curve, Compounding, Behaviour profile, and a "Sample parameters" row when a sample shock file is in use. Several currencies: EVE change and ΔNII are sums; Loss is the sum of each currency's loss (no offsets). Both tabs go in one Excel workbook through the existing "Export tabs to an Excel workbook…" (no new button).
- R3 (ΔNII): it has its own optional rate floor ("ΔNII rate floor (%)") and the beta file of the rate-shock tool; a blank current rate with that floor on stops the run (as in Phase 4); rows with no date or a date on or before the cutoff reprice at once with weight 1; the shock at each row is ΔR_s(t) with t = days to the earlier of its repricing date and maturity ÷ 365.
- Constraints: no new serialised field was added to any step (the new tabs and the gap ladder's profile are tab sources, which are not serialised), so no old-macro fixture could change; the Phase 4 fixtures still replay. The "Behaviour profile" and IRRBB files are new files, not changes to old ones.
- R4 (what it is): a step, "Classify by overdue" (Banking ▸ Loans & credit; Ctrl+K "classify"; also in the Steps panel's insert menu), so it is recorded, editable in place and replays in macros (the Python export lists it as untranslated, like the Phase 1 provisioning step). It adds `Overdue_Months`, `CL_Objective`, `CL_Final` and `CL_Rule_File` at the end and refuses a table that already has one of these names.
- R4 (loan category): the spec says "through a code map like the Phase 1 segment map": `portable_data/loan_category_map.json` (same format as `segment_map.json`: `defaults_version` and `"CODE": "Category"`), written with the four category names (and FIXED-TERM, TERM, SHORT TERM AGRI & MICRO, AGRI ST) when missing and never overwritten. The dialog also offers "Same for every row" with a category, because the four categories have the same thresholds today and a bank without a category column should not have to invent one. An unmapped or blank code, or a code mapped to a category the rule file does not have, stops the run with the codes, the count and the file to edit (no default category).
- R4 (rule file): `cl_overdue/<date>.json` has `categories[].bands[]` with `grade` and one bound, `from_days` or `from_months` (the first band has none), so STD-1 is "from 1 day", STD-2 "from 1 month", and so on; the file is validated (grades from best to worst, bounds growing) and every grade must be a code of `cl_code_map.json`. It is shipped under `assets/rules/cl_overdue/2025-04-01.json`, not a sample (A4).
- R4 (blank and edge values): a blank overdue-since date or blank days past due counts as not overdue (STD-0), and the count is in the status bar note; overdue since the cutoff date itself (0 days) is STD-0 (the spec says STD-1 starts at 1 day); a date after the cutoff, a negative or fractional DPD, a text column where a date or number is needed, and a missing cutoff stop the run (the count and the first 5 account numbers where it is per row). The cutoff date is required (no "today"). The overdue-since column must be typed as a Date.
- R4 (override): a value is a sub-grade code (STD-0, STD-1, STD-2, SMA, SS, DF, B/L) as it is, or any other code through `cl_code_map.json`; a code that maps to the coarse STD counts as STD-0, which never worsens a grade. A blank override is no override. Equal grades are in neither count.
- R4 (testing): the acceptance dates, DPD 92, the 28/29 February clamp, the overrides and counts, the category map, the later dated rule file with different Demand thresholds, bad values, and `CL_Final` running through the Phase 1 provisioning step and the Phase 4 migration matrix are in `tests/phase5/r4.rs`; the dialog is in `src/ui/render_tests_p5.rs` (`r4_*`, at 1219×686 and 1600×900 pt).
</accepted_deviations>
