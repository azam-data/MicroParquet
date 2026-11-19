//! P1: saved tool runs. For each tool: run → save → reload → re-run on the same inputs gives an
//! identical result table; the saved file names its source tabs (never an index).

use crate::common::*;
use micro_parquet::banking::*;
use micro_parquet::behaviour::BehaviourFile;
use micro_parquet::cashflow::*;
use micro_parquet::cl1::Cl1Spec;
use micro_parquet::ecl::EclSummarySpec;
use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::liquidity::*;
use micro_parquet::models::*;
use micro_parquet::rules::set_data_dir_override;
use micro_parquet::tool_runs::*;
use polars::prelude::*;
use std::path::{Path, PathBuf};

/// The tabs of a test session by name, with the folders of its `portable_data`.
struct Env {
    tabs: Vec<(String, RunTab)>,
    paths: PortablePaths,
    root: PathBuf,
}

impl Env {
    fn new(name: &str) -> Env {
        let root = dir(name);
        let paths = PortablePaths {
            root_dir: root.clone(),
            macros_dir: root.join("macros"),
            schemas_dir: root.join("schemas"),
            curves_dir: root.join("curves"),
            buckets_file: root.join("buckets.json"),
            temp_dir: root.join("temp"),
        };
        set_data_dir_override(Some(root.clone()));
        Env { tabs: Vec::new(), paths, root }
    }

    /// A tab made of a Parquet file called `<name>.parquet`.
    fn open(&mut self, name: &str, df: &DataFrame) -> TabSource {
        let src = parquet(&self.root, name, df);
        self.tabs.push((name.to_string(), RunTab { source: src.clone(), steps: Vec::new() }));
        src
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        set_data_dir_override(None);
        micro_parquet::curves::set_curves_dir_override(None);
    }
}

impl RunEnv for Env {
    fn tab(&self, name: &str) -> anyhow::Result<RunTab> {
        self.tabs.iter().find(|(n, _)| n == name).map(|(_, t)| t.clone()).ok_or_else(|| anyhow::anyhow!("The tab '{name}' is not open."))
    }
    fn name_of(&self, source: &TabSource, steps: &[TransformStep]) -> anyhow::Result<String> {
        let key = |s: &TabSource, st: &[TransformStep]| format!("{:?}|{:?}", s, st);
        self.tabs.iter().find(|(_, t)| key(&t.source, &t.steps) == key(source, steps)).map(|(n, _)| n.clone()).ok_or_else(|| anyhow::anyhow!("no open tab holds this source"))
    }
    fn paths(&self) -> &PortablePaths {
        &self.paths
    }
}

fn json(r: &ToolRun) -> String {
    serde_json::to_string_pretty(r).expect("json")
}

fn table(src: &TabSource, steps: &[TransformStep], d: &Path) -> DataFrame {
    compile_tab_lazy(src, steps, d).and_then(|lf| Ok(lf.collect()?)).expect("the tab")
}

/// run → save → reload → re-run: the saved file is the same run, and the re-run table equals the first.
fn roundtrip(env: &Env, source: &TabSource, tab_steps: &[TransformStep], what: &str) -> (ToolRun, DataFrame) {
    let first = table(source, tab_steps, &env.root);
    let run = ToolRun::from_source(source, tab_steps, env).unwrap_or_else(|e| panic!("{what}: {e:#}"));
    let runs = env.root.join("runs");
    let path = save_run(&runs, &format!("{what} run"), &run).expect("saves");
    assert!(path.starts_with(&runs) && path.extension().and_then(|e| e.to_str()) == Some("json"), "{what}: {path:?}");
    assert_eq!(list_runs(&runs).len(), list_runs(&runs).iter().collect::<std::collections::BTreeSet<_>>().len());
    let loaded = load_run(&runs, &format!("{what} run")).expect("loads");
    assert_eq!(json(&loaded.run), json(&run), "{what}: the file holds the same run");
    assert_eq!(loaded.name, format!("{what} run"));
    let built = loaded.run.build(env).unwrap_or_else(|e| panic!("{what}: {e:#}"));
    assert!(!built.is_empty(), "{what}");
    let (title, src2, steps2) = &built[0];
    assert!(!title.is_empty(), "{what}");
    let again = table(src2, steps2, &env.root);
    assert!(first.equals_missing(&again), "{what}: the re-run table is not identical\n{first}\n{again}");
    (run, first)
}

