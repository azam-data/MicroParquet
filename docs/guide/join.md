# Join

## 1. What it does

Adds columns from one open tab to another by matching one or more key columns (for example adding the sector from a mapping table to a loan list). It refuses a key that appears twice on the right, because that would double the amounts on the left.

## 2. When to use it

- Adding sector, product group or branch name from a mapping table.
- Finding accounts that are in one list and not the other (anti join).

## 3. Before you start

- Two open tabs: the left tab (the main data) and the right tab (the lookup).
- A key column with the same name in both. Make keys the same type (both text).

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Key column(s) | Yes | Text or number (same in both tabs) | Actnum | Several keys may be typed separated by commas. |
| Join type | Yes | Choice | LEFT JOIN (All Left + Matching Right) | Also INNER, FULL OUTER, ANTI and SEMI. |
| Keep first row per key | No | Tick box | off | Off: a repeated right key is an error. On: the first right row is used and the status bar counts the rest. |

## 5. Step by step

1. Open both tabs.
2. Choose More ▾ → "SQL join of two open tabs…" (Ctrl+K: "join").
3. In "SQL-Style Multi-Tab Joiner" choose "Left Tab:" and "Right Tab:".
4. Type "Join Key Column(s) (e.g. Actnum):".
5. Choose "Join Type:".
6. Tick "Keep first row per key" only if the right side may repeat a key and the first row is acceptable.
7. Click "Execute Join into New Tab". The tab is titled "Join(left+right)".

## 6. What you get

A new tab with the left columns and the right columns (key once). For a left join, every left row appears once; left rows with no match get blanks. The status bar shows rows and totals of the left side and the result, and how many right keys were duplicated and dropped.

## 7. Check your result

- For a left join, the row count and the control total equal the left tab's.
- Anti join: the rows of the left tab not found on the right; semi join: those found.
- Count blanks of a right column to see unmatched keys.

## 8. Worked example

Left tab: two accounts.

| Actnum | Balance |
| --- | ---: |
| A1 | 1,000,000.00 |
| A2 | 500,000.00 |

Test: `guide_join`

Right tab: a mapping table; A3 is not in the left tab.

| Actnum | Sector |
| --- | --- |
| A1 | RMG |
| A3 | Textile |

Test: `guide_join`

Output (left join): A1 gets its sector, A2 has none, and the total balance is unchanged.

| Actnum | Balance | Sector |
| --- | ---: | --- |
| A1 | 1,000,000.00 | RMG |
| A2 | 500,000.00 | (blank) |

Test: `guide_join`

## 9. Common messages and fixes

- "Right table has N duplicated keys on Actnum, e.g. A1. Fix the mapping table, or switch on 'Keep first row per key' for this join." Remove the duplicate row from the mapping table, or tick the box.
- "Please open at least 2 tabs to perform a join."
- "Join: the selected left or right tab no longer exists. Pick both tabs again."
- A key that is text on one side and a number on the other does not match: make both text.

## 10. Limits and assumptions

- Matching is exact (case and spaces matter); clean keys first, for example with the Counterparty name normaliser.
- The join result is a new tab; the originals are unchanged.
- Null keys never match.
