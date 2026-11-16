# MicroParquet — Phase 1 specification (correctness)

Source: code and calculation review of v1.1.1, 16 Nov 2026. This file is the single source of truth for Phase 1. Sessions are told which task IDs to do; read only the sections for those tasks plus `<constraints>` and `<workflow>`.

<goal>
Phase 1 is done when:
1. Every task T0–T9 below is implemented.
2. `cargo build --release` and `cargo test` pass with Polars 0.43.1.
3. Every acceptance check below exists as an automated assertion and passes.
4. Macro and preset JSON files saved by v1.1.1 still load.
Wrong numbers in this tool end up in ALCO packs and Bangladesh Bank returns, so correctness and explicit failure matter more than convenience.
</goal>

<project_context>
- Rust 2021 desktop app (eframe/egui 0.29). It is a single binary crate `micro_parquet` (bin name `MicroParquet`), version 1.1.1.
- Polars 0.43 with features: lazy, streaming, csv, parquet, strings, regex, temporal, dtype-date, dtype-datetime, fmt, sql, asof_join, semi_anti_join, partition_by, log, round_series, is_in, concat_str.
- Modules: `models.rs` (types, `TransformStep`, formats), `engine.rs` (CSV ingestion, schema inference, step compiler, group-by, filters), `banking.rs` (bucket, ΔNII shock, FTP, hygiene audit, reconcile, duration/ΔEVE, provisioning, limit utilisation), `macros.rs` (recipes and Python code generation), `exporters.rs`, `batch.rs`, `main.rs` (UI, 3,455 lines).
- Users are Bangladesh bank treasury and ALM staff. Dates in CBS exports are usually D/M/Y. Money is in BDT.
- `docs/review_harness.rs` is an existing test harness of 19 cases. It currently prints results instead of asserting them, and it imports from a crate named `mp_harness`.
</project_context>

<evidence>
These results came from running the v1.1.1 code. Use them as the failing baseline.
- t05: a Parquet `Datetime[μs]` maturity of 2026-11-05 with cutoff 2026-10-31 goes into "09. Over 5 Years". A string date column does the same. Cause: `col(x).cast(Int32)` gives null, and the null falls through every `when` to the overflow label.
- t01: inferred types were Account_Balance→Utf8, Residual_Maturity→Utf8, Reporting_Date→Utf8, Matdat ("05/11/2026")→Utf8, Paid_Amount→Utf8 and Liquidity_Bucket_Amt→Utf8. Causes: substring ID keywords ("account", "id", "tin"), and a leading-zero check that runs before date detection.
- t02: when every day in the sample is 12 or less, inference picks DateMDY.
- t06: `format_metric_value(25_000_000, name, …, BdtCrore, 2)` gave Modified_Duration→"25,000,000.00%" ("duration" contains "ratio"), Corporate_Loan_Balance→"%" ("corporate" contains "rate"), Account_Balance→not scaled ("account" contains "count"), Rate_Delta_Bps→"%" and Utility_Bill_Amount→"%".
- t07, t18 (provisioning): SMA→1%; CMSME SS→5%; "B/L"→performing at 1%; "NON-CMSME"→treated as CMSME; "AGRI TERM LOAN" DF→20%; B/L with security 950,000 on 1,000,000 → BFP 50,000 (no 15% floor).
- t08: WAVG of Rate [10.0, null], weighted by Balance [100, 900] → 1.0. Expected 10.0.
- t09, t10: ΔEVE and ΔNII have the same sign for assets and deposits. A matched 3-month loan and deposit show +7,534 each instead of netting to 0.
- t14: filtering Utf8 `Actnum == "12345678901234567"` returns 2 rows because of the f64 comparison.
- Join/reconcile modals in `main.rs`: `self.tabs[self.join_left_tab]` and `self.tabs[self.join_right_tab]` (around line 2692), and `self.tabs[self.rec_prev_tab]` and `self.tabs[self.rec_curr_tab]` (around line 2777), index with no bounds check. They panic after a tab is closed.
</evidence>

