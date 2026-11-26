# Budget and target tables: actual against plan

## 1. What it does

Links a **plan table** (one row per month and key, with an amount such as Budget) to a data tab, then adds a **budget / target measure** that shows the plan, the **actual less plan**, or the **% achieved** (actual over plan, in percent). The measure is used like any other: in pivots, charts, KPI cards and dashboards.

## 2. When to use it

- Monthly deposit, advance or income budget against actual, by product or wing.
- A closing target (for example year-end CASA) against the latest balance.
- A KPI card that shows "% of budget achieved" ([Target bands](target-bands.md)).

## 3. How to use it

1. Prepare the plan table as a tab: a **month column** (a date; any day of the month will do), one to three **key columns** that also exist in the data tab (for example Product), and an **amount column** (Budget). One row for each month and key.
2. Keep the plan table open as a tab in the project. Its own steps (filters, renames) apply before it is read.
3. Model ▸ Links ▸ **Add budget table**. Choose the Data tab, the Plan table, the Plan month column and the key pairs, and confirm.
4. Model ▸ Measures ▸ **Add budget / target**. Choose the Plan table, the amount column, **Months add up as** ("Sum of the months (a flow budget)" for income or cost, "Last month (a closing target)" for a balance target), and **Shows**: Plan, Actual less plan, or % achieved. Choose the actual: a base measure or a ratio of the same tab.
5. Use the measure in a pivot or a chart, split by the plan's keys only (Product). A filter on a key (Product = A) applies to the plan as well as the actual.
6. Enter budgets with the same sign convention as the measure. "Actual less plan" is not signed by favourability: a cost above budget shows a positive number.

## 4. Example

Budget for product A: January 100, February 110, a flow budget (months add up as Sum). Actual: January 96, February 121.

| Period | Plan | Actual | Actual less plan | % achieved |
| --- | ---: | ---: | ---: | ---: |
| January | 100 | 96 | −4 | 96.00% |
| January–February | 210 | 217 | +7 | 103.33% |

A month that has started counts in full: a month-to-date actual is set against the whole month's plan.

## 5. What it refuses and why

- A repeated month and key in the plan: refused when the link is added and again when the pivot is built (never summed).
- A blank month or key: refused.
- A month the plan has no row for: the cell is blank with the reason ("The plan has no row for B in Feb 2026"), never a smaller sum or a zero.
- A plan of 0 gives no % achieved.
- A pivot split by anything other than the plan's keys, or filtered by a column that is not a plan key: refused with the keys named.
- A plan measure cannot be set against another plan measure or used as a ratio term. A plan table that a measure reads, or the actual of a plan measure, cannot be removed until the measure is.
- The plan table is not open: "The plan table of '…' is not open. Open it to use its budget or target."

## 6. Python export

Not translated: a plan link is a model link, and a table that depends on links is not in the table's Python export. Use the Join tool to copy the column, or export the result tab.

## 7. Saved in

The plan link and the measure are saved in the project's model. Older projects load with none.

## 8. Related tools

[Links between tabs](model-links.md) · [Measures and ratios](measures-and-ratios.md) · [Target bands](target-bands.md) · [Time intelligence](time-intelligence.md) · [Key check](key-check.md)
