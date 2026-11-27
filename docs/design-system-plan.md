# MicroParquet design system overhaul: the plan

Status: plan only, nothing built. Written 26 Nov 2026 against `main` at v1.14.0. Build starts the week of 30 Nov.
Goal: every screen and every export looks like one polished corporate product (think Metabase and Power BI), feels smooth and fast, and stays 100% offline.

---

## 1. In one page

**Why it looks like patchwork today (found in the code, not guessed):**

1. **Two palettes that don't match.** The app's own colours (`src/ui/theme.rs`) are cool greys with a blue `#1F5FAD` accent, and *amber* `#E8A33D` in dark mode. The charts (`src/charts/tokens.rs`) use warm greys (`#FCFCFB` / `#1A1A19`) and a different blue `#2A78D6`. A chart therefore sits as an off-white rectangle inside a white tile. The HTML export (`assets/report/page.css`) holds a third hand-typed copy, and the PDF and PowerPoint code hold more (`rag_colour`, `tint`, `colour_of`, `rag_hex`).
2. **Tiles have no shared anatomy.** Dashboard tiles are drawn by hand in `ui/dashboard.rs`: title, padding, border and content spacing are decided tile by tile. KPI cards use labels at 28/14/12 pt with fixed-size sparklines (160×28), bullets (200×18) and gauges (150 wide).
3. **Charts are drawn "loud".** Every chart carries its own title, legend, axis lines and gridlines, and the default is the 8-colour categorical set even for one series.
4. **Tables and KPI cards look like app widgets**, not report elements: header fill, zebra stripes and borders are the data-grid look.
5. **One-off sizes everywhere.** `theme.rs` is 1,671 lines, with about 150 constants, many of them single-use widths (`SCHEMA_COL_SIGN_W`, `MUD_WEIGHT_FIELD_W`). There is a spacing scale (4/8/12/16) but also literals such as 5, 6 and 36×30.

**What the plan does:** (a) one token source that the app, charts, HTML, PDF, PPTX and Excel all read; (b) a small component kit so no screen draws its own chrome; (c) a "dashboard look" pass (the visible win); (d) speed and motion; (e) auto-explore (Metabase X-ray style) that builds a starter dashboard from any table; (f) export fidelity; (g) bank colour themes, accessibility, density. Each phase ends with before/after screenshots and automated checks.

**Size:** 10 phases, about 26 build sessions (one or two per day, so roughly 4 to 5 weeks at the Phase 10 pace). Each session is one or two task IDs, as in `CLAUDE.md`.

| Phase | Name | Sessions | What you will see afterwards |
|---|---|---|---|
| DS0 | Baseline and measuring tools | 1 | Nothing new on screen; "before" pictures and speed numbers exist |
| DS1 | One token source and the theme engine | 2 | Charts and app share one palette; no off-white boxes in tiles |
| DS2 | The dashboard look | 4 | Dashboard reads like Metabase/Power BI: canvas, tiles, KPI cards, tables, filter bar |
| DS3 | Speed and smoothness | 2 | Hover, drag, resize and page switches feel instant; idle CPU near zero |
| DS4 | Component kit and app chrome | 3 | Buttons, tabs, rail, menus, dialogs, notices all consistent |
| DS5 | The views: Data, Report, Model, Assistant | 3 | Data grid, model diagram, assistant panel and start screen polished |
| DS6 | Auto-explore (X-ray) | 3 | "Explore this table" builds a starter dashboard in one click |
| DS7 | Export fidelity (HTML, PDF, PPTX, Excel) | 3 | An exported dashboard looks like the screen |
| DS8 | Bank themes, accessibility, density | 2 | Pick your bank's colours; keyboard, contrast, colour-blind checks pass |
| DS9 | Hardening and release | 1 | Before/after report, guide book update, release candidate |

---

## 2. Ground rules for every phase

- No new crates, no web fonts, no network. Source Sans 3 and JetBrains Mono stay embedded.
- A phase changes how things look and feel, **never a number**. Each phase must pass the existing calculation tests (`tests/review.rs`, phase tests) untouched. Where a label or unit text changes, the test is changed in the same commit and listed in the report.
- New serialised fields (theme choice, density, tile options) get `#[serde(default)]` so old projects and macros still load.
- Light and dark are written together. Per the lean-session rule, only the light screenshots are opened by the build session; you check the dark ones.
- Decisions made during a phase go into `docs/design-system-decisions.md` (same style as `phase14-decisions.md`).
- One commit per task ID, pushed.

---

## 3. Design language (the target)

This is what DS1 to DS5 implement. Values are starting points; DS1 validates them with contrast and colour-blind tests before they are frozen.

### 3.1 Colour: roles, not hex codes

Screens ask for a **role** ("card surface", "secondary text", "positive"), never a hex value. Roles:

| Group | Roles |
|---|---|
| Surfaces | `canvas` (page behind tiles), `surface` (tiles, dialogs), `surface_raised` (menus, popovers), `surface_sunken` (inputs, table header), `scrim` (behind dialogs) |
| Lines | `border_subtle` (tile edge, row separator), `border_strong` (input edge), `focus_ring` |
| Text | `text_primary`, `text_secondary`, `text_tertiary`, `text_disabled`, `text_on_brand`, `text_link` |
| Brand | `brand`, `brand_hover`, `brand_pressed`, `brand_soft` (selected row, chip), `brand_soft_border` |
| Status | `good`, `warn`, `bad`, `info`, each with a `_soft` background and a `_text` shade that passes 4.5:1 |
| Data | 8 categorical series, sequential ramp (13 steps), diverging (negative / zero / positive), `deemphasis` (grey for "other" and older series), `target` and `band` colours |

Rules: one neutral family (cool slate) for app **and** charts; a chart has **no background of its own** (it inherits the tile surface, which removes the off-white box); dark mode is designed, not inverted (dark surfaces get lighter as they rise, shadows disappear and borders carry the depth); the brand colour is the same hue in light and dark (lighter step in dark), so amber is no longer a different identity; amber stays available as a bank theme.

