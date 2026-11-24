//! M0: the model core. A link joins a data tab to a lookup tab on 1 to 3 key columns; the key check lists duplicate, blank and
//! unmatched keys; a project with a model is written as version 2 and one without as version 1; a lookup file that changes
//! makes its link Broken in the open line.

use crate::common::{dir, parquet};
use micro_parquet::model::*;
use micro_parquet::models::*;
use micro_parquet::project::*;
use polars::prelude::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn strs(name: &str, v: &[&str]) -> Series {
    Series::new(name.into(), v.to_vec())
}
fn nums(name: &str, v: &[f64]) -> Series {
    Series::new(name.into(), v.to_vec())
}
fn opt_strs(name: &str, v: &[Option<&str>]) -> Series {
    Series::new(name.into(), v.to_vec())
}

fn tab(key: &str, title: &str, source: TabSource) -> ModelTab {
    ModelTab { key: key.into(), title: title.into(), source, steps: vec![] }
}

fn info(key: &str, title: &str, kind: TabKind, cols: &[(&str, DataType)]) -> TabInfo {
    TabInfo { key: key.into(), title: title.into(), kind, columns: cols.iter().map(|(n, d)| (n.to_string(), d.clone())).collect() }
}

fn pair(d: &str, l: &str) -> KeyPair {
    KeyPair { data: d.into(), lookup: l.into() }
}

fn run(link: &Link, data: &ModelTab, lookup: &ModelTab, d: &Path) -> KeyCheck {
    check_link(link, data, lookup, d, &|| Ok(())).expect("the key check")
}

/// Accounts (Wing, Product code, Balance): SB01 is in both wings.
fn accounts() -> DataFrame {
    DataFrame::new(vec![
        strs("Wing", &["Conventional", "Conventional", "Islami", "Islami", "Conventional"]),
        strs("Product code", &["SB01", "FD01", "SB01", "FD01", "XX99"]),
        nums("Balance", &[100.0, 200.0, 300.0, 400.0, 50.0]),
    ])
    .expect("accounts")
}

fn products(extra: bool) -> DataFrame {
    let mut wing = vec!["Conventional", "Conventional", "Islami", "Islami"];
    let mut code = vec!["SB01", "FD01", "SB01", "FD01"];
    let mut group = vec!["Savings", "Term deposit", "Mudaraba savings", "Mudaraba term"];
    if extra {
        wing.push("Conventional");
        code.push("SB01");
        group.push("Savings (old)");
    }
    DataFrame::new(vec![strs("Wing", &wing), strs("Code", &code), strs("Group", &group)]).expect("products")
}

fn composite() -> Link {
    Link { data_tab: "t1".into(), lookup_tab: "t2".into(), keys: vec![pair("Wing", "Wing"), pair("Product code", "Code")], unmatched_value: Some("Balance".into()) }
}

