# MicroParquet guide to the banking and data tools

One short note per tool, in plain English for a bank ALM or treasury officer. Every note has the same ten sections: what it does, when to use it, before you start, columns you need, step by step, what you get, check your result, a worked example, common messages and fixes, and limits. The worked examples are tested: the tables in each note are produced by an automated test from the real calculation code.

The menus are File, Edit, View, Banking, Charts, Export and Help, then the toolbar buttons (Open, Stack, Batch, Filter, Sort, Columns, Calc column, Pivot, Rate profile, Chart, SQL) and the search box (Ctrl+K). The menus never fold, and the buttons always keep their labels; in a narrow window the rightmost buttons fold into the » menu, where they are listed with the same labels. View ▸ Toolbar buttons switches each button off or on, and View ▸ Side panel switches the Steps, Column, SQL and Chart tabs of the right panel; both are kept in settings.json. Edit has Undo and Redo, the transforms (conditional column, name normaliser, clean line breaks, join, copy table) and Data checks (the hygiene audit and the data-quality scorecard).

Copying the table: Ctrl+C or Edit ▸ Copy table puts the rows on the page on the clipboard, tab-separated, with each decimal number as a plain number in the unit the grid shows it in (no thousands separators, no unit suffix, at most 15 significant digits) and the unit in the header ("Sum of Balance (BDT Cr)", "Rate (%)"), so Excel pastes numbers it can add up. A right-click on a cell or a column header also offers "Copy as shown" (the text of the cells, "44,940.64 Cr") and "Copy raw (BDT)" (the stored, unscaled numbers, header "Sum of Balance (BDT)"). A blank is an empty cell in all three; a tab or line break inside a text becomes a space. Only the rows on the page are copied (the status bar says "Copied 100 of the table's 4,200 rows"); to copy all rows, export a CSV.

Window settings: View ▸ Zoom (or Ctrl+= / Ctrl+- / Ctrl+0, or a click on the percentage at the right end of the status bar) changes the size of everything; "Set current zoom as default" keeps it for the next start (`portable_data/settings.json`). The right panel is dragged wider by its left edge; the width is kept too.

Amounts are in BDT (shown in Crore, 1 Cr = 10,000,000, when you choose that scale). Rates are in percent (9.5 means 9.5%). Shocks are in basis points. CBS dates are normally D/M/Y.

## Data preparation

- [Schema & Ingestion (types, Dr/Cr, masking)](schema-ingestion.md): how a file is read when you open it.
- [Stack](stack.md): several files of the same layout, one under the other.
- [Join](join.md): add columns from one tab to another by a key.
- [Filter](filter.md): keep or remove rows by conditions.
- [Sort](sort.md): order rows by one or more columns.
- [Custom sort lists and Tenor order](custom-sort-lists.md): High, Medium, Low and 1m, 100D, 1y in the order you read them.
- [Calc column](calc-column.md): a new column from one arithmetic step.
- [Conditional column](conditional-column.md): IF / ELSE IF / ELSE flags and groups.
- [Clean line breaks](clean-line-breaks.md): one line for each value that was split over several lines.
- [Counterparty name normaliser](name-normaliser.md): one spelling for each company.
- [Pivot table](pivot.md): Rows, Columns, Values and Filters with totals.
- [Summary row](summary-row.md): totals and weighted averages under the table.
- [Column inspector](column-inspector.md): facts about one column.
- [SQL editor](sql-editor.md): a query on the tab's table, with history and the last error.
- [Chart](chart.md): the chart studio: bars, lines, areas, scatter, heatmaps and more over the whole tab, with HTML and Excel export, banking templates, chart books and "Copy as Python".
- [Charts: how to choose one](charts.md): a primer: choose by the question, why a sorted bar beats a pie, why one axis, what the colours mean, how to write a takeaway title, when a number or a table is better. Then one note for each banking chart template: [gap ladder](chart-gap-ladder.md), [repricing gap](chart-repricing-gap.md), [NII bridge](chart-nii-bridge.md), [CL migration](chart-cl-migration.md), [classified trend](chart-classified-trend.md), [IRRBB scenarios](chart-irrbb-scenarios.md), [yield curves](chart-yield-curves.md), [LCR / NSFR](chart-lcr-nsfr.md), [ECL by stage](chart-ecl-by-stage.md), [deposit / loan mix](chart-deposit-mix.md), [rate profile](chart-rate-profile.md), [behaviour profile](chart-behaviour-profile.md), [data quality](chart-data-quality.md).
- [Mudaraba profit distribution](mudaraba-profit-distribution.md): the Islamic window's weightage and PSR calculation, with parameters from a file.
- [Data-quality scorecard](data-quality-scorecard.md): pass percentages by rule, an overall Green/Amber/Red score, failing rows and a control total.
- [MoM variance](mom-variance.md): two months by product, the interest change split into volume, rate and mix, and commentary.
- [Report packs](report-packs.md): a monthly workbook (Cover plus one sheet per item) made from this month's files.
- [Saved tool runs](saved-tool-runs.md): keep the inputs of a result tab in a file and re-run the tool with other inputs.

