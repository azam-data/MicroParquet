# MicroParquet — Phase 7 specification (Chart studio)

Sources for this phase:
- The owner's request of 19 Nov 2026: a proper charting feature with many chart types and a way to use code for what the GUI cannot do.
- The architect's design decisions recorded here. The owner has asked the architect to own the visual design.
- The reference renderings in `docs/design/charts/`. These are the visual target: colours, marks, titles, labels and spacing must match them.

Sessions are told which task IDs to do. Read only those task sections, plus `<design_system>`, `<conventions>`, `<constraints>`, `<guide_notes>`, `<workflow>` and `<report>`.

<goal>
Phase 7 is done when:
1. Tasks C0–C8 are implemented, and `cargo build --release` and `cargo test` pass.
2. Every acceptance item is an automated assertion. Every UI item has a headless egui render test at 1219×686, 1600×900 and 1291×700 pt, asserting four things:
   - nothing overflows
   - no chart text is clipped by its mark
   - no two labels overlap
   - every control works when its real rect is clicked
3. Each chart type and banking template has guide notes, and `docs/guide/charts.md` teaches how to choose a chart.
4. Existing results and files are unchanged. The Phase 6 chart tab keeps working, as the "Quick chart" of C2.
</goal>

<architecture>
**One chart model, three renderers.**

- **`ChartSpec`** (`src/charts/spec.rs`, serde with `#[serde(default)]` on every field) describes a chart completely:
  - source: a tab, after its steps
  - kind
  - encodings: x, series, measures with their aggregation (and weight for W.Avg), size, facet
  - order (Natural, a custom list, Tenor or By value) and Top N
  - scale (BDT, K, M, Lakh, Crore)
  - text: title, subtitle, footnote, and an optional takeaway typed by the user
  - reference lines and bands
  - highlight rule
  - colour map
  - for HTML only: an "advanced" JSON override

  The aggregation runs on the worker over the whole tab (the Phase 6 U5 engine, extended). The result is a `ChartData` table, which is the only thing any renderer receives.

- **Renderer A, in-app** (`src/ui/chart_render.rs`): a custom egui painter. It is not egui_plot, because egui_plot cannot meet the mark specs. It is used in the side panel, the pop-out window and the new full-size Chart tab, and it is interactive with hover tooltips.
- **Renderer B, HTML** (`src/charts/echarts.rs`): builds an Apache ECharts option as JSON with `serde_json` and writes one self-contained, offline HTML file. Presentation quality, interactive, with save-as-PNG/SVG in the page.
- **Renderer C, Excel** (`src/charts/xlsx.rs`): native, editable Excel charts through `rust_xlsxwriter`'s Chart API, placed beside the chart's data on its own sheet.

**Why this split:**
- No new Rust crates are needed.
- The app stays one small, offline executable.
- The look stays identical everywhere, because every renderer reads the same tokens.
- Each output goes where the owner works: slides take the PNG/SVG from the HTML page, Excel/Power BI users get native charts, and ALCO reviewers get interactive HTML.

**Not done, by decision:**
- No embedded Python or JavaScript runtime. It would add hundreds of MB, break portability, and the bank laptop allows only portable apps.
- No dual-axis charts. Two measures of different scale become linked panels (C1).
- No 3D, no gauges, and no pie charts with more than 5 slices.
</architecture>

<design_system>
The look is fixed here, so charts read as one system. Match `docs/design/charts/*.png`.

**Typeface**
- Source Sans 3 (already embedded) for all in-app chart text; JetBrains Mono is not used in charts.
- HTML: `font-family: "Source Sans 3", "Segoe UI", system-ui, sans-serif`. The font files are not embedded in HTML.
- Excel: Segoe UI.
- Sizes (pt): title 15 semibold; subtitle 11; axis ticks and labels 10; data labels 9.5–10; footnote 9.

**Colour.** Use exactly these values. They form a validated colour-vision-deficiency (CVD) safe set; any change must be re-validated with the dataviz palette validator before merging, and the test in C0 pins them.