<tasks>

<task id="T0" name="Make the code testable">
- Add `src/lib.rs` that exposes the modules main.rs needs (models, engine, banking, macros, exporters, batch). Change `main.rs` to `use micro_parquet::…`. Keep the binary name and the `windows_subsystem` attribute.
- Move `docs/review_harness.rs` to `tests/review.rs` and change the imports to `micro_parquet`. Turn every printed result into `assert!` or `assert_eq!` using the expected values in each task below. Compare floats with a tolerance of 1e-6 × the magnitude.
- In `.github/workflows/main.yml`, add a `cargo test --release` step before the release build on both OS jobs. Add `cargo clippy` as an informational step, not blocking.
</task>

<task id="T1" name="Date maths: fail loudly, never silently bucket">
- In `banking.rs`, add one helper, for example `fn ensure_date(lf: LazyFrame, col_name: &str) -> Result<LazyFrame>`, that reads the schema with `LazyFrame::collect_schema()` (use the 0.43.1 API name):
  - `Date` → unchanged.
  - `Datetime(_, _)` → cast to `Date`.
  - `String`, or any other type → return `Err` with this message: "Column '<name>' is <dtype>, not a date. Set it to a Date type in the Schema & Ingestion settings."
  - Missing column → `Err` that names the column.
- Use the helper for every date input in `apply_maturity_bucket`, `apply_rate_shock_sim`, `apply_ftp_curve_map`, `apply_banking_hygiene_audit` and `apply_duration_analysis`, before the day-difference maths.
- In `apply_maturity_bucket`, decide the null label from the computed remaining days (`rem_days.is_null()`), not from the source column. A failed conversion must never reach the overflow bucket.
- Acceptance (t05): a Datetime[μs] column with 2026-11-05, 2027-03-15 and 2031-01-01, cutoff 2026-10-31, default SLP preset (`default_presets()[0]`) → "02. 2 to 7 Days" (5 days), "05. 3 to 6 Months (91-180d)" (135 days), "08. 3 to 5 Years" (1,523 days). A String column → `Err` containing "not a date". A null maturity with null_as_sight = true → "01. Overnight / Sight (Non-Maturing)".
</task>

<task id="T2" name="Schema inference (engine.rs infer_safe_column_type, models.rs is_likely_id_column)">
- Split column names into lowercase tokens on `_`, space, `-`, `.` and camelCase boundaries.
- A column counts as an ID only if (a) one token is in {brancd, branch, actnum, account, acct, glcode, gl, code, cif, id, tin, nid, phone, mobile, routing}, and (b) no token is in {bal, balance, amt, amount, outstanding, limit, sum, total, rate, int, interest, prov, provision, value, val, date, dt, maturity, profit}. Keep keywords that appear inside a single token (actnum, glcode, brancd) as whole-token matches only.
- Apply the leading-zero rule only to values made entirely of ASCII digits, with length > 1 and a first character of '0'.
- Order of checks: empty → ISO date → D/M/Y vs M/D/Y → float → text.
- When D/M/Y and M/D/Y both fit every sampled value, choose **DateDMY**. Choose MDY only when some value has a first part > 12.
- Acceptance (t01): Actnum→Utf8, Account_Balance→Float64, Residual_Maturity→DateDMY, Reporting_Date→DateDMY, Matdat→DateDMY, Opndat_US (10/31/2026 style)→DateMDY, Balance→Float64, Liquidity_Bucket_Amt→Float64, Paid_Amount→Float64. t02 → DateDMY. Add: "Customer_ID" with values "0012"→Utf8, and "BranchCode"→Utf8.
</task>