Starting palette (light), to be validated in DS1: canvas `#F4F6F9`, surface `#FFFFFF`, sunken `#EEF1F5`, border subtle `#E1E6ED`, border strong `#C5CDD8`, text `#14202E` / `#46546A` / `#66758A` (4.7:1 on white), brand **royal blue**, about `#4169E1` (4.8:1 on white, so it passes for buttons and links; DS1 picks the exact hex and a deeper step for small text). The current `#1F5FAD` is retired. Dark (brand is a lighter royal blue): canvas `#0E1318`, surface `#161C23`, raised `#1D252E`, border `#2A3440`.

### 3.2 Type scale

One family (Source Sans 3), numbers included. Its default figures are tabular and lining, so columns of numbers align without a monospace font; DS0 proves this on a specimen sheet before we commit. JetBrains Mono is kept only for code, formulas and the Python export.

| Role | Size / weight | Use |
|---|---|---|
| `display` | 32 semibold | KPI hero number, big-number tile |
| `title` | 20 semibold | Dashboard title, dialog title |
| `heading` | 16 semibold | Panel and section headings |
| `tile_title` | 14 semibold | Tile heading |
| `body` | 14 regular | Default text, table cells |
| `small` | 13 regular | Secondary text, filter labels |
| `caption` | 12 regular | Units, notes, axis labels, footnotes (smallest allowed on screen) |
| `mono` | 13 mono | Code, formulas |

Rules: sentence case everywhere; numbers right-aligned and tabular; a unit is always shown once, in the tile subtitle ("BDT Crore"), not repeated in every value; line height 1.35 for body, 1.15 for display.

### 3.3 Spacing, radius, elevation, stroke

- **Spacing** (4-pt grid): 4, 8, 12, 16, 24, 32. Nothing else is allowed in new code. The existing `SPACE_XS..L` stay; `24` and `32` are added.
- **Radius**: 4 (chip, small), 6 (control), 10 (tile, card, dialog), 999 (pill). The current 3/4/6/8/10/14 collapse to these four.
- **Elevation**: 0 flat; 1 tile (1 px `border_subtle` plus a very soft shadow in light, border only in dark); 2 menu and popover; 3 dialog (with scrim). egui 0.29 `Shadow` supports blur and offset, so no new dependency.
- **Stroke**: 1 px hairline (borders, row separators, gridlines at 60% alpha), 2 px for chart lines and focus ring.

### 3.4 Motion

Short and purposeful, all through egui's `animate_*` helpers (no new crate). Hover and press 90 ms; panel open/close and tab change 150 ms; dialog enter 120 ms (fade plus 4 px rise); chart bars grow and lines draw in 250 ms **only the first time a chart appears or its data changes**; a tile that is still calculating shows a shimmering skeleton, then fades in over 120 ms. A "Reduce motion" setting turns all of it to instant. Nothing animates continuously: the app asks for a repaint only while an animation is running.

### 3.5 Density and responsiveness

Two densities: **Comfortable** (table row 32, control height 32) and **Compact** (row 26, control 28), chosen in Settings. Three width classes: compact (< 1000 px), regular (1000 to 1500), wide (> 1500, also 4K at scale). Compact windows fold side panels into the rail; dialogs cap at 90% of the window; a dashboard page whose scale falls below 0.7 reflows to 2 columns, then 1 (on screen only; exports keep the designed layout).

### 3.6 Spaciousness: why the screen feels cluttered, and the fix

Your observation (Power BI shows more tools yet feels airy; ours shows fewer yet feels cramped) has concrete causes in the code. Numbers below are estimates for the default 1320×740 window at the default zoom of 1.05; DS0.6 measures them properly.

**What causes it**

| # | Cause | Evidence in the code |
|---|---|---|
| 1 | **The canvas gets only about half the window.** Fixed bars take 116 pt of height (top bar 44, tab strip 40, status bar 32) before the view's own rows (filter row 30, header 38, summary band 38). The rail (68) plus the right panel (316 minimum) take about 31% of the width. On the Dashboard, the left pages panel (200) and right properties panel (216) are fixed and cannot be resized or collapsed, so panels take about 38%. | `theme.rs` `TOP_BAR_H`, `TAB_STRIP_H`, `STATUS_BAR_H`, `RAIL_W`, `RIGHT_PANEL_W`, `DASH_LEFT_W`, `DASH_RIGHT_W`; `dashboard.rs` `exact_width(...).resizable(false)` |
| 2 | **Outlines everywhere.** Every idle button, combo box and checkbox gets a 1 px border and a fill (`widgets.inactive.bg_stroke`), cards add another border, panels add separator lines, tables are striped. Many boxes of equal weight, so the eye sees boxes instead of content. (The top bar already switches borders off, which is why it looks calmer.) | `apply_ui_theme`, `card()`, `striped = true`, border helpers |
| 3 | **One tight rhythm.** Items are 8 pt apart horizontally and 6 vertically, buttons 30 pt tall; the gap between groups is nearly the same as inside a group, so nothing reads as a group and there is no pause for the eye. | `item_spacing`, `interact_size`, `button_padding` |
| 4 | **No figure and ground.** The top bar, status bar, right panel and content are all the same white slab separated by thin lines, so the content does not stand out from the chrome. Power BI puts a white page on a grey backdrop; the page pops, the chrome recedes. | `Tokens`: `surface` used for every region |
| 5 | **Tiles are crowded inside.** A KPI card stacks up to six small lines (value, change, reason, target, notes, period) plus a graphic. Tile text uses fixed point sizes while the page scale follows the window, so in a small window the same text sits in much smaller tiles (to be confirmed by DS0 screenshots). | `draw_kpi`, `render_dash_page` (`scale`), tile titles |
| 6 | **Everything is always visible.** Panels hide only in some places; toolbars show full-text buttons; tile settings appear as a floating popup over the tile being edited. | `dash_tile_panel`, `dash_pages`, `dash_design_props` |
| 7 | **Small start.** The window opens at 1320×740, and a 1080p Windows laptop at 150% display scaling gives only about 1280×720 usable points (to be confirmed on the office laptop). That is the size the app must feel good at. | `WINDOW_SIZE` |

