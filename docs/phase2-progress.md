# Phase 2 progress (session 2–3: P6–P15)

Branch: `phase2-rest`. Resume from the first task that is not DONE. Each task is one commit
("Phase 2 P<n>: ..."); the commit hash is filled in by the following commit (a commit cannot
contain its own hash).

| Task | Status | Commit | Tests added | Deviations | Open questions |
|---|---|---|---|---|---|
| P6 | DONE | 8a87d0c | t17 un-ignored and extended (error names Actnum/A1; semi join; keep-first → 1 row, sum 1,000,000, warning count 1; null keys) | TabSource not serialised, so no serde attribute; null keys and semi/anti joins exempt | Q1 |
| P7 | DONE | f474139 | t16 un-ignored (+Source_File, column order); t16b (type mismatch error names Balance/files/types; first-file schema loads both with 1 parse failure; saved schema with missing column; CSV+Parquet; unreadable file) | manual diagonal concat (feature not enabled); per-file read settings kept |  |
| P8 | DONE | 5003bad | t25 (DHK/dhk/null/A\/B/A_B/blank → 5 files, no overwrite, 5-row manifest, rows = source; Parquet) | control_col argument; control-sum check added |  |
| P9 | DONE | 8b6c8e1 | t26 (xlsx Date cell = serial + dd-mmm-yyyy style, Datetime with time; spill to '(2)','(3)'; Export_Errors names failed tab), t26b (PDF omitted columns list, 5,000-row note) | PDF row cap made visible; cell read errors fail; report structs | Q2 |
| P10 | DONE | 69fab1c | t27 (good + malformed → log with 1 OK, 1 ERROR row; output inside input refused, incl. ..; template mode without template refused; Parquet→CSV with COPIED companion) | log has COPIED rows and parse_failures column; template fallback removed | Q3 |
| P11 | DONE | 67e8fcc | t28 (control_totals_state: ok / pivot removes control column / text column / failed pipeline; parse_math_scalar errors name the value; Python export) | rows kept when only the control column is missing; text control column has no sum |  |
| P12 | DONE | 454184a | t13 un-ignored (1,000,000 distinct CLIENT_ tokens; same key same token; different keys differ; no 3-digit run of 2,000 account numbers); t29 (create, reuse, 0600, concurrent first use, export/import with backup, junk import refused, damaged key error); t24 updated for ACCT_ format | account mask re-derives to avoid digit runs; fingerprint, backup on import | Q4 |
| P13 | DONE | 45e7728 | t11 un-ignored (100d 9.559, 31d 8.762; NextPointUp 10.10/9.50; 10d → 30d rate on a curve starting at 30d; 20y → last point; FTP_Method; unsorted curve; empty/duplicate curve errors; old macro loads as NextPointUp) | 10-day case needs a curve starting at 30d (default curve has 1d/7d points) | Q5 |
| P14 | DONE | cf15d2b | t12 un-ignored (A1 prev rate 9.5, delta 0 bps; A2/A3 delta null; A1 Prev_Row_Count 2; multi-row key count; null-safe weights; CL worst grade / unmapped → largest balance) | CL output is the chosen row's code; row counts 0 for missing keys |  |
| P15 | DONE | 3c88622 | t19 (Active/' open ' flagged; matured-active + outlier both in Audit_All_Flags; settings; bad settings error; old macro defaults); t19b (provisioning with 2 blank balances reports 2) | settings stored in the step; note names the step | Q6 |

## P12 design: per-install PII key

- **File:** `portable_data/pii_key.bin` next to the executable (`portable_root_dir()`), exactly 32 bytes.
  `portable_data/` is added to `.gitignore`.
