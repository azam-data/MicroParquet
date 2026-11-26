# MicroParquet — Phase 10 specification (Dashboards and reports)

Sources for this phase:
- The handoff of 25 Nov 2026 (`microparquet-phase10-handoff.md`): state, decisions, session plan, working rules.
- The owner's answers of 25 Nov 2026:
  - Design templates: **option (b)**. Add one crate so a whole design shows as a background image.
  - KPI cards: the owner has no fixed idea and asked for judgement; Crore by default, changeable per card; he wants the change arrow with colours.
  - PPTX: normal landscape slide size (16:9, 13.333 × 7.5 in). No logo, footer or fixed title style needed.
- Earlier owner decisions (project notes, 23–25 Nov 2026): the in-app dashboard tab exports HTML, PPTX and PDF; KPI cards; up to 4 page filters; cross-filter; drill; fixed grid and free drag-and-resize both kept; the HTML report is offline, asks "Will you email this file?" and then holds to 5 MB; PPTX has one item per slide with real editable charts; quality over speed.
- Not in this phase:
  - editing a chart from inside the dashboard (an item edits in its own chart tab; the dashboard follows)
  - live data in the HTML file (it holds numbers computed at export)
  - account rows in the HTML file (no Show rows)
  - animations, auto-refresh, a web server
  - importing PPTX or Canva files as editable objects (a design comes in as one background image)
  - SVG parsing (replaced by option (b))

Sessions are told which task IDs to do. Read only those task sections, plus `<architecture>`, `<conventions>`, `<constraints>`, `<guide_notes>`, `<workflow>`, `<report>` and `<accepted_deviations>`.

<goal>
Phase 10 is done when:
1. Tasks C0, D0–D5, R1, R2, X1, X2 and D9 are implemented, and `cargo build --release` and `cargo test` pass.
2. Every acceptance item is an automated assertion. Every UI item has a headless egui render test at 1219×686, 1600×900 and 1291×700 pt, asserting that nothing overflows or is clipped, no two labels overlap, and every control works when its real rect is clicked.
3. Existing results are unchanged: every macro, pack, saved run, saved chart, settings file and project from v1.9.0 (Phase 9) loads and gives the same rows and the same numbers. D0 builds the baseline from the Phase 9 code.
4. A number on a dashboard, in the HTML file, in the PPTX and in the PDF is the number the chart tab or pivot shows. One code path computes it.
5. Each new tool has a guide note (`<guide_notes>`), and `docs/guide/README.md` lists them.
</goal>

<architecture>
**Dashboard model (D0).** A dashboard belongs to the open project and is saved in the `.mpproj`:
- `Dashboard { id, title, pages: Vec<DashPage>, template_library: … }`, every new field `#[serde(default)]`.
- `DashPage { title, layout: Grid { per_row } | Free, items: Vec<DashItem>, filters: Vec<PageFilter> (max 4), background: Option<TemplateRef> }`.
- `DashItem { id, kind: Chart(tab_key) | Pivot(tab_key) | Kpi(KpiSpec), rect (grid cell or free rect), title_override }`.
- An item **points at** a chart tab or pivot tab by its project key (`t1`, `t2`, …). It never holds a copy of the spec. When the tab changes, the dashboard follows. A deleted tab shows "Source tab removed" on its item and in export, never an empty chart.
- A project with a dashboard is written with `"project_version": 3`. A project without one is written as version 2 or 1 exactly as Phase 9 writes it. The v1.9.0 app refuses version 3 with its W4 message, so a dashboard is never silently dropped.
- New modules: `src/dashboard.rs` (model, page filters, cross-filter and drill state, layout maths), `src/kpi.rs` (KPI computation), `src/report_html.rs` (R1/R2), `src/pptx.rs` (X1), `src/dashboard_pdf.rs` (X2). The UI goes in `src/ui/dashboard.rs`.

