# MicroParquet — Phase 6 specification (Usability, reporting and polish)

Sources for this phase:
- The agreed roadmap (Phase 6 = reporting and polish).
- The owner's requests of 19 Nov 2026: schema renames, side panel, SQL editor, custom sort lists, landing page, default zoom, filter value picker.
- The owner's report that the bar chart is unreadable.

Sessions are told which task IDs to do. Read only those task sections, plus `<conventions>`, `<constraints>`, `<guide_notes>`, `<workflow>` and `<report>`.

<goal>
Phase 6 is done when:
1. Tasks U1–U7 and P1–P6 are implemented, and `cargo build --release` and `cargo test` pass.
2. Every acceptance item is an automated assertion. Every UI item also has a headless egui render test at 1219×686 pt (105%) and 1600×900 pt (80%) on real response rects, which checks two things:
   - nothing overflows or is cut with an ellipsis where the full value is needed
   - every clickable control works when its real rect is clicked
3. Every new or changed tool has guide notes in `docs/guide/` following `<guide_notes>`, and `docs/guide/README.md` lists them.
4. Results of existing steps and tools are unchanged except where a task says so, and Phase 1–5 macros, schemas, presets and rule files still load.
</goal>

<guide_notes>
Same rules as Phase 5. Write one file per tool, `docs/guide/<tool-id>.md`, in plain English for a bank ALM, treasury or credit-risk officer. Use these ten sections in this order:
1. What it does
2. When to use it
3. Before you start
4. Columns you need (a table: Column role | Required? | Type | Example values | Notes)
5. Step by step, using the UI's exact labels and the Ctrl+K command name
6. What you get
7. Check your result
8. Worked example, copied from the tool's tests, with the test named under the table
9. Common messages and fixes
10. Limits and assumptions

Keep each file under about two pages. When a task changes a tool, update its guide file in the same commit.
</guide_notes>

<conventions>
- **Ordering:** wherever the app orders text values (sort, pivot rows and columns, chart categories), the available orders are Natural (Phase 3 natural, case-insensitive), Custom list (U3), Tenor (U3) and By value. Blanks always go last.
- **Amounts:** amounts in charts and reports use the column's Phase 3 display format (BDT, K, M, Lakh, Crore). Nothing is divided by a fixed 10,000,000 unless the format says Crore.
- **Settings:** user settings that should survive a restart (side-panel width, chart choices) live in `portable_data/settings.json`. Every field has a serde default, and a missing or damaged file falls back to defaults with a status message.
</conventions>

<tasks>

<task id="U1" name="Rename columns in Schema & Ingestion; auto-apply saved schemas">
- The schema grid gets a "Rename to" column. Blank means keep the file's name.
- **Bad names:** a name that is empty after trimming, or that duplicates another final name, is shown in red with the reason. "Apply" and "Save" stay disabled until it's fixed.
- **When renames apply:** after typing, Dr/Cr and masking, before any step. Steps, filters, macros, the column inspector and exports all use the new names.
  - The inspector shows "From file: <original>".
  - The Python export includes the rename mapping.
- **Saving:** a saved `.schema.json` keeps the renames (`rename_to`, `#[serde(default)]`). The Save button's default folder is `portable_data/schemas/`.
- **Auto-apply:** when a file opens, the app compares its header names (trimmed, case-insensitive, any order) with the original column names of every template in `portable_data/schemas/`.
  - **Exactly one match:** the Schema dialog opens with that template applied and a line "Applied saved schema '<name>' (all N columns matched)" with an Undo button.
  - **Several matches:** a picker.
  - **No match:** today's behaviour.
  - A template ticked "Apply without asking" opens matching files directly, with a status message naming the template.
- **Stack and Batch:** with "Apply one schema to every file", renames apply there too.
- **Changing a rename on an open tab:** if existing steps use a name that would disappear, a warning lists them before the change is applied. Steps are never broken silently.
- **Acceptance:**
  - A CSV with headers ACNO, CRIRTE, BAL renamed to Account_No, Int_Rate, Balance, then saved.
  - A second CSV with headers BAL, ACNO, CRIRTE auto-applies it, and a filter on Int_Rate works.
  - A file with one extra column does not match.
  - Two final names "Balance" are refused.
  - Stack of both files with the template gives the renamed columns.
  - The Python export contains the mapping.
  - Changing Int_Rate back warns, naming the filter step.
  - An old schema file without `rename_to` loads unchanged.
