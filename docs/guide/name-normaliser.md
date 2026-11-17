# Counterparty name normaliser

## 1. What it does

Cleans counterparty and customer names so the same company written in different ways gets the same text: upper case, punctuation removed, "Limited" written LTD, "Private" PVT, "Company" CO, "Corporation" CORP, "Incorporated" INC and "Bangladesh" BD, and spaces tidied. It supports counterparty exposure, large-exposure and concentration reports.

## 2. When to use it

- Before a Pivot of exposure by counterparty.
- Before a join on names when no common ID exists (check the result by eye).

## 3. Before you start

- A tab with the name column as text.
- Decide the new column's name (the original stays unchanged).

## 4. Columns you need

| Column role | Required? | Type the app needs | Example values | Notes |
| --- | --- | --- | --- | --- |
| Name | Yes | Text | ABC Industries Limited | Any text column. Blank names stay blank. |

## 5. Step by step

1. Open the tab, then choose More ▾ → "Counterparty name normaliser…" (Ctrl+K: "normaliser").
2. In "Counterparty Entity Name Normalizer" pick the "Source Name Column (e.g. Acttit):".
3. Check "New Normalized Column:" (default Normalized_Entity).
4. Click "Normalize Counterparty Names".

## 6. What you get

One new text column. The rules, in order: upper case; the characters . , - ( ) / become spaces; LIMITED → LTD; INCORPORATED → INC; COMPANY → CO; CORPORATION → CORP; BANGLADESH → BD; PRIVATE → PVT (whole words only); repeated spaces become one; leading and trailing spaces go. Other characters (for example "&") stay.

## 7. Check your result

- Distinct count of the new column is lower than or equal to that of the source; look at names that still differ by spelling ("INDUSTRY" vs "INDUSTRIES").
- Row count is unchanged.

## 8. Worked example

Input: four names for three companies.

| Acttit |
| --- |
| ABC Industries Limited |
| abc industries ltd. |
| Karim & Sons (Private) Limited |
| Bangladesh Steel Company Ltd |

Test: `guide_name_normaliser`

Output: the first two names become the same text; the original column is untouched.

| Acttit | Normalized_Entity |
| --- | --- |
| ABC Industries Limited | ABC INDUSTRIES LTD |
| abc industries ltd. | ABC INDUSTRIES LTD |
| Karim & Sons (Private) Limited | KARIM & SONS PVT LTD |
| Bangladesh Steel Company Ltd | BD STEEL CO LTD |

Test: `guide_name_normaliser`

## 9. Common messages and fixes

- "Column 'X' was not found" after a rename: pick the source column again with Edit on the step.

## 10. Limits and assumptions

- Only the word replacements listed above; no spelling or fuzzy matching.
- Not a legal-entity register: two different companies with the same name are still the same text.
