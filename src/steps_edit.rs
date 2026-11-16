//! Editing the list of steps (Phase 3 T4): move, delete, duplicate, turn off, insert, and the
//! undo / redo history. All of it works on plain lists, so it is tested without the UI.

use crate::models::TransformStep;
use std::collections::VecDeque;

/// How many changes can be undone.
pub const HISTORY_LEVELS: usize = 100;

/// Moves the step at `from` to position `to` (the other steps shift). False when either index
/// is out of range or nothing moves.
pub fn move_step(steps: &mut Vec<TransformStep>, from: usize, to: usize) -> bool {
    if from >= steps.len() || to >= steps.len() || from == to {
        return false;
    }
    let s = steps.remove(from);
    steps.insert(to, s);
    true
}

/// One place earlier; returns the new index.
pub fn move_up(steps: &mut Vec<TransformStep>, i: usize) -> Option<usize> {
    if i == 0 || !move_step(steps, i, i - 1) {
        return None;
    }
    Some(i - 1)
}

/// One place later; returns the new index.
pub fn move_down(steps: &mut Vec<TransformStep>, i: usize) -> Option<usize> {
    if i + 1 >= steps.len() || !move_step(steps, i, i + 1) {
        return None;
    }
    Some(i + 1)
}

/// Removes step `i` and returns it.
pub fn delete_step(steps: &mut Vec<TransformStep>, i: usize) -> Option<TransformStep> {
    if i < steps.len() {
        Some(steps.remove(i))
    } else {
        None
    }
}

/// Copies step `i` right after itself; returns the copy's index.
pub fn duplicate_step(steps: &mut Vec<TransformStep>, i: usize) -> Option<usize> {
    let copy = steps.get(i)?.clone();
    steps.insert(i + 1, copy);
    Some(i + 1)
}

/// Turns step `i` off (it stays in the list but changes nothing) or back on. Returns whether it
/// is off now.
pub fn toggle_skip(steps: &mut [TransformStep], i: usize) -> Option<bool> {
    let slot = steps.get_mut(i)?;
    let current = std::mem::replace(slot, TransformStep::RenameColumn { old_name: String::new(), new_name: String::new() });
    let (next, off) = match current {
        TransformStep::Skipped { step } => (*step, false),
        other => (TransformStep::Skipped { step: Box::new(other) }, true),
    };
    *slot = next;
    Some(off)
}

/// Inserts `step` after position `after` (`None` = at the end); returns its index.
pub fn insert_after(steps: &mut Vec<TransformStep>, after: Option<usize>, step: TransformStep) -> usize {
    let at = match after {
        Some(i) if i < steps.len() => i + 1,
        _ => steps.len(),
    };
    steps.insert(at, step);
    at
}

/// Undo / redo of step-list changes: the lists as they were before each change.
#[derive(Clone, Debug, Default)]
pub struct StepHistory {
    undo: VecDeque<Vec<TransformStep>>,
    redo: Vec<Vec<TransformStep>>,
}

impl StepHistory {
    /// Call before a change, with the list as it is now. A new change ends the redo chain.
    pub fn record(&mut self, before: &[TransformStep]) {
        self.undo.push_back(before.to_vec());
        while self.undo.len() > HISTORY_LEVELS {
            self.undo.pop_front();
        }
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The list to go back to; `current` is kept for redo.
    pub fn undo(&mut self, current: &[TransformStep]) -> Option<Vec<TransformStep>> {
        let prev = self.undo.pop_back()?;
        self.redo.push(current.to_vec());
        Some(prev)
    }

    /// The list to go forward to; `current` is kept for undo.
    pub fn redo(&mut self, current: &[TransformStep]) -> Option<Vec<TransformStep>> {
        let next = self.redo.pop()?;
        self.undo.push_back(current.to_vec());
        Some(next)
    }

    pub fn undo_levels(&self) -> usize {
        self.undo.len()
    }
}

/// A fingerprint of the steps up to and including each position, so row counts stay valid as long
/// as the steps before them do not change.
pub fn prefix_hashes(steps: &[TransformStep]) -> Vec<u64> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    steps
        .iter()
        .map(|s| {
            format!("{:?}", s).hash(&mut hasher);
            hasher.finish()
        })
        .collect()
}
