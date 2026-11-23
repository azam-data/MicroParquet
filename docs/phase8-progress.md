# Phase 8 progress (workspace and dates)

Branch `phase8`. Session A: W0, W1, W2, W3. Resume from this file.

| Task | Status | Commit | Tests |
| --- | --- | --- | --- |
| W3 | DONE | (see git log: "Phase 8 W3") | tests/phase8/w3.rs ×4 (numbered defaults to true for every saved form and the plain labels, the band order and its declared list; the owner's bands unnumbered in band order in the sorted step result, a pivot, the template's chart, the Excel workbook and the Python; a Phase 7 macro with the step replays to the numbered labels and its Phase 7 hash; the title "… sit at 8%–9%" in both forms); render_tests_p8 ×1 (the tick box off, on and off by real clicks, Add makes plain labels, a pivot takes the band order at once, at 3 sizes) |
| W2 | DONE | (see git log: "Phase 8 W2") | tests/phase8/w2.rs ×6 (a pivot by a date field equals the pivot by the Split date column for 6 parts; every part with two calendars adds up to the total with "(blank)" last; a chart by Quarter: Q1…Q4 and the pivot's totals, series and facet by a part, stale parts ignored, missing columns, the calendar in the calculation key; a filter by Day name in Fri, Sat keeps exactly those rows, by Year, text refused, the Python; a real column of the field's name refused, the value lists of a part, a pivot filter on a part; old pivots, charts and conditions load); render_tests_p8 ×3 (the pivot expander and Add to Rows, the chart Category picker, the filter By choice, the picker and Apply, by real clicks at 3 sizes) |
| W1 | DONE | (see git log: "Phase 8 W1") | tests/phase8/w1.rs ×6 (every part of the 1,000-row tab equals dates.rs and 3 blanks stay blank, two calendars; Datetime = its date; text column, name clash, no part, missing column, bad month refused; declared Calendar orders and a pivot by Day name Sat…Fri; label, JSON defaults, macro round trip; the generated Python parses and its part function gives the same 14 columns for all 1,000 rows); render_tests_p8 ×1 (Column panel button, ticks, text column message, Add, Edit menu, clash message, pivot order, by real clicks at 3 sizes) |
| W0 | DONE | (see git log: "Phase 8 W0") | tests/phase8/w0.rs ×13 (week rules, fiscal labels, every part as an expression and on a Datetime, blanks, text refused, Calendar and Ranges acceptance lists and the sort step, reserved names and JSON, auto order, Python mirrors run with python3, Python export, version and workflow, the Phase 7 replay of goal 3); render_tests_p8 ×2 (View ▸ Calendar… by real clicks at 3 sizes; auto order in pivot, sort by real clicks and chart) |

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
  the app and not in Python. Outside W0, so left as it is.
