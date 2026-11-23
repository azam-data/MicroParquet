//! Project files in the UI (Phase 8 W4): File ▸ New project, Open project…, Open recent project, Save project,
//! Save project as… and Close project; the Sources dialog that comes before a project opens; the question asked
//! when there are unsaved changes; and the placeholder tab of a tab that did not open.
//!
//! The format and the rebuild rules are in `micro_parquet::project`; this file draws the dialogs and drives the
//! worker tab by tab ("Opening 4 of 9: Gap ladder").

use crate::ui::chart::{scale_word, tab_key, ChartTabInfo};
use crate::ui::prelude::*;
use micro_parquet::commands::CommandId;
use micro_parquet::project::{
    build_project_progress, changed_since_saved, hashing_text, load_project, look_in_folder, opened_summary, project_dir_of, project_name, projects_dir, push_recent, source_rows, short_date, write_project, Excluded, HashCache, Opener,
    Prepared, Project, ProjectTab, SourceRow, SourceStatus, TabSnapshot, PROJECT_EXT,
};
use micro_parquet::tab_jobs::{ProjectBuilt, ProjectChecked, ProjectLoaded};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

pub const SOURCES_TITLE: &str = "Sources";
pub const PROMPT_TITLE: &str = "Unsaved changes";
pub const SAVE_TITLE: &str = "Save project";
pub const RECENT_TITLE: &str = "Open recent project";

/// What waits for the answer to "Save / Don't save / Cancel".
#[derive(Clone, Debug, PartialEq)]
pub enum AfterPrompt {
    /// The window was asked to close.
    Quit,
    NewProject,
    /// Choose a project file, then open it.
    OpenProject,
    /// Open this project file.
    OpenPath(PathBuf),
    CloseProject,
}

/// What the project dialogs drew, for the render tests.
#[derive(Clone, Debug, Default)]
pub struct ProjectRects {
    pub buttons: Vec<(String, egui::Rect)>,
    /// One per file of the Sources dialog (its name) and its status text.
    pub rows: Vec<(String, egui::Rect)>,
    pub statuses: Vec<(String, egui::Rect)>,
    pub message: Option<egui::Rect>,
}

/// The Sources dialog: the project, and the files relinked so far.
pub struct SourcesDraft {
    pub project: Project,
    pub path: PathBuf,
    pub relinks: BTreeMap<String, PathBuf>,
    pub rows: Vec<SourceRow>,
    pub note: String,
}

/// The Save dialog that lists the tabs left out.
pub struct SaveDraft {
    pub project: Project,
    pub excluded: Vec<Excluded>,
    pub keys: Vec<(u64, String)>,
    pub path: PathBuf,
}

/// A project being opened: tabs are built one after the other, in dependency order.
pub struct Opening {
    pub opener: Opener,
    pub order: Vec<usize>,
    pub pos: usize,
    /// The tab on the worker: its id and its place in the project.
    pub in_flight: Option<(u64, usize)>,
    pub opened: usize,
    pub failures: Vec<(String, String)>,
    pub path: PathBuf,
    pub relinks: BTreeMap<String, PathBuf>,
}

#[derive(Default)]
pub struct ProjectState {
    /// A project is open (it may not have a file yet).
    pub active: bool,
    pub path: Option<PathBuf>,
    pub name: String,
    /// The key each tab has in the project, by tab id.
    pub keys: HashMap<u64, String>,
    /// The recipe of each tab that did not open, so saving the project again keeps it.
    pub kept: HashMap<u64, ProjectTab>,
    /// What the tabs looked like when the project was saved or opened.
    pub saved_fp: String,
    pub dirty: bool,
    pub fp_checked_at: f64,
    pub hash_cache: HashCache,
    pub sources: Option<SourcesDraft>,
    pub save: Option<SaveDraft>,
    pub opening: Option<Opening>,
    pub prompt: Option<AfterPrompt>,
    /// What to do once a save that the prompt asked for is done.
    pub after_save: Option<AfterPrompt>,
    /// The project is being built and hashed on the worker.
    pub saving: bool,
    pub close_confirmed: bool,
    pub quit_now: bool,
    /// The line "9 tabs opened, 1 failed: …" of the last open.
    pub last_summary: String,
    pub rects: ProjectRects,
    /// Tests: the file or folder the dialog would give, and how often a dialog was asked for.
    #[cfg(test)]
    pub test_pick: Option<PathBuf>,
    #[cfg(test)]
    pub test_dir: Option<PathBuf>,
    #[cfg(test)]
    pub dialog_requests: u32,
}

