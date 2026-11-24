//! The Phase 8 baseline of Phase 9's goal 3: sample macros, a pack, a saved chart and a project, each made and saved by
//! the Phase 8 code (the tag `v1.8.0`, in a temporary git worktree that shared this repository's build cache), and the
//! blake3 hash of what that code gave for them. `tests/fixtures/phase8_replay/` holds the files and
//! `phase8_replay.hashes.json` the hashes; `b0_the_phase8_samples_replay_to_the_hashes_the_phase8_code_gave` loads the
//! files with today's code and compares.
//!
//! This file uses only APIs that Phase 8 had, so the same code made the hashes: `generate` writes the files and then
//! calls `replay` on them, and the test calls `replay` on a copy of the committed files. Do not edit the fixtures by
//! hand. To make them again: check out the tag in a worktree, copy this file in, and call `generate` from a throw-away
//! integration test (see `tests/fixtures/README.md`).

#![allow(dead_code)]

use micro_parquet::charts::calc::compute;
use micro_parquet::charts::colours::ColourPick;
use micro_parquet::charts::data::ChartData;
use micro_parquet::charts::data_table::data_table;
use micro_parquet::charts::layout::{layout_chart, ApproxMeasure, LayoutOpts};
use micro_parquet::charts::pack::PackChart;
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec, DataTable, DrillLevel, Stack};
use micro_parquet::dates::{CalendarSettings, DateFieldRef, DatePart};
use micro_parquet::models::*;
use micro_parquet::packs::*;
use micro_parquet::project::*;
use micro_parquet::tool_runs::ToolRun;
use polars::prelude::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The year starts in July and the week on Monday: not the defaults, so the parts of a date show the calendar.
pub const CAL: CalendarSettings = CalendarSettings { fy_start_month: 7, week_start: micro_parquet::dates::Weekday::Mon };

/// 60 deposits, a new one every 11 days from 6 Jan 2025 (so the dates cross years, quarters and weeks), three
/// branches, three products, two segments, a rate that is blank on every 17th row, and a maturity a year later.
pub fn deposits_csv() -> String {
    let start = chrono::NaiveDate::from_ymd_opt(2025, 1, 6).expect("a date");
    let mut s = String::from("Open Date,Branch,Product,Segment,Rate,Balance,Maturity\n");
    for i in 0..60i64 {
        let d = start + chrono::Duration::days(i * 11);
        let rate = if i % 17 == 5 { String::new() } else { format!("{:.1}", 6.5 + (i % 9) as f64 * 0.5) };
        s.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            d.format("%Y-%m-%d"),
            ["Dhaka", "Ctg", "Sylhet"][(i % 3) as usize],
            ["Gold", "Silver", "Classic"][((i / 3) % 3) as usize],
            ["Retail", "Corporate"][(i % 2) as usize],
            rate,
            1_000_000 + ((i * 37) % 11) * 250_000,
            (d + chrono::Duration::days(365)).format("%Y-%m-%d")
        ));
    }
    s
}

pub const BRANCHES_CSV: &str = "Branch,Region\nDhaka,Central\nCtg,Southern\nSylhet,Northern\n";

fn filter(column: &str, op: CondOp, part: DatePart, value: &str) -> TransformStep {
    TransformStep::FilterGroup {
        conditions: vec![FilterCondition { column: column.into(), op, value: value.into(), part: Some(DateFieldRef::new(column, part, CAL)), ..Default::default() }],
        join: GroupJoin::And,
        keep_matching: true,
    }
}

/// The macros of the baseline: (file stem, steps).
pub fn macros() -> Vec<(&'static str, Vec<TransformStep>)> {
    let bands = RateBandDef { numbered: false, ..RateBandDef::custom("Owner", vec![0.0, 7.0, 8.0, 9.0, 10.0, 11.0]) };
    vec![
        ("phase8_split_date", vec![TransformStep::SplitDate { column: "Open Date".into(), parts: vec![DatePart::Year, DatePart::MonthYear, DatePart::DayName, DatePart::FiscalYear], calendar: CAL, prefix: "Opened".into() }]),
        ("phase8_filter_part_gte", vec![filter("Open Date", CondOp::Gte, DatePart::MonthYear, "Aug 2025")]),
        ("phase8_rate_band_unnumbered", vec![TransformStep::RateBand { rate_col: "Rate".into(), band_def: bands, new_col: "Rate Band".into() }]),
    ]
}

