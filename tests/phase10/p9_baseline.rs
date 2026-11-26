//! The Phase 9 baseline of Phase 10's goal 3: a project with a model (links, calendar, measures), a chart tab, a Nested pivot tab and
//! a saved pack, each made and saved by the Phase 9 code (the merge commit a3271e6 of `main`, version 1.9.0), and the blake3 hash of what
//! that code gave for them. `tests/fixtures/phase9_replay/` holds the files and `phase9_replay.hashes.json` the hashes;
//! `d0_the_phase9_samples_replay_to_the_hashes_the_phase9_code_gave` loads the files with today's code and compares.
//!
//! This file uses only APIs that Phase 9 had, so the same code made the hashes: `generate` writes the files and then calls `replay`
//! on them, and the test calls `replay` on a copy of the committed files. Do not edit the fixtures by hand. To make them again: check
//! out the commit in a worktree, copy this file in and run the ignored `d0_write_the_phase9_replay_fixture` (see `tests/fixtures/README.md`).

#![allow(dead_code)]

use crate::p9_m6 as m6;
use crate::p9_p8_baseline::{chart_data_hash, csv_of, hash};
use micro_parquet::charts::calc::compute;
use micro_parquet::charts::pack::PackChart;
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec};
use micro_parquet::model::*;
use micro_parquet::model_calendar::date_link;
use micro_parquet::models::*;
use micro_parquet::packs::*;
use micro_parquet::project::*;
use polars::prelude::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const NESTED: PivotLayout = PivotLayout::Nested { blank_line_after_group: false };

fn link_branch() -> Link {
    Link { data_tab: "t1".into(), lookup_tab: "t3".into(), keys: vec![KeyPair { data: "Branch".into(), lookup: "Branch code".into() }], unmatched_value: None }
}

fn sum_balance() -> PivotValue {
    PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None }
}

/// A model pivot: the CASA ratio, CASA and deposits closing by region (a lookup field) and month (a Calendar field), for Oct 2026.
pub fn model_pivot() -> PivotSpec {
    let region = ModelFieldRef::new(&link_branch(), "Accounts \u{2192} Branch master", "Branch master", "Region");
    let my = ModelFieldRef::new(&date_link("t1", "Date", "t6"), "Accounts \u{2192} Calendar", "Calendar", "Month-Year");
    PivotSpec {
        rows: vec![region.name.clone()],
        columns: vec![my.name.clone()],
        values: vec![crate::p9_m5::plain_measure("CASA ratio"), crate::p9_m5::plain_measure("CASA closing"), crate::p9_m5::plain_measure("Deposits closing")],
        filters: vec![PivotFilter { field: my.name.clone(), values: vec![Some("Oct 2026".into())] }],
        column_totals: true,
        model_fields: vec![region, my],
        data_tab: "t1".into(),
        ..Default::default()
    }
}

/// A Nested pivot of the balance by Wing › Product code.
pub fn nested_pivot() -> PivotSpec {
    PivotSpec { rows: vec!["Wing".into(), "Product code".into()], values: vec![sum_balance()], row_totals: true, column_totals: true, layout: NESTED, ..Default::default() }
}

/// The chart tab's chart: the balance by wing, as a bar chart.
pub fn wing_chart() -> ChartSpec {
    ChartSpec { category: "Wing".into(), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 500, ..Default::default() }
}

pub fn pack() -> ReportPack {
    ReportPack {
        name: "p9replay".into(),
        inputs: vec![PackInput { slot: "Accounts".into(), required_columns: vec!["Date".into(), "Wing".into(), "Product code".into(), "Balance".into()], ..Default::default() }],
        items: vec![
            PackItem {
                title: "Balance by wing".into(),
                sheet: "Wings".into(),
                source: PackItemKind::Pivot { slot: "Accounts".into(), pivot: Some(TransformStep::from_pivot_spec(PivotSpec { rows: vec!["Wing".into()], values: vec![sum_balance()], row_totals: true, column_totals: true, ..Default::default() })) },
                chart: Some(PackChart::Saved { name: "Wing pivot chart".into() }),
                ..Default::default()
            },
            PackItem { title: "Accounts".into(), sheet: "Accounts".into(), source: PackItemKind::Steps { slot: "Accounts".into() }, chart: None, ..Default::default() },
        ],
        output: PackOutput::default(),
    }
}