/// The window title: "MicroParquet v1.8.0 — Treasury Oct • — loans.csv".
pub fn project_window_title(project: Option<(&str, bool)>, file: Option<&str>) -> String {
    let mut t = format!("MicroParquet v{}", env!("CARGO_PKG_VERSION"));
    if let Some((name, dirty)) = project {
        t.push_str(&format!(" \u{2014} {}{}", name, if dirty { " \u{2022}" } else { "" }));
    }
    if let Some(f) = file.map(str::trim).filter(|f| !f.is_empty()) {
        t.push_str(&format!(" \u{2014} {}", f));
    }
    t
}

fn status_text(r: &SourceRow) -> String {
    match &r.status {
        SourceStatus::Found => "Found".to_string(),
        SourceStatus::Changed { .. } => "Changed".to_string(),
        SourceStatus::Missing => "Missing".to_string(),
    }
}

fn status_detail(r: &SourceRow) -> Option<String> {
    match &r.status {
        SourceStatus::Changed { saved, now } => Some(format!(
            "Saved {} ({} bytes), now {} ({} bytes).",
            short_date(saved.1),
            format_with_commas(saved.0 as f64, 0),
            short_date(now.1),
            format_with_commas(now.0 as f64, 0)
        )),
        _ => None,
    }
}

impl MicroParquetApp {
    // ---------------------------------------------------------------------------- what the project is

    /// A short fingerprint of everything a project saves: the tabs in order, their steps and formats, and the chart tabs.
    pub(crate) fn project_fingerprint(&self) -> String {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for t in &self.tabs {
            t.id.hash(&mut h);
            t.title.hash(&mut h);
            // what the tab is made from, not the files' sizes and dates: a data file that changes is not a change to the project
            format!("{:?}", t.source).hash(&mut h);
            for s in &t.steps {
                format!("{:?}", s).hash(&mut h);
            }
            format!("{:?}", t.col_formats.iter().collect::<BTreeMap<_, _>>()).hash(&mut h);
            format!("{:?}", t.summary.iter().collect::<BTreeMap<_, _>>()).hash(&mut h);
            t.control_col.hash(&mut h);
            if let Some(info) = self.chart_tabs.get(&t.id) {
                serde_json::to_string(&info.data.spec).unwrap_or_default().hash(&mut h);
            }
        }
        self.tabs.get(self.active_tab).map(|t| t.id).hash(&mut h);
        format!("{:016x}", h.finish())
    }

    /// The project has changes that are not saved.
    pub(crate) fn project_unsaved(&self) -> bool {
        self.project.active && self.project.opening.is_none() && self.project_fingerprint() != self.project.saved_fp
    }

    fn project_snapshots(&self) -> Vec<TabSnapshot> {
        self.tabs
            .iter()
            .map(|t| TabSnapshot {
                id: t.id,
                key: self.project.keys.get(&t.id).cloned(),
                title: t.title.clone(),
                source: t.source.clone(),
                steps: t.steps.clone(),
                exec_steps: t.exec_steps,
                col_formats: t.col_formats.clone(),
                summary: t.summary.clone(),
                control_col: t.control_col.clone(),
                chart: self.settings.chart.get(&tab_key(t)).cloned(),
                chart_tab: self.chart_tabs.get(&t.id).and_then(|i| i.source_tab.map(|s| (i.data.spec.clone(), s))),
                kept: self.project.kept.get(&t.id).cloned(),
            })
            .collect()
    }

