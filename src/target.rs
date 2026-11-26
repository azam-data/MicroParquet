//! Phase 12 T1: targets and their red/amber/green bands, shared by KPI cards and charts.
//!
//! A target is one number. Whether a result is green, amber or red depends on which way is good and on how far the result may be
//! from the target before it matters:
//!
//! - **Up is good** (income, deposits): green at or above the target less `green_within_pct` of it, amber at or above the target less
//!   `amber_within_pct` of it, red below.
//! - **Down is good** (cost of funds, NPL): green at or below the target plus `green_within_pct` of it, amber up to the target plus
//!   `amber_within_pct`, red above.
//!
//! The percentages are taken of the size of the target (`|target|`), so a negative target (a planned loss) works the same way.
//! "No judgement" has no good side and so no bands: asking for them is an error, not a guess.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Rag {
    #[default]
    Green,
    Amber,
    Red,
}

impl Rag {
    pub fn word(self) -> &'static str {
        match self {
            Rag::Green => "green",
            Rag::Amber => "amber",
            Rag::Red => "red",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Direction {
    /// A higher result is better.
    #[default]
    Up,
    /// A lower result is better.
    Down,
}

/// How far from the target a result may be (in percent of the target) and still be green, then amber.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Bands {
    pub green_within_pct: f64,
    pub amber_within_pct: f64,
}

impl Default for Bands {
    fn default() -> Self {
        Bands { green_within_pct: 0.0, amber_within_pct: 10.0 }
    }
}

impl Bands {
    /// Refuses a band that is negative or an amber zone narrower than the green one.
    pub fn check(&self) -> Result<(), String> {
        if !(self.green_within_pct.is_finite() && self.amber_within_pct.is_finite()) || self.green_within_pct < 0.0 || self.amber_within_pct < 0.0 {
            return Err("The green and amber margins are percentages of the target and can't be negative.".to_string());
        }
        if self.amber_within_pct < self.green_within_pct {
            return Err("The amber margin is smaller than the green one. Make amber at least as wide as green.".to_string());
        }
        Ok(())
    }
}

/// The two limits of the bands for a target: the result is green on the good side of `green_edge`, amber between the edges, red
/// beyond `amber_edge`.
pub fn edges(target: f64, dir: Direction, b: &Bands) -> (f64, f64) {
    let size = target.abs();
    match dir {
        Direction::Up => (target - size * b.green_within_pct / 100.0, target - size * b.amber_within_pct / 100.0),
        Direction::Down => (target + size * b.green_within_pct / 100.0, target + size * b.amber_within_pct / 100.0),
    }
}

/// The colour of a result against its target.
pub fn status(value: f64, target: f64, dir: Direction, b: &Bands) -> Rag {
    let (g, a) = edges(target, dir, b);
    match dir {
        Direction::Up => {
            if value >= g {
                Rag::Green
            } else if value >= a {
                Rag::Amber
            } else {
                Rag::Red
            }
        }
        Direction::Down => {
            if value <= g {
                Rag::Green
            } else if value <= a {
                Rag::Amber
            } else {
                Rag::Red
            }
        }
    }
}

/// The three bands as (from, to, colour) over the axis `lo..hi`, worst first from the bad end; zero-width bands are left out.
pub fn bands_over(lo: f64, hi: f64, target: f64, dir: Direction, b: &Bands) -> Vec<(f64, f64, Rag)> {
    let (g, a) = edges(target, dir, b);
    let raw: Vec<(f64, f64, Rag)> = match dir {
        Direction::Up => vec![(lo, a, Rag::Red), (a, g, Rag::Amber), (g, hi, Rag::Green)],
        Direction::Down => vec![(lo, g, Rag::Green), (g, a, Rag::Amber), (a, hi, Rag::Red)],
    };
    raw.into_iter()
        .map(|(f, t, c)| (f.clamp(lo, hi), t.clamp(lo, hi), c))
        .filter(|(f, t, _)| t - f > 1e-12 * (hi - lo).abs().max(1.0))
        .collect()
}

/// "97.2% of target" (the result over the target in percent); `None` for a target of 0.
pub fn percent_of_target(value: f64, target: f64) -> Option<f64> {
    (target != 0.0).then(|| value / target * 100.0)
}