**What Power BI does that we don't (plain words)**

1. It keeps the **work area** huge and everything else quiet: a grey backdrop, a white page, and panels that fold away with one click.
2. Buttons have **no outlines until you point at them**; space and a faint tone change group things, not boxes.
3. It leaves **bigger gaps between groups than inside them**, so you see five tidy groups, not thirty loose items.
4. Tools sit in **labelled groups** that you can collapse, and the properties of the selected visual open in a **side pane** instead of a popup on top of it.
5. A card shows **one number and one label**; detail waits until you hover.

**The fix: ten rules, each testable**

| ID | Rule | Acceptance |
|---|---|---|
| SP1 | **Canvas budget.** The work area is at least 60% of the window area with the right panel open and 75% with side panels collapsed, at 1280×720 and 1920×1080. | Layout test measures the central rectangle on screens S02, S05, S07, S09 |
| SP2 | **Chrome diet.** Fixed bars total at most 84 pt in Compact (top bar 40, tab strip 32, status bar 24) and 92 in Comfortable. One toolbar row per workspace; the rest goes into grouped menus or the inspector. | Test sums the fixed panel heights |
| SP3 | **Flat until hover.** Idle buttons and toggles have no border; only text inputs and combo boxes keep one; the primary action is filled. No striped rows by default. No box inside a box: a card never contains another bordered card. | Outline count per screen (stroked rectangles in the frame) falls at least 40% against the DS0 baseline on S02, S05, S07, S11 |
| SP4 | **Three-tier rhythm.** Inside a group 8, between groups 16, between sections 24; panel padding 16; section headings are semibold with space above them, not a ruled box. New tokens `gap_group`, `gap_section`. | Kit spec table; lint finds no literal gaps in kit-built panels |
| SP5 | **Figure and ground.** The backdrop is `canvas`; pages, tables and charts are `surface` with elevation 1; chrome sits on the backdrop tone and uses one hairline only where two regions meet. No double borders. | Screenshot check S02, S07; lint on panel frames |
| SP6 | **Collapse everything.** The rail (icon-only 48), right panel, Dashboard pages panel and properties panel each collapse and resize, remember their state per view, and fold automatically below 1100 points. Focus mode (Ctrl+Backslash) shows only the canvas. | Test: collapsed state persists and restores; `#[serde(default)]` |
| SP7 | **Disclose progressively.** A toolbar shows at most 7 items plus "More". Inspector sections are accordions with the first open. A KPI card shows value, change and label by default; reason, target text and notes move to the footer on hover or the inspector. | Test counts text lines in the default KPI card (at most 3) |
| SP8 | **Tile text follows the tile.** Tile titles, subtitles and legends scale with the page within clamps; below the clamp the subtitle and legend drop away instead of overlapping. | Layout test at three page scales (extends the DS2 test) |
| SP9 | **Charts breathe.** The plot area is at least 70% of the tile body height; no legend when direct labels fit; tile padding 16. | Layout test on the 12-tile sample |
| SP10 | **Start big.** First run opens maximised (or 90% of the screen) and remembers size and position; reference test sizes are 1280×720 and 1920×1080. | Settings test; DS0 captures both sizes |

SP1 to SP10 make the screen feel airy without removing a single feature: every tool stays one click, a menu or the command palette away.

---

## 4. The phases

Each phase lists: goal, tasks, acceptance criteria (each one is a test or a screenshot check), and risks. Test files live in `tests/design/` (`ds1.rs` and so on). Tasks are IDs a build session can pick up.

### DS0: Baseline and measuring tools (1 session)

**Goal.** Know where we are before changing anything, and build the tools that prove each later phase.

| ID | Task |
|---|---|
| DS0.1 | **Screenshot harness.** The render tests already dump frames (`tools/egui_mesh_png.py`) and charts (`tools/chart_layout_png.py`). Add one test, `design_gallery`, that renders the 16 canonical screens (list in section 5) at 1320×740, light and dark, to `docs/design/shots/<phase>/`. A contact-sheet script puts them side by side with the previous phase. |
| DS0.2 | **Audit.** `docs/design-audit.md`: every raw colour (`Color32::from_rgb`, `hex(`) and raw size outside the token files, counted per file; the list of duplicate palettes (UI, chart, `page.css`, `pptx.rs`, `dashboard_pdf.rs`); the list of components drawn by hand. |
| DS0.3 | **Speed baseline.** A hidden test or `--bench-ui` flag that renders N frames of fixed scenarios (data table 100k rows; dashboard with 12 tiles idle, hovering, resizing; chart tab with 5,000 marks; pivot 200×40) and writes median and 95th-percentile frame time, number of `layout_chart` calls per frame, and shapes per frame to `docs/design/perf-baseline.json`. Plus cold start time, idle CPU and peak memory, measured once by hand on the reference laptop (below). |
| DS0.4 | **Specimen sheet.** One screen showing the type scale in Source Sans 3 (tabular figures check on a column of Crore values) and the current palette, for the owner to react to. |
| DS0.5 | **Export capture.** Confirm what the cloud container can render: PDF to PNG, HTML to PNG (Playwright is installed; `tools/html_chart_png.js` exists), PPTX to PNG (LibreOffice if present). Record what can be checked automatically and what the owner must open in real PowerPoint and Excel. |
| DS0.6 | **Spaciousness measures.** For screens S02, S05, S07 and S09 at 1280×720 and 1920×1080, panels open and collapsed: canvas share of the window, fixed chrome height, count of outlines (stroked rectangles), text lines in a KPI card, plot-area share of a chart tile. Written to `docs/design/space-baseline.json`; the targets are SP1 to SP10 in section 3.6. |

**Acceptance.** 16 baseline screens exist in both themes; `design-audit.md` lists counts; `perf-baseline.json` has numbers for 5 scenarios; the report says which exports can be pictured automatically. No source behaviour changes (full `cargo test` unchanged).
**Risk.** Frame timing in a headless rig is not the same as on the owner's PC. Mitigation: the *counters* (layout calls, shapes) are the hard gates; timings are advisory and compared only against the same machine's baseline.

### DS1: One token source and the theme engine (2 sessions)

