# MicroParquet — Phase 3 specification (speed, new look, table tools)

Source: owner-approved design "Chosen · A look + C steps + D inspector and Ctrl+K" (reference copies in `docs/design/`), the v1.1.1 review's Phase 3 list, and the owner's feature requests of 16 Nov 2026. Sessions are told which task IDs to do; read only those task sections plus `<constraints>`, `<workflow>` and `<report>`. The pivot builder (`docs/design/Pivot.dc.html`) is Phase 4, not this phase.

<goal>
Phase 3 is done when:
1. Every task below is implemented, and `cargo build --release` and `cargo test` pass.
2. The UI never freezes: loading, step changes, paging, totals, exports and batch runs happen off the UI thread, and each can be cancelled.
3. The main window matches `docs/design/Combined.dc.html` within what egui can draw, using only the tokens in `<design_tokens>`.
4. Calculation results are unchanged: every existing test still passes without changing its expected numbers.
5. Phase 1 and 2 macro, schema, preset and rule files still load.
</goal>

<design_tokens>
Fonts (bundle the TTF files and their OFL licence texts under `assets/fonts/`; load them with `egui::FontDefinitions`):
- UI text: Source Sans 3 (Regular 400, SemiBold 600, Bold 700).
- Numbers, IDs, dates in cells, totals, counts: JetBrains Mono (Regular 400, SemiBold 600). egui cannot switch on tabular digits, so a monospace font is how columns of numbers line up.
- Icons: an icon font with an egui 0.29-compatible crate (for example `egui-phosphor`), or the Phosphor TTF bundled directly if no compatible crate version exists. No glyph may render as an empty box.

Sizes (logical px at 100% UI scale): base text 14, small 13, labels 12, inspector title 20. Title bar 36, toolbar 52, tab strip 40, status bar 32, right panel 316 wide. Table: header row 38, selector row 30, summary band 38, data rows 32. Corner radius 6 for controls, 8 for panels and the table frame. Spacing steps 4, 8, 12, 16.

Light theme (default):
| Token | Hex | Use |
|---|---|---|
| bg | #F6F7F9 | window background, selector row |
| surface | #FFFFFF | toolbar, panels, table |
| border | #E3E6EB | panel and table borders |
| grid | #F0F2F5 | cell dividers |
| zebra | #FAFBFC | alternate data rows |
| ink | #1B2430 | main text |
| ink2 | #3D4856 | secondary text, icons |
| muted | #5B6675 | hints, counts, labels |
| accent | #1F5FAD | active tab line, selected step, primary button |
| accent_dark | #174A87 | text on accent_soft |
| accent_soft | #EAF1FB | filter chips, selected step, selected column header |
| accent_soft_border | #C9DBF3 | chip borders |
| summary_bg | #1F3A5F | summary band background |
| summary_fg | #FFFFFF | summary band text |
| summary_divider | #2E4C75 | summary band cell dividers |
| selector_on_bg / border / fg | #FFFFFF / #B9C7DA / #1F3A5F | active aggregation selector |
| selector_off_fg | #8792A0 | "None" selector |
| success / success_soft | #1E7A4C / #E6F4EC | "0 parse failures", Δ = 0 |
| warning | #B26A00 | warnings |
| danger | #B42318 | delete, broken steps, errors |

Dark theme (the existing dark toggle maps to these): bg #0F1419, surface #161C23, border #2A3440, grid #1E2630, zebra #131920, ink #E6EAF0, ink2 #B7C2CF, muted #93A0AF, accent #E8A33D, accent_soft #2A2214, accent_dark #F2C27A, summary_bg #2A2214, summary_fg #F2C27A, summary_divider #3A2F1C, success #6BD39A, success_soft #15301F, danger #F97066.

Layout (top to bottom): title bar · toolbar (groups: Open/Stack/Batch | Filter/Sort/Columns/Calc column/Pivot | Banking ▾ | Export ▾ ; command search box with "Ctrl K" on the right) · tab strip (active tab white with a 2 px accent top line) · body = [filter chip bar + pagination, then the table] and a right panel with two tabs "Steps · N" and "Column" · status bar (rows · control total · Δ vs source · parse-failure chip · scale).

