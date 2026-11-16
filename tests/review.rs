// Review harness: exercises MicroParquet's calculation code against hand-checked cases.
// Expected values come from docs/phase1-spec.md. Tests for tasks outside Phase 1 part 1
// (T4, T7, Phase 2) keep their evidence printout and assert only what is already settled.
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
fn t04_numeric_parsing_silent_nulls() {
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
    let df = lf.clone().collect().unwrap();
    let vals = f64_col(&df, "Balance");
    assert_eq!(vals.len(), 7);
    assert_close(vals[0].unwrap(), 1234.5, "comma thousands");
    assert_close(vals[1].unwrap(), -500.0, "bracket negative");
    // Dr/Cr suffixes and raw-vs-parsed control sums are Phase 2; evidence only.
    let (rows, sum) = compute_control_totals(lf, Some("Balance")).unwrap();
    assert_eq!(rows, 7);
    println!("NUMS:\n{}\nNUMS control rows={} sum={:?}", df, rows, sum);
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
    let e1 = apply_rate_shock_sim(df.clone().lazy(), "Balance", "Rate", "Matdat", "2026-10-31", 100.0).err();
    let e2 = apply_ftp_curve_map(df.clone().lazy(), "Opndat", "Matdat", "Balance", "Rate", &default_bd_ftp_curve(), true).err();
    let e3 = apply_banking_hygiene_audit(df.clone().lazy(), "Opndat", "Matdat", "Clsdat", "Acstat", "Balance", "Rate", "2026-10-31", false).err();
    let e4 = apply_duration_analysis(df.lazy(), "Balance", "Rate", "Matdat", "2026-10-31", 1, 200.0, 0.0833).err();
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
    // Rates are reworked in Phase 1 T7 (not in part 1); smoke test only.
    let df = df!(
        "Balance" => [1_000_000.0; 9],
        "CL" => ["UC", "SMA", "SS", "DF", "B/L", "BL", "STD-1", "SS", "BL"],
        "Seg" => ["CORPORATE", "CORPORATE", "CORPORATE", "CORPORATE", "CORPORATE", "NON-CMSME", "CMSME", "CMSME", "CORPORATE"],
        "Susp" => [0.0, 0.0, 100_000.0, 100_000.0, 100_000.0, 100_000.0, 0.0, 0.0, 0.0],
        "Sec" => [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 950_000.0]
    )
    .unwrap();
    let out = apply_loan_classification_provision(df.lazy(), "Balance", "CL", Some("Seg"), Some("Susp"), Some("Sec"))
        .unwrap()
        .collect()
        .unwrap();
    assert_eq!(out.height(), 9);
    println!("PROV:\n{}", out.drop("Balance").unwrap());
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
    // Direction-aware ΔEVE is Phase 1 T4 (not in part 1). Duration maths only.
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
    let out = apply_duration_analysis(df, "Balance", "Rate", "Matdat", cutoff, 1, 200.0, 0.0833)
        .unwrap()
        .collect()
        .unwrap();
    let mac = f64_col(&out, "Macaulay_Duration_Yrs");
    let md = f64_col(&out, "Modified_Duration");
    assert_close(mac[0].unwrap(), 4.1699, "Macaulay duration");
    assert_close(md[0].unwrap(), 3.7908, "Modified duration");
    println!("DURATION:\n{}", out);
}

#[test]
fn t10_nii_shock_signs() {
    // Asset/liability direction is Phase 1 T4 (not in part 1); smoke test only.
    let df = df!(
        "Side" => ["Loan 3m", "Term deposit 3m", "Savings (no maturity)"],
        "Balance" => [1_000_000.0, 1_000_000.0, 1_000_000.0],
        "Rate" => [12.0, 9.0, 4.0],
        "Matdat" => [Some(days(2027, 1, 29)), Some(days(2027, 1, 29)), None]
    )
    .unwrap()
    .lazy()
    .with_column(col("Matdat").cast(DataType::Date));
    let out = apply_rate_shock_sim(df, "Balance", "Rate", "Matdat", "2026-10-31", 100.0)
        .unwrap()
        .collect()
        .unwrap();
    assert_eq!(out.height(), 3);
    println!("NII SHOCK +100bp:\n{}", out);
}

#[test]
fn t11_ftp_step_vs_interpolation() {
    // Step vs interpolation is not a part-1 task; only the tenor is asserted.
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
    println!("FTP (curve 91d=9.50, 182d=10.10, 30d=8.75):\n{}", out);
}

