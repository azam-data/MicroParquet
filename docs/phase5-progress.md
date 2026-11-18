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

## Session B (owner answers A1–A5, then R4–R7)
Branch `phase5` (PR #7). One commit per item, `cargo test`, push, don't merge. The deviations are in the spec's `<accepted_deviations>`.

| Item | Status | Commit | Tests added | Notes |
|---|---|---|---|---|
| A1 F0 trim | DONE | (this commit) | f0 ×4 rewritten, render test i15 extended | `trim_all` on the step |
| A2 R1 blank maturity | DONE | (this commit) | r1 a2_ ×3, render a2_ ×1 (both screens), R3 header rect check | option also in the IRRBB (six scenarios) dialog |
| A3 R1 floating amortisation | DONE | (this commit) | r1 a3_ ×2, guide table ×1; r1 floating assertion updated | |
| A4 shipped rule files | DONE | (this commit) | a4 ×4 | `SHIPPED_RULES` list; create_new write |
| A5 no CPR/TDRR in gap ladder | DONE | (this commit) | r2 a5 ×1 | guide only (the code already ignored them) |
| R4 | DONE | (this commit) | r4 ×10, render r4_ ×3 (both screens), guide_classify_by_overdue | step + dialog; `src/overdue.rs`, `src/problems.rs`, `src/ui/pick.rs` shared helpers |
| R5 | DONE | (this commit) | r5 ×8, render r5_ ×3 (both screens), guide_cl1_summary | `src/cl1.rs`, tab + Excel layout |
| R6 | DONE | (this commit) | r6 ×12, render r6_ ×3 + banking menu fit ×1 (both screens), guide_lcr_nsfr | `src/liquidity.rs`; Banking menu rows made compact |
| R7 | DONE | (this commit) | r7 ×11, render r7_ ×3 (both screens), guide_ifrs9_ecl | step + summary tab; `src/ecl.rs` |

R4–R7 were not asked for in Session A (the spec lists them).

## Session B, second part (owner answers B1–B3, build findings C1–C2, review fixes D1–D10)
One commit per item, except B1 and B2, which are in one commit (826ca6e, both are R7 rules). Deviations are in the spec's `<accepted_deviations>`.

| Item | Status | Commit | Tests added | Notes |
|---|---|---|---|---|
| B1 R7 cure | DONE | 826ca6e | r7 b1 | final stage never better than the current criteria |
| B2 R7 scenarios | DONE | 826ca6e | r7 scenarios | 1 unweighted, 2 refused (BRPD 06/2026 §5.9), 3+ sum to 100 |
| B3 R4 blank overdue date | DONE | 1a8d6ad | r4 b3 | count always in the note; optional "Overdue amount" role stops the run |
| C1 Steps panel buttons | DONE | 6b6d625 | render c1_* ×4 (real clicks, both screens), c1_audit_* | click area no longer over the buttons; row widgets audited, none else overlapped |
| C2 Column inspector | DONE | 3c41a7b | c2 ×N, t5 top ten, render c2_* (both screens) | p1–p99 bins, log scale, tooltips, top 10, months, stat cards never cut |
| D1 R7 cumulative PD | DONE | 06800b9 | r7 d1 | scenario scales cumulative PD, capped at 1 |
| D2 R7 grade map | DONE | 5a7f33a | r7 d2 | grades through cl_code_map.json; unknown stops the run |
| D3 R1 payment dates | DONE | 8bb2ac5 | r1 d3 | derived from the maturity each time |
| D4 R1 prepayment order | DONE | b6c47f5 | r1 d4 | interest, prepayment, then scheduled principal |
| D5 R3 required fields | DONE | 1c5e494 | r3 d5 | decay_x, outlier_pct |
| D6 R4 DPD range | DONE | aba6ca2 | r4 d6 | 0–36,500, date overflow, no panic |
| D7 R7 matured loans, blank flags | DONE | 9cb7ddd | r7 d7 | 1-year minimum term, counted |
| D8 R7 bank policy file | DONE | 4fb5d26 | r7 d8 | `portable_data/policy/ifrs9_bank_policy.json`, sample |
| D9 rule files, profiles, floating, part-years | DONE | 6b9e4de | d9 ×3, r7 part-years expectations changed | four parts, see spec |
| D10 R3 footer | DONE | 608d100 | r3 d10 | cutoff, mode, options, floors, beta file added |

`cargo test`: 148 unit, 347 phase3 (all phases), 60 review: all pass. Release build and CI not re-run for D8–D10 (see report).
