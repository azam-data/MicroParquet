//! Deposit and loan betas (Phase 4 V5): `portable_data/betas/<name>.json` maps product codes to
//! the share, in percent, of a market-rate change that the product's own rate follows.
//!
//! ```json
//! { "source": "ALCO paper 3/2026", "betas": [ { "product": "SAVINGS", "beta_pct": 40 }, { "product": "FDR", "beta_pct": 100 } ] }
//! ```
//!
//! Codes are matched trimmed and upper-case with spaces collapsed, as the CL codes are. A product
//! that is not in the file passes through 100%; the status bar counts those rows.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BetaEntry {
    pub product: String,
    pub beta_pct: f64,
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

/// Checks the text of a beta file and returns its entries as (normalised product, beta in 0..=1 of a change).
pub fn parse_beta_file(name: &str, text: &str) -> Result<Vec<(String, f64)>> {
    let f: BetaFile = serde_json::from_str(text).map_err(|e| anyhow!("The beta file {} is not valid: {}. Expected {{\"betas\": [{{\"product\": \"SAVINGS\", \"beta_pct\": 40}}]}}.", name, e))?;
    if f.betas.is_empty() {
        bail!("The beta file {} lists no products. Add at least one {{\"product\": \"...\", \"beta_pct\": 0-100}}.", name);
    }
    let mut out: Vec<(String, f64)> = Vec::new();
    for e in &f.betas {
        let key = normalise_product(&e.product);
        if key.is_empty() {
            bail!("The beta file {} has an entry with no product code.", name);
        }
        if !e.beta_pct.is_finite() || e.beta_pct < 0.0 || e.beta_pct > 100.0 {
            bail!("The beta file {}: the beta of '{}' is {}, not a percentage from 0 to 100.", name, e.product, e.beta_pct);
        }
        if out.iter().any(|(k, _)| *k == key) {
            bail!("The beta file {} lists '{}' twice (codes are matched ignoring case and extra spaces). Keep one.", name, e.product);
        }
        out.push((key, e.beta_pct / 100.0));
    }
    Ok(out)
}

/// Reads `<dir>/<name>.json`. `name` is a file name without folders.
pub fn load_beta_file(dir: &Path, name: &str) -> Result<Vec<(String, f64)>> {
    let n = name.trim();
    if n.is_empty() || n.contains(['/', '\\']) || n.contains("..") {
        bail!("'{}' is not a beta file name. Use the name of a file in portable_data/betas, without the folder or .json.", name);
    }
    let path = dir.join(format!("{}.json", n));
    if !path.exists() {
        bail!("The beta file '{}' was not found in {}. Add {}.json there, or choose another file.", n, dir.display(), n);
    }
    let text = std::fs::read_to_string(&path).with_context(|| format!("Could not read the beta file {}", path.display()))?;
    parse_beta_file(&format!("{}.json", n), &text)
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