#[test]
fn m0_a_product_master_keyed_by_wing_and_product_code_links_and_a_second_row_makes_it_broken_and_lists_both_rows() {
    let d = dir("p9m0_composite");
    let data = tab("t1", "Accounts", parquet(&d, "accounts", &accounts()));
    // SB01 is in both wings and each has its own row: the link is fine
    let ok = tab("t2", "Product master", parquet(&d, "products", &products(false)));
    let c = run(&composite(), &data, &ok, &d);
    assert_eq!(c.status(), LinkStatus::OkUnmatched(1), "XX99 has no product row, nothing is duplicated");
    assert_eq!((c.duplicate_keys, c.lookup_blank_keys), (0, 0));
    assert_eq!(c.matched_rows, 4, "SB01 and FD01 of both wings match");
    // a second (Conventional, SB01) row
    let bad = tab("t2", "Product master", parquet(&d, "products2", &products(true)));
    let c = run(&composite(), &data, &bad, &d);
    assert_eq!(c.status(), LinkStatus::Broken(vec!["1 duplicate key".to_string()]));
    assert_eq!(c.status().text(), "Broken: 1 duplicate key");
    assert_eq!(c.duplicate_keys, 1);
    assert_eq!(c.duplicate_rows, 2);
    assert_eq!(c.duplicate_examples, vec![(vec!["Conventional".to_string(), "SB01".to_string()], 2)], "the key check lists that key");
    // Show duplicates: those 2 rows, sorted by key, with Times = 2
    let df = duplicate_rows(&composite(), &bad, &d).expect("the list");
    assert_eq!(df.get_column_names().iter().map(|n| n.to_string()).collect::<Vec<_>>(), ["Wing", "Code", "Group", "Times"]);
    assert_eq!(df.height(), 2);
    let groups: Vec<String> = (0..2).map(|i| df.column("Group").expect("g").get(i).expect("v").get_str().expect("s").to_string()).collect();
    assert_eq!(groups, ["Savings", "Savings (old)"], "both rows, in the file's order");
    let times: Vec<i64> = (0..2).map(|i| df.column("Times").expect("t").get(i).expect("v").extract::<i64>().expect("n")).collect();
    assert_eq!(times, [2, 2]);
    // what a lookup field would say
    assert_eq!(broken_message("Product master", &c.status()), "'Product master' has 1 duplicate key. Open Model \u{25b8} Key check to see it.");
    assert_eq!(
        broken_message("Branch master", &LinkStatus::Broken(vec!["3 duplicate keys".to_string()])),
        "'Branch master' has 3 duplicate keys. Open Model \u{25b8} Key check to see them."
    );
}

#[test]
fn m0_duplicates_are_sorted_by_key_and_a_column_called_times_is_not_overwritten() {
    let d = dir("p9m0_sorted");
    let lk = DataFrame::new(vec![strs("Code", &["B", "A", "B", "A", "C"]), strs("Times", &["x", "y", "z", "w", "v"])]).expect("lookup");
    let data = tab("t1", "D", parquet(&d, "d", &DataFrame::new(vec![strs("Code", &["A"])]).expect("d")));
    let look = tab("t2", "L", parquet(&d, "l", &lk));
    let link = Link { data_tab: "t1".into(), lookup_tab: "t2".into(), keys: vec![pair("Code", "Code")], unmatched_value: None };
    let df = duplicate_rows(&link, &look, &d).expect("the list");
    let codes: Vec<String> = (0..df.height()).map(|i| df.column("Code").expect("c").get(i).expect("v").get_str().expect("s").to_string()).collect();
    assert_eq!(codes, ["A", "A", "B", "B"], "sorted by key; C is not repeated so it is not listed");
    assert_eq!(df.get_column_names().last().map(|n| n.to_string()), Some("Times_".to_string()));
    assert_eq!(df.column("Times").expect("the lookup's own column").get(0).expect("v").get_str(), Some("y"));
    let _ = data;
}

#[test]
fn m0_a_blank_key_in_the_lookup_makes_it_broken_and_a_blank_key_in_the_data_is_counted_apart() {
    let d = dir("p9m0_blank");
    let lookup = DataFrame::new(vec![opt_strs("Code", &[Some("A"), Some("B"), None, Some("   ")]), strs("Name", &["a", "b", "c", "d"])]).expect("lookup");
    let data = DataFrame::new(vec![opt_strs("Code", &[Some("A"), None, Some("Z"), Some("A")]), nums("Balance", &[1.0, 2.0, 4.0, 8.0])]).expect("data");
    let link = Link { data_tab: "t1".into(), lookup_tab: "t2".into(), keys: vec![pair("Code", "Code")], unmatched_value: Some("Balance".into()) };
    let c = run(&link, &tab("t1", "Accounts", parquet(&d, "d", &data)), &tab("t2", "Branch master", parquet(&d, "l", &lookup)), &d);
    assert_eq!(c.lookup_blank_keys, 2, "a missing key and one of only spaces");
    assert_eq!(c.status(), LinkStatus::Broken(vec!["2 blank lookup keys".to_string()]));
    assert_eq!(c.data_blank_keys, 1, "the blank data key is counted on its own");
    assert_eq!(c.unmatched.len(), 1, "and is not an unmatched key");
    assert_eq!(c.unmatched[0].key, vec!["Z".to_string()]);
    assert_eq!(c.unmatched_rows + c.matched_rows + c.data_blank_keys, c.data_rows, "every data row is matched, unmatched or blank");
}

