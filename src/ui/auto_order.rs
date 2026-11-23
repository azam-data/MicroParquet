//! Auto order (Phase 8 W0): when a field is *added* to a pivot's Rows or Columns, a chart's category or a
//! sort level, its order defaults to
//! 1. the column's order declared by the steps (`engine::column_orders`), else
//! 2. Calendar when every non-blank label parses, else
//! 3. Ranges when every non-blank label parses, else
//! 4. today's default (Natural).
//!
//! 1 is known at once; 2 and 3 need the labels, which the worker lists. The choice is stored in the spec,
//! and only a field whose order is still Natural (nothing chosen yet) is given one: existing specs are
//! never re-ordered.

use crate::ui::prelude::*;
use micro_parquet::sort_lists::{detect_order, SortOrder};

/// Where the order goes when it is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutoTarget {
    /// A field of the pivot builder's Rows or Columns.
    Pivot,
    /// The chart's category.
    ChartCategory,
    /// Level `n` of the sort dialog.
    SortLevel(usize),
}

/// A request for a field's labels that is on the worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoOrderWait {
    pub tab_id: u64,
    pub field: String,
    pub target: AutoTarget,
}

impl MicroParquetApp {
    /// The order the active tab's steps declare for a column (Split date parts, unnumbered rate bands).
    pub(crate) fn declared_order(&self, field: &str) -> Option<SortOrder> {
        let tab = self.tabs.get(self.active_tab)?;
        micro_parquet::engine::column_orders(&tab.steps).remove(field)
    }

    /// A field was added to `target`: its order from the steps now, or from its labels when the worker
    /// has listed them. Returns the order known at once.
    pub(crate) fn auto_order(&mut self, field: &str, target: AutoTarget) -> Option<SortOrder> {
        if let Some(o) = self.declared_order(field) {
            return Some(o);
        }
        let tab = self.tabs.get(self.active_tab)?;
        let req = PivotValuesRequest { tab_id: tab.id, source: tab.source.clone(), steps: tab.steps.clone(), field: field.to_string(), temp_dir: self.paths.temp_dir.clone() };
        self.auto_order_waits.retain(|w| !(w.target == target && w.field == field));
        self.auto_order_waits.push(AutoOrderWait { tab_id: tab.id, field: field.to_string(), target: target.clone() });
        // one job per tab and field: two fields added one after the other are both read
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&(tab.id, field), &mut h);
        let scope = std::hash::Hasher::finish(&h);
        let key = JobKey::tab(JobKind::AutoOrder, scope);
        self.jobs.submit(key, "Values of a field", false, move |ctx| compute_pivot_values(req, ctx).map(|r| JobOutput::AutoOrder(Box::new(r))));
        None
    }

    /// The worker listed a field's labels: the order goes where it was asked for, if the field is still
    /// there with no order chosen.
    pub(crate) fn apply_auto_order(&mut self, r: &micro_parquet::tab_jobs::PivotValuesResult) {
        let waits: Vec<AutoOrderWait> = self.auto_order_waits.iter().filter(|w| w.tab_id == r.tab_id && w.field == r.field).cloned().collect();
        self.auto_order_waits.retain(|w| !(w.tab_id == r.tab_id && w.field == r.field));
        let Ok((values, _)) = &r.values else {
            return;
        };
        let cal = self.calendar();
        let Some(order) = detect_order(values.iter().flatten().map(|v| v.as_str()), &cal) else {
            return;
        };
        for w in waits {
            self.put_auto_order(&w, order.clone());
        }
    }

    fn put_auto_order(&mut self, w: &AutoOrderWait, order: SortOrder) {
        match w.target {
            AutoTarget::Pivot => {
                let spec = &mut self.pivot_builder.spec;
                let in_spec = spec.rows.contains(&w.field) || spec.columns.contains(&w.field);
                if self.pivot_builder.open && in_spec && !spec.field_orders.iter().any(|o| o.field == w.field) {
                    spec.field_orders.push(PivotFieldOrder { field: w.field.clone(), order });
                }
            }
            AutoTarget::ChartCategory => {
                let s = &self.chart.spec;
                if s.category == w.field && s.order.is_natural() && !s.by_value {
                    let mut spec = s.clone();
                    spec.order = order;
                    self.chart_set_spec(spec);
                }
            }
            AutoTarget::SortLevel(n) => {
                if let Some(l) = self.sort.levels.get_mut(n) {
                    if l.column == w.field && l.order.is_natural() {
                        l.order = order;
                    }
                }
            }
        }
    }

    /// The Calendar and Ranges entries of an order menu, after Tenor.
    pub(crate) fn builtin_orders(&self) -> [SortOrder; 2] {
        [SortOrder::calendar(&self.calendar()), SortOrder::Ranges]
    }
}
