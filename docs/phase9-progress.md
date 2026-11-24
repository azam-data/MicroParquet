# Phase 9 progress (Model)

Branch phase9. Sessions S1–S9 as in the spec. Resume from this file.

| Task | Status | Commit | Tests |
| --- | --- | --- | --- |
| B0 | DONE | (see git log: "Phase 9 B0") | tests/phase9/b0.rs x1 (3 macros, the pack's 2 sheets and 2 charts, the saved chart, the project's 7 tabs and its text after save-open-save replay to the hashes the v1.8.0 code gave) |

## B0 acceptance: item -> test

| Acceptance item (spec B0) | Test |
| --- | --- |
| `b0_the_phase8_samples_replay_to_the_hashes_the_phase8_code_gave` passes with today's code | `tests/phase9/b0.rs` `b0_the_phase8_samples_replay_to_the_hashes_the_phase8_code_gave` (16 hashes: 3 macros, the pack's 2 sheets and 2 charts, the saved chart, 7 project tabs, the project's text after save-open-save) |
| The Phase 7 replay test still passes | `tests/phase8/w0.rs` `w0_the_phase7_samples_replay_to_the_hashes_the_phase7_code_gave` (run: ok) |

Proved to fail without the fix (stubs in `src/models.rs` and `src/project.rs`, restored): `b0_the_phase8_samples_replay_to_the_hashes_the_phase8_code_gave` fails with "macro:phase8_rate_band_unnumbered" and "project:bytes after save-open-save" differing (the unnumbered label turned numbered; `PROJECT_VERSION` 2).

Deviations: B0-1 to B0-4 (in `docs/phase9-spec.md` `<accepted_deviations>`).
