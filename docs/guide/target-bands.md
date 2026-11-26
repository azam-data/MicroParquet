# Target bands: bullet and gauge

## 1. What it does

Adds a **target** to a chart or a KPI card with red, amber and green zones. The target is a fixed number (typed in the unit shown, Crore by default) or a budget / target measure on the same tab ([Budget and target tables](plan-vs-actual.md)). The zones are margins around the target as a percentage of it. A card is drawn as a **Number** (default), a **Bullet** or a **Gauge**. Status is also written in words ("98% of target · amber"), so the meaning never rests on colour alone.

## 2. When to use it

- ALCO or board page: deposit growth, NPL ratio, CASA ratio against its target at a glance.
- A column or line chart of a month series with the target line and the zones behind it.

## 3. How to use it

1. For a **chart**: Report view ▸ Analytics tab ▸ card **Target and bands** ▸ "Add a target to this chart". Set **Target** and its unit, **Good is** (Up for income and deposits, Down for cost and NPL), **Green within** and **Amber within** (percent of the target; amber is 10% by default), a **Line label**, and "Show the red, amber and green zones".
2. For a **KPI card** on a dashboard: select the card, **Target with red, amber, green**. Choose **Target from** "A number" or a budget / target measure, type the value (in the card's unit), the green margin and the amber margin, and the visual: Number, Bullet or Gauge ([KPI cards](dashboard-kpi-cards.md)).
3. Charts draw a neutral target line and three tinted bands on column, bar, line and area charts.

## 4. Example

Deposits target 5,000 Cr, good is Up, green margin 0, amber within 10%. At 5,020 Cr the card is green. At 4,900 Cr it reads "Target 5,000 Cr · 98% of target · amber". At 4,400 Cr it is red (below 4,500 Cr, 90% of the target).

## 5. What it refuses and why

- A target measure must be a budget / target measure on the same tab as the card; a card with no target reads as a plain number.
- Excel charts carry the target as a line, labelled with the target. The coloured zones do not reach Excel (a chart-library limit). PowerPoint, PDF and HTML show the zones.
- Bands are drawn on column, bar, line and area charts only.

## 6. Python export

Not a table step: not in a table's Python export.

## 7. Saved in

In the chart's settings and the dashboard card's settings: in the project. Old files without a target load without one.

## 8. Related tools

[KPI cards](dashboard-kpi-cards.md) · [Budget and target tables](plan-vs-actual.md) · [Chart](chart.md) · [Export to PowerPoint](export-pptx.md)
