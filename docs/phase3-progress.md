# Phase 3 progress

Branch: `phase3` (from `main`). Run unattended; resume from the first task that is not DONE.
Each task is one commit "Phase 3 <ID>: ..."; the commit hash is filled in by the next commit.
Test baseline before R1: `cargo test` = 2 (main.rs unit) + 60 (tests/review.rs) passed.

| Task | Status | Commit | Tests added | Deviations | Open questions |
|---|---|---|---|---|---|
| R1 | DONE | d442fdc | none (2 unit tests moved with their functions: tab_index_after_close, default_side_column) | headless in src/headless.rs; no font loading yet; width literals stay; prelude module; derived dark tokens | Q1 |
| R2 | DONE | 2700231 | tests/jobs.rs: 15 tests (superseded dropped; replaced queued job never runs; unique/other-tab jobs kept; cancel removes temp file; success renames; failed write leaves nothing; error and panic reach status text; inline runs on caller thread; wake callback; load/refresh numbers; unreadable source; cancelled export, split and batch) | see spec `<accepted_deviations>` (R2): previews stay sync; coarse cancel points; GUI not runnable here | Q2 |
| R3 | DONE | 907bfa0 | tests/r3.rs: 6 tests (Parquet and CSV streamed vs collected: same rows, sum, data and CSV bytes; non-streamable view falls back and status says so; Dr/Cr check + parse failures share one scan and rescan after a file change; Dr/Cr still refuses; baseline computed once for two tabs, recomputed on change; load stores baseline) | see spec (R3): message-based fallback; baseline key includes schema and control column; scan cache | |
| L1 | DONE | 22e8657 | theme tests (23 tokens light = dark counterpart, spec values, sizes, symbol→glyph map, every UI symbol has a glyph in the loaded fonts) | Source Sans 3 TTFs missing (fallback); title strip inside OS window; More menu; dark primary text; see spec | Q3 |
| L2 | DONE | f77ebf6 | tests/l2.rs: 5 tests (widest of header / selector / summary / samples wins; 420 cap and floor; only first 100 samples; "01-Sep-2026" fits on one line at the computed width; "20,391" and "1–100 of 20,391") | name-only headers; selector/summary inputs wired but empty until T3; mono dates/IDs | |
| L3 | DONE | 32a45b7 | tests/l3.rs: 7 tests (25,000,000 → 25,000.00 K / 25.00 M / 250.00 L / 2.50 Cr via column and global scale; negatives and decimals; Crore/Lakh unchanged, rates never scaled; old macro formats load; Excel header and values in scale incl. per-column override; plain scale and old export raw; PDF header rule) | Excel writes scaled values (Excel cannot scale by 1e5/1e7); Lakh already existed | |
| L4 | DONE | (next commit) | tests/l4.rs: 7 tests ("'1,234.50" → 1234.5 and only one apostrophe dropped; Float64 column loads it and a Text column keeps "'0012"; Python mirror; Text column of numbers flagged with 98% and the exact question; 95% threshold and blanks; ID columns excluded; Set to Number step converts, errors name a missing column, JSON round trip) | Set to Number is a step, not a schema edit; check runs before GUI exports only | |
| L5 | TODO | | | | |
| T1 | TODO | | | | |
| T2 | TODO | | | | |
| T3 | TODO | | | | |
| T4 | TODO | | | | |
| T5 | TODO | | | | |
| T6 | TODO | | | | |

## Questions for the owner
- Q1 (R1): please confirm the derived dark-theme values for tokens the spec does not give (see `<accepted_deviations>`).

- Q2 (R2): the manual freeze check (open a large file, use the menus while it loads) and a look at the status-bar Cancel button and the dimmed "Updating…" table need your eyes; this container has no display libraries.

- Q3 (L1): please add `SourceSans3-Regular.ttf`, `-Semibold.ttf`, `-Bold.ttf` to `assets/fonts/` (my session could not fetch them) and look at the whole layout against `docs/design/Combined.dc.html`.

## Notes
- Disk: the test binaries are ~1.5 GB each (debug info). If a build fails with "No space left on device", delete executables in `target/debug/deps` (not rlibs) and `target/debug/incremental`; never `cargo clean`.
- R1 check: a line-by-line comparison of the old and new code (after the field renames) shows only structural changes, the 4 colour constants and the theme call.
- `grep -nE '#[0-9A-Fa-f]{6}|Color32::from_rgb' src/ui src/main.rs` matches only `src/ui/theme.rs`. `main.rs` is 521 lines.
