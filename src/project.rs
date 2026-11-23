//! Project files (Phase 8 W4): a `.mpproj` describes a session, not its data.
//!
//! It holds the tabs in order (stable keys `t1`, `t2`, …), the active tab, and for each tab how to
//! build it again: the file it was read from (path, path relative to the project, size, modified
//! time and blake3), a saved tool run whose source names are project keys, or a chart of another
//! tab. It reuses the Phase 6 machinery (`ToolRun`, `RunEnv`). Opening a project rebuilds the tabs
//! in dependency order: files, then runs, then charts.
//!
//! This module is the only place that knows the file format. The UI (File menu, Sources dialog) and
//! the headless mode (`--project`) both go through it, so they build the same tabs.

use crate::chart::ChartSpec;
use crate::models::*;
use crate::tool_runs::{RunEnv, RunTab, StackFileRun, ToolRun};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

/// The version of the file format this build writes and reads.
pub const PROJECT_VERSION: u32 = 1;
/// The extension of a project file.
pub const PROJECT_EXT: &str = "mpproj";
/// How many projects "Open recent project" lists.
pub const RECENT_MAX: usize = 8;
/// The folder of the projects, inside `portable_data`.
pub const PROJECTS_FOLDER: &str = "projects";

pub fn projects_dir() -> PathBuf {
    crate::rules::data_dir().join(PROJECTS_FOLDER)
}

// -------------------------------------------------------------------------------------- the file

/// A data file as the project knew it when it was saved.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct FileRef {
    /// The file's name ("Loans.csv").
    pub name: String,
    /// The absolute path when the project was saved.
    pub path: String,
    /// The path from the project file's folder to the file ("../data/Loans.csv"); empty when there is none.
    pub rel_path: String,
    pub size: u64,
    /// Modified time, in seconds since 1970.
    pub modified: i64,
    pub blake3: String,
}

/// A file with the column names it had: the names the tabs built on it read.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SourceFile {
    pub file: FileRef,
    /// The file's own column names (before any rename of the Schema box).
    pub columns: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct FileRecipe {
    pub source: SourceFile,
    pub is_parquet: bool,
    pub config: IngestionConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct RunRecipe {
    /// The tool run; its source names are project tab keys.
    pub run: ToolRun,
    /// A stack's files (in the order of `run`'s files), with their column names.
    pub stack_files: Vec<SourceFile>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct ChartRecipe {
    /// The key of the tab the chart draws.
    pub source: String,
    pub spec: ChartSpec,
}

/// How a tab is built again.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Recipe {
    File(FileRecipe),
    Run(RunRecipe),
    Chart(ChartRecipe),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProjectTab {
    pub key: String,
    pub title: String,
    pub recipe: Recipe,
    /// The tab's own steps (for a file tab all of them; for a tool's result the steps added after it).
    #[serde(default)]
    pub steps: Vec<TransformStep>,
    #[serde(default)]
    pub col_formats: BTreeMap<String, ColumnFormatSpec>,
    #[serde(default)]
    pub summary: BTreeMap<String, SummarySpec>,
    #[serde(default)]
    pub control_col: Option<String>,
    /// The side panel's chart choices for this tab (a `ChartSpec` as `settings.json` keeps it).
    #[serde(default)]
    pub chart: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Project {
    pub project_version: u32,
    pub app_version: String,
    pub saved_at: String,
    /// The key of the active tab ("" = none).
    pub active_tab: String,
    pub tabs: Vec<ProjectTab>,
}

impl Default for Project {
    fn default() -> Self {
        Self { project_version: PROJECT_VERSION, app_version: env!("CARGO_PKG_VERSION").to_string(), saved_at: String::new(), active_tab: String::new(), tabs: Vec::new() }
    }
}

fn sorted_value(v: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Object(m) => {
            let mut entries: Vec<(String, Value)> = m.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(entries.into_iter().map(|(k, v)| (k, sorted_value(v))).collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sorted_value).collect()),
        other => other,
    }
}

impl Project {
    /// The file's text: keys in alphabetical order, so saving the same project twice gives the same bytes.
    pub fn to_json(&self) -> Result<String> {
        let v = serde_json::to_value(self).context("The project could not be written")?;
        let mut text = serde_json::to_string_pretty(&sorted_value(v)).context("The project could not be written")?;
        text.push('\n');
        Ok(text)
    }

    /// Reads a project from its text. A newer project version is refused before anything else is read.
    pub fn from_json(text: &str, name: &str) -> Result<Project> {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| anyhow!("The project file {} is not valid: {}.", name, e))?;
        let ver = v.get("project_version").and_then(|x| x.as_u64()).unwrap_or(PROJECT_VERSION as u64);
        if ver > PROJECT_VERSION as u64 {
            let by = v.get("app_version").and_then(|x| x.as_str()).filter(|s| !s.is_empty()).map(|s| format!("v{}", s)).unwrap_or_else(|| format!("(project version {})", ver));
            bail!("This project was saved by a newer MicroParquet {}. Update the app to open it.", by);
        }
        serde_json::from_value::<Project>(v).map_err(|e| anyhow!("The project file {} is not valid: {}.", name, e))
    }

    pub fn tab(&self, key: &str) -> Option<&ProjectTab> {
        self.tabs.iter().find(|t| t.key == key)
    }
}

/// Reads a project file.
pub fn load_project(path: &Path) -> Result<Project> {
    let text = std::fs::read_to_string(path).with_context(|| format!("Could not read the project file {}", path.display()))?;
    Project::from_json(&text, &path.display().to_string())
}

/// Writes a project atomically: a temporary file in the same folder, then a rename.
pub fn write_project(path: &Path, project: &Project) -> Result<()> {
    let text = project.to_json()?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).with_context(|| format!("Could not create the folder {}", dir.display()))?;
    }
    let tmp = path.with_extension(format!("{}.tmp{}", PROJECT_EXT, std::process::id()));
    std::fs::write(&tmp, text).with_context(|| format!("Could not write the project {}", path.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        anyhow!("Could not write the project {}: {}", path.display(), e)
    })
}

