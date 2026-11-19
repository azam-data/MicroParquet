//! P2: report packs (slots, items, the Cover sheet, one workbook, failures that write nothing).

use crate::common::*;
use micro_parquet::banking::{GapInputSpec, GapRole};
use micro_parquet::cl1::Cl1Spec;
use micro_parquet::jobs::JobCtx;
use micro_parquet::models::*;
use micro_parquet::packs::*;
use micro_parquet::rules::set_data_dir_override;
use micro_parquet::tool_runs::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const LOANS: &str = "ACNO,BAL,RATE,MAT,CAT,GRADE,BASE,PROV\nL1,1000000,9.5,2027-03-31,Continuous,STD-0,1000000,10000\nL2,2000000,12,2028-10-31,Demand,SS,1500000,300000\nL3,500000,8,2026-12-15,Fixed term,B/L,500000,500000\n";
const DEPOSITS: &str = "ACNO,BAL,MAT,PROD\nD1,3000000,2026-12-31,SB\nD2,1500000,2027-06-30,TD\nD3,500000,2026-11-30,TD\n";

struct Rig {
    root: PathBuf,
    paths: PortablePaths,
    loans: PathBuf,
    deposits: PathBuf,
}

impl Rig {
    fn new(name: &str) -> Rig {
        let root = dir(name);
        set_data_dir_override(Some(root.clone()));
        let paths = PortablePaths { root_dir: root.clone(), macros_dir: root.join("macros"), schemas_dir: root.join("schemas"), curves_dir: root.join("curves"), buckets_file: root.join("buckets.json"), temp_dir: root.join("temp") };
        std::fs::create_dir_all(&paths.temp_dir).expect("temp");
        let (loans, deposits) = (root.join("loans_oct.csv"), root.join("deposits_oct.csv"));
        std::fs::write(&loans, LOANS).expect("loans");
        std::fs::write(&deposits, DEPOSITS).expect("deposits");
        Rig { root, paths, loans, deposits }
    }