- **Created:** on first use, when a pipeline masks a column (or the schema modal shows the key, or
  Export key is pressed) and no key file exists. Every path announces it: `ensure_pii_key_for`
  returns the notice (single load: status bar; headless: stderr; batch: a `NOTICE` line in
  `batch_log.csv`, "New masking key created — export a backup", and a line in the batch modal); a
  key created by a pipeline build that did not announce it is picked up by the UI loop and shown
  in the status bar. 32 bytes come from `getrandom` (OS randomness). The
  bytes go to a temporary file in the same folder, which is then hard-linked to `pii_key.bin`. A hard
  link never replaces an existing file, so two processes creating the key at the same time end up with
  the same key: the loser reads the winner's file. On Unix the file is created with mode 0600.
  The GUI says when a key was created ("Created a new PII key … Export it to keep masks
  reproducible"); headless runs print the same on stderr.
- **Used:** `blake3::keyed_hash(key, trimmed value)`. ClientNameHash → `CLIENT_` + the first 16 hex
  characters (upper case); AccountNumber → `ACCT_` + 16 hex characters (no character of the original
  is kept). Blank values stay blank. The key is read from disk each time a pipeline with a masked
  column is built (32 bytes), so an imported key takes effect on the next refresh without a restart.
  The old `pii_salt` field stays in `IngestionConfig` (old schema files still load) but is not used.
- **Fingerprint:** the schema modal shows the key file path and a fingerprint (first 8 hex characters
  of blake3 of the key), so two installs can check that they share a key without showing the key.
- **Export key:** copies the 32 bytes to a file the user picks (default name `pii_key.bin`); on Unix
  the file is written with mode 0600 (also when it already existed). The UI
  warns that the file must be kept as safe as the data (anyone with it can test guesses against masks).
- **Import key:** the user picks a file; it must be exactly 32 bytes, otherwise an error and nothing
  changes. The current key is first copied to `pii_key.bin.bak-<unix seconds>` (created with
  `create_new`; if that name exists the backup is `..._2`, `..._3` ...; a backup is never
  overwritten), then the new key is
  written to a temporary file and renamed over `pii_key.bin`. The modal warns that masks made with
  different keys don't match.
- **Missing key file:** treated as first use: a new key is created and the GUI/stderr says so. Masks
  made before with another key will not match; to keep them matching, import the exported key.
- **Corrupt key file** (not 32 bytes, unreadable): every pipeline that masks a column fails with
  "The PII key file <path> is damaged (<n> bytes, expected 32). Import your exported key in Schema &
  Ingestion, or delete the file to create a new key (masks made with a new key will not match earlier
  ones)." The key is never replaced automatically. Pipelines without masked columns are unaffected.
- **Python export:** blake3 is not in the Python standard library, so masking is still not reproduced;
  the script keeps the warning comment and also prints a warning when it runs.

## Questions for the owner
(Each one is also recorded in `<accepted_deviations>` in `docs/phase2-spec.md`.)
- Q1 (P6): joins are not saved in macros (`TabSource` is not serialised); should "Keep first row per key" also be saved once joins become part of a macro? **Answered:** yes, with `#[serde(default)]` false, when joins become saveable (recorded only).
- Q2 (P9): Int64 values with more than 15 significant digits lose digits in Excel (Excel numbers are doubles). Write such values as text, or error? **Answered:** write them as text, with a status warning naming the column (done).
- Q3 (P10): Polars reads some broken CSVs without an error (an unclosed quote swallows later lines into one field; binary junk becomes a header with 0 rows). Should the batch run flag files whose row count looks wrong (e.g. 0 rows, or far fewer rows than lines)? **Answered:** warn (not error) on single and batch loads, both counts shown (done).
- Q4 (P12): the Python export still keeps the original values in masked columns (blake3 is not in the Python standard library). Generate code that needs `pip install blake3` and reads the key file, or make masked columns stop the Python export? **Answered:** refuse the export with a plain message (done).
- Q5 (P13): old macros' FTP steps would load as Linear (the spec's default), so their FTP rates would change on re-run. Keep that, or load old steps as NextPointUp to reproduce earlier ALCO numbers? **Answered:** old steps load as NextPointUp; new UI steps default to Linear (done).
- Q6 (P15): old macros' hygiene audits now treat ACTIVE, A and OPEN as active (the spec's default), so they can flag more rows on re-run. Keep? **Answered:** keep the new defaults (recorded).

## Notes
- All ten tasks DONE; none BLOCKED. `tests/review.rs` has no `#[ignore]` left (t04, t11, t12, t13, t16, t17 run).
- Final `cargo test` (review): 60 passed; 0 failed; 0 ignored (after the PR #4 review fixes). Bin tests: 2 passed.
- Owner answers Q1–Q6 applied (one commit each); see `<accepted_deviations>`.
- PR #4 review fixes (one commit each, "Review fix N"): 1 key-creation notice on every path,
  2 backups never overwritten, 3 key export 0600, 4 mixed-sign weighted rates, 5 FTP missing dates,
  6 missing assertions (P6, P9, P12, P14, P15), 7 these docs. Deviations 4 and 5 are in
  `<accepted_deviations>` of the spec ("Recorded after the PR #4 review").
- Next step: the owner merges PR #4 (`phase2-rest` → `main`).
