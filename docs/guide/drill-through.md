# Drill-through: the rows behind a number

## 1. What it does

Opens the rows behind a number as a normal tab in the Data view, with the filter step visible, so they can be sorted, exported or changed. It works on **KPI cards**, on **pivot cells** of a base measure, and on **every kind of dashboard item**. The rows add up to the number shown.

## 2. When to use it

- A KPI card looks wrong: which accounts make it?
- A pivot cell for one branch and month: list the deals.
- Checking a dashboard page before it goes to ALCO.

## 3. How to use it

1. In the **dashboard**: double-click a KPI card; double-click a value cell of a pivot item (a nested parent line gives its whole group); right-click a chart mark ▸ **Show rows**.
2. In a **pivot** or **chart tab**: use "Show rows" on a cell or a mark ([Drill-down and Show rows](drill-down.md)).
3. The dashboard's page filters, the cross-filter and the item's drill path are applied, so the rows match what the item shows.
4. The rows tab opens in the Data view. Export it like any tab.

## 4. Example

A card shows closing deposits of 4,500 Cr for the Conventional wing. Double-click it: a tab opens with the deposit rows of the latest date, cut by the wing, and their balances add up to 4,500 Cr.

## 5. What it refuses and why

- A **ratio** has no single set of rows: refused, naming its numerator and denominator as the place to look. A **plan** measure's rows are not rows of this tab: refused with the reason.
- A measure that filters on a **Calendar field** is refused with its reason (the rows cannot be cut by a calendar field). Filtering on a column of the tab, or on a lookup field such as the branch master, works.
- Under a what-if scenario the rows are the actual rows; the tab says so ("Scenario X changes 'M' from the as-of month on; these are the actual rows").

## 6. Python export

The rows tab is an ordinary filtered tab; its filter step is in the Python export like any filter.

## 7. Saved in

As a tab, in the project if you keep it.

## 8. Related tools

[Drill-down and Show rows](drill-down.md) · [Dashboard filters, clicks and drill](dashboard-filters-and-drill.md) · [KPI cards](dashboard-kpi-cards.md) · [Balances and flows](balances-and-flows.md)
