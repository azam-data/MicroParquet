# Third-party notices

MicroParquet bundles the following third-party assets. The font licence texts are in `assets/fonts/`, one file per family; each font is also compiled into the program.

| Component | Used for | Licence | Where |
|---|---|---|---|
| JetBrains Mono (Regular, SemiBold) | Numbers, dates and number-like codes in cells, totals, counts | SIL Open Font License 1.1 | Compiled into the program (`include_bytes!`) from `assets/fonts/JetBrainsMono-*.ttf`; licence text `assets/fonts/OFL-JetBrainsMono.txt` |
| Source Sans 3 (Regular, SemiBold, Bold) | UI text and table headings | SIL Open Font License 1.1 | Compiled into the program (`include_bytes!`) from `assets/fonts/SourceSans3-*.ttf`; licence text `assets/fonts/OFL-SourceSans3.md` (the one copy; Copyright 2010-2024 Adobe, Reserved Font Name "Source"). No font file is read from disk at run time. |
| Phosphor Icons (Regular) via the `egui-phosphor` crate 0.7 | Toolbar, button and status icons | MIT (icons) / MIT or Apache-2.0 (crate) | Compiled into the program |
| egui, eframe, egui_extras, egui_plot 0.29 | User interface | MIT or Apache-2.0 | Compiled into the program |
| Polars 0.43 | Data engine | MIT | Compiled into the program |
| Apache ECharts 6.1.0 (`echarts.min.js`, from the npm package `echarts@6.1.0`; SHA-256 `b66b25aeb4df84e33199dc21694014d336d222cbd9deb0e5a7c14bd6aa0d0fd0`) | Charts in the HTML export and chart books (Phase 7) | Apache-2.0 (the d3 parts it contains: BSD-3-Clause, `LICENSE-d3`) | Compiled into the program (`include_str!`) from `assets/vendor/echarts/echarts.min.js` and written into each exported HTML file; `LICENSE`, `NOTICE` and `LICENSE-d3` are beside it. HTML files never load it, or anything else, from a URL. |

Other Rust crates are listed in `Cargo.lock`; each is licensed under its own terms.