#[test]
fn m0_unmatched_keys_are_listed_with_their_rows_and_sums_and_show_unmatched_rows_gives_exactly_those_rows() {
    let d = dir("p9m0_unmatched");
    let data = DataFrame::new(vec![
        strs("Branch", &["B1", "B2", "B9", "B7", "B9", "B7", "B9", "B1"]),
        nums("Balance", &[10.0, 20.0, 1000.0, 5.0, 2000.0, 7.0, 3000.0, 40.0]),
        strs("Ref", &["r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8"]),
    ])
    .expect("data");
    let lookup = DataFrame::new(vec![strs("Code", &["B1", "B2", "B3"]), strs("Region", &["Dhaka", "Dhaka", "Sylhet"])]).expect("lookup");
    let (dt, lt) = (tab("t1", "Accounts", parquet(&d, "d", &data)), tab("t2", "Branch master", parquet(&d, "l", &lookup)));
    let link = Link { data_tab: "t1".into(), lookup_tab: "t2".into(), keys: vec![pair("Branch", "Code")], unmatched_value: Some("Balance".into()) };
    let c = run(&link, &dt, &lt, &d);
    assert_eq!(c.status(), LinkStatus::OkUnmatched(2));
    assert_eq!(c.status().text(), "OK \u{b7} 2 unmatched keys");
    assert_eq!(
        c.unmatched,
        vec![
            UnmatchedKey { key: vec!["B7".into()], rows: 2, sum: Some(12.0) },
            UnmatchedKey { key: vec!["B9".into()], rows: 3, sum: Some(6000.0) },
        ]
    );
    assert_eq!((c.unmatched_rows, c.matched_rows, c.data_rows), (5, 3, 8));
    assert_eq!(c.unused_lookup_rows, 1, "B3 is used by no data row");
    assert_eq!(c.value_column.as_deref(), Some("Balance"));
    // Copy list
    assert_eq!(unmatched_list_text(&link, &c), "Branch\tRows\tSum of Balance\nB7\t2\t12\nB9\t3\t6000\n");
    // Show unmatched rows: exactly the five rows, in the data's order, with the data's columns
    let rows = unmatched_rows(&link, &dt, &lt, &d).expect("the rows");
    assert_eq!(rows.get_column_names().iter().map(|n| n.to_string()).collect::<Vec<_>>(), ["Branch", "Balance", "Ref"]);
    let refs: Vec<String> = (0..rows.height()).map(|i| rows.column("Ref").expect("r").get(i).expect("v").get_str().expect("s").to_string()).collect();
    assert_eq!(refs, ["r3", "r4", "r5", "r6", "r7"]);
    let sum: f64 = rows.column("Balance").expect("b").f64().expect("f").into_iter().flatten().sum();
    assert_eq!(sum, 6012.0, "the rows add up to the sums the list shows");
    // no value column chosen: no sums
    let none = Link { unmatched_value: None, ..link.clone() };
    let c2 = run(&none, &dt, &lt, &d);
    assert!(c2.unmatched.iter().all(|u| u.sum.is_none()) && c2.value_column.is_none());
    assert_eq!(unmatched_list_text(&none, &c2), "Branch\tRows\nB7\t2\nB9\t3\n");
    // a value column that is text is refused in words and no sum is shown
    let text = Link { unmatched_value: Some("Ref".into()), ..link.clone() };
    let c3 = run(&text, &dt, &lt, &d);
    assert!(c3.value_note.as_deref().is_some_and(|n| n.contains("'Ref' is text in 'Accounts'")), "{:?}", c3.value_note);
    assert!(c3.unmatched.iter().all(|u| u.sum.is_none()));
}

