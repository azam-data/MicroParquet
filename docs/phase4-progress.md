# Phase 4 progress

Branch: `phase4` (from `main` at c01b78b). Run unattended; resume from the first task that is not DONE.
Each task is one commit "Phase 4 <ID>: ..." (the hash column is filled in by the next commit). The new tests are in one binary, `tests/phase4/` (modules `guide`, then one per task; helpers in `common.rs`).
Test baseline before G0: see the first line under "Test counts".

| Task | Status | Commit | Tests added | Deviations | Open questions |
|---|---|---|---|---|---|
| G0 | DONE | | tests/phase4/guide.rs: 21 `guide_<tool>` example tests + guide completeness + README | see spec `<accepted_deviations>` (G0) | |
| V1 | DONE | | tests/phase4/v1.rs: 12 engine tests (natural order and true totals; W.Avg and mixed-sign null; % of row = 100; filter; totals, subtotals, several values; sort by value, dates, numbers, blanks; every aggregation and message; columns-only, rows-only, limits; step, macro file, old JSON defaults, Python; old GroupByPivot macro fixture; filter checklist values; status-bar note) + guide_pivot; src/ui/render_tests_p4.rs: 9 headless render tests at 1219×686 and 1600×900 pt (layout rects, field menu, drag and drop, chip cross, filter checklist, create tab with formats, edit in place, error footer, command wiring) | see spec (V1) | |
| V2 | DONE | | tests/phase4/v2.rs: 8 tests (acceptance example cells and totals, counts and % of row, normalised codes / worst grade / segment blocks, unmapped codes message, bad settings, shading rule, tab source with steps and fingerprint, command and search) + guide_cl_migration; render_tests_p4: 3 tests at 1219×686 and 1600×900 pt (dialog rects and a real combo click, green and amber cells on the right cells under the right headings, Banking menu headings) | see spec (V2) | |
| V3 | DONE | | tests/phase4/v3.rs: 7 tests (acceptance example, limit and breach on the first row only incl. boundaries, blank maturity / overdue / side column / several tabs, bad input and zero total assets, preset order and limits file reading and validation, tab source with steps, shipped sample file) + guide_gap_ladder; render_tests_p4: 3 tests at 1219×686 and 1600×900 pt (dialog rects and real clicks to choose a role and add a tab, build the ladder, breached row tinted under the right headings, Banking menu heading and search) | see spec (V3) | |
| V4 | DONE | | tests/phase4/v4.rs: 8 tests (each deal on its origination curve 8.99701 / 9.99701 with curve dates, older deal error naming the count or the earliest curve with FTP_Note, fixed date uses the January curve for all, interpolation and missing dates, CSV with tenors 30, 30 refused and every other bad file, curve files saved/listed/checked/deleted, step reads the store and follows an edited curve, old Phase 1 and 2 step JSON and macro round trip) + guide_ftp_curve_mapper (dated example) + guide_yield_curves; render_tests_p4: 3 tests at 1219×686 and 1600×900 pt (manager rects, real clicks and key input, save/import/delete; FTP dialog choice by a real click, step makes FTP_Curve_Date; command and search) | see spec (V4) | |
| V5 | TODO | | | | |

## Test counts

- Baseline before G0 (main c01b78b): 79 (binary) + 140 (`tests/phase3`) + 60 (`tests/review.rs`) passed.
- After G0: 79 + 140 + 23 (`tests/phase4`) + 60 passed.
- After V1: 88 (binary, incl. render_tests_p4) + 140 + 36 (`tests/phase4`) + 60 passed.
- After V2: 91 (binary) + 140 + 45 (`tests/phase4`) + 60 passed (confirmed by the full run).
- After V3: 94 (binary) + 140 + 53 (`tests/phase4`) + 60 passed.
- After V4: 97 (binary) + 140 + 62 (`tests/phase4`) + 60 passed.