<task id="T3" name="Null-safe weighted average">
- In `engine.rs` `AggFunc::WeightedAvg`: let mask = value is not null AND weight is not null. Compute `(v*w).filter(mask).sum() / w.filter(mask).sum()`. Return null when the denominator is 0.
- Make the weight column required. If `weight_col` is None or empty, `compile` returns `Err("Weighted average needs a weight column")`. Remove the self-weighting fallback.
- Apply the same logic in the Python generator in `macros.rs`.
- Acceptance (t08): 10.0. Add a case where every weight is 0 → null.
</task>

<task id="T4" name="Asset/liability direction for ΔNII and ΔEVE">
- Add `#[serde(default)] side_col: Option<String>` to `TransformStep::RateShockSim` and `TransformStep::DurationAnalysis`.
- Sign rule (case-insensitive, trimmed): values starting with "A" (A, ASSET, ASSETS) → +1. Values starting with "L" (L, LIAB, LIABILITY), or equal to DEPOSIT or BORROWING → −1. Any other non-null value → `Err` that lists up to 10 distinct unmapped values (collect `unique()` on that column only).
- When `side_col` is None (old macros), behave as before but add a column `Direction_Assumed` = "ASSET (unspecified)", so the assumption shows in the output.
- Signed outputs: `NII_Impact_BDT` and `Delta_EVE_Impact_BDT` are multiplied by the sign. A rise in rates must reduce the EVE of assets and increase the EVE of liabilities. `DV01_BPV_BDT` stays unsigned.
- Add the Side column picker to both modals in `main.rs`. It is optional, but default it to a column whose name tokens include side, type or direction, if one exists.
- Acceptance (t10): +100 bp, loan 3m and term deposit 3m (1,000,000 each, maturity 2027-01-29, cutoff 2026-10-31) → +7,534.25 and −7,534.25, net 0 (±0.01). t09: asset ΔEVE −7.58, deposit +7.58 (2 dp).
</task>

<task id="T5" name="Token-based number formatting">
- Rewrite `classify_column_auto_format` in `models.rs` on whole tokens, in this order:
  1. Any token in {bps, bp} → StandardCommas.
  2. Any token in {duration, years, yrs, tenor, days, count, cnt, num, nos, freq} → StandardCommas. Also names starting with "no_of" → StandardCommas.
  3. Any token in {rate, pct, percent, yield, spread, util, utilization, utilisation, ratio, wavg} → PercentAsIs.
  4. Otherwise → Auto (monetary).
- Acceptance (t06, BdtCrore, 2 dp, value 25,000,000): Modified_Duration→"25,000,000.00", Macaulay_Duration_Yrs→"25,000,000.00", Corporate_Loan_Balance→"2.50 Cr", Account_Balance→"2.50 Cr", Discount_Amount→"2.50 Cr", Rate_Delta_Bps→"25,000,000.00", Utility_Bill_Amount→"2.50 Cr", Balance→"2.50 Cr", No_of_Accounts→"25,000,000.00", Interest_Rate→"25,000,000.00%".
</task>

<task id="T6" name="Type-aware filters">
- Change `build_filter_expr` so it gets the column's `DataType` from the schema.
- String or categorical columns: Eq, NotEq and the comparisons use text comparison on the trimmed value, even when the value looks numeric.
- Numeric columns: compare as f64. A value that is not numeric → `Err("'<value>' is not a number for numeric column '<col>'")`.
- Date columns: parse the value as `YYYY-MM-DD` and compare as Date. Anything else → `Err`.
- Acceptance (t14): exactly 1 row. Add: a Float64 Balance column with `Gt 100` still works, and a Date column with `Lt 2026-11-01` works.
</task>

<task id="T7" name="Provisioning engine, BRPD Circular 15/2024 (dated rules)">
Replace the hard-coded rates in `apply_loan_classification_provision` with data files in `portable_data/`. Create default files on first run if they don't exist. Loading uses `serde_json`. On a parse error, return an `Err` that names the file and the line.

