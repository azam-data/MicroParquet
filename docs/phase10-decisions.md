# Phase 10 decisions

Taken without asking the owner (he asked for a list at the end of the phase). Newest last. Each is also in `docs/phase10-spec.md` <accepted_deviations> when it departs from the spec.

## S1 (D0, C0)
- D0-1 The Phase 9 baseline was made in the main checkout at the Phase 9 merge commit with the new files stashed, not in a separate worktree.
- D0-2 One dashboard per project (`Project.dashboard`, written only when present); the app keeps it in `ProjectState` so saving never drops it.
- D0-3 Grid items have no rect (their place is their order); a Free item holds its rect. Rows share the page height; no margin at the page edge.
- D0-4 Drill state is a sorted map so saved bytes are stable.
- C0-1 The month-end note is for months only, finished months only, on Period amount and annualised values.

## S2 (D1)
- D1-1 The dashboard is a **view** reached by the Dashboard command (Charts menu, Ctrl+K) and a "Dashboard" entry at the end of the tab strip, not a table tab: tabs in this app are tables with a source and steps, and a dashboard has neither. Any real tab click closes the view. (Spec: "dashboard tab".)
- D1-2 A page's items are put in the grid in the order added; "Move earlier/later" reorders. 1 to 4 per row is a per-page choice; Free layout comes in D4.
- D1-3 A filter reaches an item when the item's data tab is the filter's tab: for a chart tab, the tab it draws; for a pivot tab, the pivot's own data tab (model pivot), else the tab whose table the pivot reads. Otherwise the item says "Filter 'X' doesn't apply to this item".
- D1-4 The page's pick is added to a chart's own filters, and replaces the pivot's own filter on the same field (the page is the reader's latest word).
- D1-5 A filter's dropdown values are text, sorted alphabetically (Calendar fields too: month order comes with the Calendar sort in a later step), capped at 2,000 with a note.
- D1-6 A saved pick that is no longer in the data stays and is shown "(not in data)".
- D1-7 Items narrower than 200 pt (3–4 per row on a small window) clip their chart (the chart's minimum width); the default is 2 per row.
- D1-8 KPI items show "KPI cards come with the next step" until D2.

## S3 (D2)
- D2-1 A card's number is worked out by the pivot's own code: a Calendar Month-Year row filtered to the as-of month (and to the 12 months ending there for the sparkline), with the measure and, for a comparison, the same measure shifted by a prior period or a year. A card's data tab must be linked to the calendar; if not, it says what to do.
- D2-2 The period is always the month of the as-of date (the model calendar's finest period); quarter and year cards come later if the owner asks.
- D2-3 The editor lives in the properties panel when a card is selected (not a separate dialog). Page settings hide while an item is selected (a "Page settings" button brings them back), so the panel fits a 686 pt window.
- D2-4 Choices: Shows = a named measure (a column-of-a-data-tab source is supported in the file and the worker, but the editor offers measures only for now).
- D2-5 Automatic unit = Crore for money (or the dashboard's own default `default_kpi_unit`), Percent for a ratio. A ratio's change is always in percentage points; money's change is in the card's unit plus a percent.
- D2-6 Changing a card's unit, decimals, limits, direction or title does not calculate it again (they change the display, not the stored number).
- D2-7 A month with no rows at all has no pivot row; the card says "No balance in Nov 2026 for 'Accounts' (last date 31 Oct 2026)" itself, in the pivot's own words.
- D2-8 Limits are in the card's unit; the first that matches colours the number (amber, red or green). "Below 80.00" is strictly below.

## S4 (D3)
- D3-1 On a page a left click filters the page (spec: "click a mark to cross-filter"); drill moves to the right-click menu, which has "Filter the page by this" and "Drill down" only. The chart tab's own menu items (Show rows, Keep only, Exclude, Colour) are left out, because a page must not change the chart tab.
- D3-2 A click reaches every item whose data tab is the clicked field's data tab (same rule as a page filter). Another data tab, even one linked to the same lookup, says "Filter 'Region' doesn't apply to this item" (a link-aware version can come later).
- D3-3 The clicked item keeps all its marks and shows "● Selected: Region = Dhaka" under its title; the bar itself is not recoloured (words and a mark, never colour alone).
- D3-4 Refused with a plain message: "(Other)", a blank group, a date part (use a page filter on the calendar), the grand total, an item with no data tab.
- D3-5 A pivot's row labels are clickable (flat: any Rows column; Nested: the label at its level); subtotal and grand-total rows are not.
- D3-6 Drill is per item and starts from the chart tab's own level. The path is stored as the clicked values; Back and the breadcrumb cut it. A path past the last level is ignored.
- D3-7 The chip "Region = Dhaka ×" sits under the page filters; clicking it clears the click. "Clear" clears picks, click and drills as before.
- D3-8 Old Phase 10 files whose cross pick has only a dimension and a value still open (new fields have defaults); such a pick filters nothing until clicked again because its data tab is empty.

## S5 (D4)
- D4-1 A bar of three dots above each item's title is the move handle on a Free page (a table or chart under the pointer would take a body drag). The body drag works too where nothing under the pointer takes it.
- D4-2 A move or resize that would overlap another item (overlap off) is not taken: the item stays at the last allowed place of the drag, so it never jumps onto a neighbour. A place off the page is clamped inside it; minimum size 60 × 40 units.
- D4-3 Grid to Free keeps each item's exact cell (not snapped to 10, so going back is lossless). Free to Grid clears the rects and returns to the items-per-row the page had (saved as `grid_per_row`); items go to the cells of their order.
- D4-4 The layout dialog always shows its message first (what moves, naming items that left the grid) and has "Switch layout" / "Keep layout".
- D4-5 A new item on a Free page takes the first free 300 × 200 place scanning from the top left; a full page refuses in words (or places it at the top left when overlap is on).
- D4-6 Arrow keys work on the selected item when no text box has the focus; Shift+arrows grow or shrink the bottom right corner by 10 units.
- D4-7 `allow_overlap` and `grid_per_row` are written only when set, so a Grid page's saved bytes are unchanged.

## S6 (D5)
- D5-1 `image` 0.25.10 (the version already locked through eframe), `default-features = false`, features `png` and `jpeg`. The only new crates are `zune-jpeg` 0.5.15 and `zune-core` 0.5.3; `cargo check` took 29 s with them. No other feature was needed.
- D5-2 (changed in S7 on the coordinator's note that nothing may be stretched) A page keeps 1000 units of width and takes its height from the picture: 1920×1080 gives 1000 × 562.5, 16:10 gives 625 as before. Slots stay in picture coordinates (page units of that shape) and must be inside it. The height is saved as `page_h` only when it is not 625, so old pages keep their bytes. Taking a design away gives 625 back. Items that no longer fit a shorter page are pulled inside (never lost). Exports should use the page's shape (R/X sessions).
- D5-3 The project copy sits inside the dashboard as `templates` (base64 of the PNG, its blake3, its slots), because the project file is one JSON file; no base64 crate was added (a 30-line encoder, tested for every length 0–39). A 3 MB picture makes the project about 4 MB bigger.
- D5-4 Resolution rule: the data-folder copy is used when it exists and its hash is the project's; no such template there → the project's copy; another picture under the same name → the project's copy plus a warning in the page settings. A damaged project copy (hash mismatch) is refused in words.
- D5-5 Using a design makes the page Free (items that fill a slot sit where the slot is); "None" takes the background and slots away and leaves the items where they are.
- D5-6 A slot is an empty place until an item fills it; clicking it lists only what its type takes. Removing the item makes the slot empty again. A design is drawn behind the items; items keep opaque surfaces, so on a Grid page the picture shows around them.
- D5-7 Slots must be at least 60 × 40 units, inside the page, and named differently. A name that cannot be a folder name is refused.

## S7 (R1)
- R1-2 The browser-side cross-filter cube (values grouped by the page's filter and click dimensions) is built in R2 with the page that uses it; R1 holds the answers per filter state and drill path, the additive/not-additive split, the estimate and the email rule. R2 adds the cube's bytes to the estimate and its tests keep the 10% check.
- R1-3 The file's filters are single-choice dropdowns ("All" plus one value), so a multi-select page filter is exported as single picks. States are counted per item over the filters that reach it (a filter on another data tab adds no states to that item).
- R1-4 Numbers in the answers are in the unit they are shown in (Crore) at the decimals they are shown with, as fixed-decimal JSON text (1000.00): 8 bytes a number for 4-digit Crore values, which is the spec's 3,120 bytes for 390 numbers. Tables and cards keep their own units.
- R1-5 A chart counts as additive for clicks when every measure is a Sum or Count with no time aggregation and no share transform; a pivot when every value is a Sum or Count with no time aggregation and no "percent of a total"; a KPI card never ("a card is one number"). The rest are listed by name with the reason in the export dialog.
- R1-6 An item over 20,000 numbers stops the export with the spec's three things to cut; an item whose number of states alone is over the limit is refused before any answer is computed.
- R1-7 The suggestions when the email rule refuses come in the spec's order and leave out what does not apply (drill levels with no drill, pages with one page, the design picture with none): fewer filter values, fewer drill levels, Top N on the biggest chart, fewer pages, leave out the design picture.

## S8 (R2)
- R2-1 Cross-filter in the file uses precomputed answers, not a cube (this replaces R1-2). For each chart that adds up (R1-5), each top-level category and each other additive item on the same data tab, the file holds the item again with that value added, for every filter state of that item, all through `compute_item`. A click therefore shows exactly what the app shows. The bytes are in the answers and so in the estimate; the 20,000-number cap per item counts them and stops the export with the same three things to cut.
- R2-2 A click filters only from an item's top level, only from charts (a pivot cell does not click in the file). Left click filters the page (click again or the chip "Wing = Islami ×" clears it); right click gives "Filter the page by this" and "Drill down". The clicked item keeps all its bars and dims the others, and says "● Selected: …". Items that do not add up say "Not linked to clicks"; items on another data tab are left as they are. A click clears every drill on the page.
- R2-3 The file draws bar (grouped, stacked, 100%, lying down), line, area and donut. Any other chart type, a chart with several measures, a bubble size or small multiples is shown as a table of its numbers with a line saying so, never as a different chart.
- R2-4 KPI cards in the file show the number (Crore for amounts, percent for ratios), the words "Up/Down/No change by … on the earlier period (Oct 2026)", and a sparkline; thresholds and colours are not in the file yet (arrow plus words carry the meaning).
- R2-5 A pivot's numbers are shown grouped in thousands with 2 decimals as computed; its own number format is not carried into the file. Nested pivots open at the top level, with ▸/▾ to expand and collapse a row.
- R2-6 Each page is as tall as its own shape (the design picture's, 1000 wide), items sit by their rect in percent, and the page scales with the window. The picture is inlined as a data URI and counted in the estimate (`estimate_page`, within 10% of the file in the test).
- R2-7 The file carries a read-only hook `window.__mp` (what each item shows, bar positions, errors) so the browser test can compare numbers with `ItemData`. It changes nothing.
- R2-8 Table answers now carry every label column joined with " · " (R1 kept only the first).

## S8b (R2 corrections from the coordinator; they replace R2-3, R2-4 and part of R2-5)
- R2-9 KPI cards in the file are the app's card: the text, change, arrow, words, tone, threshold and sparkline come from `kpi::view` (one code path), with the dashboard's KPI unit (default Crore). The number is coloured by its threshold (amber/red/green) and the change line by its tone (good/bad), in the house status colours (`STATUS_*`, the same in both themes); arrow and words say the same in text and the card carries an aria-label.
- R2-10 A chart's numbers are held in the unit the app shows each measure in (`value_kind`): money in the item's unit (Crore), a share with one decimal, a count whole, any other number (average, weighted average) as it is. R1 divided every number by the money unit, which would have shown a count of 1,200 accounts as 0.00 Crore; fixed and pinned by a test.
- R2-11 Several measures are drawn as real bars/lines/areas: a series per measure (and per series within it); measures in the first measure's unit share the left axis, the others use a right axis labelled with their unit. Stacking stays within a measure.
- R2-12 Waterfall is drawn as a real bridge from the app's own steps (`waterfall_from_data`, including the check that it adds up; a bridge that does not add up shows the app's message): start/end in the chart colour, rises good, falls critical. A waterfall bar does not filter the page.
- R2-13 Still shown as a table with a note: scatter, bubble, heatmap, box plot, histogram, sankey, treemap and facets. Coordinator asked for a table only for scatter and facets; the rest are listed so the owner can decide which to add. OPEN QUESTION for the owner.
- R2-14 Pivot number formats: the app's dashboard pivot shows value cells as comma-grouped numbers with 2 decimals and no unit (`format_with_commas(v, 2)`), and a pivot has no unit setting in the model, so the file matches the app. A per-item pivot unit (Crore) needs a setting in the app first; not invented in the file. OPEN QUESTION for the owner.

## S9a (coordinator answers: heatmap, histogram, pivot unit)
- R2-15 Heatmap and histogram are drawn in the HTML file. The heatmap uses the app's own matrix (`heatmap()`: its row and column order, its transform, its neutral zero cells) and the 13-step sequential ramp of each theme; the histogram uses the app's bins and counts (edges as computed, counts whole). Neither filters the page. Bubble, box plot, sankey, treemap, scatter and facets stay as a table with a note (this closes R2-13).
- R2-16 Pivot items have a unit: `DashItem.unit` (`#[serde(default, skip_serializing_if = "Option::is_none")]`, `None` = Crore; Crore, Lakh, Million, Thousand, Absolute). Item settings of a pivot item have "Unit for money columns". Money columns (Sum, Min, Max, time measures) are divided by the unit with 2 decimals and their heading says it ("Sum of Balance (Cr)"); counts and distinct counts are whole; averages and weighted averages as they are; percent-change and ratio measures as they are with "(%)". `dashboard::{cell_kind, cell_scale, heading_with_unit}` is the one rule: the app's table and the HTML use it now, and the PPTX and PDF use it when they are built. Old projects have no unit and now show Crore: this changes how an existing pivot item looks (it showed raw BDT before the setting), which is what the owner asked for. This replaces R2-14.
- R2-17 `zip` 2.6.1 is already in the build through `rust_xlsxwriter`, so adding it (pinned `=2.6.1`, `default-features = false`, `deflate`) adds no crate and no build time; `Cargo.lock` gains only the `zip` entry under `micro_parquet`.

## S9 (X1)
- X1-1 `src/pptx.rs` writes the package with `zip` (Deflate). One item is one slide; a pivot longer than 16 rows continues on the next slide ("Steps (2 of 3)") because a PowerPoint table does not page. Slide 13.333 × 7.5 in, blank layout, a text title, the item, and a footnote line with the as-of date and the filters in force ("As of 31 Oct 2026 · Filters: Wing = Islami") plus the item's notes.
- X1-2 Charts are real DrawingML chart parts with an embedded workbook (`rust_xlsxwriter::save_to_buffer`), series pointing at `Sheet1!$B$2:$B$n`: bar (clustered, stacked, 100%, lying down), line, area, doughnut, histogram (bars of the app's bins), waterfall (stacked bar with a hidden base series; a bridge that crosses zero becomes a table with a note). Series colours are the chart palette's slots in order; numbers are in the unit the app shows each measure in (same rule as the HTML file).
- X1-3 Shown as a native table with a note instead of a chart: a chart whose measures are in different units (one PowerPoint axis cannot honestly show two; the HTML file has a second axis), heatmap, and every type the HTML file also shows as a table (scatter, bubble, box plot, sankey, treemap, facets).
- X1-4 A pivot is a native table with the item's unit (R2-16), nested rows indented by level, subtotals and the grand total in bold. A KPI card is a rounded rectangle with the app's text; the number carries its threshold colour and the change its tone colour; arrow and words are text. The sparkline is not drawn in the PPTX (the number, change and words are).
- X1-5 The design picture is the slide background only when the plan carries it (the export dialog's "Use design as slide background" will set it); otherwise slides are plain. The package is refused above 4 GB with a plain message; an empty dashboard is refused in words.
- X1-6 LibreOffice (headless, to PDF) converts the file in the test when it is installed (it is here); the owner checks PowerPoint by eye.
- GAP found: no export dialog exists in the app yet (R1, R2, X1 are engines). I will build one dialog for HTML, PPTX and PDF in S10 after the PDF pages, then S11 documents it.

## S10a (X2 and the PPTX corrections from the coordinator)
- X1-7 (replaces X1-3 in part) A PowerPoint chart whose measures are in different units is a real combo chart: the first measure's unit on the left axis, every measure in another unit in a second chart group on a right value axis (`crosses max`, titled with its unit), as in the HTML file. Heatmap and the table types stay tables with a note.
- X1-8 A KPI card slide carries its sparkline as a small native line chart (no axes, gridlines or legend; a period with no value is a gap, never a zero) bottom right of the card. This replaces "the sparkline is not drawn" in X1-4.
- X2-1 `src/dashboard_pdf.rs`: one A4 landscape page per dashboard page, 8 mm margins, a heading line, the page's items, and a footer ("As of … · Filters: …   ·   Page n of N"). The page's 1000-unit width maps to the paper with one scale (`page_fit`), centred, so a free layout keeps its shape; the report lists each item's place in mm (the test reads the paper with `pdftotext -bbox` and finds the titles within 1 mm of it).
- X2-2 Charts are drawn by the Phase 7 chart PDF code (`draw_chart`, vectors, embedded Source Sans 3), so every chart type the app draws is drawn, with the item's unit. A pivot is drawn with the chart data-table drawer (`draw_table_part`) in the item's unit; a pivot that does not fit its item shows the rows that fit and a line "n more rows are in the HTML and PowerPoint files" (also in the report), never text outside the item. Nested rows are indented; parent rows are not bold in the PDF table.
- X2-3 The KPI card is drawn natively: the number in its threshold colour, the change in its tone colour with arrow and words, the period, and a sparkline broken at gaps. Each item sits on an opaque off-white surface with a hairline border over the design picture.
- X2-4 The design picture is re-encoded as JPEG (quality 88) and embedded once per page (DCT), at the page's own shape, so the PDF stays small and nothing is stretched. A picture that cannot be read stops the export with a plain message and no half-written file (the PDF is written to `.pdf.part` and renamed).

## S10b (the export dialog)
- X3-1 The dashboard header has "Export…". One dialog, three formats, built from the pages as they are on the screen (`dash_export_src`: sources, filters in force, the click, drill paths, places, designs). Nothing is exported silently from stale numbers: PowerPoint and PDF calculate every item again through `compute_item` with the screen's picks; an item that cannot be calculated stops the export and is named.
- X3-2 HTML: the dialog asks "Will you email this file?" first; "Check size" stays off until it is answered. Checking calculates every answer on a worker, renders the file, and shows the size, the refusal and its suggestions in the spec's order (`gate`), and the items "Not linked to clicks". "Export…" is off while the file is over 5 MB and the answer is Yes; the file written is the one that was sized.
- X3-3 PowerPoint: "Use design as slide background" is offered only when a page has a design (the first page's design is used for every slide). PDF has no options. All three write through a temporary file and the result is in the status bar (a PDF table cut to fit is named there).
- X3-4 A page filter whose values are not read yet stops the HTML check with "The values of the filter 'X' are not read yet. Wait a moment and try again." instead of exporting an empty dropdown. The as-of date comes from the model; without one the footnote says "As of date not set".
- X3-5 `JobKind::DashPrepare` is new (its failure shows in the dialog); the writes use the existing Export job kind.

## S11 (D9)
- D9-1 No new pictures: the five Phase 10 pictures (page, KPI, cross, free, design; light and dark) already show every screen the notes describe. The exports have no picture because the file is the output. The notes test requires all five in both themes.
- D9-2 Four tables are produced by the dashboard's own calls (`compute_item`, `kpi::view`): the bar by region, the KPI money and ratio cards, and the Dhaka click. The other notes quote hand-worked numbers from named tests (D5, D4, R1, X1, X2) in prose; the test checks every named test exists.
- D9-3 The guide helpers of the Phase 9 notes test (`section`, `tables_of`, `read`, …) were made `pub` in `tests/phase9/m6.rs` so D9 reuses them; no behaviour change.
- D9-4 The notes that Phase 10 changed (projects, chart, pivot, report packs) got one appended paragraph each, not a rewrite. The README has a new "Dashboards and reports (Phase 10)" section.

## UI revamp (three views, mockup sections 0 to 6)
The owner asked for the UI review's mockup to be built on the Phase 10 branch, with no change to any calculation and with old project files still opening. The assistant panel is left for Phase 14 (nothing of it is in the window). Each choice below was taken without asking.

### The three views (mockup section 0)
- UI-1 One window, a rail on the left (Report, Data, Model), the top menus File / Settings / Help only, and a toolbar of its own for each view. The rules live in `src/workspace.rs` (plain logic, tested); the drawing is in `src/ui/shell.rs`.
- UI-2 Which tab is in which view: **Data lists every table tab** (a pivot is a table with a pivot step, and its steps are edited there). **Report lists chart tabs, pivot tabs and tabs sent to the report.** A chart tab has no data of its own, so it is Report only. A pivot tab is in both views.
- UI-3 "Send to report" (Data toolbar, tab right-click, Ctrl+K) puts a table in Report as well and switches there; "Remove from report" is on the tab's right-click. The list is saved in the project as `report_tabs` (keys of the tabs), written only when it has tabs and read with a default, so a project without any is byte for byte what Phase 10 wrote and every old project opens. Without a project the list lasts for the session.
- UI-4 The view follows a tab that becomes active (a chart made from Data opens in Report; a dashboard that has just been opened shows Report). A view you chose with the rail is never changed by a quiet frame. A command of one view (a banking tool, Filter, Dashboard, the model steps) takes the window there first; if it still cannot run, you are put back where you were. The view you were in is kept in `settings.json` (`last_view`) and comes back next time.
- UI-5 Data is the default view (the owner works from files first). Model has no tab strip. The right panel is shown in Data and Report, never in Model.
- UI-6 The Analytics pane is the fifth tab of the right panel, Report view only (the SQL tab is Data only). It is a placeholder: forecast, plan vs actual, top movers and what-if sliders, each a card saying it comes later. It changes no number. A "Analytics" toolbar button and Ctrl+K show or hide it. (A separate dock beside the panel was tried on paper and dropped: two right-hand columns leave a 1219 pt window too little for a chart.)
- UI-7 Pop-out to a second monitor is unchanged (the pivot builder window, the chart pop-out).

### Commands by workflow (section 1)
- UI-8 Five workflow names: Data, Analyse, Visualise, Share, Settings (plus File, Help, Model, Workspace for the new commands). `workflow_of` gives each command its name, and it is the small tag in the Ctrl+K search. The old `CommandGroup` stays as it was (tests and the macro/headless code read it); the workflow is a second, display-only view of the same list.
- UI-9 Data toolbar: Open, Stack, Batch | Filter, Sort, Columns, Calc column, Pivot, Rate profile | **Prepare ▾** (conditional column, line breaks, split date, counterparty names, join, undo and redo, copy, macros) and **Analyse ▾** | SQL | Send to report and **Share ▾** (every export). Report toolbar: Visualise ▾, Chart panel, Pivot, Dashboard, Share ▾, Analytics. Model toolbar: Set up the model, Links, Key check, Calendar, As-of date, Measures.
- UI-10 Menus hold at most eight entries on their top level. The long banking list is the Analyse menu's sub-menus (Treasury & ALM, Loans & credit, Islamic window, Reports, Data checks), in the order the Banking menu had.
- UI-11 A test (`no_command_is_lost`) fails if a command is on no menu, no toolbar and not in the Settings dialog. Commands with no button (units, decimals, zoom, sort lists, calendar, house colours, summary row, side panel) are the "Settings only" list and are found with Ctrl+K and in the dialog.

### Ctrl+K (section 2)
- UI-12 The search box is wider (360 pt at most), tinted in the accent colour and reads "Search actions, columns, help…". With nothing typed the list is: RECENT (what you ran last), TRY TYPING (four phrases that fill the box when clicked: nim, as of, crore, gap), then ALL ACTIONS. Recent commands were already kept in `recent_commands.txt`; nothing new is stored.

### Settings (section 3)
- UI-13 One dialog, a list on the left: Display (theme, zoom, summary row), Units & numbers (money unit, decimals, rates note), Calendar & fiscal year (the old View ▸ Calendar, with Save), As-of date, Colours, Sort lists, Toolbar & panels. The Calendar that was in two places is now: this one (fiscal year and week, `settings.json`) and the model's calendar, which is a step of the model set-up.
- UI-14 The as-of date, house colours and sort lists keep their own editors (the colour and sort-list editors are 400 to 500 lines each, with their own save and preview), opened from their section by one button. Moving them in would be a rewrite of two working editors for no new function. If the owner wants them inside the dialog, that is a separate step.
- UI-15 The Day-count line of the mockup is not a global setting in this app (each ratio measure has its own), so the dialog does not show one.

### Model (section 4 and the Model view)
- UI-16 The five model dialogs are one window: the same title ("Model set-up"), the same width, a strip of the six steps on top (Tables, Links, Key check, Calendar, As-of date, Measures; a tick when done, a warning when a key check found problems, a dash for optional) and Back / Next at the bottom. The step you are on is highlighted; clicking a step goes there. A new first step, Tables, lists the open tables and whether a link uses each. Nothing in a dialog's own logic changed: links, key check, calendar, as-of and measures work as before. The Links form is dropped when you leave the step.
- UI-17 "Done" rules (`model_steps`): Tables done with one table open; Links with one link; Key check when every link was checked with no broken link and no unmatched keys (else "N problems"); Calendar when the shared calendar is on; As-of date is optional until the calendar exists, then done (the latest data date is the default); Measures optional.
- UI-18 The Model view shows the steps (clickable), a relationship diagram (data tables left, lookups to their right, the calendar as a lookup with a "Date link" edge; each edge carries its key-check words, in amber when something is wrong), and three cards: as-of date, calendar, measures, each with Change… that opens its step. With no links and no calendar it says what to do (Start set-up…, Open files first); with no project it says the model needs one.

### Dashboard editor (section 5)
- UI-19 The header is a toolbar: + Chart ▾ and + Table ▾ (each lists the open chart or pivot tabs), + KPI, + Filter, then on the right Close, **one Export ▾** (HTML report, PowerPoint, PDF: each opens the existing export dialog on that format, so the email question and the size check are as before) and a ⋯ menu holding **Clear** (it clears the page's filters and clicks, not the items).
- UI-20 The selected tile's settings (title, unit, KPI editor, move, remove) are a **floating panel beside the tile** (right of it, or left when there is no room) and close when you click the page or another tile. The page's own settings (title, items per row, grid or free layout, design) moved under the pages list on the left, which is 200 pt wide now, so the canvas has the full width otherwise taken by the fixed right column. The design editor keeps its own right column.
- UI-21 Units: every item already shows money in Crore unless its unit is changed (pivot items since S9, chart items from the chart's own scale, KPI cards by the dashboard default), so a chart and a table side by side agree. The raw-BDT table in the old picture was the Phase 10 guide image from before that change. A test now pins the pivot item and the chart item to the same unit.
- UI-22 An empty page shows cards instead of a sentence: Chart tab (pick from the open chart tabs), Table (the open pivot tabs), KPI card, Import a design. They add or import exactly what the toolbar and the page settings do.

### Empty screens (section 6)
- UI-23 Data with nothing open: Open file, Stack files, Batch convert. Report with nothing: with no data it points to the Data view; with data it offers Pivot, Chart, Send a table. Model with nothing linked: Start set-up, Go to Data. The three start buttons of the Data screen keep the rectangle names the first screen had (open, stack, batch).

### What was not touched
- No calculation, export format, project field (beyond the optional `report_tabs`) or step changed. The guide notes under `docs/guide` still name the old menus (View, Banking, Charts, Export) in places; they are the next thing to bring in line, together with the pictures.
