//! Background job queue (Phase 3 R2).
//!
//! One worker thread runs all Polars work; the UI submits jobs and polls the results each frame.
//! The queue knows nothing about egui or Polars: the UI passes a "wake" callback that requests a
//! repaint when a result is ready.
//!
//! * Every job has an id. A job submitted with a [`JobKey`] that has a scope (for example a tab)
//!   supersedes an older job with the same key: the old one is cancelled and its result is dropped.
//! * Long jobs report progress through their [`JobCtx`] and can be cancelled. Cancelling stops at
//!   the job's next safe point; output files are written with [`TempOutput`], so a cancelled or
//!   failed job leaves no partial file.
//! * An error in a job reaches the UI as text ([`JobResult::status_line`]).
//! * [`JobQueue::inline`] runs jobs on the calling thread (headless mode) with the same code.

use anyhow::{Context, Result};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// What a job does. Jobs of the same kind for the same scope supersede each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JobKind {
    /// Opens a file or builds a new tab.
    Load,
    /// Page, control totals and notes of one tab.
    Refresh,
    /// Column statistics for the inspector.
    Stats,
    /// The pre-export check for Text columns that look like numbers. Its own kind, so an
    /// inspector failure can never stop an export (or say the wrong reason).
    TextCheck,
    /// The summary row of a tab.
    Summary,
    /// Row counts after each step.
    StepCounts,
    /// The live preview of the pivot builder (and the field types it lists).
    PivotPreview,
    /// The values of one field for the pivot builder's Filters checklist.
    PivotValues,
    /// Writes an output file or folder.
    Export,
    /// Folder batch conversion.
    Batch,
    /// Applies a schema to an open tab.
    Schema,
    /// The values of one column with their counts, for the filter's value picker.
    PickerValues,
    /// The numbers of the chart (the whole tab, after its steps).
    Chart,
    /// Checks a SQL query of the SQL tab against the tab's columns before it becomes a step.
    SqlCheck,
    /// The data-quality checks of a tab (Phase 6 P4).
    Dq,
    /// The labels of a field just added to a pivot, chart or sort, for its auto order (Phase 8 W0).
    AutoOrder,
    /// One tab of a project being opened (Phase 8 W4).
    ProjectTab,
    /// The files of a project compared with the hashes it saved (Phase 8 W4).
    ProjectCheck,
    /// Builds the project from the open tabs, hashing the files (Phase 8 W4 follow-up).
    ProjectSave,
    /// A linked chart tab calculated again from its source tab (Phase 8 W5).
    ChartTab,
    /// The rows behind a cell of a pivot table, found by building the pivot again (Phase 8 W6).
    ShowRows,
    /// The values of a field of the chart studio's "Filters on this chart" tick lists (Phase 8 W10).
    ChartPickerValues,
    /// The Nested view of a pivot tab, built from the pivot's source rows (Phase 9 P2).
    NestedView,
}

/// Identifies "the same job again". `scope` is usually a tab id; `None` means the job is never
/// superseded (two exports can run one after the other).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct JobKey {
    pub kind: JobKind,
    pub scope: Option<u64>,
}

impl JobKey {
    pub fn tab(kind: JobKind, tab_id: u64) -> Self {
        Self { kind, scope: Some(tab_id) }
    }
    pub fn unique(kind: JobKind) -> Self {
        Self { kind, scope: None }
    }
}

pub type JobId = u64;

/// Returned (inside an `anyhow::Error`) by a job that stopped because it was cancelled.
#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cancelled")
    }
}
impl std::error::Error for Cancelled {}

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub text: String,
    /// 0.0..=1.0 when the job knows how far it is.
    pub fraction: Option<f32>,
    /// A job with named stages ("Reading", "Parsing", "Checking"): their names and the current one.
    pub stages: Vec<String>,
    pub stage: usize,
    /// Set while the job reads a file's bytes itself: (bytes read, file size). The fraction of
    /// such a step is exactly this ratio.
    pub bytes: Option<(u64, u64)>,
    /// When the job started running.
    pub started: Option<std::time::Instant>,
}

/// Handed to a running job: cancellation flag and progress line.
#[derive(Clone)]
pub struct JobCtx {
    id: JobId,
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<Progress>>,
}

