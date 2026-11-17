# Phase 3 progress

Branch: `phase3` (from `main`). Run unattended; resume from the first task that is not DONE.
Each task is one commit "Phase 3 <ID>: ..." (the hash column is filled in by the next commit). The new tests are in one binary, `tests/phase3/` (modules jobs, r3, l2, l3, l4, l5, t1–t6).
Test baseline before R1: `cargo test` = 2 (main.rs unit) + 60 (tests/review.rs) passed.

| Task | Status | Commit | Tests added | Deviations | Open questions |
|---|---|---|---|---|---|
| R1 | DONE | d442fdc | none (2 unit tests moved with their functions: tab_index_after_close, default_side_column) | headless in src/headless.rs; no font loading yet; width literals stay; prelude module; derived dark tokens | Q1 |
| R2 | DONE | 2700231 | tests/phase3/jobs.rs: 15 tests (superseded dropped; replaced queued job never runs; unique/other-tab jobs kept; cancel removes temp file; success renames; failed write leaves nothing; error and panic reach status text; inline runs on caller thread; wake callback; load/refresh numbers; unreadable source; cancelled export, split and batch) | see spec `<accepted_deviations>` (R2): previews stay sync; coarse cancel points; GUI not runnable here | Q2 |
| R3 | DONE | 907bfa0 | tests/phase3/r3.rs: 6 tests (Parquet and CSV streamed vs collected: same rows, sum, data and CSV bytes; non-streamable view falls back and status says so; Dr/Cr check + parse failures share one scan and rescan after a file change; Dr/Cr still refuses; baseline computed once for two tabs, recomputed on change; load stores baseline) | see spec (R3): message-based fallback; baseline key includes schema and control column; scan cache | |
| L1 | DONE | 22e8657 | theme tests (23 tokens light = dark counterpart, spec values, sizes, symbol→glyph map, every UI symbol has a glyph in the loaded fonts) | Source Sans 3 TTFs missing (fallback); title strip inside OS window; More menu; dark primary text; see spec | Q3 |
| L2 | DONE | f77ebf6 | tests/phase3/l2.rs: 5 tests (widest of header / selector / summary / samples wins; 420 cap and floor; only first 100 samples; "01-Sep-2026" fits on one line at the computed width; "20,391" and "1–100 of 20,391") | name-only headers; selector/summary inputs wired but empty until T3; mono dates/IDs | |
| L3 | DONE | 32a45b7 | tests/phase3/l3.rs: 7 tests (25,000,000 → 25,000.00 K / 25.00 M / 250.00 L / 2.50 Cr via column and global scale; negatives and decimals; Crore/Lakh unchanged, rates never scaled; old macro formats load; Excel header and values in scale incl. per-column override; plain scale and old export raw; PDF header rule) | Excel writes scaled values (Excel cannot scale by 1e5/1e7); Lakh already existed | |
| L4 | DONE | b262909 | tests/phase3/l4.rs: 7 tests ("'1,234.50" → 1234.5 and only one apostrophe dropped; Float64 column loads it and a Text column keeps "'0012"; Python mirror; Text column of numbers flagged with 98% and the exact question; 95% threshold and blanks; ID columns excluded; Set to Number step converts, errors name a missing column, JSON round trip) | Set to Number is a step, not a schema edit; check runs before GUI exports only | |
| L5 | DONE | be38e37 | tests/phase3/l5.rs: 10 tests (forbidden characters; 120-char cap keeps extension; macOS folder beside the .app, Library fallback when not writable, bare binary, Windows/Linux next to exe; writability probe; one version number in macro files, Python header, sources and the workflow; Phase 1, 2 and 3 macros load; old schema, preset and rule files load). New tests moved into one binary `tests/phase3/` to save disk | no Info.plist in repo (workflow writes it, now from Cargo.toml); app_version is a JSON key | |
| T1 | DONE | 67c3f12 | tests/phase3/t1.rs: 8 tests (AND / OR rows; delete keeps unknown rows; between on dates, numbers, text and a bad date bound; in list with numeric-looking text IDs; contains / starts / ends / blank; v1.2 FilterRows macro gives the same rows and shows as a group; serde defaults and chip text; Python translation) | own operator set; old FilterRows untouched; unknown test = no match | |
| T2 | DONE | 3348edf | tests/phase3/t2.rs: 6 tests (3-level text asc / number desc / date asc with ties, blanks and a stable tie; blanks last both directions and "" counts as blank; stability and two sorts in a row; empty levels and a missing column; header marks "1 ↑" / "2 ↓"; serde defaults and Python) | byte-order text sort; right-click header menu | Q4 |
| T3 | DONE | 1121b29 | tests/phase3/t3.rs: 10 tests (every aggregation on a frame with nulls; W.Avg Phase 2 rule and mixed-sign null with reason; unfit choices say why; defaults; value formatting with separators and scale; macro save and old-macro load; Excel totals row values, labels and colour; scale and last-sheet placement; PDF band; export job computes on the whole result) | ID-like numbers default to Count; weight pre-selected; exports always carry the summary; MacroRecipe.summary (2 test literals got one line) | Q5 |
| T4 | DONE | b6732bc | tests/phase3/t4.rs: 11 tests (move, delete, duplicate, insert; turn off and on; off step changes no rows and leaves a script comment; deleting a Calc column marks the later filter invalid naming Balance_Cr; reorder / turn-off revalidate; table runs up to the last valid step; row counts per step and reuse; counts stay valid until an earlier step changes; undo / redo 100 levels; edit-undo round trip) | Edit only for filters and sorts; Skipped wrapper; counts re-run prefixes; undo only for the steps list | Q6 |
| T5 | DONE | 613f4c1 | tests/phase3/t5.rs: 7 tests (number stats with nulls; histogram bins add up to the non-null count, equal values, maximum in the last bin; dates min / max; text distinct and top 5 incl. ties and blanks; other types and a missing column; all-blank column; worker job follows the steps and carries a key; found and fixed a bug: equal values gave one count not N) | blank text is not a value; change type/format is a menu | |
| T6 | DONE | 5c37be6 | tests/phase3/t6.rs: 7 tests (word start beats mid-word, exact word beats longer, label beats extra words, earlier word wins; recently used breaks ties only and orders an empty query; every word must match, letters in order, case; the real list finds filter / excel / nii / eve / lakh / undo / a column; every toolbar action, banking tool, export type, theme toggle and menu item is listed and the toolbar draws from the list; unavailable reasons; recent list kept and saved) | one command list drives toolbar and menus; Ctrl+K overlay | |

