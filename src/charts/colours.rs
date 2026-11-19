//! The colour map of a chart (`docs/phase7-spec.md`, "Colour follows the entity, never its rank").
//! The first time a category or series is seen it takes the next free slot of the eight categorical
//! colours, and the slot is stored in the `ChartSpec`. Filtering one out later never repaints the
//! others. A ninth name gets the de-emphasised "Other" colour; so does a series called "Other".

use super::tokens::{self, SLOTS};
use crate::chart::OTHER_LABEL;
use serde::{Deserialize, Serialize};

/// The colour of one name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Colour {
    /// Slot 0..8 of the categorical set (blue, orange, aqua, yellow, magenta, green, violet, red).
    Slot(usize),
    /// The de-emphasised colour.
    Other,
}

impl Colour {
    /// `0xRRGGBB` in the light or the dark set.
    pub fn hex(&self, dark: bool) -> u32 {
        match self {
            Colour::Slot(i) => tokens::series_colour(*i, dark).unwrap_or_else(|| tokens::ink(dark).deemph),
            Colour::Other => tokens::ink(dark).deemph,
        }
    }
}

/// Name to slot, in the order the names were first seen. A stored slot of `SLOTS` or more means "Other".
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct ColourMap {
    pub slots: Vec<(String, usize)>,
}

impl ColourMap {
    /// The colour of a name already seen; `None` for a new name (use `assign`).
    pub fn get(&self, name: &str) -> Option<Colour> {
        if name == OTHER_LABEL {
            return Some(Colour::Other);
        }
        self.slots.iter().find(|(n, _)| n == name).map(|(_, s)| if *s < SLOTS { Colour::Slot(*s) } else { Colour::Other })
    }

    /// The colour of a name; a new name takes the next free slot (and is stored), or "Other" once the eight are taken.
    pub fn assign(&mut self, name: &str) -> Colour {
        if let Some(c) = self.get(name) {
            return c;
        }
        // the next free slot: the lowest one no stored name holds (a slot is never given to a second name)
        let slot = (0..SLOTS).find(|s| !self.slots.iter().any(|(_, u)| u == s)).unwrap_or(SLOTS);
        self.slots.push((name.to_string(), slot));
        if slot < SLOTS {
            Colour::Slot(slot)
        } else {
            Colour::Other
        }
    }

    /// Gives every name a colour, in the order of the list (names already seen keep theirs).
    pub fn assign_all<S: AsRef<str>>(&mut self, names: &[S]) {
        for n in names {
            self.assign(n.as_ref());
        }
    }
}