fn ccy_book() -> DataFrame {
    DataFrame::new(vec![
        Series::new("ACNO".into(), &["A1", "A2", "A3", "A4"]),
        Series::new("BAL".into(), &[1_000_000.0, 2_000_000.0, 500_000.0, 800_000.0]),
        Series::new("RATE".into(), &[9.0, 12.0, 6.0, 7.5]),
        dates("MAT", &["2027-03-31", "2028-10-31", "2026-12-15", "2031-04-30"]),
        Series::new("SIDE".into(), &["A", "A", "L", "L"]),
        Series::new("CCY".into(), &["BDT", "BDT", "BDT", "BDT"]),
    ])
    .expect("book")
}

fn side_cf() -> CfInputSpec {
    CfInputSpec {
        role: GapRole::Side("SIDE".into()),
        principal_col: "BAL".into(),
        rate_col: Some("RATE".into()),
        maturity_col: "MAT".into(),
        currency_col: Some("CCY".into()),
        default_currency: "BDT".into(),
        ..Default::default()
    }
}

#[test]
fn p1_the_repricing_gap_run_names_its_tab_and_makes_the_same_table() {
    let mut env = Env::new("p1_rg");
    let src = env.open("Loans", &ccy_book());
    let spec = RateShockSpec { bal_col: "BAL".into(), rate_col: "RATE".into(), mat_col: "MAT".into(), cutoff_date: "2026-10-31".into(), shock_bps: 100.0, side_col: Some("SIDE".into()), ..Default::default() };
    let tab = TabSource::RepricingGap { source: Box::new(src), steps: vec![], spec };
    let (run, _) = roundtrip(&env, &tab, &[], "repricing gap");
    assert_eq!(run.source_names(), vec!["Loans".to_string()]);
    let text = json(&run);
    assert!(text.contains("\"tool\": \"repricing_gap\"") && text.contains("\"source\": \"Loans\""), "{text}");
    assert!(!text.contains("tab_index") && !text.contains("\"tab\":"), "no tab index in the file: {text}");
}

#[test]
fn p1_the_gap_ladder_run_keeps_roles_preset_cutoff_and_makes_the_same_ladder() {
    let mut env = Env::new("p1_gap");
    let a = DataFrame::new(vec![Series::new("Balance".into(), &[100.0, 200.0]), dates("Matdat", &["2026-11-15", "2026-12-31"])]).expect("a");
    let l = DataFrame::new(vec![Series::new("Balance".into(), &[150.0, 50.0]), dates("Matdat", &["2026-11-20", "2026-12-15"])]).expect("l");
    let sa = env.open("Assets", &a);
    let sl = env.open("Liabilities", &l);
    let spec = |role| GapInputSpec { role, amount_col: "Balance".into(), maturity_col: "Matdat".into() };
    let tab = TabSource::GapLadder {
        inputs: vec![GapTabInput { source: sa, steps: vec![], spec: spec(GapRole::Asset) }, GapTabInput { source: sl, steps: vec![], spec: spec(GapRole::Liability) }],
        preset: CustomBucketDef::default_presets()[0].clone(),
        cutoff_date: "2026-10-31".into(),
        null_as_sight: false,
        limits: None,
        behaviour: None,
    };
    let (run, df) = roundtrip(&env, &tab, &[], "gap ladder");
    assert_eq!(run.source_names(), vec!["Assets".to_string(), "Liabilities".to_string()]);
    assert!(df.height() > 3);
    let ToolRun::GapLadder(g) = &run else { panic!("a gap ladder run") };
    assert_eq!((g.cutoff.as_str(), g.null_as_sight, g.behaviour_file.as_str()), ("2026-10-31", false, ""));
    assert_eq!(g.preset, CustomBucketDef::default_presets()[0].name);
    // a stress option or a behaviour file can be added to the saved run, and the file is read when it is made
    let mut with_file = run.clone();
    if let ToolRun::GapLadder(g) = &mut with_file {
        g.behaviour_file = "nowhere.json".into();
        g.inputs.iter_mut().for_each(|i| i.product_col = "Product".into());
    }
    let e = format!("{:#}", with_file.build(&env).expect_err("the profile is not there"));
    assert!(e.contains("nowhere"), "{e}");
}

