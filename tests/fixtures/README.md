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
