# MicroParquet — Phase 2 specification (data integrity)

Source: code and calculation review of v1.1.1 (16 Nov 2026), Phase 1 results, and the owner decisions recorded in `docs/phase1-spec.md` `<phase2_decisions>`. Sessions are told which task IDs to do; read only those task sections plus `<constraints>`, `<workflow>` and `<report>`.

<goal>
Phase 2 is done when:
1. Every task P1–P15 below is implemented.
2. `cargo build --release` and `cargo test` pass, and the six Phase 2 tests in `tests/review.rs` (t04, t11, t12, t13, t16, t17) are no longer `#[ignore]` and pass.
3. Every acceptance check below exists as an automated assertion.
4. Macro, preset, schema and rule files saved by Phase 1 builds still load.
The principle is the same as Phase 1: no value is lost, guessed or defaulted without the user seeing it.
</goal>

<context>
- Phase 1 is merged. `src/lib.rs` exposes the modules; tests live in `tests/review.rs`.
- Ingestion lives in `engine.rs` (`auto_detect_smart_schema`, `infer_safe_column_type`, `build_lazy_pipeline`, `compile_tab_lazy`); types in `models.rs` (`IngestionConfig`, `ColumnSchemaConfig`, `PiiMaskMode`, `TransformStep`); exports in `exporters.rs`; batch in `batch.rs`; UI in `main.rs`.
- Known current behaviour (baseline): Dr/Cr suffixes, currency prefixes and "-" parse to null silently (t04); a column that is blank in every sampled row is typed Utf8, so an all-blank Closure_Date makes the hygiene audit fail; only comma-separated UTF-8 files load; Parquet sources skip the schema (types and PII masks); stacking fails when columns differ (t16); a duplicated right-side join key doubles balances silently (t17); FTP takes the next tenor point up (t11); reconcile averages rates without weights (t12); the CLIENT_ hash has 32 bits and the salt is a constant in a public repository (t13); the hygiene audit only recognises the exact status "ACT" (t19); banking steps treat a blank balance as 0 without saying so.
</context>

<tasks>

<group name="A: ingestion" session="1">

<task id="P1" name="Number parsing: Dr/Cr, currency, trailing minus, dash">
- Add `#[serde(default)] sign_convention: SignConvention` to `ColumnSchemaConfig`, with variants `Unset` (default), `DrPositive`, `CrPositive`. Show it as a dropdown for Float64 columns in the schema modal.
- Float64 parsing order, applied to the trimmed text: strip a currency prefix (`BDT`, `TK`, `Tk.`, `৳`, case-insensitive, optional space); strip thousands commas; brackets `(x)` → −x; trailing minus `x-` → −x; a Dr/Cr suffix or prefix (`Dr`, `Dr.`, `CR`, case-insensitive, optional space) applies the column's sign convention; a lone `-` → 0; blank, `NA`, `N/A`, `NULL` → null (not a parse failure).
- If any value in a Float64 column carries Dr/Cr and the convention is `Unset`, loading fails with: "Column '<name>' has Dr/Cr amounts. Choose 'Dr is +' or 'Cr is +' for it in Schema & Ingestion." Detect this with a one-column scan, not by sampling.
- Mark Dr/Cr presence during auto-detect so the modal highlights the column.
- Acceptance: un-ignore t04 and set `CrPositive` on Balance → values [1234.5, −500, 1234.5, −250, 0, 99.5, 10], sum 1,828.50, 0 nulls. Same file with `Unset` → error naming Balance. `DrPositive` → [1234.5, −500, −1234.5, 250, 0, 99.5, 10]. `"500-"` → −500. `"Tk. 1,000"` → 1000.
</task>