**Goal.** One place defines colour, type, spacing, radius, elevation, motion. App, charts and every export derive from it.

| ID | Task |
|---|---|
| DS1.1 | New library module `src/design/` (library crate, so exporters can read it; `ui/theme.rs` keeps only egui adapters). Holds the role tokens of 3.1 to 3.4 for light and dark. |
| DS1.2 | **Theme engine.** A `Theme { name, brand, optional accent, series set, header band colour, logo }` produces a full token set from one brand colour: it derives the brand ramp by lightness steps, picks `text_on_brand` by contrast, and nudges any role that fails 4.5:1. Presets and files live in `portable_data/themes/*.json` (`#[serde(default)]` on every field). The default theme reproduces the new palette of 3.1. |
| DS1.3 | Rewire `ui/theme.rs` `Tokens` and `ChartInk/SERIES` from `design`. Remove the warm-grey chart inks; charts take `surface` from the tile (transparent). Keep the validated 8 series colours (they passed the colour-blind validator) and add a one-hue **single-series default** (`brand`) and a "highlight one, grey the rest" mode (`deemphasis`). |
| DS1.4 | Generate the HTML `:root` CSS variables and the ECharts theme object from `design` at export time; delete the hand-typed colours in `assets/report/page.css`. |
| DS1.5 | `pptx.rs`, `dashboard_pdf.rs`, `xlsx.rs` read colours from `design` (delete `colour_of`, `rag_hex`, `rag_colour`, `tint` duplicates). |
| DS1.6 | **Checks.** Contrast test: every text role on every surface role meets 4.5:1 (3:1 for large text and graphical objects), light and dark. Colour-blind test: the series set keeps its validated pairwise distance. "No raw colours" test: `src/ui/**` and `src/charts/**` contain no `Color32::from_*` / `hex(` outside `design` and `theme`. |

**Acceptance.**
- Changing the brand colour in one file changes the app, a chart, the HTML export, the PDF and the PPTX together (a test exports all four and finds the new hex in each).
- No off-white rectangle inside a tile in the DS0 dashboard screenshot.
- All numbers on every screen and export are unchanged (calculation tests untouched).
- Old project files and macros load (`#[serde(default)]` test).

**Risk.** Retuning chart ink changes pinned values in `tests/phase7/c0.rs`. Those pins are updated deliberately and listed in the report. Re-run the palette validator after any series change.

### DS2: The dashboard look (4 sessions)

This is the phase that answers "looks like patchwork". It is where you will see the biggest change.

**Tile anatomy** (one function, `tile()`, used for charts, tables, KPI, text and image tiles):

```
canvas ─────────────────────────────────────────────
  ┌────────────────────────────────────────────┐   radius 10, border_subtle, elevation 1
  │ 16  Deposit mix by product      ⋯  ⤢       │   tile_title 14 semibold; actions appear on hover only
  │     BDT Crore · as of 31 Oct 2026          │   caption 12, text_secondary
  │                                            │   8 gap
  │  [ body: chart / KPI / table ]             │
  │                                            │
  │  Note or filter badge (only if present)    │   caption, text_tertiary
  └────────────────────────────────────────────┘   16 padding all round, 12 gap between tiles
```

| ID | Task | Session |
|---|---|---|
| DS2.1 | **Chart style v2 in the layout engine** (`charts/layout.rs`, one engine for screen and PDF): no in-chart title (the tile supplies it); horizontal gridlines only, 1 px at 60% alpha; no axis lines except the zero baseline; ticks 12 pt `text_tertiary`; values in compact Crore format ("1,250 Cr"; lakh/crore grouping kept, never "k" or "M"); legend moved to top-right, small, clickable to hide a series, and **dropped when direct labels fit** (line end labels, bar value labels); bars 3 px top radius, gap 40%; lines 2 px with a dot and value label on the last point; single-series area gets a soft vertical gradient (mesh with alpha vertices, no new crate); donut instead of pie (max 6 slices plus "Other", total in the centre); hover shows a crosshair and a popover-style tooltip; first-appearance draw-in animation. | 1 |
| DS2.2 | **Tile frame and page.** `tile()` as drawn above; page canvas uses `canvas`, tiles use `surface`; grid gap 12 units; titles one line with tooltip on truncation; consistent unit subtitle; the "bar above the title" drag handle in Free layout becomes an invisible top-edge grab that shows a move cursor and an outline on hover; selection shows a 2 px brand outline plus corner handles. | 1 **Tile settings move out of the floating popup (`dash_tile_panel`) into the right-hand inspector** (selected object, accordion sections, so nothing covers the tile being edited). The tile footer shows one **provenance line** (source tab, active filters, as-of date, unit) built by one library function that the status bar and every export reuse. |
| DS2.3 | **KPI card v2.** Label (13 secondary) on top; hero number (`display` 32, tabular) with unit as suffix in `caption`; **delta pill** ("▲ 4.2% vs Oct 2026", soft green/red background, arrow and words so colour is never the only signal); full-width sparkline with soft area fill under the number (replaces the fixed 160×28); target shown as a thin progress bar with the tick, bullets and gauges resize with the tile; threshold colour tints the pill and bar, not the whole number; empty and error states ("No data for 2026-11") in the same card. A KPI strip (4 across, shared height) is the default first row. | 1 **A number is never truncated:** the hero number shrinks to fit, and any shortened value in a card or table cell has a tooltip with the full figure. Default card text is at most 3 lines (SP7). |
| DS2.4 | **Table v2 and page header.** Table: no zebra by default, 1 px row separators, header `caption` semibold secondary on `surface_sunken`, numbers right-aligned tabular, negatives per the existing number-format setting, totals row semibold with a top rule, sticky header, optional in-cell data bars and heat tint (same sequential ramp as charts), row hover. Page header: dashboard title, **as-of chip**, filter pills (a chip per filter, click to open the value list, "×" clears) instead of combo boxes, "Presentation" button (full screen, type scale ×1.25, no editing chrome). | 1 |

