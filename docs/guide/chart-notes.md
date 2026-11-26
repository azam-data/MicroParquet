# Chart notes

## 1. What it does

Attaches a short note to a category of a chart ("Feb 2026: Policy rate cut"). The note appears as a numbered footnote under the chart in the app, PDF, HTML report, Excel and PowerPoint, and as a numbered marker over the category on the screen, PDF and HTML charts.

## 2. When to use it

- Explaining a jump on an ALCO chart: a rate decision, a merger, a holiday.
- Making a printed chart self-explaining.

## 3. How to use it

1. In the chart studio, open **Notes** (for the studio chart and for chart tabs).
2. Choose **Add note**. Choose its category from the chart's own categories (no typing mistakes) and type the text. Up to 9 notes per chart.
3. Export as usual. Each note is a footnote line "[1] Feb 2026: Policy rate cut"; markers are drawn on column, bar, line and area charts.

## 4. Example

On a monthly deposits line, note 1 on "Feb 2026": "Policy rate cut". The chart shows a marker 1 over February and the footnote "[1] Feb 2026: Policy rate cut" under it.

## 5. What it refuses and why

- A note whose category is not on the chart now (a drill level or a filter changed it) is not drawn, and the footnote says "Note [2] … is not shown: 'Jun 2026' is not on this chart now", so it is never lost without a word.
- Notes with no text are not written.
- A PowerPoint or Excel chart is a native chart: the numbers are in the footnote only, with no marker over the category. Other forms than column, bar, line and area keep the footnote line only.

## 6. Python export

Not a table step: not in a table's Python export.

## 7. Saved in

With the chart (in the project, saved charts and chart tabs).

## 8. Related tools

[Chart](chart.md) · [Export to PowerPoint](export-pptx.md) · [Export to PDF pages](export-pdf-pages.md) · [HTML report](export-html-report.md)