/// "Treasury Oct" for `…/Treasury Oct.mpproj`.
pub fn project_name(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "Project".to_string())
}

/// `path` at the front of the recent list, without a duplicate, at most `RECENT_MAX` long.
pub fn push_recent(recent: &mut Vec<String>, path: &Path) {
    let p = path.display().to_string();
    recent.retain(|r| r != &p);
    recent.insert(0, p);
    recent.truncate(RECENT_MAX);
}

// -------------------------------------------------------------------------------------- paths

fn absolute(path: &Path) -> PathBuf {
    let p = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().map(|d| d.join(path)).unwrap_or_else(|_| path.to_path_buf()) };
    // `.` and `..` are resolved by the text (the file need not exist)
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The folder a project file is in.
pub fn project_dir_of(path: &Path) -> PathBuf {
    let abs = absolute(path);
    abs.parent().map(Path::to_path_buf).unwrap_or(abs)
}

/// The path from `base_dir` to `target` ("../data/Loans.csv"), or `None` when they have no common root (two drives).
pub fn relative_to(base_dir: &Path, target: &Path) -> Option<String> {
    let (b, t) = (absolute(base_dir), absolute(target));
    let bc: Vec<Component> = b.components().collect();
    let tc: Vec<Component> = t.components().collect();
    if bc.first() != tc.first() {
        return None;
    }
    let common = bc.iter().zip(&tc).take_while(|(x, y)| x == y).count();
    let mut parts: Vec<String> = bc[common..].iter().map(|_| "..".to_string()).collect();
    parts.extend(tc[common..].iter().map(|c| c.as_os_str().to_string_lossy().to_string()));
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

/// Size and modified time (seconds since 1970) of a file.
pub fn file_stamp(path: &Path) -> Result<(u64, i64)> {
    let md = std::fs::metadata(path).with_context(|| format!("Could not read {}", path.display()))?;
    let secs = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64).or_else(|| std::time::UNIX_EPOCH.duration_since(t).ok().map(|d| -(d.as_secs() as i64))))
        .unwrap_or(0);
    Ok((md.len(), secs))
}

/// "31 Oct" for a modified time.
pub fn short_date(secs: i64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_opt(secs, 0).single().map(|d| d.format("%-d %b").to_string()).unwrap_or_else(|| "?".to_string())
}

/// Hashes already worked out, by file: (size, modified time, blake3). A file is hashed again only when its size or time changed.
pub type HashCache = HashMap<PathBuf, (u64, i64, String)>;

fn hashed(path: &Path, size: u64, modified: i64, cache: &mut HashCache) -> Result<String> {
    if let Some((s, m, h)) = cache.get(path) {
        if *s == size && *m == modified {
            return Ok(h.clone());
        }
    }
    let h = crate::packs::file_hash(path)?;
    cache.insert(path.to_path_buf(), (size, modified, h.clone()));
    Ok(h)
}

