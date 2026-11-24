# Rate profile (rate bands)

## 1. What it does

Puts an interest-rate column (in percent) into bands such as "8%–9%". It gives the yield or cost profile of the loan or deposit book: how much of the balance sits at each rate. The bands are your own choice; a rate exactly on an edge goes to the band you pick.

## 2. When to use it

- Deposit cost profile and lending yield profile for ALCO.
- Showing the share of fixed deposits above a policy rate.
- Pivot the bands against balance to see the rate distribution.

## 3. Before you start

- Open the extract with the rate column typed as a number (Schema & Ingestion → Decimal). Rates are in percent: 9.5 means 9.5%.
- Decide the bands: a fixed width (for example 0% to 15% by 1%) or your own edges (for example 0, 5, 7, 9, 12).
- Saved presets are kept in `portable_data/rate_bands.json`.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Interest rate | Yes | Number (percent) | 9.5, 12.25, blank | A text rate is refused: it would be read as blank. 9.5 means 9.5%, not 0.095. |
| Preset or edges | Yes | Chosen or typed in the dialog | 0% to 15% by 1%, or 0, 5, 7, 9, 12 | Edges must be numbers at or above 0. |
| Edge rule | Yes | Choice in the dialog | the lower band (a, b] | Decides whether exactly 9.00 is in 8%–9% or 9%–10%. |
| Number the bands | No | Tick box in the dialog, off | 09. 8%–9% | Off: plain labels ("8%–9%"). On: each label starts with a number, as before. |
| Output column name | Yes | Text | Rate_Band |  |

## 5. Step by step

1. Open a tab with the data, then choose Banking ▾ → "Rate profile (rate bands)…" (or Ctrl+K and type "rate band").
2. In "Rate profile: put a rate column into bands", pick "Rate column (percent):".
3. Pick a "Preset:" or choose "Fixed width" (fill "Start %", "End %", "Step %") or "Custom edges" (type "Edges (%):", for example 0, 5, 7, 9, 12).
4. Under "A rate on a band's edge goes to:" choose "the lower band: (a, b], so 9.00 falls in 8%–9%" or "the upper band: [a, b), so 9.00 falls in 9%–10%".
5. Leave "Number the bands (09. 8%–9%)" unticked for plain labels such as "8%–9%". Tick it only if a sheet or macro elsewhere expects the old numbered labels.
6. Read the band list and the label preview, set "Output column:" (default Rate_Band).
7. Optional: type a "Name:" and click "Save preset" to keep these bands.
8. Click "Add Rate Profile".

## 6. What you get

One new text column (default `Rate_Band`). The labels are "0%" (exactly zero), "0%–1%", "1%–2%" and so on, "Below 0%" for negative rates, "> 15%" beyond the last edge (or "≥ 15%" with the upper-band rule), and "No rate" for a blank rate. No rows or other columns change.

The column knows its band order: Below 0%, 0%, the bands from low to high, the top band, then No rate. A pivot, a chart or a sort on it lists the bands in that order without you choosing anything (not alphabetically, where "10%–11%" would come before "2%–3%").

With "Number the bands" ticked the labels start with a serial number ("00. 0%", "01. 0%–1%", "09. 8%–9%"), so they also sort in band order as plain text, for example in a spreadsheet. Steps, presets and macros saved before this version keep their numbered labels, so their results do not change.

## 7. Check your result

- Row count and control total are unchanged.
- Pivot Rate_Band by balance: the grand total equals the balance total of the tab.
- "No rate" rows are loans or deposits with no rate in the extract: count them and ask the branch or CBS team.

## 8. Worked example

Input: five loans with their rates (percent), default profile 0% to 15% by 1%, edge goes to the lower band.

| Actnum | Rate |
| --- | ---: |
| L1 | 9.00 |
| L2 | 8.50 |
| L3 | 0.00 |
| L4 | 15.50 |
| L5 | (blank) |

Test: `guide_rate_profile`

Output: the new column. Exactly 9.00 is in 8%–9% under the lower-band rule.

| Actnum | Rate_Band |
| --- | --- |
| L1 | 8%–9% |
| L2 | 8%–9% |
| L3 | 0% |
| L4 | > 15% |
| L5 | No rate |

Test: `guide_rate_profile`

## 9. Common messages and fixes

- "Rate column 'X' is String, not a number. Set it to a number in the schema first; a text rate cannot be put into bands." Re-open the file with the column set to Decimal, or use Set to Number.
- "Column 'X' was not found. Choose the rate column of this rate profile." The column was renamed or dropped.
- "'x' is not a number. Write the band edges as numbers in percent, like 0, 5, 7, 9, 12." Remove letters or % signs.
- "The step of the rate bands must be more than 0." / "The end of the rate bands (a%) must be above the start (b%)." Correct the fixed-width fields.
- "A band edge (x%) is below 0. …" Start the edges at 0 or above; negative rates are put in "Below 0%" automatically.

## 10. Limits and assumptions

- At most 500 bands.
- It only labels rows; it does not weight rates (use Pivot with W.Avg for weighted average rates).
- Bands are the same for all rows; there are no separate bands per product.
- The band order travels with the step. A table saved to a file and opened again has the labels but not the step: there the Ranges order (Custom sort lists) puts the plain labels in the same order.