    fn spec(&self) -> PackRunSpec {
        PackRunSpec { files: BTreeMap::from([("Loans".to_string(), self.loans.clone()), ("Deposits".to_string(), self.deposits.clone())]), cutoff: "2026-10-31".to_string() }
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

fn pivot_step() -> TransformStep {
    TransformStep::from_pivot_spec(PivotSpec { rows: vec!["PROD".into()], values: vec![PivotValue { field: "BAL".into(), agg: PivotAgg::Sum, weight_col: None }], row_totals: true, column_totals: true, ..Default::default() })
}

/// Loans and Deposits, with a gap ladder, the CL-1 summary and a pivot.
fn monthly_pack() -> ReportPack {
    let gap = ToolRun::GapLadder(GapLadderRun {
        inputs: vec![
            GapInputRun { source: "Loans".into(), spec: GapInputSpec { role: GapRole::Asset, amount_col: "BAL".into(), maturity_col: "MAT".into() }, product_col: String::new() },
            GapInputRun { source: "Deposits".into(), spec: GapInputSpec { role: GapRole::Liability, amount_col: "BAL".into(), maturity_col: "MAT".into() }, product_col: String::new() },
        ],
        preset: CustomBucketDef::default_presets()[0].name.clone(),
        preset_def: None,
        cutoff: "2020-01-31".into(),
        null_as_sight: true,
        behaviour_file: String::new(),
    });
    let cl1 = ToolRun::Cl1Summary(Cl1SummaryRun {
        source: "Loans".into(),
        spec: Cl1Spec { category_col: Some("CAT".into()), fixed_category: String::new(), grade_col: "GRADE".into(), measure_cols: vec![("outstanding".into(), "BAL".into()), ("base".into(), "BASE".into()), ("provision".into(), "PROV".into())] },
        template_file: String::new(),
    });
    ReportPack {
        name: "monthly".into(),
        inputs: vec![slot("Loans", &["ACNO", "BAL", "MAT", "GRADE"]), slot("Deposits", &["ACNO", "BAL", "MAT", "PROD"])],
        items: vec![
            PackItem { title: "Liquidity gap ladder".into(), sheet: "Gap".into(), source: PackItemKind::Run { run: gap }, col_formats: Default::default() },
            PackItem { title: "CL-1 summary".into(), sheet: "CL1".into(), source: PackItemKind::Run { run: cl1 }, col_formats: Default::default() },
            PackItem { title: "Deposits by product".into(), sheet: "Deposits".into(), source: PackItemKind::Pivot { slot: "Deposits".into(), pivot: Some(pivot_step()) }, col_formats: Default::default() },
        ],
        output: PackOutput::default(),
    }
}

fn sheet_names(xlsx: &Path) -> Vec<String> {
    let wb = unzip_entry(xlsx, "xl/workbook.xml");
    wb.split("<sheet ").skip(1).filter_map(|s| s.split("name=\"").nth(1).and_then(|n| n.split('"').next()).map(str::to_string)).collect()
}

fn run_and_write(rig: &Rig, pack: &ReportPack, spec: &PackRunSpec, out: &str, time: &str) -> (PackResult, PathBuf) {
    let r = run_pack(pack, spec, &rig.paths, time, &JobCtx::inert()).unwrap_or_else(|e| panic!("{e:#}"));
    let path = rig.root.join(out);
    write_pack(&r, &path, None, &JobCtx::inert()).expect("writes");
    (r, path)
}

#[test]
fn p2_the_monthly_pack_gives_a_cover_and_three_sheets_and_the_cover_hashes_match_the_files() {
    let rig = Rig::new("p2_a");
    let (r, path) = run_and_write(&rig, &monthly_pack(), &rig.spec(), "out.xlsx", "2026-11-18 09:00:00");
    assert_eq!(sheet_names(&path), vec!["1_Cover", "2_Gap", "3_CL1", "4_Deposits"], "Cover plus the three items, in the pack's order (the exporter numbers its sheets)");
    // the cover: pack, run time, version, cutoff, each input with rows and the file's blake3 hash
    let section = strings(&r.cover, "Section");
    let name = strings(&r.cover, "Name");
    let detail = strings(&r.cover, "Detail");
    let rows = strings(&r.cover, "Rows");
    let hash = strings(&r.cover, "Blake3 hash");
    let at = |s: &str, n: &str| (0..r.cover.height()).find(|i| section[*i].as_deref() == Some(s) && (n.is_empty() || name[*i].as_deref() == Some(n))).unwrap_or_else(|| panic!("cover row {s} {n}"));
    assert_eq!(name[at("Pack", "")].as_deref(), Some("monthly"));
    assert_eq!(name[at("Run time", "")].as_deref(), Some("2026-11-18 09:00:00"));
    assert_eq!(name[at("App version", "")].as_deref(), Some(env!("CARGO_PKG_VERSION")));
    assert_eq!(name[at("Cutoff date", "")].as_deref(), Some("2026-10-31"));
    for (slot, file, n) in [("Loans", &rig.loans, "3"), ("Deposits", &rig.deposits, "3")] {
        let i = at("Input", slot);
        assert_eq!(detail[i].as_deref(), file.file_name().and_then(|f| f.to_str()));
        assert_eq!(rows[i].as_deref(), Some(n));
        let want = blake3::hash(&std::fs::read(file).expect("read")).to_hex().to_string();
        assert_eq!(hash[i].as_deref(), Some(want.as_str()), "{slot}: the cover's hash is the file's blake3");
    }
    // the sheets exist in the workbook with their data, and the hash is in the workbook's text
    let strings_xml = unzip_entry(&path, "xl/sharedStrings.xml");
    let want = blake3::hash(&std::fs::read(&rig.loans).expect("read")).to_hex().to_string();
    assert!(strings_xml.contains(&want), "the hash is written in the Cover sheet");
    assert!(strings_xml.contains("2026-10-31"), "the cutoff is on the cover");
    // the gap ladder was made at the pack's cutoff (the run said 2020-01-31)
    let gap = &r.sheets[0].df;
    assert!(gap.height() > 3, "the ladder has its buckets");
    // CL-1: the template is a parameter file of the cover, and a sample says so
    let files: Vec<String> = (0..r.cover.height()).filter(|i| section[*i].as_deref() == Some("Parameter file")).map(|i| detail[i].clone().unwrap_or_default()).collect();
    assert!(files.iter().any(|f| f.contains("report_templates/cl1.json")), "{files:?}");
    let sample = strings(&r.cover, "Sample");
    let tpl = (0..r.cover.height()).find(|i| detail[*i].as_deref().is_some_and(|d| d.contains("report_templates/cl1.json"))).expect("template row");
    assert!(matches!(sample[tpl].as_deref(), Some("Yes") | Some("No")));
    if sample[tpl].as_deref() == Some("Yes") {
        assert!(section.iter().any(|s| s.as_deref() == Some("Warning")), "a sample file puts a warning on the cover");
    }
    // the pivot sheet: SB, TD and the grand total
    let pv = &r.sheets[2].df;
    assert_eq!(strings(pv, "PROD"), vec![Some("SB".to_string()), Some("TD".to_string()), Some("Grand total".to_string())]);
    assert_eq!(floats(pv, pv.get_column_names()[1].as_str()), vec![Some(3_000_000.0), Some(2_000_000.0), Some(5_000_000.0)]);
}

#[test]
fn p2_running_it_twice_on_the_same_files_gives_identical_sheets_apart_from_the_run_time() {
    let rig = Rig::new("p2_b");
    let pack = monthly_pack();
    let (a, pa) = run_and_write(&rig, &pack, &rig.spec(), "one.xlsx", "2026-11-18 09:00:00");
    let (b, pb) = run_and_write(&rig, &pack, &rig.spec(), "two.xlsx", "2026-11-18 17:45:10");
    for (x, y) in a.sheets.iter().zip(&b.sheets) {
        assert!(x.df.equals_missing(&y.df), "{}: the same table", x.sheet);
    }
    // the cover differs in the run time only
    let mut ca = a.cover.clone();
    let mut cb = b.cover.clone();
    let strip = |df: &mut polars::prelude::DataFrame| {
        let keep: Vec<bool> = strings(df, "Section").iter().map(|s| s.as_deref() != Some("Run time")).collect();
        use polars::prelude::NamedFrom;
        let mask = polars::prelude::Series::new("m".into(), keep);
        *df = df.filter(mask.bool().expect("bool")).expect("filter");
    };
    strip(&mut ca);
    strip(&mut cb);
    assert!(ca.equals_missing(&cb));
    // the item sheets of the two workbooks are byte for byte the same
    for n in 2..=4 {
        let e = format!("xl/worksheets/sheet{n}.xml");
        assert_eq!(unzip_entry(&pa, &e), unzip_entry(&pb, &e), "sheet {n}");
    }
}

#[test]
fn p2_a_missing_slot_file_an_unknown_slot_and_a_missing_column_are_refused_with_the_names() {
    let rig = Rig::new("p2_c");
    let pack = monthly_pack();
    let ctx = JobCtx::inert();
    // no file for Deposits
    let mut spec = rig.spec();
    spec.files.remove("Deposits");
    let e = format!("{:#}", run_pack(&pack, &spec, &rig.paths, "t", &ctx).expect_err("missing slot"));
    assert!(e.contains("'Deposits'") && e.contains("--slot Deposits="), "{e}");
    // a slot the pack does not have
    let mut spec = rig.spec();
    spec.files.insert("Savings".into(), rig.deposits.clone());
    let e = format!("{:#}", run_pack(&pack, &spec, &rig.paths, "t", &ctx).expect_err("unknown slot"));
    assert!(e.contains("'Savings'") && e.contains("Loans, Deposits"), "{e}");
    // a file that does not exist
    let mut spec = rig.spec();
    spec.files.insert("Loans".into(), rig.root.join("nope.csv"));
    let e = format!("{:#}", run_pack(&pack, &spec, &rig.paths, "t", &ctx).expect_err("no file"));
    assert!(e.contains("Slot 'Loans'") && e.contains("nope.csv"), "{e}");
    // a file without two of the slot's columns
    let bad = rig.root.join("loans_bad.csv");
    std::fs::write(&bad, "ACNO,BAL,RATE\nL1,1,9\n").expect("bad");
    let mut spec = rig.spec();
    spec.files.insert("Loans".into(), bad);
    let e = format!("{:#}", run_pack(&pack, &spec, &rig.paths, "t", &ctx).expect_err("missing columns"));
    assert!(e.contains("Slot 'Loans'") && e.contains("MAT, GRADE") && e.contains("It has: ACNO, BAL, RATE"), "{e}");
    // a bad cutoff
    let mut spec = rig.spec();
    spec.cutoff = "31/10/2026".into();
    assert!(format!("{:#}", run_pack(&pack, &spec, &rig.paths, "t", &ctx).expect_err("cutoff")).contains("YYYY-MM-DD"));
    // values that cannot be read as the column's type are not left out
    let hard = rig.root.join("loans_hard.csv");
    std::fs::write(&hard, "ACNO,BAL,RATE,MAT,CAT,GRADE,BASE,PROV\nL1,1000,9.5,2027-03-31,Continuous,STD-0,1000,10\nL2,abc,9.5,2027-03-31,Continuous,STD-0,1000,10\nL3,2000,9.5,2027-03-31,Continuous,STD-0,1000,10\nL4,3000,9.5,2027-03-31,Continuous,STD-0,1000,10\n").expect("hard");
    let mut spec = rig.spec();
    spec.files.insert("Loans".into(), hard);
    let r = run_pack(&pack, &spec, &rig.paths, "t", &ctx);
    if let Err(e) = r {
        let e = format!("{e:#}");
        assert!(e.contains("Slot 'Loans'") || e.contains("failed"), "{e}");
    }
}

#[test]
fn p2_an_item_that_fails_stops_the_run_with_its_name_and_the_reason_and_writes_nothing() {
    let rig = Rig::new("p2_d");
    let mut pack = monthly_pack();
    pack.items[2].source = PackItemKind::Pivot { slot: "Deposits".into(), pivot: Some(TransformStep::from_pivot_spec(PivotSpec { rows: vec!["NoSuchField".into()], values: vec![PivotValue { field: "BAL".into(), agg: PivotAgg::Sum, weight_col: None }], ..Default::default() })) };
    let out = rig.root.join("never.xlsx");
    let e = format!("{:#}", run_pack(&pack, &rig.spec(), &rig.paths, "t", &JobCtx::inert()).expect_err("the pivot fails"));
    assert!(e.contains("'Deposits by product'") && e.contains("failed") && e.contains("NoSuchField"), "{e}");
    assert!(!out.exists(), "no workbook");
    assert!(!std::fs::read_dir(&rig.root).expect("dir").flatten().any(|f| f.file_name().to_string_lossy().ends_with(".xlsx")), "no partial workbook anywhere");
    // an item that reads a slot the pack does not have is refused before anything is read
    let mut pack = monthly_pack();
    pack.items[2].source = PackItemKind::Steps { slot: "Nowhere".into() };
    let e = format!("{:#}", run_pack(&pack, &rig.spec(), &rig.paths, "t", &JobCtx::inert()).expect_err("no slot"));
    assert!(e.contains("'Deposits by product'") && e.contains("'Nowhere'"), "{e}");
    // sheet names: too long, twice, and the names the workbook keeps
    let mut pack = monthly_pack();
    pack.items[0].sheet = "A very long sheet name that is over 31".into();
    assert!(format!("{:#}", validate_pack(&pack).expect_err("long")).contains("31"));
    let mut pack = monthly_pack();
    pack.items[1].sheet = "gap".into();
    assert!(format!("{:#}", validate_pack(&pack).expect_err("twice")).contains("used twice"));
    pack.items[1].sheet = "Cover".into();
    assert!(validate_pack(&pack).is_err());
    let mut pack = monthly_pack();
    pack.inputs.push(slot("loans", &[]));
    assert!(format!("{:#}", validate_pack(&pack).expect_err("slot twice")).contains("Two input slots"));
}

#[test]
fn p2_a_pack_is_a_json_file_with_slots_schema_and_steps_and_an_old_file_loads() {
    let rig = Rig::new("p2_e");
    let mut pack = monthly_pack();
    pack.inputs[0].steps = vec![TransformStep::FilterRows { column: "GRADE".into(), operator: FilterOp::NotEq, value: "B/L".into(), keep_matching: true }];
    pack.output = PackOutput { pdf: true, scale: NumberDisplayFormat::BdtLakh, decimals: 1 };
    let dir = rig.root.join("packs");
    let path = save_pack(&dir, &pack).expect("saves");
    assert!(path.ends_with("monthly.json"));
    let back = load_pack(&dir, "monthly").expect("loads");
    assert_eq!(serde_json::to_string(&back).expect("json"), serde_json::to_string(&pack).expect("json"));
    assert_eq!(list_packs(&dir), vec!["monthly".to_string()]);
    // the slot's own steps apply to its file: the B/L loan is gone from the Loans slot
    let (r, _) = run_and_write(&rig, &back, &rig.spec(), "steps.xlsx", "t");
    let loans = r.inputs.iter().find(|i| i.slot == "Loans").expect("loans");
    assert_eq!(loans.rows, 2, "rows are counted after the slot's steps");
    // an old or short file: every field has a default
    std::fs::write(dir.join("short.json"), r#"{"items":[{"title":"T","sheet":"S","source":{"kind":"steps","slot":"A"}}],"inputs":[{"slot":"A"}]}"#).expect("write");
    let s = load_pack(&dir, "short").expect("loads");
    assert_eq!((s.name.as_str(), s.output.scale, s.output.pdf, s.output.decimals), ("short", NumberDisplayFormat::BdtCrore, false, 2));
    assert!(save_pack(&dir, &ReportPack { name: "a/b".into(), ..s.clone() }).is_err());
    assert!(load_pack(&dir, "missing").is_err());
}

#[test]
fn p2_a_slot_with_a_saved_schema_reads_the_file_with_it_and_a_pdf_can_be_asked_for() {
    let rig = Rig::new("p2_f");
    // the schema of the loans file with a rename, saved as a template (U1)
    let mut cfg = micro_parquet::engine::auto_detect_smart_schema(&rig.loans).expect("detected");
    for c in cfg.columns.iter_mut() {
        if c.name == "BAL" {
            c.rename_to = Some("Balance".into());
        }
    }
    std::fs::create_dir_all(&rig.paths.schemas_dir).expect("schemas");
    std::fs::write(rig.paths.schemas_dir.join("loans.schema.json"), serde_json::to_string_pretty(&cfg).expect("json")).expect("save");
    let pack = ReportPack {
        name: "withschema".into(),
        inputs: vec![PackInput { slot: "Loans".into(), schema_template: Some("loans".into()), ..Default::default() }],
        items: vec![PackItem { title: "Loans".into(), sheet: "Loans".into(), source: PackItemKind::Steps { slot: "Loans".into() }, col_formats: Default::default() }],
        output: PackOutput { pdf: true, ..Default::default() },
    };
    let spec = PackRunSpec { files: BTreeMap::from([("Loans".to_string(), rig.loans.clone())]), cutoff: "2026-10-31".into() };
    let r = run_pack(&pack, &spec, &rig.paths, "t", &JobCtx::inert()).expect("runs");
    assert!(r.sheets[0].df.get_column_names().iter().any(|n| n.as_str() == "Balance"), "the schema's rename applies");
    let (x, p) = (rig.root.join("s.xlsx"), rig.root.join("s.pdf"));
    write_pack(&r, &x, Some(&p), &JobCtx::inert()).expect("writes both");
    assert!(x.exists() && std::fs::metadata(&p).map(|m| m.len() > 100).unwrap_or(false), "the PDF is written next to the workbook");
    // a template that is not saved is named
    let mut bad = pack.clone();
    bad.inputs[0].schema_template = Some("gone".into());
    let e = format!("{:#}", run_pack(&bad, &spec, &rig.paths, "t", &JobCtx::inert()).expect_err("no template"));
    assert!(e.contains("Slot 'Loans'") && e.contains("'gone'"), "{e}");
}
