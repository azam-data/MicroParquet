# Saved views of a dashboard page

## 1. What it does

Saves what a dashboard page's filters were showing (each page filter's picks and any click on a mark or row) under a name, so a reader returns to it with one click: "Conventional wing, Q4".

## 2. When to use it

- The same four pages of filters every month for ALCO.
- Giving a reader named starting points: "Islami wing", "Whole bank".

## 3. How to use it

1. On the dashboard, set the page's filters as you want them (and click a mark if needed).
2. In the toolbar's **Views** menu, type a **View name** and choose **Save view**. Saving the same name again replaces it.
3. To use a view, open **Views** and click its name. The view that matches the page now is ticked. The cross deletes a view.
4. To make a view always show the newest period, tick **Follow the latest period (Year, Year-Quarter or Month-Year filters)** before saving.
5. Views are saved with the page, so they are in the project file.

## 4. Example

Pick Wing = Conventional and Quarter = "2026 Q4", then Save view "Conventional Q4". Move to another wing; click "Conventional Q4" and both filters return. Saved with "Follow the latest period", the Quarter filter picks the newest quarter the data lists next time.

## 5. What it refuses and why

- A blank name is refused. At most 20 views for one page; names are unique ignoring capitals.
- A filter deleted since is said ("no longer on this page"), never silently dropped.
- Applying a view starts each item's drill path again from the top.
- "Latest" is found by time, not text ("Jan 2026" is after "Dec 2025"), and means the latest period in the data, not the as-of date's quarter. If a filter's values are not loaded or a value is not a period, applying the view says so in words and changes nothing.

## 6. Python export

Not applicable: views belong to the dashboard, not to a table.

## 7. Saved in

The project file (with the page). Picks are kept by the filter's id, so a view survives reordering the filters.

## 8. Related tools

[Filters, clicks and drill](dashboard-filters-and-drill.md) · [Dashboards](dashboards.md) · [Layouts](dashboard-layouts.md) · [A monthly dashboard routine](monthly-dashboard-routine.md)