</task>

<task id="U2" name="Resizable, scrollable side panel; SQL editor in the panel">
- **Width:** the right panel can be resized by dragging its left edge.
  - Minimum is today's width (`RIGHT_PANEL_W`).
  - Maximum is half the window width.
  - The width is kept in `settings.json`.
  - Double-clicking the edge resets it.
- **Scrolling:** each panel tab scrolls vertically with a visible scroll bar whenever its content is taller than the panel, so nothing is cut off at the screen's height.
- **SQL tab:** a third panel tab "SQL" replaces the one-line SQL bar.
  - A multi-line monospace editor, at least 8 lines tall, that grows with the panel.
  - The hint "Table name: self".
  - **Run:** buttons "Apply here" (Ctrl+Enter) and "New tab" (Ctrl+Shift+Enter).
  - **Errors:** the last error is shown under the editor in a scrollable box.
  - **History:** a dropdown with this tab's last 10 queries.
  - **Opening it:** View ▸ SQL and the Ctrl+K command "SQL editor" open the panel on this tab. Existing `sql_scratchpad` text is kept.
- **Tab labels:** "Steps · N", "Column" and "SQL" fit in one row at the minimum width.
- **Acceptance:**
  - Dragging to 500 pt gives 500.
  - Dragging to more than half the window is clamped.
  - The width survives a restart (save, then load `settings.json`).
  - A tab with 30 steps scrolls to the last step, which can then be clicked.
  - A 12-line query runs with Ctrl+Enter.
  - A bad query shows its error.
  - History holds 10 entries, with the oldest dropped.
  - Render and click tests at both scales and at minimum and maximum width.
</task>

<task id="U3" name="Custom sort lists and Tenor order">
- **Lists:** saved as `portable_data/sort_lists/<name>.json`, with name, values (in order), case_insensitive (default true) and apply_to_columns (column names the list is suggested for).
- **Manager:** View ▸ "Custom sort lists" (Ctrl+K "Custom sort lists") can:
  - create, rename and delete lists
  - edit values (one per line; move up and down)
  - "Fill from column", which loads the active tab's distinct values in natural order for reordering
- **Built-in "Tenor" order:** parses a number plus a unit (D/day/days = 1, W/wk/week = 7, M/mon/month = 30, Y/yr/year = 365 days, case-insensitive, with or without a space), plus O/N, ON and Overnight = 1 day.
  - Values are ordered by days, with ties keeping natural order.
  - Values that don't parse go after the parsed ones, in natural order, and are counted in the status message.
- **Where lists are used:**
  - **Sort:** each Sort level gets an "Order" choice: A→Z, Z→A, a custom list, or Tenor. With a list, values not in it come after the listed ones, in natural order. Z→A with a list reverses the list.
  - **Pivot:** the field menu for Rows and Columns gets "Order: Natural / <list> / Tenor / By value". A list whose apply_to_columns names the field is pre-selected.
  - **Chart:** category order offers the same choices (U5).
- **Storage:** `SortLevel` and the pivot spec get `#[serde(default)]` order fields, so old macros replay unchanged.
- **Acceptance:**
  - Tenor sorts "3y, 1m, 400D, 2m, 1y, 100D, 6m, 3m, 2y, 200D" into "1m, 2m, 3m, 100D, 6m, 200D, 1y, 400D, 2y, 3y".
  - "abc" goes last, counted as 1.
  - A custom list [High, Medium, Low] sorts Low, High, Medium, (blank), Other into High, Medium, Low, Other, (blank).
  - Z→A gives Low, Medium, High, Other, (blank).
  - A pivot on "Term Period" with the list pre-selected shows rows in list order.
  - A Phase 4 sort macro replays identically.
</task>