Table rules:
- Header row, selector row, summary band: never wrap. Each column's minimum width = the widest of its header text, its selector label, its summary value and a sample of its cell values (first 100 visible rows), plus padding, capped at 420 px; text beyond the cap ends in "…" and shows the full text on hover.
- Users can drag column edges to resize; the width is remembered per tab.
- Numbers right-aligned in JetBrains Mono with thousands separators; text left-aligned in Source Sans 3; the selected column's header gets accent_soft background and a 2 px accent underline.
- Every count shown in the UI (row counts, "1–100 of 20,391", status bar) uses thousands separators.
</design_tokens>

<tasks>

<group name="A: architecture and speed" session="1">

<task id="R1" name="Split main.rs by screen, no behaviour change">
- Move each modal and panel out of `main.rs` into `src/ui/<name>.rs` with its own state struct (schema, filter, columns, calc column, conditional column, pivot/WAVG, buckets, rate shock, duration, FTP, provisioning, limits, audit, normaliser, join, stack, reconcile, batch, export, SQL, steps). `main.rs` keeps the app struct, the menu/toolbar wiring and the frame loop.
- Add `src/ui/theme.rs` holding every token in `<design_tokens>` (light and dark) and the font loading. No colour or size literal may remain in UI code outside `theme.rs`.
- This task changes structure only. The app must look and behave exactly as before; all tests pass unchanged.
- Acceptance: `grep -nE '#[0-9A-Fa-f]{6}|Color32::from_rgb' src/ui src/main.rs` finds matches only in `theme.rs`; `main.rs` is under 800 lines; `cargo test` unchanged.
</task>

<task id="R2" name="Background worker with progress and cancel">
- One worker thread owns all Polars work: load, compile + collect of the visible page, control totals, parse-failure counts, summary row, inspector stats, step row counts, exports, batch. The UI sends jobs over a channel and polls results each frame (`ctx.request_repaint()` when a result arrives).
- Each job has an id; a newer job of the same kind for the same tab supersedes the older one (its result is dropped). Long jobs (load, export, batch) show a progress line in the status bar with a Cancel button; cancel stops at the next safe point and leaves no partial output file (write to a temp name, rename on success).
- While a job runs the table keeps showing the last good page, dimmed, with "Updating…" in the status bar.
- Headless mode runs jobs inline (no thread) with the same functions.
- Acceptance: unit tests on the job queue: superseded jobs are dropped; cancel removes the temp file; an error in a job reaches the status bar as text. Manual check: opening a large file and scrolling the menus while it loads does not freeze the window.
</task>

<task id="R3" name="Streaming exports and fewer passes">
- Parquet and CSV exports use `sink_parquet` / `sink_csv` when the plan supports streaming, and fall back to collect-then-write otherwise (record which path was used in the status message).
- The Phase 2 load does extra passes (Dr/Cr check, parse-failure counts). Combine them into one aggregation pass over the typed columns.
- Cache the source baseline control total keyed by (path, file size, modified time); recompute only when one changes.
- Acceptance: a test exports the same pipeline both ways and gets identical row count and control sum; a test shows the baseline is computed once for two tabs on the same unchanged file.
</task>

</group>

<group name="B: look and formats" session="2">

<task id="L1" name="Theme, fonts, icons and layout">
- Apply `<design_tokens>` and the layout to the main window, tab strip, toolbar, status bar and every modal (same header, buttons and error display everywhere; primary button = accent fill, white text).
- Replace every emoji/symbol icon with the icon font. Replace the menu bar's duplicated actions with the toolbar groups; keep the menu bar for File, View (theme, UI scale, panels) and Help only.
- The dark toggle switches between the light and dark token sets.
- Acceptance: a test renders the theme module's token table and checks every light token has a dark counterpart; manual check against `docs/design/Combined.dc.html` (the owner will compare by eye).
</task>

<task id="L2" name="Table rendering rules">
- Implement the table rules in `<design_tokens>`: no wrapping in header/selector/summary, minimum width from the four sources, resize by dragging, right-aligned mono numbers, separators on every count.
- Put the width calculation in a pure function `column_min_width(header, selector_label, summary_value, samples, font_metrics) -> f32` so it can be tested with fixed metrics.
- Acceptance: tests for `column_min_width` (the widest of the four wins; the 420 px cap applies; a date like "01-Sep-2026" never needs two lines at the computed width); a test that the row-count formatter gives "20,391" and "1–100 of 20,391".
</task>

