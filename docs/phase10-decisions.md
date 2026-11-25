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
