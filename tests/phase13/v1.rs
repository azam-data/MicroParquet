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
    p.save_view("Conventional wing, Q4", false).expect("save");
    // the reader moves on: another wing, no quarter
    p.state.clear();
    pick(&mut p, 0, &["Islami"]);
    assert!(!p.view_is_on(&p.views[0].clone(), &|_| Vec::new()));
    let notes = p.apply_view("Conventional wing, Q4", &|_| Vec::new()).expect("apply");
    assert!(notes.is_empty());
    assert_eq!(p.state.filter_picks[0], Some(vec!["Conventional".to_string()]));
    assert_eq!(p.state.filter_picks[1], Some(vec!["2026 Q4".to_string()]));
    assert!(p.view_is_on(&p.views[0].clone(), &|_| Vec::new()));
    // a view saved with nothing picked is the "all" view
    p.state.clear();
    p.save_view("Everything", false).expect("save");
    pick(&mut p, 0, &["OBU"]);
    p.apply_view("Everything", &|_| Vec::new()).expect("apply");
    assert_eq!(p.state, PageState::default());
}

#[test]
fn v1_b_the_same_name_replaces_the_view_and_names_and_counts_are_checked_in_plain_words() {
    let mut p = page();
    pick(&mut p, 0, &["Conventional"]);
    p.save_view("Wing", false).expect("save");
    pick(&mut p, 0, &["Islami"]);
    p.save_view("  wing ", false).expect("replace by the same name, capitals ignored");
    assert_eq!(p.views.len(), 1);
    assert_eq!(p.views[0].picks, vec![("f1".to_string(), vec!["Islami".to_string()])]);
    assert!(p.save_view("   ", false).expect_err("empty").to_string().contains("Type a name"));
    for i in 0..MAX_VIEWS - 1 {
        p.save_view(&format!("v{i}"), false).expect("save");
    }
    assert!(p.save_view("one too many", false).expect_err("full").to_string().contains("keeps 20 views"));
    assert!(p.apply_view("nope", &|_| Vec::new()).expect_err("missing").to_string().contains("is not on this page"));
    assert!(p.delete_view("wing") && !p.delete_view("wing"));
}

#[test]
fn v1_c_a_filter_that_is_gone_is_said_and_a_reordered_filter_keeps_its_pick() {
    let mut p = page();
    pick(&mut p, 0, &["Conventional"]);
    pick(&mut p, 1, &["2026 Q4"]);
    p.save_view("Both", false).expect("save");
    // the first filter is removed: the quarter filter moves up to the first place
    p.remove_filter(0);
    let notes = p.apply_view("Both", &|_| Vec::new()).expect("apply");
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
    q.save_view("x", false).expect("save");
    let back: DashPage = serde_json::from_str(&serde_json::to_string(&q).expect("json")).expect("round trip");
    assert_eq!(back.views, q.views);
}

#[test]
fn v1_e_a_view_can_follow_the_latest_period_of_a_period_filter_by_time_not_by_text() {
    let mut p = DashPage { id: "p1".into(), title: "Page 1".into(), ..Default::default() };
    p.add_filter(PageFilter { id: "f1".into(), label: "Month".into(), tab: "t1".into(), field: "Open Date \u{b7} Month-Year".into(), model: None, multi: false }).expect("f1");
    p.add_filter(filter("f2", "Wing")).expect("f2");
    pick(&mut p, 0, &["Dec 2025"]);
    pick(&mut p, 1, &["Islami"]);
    p.save_view("Latest month, Islami", true).expect("save");
    assert_eq!(p.views[0].latest, ["f1"], "only the period filter follows the latest; the wing stays fixed");
    // new data came in: Jan 2026 is listed. As text "Jan 2026" sorts before "Dec 2025", so only the time order finds it.
    let values = |id: &str| -> Vec<String> { if id == "f1" { vec!["Dec 2025".into(), "Feb 2026".into(), "Jan 2026".into()] } else { Vec::new() } };
    p.state.clear();
    assert!(p.apply_view("Latest month, Islami", &values).expect("apply").is_empty());
    assert_eq!(p.state.filter_picks[0], Some(vec!["Feb 2026".to_string()]));
    assert_eq!(p.state.filter_picks[1], Some(vec!["Islami".to_string()]));
    assert!(p.view_is_on(&p.views[0].clone(), &values));
    // values not loaded yet, and a filter that is not a period: refused in words, never guessed
    let err = p.apply_view("Latest month, Islami", &|_| Vec::new()).expect_err("not loaded").to_string();
    assert!(err.contains("not loaded yet"), "{err}");
    let mut q = page();
    pick(&mut q, 0, &["Islami"]);
    q.save_view("Wing", true).expect("save");
    assert!(q.views[0].latest.is_empty(), "a Wing filter has no latest period");
}
