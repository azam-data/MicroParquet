# Phase 3 progress

Branch: `phase3` (from `main`). Run unattended; resume from the first task that is not DONE.
Each task is one commit "Phase 3 <ID>: ..."; the commit hash is filled in by the next commit.
Baseline before R1: `cargo test` = 2 (main.rs unit) + 60 (tests/review.rs) passed.

| Task | Status | Commit | Tests added | Deviations | Open questions |
|---|---|---|---|---|---|
| R1 | DONE | (next commit) | none (2 unit tests moved with their functions: tab_index_after_close, default_side_column) | headless in src/headless.rs; no font loading yet; width literals stay; prelude module; derived dark tokens | Q1 |
| R2 | TODO | | | | |
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

## Notes
- R1 check: a line-by-line comparison of the old and new code (after the field renames) shows only structural changes, the 4 colour constants and the theme call.
- `grep -nE '#[0-9A-Fa-f]{6}|Color32::from_rgb' src/ui src/main.rs` matches only `src/ui/theme.rs`. `main.rs` is 521 lines.