## Fixes, part 1 (correctness: review defects and owner answers)
One commit per item, "Phase 3 fix <n>: ...", full `cargo test` before each. The commit column is filled in by the next commit. UI-state tests are unit tests of the binary (`src/ui/tests.rs`: the app is built without a window and with an inline job queue).

| Item | Status | Commit | Tests added | Deviations |
|---|---|---|---|---|
| 1 Text check has its own JobKind | DONE | 0163c32 | src/ui/tests.rs: inspector failure keeps a pending export and says "Column statistics failed"; a failed Text check stops it with its own reason; cancel of each | none |
| 2 Page and summary failures are shown | DONE | 3e162c8 | src/ui/tests.rs: a step whose rows cannot be read (Dr value) keeps the last page, sets "Out of date" and the status, clears the summary band to "—" with the reason; undo clears both. tests/phase3/fixes.rs: refresh and load report the page error | see spec |
| 3 Staleness by pipeline hash | DONE | 84dae51 | fixes.rs: hash changes with a setting at the same step count, with on/off, with a column type and with a replaced file; src/ui/tests.rs: a summary result for a same-count pipeline (other filter value, filter turned off) is dropped, the current one applied | see spec |
| 4 Step-check errors surface | DONE | 964b714 | fixes.rs: a source that cannot be read gives a refresh error starting "Could not check the steps…" with "—" totals and an out-of-date page; Load fails with the same reason | see spec |
| 5 Close: cancel, delete .part, ask during export | DONE | 7434fd4 | src/ui/tests.rs: closing during a stuck export asks first (nothing cancelled), then after "Cancel export and quit" cancels, deletes the .part file and exits; closing with only a refresh running does not ask and the job stops | see spec |
| 6 Row-count cache keyed on source identity | DONE | (this) | fixes.rs: counts kept when nothing changed, only the source and first-step counts kept when the last step changes, none kept for another ingestion setting, another path or the same path rewritten; src/ui/tests.rs: a replaced file under the same steps is recounted (5, 4 instead of the old 3, 2) | none |

## Questions for the owner
- Q1 (R1): please confirm the derived dark-theme values for tokens the spec does not give (see `<accepted_deviations>`).

- Q2 (R2): the manual freeze check (open a large file, use the menus while it loads) and a look at the status-bar Cancel button and the dimmed "Updating…" table need your eyes; this container has no display libraries.

- Q3 (L1): please add `SourceSans3-Regular.ttf`, `-Semibold.ttf`, `-Bold.ttf` to `assets/fonts/` (my session could not fetch them) and look at the whole layout against `docs/design/Combined.dc.html`.

- Q4 (T2): should text sort ignore case ("ctg" next to "CTG")? It is byte order now.

- Q5 (T3): numeric columns that hold rates ("Int Rate") default to Sum as the spec says; a weighted average or no summary would be more useful. Should rate columns default to Average?

- Q6 (T4): Edit works for filters and sorts only. Do you want every step type editable in place (the Banking tools included)?

## Notes
- Disk: the test binaries are ~1.5 GB each (debug info). If a build fails with "No space left on device", delete executables in `target/debug/deps` (not rlibs) and `target/debug/incremental`; never `cargo clean`.
- R1 check: a line-by-line comparison of the old and new code (after the field renames) shows only structural changes, the 4 colour constants and the theme call.
- `grep -nE '#[0-9A-Fa-f]{6}|Color32::from_rgb' src/ui src/main.rs` matches only `src/ui/theme.rs`. `main.rs` is 504 lines.

## Final state
- All 14 tasks DONE; none BLOCKED. `cargo test`: 7 (binary) + 99 (`tests/phase3`) + 60 (`tests/review.rs`, unchanged expected values; two `MacroRecipe` literals got `summary: Default::default()`) passed.
- Phase 3 deviations are all in `<accepted_deviations>` of `docs/phase3-spec.md`.
- This container cannot open a window (no `libxkbcommon-x11`; package installs were refused), so nothing was checked by eye.

## What to look at by eye
1. The whole window against `docs/design/Combined.dc.html` (title strip, toolbar groups, tab strip, chip bar, table, right panel, status bar), light and dark.
2. Fonts: drop the three Source Sans 3 TTFs into `assets/fonts/` and restart; check that no icon is an empty box.
3. Open a large CSV and use the menus while it loads (no freeze); the status-bar progress line and Cancel; the dimmed table with "Updating…" while paging.
4. Table: column widths never wrap, drag a column edge, numbers right-aligned in mono, the header marks of a sort ("1 ↑"), the selector row and the dark summary band lining up with the columns.
5. Steps tab: timeline, selecting a step, the six buttons, an invalid step in red after deleting a Calc column, undo with Ctrl+Z.
6. Column tab after clicking a header; Ctrl+K search (type "excel", "nii", a column name).
