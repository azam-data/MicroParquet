//! User settings that should survive a restart (`portable_data/settings.json`): the width of the
//! right panel (Phase 6 U2), the default zoom (U6) and the chart choices per tab (U5).
//! Every field has a default, and a missing or damaged file falls back to the defaults with a
//! message the app shows in its status line.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The name of the file inside `portable_data`.
pub const SETTINGS_FILE: &str = "settings.json";

/// The lowest and highest default zoom (percent steps of 5).
pub const ZOOM_MIN: f32 = 0.50;
pub const ZOOM_MAX: f32 = 2.00;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Settings {
    /// The width of the right panel in points; none = the panel's minimum width.
    pub right_panel_width: Option<f32>,
    /// The zoom the app starts at (1.0 = 100%); none = 105%.
    pub default_zoom: Option<f32>,
    /// The tabs of the side panel the owner switched off in View ▸ Side panel (their names: Steps, Column, SQL, Chart).
    pub side_panel_hidden: Vec<String>,
    /// The toolbar buttons the owner switched off in View ▸ Toolbar buttons (their names as the bar shows them).
    pub toolbar_hidden: Vec<String>,
    /// The PDF export dialog's last choices (L2): the heading ("" = none), upright pages, Legal paper.
    pub pdf_title: String,
    pub pdf_portrait: bool,
    pub pdf_legal: bool,
    /// "Add the data table on the next page" for chart tabs in the PDF export.
    #[serde(default)]
    pub pdf_chart_tables: bool,
    /// Chart choices by tab key (the file name of the tab's first file, or its title).
    pub chart: HashMap<String, serde_json::Value>,
    /// The chart tabs of the session (Phase 7): title, the key of the tab they chart, and the chart.
    pub chart_tabs: Vec<serde_json::Value>,
    /// View ▸ Calendar… (Phase 8 W0): the fiscal year start and the week start (January and Saturday).
    #[serde(default)]
    pub calendar: crate::dates::CalendarSettings,
    /// File ▸ Open recent project (Phase 8 W4): the last 8 project files, newest first.
    #[serde(default)]
    pub recent_projects: Vec<String>,
    /// The pivot builder's own window (Phase 9 P1), as [x, y, width, height] in points: where it was and how big (none = never popped out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pivot_popout_rect: Option<[f32; 4]>,
    /// The view the window opened in last time: "Report", "Data" or "Model" (UI revamp). None = Data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_view: Option<String>,
}

pub fn settings_path(root: &Path) -> PathBuf {
    root.join(SETTINGS_FILE)
}

impl Settings {
    /// Reads `settings.json` from the data folder. A missing file gives the defaults and no
    /// message; a damaged one gives the defaults and a message (the damaged file is kept next to
    /// it as `settings.json.bad`).
    pub fn load(root: &Path) -> (Settings, Option<String>) {
        let path = settings_path(root);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Settings::default(), None),
            Err(e) => return (Settings::default(), Some(format!("settings.json could not be read ({}). The defaults are used.", e))),
        };
        match serde_json::from_str::<Settings>(&text) {
            Ok(s) => (s, None),
            Err(e) => {
                let kept = std::fs::copy(&path, root.join(format!("{}.bad", SETTINGS_FILE))).is_ok();
                let kept_note = if kept { "the damaged file was kept as settings.json.bad" } else { "the damaged file could not be copied to settings.json.bad" };
                (Settings::default(), Some(format!("settings.json is damaged ({}). The defaults are used; {}.", e, kept_note)))
            }
        }
    }

    /// Writes the file (the folder is created if it is missing).
    pub fn save(&self, root: &Path) -> anyhow::Result<()> {
        use anyhow::Context;
        std::fs::create_dir_all(root).with_context(|| format!("Could not create the folder {}", root.display()))?;
        let text = serde_json::to_string_pretty(self).context("Could not write the settings")?;
        let path = settings_path(root);
        // through a temporary file, so a crash or a full disk never leaves half a settings file
        let tmp = path.with_extension(format!("json.tmp{}", std::process::id()));
        std::fs::write(&tmp, text).with_context(|| format!("Could not save {}", path.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            anyhow::anyhow!("Could not save {}: {}", path.display(), e)
        })
    }

    /// The zoom to start at and a message when the file's value had to be changed: a value
    /// outside 50% to 200% is clamped, one that is not a number is ignored.
    pub fn start_zoom(&self, fallback: f32) -> (f32, Option<String>) {
        match self.default_zoom {
            None => (fallback, None),
            Some(z) if !z.is_finite() => (fallback, Some("The default zoom in settings.json is not a number, so 105% is used.".to_string())),
            Some(z) if z > ZOOM_MAX + 1e-4 || z < ZOOM_MIN - 1e-4 => {
                let c = z.clamp(ZOOM_MIN, ZOOM_MAX);
                (c, Some(format!("The default zoom in settings.json ({:.0}%) is outside 50% to 200%, so {:.0}% is used.", z * 100.0, c * 100.0)))
            }
            Some(z) => (z, None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mp_settings_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("folder");
        d
    }

    fn msg_text_has_bad(m: &Option<String>) -> bool {
        m.as_ref().is_some_and(|t| t.contains("settings.json.bad"))
    }

    #[test]
    fn saving_goes_through_a_temporary_file_and_leaves_none_behind() {
        let d = tmp("atomic");
        let s = Settings { right_panel_width: Some(480.0), ..Settings::default() };
        s.save(&d).expect("saves");
        s.save(&d).expect("saves again over the file");
        assert_eq!(Settings::load(&d).0.right_panel_width, Some(480.0));
        let left: Vec<String> = std::fs::read_dir(&d).expect("dir").flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        assert_eq!(left, vec!["settings.json".to_string()], "no temporary file is left: {left:?}");
    }

    #[test]
    fn a_missing_file_gives_the_defaults_and_a_damaged_one_says_so() {
        let d = tmp("load");
        assert_eq!(Settings::load(&d), (Settings::default(), None));
        std::fs::write(settings_path(&d), "{ not json").expect("write");
        let (s, msg) = Settings::load(&d);
        assert_eq!(s, Settings::default());
        assert!(msg.expect("a message").contains("damaged"));
        assert!(d.join("settings.json.bad").exists());
        assert!(msg_text_has_bad(&Settings::load(&d).1));
        // fields that are missing take their defaults; unknown ones are ignored
        std::fs::write(settings_path(&d), r#"{"right_panel_width": 500.0, "future": 1}"#).expect("write");
        let (s, msg) = Settings::load(&d);
        assert!(msg.is_none());
        assert_eq!((s.right_panel_width, s.default_zoom), (Some(500.0), None));
    }

    #[test]
    fn settings_round_trip_and_the_zoom_is_clamped_with_a_message() {
        let d = tmp("save");
        let s = Settings { right_panel_width: Some(480.0), default_zoom: Some(0.9), chart: HashMap::new(), ..Settings::default() };
        s.save(&d).expect("saved");
        assert_eq!(Settings::load(&d).0, s);
        assert_eq!(s.start_zoom(1.05), (0.9, None));
        let big = Settings { default_zoom: Some(5.0), ..Default::default() };
        let (z, msg) = big.start_zoom(1.05);
        assert_eq!(z, 2.0);
        assert!(msg.expect("a message").contains("500%"));
        assert_eq!(Settings::default().start_zoom(1.05), (1.05, None));
    }
}