1. `cl_code_map.json`: normalised CBS code → grade, where grade is one of STD, SMA, SS, DF, BL. Normalise by uppercase, trim and collapse internal spaces. Defaults:
   - STD ← STD, STD-0, STD-1, STD-2, UC, STANDARD, UNCLASSIFIED
   - SMA ← SMA
   - SS ← SS, SUB-STANDARD, SUBSTANDARD, SUB STANDARD
   - DF ← DF, DOUBTFUL
   - BL ← BL, B/L, BLW, BAD/LOSS, BAD, LOSS
   Any code not in the map → `Err` listing up to 20 distinct unmapped codes. Never default an unknown code to 1%.
2. `segment_map.json`: normalised segment code → category, where category is one of SHORT_TERM_AGRI, CMSME, OTHER. Exact match only, no regex. Defaults map CMSME, SME, CMS, COTTAGE, MICRO, SMALL and MEDIUM → CMSME, and AGRI ST and SHORT TERM AGRI → SHORT_TERM_AGRI. Unmapped values → OTHER, and the step adds a column `Segment_Unmapped` = true so they can be reviewed. "NON-CMSME" must map to OTHER.
3. `provision_rules.json`: a list of `{grade, category, rate_pct, effective_from, effective_to (nullable), source, verify}`. Rate lookup = the row matching grade and category (category "ANY" as fallback) where `effective_from ≤ cutoff ≤ effective_to`. Two matching rows, or none → `Err`. Seed these rows, keeping `"verify": true` on the ones marked so the owner checks them against the circular PDFs:
   - ANY: STD 1%, SMA 5%, SS 20%, DF 50%, BL 100%, from 2025-04-01 (BRPD Circular 15, 27 Nov 2024).
   - SHORT_TERM_AGRI and CMSME: STD 1% and SMA 1%, from the October 2025 BRPD circular letter (believed to be No. 22 of 2025) to the day before the December 2025 directive. Leave the exact dates as `"verify": true`.
   - SHORT_TERM_AGRI and CMSME: STD 0.5% and SMA 0.5%, from 2025-12-21 to 2026-12-31 (directive that replaced Circular Letter 22/2025; `"verify": true`).
   - SS, DF and BL for SHORT_TERM_AGRI and CMSME use the ANY rows. The concession covers only unclassified loans.
4. Add `#[serde(default)]` fields to `TransformStep::LoanClassificationProvision`: `cutoff_date: String` (if empty, use today and add a column `Cutoff_Assumed`) and `collateral_type_col: Option<String>`.
5. Base for provision:
   - STD and SMA: BFP = max(outstanding, 0).
   - SS, DF and BL: raw = max(outstanding − interest suspense − eligible security, 0).
   - If the collateral type, normalised, is in {DEPOSIT_LIEN, GOVT_SECURITY, GOVT_GUARANTEE, BB_GUARANTEE, AAA_MDB_GUARANTEE}, then BFP = raw. Otherwise (including when there is no collateral-type column and security > 0), BFP = max(raw, 15% × outstanding).
   - Eligible-security haircuts are out of scope for Phase 1. The security column is taken as already eligible. Say so in a code comment.
6. Output columns: `CL_Grade`, `Reg_Category`, `NPL_Category`, `Base_For_Provision_BDT`, `Statutory_Prov_Rate_Pct`, `Required_Provision_BDT`, `Rule_Source`.
7. Acceptance at cutoff 2026-10-31, balance 1,000,000 per row (t07, t18 rewritten):
   - UC CORPORATE → 10,000
   - SMA CORPORATE → 50,000
   - SS CORPORATE, suspense 100,000 → BFP 900,000, provision 180,000
   - DF CORPORATE, suspense 100,000 → 450,000
   - B/L CORPORATE, suspense 100,000 → 900,000
   - BL NON-CMSME, suspense 100,000 → 900,000
   - STD-1 CMSME → 5,000
   - SS CMSME → 200,000
   - BL CORPORATE, security 950,000, no collateral type → BFP 150,000, provision 150,000
   - Same row with collateral type DEPOSIT_LIEN → 50,000
   - "AGRI TERM LOAN" DF → 50% (segment unmapped → OTHER)
   - Code "XYZ" → `Err` that mentions XYZ
   - The same CMSME STD row at cutoff 2027-01-31 → 10,000, and at cutoff 2025-06-30 → 10,000
