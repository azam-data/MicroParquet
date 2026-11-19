//! Editing the list of steps (Phase 3 T4): move, delete, duplicate, turn off, insert, and the
//! undo / redo history. All of it works on plain lists, so it is tested without the UI.

use crate::models::{ColumnFormatSpec, SummarySpec, TabSource, TabState, TransformStep};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
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

/// Everything a change to a tab's steps can touch: the steps, the control column, the summary
/// choices and the column formats. Undo and redo restore all of it together.
#[derive(Clone, Debug, Default)]
pub struct TabSnapshot {
    pub steps: Vec<TransformStep>,
    pub control_col: Option<String>,
    pub summary: HashMap<String, SummarySpec>,
    pub col_formats: HashMap<String, ColumnFormatSpec>,
}

impl TabState {
    /// The tab's undoable state as it is now.
    pub fn snapshot(&self) -> TabSnapshot {
        TabSnapshot {
            steps: self.steps.clone(),
            control_col: self.control_col.clone(),
            summary: self.summary.clone(),
            col_formats: self.col_formats.clone(),
        }
    }

    /// Puts back a snapshot. Returns true when the control column changed (its baseline must
    /// then be recomputed).
    pub fn restore(&mut self, s: TabSnapshot) -> bool {
        let ctrl_changed = self.control_col != s.control_col;
        self.steps = s.steps;
        self.control_col = s.control_col;
        self.summary = s.summary;
        self.col_formats = s.col_formats;
        ctrl_changed
    }

    /// Call before an undoable change: keeps the state as it is now.
    pub fn record_undo(&mut self) {
        let s = self.snapshot();
        self.history.record(&s);
    }
}

/// Undo / redo: the states as they were before each change (a tab keeps [`TabSnapshot`]s; the
/// default is a plain list of steps).
#[derive(Clone, Debug)]
pub struct StepHistory<T = Vec<TransformStep>> {
    undo: VecDeque<T>,
    redo: Vec<T>,
}

impl<T> Default for StepHistory<T> {
    fn default() -> Self {
        Self { undo: VecDeque::new(), redo: Vec::new() }
    }
}

impl<T: Clone> StepHistory<T> {
    /// Call before a change, with the state as it is now. A new change ends the redo chain.
    pub fn record(&mut self, before: &T) {
        self.undo.push_back(before.clone());
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

    /// The state to go back to; `current` is kept for redo.
    pub fn undo(&mut self, current: &T) -> Option<T> {
        let prev = self.undo.pop_back()?;
        self.redo.push(current.clone());
        Some(prev)
    }

    /// The state to go forward to; `current` is kept for undo.
    pub fn redo(&mut self, current: &T) -> Option<T> {
        let next = self.redo.pop()?;
        self.undo.push_back(current.clone());
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

/// Every file a source reads: the file, the stacked files, both sides of a join or reconcile.
pub fn source_files(source: &TabSource) -> Vec<&Path> {
    match source {
        TabSource::SingleFile { path, .. } => vec![path.as_path()],
        TabSource::VerticalStack { files, .. } => files.iter().map(|(p, _, _)| p.as_path()).collect(),
        TabSource::JoinedTabs { left_source, right_source, .. } => {
            let mut v = source_files(left_source);
            v.extend(source_files(right_source));
            v
        }
        TabSource::PeriodReconcile { prev_source, curr_source, .. } | TabSource::ClMigration { prev_source, curr_source, .. } | TabSource::MomVariance { prev_source, curr_source, .. } => {
            let mut v = source_files(prev_source);
            v.extend(source_files(curr_source));
            v
        }
        TabSource::GapLadder { inputs, .. } => inputs.iter().flat_map(|i| source_files(&i.source)).collect(),
        TabSource::IrrbbLadder { inputs, .. } | TabSource::Irrbb { inputs, .. } => inputs.iter().flat_map(|i| source_files(&i.source)).collect(),
        TabSource::RepricingGap { source, .. } | TabSource::Cl1Summary { source, .. } | TabSource::Liquidity { source, .. } | TabSource::EclSummary { source, .. } => source_files(source),
    }
}

/// Identity of a source: its settings (paths, ingestion configuration with the column types,
/// join and reconcile settings and their steps) and each file's size and modified time, so a
/// replaced file or a schema change gives another value.
pub fn source_fingerprint(source: &TabSource) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}", source).hash(&mut h);
    for p in source_files(source) {
        match std::fs::metadata(p) {
            Ok(m) => {
                m.len().hash(&mut h);
                m.modified().ok().hash(&mut h);
            }
            Err(_) => "missing".hash(&mut h),
        }
    }
    h.finish()
}

/// A fingerprint of a whole pipeline: the source (see [`source_fingerprint`]) and every step
/// with its settings and on/off state. Background results carry it, so a result for another
/// pipeline (even one with the same number of steps) is never shown.
pub fn pipeline_hash(source: &TabSource, steps: &[TransformStep]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source_fingerprint(source).hash(&mut h);
    for s in steps {
        format!("{:?}", s).hash(&mut h);
    }
    // an FTP step that reads the curve store follows the curve files too
    if steps.iter().any(uses_curve_store) {
        crate::curves::curves_stamp().hash(&mut h);
    }
    // so does a rate shock that reads a beta file
    if steps.iter().any(uses_beta_file) {
        crate::betas::betas_stamp().hash(&mut h);
    }
    h.finish()
}

/// True for a rate-shock step (turned off or not) that reads a beta file.
fn uses_beta_file(step: &TransformStep) -> bool {
    matches!(step.inner(), TransformStep::RateShockSim { beta_file: Some(b), .. } if !b.trim().is_empty())
}

/// True for an FTP step (turned off or not) that takes its curve from the store.
fn uses_curve_store(step: &TransformStep) -> bool {
    matches!(step.inner(), TransformStep::FtpCurveMap { curve_mode, .. } if *curve_mode != crate::models::FtpCurveMode::Inline)
}

/// The keys of the per-step row counts: index 0 is the source (its identity: paths, ingestion
/// settings, file size and modified time), index i + 1 the source plus steps 0..=i. A count is
/// reused only while its key is unchanged, so a replaced file or a new schema recounts everything.
pub fn count_keys(source: &TabSource, steps: &[TransformStep]) -> Vec<u64> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source_fingerprint(source).hash(&mut h);
    let mut keys = Vec::with_capacity(steps.len() + 1);
    keys.push(h.finish());
    for s in steps {
        format!("{:?}", s).hash(&mut h);
        keys.push(h.finish());
    }
    keys
}

/// The counts that can be kept for `new_keys`: a count whose key is the same as when it was
/// counted, `None` (to count) for every other position.
pub fn reusable_counts(old_keys: &[u64], old_counts: &[Option<usize>], new_keys: &[u64]) -> Vec<Option<usize>> {
    new_keys
        .iter()
        .enumerate()
        .map(|(i, k)| if old_keys.get(i) == Some(k) { old_counts.get(i).copied().flatten() } else { None })
        .collect()
}
