# Phase 7 progress (chart studio)

Branch `phase7`. Session A: C0, C1, C2 done. Session B: owner decisions 1–5, C3, C4, C5 (done). Session C (this one): owner decisions Q1/Q2 and fixes 1–10, then C6, C7, C8. Resume from this file.

| Task | Status | Commit | Tests |
| --- | --- | --- | --- |
| C0 | DONE | (see git log: "Phase 7 C0") | tests/phase7/c0.rs ×20; render_tests_p7 ×1 (theme tokens) |
| C1 | DONE | (see git log: "Phase 7 C1") | tests/phase7/c1.rs ×22 (layout rules, all forms); render_tests_p7 ×4 (every form × light/dark × 3 sizes with real fonts, hover by real rect, Table toggle and Chart tab by real clicks, screenshots) |
| C2 | DONE | (see git log: "Phase 7 C2") | tests/phase7/c2.rs ×9 (suggestion rules, gallery catalogue, card to columns, saved charts round trip and plain errors, Charts commands and Ctrl+K); render_tests_p7 ×4 (Charts menu order and clicks, gallery cards/suggest by real rects at 3 sizes, studio sections typed and clicked, save and open by real clicks) |

| C3 | DONE | (see git log: "Phase 7 C3") | tests/phase7/c3.rs ×16 (golden option of all 20 forms, dark option, offline page, vendored file, 2 MB limit, Sankey values, treemap, waterfall bases, donut refusal, marks, theme, advanced JSON, chart book, preview/write, Δ); render_tests_p7 ×1 (Export, Preview, advanced box, Reset, chart book by real clicks at 3 sizes); `tools/html_chart_png.js` loads every page in headless Chromium with the network blocked (no script error, no request) and saves PNGs `docs/guide/img/charts/html-*.png` |
| C4 | DONE | (see git log: "Phase 7 C4") | tests/phase7/c4.rs ×9 (every form's .xlsx opened as a zip with python3: chart type, series references on the data range, palette colours, waterfall base with no fill, heatmap scale, Δ, notes, sheet names); render_tests_p7 (Export to Excel by a real click at 3 sizes); every workbook also rendered in LibreOffice for `docs/guide/img/charts/xlsx-*.png` |
| C6 | DONE | (see git log: "Phase 7 C6") | tests/phase7/c6.rs ×4 (a pack with a gap-ladder template item and a saved-chart item writes the workbook with native charts, the chart book and the Cover rows; twice gives the same charts and book; a failing chart writes nothing; old packs load); headless test (same book from the command line, `--export-html`, nothing written on failure); render_tests_p6 ×1 (the Chart drop-down, the HTML chart book tick and Run by real clicks at 3 sizes) |
| C5 | DONE | (see git log: "Phase 7 C5") | tests/phase7/c5.rs ×8 (each of the 13 templates on its tool's guide example: form, exact finding, three renderers, save/load; gap ladder breach ringed and labelled; limit ticks in HTML and Excel); render_tests_p7 ×2 (gallery cards and a click on the gap ladder card by real rects at 3 sizes, Chart this result) |

## Deviations (also in `docs/phase7-spec.md` `<accepted_deviations>`)

- C0-1: the palette and size literals live in `src/charts/tokens.rs` (library crate), not in `src/ui/theme.rs`: the HTML and Excel renderers are in the library and cannot read the binary's theme. `theme.rs` exposes them as `Color32`/`f32` tokens (`CHART_*`), and both the library test (`c0_every_hex_value…`) and the UI test pin every hex value.
- C0-2: the Phase 6 types (`ChartSpec`, `ChartAgg`, `ChartKind`, `ChartScale`, `ChartData`) moved to `src/charts/` and are re-exported from `src/chart.rs`, so Phase 6 code and tests compile unchanged. `ChartKind::Bar` stays the name of the vertical column chart (old settings say "Bar"); horizontal bars are `horizontal: true`.
- C0-3: a percentage of a zero total is blank with a note, not 0; a waterfall whose end does not equal start plus steps is refused with a message (a bridge that does not add up is a wrong number).
- C0-4: more than 12 facet values is refused (spec says "up to 12"), asking the user to filter first, rather than silently dropping panels.
- C1-1: the geometry (marks, labels, collision checks) is in the library, `src/charts/layout.rs`, so it is asserted without a window and the HTML/Excel renderers can share the same tokens; `src/ui/chart_render.rs` only paints it, adds hover and the Table view.
- C1-2: histogram bars fill their band (minus the 2 px gap): the 24 px cap would leave floating slivers; every other bar, column, segment and box is capped at 24 px. Stacked areas use 55% opacity (10% is unreadable when stacked); plain areas use the 10%.
- C1-3: the axis name sits above the plot (horizontal), not rotated; scatter/bubble fold more than 3 colour series into the top 2 and "Other".
- C1-4: a "Chart tab" is a normal tab (the chart's data table, so it exports like any tab) whose main area draws the chart (`chart_tabs`, by tab id). Tabs are not restored when the app restarts, so "saved with the session" is `settings.json` `chart_tabs` (title, source key, chart); Charts ▸ "Open saved chart…" (C2) reopens it.
- C1-5: heatmap rows follow the series' size, not the order list (columns follow the order list).
- C1-6: the Phase 6 quick chart keeps its own title and notes above the plot (the renderer runs in `bare` mode there); the studio of C2 uses the full title block. Screenshots are made by `tools/chart_layout_png.py` (Pillow) from the layout the render tests produced; they are skipped with a note when python3/Pillow is absent. PNGs are in `docs/guide/img/charts/`.
- C2-1: "Chart this result" is enabled on tool result tabs and opens the studio on that tab; the banking templates it applies are task C5. "Chart book (HTML)…" is in the menu and Ctrl+K but disabled with its reason until the HTML export (C3). The gallery's "Banking templates" group is a line of text for the same reason.
- C2-2: the Phase 6 Chart tab is the studio. Its Phase 6 controls keep their places; the new sections (shape, Titles and look, Reference lines, Measures and panels, Save / Open saved chart) are under the chart in the side panel and beside it in the pop-out window (controls in a left column). The right panel no longer scrolls by dragging with the mouse (a click on a control scrolled it once the panel was taller than the window); the wheel and the bar still scroll.
- C2-3: Phase 6 tests adjusted for the intended changes: the menu order has Charts; the "Chart" toolbar button test matches the label exactly (the Charts menu contains the word); the quick chart's status line ("… all 250 rows (Crore)") is found by its text now that the chart has its own title; fields under the chart are not required to be inside the window (they scroll).
- C2-4: a reference line's label goes beside the line where there is room; when the line crosses the bars it stands above the plot after a short piece of the line in the status colour. The extra measures are panels (never a second axis); a facet and several measures at once are allowed.
- C2-5: `Needs::ResultTab` / `Needs::Later` were added to the command list (a tool's result tab is any tab that is not a file, a stack or a join).

## Owner decisions on Session A (also in the spec `<accepted_deviations>`)

- D1: heatmap rows and columns follow the custom order list; with no list, CL grade codes use `cl_code_map` order (STD, SMA, SS, DF, BL), other text sorts naturally. Every row×column pair is drawn; a missing pair is a 0.0 cell in the lightest step with its label. Radius 2 px, gaps 2 px. Same in HTML and Excel. Replaces C1-5.
- D2: stacked area fill about 35% (in `tokens.rs`), 2 px line on top of each layer. Replaces the 55% of C1-2.
- D3: Δ (not "d") in ΔEVE and ΔNII everywhere; tested in all three renderers.
- D4: C3 "Preview in browser" for Sankey and Treemap (temp HTML file, default browser, no network).
- D5: accepted as is: tokens.rs; blank cell for % of zero total; histogram bars fill their band; horizontal axis names; chart tab in settings.json; panel scrolls by scrollbar.

## C3 deviations

- C3-1: the vendored `echarts.min.js` (6.1.0, SHA-256 in `THIRD_PARTY_NOTICES.md`) contains six `http(s)://` strings: XML namespace names (`www.w3.org/...`) and two licence comments. None is ever fetched. The test asserts that the page *without* the library has no `http`, `src=`, `href=`, `url(` or `@import`, and that the library holds exactly those six strings, so a new one in a future version is noticed. The page was also opened in headless Chromium with every network request blocked: it drew, with no error.
- C3-2: the title, subtitle and footnote are drawn inside the chart (ECharts `title` and `graphic`), so "Save as PNG/SVG" carries them; the page has only the Table and Dark/Light buttons above the chart.
- C3-3: ECharts has no "gap between segments" setting, so stacked and grouped bars use a 1 px border in the surface colour (a 2 px gap between neighbours), the one place a border is used.
- C3-4: formatters, tooltips and the toolbox's SVG button are functions in ECharts; the option holds names (`@fmt:amount:2`, `@tip`, `@axistip`, `@saveSvg`) that the page's script swaps for functions. The advanced box may use the same names.
- C3-5: Sankey node names are "From STD" / "To STD" (a grade is on both sides) and their label shows the grade; node values are the sums of their links. A flow table needs the Series column (from) and the Category column (to).
- C3-6: `Needs::Later` was removed from the command list; "Chart book (HTML)…" and "Chart to HTML…" need an open tab. The book dialog lists the saved charts that fit the active tab; the worker calculates each over the whole tab and writes the page (one that cannot be drawn stops the book and nothing is written).
- C3-7: tests can set `chart.export_path_override`, `chart.no_browser` and `chart_book.path_override`, so a click never opens a file dialog or the browser.

## C4 deviations

- C4-1: the chart's title is the chart title in Excel; the subtitle and footnote are cells above the table (Excel's chart title has one text style, so two sizes in one title are not possible with rust_xlsxwriter).
- C4-2: Excel has no bubble chart in rust_xlsxwriter 0.79 (a scatter with a note and the size column), no stepped line (an ordinary line, noted), and no cap of 24 px on bars (the gap width is set instead).
- C4-3: the zip is read by `python3` in the tests (no new crate, so no zip reader in Rust); they are skipped with a note where `python3` is missing.
- C4-4: reference lines are a helper column and a 1.5 px line series in the status colour on the same chart; "Chart to Excel…" writes the active chart. A tab in the multi-tab workbook export and a report-pack item get their chart in C6.
- C4-5: box plots (not listed in the spec) are stacked columns with an invisible base and error-bar whiskers; the whisker columns are on the sheet.
- C4-6: the Phase 3 Ctrl+K test for "excel" accepts either Excel export as the first hit (there are two now) and still requires the workbook of tabs to be found.

## C5 deviations

- C5-1: the finding titles follow the reference images (`docs/design/charts/01_gap_ladder.png`, `04_irrbb_scenarios.png`), not the longer examples printed in the spec: "Parallel up is the worst case at −10.5% of Tier 1" (one decimal, true minus; the spec's acceptance printed −10.50%) and the breach label "Breach: -16.7% vs −10% limit" (the value like a data label, the limit like an axis tick; the spec printed −16.67%). The gap-ladder title is "Cumulative gap turns positive after 1–3 months" (the first bucket with a positive cumulative gap, as the spec's V3 example). Other templates use the spec's wording; the NII bridge follows the image's "Higher balances explain most of the +13.5 Cr rise in total interest".
- C5-2: new chart features the templates needed: per-category limit ticks with a breach ring and text (`ChartSpec.limit_ticks`, `limit_note`), drawn in the app, as scatter series in the HTML and as a "Limit" column with dash markers in Excel (the ring is HTML and app only); `MeasureSpec.pct` for numbers that are already percentages ("%" labels).
- C5-3: a template whose chart needs numbers the tab does not have as columns (ΔEVE % of Tier 1, bridge steps, grade shares, NPL %) opens that small table as a tab (named "Chart: …") and charts it; the others chart the tab as it is.
- C5-4: not done: the LCR/NSFR "HQLA composition" tall segmented bar (the ratios are drawn against their minimums); the NII bridge is for the total, not per key (filter the tab, then chart); older yield curves are de-emphasised and the latest highlighted but not labelled at their start; the behaviour-profile band is the core balance (volatility method) as a band from zero.
- C5-5: Classified trend and Behaviour profile read the rows in the tab's view, so a tab with more rows than the view holds is refused with the pivot to make first.
- C5-6: bar labels of a percentage use one decimal and no plus sign ("-10.5%", "4.8%"); the reference image shows two decimals and "+4.82%".

## Owner decisions on Session B (also in the spec `<accepted_deviations>`)

- Q1: one decimal in finding titles (spec strings updated). Q2: diverging bar labels always signed with two decimals in all renderers (replaces C5-6).
- F1 dark sequential: low = darkest step, high = lightest, pinned in tokens.rs. F2 U+2212 everywhere, scan test. F3 classified trend top panel SMA/SS/DF/BL, "Standard (STD) not shown". F4 yield curves labelled at the right end (replaces start labels), latest blue with markers, older two greys, y padded one tick. F5 data quality statuses "Below Amber"/"Below Green". F6 gap-ladder breach label away from the line. F7 waterfall min 2 px, HTML connectors. F8 HTML tooltips row/rows, no repeated category. F9 Excel line axis like the app; no markers on stacked areas. F10 HQLA composition, per-key NII bridge.

## Session C: deviations (fixes 1-10)

- F4-1: "lighter for older" is read as fainter: the older curves use the muted ink, then the de-emphasised grey, so the oldest is nearest the surface in both themes (in the dark theme that is the darker grey). Stored in the colour map as grey steps (`Colour::Grey`).
- F5-1: on a status bar (data quality) the icon and the words sit inside the start of the bar (white or ink, by the fill's contrast) when the bar is long enough, else after the value. The icon is the text colour, not the status colour, because the bar already is the status colour. A horizontal bar whose value does not fit beyond its tip (it ends at the edge of the plot) carries the value inside its end.
- F5-2: of several vertical threshold lines the highest has its label to its right, the others to their left; a label is never put where a line of 1.5 px or more runs; if no place is free the labels stagger in rows above the plot.
- F6-1: a panel with a breach gets 25% room on both sides of its axis so the label can stand on the side away from the line; the label is tried below the ring first when the line rises, above when it falls, and always clear of lines, rings and marks.
- F7-1: the HTML waterfall puts the connectors in a markLine under the bars (z 1) and sets `barMinHeight` 2; a part with no value is null, so no stub is drawn for a zero.
- F9-1: "no point markers on stacked areas" is applied in the app and the HTML (a point shows on hover only); Excel area charts have no markers. Excel line axes use the nice-tick range for six ticks (the app's tick count depends on the window height), so the ranges agree in kind but not always in step.
- F10-1: the NII bridge "per key": the tab's bridge is the total of its rows (as the reference image, "total interest"; filter the tab to one key for that key's bridge), and with two or more keys a second chart, "Chart: NII effects by key", opens (volume, rate and mix per key, the largest change first, at most 12 keys). The LCR/NSFR HQLA composition is a second chart too ("Chart: HQLA composition": Level 1, 2A, 2B and, when caps leave some out, "Not counted (caps)"), shown only when the summary has the level rows.
- F-misc: `1 rows` is written `1 row` in every renderer; the sample LCR summary now has the level rows (guide numbers unchanged).

## C6 deviations

- C6-1: the native chart of a pack item is on its own sheet right after the item's ("<sheet> chart"), not on the item's data sheet: the chart's table has its own layout (categories and series columns) and the item's sheet is the raw table. The Cover lists them. A charted item's sheet name is at most 20 characters (the exporter's "2_" numbering and " chart 2" must fit Excel's 31).
- C6-2: a template that makes two charts (LCR/NSFR with the HQLA composition, NII bridge with effects by key) writes both ("<sheet> chart", "<sheet> chart 2") and both go in the book.
- C6-3: the template's thresholds in a pack are the defaults (data quality 98/95 %, IRRBB outlier 15 %): a pack does not read the rule set of the result's tab.
- C6-4: `PackOutput.html_charts` (default off) asks for the book; `--export-html <file>` also asks, and names it. The book's date is the cutoff, so a rerun of the same files gives the same book.