impl JobCtx {
    /// A context that is never cancelled and whose progress goes nowhere (headless, tests, and the
    /// plain functions that wrap the `_ctx` ones).
    pub fn inert() -> Self {
        Self {
            id: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(Mutex::new(Progress::default())),
        }
    }
    pub fn id(&self) -> JobId {
        self.id
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    /// A safe point: returns `Err(Cancelled)` when the user asked to stop.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(Cancelled.into())
        } else {
            Ok(())
        }
    }
    pub fn set_progress(&self, text: impl Into<String>, fraction: Option<f32>) {
        if let Ok(mut p) = self.progress.lock() {
            p.text = text.into();
            p.fraction = fraction;
            p.bytes = None;
        }
    }
    /// Names the stages of this job; the first one is current. No percentage is shown for a
    /// stage unless a loop reports the bytes it has read ([`JobCtx::set_bytes`]).
    pub fn set_stages(&self, names: &[&str]) {
        if let Ok(mut p) = self.progress.lock() {
            p.stages = names.iter().map(|s| s.to_string()).collect();
            p.stage = 0;
            p.text = names.first().map(|s| s.to_string()).unwrap_or_default();
            p.fraction = None;
            p.bytes = None;
        }
    }
    /// Moves to stage `index` (0-based).
    pub fn set_stage(&self, index: usize) {
        if let Ok(mut p) = self.progress.lock() {
            p.stage = index.min(p.stages.len().saturating_sub(1));
            p.text = p.stages.get(p.stage).cloned().unwrap_or_default();
            p.fraction = None;
            p.bytes = None;
        }
    }
    /// A loop that reads the file itself reports `read` of `total` bytes: this is the only way
    /// a real percentage appears. `total == 0` says the loop is finished; the percentage goes.
    pub fn set_bytes(&self, what: &str, read: u64, total: u64) {
        if let Ok(mut p) = self.progress.lock() {
            if total == 0 {
                p.fraction = None;
                p.bytes = None;
                p.text = p.stages.get(p.stage).cloned().unwrap_or_default();
            } else {
                p.fraction = Some((read as f64 / total as f64).clamp(0.0, 1.0) as f32);
                p.bytes = Some((read.min(total), total));
                p.text = what.to_string();
            }
        }
    }
    /// What the job reports now.
    pub fn progress(&self) -> Progress {
        self.progress.lock().map(|p| p.clone()).unwrap_or_default()
    }
    /// Remembers when the job started running (the queue calls this).
    fn mark_started(&self) {
        if let Ok(mut p) = self.progress.lock() {
            p.started = Some(std::time::Instant::now());
        }
    }
    /// Asks the job to stop (used by tests and by the queue).
    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[derive(Debug)]
pub enum Outcome<T> {
    Done(T),
    /// The error as text a banker can read.
    Failed(String),
    Cancelled,
}

pub struct JobResult<T> {
    pub id: JobId,
    pub key: JobKey,
    pub label: String,
    pub outcome: Outcome<T>,
}

impl<T> JobResult<T> {
    /// The status-bar text for a failed or cancelled job; `None` when the job succeeded.
    pub fn status_line(&self) -> Option<String> {
        match &self.outcome {
            Outcome::Done(_) => None,
            Outcome::Failed(msg) => Some(format!("{} failed: {}", self.label, msg)),
            Outcome::Cancelled => Some(format!("{} cancelled.", self.label)),
        }
    }
}

/// The long job shown in the status bar.
#[derive(Clone, Debug)]
pub struct ActiveJob {
    pub id: JobId,
    pub label: String,
    pub progress: Progress,
    pub cancelling: bool,
}

/// "7 s", "1 min 05 s", "1 h 02 min".
pub fn format_elapsed(d: std::time::Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{} s", s)
    } else if s < 3600 {
        format!("{} min {:02} s", s / 60, s % 60)
    } else {
        format!("{} h {:02} min", s / 3600, (s % 3600) / 60)
    }
}

impl ActiveJob {
    /// The time the job has been running (zero while it is still queued).
    pub fn elapsed(&self) -> std::time::Duration {
        self.progress.started.map(|t| t.elapsed()).unwrap_or_default()
    }

