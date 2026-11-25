//! W4: project files. A project of tabs (a file, a file with steps, a join, a pivot tab, a gap ladder run, a chart
//! tab) round-trips: every tab equals the original by the hash of its rows, and the order, the active tab, the
//! formats and the chart specs are the same. Moving the project and its files together opens through the relative
//! paths; relinking works with an extra column and is refused, with the columns listed, when one is missing; a
//! changed file says so; save → open → save gives the same JSON but `saved_at`; headless writes the same Parquet
//! and a failing tab writes nothing.

use crate::common::dir;
use micro_parquet::charts::spec::{ChartAgg, ChartSpec};
use micro_parquet::models::*;
use micro_parquet::project::*;
use micro_parquet::tool_runs::ToolRun;
use polars::prelude::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const LOANS: &str = "ACNO,BAL,MAT,CAT,GRADE\nL1,1000000,2027-03-31,Continuous,STD\nL2,2000000,2028-10-31,Demand,SS\nL3,500000,2026-12-31,Continuous,STD\nL4,750000,2029-06-30,Term,BL\n";
const RATES: &str = "CAT,RATE\nContinuous,9.5\nDemand,10.5\nTerm,11.25\n";

struct Rig {
    root: PathBuf,
    paths: PortablePaths,
}

fn rig(name: &str) -> Rig {
    let root = dir(name);
    for d in ["data", "proj", "temp"] {
        std::fs::create_dir_all(root.join(d)).expect("folder");
    }
    std::fs::write(root.join("data/Loans.csv"), LOANS).expect("loans");
    std::fs::write(root.join("data/Rates.csv"), RATES).expect("rates");
    micro_parquet::rules::set_data_dir_override(Some(root.clone()));
    let paths = PortablePaths { root_dir: root.clone(), macros_dir: root.join("m"), schemas_dir: root.join("s"), curves_dir: root.join("c"), buckets_file: root.join("b.json"), temp_dir: root.join("temp") };
    Rig { root, paths }
}

fn csv_source(path: &Path) -> TabSource {
    TabSource::SingleFile { path: path.to_path_buf(), is_parquet: false, config: micro_parquet::engine::auto_detect_smart_schema(path).expect("a schema") }
}

fn rows_hash(source: &TabSource, steps: &[TransformStep], temp: &Path) -> String {
    let lf = micro_parquet::engine::compile_tab_lazy(source, steps, temp).expect("compiles");
    let mut df = lf.collect().expect("collects");
    let mut buf: Vec<u8> = Vec::new();
    CsvWriter::new(&mut buf).finish(&mut df).expect("csv");
    blake3::hash(&buf).to_hex().to_string()
}

fn snap(id: u64, title: &str, source: TabSource, steps: Vec<TransformStep>) -> TabSnapshot {
    let n = steps.len();
    TabSnapshot { id, key: None, title: title.to_string(), source, steps, exec_steps: n, col_formats: Default::default(), summary: Default::default(), control_col: None, chart: None, chart_tab: None, kept: None }
}

fn pivot_step() -> TransformStep {
    TransformStep::from_pivot_spec(PivotSpec { rows: vec!["CAT".into()], values: vec![PivotValue { field: "BAL".into(), agg: PivotAgg::Sum, weight_col: None, time: None }], row_totals: true, column_totals: true, ..Default::default() })
}

fn chart_spec() -> ChartSpec {
    ChartSpec { category: "CAT".into(), value: Some("BAL".into()), agg: ChartAgg::Sum, ..Default::default() }
}