</task>

<task id="T8" name="No panics on stale tab indices">
- Use `self.tabs.get(i)` at the join and reconcile execution sites in `main.rs` (around lines 2692–2693 and 2777–2778). On None, set `self.status_message` and keep the modal open.
- When a tab is closed, clamp `join_left_tab`, `join_right_tab`, `rec_prev_tab` and `rec_curr_tab` to the new length, or reset them to 0.
- Search `main.rs` for any other `self.tabs[<index other than active_tab>]` and fix it the same way.
</task>

<task id="T9" name="Python export must not silently drop steps">
- In `macros.rs` `generate_python_polars_code`, for any step that has no Python translation (the banking steps that print "(Specialized Banking Step Executed via Polars Expressions)"), stop emitting the placeholder comment. Instead put `raise NotImplementedError("Step N '<label>' has no Python translation in this version; run it in MicroParquet")` at that point in the script, and add a warning to the UI status message when the script is exported.
- Emit casts for the ingestion schema: Float64 columns go through the same comma and bracket cleaning and are cast to Float64. Date columns use `str.to_date` with the matching format. The PII mask is out of scope: emit a `# WARNING: PII masking not reproduced` line when any column has a mask.
</task>

</tasks>

<constraints>
- Do not upgrade Polars, egui or any other dependency, and do not add new crates. `serde_json`, `chrono` and `anyhow` are already available.
- Every new field on a serialised struct or enum variant gets `#[serde(default)]`. A v1.1.1 macro JSON with a `RateShockSim`, `DurationAnalysis` or `LoanClassificationProvision` step must still deserialise. Add a test that loads such JSON from a string literal.
- Keep public function names. Extend signatures where a task needs it, and update every call site, including the headless path in `main.rs` and `macros.rs`.
- No `unwrap()`, `expect()` or index-panics on user data paths. Return `anyhow::Result` with a message a banker can act on.
- Change only what the tasks need. Don't run `cargo fmt` on untouched files, don't rename unrelated items, and don't start Phase 2 work (Dr/Cr parsing, raw-vs-parsed control sums, join validation, streaming exports, background threads).
- If a task conflicts with the code, or a Polars 0.43.1 API differs from what is named here, do the closest correct thing and record it as a deviation. Check the API in `~/.cargo/registry` source rather than guessing.
- The regulatory rates in T7 come from the owner's review notes and are marked for verification. Encode them exactly as given, as data. Don't add, change or "correct" rates; raise doubts as questions for the owner.
</constraints>

<workflow>
- Use `grep`/`rg` to find code sites. Don't read `main.rs` (3,455 lines) whole; read only the ranges you need.
- The first build compiles Polars and takes 10–20 minutes. Build once, then rely on incremental builds. Never run `cargo clean`. Use `cargo check` while editing and `cargo test --test review` for the task's tests; run the full `cargo test` before each commit.
- One commit per task, message `Phase 1 T<n>: <short summary>`. Push the branch after each commit so progress isn't lost.
- If the same test fails three times for the same reason, stop and report the blocker instead of trying more variations.
- Don't paste large file contents or full diffs into the conversation; the diff is visible in the Changes view.
</workflow>

<report>
At the end of the session, reply with no more than 30 lines:
- A table: Task | Status (DONE / DONE with deviations / BLOCKED) | Tests added | Deviations (one line).
- The final `cargo test` summary line.
- Questions for the owner, if any (especially regulatory rates or CBS code values).
</report>