#[test]
fn m0_the_role_type_second_link_and_chart_tab_refusals_give_their_messages() {
    let text = DataType::String;
    let num = DataType::Int64;
    let accounts = info("t1", "Accounts", TabKind::Table, &[("Branch Code", text.clone()), ("Balance", DataType::Float64)]);
    let gl = info("t3", "GL", TabKind::Table, &[("Branch Code", text.clone())]);
    let branches = info("t2", "Branch master", TabKind::Table, &[("Branch Code", text.clone()), ("Region", text.clone())]);
    let numeric = info("t4", "Branch numbers", TabKind::Table, &[("Branch Code", num.clone())]);
    let chart = info("t5", "Chart of Accounts", TabKind::Chart, &[("Branch Code", text.clone())]);
    let broken_tab = info("t6", "Rates", TabKind::Placeholder, &[]);
    let link = |d: &str, l: &str| Link { data_tab: d.into(), lookup_tab: l.into(), keys: vec![pair("Branch Code", "Branch Code")], unmatched_value: None };
    let model = Model { links: vec![link("t1", "t2")], ..Default::default() };
    let say = |m: &Model, l: &Link, replacing: Option<usize>, d: &TabInfo, k: &TabInfo| format!("{:#}", validate_link(m, l, replacing, d, k).err().expect("refused"));
    // the lookup as the data tab
    assert_eq!(say(&model, &link("t2", "t3"), None, &branches, &gl), "'Branch master' is a lookup; a lookup can't have links of its own");
    // the data tab as a lookup
    assert_eq!(say(&model, &link("t3", "t1"), None, &gl, &accounts), "'Accounts' is a data tab with links of its own; it can't also be a lookup");
    // a second link between the same two tabs
    assert_eq!(say(&model, &link("t1", "t2"), None, &accounts, &branches), "'Accounts' is already linked to 'Branch master'. Edit that link instead.");
    // ... but editing that link is not a second link
    assert!(validate_link(&model, &link("t1", "t2"), Some(0), &accounts, &branches).is_ok());
    // a different type
    assert_eq!(
        say(&model, &link("t3", "t4"), None, &gl, &numeric),
        "'Branch Code' is text in 'GL' and a number in 'Branch numbers'. Set both to text in Schema & Ingestion first."
    );
    // chart tabs and tabs that did not open
    assert!(say(&model, &link("t3", "t5"), None, &gl, &chart).contains("'Chart of Accounts' is a chart tab; a chart tab can't be linked"));
    assert!(say(&model, &link("t6", "t2"), None, &broken_tab, &branches).contains("'Rates' did not open"));
    // the others: a tab to itself, no columns, a column twice, a column that is not there, more than three
    assert!(say(&Model::default(), &link("t1", "t1"), None, &accounts, &accounts).contains("can't be linked to itself"));
    let none = Link { keys: vec![], ..link("t3", "t2") };
    assert!(say(&model, &none, None, &gl, &branches).contains("at least one key column"));
    let twice = Link { keys: vec![pair("Branch Code", "Branch Code"), pair("Branch Code", "Region")], ..link("t3", "t2") };
    assert!(say(&model, &twice, None, &gl, &branches).contains("a column can be used once"));
    let missing = Link { keys: vec![pair("Branch Code", "Nope")], ..link("t3", "t2") };
    assert_eq!(say(&model, &missing, None, &gl, &branches), "'Branch master' has no column 'Nope'.");
    let four = Link { keys: (0..4).map(|i| pair(&format!("a{i}"), &format!("b{i}"))).collect(), ..link("t3", "t2") };
    assert!(say(&model, &four, None, &gl, &branches).contains("at most 3 key columns"));
    // a good link, one lookup for two data tabs
    assert!(validate_link(&model, &link("t3", "t2"), None, &gl, &branches).is_ok(), "the branch master serves the accounts and the GL");
    // the same column pairs in different types of one family are named
    let small = info("t7", "Small", TabKind::Table, &[("Branch Code", DataType::Int32)]);
    let big = info("t8", "Big", TabKind::Table, &[("Branch Code", DataType::Int64)]);
    assert!(say(&model, &link("t7", "t8"), None, &small, &big).contains("Set both to the same type"));
}

