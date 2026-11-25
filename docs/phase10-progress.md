# Phase 10 progress

Branch `phase10` (draft PR to `main`). Version stays 1.9.0; no tag; never merge (owner merges after his own test).
Spec: `docs/phase10-spec.md`. Decisions: `docs/phase10-spec.md` <accepted_deviations> (D0-1 … D0-4 so far).

| Session | Tasks | Status |
|---|---|---|
| S1 | D0, C0 | D0 done, C0 done |
| S2 | D1 | done |
| S3 | D2 | done |
| S4 | D3 | done |
| S5 | D4 | done |
| S6 | D5 | done |
| S7 | R1 (and D5 page shape) | done |
| S8 | R2 (+ corrections, heatmap/histogram, pivot unit) | done |
| S9 | X1 (+ combo chart, sparkline) | done |
| S10 | X2 PDF pages and the export dialog | done |
| S8–S12 | R2 … fixes | not started |

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

## S4 acceptance → test (D3)
| Acceptance | Test |
|---|---|
| Dhaka click gives the Product bar Dhaka rows only; clicked item keeps its marks | `d3_clicking_dhaka_gives_the_product_bar_the_numbers_of_dhaka_rows_only_and_the_clicked_bar_keeps_all_its_marks`; render `d3_a_click_on_a_bar_filters_…` |
| A pivot row click filters the charts | `d3_a_pivot_row_click_filters_the_charts_flat_and_nested_…`; render test |
| Another data tab: "filter doesn't apply" | both of the above |
| Back after two drill levels restores the first | `d3_back_after_two_drill_levels_restores_the_first_level_…`; render `d3_the_right_click_menu_…` |
| State saved and restored with the project | `d3_the_cross_pick_and_the_drill_paths_are_saved_and_restored_with_the_project` |

Stub proofs: D3 — `cross_for` that also filters the clicked item → the Dhaka test fails; `drill_back_to` cutting one level too late → the Back test fails (both restored). Picture: `docs/guide/img/dashboard-cross.png` (light opened).

## S5 acceptance → test (D4)
| Acceptance | Test |
|---|---|
| Drag and resize by real events at 3 sizes | render `d4_the_layout_switch_says_what_changes_and_a_free_page_moves_resizes_snaps_and_clamps_by_real_drags_at_three_sizes` |
| Snap to 10, inside the page, no overlap unless on | `d4_a_move_snaps_…`, `d4_each_corner_and_edge_resizes_…`, `d4_items_may_not_overlap_…` |
| Grid → Free → Grid same positions | `d4_grid_to_free_to_grid_gives_the_same_positions_…`; render `d4_grid_to_free_to_grid_by_clicks_…` |
| Dragged off the page is clamped, never lost | `d4_a_move_snaps_…`; render test (900 pt off) |

Stub proofs: D4 — no snapping in `snap` → the move and resize tests fail; overlap test disabled → the overlap test fails (both restored). Picture: `docs/guide/img/dashboard-free.png` (light opened).

## S6 acceptance → test (D5)
| Acceptance | Test |
|---|---|
| 1920×1080 PNG, 4 slots, save, reopen: same template, slots, hash | `d5_a_1920_by_1080_png_with_4_slots_is_the_same_template_after_save_and_reopen`; render `d5_a_design_is_imported_slots_are_drawn_named_saved_applied_and_filled_by_clicks_…` |
| Project copy used when `portable_data` has none | `d5_the_project_copy_is_used_…`; render `d5_the_project_copy_shows_the_design_…` |
| Image behind the items; items keep their palette | render flow test (`chart_fills` equal with and without the design) |
| Hash mismatch reported | both of the copy tests |
| Refusals: not a picture, over 3 MB, slot outside | `d5_a_file_that_is_not_a_picture_a_picture_over_3_mb_and_a_slot_outside_are_refused_in_words`; render `d5_a_file_that_is_not_a_picture_…` |

Stub proofs: D5 — hash check switched off → the copy test fails; slot kind check switched off → the slot test fails (both restored). Picture: `docs/guide/img/dashboard-design.png` (light opened).

## S7 acceptance → test (R1)
| Acceptance | Test |
|---|---|
| 12 and 4 values: 65 states, 390 numbers, 3,120 bytes ±10% | `r1_two_filters_of_12_and_4_values_make_65_states_and_a_6_bar_chart_390_numbers_at_8_bytes_each` |
| Estimate within 10% of the real file | same test (stub file) |
| 5.3 MB by email refused, suggestions in order, fewer values under 5 MB | `r1_five_point_three_mb_by_email_is_refused_…` |
| Answers equal `compute_item` for 20 random states | `r1_the_answers_equal_compute_item_for_20_random_states_…` |
| Non-additive items listed by name | `r1_the_items_that_do_not_react_to_clicks_are_listed_by_name_with_why` |
| No account rows in the output | `r1_no_account_row_goes_into_the_file_…` |
| (coordinator note) page takes the picture's shape | `d5_a_page_takes_the_shape_of_its_picture_…`; render flow test (16:9 page area) |