| Role | Light | Dark |
| --- | --- | --- |
| Chart surface | `#fcfcfb` | `#1a1a19` |
| Primary ink (titles, values) | `#0b0b0b` | `#f0efec` |
| Secondary ink (subtitles, labels) | `#52514e` | `#c3c2b7` |
| Muted ink (ticks, footnote) | `#898781` | `#898781` |
| Gridline (1 px, solid) | `#e1e0d9` | `#2c2c2a` |
| Baseline / zero line | `#c3c2b7` | `#383835` |
| De-emphasised series (older curves, "Other") | `#c3c2b7` | `#55544f` |

- **Categorical slots**, in fixed order, never cycled:

  | Slot | Light | Dark |
  | --- | --- | --- |
  | 1 blue | `#2a78d6` | `#3987e5` |
  | 2 orange | `#eb6834` | `#d95926` |
  | 3 aqua | `#1baf7a` | `#199e70` |
  | 4 yellow | `#eda100` | `#c98500` |
  | 5 magenta | `#e87ba4` | `#d55181` |
  | 6 green | `#008300` | `#0ca30c` |
  | 7 violet | `#6250d6` | `#9085e9` |
  | 8 red | `#e34948` | `#e66767` |

  A 9th category folds into "Other" (de-emphasised colour). Scatter, bubble and small-multiple colour series are capped at 3; above that, fold into "Other" or facet.
- **Sequential (heatmaps):** one hue, blue, 13 steps: `#cde2fb #b7d3f6 #9ec5f4 #86b6ef #6da7ec #5598e7 #3987e5 #2a78d6 #256abf #1c5cab #184f95 #104281 #0d366b`. Cell text is white from step 7 (`#3987e5`) and darker, ink below that.
- **Diverging (signed values such as gaps, ΔEVE and variance):** positive is blue `#2a78d6`, negative is red `#e34948`, and the midpoint or zero is grey. Never a hue at zero.
- **Status (reserved, never used as a series colour):** good `#0ca30c`, warning `#fab219`, serious `#ec835a`, critical `#d03b3b`. Always shown with a text label such as "Breach", never by colour alone.
- **Text never wears a series colour.** Values, labels and legends use the ink colours. The coloured mark beside the text carries the identity.
- **Colour follows the entity, never its rank.** The colour map is stored in the ChartSpec, so filtering out a series never repaints the others.

**Marks**
- **Bars:** at most 24 px thick (they never fill their band). A 4 px rounded data-end, square at the baseline. They grow from one zero baseline.
- **Stacked segments and touching bars:** separated by a 2 px gap in the surface colour, never by a border.
- **Lines:** 2 px with round joins. Markers are at least 8 px, with a 2 px surface-coloured ring.
- **Area fill:** about 10% opacity of the series colour.
- **Gridlines:** horizontal only for column charts and vertical only for bar charts; 1 px, solid, never dashed. No chart border, no plot background box, no axis lines except the baseline.
- **Reference lines** (limits, minimums, the 15% outlier line): 1.5 px solid in the status colour, with a text label. Never dashed.
- **Heatmap cells:** rounded (radius 6% of the cell), with a 2 px surface gap.

**Labels**
- **Titles.** Title = the auto-title, or the user's takeaway when typed. Subtitle = what is measured, the unit and the date. The footnote gives the source, the method and "Sample parameters" when a sample file fed the tool.
- **Auto-titles** are descriptive: "Sum of Balance by Term Period". The template auto-titles in C5 state the finding: "Parallel up is the worst case at −10.5% of Tier 1".
- **Label selectively, never a number on every point:**
  - bars: value at the tip
  - lines: value at the end
  - waterfalls: every step
  - heatmaps: every cell
  - stacked columns: the total on top only
- **Collisions.** A label that does not fit is moved outside the mark or dropped to the tooltip, never clipped. Converging line-end labels use the legend instead of nudging.
- **Legend.** Present for two or more series, in one row above the plot or to the right. With four or fewer series, direct labels at line ends are added. A single series has no legend.
- **Axis ticks** use clean round numbers in the chart's scale ("0 / 500 / 1,000 Cr", "−10%"), one style per axis.

