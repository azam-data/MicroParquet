# MicroParquet — Phase 9 specification (Model)

Sources for this phase:
- The roadmap's Phase 9 row: links between tabs (star schema only), a shared calendar on the Phase 8 calendar core, time intelligence (MTD, QTD, YTD, prior period, same period last year) and stock/flow rules (closing and average daily balance for balances, sums for flows).
- The owner's answers of 24 Nov 2026:
  - Lookups he will link: branch master, product master, GL map, customer or segment master, rate or FTP table.
  - Some keys need two columns (wing + product code, branch + GL). Duplicate keys must be raised and listed separately, so they can be fixed at source.
  - Data: a daily trial balance at GL level; account data mostly as month-end snapshots. Each arrives as one long file with a date column.
  - Days with no data (Friday, Saturday, holidays): the last balance is carried forward over every calendar day.
  - Month-end data only: the average balance is the average of the month-ends, labelled as that.
  - Reports: ALCO pack, deposit mix, NIM / yield / cost of funds, loan portfolio, ADR, board memo (probably all).
  - Ratios (yield, cost of funds, NIM) are in this phase, with a day count of Actual/365, Actual/360 or Actual/Actual.
  - Prior period and same period last year when the current period is partial: a choice, default **the same days** (1–24 Nov 2025 against 1–24 Nov 2026).
  - As-of date: the latest date in the data by default; the owner can set another, saved in the project.
