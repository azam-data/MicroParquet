# Phase 4 progress

Branch: `phase4` (from `main` at c01b78b). Run unattended; resume from the first task that is not DONE.
Each task is one commit "Phase 4 <ID>: ..." (the hash column is filled in by the next commit). The new tests are in one binary, `tests/phase4/` (modules `guide`, then one per task; helpers in `common.rs`).
Test baseline before G0: see the first line under "Test counts".

| Task | Status | Commit | Tests added | Deviations | Open questions |
|---|---|---|---|---|---|
| G0 | DONE | | tests/phase4/guide.rs: 21 `guide_<tool>` example tests + guide completeness + README | see spec `<accepted_deviations>` (G0) | |
| V1 | DONE | | tests/phase4/v1.rs: 12 engine tests (natural order and true totals; W.Avg and mixed-sign null; % of row = 100; filter; totals, subtotals, several values; sort by value, dates, numbers, blanks; every aggregation and message; columns-only, rows-only, limits; step, macro file, old JSON defaults, Python; old GroupByPivot macro fixture; filter checklist values; status-bar note) + guide_pivot; src/ui/render_tests_p4.rs: 9 headless render tests at 1219×686 and 1600×900 pt (layout rects, field menu, drag and drop, chip cross, filter checklist, create tab with formats, edit in place, error footer, command wiring) | see spec (V1) | |
| V2 | TODO | | | | |
| V3 | TODO | | | | |
| V4 | TODO | | | | |
| V5 | TODO | | | | |

## Test counts

- Baseline before G0 (main c01b78b): 79 (binary) + 140 (`tests/phase3`) + 60 (`tests/review.rs`) passed.
- After G0: 79 + 140 + 23 (`tests/phase4`) + 60 passed.
- After V1: 88 (binary, incl. render_tests_p4) + 140 + 36 (`tests/phase4`) + 60 passed.
