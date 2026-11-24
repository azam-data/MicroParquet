# Phase 9 progress (Model)

Branch phase9. Sessions S1–S9 as in the spec. Resume from this file.

| Task | Status | Commit | Tests |
| --- | --- | --- | --- |
| P1 | DONE | (see git log: "Phase 9 P1") | tests/phase9/p1.rs x2 (the saved rectangle is none in old files, round-trips and leaves old bytes alone; the preview brings back the rows asked for, 20 to 500); render_tests_p9 x10 (Pop out by real clicks and Apply gives the docked step as JSON, Dock and the window's close button, Cancel and Esc, the preview's rows and width, where the window opens, the rectangle saved and restored, no Pop out without windows, the fit at 3 sizes with windows of 900x600 and 1600x1000, closing the source tab, the note's Dock) |
| B0 | DONE | (see git log: "Phase 9 B0") | tests/phase9/b0.rs x1 (3 macros, the pack's 2 sheets and 2 charts, the saved chart, the project's 7 tabs and its text after save-open-save replay to the hashes the v1.8.0 code gave) |

## B0 acceptance: item -> test

| Acceptance item (spec B0) | Test |
| --- | --- |
| `b0_the_phase8_samples_replay_to_the_hashes_the_phase8_code_gave` passes with today's code | `tests/phase9/b0.rs` `b0_the_phase8_samples_replay_to_the_hashes_the_phase8_code_gave` (16 hashes: 3 macros, the pack's 2 sheets and 2 charts, the saved chart, 7 project tabs, the project's text after save-open-save) |
| The Phase 7 replay test still passes | `tests/phase8/w0.rs` `w0_the_phase7_samples_replay_to_the_hashes_the_phase7_code_gave` (run: ok) |

Proved to fail without the fix (stubs in `src/models.rs` and `src/project.rs`, restored): `b0_the_phase8_samples_replay_to_the_hashes_the_phase8_code_gave` fails with "macro:phase8_rate_band_unnumbered" and "project:bytes after save-open-save" differing (the unnumbered label turned numbered; `PROJECT_VERSION` 2).

Deviations: B0-1 to B0-4 (in `docs/phase9-spec.md` `<accepted_deviations>`).

## P1 acceptance: item -> test

Library tests are `tests/phase9/p1.rs` (`cargo test -q --test phase3 p9_p1`); UI tests are in `src/ui/render_tests_p9.rs`
(`cargo test -q --bin MicroParquet p1_`), each at the 3 main-window sizes (1219x686, 1600x900, 1291x700 pt) unless noted, with
the builder's window at 900x600 and 1600x1000 where it is drawn. Totals: 2 library + 10 UI = 12.

| Acceptance item (spec P1) | Test |
| --- | --- |
| Pop out by a real click; add a Rows field, a value and a filter in the pop-out; the preview updates; Apply makes a step equal (JSON) to the docked step from the same clicks | `p1_pop_out_by_a_real_click_edits_the_same_state_and_apply_makes_the_step_the_docked_dialog_makes` (never both: the docked dialog is not drawn while the window has the builder; the same state, as Debug, as the docked clicks make; the preview is Ok in the window) |
| Close the window: the docked dialog shows the same fields and values, unapplied changes included | `p1_the_dock_button_and_the_windows_close_button_dock_the_builder_back_with_everything_as_it_was` (Dock button and a real close event; spec, chips, the open list after the close button, nothing applied, the dialog fits) |
| Editing an existing pivot step in the pop-out and Cancel leaves the step unchanged; Esc is Cancel | `p1_cancel_in_the_window_leaves_the_edited_step_unchanged_and_esc_is_cancel_unless_a_list_is_open` |
| The preview shows more rows in a 1600x1000 pop-out than docked; its columns use the window's width | `p1_the_window_previews_as_many_rows_as_fit_more_than_the_docked_dialog_and_uses_its_width` (library: `p1_the_preview_brings_back_the_rows_asked_for_and_never_fewer_than_the_docked_twenty`) |
| The saved rectangle is restored, and an off-screen one is centred | `p1_the_saved_window_is_restored_where_the_monitor_shows_it_and_an_off_screen_one_is_centred` (the rule), `p1_the_window_rectangle_is_saved_in_settings_when_the_builder_docks_and_opens_there_next_time` (read from the window, kept in `settings.json`, restored, off-screen centred), library `p1_the_saved_window_rectangle_is_none_in_old_files_round_trips_and_leaves_old_bytes_alone` |
| Closing the source tab, or the app, with the pop-out open follows the docked dialog's rule | `p1_closing_the_source_tab_with_the_builder_open_behaves_the_same_docked_and_in_its_window` (the app: `on_exit` keeps the rectangle) |
| Render tests by real clicks at the 3 main sizes with the pop-out at 900x600 and 1600x1000 | every UI test above; layout (nothing overflows or is clipped, no two labels overlap, every button inside the window): `p1_the_pop_out_button_and_the_windows_header_fit_and_nothing_overlaps_at_three_sizes`; `p1_the_note_in_the_app_window_docks_the_builder_by_a_real_click`; `p1_where_windows_cannot_open_there_is_no_pop_out_and_a_popped_builder_docks_back` |

Proved to fail without the fix (stubs, restored): with the docked dialog drawn as well as the window, the Esc key not read, the
rectangle not saved, any saved rectangle accepted, and docking forgetting the choices, these fail: `p1_pop_out_by_a_real_click…`,
`p1_the_dock_button_and_the_windows_close_button…`, `p1_cancel_in_the_window…`, `p1_the_note_in_the_app_window_docks…`,
`p1_the_saved_window_is_restored…`, `p1_the_window_rectangle_is_saved_in_settings…`, `p1_the_window_previews_as_many_rows…` (the preview
always 20 rows); with the preview cut to 20 and the setting always written, the two library tests fail
(`p1_the_preview_brings_back_the_rows_asked_for…`, `p1_the_saved_window_rectangle_is_none_in_old_files…`). Also fixed on the way:
`no_size_literal_is_left_outside_theme_rs` (the new sizes are in `theme.rs`; `render_tests_p9.rs` is listed with the other render tests).

Deviations: P1-1 to P1-8 (in `docs/phase9-spec.md` `<accepted_deviations>`).
