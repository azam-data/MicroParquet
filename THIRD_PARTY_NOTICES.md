# Third-party notices

MicroParquet bundles the following third-party assets. The font licence texts are in `assets/fonts/`, one file per family; each font is also compiled into the program.

| Component | Used for | Licence | Where |
|---|---|---|---|
| JetBrains Mono (Regular, SemiBold) | Numbers, dates and number-like codes in cells, totals, counts | SIL Open Font License 1.1 | Compiled into the program (`include_bytes!`) from `assets/fonts/JetBrainsMono-*.ttf`; licence text `assets/fonts/OFL-JetBrainsMono.txt` |
| Source Sans 3 (Regular, SemiBold, Bold) | UI text and table headings | SIL Open Font License 1.1 | Compiled into the program (`include_bytes!`) from `assets/fonts/SourceSans3-*.ttf`; licence text `assets/fonts/OFL-SourceSans3.md` (the one copy; Copyright 2010-2024 Adobe, Reserved Font Name "Source"). No font file is read from disk at run time. |
| Phosphor Icons (Regular) via the `egui-phosphor` crate 0.7 | Toolbar, button and status icons | MIT (icons) / MIT or Apache-2.0 (crate) | Compiled into the program |
| egui, eframe, egui_extras, egui_plot 0.29 | User interface | MIT or Apache-2.0 | Compiled into the program |
| Polars 0.43 | Data engine | MIT | Compiled into the program |

Other Rust crates are listed in `Cargo.lock`; each is licensed under its own terms.
