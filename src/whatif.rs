//! Phase 12 W4: what-if scenarios. Two assumptions, a **rate shift** (basis points, as the IRRBB shocks are) and a **growth** (percent),
//! change the measures a person has marked, in every place a measure is shown: pivots, charts, KPI cards, dashboards, and the months
//! a forecast adds. A scenario has a name (Base, Optimistic, Stressed, or the person's own); one is active at a time.
//!
//! What a scenario does, and what it never does:
//!
//! - A measure is marked **Volume** (a balance or an income: it is multiplied by 1 + growth/100) or **Rate** (a yield, a cost of funds,
//!   a margin stored in percent: the shift is added, 100 bp = 1 percentage point). An unmarked measure is never touched.
//! - Only the as-of month and later months change. Closed history is always the actual number, so a past figure never moves with a
//!   slider. A window that ends in or after the as-of month (YTD, rolling 12 months) is changed as a whole.
//! - Every output that shows a changed number says so ("Scenario Stressed: rate +200 bp, growth -10% ..."), so a scenario number can
//!   not pass for an actual.
//! - Budget and target (plan) measures are not changed: a plan is the plan.
//! - The rate shift is in bp like `irrbb::shock_bp`. The IRRBB parallel shock in force at the as-of date can be copied into a scenario
//!   (`parallel_shock_bp`), so the size is never typed from memory or kept in the code.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Role {
    /// A balance or an amount: multiplied by 1 + growth/100.
    #[default]
    Volume,
    /// A rate in percent: the rate shift is added (bp / 100 percentage points).
    Rate,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Role::Volume => "Volume (grows with the growth %)",
            Role::Rate => "Rate (moves with the rate shift)",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Scenario {
    pub name: String,
    /// A parallel shift of rates, in basis points (+200 = rates up 2 percentage points).
    pub rate_shift_bp: f64,
    /// Balances and amounts above (+) or below (-) the base, in percent.
    pub growth_pct: f64,
}

impl Eq for Scenario {}

impl Scenario {
    pub fn is_base(&self) -> bool {
        self.rate_shift_bp == 0.0 && self.growth_pct == 0.0
    }
    /// "rate +200 bp, growth -10%"
    pub fn words(&self) -> String {
        let n = |v: f64| {
            let t = format!("{:.2}", v);
            let t = t.trim_end_matches('0').trim_end_matches('.').to_string();
            if v > 0.0 {
                format!("+{}", t)
            } else {
                t.replace('-', "\u{2212}")
            }
        };
        format!("rate {} bp, growth {}%", n(self.rate_shift_bp), n(self.growth_pct))
    }
}

/// A measure and what it is in a scenario.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Marked {
    pub measure: String,
    pub role: Role,
}

/// The what-if part of a project's model. Absent in old projects (every field defaults), and not written while empty.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(default)]
pub struct WhatIf {
    pub scenarios: Vec<Scenario>,
    /// The name of the active scenario; empty = none (the base, no change).
    pub active: String,
    pub marked: Vec<Marked>,
}

impl WhatIf {
    /// The three starting scenarios: no change, a little better, and a stress. The sizes are assumptions to be edited; none is a
    /// regulatory number (the IRRBB shock is copied in from its rule file by a button).
    pub fn starter() -> Vec<Scenario> {
        vec![
            Scenario { name: "Base".into(), rate_shift_bp: 0.0, growth_pct: 0.0 },
            Scenario { name: "Optimistic".into(), rate_shift_bp: 0.0, growth_pct: 5.0 },
            Scenario { name: "Stressed".into(), rate_shift_bp: 0.0, growth_pct: -10.0 },
        ]
    }

    pub fn is_empty(&self) -> bool {
        *self == WhatIf::default()
    }

    pub fn active_scenario(&self) -> Option<&Scenario> {
        if self.active.is_empty() {
            None
        } else {
            self.scenarios.iter().find(|s| s.name == self.active)
        }
    }

    pub fn role_of(&self, measure: &str) -> Option<Role> {
        self.marked.iter().find(|m| m.measure == measure).map(|m| m.role)
    }

    /// The scenario the worker applies: `None` when none is active, or the active one changes nothing, or no measure is marked.
    pub fn scope(&self) -> Option<WhatIfScope> {
        let s = self.active_scenario()?;
        if s.is_base() || self.marked.is_empty() {
            return None;
        }
        Some(WhatIfScope { name: s.name.clone(), rate_shift_bp: s.rate_shift_bp, growth_pct: s.growth_pct, marked: self.marked.clone() })
    }

