# Phase 8 progress (workspace and dates)

Branch `phase8`. Session A: W0, W1, W2, W3. Session B: W4, W5 (done; follow-ups in "Phase 8 W5: follow-ups"). Session C: W6 (this session), then W7, W8. Resume from this file.

| Task | Status | Commit | Tests |
| --- | --- | --- | --- |
| W5 | DONE | (see git log: "Phase 8 W5" and "Phase 8 W5: follow-ups") | tests/phase8/w5.rs ×4 (a Phase 7 `settings.json` with chart tabs loads and saves back unchanged, its chart reads with the Phase 8 fields at default; the owner's case is calculated from the source and reads "Sum of Balance", a filtered source gives the source's numbers; a source's signature changes with a step or a changed file and not otherwise; the step commands are greyed on a chart tab and every other command is as it was); render_tests_p8 ×7 (a filter on the source updates the chart tab to equal a new chart, table included; a file that changes under the source does the same; Edit chart by a real click opens the studio on the source with the banner, Update chart tab keeps one chart tab with the new numbers and the title rule, Open as new chart tab leaves the first as it was; closing the source dims the chart (glyph alpha 0.45) with the message, Reopen by a real click restores it, Reopen is not offered outside a project; a chart tab of an older version stays frozen and Edit chart opens it on its own table; recalculating does not set the project's "•"; the step commands are greyed in the Edit menu and Ctrl+K by real clicks, with the note as tooltip and the status line; at 3 sizes, the "•" test at one) |
| W4 | DONE | (see git log: "Phase 8 W4") | tests/phase8/w4.rs ×10 (six kinds of tab round-trip by the hash of their rows, with keys, active tab, formats, summary, control column, chart specs, and save → open → save equal but `saved_at`; moving the project and its files together opens by relative paths; a relink to a file with an extra column works and a missing column is refused with the list, and the tabs built on it fail; Found/Changed/Missing, Look in folder, the changed-since-saved line; tabs left out with the reason and their dependants; a placeholder keeps its recipe; a newer version is refused, unknown fields ignored, recent list of 8; headless writes every tab and a failing tab writes nothing; save → open → save gives the same file bytes but the saved_at line; the save job reports each file it hashes and the cache spares the second save); src/headless.rs ×1 (`--project`/`--export-dir`); render_tests_p8 ×6 (File menu entries, Ctrl+K, Save as, "•", Save, Open ▸ Sources dialog ▸ Open, Close; Browse and Look in folder, placeholders and Save keeping them; close request prompt with Cancel/Don't save/Save, New project, recent list; the left-out list; the changed-file status line, at 3 sizes; Save is a worker job) |
| W3 | DONE | (see git log: "Phase 8 W3") | tests/phase8/w3.rs ×4 (numbered defaults to true for every saved form and the plain labels, the band order and its declared list; the owner's bands unnumbered in band order in the sorted step result, a pivot, the template's chart, the Excel workbook and the Python; a Phase 7 macro with the step replays to the numbered labels and its Phase 7 hash; the title "… sit at 8%–9%" in both forms); render_tests_p8 ×1 (the tick box off, on and off by real clicks, Add makes plain labels, a pivot takes the band order at once, at 3 sizes) |
| W2 | DONE | (see git log: "Phase 8 W2") | tests/phase8/w2.rs ×6 (a pivot by a date field equals the pivot by the Split date column for 6 parts; every part with two calendars adds up to the total with "(blank)" last; a chart by Quarter: Q1…Q4 and the pivot's totals, series and facet by a part, stale parts ignored, missing columns, the calendar in the calculation key; a filter by Day name in Fri, Sat keeps exactly those rows, by Year, text refused, the Python; a real column of the field's name refused, the value lists of a part, a pivot filter on a part; old pivots, charts and conditions load); render_tests_p8 ×3 (the pivot expander and Add to Rows, the chart Category picker, the filter By choice, the picker and Apply, by real clicks at 3 sizes) |
| W1 | DONE | (see git log: "Phase 8 W1") | tests/phase8/w1.rs ×6 (every part of the 1,000-row tab equals dates.rs and 3 blanks stay blank, two calendars; Datetime = its date; text column, name clash, no part, missing column, bad month refused; declared Calendar orders and a pivot by Day name Sat…Fri; label, JSON defaults, macro round trip; the generated Python parses and its part function gives the same 14 columns for all 1,000 rows); render_tests_p8 ×1 (Column panel button, ticks, text column message, Add, Edit menu, clash message, pivot order, by real clicks at 3 sizes) |
| W0 | DONE | (see git log: "Phase 8 W0") | tests/phase8/w0.rs ×13 (week rules, fiscal labels, every part as an expression and on a Datetime, blanks, text refused, Calendar and Ranges acceptance lists and the sort step, reserved names and JSON, auto order, Python mirrors run with python3, Python export, version and workflow, the Phase 7 replay of goal 3); render_tests_p8 ×2 (View ▸ Calendar… by real clicks at 3 sizes; auto order in pivot, sort by real clicks and chart) |

## W4 acceptance: item -> test

Library tests are `tests/phase8/w4.rs` (run with `cargo test -q --test phase3 p8_w4`); UI tests are in
`src/ui/render_tests_p8.rs` (`cargo test -q --bin MicroParquet w4_`, each at the 3 sizes unless noted); the headless test
is in `src/headless.rs`. Totals: 10 library + 6 UI + 1 headless = 17.

| Acceptance item (spec W4) | Test |
| --- | --- |
| A project of 6 tabs round-trips by hash of rows; active tab, order, formats, chart specs the same | `w4_a_project_of_six_kinds_of_tab_round_trips_by_the_hash_of_its_rows` |
| Moving the project folder and its files together opens through the relative paths | `w4_moving_the_project_folder_and_its_files_together_opens_through_the_relative_paths` |
| Relinking to a file with an extra column works; a missing column is refused with the list | `w4_relinking_to_a_file_with_an_extra_column_works_and_a_missing_column_is_refused_with_the_list` |
| Sources: Found / Changed / Missing, Browse, Look in folder | `w4_the_sources_say_found_changed_or_missing_and_look_in_folder_relinks_by_name` (library); `w4_the_sources_dialog_relinks_by_browse_and_look_in_folder_and_a_tab_that_fails_is_a_placeholder` (UI) |
| A changed file shows the status line | `w4_a_file_that_changed_since_the_project_was_saved_is_said_in_the_status_line_after_the_tabs_open` (UI) |
| Save → reopen → save: identical except `saved_at` | `w4_save_open_save_writes_the_same_file_bytes_but_the_saved_at_line` (file bytes); the JSON check inside the six-kinds test; the UI menu test saves twice and compares the files |
| The unsaved prompt appears by a real close request and Cancel keeps the app open | `w4_unsaved_changes_ask_save_dont_save_or_cancel_on_a_real_close_request_and_on_new_project` (UI: a `ViewportEvent::Close` is held, Cancel leaves `close_confirmed` false; Don't save and Save then close) |
| Headless writes the same Parquet as the UI | `w4_headless_writes_every_tab_as_parquet_and_a_failing_tab_writes_nothing` (library); `w4_headless_project_writes_every_tab_and_the_two_options_go_together` (`src/headless.rs`) |
| Render tests: menus, Sources dialog, prompt by real clicks at 3 sizes | `w4_the_file_menu_saves_closes_and_opens_a_project_by_real_clicks_at_three_sizes`, the Sources, prompt and status-line tests above, `w4_save_lists_the_tabs_it_leaves_out_and_a_newer_project_is_refused_by_the_menu` |
| Tabs that cannot be saved are left out with the reason; a placeholder keeps its recipe; a newer version is refused | `w4_tabs_that_cannot_be_rebuilt_are_left_out_with_the_reason_and_so_are_the_tabs_built_on_them`, `w4_a_tab_that_did_not_open_keeps_its_recipe_when_the_project_is_saved_again`, `w4_a_newer_project_is_refused_unknown_fields_are_ignored_and_the_recent_list_keeps_eight` |
| Owner decision: the hash runs on the worker with progress ("Hashing Loans.csv 40%"), reusing the cache; Save is a worker job | `w4_save_is_a_worker_job_the_window_does_not_wait_for` (UI: the command returns with the job pending and nothing written; a second Save is refused; the result is taken at the next frame); `w4_the_save_job_reports_each_file_it_hashes_and_the_cache_spares_the_second_save` (library: progress per file, text "Hashing Loans.csv 40%", cached file not read, stop request is an error) |

## W5 acceptance: item -> test

Library tests are `tests/phase8/w5.rs` (`cargo test -q --test phase3 p8_w5`); UI tests are in `src/ui/render_tests_p8.rs`
(`cargo test -q --bin MicroParquet w5_`), each at the 3 sizes unless noted. Totals: 4 library + 7 UI = 11. "Spec" means
the bullets of task W5 and its Acceptance list in `docs/phase8-spec.md`.

| Acceptance item (spec W5) | Test |
| --- | --- |
| Changing a filter on the source updates the chart tab's numbers to equal a new chart made from the source (its table too); nothing is calculated when nothing changed | `w5_a_filter_on_the_source_updates_the_chart_tab_to_equal_a_new_chart` (UI) |
| Recalculated when the source's data change ("steps, filters or data") | `w5_a_source_looks_different_when_its_steps_or_its_file_change_and_the_same_when_nothing_did` (library); `w5_a_file_that_changes_under_the_source_updates_the_chart_tab_to_equal_a_new_chart` (UI) — **added in the follow-ups** |
| Recalculated after a project opens | `w4_a_project_of_six_kinds_of_tab_round_trips_by_the_hash_of_its_rows` (library); `w4_the_file_menu_saves_closes_and_opens_a_project_by_real_clicks_at_three_sizes` (UI: the chart tab has the same rows and knows its source tab) |
| Edit chart opens the studio on the source with the chart tab's spec, the banner "Editing chart tab '…'", Update chart tab and Open as new chart tab | `w5_edit_chart_opens_the_studio_on_the_source_and_update_keeps_one_chart_tab_with_the_new_numbers` (UI) |
| Edit chart → change the aggregation → Update keeps one chart tab with the new numbers and the old title rule; Open as new chart tab leaves the first as it was | same test |
| Edit chart on the owner's case (Sum of Balance by Term period) shows "Sum of Balance", never "Sum of Sum of Balance" | `w5_the_chart_of_the_owners_case_is_calculated_from_the_source_and_reads_sum_of_balance` (library); the studio's measure names and the tab title in the Edit chart test (UI) |
| The steps panel of a chart tab is read-only with the note "This tab draws a chart of '…'. Change the data on that tab, or use Edit chart." | the Edit chart test (the note is drawn); the greyed-commands test (the note is the steps panel's) |
| Owner decision (B2): the step commands are greyed in the Edit menu, the toolbar and Ctrl+K, with the note as tooltip | `w5_the_step_commands_are_greyed_on_a_chart_tab_in_the_edit_menu_and_ctrl_k_by_real_clicks_at_three_sizes` (UI: real clicks on the four Edit-menu items and on the Ctrl+K row, tooltips, status line, no overlap, a step pushed by another door is refused, the source tab's items work); `w5_the_step_commands_are_greyed_on_a_chart_tab_and_every_other_command_is_as_it_was` (library: the exact list of 17) |
| Closing the source dims the chart with the message; Reopen restores it; Reopen only for a file tab of the open project; Edit chart is greyed while the source is closed | `w5_closing_the_source_dims_the_chart_with_a_message_and_reopen_restores_it` (UI: glyph alpha open 255 → closed 0.45 × → open again; the message; Reopen by a real click) — the dimming assertion was **added in the follow-ups** |
| A Phase 7 `settings.json` with chart tabs loads and is saved back unchanged (owner decision: Phase 7 never reopened them as tabs, and Phase 8 does not) | `w5_a_phase7_settings_file_with_chart_tabs_loads_and_keeps_them` (library) |
| A chart tab with no source tab is frozen, marked "Made by an older version; …", Edit chart opens it on its own table, only "Open as new chart tab" is offered | `w5_a_chart_tab_of_an_older_version_stays_as_it_was_and_edit_chart_opens_it_on_its_own_table` (UI) |
| Recalculating a chart tab does not set the project's "•" | `w5_the_project_fingerprint_does_not_move_when_a_chart_tab_is_calculated_again` (UI, one size) |
| Render tests by real clicks at 3 sizes | every UI test above but the last; the last runs at 1280×720 only |

## The Phase 7 baseline (goal 3)

`tests/fixtures/phase7_replay/` holds three sample macros (every row step kind with Tenor and list sorts, a rate-band
pivot with a list order, a tenor pivot), a pack (a slot step with a Tenor sort, a pivot with a list order, a gap
ladder run) and a saved chart, each saved by the Phase 7 code (`main` at f3e6f91, in a temporary git worktree that
shared the build cache), and `phase7_replay.hashes.json`: the blake3 of what that code gave for those and for the
seven older macro fixtures. `tests/phase8/p7_baseline.rs` is the code that made them (it uses only Phase 7 APIs);
`w0_the_phase7_samples_replay_to_the_hashes_the_phase7_code_gave` replays them with today's code.

## Deviations (also in `docs/phase8-spec.md` `<accepted_deviations>`)

- W0-1: auto order applies to the chart's **category** only. `ChartSpec` has one order (`order`); series are ordered
  by size and facets as they come, as in Phase 7. Adding orders for series and facets would change how existing
  charts are calculated, so it is left for W2/W6 if the owner wants it.
- W0-2: the filter's value picker shows a list in the Calendar or Ranges order when every label parses (with no
  custom list for the column). It is the order of the list on screen only; the filter's values are unchanged.
- W0-3: "Week of year" can reach W54 in a leap year whose 1 January is the last day of its week (2028 with a Sunday
  start: 31 Dec 2028 is W54). The rule of the spec (W01 contains 1 January) gives it; the Calendar order ranks W01–W54.
- W0-4: Ranges parses a fixed grammar: an optional "Below / Under / Less than / Up to / < / ≤" or "Above / Over /
  More than / > / ≥" before the first number, a unit after it, an optional "– / - / to" and a second number, or a
  trailing "and above / or more / +" ("and below / or less"). A label with other words ("8%–9% p.a.") is unparsed
  and goes after the ranked ones in natural order. Plain numbers alone ("1", "2") do not make a column Ranges.
- W0-5: `actionlint` was installed (`go install`), but running it was refused by this session's permission
  classifier, so it was not run. The workflow change is one step with a plain shell `if`; the test
  `w0_the_version_is_1_8_0_and_a_tag_must_match_it` checks the step is in the tag-push job before `cargo test`.
- W0-6: the Python macro export is Polars Python (as in every phase), not pandas: the Calendar and Ranges mirrors
  are plain Python functions (`PY_CALENDAR_RANGES_HELPER`) that the sort step calls through `map_elements`; the
  tests run them with python3 (Polars is not installed here).

- W0-7: tests adjusted for the intended version change: `u4_…` (Phase 6) and `l5_one_version_number_everywhere`
  (Phase 3) now expect 1.8.0. `render_tests_p8.rs` joins the other render-test files exempt from the size-literal scan.

- W1-1: the dialog lists the Date and Datetime columns first, then the other columns under "Not dates": picking one
  shows the spec's message ("'Month' is text. Set it to Date in Schema & Ingestion (D/M/Y) first.") and Add is off.
  The menu entry is in the Edit menu with the other transforms (the app has no Transform menu; Ctrl+K finds it too).
- W1-2: a prefix equal to the column's name is stored empty (the default), so a renamed column keeps naming the parts.
- W1-3: the Python export is Polars Python with a plain-Python part function (`_mp_date_part`, through
  `map_elements`), not pandas `dt`: the macro export has always been Polars. The test runs that function with
  python3 on all 1,000 dates and times (Polars is not installed here, so the whole script is parsed, not run).
- W1-4: the preview is the first 5 rows of the page in view (already in memory), worked out in the dialog.

- W2-1: a date field's name is "Open Date · Month" everywhere it is stored (pivot field, chart category, series
  and facet); the pickers and the field list show it as "Open Date › Month" (chart) and "› Month" under its column
  (pivot). A part offers Rows, Columns and Filters in the pivot, not Values.
- W2-2: the value lists of a date field (the pivot's Filters checklist, the pivot bar, the filter's picker) are read
  through a one-part Split date step named like the field (`TransformStep::date_field_step`), added to the steps
  the job reads; nothing is added to the tab.
- W2-3: the filter's "By" choice offers Date, Year, Month-Year, Month, Quarter and Day name (the spec's list);
  changing it clears the picked values (values of one part are not values of another). A filter condition on a
  part tests a temporary column `__mp_part<i>` that is dropped after the filter, in the app and in the Python.
- W2-4: the chart pickers' lists are taller (`CHART_PICKER_POPUP_H`, 460 pt) so the parts of a date column are in
  view; with many columns the list scrolls. Series order and facet order are as in Phase 7 (W0-1).
- W2-5: the pivot step has no Python translation (as in Phase 7), so W2's Python covers the filter "By" choice.
- W2-6: fixed while testing: a chart with a facet recalculated each panel on a table that already had the parts,
  which then counted as a clash; the panels now skip adding them again (`charts::calc::compute_on`).
- W2-7: the render test of the filter picks Sat and Sun: the picker's list (Phase 6) scrolls below its height, and
  Fri is the last of the seven; the library test pins Fri and Sat.

- W3-1: the template orders plain labels by Ranges (it sees the tab's table, not its steps; Ranges gives the same
  order as the declared list: Below 0%, 0%, the bands, the top band, No rate). Numbered labels keep the Natural
  order, so Phase 7 rate-profile charts are unchanged. The finding reads the bands in band order, so a tie goes to
  the lower band (with numbered labels that is the same as before).
- W3-2: the presets keep their own `numbered` (true for the built-in ones and old files). The dialog's tick box
  decides the step: off for a new step, the step's own value when a step is edited; picking a preset does not
  change the tick box.
- W3-3: tests adjusted for the intended change: the Phase 3 render test of the Rate profile dialog now expects
  the plain labels of a new step; the Phase 4 guide test of `rate-profile.md` runs the step the dialog makes
  (unnumbered), as the note's worked example now shows. The guide's chart picture `tpl-rate-profile` is drawn from
  a new sample with plain labels (`samples::rate_bands_plain`); the Phase 7 template tests keep the numbered sample.
- W3-4: the Phase 8 overlap checks measure a text by its laid-out rows (`Painted.glyphs`): in a wrapped row egui's
  galley rectangle starts at the row's left edge, before the label's leading space, so two labels of one row
  looked as if they overlapped.

## Found, not changed

- The Python Tenor mirror counts a month as 30 days (`_MP_TENOR_DAYS`), the app as 365/12; "12M" and "1Y" tie in
  the app and not in Python. The owner decided (Q2) that the mirror uses the app's day count: **built** (`tests/phase8/q.rs`).
  Follow-up (B2): the mirror must read exactly the labels the app reads. The app reads "mo"/"mos" (and "mon"/"mons"), so they
  stay; the test now puts every unit word, in four spellings, to both and found one difference (a number too big to hold:
  the app refuses it, the mirror read infinity), now fixed.
- The Ctrl+K list can overlap a row's title and its reason when the reason is long: at 1219 pt "Chart this result" and
  "Open a tool's result tab (for example the gap ladder or the IRRBB scenarios) to chart it." overlap (a Phase 7 reason).
  Not changed (not part of W5); the new test checks only the rows it is about. Worth a short reason or a tooltip-only reason.
- A project's file stamp is size and modified time in whole seconds (`project::file_stamp`): a file replaced within the same
  second by one of the same size looks unchanged to a chart tab. Not changed.
- Owner decision Q1 (`ChartSpec.series_order` and `facet_order`) is **built** (`tests/phase8/q.rs`).

## Session B (W4): deviations W4-1 to W4-13

Owner decisions (B1): W4-1 to W4-13 accepted; W4-4 is replaced: the hash runs on the worker with progress and the
cache, and Save never blocks the window (the Save job builds the recipes and hashes the files, the window writes
the file when the result arrives; Save while a save runs is refused with a message).

They are listed in `docs/phase8-spec.md` `<accepted_deviations>`. In short: a pivot tab is its source's recipe plus its
steps; a result tab is its run plus later steps; a chart tab is recalculated from its source tab on open (W5 links it);
"unsaved" is asked only with a project open; Open recent project is a submenu and a Ctrl+K list; the guide note and the
screenshots are W8; the release build was not run in session B (it was run, and passed, at the end of session B2).

## Session B2 (W5): deviations W5-1 to W5-6

They are listed in `docs/phase8-spec.md` `<accepted_deviations>`. In short: the chart tab compares its source's recipe
(source, steps that run, file size and time) every 0.4 s and is calculated again on the worker from the source; the
steps panel of a chart tab shows the note and no list; Edit chart is greyed while the source is closed; a chart tab
with no source (made before Phase 8) is frozen. Owner decisions (B2): all accepted; W5-6's "step commands are not greyed" is
replaced by **W5-7** (the step commands are greyed on a chart tab in the Edit menu, the toolbar and Ctrl+K, with the steps
panel's note as tooltip; `push_step_to_active_tab` refuses a step for a chart tab as a backstop). Phase 7 chart tabs: the
current behaviour is correct, and the spec's compatibility line now says so.