#[test]
fn t12_reconcile() {
    // Rate WAVG in reconcile is not a part-1 task; balances and status are asserted.
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
    println!("RECONCILE (A1 prev WAVG rate = 9.5%):\n{}", out);
}

#[test]
#[ignore = "evidence only (PII hash width); no expected value in Phase 1 part 1"]
fn t13_pii_hash_collisions() {
    use std::collections::HashSet;
    let n = 1_000_000u32;
    let mut seen = HashSet::with_capacity(n as usize);
    let mut coll = 0u32;
    for i in 0..n {
        let mut h = blake3::Hasher::new();
        h.update(b"BANK_ASIA_ALM_2026");
        h.update(format!("CUSTOMER NAME {}", i).as_bytes());
        let hex = h.finalize().to_hex().to_string().to_uppercase();
        if !seen.insert(hex[..8].to_string()) {
            coll += 1;
        }
    }
    println!("PII: {} names -> {} CLIENT_xxxxxxxx collisions", n, coll);
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
        },
        TransformStep::RateShockSim {
            bal_col: "Balance".into(),
            rate_col: "Rate".into(),
            mat_col: "Matdat".into(),
            cutoff_date: "2026-10-31".into(),
            shock_bps: 100.0,
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
    assert!(py.contains("pl.col(\"Balance\")") && py.contains(".cast(pl.Float64)"), "{py}");
    assert!(py.contains("pl.col(\"Matdat\").str.to_date(\"%d/%m/%Y\""), "{py}");
    assert!(py.contains("pl.col(\"Opndat\").str.to_date(\"%m/%d/%Y\""), "{py}");
    assert!(py.contains("pl.col(\"Valdat\").str.to_date(\"%Y-%m-%d\""), "{py}");
    let missing = micro_parquet::macros::python_untranslated_steps(&steps);
    assert_eq!(missing, vec!["Step 1 '+BB CL Provision & BFP'".to_string(), "Step 2 '+ΔNII Shock (+100 bps)'".to_string()]);
}

#[test]
#[ignore = "evidence only: stack schema drift is not a Phase 1 part 1 task"]
fn t16_stack_schema_drift() {
    let a = write_csv("sep.csv", "Actnum,Balance\n001,100\n");
    let b = write_csv("oct.csv", "Actnum,Balance,Branch\n001,200,DHK\n");
    let files = vec![
        (a.clone(), false, auto_detect_smart_schema(&a).unwrap()),
        (b.clone(), false, auto_detect_smart_schema(&b).unwrap()),
    ];
    let src = TabSource::VerticalStack { files, include_source_col: true };
    match compile_tab_lazy(&src, &[], &tmpdir()).and_then(|lf| Ok(lf.collect()?)) {
        Ok(df) => println!("STACK ok:\n{}", df),
        Err(e) => println!("STACK error when Oct has one extra column: {e}"),
    }
}

#[test]
#[ignore = "Phase 2: join validation"]
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
    let lf = compile_tab_lazy(&src, &[], &tmpdir()).unwrap();
    let (rows, sum) = compute_control_totals(lf, Some("Balance")).unwrap();
    println!("JOIN: rows={} Balance sum={:?} (source sum 1,000,000)", rows, sum);
    assert_close(sum.unwrap(), 1_000_000.0, "join must not inflate the control sum");
}

#[test]
fn t18_segment_and_status_edge_cases() {
    // Status/segment mapping is Phase 1 T7 (not in part 1); smoke test only.
    let df = df!(
        "Balance" => [1_000_000.0; 4],
        "CL" => ["SS", "ss ", "Sub-Standard", "DF"],
        "Seg" => ["NON-CMSME", "CORPORATE", "CORPORATE", "AGRI TERM LOAN"]
    )
    .unwrap();
    let out = apply_loan_classification_provision(df.lazy(), "Balance", "CL", Some("Seg"), None, None)
        .unwrap()
        .collect()
        .unwrap();
    assert_eq!(out.height(), 4);
    println!("PROV-EDGE:\n{}", out.select(["CL", "Seg", "NPL_Category", "Statutory_Prov_Rate_Pct"]).unwrap());
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
    assert!(matches!(recipe.steps[0], TransformStep::RateShockSim { .. }));
    assert!(matches!(recipe.steps[1], TransformStep::DurationAnalysis { .. }));
    assert!(matches!(recipe.steps[2], TransformStep::LoanClassificationProvision { .. }));
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