#[test]
fn p1_the_irrbb_ladder_run_makes_the_same_ladder() {
    let mut env = Env::new("p1_cf");
    let src = env.open("Book", &ccy_book());
    let tab = TabSource::IrrbbLadder { inputs: vec![CfTabInput { source: src, steps: vec![], spec: side_cf() }], cutoff_date: "2026-10-31".into(), mode: CashFlowMode::PrincipalOnly, scenario: Scenario::Base, behaviour: None, blank_maturity_on: true };
    let (run, df) = roundtrip(&env, &tab, &[], "irrbb ladder");
    assert!(df.height() > 0);
    let ToolRun::IrrbbLadder(r) = &run else { panic!("an IRRBB ladder run") };
    assert_eq!((r.cutoff.as_str(), r.mode, r.blank_maturity_on), ("2026-10-31", CashFlowMode::PrincipalOnly, true));
    assert_eq!(r.inputs[0].spec.rate_col.as_deref(), Some("RATE"));
}

fn irrbb_env(name: &str) -> (Env, TabSource) {
    let mut env = Env::new(name);
    let src = env.open("Book", &ccy_book());
    let curves = env.root.join("curves");
    micro_parquet::curves::set_curves_dir_override(Some(curves.clone()));
    let curve = micro_parquet::curves::YieldCurve {
        date: chrono::NaiveDate::from_ymd_opt(2026, 10, 1).expect("date"),
        currency: "BDT".into(),
        source: "test".into(),
        points: vec![FtpPoint { tenor_days: 30, ftp_rate: 10.0 }, FtpPoint { tenor_days: 9000, ftp_rate: 10.0 }],
    };
    micro_parquet::curves::save_curve(&curves, &curve).expect("saves");
    (env, src)
}

fn irrbb_source(src: TabSource, view: IrrbbView, floors: bool) -> TabSource {
    use micro_parquet::irrbb::*;
    let cutoff = chrono::NaiveDate::from_ymd_opt(2026, 10, 31).expect("date");
    let shocks = micro_parquet::rules::load_shocks(cutoff).expect("the shipped shocks");
    let store = micro_parquet::curves::CurveStore::load(&micro_parquet::curves::curves_dir());
    let spec = IrrbbSpec {
        cutoff,
        mode: CashFlowMode::PrincipalOnly,
        blank_maturity_on: false,
        tier1: 5_000_000.0,
        compounding: Compounding::Continuous,
        post_shock_floor_pct: if floors { Some(0.0) } else { None },
        nii_floor_pct: if floors { Some(0.5) } else { None },
        shocks,
        curves: store.strict().expect("curves").to_vec(),
        behaviour: None,
        betas: vec![],
        beta_file: String::new(),
    };
    TabSource::Irrbb { inputs: vec![CfTabInput { source: src, steps: vec![], spec: side_cf() }], spec: Box::new(spec), view }
}

