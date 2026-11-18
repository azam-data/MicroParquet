# Phase 6 progress

Branch: `phase6` (from `origin/main` at 9aadb59, which holds Phase 5 and the Phase 6 spec). Run unattended; resume from the first task that is not DONE.
Asked for in this run, in this order: U3, U1, U2, U6, U4, U5, U7. P1–P6 were not asked for.
Each task is one commit "Phase 6 <ID>: ...". New tests: `tests/phase6/` (compiled into the `phase3` test binary, like phase4/5) and `src/ui/render_tests_p6.rs` (headless egui at 1219×686 and 1600×900 pt).

| Task | Status | Commit | Tests added | Deviations |
|---|---|---|---|---|
| U3 | DONE | (this commit) | tests/phase6/u3.rs ×11, guide_custom_sort_lists + completeness, render_tests_p6 ×3 (both screens) | D1–D4 below |
| U1 | DONE | (this commit) | tests/phase6/u1.rs ×10 (rename order and typing, header match and no match, duplicate names, stack, batch, Python, old schema, warning with the step named, parse failures and masks, swap), render_tests_p6 ×5 (both screens; real clicks on Undo, Lock Schema & Open, picker buttons, Apply anyway / Cancel) | D7–D10 |
| U2 | DONE | (this commit) | src/settings.rs ×2 unit; render_tests_p6 ×8 (drag to 500, clamp to half the window and to today's width, save and restart, double-click reset, three tabs in one row at both widths, 30 steps scroll to the last step and click it, SQL tab layout, 12-line Ctrl+Enter and Ctrl+Shift+Enter, bad query error, history of 10, View menu and Ctrl+K); guide_sql_editor + completeness | D11–D14 |
| U6 | | | | |
| U4 | | | | |
| U5 | | | | |
| U7 | | | | |

## Deviations (also in the spec's `<accepted_deviations>`)
- D1 (U3): the sort step and the pivot keep a copy of the list they use (name, values, case flag) instead of its name, so a macro replays the same order if the list file is edited, renamed or missing. Fewest changes to existing results.
- D2 (U3): the Sort dialog keeps its "Lowest first / Highest first" choice and adds an "Order" choice (Natural, each list, Tenor). "A→Z / Z→A" are the existing radios; "Highest first" reverses a list.
- D3 (U3): in the pivot, "By value" is the existing "sort rows by the first value field, high to low" and is offered for Rows fields only; Columns have no by-value order today.
- D4 (U3): with a list or Tenor, values that are not in the list are always after the listed ones, in natural A→Z order, also for Z→A (the spec's example: Low, Medium, High, Other, blank).
- D5 (U3): the Tenor count of values that are not tenors is read from the first 2,000 distinct values of the column (the existing value-list job); the status line says so when the column has more.
- D6 (U3 fixture): `phase6_sort_pivot_unset` was written by hand in the Phase 5 form (see tests/fixtures/README.md).
- D7 (U1): a blank "Rename to" keeps the file's name, so "a name that is empty after trimming" is reported for a file column that has no name and no rename. Two columns with the same final name are both marked.
- D8 (U1): a Parquet file that matches a saved schema opens straight away with a status message naming it (the Parquet dialog is the tab's Schema button, so there is no dialog and no Undo for it at open).
- D9 (U1): the warning about steps is shown once the worker has checked the new schema, before the tab changes. "Apply anyway" keeps the steps (shown as broken), Cancel leaves the tab as it was. Steps that were already broken are not listed.
- D10 (U1): parse failures, the masked-column list and the Column inspector use the final names; "Convert to Parquet…" is disabled with a bad name, like Apply and Save.
- D11 (U2): the resize handle is our own strip (6 pt inside the panel's left edge) instead of egui's, so a wide child can never push the panel wider; the panel is drawn at exactly the width we keep (`right_panel_w`).
- D12 (U2): a query is checked on the worker (the engine builds it as the next step) before it becomes a step; a wrong query adds no step and its message stays in the box under the editor. "Apply here" and "New tab" both work this way.
- D13 (U2): the one-line SQL bar is gone; View ▸ "SQL" and the Ctrl+K command "SQL editor" (needs an open tab) open the panel on the SQL tab. Each tab of the panel scrolls with an always-visible solid bar, so the content is about 10 pt narrower than before.
- D14 (U2): fixed an existing 4 pt overflow of the three buttons at the foot of the Steps tab (the panel used to hide it by clipping).
