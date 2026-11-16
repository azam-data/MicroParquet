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
| P10 | DONE | (this commit) | t27 (good + malformed → log with 1 OK, 1 ERROR row; output inside input refused, incl. ..; template mode without template refused; Parquet→CSV with COPIED companion) | log has COPIED rows and parse_failures column; template fallback removed | Q3 |
| P11 | todo | | | | |
| P12 | todo | | | | |
| P13 | todo | | | | |
| P14 | todo | | | | |
| P15 | todo | | | | |

## Questions for the owner
(Each one is also recorded in `<accepted_deviations>` in `docs/phase2-spec.md`.)
- Q1 (P6): joins are not saved in macros (`TabSource` is not serialised); should "Keep first row per key" also be saved once joins become part of a macro?
- Q2 (P9): Int64 values with more than 15 significant digits lose digits in Excel (Excel numbers are doubles). Write such values as text, or error?
- Q3 (P10): Polars reads some broken CSVs without an error (an unclosed quote swallows later lines into one field; binary junk becomes a header with 0 rows). Should the batch run flag files whose row count looks wrong (e.g. 0 rows, or far fewer rows than lines)?

## Notes
