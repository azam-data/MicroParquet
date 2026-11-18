//! Phase 3 L5: file-name sanitiser, data-folder chooser, one version number; and the compatibility
//! test that loads a macro, schema, preset and rule file from each earlier phase.

use micro_parquet::macros::*;
use micro_parquet::models::*;
use micro_parquet::paths::*;
use std::path::{Path, PathBuf};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mp_l5_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn l5_windows_forbidden_characters_are_replaced() {
    assert_eq!(sanitize_file_name(r#"Join(A/B):C*D?"E"<F>|G\H"#), "Join(A_B)_C_D__E__F__G_H");
    assert_eq!(sanitize_file_name("Loans_Oct"), "Loans_Oct");
    assert_eq!(sanitize_file_name("Q3 2026 – Crore"), "Q3 2026 – Crore", "other characters stay");
    assert_eq!(sanitize_file_name("name. "), "name", "Windows drops trailing dots and spaces");
    assert_eq!(sanitize_file_name("a\tb\nc"), "a_b_c", "control characters");
    assert_eq!(sanitize_file_name("///"), "___");
    assert_eq!(sanitize_file_name(""), "export");
    assert_eq!(sanitize_file_name("  . "), "export");
    assert_eq!(default_export_file_name("View of Join(A/B)", "parquet"), "View of Join(A_B).parquet");
    assert_eq!(default_export_file_name("FDR_Sep", ".csv"), "FDR_Sep.csv");
    assert_eq!(default_export_file_name("Mac ro", "mpq.json"), "Mac ro.mpq.json");
}

#[test]
fn l5_long_names_are_cut_to_120_characters_and_keep_the_extension() {
    let long = "x".repeat(300);
    let n = default_export_file_name(&long, "parquet");
    assert_eq!(n.chars().count(), 120);
    assert!(n.ends_with(".parquet"));
    let n = default_export_file_name(&format!("{}.", "y".repeat(112)), "csv"); // cut leaves a dot
    assert!(n.chars().count() <= 120 && n.ends_with(".csv") && !n.contains("..") , "{n}");
    // multi-byte characters count as one each
    let bn = default_export_file_name(&"৳".repeat(200), "csv");
    assert_eq!(bn.chars().count(), 120);
    assert!(sanitize_file_name(&long).chars().count() <= 300);
}

fn always(_: &Path) -> bool {
    true
}
fn never(_: &Path) -> bool {
    false
}

#[test]
fn l5_macos_data_folder_is_next_to_the_app_bundle() {
    let exe = Path::new("/Applications/MicroParquet.app/Contents/MacOS/MicroParquet");
    let home = Path::new("/Users/azam");
    let p = choose_portable_root(exe, true, Some(home), &always);
    assert_eq!(p, PathBuf::from("/Applications/portable_data"), "beside the .app, not inside it");
    assert!(!p.to_string_lossy().contains(".app/"));
    // The folder next to the bundle is not writable (a locked /Applications): the user's Library
    let p = choose_portable_root(exe, true, Some(home), &never);
    assert_eq!(p, PathBuf::from("/Users/azam/Library/Application Support/MicroParquet"));
    // writability is asked about the folder that would be used
    let asked = std::cell::RefCell::new(Vec::new());
    let rec = |d: &Path| {
        asked.borrow_mut().push(d.to_path_buf());
        false
    };
    let _ = choose_portable_root(exe, true, Some(home), &rec);
    assert_eq!(asked.borrow().as_slice(), &[PathBuf::from("/Applications/portable_data")]);
    // an app in a user folder
    let exe2 = Path::new("/Users/azam/Tools/MicroParquet.app/Contents/MacOS/MicroParquet");
    assert_eq!(choose_portable_root(exe2, true, Some(home), &always), PathBuf::from("/Users/azam/Tools/portable_data"));
    // no home folder known and not writable: nothing better exists, keep the folder beside the app
    assert_eq!(choose_portable_root(exe, true, None, &never), PathBuf::from("/Applications/portable_data"));
    // a bare binary on macOS (cargo run): next to the executable
    assert_eq!(
        choose_portable_root(Path::new("/Users/azam/src/target/release/MicroParquet"), true, Some(home), &never),
        PathBuf::from("/Users/azam/src/target/release/portable_data")
    );
}

#[test]
fn l5_windows_and_linux_keep_it_next_to_the_executable() {
    let p = choose_portable_root(Path::new(r"D:\Tools\MicroParquet\MicroParquet.exe"), false, Some(Path::new("/home/a")), &never);
    assert!(p.ends_with("portable_data"));
    let p = choose_portable_root(Path::new("/opt/mp/MicroParquet"), false, Some(Path::new("/home/a")), &never);
    assert_eq!(p, PathBuf::from("/opt/mp/portable_data"), "no fallback outside macOS");
}

#[test]
fn l5_dir_is_writable_checks_by_writing() {
    let d = dir("w");
    assert!(dir_is_writable(&d.join("portable_data")), "creates the folder");
    assert!(d.join("portable_data").is_dir());
    // a folder that cannot be created: its parent is a file
    let file = d.join("a_file");
    std::fs::write(&file, b"x").unwrap();
    assert!(!dir_is_writable(&file.join("portable_data")));
    // no probe file left behind
    assert_eq!(std::fs::read_dir(d.join("portable_data")).unwrap().count(), 0);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn l5_one_version_number_everywhere() {
    assert_eq!(env!("CARGO_PKG_VERSION"), "1.3.0");
    let d = dir("ver");
    let p = d.join("m.mpq.json");
    let recipe = MacroRecipe {
        name: "v".into(),
        description: String::new(),
        created_at: String::new(),
        enforce_balance_control: false,
        control_column: None,
        optional_ingestion_schema: None,
        steps: vec![],
        summary: Default::default(),
    };
    save_macro_to_file(&recipe, &p).unwrap();
    assert_eq!(macro_file_version(&p).as_deref(), Some(env!("CARGO_PKG_VERSION")));
    assert_eq!(load_macro_from_file(&p).unwrap().name, "v", "the version key does not stop the file loading");
    // the Python export header names the same version
    let py = generate_python_polars_code(Path::new("x.csv"), false, &IngestionConfig::default(), &[], None).unwrap();
    assert!(py.contains(&format!("MicroParquet v{}", env!("CARGO_PKG_VERSION"))), "{}", &py[..200.min(py.len())]);
    // no stale version strings left in the sources
    for f in ["src/main.rs", "src/macros.rs", "src/ui/buckets.rs", "src/ui/columns.rs", "src/ui/steps.rs"] {
        let t = std::fs::read_to_string(f).unwrap();
        assert!(!t.contains("v1.2"), "{f} still names v1.2");
    }
    // the macOS Info.plist is written from Cargo.toml's version
    let wf = std::fs::read_to_string(".github/workflows/main.yml").unwrap();
    assert!(wf.contains("${VERSION}") && !wf.contains("<string>1.1.0</string>"));
    std::fs::remove_dir_all(&d).ok();
}

// ------------------------------------------------------------------------ compatibility

fn strip_keys(v: &mut serde_json::Value, keys: &[&str]) {
    match v {
        serde_json::Value::Object(m) => {
            for k in keys {
                m.remove(*k);
            }
            for (_, x) in m.iter_mut() {
                strip_keys(x, keys);
            }
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(|x| strip_keys(x, keys)),
        _ => {}
    }
}

fn rich_recipe() -> MacroRecipe {
    let mut cfg = IngestionConfig::default();
    cfg.columns.push(ColumnSchemaConfig { name: "Balance".into(), dtype: ColDataType::Float64, ..Default::default() });
    MacroRecipe {
        name: "ALCO".into(),
        description: "d".into(),
        created_at: "2026-09-30 10:00".into(),
        enforce_balance_control: true,
        control_column: Some("Balance".into()),
        optional_ingestion_schema: Some(cfg),
        steps: vec![
            TransformStep::FilterRows {
                column: "Brancd".into(),
                operator: FilterOp::Eq,
                value: "DHK".into(),
                keep_matching: true,
            },
            TransformStep::RateShockSim {
                bal_col: "Balance".into(),
                rate_col: "Int Rate".into(),
                mat_col: "Matdat".into(),
                cutoff_date: "2026-10-31".into(),
                shock_bps: 100.0,
                side_col: Some("Side".into()),
                reprice_col: None,
                beta_file: None,
                product_col: None,
                floor_pct: None,
            },
            TransformStep::FtpCurveMap {
                opndat_col: "Opndat".into(),
                matdat_col: "Matdat".into(),
                bal_col: "Balance".into(),
                rate_col: "Int Rate".into(),
                curve: default_bd_ftp_curve(),
                is_liability: true,
                interpolation: FtpInterpolation::Linear,

                curve_mode: Default::default(), currency: "BDT".into(),
            },
        ],
        summary: Default::default(),
    }
}

#[test]
fn l5_a_phase1_macro_without_any_later_field_still_loads_with_the_old_meaning() {
    let d = dir("p1");
    let mut v = serde_json::to_value(rich_recipe()).unwrap();
    // fields added in Phase 2 and later
    strip_keys(
        &mut v,
        &["side_col", "interpolation", "sign_convention", "has_dr_cr", "source_dtype", "separator", "skip_rows", "encoding", "format_spec", "app_version", "active_statuses", "rate_min_pct", "rate_max_pct"],
    );
    let p = d.join("phase1.mpq.json");
    std::fs::write(&p, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    let r = load_macro_from_file(&p).unwrap();
    assert_eq!(r.name, "ALCO");
    assert_eq!(r.steps.len(), 3);
    match &r.steps[1] {
        TransformStep::RateShockSim { side_col, .. } => assert!(side_col.is_none(), "old shock macros have no side column"),
        s => panic!("{s:?}"),
    }
    match &r.steps[2] {
        TransformStep::FtpCurveMap { interpolation, .. } => {
            assert_eq!(*interpolation, FtpInterpolation::NextPointUp, "old FTP macros keep their past results")
        }
        s => panic!("{s:?}"),
    }
    let col = &r.optional_ingestion_schema.unwrap().columns[0];
    assert_eq!(col.sign_convention, SignConvention::Unset);
    assert!(!col.has_dr_cr);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn l5_a_phase2_macro_and_a_phase3_macro_load_and_round_trip() {
    let d = dir("p2");
    // Phase 2: everything present, no app_version key.
    let mut v = serde_json::to_value(rich_recipe()).unwrap();
    strip_keys(&mut v, &["app_version"]);
    let p2 = d.join("phase2.mpq.json");
    std::fs::write(&p2, serde_json::to_string(&v).unwrap()).unwrap();
    let r2 = load_macro_from_file(&p2).unwrap();
    assert_eq!(macro_file_version(&p2), None);
    match &r2.steps[2] {
        TransformStep::FtpCurveMap { interpolation, .. } => assert_eq!(*interpolation, FtpInterpolation::Linear),
        s => panic!("{s:?}"),
    }
    // Phase 3: saved by this version, with a Set to Number step.
    let mut r3 = rich_recipe();
    r3.steps.push(TransformStep::ConvertToNumber { columns: vec!["Amount_Text".into()] });
    let p3 = d.join("phase3.mpq.json");
    save_macro_to_file(&r3, &p3).unwrap();
    let back = load_macro_from_file(&p3).unwrap();
    assert_eq!(back.steps.len(), 4);
    assert_eq!(macro_file_version(&p3).as_deref(), Some("1.3.0"));
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn l5_schema_files_from_earlier_phases_still_load() {
    let mut cfg = IngestionConfig::default();
    cfg.columns.push(ColumnSchemaConfig { name: "Balance".into(), dtype: ColDataType::Float64, ..Default::default() });
    let mut v = serde_json::to_value(&cfg).unwrap();
    strip_keys(&mut v, &["sign_convention", "has_dr_cr", "source_dtype", "separator", "skip_rows", "encoding"]);
    let back: IngestionConfig = serde_json::from_value(v).unwrap();
    assert_eq!(back.columns[0].name, "Balance");
    assert_eq!(back.separator, ',');
    assert_eq!(back.skip_rows, 0);
}

#[test]
fn l5_preset_and_rule_files_from_earlier_phases_still_load() {
    let d = dir("rules");
    // bucket presets file (Phase 1/2 format): a JSON list of definitions
    let def = CustomBucketDef {
        name: "legacy".into(),
        rules: vec![BucketRuleItem { max_days: 30, label: "01. 1-30 Days".into() }],
        overflow_label: "02. Over".into(),
        overdue_label: "03. Overdue".into(),
        null_sight_label: "00. Sight".into(),
        null_long_label: "04. Core".into(),
    };
    let file = d.join("buckets.json");
    std::fs::write(&file, serde_json::to_string(&vec![def.clone()]).unwrap()).unwrap();
    let loaded = CustomBucketDef::load_all_presets(&file);
    assert!(loaded.iter().any(|b| b.name == "legacy"), "{:?}", loaded.iter().map(|b| &b.name).collect::<Vec<_>>());
    // provisioning rule files: written on first use, and read back
    let cfg = micro_parquet::banking::ProvisionConfig::load_or_create(&d).unwrap();
    assert!(cfg.warnings.is_empty(), "{:?}", cfg.warnings);
    let again = micro_parquet::banking::ProvisionConfig::load_or_create(&d).unwrap();
    assert_eq!(cfg.cl_code_map.len(), again.cl_code_map.len());
    std::fs::remove_dir_all(&d).ok();
}
