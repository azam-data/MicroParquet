//! Saved schema templates (`*.schema.json` in `portable_data/schemas/`) and how a file that is
//! opened is matched against them (Phase 6 U1).

use crate::models::{ColumnSchemaConfig, IngestionConfig};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A template: its name (the file name without ".schema.json"), its file and its settings.
#[derive(Clone, Debug)]
pub struct SchemaTemplate {
    pub name: String,
    pub path: PathBuf,
    pub config: IngestionConfig,
}

/// The name of a template file: "monthly_loans.schema.json" is "monthly_loans".
pub fn template_name(path: &Path) -> String {
    let file = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let lower = file.to_lowercase();
    for ext in [".schema.json", ".json"] {
        if lower.ends_with(ext) {
            return file[..file.len() - ext.len()].to_string();
        }
    }
    file
}

/// Every template of the folder that has columns, by name. Files that are not schema files are
/// skipped (the Schema dialog's Load button reports those).
pub fn load_templates(dir: &Path) -> Vec<SchemaTemplate> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<SchemaTemplate> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("json")))
        .filter_map(|p| {
            let text = std::fs::read_to_string(&p).ok()?;
            let config: IngestionConfig = serde_json::from_str(&text).ok()?;
            (!config.columns.is_empty()).then(|| SchemaTemplate { name: template_name(&p), path: p, config })
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.name.cmp(&b.name)));
    out
}

/// A header name as it is compared: trimmed, case-insensitive.
fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// True when the file's header names and the template's original column names are the same set
/// (trimmed, case-insensitive, any order). A file with one more or one fewer column does not match.
pub fn header_matches(headers: &[String], template: &IngestionConfig) -> bool {
    let file: BTreeSet<String> = headers.iter().map(|h| key(h)).collect();
    let saved: BTreeSet<String> = template.columns.iter().map(|c| key(&c.name)).collect();
    !file.is_empty() && file == saved
}

/// The templates that match a file's header names.
pub fn matching_templates<'a>(templates: &'a [SchemaTemplate], headers: &[String]) -> Vec<&'a SchemaTemplate> {
    templates.iter().filter(|t| header_matches(headers, &t.config)).collect()
}

/// The file's detected schema with the template's choices: for each column of the file, the
/// template's type, mask, Dr/Cr sign and "Rename to"; the file's own name, samples and reading
/// options (separator, title lines, encoding) are kept.
pub fn apply_template(detected: &IngestionConfig, template: &IngestionConfig) -> IngestionConfig {
    let columns: Vec<ColumnSchemaConfig> = detected
        .columns
        .iter()
        .map(|d| match template.columns.iter().find(|t| key(&t.name) == key(&d.name)) {
            Some(t) => ColumnSchemaConfig {
                name: d.name.clone(),
                dtype: t.dtype,
                pii_mask: t.pii_mask,
                sample_values: d.sample_values.clone(),
                sign_convention: t.sign_convention,
                has_dr_cr: d.has_dr_cr,
                source_dtype: d.source_dtype.clone(),
                rename_to: t.rename_to.clone(),
            },
            None => d.clone(),
        })
        .collect();
    IngestionConfig {
        columns,
        sanitize_linebreaks: template.sanitize_linebreaks,
        trim_before_mask: template.trim_before_mask,
        parse_accounting_negatives: template.parse_accounting_negatives,
        heal_broken_csv_lines: template.heal_broken_csv_lines,
        pii_salt: template.pii_salt.clone(),
        separator: detected.separator,
        skip_rows: detected.skip_rows,
        encoding: detected.encoding,
        auto_apply: template.auto_apply,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, rename: Option<&str>) -> ColumnSchemaConfig {
        ColumnSchemaConfig { name: name.into(), rename_to: rename.map(str::to_string), ..Default::default() }
    }

    #[test]
    fn headers_match_as_a_trimmed_case_insensitive_set() {
        let t = IngestionConfig { columns: vec![col("ACNO", Some("Account_No")), col("CRIRTE", None), col("BAL", None)], ..Default::default() };
        let h = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(header_matches(&h(&["BAL", "ACNO", "CRIRTE"]), &t));
        assert!(header_matches(&h(&[" bal ", "Acno", "crirte"]), &t));
        assert!(!header_matches(&h(&["BAL", "ACNO", "CRIRTE", "EXTRA"]), &t));
        assert!(!header_matches(&h(&["BAL", "ACNO"]), &t));
        assert!(!header_matches(&[], &t));
    }

    #[test]
    fn template_names_drop_the_extension() {
        assert_eq!(template_name(Path::new("a/monthly.schema.json")), "monthly");
        assert_eq!(template_name(Path::new("a/Loans.JSON")), "Loans");
        assert_eq!(template_name(Path::new("a/x.csv")), "x.csv");
    }
}
