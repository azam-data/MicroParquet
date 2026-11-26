# Export the dashboard as an HTML report

## 1. What it does

Writes one HTML file with every page of the dashboard. It opens in any browser with no internet and no program: tabs for the pages, the page filters as dropdowns, a click on a bar that filters the other items, drill down, the KPI cards with their colours and arrows, pivots (nested ones open and close), and light and dark mode. The numbers in the file are the ones the app calculates: for every filter state and click the app has worked out the answer in advance, so the page only shows them.

## 2. When to use it

- Sending the monthly pack by email to people who only have a browser.
- Putting the report on a shared folder.

## 3. How to use it

1. Open the dashboard and press **Export…**, then choose **HTML report**.
2. Answer **Will you email this file?** first. Yes applies the 5 MB limit that mail servers set; No has no limit.
3. Press **Check size**. The app calculates every answer on a worker (the window stays usable) and shows the size and what it is made of. If an item has no click link (a pivot, or a chart that is not a plain sum or count) it is listed under "Not linked to clicks"; it still follows the filters.
4. When the size is accepted, press **Export** and choose the file. The file is written to a temporary name and renamed, so a failed write leaves no half file.

## 4. Example

Two filters of 12 and 4 values give 65 states for a chart (all, plus each value of each filter); a bar chart of 6 bars then holds 390 numbers at about 8 bytes each, 3,120 bytes. A 5.3 MB file for email is refused, and the suggestions come in this order: fewer filter values, fewer drill levels, Top N on the biggest chart, fewer pages, leave out the design picture. With fewer filter values the file comes under 5 MB.

Test: `r1_five_point_three_mb_by_email_is_refused_with_the_suggestions_in_order_and_fewer_filter_values_bring_it_under_5_mb`

## 5. What it refuses and why

- A file over 5 MB when you will email it: "Export…" stays off until it is smaller.
- An item over 20,000 numbers stops the export and says what to cut (fewer filter values, fewer drill levels, Top N). The file is never cut silently.
- A page filter whose values are not read yet: "The values of the filter 'Wing' are not read yet. Wait a moment and try again."
- Account-level rows never go into the file; only the numbers the items show do.
- Scatter, bubble, box plot, sankey, treemap and small multiples show as a table of their numbers with a note. Bars, lines, areas, donuts, waterfalls, heatmaps and histograms are drawn.

## 6. Python export

None.

## 7. Saved in

Nothing is saved about the export itself. The file is the output; the dashboard it came from is in the project.

## 8. Related tools

[Dashboards](dashboards.md), [Filters, clicks and drill](dashboard-filters-and-drill.md), [KPI cards](dashboard-kpi-cards.md), [PowerPoint](export-pptx.md), [PDF pages](export-pdf-pages.md), [Report packs](report-packs.md).
