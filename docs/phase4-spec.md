# MicroParquet — Phase 4 specification (ALM core)

Source: the agreed roadmap (Phase 4 = ALM core), the v1.1.1 review's ALM findings, the owner-approved pivot design `docs/design/Pivot.dc.html`, and the owner's request for a tutorial-style guide to every banking function. Sessions are told which task IDs to do; read only those task sections plus `<constraints>`, `<guide_notes>`, `<workflow>` and `<report>`.

<goal>
Phase 4 is done when:
1. Tasks G0 and V1–V5 are implemented; `cargo build --release` and `cargo test` pass.
2. Every acceptance item is an automated assertion; every UI item also has a headless egui render test at 1219×686 pt (105%) and 1600×900 pt (80%) asserting on real response rects (nothing overflows its panel, dialog or card).
3. Every banking tool, old and new, has guide notes in `docs/guide/` following `<guide_notes>`.
4. Calculation results of existing steps are unchanged (existing expected values stay), and Phase 1–3 macro, schema, preset, rule and curve files still load.
</goal>

<guide_notes>
The owner will receive a tutorial-style book built from these notes, so they must be exact and practical. One file per tool: `docs/guide/<tool-id>.md`, written in plain English for a bank ALM/treasury officer, with these sections in this order:
1. **What it does** — two or three sentences, including the regulatory or business purpose.
2. **When to use it** — typical monthly/quarterly tasks.
3. **Before you start** — files needed (e.g. deposit extract, loan extract, curve file), settings to prepare (cutoff date, presets, rule files), and steps that must run first.
4. **Columns you need** — a table: Column role | Required? | Type the app needs | Example values | Notes (e.g. "D/M/Y dates are fine"; "set Dr/Cr sign"). Use roles, not fixed names, since the user picks columns in the dialog.
5. **Step by step** — numbered clicks from the open tab to the result, naming each control exactly as it appears in the UI, including the Ctrl+K command name.
6. **What you get** — every output column or table: name, meaning, unit, sign convention.
7. **Check your result** — how to reconcile: control totals, totals to compare with the GL/trial balance or a regulatory return, and typical sanity ranges.
8. **Worked example** — a small table of synthetic input and the exact expected output, copied from that tool's tests (so the example is proven).
9. **Common messages and fixes** — each error or warning text the tool can show, what it means and what to do.
10. **Limits and assumptions** — what the tool does not do yet (and the phase planned to add it, if any).
Keep each file under ~2 pages; use the UI's exact labels. When a later task changes a tool, update its guide file in the same commit.
</guide_notes>

<tasks>

<task id="G0" name="Guide notes for existing banking tools">
- Write `docs/guide/` notes, per `<guide_notes>`, for the tools that exist today: Maturity bucketer, Rate profile, Interest-rate shock / ΔNII, Duration / DV01 / ΔEVE, FTP curve mapper, BB CL provisioning, Limit utilisation, Hygiene (dirty data / ghost) audit, Counterparty name normaliser, Period reconcile, Conditional column, Calc column, Filter, Sort, Summary row, Column inspector, Schema & Ingestion (types, Dr/Cr, masking), Stack, Join, Split export, Batch.
- Add `docs/guide/README.md`: a table of contents grouping tools as Data preparation, Treasury & ALM, Loans & credit, Export & automation.
- Take every worked example from an existing test; if a tool has no suitable test, add one and use it.
- Acceptance: a test checks each listed tool has a guide file containing all ten section headings, and that every worked-example table in it matches a test (reference the test name under the table).
</task>