/// The names of the columns a file has, read the way a tab reads it.
pub fn file_columns(path: &Path, is_parquet: bool, config: &IngestionConfig) -> Result<Vec<String>> {
    let df = if is_parquet { crate::engine::preview_parquet_raw(path, 0)? } else { crate::engine::preview_csv_with(path, 0, config)? };
    Ok(df.get_column_names().iter().map(|n| n.to_string()).collect())
}

fn describe_file(path: &Path, is_parquet: bool, config: &IngestionConfig, project_dir: &Path, cache: &mut HashCache) -> Result<SourceFile> {
    let (size, modified) = file_stamp(path).map_err(|_| anyhow!("the file {} is not there", path.display()))?;
    let blake3 = hashed(path, size, modified, cache)?;
    let columns = if !is_parquet && !config.columns.is_empty() { config.columns.iter().map(|c| c.name.clone()).collect() } else { file_columns(path, is_parquet, config)? };
    let abs = absolute(path);
    Ok(SourceFile {
        file: FileRef {
            name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            path: abs.display().to_string(),
            rel_path: relative_to(project_dir, &abs).unwrap_or_default(),
            size,
            modified,
            blake3,
        },
        columns,
    })
}

// -------------------------------------------------------------------------------------- saving

/// One open tab, as the project is made from it.
#[derive(Clone, Debug)]
pub struct TabSnapshot {
    pub id: u64,
    /// The key the tab has in the project already (`None` = a new tab).
    pub key: Option<String>,
    pub title: String,
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    /// How many of the steps run (the rest are broken).
    pub exec_steps: usize,
    pub col_formats: HashMap<String, ColumnFormatSpec>,
    pub summary: HashMap<String, SummarySpec>,
    pub control_col: Option<String>,
    /// The side panel's chart choices for the tab.
    pub chart: Option<serde_json::Value>,
    /// A chart tab: its chart and the id of the tab it draws.
    pub chart_tab: Option<(ChartSpec, u64)>,
    /// A tab that did not open: the recipe it was to be built from, kept so saving does not lose it.
    pub kept: Option<ProjectTab>,
}

/// A tab left out of the project, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct Excluded {
    pub title: String,
    pub reason: String,
}

pub struct SavedProject {
    pub project: Project,
    pub excluded: Vec<Excluded>,
    /// The key each tab has now (id, key), new keys included.
    pub keys: Vec<(u64, String)>,
}

fn key_of(source: &TabSource, steps: &[TransformStep]) -> String {
    format!("{:?}|{:?}", source, steps)
}

/// The tabs of a project seen as the sources of a run: by key.
pub struct ProjectEnv {
    pub tabs: Vec<(String, RunTab)>,
    pub paths: PortablePaths,
    /// Keys that did not open, with the reason.
    pub failed: HashMap<String, String>,
}

impl RunEnv for ProjectEnv {
    fn tab(&self, name: &str) -> Result<RunTab> {
        if let Some(why) = self.failed.get(name) {
            bail!("it is built on the tab '{}', which did not open: {}", name, why);
        }
        self.tabs.iter().find(|(k, _)| k == name).map(|(_, t)| t.clone()).ok_or_else(|| anyhow!("it is built on the tab '{}', which is not in the project", name))
    }

    fn name_of(&self, source: &TabSource, steps: &[TransformStep]) -> Result<String> {
        let want = key_of(source, steps);
        self.tabs
            .iter()
            .find(|(_, t)| key_of(&t.source, &t.steps) == want)
            .map(|(k, _)| k.clone())
            .ok_or_else(|| anyhow!("The tab this result was made from is not open any more, or its steps changed since. Open it again (with the same steps) and save the project again."))
    }

    fn paths(&self) -> &PortablePaths {
        &self.paths
    }
}

