# Phase 7 progress (chart studio)

Branch `phase7`. Tasks done in this session: C0, C1, C2 (in that order). Resume from this file.

| Task | Status | Commit | Tests |
| --- | --- | --- | --- |
| C0 | DONE | (see git log: "Phase 7 C0") | tests/phase7/c0.rs ×20; render_tests_p7 ×1 (theme tokens) |
| C1 | pending | | |
| C2 | pending | | |

## Deviations (also in `docs/phase7-spec.md` `<accepted_deviations>`)

- C0-1: the palette and size literals live in `src/charts/tokens.rs` (library crate), not in `src/ui/theme.rs`: the HTML and Excel renderers are in the library and cannot read the binary's theme. `theme.rs` exposes them as `Color32`/`f32` tokens (`CHART_*`), and both the library test (`c0_every_hex_value…`) and the UI test pin every hex value.
- C0-2: the Phase 6 types (`ChartSpec`, `ChartAgg`, `ChartKind`, `ChartScale`, `ChartData`) moved to `src/charts/` and are re-exported from `src/chart.rs`, so Phase 6 code and tests compile unchanged. `ChartKind::Bar` stays the name of the vertical column chart (old settings say "Bar"); horizontal bars are `horizontal: true`.
- C0-3: a percentage of a zero total is blank with a note, not 0; a waterfall whose end does not equal start plus steps is refused with a message (a bridge that does not add up is a wrong number).
- C0-4: more than 12 facet values is refused (spec says "up to 12"), asking the user to filter first, rather than silently dropping panels.
