# Phase 3 progress

Branch: `phase3` (from `main`). Run unattended; resume from the first task that is not DONE.
Each task is one commit "Phase 3 <ID>: ..."; the commit hash is filled in by the next commit.
Test baseline before R1: `cargo test` = 2 (main.rs unit) + 60 (tests/review.rs) passed.

| Task | Status | Commit | Tests added | Deviations | Open questions |
|---|---|---|---|---|---|
| R1 | DONE | d442fdc | none (2 unit tests moved with their functions: tab_index_after_close, default_side_column) | headless in src/headless.rs; no font loading yet; width literals stay; prelude module; derived dark tokens | Q1 |
| R2 | DONE | (next commit) | tests/jobs.rs: 15 tests (superseded dropped; replaced queued job never runs; unique/other-tab jobs kept; cancel removes temp file; success renames; failed write leaves nothing; error and panic reach status text; inline runs on caller thread; wake callback; load/refresh numbers; unreadable source; cancelled export, split and batch) | see spec `<accepted_deviations>` (R2): previews stay sync; coarse cancel points; GUI not runnable here | Q2 |
| R3 | TODO | | | | |
| L1 | TODO | | | | |
| L2 | TODO | | | | |
| L3 | TODO | | | | |
| L4 | TODO | | | | |
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

## Notes
- R1 check: a line-by-line comparison of the old and new code (after the field renames) shows only structural changes, the 4 colour constants and the theme call.
- `grep -nE '#[0-9A-Fa-f]{6}|Color32::from_rgb' src/ui src/main.rs` matches only `src/ui/theme.rs`. `main.rs` is 521 lines.
