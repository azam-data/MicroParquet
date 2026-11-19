//! Deposit and loan betas (Phase 4 V5): `portable_data/betas/<name>.json` maps product codes to
//! the share, in percent, of a market-rate change that the product's own rate follows.
//!
//! ```json
//! { "source": "ALCO paper 3/2026", "betas": [ { "product": "SAVINGS", "beta_pct": 40 }, { "product": "FDR", "beta_pct": 100 } ] }
//! ```
//!
//! A product may also have its own `beta_up_pct` (used when the shock raises rates) and
//! `beta_down_pct` (when it lowers them); each defaults to `beta_pct`, so a file with only
//! `beta_pct` gives what it always gave. `beta_pct` may be left out when both are given.
//!
//! Codes are matched trimmed and upper-case with spaces collapsed, as the CL codes are. A product
//! that is not in the file passes through 100%; the status bar counts those rows.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BetaEntry {
    pub product: String,
    /// The single pass-through, up and down. May be left out when both `beta_up_pct` and `beta_down_pct` are given.
    #[serde(default)]
    pub beta_pct: Option<f64>,
    /// Used when the shock raises rates (defaults to `beta_pct`).
    #[serde(default, alias = "beta_up")]
    pub beta_up_pct: Option<f64>,
    /// Used when the shock lowers rates (defaults to `beta_pct`).
    #[serde(default, alias = "beta_down")]
    pub beta_down_pct: Option<f64>,
}

/// One product's pass-through as fractions of a change (0.4 = 40%).
#[derive(Clone, Debug, PartialEq)]
pub struct BetaRow {
    /// The normalised product code.
    pub product: String,
    /// The single beta (`beta_pct`, or the up beta when only up and down are given).
    pub base: f64,
    pub up: f64,
    pub down: f64,
}

/// The JSON of a beta file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct BetaFile {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub betas: Vec<BetaEntry>,
}

thread_local! {
    static BETAS_DIR_OVERRIDE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Makes this thread read beta files from `dir` instead of `portable_data/betas`.
pub fn set_betas_dir_override(dir: Option<PathBuf>) {
    BETAS_DIR_OVERRIDE.with(|d| *d.borrow_mut() = dir);
}

/// Where the beta files are.
pub fn betas_dir() -> PathBuf {
    BETAS_DIR_OVERRIDE.with(|d| d.borrow().clone()).unwrap_or_else(|| crate::models::portable_root_dir().join("betas"))
}

/// The names (without `.json`) of the beta files in `dir`, in order.
pub fn list_beta_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let p = e.path();
                    (p.extension().map(|x| x.eq_ignore_ascii_case("json")).unwrap_or(false)).then(|| p.file_stem().and_then(|s| s.to_str()).map(str::to_string)).flatten()
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// A product code as it is matched.
pub fn normalise_product(s: &str) -> String {
    crate::banking::normalise_code(s)
}

/// Checks the text of a beta file and returns its entries with the up and down betas as fractions.
pub fn parse_beta_rows(name: &str, text: &str) -> Result<Vec<BetaRow>> {
    let f: BetaFile = serde_json::from_str(text).map_err(|e| anyhow!("The beta file {} is not valid: {}. Expected {{\"betas\": [{{\"product\": \"SAVINGS\", \"beta_pct\": 40}}]}}.", name, e))?;
    if f.betas.is_empty() {
        bail!("The beta file {} lists no products. Add at least one {{\"product\": \"...\", \"beta_pct\": 0-100}}.", name);
    }
    let mut out: Vec<BetaRow> = Vec::new();
    for e in &f.betas {
        let key = normalise_product(&e.product);
        if key.is_empty() {
            bail!("The beta file {} has an entry with no product code.", name);
        }
        let check = |label: &str, v: f64| -> Result<f64> {
            if !v.is_finite() || !(0.0..=100.0).contains(&v) {
                bail!("The beta file {}: the {} of '{}' is {}, not a percentage from 0 to 100.", name, label, e.product, v);
            }
            Ok(v / 100.0)
        };
        let base = e.beta_pct.map(|v| check("beta", v)).transpose()?;
        let up = e.beta_up_pct.map(|v| check("beta_up", v)).transpose()?;
        let down = e.beta_down_pct.map(|v| check("beta_down", v)).transpose()?;
        let (Some(u), Some(d)) = (up.or(base), down.or(base)) else {
            bail!("The beta file {}: '{}' needs beta_pct, or both beta_up_pct and beta_down_pct.", name, e.product);
        };
        if out.iter().any(|r| r.product == key) {
            bail!("The beta file {} lists '{}' twice (codes are matched ignoring case and extra spaces). Keep one.", name, e.product);
        }
        out.push(BetaRow { product: key, base: base.unwrap_or(u), up: u, down: d });
    }
    Ok(out)
}

/// The single beta of each product (`beta_pct`) as (normalised product, fraction); what a file
/// without up and down betas gives for both directions.
pub fn parse_beta_file(name: &str, text: &str) -> Result<Vec<(String, f64)>> {
    Ok(parse_beta_rows(name, text)?.into_iter().map(|r| (r.product, r.base)).collect())
}

fn read_beta_text(dir: &Path, name: &str) -> Result<(String, String)> {
    let n = name.trim();
    if n.is_empty() || n.contains(['/', '\\']) || n.contains("..") {
        bail!("'{}' is not a beta file name. Use the name of a file in portable_data/betas, without the folder or .json.", name);
    }
    let path = dir.join(format!("{}.json", n));
    if !path.exists() {
        bail!("The beta file '{}' was not found in {}. Add {}.json there, or choose another file.", n, dir.display(), n);
    }
    let text = std::fs::read_to_string(&path).with_context(|| format!("Could not read the beta file {}", path.display()))?;
    crate::rules::note_used_file("Beta file", &format!("betas/{}.json", n), "", "", false);
    Ok((format!("{}.json", n), text))
}

/// Reads `<dir>/<name>.json` with its up and down betas. `name` is a file name without folders.
pub fn load_beta_rows(dir: &Path, name: &str) -> Result<Vec<BetaRow>> {
    let (file, text) = read_beta_text(dir, name)?;
    parse_beta_rows(&file, &text)
}

/// Reads `<dir>/<name>.json` as single betas. `name` is a file name without folders.
pub fn load_beta_file(dir: &Path, name: &str) -> Result<Vec<(String, f64)>> {
    let (file, text) = read_beta_text(dir, name)?;
    parse_beta_file(&file, &text)
}

/// A fingerprint of the beta files (names, sizes, modified times), so a tab whose rate-shock step
/// reads one notices an edited file.
pub fn betas_stamp() -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut entries: Vec<(String, u64, Option<std::time::SystemTime>)> = std::fs::read_dir(betas_dir())
        .map(|rd| rd.flatten().filter_map(|e| { let m = e.metadata().ok()?; Some((e.file_name().to_string_lossy().to_string(), m.len(), m.modified().ok())) }).collect())
        .unwrap_or_default();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.hash(&mut h);
    h.finish()
}
