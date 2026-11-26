# Top movers

## 1. What it does

For each month in a tab with a date column and a balance, lists the lines (accounts, GLs, products, branches) whose balance moved most, up or down. The balance of a line is taken on the **last date with data in that month** and compared with a starting point you choose. New and Closed lines are marked.

## 2. When to use it

- Monthly ALCO commentary: "which ten lines moved the deposits".
- Spotting a large new deposit or a closed account behind a change in a total.

## 3. How to use it

1. Open one long tab with a date column, a balance and a line column (the Phase 9 shape: one file with all dates).
2. Banking ▸ Reports ▸ **Top movers…** (Ctrl+K: "Top movers"). The dialog guesses the Date, Balance and Line columns; confirm or change them. A **Second line column (optional)** adds a second key.
3. Set **Movers per month** (10 by default) and **Latest months** (12 by default; 0 lists every month).
4. **Compare with**: Start of the year (the default: the year-end balance of the year before), Previous month, or Same month last year.
5. Choose **Show top movers**. The result is a normal tab: chart it, pivot it, export it.

## 4. Example

Month-end balances: December 2025 A 100, B 50; January 2026 A 130, B 40, C 20 (new). Changes: A +30, B −10, C +20 (New). Ranked by size: A 30, C 20, B 10. They add up to the month's total change, 40 (190 − 150).

## 5. What it refuses and why

- A blank line key, a blank balance or a row with no date is an error, not a zero.
- Mid-month dates are ignored: only the last date with data in each month counts.
- A month with nothing to compare with is left out, and the notes say which. A gap in the months is said ("Mar 2026 is compared with Jan 2026").
- Lines are matched trimmed and ignoring capitals. Change % is blank for a New line.
- It works on one tab with a date column; two month tabs are compared by [MoM variance](mom-variance.md).

## 6. Python export

Not translated: a result tab. Export the tab.

## 7. Saved in

As a result tab: in the project and in Saved runs and packs' run list ([Saved tool runs](saved-tool-runs.md)).

## 8. Related tools

[MoM variance](mom-variance.md) · [Variance bridge](variance-bridge.md) · [Report packs](report-packs.md) · [Pivot table](pivot.md)
