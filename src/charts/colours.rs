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
    /// One of the two greys of older curves: 0 is the darker (more recent), 1 the fainter (older).
    Grey(u8),
}

/// The stored slot numbers of the two greys (a slot of `SLOTS` to 99 is "Other", as before).
const GREY_BASE: usize = 100;

impl Colour {
    /// `0xRRGGBB` in the light or the dark set.
    pub fn hex(&self, dark: bool) -> u32 {
        match self {
            Colour::Slot(i) => tokens::series_colour(*i, dark).unwrap_or_else(|| tokens::ink(dark).deemph),
            Colour::Other => tokens::ink(dark).deemph,
            // the older curves: muted ink, then the de-emphasised grey (nearest the surface in each theme)
            Colour::Grey(0) => tokens::ink(dark).muted,
            Colour::Grey(_) => tokens::ink(dark).deemph,
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
        self.slots.iter().find(|(n, _)| n == name).map(|(_, s)| {
            if *s < SLOTS {
                Colour::Slot(*s)
            } else if *s >= GREY_BASE {
                Colour::Grey((*s - GREY_BASE).min(1) as u8)
            } else {
                Colour::Other
            }
        })
    }

    /// Stores a grey for a name (step 0 or 1); an existing entry is replaced.
    pub fn set_grey(&mut self, name: &str, step: u8) {
        self.slots.retain(|(n, _)| n != name);
        self.slots.push((name.to_string(), GREY_BASE + step.min(1) as usize));
    }

    /// The colour of a name that is not the highlighted one: its grey when it has one, else the common de-emphasised colour.
    pub fn dimmed(&self, name: &str, dark: bool) -> u32 {
        match self.get(name) {
            Some(c @ Colour::Grey(_)) => c.hex(dark),
            _ => tokens::ink(dark).deemph,
        }
    }

    /// Whether the name has one of the greys (an older curve: no markers).
    pub fn is_grey(&self, name: &str) -> bool {
        matches!(self.get(name), Some(Colour::Grey(_)))
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
