# Phase 13 decisions

Built autonomously; each choice below was made without asking. Rates are in percent, shocks in basis points, money in BDT shown in Crore.

## Drill-through (D1)
- Pivot cells and chart marks already had "Show rows" (Phases 8 and 9). Phase 13 closes the gaps: a **KPI card**, a **cell of a base measure**, and every kind of **dashboard item**.
- Base measures have rows. A KPI card or a pivot cell on a base measure (a column, an aggregation, optional filters) lists the rows of the dates its number reads, cut by the measure's own filters and the cell's row and column values. The rows add up to the number shown (tested for a closing balance, a filtered measure and a flow).
- Ratio and plan measures have no single set of rows. They are refused with the reason in words (a ratio names its numerator and denominator as the place to look; a plan measure says the plan table's rows are not rows of this tab).
- A measure that filters on a Calendar field is refused with its reason (the rows cannot be cut by a calendar field); filtering it on a column of the tab, or using a column value, works. This was the cheapest honest answer; lookup-field filters (branch master and the like) do work.
- A card has its rows through the same one-cell pivot that calculates it (`kpi::kpi_rows`), so the card and its rows cannot disagree. The card's page filters and clicks apply.
- On the dashboard: double-click a KPI card, double-click a value cell of a pivot item (flat or Nested; a Nested parent line gives its whole group), and right-click a chart mark then Show rows. The rows open as a normal tab in the Data view with the filter step visible, so they can be sorted, exported or changed. The dashboard's own page filters, cross-filter and the item's drill path are applied, so the rows match what the item shows.
- Under a what-if scenario the rows are the actual rows; the tab says so in its note ("Scenario X changes 'M' from the as-of month on; these are the actual rows").

## Top movers (M1)
- Banking ▸ Reports ▸ "Top movers". One tab with a date column and a balance; the dialog guesses the date column (first Date column, "date" in the name first), the balance (balance, outstanding, closing, amount) and the line (account, GL, head, customer, product, scheme, branch, name; else the first text column). The banker confirms or changes them and clicks once.
- For each month the balance of each line on the **last date with data in that month** is set against the previous month that has data. Mid-month dates are ignored (tested). A gap in the months is said in a note ("Mar 2026 is compared with Jan 2026").
- Lines are matched trimmed and ignoring capitals, as the MoM variance does. A blank line key, a blank balance or a row with no date is an error, not a zero.
- The 10 biggest changes by size, up or down (the number and the number of latest months, 12 by default, can be changed). A tie goes to the line that comes first by name. New and Closed lines are marked; Change % is blank for a New line. The listed changes add up to the month's total change when every line is listed (tested).
- The result is a normal tab (`TabSource::TopMovers`), so it can be charted, pivoted, exported and saved in the project and in Saved runs and packs' run list like the other tools.
- It works on one tab with a date column (the Phase 9 shape: one long file). It does not take two month tabs; MoM variance does that.

## Saved views (V1)
- A saved view is the **filter state of a dashboard page**: what each page filter had picked, and the click on a mark or row. It is saved with the page, so it is in the project file. Views menu in the dashboard toolbar: one click applies a view, the one that matches the page now is ticked, "Save view" saves the present state under a name, the cross deletes.
- Names are unique per page ignoring capitals (saving the same name replaces it); at most 20 per page; a blank name is refused.
- Picks are kept by the filter's id, so a view survives reordering the filters. A filter that has been deleted is said ("no longer on this page"), never silently dropped. A view saved with nothing picked is the "everything" view.
- Applying a view starts every item's drill path again from the top (a view is a repeatable state; a drill is exploration).
- **Not included:** "this quarter" as a moving period. A view stores the values picked ("2026 Q4"), not "whatever the latest quarter is", because a filter's value list is sorted as text and "latest" would be a guess for month labels. Pick the quarter again and save over it each quarter, or see the open question below.

## Chart notes (N1)
- A note is a category (chosen from the chart's own categories, so no typing mistakes) and a text. At most 9 per chart. Added in the chart studio under "Notes", for the studio chart and for chart tabs.
- Every output carries the text as a numbered footnote line "[1] Feb 2026: Policy rate cut": screen, PDF, the HTML report and chart files, Excel (the footnote under the chart) and PowerPoint (the slide's footnote). The screen, PDF and HTML charts also draw a numbered marker over the category (a disc at the top of the plot with a thin line down to the axis; on the HTML chart a marker with a tooltip).
- Known limit: a PowerPoint or Excel chart is a native chart, so it shows the numbers only in the footnote, with no marker over the category.
- A note whose category is not on the chart now (a drill level or a filter changed it) is not drawn and the footnote says "Note [2] ... is not shown: 'Jun 2026' is not on this chart now", so it is never lost without a word. Notes with no text are not written.
- Notes are drawn on column, bar, line and area charts (the forms that have a category axis with a value axis); other forms keep the footnote line only.

## Owner answers (26 Nov 2026) and what was done
1. **Latest period in a saved view: yes.** When saving a view, the box "Follow the latest period" marks every picked Year, Year-Quarter or Month-Year filter to follow the latest period. Applying the view picks the latest value that filter lists, found by time (so "Jan 2026" is after "Dec 2025"), not by text. Other filters keep their saved values. If the filter's values are not loaded yet, or a value is not a period, applying the view says so in words and changes nothing. "Latest" means the latest period in the data, not the as-of date's quarter.
2. **Top movers compares with the start of the year by default.** The dialog has "Compare with": Start of the year (default), Previous month, Same month last year. Start of the year means the year-end balance of the year before; the first year with data uses its first month, and a month with nothing to compare with is left out, and the notes say which. A tab saved before this choice existed keeps comparing month with month.
3. **PowerPoint: no note markers.** Kept as it is: the numbered notes are in the slide's footnote only.