fn month_year() -> DateFieldRef {
    DateFieldRef::new("Open Date", DatePart::MonthYear, CAL)
}

fn sum_balance() -> PivotValue {
    PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None }
}

/// A pivot of the balance by Open Date · Month-Year (a date field).
pub fn month_pivot() -> TransformStep {
    TransformStep::from_pivot_spec(PivotSpec { rows: vec![month_year().name], values: vec![sum_balance()], row_totals: true, column_totals: true, date_fields: vec![month_year()], ..Default::default() })
}

/// The saved chart: Balance by Product and Segment, stacked, with the drill Product › Branch › Year, its own filter on the
/// branch, a colour pick, segment labels and the data table below.
pub fn drill_chart() -> ChartSpec {
    let mut s = ChartSpec { category: "Product".into(), series: Some("Segment".into()), value: Some("Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, stack: Stack::Stacked, top_n: 500, segment_labels: true, data_table: DataTable::Below, ..Default::default() };
    s.set_drill(vec![DrillLevel::column("Product"), DrillLevel::column("Branch"), DrillLevel::of_part(&DateFieldRef::new("Open Date", DatePart::Year, CAL))]);
    s.filters = vec![FilterCondition { column: "Branch".into(), op: CondOp::InList, values: vec!["Dhaka".into(), "Ctg".into()], ..Default::default() }];
    s.colour_map.assign_all(&["Retail", "Corporate"]);
    s.colour_map.set_pick("Retail", ColourPick::swatch(2, 2));
    s
}

/// The chart of the pack's pivot item: the pivot's balance by month.
pub fn month_chart() -> ChartSpec {
    ChartSpec { category: month_year().name, value: Some("Sum of Balance".into()), agg: ChartAgg::Sum, kind: ChartKind::Bar, top_n: 500, ..Default::default() }
}

/// The pack: the deposits table with the drill chart, and a pivot by month with its chart.
pub fn pack() -> ReportPack {
    ReportPack {
        name: "p8replay".into(),
        inputs: vec![PackInput { slot: "Deposits".into(), required_columns: vec!["Open Date".into(), "Branch".into(), "Product".into(), "Segment".into(), "Rate".into(), "Balance".into(), "Maturity".into()], ..Default::default() }],
        items: vec![
            PackItem { title: "Deposits by month".into(), sheet: "Months".into(), source: PackItemKind::Pivot { slot: "Deposits".into(), pivot: Some(month_pivot()) }, chart: Some(PackChart::Saved { name: "Deposits by month".into() }), ..Default::default() },
            PackItem { title: "Deposits".into(), sheet: "Deposits".into(), source: PackItemKind::Steps { slot: "Deposits".into() }, chart: Some(PackChart::Saved { name: "Deposits drill".into() }), ..Default::default() },
        ],
        output: PackOutput::default(),
    }
}

pub fn hash(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

pub fn csv_of(df: &DataFrame) -> String {
    let mut df = df.clone();
    let mut buf = Vec::new();
    CsvWriter::new(&mut buf).finish(&mut df).expect("CSV");
    String::from_utf8(buf).expect("UTF-8")
}

/// The numbers of a chart and how it is laid out in both themes (the shapes and their text), with its data table.
pub fn chart_data_hash(d: &ChartData) -> String {
    let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(1200.0, 700.0));
    let mut text = format!("{:?}|{:?}|{:?}|{:?}|{}", d.categories, d.series, d.values, d.rows, d.total_rows);
    for dark in [false, true] {
        let opts = LayoutOpts { dark, divisor: 1.0, decimals: 2, unit: "BDT".into(), bare: false, colours: None };
        let l = layout_chart(d, rect, &opts, &ApproxMeasure);
        text.push_str(&format!("|{:?}|{:?}|{:?}", l.prims, l.segment_labels, data_table(d, &opts)));
    }
    hash(&text)
}

fn csv_source(path: &Path) -> TabSource {
    TabSource::SingleFile { path: path.to_path_buf(), is_parquet: false, config: micro_parquet::engine::auto_detect_smart_schema(path).expect("a schema") }
}

fn rows_hash(source: &TabSource, steps: &[TransformStep], temp: &Path) -> String {
    let lf = micro_parquet::engine::compile_tab_lazy(source, steps, temp).expect("compiles");
    hash(&csv_of(&lf.collect().expect("collects")))
}

fn snap(id: u64, title: &str, source: TabSource, steps: Vec<TransformStep>) -> TabSnapshot {
    let n = steps.len();
    TabSnapshot { id, key: None, title: title.to_string(), source, steps, exec_steps: n, col_formats: Default::default(), summary: Default::default(), control_col: None, chart: None, chart_tab: None, kept: None }
}

/// The project's tabs: a file, a file with steps, the branch file, a join, a pivot tab, a gap ladder run and a chart tab
/// at drill level 2 (the six kinds of tab and the file the join reads): ids 1 to 7.
pub fn session(data: &Path, temp: &Path) -> Vec<TabSnapshot> {
    let deposits = csv_source(&data.join("Deposits.csv"));
    let branches = csv_source(&data.join("Branches.csv"));
    let steps = vec![macros()[0].1[0].clone(), macros()[1].1[0].clone()];
    let join = TabSource::JoinedTabs {
        left_source: Box::new(deposits.clone()),
        left_steps: vec![],
        right_source: Box::new(branches.clone()),
        right_steps: vec![],
        join_kind: SqlJoinKind::Left,
        join_keys: vec!["Branch".into()],
        right_cols_to_keep: vec!["Region".into()],
        keep_first_duplicate_right: false,
    };
    let gap = TabSource::GapLadder {
        inputs: vec![GapTabInput { source: deposits.clone(), steps: vec![], spec: micro_parquet::banking::GapInputSpec { role: micro_parquet::banking::GapRole::Asset, amount_col: "Balance".into(), maturity_col: "Maturity".into() } }],
        preset: CustomBucketDef::default_presets()[0].clone(),
        cutoff_date: "2026-10-31".into(),
        null_as_sight: true,
        limits: None,
        behaviour: None,
    };
    let mut level2 = drill_chart();
    assert!(level2.drill_down("Gold") && level2.drill_down("Dhaka"), "two levels down");
    assert_eq!(level2.depth(), 2);
    let (_, chart_src) = chart_tab_source(&deposits, &[], &level2, temp).expect("the chart table");
    let mut s = vec![
        snap(1, "Deposits.csv", deposits.clone(), vec![]),
        snap(2, "Deposits by date part", deposits.clone(), steps),
        snap(3, "Branches.csv", branches, vec![]),
        snap(4, "Join(Deposits.csv+Branches.csv)", join, vec![]),
        snap(5, "Pivot: Month-Year", deposits.clone(), vec![month_pivot()]),
        snap(6, "Gap ladder (SLP)", gap, vec![]),
        snap(7, "Chart: Balance by Open Date Year", chart_src, vec![]),
    ];
    s[0].col_formats.insert("Balance".into(), ColumnFormatSpec { fmt_type: MetricFormatType::BdtCrore, decimals: 2 });
    s[0].control_col = Some("Balance".into());
    s[0].chart = Some(serde_json::to_value(drill_chart()).expect("json"));
    s[6].chart_tab = Some((level2, 1));
    s
}

fn portable(root: &Path) -> PortablePaths {
    PortablePaths { root_dir: root.to_path_buf(), macros_dir: root.join("macros"), schemas_dir: root.join("schemas"), curves_dir: root.join("curves"), buckets_file: root.join("buckets.json"), temp_dir: root.join("temp") }
}

/// Opens a project the way the app does, without a window: each tab is built in order and its rows hashed.
fn open_all(project: &Project, project_path: &Path, relinks: BTreeMap<String, PathBuf>, paths: &PortablePaths) -> (BTreeMap<String, String>, Vec<TabSnapshot>) {
    let mut opener = Opener::new(project.clone(), project_path, relinks, paths.clone());
    let mut hashes = BTreeMap::new();
    let mut snaps: Vec<Option<TabSnapshot>> = vec![None; project.tabs.len()];
    for i in opener.order() {
        let pt = project.tabs[i].clone();
        match opener.prepare(i) {
            Err(why) => panic!("tab {} did not open: {why}", pt.key),
            Ok(Prepared::Table { source, steps }) => {
                hashes.insert(format!("project:{}", pt.key), rows_hash(&source, &steps, &paths.temp_dir));
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
                hashes.insert(format!("project:{}", pt.key), rows_hash(&src, &[], &paths.temp_dir));
                opener.built(&pt.key, src.clone(), vec![]);
                let mut s = snap(i as u64 + 1, &pt.title, src, vec![]);
                s.key = Some(pt.key.clone());
                let src_id = project.tabs.iter().position(|t| t.key == source_key).map(|p| p as u64 + 1).expect("the source tab");
                s.chart_tab = Some((spec, src_id));
                snaps[i] = Some(s);
            }
        }
    }
    (hashes, snaps.into_iter().flatten().collect())
}

/// The project's text as the hash sees it: without `saved_at` and `app_version`, with the files' modified times set to 0
/// (a copy of a file has a new time), and with the folder the files are in written as `<ROOT>` (the same project is saved
/// from a different folder each time).
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

/// Loads every file of the baseline from `fixture` (copied to `work` first, so nothing in it changes) with the code that
/// is running, and gives the hashes of what comes out: key → blake3.
pub fn replay(fixture: &Path, work: &Path) -> BTreeMap<String, String> {
    let _ = std::fs::remove_dir_all(work);
    copy_dir(fixture, work);
    let paths = portable(work);
    std::fs::create_dir_all(&paths.temp_dir).expect("temp");
    let mut got = BTreeMap::new();

    // the macros: the rows each gives on the deposits file
    let deposits = csv_source(&work.join("data/Deposits.csv"));
    for (stem, _) in macros() {
        let r = micro_parquet::macros::load_macro_from_file(&work.join(format!("{stem}.mpq.json"))).unwrap_or_else(|e| panic!("{stem}: {e:#}"));
        got.insert(format!("macro:{stem}"), rows_hash(&deposits, &r.steps, &paths.temp_dir));
    }

    // the pack: each sheet's rows and each chart on it
    let pack = load_pack(work, "p8replay").expect("the Phase 8 pack loads");
    let spec = PackRunSpec { files: BTreeMap::from([("Deposits".to_string(), work.join("data/Deposits.csv"))]), cutoff: "2026-10-31".to_string() };
    let r = run_pack(&pack, &spec, &paths, "2026-11-23 10:00:00", &micro_parquet::jobs::JobCtx::inert()).unwrap_or_else(|e| panic!("pack: {e:#}"));
    for s in &r.sheets {
        got.insert(format!("pack:{}", s.sheet), hash(&csv_of(&s.df)));
        for (i, c) in s.charts.iter().enumerate() {
            got.insert(format!("pack-chart:{}:{i}", s.sheet), chart_data_hash(&c.data));
        }
    }

    // the saved chart on the deposits file
    let (name, chart) = micro_parquet::charts::library::load_chart(&work.join("charts/Deposits drill.json")).expect("the Phase 8 chart loads");
    let lf = micro_parquet::engine::compile_tab_lazy(&deposits, &[], &paths.temp_dir).expect("deposits");
    got.insert(format!("chart:{name}"), chart_data_hash(&compute(lf, &chart.at_level()).unwrap_or_else(|e| panic!("chart: {e:#}"))));

    // the project: every tab's rows, then save → open → save
    let pfile = work.join("proj/phase8_replay.mpproj");
    let project = load_project(&pfile).expect("the Phase 8 project loads");
    // the project remembers where its files were when it was saved: read the copies next to it, wherever this runs
    let relinks: BTreeMap<String, PathBuf> = source_rows(&project, &project_dir_of(&pfile), &BTreeMap::new())
        .iter()
        .map(|r| (r.saved_path.clone(), work.join("data").join(Path::new(&r.saved_path).file_name().expect("a file name"))))
        .collect();
    assert_eq!(relinks.len(), 2, "the deposits and the branches");
    let (rows, snaps) = open_all(&project, &pfile, relinks, &paths);
    assert_eq!(rows.len(), 7, "seven tabs");
    got.extend(rows);
    let active = snaps.iter().find(|s| s.key.as_deref() == Some(project.active_tab.as_str())).map(|s| s.id);
    let again = build_project(&snaps, active, &project_dir_of(&pfile), &paths, &mut HashCache::new()).expect("builds again");
    assert!(again.excluded.is_empty(), "every tab can be written: {:?}", again.excluded);
    if let Ok(d) = std::env::var("P8_DUMP") {
        std::fs::write(d, project_text(&again.project, work)).expect("dump");
    }
    got.insert("project:bytes after save-open-save".to_string(), hash(&project_text(&again.project, work)));
    got
}

/// Makes the files of the baseline in `out` and writes `phase8_replay.hashes.json` there: run with the Phase 8 code.
pub fn generate(out: &Path, scratch: &Path) {
    let _ = std::fs::remove_dir_all(scratch);
    std::fs::create_dir_all(out.join("data")).expect("data folder");
    std::fs::create_dir_all(out.join("charts")).expect("charts folder");
    std::fs::create_dir_all(out.join("proj")).expect("project folder");
    std::fs::write(out.join("data/Deposits.csv"), deposits_csv()).expect("deposits");
    std::fs::write(out.join("data/Branches.csv"), BRANCHES_CSV).expect("branches");
    for (stem, steps) in macros() {
        let recipe = MacroRecipe { name: stem.to_string(), description: String::new(), created_at: "2026-11-23 10:00:00".into(), enforce_balance_control: false, control_column: None, optional_ingestion_schema: None, steps, summary: Default::default() };
        micro_parquet::macros::save_macro_to_file(&recipe, &out.join(format!("{stem}.mpq.json"))).expect("saves the macro");
    }
    micro_parquet::charts::library::save_chart(&out.join("charts"), "Deposits drill", &drill_chart()).expect("saves the chart");
    micro_parquet::charts::library::save_chart(&out.join("charts"), "Deposits by month", &month_chart()).expect("saves the chart");
    save_pack(out, &pack()).expect("saves the pack");
    // the project: saved from the files in `out`, so its paths are relative to its own folder
    let temp = scratch.join("save_temp");
    std::fs::create_dir_all(&temp).expect("temp");
    let paths = portable(out);
    let snaps = session(&out.join("data"), &temp);
    let pfile = out.join("proj/phase8_replay.mpproj");
    let saved = build_project(&snaps, Some(6), &project_dir_of(&pfile), &PortablePaths { temp_dir: temp, ..paths }, &mut HashCache::new()).expect("builds");
    assert!(saved.excluded.is_empty(), "every tab can be written: {:?}", saved.excluded);
    assert!(matches!(&saved.project.tabs[5].recipe, Recipe::Run(r) if matches!(r.run, ToolRun::GapLadder(_))), "the gap ladder run");
    write_project(&pfile, &saved.project).expect("writes the project");
    // the hashes: what the same code gives when it loads those files
    let hashes = replay(out, &scratch.join("replay"));
    std::fs::write(out.join("phase8_replay.hashes.json"), serde_json::to_string_pretty(&hashes).expect("json") + "\n").expect("hashes");
}