<task id="U4" name="Landing page, title bar and About">
- **Window title:** "MicroParquet v<version>", plus " — <active file name>" when a tab is open. The subtitle "— Portable CSV/Parquet, ALM & Loans Analytics Workstation" is removed.
- **Landing page:** remove the heading "MicroParquet x.y.z: Portable CSV/Parquet, ALM & Loans Analytics Workstation" and the line "Out-of-Core Polars Engine • Multi-Tab Views • Custom Maturity Buckets • Per-Metric Formats". Keep the three buttons, centred vertically.
- **About:** Help ▸ "About MicroParquet" opens a dialog with:
  - version and build date
  - a one-line description
  - the main capabilities (from the removed text)
  - the `portable_data` folder path, with an "Open folder" button
  - the PII key fingerprint
  - the font licences (OFL)
- **Version:** bump Cargo version to 1.6.0.
- **Acceptance:** the title strings for no tab and an open tab; the landing render test has no heading text, and its buttons are clickable; the About dialog fits at both scales.
</task>

<task id="U5" name="Chart rework (replaces the 'ALCO Visual Bar Chart')">
- **The problem:** today the drawer plots the first 25 rows of the visible page, one bar per row. The x-axis shows row numbers, not categories, and every value is divided by 10,000,000. Types other than Float64, Float32, Int64 and UInt32 silently become 0.
- **Replacement:** a "Chart" drawer with these controls:
  - Category (X)
  - optional Series (grouped bars, at most 8, with the rest as "Other")
  - Value column
  - Aggregation: Sum, Count, Average, W.Avg (by weight column), Min, Max
  - Type: Bar or Line
  - Order, per `<conventions>`
  - Top N categories (default 30, with the rest as "Other")
  - Scale: follows the value column's display format, or can be overridden
- **Calculation:** done on the worker over the whole tab after its steps, never just the page. A non-numeric value column is refused with the column's type. Integer and decimal types are read correctly.
- **Axes and labels:**
  - Category labels appear on the x-axis; long labels are shortened, with the full label in the tooltip.
  - Each bar or point has a tooltip with the category, series, value and row count.
  - The title reads like "Sum of Balance by Term Period: all 244,250 rows (Crore)".
- **"Open chart data as tab":** puts the aggregated table in a new tab, so it can be exported.
- **Saving choices:** chart choices are kept per tab in `settings.json`.
- **Acceptance:**
  - 250 synthetic rows across three pages, with Term Period in 5 tenors: the sums equal a group-by over all 250 rows.
  - With Tenor order the categories run 1m < 3m < 100D < 6m < 1y.
  - An Int32 value column and a Decimal value column give correct sums.
  - Count needs no numeric column.
  - W.Avg matches the Phase 2 rule.
  - Top N = 3 gives 3 categories plus "Other", with the totals preserved.
  - A text value column with Sum is refused.
  - Render test: the x-axis labels are the category names.
</task>

<task id="U6" name="Default zoom setting">
- **Today:** the app always starts at `UI_ZOOM_DEFAULT` (105%), and zoom changes are lost on restart.
- **Setting:** View ▸ "Zoom" shows the current % with Zoom in, Zoom out, "Reset to default" and "Set current zoom as default". A "Default zoom" choice runs from 50% to 200% in 5% steps.
  - The default is kept in `settings.json` and used at start-up, and the Ctrl+K commands do the same.
  - With no setting, the start-up zoom stays 105%.
- **Keys:** Ctrl+= zooms in, Ctrl+- zooms out, and Ctrl+0 resets to the default.
- **Status bar:** shows the zoom % at its right end. Clicking it opens the Zoom menu.
- **Acceptance:**
  - Setting a 90% default writes 0.90. A new app state built from that file starts at 0.90.
  - Ctrl+0 after zooming to 120% returns to 90%.
  - A file value of 5.0 is clamped to 2.00, with a status message.
  - A missing file gives 1.05.
  - The status-bar zoom label is clickable at both scales.
</task>

<task id="U7" name="Pick filter values from a list">
- **Today:** the filter needs exact typed values.
- **Value picker:** for the operators =, ≠, "In list" and a new "Not in list", the value box becomes a picker, as in the pivot Filters area.
  - It lists the column's distinct values with row counts, in natural order (or the column's custom list, U3).
  - It's calculated on the worker over the whole tab as it stands before this filter, never just the page.
  - It has a search box, "Select all", "Clear", and a "(blank)" entry.
  - A summary reads like "3 of 42 selected".