**One compute path.** An item's numbers come from `dashboard::compute_item(item, page_state, project) -> ItemData` on the worker, which calls the same ChartSpec / `ValueSpec` / measure code the chart tab and pivot use (Phase 7–9). The screen, HTML, PPTX and PDF all draw `ItemData`. A test asserts the dashboard's item equals the chart tab's own data on every fixture.

**Page state.** `PageState { filter_picks: [Option<Vec<value>>; 4], cross: Option<CrossPick>, drill: HashMap<item_id, Vec<value>> }`. It is a view state, saved with the project (`#[serde(default)]`) so reopening shows the same page, and reset with a "Clear" button.
- A **page filter** is a lookup field, Calendar field or data column of one data tab, shown as a dropdown with all its items plus "All". A filter applies to every item whose source tab is that data tab, or is linked to it through the model. An item whose source can't take the filter shows a "filter doesn't apply" tag; it is never silently ignored.
- **Cross-filter**: clicking a bar, slice or pivot row sets `cross` to (dimension, value). Every other item on the page that shares the dimension (same field, or a linked lookup field) is filtered by it. Clicking the same mark again clears it. The clicked item itself keeps all its marks (highlighted).
- **Drill**: the Phase 8 chart drill (click, breadcrumb, Back). On a dashboard each item drills on its own.