fn csv_source(path: &Path) -> TabSource {
    TabSource::SingleFile { path: path.to_path_buf(), is_parquet: false, config: micro_parquet::engine::auto_detect_smart_schema(path).expect("a schema") }
}

fn snap(id: u64, key: &str, title: &str, source: TabSource, steps: Vec<TransformStep>) -> TabSnapshot {
    let n = steps.len();
    TabSnapshot { id, key: Some(key.into()), title: title.into(), source, steps, exec_steps: n, col_formats: Default::default(), summary: Default::default(), control_col: None, chart: None, chart_tab: None, kept: None }
}

/// The project's tabs: the five data tabs, the Calendar, the model pivot, the Nested pivot and a chart tab (t1 to t9).
fn session(data: &Path, temp: &Path) -> Vec<TabSnapshot> {
    let accounts = csv_source(&data.join("Accounts.csv"));
    let (_, chart_src) = chart_tab_source(&accounts, &[], &wing_chart(), temp).expect("the chart table");
    let mut s = vec![
        snap(1, "t1", "Accounts", accounts.clone(), vec![]),
        snap(2, "t2", "GL TB", csv_source(&data.join("GL_TB.csv")), vec![]),
        snap(3, "t3", "Branch master", csv_source(&data.join("Branch_master.csv")), vec![]),
        snap(4, "t4", "Product master", csv_source(&data.join("Product_master.csv")), vec![]),
        snap(5, "t5", "GL map", csv_source(&data.join("GL_map.csv")), vec![]),
        snap(6, "t6", "Calendar", TabSource::Calendar { settings: crate::p9_m3::settings(1), first_year: 2025, last_year: 2027 }, vec![]),
        snap(7, "t7", "CASA by region", accounts.clone(), vec![TransformStep::from_pivot_spec(model_pivot())]),
        snap(8, "t8", "Balance by wing and product", accounts, vec![TransformStep::from_pivot_spec(nested_pivot())]),
        snap(9, "t9", "Chart: Balance by Wing", chart_src, vec![]),
    ];
    s[8].chart_tab = Some((wing_chart(), 1));
    s
}

fn portable(root: &Path) -> PortablePaths {
    PortablePaths { root_dir: root.to_path_buf(), macros_dir: root.join("macros"), schemas_dir: root.join("schemas"), curves_dir: root.join("curves"), buckets_file: root.join("buckets.json"), temp_dir: root.join("temp") }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("folder");
    for e in std::fs::read_dir(from).expect("read the folder").flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).expect("copy");
        }
    }
}

/// The project's text as the hash sees it: without `saved_at` and `app_version`, with the files' modified times set to 0 and the folder written as `<ROOT>`.
fn project_text(p: &Project, root: &Path) -> String {
    fn zero_modified(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, x) in m.iter_mut() {
                    if k == "modified" && x.is_number() {
                        *x = serde_json::Value::from(0);
                    } else {
                        zero_modified(x);
                    }
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(zero_modified),
            _ => {}
        }
    }
    let mut p = p.clone();
    p.saved_at.clear();
    p.app_version.clear();
    let mut v: serde_json::Value = serde_json::from_str(&p.to_json().expect("json").replace(&root.display().to_string(), "<ROOT>")).expect("json");
    zero_modified(&mut v);
    serde_json::to_string_pretty(&v).expect("json")
}