#[test]
fn p1_the_irrbb_six_scenario_runs_make_the_same_tables_and_the_footer_is_rebuilt_from_the_saved_run_alone() {
    let (env, src) = irrbb_env("p1_six");
    let results = irrbb_source(src.clone(), IrrbbView::Results, true);
    let (run, df) = roundtrip(&env, &results, &[], "irrbb results");
    roundtrip(&env, &irrbb_source(src, IrrbbView::CashFlows, false), &[], "irrbb cash flows");
    // the footer of the results table (Phase 5 D10) reads cutoff, mode, options, floors, Tier 1 and the files
    let footer: Vec<String> = df.get_columns().iter().flat_map(|c| (0..df.height()).filter_map(|i| c.get(i).ok().map(|v| v.to_string()))).collect();
    let all = footer.join("\n");
    for needle in ["2026-10-31", "Principal only", "Post-shock floor", "Tier 1", "Beta file"] {
        assert!(all.contains(needle), "the footer has '{needle}':\n{all}");
    }
    let ToolRun::IrrbbSix(r) = &run else { panic!("a six-scenario run") };
    assert_eq!((r.cutoff.as_str(), r.tier1, r.post_shock_floor_pct, r.nii_floor_pct, r.compounding), ("2026-10-31", 5_000_000.0, Some(0.0), Some(0.5), micro_parquet::irrbb::Compounding::Continuous));
    // a run saved without Tier 1 is refused with the reason
    let mut bad = run.clone();
    if let ToolRun::IrrbbSix(b) = &mut bad {
        b.tier1 = 0.0;
    }
    assert!(format!("{:#}", bad.build(&env).expect_err("no Tier 1")).contains("Tier 1"));
}

#[test]
fn p1_the_cl_migration_and_period_reconcile_runs_name_two_tabs() {
    let mut env = Env::new("p1_mig");
    let prev = DataFrame::new(vec![Series::new("Actnum".into(), &["A1", "A2", "A3"]), Series::new("Balance".into(), &[100.0, 50.0, 25.0]), Series::new("CL_Status".into(), &["STD", "SMA", "SS"]), Series::new("Rate".into(), &[9.0, 8.0, 7.0])]).expect("prev");
    let curr = DataFrame::new(vec![Series::new("Actnum".into(), &["A1", "A2", "A4"]), Series::new("Balance".into(), &[90.0, 60.0, 10.0]), Series::new("CL_Status".into(), &["SMA", "SMA", "STD"]), Series::new("Rate".into(), &[9.0, 8.5, 6.0])]).expect("curr");
    let p = env.open("Sept", &prev);
    let c = env.open("Oct", &curr);
    let mig = TabSource::ClMigration { prev_source: Box::new(p.clone()), prev_steps: vec![], curr_source: Box::new(c.clone()), curr_steps: vec![], spec: ClMigrationSpec { key_col: "Actnum".into(), bal_col: "Balance".into(), cl_col: "CL_Status".into(), pct_of_row: true, ..Default::default() } };
    let (run, _) = roundtrip(&env, &mig, &[], "cl migration");
    assert_eq!(run.source_names(), vec!["Sept".to_string(), "Oct".to_string()]);
    let rec = TabSource::PeriodReconcile { prev_source: Box::new(p), prev_steps: vec![], curr_source: Box::new(c), curr_steps: vec![], key_col: "Actnum".into(), bal_col: "Balance".into(), rate_col: Some("Rate".into()), cl_status_col: Some("CL_Status".into()) };
    let (run, _) = roundtrip(&env, &rec, &[], "period reconcile");
    assert_eq!(run.source_names(), vec!["Sept".to_string(), "Oct".to_string()]);
    // the same tab for both months is refused for a migration
    let mut same = ToolRun::from_source(&mig, &[], &env).expect("run");
    if let ToolRun::ClMigration(m) = &mut same {
        m.curr = m.prev.clone();
    }
    assert!(format!("{:#}", same.build(&env).expect_err("same tab")).contains("two different tabs"));
}

