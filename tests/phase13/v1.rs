//! Phase 13 V1: saved views of a dashboard page's filters.

use micro_parquet::dashboard::*;

fn filter(id: &str, label: &str) -> PageFilter {
    PageFilter { id: id.into(), label: label.into(), tab: "t1".into(), field: label.into(), model: None, multi: true }
}

fn page() -> DashPage {
    let mut p = DashPage { id: "p1".into(), title: "Page 1".into(), ..Default::default() };
    p.add_filter(filter("f1", "Wing")).expect("f1");
    p.add_filter(filter("f2", "Quarter")).expect("f2");
    p
}

fn pick(p: &mut DashPage, j: usize, v: &[&str]) {
    p.state.filter_picks[j] = Some(v.iter().map(|s| s.to_string()).collect());
}

#[test]
fn v1_a_a_view_gives_the_filters_back_as_they_were_saved_and_the_rest_go_to_all() {
    let mut p = page();
    pick(&mut p, 0, &["Conventional"]);
    pick(&mut p, 1, &["2026 Q4"]);
    p.save_view("Conventional wing, Q4").expect("save");
    // the reader moves on: another wing, no quarter
    p.state.clear();
    pick(&mut p, 0, &["Islami"]);
    assert!(!p.view_is_on(&p.views[0].clone()));
    let notes = p.apply_view("Conventional wing, Q4").expect("apply");
    assert!(notes.is_empty());
    assert_eq!(p.state.filter_picks[0], Some(vec!["Conventional".to_string()]));
    assert_eq!(p.state.filter_picks[1], Some(vec!["2026 Q4".to_string()]));
    assert!(p.view_is_on(&p.views[0].clone()));
    // a view saved with nothing picked is the "all" view
    p.state.clear();
    p.save_view("Everything").expect("save");
    pick(&mut p, 0, &["OBU"]);
    p.apply_view("Everything").expect("apply");
    assert_eq!(p.state, PageState::default());
}

#[test]
fn v1_b_the_same_name_replaces_the_view_and_names_and_counts_are_checked_in_plain_words() {
    let mut p = page();
    pick(&mut p, 0, &["Conventional"]);
    p.save_view("Wing").expect("save");
    pick(&mut p, 0, &["Islami"]);
    p.save_view("  wing ").expect("replace by the same name, capitals ignored");
    assert_eq!(p.views.len(), 1);
    assert_eq!(p.views[0].picks, vec![("f1".to_string(), vec!["Islami".to_string()])]);
    assert!(p.save_view("   ").expect_err("empty").to_string().contains("Type a name"));
    for i in 0..MAX_VIEWS - 1 {
        p.save_view(&format!("v{i}")).expect("save");
    }
    assert!(p.save_view("one too many").expect_err("full").to_string().contains("keeps 20 views"));
    assert!(p.apply_view("nope").expect_err("missing").to_string().contains("is not on this page"));
    assert!(p.delete_view("wing") && !p.delete_view("wing"));
}

#[test]
fn v1_c_a_filter_that_is_gone_is_said_and_a_reordered_filter_keeps_its_pick() {
    let mut p = page();
    pick(&mut p, 0, &["Conventional"]);
    pick(&mut p, 1, &["2026 Q4"]);
    p.save_view("Both").expect("save");
    // the first filter is removed: the quarter filter moves up to the first place
    p.remove_filter(0);
    let notes = p.apply_view("Both").expect("apply");
    assert_eq!(notes.len(), 1);
    assert!(notes[0].contains("no longer on this page"), "{notes:?}");
    assert_eq!(p.state.filter_picks[0], Some(vec!["2026 Q4".to_string()]), "kept by the filter's id, not its place");
}

#[test]
fn v1_d_an_old_dashboard_file_without_views_loads_and_a_page_without_views_writes_none() {
    let old = r#"{"id":"p1","title":"Page 1"}"#;
    let p: DashPage = serde_json::from_str(old).expect("old page");
    assert!(p.views.is_empty());
    assert!(!serde_json::to_string(&p).expect("json").contains("views"));
    let mut q = page();
    pick(&mut q, 0, &["A"]);
    q.save_view("x").expect("save");
    let back: DashPage = serde_json::from_str(&serde_json::to_string(&q).expect("json")).expect("round trip");
    assert_eq!(back.views, q.views);
}