**Acceptance (all four).**
- DS0's 12-tile dashboard screenshot, before and after, shows: grey canvas, white tiles with equal padding, no off-white boxes, one title style, one unit style, at most one legend per chart, no vertical gridlines.
- A unit test checks the tile function is the only place a tile border or padding is drawn (grep-style lint on `ui/dashboard.rs`).
- A KPI card at three tile sizes (2×1, 3×1, 4×2 grid cells) has no clipped or overlapping text (extend `layout_tests.rs`).
- Every number shown equals the same number before the phase (compare `ItemData` hashes of the Phase 10 sample dashboards: unchanged).
- Min window 800×560: nothing overlaps; DS2.4 compact reflow works below scale 0.7.
- A chart with 1 series uses the brand colour; with 3 series uses slots 1 to 3; with 9 series folds the smallest into "Other" (the existing rule, now visible).

**Risk.** Chart labels in the layout engine are measured by the same font code for screen and PDF; a new label style must keep that measuring shared, or PDF and screen drift.

### DS3: Speed and smoothness (2 sessions)

**Goal.** Frame time stays under 16.6 ms (60 fps) in the common cases and the app sits at about 0% CPU when nothing happens.

**Hotspots found by reading the code** (to be confirmed by DS0.3 numbers before fixing):

| Hotspot | Where | Fix |
|---|---|---|
| Every chart is laid out **twice per frame** (a "probe" for needed height, then the real layout), with no cache, for every visible chart, on every mouse move | `ui/chart_render.rs` `show_chart_with` | Cache the layout in egui temp memory, keyed by (data hash, options hash, rect size, theme); the probe becomes a cache hit. Only the hover overlay is drawn per frame. |
| `dashboard_tick` clones the whole `Dashboard` and the current page, and rebuilds every item's signature string, each frame the dashboard is open | `ui/dashboard.rs` | A revision counter on the dashboard and page; signatures recomputed only when the revision, a filter or a drill changes. |
| Release profile is `opt-level = "z"` (smallest size) with fat LTO | `Cargo.toml` | Measure per-package: `opt-level = 3` for the paint path (`epaint`, `egui`, `egui_plot`, our chart code) while the heavy engine keeps `z`/`s`. No dependency change. Keep the change only if exe growth is acceptable (target: under +15%). |
| Per-cell `format!` in tables and KPI text each frame | `ui/table.rs`, `ui/dashboard.rs` | Cache formatted strings per (row range, column, format) for the visible window; re-format only on scroll or format change. |
| Repaint requests: some paths call `request_repaint()` unconditionally while jobs run | `ui/steps.rs`, `ui/jobs.rs`, `ui/tabs.rs` | Repaint only while a job, an animation or a drag is active; `request_repaint_after` for polling. |
| Large charts (thousands of marks) emit one shape per mark | `charts/layout.rs` `Prim` list | Merge same-colour rectangles and triangles into one `Mesh` per colour; down-sample line series to pixel width (min/max per pixel column) with the full data still used for the tooltip. |

| ID | Task |
|---|---|
| DS3.1 | Layout cache and revision counters (the first two rows). |
| DS3.2 | Table string cache, repaint discipline, mesh merging, line down-sampling. |
| DS3.3 | Release profile experiment: measure `opt-level` options, exe size and frame time; write the decision into `design-system-decisions.md`. |
| DS3.4 | **Motion layer.** `motion` helpers over `animate_value_with_time`; hover/press/panel/dialog/skeleton timings from 3.4; "Reduce motion" setting; skeleton placeholder for tiles whose job is running (replaces text such as "Calculating the dashboard item"). |

**Acceptance (targets; DS0 baseline decides the exact figures).**
- Idle dashboard with 12 tiles: **0** `layout_chart` calls per frame after warm-up (deterministic test); hover over a chart: at most 1 layout call.
- Median frame time at least 3× lower than baseline on the 12-tile hover scenario, 95th percentile under 16.6 ms on the build machine, and the same figures re-measured on the reference laptop (below), which is the target that counts.
- Idle CPU under 1% with a dashboard open (nothing repainting); with Reduce motion on, zero animation repaints.
- Scrolling a 100k-row table: no frame over 33 ms.
- Cold start to first frame not slower than baseline.
- On the reference laptop, with the app and a 12-tile dashboard open on a 1-million-row file, memory stays under 2 GB, so Windows and Office keep room within 8 GB.
- No visual change except motion (screenshot diff against DS2 is empty).

**Risk.** Caches can show stale pictures. The key must include everything that changes a picture; a test changes each input (data, option, size, theme, unit) and checks the cache misses.

### DS4: Component kit and app chrome (3 sessions)

**Goal.** No screen draws its own button, input or container. Everything comes from `ui/kit/`.

| ID | Component | States and variants |
|---|---|---|
| DS4.1 | Button, icon button, toggle button, segmented control, checkbox, radio, switch | primary / secondary / ghost / danger; sizes S (28) and M (32); default, hover, pressed, focus ring, disabled, loading |
| DS4.2 | Text input, number input, combo box, search field, date picker field | default, focus, error with message below, disabled, with unit suffix (for Crore, %, bps) |
| DS4.3 | Chip, pill filter, badge, delta pill, tag | selectable, removable, status colours with icon |
| DS4.4 | Tab strip, rail, top bar, breadcrumb (drill path), toolbar | selected, hover, overflow menu, closable tab, drag to reorder One toolbar row per workspace, at most 7 items plus "More" (SP2, SP7); the rail collapses to icon-only 48. |
| DS4.5 | Menu, popover, tooltip (rich: title + body + shortcut), command palette | elevation 2, 150 ms enter, keyboard navigation |
| DS4.6 | Dialog frame (S 420 / M 640 / L 960), notice (info / success / warn / error, inline and toast), empty state (icon, one sentence, one action), progress and skeleton | dialog: title, body, footer with right-aligned actions, primary last, Esc closes, Enter confirms, scrim |
| DS4.7 | **Kit gallery** (`Help > Design kit`, also the screenshot target): every component in every state, light and dark | Also the **space kit** (section 3.6): flat-until-hover buttons, `gap_group` and `gap_section` tokens, a collapsible resizable panel that remembers its state, accordion inspector sections, a panel frame with no double borders (SP3 to SP7). |