- **Operator from the selection:** one value selected stores =, and several store "In list" ("Not in list" when ≠ was chosen). Selecting "(blank)" adds an is-blank condition joined with OR.
- **Storage:** values go in a new `values: Vec<String>` field (`#[serde(default)]`), so values containing commas (e.g. "Khulna, West") work.
  - Old filters with a comma-separated string still replay identically.
  - Dates and numbers are matched as typed values, not text.
- **Large columns:** above 1,000 distinct values, the list shows the 1,000 most frequent, and the search box queries all values on the worker. A note says "Showing 1,000 of N; search to find others".
- **Conditional column:** rules using =, ≠ or "In list" get the same picker.
- **Acceptance:**
  - A 250-row tab with Zone values including "DHAKA NORTH", "WEST-1 ZONE", "Khulna, West" and blanks, spread over three pages: the picker lists every distinct value with correct counts.
  - Selecting two values gives the matching rows.
  - "Khulna, West" matches as one value.
  - "(blank)" adds blank rows.
  - "Not in list" excludes the selected values.
  - A 2,000-distinct-value column shows the note, and search finds a value outside the top 1,000.
  - A Phase 3 filter macro with a comma-separated "In list" value replays identically.
  - Click tests on the checkboxes at both scales.
</task>

<task id="P1" name="Saved tool runs (prerequisite for report packs)">
- **Configs:** every tool that makes a result tab gets a serialisable config (`ToolRun` enum, `#[serde(default)]` on every field) holding all its dialog inputs: cutoff, column roles, files used, options and Tier 1. The tools are:
  - Repricing gap, Gap ladder, IRRBB cash-flow ladder, IRRBB six scenarios
  - CL migration, CL-1 summary, LCR/NSFR, IFRS 9 ECL summary
  - Period reconcile, Join, Stack, Pivot tab
