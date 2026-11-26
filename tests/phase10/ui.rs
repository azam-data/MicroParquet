//! UI revamp: the project file keeps its bytes when nothing was sent to the report, a list of sent tabs is saved and read back, and
//! an old project (no list) opens with none.

use micro_parquet::project::Project;

#[test]
fn ui_a_project_without_sent_tabs_has_no_report_tabs_in_its_file() {
    let p = Project::default();
    let text = p.to_json().expect("json");
    assert!(!text.contains("report_tabs"), "nothing sent: nothing written, so the bytes are what Phase 10 wrote\n{text}");
    assert!(text.contains("\"project_version\": 1"), "{text}");
}

#[test]
fn ui_sent_tabs_are_written_and_read_back_in_the_same_order() {
    let p = Project { report_tabs: vec!["t2".into(), "t5".into()], ..Project::default() };
    let text = p.to_json().expect("json");
    assert!(text.contains("report_tabs"), "{text}");
    let back: Project = serde_json::from_str(&text).expect("read back");
    assert_eq!(back.report_tabs, vec!["t2".to_string(), "t5".to_string()]);
    // saving what was read gives the same bytes
    assert_eq!(back.to_json().expect("again"), text);
}

#[test]
fn ui_an_older_project_file_opens_with_no_sent_tabs() {
    let old = r#"{"project_version":1,"app_version":"1.8.0","saved_at":"2026-11-01T10:00:00+06:00","active_tab":"","tabs":[]}"#;
    let p: Project = serde_json::from_str(old).expect("an old project opens");
    assert!(p.report_tabs.is_empty());
    assert!(p.dashboard.is_none());
}

#[test]
fn ui_the_settings_file_keeps_working_without_a_last_view() {
    let s: micro_parquet::settings::Settings = serde_json::from_str("{\"right_panel_width\": 400.0}").expect("settings");
    assert_eq!(s.last_view, None);
    let with = micro_parquet::settings::Settings { last_view: Some("Report".into()), ..s.clone() };
    let text = serde_json::to_string(&with).expect("json");
    let back: micro_parquet::settings::Settings = serde_json::from_str(&text).expect("back");
    assert_eq!(back.last_view.as_deref().and_then(micro_parquet::workspace::View::from_name), Some(micro_parquet::workspace::View::Report));
    assert!(!serde_json::to_string(&s).expect("json").contains("last_view"), "an unset view is not written");
}