**Acceptance.**
- Each component has a spec table in `docs/design/kit.md` (anatomy, sizes, tokens used, states) and a row in the kit gallery screenshot.
- Lint test: `egui::Frame::none()`, `Button::new` styled by hand, and `Color32` literals do not appear in `ui/*.rs` outside `ui/kit/`, with an explicit allow-list file for screens not yet migrated (the list shrinks each phase and must be empty at DS9).
- All dialogs pass the existing `assert_dialog_fits` at 800×560 and 1320×740.
- Focus ring visible on every interactive component (screenshot of the gallery with Tab pressed).
- Disabled controls always explain why on hover (the app already does this for the chart menu; the kit makes it the rule).

### DS5: The views (3 sessions)

| ID | View | What changes |
|---|---|---|
| DS5.1 | **Data view** | Grid per DS2.4 table rules; column header shows a small type icon (date, number, text) and a sort arrow; null cells show a muted "—" (never blank, never zero); selected column and row use `brand_soft`; filter pills row above the grid; summary band restyled as a quiet footer; density switch; right panel (column inspector) in the standard panel frame; schema dialog rebuilt on the kit table so its twelve fixed column widths become one column spec. Null, zero, blank and parse-failure cells look different from each other. |
| DS5.2 | **Report view and chart tabs** | Chart tabs use the tile frame (so a chart looks the same in a tab, a dashboard, a PDF); chart studio left panel in kit controls; pivot builder pop-out cleaned (nested vs side-by-side layout toggle stays); breadcrumbs for drill path. |
| DS5.3 | **Model view** | Tables as small cards with header band, key icon, field list; relationship lines 1.5 px with cardinality markers (1 and ∞), orthogonal routing, selected relationship highlighted; measure list in the kit table. |
| DS5.4 | **Assistant panel** | Panel header with model status dot (offline / loading / ready); user message as `brand_soft` bubble on the right, answer as a plain surface block on the left; source sections as chips ("Guide: Liquidity gap ladder") that open the note; the "Not from the guide book" label as a warn badge; input row with send button and shortcut hint; shimmer while answering; width 360 and collapsible. A mode label ("Guide book" or "Local model phrasing") and honest states: Searching guide, Preparing answer, No matching section. |
| DS5.5 | **Start screen and empty states** | First run and "no tab open": drop zone, recent files with size and date, "Open sample data", and the three main jobs (view and convert, analyse, build a dashboard) as cards. Every empty tab/view gets a purposeful empty state. |
| DS5.6 | **Status bar, notices, errors** | One notice style; errors say what to do ("Column 'Balance' is text. Convert it in the schema, then retry") with a button; long-running jobs show a progress pill with cancel. Status-bar items are clickable and open the relevant detail (parse failures, control total, filters). |

**Acceptance.** Screens S01 to S16 (section 5) all show the same header height, panel padding, title style and button style; no screen is on the lint allow-list at the end of DS5 except Settings and Export (done in DS8 and DS7); each view has a before/after pair in the report; screens fit at 800×560.

### DS6: Auto-explore, the X-ray (3 sessions)

Metabase's X-ray builds a starter dashboard from any table. MicroParquet's version works **offline, with fixed rules and no AI**, so the same table always gives the same dashboard, and every number reconciles to the table.

**Entry points.**
1. Right-click a tab, or the start screen: **Explore this table** (builds a new dashboard page).
2. Column header menu: **Explore this column** (distribution, top values, trend if a date exists, quality).
3. Chart or pivot mark menu: **Explore this segment** (the same table filtered to that branch, product or month, with a "compared with all" tile).

**How it decides (all deterministic).**
1. **Profile** with the existing `column_stats` and schema types: each column becomes *date*, *measure* (money or count), *rate* (percent), *dimension* (low-cardinality text, up to about 50 values), *identifier* (high-cardinality text) or *flag*. Schema hints win (BDT amounts, CBS dates, Crore display), then naming rules (balance, outstanding, rate, branch, product, wing), then statistics.
2. **Pick the story.** Main measure = the schema's balance column, else the numeric column with the largest sum. Main date = the one date column (latest data date is the as-of default, as in Phase 9). Dimensions are ranked by an interestingness score: number of values (3 to 20 is best), concentration (top 3 share), and spread across time.
3. **Recognise banking shapes.** If the columns match a known shape (deposit, advances, trial balance, investment, maturity ladder), use the matching Phase 7 template and Phase 6 pack recipe instead of generic tiles, for example gap ladder, deposit mix, CL migration.
4. **Compose** a page on the 3×4 grid, at most 12 tiles:
   - Row 1, KPI strip: total of the main measure at the as-of date with change vs previous period; row (or account) count; average rate if a rate exists; top-3 concentration share.
   - Row 2: trend of the measure by month (line) and the same stacked by the best dimension.
   - Row 3: top 10 by the best dimension (bar, one hue, top bar highlighted); composition of the second dimension (donut); distribution of the measure (histogram).
   - Row 4: biggest movers vs previous period (Phase 13 movers) and a **data-quality tile** (null %, duplicates, out-of-range, from the Phase 6 scorecard).
   - Page filters: as-of month and the two best dimensions.
5. Every tile carries a one-line "Why this tile" caption in its menu (for example "Largest dimension by balance; 5 values"). A banner says "Starter dashboard: every tile can be edited, moved or deleted."
6. **Fail loudly**: a column with mixed types or an unmapped code produces a notice tile saying what is wrong, not a guess. Any tile whose total does not reconcile with the table's own total (within rounding) is dropped and reported.

| ID | Task |
|---|---|
| DS6.1 | `explore.rs` in the library: profile to column roles, interestingness scoring, tile recipe list (pure functions, unit-tested, no UI). Output is a normal `Dashboard` value. |
| DS6.2 | UI: the three entry points, a progress pill (runs on the job queue, cancellable), the banner, "Why this tile", Undo (the whole generated page is one undo step). |
| DS6.3 | Banking-shape recognition and the 5 fixture tables (deposit, advances, trial balance, a plain sales-style table, a table with a bad column). Snapshot tests of the generated dashboard JSON. |

