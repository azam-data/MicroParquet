# MicroParquet — Phase 8 specification (Workspace and dates)

Sources for this phase:
- The owner's requests of 20–23 Nov 2026, made after testing the Phase 7 builds:
  - a chart tab stays tied to its source tab and can be edited again
  - drill-down from a chart, as in Power BI
  - a project file that reopens the sources, steps, pivots and charts
  - a colour picker for single items ("management asks to change colours for a specific item")
  - rate-band labels without serial numbers
  - month and day names in their natural order
  - a "Split date" tool (Open Date → Year, Quarter, Month, Day name…) and date grouping as in Power BI
- The owner's decisions of 23 Nov 2026: the fiscal year starts in **January** and the week starts on **Saturday**, both changeable.
- Not in this phase: links between tabs, a shared calendar across tabs, and time intelligence (MTD, YTD, same period last year, average daily balance). They are Phase 9 ("Model"), which needs this phase's project file.

Sessions are told which task IDs to do. Read only those task sections, plus `<architecture>`, `<conventions>`, `<constraints>`, `<guide_notes>`, `<workflow>`, `<report>` and `<accepted_deviations>`.

<goal>
Phase 8 is done when:
1. Tasks W0–W8 are implemented, and `cargo build --release` and `cargo test` pass.
2. Every acceptance item is an automated assertion. Every UI item has a headless egui render test at 1219×686, 1600×900 and 1291×700 pt, asserting that:
   - nothing overflows or is clipped
   - no two labels overlap
   - every control works when its real rect is clicked
3. Existing results are unchanged: every macro, pack, saved run, saved chart and settings file from Phase 7 loads and gives the same rows in the same order. A test replays the Phase 7 sample macros and packs and compares their output hashes.
4. Each new tool has a guide note (`<guide_notes>`), and `docs/guide/README.md` lists them.
</goal>

<architecture>
**Calendar core (W0).** A new library module `src/dates.rs` is the only place that knows about calendars. Everything else calls it: the Split date step, pivot, chart, filter, sort and the Python export.
- `CalendarSettings { fy_start_month: u32 (1–12), week_start: Weekday }`, serde with defaults **January** and **Saturday**. It is stored in `settings.json` and edited in View ▸ Calendar….
- `DatePart`: the parts in the table under W0, each with:
  - a Polars expression on a Date or Datetime column
  - the label it writes
  - its data type
  - its order
- Every step or spec that uses a date part **copies** the `CalendarSettings` it was made with, the way a sort step copies its custom list. A macro therefore replays the same result even if the settings change later.

**Orders.** Two built-in orders join Natural, Tenor and custom lists in `sort_lists::SortOrder`:
- **Calendar**: month names, day names (in week order from the copied week start), quarters, "Aug 2026" and "2026 Q1" labels
- **Ranges**: labels of numeric ranges, such as "8%–9%", "Below 0%", "> 15%", "1 Crore to 5 Crore" and "Above 10 Crore"

Natural is **not** changed, so old results keep their order. Pivot, chart and filter pick Calendar or Ranges *when a field is added* if every non-blank label parses, and store that choice in their spec. A step that writes an ordered label column (Split date, Rate band) declares the column's order; `engine::column_orders(steps)` derives it from the steps, so no new tab state is needed.

**Project file (W4).** A `.mpproj` file (JSON, `"project_version": 1`) describes a session, not its data:
- the tabs, in order, with stable keys `t1`, `t2`, …
- the active tab
- each tab's recipe, which is one of:
  - **File**: the file (absolute path, path relative to the project file, size, modified time, blake3), Parquet or CSV, its `IngestionConfig`, and the file's own column names
  - **Run**: a `tool_runs::ToolRun` whose source names are project tab keys. This covers join, stack, period reconcile, pivot tabs and every banking tool.
  - **Chart**: a `ChartSpec` and the key of its source tab (W5)
- for every tab: its steps, column formats, summary choices, control column, side-panel chart spec and title

It reuses the Phase 6 machinery: `ToolRun`, `RunEnv`, `SourceId` and the pack `required_columns` check. Opening a project rebuilds the tabs in dependency order on the worker.

**Linked chart tabs (W5).** A chart tab stops being a frozen table. It is a view of its **source tab**: its `ChartSpec` plus the source tab's id. It recalculates when the source's steps change. "Edit chart" opens the studio on the source with the chart's spec.

**Drill (W6).** `ChartSpec` gains drill levels and a drill path:
- clicking a mark goes one level down
- a breadcrumb goes back up
- "Show rows" opens a new tab: the source's steps plus a `FilterGroup` for the path

Pivot cells get the same "Show rows".

**Colours (W7).** Precedence for an item's colour:
1. the chart's own pick
2. the house colour (`portable_data/house_colours.json`)
3. the automatic slot of `ColourMap`

The 8-slot palette is not extended (see `<conventions>`).
</architecture>

<conventions>
- **Dates are typed.** Every date feature needs a Date or Datetime column. A text column is refused with: "'Open Date' is text. Set it to Date in Schema & Ingestion (D/M/Y) first." Nothing is guessed from text.
- **Blank dates give blank parts.** They are never "Unknown", never 0 and never a default date. Pivot and chart show them as "(blank)", as they do today.
- **No new palette slots.** Past 8 series the existing rule stands: grey plus direct labels, or "Top N + Other". A palette past about 8 colours can't be told apart, especially by colour-blind readers. The owner's need is met by the colour picker and house colours (W7).
- **Week numbers** are defined once (W0) and quoted the same way in the guide, the tooltip and the Python export.
- **Fiscal labels:** with a January start, fiscal year = calendar year and is written "FY2026". With any other start it is written "FY2026-27" (the year it starts in, then the last two digits of the year it ends in).
- **Project files hold no row data.** They do hold what the user typed into steps (filter values, SQL text, labels). The guide says to treat a `.mpproj` like a macro file: keep it out of shared folders that the data itself may not go to.
- **Saving:** settings in `settings.json`; house colours in `portable_data/house_colours.json`; projects wherever the user saves them (default folder `portable_data/projects/`).
</conventions>

<tasks>