**Interaction (in-app and HTML)**
- **Hover:** a per-mark tooltip on bars, cells and points, and a crosshair with all series values on lines and areas. The tooltip shows the category, series, formatted value, row count and, for percentages, the base. The hit target is larger than the mark.
- **Table view:** every chart has a "Table" toggle that shows its `ChartData`.

**Layout:** padding of 3% around the plot; title block, plot, then footnote; no element closer than 8 px to another.
</design_system>

<conventions>
- **Units:** amounts follow the measure's display scale (Phase 3); the scale is stated once, in the subtitle, and is never repeated on every label.
- **Zero:** every bar, column and area chart starts at zero. Lines may start elsewhere, with the axis showing it.
- **Order:** categories follow the ChartSpec order. The default is by value for nominal categories, and natural, Tenor or list order for ordered categories (buckets, tenors, grades, months).
- **Data size:** at most 5,000 points per series. Above that the chart says "Showing a 5,000-point sample of N" (scatter) or asks for an aggregation (line).
- **Saving:** charts are saved per tab in `settings.json` (as in Phase 6) and as named charts in `portable_data/charts/<name>.json`.
</conventions>

<tasks>

<task id="C0" name="Chart model, tokens and data">
- Add `ChartSpec` and `ChartData` as described in `<architecture>`. Extend the Phase 6 chart aggregation to:
  - several measures
  - size and facet encodings
  - percent-of-total
  - a running sum (for cumulative lines)
  - waterfall steps (start, deltas, end) and heatmap matrices
- Add the `<design_system>` colours and sizes as named tokens in `theme.rs`. A test pins every hex value of the palette tables.
- Add the colour map: the first time a category is seen it takes the next free slot, and the slot is stored. Tests:
  - filtering out B keeps A and C on their colours
  - a 9th category goes to "Other"
- Acceptance:
  - percent-of-total rows sum to 100.00 (±0.01)
  - a running sum of [−1,850, −620, 410] is [−1,850, −2,470, −2,060]
  - the MoM example gives waterfall steps 80.00 → +8.00 → +5.00 → +0.50 → 93.50, with floating bases 80.00, 88.00 and 93.00
  - each heatmap row of the Phase 4 V2 example, as "% of row", sums to 100
  - an old Phase 6 chart setting loads unchanged
</task>

<task id="C1" name="In-app renderer">
- **Types:**
  - column and bar: single, grouped, stacked and 100% stacked
  - line, including stepped
  - area and stacked area
  - scatter and bubble (at most 3 colour series)
  - histogram (bins as in the Phase 5 column inspector)
  - box plot
  - waterfall
  - heatmap
  - donut (2 to 5 slices; otherwise refused with "Use a bar chart: more than 5 parts are hard to compare as slices")
  - **linked panels**: two to three panels stacked vertically on one shared x-axis, each with its own measure and type. This replaces a dual axis.
  - **small multiples**: one panel per facet value, up to 12, with shared scales by default
- **Look:** every rule of `<design_system>`, drawn with the egui painter at the window's scale, in light and dark. Bars with rounded data-ends, label placement with collision checks, the legend rules, hover tooltips and the Table toggle.
- **Where it shows:** the Chart side panel, the pop-out window, and a new **Chart tab** ("Open as tab") that fills the main area. Chart tabs are listed as tabs and saved with the session.
- **Acceptance:** render tests at all three sizes for every type in light and dark, asserting:
  - no label rect overlaps another or leaves the chart rect
  - bars are at most 24 px thick
  - hovering a bar's rect shows its tooltip
  - stacked segments have the 2 px gap
  - the waterfall's floating bars start at the C0 bases
  - a negative bar grows down from zero
</task>

<task id="C2" name="Charts menu and chart studio">
- Add a **Charts** menu between Banking and Export (menu order File, Edit, View, Banking, Charts, Export, Help; menus never fold). It holds:
  - "New chart…"
  - "Chart this result" (enabled on tool result tabs, C5)
  - "Open saved chart…"
  - "Chart book (HTML)…" (C3)
  - Ctrl+K entries for each