    /// Why this scenario list can't be used (a repeated name, an empty name, a number that is not a number).
    pub fn check(&self) -> Result<(), String> {
        for (i, s) in self.scenarios.iter().enumerate() {
            if s.name.trim().is_empty() {
                return Err("A scenario has no name. Give each scenario a name.".to_string());
            }
            if self.scenarios[..i].iter().any(|o| o.name.trim().eq_ignore_ascii_case(s.name.trim())) {
                return Err(format!("Two scenarios are called '{}'. Give each its own name.", s.name.trim()));
            }
            if !s.rate_shift_bp.is_finite() || !s.growth_pct.is_finite() {
                return Err(format!("The scenario '{}' has a number that is not a number.", s.name));
            }
            if s.growth_pct <= -100.0 {
                return Err(format!("The scenario '{}' takes growth to -100% or below, which would wipe the balances out.", s.name));
            }
        }
        if !self.active.is_empty() && self.active_scenario().is_none() {
            return Err(format!("The active scenario '{}' is not in the list.", self.active));
        }
        Ok(())
    }

    /// Renames a measure in the marks (the measure was renamed in the model).
    pub fn rename_measure(&mut self, old: &str, new: &str) {
        for m in self.marked.iter_mut().filter(|m| m.measure == old) {
            m.measure = new.to_string();
        }
    }

    /// Drops the mark of a measure that was removed.
    pub fn forget_measure(&mut self, name: &str) {
        self.marked.retain(|m| m.measure != name);
    }
}

/// The scenario as a worker applies it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct WhatIfScope {
    pub name: String,
    pub rate_shift_bp: f64,
    pub growth_pct: f64,
    pub marked: Vec<Marked>,
}

impl WhatIfScope {
    pub fn role_of(&self, measure: &str) -> Option<Role> {
        self.marked.iter().find(|m| m.measure == measure).map(|m| m.role)
    }

    /// The number a measure shows in the scenario. `value` is the stored number (BDT, or percent for a rate).
    pub fn apply(&self, role: Role, value: f64) -> f64 {
        match role {
            Role::Volume => value * (1.0 + self.growth_pct / 100.0),
            Role::Rate => value + self.rate_shift_bp / 100.0,
        }
    }

    /// The words every output carries beside a changed number.
    pub fn note(&self, measures: &[&str]) -> String {
        format!("Scenario {}: {} on {}, from the as-of month on. These are not actual figures.", self.name, Scenario { name: String::new(), rate_shift_bp: self.rate_shift_bp, growth_pct: self.growth_pct }.words(), measures.join(", "))
    }
}

/// Why a measure can't take this role: a Volume is a base measure, a Rate is a ratio shown in percent, and a plan measure is neither.
pub fn check_role(m: &crate::measures::NamedMeasure, role: Role) -> Result<(), String> {
    use crate::measures::{NamedMeasure, RatioFormat};
    match (m, role) {
        (NamedMeasure::Base { .. }, Role::Volume) => Ok(()),
        (NamedMeasure::Base { name, .. }, Role::Rate) => Err(format!("'{}' is an amount, not a rate. Mark it Volume, or mark a ratio such as a yield or cost of funds as the Rate.", name)),
        (NamedMeasure::Ratio { format, .. }, Role::Rate) if *format == RatioFormat::Percent => Ok(()),
        (NamedMeasure::Ratio { name, .. }, Role::Rate) => Err(format!("'{}' is not shown in percent, so a rate shift in percentage points would not make sense. Show it in percent first.", name)),
        (NamedMeasure::Ratio { name, .. }, Role::Volume) => Err(format!("'{}' is a ratio of two amounts; growth that scales both leaves it where it is. Mark the amounts, or mark the ratio as a Rate.", name)),
        (NamedMeasure::Plan { name, .. }, _) => Err(format!("'{}' is a budget or target. A plan is the plan and is not changed by a scenario.", name)),
    }
}

/// The IRRBB parallel shock in basis points of the rule file in force at `as_of` for the currency (a regulatory size, read from the
/// data file, never kept here).
pub fn parallel_shock_bp(as_of: chrono::NaiveDate, currency: &str) -> anyhow::Result<f64> {
    let f = crate::rules::load_shocks(as_of)?;
    let c = f.data.for_currency(currency, &f.meta.file)?;
    Ok(c.parallel_bp)
}