    /// "Reading 1/3", "Parsing 2/3", ...: one label per stage, empty for a job without stages.
    pub fn stage_labels(&self) -> Vec<String> {
        let n = self.progress.stages.len();
        self.progress.stages.iter().enumerate().map(|(i, s)| format!("{} {}/{}", s, i + 1, n)).collect()
    }

    /// The percentage to show: only when a loop reported real bytes read, or the job counted
    /// finished units itself (files, tabs). Never a guess.
    pub fn percent(&self) -> Option<u32> {
        self.progress.fraction.map(|f| (f.clamp(0.0, 1.0) * 100.0).floor() as u32)
    }
}

type Work<T> = Box<dyn FnOnce(&JobCtx) -> Result<T> + Send>;

struct Queued<T> {
    id: JobId,
    key: JobKey,
    label: String,
    long: bool,
    ctx: JobCtx,
    work: Work<T>,
}

struct Running {
    id: JobId,
    key: JobKey,
    label: String,
    long: bool,
    ctx: JobCtx,
}

struct State<T> {
    pending: VecDeque<Queued<T>>,
    running: Option<Running>,
    /// The newest job id per scoped key that has not been polled yet.
    latest: HashMap<JobKey, JobId>,
    /// Cancel flags of jobs that are queued or running.
    cancels: HashMap<JobId, Arc<AtomicBool>>,
    results: Vec<JobResult<T>>,
    shutdown: bool,
    worker_done: bool,
}

struct Shared<T> {
    state: Mutex<State<T>>,
    cv: Condvar,
    wake: Box<dyn Fn() + Send + Sync>,
}

pub struct JobQueue<T: Send + 'static> {
    shared: Arc<Shared<T>>,
    next_id: AtomicU64,
    threaded: bool,
    handle: Option<std::thread::JoinHandle<()>>,
}

fn lock<T>(m: &Mutex<State<T>>) -> std::sync::MutexGuard<'_, State<T>> {
    // A job that panicked is caught below, so a poisoned lock only means a result was lost.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn run_one<T: Send + 'static>(shared: &Shared<T>, job: Queued<T>) {
    {
        let mut st = lock(&shared.state);
        st.running = Some(Running {
            id: job.id,
            key: job.key,
            label: job.label.clone(),
            long: job.long,
            ctx: job.ctx.clone(),
        });
    }
    let Queued { id, key, label, ctx, work, .. } = job;
    ctx.mark_started();
    let outcome = if ctx.is_cancelled() {
        Outcome::Cancelled
    } else {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&ctx))) {
            Ok(Ok(v)) => Outcome::Done(v),
            Ok(Err(e)) if e.downcast_ref::<Cancelled>().is_some() => Outcome::Cancelled,
            Ok(Err(e)) => Outcome::Failed(format!("{:#}", e)),
            Err(p) => {
                let what = p
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| p.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown error".to_string());
                Outcome::Failed(format!("internal error: {}", what))
            }
        }
    };
    {
        let mut st = lock(&shared.state);
        st.running = None;
        st.cancels.remove(&id);
        st.results.push(JobResult { id, key, label, outcome });
    }
    // `wait_idle` and `Drop` wait on this.
    shared.cv.notify_all();
    (shared.wake)();
}