**KPI card (D2).** `KpiSpec { title, source: NamedMeasure | ValueSpec on a data tab, unit: Crore | Lakh | Million | Absolute | Percent | Pp | AsIs, decimals, compare: None | PriorPeriod | Sply, good_direction: Up | Down | Neutral, thresholds: Vec<Threshold> (max 2), sparkline: bool }`.
- The big number is the measure at the model's as-of date and period (Phase 9 M2/M4). Unit default **Crore** (BDT ÷ 10,000,000) for money, **Percent** for ratios; both changeable per card and as a dashboard default.
- Change = Phase 9 rule: percent change is (current − earlier) / |earlier|, blank when earlier is 0 or blank; a ratio's change is in **pp**. The card shows both the amount and the percent for money ("+125.00 Cr · +2.66%"), pp only for ratios.
- Arrow: ▲ up, ▼ down, ► unchanged (|change| rounds to 0 at the card's decimals). Colour follows `good_direction`: good = house green, bad = house red, Neutral = house blue-grey (no judgement). The colour is **never the only signal**: the arrow and the words "up"/"down" (in the tooltip and in the HTML/PPTX alt text) carry it too, and the colours pass the Phase 7 colour-blind check.
- Thresholds: up to 2 rules on the value, such as "below 80.00 → amber", "below 70.00 → red". The first matching rule colours the number. Thresholds are the owner's numbers, saved in the dashboard; **none are hard-coded**, and regulatory limits come from the Phase 5 data files with effective dates if the owner picks one.
- Sparkline: the measure for the last 12 periods of the model calendar (months by default), same code as the measure. A period that can't be computed is a gap in the line, never 0.
- A measure that can't be computed shows "—" and the reason ("No balance in Nov 2026 for 'Accounts' (last date 31 Oct 2026)"), never 0.

**Free layout (D4).** Items hold a rect in page units (1000 × 625 per 16:10 page, or the template's size). Dragging and resizing snap to a 10-unit grid; items may not leave the page or overlap by default (overlap is allowed only when the owner turns it on). Switching grid ↔ free converts positions; switching back is lossless only for items that still sit on the grid, and the dialog says so before it changes.

**Design templates (D5, option b).** One new crate: `image` (already in `Cargo.lock` through `egui_extras`; pin the locked version, features `png` and `jpeg` only, `default-features = false`; S6 runs `cargo tree -i image` first and reports the real added build time). It decodes PNG and JPEG for the screen, the PDF and the HTML file. The owner exports the design from Claude Design or Canva as PNG or JPEG at about 1920 px wide.
- Importing a template: choose the image, then draw the **slots** over it in the free layout (drag a rectangle, name it, set its type: chart, pivot, KPI, any). Slots are saved with the template. The template is then applied to a page: slots become empty placeholders, and the owner fills each with an item.
- A template is stored in `portable_data/dashboard_templates/<name>/design.png` + `slots.json`, and a **copy** (downscaled so the long side is at most 2000 px, re-encoded as PNG, refused above 3 MB with a plain message) goes into the project with its blake3, so a project moved to another computer still shows its design.
- Whatever the design, items keep the house palette, the colour picker and the colour-blind checks. A design never recolours a chart.
- PDF draws the image as the page background. PPTX uses it as the slide background only if the owner picks "Use design as slide background". The HTML file inlines it (as a data URI) and counts it toward 5 MB.

**HTML report (R1, R2).** One offline file, ECharts like the chart book (`src/charts/echarts.rs`).
- **Everything the reader can reach is computed at export.** The file holds, per page, a table of answers keyed by (filter picks, cross pick, drill paths). No account rows go in the file.
- **Reachable states.** For each item, the answers are computed for: each page-filter combination × each drill path the item allows. To keep the size bounded:
  - Every filter has "All" plus its values. States = product of (values + 1) over the page's filters.
  - **Cross-filter** works in the file only for items whose measures are **additive** over the clicked dimension (sums, flows, a closing balance of a single date, counts). For these the app exports a compact cube (the item's values grouped by every dimension the page filters and cross-filters use) and the page recomputes by summing in the browser. Items with a **non-additive** measure (ratios, averages, average balance, windows, SPLY) react to page filters and drill (precomputed) but **not** to cross-filter clicks; the export dialog lists them and the page shows "Not linked to clicks" on them. (Deviation recorded in `<accepted_deviations>` as R1-1 for the owner to accept or change.)
  - If the states for one page exceed 20,000 numbers per item, the export refuses with what to cut (fewer filter values, Top N, fewer drill levels), as for the 5 MB rule.
- **Email question.** Export asks "Will you email this file?". If yes: a size estimate is shown **before** writing; if over 5 MB (5,000,000 bytes) the Export button stays off and the dialog suggests, in this order: fewer filter values, fewer drill levels, Top N on the biggest chart, fewer pages, leaving out the template image. If no: no limit.
- **Size estimate** = ECharts library bytes (measured by the test, not guessed) + HTML shell bytes + template data-URI bytes + the answers' JSON bytes. The estimate must be within 10% of the real written file on every fixture.
- Pivots appear as Nested tables with expand/collapse (Phase 9 `NestedView`); the numbers are the pivot's.
- Colours, drill path in the subtitle, and number formats (Crore) match the app.

**PPTX (X1).** One item per slide: title, chart (DrawingML, **editable**, with an embedded workbook), footnote with the as-of date and filters in force. A pivot item is a native table. A KPI card is shape + text. Slide size 12192000 × 6858000 EMU. The package is written with the `zip` crate (the second new crate, owner decision of 25 Nov 2026), with Deflate compression, so the file is as small as PowerPoint's own; the embedded workbooks come from `rust_xlsxwriter` (`save_to_buffer`). Charts use the chart tab's palette and series order.

**PDF (X2).** `printpdf` as in Phase 6/7. One A4 landscape page per dashboard page; items are drawn by the Phase 7 chart PDF code and a new KPI and table drawer; the design image is the page background.
</architecture>

<conventions>
- **Dates are typed** (Phase 8). Money displays in **Crore**; rates in percent; shocks in bp. A KPI card's unit never changes the stored number.
- **Nothing is guessed.** A filter value that no longer exists after relinking next month's files is kept in the saved pick and shown as "(not in data)"; the page shows blank, not all rows. A dashboard refresh that can't be computed says why.
- **Names.** Pages, items, templates and filters are renamed by the owner; links use ids, so renaming never breaks a dashboard.
- **Outside a project.** A dashboard needs a project. Without one, the Dashboard button says: "Dashboards belong to a project. Save or open a project first."
- **Python export.** A dashboard is refused: "Python export can't build dashboards yet. Export the chart tab's Python or the dashboard's HTML, PPTX or PDF."
- **Saving:** the dashboard in the `.mpproj`; templates in `portable_data/dashboard_templates/` and a copy in the project; the last export folder and the "email" choice in `settings.json`.
- **Plain messages** for every refusal, naming what the owner can do.
- **Relinking.** When next month's files are relinked, the dashboard recomputes every item, KPI and sparkline on the worker. The window never waits.
</conventions>

<tasks>

<task id="D0" name="Baseline and dashboard model">
- In a temporary git worktree at the Phase 9 merge commit on main (`Cargo.toml` 1.9.0, no tag), save with Phase 9 code: a project with a model (links, calendar, measures), a chart tab, a pivot tab (Nested), a saved pack. Commit them to `tests/fixtures/phase9_replay/` with `phase9_replay.hashes.json` (blake3 of rows, chart layout, project bytes after save → open → save except `saved_at`). Keep the code as `tests/phase10/p9_baseline.rs`.
- Add `Dashboard`, `DashPage`, `DashItem`, `PageFilter`, `PageState` and the grid layout maths (1–4 per row, equal widths, row height from the page) in `src/dashboard.rs`; save and load in the `.mpproj` (`project_version` 3 only when a dashboard exists).
- **Acceptance:**
  - `d0_the_phase9_samples_replay_to_the_hashes_the_phase9_code_gave`
  - a project without a dashboard saves byte for byte as Phase 9 saves it; one with a dashboard saves as version 3 and the v1.9.0 refusal message is asserted
  - save → open → save of a dashboard of 2 pages and 5 items is stable
  - a grid of 3 per row on a 1000-unit page gives widths 326.67 (hand-worked: (1000 − 2 × 10 gap) ÷ 3 = 326.67; assert ± 0.01)
  - an item pointing at a deleted tab round-trips and reports "Source tab removed"
</task>

<task id="C0" name="Phase 9 carry-over">
- **Month-end note.** When a period's last calendar day has no rows (a Friday or Saturday month-end), a pivot or chart whose value is a **YTD-balance Period amount**, or an **annualised yield**, adds a note: "The last date with rows in Oct 2026 is 29 Oct; income posted after it counts in Nov 2026." Numbers are unchanged. The note appears in the pivot's notes, the chart's footnote and the dashboard item's tooltip.
- **Faults found by eye** in the `phase9` build: the owner lists them in the first prompt; each becomes a test, then a fix. If the list is empty, say so in the report.
- **Acceptance:**
  - hand-worked fixture: rows through 29 Oct 2026, a YTD income measure for Oct 2026 gives the same number as before and carries the note with "29 Oct" and "Nov 2026"
  - a month whose last day has rows has no note
</task>

<task id="D1" name="Dashboard tab UI: pages, items, filters">
- A **Dashboard** button (toolbar and tab menu) creates a dashboard tab. Left: pages list (add, rename, reorder, delete with a confirm). Centre: the page. Right: item properties.
- Add an item: pick a chart tab or pivot tab from the project; it appears in the next free grid cell. 1–4 items per row; per-row choice per page.
- **Page filters:** up to 4, each "Add filter ▸" a field of a data tab (incl. lookup and Calendar fields), a dropdown with all items plus "All", multi-select off by default (single value), on as a choice. The 5th is refused: "A page can have 4 filters. Remove one first."
- Items draw from `ItemData` (the chart through the Phase 7 renderer; a pivot as a table, Nested if the pivot is Nested).
- Clicking the item's title opens its source tab.
- **Acceptance:**
  - the dashboard item's numbers equal the chart tab's own on every fixture
  - a filter on a lookup field filters an item whose tab is linked, and an unlinked item shows "filter doesn't apply"
  - a 5th filter is refused with the message above
  - renaming a source tab keeps the item
  - every control works when its real rect is clicked at the three sizes
</task>

<task id="D2" name="KPI cards">
- As in `<architecture>` (KPI card). The card editor is a small dialog: title, measure, unit, decimals, compare, good direction, up to 2 thresholds, sparkline.
- Hand-worked numbers for the tests:
  - Closing deposits at 31 Oct 2026 = 48,250,000,000 BDT = **4,825.00 Cr**; at 30 Sep 2026 = 47,000,000,000 = 4,700.00 Cr. Prior period: **+125.00 Cr · +2.66%** (125 ÷ 4,700 = 0.026596), ▲, good if Up is good.
  - Yield (YTD, Actual/365) 11.00% against 10.50% SPLY: **+0.50 pp**, ▲.
  - Cost of funds 6.00% against 6.40% prior: **−0.40 pp**, ▼, **green** because Down is good.
  - Earlier value 0 or blank: the percent shows "—", the amount stays.
  - Threshold "below 80.00 → amber" on 79.50 gives amber; on 80.00 gives no colour.
- **Acceptance:** the five numbers above, a blank measure showing "—" with its reason (never 0), the unit switch (Crore → Absolute 48,250,000,000) changing the display and not the stored number, a sparkline of 12 points with one gap, and the arrow plus words present when colours are removed.
</task>

<task id="D3" name="Cross-filter and drill on a page">
- As in `<architecture>` (Page state). Click a mark to cross-filter; click again to clear; a "Clear" button on the page; the cross pick shows as a chip ("Region = Dhaka ×").
- Drill (Phase 8) works per item on the page; breadcrumb and Back are per item.
- **Acceptance:**
  - fixture: a bar by Region and a bar by Product on one data tab; clicking Region "Dhaka" gives the Product bar the numbers of Dhaka rows only (assert the numbers against a hand-filtered table)
  - the clicked item keeps all its marks
  - a pivot row click filters the charts
  - an item on another data tab with no link to the dimension shows "filter doesn't apply"
  - Back after two drill levels restores the first level
  - the page state is saved and restored with the project
</task>

<task id="D4" name="Free drag-and-resize layout">
- As in `<architecture>` (Free layout). Handles on all four corners and edges; arrow keys nudge by 10 units; Shift+arrows resize.
- A layout toggle per page: Grid ↔ Free, with the lossless-or-not message.
- **Acceptance:**
  - drag and resize through real rect events at the three sizes
  - snap to 10 units, items kept inside the page, no overlap unless turned on
  - Grid → Free → Grid returns the same positions for on-grid items
  - an item dragged off the page is clamped, never lost
</task>

<task id="D5" name="Design templates (option b: image crate)">
- Add the `image` crate as in `<architecture>`. **First step:** `cargo tree -i image` and report the version and the added build time; if the locked version needs a feature not listed, stop and list it in the report instead of adding it.
- Import PNG/JPEG, draw and name slots, save the template, apply it to a page, fill slots, show the image as the page background in grid and free layouts.
- Refusals: not an image ("This file is not a PNG or JPEG picture."), over 3 MB after downscale, a slot outside the image.
- **Acceptance:**
  - import a 1920×1080 test PNG, draw 4 slots, save, reopen the project: the template, its slots and the image hash are the same
  - the project copy is used when `portable_data` has no such template
  - the image is drawn behind the items; items keep their palette
  - a template with a hash mismatch is reported
</task>

<task id="R1" name="HTML export engine">
- `src/report_html.rs`: computes, for each page, the answer table (`<architecture>` HTML report), the size estimate, the email question and the 5 MB rule. No HTML page yet beyond a stub.
- **Acceptance:**
  - hand-worked count: filter A with 12 values and filter B with 4 → (12+1) × (4+1) = **65** states; a 6-bar chart with no drill = 65 × 6 = **390** numbers; at 8 bytes a number ≈ 3,120 bytes of JSON (assert the count exactly, the bytes within 10%)
  - the estimate is within 10% of the real file
  - email = yes and estimate 5.3 MB: Export is refused with the suggestions in order; fewer filter values bring it under 5 MB in the test
  - the answers equal `compute_item` for 20 random states per fixture
  - non-additive items are listed by name in the export dialog
  - no account rows in the output (assert a known account number does not appear)
</task>

<task id="R2" name="HTML page">
- The page: pages as tabs, filters as dropdowns, cross-filter by click, drill with breadcrumb and Back, the path in the subtitle, Nested pivots with expand/collapse, the house palette, Crore formats, the template image.
- Offline (no network call), works from a file on disk in Chrome, Edge and Firefox. ECharts is inlined.
- **Acceptance:** a headless-browser test (Playwright, in the test only, not a dependency) loads the file, picks a filter, clicks a bar, drills, goes Back, expands a pivot row and compares each number with `ItemData`; no console errors; no request leaves the file.
</task>

<task id="X1" name="PPTX with the zip crate">
- **First step:** `cargo tree -i zip`; report the locked version and the added build time. Add `zip` pinned to the locked version, `default-features = false`, feature `deflate` only (if the locked version needs another feature, stop and list it in the report).
- Refuse a package above 4 GB with a plain message.
- `src/pptx.rs`: the package (content types, rels, presentation, slide master and layout, one slide per item, chart parts with embedded workbook).
- **Acceptance:**
  - the package is read back with the zip crate's reader in the test: every entry's CRC checks, `[Content_Types].xml` comes first, and the file is smaller than the same package stored uncompressed
  - one slide per item; each chart part's type matches the chart tab (bar, line, pie, …); series references point at the embedded workbook ranges; the palette is the chart's
  - the file opens in PowerPoint and LibreOffice (checked by the owner by eye; LibreOffice headless conversion to PDF succeeds in the test when LibreOffice exists, otherwise skipped with a note)
  - the footnote carries the as-of date and the filters in force
</task>

<task id="X2" name="PDF export of dashboard pages">
- One A4 landscape page per dashboard page; items from the chart PDF code; KPI cards and tables drawn natively; the design as background; the filters in force in a footer line.
- **Acceptance:** page count = dashboard pages; text extracted from the PDF contains each KPI number as the card shows it; the image is embedded once per page; no overflow at A4 landscape; the PDF of a free-layout page keeps item positions (within 1 mm).
</task>

<task id="D9" name="Guide notes, pictures, release build">
- Notes: `dashboards`, `dashboard-kpi-cards`, `dashboard-filters-and-drill`, `dashboard-layouts`, `dashboard-templates`, `export-html-report`, `export-pptx`, `export-pdf-pages`, `monthly-dashboard-routine` (relink next month's files, refresh, export, email). Pictures light and dark.
- Update the notes Phase 10 changed. `docs/guide/README.md` lists them. Release build.
- **Acceptance:** as Phase 9 M6: the notes test with the 8 sections in order; the worked example's tables are the tables the code gives; every named picture exists in light and dark.
</task>

</tasks>

<guide_notes>
Every new note has these sections, in this order: What it does · When to use it · How to use it (numbered steps) · Example (from the tests, with real numbers) · What it refuses and why · Python export · Saved in (macro, project, settings) · Related tools. Plain English for a banker; no Rust or Polars terms; at most 150 lines.
</guide_notes>

<constraints>
- **Two new crates only, both approved by the owner:** `image` (`default-features = false`, features `png` and `jpeg`), added in D5, and `zip` (`default-features = false`, feature `deflate`), added in X1. Both are already in `Cargo.lock` and are pinned to the locked version. No other crate, no upgrades.
- Every new serialised field gets `#[serde(default)]`; old files load unchanged and D0's replay proves it.
- No `unwrap()`, `expect()` or index panics on user data. Refuse with a plain message; never default a value (no 0 for a blank KPI, no guessed filter value).
- Computation, export and image decoding run on the worker; the window never waits.
- Colour literals live only in `src/charts/tokens.rs` and `theme.rs`.
- Test data is synthetic. Never commit `portable_data`, never a template image from the owner, and never a `.mpproj` that points outside `tests/fixtures`.
- The version stays 1.9.0. Don't tag.
</constraints>

<workflow>
- Never run `cargo clean`. `cargo check` while editing, only the task's tests while working, the full `cargo test` once before each commit, piped through `tail -40`. Grep large files; never read `src/main.rs` whole.
- One commit per task (`Phase 10 <ID>: …`), pushed after each. Never amend or force-push; a correction is a new commit.
- Tests go in `tests/phase10/`, compiled into the phase3 binary like `tests/phase9/`. Render tests go in `src/ui/render_tests_p10.rs`.
- Keep `docs/phase10-progress.md` updated and resume from it; each session ends with an acceptance item → test table.
- Lean sessions (owner decision, 24 Nov 2026): prove one or two key tests per task by stub, one at a time, restored; open only the light picture; read only the named sections; report in the `<report>` format.
- Never stop to ask. Take the option closest to this spec and the Phase 9 house style, record it in `<accepted_deviations>` as `<ID>-n` and in the progress file, and list it in the report.
- If blocked three times on the same failure, mark the task BLOCKED and continue.
- Dependencies: D0 first. C0 any time after D0. D1 needs D0. D2, D3 and D4 need D1. D5 needs D4. R1 needs D3 and D2. R2 needs R1. X1 and X2 need D2 and D4. D9 last.
- Sessions: S1 = D0 + C0, S2 = D1, S3 = D2, S4 = D3, S5 = D4, S6 = D5, S7 = R1, S8 = R2, S9 = X1, S10 = X2, S11 = D9, S12 = fixes after the owner's by-eye check, release build, PR ready.
- Setup (S1 only): `git fetch`; branch `phase10` from main; copy this spec to `docs/phase10-spec.md`; create `docs/phase10-progress.md`; open a **draft** PR `phase10` → main. If a session is not on `phase10`, it fetches and checks it out first.
- Wait for every background run before committing or ending a session. Restore guide pictures a full test run rewrites but the task didn't change.
- `cargo build --release` in the last session. One PR for the phase, draft until S12. Don't merge, don't tag.
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
Owner decisions of 25 Nov 2026 (S1 prompt): R1-1 and D5-1 accepted as written; design templates use option (b), the `image` crate; PPTX uses the `zip` crate (Deflate); KPI cards default to Crore; PPTX is normal 16:9 landscape. The repository copy of this spec is authoritative from now on.

Sessions record their choices here and in `docs/phase10-progress.md`, for the owner to accept or change.

Planned by the spec author, for the owner to accept or change:
- R1-1: cross-filter clicks work in the HTML file only for items with additive measures; ratio, average-balance and window measures react to filters and drill but not to clicks. The reason is that a ratio can't be rebuilt from clicked parts in the browser without account rows, and the file holds none.
- D5-1: slots are drawn by the owner over the imported image (no SVG or sidecar file), because option (b) gives the app a picture only.

Recorded by sessions:
- D0-1: the baseline was made in the main checkout at the Phase 9 merge commit (the dashboard files stashed) instead of a separate worktree, because `phase10` had no other change yet; the code that made the hashes is the Phase 9 code.
- D0-2: `Project.dashboard` is `Option<Dashboard>` (one dashboard per project), written only when present; the app keeps it in `ProjectState.dashboard` so saving never drops it.
- D0-3: a grid item has `rect: None` (its cell is its order); a Free item holds its rect. Rows share the page height equally; no margin at the page edge.
- D0-4: `PageState.drill` is a `BTreeMap` (the spec says HashMap) so saved bytes are stable.
</accepted_deviations>