#[test]
fn p1_the_cl1_and_liquidity_runs_read_their_template_and_map_files_again() {
    let mut env = Env::new("p1_cl1");
    let loans = DataFrame::new(vec![
        Series::new("Account_No".into(), &["A1", "A2", "A3"]),
        Series::new("Loan_Category".into(), &["Continuous", "Demand", "Fixed term"]),
        Series::new("CL_Final".into(), &["STD-0", "SS", "B/L"]),
        Series::new("Balance".into(), &[1000.0, 300.0, 100.0]),
        Series::new("Base_For_Provision_BDT".into(), &[1000.0, 250.0, 100.0]),
        Series::new("Required_Provision_BDT".into(), &[10.0, 50.0, 100.0]),
    ])
    .expect("loans");
    let src = env.open("Loans", &loans);
    let spec = Cl1Spec {
        category_col: Some("Loan_Category".into()),
        fixed_category: String::new(),
        grade_col: "CL_Final".into(),
        measure_cols: vec![("outstanding".into(), "Balance".into()), ("base".into(), "Base_For_Provision_BDT".into()), ("provision".into(), "Required_Provision_BDT".into())],
    };
    let template = micro_parquet::cl1::load_template().expect("the shipped template");
    let tab = TabSource::Cl1Summary { source: Box::new(src), steps: vec![], spec, template: Box::new(template) };
    let (run, df) = roundtrip(&env, &tab, &[], "cl1");
    assert_eq!(df.height(), 5, "four categories and the total");
    assert_eq!(run.tool_name(), "CL-1 summary");

    // LCR / NSFR from GL balances, with the sample map
    let gl = DataFrame::new(vec![Series::new("GL_Code".into(), &["1010", "1020", "2010", "300000"]), Series::new("Balance".into(), &[1000.0, 400.0, 2000.0, 900.0])]).expect("gl");
    let g = env.open("GL", &gl);
    let map = load_map("sample_basel.json").expect("the sample map");
    let lspec = LiqSpec { gl_col: "GL_Code".into(), balance_col: "Balance".into(), sign: BalanceSign::NormalSide, maturity_col: None, maturity_unit: MaturityUnit::Months };
    for view in [LiquidityView::Lines, LiquidityView::Summary] {
        let tab = TabSource::Liquidity { source: Box::new(g.clone()), steps: vec![], spec: lspec.clone(), map: Box::new(map.clone()), map_file: "sample_basel.json".into(), view };
        let (run, _) = roundtrip(&env, &tab, &[], &format!("liquidity {view:?}"));
        let ToolRun::Liquidity(l) = &run else { panic!("a liquidity run") };
        assert_eq!((l.map_file.as_str(), l.view), ("sample_basel.json", view));
    }
}

#[test]
fn p1_the_ifrs9_summary_run_keeps_the_ecl_step_and_adds_it_to_the_tab_again() {
    let mut env = Env::new("p1_ecl");
    let folder = env.root.join("ecl_params");
    std::fs::create_dir_all(&folder).expect("folder");
    std::fs::write(
        folder.join("2026-03-08.json"),
        serde_json::json!({
            "as_of": "2026-03-08", "source": "test parameters (not a circular)", "sample": true,
            "segments": [{"name": "Corporate", "cumulative_pd_pct": [2.0, 5.0], "lgd_pct": 45, "ccf_pct": 50}],
            "scenarios": [{"name": "Base", "weight_pct": 100, "pd_multiplier": 1.0}]
        })
        .to_string(),
    )
    .expect("write");
    let book = DataFrame::new(vec![
        Series::new("Account_No".into(), &["A1", "A2"]),
        Series::new("Drawn".into(), &[1_000_000.0, 500_000.0]),
        Series::new("EIR".into(), &[10.0, 11.0]),
        dates("Matdat", &["2028-10-31", "2027-10-31"]),
        Series::new("CL_Final".into(), &["STD-0", "SMA"]),
        Series::new("Required_Provision_BDT".into(), &[10_000.0, 30_000.0]),
    ])
    .expect("book");
    let src = env.open("Loans", &book);
    let step: TransformStep = serde_json::from_value(serde_json::json!({"IfrsEcl": {"fixed_segment": "Corporate", "drawn_col": "Drawn", "eir_col": "EIR", "maturity_col": "Matdat", "classified_stage3": true, "cutoff_date": "2026-10-31"}})).expect("step");
    let tab = TabSource::EclSummary { source: Box::new(src), steps: vec![step], spec: EclSummarySpec { grade_col: "CL_Final".into(), provision_col: "Required_Provision_BDT".into() } };
    let (run, df) = roundtrip(&env, &tab, &[], "ifrs9");
    assert!(df.height() > 3);
    let ToolRun::EclSummary(r) = &run else { panic!("an ECL run") };
    assert!(matches!(r.ecl_step, Some(TransformStep::IfrsEcl { .. })), "the ECL step is part of the run");
    assert_eq!(r.source, "Loans", "the source is the tab without the ECL step");
}

