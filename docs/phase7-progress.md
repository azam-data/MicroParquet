# Phase 7 progress (chart studio)

Branch `phase7`. Session A: C0, C1, C2 done. Session B (this one): owner decisions 1–5, then C3, C4, C5. Resume from this file.

| Task | Status | Commit | Tests |
| --- | --- | --- | --- |
| C0 | DONE | (see git log: "Phase 7 C0") | tests/phase7/c0.rs ×20; render_tests_p7 ×1 (theme tokens) |
| C1 | DONE | (see git log: "Phase 7 C1") | tests/phase7/c1.rs ×22 (layout rules, all forms); render_tests_p7 ×4 (every form × light/dark × 3 sizes with real fonts, hover by real rect, Table toggle and Chart tab by real clicks, screenshots) |
| C2 | DONE | (see git log: "Phase 7 C2") | tests/phase7/c2.rs ×9 (suggestion rules, gallery catalogue, card to columns, saved charts round trip and plain errors, Charts commands and Ctrl+K); render_tests_p7 ×4 (Charts menu order and clicks, gallery cards/suggest by real rects at 3 sizes, studio sections typed and clicked, save and open by real clicks) |

| C3 | DONE | (see git log: "Phase 7 C3") | tests/phase7/c3.rs ×16 (golden option of all 20 forms, dark option, offline page, vendored file, 2 MB limit, Sankey values, treemap, waterfall bases, donut refusal, marks, theme, advanced JSON, chart book, preview/write, Δ); render_tests_p7 ×1 (Export, Preview, advanced box, Reset, chart book by real clicks at 3 sizes); `tools/html_chart_png.js` loads every page in headless Chromium with the network blocked (no script error, no request) and saves PNGs `docs/guide/img/charts/html-*.png` |

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