<task id="L3" name="Thousands, Millions and Lakh formats">
- Add Thousands (K, ÷1,000), Millions (M, ÷1,000,000) and Lakh (L, ÷100,000) to the per-column formats and to View → Default monetary scale, next to Crore and Plain.
- Exports keep raw values in data files; Excel number formats and PDF text use the chosen scale and show it in the header (for example "Balance (BDT M)").
- Acceptance: 25,000,000 → "25,000.00 K", "25.00 M", "250.00 L", "2.50 Cr"; negatives keep the sign; the existing Crore tests still pass.
</task>

<task id="L4" name="Leading apostrophes and text-that-should-be-numbers">
- Number and date parsing strips one leading `'` (common in CBS exports that protect leading zeros). Text columns keep their values unchanged.
- Before an export, if a Text column has ≥ 95% of non-blank values that parse as numbers, show: "<col> is Text, but 98% of its values look like numbers. Set it to Number before exporting?" with Set to Number / Export as is.
- Acceptance: "'1,234.50" → 1234.5 in a Float64 column; "'0012" stays "'0012" in a Text column; the warning appears for a Text column of numbers and not for an ID column with leading zeros (those parse as numbers but are flagged as IDs by the Phase 1 tokeniser and are excluded).
</task>

<task id="L5" name="Housekeeping">
- One version number: read `CARGO_PKG_VERSION` everywhere (window title, about, macro files); bump Cargo.toml to 1.3.0 and the macOS Info.plist to match.
- Default export file names: replace characters Windows forbids (`<>:"/\|?*`) and trim to 120 characters.
- macOS: store `portable_data` next to the `.app` bundle (not inside it); if that folder is not writable, use `~/Library/Application Support/MicroParquet`. Windows and Linux keep it next to the executable. Show the active path in Help → About.
- Acceptance: tests for the file-name sanitiser and for the portable_data path chooser (with a fake non-writable directory).
</task>

</group>

<group name="C: filters, sort, summary, steps" session="3">

<task id="T1" name="Multi-condition filter, editable">
- New step `FilterGroup { conditions: Vec<FilterCondition>, join: And | Or, keep_matching: bool }` with `#[serde(default)]` fields. Operators: equals, not equals, >, ≥, <, ≤, between, contains, starts with, ends with, in list, is blank, is not blank. Use the Phase 1 type-aware comparison (text compares as text, dates as dates).
- The filter chip bar shows the active tab's filter conditions; × on a chip removes that condition; "+ Add condition" adds one to the existing filter step (creating the step if there is none). Clicking a chip opens the filter editor on that step. Old single `FilterRows` steps still load and are shown as a one-condition group.
- Acceptance: tests for AND and OR results on a small frame; between with dates; in list with text IDs that look numeric; a v1.2 macro with `FilterRows` loads and gives the same rows.
</task>

<task id="T2" name="Multi-level sort, editable">
- New step `SortRows { levels: Vec<SortLevel { column, descending }> }`; blanks always last; stable sort.
- Sort editor: add, remove, reorder levels; each level has Lowest first / Highest first. Header arrows show the level number and direction ("1 ↑", "2 ↓").
- Acceptance: tests for a 3-level sort (text asc, number desc, date asc) with ties and blanks.
</task>

<task id="T3" name="Summary row">
- Selector row above the summary band, one selector per column: None, Count, Count distinct, Sum, Average, Min, Max, Weighted average. Defaults: numbers → Sum, text → Count, dates → Min. Weighted average asks for its weight column (set in the Column inspector; the selector shows just "W.Avg").
- Values are computed on the whole result after all steps (not the visible page) as one aggregation job on the worker; they refresh after any step change. Weighted average uses the Phase 2 rule (null when weights mix signs, with a tooltip saying why).
- The summary configuration is saved with the tab and in macros (`#[serde(default)]`). Excel and PDF exports add it as a totals row in the summary_bg colour, labelled with each aggregation.
- Acceptance: tests for each aggregation on a frame with nulls; W.Avg mixed-sign → null; the exported xlsx has a totals row with the right values and labels.
</task>

<task id="T4" name="Steps panel: timeline and editing">
- Right panel "Steps · N" tab, drawn as a vertical timeline (dot, connecting line, title in bold, one-line detail, row count after the step in mono on the right). The selected step is highlighted (accent_soft) and shows buttons: Edit, Move up, Move down, Turn off/on (skip), Duplicate, Delete (danger colour, asks to confirm). Below the last step: "+ Insert step after <selected>".
- Row counts per step are computed lazily on the worker and cached until a step before them changes.
- After any change, re-validate every later step against the schema it would see (for example a column that no longer exists). An invalid step turns danger-coloured with the reason; the table shows the result up to the last valid step, and the status bar says which step is broken. Turned-off steps are skipped and shown dimmed.
- Undo/redo for step changes (Ctrl+Z / Ctrl+Y), at least 50 levels.
- Acceptance: pure-function tests for move, delete, duplicate, skip and undo on the steps list; a test that deleting a "Calc column" step marks a later filter on that column invalid with a reason naming the column.
</task>