fn csv_source(path: &Path) -> TabSource {
    TabSource::SingleFile { path: path.to_path_buf(), is_parquet: false, config: micro_parquet::engine::auto_detect_smart_schema(path).expect("a schema") }
}

fn snap(id: u64, key: &str, title: &str, source: TabSource) -> TabSnapshot {
    TabSnapshot { id, key: Some(key.into()), title: title.into(), source, steps: vec![], exec_steps: 0, col_formats: Default::default(), summary: Default::default(), control_col: None, chart: None, chart_tab: None, kept: None }
}

struct Rig {
    root: PathBuf,
    paths: PortablePaths,
}

fn rig(name: &str) -> Rig {
    let root = dir(name);
    for f in ["data", "proj", "temp"] {
        std::fs::create_dir_all(root.join(f)).expect("folder");
    }
    std::fs::write(root.join("data/Accounts.csv"), "BRANCH,BAL\nB1,100\nB2,200\nB3,300\n").expect("accounts");
    std::fs::write(root.join("data/Branches.csv"), "CODE,REGION\nB1,Dhaka\nB2,Dhaka\nB3,Sylhet\n").expect("branches");
    micro_parquet::rules::set_data_dir_override(Some(root.clone()));
    let paths = PortablePaths { root_dir: root.clone(), macros_dir: root.join("m"), schemas_dir: root.join("s"), curves_dir: root.join("c"), buckets_file: root.join("b.json"), temp_dir: root.join("temp") };
    Rig { root, paths }
}

fn model_of(rig: &Rig) -> (Vec<TabSnapshot>, Model) {
    let snaps = vec![snap(1, "t1", "Accounts", csv_source(&rig.root.join("data/Accounts.csv"))), snap(2, "t2", "Branch master", csv_source(&rig.root.join("data/Branches.csv")))];
    let link = Link { data_tab: "t1".into(), lookup_tab: "t2".into(), keys: vec![pair("BRANCH", "CODE")], unmatched_value: Some("BAL".into()) };
    (snaps, Model { links: vec![link], ..Default::default() })
}

fn save(rig: &Rig, snaps: &[TabSnapshot], model: Model, name: &str) -> PathBuf {
    let pfile = rig.root.join("proj").join(format!("{name}.mpproj"));
    let mut cache = HashCache::new();
    let mut saved = build_project(snaps, Some(1), &project_dir_of(&pfile), &rig.paths, &mut cache).expect("builds");
    saved.project.model = model;
    write_project(&pfile, &saved.project).expect("writes");
    pfile
}

fn without_saved_at(text: &str) -> String {
    text.lines().filter(|l| !l.trim_start().starts_with("\"saved_at\"")).collect::<Vec<_>>().join("\n")
}

#[test]
fn m0_a_project_with_a_model_is_version_2_and_round_trips_and_one_without_is_version_1_with_no_model_section() {
    let r = rig("p9m0_project");
    let (snaps, model) = model_of(&r);
    // with a model: version 2, a model section
    let with = save(&r, &snaps, model.clone(), "with");
    let text = std::fs::read_to_string(&with).expect("text");
    let v: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(v["project_version"], 2);
    assert_eq!(v["model"]["links"][0]["lookup_tab"], "t2");
    assert_eq!(v["model"]["links"][0]["keys"][0], serde_json::json!({"data": "BRANCH", "lookup": "CODE"}));
    assert_eq!(v["model"]["links"][0]["unmatched_value"], "BAL");
    assert!(v["model"].get("calendar").is_none() && v["model"].get("measures").is_none(), "the parts that are empty are not written: {}", v["model"]);
    // save -> open -> save gives the same bytes but saved_at
    let loaded = load_project(&with).expect("opens");
    assert_eq!(loaded.model, model);
    assert_eq!(loaded.project_version, 2);
    let again = r.root.join("proj/again.mpproj");
    write_project(&again, &loaded).expect("writes again");
    assert_eq!(without_saved_at(&std::fs::read_to_string(&again).expect("again")), without_saved_at(&text), "the same bytes but saved_at");
    // without a model: version 1 and no model section, as Phase 8 wrote it
    let without = save(&r, &snaps, Model::default(), "without");
    let t = std::fs::read_to_string(&without).expect("text");
    let v: serde_json::Value = serde_json::from_str(&t).expect("json");
    assert_eq!(v["project_version"], 1);
    assert!(v.get("model").is_none(), "no model section in a project without a model");
    // even if the version in memory says 2
    let mut p = load_project(&without).expect("opens");
    p.project_version = 2;
    assert!(p.to_json().expect("text").contains("\"project_version\": 1"));
    // an old project (no model section) opens with an empty model
    assert!(load_project(&without).expect("opens").model.is_empty());
}