/// Why a tab that reads a temporary file cannot be written to a project.
fn temp_file_reason(path: &Path, temp_dir: &Path) -> Option<String> {
    if !path.starts_with(temp_dir) {
        return None;
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    Some(if name.starts_with("chart_data_") {
        "Chart data tab: it is a copy of a chart's table. Draw the chart again from its source tab.".to_string()
    } else if name.starts_with("chart_template_") {
        "Template chart tab: it holds a table made for the chart. Draw the chart again from its source tab.".to_string()
    } else {
        "This tab reads a temporary file the program made. Save its result as a file first, or add the steps that make it.".to_string()
    })
}

fn steps_of(s: &TabSnapshot) -> Vec<TransformStep> {
    s.steps[..s.exec_steps.min(s.steps.len())].to_vec()
}

/// Makes the project from the open tabs. A tab that cannot be built again is left out with the reason;
/// a tab built on one that is left out is left out too.
pub fn build_project(tabs: &[TabSnapshot], active: Option<u64>, project_dir: &Path, paths: &PortablePaths, cache: &mut HashCache) -> Result<SavedProject> {
    // keys: those the tabs have, then the next numbers
    let mut used: HashSet<String> = tabs.iter().filter_map(|t| t.key.clone()).collect();
    let mut next = used.iter().filter_map(|k| k.strip_prefix('t').and_then(|n| n.parse::<u32>().ok())).max().unwrap_or(0);
    let keys: Vec<String> = tabs
        .iter()
        .map(|t| match &t.key {
            Some(k) => k.clone(),
            None => {
                let mut k;
                loop {
                    next += 1;
                    k = format!("t{}", next);
                    if used.insert(k.clone()) {
                        break;
                    }
                }
                k
            }
        })
        .collect();
    let key_of_id = |id: u64| tabs.iter().position(|t| t.id == id).map(|i| keys[i].clone());

    // every tab is a possible source of a run, so names are looked up among all of them
    let env = ProjectEnv {
        tabs: tabs.iter().zip(&keys).map(|(t, k)| (k.clone(), RunTab { source: t.source.clone(), steps: steps_of(t) })).chain(tabs.iter().zip(&keys).map(|(t, k)| (k.clone(), RunTab { source: t.source.clone(), steps: t.steps.clone() }))).collect(),
        paths: paths.clone(),
        failed: HashMap::new(),
    };

    let mut recipes: Vec<std::result::Result<Recipe, String>> = Vec::new();
    for (t, _) in tabs.iter().zip(&keys) {
        recipes.push(recipe_of(t, &key_of_id, &env, project_dir, paths, cache));
    }

    // a tab built on a tab that is left out is left out
    loop {
        let mut changed = false;
        for i in 0..tabs.len() {
            let Ok(r) = &recipes[i] else { continue };
            let deps: Vec<String> = match r {
                Recipe::File(_) => Vec::new(),
                Recipe::Run(r) => r.run.source_names(),
                Recipe::Chart(c) => vec![c.source.clone()],
            };
            let bad = deps.iter().find_map(|d| keys.iter().position(|k| k == d).filter(|j| recipes[*j].is_err()).map(|j| (d.clone(), tabs[j].title.clone())));
            if let Some((_, title)) = bad {
                recipes[i] = Err(format!("it is built on '{}', which cannot be saved", title));
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut project = Project { saved_at: chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false), ..Project::default() };
    let mut excluded = Vec::new();
    for (i, t) in tabs.iter().enumerate() {
        match std::mem::replace(&mut recipes[i], Err(String::new())) {
            Ok(recipe) => {
                // a tab that did not open keeps the recipe it had, as it was written
                if let Some(k) = &t.kept {
                    project.tabs.push(ProjectTab { key: keys[i].clone(), title: t.title.clone(), ..k.clone() });
                    continue;
                }
                project.tabs.push(ProjectTab {
                    key: keys[i].clone(),
                    title: t.title.clone(),
                    recipe,
                    steps: if t.chart_tab.is_some() { Vec::new() } else { t.steps.clone() },
                    col_formats: t.col_formats.iter().map(|(k, v)| (k.clone(), *v)).collect(),
                    summary: t.summary.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                    control_col: t.control_col.clone(),
                    chart: t.chart.clone(),
                });
            }
            Err(reason) => excluded.push(Excluded { title: t.title.clone(), reason }),
        }
    }
    project.active_tab = active.and_then(|id| key_of_id(id)).filter(|k| project.tabs.iter().any(|t| &t.key == k)).unwrap_or_default();
    Ok(SavedProject { project, excluded, keys: tabs.iter().zip(keys).map(|(t, k)| (t.id, k)).collect() })
}

fn recipe_of(t: &TabSnapshot, key_of_id: &dyn Fn(u64) -> Option<String>, env: &ProjectEnv, project_dir: &Path, paths: &PortablePaths, cache: &mut HashCache) -> std::result::Result<Recipe, String> {
    if let Some(k) = &t.kept {
        return Ok(k.recipe.clone());
    }
    if let Some((spec, source_id)) = &t.chart_tab {
        let Some(source) = key_of_id(*source_id) else {
            let what = t.title.trim_start_matches("Chart: ").to_string();
            return Err(format!("the tab it draws is closed. Open that tab again and draw the chart '{}' again", what));
        };
        return Ok(Recipe::Chart(ChartRecipe { source, spec: spec.clone() }));
    }
    match &t.source {
        TabSource::Unavailable { reason } => Err(format!("it did not open and has no recipe to keep: {}", reason)),
        TabSource::SingleFile { path, is_parquet, config } => {
            if let Some(why) = temp_file_reason(path, &paths.temp_dir) {
                return Err(why);
            }
            let source = describe_file(path, *is_parquet, config, project_dir, cache).map_err(|e| format!("{:#}", e))?;
            Ok(Recipe::File(FileRecipe { source, is_parquet: *is_parquet, config: config.clone() }))
        }
        source => {
            // a tool's result: its run, with the sources named by key; the tab's own steps are kept apart
            let run = ToolRun::from_source(source, &[], env).map_err(|e| format!("{:#}", e))?;
            let mut stack_files = Vec::new();
            if let ToolRun::Stack(s) = &run {
                for f in &s.files {
                    if let Some(why) = temp_file_reason(&f.path, &paths.temp_dir) {
                        return Err(why);
                    }
                    stack_files.push(describe_file(&f.path, f.is_parquet, &f.config, project_dir, cache).map_err(|e| format!("{:#}", e))?);
                }
            }
            Ok(Recipe::Run(RunRecipe { run, stack_files }))
        }
    }
}

// ----------------------------------------------------------------------------------- the sources

/// Where a project's file is now.
#[derive(Clone, Debug, PartialEq)]
pub enum SourceStatus {
    /// There, with the size and modified time the project saved.
    Found,
    /// There, but its size or time differs: (size, modified) saved and now.
    Changed { saved: (u64, i64), now: (u64, i64) },
    Missing,
}

/// One row of the Sources dialog.
#[derive(Clone, Debug)]
pub struct SourceRow {
    pub name: String,
    /// The path the project saved.
    pub saved_path: String,
    /// Where the file is read from now (the saved place, the place relative to the project, or the file it was relinked to).
    pub path: PathBuf,
    pub status: SourceStatus,
    pub relinked: bool,
    /// The titles of the tabs that read it.
    pub tabs: Vec<String>,
}

fn source_files_of(tab: &ProjectTab) -> Vec<&SourceFile> {
    match &tab.recipe {
        Recipe::File(f) => vec![&f.source],
        Recipe::Run(r) => r.stack_files.iter().collect(),
        Recipe::Chart(_) => Vec::new(),
    }
}

/// Where a saved file is now: the saved path, else the path relative to the project folder.
pub fn locate(file: &FileRef, project_dir: &Path, relinks: &BTreeMap<String, PathBuf>) -> (PathBuf, bool) {
    if let Some(p) = relinks.get(&file.path) {
        return (p.clone(), true);
    }
    let saved = PathBuf::from(&file.path);
    if saved.is_file() {
        return (saved, false);
    }
    if !file.rel_path.is_empty() {
        let rel = absolute(&project_dir.join(&file.rel_path));
        if rel.is_file() {
            return (rel, false);
        }
    }
    (saved, false)
}

/// One row per file of the project, in the order the tabs first read them.
pub fn source_rows(project: &Project, project_dir: &Path, relinks: &BTreeMap<String, PathBuf>) -> Vec<SourceRow> {
    let mut rows: Vec<SourceRow> = Vec::new();
    for tab in &project.tabs {
        for sf in source_files_of(tab) {
            if let Some(r) = rows.iter_mut().find(|r| r.saved_path == sf.file.path) {
                if !r.tabs.contains(&tab.title) {
                    r.tabs.push(tab.title.clone());
                }
                continue;
            }
            let (path, relinked) = locate(&sf.file, project_dir, relinks);
            let status = match file_stamp(&path) {
                Ok(now) if path.is_file() => {
                    if now == (sf.file.size, sf.file.modified) {
                        SourceStatus::Found
                    } else {
                        SourceStatus::Changed { saved: (sf.file.size, sf.file.modified), now }
                    }
                }
                _ => SourceStatus::Missing,
            };
            rows.push(SourceRow { name: sf.file.name.clone(), saved_path: sf.file.path.clone(), path, status, relinked, tabs: vec![tab.title.clone()] });
        }
    }
    rows
}

/// Relinks every missing file to the file of the same name in `folder` (searched below it too). Returns how many were found.
pub fn look_in_folder(rows: &[SourceRow], folder: &Path, relinks: &mut BTreeMap<String, PathBuf>) -> usize {
    let mut found = 0;
    for r in rows.iter().filter(|r| r.status == SourceStatus::Missing && !r.name.is_empty()) {
        let direct = folder.join(&r.name);
        let hit = if direct.is_file() {
            Some(direct)
        } else {
            walkdir::WalkDir::new(folder).max_depth(6).into_iter().flatten().find(|e| e.file_type().is_file() && e.file_name().to_string_lossy() == r.name).map(|e| e.path().to_path_buf())
        };
        if let Some(p) = hit {
            relinks.insert(r.saved_path.clone(), p);
            found += 1;
        }
    }
    found
}

/// "Loans.csv changed since the project was saved (saved 31 Oct, file 30 Nov)" for each file whose content
/// is not the content the project saved. Files that were relinked are left out: a different file is the point of relinking.
/// Returns the warnings and the hashes worked out (for the cache).
pub fn changed_since_saved(project: &Project, project_dir: &Path, relinks: &BTreeMap<String, PathBuf>, cancelled: &dyn Fn() -> bool) -> (Vec<String>, Vec<(PathBuf, u64, i64, String)>) {
    let mut warnings = Vec::new();
    let mut hashes = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for tab in &project.tabs {
        for sf in source_files_of(tab) {
            if cancelled() {
                return (warnings, hashes);
            }
            if sf.file.blake3.is_empty() || !seen.insert(sf.file.path.clone()) {
                continue;
            }
            let (path, relinked) = locate(&sf.file, project_dir, relinks);
            if relinked || !path.is_file() {
                continue;
            }
            let Ok((size, modified)) = file_stamp(&path) else { continue };
            let Ok(h) = crate::packs::file_hash(&path) else { continue };
            hashes.push((path.clone(), size, modified, h.clone()));
            if h != sf.file.blake3 {
                warnings.push(format!("{} changed since the project was saved (saved {}, file {})", sf.file.name, short_date(sf.file.modified), short_date(modified)));
            }
        }
    }
    (warnings, hashes)
}

// -------------------------------------------------------------------------------------- opening

/// What one tab is built from.
pub enum Prepared {
    /// A table: a source and its steps (the tab's own steps, after the source).
    Table { source: TabSource, steps: Vec<TransformStep> },
    /// A chart tab: the chart of `spec` over the source tab's table.
    Chart { source: TabSource, steps: Vec<TransformStep>, spec: ChartSpec, source_key: String },
}

/// Opens a project tab by tab, in dependency order. The caller builds each tab (the app on its worker,
/// the headless mode inline) and tells the opener how it went, so the tabs built on it can follow.
pub struct Opener {
    pub project: Project,
    project_dir: PathBuf,
    relinks: BTreeMap<String, PathBuf>,
    env: ProjectEnv,
    file_checks: HashMap<String, std::result::Result<(), String>>,
}

fn rank(t: &ProjectTab) -> u8 {
    match t.recipe {
        Recipe::File(_) => 0,
        Recipe::Run(_) => 1,
        Recipe::Chart(_) => 2,
    }
}

impl Opener {
    pub fn new(project: Project, project_path: &Path, relinks: BTreeMap<String, PathBuf>, paths: PortablePaths) -> Self {
        Self { project, project_dir: project_dir_of(project_path), relinks, env: ProjectEnv { tabs: Vec::new(), paths, failed: HashMap::new() }, file_checks: HashMap::new() }
    }

    /// The order the tabs are built in: files, then runs, then charts, and a tab after the tabs it is built on.
    pub fn order(&self) -> Vec<usize> {
        let tabs = &self.project.tabs;
        let deps = |t: &ProjectTab| -> Vec<String> {
            match &t.recipe {
                Recipe::File(_) => Vec::new(),
                Recipe::Run(r) => r.run.source_names(),
                Recipe::Chart(c) => vec![c.source.clone()],
            }
        };
        let mut done: HashSet<String> = HashSet::new();
        let mut order: Vec<usize> = Vec::new();
        let mut left: Vec<usize> = (0..tabs.len()).collect();
        while !left.is_empty() {
            let pick = left
                .iter()
                .copied()
                .filter(|i| deps(&tabs[*i]).iter().all(|d| done.contains(d) || !tabs.iter().any(|t| &t.key == d)))
                .min_by_key(|i| (rank(&tabs[*i]), *i));
            // a loop (a damaged file): the rest follow in file order and fail with their reason
            let i = pick.unwrap_or(left[0]);
            left.retain(|x| *x != i);
            done.insert(tabs[i].key.clone());
            order.push(i);
        }
        order
    }

    fn check_file(&mut self, sf: &SourceFile, is_parquet: bool, config: &IngestionConfig) -> std::result::Result<PathBuf, String> {
        let (path, _) = locate(&sf.file, &self.project_dir, &self.relinks);
        let name = if sf.file.name.is_empty() { path.display().to_string() } else { sf.file.name.clone() };
        let res = self
            .file_checks
            .entry(path.display().to_string())
            .or_insert_with(|| {
                if !path.is_file() {
                    return Err(format!("the file {} is missing ({}). Relink it in the Sources dialog", name, sf.file.path));
                }
                let have = file_columns(&path, is_parquet, config).map_err(|e| format!("the file {} could not be read: {:#}", name, e))?;
                let missing: Vec<&String> = sf.columns.iter().filter(|c| !have.contains(c)).collect();
                if missing.is_empty() {
                    Ok(())
                } else {
                    let list = missing.iter().map(|c| format!("'{}'", c)).collect::<Vec<_>>().join(", ");
                    Err(format!("the file has no {} {}. Columns in the file: {}", list, if missing.len() == 1 { "column" } else { "columns" }, have.join(", ")))
                }
            })
            .clone();
        res.map(|_| path)
    }

    /// What tab `index` is built from, or why it cannot be built.
    pub fn prepare(&mut self, index: usize) -> std::result::Result<Prepared, String> {
        let Some(tab) = self.project.tabs.get(index).cloned() else {
            return Err("there is no such tab in the project".to_string());
        };
        match &tab.recipe {
            Recipe::File(f) => {
                let path = self.check_file(&f.source, f.is_parquet, &f.config)?;
                Ok(Prepared::Table { source: TabSource::SingleFile { path, is_parquet: f.is_parquet, config: f.config.clone() }, steps: tab.steps.clone() })
            }
            Recipe::Run(r) => {
                let mut run = r.run.clone();
                if let ToolRun::Stack(s) = &mut run {
                    let mut files: Vec<StackFileRun> = Vec::new();
                    for (i, f) in s.files.iter().enumerate() {
                        let Some(sf) = r.stack_files.get(i) else {
                            return Err("the stack's files are not all in the project".to_string());
                        };
                        let path = self.check_file(sf, f.is_parquet, &f.config)?;
                        files.push(StackFileRun { path, is_parquet: f.is_parquet, config: f.config.clone() });
                    }
                    s.files = files;
                }
                let mut sources = run.to_sources(&self.env).map_err(|e| format!("{:#}", e))?;
                if sources.is_empty() {
                    return Err("the saved run makes no tab".to_string());
                }
                let (_, source) = sources.remove(0);
                let mut steps = run.tab_steps(&self.env).map_err(|e| format!("{:#}", e))?;
                steps.extend(tab.steps.clone());
                Ok(Prepared::Table { source, steps })
            }
            Recipe::Chart(c) => {
                let src = self.env.tab(&c.source).map_err(|e| format!("{:#}", e))?;
                Ok(Prepared::Chart { source: src.source, steps: src.steps, spec: c.spec.clone(), source_key: c.source.clone() })
            }
        }
    }

    /// The tab was built: tabs built on it may use it, with the steps that run.
    pub fn built(&mut self, key: &str, source: TabSource, exec_steps: Vec<TransformStep>) {
        self.env.failed.remove(key);
        self.env.tabs.retain(|(k, _)| k != key);
        self.env.tabs.push((key.to_string(), RunTab { source, steps: exec_steps }));
    }

    /// The tab could not be built: tabs built on it fail with this reason.
    pub fn failed(&mut self, key: &str, reason: &str) {
        self.env.tabs.retain(|(k, _)| k != key);
        self.env.failed.insert(key.to_string(), reason.to_string());
    }
}

/// "9 tabs opened, 1 failed: Gap ladder — the file has no 'MATDAT' column".
pub fn opened_summary(opened: usize, failures: &[(String, String)]) -> String {
    let tabs = |n: usize| if n == 1 { "1 tab".to_string() } else { format!("{} tabs", n) };
    if failures.is_empty() {
        return format!("{} opened.", tabs(opened));
    }
    let list = failures.iter().map(|(t, r)| format!("{} \u{2014} {}", t, r)).collect::<Vec<_>>().join("; ");
    format!("{} opened, {} failed: {}", tabs(opened), failures.len(), list)
}

// ------------------------------------------------------------------------------------- the chart tab

/// The numbers of a chart over a tab's table, and the table of them as a tab's source (a Parquet file in `temp_dir`).
pub fn chart_tab_source(base: &TabSource, steps: &[TransformStep], spec: &ChartSpec, temp_dir: &Path) -> Result<(crate::chart::ChartData, TabSource)> {
    let lf = crate::engine::compile_tab_lazy(base, steps, temp_dir)?;
    let data = crate::charts::calc::compute(crate::pivot_table::without_total_rows(lf, steps), spec)?;
    let mut df = crate::chart::chart_table(&data)?;
    std::fs::create_dir_all(temp_dir)?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let path = temp_dir.join(format!("chart_tab_{}_{}.parquet", std::process::id(), stamp));
    let f = std::fs::File::create(&path)?;
    polars::prelude::ParquetWriter::new(f).finish(&mut df)?;
    Ok((data, TabSource::SingleFile { path, is_parquet: true, config: IngestionConfig::default() }))
}

// ------------------------------------------------------------------------------------- headless

/// A file name for a tab's Parquet export: the title with what a file name cannot hold left out; the same
/// title twice gets "(2)".
pub fn export_file_names(titles: &[String]) -> Vec<String> {
    let mut used: HashSet<String> = HashSet::new();
    titles
        .iter()
        .map(|t| {
            let base = crate::paths::sanitize_file_name(t);
            let base = if base.trim().is_empty() { "tab".to_string() } else { base };
            let mut name = format!("{}.parquet", base);
            let mut n = 1;
            while !used.insert(name.to_lowercase()) {
                n += 1;
                name = format!("{} ({}).parquet", base, n);
            }
            name
        })
        .collect()
}

/// `--project <file> --export-dir <folder>`: opens the project (no relinking) and writes every tab as Parquet.
/// A tab that fails stops the run, and nothing is written. Returns the files written.
pub fn export_project_headless(project_path: &Path, export_dir: &Path, paths: &PortablePaths) -> Result<Vec<PathBuf>> {
    let project = load_project(project_path)?;
    if project.tabs.is_empty() {
        bail!("The project {} has no tabs.", project_path.display());
    }
    let mut opener = Opener::new(project.clone(), project_path, BTreeMap::new(), paths.clone());
    let order = opener.order();
    // each tab is built and written to a staging folder; the files move to `export_dir` only when every tab worked
    let staging = export_dir.join(format!(".mpproj-staging-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).with_context(|| format!("Could not create the folder {}", staging.display()))?;
    let titles: Vec<String> = project.tabs.iter().map(|t| t.title.clone()).collect();
    let names = export_file_names(&titles);
    let result = (|| -> Result<Vec<(usize, PathBuf)>> {
        let mut written = Vec::new();
        for i in order {
            let tab = &project.tabs[i];
            let fail = |why: &str| anyhow!("The tab '{}' could not be opened: {}", tab.title, why);
            let prepared = opener.prepare(i).map_err(|e| fail(&e))?;
            let (source, steps) = match prepared {
                Prepared::Table { source, steps } => (source, steps),
                Prepared::Chart { source, steps, spec, .. } => {
                    let (_, src) = chart_tab_source(&source, &steps, &spec, &paths.temp_dir).map_err(|e| fail(&format!("{:#}", e)))?;
                    (src, Vec::new())
                }
            };
            let issues = crate::engine::validate_steps(&source, &steps, &paths.temp_dir).map_err(|e| fail(&format!("{:#}", e)))?;
            if let Some(first) = issues.first() {
                let n = first.index + 1;
                return Err(fail(&format!("step {} cannot run: {}", n, first.reason)));
            }
            let out = staging.join(&names[i]);
            crate::jobs::run_inline(|ctx| crate::tab_jobs::export_tab_parquet(&source, &steps, &paths.temp_dir, &out, ParquetDialect::ArchiveZstd, None, ctx)).map_err(|e| fail(&format!("{:#}", e)))?;
            opener.built(&tab.key, source, steps);
            written.push((i, out));
        }
        Ok(written)
    })();
    match result {
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            Err(e)
        }
        Ok(written) => {
            let mut done = Vec::new();
            for (i, from) in &written {
                let to = export_dir.join(&names[*i]);
                std::fs::rename(from, &to).with_context(|| format!("Could not write {}", to.display()))?;
                done.push(to);
            }
            let _ = std::fs::remove_dir_all(&staging);
            done.sort();
            Ok(done)
        }
    }
}