- The owner's requests of 24 Nov 2026, done first because they don't depend on the model: P1 (pop-out pivot builder) and P2 (Nested pivot layout, Excel's Compact form).
- Not in this phase:
  - many-to-many links, a lookup of a lookup (snowflake), filters from data to lookup (both directions)
  - one pivot or chart mixing measures of two data tabs
  - a formula language for measures (measures are built from the choices in M3–M5)
  - working-day or holiday calendars (every calendar day counts, as the owner decided)
  - collapse and expand buttons in a Nested pivot
  - Python export of links, Calendar fields and measures (refused in words, see `<conventions>`)

Sessions are told which task IDs to do. Read only those task sections, plus `<architecture>`, `<conventions>`, `<constraints>`, `<guide_notes>`, `<workflow>`, `<report>` and `<accepted_deviations>`.

<goal>
Phase 9 is done when:
1. Tasks B0, P1, P2 and M0–M6 are implemented, and `cargo build --release` and `cargo test` pass.
2. Every acceptance item is an automated assertion. Every UI item has a headless egui render test at 1219×686, 1600×900 and 1291×700 pt, asserting that:
   - nothing overflows or is clipped
   - no two labels overlap
   - every control works when its real rect is clicked
3. Existing results are unchanged: every macro, pack, saved run, saved chart, settings file and project from Phase 8 loads and gives the same rows in the same order. B0 builds the baseline from the v1.8.0 code, and the Phase 7 replay test still passes.
4. Each new tool has a guide note (`<guide_notes>`), and `docs/guide/README.md` lists them.
</goal>

<architecture>
**Pivot builder state (P1).** The pivot dialog's state (fields, values, filters, layout, preview) is one value, held once. The docked dialog and the pop-out window are two ways of drawing it, never both at once. The pop-out uses the same multi-viewport mechanism as the Phase 7 chart pop-out.

**Nested layout (P2).** `PivotSpec.layout` chooses how a pivot is *shown*. The pivot step's result, the flat table, is unchanged. Later steps, charts, CSV/Parquet exports, macros and the Python export see the flat table. A Nested view (`pivot_view::NestedView`: rows with a level, a label, values, and flags for subtotal, grand total and blank line) is built on the worker when the pivot runs, from the pivot's **source rows**, and kept with the tab as a display cache. The screen, the Excel export and the PDF export draw it.

**The model (M0).** The model belongs to the open project and is saved in the `.mpproj`:
- `Model { links: Vec<Link>, calendar: Option<ModelCalendar>, column_kinds: Vec<ColumnKind>, measures: Vec<NamedMeasure>, as_of: AsOf }`, every field `#[serde(default)]`.
- A project with a model is written with `"project_version": 2`. A project without one is written as version 1, byte for byte as Phase 8 writes it. The v1.8.0 app refuses a version-2 file with its W4 message, so a model is never silently dropped.
- Tabs are named by their project keys (`t1`, `t2`, …), so renaming a tab keeps its links.
- New library modules:
  - `src/model.rs`: links, key checks, lookup fields, the model calendar and the as-of date
  - `src/measures.rs`: stock and flow measures, time windows and ratios
- `src/dates.rs` stays the only place that knows calendars. M2–M4 add period functions there: the period of a date for a part, period start and end, shift by a unit with day clamping.

**Links (M0, M1).** A link joins a **data** tab to a **lookup** tab on 1–3 key columns, many-to-one:
- A tab is either a data tab or a lookup in the model, never both, so the model is a star.
- One lookup can serve several data tabs (the branch master for both accounts and the GL trial balance).
- A data tab links to a given lookup once.
- A lookup is read after its own steps.

A **lookup field** (`ModelFieldRef { link, column }`) is a lookup column seen from a data tab, stored as "Branch master · Region" and shown in pickers as "Branch master › Region", as W2 does for date fields. The engine adds it by a left join on the worker before grouping or filtering, and drops it after. This is the W2 pattern, so nothing is added to the tab. The join can't change the row count, because a link is used only while its lookup keys are unique (M0 key check). A test asserts the row count on every fixture.

**Filters flow from lookup to data.** A filter on a lookup field keeps the data rows whose key matches. Nothing flows back: the lookup tab is never filtered by its data.

**Shared calendar (M2).** The model calendar is a generated lookup tab, "Calendar", with one row per day and the W0 parts as columns. It is built from one `CalendarSettings` held by the model, so every data tab reads the same fiscal year and week. A data tab's **date link** joins its date column to Calendar · Date. Calendar fields are lookup fields through that link, so M1's machinery serves them.

**Measures (M3–M5).** A time-aware value is computed from a **daily series**:
1. Filter the data tab (its steps, then the cell's filters, lookup fields included).
2. Group by the date.
3. Sum the value column.

Closing balance, average balance, flows, YTD movements, windows and shifts are then computed in Rust from that series and the tab's **snapshot dates**. A subtotal, a grand total or a Total column is the measure of its own rows and its own period. It is never the sum of its children. The pivot (`ValueSpec`) and the chart use one value type, so a chart shows the same number as the pivot.
</architecture>

<conventions>
- **Dates are typed** (Phase 8). A date link needs a Date or Datetime column. A text column is refused with the Phase 8 message.
- **Blank and unmatched are different.** A lookup field has three labels of its own, never merged: "(blank)" is a matched lookup row whose cell is blank, "(blank key)" is a data row whose key is blank (missing, empty or only spaces), "(no match)" is a data row whose key has no lookup row. They sort last in that order. All are kept in every total, so totals reconcile with the unlinked tab. (Changed in S6 by the owner: a blank data key used to show as "(blank)".)
- **Nothing is guessed.** A measure that can't be computed for a cell leaves the cell blank, never 0. The reason is in its tooltip and in the pivot's notes, for example "No balance in Nov 2026 for 'Accounts' (last date 31 Oct 2026)".
- **Names.** Lookup and Calendar fields are stored as "Lookup · Column" and shown as "Lookup › Column". A real column with that name is refused, as in W2-1. A measure's heading names what it is: "Closing balance of Balance", "Average balance of Balance", "Average of month-ends of Balance", "Total of Interest", "Period amount of Balance (YTD)", and with a window "· YTD", "· vs SPLY %", and so on.
- **Percent change** is (current − earlier) / |earlier|. It is blank when the earlier value is 0 or blank. A change in a ratio is in percentage points ("pp").
- **Outside a project.** Macros, packs and saved runs that use lookup fields, Calendar fields or measures need the project's model. Outside a project they are refused with: "This step uses the model of a project (link 'Accounts → Branch master'). Open the project to run it."
- **Python export.** A step that uses a lookup field, a Calendar field or a measure is refused with: "Python export can't follow links between tabs yet. Use the Join tool to copy the column, or export the result tab." Everything else exports as before.
- **The Join tool stays** the way to copy lookup columns into a tab for good. Links add no columns.
- **Saving:** the model in the `.mpproj`; the pivot pop-out's window rectangle in `settings.json`.
</conventions>

<tasks>

<task id="B0" name="Phase 8 baseline (goal 3)">
- In a temporary git worktree at tag `v1.8.0`, sharing the build cache (as `tests/phase8/p7_baseline.rs` did for Phase 7), save with the Phase 8 code:
  - three sample macros: Split date, a filter by a date part with ≥, and a rate band unnumbered
  - a pack with a pivot by a date field and a chart
  - a saved chart with drill levels, its own filters, a colour pick, segment labels and a data table
  - a project of 6 tabs (file, file + steps, join, pivot tab, gap ladder run, chart tab at drill level 2)
- Commit them to `tests/fixtures/phase8_replay/` with `phase8_replay.hashes.json`: the blake3 of what that code gave (rows, the chart's layout, the project bytes after save → open → save, except `saved_at`).
- Keep the code that made them as `tests/phase9/p8_baseline.rs` (Phase 8 APIs only).
- **Acceptance:**
  - `b0_the_phase8_samples_replay_to_the_hashes_the_phase8_code_gave` passes with today's code
  - the Phase 7 replay test still passes
</task>

<task id="P1" name="Pop-out pivot builder">
- The pivot dialog (new pivot, and editing a pivot step) gets a **Pop out** button in its title row. It opens the same builder in its own OS window: resizable, movable to a second monitor, titled "Pivot builder — <tab title>".
- The pop-out edits **the same state** as the docked dialog (`<architecture>`). It is never a copy.
  - Closing the window, or its **Dock** button, docks it back: the docked dialog reopens with every field, value, filter, layout choice and unapplied change as it was.
  - Apply and Cancel do in the pop-out exactly what they do docked.
  - Esc in the pop-out is Cancel, as docked.
- **Larger preview:** in the pop-out the preview fills the window's height, showing as many rows as fit (at least the docked dialog's count), and its columns use the window's width.
- The window's size and position are saved in `settings.json` (`pivot_popout_rect`, serde default none). They are restored when that rectangle is on a monitor the app can see; otherwise the window is centred on the main window.
- Closing the source tab, or the app, with the pop-out open follows the docked dialog's rule for that case.
- **Acceptance:**
  - Pop out by a real click; add a Rows field, a value and a filter in the pop-out; the preview updates; Apply makes a step equal (JSON) to the step the docked dialog makes from the same clicks
  - close the window: the docked dialog shows the same fields and values, unapplied changes included
  - editing an existing pivot step in the pop-out and pressing Cancel leaves the step unchanged
  - the preview shows more rows in a 1600×1000 pop-out than docked
  - the saved rectangle is restored, and an off-screen one is centred
  - render tests by real clicks at the 3 main-window sizes, with the pop-out at 900×600 and 1600×1000
</task>

<task id="P2" name="Nested pivot layout (Excel Compact form)">
- `PivotSpec.layout: PivotLayout`, serde default **SideBySide**:
  - **Side by side**: today's layout and the default, so old pivots don't change
  - **Nested (Excel style)** `{ blank_line_after_group: bool }` (default false)

  It is chosen in the pivot dialog (docked and pop-out) when creating or editing a pivot, and can be switched on a pivot tab without rebuilding the flat table.
- **Nested display:**
  - All Rows fields share one first column, "Row Labels".
  - Each parent row carries its subtotal on the same row, in bold. Children are indented under it, one level per field. The last field's rows are the leaves.
  - An optional blank line follows each top-level group.
  - Grand Total is at the bottom.
  - Columns fields spread across the columns as in Side by side.
  - Groups and children follow each field's order in the spec (Calendar, Ranges, custom list, …).
  - Blank keys show as "(blank)".
- **Subtotals are recalculated from the group's source rows, never added up from the children:**
  - Sum sums.
  - Count counts.
  - Count distinct counts distinct over the group.
  - Average and W.Avg are computed over the group's rows (W.Avg with its weight column).
  - Min and Max are taken over the group's rows.
  - The leaves and the Grand Total are the flat table's rows and grand total.
- **Flat underneath.** The step's result is the flat table, byte for byte as Side by side gives it. Only the screen, the Excel export and the PDF export draw Nested:
  - Excel: one "Row Labels" column with real indent levels (cell alignment indent = depth, not spaces), bold subtotal rows and a bold Grand Total, the pivot's number formats, and the blank lines if chosen
  - PDF: the same, with a 10 pt indent per level
  - CSV and Parquet exports, later steps, charts, macros and Python see the flat table
- If steps follow the pivot, the tab shows the flat table with the note "Nested layout shows when the pivot is the tab's last step", and exports are flat.
- **Show rows** (W6-8) works on a parent row: it gives every source row of that group, with the pivot's filters. On a leaf or a value cell it behaves as today.
- If the Side by side subtotals turn out to be added up (not recalculated) for Average, W.Avg or Count distinct, don't change them: record it under "Found, not changed" with an example, because changing it would change old results.
- **Acceptance.** Use a synthetic tab shaped like the owner's example: Bank (Conventional, Islami) › CL status ((blank), HC, LC, NC); Sum of Balance, Count of AC Number, W.Avg of Rate weighted by Balance; some accounts appear under two CL statuses.
  - every subtotal row equals a pivot by Bank alone (Sum, Count, W.Avg rate, and a Count distinct of AC Number that is not the sum of its children)
  - the Grand Total equals Side by side's
  - the leaves equal the flat table row for row
  - switching layout never changes a number: the flat table's hash is the same in both layouts
  - a three-level Nested pivot (Bank › CL status › Branch) has correct subtotals at both parent levels
  - the Excel export has indent 0/1/2 on the label cells, bold on subtotal and Grand Total rows, and a blank row after each group when chosen (read from the workbook XML)
  - the PDF page text lists the rows in Nested order
  - Show rows on "Conventional" gives exactly its rows, and its sum equals the subtotal
  - an old pivot loads as Side by side, unchanged
  - render tests by real clicks at 3 sizes, light and dark: the layout choice in the dialog, the blank-line option, bold and indent drawn, Show rows from a parent row
</task>

<task id="M0" name="Model core: links and key check">
- **Model menu** (after View): Links…, Key check…, Calendar… (M2), Measures… (M3, M5), As-of date… (M2). With no project open, each item says "The model is part of a project. Save the tabs as a project first." and offers Save project as….
- **Links… dialog:** a table, one row per link: Data tab | key columns | Lookup tab | key columns | Status. Buttons: Add, Edit, Remove.
  - Add/Edit: pick the data tab, the lookup tab, then 1–3 key column pairs (data column ↔ lookup column).
  - Chart tabs and placeholder tabs can't be picked.
  - Refused, each with a plain message:
    - a tab that is already the other role ("'Branch master' is a lookup; a lookup can't have links of its own")
    - a second link between the same two tabs
    - a column pair of different types ("'Branch Code' is text in 'Accounts' and a number in 'Branch master'. Set both to text in Schema & Ingestion first.")
  - Keys match the way the Join tool matches them (Phase 2 join validation): no new matching rule.
- **Key check** (on the worker, after the lookup's steps), run when a link is made or edited, when a project opens, and when either tab's source or steps change:
  - **Duplicate keys** in the lookup: every key that appears more than once, with all its rows
  - **Blank keys** in the lookup
  - **Unmatched keys** in the data: distinct keys with no lookup row, each with its row count and the sum of one value column the owner picks (remembered per link)
  - **Blank keys** in the data: the row count
  - Matched rows, and lookup rows that no data row uses (for information)
- **Status:** "OK", "OK · 12 unmatched keys", or **Broken** ("3 duplicate keys", "2 blank lookup keys").
  - A broken link stays in the model, but its lookup fields are refused until it is fixed: "'Branch master' has 3 duplicate keys. Open Model ▸ Key check to see them."
  - Unmatched keys don't break a link; they show as "(no match)" (M1).
- **Key check… dialog:** one section per link, with the counts above, and buttons:
  - **Show duplicates**: a new tab of the lookup rows whose key repeats, sorted by key, with a "Times" column
  - **Show unmatched rows**: a new tab of the data rows with no match
  - **Copy list**: the unmatched keys with counts and sums, as text

  These are the lists the owner sends back to be fixed.
- The project's open line adds the broken links: "9 tabs opened; link 'Accounts → Branch master' is broken: 3 duplicate keys". The status bar shows "Model: 4 links · 1 broken".
- `.mpproj`: the `model` section and `project_version` 2 as in `<architecture>`.
- **Acceptance:**
  - a product master keyed by (Wing, Product code) with SB01 in both Conventional and Islami links correctly; adding a second (Conventional, SB01) row makes the link Broken, and Key check lists that key with both rows; Show duplicates gives those 2 rows with Times = 2
  - a lookup with a blank key is Broken
  - unmatched data keys are listed with the right row counts and sums, and Show unmatched rows gives exactly those rows
  - the role, type, second-link and chart-tab refusals give their messages
  - a project with a model round-trips (save → open → save gives the same bytes except `saved_at`) and is written as version 2
  - a project without a model is written as version 1, byte for byte as B0's baseline
  - the v1.8.0 refusal text appears for version 2 (tested through the version check)
  - relinking a lookup file (W4) to a file with a duplicate key makes the link Broken in the open line
  - render tests by real clicks at 3 sizes: the Model menu (with and without a project), Links… add/edit/remove, Key check and its three buttons
</task>

<task id="M1" name="Lookup fields in pivot, chart, filter and Show rows">
- **Pivot:** the field list of a data tab shows its links under the tab's own columns: "Branch master ›" expands to the lookup's columns. A lookup field can go to Rows, Columns, Filters and Values (Count, Count distinct, and Sum and so on for numbers), stored as `PivotSpec.model_fields: Vec<ModelFieldRef>` (serde default). Its order is taken as when a field is added (W0 auto order, on the lookup's own steps).
- **Chart:** the category, series, facet and drill pickers list the lookup fields the same way (`ModelFieldRef` beside `DateFieldRef` in the chart's field type; serde default).
- **Filter:**
  - the tab's Filter dialog and W10's "Filters on this chart" take lookup fields, with the value picker listing the lookup's values
  - stored in `FilterCondition.lookup: Option<ModelFieldRef>` (serde default None)
  - a filter on a lookup field keeps the data rows whose key's lookup row matches
  - "(no match)" and "(blank)" can be picked like values
- **Show rows** (W6, pivot and chart) includes lookup fields in its filter, so the rows tab's control total equals the cell.
- **"(no match)"** is a value of every lookup field for unmatched data rows; "(blank)" for blank data keys. Both sort last, "(blank)" before "(no match)".
- A broken link's fields are listed greyed with its reason, and a saved pivot or chart that uses one shows the reason instead of numbers.
- Lookup fields of a link to the Calendar are M2's; this task covers ordinary lookups.
- **Acceptance:**
  - a pivot of Accounts by "Branch master › Region" equals a pivot of Accounts joined to the branch master by the Join tool, row for row, with "(no match)" holding the unmatched keys' sum; the total equals the ungrouped total
  - linking never changes the data tab's row count, on every fixture
  - a filter "Branch master › Region = Dhaka" keeps exactly the rows of Dhaka's branches, and the branch master tab's rows are unchanged (no flow back)
  - a filter on "(no match)" keeps exactly the unmatched rows
  - a composite-key link (Wing, Product code) gives the right product group for SB01 in each wing
  - a chart by a lookup field equals the pivot's totals; a drill Region › Branch name adds up; Show rows from a chart mark and from a pivot cell grouped by a lookup field gives row counts and sums equal to the cell
  - a broken link greys its fields with the reason, and a pivot using one shows the reason
  - a macro with a lookup filter run outside a project gives the `<conventions>` message; the Python export gives its message
  - old pivots, charts and filters load unchanged
  - render tests by real clicks at 3 sizes: the field list expander, the chart picker, the filter value picker on a lookup field, Show rows
</task>

<task id="M2" name="Shared calendar and as-of date">
- **Model ▸ Calendar…:**
  - Turn on the model calendar.
  - Its `CalendarSettings` (fiscal year start, week start) are copied from `settings.json` when it is turned on, and can be changed here for the project.
  - The W0 line ("Week starts Saturday · Fiscal year from January") is shown.
  - **Date links:** for each data tab, pick its date column (Date or Datetime only; text refused with the Phase 8 message). A data tab has at most one date link.
- **Calendar tab:** generated, read-only, titled "Calendar".
  - Columns: Date and every W0 part (Year, Quarter, Year-Quarter, Month, Month number, Month-Year, Week of year, Week starting, Day name, Day of month, Fiscal year, Fiscal quarter, Fiscal month), plus "Month end" (Yes/No) and "Days in month".
  - Rows: every day from 1 January of the earliest year to 31 December of the latest year found in the linked date columns (after their steps; blank dates ignored), so whole years are always there for YTD and same period last year.
  - It is rebuilt when a linked tab's dates go outside its range. It is saved as a recipe (`Calendar { settings }`), not as rows.
  - Its steps panel says "This tab is made by the model calendar. Change it in Model ▸ Calendar…".
- **Calendar fields:** on a date-linked data tab, the pivot, chart and filter pickers list "Calendar › Month-Year" and the other parts (M1's lookup fields through the date link), with the Calendar order of W0. Changing the model calendar's settings changes every Calendar field of the project at once. That is the point of a shared calendar; the Phase 8 per-field date parts still copy their own settings, as before.
- **Snapshot dates:** for each date-linked data tab, the dialog shows:
  - first and last date, and the number of snapshot dates (distinct dates with rows, after the tab's steps)
  - **Frequency:** **Month-end** when no calendar month has more than one snapshot date, otherwise **Daily**. A Daily tab with months of only one snapshot date lists those months: "Jan–Oct 2025 have one date each; their average balances carry that date forward."
  - **Looks incomplete:** snapshot dates whose row count is under half the tab's median row count per date, listed as a warning
- **As-of date** (Model ▸ As-of date…):
  - **Latest data** (default): the latest snapshot date of all date-linked tabs
  - **A date** picked by the owner, saved in the model
  - The dialog lists each tab's last date. When they differ, it says so: "Accounts ends 31 Oct 2026; GL TB ends 24 Nov 2026."
  - The status bar shows "As of 24 Nov 2026 (latest data)".
- **Acceptance:**
  - Calendar rows run from 1 Jan of the earliest year to 31 Dec of the latest; every part equals `dates.rs`; Month end and Days in month are right for Feb 2028 (29) and Feb 2026 (28)
  - a pivot of Accounts by "Calendar › Month-Year" equals a pivot by the W2 date field "Open Date · Month-Year" made with the same settings, in the same order
  - changing the model's fiscal start to July changes "Calendar › Fiscal year" on two data tabs at once
  - a tab of month-end dates is Month-end; a tab of daily dates is Daily; a Daily tab with single-date months lists them; a date with a quarter of the median rows is listed as incomplete
  - the as-of default is the latest date of all date-linked tabs, a picked date round-trips in the project, and the differing-ends note appears
  - text date columns are refused
  - render tests by real clicks at 3 sizes: Calendar… (turn on, settings, date links), the Calendar tab note, As-of date…
</task>

<task id="M3" name="Balances and flows (stock/flow measures)">
- **Column kinds** (Model ▸ Measures… ▸ Columns): a numeric column of a date-linked data tab can be tagged:
  - **Balance (stock)**
  - **Flow**
  - **YTD balance**: a trial balance's income and expense GLs, which add up from the start of the fiscal year
  - or untagged

  Suggestions come from the column name (Balance, Outstanding, Bal, OS → Balance; Interest, Income, Expense, Disbursed, Recovery → Flow). They are suggestions only, applied by a click. The tag sets the default aggregation when the column is added to Values, and the warning below.
- **Aggregations**, added to a value's choice on a date-linked data tab (`ValueSpec.time: Option<TimeValue>`, serde default). P is the period of the cell (M4), its end capped at the as-of date. The **value of a date** is the sum of the cell's rows on that date, 0 when it has none. **Snapshot dates** are the tab's dates after its steps, before the cell's filters, so a branch with no rows on a date counts 0.
  - **Closing balance:** the value at the last snapshot date in P. Blank if P has no snapshot date ("No balance in Nov 2026 for 'Accounts' (last date 31 Oct 2026)").
  - **Opening balance:** the value at the last snapshot date before P's start, if there is one; blank otherwise.
  - **Average balance:**
    - on a **Daily** tab: the mean over every calendar day d of P of the value at the last snapshot date on or before d (carried forward over Fridays, Saturdays and holidays). Blank if there is no snapshot date on or before P's start: "No balance on or before 1 Jan 2025. Add that date's data or start later."
    - on a **Month-end** tab: the mean of the values on P's snapshot dates, headed "Average of month-ends of Balance"
  - **Total (flow):** the sum of the cell's rows dated in P.
  - **Period amount (YTD balance):** the value at P's last snapshot date, minus the value at the last snapshot date before P's start when that date is in the same fiscal year (otherwise minus 0). Blank if P has no snapshot date.
  - **Count at closing**, **Distinct count at closing**, **W.Avg at closing** (weighted by a column): over the rows of P's last snapshot date. These give "accounts at month-end" and "weighted rate at month-end" without counting an account once per date.
- **Totals:** a subtotal, a grand total and a Total column across periods are the measure over their own rows and their own period (the union of the periods they cover): closing at the end, average over the whole span, flows summed. Balances are never added across dates.
- **Stock warning:** a pivot or chart that uses an ordinary Sum (or Count) of a column tagged Balance, over rows spanning more than one snapshot date, shows a banner and a chart note: "Sum of Balance adds 23 dates of balances. Use Closing balance or Average balance." It is a warning, not a refusal. Untagged columns and old pivots have no banner and the same numbers.
- **Acceptance**, on synthetic data with hand-worked numbers:
  - Daily tab with snapshots 1 Nov = 100, 3 Nov = 130 (2 Nov missing), as of 3 Nov: Closing = 130, Average balance for Nov to date = (100 + 100 + 130) / 3 = 110
  - the same with no snapshot on or before 1 Nov: Average balance blank, with the message
  - Month-end tab with 31 Jan = 100, 28 Feb = 120, 31 Mar = 140: Q1 Average = 120, headed "Average of month-ends of Balance"; Q1 Closing = 140; Q1 Opening blank; Q2 Opening = 140
  - YTD balance tab with income 31 Jan = 10, 28 Feb = 25, 31 Mar = 45 (fiscal year from January): Period amount Jan = 10, Feb = 15, Mar = 20, Q1 = 45; with a fiscal start of February, Feb = 25
  - Flow tab: Total for a month equals the sum of its rows
  - Count at closing counts each account once, whatever the number of snapshot dates
  - a pivot Branch × Calendar · Month-Year of Average balance: the Total column is the average over the whole span, not the sum of the months, and a branch subtotal equals the measure of its own rows
  - the stock warning appears on Sum of a tagged Balance over 23 dates and not on an untagged column; the numbers of the old pivot are unchanged
  - Nested (P2) shows measure subtotals equal to a pivot by the parent field alone
  - render tests by real clicks at 3 sizes: the Columns tags and suggestions, the aggregation choice in Values, the warning banner, a blank cell's reason tooltip
</task>

<task id="M4" name="Time intelligence">
- A time value (`TimeValue`) has, besides its aggregation (M3):
  - **Window:** Period (the cell's) / MTD / QTD / YTD
  - **Shift:** None / Prior period / Same period last year (SPLY)
  - **Show:** Value / Difference / % change (Difference and % change need a shift: current − shifted)
  - **Partial period:** Same days (default) / Full period (owner's decision)
- **The cell's period.** When the pivot's Rows or Columns hold a Calendar field that names one period (Year, Year-Quarter, Month-Year, Week starting, Date, Fiscal year), the cell's period is that field's period. Otherwise the cell has no period, and its anchor is the as-of date.
  - Window **Period** needs such a field. Without one it is refused: "Add a Calendar field that names a period (Calendar › Month-Year, for example) to Rows or Columns, or choose MTD, QTD or YTD."
  - A field that mixes years (Month, Quarter, Day name, Week of year, Fiscal quarter, Fiscal month) gives no period: "'Calendar › Month' mixes years. Use Month-Year."
- **The anchor** is the period's end (or the as-of date), capped at the as-of date. A period starting after the as-of date is blank.
  - MTD: the anchor's month start to the anchor
  - QTD: the anchor's fiscal quarter start to the anchor
  - YTD: the anchor's fiscal year start to the anchor
- **Shift.**
  - Prior period moves the window back one of its own unit: Period by its field's unit (day, week, month, quarter, year), MTD by a month, QTD by a quarter, YTD by a year.
  - SPLY moves it back one year.
  - **Same days:** a window cut by the as-of date moves with the same day numbers, clamped to the month's last day: 1–24 Nov 2026 → 1–24 Nov 2025; 1–31 Mar 2026 → 1–28 Feb 2026; 29 Feb 2028 → 28 Feb 2027.
  - **Full period:** the shifted window is the whole shifted period (all of Nov 2025).
  - A window that isn't cut gives the same result either way.
- Blank when the shifted window has no data, with the reason. The heading names the choices: "Average balance of Balance · YTD · vs SPLY %".
- Window, shift and show apply to every M3 aggregation, and in M5 to named measures and ratios (where a Difference is in pp).
- **Acceptance**, with hand-worked numbers on a synthetic daily tab of two years:
  - as of 24 Nov 2026, MTD Total = 1–24 Nov 2026; SPLY Same days = 1–24 Nov 2025; SPLY Full = all of Nov 2025
  - as of 31 Mar 2026, Prior period of MTD with Same days = 1–28 Feb 2026
  - a leap-day case: as of 29 Feb 2028, SPLY = up to 28 Feb 2027
  - YTD with fiscal start July as of 15 Jan 2026 covers 1 Jul 2025–15 Jan 2026
  - a pivot by Calendar · Month-Year: each month's Closing vs Prior period % equals (this − last) / |last|, January against December of the previous year, and 0 → blank
  - Window Period without a period field is refused with its message; Calendar › Month is refused
  - periods after the as-of date are blank; the as-of override changes MTD as expected
  - a chart of Closing balance · vs SPLY % by Month-Year equals the pivot
  - render tests by real clicks at 3 sizes: window, shift, show and partial choices in Values, the refusal message, headings at 1219 pt without overlap
</task>

<task id="M5" name="Named measures and ratios">
- **Model ▸ Measures… ▸ Measures:** list, add, edit, remove named measures of one data tab:
  - `NamedMeasure::Base { name, tab, column, aggregation (M3), filters: Vec<FilterCondition>, sign: Plus | Minus }`. The filters may use lookup and Calendar fields, as in "Interest income = Total of Balance where GL map › Head = Interest income". Sign Minus turns a credit-negative trial balance around.
  - `NamedMeasure::Ratio { name, tab, numerator: Vec<(measure, sign)>, denominator: Vec<(measure, sign)>, annualise: Option<DayCount>, format }`, with `DayCount`: **Actual/365**, **Actual/360**, **Actual/Actual**.
    - Annualising multiplies the numerator's flow terms by (year days / days in the window). Days in the window are its calendar days, capped at the as-of date. Year days are 365, 360, or the days of the fiscal year holding the window's end (365 or 366).
    - Annualising a numerator with a balance term is refused: "Only flows and period amounts can be annualised; 'Loans closing' is a balance."
    - A zero or blank denominator gives a blank.
    - Without annualising, a ratio is a plain share: CASA ratio = closing CASA / closing deposits; ADR = closing advances / closing deposits.
  - Names are unique within the project. A measure used by a ratio, pivot or chart can't be removed; it is refused, listing the users.
- Named measures appear in the pivot's Values list and the chart's values under "Measures" for their own tab, with M4's window, shift and show. A pivot on another tab doesn't list them.
- Subtotals, grand totals and Total columns follow the M3 rule: a ratio's subtotal is the ratio of the group's own numerator and denominator, never an average of the children's ratios.
- **Acceptance**, on a synthetic daily trial balance with a GL map:
  - Interest income of Jan 2026 = 31 (credits, sign Minus) and average earning assets = 3,650: yield Actual/365 = 10.00%, Actual/360 = 9.86%, Actual/Actual = 10.00%; the same numbers in Jan 2028 with Actual/Actual = 10.03%
  - NIM = (interest income − interest expense) annualised / average earning assets, by Month-Year, equals the hand-worked values; its YTD and vs SPLY (pp) are right
  - CASA ratio = closing CASA / closing deposits by Branch master › Region; its grand total equals total CASA / total deposits, not the average of the regions
  - ADR on the month-end accounts tab uses "Average of month-ends" terms when asked for averages, labelled
  - the refusals: annualising a balance, a duplicate name, removing a measure in use
  - a chart of NIM by Month-Year equals the pivot
  - a project with measures round-trips; headless `--project` writes the same pivot tab as the UI
  - render tests by real clicks at 3 sizes: the Measures dialog (base and ratio editors, day count, sign), the Values list "Measures" group, a ratio heading
</task>

<task id="M6" name="Guide, worked examples, release">
- New notes, following `<guide_notes>`: `pivot-popout.md`, `pivot-nested.md`, `model-links.md`, `key-check.md`, `model-calendar.md` (shared calendar, snapshot dates, as-of date), `balances-and-flows.md`, `time-intelligence.md`, `measures-and-ratios.md`.
- `balances-and-flows.md` explains, with the M3 numbers, why balances are never added across dates, how carry-forward works over Fridays, Saturdays and holidays, why a month-end tab gives "Average of month-ends", and why a trial balance's income GLs are a YTD balance.
- **Worked examples** (one note, `monthly-model.md`, from a synthetic project in `tests/fixtures/phase9_model/` that points only inside `tests/fixtures`): Accounts (month-end), GL TB (daily), branch master, product master (Wing + Product code), GL map. It walks through:
  - deposit mix: CASA ratio by Month-Year and by Region, MoM and vs SPLY
  - NIM, yield on loans and cost of deposits: YTD, Actual/365
  - loan portfolio growth: closing by product group vs SPLY %
  - ADR
  - the monthly routine: open last month's project, relink the files (W4), read Key check, check the as-of date
- Updates: `pivot.md` (pop-out, layout, lookup and Calendar fields, measures in Values), `chart.md`, `filter.md`, `projects.md` (the model, version 2), `date-grouping.md` (the "Balances are stocks" box now points to `balances-and-flows.md`), README.
- Pictures from the render tests in `docs/guide/img/`, light and dark: the pop-out, a Nested pivot with a blank line, Links…, Key check, Calendar…, the Values time choices, the Measures dialog, a NIM pivot.
- `cargo build --release`. Run the `#[ignore]` timing test: a pivot by a lookup field on a synthetic 3-million-row tab, against the same pivot by a data column. Report both times; no limit is asserted.
- **Acceptance:** the c8/W8-style test checks every note's sections, that every Example table is the table the code gives and names a test that exists, that every picture named exists, and that the README lists the notes.
</task>

</tasks>

<guide_notes>
Every new note has these sections, in this order: What it does · When to use it · How to use it (numbered steps) · Example (from the tests, with real numbers) · What it refuses and why · Python export · Saved in (macro, project, settings) · Related tools. Plain English for a banker; no Rust or Polars terms; at most 150 lines.
</guide_notes>

<constraints>
- No new Rust crates. Use Polars (joins and `temporal`, already enabled), `serde_json`, `blake3`, `egui` and `printpdf`.
- Every new serialised field gets `#[serde(default)]` with the default stated in its task (`layout` = SideBySide, every model field empty). Old macros, packs, runs, settings, saved charts and projects load unchanged, and B0's replay proves the results are the same.
- No `unwrap()`, `expect()` or index panics on user data. Refuse with a plain message a banker can act on; never default a value (no 0 for a blank measure, no guessed key match, no guessed date).
- Joins, key checks, the Calendar and measures run on the worker; the window never waits for them.
- Colour literals live only in `src/charts/tokens.rs` and `theme.rs`.
- Test data is synthetic. Never commit anything from `portable_data`, and never a `.mpproj` that points outside `tests/fixtures`.
</constraints>

<workflow>
- Never run `cargo clean`. Run `cargo check` while editing, only the task's tests while working, and the full `cargo test` once before each commit. Pipe output through `tail -40`. Grep large files instead of reading them whole.
- One commit per task (`Phase 9 <ID>: …`), pushed after each. Guide notes for a task go in M6 unless the task says otherwise.
- Tests go in `tests/phase9/`, compiled into the phase3 binary like `tests/phase8/`. Render tests go in `src/ui/render_tests_p9.rs`.
- Keep `docs/phase9-progress.md` updated, and resume from it. Each session ends with an acceptance item → test table for its tasks in that file.
- Prove each new test fails without its fix (stub or revert the fix locally, run, restore) and list the tests that failed in the progress file.
- Never stop to ask. Take the option closest to this spec and the Phase 8 house style, record it in `<accepted_deviations>` as `<ID>-n` and in the progress file, and list it in the report.
- If blocked three times on the same failure, mark the task BLOCKED and continue. Dependencies:
  - B0 runs first. P1 and P2 need nothing else.
  - M1 and M2 need M0. M3 needs M2. M4 needs M3. M5 needs M4.
  - M6 runs last.
- Sessions: S1 = B0 + P1, S2 = P2, S3 = M0, S4 = M1, S5 = M2, S6 = M3, S7 = M4, S8 = M5, S9 = M6 (with `cargo build --release`).
- One PR from `phase9` to main, draft until S9, then ready for review. Wait for Tests, and don't merge. Don't commit to main while `phase9` is open.

Lean sessions (from S7), an owner decision of 24 Nov 2026. These replace the stricter rules above where the two differ:
- Stub proofs: prove one or two key tests per task (the ones that pin the task's main numbers or rules), one stub at a time, restored after each. Name them in the progress file. Not every new test needs a proof.
- Pictures: write the light and dark versions, but open only the light one to check it. The owner checks the dark ones.
- Read only what the task needs: the named spec sections, grep for the rest, never a whole large file.
- Keep the report to the `<report>` format, with no narration of the work.
</workflow>

<report>
Reply in no more than 30 lines:
- a table: Task | Status | Tests added | Deviations
- the final `cargo test` summary
- the tests proved to fail without the fix
- questions
- a "look at by eye" list of screenshot paths: one per new dialog or view, light and dark where the app draws both
</report>

<accepted_deviations>
Sessions record their choices here and in `docs/phase9-progress.md`, for the owner to accept or change.

- **B0-1:** the project has 7 tabs, not 6: the six kinds the spec lists (file, file + steps, join, pivot tab, gap ladder run, chart tab at drill level 2) and the branch file the join reads, as the W4 test does (a join names its sources by tab).
- **B0-2:** the hash of the project's text after save → open → save also leaves out `app_version`, sets the files' modified times to 0 and writes the folder as `<ROOT>`: a copy of a file has a new time, a release changes the version, and the repository can be anywhere. Everything else in the text is hashed (the spec says "except `saved_at`").
- **B0-3:** the committed `.mpproj` holds the absolute path of the machine that made it, next to the path relative to its folder (the Phase 8 code writes both). The replay copies the fixtures to a temporary folder and opens the project with relinks to the copies of the data files, so it never reads or depends on the absolute path.
- **B0-4:** `p8_baseline.rs` has one function, `generate`, that writes the files and then calls `replay` on them; the committed hashes are what `replay` gave the v1.8.0 code, and the test calls the same `replay` with today's code. The worktree was at v1.8.0, which is the same commit as `main` at the start of Phase 9 (8749496).
- **P1-1:** the Pop out and Dock button is in the row under the dialog's title bar, at its right, beside the tab's name (the row the Phase 4 schema dialog puts its Pop out in), not in the title bar: an egui window's title bar holds no buttons of ours without rebuilding the window frame. In the window the same row is under the window's own title bar.
- **P1-2 (replaced by the owner, see "Owner decisions on S1"):** the only monitor this app can see is the main window's: egui 0.29 gives that monitor's size, not a list of monitors (and no new crates, so no winit). The saved rectangle is used when at least 100 × 20 pt of its title strip is on the monitor of the main window (monitors taken as a grid of the same size); a rectangle on any other monitor, or off every screen, is not used and the window is centred on the main window. So a window left on a second monitor reopens centred on the main window, not on the second monitor. A monitor list would lift this.
- **P1-3 (replaced by the owner, see "Owner decisions on S1"):** the docked pivot dialog has no Esc key (only its Filters list closes with Esc); the spec says "Esc in the pop-out is Cancel, as docked". The window's Esc is Cancel, except that with the Filters list open Esc closes the list first (as docked, then the next Esc is Cancel). The docked dialog is not changed: that is not part of P1.
- **P1-4:** while the builder is in its own window the app window shows a small note, "The pivot builder is in its own window", with a Dock button (the spec does not ask): the window may be on another monitor, or behind another program, and the owner must always have a way back.
- **P1-5:** the preview in the window brings back as many rows as fit the preview's height, rounded up to a multiple of 20 (so a window being dragged does not ask for a new table at each pixel), from 20 to 500. Docked it is 20, as before.
- **P1-6:** closing the source tab with the builder open: the docked dialog has no rule (it stays open and reads the tab that is active then; with no tab it has nothing to read), and the window does the same. Closing the app closes the window with it and keeps its rectangle in `settings.json` (`on_exit`).
- **P1-7:** a click outside the Filters list closes it (as docked), so after the Dock button, which is such a click, the list is closed; its ticked values are in the filter and stay. After the window's close button, which is not a click in the window, the list is still open.
- **P1-8:** headless render tests have no second window, so the window is a second egui context of its size (900×600, 1600×1000) that draws what the window draws (`pivot_popout_frame`) from the same state, with real clicks in it; a backend without windows keeps the builder docked and offers no Pop out.

**Owner decisions on S1** (given at the start of S2):
- **Accepted as they stand:** B0-1 to B0-4, P1-1, P1-4, P1-5, P1-6, P1-7, P1-8.
- **P1-2 replaced:** the pop-out is restored at its saved position and size exactly as they are, on whatever monitor that is. The app does not check the monitor and does not centre it. If the window is off every screen, the Dock button of the note "The pivot builder is in its own window" in the app window brings it back (P1-4). The window is centred on the main window, at the default size, only when no rectangle is saved. (Kept from the old rule: the window is never smaller than its least size, which a window can't be anyway, and a damaged number counts as "nothing saved".)
- **P1-3 replaced:** Esc is Cancel in the docked pivot dialog too, the same as in the pop-out. In both, Esc does nothing to the dialog while a text box has the keyboard, the Filters list is open, or a drop-down list or menu is open: Esc then closes that first, and the next Esc is Cancel.
- **P1-9 (found while doing P1-3):** egui takes the keyboard from a text box when Esc arrives, before the builder looks, so "a text box has the keyboard" is not enough to tell that Esc was meant for it. The builder keeps, at the end of each frame, whether a text box, list, drop-down or menu was holding Esc (`esc_busy`), and the next Esc counts as theirs. egui's menus (the "..." menu of a field) are not among its open pop-ups, so the builder asks egui whether the menu of each field row is open.
- **P1-10:** the window's last seen rectangle is cleared each time the window is placed afresh, so docking from the app window's note (a window that never drew) never saves an older window's rectangle over the saved one.

**P2 choices (S2):**
- **P2-1 (replaced by the owner, see "Owner decisions on S2"):** the dialog's preview (docked and in the window) shows the flat table, as the step gives it. Nested shows on the tab. The Layout box says so in its hover text. (A nested preview would be a second table to build for every change of the settings.)
- **P2-2:** with "Blank line after each group" the empty line follows every top-level group, the last one too, so it stands between the last group and the Grand total (Excel does the same).
- **P2-3:** the Nested view needs at least one Rows field and the pivot as the tab's last step that runs (steps that are switched off don't count). With no Rows field, or with steps after the pivot, the tab shows the flat table; with steps after it the tab says "Nested layout shows when the pivot is the tab's last step".
- **P2-4:** the layout is switched on a pivot tab in the bar above it (the bar that has Edit pivot and the filter boxes): a Layout box, and "Blank line after each group" when Nested. The switch changes the step in place (one undo each) and builds only the view; the flat table is not rebuilt (a test leaves a marker in the tab that a rebuild would overwrite).
- **P2-5:** on the screen the Nested table has no summary row, no header menu for sorting or summaries, and no paging: all lines are drawn (scrolling). A sort is a step after the pivot, so it turns the tab back to the flat table with the note. Ctrl+C and the copy menu give the flat table, as the step gives it.
- **P2-6:** "Show rows" works on every value cell of a line: a leaf (as today), a parent (every source row of its group, with the pivot's filters) and the Grand total (all rows with the pivot's filters). The labels and blank lines show no rows. The job is a new entry point (`show_rows_nested_job`) next to the old one, which is unchanged.
- **P2-7:** a Nested sheet of more lines than an Excel sheet holds (1,048,575) is not cut or split: its tab is listed on `Export_Errors` with "Use the Side by side layout, or filter the pivot first." The PDF prints the first 5,000 lines, as it does for a flat table, and says it was cut.
- **P2-8:** to keep the old structs and tests as they are, `XlsxTab`, `PdfTab` and `ShowRowsRequest` have no new field. The Nested views travel beside them (`write_xlsx_tabs_nested`, `export_pdf_tabs_nested`, `NestedShowRowsRequest`) and the old entry points call the same code with none. `PdfRow` has a new `bold` field (not set by any old table). One line of `tests/phase8/w1.rs` is changed because the pivot step has a new field (`layout: PivotLayout::SideBySide`).
- **P2-10:** in the dialog (docked and in its window) the Layout box and the blank-line option are in the header row, at the left of the tab's name and the Pop out / Dock button, not in the row of the totals, Show values as, Scale and Sort rows. A further row there took two rows from the window's preview and broke P1's "more rows in a 1600×1000 window than docked".
- **P2-9:** the Excel number scale is the flat table's: the same cell value (the number divided by the unit for Lakh and Crore) and the unit in the heading ("Sum of Balance (BDT Cr)"). A bold parent or Grand total line is bold in its numbers as well.

**Owner decisions on S2** (given at the start of S3):
- **Accepted as they stand:** P1-9, P1-10 and P2-1 to P2-10, except P2-1 as changed below.
- **P2-1 replaced:** when Nested is chosen, the dialog's preview (docked and pop-out) shows the Nested view: one "Row Labels" column, bold subtotals on the parent rows, indents, the blank lines if ticked, Grand Total at the bottom. It is built by the same worker job as the tab's Nested view (`pivot_view::build_nested`, from the same source rows), so the preview and the tab cannot disagree. Side by side previews the flat table as before. Switching the layout in the dialog updates the preview and changes nothing else.
- **Question 1 (P2-10):** the Layout box and the blank-line option stay in the dialog's header row: accepted.
- **Question 3:** the owner checks the first click on the second field's "..." menu by hand. The code and the test's retry stay as they are; the item stays under "Found, not changed".
- **Question 4 (P2-2):** the blank line after the last group, before the Grand total, is kept (Excel does the same).
- **P2-11 (S3, how P2-1 was done):** the preview cuts the Nested view to the same line count as the flat preview (the docked 20, or as many as fit the window, 20 to 500), so the Grand total shows only when every line fits, as the flat preview's total row does. The dialog's caption counts lines of the Nested view. At the smaller main-window sizes the docked preview has room for the heading and one line; that is as it was for the flat table.

**M0 choices (S3):**
- **M0-1:** the Model menu (after View) lists Links… and Key check… now. Calendar…, Measures… and As-of date… are added by M2, M3/M5 and M2 when they exist (no menu item that does nothing). With no project open each item opens one dialog with the message "The model is part of a project. Save the tabs as a project first." and a "Save project as…" button.
- **M0-2:** chart tabs and tabs that did not open are listed in the Links pickers, greyed, with the reason on hover (they cannot be picked); the library refuses them with a message too ("'X' is a chart tab; a chart tab can't be linked. Pick the table the chart draws." / "'X' did not open; it can't be linked until it does."). A data tab used as a lookup is refused with "'X' is a data tab with links of its own; it can't also be a lookup" (the converse of the spec's message).
- **M0-3:** a **blank key** is a missing key, or a text key that is empty or only spaces, in any key column. A blank key never matches (the Join tool would match "" with ""; the spec's "(blank)" convention wins). Every other key matches exactly as the Join tool matches it: same value, no trimming, case kept, same type. Blank data keys are counted on their own and are not in "Show unmatched rows"; every data row is matched, unmatched or blank.
- **M0-4:** a link whose check cannot run (a tab is not open, a key column is gone, a type changed since) shows "Broken: <reason>", counts as broken in the status bar ("Model: 4 links · 1 broken") and in the open line, and its lookup fields are refused like a broken link's.
- **M0-5:** `Model.calendar`, `column_kinds`, `measures` and `as_of` exist with their `#[serde(default)]` but their types are empty placeholders (`ModelCalendar {}`, `ColumnKind {}`, `NamedMeasure {}`, `AsOf::Latest`) that M2 to M5 fill in; an empty part is not written, so M0 writes `"model": {"links": [...]}` only.
- **M0-6:** a tab that has no project key yet gets the next `tN` (the numbering a save gives) when it is first used in a link, so links are by key before the project is saved. A link to a tab that is closed stays in the model (shown as "Broken: '<tab>' is not open as a table") and is saved with the project.
- **M0-7:** a key check runs when a link is made or edited, when a project opens, and whenever either tab's source or steps or the link change (a fingerprint of both tabs' pipelines and the link). The Key check dialog shows the first 8 duplicate keys and the first 8 unmatched keys of each link; Copy list gives all of them (tab separated, headed by the data columns, with Rows and the sum when a value column is chosen).
- **M0-8:** "Show duplicates" and "Show unmatched rows" open a tab that reads a Parquet file in the temporary folder (a list to send back and fix at source), so the project's save leaves it out with its usual temporary-file reason. The lists are made on the worker.
- **M0-9:** the "Sum of … for each unmatched key" box in the Key check lists the data tab's number columns and "No sums"; the choice is `Link.unmatched_value`, saved with the link. A column that has gone or is not a number is said in words and no sum is shown.
- **M0-10:** the open line is "<N tabs opened> ; link 'A → B' is broken: reason" with the Phase 8 line's final full stop dropped, one clause per broken link. It is written when every key check is back, so it may show a moment after "N tabs opened.".
- **M0-11:** the project's unsaved-changes marker follows the model (a project without a model fingerprints as before). `PROJECT_VERSION` stays 1; `PROJECT_VERSION_MODEL` is 2 and is what the app reads up to. `tests/phase8/w4.rs` and `src/ui/render_tests_p8.rs` used a version-2 file as "a newer project": they use version 3 now, and `src/ui/layout_tests.rs` lists "Model" among the menus.

**Owner decisions on S3** (given at the start of S4):
- **Accepted as they stand:** P2-11 and M0-1 to M0-11. The extra correction commit 6e14077 is accepted, and so are the old tests that now use version 3 as "a newer project".
- **M0-3 accepted:** a blank key (missing, empty or only spaces) never matches, in the lookup or in the data. Blank data keys show as "(blank)" in M1, never as a match.
- **M0-1 accepted:** the Model menu lists only the items whose tasks are built.
- **M0-4 accepted, with a change:** a link whose check can't run counts as broken. Its status must say why in its own words, distinct from duplicate or blank keys, for example "Can't check: the tab 'Branch master' is closed" or "Can't check: 'Accounts' has no column 'Branch Code'". If the current text doesn't name the cause that way, it is fixed and tested in the M1 commit.

**M1 choices (S4):**
- **M1-1:** `ModelFieldRef` is `{ name, data_tab, lookup_tab, link_name, column }`, not `{ link, column }`: the link is the pair of project keys (a data tab links to a lookup once), and `link_name` ("Accounts → Branch master") is carried for the "Open the project to run it" message, which has no project to read it from. It is in `FilterCondition.lookup` (with `column` = the field's name), `PivotSpec.model_fields`, `ChartSpec.category_model / series_model / facet_model` and `DrillLevel.model`; each is `#[serde(default)]` and left out of the file when empty, so old bytes are kept.
- **M1-2:** the model reaches the workers as a scope: the app sets `model::scope()` on its own thread every frame, and `JobQueue::submit` takes the scope current when a job is submitted and sets it on the worker. Every table a job builds therefore reads the model the project had at submit, with no change to the 200 `compile_tab_lazy` calls. With no scope (a macro, a pack or a run outside a project) a step with a lookup field gives the `<conventions>` message; a project whose model lacks the link says so.
- **M1-3:** a lookup field that labels (Rows, Columns, Filters, a chart's category, series, facet or drill level) is text: the lookup's value, "(no match)" for a key with no lookup row, and a blank for a blank data key or a blank lookup value (shown "(blank)", sorted before "(no match)"). In Values it keeps its type, and an unmatched or blank row is left blank, so Sum, Count and Count distinct skip it (never a 0, never "(no match)" counted as a value).
- **M1-4:** in a filter condition a text lookup column is as in M1-3. A number or date lookup column is compared as a number or date, so for it an unmatched row is a blank (no "(no match)" to pick).
- **M1-5:** a broken link's fields are listed greyed with the reason under the link, their menu does not open, and a chart picker or the filter's column list shows them disabled. A saved pivot or chart that uses one fails with the reason as its message: the pivot step is flagged broken (the status line and the step list say the reason, and the table shows the result up to the step before it, as for any broken step), the dialog's preview and the chart show the reason.
- **M1-6:** a link whose check has not come back yet is usable; if its lookup has a repeated key by then the join itself refuses (Polars' many-to-one check), so a row is never counted twice. That refusal has the engine's general wording, not the Key check's.
- **M1-7:** a lookup field added to Rows, Columns, a series or panels gets the order its lookup's own steps declare (Split date parts, rate bands) and otherwise Natural; its labels are not scanned for Calendar or Ranges because "(no match)" would defeat that.
- **M1-8:** the app remembers the title a linked tab had when it was last open, so a closed tab is still named: "Can't check: the tab 'Branch master' is closed".
- **M1-9:** `PivotValuesRequest` and `PickerRequest` have a new `lookup` field (the other sites say `None`); `TransformStep::PivotTable` has `model_fields`, so one line each of `tests/phase8/w1.rs` and `tests/phase8/w2.rs` changed. Some helpers of `render_tests_p8.rs` are `pub(crate)` now for the P9 tests. The M0 message "… has no column '…' any more" lost its "any more" (owner decision on M0-4).
- **M1-10:** a macro or step recorded on one data tab carries that tab's link. Run on another tab it fails loudly ("there is no column '…' at this step, and the link '…' needs it") or, if the column exists, uses that link's keys; it never guesses another link.
- **M1-11:** "(blank)" and "(no match)" sort last in the pivot (also by value), the chart (category, series, panels) and the value pickers, whatever the order chosen.

**Owner decisions on S4** (given at the start of S5):
- **Accepted as they stand:** M1-1 to M1-11, except M1-4 and M1-6 as changed below. The M0-4 wording fix is accepted.
- **M1-3 accepted:** a lookup field in Values leaves unmatched and blank rows out of Sum, Count and Count distinct.
- **M1-5 accepted:** a saved pivot on a broken link is a broken step with the link's reason, and the steps before it still run.
- **M1-4 replaced:** "(no match)" can be picked on a lookup field of every type (text, number, date, Date part), in the tab's filter and in "Filters on this chart". It is stored as its own flag on the condition (`FilterCondition.no_match`, serde default false, left out of the file when false), never as a value, so it can't collide with a real value. "(blank)" means a matched lookup row whose cell is blank, and never includes unmatched rows. Grouping by a number or date lookup column in a pivot or chart shows "(no match)" and "(blank)" as two separate rows, "(blank)" before "(no match)".
- **M1-6 replaced:** when a lookup's keys repeat at the time of a join (after the check: the file or the lookup's steps changed), the refusal uses the Key check's words, naming the link and pointing to Model ▸ Key check ("'Branch master' has 2 duplicate keys. Open Model ▸ Key check to see them."), and the link is re-checked and marked Broken at once (status bar and Links…).

**M1 follow-up choices (S5):**
- **M1-12 (replaced by the owner, see "Owner decisions on S5"):** a data row whose key is blank is not an unmatched row (the Key check counts it apart, M0-3). It keeps showing as "(blank)" when it labels a pivot or chart (M1-3), and a filter on "(blank)" keeps it with the matched rows whose cell is blank, so Show rows of a "(blank)" row of a pivot still gives exactly that row's rows. "(no match)" is only a nonblank key with no lookup row. The tests of the spec's two sets use data with no blank keys; one older test keeps both.
- **M1-13:** the filter's lookup column is now read in its own type, with an extra true/false column for "unmatched" (dropped after the filter), for every type. A condition that leaves values out ("does not equal", "is not in list", "is not blank") keeps an unmatched row unless "(no match)" is picked; every other test (equals, in list, greater than, is blank) leaves it out unless "(no match)" is picked. A text lookup value that really reads "(no match)" is a real value and is found by value, apart from the flag. A filter saved in S4 with "(no match)" among its values (the S4 build had not been released) is not turned into the flag.
- **M1-14:** the repeated-key test at a join reads the lookup's keys once more on the worker (a group-by on the keys, no rows kept) before the join; a lookup is a master table, so this is small. The link is reported through a queue the app drains each frame (`model::take_broken_at_join`), because the error travels as text through every job that can build a table. Its status is set Broken at once and the link is sent to the worker for the full check.

**M2 choices (S5):**
- **M2-1:** a date link is stored in `ModelCalendar.date_links` (`{data_tab, column}`), not in `Model.links`. The engine, the key checks and the pickers read `Model::all_links()`: the user's links, then each date link as a link from its date column to Calendar · Date. So the Links dialog and the status bar's "Model: n links" count only the user's links, and a date-linked tab can't also be a lookup (the role rules read every link).
- **M2-2:** the Calendar tab is `TabSource::Calendar { settings, first_year, last_year }`, saved as the run `{"tool": "calendar", settings, first_year, last_year}` (the spec's `Calendar { settings }` plus its years), never as rows. Its years are the earliest and latest year of the linked tabs' dates exactly (read by a worker job per tab, after the tab's steps, blank dates ignored); it is made again when they change, up or down. With no date link it covers the current year.
- **M2-3:** the Calendar tab has the project key kept in `ModelCalendar.tab`. If the owner closes it, the date links say "Can't check: the tab 'Calendar' is closed" until Model ▸ Calendar… is opened, which makes it again with the same key. Turning the calendar off closes the tab and removes the date links and any picked as-of date.
- **M2-4:** a Date and time column is joined to the Calendar by its day (the key is cast to a date on the data side only; the data tab's own column is not changed). A user link of a date and time column to a date column is still refused by type, as in M0.
- **M2-5:** both dialogs apply each choice at once and have a Close button only (no Save), because the snapshot dates, the Calendar tab and the status bar follow the choice. The settings are copied from `settings.json` when the calendar is turned on; `settings.json` and View ▸ Calendar… are never changed by the project's calendar.
- **M2-6:** "read-only" is: the steps panel of the Calendar tab shows the note instead of the steps, and a step sent to it (menus, tools) is refused with the note. Cell editing, if the tab offers it, is not changed.
- **M2-7:** Month-end / Daily is by calendar month of the snapshot dates; single-date months are listed only for a Daily tab, in runs ("Jan–Oct 2026 have one date each; their average balances carry that date forward.", singular for one month). A date is incomplete when its rows are fewer than half the median rows per date (median of an even count is the middle two averaged); a date with exactly half is not listed.
- **M2-8:** the as-of date is typed D/M/Y (or 2026-11-24 or 24 Nov 2026) and saved as `{"Date": "2026-10-31"}` in the model; "Latest data" is the default and is not written. The status bar shows "As of …" only when the calendar is on and a tab's dates have been read. A picked date later than every tab's data is accepted with a note. Nothing calculates from the as-of date yet (M3 to M5 do).
- **M2-9:** a Calendar field added to Rows, Columns, a series or panels gets the order of its date part (text parts in Calendar order from the model's week start; numbers and dates sort as they are), so "Calendar › Month-Year" is in time order, as the W2 date field of the same column.

**Owner decisions on S5** (given at the start of S6):
- **Accepted as they stand:** M1-13, M1-14 and M2-1 to M2-9. The M1-6 UI test relying on the library test for the wording is accepted. Date links not counted in "Model: n links", and the Calendar and As-of dialogs applying each change at once, are accepted.
- **M1-12 replaced:** every lookup field (Calendar fields included) has three labels of its own, never merged: "(blank)" (a matched lookup row whose cell is blank), "(blank key)" (a data row whose key is blank: missing, empty or only spaces, as M0-3) and "(no match)" (a data row whose key has no lookup row). They sort last in that order: (blank), (blank key), (no match). Pivot and chart grouping show them as three rows; the tab's filter and "Filters on this chart" list each one that occurs, stored as its own flag on the condition (`FilterCondition.blank_key`, beside `no_match`; serde default false, left out of the file when false, so saved filters load unchanged); Show rows from each gives exactly its rows. Totals still equal the ungrouped total. `<conventions>` says "(blank key)" now.
- **Date-link rows aligned:** the date-link rows in Model > Calendar... have the same box width on every row, with a render test asserting it at 3 sizes.

**S6 follow-up choices:**
- **M1-15:** "(blank key)" is text in a label (a pivot or chart label, the picker's list) and a flag in a filter: it is a string label that is added in the join where a label is made, and `FilterCondition.blank_key` (beside `no_match`) where a filter is made; a real lookup value that reads "(blank key)" is found by value in a pivot label (the same limit "(no match)" has) and by value in a filter (the flag is separate). A lookup field in Values keeps leaving blank-key and unmatched rows blank (M1-3).
- **M1-16:** a data row with a blank key is kept out of "(blank)" (a matched lookup row with a blank cell) in the filter and in Show rows, as the owner decided; "(blank)" on a number or date lookup column never includes a blank-key row either.
- **M1-17:** the date-link rows of Model > Calendar... have a title cell of one width (`MODEL_LINK_TITLE_W`, 170 pt, cut with an ellipsis and the full title on hover), so every box starts at the same place and has `MODEL_COMBO_W`.

**M3 choices (S6):**
- **M3-1:** the spec's `ValueSpec` is the pivot's `PivotValue` here: `PivotValue.time: Option<TimeValue>` (`TimeValue { agg: TimeAgg }`, a struct so M4 can add its window and shift), not written when none. A pivot made on a date-linked data tab also carries `PivotSpec.data_tab` (and the `PivotTable` step the same field), the tab's project key, not written when empty: the time aggregations and the stock warning read that tab's date link and column tags from the model. A pivot on a tab with no date link, outside a project, and every pivot made before M3 has none.
- **M3-2:** the period P of a cell is the set of calendar days whose Calendar fields (the pivot's Rows, Columns and Filters) equal the cell's keys, cut at the as-of date. A subtotal, a Total column or the Grand total (no key on a Calendar field) takes the union of the periods that field shows in the pivot; a pivot with no Calendar field takes every day from the tab's first snapshot date to the as-of date. A date part of the tab's own column (W2) in a pivot with a time aggregation is refused in words (use the Calendar field). The daily series ignores Calendar keys and filters, so the carry-forward reaches back before P starts.
- **M3-3:** the acceptance case "Q2 Opening = 140" needs a Q2 column, so the test has a fourth month-end, 30 Apr = 150 (Q1 average 120, Q1 closing 140, Q1 opening blank are as the spec says).
- **M3-4:** Period amount subtracts the value at the last snapshot date before P only when that date is in the same fiscal year as P's closing date (otherwise 0), with the model calendar's fiscal start.
- **M3-5:** a value's label is the static "Average balance of Balance"; the pivot's heading becomes "Average of month-ends of Balance" when the tab is Month-end (worked out from its snapshot dates when the pivot is built), so the heading always agrees with the calculation.
- **M3-6:** in a message "last date" is the tab's last snapshot date up to the as-of date. If the as-of date is not yet known (the tabs' dates are still being read) a time aggregation is refused with "the as-of date is not known yet"; the app works every tab with a time aggregation out again when the model (as-of date, tags, calendar, links) changes.
- **M3-7:** the chart gets the stock warning as a chart note (`ChartSpec.data_tab`, never saved, filled by the app). The time aggregations themselves are not in the chart's value choice yet: the spec's M3 section lists only the pivot's value choice, and the shared value type of the chart comes with M4.
- **M3-8:** why a cell is blank and the warnings reach a pivot tab through a worker report (`PivotReport`, made again when the steps or the model change), drawn as a banner above the grid and as the tooltip of the blank cell. They are in the flat grid and the dialog's flat preview; the Nested view and its preview show the numbers without the tooltip. Show rows of a time cell gives the rows of its keys (as for any cell), not only the rows of the closing date.
- **M3-9:** tags are in `Model.column_kinds` as `{ tab, column, kind }`; Model > Measures... lists the numeric columns of date-linked tabs. A name is matched by whole words (Balance, Outstanding, Bal, OS; Interest, Income, Expense, Disbursed, Recovery, in any case, also in CamelCase); a name with words of both kinds, or neither, gets no suggestion.
- **M3-10:** the aggregation list of a value is as tall as the room under it in the window (it scrolls), so no item is below the window's edge.

**Owner decisions on S6** (given at the start of S7):
- **Accepted:** M1-15 to M1-17 and M3-1 to M3-10, except M3-8 as changed below.
- **Q2 (M3-3):** the extra 30 Apr = 150 row for "Q2 Opening = 140" is accepted.
- **Q1 (M3-7):** charts get the time aggregations in M4, through one value type shared by the pivot and the chart (`ValueSpec` with its time value), so a chart shows exactly the pivot's number. Part of M4's acceptance: a chart of Closing balance and of Average balance by Calendar · Month-Year equals the pivot, and a chart of Closing balance · vs SPLY % by Month-Year equals the pivot.
- **M3-8 is replaced:** Show rows on a time cell gives the rows behind its number, with the cell's keys and filters:
  - Closing balance, Count at closing, Distinct count at closing, W.Avg at closing: the rows of the closing date
  - Opening balance: the rows of the opening date
  - Total (flow): the rows dated in the period
  - Period amount (YTD balance): the rows of the two dates it subtracts, with a "Part" column ("End" / "Start")
  - Average balance: the rows dated in the period plus the rows of the date carried in, and a note in the tab title area: "These rows make up an average; their sum is not the cell."
  - A blank cell has no Show rows, and its menu item gives the cell's reason.
  - Tests: the sum of the rows equals the cell for Closing, Opening and Total; the count equals Count at closing; End minus Start equals Period amount; on the M3 daily example, the Average balance rows are 1 Nov and 3 Nov with the note; one render test by a real click at 3 sizes.

**M3 follow-up choices (S7):**
- **M3-11:** Show rows on a time cell keeps the cell's keys and filters on every field except Calendar fields (a Calendar key or filter only picks the period) and picks the rows by the dates the number reads (`measures::rows_of`, an "in list" on the tab's date column). A pivot's Total column, subtotal and Grand total give the rows of their own number the same way. The Nested view's lines use the same code.
- **M3-12:** a data tab whose date column is Date and time is refused for Show rows on a time cell, in words ("Convert it to a Date ... first"), because the days can't be picked by value from it without guessing; the pivot's numbers are not affected.
- **M3-13:** the note of an average ("These rows make up an average; their sum is not the cell.") is drawn in a muted line under the tab bar of the rows tab (`tab_notes`, never saved) and goes when the tab closes. A month-end average has the note too, because its rows are the month-ends.
- **M3-14:** the tab title of a time cell's rows is "Rows: <keys> · <heading of the value>", for example "Rows: Branch = B1 · Closing balance of Balance". A blank cell's "Show rows" item is disabled with the cell's reason as its hover text, and a double-click on it does nothing.

**M4 choices (S7):**
- **M4-1:** `TimeValue { agg, window, shift, show, partial }`; the four new choices are left out of the file while they are the defaults (Period, None, Value, Same days), so an M3 value keeps its bytes. A value with none of the M4 choices set keeps M3's whole-span period when the pivot has no Calendar field (so the M3 numbers do not change); "Window Period needs a period field" is refused as soon as a shift is chosen (a shift needs the field's unit), and the Difference and % change need a shift ("shows a difference or a % change, which needs a shift"). MTD, QTD and YTD need no field: their anchor is the as-of date.
- **M4-2:** a Calendar field that mixes years in Rows or Columns (Month, Quarter, Day name, Week of year, Fiscal quarter, Fiscal month) is refused with "'Calendar › Month' mixes years. Use Month-Year ..." for every value that uses an M4 choice, MTD included (the rows would repeat one number). A plain M3 value on such a field works as in M3. A filter on a Calendar field only picks the cell's own period: windows and shifts ignore it, so a filter to Aug-Oct does not stop Sep's prior period from reading Aug.
- **M4-3:** a subtotal, a Total column or the Grand total over a period field has the span of the periods the pivot shows as its period; its anchor is the span's last day capped at the as-of date, and a shift moves the whole span.
- **M4-4:** the anchor of a cell is its period's end capped at the as-of date, and with no period field it is the as-of date. A period or window that starts after the as-of date is blank with M3's "No balance in Nov 2026 ..." reason, whatever its shift.
- **M4-5:** shifting. A window cut by the as-of date with Same days moves both ends by the unit, the day number clamped to the month's last day (1-30 Mar goes to 1-28 Feb); a window that is not cut, and Full period, move the period's natural end so that the last day of a month stays the last day of its month (30 Apr goes to 31 Mar). Prior period of a Period window is one of its field's unit (a Date field a day, Week starting 7 days, Month-Year a month, Year-Quarter a quarter, Year and Fiscal year a year); SPLY is 12 months, except a Week starting period, which goes back 52 weeks (364 days) so that it stays a week start.
- **M4-6:** headings: the window ("· MTD"), then for a shift "· Prior period" or "· SPLY" when the value is the shifted window's, "· vs SPLY" for a Difference, "· vs SPLY %" for a % change, and "· full period" when Full period is chosen with a shift. Period is not written.
- **M4-7:** a % change is a percent number (12.5 = 12.5%, shown with 2 decimals); a count with a Difference is a plain number and with a % change a percent. "Show values as % of total" is refused for a value with an M4 choice, as for any value whose parts do not add up.
- **M4-8:** an earlier window with no value says so in the cell's reason: "Earlier window (1 Oct 2026 to 24 Oct 2026): No balance in ..."; an earlier value of 0 gives "The earlier window (...) is 0, so a % change can't be worked out." On a Month-end tab a partial current month with Same days looks for a month-end inside the shifted same days, finds none and is blank with that reason: Full period is the choice for such a tab.
- **M4-9:** Show rows of a time cell with a shift gives the rows of the shifted window when the cell shows the shifted Value, and the rows of the current window for a Difference and a % change (the cell is made of two windows; the reason is not repeated in the tab).
- **M4-10:** charts. `ChartSpec.time` (the one measure) and `MeasureSpec.time` carry the shared `TimeValue`; a chart with a time measure is calculated by `build_pivot` for rows = the category and columns = the series, so a mark is the pivot's cell. Its categories are in the pivot's order (Calendar order for a Calendar field); By value, Top N and "Other" are not applied. A chart that mixes a time measure with an ordinary one, has small multiples, a size, a scatter, bubble, histogram or box form, or a date part of the tab's own column as its category is refused in words. A chart filter or a drill level takes its rows away before the series, so the snapshot dates are those of the rows left. The first four blank marks are named in the chart's notes with their reasons. Python export of such a chart is refused with the `<conventions>` wording.
- **M4-11:** the Values chip of a time aggregation has a "Time" button that opens a popup with four rows of choices (Window, Shift, Show, Partial period); Difference and % change are greyed while the shift is None, and choosing no shift puts Show back to Value. The chart studio has the same four choices as combo rows under the aggregation.

**Owner decisions on S7** (given at the start of S8):
- **Accepted:** M3-11 to M3-14 and M4-1 to M4-11.
- **M4-1 follow-up:** a plain Period value with no Calendar field keeps M3's whole-span number, and the "needs a period field" refusal starts once a shift is chosen: accepted. So the number can't be misread, the pivot's notes (and the chart's notes) state the span it covers, for example "Closing balance of Balance: whole data, 1 Jan 2025 \u2013 24 Nov 2026 (as of 24 Nov 2026)". Test: the note's dates on the daily example, and no note when the pivot has a Calendar period field (or the value has an M4 choice).
- The chart studio's time-aggregation list was not opened by a headless click: the owner checks it by eye. Left as recorded.

**M5 choices (S8):**
- **M5-1:** a named measure in Values is a `PivotValue` with `field` = the measure's name and `time.agg = TimeAgg::Measure` (a new variant, not in the aggregation lists), so M4's window, shift, show and partial period apply unchanged; in a chart it is `MeasureSpec.column` = the name with the same time value. `NamedMeasure` is `Base { name, tab, column, aggregation, filters, sign, weight }` or `Ratio { name, tab, numerator, denominator, annualise, format }`, saved as `model.measures` with a `kind` tag.
- **M5-2:** a ratio's terms are base measures only (a ratio of ratios is refused in words); a term's sign multiplies the base measure's own sign. Each term is evaluated for the cell's own rows and window, so a subtotal, a Total column and the Grand total are the ratio of their own numerator and denominator.
- **M5-3:** names are unique ignoring case, and can't be a column of the tab; a saved measure's name and tab can't be changed in the dialog (add a new one and remove the old), because pivots, charts and ratios refer to the name.
- **M5-4:** a ratio is shown as Percent (9.5 = 9.5%) or Number; the heading of a Difference of a Percent ratio ends "(pp)". A % change is the relative change as for any value.
- **M5-5:** Show rows is not offered on the cell of a named measure (its number comes from several filtered series); plain time values keep it.
- **M5-6:** a blank term, a blank denominator or a denominator of 0 gives a blank cell with its reason ("The denominator of 'X' is 0 in ...").
- **M5-7:** a ratio with an average term on a month-end tab says "ADR: its average terms are averages of month-ends (the tab holds month-end data)." in the pivot's notes; its heading is its name.
- **M5-8:** `--project` (headless) builds the model of the project (links with the key check, calendar, as-of date from the data or the saved date, tags, measures) before it builds the tabs; before, a headless run could not read a tab that used a link.
- **M5-9 (a fix of M4):** a cell with a period (a Calendar period field) reads MTD, QTD and YTD up to its own anchor; M4 read them to the end of the month, quarter or year capped at the as-of date, so every month showed the same YTD. Test: `m5_a_ytd_window_of_a_month_reads_up_to_that_month_and_not_to_the_as_of_date`.
- **M5-10:** the filters of a base measure in the dialog are "equals", "is not" or "contains" with one value, on a column, a lookup field or a Calendar field. The chart studio's Value list has a "Measures" group; like the aggregation list (M4), it was not opened by a headless click, and the chart equals the pivot in a library test (`m5_a_chart_of_the_nim_by_month_year_equals_the_pivot`). The owner checks the list by eye.
- **M5-11:** annualising multiplies the numerator by year days / calendar days of the window (capped at the as-of date); Actual/Actual uses the days of the fiscal year holding the window's end. The editor of a measure is drawn above the list of measures.

**Found, not changed (S2):**
- The Side by side layout's Subtotals box was checked for Average, W.Avg, Count distinct, Min and Max: each subtotal is calculated from the group's own rows (one group-by per level), not added up from the cells (`p2_side_by_side_subtotals_are_calculated_from_the_group_s_rows_not_added_up`). So there is nothing to record here, and nothing was changed.
- In a fresh pivot dialog, a first click on the "..." menu of the second field in the list does not open the menu in the headless render test (a second click does; the first and the third field open at once). It is not reproduced or looked at in a real window. The P9 test helper clicks once more. Not changed.
</accepted_deviations>