impl<T: Send + 'static> JobQueue<T> {
    /// A queue with its own worker thread. `wake` is called (from the worker) after each result.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                pending: VecDeque::new(),
                running: None,
                latest: HashMap::new(),
                cancels: HashMap::new(),
                results: Vec::new(),
                shutdown: false,
                worker_done: false,
            }),
            cv: Condvar::new(),
            wake: Box::new(wake),
        });
        let worker = shared.clone();
        let handle = std::thread::Builder::new()
            .name("mp-worker".into())
            .spawn(move || loop {
                let job = {
                    let mut st = lock(&worker.state);
                    loop {
                        if st.shutdown {
                            st.worker_done = true;
                            worker.cv.notify_all();
                            return;
                        }
                        if let Some(j) = st.pending.pop_front() {
                            // A job that a newer one replaced does not run at all.
                            let stale = j.key.scope.is_some()
                                && st.latest.get(&j.key).is_some_and(|l| *l != j.id);
                            if stale {
                                st.cancels.remove(&j.id);
                                continue;
                            }
                            break j;
                        }
                        st = worker.cv.wait(st).unwrap_or_else(|e| e.into_inner());
                    }
                };
                run_one(&worker, job);
            })
            .ok();
        Self { shared, next_id: AtomicU64::new(1), threaded: handle.is_some(), handle }
    }

    /// A queue without a thread: `submit` runs the job on the calling thread (headless mode).
    pub fn inline() -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                pending: VecDeque::new(),
                running: None,
                latest: HashMap::new(),
                cancels: HashMap::new(),
                results: Vec::new(),
                shutdown: false,
                worker_done: true,
            }),
            cv: Condvar::new(),
            wake: Box::new(|| {}),
        });
        Self { shared, next_id: AtomicU64::new(1), threaded: false, handle: None }
    }

    /// Queues a job and returns its id. `long` jobs show a progress line with Cancel.
    pub fn submit(
        &self,
        key: JobKey,
        label: impl Into<String>,
        long: bool,
        work: impl FnOnce(&JobCtx) -> Result<T> + Send + 'static,
    ) -> JobId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let cancel = Arc::new(AtomicBool::new(false));
        let ctx = JobCtx { id, cancel: cancel.clone(), progress: Arc::new(Mutex::new(Progress::default())) };
        let job = Queued { id, key, label: label.into(), long, ctx, work: Box::new(work) };
        {
            let mut st = lock(&self.shared.state);
            if key.scope.is_some() {
                if let Some(old) = st.latest.insert(key, id) {
                    if let Some(flag) = st.cancels.get(&old) {
                        flag.store(true, Ordering::Relaxed);
                    }
                }
            }
            st.cancels.insert(id, cancel);
            if self.threaded {
                st.pending.push_back(job);
                self.shared.cv.notify_all();
                return id;
            }
        }
        run_one(&self.shared, job);
        id
    }

    /// Results that are ready. Results of superseded jobs are dropped here.
    pub fn poll(&self) -> Vec<JobResult<T>> {
        let mut st = lock(&self.shared.state);
        let results = std::mem::take(&mut st.results);
        let mut out = Vec::with_capacity(results.len());
        for r in results {
            if r.key.scope.is_some() {
                match st.latest.get(&r.key) {
                    Some(l) if *l == r.id => {
                        st.latest.remove(&r.key);
                    }
                    _ => continue, // superseded
                }
            }
            out.push(r);
        }
        out
    }

    /// True while a job with this key is queued or running (its result not yet polled).
    pub fn is_busy(&self, key: JobKey) -> bool {
        lock(&self.shared.state).latest.contains_key(&key)
    }

    /// True while any job is queued or running.
    pub fn any_busy(&self) -> bool {
        let st = lock(&self.shared.state);
        st.running.is_some() || !st.pending.is_empty() || !st.results.is_empty()
    }

    /// The running long job (or the first queued one), for the status bar.
    pub fn active_long(&self) -> Option<ActiveJob> {
        let st = lock(&self.shared.state);
        if let Some(r) = st.running.as_ref().filter(|r| r.long) {
            let progress = r.ctx.progress.lock().map(|p| p.clone()).unwrap_or_default();
            return Some(ActiveJob { id: r.id, label: r.label.clone(), progress, cancelling: r.ctx.is_cancelled() });
        }
        st.pending.iter().find(|j| j.long).map(|j| ActiveJob {
            id: j.id,
            label: j.label.clone(),
            progress: Progress::default(),
            cancelling: j.ctx.is_cancelled(),
        })
    }

    /// Asks a job to stop at its next safe point.
    pub fn cancel(&self, id: JobId) {
        if let Some(flag) = lock(&self.shared.state).cancels.get(&id) {
            flag.store(true, Ordering::Relaxed);
        }
    }

    /// Asks every queued and running job to stop (used when the app closes).
    pub fn cancel_all(&self) {
        for flag in lock(&self.shared.state).cancels.values() {
            flag.store(true, Ordering::Relaxed);
        }
    }

    /// True while a job of this kind is running, or queued and not cancelled.
    pub fn kind_busy(&self, kind: JobKind) -> bool {
        let st = lock(&self.shared.state);
        st.running.as_ref().is_some_and(|r| r.key.kind == kind && !r.ctx.is_cancelled())
            || st.pending.iter().any(|j| j.key.kind == kind && !j.ctx.is_cancelled())
    }

    /// Waits until no job is running or queued, at most `timeout`. True when the queue is idle.
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        let mut st = lock(&self.shared.state);
        while st.running.is_some() || !st.pending.is_empty() {
            if !self.threaded {
                return false;
            }
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return false;
            }
            st = match self.shared.cv.wait_timeout(st, left) {
                Ok((g, _)) => g,
                Err(e) => e.into_inner().0,
            };
        }
        true
    }
}

