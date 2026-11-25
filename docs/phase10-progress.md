# Phase 10 progress

Branch `phase10` (draft PR to `main`). Version stays 1.9.0; no tag; never merge (owner merges after his own test).
Spec: `docs/phase10-spec.md`. Decisions: `docs/phase10-spec.md` <accepted_deviations> (D0-1 … D0-4 so far).

| Session | Tasks | Status |
|---|---|---|
| S1 | D0, C0 | D0 done, C0 done |
| S2 | D1 | done |
| S3 | D2 | done |
| S4–S12 | D3 … fixes | not started |

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

## C0 (tests/phase10/c0.rs)
| Acceptance | Test |
|---|---|
| Rows through 29 Oct: YTD income Oct unchanged (30) and note names "29 Oct" and "Nov 2026" | `c0_a_ytd_income_month_whose_last_day_has_no_rows_carries_the_note_and_the_same_number` |
| Month whose last day has rows: no note | `c0_a_month_whose_last_day_has_rows_has_no_note` |
| Flows: no note | `c0_a_flow_gets_no_note_even_when_the_last_day_has_no_rows` |

The note is in `PivotBuild::warnings` (the pivot's notes; the chart's footnote copies them in `charts::calc`). Applies to Period amount and annualised ratios, months only, finished months only. The dashboard item tooltip (D1) reads the same chart notes.
Stub proof: C0 — note function forced to `None` → the note test fails (restored).
Faults listed by the owner: none.
Full test: 821 + 351 + 9 passed; two older tests used version 3 as "a newer project" and now use 4 (D0 makes 3 known), re-run green.

## S2 acceptance → test (D1)
| Acceptance | Test |
|---|---|
| Item numbers equal the chart tab's / pivot tab's own, on every fixture | `d1_a_dashboard_item_has_the_numbers_of_the_chart_tab_and_the_pivot_tab_on_every_fixture` (tests/phase10/d1.rs); `d1_items_are_added_by_clicks_draw_the_tabs_own_numbers_and_fit_at_three_sizes` |
| Lookup-field filter filters a linked item; unlinked says "filter doesn't apply" | `d1_a_filter_on_a_lookup_field_filters_a_linked_item_and_an_unlinked_item_is_told_it_does_not_apply`; `d1_a_filter_on_a_data_field_filters_the_linked_item_and_the_other_item_says_it_does_not_apply` (3 sizes) |
| 5th filter refused with the message | `d1_the_fifth_filter_is_refused_and_removing_a_filter_moves_the_picks_up`; the 3-size filter test |
| Renaming a source tab keeps the item; deleted tab says so; title opens the tab | `d1_renaming_a_source_tab_keeps_the_item_and_a_closed_one_says_source_tab_removed_and_the_title_opens_the_tab` |
| Every control works at its real rect at three sizes | the render tests above, plus `d1_pages_are_added_renamed_reordered_and_deleted_after_a_confirm_and_items_move_and_go`, `d1_outside_a_project_the_dashboard_says_so` |

Stub proof: D1 — page picks not added to the chart's filters → the lookup-filter test fails (restored).
Picture: `docs/guide/img/dashboard-page.png` (light opened, dark written). Decisions: `docs/phase10-decisions.md`.

## S3 acceptance → test (D2)
| Acceptance | Test |
|---|---|
| 4,825.00 Cr; +125.00 Cr · +2.66%; ▲ | `d2_closing_deposits_4825_cr_against_4700_cr_is_plus_125_cr_and_plus_2_66_percent_up_and_good` |
| Yield +0.50 pp ▲; cost of funds −0.40 pp ▼ green when Down is good | `d2_a_yield_of_11_against_10_50_is_plus_0_50_pp_and_a_cost_of_funds_down_0_40_is_green_when_down_is_good` |
| Earlier 0 or blank: percent "—", amount stays | `d2_an_earlier_value_of_zero_has_no_percent_and_a_blank_one_has_no_change_and_never_a_zero` |
| Below 80.00 → amber on 79.50, none on 80.00 | `d2_below_80_is_amber_on_79_50_and_no_colour_on_80_00_and_the_first_matching_limit_wins` |
| Blank measure: "—" and its reason, never 0 | `d2_a_blank_measure_shows_a_dash_and_its_reason_never_a_zero`, `d2_a_month_with_no_balance_gives_no_number_and_the_pivots_reason` |
| Unit switch changes display, not the stored number | `d2_the_unit_switch_changes_the_display_and_not_the_stored_number`; render test |
| Sparkline of 12 points with one gap | `d2_the_card_works_out_the_closing_balance_the_prior_period_and_a_sparkline_with_one_gap` |
| Arrow and words without colours | `d2_the_arrow_and_the_words_carry_the_change_when_the_colours_are_taken_away` |
| Window: add, settings, fit at 3 sizes | `d2_a_kpi_card_is_made_by_clicks_shows_number_arrow_and_words_and_its_settings_work_at_three_sizes`, `d2_a_card_that_cannot_be_worked_out_says_why_and_a_picture_is_written_in_both_themes` |

Stub proof: D2 — "below" made "at or below" → the 80.00 limit test fails (restored). Picture: `docs/guide/img/dashboard-kpi.png` (light opened).
