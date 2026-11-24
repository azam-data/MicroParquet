# Phase 9 progress (Model)

Branch phase9. Sessions S1–S9 as in the spec. Resume from this file.

| Task | Status | Commit | Tests |
| --- | --- | --- | --- |
| P1 | DONE (S1), follow-ups DONE (S2) | (see git log: "Phase 9 P1", "Phase 9 P1: follow-ups") | tests/phase9/p1.rs x2 (the saved rectangle is none in old files, round-trips and leaves old bytes alone; the preview brings back the rows asked for, 20 to 500); render_tests_p9 x10 (Pop out by real clicks and Apply gives the docked step as JSON, Dock and the window's close button, Cancel and Esc, the preview's rows and width, where the window opens, the rectangle saved and restored, no Pop out without windows, the fit at 3 sizes with windows of 900x600 and 1600x1000, closing the source tab, the note's Dock) |
| P2 | DONE (S2), follow-up DONE (S3) | (see git log: "Phase 9 P2") | tests/phase9/p2.rs x16 (subtotals, grand total, leaves, hash of the flat table, three levels, blank lines, Columns, last-step rule, Excel XML, scale, PDF text and indent, CSV and Python, Show rows, old pivots, the sheet limit); render_tests_p9 x5 (the dialog, the tab's bar and undo, Show rows by clicks, steps after the pivot, the pop-out) |
| M0 | DONE (S3) | (see git log: "Phase 9 M0") | tests/phase9/m0.rs x10 (composite key and duplicates, sorted duplicates, blank keys, unmatched keys and sums, the refusals, project v2 round trip and v1 bytes, the v1.8.0 refusal, relink to a duplicate, the open line and status texts, a tab changed since); render_tests_p9 x5 (the menu with and without a project, Links add/edit/remove and refusals, Key check and its three buttons, project save/open with the open line, pictures) |
| M1 | DONE (S4), follow-ups DONE (S5), three labels DONE (S6) | (see git log: "Phase 9 M1") | tests/phase9/m1.rs x12; render_tests_p9 x7 (field list, chart pickers, filter dialog and value picker, chart filters, Show rows, broken links, closed tab) + 1 picture test |
| M2 | DONE (S5) | (see git log: "Phase 9 M2") | tests/phase9/m2.rs x11; render_tests_p9 x5 (menu, Calendar dialog, Calendar tab and fields, As-of dialog, pictures) |
| M3 | DONE (S6) | (see git log: "Phase 9 M3") | tests/phase9/m3.rs x15; render_tests_p9 x4 (Measures dialog, aggregation choice, banner and tooltip, pictures) |
| M3 follow-up | DONE (S7) | (see git log: "Phase 9 M3: follow-ups") | tests/phase9/m3.rs x3 (Show rows on a time cell); render_tests_p9 x2 |
| M4 | DONE (S7) | (see git log: "Phase 9 M4") | tests/phase9/m4.rs x9; render_tests_p9 x5 (Values choices by clicks, the refusal, a Month-Year pivot, the chart studio, pictures) |
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

## P1 follow-ups (S2): the owner's decisions on S1

Accepted as they stand: B0-1 to B0-4, P1-1, P1-4 to P1-8. Replaced: P1-2 and P1-3. Both are written under "Owner decisions on S1" in
`docs/phase9-spec.md` `<accepted_deviations>` (with P1-9 and P1-10, found on the way).

| Decision | Test |
| --- | --- |
| P1-2: the pop-out opens at its saved position and size exactly as they are, on any monitor (no monitor check, no centring); centred on the main window only when no rectangle is saved | `p1_the_saved_window_is_used_as_it_is_on_any_monitor_and_it_is_centred_only_when_none_is_saved` (the rule: on the monitor, off every screen, left, above, below, right, a size bigger than the monitor, none saved, the least size, a damaged number) |
| P1-2: an off-screen saved rectangle is used, and the Dock button of the note in the app window brings the builder back by a real click | `p1_the_window_rectangle_is_saved_in_settings_when_the_builder_docks_and_opens_there_next_time` (last part: saved at 9000, 7000 opens there; click on the note's Dock docks it; the saved rectangle is left as it was) |
| P1-3: Esc is Cancel in the docked dialog and in the pop-out; nothing happens to the dialog while a text box has the keyboard, the Filters list is open, or a drop-down list or menu is open (Esc closes that first, the next Esc is Cancel) | `p1_esc_is_cancel_docked_and_in_the_window_unless_a_text_box_or_an_open_list_has_it`: by real key presses at 1219x686, 1600x900 and 1291x700, docked and in windows of 900x600 and 1600x1000: Esc cancels (the step is as it was); Esc in the field search box; Esc with the Filters list open; Esc with a field's menu open; Esc with the aggregation list open |
| Old Esc test of the pop-out | `p1_cancel_in_the_window_leaves_the_edited_step_unchanged_and_esc_is_cancel_unless_a_list_is_open` (unchanged, passes) |

Proved to fail without the fix (stubs, restored): with the docked dialog not reading Esc, the last frame's text-box state ignored
(egui takes the keyboard from a text box when Esc arrives, so the builder has to remember it), and the open menu not counted, the
new Esc test fails (`p1_esc_is_cancel_docked_and_in_the_window_unless_a_text_box_or_an_open_list_has_it`, three runs); with the old
monitor rule put back, `p1_the_saved_window_is_used_as_it_is_on_any_monitor_and_it_is_centred_only_when_none_is_saved` and
`p1_the_window_rectangle_is_saved_in_settings_when_the_builder_docks_and_opens_there_next_time` fail.



## P2 acceptance: item -> test (S2)

Library and file tests are `tests/phase9/p2.rs` (`cargo test -q --test phase3 p9_p2`); UI tests are in `src/ui/render_tests_p9.rs`
(`cargo test -q --bin MicroParquet p2_`), at the 3 main-window sizes (1219x686, 1600x900, 1291x700 pt), light and dark where the
tab or dialog is drawn. The synthetic tab for the library tests has the owner's shape: Bank (Conventional, Islami) › CL status
((blank), HC, LC, NC) with Sum of Balance, Count of AC Number, W.Avg of Rate by Balance (and Count distinct, Average, Min, Max);
A1 and A2 are under two CL statuses of Conventional, A6 under two of Islami, A11 under two Branches. The UI tests use 150 branches
in 3 products (Product › Branch).

| Acceptance item (spec P2) | Test |
| --- | --- |
| Every subtotal row equals a pivot by Bank alone (Sum, Count, W.Avg rate), and a Count distinct that is not the sum of its children | `p2_every_subtotal_row_equals_a_pivot_by_bank_alone_and_a_distinct_count_is_not_the_sum_of_its_children` (all seven aggregations; Conventional counts 6 distinct accounts, its children add up to 8; the Average is over its 8 rows) |
| (the check behind "Found, not changed") Side by side subtotals are calculated, not added up | `p2_side_by_side_subtotals_are_calculated_from_the_group_s_rows_not_added_up` |
| The Grand Total equals Side by side's; the leaves equal the flat table row for row; order and labels | `p2_the_grand_total_equals_side_by_sides_and_the_leaves_equal_the_flat_tables_rows_one_for_one` |
| Switching layout never changes a number: the flat table's hash is the same in both layouts | `p2_switching_layout_never_changes_a_number_the_flat_tables_hash_is_the_same` (3 row sets, with and without the Subtotals box; the step's result and through the tab's compiler from a file) |
| A three-level Nested pivot (Bank › CL status › Branch) has correct subtotals at both parent levels | `p2_a_three_level_pivot_has_correct_subtotals_at_both_parent_levels` |
| The optional blank line after each top-level group | `p2_blank_lines_follow_each_top_level_group_when_chosen_and_never_carry_numbers`; Excel: `p2_the_excel_export_has_one_row_labels_column_with_indent_0_1_2_bold_parents_and_grand_total_and_the_blank_rows` (both settings) |
| Columns fields spread across the columns as in Side by side; one Rows field; refusals in words | `p2_columns_fields_spread_across_the_columns_and_a_single_rows_field_has_leaves_and_a_grand_total` |
| The Excel export: indent 0/1/2 on the label cells, bold on subtotal and Grand Total rows, a blank row after each group (from the workbook XML) | `p2_the_excel_export_has_one_row_labels_column_with_indent_0_1_2_bold_parents_and_grand_total_and_the_blank_rows` (cell styles read from `styles.xml`: the alignment indent equals the level, no spaces; numbers of a bold line are bold); the pivot's number scale: `p2_the_excel_export_keeps_the_pivots_scale_and_says_it_in_the_heading` |
| The PDF page text lists the rows in Nested order; 10 pt per level | `p2_the_pdf_lists_the_rows_in_nested_order_with_the_indent` (pdftotext of the exported file); `p2_the_pdf_indents_ten_points_a_level_and_sets_parents_and_the_grand_total_in_bold` |
| CSV and Parquet exports, later steps, charts, macros and Python see the flat table | `p2_csv_exports_and_python_see_the_flat_table_whatever_the_layout`, and the hash test above (the step's result is the flat table, which is all that charts, macros and exports read) |
| With steps after the pivot: the flat table and the note, exports flat | `p2_a_pivot_that_is_the_tabs_last_step_has_a_nested_view_and_one_that_steps_follow_has_the_flat_table_and_a_note`; UI: `p2_with_steps_after_the_pivot_the_tab_shows_the_flat_table_and_says_so_at_three_sizes` (the note on the screen, the flat grid, and the view is back when the step is taken off) |
| Show rows on "Conventional" gives exactly its rows and its sum equals the subtotal | `p2_show_rows_on_a_parent_row_gives_exactly_the_group_s_rows_and_their_sum_is_the_subtotal` (8 rows, 1,060; a leaf; the Grand total: 16 rows); UI by real double-clicks and the cell menu: `p2_show_rows_on_a_parent_line_gives_every_row_of_its_group_a_leaf_and_the_grand_total_work_as_before_at_three_sizes` |
| An old pivot loads as Side by side, unchanged | `p2_an_old_pivot_loads_as_side_by_side_unchanged_and_writes_no_layout`; and B0's replay (`b0_the_phase8_samples_replay_to_the_hashes_the_phase8_code_gave`, project and macros with pivots) still passes |
| Render tests by real clicks at 3 sizes, light and dark: the layout choice in the dialog, the blank-line option, bold and indent drawn, Show rows from a parent row | `p2_the_dialog_chooses_the_layout_and_the_blank_line_and_the_tab_draws_bold_parents_and_the_indent_light_and_dark_at_three_sizes` (the Layout box and the blank-line option by real clicks, the dialog fits, no two labels overlap, nothing cut, the step carries the layout; the tab draws one "Row Labels" column, a bold parent with a bold subtotal, its child indented 16 px, a leaf not bold, blank lines, the bold Grand total at level 0); the pop-out: `p2_the_pop_out_builder_chooses_the_layout_too_and_apply_makes_the_step_the_docked_dialog_makes`; Show rows: above |
| The layout "can be switched on a pivot tab without rebuilding the flat table" | `p2_a_pivot_tab_switches_its_layout_from_the_bar_without_rebuilding_the_flat_table_and_undo_takes_it_back_light_and_dark_at_three_sizes` (a marker in the tab that a rebuild would overwrite is still there after the switches; three undos) |
| Nested lines that don't fit an Excel sheet are refused in words, not cut | `p2_a_nested_pivot_with_more_lines_than_a_sheet_holds_is_listed_on_export_errors_not_cut` |

Deviations: P2-1 to P2-10 and "Found, not changed" (in `docs/phase9-spec.md` `<accepted_deviations>`).

Pictures to look at by eye (light and dark, written by `p2_pictures_of_the_layout_in_the_dialog_and_of_the_nested_tab`):
`docs/guide/img/pivot-layout-dialog.png`, `pivot-layout-dialog-dark.png` (the dialog with the Layout box and the blank-line option in its header), `pivot-nested-tab.png`, `pivot-nested-tab-dark.png` (the tab with its bar).

Proved to fail without the fix (stubs, each restored; failing tests by stub):
- Parents taking the first child's numbers: `p2_every_subtotal_row_equals…`, `p2_a_three_level_pivot_has_correct_subtotals…`, `p2_show_rows_on_a_parent_row_gives_exactly…`, `p2_the_excel_export_keeps_the_pivots_scale…`.
- Grand total blank: `p2_the_grand_total_equals_side_by_sides…`, `p2_the_excel_export_has_one_row_labels_column…`.
- The layout changing the flat table (subtotals forced on): `p2_switching_layout_never_changes_a_number…`, `p2_csv_exports_and_python_see_the_flat_table…`, `p2_a_pivot_that_is_the_tabs_last_step…`.
- Excel indent 0, and not bold: `p2_the_excel_export_has_one_row_labels_column…` (each alone). Excel scale ignored: `p2_the_excel_export_keeps_the_pivots_scale…`.
- PDF indent ignored: `p2_the_pdf_indents_ten_points_a_level…`, `p2_the_pdf_lists_the_rows_in_nested_order_with_the_indent`.
- Show rows ignoring the line's values: `p2_show_rows_on_a_parent_row_gives_exactly…`. Layout always written: `p2_an_old_pivot_loads_as_side_by_side…`.
- No blank lines: `p2_blank_lines_follow_each_top_level_group…`, the Excel test. Followed pivot never noted: `p2_a_pivot_that_is_the_tabs_last_step…`. Column keys lost: `p2_columns_fields_spread_across_the_columns…`. Sheet limit removed: `p2_a_nested_pivot_with_more_lines_than_a_sheet_holds…`.
- UI: Nested grid never shown: `p2_the_dialog_chooses…`, `p2_a_pivot_tab_switches_its_layout…`, `p2_show_rows_on_a_parent_line…`, `p2_with_steps_after_the_pivot…`. Switch rebuilding the flat table: `p2_a_pivot_tab_switches_its_layout…`. No note: `p2_with_steps_after_the_pivot…`. Show rows reading the flat row: `p2_show_rows_on_a_parent_line…`. Indent not drawn, bold not drawn: `p2_the_dialog_chooses…`. The dialog unable to choose Nested, or the blank-line option doing nothing: `p2_the_dialog_chooses…`, `p2_the_pop_out_builder_chooses_the_layout_too…`. The stale Nested report kept after switching back: `p2_a_pivot_tab_switches_its_layout…`.
- Not stubbed: `p2_side_by_side_subtotals_are_calculated…` (it documents what the unchanged code does; it passes on it).

Found while doing P2: the P1 test `p1_the_window_previews_as_many_rows_as_fit…` failed when the Layout box was a new row of the toggles (the window lost two preview rows), so the box is in the dialog's header row (P2-10). Also: a first click on the second field's "..." menu of a fresh dialog does not open it in the headless frames (see "Found, not changed").

## P2 follow-up (S3): the owner's decisions on S2

Written under "Owner decisions on S2" in `docs/phase9-spec.md` `<accepted_deviations>`: P1-9, P1-10, P2-1 to P2-10 accepted (P2-1 changed); Questions 1, 3, 4 as the owner answered; new P2-11.

| Decision | Test |
| --- | --- |
| P2-1 changed: with Nested chosen the dialog's preview (docked and pop-out) is the Nested view (Row Labels column, bold parents, indent, blank lines, Grand total); Side by side previews the flat table; switching the layout changes only the preview | `p2_the_dialogs_preview_shows_the_nested_view_the_tab_shows_docked_and_in_the_window_at_three_sizes` (3 sizes x light/dark x docked, 900x600 and 1600x1000 windows, all by real clicks: the flat columns first, Nested by the Layout box, only the layout changed in the spec, "Row Labels" drawn and no Rows-field columns, parent in the tab's bold, child one level (16) in, no overlap, the blank-line option, back and forth between layouts, Apply; then the preview's lines equal the tab's Nested view line for line: labels, levels, bold flags, blank lines, kinds, keys and values; docked 20 lines, the 1600x1000 window more) |
| The P1 test of more preview rows in the pop-out still passes | `p1_the_window_previews_as_many_rows_as_fit_more_than_the_docked_dialog_and_uses_its_width` (run: ok) |

Proved to fail without the fix (stubs, restored): the worker not returning the Nested view (`p2_the_dialogs_preview_shows_the_nested_view…` times out waiting for it); the dialog drawing the flat table when a Nested view is there (same test: "the preview has a Row Labels column").

The pictures `docs/guide/img/pivot-layout-dialog.png` and `-dark.png` changed on purpose (the dialog's preview is Nested now). The full test run also rewrote `pivot-date-expander*.png` and `sources-dialog*.png`; those were restored.



## M0 acceptance: item -> test (S3)

Library tests are `tests/phase9/m0.rs` (`cargo test -q --test phase3 p9_m0`); UI tests are in `src/ui/render_tests_p9.rs` (`cargo test -q --bin MicroParquet m0_`), at 1219x686, 1600x900 and 1291x700 pt, light and dark.

| Acceptance item (spec M0) | Test |
| --- | --- |
| A product master keyed by (Wing, Product code) with SB01 in both wings links correctly; a second (Conventional, SB01) row makes it Broken, Key check lists that key with both rows; Show duplicates gives those 2 rows with Times = 2 | `m0_a_product_master_keyed_by_wing_and_product_code_links_and_a_second_row_makes_it_broken_and_lists_both_rows`; sorted by key and a "Times" column of the lookup's own kept: `m0_duplicates_are_sorted_by_key_and_a_column_called_times_is_not_overwritten` |
| A lookup with a blank key is Broken | `m0_a_blank_key_in_the_lookup_makes_it_broken_and_a_blank_key_in_the_data_is_counted_apart` (missing and only-spaces keys) |
| Unmatched data keys with the right row counts and sums; Show unmatched rows gives exactly those rows; Copy list | `m0_unmatched_keys_are_listed_with_their_rows_and_sums_and_show_unmatched_rows_gives_exactly_those_rows`; UI by real clicks: `m0_key_check_lists_duplicates_blanks_and_unmatched_keys_and_its_three_buttons_open_and_copy_the_lists_light_and_dark_at_three_sizes` |
| The role, type, second-link and chart-tab refusals give their messages | `m0_the_role_type_second_link_and_chart_tab_refusals_give_their_messages`; in the dialog: `m0_links_are_added_edited_and_removed_by_real_clicks_and_refusals_are_said_in_the_dialog_light_and_dark_at_three_sizes` (role and second link) |
| A project with a model round-trips (same bytes but `saved_at`) and is written as version 2 | `m0_a_project_with_a_model_is_version_2_and_round_trips_and_one_without_is_version_1_with_no_model_section`; through the app: `m0_a_project_with_links_saves_and_opens_and_a_lookup_file_with_a_duplicate_key_makes_its_link_broken_in_the_open_line` |
| A project without a model is written as version 1, byte for byte as B0's baseline | the same test (no `model` section, version 1 even if memory says 2) and `b0_the_phase8_samples_replay_to_the_hashes_the_phase8_code_gave` (passes) |
| The v1.8.0 refusal text appears for version 2 | `m0_the_v1_8_0_app_refuses_a_version_2_project_with_its_newer_app_message` (through `check_version(.., 1)`) |
| Relinking a lookup file (W4) to a file with a duplicate key makes the link Broken in the open line | library `m0_relinking_a_lookup_file_to_one_with_a_duplicate_key_makes_the_link_broken_in_the_open_line`; in the app (real Open, "2 tabs opened; link 'Deposits -> Branches' is broken: 1 duplicate key", the status bar's "Model: 1 link · 1 broken"): the project test above |
| Render tests at 3 sizes: the Model menu with and without a project | `m0_the_model_menu_says_a_project_is_needed_with_none_and_opens_links_and_key_check_with_one_at_three_sizes` (Model between View and Banking; the message; Save project as...; Links and Key check open) |
| Links... add / edit / remove | the Links test above (tabs and key pairs picked by clicks, a second pair added and taken away, Remove; the status "OK · 10 unmatched keys", the status bar item, a renamed tab keeps its link) |
| Key check and its three buttons | the Key check test above (counts, the sums box, Copy list, Show unmatched rows = exactly B141 to B150, Show duplicates = the 2 Gold rows with Times 2, a greyed button opens nothing) |
| The open line and the status bar | the project test (open line, status bar), the Links and Key check tests (status bar), `m0_the_open_line_the_status_bar_and_the_status_texts_say_what_they_should` |

Pictures to look at by eye (light and dark, written by `m0_pictures_of_the_links_and_key_check_dialogs`): `docs/guide/img/model-links-dialog.png`, `model-links-dialog-dark.png` (the list with an OK-with-unmatched link and a Broken one), `model-key-check-dialog.png`, `model-key-check-dialog-dark.png` (a link's counts, the first unmatched keys with sums, the sums box and the three buttons).

Proved to fail without the fix (stubs, each restored; all 16 runs finished, none cut short):
- duplicates not counted: `m0_a_product_master_keyed…`, `m0_relinking_a_lookup_file…`, UI `m0_a_project_with_links…`, `m0_key_check_lists…`.
- blank lookup keys not counted: `m0_a_blank_key_in_the_lookup…`, UI `m0_key_check_lists…`.
- unmatched rows taken as the matched ones: `m0_unmatched_keys_are_listed…`, UI `m0_key_check_lists…`. Sums wrong: `m0_unmatched_keys_are_listed…`.
- duplicates list with every row: `m0_a_product_master_keyed…`, `m0_duplicates_are_sorted_by_key…`, UI `m0_key_check_lists…`.
- no role refusal, no second-link refusal: `m0_the_role_type_second_link_and_chart_tab_refusals…`, UI `m0_links_are_added_edited…`. No type refusal: `m0_the_role_type_second_link_and_chart_tab_refusals…`.
- version always 1: `m0_a_project_with_a_model_is_version_2…`, `m0_the_v1_8_0_app_refuses…`, UI `m0_a_project_with_links…`. Empty model written: `m0_a_project_with_a_model_is_version_2…` and `b0_the_phase8_samples_replay…`. Version 2 refused by this app: `m0_a_project_with_a_model…`, `m0_relinking_a_lookup_file…`, UI `m0_a_project_with_links…`.
- open line not added, model not saved, model not opened: UI `m0_a_project_with_links…`. No status bar item: UI `m0_a_project_with_links…`, `m0_key_check_lists…`, `m0_links_are_added_edited…`.
- Copy list not kept, and a changed link not re-checked (choosing the sums column changes the link, so its check has to run again): UI `m0_key_check_lists…` fails with each (the second times out waiting for the sums).
- model not part of "unsaved": the first run found no failing test (the test also added a tab, which moves the fingerprint). A model-only change was added to `m0_a_project_with_links…`; it fails with the stub and passes without.



## S4: owner decisions on S3, recorded

Written under "Owner decisions on S3" in `docs/phase9-spec.md` `<accepted_deviations>`: P2-11, M0-1 to M0-11, commit 6e14077 and the version-3 "newer project" tests accepted; M0-3 (blank keys never match, "(blank)" in M1), M0-1 accepted; M0-4 accepted with its status text changed: a link whose check can't run says "Can't check: the tab 'Branch master' is closed" / "Can't check: 'Accounts' has no column 'Branch Code'" (it was "Broken: ..."). Fixed in M1 (`LinkStatus::cant_check`, closed tabs named from a remembered title), tested by `m1_a_broken_link_refuses...`, `m1_the_check_of_a_link_whose_column_is_gone...`, UI `m1_a_link_whose_tab_is_closed...`. The M0 test text "... any more" was updated.

## M1 acceptance: item -> test (S4)

Library: `cargo test -q --test phase3 p9_m1`; UI: `cargo test -q --bin MicroParquet m1_` (3 sizes, light and dark).

| Acceptance item (spec M1) | Test |
| --- | --- |
| Pivot by "Branch master > Region" equals the Join tool's pivot row for row, (no match) holds the unmatched sum, total = ungrouped total | `m1_a_pivot_by_a_lookup_field_equals_the_join_tools_pivot...`; UI by clicks: `m1_the_pivot_field_list_expands_a_link...` |
| Linking never changes the data tab's row count, on every fixture | `m1_linking_never_changes_the_data_tabs_row_count_on_any_fixture` (matched, unmatched, blank keys, 1-3 columns, a lookup with no matching key, a one-row lookup, a composite key, a filter step); UI: data tab stays 150 rows |
| Filter Region = Dhaka keeps exactly Dhaka's rows; the lookup tab unchanged | `m1_a_filter_on_a_lookup_field_keeps_exactly_that_regions_rows...`; UI `m1_the_filter_dialog_takes_a_lookup_field...` (branch master still 140 rows) |
| Filter on (no match) keeps exactly the unmatched rows | `m1_a_filter_on_no_match_keeps_exactly_the_unmatched_rows...`, UI filter dialog (B141 to B150) |
| Composite key (Wing, Product code) gives the right group for SB01 in each wing | `m1_a_composite_key_gives_the_right_product_group_for_sb01_in_each_wing` |
| Chart by a lookup field equals the pivot; drill Region > Branch name adds up; Show rows from a chart mark and a pivot cell equal the cell | `m1_a_chart_by_a_lookup_field_equals_the_pivot_and_a_drill...`; UI `m1_show_rows_from_a_pivot_cell_and_a_chart_mark...`, `m1_the_chart_pickers_list_the_lookup_fields...` |
| Broken link greys its fields with the reason; a pivot using one shows the reason | `m1_a_broken_link_refuses_its_fields_with_the_reason...`; UI `m1_a_broken_links_fields_are_greyed...` |
| Macro with a lookup filter outside a project gives the message; Python export gives its message | `m1_outside_a_project_a_step_with_a_lookup_filter_gives_the_conventions_message...`, `m1_the_scope_is_taken_by_a_job...` |
| Old pivots, charts and filters load unchanged | `m1_old_pivots_charts_and_filters_load_unchanged...` (and B0's replay) |
| Render tests: field list expander, chart picker, filter value picker, Show rows | the UI tests above, plus `m1_filters_on_this_chart_take_a_lookup_field...`; "(no match)" and "(blank)" sort last: library tests and the pickers |
| M0-4 "Can't check" wording | see S3 decisions above |

Pictures (light and dark): `docs/guide/img/pivot-lookup-fields.png`, `chart-picker-lookup-fields.png`, `filter-lookup-values.png` (each with `-dark`).

Deviations: M1-1 to M1-11 (in the spec).

Proved to fail without the fix (stubs, each restored, run one at a time; failing tests by stub):
- no "(no match)" label: lib `m1_a_pivot_by_a_lookup_field_equals...`, `m1_a_filter_on_no_match...`, `m1_a_chart_by...`, `m1_a_numeric_lookup_field...`; UI filters-on-chart, filter dialog, pictures, show rows.
- blank key flag ignored: the same four lib tests.
- sentinels not last: lib `m1_a_chart_by...`; UI `m1_the_chart_pickers...`.
- pivot ignores lookup fields: 8 lib tests (pivot, composite, no match, values, chart, broken, outside project, ...).
- filter lookups not applied: lib filter, no match, row count, outside project, broken, chart, scope tests.
- job forgets the scope: lib `m1_the_scope_is_taken_by_a_job...`. No outside-project message: `m1_outside_a_project...`, `m1_the_scope...`. Python not refused: `m1_outside_a_project...`.
- "Can't check" text old: lib `m1_a_broken_link_refuses...`, `m1_the_check_of_a_link_whose_column_is_gone...`; UI `m1_a_link_whose_tab_is_closed...`.
- new fields always written: lib `m1_old_pivots_charts_and_filters_load_unchanged...`. Broken link not refused: lib `m1_a_broken_link_refuses...`, UI `m1_a_broken_links_fields_are_greyed...`. Menu of a broken field enabled: same UI test.
- chart series lookup ignored: lib chart test, UI `m1_the_chart_pickers...`. Picker (no match) not last: UI chart-filters and filter-dialog tests. Expander missing: UI field-list, broken, pictures tests. Filter dialog not storing the lookup: UI filter dialog, pictures. Show rows dropping the lookup: lib chart and no-match tests, UI show-rows test.

Full `cargo test` before the commit: 9 + 324 + 747 + 60 passed. The full run rewrote `pivot-date-expander*.png` and `sources-dialog*.png`; those were restored. A first full run found `p1_esc_is_cancel...` failing (my wrapper around the field menu); fixed by wrapping only a broken link's menu.


## S5: M1 follow-ups (the owner's decisions on S4)

Written under "Owner decisions on S4" in `docs/phase9-spec.md` `<accepted_deviations>`: M1-1 to M1-11 accepted except M1-4 and M1-6 (replaced); M0-4 wording fix, M1-3 and M1-5 accepted. New choices M1-12 to M1-14.

| Decision | Test |
| --- | --- |
| M1-4: "(no match)" is its own flag (`FilterCondition.no_match`), never a value, on a lookup field of every type; "(blank)" is a matched row with a blank cell and never an unmatched row; the two sets don't overlap | `m1_no_match_and_blank_are_two_separate_filters_on_a_number_and_a_date_lookup_column` (a number column and a date column: "(no match)" keeps exactly A6, A8; "(blank)" exactly A3, A7; no overlap; both together; "not in" the two keeps the rest; "does not equal" a value keeps an unmatched row); `m1_no_match_is_a_flag_of_its_own_so_a_real_value_called_no_match_never_collides_with_it` (a real lookup value "(no match)" is found by value, the flag finds the unmatched rows; written only when on; old conditions load with it off) |
| M1-4: a pivot by a number or date lookup column has "(blank)" and "(no match)" as two rows, "(blank)" first, with the right sums; the rows add up to the ungrouped total | `m1_a_pivot_by_a_number_or_a_date_lookup_column_shows_blank_and_no_match_as_two_rows_blank_first_and_the_total_is_the_ungrouped_total` (Limit and Opened: 210 and 57, total 1,870) |
| M1-4: Show rows of a "(no match)" or "(blank)" row of a number pivot gives the cell's rows; the condition holds the flag, not a value | `m1_show_rows_of_the_no_match_and_blank_rows_of_a_number_pivot_give_the_cells_rows` |
| M1-4: the value picker lists "(no match)" for a number column, last, after "(blank)"; a real click on it stores the flag and Apply keeps exactly the unmatched rows (not the matched row with a blank limit), at 3 sizes, light and dark | UI `m1_the_value_picker_of_a_number_lookup_column_lists_blank_and_no_match_last_and_ticking_no_match_keeps_exactly_the_unmatched_rows_at_three_sizes`; the S4 filter test now asserts the flag (`m1_the_filter_dialog_takes_a_lookup_field_its_value_picker_lists_no_match_last...`) |
| M1-6: repeated keys at the time of a join are refused with the Key check's words, naming the link; the link is reported Broken | library `m1_repeated_keys_at_the_time_of_a_join_are_refused_in_the_key_checks_words_and_the_link_is_reported_broken` ("'Branch master' has 2 duplicate keys. Open Model \u{25b8} Key check to see them.", the queue the app drains, a filter refuses the same way) |
| M1-6: change the lookup file under an open project to add a duplicate: the pivot refuses with that message, and the link shows Broken without reopening the project (status bar and Links...) | UI `m1_a_lookup_that_gets_a_repeated_key_after_the_check_is_refused_in_the_key_checks_words_and_its_link_shows_broken_without_reopening_the_project` (3 sizes) |

Proved to fail without the fix (stubs, each restored; run one at a time, every run finished):
- flag ignored (an unmatched row never kept by "(no match)"): lib `m1_a_chart_by_a_lookup_field_equals_the_pivot...`, `m1_a_filter_on_no_match_keeps_exactly_the_unmatched_rows...`, `m1_no_match_and_blank_are_two_separate_filters...`, `m1_no_match_is_a_flag_of_its_own...`, `m1_show_rows_of_the_no_match_and_blank_rows...`; UI `m1_the_value_picker_of_a_number_lookup_column...`.
- unmatched rows taken as "(blank)": the same five lib tests.
- the picker keeping "(no match)" as a value instead of the flag: lib `m1_a_filter_on_no_match_keeps_exactly...`, `m1_no_match_and_blank_are_two_separate_filters...`, `m1_no_match_is_a_flag_of_its_own...`; UI `m1_the_value_picker_of_a_number_lookup_column...`.
- no repeated-key test at the join (Polars' own refusal remains): lib `m1_repeated_keys_at_the_time_of_a_join...`; UI `m1_a_lookup_that_gets_a_repeated_key...`.
- the join not reporting its link: lib `m1_repeated_keys_at_the_time_of_a_join...`.
- "(no match)" listed before "(blank)" in the pivot: `m1_a_pivot_by_a_number_or_a_date_lookup_column...`, `m1_a_pivot_by_a_lookup_field_equals_the_join_tools_pivot...`, `m1_a_numeric_lookup_field_in_values...`; the same swap in the chart's label order: `m1_a_chart_by_a_lookup_field_equals_the_pivot...`.

Note on the UI test of M1-6: the app also re-checks a link when a lookup's file changes (the file's size and time are in the link's fingerprint), so the UI test cannot tell the join's report from that check by itself; the library test proves the report (the queue) and the refusal wording, and the UI test proves the words on the screen and the Broken status without reopening.


## M2 acceptance: item -> test (S5)

Library: `cargo test -q --test phase3 p9_m2` (11 tests); UI: `cargo test -q --bin MicroParquet -- m2_` (5 tests, 3 sizes: 1219x686, 1600x900, 1291x700 pt, light and dark). Deviations: M2-1 to M2-9 (in `docs/phase9-spec.md`).

| Acceptance item (spec M2) | Test |
| --- | --- |
| Calendar rows run from 1 Jan of the earliest year to 31 Dec of the latest; every part equals `dates.rs`; Month end and Days in month right for Feb 2028 (29) and Feb 2026 (28) | `m2_calendar_rows_run_from_1_jan_of_the_earliest_year_to_31_dec_of_the_latest_and_every_part_equals_dates_rs` (1,461 rows, 16 columns in the dialog order, every part of every row against `DatePart::value`, 48 month ends); `m2_the_calendars_years_come_from_the_dates_found...` (year range, refusals, the tab's source compiles to the same table) |
| A pivot of Accounts by "Calendar › Month-Year" equals a pivot by the W2 field "Open Date · Month-Year" made with the same settings, in the same order | library `m2_a_pivot_by_calendar_month_year_equals_a_pivot_by_the_w2_date_field...` (row for row, Nov 2025 ... Jan 2027 in time order, total 3,600); UI `m2_the_calendar_tab_says_who_makes_it...` (the field list's expander, Month-Year to Rows by real clicks: Jan, Feb, Mar 2025 first, not the alphabet) |
| Changing the model's fiscal start to July changes "Calendar › Fiscal year" on two data tabs at once | library `m2_changing_the_fiscal_start_to_july_changes_calendar_fiscal_year_on_two_data_tabs_at_once` (one Date tab and one Date and time tab: FY2026 -> FY2025-26 / FY2026-27 with the right sums); UI (same test as above): the model's setting changed, the Calendar tab rebuilt, Fiscal year reads FY2024-25, FY2025-26, FY2026-27 |
| A Month-end tab is Month-end; a Daily tab is Daily; a Daily tab with single-date months lists them; a date with a quarter of the median rows is listed as incomplete | `m2_snapshot_frequency_is_month_end_for_month_end_dates_and_daily_for_daily_ones_with_the_single_date_months_and_the_incomplete_dates_listed`; `m2_snapshot_dates_are_the_distinct_dates_with_rows_after_the_tabs_steps_and_blank_dates_are_ignored`; on screen in the Calendar dialog's snapshot section |
| The as-of default is the latest date of all date-linked tabs, a picked date round-trips in the project, the differing-ends note appears | `m2_the_as_of_date_is_the_latest_date_of_all_date_linked_tabs...`; `m2_the_model_calendar_and_the_as_of_date_round_trip_and_an_old_model_has_neither`; `m2_the_calendar_tab_is_saved_as_a_recipe_not_rows_and_the_calendar_the_links_and_the_as_of_date_come_back_with_the_project` (version 2 file, `as_of.Date`, the Calendar as a recipe under 20 kB, save-open-save identical); UI `m2_the_as_of_dialog_lists_each_tabs_last_date...` |
| Text date columns are refused | `m2_a_date_link_on_a_text_column_is_refused_with_the_phase_8_message`; UI in the Calendar dialog ("'Branch' is text. Set it to Date in Schema & Ingestion (D/M/Y) first.", nothing linked) |
| A Date and time column links by its day; a blank date is "(blank)", not a match | `m2_a_date_link_joins_a_date_and_time_column_to_the_calendar_by_its_day...` |
| Render tests by real clicks at 3 sizes: Calendar... (turn on, settings, date links), the Calendar tab note, As-of date... | `m2_the_model_menu_lists_calendar_and_as_of_date...` (no project -> the M0 message; with one both open), `m2_the_calendar_dialog_turns_the_calendar_on_changes_its_settings_makes_date_links_and_lists_the_snapshot_dates...` (checkbox, the two settings boxes, the W0 line, links, refusal, snapshot lines, no overlap or clipping, turning off), `m2_the_calendar_tab_says_who_makes_it_takes_no_steps_and_its_fields_serve_a_pivot...` (the note, no steps, fields), `m2_the_as_of_dialog_lists_each_tabs_last_date...` (typing 31/10/2026 and Set date, a bad date refused, Latest data, the status bar "As of 24 Nov 2026 (latest data)" / "As of 31 Oct 2026") |

Pictures looked at by eye (light and dark, 2 px to the point, written by `m2_pictures_of_the_calendar_dialog_the_calendar_tab_and_the_as_of_dialog`): `docs/guide/img/model-calendar-dialog.png` (+ `-dark`), `model-calendar-tab.png` (+ `-dark`), `model-as-of-dialog.png` (+ `-dark`).

Proved to fail without the fix (stubs, each restored, run one at a time, every run finished; failing tests by stub):
- Month end always "No": `m2_calendar_rows_run_from...`. Calendar starting on 2 January: `m2_calendar_rows_run_from...`, `m2_the_calendars_years...`. The calendar ignoring its settings: `m2_calendar_rows_run_from...` and the fiscal-start test.
- Frequency always Daily: `m2_snapshot_frequency...`. A date needing a quarter of the median to be incomplete: `m2_snapshot_frequency...`.
- Latest = the earliest date: `m2_the_as_of_date_is_the_latest_date...`. No differing-ends note: the same library test and UI `m2_the_as_of_dialog_lists...`.
- A date and time column not joined by its day: `m2_changing_the_fiscal_start...` (its second tab) and `m2_a_date_link_joins_a_date_and_time_column...`. Text dates accepted by the snapshot reader: `m2_a_date_link_on_a_text_column...`.
- UI: the Calendar tab taking steps: `m2_the_calendar_tab_says_who_makes_it...`. Calendar fields not in Calendar order: the same test. No "As of" in the status bar: `m2_the_as_of_dialog_lists...`. The Calendar not made again for new years: `m2_the_calendar_dialog_turns_the_calendar_on...`, `m2_the_calendar_tab_says_who_makes_it...`, `m2_the_as_of_dialog_lists...`. A change of settings not applied: `m2_the_calendar_dialog_turns_the_calendar_on...`, `m2_the_calendar_tab_says_who_makes_it...`. A text date column accepted in the dialog: `m2_the_calendar_dialog_turns_the_calendar_on...`.

Found on the way: Polars 0.43 `DataFrame::new` takes `Vec<Series>` (no `Column` type). The docked pivot preview shows 20 lines, so the tab test checks the first three months and the count of 20.


## S6: the owner's decisions on S5, recorded and done

Written under "Owner decisions on S5" in `docs/phase9-spec.md` `<accepted_deviations>`: M1-13, M1-14, M2-1 to M2-9 accepted; the M1-6 UI test relying on the library test for the wording accepted; date links not counted in "Model: n links" and the Calendar and As-of dialogs applying each change at once accepted. **M1-12 replaced** (three labels), and the date-link rows aligned. `<conventions>` now says "(blank key)".

| Decision | Test |
| --- | --- |
| M1-12: a pivot by a text, a number and a date lookup column has three separate rows, (blank) then (blank key) then (no match), with the right sums and counts; the total equals the ungrouped total | `m1_a_pivot_by_a_text_a_number_and_a_date_lookup_column_has_three_separate_rows_blank_blank_key_no_match_in_that_order_and_the_total_is_the_ungrouped_total` (Region, Limit, Opened; 210 / 30 / 57, counts 2 / 2 / 2, total 1,900, 11 rows) |
| M1-12: a filter on each keeps exactly its rows; the three sets don't overlap; the flag is its own field (`FilterCondition.blank_key`, written only when on, old files load with it off) | `m1_a_filter_on_blank_blank_key_and_no_match_keeps_exactly_each_ones_rows_the_three_sets_never_overlap_and_the_flag_is_its_own_field` (A3 A7 / A10 A11 / A6 A8 for a text, a number and a date column; all three together; not in; JSON) |
| M1-12: Show rows from each pivot row matches the cell; chart marks and categories match the pivot | `m1_show_rows_from_each_of_the_three_rows_gives_exactly_that_cells_rows_and_a_chart_shows_the_same_three` |
| M1-12: the picker lists all three by a real click at 3 sizes (the tab's filter) | UI `m1_the_value_picker_lists_blank_blank_key_and_no_match_last_in_that_order_for_a_text_and_a_number_lookup_column_and_a_real_click_on_each_keeps_exactly_its_rows_at_three_sizes` (light and dark; Region: each of the three clicked, its own flag only, Apply keeps 1 / 2 / 8 rows with sums 1,000 / 4,970 / 19,480; Limit: (blank key)) |
| M1-12: "Filters on this chart" lists each one that occurs | UI `m1_filters_on_this_chart_list_blank_blank_key_and_no_match_each_with_its_own_flag_and_the_chart_shows_the_three_light_and_dark_at_three_sizes` (the chart's five categories, the tick list, un-ticking (blank key) then (no match) moves one flag each) |
| Older tests that said "(blank)" for a blank key | updated to "(blank key)" in `tests/phase9/m1.rs` (five assertions) |
| Date-link rows aligned: the same box width on every row | UI `m2_the_date_link_rows_have_the_same_box_width_and_left_edge_at_three_sizes` (four rows, one with a 70-character title; same width and same left edge, inside the dialog; light and dark) |

Proved to fail without the fix (stubs, each restored, run one at a time, every run finished):
- blank keys shown as "(blank)" again: lib `m1_a_pivot_by_a_text_a_number...`, `m1_show_rows_from_each...`, and the five older tests.
- the "(blank key)" flag ignored in the filter: lib `m1_a_filter_on_blank_blank_key_and_no_match...`, `m1_show_rows_from_each...`, `m1_a_filter_on_no_match_keeps_exactly...`.
- "(blank key)" sorted before "(blank)": lib `m1_a_pivot_by_a_text_a_number...`, `m1_show_rows_from_each...`.
- the picker not storing the "(blank key)" flag: lib `m1_a_filter_on_blank_blank_key...`, `m1_a_filter_on_no_match_keeps_exactly...`; UI `m1_filters_on_this_chart_list...`, `m1_the_value_picker_lists...`.
- date-link rows sized by their title (the old code, `horizontal_wrapped`): `m2_the_date_link_rows_have_the_same_box_width_and_left_edge_at_three_sizes` failed with "'Date column: Accounts' starts at 411.43, the first row's box at 407.62" before the fix.

Choices M1-15 to M1-17 are in `<accepted_deviations>`.


## M3 acceptance: item -> test (S6)

Library: `cargo test -q --test phase3 p9_m3` (15 tests, every number worked by hand in the test); UI: `cargo test -q --bin MicroParquet m3_` (4 tests, 3 sizes: 1219x686, 1600x900, 1291x700 pt, light and dark). Deviations: M3-1 to M3-10 (in `docs/phase9-spec.md`).

| Acceptance item (spec M3) | Test |
| --- | --- |
| Daily tab 1 Nov = 100, 3 Nov = 130, as of 3 Nov: Closing 130, Average for Nov to date (100 + 100 + 130) / 3 = 110 | `m3_a_daily_tab_closes_at_the_last_balance_and_the_average_carries_the_balance_over_the_missing_day` (also the Grand total, and Opening blank with its reason) |
| No snapshot on or before 1 Nov: Average blank with the message | `m3_with_no_snapshot_on_or_before_the_start_the_average_is_blank_with_the_message_and_never_zero` ("No balance on or before 1 Nov 2026. Add that date's data or start later."); blank period: `m3_a_period_with_no_snapshot_before_the_as_of_date_is_blank_and_says_when_the_tab_ended` ("No balance in Nov 2026 for 'Accounts' (last date 31 Oct 2026)") |
| Month-end tab 31 Jan 100, 28 Feb 120, 31 Mar 140: Q1 Average 120 headed "Average of month-ends of Balance"; Q1 Closing 140; Q1 Opening blank; Q2 Opening 140 | `m3_a_month_end_tab_averages_its_month_ends_and_says_so_in_the_heading` (M3-3: a 30 Apr row gives Q2) |
| YTD balance income 10, 25, 45: Period amount Jan 10, Feb 15, Mar 20, Q1 45; fiscal start February: Feb 25 | `m3_a_ytd_balance_gives_the_movement_of_each_period_and_a_new_fiscal_year_starts_from_nothing` |
| Flow tab: Total for a month equals the sum of its rows | `m3_a_flow_total_is_the_sum_of_the_rows_dated_in_the_period` (also equals a plain Sum, and the as-of date cuts it) |
| Count at closing counts each account once whatever the number of dates (and Distinct, W.Avg at closing) | `m3_counts_at_closing_count_each_account_once_whatever_the_number_of_snapshot_dates` (3 accounts on 3 dates: 3, not 9; W.Avg 14,000 / 600) |
| Branch x Calendar Month-Year of Average balance: the Total column is the average over the whole span, not the sum of the months; a branch subtotal equals the measure of its own rows | `m3_the_total_column_is_the_average_over_the_whole_span_and_not_the_sum_of_the_months` (B1: Oct 154.84, Nov 310, Total 5730 / 34); `m3_a_subtotal_is_the_measure_of_its_own_rows_and_equals_a_pivot_by_the_parent_field_alone` |
| Stock warning on Sum of a tagged Balance over 23 dates, not on an untagged column; old pivot unchanged | `m3_a_plain_sum_of_a_tagged_balance_over_many_dates_warns_and_an_untagged_column_or_an_old_pivot_does_not` ("Sum of Balance adds 23 dates of balances. Use Closing balance or Average balance."; Count too; a warning, the number is the sum); chart note: `m3_a_chart_of_a_sum_of_a_tagged_balance_over_many_dates_says_so_in_its_notes_and_an_untagged_column_or_closing_does_not` |
| Nested (P2) shows measure subtotals equal to a pivot by the parent field alone | `m3_nested_subtotals_of_a_measure_equal_a_pivot_by_the_parent_field_alone` |
| Suggestions from the column name; old files unchanged | `m3_the_name_of_a_column_suggests_a_tag_and_a_name_that_says_both_or_neither_suggests_nothing`, `m3_old_files_load_unchanged_and_the_new_fields_are_written_only_when_used`; refusals in words: `m3_a_time_aggregation_outside_a_project_or_without_a_date_link_or_on_a_date_part_is_refused_in_words` |
| Render tests by real clicks at 3 sizes: the Columns tags and suggestions | UI `m3_the_measures_dialog_tags_columns_and_applies_a_suggestion_by_a_click_light_and_dark_at_three_sizes` (no tab yet; suggestion click tags; tag box Flow, Not tagged; dialog fits, no overlap, nothing clipped) |
| the aggregation choice in Values | UI `m3_the_aggregation_choice_in_values_lists_the_time_aggregations_on_a_date_linked_tab_and_a_tagged_column_starts_with_its_default_light_and_dark_at_three_sizes` (no time choices on a tab with no date link; a tagged column starts as Closing balance; the 8 time aggregations in the list; Average, back to Sum) |
| the warning banner, a blank cell's reason tooltip | UI `m3_a_pivot_tab_shows_the_stock_warning_and_a_blank_measure_cell_says_why_on_hover_light_and_dark_at_three_sizes` and the dialog's banner in the aggregation test (as of 31 Oct 2026, November is blank, never 0) |

Pictures looked at by eye (light and dark, written by `m3_pictures_of_the_columns_tags_the_aggregation_choice_the_stock_warning_and_a_blank_cell_with_its_reason`): `docs/guide/img/model-measures-columns.png`, `pivot-time-aggregations.png`, `pivot-stock-warning.png`, `pivot-blank-measure-reason.png` (each with `-dark`). `pivot-stock-warning` shows the banner on the pivot tab.

Proved to fail without the fix (stubs, each restored, run one at a time; every run finished):
- average not carried forward over a missing day: `m3_a_daily_tab_closes...`, `m3_nested_subtotals...`, `m3_the_total_column...`.
- Period amount ignoring the fiscal year: `m3_a_ytd_balance...`.
- Count at closing adding every date: `m3_counts_at_closing...`.
- no stock warning: `m3_a_plain_sum_of_a_tagged_balance...`, `m3_a_chart_of_a_sum...`.
- month-end average treated as daily (heading and number): `m3_a_month_end_tab...`.
- a blank measure shown as 0: `m3_a_daily_tab_closes...`, `m3_a_month_end_tab...`, `m3_a_period_with_no_snapshot...`, `m3_with_no_snapshot_on_or_before...`.
- the as-of date not cutting the period: `m3_a_daily_tab_closes...`, `m3_a_flow_total...`, `m3_a_period_with_no_snapshot...`, `m3_counts_at_closing...`, `m3_the_total_column...`.
- a total using every calendar day instead of the shown periods: `m3_a_daily_tab_closes...`, `m3_the_total_column...`.
- UI: no time aggregations in the list: `m3_the_aggregation_choice_in_values...`; a suggestion click doing nothing: `m3_the_measures_dialog_tags_columns...`; no tooltip: `m3_a_pivot_tab_shows_the_stock_warning...` and the pictures test.

Found on the way: the aggregation list used to open below the window's edge at 1219x686 (its last items could not be clicked); it is now as tall as the room under it (M3-10). The Calendar dialog stayed open over a later test's tab until closed; the M3 rig closes it.

## S7: owner decisions on S6 and the M3 follow-up

Owner decisions on S6 (24 Nov 2026), also in `<accepted_deviations>` of the spec ("Owner decisions on S6"): M1-15 to M1-17 and M3-1 to M3-10 accepted except M3-8, which is replaced by Show rows on a time cell as below; Q2 (M3-3, the extra 30 Apr row) accepted; Q1 (M3-7): charts get the time aggregations in M4 through the value type shared with the pivot (done in M4). The lean-session rules are in the spec's `<workflow>` and in CLAUDE.md. Deviations M3-11 to M3-14 (spec).

| Acceptance item (S6 decision, replaces M3-8) | Test |
| --- | --- |
| Closing, Opening and Total: the sum of the rows equals the cell; Count at closing: the count of the rows equals the cell | `m3_show_rows_of_a_closing_opening_total_or_count_cell_gives_the_rows_whose_sum_or_count_is_the_cell` (B1, B2; closing rows are the closing date only, opening rows the opening date only, flow rows dated in November only) |
| Period amount: the rows of the two dates with a Part column; End minus Start equals the cell | `m3_show_rows_of_a_period_amount_gives_the_rows_of_its_two_dates_with_a_part_column_and_end_minus_start_is_the_cell` (Feb 15, Mar 20; Jan has only End) |
| Average balance: on the M3 daily example the rows are 1 Nov and 3 Nov with the note; a carried-in date is added; a blank cell has no rows | `m3_show_rows_of_an_average_gives_the_rows_dated_in_the_period_and_the_date_carried_in_with_a_note_and_a_blank_cell_has_none` |
| Render test by a real click at 3 sizes | UI `m3_show_rows_on_a_time_cell_by_a_real_double_click_gives_the_rows_of_its_closing_date_and_a_blank_cell_gives_its_reason_at_three_sizes` (double-click on the Nov closing cell: 10 rows of 24 Nov, their sum is the cell, no note) and `m3_a_blank_time_cell_has_no_show_rows_and_its_menu_item_gives_the_reason_at_three_sizes` (right-click, the item opens nothing and says why) |

Proved to fail without the fix (two stubs, each restored, run one at a time): Closing rows returning every date of the period: `m3_show_rows_of_a_closing_opening_total_or_count_cell...`; Period amount without its Start date: `m3_show_rows_of_a_period_amount...`.

## M4 acceptance: item -> test (S7)

Library: `cargo test -q --test phase3 p9_m4` (9 tests, hand-worked from a daily tab: Balance = days since 31 Dec 2024, so 24 Nov 2025 = 328, 30 Nov 2025 = 334, 24 Nov 2026 = 693; Interest 2 a day in 2025, 1 a day after). UI: `cargo test -q --bin MicroParquet m4_` (5 tests, 3 sizes, light). Deviations M4-1 to M4-11 (spec).

| Acceptance item (spec M4) | Test |
| --- | --- |
| as of 24 Nov 2026, MTD Total = 1-24 Nov 2026 (24); SPLY Same days = 1-24 Nov 2025 (48); SPLY Full = all of Nov 2025 (60); closing 328 / 334; difference 365; % (693-328)/328 | `m4_mtd_and_sply_use_the_same_days_by_default_and_the_whole_period_when_asked` (also the seven headings) |
| as of 31 Mar 2026, Prior period of MTD with Same days = 1-28 Feb (28 against 31), the same with Full | `m4_prior_period_of_mtd_as_of_31_mar_is_1_to_28_feb_and_a_leap_day_goes_to_28_feb_the_next_year` |
| leap day: as of 29 Feb 2028, SPLY = up to 28 Feb 2027 (29 against 28); 1-30 Mar against 1-28 Feb | same test |
| YTD with fiscal start July as of 15 Jan 2026 covers 1 Jul 2025-15 Jan 2026 (184 x 2 + 15) | `m4_ytd_with_a_july_fiscal_year_as_of_15_jan_covers_1_jul_to_15_jan` (QTD 15; a January year 15) |
| pivot by Month-Year: Closing vs Prior period % = (this - last) / abs(last), January against December, 0 -> blank | `m4_a_pivot_by_month_year_shows_each_months_change_against_the_month_before_and_blank_where_that_is_zero` (Dec 2025, Jan 2026, the cut Nov 2026 against 1-24 Oct, Jan 2025 blank with its reason, month-ends 0 / 10 / 20: Feb blank, Mar 100) |
| Window Period without a period field refused with its message; Calendar > Month refused | `m4_a_period_window_needs_a_period_field_and_a_field_that_mixes_years_is_refused_in_words` (also: MTD needs none, a difference needs a shift, M3 pivots unchanged) |
| periods after the as-of date blank; the as-of override changes MTD | `m4_a_period_after_the_as_of_date_is_blank_and_the_as_of_date_moves_the_mtd_window` (24 / 10 / 31) |
| chart of Closing and of Average balance by Month-Year equals the pivot; chart of Closing . vs SPLY % equals the pivot | `m4_a_chart_of_closing_average_and_closing_vs_sply_percent_by_month_year_equals_the_pivot` (also YTD, Prior period, Difference, Full period); UI: the chart's marks equal the pivot tab's cells (`m4_the_chart_studio_lists...`) |
| files: M3 bytes unchanged, round trip | `m4_the_new_choices_are_written_only_when_used_and_an_m3_file_loads_unchanged`; series: `m4_a_chart_with_a_series_gives_one_line_per_key_and_a_chart_that_mixes_time_and_plain_values_is_refused` |
| render tests by real clicks at 3 sizes: window, shift, show and partial choices in Values | UI `m4_the_time_button_opens_the_window_shift_show_and_partial_choices_and_each_click_sets_the_value_at_three_sizes` (12 choices, inside the window, no overlap, % change greyed with no shift) |
| the refusal message | UI `m4_a_shift_without_a_period_field_shows_the_refusal_in_the_preview_and_a_field_that_mixes_years_is_refused_at_three_sizes` |
| headings at 1219 pt without overlap | UI `m4_a_pivot_by_month_year_of_closing_vs_prior_period_percent_has_its_heading_and_numbers_and_no_overlap_at_three_sizes` |

Not tested by a real click: picking a time aggregation from the chart studio's Aggregation list (the headless click did not open that list in the chart panel; the studio's four rows are asserted to exist and not overlap, and the list is the pivot's pattern). Recorded for the owner to check by eye.

Pictures (light and dark, written by `m4_pictures_...`): `docs/guide/img/pivot-time-choices.png`, `pivot-time-needs-period.png`, `pivot-time-prior-period.png`, `chart-time-sply.png` (each with `-dark`). Found by looking at them: the % columns first showed in Crore (the test helper had no column formats; fixed by passing them) and the chart showed a % divided into Crore (the chart's scale is chosen in the app from the value column; a % change is now never scaled, its tooltip says "%"). In `chart-time-sply` the side panel's own small chart is the chart tab's data, not the M4 chart.

Proved to fail without the fix (two stubs, each restored, run one at a time): Same days ignored (always the full shifted period): `m4_mtd_and_sply...` and `m4_a_pivot_by_month_year...`; a zero earlier value shown as 0 instead of blank: `m4_a_pivot_by_month_year...`.

## S8: owner decisions on S7, the M4 follow-up and M5

Owner decisions on S7 (24 Nov 2026), also in `<accepted_deviations>` of the spec ("Owner decisions on S7"): M3-11 to M3-14 and M4-1 to M4-11 accepted; M4-1 follow-up: the pivot's and the chart's notes state the span of a plain whole-data value ("Closing balance of Balance: whole data, 1 Jan 2025 \u2013 24 Nov 2026 (as of 24 Nov 2026)"); the chart studio's time-aggregation list is checked by the owner by eye (left as recorded).

### M4 follow-up (span note): item -> test

| Acceptance item | Test |
| --- | --- |
| A plain whole-data value says its span in the pivot's notes: "Closing balance of Balance: whole data, 1 Jan 2025 – 24 Nov 2026 (as of 24 Nov 2026)"; the as-of date moves its end; no note with a Calendar period field or with an M4 choice (MTD); the chart's notes carry it | `tests/phase9/m4.rs` `m4_a_plain_value_with_no_calendar_field_says_which_span_it_covers_and_one_with_a_period_field_does_not` |
| Existing tests that expected no note now expect it | `m3_a_plain_sum_of_a_tagged_balance...` (tests/phase9/m3.rs), UI `m3_the_aggregation_choice_in_values_lists...` (a stock warning gone, the span note left) |

Proved to fail without the fix (note push stubbed, restored): `m4_a_plain_value_with_no_calendar_field...` fails (left: [] right: [the note]). Picture `pivot-time-aggregations(-dark).png` rewritten on purpose (it now shows the note); the other pictures a full run rewrote were restored. Full `cargo test --no-fail-fast`: all green except the two tests above, fixed and run alone.
