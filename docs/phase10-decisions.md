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
