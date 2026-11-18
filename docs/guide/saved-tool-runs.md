# Saved tool runs

## 1. What it does

Keeps everything you gave a tool (the cutoff date, which column is the balance, the files used, the options and Tier 1) in one small file, so the same result tab can be made again next month in a few clicks. It works for the repricing gap, gap ladder, IRRBB cash-flow ladder, IRRBB six scenarios, CL migration, CL-1 summary, LCR/NSFR, IFRS 9 summary, period reconcile, join, stack and a pivot made into a new tab. A saved run names its source tabs ("Loans", "Sept") and never their position, so it still works when the tabs are open in another order.

## 2. When to use it

- A monthly return that always uses the same tool with the same columns and a new cutoff date.
- A colleague must repeat your gap ladder: send the run file, not a description.
- A check: re-run an old run on last month's file and compare with what was submitted.

## 3. Before you start

- Make the result tab with the tool as usual and check it.
- The tabs a run reads must be open when you re-run it: open the files, and apply the same steps as before.
- A result tab with steps of its own (a sort or filter added after the tool) cannot be saved; save those steps as a macro.

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Source tab name | Yes, for each tab the tool reads | The tab's title (the file name for a tab opened from a file) | Loans, Sept 2026.csv | Two tabs with the same title: the first is used. |
| Columns of each tab | As the tool needs | As the tool says | BAL, MAT, RATE | They are saved by name; a column that is gone is reported by the tool when you run it. |
| Rule, curve, beta, behaviour and map files | As the tool needs | Files in `portable_data` | `behaviour/2026.json`, `sample_basel.json` | Saved by name and read again when the run is made: a changed file gives a changed result. |

## 5. Step by step

1. Make the result tab with its tool (for example Banking ▸ Treasury & ALM ▸ "Liquidity gap ladder").
2. Right-click the result tab's name and choose "Save run…" (Ctrl+K: "Saved tool runs" lists the saved ones).
3. Type a name (no `/ \ : * ? " < > |`) and choose "Save". The file is `portable_data/runs/<name>.json`. A name that exists asks you to choose "Replace".
4. To use it again, open File ▸ "Saved tool runs…" (Ctrl+K: "Saved tool runs") and choose "Re-run with changes" next to the run. Or right-click a result tab and choose "Re-run with changes" to repeat that tab with other inputs.
5. The tool's dialog opens with the saved inputs. Change the cutoff date (or anything else) and run the tool as usual.

## 6. What you get

The tool's own dialog, filled in. Nothing is calculated until you press the tool's button, so you can see and change every input first. A run whose tab is not open tells you which one to open (for example "The tab 'Loans' is not open").

## 7. Check your result

- Run it unchanged on the same tabs: the new result tab has the same rows and numbers as the first one.
- For the IRRBB six-scenario run the footer lists the cutoff, cash-flow mode, options, floors, Tier 1, the shock file, the curve files and the beta and behaviour files: it is rebuilt from the saved run alone.
- Open the `.json` file in a text editor: it holds the tool's name, the source tab names and the settings, and no tab numbers.

## 8. Worked example

A join of a loan list with the grades of the grading file (the grading file repeats A3, "Keep first row per key" is ticked). The run is saved, read back from its file, and made again.

| ACNO | BAL |
| --- | ---: |
| A1 | 1.00 |
| A2 | 2.00 |
| A3 | 3.00 |

Test: `guide_saved_tool_runs`

| ACNO | Grade |
| --- | --- |
| A1 | STD |
| A3 | SS |
| A3 | SS |

Test: `guide_saved_tool_runs`

The made-again tab is the same as the first one:

| ACNO | BAL | Grade |
| --- | ---: | --- |
| A1 | 1.00 | STD |
| A2 | 2.00 | (blank) |
| A3 | 3.00 | SS |

Test: `guide_saved_tool_runs`

## 9. Common messages and fixes

- "The tab 'Loans' is not open. Open it (or the file of that name) and try again." Open the file or the tab the run was made from.
- "This tab has steps of its own, which a saved run does not keep." Save those steps as a macro, or save the run before you add steps.
- "This tab was not made by a tool whose run can be saved." Only result tabs of the tools above have a run.
- "A saved run called 'x' exists. Choose Replace to overwrite it." Choose Replace, or type another name.
- "The saved run … is not valid" in the list: the file was edited by hand and is damaged; the list names the file.

## 10. Limits and assumptions

- A run saves inputs, not results: change a rule file or a tab's steps and the same run gives another table.
- A stack run keeps the files and their schemas; when it is re-run in the Stack dialog each file is read with its own detected schema, so pick the schema option again if you used a saved one.
- The reconcile table is now sorted by key, so the same two months always give the same row order.
- A run file is for the same version of the tool or a later one; fields it does not know get their defaults.