impl<T: Send + 'static> Drop for JobQueue<T> {
    fn drop(&mut self) {
        self.cancel_all();
        let mut st = lock(&self.shared.state);
        st.shutdown = true;
        self.shared.cv.notify_all();
        // Give the running job a moment to reach a safe point; then let the process end.
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !st.worker_done {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                break;
            }
            st = match self.shared.cv.wait_timeout(st, left) {
                Ok((g, _)) => g,
                Err(e) => e.into_inner().0,
            };
        }
        drop(st);
        if let Some(h) = self.handle.take() {
            if lock(&self.shared.state).worker_done {
                let _ = h.join();
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Output files without partial results

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Temporary files that exist now (created and neither committed nor removed yet).
static LIVE_PARTS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn live_parts() -> std::sync::MutexGuard<'static, Vec<PathBuf>> {
    LIVE_PARTS.lock().unwrap_or_else(|e| e.into_inner())
}

/// The `.part` files of unfinished outputs of this process.
pub fn part_files() -> Vec<PathBuf> {
    live_parts().clone()
}

/// Deletes the `.part` file of every unfinished output (the app is closing). Returns how many
/// were deleted and the files that could not be (with the reason).
pub fn remove_part_files() -> (usize, Vec<String>) {
    let mut removed = 0;
    let mut failed = Vec::new();
    let mut parts = live_parts();
    parts.retain(|p| match std::fs::remove_file(p) {
        Ok(()) => {
            removed += 1;
            false
        }
        // never written, or already gone
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            failed.push(format!("{}: {}", p.display(), e));
            true
        }
    });
    (removed, failed)
}

/// File in the data folder listing `.part` files that could not be deleted when the program
/// closed (for example Windows still holds one). One path per line.
pub const PENDING_PARTS_FILE: &str = "pending_part_deletes.txt";

/// A path the sweep may delete: only the program's own temporary names (`.<name>.<pid>-<n>.part`).
/// The list is a plain text file; a path in it that is not one of ours is ignored, never deleted.
fn is_our_part_file(p: &Path) -> bool {
    p.file_name()
        .map(|n| {
            let n = n.to_string_lossy();
            n.starts_with('.') && n.ends_with(".part")
        })
        .unwrap_or(false)
}

fn read_pending(root: &Path) -> Vec<PathBuf> {
    std::fs::read_to_string(root.join(PENDING_PARTS_FILE))
        .map(|t| t.lines().map(str::trim).filter(|l| !l.is_empty()).map(PathBuf::from).collect())
        .unwrap_or_default()
}

fn write_pending(root: &Path, paths: &[PathBuf]) -> Result<()> {
    let file = root.join(PENDING_PARTS_FILE);
    if paths.is_empty() {
        return match std::fs::remove_file(&file) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).with_context(|| format!("Could not update {:?}", file)),
        };
    }
    std::fs::create_dir_all(root).with_context(|| format!("Could not create {:?}", root))?;
    let text: String = paths.iter().map(|p| format!("{}\n", p.display())).collect();
    std::fs::write(&file, text).with_context(|| format!("Could not write {:?}", file))
}

/// Adds `paths` to the list of `.part` files to delete at the next start (no duplicates).
pub fn record_pending_parts(root: &Path, paths: &[PathBuf]) -> Result<()> {
    let mut all = read_pending(root);
    for p in paths {
        if !all.contains(p) {
            all.push(p.clone());
        }
    }
    write_pending(root, &all)
}

/// What the closing step did with unfinished output files.
#[derive(Debug, Default, PartialEq)]
pub struct PartCleanup {
    /// Deleted now.
    pub removed: usize,
    /// Could not be deleted now (path and reason). They are recorded for the next start.
    pub failed: Vec<String>,
    /// Set when the failed files could not even be recorded (the reason).
    pub record_error: Option<String>,
}

