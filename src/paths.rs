//! File names and data-folder location (Phase 3 L5).

use std::path::{Path, PathBuf};

/// The longest default export file name (with its extension).
pub const MAX_FILE_NAME_CHARS: usize = 120;

/// Characters Windows does not allow in a file name.
const FORBIDDEN: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// A file-name stem that is safe on Windows: `<>:"/\|?*` and control characters become `_`,
/// trailing dots and spaces go (Windows drops them), and an empty result is "export".
pub fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if FORBIDDEN.contains(&c) || c.is_control() { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        "export".to_string()
    } else {
        trimmed.to_string()
    }
}

/// `<sanitised stem>.<ext>`, at most 120 characters in total; the extension is always kept.
pub fn default_export_file_name(stem: &str, ext: &str) -> String {
    let ext = ext.trim_start_matches('.');
    let room = MAX_FILE_NAME_CHARS.saturating_sub(ext.chars().count() + 1).max(1);
    let mut s: String = sanitize_file_name(stem).chars().take(room).collect();
    // cutting may leave a trailing dot or space
    while s.ends_with(['.', ' ']) {
        s.pop();
    }
    if s.is_empty() {
        s = "export".to_string();
    }
    if ext.is_empty() {
        s
    } else {
        format!("{}.{}", s, ext)
    }
}

/// True when files can be created in `dir` (it is created if missing, and a probe file is written
/// and removed).
pub fn dir_is_writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".mp_write_test_{}", std::process::id()));
    match std::fs::write(&probe, b"x") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Where `portable_data` lives.
/// * Windows and Linux: next to the executable.
/// * macOS, inside `Name.app`: next to the `.app` bundle (never inside it, where updates and code
///   signing would wipe it); when that folder is not writable,
///   `~/Library/Application Support/MicroParquet`.
/// * macOS, a bare binary: next to the executable.
pub fn choose_portable_root(
    exe: &Path,
    macos: bool,
    home: Option<&Path>,
    writable: &dyn Fn(&Path) -> bool,
) -> PathBuf {
    let exe_dir = exe.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    if !macos {
        return exe_dir.join("portable_data");
    }
    let bundle = exe.ancestors().find(|a| {
        a.extension().is_some_and(|e| e.eq_ignore_ascii_case("app"))
    });
    let Some(bundle) = bundle else {
        return exe_dir.join("portable_data");
    };
    let beside = bundle.parent().unwrap_or(Path::new(".")).join("portable_data");
    if writable(&beside) {
        return beside;
    }
    match home {
        Some(h) => h.join("Library").join("Application Support").join("MicroParquet"),
        None => beside,
    }
}