- **New chart** opens the **chart gallery**: cards with a small drawn thumbnail in the house style, grouped by the question they answer:
  - **Compare:** bar, grouped, sorted bar
  - **Change over time:** line, area, column by month
  - **Composition:** stacked, 100% stacked, donut, waterfall
  - **Distribution:** histogram, box plot
  - **Relationship:** scatter, bubble, heatmap
  - **Flow and share (HTML only):** Sankey, treemap
  - **Banking templates** (C5)

  Each card has one line of "use it when…" and one of "avoid it when…" (the guide text of C8).
- **"Suggest a chart":** pick columns, and the studio proposes the best form with a one-line reason:
  - date + number → line
  - text + number → sorted bar
  - ordered text (buckets, tenors, grades) + number → column in that order
  - two numbers → scatter
  - parts of a total over time → stacked column
  - two text columns + number → heatmap
  - a signed change between two totals → waterfall
- **Studio controls** (side panel, two-column grid as in Phase 6): type, x, series, measures (each with aggregation), order, Top N, scale, title, takeaway, footnote, reference lines (value, label, status colour) and highlight (one category or series emphasised, the rest de-emphasised). There is also "Save chart…" and "Open as tab".
- The Phase 6 Chart side-panel tab becomes the studio. Old per-tab settings are migrated.
- **Acceptance:**
  - each suggestion rule gives its form
  - a donut with 6 parts is refused with the message
  - a saved chart reloads identically
  - the Charts menu sits between Banking and Export and is clickable at all three sizes
</task>

<task id="C3" name="HTML export (ECharts), chart books and advanced options">
- **Vendored ECharts:**
  - Take the latest stable Apache ECharts `dist/echarts.min.js` from the npm package.
  - Store it under `assets/vendor/echarts/` with its LICENSE and NOTICE (Apache-2.0).
  - Record the version and SHA-256 in `THIRD_PARTY_NOTICES.md`.
  - Embed it with `include_str!`. HTML files never reference a CDN or any URL.
- **Theme:** register an ECharts theme `microparquet-light` / `microparquet-dark` built from the design tokens: palette, fonts, grid lines, no borders, the tooltip style and bar border radius [4, 4, 0, 0].
- **Export ▸ "Chart to HTML…"** writes one file: title, subtitle, chart, footnote, a Table toggle, a light/dark switch, and a toolbox with "Save as PNG" (pixelRatio 3), "Save as SVG" (SVG renderer) and "Data view".
- **HTML-only types:**
  - **Sankey**, e.g. CL migration flows. The template uses previous grade → current grade, with Closed/Repaid and New as nodes.
  - **Treemap**, e.g. deposit or loan mix by product and branch.
- **Chart books:** Charts ▸ "Chart book (HTML)…" puts several saved charts into one page: a title block, the date, then a responsive two-column grid of the charts, each with its own toolbox.
- **Advanced options:** a code box in the studio (monospace, as the SQL editor) holds JSON merged over the generated ECharts option for HTML output, for anything the GUI cannot do. It is checked as JSON, saved with the chart, and has a "Reset" button and a link line naming the ECharts option reference. It never affects in-app or Excel rendering, and the studio says so.
- **Acceptance:**
  - golden JSON files of the generated option for every type
  - the written HTML contains no `http://` or `https://` reference, and opens offline
  - the file is under 2 MB for a 1,000-point chart
  - the Sankey node and link values of the V2 example equal the matrix cells
  - invalid advanced JSON is refused with the line and column
  - a chart book with 3 charts has 3 chart containers
</task>

<task id="C4" name="Native Excel charts">
- **Excel export.** A tab or report-pack item that has a chart writes its `ChartData` to the sheet, plus a native chart beside it:
  - column, bar, line, area, scatter, doughnut and stacked variants, with the palette, Segoe UI, no chart border, light gridlines and the title and subtitle
  - **waterfall** as a stacked column with an invisible base series
  - **heatmap** as the data range with a 3-colour conditional format using the sequential ramp ends and middle (`#cde2fb`, `#3987e5`, `#0d366b`)
  - **linked panels** as one chart per panel, stacked
  - Sankey and treemap are not written (their data is), with a note on the sheet
- **Acceptance:** for each type, open the written .xlsx (zip) and assert:
  - `xl/charts/chartN.xml` exists with the right chart type
  - its series references point at the data range
  - the series colours equal the palette
  - for a waterfall, the base series has no fill
