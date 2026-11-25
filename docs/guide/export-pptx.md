# Export the dashboard as a PowerPoint file

## 1. What it does

Writes a .pptx with one slide for each item of the dashboard. Charts are real PowerPoint charts with their numbers in the chart's own sheet, so you can right-click and Edit Data. Pivots are real tables, KPI cards are shapes with the card's number, change and colour, with the trend line as a small chart. Every slide has a footnote with the as-of date and the filters that were on the screen.

## 2. When to use it

- The ALCO deck: take the slides and add your commentary.
- When the chart must be restyled by someone else in PowerPoint.

## 3. How to use it

1. Set the page filters and the click as you want the slides to show them; the file shows the screen's state.
2. Press **Export…** and choose **PowerPoint**: "One slide per item, with editable charts and tables."
3. Tick **Use design as slide background** if you want the design picture behind the slides. It is offered only when a page has a design, and the first page's design is used for every slide.
4. Press **Export** and choose the file. The status bar says how many slides were written.

## 4. Example

A bar chart item filtered by Wing = Islami gives one slide: the bars in the chart's sheet are the filtered numbers, and the footnote reads "As of 31 Oct 2026 · Filters: Wing = Islami". A pivot of 40 rows continues on two more slides titled "… (2 of 3)" and "… (3 of 3)", because a PowerPoint table does not scroll.

Test: `x1_one_slide_per_item_each_chart_part_has_its_chart_type_palette_and_workbook_ranges`

## 5. What it refuses and why

- A dashboard with no items: "The dashboard has no items to put on slides."
- A chart with measures in different units (money and a ratio) is a combo chart with a second axis on the right.
- A heatmap, a waterfall that crosses zero and the chart types PowerPoint has no chart for become a native table with a note.
- A file over 4 GB is refused before it is written.
- With no as-of date in the model the footnote says "As of date not set"; it never guesses one.

## 6. Python export

None.

## 7. Saved in

Nothing about the export is saved. The design picture and filters come from the dashboard.

## 8. Related tools

[Dashboards](dashboards.md), [Designs](dashboard-templates.md), [HTML report](export-html-report.md), [PDF pages](export-pdf-pages.md).