<task id="W0" name="Calendar core, Calendar and Ranges orders">
- Add `src/dates.rs` with `CalendarSettings` and `DatePart`. Each part maps to one Polars expression (`temporal` feature; no new crate) with this output:

  | Part | Label (example) | Type | Order |
  | --- | --- | --- | --- |
  | Year | 2026 | Int32 | numeric |
  | Quarter | Q1 | text | Calendar |
  | Year-Quarter | 2026 Q1 | text | Calendar |
  | Month | Jan | text | Calendar |
  | Month number | 1 | Int32 | numeric |
  | Month-Year | Aug 2026 | text | Calendar |
  | Week of year | W01 | text | Calendar |
  | Week starting | 01 Aug 2026 | Date | date |
  | Day name | Sat | text | Calendar, from the week start |
  | Day of month | 1 | Int32 | numeric |
  | Date | 01 Aug 2026 | Date | date (drops the time of a Datetime) |
  | Fiscal year | FY2026 or FY2026-27 | text | Calendar |
  | Fiscal quarter | FQ1 | text | Calendar |
  | Fiscal month | 1 (first month of the fiscal year) | Int32 | numeric |

- **Week rules.** A week starts on `week_start`. "Week starting" is the date of the week's first day, on or before the date. "Week of year" counts the weeks of the calendar year: W01 is the week that contains 1 January, so it can start in the previous December. Pin these with tests on known dates, using Saturday and one other start day.
- **View ▸ Calendar…** edits the settings: fiscal year start (Jan–Dec) and week start (Sat–Fri). The defaults are January and Saturday. Shown in the status of any dialog that uses date parts: "Week starts Saturday · Fiscal year from January".
- **Orders.** Add `SortOrder::Calendar` and `SortOrder::Ranges`, and add "calendar" and "ranges" to `RESERVED_NAMES`.
  - **Calendar** ranks month names (short and long, any case), day names (in week order from the copied week start), Q1–Q4, FQ1–FQ4, W01–W53, "Aug 2026", "2026 Q1", "FY2026" and "FY2026-27".
  - **Ranges** ranks a label by its first number, scaled by its unit word: % none; Thousand/K ×1e3; Lakh ×1e5; Million/M ×1e6; Crore/Cr ×1e7; days, months and years by their length in days. Order:
    1. "Below x" / "< x" / "Under x" before x
    2. an exact "x" (e.g. "0%") before a range that starts at x
    3. "Above x" / "> x" / "≥ x" / "Over x" after every range that starts below x
    4. "No rate" / "No …" / unparsed labels after all ranked ones, in Natural order
  - Mirror both orders in the Python export (`macros.rs`, next to the Tenor mirror).
- **Column orders from steps.** Add `engine::column_orders(steps) -> HashMap<String, SortOrder>` (filled by W1 and W3).
- **Version housekeeping.** The v1.7.0 release was built with `Cargo.toml` still at 1.6.0, so its window title, macro `app_version` and Mac bundle say 1.6.0.
  - Set `version = "1.8.0"` in `Cargo.toml`, keeping its CRLF line endings.
  - In `.github/workflows/main.yml`, add a first step to the tag-push path that fails with "Tag vX does not match Cargo.toml version Y" when they differ. Manual runs are unaffected.
  - Run `actionlint` on the workflow.
- **Auto order.** When a field is *added* to a pivot's Rows or Columns, a chart's category, series or facet, or a sort level, the order defaults to:
  1. the column's order from `column_orders`, else
  2. Calendar if every non-blank label parses, else
  3. Ranges if every non-blank label parses, else
  4. today's default

  The choice is stored in the spec. Existing specs are never re-ordered.
- **Acceptance:**
  - 15 Jan 2026 (a Thursday) with Saturday start: Week starting = 10 Jan 2026, Day name = Thu, Week of year = W03; with Monday start, Week starting = 12 Jan 2026
  - 1 Jan 2026 is in W01 for every start day
  - with fy_start 7: 15 Jan 2026 is FY2025-26, FQ3, Fiscal month 7; with fy_start 1: FY2026, FQ1, Fiscal month 1
  - Calendar sorts ["Mar", "Jan", "Feb"] → Jan, Feb, Mar, and ["Mon", "Sat", "Fri"] with Saturday start → Sat, Mon, Fri
  - Ranges sorts ["10%–11%", "No rate", "0%", "8%–9%", "> 15%", "Below 0%"] → Below 0%, 0%, 8%–9%, 10%–11%, > 15%, No rate
  - Ranges sorts ["Below 1 Crore", "25 Crore to 50 Crore", "1 Crore to 5 Crore", "Above 10 Crore", "5 Crore to 10 Crore"] → Below 1 Crore, 1 Crore to 5 Crore, 5 Crore to 10 Crore, Above 10 Crore, 25 Crore to 50 Crore
  - Python mirrors give the same orders
  - the Phase 7 replay test (goal 3) passes
</task>

<task id="W1" name="Split date step">
- New step `TransformStep::SplitDate`, with every field `#[serde(default)]`:
  - `column`
  - `parts: Vec<DatePart>`
  - `calendar: CalendarSettings`, copied from the settings when the step is made
  - `prefix: String`, default = the column name
- New columns are named "<prefix> <Part>" ("Open Date Year", "Open Date Month", …). A name that already exists is refused, naming the column, with the option to change the prefix in the dialog.
- **Dialog:** Transform ▸ Split date…, also offered as a button in the Column panel when the inspected column is a Date, and in Ctrl+K.
  - pick the column (Date and Datetime columns only; a text column shows the `<conventions>` message)
  - tick the parts. Default ticks: Year, Quarter, Month, Month-Year, Day name.
  - a preview of 5 rows
  - the calendar line from W0
- The step declares its text parts' order to `column_orders`, so a pivot of "Open Date Month" lists Jan…Dec without any choice.
- Steps panel label: "Split date: Open Date → 5 columns". Python export uses pandas `dt` with the same labels and week rules.
- **Acceptance:**
  - on a synthetic 1,000-row tab, every new column equals `dates.rs` part by part, and the 3 blank dates give 3 blanks in every part
  - a Datetime column gives the same parts as its date
  - a text date column is refused with the message
  - an existing "Open Date Year" is refused
  - a pivot by "Open Date Day name" with Saturday start lists Sat…Fri
  - the generated Python gives identical columns (run when pandas is present, as in C7)
  - an old macro without the step loads unchanged