<task id="V1" name="Pivot builder (Rows, Columns, Values, Filters)">
- Replace the "Pivot/WAVG" dialog with the approved design (`docs/design/Pivot.dc.html`): searchable field list with type badges; four areas Filters, Columns, Rows, Values; each Values field has its own aggregation (Sum, Count, Count distinct, Average, Min, Max, W.Avg with a weight column); Row totals, Column totals and Subtotals toggles; "Show values as" (No calculation, % of row total, % of column total, % of grand total); Scale; a live preview (first 20 rows) computed on the worker; "Create pivot tab".
- Fields move between areas by drag and drop and by a menu on each field (Add to Rows / Columns / Values / Filters). A Filters field opens a value checklist with search.
- Ordering: numbered labels such as "01. …" and dates sort in natural order; other text sorts with the Phase 3 natural case-insensitive order; never sorted by size unless the user picks "Sort by value".
- The pivot is a step (`PivotTable`, `#[serde(default)]` fields) that can be edited in place and replayed by macros and headless mode. The old `GroupByPivot` steps still load and replay identically.
- Enable the Polars `pivot` feature if needed (a feature flag, not an upgrade).
- Acceptance (frame: Bucket ∈ {"02. 2–7 Days", "10. 3–5 Years", "04. 1–3 Months"}, Bank ∈ {Conventional, Islami}, Balance, Rate):
  - Rows=Bucket, Columns=Bank, Values=Sum(Balance): row order 02, 04, 10; grand totals equal the column sums and the overall sum.
  - Values=W.Avg(Rate by Balance) per cell matches the Phase 2 rule (mixed-sign weights → null).
  - % of row total: each row sums to 100.00 (±0.01).
  - Filter Bank = Islami gives only that column and the right totals.
  - A v1.2 GroupByPivot macro replays to the same output as before.
</task>

<task id="V2" name="CL migration matrix">
- New tool under Banking ▸ Loans & credit: choose the previous-month tab and the current-month tab, the account key, balance and CL status columns (and optionally segment).
- Output tab: rows = previous grade, columns = current grade, using the `cl_code_map.json` order STD, SMA, SS, DF, BL, plus a "New" row (key only in the current month) and a "Closed/Repaid" column (key only in the previous month). Values: balance and account count (toggle), optional % of the previous grade's row total. Upgrades are shaded green and downgrades amber (using theme tokens).
- Unmapped codes stop with the Phase 1 error listing them.
- Acceptance: previous A1 STD 100, A2 SS 50, A3 STD 30; current A1 SMA 100, A2 DF 50, A4 STD 20 → STD→SMA 100 (1 account), SS→DF 50 (1), STD→Closed/Repaid 30 (1), New→STD 20 (1); row totals STD 130, SS 50, New 20; the grand total of previous-month balances equals 180.
</task>

<task id="V3" name="Liquidity gap ladder">
- New tool under Banking ▸ Treasury & ALM: choose one or more tabs, each with its role (Asset/inflow or Liability/outflow, or a side column with A/L values), amount column and maturity column; pick a bucket preset (from the maturity bucketer) and the cutoff date.
- Output tab, one row per bucket in preset order: Inflows, Outflows, Net gap, Cumulative gap, Net gap % of total assets, Cumulative gap % of total assets. Rows with null maturity follow the preset's sight/overflow setting.
- Limits: an optional limits file per preset (`portable_data/gap_limits/<preset>.json`: minimum cumulative gap % per bucket); a Limit column and a Breach flag (danger colour) when the cumulative gap % is below the limit.
- Behavioural (core/volatile) treatment of CASA is Phase 5: say so in the dialog and the guide.
- Acceptance: assets 100 in "0–1 month" and 200 in "1–3 months"; liabilities 150 and 50 in the same buckets → net −50, +150; cumulative −50, +100; total assets 300 → net gap % −16.67, +50.00; cumulative % −16.67, +33.33. With a limit of −10% on the first bucket → Breach on the first row only.
</task>

<task id="V4" name="Dated yield-curve store and FTP at origination">
- Curves live in `portable_data/curves/<YYYY-MM-DD>.json` as `{ "currency": "BDT", "as_of": "...", "source": "...", "points": [{"tenor_days": 30, "rate_pct": 8.0}, …] }`. A Curves manager (Banking ▸ Treasury & ALM ▸ Yield curves) lists curves, edits points, and imports a two-column CSV (tenor_days, rate_pct). Points must have strictly increasing tenors; invalid files are refused with the reason.
- The FTP step gains `curve_mode`: Fixed date (the curve on or before a chosen date) or Origination date (for each row, the latest curve on or before its open date). Interpolation is the Phase 2 setting (Linear default for new steps; NextPointUp for old macros). Output adds `FTP_Curve_Date`.
- A row whose open date is before the earliest curve: error by default, with the count of affected rows; an option "Use the earliest curve for older deals" applies it and sets `FTP_Note = "earliest curve used"`.
- Acceptance: curves 2026-01-01 {30: 8.00, 365: 10.00} and 2026-07-01 {30: 9.00, 365: 11.00}; a 197-day deal opened 2026-03-15 → 8.99701 (±0.00001) with curve date 2026-01-01; the same tenor opened 2026-08-01 → 9.99701 with curve date 2026-07-01; a deal opened 2025-12-01 → error naming 1 row, or with the option → 8.99701 and the note. Fixed date 2026-06-30 → both deals use the January curve. A CSV with tenors 30, 30 is refused.
</task>