/// The six kinds of tab (and the rates file the join reads): ids 1 to 7.
fn session(r: &Rig) -> Vec<TabSnapshot> {
    let loans = csv_source(&r.root.join("data/Loans.csv"));
    let rates = csv_source(&r.root.join("data/Rates.csv"));
    let by_cat = TransformStep::SelectAndReorderColumns { cols: vec!["ACNO".into(), "BAL".into(), "CAT".into()] };
    let join = TabSource::JoinedTabs {
        left_source: Box::new(loans.clone()),
        left_steps: vec![],
        right_source: Box::new(rates.clone()),
        right_steps: vec![],
        join_kind: SqlJoinKind::Left,
        join_keys: vec!["CAT".into()],
        right_cols_to_keep: vec!["RATE".into()],
        keep_first_duplicate_right: false,
    };
    let gap = TabSource::GapLadder {
        inputs: vec![GapTabInput { source: loans.clone(), steps: vec![], spec: micro_parquet::banking::GapInputSpec { role: micro_parquet::banking::GapRole::Asset, amount_col: "BAL".into(), maturity_col: "MAT".into() } }],
        preset: CustomBucketDef::default_presets()[0].clone(),
        cutoff_date: "2026-10-31".into(),
        null_as_sight: true,
        limits: None,
        behaviour: None,
    };
    // the chart tab: the chart's table, in a Parquet file the program made
    let (_, chart_src) = chart_tab_source(&loans, &[], &chart_spec(), &r.paths.temp_dir).expect("the chart table");
    let mut s = vec![
        snap(1, "Loans.csv", loans.clone(), vec![]),
        snap(2, "Loans by category", loans.clone(), vec![by_cat]),
        snap(3, "Rates.csv", rates, vec![]),
        snap(4, "Join(Loans.csv+Rates.csv)", join, vec![]),
        snap(5, "Pivot: CAT", loans.clone(), vec![pivot_step()]),
        snap(6, "Gap ladder (SLP)", gap, vec![]),
        snap(7, "Chart: Sum of BAL by CAT", chart_src, vec![]),
    ];
    s[0].col_formats.insert("BAL".into(), ColumnFormatSpec { fmt_type: MetricFormatType::BdtCrore, decimals: 2 });
    s[0].summary.insert("BAL".into(), SummarySpec::default());
    s[0].control_col = Some("BAL".into());
    s[0].chart = Some(serde_json::to_value(chart_spec()).expect("json"));
    s[6].chart_tab = Some((chart_spec(), 1));
    s
}

/// Opens a project the way the app does, without a window: each tab is built in order and its rows hashed.
fn open_all(project: &Project, project_path: &Path, relinks: BTreeMap<String, PathBuf>, paths: &PortablePaths) -> (Vec<(String, std::result::Result<String, String>)>, Vec<TabSnapshot>) {
    let mut opener = Opener::new(project.clone(), project_path, relinks, paths.clone());
    let mut hashes: Vec<(String, std::result::Result<String, String>)> = Vec::new();
    let mut snaps: Vec<Option<TabSnapshot>> = vec![None; project.tabs.len()];
    for i in opener.order() {
        let pt = project.tabs[i].clone();
        match opener.prepare(i) {
            Err(why) => {
                opener.failed(&pt.key, &why);
                hashes.push((pt.key.clone(), Err(why)));
            }
            Ok(Prepared::Table { source, steps }) => {
                hashes.push((pt.key.clone(), Ok(rows_hash(&source, &steps, &paths.temp_dir))));
                opener.built(&pt.key, source.clone(), steps.clone());
                let mut s = snap(i as u64 + 1, &pt.title, source, steps);
                s.key = Some(pt.key.clone());
                s.col_formats = pt.col_formats.iter().map(|(k, v)| (k.clone(), *v)).collect();
                s.summary = pt.summary.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                s.control_col = pt.control_col.clone();
                s.chart = pt.chart.clone();
                snaps[i] = Some(s);
            }
            Ok(Prepared::Chart { source, steps, spec, source_key }) => {
                let (_, src) = chart_tab_source(&source, &steps, &spec, &paths.temp_dir).expect("the chart table");
                hashes.push((pt.key.clone(), Ok(rows_hash(&src, &[], &paths.temp_dir))));
                opener.built(&pt.key, src.clone(), vec![]);
                let mut s = snap(i as u64 + 1, &pt.title, src, vec![]);
                s.key = Some(pt.key.clone());
                let src_id = project.tabs.iter().position(|t| t.key == source_key).map(|p| p as u64 + 1).expect("the source tab");
                s.chart_tab = Some((spec, src_id));
                snaps[i] = Some(s);
            }
        }
    }
    hashes.sort();
    (hashes, snaps.into_iter().flatten().collect())
}

