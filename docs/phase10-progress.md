# Phase 10 progress

Branch `phase10` (draft PR to `main`). Version stays 1.9.0; no tag; never merge (owner merges after his own test).
Spec: `docs/phase10-spec.md`. Decisions: `docs/phase10-spec.md` <accepted_deviations> (D0-1 … D0-4 so far).

| Session | Tasks | Status |
|---|---|---|
| S1 | D0, C0 | D0 done, C0 in progress |
| S2–S12 | D1 … fixes | not started |

## S1 acceptance → test (tests/phase10/d0.rs)
| Acceptance | Test |
|---|---|
| Phase 9 samples replay to the Phase 9 hashes | `d0_the_phase9_samples_replay_to_the_hashes_the_phase9_code_gave` |
| No dashboard saves byte for byte as Phase 9 | `d0_a_project_without_a_dashboard_saves_byte_for_byte_as_phase9_saves_it` |
| Dashboard saves as version 3; v1.9.0 refusal | `d0_a_project_with_a_dashboard_saves_as_version_3_and_the_v190_app_refuses_it` |
| 2 pages, 5 items stable | `d0_a_dashboard_of_2_pages_and_5_items_is_stable_over_save_open_save` |
| 3 per row = 326.67 | `d0_a_grid_of_3_per_row_on_a_1000_unit_page_gives_widths_of_326_67` |
| Deleted tab: "Source tab removed" | `d0_an_item_pointing_at_a_deleted_tab_round_trips_and_reports_source_tab_removed` |
| Extra: old file loads; limits refused | `d0_an_old_dashboard_without_new_fields_still_loads`, `d0_too_many_filters_and_duplicate_ids_are_refused_with_a_plain_message` |

Stub proofs: D0 — version forced to 2 → the version-3 test fails (restored).
