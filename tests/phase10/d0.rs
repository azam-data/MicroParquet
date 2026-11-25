//! D0: the Phase 9 baseline replay and the dashboard model.

use crate::common::fixtures_dir;
use crate::p10_p9_baseline as base;
use std::collections::BTreeMap;

/// Writes the fixture. Run on the Phase 9 code only: `cargo test --test phase3 d0_write_the_phase9 -- --ignored`.
#[test]
#[ignore]
fn d0_write_the_phase9_replay_fixture() {
    base::generate(&fixtures_dir().join("phase9_replay"), &std::env::temp_dir().join(format!("mp_d0_gen_{}", std::process::id())));
}

#[test]
fn d0_the_phase9_samples_replay_to_the_hashes_the_phase9_code_gave() {
    let fx = fixtures_dir().join("phase9_replay");
    let want: BTreeMap<String, String> = serde_json::from_str(&std::fs::read_to_string(fx.join("phase9_replay.hashes.json")).expect("hashes")).expect("json");
    // 9 tabs, the Nested view, the chart tab's chart, the saved chart, 2 pack sheets and their chart, and the project's bytes
    for k in ["project:t7", "project:t8", "project:t9", "nested:view", "chart:Balance by Wing", "saved-chart:Balance by wing", "pack:Wings", "pack-chart:Wings:0", "project:bytes after save-open-save"] {
        assert!(want.contains_key(k), "{k} is in the hashes: {:?}", want.keys().collect::<Vec<_>>());
    }
    let project = std::fs::read_to_string(fx.join("proj/phase9_replay.mpproj")).expect("the project");
    for line in project.lines().filter(|l| l.contains("\"path\":")) {
        let v = line.split('"').nth(3).unwrap_or("");
        assert!(!v.starts_with('/') && !v.contains(":\\") || v.contains("tests/fixtures/phase9_replay"), "a path outside the fixtures: {v}");
    }
    let got = base::replay(&fx, &std::env::temp_dir().join(format!("mp_d0_replay_{}", std::process::id())));
    let differ: Vec<&String> = want.keys().chain(got.keys()).filter(|k| want.get(*k) != got.get(*k)).collect();
    assert!(differ.is_empty(), "these differ from what the Phase 9 code gave: {differ:?}");
    assert_eq!(got, want);
}

// ------------------------------------------------------------------------------------------------ the dashboard model

use micro_parquet::dashboard::*;
use micro_parquet::kpi::KpiSpec;
use micro_parquet::project::*;

fn item(id: &str, kind: DashItemKind) -> DashItem {
    DashItem { id: id.into(), kind, ..Default::default() }
}

/// 2 pages, 5 items: a chart, a pivot and a KPI card on a grid page with a filter and a state; a free page with two items.
fn dashboard() -> Dashboard {
    let mut state = PageState::default();
    state.filter_picks[0] = Some(vec!["Dhaka".into(), "Ctg".into()]);
    state.cross = Some(CrossPick { dimension: "Wing".into(), value: "Islami".into() });
    state.drill.insert("i1".into(), vec!["Conventional".into()]);
    Dashboard {
        id: "d1".into(),
        title: "ALCO".into(),
        pages: vec![
            DashPage {
                id: "p1".into(),
                title: "Overview".into(),
                layout: PageLayout::Grid { per_row: 3 },
                items: vec![item("i1", DashItemKind::Chart("t9".into())), item("i2", DashItemKind::Pivot("t8".into())), item("i3", DashItemKind::Kpi(KpiSpec { title: "CASA ratio".into(), ..Default::default() }))],
                filters: vec![PageFilter { id: "f1".into(), label: "Region".into(), tab: "t1".into(), field: "Branch master \u{b7} Region".into(), ..Default::default() }],
                state,
                ..Default::default()
            },
            DashPage {
                id: "p2".into(),
                title: "Detail".into(),
                layout: PageLayout::Free,
                items: vec![
                    DashItem { rect: Some(DashRect { x: 0.0, y: 0.0, w: 500.0, h: 300.0 }), title_override: Some("Wings".into()), ..item("i4", DashItemKind::Chart("t9".into())) },
                    DashItem { rect: Some(DashRect { x: 510.0, y: 0.0, w: 490.0, h: 300.0 }), ..item("i5", DashItemKind::Pivot("t8".into())) },
                ],
                background: Some(TemplateRef { name: "Bank".into(), blake3: "ab".into() }),
                ..Default::default()
            },
        ],
        template_library: vec![TemplateRef { name: "Bank".into(), blake3: "ab".into() }],
        default_kpi_unit: Default::default(),
    }
}

fn replay_project() -> Project {
    load_project(&fixtures_dir().join("phase9_replay/proj/phase9_replay.mpproj")).expect("the project")
}

#[test]
fn d0_a_project_without_a_dashboard_saves_byte_for_byte_as_phase9_saves_it() {
    for rel in ["phase9_replay/proj/phase9_replay.mpproj", "phase9_model/proj/monthly.mpproj"] {
        let path = fixtures_dir().join(rel);
        let text = std::fs::read_to_string(&path).expect("the file");
        let p = load_project(&path).expect("loads");
        assert!(p.dashboard.is_none());
        assert_eq!(p.to_json().expect("json"), text, "{rel}: the same bytes");
        let v: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(v["project_version"], 2, "{rel}: a model: version 2");
        assert!(v.get("dashboard").is_none());
    }
    // no model, no dashboard: version 1
    let v: serde_json::Value = serde_json::from_str(&Project::default().to_json().expect("json")).expect("json");
    assert_eq!(v["project_version"], 1);
}