</task>

<task id="W2" name="Date grouping in pivot, chart and filter (no new columns)">
The Power BI date hierarchy, without adding columns.
- **Pivot.** In the field list, a Date or Datetime column has an expander showing the parts of W0. Adding "Open Date › Month" to Rows, Columns or Filters (the same way as a column) adds a date field:
  - `PivotSpec.date_fields: Vec<DateFieldRef { name, column, part, calendar }>`, `#[serde(default)]`
  - its name is "Open Date · Month"; a real column with that name is refused
  - the engine adds the part as a column before grouping and drops it after
  - Rows/Columns accept these names like any field, with the order from W0
- **Chart.** The category, series and facet pickers list "Open Date › Year / Quarter / Month / Month-Year / Day name / …" under each date column (`ChartSpec.category_part`, `series_part`, `facet_part`, each `Option<DateFieldRef>`, serde default).
- **Filter.** The value picker of a date column gets a "By" choice: Date (today's list) / Year / Month-Year / Month / Quarter / Day name. Picking values by part stores `FilterCondition.part: Option<DateFieldRef>` (serde default None) and matches on the part. Example: keep Day name in {Fri, Sat}.
- **Python export:** the same parts through the W0 mirror.
- **Acceptance:**
  - a pivot by "Open Date · Month-Year" equals a pivot by the W1 column of the same name, row for row and in the same order
  - a chart by "Open Date › Quarter" has categories Q1…Q4 in order and the same totals as the pivot
  - a filter "Day name in Fri, Sat" keeps exactly the weekend rows of the synthetic tab
  - the totals of every date grouping equal the ungrouped total, blanks included as "(blank)"
  - old pivots, charts and filters load unchanged
  - render tests: the expander, the chart picker and the filter "By" choice work by real clicks at 3 sizes
</task>

<task id="W3" name="Rate bands without serial numbers">
- `RateBandDef` gets `numbered: bool`:
  - serde default **true**, so every saved step, preset and macro writes "09. 8%–9%" as before
  - the Rate profile dialog makes new steps with **false**, and has a tick box "Number the bands (09. 8%–9%)", off
- With `numbered = false` the labels are "0%", "8%–9%", "> 15%", "Below 0%" and "No rate". The step declares the band order to `column_orders` as a list in band order: Below 0%, 0%, the bands, the top band, No rate.
- The rate profile template (`charts/templates.rs` `rate_profile`):
  - orders by the declared order or by Ranges, not by a `BTreeMap` of the text
  - its finding strips a number prefix only when one is there (as today)
- The Python export writes the same labels.
- `docs/guide/rate-profile.md` and `chart-rate-profile.md` show the plain labels and explain the tick box.
- **Acceptance:**
  - the owner's example bands (0, 2–3 … 10–11, No rate) unnumbered come out in band order in the step result, a pivot, the chart, the Excel export and the Python export
  - an old macro with the step gives the numbered labels byte for byte
  - the template title reads "… sit at 8%–9%" in both forms
</task>

<task id="W4" name="Project files">
- **File menu:**
  - New project
  - Open project…
  - Open recent project (the last 8, in `settings.json` `recent_projects`)
  - Save project
  - Save project as…
  - Close project

  These are also in Ctrl+K. The window title becomes "MicroParquet v<version> — <project> — <file>". A "•" marks unsaved changes.
- **Save** writes the `.mpproj` described in `<architecture>`:
  - atomically: a temp file, then a rename
  - with `app_version` and `saved_at`
  - every tab whose recipe can be written
  - tabs that cannot be rebuilt are listed in the save dialog and left out, with the reason ("SQL result tab: save its query as a step first")
- **Open** shows the **Sources** dialog before anything runs, one row per file:
  - name and saved path
  - status, one of:
    - **Found** (same size and modified time)
    - **Changed** (the file is there, but its size or time differs, with both dates)
    - **Missing**
  - **Browse…** to relink one file
  - **Look in folder…** to relink every missing file by its file name

  Relinking to a different file is allowed and is the monthly workflow: last month's project on this month's file. The file's columns are checked against the saved column names. A missing column refuses that file, listing the columns, and the tabs built on it don't open.
- **Rebuild** runs on the worker in dependency order: files, then runs, then charts.
  - Progress shows "Opening 4 of 9: Gap ladder".
  - A tab that fails opens as a placeholder tab showing the reason in the main area (a new `TabSource::Unavailable { reason }`; it has no rows and can't be exported). Its recipe is kept, so saving the project again doesn't lose it. The other tabs still open.
  - At the end, one line: "9 tabs opened, 1 failed: Gap ladder — the file has no 'MATDAT' column".
  - The blake3 of each file is computed on the worker after the tabs open. A different hash from the one saved shows in the status line: "Loans.csv changed since the project was saved (saved 31 Oct, file 30 Nov)".
- **Unsaved changes.** Closing the app, opening another project or New project with unsaved changes asks: Save / Don't save / Cancel. Extend the existing close check (`close_requested`, `ui::jobs::CloseDecision`) rather than adding a second one.
- **Compatibility:**
  - a project with a newer `project_version` is refused with "This project was saved by a newer MicroParquet (vX). Update the app to open it."
  - unknown fields are ignored
  - tabs, macros, packs and runs are unaffected
- **Headless:** `--project <file> --export-dir <folder>` opens the project (no relink: a missing or column-short file stops the run with its name) and writes every tab as Parquet. A failing tab stops the run, and nothing is written.
- **Acceptance:**
  - a project of 6 tabs round-trips: file, file + steps, join, pivot tab, gap ladder run, chart tab. Every tab equals the original by hash of its rows, and the active tab, order, formats and chart specs are the same.
  - moving the project folder and its files together opens through the relative paths
  - relinking to a file with an extra column works; with a missing column it is refused with the list
  - a changed file shows the status line
  - the save → reopen → save cycle gives byte-identical JSON except `saved_at`
  - the unsaved-changes prompt appears by a real close request and Cancel keeps the app open
  - headless writes the same Parquet as the UI
  - render tests: the menus, the Sources dialog (Browse, Look in folder) and the prompt work by real clicks at 3 sizes
</task>

<task id="W5" name="Linked, editable chart tabs">
- A chart tab stores `source_tab` (the session id, and the project key in W4) and its `ChartSpec`. Its table is the chart's data, as today, so export still works. It is recalculated:
  - when the source's steps, filters or data change
  - after a project opens
- **"Edit chart"** on the chart tab opens the studio on the **source** tab with the chart tab's spec. The studio shows "Editing chart tab 'Chart: …'" with **Update chart tab** (replaces the spec) and **Open as new chart tab**. Values never re-aggregate the chart's own table, so "Sum of Sum of …" can't happen.
- The steps panel of a chart tab is read-only, with: "This tab draws a chart of 'Loans'. Change the data on that tab, or use Edit chart."
- **Source closed:** the chart keeps its last data, dimmed, with "The source tab 'Loans' is closed. Reopen it to edit or refresh this chart" and a **Reopen** button when the source is a file tab of the open project.
- **Phase 7 compatibility:** Phase 7 kept only the `ChartSpec` of a chart tab in `settings.json` and never reopened it as a tab, and Phase 8 does not either: those entries load and are saved back unchanged. A chart tab that has no source tab is frozen (never calculated again) and marked "Made by an older version; Edit chart opens it on its own table".
- **Acceptance:**
  - changing a filter on the source updates the chart tab's numbers to equal a new chart made from the source
  - Edit chart → change the aggregation → Update keeps one chart tab with the new numbers and the old title rule
  - Edit chart on the owner's case (Sum of Balance by Term period) shows "Sum of Balance", never "Sum of Sum of Balance"
  - closing the source dims the chart with the message; Reopen restores it
  - a Phase 7 `settings.json` with chart tabs loads and is saved back unchanged (as in Phase 7, it is not reopened as tabs), and a chart tab with no source tab stays frozen with the note
  - render tests by real clicks at 3 sizes
</task>

<task id="W6" name="Drill-down and Show rows">
- **Drill levels.** `ChartSpec.drill: Vec<DrillLevel>` (a column, or a `DateFieldRef`) and `drill_path: Vec<(String, String)>` (level, value), both serde default. In the studio: "Drill levels", a list to add, remove and reorder. When the category is a date (or a date part), the studio offers "Year › Quarter › Month › Day" in one click.
- **In the app** (studio, side panel, pop-out and chart tab):
  - clicking a mark of column, bar, stacked, grouped, line (category point), donut or heatmap (both levels) drills one level down, with the clicked value added to the path
  - a breadcrumb above the plot ("All › FDR › Dhaka") goes back to any level, as does the Back key
  - the subtitle states the path ("Filtered: Deposit Type = FDR")
  - at the last level a click does nothing and the tooltip says so
  - scatter, bubble, histogram, box and waterfall have no drill: the "Drill levels" section is hidden with its reason
- **Right-click a mark:**
  - **Drill down**
  - **Show rows**: a new tab with the source's file and steps plus a `FilterGroup` (keep matching) for the path and the clicked value. Its title is "Rows: Deposit Type = FDR, Branch = Dhaka", and its control total equals the mark's value when the chart sums that column.
  - **Keep only this** and **Exclude this**: add the filter to the chart's own filters
- **Pivot "Show rows".** Double-click or right-click a value cell (totals included) of a pivot tab. Show rows filters by every row and column field of that cell (date fields through `FilterCondition.part`) plus the pivot's filters.
- **Exports** (HTML, Excel, PDF, Python, chart book) draw the current level, with the path in the subtitle.
- **Acceptance:**
  - on a synthetic deposit tab, drill Deposit Type › Branch › Product: the totals of each level equal the clicked parent
  - the breadcrumb goes back to each level by real clicks
  - Show rows from a column gives a tab whose row count and sum equal the mark, and the same from a pivot cell, a pivot total and a date-part cell (Month-Year)
  - a date drill Year › Quarter › Month › Day on a Datetime column sums to the year
  - a chart tab saved at level 2 reopens at level 2 (project round trip)
  - exports at a level carry the path
  - render tests by real clicks at 3 sizes: a click drills, the breadcrumb goes back, the right-click menu items, pivot Show rows
</task>

<task id="W7" name="Colour picker and house colours">
- **Picker.** In the studio, "Colours" lists the chart's names with their swatch. Clicking a swatch (or right-click a mark ▸ Colour…) opens a picker with:
  - **Automatic**: the palette slot, as today
  - **24 swatches**: the 8 house hues in three shades. The Base shade pins the item to that palette **slot**, so it keeps the validated dark-mode pair. Light and Deep are fixed hex values, used in both themes.

    | Hue | Light | Base (slot) | Deep |
    | --- | --- | --- | --- |
    | Blue | `#6ea5ee` | slot 1 | `#014a94` |
    | Orange | `#ffa889` | slot 2 | `#ac3c01` |
    | Green | `#7dd6ac` | slot 3 | `#037952` |
    | Amber | `#ffd7a0` | slot 4 | `#ad7502` |
    | Pink | `#ffb9d0` | slot 5 | `#ac5074` |
    | Dark green | `#60a95a` | slot 6 | `#024e02` |
    | Violet | `#8a86eb` | slot 7 | `#3a2196` |
    | Red | `#fe8c85` | slot 8 | `#a31921` |

  - **Hex**: any `#rrggbb`
  - a tick box **Save as house colour**

  The pick is stored in `ColourMap.custom: Vec<(String, ColourPick)>` (serde default). It is honoured by the in-app, HTML, Excel, PDF and Python renderers and in chart books.
- **Warnings, not refusals.** The pick is the owner's call; the warning is the honest part. They appear under the picker and in the chart's notes:
  - "Hard to tell from 'Savings'": OKLab ΔE×100 below 10 against another item of this chart, in either theme (a small Rust OKLab function, no crate)
  - "Low contrast on the dark background": below 3:1 against the surface of a theme
  - "This is a status colour": within ΔE 5 of good, warning, serious or critical. Status colours mean a state and shouldn't name an item.
  - Text never takes the item's colour, as before.
- **House colours.** `portable_data/house_colours.json` holds `[{ "name": "TD", "pick": … }]`, matched trimmed and case-insensitive. It applies to every chart, template, pack chart and chart book unless the chart has its own pick. View ▸ House colours… lists, edits and removes them. A missing file means none. A broken file is refused at start-up with its line, and charts then use the palette.
- **Reset colours** (Phase 7) clears the chart's own picks and slots. House colours stay, and the button's tooltip says so.
- **Acceptance:**
  - a pick of Deep green on "TD" draws TD in `#037952` in all five renderers, and a Base pick follows the theme
  - a house colour for "TD" colours TD in a new chart and a template; a chart's own pick overrides it
  - the three warnings fire on known pairs and not on the default palette
  - Reset keeps the house colour
  - a filter or Top N never repaints a picked item
  - render tests: the picker, the hex field, the tick box and View ▸ House colours… by real clicks at 3 sizes, light and dark
</task>

<task id="W8" name="Guide and screenshots">
- New notes, following `<guide_notes>`: `split-date.md`, `date-grouping.md`, `calendar-settings.md`, `projects.md`, `drill-down.md`, `colours.md`.
- Updates: `chart.md` (linked chart tabs, Edit chart), `pivot.md` (date fields, Show rows), `filter.md` (filter by part), `custom-sort-lists.md` (Calendar and Ranges), `rate-profile.md` and `chart-rate-profile.md` (W3).
- `date-grouping.md` has a box, **"Balances are stocks"**: summing month-end balances across months gives a meaningless total. Group by Month-Year and read one month at a time, or wait for Phase 9's closing and average-balance measures.
- Screenshots from the render tests in `docs/guide/img/` (light and dark where the app draws it):
  - the Split date dialog
  - the pivot date expander
  - the filter "By" choice
  - the Sources dialog
  - a drill breadcrumb at level 2
  - the right-click menu
  - the colour picker with a warning
  - House colours
- **Acceptance:** a c8-style test checks every note has its sections, every picture it names exists, and the README lists the notes.
</task>

<task id="W10" name="Filters on a chart">
- Studio: the series, category and facet fields each get a value tick list (search, All/None, row counts), like the filter value picker. A "Filters on this chart" section takes any column with the tab filter's picker, date "By" included.
- Stored in ChartSpec.filters (added in W6, where Keep only this / Exclude this write), so the list shows those too and can remove them. Applied before aggregation; the source tab is unchanged.
- The subtitle states them ("Bank: Conventional"). Colours follow the entity, so a kept item keeps its colour. Honoured by every renderer, chart tabs, saved charts, chart books, projects, drill and Show rows.
- Acceptance:
  - Balance by Bucket, series Bank, keep Conventional: totals equal a chart of the source filtered to Bank = Conventional; the source tab's row count is unchanged; Conventional keeps its colour
  - a filter made by Keep only this appears in the list and can be removed there
  - a saved chart and a project round trip keep the filter; old charts load unchanged
  - Show rows from a filtered chart includes the chart's filters
  - render tests by real clicks at 3 sizes: the tick list, All/None, the "Filters on this chart" section
</task>

</tasks>

<guide_notes>
Every new note has these sections, in this order: What it does · When to use it · How to use it (numbered steps) · Example (from the tests, with real numbers) · What it refuses and why · Python export · Saved in (macro, project, settings) · Related tools. Plain English for a banker; no Rust or Polars terms.
</guide_notes>

<constraints>
- No new Rust crates. Use Polars `temporal` (already enabled), `serde_json`, `blake3`, `egui` and `printpdf`.
- Every new serialised field gets `#[serde(default)]`, with the defaults stated in the task (W3's `numbered` defaults to true). Old macros, packs, runs, settings, saved charts and chart tabs load unchanged, and the replay test of goal 3 proves the results are the same.
- No `unwrap()`, `expect()` or index panics on user data. Refuse with a plain message a banker can act on; never default a value (no "Unknown" date part, no 0 for a blank, no guessed date format).
- Colour literals live only in `src/charts/tokens.rs` and `theme.rs`, pinned by tests (the W7 swatches included).
- Test data is synthetic. Never commit anything from `portable_data`, and never a `.mpproj` that points outside `tests/fixtures`.
</constraints>

<workflow>
- Never run `cargo clean`. Run `cargo check` while editing and the full `cargo test` before each commit.
- One commit per task (`Phase 8 <ID>: …`), pushed after each. Guide notes go in the same commit.
- Tests go in `tests/phase8/`, compiled into the phase3 binary like `tests/phase7/`. Render tests go in `src/ui/render_tests_p8.rs`.
- Keep `docs/phase8-progress.md` updated, and resume from it.
- Never stop to ask. Take the option closest to this spec and the Phase 7 house style, record it in `<accepted_deviations>`, and list it in the report.
- If blocked three times on the same failure, mark the task BLOCKED and continue. Dependencies:
  - W1, W2 and W3 need W0.
  - W5 needs W4.
  - W6 needs W2 and W5.
  - W10 needs W6 (it reads and writes `ChartSpec.filters`).
  - W8 runs last.
- Sessions: A = W0, W1, W2, W3; B = W4, W5; C1 = W6, C2 = W7 + W10, C3 = W9 + W11, C4 = W8.
- One PR from `phase8` to main. Wait for Tests, and don't merge.
</workflow>

<report>
Reply in no more than 30 lines:
- a table: Task | Status | Tests added | Deviations
- the final `cargo test` summary
- questions
- a "look at by eye" list of screenshot paths: one per new dialog, plus a drilled chart and a picked colour in both themes
</report>

<accepted_deviations>
Session A (W0–W3) records its choices here and in `docs/phase8-progress.md`, for the owner to accept or change.

- W0-1: auto order applies to a chart's category only (the chart has one order; series stay by size, facets as they come).
- W0-2: the filter's value picker lists the values in the Calendar or Ranges order when every label parses (display only).
- W0-3: "Week of year" can reach W54 in a leap year whose 1 January is the last day of its week; Calendar ranks W01–W54.
- W0-4: Ranges reads a fixed grammar (Below/Under/Up to/</≤, Above/Over/>/≥, a unit, "–"/"-"/"to", "and above"/"or more");
  other labels follow in natural order; plain numbers alone do not make a column Ranges.
- W0-5: `actionlint` was refused by the session's permission check and not run; a test checks the workflow step instead.
- W0-6: the macro Python export stays Polars Python; the Calendar and Ranges mirrors are plain Python functions, run by the tests with python3.
- W0-7: two older tests that pinned 1.6.0 now expect 1.8.0.
- W1-1: Split date is in the Edit menu with the other transforms (no Transform menu exists), the Column panel and Ctrl+K; non-date columns are listed under "Not dates" and refused with the message.
- W1-2: a prefix equal to the column's name is stored empty (the default).
- W1-3: the Python export is Polars with a plain-Python part function, not pandas `dt`; the test runs the function on all 1,000 rows.
- W1-4: the preview is the first 5 rows of the page in view.
- W2-1: date fields are stored as "Open Date · Month"; pickers show "Open Date › Month" (chart) and "› Month" (pivot list); a part is offered for Rows, Columns and Filters, not Values.
- W2-2: a date field's value lists are read through a one-part Split date step on the worker; nothing is added to the tab.
- W2-3: "By" offers Date, Year, Month-Year, Month, Quarter, Day name; changing it clears the picked values.
- W2-4: chart picker lists are 460 pt tall; series and facet orders are unchanged from Phase 7.
- W2-5: the pivot has no Python translation (as before); the filter "By" choice has one.
- W3-1: the rate profile template orders plain labels by Ranges (the same order as the declared list); numbered labels keep the Natural order.
- W3-2: built-in and saved presets stay numbered; the dialog's tick box (off for a new step, the step's own when editing) decides.
- W3-3: the Phase 3 dialog test and the Phase 4 guide example now expect the plain labels a new step makes; the guide picture uses a plain-label sample.
- W3-4: the Phase 8 overlap checks measure texts by their laid-out rows, not egui's row-wide galley rectangle.

Owner decisions on Session A (given with the Session B brief):
- W0-1 to W3-4 are accepted.
- Q1: `ChartSpec.series_order` and `facet_order` (`Option<SortOrder>`, serde default `None` = today's behaviour), filled by the W0 auto-order rule when a field is added; existing charts never change. Test: a new chart with series "Open Date › Year" lists 2024, 2025, 2026; a Phase 7 saved chart keeps its order. Built (Session B2), `tests/phase8/q.rs`. A series or facet picked in the studio gets its order from the field (a date part: its own order, Natural for a number such as the year; a column: the steps' order, else Calendar or Ranges when its labels parse). A column whose labels give no order keeps `None` (series by size, panels largest first), so only a field that was added changes. The series that stay are still the largest; `series_order` then lists them. Suggestions, templates and opened charts are not touched.
- Q2: the Python Tenor mirror uses the app's day count (month = 365/12 days, year = 365). Test against the app on 12M, 1Y, 18M, 2Y, 30D, 1M. Built (Session B2), `tests/phase8/q.rs`; the mirror also reads the units "mo" and "mos" like the app.
- Q3: plain-Python date parts in the Polars export: accepted.

Session B (W4) choices:
- W4-1: a pivot tab is saved as its source's recipe plus its steps (the pivot is the last step), not as `ToolRun::PivotTab`: it opens even when the tab it was made from has changed since. Every tab on a file is a File recipe with all its steps.
- W4-2: a tool's result tab (join, stack, reconcile, banking tools) is its `ToolRun` with the sources named by project key, plus the steps added to it afterwards. Its sources must be open tabs with the same source and steps (as for "Save run"); if not, the tab is left out with that message.
- W4-3: the recipes are resolved on the UI thread one tab at a time (they read headers and small rule files); each tab is built on the worker, so "Opening 4 of 9: Gap ladder" follows the worker. A tab waits for the tabs it is built on.
- W4-4: (replaced by the owner's decision) Save is a worker job: it builds the recipes and hashes the files it has not hashed before (blake3, remembering size, time and hash) with progress "Hashing Loans.csv 40%"; the window writes the file when the job's result arrives, and a second Save while one runs is refused.
- W4-5: "unsaved" is asked only when a project is open, or (for New and Open) when tabs are open outside any project. It compares the tabs' sources (settings, not the files' sizes and dates), steps, formats, summary, control column, chart tabs and the active tab, every 0.4 s and at the moment of a close. The side panel's own chart choices are saved but do not set the "•".
- W4-6: "Open recent project" is a submenu of File and, in Ctrl+K, a small list dialog.
- W4-7: a chart tab is saved as its `ChartSpec` and the key of the tab it draws (`ChartTabInfo.source_tab`) and is calculated again on open; W5 builds the linked chart tab on this. A chart whose source tab is closed is left out with the reason.
- W4-8: the column check is per file: a CSV's saved names are its schema's column names, a Parquet file's are its own. One missing column refuses the file and every tab built on it. Extra columns are fine.
- W4-9: a tab that cannot open is a placeholder with no steps; its recipe stays in memory and is written again by Save. Export, copy, macro and Python commands are greyed for it.
- W4-10: the "changed since saved" line leaves out files that were relinked, and follows "N tabs opened.".
- W4-11: Open closes the session's tabs only after "Open" is pressed in the Sources dialog; Cancel leaves everything as it was.
- W4-12: `--project` writes `<tab title>.parquet` (a second title gets "(2)") through a staging folder in `--export-dir`; a step that cannot run stops the run like a failing tab.
- W4-13: not done in W4: the guide note `projects.md` and screenshots (W8); `cargo build --release` was not run in that session; it was run and passed at the end of B2.

Owner decisions on Session B (W4):
- W4-1 to W4-13 are accepted.
- Q1 and Q2 are built (see above).
- Project save: the blake3 hash runs on the worker with progress ("Hashing Loans.csv 40%"), reusing the cache; the UI never blocks. A test checks that save is dispatched as a worker job. (This replaces W4-4.)

Session B2 (W5) choices:
- W5-1: a chart tab is linked by `ChartTabInfo.source_tab` (the session id; in a project the key, as in W4-7). It remembers the source's signature (`tab_jobs::source_signature`: the source, the steps that run and, for a file, its size and time) and looks at it every 0.4 s; a different signature calculates the chart again on the worker from the source tab (`project::chart_tab_source`, the studio's own calculation), then replaces the chart tab's numbers, table and, when it still had the rule's title ("Chart: " and the chart's title), its title. Nothing is calculated when nothing changed.
- W5-2: a calculation that fails (a step removed the column, for example) keeps the last numbers and says why in red above the chart; it is not tried again until the source changes once more.
- W5-3: "Update chart tab" puts the studio's spec in the chart tab and the worker calculates it from the source (not from the studio's numbers); the app then shows the chart tab. "Open as new chart tab" is the existing "Open as tab". The banner is in the studio (side panel and pop-out); a switch to another tab ends the editing.
- W5-4: Reopen is offered when the closed source was a file tab of the open project (`ClosedSource.key` and a file outside the temporary folder); any other closed source gets the message only. Edit chart is greyed while the source is closed. A chart tab whose source is closed is left out of a saved project (W4-7).
- W5-5: a chart tab with no source tab (made by Phase 7, which kept only its spec in `settings.json`; the app never restored those entries, and still does not) is frozen: it is never calculated again, says "Made by an older version; Edit chart opens it on its own table", and Edit chart opens the studio on the chart tab's own table without loading its spec (the table's columns are not the spec's); only "Open as new chart tab" is offered.
- W5-6: the project's "unsaved" fingerprint leaves out a chart tab's own source (a temporary table made again from the source). (Its second sentence, that the step commands were not greyed on a chart tab, is replaced by the owner's decision W5-7.)

Owner decisions on Session B2 (W5):
- Q1, Q2, the W4 follow-ups and W5-1 to W5-6 are accepted.
- Q2: the Python Tenor mirror accepts exactly the labels the app accepts. The app reads "mo", "mos", "mon" and "mons" (`sort_lists::parse_tenor_days`), so they stay in the mirror and the test now lists them. The test (`tests/phase8/q.rs`) puts every unit word of the app, of the Ranges grammar and some near misses, each in four spellings, to the app and to the mirror, and they must agree on every label. This found one difference, now fixed: a number too big to hold (400 nines and a unit) was refused by the app and read as infinity by the mirror.
- W5-7 (replaces the second sentence of W5-6): on a chart tab the step commands are greyed, in the Edit menu, the toolbar and Ctrl+K, with the text of the steps panel's note ("This tab draws a chart of 'Loans'. Change the data on that tab, or use Edit chart.") as the tooltip. They are the commands that add a step to the active tab in place (`commands::adds_step_to_active_tab`: Filter, Sort, Columns, Calc column, Pivot, Conditional column, Counterparty normaliser, Clean line breaks, Split date, Maturity bucketer, Rate profile, Duration, FTP, Provision, Classify overdue, IFRS 9 ECL, Limits). Commands that open a new tab or change nothing stay (Join, Copy, exports, Chart, the view commands, and the banking tools that make a result tab). In Ctrl+K the row shows "Not on a chart tab." and the whole note on hover and in the status line. As a backstop `push_step_to_active_tab` refuses a step for a chart tab (other doors such as the Column panel's button) with the same note. Test: `w5_the_step_commands_are_greyed_on_a_chart_tab_in_the_edit_menu_and_ctrl_k_by_real_clicks_at_three_sizes` and `w5_the_step_commands_are_greyed_on_a_chart_tab_and_every_other_command_is_as_it_was`.
- Phase 7 chart tabs: the current behaviour is correct (Phase 7 never reopened them). The W5 compatibility line and its acceptance item are reworded above.
- The W5 acceptance table (item to test) is in `docs/phase8-progress.md`. Writing it found one item with no test, the chart tab following a change of the source's *file* ("or data change"); added: `w5_a_source_looks_different_when_its_steps_or_its_file_change_and_the_same_when_nothing_did` and `w5_a_file_that_changes_under_the_source_updates_the_chart_tab_to_equal_a_new_chart`.

Session C (W6) choices:
- W6-1: `ChartSpec.drill` holds **all** the levels, top first (`DrillLevel { name, part }`, a column or a date part); while it is not empty the chart's `category` is the level it is at (`drill[drill_path.len()]`), kept by `ChartSpec::sync_level`. Every renderer therefore reads the current level from `category` unchanged, and a spec saved at level 2 (a chart tab, a project, a saved chart) calculates to level 2 again. One level is no drill (stored as none). In the studio the Category picker names the top level: picking another category starts the chart again at its top.
- W6-2: a chart had no filters of its own, so `ChartSpec.filters` (`Vec<FilterCondition>`, serde default) is new: "Keep only this" adds an "in list" and "Exclude this" a "not in list" of the category value at the level shown. The drill path, these filters and the rows behind a mark all go through one function, `engine::filter_by_conditions` (a Filter group step, keep matching, all of the conditions), so the chart at a level and its Show rows tab cannot disagree. A date part is read through `FilterCondition.part`, a blank as "(blank)".
- W6-3: Show rows of a mark opens a tab with the source tab's steps that run plus one Filter group: the chart's own filters, the path, the mark's category and, when the mark has them, its series and its panel (small multiples). A stacked or grouped segment is therefore the rows of that cell; a click that drills goes by the category alone. Title: "Rows: Deposit Type = FDR, Branch = Dhaka" (path, category, series, panel).
- W6-4: the kinds that drill: column, bar, stacked, grouped, line (the category point), area (like a line), donut and heatmap. A heatmap click drills the category (x) and the cell's series value is part of its Show rows (the spec says "both levels"; the series is not a drill level of its own). Scatter, bubble, histogram, box, waterfall and the HTML-only kinds have no drill: the studio shows "Drill levels: <reason>" in place of the section.
- W6-5: "Other" (Top N's remainder) is several values: a drill, Keep, Exclude and Show rows on it are refused with that message. A series called "Other" likewise. A series or panel column cannot also be a drill level (the chart says so).
- W6-6: the Back key is Backspace or Alt+Left, on the active chart tab, or on the studio's chart while the pointer is over it, and never while a text box, a dialog or the search has the keyboard.
- W6-7: a chart tab drills by asking the worker for the next level from its source (`ChartTabInfo.next_spec`); the numbers on the screen stay the calculated ones, with their own spec, until the new ones arrive. A chart tab whose source is closed, or that was made before it had one, cannot be calculated again, so its menu is greyed with the reason and a click does nothing. A drilled chart tab changes the project's "•" and is saved at its level.
- W6-8: Show rows of a pivot cell is a worker job (`JobKind::ShowRows`): the pivot is built again to learn which values the cell's row and column stand for (`PivotBuild.row_keys`, `col_keys`), refused when its row count is not the screen's. A subtotal row restricts the fields it covers, a Total column and the grand total none; the pivot's own filters are always part of it; a row label shows no rows. It is offered when the pivot is the tab's last step and every step runs. The new tab has the steps before the pivot and one Filter group.
- W6-9: the tooltip of a mark of a chart with drill levels has one more line: "Click to drill into Month", or "Last level: a click does nothing". A chart without drill levels is as before. The breadcrumb ends with a muted "by Month" naming the level the chart shows.
- W6-10: a click down or up the same levels keeps the colour map (it belongs to the columns, and they are the same); another series column starts the colours again, as before.
- W6-11: the one test of Phase 6 that lists the studio sections allowed to be out of view in the small pop-out window (`f2_…`) now names "Drill levels". Not done in W6: the guide note `drill-down.md` and the screenshots (W8); `cargo build --release` was not run in this session.

Owner decisions on Session C (C1, W6):
- W5-7 and W6-1 to W6-11 are accepted.
- Pivot Show rows also works when steps follow the pivot, if those steps keep the pivot's row and column key columns intact (filter, sort, format, hide, add column). If a later step renames or drops a key column, or groups or pivots again, Show rows is greyed with a plain reason. Built (follow-ups): W6-12.
- The Ctrl+K overlap of "Chart this result" and its reason at 1219 pt is fixed: a row's reason has a line of its own under the title and wraps, never clipped (W6-13).

Session C (W6 follow-ups) choices:
- W6-12: `pivot_table::pivot_rows_of(steps, steps_run)` reads the steps after a tab's last pivot. A step that keeps the pivot's row fields and rows (sort, filter, a new column, Split date, a column selection that keeps the row fields, a rename of another column) leaves Show rows on, and a cell is found again by its row fields' values (`pivot_row_of_shown`): the worker builds the pivot, applies the later steps to it, and matches the clicked row of the screen to exactly one pivot row (none or two is refused in words). Show rows is off for the whole tab, with a reason that names the step and the field, when a later step renames, drops (a column selection that leaves it out), writes over or sets to Number a row field, groups or pivots again (Group by, SQL, the banking tools). The "key columns" are read as the pivot's row fields; the value columns carry the column keys in their headings, and a value column that a later step renamed, rewrote or added is greyed **per cell** ("added, renamed or changed it") while the pivot's own columns keep working; a column dropped or hidden has no cell to click. A step that is off does not count, and a second pivot is the tab's pivot.
- W6-13: the Ctrl+K row puts its reason on a second line, wrapped, under the title (the group name stays at the right of the title); the other palettes and menus are unchanged. The W5 test of the Split date row measures its band to include that line.

Session C2 (W7) choices:
- W7-1: `ColourMap.custom: Vec<(String, ColourPick)>` (serde default, left out of the file when empty, so a chart with no picks is written as before). `ColourPick` is `Slot` (the Base shade: a palette slot, with its dark pair) or `Hex` (the Light and Deep shades and any hex: one colour in both themes, written "#rrggbb"; a bad one is refused when the file is read). `ColourMap::get` reads the chart's own pick, then the house colour, then the slot; a name with a house colour is given no slot, so Reset colours leaves it alone.
- W7-2: the house colours are read from `portable_data/house_colours.json` by each thread (`colours::house`, a thread-local copy that looks at the file's size and time every 0.5 s, and at once after the dialog saves). Names are matched trimmed and case-insensitive. A missing file is none; a broken one (bad hex, a slot past 8, a name that is empty or twice) is refused with its line in the status line at start-up and in View ▸ House colours…, and the charts use the palette. Tests change the folder with `rules::set_data_dir_override`.
- W7-3: the warnings are for items that have a pick or a house colour, never for the automatic palette. A Base pick is the palette itself, so it is checked only for "Hard to tell from 'X'" (the palette's own orange and red are 7.1 apart; the dark green in the dark set is the status "good"); the Light, Deep and hex picks get all three: "Hard to tell from" (OKLab ΔE×100 below 10 in either theme), "Low contrast on the dark background" or "…light background" (below 3:1 against that theme's surface; the spec names only the dark one, the light is the same rule), and "This is a status colour (good)…" (ΔE below 5 of good, warning, serious or critical).
- W7-4: the warnings are under the picker and in the chart's notes (the footnote of the app, the PDF, the HTML page and the workbook: "Colour of 'DPS': low contrast on the dark background."). The Python script has no notes.
- W7-5: the picker is a dialog (not a pop-up): Automatic, the 24 swatches (Light, Base and Deep of each hue, Base drawn in the colour of the theme), the hex box with "Use hex", the tick box "Save as house colour" (the pick made while it is on is also written to the house file), the warnings and Done. A swatch is named on a line under the grid when the pointer is over it; a tooltip was tried and at 1219 pt it sat under the pointer and swallowed the click.
- W7-6: the studio's "Colours (n)" section lists the names with their swatch and a tag ("your pick", "house colour"); the names are the series, or the categories of a donut. A chart of one colour (no series) says so. Right-click ▸ Colour… is in the mark menu, which the kinds that drill have; the other kinds use the studio list. A chart tab takes the pick without being calculated again, keeps it when a calculation that was running arrives, and writes it to `settings.json` like Reset colours does.
- W7-7: View ▸ House colours… lists the colours (swatch, name, hex, Remove), adds a name (the picker opens for it) and changes one by its swatch. "Reset colours" has the tooltip "House colours stay."
- Not done in W7: the guide note `colours.md` and the screenshots (W8); `cargo build --release` was not run in this session.
</accepted_deviations>