<task id="V5" name="ΔNII with repricing dates, pass-through and floors">
- The rate-shock step gains, all `#[serde(default)]` so old macros replay unchanged:
  - `reprice_col` (optional): when set, the effective repricing date is the earlier of the repricing date and maturity.
  - `beta_file` (optional): `portable_data/betas/<name>.json` mapping product codes (with a product column picker) to pass-through % (0–100); unmapped products → 100% and are counted in the status message.
  - `floor_pct` (optional, default none): the shocked rate never goes below this value; the rate change is reduced accordingly.
  - A repricing-gap summary output: buckets 0–1m, 1–3m, 3–6m, 6–12m, >12m/non-repricing, with signed assets, liabilities, gap and cumulative gap, plus total ΔNII.
- Acceptance (cutoff 2026-10-31, +100 bp unless stated):
  - Floating loan, asset, 1,000,000, maturity 2030-06-30, reprices 2026-11-30 → weight (365−30)/365 → ΔNII +9,178.08.
  - Same loan without `reprice_col` → weight 0 (matures after 12 months, no repricing) → 0.00, matching the current behaviour.
  - Savings, liability, 1,000,000, no maturity, beta 40% → −4,000.00.
  - Savings at 1.50%, beta 100%, −200 bp with floor 0 → the rate change is limited to −1.50% → ΔNII +15,000.00 (without the floor +20,000.00).
  - An old rate-shock macro replays to exactly its previous output.
</task>

</tasks>

<constraints>
- Allowed dependency changes: the Polars `pivot` feature flag (V1). No other new crates or upgrades.
- Every new field on a serialised type gets `#[serde(default)]`; add old-macro fixtures (as in Phase 3) for the pivot and rate-shock steps and prove they replay identically.
- No change to existing results unless a task says so.
- Regulatory or bank-policy parameters (limits, betas, curves) live in dated or named data files in `portable_data`, never in code. Ship only examples clearly marked as samples.
- No `unwrap()`, `expect()` or index-panics on user-data paths or UI input.
- UI follows `docs/design/` and the Phase 3 theme tokens; every new dialog uses the shared dialog helper.
- Test data is synthetic only.
</constraints>

<workflow>
- Never run `cargo clean`. `cargo check` while editing; full `cargo test` before each commit.
- One commit per task (`Phase 4 <ID>: …`); push after each. Update the tool's guide file in the same commit.
- Keep `docs/phase4-progress.md` updated after each task; resume from it after a stop or compaction.
- Never stop to ask the owner mid-run: take the option that changes the fewest existing results, record it in `<accepted_deviations>` here, and list it in the report.
- Blocked three times on the same failure: mark BLOCKED, continue unless later tasks depend on it (V2 depends on V1's matrix rendering only if you reuse it; V3 and V5 are independent).
- Open a PR to main at the end, wait for the Tests check, fix failures, don't merge.
</workflow>

<accepted_deviations>
Recorded during the unattended Phase 4 run (each also appears in `docs/phase4-progress.md` and in the report).
- G0: the guide checker is `tests/phase4/guide.rs`. Each worked-example table is followed by `Test: \`guide_<tool>\``; that test builds the example, runs it through the real engine, asserts the numbers and compares the Markdown tables with the guide file, so a guide cannot drift from the code. Where an older test already had the figures (t05, t07, t09, t10, t11, t12, t16, t17, t25, t28, phase 3 t1) the example reuses its data and says so in a comment; balances were scaled to BDT 1,000,000 in the duration example so DV01 is not rounded to 0.04, and the maturity and hygiene examples add rows for the overdue and ghost cases.
- G0: UI labels, menu paths and error texts in the guides were taken from the source (`src/commands.rs`, `src/ui/*.rs`, `src/banking.rs`), not from a running window (this container cannot open one). Check the menu paths by eye.
- G0: guides use the ten headings as `## 1. What it does` … `## 10. Limits and assumptions`; section 4 always has the table header from the spec; a guide over 150 lines fails the test (the "about two pages" limit).
</accepted_deviations>

<report>
Reply in no more than 30 lines: a table (Task | Status | Tests added | Deviations), the final `cargo test` summary, questions for the owner, and a "look at by eye" list.
</report>