/// Opens every tab the way the app does, without a window, and builds the project text again (save → open → save).
fn save_open_save(project: &Project, pfile: &Path, relinks: BTreeMap<String, PathBuf>, paths: &PortablePaths) -> Project {
    let mut opener = Opener::new(project.clone(), pfile, relinks, paths.clone());
    let mut snaps: Vec<Option<TabSnapshot>> = vec![None; project.tabs.len()];
    for i in opener.order() {
        let pt = project.tabs[i].clone();
        match opener.prepare(i) {
            Err(why) => panic!("tab {} did not open: {why}", pt.key),
            Ok(Prepared::Table { source, steps }) => {
                opener.built(&pt.key, source.clone(), steps.clone());
                let mut s = snap(i as u64 + 1, &pt.key, &pt.title, source, steps);
                s.col_formats = pt.col_formats.iter().map(|(k, v)| (k.clone(), *v)).collect();
                s.summary = pt.summary.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                s.control_col = pt.control_col.clone();
                s.chart = pt.chart.clone();
                snaps[i] = Some(s);
            }
            Ok(Prepared::Chart { source, steps, spec, source_key }) => {
                let (_, src) = chart_tab_source(&source, &steps, &spec, &paths.temp_dir).expect("the chart table");
                opener.built(&pt.key, src.clone(), vec![]);
                let mut s = snap(i as u64 + 1, &pt.key, &pt.title, src, vec![]);
                let src_id = project.tabs.iter().position(|t| t.key == source_key).map(|p| p as u64 + 1).expect("the source tab");
                s.chart_tab = Some((spec, src_id));
                snaps[i] = Some(s);
            }
        }
    }
    let snaps: Vec<TabSnapshot> = snaps.into_iter().flatten().collect();
    let active = snaps.iter().find(|s| s.key.as_deref() == Some(project.active_tab.as_str())).map(|s| s.id);
    let again = build_project(&snaps, active, &project_dir_of(pfile), paths, &mut HashCache::new()).expect("builds again");
    assert!(again.excluded.is_empty(), "every tab can be written: {:?}", again.excluded);
    let mut p = again.project;
    p.model = project.model.clone();
    p
}

