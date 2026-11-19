//! C6: charts in report packs. An item carries a saved chart or a template; the pack writes the native
//! Excel chart on a sheet beside the item's, the chart book as `<pack>-charts.html` when asked, and lists
//! the charts on the Cover. An item whose chart cannot be drawn stops the run and nothing is written.

use crate::common::*;
use micro_parquet::banking::{GapInputSpec, GapRole};
use micro_parquet::charts::pack::PackChart;
use micro_parquet::charts::spec::ChartSpec;
use micro_parquet::jobs::JobCtx;
use micro_parquet::models::*;
use micro_parquet::packs::*;
use micro_parquet::rules::set_data_dir_override;
use micro_parquet::tool_runs::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const LOANS: &str = "ACNO,BAL,RATE,MAT,CAT,GRADE\nL1,1000000,9.5,2027-03-31,Continuous,STD-0\nL2,2000000,12,2028-10-31,Demand,SS\nL3,500000,8,2026-12-15,Fixed term,B/L\n";
const DEPOSITS: &str = "ACNO,BAL,MAT,PROD\nD1,3000000,2026-12-31,SB\nD2,1500000,2027-06-30,TD\nD3,500000,2026-11-30,TD\n";

struct Rig {
    root: PathBuf,
    paths: PortablePaths,
    spec: PackRunSpec,
}

impl Rig {
    fn new(name: &str) -> Rig {
        let root = dir(name);
        set_data_dir_override(Some(root.clone()));
        let paths = PortablePaths { root_dir: root.clone(), macros_dir: root.join("macros"), schemas_dir: root.join("schemas"), curves_dir: root.join("curves"), buckets_file: root.join("buckets.json"), temp_dir: root.join("temp") };
        std::fs::create_dir_all(&paths.temp_dir).expect("temp");
        let (loans, deposits) = (root.join("loans.csv"), root.join("deposits.csv"));
        std::fs::write(&loans, LOANS).expect("loans");
        std::fs::write(&deposits, DEPOSITS).expect("deposits");
        let spec = PackRunSpec { files: BTreeMap::from([("Loans".to_string(), loans), ("Deposits".to_string(), deposits)]), cutoff: "2026-10-31".to_string() };
        Rig { root, paths, spec }
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        set_data_dir_override(None);
    }
}

fn slot(name: &str, required: &[&str]) -> PackInput {
    PackInput { slot: name.into(), required_columns: required.iter().map(|s| s.to_string()).collect(), ..Default::default() }
}

fn gap_run() -> ToolRun {
    ToolRun::GapLadder(GapLadderRun {
        inputs: vec![
            GapInputRun { source: "Loans".into(), spec: GapInputSpec { role: GapRole::Asset, amount_col: "BAL".into(), maturity_col: "MAT".into() }, product_col: String::new() },
            GapInputRun { source: "Deposits".into(), spec: GapInputSpec { role: GapRole::Liability, amount_col: "BAL".into(), maturity_col: "MAT".into() }, product_col: String::new() },
        ],
        preset: CustomBucketDef::default_presets()[0].name.clone(),
        preset_def: None,
        cutoff: "2020-01-31".into(),
        null_as_sight: true,
        behaviour_file: String::new(),
        stress_tdrr: false,
    })
}

/// A gap ladder with its template, and the deposits table with a saved chart.
fn pack() -> ReportPack {
    ReportPack {
        name: "monthly".into(),
        inputs: vec![slot("Loans", &["ACNO", "BAL", "MAT"]), slot("Deposits", &["ACNO", "BAL", "MAT", "PROD"])],
        items: vec![
            PackItem { title: "Liquidity gap ladder".into(), sheet: "Gap".into(), source: PackItemKind::Run { run: gap_run() }, col_formats: Default::default(), chart: Some(PackChart::Template { template: "gap-ladder".into() }) },
            PackItem { title: "Deposits table".into(), sheet: "Deposits".into(), source: PackItemKind::Steps { slot: "Deposits".into() }, col_formats: Default::default(), chart: Some(PackChart::Saved { name: "Deposits by product".into() }) },
            PackItem { title: "Loans table".into(), sheet: "Loans".into(), source: PackItemKind::Steps { slot: "Loans".into() }, col_formats: Default::default(), chart: None },
        ],
        output: PackOutput { html_charts: true, ..Default::default() },
    }
}