</task>

<task id="C5" name="Banking chart templates (“Chart this”)">
A result tab of each tool gets a **"Chart this"** button (and the Charts menu entry). It opens the studio with the template applied; every template is still editable. Templates, matching `docs/design/charts/`:

| Template | Source tool | Form | Auto-title (finding) |
| --- | --- | --- | --- |
| Gap ladder | Liquidity gap ladder | Linked panels: net gap columns (diverging) over a cumulative gap % line with limit ticks; breaches ringed and labelled "Breach: x% vs y% limit" | "Cumulative gap turns positive after <bucket>", or "Cumulative gap stays negative to <bucket>" |
| Repricing gap | ΔNII repricing summary | Diverging columns of the gap by bucket, with the cumulative gap as a linked panel | "Largest repricing gap in <bucket>" |
| NII bridge | MoM variance | Waterfall: prior interest → volume → rate → mix → current, per key or total | "<Effect> explains most of the <±x> change in <key> interest" |
| CL migration | CL migration matrix | Heatmap of % of the previous grade (Sankey in HTML as an alternative) | "<x>% of SS moved to DF" (the largest off-diagonal downgrade) |
| Classified trend | Stacked tabs over months | Stacked columns by grade over months, plus an NPL % linked panel | "NPL ratio <rose/fell> to x%" |
| IRRBB scenarios | IRRBB (six scenarios) | Horizontal diverging bars of ΔEVE % of Tier 1, sorted, with the −15% outlier line | "<Scenario> is the worst case at x% of Tier 1" |
| Yield curves | Yield curves store | Lines by curve date; latest highlighted and labelled at the end, older curves de-emphasised and labelled at the start | "The curve <rose/fell> by x bp at <tenor> since <date>" |
| LCR / NSFR | LCR / NSFR summary | Bullet bars: ratio against the minimum line, plus HQLA composition as a tall segmented bar | "LCR x% against a 100% minimum" |
| ECL by stage | IFRS 9 summary | Grouped bars of EAD and ECL by stage as small multiples (no dual axis), with coverage % labelled | "Stage 3 holds x% of ECL" |
| Deposit / loan mix | Any tab with product and month | Stacked columns over months; treemap in HTML | "<Product> drove growth" |
| Rate profile | Rate profile | Column histogram of balance by rate band in band order | "x% of balances sit at <band>" |
| Behaviour profile | Behaviour profile | Balance-history line with the core level as a shaded band | "Core balance x% of the latest" |
| Data quality | DQ scorecard | Horizontal bars of pass % by rule, with Green/Amber threshold lines | "<n> rules below Amber" |

- **Finding titles** are computed from the data by fixed rules (no AI). The user's takeaway replaces them.
- **Acceptance:**
  - each template on its tool's guide example gives the stated form and the exact auto-title, e.g. "Parallel up is the worst case at −10.5% of Tier 1"
  - on the Phase 4 V3 gap-ladder example (limit −10% on the first bucket), the breach is ringed on bucket 1 and labelled "Breach: −16.7% vs −10% limit", and the auto-title is "Cumulative gap turns positive after 1–3 months"
  - the NII bridge has the C0 bases
  - the CL migration title is "100.0% of SS moved to DF" on the V2 example
</task>

<task id="C6" name="Charts in report packs and headless">
- A report-pack item may carry a chart: a saved chart, or a template on a tool item. The pack writes:
  - the native Excel chart on that item's sheet (C4)
  - with "Also write an HTML chart book", one `<pack>-charts.html` beside the workbook (C3)
  - the Cover lists the charts
- **Headless:** `--export-html <file>` writes the chart book of a pack run.
- **Acceptance:**
  - a pack with a gap-ladder item and its template writes both outputs
  - headless output equals the UI output (except run time)
  - with any item failing, no file is written
</task>

<task id="C7" name="Copy as Python (matplotlib)">
- The studio's "Copy as Python" puts a self-contained script on the clipboard: the `ChartData` as a pandas DataFrame literal, plus matplotlib code in the house style (fonts, palette, marks, titles), so a user can continue in Python.
- For Sankey and treemap it uses plotly and says so in a comment.
- **Acceptance:** for each type, the generated script parses as Python (`python3 -c "import ast; ast.parse(...)"` in the test, skipped with a note if python3 is absent) and contains the data values.
</task>