## Treasury & ALM

- [Maturity bucketer](maturity-bucketer.md): residual maturity buckets (SLP, LCR, your own).
- [Rate profile](rate-profile.md): put a rate column into bands.
- [Liquidity gap ladder](gap-ladder.md): inflows, outflows, net and cumulative gap by bucket, with limits.
- [Behaviour profile](behaviour-profile.md): core and volatile parts of current and savings deposits, CPR and TDRR, saved as a dated file.
- [IRRBB cash-flow ladder](irrbb-cashflow-ladder.md): dated cash flows in the 19 Bangladesh Bank IRRBB buckets, by currency.
- [IRRBB (six scenarios)](irrbb-six-scenarios.md): ΔEVE, loss as a share of Tier 1, the outlier test and ΔNII in the six Bangladesh Bank scenarios.
- [LCR / NSFR](lcr-nsfr.md): liquidity coverage and net stable funding ratios from GL balances, through a map file.
- [Interest-rate shock / ΔNII](rate-shock-nii.md): one-year earnings sensitivity.
- [Duration, DV01 and ΔEVE](duration-dv01-eve.md): economic-value sensitivity.
- [FTP curve mapper](ftp-curve-mapper.md): funds-transfer-pricing rate, spread and margin.
- [Yield curves](yield-curves.md): dated curves for FTP, edited and imported.

## Loans & credit

- [BB classified-loan provisioning](cl-provisioning.md): grade, base for provision and required provision.
- [Classify by overdue](classify-by-overdue.md): the classification grade from the overdue period (BRPD 15/2024), with an override.
- [CL-1 summary](cl1-summary.md): loans by category and grade group in the layout of a template, with an Excel export.
- [IFRS 9 ECL](ifrs9-ecl.md): stage 1, 2 or 3 and the expected credit loss of each loan, with a summary against the regulatory provision.
- [Limit utilisation](limit-utilisation.md): limit use, over-limit and undrawn commitment.
- [Hygiene (dirty data / ghost) audit](hygiene-audit.md): ghost accounts, date, rate and balance problems.
- [Period reconcile](period-reconcile.md): two-period variance and CL migration.
- [CL migration matrix](cl-migration.md): last month's grade against this month's.

## Export & automation

- [Split export](split-export.md): one file per branch or other value, with a manifest.
- [Batch](batch.md): convert a whole folder tree.


## Exports: file names, file types and the PDF options

Every "save as" box offers only the file type it writes (CSV, Parquet, Excel workbook, PDF, macro, Python script), and a name typed without the extension gets it before anything is written, so an export is never a file without an extension. The box suggests a name from the tab: the tab's title, cleaned of characters Windows does not allow ("Pivot: Segment" is suggested as "Pivot_ Segment.csv"). For an Excel workbook or PDF of several tabs it is "<first ticked tab> and N more.xlsx".

PDF: "Export Selected Tabs to a .pdf…" first asks for a Title (blank = no heading line; whatever you type is printed at the top of every page, wrapped if it is longer than the page), the Orientation (Portrait or Landscape) and the Page size (A4 or Legal), then opens the save box, which suggests the title as the file name (or the tab's title when there is none). The last choices are remembered in settings.json. Columns that do not fit the page width continue on the following pages with the row labels repeated; a narrow page (Portrait) therefore needs more pages than a wide one.

Parquet, CSV, Excel, PDF and the split export all write through a temporary name and rename it when finished, so a failed export leaves no half file. The file signatures are PAR1 (Parquet), PK (Excel, a zip file) and %PDF (PDF); a CSV is plain text. If a file you exported opens as unreadable characters, look at its first characters in Notepad: PAR1 means it is a Parquet file, PK an Excel workbook, %PDF a PDF; then it was saved with the wrong export command or a wrong extension.