#[test]
fn p1_the_join_stack_and_pivot_tab_runs() {
    let mut env = Env::new("p1_join");
    let left = DataFrame::new(vec![Series::new("ACNO".into(), &["A1", "A2", "A3"]), Series::new("BAL".into(), &[1.0, 2.0, 3.0])]).expect("left");
    let right = DataFrame::new(vec![Series::new("ACNO".into(), &["A1", "A3", "A3"]), Series::new("Grade".into(), &["STD", "SS", "SS"])]).expect("right");
    let l = env.open("Left", &left);
    let r = env.open("Right", &right);
    let join = TabSource::JoinedTabs { left_source: Box::new(l), left_steps: vec![], right_source: Box::new(r), right_steps: vec![], join_kind: SqlJoinKind::Left, join_keys: vec!["ACNO".into()], right_cols_to_keep: vec![], keep_first_duplicate_right: true };
    let (run, df) = roundtrip(&env, &join, &[], "join");
    assert_eq!(df.height(), 3);
    assert_eq!(run.source_names(), vec!["Left".to_string(), "Right".to_string()]);

    // stack: the files and their schemas are in the run
    let a = parquet(&env.root, "jan", &left);
    let b = parquet(&env.root, "feb", &left);
    let (TabSource::SingleFile { path: pa, .. }, TabSource::SingleFile { path: pb, .. }) = (a, b) else { panic!("files") };
    let stack = TabSource::VerticalStack { files: vec![(pa.clone(), true, IngestionConfig::default()), (pb, true, IngestionConfig::default())], include_source_col: true };
    let (run, df) = roundtrip(&env, &stack, &[], "stack");
    assert_eq!(df.height(), 6);
    assert!(run.source_names().is_empty(), "a stack names files, not tabs");
    // a file that is gone is named
    std::fs::remove_file(&pa).expect("remove");
    let e = format!("{:#}", run.build(&env).expect_err("missing file"));
    assert!(e.contains("jan.parquet") && e.contains("not there"), "{e}");

    // a pivot made into a new tab: the tab it came from and the pivot step
    let book = ccy_book();
    let src = env.open("Book", &book);
    let spec = PivotSpec { rows: vec!["SIDE".into()], values: vec![PivotValue { field: "BAL".into(), agg: PivotAgg::Sum, weight_col: None }], row_totals: true, column_totals: true, ..Default::default() };
    let step = TransformStep::from_pivot_spec(spec);
    let (run, df) = roundtrip(&env, &src, std::slice::from_ref(&step), "pivot tab");
    assert_eq!(df.height(), 3, "A, L and the grand total");
    assert_eq!(run.source_names(), vec!["Book".to_string()]);
    let built = run.build(&env).expect("builds");
    assert_eq!(built[0].0, "Pivot: SIDE");
    assert_eq!(built[0].2.len(), 1, "the pivot step goes in the tab's own steps");
}