fn save(r: &Rig, snaps: &[TabSnapshot], active: u64, name: &str) -> (PathBuf, SavedProject) {
    let pfile = r.root.join("proj").join(format!("{name}.mpproj"));
    let mut cache = HashCache::new();
    let saved = build_project(snaps, Some(active), &project_dir_of(&pfile), &r.paths, &mut cache).expect("builds");
    write_project(&pfile, &saved.project).expect("writes");
    (pfile, saved)
}

#[test]
fn w4_a_project_of_six_kinds_of_tab_round_trips_by_the_hash_of_its_rows() {
    let r = rig("p8w4_round");
    let snaps = session(&r);
    let (pfile, saved) = save(&r, &snaps, 6, "all");
    assert!(saved.excluded.is_empty(), "every tab can be written: {:?}", saved.excluded);
    let kinds: Vec<&str> = saved.project.tabs.iter().map(|t| match &t.recipe {
        Recipe::File(_) => "file",
        Recipe::Run(r) => match r.run {
            ToolRun::Join(_) => "join",
            ToolRun::GapLadder(_) => "gap",
            _ => "run",
        },
        Recipe::Chart(_) => "chart",
    }).collect();
    assert_eq!(kinds, vec!["file", "file", "file", "join", "file", "gap", "chart"], "a pivot tab is its source's recipe and its steps (W4-1)");
    assert_eq!(saved.project.tabs.iter().map(|t| t.key.as_str()).collect::<Vec<_>>(), vec!["t1", "t2", "t3", "t4", "t5", "t6", "t7"], "stable keys in tab order");
    assert_eq!(saved.project.active_tab, "t6");
    let Recipe::Run(run) = &saved.project.tabs[3].recipe else { panic!("a run") };
    assert_eq!(run.run.source_names(), vec!["t1".to_string(), "t3".to_string()], "a run names its sources by project key");
    let Recipe::Chart(c) = &saved.project.tabs[6].recipe else { panic!("a chart") };
    assert_eq!(c.source, "t1");

    // open: every tab equals the original by the hash of its rows
    let loaded = load_project(&pfile).expect("reads");
    let (hashes, rebuilt) = open_all(&loaded, &pfile, BTreeMap::new(), &r.paths);
    assert_eq!(hashes.len(), 7);
    for (i, s) in snaps.iter().enumerate() {
        let want = rows_hash(&s.source, &s.steps, &r.paths.temp_dir);
        let got = hashes.iter().find(|(k, _)| *k == format!("t{}", i + 1)).map(|(_, h)| h.clone()).expect("a hash");
        assert_eq!(got, Ok(want), "tab '{}' has the same rows", s.title);
    }
    // order, titles, formats, summary, control column, chart choices and the chart tab's spec are the same
    assert_eq!(loaded.tabs.iter().map(|t| t.title.clone()).collect::<Vec<_>>(), snaps.iter().map(|s| s.title.clone()).collect::<Vec<_>>());
    assert_eq!(loaded.active_tab, "t6");
    assert_eq!(loaded.tabs[0].col_formats.get("BAL").map(|f| f.fmt_type), Some(MetricFormatType::BdtCrore));
    assert!(loaded.tabs[0].summary.contains_key("BAL"));
    assert_eq!(loaded.tabs[0].control_col.as_deref(), Some("BAL"));
    assert_eq!(loaded.tabs[0].chart, snaps[0].chart);
    let Recipe::Chart(c) = &loaded.tabs[6].recipe else { panic!("a chart") };
    assert_eq!(serde_json::to_value(&c.spec).expect("json"), serde_json::to_value(chart_spec()).expect("json"));
    assert_eq!(rebuilt.len(), 7);

    // save → open → save: the same JSON but saved_at
    let again = build_project(&rebuilt, Some(6), &project_dir_of(&pfile), &r.paths, &mut HashCache::new()).expect("builds again");
    let strip = |p: &Project| {
        let mut p = p.clone();
        p.saved_at.clear();
        p.to_json().expect("json")
    };
    assert_eq!(strip(&again.project), strip(&saved.project), "save, open and save again give the same file but saved_at");
    // and the text is canonical: the same project writes the same bytes
    assert_eq!(saved.project.to_json().expect("json"), saved.project.to_json().expect("json"));
}

