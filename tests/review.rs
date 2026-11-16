// Review harness: exercises MicroParquet's calculation code against hand-checked cases.
// Expected values come from docs/phase1-spec.md. Tests for work outside Phase 1
// (Phase 2) keep their evidence printout and assert only what is already settled.
use micro_parquet::banking::*;
use micro_parquet::engine::*;
use micro_parquet::models::*;
use polars::prelude::*;
use std::io::Write as _;
use std::path::PathBuf;

fn tmpdir() -> PathBuf {
    let d = std::env::temp_dir().join("mp_review");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write_csv(name: &str, body: &str) -> PathBuf {
    let p = tmpdir().join(name);
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(body.as_bytes()).unwrap();
    p
}

fn write_parquet(name: &str, df: &DataFrame) -> PathBuf {
    let p = tmpdir().join(name);
    let mut d = df.clone();
    ParquetWriter::new(std::fs::File::create(&p).unwrap()).finish(&mut d).unwrap();
    p
}

fn dtype_of(cfg: &IngestionConfig, col: &str) -> ColDataType {
    cfg.columns.iter().find(|c| c.name == col).unwrap().dtype
}

/// Float comparison with tolerance 1e-6 × magnitude (absolute 1e-6 near zero).
fn assert_close(actual: f64, expected: f64, what: &str) {
    let tol = 1e-6 * expected.abs().max(1.0);
    assert!(
        (actual - expected).abs() <= tol,
        "{what}: expected {expected}, got {actual}"
    );
}

fn str_col(df: &DataFrame, name: &str) -> Vec<Option<String>> {
    df.column(name)
        .unwrap()
        .str()
        .unwrap()
        .into_iter()
        .map(|v| v.map(|s| s.to_string()))
        .collect()
}

fn f64_col(df: &DataFrame, name: &str) -> Vec<Option<f64>> {
    df.column(name)
        .unwrap()
        .cast(&DataType::Float64)
        .unwrap()
        .f64()
        .unwrap()
        .into_iter()
        .collect()
}

fn days(y: i32, m: u32, d: u32) -> i32 {
    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    (chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap() - epoch).num_days() as i32
}

fn date_days_col(df: &DataFrame, name: &str) -> Vec<Option<i32>> {
    df.column(name)
        .unwrap()
        .cast(&DataType::Int32)
        .unwrap()
        .i32()
        .unwrap()
        .into_iter()
        .collect()
}

#[test]
fn t01_schema_inference() {
    let p = write_csv(
        "infer.csv",
        "Actnum,Account_Balance,Residual_Maturity,Reporting_Date,Matdat,Opndat_US,Balance,Liquidity_Bucket_Amt,Paid_Amount,Customer_ID,BranchCode\n\
         0012345,1500000.50,15/11/2026,31/10/2026,05/11/2026,10/31/2026,100,5,7,0012,101\n\
         0012346,2500000.00,20/12/2027,31/10/2026,15/03/2027,11/30/2026,200,6,8,0013,102\n\
         0012347,(350.25),01/01/2030,31/10/2026,28/02/2028,12/31/2026,300,7,9,0014,103\n",
    );
    let cfg = auto_detect_smart_schema(&p).unwrap();
    let expected = [
        ("Actnum", ColDataType::Utf8),
        ("Account_Balance", ColDataType::Float64),
        ("Residual_Maturity", ColDataType::DateDMY),
        ("Reporting_Date", ColDataType::DateDMY),
        ("Matdat", ColDataType::DateDMY),
        ("Opndat_US", ColDataType::DateMDY),
        ("Balance", ColDataType::Float64),
        ("Liquidity_Bucket_Amt", ColDataType::Float64),
        ("Paid_Amount", ColDataType::Float64),
        ("Customer_ID", ColDataType::Utf8),
        ("BranchCode", ColDataType::Utf8),
    ];
    for (name, dtype) in expected {
        assert_eq!(dtype_of(&cfg, name), dtype, "inferred type of {name}");
    }
}

#[test]
fn t02_ambiguous_dates_prefer_dmy() {
    // All days <= 12: genuinely ambiguous; Bangladesh CBS exports are usually D/M/Y
    let p = write_csv("ambig.csv", "Matdat\n3/4/2027\n5/6/2027\n11/12/2027\n");
    let cfg = auto_detect_smart_schema(&p).unwrap();
    assert_eq!(dtype_of(&cfg, "Matdat"), ColDataType::DateDMY);
}

#[test]
fn t03_date_parsing_variants() {
    let p = write_csv(
        "dates.csv",
        "Matdat\n05/11/2026\n5/11/26\n31-10-2026\n31-OCT-2026\n2026-10-31\n",
    );
    let cfg = IngestionConfig {
        columns: vec![ColumnSchemaConfig {
            name: "Matdat".into(),
            dtype: ColDataType::DateDMY,
            pii_mask: PiiMaskMode::None,
            sample_values: vec![],
        }],
        ..Default::default()
    };
    let df = build_lazy_pipeline(&p, false, &cfg, &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(
        date_days_col(&df, "Matdat"),
        vec![
            Some(days(2026, 11, 5)),
            Some(days(2026, 11, 5)),
            Some(days(2026, 10, 31)),
            Some(days(2026, 10, 31)),
            Some(days(2026, 10, 31)),
        ]
    );
}

#[test]
fn t04a_numeric_parsing_phase1() {
    // Phase 1 behaviour: comma thousands and bracket negatives parse; row count is kept.
    let p = write_csv(
        "nums.csv",
        "Balance\n\"1,234.50\"\n(500.00)\n1234.50 Cr\n250.00 Dr\n-\n 99.5 \nBDT 10\n",
    );
    let cfg = IngestionConfig {
        columns: vec![ColumnSchemaConfig {
            name: "Balance".into(),
            dtype: ColDataType::Float64,
            pii_mask: PiiMaskMode::None,
            sample_values: vec![],
        }],
        ..Default::default()
    };
    let lf = build_lazy_pipeline(&p, false, &cfg, &tmpdir()).unwrap();
    let vals = f64_col(&lf.clone().collect().unwrap(), "Balance");
    assert_eq!(vals.len(), 7);
    assert_close(vals[0].unwrap(), 1234.5, "comma thousands");
    assert_close(vals[1].unwrap(), -500.0, "bracket negative");
    assert_close(vals[5].unwrap(), 99.5, "padded number");
    let (rows, _) = compute_control_totals(lf, Some("Balance")).unwrap();
    assert_eq!(rows, 7);
}

#[test]
#[ignore = "Phase 2: Dr/Cr suffixes, currency prefix and dash-as-zero parsing; raw-vs-parsed control sums"]
fn t04_numeric_parsing_silent_nulls() {
    // Target: no value silently becomes null. Cr = +, Dr = − (owner to confirm), "-" = 0,
    // "BDT 10" = 10. Parsed control sum = 1,828.50 with 0 nulls.
    let p = write_csv(
        "nums.csv",
        "Balance\n\"1,234.50\"\n(500.00)\n1234.50 Cr\n250.00 Dr\n-\n 99.5 \nBDT 10\n",
    );
    let cfg = IngestionConfig {
        columns: vec![ColumnSchemaConfig {
            name: "Balance".into(),
            dtype: ColDataType::Float64,
            pii_mask: PiiMaskMode::None,
            sample_values: vec![],
        }],
        ..Default::default()
    };
    let lf = build_lazy_pipeline(&p, false, &cfg, &tmpdir()).unwrap();
    let vals = f64_col(&lf.clone().collect().unwrap(), "Balance");
    assert_eq!(
        vals,
        vec![Some(1234.5), Some(-500.0), Some(1234.5), Some(-250.0), Some(0.0), Some(99.5), Some(10.0)]
    );
    let (rows, sum) = compute_control_totals(lf, Some("Balance")).unwrap();
    assert_eq!(rows, 7);
    assert_close(sum.unwrap(), 1828.5, "parsed control sum");
}

#[test]
fn t05_parquet_datetime_maturity_bucket() {
    // Parquet written by pandas / Power BI usually stores dates as Datetime(ns/us/ms), not Date
    let mut df = df!(
        "Balance" => [100.0, 200.0, 300.0],
        "Matdat_str" => ["2026-11-05", "2027-03-15", "2031-01-01"]
    )
    .unwrap();
    let dt = df
        .column("Matdat_str")
        .unwrap()
        .str()
        .unwrap()
        .as_date(Some("%Y-%m-%d"), false)
        .unwrap()
        .into_series()
        .cast(&DataType::Datetime(TimeUnit::Microseconds, None))
        .unwrap()
        .with_name("Matdat".into());
    df.with_column(dt).unwrap();
    let p = write_parquet("dt.parquet", &df);

    let lf = build_lazy_pipeline(&p, true, &IngestionConfig::default(), &tmpdir()).unwrap();
    let preset = &CustomBucketDef::default_presets()[0];
    let out = apply_maturity_bucket(lf.clone(), "Matdat", "2026-10-31", preset, true, "Bucket")
        .unwrap()
        .collect()
        .unwrap();
    assert_eq!(
        str_col(&out, "Bucket"),
        vec![
            Some("02. 2 to 7 Days".to_string()),
            Some("05. 3 to 6 Months (91-180d)".to_string()),
            Some("08. 3 to 5 Years".to_string()),
        ]
    );

    // String-typed date column (parquet that came from an untyped CSV) must fail loudly
    let err = apply_maturity_bucket(lf.clone(), "Matdat_str", "2026-10-31", preset, true, "Bucket")
        .err()
        .expect("string date column must be rejected");
    assert!(format!("{err:#}").contains("not a date"), "got: {err:#}");

    // Missing column must name the column
    let err = apply_maturity_bucket(lf, "No_Such_Col", "2026-10-31", preset, true, "Bucket")
        .err()
        .expect("missing column must be rejected");
    assert!(format!("{err:#}").contains("No_Such_Col"), "got: {err:#}");

    // Null maturity with null_as_sight = true -> sight bucket
    let nulls = df!("Matdat" => [None::<i32>, Some(days(2026, 11, 5))])
        .unwrap()
        .lazy()
        .with_column(col("Matdat").cast(DataType::Date));
    let out = apply_maturity_bucket(nulls, "Matdat", "2026-10-31", preset, true, "Bucket")
        .unwrap()
        .collect()
        .unwrap();
    assert_eq!(
        str_col(&out, "Bucket"),
        vec![
            Some("01. Overnight / Sight (Non-Maturing)".to_string()),
            Some("02. 2 to 7 Days".to_string()),
        ]
    );
}

#[test]
fn t05b_other_date_inputs_reject_strings() {
    let df = df!(
        "Opndat" => ["2026-01-01"],
        "Matdat" => ["2027-01-01"],
        "Clsdat" => [None::<&str>],
        "Acstat" => ["ACT"],
        "Balance" => [100.0],
        "Rate" => [9.0]
    )
    .unwrap();
    let e1 = apply_rate_shock_sim(df.clone().lazy(), "Balance", "Rate", "Matdat", "2026-10-31", 100.0, None).err();
    let e2 = apply_ftp_curve_map(df.clone().lazy(), "Opndat", "Matdat", "Balance", "Rate", &default_bd_ftp_curve(), true).err();
    let e3 = apply_banking_hygiene_audit(df.clone().lazy(), "Opndat", "Matdat", "Clsdat", "Acstat", "Balance", "Rate", "2026-10-31", false).err();
    let e4 = apply_duration_analysis(df.lazy(), "Balance", "Rate", "Matdat", "2026-10-31", 1, 200.0, 0.0833, None).err();
    for (name, e) in [("shock", e1), ("ftp", e2), ("hygiene", e3), ("duration", e4)] {
        let e = e.unwrap_or_else(|| panic!("{name}: string date column must be rejected"));
        assert!(format!("{e:#}").contains("not a date"), "{name}: got {e:#}");
    }
}

#[test]
fn t06_auto_format_classification() {
    let cases = [
        ("Modified_Duration", "25,000,000.00"),
        ("Macaulay_Duration_Yrs", "25,000,000.00"),
        ("Corporate_Loan_Balance", "2.50 Cr"),
        ("Account_Balance", "2.50 Cr"),
        ("Discount_Amount", "2.50 Cr"),
        ("Rate_Delta_Bps", "25,000,000.00"),
        ("Utility_Bill_Amount", "2.50 Cr"),
        ("Balance", "2.50 Cr"),
        ("No_of_Accounts", "25,000,000.00"),
        ("Interest_Rate", "25,000,000.00%"),
    ];
    for (name, expected) in cases {
        let s = format_metric_value(25_000_000.0, name, None, NumberDisplayFormat::BdtCrore, 2);
        assert_eq!(s, expected, "format of {name}");
    }
}

#[test]
fn t07_provisioning() {
    // T7 acceptance at cutoff 2026-10-31, using rule files written to and re-read from disk.
    let dir = tmpdir().join("t07_rules");
    let _ = std::fs::remove_dir_all(&dir);
    let cfg = ProvisionConfig::load_or_create(&dir).unwrap();
    assert!(dir.join("cl_code_map.json").exists() && dir.join("segment_map.json").exists());
    assert_eq!(cfg.rules, ProvisionConfig::defaults().rules, "rule file round trip");

    let df = df!(
        "Balance" => [1_000_000.0; 13],
        "CL" => ["UC", "SMA", "SS", "DF", "B/L", "BL", "STD-1", "SS", "BL", "BL", "DF", "SS", "STD"],
        "Seg" => ["CORPORATE", "CORPORATE", "CORPORATE", "CORPORATE", "CORPORATE", "NON-CMSME", "SMALL", "SMALL", "CORPORATE", "CORPORATE", "AGRI TERM LOAN", "CORPORATE", "MEDIUM"],
        "Susp" => [0.0, 0.0, 100_000.0, 100_000.0, 100_000.0, 100_000.0, 0.0, 0.0, 0.0, 0.0, 0.0, 900_000.0, 0.0],
        "Sec" => [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 950_000.0, 950_000.0, 0.0, 0.0, 0.0],
        "Coll" => [None, None, None, None, None, None, None, None, None, Some(" deposit lien"), None, None, None]
    )
    .unwrap();
    let run = |df: DataFrame, coll: Option<&str>, cutoff: &str| {
        apply_loan_classification_provision_with(df.lazy(), "Balance", "CL", Some("Seg"), Some("Susp"), Some("Sec"), coll, cutoff, &cfg)
    };
    let out = run(df.clone(), Some("Coll"), "2026-10-31").unwrap().collect().unwrap();
    let prov = f64_col(&out, "Required_Provision_BDT");
    let expected = [
        10_000.0, 50_000.0, 180_000.0, 450_000.0, 900_000.0, 900_000.0, 5_000.0, 200_000.0, 150_000.0, 50_000.0, 500_000.0,
        20_000.0, // SS, suspense 900,000, no security: no 15% floor
        10_000.0, // MEDIUM is not in the CMSME concession
    ];
    for (i, e) in expected.iter().enumerate() {
        assert_close(prov[i].unwrap(), *e, &format!("row {} provision", i + 1));
    }
    let bfp = f64_col(&out, "Base_For_Provision_BDT");
    assert_close(bfp[2].unwrap(), 900_000.0, "SS BFP");
    assert_close(bfp[8].unwrap(), 150_000.0, "BL BFP with 15% floor");
    assert_close(bfp[11].unwrap(), 100_000.0, "SS BFP, no security, no floor");
    assert_eq!(f64_col(&out, "Statutory_Prov_Rate_Pct")[10], Some(50.0));
    let cat = str_col(&out, "Reg_Category");
    assert_eq!(cat[5].as_deref(), Some("OTHER"), "NON-CMSME");
    assert_eq!(cat[6].as_deref(), Some("CMSME"));
    assert_eq!(cat[10].as_deref(), Some("OTHER"), "AGRI TERM LOAN is not an exact match");
    assert_eq!(cat[12].as_deref(), Some("OTHER"), "MEDIUM");
    let unmapped: Vec<Option<bool>> = out.column("Segment_Unmapped").unwrap().bool().unwrap().into_iter().collect();
    assert_eq!(unmapped[0], Some(false), "CORPORATE is listed as OTHER");
    assert_eq!(unmapped[5], Some(false), "NON-CMSME is listed as OTHER");
    assert_eq!(unmapped[6], Some(false));
    assert_eq!(unmapped[10], Some(true));
    assert_eq!(unmapped[12], Some(false), "MEDIUM is listed as OTHER");
    let grade = str_col(&out, "CL_Grade");
    assert_eq!(grade[0].as_deref(), Some("STD"));
    assert_eq!(grade[4].as_deref(), Some("BL"));
    assert_eq!(str_col(&out, "NPL_Category")[2].as_deref(), Some("NPL (Classified)"));
    let src = str_col(&out, "Rule_Source");
    assert!(src[0].as_deref().unwrap().contains("Circular 15"));
    assert!(src[6].as_deref().unwrap().ends_with("[verify]"), "{:?}", src[6]);
    assert!(out.column("Cutoff_Assumed").is_err());
    println!("PROV:\n{}", out.drop("Balance").unwrap());

    // No collateral-type column at all: the 15% floor applies
    let out = run(df.slice(8, 1), None, "2026-10-31").unwrap().collect().unwrap();
    assert_close(f64_col(&out, "Base_For_Provision_BDT")[0].unwrap(), 150_000.0, "BL BFP, no collateral column");
    assert_close(f64_col(&out, "Required_Provision_BDT")[0].unwrap(), 150_000.0, "BL, no collateral column");

    // Literal spec value DEPOSIT_LIEN: no floor, BFP 50,000, provision 50,000
    let lien = df!(
        "Balance" => [1_000_000.0], "CL" => ["BL"], "Seg" => ["CORPORATE"],
        "Susp" => [0.0], "Sec" => [950_000.0], "Coll" => ["DEPOSIT_LIEN"]
    )
    .unwrap();
    let out = run(lien, Some("Coll"), "2026-10-31").unwrap().collect().unwrap();
    assert_close(f64_col(&out, "Base_For_Provision_BDT")[0].unwrap(), 50_000.0, "DEPOSIT_LIEN BFP");
    assert_close(f64_col(&out, "Required_Provision_BDT")[0].unwrap(), 50_000.0, "DEPOSIT_LIEN provision");

    // CMSME STD outside the 0.5% window falls back to the ANY 1% row
    for cutoff in ["2027-01-31", "2025-06-30"] {
        let out = run(df.slice(6, 1), None, cutoff).unwrap().collect().unwrap();
        assert_close(f64_col(&out, "Required_Provision_BDT")[0].unwrap(), 10_000.0, cutoff);
    }

    // Unknown CL codes: an error that names them, never a default rate (BLW is not mapped)
    let bad = df!("Balance" => [1.0, 2.0, 3.0], "CL" => ["UC", "XYZ", "BLW"], "Seg" => ["A", "B", "C"], "Susp" => [0.0; 3], "Sec" => [0.0; 3]).unwrap();
    let err = run(bad, None, "2026-10-31").err().expect("XYZ and BLW must fail").to_string();
    assert!(err.contains("XYZ") && err.contains("BLW") && !err.contains("'UC'"), "{err}");

    // Cutoff before the first rule: the owner's message
    let err = run(df.slice(0, 1), None, "2025-03-31").err().expect("no rule before 2025-04-01").to_string();
    assert!(err.contains("No provisioning rule covers cutoff 2025-03-31; rules start 2025-04-01 (BRPD Circular 15/2024)"), "{err}");

    // Blank cutoff = today, flagged
    let out = run(df.slice(0, 1), None, "").unwrap().collect().unwrap();
    assert!(out.column("Cutoff_Assumed").is_ok());
}

#[test]
fn t07b_provision_rule_files() {
    let cutoff = |s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    let mut cfg = ProvisionConfig::defaults();
    assert_eq!(cfg.lookup("SMA", "CMSME", cutoff("2025-10-12")).unwrap().rate_pct, 5.0);
    assert_eq!(cfg.lookup("SMA", "CMSME", cutoff("2025-10-13")).unwrap().rate_pct, 1.0);
    assert_eq!(cfg.lookup("SMA", "CMSME", cutoff("2025-12-20")).unwrap().rate_pct, 1.0);
    assert_eq!(cfg.lookup("SMA", "CMSME", cutoff("2025-12-21")).unwrap().rate_pct, 0.5);
    assert_eq!(cfg.lookup("SMA", "CMSME", cutoff("2026-12-31")).unwrap().rate_pct, 0.5);
    assert_eq!(cfg.lookup("SMA", "CMSME", cutoff("2027-01-01")).unwrap().rate_pct, 5.0);
    assert_eq!(cfg.lookup("SS", "CMSME", cutoff("2026-10-31")).unwrap().rate_pct, 20.0);
    // Before the first rule: no rate, error
    let err = cfg.lookup("STD", "OTHER", cutoff("2025-03-31")).unwrap_err().to_string();
    assert!(err.contains("No provisioning rule covers cutoff 2025-03-31"), "{err}");
    // Overlap: a second ANY row in force on the same day is an error naming both
    let mut extra = cfg.rules[0].clone();
    extra.source = "duplicate row".into();
    extra.effective_from = "2026-01-01".into();
    cfg.rules.push(extra);
    let err = cfg.lookup("STD", "OTHER", cutoff("2026-10-31")).unwrap_err().to_string();
    assert!(err.contains("duplicate row") && err.contains("Circular 15"), "{err}");

    // File errors name the file and line
    let dir = tmpdir().join("t07b_rules");
    let _ = std::fs::remove_dir_all(&dir);
    ProvisionConfig::ensure_default_files(&dir).unwrap();
    std::fs::write(dir.join("provision_rules.json"), "[\n  {\"grade\": \"STD\",\n   \"category\": \"ANY\" oops\n]").unwrap();
    let err = ProvisionConfig::load_or_create(&dir).unwrap_err().to_string();
    assert!(err.contains("provision_rules.json") && err.contains("line 3"), "{err}");
    std::fs::remove_file(dir.join("provision_rules.json")).unwrap();
    for f in ["cl_code_map.json", "segment_map.json"] {
        std::fs::write(dir.join(f), "{\n  \"STD\": \"STD\",\n  \"UC\" \"STD\"\n}").unwrap();
        let err = ProvisionConfig::load_or_create(&dir).unwrap_err().to_string();
        assert!(err.contains(f) && err.contains("line 3"), "{f}: {err}");
        std::fs::remove_file(dir.join(f)).unwrap();
    }
    // Same code listed twice (after normalising) is an error, not last-one-wins
    std::fs::write(dir.join("cl_code_map.json"), "{\"BL\": \"BL\", \" bl \": \"DF\"}").unwrap();
    let err = ProvisionConfig::load_or_create(&dir).unwrap_err().to_string();
    assert!(err.contains("cl_code_map.json") && err.contains("more than once"), "{err}");
    // Grades are validated
    std::fs::write(dir.join("cl_code_map.json"), "{\"UC\": \"PERFORMING\"}").unwrap();
    let err = ProvisionConfig::load_or_create(&dir).unwrap_err().to_string();
    assert!(err.contains("PERFORMING"), "{err}");
}

fn wavg_step(weight: Option<&str>) -> TransformStep {
    TransformStep::GroupByPivot {
        group_cols: vec!["Grp".into()],
        pivot_col: None,
        aggs: vec![AggSpec {
            target_col: "Rate".into(),
            func: AggFunc::WeightedAvg,
            weight_col: weight.map(|w| w.to_string()),
            alias: "WAVG_Rate".into(),
            format_spec: ColumnFormatSpec::default(),
        }],
        sort_by_col: None,
        top_n: None,
    }
}

#[test]
fn t08_wavg_with_nulls() {
    // Rate missing on the big account: true WAVG over known rates = (10*100)/100 = 10%
    let df = df!(
        "Grp" => ["A", "A"],
        "Rate" => [Some(10.0), None],
        "Balance" => [100.0, 900.0]
    )
    .unwrap();
    // compile via a parquet source so the real step path is exercised
    let p = write_parquet("wavg.parquet", &df);
    let src = TabSource::SingleFile { path: p, is_parquet: true, config: IngestionConfig::default() };
    let out = compile_tab_lazy(&src, &[wavg_step(Some("Balance"))], &tmpdir()).unwrap().collect().unwrap();
    let v = f64_col(&out, "WAVG_Rate");
    assert_close(v[0].expect("WAVG must not be null"), 10.0, "WAVG with null rate");

    // Every weight 0 -> null, not NaN or inf
    let zero = df!("Grp" => ["A", "A"], "Rate" => [10.0, 12.0], "Balance" => [0.0, 0.0]).unwrap();
    let p0 = write_parquet("wavg0.parquet", &zero);
    let src0 = TabSource::SingleFile { path: p0, is_parquet: true, config: IngestionConfig::default() };
    let out0 = compile_tab_lazy(&src0, &[wavg_step(Some("Balance"))], &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(f64_col(&out0, "WAVG_Rate"), vec![None]);

    // Missing weight column is an error, not a self-weighted average
    for w in [None, Some(""), Some("  ")] {
        let src = TabSource::SingleFile {
            path: tmpdir().join("wavg.parquet"),
            is_parquet: true,
            config: IngestionConfig::default(),
        };
        let err = compile_tab_lazy(&src, &[wavg_step(w)], &tmpdir()).err().expect("weight column required");
        assert!(format!("{err:#}").contains("Weighted average needs a weight column"), "got {err:#}");
    }
}

#[test]
fn t09_duration_and_eve() {
    // T4: ΔEVE is signed by side (asset −, liability +); DV01 stays unsigned.
    // 5y fixed 10% annual-pay par instrument, BDT 100: textbook Macaulay D = 4.1699, ModD = 3.7908
    let cutoff = "2026-10-31";
    let mat_days = days(2031, 10, 30); // 1825 days later
    let df = df!(
        "Side" => ["Asset", "Deposit"],
        "Balance" => [100.0, 100.0],
        "Rate" => [10.0, 10.0],
        "Matdat" => [mat_days, mat_days]
    )
    .unwrap()
    .lazy()
    .with_column(col("Matdat").cast(DataType::Date));
    let out = apply_duration_analysis(df, "Balance", "Rate", "Matdat", cutoff, 1, 200.0, 0.0833, Some("Side"))
        .unwrap()
        .collect()
        .unwrap();
    let mac = f64_col(&out, "Macaulay_Duration_Yrs");
    let md = f64_col(&out, "Modified_Duration");
    assert_close(mac[0].unwrap(), 4.1699, "Macaulay duration");
    assert_close(md[0].unwrap(), 3.7908, "Modified duration");
    let eve = f64_col(&out, "Delta_EVE_Impact_BDT");
    assert_close(eve[0].unwrap(), -7.58, "asset ΔEVE +200bp");
    assert_close(eve[1].unwrap(), 7.58, "deposit ΔEVE +200bp");
    let dv01 = f64_col(&out, "DV01_BPV_BDT");
    assert!(dv01[0].unwrap() > 0.0 && dv01[1].unwrap() > 0.0, "DV01 stays unsigned");
    assert!(out.column("Direction_Assumed").is_err());
    println!("DURATION:\n{}", out);
}

#[test]
fn t10_nii_shock_signs() {
    // T4: +100bp, 3m loan and 3m term deposit (90 days to 2027-01-29) → +7,534.25 / −7,534.25, net 0.
    let make = |sides: [&str; 2]| {
        df!(
            "Product" => ["Loan 3m", "Term deposit 3m"],
            "Side" => sides,
            "Balance" => [1_000_000.0, 1_000_000.0],
            "Rate" => [12.0, 9.0],
            "Matdat" => [days(2027, 1, 29), days(2027, 1, 29)]
        )
        .unwrap()
        .lazy()
        .with_column(col("Matdat").cast(DataType::Date))
    };
    let out = apply_rate_shock_sim(make([" asset", "Deposit"]), "Balance", "Rate", "Matdat", "2026-10-31", 100.0, Some("Side"))
        .unwrap()
        .collect()
        .unwrap();
    let nii = f64_col(&out, "NII_Impact_BDT");
    assert!((nii[0].unwrap() - 7_534.25).abs() <= 0.01, "loan ΔNII: {:?}", nii[0]);
    assert!((nii[1].unwrap() + 7_534.25).abs() <= 0.01, "deposit ΔNII: {:?}", nii[1]);
    assert!((nii[0].unwrap() + nii[1].unwrap()).abs() <= 0.01, "net ΔNII");
    // Shocked interest is the interest amount after the shock (income or expense), so it rises for both.
    let shocked = f64_col(&out, "Shocked_Annual_Int");
    assert!((shocked[1].unwrap() - (90_000.0 + 7_534.25)).abs() <= 0.01);
    println!("NII SHOCK +100bp:\n{}", out);

    // Other side spellings
    let out = apply_rate_shock_sim(make(["A", "liab"]), "Balance", "Rate", "Matdat", "2026-10-31", 100.0, Some("Side"))
        .unwrap()
        .collect()
        .unwrap();
    let nii = f64_col(&out, "NII_Impact_BDT");
    assert!(nii[0].unwrap() > 0.0 && nii[1].unwrap() < 0.0);

    // Unmapped side values fail loudly and are named
    let err = apply_rate_shock_sim(make(["Loan", "Term deposit"]), "Balance", "Rate", "Matdat", "2026-10-31", 100.0, Some("Side"))
        .err()
        .expect("unmapped sides must fail")
        .to_string();
    assert!(err.contains("'Term deposit'"), "{err}");
    assert!(!err.contains("'Loan'"), "'Loan' starts with L and is a liability: {err}");

    // More than 10 distinct bad values: exactly 10 are listed
    let bad_sides: Vec<String> = (1..=12).map(|i| format!("X{i:02}")).collect();
    let many = df!(
        "Side" => bad_sides.clone(),
        "Balance" => [1.0; 12],
        "Rate" => [9.0; 12],
        "Matdat" => [days(2027, 1, 29); 12]
    )
    .unwrap()
    .lazy()
    .with_column(col("Matdat").cast(DataType::Date));
    let err = apply_rate_shock_sim(many, "Balance", "Rate", "Matdat", "2026-10-31", 100.0, Some("Side"))
        .err()
        .expect("bad sides must fail")
        .to_string();
    let listed = bad_sides.iter().filter(|v| err.contains(&format!("'{v}'"))).count();
    assert_eq!(listed, 10, "{err}");

    // Old macros (no side column): treated as assets, and the output says so
    let out = apply_rate_shock_sim(make(["x", "y"]), "Balance", "Rate", "Matdat", "2026-10-31", 100.0, None)
        .unwrap()
        .collect()
        .unwrap();
    let nii = f64_col(&out, "NII_Impact_BDT");
    assert!(nii[0].unwrap() > 0.0 && nii[1].unwrap() > 0.0);
    assert_eq!(str_col(&out, "Direction_Assumed")[0].as_deref(), Some("ASSET (unspecified)"));
}

#[test]
fn t11a_ftp_tenor_days() {
    // Phase 1 behaviour: contractual tenor in days.
    let o = days(2026, 1, 1);
    let df = df!(
        "Opndat" => [o, o],
        "Matdat" => [o + 100, o + 31],
        "Balance" => [1_000_000.0, 1_000_000.0],
        "Rate" => [9.0, 9.0]
    )
    .unwrap()
    .lazy()
    .with_columns([col("Opndat").cast(DataType::Date), col("Matdat").cast(DataType::Date)]);
    let out = apply_ftp_curve_map(df, "Opndat", "Matdat", "Balance", "Rate", &default_bd_ftp_curve(), true)
        .unwrap()
        .collect()
        .unwrap();
    assert_eq!(f64_col(&out, "Tenor_Days"), vec![Some(100.0), Some(31.0)]);
}

#[test]
#[ignore = "Phase 2: FTP curve linear interpolation between tenor points"]
fn t11_ftp_step_vs_interpolation() {
    // Target: linear interpolation on the default curve (30d 8.75, 91d 9.50, 182d 10.10).
    // 100d = 9.50 + 9/91 × 0.60 = 9.5593…; 31d = 8.75 + 1/61 × 0.75 = 8.7623…
    // (the step lookup gives 10.10 and 9.50).
    let o = days(2026, 1, 1);
    let df = df!(
        "Opndat" => [o, o],
        "Matdat" => [o + 100, o + 31],
        "Balance" => [1_000_000.0, 1_000_000.0],
        "Rate" => [9.0, 9.0]
    )
    .unwrap()
    .lazy()
    .with_columns([col("Opndat").cast(DataType::Date), col("Matdat").cast(DataType::Date)]);
    let out = apply_ftp_curve_map(df, "Opndat", "Matdat", "Balance", "Rate", &default_bd_ftp_curve(), true)
        .unwrap()
        .collect()
        .unwrap();
    let ftp = f64_col(&out, "Benchmark_FTP_Rate");
    assert_close(ftp[0].unwrap(), 9.50 + 9.0 / 91.0 * 0.60, "100-day FTP");
    assert_close(ftp[1].unwrap(), 8.75 + 1.0 / 61.0 * 0.75, "31-day FTP");
}

fn reconcile_frames() -> (DataFrame, DataFrame) {
    let prev = df!(
        "Actnum" => ["A1", "A1", "A2"],
        "Balance" => [100.0, 900.0, 500.0],
        "Rate" => [5.0, 10.0, 8.0]
    )
    .unwrap();
    let curr = df!(
        "Actnum" => ["A1", "A3"],
        "Balance" => [1000.0, 50.0],
        "Rate" => [9.5, 11.0]
    )
    .unwrap();
    (prev, curr)
}

#[test]
fn t12a_reconcile_status_and_balances() {
    // Phase 1 behaviour: statuses and balance deltas.
    let (prev, curr) = reconcile_frames();
    let out = build_period_reconcile_lazy(prev.lazy(), curr.lazy(), "Actnum", "Balance", Some("Rate"), None)
        .unwrap()
        .sort(["Actnum"], Default::default())
        .collect()
        .unwrap();
    assert_eq!(
        str_col(&out, "Reconcile_Status"),
        vec![
            Some("5. Unchanged".to_string()),
            Some("2. Closed / Matured Account".to_string()),
            Some("1. New Account Opened".to_string()),
        ]
    );
    assert_eq!(f64_col(&out, "Balance_Delta"), vec![Some(0.0), Some(-500.0), Some(50.0)]);
}

#[test]
#[ignore = "Phase 2: balance-weighted rates in period reconcile"]
fn t12_reconcile() {
    // Target: A1's two previous rows (100 @ 5%, 900 @ 10%) give a balance-weighted 9.5%,
    // not the simple mean 7.5%, so the rate change to 9.5% is 0 bps.
    let (prev, curr) = reconcile_frames();
    let out = build_period_reconcile_lazy(prev.lazy(), curr.lazy(), "Actnum", "Balance", Some("Rate"), None)
        .unwrap()
        .sort(["Actnum"], Default::default())
        .collect()
        .unwrap();
    assert_close(f64_col(&out, "Prev_Rate")[0].unwrap(), 9.5, "A1 previous WAVG rate");
    assert_close(f64_col(&out, "Rate_Delta_Bps")[0].unwrap(), 0.0, "A1 rate change (bps)");
}

#[test]
#[ignore = "Phase 2: wider PII name hash (CLIENT_ token has 8 hex characters = 32 bits)"]
fn t13_pii_hash_collisions() {
    // Target: 1,000,000 distinct customer names give 1,000,000 distinct CLIENT_ tokens through
    // the real ingestion mask. With 32 bits about 116 collisions are expected.
    let n = 1_000_000usize;
    let mut body = String::with_capacity(n * 24);
    body.push_str("Name\n");
    for i in 0..n {
        body.push_str(&format!("CUSTOMER NAME {}\n", i));
    }
    let p = write_csv("pii_names.csv", &body);
    let cfg = IngestionConfig {
        columns: vec![ColumnSchemaConfig {
            name: "Name".into(),
            dtype: ColDataType::Utf8,
            pii_mask: PiiMaskMode::ClientNameHash,
            sample_values: vec![],
        }],
        ..Default::default()
    };
    let df = build_lazy_pipeline(&p, false, &cfg, &tmpdir()).unwrap().collect().unwrap();
    let names = df.column("Name").unwrap();
    assert_eq!(names.len(), n);
    assert_eq!(names.n_unique().unwrap(), n, "masked names must stay distinct");
}

#[test]
fn t14_filter_big_account_numbers() {
    // 17-digit IDs lose precision when the filter compares as f64
    let df = df!("Actnum" => ["12345678901234567", "12345678901234568"]).unwrap();
    let p = write_parquet("ids.parquet", &df);
    let src = TabSource::SingleFile { path: p, is_parquet: true, config: IngestionConfig::default() };
    let step = TransformStep::FilterRows {
        column: "Actnum".into(),
        operator: FilterOp::Eq,
        value: "12345678901234567".into(),
        keep_matching: true,
    };
    let out = compile_tab_lazy(&src, &[step], &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(out.height(), 1);
    assert_eq!(str_col(&out, "Actnum"), vec![Some("12345678901234567".to_string())]);

    // Numeric and date columns still compare by value
    let df = df!(
        "Balance" => [50.0, 150.0, 250.0],
        "Matdat" => [days(2026, 10, 15), days(2026, 11, 1), days(2027, 1, 1)]
    )
    .unwrap()
    .lazy()
    .with_column(col("Matdat").cast(DataType::Date))
    .collect()
    .unwrap();
    let p = write_parquet("typed.parquet", &df);
    let src = TabSource::SingleFile { path: p, is_parquet: true, config: IngestionConfig::default() };
    let filt = |c: &str, op: FilterOp, v: &str| TransformStep::FilterRows {
        column: c.into(),
        operator: op,
        value: v.into(),
        keep_matching: true,
    };
    let out = compile_tab_lazy(&src, &[filt("Balance", FilterOp::Gt, "100")], &tmpdir())
        .unwrap()
        .collect()
        .unwrap();
    assert_eq!(f64_col(&out, "Balance"), vec![Some(150.0), Some(250.0)]);
    let out = compile_tab_lazy(&src, &[filt("Matdat", FilterOp::Lt, "2026-11-01")], &tmpdir())
        .unwrap()
        .collect()
        .unwrap();
    assert_eq!(date_days_col(&out, "Matdat"), vec![Some(days(2026, 10, 15))]);

    // Text columns compare as text even when the value looks numeric: "9" > "50" > "100" > "10"
    let codes = df!("Code" => ["9", "10", "100"]).unwrap();
    let pc = write_parquet("codes.parquet", &codes);
    let src_c = TabSource::SingleFile { path: pc, is_parquet: true, config: IngestionConfig::default() };
    let out = compile_tab_lazy(&src_c, &[filt("Code", FilterOp::Gt, "50")], &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(str_col(&out, "Code"), vec![Some("9".to_string())], "text Gt");
    let out = compile_tab_lazy(&src_c, &[filt("Code", FilterOp::Lt, "50")], &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(str_col(&out, "Code"), vec![Some("10".to_string()), Some("100".to_string())], "text Lt");

    // Bad values fail loudly
    let err = compile_tab_lazy(&src, &[filt("Balance", FilterOp::Gt, "abc")], &tmpdir()).err().expect("non-numeric");
    assert!(format!("{err:#}").contains("'abc' is not a number for numeric column 'Balance'"), "got {err:#}");
    let err = compile_tab_lazy(&src, &[filt("Matdat", FilterOp::Lt, "01/11/2026")], &tmpdir()).err().expect("bad date");
    assert!(format!("{err:#}").contains("Matdat"), "got {err:#}");
}

#[test]
fn t15_python_codegen_drops_banking_steps() {
    let steps = vec![
        TransformStep::LoanClassificationProvision {
            bal_col: "Balance".into(),
            cl_status_col: "CL".into(),
            segment_col: None,
            suspense_col: None,
            security_col: None,
            cutoff_date: "2026-10-31".into(),
            collateral_type_col: None,
        },
        TransformStep::RateShockSim {
            bal_col: "Balance".into(),
            rate_col: "Rate".into(),
            mat_col: "Matdat".into(),
            cutoff_date: "2026-10-31".into(),
            shock_bps: 100.0,
            side_col: Some("Side".into()),
        },
    ];
    let col_cfg = |name: &str, dtype, pii_mask| ColumnSchemaConfig {
        name: name.into(),
        dtype,
        pii_mask,
        sample_values: vec![],
    };
    let cfg = IngestionConfig {
        columns: vec![
            col_cfg("Balance", ColDataType::Float64, PiiMaskMode::AccountNumber),
            col_cfg("Matdat", ColDataType::DateDMY, PiiMaskMode::None),
            col_cfg("Opndat", ColDataType::DateMDY, PiiMaskMode::None),
            col_cfg("Valdat", ColDataType::DateISO, PiiMaskMode::None),
        ],
        ..Default::default()
    };
    let py = micro_parquet::macros::generate_python_polars_code(
        std::path::Path::new("x.csv"),
        false,
        &cfg,
        &steps,
        None,
    );
    assert!(!py.contains("Specialized Banking Step Executed"), "placeholder still emitted:\n{py}");
    assert!(py.contains("raise NotImplementedError(\"Step 1 '+BB CL Provision & BFP' has no Python translation"), "{py}");
    assert!(py.contains("raise NotImplementedError(\"Step 2 '+ΔNII Shock (+100 bps)' has no Python translation"), "{py}");
    assert!(py.contains("# WARNING: PII masking not reproduced"), "{py}");
    assert!(
        py.contains("pl.col(\"Balance\").str.replace_all(\",\", \"\", literal=True).str.replace(r\"^\\((.*)\\)$\", \"-${1}\").cast(pl.Float64"),
        "comma and bracket cleaning must come before the Float64 cast:\n{py}"
    );
    assert!(py.contains("pl.col(\"Matdat\").str.to_date(\"%d/%m/%Y\""), "{py}");
    assert!(py.contains("pl.col(\"Opndat\").str.to_date(\"%m/%d/%Y\""), "{py}");
    assert!(py.contains("pl.col(\"Valdat\").str.to_date(\"%Y-%m-%d\""), "{py}");
    // Casts are not strict (bad values become null, as in the app) and the null counts are printed
    assert!(py.contains("\").cast(pl.Float64, strict=False)"), "{py}");
    assert!(py.contains("str.to_date(\"%d/%m/%Y\", strict=False)"), "{py}");
    assert!(!py.contains("strict=True"), "{py}");
    assert!(py.contains("_cast_cols = [\"Balance\", \"Matdat\", \"Opndat\", \"Valdat\"]"), "{py}");
    assert!(py.contains("could not be converted and became null"), "{py}");
    let missing = micro_parquet::macros::python_untranslated_steps(&steps);
    assert_eq!(missing, vec!["Step 1 '+BB CL Provision & BFP'".to_string(), "Step 2 '+ΔNII Shock (+100 bps)'".to_string()]);
}

#[test]
#[ignore = "Phase 2: stacking files whose columns differ"]
fn t16_stack_schema_drift() {
    // Target: October's extra Branch column does not break the stack; September rows get null.
    let a = write_csv("sep.csv", "Actnum,Balance\n001,100\n");
    let b = write_csv("oct.csv", "Actnum,Balance,Branch\n001,200,DHK\n");
    let files = vec![
        (a.clone(), false, auto_detect_smart_schema(&a).unwrap()),
        (b.clone(), false, auto_detect_smart_schema(&b).unwrap()),
    ];
    let src = TabSource::VerticalStack { files, include_source_col: true };
    let df = compile_tab_lazy(&src, &[], &tmpdir())
        .and_then(|lf| Ok(lf.collect()?))
        .expect("stack with an extra column must succeed");
    assert_eq!(df.height(), 2);
    assert_eq!(f64_col(&df, "Balance"), vec![Some(100.0), Some(200.0)]);
    assert_eq!(str_col(&df, "Branch"), vec![None, Some("DHK".to_string())]);
}

#[test]
#[ignore = "Phase 2: join validation (duplicate keys on the right side)"]
fn t17_join_duplicates_inflate() {
    let left = df!("Actnum" => ["A1"], "Balance" => [1_000_000.0]).unwrap();
    let right = df!("Actnum" => ["A1", "A1"], "Sector" => ["RMG", "RMG (dup row)"]).unwrap();
    let pl = write_parquet("l.parquet", &left);
    let pr = write_parquet("r.parquet", &right);
    let src = TabSource::JoinedTabs {
        left_source: Box::new(TabSource::SingleFile { path: pl, is_parquet: true, config: IngestionConfig::default() }),
        left_steps: vec![],
        right_source: Box::new(TabSource::SingleFile { path: pr, is_parquet: true, config: IngestionConfig::default() }),
        right_steps: vec![],
        join_kind: SqlJoinKind::Left,
        join_keys: vec!["Actnum".into()],
        right_cols_to_keep: vec![],
    };
    // Target: a right side with a repeated key fails loudly, naming the key, instead of
    // doubling the left row (BDT 1,000,000 would become 2,000,000).
    let err = compile_tab_lazy(&src, &[], &tmpdir())
        .and_then(|lf| compute_control_totals(lf, Some("Balance")))
        .err()
        .expect("duplicate right-side keys must be an error");
    let msg = format!("{err:#}");
    assert!(msg.contains("A1") && msg.to_lowercase().contains("duplicate"), "{msg}");
}

#[test]
fn t18_segment_and_status_edge_cases() {
    // T7: codes are normalised (case, spaces); segments match exactly, unmapped -> OTHER (flagged).
    let df = df!(
        "Balance" => [1_000_000.0; 6],
        "CL" => ["SS", "ss ", "Sub  Standard", "DF", "Sub-Standard", "UC"],
        "Seg" => ["NON-CMSME", "CORPORATE", "CORPORATE", "AGRI TERM LOAN", " small ", "CMSME"]
    )
    .unwrap();
    let cfg = ProvisionConfig::defaults();
    let out = apply_loan_classification_provision_with(df.lazy(), "Balance", "CL", Some("Seg"), None, None, None, "2026-10-31", &cfg)
        .unwrap()
        .collect()
        .unwrap();
    assert_eq!(f64_col(&out, "Statutory_Prov_Rate_Pct"), vec![Some(20.0), Some(20.0), Some(20.0), Some(50.0), Some(20.0), Some(1.0)]);
    let cat = str_col(&out, "Reg_Category");
    assert_eq!(cat[0].as_deref(), Some("OTHER"));
    assert_eq!(cat[3].as_deref(), Some("OTHER"));
    assert_eq!(cat[4].as_deref(), Some("CMSME"));
    // A bare "CMSME" segment code may include medium loans, so it is not in the map: OTHER, flagged.
    assert_eq!(cat[5].as_deref(), Some("OTHER"));
    let unmapped: Vec<Option<bool>> = out.column("Segment_Unmapped").unwrap().bool().unwrap().into_iter().collect();
    assert_eq!(unmapped[5], Some(true));
    println!("PROV-EDGE:\n{}", out.select(["CL", "Seg", "CL_Grade", "Reg_Category", "Segment_Unmapped", "Statutory_Prov_Rate_Pct"]).unwrap());
}

#[test]
fn t19_hygiene_case_sensitivity() {
    // Status-code mapping is not a part-1 task; the exact-code row is asserted.
    let past = days(2025, 1, 1);
    let open = days(2024, 1, 1);
    let df = df!(
        "Opndat" => [open, open],
        "Matdat" => [past, past],
        "Clsdat" => [None::<i32>, None],
        "Acstat" => ["ACT", "Active"],
        "Balance" => [500.0, 500.0],
        "Rate" => [9.0, 9.0]
    )
    .unwrap()
    .lazy()
    .with_columns([
        col("Opndat").cast(DataType::Date),
        col("Matdat").cast(DataType::Date),
        col("Clsdat").cast(DataType::Date),
    ]);
    let out = apply_banking_hygiene_audit(df, "Opndat", "Matdat", "Clsdat", "Acstat", "Balance", "Rate", "2026-10-31", false)
        .unwrap()
        .collect()
        .unwrap();
    let flags = str_col(&out, "Audit_Exception_Flag");
    assert_eq!(flags[0].as_deref(), Some("1. GHOST_MATURED_ACTIVE"));
    println!("HYGIENE:\n{}", out.select(["Acstat", "Audit_Exception_Flag"]).unwrap());
}

#[test]
fn t20_v111_macro_json_still_loads() {
    // A macro saved by v1.1.1 (before any Phase 1 fields existed)
    let json = r#"{
        "name": "ALCO monthly",
        "description": "v1.1.1 recipe",
        "created_at": "2026-10-31 10:00:00",
        "enforce_balance_control": true,
        "control_column": "Balance",
        "optional_ingestion_schema": null,
        "steps": [
            {"RateShockSim": {"bal_col": "Balance", "rate_col": "Rate", "mat_col": "Matdat",
                              "cutoff_date": "2026-10-31", "shock_bps": 200.0}},
            {"DurationAnalysis": {"bal_col": "Balance", "rate_col": "Rate", "mat_col": "Matdat",
                                  "cutoff_date": "2026-10-31", "compounding_freq": 1,
                                  "shock_bps": 200.0, "default_sight_years": 0.0833}},
            {"LoanClassificationProvision": {"bal_col": "Balance", "cl_status_col": "CL",
                                             "segment_col": null, "suspense_col": "Susp",
                                             "security_col": null}}
        ]
    }"#;
    let recipe: MacroRecipe = serde_json::from_str(json).expect("v1.1.1 macro JSON must load");
    assert_eq!(recipe.steps.len(), 3);
    assert!(matches!(recipe.steps[0], TransformStep::RateShockSim { side_col: None, .. }));
    assert!(matches!(recipe.steps[1], TransformStep::DurationAnalysis { side_col: None, .. }));
    match &recipe.steps[2] {
        TransformStep::LoanClassificationProvision { cutoff_date, collateral_type_col, .. } => {
            assert!(cutoff_date.is_empty() && collateral_type_col.is_none());
        }
        other => panic!("unexpected step {other:?}"),
    }
}

#[test]
fn t21_column_tokens_and_ids() {
    assert_eq!(column_name_tokens("BranchCode"), vec!["branch", "code"]);
    assert_eq!(column_name_tokens("CIFNumber"), vec!["cif", "number"]);
    assert_eq!(column_name_tokens("Rate_Delta-Bps.x"), vec!["rate", "delta", "bps", "x"]);
    for id in ["Actnum", "Customer_ID", "BranchCode", "GL_Code", "glcode", "CIF", "Mobile No"] {
        assert!(TabState::is_likely_id_column(id), "{id} should be an ID");
    }
    for not_id in ["Account_Balance", "Paid_Amount", "Acct_Int_Rate", "Branch_Total", "Matdat", "Valid_Flag", "Dividend"] {
        assert!(!TabState::is_likely_id_column(not_id), "{not_id} should not be an ID");
    }
}

#[test]
fn t08b_wavg_python_is_null_safe() {
    let py = micro_parquet::macros::generate_python_polars_code(
        std::path::Path::new("x.parquet"),
        true,
        &IngestionConfig::default(),
        &[wavg_step(Some("Balance"))],
        None,
    );
    assert!(py.contains("pl.col(\"Rate\").cast(pl.Float64).is_not_null() & pl.col(\"Balance\").cast(pl.Float64).is_not_null()"), "{py}");
    assert!(py.contains(".then(None)"), "{py}");
    let py = micro_parquet::macros::generate_python_polars_code(
        std::path::Path::new("x.parquet"),
        true,
        &IngestionConfig::default(),
        &[wavg_step(None)],
        None,
    );
    assert!(py.contains("raise ValueError(\"Weighted average needs a weight column"), "{py}");
}

#[test]
fn t09b_duration_python_is_signed() {
    // T4: the Python export of the ΔEVE step applies the same side sign (or flags the assumption).
    let step = |side: Option<&str>| TransformStep::DurationAnalysis {
        bal_col: "Balance".into(),
        rate_col: "Rate".into(),
        mat_col: "Matdat".into(),
        cutoff_date: "2026-10-31".into(),
        compounding_freq: 1,
        shock_bps: 200.0,
        default_sight_years: 0.0833,
        side_col: side.map(str::to_string),
    };
    let cfg = IngestionConfig { columns: vec![], ..Default::default() };
    let gen = |s| micro_parquet::macros::generate_python_polars_code(std::path::Path::new("in.csv"), false, &cfg, &[s], None);
    let signed = gen(step(Some("Side")));
    assert!(signed.contains("_sign = pl.when"), "{signed}");
    assert!(signed.contains("* _sign).round(2).alias(\"Delta_EVE_Impact_BDT\")"), "{signed}");
    assert!(signed.contains("raise ValueError"), "{signed}");
    let unsigned = gen(step(None));
    assert!(unsigned.contains("alias(\"Direction_Assumed\")"), "{unsigned}");
    println!("{signed}");
}

/// Writes the three rule files in the pre-versioning (defaults version 1) layout.
fn write_v1_rule_files(dir: &std::path::Path, extra_cl: Option<(&str, &str)>) {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    let flat = |pairs: Vec<(&str, &str)>| {
        let body: Vec<String> = pairs.iter().map(|(k, v)| format!("  {:?}: {:?}", k, v)).collect();
        format!("{{\n{}\n}}\n", body.join(",\n"))
    };
    let mut cl = default_cl_code_map(1);
    cl.extend(extra_cl);
    std::fs::write(dir.join("cl_code_map.json"), flat(cl)).unwrap();
    std::fs::write(dir.join("segment_map.json"), flat(default_segment_map(1))).unwrap();
    let rules = serde_json::to_string_pretty(&ProvisionConfig::default_rules_for(1)).unwrap();
    std::fs::write(dir.join("provision_rules.json"), rules).unwrap();
}

#[test]
fn t07c_rule_file_versioning() {
    // Untouched version-1 defaults are replaced by the current defaults, silently.
    let dir = tmpdir().join("t07c_untouched");
    write_v1_rule_files(&dir, None);
    let cfg = ProvisionConfig::load_or_create(&dir).unwrap();
    assert!(cfg.warnings.is_empty(), "{:?}", cfg.warnings);
    for f in ["cl_code_map.json", "segment_map.json", "provision_rules.json"] {
        let text = std::fs::read_to_string(dir.join(f)).unwrap();
        assert!(text.contains(&format!("\"defaults_version\": {}", RULE_FILES_VERSION)), "{f}: {text}");
    }
    assert!(!cfg.cl_code_map.iter().any(|(k, _)| k == "BLW"));
    assert!(cfg.segment_map.contains(&("MEDIUM".to_string(), "OTHER".to_string())));
    assert_eq!(cfg.rules, ProvisionConfig::default_rules());
    // A second load of the upgraded files is quiet and unchanged
    let again = ProvisionConfig::load_or_create(&dir).unwrap();
    assert!(again.warnings.is_empty());
    assert_eq!(again.rules, cfg.rules);

    // An edited version-1 file is kept as is and named in a warning; untouched ones still upgrade.
    let dir = tmpdir().join("t07c_edited");
    write_v1_rule_files(&dir, Some(("XX", "SS")));
    let before = std::fs::read_to_string(dir.join("cl_code_map.json")).unwrap();
    let cfg = ProvisionConfig::load_or_create(&dir).unwrap();
    assert_eq!(cfg.warnings.len(), 1, "{:?}", cfg.warnings);
    assert!(cfg.warnings[0].contains("cl_code_map.json"), "{}", cfg.warnings[0]);
    assert_eq!(std::fs::read_to_string(dir.join("cl_code_map.json")).unwrap(), before, "edited file must not be touched");
    assert!(cfg.cl_code_map.contains(&("XX".to_string(), "SS".to_string())));
    assert!(cfg.cl_code_map.iter().any(|(k, _)| k == "BLW"), "the kept file still has its own codes");
    assert!(std::fs::read_to_string(dir.join("segment_map.json")).unwrap().contains("\"defaults_version\": 2"));

    // Edited files at the current version load without a warning
    let mut text = std::fs::read_to_string(dir.join("cl_code_map.json")).unwrap();
    text = text.replacen('{', "{\n  \"defaults_version\": 2,", 1);
    std::fs::write(dir.join("cl_code_map.json"), text).unwrap();
    let cfg = ProvisionConfig::load_or_create(&dir).unwrap();
    assert!(cfg.warnings.is_empty(), "{:?}", cfg.warnings);
}
