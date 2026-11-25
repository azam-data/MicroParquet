//! KPI cards (Phase 10 D2). D0 holds only the card's saved shape so a dashboard can carry one; D2 fills in the unit, the
//! comparison, the thresholds, the sparkline and the computation.

use serde::{Deserialize, Serialize};

/// A KPI card's saved settings. Every field is `#[serde(default)]`, so a card saved by D0 loads in D2.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct KpiSpec {
    pub title: String,
}