#[test]
fn d0_a_project_with_a_dashboard_saves_as_version_3_and_the_v190_app_refuses_it() {
    let mut p = replay_project();
    p.dashboard = Some(dashboard());
    let text = p.to_json().expect("json");
    let v: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(v["project_version"], 3);
    // the v1.9.0 app reads up to version 2: its check refuses this file
    let e = check_version(&v, PROJECT_VERSION_MODEL).expect_err("refused").to_string();
    assert!(e.starts_with("This project was saved by a newer MicroParquet v") && e.ends_with("Update the app to open it."), "{e}");
    // this build reads it
    assert!(Project::from_json(&text, "x").is_ok());
    // even a dashboard on a project without a model is version 3
    let mut bare = Project::default();
    bare.dashboard = Some(Dashboard::default());
    assert!(bare.to_json().expect("json").contains("\"project_version\": 3"));
}

#[test]
fn d0_a_dashboard_of_2_pages_and_5_items_is_stable_over_save_open_save() {
    let mut p = replay_project();
    let d = dashboard();
    assert_eq!(d.pages.iter().map(|p| p.items.len()).sum::<usize>(), 5);
    p.dashboard = Some(d.clone());
    let first = p.to_json().expect("json");
    let again = Project::from_json(&first, "x").expect("opens");
    assert_eq!(again.dashboard, Some(d));
    assert_eq!(again.to_json().expect("json"), first);
    // through a file as well
    let f = std::env::temp_dir().join(format!("mp_d0_dash_{}.mpproj", std::process::id()));
    write_project(&f, &p).expect("writes");
    assert_eq!(std::fs::read_to_string(&f).expect("read"), first);
    let _ = std::fs::remove_file(&f);
}

#[test]
fn d0_an_old_dashboard_without_new_fields_still_loads() {
    let text = r#"{"project_version": 3, "tabs": [], "dashboard": {"id": "d", "pages": [{"id": "p", "items": [{"id": "i"}]}]}}"#;
    let p = Project::from_json(text, "x").expect("loads");
    let page = &p.dashboard.expect("a dashboard").pages[0];
    assert_eq!(page.layout, PageLayout::Grid { per_row: 2 });
    assert!(page.filters.is_empty() && page.state == PageState::default());
}

#[test]
fn d0_a_grid_of_3_per_row_on_a_1000_unit_page_gives_widths_of_326_67() {
    // hand-worked: (1000 - 2 x 10 gap) / 3 = 326.67
    let r = grid_rects(3, 5, 1000.0, 625.0).expect("rects");
    assert_eq!(r.len(), 5);
    for c in &r {
        assert!((c.w - 326.67).abs() < 0.01, "{}", c.w);
    }
    // two rows: (625 - 10) / 2 = 307.5 high; the second row starts at 307.5 + 10 and the third column at 2 x (326.67 + 10)
    assert!((r[0].h - 307.5).abs() < 1e-9 && (r[3].y - 317.5).abs() < 1e-9 && (r[2].x - 673.33).abs() < 0.01);
    // the last cell ends at the page's edge
    assert!((r[2].x + r[2].w - 1000.0).abs() < 1e-9);
    // 1 per row: the full width; 4 per row: (1000 - 30) / 4 = 242.5
    assert!((grid_rects(1, 2, 1000.0, 625.0).expect("rects")[0].w - 1000.0).abs() < 1e-9);
    assert!((grid_rects(4, 4, 1000.0, 625.0).expect("rects")[0].w - 242.5).abs() < 1e-9);
    // 0 or 5 per row is refused, not clamped
    assert!(grid_rects(0, 1, 1000.0, 625.0).is_err() && grid_rects(5, 1, 1000.0, 625.0).is_err());
}

#[test]
fn d0_an_item_pointing_at_a_deleted_tab_round_trips_and_reports_source_tab_removed() {
    let mut p = replay_project();
    let mut d = dashboard();
    d.pages[0].items[0].kind = DashItemKind::Chart("t99".into());
    p.dashboard = Some(d);
    let again = Project::from_json(&p.to_json().expect("json"), "x").expect("opens");
    let d = again.dashboard.as_ref().expect("a dashboard");
    assert_eq!(d.pages[0].items[0].kind, DashItemKind::Chart("t99".into()), "the key is kept, not dropped");
    assert_eq!(d.pages[0].items[0].source_problem(&again), Some("Source tab removed"));
    assert_eq!(d.pages[0].items[1].source_problem(&again), None, "t8 is there");
    assert_eq!(d.pages[0].items[2].source_problem(&again), None, "a KPI card has no source tab");
}

#[test]
fn d0_too_many_filters_and_duplicate_ids_are_refused_with_a_plain_message() {
    let mut d = dashboard();
    d.pages[0].filters = (0..5).map(|i| PageFilter { id: format!("f{i}"), ..Default::default() }).collect();
    let e = d.validate().expect_err("5 filters").to_string();
    assert!(e.contains("at most 4") && e.contains("Overview"), "{e}");
    let mut d = dashboard();
    d.pages[0].items[1].id = "i1".into();
    assert!(d.validate().expect_err("same id").to_string().contains("two items"));
    let mut p = replay_project();
    p.dashboard = Some(d);
    assert!(p.to_json().is_err(), "an invalid dashboard is never written");
    // a Free page whose item has no place says so
    let mut d = dashboard();
    d.pages[1].items[0].rect = None;
    assert!(item_rects(&d.pages[1]).expect_err("no place").to_string().contains("no place"));
}