/// Loads every file of the baseline from `fixture` (copied to `work` first, so nothing in it changes) with the code that is running,
/// and gives the hashes of what comes out: key → blake3.
pub fn replay(fixture: &Path, work: &Path) -> BTreeMap<String, String> {
    let _ = std::fs::remove_dir_all(work);
    copy_dir(fixture, work);
    let paths = portable(work);
    std::fs::create_dir_all(&paths.temp_dir).expect("temp");
    let mut got = BTreeMap::new();
    let accounts = csv_source(&work.join("data/Accounts.csv"));

    // the project: every tab's rows, as the headless export writes them (model, Calendar, Nested and chart tabs included)
    let pfile = work.join("proj/phase9_replay.mpproj");
    let project = load_project(&pfile).expect("the Phase 9 project loads");
    assert_eq!(project.tabs.len(), 9, "nine tabs");
    assert!(!project.model.is_empty(), "the model is in the project");
    let out = work.join("out");
    std::fs::create_dir_all(&out).expect("out");
    let written = export_project_headless(&pfile, &out, &paths).unwrap_or_else(|e| panic!("headless: {e:#}"));
    assert_eq!(written.len(), 9, "{written:?}");
    let names = export_file_names(&project.tabs.iter().map(|t| t.title.clone()).collect::<Vec<_>>());
    for (t, name) in project.tabs.iter().zip(&names) {
        let file = written.iter().find(|p| p.file_name().is_some_and(|n| n.to_string_lossy() == name.as_str())).unwrap_or_else(|| panic!("the tab '{}' in {written:?}", t.title));
        let df = LazyFrame::scan_parquet(file, Default::default()).expect("scan").collect().expect("the tab");
        got.insert(format!("project:{}", t.key), hash(&csv_of(&df)));
    }

    // the Nested view of the Nested pivot, and the chart tab's chart numbers and layout
    let lf = micro_parquet::engine::compile_tab_lazy(&accounts, &[], &paths.temp_dir).expect("accounts");
    let view = micro_parquet::pivot_view::build_nested(lf.clone(), &nested_pivot()).expect("nested");
    let names: Vec<String> = view.values.get_column_names().iter().map(|n| n.to_string()).collect();
    got.insert("nested:view".to_string(), hash(&format!("{:?}|{:?}|{}", view.rows.iter().map(|r| (r.level, r.label.clone())).collect::<Vec<_>>(), names, csv_of(&view.values))));
    got.insert("chart:Balance by Wing".to_string(), chart_data_hash(&compute(lf, &wing_chart()).unwrap_or_else(|e| panic!("chart: {e:#}"))));

    // the saved chart and the pack: each sheet's rows and each chart on it
    let (name, chart) = micro_parquet::charts::library::load_chart(&work.join("charts/Balance by wing.json")).expect("the Phase 9 chart loads");
    let lf = micro_parquet::engine::compile_tab_lazy(&accounts, &[], &paths.temp_dir).expect("accounts");
    got.insert(format!("saved-chart:{name}"), chart_data_hash(&compute(lf, &chart.at_level()).unwrap_or_else(|e| panic!("chart: {e:#}"))));
    let pack = load_pack(work, "p9replay").expect("the Phase 9 pack loads");
    let spec = PackRunSpec { files: BTreeMap::from([("Accounts".to_string(), work.join("data/Accounts.csv"))]), cutoff: "2026-10-31".to_string() };
    let r = run_pack(&pack, &spec, &paths, "2026-11-25 10:00:00", &micro_parquet::jobs::JobCtx::inert()).unwrap_or_else(|e| panic!("pack: {e:#}"));
    for s in &r.sheets {
        got.insert(format!("pack:{}", s.sheet), hash(&csv_of(&s.df)));
        for (i, c) in s.charts.iter().enumerate() {
            got.insert(format!("pack-chart:{}:{i}", s.sheet), chart_data_hash(&c.data));
        }
    }

    // save → open → save
    let relinks: BTreeMap<String, PathBuf> = source_rows(&project, &project_dir_of(&pfile), &BTreeMap::new())
        .iter()
        .map(|r| (r.saved_path.clone(), work.join("data").join(Path::new(&r.saved_path).file_name().expect("a file name"))))
        .collect();
    let again = save_open_save(&project, &pfile, relinks, &paths);
    got.insert("project:bytes after save-open-save".to_string(), hash(&project_text(&again, work)));
    got
}

/// Makes the files of the baseline in `out` and writes `phase9_replay.hashes.json` there: run with the Phase 9 code.
pub fn generate(out: &Path, scratch: &Path) {
    let _ = std::fs::remove_dir_all(scratch);
    std::fs::create_dir_all(out.join("data")).expect("data folder");
    std::fs::create_dir_all(out.join("charts")).expect("charts folder");
    std::fs::create_dir_all(out.join("proj")).expect("project folder");
    copy_dir(&m6::fixture_root().join("data"), &out.join("data"));
    micro_parquet::charts::library::save_chart(&out.join("charts"), "Balance by wing", &wing_chart()).expect("saves the chart");
    micro_parquet::charts::library::save_chart(&out.join("charts"), "Wing pivot chart", &ChartSpec { value: Some("Sum of Balance".into()), ..wing_chart() }).expect("saves the chart");
    save_pack(out, &pack()).expect("saves the pack");
    let temp = scratch.join("save_temp");
    std::fs::create_dir_all(&temp).expect("temp");
    let paths = PortablePaths { temp_dir: temp.clone(), ..portable(out) };
    let pfile = out.join("proj/phase9_replay.mpproj");
    let mut saved = build_project(&session(&out.join("data"), &temp), Some(7), &project_dir_of(&pfile), &paths, &mut HashCache::new()).expect("builds");
    assert!(saved.excluded.is_empty(), "every tab can be written: {:?}", saved.excluded);
    saved.project.model = m6::model();
    write_project(&pfile, &saved.project).expect("writes the project");
    let hashes = replay(out, &scratch.join("replay"));
    std::fs::write(out.join("phase9_replay.hashes.json"), serde_json::to_string_pretty(&hashes).expect("json") + "\n").expect("hashes");
}