</group>

<group name="D: inspector and command search" session="4">

<task id="T5" name="Column inspector">
- Right panel "Column" tab. Clicking a column header selects the column and switches to this tab.
- Shows: name, type, format and scale; rows, blanks, parse failures; for numbers sum, min, max, mean, distinct count and a 10-bin histogram; for dates min and max; for text distinct count and the top 5 values with counts. Computed on the worker over the whole result.
- Actions: Filter on this column…, Add as sort level, Summary (selector, including the weight column for W.Avg), Change type or format….
- Acceptance: tests for the stats function on a frame with nulls (numbers, dates, text); histogram bin counts sum to the non-null count.
</task>

<task id="T6" name="Command search (Ctrl+K)">
- Ctrl+K (or clicking the toolbar box) opens a centred search overlay. It lists every action (all toolbar and menu items, each banking tool, export types, theme toggle) and every column of the active tab ("Column: <name>" selects it and opens the inspector).
- Fuzzy match on words, recently used first, Up/Down to move, Enter to run, Esc to close. Actions not available in the current state are listed greyed with the reason.
- Acceptance: tests for the ranking function (exact word start beats a mid-word match; recently used breaks ties) and that every toolbar action is registered in the command list.
</task>

</group>

</tasks>

<constraints>
- Allowed new dependencies: an egui 0.29-compatible icon-font crate (or none, if the TTF is bundled directly). No other new crates and no upgrades of Polars, egui or eframe.
- Bundled fonts must include their licence files and be listed in a `THIRD_PARTY_NOTICES.md`.
- Every new field on a serialised type gets `#[serde(default)]`. Phase 1 and 2 macro, schema, preset and rule files must still load; add a test that loads one macro from each phase.
- No change to calculation results: existing tests keep their expected values.
- No `unwrap()`, `expect()` or index-panics on user-data paths or in UI code reacting to user input.
- Headless mode keeps working and keeps its output format.
- Change only what the tasks need; don't reformat untouched files (R1 moves code, so its diff is large by design: move, don't rewrite).
- If an egui 0.29 limitation prevents a design detail, do the closest thing egui supports and record it as a deviation.
- Test data is synthetic only.
</constraints>

<workflow>
- Use `grep`/`rg` to find code sites. Never run `cargo clean`. Use `cargo check` while editing; run the full `cargo test` before each commit.
- One commit per task, message `Phase 3 <ID>: <short summary>`. Push after each commit.
- Keep `docs/phase3-progress.md` updated after each task (status, commit, tests, deviations, questions). If a session stops or context is compacted, resume from it.
- Never stop to ask the owner mid-run: choose the option that changes the fewest existing results, record it in `<accepted_deviations>` in this file, and list it in the report.
- If a test fails three times for the same reason, mark the task BLOCKED in the progress file and continue with the next task.
- Open one PR per session to main, wait for the Tests check, fix failures, don't merge.
</workflow>

<report>
At the end of the session, reply in no more than 30 lines: a table (Task | Status | Tests added | Deviations), the final `cargo test` summary line, and questions for the owner. Add a short "What to look at by eye" list for the owner's visual check.
</report>