<task id="C8" name="Guide: charts">
- Add `docs/guide/charts.md`, a primer for a reader new to charts:
  - choosing by question
  - why sorted bars beat pies
  - why one axis
  - colour meaning (blue up / red down, status colours)
  - writing a takeaway title
  - when a single number or a table is better than a chart
- Add one guide note per banking template (ten sections each), worked examples from the tests, and screenshots from the render tests saved as PNG under `docs/guide/img/`.
- Update the guide README and the chart tool notes.
</task>

</tasks>

<constraints>
- No new Rust crates. Use the existing `egui`, `serde_json`, `rust_xlsxwriter`, `blake3` and `printpdf`. The only new third-party asset is the vendored ECharts file (Apache-2.0), recorded in THIRD_PARTY_NOTICES.
- Every new serialised field gets `#[serde(default)]`; old macros, settings, packs and runs load unchanged.
- Colour and size literals live only in `theme.rs` (the Phase 3 rule and test).
- No `unwrap()`, `expect()` or index panics on user data. An empty `ChartData` shows "No data to chart" with the reason.
- Charts never put a number on every point except where `<design_system>` allows it (waterfall, heatmap, a single series of 12 or fewer bars).
- Test data is synthetic. Never commit anything from `portable_data`.
</constraints>

<workflow>
- Never run `cargo clean`. Run `cargo check` while editing and the full `cargo test` before each commit.
- Make one commit per task (`Phase 7 <ID>: …`) and push after each. Guide notes go in the same commit.
- Keep `docs/phase7-progress.md` updated, and resume from it.
- Never stop to ask. Take the option closest to `docs/design/charts/`, record it in `<accepted_deviations>`, and list it in the report.
- If blocked three times on the same failure, mark the task BLOCKED and continue. Dependencies:
  - C1–C7 need C0.
  - C5 needs C1 and C2.
  - C6 needs C3 and C4.
  - C8 runs last.
- One PR from `phase7` to main. Wait for Tests, and don't merge.
</workflow>

<report>
Reply in no more than 30 lines: a table (Task | Status | Tests added | Deviations), the final `cargo test` summary, questions, and a "look at by eye" list. The list must include one screenshot path per chart type from the render tests.
</report>