fn save_deposit_chart(rig: &Rig) {
    let spec = ChartSpec { category: "PROD".into(), value: Some("BAL".into()), kind: micro_parquet::chart::ChartKind::Bar, ..Default::default() };
    micro_parquet::charts::library::save_chart(&rig.root.join("charts"), "Deposits by product", &spec).expect("saves the chart");
}

fn sheet_names(xlsx: &Path) -> Vec<String> {
    let wb = unzip_entry(xlsx, "xl/workbook.xml");
    wb.split("<sheet ").skip(1).filter_map(|s| s.split("name=\"").nth(1).and_then(|n| n.split('"').next()).map(str::to_string)).collect()
}

fn run(rig: &Rig, pack: &ReportPack, time: &str) -> anyhow::Result<PackResult> {
    run_pack(pack, &rig.spec, &rig.paths, time, &JobCtx::inert())
}

#[test]
fn c6_a_pack_with_a_gap_ladder_item_and_its_template_writes_the_workbook_chart_and_the_chart_book() {
    let rig = Rig::new("c6_a");
    save_deposit_chart(&rig);
    let pack = pack();
    let r = run(&rig, &pack, "2026-11-19 10:00:00").unwrap_or_else(|e| panic!("{e:#}"));
    // the charts: the gap ladder's linked panels are one chart; the deposits chart another
    assert_eq!(r.sheets[0].charts.len(), 1);
    assert_eq!(r.sheets[1].charts.len(), 1);
    assert!(r.sheets[2].charts.is_empty());
    assert_eq!(r.sheets[0].charts[0].data.spec.panels.len(), 2, "the gap-ladder template: net gap columns over the cumulative line");
    let xlsx = rig.root.join("monthly_2026-10-31.xlsx");
    let html = html_path_for(&xlsx, &pack.name);
    assert_eq!(html.file_name().unwrap().to_string_lossy(), "monthly-charts.html", "<pack>-charts.html beside the workbook");
    let msg = write_pack_with_charts(&r, &xlsx, None, Some(&html), &JobCtx::inert()).expect("writes both");
    assert!(msg.contains("monthly-charts.html") && msg.contains("2 chart(s)"), "{msg}");
    // the workbook: each charted item's sheet is followed by its chart sheet
    assert_eq!(sheet_names(&xlsx), ["1_Cover", "2_Gap", "2_Gap chart", "3_Deposits", "3_Deposits chart", "4_Loans"]);
    // the native charts: the gap ladder is a column panel and a line panel, the deposits chart a column chart
    let c1 = unzip_entry(&xlsx, "xl/charts/chart1.xml");
    let c2 = unzip_entry(&xlsx, "xl/charts/chart2.xml");
    let c3 = unzip_entry(&xlsx, "xl/charts/chart3.xml");
    assert!(c1.contains("<c:barChart>") && c2.contains("<c:lineChart>"), "the two panels of the ladder");
    assert!(c3.contains("<c:barChart>"), "the deposits chart");
    assert!(c1.contains("2_Gap chart") || c1.contains("2_Gap"), "the series point at the chart's sheet");
    // the chart book: three chart containers (two panels count as one chart), a title block and the date
    let page = std::fs::read_to_string(&html).expect("the book");
    assert_eq!(page.matches("class=\"mp-chart\"").count(), 2);
    assert!(page.contains("monthly") && page.contains("As of 2026-10-31"));
    assert!(!page.contains("src=\"http") && !page.contains("href=\"http"), "offline");
    // the Cover lists the charts under their sheets
    let cover = strings(&r.cover, "Section");
    let names = strings(&r.cover, "Name");
    let charts: Vec<String> = cover.iter().zip(&names).filter(|(s, _)| s.as_deref() == Some("Chart")).map(|(_, n)| n.clone().unwrap_or_default()).collect();
    assert_eq!(charts, ["Gap chart", "Deposits chart"]);
    // without the chart book option nothing is written beside the workbook
    let _ = std::fs::remove_file(&html);
    let none = rig.root.join("none.xlsx");
    write_pack(&r, &none, None, &JobCtx::inert()).expect("workbook only");
    assert!(!html_path_for(&none, "monthly").exists());
}