<task id="P2" name="Parse-failure counts and export guard">
- After a load, count per typed column (Float64, Int64, dates): non-blank raw values that became null. Keep up to 5 example raw values per column.
- Show the counts in the schema modal and in the status bar ("Parse failures: Balance 3, Matdat 1"). Zero failures shows nothing.
- Exports (Parquet, CSV, Excel, split, multi-tab) stop with a confirmation listing the failures when the control column has any. The user can cancel or "Export anyway"; "Export anyway" writes a `<output>.parse_failures.csv` next to the export (column, count, examples).
- Headless mode: failures in the control column make the run exit non-zero unless `--allow-parse-failures` is passed; the counts are printed to stderr either way.
- Acceptance: a file with Balance values ["100", "abc", "200", ""] → 1 failure for Balance with example "abc" (the blank is not a failure). Headless run on it exits non-zero; with the flag exits 0.
</task>

<task id="P3" name="Date formats and blank date columns">
- Add parsing for `%d-%b-%Y` and `%d-%b-%y` (month names case-insensitive: 31-OCT-2026, 31-Oct-26), `%d.%m.%Y`, and two-digit years in D/M/Y and M/D/Y. Two-digit years: 00–69 → 2000–2069, 70–99 → 1970–1999. The four-digit pattern must not swallow a two-digit year (today "5/11/26" parses to null).
- Inference: a column blank in every sampled row is typed DateDMY when its name tokens include date, dt, dat, maturity, expiry, closure, close or open (reuse the Phase 1 tokeniser); otherwise Utf8.
- Acceptance: "5/11/26" → 2026-11-05; "31-OCT-2026" and "31-Oct-26" → 2026-10-31; "05.11.2026" → 2026-11-05; "01/01/75" → 1975-01-01. An all-blank `Closure_Date` column → DateDMY, and the hygiene audit runs on a file whose Closure_Date is blank in every row.
</task>

<task id="P4" name="CSV reading options">
- Add to `IngestionConfig` with `#[serde(default)]`: `separator` (default ','), `skip_rows` (default 0; title lines above the header), `encoding` (`Utf8` default, `Windows1252`).
- Auto-detect the separator from the header line among `,` `|` `;` tab (most frequent, outside quotes); show all three settings in the schema modal.
- Windows-1252: transcode to a temp UTF-8 file before reading (no new crate: bytes 0x00–0x7F and 0xA0–0xFF map to the same code point; 0x80–0x9F use the standard Windows-1252 table). Strip a UTF-8 BOM from the first header name in all cases.
- The line healer must respect the configured separator.
- Acceptance: a pipe-delimited file loads with the right columns; a file with 3 title lines and `skip_rows = 3` loads; a Windows-1252 file with "Café" (0xE9) and "€" (0x80) reads correctly; a BOM-prefixed header gives the column name without the BOM.
</task>

<task id="P5" name="Schema applies to Parquet sources">
- When a tab's source is Parquet, the schema modal lists its columns with their Parquet types. String columns can be set to Float64, Int64 or a date type (parsed with the P1/P3 rules); PII masks apply to any String column.
- Columns already numeric or date in the Parquet file keep their type unless the user changes it.
- P2 parse-failure counts cover Parquet string conversions too.
- Acceptance: a Parquet file with Balance as String ["1,000", "(50)"] and Matdat as String ["05/11/2026"] → after setting Float64 and DateDMY, Balance = [1000, −50] and Matdat = 2026-11-05; an AccountNumber mask on a Parquet String column masks it.
</task>

</group>

<group name="B: combine, export, batch" session="2">

<task id="P6" name="Join validation">
- Before a join, check the right side for duplicate key combinations. If any: error "Right table has <n> duplicated keys on <cols>, e.g. <up to 10>. Fix the mapping table, or switch on 'Keep first row per key' for this join."
- Add `#[serde(default)] keep_first_duplicate_right: bool` to the join source/step. When on, keep the first row per key and put a status-bar warning with the count.
- After any join, the status bar shows rows and control sum against the left table's.
- Acceptance: un-ignore t17 → error mentioning Actnum and A1; with the toggle on → 1 row, sum 1,000,000, a warning with count 1.
</task>

