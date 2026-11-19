//! Chart studio (Phase 7): one chart model, three renderers. `spec` describes a chart, `calc` turns a
//! tab into `ChartData` on the worker, `data` holds the table and its transforms (shares, running sums,
//! waterfall steps, heatmap matrices), `colours` the stored colour map and `tokens` the design system.

pub mod axis;
pub mod calc;
pub mod colours;
pub mod data;
pub mod gallery;
pub mod layout;
pub mod library;
pub mod spec;
pub mod suggest;
pub mod thumbnail;
pub mod tokens;