#[test]
fn p1_a_tab_that_no_tool_made_and_extra_steps_are_refused_and_a_missing_tab_is_named() {
    let mut env = Env::new("p1_err");
    let src = env.open("Book", &ccy_book());
    let e = format!("{:#}", ToolRun::from_source(&src, &[], &env).expect_err("a plain file"));
    assert!(e.contains("not made by a tool"), "{e}");
    let tab = TabSource::RepricingGap { source: Box::new(src.clone()), steps: vec![], spec: RateShockSpec { bal_col: "BAL".into(), rate_col: "RATE".into(), mat_col: "MAT".into(), cutoff_date: "2026-10-31".into(), shock_bps: 50.0, ..Default::default() } };
    let extra = [TransformStep::FilterRows { column: "SIDE".into(), operator: FilterOp::Eq, value: "A".into(), keep_matching: true }];
    let e = format!("{:#}", ToolRun::from_source(&tab, &extra, &env).expect_err("steps of its own"));
    assert!(e.contains("steps of its own"), "{e}");
    // the run of a tab that is closed names it
    let run = ToolRun::from_source(&tab, &[], &env).expect("run");
    env.tabs.clear();
    let e = format!("{:#}", run.build(&env).expect_err("closed"));
    assert!(e.contains("'Book'") && e.contains("not open"), "{e}");
    // names that are not file names
    assert!(save_run(&env.root.join("runs"), "a/b", &run).is_err());
    assert!(save_run(&env.root.join("runs"), "  ", &run).is_err());
}

#[test]
fn p1_a_run_file_with_missing_fields_loads_with_defaults_and_the_runs_are_listed() {
    let d = dir("p1_old");
    std::fs::write(d.join("old.json"), r#"{"name":"old","run":{"tool":"gap_ladder","preset":"X","cutoff":"2026-10-31"}}"#).expect("write");
    let r = load_run(&d, "old").expect("loads");
    let ToolRun::GapLadder(g) = &r.run else { panic!("a gap ladder") };
    assert!(g.inputs.is_empty() && g.null_as_sight, "missing fields have their defaults ({g:?})");
    assert_eq!(r.app_version, "");
    std::fs::write(d.join("bad.json"), "{not json").expect("write");
    let e = format!("{:#}", load_run(&d, "bad").expect_err("bad"));
    assert!(e.contains("is not valid"), "{e}");
    assert!(load_run(&d, "nothing").is_err());
    assert_eq!(list_runs(&d), vec!["bad".to_string(), "old".to_string()]);
    let _ = BehaviourFile::default();
}

#[test]
fn h4_a_saved_run_keeps_the_hash_of_each_file_it_read_and_warns_naming_a_file_that_changed() {
    use micro_parquet::rules::UsedFile;
    let d = dir("h4_files");
    set_data_dir_override(Some(d.clone()));
    std::fs::create_dir_all(d.join("irrbb_shocks")).expect("dir");
    std::fs::write(d.join("irrbb_shocks/x.json"), "{\"a\":1}").expect("file");
    std::fs::write(d.join("y.json"), "{\"b\":1}").expect("file");
    let used = vec![
        UsedFile { kind: "IRRBB curve".into(), file: "irrbb_shocks/x.json".into(), as_of: String::new(), source: String::new(), sample: false },
        UsedFile { kind: "Rule file".into(), file: "y.json".into(), as_of: String::new(), source: String::new(), sample: false },
    ];
    let files = hash_used_files(&used);
    assert_eq!(files.len(), 2);
    assert!(changed_files_warning(&files).is_none(), "nothing changed");
    // the hashes travel in the saved run's file
    let run = ToolRun::PivotTab(PivotTabRun { source: "Sept".into(), pivot: None });
    save_run_with(&d.join("runs"), "h4", &run, vec![], files.clone()).expect("saves");
    let back = load_run(&d.join("runs"), "h4").expect("loads");
    assert_eq!(back.files, files);
    std::fs::write(d.join("irrbb_shocks/x.json"), "{\"a\":2}").expect("edit");
    let w = changed_files_warning(&back.files).expect("a warning");
    assert!(w.contains("irrbb_shocks/x.json") && !w.contains("y.json"), "{w}");
    // a file that is gone counts as changed
    std::fs::remove_file(d.join("y.json")).expect("remove");
    assert!(changed_files_warning(&back.files).expect("warning").contains("y.json"));
    set_data_dir_override(None);
}