**Acceptance.**
- The five fixtures each produce a dashboard of 6 to 12 tiles with no error tile except the bad-column fixture, in under 3 seconds for 1 million rows (measured).
- Same input, same JSON (deterministic snapshot test).
- Every KPI and top-10 total reconciles to the table's sum (test).
- The generated page uses only DS2 tiles, so it looks like any other dashboard, in light and dark, and exports to PDF, PPTX and HTML.
- Stub proof: break the dimension scorer; the "best dimension" test fails.

**Risk.** Generic tiles can look arbitrary on odd tables. Mitigation: fewer, better tiles beat twelve; the generator may produce 6, and says why it skipped the rest.

### DS7: Export fidelity (3 sessions)

**Goal.** An exported dashboard looks like the screen. The same tokens drive all outputs, and the differences that cannot be avoided are listed.

**Parity matrix** (what must match, and the known limits):

| Element | App | PDF | HTML | PPTX | Excel |
|---|---|---|---|---|---|
| Tile frame (radius, border, padding) | yes | yes | yes (CSS) | rounded rectangle shape | cell block with border |
| Fonts | Source Sans 3 | embedded | embedded subset (woff2, base64) with Segoe UI fallback | **Segoe UI** (Source Sans is not on management PCs) with 10% slack in text boxes | Segoe UI / Calibri |
| Palette and brand | tokens | tokens | CSS variables + ECharts theme | theme colours in `theme1.xml`, so edited charts inherit | workbook theme colours |
| Chart style v2 | exact | exact (same layout engine) | ECharts configured to match | native editable charts, styled in XML | native charts, styled |
| KPI card | exact | drawn natively | HTML card | shapes and text | formatted cells |
| Gradient area, rounded bar tops | yes | yes | yes | solid fill (limit) | solid fill (limit) |
| Page header / footer | tile | logo, title, as-of, "Page n of N", confidentiality text | header bar | master slide footer | header/footer print setup |

| ID | Task |
|---|---|
| DS7.1 | **HTML.** Generated CSS variables and an ECharts theme from `design`; tile and KPI CSS equal to the screen; embedded font subset; responsive (columns reflow below 900 px); dark theme toggle follows `prefers-color-scheme` plus a button; `prefers-reduced-motion` honoured; ARIA roles and a "view as table" fallback per chart; the 5 MB email size check stays (fonts add under 100 KB). |
| DS7.2 | **PDF.** A4 landscape with the page header (optional bank logo from the theme), tile frames, KPI v2, tables v2; page numbers; "Internal, confidential" text as a setting; always white background (print-safe), even when the app is in dark mode. |
| DS7.3 | **PPTX.** Real `theme1.xml` with the bank theme colours and fonts, a slide master with title, footer and logo placeholder, native charts styled to chart style v2 (no chart title, light gridlines, series colours from theme so the user can recolour in one click), KPI cards as grouped shapes, tables with the table v2 style. 16:9 stays. |
| DS7.4 | **Excel.** Workbook theme colours; header style (bold, `surface_sunken` fill, bottom rule); number formats for Crore, lakh grouping, percent, bps and brackets for negatives; freeze panes; column widths fitted; gridlines hidden on report sheets; landscape, fit to one page wide; native charts styled (no border, light gridlines, theme colours, legend at top or none). |
| DS7.5 | **Parity checks.** One "export gallery" test builds the same sample dashboard to all four formats, renders what the container can (PDF to PNG, HTML through Playwright, PPTX through LibreOffice if available) and writes a contact sheet. Colour test: every hex in the outputs belongs to the token set. XML golden tests for PPTX chart parts. |

**Acceptance.** For the Phase 10 sample dashboard and the DS6 fixtures: the contact sheet shows App, PDF and HTML visibly the same design; the PPTX opens in PowerPoint (owner check) with editable charts and theme-coloured series; the Excel file opens with no "repair" prompt (owner check); every export number equals the screen number (the existing export tests); HTML under 5 MB for the email option.

**Risk.** Office renders text a little differently from our engine. Mitigation: slack in text boxes, and the owner's open-in-PowerPoint check is part of the acceptance, listed in the report.

### DS8: Bank themes, accessibility, density (2 sessions)

| ID | Task |
|---|---|
| DS8.1 | **Theme picker** in Settings: preview card showing a mini dashboard live; presets named by colour (not by bank): *Royal blue* (default), *Teal*, *Emerald* (Islamic banking green), *Crimson*, *Navy and gold*, *Graphite* (mono); **Custom**: pick one brand colour (and optionally an accent), the engine derives the rest, shows a contrast report, and fixes any failing role automatically. Optional logo (PNG or JPEG, resized, stored in `portable_data/themes/`), used on PDF, PPTX and HTML headers. Theme travels with a project file (so a colleague opens it with the same look) and has an "Apply to all new projects" choice. Light, Dark and **System** mode. |
| DS8.2 | **Density and zoom**: Comfortable/Compact setting; UI zoom range kept (0.5 to 2.0); chart text scales with zoom; presentation scale ×1.25. |
| DS8.3 | **Accessibility pass.** Contrast tests run for every preset in both modes (a preset that fails is not shipped); focus ring and keyboard order checked on every dialog; every chart and KPI exposes a text summary to screen readers through egui's accessibility tree (the painted charts currently have none; `data_table.rs` already produces the table text); **colour is never the only signal** (arrows and words on deltas, already the rule for KPI) extended to charts with a "pattern and shape" option (marker shapes for lines, hatching for bars) that also makes a greyscale print readable; minimum text size 12 on screen; motion reduce setting; HTML export ARIA. |
| DS8.4 | **Colour-blind check** of every preset's series set with the existing palette validator; the report lists the worst pair distance for each. |

**Acceptance.** Every preset passes contrast and colour-blind checks in light and dark (test); switching theme repaints the whole app, a chart tab and a dashboard without restart; a project saved with theme X opens with theme X; keyboard-only run through (open file, build chart, export) works, recorded as a checklist in the report.

### DS9: Hardening and release (1 session)

