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
(none yet)
</accepted_deviations>
