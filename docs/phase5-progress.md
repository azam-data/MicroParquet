# Phase 5 progress

Branch: `phase5` (from `origin/main` at 1cea376, which holds Phase 4 and the Phase 5 spec). Run unattended; resume from the first task that is not DONE.
Each task is one commit "Phase 5 <ID>: ...". The new tests are in `tests/phase5/` (compiled into the `phase3` test binary, like `tests/phase4/`; helpers are `tests/phase4/common.rs`) and `src/ui/render_tests_p5.rs` (headless egui at 1219×686 and 1600×900 pt).
Asked for in this run: F0, R1, R2, R3 (in that order). R4–R7 were not asked for.

| Task | Status | Commit | Tests added | Deviations | Open questions |
|---|---|---|---|---|---|
| F0 | DONE | (this commit) | tests/phase5/f0.rs ×4, render_tests_p5 ×1 | see spec `<accepted_deviations>` (F0) | |
| R1 | DONE | (this commit) | tests/phase5/r1.rs ×13 (buckets, annuity, equal principal, bullet, slotting/matured, floating, TDRR, CPR, NMD, currencies, tab roles, errors, profile column) + guide_irrbb_cashflow_ladder + guide completeness; render_tests_p5 ×3 at 1219×686 and 1600×900 pt (dialog rects, real clicks on role and mode combos, build; error in the fixed header; Banking menu + search) | see spec (R1) | |
| R2 | DONE | (this commit) | tests/phase5/r2.rs ×11 (caps file and dating, minimum/capped 90/70, volatility σ 0.126772 and 70.46, typed refusals, short-history warning, daily to month-end and gaps, history read, save and read back, gap-ladder split and Phase 4 result unchanged, bucket of an average maturity, tab source) + guide_behaviour_profile; render_tests_p5 ×4 at 1219×686 and 1600×900 pt (dialog rects, real clicks to read, category choice and Save; typed refusal text; gap ladder box, profile choice and build; Banking menu + search) | see spec (R2) | |
| R3 | DONE | (this commit) | tests/phase5/r3.rs ×14 (shocks at 3.5 and 4.5, base EVE 37,796.13 and the six EVE changes, outlier 10.50/17.49 and the 15% edge, annual compounding DF 0.651228, curve interpolation and choice by date, post-shock floor, ΔNII six scenarios 36,712.33 … and betas/floor/weights, two currencies without offsets, Tier 1 and file checks, the BB shock file and a later dated file, tabs and both tables, CPR/TDRR by scenario, one Excel workbook) + guide_irrbb_six_scenarios; render_tests_p5 ×3 at 1219×686 and 1600×900 pt (dialog and header rects, real clicks on the compounding combo and Add tab, Run builds both tabs with the acceptance numbers; outlier Yes and the Sample parameters banner; Banking menu + search) | see spec (R3) | |

R4–R7 were not asked for in this run (the spec lists them; they are NOT DONE).
