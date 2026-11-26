# A monthly dashboard routine

## 1. What it does

The steps from "the new month's files arrived" to "the pack is in the inbox", using a project that already holds the dashboard. Nothing is rebuilt by hand: the project rebuilds its tabs from the new files, the dashboard reads the tabs, and the exports write the files.

## 2. When to use it

- Every month end, after the trial balance and the account files are ready.
- After a correction: run it again and the new file replaces the old one.

## 3. How to use it

1. **File ▸ Open recent project** and open last month's project. The **Sources** dialog lists every file with Found, Changed or Missing.
2. Relink to this month's files: **Look in folder…** for the new folder (it relinks by file name), or **Browse…** one file at a time. A missing column is refused in words; fix the file, not the project.
3. Press **Open**. The tabs are rebuilt on the worker.
4. Open **Charts ▸ Dashboard**. Check the as-of date in the model's Calendar (View ▸ Calendar…); the cards and the footnotes show that month.
5. Set the page filters you want in the pack, then press **Export…**: HTML report (answer the email question), PowerPoint and PDF, in that order if you need all three.
6. File ▸ **Save project as…** with this month's name, so last month's stays as it was. Attach the files to the email.

## 4. Example

October's project is relinked to November's folder. Each card says November; the cards' earlier month is October, so "Closing deposits" shows the November number and its change against October. If a file for November is missing, its tab opens as a placeholder with the reason, and the dashboard items that read it say "source tab removed" or the tab's reason in words instead of showing October's numbers.

Test: `d1_renaming_a_source_tab_keeps_the_item_and_a_closed_one_says_source_tab_removed_and_the_title_opens_the_tab`

## 5. What it refuses and why

- A month with no rows gives a card with a dash and "No balance in Nov 2026 …", never the old number or zero.
- An HTML file over 5 MB for email is refused with what to cut ([HTML report](export-html-report.md)).
- A relinked file with a missing column stops that tab; the rest of the project opens.

## 6. Python export

None for the dashboard; each tab keeps its script.

## 7. Saved in

The project file holds the dashboard, the filters and the layout. The exported files are yours to keep; the app does not store them.

## 8. Related tools

[Projects](projects.md), [Dashboards](dashboards.md), [HTML report](export-html-report.md), [PowerPoint](export-pptx.md), [PDF pages](export-pdf-pages.md), [Shared calendar](model-calendar.md), [Monthly model](monthly-model.md).