- **Tab references:** source tabs are referred to by a source name (the tab's file name or input slot), never by tab index.
- **Result tabs:** each one's menu gets "Re-run with changes" (opens the dialog pre-filled) and "Save run…", which writes `portable_data/runs/<name>.json`.
- **Acceptance:** for each tool, run → save → reload → re-run on the same inputs gives an identical result tab. The IRRBB footer (Phase 5 D10) is reproduced from the saved run alone.
</task>

<task id="P2" name="Report packs">
- **A pack** is `portable_data/packs/<name>.json` with:
  - **inputs[]:** slot name, an optional schema template (U1) and optional steps (a macro), applied when the slot's file is loaded
  - **items[]:** title, sheet name, and one of:
    - the steps output of a slot
    - a pivot on a slot
    - a saved tool run (P1) whose sources point to slots
  - **output:** Excel (always) and an optional PDF
- **Pack builder:** Banking ▸ "Report packs" (Ctrl+K "Report packs").
  - "Add input slot from open tab" captures that tab's schema and steps.
  - "Add item from result tab" turns its tab sources into slot references.
  - Items can be reordered and renamed.
- **Run pack:** pick a file per slot and the cutoff date. The pack runs on the worker with progress per item. One workbook comes out:
  - a "Cover" sheet listing the pack name, run time, app version, cutoff, and each input (slot, file name, rows, blake3 hash of the file)
  - every rule, curve, profile and parameter file used, with any sample flags
  - then one sheet per item, using the Phase 3/4 export rules
- **Failure:** any item that fails stops the run with the item and the reason, and no partial workbook is written.
- **Headless:** `--pack <name> --slot <slot>=<file> … --cutoff YYYY-MM-DD --export-xlsx <file>` runs the same thing.
- **Acceptance:**
  - A pack with slots Loans and Deposits and items Gap ladder, CL-1 summary and a pivot gives a workbook with Cover plus 3 sheets; the cover hashes match the files.
  - Running it twice on the same files gives identical sheets (apart from the run time).
  - A missing slot file is refused, naming the slot.
  - A file missing a needed column is refused, listing the columns.
  - Headless gives the same workbook as the UI.
</task>

<task id="P3" name="Month-on-month variance and commentary">
- **New tool:** Banking ▸ Treasury & ALM ▸ "MoM variance". Inputs:
  - the previous-month and current-month tabs
  - key columns (one or two, e.g. Product, Segment)
  - balance column
  - optional rate column (W.Avg by balance)
  - optional count
- **Output tab, per key:**
  - Prev balance, Curr balance, Change, Change %
  - Prev rate, Curr rate, Rate change (bp)
  - Annualised interest: Prev, Curr, Change
  - Interest change split into Volume effect = ΔBalance × prev rate, Rate effect = prev balance × Δrate, and Mix effect = ΔBalance × Δrate
  - Keys present in only one month are flagged New or Closed.
- **Commentary:**
  - **Materiality:** a key is material when |Change| ≥ the threshold amount or |Change %| ≥ the threshold %. Both thresholds are typed in the dialog and stored in the step, with defaults 50 Cr and 5%.
  - Commentary lines are generated for material keys only, ordered by |Change|, from fixed sentence templates (no AI), with amounts in the balance column's display format. For example: "TD rose by 100.00 Cr (10.00%) to 1,100.00 Cr; rate up 50 bp to 8.50%." and "SB fell by 50.00 Cr (10.00%) to 450.00 Cr; rate unchanged at 3.00%."
  - Lines also note New and Closed keys, and give a total line.
  - The commentary appears in a copyable text box and on its own sheet in exports and packs.
- **Acceptance:** prev TD 1,000 Cr at 8.00% and SB 500 Cr at 3.00%; curr TD 1,100 Cr at 8.50% and SB 450 Cr at 3.00%.
  - **TD:** change +100.00 (10.00%), +50 bp; interest 80.00 → 93.50, change +13.50 = volume 8.00 + rate 5.00 + mix 0.50.
  - **SB:** change −50.00 (−10.00%); interest change −1.50 = volume −1.50.
  - The two commentary lines above appear exactly, TD first.
  - A key only in the current month shows "New".
  - With a threshold of 200 Cr and 20%, there are no lines except the total.
</task>

<task id="P4" name="Data-quality scorecard">
- **New tool:** Data ▸ "Data-quality scorecard", building on the Phase 2 hygiene audit. Rules live in `portable_data/dq_rules/<name>.json`.
- **Rule kinds:**
  - completeness (column not blank)
  - uniqueness (key column(s))
  - type validity (parse failures)
  - range (min/max)
  - consistency (column A ≥/≤/= column B, e.g. Maturity ≥ Open date)
  - allowed values (a list or a code map)
  - control total (column sum vs a typed GL figure, with a tolerance %)
- **Rule results:** pass % = passing rows ÷ rows checked. Rows checked exclude blanks for every kind except completeness. For uniqueness, every row whose key appears more than once fails.
- **Control total:** shown as Pass or Fail with the difference, outside the average.
- **Overall score:** the simple average of the rule pass %s. RAG thresholds come from the rules file (sample: Green ≥ 98, Amber ≥ 95, otherwise Red).
- **Failing rows:** each failing rule lists its first 20 failing rows, plus an "Open failing rows as tab" button.
- **Output:** a scorecard tab, exportable and usable as a pack item. The rule set can be saved from the dialog.
- **Acceptance:** 10 rows with one blank Account_No, one pair of duplicate Account_No values, two Rate values that fail to parse, one Rate of 45 (range 0–30) and one Maturity before its Open date.
  - completeness 90.00%, uniqueness 77.78%, type 80.00%, range 87.50%, consistency 90.00%
  - overall 85.06%, Red
  - control total: 1,000 vs GL 1,005 at 0.1% tolerance is a Fail with difference −5.00
</task>

<task id="P5" name="Islamic window: Mudaraba profit distribution (weightage / PSR view)">
- **New tool:** Banking ▸ "Islamic window" ▸ "Mudaraba profit distribution".
- **Its job is calculation only:** the method, weightages and PSR must be the bank's Shariah Supervisory Committee-approved ones. Every parameter comes from `portable_data/islamic/psr_<YYYY-MM-DD>.json` (shipped as a sample), and the guide says so plainly.
- **Inputs:**
  - a deposit tab (product code, average balance for the period, or daily balances reduced to the average)
  - the period start and end dates (days = end − start + 1)
  - investment income of the period
  - other investable funds (bank's own and non-Mudaraba)
  - the CRR deduction %
  - the depositors' PSR %
  - weightage per product (from the file; unmapped products stop the run and are listed)
- **Calculation:**
  - investable Mudaraba = Σ avg × (1 − CRR)
  - pool share = investable Mudaraba ÷ (investable Mudaraba + other investable)
  - pool income = income × pool share
  - depositors' share = pool income × PSR; Mudarib share = the rest
  - weighted balance = avg × weightage
  - product profit = depositors' share × weighted ÷ Σ weighted
  - indicative rate % p.a. = product profit ÷ avg × 365 ÷ days × 100
- **Output:** a tab with each product's figures, plus a footer showing the pool share, pool income, depositors' and Mudarib shares, and the parameter file used. The "Sample parameters" banner shows while a sample file is in use.
- **Acceptance:** MSND avg 200 (w 0.4), MSB avg 300 (w 0.6), MTDR-1Y avg 500 (w 1.0); income 12.00; CRR 4%; other investable 240; PSR 70%; 30 days.
  - investable 960.00, pool share 80.00%, pool income 9.60, depositors 6.72, Mudarib 2.88
  - weighted 80, 180, 500 (Σ 760)
  - profits 0.707368, 1.591579, 4.421053
  - rates 4.3032%, 6.4547%, 10.7579%
  - an unmapped product is refused and named
</task>

<task id="P6" name="Liquidity stress option and build workflow">
- **Gap ladder stress:** a new option "Stress: early withdrawal (TDRR)". For each term-deposit product in the selected behaviour profile, TDRR × balance moves from its maturity bucket to the first bucket. The output title gets "(stressed)". With the option off, results are unchanged.
- **Build workflow** (`.github/workflows/main.yml`):
  - Remove step "3b. Run Calculation Tests"; the Tests workflow already runs them.
  - Add a `workflow_dispatch` input "platforms" (windows / macos / both, default windows), so a manual build makes only what is chosen. Tag builds still make both.
- **Acceptance:**
  - A term deposit of 1,000 in the 1–3m bucket with TDRR 10% gives 100 in the first bucket and 900 in 1–3m when stressed, and 1,000 in 1–3m when not.
  - The workflow file validates (actionlint, if available; otherwise a YAML parse test).
</task>

</tasks>

<constraints>
- No new crates or upgrades. Use the existing `blake3`, `rust_xlsxwriter`, `printpdf` and `serde_json`.
- Every new field on a serialised type gets `#[serde(default)]`. Old macros, schemas, presets, rule, curve, beta, behaviour and policy files load and replay unchanged; add fixtures for Sort and Pivot with the new order fields unset.
- Bank or regulatory parameters (PSR, weightages, DQ thresholds, materiality defaults kept in files) live only in `portable_data` files. Samples are marked `"sample": true` and show the banner.
- No `unwrap()`, `expect()` or index panics on user-data paths or UI input. Bad input stops with a message a banker can act on.
- UI follows `docs/design/` and the Phase 3 theme tokens. Every dialog uses the shared helper and fits 1280×720 at 80% and 105%. No text wraps in header, selector or summary rows.
- Test data is synthetic only. The repo is public: never commit anything from `portable_data`.
</constraints>

<workflow>
- Never run `cargo clean`. Run `cargo check` while editing and the full `cargo test` before each commit.
- Make one commit per task (`Phase 6 <ID>: …`) and push after each. Update the guide files and `docs/guide/README.md` in the same commit.
- Keep `docs/phase6-progress.md` updated, and resume from it after a stop or compaction.
- Never stop to ask the owner mid-run. Take the option that changes the fewest existing results, record it in `<accepted_deviations>`, and list it in the report.
- If blocked three times on the same failure, mark the task BLOCKED and continue. Dependencies: P2 needs P1 and uses U1 templates; U5 and U7 use U3 orders; U6 and U2 share `settings.json`. The rest are independent.
- One PR from `phase6` to main. Wait for the Tests check, fix failures, and don't merge.
</workflow>

<report>
Reply in no more than 30 lines: a table (Task | Status | Tests added | Deviations), the final `cargo test` summary, questions for the owner, and a "look at by eye" list.
</report>

<accepted_deviations>
U3 (details in docs/phase6-progress.md):
- The sort step and the pivot keep a copy of the list they use (name, values, case flag), so macros replay the same order whatever happens to the list file.
- The Sort dialog keeps "Lowest first / Highest first" and adds an "Order" choice (Natural, each list, Tenor); "Highest first" reverses a list.
- In the pivot, "By value" is the existing sort by the first value field (Rows only).
- Values not in a list or not a tenor follow the others in natural A→Z order, also for Z→A.
- The count of values that are not tenors is taken from the first 2,000 distinct values.
- The fixture `phase6_sort_pivot_unset` was written by hand in the Phase 5 form.
U1:
- A blank "Rename to" keeps the file's name; "empty after trimming" is reported for a file column with no name and no rename.
- (Corrected by the owner after session A) a Parquet file that matches a saved schema behaves like a CSV: the Parquet Schema dialog opens with the template applied and the "Applied saved schema…" line with Undo, unless the template is ticked "Apply without asking".
- The warning lists steps the worker found would break, before the tab changes; "Apply anyway" keeps them as broken steps.
- Parse failures, masked-column names and the inspector use the final names; "Convert to Parquet…" is disabled with a bad name.
U2:
- The panel's resize handle is our own strip at its left edge; the panel is drawn at exactly the kept width.
- A query is checked on the worker first; a wrong one adds no step and its message stays under the editor (also for "New tab").
- The SQL bar is replaced by the SQL tab; View ▸ SQL and Ctrl+K "SQL editor" open it (needs an open tab).
- Each panel tab scrolls with an always-visible solid bar (about 10 pt narrower content); a 4 pt overflow of the Steps tab's three buttons was fixed.
U6:
- The live zoom runs 50%–200% (was 80%–150%); Zoom in / out still step by 10%, kept on 5% values.
- The status-bar zoom label is the right-most item and is left out of a bar narrower than 900 pt; egui's own Ctrl+= / Ctrl+- zoom is switched off.
U4:
- The build date comes from a std-only `build.rs`; the title names the tab's first file (or the tab's title for a tool's result). Two Phase 3 version assertions read the Cargo version instead of "1.3.0".
U5:
- Top N keeps N categories plus "Other"; series: at most 8 drawn in all (7 plus "Other"); both are ranked by aggregated value and "Other" is recalculated from its rows.
- Count ignores the value column; choices are kept under the tab's first file name; the old bar chart is replaced by View ▸ Chart.
Owner answers on session A: live zoom 50–200% accepted (U6); "(blank)" as the `or_blank` flag on the filter condition accepted (U7); chart series 8 in total, 7 plus "Other", accepted (U5).
U7:
- "(blank)" is the flag `or_blank` of the condition (same rows as an extra is-blank condition joined with OR, without disturbing the group's join).
- No "(blank)" entry in the conditional column's picker (blanks have their own result). The picker is an inline panel under the row. "Select all" selects the listed values.
P1 (details in docs/phase6-progress.md D28–D31):
- A run keeps file names and typed inputs, not file contents; source tabs by tab title (or file name); a result tab with steps of its own is refused.
- "Re-run with changes" opens the tool's dialog pre-filled; File ▸ "Saved tool runs" (Ctrl+K) lists `portable_data/runs`.
- The period reconcile table is sorted by key so a re-run is identical (row order only).
P2 (details in docs/phase6-progress.md D32–D36):
- Sheets are numbered by the exporter ("1_Cover", "2_Gap"); the Cover is one text table including the parameter files with sample flags.
- A slot with unconvertible values or a missing column is refused; the pack cutoff replaces the cutoff of the tools that have one; Banking gets a last heading "Reports".
P3 (details in docs/phase6-progress.md D37–D40):
- Two tabs (variance and commentary); thresholds kept in the tab source and the saved run; rows with a balance but no rate, and blank keys, stop the run; New/Closed keys are always in the commentary.
P4 (details in docs/phase6-progress.md D41–D44):
- The tool is in the Transform menu (there is no Data menu); the type rule shows examples, not rows; rule sets are `dq_rules/<name>.json` with a sample written on first use.
</accepted_deviations>