<accepted_deviations>
- C0: the colour and size literals live in `src/charts/tokens.rs` (library), exposed to the UI as `CHART_*` tokens in `theme.rs`; the HTML and Excel renderers are in the library and cannot read the binary's theme. Both crates pin every hex value in a test.
- C0: the Phase 6 chart types moved to `src/charts/` and are re-exported from `src/chart.rs`; `ChartKind::Bar` stays the vertical column form (horizontal is a flag).
- C1: layout geometry lives in `src/charts/layout.rs` (library), painter in `src/ui/chart_render.rs`; histogram bars fill their band (2 px gaps); stacked areas 55% opacity; axis names are horizontal above the plot; more than 3 scatter series fold into 2 + Other; heatmap rows follow series size.
- C2: "Chart this result" opens the studio (templates are C5); "Chart book" is disabled until C3; the studio's extra sections are under the chart in the side panel and beside it in the pop-out; the right panel no longer drag-scrolls; Phase 6 tests adjusted for the Charts menu and the chart's own title.
- C1: a chart tab is a normal tab (the chart's data table) drawn as a chart in the main area; tabs are not restored at start-up, so "saved with the session" is `settings.json` `chart_tabs`.
- C0: a percentage of a zero total is blank with a note; a waterfall that does not add up, and more than 12 facet values, are refused with a message.
- Owner decisions on Session A (19 Nov 2026), these replace the earlier C1 lines they contradict:
  1. Heatmap: rows AND columns follow the custom order list; with no list, CL/grade codes use `cl_code_map` order (STD, SMA, SS, DF, BL), other text sorts naturally. Every row×column pair is drawn; a missing pair is a 0.0 cell in the lightest sequential step, with its value label. Cell corner radius 2 px (not 6% of the cell), gaps stay 2 px. Same rules in the HTML and Excel renderers (replaces C1-5, "heatmap rows follow series size").
  2. Stacked area: fill opacity about 35% (pinned in `tokens.rs`), with a 2 px line on top of each layer (replaces 55%).
  3. Write Δ, not "d": ΔEVE and ΔNII everywhere (titles, axes, tooltips, HTML, Excel), tested in all three renderers.
  4. C3 adds "Preview in browser" for the HTML-only forms (Sankey, Treemap): it writes a temp HTML file and opens the default browser. Nothing goes over the network.
  5. Accepted as is: `tokens.rs`, blank cell for a percent of a zero total, histogram bars filling their band, horizontal axis names, chart tab in `settings.json`, panel scrolls by scrollbar.
- C3: the vendored ECharts file holds six `http(s)://` strings (XML namespace names and licence comments, never fetched); the page around it has none, and the test pins both. Title, subtitle and footnote are inside the chart so PNG/SVG carry them. Stacked segments use a 1 px surface-colour border (ECharts has no gap setting). Function-valued options are names ("@fmt:amount:2") that the page's script replaces.
- C4: subtitle and footnote are cells above the table, not part of the Excel chart title; a bubble chart is written as a scatter and a stepped line as an ordinary line, each with a note on the sheet; the .xlsx checks read the zip with `python3` (no new crate) and are skipped where it is absent.
- C5: finding titles follow the reference images (one decimal, true minus: "−10.5% of Tier 1", "Breach: -16.7% vs −10% limit"), not the two-decimal examples in the C5 acceptance; the LCR/NSFR HQLA composition, a per-key NII bridge and start-of-line labels on older yield curves are not done; percentage bar labels have one decimal and no plus sign.
- Owner decisions on Session B (19 Nov 2026), these replace the C5 line above where they contradict it:
  - Q1: finding titles use one decimal for percentages, as the reference images ("−10.5% of Tier 1", "Breach: −16.7% vs −10% limit"); the C5 acceptance strings above are updated.
  - Q2: data labels on diverging bars always carry a sign and two decimals ("+4.82%", "−10.50%"), in all three renderers (replaces C5-6).
  - Dark-mode sequential fills (in-app and HTML heatmaps, any sequential fill): low values use the darkest step (nearest the surface), high values the lightest; both directions pinned in `tokens.rs` and tested.
  - Negative numbers use U+2212 (−) everywhere (titles, data labels, axis ticks, tooltips, breach labels, HTML, Excel number formats); a test scans every rendered string and the xlsx XML for "-" before a digit.
  - Classified trend: the top panel stacks SMA, SS, DF and BL only, subtitle "Standard (STD) not shown"; the NPL % panel is unchanged.
  - Yield curves: every curve is labelled at its right end, collisions nudged vertically; latest curve blue with markers, older curves two grey steps (lighter for older); y-axis padded one tick below the minimum. This replaces the start-of-line labels of the C5 table.
  - Data quality: a rule below Amber is drawn in the critical status colour with an icon and "Below Amber"; between Amber and Green the warning colour and "Below Green"; other bars default; threshold labels never overlap.
  - Gap ladder: the breach label sits on the side away from the line, overlapping no mark (layout test).
  - Waterfall: every step is at least 2 px tall; the HTML waterfall draws the connector lines the app draws.
  - HTML tooltips: "row"/"rows" singular/plural, no repeated category line.
  - Excel line panels use the app's nice axis range (not forced to 0); stacked areas show no point markers.
  - C5 completed: LCR/NSFR HQLA composition as a tall segmented bar, NII bridge per key.
- Session C details: older yield curves are fainter (muted, then de-emphasised grey) in both themes; status bars carry their icon and words inside the bar when it is long enough; of several vertical threshold lines the highest is labelled to its right and the others to their left; a panel with a breach gets 25% axis room; the NII bridge per key is a second chart ("effects by key"), the HQLA composition a second chart; "stacked areas show no markers" applies to the app and HTML (Excel areas have none). See `docs/phase7-progress.md`.
</accepted_deviations>