#[test]
fn w4_moving_the_project_folder_and_its_files_together_opens_through_the_relative_paths() {
    let r = rig("p8w4_move");
    let snaps = session(&r);
    let (pfile, saved) = save(&r, &snaps[..3], 1, "move");
    let Recipe::File(f) = &saved.project.tabs[0].recipe else { panic!("a file") };
    assert_eq!(f.source.file.rel_path, "../data/Loans.csv", "the path from the project's folder");
    assert!(f.source.file.path.ends_with("data/Loans.csv"));
    assert_eq!(f.source.file.size, LOANS.len() as u64);
    assert_eq!(f.source.file.blake3.len(), 64);
    // move the whole folder
    let moved = r.root.with_file_name(format!("{}_moved", r.root.file_name().expect("name").to_string_lossy()));
    let _ = std::fs::remove_dir_all(&moved);
    std::fs::rename(&r.root, &moved).expect("moved");
    let new_pfile = moved.join("proj/move.mpproj");
    let loaded = load_project(&new_pfile).expect("reads");
    let rows = source_rows(&loaded, &project_dir_of(&new_pfile), &BTreeMap::new());
    assert_eq!(rows.len(), 2, "one row per file");
    assert!(rows.iter().all(|r| r.status == SourceStatus::Found), "found through the relative path, same size and time: {:?}", rows.iter().map(|r| (&r.name, &r.status)).collect::<Vec<_>>());
    assert!(rows[0].path.starts_with(&moved), "{:?}", rows[0].path);
    let paths = PortablePaths { temp_dir: moved.join("temp"), ..r.paths.clone() };
    let (hashes, _) = open_all(&loaded, &new_pfile, BTreeMap::new(), &paths);
    assert!(hashes.iter().all(|(_, h)| h.is_ok()), "every tab opens: {hashes:?}");
    let _ = std::fs::remove_dir_all(&moved);
}

#[test]
fn w4_relinking_to_a_file_with_an_extra_column_works_and_a_missing_column_is_refused_with_the_list() {
    let r = rig("p8w4_relink");
    let snaps = session(&r);
    let (pfile, saved) = save(&r, &snaps[..6], 1, "relink");
    let saved_path = match &saved.project.tabs[0].recipe {
        Recipe::File(f) => f.source.file.path.clone(),
        _ => panic!("a file"),
    };
    // this month's file: the same columns and one more, other rows
    let this_month = r.root.join("data/Loans_2026-11.csv");
    std::fs::write(&this_month, "ACNO,BAL,MAT,CAT,GRADE,EXTRA\nL1,1100000,2027-03-31,Continuous,STD,x\nL9,10,2030-01-31,Term,SS,y\n").expect("file");
    let loaded = load_project(&pfile).expect("reads");
    let mut relinks = BTreeMap::new();
    relinks.insert(saved_path.clone(), this_month.clone());
    let rows = source_rows(&loaded, &project_dir_of(&pfile), &relinks);
    let row = rows.iter().find(|x| x.name == "Loans.csv").expect("the Loans row");
    assert!(row.relinked && row.path == this_month, "{row:?}");
    assert!(matches!(row.status, SourceStatus::Changed { .. }), "a different file is Changed: {:?}", row.status);
    let (hashes, _) = open_all(&loaded, &pfile, relinks.clone(), &r.paths);
    assert!(hashes.iter().all(|(_, h)| h.is_ok()), "every tab opens on the new file: {hashes:?}");
    let t1 = hashes.iter().find(|(k, _)| k == "t1").and_then(|(_, h)| h.clone().ok()).expect("t1");
    assert_ne!(t1, rows_hash(&snaps[0].source, &[], &r.paths.temp_dir), "the rows are this month's");
    // a file without the MAT column is refused with the columns listed, and the tabs built on it do not open
    let short = r.root.join("data/Loans_short.csv");
    std::fs::write(&short, "ACNO,BAL,CAT,GRADE\nL1,1,Term,STD\n").expect("file");
    let mut relinks = BTreeMap::new();
    relinks.insert(saved_path, short);
    let (hashes, _) = open_all(&loaded, &pfile, relinks, &r.paths);
    let get = |k: &str| hashes.iter().find(|(x, _)| x == k).map(|(_, h)| h.clone()).expect("a result");
    let why = get("t1").expect_err("the file is refused");
    assert!(why.contains("no 'MAT' column") && why.contains("Columns in the file: ACNO, BAL, CAT, GRADE"), "{why}");
    assert!(get("t2").is_err() && get("t5").is_err(), "the other tabs on that file do not open either");
    assert!(get("t3").is_ok(), "a file that is fine opens");
    let join = get("t4").expect_err("the join is built on the refused file");
    assert!(join.contains("'t1'") && join.contains("did not open"), "{join}");
    assert!(get("t6").expect_err("the gap ladder").contains("did not open"));
    assert_eq!(opened_summary(1, &[("Loans.csv".into(), "the file has no 'MAT' column".into())]), "1 tab opened, 1 failed: Loans.csv \u{2014} the file has no 'MAT' column");
    assert_eq!(opened_summary(9, &[("Gap ladder".into(), "the file has no 'MATDAT' column".into())]), "9 tabs opened, 1 failed: Gap ladder \u{2014} the file has no 'MATDAT' column");
}