#[test]
fn m0_the_v1_8_0_app_refuses_a_version_2_project_with_its_newer_app_message() {
    let r = rig("p9m0_refuse");
    let (snaps, model) = model_of(&r);
    let with = save(&r, &snaps, model, "with");
    let text = std::fs::read_to_string(&with).expect("text");
    let v: serde_json::Value = serde_json::from_str(&text).expect("json");
    // the version check as v1.8.0 has it: it reads up to version 1
    let e = check_version(&v, 1).err().expect("refused by v1.8.0");
    assert_eq!(format!("{e:#}"), format!("This project was saved by a newer MicroParquet v{}. Update the app to open it.", env!("CARGO_PKG_VERSION")));
    // this app reads it
    assert!(check_version(&v, PROJECT_VERSION_MODEL).is_ok());
    // a project of a version this app does not know is still refused
    let e = Project::from_json(r#"{"project_version": 3, "app_version": "9.9.0", "tabs": []}"#, "x.mpproj").err().expect("refused");
    assert_eq!(format!("{e:#}"), "This project was saved by a newer MicroParquet v9.9.0. Update the app to open it.");
}

/// Opens the project's tabs as the app does (without a window) and gives each tab for the key check.
fn open_tabs(project: &Project, pfile: &Path, relinks: BTreeMap<String, PathBuf>, paths: &PortablePaths) -> Vec<ModelTab> {
    let mut opener = Opener::new(project.clone(), pfile, relinks, paths.clone());
    let mut out = Vec::new();
    for i in opener.order() {
        let pt = project.tabs[i].clone();
        match opener.prepare(i) {
            Ok(Prepared::Table { source, steps }) => {
                opener.built(&pt.key, source.clone(), steps.clone());
                out.push(ModelTab { key: pt.key.clone(), title: pt.title.clone(), source, steps });
            }
            other => panic!("the tab {} did not open: {:?}", pt.key, other.is_ok()),
        }
    }
    out
}

#[test]
fn m0_relinking_a_lookup_file_to_one_with_a_duplicate_key_makes_the_link_broken_in_the_open_line() {
    let r = rig("p9m0_relink");
    let (snaps, model) = model_of(&r);
    let pfile = save(&r, &snaps, model, "relink");
    let project = load_project(&pfile).expect("opens");
    let status_line = |relinks: BTreeMap<String, PathBuf>| -> String {
        let tabs = open_tabs(&project, &pfile, relinks, &r.paths);
        let find = |k: &str| tabs.iter().find(|t| t.key == k).expect("tab").clone();
        let mut broken = Vec::new();
        for l in &project.model.links {
            let (d, k) = (find(&l.data_tab), find(&l.lookup_tab));
            let st = match check_link(l, &d, &k, &r.paths.temp_dir, &|| Ok(())) {
                Ok(c) => c.status(),
                Err(e) => LinkStatus::Failed(format!("{e:#}")),
            };
            if st.is_broken() {
                broken.push((link_name(&d.title, &k.title), st.reason()));
            }
        }
        open_line(&opened_summary(tabs.len(), &[]), &broken)
    };
    assert_eq!(status_line(BTreeMap::new()), "2 tabs opened.", "the files as saved: nothing to say");
    // this month's branch file has B2 twice
    let this_month = r.root.join("data/Branches_new.csv");
    std::fs::write(&this_month, "CODE,REGION\nB1,Dhaka\nB2,Dhaka\nB2,Khulna\nB3,Sylhet\n").expect("new file");
    let saved_path = project.tabs.iter().find_map(|t| match &t.recipe {
        Recipe::File(f) if t.key == "t2" => Some(f.source.file.path.clone()),
        _ => None,
    });
    let mut relinks = BTreeMap::new();
    relinks.insert(saved_path.expect("the lookup's file"), this_month);
    assert_eq!(status_line(relinks), "2 tabs opened; link 'Accounts \u{2192} Branch master' is broken: 1 duplicate key");
}

#[test]
fn m0_the_open_line_the_status_bar_and_the_status_texts_say_what_they_should() {
    assert_eq!(open_line("9 tabs opened.", &[]), "9 tabs opened.");
    assert_eq!(
        open_line("9 tabs opened.", &[("Accounts \u{2192} Branch master".to_string(), "3 duplicate keys".to_string())]),
        "9 tabs opened; link 'Accounts \u{2192} Branch master' is broken: 3 duplicate keys"
    );
    assert_eq!(
        open_line("9 tabs opened.", &[("A \u{2192} B".to_string(), "3 duplicate keys".to_string()), ("C \u{2192} D".to_string(), "2 blank lookup keys".to_string())]),
        "9 tabs opened; link 'A \u{2192} B' is broken: 3 duplicate keys; link 'C \u{2192} D' is broken: 2 blank lookup keys"
    );
    assert_eq!(status_bar_text(0, 0), None);
    assert_eq!(status_bar_text(4, 1).as_deref(), Some("Model: 4 links \u{b7} 1 broken"));
    assert_eq!(status_bar_text(1, 0).as_deref(), Some("Model: 1 link"));
    assert_eq!(LinkStatus::Ok.text(), "OK");
    assert_eq!(LinkStatus::OkUnmatched(12).text(), "OK \u{b7} 12 unmatched keys");
    assert_eq!(LinkStatus::OkUnmatched(1).text(), "OK \u{b7} 1 unmatched key");
    assert_eq!(LinkStatus::Broken(vec!["3 duplicate keys".into(), "2 blank lookup keys".into()]).text(), "Broken: 3 duplicate keys, 2 blank lookup keys");
    assert_eq!(LinkStatus::Failed("the tab 'Branch master' is closed".into()).text(), "Can't check: the tab 'Branch master' is closed");
    assert!(LinkStatus::Failed("x".into()).is_broken() && !LinkStatus::OkUnmatched(3).is_broken());
}

#[test]
fn m0_a_tab_that_changed_since_the_link_was_made_says_so_instead_of_guessing() {
    let d = dir("p9m0_changed");
    let data = tab("t1", "Accounts", parquet(&d, "d", &DataFrame::new(vec![strs("Branch", &["B1"])]).expect("d")));
    let look = tab("t2", "Branch master", parquet(&d, "l", &DataFrame::new(vec![strs("Code", &["B1"])]).expect("l")));
    let gone = Link { data_tab: "t1".into(), lookup_tab: "t2".into(), keys: vec![pair("Branch", "Gone")], unmatched_value: None };
    let e = check_link(&gone, &data, &look, &d, &|| Ok(())).err().expect("refused");
    assert_eq!(format!("{e:#}"), "'Branch master' has no column 'Gone'");
    let numeric = tab("t2", "Branch master", parquet(&d, "l2", &DataFrame::new(vec![Series::new("Code".into(), [1i64])]).expect("l")));
    let link = Link { data_tab: "t1".into(), lookup_tab: "t2".into(), keys: vec![pair("Branch", "Code")], unmatched_value: None };
    let e = check_link(&link, &data, &numeric, &d, &|| Ok(())).err().expect("refused");
    assert!(format!("{e:#}").contains("'Branch' is text in 'Accounts' and a number in 'Branch master' now"), "{e:#}");
}