Stub proofs: R1 — states counted as n instead of n + 1 fails the 65-state test; the email limit made exclusive fails the gate test (both restored). No picture (R1 has no window).

## S8 acceptance → test (R2)
| Acceptance | Test |
|---|---|
| file opens in a headless browser, picks a filter, clicks a bar, drills, Back, expands a pivot row; each number equals `compute_item`; no console errors; no request leaves the file | `r2_the_page_in_a_headless_browser_shows_the_numbers_of_compute_item_through_filter_click_drill_back_and_expand` (skips with a note if node/playwright is missing) |
| estimate within 10% of the real file | same test |
| a layout that does not match the dashboard is refused in words | `r2_a_page_layout_that_does_not_match_the_dashboard_is_refused_in_plain_words` |

Stub proofs: R2 — the page ignoring the click's answers fails "crossed i2"; the expand button doing nothing fails "rows 3 then 3" (both restored). No picture (the file is the output).

## S9 acceptance → test (X1)
| Acceptance | Test |
|---|---|
| read back with the zip reader: every CRC, content types first, smaller than stored | `x1_the_package_reads_back_with_every_crc_content_types_first_and_deflate_smaller_than_stored` |
| one slide per item; chart type per chart tab; series refs point at the workbook; palette | `x1_one_slide_per_item_each_chart_part_has_its_chart_type_palette_and_workbook_ranges` |
| footnote with as-of and filters; pivot table unit; KPI card text and colours | `x1_the_footnote_the_pivot_table_and_the_kpi_card_say_what_the_app_says` |
| LibreOffice converts the file (when installed) | `x1_libreoffice_opens_the_file_when_it_is_installed` |
| design as background only when asked | `x1_a_design_picture_goes_behind_every_slide_only_when_asked` |

Stub proofs: X1 — always-Stored compression fails the deflate test; dropping the threshold colour on the KPI number fails the card test (both restored). Added with it: pivot item unit (`r2_a_pivot_item_shows_money_in_its_unit…`; counts-as-money stub failed it), heatmap/histogram test.

## S10 acceptance → test (X2)
| Acceptance | Test |
|---|---|
| page count; A4 landscape; KPI number as the card shows it; image once per page; footer; positions within 1 mm; no overflow | `x2_pages_text_picture_footer_and_positions_of_a_free_layout_page` (poppler; skipped with a note if absent) |
| empty dashboard and unreadable picture refused in words, no half file | `x2_an_empty_dashboard_is_refused_and_a_picture_that_is_not_one_says_so` |
| combo chart with a right axis; card sparkline chart | `x1_one_slide_per_item_each_chart_part_has_its_chart_type_palette_and_workbook_ranges` |

Stub proofs: X2 — shifting every item 3 mm fails the position test; not drawing the background fails the picture test (restored).

## S10b acceptance → test (export dialog)
| Acceptance | Test |
|---|---|
| email question first; size before writing; the three files written by real clicks | `x3_the_export_dialog_asks_the_email_question_checks_the_size_and_writes_the_three_files` (render test) |
| the screen's filters in the files; the design counted and inlined; as-of said when unknown | `x3_the_html_file_counts_its_picture…`, `x3_powerpoint_and_pdf_show_the_screens_filters…` |

Stub proof: letting "Check size" work before the email question is answered fails the dialog test (restored).

## S11 acceptance → test (D9)
| Acceptance | Test |
|---|---|
| nine notes, eight sections in order, each names tests that exist, ≤150 lines, no code words | `d9_the_nine_notes_have_their_eight_sections_and_their_examples_are_the_tables_the_code_gives` |
| the example tables are what the code gives | same test, with `d9_the_example_tables_of_the_notes_are_made_by_the_dashboards_own_calls` |
| every named picture exists in light and dark; README lists the notes | `d9_every_picture_a_note_names_exists_in_light_and_dark_and_the_readme_lists_the_notes` |
| changed notes say what the code does | `d9_the_notes_that_phase_10_changed_say_what_the_code_now_does` |

Stub proofs: D9 — a wrong number in a table fails the tables test; a renamed section fails the sections test (both restored). No new picture.
