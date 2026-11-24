//! Named charts: `portable_data/charts/<name>.json` (Phase 7 C2). A file holds one `ChartSpec` and its
//! name, so a chart reloads identically. Errors are plain messages a banker can act on.

use super::spec::ChartSpec;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The file format version.
pub const CHART_FILE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ChartFile {
    version: u32,
    name: String,
    chart: ChartSpec,
}

/// The file of a named chart in `dir`.
pub fn chart_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(crate::paths::default_export_file_name(name, "json"))
}

/// True when a chart of this name is already saved.
pub fn exists(dir: &Path, name: &str) -> bool {
    chart_path(dir, name).exists()
}

/// Saves the chart (the folder is created). The name must not be blank.
pub fn save_chart(dir: &Path, name: &str, spec: &ChartSpec) -> Result<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        bail!("Give the chart a name before saving it.");
    }
    std::fs::create_dir_all(dir).with_context(|| format!("Could not create the folder {}", dir.display()))?;
    let path = chart_path(dir, name);
    let text = serde_json::to_string_pretty(&ChartFile { version: CHART_FILE_VERSION, name: name.to_string(), chart: spec.clone() }).context("Could not write the chart")?;
    // through a temporary file, so a crash never leaves half a chart
    let tmp = path.with_extension(format!("json.tmp{}", std::process::id()));
    std::fs::write(&tmp, text).with_context(|| format!("Could not save {}", path.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        anyhow::anyhow!("Could not save {}: {}", path.display(), e)
    })?;
    Ok(path)
}

/// Reads a saved chart: its name and the chart. A damaged file says which file and why.
pub fn load_chart(path: &Path) -> Result<(String, ChartSpec)> {
    let text = std::fs::read_to_string(path).with_context(|| format!("Could not read the chart file {}", path.display()))?;
    let f: ChartFile = serde_json::from_str(&text).with_context(|| format!("The chart file {} is damaged (line and column in the message)", path.display()))?;
    if f.version > CHART_FILE_VERSION {
        bail!("The chart file {} was saved by a newer MicroParquet (version {}). Update the program to open it.", path.display(), f.version);
    }
    let name = if f.name.is_empty() { path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default() } else { f.name };
    Ok((name, f.chart))
}

/// The saved charts, by name: (name, path), sorted by name. Files that cannot be read are skipped here
/// (opening one says why).
pub fn list_charts(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<(String, PathBuf)> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|p| (p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), p))
        .collect();
    v.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    v
}

/// The columns the chart uses that the tab does not have, for the message when a chart is opened on another tab.
pub fn missing_columns(spec: &ChartSpec, available: &[String]) -> Vec<String> {
    // a date part (W2) needs its date column, not a column of its own name
    let fields = spec.date_fields();
    let mut used: Vec<String> = fields.iter().map(|f| f.column.clone()).collect();
    if !spec.category.is_empty() {
        used.push(spec.category.clone());
    }
    used.extend(spec.series.clone());
    used.extend(spec.facet.clone());
    used.extend(spec.size.iter().filter_map(|m| m.column.clone()));
    for m in spec.measure_list() {
        used.extend(m.column);
        used.extend(m.weight);
    }
    used.retain(|c| !fields.iter().any(|f| &f.name == c));
    used.sort();
    used.dedup();
    used.into_iter().filter(|c| !available.contains(c)).collect()
}
