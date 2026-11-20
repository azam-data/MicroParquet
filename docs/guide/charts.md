# Charts: how to choose one

A short primer for a reader new to charts. The chart studio (Charts ▸ New chart) draws many forms; this note says which to pick, and what to leave out, so the chart says one thing clearly. The tool notes ([Chart](chart.md) and the banking templates at the end of this page) say how each control works.

## 1. Choose by the question

Start with the question your reader asks, not with the chart.

| The question | The form | Picture |
| --- | --- | --- |
| Which is bigger? (compare) | A sorted bar, or grouped bars for two series | ![Bars](img/charts/bar.png) |
| How does it change over time? | A line (or columns by month); an area when the total matters | ![Line](img/charts/line.png) |
| What is it made of? (composition) | Stacked columns; 100% stacked for shares; a donut for 2 to 5 parts; a waterfall for a bridge | ![Stacked](img/charts/stacked.png) |
| How are the values spread? (distribution) | A histogram, or a box plot for several groups | ![Histogram](img/charts/histogram.png) |
| Do two numbers move together? (relationship) | A scatter, a bubble (a third number as size), a heatmap for two categories | ![Scatter](img/charts/scatter.png) |
| Where did it flow? (HTML only) | A Sankey, or a treemap for the share of parts and groups | ![Sankey](img/charts/html-sankey.png) |

"Suggest a chart" in the studio applies the same rules: date and number gives a line, text and number a sorted bar, ordered text (buckets, tenors, grades) a column in that order, two numbers a scatter, two text columns and a number a heatmap.

## 2. Why a sorted bar beats a pie

The eye compares lengths from a common baseline well and angles badly. A sorted bar shows the order at once and every label has room. A donut is allowed only for two to five parts of one whole; with more, the studio refuses with "Use a bar chart: more than 5 parts are hard to compare as slices". There are no 3D charts and no gauges.

![A sorted bar](img/charts/bar.png)

## 3. Why one axis

A chart with two value axes lets the author make two lines cross wherever they like: the crossing means nothing. When two numbers have different scales (a gap in Crore and a ratio in per cent) the studio draws **linked panels**: two or three panels stacked on one shared x-axis, each with its own scale. The gap ladder is the model.

![Linked panels](img/charts/linked-panels.png)

Bars, columns and areas always start at zero, because their length is the number. A line may start elsewhere, and its axis then shows it.

## 4. What the colours mean

- **Blue is up and red is down** when the sign matters (gaps, ΔEVE, variances); the zero line is grey, never a hue.
- **Status colours** (green, amber, orange, red) are reserved for status and always come with words ("Breach", "Below Amber"); they are never a series colour.
- **Series colours** are eight fixed slots, used in order and never cycled; a series keeps its colour when another is filtered out, and a ninth folds into a grey "Other".
- **Text is never coloured**: the coloured mark beside it carries the identity.
- One hue in 13 steps shows size in a heatmap. In the dark theme the low values take the darkest step (nearest the surface) and the high values the lightest.

![A heatmap](img/charts/heatmap.png)

## 5. Write the title as a takeaway

A title that says what to see ("Parallel up is the worst case at −10.5% of Tier 1") beats one that says what it is ("ΔEVE by scenario"). The banking templates write such a title from the numbers by fixed rules (no AI); whatever you type in "Takeaway" replaces it. The subtitle says what is measured, the unit and the date, and the footnote the source and the method.

Label selectively: the value at the tip of a bar, the end of a line, every step of a waterfall, every cell of a heatmap, the total on top of a stack. A number on every point is noise; the full numbers are in the tooltip and the Table toggle.

## 6. When a number or a table is better

- **One number** (the LCR today) is better as a number with its minimum than as a chart of one bar; use a chart when you compare it with a limit or with its past.
- **A table** is better when the reader will look up exact values, when there are more than about 12 categories to label, or when the rows are the answer (a list of accounts).
- **A chart** is better when the shape is the message: a trend, a gap, a breach, a share.

Every chart in the studio has a "Table" switch and a "Chart tab" (Open as tab) for the table behind it.

## 7. The forms, with a picture each

| Form | Picture (light) | Picture (dark) |
| --- | --- | --- |
| Column | ![Column](img/charts/column.png) | ![Column, dark](img/charts/column-dark.png) |
| Grouped columns | ![Grouped columns](img/charts/grouped.png) | ![Grouped columns, dark](img/charts/grouped-dark.png) |
| Stacked columns | ![Stacked columns](img/charts/stacked.png) | ![Stacked columns, dark](img/charts/stacked-dark.png) |
| 100% stacked | ![100% stacked](img/charts/percent.png) | ![100% stacked, dark](img/charts/percent-dark.png) |
| Horizontal bars | ![Horizontal bars](img/charts/bar.png) | ![Horizontal bars, dark](img/charts/bar-dark.png) |
| Line | ![Line](img/charts/line.png) | ![Line, dark](img/charts/line-dark.png) |
| Stepped line | ![Stepped line](img/charts/stepped.png) | ![Stepped line, dark](img/charts/stepped-dark.png) |
| Area | ![Area](img/charts/area.png) | ![Area, dark](img/charts/area-dark.png) |
| Stacked area | ![Stacked area](img/charts/stacked-area.png) | ![Stacked area, dark](img/charts/stacked-area-dark.png) |
| Scatter | ![Scatter](img/charts/scatter.png) | ![Scatter, dark](img/charts/scatter-dark.png) |
| Bubble | ![Bubble](img/charts/bubble.png) | ![Bubble, dark](img/charts/bubble-dark.png) |
| Histogram | ![Histogram](img/charts/histogram.png) | ![Histogram, dark](img/charts/histogram-dark.png) |
| Box plot | ![Box plot](img/charts/box.png) | ![Box plot, dark](img/charts/box-dark.png) |
| Waterfall | ![Waterfall](img/charts/waterfall.png) | ![Waterfall, dark](img/charts/waterfall-dark.png) |
| Heatmap | ![Heatmap](img/charts/heatmap.png) | ![Heatmap, dark](img/charts/heatmap-dark.png) |
| Donut | ![Donut](img/charts/donut.png) | ![Donut, dark](img/charts/donut-dark.png) |
| Linked panels | ![Linked panels](img/charts/linked-panels.png) | ![Linked panels, dark](img/charts/linked-panels-dark.png) |
| Small multiples | ![Small multiples](img/charts/small-multiples.png) | ![Small multiples, dark](img/charts/small-multiples-dark.png) |

The same charts as a web page and as a native Excel chart are in [Chart](chart.md); "Copy as Python" gives a script to carry on in Python.

## 8. The banking templates

Each template is a chart a banker would draw for a tool's result, with a finding as its title. Each note has the same ten sections and a worked example whose title is checked by an automated test.

- [Gap ladder](chart-gap-ladder.md)
- [Repricing gap](chart-repricing-gap.md)
- [NII bridge](chart-nii-bridge.md)
- [CL migration](chart-cl-migration.md)
- [Classified trend](chart-classified-trend.md)
- [IRRBB scenarios](chart-irrbb-scenarios.md)
- [Yield curves](chart-yield-curves.md)
- [LCR / NSFR](chart-lcr-nsfr.md)
- [ECL by stage](chart-ecl-by-stage.md)
- [Deposit / loan mix](chart-deposit-mix.md)
- [Rate profile](chart-rate-profile.md)
- [Behaviour profile](chart-behaviour-profile.md)
- [Data quality](chart-data-quality.md)