#[test]
fn w4_the_sources_say_found_changed_or_missing_and_look_in_folder_relinks_by_name() {
    let r = rig("p8w4_sources");
    let snaps = session(&r);
    let (pfile, _) = save(&r, &snaps[..3], 1, "src");
    let loaded = load_project(&pfile).expect("reads");
    let pdir = project_dir_of(&pfile);
    let status = |rows: &[SourceRow], n: &str| rows.iter().find(|r| r.name == n).map(|r| r.status.clone()).expect("a row");
    let rows = source_rows(&loaded, &pdir, &BTreeMap::new());
    assert_eq!((status(&rows, "Loans.csv"), status(&rows, "Rates.csv")), (SourceStatus::Found, SourceStatus::Found));
    // Changed: the size differs, and both are reported; Missing: the file is gone
    std::fs::write(r.root.join("data/Loans.csv"), format!("{LOANS}L5,1,2031-01-31,Term,STD\n")).expect("grow");
    let moved = r.root.join("elsewhere");
    std::fs::create_dir_all(&moved).expect("folder");
    std::fs::rename(r.root.join("data/Rates.csv"), moved.join("Rates.csv")).expect("moved");
    let rows = source_rows(&loaded, &pdir, &BTreeMap::new());
    match status(&rows, "Loans.csv") {
        SourceStatus::Changed { saved, now } => assert!(saved.0 == LOANS.len() as u64 && now.0 > saved.0, "{saved:?} {now:?}"),
        other => panic!("changed: {other:?}"),
    }
    assert_eq!(status(&rows, "Rates.csv"), SourceStatus::Missing);
    // Look in folder: the missing file is found by its name
    let mut relinks = BTreeMap::new();
    assert_eq!(look_in_folder(&rows, &r.root, &mut relinks), 1);
    assert_eq!(relinks.len(), 1);
    assert!(relinks.values().next().is_some_and(|p| p == &moved.join("Rates.csv")), "{relinks:?}");
    // the changed file says so, in the status line, with both dates; a relinked file does not
    let (warnings, hashes) = changed_since_saved(&loaded, &pdir, &relinks, &|| false);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].starts_with("Loans.csv changed since the project was saved (saved ") && warnings[0].contains(", file ") && warnings[0].ends_with(')'), "{}", warnings[0]);
    assert_eq!(hashes.len(), 1);
}

