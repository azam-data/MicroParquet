# Old-macro fixtures

Made by the code of the Phase 1 merge commit `c6e780d` (PR #2) and the Phase 2 merge commit
`4278ef5` (PR #4), each checked out in a temporary git worktree, with a throw-away test that
built a `MacroRecipe` holding one step of every type that version had (15 types), saved it
with that version's `save_macro_to_file`, replayed it on the synthetic loan book
(`fixture_frame` in `tests/phase3/fixes.rs`) and wrote the result as CSV:

- `phase1_every_step.mpq.json`, `phase1_every_step.expected.csv`
- `phase2_every_step.mpq.json`, `phase2_every_step.expected.csv`

`fix13_genuine_old_macros_load_and_replay_to_the_results_their_code_gave` loads each file with
today's code and checks that it replays to the same CSV. Do not edit these files by hand.

## Phase 4 fixtures

Made the same way from the code of `main` at `c01b78b` (the end of Phase 3), in a temporary worktree
that shared this repository's build cache, with a throw-away test that saved a `MacroRecipe`
with that code's `save_macro_to_file` and wrote the result of replaying it on the same synthetic
loan book:

- `phase4_pivot_main.*`: a filter and a `GroupByPivot` step (Sum, Count, Mean, WeightedAvg, PctOfTotal), before the pivot builder (V1).
- `phase4_rate_shock_main.*`: a `RateShockSim` step without a side column, before V5.
- `phase4_rate_shock_side_main.*`: a `RateShockSim` step with a side column and a −200 bp shock, before V5.

`v1_an_old_group_by_pivot_macro_replays_to_what_main_gave` and the V5 tests replay them.
Do not edit these files by hand.

## Phase 6 fixture

`phase6_sort_pivot_unset.mpq.json` holds a Sort step and a Pivot table step in the form `main`
(9aadb59, Phase 5) saved them, without the Phase 6 `order` and `field_orders` fields. Those fields did
not exist then, so the file was written by hand in that form (not by a worktree of the old code);
`phase6_sort_pivot_unset.expected.csv` is the Phase 3 and 4 result worked out from the data: the rows
in natural branch order with each branch's balance (CTG 173,000,000, DHK 375,000,000, SYL 0, XXX 1) and
the grand total.

## Phase 7 baseline (Phase 8 goal 3)

`phase7_replay/` was made by the code of `main` at `f3e6f91` (the end of Phase 7), in a temporary worktree that
shared this repository's build cache, with a throw-away test that used `tests/phase8/p7_baseline.rs` (Phase 7 APIs
only): three macros saved with `save_macro_to_file`, a pack saved with `save_pack`, a chart saved with `save_chart`,
and `phase7_replay.hashes.json`, the blake3 of what that code gave for them and for the seven older macro fixtures
above. `w0_the_phase7_samples_replay_to_the_hashes_the_phase7_code_gave` replays them. Do not edit these files by hand.