#[test]
fn c6_running_it_twice_gives_the_same_charts_and_the_same_book() {
    let rig = Rig::new("c6_b");
    save_deposit_chart(&rig);
    let pack = pack();
    let write = |name: &str, time: &str| {
        let r = run(&rig, &pack, time).expect("runs");
        let x = rig.root.join(format!("{name}.xlsx"));
        let h = rig.root.join(format!("{name}.html"));
        write_pack_with_charts(&r, &x, None, Some(&h), &JobCtx::inert()).expect("writes");
        (x, h)
    };
    let (xa, ha) = write("one", "2026-11-19 09:00:00");
    let (xb, hb) = write("two", "2026-11-19 17:00:00");
    assert_eq!(std::fs::read_to_string(&ha).unwrap(), std::fs::read_to_string(&hb).unwrap(), "the book has no run time");
    for n in 1..=3 {
        let e = format!("xl/charts/chart{n}.xml");
        assert_eq!(unzip_entry(&xa, &e), unzip_entry(&xb, &e), "{e}");
    }
    for n in [3, 5] {
        let e = format!("xl/worksheets/sheet{n}.xml");
        assert_eq!(unzip_entry(&xa, &e), unzip_entry(&xb, &e), "{e}");
    }
}

#[test]
fn c6_an_item_whose_chart_cannot_be_drawn_stops_the_run_and_writes_nothing() {
    let rig = Rig::new("c6_c");
    // the saved chart is not there
    let p = pack();
    let e = format!("{:#}", run(&rig, &p, "t").expect_err("no saved chart"));
    assert!(e.contains("'Deposits table'") && e.contains("Deposits by product") && e.contains("not in"), "{e}");
    // the saved chart needs a column the item's table does not have
    let spec = ChartSpec { category: "BRANCH".into(), value: Some("BAL".into()), kind: micro_parquet::chart::ChartKind::Bar, ..Default::default() };
    micro_parquet::charts::library::save_chart(&rig.root.join("charts"), "Deposits by product", &spec).unwrap();
    let e = format!("{:#}", run(&rig, &p, "t").expect_err("missing column"));
    assert!(e.contains("BRANCH") && e.contains("does not have"), "{e}");
    // a template that does not fit the table says what it needs
    save_deposit_chart(&rig);
    let mut p2 = pack();
    p2.items[2].chart = Some(PackChart::Template { template: "irrbb-scenarios".into() });
    let e = format!("{:#}", run(&rig, &p2, "t").expect_err("template does not fit"));
    assert!(e.contains("'Loans table'") && e.contains("Scenario"), "{e}");
    // the yield curves template reads the store, not a table
    p2.items[2].chart = Some(PackChart::Template { template: "yield-curves".into() });
    assert!(format!("{:#}", run(&rig, &p2, "t").expect_err("store")).contains("curve store"));
    // a sheet name too long to carry a chart sheet
    let mut p3 = pack();
    p3.items[0].sheet = "A very long sheet name here".into();
    assert!(format!("{:#}", validate_pack(&p3).expect_err("long")).contains("at most 20 characters"));
    // a chart book of a pack without charts is refused, and no file is made
    let mut p4 = pack();
    for it in p4.items.iter_mut() {
        it.chart = None;
    }
    let r = run(&rig, &p4, "t").expect("runs");
    let (x, h) = (rig.root.join("n.xlsx"), rig.root.join("n.html"));
    let e = format!("{:#}", write_pack_with_charts(&r, &x, None, Some(&h), &JobCtx::inert()).expect_err("no charts"));
    assert!(e.contains("no chart"), "{e}");
    assert!(!x.exists() && !h.exists(), "nothing is written");
}

#[test]
fn c6_an_old_pack_without_charts_loads_and_a_chart_is_saved_with_the_pack() {
    let rig = Rig::new("c6_d");
    let old = r#"{"name":"old","inputs":[{"slot":"Deposits"}],"items":[{"title":"t","sheet":"T","source":{"kind":"steps","slot":"Deposits"}}],"output":{"pdf":false}}"#;
    let dir = rig.root.join("packs");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("old.json"), old).unwrap();
    let p = load_pack(&dir, "old").expect("an old pack loads");
    assert!(p.items[0].chart.is_none() && !p.output.html_charts);
    let saved = pack();
    save_pack(&dir, &saved).expect("saves");
    let back = load_pack(&dir, "monthly").expect("loads");
    assert_eq!(back.items[0].chart, Some(PackChart::Template { template: "gap-ladder".into() }));
    assert_eq!(back.items[1].chart, Some(PackChart::Saved { name: "Deposits by product".into() }));
    assert!(back.output.html_charts);
}