<accepted_deviations>
Recorded during the unattended Phase 3 run (each also appears in `docs/phase3-progress.md` and in the report).
- R1: `run_headless_cli` and `CliArgs` handling moved to `src/headless.rs` so `main.rs` is under 800 lines. No change to headless behaviour or output.
- R1: font loading is not in `theme.rs` yet. R1 must not change the look, and the font files arrive with L1; `theme.rs` gets `load_fonts` in L1. Until then `apply_ui_theme` uses the pre-Phase-3 values (named `OLD_*` and `WARN_*` constants), so R1 changes nothing by eye.
- R1: only colour literals moved into `theme.rs` (the acceptance grep). Widget width/height literals inside modals (`desired_width(520.0)` and similar) stay where they are until L1 restyles the modals.
- R1: `src/ui/prelude.rs` re-exports the imports every screen needs, so each screen file has one `use` line.
- R1: dark-theme tokens the spec does not list (accent_soft_border, selector_on_bg/border/fg, selector_off_fg, warning) were derived from the dark palette: #4A3A1C, #161C23, #3A4756, #F2C27A, #7B8794, #F0A93A. Owner to confirm.
- R2: the job queue is `src/jobs.rs` and the job bodies are `src/tab_jobs.rs` (library crate, so headless and tests use them). Job kinds Stats and StepCounts exist for T3–T5; nothing submits them yet.
- R2: file-open preview, the schema modal's "Re-read" and the stack modal's schema detection still run on the UI thread. They read the first lines of a file only (a few thousand rows at most). Loading the tab, refreshing it, applying a schema, exports, direct CSV→Parquet conversion and batch runs are all background jobs.
- R2: a pipeline that fails to build is a normal refresh result (the tab shows "—" and the reason), as before. A failed Load shows "Pipeline error: …"; a failed export shows "<label> failed: …" instead of the old per-export wording.
- R2: cancel in Polars-bound steps stops at the next safe point: before and after a write, between tabs (PDF), between partition files (split) and between files (batch). A single Parquet/CSV collect, or the Excel workbook build, cannot be interrupted while it runs. A cancelled batch logs a CANCELLED row; a cancelled split removes the files it wrote and writes no manifest.
- R2: `TabState` gained `id` and `slice_offset` (not serialised, so no serde attribute). A refresh that a newer one replaces hands its pending baseline recount and "Step added" note to the newer one, so a superseded job never loses either.
- R2: this container cannot run the GUI (libxkbcommon-x11 is missing and installing packages was refused), so the manual freeze check (open a large file while using menus) is for the owner. Headless output was checked by hand (all four export types) and `cargo test` covers the job functions.
- R3: Polars 0.43 has no "can this plan stream?" check, so the export tries `sink_parquet`/`sink_csv` and falls back to collect-then-write when Polars refuses before writing anything (it says "cannot run the whole query in a streaming order" or "not yet supported in standard engine"; both are recognised). Any other error fails the export. Split and batch exports use the same functions, so they stream too; only the single-file exports report which path was used.
- R3: the baseline cache key is (path, size, modified time) plus the schema and the control column, not just the first three: a schema change or another control column changes the number, and a stale baseline would be a silently wrong Δ. Joined and reconciled sources are not cached. With no control column the baseline needs no scan at all (it was always null).
- R3: the Dr/Cr check and the parse-failure counts now come from one cached scan per (file, size, modified time, schema), so opening a tab, refreshing it and applying a schema do not rescan an unchanged file. The pass itself is the same aggregation; on a cache miss it is one pass instead of two.
- L1: **Source Sans 3 is not bundled.** The font repositories could not be added to this session (the add_repo request was refused), so the three Source Sans 3 TTF files are missing. `theme::load_fonts` loads them from `assets/fonts/` next to the program (or the source tree) when present and otherwise keeps egui's default text font; all UI text styles and the semibold/bold families are wired, so dropping the files in is enough. JetBrains Mono Regular and SemiBold are bundled (copied from the system package `fonts-jetbrains-mono`, OFL text included). `THIRD_PARTY_NOTICES.md` says which file is still to be added.
- L1: icons use the `egui-phosphor` 0.7.3 crate (the allowed icon-font crate; the only new dependency). Emoji in UI strings are replaced by Phosphor glyphs (`\u{E…}` escapes); symbols that lower layers put in messages (warning, tick, cross, key) are mapped to glyphs by `theme::icons_in_text` when shown. A test checks that every escape and non-ASCII character in the UI sources has a glyph in the loaded fonts.
- L1: the title bar is a 36 px strip inside the OS window (logo, name, File / View / Help, active tab name, version); the window's minimise / maximise / close buttons stay the operating system's.
- L1: toolbar groups follow the spec; the spec's list has no home for Conditional column, Counterparty normaliser, SQL join, the SQL bar and chart toggles and Copy table, so they sit in a "More" menu in the second group. Reconcile and the dirty-data auditor are in Banking. The Sort button is added by T2. The command search box is drawn and takes clicks; its overlay is T6.
- L1: the dark theme's primary button uses the dark window colour for its text (white on amber is hard to read); the light theme is white on accent as specified. Heading (modal titles) is 17 px, the inspector title 20 px.
- L1: the Steps tab is still the plain step list and the Column tab is a stub (name and type); T4 and T5 fill them. The filter chip bar shows the existing single-condition filter steps; T1 extends it. The table grid itself is L2.
- L1: this container cannot open a window, so the layout was checked by compiling and by tests, not by eye (see the report's "What to look at by eye").
- L2: the column header shows the column name only (the type is in the hover text and, from T5, in the Column tab). The selector row and the summary band are drawn by T3; `column_min_width` already takes their texts, which are empty until then. Dates and ID-like columns are mono and left-aligned; numbers are mono and right-aligned; whole-number (Int64) cells get thousands separators unless the column looks like an ID. Column widths dragged by the user are remembered per tab through egui's table state (keyed by the tab id), so they reset when the number of columns changes.
- L2: counts in lib messages (row-count warning, join summary, parse-failure summary) and in the batch modal now use thousands separators; the numbers in the existing tests are all below 1,000, so no expected value changed.
- L3: Lakh already existed (per-column and global); this task added Thousand (K) and Million (M) next to it. In Excel, money columns are written as value ÷ divisor with the scale in the header ("Balance (BDT M)") instead of keeping the raw value under a scaling number format, because Excel's number formats can only scale by 1,000 and 1,000,000, not by 100,000 (Lakh) or 10,000,000 (Crore); one rule for all four is easier to explain. Rates, counts and durations are never scaled, plain-number settings divide nothing, and Parquet / CSV exports keep raw values. PDF cell text already used the scale; its header now names it.
- L4: "Set to Number" adds a visible pipeline step `ConvertToNumber` (listed in the Steps panel, saved in macros, translated to Python) instead of editing the file's schema, so it works for stacked, joined and pivoted tabs as well as single files. It reads the column with the Float64 rules (apostrophe, commas, brackets, currency; Dr/Cr values still need a sign convention and fail loudly). The check runs as a background job before every GUI export (Parquet, CSV, Excel, PDF, split); headless exports are unchanged. A value counts as a number when it parses to a value under those rules; text with Dr/Cr markers does not count. ID-like columns are excluded by name (Phase 1 tokeniser), as the spec says.
- L4: dates already ignored surrounding apostrophes; only amount parsing changed (and its Python mirror). A lone apostrophe is a blank cell.
- L5: there is no Info.plist in the repository: the macOS bundle's plist is written by `.github/workflows/main.yml`. It now takes the version from `Cargo.toml` (was a hard-coded 1.1.0), and the workflow no longer packs `portable_data` inside the `.app`. The version in macro files is a top-level `app_version` key written on save (not a field of `MacroRecipe`, so existing code and tests that build the struct are untouched); older files without it load as before. Default file names are cut at 120 characters including the extension, and trailing dots and spaces are removed because Windows drops them.
- L5: the new tests for R2–L5 live in one test binary, `tests/phase3/` (jobs, r3, l2, l3, l4, l5 as modules). Each integration test file links all of Polars (about 1.5 GB in a debug build), and the container's disk is small.
- T1: the multi-condition step has its own operator list (`CondOp`: equals, not equals, >, ≥, <, ≤, between, contains, starts with, ends with, in list, is blank, is not blank); old `FilterRows` steps are not rewritten and keep their exact behaviour, they are only *shown* as one-condition groups and become a group when edited. "Is blank" means null or text that is empty after trimming. A condition whose test is unknown (a null against a value) does not match, so "Keep matching" drops that row and "Delete matching" keeps it (the old single filter dropped it in both modes; it is unchanged). Contains / starts with / ends with are case-sensitive, as the old filter. Python export treats values as numbers when they parse as one, else as text, as the single filter does, so date comparisons in the script compare text.
- T1: the toolbar Filter button edits the tab's first filter (or starts one); "+ Add condition" adds to that filter; a chip click opens the editor on its step; × on the last condition removes the step.
- T2: text is sorted by its characters (Polars' order: capital letters before small letters, so "DHK" sorts before "ctg"); the spec does not ask for case-insensitive order and changing it would be a choice for the owner. Blank text (null, empty or spaces only) is "blank" and sorts last in either direction. A tab's sort is the last sort step; the toolbar Sort button edits it, and the header context menu (right-click) sorts by one column, adds a level or removes the sort. Paging a sorted tab sorts the whole result each time (Polars does not stream a sort).
</accepted_deviations>
