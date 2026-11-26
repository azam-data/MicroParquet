# What-if scenarios

## 1. What it does

Lets you ask "what if deposits grow 5% and rates rise 100 bp?" without touching the data. A **scenario** has a **rate shift** and a **growth %**. The measures you mark change from the as-of month on; closed history stays actual. Every number a scenario changes is labelled "Scenario NAME: … These are not actual figures."

## 2. When to use it

- ALCO: deposit growth and yield under an optimistic and a stressed case, side by side with actual.
- A quick parallel-rate-shock view of NII using the IRRBB shock size in force.
- Showing management a KPI card under a scenario ("Stressed") for discussion only.

## 3. How to use it

1. Set the as-of date in Model first, and add the measures in Model ▸ Measures ([Measures and ratios](measures-and-ratios.md)).
2. Report view ▸ **Analytics** tab ▸ card **What-if**. Click "Start with Base, Optimistic and Stressed", or Add your own. The starter sizes (Base 0 / 0, Optimistic +5% growth, Stressed −10% growth) are assumptions to edit, not regulatory numbers.
3. For each scenario set **Name**, **Rate shift** (100 bp = 1 percentage point) and **Growth**. "Copy the IRRBB parallel shock" reads the size from the IRRBB shock rule file in force at the as-of date; it is never typed in the program.
4. Under **Measures it moves** mark each measure: **Volume** (multiplied by 1 + growth) or **Rate** (the shift is added), or "Not changed".
5. Pick the active **Scenario** ("None (actuals)" switches it off). Pivots, charts, KPI cards and the dashboard now show the scenario, with its label.

## 4. Example

Closing deposits (marked Volume) are 1,000 Cr in the as-of month; the yield on advances (marked Rate) is 9.0%. Under Stressed (growth −10%, rate shift 0) deposits read 900 Cr from the as-of month on, labelled "Scenario Stressed". A scenario with a +100 bp rate shift makes the yield 10.0%. Earlier months are unchanged.

## 5. What it refuses and why

- Only amounts can be marked Volume; only ratios shown in percent can be marked Rate; plan measures cannot be marked. A wrong mark is refused with a plain message.
- A window that reaches the as-of month is changed as a whole.
- A forecast is fitted to actuals only; the scenario is applied afterwards to the as-of month and the forecast months ([Forecasting](forecasting.md)).
- "Show rows" under a scenario lists the actual rows, and the tab says so ([Drill-through](drill-through.md)).
- Marks are model-wide, not per dashboard.

## 6. Python export

Not a table step: not in a table's Python export.

## 7. Saved in

The scenarios and the marks are saved in the project. Older projects load with none. Renaming or removing a measure updates the marks.

## 8. Related tools

[Forecasting](forecasting.md) · [Measures and ratios](measures-and-ratios.md) · [IRRBB six scenarios](irrbb-six-scenarios.md) · [Rate shock and ΔNII](rate-shock-nii.md) · [KPI cards](dashboard-kpi-cards.md)