#[test]
fn w4_tabs_that_cannot_be_rebuilt_are_left_out_with_the_reason_and_so_are_the_tabs_built_on_them() {
    let r = rig("p8w4_excluded");
    let mut snaps = session(&r);
    // a tab on a temporary file the program made, and a join built on it
    let tmp_path = r.paths.temp_dir.join("chart_data_1_1.parquet");
    let mut df = df!("CAT" => &["a"], "BAL" => &[1.0f64]).expect("df");
    ParquetWriter::new(std::fs::File::create(&tmp_path).expect("file")).finish(&mut df).expect("parquet");
    let tmp = TabSource::SingleFile { path: tmp_path.clone(), is_parquet: true, config: IngestionConfig::default() };
    snaps.push(snap(8, "Chart data", tmp.clone(), vec![]));
    let loans = csv_source(&r.root.join("data/Loans.csv"));
    snaps.push(snap(
        9,
        "Join(Loans.csv+Chart data)",
        TabSource::JoinedTabs { left_source: Box::new(loans), left_steps: vec![], right_source: Box::new(tmp), right_steps: vec![], join_kind: SqlJoinKind::Left, join_keys: vec!["CAT".into()], right_cols_to_keep: vec!["BAL".into()], keep_first_duplicate_right: false },
        vec![],
    ));
    // a chart tab whose source tab is closed
    let mut orphan = snap(10, "Chart: orphan", snaps[6].source.clone(), vec![]);
    orphan.chart_tab = Some((chart_spec(), 99));
    snaps.push(orphan);
    let (_, saved) = save(&r, &snaps, 1, "excluded");
    let titles: Vec<&str> = saved.excluded.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(titles, vec!["Chart data", "Join(Loans.csv+Chart data)", "Chart: orphan"], "{:?}", saved.excluded);
    assert!(saved.excluded[0].reason.contains("copy of a chart's table"), "{}", saved.excluded[0].reason);
    assert!(saved.excluded[1].reason.contains("built on") || saved.excluded[1].reason.contains("made from"), "{}", saved.excluded[1].reason);
    assert!(saved.excluded[2].reason.contains("closed"), "{}", saved.excluded[2].reason);
    assert_eq!(saved.project.tabs.len(), 7, "the others are saved");
    // the keys of the tabs that were left out are still given, so they do not change when they can be saved
    assert_eq!(saved.keys.len(), 10);
}

#[test]
fn w4_a_tab_that_did_not_open_keeps_its_recipe_when_the_project_is_saved_again() {
    let r = rig("p8w4_kept");
    let snaps = session(&r);
    let (pfile, saved) = save(&r, &snaps[..2], 1, "kept");
    // the placeholder of t2: its recipe is kept as it was written
    let kept = saved.project.tabs[1].clone();
    let mut placeholder = snap(2, "Loans by category", TabSource::Unavailable { reason: "the file Loans.csv is missing".into() }, vec![]);
    placeholder.key = Some("t2".into());
    placeholder.kept = Some(kept);
    let mut first = snaps[0].clone();
    first.key = Some("t1".into());
    let again = build_project(&[first, placeholder], Some(1), &project_dir_of(&pfile), &r.paths, &mut HashCache::new()).expect("builds");
    assert!(again.excluded.is_empty());
    let (a, b) = (serde_json::to_value(&again.project.tabs[1]).expect("json"), serde_json::to_value(&saved.project.tabs[1]).expect("json"));
    assert_eq!(a, b, "saving again does not lose the tab");
    // and a placeholder has no rows and cannot be compiled
    let e = micro_parquet::engine::compile_tab_lazy(&TabSource::Unavailable { reason: "the file is missing".into() }, &[], &r.paths.temp_dir).err().expect("an error");
    assert!(format!("{e:#}").contains("could not be opened: the file is missing"), "{e:#}");
    let t = TabState::unavailable(5, "Gap ladder".into(), "the file has no 'MATDAT' column".into(), vec![]);
    assert_eq!((t.total_rows, t.cached_slice.height()), (0, 0));
}

