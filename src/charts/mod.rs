//! Chart studio (Phase 7): one chart model, three renderers. `spec` describes a chart, `calc` turns a
//! tab into `ChartData` on the worker, `data` holds the table and its transforms (shares, running sums,
//! waterfall steps, heatmap matrices), `colours` the stored colour map and `tokens` the design system.

pub mod calc;
pub mod colours;
pub mod data;
pub mod spec;
pub mod tokens;