<task id="P7" name="Stacking files whose columns differ">
- Stack with diagonal concatenation (`concat_lf_diagonal`): a column missing from a file is null for that file's rows.
- Add an option to apply one schema to every file in the stack (the first file's schema, or a loaded `.schema.json`). When files infer different types for the same column and no shared schema is chosen, error naming the column, the files and the two types.
- Acceptance: un-ignore t16 → September + October stack gives 2 rows, Branch null for September, Source_File set; a stack where Balance is Float64 in one file and Utf8 in another errors naming Balance, unless the shared schema is applied, which then loads both.
</task>

<task id="P8" name="Split export keeps every row">
- Rows with a null or blank split key go to `BLANK_KEY.<ext>`.
- File-name collisions after sanitising, compared case-insensitively, get `_2`, `_3` … suffixes.
- Write `split_manifest.csv` (key, file, rows, control sum). The sum of rows across files must equal the source row count; otherwise error.
- Acceptance: keys ["DHK", "dhk", null, "A/B", "A_B"] → 5 files with no overwrite, a manifest with 5 rows, and total rows equal to the source.
</task>

<task id="P9" name="Excel, PDF and multi-tab exports never drop data silently">
- Excel: write Date columns as real Excel dates (format dd-mmm-yyyy) and Datetime as date-time. Above 1,048,575 data rows, continue on new sheets named `<sheet> (2)`, `(3)` … instead of truncating; status message says how many sheets.
- Multi-tab Excel: a tab that fails adds a row to an `Export_Errors` sheet (tab name, error) and the status message names it; no silent skip.
- PDF: right-align numbers. With more columns than fit the page, include the columns that fit and print "Columns <list> omitted — use Excel export" on each page; the status message says so.
- Acceptance: a test writing an xlsx with a Date column reads the cell back as a date serial with a date format (use `rust_xlsxwriter` test helpers or unzip and check the sheet XML); a multi-tab export with one failing tab produces `Export_Errors` with that tab's name.
</task>

<task id="P10" name="Batch conversion">
- Add a file picker for the `.schema.json` template in the batch modal; when the mode is "Apply Saved Template" and none is chosen, the Run button is disabled with a hint.
- Show the error list in the modal after a run.
- Write `batch_log.csv` in the output folder: file, rows, control sum, status, error.
- Refuse an output folder inside the input folder (it would be re-read during the walk).
- Acceptance: a batch over a folder with one good and one malformed CSV writes a log with one OK row and one error row; output inside input is refused with a clear message.
</task>

<task id="P11" name="Status bar honesty">
- When control totals can't be computed (for example the control column no longer exists after a pivot), show "—" for rows and sum and the reason, instead of keeping the previous step's values.
- Math column: a scalar that doesn't parse is an error naming the value, not 1.0.
- Acceptance: unit tests for the control-total state function (pull it out of the UI code if needed) and for the math scalar error.
</task>

</group>

<group name="C: calculations and security" session="3">

<task id="P12" name="PII masking with a per-install secret key">
- On first use, create `portable_data/pii_key.bin` with 32 random bytes (add `getrandom = "0.2"` as a direct dependency; it is already in Cargo.lock). Never commit it: add `portable_data/` to `.gitignore`.
- Masks use `blake3::keyed_hash` with that key. ClientNameHash → `CLIENT_` + 16 hex characters. AccountNumber → `ACCT_` + 16 hex characters (no digits of the original kept).
- The old `pii_salt` field stays readable for old schema files but is no longer used; the schema modal says masks now use the install key.
- Add Export key / Import key buttons in the schema modal, with a warning that masks from different keys don't match.
- Acceptance: un-ignore t13 → 1,000,000 names give 1,000,000 distinct tokens; the same input with the same key gives the same token; two different keys give different tokens; tokens contain no run of 3+ digits from the original account number.
</task>

<task id="P13" name="FTP curve interpolation">
- Add `#[serde(default)] interpolation: FtpInterpolation` to the FTP step, with values `Linear` (default) and `NextPointUp`. Linear on days between the two surrounding tenor points; below the first point use the first rate; beyond the last use the last.
- Add an `FTP_Method` output column. Show the setting in the FTP modal.
- Acceptance: un-ignore t11 → 100-day deal 9.559 (±0.001), 31-day deal 8.762 (±0.001); `NextPointUp` gives 10.10 and 9.50; a 10-day deal gives the 30-day rate; a 20-year deal gives the last point.
</task>

<task id="P14" name="Period reconcile">
- Rates per key and period are balance-weighted (null-safe, as in Phase 1 T3).
- `Rate_Delta_Bps` is null when the key is missing in either period.
- Add `Prev_Row_Count` and `Curr_Row_Count`; the status message reports how many keys have more than one row.
- CL status per key: the worst grade by the CL map order STD < SMA < SS < DF < BL; codes not in `cl_code_map.json` → the code of the largest-balance row.
- Acceptance: un-ignore t12 → A1 previous rate 9.5, delta 0 bps; A2 (closed) and A3 (new) delta null; A1 Prev_Row_Count 2.
</task>

<task id="P15" name="Hygiene audit and blank balances">
- Active statuses come from a setting in the audit modal (default ACT, ACTIVE, A, OPEN), matched trimmed and case-insensitive. The rate outlier bounds become settings (default 0 to 25).
- Keep `Audit_Exception_Flag` (first flag, unchanged) and add `Audit_All_Flags` listing every flag that applies, separated by "; ".
- Provisioning, limit utilisation, ΔNII and duration: count rows whose balance is blank and report it in the status message ("Balance blank on N rows; treated as 0"). Do not change the treatment.
- Acceptance: t19 → "Active" with a past maturity and positive balance is flagged; a row that is both matured-active and a rate outlier lists both in Audit_All_Flags; a provisioning run with 2 blank balances reports 2.
</task>

</group>

</tasks>

<accepted_deviations>
Recorded during implementation of P1–P5 (session 1). Items marked "owner accepted" were confirmed by the owner.
- P1: a Dr/Cr marker on a value that is already negative ("(500) Dr", "-500 Cr") is a parse failure (null, counted by P2), not a guessed sign. Owner accepted.
- P1: with "Parse Accounting Negatives" off, brackets and a trailing minus are not negatives (the value is a parse failure); thousands commas are always stripped.
- P1: Int64 columns use the same amount rules; a decimal point or a Dr/Cr marker without a sign convention makes the value a parse failure (the Dr/Cr load error applies to Float64 columns only, as specified).
- P2: for split exports the failure list is `<output folder>/split_export.parse_failures.csv`. Owner accepted.
- P2: the headless acceptance test takes the schema from a macro, because auto-detect types the sample Balance column ["100", "abc", ...] as text.
- P2: date blanks that are not failures: blank, "-", "0", NULL, NONE, NA, N/A.
- P3: the Rust parser already read "5/11/26"; the four-digit-year problem was in the Python export (fixed `%d/%m/%Y`). Python now uses `_mp_date`, a mirror of the app's parser.
- P5: opening a Parquet file still opens it directly; its schema is edited with "Schema & Ingestion". Boolean/List columns cannot be converted or masked (error).
- P5: headless runs on a Parquet input apply the macro schema to columns that exist and are String in the file; other existing columns keep their Parquet type; absent columns are skipped with a stderr warning. Owner decision.
Recorded during P6–P15 (sessions 2–3, unattended run; not yet confirmed by the owner):
- P6: `TabSource` is not serialised (joins are not saved in macros), so `keep_first_duplicate_right` is a plain field without `#[serde(default)]`. The toggle is the "Keep first row per key" checkbox in the Join dialog. Owner decision: when joins become saveable, persist it with `#[serde(default)]` (false).
- P6: rows whose key has a null in any key column are not counted as duplicates (they never match) and are kept as they are by "Keep first row per key". Semi and anti joins skip the check (they never repeat a left row).
- P6: <n> in the error is the number of distinct key combinations that repeat; the status bar shows that number and the number of right rows dropped. The left-side control sum uses the joined tab's control column when the left table has it, otherwise "—".
- P7: the `diagonal_concat` Polars feature is not enabled and enabling it would change Cargo.toml, so the diagonal stack is built by hand: each file is selected onto the union of columns (first-appearance order, `Source_File` last) with missing columns as typed nulls, then concatenated. Same result as `concat_lf_diagonal`.
- P7: "Apply one schema to every file" keeps each CSV's own detected separator, title-line count and encoding (only column settings and the other flags come from the shared schema); a file's columns that the schema does not name keep their detected type. Files lacking a schema column get a status-bar note. Type check: any dtype difference is an error except a column that is all-null (Null type) in one file.
- P7: a CSV whose schema detection fails is now an error naming the file (it used to stack with an empty schema).
- P8: `split_and_export_by_column` gains a `control_col` argument (the tab's control column). Null keys and keys that are only spaces both go to `BLANK_KEY.<ext>` (no file when there are none; its manifest key is empty). Besides the row check, the files' control sum must equal the source's, otherwise error. The manifest is written before the checks so a failed split can be inspected.
- P9: `export_tabs_to_xlsx` takes the failed tabs as a second argument and returns a report (sheets per tab, errors); `write_xlsx_workbook` is the same with the rows-per-sheet limit as a parameter, for tests. A cell that cannot be read is now an error (it used to be left empty), and a workbook with no tabs and no errors is an error. The multi-tab Excel export no longer caps tabs at 1,048,500 rows.
- P9: the PDF also had a silent 5,000-row cap per tab; it is kept, but each page of such a tab says "Only the first 5000 of <n> rows are printed — use Excel export" and so does the status message. Multi-tab PDF: failed tabs are named in the status message (there is no error page in the PDF). Number right-alignment uses Helvetica character widths (AFM), so it is exact for digits, separators and signs; at most 15 columns (18 mm each) fit a landscape A4 page.
- P9 (question): Int64 values beyond 15 significant digits are still written to Excel as numbers and lose digits; ID columns are text by default, so this only affects numeric columns with very large values.
- P10: `batch_log.csv` also lists copied companion files (status COPIED) and has a `parse_failures` column (values that became null, CSV -> Parquet). Rows and control sum: CSV -> Parquet from the written Parquet file, Parquet -> CSV from the source. The control column is a numeric column named Balance, else the first numeric column whose name contains balance or amount; empty when there is none.
- P10: "Apply Saved .schema.json Template" without a template is now an error in `run_folder_mirror_batch` too (it used to fall back to auto-detect silently). The folder check resolves `..` and symlinks and also refuses output = input. An input file named `batch_log.csv` at the top level is not copied over the log (ERROR row).
- P10: "malformed CSV" in the test is an empty file (no header row): Polars reads many broken CSVs without an error (an unclosed quote swallows the following lines, binary junk becomes a header with 0 rows). Those are not detected by the batch run.
- P11: when only the control column is missing (e.g. after a pivot) the row count is still shown and only the sum is "—" with the reason; when the pipeline itself fails both are "—". A control column that is not numeric now has no sum ("—", reason shown); it used to be cast to numbers and summed. State function: `control_totals_state` (engine).
- P11: the math scalar accepts thousands commas ("10,000,000"); NaN and infinity are errors. The Python export writes the parsed number, or `raise ValueError(<same message>)` for a value that is not a number.
- P12: hex can repeat 3 digits of an account number by chance ("ACCT_2A7C2AB96375C989" for 1234567237570), so the account mask takes the next round of the keyed hash (key, value, 0x00, round) until the 16 hex characters contain no 3-digit run of the original. Same value + same key still gives the same token.
- P12: design in `docs/phase2-progress.md`. Additions to the spec: the schema modal shows a key fingerprint (first 8 hex of blake3 of the key); Import key backs up the current key as `pii_key.bin.bak-<unix seconds>`; a damaged key file is an error and is never replaced automatically; a newly created key is announced in the status bar / on stderr. The Python export still does not reproduce masking (blake3 is not in the Python standard library); the script now also prints a warning when it runs.
- P12 (question): tokens are 64-bit keyed hashes; anyone holding the exported key file can test guesses (e.g. all account numbers of a branch) against the masks. The key file must be handled like the data.
- P13: the acceptance line "a 10-day deal gives the 30-day rate" assumes a curve that starts at 30 days; the default Bangladesh curve has 1-day and 7-day points, so with Linear a 10-day deal gets 8.25 + 3/23 × 0.50 = 8.315. The test checks the 30-day rate on the default curve without its 1d/7d points, and 8.315 on the full default curve.
- P13: `apply_ftp_curve_map` takes the interpolation as a new last argument. An empty curve or two points with the same tenor are errors (an empty curve used to give 0%). `FTP_Method` is per row: "Linear 91d-182d", "Next point up 182d", "First point 30d (at or below curve start)" or "Last point 1825d (beyond curve)".
- P13 (question): as specified, FTP steps in macros saved before Phase 2 load as Linear, so re-running an old macro changes its FTP rates (step → linear). Choose "Next tenor point up" in the FTP modal to reproduce old results.
- P14: the per-key CL output is the original CL code of the chosen row (not the grade name), so single-row keys show the same code as before. The row with the worst mapped grade wins; ties, and keys whose codes are all unmapped, go to the largest-balance row. In a key that mixes mapped and unmapped codes the mapped worst grade wins. The CL map comes from `cl_code_map.json` (loaded only when a CL column is reconciled); `build_period_reconcile_lazy_with` takes the map directly (tests).
- P14: `Prev_Row_Count` / `Curr_Row_Count` are 0 (not null) for a key missing in that period. New/closed status now follows the row counts (same result as before). A key whose rate rows all have zero or missing balance gets a null rate.
- P15: the audit settings are saved in the step (`active_statuses`, `rate_min_pct`, `rate_max_pct`; old macros load with ACT, ACTIVE, A, OPEN and 0–25). An empty status list or a lower bound above the upper bound is an error. `Audit_All_Flags` is "OK" when no flag applies.
- P15: the blank-balance note names the step, e.g. "+BB CL Provision & BFP: Balance blank on 2 rows; treated as 0". It is shown when a step is added, when a tab (or macro) is opened, and on stderr in headless runs. "Blank" means null after conversion to a number, so a text value that is not a number also counts (those are already listed as parse failures).
- P15 (question): old macros' audit steps now treat ACTIVE, A and OPEN as active (as specified), so re-running them can flag more rows than before.
</accepted_deviations>

<constraints>
- Do not upgrade Polars, egui or any other dependency. The only new direct dependency allowed is `getrandom = "0.2"` (P12).
- Every new field on a serialised struct or enum variant gets `#[serde(default)]`; Phase 1 macro, schema, preset and rule files must still load. Add a test that loads a Phase 1 schema JSON from a string literal.
- Keep public function names. Extend signatures where a task needs it and update every call site, including headless mode and `macros.rs` (Python export must stay in step for P1/P3 parsing rules; steps it cannot translate keep raising NotImplementedError).
- No `unwrap()`, `expect()` or index-panics on user data paths. Errors are plain messages a banker can act on.
- Change only what the tasks need. Don't reformat untouched files. Don't start Phase 3 work (streaming exports, background threads, caching).
- If the spec conflicts with the code, or a Polars 0.43.1 API differs, do the closest correct thing and record it as a deviation. Check APIs in `~/.cargo/registry` source rather than guessing.
- Test data is synthetic only. Never add real customer data to the repository.
</constraints>

<workflow>
- Use `grep`/`rg` to find code sites; read only the ranges of `main.rs` you need.
- Never run `cargo clean`. Use `cargo check` while editing and `cargo test --test review` for the task's tests; run the full `cargo test` before each commit.
- One commit per task, message `Phase 2 P<n>: <short summary>`. Push after each commit.
- If the same test fails three times for the same reason, stop and report the blocker.
- Don't paste large file contents or diffs into the conversation.
</workflow>

<report>
At the end of the session, reply in no more than 30 lines:
- A table: Task | Status (DONE / DONE with deviations / BLOCKED) | Tests added | Deviations (one line).
- The final `cargo test` summary line.
- Questions for the owner, if any.
</report>
