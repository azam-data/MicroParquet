# Phase 8 progress (workspace and dates)

Branch `phase8`. Session A: W0, W1, W2, W3. Resume from this file.

| Task | Status | Commit | Tests |
| --- | --- | --- | --- |
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

## Found, not changed

- The Python Tenor mirror counts a month as 30 days (`_MP_TENOR_DAYS`), the app as 365/12; "12M" and "1Y" tie in
  the app and not in Python. Outside W0, so left as it is.