| ID | Task |
|---|---|
| DS9.1 | Empty the lint allow-list (no raw colours, no hand-drawn chrome left); delete the dead constants in `theme.rs`. |
| DS9.2 | Full before/after report: 16 screens, 4 export formats, speed table (DS0 vs now), the list of decisions and deviations. |
| DS9.3 | Guide book: update screenshots and add notes for themes, density, presentation mode, Explore. |
| DS9.4 | Release candidate (version bump follows your release routine), release notes, owner test checklist. |

**Acceptance.** Full `cargo test` green; `design-system-decisions.md` complete; owner checklist (section 6) passes on the owner's PC.

---

## 5. How each phase is verified

**Canonical screens** (rendered light and dark at 1320×740, plus S07 at 1000×640 and 800×560):

| # | Screen | # | Screen |
|---|---|---|---|
| S01 | Start screen / no tab open | S09 | Model view with relationships |
| S02 | Data view, wide table (30 columns, 100k rows) | S10 | Assistant panel, an answer with sources |
| S03 | Schema dialog | S11 | Settings dialog |
| S04 | Pivot builder (nested and side-by-side) | S12 | Command palette (Ctrl+K) |
| S05 | Chart studio | S13 | Export dialog |
| S06 | Chart tab, drill-down open | S14 | Notices: info, warning, error, empty states |
| S07 | **Dashboard, 12 tiles (4 KPI, 5 charts, 2 tables, 1 text)** | S15 | Explore result (from DS6) |
| S08 | Report view with several chart tabs | S16 | Kit gallery (from DS4) |

**Five kinds of check:**

1. **Before/after pictures.** Every phase's report has a contact sheet (previous phase on the left, new on the right) for the screens it touched. Only light is opened by the build session; dark pictures are written to `docs/design/shots/` for you.
2. **Deterministic tests** (hard gates): contrast ratios; colour-blind distances; no raw colours or hand-drawn chrome outside the kit; layout calls per frame; dialogs fit at 800×560 and 1320×740; KPI text never clipped at three tile sizes; tokens present in each export; old projects load.
3. **Number integrity.** The existing calculation tests and the export tests pass unchanged, and the Phase 10 sample dashboards produce identical `ItemData` before and after every phase. A design phase that changes a number is a failed phase.
4. **Speed.** Counters are hard gates; timings are compared with the DS0 baseline on the same machine and reported (median, p95).
5. **Owner check (10 minutes per phase).** A short checklist in the report: three things to look at, one thing to try, and "open the PDF/PPTX/Excel in the real program" where exports changed.

Stub proofs follow the lean-session rule: one or two key tests per task, named in the progress file (for example DS1: force the HTML hex to the old value, the token-parity test fails; DS3: remove the layout cache, the 0-layouts-per-frame test fails).

---

## 6. Owner acceptance checklist (used at DS9, and in short form each phase)

1. Open the same dashboard in light and dark; no tile looks different from its neighbour in padding, title or unit style.
2. Hover over every chart on the dashboard: no lag, tooltip appears at once.
3. Resize the window from 800 wide to full screen: nothing overlaps or disappears.
4. Click Explore on a deposit file and a trial balance: a sensible starter dashboard appears in seconds.
5. Export the dashboard to PDF, PPTX, HTML and Excel; open each in the real program; it looks like the screen, charts in PPTX are editable, and numbers match.
6. Change the bank theme to your bank's colour; the app and all four exports follow.
7. Use only the keyboard to open a file and build a chart.

---

## 7. Decisions I made in this plan (veto any; defaults are chosen so building can start)

1. One cool-slate neutral family for app and charts; charts have no background of their own.
2. **Royal blue** brand (your choice) in light **and** dark (the current amber dark accent becomes an optional theme).
3. Single-series charts use the brand colour; the 8-colour set is only for multi-series; "highlight one, grey the rest" for rankings.
4. Dashboard: grey canvas, white tiles with 10 px radius and soft shadow; dark mode uses lighter tiles plus a hairline instead of shadows.
5. Source Sans 3 for all text **including numbers** (tabular), mono only for code; Office files use Segoe UI.
6. Presets named by colour, never by bank (no trademarks).
7. Motion is modest and fully switchable; nothing animates when idle.
8. Release profile: try `opt-level = 3` for the paint path only, keep the exe growth under about 15%; decided by measurement in DS3.
9. Explore is rule-based and offline, deterministic, at most 12 tiles, and drops any tile that does not reconcile.
10. Print and PDF are always light.
11. No new dependencies in any phase.

**Answered (26 Nov):** brand colour is royal blue; the reference machine for speed targets is the office laptop (Intel Core Ultra 5, Windows 11, 8 GB RAM, integrated graphics); the desktop has 16 GB and is the easy case. Every speed and memory target is judged on the 8 GB laptop.

**Questions that can wait (a default is chosen for each):** (a) your bank's logo for the first custom theme; (b) whether PowerPoint at work has Segoe UI (default: yes, standard on Windows).

---

## 8. Suggested order and calendar

DS0 and DS1 first (everything depends on them). Then DS2, the visible win, **before** the kit, because the tile and KPI are the smallest kit pieces that matter most. DS3 immediately after, so all later work is built on a fast base. DS4 and DS5 next, DS6 once tiles are stable, DS7 after DS2, DS4 and DS6 (so exports pick up final styling once), DS8 and DS9 last.

Roughly: week 1 DS0, DS1, DS2.1; week 2 DS2.2 to DS2.4, DS3; week 3 DS4, DS5.1 and DS5.2; week 4 DS5.3 to DS5.6, DS6; week 5 DS7, DS8, DS9. Each week ends with a before/after report. A phase can be merged separately; each leaves the app consistent, never half-styled (the lint allow-list is what lets old screens keep working while they wait for their phase).

*Later, as a product feature (Phase 15 candidate), not design work:* a Model validation view with error, warning and information levels, and align/distribute and multi-select on the Free dashboard layout.

*Not in scope:* new calculations, new banking functions, cloud or web features, a web-based UI, an AI-generated dashboard (Explore is rule-based on purpose, so the same data always gives the same result and every number can be traced).
