# Third-party notices

MicroParquet bundles the following third-party assets. Their licence texts are in `assets/fonts/`.

| Component | Used for | Licence | Where |
|---|---|---|---|
| JetBrains Mono (Regular, SemiBold) | Numbers, IDs, dates in cells, totals, counts | SIL Open Font License 1.1 | `assets/fonts/JetBrainsMono-*.ttf`, `assets/fonts/OFL-JetBrainsMono.txt` |
| Source Sans 3 (Regular, SemiBold, Bold) | UI text | SIL Open Font License 1.1 | Licence text: `assets/fonts/OFL-SourceSans3.md`. **The TTF files are not in this repository yet**: copy `SourceSans3-Regular.ttf`, `SourceSans3-Semibold.ttf` and `SourceSans3-Bold.ttf` from https://github.com/adobe-fonts/source-sans (release/TTF) into `assets/fonts/` next to the program. Until then the UI uses egui's default text font. |
| Phosphor Icons (Regular) via the `egui-phosphor` crate 0.7 | Toolbar, button and status icons | MIT (icons) / MIT or Apache-2.0 (crate) | Compiled into the program |
| egui, eframe, egui_extras, egui_plot 0.29 | User interface | MIT or Apache-2.0 | Compiled into the program |
| Polars 0.43 | Data engine | MIT | Compiled into the program |

Other Rust crates are listed in `Cargo.lock`; each is licensed under its own terms.
