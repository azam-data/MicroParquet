# Export the dashboard as PDF pages

## 1. What it does

Writes a PDF with one A4 landscape page for each page of the dashboard. Charts are drawn as vectors by the same code as the chart PDF export, pivots as tables with their unit, KPI cards with the number in its limit colour, the arrow, the change and the trend line. A Free page keeps the places of its items; a design is drawn behind them. The footer says the as-of date, the filters in force and "Page n of N".

## 2. When to use it

- The printed ALCO pack and the file for the board portal.
- A fixed picture of the month that nobody can change.

## 3. How to use it

1. Set the filters and the click on the screen as you want them printed.
2. Press **Export…**, choose **PDF**, then **Export**, and name the file. There are no options.
3. Read the status bar. If a table was cut to fit its place, the message names it, for example "Balances: 12 more rows are in the HTML and PowerPoint files".

## 4. Example

A Free page with a chart at the left, a pivot at the right and a card below gives one A4 page; the text of the page holds the card's number as the card shows it, and each item sits within 1 mm of its place on the screen (the page's 1000 units are laid on 281 mm of A4 landscape width).

Test: `x2_pages_text_picture_footer_and_positions_of_a_free_layout_page`

## 5. What it refuses and why

- A dashboard with no pages or no items: "The dashboard has no pages to put in a PDF."
- A design picture that cannot be read: the export says so in words and writes nothing.
- A pivot that does not fit its place shows the rows that fit and a line saying how many more rows are in the HTML and PowerPoint files; it is never shrunk to unreadable size or drawn outside its place. Nested parent rows are indented but not bold.
- The picture is stored once for each page, as a JPEG, so the file stays small.

## 6. Python export

None.

## 7. Saved in

Nothing about the export is saved.

## 8. Related tools

[Dashboards](dashboards.md), [Layouts](dashboard-layouts.md), [Designs](dashboard-templates.md), [Report packs](report-packs.md), [PowerPoint](export-pptx.md).
