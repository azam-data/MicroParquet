# Phase 4 progress

Branch: `phase4` (from `main` at c01b78b). Run unattended; resume from the first task that is not DONE.
Each task is one commit "Phase 4 <ID>: ..." (the hash column is filled in by the next commit). The new tests are in `tests/phase4/` (modules `guide`, then one per task; helpers in `common.rs`); they are compiled into the `phase3` test binary (`tests/phase3/main.rs` includes them with `#[path]`), because every integration test binary links all of Polars and the CI runner's disk is small. Run them with `cargo test --test phase3`.
Test baseline before G0: see the first line under "Test counts".

| Task | Status | Commit | Tests added | Deviations | Open questions |
|---|---|---|---|---|---|
| G0 | DONE | 493095f | tests/phase4/guide.rs: 21 `guide_<tool>` example tests + guide completeness + README | see spec `<accepted_deviations>` (G0) | |
| V1 | DONE | 1cd9163 | tests/phase4/v1.rs: 12 engine tests (natural order and true totals; W.Avg and mixed-sign null; % of row = 100; filter; totals, subtotals, several values; sort by value, dates, numbers, blanks; every aggregation and message; columns-only, rows-only, limits; step, macro file, old JSON defaults, Python; old GroupByPivot macro fixture; filter checklist values; status-bar note) + guide_pivot; src/ui/render_tests_p4.rs: 9 headless render tests at 1219×686 and 1600×900 pt (layout rects, field menu, drag and drop, chip cross, filter checklist, create tab with formats, edit in place, error footer, command wiring) | see spec (V1) | |
| V2 | DONE | 59484d5 | tests/phase4/v2.rs: 8 tests (acceptance example cells and totals, counts and % of row, normalised codes / worst grade / segment blocks, unmapped codes message, bad settings, shading rule, tab source with steps and fingerprint, command and search) + guide_cl_migration; render_tests_p4: 3 tests at 1219×686 and 1600×900 pt (dialog rects and a real combo click, green and amber cells on the right cells under the right headings, Banking menu headings) | see spec (V2) | |
| V3 | DONE | e5803b9 | tests/phase4/v3.rs: 7 tests (acceptance example, limit and breach on the first row only incl. boundaries, blank maturity / overdue / side column / several tabs, bad input and zero total assets, preset order and limits file reading and validation, tab source with steps, shipped sample file) + guide_gap_ladder; render_tests_p4: 3 tests at 1219×686 and 1600×900 pt (dialog rects and real clicks to choose a role and add a tab, build the ladder, breached row tinted under the right headings, Banking menu heading and search) | see spec (V3) | |
| V4 | DONE | b618657 | tests/phase4/v4.rs: 8 tests (each deal on its origination curve 8.99701 / 9.99701 with curve dates, older deal error naming the count or the earliest curve with FTP_Note, fixed date uses the January curve for all, interpolation and missing dates, CSV with tenors 30, 30 refused and every other bad file, curve files saved/listed/checked/deleted, step reads the store and follows an edited curve, old Phase 1 and 2 step JSON and macro round trip) + guide_ftp_curve_mapper (dated example) + guide_yield_curves; render_tests_p4: 3 tests at 1219×686 and 1600×900 pt (manager rects, real clicks and key input, save/import/delete; FTP dialog choice by a real click, step makes FTP_Curve_Date; command and search) | see spec (V4) | |
| V5 | DONE | 8e20caf | tests/phase4/v5.rs: 7 tests (floating loan +9,178.08 and 0.00 without the repricing date, earlier-of reprice and maturity incl. blanks and a text date, beta -4,000.00 and unmapped products counted, floor 15,000.00 against 20,000.00 and its edge cases, beta file reading and validation, step with beta file + status note + missing file + old Phase 4 fixtures replay byte for byte + old JSON + macro round trip, repricing-gap summary buckets/totals/agreement with the step) + guide_rate_shock_nii (options and gap summary); render_tests_p4: 2 tests at 1219×686 and 1600×900 pt (dialog rows inside the window, real clicks to choose the repricing column and the beta file, step settings, summary tab and the step agree, status note) | see spec (V5) | |

## Test counts

- Baseline before G0 (main c01b78b): 79 (binary) + 140 (`tests/phase3`) + 60 (`tests/review.rs`) passed.
- After G0: 79 + 140 + 23 (`tests/phase4`) + 60 passed.
- After V1: 88 (binary, incl. render_tests_p4) + 140 + 36 (`tests/phase4`) + 60 passed.
- After V2: 91 (binary) + 140 + 45 (`tests/phase4`) + 60 passed (confirmed by the full run).
- After V3: 94 (binary) + 140 + 53 (`tests/phase4`) + 60 passed.
- After V4: 97 (binary) + 140 + 62 (`tests/phase4`) + 60 passed.
- After V5: 99 (binary) + 140 + 69 (`tests/phase4`, inside the `phase3` binary, which therefore runs 209 tests) + 60 passed.

## Final state
- All six tasks DONE; none BLOCKED. `cargo test`: 99 (binary, incl. `src/ui/render_tests_p4.rs`) + 140 (`tests/phase3`) + 69 (`tests/phase4`, inside the `phase3` binary, which therefore runs 209 tests) + 60 (`tests/review.rs`) passed. No expected value of an earlier test changed; the only edits to older tests are the new fields in struct literals (`curve_mode`, and `reprice_col`, `beta_file`, `product_col`, `floor_pct`) in `src/ui/tests.rs`, `tests/phase3/l5.rs` and `tests/review.rs`.
- Old-macro fixtures made by the code of `main`: `tests/fixtures/phase4_*` (see its README).
- Every deviation is in `<accepted_deviations>` of `docs/phase4-spec.md`.
- This container cannot open a window, so nothing was looked at by eye; the render tests draw the real UI in a headless egui context at 1219×686 pt (105%) and 1600×900 pt (80%) and check the rectangles of what was drawn.

## Questions for the owner
1. V1: should an old "Group/Pivot" step be turned into a new pivot step when it is edited (its column names and "% of total" metric would change)? It now keeps the older dialog.
2. V1: "Scale" is a display format of the pivot's value columns (the numbers stay in BDT). Is that what the design means, or should the pivot divide the numbers?
3. V2: the balance in a migration cell is last month's balance (this month's for a new account). Do you want this month's balance for accounts that are still there?
4. V3: the ladder has no totals row (the spec says one row per bucket); add one?
5. V4: should the FTP dialog open on the stored curves when there are any (it opens on the typed curve, as before)?
6. V5: should a beta apply differently to a rise and a fall (two percentages)? Today one percentage multiplies the whole shock.

## What to look at by eye
1. Pivot builder (Transform ▾ Pivot, or Ctrl+K "pivot"): the fields list and its type badges, dragging a field into an area (the ghost chip follows the pointer), the "..." menu, a Filters checklist, the Preview, the Scale and Sort rows choices, light and dark; at 1219×686 the right side is a little taller than the dialog's middle, so it scrolls.
2. CL migration matrix: the green and amber cells in light and dark, and the "Loans & credit" / "Treasury & ALM" headings in the Banking menu.
3. Liquidity gap ladder: the dialog's lines, the limits note, the red-tinted BREACH row.
4. Yield curves manager and the FTP dialog's new "Yield curve:" choices; importing a real CSV with the file picker (the picker itself is not covered by tests).
5. Rate shock dialog: the new "Repricing, pass-through and floor" rows, and the "Repricing gap summary" tab.
6. `docs/guide/`: read two or three guides against the real menus (the menu paths and button names were taken from the source, not from a running window).