    /// Called every frame: keeps the "•" up to date and moves an opening project on.
    pub(crate) fn project_tick(&mut self, ctx: &egui::Context) {
        if self.project.quit_now {
            self.project.quit_now = false;
            self.project.close_confirmed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.project_advance();
        if self.project.active {
            let now = ctx.input(|i| i.time);
            if now - self.project.fp_checked_at > 0.4 { // not a size: seconds between two checks of the unsaved marker
                self.project.fp_checked_at = now;
                self.project.dirty = self.project_unsaved();
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        } else {
            self.project.dirty = false;
        }
    }

    // ---------------------------------------------------------------------------- commands

    pub(crate) fn run_project_command(&mut self, id: CommandId) {
        match id {
            CommandId::ProjectNew => self.project_request(AfterPrompt::NewProject),
            CommandId::ProjectOpen => self.project_request(AfterPrompt::OpenProject),
            CommandId::ProjectRecent => self.active_tool = ActiveToolModal::ProjectRecent,
            CommandId::ProjectSave => self.project_save(None),
            CommandId::ProjectSaveAs => self.project_save_as(None),
            CommandId::ProjectClose => self.project_request(AfterPrompt::CloseProject),
            _ => {}
        }
    }

    /// Does `action`, after asking about unsaved changes when there are some.
    pub(crate) fn project_request(&mut self, action: AfterPrompt) {
        let unsaved = self.project_unsaved() || (!self.project.active && !self.tabs.is_empty() && !matches!(action, AfterPrompt::Quit));
        if unsaved && !(action == AfterPrompt::Quit && self.project.close_confirmed) {
            self.project.prompt = Some(action);
            self.active_tool = ActiveToolModal::ProjectPrompt;
        } else {
            self.project_do(action);
        }
    }

    fn project_do(&mut self, action: AfterPrompt) {
        match action {
            AfterPrompt::Quit => self.project.quit_now = true,
            AfterPrompt::NewProject => self.project_new(),
            AfterPrompt::OpenProject => {
                if let Some(p) = self.project_pick_file(false) {
                    self.project_open_path(p);
                }
            }
            AfterPrompt::OpenPath(p) => self.project_open_path(p),
            AfterPrompt::CloseProject => self.project_close(),
        }
    }

    /// The file dialog (or, in tests, the file the test picked). `save`: a name to write.
    fn project_pick_file(&mut self, save: bool) -> Option<PathBuf> {
        #[cfg(test)]
        {
            self.project.dialog_requests += 1;
            let _ = save;
            self.project.test_pick.take()
        }
        #[cfg(not(test))]
        {
            let dir = projects_dir();
            let mut d = rfd::FileDialog::new().add_filter("MicroParquet project (.mpproj)", &[PROJECT_EXT]).set_directory(if dir.is_dir() { dir } else { self.paths.root_dir.clone() });
            if save {
                d = d.set_file_name(format!("{}.{}", if self.project.name.is_empty() { "Project" } else { &self.project.name }, PROJECT_EXT));
                d.save_file()
            } else {
                d.pick_file()
            }
        }
    }

    fn project_pick_folder(&mut self) -> Option<PathBuf> {
        #[cfg(test)]
        {
            self.project.dialog_requests += 1;
            self.project.test_dir.take()
        }
        #[cfg(not(test))]
        {
            rfd::FileDialog::new().pick_folder()
        }
    }

    fn close_all_tabs(&mut self) {
        while !self.tabs.is_empty() {
            self.close_tab(0);
        }
        self.chart_tabs.clear();
        self.project.keys.clear();
        self.project.kept.clear();
        self.project.opening = None;
    }

    fn project_new(&mut self) {
        self.close_all_tabs();
        self.project.active = true;
        self.project.path = None;
        self.project.name = "Untitled".to_string();
        self.project.saved_fp = self.project_fingerprint();
        self.project.dirty = false;
        self.status_message = "New project. Open files, then File \u{25B8} Save project.".to_string();
    }

    fn project_close(&mut self) {
        self.close_all_tabs();
        self.project.active = false;
        self.project.path = None;
        self.project.name.clear();
        self.project.dirty = false;
        self.status_message = "Project closed.".to_string();
    }

    // ---------------------------------------------------------------------------- saving

    /// File ▸ Save project: to its file, or, when it has none yet, as a new file.
    pub(crate) fn project_save(&mut self, after: Option<AfterPrompt>) {
        match self.project.path.clone() {
            Some(p) => self.project_write(p, after),
            None => self.project_save_as(after),
        }
    }

    pub(crate) fn project_save_as(&mut self, after: Option<AfterPrompt>) {
        let Some(mut path) = self.project_pick_file(true) else {
            return;
        };
        if path.extension().is_none_or(|e| !e.eq_ignore_ascii_case(PROJECT_EXT)) {
            path.set_extension(PROJECT_EXT);
        }
        self.project_write(path, after);
    }

    /// Builds the project from the open tabs. Tabs that cannot be rebuilt are listed first; the file is written
    /// when the list is empty or the owner accepts it.
    fn project_write(&mut self, path: PathBuf, after: Option<AfterPrompt>) {
        if self.project.saving {
            self.status_message = "The project is being saved. Wait for it to finish.".to_string();
            return;
        }
        let snaps = self.project_snapshots();
        let active = self.tabs.get(self.active_tab).map(|t| t.id);
        let dir = project_dir_of(&path);
        let paths = self.paths.clone();
        let mut cache = self.project.hash_cache.clone();
        self.project.saving = true;
        self.project.after_save = after;
        self.status_message = "Saving the project\u{2026}".to_string();
        // the recipes are built and the files hashed on the worker (reusing the cache): the window never waits
        self.jobs.submit(JobKey::unique(JobKind::ProjectSave), "Saving the project", false, move |ctx| {
            let last = std::cell::RefCell::new(String::new());
            let progress = |name: &str, read: u64, total: u64| {
                let (text, fraction) = hashing_text(name, read, total);
                // one report per whole percent of a file
                if *last.borrow() != text {
                    ctx.set_progress(text.clone(), Some(fraction));
                    *last.borrow_mut() = text;
                }
                ctx.is_cancelled()
            };
            let saved = build_project_progress(&snaps, active, &dir, &paths, &mut cache, &progress)?;
            Ok(JobOutput::ProjectSave(Box::new(ProjectBuilt { saved, cache, path })))
        });
    }

    /// The worker built the project: written at once, or the tabs that were left out are listed first.
    pub(crate) fn apply_project_built(&mut self, b: ProjectBuilt) {
        self.project.saving = false;
        self.project.hash_cache.extend(b.cache);
        let draft = SaveDraft { project: b.saved.project, excluded: b.saved.excluded, keys: b.saved.keys, path: b.path };
        if draft.excluded.is_empty() {
            self.project_commit(draft);
        } else {
            self.project.save = Some(draft);
            self.active_tool = ActiveToolModal::ProjectSave;
        }
    }

    pub(crate) fn project_save_failed(&mut self, msg: &str) {
        self.project.saving = false;
        self.project.after_save = None;
        self.status_message = format!("The project was not saved: {}", msg);
    }

    fn project_commit(&mut self, draft: SaveDraft) {
        match write_project(&draft.path, &draft.project) {
            Err(e) => {
                self.project.after_save = None;
                self.status_message = format!("The project was not saved: {:#}", e);
            }
            Ok(()) => {
                for (id, key) in &draft.keys {
                    self.project.keys.insert(*id, key.clone());
                }
                self.project.active = true;
                self.project.name = project_name(&draft.path);
                self.project.path = Some(draft.path.clone());
                self.project.saved_fp = self.project_fingerprint();
                self.project.dirty = false;
                push_recent(&mut self.settings.recent_projects, &draft.path);
                self.save_settings();
                let n = draft.project.tabs.len();
                self.status_message = format!(
                    "Saved the project {} ({} tab{}{}).",
                    draft.path.display(),
                    n,
                    if n == 1 { "" } else { "s" },
                    if draft.excluded.is_empty() { String::new() } else { format!(", {} left out", draft.excluded.len()) }
                );
                if let Some(a) = self.project.after_save.take() {
                    self.project_do(a);
                }
            }
        }
    }

    // ---------------------------------------------------------------------------- opening

    /// Reads the project file and shows the Sources dialog; nothing runs before it is answered.
    pub(crate) fn project_open_path(&mut self, path: PathBuf) {
        match load_project(&path) {
            Err(e) => self.status_message = format!("{:#}", e),
            Ok(project) => {
                let dir = project_dir_of(&path);
                let relinks = BTreeMap::new();
                let rows = source_rows(&project, &dir, &relinks);
                self.project.sources = Some(SourcesDraft { project, path, relinks, rows, note: String::new() });
                self.active_tool = ActiveToolModal::ProjectSources;
            }
        }
    }

    /// "Open" of the Sources dialog: the current tabs close and the project's tabs are built one by one.
    fn project_start_opening(&mut self, draft: SourcesDraft) {
        self.close_all_tabs();
        self.project.active = true;
        self.project.path = Some(draft.path.clone());
        self.project.name = project_name(&draft.path);
        self.project.dirty = false;
        self.project.last_summary.clear();
        push_recent(&mut self.settings.recent_projects, &draft.path);
        self.save_settings();
        let opener = Opener::new(draft.project, &draft.path, draft.relinks.clone(), self.paths.clone());
        let order = opener.order();
        self.project.opening = Some(Opening { opener, order, pos: 0, in_flight: None, opened: 0, failures: Vec::new(), path: draft.path, relinks: draft.relinks });
        self.status_message = "Opening the project\u{2026}".to_string();
    }

    /// Builds the next tab (or the placeholder of one that cannot be built) until a tab is on the worker or all are done.
    pub(crate) fn project_advance(&mut self) {
        loop {
            let Some(op) = self.project.opening.as_mut() else { return };
            if op.in_flight.is_some() {
                return;
            }
            if op.pos >= op.order.len() {
                self.project_finish_opening();
                return;
            }
            let (n, total) = (op.pos + 1, op.order.len());
            let index = op.order[op.pos];
            op.pos += 1;
            let Some(pt) = op.opener.project.tabs.get(index).cloned() else { continue };
            let prepared = op.opener.prepare(index);
            let id = self.next_tab_id();
            match prepared {
                Err(reason) => self.project_placeholder(id, &pt, &reason),
                Ok(p) => {
                    let col_formats: HashMap<String, ColumnFormatSpec> = pt.col_formats.iter().map(|(k, v)| (k.clone(), *v)).collect();
                    let temp_dir = self.paths.temp_dir.clone();
                    let title = pt.title.clone();
                    let label = format!("Opening {} of {}: {}", n, total, title);
                    self.status_message = label.clone();
                    if let Some(op) = self.project.opening.as_mut() {
                        op.in_flight = Some((id, index));
                    }
                    match p {
                        Prepared::Table { source, steps } => {
                            let req = LoadRequest { tab_id: id, title, source, steps, col_formats, temp_dir, reconcile_rate_used: None };
                            self.jobs.submit(JobKey::tab(JobKind::ProjectTab, id), label, true, move |ctx| {
                                load_tab(req, ctx).map(|l| JobOutput::ProjectTab(Box::new(ProjectLoaded { loaded: l, chart: None, index })))
                            });
                        }
                        Prepared::Chart { source, steps, spec, .. } => {
                            self.jobs.submit(JobKey::tab(JobKind::ProjectTab, id), label, true, move |ctx| {
                                ctx.check()?;
                                let (data, src) = micro_parquet::project::chart_tab_source(&source, &steps, &spec, &temp_dir)?;
                                let req = LoadRequest { tab_id: id, title, source: src, steps: Vec::new(), col_formats, temp_dir, reconcile_rate_used: None };
                                load_tab(req, ctx).map(|l| JobOutput::ProjectTab(Box::new(ProjectLoaded { loaded: l, chart: Some(data), index })))
                            });
                        }
                    }
                    return;
                }
            }
        }
    }

    /// A tab that cannot be built: a placeholder with the reason, and its recipe kept.
    fn project_placeholder(&mut self, id: u64, pt: &ProjectTab, reason: &str) {
        let tab = TabState::unavailable(id, pt.title.clone(), reason.to_string(), Vec::new());
        self.tabs.push(tab);
        self.project.keys.insert(id, pt.key.clone());
        self.project.kept.insert(id, pt.clone());
        if let Some(op) = self.project.opening.as_mut() {
            op.opener.failed(&pt.key, reason);
            op.failures.push((pt.title.clone(), reason.to_string()));
            op.in_flight = None;
        }
    }

    /// The worker built a tab of the project.
    pub(crate) fn apply_project_tab(&mut self, l: ProjectLoaded) {
        let ProjectLoaded { loaded, chart, index } = l;
        let LoadedTab { mut tab, .. } = loaded;
        let Some(op) = self.project.opening.as_mut() else { return };
        if op.in_flight.map(|(id, _)| id) != Some(tab.id) {
            return;
        }
        let Some(pt) = op.opener.project.tabs.get(index).cloned() else { return };
        op.in_flight = None;
        op.opened += 1;
        let exec = tab.exec_steps.min(tab.steps.len());
        op.opener.built(&pt.key, tab.source.clone(), tab.steps[..exec].to_vec());
        tab.summary = pt.summary.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let want_control = pt.control_col.clone().filter(|c| tab.control_col.as_ref() != Some(c) && tab.cached_slice.get_column_names().iter().any(|n| n.as_str() == c.as_str()));
        let id = tab.id;
        self.project.keys.insert(id, pt.key.clone());
        if let Some(v) = &pt.chart {
            self.settings.chart.insert(tab_key(&tab), v.clone());
        }
        if let (Some(data), micro_parquet::project::Recipe::Chart(c)) = (chart, &pt.recipe) {
            let src_id = self.project.keys.iter().find(|(_, k)| **k == c.source).map(|(i, _)| *i);
            let src_tab = src_id.and_then(|i| self.tabs.iter().find(|t| t.id == i));
            let scale = self.chart_scale_of(&data.spec, src_tab);
            let info = ChartTabInfo { data, show_table: false, divisor: scale.map(|s| s.divisor).unwrap_or(1.0), decimals: self.global_decimals, unit: scale.as_ref().map(scale_word).unwrap_or("").to_string(), source_tab: src_id };
            self.chart_tabs.insert(id, info);
        }
        self.tabs.push(tab);
        let idx = self.tabs.len() - 1;
        if let Some(c) = want_control {
            self.tabs[idx].control_col = Some(c);
            self.refresh_tab_at(idx, true, None);
        }
        self.submit_summary(idx);
        self.submit_step_counts(idx);
    }

    /// The worker could not build a tab of the project.
    pub(crate) fn project_tab_failed(&mut self, tab_id: u64, msg: &str) {
        let Some(op) = self.project.opening.as_mut() else { return };
        let Some((id, index)) = op.in_flight else { return };
        if id != tab_id {
            return;
        }
        let Some(pt) = op.opener.project.tabs.get(index).cloned() else { return };
        self.project_placeholder(tab_id, &pt, msg);
    }

    fn project_finish_opening(&mut self) {
        let Some(op) = self.project.opening.take() else { return };
        let summary = opened_summary(op.opened, &op.failures);
        let active_key = op.opener.project.active_tab.clone();
        if let Some(id) = self.project.keys.iter().find(|(_, k)| **k == active_key && !active_key.is_empty()).map(|(i, _)| *i) {
            if let Some(i) = self.tabs.iter().position(|t| t.id == id) {
                self.active_tab = i;
            }
        } else {
            self.active_tab = self.tabs.len().saturating_sub(1);
        }
        if !self.tabs.is_empty() {
            self.init_tool_defaults_from_active_tab();
        }
        self.selected_column = None;
        self.selected_step = None;
        self.project.saved_fp = self.project_fingerprint();
        self.project.dirty = false;
        self.project.last_summary = summary.clone();
        self.status_message = summary;
        // the files are compared with the hashes the project saved, on the worker, once the tabs are open
        let project = op.opener.project.clone();
        let (dir, relinks) = (project_dir_of(&op.path), op.relinks.clone());
        self.jobs.submit(JobKey::unique(JobKind::ProjectCheck), "Checking the project's files", false, move |ctx| {
            let (warnings, hashes) = changed_since_saved(&project, &dir, &relinks, &|| ctx.check().is_err());
            Ok(JobOutput::ProjectCheck(Box::new(ProjectChecked { warnings, hashes })))
        });
    }

    pub(crate) fn apply_project_check(&mut self, c: ProjectChecked) {
        for (p, size, modified, h) in c.hashes {
            self.project.hash_cache.insert(p, (size, modified, h));
        }
        if !c.warnings.is_empty() {
            let base = self.project.last_summary.clone();
            self.status_message = if base.is_empty() { c.warnings.join("; ") } else { format!("{} {}", base, c.warnings.join("; ")) };
        }
    }

    // ---------------------------------------------------------------------------- the dialogs

    pub(crate) fn render_project_sources_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let Some(mut draft) = self.project.sources.take() else {
            self.active_tool = ActiveToolModal::None;
            return ModalOutcome { close: false, step: None };
        };
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = ProjectRects::default();
        let (mut open, mut cancel, mut folder) = (false, false, false);
        let mut browse: Option<usize> = None;
        let missing = draft.rows.iter().filter(|r| r.status == SourceStatus::Missing).count();
        let title = format!("{}  {}", theme::ph::FOLDER_OPEN, SOURCES_TITLE);
        theme::modal(&title).default_width(theme::DIALOG_W_PIVOT).show(ctx, |d| {
            d.body(|ui| {
                ui.add(egui::Label::new(egui::RichText::new(format!("The project '{}' reads these files. Check them before its tabs open.", project_name(&draft.path))).color(tk.ink)).wrap());
                ui.add(egui::Label::new(egui::RichText::new("Browse\u{2026} points a file at another file: last month's project on this month's file. The columns the project uses must be in it.").size(theme::TEXT_LABEL).color(tk.muted)).wrap());
                ui.add_space(theme::SPACE_S);
                if draft.rows.is_empty() {
                    ui.label(egui::RichText::new("This project reads no data files.").color(tk.muted));
                }
                egui::ScrollArea::vertical().id_salt("project_sources").max_height(theme::LIST_MAX_H_L).auto_shrink([false, true]).show(ui, |ui| {
                    for (i, r) in draft.rows.iter().enumerate() {
                        let row = ui.vertical(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(egui::RichText::new(&r.name).strong().color(tk.ink));
                                let colour = match r.status {
                                    SourceStatus::Found => tk.success,
                                    SourceStatus::Changed { .. } => tk.warning,
                                    SourceStatus::Missing => tk.danger,
                                };
                                let s = ui.label(egui::RichText::new(status_text(r)).color(colour).strong());
                                rects.statuses.push((r.name.clone(), s.rect));
                            });
                            ui.horizontal_wrapped(|ui| {
                                let b = ui.button("Browse\u{2026}");
                                rects.buttons.push((format!("Browse {}", r.name), b.rect));
                                if b.clicked() {
                                    browse = Some(i);
                                }
                                let shown = if r.relinked { format!("{} (relinked from {})", r.path.display(), r.saved_path) } else { r.saved_path.clone() };
                                ui.add(egui::Label::new(egui::RichText::new(shown).size(theme::TEXT_LABEL).color(tk.muted)).wrap());
                            });
                            if let Some(t) = status_detail(r) {
                                ui.add(egui::Label::new(egui::RichText::new(t).size(theme::TEXT_LABEL).color(tk.muted)).wrap());
                            }
                            ui.add(egui::Label::new(egui::RichText::new(format!("Used by: {}", r.tabs.join(", "))).size(theme::TEXT_LABEL).color(tk.muted)).wrap());
                        });
                        rects.rows.push((r.name.clone(), row.response.rect));
                        ui.add_space(theme::SPACE_XS);
                        ui.separator();
                    }
                });
                if missing > 0 {
                    let m = ui.add(egui::Label::new(egui::RichText::new(format!("{} missing. Their tabs open as placeholders with the reason; the others open as usual.", if missing == 1 { "1 file is".to_string() } else { format!("{} files are", missing) })).color(tk.warning)).wrap());
                    rects.message = Some(m.rect);
                }
                if !draft.note.is_empty() {
                    ui.add(egui::Label::new(egui::RichText::new(&draft.note).color(tk.muted)).wrap());
                }
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let b = theme::primary_button(ui, "Open");
                    rects.buttons.push(("Open".to_string(), b.rect));
                    open = b.clicked();
                    let f = ui.button("Look in folder\u{2026}");
                    rects.buttons.push(("Look in folder\u{2026}".to_string(), f.rect));
                    folder = f.clicked();
                    let c = ui.button("Cancel");
                    rects.buttons.push(("Cancel".to_string(), c.rect));
                    cancel = c.clicked();
                });
            });
        });
        self.project.rects = rects;
        let dir = project_dir_of(&draft.path);
        if let Some(i) = browse {
            if let Some(p) = self.project_pick_file_any() {
                let saved = draft.rows[i].saved_path.clone();
                draft.relinks.insert(saved, p);
                draft.rows = source_rows(&draft.project, &dir, &draft.relinks);
                draft.note.clear();
            }
        }
        if folder {
            if let Some(f) = self.project_pick_folder() {
                let found = look_in_folder(&draft.rows, &f, &mut draft.relinks);
                draft.rows = source_rows(&draft.project, &dir, &draft.relinks);
                draft.note = format!("Looked in {}: {} file{} found.", f.display(), found, if found == 1 { "" } else { "s" });
            }
        }
        if open {
            self.active_tool = ActiveToolModal::None;
            self.project_start_opening(draft);
        } else if cancel {
            self.active_tool = ActiveToolModal::None;
        } else {
            self.project.sources = Some(draft);
        }
        ModalOutcome { close: false, step: None }
    }

    /// A data file to relink to.
    fn project_pick_file_any(&mut self) -> Option<PathBuf> {
        #[cfg(test)]
        {
            self.project.dialog_requests += 1;
            self.project.test_pick.take()
        }
        #[cfg(not(test))]
        {
            rfd::FileDialog::new().add_filter("Data Files (CSV, Parquet)", &["csv", "parquet"]).pick_file()
        }
    }

    pub(crate) fn render_project_save_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let Some(draft) = self.project.save.take() else {
            self.active_tool = ActiveToolModal::None;
            return ModalOutcome { close: false, step: None };
        };
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = ProjectRects::default();
        let (mut save, mut cancel) = (false, false);
        let title = format!("{}  {}", theme::ph::FLOPPY_DISK, SAVE_TITLE);
        theme::modal(&title).default_width(theme::DIALOG_W_PIVOT).show(ctx, |d| {
            d.body(|ui| {
                let n = draft.excluded.len();
                ui.add(egui::Label::new(egui::RichText::new(format!("{} tab{} cannot be built again from a project, so {} left out of it:", n, if n == 1 { "" } else { "s" }, if n == 1 { "it is" } else { "they are" })).color(tk.ink)).wrap());
                ui.add_space(theme::SPACE_S);
                egui::ScrollArea::vertical().id_salt("project_excluded").max_height(theme::LIST_MAX_H_L).auto_shrink([false, true]).show(ui, |ui| {
                    for e in &draft.excluded {
                        let r = ui.add(egui::Label::new(egui::RichText::new(format!("{}: {}", e.title, e.reason)).color(tk.ink)).wrap());
                        rects.rows.push((e.title.clone(), r.rect));
                        ui.add_space(theme::SPACE_XS);
                    }
                });
                ui.add(egui::Label::new(egui::RichText::new(format!("Saved as {}", draft.path.display())).size(theme::TEXT_LABEL).color(tk.muted)).wrap());
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let b = theme::primary_button(ui, "Save the others");
                    rects.buttons.push(("Save the others".to_string(), b.rect));
                    save = b.clicked();
                    let c = ui.button("Cancel");
                    rects.buttons.push(("Cancel".to_string(), c.rect));
                    cancel = c.clicked();
                });
            });
        });
        self.project.rects = rects;
        if save {
            self.active_tool = ActiveToolModal::None;
            self.project_commit(draft);
        } else if cancel {
            self.active_tool = ActiveToolModal::None;
            self.project.after_save = None;
        } else {
            self.project.save = Some(draft);
        }
        ModalOutcome { close: false, step: None }
    }

    /// "Save / Don't save / Cancel".
    pub(crate) fn render_project_prompt_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let Some(action) = self.project.prompt.clone() else {
            self.active_tool = ActiveToolModal::None;
            return ModalOutcome { close: false, step: None };
        };
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = ProjectRects::default();
        let (mut save, mut dont, mut cancel) = (false, false, false);
        let line = if self.project.active {
            format!("The project '{}' has changes that are not saved.", self.project.name)
        } else {
            "The tabs you have open are not saved in a project.".to_string()
        };
        let then = match &action {
            AfterPrompt::Quit => "before the program closes",
            AfterPrompt::NewProject => "before a new project starts",
            AfterPrompt::OpenProject | AfterPrompt::OpenPath(_) => "before another project opens",
            AfterPrompt::CloseProject => "before the project closes",
        };
        theme::modal(PROMPT_TITLE).default_width(theme::DIALOG_W_PIVOT_CONVERT).show(ctx, |d| {
            d.body(|ui| {
                ui.add(egui::Label::new(egui::RichText::new(format!("{} Save them {}?", line, then)).color(tk.ink)).wrap());
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let s = theme::primary_button(ui, "Save");
                    rects.buttons.push(("Save".to_string(), s.rect));
                    save = s.clicked();
                    let n = ui.button("Don't save");
                    rects.buttons.push(("Don't save".to_string(), n.rect));
                    dont = n.clicked();
                    let c = ui.button("Cancel");
                    rects.buttons.push(("Cancel".to_string(), c.rect));
                    cancel = c.clicked();
                });
            });
        });
        self.project.rects = rects;
        if save {
            self.project.prompt = None;
            self.active_tool = ActiveToolModal::None;
            // the action follows the save (a file chosen, a list of left-out tabs answered)
            self.project_save(Some(action));
        } else if dont {
            self.project.prompt = None;
            self.active_tool = ActiveToolModal::None;
            if action == AfterPrompt::Quit {
                self.project.quit_now = true;
            } else {
                self.project_do(action);
            }
        } else if cancel {
            self.project.prompt = None;
            self.active_tool = ActiveToolModal::None;
        }
        ModalOutcome { close: false, step: None }
    }

    /// The recent projects, as a list.
    pub(crate) fn render_project_recent_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = ProjectRects::default();
        let mut chosen: Option<PathBuf> = None;
        let mut close = false;
        let recent = self.settings.recent_projects.clone();
        let title = format!("{}  {}", theme::ph::CLOCK_COUNTER_CLOCKWISE, RECENT_TITLE);
        theme::modal(&title).default_width(theme::DIALOG_W_PIVOT_CONVERT).show(ctx, |d| {
            d.body(|ui| {
                if recent.is_empty() {
                    ui.label(egui::RichText::new("No recent projects yet. Save a project and it is listed here.").color(tk.muted));
                }
                for p in &recent {
                    let path = PathBuf::from(p);
                    let b = ui.button(project_name(&path));
                    rects.buttons.push((project_name(&path), b.rect));
                    if b.clicked() {
                        chosen = Some(path.clone());
                    }
                    ui.add(egui::Label::new(egui::RichText::new(p).size(theme::TEXT_LABEL).color(tk.muted)).wrap());
                    ui.add_space(theme::SPACE_XS);
                }
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let c = ui.button("Close");
                    rects.buttons.push(("Close".to_string(), c.rect));
                    close = c.clicked();
                });
            });
        });
        self.project.rects = rects;
        if let Some(p) = chosen {
            self.active_tool = ActiveToolModal::None;
            self.project_request(AfterPrompt::OpenPath(p));
        } else if close {
            self.active_tool = ActiveToolModal::None;
        }
        ModalOutcome { close: false, step: None }
    }

    /// The main area of a tab that did not open: the reason, and what to do about it.
    pub(crate) fn render_unavailable_main(&mut self, ui: &mut egui::Ui) {
        let tk = *theme::tokens(self.dark_mode);
        let Some(tab) = self.tabs.get(self.active_tab) else { return };
        let TabSource::Unavailable { reason } = &tab.source else { return };
        ui.add_space(theme::SPACE_L);
        ui.add(egui::Label::new(egui::RichText::new(format!("'{}' could not be opened", tab.title)).heading().color(tk.ink)).wrap());
        ui.add_space(theme::SPACE_S);
        ui.add(egui::Label::new(egui::RichText::new(format!("Because {}.", reason.trim_end_matches('.'))).color(tk.ink)).wrap());
        ui.add_space(theme::SPACE_S);
        ui.add(egui::Label::new(egui::RichText::new("The project keeps how this tab is built. Fix the cause (relink the file in File \u{25B8} Open project\u{2026}, or open the project again) and it opens. Saving the project keeps it as it is.").color(tk.muted)).wrap());
    }

    pub(crate) fn active_is_unavailable(&self) -> bool {
        self.tabs.get(self.active_tab).is_some_and(|t| matches!(t.source, TabSource::Unavailable { .. }))
    }

    /// The window title for the active tab and the project.
    pub(crate) fn current_window_title(&self) -> String {
        let proj = self.project.active.then(|| (self.project.name.as_str(), self.project.dirty));
        project_window_title(proj, self.active_file_name().as_deref())
    }
}