/// Closing: deletes the `.part` file of every unfinished output; any that cannot be deleted
/// (Windows still holds it) are recorded in `<root>/pending_part_deletes.txt` and swept at the
/// next start by [`sweep_pending_parts`].
pub fn remove_part_files_recording(root: &Path) -> PartCleanup {
    let (removed, failed) = remove_part_files();
    let mut out = PartCleanup { removed, failed, record_error: None };
    if !out.failed.is_empty() {
        let left = part_files();
        if let Err(e) = record_pending_parts(root, &left) {
            out.record_error = Some(format!("{:#}", e));
        }
    }
    out
}

/// What the start-up sweep did.
#[derive(Debug, Default, PartialEq)]
pub struct SweepReport {
    pub deleted: usize,
    /// Still cannot be deleted (path and reason); they stay on the list for the next start.
    pub still_locked: Vec<String>,
}

impl SweepReport {
    /// Text for the status bar; `None` when there was nothing on the list.
    pub fn message(&self) -> Option<String> {
        if self.deleted == 0 && self.still_locked.is_empty() {
            return None;
        }
        let mut m = format!("Start-up: {} unfinished output file(s) left by the last session deleted.", self.deleted);
        if !self.still_locked.is_empty() {
            m.push_str(&format!(
                " {} still cannot be deleted and will be tried again at the next start: {}",
                self.still_locked.len(),
                self.still_locked.join("; ")
            ));
        }
        Some(m)
    }
}

/// Start-up sweep: deletes the `.part` files the last session could not delete. Files already
/// gone are dropped from the list; files that still cannot be deleted stay on it.
pub fn sweep_pending_parts(root: &Path) -> SweepReport {
    let listed = read_pending(root);
    let mut report = SweepReport::default();
    let mut keep: Vec<PathBuf> = Vec::new();
    for p in listed {
        if !is_our_part_file(&p) {
            continue;
        }
        match std::fs::remove_file(&p) {
            Ok(()) => report.deleted += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                report.still_locked.push(format!("{}: {}", p.display(), e));
                keep.push(p);
            }
        }
    }
    if let Err(e) = write_pending(root, &keep) {
        report.still_locked.push(format!("the list {} could not be updated: {:#}", PENDING_PARTS_FILE, e));
    }
    report
}

/// A temporary name next to the final output file. Dropping it without `commit` deletes the
/// temporary file, so an error, a cancel or a panic leaves nothing behind.
pub struct TempOutput {
    final_path: PathBuf,
    temp_path: PathBuf,
    committed: bool,
}

impl TempOutput {
    pub fn new(final_path: &Path) -> Self {
        let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = final_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "output".to_string());
        let temp_path = final_path.with_file_name(format!(".{}.{}-{}.part", name, std::process::id(), n));
        live_parts().push(temp_path.clone());
        Self { final_path: final_path.to_path_buf(), temp_path, committed: false }
    }

    /// Where the writer should write.
    pub fn path(&self) -> &Path {
        &self.temp_path
    }

    /// Moves the finished file to its final name (replacing an older file of that name).
    pub fn commit(mut self) -> Result<()> {
        std::fs::rename(&self.temp_path, &self.final_path).with_context(|| {
            format!("Could not move the finished file to {:?}. The output was not written.", self.final_path)
        })?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for TempOutput {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.temp_path);
        }
        live_parts().retain(|p| p != &self.temp_path);
    }
}

/// Writes `final_path` through a temporary name: `write` gets the temporary path; on success the
/// file is renamed, on error or cancel it is removed. Checks `ctx` before and after writing.
pub fn write_atomic<R>(
    final_path: &Path,
    ctx: &JobCtx,
    write: impl FnOnce(&Path) -> Result<R>,
) -> Result<R> {
    ctx.check()?;
    let tmp = TempOutput::new(final_path);
    let r = write(tmp.path())?;
    ctx.check()?;
    tmp.commit()?;
    Ok(r)
}

/// Runs a job body on the calling thread with a context that is never cancelled. Headless mode
/// uses this: the same job functions, no thread.
pub fn run_inline<R>(work: impl FnOnce(&JobCtx) -> Result<R>) -> Result<R> {
    work(&JobCtx::inert())
}
