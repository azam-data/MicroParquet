//! Every "save as" dialog of the app goes through here (L1): the dialog offers only the file type
//! it writes (a filter), and a path that comes back without that extension gets it before anything
//! is written, so an export is never a file without an extension.

use std::path::{Path, PathBuf};

/// What a save dialog asks for.
#[derive(Clone, Debug)]
pub struct SaveSpec {
    /// The name of the file type in the dialog's type box ("CSV (*.csv)").
    pub filter_name: String,
    /// The extension the file must have, without the dot ("csv").
    pub ext: String,
    /// The suggested file name, with its extension.
    pub default_name: String,
    /// The folder the dialog opens in, when there is a preferred one.
    pub directory: Option<PathBuf>,
}

impl SaveSpec {
    pub fn new(filter_name: &str, ext: &str, default_name: impl Into<String>) -> Self {
        Self { filter_name: filter_name.to_string(), ext: ext.trim_start_matches('.').to_string(), default_name: default_name.into(), directory: None }
    }

    pub fn in_dir(mut self, dir: &Path) -> Self {
        self.directory = Some(dir.to_path_buf());
        self
    }
}

/// `path` with `ext` at the end of its file name unless it already has it (any letter case): "report"
/// becomes "report.csv", "report.txt" becomes "report.txt.csv". A multi-part extension such as
/// "mpq.json" is checked as a whole name ending.
pub fn ensure_extension(path: PathBuf, ext: &str) -> PathBuf {
    let ext = ext.trim_start_matches('.');
    if ext.is_empty() {
        return path;
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    if name.ends_with(&format!(".{}", ext.to_lowercase())) {
        return path;
    }
    let mut s = path.into_os_string();
    s.push(format!(".{}", ext));
    PathBuf::from(s)
}

#[cfg(test)]
type SaveHook = Box<dyn FnMut(&SaveSpec) -> Option<PathBuf>>;

#[cfg(test)]
thread_local! {
    /// Tests answer the dialog: the hook sees what was asked (filter, extension, suggested name).
    static HOOK: std::cell::RefCell<Option<SaveHook>> = std::cell::RefCell::new(None);
    /// The folder a test answers a "choose a folder" dialog with.
    static FOLDER: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub fn set_save_hook(h: Option<SaveHook>) {
    HOOK.with(|c| *c.borrow_mut() = h);
}

#[cfg(test)]
pub fn set_folder_answer(p: Option<PathBuf>) {
    FOLDER.with(|c| *c.borrow_mut() = p);
}

/// Shows the save dialog and returns the chosen path with the extension ensured; `None` when the
/// dialog was cancelled.
pub fn choose_save_path(spec: &SaveSpec) -> Option<PathBuf> {
    #[cfg(test)]
    {
        let answered = HOOK.with(|c| c.borrow_mut().as_mut().map(|h| h(spec)));
        return match answered {
            Some(p) => p.map(|p| ensure_extension(p, &spec.ext)),
            None => None,
        };
    }
    #[cfg(not(test))]
    {
        let mut d = rfd::FileDialog::new().add_filter(spec.filter_name.as_str(), &[spec.ext.rsplit('.').next().unwrap_or(spec.ext.as_str())]).set_file_name(spec.default_name.as_str());
        if let Some(dir) = &spec.directory {
            d = d.set_directory(dir);
        }
        d.save_file().map(|p| ensure_extension(p, &spec.ext))
    }
}

/// Shows the "choose a folder" dialog (the split export).
pub fn choose_folder() -> Option<PathBuf> {
    #[cfg(test)]
    {
        return FOLDER.with(|c| c.borrow().clone());
    }
    #[cfg(not(test))]
    {
        rfd::FileDialog::new().pick_folder()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_without_the_extension_gets_it_and_one_with_it_is_left_alone() {
        assert_eq!(ensure_extension(PathBuf::from("/x/report"), "csv"), PathBuf::from("/x/report.csv"));
        assert_eq!(ensure_extension(PathBuf::from("/x/report.txt"), "csv"), PathBuf::from("/x/report.txt.csv"));
        assert_eq!(ensure_extension(PathBuf::from("/x/report.CSV"), "csv"), PathBuf::from("/x/report.CSV"));
        assert_eq!(ensure_extension(PathBuf::from("/x/my.mpq.json"), "mpq.json"), PathBuf::from("/x/my.mpq.json"));
        assert_eq!(ensure_extension(PathBuf::from("/x/my"), "mpq.json"), PathBuf::from("/x/my.mpq.json"));
        assert_eq!(ensure_extension(PathBuf::from("/x/Pivot_ Term Period"), "xlsx"), PathBuf::from("/x/Pivot_ Term Period.xlsx"));
    }
}