#[test]
fn w4_a_newer_project_is_refused_unknown_fields_are_ignored_and_the_recent_list_keeps_eight() {
    let newer = r#"{"project_version": 3, "app_version": "9.9.0", "tabs": []}"#;
    let e = Project::from_json(newer, "x.mpproj").err().expect("refused");
    assert_eq!(format!("{e:#}"), "This project was saved by a newer MicroParquet v9.9.0. Update the app to open it.");
    let p = Project::from_json(r#"{"project_version": 1, "future": {"a": 1}, "tabs": [{"key": "t1", "title": "T", "recipe": {"kind": "file", "future": 1, "source": {"file": {"name": "a.csv"}}}, "later": true}]}"#, "x.mpproj").expect("unknown fields are ignored");
    assert_eq!((p.tabs.len(), p.tabs[0].key.as_str()), (1, "t1"));
    assert!(Project::from_json("{ not json", "x.mpproj").is_err());
    let mut recent: Vec<String> = Vec::new();
    for i in 0..10 {
        push_recent(&mut recent, Path::new(&format!("/p/{i}.mpproj")));
    }
    push_recent(&mut recent, Path::new("/p/5.mpproj"));
    assert_eq!(recent.len(), RECENT_MAX);
    assert_eq!(recent[0], "/p/5.mpproj", "newest first, no duplicate");
    assert_eq!(recent.iter().filter(|r| *r == "/p/5.mpproj").count(), 1);
    // settings.json keeps the list and an old file without it loads
    let d = dir("p8w4_settings");
    let s = micro_parquet::settings::Settings { recent_projects: recent.clone(), ..Default::default() };
    s.save(&d).expect("saves");
    assert_eq!(micro_parquet::settings::Settings::load(&d).0.recent_projects, recent);
    std::fs::write(micro_parquet::settings::settings_path(&d), r#"{"right_panel_width": 400.0}"#).expect("old");
    assert!(micro_parquet::settings::Settings::load(&d).0.recent_projects.is_empty());
}

#[test]
fn w4_headless_writes_every_tab_as_parquet_and_a_failing_tab_writes_nothing() {
    let r = rig("p8w4_headless");
    let snaps = session(&r);
    let (pfile, _) = save(&r, &snaps, 1, "headless");
    let out = r.root.join("out");
    let written = export_project_headless(&pfile, &out, &r.paths).expect("exports");
    assert_eq!(written.len(), 7, "{written:?}");
    let titles: Vec<String> = snaps.iter().map(|s| s.title.clone()).collect();
    let file_names = export_file_names(&titles);
    let mut names: Vec<String> = written.iter().map(|p| p.file_name().expect("name").to_string_lossy().to_string()).collect();
    names.sort();
    let mut want_names = file_names.clone();
    want_names.sort();
    assert_eq!(names, want_names, "one file per tab, named by its title");
    // the same rows as the tabs, by hash (read back from Parquet)
    for (i, s) in snaps.iter().enumerate() {
        let back = TabSource::SingleFile { path: out.join(&file_names[i]), is_parquet: true, config: IngestionConfig::default() };
        assert_eq!(rows_hash(&back, &[], &r.paths.temp_dir), rows_hash(&s.source, &s.steps, &r.paths.temp_dir), "'{}' has the same rows in the Parquet file", s.title);
    }
    assert!(!std::fs::read_dir(&out).expect("dir").flatten().any(|e| e.file_name().to_string_lossy().starts_with(".mpproj")), "no staging folder is left");
    // a missing file stops the run with its name, and nothing is written
    std::fs::remove_file(r.root.join("data/Rates.csv")).expect("removed");
    let out2 = r.root.join("out2");
    let e = export_project_headless(&pfile, &out2, &r.paths).err().expect("a failing tab stops the run");
    assert!(format!("{e:#}").contains("Rates.csv"), "{e:#}");
    let left: Vec<_> = std::fs::read_dir(&out2).map(|d| d.flatten().collect()).unwrap_or_default();
    assert!(left.is_empty(), "nothing is written: {left:?}");
}

#[test]
fn w4_save_open_save_writes_the_same_file_bytes_but_the_saved_at_line() {
    let r = rig("p8w4_bytes");
    let snaps = session(&r);
    let (pfile, _) = save(&r, &snaps, 6, "first");
    let loaded = load_project(&pfile).expect("reads");
    let (hashes, rebuilt) = open_all(&loaded, &pfile, BTreeMap::new(), &r.paths);
    assert!(hashes.iter().all(|(_, h)| h.is_ok()), "every tab opens: {hashes:?}");
    let (pfile2, _) = save(&r, &rebuilt, 6, "first");
    // the same name in the same folder: the second write replaces the first; keep the first's bytes to compare
    let second = std::fs::read(&pfile2).expect("second file");
    let (first_path, _) = save(&r, &snaps, 6, "other");
    let first = std::fs::read(&first_path).expect("first file");
    let strip = |b: &[u8]| -> Vec<u8> { String::from_utf8(b.to_vec()).expect("utf-8").lines().filter(|l| !l.contains("\"saved_at\"")).collect::<Vec<_>>().join("\n").into_bytes() };
    assert!(String::from_utf8_lossy(&first).contains("\"saved_at\""), "the file has a saved_at line");
    assert_eq!(strip(&first), strip(&second), "the two files are the same bytes but the saved_at line");
}

#[test]
fn w4_the_save_job_reports_each_file_it_hashes_and_the_cache_spares_the_second_save() {
    let r = rig("p8w4_hashing");
    // a file of 3 MiB, so the hash reports several times
    let big = r.root.join("data/Big.csv");
    let mut text = String::from("ACNO,BAL\n");
    while text.len() < 3 << 20 {
        text.push_str("L100000,12345678\n");
    }
    std::fs::write(&big, text).expect("big file");
    let snaps = vec![snap(1, "Big.csv", csv_source(&big), vec![])];
    let seen = std::cell::RefCell::new(Vec::<(String, u64, u64)>::new());
    let mut cache = HashCache::new();
    let pdir = project_dir_of(&r.root.join("proj/x.mpproj"));
    let built = build_project_progress(&snaps, Some(1), &pdir, &r.paths, &mut cache, &|n, read, total| {
        seen.borrow_mut().push((n.to_string(), read, total));
        false
    })
    .expect("builds");
    assert!(built.excluded.is_empty());
    let calls = seen.borrow().clone();
    assert!(calls.len() >= 3, "the hash reports as it reads: {}", calls.len());
    assert!(calls.iter().all(|(n, _, t)| n == "Big.csv" && *t == 3 << 20 || n == "Big.csv" && *t > 3 << 20), "{calls:?}");
    assert!(calls.windows(2).all(|w| w[0].1 < w[1].1), "bytes read only grow");
    assert_eq!(calls.last().map(|c| c.1), calls.last().map(|c| c.2), "the last report is the whole file");
    // the same file again: the cache has its hash, so nothing is read and nothing is reported
    let again = std::cell::Cell::new(0usize);
    let second = build_project_progress(&snaps, Some(1), &pdir, &r.paths, &mut cache, &|_, _, _| {
        again.set(again.get() + 1);
        false
    })
    .expect("builds again");
    assert_eq!(again.get(), 0, "a file that did not change is not hashed again");
    let Recipe::File(a) = &built.project.tabs[0].recipe else { panic!("a file") };
    let Recipe::File(b) = &second.project.tabs[0].recipe else { panic!("a file") };
    assert_eq!(a.source.file.blake3, b.source.file.blake3);
    // a stop request ends the hash with an error and writes nothing
    let stopped = build_project_progress(&snaps, Some(1), &pdir, &r.paths, &mut HashCache::new(), &|_, _, _| true);
    assert!(stopped.is_err() || !stopped.expect("ok").excluded.is_empty(), "a cancelled save is not a saved project");
    // the line the job shows
    assert_eq!(hashing_text("Loans.csv", 40, 100), ("Hashing Loans.csv 40%".to_string(), 0.4));
    assert_eq!(hashing_text("Loans.csv", 100, 100).0, "Hashing Loans.csv 100%");
    assert_eq!(hashing_text("Empty.csv", 0, 0).0, "Hashing Empty.csv 100%");
}
